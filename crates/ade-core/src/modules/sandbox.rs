//! Module: AI-native sandboxing — port of `src/modules/sandbox.ts`.
//! Spec component: "AI-native sandboxing — declarative filesystem/network/
//! credential policy for harness-driven execution, with kernel-level
//! enforcement via nono (nono.sh) when installed."
//! Boundary controlled: the shell/filesystem/network boundary.
//!
//! Integration over rebuild: nono is the enforcer. The module always writes
//! the declarative policy (`.ade/policy/sandbox.json`) so harnesses and
//! humans share one contract; when nono is absent the policy is advisory and
//! the module reports 'degraded' with install guidance. For claude-code
//! targets, the deny-read surface is additionally mapped into
//! `.claude/settings.json` permissions so the harness itself refuses
//! credential-file reads.

use serde_json::json;

use crate::harness::claude::{merge_claude_settings, CLAUDE_SETTINGS_PATH};
use crate::modules::shared::{read_json, tool_finding, verify_json_artifact, write_policy};
use crate::types::{
    ActionKind, AdeModule, Ctx, Finding, FindingLevel, InstructionBlock, ModuleResult,
    ModuleStatus, PlannedAction, VerifyResult,
};

pub const SANDBOX_POLICY_PATH: &str = ".ade/policy/sandbox.json";

/// Default egress allowlist: package registries + source forge only.
pub const DEFAULT_NETWORK_ALLOWLIST: [&str; 5] = [
    "registry.npmjs.org",
    "pypi.org",
    "crates.io",
    "proxy.golang.org",
    "github.com",
];

/// Claude Code permission entries denying reads of credential-bearing files.
pub const CLAUDE_DENY_READ: [&str; 4] = [
    "Read(./.env)",
    "Read(./.env.*)",
    "Read(~/.ssh/**)",
    "Read(~/.aws/**)",
];

const NONO_REMEDIATION: &str = "install nono (nono.sh) for kernel-enforced sandboxing";

/// Validate sandbox options; returns errors (empty = valid).
pub fn validate_sandbox_options(options: &serde_json::Value) -> Vec<String> {
    let mut errors: Vec<String> = Vec::new();
    for key in ["allowHosts", "denyRead", "denyWrite"] {
        if let Some(value) = options.get(key) {
            let valid = value
                .as_array()
                .map(|entries| {
                    entries
                        .iter()
                        .all(|entry| entry.as_str().map(|text| !text.is_empty()).unwrap_or(false))
                })
                .unwrap_or(false);
            if !valid {
                errors.push(format!(
                    "options.{key} must be an array of non-empty strings"
                ));
            }
        }
    }
    errors
}

/// Append extras to a base list, deduplicated, base order preserved (deterministic).
fn with_extras(base: &[&str], extras: Option<&serde_json::Value>) -> Vec<String> {
    let mut out: Vec<String> = base.iter().map(|entry| entry.to_string()).collect();
    if let Some(serde_json::Value::Array(entries)) = extras {
        for entry in entries {
            if let Some(text) = entry.as_str() {
                if !out.iter().any(|existing| existing == text) {
                    out.push(text.to_string());
                }
            }
        }
    }
    out
}

/// Declarative policy — paths are placeholders (`<repo>`, `~`), never machine-absolute.
fn build_policy(options: &serde_json::Value, enforcer: Option<&str>) -> serde_json::Value {
    json!({
        "schemaVersion": 1,
        "enforcement": enforcer.unwrap_or("advisory"),
        "filesystem": {
            "writeScope": ["<repo>"],
            "denyWrite": with_extras(
                &["~/.ssh", "~/.aws", "~/.claude", "system paths"],
                options.get("denyWrite"),
            ),
            "denyRead": with_extras(&[".env", ".env.*", "~/.ssh/**"], options.get("denyRead")),
        },
        "network": {
            "default": "deny",
            "allowlist": with_extras(&DEFAULT_NETWORK_ALLOWLIST, options.get("allowHosts")),
        },
        "credentials": {
            "injection": "at-boundary",
            "rule": "secrets are injected by the sandbox at exec time, never stored in the environment or files",
        },
    })
}

fn targets_claude_code(ctx: &Ctx) -> bool {
    ctx.config
        .harnesses
        .iter()
        .any(|harness| harness == "claude-code")
}

pub struct SandboxModule;

pub static MODULE: SandboxModule = SandboxModule;

fn apply_inner(ctx: &Ctx) -> std::io::Result<ModuleResult> {
    let options = ctx.module_options("sandbox");
    let errors = validate_sandbox_options(&options);
    if !errors.is_empty() {
        return Ok(ModuleResult {
            status: ModuleStatus::Failed,
            findings: errors.into_iter().map(Finding::error).collect(),
            wrote_paths: vec![],
        });
    }

    let mut findings: Vec<Finding> = Vec::new();
    let mut wrote_paths: Vec<String> = Vec::new();
    let enforcer = if ctx.tool_present("nono") {
        Some("nono")
    } else {
        None
    };

    write_policy(ctx, SANDBOX_POLICY_PATH, &build_policy(&options, enforcer))?;
    wrote_paths.push(SANDBOX_POLICY_PATH.to_string());
    findings.push(Finding::ok(format!("wrote {SANDBOX_POLICY_PATH}")));

    let mut degraded = false;
    if targets_claude_code(ctx) {
        let merge =
            merge_claude_settings(ctx, &json!({ "permissions": { "deny": CLAUDE_DENY_READ } }))?;
        let merge_ok = merge.level == FindingLevel::Ok;
        findings.push(merge);
        if merge_ok {
            wrote_paths.push(CLAUDE_SETTINGS_PATH.to_string());
        } else {
            degraded = true;
        }
    }

    if enforcer.is_none() {
        degraded = true;
        findings.push(Finding::degraded(
            "nono not installed — sandbox policy is advisory (harness-instruction level only)",
            NONO_REMEDIATION,
        ));
    }
    Ok(ModuleResult {
        status: if degraded {
            ModuleStatus::Degraded
        } else {
            ModuleStatus::Applied
        },
        findings,
        wrote_paths,
    })
}

impl AdeModule for SandboxModule {
    fn id(&self) -> &'static str {
        "sandbox"
    }
    fn title(&self) -> &'static str {
        "AI-Native Sandboxing"
    }
    fn category(&self) -> &'static str {
        "security"
    }
    fn spec(&self) -> &'static str {
        "AI-native sandboxing"
    }

    fn instruction_blocks(&self) -> Vec<InstructionBlock> {
        vec![InstructionBlock {
            id: "sandbox",
            title: "Sandbox Policy",
            content: [
                "This project has a sandbox contract at `.ade/policy/sandbox.json`. Operate inside it.",
                "- Write only inside this repository; NEVER write to `~/.ssh`, `~/.aws`, `~/.claude`, or system paths.",
                "- NEVER attempt to read credential files (`.env`, `.env.*`, `~/.ssh/**`, `~/.aws/**`).",
                "- Network egress is deny-by-default with a package-registry allowlist; never attempt to bypass, tunnel, or proxy around network controls.",
                "- Secrets are injected at the sandbox boundary at exec time — never persist them to the environment or files.",
                "- If a task needs access outside this policy, STOP and ask a human — do not work around the sandbox.",
            ]
            .join("\n"),
        }]
    }

    fn detect(&self, ctx: &Ctx) -> Vec<Finding> {
        let mut findings = vec![tool_finding(ctx, "nono", NONO_REMEDIATION)];
        for message in validate_sandbox_options(&ctx.module_options("sandbox")) {
            findings.push(Finding::error(message));
        }
        findings
    }

    fn plan(&self, ctx: &Ctx) -> Vec<PlannedAction> {
        let mut actions = vec![PlannedAction {
            kind: ActionKind::Write,
            path: Some(SANDBOX_POLICY_PATH.to_string()),
            description:
                "write sandbox policy (filesystem scope, deny-by-default network, credential injection)"
                    .to_string(),
        }];
        if targets_claude_code(ctx) {
            actions.push(PlannedAction {
                kind: ActionKind::Merge,
                path: Some(CLAUDE_SETTINGS_PATH.to_string()),
                description:
                    "merge credential-file deny-read permissions into Claude Code settings"
                        .to_string(),
            });
        }
        actions
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
        let mut findings: Vec<Finding> = Vec::new();
        let artifact = verify_json_artifact(ctx, SANDBOX_POLICY_PATH);
        let artifact_ok = artifact.level == FindingLevel::Ok;
        findings.push(artifact);
        if !artifact_ok {
            return VerifyResult {
                ok: false,
                findings,
            };
        }

        let mut ok = true;
        let policy = read_json(ctx, SANDBOX_POLICY_PATH);
        let network_ok = policy
            .as_ref()
            .and_then(|policy| policy.get("network"))
            .map(|network| {
                network.get("default").and_then(|value| value.as_str()) == Some("deny")
                    && network
                        .get("allowlist")
                        .map(|allowlist| allowlist.is_array())
                        .unwrap_or(false)
            })
            .unwrap_or(false);
        if !network_ok {
            ok = false;
            findings.push(Finding::error_with(
                format!("{SANDBOX_POLICY_PATH} network policy is not deny-with-allowlist"),
                "run `ade apply` to regenerate",
            ));
        } else {
            findings.push(Finding::ok(
                "network policy is deny-by-default with allowlist",
            ));
        }

        if targets_claude_code(ctx) {
            let settings = read_json(ctx, CLAUDE_SETTINGS_PATH);
            let deny = settings
                .as_ref()
                .and_then(|settings| settings.get("permissions"))
                .and_then(|permissions| permissions.get("deny"))
                .and_then(|deny| deny.as_array());
            let missing: Vec<&str> = CLAUDE_DENY_READ
                .iter()
                .copied()
                .filter(|entry| {
                    !deny
                        .map(|list| list.iter().any(|item| item.as_str() == Some(entry)))
                        .unwrap_or(false)
                })
                .collect();
            if !missing.is_empty() {
                ok = false;
                findings.push(Finding::error_with(
                    format!(
                        "{CLAUDE_SETTINGS_PATH} missing deny-read entries: {}",
                        missing.join(", ")
                    ),
                    "run `ade apply`",
                ));
            } else {
                findings.push(Finding::ok(format!(
                    "{CLAUDE_SETTINGS_PATH} denies credential-file reads"
                )));
            }
        }
        VerifyResult { ok, findings }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fsutil::sha256_hex;
    use crate::testutil::{make_temp_dir, make_test_ctx, test_config_with, TestCtxOptions};
    use std::path::Path;

    fn read_policy(dir: &Path) -> serde_json::Value {
        serde_json::from_str(&std::fs::read_to_string(dir.join(SANDBOX_POLICY_PATH)).unwrap())
            .unwrap()
    }

    fn write_file(dir: &Path, rel: &str, content: &str) {
        let path = dir.join(rel);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(path, content).unwrap();
    }

    #[test]
    fn isc_68_apply_writes_sandbox_policy_with_filesystem_network_and_credential_contract() {
        let dir = make_temp_dir("sandbox-68");
        let ctx = make_test_ctx(
            &dir,
            TestCtxOptions {
                present_tools: &[("nono", "nono 0.3.0")],
                ..Default::default()
            },
        );
        let result = MODULE.apply(&ctx);
        assert_eq!(result.status, ModuleStatus::Applied);
        assert!(result
            .wrote_paths
            .contains(&SANDBOX_POLICY_PATH.to_string()));
        let policy = read_policy(&dir);
        assert_eq!(
            policy["filesystem"]["writeScope"],
            serde_json::json!(["<repo>"])
        );
        assert_eq!(
            policy["filesystem"]["denyWrite"],
            serde_json::json!(["~/.ssh", "~/.aws", "~/.claude", "system paths"])
        );
        assert_eq!(
            policy["filesystem"]["denyRead"],
            serde_json::json!([".env", ".env.*", "~/.ssh/**"])
        );
        assert_eq!(
            policy["network"]["allowlist"],
            serde_json::json!(DEFAULT_NETWORK_ALLOWLIST)
        );
        assert!(policy["network"]["allowlist"]
            .as_array()
            .unwrap()
            .iter()
            .any(|entry| entry.as_str() == Some("registry.npmjs.org")));
        assert_eq!(
            policy["credentials"]["injection"],
            serde_json::json!("at-boundary")
        );
        assert!(policy["credentials"]["rule"]
            .as_str()
            .unwrap()
            .contains("injected by the sandbox at exec time"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn isc_68_policy_contains_no_absolute_machine_paths() {
        let dir = make_temp_dir("sandbox-abs");
        MODULE.apply(&make_test_ctx(
            &dir,
            TestCtxOptions {
                present_tools: &[("nono", "0.3.0")],
                ..Default::default()
            },
        ));
        let raw = std::fs::read_to_string(dir.join(SANDBOX_POLICY_PATH)).unwrap();
        assert!(!raw.contains(&dir.to_string_lossy().to_string()));
        assert!(!raw.contains("/Users/"));
        assert!(!raw.contains("/home/"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn isc_71_default_network_policy_is_deny_with_allowlist() {
        let dir = make_temp_dir("sandbox-71");
        MODULE.apply(&make_test_ctx(
            &dir,
            TestCtxOptions {
                present_tools: &[("nono", "0.3.0")],
                ..Default::default()
            },
        ));
        let policy = read_policy(&dir);
        assert_eq!(policy["network"]["default"], serde_json::json!("deny"));
        assert!(policy["network"]["allowlist"].is_array());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn isc_69_detect_reports_nono_with_install_remediation_when_absent() {
        let dir = make_temp_dir("sandbox-69");
        let findings = MODULE.detect(&make_test_ctx(&dir, TestCtxOptions::default()));
        let nono = findings
            .iter()
            .find(|finding| finding.message.contains("nono"))
            .expect("nono finding expected");
        assert_eq!(nono.level, FindingLevel::Degraded);
        assert!(nono.remediation.as_deref().unwrap().contains("nono.sh"));
        let present = MODULE.detect(&make_test_ctx(
            &dir,
            TestCtxOptions {
                present_tools: &[("nono", "0.3.0")],
                ..Default::default()
            },
        ));
        assert!(present
            .iter()
            .any(|finding| finding.level == FindingLevel::Ok && finding.message.contains("nono")));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn isc_69_nono_absent_degraded_but_policy_still_written() {
        let dir = make_temp_dir("sandbox-69b");
        let ctx = make_test_ctx(&dir, TestCtxOptions::default());
        let result = MODULE.apply(&ctx);
        assert_eq!(result.status, ModuleStatus::Degraded);
        assert!(result.findings.iter().any(|finding| finding
            .remediation
            .as_deref()
            .map(|remediation| remediation.contains("nono.sh"))
            .unwrap_or(false)));
        let policy = read_policy(&dir);
        assert_eq!(policy["network"]["default"], serde_json::json!("deny"));
        assert_eq!(policy["enforcement"], serde_json::json!("advisory"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn isc_70_claude_code_targeted_deny_read_surface_mapped_into_claude_settings() {
        let dir = make_temp_dir("sandbox-70");
        let ctx = make_test_ctx(
            &dir,
            TestCtxOptions {
                present_tools: &[("nono", "0.3.0")],
                ..Default::default()
            },
        );
        let result = MODULE.apply(&ctx);
        assert!(result
            .wrote_paths
            .contains(&CLAUDE_SETTINGS_PATH.to_string()));
        let settings: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(dir.join(CLAUDE_SETTINGS_PATH)).unwrap())
                .unwrap();
        let deny = settings["permissions"]["deny"].as_array().unwrap();
        for entry in CLAUDE_DENY_READ {
            assert!(
                deny.iter().any(|item| item.as_str() == Some(entry)),
                "missing deny entry {entry}"
            );
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn isc_70_pre_existing_user_deny_entry_is_preserved_array_union() {
        let dir = make_temp_dir("sandbox-70b");
        write_file(
            &dir,
            CLAUDE_SETTINGS_PATH,
            &serde_json::to_string(&serde_json::json!({
                "permissions": { "deny": ["Bash(rm -rf *)"] },
                "model": "user-choice",
            }))
            .unwrap(),
        );
        let ctx = make_test_ctx(
            &dir,
            TestCtxOptions {
                present_tools: &[("nono", "0.3.0")],
                ..Default::default()
            },
        );
        MODULE.apply(&ctx);
        let settings: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(dir.join(CLAUDE_SETTINGS_PATH)).unwrap())
                .unwrap();
        let deny = settings["permissions"]["deny"].as_array().unwrap();
        assert!(deny
            .iter()
            .any(|item| item.as_str() == Some("Bash(rm -rf *)")));
        assert_eq!(settings["model"], serde_json::json!("user-choice"));
        for entry in CLAUDE_DENY_READ {
            assert!(
                deny.iter().any(|item| item.as_str() == Some(entry)),
                "missing deny entry {entry}"
            );
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn isc_70_claude_code_not_targeted_no_claude_settings_written() {
        let dir = make_temp_dir("sandbox-70c");
        let config = test_config_with(&[], &["codex"]);
        let ctx = make_test_ctx(
            &dir,
            TestCtxOptions {
                config: Some(config),
                present_tools: &[("nono", "0.3.0")],
                ..Default::default()
            },
        );
        let result = MODULE.apply(&ctx);
        assert_eq!(result.status, ModuleStatus::Applied);
        assert!(!dir.join(CLAUDE_SETTINGS_PATH).exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn isc_116_plan_writes_nothing_to_disk() {
        let dir = make_temp_dir("sandbox-116");
        let ctx = make_test_ctx(
            &dir,
            TestCtxOptions {
                present_tools: &[("nono", "0.3.0")],
                ..Default::default()
            },
        );
        let actions = MODULE.plan(&ctx);
        assert!(actions
            .iter()
            .any(|action| action.path.as_deref() == Some(SANDBOX_POLICY_PATH)));
        assert!(actions
            .iter()
            .any(|action| action.path.as_deref() == Some(CLAUDE_SETTINGS_PATH)));
        assert!(!dir.join(SANDBOX_POLICY_PATH).exists());
        assert!(!dir.join(CLAUDE_SETTINGS_PATH).exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn isc_117_apply_is_idempotent_second_run_byte_identical_for_policy_and_settings() {
        let dir = make_temp_dir("sandbox-117");
        let ctx = make_test_ctx(
            &dir,
            TestCtxOptions {
                present_tools: &[("nono", "0.3.0")],
                ..Default::default()
            },
        );
        MODULE.apply(&ctx);
        let first_policy =
            sha256_hex(&std::fs::read_to_string(dir.join(SANDBOX_POLICY_PATH)).unwrap());
        let first_settings =
            sha256_hex(&std::fs::read_to_string(dir.join(CLAUDE_SETTINGS_PATH)).unwrap());
        MODULE.apply(&make_test_ctx(
            &dir,
            TestCtxOptions {
                present_tools: &[("nono", "0.3.0")],
                ..Default::default()
            },
        ));
        assert_eq!(
            sha256_hex(&std::fs::read_to_string(dir.join(SANDBOX_POLICY_PATH)).unwrap()),
            first_policy
        );
        assert_eq!(
            sha256_hex(&std::fs::read_to_string(dir.join(CLAUDE_SETTINGS_PATH)).unwrap()),
            first_settings
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn options_allow_hosts_extends_the_network_allowlist_without_displacing_defaults() {
        let dir = make_temp_dir("sandbox-hosts");
        let mut config = test_config_with(&[], &["claude-code", "codex"]);
        config.modules.get_mut("sandbox").unwrap().options =
            serde_json::json!({ "allowHosts": ["internal.example.test"] });
        MODULE.apply(&make_test_ctx(
            &dir,
            TestCtxOptions {
                config: Some(config),
                present_tools: &[("nono", "0.3.0")],
                ..Default::default()
            },
        ));
        let policy = read_policy(&dir);
        assert_eq!(policy["network"]["default"], serde_json::json!("deny"));
        let allowlist = policy["network"]["allowlist"].as_array().unwrap();
        for host in DEFAULT_NETWORK_ALLOWLIST {
            assert!(
                allowlist.iter().any(|entry| entry.as_str() == Some(host)),
                "missing default host {host}"
            );
        }
        assert!(allowlist
            .iter()
            .any(|entry| entry.as_str() == Some("internal.example.test")));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn options_malformed_option_arrays_are_rejected_apply_fails_nothing_written() {
        let dir = make_temp_dir("sandbox-badopt");
        let mut config = test_config_with(&[], &["claude-code", "codex"]);
        config.modules.get_mut("sandbox").unwrap().options =
            serde_json::json!({ "allowHosts": "not-an-array" });
        let result = MODULE.apply(&make_test_ctx(
            &dir,
            TestCtxOptions {
                config: Some(config),
                ..Default::default()
            },
        ));
        assert_eq!(result.status, ModuleStatus::Failed);
        assert!(result
            .findings
            .iter()
            .any(|finding| finding.level == FindingLevel::Error));
        assert!(!dir.join(SANDBOX_POLICY_PATH).exists());
        assert_eq!(
            validate_sandbox_options(&serde_json::json!({ "denyRead": [""] })).len(),
            1
        );
        assert_eq!(
            validate_sandbox_options(&serde_json::json!({ "allowHosts": ["ok.example"] })),
            Vec::<String>::new()
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn apply_refuses_to_clobber_unparseable_user_settings_and_degrades() {
        let dir = make_temp_dir("sandbox-clobber");
        write_file(&dir, CLAUDE_SETTINGS_PATH, "{not json");
        let ctx = make_test_ctx(
            &dir,
            TestCtxOptions {
                present_tools: &[("nono", "0.3.0")],
                ..Default::default()
            },
        );
        let result = MODULE.apply(&ctx);
        assert_eq!(result.status, ModuleStatus::Degraded);
        assert_eq!(
            std::fs::read_to_string(dir.join(CLAUDE_SETTINGS_PATH)).unwrap(),
            "{not json"
        );
        assert!(dir.join(SANDBOX_POLICY_PATH).exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn verify_passes_after_apply_fails_when_network_default_tampered_to_allow() {
        let dir = make_temp_dir("sandbox-vnet");
        let ctx = make_test_ctx(
            &dir,
            TestCtxOptions {
                present_tools: &[("nono", "0.3.0")],
                ..Default::default()
            },
        );
        MODULE.apply(&ctx);
        assert!(MODULE.verify(&ctx).ok);

        let mut policy = read_policy(&dir);
        policy["network"]["default"] = serde_json::json!("allow");
        write_file(
            &dir,
            SANDBOX_POLICY_PATH,
            &serde_json::to_string(&policy).unwrap(),
        );
        let tampered = MODULE.verify(&ctx);
        assert!(!tampered.ok);
        assert!(tampered
            .findings
            .iter()
            .any(|finding| finding.level == FindingLevel::Error));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn verify_fails_when_policy_missing_or_a_claude_deny_entry_is_removed() {
        let dir = make_temp_dir("sandbox-vdeny");
        let ctx = make_test_ctx(
            &dir,
            TestCtxOptions {
                present_tools: &[("nono", "0.3.0")],
                ..Default::default()
            },
        );
        assert!(!MODULE.verify(&ctx).ok);

        MODULE.apply(&ctx);
        let mut settings: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(dir.join(CLAUDE_SETTINGS_PATH)).unwrap())
                .unwrap();
        let filtered: Vec<serde_json::Value> = settings["permissions"]["deny"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|entry| entry.as_str() != Some("Read(~/.ssh/**)"))
            .cloned()
            .collect();
        settings["permissions"]["deny"] = serde_json::Value::Array(filtered);
        write_file(
            &dir,
            CLAUDE_SETTINGS_PATH,
            &serde_json::to_string(&settings).unwrap(),
        );
        let result = MODULE.verify(&ctx);
        assert!(!result.ok);
        assert!(result
            .findings
            .iter()
            .any(|finding| finding.message.contains("Read(~/.ssh/**)")));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn instruction_block_tells_harnesses_to_stay_inside_the_sandbox_and_escalate() {
        let blocks = MODULE.instruction_blocks();
        assert!(!blocks.is_empty());
        let block = &blocks[0];
        assert!(block.content.contains(".ade/policy/sandbox.json"));
        assert!(block
            .content
            .to_lowercase()
            .contains("never attempt to read credential files"));
        assert!(block.content.to_lowercase().contains("ask a human"));
        assert!(block.content.to_lowercase().contains("bypass"));
    }
}
