//! Phase G — the metrics foundation. Pure `assess`-style functions over
//! probed facts, mirroring `inventory.rs`'s split: I/O (exec a command, stat
//! a file) stays a thin, separately-testable seam; the arithmetic that turns
//! those facts into a displayable stat is pure and unit-tested here. Every
//! function returns `Option` (or an empty `Vec`/count-of-zero-inputs) when
//! its real data source is absent — there is no placeholder-zero path
//! anywhere in this file, per the house rule that absent data renders as
//! absent, never fabricated.
//!
//! Two tiers, per the plan (`Plans/iridescent-moseying-sloth.md`, "Phase G"):
//! - **Tier 1** (first section below): facts already sitting in `Job`
//!   history and per-project `StatusRow` reports — no new probing, just
//!   pulling the right record and doing date/coverage math on it.
//! - **Tier 2** (the rest of the file): six new, narrowly-scoped probes,
//!   each grounded in a real command or file this session actually verified
//!   on a live machine — see the doc comment on each function for its exact
//!   source. A seventh candidate (per-group project "wiring" beyond the five
//!   groups with a real corresponding `ade.json` module, and OSV/CocoIndex
//!   probes for groups with no such tool at all) was considered and is
//!   deliberately NOT here: there is no real data source for it without
//!   inventing an association the codebase does not itself make.

use crate::gui::inventory::{get_capability, with_timeout};
use crate::gui::jobs::{parse_utc_seconds, Job, JobStatus};
use crate::types::{ExecFn, ExecOpts, ExecResult};
use std::path::{Path, PathBuf};
use std::time::Duration;

const DAY_SECONDS: u64 = 86_400;

// ───────────────────────── Tier 1 — already have ─────────────────────────

/// One capability group's most recently touched provider, from job history.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JobRecency {
    pub capability_id: String,
    pub capability_name: String,
    pub action: String,
    pub failed: bool,
    pub exit_code: Option<i32>,
    /// Whole days since the job finished, floored — "update lag in days".
    pub days_ago: u64,
}

/// The most recently FINISHED job (installs/updates/uninstalls — never a
/// still-running one) among the providers belonging to `group`. `None` when
/// nothing in this group has ever had a job run through ADE — the common
/// case for a provider the owner installed by hand before ADE existed, or
/// never touched at all. Source: `jobs.json` (already loaded into `Shared`
/// by every caller) — no new I/O.
pub fn last_group_job(jobs: &[Job], group: &str, now_seconds: u64) -> Option<JobRecency> {
    jobs.iter()
        .filter(|job| job.status != JobStatus::Running)
        .filter_map(|job| {
            let def = get_capability(&job.capability_id)?;
            if def.capability != group {
                return None;
            }
            let stamp = job.finished_at.as_deref().unwrap_or(&job.started_at);
            let seconds = parse_utc_seconds(stamp)?;
            Some((seconds, job, def))
        })
        .max_by_key(|(seconds, _, _)| *seconds)
        .map(|(seconds, job, def)| JobRecency {
            capability_id: def.id.to_string(),
            capability_name: def.name.to_string(),
            action: job.action.clone(),
            failed: job.status == JobStatus::Error,
            exit_code: job.exit_code,
            days_ago: now_seconds.saturating_sub(seconds) / DAY_SECONDS,
        })
}

/// The project-module id whose apply/verify pipeline corresponds to a
/// capability group — grounded, not invented: each mapping below is
/// verified against that module's OWN `ctx.tool_present("<tool>")` call,
/// i.e. the module already treats that `CAPABILITIES` tool as ITS presence
/// signal, so the two concepts are provably the same thing wearing two
/// different id strings.
///
/// | group                | module           | grounding                                        |
/// |-----------------------|------------------|---------------------------------------------------|
/// | secret-scanning        | secrets           | `secrets.rs`: `ctx.tool_present("trufflehog")`     |
/// | sandboxing              | sandbox           | `sandbox.rs`: `ctx.tool_present("nono")`           |
/// | repo-hygiene            | git-hygiene       | `git_hygiene.rs`: `ctx.tool_present("ocean")`      |
/// | dependency-scanning     | supply-chain      | `supply_chain.rs`: `ctx.tool_present("osv-scanner")` |
/// | token-efficiency        | token-efficiency  | `token_efficiency.rs`: `ctx.tool_present("rtk")`   |
///
/// `None` for every other group (the common case): CodeGuard is a Claude
/// Code plugin outside the `ade.json` apply/verify pipeline; OpenWiki/
/// CocoIndex/ccc are informational tools with no module; hook-orchestration
/// is checked FROM WITHIN the `secrets` module (via `pre-commit`) rather
/// than owning a module of its own, so it has no clean 1:1 mapping either;
/// harness identity has no module at all. Those groups' "project coverage"
/// stat is honestly absent, not a fabricated zero.
pub fn module_for_group(group: &str) -> Option<&'static str> {
    match group {
        "secret-scanning" => Some("secrets"),
        "sandboxing" => Some("sandbox"),
        "repo-hygiene" => Some("git-hygiene"),
        "dependency-scanning" => Some("supply-chain"),
        "token-efficiency" => Some("token-efficiency"),
        _ => None,
    }
}

/// Whether a `StatusRow.state` (`report.rs`) counts as this module actually
/// covering the project: "applied" or "degraded" (configured and at least
/// partially verified) — never merely "the project is registered", and
/// never "disabled"/"not-applied".
pub fn module_state_covers(state: &str) -> bool {
    state == "applied" || state == "degraded"
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProjectCoverage {
    pub covered: usize,
    pub inspected: usize,
}

/// `covered_flags` is one bool per registered project this session actually
/// has a report for — `Shared.reports` is populated on demand (Inspect),
/// not eagerly for every registered project, so this is honestly scoped to
/// "of the projects looked at", never silently padded to "of all
/// registered projects". `None` when there is nothing to report over: no
/// project has been inspected yet, or (checked by the caller via
/// `module_for_group`) this group has no corresponding module at all.
pub fn project_coverage(covered_flags: &[bool]) -> Option<ProjectCoverage> {
    if covered_flags.is_empty() {
        return None;
    }
    Some(ProjectCoverage {
        covered: covered_flags.iter().filter(|flag| **flag).count(),
        inspected: covered_flags.len(),
    })
}

// ───────────────────────── Tier 2 — rtk gain ─────────────────────────

/// `rtk gain --format json` — verified against a real `rtk` build on this
/// machine: a stable `{"summary":{"total_commands","total_input",
/// "total_output","total_saved","avg_savings_pct","total_time_ms",
/// "avg_time_ms"}}` object. The per-command history/graph views are
/// text/CSV-only (no JSON path for them) — this stat sticks to the one
/// field the JSON output actually ships: the summary.
pub fn rtk_gain_argv() -> Vec<String> {
    vec![
        "rtk".to_string(),
        "gain".to_string(),
        "--format".to_string(),
        "json".to_string(),
    ]
}

#[derive(Debug, Clone, PartialEq)]
pub struct TokenGain {
    pub tokens_saved: u64,
    pub avg_savings_pct: f64,
    pub total_commands: u64,
}

/// `None` on non-JSON output, a JSON value missing `summary`, or a summary
/// missing any of the three fields used — never a zeroed-out struct standing
/// in for a probe that actually failed.
pub fn parse_rtk_gain(stdout: &str) -> Option<TokenGain> {
    let value: serde_json::Value = serde_json::from_str(stdout).ok()?;
    let summary = value.get("summary")?;
    Some(TokenGain {
        tokens_saved: summary.get("total_saved")?.as_u64()?,
        avg_savings_pct: summary.get("avg_savings_pct")?.as_f64()?,
        total_commands: summary.get("total_commands")?.as_u64()?,
    })
}

// ───────────────────────── Tier 2 — OSV database age ─────────────────────────

/// osv-scanner's own local vulnerability-database cache directory, verified
/// against the shipped v2.4.0 source
/// (`internal/clients/clientimpl/localmatcher/localmatcher.go::setupLocalDBDirectory`):
/// `os.UserCacheDir()` joined with `osv-scanner` — on macOS that resolves to
/// `~/Library/Caches/osv-scanner` — unless the scanner's own
/// `OSV_SCANNER_LOCAL_DB_CACHE_DIRECTORY` env var overrides it (an env
/// override is the caller's concern, not this pure path join's).
pub fn osv_scanner_cache_dir(home: &Path) -> PathBuf {
    home.join("Library").join("Caches").join("osv-scanner")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OsvDbAge {
    pub days_old: u64,
    /// How many ecosystem archives (`<ecosystem>/all.zip`) were found.
    pub ecosystems: usize,
}

/// The OLDEST `<ecosystem>/all.zip` under `cache_dir` — verified from the
/// same source: each ecosystem gets its own `{dbBasePath}/{ecosystem}/
/// all.zip`, never one combined file. The most-stale ecosystem is the
/// honest number to surface: a fresh `npm` archive next to a month-old `Go`
/// archive means Go advisories are a month stale regardless of how current
/// npm's archive is. `None` when the directory doesn't exist or holds no
/// archive at all — i.e. `--download-offline-databases` has never run on
/// this machine, the common state for a scanner that otherwise checks
/// live over the network.
pub fn osv_db_age(cache_dir: &Path, now_seconds: u64) -> Option<OsvDbAge> {
    let entries = std::fs::read_dir(cache_dir).ok()?;
    let mut oldest: Option<u64> = None;
    let mut ecosystems = 0usize;
    for entry in entries.flatten() {
        let Ok(modified) =
            std::fs::metadata(entry.path().join("all.zip")).and_then(|meta| meta.modified())
        else {
            continue;
        };
        let seconds = modified
            .duration_since(std::time::UNIX_EPOCH)
            .map(|elapsed| elapsed.as_secs())
            .unwrap_or(0);
        ecosystems += 1;
        oldest = Some(oldest.map_or(seconds, |current: u64| current.min(seconds)));
    }
    let oldest = oldest?;
    Some(OsvDbAge {
        days_old: now_seconds.saturating_sub(oldest) / DAY_SECONDS,
        ecosystems,
    })
}

// ───────────────────────── Tier 2 — semantic-index staleness ─────────────────────────

/// CocoIndex Code (`ccc`)'s per-project index database — verified against a
/// real indexed project on this machine: `.cocoindex_code/target_sqlite.db`
/// sits beside `settings.yml` and a `cocoindex.db/` directory. `ccc status`
/// itself prints no timestamp (its plain-text output was checked directly —
/// no date field anywhere in it), so the index file's own mtime is the only
/// honest freshness signal available without adding a dependency to parse
/// `ccc`'s output some other way.
pub fn cocoindex_index_path(project_dir: &Path) -> PathBuf {
    project_dir.join(".cocoindex_code").join("target_sqlite.db")
}

/// `git -C <dir> log -1 --format=%ct HEAD` — the HEAD commit's timestamp as
/// a unix time, the simplest single value that answers "how current is this
/// repo".
pub fn git_head_commit_argv(project_dir: &str) -> Vec<String> {
    vec![
        "git".to_string(),
        "-C".to_string(),
        project_dir.to_string(),
        "log".to_string(),
        "-1".to_string(),
        "--format=%ct".to_string(),
        "HEAD".to_string(),
    ]
}

/// `None` on a non-zero exit (not a git repo, no commits yet) or unparseable
/// stdout — never a zero standing in for "we don't actually know".
pub fn parse_unix_seconds(result: &ExecResult) -> Option<u64> {
    if result.code != 0 {
        return None;
    }
    result.stdout.trim().parse::<u64>().ok()
}

/// Whole days the index is behind HEAD. Zero or negative — the index is at
/// least as new as the latest commit — both mean "current"; the caller
/// clamps for display rather than this pure function inventing a floor.
pub fn index_staleness_days(index_mtime_seconds: u64, head_commit_seconds: u64) -> i64 {
    (head_commit_seconds as i64 - index_mtime_seconds as i64) / DAY_SECONDS as i64
}

// ───────────────────────── Tier 2 — hook-chain latency ─────────────────────────

/// The hook git itself invokes on every commit — real, not synthetic:
/// whichever of the two shapes `secrets.rs` installs (the `pre-commit`
/// framework's own generated hook, or ADE's native shim when that framework
/// is absent), the file that actually runs on `git commit` lives here.
pub fn pre_commit_hook_path(project_dir: &Path) -> PathBuf {
    project_dir.join(".git").join("hooks").join("pre-commit")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HookLatency {
    pub millis: u64,
}

/// Time ONE real invocation of `hook_path` with nothing staged — the same
/// no-op-shaped run git performs when a commit's scanner has nothing new to
/// look at, so the wall time is the genuine "your commits pay Nms" floor
/// cost, not a synthetic benchmark. Bounded by `timeout`: a pathological
/// first run (the `pre-commit` framework bootstrapping a fresh virtualenv
/// can take tens of seconds) reports absent rather than blocking the caller
/// or being reported as a real steady-state cost.
pub fn time_hook_run(
    exec: &ExecFn,
    hook_path: &str,
    cwd: &Path,
    timeout: Duration,
) -> Option<HookLatency> {
    let bounded = with_timeout(exec.clone(), timeout);
    let started = std::time::Instant::now();
    let result = bounded(
        &[hook_path.to_string()],
        &ExecOpts {
            cwd: Some(cwd.to_path_buf()),
            stdin: None,
        },
    );
    // `with_timeout`'s own sentinel (inventory.rs) for "the probe was killed
    // by the deadline" — its elapsed time is the timeout, not a real
    // measurement, so it must not be reported as one.
    if result.code == 124 && result.stderr.contains("timed out") {
        return None;
    }
    Some(HookLatency {
        millis: started.elapsed().as_millis() as u64,
    })
}

// ───────────────────────── Tier 2 — CodeGuard harness surfaces ─────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HarnessSurface {
    pub harness_id: &'static str,
    pub harness_name: &'static str,
    pub covered: bool,
}

/// Real, vendor-documented per-harness install locations for the CodeGuard
/// ruleset (CoSAI/OASIS `project-codeguard`, the same marketplace
/// `CODEGUARD_MARKETPLACE` names in `inventory.rs`) BEYOND Claude Code —
/// Claude Code's own coverage is already answered by the `codeguard`
/// capability's plugin probe in `inventory.rs`, so it is not repeated here.
/// Each path below was verified present on a real CodeGuard install on this
/// machine: Codex reads `~/.agents/skills/codeguard`; OpenCode
/// `~/.config/opencode/skills/codeguard`; Cursor any
/// `~/.cursor/rules/codeguard-*.mdc`; Antigravity's single combined global
/// rules file `~/.gemini/GEMINI.md` IF its content actually mentions
/// CodeGuard (that file is shared with everything else Antigravity loads,
/// so mere presence proves nothing — content is the honest check).
///
/// Hermes and Pi are deliberately absent from this list: no vendor-
/// documented install path for either was found while researching this
/// probe, and guessing one would be exactly the fabrication the house rule
/// forbids — they are omitted from the denominator entirely, not counted
/// as "not covered".
pub fn codeguard_harness_surfaces(home: &Path) -> Vec<HarnessSurface> {
    vec![
        HarnessSurface {
            harness_id: "codex",
            harness_name: "Codex",
            covered: home
                .join(".agents")
                .join("skills")
                .join("codeguard")
                .is_dir(),
        },
        HarnessSurface {
            harness_id: "opencode",
            harness_name: "OpenCode",
            covered: home
                .join(".config")
                .join("opencode")
                .join("skills")
                .join("codeguard")
                .is_dir(),
        },
        HarnessSurface {
            harness_id: "cursor",
            harness_name: "Cursor",
            covered: dir_has_prefixed_entry(&home.join(".cursor").join("rules"), "codeguard-"),
        },
        HarnessSurface {
            harness_id: "antigravity",
            harness_name: "Antigravity",
            covered: crate::fsutil::read_if_exists(&home.join(".gemini").join("GEMINI.md"))
                .is_some_and(|text| text.contains("CodeGuard")),
        },
    ]
}

fn dir_has_prefixed_entry(dir: &Path, prefix: &str) -> bool {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return false;
    };
    entries
        .flatten()
        .any(|entry| entry.file_name().to_string_lossy().starts_with(prefix))
}

// ───────────────────────── Tier 2 — hardened-repo git config ─────────────────────────

/// The exact signal `git_hygiene.rs`'s own `signing_finding` already checks
/// as its hardening indicator — reused verbatim (same argv) so this stat can
/// never disagree with the module that owns the concept.
pub fn git_signing_argv(project_dir: &str) -> Vec<String> {
    vec![
        "git".to_string(),
        "-C".to_string(),
        project_dir.to_string(),
        "config".to_string(),
        "--get".to_string(),
        "commit.gpgsign".to_string(),
    ]
}

pub fn parse_git_signing(result: &ExecResult) -> bool {
    result.code == 0 && result.stdout.trim() == "true"
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HardenedRepoCoverage {
    pub hardened: usize,
    pub total: usize,
}

/// `None` when there is nothing to report over — no registered project.
pub fn hardened_repo_coverage(flags: &[bool]) -> Option<HardenedRepoCoverage> {
    if flags.is_empty() {
        return None;
    }
    Some(HardenedRepoCoverage {
        hardened: flags.iter().filter(|flag| **flag).count(),
        total: flags.len(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gui::jobs::now_utc_seconds;
    use crate::testutil::{fake_exec, make_temp_dir};
    use std::fs;
    use std::sync::Arc;

    fn job(id: &str, cap: &str, action: &str, status: JobStatus, finished_at: &str) -> Job {
        Job {
            id: id.to_string(),
            capability_id: cap.to_string(),
            action: action.to_string(),
            status,
            started_at: finished_at.to_string(),
            finished_at: (status != JobStatus::Running).then(|| finished_at.to_string()),
            exit_code: match status {
                JobStatus::Ok => Some(0),
                JobStatus::Error => Some(1),
                JobStatus::Running => None,
            },
            log: Vec::new(),
        }
    }

    // ── Tier 1 ──

    #[test]
    fn last_group_job_ignores_other_groups_and_running_jobs_and_picks_the_newest() {
        let now = 2_000_000u64;
        let jobs = vec![
            // Different group entirely (rtk -> token-efficiency).
            job(
                "j1",
                "rtk",
                "install",
                JobStatus::Ok,
                &now_utc_seconds(now - 5 * DAY_SECONDS),
            ),
            // Same group (trufflehog -> secret-scanning) but older.
            job(
                "j2",
                "trufflehog",
                "install",
                JobStatus::Ok,
                &now_utc_seconds(now - 10 * DAY_SECONDS),
            ),
            // Same group, newer, and this one is the answer.
            job(
                "j3",
                "gitleaks",
                "update",
                JobStatus::Error,
                &now_utc_seconds(now - 2 * DAY_SECONDS),
            ),
            // Same group but still running — must never win even though its
            // (fallback) timestamp is the newest.
            job(
                "j4",
                "gitleaks",
                "install",
                JobStatus::Running,
                &now_utc_seconds(now),
            ),
        ];
        let recency = last_group_job(&jobs, "secret-scanning", now).expect("a finished job exists");
        assert_eq!(recency.capability_id, "gitleaks");
        assert_eq!(recency.action, "update");
        assert!(recency.failed);
        assert_eq!(recency.exit_code, Some(1));
        assert_eq!(recency.days_ago, 2);
    }

    #[test]
    fn last_group_job_is_absent_when_nothing_in_the_group_has_a_job() {
        let jobs = vec![job(
            "j1",
            "rtk",
            "install",
            JobStatus::Ok,
            &now_utc_seconds(1_000_000),
        )];
        assert!(last_group_job(&jobs, "secret-scanning", 1_000_000).is_none());
        assert!(last_group_job(&[], "token-efficiency", 1_000_000).is_none());
    }

    #[test]
    fn last_group_job_skips_a_job_whose_capability_id_is_unknown() {
        // A retired capability id left in old jobs.json history — must be
        // skipped, never panic get_capability's caller.
        let jobs = vec![job(
            "j1",
            "no-longer-exists",
            "install",
            JobStatus::Ok,
            &now_utc_seconds(1_000_000),
        )];
        assert!(last_group_job(&jobs, "secret-scanning", 1_000_000).is_none());
    }

    #[test]
    fn module_for_group_covers_the_five_grounded_mappings_and_nothing_else() {
        assert_eq!(module_for_group("secret-scanning"), Some("secrets"));
        assert_eq!(module_for_group("sandboxing"), Some("sandbox"));
        assert_eq!(module_for_group("repo-hygiene"), Some("git-hygiene"));
        assert_eq!(
            module_for_group("dependency-scanning"),
            Some("supply-chain")
        );
        assert_eq!(
            module_for_group("token-efficiency"),
            Some("token-efficiency")
        );
        for ungrounded in [
            "agent-security-rules",
            "hook-orchestration",
            "codebase-wiki",
            "semantic-search",
            "coding-harness",
            "not-a-real-group",
        ] {
            assert_eq!(module_for_group(ungrounded), None, "{ungrounded}");
        }
    }

    #[test]
    fn module_state_covers_only_applied_and_degraded() {
        assert!(module_state_covers("applied"));
        assert!(module_state_covers("degraded"));
        assert!(!module_state_covers("not-applied"));
        assert!(!module_state_covers("disabled"));
        assert!(!module_state_covers(""));
    }

    #[test]
    fn project_coverage_counts_and_is_absent_when_nothing_was_inspected() {
        assert!(project_coverage(&[]).is_none());
        let coverage = project_coverage(&[true, false, true]).expect("real data");
        assert_eq!(coverage.covered, 2);
        assert_eq!(coverage.inspected, 3);
    }

    // ── Tier 2: rtk gain ──

    #[test]
    fn rtk_gain_argv_matches_the_real_json_invocation() {
        assert_eq!(rtk_gain_argv(), vec!["rtk", "gain", "--format", "json"]);
    }

    #[test]
    fn parse_rtk_gain_reads_the_real_summary_shape() {
        // Captured verbatim from a real `rtk gain --format json` run.
        let stdout = r#"{
          "summary": {
            "total_commands": 12129,
            "total_input": 381108800,
            "total_output": 70769486,
            "total_saved": 310350059,
            "avg_savings_pct": 81.43345391132401,
            "total_time_ms": 51143365,
            "avg_time_ms": 4216
          }
        }"#;
        let gain = parse_rtk_gain(stdout).expect("real fixture parses");
        assert_eq!(gain.tokens_saved, 310_350_059);
        assert_eq!(gain.total_commands, 12_129);
        assert!((gain.avg_savings_pct - 81.433).abs() < 0.01);
    }

    #[test]
    fn parse_rtk_gain_degrades_on_malformed_or_incomplete_output() {
        assert!(parse_rtk_gain("not json").is_none());
        assert!(parse_rtk_gain(r#"{"nope": true}"#).is_none());
        assert!(parse_rtk_gain(r#"{"summary": {"total_saved": 5}}"#).is_none());
        assert!(parse_rtk_gain("").is_none());
    }

    // ── Tier 2: OSV database age ──

    #[test]
    fn osv_scanner_cache_dir_matches_the_verified_localmatcher_layout() {
        let home = Path::new("/Users/x");
        assert_eq!(
            osv_scanner_cache_dir(home),
            PathBuf::from("/Users/x/Library/Caches/osv-scanner")
        );
    }

    fn set_mtime(path: &Path, seconds_ago: u64, now: std::time::SystemTime) {
        let when = now - Duration::from_secs(seconds_ago);
        let file = fs::File::options()
            .write(true)
            .open(path)
            .expect("open for mtime");
        file.set_modified(when).expect("set mtime");
    }

    #[test]
    fn osv_db_age_reports_the_oldest_ecosystem_archive() {
        let dir = make_temp_dir("osv-db-age");
        let now_system = std::time::SystemTime::now();
        let now_seconds = now_system
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        fs::create_dir_all(dir.join("npm")).unwrap();
        fs::write(dir.join("npm").join("all.zip"), b"npm").unwrap();
        set_mtime(
            &dir.join("npm").join("all.zip"),
            2 * DAY_SECONDS,
            now_system,
        );

        fs::create_dir_all(dir.join("Go")).unwrap();
        fs::write(dir.join("Go").join("all.zip"), b"go").unwrap();
        set_mtime(
            &dir.join("Go").join("all.zip"),
            30 * DAY_SECONDS,
            now_system,
        );

        let age = osv_db_age(&dir, now_seconds).expect("two archives present");
        assert_eq!(age.ecosystems, 2);
        assert_eq!(age.days_old, 30);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn osv_db_age_is_absent_when_never_downloaded() {
        let missing = Path::new("/tmp/definitely-does-not-exist-osv-scanner-cache");
        assert!(osv_db_age(missing, 1_000_000).is_none());

        // Directory exists but has never actually downloaded an archive.
        let dir = make_temp_dir("osv-db-age-empty");
        fs::create_dir_all(dir.join("npm")).unwrap();
        assert!(osv_db_age(&dir, 1_000_000).is_none());
        let _ = fs::remove_dir_all(&dir);
    }

    // ── Tier 2: semantic-index staleness ──

    #[test]
    fn cocoindex_index_path_matches_the_verified_layout() {
        let dir = Path::new("/repo");
        assert_eq!(
            cocoindex_index_path(dir),
            PathBuf::from("/repo/.cocoindex_code/target_sqlite.db")
        );
    }

    #[test]
    fn git_head_commit_argv_and_parsing() {
        assert_eq!(
            git_head_commit_argv("/repo"),
            vec!["git", "-C", "/repo", "log", "-1", "--format=%ct", "HEAD"]
        );
        let ok = ExecResult {
            code: 0,
            stdout: "1700000000\n".into(),
            stderr: String::new(),
        };
        assert_eq!(parse_unix_seconds(&ok), Some(1_700_000_000));
        let not_a_repo = ExecResult {
            code: 128,
            stdout: String::new(),
            stderr: "fatal: not a git repository".into(),
        };
        assert!(parse_unix_seconds(&not_a_repo).is_none());
        let garbage = ExecResult {
            code: 0,
            stdout: "not-a-number".into(),
            stderr: String::new(),
        };
        assert!(parse_unix_seconds(&garbage).is_none());
    }

    #[test]
    fn index_staleness_days_measures_both_directions() {
        let head = 1_000_000_u64 + 10 * DAY_SECONDS;
        assert_eq!(index_staleness_days(1_000_000, head), 10);
        // Index built after the latest commit: current, expressed as <= 0.
        assert_eq!(index_staleness_days(head, 1_000_000), -10);
        assert_eq!(index_staleness_days(1_000_000, 1_000_000), 0);
    }

    // ── Tier 2: hook-chain latency ──

    #[test]
    fn pre_commit_hook_path_matches_gits_own_convention() {
        assert_eq!(
            pre_commit_hook_path(Path::new("/repo")),
            PathBuf::from("/repo/.git/hooks/pre-commit")
        );
    }

    #[test]
    fn time_hook_run_measures_a_fast_real_invocation() {
        let exec = fake_exec(&[("/repo/.git/hooks/pre-commit", (0, "ok\n", ""))]);
        let latency = time_hook_run(
            &exec,
            "/repo/.git/hooks/pre-commit",
            Path::new("/repo"),
            Duration::from_secs(5),
        )
        .expect("a completed run reports latency");
        // Millis is a real (small) number, not asserted to an exact value —
        // wall-clock timing is inherently non-deterministic, but it must be
        // bounded well under the 5s timeout for a fake exec that returns
        // instantly.
        assert!(latency.millis < 4_000);
    }

    #[test]
    fn time_hook_run_reports_absent_rather_than_a_fabricated_duration_on_timeout() {
        let hung: ExecFn = Arc::new(|_argv, _opts| {
            std::thread::sleep(Duration::from_secs(60));
            ExecResult {
                code: 0,
                stdout: String::new(),
                stderr: String::new(),
            }
        });
        let latency = time_hook_run(
            &hung,
            "/repo/.git/hooks/pre-commit",
            Path::new("/repo"),
            Duration::from_millis(50),
        );
        assert!(latency.is_none());
    }

    // ── Tier 2: CodeGuard harness surfaces ──

    #[test]
    fn codeguard_harness_surfaces_reflect_exactly_what_is_on_disk() {
        let home = make_temp_dir("codeguard-surfaces");

        // Codex: covered.
        fs::create_dir_all(home.join(".agents").join("skills").join("codeguard")).unwrap();
        // OpenCode: absent — no dir created at all.
        // Cursor: covered via a prefixed file amongst unrelated ones.
        fs::create_dir_all(home.join(".cursor").join("rules")).unwrap();
        fs::write(
            home.join(".cursor")
                .join("rules")
                .join("codeguard-0-crypto.mdc"),
            "rule",
        )
        .unwrap();
        fs::write(
            home.join(".cursor").join("rules").join("unrelated.mdc"),
            "rule",
        )
        .unwrap();
        // Antigravity: file exists but does NOT mention CodeGuard — must not count.
        fs::create_dir_all(home.join(".gemini")).unwrap();
        fs::write(
            home.join(".gemini").join("GEMINI.md"),
            "# just some other rules",
        )
        .unwrap();

        let surfaces = codeguard_harness_surfaces(&home);
        assert_eq!(surfaces.len(), 4);
        let covered = |id: &str| {
            surfaces
                .iter()
                .find(|s| s.harness_id == id)
                .unwrap()
                .covered
        };
        assert!(covered("codex"));
        assert!(!covered("opencode"));
        assert!(covered("cursor"));
        assert!(!covered("antigravity"));

        // Now put a real CodeGuard marker into GEMINI.md — must flip to covered.
        fs::write(
            home.join(".gemini").join("GEMINI.md"),
            "# Global rules\nCodeGuard secure-coding baseline\n",
        )
        .unwrap();
        let surfaces_after = codeguard_harness_surfaces(&home);
        assert!(
            surfaces_after
                .iter()
                .find(|s| s.harness_id == "antigravity")
                .unwrap()
                .covered
        );

        let _ = fs::remove_dir_all(&home);
    }

    #[test]
    fn codeguard_harness_surfaces_on_a_bare_home_are_all_uncovered() {
        let home = make_temp_dir("codeguard-surfaces-empty");
        let surfaces = codeguard_harness_surfaces(&home);
        assert!(surfaces.iter().all(|surface| !surface.covered));
        let _ = fs::remove_dir_all(&home);
    }

    // ── Tier 2: hardened-repo git config ──

    #[test]
    fn git_signing_argv_and_parsing() {
        assert_eq!(
            git_signing_argv("/repo"),
            vec!["git", "-C", "/repo", "config", "--get", "commit.gpgsign"]
        );
        assert!(parse_git_signing(&ExecResult {
            code: 0,
            stdout: "true\n".into(),
            stderr: String::new(),
        }));
        assert!(!parse_git_signing(&ExecResult {
            code: 0,
            stdout: "false\n".into(),
            stderr: String::new(),
        }));
        // Not set at all: git exits non-zero.
        assert!(!parse_git_signing(&ExecResult {
            code: 1,
            stdout: String::new(),
            stderr: String::new(),
        }));
    }

    #[test]
    fn hardened_repo_coverage_counts_and_is_absent_with_no_projects() {
        assert!(hardened_repo_coverage(&[]).is_none());
        let coverage = hardened_repo_coverage(&[true, true, false]).expect("real data");
        assert_eq!(coverage.hardened, 2);
        assert_eq!(coverage.total, 3);
    }
}
