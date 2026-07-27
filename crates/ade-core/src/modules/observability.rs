//! Module: tamper-evident observability & audit logging.
//! Port of `src/modules/observability.ts`.
//!
//! Spec component: "Tamper-evident observability and audit logging — a
//! hash-chained, append-only audit trail of all harness tool activity so that
//! what an agent did is reconstructable and undetectable modification is
//! impossible."
//! Boundary controlled: the accountability boundary (forensics/compliance) —
//! every tool invocation leaves a chained record; editing or deleting history
//! breaks every subsequent link and is caught by `ade audit verify`.
//!
//! SANCTIONED DIVERGENCES from the TS oracle:
//! - ISC-165: the PostToolUse hook artifact keeps the oracle's path
//!   (`.ade/hooks/audit-log.ts`) but is a POSIX sh shim delegating to
//!   `ade hook append` (via `harness::claude::ade_hook_script`) instead of a
//!   self-contained bun script; the wired command is `sh …` instead of
//!   `bun …`. Entry redaction/format tests move to the `ade` CLI with the
//!   append implementation.
//! - ISC-85 (Rust reading): the TS pipeline initializes the audit chain during
//!   `ade apply`; with no pipeline owning that here yet, THIS module performs
//!   the genesis init at apply time (a single `audit.genesis` event chained
//!   from the fixed genesis anchor, only when the log is absent).

use crate::audit::{append_events, parse_log, verify_chain, AuditEventInput};
use crate::fsutil::{ensure_lines, read_if_exists};
use crate::harness::claude::{ade_hook_script, wire_claude_hook, CLAUDE_SETTINGS_PATH};
use crate::modules::shared::read_json;
use crate::types::{
    ActionKind, AdeModule, Ctx, Finding, FindingLevel, InstructionBlock, ModuleResult,
    ModuleStatus, PlannedAction, VerifyResult,
};
use crate::version::AUDIT_GENESIS;

pub const AUDIT_README_PATH: &str = ".ade/audit/README.md";
pub const AUDIT_HOOK_PATH: &str = ".ade/hooks/audit-log.ts";
pub const AUDIT_LOG_PATH: &str = ".ade/audit/log.jsonl";
/// ISC-165 divergence: the oracle wires `bun .ade/hooks/audit-log.ts`.
pub const AUDIT_HOOK_COMMAND: &str = "sh .ade/hooks/audit-log.ts";
const GITIGNORE_LABEL: &str = "ADE Bootstrapper — observability";

/// README shipped into `.ade/audit/` documenting the chain contract.
pub fn audit_readme() -> String {
    let mut out = String::from(
        r#"# ADE Audit Log

Managed by ADE Bootstrapper (observability module).

`log.jsonl` in this directory is a **tamper-evident, hash-chained audit log**
of ADE pipeline events and harness tool activity.

- Each line is one JSON entry: `{ts, actor, action, target, result, prev, hash}`.
- `hash = sha256(prev + JSON.stringify({ts, actor, action, target, result, prev}))`
  with that exact key order.
- The first entry chains from the fixed genesis value `"#,
    );
    out.push_str(AUDIT_GENESIS);
    out.push_str(
        r#"`.
- Any edit, deletion, or reordering of a historical entry breaks every
  subsequent link in the chain.

Verify the chain at any time:

```sh
ade audit verify
```

Do NOT edit or delete `log.jsonl`. The chain is initialized by the ADE
pipeline and extended by harness hooks (e.g. the Claude Code PostToolUse hook
at `.ade/hooks/audit-log.ts`).
"#,
    );
    out
}

/// The Claude Code PostToolUse hook script — the shared ISC-165 sh shim from
/// the claude harness adapter, anchored at the project root (hooks run with
/// cwd = project root, so the artifact stays free of absolute paths).
pub fn audit_hook_script() -> String {
    ade_hook_script(".")
}

fn module_options(ctx: &Ctx) -> serde_json::Value {
    ctx.module_options("observability")
}

fn targets_claude_code(ctx: &Ctx) -> bool {
    ctx.config
        .harnesses
        .iter()
        .any(|harness| harness == "claude-code")
}

/// Structural check that `.claude/settings.json` wires the PostToolUse audit hook.
fn settings_wire_hook(parsed: Option<&serde_json::Value>) -> bool {
    let Some(post) = parsed
        .filter(|value| value.is_object())
        .and_then(|value| value.get("hooks"))
        .filter(|hooks| hooks.is_object())
        .and_then(|hooks| hooks.get("PostToolUse"))
    else {
        return false;
    };
    post.is_array()
        && serde_json::to_string(post)
            .map(|serialized| serialized.contains(AUDIT_HOOK_COMMAND))
            .unwrap_or(false)
}

/// UTC now as the oracle's `new Date().toISOString()` shape
/// (`YYYY-MM-DDTHH:MM:SS.mmmZ`). Used only for audit-log timestamps — never
/// embedded in lockfile-scoped generated files.
fn now_iso() -> String {
    let duration = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let millis = duration.subsec_millis();
    let secs = duration.as_secs();
    let days = (secs / 86_400) as i64;
    let secs_of_day = secs % 86_400;
    let (year, month, day) = civil_from_days(days);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}.{millis:03}Z",
        secs_of_day / 3600,
        (secs_of_day % 3600) / 60,
        secs_of_day % 60
    )
}

/// Days-since-epoch → (year, month, day). Howard Hinnant's civil_from_days.
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

pub struct ObservabilityModule;

pub static MODULE: ObservabilityModule = ObservabilityModule;

fn apply_inner(ctx: &Ctx) -> std::io::Result<ModuleResult> {
    let mut findings: Vec<Finding> = Vec::new();
    let mut wrote_paths: Vec<String> = Vec::new();
    let mut degraded = false;

    // Audit surface: the chain-contract README.
    ctx.artifacts.write(AUDIT_README_PATH, &audit_readme())?;
    wrote_paths.push(AUDIT_README_PATH.to_string());
    findings.push(Finding::ok(format!("wrote {AUDIT_README_PATH}")));

    // ISC-85 (Rust reading): genesis-init the audit chain when absent. The
    // log is mutable runtime state — written directly (not via the recorded
    // artifact writer) and excluded from lockfile scope, exactly like the
    // oracle pipeline's appendEvents.
    let log_path = ctx.target_dir.join(AUDIT_LOG_PATH);
    if read_if_exists(&log_path).is_none() {
        append_events(
            &log_path,
            &[AuditEventInput {
                ts: now_iso(),
                actor: "ade".to_string(),
                action: "audit.genesis".to_string(),
                target: AUDIT_LOG_PATH.to_string(),
                result: "ok".to_string(),
            }],
        )?;
        findings.push(Finding::ok(format!(
            "initialized {AUDIT_LOG_PATH} (genesis event)"
        )));
    }

    // ISC-86/87: ship the runtime-free hook shim and wire it into Claude Code.
    if targets_claude_code(ctx) {
        ctx.artifacts.write(AUDIT_HOOK_PATH, &audit_hook_script())?;
        wrote_paths.push(AUDIT_HOOK_PATH.to_string());
        let wired = wire_claude_hook(ctx, "PostToolUse", AUDIT_HOOK_COMMAND)?;
        if wired.level == FindingLevel::Ok {
            wrote_paths.push(CLAUDE_SETTINGS_PATH.to_string());
            findings.push(Finding::ok(format!(
                "wired PostToolUse audit hook into {CLAUDE_SETTINGS_PATH}"
            )));
        } else {
            degraded = true;
            findings.push(wired);
        }
    }

    // ISC-88: audit dir git-ignored by default.
    if module_options(ctx)
        .get("commitAuditLog")
        .and_then(|value| value.as_bool())
        == Some(true)
    {
        findings.push(Finding::info(
            "commitAuditLog=true — .ade/audit/ left un-ignored so the audit log can be committed",
        ));
    } else {
        ensure_lines(
            &ctx.target_dir.join(".gitignore"),
            &[".ade/audit/"],
            GITIGNORE_LABEL,
        )?;
        findings.push(Finding::ok(".gitignore covers .ade/audit/"));
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

impl AdeModule for ObservabilityModule {
    fn id(&self) -> &'static str {
        "observability"
    }
    fn title(&self) -> &'static str {
        "Tamper-Evident Observability"
    }
    fn category(&self) -> &'static str {
        "governance"
    }
    fn spec(&self) -> &'static str {
        "Tamper-evident observability and audit logging"
    }

    fn instruction_blocks(&self) -> Vec<InstructionBlock> {
        vec![InstructionBlock {
            id: "observability",
            title: "Audit Logging",
            content: [
                "- All tool activity in this repo is audit-logged to a tamper-evident hash chain at `.ade/audit/log.jsonl`.",
                "- NEVER edit, delete, truncate, or reorder `.ade/audit/log.jsonl` — any change breaks the chain and is flagged by `ade audit verify`.",
                "- Treat the audit log as append-only forensic evidence; only the ADE pipeline and installed hooks write it. If it interferes with a task, surface that to the human instead of touching it.",
            ]
            .join("\n"),
        }]
    }

    fn detect(&self, ctx: &Ctx) -> Vec<Finding> {
        let mut findings = vec![Finding::ok(
            "audit chain requires no external tools (Bun-native sha256)",
        )];
        if targets_claude_code(ctx) {
            findings.push(Finding::ok(
                "claude-code targeted — per-tool activity logging via PostToolUse hook available",
            ));
        } else {
            findings.push(Finding {
                level: FindingLevel::Info,
                message:
                    "no hook-capable harness targeted — audit chain records pipeline events only"
                        .to_string(),
                remediation: Some(
                    "add \"claude-code\" to harnesses in ade.json to capture per-tool activity"
                        .to_string(),
                ),
            });
        }
        findings
    }

    fn plan(&self, ctx: &Ctx) -> Vec<PlannedAction> {
        let mut actions = vec![PlannedAction {
            kind: ActionKind::Write,
            path: Some(AUDIT_README_PATH.to_string()),
            description:
                "write audit-chain README (hash-chain contract, genesis, `ade audit verify`)"
                    .to_string(),
        }];
        if targets_claude_code(ctx) {
            actions.push(PlannedAction {
                kind: ActionKind::Write,
                path: Some(AUDIT_HOOK_PATH.to_string()),
                description: "ship self-contained PostToolUse audit-log hook script".to_string(),
            });
            actions.push(PlannedAction {
                kind: ActionKind::Hook,
                path: Some(CLAUDE_SETTINGS_PATH.to_string()),
                description:
                    "wire PostToolUse hook to append hash-chain entries for every tool call"
                        .to_string(),
            });
        }
        if module_options(ctx)
            .get("commitAuditLog")
            .and_then(|value| value.as_bool())
            == Some(true)
        {
            actions.push(PlannedAction {
                kind: ActionKind::Info,
                path: None,
                description:
                    "commitAuditLog=true — leaving .ade/audit/ un-ignored so the log can be committed"
                        .to_string(),
            });
        } else {
            actions.push(PlannedAction {
                kind: ActionKind::Append,
                path: Some(".gitignore".to_string()),
                description: "git-ignore .ade/audit/ (audit log stays local by default)"
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
        let mut ok = true;

        let readme = read_if_exists(&ctx.target_dir.join(AUDIT_README_PATH));
        match readme {
            Some(text) if text.contains(AUDIT_GENESIS) => {
                findings.push(Finding::ok(format!(
                    "{AUDIT_README_PATH} documents the audit chain"
                )));
            }
            _ => {
                ok = false;
                findings.push(Finding::error_with(
                    format!("{AUDIT_README_PATH} missing or does not document the audit chain"),
                    "run `ade apply`",
                ));
            }
        }

        if targets_claude_code(ctx) {
            match read_if_exists(&ctx.target_dir.join(AUDIT_HOOK_PATH)) {
                None => {
                    ok = false;
                    findings.push(Finding::error_with(
                        format!("{AUDIT_HOOK_PATH} missing"),
                        "run `ade apply`",
                    ));
                }
                Some(_) => findings.push(Finding::ok(format!("{AUDIT_HOOK_PATH} present"))),
            }
            let settings = read_json(ctx, CLAUDE_SETTINGS_PATH);
            if !settings_wire_hook(settings.as_ref()) {
                ok = false;
                findings.push(Finding::error_with(
                    format!("{CLAUDE_SETTINGS_PATH} does not wire the PostToolUse audit hook"),
                    "run `ade apply`",
                ));
            } else {
                findings.push(Finding::ok("PostToolUse audit hook wired"));
            }
        }

        // Integrity of the chain itself, when one exists.
        if let Some(log_text) = read_if_exists(&ctx.target_dir.join(AUDIT_LOG_PATH)) {
            match parse_log(&log_text) {
                Ok(entries) => {
                    let verdict = verify_chain(&entries, None);
                    if verdict.valid {
                        findings.push(Finding::ok(format!(
                            "audit chain valid ({} entries)",
                            verdict.length
                        )));
                    } else {
                        ok = false;
                        let index = verdict
                            .broken_index
                            .map(|value| value.to_string())
                            .unwrap_or_else(|| "undefined".to_string());
                        let reason = verdict.reason.unwrap_or_else(|| "undefined".to_string());
                        findings.push(Finding::error_with(
                            format!("audit chain BROKEN at entry {index} ({reason})"),
                            "investigate tampering — do not edit .ade/audit/log.jsonl",
                        ));
                    }
                }
                Err(_) => {
                    ok = false;
                    findings.push(Finding::error_with(
                        format!("{AUDIT_LOG_PATH} contains unparseable entries"),
                        "investigate tampering — do not edit .ade/audit/log.jsonl",
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
    use crate::audit::AuditEntry;
    use crate::fsutil::sha256_hex;
    use crate::testutil::{make_temp_dir, make_test_ctx, test_config_with, TestCtxOptions};
    use crate::types::AdeConfig;
    use std::path::Path;

    fn read_file(dir: &Path, rel: &str) -> String {
        std::fs::read_to_string(dir.join(rel)).expect("read file")
    }

    fn read_log(dir: &Path) -> Vec<AuditEntry> {
        parse_log(&read_file(dir, AUDIT_LOG_PATH)).expect("parse log")
    }

    fn config_with_options(options: serde_json::Value) -> AdeConfig {
        let mut config = test_config_with(&[], &["claude-code", "codex"]);
        if let Some(entry) = config.modules.get_mut("observability") {
            entry.options = options;
        }
        config
    }

    #[test]
    fn isc_85_apply_writes_audit_readme_and_genesis_initializes_the_chain() {
        let dir = make_temp_dir("obs-readme");
        let ctx = make_test_ctx(&dir, TestCtxOptions::default());
        let result = MODULE.apply(&ctx);
        assert_eq!(result.status, ModuleStatus::Applied);
        assert!(result.wrote_paths.contains(&AUDIT_README_PATH.to_string()));
        let readme = read_file(&dir, AUDIT_README_PATH);
        assert!(readme.contains(AUDIT_GENESIS));
        assert!(readme.contains("ade audit verify"));
        assert!(readme.to_lowercase().contains("hash"));
        // ISC-85 (Rust reading): the module genesis-initializes the chain at
        // apply — divergence from the TS module, where the pipeline owns init.
        let entries = read_log(&dir);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].prev, AUDIT_GENESIS);
        assert_eq!(entries[0].actor, "ade");
        assert_eq!(entries[0].action, "audit.genesis");
        assert_eq!(entries[0].result, "ok");
        let verdict = verify_chain(&entries, None);
        assert!(verdict.valid);
        assert_eq!(verdict.length, 1);
        // ts carries the oracle's toISOString shape.
        let ts = &entries[0].ts;
        assert_eq!(ts.len(), 24);
        assert_eq!(&ts[10..11], "T");
        assert!(ts.ends_with('Z'));
        // The genesis init is not part of the lockfile-scoped generated set.
        assert!(!result.wrote_paths.contains(&AUDIT_LOG_PATH.to_string()));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn isc_86_claude_code_targeted_hook_shipped_and_post_tool_use_wired() {
        let dir = make_temp_dir("obs-wire");
        let ctx = make_test_ctx(&dir, TestCtxOptions::default());
        let result = MODULE.apply(&ctx);
        assert_eq!(result.status, ModuleStatus::Applied);
        assert!(result.wrote_paths.contains(&AUDIT_HOOK_PATH.to_string()));
        assert!(result
            .wrote_paths
            .contains(&CLAUDE_SETTINGS_PATH.to_string()));
        assert!(dir.join(AUDIT_HOOK_PATH).exists());
        let settings: serde_json::Value =
            serde_json::from_str(&read_file(&dir, CLAUDE_SETTINGS_PATH)).unwrap();
        let post = &settings["hooks"]["PostToolUse"];
        assert!(post.is_array());
        assert!(serde_json::to_string(post)
            .unwrap()
            .contains(AUDIT_HOOK_COMMAND));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn isc_86_claude_code_not_targeted_no_hook_script_no_settings_touched() {
        let dir = make_temp_dir("obs-nowire");
        let ctx = make_test_ctx(
            &dir,
            TestCtxOptions {
                config: Some(test_config_with(&[], &["codex"])),
                ..Default::default()
            },
        );
        let result = MODULE.apply(&ctx);
        assert_eq!(result.status, ModuleStatus::Applied);
        assert!(!dir.join(AUDIT_HOOK_PATH).exists());
        assert!(!dir.join(CLAUDE_SETTINGS_PATH).exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn isc_86_preexisting_user_hooks_preserved_by_the_additive_merge() {
        let dir = make_temp_dir("obs-merge");
        let user_settings = serde_json::json!({
            "hooks": {
                "PostToolUse": [
                    { "matcher": "*", "hooks": [{ "type": "command", "command": "bun user-hook.ts" }] }
                ]
            }
        });
        let settings_path = dir.join(CLAUDE_SETTINGS_PATH);
        std::fs::create_dir_all(settings_path.parent().unwrap()).unwrap();
        std::fs::write(&settings_path, user_settings.to_string()).unwrap();
        MODULE.apply(&make_test_ctx(&dir, TestCtxOptions::default()));
        let merged: serde_json::Value =
            serde_json::from_str(&read_file(&dir, CLAUDE_SETTINGS_PATH)).unwrap();
        let serialized = serde_json::to_string(&merged["hooks"]["PostToolUse"]).unwrap();
        assert!(serialized.contains("bun user-hook.ts"));
        assert!(serialized.contains(AUDIT_HOOK_COMMAND));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[cfg(unix)]
    fn run_hook_shim(dir: &Path, stdin_text: &str, path_env: &str) -> i32 {
        use std::io::Write;
        use std::process::{Command, Stdio};
        let mut child = Command::new("/bin/sh")
            .arg(dir.join(AUDIT_HOOK_PATH))
            .current_dir(dir)
            .env("PATH", path_env)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn sh");
        let mut stdin = child.stdin.take().expect("stdin");
        stdin.write_all(stdin_text.as_bytes()).expect("write stdin");
        drop(stdin);
        child.wait().expect("wait").code().unwrap_or(-1)
    }

    #[cfg(unix)]
    #[test]
    fn isc_165_hook_shim_exits_0_and_appends_nothing_when_ade_is_absent() {
        let dir = make_temp_dir("obs-noade");
        MODULE.apply(&make_test_ctx(&dir, TestCtxOptions::default()));
        let log_before = read_file(&dir, AUDIT_LOG_PATH);
        // Malformed payload AND no `ade` on PATH — the hook must never block.
        let code = run_hook_shim(&dir, "this is not json", "/usr/bin:/bin");
        assert_eq!(code, 0);
        assert_eq!(read_file(&dir, AUDIT_LOG_PATH), log_before);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[cfg(unix)]
    #[test]
    fn isc_165_hook_shim_delegates_stdin_and_argv_to_ade_hook_append() {
        use std::os::unix::fs::PermissionsExt;
        let dir = make_temp_dir("obs-fakeade");
        MODULE.apply(&make_test_ctx(&dir, TestCtxOptions::default()));
        let bin_dir = dir.join("fake-bin");
        std::fs::create_dir_all(&bin_dir).unwrap();
        let fake = bin_dir.join("ade");
        std::fs::write(
            &fake,
            "#!/bin/sh\nprintf '%s\\n' \"$*\" > argv.txt\ncat > stdin.txt\nexit 0\n",
        )
        .unwrap();
        std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755)).unwrap();
        let payload = r#"{"tool_name":"Bash","tool_input":{"command":"ls -la"}}"#;
        let code = run_hook_shim(
            &dir,
            payload,
            &format!("{}:/usr/bin:/bin", bin_dir.display()),
        );
        assert_eq!(code, 0);
        assert_eq!(read_file(&dir, "argv.txt"), "hook append --dir .\n");
        assert_eq!(read_file(&dir, "stdin.txt"), payload);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn isc_87_shipped_hook_script_is_self_contained_and_runtime_free() {
        let script = audit_hook_script();
        assert!(!script.contains("../"));
        assert!(!script.contains("./src"));
        assert!(!script.contains("import "));
        assert!(!script.contains("require("));
        // ISC-165 shape instead of the oracle's embedded Bun.CryptoHasher chain.
        assert!(script.starts_with("#!/bin/sh\n"));
        assert!(script.contains("command -v ade"));
        assert!(script.contains("ade hook append"));
    }

    #[test]
    fn isc_88_audit_dir_git_ignored_by_default() {
        let dir = make_temp_dir("obs-gitignore");
        std::fs::write(dir.join(".gitignore"), "node_modules/\n").unwrap();
        MODULE.apply(&make_test_ctx(&dir, TestCtxOptions::default()));
        let gitignore = read_file(&dir, ".gitignore");
        assert!(gitignore.contains("node_modules/"));
        assert!(gitignore
            .split('\n')
            .map(str::trim)
            .any(|line| line == ".ade/audit/"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn isc_88_commit_audit_log_true_skips_gitignore_and_emits_info_finding() {
        let dir = make_temp_dir("obs-commitlog");
        let config = config_with_options(serde_json::json!({ "commitAuditLog": true }));
        let result = MODULE.apply(&make_test_ctx(
            &dir,
            TestCtxOptions {
                config: Some(config),
                ..Default::default()
            },
        ));
        assert_eq!(result.status, ModuleStatus::Applied);
        assert!(result.findings.iter().any(|finding| {
            finding.level == FindingLevel::Info && finding.message.contains("commitAuditLog")
        }));
        if dir.join(".gitignore").exists() {
            assert!(!read_file(&dir, ".gitignore")
                .split('\n')
                .map(str::trim)
                .any(|line| line == ".ade/audit/"));
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn isc_116_plan_writes_nothing_to_disk() {
        let dir = make_temp_dir("obs-plan");
        let ctx = make_test_ctx(&dir, TestCtxOptions::default());
        let actions = MODULE.plan(&ctx);
        assert!(!actions.is_empty());
        assert!(actions.iter().any(|action| action.kind == ActionKind::Hook));
        assert!(!dir.join(AUDIT_README_PATH).exists());
        assert!(!dir.join(AUDIT_HOOK_PATH).exists());
        assert!(!dir.join(CLAUDE_SETTINGS_PATH).exists());
        assert!(!dir.join(AUDIT_LOG_PATH).exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn isc_117_apply_is_idempotent_second_run_byte_identical_across_all_artifacts() {
        let dir = make_temp_dir("obs-idem");
        MODULE.apply(&make_test_ctx(&dir, TestCtxOptions::default()));
        let artifacts = [
            AUDIT_README_PATH,
            AUDIT_HOOK_PATH,
            CLAUDE_SETTINGS_PATH,
            ".gitignore",
        ];
        let first: Vec<String> = artifacts
            .iter()
            .map(|rel| sha256_hex(&read_file(&dir, rel)))
            .collect();
        let log_first = read_file(&dir, AUDIT_LOG_PATH);
        MODULE.apply(&make_test_ctx(&dir, TestCtxOptions::default()));
        let second: Vec<String> = artifacts
            .iter()
            .map(|rel| sha256_hex(&read_file(&dir, rel)))
            .collect();
        assert_eq!(second, first);
        // Genesis init happens once — the chain does not grow on re-apply.
        assert_eq!(read_file(&dir, AUDIT_LOG_PATH), log_first);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn degraded_path_unparseable_user_settings_refused_with_remediation_file_untouched() {
        let dir = make_temp_dir("obs-degraded");
        let broken = "{not valid json";
        let settings_path = dir.join(CLAUDE_SETTINGS_PATH);
        std::fs::create_dir_all(settings_path.parent().unwrap()).unwrap();
        std::fs::write(&settings_path, broken).unwrap();
        let result = MODULE.apply(&make_test_ctx(&dir, TestCtxOptions::default()));
        assert_eq!(result.status, ModuleStatus::Degraded);
        assert!(result.findings.iter().any(|finding| {
            finding.level == FindingLevel::Error && finding.remediation.is_some()
        }));
        assert_eq!(read_file(&dir, CLAUDE_SETTINGS_PATH), broken);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn verify_passes_after_apply_and_fails_when_surface_pieces_are_removed() {
        let dir = make_temp_dir("obs-verify");
        let ctx = make_test_ctx(&dir, TestCtxOptions::default());
        MODULE.apply(&ctx);
        assert!(MODULE.verify(&ctx).ok);

        std::fs::remove_file(dir.join(AUDIT_HOOK_PATH)).unwrap();
        assert!(!MODULE.verify(&ctx).ok);
        MODULE.apply(&make_test_ctx(&dir, TestCtxOptions::default()));

        std::fs::write(dir.join(CLAUDE_SETTINGS_PATH), "{}\n").unwrap();
        assert!(!MODULE.verify(&ctx).ok);
        MODULE.apply(&make_test_ctx(&dir, TestCtxOptions::default()));

        std::fs::remove_file(dir.join(AUDIT_README_PATH)).unwrap();
        assert!(!MODULE.verify(&ctx).ok);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn verify_detects_a_tampered_audit_chain() {
        let dir = make_temp_dir("obs-tamper");
        let ctx = make_test_ctx(&dir, TestCtxOptions::default());
        MODULE.apply(&ctx);
        // Simulate harness hooks extending the chain past the genesis entry
        // (the `ade hook append` CLI is ported separately; the chain math is
        // the same crate::audit implementation it delegates to).
        append_events(
            &dir.join(AUDIT_LOG_PATH),
            &[
                AuditEventInput {
                    ts: "2026-07-25T00:00:00.000Z".to_string(),
                    actor: "harness-hook".to_string(),
                    action: "tool.Bash".to_string(),
                    target: "{\"command\":\"ls\"}".to_string(),
                    result: "observed".to_string(),
                },
                AuditEventInput {
                    ts: "2026-07-25T00:00:01.000Z".to_string(),
                    actor: "harness-hook".to_string(),
                    action: "tool.Write".to_string(),
                    target: "{\"file_path\":\"a.ts\"}".to_string(),
                    result: "observed".to_string(),
                },
            ],
        )
        .unwrap();
        assert!(MODULE.verify(&ctx).ok);

        // Tamper with a historical entry — every subsequent link must break.
        let mut entries = read_log(&dir);
        entries[0].target = "something-else-entirely".to_string();
        let rewritten: String = entries
            .iter()
            .map(|entry| format!("{}\n", serde_json::to_string(entry).unwrap()))
            .collect();
        std::fs::write(dir.join(AUDIT_LOG_PATH), rewritten).unwrap();
        let verdict = MODULE.verify(&ctx);
        assert!(!verdict.ok);
        assert!(verdict
            .findings
            .iter()
            .any(|finding| finding.message.contains("BROKEN")));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn verify_flags_unparseable_log_entries() {
        let dir = make_temp_dir("obs-unparseable");
        let ctx = make_test_ctx(&dir, TestCtxOptions::default());
        MODULE.apply(&ctx);
        let log_path = dir.join(AUDIT_LOG_PATH);
        let mut text = read_file(&dir, AUDIT_LOG_PATH);
        text.push_str("this is not a json line\n");
        std::fs::write(&log_path, text).unwrap();
        let verdict = MODULE.verify(&ctx);
        assert!(!verdict.ok);
        assert!(verdict
            .findings
            .iter()
            .any(|finding| finding.message.contains("unparseable entries")));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn instruction_block_declares_the_tamper_evident_chain_and_forbids_touching_the_log() {
        let blocks = MODULE.instruction_blocks();
        assert!(!blocks.is_empty());
        let block = &blocks[0];
        assert!(block.content.contains(".ade/audit/log.jsonl"));
        assert!(block.content.contains("NEVER"));
        assert!(block.content.to_lowercase().contains("tamper-evident"));
    }

    #[test]
    fn detect_reports_harness_hook_availability() {
        let dir = make_temp_dir("obs-detect");
        let with_claude = MODULE.detect(&make_test_ctx(&dir, TestCtxOptions::default()));
        assert!(with_claude
            .iter()
            .any(|finding| finding.message.contains("claude-code")));
        let without = MODULE.detect(&make_test_ctx(
            &dir,
            TestCtxOptions {
                config: Some(test_config_with(&[], &["codex"])),
                ..Default::default()
            },
        ));
        assert!(without.iter().any(|finding| {
            finding.level == FindingLevel::Info && finding.remediation.is_some()
        }));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn civil_from_days_and_now_iso_produce_the_oracle_timestamp_shape() {
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        assert_eq!(civil_from_days(19_723), (2024, 1, 1));
        assert_eq!(civil_from_days(11_016), (2000, 2, 29));
        assert_eq!(civil_from_days(11_017), (2000, 3, 1));
        let ts = now_iso();
        assert_eq!(ts.len(), 24);
        assert_eq!(&ts[4..5], "-");
        assert_eq!(&ts[10..11], "T");
        assert_eq!(&ts[19..20], ".");
        assert!(ts.ends_with('Z'));
    }
}
