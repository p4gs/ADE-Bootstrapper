//! Module: Git & Repository Hygiene — port of `src/modules/git-hygiene.ts`.
//!
//! Spec component: "Git and repository hygiene enforcement — protected-branch
//! policy (no force-push, no rewrite of pushed history, PR-only changes),
//! branch naming and commit-style conventions, commit-signing posture, and
//! deep repo hardening via OCEAN integration."
//! Boundary controlled: the git boundary (repo integrity).
//!
//! Integration over rebuild: OCEAN is the deep-hardening engine. This module
//! writes the machine-readable hygiene contract and points at
//! `ocean harden --tags baseline` for enforcement beyond the contract —
//! it never reimplements OCEAN's checks.

use crate::modules::shared::{read_json, tool_finding, verify_json_artifact, write_policy};
use crate::types::{
    ActionKind, AdeModule, Ctx, ExecOpts, Finding, FindingLevel, InstructionBlock, ModuleResult,
    ModuleStatus, PlannedAction, VerifyResult,
};
use serde_json::json;

pub const GIT_POLICY_PATH: &str = ".ade/policy/git.json";

fn git_policy() -> serde_json::Value {
    json!({
        "schemaVersion": 1,
        "protectedBranches": ["main", "master"],
        "forcePushToProtected": "deny",
        "requirePrForProtected": true,
        "historyRewriteOfPushed": "deny",
        "branchNaming": "type/short-kebab-description (feat/fix/chore/docs)",
        "commitStyle": "conventional",
    })
}

fn not_git_finding() -> Finding {
    Finding::degraded(
        "target is not a git repository — git hygiene enforcement unavailable",
        "run `git init` in the target, then `ade apply`",
    )
}

/// Standard finding for OCEAN presence: integrate when present, guide install when absent.
fn ocean_findings(ctx: &Ctx) -> Vec<Finding> {
    let mut findings = vec![tool_finding(
        ctx,
        "ocean",
        "install OCEAN for deep repo hardening — see github.com/grcengineering/OCEAN",
    )];
    if ctx.tool_present("ocean") {
        findings.push(Finding::ok(
            "OCEAN available — run `ocean harden --tags baseline` for deep repository hardening (integration, not reimplementation)",
        ));
    }
    findings
}

/// Check the repo's commit-signing configuration via `git config`.
fn signing_finding(ctx: &Ctx) -> Finding {
    let argv: Vec<String> = vec![
        "git".to_string(),
        "-C".to_string(),
        ctx.target_dir.display().to_string(),
        "config".to_string(),
        "--get".to_string(),
        "commit.gpgsign".to_string(),
    ];
    let result = (ctx.exec)(&argv, &ExecOpts::default());
    if result.code == 0 && result.stdout.trim() == "true" {
        return Finding::ok("commit signing enabled");
    }
    Finding {
        level: FindingLevel::Info,
        message: "commit signing not enabled".to_string(),
        remediation: Some(
            "enable with `git config commit.gpgsign true` and set `user.signingkey`".to_string(),
        ),
    }
}

fn apply_inner(ctx: &Ctx) -> std::io::Result<ModuleResult> {
    let mut findings: Vec<Finding> = Vec::new();
    let mut wrote_paths: Vec<String> = Vec::new();

    write_policy(ctx, GIT_POLICY_PATH, &git_policy())?;
    wrote_paths.push(GIT_POLICY_PATH.to_string());
    findings.push(Finding::ok(format!("wrote {GIT_POLICY_PATH}")));

    if !ctx.is_git_repo {
        findings.push(not_git_finding());
        return Ok(ModuleResult {
            status: ModuleStatus::Degraded,
            findings,
            wrote_paths,
        });
    }

    findings.extend(ocean_findings(ctx));
    if !ctx.tool_present("ocean") {
        return Ok(ModuleResult {
            status: ModuleStatus::Degraded,
            findings,
            wrote_paths,
        });
    }
    Ok(ModuleResult {
        status: ModuleStatus::Applied,
        findings,
        wrote_paths,
    })
}

pub struct GitHygieneModule;
pub static MODULE: GitHygieneModule = GitHygieneModule;

impl AdeModule for GitHygieneModule {
    fn id(&self) -> &'static str {
        "git-hygiene"
    }
    fn title(&self) -> &'static str {
        "Git & Repository Hygiene"
    }
    fn category(&self) -> &'static str {
        "security"
    }
    fn spec(&self) -> &'static str {
        "Git and repository hygiene enforcement"
    }

    fn instruction_blocks(&self) -> Vec<InstructionBlock> {
        vec![InstructionBlock {
            id: "git-hygiene",
            title: "Git & Repository Hygiene",
            content: r#"The repo integrity contract lives at `.ade/policy/git.json`.
- NEVER force-push protected branches (`main`, `master`).
- NEVER rewrite pushed history (no rebase/amend of commits that exist on a remote).
- Route all protected-branch changes through pull requests — never commit to them directly.
- Name branches `type/short-kebab-description` (types: feat/fix/chore/docs).
- Write conventional commit messages.
- NEVER delete branches you did not create without explicit approval."#
                .to_string(),
        }]
    }

    fn detect(&self, ctx: &Ctx) -> Vec<Finding> {
        let mut findings = ocean_findings(ctx);
        if !ctx.is_git_repo {
            findings.push(not_git_finding());
            return findings;
        }
        findings.push(signing_finding(ctx));
        findings
    }

    fn plan(&self, _ctx: &Ctx) -> Vec<PlannedAction> {
        vec![PlannedAction {
            kind: ActionKind::Write,
            path: Some(GIT_POLICY_PATH.to_string()),
            description:
                "write git hygiene policy (protected branches, force-push/rewrite denial, PR requirement, naming + commit conventions)"
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
        let mut findings: Vec<Finding> = vec![verify_json_artifact(ctx, GIT_POLICY_PATH)];
        let mut ok = findings
            .iter()
            .all(|finding| finding.level == FindingLevel::Ok);

        if ok {
            let parsed = read_json(ctx, GIT_POLICY_PATH);
            let branches_valid = parsed
                .as_ref()
                .and_then(|policy| policy.get("protectedBranches"))
                .and_then(|value| value.as_array())
                .map(|branches| !branches.is_empty())
                .unwrap_or(false);
            let force_push_denied = parsed
                .as_ref()
                .and_then(|policy| policy.get("forcePushToProtected"))
                .and_then(|value| value.as_str())
                .map(|decision| decision == "deny")
                .unwrap_or(false);
            if !branches_valid || !force_push_denied {
                ok = false;
                findings.push(Finding::error_with(
                    format!("{GIT_POLICY_PATH} must list protected branches and deny force-push to them"),
                    "run `ade apply` to regenerate",
                ));
            }
        }

        if !ctx.is_git_repo {
            findings.push(Finding::degraded(
                "not a git repository — git-boundary enforcement not verifiable",
                "run `git init`, then `ade apply`",
            ));
        }
        VerifyResult { ok, findings }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fsutil::sha256_hex;
    use crate::testutil::{fake_exec, make_temp_dir, make_test_ctx, TestCtxOptions};
    use std::collections::BTreeMap;
    use std::fs;

    #[test]
    fn isc_99_non_git_target_detect_degrades_with_git_init_guidance_no_crash() {
        let dir = make_temp_dir("gh-99d");
        let ctx = make_test_ctx(
            &dir,
            TestCtxOptions {
                is_git_repo: false,
                ..Default::default()
            },
        );
        let findings = MODULE.detect(&ctx);
        let degraded = findings.iter().find(|f| {
            f.level == FindingLevel::Degraded && f.message.contains("not a git repository")
        });
        assert!(degraded.is_some());
        assert!(degraded
            .unwrap()
            .remediation
            .as_deref()
            .map(|r| r.contains("git init"))
            .unwrap_or(false));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn isc_99_non_git_target_apply_degrades_never_failed_with_git_init_guidance() {
        let dir = make_temp_dir("gh-99a");
        let ctx = make_test_ctx(
            &dir,
            TestCtxOptions {
                is_git_repo: false,
                present_tools: &[("ocean", "ocean 1.0.0")],
                ..Default::default()
            },
        );
        let result = MODULE.apply(&ctx);
        assert_eq!(result.status, ModuleStatus::Degraded);
        assert!(result.findings.iter().any(|f| f
            .remediation
            .as_deref()
            .map(|r| r.contains("git init"))
            .unwrap_or(false)));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn isc_100_commit_signing_enabled_detect_reports_ok() {
        let dir = make_temp_dir("gh-100e");
        let ctx = make_test_ctx(
            &dir,
            TestCtxOptions {
                present_tools: &[("ocean", "ocean 1.0.0")],
                exec: Some(fake_exec(&[(
                    "config --get commit.gpgsign",
                    (0, "true\n", ""),
                )])),
                ..Default::default()
            },
        );
        let findings = MODULE.detect(&ctx);
        assert!(findings
            .iter()
            .any(|f| f.level == FindingLevel::Ok && f.message == "commit signing enabled"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn isc_100_commit_signing_not_configured_info_finding_with_enable_guidance() {
        let dir = make_temp_dir("gh-100n");
        let ctx = make_test_ctx(
            &dir,
            TestCtxOptions {
                present_tools: &[("ocean", "ocean 1.0.0")],
                // git config --get on an unset key exits 1 with empty stdout
                exec: Some(fake_exec(&[("config --get commit.gpgsign", (1, "", ""))])),
                ..Default::default()
            },
        );
        let findings = MODULE.detect(&ctx);
        let signing = findings
            .iter()
            .find(|f| f.message.contains("commit signing not enabled"));
        assert!(signing.is_some());
        let signing = signing.unwrap();
        assert_eq!(signing.level, FindingLevel::Info);
        let remediation = signing.remediation.as_deref().unwrap();
        assert!(remediation.contains("git config commit.gpgsign true"));
        assert!(remediation.contains("user.signingkey"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn isc_100_commit_gpgsign_explicitly_false_info_finding_not_ok() {
        let dir = make_temp_dir("gh-100f");
        let ctx = make_test_ctx(
            &dir,
            TestCtxOptions {
                present_tools: &[("ocean", "ocean 1.0.0")],
                exec: Some(fake_exec(&[(
                    "config --get commit.gpgsign",
                    (0, "false\n", ""),
                )])),
                ..Default::default()
            },
        );
        let findings = MODULE.detect(&ctx);
        assert!(!findings
            .iter()
            .any(|f| f.message == "commit signing enabled"));
        assert!(findings
            .iter()
            .any(|f| f.level == FindingLevel::Info && f.message.contains("commit signing")));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn isc_101_apply_writes_git_json_with_the_full_repo_integrity_contract() {
        let dir = make_temp_dir("gh-101");
        let ctx = make_test_ctx(
            &dir,
            TestCtxOptions {
                present_tools: &[("ocean", "ocean 1.0.0")],
                ..Default::default()
            },
        );
        let result = MODULE.apply(&ctx);
        assert_eq!(result.status, ModuleStatus::Applied);
        assert!(result.wrote_paths.contains(&GIT_POLICY_PATH.to_string()));
        let policy: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(dir.join(GIT_POLICY_PATH)).unwrap()).unwrap();
        assert_eq!(policy["protectedBranches"], json!(["main", "master"]));
        assert_eq!(policy["forcePushToProtected"], "deny");
        assert_eq!(policy["requirePrForProtected"], true);
        assert_eq!(policy["historyRewriteOfPushed"], "deny");
        assert_eq!(
            policy["branchNaming"],
            "type/short-kebab-description (feat/fix/chore/docs)"
        );
        assert_eq!(policy["commitStyle"], "conventional");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn isc_102_ocean_present_detect_recommends_ocean_harden_tags_baseline() {
        let dir = make_temp_dir("gh-102p");
        let ctx = make_test_ctx(
            &dir,
            TestCtxOptions {
                present_tools: &[("ocean", "ocean 1.0.0")],
                exec: Some(fake_exec(&[(
                    "config --get commit.gpgsign",
                    (0, "true\n", ""),
                )])),
                ..Default::default()
            },
        );
        let findings = MODULE.detect(&ctx);
        assert!(findings
            .iter()
            .any(|f| f.level == FindingLevel::Ok && f.message.contains("ocean present")));
        assert!(findings
            .iter()
            .any(|f| f.level == FindingLevel::Ok
                && f.message.contains("ocean harden --tags baseline")));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn isc_102_ocean_absent_degraded_finding_with_install_guidance_apply_degrades_not_fails() {
        let dir = make_temp_dir("gh-102a");
        let ctx = make_test_ctx(&dir, TestCtxOptions::default());
        let findings = MODULE.detect(&ctx);
        let ocean = findings
            .iter()
            .find(|f| f.message.contains("ocean not installed"));
        assert!(ocean.is_some());
        let ocean = ocean.unwrap();
        assert_eq!(ocean.level, FindingLevel::Degraded);
        assert!(ocean
            .remediation
            .as_deref()
            .map(|r| r.contains("github.com/grcengineering/OCEAN"))
            .unwrap_or(false));

        let result = MODULE.apply(&ctx);
        assert_eq!(result.status, ModuleStatus::Degraded);
        assert!(result.findings.iter().any(|f| f
            .remediation
            .as_deref()
            .map(|r| r.contains("github.com/grcengineering/OCEAN"))
            .unwrap_or(false)));
        // policy artifact is still written — degradation is about deep hardening, not the contract
        assert!(dir.join(GIT_POLICY_PATH).exists());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn isc_103_instruction_block_forbids_force_push_history_rewrite_and_mandates_conventions() {
        let blocks = MODULE.instruction_blocks();
        assert!(!blocks.is_empty());
        let block = &blocks[0];
        assert!(block.content.contains("NEVER force-push"));
        assert!(block.content.contains("NEVER rewrite pushed history"));
        assert!(block.content.contains("pull request"));
        assert!(block.content.contains("type/short-kebab-description"));
        assert!(block.content.to_lowercase().contains("conventional commit"));
        assert!(block
            .content
            .contains("NEVER delete branches you did not create"));
    }

    #[test]
    fn isc_116_plan_writes_nothing_to_disk() {
        let dir = make_temp_dir("gh-116");
        let ctx = make_test_ctx(
            &dir,
            TestCtxOptions {
                present_tools: &[("ocean", "ocean 1.0.0")],
                ..Default::default()
            },
        );
        let actions = MODULE.plan(&ctx);
        assert!(!actions.is_empty());
        assert!(actions.iter().any(|action| action.kind == ActionKind::Write
            && action.path.as_deref() == Some(GIT_POLICY_PATH)));
        assert!(!dir.join(GIT_POLICY_PATH).exists());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn isc_117_apply_is_idempotent_second_run_byte_identical() {
        let dir = make_temp_dir("gh-117");
        MODULE.apply(&make_test_ctx(
            &dir,
            TestCtxOptions {
                present_tools: &[("ocean", "ocean 1.0.0")],
                ..Default::default()
            },
        ));
        let first = sha256_hex(&fs::read_to_string(dir.join(GIT_POLICY_PATH)).unwrap());
        MODULE.apply(&make_test_ctx(
            &dir,
            TestCtxOptions {
                present_tools: &[("ocean", "ocean 1.0.0")],
                ..Default::default()
            },
        ));
        let second = sha256_hex(&fs::read_to_string(dir.join(GIT_POLICY_PATH)).unwrap());
        assert_eq!(second, first);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn verify_passes_after_apply_and_re_derives_the_contract_from_disk() {
        let dir = make_temp_dir("gh-vp");
        let ctx = make_test_ctx(
            &dir,
            TestCtxOptions {
                present_tools: &[("ocean", "ocean 1.0.0")],
                ..Default::default()
            },
        );
        MODULE.apply(&ctx);
        let result = MODULE.verify(&ctx);
        assert!(result.ok);
        assert!(result.findings.iter().any(|f| f.level == FindingLevel::Ok));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn verify_fails_on_missing_invalid_or_tampered_git_json() {
        let dir = make_temp_dir("gh-vf");
        let ctx = make_test_ctx(&dir, TestCtxOptions::default());
        assert!(!MODULE.verify(&ctx).ok);

        MODULE.apply(&ctx);
        fs::write(dir.join(GIT_POLICY_PATH), "{not json").unwrap();
        assert!(!MODULE.verify(&ctx).ok);

        // tampered: force-push allowed
        fs::write(
            dir.join(GIT_POLICY_PATH),
            serde_json::to_string(
                &json!({ "protectedBranches": ["main"], "forcePushToProtected": "allow" }),
            )
            .unwrap(),
        )
        .unwrap();
        assert!(!MODULE.verify(&ctx).ok);

        // tampered: no protected branches left
        fs::write(
            dir.join(GIT_POLICY_PATH),
            serde_json::to_string(
                &json!({ "protectedBranches": [], "forcePushToProtected": "deny" }),
            )
            .unwrap(),
        )
        .unwrap();
        assert!(!MODULE.verify(&ctx).ok);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn verify_degrades_not_failed_when_target_is_not_a_git_repo() {
        let dir = make_temp_dir("gh-vd");
        let ctx = make_test_ctx(
            &dir,
            TestCtxOptions {
                is_git_repo: false,
                ..Default::default()
            },
        );
        MODULE.apply(&ctx);
        let result = MODULE.verify(&ctx);
        assert!(result.ok);
        assert!(result
            .findings
            .iter()
            .any(|f| f.level == FindingLevel::Degraded));
        assert!(!result
            .findings
            .iter()
            .any(|f| f.level == FindingLevel::Error));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn generated_git_json_is_deterministic_no_timestamps_env_values_or_absolute_paths() {
        let dir = make_temp_dir("gh-det");
        let planted = "PLANTED_ENV_SECRET_VALUE_98765";
        let mut env = BTreeMap::new();
        env.insert("GITHUB_TOKEN".to_string(), planted.to_string());
        let ctx = make_test_ctx(
            &dir,
            TestCtxOptions {
                present_tools: &[("ocean", "ocean 1.0.0")],
                env,
                ..Default::default()
            },
        );
        MODULE.apply(&ctx);
        let text = fs::read_to_string(dir.join(GIT_POLICY_PATH)).unwrap();
        assert!(!text.contains(planted));
        assert!(!text.contains(&dir.display().to_string()));
        let _ = fs::remove_dir_all(&dir);
    }
}
