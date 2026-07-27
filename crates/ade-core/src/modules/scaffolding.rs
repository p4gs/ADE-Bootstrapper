//! Module: Quality & Performance Scaffolding — port of `src/modules/scaffolding.ts`.
//! Spec component: "Opinionated performance and quality scaffolding — PR
//! checklists, testing conventions, and commit conventions written into
//! `.ade/templates/` so every agent-authored change is held to the same
//! quality bar."
//! Boundary controlled: the quality boundary (inconsistent agent output) —
//! without shared conventions each harness session invents its own standards,
//! so review quality, test rigor, and commit hygiene drift run to run.

use crate::fsutil::read_if_exists;
use crate::types::{
    ActionKind, AdeModule, Ctx, Finding, InstructionBlock, ModuleResult, ModuleStatus,
    PlannedAction, VerifyResult,
};

pub const PR_CHECKLIST_PATH: &str = ".ade/templates/pr-checklist.md";
pub const TESTING_CONVENTIONS_PATH: &str = ".ade/templates/testing-conventions.md";
pub const COMMIT_CONVENTIONS_PATH: &str = ".ade/templates/commit-conventions.md";

pub const DEFAULT_COVERAGE_FLOOR_PCT: f64 = 95.0;

/// Template path → required h1 header, used by both apply and verify.
pub const TEMPLATE_HEADERS: [(&str, &str); 3] = [
    (PR_CHECKLIST_PATH, "# PR Checklist"),
    (TESTING_CONVENTIONS_PATH, "# Testing Conventions"),
    (COMMIT_CONVENTIONS_PATH, "# Commit Conventions"),
];

/// Lookup into [`TEMPLATE_HEADERS`] by template path.
pub fn template_header(rel_path: &str) -> Option<&'static str> {
    TEMPLATE_HEADERS
        .iter()
        .find(|(path, _)| *path == rel_path)
        .map(|(_, header)| *header)
}

/// Validate scaffolding options; returns errors (empty = valid).
pub fn validate_scaffolding_options(options: &serde_json::Value) -> Vec<String> {
    let mut errors: Vec<String> = Vec::new();
    if let Some(floor) = options.get("coverageFloorPct") {
        let valid = floor
            .as_f64()
            .map(|value| value.is_finite() && value > 0.0 && value <= 100.0)
            .unwrap_or(false);
        if !valid {
            errors.push(
                "options.coverageFloorPct must be a number between 0 (exclusive) and 100"
                    .to_string(),
            );
        }
    }
    errors
}

fn coverage_floor(options: &serde_json::Value) -> f64 {
    options
        .get("coverageFloorPct")
        .and_then(|value| value.as_f64())
        .unwrap_or(DEFAULT_COVERAGE_FLOOR_PCT)
}

pub fn pr_checklist_template(floor_pct: f64) -> String {
    format!(
        r#"# PR Checklist

Work through every item before requesting review. An unchecked item is a
blocker, not a suggestion.

## Tests
- [ ] Tests added or updated for every behavior change in this PR.
- [ ] Full test suite passes locally — paste the command and exit code in the PR description.
- [ ] Coverage floor met ({floor_pct}% line and function minimum) — the CI gate is not lowered.

## Security
- [ ] Security review done on every touched surface (inputs, auth, subprocess, file, and network boundaries).
- [ ] No secrets, tokens, or credentials in code, config, tests, or fixtures.
- [ ] No scanner findings suppressed — every true positive is fixed at the code level.

## Hygiene
- [ ] Docs updated for any changed behavior, options, or public interfaces.
- [ ] No generated artifacts, lockfile noise, or unrelated changes bundled in.
- [ ] PR description states WHAT changed, WHY, and how it was verified.
"#
    )
}

pub fn testing_conventions_template(floor_pct: f64) -> String {
    format!(
        r#"# Testing Conventions

These conventions define what "tested" means in this repository.

## Test-first
- Write the failing test BEFORE the behavior change; the test defines done.
- A bug fix starts with a regression test that reproduces the bug.

## Meaningful assertions only
- Every test asserts an observable outcome that maps to an intended use case.
- Coverage padding is a defect: no assertion-free tests, no tests that only
  execute a line without verifying its effect, no contrived inputs whose only
  purpose is touching a branch.
- If reachable code cannot be covered by a meaningful test, treat the code as
  suspect — fix or delete it rather than padding around it.

## Coverage
- The coverage floor ({floor_pct}% line and function) is a hard gate; never lower
  it to make a change pass — write the real test instead.

## Integration over mocks
- Prefer exercising real components (temp dirs, real files, real subprocess
  contracts) when the real thing is cheap; mock only true external boundaries.
"#
    )
}

pub fn commit_conventions_template() -> String {
    r#"# Commit Conventions

Every commit in this repository follows these rules.

## Format
- Conventional-commit style: `type(scope): subject` with types
  `feat`, `fix`, `docs`, `test`, `refactor`, `perf`, `chore`, `ci`, `build`.
- Subject is imperative mood ("add", not "added" or "adds") and at most 72 characters.
- Body explains WHY the change was made — the diff already shows what.

## Scope
- One logical change per commit; split unrelated changes into separate commits.
- Never mix a refactor with a behavior change in the same commit.

## Never commit
- Secrets, tokens, credentials, or private keys of any kind.
- Generated artifacts, build output, or local tooling state that belongs in
  `.gitignore`.
- Commented-out code or debugging leftovers.
"#
    .to_string()
}

fn module_options(ctx: &Ctx) -> serde_json::Value {
    ctx.module_options("scaffolding")
}

/// All three templates, rendered from options — the single source for apply.
fn render_templates(options: &serde_json::Value) -> [(&'static str, String); 3] {
    let floor = coverage_floor(options);
    [
        (PR_CHECKLIST_PATH, pr_checklist_template(floor)),
        (
            TESTING_CONVENTIONS_PATH,
            testing_conventions_template(floor),
        ),
        (COMMIT_CONVENTIONS_PATH, commit_conventions_template()),
    ]
}

fn apply_inner(ctx: &Ctx) -> std::io::Result<ModuleResult> {
    let options = module_options(ctx);
    let errors = validate_scaffolding_options(&options);
    if !errors.is_empty() {
        return Ok(ModuleResult {
            status: ModuleStatus::Failed,
            findings: errors.into_iter().map(Finding::error).collect(),
            wrote_paths: vec![],
        });
    }
    let templates = render_templates(&options);
    let mut wrote_paths: Vec<String> = Vec::new();
    for (rel_path, content) in &templates {
        ctx.artifacts.write(rel_path, content)?;
        wrote_paths.push((*rel_path).to_string());
    }
    Ok(ModuleResult {
        status: ModuleStatus::Applied,
        findings: vec![Finding::ok(format!(
            "wrote {} quality templates to .ade/templates/",
            wrote_paths.len()
        ))],
        wrote_paths,
    })
}

pub struct ScaffoldingModule;
pub static MODULE: ScaffoldingModule = ScaffoldingModule;

impl AdeModule for ScaffoldingModule {
    fn id(&self) -> &'static str {
        "scaffolding"
    }
    fn title(&self) -> &'static str {
        "Quality & Performance Scaffolding"
    }
    fn category(&self) -> &'static str {
        "governance"
    }
    fn spec(&self) -> &'static str {
        "Opinionated performance and quality scaffolding"
    }

    fn instruction_blocks(&self) -> Vec<InstructionBlock> {
        vec![InstructionBlock {
            id: "scaffolding",
            title: "Quality & Performance Scaffolding",
            content: [
                "This project ships quality conventions in `.ade/templates/` — follow them on every change.",
                "- Test-first: write the failing test before the behavior change; a bug fix starts with a regression test.",
                "- The coverage floor is a HARD gate — never lower it to make a change pass; write the real test.",
                "- Fix security findings at the code level; NEVER suppress, exclude, or annotate them away.",
                "- Follow `.ade/templates/pr-checklist.md` before requesting review and `.ade/templates/commit-conventions.md` for every commit.",
            ]
            .join("\n"),
        }]
    }

    fn detect(&self, ctx: &Ctx) -> Vec<Finding> {
        let errors = validate_scaffolding_options(&module_options(ctx));
        if !errors.is_empty() {
            return errors.into_iter().map(Finding::error).collect();
        }
        vec![Finding::ok("scaffolding options valid")]
    }

    fn plan(&self, _ctx: &Ctx) -> Vec<PlannedAction> {
        vec![
            PlannedAction {
                kind: ActionKind::Write,
                path: Some(PR_CHECKLIST_PATH.to_string()),
                description: "write PR checklist template".to_string(),
            },
            PlannedAction {
                kind: ActionKind::Write,
                path: Some(TESTING_CONVENTIONS_PATH.to_string()),
                description: "write testing conventions template".to_string(),
            },
            PlannedAction {
                kind: ActionKind::Write,
                path: Some(COMMIT_CONVENTIONS_PATH.to_string()),
                description: "write commit conventions template".to_string(),
            },
        ]
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
        let mut ok = true;
        for (rel_path, header) in TEMPLATE_HEADERS {
            let text = read_if_exists(&ctx.target_dir.join(rel_path));
            match text {
                Some(text) if !text.trim().is_empty() => {
                    if text.split('\n').any(|line| line.trim() == header) {
                        findings.push(Finding::ok(format!(
                            "{rel_path} present with expected heading"
                        )));
                    } else {
                        ok = false;
                        findings.push(Finding::error_with(
                            format!("{rel_path} missing its `{header}` heading"),
                            "run `ade apply` to regenerate",
                        ));
                    }
                }
                _ => {
                    ok = false;
                    findings.push(Finding::error_with(
                        format!("{rel_path} missing or empty"),
                        "run `ade apply`",
                    ));
                }
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
    use crate::types::{AdeConfig, FindingLevel};
    use std::path::Path;

    const ALL_TEMPLATE_PATHS: [&str; 3] = [
        PR_CHECKLIST_PATH,
        TESTING_CONVENTIONS_PATH,
        COMMIT_CONVENTIONS_PATH,
    ];

    fn read_file(dir: &Path, rel: &str) -> Option<String> {
        read_if_exists(&dir.join(rel))
    }

    fn config_with_floor(floor: serde_json::Value) -> AdeConfig {
        let mut config = test_config_with(&[], &["claude-code", "codex"]);
        if let Some(module) = config.modules.get_mut("scaffolding") {
            module.options = serde_json::json!({ "coverageFloorPct": floor });
        }
        config
    }

    #[test]
    fn isc_76_apply_writes_all_three_templates_with_real_content_15_to_30_lines_each() {
        let dir = make_temp_dir("scaf-apply");
        let ctx = make_test_ctx(&dir, TestCtxOptions::default());
        let result = MODULE.apply(&ctx);
        assert_eq!(result.status, ModuleStatus::Applied);
        for rel_path in ALL_TEMPLATE_PATHS {
            assert!(result.wrote_paths.contains(&rel_path.to_string()));
            let text = read_file(&dir, rel_path).unwrap();
            let line_count = text.trim_end().split('\n').count();
            assert!(line_count >= 15, "{rel_path} has {line_count} lines");
            assert!(line_count <= 30, "{rel_path} has {line_count} lines");
            assert!(text.contains(template_header(rel_path).unwrap()));
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn isc_76_pr_checklist_covers_tests_coverage_security_secrets_docs_suppression() {
        let dir = make_temp_dir("scaf-pr");
        MODULE.apply(&make_test_ctx(&dir, TestCtxOptions::default()));
        let checklist = read_file(&dir, PR_CHECKLIST_PATH).unwrap();
        assert!(checklist.contains("Tests added"));
        assert!(checklist.contains("test suite passes"));
        assert!(checklist.contains("Coverage floor met"));
        assert!(checklist.to_lowercase().contains("security review"));
        assert!(checklist.contains("No secrets"));
        assert!(checklist.contains("Docs updated"));
        assert!(checklist.contains("suppressed"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn isc_76_testing_conventions_mandate_test_first_and_forbid_coverage_padding() {
        let dir = make_temp_dir("scaf-testing");
        MODULE.apply(&make_test_ctx(&dir, TestCtxOptions::default()));
        let conventions = read_file(&dir, TESTING_CONVENTIONS_PATH).unwrap();
        assert!(conventions.contains("BEFORE the behavior change"));
        assert!(conventions.contains("Coverage padding is a defect"));
        assert!(conventions.contains("intended use case"));
        assert!(conventions
            .to_lowercase()
            .contains("integration over mocks"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn isc_76_commit_conventions_specify_types_imperative_subject_why_body_atomic_no_secrets() {
        let dir = make_temp_dir("scaf-commit");
        MODULE.apply(&make_test_ctx(&dir, TestCtxOptions::default()));
        let conventions = read_file(&dir, COMMIT_CONVENTIONS_PATH).unwrap();
        assert!(conventions.contains("type(scope): subject"));
        assert!(conventions.to_lowercase().contains("imperative"));
        assert!(conventions.contains("72 characters"));
        assert!(conventions.contains("WHY"));
        assert!(conventions.contains("One logical change per commit"));
        assert!(conventions.contains("Secrets"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn isc_77_instruction_block_carries_test_first_hard_gate_never_suppress_and_pointers() {
        let blocks = MODULE.instruction_blocks();
        assert!(!blocks.is_empty());
        let block = &blocks[0];
        assert!(block.content.contains("Test-first"));
        assert!(block.content.contains("HARD gate"));
        assert!(block.content.to_lowercase().contains("never suppress"));
        assert!(block.content.contains(".ade/templates/pr-checklist.md"));
        assert!(block
            .content
            .contains(".ade/templates/commit-conventions.md"));
    }

    #[test]
    fn isc_116_plan_writes_nothing_to_disk() {
        let dir = make_temp_dir("scaf-plan");
        let ctx = make_test_ctx(&dir, TestCtxOptions::default());
        let actions = MODULE.plan(&ctx);
        assert_eq!(actions.len(), 3);
        for rel_path in ALL_TEMPLATE_PATHS {
            assert!(read_file(&dir, rel_path).is_none());
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn isc_117_apply_is_idempotent_second_run_byte_identical_for_all_templates() {
        let dir = make_temp_dir("scaf-idem");
        MODULE.apply(&make_test_ctx(&dir, TestCtxOptions::default()));
        let mut first: Vec<(&str, String)> = Vec::new();
        for rel_path in ALL_TEMPLATE_PATHS {
            first.push((rel_path, sha256_hex(&read_file(&dir, rel_path).unwrap())));
        }
        MODULE.apply(&make_test_ctx(&dir, TestCtxOptions::default()));
        for (rel_path, hash) in first {
            assert_eq!(sha256_hex(&read_file(&dir, rel_path).unwrap()), hash);
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn coverage_floor_pct_option_rendered_into_checklist_and_testing_conventions() {
        let dir = make_temp_dir("scaf-floor");
        let ctx = make_test_ctx(
            &dir,
            TestCtxOptions {
                config: Some(config_with_floor(serde_json::json!(80))),
                ..Default::default()
            },
        );
        MODULE.apply(&ctx);
        assert!(read_file(&dir, PR_CHECKLIST_PATH).unwrap().contains("80%"));
        assert!(read_file(&dir, TESTING_CONVENTIONS_PATH)
            .unwrap()
            .contains("80%"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn malformed_coverage_floor_pct_rejected_apply_fails_nothing_written() {
        let dir = make_temp_dir("scaf-malformed");
        let ctx = make_test_ctx(
            &dir,
            TestCtxOptions {
                config: Some(config_with_floor(serde_json::json!(150))),
                ..Default::default()
            },
        );
        let result = MODULE.apply(&ctx);
        assert_eq!(result.status, ModuleStatus::Failed);
        assert!(result
            .findings
            .iter()
            .any(|finding| finding.level == FindingLevel::Error));
        for rel_path in ALL_TEMPLATE_PATHS {
            assert!(read_file(&dir, rel_path).is_none());
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn validate_scaffolding_options_catches_every_malformed_floor_value() {
        assert!(validate_scaffolding_options(&serde_json::json!({})).is_empty());
        assert!(
            validate_scaffolding_options(&serde_json::json!({"coverageFloorPct": 95})).is_empty()
        );
        assert!(
            validate_scaffolding_options(&serde_json::json!({"coverageFloorPct": 100})).is_empty()
        );
        assert_eq!(
            validate_scaffolding_options(&serde_json::json!({"coverageFloorPct": 0})).len(),
            1
        );
        assert_eq!(
            validate_scaffolding_options(&serde_json::json!({"coverageFloorPct": 101})).len(),
            1
        );
        assert_eq!(
            validate_scaffolding_options(&serde_json::json!({"coverageFloorPct": "high"})).len(),
            1
        );
        // NaN is unrepresentable in serde_json — a present-but-non-numeric value
        // (null) exercises the same rejection branch as the TS oracle's NaN case.
        assert_eq!(
            validate_scaffolding_options(&serde_json::json!({"coverageFloorPct": null})).len(),
            1
        );
    }

    #[test]
    fn detect_reports_option_validity() {
        let dir = make_temp_dir("scaf-detect");
        let ok = MODULE.detect(&make_test_ctx(&dir, TestCtxOptions::default()));
        assert!(ok.iter().all(|finding| finding.level == FindingLevel::Ok));
        let bad = MODULE.detect(&make_test_ctx(
            &dir,
            TestCtxOptions {
                config: Some(config_with_floor(serde_json::json!(-1))),
                ..Default::default()
            },
        ));
        assert!(bad
            .iter()
            .any(|finding| finding.level == FindingLevel::Error));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn verify_passes_after_apply_all_three_templates_present_with_h1_headers() {
        let dir = make_temp_dir("scaf-verify");
        let ctx = make_test_ctx(&dir, TestCtxOptions::default());
        MODULE.apply(&ctx);
        let result = MODULE.verify(&ctx);
        assert!(result.ok);
        assert_eq!(
            result
                .findings
                .iter()
                .filter(|finding| finding.level == FindingLevel::Ok)
                .count(),
            3
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn verify_fails_when_a_template_is_missing() {
        let dir = make_temp_dir("scaf-missing");
        let ctx = make_test_ctx(&dir, TestCtxOptions::default());
        MODULE.apply(&ctx);
        std::fs::remove_dir_all(dir.join(".ade").join("templates")).unwrap();
        let result = MODULE.verify(&ctx);
        assert!(!result.ok);
        assert!(result.findings.iter().any(|finding| finding
            .remediation
            .as_deref()
            .map(|remediation| remediation.contains("ade apply"))
            .unwrap_or(false)));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn verify_fails_on_emptied_template_and_on_stripped_h1_header() {
        let dir = make_temp_dir("scaf-tamper");
        let ctx = make_test_ctx(&dir, TestCtxOptions::default());
        MODULE.apply(&ctx);

        std::fs::write(dir.join(PR_CHECKLIST_PATH), "   \n").unwrap();
        assert!(!MODULE.verify(&ctx).ok);

        MODULE.apply(&make_test_ctx(&dir, TestCtxOptions::default()));
        let original = read_file(&dir, TESTING_CONVENTIONS_PATH).unwrap();
        std::fs::write(
            dir.join(TESTING_CONVENTIONS_PATH),
            original.replace(
                template_header(TESTING_CONVENTIONS_PATH).unwrap(),
                "tampered",
            ),
        )
        .unwrap();
        let result = MODULE.verify(&ctx);
        assert!(!result.ok);
        assert!(result
            .findings
            .iter()
            .any(|finding| finding.message.contains(TESTING_CONVENTIONS_PATH)));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
