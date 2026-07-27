//! Translate the canonical instructions into every target harness's
//! instruction file, inside managed markers. User content outside the
//! markers is preserved byte-for-byte; ambiguous or hand-edited blocks
//! are refused, never clobbered.
//! Port of `src/translate.ts` (error strings byte-identical to the oracle).

use crate::fsutil::read_if_exists;
use crate::harness::adapters::{get_adapter, render_target};
use crate::managed::{
    extract_managed_block, render_managed_block, upsert_managed_block, UpsertResult,
};
use crate::types::{Ctx, Finding};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TranslateFileResult {
    pub path: String,
    pub harnesses: Vec<String>,
    pub ok: bool,
    pub changed: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// One unique instruction-file target (path + fresh-file prefix + the harness
/// ids that share it).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TranslationTarget {
    pub path: &'static str,
    pub fresh_file_prefix: &'static str,
    pub harnesses: Vec<String>,
}

/// Unique instruction-file targets for the configured harnesses (deduped by path).
pub fn translation_targets(harness_ids: &[String]) -> Vec<TranslationTarget> {
    let mut targets: Vec<TranslationTarget> = Vec::new();
    for id in harness_ids {
        let Some(adapter) = get_adapter(id) else {
            continue;
        };
        let rendered = render_target(adapter);
        match targets
            .iter_mut()
            .find(|target| target.path == rendered.path)
        {
            Some(existing) => existing.harnesses.push(id.clone()),
            None => targets.push(TranslationTarget {
                path: rendered.path,
                fresh_file_prefix: rendered.fresh_file_prefix,
                harnesses: vec![id.clone()],
            }),
        }
    }
    targets.sort_by(|a, b| a.path.cmp(b.path));
    targets
}

/// Guard: only ever write over a regular file (or nothing). FIFOs, sockets,
/// device nodes, and symlinks in config paths are frequently intentional
/// secret mounts (1Password/sops) — clobbering one destroys the mount.
fn is_safe_write_target(absolute: &Path) -> bool {
    match std::fs::symlink_metadata(absolute) {
        Ok(stats) => stats.file_type().is_file(),
        Err(_) => true, // absent → safe to create
    }
}

pub fn translate_all(ctx: &Ctx, canonical_body: &str) -> std::io::Result<Vec<TranslateFileResult>> {
    let mut results: Vec<TranslateFileResult> = Vec::new();
    for target in translation_targets(&ctx.config.harnesses) {
        let absolute = ctx.target_dir.join(target.path);
        if !is_safe_write_target(&absolute) {
            results.push(TranslateFileResult {
                path: target.path.to_string(),
                harnesses: target.harnesses,
                ok: false,
                changed: false,
                error: Some(
                    "target exists but is not a regular file (symlink/FIFO/socket) — refusing to write; it may be an intentional mount"
                        .to_string(),
                ),
            });
            continue;
        }
        let existing = read_if_exists(&absolute);
        let base = existing.unwrap_or_else(|| target.fresh_file_prefix.to_string());
        match upsert_managed_block(&base, canonical_body) {
            UpsertResult::Err { error } => {
                results.push(TranslateFileResult {
                    path: target.path.to_string(),
                    harnesses: target.harnesses,
                    ok: false,
                    changed: false,
                    error: Some(error),
                });
            }
            UpsertResult::Ok { content, changed } => {
                // Unchanged content still belongs to the generated set (lockfile coverage).
                ctx.artifacts.write(target.path, &content)?;
                results.push(TranslateFileResult {
                    path: target.path.to_string(),
                    harnesses: target.harnesses,
                    ok: true,
                    changed,
                    error: None,
                });
            }
        }
    }
    Ok(results)
}

/// Drift check: every target file's managed block matches the canonical rendering.
pub fn check_translation_drift(ctx: &Ctx, canonical_body: &str) -> Vec<Finding> {
    let mut findings: Vec<Finding> = Vec::new();
    let expected = render_managed_block(canonical_body);
    for target in translation_targets(&ctx.config.harnesses) {
        let Some(existing) = read_if_exists(&ctx.target_dir.join(target.path)) else {
            findings.push(Finding::error_with(
                format!("instruction file missing: {}", target.path),
                "run `ade translate`",
            ));
            continue;
        };
        let Some(block) = extract_managed_block(&existing) else {
            findings.push(Finding::error_with(
                format!("managed block missing or corrupt in {}", target.path),
                "run `ade translate` (or fix the markers manually)",
            ));
            continue;
        };
        if block != expected {
            findings.push(Finding::error_with(
                format!(
                    "instruction drift in {}: managed block does not match .ade/instructions.md",
                    target.path
                ),
                "run `ade translate` to regenerate",
            ));
        }
    }
    if findings.is_empty() {
        findings.push(Finding::ok("instruction files match canonical source"));
    }
    findings
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::instructions::compose_instructions;
    use crate::testutil::{make_temp_dir, make_test_ctx, test_config_with, TestCtxOptions};
    use crate::types::{FindingLevel, InstructionBlock};
    use std::fs;

    fn body() -> String {
        compose_instructions(&[InstructionBlock {
            id: "t",
            title: "Test Block",
            content: "Do the test thing.".to_string(),
        }])
    }

    fn ids(names: &[&str]) -> Vec<String> {
        names.iter().map(|name| name.to_string()).collect()
    }

    fn ctx_for(dir: &Path, harnesses: &[&str]) -> Ctx {
        make_test_ctx(
            dir,
            TestCtxOptions {
                config: Some(test_config_with(&[], harnesses)),
                ..Default::default()
            },
        )
    }

    #[test]
    fn isc_47_adjacent_targets_dedupe_agents_md_across_the_open_standard_harnesses() {
        let targets =
            translation_targets(&ids(&["codex", "opencode", "hermes", "pi", "antigravity"]));
        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].path, "AGENTS.md");
        let mut harnesses = targets[0].harnesses.clone();
        harnesses.sort();
        assert_eq!(
            harnesses,
            ids(&["antigravity", "codex", "hermes", "opencode", "pi"])
        );
    }

    #[test]
    fn cursor_targets_modern_cursor_rules_path_with_frontmatter_prefix() {
        let targets = translation_targets(&ids(&["cursor"]));
        assert_eq!(targets[0].path, ".cursor/rules/ade.mdc");
        assert!(targets[0].fresh_file_prefix.contains("alwaysApply: true"));
    }

    #[test]
    fn isc_54_translate_all_writes_managed_blocks_into_every_target() {
        let dir = make_temp_dir("translate-54");
        let ctx = ctx_for(&dir, &["claude-code", "codex", "cursor"]);
        let results = translate_all(&ctx, &body()).unwrap();
        assert_eq!(results.len(), 3);
        assert!(results.iter().all(|result| result.ok));
        let cursor_file =
            fs::read_to_string(dir.join(".cursor").join("rules").join("ade.mdc")).unwrap();
        assert!(cursor_file.starts_with("---\n"));
        assert!(cursor_file.contains("Do the test thing."));
        let claude = fs::read_to_string(dir.join("CLAUDE.md")).unwrap();
        assert!(claude.contains("<!-- ade:begin -->"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn isc_55_pre_existing_user_content_survives_translation_byte_for_byte() {
        let dir = make_temp_dir("translate-55");
        fs::write(dir.join("CLAUDE.md"), "# Mine\n\nkeep me.\n").unwrap();
        let ctx = ctx_for(&dir, &["claude-code"]);
        translate_all(&ctx, &body()).unwrap();
        let content = fs::read_to_string(dir.join("CLAUDE.md")).unwrap();
        assert!(content.starts_with("# Mine\n\nkeep me.\n"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn isc_56_repeat_translate_with_unchanged_canonical_body_is_byte_identical() {
        let dir = make_temp_dir("translate-56");
        let ctx = ctx_for(&dir, &["claude-code"]);
        translate_all(&ctx, &body()).unwrap();
        let first = fs::read_to_string(dir.join("CLAUDE.md")).unwrap();
        let again = translate_all(&ctx_for(&dir, &["claude-code"]), &body()).unwrap();
        assert!(!again[0].changed);
        assert_eq!(fs::read_to_string(dir.join("CLAUDE.md")).unwrap(), first);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn isc_55_1_55_2_corrupt_markers_and_hand_edited_blocks_are_refused_per_file() {
        let dir = make_temp_dir("translate-refuse");
        let ctx = ctx_for(&dir, &["claude-code", "codex"]);
        translate_all(&ctx, &body()).unwrap();
        let claude_path = dir.join("CLAUDE.md");
        let tampered = fs::read_to_string(&claude_path)
            .unwrap()
            .replace("Do the test thing.", "corrupted by user");
        fs::write(&claude_path, &tampered).unwrap();
        let results = translate_all(&ctx_for(&dir, &["claude-code", "codex"]), &body()).unwrap();
        let claude_result = results
            .iter()
            .find(|result| result.path == "CLAUDE.md")
            .unwrap();
        let agents_result = results
            .iter()
            .find(|result| result.path == "AGENTS.md")
            .unwrap();
        assert!(!claude_result.ok);
        assert!(claude_result
            .error
            .as_deref()
            .unwrap()
            .contains("hand-edited"));
        assert!(agents_result.ok);
        assert_eq!(fs::read_to_string(&claude_path).unwrap(), tampered);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn isc_57_drift_check_flags_stale_and_missing_managed_blocks() {
        let dir = make_temp_dir("translate-drift");
        let ctx = ctx_for(&dir, &["claude-code", "codex"]);
        translate_all(&ctx, &body()).unwrap();
        let findings = check_translation_drift(&ctx, &body());
        assert!(findings
            .iter()
            .all(|finding| finding.level == FindingLevel::Ok));

        let stale_body = compose_instructions(&[InstructionBlock {
            id: "t",
            title: "Test Block",
            content: "OLD content.".to_string(),
        }]);
        let findings = check_translation_drift(&ctx, &stale_body);
        assert_eq!(
            findings
                .iter()
                .filter(|finding| finding.level == FindingLevel::Error)
                .count(),
            2
        );

        fs::remove_file(dir.join("AGENTS.md")).unwrap();
        let findings = check_translation_drift(&ctx, &body());
        assert!(findings.iter().any(|finding| finding
            .message
            .contains("instruction file missing: AGENTS.md")));
        let _ = fs::remove_dir_all(&dir);
    }

    #[cfg(unix)]
    #[test]
    fn non_regular_file_target_is_refused_and_left_untouched() {
        let dir = make_temp_dir("translate-symlink");
        fs::write(dir.join("real.md"), "mounted secret\n").unwrap();
        std::os::unix::fs::symlink(dir.join("real.md"), dir.join("CLAUDE.md")).unwrap();
        let ctx = ctx_for(&dir, &["claude-code"]);
        let results = translate_all(&ctx, &body()).unwrap();
        assert_eq!(results.len(), 1);
        assert!(!results[0].ok);
        assert!(!results[0].changed);
        assert_eq!(
            results[0].error.as_deref(),
            Some(
                "target exists but is not a regular file (symlink/FIFO/socket) — refusing to write; it may be an intentional mount"
            )
        );
        // The mount target was never written through.
        assert_eq!(
            fs::read_to_string(dir.join("real.md")).unwrap(),
            "mounted secret\n"
        );
        let _ = fs::remove_dir_all(&dir);
    }
}
