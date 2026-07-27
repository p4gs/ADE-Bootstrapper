//! Module: network-syncable agent memory — port of `src/modules/memory.ts`.
//! Spec component: "Network-syncable agent memory — persistent, portable agent
//! memory via the OpenMemory/Mem0 MCP server, local-first by default with
//! user-controlled sync."
//! Boundary controlled: the memory/data boundary (sensitive context leaving
//! the machine). Memory content is sensitive user data: the store is always
//! git-ignored, and the credential-bearing MCP integration is strictly OPT-IN
//! (`options.enableMcp = true`) — the default apply never touches `.mcp.json`.

use crate::fsutil::{ensure_lines, read_if_exists};
use crate::harness::claude::register_mcp_server;
use crate::modules::shared::{read_json, verify_json_artifact, write_policy};
use crate::types::{
    ActionKind, AdeModule, Ctx, Finding, FindingLevel, InstructionBlock, ModuleResult,
    ModuleStatus, PlannedAction, VerifyResult,
};

pub const MEMORY_POLICY_PATH: &str = ".ade/memory.json";
pub const MEMORY_STORE_PATH: &str = ".ade/memory-store/";
pub const MCP_SERVER_NAME: &str = "openmemory";

const ACTIVATION_INSTRUCTIONS: &str = "to enable the OpenMemory MCP server, set modules.memory.options.enableMcp = true in ade.json, then run `ade apply`";

fn memory_policy() -> serde_json::Value {
    serde_json::json!({
        "schemaVersion": 1,
        "provider": "openmemory",
        "project": "mem0.ai/openmemory",
        "posture": "local-first",
        "storePath": MEMORY_STORE_PATH,
        "sensitivity": {
            "classification": "sensitive-user-data",
            "rule": "memory content is never committed to version control and never leaves the machine without explicit user-configured sync",
            "gitIgnored": true,
        },
        "sync": "the memory server runs locally; network sync is user-controlled and off until the user configures it",
        "activation": { "mcp": ACTIVATION_INSTRUCTIONS },
    })
}

fn mcp_enabled(ctx: &Ctx) -> bool {
    ctx.config
        .harnesses
        .iter()
        .any(|harness| harness == "claude-code")
        && ctx.module_options("memory").get("enableMcp") == Some(&serde_json::Value::Bool(true))
}

fn apply_inner(ctx: &Ctx) -> std::io::Result<ModuleResult> {
    let mut findings: Vec<Finding> = Vec::new();
    let mut wrote_paths: Vec<String> = Vec::new();

    write_policy(ctx, MEMORY_POLICY_PATH, &memory_policy())?;
    wrote_paths.push(MEMORY_POLICY_PATH.to_string());
    findings.push(Finding::ok(format!("wrote {MEMORY_POLICY_PATH}")));

    ensure_lines(
        &ctx.target_dir.join(".gitignore"),
        &[MEMORY_STORE_PATH],
        "ADE Bootstrapper — agent memory (sensitive, never committed)",
    )?;
    findings.push(Finding::ok(format!(
        ".gitignore covers {MEMORY_STORE_PATH}"
    )));

    if mcp_enabled(ctx) {
        let registration = register_mcp_server(
            ctx,
            MCP_SERVER_NAME,
            &serde_json::json!({
                "command": "npx",
                "args": ["-y", "openmemory"],
                "env": {},
            }),
        )?;
        let level = registration.level;
        findings.push(registration);
        if level == FindingLevel::Error {
            return Ok(ModuleResult {
                status: ModuleStatus::Degraded,
                findings,
                wrote_paths,
            });
        }
        if level == FindingLevel::Ok {
            wrote_paths.push(".mcp.json".to_string());
        }
    } else {
        findings.push(Finding {
            level: FindingLevel::Info,
            message:
                "OpenMemory MCP server NOT registered — credential-bearing integrations are opt-in; .mcp.json untouched"
                    .to_string(),
            remediation: Some(ACTIVATION_INSTRUCTIONS.to_string()),
        });
    }

    Ok(ModuleResult {
        status: ModuleStatus::Applied,
        findings,
        wrote_paths,
    })
}

pub struct MemoryModule;
pub static MODULE: MemoryModule = MemoryModule;

impl AdeModule for MemoryModule {
    fn id(&self) -> &'static str {
        "memory"
    }
    fn title(&self) -> &'static str {
        "Network-Syncable Agent Memory"
    }
    fn category(&self) -> &'static str {
        "context"
    }
    fn spec(&self) -> &'static str {
        "Network-syncable agent memory"
    }

    fn instruction_blocks(&self) -> Vec<InstructionBlock> {
        vec![InstructionBlock {
            id: "memory",
            title: "Agent Memory",
            content: [
                "Persistent agent memory lives in the OpenMemory MCP server when activated (see `.ade/memory.json`).",
                "- Memory content is sensitive user data — the store (`.ade/memory-store/`) is git-ignored; never commit it or copy it into tracked files.",
                "- NEVER write secrets, credentials, or tokens into memory.",
                "- Memory is local-first; do not configure network sync on the user's behalf — sync is user-controlled.",
            ]
            .join("\n"),
        }]
    }

    fn detect(&self, ctx: &Ctx) -> Vec<Finding> {
        if mcp_enabled(ctx) {
            return vec![Finding::ok(
                "OpenMemory MCP integration enabled (options.enableMcp = true, claude-code harness targeted)",
            )];
        }
        vec![Finding {
            level: FindingLevel::Info,
            message:
                "OpenMemory MCP integration is opt-in and currently OFF — memory policy is documentation-only"
                    .to_string(),
            remediation: Some(ACTIVATION_INSTRUCTIONS.to_string()),
        }]
    }

    fn plan(&self, ctx: &Ctx) -> Vec<PlannedAction> {
        let mut actions = vec![
            PlannedAction {
                kind: ActionKind::Write,
                path: Some(MEMORY_POLICY_PATH.to_string()),
                description:
                    "write agent memory policy (openmemory provider, local-first posture, sensitivity rules)"
                        .to_string(),
            },
            PlannedAction {
                kind: ActionKind::Append,
                path: Some(".gitignore".to_string()),
                description: format!("ensure the memory store ({MEMORY_STORE_PATH}) is git-ignored"),
            },
        ];
        if mcp_enabled(ctx) {
            actions.push(PlannedAction {
                kind: ActionKind::Merge,
                path: Some(".mcp.json".to_string()),
                description: format!(
                    "register the \"{MCP_SERVER_NAME}\" MCP server (opt-in, preserves user entries)"
                ),
            });
        } else {
            actions.push(PlannedAction {
                kind: ActionKind::Info,
                path: None,
                description:
                    "OpenMemory MCP registration skipped — opt-in via options.enableMcp (.mcp.json untouched)"
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
        let mut findings = vec![verify_json_artifact(ctx, MEMORY_POLICY_PATH)];
        let mut ok = findings
            .iter()
            .all(|finding| finding.level == FindingLevel::Ok);

        if ok {
            let parsed = read_json(ctx, MEMORY_POLICY_PATH);
            let contract_ok = parsed
                .as_ref()
                .map(|policy| {
                    policy.get("provider") == Some(&serde_json::json!("openmemory"))
                        && policy.get("posture") == Some(&serde_json::json!("local-first"))
                        && policy.get("storePath") == Some(&serde_json::json!(MEMORY_STORE_PATH))
                })
                .unwrap_or(false);
            if !contract_ok {
                ok = false;
                findings.push(Finding::error_with(
                    format!(
                        "{MEMORY_POLICY_PATH} does not declare the openmemory/local-first contract"
                    ),
                    "run `ade apply` to regenerate",
                ));
            }
        }

        let gitignore = read_if_exists(&ctx.target_dir.join(".gitignore")).unwrap_or_default();
        let covered = gitignore
            .split('\n')
            .map(|line| line.trim())
            .any(|line| line == MEMORY_STORE_PATH);
        if !covered {
            ok = false;
            findings.push(Finding::error_with(
                format!(".gitignore does not cover {MEMORY_STORE_PATH} — memory content could be committed"),
                "run `ade apply`",
            ));
        } else {
            findings.push(Finding::ok(format!(
                ".gitignore covers {MEMORY_STORE_PATH}"
            )));
        }

        VerifyResult { ok, findings }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fsutil::{sha256_hex, write_ensured};
    use crate::testutil::{make_temp_dir, make_test_ctx, test_config_with, TestCtxOptions};
    use crate::types::AdeConfig;
    use std::collections::BTreeMap;
    use std::path::Path;

    fn config_with_mcp(enable_mcp: Option<serde_json::Value>, harnesses: &[&str]) -> AdeConfig {
        let mut config = test_config_with(&[], harnesses);
        if let Some(value) = enable_mcp {
            if let Some(module) = config.modules.get_mut("memory") {
                module.options = serde_json::json!({ "enableMcp": value });
            }
        }
        config
    }

    fn ctx_with_mcp(dir: &Path, enable_mcp: Option<serde_json::Value>) -> crate::types::Ctx {
        ctx_with_mcp_harnesses(dir, enable_mcp, &["claude-code", "codex"])
    }

    fn ctx_with_mcp_harnesses(
        dir: &Path,
        enable_mcp: Option<serde_json::Value>,
        harnesses: &[&str],
    ) -> crate::types::Ctx {
        make_test_ctx(
            dir,
            TestCtxOptions {
                config: Some(config_with_mcp(enable_mcp, harnesses)),
                ..Default::default()
            },
        )
    }

    fn read_artifact(dir: &Path, rel: &str) -> Option<String> {
        read_if_exists(&dir.join(rel))
    }

    #[test]
    fn isc_78_apply_writes_memory_json_with_provider_posture_store_path_and_activation() {
        let dir = make_temp_dir("mem-policy");
        let result = MODULE.apply(&ctx_with_mcp(&dir, None));
        assert_eq!(result.status, ModuleStatus::Applied);
        assert!(result.wrote_paths.contains(&MEMORY_POLICY_PATH.to_string()));
        let policy: serde_json::Value =
            serde_json::from_str(&read_artifact(&dir, MEMORY_POLICY_PATH).unwrap()).unwrap();
        assert_eq!(policy["provider"], serde_json::json!("openmemory"));
        assert_eq!(policy["project"], serde_json::json!("mem0.ai/openmemory"));
        assert_eq!(policy["posture"], serde_json::json!("local-first"));
        assert_eq!(policy["storePath"], serde_json::json!(MEMORY_STORE_PATH));
        let activation = policy["activation"]["mcp"].as_str().unwrap();
        assert!(activation.contains("enableMcp"));
        assert!(activation.contains("ade.json"));
        assert!(activation.contains("ade apply"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn isc_79_enable_mcp_true_with_claude_code_harness_registers_openmemory_via_npx() {
        let dir = make_temp_dir("mem-mcp-on");
        let result = MODULE.apply(&ctx_with_mcp(&dir, Some(serde_json::json!(true))));
        assert_eq!(result.status, ModuleStatus::Applied);
        assert!(result.wrote_paths.contains(&".mcp.json".to_string()));
        let mcp: serde_json::Value =
            serde_json::from_str(&read_artifact(&dir, ".mcp.json").unwrap()).unwrap();
        let server = &mcp["mcpServers"][MCP_SERVER_NAME];
        assert_eq!(server["command"], serde_json::json!("npx"));
        assert_eq!(server["args"], serde_json::json!(["-y", "openmemory"]));
        assert_eq!(server["env"], serde_json::json!({}));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn isc_79_default_enable_mcp_absent_info_finding_with_activation_mcp_json_not_touched() {
        let dir = make_temp_dir("mem-mcp-off");
        let result = MODULE.apply(&ctx_with_mcp(&dir, None));
        assert_eq!(result.status, ModuleStatus::Applied);
        assert!(read_artifact(&dir, ".mcp.json").is_none());
        assert!(!result.wrote_paths.contains(&".mcp.json".to_string()));
        let info = result
            .findings
            .iter()
            .find(|finding| {
                finding.level == FindingLevel::Info && finding.message.contains("NOT registered")
            })
            .expect("info finding present");
        assert!(info.remediation.as_deref().unwrap().contains("enableMcp"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn isc_79_enable_mcp_false_mcp_json_not_touched() {
        let dir = make_temp_dir("mem-mcp-false");
        MODULE.apply(&ctx_with_mcp(&dir, Some(serde_json::json!(false))));
        assert!(read_artifact(&dir, ".mcp.json").is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn isc_79_enable_mcp_true_without_claude_code_harness_mcp_json_not_touched() {
        let dir = make_temp_dir("mem-mcp-no-claude");
        let result = MODULE.apply(&ctx_with_mcp_harnesses(
            &dir,
            Some(serde_json::json!(true)),
            &["codex"],
        ));
        assert_eq!(result.status, ModuleStatus::Applied);
        assert!(read_artifact(&dir, ".mcp.json").is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn isc_79_pre_existing_user_openmemory_entry_in_mcp_json_is_preserved_untouched() {
        let dir = make_temp_dir("mem-preserve");
        let user_server = serde_json::json!({
            "command": "/usr/local/bin/my-openmemory",
            "args": ["--custom"],
            "env": { "PORT": "9999" },
        });
        write_ensured(
            &dir.join(".mcp.json"),
            &serde_json::to_string_pretty(
                &serde_json::json!({ "mcpServers": { "openmemory": user_server } }),
            )
            .unwrap(),
        )
        .unwrap();
        let result = MODULE.apply(&ctx_with_mcp(&dir, Some(serde_json::json!(true))));
        assert_eq!(result.status, ModuleStatus::Applied);
        let mcp: serde_json::Value =
            serde_json::from_str(&read_artifact(&dir, ".mcp.json").unwrap()).unwrap();
        assert_eq!(mcp["mcpServers"][MCP_SERVER_NAME], user_server);
        assert!(result.findings.iter().any(|finding| {
            finding.level == FindingLevel::Info && finding.message.contains("preserved")
        }));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn isc_79_invalid_user_mcp_json_degraded_with_fix_remediation_file_not_clobbered() {
        let dir = make_temp_dir("mem-broken");
        let broken = "{ not json !!!";
        write_ensured(&dir.join(".mcp.json"), broken).unwrap();
        let result = MODULE.apply(&ctx_with_mcp(&dir, Some(serde_json::json!(true))));
        assert_eq!(result.status, ModuleStatus::Degraded);
        assert!(result.findings.iter().any(|finding| {
            finding.level == FindingLevel::Error && finding.message.contains(".mcp.json")
        }));
        assert_eq!(read_artifact(&dir, ".mcp.json").as_deref(), Some(broken));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn isc_80_apply_git_ignores_memory_store_idempotently_preserving_user_content() {
        let dir = make_temp_dir("mem-gitignore");
        write_ensured(&dir.join(".gitignore"), "node_modules/\n").unwrap();
        MODULE.apply(&ctx_with_mcp(&dir, None));
        let first = read_artifact(&dir, ".gitignore").unwrap();
        assert!(first.contains("node_modules/"));
        assert!(first
            .split('\n')
            .map(|line| line.trim())
            .any(|line| line == MEMORY_STORE_PATH));
        MODULE.apply(&ctx_with_mcp(&dir, None));
        assert_eq!(read_artifact(&dir, ".gitignore").unwrap(), first);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn isc_80_policy_declares_memory_as_sensitive_and_never_committed() {
        let dir = make_temp_dir("mem-sensitive");
        MODULE.apply(&ctx_with_mcp(&dir, None));
        let policy: serde_json::Value =
            serde_json::from_str(&read_artifact(&dir, MEMORY_POLICY_PATH).unwrap()).unwrap();
        assert_eq!(policy["sensitivity"]["gitIgnored"], serde_json::json!(true));
        assert!(policy["sensitivity"]["rule"]
            .as_str()
            .unwrap()
            .contains("never committed"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn instruction_block_marks_memory_sensitive_and_forbids_secrets_in_memory() {
        let blocks = MODULE.instruction_blocks();
        let block = &blocks[0];
        assert!(block.content.contains("OpenMemory MCP server"));
        assert!(block.content.to_lowercase().contains("never write secrets"));
        assert!(block.content.to_lowercase().contains("sensitive user data"));
    }

    #[test]
    fn plan_writes_nothing_to_disk() {
        let dir = make_temp_dir("mem-plan");
        let ctx = ctx_with_mcp(&dir, Some(serde_json::json!(true)));
        let actions = MODULE.plan(&ctx);
        assert!(actions
            .iter()
            .any(|action| action.path.as_deref() == Some(MEMORY_POLICY_PATH)));
        assert!(actions.iter().any(|action| action.kind == ActionKind::Merge
            && action.path.as_deref() == Some(".mcp.json")));
        assert!(read_artifact(&dir, MEMORY_POLICY_PATH).is_none());
        assert!(read_artifact(&dir, ".mcp.json").is_none());
        assert!(read_artifact(&dir, ".gitignore").is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn plan_without_enable_mcp_reports_mcp_registration_as_skipped_info_no_mcp_json_action() {
        let dir = make_temp_dir("mem-plan-off");
        let actions = MODULE.plan(&ctx_with_mcp(&dir, None));
        assert!(!actions
            .iter()
            .any(|action| action.path.as_deref() == Some(".mcp.json")));
        assert!(
            actions
                .iter()
                .any(|action| action.kind == ActionKind::Info
                    && action.description.contains("opt-in"))
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn double_apply_is_byte_identical_for_memory_json_mcp_json_gitignore() {
        let dir = make_temp_dir("mem-idem");
        MODULE.apply(&ctx_with_mcp(&dir, Some(serde_json::json!(true))));
        let hashes = |dir: &Path| -> Vec<String> {
            [MEMORY_POLICY_PATH, ".mcp.json", ".gitignore"]
                .iter()
                .map(|rel| sha256_hex(&read_artifact(dir, rel).unwrap()))
                .collect()
        };
        let first = hashes(&dir);
        MODULE.apply(&ctx_with_mcp(&dir, Some(serde_json::json!(true))));
        assert_eq!(hashes(&dir), first);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn verify_passes_after_apply() {
        let dir = make_temp_dir("mem-verify");
        let ctx = ctx_with_mcp(&dir, None);
        MODULE.apply(&ctx);
        let result = MODULE.verify(&ctx);
        assert!(result.ok);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn verify_fails_when_memory_json_is_tampered_into_invalid_json() {
        let dir = make_temp_dir("mem-tamper");
        let ctx = ctx_with_mcp(&dir, None);
        MODULE.apply(&ctx);
        std::fs::write(dir.join(MEMORY_POLICY_PATH), "{ tampered").unwrap();
        let result = MODULE.verify(&ctx);
        assert!(!result.ok);
        assert!(result
            .findings
            .iter()
            .any(|finding| finding.level == FindingLevel::Error));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn verify_fails_when_policy_no_longer_declares_local_first_openmemory_contract() {
        let dir = make_temp_dir("mem-contract");
        let ctx = ctx_with_mcp(&dir, None);
        MODULE.apply(&ctx);
        std::fs::write(
            dir.join(MEMORY_POLICY_PATH),
            serde_json::to_string(&serde_json::json!({
                "provider": "other",
                "posture": "cloud",
                "storePath": "/elsewhere",
            }))
            .unwrap(),
        )
        .unwrap();
        let result = MODULE.verify(&ctx);
        assert!(!result.ok);
        assert!(result
            .findings
            .iter()
            .any(|finding| finding.message.contains("local-first")));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn verify_fails_when_gitignore_no_longer_covers_memory_store() {
        let dir = make_temp_dir("mem-uncovered");
        let ctx = ctx_with_mcp(&dir, None);
        MODULE.apply(&ctx);
        std::fs::write(dir.join(".gitignore"), "node_modules/\n").unwrap();
        let result = MODULE.verify(&ctx);
        assert!(!result.ok);
        assert!(result.findings.iter().any(|finding| {
            finding.level == FindingLevel::Error && finding.message.contains(MEMORY_STORE_PATH)
        }));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn detect_reports_opt_in_status_off_by_default_on_when_enabled() {
        let dir = make_temp_dir("mem-detect");
        let off = MODULE.detect(&ctx_with_mcp(&dir, None));
        assert!(off.iter().any(|finding| finding.level == FindingLevel::Info
            && finding
                .remediation
                .as_deref()
                .map(|remediation| remediation.contains("enableMcp"))
                .unwrap_or(false)));
        let on = MODULE.detect(&ctx_with_mcp(&dir, Some(serde_json::json!(true))));
        assert!(on.iter().any(
            |finding| finding.level == FindingLevel::Ok && finding.message.contains("enabled")
        ));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn no_environment_variable_values_leak_into_generated_artifacts() {
        let dir = make_temp_dir("mem-env");
        let planted = "PLANTED_MEMORY_TEST_SECRET_98765";
        let env: BTreeMap<String, String> =
            [("OPENMEMORY_API_KEY".to_string(), planted.to_string())]
                .into_iter()
                .collect();
        let ctx = make_test_ctx(
            &dir,
            TestCtxOptions {
                config: Some(config_with_mcp(
                    Some(serde_json::json!(true)),
                    &["claude-code", "codex"],
                )),
                env,
                ..Default::default()
            },
        );
        MODULE.apply(&ctx);
        for rel in [MEMORY_POLICY_PATH, ".mcp.json", ".gitignore"] {
            if let Some(text) = read_artifact(&dir, rel) {
                assert!(!text.contains(planted));
            }
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}
