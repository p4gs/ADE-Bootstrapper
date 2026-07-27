//! Pipeline orchestration — port of `src/run.ts`: init / plan / apply / verify
//! across all enabled modules plus the core-owned steps (instruction
//! composition, translation, lockfile, audit). Modules are fault-isolated:
//! one panicking module is reported as failed and the run continues.

use crate::audit::{append_events, checkpoint_of, parse_log, verify_chain, AuditEventInput};
use crate::config::{default_config, load_config, serialize_config, CONFIG_FILE};
use crate::context::{build_ctx, BuildCtxOptions};
use crate::fsutil::{read_if_exists, write_ensured};
use crate::harness::adapters::HARNESS_ADAPTERS;
use crate::instructions::{
    collect_blocks, compose_instructions, compose_managed_body, INSTRUCTIONS_PATH,
    LOCAL_INSTRUCTIONS_PATH, LOCAL_INSTRUCTIONS_STUB,
};
use crate::lockfile::{
    generate_lockfile, load_lockfile, scan_ade_tree, serialize_lockfile, verify_against_lockfile,
    LOCKFILE_NAME,
};
use crate::registry::{module_ids, modules};
use crate::translate::{check_translation_drift, translate_all, TranslateFileResult};
use crate::types::{
    ActionKind, AdeConfig, Ctx, ExecFn, Finding, FindingLevel, ModuleResult, ModuleStatus,
    PlannedAction, WhichFn,
};
use std::collections::BTreeSet;
use std::panic::AssertUnwindSafe;
use std::path::Path;
use std::sync::Arc;

pub const AUDIT_LOG_PATH: &str = ".ade/audit/log.jsonl";

pub type NowFn = Arc<dyn Fn() -> String + Send + Sync>;

pub struct PipelineDeps {
    pub exec: ExecFn,
    pub which: WhichFn,
    /// Clock injected so audit timestamps are testable; generated FILES never embed time.
    pub now: Option<NowFn>,
}

fn now_iso() -> String {
    let duration = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let millis = duration.subsec_millis();
    let base = crate::gui::jobs::now_utc_seconds(duration.as_secs());
    format!("{}.{millis:03}Z", base.trim_end_matches('Z'))
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ModuleRunReport {
    pub id: String,
    pub title: String,
    pub enabled: bool,
    pub result: ModuleResult,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplyReport {
    pub ok: bool,
    pub modules: Vec<ModuleRunReport>,
    pub translate: Vec<TranslateFileResult>,
    pub lockfile_path: String,
    pub audit_appended: usize,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ModuleVerify {
    pub id: String,
    pub ok: bool,
    pub findings: Vec<Finding>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct VerifyReport {
    pub ok: bool,
    pub lockfile: Vec<Finding>,
    pub audit: Vec<Finding>,
    pub translation: Vec<Finding>,
    pub modules: Vec<ModuleVerify>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct PlanModule {
    pub id: String,
    pub enabled: bool,
    pub actions: Vec<PlannedAction>,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanReport {
    pub modules: Vec<PlanModule>,
    pub core_actions: Vec<PlannedAction>,
}

fn enabled_ids(config: &AdeConfig) -> BTreeSet<String> {
    config
        .modules
        .iter()
        .filter(|(_, module_config)| module_config.enabled)
        .map(|(id, _)| id.clone())
        .collect()
}

/// Lockfile scope: fully ADE-owned generated files (`.ade/**`), excluding the mutable audit log.
pub fn lockfile_scope(paths: &[String]) -> Vec<String> {
    paths
        .iter()
        .filter(|path| path.starts_with(".ade/") && !path.starts_with(".ade/audit/"))
        .cloned()
        .collect()
}

pub fn make_ctx(target_dir: &Path, config: AdeConfig, deps: &PipelineDeps) -> Ctx {
    build_ctx(BuildCtxOptions {
        target_dir: target_dir.to_path_buf(),
        config,
        exec: deps.exec.clone(),
        which: deps.which.clone(),
        env: None,
    })
}

/// Default harness targets for a fresh init: detected in-repo harnesses, else the opinionated pair.
pub fn default_harness_targets(repo_harnesses: &[String]) -> Vec<String> {
    if !repo_harnesses.is_empty() {
        let mut sorted = repo_harnesses.to_vec();
        sorted.sort();
        return sorted;
    }
    vec!["claude-code".to_string(), "codex".to_string()]
}

pub struct InitOutcome {
    pub created: bool,
    pub config: AdeConfig,
}

pub fn init_target(target_dir: &Path, deps: &PipelineDeps) -> Result<InitOutcome, String> {
    let ids = module_ids();
    let harness_ids: Vec<&str> = HARNESS_ADAPTERS.iter().map(|adapter| adapter.id).collect();
    let config_path = target_dir.join(CONFIG_FILE);
    match load_config(target_dir, &ids, &harness_ids) {
        Ok(config) => Ok(InitOutcome {
            created: false,
            config,
        }),
        Err(error) => {
            if config_path.exists() {
                // Present but invalid — surface the validation error rather than overwrite.
                return Err(error);
            }
            let probe_ctx = make_ctx(target_dir, default_config(&ids, &[]), deps);
            let config = default_config(&ids, &default_harness_targets(&probe_ctx.repo_harnesses));
            write_ensured(&config_path, &serialize_config(&config)).map_err(|e| e.to_string())?;
            Ok(InitOutcome {
                created: true,
                config,
            })
        }
    }
}

pub fn plan_pipeline(ctx: &Ctx) -> PlanReport {
    let enabled = enabled_ids(&ctx.config);
    let mut module_reports = Vec::new();
    for module in modules() {
        let is_enabled = enabled.contains(module.id());
        module_reports.push(PlanModule {
            id: module.id().to_string(),
            enabled: is_enabled,
            actions: if is_enabled {
                module.plan(ctx)
            } else {
                Vec::new()
            },
        });
    }
    PlanReport {
        modules: module_reports,
        core_actions: vec![
            PlannedAction {
                kind: ActionKind::Write,
                path: Some(INSTRUCTIONS_PATH.to_string()),
                description: "compose canonical instructions from enabled modules".to_string(),
            },
            PlannedAction {
                kind: ActionKind::Merge,
                path: None,
                description: "translate canonical instructions into harness instruction files (managed blocks)"
                    .to_string(),
            },
            PlannedAction {
                kind: ActionKind::Write,
                path: Some(LOCKFILE_NAME.to_string()),
                description: "write deterministic lockfile over generated .ade artifacts".to_string(),
            },
            PlannedAction {
                kind: ActionKind::Append,
                path: Some(AUDIT_LOG_PATH.to_string()),
                description: "append hash-chained audit events".to_string(),
            },
        ],
    }
}

pub fn apply_pipeline(ctx: &Ctx, deps: &PipelineDeps) -> ApplyReport {
    let enabled = enabled_ids(&ctx.config);
    let now: NowFn = deps.now.clone().unwrap_or_else(|| Arc::new(now_iso));
    let mut module_reports: Vec<ModuleRunReport> = Vec::new();
    let mut audit_events: Vec<AuditEventInput> = Vec::new();

    for module in modules() {
        if !enabled.contains(module.id()) {
            module_reports.push(ModuleRunReport {
                id: module.id().to_string(),
                title: module.title().to_string(),
                enabled: false,
                result: ModuleResult {
                    status: ModuleStatus::Skipped,
                    findings: vec![Finding::info("disabled in ade.json")],
                    wrote_paths: Vec::new(),
                },
            });
            continue;
        }
        let result = std::panic::catch_unwind(AssertUnwindSafe(|| module.apply(ctx)))
            .unwrap_or_else(|panic| ModuleResult {
                status: ModuleStatus::Failed,
                findings: vec![Finding::error(format!(
                    "module threw: {}",
                    panic_text(&panic)
                ))],
                wrote_paths: Vec::new(),
            });
        let status_text = match result.status {
            ModuleStatus::Applied => "applied",
            ModuleStatus::Skipped => "skipped",
            ModuleStatus::Degraded => "degraded",
            ModuleStatus::Failed => "failed",
        };
        audit_events.push(AuditEventInput {
            ts: now(),
            actor: "ade".to_string(),
            action: "module.apply".to_string(),
            target: module.id().to_string(),
            result: status_text.to_string(),
        });
        module_reports.push(ModuleRunReport {
            id: module.id().to_string(),
            title: module.title().to_string(),
            enabled: true,
            result,
        });
    }

    // Core-owned: canonical instructions + user-owned local instructions + translation.
    let module_list = modules();
    let blocks = collect_blocks(&module_list, &enabled);
    let generated = compose_instructions(&blocks);
    let _ = ctx.artifacts.write(INSTRUCTIONS_PATH, &generated);

    // The local file is USER-owned: created once, never overwritten, never hash-locked.
    let local_path = ctx.target_dir.join(LOCAL_INSTRUCTIONS_PATH);
    let local = read_if_exists(&local_path);
    if local.is_none() {
        let _ = write_ensured(&local_path, LOCAL_INSTRUCTIONS_STUB);
    }
    let canonical_body = compose_managed_body(
        &generated,
        Some(local.as_deref().unwrap_or(LOCAL_INSTRUCTIONS_STUB)),
    );
    let translate = translate_all(ctx, &canonical_body).unwrap_or_default();
    for file in &translate {
        audit_events.push(AuditEventInput {
            ts: now(),
            actor: "ade".to_string(),
            action: "translate".to_string(),
            target: file.path.clone(),
            result: if file.ok {
                if file.changed {
                    "updated".to_string()
                } else {
                    "unchanged".to_string()
                }
            } else {
                format!(
                    "refused: {}",
                    file.error.clone().unwrap_or_else(|| "unknown".to_string())
                )
            },
        });
    }

    audit_events.push(AuditEventInput {
        ts: now(),
        actor: "ade".to_string(),
        action: "lockfile.write".to_string(),
        target: LOCKFILE_NAME.to_string(),
        result: "ok".to_string(),
    });
    let log_path = ctx.target_dir.join(AUDIT_LOG_PATH);
    let appended = append_events(&log_path, &audit_events).unwrap_or_default();

    // Lockfile over the FULL ADE-owned tree + the audit-chain checkpoint.
    let chain = read_if_exists(&log_path)
        .and_then(|text| parse_log(&text).ok())
        .unwrap_or_default();
    let scanned = scan_ade_tree(&ctx.target_dir);
    let lock = generate_lockfile(ctx, &scanned, checkpoint_of(&chain));
    let _ = write_ensured(
        &ctx.target_dir.join(LOCKFILE_NAME),
        &serialize_lockfile(&lock),
    );

    let ok = module_reports
        .iter()
        .all(|report| report.result.status != ModuleStatus::Failed)
        && translate.iter().all(|file| file.ok);
    ApplyReport {
        ok,
        modules: module_reports,
        translate,
        lockfile_path: LOCKFILE_NAME.to_string(),
        audit_appended: appended.len(),
    }
}

fn panic_text(panic: &Box<dyn std::any::Any + Send>) -> String {
    if let Some(text) = panic.downcast_ref::<String>() {
        text.clone()
    } else if let Some(text) = panic.downcast_ref::<&str>() {
        (*text).to_string()
    } else {
        "unknown panic".to_string()
    }
}

pub fn verify_pipeline(ctx: &Ctx) -> VerifyReport {
    let enabled = enabled_ids(&ctx.config);

    let lock = load_lockfile(&ctx.target_dir);
    let (lock_ok, lock_findings) = match &lock {
        None => (
            false,
            vec![Finding::error_with(
                format!("{LOCKFILE_NAME} missing or invalid"),
                "run `ade apply`",
            )],
        ),
        Some(lock) => {
            let result = verify_against_lockfile(ctx, lock);
            (result.ok, result.findings)
        }
    };

    // Audit-chain verification against the lockfile checkpoint (catches truncation,
    // tail-drop, and a chain re-forged from the public genesis anchor).
    let mut audit_findings: Vec<Finding> = Vec::new();
    let mut audit_ok = true;
    match read_if_exists(&ctx.target_dir.join(AUDIT_LOG_PATH)) {
        None => {
            audit_ok = false;
            audit_findings.push(Finding::error_with(
                format!("audit log missing: {AUDIT_LOG_PATH}"),
                "run `ade apply`",
            ));
        }
        Some(log_text) => match parse_log(&log_text) {
            Err(_) => {
                audit_ok = false;
                audit_findings.push(Finding::error_with(
                    "audit log is not parseable JSONL",
                    "investigate tampering",
                ));
            }
            Ok(entries) => {
                let verdict = verify_chain(&entries, lock.as_ref().map(|l| &l.audit));
                if verdict.valid {
                    audit_findings.push(Finding::ok(format!(
                        "audit chain valid ({} entries)",
                        verdict.length
                    )));
                } else {
                    audit_ok = false;
                    let position = verdict
                        .broken_index
                        .map(|index| format!(" at entry {index}"))
                        .unwrap_or_default();
                    audit_findings.push(Finding::error_with(
                        format!(
                            "audit chain BROKEN ({}{position})",
                            verdict.reason.clone().unwrap_or_else(|| "unknown".to_string())
                        ),
                        "investigate tampering — the audit log or its lockfile checkpoint was altered",
                    ));
                }
            }
        },
    }

    let module_list = modules();
    let blocks = collect_blocks(&module_list, &enabled);
    let generated = compose_instructions(&blocks);
    let local = read_if_exists(&ctx.target_dir.join(LOCAL_INSTRUCTIONS_PATH));
    let translation =
        check_translation_drift(ctx, &compose_managed_body(&generated, local.as_deref()));
    let translation_ok = translation
        .iter()
        .all(|finding| finding.level != FindingLevel::Error);

    let mut module_results: Vec<ModuleVerify> = Vec::new();
    let mut modules_ok = true;
    for module in modules() {
        if !enabled.contains(module.id()) {
            continue;
        }
        let verdict = std::panic::catch_unwind(AssertUnwindSafe(|| module.verify(ctx)));
        match verdict {
            Ok(verdict) => {
                if !verdict.ok {
                    modules_ok = false;
                }
                module_results.push(ModuleVerify {
                    id: module.id().to_string(),
                    ok: verdict.ok,
                    findings: verdict.findings,
                });
            }
            Err(panic) => {
                modules_ok = false;
                module_results.push(ModuleVerify {
                    id: module.id().to_string(),
                    ok: false,
                    findings: vec![Finding::error(format!(
                        "verify threw: {}",
                        panic_text(&panic)
                    ))],
                });
            }
        }
    }

    VerifyReport {
        ok: lock_ok && audit_ok && translation_ok && modules_ok,
        lockfile: lock_findings,
        audit: audit_findings,
        translation,
        modules: module_results,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fsutil::sha256_hex;
    use crate::testutil::{
        fake_exec, fake_which, make_temp_dir, make_test_ctx, test_config, test_config_with,
        TestCtxOptions,
    };
    use std::fs;
    use std::path::PathBuf;

    fn test_deps() -> PipelineDeps {
        PipelineDeps {
            exec: fake_exec(&[("git -C", (0, "true\n", ""))]),
            which: fake_which(&[]),
            now: Some(Arc::new(|| "2026-07-12T00:00:00Z".to_string())),
        }
    }

    /// Fixture matching the TS beforeEach: temp dir + `.git/hooks` + package.json.
    fn fixture_dir(tag: &str) -> PathBuf {
        let dir = make_temp_dir(tag);
        fs::create_dir_all(dir.join(".git").join("hooks")).expect("create .git/hooks");
        fs::write(dir.join("package.json"), "{\"name\":\"fixture\"}\n")
            .expect("write package.json");
        dir
    }

    fn fixture_ctx(dir: &Path, config: AdeConfig) -> Ctx {
        make_test_ctx(
            dir,
            TestCtxOptions {
                config: Some(config),
                exec: Some(fake_exec(&[("git -C", (0, "true\n", ""))])),
                ..Default::default()
            },
        )
    }

    /// Sorted `path:sha256` listing of every file under `root` (dotfiles included).
    fn tree_snapshot(root: &Path) -> Vec<String> {
        fn walk(root: &Path, dir: &Path, out: &mut Vec<String>) {
            let Ok(entries) = fs::read_dir(dir) else {
                return;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    walk(root, &path, out);
                } else {
                    let content =
                        fs::read_to_string(&path).unwrap_or_else(|_| "<binary>".to_string());
                    let rel = path
                        .strip_prefix(root)
                        .unwrap_or(&path)
                        .to_string_lossy()
                        .replace('\\', "/");
                    out.push(format!("{rel}:{}", sha256_hex(&content)));
                }
            }
        }
        let mut out = Vec::new();
        walk(root, root, &mut out);
        out.sort();
        out
    }

    #[test]
    fn lockfile_scope_keeps_ade_artifacts_drops_audit_log_and_user_files() {
        let paths: Vec<String> = [
            ".ade/policy/a.json",
            ".ade/audit/log.jsonl",
            "CLAUDE.md",
            ".ade/instructions.md",
            ".gitignore",
        ]
        .iter()
        .map(|p| p.to_string())
        .collect();
        assert_eq!(
            lockfile_scope(&paths),
            vec![
                ".ade/policy/a.json".to_string(),
                ".ade/instructions.md".to_string()
            ]
        );
    }

    #[test]
    fn default_harness_targets_detected_sorted_else_opinionated_pair() {
        assert_eq!(
            default_harness_targets(&["cursor".to_string()]),
            vec!["cursor".to_string()]
        );
        // Detected harnesses are returned sorted.
        assert_eq!(
            default_harness_targets(&["codex".to_string(), "claude-code".to_string()]),
            vec!["claude-code".to_string(), "codex".to_string()]
        );
        assert_eq!(
            default_harness_targets(&[]),
            vec!["claude-code".to_string(), "codex".to_string()]
        );
    }

    #[test]
    fn init_target_creates_default_config_once_reuses_after_refuses_invalid() {
        let dir = fixture_dir("run-init");
        let deps = test_deps();
        let first = init_target(&dir, &deps).expect("first init");
        assert!(first.created);
        assert!(dir.join(CONFIG_FILE).exists());
        assert_eq!(
            first.config.harnesses,
            vec!["claude-code".to_string(), "codex".to_string()]
        );
        let second = init_target(&dir, &deps).expect("second init");
        assert!(!second.created);
        assert_eq!(second.config, first.config);

        // Present-but-invalid config: surfaced as an error, file NOT overwritten.
        let bad = make_temp_dir("run-init-bad");
        fs::write(bad.join(CONFIG_FILE), "{broken").expect("write invalid config");
        let error = match init_target(&bad, &deps) {
            Err(error) => error,
            Ok(_) => panic!("invalid config must error"),
        };
        assert!(error.contains("not valid JSON"), "got: {error}");
        assert_eq!(
            fs::read_to_string(bad.join(CONFIG_FILE)).expect("read back"),
            "{broken"
        );
        let _ = fs::remove_dir_all(&dir);
        let _ = fs::remove_dir_all(&bad);
    }

    #[test]
    fn plan_reports_module_and_core_actions_and_writes_nothing() {
        let dir = fixture_dir("run-plan");
        let ctx = fixture_ctx(
            &dir,
            test_config_with(&["cost-governance"], &["claude-code", "codex"]),
        );
        let before = tree_snapshot(&dir);
        let report = plan_pipeline(&ctx);
        assert_eq!(tree_snapshot(&dir), before, "plan must not touch the tree");
        assert!(!dir.join(INSTRUCTIONS_PATH).exists());

        assert_eq!(report.modules.len(), 15);
        let disabled = report
            .modules
            .iter()
            .find(|module| module.id == "cost-governance")
            .expect("cost-governance planned");
        assert!(!disabled.enabled);
        assert!(disabled.actions.is_empty());
        assert!(report
            .modules
            .iter()
            .any(|module| module.enabled && !module.actions.is_empty()));
        assert!(report
            .core_actions
            .iter()
            .any(|action| action.path.as_deref() == Some(LOCKFILE_NAME)));
        assert!(report
            .core_actions
            .iter()
            .any(|action| action.path.as_deref() == Some(AUDIT_LOG_PATH)));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn apply_composes_instructions_translates_locks_and_audits() {
        let dir = fixture_dir("run-apply");
        let ctx = fixture_ctx(&dir, test_config());
        let deps = test_deps();
        let report = apply_pipeline(&ctx, &deps);
        assert!(report.ok);
        assert_eq!(report.modules.len(), 15);
        for module in &report.modules {
            assert!(module.enabled, "{} must be enabled", module.id);
            assert!(
                matches!(
                    module.result.status,
                    ModuleStatus::Applied | ModuleStatus::Degraded
                ),
                "{} unexpectedly {:?}",
                module.id,
                module.result.status
            );
        }

        let canonical =
            fs::read_to_string(dir.join(INSTRUCTIONS_PATH)).expect("canonical instructions");
        assert!(canonical.contains("# ADE Baseline Instructions"));
        assert!(canonical.contains("Secrets & Credential Hygiene"));
        let claude = fs::read_to_string(dir.join("CLAUDE.md")).expect("CLAUDE.md");
        assert!(claude.contains("<!-- ade:begin -->"));
        let agents = fs::read_to_string(dir.join("AGENTS.md")).expect("AGENTS.md");
        assert!(agents.contains("Cost & Token Budget"));

        // Lockfile written + audit chain valid with module.apply/lockfile.write events.
        assert!(load_lockfile(&dir).is_some());
        assert!(report.audit_appended > 0);
        let entries = parse_log(&fs::read_to_string(dir.join(AUDIT_LOG_PATH)).expect("audit log"))
            .expect("parseable chain");
        assert!(verify_chain(&entries, None).valid);
        assert!(entries
            .iter()
            .any(|entry| entry.action == "module.apply" && entry.target == "secrets"));
        assert!(entries.iter().any(|entry| entry.action == "lockfile.write"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn apply_with_default_clock_stamps_parseable_timestamps() {
        let dir = fixture_dir("run-apply-clock");
        let ctx = fixture_ctx(&dir, test_config());
        let deps = PipelineDeps {
            exec: fake_exec(&[("git -C", (0, "true\n", ""))]),
            which: fake_which(&[]),
            now: None,
        };
        let report = apply_pipeline(&ctx, &deps);
        assert!(report.ok);
        let entries = parse_log(&fs::read_to_string(dir.join(AUDIT_LOG_PATH)).expect("audit log"))
            .expect("parseable chain");
        assert!(!entries.is_empty());
        for entry in &entries {
            assert!(
                entry.ts.ends_with('Z') && entry.ts.contains('T') && entry.ts.contains('.'),
                "real-clock timestamp must be ISO-like: {}",
                entry.ts
            );
        }
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn double_apply_is_byte_stable_while_audit_checkpoint_advances() {
        let dir = fixture_dir("run-idem");
        let deps = test_deps();
        apply_pipeline(&fixture_ctx(&dir, test_config()), &deps);
        let lock_first = load_lockfile(&dir).expect("first lockfile");
        let claude_first = fs::read_to_string(dir.join("CLAUDE.md")).expect("CLAUDE.md");

        apply_pipeline(&fixture_ctx(&dir, test_config()), &deps);
        let lock_second = load_lockfile(&dir).expect("second lockfile");

        // Content hashes, environment, and harnesses are byte-stable...
        assert_eq!(lock_second.files, lock_first.files);
        assert_eq!(lock_second.environment, lock_first.environment);
        assert_eq!(lock_second.harnesses, lock_first.harnesses);
        assert_eq!(
            fs::read_to_string(dir.join("CLAUDE.md")).expect("CLAUDE.md again"),
            claude_first
        );
        // ...and the audit checkpoint legitimately advances: the second apply
        // was itself logged. A chain that did NOT grow would mean apply went unaudited.
        assert!(lock_second.audit.length > lock_first.audit.length);
        assert_ne!(lock_second.audit.head_hash, lock_first.audit.head_hash);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn disabled_module_is_skipped_by_apply_and_writes_nothing() {
        let dir = fixture_dir("run-skip");
        let config = test_config_with(&["cost-governance"], &["claude-code", "codex"]);
        let report = apply_pipeline(&fixture_ctx(&dir, config), &test_deps());
        let skipped = report
            .modules
            .iter()
            .find(|module| module.id == "cost-governance")
            .expect("cost-governance reported");
        assert!(!skipped.enabled);
        assert_eq!(skipped.result.status, ModuleStatus::Skipped);
        assert!(skipped
            .result
            .findings
            .iter()
            .any(|finding| finding.message == "disabled in ade.json"));
        assert!(!dir.join(".ade/policy/budget.json").exists());
        let _ = fs::remove_dir_all(&dir);
    }

    #[cfg(unix)]
    #[test]
    fn apply_reports_translate_refusal_on_symlinked_instruction_file() {
        let dir = fixture_dir("run-refuse");
        std::os::unix::fs::symlink(dir.join("nowhere"), dir.join("CLAUDE.md"))
            .expect("create symlink");
        let report = apply_pipeline(&fixture_ctx(&dir, test_config()), &test_deps());
        assert!(!report.ok, "refused translation must fail the apply");
        let refused = report
            .translate
            .iter()
            .find(|file| file.path == "CLAUDE.md")
            .expect("CLAUDE.md translate result");
        assert!(!refused.ok);
        // The refusal is audited verbatim.
        let entries = parse_log(&fs::read_to_string(dir.join(AUDIT_LOG_PATH)).expect("audit log"))
            .expect("parseable chain");
        assert!(entries.iter().any(|entry| {
            entry.action == "translate"
                && entry.target == "CLAUDE.md"
                && entry.result.starts_with("refused:")
        }));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn verify_green_after_apply_and_fails_naming_a_tampered_file() {
        let dir = fixture_dir("run-verify");
        let deps = test_deps();
        apply_pipeline(&fixture_ctx(&dir, test_config()), &deps);
        let clean = verify_pipeline(&fixture_ctx(&dir, test_config()));
        assert!(
            clean.ok,
            "verify must be green after apply: lock={:?} audit={:?} translation={:?} modules={:?}",
            clean.lockfile,
            clean.audit,
            clean.translation,
            clean
                .modules
                .iter()
                .filter(|m| !m.ok)
                .map(|m| (&m.id, &m.findings))
                .collect::<Vec<_>>()
        );

        fs::write(dir.join(".ade/policy/budget.json"), "{\"tampered\":true}\n")
            .expect("tamper policy");
        let tampered = verify_pipeline(&fixture_ctx(&dir, test_config()));
        assert!(!tampered.ok);
        assert!(tampered
            .lockfile
            .iter()
            .any(|finding| finding.message.contains("budget.json")));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn verify_fails_with_missing_lockfile_and_missing_audit_log() {
        let dir = fixture_dir("run-verify-missing");
        let report = verify_pipeline(&fixture_ctx(&dir, test_config()));
        assert!(!report.ok);
        assert!(report.lockfile[0].message.contains(LOCKFILE_NAME));
        assert_eq!(report.lockfile[0].level, FindingLevel::Error);
        assert!(report
            .audit
            .iter()
            .any(|finding| finding.message.contains("audit log missing")));
        let _ = fs::remove_dir_all(&dir);
    }

    /// ISC-211: `remove` is the inverse of `init` + `apply`.
    ///
    /// This is the claim that makes ADE safe to try: whatever it wrote, it can
    /// take back, leaving the repo byte-identical to how it was found. Anything
    /// weaker (a stray file, a mangled CLAUDE.md) means "just try it" was a lie.
    /// It also guards against drift — a module that starts merging something new
    /// into a co-owned file without teaching `remove` about it fails right here.
    #[test]
    fn remove_restores_the_repo_to_its_pre_init_state() {
        use crate::remove::remove_pipeline;

        let dir = fixture_dir("run-remove-roundtrip");
        // Pre-existing user content in the very files ADE co-owns.
        fs::write(dir.join("CLAUDE.md"), "# My project\n\nMy own notes.\n").expect("claude");
        fs::write(dir.join(".gitignore"), "node_modules/\ndist/\n").expect("gitignore");
        fs::create_dir_all(dir.join(".claude")).expect("claude dir");
        fs::write(
            dir.join(".claude/settings.json"),
            "{\n  \"permissions\": {\n    \"allow\": [\n      \"Read(./src/**)\"\n    ]\n  }\n}\n",
        )
        .expect("settings");
        let before = tree_snapshot(&dir);

        let deps = test_deps();
        init_target(&dir, &deps).expect("init");
        let ctx = fixture_ctx(&dir, test_config());
        apply_pipeline(&ctx, &deps);
        assert_ne!(tree_snapshot(&dir), before, "apply must change the tree");

        // A dry run decides everything and writes nothing.
        let planned = remove_pipeline(&ctx, false);
        assert!(!planned.executed);
        assert!(planned.deleted() > 0);
        assert_eq!(
            tree_snapshot(&dir),
            tree_snapshot(&dir),
            "snapshot helper is stable"
        );
        let after_dry_run = tree_snapshot(&dir);

        let report = remove_pipeline(&ctx, true);
        assert!(report.executed);
        assert_eq!(
            report.actions.len(),
            planned.actions.len(),
            "the dry run must plan exactly what the real run does"
        );
        assert_ne!(after_dry_run, tree_snapshot(&dir));

        assert_eq!(
            tree_snapshot(&dir),
            before,
            "remove must leave the repo exactly as it was found"
        );
        // A file-hash snapshot cannot see empty directories, and an abandoned
        // `.ade/` is exactly the kind of residue that makes an uninstall feel
        // unfinished — assert on the directories too.
        assert!(
            !dir.join(".ade").exists(),
            "no empty .ade/ may be left behind"
        );
        assert!(
            !dir.join(".cursor").exists(),
            "no empty .cursor/ may be left behind"
        );
        // …but a directory the USER owns content in survives untouched.
        assert!(
            dir.join(".claude/settings.json").exists(),
            "the user's own settings must still be there"
        );

        // Idempotent: a second removal finds nothing and changes nothing.
        let again = remove_pipeline(&ctx, true);
        assert!(
            again.is_noop(),
            "second remove should be a no-op: {again:?}"
        );
        assert_eq!(tree_snapshot(&dir), before);
        let _ = fs::remove_dir_all(&dir);
    }

    /// ISC-216: a pre-existing git hook that `apply` chained aside is put BACK.
    ///
    /// Deleting ade's shim and leaving `pre-commit.pre-ade` orphaned would
    /// silently disable the user's own hook — the repo would look clean while
    /// quietly having lost a gate it relied on.
    #[test]
    fn remove_restores_a_git_hook_that_apply_chained_aside() {
        use crate::modules::secrets::CHAINED_HOOK_NAME;
        use crate::remove::remove_pipeline;

        let dir = fixture_dir("run-remove-hook");
        let hooks = dir.join(".git/hooks");
        let user_hook = "#!/bin/sh\necho my own pre-commit\n";
        fs::write(hooks.join("pre-commit"), user_hook).expect("user hook");
        let before = tree_snapshot(&dir);

        let deps = test_deps();
        init_target(&dir, &deps).expect("init");
        let ctx = fixture_ctx(&dir, test_config());
        apply_pipeline(&ctx, &deps);
        assert!(
            hooks.join(CHAINED_HOOK_NAME).exists(),
            "apply should have chained the user hook aside"
        );

        remove_pipeline(&ctx, true);
        assert_eq!(
            fs::read_to_string(hooks.join("pre-commit")).expect("hook restored"),
            user_hook,
            "the user's original hook must be back in place"
        );
        assert!(
            !hooks.join(CHAINED_HOOK_NAME).exists(),
            "no orphaned .pre-ade copy should remain"
        );
        assert_eq!(tree_snapshot(&dir), before);
        let _ = fs::remove_dir_all(&dir);
    }

    /// ISC-213/214/215: removal refuses to destroy what it cannot prove it wrote.
    #[test]
    fn remove_keeps_hand_edited_planted_and_user_authored_files() {
        use crate::remove::{remove_pipeline, RemoveOutcome};

        let dir = fixture_dir("run-remove-keeps");
        let deps = test_deps();
        init_target(&dir, &deps).expect("init");
        let ctx = fixture_ctx(&dir, test_config());
        apply_pipeline(&ctx, &deps);

        // (a) hand-edit a locked ADE file, (b) plant an unknown file in the
        // ADE tree, (c) make instructions.local.md genuinely yours.
        let edited = dir.join(".ade/policy/secrets.json");
        fs::write(&edited, "{\n  \"hand\": \"edited\"\n}\n").expect("edit policy");
        let planted = dir.join(".ade/guardrails/mine.md");
        fs::write(&planted, "my own rule\n").expect("plant");
        let local = dir.join(LOCAL_INSTRUCTIONS_PATH);
        fs::write(&local, "# my project rules\n\nnever deploy on fridays\n").expect("local");

        let report = remove_pipeline(&ctx, true);
        let kept: Vec<&str> = report
            .actions
            .iter()
            .filter(|action| action.outcome == RemoveOutcome::Kept)
            .map(|action| action.path.as_str())
            .collect();
        assert!(kept.contains(&".ade/policy/secrets.json"), "{kept:?}");
        assert!(kept.contains(&".ade/guardrails/mine.md"), "{kept:?}");
        assert!(kept.contains(&LOCAL_INSTRUCTIONS_PATH), "{kept:?}");

        assert!(edited.exists(), "hand-edited file must survive");
        assert!(planted.exists(), "planted file must survive");
        assert_eq!(
            fs::read_to_string(&local).expect("local survives"),
            "# my project rules\n\nnever deploy on fridays\n"
        );
        let _ = fs::remove_dir_all(&dir);
    }

    /// ISC-177 (Rust port of the v0.1 anti-test): a full bootstrap must never
    /// copy an environment value into anything it writes. Env is where secrets
    /// live, and every file here is destined for a git repo.
    #[test]
    fn planted_env_secret_never_reaches_any_generated_file() {
        let planted = "PLANTED_SUPER_SECRET_VALUE_XYZZY_99";
        let previous = std::env::var("ADE_TEST_SECRET").ok();
        // SAFETY: single-threaded within this test; restored below.
        unsafe { std::env::set_var("ADE_TEST_SECRET", planted) };

        let dir = fixture_dir("run-planted-secret");
        let deps = test_deps();
        init_target(&dir, &deps).expect("init");
        apply_pipeline(&fixture_ctx(&dir, test_config()), &deps);

        let mut checked = 0usize;
        let mut stack = vec![dir.clone()];
        while let Some(current) = stack.pop() {
            for entry in fs::read_dir(&current).expect("read dir").flatten() {
                let path = entry.path();
                if path.is_dir() {
                    stack.push(path);
                    continue;
                }
                if let Ok(content) = fs::read_to_string(&path) {
                    assert!(
                        !content.contains(planted),
                        "planted secret leaked into {}",
                        path.display()
                    );
                    checked += 1;
                }
            }
        }
        assert!(checked > 10, "expected a full tree, saw {checked} files");

        match previous {
            Some(value) => unsafe { std::env::set_var("ADE_TEST_SECRET", value) },
            None => unsafe { std::env::remove_var("ADE_TEST_SECRET") },
        }
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn verify_flags_truncated_and_unparseable_audit_logs() {
        let dir = fixture_dir("run-verify-audit");
        apply_pipeline(&fixture_ctx(&dir, test_config()), &test_deps());
        let log_path = dir.join(AUDIT_LOG_PATH);

        // Truncation-to-empty: caught via the lockfile checkpoint.
        fs::write(&log_path, "").expect("truncate log");
        let truncated = verify_pipeline(&fixture_ctx(&dir, test_config()));
        assert!(!truncated.ok);
        assert!(truncated
            .audit
            .iter()
            .any(|finding| finding.message.contains("audit chain BROKEN")));

        // Unparseable JSONL is a distinct failure.
        fs::write(&log_path, "not json at all\n").expect("corrupt log");
        let corrupt = verify_pipeline(&fixture_ctx(&dir, test_config()));
        assert!(!corrupt.ok);
        assert!(corrupt
            .audit
            .iter()
            .any(|finding| finding.message.contains("not parseable JSONL")));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn verify_skips_disabled_modules_in_module_results() {
        let dir = fixture_dir("run-verify-disabled");
        let config = test_config_with(&["sandbox"], &["claude-code", "codex"]);
        apply_pipeline(&fixture_ctx(&dir, config.clone()), &test_deps());
        let report = verify_pipeline(&fixture_ctx(&dir, config));
        assert!(!report.modules.iter().any(|module| module.id == "sandbox"));
        assert_eq!(report.modules.len(), 14);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn panic_containment_holds_across_the_real_registry() {
        // The registry is static (no bomb-module injection as in the TS oracle):
        // instead prove the catch_unwind wrapper lets a full real-registry apply
        // and verify complete without any module escaping as a panic.
        let dir = fixture_dir("run-contain");
        let report = apply_pipeline(&fixture_ctx(&dir, test_config()), &test_deps());
        assert!(report
            .modules
            .iter()
            .all(|module| module.result.status != ModuleStatus::Failed));
        let verify = verify_pipeline(&fixture_ctx(&dir, test_config()));
        assert_eq!(verify.modules.len(), 15);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn panic_text_formats_string_str_and_unknown_payloads() {
        let string_panic: Box<dyn std::any::Any + Send> = Box::new("kaboom".to_string());
        assert_eq!(panic_text(&string_panic), "kaboom");
        let str_panic: Box<dyn std::any::Any + Send> = Box::new("kaboom-str");
        assert_eq!(panic_text(&str_panic), "kaboom-str");
        let opaque_panic: Box<dyn std::any::Any + Send> = Box::new(42_i32);
        assert_eq!(panic_text(&opaque_panic), "unknown panic");
    }
}
