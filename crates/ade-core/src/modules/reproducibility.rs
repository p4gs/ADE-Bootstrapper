//! Module: reproducible environment & manifest management — port of
//! `src/modules/reproducibility.ts`.
//! Spec component: "Reproducible environment and lockfile management" — an
//! environment manifest (`.ade/manifest.json`) recording os/arch, integrated
//! tool versions, core toolchain versions (bun, git), and harness CLI versions
//! so any drift between machines is visible instead of silent.
//! Boundary controlled: the environment boundary (works-on-my-machine drift).
//!
//! Machine-varying values (versions) live ONLY here and in the lockfile's
//! environment section — that is their designed home. The manifest content is
//! still deterministically ORDERED (write_policy → stable_stringify), and
//! version drift is informational: verify reports it, never fails on it.

use crate::harness::adapters::HARNESS_ADAPTERS;
use crate::modules::shared::{read_json, verify_json_artifact, write_policy};
use crate::types::{
    run_argv, ActionKind, AdeModule, Ctx, Finding, FindingLevel, InstructionBlock, ModuleResult,
    ModuleStatus, PlannedAction, VerifyResult,
};
use serde_json::json;

pub const MANIFEST_PATH: &str = ".ade/manifest.json";

/// Core toolchain executables always probed for the manifest.
pub const CORE_TOOLS: [&str; 2] = ["bun", "git"];

/// Probe `<cmd> --version` and return the first output line, or None on ANY
/// failure (missing binary, nonzero exit, empty output). Absence is a fact the
/// manifest records — never an error.
pub fn probe_version(ctx: &Ctx, cmd: &str) -> Option<String> {
    let result = run_argv(&ctx.exec, &[cmd, "--version"]);
    if result.code != 0 {
        return None;
    }
    let first_line = result.stdout.split('\n').next().map(str::trim)?;
    if first_line.is_empty() {
        None
    } else {
        Some(first_line.to_string())
    }
}

/// Current integrated-tool versions from detection (name → version|null).
fn current_tool_versions(ctx: &Ctx) -> serde_json::Map<String, serde_json::Value> {
    let mut tools = serde_json::Map::new();
    for (name, info) in &ctx.tools {
        let value = if info.present {
            info.version
                .clone()
                .map(serde_json::Value::String)
                .unwrap_or(serde_json::Value::Null)
        } else {
            serde_json::Value::Null
        };
        tools.insert(name.clone(), value);
    }
    tools
}

fn build_manifest(ctx: &Ctx) -> serde_json::Value {
    let mut core = serde_json::Map::new();
    for name in CORE_TOOLS {
        core.insert(
            name.to_string(),
            probe_version(ctx, name)
                .map(serde_json::Value::String)
                .unwrap_or(serde_json::Value::Null),
        );
    }

    let mut harnesses = serde_json::Map::new();
    for adapter in &HARNESS_ADAPTERS {
        let cli = adapter
            .cli_names
            .iter()
            .find(|name| (ctx.which)(name).is_some());
        harnesses.insert(
            adapter.id.to_string(),
            cli.and_then(|cli| probe_version(ctx, cli))
                .map(serde_json::Value::String)
                .unwrap_or(serde_json::Value::Null),
        );
    }

    json!({
        "schemaVersion": 1,
        "os": ctx.os,
        "arch": ctx.arch,
        "core": core,
        "tools": current_tool_versions(ctx),
        "harnesses": harnesses,
    })
}

fn display_version(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::Null => "absent".to_string(),
        serde_json::Value::String(version) => version.clone(),
        other => other.to_string(),
    }
}

fn apply_inner(ctx: &Ctx) -> std::io::Result<ModuleResult> {
    // ISC-108: tolerates everything missing — every probe failure becomes a
    // recorded null, and a fully-null manifest is still a valid, applied state.
    let manifest = build_manifest(ctx);
    write_policy(ctx, MANIFEST_PATH, &manifest)?;
    Ok(ModuleResult {
        status: ModuleStatus::Applied,
        findings: vec![Finding::ok(format!("wrote {MANIFEST_PATH}"))],
        wrote_paths: vec![MANIFEST_PATH.to_string()],
    })
}

pub struct ReproducibilityModule;

pub static MODULE: ReproducibilityModule = ReproducibilityModule;

impl AdeModule for ReproducibilityModule {
    fn id(&self) -> &'static str {
        "reproducibility"
    }
    fn title(&self) -> &'static str {
        "Reproducible Environment"
    }
    fn category(&self) -> &'static str {
        "reproducibility"
    }
    fn spec(&self) -> &'static str {
        "Reproducible environment and lockfile management"
    }

    fn instruction_blocks(&self) -> Vec<InstructionBlock> {
        vec![InstructionBlock {
            id: "reproducibility",
            title: "Reproducible Environment",
            content: concat!(
                "The environment manifest is at `.ade/manifest.json` (os/arch, tool and harness CLI versions).\n",
                "- Before assuming a tool exists, check the manifest; a `null` version means it was absent at bootstrap time.\n",
                "- Report version drift between the manifest and the live environment to the human rather than working around it silently.\n",
                "- Do not hand-edit the manifest; regenerate it with `ade apply` so it reflects the real environment.",
            )
            .to_string(),
        }]
    }

    fn detect(&self, ctx: &Ctx) -> Vec<Finding> {
        let mut findings: Vec<Finding> = vec![Finding::info(format!(
            "environment: {}/{}",
            ctx.os, ctx.arch
        ))];
        for name in CORE_TOOLS {
            findings.push(if (ctx.which)(name).is_some() {
                Finding::ok(format!("{name} resolvable on PATH"))
            } else {
                Finding::info(format!("{name} not on PATH — manifest will record null"))
            });
        }
        let detected_harnesses: Vec<&str> = HARNESS_ADAPTERS
            .iter()
            .filter(|adapter| {
                adapter
                    .cli_names
                    .iter()
                    .any(|name| (ctx.which)(name).is_some())
            })
            .map(|adapter| adapter.id)
            .collect();
        findings.push(Finding::info(if detected_harnesses.is_empty() {
            "no harness CLIs detected — manifest will record null versions".to_string()
        } else {
            format!("harness CLIs detected: {}", detected_harnesses.join(", "))
        }));
        findings
    }

    fn plan(&self, _ctx: &Ctx) -> Vec<PlannedAction> {
        vec![PlannedAction {
            kind: ActionKind::Write,
            path: Some(MANIFEST_PATH.to_string()),
            description: "write environment manifest (os/arch, core + integrated tool versions, harness CLI versions)"
                .to_string(),
        }]
    }

    fn apply(&self, ctx: &Ctx) -> ModuleResult {
        match apply_inner(ctx) {
            Ok(result) => result,
            Err(err) => ModuleResult {
                status: ModuleStatus::Failed,
                findings: vec![Finding::error(format!("module threw: {err}"))],
                wrote_paths: vec![],
            },
        }
    }

    fn verify(&self, ctx: &Ctx) -> VerifyResult {
        let artifact = verify_json_artifact(ctx, MANIFEST_PATH);
        if artifact.level != FindingLevel::Ok {
            return VerifyResult {
                ok: false,
                findings: vec![artifact],
            };
        }

        let parsed = read_json(ctx, MANIFEST_PATH);
        let os_ok = parsed
            .as_ref()
            .and_then(|manifest| manifest.get("os"))
            .map(|value| value.is_string())
            .unwrap_or(false);
        let arch_ok = parsed
            .as_ref()
            .and_then(|manifest| manifest.get("arch"))
            .map(|value| value.is_string())
            .unwrap_or(false);
        // JS `typeof x === "object"` admits arrays too — mirrored here.
        let tools_ok = parsed
            .as_ref()
            .and_then(|manifest| manifest.get("tools"))
            .map(|value| value.is_object() || value.is_array())
            .unwrap_or(false);
        if !(os_ok && arch_ok && tools_ok) {
            return VerifyResult {
                ok: false,
                findings: vec![Finding::error_with(
                    format!("{MANIFEST_PATH} missing required keys (os, arch, tools)"),
                    "run `ade apply` to regenerate the manifest",
                )],
            };
        }
        let parsed = parsed.unwrap_or(serde_json::Value::Null);

        // ISC-109: drift between the recorded manifest and the live environment
        // is informational — surfaced to the human, never a verification failure.
        let mut findings: Vec<Finding> = vec![artifact];
        let recorded = parsed.get("tools").and_then(|tools| tools.as_object());
        for (name, current) in current_tool_versions(ctx) {
            let stored = recorded.and_then(|map| map.get(&name));
            match stored {
                None => findings.push(Finding {
                    level: FindingLevel::Info,
                    message: format!(
                        "tool {name} not recorded in manifest (current: {})",
                        display_version(&current)
                    ),
                    remediation: Some("run `ade apply` to refresh the manifest".to_string()),
                }),
                Some(stored) if *stored != current => findings.push(Finding {
                    level: FindingLevel::Info,
                    message: format!(
                        "version drift for {name}: manifest has {}, environment has {}",
                        display_version(stored),
                        display_version(&current)
                    ),
                    remediation: Some(
                        "run `ade apply` to refresh the manifest after confirming the drift is intended"
                            .to_string(),
                    ),
                }),
                _ => {}
            }
        }
        VerifyResult { ok: true, findings }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fsutil::sha256_hex;
    use crate::testutil::{fake_exec, make_temp_dir, make_test_ctx, TestCtxOptions};
    use crate::types::{ExecResult, Finding};
    use std::sync::Arc;

    fn read_manifest(dir: &std::path::Path) -> serde_json::Value {
        let text = std::fs::read_to_string(dir.join(MANIFEST_PATH)).unwrap();
        serde_json::from_str(&text).unwrap()
    }

    fn write_manifest_raw(dir: &std::path::Path, content: &str) {
        crate::fsutil::write_ensured(&dir.join(MANIFEST_PATH), content).unwrap();
    }

    #[test]
    fn isc_107_apply_writes_manifest_with_os_arch_tools_and_core_bun_git_versions() {
        let dir = make_temp_dir("reproducibility");
        let ctx = make_test_ctx(
            &dir,
            TestCtxOptions {
                present_tools: &[("trufflehog", "3.90.0")],
                exec: Some(fake_exec(&[
                    ("bun --version", (0, "1.2.0\n", "")),
                    (
                        "git --version",
                        (0, "git version 2.44.0\nbuilt from source\n", ""),
                    ),
                ])),
                ..Default::default()
            },
        );
        let result = MODULE.apply(&ctx);
        assert_eq!(result.status, ModuleStatus::Applied);
        assert!(result.wrote_paths.contains(&MANIFEST_PATH.to_string()));
        let manifest = read_manifest(&dir);
        assert_eq!(manifest["os"], "test-os");
        assert_eq!(manifest["arch"], "test-arch");
        assert_eq!(manifest["tools"]["trufflehog"], "3.90.0");
        assert_eq!(manifest["tools"]["gitleaks"], serde_json::Value::Null);
        // first line only, trimmed — multi-line --version output is truncated
        assert_eq!(manifest["core"]["bun"], "1.2.0");
        assert_eq!(manifest["core"]["git"], "git version 2.44.0");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn isc_107_core_tool_version_failure_is_tolerated_and_recorded_as_null() {
        let dir = make_temp_dir("reproducibility");
        let ctx = make_test_ctx(
            &dir,
            TestCtxOptions {
                exec: Some(fake_exec(&[
                    ("bun --version", (0, "1.2.0\n", "")),
                    ("git --version", (1, "", "boom")),
                ])),
                ..Default::default()
            },
        );
        MODULE.apply(&ctx);
        let manifest = read_manifest(&dir);
        assert_eq!(manifest["core"]["bun"], "1.2.0");
        assert_eq!(manifest["core"]["git"], serde_json::Value::Null);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn isc_108_everything_missing_valid_manifest_of_nulls_status_applied() {
        let dir = make_temp_dir("reproducibility");
        // no present_tools, default fake_exec → 127 for everything, which → None
        let ctx = make_test_ctx(&dir, TestCtxOptions::default());
        let result = MODULE.apply(&ctx);
        assert_eq!(result.status, ModuleStatus::Applied);
        let manifest = read_manifest(&dir);
        assert_eq!(manifest["os"], "test-os");
        assert_eq!(manifest["arch"], "test-arch");
        assert!(manifest["core"]
            .as_object()
            .unwrap()
            .values()
            .all(|value| value.is_null()));
        assert!(manifest["tools"]
            .as_object()
            .unwrap()
            .values()
            .all(|value| value.is_null()));
        assert!(manifest["harnesses"]
            .as_object()
            .unwrap()
            .values()
            .all(|value| value.is_null()));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn isc_108_exec_hard_failure_never_propagates_manifest_still_written_applied() {
        // TS oracle uses an exec that THROWS; the Rust ExecFn signature cannot
        // throw, so the closest analogue is an exec that always hard-fails.
        let dir = make_temp_dir("reproducibility");
        let ctx = make_test_ctx(
            &dir,
            TestCtxOptions {
                exec: Some(Arc::new(
                    |_argv: &[String], _opts: &crate::types::ExecOpts| {
                        ExecResult::failure(-1, "spawn refused")
                    },
                )),
                ..Default::default()
            },
        );
        let result = MODULE.apply(&ctx);
        assert_eq!(result.status, ModuleStatus::Applied);
        let manifest = read_manifest(&dir);
        assert_eq!(manifest["core"]["bun"], serde_json::Value::Null);
        assert_eq!(manifest["core"]["git"], serde_json::Value::Null);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn isc_110_harness_cli_hit_via_which_recorded_under_adapter_id_absent_null() {
        let dir = make_temp_dir("reproducibility");
        let ctx = make_test_ctx(
            &dir,
            TestCtxOptions {
                // makes which("claude") resolve
                present_tools: &[("claude", "present")],
                exec: Some(fake_exec(&[(
                    "claude --version",
                    (0, "2.1.0 (Claude Code)\n", ""),
                )])),
                ..Default::default()
            },
        );
        MODULE.apply(&ctx);
        let manifest = read_manifest(&dir);
        assert_eq!(manifest["harnesses"]["claude-code"], "2.1.0 (Claude Code)");
        assert_eq!(manifest["harnesses"]["codex"], serde_json::Value::Null);
        assert_eq!(manifest["harnesses"]["cursor"], serde_json::Value::Null);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn isc_110_harness_cli_present_but_version_fails_recorded_as_null_not_error() {
        let dir = make_temp_dir("reproducibility");
        let ctx = make_test_ctx(
            &dir,
            TestCtxOptions {
                present_tools: &[("codex", "present")],
                // default responses: codex --version unmatched → exit 127
                exec: Some(fake_exec(&[])),
                ..Default::default()
            },
        );
        let result = MODULE.apply(&ctx);
        assert_eq!(result.status, ModuleStatus::Applied);
        let manifest = read_manifest(&dir);
        assert_eq!(manifest["harnesses"]["codex"], serde_json::Value::Null);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn isc_109_verify_reports_version_drift_as_info_findings_never_a_failure() {
        let dir = make_temp_dir("reproducibility");
        let apply_ctx = make_test_ctx(
            &dir,
            TestCtxOptions {
                present_tools: &[("trufflehog", "3.90.0")],
                ..Default::default()
            },
        );
        MODULE.apply(&apply_ctx);

        // Same environment → no drift findings.
        let clean = MODULE.verify(&make_test_ctx(
            &dir,
            TestCtxOptions {
                present_tools: &[("trufflehog", "3.90.0")],
                ..Default::default()
            },
        ));
        assert!(clean.ok);
        assert!(!clean
            .findings
            .iter()
            .any(|finding| finding.message.contains("drift")));

        // Upgraded tool → drift surfaced as info, verify still passes.
        let drifted = MODULE.verify(&make_test_ctx(
            &dir,
            TestCtxOptions {
                present_tools: &[("trufflehog", "3.91.0")],
                ..Default::default()
            },
        ));
        assert!(drifted.ok);
        let drift: &Finding = drifted
            .findings
            .iter()
            .find(|finding| finding.message.contains("drift"))
            .expect("drift finding");
        assert_eq!(drift.level, FindingLevel::Info);
        assert!(drift.message.contains("trufflehog"));
        assert!(drift.message.contains("3.90.0"));
        assert!(drift.message.contains("3.91.0"));

        // Tool disappeared entirely → also informational drift.
        let removed = MODULE.verify(&make_test_ctx(&dir, TestCtxOptions::default()));
        assert!(removed.ok);
        assert!(removed.findings.iter().any(|finding| {
            finding.level == FindingLevel::Info && finding.message.contains("trufflehog")
        }));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn isc_109_manifest_missing_or_corrupt_verify_error() {
        let dir = make_temp_dir("reproducibility");
        let ctx = make_test_ctx(&dir, TestCtxOptions::default());
        let missing = MODULE.verify(&ctx);
        assert!(!missing.ok);
        assert!(missing
            .findings
            .iter()
            .any(|finding| finding.level == FindingLevel::Error));

        MODULE.apply(&ctx);
        assert!(MODULE.verify(&ctx).ok);

        write_manifest_raw(&dir, "{not json");
        assert!(!MODULE.verify(&ctx).ok);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn verify_fails_when_manifest_parses_but_lacks_os_arch_tools_keys() {
        let dir = make_temp_dir("reproducibility");
        write_manifest_raw(&dir, r#"{"os":"test-os"}"#);
        let result = MODULE.verify(&make_test_ctx(&dir, TestCtxOptions::default()));
        assert!(!result.ok);
        assert!(result
            .findings
            .iter()
            .any(|finding| finding.message.contains("os, arch, tools")));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn isc_116_plan_writes_nothing_to_disk() {
        let dir = make_temp_dir("reproducibility");
        let ctx = make_test_ctx(&dir, TestCtxOptions::default());
        let actions = MODULE.plan(&ctx);
        assert!(!actions.is_empty());
        assert_eq!(actions[0].path.as_deref(), Some(MANIFEST_PATH));
        assert!(!dir.join(MANIFEST_PATH).exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn isc_117_apply_is_idempotent_second_run_byte_identical() {
        let dir = make_temp_dir("reproducibility");
        let rules: &[(&str, (i32, &str, &str))] = &[
            ("bun --version", (0, "1.2.0\n", "")),
            ("git --version", (0, "git version 2.44.0\n", "")),
        ];
        MODULE.apply(&make_test_ctx(
            &dir,
            TestCtxOptions {
                present_tools: &[("trufflehog", "3.90.0")],
                exec: Some(fake_exec(rules)),
                ..Default::default()
            },
        ));
        let first = sha256_hex(&std::fs::read_to_string(dir.join(MANIFEST_PATH)).unwrap());
        MODULE.apply(&make_test_ctx(
            &dir,
            TestCtxOptions {
                present_tools: &[("trufflehog", "3.90.0")],
                exec: Some(fake_exec(rules)),
                ..Default::default()
            },
        ));
        let second = sha256_hex(&std::fs::read_to_string(dir.join(MANIFEST_PATH)).unwrap());
        assert_eq!(second, first);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn manifest_contains_no_timestamps_or_absolute_paths() {
        let dir = make_temp_dir("reproducibility");
        let ctx = make_test_ctx(
            &dir,
            TestCtxOptions {
                present_tools: &[("trufflehog", "3.90.0")],
                ..Default::default()
            },
        );
        MODULE.apply(&ctx);
        let text = std::fs::read_to_string(dir.join(MANIFEST_PATH)).unwrap();
        assert!(!text.contains(dir.to_str().unwrap()));
        assert!(!text.contains("/fake/bin"));
        let timestamp = regex::Regex::new(r"\d{4}-\d{2}-\d{2}T").unwrap();
        assert!(!timestamp.is_match(&text));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn instruction_block_points_harnesses_at_manifest_and_mandates_drift_reporting() {
        let blocks = MODULE.instruction_blocks();
        assert!(!blocks.is_empty());
        assert!(blocks[0].content.contains(".ade/manifest.json"));
        assert!(blocks[0].content.to_lowercase().contains("drift"));
    }

    #[test]
    fn detect_reports_environment_facts_without_degradation_when_tools_absent() {
        let dir = make_temp_dir("reproducibility");
        let findings = MODULE.detect(&make_test_ctx(&dir, TestCtxOptions::default()));
        assert!(findings
            .iter()
            .any(|finding| finding.message.contains("test-os/test-arch")));
        assert!(findings.iter().all(|finding| {
            finding.level == FindingLevel::Ok || finding.level == FindingLevel::Info
        }));
        assert!(!dir.join(MANIFEST_PATH).exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn probe_version_returns_none_on_empty_stdout_and_first_trimmed_line_on_success() {
        let dir = make_temp_dir("reproducibility");
        let empty_ctx = make_test_ctx(
            &dir,
            TestCtxOptions {
                exec: Some(fake_exec(&[("bun --version", (0, "\n", ""))])),
                ..Default::default()
            },
        );
        assert_eq!(probe_version(&empty_ctx, "bun"), None);
        let ok_ctx = make_test_ctx(
            &dir,
            TestCtxOptions {
                exec: Some(fake_exec(&[("bun --version", (0, "  1.2.0  \nextra", ""))])),
                ..Default::default()
            },
        );
        assert_eq!(probe_version(&ok_ctx, "bun"), Some("1.2.0".to_string()));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
