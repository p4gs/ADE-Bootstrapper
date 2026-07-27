//! Module: token efficiency (RTK integration) — port of
//! `src/modules/token-efficiency.ts`.
//! Spec component: "Lossless or minimally lossy token efficiency mechanisms —
//! shell-output filtering, grouping, truncation, and deduplication so
//! high-volume command output does not flood the context window (e.g. RTK)."
//! Boundary controlled: the shell-output/context-window boundary (token waste).
//!
//! Integration over rebuild: RTK is the reducer. When present, the policy
//! records the detected version and enables shell-boundary wrapping; when
//! absent, the policy is still written (enabled:false) so the contract is
//! explicit and verify can re-derive expectations from the live machine.

use crate::modules::shared::{read_json, tool_finding, verify_json_artifact, write_policy};
use crate::types::{
    ActionKind, AdeModule, Ctx, Finding, FindingLevel, InstructionBlock, ModuleResult,
    ModuleStatus, PlannedAction, VerifyResult,
};
use serde_json::json;

pub const TOKEN_EFFICIENCY_POLICY_PATH: &str = ".ade/policy/token-efficiency.json";

pub const RTK_INSTALL_GUIDANCE: &str =
    "install rtk (github.com/rtk-ai/rtk — cuts common dev-command output 60-90%)";

fn build_policy(ctx: &Ctx) -> serde_json::Value {
    let rtk = ctx.tools.get("rtk");
    let present = rtk.map(|tool| tool.present).unwrap_or(false);
    let mut policy = json!({
        "schemaVersion": 1,
        "enabled": present,
        "tool": if present { "rtk" } else { "none" },
        "integration": "shell-boundary",
        "mechanisms": ["filtering", "grouping", "truncation", "deduplication"],
        "guarantee": "reversible/semantically-lossless transforms preferred over naive summarization",
    });
    if present {
        if let Some(version) = rtk.and_then(|tool| tool.version.as_ref()) {
            policy["toolVersion"] = serde_json::Value::String(version.clone());
        }
    }
    policy
}

fn apply_inner(ctx: &Ctx) -> std::io::Result<ModuleResult> {
    let policy = build_policy(ctx);
    write_policy(ctx, TOKEN_EFFICIENCY_POLICY_PATH, &policy)?;
    let enabled = policy["enabled"].as_bool().unwrap_or(false);
    if !enabled {
        return Ok(ModuleResult {
            status: ModuleStatus::Degraded,
            findings: vec![
                Finding::ok(format!("wrote {TOKEN_EFFICIENCY_POLICY_PATH} (disabled)")),
                Finding::degraded(
                    "rtk not installed — shell-output token reduction unavailable",
                    RTK_INSTALL_GUIDANCE,
                ),
            ],
            wrote_paths: vec![TOKEN_EFFICIENCY_POLICY_PATH.to_string()],
        });
    }
    Ok(ModuleResult {
        status: ModuleStatus::Applied,
        findings: vec![Finding::ok(format!("wrote {TOKEN_EFFICIENCY_POLICY_PATH}"))],
        wrote_paths: vec![TOKEN_EFFICIENCY_POLICY_PATH.to_string()],
    })
}

pub struct TokenEfficiencyModule;

pub static MODULE: TokenEfficiencyModule = TokenEfficiencyModule;

impl AdeModule for TokenEfficiencyModule {
    fn id(&self) -> &'static str {
        "token-efficiency"
    }
    fn title(&self) -> &'static str {
        "Token Efficiency"
    }
    fn category(&self) -> &'static str {
        "efficiency"
    }
    fn spec(&self) -> &'static str {
        "Lossless or minimally lossy token efficiency mechanisms"
    }

    fn instruction_blocks(&self) -> Vec<InstructionBlock> {
        vec![InstructionBlock {
            id: "token-efficiency",
            title: "Token Efficiency",
            content: concat!(
                "This project has a token-efficiency contract at `.ade/policy/token-efficiency.json`.\n",
                "- Prefer rtk-wrapped commands for high-volume output: test runs, builds, logs, diffs, and file listings.\n",
                "- Never paste multi-hundred-line raw output into context when a filtered form answers the question.\n",
                "- Token efficiency must never drop error details — keep failures verbatim.",
            )
            .to_string(),
        }]
    }

    fn detect(&self, ctx: &Ctx) -> Vec<Finding> {
        vec![tool_finding(ctx, "rtk", RTK_INSTALL_GUIDANCE)]
    }

    fn plan(&self, _ctx: &Ctx) -> Vec<PlannedAction> {
        vec![PlannedAction {
            kind: ActionKind::Write,
            path: Some(TOKEN_EFFICIENCY_POLICY_PATH.to_string()),
            description: "write token-efficiency policy (rtk shell-boundary integration)"
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
        let artifact = verify_json_artifact(ctx, TOKEN_EFFICIENCY_POLICY_PATH);
        if artifact.level != FindingLevel::Ok {
            return VerifyResult {
                ok: false,
                findings: vec![artifact],
            };
        }
        let parsed = read_json(ctx, TOKEN_EFFICIENCY_POLICY_PATH);
        let enabled = parsed
            .as_ref()
            .and_then(|policy| policy.get("enabled"))
            .and_then(|value| value.as_bool());
        let Some(enabled) = enabled else {
            return VerifyResult {
                ok: false,
                findings: vec![Finding::error_with(
                    format!("{TOKEN_EFFICIENCY_POLICY_PATH} missing boolean 'enabled'"),
                    "run `ade apply` to regenerate",
                )],
            };
        };
        let rtk_present = ctx.tool_present("rtk");
        if enabled != rtk_present {
            return VerifyResult {
                ok: false,
                findings: vec![Finding::error_with(
                    format!(
                        "{TOKEN_EFFICIENCY_POLICY_PATH} enabled={enabled} but rtk {} installed",
                        if rtk_present { "is" } else { "is not" }
                    ),
                    "run `ade apply` to re-derive the policy from the current machine",
                )],
            };
        }
        VerifyResult {
            ok: true,
            findings: vec![artifact],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fsutil::sha256_hex;
    use crate::testutil::{make_temp_dir, make_test_ctx, TestCtxOptions};

    fn read_policy(dir: &std::path::Path) -> serde_json::Value {
        let text = std::fs::read_to_string(dir.join(TOKEN_EFFICIENCY_POLICY_PATH)).unwrap();
        serde_json::from_str(&text).unwrap()
    }

    fn write_policy_raw(dir: &std::path::Path, content: &str) {
        crate::fsutil::write_ensured(&dir.join(TOKEN_EFFICIENCY_POLICY_PATH), content).unwrap();
    }

    #[test]
    fn isc_111_rtk_present_applied_with_enabled_policy_at_the_shell_boundary() {
        let dir = make_temp_dir("token-efficiency");
        let ctx = make_test_ctx(
            &dir,
            TestCtxOptions {
                present_tools: &[("rtk", "rtk 0.9.2")],
                ..Default::default()
            },
        );
        let result = MODULE.apply(&ctx);
        assert_eq!(result.status, ModuleStatus::Applied);
        assert!(result
            .wrote_paths
            .contains(&TOKEN_EFFICIENCY_POLICY_PATH.to_string()));
        let policy = read_policy(&dir);
        assert_eq!(policy["enabled"], serde_json::json!(true));
        assert_eq!(policy["tool"], "rtk");
        assert_eq!(policy["integration"], "shell-boundary");
        assert_eq!(
            policy["mechanisms"],
            serde_json::json!(["filtering", "grouping", "truncation", "deduplication"])
        );
        assert!(policy["guarantee"]
            .as_str()
            .unwrap()
            .contains("reversible/semantically-lossless"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn isc_112_rtk_absent_policy_still_written_enabled_false_degraded_with_guidance() {
        let dir = make_temp_dir("token-efficiency");
        let ctx = make_test_ctx(&dir, TestCtxOptions::default());
        let result = MODULE.apply(&ctx);
        assert_eq!(result.status, ModuleStatus::Degraded);
        assert!(result
            .wrote_paths
            .contains(&TOKEN_EFFICIENCY_POLICY_PATH.to_string()));
        let policy = read_policy(&dir);
        assert_eq!(policy["enabled"], serde_json::json!(false));
        let degraded = result
            .findings
            .iter()
            .find(|finding| finding.level == FindingLevel::Degraded)
            .expect("degraded finding");
        let remediation = degraded.remediation.as_deref().expect("remediation");
        assert!(remediation.contains("github.com/rtk-ai/rtk"));
        assert!(remediation.contains("60-90%"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn isc_113_policy_records_the_detected_rtk_version() {
        let dir = make_temp_dir("token-efficiency");
        let ctx = make_test_ctx(
            &dir,
            TestCtxOptions {
                present_tools: &[("rtk", "rtk 1.4.7")],
                ..Default::default()
            },
        );
        MODULE.apply(&ctx);
        let policy = read_policy(&dir);
        assert_eq!(policy["toolVersion"], "rtk 1.4.7");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn isc_116_plan_writes_nothing_to_disk() {
        let dir = make_temp_dir("token-efficiency");
        let ctx = make_test_ctx(
            &dir,
            TestCtxOptions {
                present_tools: &[("rtk", "rtk 0.9.2")],
                ..Default::default()
            },
        );
        let actions = MODULE.plan(&ctx);
        assert!(!actions.is_empty());
        assert_eq!(
            actions[0].path.as_deref(),
            Some(TOKEN_EFFICIENCY_POLICY_PATH)
        );
        assert!(!dir.join(TOKEN_EFFICIENCY_POLICY_PATH).exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn isc_117_apply_is_idempotent_second_run_byte_identical() {
        let dir = make_temp_dir("token-efficiency");
        MODULE.apply(&make_test_ctx(
            &dir,
            TestCtxOptions {
                present_tools: &[("rtk", "rtk 0.9.2")],
                ..Default::default()
            },
        ));
        let first =
            sha256_hex(&std::fs::read_to_string(dir.join(TOKEN_EFFICIENCY_POLICY_PATH)).unwrap());
        MODULE.apply(&make_test_ctx(
            &dir,
            TestCtxOptions {
                present_tools: &[("rtk", "rtk 0.9.2")],
                ..Default::default()
            },
        ));
        let second =
            sha256_hex(&std::fs::read_to_string(dir.join(TOKEN_EFFICIENCY_POLICY_PATH)).unwrap());
        assert_eq!(second, first);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn instruction_block_carries_rtk_wrapping_and_verbatim_failure_guidance() {
        let blocks = MODULE.instruction_blocks();
        assert!(!blocks.is_empty());
        assert!(blocks[0]
            .content
            .contains(".ade/policy/token-efficiency.json"));
        assert!(blocks[0].content.contains("rtk-wrapped"));
        assert!(blocks[0].content.to_lowercase().contains("verbatim"));
    }

    #[test]
    fn detect_reports_rtk_presence_and_absence_with_remediation() {
        let dir = make_temp_dir("token-efficiency");
        let present = MODULE.detect(&make_test_ctx(
            &dir,
            TestCtxOptions {
                present_tools: &[("rtk", "rtk 0.9.2")],
                ..Default::default()
            },
        ));
        assert!(present
            .iter()
            .all(|finding| finding.level == FindingLevel::Ok));
        let absent = MODULE.detect(&make_test_ctx(&dir, TestCtxOptions::default()));
        assert!(absent.iter().any(|finding| {
            finding.level == FindingLevel::Degraded
                && finding
                    .remediation
                    .as_deref()
                    .map(|remediation| remediation.contains("github.com/rtk-ai/rtk"))
                    .unwrap_or(false)
        }));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn verify_passes_after_apply_fails_on_missing_or_corrupted_policy() {
        let dir = make_temp_dir("token-efficiency");
        let ctx = make_test_ctx(
            &dir,
            TestCtxOptions {
                present_tools: &[("rtk", "rtk 0.9.2")],
                ..Default::default()
            },
        );
        assert!(!MODULE.verify(&ctx).ok);
        MODULE.apply(&ctx);
        assert!(MODULE.verify(&ctx).ok);

        write_policy_raw(&dir, "{not json");
        assert!(!MODULE.verify(&ctx).ok);

        write_policy_raw(&dir, r#"{"enabled":"yes"}"#);
        assert!(!MODULE.verify(&ctx).ok);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn verify_enabled_must_equal_rtk_presence_in_the_current_ctx_re_derived() {
        let dir = make_temp_dir("token-efficiency");
        // Applied with rtk present, then verified on a machine without rtk → stale policy fails.
        MODULE.apply(&make_test_ctx(
            &dir,
            TestCtxOptions {
                present_tools: &[("rtk", "rtk 0.9.2")],
                ..Default::default()
            },
        ));
        let without_rtk = make_test_ctx(&dir, TestCtxOptions::default());
        let stale_enabled = MODULE.verify(&without_rtk);
        assert!(!stale_enabled.ok);
        assert!(stale_enabled
            .findings
            .iter()
            .any(|finding| finding.level == FindingLevel::Error));

        // Applied without rtk, then verified on a machine with rtk → also stale, also fails.
        MODULE.apply(&without_rtk);
        let stale_disabled = MODULE.verify(&make_test_ctx(
            &dir,
            TestCtxOptions {
                present_tools: &[("rtk", "rtk 0.9.2")],
                ..Default::default()
            },
        ));
        assert!(!stale_disabled.ok);

        // Re-applying on the current machine repairs verification.
        assert!(MODULE.verify(&without_rtk).ok);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
