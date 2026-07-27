//! Module: cost & token budget governance — port of `src/modules/cost-governance.ts`.
//! Spec component: "Cost and token budget governance — model routing controls,
//! per-session and per-project token/cost limits, budget alerts, and policies
//! to prevent runaway harness activity and uncontrolled spend."
//! Boundary controlled: the spend boundary (model routing + budget contract).

use crate::modules::shared::{read_json, verify_json_artifact, write_policy};
use crate::types::{
    ActionKind, AdeModule, Ctx, Finding, FindingLevel, InstructionBlock, ModuleResult,
    ModuleStatus, PlannedAction, VerifyResult,
};
use serde_json::json;

pub const BUDGET_POLICY_PATH: &str = ".ade/policy/budget.json";

const LIMIT_KEYS: [&str; 4] = [
    "perSessionTokens",
    "perProjectDailyTokens",
    "perSessionCostUsd",
    "perProjectDailyCostUsd",
];

fn default_policy() -> serde_json::Value {
    json!({
        "schemaVersion": 1,
        "limits": {
            "perSessionTokens": 2_000_000,
            "perProjectDailyTokens": 10_000_000,
            "perSessionCostUsd": 25,
            "perProjectDailyCostUsd": 100,
        },
        "alerts": { "warnAtFraction": 0.8 },
        "routing": {
            "default": "balanced",
            "cheap": "small-model for mechanical edits, summaries, and classification",
            "escalation": "large-model only for architecture, security, and cross-cutting design",
            "policy": "route to the cheapest model that meets the task's quality bar",
        },
    })
}

/// Validate numeric budget options; returns errors (empty = valid).
pub fn validate_budget_options(options: &serde_json::Value) -> Vec<String> {
    let mut errors: Vec<String> = Vec::new();
    for key in LIMIT_KEYS {
        if let Some(value) = options.get(key) {
            let valid = value
                .as_f64()
                .map(|number| number.is_finite() && number > 0.0)
                .unwrap_or(false);
            if !valid {
                errors.push(format!("options.{key} must be a positive number"));
            }
        }
    }
    if let Some(warn) = options.get("warnAtFraction") {
        let valid = warn
            .as_f64()
            .map(|number| number > 0.0 && number < 1.0)
            .unwrap_or(false);
        if !valid {
            errors.push("options.warnAtFraction must be a number between 0 and 1".to_string());
        }
    }
    errors
}

fn build_policy(options: &serde_json::Value) -> serde_json::Value {
    let mut policy = default_policy();
    for key in LIMIT_KEYS {
        if let Some(value) = options.get(key) {
            if value.is_number() {
                policy["limits"][key] = value.clone();
            }
        }
    }
    if let Some(warn) = options.get("warnAtFraction") {
        if warn.is_number() {
            policy["alerts"]["warnAtFraction"] = warn.clone();
        }
    }
    policy
}

fn module_options(ctx: &Ctx) -> serde_json::Value {
    ctx.module_options("cost-governance")
}

fn apply_inner(ctx: &Ctx) -> std::io::Result<ModuleResult> {
    let options = module_options(ctx);
    let errors = validate_budget_options(&options);
    if !errors.is_empty() {
        return Ok(ModuleResult {
            status: ModuleStatus::Failed,
            findings: errors.into_iter().map(Finding::error).collect(),
            wrote_paths: vec![],
        });
    }
    write_policy(ctx, BUDGET_POLICY_PATH, &build_policy(&options))?;
    Ok(ModuleResult {
        status: ModuleStatus::Applied,
        findings: vec![Finding::ok(format!("wrote {BUDGET_POLICY_PATH}"))],
        wrote_paths: vec![BUDGET_POLICY_PATH.to_string()],
    })
}

pub struct CostGovernanceModule;

pub static MODULE: CostGovernanceModule = CostGovernanceModule;

impl AdeModule for CostGovernanceModule {
    fn id(&self) -> &'static str {
        "cost-governance"
    }
    fn title(&self) -> &'static str {
        "Cost & Token Budget Governance"
    }
    fn category(&self) -> &'static str {
        "governance"
    }
    fn spec(&self) -> &'static str {
        "Cost and token budget governance"
    }

    fn instruction_blocks(&self) -> Vec<InstructionBlock> {
        vec![InstructionBlock {
            id: "cost-governance",
            title: "Cost & Token Budget",
            content: concat!(
                "This project has a token/cost budget contract at `.ade/policy/budget.json`.\n",
                "- Prefer the cheapest model that meets the task's quality bar; escalate model tier only for architecture, security, or cross-cutting design work.\n",
                "- Avoid re-reading large files you have already read; use the context artifacts in `.ade/context/` first.\n",
                "- Stop and surface a budget warning instead of looping when a task repeatedly fails the same way.",
            )
            .to_string(),
        }]
    }

    fn detect(&self, ctx: &Ctx) -> Vec<Finding> {
        let errors = validate_budget_options(&module_options(ctx));
        if !errors.is_empty() {
            return errors.into_iter().map(Finding::error).collect();
        }
        vec![Finding::ok("budget options valid")]
    }

    fn plan(&self, _ctx: &Ctx) -> Vec<PlannedAction> {
        vec![PlannedAction {
            kind: ActionKind::Write,
            path: Some(BUDGET_POLICY_PATH.to_string()),
            description: "write token/cost budget policy (limits, alerts, model routing)"
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
        let artifact = verify_json_artifact(ctx, BUDGET_POLICY_PATH);
        if artifact.level != FindingLevel::Ok {
            return VerifyResult {
                ok: false,
                findings: vec![artifact],
            };
        }
        let Some(parsed) = read_json(ctx, BUDGET_POLICY_PATH) else {
            return VerifyResult {
                ok: false,
                findings: vec![artifact],
            };
        };
        let values: Vec<&serde_json::Value> = parsed
            .get("limits")
            .and_then(|limits| limits.as_object())
            .map(|map| map.values().collect())
            .unwrap_or_default();
        let numbers_valid = values.len() == 4
            && values
                .iter()
                .all(|value| value.as_f64().map(|number| number > 0.0).unwrap_or(false));
        if !numbers_valid {
            return VerifyResult {
                ok: false,
                findings: vec![Finding::error_with(
                    format!("{BUDGET_POLICY_PATH} contains non-positive or missing limits"),
                    "run `ade apply` to regenerate",
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
    use crate::testutil::{make_temp_dir, make_test_ctx, test_config, TestCtxOptions};
    use serde_json::json;

    fn read_policy(dir: &std::path::Path) -> serde_json::Value {
        let text = std::fs::read_to_string(dir.join(BUDGET_POLICY_PATH)).unwrap();
        serde_json::from_str(&text).unwrap()
    }

    fn config_with_options(options: serde_json::Value) -> crate::types::AdeConfig {
        let mut config = test_config();
        config
            .modules
            .get_mut("cost-governance")
            .expect("cost-governance module config")
            .options = options;
        config
    }

    #[test]
    fn isc_104_apply_writes_budget_policy_with_limits_alerts_and_model_routing() {
        let dir = make_temp_dir("cost-governance");
        let ctx = make_test_ctx(&dir, TestCtxOptions::default());
        let result = MODULE.apply(&ctx);
        assert_eq!(result.status, ModuleStatus::Applied);
        assert!(result.wrote_paths.contains(&BUDGET_POLICY_PATH.to_string()));
        let policy = read_policy(&dir);
        assert!(policy["limits"]["perSessionTokens"].as_f64().unwrap() > 0.0);
        assert!(policy["limits"]["perProjectDailyCostUsd"].as_f64().unwrap() > 0.0);
        assert!(policy["alerts"]["warnAtFraction"].as_f64().unwrap() > 0.0);
        assert!(policy["routing"]["policy"]
            .as_str()
            .unwrap()
            .contains("cheapest"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn isc_105_instruction_block_carries_the_budget_contract_to_harnesses() {
        let blocks = MODULE.instruction_blocks();
        assert!(!blocks.is_empty());
        assert!(blocks[0].content.contains(".ade/policy/budget.json"));
    }

    #[test]
    fn isc_106_malformed_budget_options_are_rejected_apply_fails_nothing_written() {
        let dir = make_temp_dir("cost-governance");
        let config = config_with_options(json!({ "perSessionTokens": -5 }));
        let ctx = make_test_ctx(
            &dir,
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
            .any(|finding| finding.level == FindingLevel::Error));
        assert!(!dir.join(BUDGET_POLICY_PATH).exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn isc_106_validate_budget_options_catches_every_malformed_numeric() {
        assert_eq!(validate_budget_options(&json!({})), Vec::<String>::new());
        assert_eq!(
            validate_budget_options(&json!({ "perSessionTokens": 100 })),
            Vec::<String>::new()
        );
        assert_eq!(
            validate_budget_options(&json!({ "perSessionTokens": 0 })).len(),
            1
        );
        assert_eq!(
            validate_budget_options(&json!({ "perSessionCostUsd": "many" })).len(),
            1
        );
        assert_eq!(
            validate_budget_options(&json!({ "warnAtFraction": 1.5 })).len(),
            1
        );
    }

    #[test]
    fn isc_116_plan_writes_nothing_to_disk() {
        let dir = make_temp_dir("cost-governance");
        let ctx = make_test_ctx(&dir, TestCtxOptions::default());
        let actions = MODULE.plan(&ctx);
        assert!(!actions.is_empty());
        assert!(!dir.join(BUDGET_POLICY_PATH).exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn isc_117_apply_is_idempotent_second_run_byte_identical() {
        let dir = make_temp_dir("cost-governance");
        let ctx = make_test_ctx(&dir, TestCtxOptions::default());
        MODULE.apply(&ctx);
        let first = sha256_hex(&std::fs::read_to_string(dir.join(BUDGET_POLICY_PATH)).unwrap());
        MODULE.apply(&make_test_ctx(&dir, TestCtxOptions::default()));
        let second = sha256_hex(&std::fs::read_to_string(dir.join(BUDGET_POLICY_PATH)).unwrap());
        assert_eq!(second, first);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn isc_33_options_override_default_limits() {
        let dir = make_temp_dir("cost-governance");
        let config = config_with_options(json!({ "perSessionCostUsd": 5 }));
        let ctx = make_test_ctx(
            &dir,
            TestCtxOptions {
                config: Some(config),
                ..Default::default()
            },
        );
        MODULE.apply(&ctx);
        let policy = read_policy(&dir);
        assert_eq!(policy["limits"]["perSessionCostUsd"], json!(5));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn verify_passes_after_apply_and_fails_on_corrupted_policy() {
        let dir = make_temp_dir("cost-governance");
        let ctx = make_test_ctx(&dir, TestCtxOptions::default());
        MODULE.apply(&ctx);
        assert!(MODULE.verify(&ctx).ok);

        std::fs::write(dir.join(BUDGET_POLICY_PATH), "{not json").unwrap();
        assert!(!MODULE.verify(&ctx).ok);

        std::fs::write(
            dir.join(BUDGET_POLICY_PATH),
            serde_json::to_string(&json!({ "limits": { "perSessionTokens": -1 } })).unwrap(),
        )
        .unwrap();
        assert!(!MODULE.verify(&ctx).ok);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn detect_reports_option_validity() {
        let dir = make_temp_dir("cost-governance");
        let ok = MODULE.detect(&make_test_ctx(&dir, TestCtxOptions::default()));
        assert!(ok.iter().all(|finding| finding.level == FindingLevel::Ok));
        let bad_config = config_with_options(json!({ "perSessionTokens": -1 }));
        let bad = MODULE.detect(&make_test_ctx(
            &dir,
            TestCtxOptions {
                config: Some(bad_config),
                ..Default::default()
            },
        ));
        assert!(bad
            .iter()
            .any(|finding| finding.level == FindingLevel::Error));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
