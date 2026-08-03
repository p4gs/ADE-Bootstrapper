//! Module: secrets & credential hygiene — port of `src/modules/secrets.ts`.
//!
//! Spec component: "Secrets and credential hygiene — secure bootstrap-time
//! secret provisioning, scoped credential access, secret leak prevention,
//! environment scrubbing, and protection against accidental inclusion of
//! secrets in code, logs, prompts, or commits (e.g. TruffleHog with
//! pre-commit hook)."
//! Boundary controlled: the git commit boundary + the prompt/log boundary.
//!
//! Integration over rebuild: TruffleHog is the scanner. When the pre-commit
//! framework is present we emit a `.pre-commit-config.yaml`; otherwise we
//! install a native `.git/hooks/pre-commit` shim that chains any pre-existing
//! hook (non-destructive) and runs TruffleHog on the staged range.

use crate::fsutil::{ensure_lines, read_if_exists};
use crate::modules::shared::{tool_finding, verify_json_artifact, write_policy};
use crate::types::{
    ActionKind, AdeModule, Ctx, Finding, FindingLevel, InstructionBlock, ModuleResult,
    ModuleStatus, PlannedAction, VerifyResult,
};
use serde_json::json;
use std::fs;
use std::path::Path;

pub const SECRETS_POLICY_PATH: &str = ".ade/policy/secrets.json";
pub const PRECOMMIT_CONFIG_PATH: &str = ".pre-commit-config.yaml";
pub const HOOK_MARKER: &str = "# ade-secrets-hook v1";
pub const CHAINED_HOOK_NAME: &str = "pre-commit.pre-ade";

pub const GITIGNORE_LINES: [&str; 9] = [
    ".env",
    ".env.*",
    "*.pem",
    "*.key",
    "id_rsa",
    "id_ed25519",
    ".ade/memory-store/",
    ".ade/audit/",
    ".ade/logs/",
];

/// The native pre-commit shim. Warns-and-passes when trufflehog is missing
/// (never bricks commits).
///
/// IMPORTANT (live-probe-verified 2026-07-12): the commonly-documented
/// `trufflehog git file://. --since-commit HEAD` invocation scans the
/// COMMITTED range, which is empty at pre-commit time — it blocks nothing.
/// This shim instead materializes the staged index via `git checkout-index`
/// and runs `trufflehog filesystem` over the snapshot, so staged secrets are
/// actually caught before they enter history.
pub fn hook_script() -> String {
    format!(
        r##"#!/bin/sh
{HOOK_MARKER}
# Installed by ADE Bootstrapper (secrets module). Chains any pre-existing hook.
# Blocks commits whose STAGED content contains a verified secret (TruffleHog).

if [ -x "$(dirname "$0")/{CHAINED_HOOK_NAME}" ]; then
  "$(dirname "$0")/{CHAINED_HOOK_NAME}" "$@" || exit $?
fi

if command -v trufflehog >/dev/null 2>&1; then
  # Fail CLOSED: if we cannot stage a snapshot we cannot scan, and an unscanned
  # commit is exactly what this hook exists to prevent.
  tmpdir=$(mktemp -d) || {{
    echo "ade: commit BLOCKED — could not create a temp dir to stage the secret scan." >&2
    exit 1
  }}
  trap 'rm -rf "$tmpdir"' EXIT
  # Materialize the staged snapshot (index), then scan the real bytes being committed.
  git checkout-index --prefix="$tmpdir/" -af
  scan_json="$tmpdir/.ade-secrets-scan.json"
  trufflehog filesystem "$tmpdir" --results=verified --fail --no-update --json >"$scan_json" 2>/dev/null
  status=$?
  # Tier-3 catch metric, redacted ON PURPOSE: TruffleHog's JSON carries raw
  # secret values, so the retained log keeps ONLY counts — the findings
  # themselves die with the temp dir. Metric failure never blocks a commit.
  findings=$(wc -l <"$scan_json" | tr -d '[:space:]')
  [ -n "$findings" ] || findings=0
  result=clean
  [ $status -ne 0 ] && result=blocked
  {{ mkdir -p .ade/logs && printf '{{"ts":%s,"findings":%s,"result":"%s"}}\n' "$(date +%s)" "$findings" "$result" >>.ade/logs/secrets-scan.jsonl; }} 2>/dev/null || true
  if [ $status -ne 0 ]; then
    echo "ade: commit BLOCKED — TruffleHog found a verified secret in the staged content." >&2
    echo "ade: remove the secret (and rotate it), then commit again." >&2
    exit 1
  fi
else
  echo "ade: warning — trufflehog not installed; secret scan skipped (install: brew install trufflehog)" >&2
fi
exit 0
"##
    )
}

/// pre-commit framework config (static template — the framework requires YAML).
pub fn pre_commit_config() -> String {
    r#"# Managed by ADE Bootstrapper (secrets module).
# Note: scans the files pre-commit passes (staged names, working-tree bytes) —
# the framework's standard contract for local hooks.
repos:
  - repo: local
    hooks:
      - id: trufflehog
        name: TruffleHog secret scan
        entry: trufflehog filesystem --results=verified --fail --no-update
        language: system
        stages: ["pre-commit"]
"#
    .to_string()
}

fn secrets_policy(scanner: Option<&str>) -> serde_json::Value {
    json!({
        "schemaVersion": 1,
        "scanner": scanner.unwrap_or("none"),
        "scanScope": "staged-changes",
        "blockOn": "verified-secrets",
        "environmentScrubbing": {
            "neverEcho": ["*_KEY", "*_TOKEN", "*_SECRET", "*_PASSWORD", "AWS_*", "GITHUB_TOKEN"],
            "rule": "never print environment variable values into code, logs, prompts, or commits",
        },
        "credentialScoping": {
            "rule": "prefer short-lived, least-privilege credentials injected at the tool boundary; never commit long-lived credentials",
        },
    })
}

fn make_executable(path: &Path) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(path)?.permissions();
        perms.set_mode(0o755);
        fs::set_permissions(path, perms)?;
    }
    #[cfg(not(unix))]
    {
        let _ = path;
    }
    Ok(())
}

fn apply_inner(ctx: &Ctx) -> std::io::Result<ModuleResult> {
    let mut findings: Vec<Finding> = Vec::new();
    let mut wrote_paths: Vec<String> = Vec::new();
    let scanner = if ctx.tool_present("trufflehog") {
        Some("trufflehog")
    } else {
        None
    };

    write_policy(ctx, SECRETS_POLICY_PATH, &secrets_policy(scanner))?;
    wrote_paths.push(SECRETS_POLICY_PATH.to_string());

    ensure_lines(
        &ctx.target_dir.join(".gitignore"),
        &GITIGNORE_LINES,
        "ADE Bootstrapper — secrets hygiene",
    )?;
    findings.push(Finding::ok(".gitignore covers secret-bearing paths"));

    if !ctx.is_git_repo {
        findings.push(Finding::degraded(
            "not a git repository — skipped commit-hook installation",
            "run `git init`, then `ade apply`",
        ));
        return Ok(ModuleResult {
            status: ModuleStatus::Degraded,
            findings,
            wrote_paths,
        });
    }

    if ctx.tool_present("pre-commit") {
        let existing = read_if_exists(&ctx.target_dir.join(PRECOMMIT_CONFIG_PATH));
        match existing {
            None => {
                ctx.artifacts
                    .write(PRECOMMIT_CONFIG_PATH, &pre_commit_config())?;
                wrote_paths.push(PRECOMMIT_CONFIG_PATH.to_string());
                findings.push(Finding::ok(
                    "wrote .pre-commit-config.yaml with TruffleHog hook (run `pre-commit install`)",
                ));
            }
            Some(existing) if existing.contains("trufflehog") => {
                if existing.contains("--since-commit") {
                    // A scanner configured to find nothing is the same failure class as a
                    // suppression (live-probe-verified: --since-commit HEAD scans an empty
                    // range at pre-commit time). Flag it loudly; we do not rewrite user YAML.
                    findings.push(Finding::error_with(
                        ".pre-commit-config.yaml uses a trufflehog invocation with --since-commit — this scans an EMPTY range at pre-commit time and blocks nothing",
                        "replace the entry with `trufflehog filesystem --results=verified --fail --no-update` (see .ade/policy/secrets.json)",
                    ));
                } else {
                    findings.push(Finding::ok(
                        ".pre-commit-config.yaml already includes a trufflehog hook",
                    ));
                }
            }
            Some(_) => {
                findings.push(Finding::degraded(
                    ".pre-commit-config.yaml exists without a trufflehog hook — not modifying a user-owned YAML file",
                    "add the trufflehog hook to your .pre-commit-config.yaml (see .ade/policy/secrets.json)",
                ));
            }
        }
    } else {
        let hooks_dir = ctx.target_dir.join(".git").join("hooks");
        let hook_path = hooks_dir.join("pre-commit");
        let existing = read_if_exists(&hook_path);
        if let Some(existing_content) = &existing {
            if !existing_content.contains(HOOK_MARKER) {
                fs::rename(&hook_path, hooks_dir.join(CHAINED_HOOK_NAME))?;
                findings.push(Finding::info(format!(
                    "existing pre-commit hook preserved as {CHAINED_HOOK_NAME} and chained"
                )));
            }
        }
        let has_marker = existing
            .as_ref()
            .map(|content| content.contains(HOOK_MARKER))
            .unwrap_or(false);
        if !has_marker {
            fs::create_dir_all(&hooks_dir)?;
            fs::write(&hook_path, hook_script())?;
            make_executable(&hook_path)?;
            findings.push(Finding::ok(
                "installed .git/hooks/pre-commit secret-scan shim",
            ));
        } else {
            fs::write(&hook_path, hook_script())?;
            make_executable(&hook_path)?;
            findings.push(Finding::ok(
                "refreshed .git/hooks/pre-commit secret-scan shim",
            ));
        }
    }

    if scanner.is_none() {
        findings.push(Finding::degraded(
            "trufflehog not installed — hook will warn instead of scanning",
            "install TruffleHog (e.g. `brew install trufflehog`)",
        ));
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

pub struct SecretsModule;
pub static MODULE: SecretsModule = SecretsModule;

impl AdeModule for SecretsModule {
    fn id(&self) -> &'static str {
        "secrets"
    }
    fn title(&self) -> &'static str {
        "Secrets & Credential Hygiene"
    }
    fn category(&self) -> &'static str {
        "security"
    }
    fn spec(&self) -> &'static str {
        "Secrets and credential hygiene"
    }

    fn instruction_blocks(&self) -> Vec<InstructionBlock> {
        vec![InstructionBlock {
            id: "secrets",
            title: "Secrets & Credential Hygiene",
            content: r#"- NEVER write secret values into code, logs, prompts, commit messages, or generated files.
- NEVER echo environment variables that look like credentials (`*_KEY`, `*_TOKEN`, `*_SECRET`, `*_PASSWORD`).
- A pre-commit secret scan (TruffleHog) guards this repo; if it blocks a commit, remove AND rotate the secret — do not bypass the hook.
- Use scoped, short-lived credentials; request the human provision them at the boundary (env injection), never inline."#
                .to_string(),
        }]
    }

    fn detect(&self, ctx: &Ctx) -> Vec<Finding> {
        let mut findings = vec![
            tool_finding(
                ctx,
                "trufflehog",
                "install TruffleHog (e.g. `brew install trufflehog`) to enable commit-boundary secret scanning",
            ),
            tool_finding(
                ctx,
                "pre-commit",
                "optional: install the pre-commit framework to manage hooks declaratively",
            ),
        ];
        if !ctx.is_git_repo {
            findings.push(Finding::degraded(
                "target is not a git repository — commit-boundary scanning unavailable",
                "run `git init` in the target, then `ade apply`",
            ));
        }
        findings
    }

    fn plan(&self, ctx: &Ctx) -> Vec<PlannedAction> {
        let mut actions = vec![
            PlannedAction {
                kind: ActionKind::Write,
                path: Some(SECRETS_POLICY_PATH.to_string()),
                description: "write secrets hygiene policy".to_string(),
            },
            PlannedAction {
                kind: ActionKind::Append,
                path: Some(".gitignore".to_string()),
                description: "ensure secret-bearing paths are git-ignored".to_string(),
            },
        ];
        if ctx.is_git_repo {
            if ctx.tool_present("pre-commit") {
                actions.push(PlannedAction {
                    kind: ActionKind::Write,
                    path: Some(PRECOMMIT_CONFIG_PATH.to_string()),
                    description: "write pre-commit framework config with TruffleHog hook"
                        .to_string(),
                });
            } else {
                actions.push(PlannedAction {
                    kind: ActionKind::Hook,
                    path: Some(".git/hooks/pre-commit".to_string()),
                    description:
                        "install native pre-commit shim (chains existing hook) running TruffleHog"
                            .to_string(),
                });
            }
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
        let mut findings: Vec<Finding> = vec![verify_json_artifact(ctx, SECRETS_POLICY_PATH)];
        let mut ok = findings
            .iter()
            .all(|finding| finding.level == FindingLevel::Ok);

        let gitignore = read_if_exists(&ctx.target_dir.join(".gitignore")).unwrap_or_default();
        let present: Vec<String> = gitignore
            .split('\n')
            .map(|line| line.trim().to_string())
            .collect();
        let missing: Vec<&str> = GITIGNORE_LINES
            .iter()
            .copied()
            .filter(|line| !present.iter().any(|existing| existing == line))
            .collect();
        if !missing.is_empty() {
            ok = false;
            findings.push(Finding::error_with(
                format!(".gitignore missing entries: {}", missing.join(", ")),
                "run `ade apply`",
            ));
        }

        if ctx.is_git_repo && !ctx.tool_present("pre-commit") {
            let hook =
                read_if_exists(&ctx.target_dir.join(".git").join("hooks").join("pre-commit"));
            match hook {
                Some(hook) if hook.contains(HOOK_MARKER) => {
                    if hook.contains("--since-commit") {
                        // Rot-guard: an older/foreign shim using the scan-nothing invocation.
                        ok = false;
                        findings.push(Finding::error_with(
                            "installed pre-commit shim uses --since-commit (scans an empty range — blocks nothing)",
                            "run `ade apply` to refresh the shim to the staged-index scan",
                        ));
                    } else {
                        findings.push(Finding::ok(
                            "pre-commit secret-scan shim installed (staged-index scan)",
                        ));
                    }
                }
                _ => {
                    ok = false;
                    findings.push(Finding::error_with(
                        "pre-commit secret-scan shim not installed",
                        "run `ade apply`",
                    ));
                }
            }
        }
        if ctx.is_git_repo && ctx.tool_present("pre-commit") {
            let config = read_if_exists(&ctx.target_dir.join(PRECOMMIT_CONFIG_PATH));
            if let Some(config) = config {
                if config.contains("trufflehog") && config.contains("--since-commit") {
                    ok = false;
                    findings.push(Finding::error_with(
                        ".pre-commit-config.yaml trufflehog entry uses --since-commit (scans an empty range — blocks nothing)",
                        "replace with `trufflehog filesystem --results=verified --fail --no-update`",
                    ));
                }
            }
        }
        if !ctx.is_git_repo {
            findings.push(Finding {
                level: FindingLevel::Degraded,
                message: "not a git repository — commit-boundary scan not verifiable".to_string(),
                remediation: None,
            });
        }
        VerifyResult { ok, findings }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::{make_temp_dir, make_test_ctx, TestCtxOptions};
    use std::collections::BTreeMap;
    use std::fs;
    use std::path::PathBuf;

    fn make_git_dir(tag: &str) -> PathBuf {
        let dir = make_temp_dir(tag);
        fs::create_dir_all(dir.join(".git").join("hooks")).unwrap();
        dir
    }

    #[test]
    fn isc_93_trufflehog_present_applied_with_native_hook_installed() {
        let dir = make_git_dir("sec-93a");
        let ctx = make_test_ctx(
            &dir,
            TestCtxOptions {
                present_tools: &[("trufflehog", "trufflehog 3.90.0")],
                ..Default::default()
            },
        );
        let result = MODULE.apply(&ctx);
        assert_eq!(result.status, ModuleStatus::Applied);
        let hook = fs::read_to_string(dir.join(".git/hooks/pre-commit")).unwrap();
        assert!(hook.contains(HOOK_MARKER));
        assert!(hook.contains("git checkout-index"));
        assert!(hook.contains("trufflehog filesystem"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn isc_93_trufflehog_absent_degraded_with_install_remediation() {
        let dir = make_git_dir("sec-93b");
        let ctx = make_test_ctx(&dir, TestCtxOptions::default());
        let result = MODULE.apply(&ctx);
        assert_eq!(result.status, ModuleStatus::Degraded);
        assert!(result.findings.iter().any(|f| f
            .remediation
            .as_deref()
            .map(|r| r.contains("brew install trufflehog"))
            .unwrap_or(false)));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn isc_94_pre_commit_framework_present_writes_config_with_trufflehog_hook() {
        let dir = make_git_dir("sec-94a");
        let ctx = make_test_ctx(
            &dir,
            TestCtxOptions {
                present_tools: &[("trufflehog", "3.90.0"), ("pre-commit", "pre-commit 4.0.0")],
                ..Default::default()
            },
        );
        let result = MODULE.apply(&ctx);
        assert_eq!(result.status, ModuleStatus::Applied);
        let config = fs::read_to_string(dir.join(PRECOMMIT_CONFIG_PATH)).unwrap();
        assert!(config.contains("trufflehog"));
        assert!(!dir.join(".git/hooks/pre-commit").exists());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn isc_94_existing_user_pre_commit_config_without_trufflehog_is_not_modified() {
        let dir = make_git_dir("sec-94b");
        let user_config = "repos: []\n# user-owned\n";
        fs::write(dir.join(PRECOMMIT_CONFIG_PATH), user_config).unwrap();
        let ctx = make_test_ctx(
            &dir,
            TestCtxOptions {
                present_tools: &[("trufflehog", "3.90.0"), ("pre-commit", "4.0.0")],
                ..Default::default()
            },
        );
        let result = MODULE.apply(&ctx);
        assert_eq!(
            fs::read_to_string(dir.join(PRECOMMIT_CONFIG_PATH)).unwrap(),
            user_config
        );
        assert!(result
            .findings
            .iter()
            .any(|f| f.level == FindingLevel::Degraded));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn isc_94_pre_existing_non_ade_git_hook_is_chained_not_destroyed() {
        let dir = make_git_dir("sec-94c");
        let user_hook = "#!/bin/sh\necho user-hook\n";
        fs::write(dir.join(".git/hooks/pre-commit"), user_hook).unwrap();
        let ctx = make_test_ctx(
            &dir,
            TestCtxOptions {
                present_tools: &[("trufflehog", "3.90.0")],
                ..Default::default()
            },
        );
        MODULE.apply(&ctx);
        let preserved = fs::read_to_string(dir.join(".git/hooks").join(CHAINED_HOOK_NAME)).unwrap();
        assert_eq!(preserved, user_hook);
        let hook = fs::read_to_string(dir.join(".git/hooks/pre-commit")).unwrap();
        assert!(hook.contains(CHAINED_HOOK_NAME));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn isc_96_gitignore_gains_secret_bearing_paths_idempotently() {
        let dir = make_git_dir("sec-96");
        fs::write(dir.join(".gitignore"), "node_modules/\n").unwrap();
        let ctx = make_test_ctx(
            &dir,
            TestCtxOptions {
                present_tools: &[("trufflehog", "3.90.0")],
                ..Default::default()
            },
        );
        MODULE.apply(&ctx);
        let first = fs::read_to_string(dir.join(".gitignore")).unwrap();
        assert!(first.contains("node_modules/"));
        assert!(first.contains(".env"));
        assert!(first.contains("*.pem"));
        MODULE.apply(&make_test_ctx(
            &dir,
            TestCtxOptions {
                present_tools: &[("trufflehog", "3.90.0")],
                ..Default::default()
            },
        ));
        let second = fs::read_to_string(dir.join(".gitignore")).unwrap();
        assert_eq!(second, first);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn isc_97_instruction_block_forbids_secret_echoing_and_names_scoped_credentials() {
        let blocks = MODULE.instruction_blocks();
        let block = &blocks[0];
        assert!(block.content.contains("NEVER"));
        assert!(block.content.to_lowercase().contains("rotate"));
        assert!(block.content.to_lowercase().contains("scoped"));
    }

    #[test]
    fn isc_98_isc_123_planted_env_secret_never_appears_in_any_generated_artifact() {
        let dir = make_git_dir("sec-98");
        let planted = "AKIA_PLANTED_TEST_SECRET_VALUE_12345";
        let mut env = BTreeMap::new();
        env.insert("AWS_SECRET_ACCESS_KEY".to_string(), planted.to_string());
        let ctx = make_test_ctx(
            &dir,
            TestCtxOptions {
                present_tools: &[("trufflehog", "3.90.0")],
                env,
                ..Default::default()
            },
        );
        MODULE.apply(&ctx);
        for rel in [SECRETS_POLICY_PATH, ".gitignore", ".git/hooks/pre-commit"] {
            let path = dir.join(rel);
            if path.exists() {
                assert!(!fs::read_to_string(&path).unwrap().contains(planted));
            }
        }
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn isc_15_1_non_git_directory_degrades_without_crashing_and_skips_hook_install() {
        let bare = make_temp_dir("sec-151");
        let ctx = make_test_ctx(
            &bare,
            TestCtxOptions {
                is_git_repo: false,
                present_tools: &[("trufflehog", "3.90.0")],
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
        assert!(!bare.join(".git/hooks/pre-commit").exists());
        let _ = fs::remove_dir_all(&bare);
    }

    #[test]
    fn isc_116_plan_writes_nothing() {
        let dir = make_git_dir("sec-116");
        let ctx = make_test_ctx(
            &dir,
            TestCtxOptions {
                present_tools: &[("trufflehog", "3.90.0")],
                ..Default::default()
            },
        );
        let actions = MODULE.plan(&ctx);
        assert!(actions.iter().any(|action| action.kind == ActionKind::Hook));
        assert!(!dir.join(SECRETS_POLICY_PATH).exists());
        assert!(!dir.join(".git/hooks/pre-commit").exists());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn broken_scanner_remediation_since_commit_invocations_flagged_as_errors() {
        // Adopted repo with the documented-but-broken pre-commit framework entry.
        let dir = make_git_dir("sec-bsr");
        fs::write(
            dir.join(PRECOMMIT_CONFIG_PATH),
            "repos:\n  - repo: local\n    hooks:\n      - id: trufflehog\n        entry: trufflehog git file://. --since-commit HEAD --fail\n",
        )
        .unwrap();
        let ctx = make_test_ctx(
            &dir,
            TestCtxOptions {
                present_tools: &[("trufflehog", "3.9.0"), ("pre-commit", "4.0.0")],
                ..Default::default()
            },
        );
        let result = MODULE.apply(&ctx);
        assert!(result
            .findings
            .iter()
            .any(|f| f.level == FindingLevel::Error && f.message.contains("--since-commit")));
        let verdict = MODULE.verify(&ctx);
        assert!(!verdict.ok);
        let _ = fs::remove_dir_all(&dir);

        // Rot-guard on the native shim path: a stale shim with the broken invocation fails verify.
        let bare = make_git_dir("sec-bsr2");
        fs::write(
            bare.join(".git/hooks/pre-commit"),
            format!(
                "#!/bin/sh\n{HOOK_MARKER}\ntrufflehog git file://. --since-commit HEAD --fail\n"
            ),
        )
        .unwrap();
        let stale_ctx = make_test_ctx(
            &bare,
            TestCtxOptions {
                present_tools: &[("trufflehog", "3.9.0")],
                ..Default::default()
            },
        );
        let stale_verdict = MODULE.verify(&stale_ctx);
        assert!(!stale_verdict.ok);
        assert!(stale_verdict
            .findings
            .iter()
            .any(|f| f.message.contains("--since-commit")));
        let _ = fs::remove_dir_all(&bare);
    }

    #[test]
    fn verify_passes_after_apply_fails_when_hook_removed() {
        let dir = make_git_dir("sec-vh");
        let ctx = make_test_ctx(
            &dir,
            TestCtxOptions {
                present_tools: &[("trufflehog", "3.90.0")],
                ..Default::default()
            },
        );
        MODULE.apply(&ctx);
        assert!(MODULE.verify(&ctx).ok);
        fs::remove_dir_all(dir.join(".git/hooks")).unwrap();
        fs::create_dir_all(dir.join(".git/hooks")).unwrap();
        assert!(!MODULE.verify(&ctx).ok);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn detect_reports_tool_presence_and_non_git_degradation() {
        let dir = make_git_dir("sec-det");
        let findings = MODULE.detect(&make_test_ctx(
            &dir,
            TestCtxOptions {
                is_git_repo: false,
                ..Default::default()
            },
        ));
        assert!(findings.iter().any(|f| f.message.contains("trufflehog")));
        assert!(findings
            .iter()
            .any(|f| f.level == FindingLevel::Degraded && f.message.contains("git")));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn hook_script_scans_the_staged_index_snapshot_and_warns_and_passes_when_missing() {
        let script = hook_script();
        assert!(script.contains("command -v trufflehog"));
        assert!(script.contains("exit 0"));
        assert!(script.contains("BLOCKED"));
        // Live-probe-verified: since-commit HEAD scans nothing at pre-commit time.
        assert!(!script.contains("--since-commit"));
        assert!(script.contains("git checkout-index"));
        assert!(pre_commit_config().contains("trufflehog filesystem"));
    }

    #[test]
    fn hook_script_matches_oracle_bytes() {
        // Full expansion of the TS `hookScript()` template literal (ISC-160 parity).
        let expected = r##"#!/bin/sh
# ade-secrets-hook v1
# Installed by ADE Bootstrapper (secrets module). Chains any pre-existing hook.
# Blocks commits whose STAGED content contains a verified secret (TruffleHog).

if [ -x "$(dirname "$0")/pre-commit.pre-ade" ]; then
  "$(dirname "$0")/pre-commit.pre-ade" "$@" || exit $?
fi

if command -v trufflehog >/dev/null 2>&1; then
  # Fail CLOSED: if we cannot stage a snapshot we cannot scan, and an unscanned
  # commit is exactly what this hook exists to prevent.
  tmpdir=$(mktemp -d) || {
    echo "ade: commit BLOCKED — could not create a temp dir to stage the secret scan." >&2
    exit 1
  }
  trap 'rm -rf "$tmpdir"' EXIT
  # Materialize the staged snapshot (index), then scan the real bytes being committed.
  git checkout-index --prefix="$tmpdir/" -af
  scan_json="$tmpdir/.ade-secrets-scan.json"
  trufflehog filesystem "$tmpdir" --results=verified --fail --no-update --json >"$scan_json" 2>/dev/null
  status=$?
  # Tier-3 catch metric, redacted ON PURPOSE: TruffleHog's JSON carries raw
  # secret values, so the retained log keeps ONLY counts — the findings
  # themselves die with the temp dir. Metric failure never blocks a commit.
  findings=$(wc -l <"$scan_json" | tr -d '[:space:]')
  [ -n "$findings" ] || findings=0
  result=clean
  [ $status -ne 0 ] && result=blocked
  { mkdir -p .ade/logs && printf '{"ts":%s,"findings":%s,"result":"%s"}\n' "$(date +%s)" "$findings" "$result" >>.ade/logs/secrets-scan.jsonl; } 2>/dev/null || true
  if [ $status -ne 0 ]; then
    echo "ade: commit BLOCKED — TruffleHog found a verified secret in the staged content." >&2
    echo "ade: remove the secret (and rotate it), then commit again." >&2
    exit 1
  fi
else
  echo "ade: warning — trufflehog not installed; secret scan skipped (install: brew install trufflehog)" >&2
fi
exit 0
"##;
        assert_eq!(hook_script(), expected);
    }
}
