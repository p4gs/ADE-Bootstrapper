//! Module: human-in-the-loop approval gates — port of `src/modules/approval-gates.ts`.
//!
//! Spec component: "Human-in-the-loop approval gates — explicit human approval
//! requirements for high-risk actions (destructive shell commands, credential
//! use, external network access, dependency installs, branch operations, PR
//! creation, merges, and production-affecting changes)."
//! Boundary controlled: the action-authorization boundary — no high-risk action
//! executes without an explicit human decision, and production-affecting
//! changes are denied outright absent one.
//!
//! The policy is enforced at two layers: a machine-readable contract at
//! `.ade/policy/approvals.json` (consumed by harnesses and tooling), and — for
//! Claude Code — native permission deny/ask rules merged additively into
//! `.claude/settings.json` (user entries are always preserved).

use crate::modules::shared::{read_json, verify_json_artifact, write_policy};
use crate::types::{
    ActionKind, AdeModule, Ctx, Finding, FindingLevel, InstructionBlock, ModuleResult,
    ModuleStatus, PlannedAction, VerifyResult,
};
use serde_json::json;

pub const APPROVALS_POLICY_PATH: &str = ".ade/policy/approvals.json";

pub use crate::harness::claude::CLAUDE_SETTINGS_PATH;

/// The eight gated action classes — the policy enumerates EXACTLY these.
pub const ACTION_CLASSES: [&str; 8] = [
    "destructiveShell",
    "credentialUse",
    "externalNetwork",
    "dependencyInstall",
    "branchOps",
    "prCreation",
    "merge",
    "productionAffecting",
];

/// Classes that can NEVER be configured to 'allow' (ISC-126 anti-criterion, enforced at the writer).
pub const NEVER_ALLOW: [&str; 3] = ["destructiveShell", "credentialUse", "productionAffecting"];

const DECISIONS: [&str; 3] = ["ask", "allow", "deny"];

/// Secure defaults: everything asks; production-affecting changes are denied.
fn default_decision(cls: &str) -> &'static str {
    match cls {
        "destructiveShell" => "ask",
        "credentialUse" => "ask",
        "externalNetwork" => "ask",
        "dependencyInstall" => "ask",
        "branchOps" => "ask",
        "prCreation" => "ask",
        "merge" => "ask",
        "productionAffecting" => "deny",
        _ => "ask",
    }
}

fn rationale(cls: &str) -> &'static str {
    match cls {
        "destructiveShell" => {
            "destructive shell commands can irreversibly delete work or system state"
        }
        "credentialUse" => {
            "credential use can exfiltrate or misuse secrets beyond the task's scope"
        }
        "externalNetwork" => {
            "external network access can leak repository content or fetch untrusted code"
        }
        "dependencyInstall" => {
            "installing dependencies executes third-party code inside the project"
        }
        "branchOps" => "branch deletion and force operations can discard unreviewed history",
        "prCreation" => {
            "opening a PR publishes work product and triggers CI under the human's identity"
        }
        "merge" => "merging lands changes on shared branches other collaborators build on",
        "productionAffecting" => {
            "production-affecting changes have blast radius beyond the repository"
        }
        _ => "",
    }
}

fn examples(cls: &str) -> serde_json::Value {
    match cls {
        "destructiveShell" => {
            json!([
                "rm -rf",
                "git reset --hard",
                "sudo rm",
                "dd of=/dev/…",
                "DROP TABLE"
            ])
        }
        "credentialUse" => json!([
            "reading .env values",
            "using AWS/GH tokens",
            "authenticating to external services"
        ]),
        "externalNetwork" => json!([
            "curl/fetch to non-allowlisted hosts",
            "uploading files",
            "calling third-party APIs"
        ]),
        "dependencyInstall" => json!([
            "npm install <pkg>",
            "bun add <pkg>",
            "pip install <pkg>",
            "cargo add <pkg>"
        ]),
        "branchOps" => {
            json!([
                "git branch -D",
                "git push --force",
                "rewriting published history"
            ])
        }
        "prCreation" => json!(["gh pr create", "opening a merge request"]),
        "merge" => json!([
            "gh pr merge",
            "git merge into main",
            "clicking the merge button"
        ]),
        "productionAffecting" => json!([
            "deploys",
            "database migrations",
            "infra changes",
            "editing prod config or feature flags"
        ]),
        _ => json!([]),
    }
}

/// Claude Code permission rules mapped from the action classes.
pub const CLAUDE_DENY_RULES: [&str; 3] = [
    "Bash(rm -rf /:*)",
    "Bash(git push --force:*)",
    "Bash(sudo rm:*)",
];

pub const CLAUDE_ASK_RULES: [&str; 6] = [
    "Bash(rm -rf:*)",
    "Bash(git push:*)",
    "Bash(npm install:*)",
    "Bash(bun install:*)",
    "Bash(pip install:*)",
    "Bash(git branch -D:*)",
];

/// Validate decision overrides; returns errors (empty = valid). The never-allow
/// trio (destructiveShell, credentialUse, productionAffecting) is rejected at
/// this writer if any option attempts to set it to 'allow'.
pub fn validate_approval_options(options: &serde_json::Value) -> Vec<String> {
    let mut errors: Vec<String> = Vec::new();
    for cls in ACTION_CLASSES {
        let Some(value) = options.get(cls) else {
            continue;
        };
        match value.as_str() {
            Some(decision) if DECISIONS.contains(&decision) => {
                if decision == "allow" && NEVER_ALLOW.contains(&cls) {
                    errors.push(format!(
                        "options.{cls} can never be \"allow\" — this class always requires a human decision"
                    ));
                }
            }
            _ => errors.push(format!(
                "options.{cls} must be one of \"ask\" | \"allow\" | \"deny\""
            )),
        }
    }
    errors
}

fn build_policy(options: &serde_json::Value) -> serde_json::Value {
    let mut actions = serde_json::Map::new();
    for cls in ACTION_CLASSES {
        let decision = match options.get(cls).and_then(|value| value.as_str()) {
            Some(value) if DECISIONS.contains(&value) => value,
            _ => default_decision(cls),
        };
        actions.insert(
            cls.to_string(),
            json!({
                "decision": decision,
                "rationale": rationale(cls),
                "examples": examples(cls),
            }),
        );
    }
    json!({
        "schemaVersion": 1,
        "neverAllow": NEVER_ALLOW,
        "actions": actions,
    })
}

fn module_options(ctx: &Ctx) -> serde_json::Value {
    ctx.module_options("approval-gates")
}

fn targets_claude_code(ctx: &Ctx) -> bool {
    ctx.config
        .harnesses
        .iter()
        .any(|harness| harness == "claude-code")
}

/// Merge permission (or other) settings into `.claude/settings.json`.
/// Additive merge — user entries always preserved; an unparseable user file is
use crate::harness::claude::merge_claude_settings;

fn apply_inner(ctx: &Ctx) -> std::io::Result<ModuleResult> {
    let options = module_options(ctx);
    let errors = validate_approval_options(&options);
    if !errors.is_empty() {
        return Ok(ModuleResult {
            status: ModuleStatus::Failed,
            findings: errors.into_iter().map(Finding::error).collect(),
            wrote_paths: vec![],
        });
    }

    let mut findings: Vec<Finding> = Vec::new();
    let mut wrote_paths: Vec<String> = Vec::new();

    write_policy(ctx, APPROVALS_POLICY_PATH, &build_policy(&options))?;
    wrote_paths.push(APPROVALS_POLICY_PATH.to_string());
    findings.push(Finding::ok(format!("wrote {APPROVALS_POLICY_PATH}")));

    let mut degraded = false;
    if targets_claude_code(ctx) {
        let merge = merge_claude_settings(
            ctx,
            &json!({
                "permissions": { "deny": CLAUDE_DENY_RULES, "ask": CLAUDE_ASK_RULES },
            }),
        )?;
        let merge_ok = merge.level == FindingLevel::Ok;
        findings.push(merge);
        if merge_ok {
            wrote_paths.push(CLAUDE_SETTINGS_PATH.to_string());
        } else {
            degraded = true;
        }
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

pub struct ApprovalGatesModule;
pub static MODULE: ApprovalGatesModule = ApprovalGatesModule;

impl AdeModule for ApprovalGatesModule {
    fn id(&self) -> &'static str {
        "approval-gates"
    }
    fn title(&self) -> &'static str {
        "Human-in-the-Loop Approval Gates"
    }
    fn category(&self) -> &'static str {
        "security"
    }
    fn spec(&self) -> &'static str {
        "Human-in-the-loop approval gates"
    }

    fn instruction_blocks(&self) -> Vec<InstructionBlock> {
        vec![InstructionBlock {
            id: "approval-gates",
            title: "Human Approval Gates",
            content: r#"This project gates high-risk actions behind explicit human approval (`.ade/policy/approvals.json`).
- Eight action classes REQUIRE explicit human approval before execution: destructive shell commands, credential use, external network access, dependency installs, branch operations, PR creation, merges, and production-affecting changes.
- Never execute an action in these classes on your own authority; state what you intend to do and wait for the human's decision.
- When in doubt whether an action falls into a gated class, ASK — treat ambiguity as gated.
- Production-affecting changes are DENIED without a human decision; there is no default-approve path for them."#
                .to_string(),
        }]
    }

    fn detect(&self, ctx: &Ctx) -> Vec<Finding> {
        let errors = validate_approval_options(&module_options(ctx));
        if !errors.is_empty() {
            return errors.into_iter().map(Finding::error).collect();
        }
        vec![Finding::ok("approval-gate options valid")]
    }

    fn plan(&self, ctx: &Ctx) -> Vec<PlannedAction> {
        let mut actions = vec![PlannedAction {
            kind: ActionKind::Write,
            path: Some(APPROVALS_POLICY_PATH.to_string()),
            description: "write approval-gate policy (eight gated action classes with decisions)"
                .to_string(),
        }];
        if targets_claude_code(ctx) {
            actions.push(PlannedAction {
                kind: ActionKind::Merge,
                path: Some(CLAUDE_SETTINGS_PATH.to_string()),
                description:
                    "merge deny/ask permission rules for high-risk commands into Claude Code settings"
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
        let artifact = verify_json_artifact(ctx, APPROVALS_POLICY_PATH);
        let artifact_ok = artifact.level == FindingLevel::Ok;
        findings.push(artifact);
        if !artifact_ok {
            return VerifyResult {
                ok: false,
                findings,
            };
        }

        let mut ok = true;
        let policy = read_json(ctx, APPROVALS_POLICY_PATH);
        let empty = json!({});
        let actions = policy
            .as_ref()
            .and_then(|value| value.get("actions"))
            .unwrap_or(&empty);

        let missing: Vec<&str> = ACTION_CLASSES
            .iter()
            .copied()
            .filter(|cls| {
                let Some(entry) = actions.get(*cls) else {
                    return true;
                };
                let decision_ok = entry
                    .get("decision")
                    .and_then(|value| value.as_str())
                    .map(|decision| DECISIONS.contains(&decision))
                    .unwrap_or(false);
                let rationale_ok = entry
                    .get("rationale")
                    .map(|value| value.is_string())
                    .unwrap_or(false);
                let examples_ok = entry
                    .get("examples")
                    .map(|value| value.is_array())
                    .unwrap_or(false);
                !(decision_ok && rationale_ok && examples_ok)
            })
            .collect();
        if !missing.is_empty() {
            ok = false;
            findings.push(Finding::error_with(
                format!(
                    "{APPROVALS_POLICY_PATH} missing or malformed action classes: {}",
                    missing.join(", ")
                ),
                "run `ade apply` to regenerate",
            ));
        } else {
            findings.push(Finding::ok("all eight gated action classes present"));
        }

        let escaped: Vec<&str> = NEVER_ALLOW
            .iter()
            .copied()
            .filter(|cls| {
                actions
                    .get(*cls)
                    .and_then(|entry| entry.get("decision"))
                    .and_then(|value| value.as_str())
                    .map(|decision| decision == "allow")
                    .unwrap_or(false)
            })
            .collect();
        if !escaped.is_empty() {
            ok = false;
            findings.push(Finding::error_with(
                format!(
                    "{APPROVALS_POLICY_PATH} sets never-allow classes to \"allow\": {}",
                    escaped.join(", ")
                ),
                "run `ade apply` to restore the secure decisions",
            ));
        }

        if targets_claude_code(ctx) {
            let settings = read_json(ctx, CLAUDE_SETTINGS_PATH);
            let empty_list: Vec<serde_json::Value> = Vec::new();
            let deny = settings
                .as_ref()
                .and_then(|value| value.get("permissions"))
                .and_then(|value| value.get("deny"))
                .and_then(|value| value.as_array())
                .unwrap_or(&empty_list);
            let ask = settings
                .as_ref()
                .and_then(|value| value.get("permissions"))
                .and_then(|value| value.get("ask"))
                .and_then(|value| value.as_array())
                .unwrap_or(&empty_list);
            let absent: Vec<&str> = CLAUDE_DENY_RULES
                .iter()
                .copied()
                .filter(|rule| !deny.iter().any(|entry| entry == rule))
                .chain(
                    CLAUDE_ASK_RULES
                        .iter()
                        .copied()
                        .filter(|rule| !ask.iter().any(|entry| entry == rule)),
                )
                .collect();
            if !absent.is_empty() {
                ok = false;
                findings.push(Finding::error_with(
                    format!(
                        "{CLAUDE_SETTINGS_PATH} missing approval permission rules: {}",
                        absent.join(", ")
                    ),
                    "run `ade apply` to re-merge the deny/ask rules",
                ));
            } else {
                findings.push(Finding::ok("Claude Code deny/ask permission rules present"));
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
    use crate::types::AdeConfig;
    use std::fs;
    use std::path::Path;

    fn config_with_options(harnesses: &[&str], options: serde_json::Value) -> AdeConfig {
        let mut config = test_config_with(&[], harnesses);
        config
            .modules
            .get_mut("approval-gates")
            .expect("approval-gates module config")
            .options = options;
        config
    }

    fn read_policy(dir: &Path) -> serde_json::Value {
        let text = fs::read_to_string(dir.join(APPROVALS_POLICY_PATH)).expect("read policy");
        serde_json::from_str(&text).expect("parse policy")
    }

    #[test]
    fn isc_89_apply_writes_approvals_json_enumerating_exactly_the_eight_action_classes() {
        let dir = make_temp_dir("ag-89");
        let ctx = make_test_ctx(
            &dir,
            TestCtxOptions {
                config: Some(test_config_with(&[], &["codex"])),
                ..Default::default()
            },
        );
        let result = MODULE.apply(&ctx);
        assert_eq!(result.status, ModuleStatus::Applied);
        assert!(result
            .wrote_paths
            .contains(&APPROVALS_POLICY_PATH.to_string()));
        let policy = read_policy(&dir);
        let mut keys: Vec<String> = policy["actions"]
            .as_object()
            .unwrap()
            .keys()
            .cloned()
            .collect();
        keys.sort();
        let mut expected: Vec<String> = ACTION_CLASSES.iter().map(|c| c.to_string()).collect();
        expected.sort();
        assert_eq!(keys, expected);
        for cls in ACTION_CLASSES {
            let entry = &policy["actions"][cls];
            let decision = entry["decision"].as_str().unwrap();
            assert!(["ask", "allow", "deny"].contains(&decision));
            assert!(!entry["rationale"].as_str().unwrap().is_empty());
            let examples = entry["examples"].as_array().unwrap();
            assert!(!examples.is_empty());
        }
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn isc_90_secure_defaults_are_exactly_ask_x7_plus_production_affecting_deny() {
        let dir = make_temp_dir("ag-90d");
        let ctx = make_test_ctx(
            &dir,
            TestCtxOptions {
                config: Some(test_config_with(&[], &["codex"])),
                ..Default::default()
            },
        );
        MODULE.apply(&ctx);
        let policy = read_policy(&dir);
        assert_eq!(policy["actions"]["productionAffecting"]["decision"], "deny");
        for cls in ACTION_CLASSES
            .iter()
            .filter(|c| **c != "productionAffecting")
        {
            assert_eq!(policy["actions"][*cls]["decision"], "ask");
        }
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn isc_90_options_override_decisions_for_non_protected_classes() {
        let dir = make_temp_dir("ag-90o");
        let config = config_with_options(
            &["codex"],
            json!({ "externalNetwork": "allow", "merge": "deny" }),
        );
        let ctx = make_test_ctx(
            &dir,
            TestCtxOptions {
                config: Some(config),
                ..Default::default()
            },
        );
        let result = MODULE.apply(&ctx);
        assert_eq!(result.status, ModuleStatus::Applied);
        let policy = read_policy(&dir);
        assert_eq!(policy["actions"]["externalNetwork"]["decision"], "allow");
        assert_eq!(policy["actions"]["merge"]["decision"], "deny");
        assert_eq!(policy["actions"]["destructiveShell"]["decision"], "ask");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn isc_90_never_allow_classes_cannot_become_allow_apply_fails_nothing_written() {
        for cls in NEVER_ALLOW {
            let scratch = make_temp_dir("ag-90na");
            let mut options = serde_json::Map::new();
            options.insert(cls.to_string(), json!("allow"));
            let config = config_with_options(
                &["claude-code", "codex"],
                serde_json::Value::Object(options),
            );
            let ctx = make_test_ctx(
                &scratch,
                TestCtxOptions {
                    config: Some(config),
                    ..Default::default()
                },
            );
            let result = MODULE.apply(&ctx);
            assert_eq!(result.status, ModuleStatus::Failed);
            assert!(result
                .findings
                .iter()
                .any(|f| f.level == FindingLevel::Error && f.message.contains(cls)));
            assert!(!scratch.join(APPROVALS_POLICY_PATH).exists());
            let _ = fs::remove_dir_all(&scratch);
        }
    }

    #[test]
    fn isc_90_validate_approval_options_rejects_never_allow_escalation_and_malformed_decisions() {
        assert!(validate_approval_options(&json!({})).is_empty());
        assert!(
            validate_approval_options(&json!({ "merge": "allow", "branchOps": "deny" })).is_empty()
        );
        assert_eq!(
            validate_approval_options(&json!({ "destructiveShell": "allow" })).len(),
            1
        );
        assert_eq!(
            validate_approval_options(&json!({ "credentialUse": "allow" })).len(),
            1
        );
        assert_eq!(
            validate_approval_options(&json!({ "productionAffecting": "allow" })).len(),
            1
        );
        // never-allow classes may still be tightened
        assert!(validate_approval_options(&json!({ "destructiveShell": "deny" })).is_empty());
        assert_eq!(
            validate_approval_options(&json!({ "merge": "yolo" })).len(),
            1
        );
        assert_eq!(validate_approval_options(&json!({ "merge": 42 })).len(), 1);
    }

    #[test]
    fn isc_91_claude_code_targeted_deny_ask_permission_rules_merged_into_settings() {
        let dir = make_temp_dir("ag-91m");
        let ctx = make_test_ctx(
            &dir,
            TestCtxOptions {
                config: Some(test_config_with(&[], &["claude-code"])),
                ..Default::default()
            },
        );
        let result = MODULE.apply(&ctx);
        assert_eq!(result.status, ModuleStatus::Applied);
        assert!(result
            .wrote_paths
            .contains(&CLAUDE_SETTINGS_PATH.to_string()));
        let settings: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(dir.join(CLAUDE_SETTINGS_PATH)).unwrap())
                .unwrap();
        let deny = settings["permissions"]["deny"].as_array().unwrap();
        let ask = settings["permissions"]["ask"].as_array().unwrap();
        for rule in CLAUDE_DENY_RULES {
            assert!(deny.iter().any(|entry| entry == rule));
        }
        for rule in CLAUDE_ASK_RULES {
            assert!(ask.iter().any(|entry| entry == rule));
        }
        assert!(deny.iter().any(|entry| entry == "Bash(rm -rf /:*)"));
        assert!(ask.iter().any(|entry| entry == "Bash(git push:*)"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn isc_91_pre_seeded_user_deny_entry_is_preserved_after_merge() {
        let dir = make_temp_dir("ag-91p");
        fs::create_dir_all(dir.join(".claude")).unwrap();
        fs::write(
            dir.join(CLAUDE_SETTINGS_PATH),
            serde_json::to_string(
                &json!({ "permissions": { "deny": ["Bash(curl:*)"] }, "model": "user-choice" }),
            )
            .unwrap(),
        )
        .unwrap();
        let ctx = make_test_ctx(
            &dir,
            TestCtxOptions {
                config: Some(test_config_with(&[], &["claude-code"])),
                ..Default::default()
            },
        );
        MODULE.apply(&ctx);
        let settings: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(dir.join(CLAUDE_SETTINGS_PATH)).unwrap())
                .unwrap();
        let deny = settings["permissions"]["deny"].as_array().unwrap();
        assert!(deny.iter().any(|entry| entry == "Bash(curl:*)"));
        assert_eq!(settings["model"], "user-choice");
        for rule in CLAUDE_DENY_RULES {
            assert!(deny.iter().any(|entry| entry == rule));
        }
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn isc_91_non_claude_harness_settings_not_written() {
        let dir = make_temp_dir("ag-91n");
        let ctx = make_test_ctx(
            &dir,
            TestCtxOptions {
                config: Some(test_config_with(&[], &["codex"])),
                ..Default::default()
            },
        );
        let result = MODULE.apply(&ctx);
        assert!(!result
            .wrote_paths
            .contains(&CLAUDE_SETTINGS_PATH.to_string()));
        assert!(!dir.join(CLAUDE_SETTINGS_PATH).exists());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn isc_91_unparseable_user_settings_degraded_user_file_never_clobbered() {
        let dir = make_temp_dir("ag-91u");
        let broken = "{not json";
        fs::create_dir_all(dir.join(".claude")).unwrap();
        fs::write(dir.join(CLAUDE_SETTINGS_PATH), broken).unwrap();
        let ctx = make_test_ctx(
            &dir,
            TestCtxOptions {
                config: Some(test_config_with(&[], &["claude-code"])),
                ..Default::default()
            },
        );
        let result = MODULE.apply(&ctx);
        assert_eq!(result.status, ModuleStatus::Degraded);
        assert!(result
            .findings
            .iter()
            .any(|f| f.level == FindingLevel::Error));
        assert_eq!(
            fs::read_to_string(dir.join(CLAUDE_SETTINGS_PATH)).unwrap(),
            broken
        );
        // the policy itself still lands
        assert!(dir.join(APPROVALS_POLICY_PATH).exists());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn isc_92_instruction_block_requires_human_approval_and_denies_production_changes() {
        let blocks = MODULE.instruction_blocks();
        assert!(!blocks.is_empty());
        let block = &blocks[0];
        assert!(block.content.contains("REQUIRE explicit human approval"));
        assert!(block.content.to_lowercase().contains("when in doubt"));
        assert!(block.content.contains("DENIED without a human decision"));
        for phrase in [
            "destructive shell",
            "credential use",
            "external network",
            "dependency installs",
            "branch operations",
            "PR creation",
            "merges",
            "production-affecting",
        ] {
            assert!(block.content.contains(phrase), "missing phrase: {phrase}");
        }
    }

    #[test]
    fn isc_116_plan_writes_nothing_to_disk() {
        let dir = make_temp_dir("ag-116");
        let ctx = make_test_ctx(
            &dir,
            TestCtxOptions {
                config: Some(test_config_with(&[], &["claude-code"])),
                ..Default::default()
            },
        );
        let actions = MODULE.plan(&ctx);
        assert_eq!(actions.len(), 2);
        assert!(actions.iter().any(|action| action.kind == ActionKind::Merge
            && action.path.as_deref() == Some(CLAUDE_SETTINGS_PATH)));
        assert!(!dir.join(APPROVALS_POLICY_PATH).exists());
        assert!(!dir.join(CLAUDE_SETTINGS_PATH).exists());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn isc_117_apply_is_idempotent_second_run_byte_identical() {
        let dir = make_temp_dir("ag-117");
        let config = test_config_with(&[], &["claude-code"]);
        MODULE.apply(&make_test_ctx(
            &dir,
            TestCtxOptions {
                config: Some(config.clone()),
                ..Default::default()
            },
        ));
        let first_policy =
            sha256_hex(&fs::read_to_string(dir.join(APPROVALS_POLICY_PATH)).unwrap());
        let first_settings =
            sha256_hex(&fs::read_to_string(dir.join(CLAUDE_SETTINGS_PATH)).unwrap());
        MODULE.apply(&make_test_ctx(
            &dir,
            TestCtxOptions {
                config: Some(config),
                ..Default::default()
            },
        ));
        assert_eq!(
            sha256_hex(&fs::read_to_string(dir.join(APPROVALS_POLICY_PATH)).unwrap()),
            first_policy
        );
        assert_eq!(
            sha256_hex(&fs::read_to_string(dir.join(CLAUDE_SETTINGS_PATH)).unwrap()),
            first_settings
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn verify_passes_after_apply_claude_code_plus_policy_checks() {
        let dir = make_temp_dir("ag-vp");
        let ctx = make_test_ctx(
            &dir,
            TestCtxOptions {
                config: Some(test_config_with(&[], &["claude-code"])),
                ..Default::default()
            },
        );
        MODULE.apply(&ctx);
        let result = MODULE.verify(&ctx);
        assert!(result.ok);
        assert!(result
            .findings
            .iter()
            .any(|f| f.message.contains("eight gated action classes")));
        assert!(result
            .findings
            .iter()
            .any(|f| f.message.contains("deny/ask permission rules")));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn verify_fails_when_policy_missing_or_invalid_json() {
        let dir = make_temp_dir("ag-vm");
        let ctx = make_test_ctx(
            &dir,
            TestCtxOptions {
                config: Some(test_config_with(&[], &["codex"])),
                ..Default::default()
            },
        );
        assert!(!MODULE.verify(&ctx).ok);
        fs::create_dir_all(dir.join(".ade/policy")).unwrap();
        fs::write(dir.join(APPROVALS_POLICY_PATH), "{not json").unwrap();
        assert!(!MODULE.verify(&ctx).ok);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn verify_fails_when_a_never_allow_class_was_tampered_to_allow() {
        let dir = make_temp_dir("ag-vt");
        let ctx = make_test_ctx(
            &dir,
            TestCtxOptions {
                config: Some(test_config_with(&[], &["codex"])),
                ..Default::default()
            },
        );
        MODULE.apply(&ctx);
        let mut policy = read_policy(&dir);
        policy["actions"]["productionAffecting"]["decision"] = json!("allow");
        fs::write(
            dir.join(APPROVALS_POLICY_PATH),
            serde_json::to_string(&policy).unwrap(),
        )
        .unwrap();
        let result = MODULE.verify(&ctx);
        assert!(!result.ok);
        assert!(result
            .findings
            .iter()
            .any(|f| f.message.contains("productionAffecting")));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn verify_fails_when_an_action_class_was_removed_from_the_policy() {
        let dir = make_temp_dir("ag-vr");
        let ctx = make_test_ctx(
            &dir,
            TestCtxOptions {
                config: Some(test_config_with(&[], &["codex"])),
                ..Default::default()
            },
        );
        MODULE.apply(&ctx);
        let mut policy = read_policy(&dir);
        policy["actions"]
            .as_object_mut()
            .unwrap()
            .remove("credentialUse");
        fs::write(
            dir.join(APPROVALS_POLICY_PATH),
            serde_json::to_string(&policy).unwrap(),
        )
        .unwrap();
        let result = MODULE.verify(&ctx);
        assert!(!result.ok);
        assert!(result
            .findings
            .iter()
            .any(|f| f.message.contains("credentialUse")));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn verify_fails_when_claude_settings_lose_a_deny_rule_after_apply() {
        let dir = make_temp_dir("ag-vd");
        let ctx = make_test_ctx(
            &dir,
            TestCtxOptions {
                config: Some(test_config_with(&[], &["claude-code"])),
                ..Default::default()
            },
        );
        MODULE.apply(&ctx);
        let mut settings: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(dir.join(CLAUDE_SETTINGS_PATH)).unwrap())
                .unwrap();
        let filtered: Vec<serde_json::Value> = settings["permissions"]["deny"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|rule| *rule != &json!("Bash(git push --force:*)"))
            .cloned()
            .collect();
        settings["permissions"]["deny"] = json!(filtered);
        fs::write(
            dir.join(CLAUDE_SETTINGS_PATH),
            serde_json::to_string(&settings).unwrap(),
        )
        .unwrap();
        let result = MODULE.verify(&ctx);
        assert!(!result.ok);
        assert!(result
            .findings
            .iter()
            .any(|f| f.message.contains("git push --force")));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn detect_reports_option_validity() {
        let dir = make_temp_dir("ag-det");
        let ok = MODULE.detect(&make_test_ctx(&dir, TestCtxOptions::default()));
        assert!(ok.iter().all(|f| f.level == FindingLevel::Ok));
        let bad = MODULE.detect(&make_test_ctx(
            &dir,
            TestCtxOptions {
                config: Some(config_with_options(
                    &["claude-code", "codex"],
                    json!({ "credentialUse": "allow" }),
                )),
                ..Default::default()
            },
        ));
        assert!(bad.iter().any(|f| f.level == FindingLevel::Error));
        let _ = fs::remove_dir_all(&dir);
    }
}
