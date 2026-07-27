//! Deterministic lockfile — `ade.lock.json`. Port of `src/lockfile.ts`.
//!
//! Content-addressed record of everything ADE generated plus environment
//! facts (tool versions). No timestamps: identical inputs → byte-identical
//! lockfile. Environment differences on another machine are DRIFT verdicts
//! (informational), never hard failures; only content tampering fails verify.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use crate::audit::AuditCheckpoint;
use crate::fsutil::{read_if_exists, sha256_hex, stable_stringify};
use crate::types::{Ctx, Finding};
use crate::version::{ADE_SCHEMA_VERSION, ADE_VERSION};

pub const LOCKFILE_NAME: &str = "ade.lock.json";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LockEnvironment {
    pub os: String,
    pub arch: String,
    /// Tool name → version string, or None (JSON null) when the tool is absent.
    pub tools: BTreeMap<String, Option<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Lockfile {
    pub schema_version: u64,
    pub ade_version: String,
    /// Repo-relative path (POSIX separators) → sha256 of file content.
    pub files: BTreeMap<String, String>,
    /// Append-only audit-chain commitment. The log grows after apply (harness hooks
    /// append), so it cannot be content-hashed — instead we pin its length and head
    /// hash at apply time. Verify requires the head hash to still be PRESENT in the
    /// chain and the chain to be at least as long, which is what makes truncation,
    /// tail-dropping, and wholesale re-forging from genesis detectable.
    pub audit: AuditCheckpoint,
    pub environment: LockEnvironment,
    pub harnesses: Vec<String>,
}

pub fn generate_lockfile(
    ctx: &Ctx,
    generated_paths: &[String],
    audit: AuditCheckpoint,
) -> Lockfile {
    let mut sorted_paths: Vec<&String> = generated_paths.iter().collect();
    sorted_paths.sort();
    let mut files: BTreeMap<String, String> = BTreeMap::new();
    for rel_path in sorted_paths {
        if let Some(content) = read_if_exists(&ctx.target_dir.join(rel_path)) {
            files.insert(rel_path.clone(), sha256_hex(&content));
        }
    }
    let mut tools: BTreeMap<String, Option<String>> = BTreeMap::new();
    for info in ctx.tools.values() {
        let version = if info.present {
            Some(
                info.version
                    .clone()
                    .unwrap_or_else(|| "present".to_string()),
            )
        } else {
            None
        };
        tools.insert(info.name.clone(), version);
    }
    let mut harnesses = ctx.config.harnesses.clone();
    harnesses.sort();
    Lockfile {
        schema_version: ADE_SCHEMA_VERSION,
        ade_version: ADE_VERSION.to_string(),
        files,
        audit,
        environment: LockEnvironment {
            os: ctx.os.clone(),
            arch: ctx.arch.clone(),
            tools,
        },
        harnesses,
    }
}

/// Every file currently under `.ade/`, excluding the append-only audit log.
pub fn scan_ade_tree(target_dir: &Path) -> Vec<String> {
    let mut paths: Vec<String> = Vec::new();
    match walk_ade(&target_dir.join(".ade"), ".ade", &mut paths) {
        Ok(()) => {
            paths.sort();
            paths
        }
        // Oracle parity: any scan failure (absent `.ade/`, unreadable subtree)
        // yields an empty listing, exactly like the TS `catch { return [] }`.
        Err(_) => Vec::new(),
    }
}

fn walk_ade(dir: &Path, rel: &str, out: &mut Vec<String>) -> std::io::Result<()> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().replace('\\', "/");
        let child_rel = format!("{rel}/{name}");
        if entry.file_type()?.is_dir() {
            walk_ade(&entry.path(), &child_rel, out)?;
        } else if !child_rel.starts_with(".ade/audit/") {
            out.push(child_rel);
        }
    }
    Ok(())
}

pub fn serialize_lockfile(lock: &Lockfile) -> String {
    // `to_value` cannot fail for this plain-data struct; the fallback exists
    // only to keep the production path panic-free.
    let value = serde_json::to_value(lock).unwrap_or(serde_json::Value::Null);
    stable_stringify(&value)
}

pub fn load_lockfile(target_dir: &Path) -> Option<Lockfile> {
    let text = read_if_exists(&target_dir.join(LOCKFILE_NAME))?;
    serde_json::from_str(&text).ok()
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LockVerifyResult {
    pub ok: bool,
    pub findings: Vec<Finding>,
}

/// Verify on-disk state against the lockfile.
/// - missing/modified generated file → error (fails verify, names the file)
/// - tool version drift vs recorded environment → info (never fails)
pub fn verify_against_lockfile(ctx: &Ctx, lock: &Lockfile) -> LockVerifyResult {
    let mut findings: Vec<Finding> = Vec::new();
    let mut ok = true;
    for (rel_path, expected_hash) in &lock.files {
        let content = read_if_exists(&ctx.target_dir.join(rel_path));
        let content = match content {
            Some(content) => content,
            None => {
                ok = false;
                findings.push(Finding::error_with(
                    format!("generated file missing: {rel_path}"),
                    "run `ade apply` to regenerate, or `ade lock` to accept the removal",
                ));
                continue;
            }
        };
        if sha256_hex(&content) != *expected_hash {
            ok = false;
            findings.push(Finding::error_with(
                format!("generated file modified since lock: {rel_path}"),
                "run `ade apply` to regenerate, or `ade lock` to accept the change",
            ));
        }
    }

    // Unknown files inside the ADE-owned tree are a planted-artifact vector: a rule
    // file dropped into .ade/guardrails/ is BINDING on the harness but was never
    // written by ade. Anything under .ade/ that the lockfile doesn't know is an error.
    for rel_path in scan_ade_tree(&ctx.target_dir) {
        if !lock.files.contains_key(&rel_path) {
            ok = false;
            findings.push(Finding::error_with(
                format!("unknown file in the ADE-owned tree (not written by ade): {rel_path}"),
                "remove it, or run `ade lock` to adopt it deliberately",
            ));
        }
    }
    for (tool, locked_version) in &lock.environment.tools {
        let current = ctx.tools.get(tool);
        let current_version: Option<String> = match current {
            Some(info) if info.present => Some(
                info.version
                    .clone()
                    .unwrap_or_else(|| "present".to_string()),
            ),
            _ => None,
        };
        if current_version != *locked_version {
            let locked_desc = locked_version
                .clone()
                .unwrap_or_else(|| "absent".to_string());
            let current_desc = current_version.unwrap_or_else(|| "absent".to_string());
            findings.push(Finding::info(format!(
                "environment drift: {tool} was {locked_desc} at lock time, now {current_desc}"
            )));
        }
    }
    if ok && findings.is_empty() {
        findings.push(Finding::ok("lockfile verification passed"));
    }
    LockVerifyResult { ok, findings }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::{make_temp_dir, make_test_ctx, TestCtxOptions};
    use crate::types::FindingLevel;
    use crate::version::AUDIT_GENESIS;
    use std::path::PathBuf;

    fn checkpoint() -> AuditCheckpoint {
        AuditCheckpoint {
            length: 0,
            head_hash: AUDIT_GENESIS.to_string(),
        }
    }

    fn paths() -> Vec<String> {
        vec![
            ".ade/policy/a.json".to_string(),
            ".ade/instructions.md".to_string(),
        ]
    }

    fn setup_dir() -> PathBuf {
        let dir = make_temp_dir("lockfile");
        std::fs::create_dir_all(dir.join(".ade").join("policy")).unwrap();
        std::fs::write(
            dir.join(".ade").join("policy").join("a.json"),
            "{\"a\":1}\n",
        )
        .unwrap();
        std::fs::write(dir.join(".ade").join("instructions.md"), "# canonical\n").unwrap();
        dir
    }

    /// Port of the TS test's timestamp probe: /20\d{2}-\d{2}-\d{2}T/
    fn contains_timestamp(text: &str) -> bool {
        text.as_bytes().windows(11).any(|window| {
            window[0] == b'2'
                && window[1] == b'0'
                && window[2].is_ascii_digit()
                && window[3].is_ascii_digit()
                && window[4] == b'-'
                && window[5].is_ascii_digit()
                && window[6].is_ascii_digit()
                && window[7] == b'-'
                && window[8].is_ascii_digit()
                && window[9].is_ascii_digit()
                && window[10] == b'T'
        })
    }

    #[test]
    fn isc_35_36_40_records_file_hashes_environment_tools_and_ade_version() {
        let dir = setup_dir();
        let ctx = make_test_ctx(
            &dir,
            TestCtxOptions {
                present_tools: &[("rtk", "rtk 0.9.0")],
                ..Default::default()
            },
        );
        let lock = generate_lockfile(&ctx, &paths(), checkpoint());
        let mut expected = paths();
        expected.sort();
        let keys: Vec<String> = lock.files.keys().cloned().collect();
        assert_eq!(keys, expected);
        let hash = &lock.files[".ade/policy/a.json"];
        assert_eq!(hash.len(), 64);
        assert!(hash.chars().all(|c| matches!(c, '0'..='9' | 'a'..='f')));
        assert_eq!(lock.environment.tools["rtk"], Some("rtk 0.9.0".to_string()));
        assert_eq!(lock.environment.tools["nono"], None);
        let parts: Vec<&str> = lock.ade_version.split('.').collect();
        assert_eq!(parts.len(), 3);
        assert!(parts
            .iter()
            .all(|part| !part.is_empty() && part.chars().all(|c| c.is_ascii_digit())));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn isc_37_serialization_is_byte_deterministic_and_timestamp_free() {
        let dir = setup_dir();
        let ctx = make_test_ctx(&dir, TestCtxOptions::default());
        let first = serialize_lockfile(&generate_lockfile(&ctx, &paths(), checkpoint()));
        let mut reversed = paths();
        reversed.reverse();
        let second_ctx = make_test_ctx(&dir, TestCtxOptions::default());
        let second = serialize_lockfile(&generate_lockfile(&second_ctx, &reversed, checkpoint()));
        assert_eq!(first, second);
        assert!(!contains_timestamp(&first));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn isc_38_modified_generated_file_fails_verify_naming_the_file() {
        let dir = setup_dir();
        let ctx = make_test_ctx(&dir, TestCtxOptions::default());
        let lock = generate_lockfile(&ctx, &paths(), checkpoint());
        std::fs::write(
            dir.join(".ade").join("policy").join("a.json"),
            "{\"a\":2}\n",
        )
        .unwrap();
        let result = verify_against_lockfile(&ctx, &lock);
        assert!(!result.ok);
        assert!(result
            .findings
            .iter()
            .any(|finding| finding.level == FindingLevel::Error
                && finding.message.contains(".ade/policy/a.json")));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn isc_39_deleted_generated_file_fails_verify_naming_the_file() {
        let dir = setup_dir();
        let ctx = make_test_ctx(&dir, TestCtxOptions::default());
        let lock = generate_lockfile(&ctx, &paths(), checkpoint());
        std::fs::remove_dir_all(dir.join(".ade").join("policy")).unwrap();
        let result = verify_against_lockfile(&ctx, &lock);
        assert!(!result.ok);
        assert!(result
            .findings
            .iter()
            .any(|finding| finding.message.contains("missing: .ade/policy/a.json")));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn isc_109_tool_version_drift_is_informational_never_a_failure() {
        let dir = setup_dir();
        let lock_ctx = make_test_ctx(
            &dir,
            TestCtxOptions {
                present_tools: &[("trufflehog", "3.0.0")],
                ..Default::default()
            },
        );
        let lock = generate_lockfile(&lock_ctx, &paths(), checkpoint());
        let now_ctx = make_test_ctx(
            &dir,
            TestCtxOptions {
                present_tools: &[("trufflehog", "3.9.9")],
                ..Default::default()
            },
        );
        let result = verify_against_lockfile(&now_ctx, &lock);
        assert!(result.ok);
        assert!(result.findings.iter().any(
            |finding| finding.level == FindingLevel::Info && finding.message.contains("drift")
        ));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn untampered_state_passes_with_an_ok_finding() {
        let dir = setup_dir();
        let ctx = make_test_ctx(&dir, TestCtxOptions::default());
        let lock = generate_lockfile(&ctx, &paths(), checkpoint());
        let result = verify_against_lockfile(&ctx, &lock);
        assert!(result.ok);
        assert!(result
            .findings
            .iter()
            .any(|finding| finding.level == FindingLevel::Ok));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn load_lockfile_absent_and_corrupt_both_return_none() {
        let dir = setup_dir();
        assert!(load_lockfile(&dir).is_none());
        std::fs::write(dir.join(LOCKFILE_NAME), "{broken").unwrap();
        assert!(load_lockfile(&dir).is_none());
        let ctx = make_test_ctx(&dir, TestCtxOptions::default());
        let serialized = serialize_lockfile(&generate_lockfile(&ctx, &paths(), checkpoint()));
        std::fs::write(dir.join(LOCKFILE_NAME), &serialized).unwrap();
        assert!(load_lockfile(&dir).is_some());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
