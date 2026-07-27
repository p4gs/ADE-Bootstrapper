//! Module: software supply chain security — port of `src/modules/supply-chain.ts`.
//! Spec component: "Software supply chain security — dependency policy for
//! traditional package ecosystems (registry allowlist, minimum package age,
//! lockfile enforcement, typosquatting defense) AND AI-native dependencies
//! (skills, plugins, MCP servers, instruction packs, agent configs), which
//! are treated as untrusted code requiring review, pinning, and explicit
//! user approval before install (e.g. OSV-Scanner for vulnerability audit)."
//! Boundary controlled: the dependency boundary — nothing enters the project
//! (package or AI-native artifact) without policy review, and AI-suggested
//! package names are treated as hallucination-prone until verified.

use std::path::Path;

use serde_json::json;

use crate::fsutil::read_if_exists;
use crate::modules::shared::{read_json, tool_finding, verify_json_artifact, write_policy};
use crate::types::{
    ActionKind, AdeModule, Ctx, Finding, FindingLevel, InstructionBlock, ModuleResult,
    ModuleStatus, PlannedAction, VerifyResult,
};

pub const DEPENDENCIES_POLICY_PATH: &str = ".ade/policy/dependencies.json";
pub const DEFAULT_MIN_AGE_DAYS: u64 = 14;

pub const REGISTRY_ALLOWLIST: [&str; 4] = [
    "registry.npmjs.org",
    "pypi.org",
    "crates.io",
    "proxy.golang.org",
];

/// The AI-native dependency classes the policy governs.
pub const AI_NATIVE_DEP_KINDS: [&str; 5] = [
    "skills",
    "plugins",
    "mcpServers",
    "instructionPacks",
    "agentConfigs",
];

const OSV_REMEDIATION: &str =
    "install OSV-Scanner (e.g. `brew install osv-scanner`) to enable dependency vulnerability scanning";

fn build_policy(min_age_days: serde_json::Value) -> serde_json::Value {
    let mut ai_native_dependencies = serde_json::Map::new();
    for kind in AI_NATIVE_DEP_KINDS {
        ai_native_dependencies.insert(
            kind.to_string(),
            json!({
                "rule": "review-before-install",
                "pinningRequired": true,
                "sourceAllowlist": ["explicit-user-approval"],
            }),
        );
    }
    json!({
        "schemaVersion": 1,
        "registries": { "allowlist": REGISTRY_ALLOWLIST },
        "minAgeDays": min_age_days,
        "requireLockfiles": true,
        "installReview": "required",
        "typosquattingPolicy": "verify exact package name against its repository before install",
        "aiNativeDependencies": ai_native_dependencies,
    })
}

fn is_valid_min_age(value: &serde_json::Value) -> bool {
    value
        .as_f64()
        .map(|min_age| min_age.is_finite() && min_age >= 0.0)
        .unwrap_or(false)
}

/// Validate module options; returns errors (empty = valid).
pub fn validate_supply_chain_options(options: &serde_json::Value) -> Vec<String> {
    if let Some(min_age) = options.get("minAgeDays") {
        if !is_valid_min_age(min_age) {
            return vec!["options.minAgeDays must be a non-negative number".to_string()];
        }
    }
    vec![]
}

fn resolve_min_age_days(options: &serde_json::Value) -> serde_json::Value {
    match options.get("minAgeDays") {
        Some(min_age) if is_valid_min_age(min_age) => min_age.clone(),
        _ => json!(DEFAULT_MIN_AGE_DAYS),
    }
}

/// Detect ecosystem manifests in `target_dir` and flag missing lockfiles as
/// warn findings. Exported for direct testing (ISC-66).
///
/// Rules: package.json → (bun.lock|bun.lockb|package-lock.json|yarn.lock|
/// pnpm-lock.yaml); Cargo.toml → Cargo.lock; go.mod → go.sum;
/// (pyproject.toml|requirements.txt) → (uv.lock|poetry.lock|requirements.txt
/// itself counts as pinned only if it contains `==`).
pub fn check_lockfiles(target_dir: &Path) -> Vec<Finding> {
    let mut findings: Vec<Finding> = Vec::new();
    let exists = |name: &str| target_dir.join(name).exists();

    if exists("package.json") {
        let npm_locks = [
            "bun.lock",
            "bun.lockb",
            "package-lock.json",
            "yarn.lock",
            "pnpm-lock.yaml",
        ];
        if npm_locks.iter().any(|name| exists(name)) {
            findings.push(Finding::ok("package.json has a lockfile"));
        } else {
            findings.push(Finding::warn(
                "package.json present without a lockfile",
                "generate one (e.g. `bun install` → bun.lock) and commit it so dependency resolution is pinned",
            ));
        }
    }

    if exists("Cargo.toml") {
        if exists("Cargo.lock") {
            findings.push(Finding::ok("Cargo.toml has Cargo.lock"));
        } else {
            findings.push(Finding::warn(
                "Cargo.toml present without Cargo.lock",
                "run `cargo generate-lockfile` and commit Cargo.lock",
            ));
        }
    }

    if exists("go.mod") {
        if exists("go.sum") {
            findings.push(Finding::ok("go.mod has go.sum"));
        } else {
            findings.push(Finding::warn(
                "go.mod present without go.sum",
                "run `go mod tidy` and commit go.sum",
            ));
        }
    }

    let has_pyproject = exists("pyproject.toml");
    let requirements = read_if_exists(&target_dir.join("requirements.txt"));
    if has_pyproject || requirements.is_some() {
        let pinned_lock = exists("uv.lock") || exists("poetry.lock");
        let pinned_requirements = requirements
            .as_deref()
            .map(|text| text.contains("=="))
            .unwrap_or(false);
        if pinned_lock || pinned_requirements {
            findings.push(Finding::ok("python manifest has pinned dependencies"));
        } else {
            findings.push(Finding::warn(
                "python manifest present without pinned dependencies",
                "add a lockfile (uv.lock or poetry.lock) or pin exact versions in requirements.txt with `==`",
            ));
        }
    }

    findings
}

pub struct SupplyChainModule;

pub static MODULE: SupplyChainModule = SupplyChainModule;

fn apply_inner(ctx: &Ctx) -> std::io::Result<ModuleResult> {
    let options = ctx.module_options("supply-chain");
    let errors = validate_supply_chain_options(&options);
    if !errors.is_empty() {
        return Ok(ModuleResult {
            status: ModuleStatus::Failed,
            findings: errors.into_iter().map(Finding::error).collect(),
            wrote_paths: vec![],
        });
    }

    write_policy(
        ctx,
        DEPENDENCIES_POLICY_PATH,
        &build_policy(resolve_min_age_days(&options)),
    )?;
    let mut findings = vec![Finding::ok(format!("wrote {DEPENDENCIES_POLICY_PATH}"))];
    findings.extend(check_lockfiles(&ctx.target_dir));

    if !ctx.tool_present("osv-scanner") {
        findings.push(Finding::degraded(
            "osv-scanner not installed — dependency vulnerability scanning unavailable",
            OSV_REMEDIATION,
        ));
        return Ok(ModuleResult {
            status: ModuleStatus::Degraded,
            findings,
            wrote_paths: vec![DEPENDENCIES_POLICY_PATH.to_string()],
        });
    }
    Ok(ModuleResult {
        status: ModuleStatus::Applied,
        findings,
        wrote_paths: vec![DEPENDENCIES_POLICY_PATH.to_string()],
    })
}

impl AdeModule for SupplyChainModule {
    fn id(&self) -> &'static str {
        "supply-chain"
    }
    fn title(&self) -> &'static str {
        "Supply Chain Security"
    }
    fn category(&self) -> &'static str {
        "security"
    }
    fn spec(&self) -> &'static str {
        "Software supply chain security"
    }

    fn instruction_blocks(&self) -> Vec<InstructionBlock> {
        vec![InstructionBlock {
            id: "supply-chain",
            title: "Supply Chain Security",
            content: [
                "- NEVER install a dependency without checking `.ade/policy/dependencies.json` first.",
                "- New dependencies require: the minimum-age check (skip packages younger than the policy's `minAgeDays`), exact-name verification against the package's source repository, and human approval before install.",
                "- AI-suggested package names are hallucination-prone — verify the package exists AND that its repository matches the claimed project before installing anything.",
                "- Install only from the policy's registry allowlist, and keep lockfiles committed — never install with lockfile updates disabled or bypassed.",
                "- AI-native dependencies (skills, plugins, MCP servers, instruction packs, agent configs) are untrusted code: review their contents, pin their versions, and obtain explicit user approval before adding them.",
            ]
            .join("\n"),
        }]
    }

    fn detect(&self, ctx: &Ctx) -> Vec<Finding> {
        let mut findings = vec![tool_finding(ctx, "osv-scanner", OSV_REMEDIATION)];
        for message in validate_supply_chain_options(&ctx.module_options("supply-chain")) {
            findings.push(Finding::error(message));
        }
        findings
    }

    fn plan(&self, _ctx: &Ctx) -> Vec<PlannedAction> {
        vec![
            PlannedAction {
                kind: ActionKind::Write,
                path: Some(DEPENDENCIES_POLICY_PATH.to_string()),
                description:
                    "write dependency policy (registry allowlist, min package age, lockfile + install review requirements, AI-native dependency rules)"
                        .to_string(),
            },
            PlannedAction {
                kind: ActionKind::Info,
                path: None,
                description: "check ecosystem manifests for missing lockfiles".to_string(),
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
        let artifact = verify_json_artifact(ctx, DEPENDENCIES_POLICY_PATH);
        if artifact.level != FindingLevel::Ok {
            return VerifyResult {
                ok: false,
                findings: vec![artifact],
            };
        }
        let parsed = match read_json(ctx, DEPENDENCIES_POLICY_PATH) {
            Some(parsed) => parsed,
            None => {
                return VerifyResult {
                    ok: false,
                    findings: vec![artifact],
                }
            }
        };

        let mut findings = vec![artifact];
        let mut ok = true;

        let traditional_valid = parsed
            .get("registries")
            .and_then(|registries| registries.get("allowlist"))
            .and_then(|allowlist| allowlist.as_array())
            .map(|allowlist| !allowlist.is_empty())
            .unwrap_or(false)
            && parsed
                .get("minAgeDays")
                .map(|value| value.is_number())
                .unwrap_or(false)
            && parsed
                .get("requireLockfiles")
                .and_then(|value| value.as_bool())
                == Some(true)
            && parsed
                .get("installReview")
                .map(|value| value.is_string())
                .unwrap_or(false)
            && parsed
                .get("typosquattingPolicy")
                .map(|value| value.is_string())
                .unwrap_or(false);
        if !traditional_valid {
            ok = false;
            findings.push(Finding::error_with(
                format!("{DEPENDENCIES_POLICY_PATH} missing or malformed traditional-dependency section"),
                "run `ade apply` to regenerate",
            ));
        }

        let ai_native_valid = parsed
            .get("aiNativeDependencies")
            .and_then(|ai_native| ai_native.as_object())
            .map(|ai_native| {
                AI_NATIVE_DEP_KINDS.iter().all(|kind| {
                    ai_native
                        .get(*kind)
                        .map(|entry| {
                            entry
                                .get("rule")
                                .map(|rule| rule.is_string())
                                .unwrap_or(false)
                                && entry
                                    .get("pinningRequired")
                                    .and_then(|value| value.as_bool())
                                    == Some(true)
                                && entry
                                    .get("sourceAllowlist")
                                    .map(|value| value.is_array())
                                    .unwrap_or(false)
                        })
                        .unwrap_or(false)
                })
            })
            .unwrap_or(false);
        if !ai_native_valid {
            ok = false;
            findings.push(Finding::error_with(
                format!(
                    "{DEPENDENCIES_POLICY_PATH} missing or malformed aiNativeDependencies section"
                ),
                "run `ade apply` to regenerate",
            ));
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
        serde_json::from_str(&std::fs::read_to_string(dir.join(DEPENDENCIES_POLICY_PATH)).unwrap())
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
    fn isc_63_apply_writes_dependency_policy_with_all_traditional_requirements() {
        let dir = make_temp_dir("supply-63");
        let ctx = make_test_ctx(
            &dir,
            TestCtxOptions {
                present_tools: &[("osv-scanner", "osv-scanner 2.0.0")],
                ..Default::default()
            },
        );
        let result = MODULE.apply(&ctx);
        assert!(result
            .wrote_paths
            .contains(&DEPENDENCIES_POLICY_PATH.to_string()));
        let policy = read_policy(&dir);
        let allowlist = policy["registries"]["allowlist"].as_array().unwrap();
        for host in [
            "registry.npmjs.org",
            "pypi.org",
            "crates.io",
            "proxy.golang.org",
        ] {
            assert!(
                allowlist.iter().any(|entry| entry.as_str() == Some(host)),
                "missing {host}"
            );
        }
        assert_eq!(allowlist.len(), REGISTRY_ALLOWLIST.len());
        assert_eq!(
            policy["minAgeDays"],
            serde_json::json!(DEFAULT_MIN_AGE_DAYS)
        );
        assert_eq!(policy["requireLockfiles"], serde_json::json!(true));
        assert_eq!(policy["installReview"], serde_json::json!("required"));
        assert_eq!(
            policy["typosquattingPolicy"],
            serde_json::json!("verify exact package name against its repository before install")
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn isc_63_min_age_days_option_overrides_the_14_day_default() {
        let dir = make_temp_dir("supply-minage");
        let mut config = test_config_with(&[], &["claude-code", "codex"]);
        config.modules.get_mut("supply-chain").unwrap().options =
            serde_json::json!({ "minAgeDays": 30 });
        let ctx = make_test_ctx(
            &dir,
            TestCtxOptions {
                config: Some(config),
                present_tools: &[("osv-scanner", "2.0.0")],
                ..Default::default()
            },
        );
        MODULE.apply(&ctx);
        let policy = read_policy(&dir);
        assert_eq!(policy["minAgeDays"], serde_json::json!(30));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn isc_63_malformed_min_age_days_apply_fails_nothing_written() {
        let dir = make_temp_dir("supply-badage");
        let mut config = test_config_with(&[], &["claude-code", "codex"]);
        config.modules.get_mut("supply-chain").unwrap().options =
            serde_json::json!({ "minAgeDays": "soon" });
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
        assert!(!dir.join(DEPENDENCIES_POLICY_PATH).exists());
        assert_eq!(
            validate_supply_chain_options(&serde_json::json!({ "minAgeDays": -1 })).len(),
            1
        );
        assert_eq!(
            validate_supply_chain_options(&serde_json::json!({ "minAgeDays": 7 })),
            Vec::<String>::new()
        );
        assert_eq!(
            validate_supply_chain_options(&serde_json::json!({})),
            Vec::<String>::new()
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn isc_64_policy_has_first_class_ai_native_dependencies_section() {
        let dir = make_temp_dir("supply-64");
        let ctx = make_test_ctx(
            &dir,
            TestCtxOptions {
                present_tools: &[("osv-scanner", "2.0.0")],
                ..Default::default()
            },
        );
        MODULE.apply(&ctx);
        let policy = read_policy(&dir);
        assert_eq!(
            AI_NATIVE_DEP_KINDS,
            [
                "skills",
                "plugins",
                "mcpServers",
                "instructionPacks",
                "agentConfigs"
            ]
        );
        for kind in [
            "skills",
            "plugins",
            "mcpServers",
            "instructionPacks",
            "agentConfigs",
        ] {
            let entry = &policy["aiNativeDependencies"][kind];
            assert_eq!(entry["rule"], serde_json::json!("review-before-install"));
            assert_eq!(entry["pinningRequired"], serde_json::json!(true));
            assert_eq!(
                entry["sourceAllowlist"],
                serde_json::json!(["explicit-user-approval"])
            );
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn isc_65_detect_flags_absent_osv_scanner_as_degraded_ok_when_present() {
        let dir = make_temp_dir("supply-65");
        let absent = MODULE.detect(&make_test_ctx(&dir, TestCtxOptions::default()));
        let degraded = absent
            .iter()
            .find(|finding| finding.level == FindingLevel::Degraded)
            .expect("degraded finding expected");
        assert!(degraded
            .remediation
            .as_deref()
            .unwrap()
            .contains("brew install osv-scanner"));

        let present = MODULE.detect(&make_test_ctx(
            &dir,
            TestCtxOptions {
                present_tools: &[("osv-scanner", "osv-scanner 2.0.0")],
                ..Default::default()
            },
        ));
        assert!(present
            .iter()
            .any(|finding| finding.level == FindingLevel::Ok
                && finding.message.contains("osv-scanner")));
        assert!(!present
            .iter()
            .any(|finding| finding.level == FindingLevel::Degraded));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn isc_65_osv_scanner_absent_apply_still_writes_policy_and_returns_degraded() {
        let dir = make_temp_dir("supply-65b");
        let result = MODULE.apply(&make_test_ctx(&dir, TestCtxOptions::default()));
        assert_eq!(result.status, ModuleStatus::Degraded);
        assert!(result
            .wrote_paths
            .contains(&DEPENDENCIES_POLICY_PATH.to_string()));
        assert!(dir.join(DEPENDENCIES_POLICY_PATH).exists());
        assert!(result.findings.iter().any(|finding| finding
            .remediation
            .as_deref()
            .map(|remediation| remediation.contains("osv-scanner"))
            .unwrap_or(false)));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn isc_65_osv_scanner_present_apply_returns_applied() {
        let dir = make_temp_dir("supply-65c");
        let result = MODULE.apply(&make_test_ctx(
            &dir,
            TestCtxOptions {
                present_tools: &[("osv-scanner", "osv-scanner 2.0.0")],
                ..Default::default()
            },
        ));
        assert_eq!(result.status, ModuleStatus::Applied);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn isc_66_missing_lockfiles_per_ecosystem_flagged_as_warn_findings() {
        let dir = make_temp_dir("supply-66a");
        write_file(&dir, "package.json", "{}\n");
        write_file(&dir, "Cargo.toml", "[package]\n");
        write_file(&dir, "go.mod", "module example.com/m\n");
        write_file(&dir, "pyproject.toml", "[project]\n");
        let findings = check_lockfiles(&dir);
        let warns: Vec<&Finding> = findings
            .iter()
            .filter(|finding| finding.level == FindingLevel::Warn)
            .collect();
        assert_eq!(warns.len(), 4);
        assert!(warns
            .iter()
            .any(|finding| finding.message.contains("package.json")));
        assert!(warns
            .iter()
            .any(|finding| finding.message.contains("Cargo.toml")));
        assert!(warns
            .iter()
            .any(|finding| finding.message.contains("go.mod")));
        assert!(warns
            .iter()
            .any(|finding| finding.message.contains("python")));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn isc_66_any_accepted_lockfile_satisfies_its_ecosystem() {
        let dir = make_temp_dir("supply-66b");
        write_file(&dir, "package.json", "{}\n");
        write_file(&dir, "pnpm-lock.yaml", "lockfileVersion: 9\n");
        write_file(&dir, "Cargo.toml", "[package]\n");
        write_file(&dir, "Cargo.lock", "version = 4\n");
        write_file(&dir, "go.mod", "module example.com/m\n");
        write_file(&dir, "go.sum", "example.com/dep v1.0.0 h1:abc=\n");
        let findings = check_lockfiles(&dir);
        assert_eq!(
            findings
                .iter()
                .filter(|finding| finding.level == FindingLevel::Warn)
                .count(),
            0
        );
        assert_eq!(
            findings
                .iter()
                .filter(|finding| finding.level == FindingLevel::Ok)
                .count(),
            3
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn isc_66_requirements_txt_counts_as_pinned_only_with_double_equals() {
        let dir = make_temp_dir("supply-66c");
        write_file(&dir, "requirements.txt", "flask>=2.0\nrequests\n");
        let unpinned = check_lockfiles(&dir);
        assert!(unpinned.iter().any(
            |finding| finding.level == FindingLevel::Warn && finding.message.contains("python")
        ));

        write_file(&dir, "requirements.txt", "flask==2.3.0\nrequests==2.31.0\n");
        let pinned = check_lockfiles(&dir);
        assert_eq!(
            pinned
                .iter()
                .filter(|finding| finding.level == FindingLevel::Warn)
                .count(),
            0
        );
        assert!(pinned
            .iter()
            .any(|finding| finding.level == FindingLevel::Ok));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn isc_66_uv_lock_or_poetry_lock_satisfies_an_unpinned_python_manifest() {
        let dir = make_temp_dir("supply-66d");
        write_file(&dir, "pyproject.toml", "[project]\n");
        write_file(&dir, "uv.lock", "version = 1\n");
        let findings = check_lockfiles(&dir);
        assert_eq!(
            findings
                .iter()
                .filter(|finding| finding.level == FindingLevel::Warn)
                .count(),
            0
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn isc_66_no_manifests_no_lockfile_findings_at_all() {
        let dir = make_temp_dir("supply-66e");
        assert_eq!(check_lockfiles(&dir), Vec::<Finding>::new());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn isc_66_apply_surfaces_lockfile_warns_in_its_findings() {
        let dir = make_temp_dir("supply-66f");
        write_file(&dir, "package.json", "{}\n");
        let result = MODULE.apply(&make_test_ctx(
            &dir,
            TestCtxOptions {
                present_tools: &[("osv-scanner", "2.0.0")],
                ..Default::default()
            },
        ));
        assert!(result
            .findings
            .iter()
            .any(|finding| finding.level == FindingLevel::Warn
                && finding.message.contains("package.json")));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn isc_67_instruction_block_mandates_the_full_dependency_discipline() {
        let blocks = MODULE.instruction_blocks();
        assert!(!blocks.is_empty());
        let content = &blocks[0].content;
        assert!(content.contains(".ade/policy/dependencies.json"));
        assert!(content.contains("NEVER install a dependency without checking"));
        assert!(content.contains("minimum-age check"));
        assert!(content.contains("exact-name verification"));
        assert!(content.to_lowercase().contains("human approval"));
        assert!(content.to_lowercase().contains("hallucination-prone"));
    }

    #[test]
    fn isc_116_plan_writes_nothing_to_disk() {
        let dir = make_temp_dir("supply-116");
        let actions = MODULE.plan(&make_test_ctx(&dir, TestCtxOptions::default()));
        assert!(!actions.is_empty());
        assert!(actions.iter().any(|action| action.kind == ActionKind::Write
            && action.path.as_deref() == Some(DEPENDENCIES_POLICY_PATH)));
        assert!(!dir.join(DEPENDENCIES_POLICY_PATH).exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn isc_117_apply_is_idempotent_second_run_byte_identical() {
        let dir = make_temp_dir("supply-117");
        MODULE.apply(&make_test_ctx(
            &dir,
            TestCtxOptions {
                present_tools: &[("osv-scanner", "2.0.0")],
                ..Default::default()
            },
        ));
        let first =
            sha256_hex(&std::fs::read_to_string(dir.join(DEPENDENCIES_POLICY_PATH)).unwrap());
        MODULE.apply(&make_test_ctx(
            &dir,
            TestCtxOptions {
                present_tools: &[("osv-scanner", "2.0.0")],
                ..Default::default()
            },
        ));
        let second =
            sha256_hex(&std::fs::read_to_string(dir.join(DEPENDENCIES_POLICY_PATH)).unwrap());
        assert_eq!(second, first);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn verify_passes_after_apply_and_fails_on_each_tampering_class() {
        let dir = make_temp_dir("supply-verify");
        let ctx = make_test_ctx(
            &dir,
            TestCtxOptions {
                present_tools: &[("osv-scanner", "2.0.0")],
                ..Default::default()
            },
        );
        assert!(!MODULE.verify(&ctx).ok);

        MODULE.apply(&ctx);
        assert!(MODULE.verify(&ctx).ok);

        let good = read_policy(&dir);

        write_file(&dir, DEPENDENCIES_POLICY_PATH, "{not json");
        assert!(!MODULE.verify(&ctx).ok);

        let mut no_traditional = good.clone();
        no_traditional["registries"] = serde_json::json!({ "allowlist": [] });
        write_file(
            &dir,
            DEPENDENCIES_POLICY_PATH,
            &serde_json::to_string(&no_traditional).unwrap(),
        );
        let traditional_result = MODULE.verify(&ctx);
        assert!(!traditional_result.ok);
        assert!(traditional_result
            .findings
            .iter()
            .any(|finding| finding.message.contains("traditional")));

        let mut dropped_kind = good.clone();
        dropped_kind["aiNativeDependencies"]
            .as_object_mut()
            .unwrap()
            .remove("mcpServers");
        write_file(
            &dir,
            DEPENDENCIES_POLICY_PATH,
            &serde_json::to_string(&dropped_kind).unwrap(),
        );
        let ai_result = MODULE.verify(&ctx);
        assert!(!ai_result.ok);
        assert!(ai_result
            .findings
            .iter()
            .any(|finding| finding.message.contains("aiNativeDependencies")));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
