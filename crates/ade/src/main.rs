//! `ade` — ADE Bootstrapper CLI (Rust port of the TS v0.1 `src/cli.ts`).
//! Exit codes: 0 success · 1 failure · 2 usage error.
//! `--json` emits a single JSON document on stdout; human output goes to stderr.

use ade_core::audit::checkpoint_of;
use ade_core::audit::{parse_log, verify_chain};
use ade_core::config::load_config;
use ade_core::exec::{real_exec, real_which};
use ade_core::fsutil::{read_if_exists, write_ensured};
use ade_core::gui::install::{gui_install, gui_status, gui_uninstall, InstallDeps};
use ade_core::gui::inventory::{detect_capabilities, DetectOptions};
use ade_core::gui::jobs::read_last_finished;
use ade_core::gui::state::{ade_home_from_env, load_gui_state};
use ade_core::gui::verdict::{build_verdict, render_health_text};
use ade_core::harness::adapters::HARNESS_ADAPTERS;
use ade_core::hook::hook_append;
use ade_core::instructions::{
    collect_blocks, compose_instructions, compose_managed_body, INSTRUCTIONS_PATH,
    LOCAL_INSTRUCTIONS_PATH,
};
use ade_core::lockfile::{
    generate_lockfile, load_lockfile, scan_ade_tree, serialize_lockfile, LOCKFILE_NAME,
};
use ade_core::modules::injection_defense::scan_text;
use ade_core::registry::{module_ids, modules};
use ade_core::remove::{remove_pipeline, RemoveOutcome};
use ade_core::report::{doctor_report, status_report};
use ade_core::run::{
    apply_pipeline, init_target, make_ctx, plan_pipeline, verify_pipeline, ApplyReport,
    PipelineDeps, AUDIT_LOG_PATH,
};
use ade_core::translate::translate_all;
use ade_core::types::Finding;
use ade_core::version::ADE_VERSION;
use std::io::Read;
use std::path::{Path, PathBuf};

const COMMANDS: &[(&str, &str)] = &[
    (
        "init [dir]",
        "bootstrap an ADE in the target repository (config + modules + lockfile)",
    ),
    (
        "plan",
        "dry-run: show every action apply would take (writes nothing)",
    ),
    (
        "apply",
        "apply all enabled modules, retranslate instructions, refresh lockfile",
    ),
    (
        "verify",
        "verify on-disk state against the lockfile, canonical instructions, and module checks",
    ),
    ("status", "per-module status summary"),
    (
        "doctor",
        "report integrated tools, harnesses, and environment health",
    ),
    ("modules", "list all modules with enabled state"),
    (
        "translate",
        "regenerate harness instruction files from .ade/instructions.md",
    ),
    (
        "lock",
        "regenerate ade.lock.json from current on-disk artifacts",
    ),
    (
        "audit verify",
        "validate the tamper-evident audit log hash chain",
    ),
    (
        "remove",
        "withdraw ade from this repo (prints the plan; --yes to carry it out)",
    ),
    (
        "gui install",
        "build + install the native Control Center apps and tray agent (macOS)",
    ),
    ("gui uninstall", "remove the ADE apps and tray agent"),
    ("gui status", "report the tray agent's launchd state"),
    (
        "gui health",
        "state whether this machine's agent environment is sound, and what needs doing",
    ),
    (
        "hook append",
        "append a harness hook event to the audit chain (stdin JSON)",
    ),
    (
        "hook scan",
        "scan stdin for prompt-injection patterns (exit 1 when flagged)",
    ),
    ("version", "print the ade version"),
    ("help", "show this help"),
];

fn usage() -> String {
    let mut lines = vec![
        format!("ade {ADE_VERSION} — bootstrap a secure Agentic Development Environment"),
        String::new(),
        "Usage: ade <command> [--dir <path>] [--json]".to_string(),
        String::new(),
    ];
    for (command, description) in COMMANDS {
        lines.push(format!("  ade {command:<14} {description}"));
    }
    lines.push(String::new());
    lines.join("\n")
}

struct Parsed {
    command: String,
    positionals: Vec<String>,
    json: bool,
    dir: Option<String>,
    extras: Vec<String>,
    /// Explicit go-ahead for a destructive command (`remove`).
    yes: bool,
    error: Option<String>,
}

fn parse_args(argv: &[String]) -> Parsed {
    let mut positionals: Vec<String> = Vec::new();
    let mut json = false;
    let mut dir: Option<String> = None;
    let mut extras: Vec<String> = Vec::new();
    let mut yes = false;
    let mut error: Option<String> = None;
    let mut index = 0;
    while index < argv.len() {
        let arg = &argv[index];
        if arg == "--json" {
            json = true;
        } else if arg == "--yes" {
            yes = true;
        } else if arg == "--dir" || arg == "-C" {
            match argv.get(index + 1) {
                None => error = Some("--dir requires a path argument".to_string()),
                Some(value) => {
                    dir = Some(value.clone());
                    index += 1;
                }
            }
        } else if arg == "--extra" {
            match argv.get(index + 1) {
                None => error = Some("--extra requires a pattern argument".to_string()),
                Some(value) => {
                    extras.push(value.clone());
                    index += 1;
                }
            }
        } else if arg == "--help" || arg == "-h" {
            positionals.insert(0, "help".to_string());
        } else if arg.starts_with('-') {
            error = Some(format!("unknown flag: {arg}"));
        } else {
            positionals.push(arg.clone());
        }
        index += 1;
    }
    let command = if positionals.is_empty() {
        "help".to_string()
    } else {
        positionals.remove(0)
    };
    Parsed {
        command,
        positionals,
        json,
        dir,
        extras,
        yes,
        error,
    }
}

fn emit(json: bool, payload: &serde_json::Value, human: &str) {
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(payload).unwrap_or_else(|_| "{}".to_string())
        );
    } else {
        println!("{human}");
    }
}

fn summarize_findings(prefix: &str, findings: &[Finding]) -> String {
    findings
        .iter()
        .map(|finding| {
            let level = serde_json::to_value(finding.level)
                .ok()
                .and_then(|value| value.as_str().map(String::from))
                .unwrap_or_default();
            let remediation = finding
                .remediation
                .as_ref()
                .map(|text| format!(" → {text}"))
                .unwrap_or_default();
            format!("{prefix}[{level}] {}{remediation}", finding.message)
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn pipeline_deps() -> PipelineDeps {
    PipelineDeps {
        exec: real_exec(),
        which: real_which(),
        now: None,
    }
}

fn harness_ids() -> Vec<&'static str> {
    HARNESS_ADAPTERS.iter().map(|adapter| adapter.id).collect()
}

fn load_ctx(target_dir: &Path, json: bool) -> Option<ade_core::types::Ctx> {
    match load_config(target_dir, &module_ids(), &harness_ids()) {
        Ok(config) => Some(make_ctx(target_dir, config, &pipeline_deps())),
        Err(error) => {
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&serde_json::json!({"ok": false, "error": error}))
                        .unwrap_or_default()
                );
            } else {
                eprintln!("ade: {error}");
            }
            None
        }
    }
}

fn report_payload(report: &ApplyReport) -> serde_json::Value {
    serde_json::json!({
        "modules": report.modules.iter().map(|module| serde_json::json!({
            "id": module.id,
            "enabled": module.enabled,
            "status": module.result.status,
            "findings": module.result.findings,
        })).collect::<Vec<_>>(),
        "translate": report.translate,
        "lockfile": report.lockfile_path,
        "auditEventsAppended": report.audit_appended,
    })
}

fn human_apply(verb: &str, report: &ApplyReport, created: bool) -> String {
    use ade_core::types::ModuleStatus;
    let mut lines = vec![format!(
        "ade {verb}: {}{}",
        if report.ok { "OK" } else { "FAILED" },
        if created { " (created ade.json)" } else { "" }
    )];
    for module in &report.modules {
        let marker = match module.result.status {
            ModuleStatus::Failed => "✗",
            ModuleStatus::Degraded => "◐",
            _ if module.enabled => "✓",
            _ => "·",
        };
        let status = serde_json::to_value(module.result.status)
            .ok()
            .and_then(|value| value.as_str().map(String::from))
            .unwrap_or_default();
        lines.push(format!("  {marker} {status:<9} {}", module.id));
    }
    for file in &report.translate {
        let error = file
            .error
            .as_ref()
            .map(|text| format!(" — {text}"))
            .unwrap_or_default();
        lines.push(format!(
            "  {} translate {}{error}",
            if file.ok { "✓" } else { "✗" },
            file.path
        ));
    }
    lines.push(format!(
        "  lockfile: {} · audit: +{} events",
        report.lockfile_path, report.audit_appended
    ));
    lines.join("\n")
}

fn install_deps() -> InstallDeps {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
    let home_path = PathBuf::from(&home);
    InstallDeps {
        exec: real_exec(),
        repo_root: std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
        apps_dir: home_path.join("Applications"),
        launch_agents_dir: home_path.join("Library/LaunchAgents"),
        bin_dir: home_path.join(".local/bin"),
        ade_home: ade_home_from_env(),
        uid: None,
        skip_build: false,
        settle_interval: ade_core::gui::install::SERVICE_POLL_INTERVAL,
        home_dir: home_path,
    }
}

fn install_report_output(
    verb: &str,
    report: &ade_core::gui::install::InstallReport,
    json: bool,
) -> i32 {
    let payload = serde_json::json!({
        "ok": report.ok,
        "steps": report.steps.iter().map(|step| serde_json::json!({
            "step": step.step, "ok": step.ok, "detail": step.detail,
        })).collect::<Vec<_>>(),
    });
    let mut lines = vec![format!(
        "ade gui {verb}: {}",
        if report.ok { "OK" } else { "FAILED" }
    )];
    for step in &report.steps {
        let detail = step
            .detail
            .as_ref()
            .map(|text| format!(" — {text}"))
            .unwrap_or_default();
        lines.push(format!(
            "  {} {}{detail}",
            if step.ok { "✓" } else { "✗" },
            step.step
        ));
    }
    emit(json, &payload, &lines.join("\n"));
    if report.ok {
        0
    } else {
        1
    }
}

fn read_stdin() -> String {
    let mut buffer = String::new();
    let _ = std::io::stdin().read_to_string(&mut buffer);
    buffer
}

fn main() {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    std::process::exit(run(&argv));
}

fn run(argv: &[String]) -> i32 {
    let parsed = parse_args(argv);
    if let Some(error) = parsed.error {
        eprintln!("ade: {error}\n\n{}", usage());
        return 2;
    }
    let positional_dir = if parsed.command == "init" {
        parsed.positionals.first().cloned()
    } else {
        None
    };
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let target_dir = match parsed.dir.clone().or(positional_dir) {
        Some(dir) => {
            let path = PathBuf::from(&dir);
            if path.is_absolute() {
                path
            } else {
                cwd.join(path)
            }
        }
        None => cwd.clone(),
    };
    let json = parsed.json;

    match parsed.command.as_str() {
        "help" => {
            print!("{}", usage());
            0
        }
        "version" => {
            emit(
                json,
                &serde_json::json!({"version": ADE_VERSION}),
                ADE_VERSION,
            );
            0
        }
        "init" => {
            if !target_dir.is_dir() {
                eprintln!(
                    "ade: target directory does not exist: {}",
                    target_dir.display()
                );
                return 1;
            }
            let deps = pipeline_deps();
            let outcome = match init_target(&target_dir, &deps) {
                Ok(outcome) => outcome,
                Err(error) => {
                    eprintln!("ade: {error}");
                    return 1;
                }
            };
            let ctx = make_ctx(&target_dir, outcome.config, &deps);
            let report = apply_pipeline(&ctx, &deps);
            let mut payload = report_payload(&report);
            payload["ok"] = serde_json::json!(report.ok);
            payload["created"] = serde_json::json!(outcome.created);
            emit(
                json,
                &payload,
                &human_apply("init", &report, outcome.created),
            );
            if report.ok {
                0
            } else {
                1
            }
        }
        "apply" => {
            let Some(ctx) = load_ctx(&target_dir, json) else {
                return 1;
            };
            let deps = pipeline_deps();
            let report = apply_pipeline(&ctx, &deps);
            let mut payload = report_payload(&report);
            payload["ok"] = serde_json::json!(report.ok);
            emit(json, &payload, &human_apply("apply", &report, false));
            if report.ok {
                0
            } else {
                1
            }
        }
        "plan" => {
            let Some(ctx) = load_ctx(&target_dir, json) else {
                return 1;
            };
            let report = plan_pipeline(&ctx);
            let mut human = vec!["ade plan (dry-run — nothing written):".to_string()];
            for module in &report.modules {
                if module.enabled {
                    for action in &module.actions {
                        let kind = serde_json::to_value(action.kind)
                            .ok()
                            .and_then(|value| value.as_str().map(String::from))
                            .unwrap_or_default();
                        let path = action
                            .path
                            .as_ref()
                            .map(|p| format!(" {p}"))
                            .unwrap_or_default();
                        human.push(format!(
                            "  [{}] {kind}{path} — {}",
                            module.id, action.description
                        ));
                    }
                } else {
                    human.push(format!("  [{}] skipped (disabled)", module.id));
                }
            }
            for action in &report.core_actions {
                let kind = serde_json::to_value(action.kind)
                    .ok()
                    .and_then(|value| value.as_str().map(String::from))
                    .unwrap_or_default();
                let path = action
                    .path
                    .as_ref()
                    .map(|p| format!(" {p}"))
                    .unwrap_or_default();
                human.push(format!("  [core] {kind}{path} — {}", action.description));
            }
            emit(
                json,
                &serde_json::to_value(&report).unwrap_or(serde_json::Value::Null),
                &human.join("\n"),
            );
            0
        }
        "verify" => {
            let Some(ctx) = load_ctx(&target_dir, json) else {
                return 1;
            };
            let report = verify_pipeline(&ctx);
            let mut human = vec![format!(
                "ade verify: {}",
                if report.ok { "PASS" } else { "FAIL" }
            )];
            human.push(summarize_findings("  lock: ", &report.lockfile));
            human.push(summarize_findings("  audit: ", &report.audit));
            human.push(summarize_findings("  instructions: ", &report.translation));
            for module in &report.modules {
                human.push(format!(
                    "  {} {}\n{}",
                    if module.ok { "✓" } else { "✗" },
                    module.id,
                    summarize_findings("    ", &module.findings)
                ));
            }
            emit(
                json,
                &serde_json::to_value(&report).unwrap_or(serde_json::Value::Null),
                &human.join("\n"),
            );
            if report.ok {
                0
            } else {
                1
            }
        }
        "status" => {
            let Some(ctx) = load_ctx(&target_dir, json) else {
                return 1;
            };
            let rows = status_report(&ctx);
            let payload = serde_json::json!({
                "modules": rows.iter().map(|row| serde_json::json!({
                    "id": row.id, "title": row.title, "enabled": row.enabled, "state": row.state,
                })).collect::<Vec<_>>(),
            });
            let mut human = vec!["ade status:".to_string()];
            for row in &rows {
                human.push(format!("  {:<12} {} — {}", row.state, row.id, row.title));
            }
            emit(json, &payload, &human.join("\n"));
            0
        }
        "doctor" => {
            let report = doctor_report(&target_dir, &real_which(), &real_exec());
            let mut human = vec![
                format!("ade doctor — {}", report.target_dir),
                format!(
                    "  git repository: {}",
                    if report.git_repo {
                        "yes"
                    } else {
                        "NO — run git init"
                    }
                ),
                "  tools:".to_string(),
            ];
            for tool in &report.tools {
                let version = tool
                    .version
                    .as_ref()
                    .map(|v| format!(" ({v})"))
                    .unwrap_or_default();
                human.push(format!(
                    "    {} {}{version}",
                    if tool.present { "✓" } else { "✗" },
                    tool.name
                ));
            }
            human.push(format!(
                "  harnesses configured in repo: {}",
                if report.harnesses.repo.is_empty() {
                    "none".to_string()
                } else {
                    report.harnesses.repo.join(", ")
                }
            ));
            human.push(format!(
                "  harness CLIs on machine: {}",
                if report.harnesses.machine.is_empty() {
                    "none".to_string()
                } else {
                    report.harnesses.machine.join(", ")
                }
            ));
            emit(
                json,
                &serde_json::to_value(&report).unwrap_or(serde_json::Value::Null),
                &human.join("\n"),
            );
            0
        }
        "modules" => {
            let config = load_config(&target_dir, &module_ids(), &harness_ids()).ok();
            let mut rows = Vec::new();
            let mut human = vec!["ade modules:".to_string()];
            for module in modules() {
                let enabled = match &config {
                    Some(config) => config
                        .modules
                        .get(module.id())
                        .map(|m| m.enabled)
                        .unwrap_or(false),
                    None => module.default_enabled(),
                };
                rows.push(serde_json::json!({
                    "id": module.id(),
                    "title": module.title(),
                    "category": module.category(),
                    "spec": module.spec(),
                    "enabled": enabled,
                }));
                human.push(format!(
                    "  {} {:<18} {}",
                    if enabled { "on " } else { "off" },
                    module.id(),
                    module.title()
                ));
            }
            emit(
                json,
                &serde_json::json!({"modules": rows}),
                &human.join("\n"),
            );
            0
        }
        "translate" => {
            let Some(ctx) = load_ctx(&target_dir, json) else {
                return 1;
            };
            let enabled: std::collections::BTreeSet<String> = ctx
                .config
                .modules
                .iter()
                .filter(|(_, m)| m.enabled)
                .map(|(id, _)| id.clone())
                .collect();
            let module_list = modules();
            let generated = compose_instructions(&collect_blocks(&module_list, &enabled));
            let _ = ctx.artifacts.write(INSTRUCTIONS_PATH, &generated);
            // The local file is user-owned: read it, never overwrite it.
            let local = read_if_exists(&ctx.target_dir.join(LOCAL_INSTRUCTIONS_PATH));
            let body = compose_managed_body(&generated, local.as_deref());
            let results = translate_all(&ctx, &body).unwrap_or_default();
            let ok = results.iter().all(|result| result.ok);
            let mut human = vec![format!(
                "ade translate: {}",
                if ok { "OK" } else { "REFUSED" }
            )];
            for result in &results {
                let marker = if result.ok {
                    if result.changed {
                        "updated "
                    } else {
                        "unchanged"
                    }
                } else {
                    "REFUSED "
                };
                let error = result
                    .error
                    .as_ref()
                    .map(|text| format!(" — {text}"))
                    .unwrap_or_default();
                human.push(format!("  {marker} {}{error}", result.path));
            }
            emit(
                json,
                &serde_json::json!({"ok": ok, "files": results}),
                &human.join("\n"),
            );
            if ok {
                0
            } else {
                1
            }
        }
        "remove" => {
            let Some(ctx) = load_ctx(&target_dir, json) else {
                return 1;
            };
            // Destructive, so it defaults to showing its work: no `--yes`, no
            // writes. Same shape as the Control Center's two-step uninstall.
            let report = remove_pipeline(&ctx, parsed.yes);
            let mut human: Vec<String> = Vec::new();
            for action in &report.actions {
                let verb = match action.outcome {
                    RemoveOutcome::Deleted => "delete",
                    RemoveOutcome::Excised => "edit  ",
                    RemoveOutcome::Kept => "keep  ",
                };
                human.push(format!("  {verb} {} — {}", action.path, action.detail));
            }
            let summary = format!(
                "{} to delete · {} to edit · {} kept",
                report.deleted(),
                report.excised(),
                report.kept()
            );
            if report.is_noop() {
                human.push("ade remove: nothing of ade's found here".to_string());
            } else if parsed.yes {
                human.push(format!("ade remove: done — {summary}"));
            } else {
                human.push(format!("ade remove: PLAN ONLY — {summary}"));
                human.push("re-run with --yes to carry it out".to_string());
            }
            emit(
                json,
                &serde_json::json!({
                    "ok": true,
                    "executed": report.executed,
                    "deleted": report.deleted(),
                    "excised": report.excised(),
                    "kept": report.kept(),
                    "actions": report.actions.iter().map(|action| serde_json::json!({
                        "path": action.path,
                        "outcome": match action.outcome {
                            RemoveOutcome::Deleted => "deleted",
                            RemoveOutcome::Excised => "excised",
                            RemoveOutcome::Kept => "kept",
                        },
                        "detail": action.detail,
                    })).collect::<Vec<_>>(),
                }),
                &human.join("\n"),
            );
            0
        }
        "lock" => {
            let Some(ctx) = load_ctx(&target_dir, json) else {
                return 1;
            };
            let chain = read_if_exists(&ctx.target_dir.join(AUDIT_LOG_PATH))
                .and_then(|text| parse_log(&text).ok())
                .unwrap_or_default();
            let scanned = scan_ade_tree(&ctx.target_dir);
            let lock = generate_lockfile(&ctx, &scanned, checkpoint_of(&chain));
            let count = lock.files.len();
            let _ = write_ensured(
                &ctx.target_dir.join(LOCKFILE_NAME),
                &serialize_lockfile(&lock),
            );
            emit(
                json,
                &serde_json::json!({"ok": true, "files": count}),
                &format!("ade lock: recorded {count} files"),
            );
            0
        }
        "audit" => {
            let sub = parsed.positionals.first().map(String::as_str);
            let Some(text) = read_if_exists(&target_dir.join(AUDIT_LOG_PATH)) else {
                eprintln!("ade: audit log not found at {AUDIT_LOG_PATH} — run `ade init` first");
                return 1;
            };
            let entries = match parse_log(&text) {
                Ok(entries) => entries,
                Err(_) => {
                    emit(
                        json,
                        &serde_json::json!({"valid": false, "reason": "unparseable"}),
                        "ade audit: INVALID — log is not parseable JSONL",
                    );
                    return 1;
                }
            };
            if sub == Some("show") {
                let human = entries
                    .iter()
                    .map(|entry| {
                        format!(
                            "{} {} {} → {}",
                            entry.ts, entry.action, entry.target, entry.result
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("\n");
                emit(json, &serde_json::json!({"entries": entries}), &human);
                return 0;
            }
            // Verify against the lockfile checkpoint when one exists — internal chain
            // consistency alone cannot detect truncation or a re-forged chain.
            let lock = load_lockfile(&target_dir);
            let verdict = verify_chain(&entries, lock.as_ref().map(|l| &l.audit));
            let mut payload = serde_json::to_value(&verdict).unwrap_or(serde_json::Value::Null);
            payload["checkpointUsed"] = serde_json::json!(lock.is_some());
            let human = if verdict.valid {
                format!(
                    "ade audit: chain VALID ({} entries{})",
                    verdict.length,
                    if lock.is_some() {
                        ", checkpoint matched"
                    } else {
                        ", NO lockfile checkpoint"
                    }
                )
            } else {
                format!(
                    "ade audit: chain BROKEN ({}{})",
                    verdict.reason.clone().unwrap_or_default(),
                    verdict
                        .broken_index
                        .map(|index| format!(" at entry {index}"))
                        .unwrap_or_default()
                )
            };
            emit(json, &payload, &human);
            if verdict.valid {
                0
            } else {
                1
            }
        }
        "gui" => match parsed.positionals.first().map(String::as_str) {
            Some("install") => {
                let deps = install_deps();
                if !deps.repo_root.join("scripts/bundle-apps.sh").is_file() {
                    eprintln!(
                        "ade: gui install must run from the ADE Bootstrapper repository (scripts/bundle-apps.sh not found in {})",
                        deps.repo_root.display()
                    );
                    return 1;
                }
                install_report_output("install", &gui_install(&deps), json)
            }
            Some("uninstall") => {
                install_report_output("uninstall", &gui_uninstall(&install_deps()), json)
            }
            Some("health") => {
                let home = ade_home_from_env();
                let state = load_gui_state(&home).state;
                let last_jobs = read_last_finished(&home);
                let latest = std::collections::BTreeMap::new();
                let capabilities = detect_capabilities(&DetectOptions {
                    exec: real_exec(),
                    which: real_which(),
                    disabled: &state.disabled,
                    last_jobs: &last_jobs,
                    latest_versions: &latest,
                    probe_running: true,
                    probe_timeout: std::time::Duration::from_secs(10),
                });
                let health = build_verdict(&capabilities);
                let payload = serde_json::json!({
                    "verdict": format!("{:?}", health.verdict),
                    "headline": health.headline,
                    "detail": health.detail,
                    "counts": {
                        "groupsTotal": health.counts.groups_total,
                        "groupsCovered": health.counts.groups_covered,
                        "ok": health.counts.ok,
                        "warnings": health.counts.warnings,
                        "errors": health.counts.errors,
                        "missing": health.counts.missing,
                    },
                    "coverage": health.coverage.iter().map(|row| serde_json::json!({
                        "group": row.group_id, "name": row.group_name,
                        "state": format!("{:?}", row.state),
                        "working": row.working, "enabledTotal": row.enabled_total,
                    })).collect::<Vec<_>>(),
                    "attention": health.attention.iter().map(|item| serde_json::json!({
                        "rank": format!("{:?}", item.rank),
                        "group": item.group_id, "capability": item.capability_id,
                        "title": item.title, "why": item.why,
                        "action": item.action.as_ref().map(|action| serde_json::json!({
                            "label": action.label, "capability": action.capability_id,
                            "lifecycle": action.lifecycle.as_str(), "guidance": action.guidance,
                        })),
                    })).collect::<Vec<_>>(),
                });
                emit(json, &payload, &render_health_text(&health));
                // Health is a report, not a gate: a broken environment is
                // still a successful description of one.
                0
            }
            Some("status") => {
                let agents = gui_status(&install_deps());
                let payload = serde_json::json!({
                    "agents": agents.iter().map(|agent| serde_json::json!({
                        "label": agent.label, "loaded": agent.loaded,
                        "state": agent.state, "pid": agent.pid,
                    })).collect::<Vec<_>>(),
                });
                let mut human = vec!["ade gui status:".to_string()];
                for agent in &agents {
                    let state = agent
                        .state
                        .as_ref()
                        .map(|s| format!(" — {s}"))
                        .unwrap_or_default();
                    let pid = agent.pid.map(|p| format!(" (pid {p})")).unwrap_or_default();
                    human.push(format!(
                        "  {} {}{state}{pid}",
                        if agent.loaded { "✓" } else { "✗" },
                        agent.label
                    ));
                }
                emit(json, &payload, &human.join("\n"));
                0
            }
            other => {
                eprintln!(
                    "ade: unknown gui subcommand \"{}\"\n\n{}",
                    other.unwrap_or(""),
                    usage()
                );
                2
            }
        },
        "hook" => match parsed.positionals.first().map(String::as_str) {
            Some("append") => {
                // Never blocks the harness: parse/io failures are swallowed (exit 0).
                hook_append(&target_dir, &read_stdin());
                0
            }
            Some("scan") => {
                let verdict = scan_text(&read_stdin(), &parsed.extras);
                let payload = serde_json::json!({
                    "flagged": verdict.flagged,
                    "matches": verdict.matches.iter().map(|hit| serde_json::json!({
                        "pattern": hit.pattern, "excerpt": hit.excerpt,
                    })).collect::<Vec<_>>(),
                });
                println!("{}", serde_json::to_string(&payload).unwrap_or_default());
                if verdict.flagged {
                    1
                } else {
                    0
                }
            }
            other => {
                eprintln!(
                    "ade: unknown hook subcommand \"{}\"\n\n{}",
                    other.unwrap_or(""),
                    usage()
                );
                2
            }
        },
        unknown => {
            eprintln!("ade: unknown command \"{unknown}\"\n\n{}", usage());
            2
        }
    }
}
