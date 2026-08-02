//! Phase I — the posture evidence export.
//!
//! `ade export posture` and the Control Center's "Export report" button both
//! call `collect_posture` — the SAME function — so the two surfaces can never
//! silently disagree about what shipped. Everything in the report is real,
//! already-probed data: `collect_posture` runs the identical detection pass
//! `ade gui health` runs (`gui::inventory::detect_capabilities` +
//! `gui::verdict::build_verdict`), re-derives every registered project's
//! module status the way `ade status` does (`report::status_report`), and
//! reuses Phase G's own Tier-1/Tier-2 stat functions (`gui::stats`) for job
//! history and the one real intervention metric (`rtk gain`). Nothing here
//! invents a fact `gui::stats`/`gui::verdict`/`report.rs` did not already
//! compute from a live probe.
//!
//! Two runs against IDENTICAL state (same `gui.json`, same `jobs.json`, same
//! registered-project configs, same injected clock) produce byte-identical
//! Markdown and JSON: every collection this module builds is placed into a
//! fixed, explicit order before rendering (capability declaration order,
//! capability-group taxonomy order, or an explicit sort), and `render_json`
//! goes through `fsutil::stable_stringify`, which recursively sorts object
//! keys — so no `HashMap`/`BTreeMap`-vs-registration-order accident can leak
//! into either output. See `collect_posture_is_byte_deterministic_for_identical_state`.
//!
//! Hard rule (the same class of discipline `scripts/emoji-ban.sh` enforces
//! for UI glyphs, applied here to filesystem paths instead): the export must
//! never contain an absolute home-directory path. Every string that could
//! carry one — a registered project's directory, a `load_config` error that
//! names one inline — is passed through `redact_home` before it reaches
//! `PostureReport`. See `collect_posture_never_embeds_the_home_directory`.

use crate::config::load_config;
use crate::fsutil::stable_stringify;
use crate::gui::inventory::{
    detect_capabilities, get_group, with_timeout, DetectOptions, CAPABILITY_GROUPS,
};
use crate::gui::jobs::{now_utc_seconds, read_last_finished, JobRunner};
use crate::gui::state::load_gui_state;
use crate::gui::stats;
use crate::gui::verdict::{build_verdict, AttentionRank, CoverageState, HealthCounts, Verdict};
use crate::harness::adapters::HARNESS_ADAPTERS;
use crate::registry::module_ids;
use crate::report::{status_report, StatusRow};
use crate::run::{make_ctx, PipelineDeps};
use crate::types::{ExecFn, ExecOpts, WhichFn};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

fn harness_ids() -> Vec<&'static str> {
    HARNESS_ADAPTERS.iter().map(|adapter| adapter.id).collect()
}

/// Clock seam, second resolution (posture never needs sub-second precision):
/// the same injection shape `run::PipelineDeps.now` already uses, so a
/// determinism test can hold "now" fixed across two collections instead of
/// racing the real wall clock.
pub type NowSecondsFn = Arc<dyn Fn() -> u64 + Send + Sync>;

pub struct PostureDeps {
    pub exec: ExecFn,
    pub which: WhichFn,
    pub ade_home: PathBuf,
    /// The real `$HOME` (distinct from `$ADE_HOME`) — every project path is
    /// redacted against this before it reaches the report. `None` only in
    /// the pathological case `$HOME` itself is unset; paths then render
    /// absolute (there is nothing to redact against) rather than guessed.
    pub user_home: Option<PathBuf>,
    pub now_seconds: Option<NowSecondsFn>,
}

fn real_now_seconds() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs())
        .unwrap_or(0)
}

/// Replace every occurrence of the real home directory with `~`. Applied to
/// every string that could conceivably carry a filesystem path — not just
/// fields typed as paths, since `load_config`'s own error text embeds
/// `target_dir.display()` inline in a sentence ("ade.json not found in
/// {path} — run `ade init` first"), not as a standalone field.
pub fn redact_home(text: &str, home: Option<&Path>) -> String {
    match home.and_then(|dir| dir.to_str()) {
        Some(home_str) if !home_str.is_empty() && text.contains(home_str) => {
            text.replace(home_str, "~")
        }
        _ => text.to_string(),
    }
}

// ───────────────────────── report shape ─────────────────────────

#[derive(Debug, Clone, serde::Serialize)]
pub struct PostureCapability {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub group_id: String,
    pub group_name: String,
    pub method: String,
    pub enabled: bool,
    pub installed: bool,
    pub healthy: bool,
    pub version: Option<String>,
    pub latest_version: Option<String>,
    pub update_available: bool,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct PostureCoverage {
    pub group_id: String,
    pub group_name: String,
    pub state: String,
    pub summary: String,
    pub provider: Option<String>,
    pub provider_version: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct PostureIssue {
    pub rank: String,
    pub group_id: String,
    pub group_name: String,
    pub capability_id: String,
    pub title: String,
    pub why: String,
    pub note: Option<String>,
    pub action_label: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct PostureMatrixCell {
    pub group_id: String,
    pub group_name: String,
    /// "wired" | "not-wired" | "not-applicable" | "error"
    pub state: String,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct PostureProject {
    /// `~`-relative when the project lives under `$HOME`, absolute otherwise
    /// (never a raw path under the real home directory — see `redact_home`).
    pub path: String,
    pub config_ok: bool,
    pub error: Option<String>,
    pub matrix: Vec<PostureMatrixCell>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct PostureJobHistory {
    pub group_id: String,
    pub group_name: String,
    pub capability_name: String,
    pub action: String,
    /// "ok" | "failed"
    pub outcome: String,
    pub days_ago: u64,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct PostureIntervention {
    pub group_id: String,
    pub group_name: String,
    pub metric: String,
    pub value: String,
    pub source: String,
}

/// A Tier-3 "what did it catch" candidate this session researched and
/// deliberately did NOT build an adapter for, with exactly why — the
/// documented-follow-up half of the house rule ("if no real retained output
/// exists, do not build a fake adapter — state exactly what is missing").
#[derive(Debug, Clone, serde::Serialize)]
pub struct Tier3Status {
    pub group_id: String,
    pub group_name: String,
    pub tool: String,
    /// "no-retained-output" | "not-invoked"
    pub status: String,
    pub note: String,
}

#[derive(Debug, Clone, Copy, serde::Serialize)]
pub struct PostureCounts {
    pub groups_total: usize,
    pub groups_covered: usize,
    pub enabled: u32,
    pub ok: u32,
    pub warnings: u32,
    pub errors: u32,
    pub missing: u32,
}

impl From<HealthCounts> for PostureCounts {
    fn from(counts: HealthCounts) -> Self {
        PostureCounts {
            groups_total: counts.groups_total,
            groups_covered: counts.groups_covered,
            enabled: counts.enabled,
            ok: counts.ok,
            warnings: counts.warnings,
            errors: counts.errors,
            missing: counts.missing,
        }
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct PostureReport {
    pub generated_at: String,
    /// "sound" | "needs-attention" | "not-working" | "unknown"
    pub verdict: String,
    pub headline: String,
    pub detail: String,
    pub counts: PostureCounts,
    pub capabilities: Vec<PostureCapability>,
    pub coverage: Vec<PostureCoverage>,
    /// Broken/uncovered items only — the same population `HealthVerdict::action_items()`
    /// puts in front of the owner; spares/updates/insights are not "issues".
    pub issues: Vec<PostureIssue>,
    pub projects: Vec<PostureProject>,
    pub job_history: Vec<PostureJobHistory>,
    pub interventions: Vec<PostureIntervention>,
    pub tier3_status: Vec<Tier3Status>,
}

// ───────────────────────── word mappings ─────────────────────────

fn verdict_word(verdict: Verdict) -> &'static str {
    match verdict {
        Verdict::Sound => "sound",
        Verdict::NeedsAttention => "needs-attention",
        Verdict::NotWorking => "not-working",
        Verdict::Unknown => "unknown",
    }
}

fn coverage_state_word(state: CoverageState) -> &'static str {
    match state {
        CoverageState::Covered => "covered",
        CoverageState::Broken => "broken",
        CoverageState::Uncovered => "uncovered",
        CoverageState::Off => "off",
    }
}

fn attention_rank_word(rank: AttentionRank) -> &'static str {
    match rank {
        AttentionRank::Broken => "broken",
        AttentionRank::Uncovered => "uncovered",
        AttentionRank::Insight => "insight",
        AttentionRank::Spare => "spare",
        AttentionRank::Update => "update",
    }
}

/// One coverage-matrix cell — the exact same classification
/// `ade-control-center::projects::matrix_cell` computes for the live UI
/// (`module_for_group` + `module_state_covers`, Phase G/H's own functions),
/// expressed over an already-fetched `&[StatusRow]` instead of an
/// `Option<&ProjectReport>` since posture always inspects fresh rather than
/// reading a maybe-cached report. `status_report` unconditionally emits every
/// module, so "error" here is defensive only — the honest surface for a
/// project whose module set does not (for whatever reason) include the
/// expected id, never assumed unreachable.
fn matrix_cell_word(rows: &[StatusRow], group_id: &str) -> &'static str {
    let Some(module_id) = stats::module_for_group(group_id) else {
        return "not-applicable";
    };
    match rows.iter().find(|row| row.id == module_id) {
        Some(row) if stats::module_state_covers(&row.state) => "wired",
        Some(_) => "not-wired",
        None => "error",
    }
}

/// Tier 3 — researched, not fabricated: exactly which named candidates
/// (Gitleaks/TruffleHog report files from hook runs, per the plan) have a
/// real retained output today, and why the two that do not are absent rather
/// than faked. Traced against the secrets module's ACTUAL invocations
/// (`modules::secrets::hook_script` and `modules::secrets::pre_commit_config`
/// — grep both: neither ever passes `--json`/an output-file flag, and the
/// native shim additionally redirects both streams to `/dev/null`), not a
/// guess about what the tools are capable of in general.
fn tier3_status_rows() -> Vec<Tier3Status> {
    vec![
        Tier3Status {
            group_id: "secret-scanning".to_string(),
            group_name: "Secret Scanning".to_string(),
            tool: "TruffleHog".to_string(),
            status: "no-retained-output".to_string(),
            note: "the secrets module's installed hook invokes TruffleHog \
                   two ways depending on what is present — the native shim \
                   (`.git/hooks/pre-commit`) runs `trufflehog filesystem \"$tmpdir\" \
                   --results=verified --fail --no-update >/dev/null 2>&1` \
                   (output explicitly discarded), and the pre-commit-framework \
                   config runs the same command with no output flag at all. \
                   Neither writes a JSON or any other report file, so there \
                   is nothing on disk today to parse a finding count from."
                .to_string(),
        },
        Tier3Status {
            group_id: "secret-scanning".to_string(),
            group_name: "Secret Scanning".to_string(),
            tool: "Gitleaks".to_string(),
            status: "not-invoked".to_string(),
            note: "Gitleaks is a detected, optional complementary provider \
                   (it can satisfy Secret Scanning coverage on its own) but \
                   the secrets module never runs it — only TruffleHog is \
                   wired into the commit-boundary hook — so there is no run \
                   to produce a report from."
                .to_string(),
        },
    ]
}

// ───────────────────────── collection ─────────────────────────

/// Build the full posture report from real, freshly-probed state. Runs the
/// identical detection pass `ade gui health` runs, re-derives every
/// registered project's module status the way `ade status` does, and adds
/// Phase G/H's own Tier-1 job history plus the one real Tier-2 intervention
/// metric (`rtk gain`, gated exactly like `insights::token_gain_affirmation`
/// so a freshly-installed, never-used `rtk` never renders a degenerate
/// "saved 0 tokens across 0 commands" line).
pub fn collect_posture(deps: &PostureDeps) -> PostureReport {
    let now = deps
        .now_seconds
        .as_ref()
        .map(|clock| clock())
        .unwrap_or_else(real_now_seconds);
    let load = load_gui_state(&deps.ade_home);
    let jobs = JobRunner::new(deps.exec.clone(), Some(deps.ade_home.clone())).list();
    let last_jobs = read_last_finished(&deps.ade_home);
    let capabilities = detect_capabilities(&DetectOptions {
        exec: deps.exec.clone(),
        which: deps.which.clone(),
        disabled: &load.state.disabled,
        last_jobs: &last_jobs,
        latest_versions: &BTreeMap::new(),
        probe_running: true,
        probe_timeout: Duration::from_secs(10),
    });
    let verdict = build_verdict(&capabilities);

    let capability_rows: Vec<PostureCapability> = capabilities
        .iter()
        .map(|cap| {
            let group = get_group(&cap.capability);
            PostureCapability {
                id: cap.id.clone(),
                name: cap.name.clone(),
                kind: cap.kind.as_str().to_string(),
                group_id: cap.capability.clone(),
                group_name: group.map(|g| g.name.to_string()).unwrap_or_default(),
                method: cap.method.as_str().to_string(),
                enabled: cap.enabled,
                installed: cap.installed,
                healthy: cap.is_healthy(),
                version: cap.short_version(),
                latest_version: cap.latest_version.clone(),
                update_available: cap.update_available,
            }
        })
        .collect();

    let coverage_rows: Vec<PostureCoverage> = verdict
        .coverage
        .iter()
        .map(|row| PostureCoverage {
            group_id: row.group_id.clone(),
            group_name: row.group_name.clone(),
            state: coverage_state_word(row.state).to_string(),
            summary: row.summary.clone(),
            provider: row.provider.clone(),
            provider_version: row.provider_version.clone(),
        })
        .collect();

    let issue_rows: Vec<PostureIssue> = verdict
        .action_items()
        .map(|item| PostureIssue {
            rank: attention_rank_word(item.rank).to_string(),
            group_id: item.group_id.clone(),
            group_name: item.group_name.clone(),
            capability_id: item.capability_id.clone(),
            title: item.title.clone(),
            why: item.why.clone(),
            note: item.note.clone(),
            action_label: item.action.as_ref().map(|action| action.label.clone()),
        })
        .collect();

    let pipeline_deps = PipelineDeps {
        exec: deps.exec.clone(),
        which: deps.which.clone(),
        now: None,
    };
    let mut projects: Vec<PostureProject> = Vec::new();
    for dir in &load.state.projects {
        let redacted_path = redact_home(dir, deps.user_home.as_deref());
        match load_config(Path::new(dir), &module_ids(), &harness_ids()) {
            Ok(config) => {
                let ctx = make_ctx(Path::new(dir), config, &pipeline_deps);
                let rows = status_report(&ctx);
                let matrix = CAPABILITY_GROUPS
                    .iter()
                    .map(|group| PostureMatrixCell {
                        group_id: group.id.to_string(),
                        group_name: group.name.to_string(),
                        state: matrix_cell_word(&rows, group.id).to_string(),
                    })
                    .collect();
                projects.push(PostureProject {
                    path: redacted_path,
                    config_ok: true,
                    error: None,
                    matrix,
                });
            }
            Err(error) => {
                let matrix = CAPABILITY_GROUPS
                    .iter()
                    .map(|group| PostureMatrixCell {
                        group_id: group.id.to_string(),
                        group_name: group.name.to_string(),
                        state: "error".to_string(),
                    })
                    .collect();
                projects.push(PostureProject {
                    path: redacted_path,
                    config_ok: false,
                    error: Some(redact_home(&error, deps.user_home.as_deref())),
                    matrix,
                });
            }
        }
    }
    // Sorted, not insertion-order: two collections over identical state agree
    // trivially either way, but a sort also means the export's project order
    // never depends on WHEN each one happened to be registered.
    projects.sort_by(|a, b| a.path.cmp(&b.path));

    let job_history: Vec<PostureJobHistory> = CAPABILITY_GROUPS
        .iter()
        .filter_map(|group| {
            stats::last_group_job(&jobs, group.id, now).map(|recency| PostureJobHistory {
                group_id: group.id.to_string(),
                group_name: group.name.to_string(),
                capability_name: recency.capability_name,
                action: recency.action,
                outcome: if recency.failed { "failed" } else { "ok" }.to_string(),
                days_ago: recency.days_ago,
            })
        })
        .collect();

    let mut interventions: Vec<PostureIntervention> = Vec::new();
    if let Some(rtk) = capabilities.iter().find(|cap| cap.id == "rtk") {
        if rtk.installed {
            let bounded = with_timeout(deps.exec.clone(), Duration::from_secs(5));
            let result = bounded(&stats::rtk_gain_argv(), &ExecOpts::default());
            if result.code == 0 {
                if let Some(gain) = stats::parse_rtk_gain(&result.stdout) {
                    // Same de-degenerate gate `insights::token_gain_affirmation`
                    // uses: real zero data is not a useful intervention line.
                    if gain.total_commands > 0 {
                        interventions.push(PostureIntervention {
                            group_id: "token-efficiency".to_string(),
                            group_name: "Token Efficiency".to_string(),
                            metric: "tokens_saved".to_string(),
                            value: format!(
                                "{} tokens saved across {} commands ({:.0}% average reduction)",
                                gain.tokens_saved, gain.total_commands, gain.avg_savings_pct
                            ),
                            source: "rtk gain --format json".to_string(),
                        });
                    }
                }
            }
        }
    }

    PostureReport {
        generated_at: now_utc_seconds(now),
        verdict: verdict_word(verdict.verdict).to_string(),
        headline: verdict.headline.clone(),
        detail: verdict.detail.clone(),
        counts: verdict.counts.into(),
        capabilities: capability_rows,
        coverage: coverage_rows,
        issues: issue_rows,
        projects,
        job_history,
        interventions,
        tier3_status: tier3_status_rows(),
    }
}

// ───────────────────────── rendering ─────────────────────────

/// Deterministic pretty JSON: `stable_stringify` recursively sorts object
/// keys, so field-declaration order in the structs above cannot leak into
/// the byte output either — only the (already-fixed) array orderings from
/// `collect_posture` matter.
pub fn render_json(report: &PostureReport) -> String {
    let value = serde_json::to_value(report).unwrap_or(serde_json::Value::Null);
    stable_stringify(&value)
}

fn plural(n: usize) -> &'static str {
    if n == 1 {
        ""
    } else {
        "s"
    }
}

/// Deterministic Markdown, hand-rolled section by section from the SAME
/// already-ordered fields `render_json` walks — plain flat text (headers +
/// bullet lists, mirroring `verdict::render_health_text`'s own style) rather
/// than tables, so there is no column-escaping edge case to reason about.
pub fn render_markdown(report: &PostureReport) -> String {
    let mut out: Vec<String> = vec![
        "# ADE Posture Report".to_string(),
        String::new(),
        format!("Generated: {}", report.generated_at),
        format!("Verdict: {} — {}", report.verdict, report.headline),
    ];
    if !report.detail.is_empty() {
        out.push(report.detail.clone());
    }
    out.push(String::new());

    out.push("## Counts".to_string());
    out.push(format!(
        "- capability groups covered: {} of {}",
        report.counts.groups_covered, report.counts.groups_total
    ));
    out.push(format!(
        "- enabled capabilities: {} (ok {}, warnings {}, errors {}, missing {})",
        report.counts.enabled,
        report.counts.ok,
        report.counts.warnings,
        report.counts.errors,
        report.counts.missing
    ));
    out.push(String::new());

    out.push("## Capabilities".to_string());
    if report.capabilities.is_empty() {
        out.push("- (none detected)".to_string());
    }
    for cap in &report.capabilities {
        let status = if !cap.enabled {
            "disabled"
        } else if cap.installed && cap.healthy {
            "healthy"
        } else if cap.installed {
            "broken"
        } else {
            "not installed"
        };
        let version = cap.version.as_deref().unwrap_or("n/a");
        let update = if cap.update_available {
            format!(
                " — update available: {}",
                cap.latest_version.as_deref().unwrap_or("n/a")
            )
        } else {
            String::new()
        };
        out.push(format!(
            "- {} ({}) — {} — {} — version {version}{update}",
            cap.name, cap.group_name, cap.method, status
        ));
    }
    out.push(String::new());

    out.push("## Coverage".to_string());
    if report.coverage.is_empty() {
        out.push("- (no detection has run yet)".to_string());
    }
    for row in &report.coverage {
        out.push(format!(
            "- {}: {} — {}",
            row.group_name, row.state, row.summary
        ));
    }
    out.push(String::new());

    out.push("## Open Issues".to_string());
    if report.issues.is_empty() {
        out.push("- none".to_string());
    }
    for issue in &report.issues {
        let action = issue
            .action_label
            .as_deref()
            .unwrap_or("no automated action");
        let note = issue
            .note
            .as_deref()
            .map(|text| format!(" ({text})"))
            .unwrap_or_default();
        out.push(format!(
            "- [{}] {}{note} — {action}",
            issue.rank, issue.title
        ));
    }
    out.push(String::new());

    out.push("## Coverage Matrix".to_string());
    if report.projects.is_empty() {
        out.push("- no projects registered".to_string());
    }
    for project in &report.projects {
        out.push(format!("### {}", project.path));
        if let Some(error) = &project.error {
            out.push(format!("- error: {error}"));
            continue;
        }
        for cell in &project.matrix {
            out.push(format!("- {}: {}", cell.group_name, cell.state));
        }
    }
    out.push(String::new());

    out.push("## Job History".to_string());
    if report.job_history.is_empty() {
        out.push("- no ADE-run install/update jobs recorded".to_string());
    }
    for job in &report.job_history {
        out.push(format!(
            "- {}: {} {} {}, {} day{} ago",
            job.group_name,
            job.capability_name,
            job.action,
            job.outcome,
            job.days_ago,
            plural(job.days_ago as usize)
        ));
    }
    out.push(String::new());

    out.push("## Interventions".to_string());
    if report.interventions.is_empty() {
        out.push("- none recorded this run".to_string());
    }
    for item in &report.interventions {
        out.push(format!(
            "- {}: {} (source: {})",
            item.group_name, item.value, item.source
        ));
    }
    out.push(String::new());

    out.push("## Tier 3 — not yet available".to_string());
    for status in &report.tier3_status {
        out.push(format!(
            "- {} / {}: {} — {}",
            status.group_name, status.tool, status.status, status.note
        ));
    }
    out.push(String::new());

    out.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::run::{apply_pipeline, init_target};
    use crate::testutil::{fake_exec, fake_which, make_temp_dir};
    use std::fs;

    fn empty_deps(ade_home: PathBuf, now: u64) -> PostureDeps {
        PostureDeps {
            exec: fake_exec(&[]),
            which: fake_which(&[]),
            ade_home,
            user_home: None,
            now_seconds: Some(Arc::new(move || now)),
        }
    }

    // ── redact_home ──

    #[test]
    fn redact_home_replaces_every_occurrence_and_is_a_noop_without_a_match() {
        let home = Path::new("/Users/owner");
        assert_eq!(
            redact_home("/Users/owner/Code/proj", Some(home)),
            "~/Code/proj"
        );
        assert_eq!(
            redact_home(
                "ade.json not found in /Users/owner/Code/proj — run `ade init` first",
                Some(home)
            ),
            "ade.json not found in ~/Code/proj — run `ade init` first"
        );
        // Two occurrences in one string — both replaced.
        assert_eq!(
            redact_home("/Users/owner/a and /Users/owner/b", Some(home)),
            "~/a and ~/b"
        );
        // No match: unchanged.
        assert_eq!(
            redact_home("/opt/repos/proj", Some(home)),
            "/opt/repos/proj"
        );
        // No home at all: unchanged, never a guess.
        assert_eq!(redact_home("/Users/owner/proj", None), "/Users/owner/proj");
        // Empty home string: unchanged (never replaces every path with "~").
        assert_eq!(
            redact_home("/Users/owner/proj", Some(Path::new(""))),
            "/Users/owner/proj"
        );
    }

    // ── word mappings ──

    #[test]
    fn word_mappings_cover_every_real_variant() {
        assert_eq!(verdict_word(Verdict::Sound), "sound");
        assert_eq!(verdict_word(Verdict::NeedsAttention), "needs-attention");
        assert_eq!(verdict_word(Verdict::NotWorking), "not-working");
        assert_eq!(verdict_word(Verdict::Unknown), "unknown");

        assert_eq!(coverage_state_word(CoverageState::Covered), "covered");
        assert_eq!(coverage_state_word(CoverageState::Broken), "broken");
        assert_eq!(coverage_state_word(CoverageState::Uncovered), "uncovered");
        assert_eq!(coverage_state_word(CoverageState::Off), "off");

        assert_eq!(attention_rank_word(AttentionRank::Broken), "broken");
        assert_eq!(attention_rank_word(AttentionRank::Uncovered), "uncovered");
        assert_eq!(attention_rank_word(AttentionRank::Insight), "insight");
        assert_eq!(attention_rank_word(AttentionRank::Spare), "spare");
        assert_eq!(attention_rank_word(AttentionRank::Update), "update");
    }

    #[test]
    fn matrix_cell_word_matches_the_live_uis_classification() {
        let rows = vec![
            StatusRow {
                id: "secrets".to_string(),
                title: "t".to_string(),
                enabled: true,
                state: "applied".to_string(),
                findings: Vec::new(),
            },
            StatusRow {
                id: "sandbox".to_string(),
                title: "t".to_string(),
                enabled: true,
                state: "not-applied".to_string(),
                findings: Vec::new(),
            },
        ];
        assert_eq!(matrix_cell_word(&rows, "secret-scanning"), "wired");
        assert_eq!(matrix_cell_word(&rows, "sandboxing"), "not-wired");
        // No governing module at all — codebase-wiki has none.
        assert_eq!(matrix_cell_word(&rows, "codebase-wiki"), "not-applicable");
        // A governing module exists (dependency-scanning -> supply-chain) but
        // this fixture's rows do not carry it — defensive "error", never a
        // silent "not-wired" guess.
        assert_eq!(matrix_cell_word(&rows, "dependency-scanning"), "error");
    }

    #[test]
    fn tier3_status_names_exactly_trufflehog_and_gitleaks_under_secret_scanning() {
        let rows = tier3_status_rows();
        assert_eq!(rows.len(), 2);
        assert!(rows.iter().all(|row| row.group_id == "secret-scanning"));
        let trufflehog = rows.iter().find(|row| row.tool == "TruffleHog").unwrap();
        assert_eq!(trufflehog.status, "no-retained-output");
        assert!(trufflehog.note.contains("/dev/null"));
        let gitleaks = rows.iter().find(|row| row.tool == "Gitleaks").unwrap();
        assert_eq!(gitleaks.status, "not-invoked");
        assert!(gitleaks.note.contains("never runs it"));
    }

    // ── collection: absent data stays absent ──

    #[test]
    fn a_bare_machine_with_nothing_registered_fabricates_nothing() {
        let home = make_temp_dir("posture-bare");
        let report = collect_posture(&empty_deps(home.clone(), 2_000_000));
        // 18 enabled, 0 installed: every group is a warned gap (Uncovered),
        // never an Error — nothing has ever been INSTALLED-then-broken, so
        // this is NeedsAttention, not NotWorking (verdict.rs's own
        // distinction between "never arrived" and "installed but broken").
        assert_eq!(report.verdict, "needs-attention");
        assert!(report.projects.is_empty());
        assert!(report.job_history.is_empty());
        assert!(report.interventions.is_empty());
        assert_eq!(report.capabilities.len(), 18);
        assert!(report.capabilities.iter().all(|cap| !cap.installed));
        assert_eq!(report.tier3_status.len(), 2);
        let markdown = render_markdown(&report);
        assert!(markdown.contains("no projects registered"));
        assert!(markdown.contains("no ADE-run install/update jobs recorded"));
        assert!(markdown.contains("none recorded this run"));
        let _ = fs::remove_dir_all(&home);
    }

    // ── determinism ──

    #[test]
    fn collect_posture_is_byte_deterministic_for_identical_state() {
        let home = make_temp_dir("posture-det");
        let deps_a = empty_deps(home.clone(), 5_000_000);
        let deps_b = empty_deps(home.clone(), 5_000_000);
        let report_a = collect_posture(&deps_a);
        let report_b = collect_posture(&deps_b);
        assert_eq!(render_markdown(&report_a), render_markdown(&report_b));
        assert_eq!(render_json(&report_a), render_json(&report_b));

        // A THIRD run with only the clock changed must differ (in exactly the
        // timestamp line) — proving the equality above is a real assertion
        // about determinism, not a coincidence of two empty strings.
        let report_c = collect_posture(&empty_deps(home.clone(), 5_000_100));
        assert_ne!(render_json(&report_a), render_json(&report_c));
        let _ = fs::remove_dir_all(&home);
    }

    // ── home-directory redaction ──

    #[test]
    fn collect_posture_never_embeds_the_home_directory() {
        let ade_home = make_temp_dir("posture-redact-adehome");
        let fake_home = make_temp_dir("posture-redact-userhome");
        let project = fake_home.join("Code").join("example-repo");
        fs::create_dir_all(&project).expect("project dir");
        crate::gui::state::save_gui_state(
            &ade_home,
            &crate::gui::state::GuiState {
                projects: vec![project.to_string_lossy().to_string()],
                ..Default::default()
            },
        )
        .expect("seed gui state");

        let deps = PostureDeps {
            exec: fake_exec(&[]),
            which: fake_which(&[]),
            ade_home: ade_home.clone(),
            user_home: Some(fake_home.clone()),
            now_seconds: Some(Arc::new(|| 3_000_000)),
        };
        let report = collect_posture(&deps);
        // The project has no ade.json, so config_ok is false and every cell
        // is "error" — but the PATH itself must still be redacted.
        assert_eq!(report.projects.len(), 1);
        let home_str = fake_home.to_string_lossy().to_string();
        let markdown = render_markdown(&report);
        let json_text = render_json(&report);
        assert!(
            !markdown.contains(&home_str),
            "markdown embedded the real home directory: {markdown}"
        );
        assert!(
            !json_text.contains(&home_str),
            "json embedded the real home directory: {json_text}"
        );
        assert!(report.projects[0].path.starts_with('~'));
        assert!(markdown.contains("~/Code/example-repo"));

        let _ = fs::remove_dir_all(&ade_home);
        let _ = fs::remove_dir_all(&fake_home);
    }

    // ── a real registered project ──

    #[test]
    fn collect_posture_reflects_a_real_projects_module_status_via_the_matrix() {
        let ade_home = make_temp_dir("posture-real-adehome");
        let repo = make_temp_dir("posture-real-repo");
        fs::create_dir_all(repo.join(".git").join("hooks")).expect(".git/hooks");

        let pipeline = PipelineDeps {
            exec: fake_exec(&[("git -C", (0, "true\n", ""))]),
            which: fake_which(&[]),
            now: None,
        };
        init_target(&repo, &pipeline).expect("init");
        let config = load_config(&repo, &module_ids(), &harness_ids())
            .expect("load the config just written");
        let ctx = make_ctx(&repo, config, &pipeline);
        apply_pipeline(&ctx, &pipeline);

        crate::gui::state::save_gui_state(
            &ade_home,
            &crate::gui::state::GuiState {
                projects: vec![repo.to_string_lossy().to_string()],
                ..Default::default()
            },
        )
        .expect("seed gui state");

        let deps = PostureDeps {
            exec: fake_exec(&[("git -C", (0, "true\n", ""))]),
            which: fake_which(&[]),
            ade_home: ade_home.clone(),
            user_home: None,
            now_seconds: Some(Arc::new(|| 1_000_000)),
        };
        let report = collect_posture(&deps);
        assert_eq!(report.projects.len(), 1);
        let project = &report.projects[0];
        assert!(project.config_ok);
        assert!(project.error.is_none());
        assert_eq!(project.matrix.len(), CAPABILITY_GROUPS.len());
        // secret-scanning governs "secrets": `ade init` + apply on a
        // hermetic git fixture reaches `applied` (its verify only checks
        // ADE's OWN written artifacts — the policy file and the installed
        // hook shim — never re-checks trufflehog's presence), matching
        // `report.rs`'s own proof that a hermetic apply with zero real
        // tools present still lands every module on applied/degraded, never
        // not-applied. `module_state_covers("applied")` is true, so the
        // matrix cell is "wired" — the same fact `ade status` would print.
        let secrets_cell = project
            .matrix
            .iter()
            .find(|cell| cell.group_id == "secret-scanning")
            .unwrap();
        assert_eq!(secrets_cell.state, "wired");
        // A group with no governing module at all.
        let wiki_cell = project
            .matrix
            .iter()
            .find(|cell| cell.group_id == "codebase-wiki")
            .unwrap();
        assert_eq!(wiki_cell.state, "not-applicable");

        let _ = fs::remove_dir_all(&ade_home);
        let _ = fs::remove_dir_all(&repo);
    }

    #[test]
    fn an_unbootstrapped_registered_project_reports_a_redacted_error_not_a_crash() {
        let ade_home = make_temp_dir("posture-badproj-adehome");
        let empty_repo = make_temp_dir("posture-badproj-repo");
        crate::gui::state::save_gui_state(
            &ade_home,
            &crate::gui::state::GuiState {
                projects: vec![empty_repo.to_string_lossy().to_string()],
                ..Default::default()
            },
        )
        .expect("seed gui state");

        let report = collect_posture(&empty_deps(ade_home.clone(), 1_000_000));
        assert_eq!(report.projects.len(), 1);
        let project = &report.projects[0];
        assert!(!project.config_ok);
        let error = project.error.as_deref().expect("an error is recorded");
        assert!(error.contains("ade.json"));
        assert!(project.matrix.iter().all(|cell| cell.state == "error"));

        let _ = fs::remove_dir_all(&ade_home);
        let _ = fs::remove_dir_all(&empty_repo);
    }

    // ── job history (Tier 1) ──

    #[test]
    fn job_history_names_the_real_last_job_per_group() {
        let ade_home = make_temp_dir("posture-jobs");
        let runner = JobRunner::new(
            fake_exec(&[("real-install", (0, "ok\n", ""))]),
            Some(ade_home.clone()),
        );
        let outcome = runner.start(
            "trufflehog",
            crate::gui::inventory::LifecycleAction::Install,
            vec![vec!["real-install".to_string()]],
        );
        assert!(matches!(
            outcome,
            crate::gui::jobs::StartOutcome::Started(_)
        ));
        runner.settle();

        let report = collect_posture(&empty_deps(ade_home.clone(), 9_999_999_999));
        let entry = report
            .job_history
            .iter()
            .find(|row| row.group_id == "secret-scanning")
            .expect("a real finished job exists for secret-scanning");
        assert_eq!(entry.capability_name, "TruffleHog");
        assert_eq!(entry.action, "install");
        assert_eq!(entry.outcome, "ok");
        assert!(render_markdown(&report).contains("TruffleHog install ok"));

        let _ = fs::remove_dir_all(&ade_home);
    }

    // ── interventions (Tier 2 — rtk gain) ──

    #[test]
    fn token_efficiency_intervention_appears_only_with_real_nonzero_gain() {
        let home = make_temp_dir("posture-rtk-real");
        let exec = fake_exec(&[
            ("rtk --version", (0, "rtk 1.0.0\n", "")),
            (
                "rtk gain --format json",
                (
                    0,
                    r#"{"summary":{"total_commands":12129,"total_input":1,"total_output":1,"total_saved":310350059,"avg_savings_pct":81.43,"total_time_ms":1,"avg_time_ms":1}}"#,
                    "",
                ),
            ),
        ]);
        let deps = PostureDeps {
            exec,
            which: fake_which(&["rtk"]),
            ade_home: home.clone(),
            user_home: None,
            now_seconds: Some(Arc::new(|| 1_000_000)),
        };
        let report = collect_posture(&deps);
        let intervention = report
            .interventions
            .iter()
            .find(|item| item.group_id == "token-efficiency")
            .expect("a real, nonzero rtk gain produces an intervention line");
        assert!(intervention.value.contains("310350059 tokens saved"));
        assert_eq!(intervention.source, "rtk gain --format json");
        assert!(render_markdown(&report).contains("310350059 tokens saved"));

        let _ = fs::remove_dir_all(&home);
    }

    #[test]
    fn token_efficiency_intervention_is_absent_when_rtk_is_not_installed() {
        // rtk is not present at all: fake_which(&[]) resolves nothing, so the
        // capability itself is `installed == false` and the probe never runs.
        let home = make_temp_dir("posture-rtk-absent");
        let report = collect_posture(&empty_deps(home.clone(), 1_000_000));
        assert!(report
            .interventions
            .iter()
            .all(|item| item.group_id != "token-efficiency"));
        let _ = fs::remove_dir_all(&home);
    }

    // ── PostureCounts::from ──

    #[test]
    fn posture_counts_carries_every_field_from_health_counts() {
        let counts = HealthCounts {
            groups_total: 10,
            groups_covered: 7,
            enabled: 18,
            ok: 14,
            warnings: 1,
            errors: 1,
            missing: 3,
        };
        let posture: PostureCounts = counts.into();
        assert_eq!(posture.groups_total, 10);
        assert_eq!(posture.groups_covered, 7);
        assert_eq!(posture.enabled, 18);
        assert_eq!(posture.ok, 14);
        assert_eq!(posture.warnings, 1);
        assert_eq!(posture.errors, 1);
        assert_eq!(posture.missing, 3);
    }

    // ── markdown renders every section header regardless of content ──

    #[test]
    fn render_markdown_always_emits_every_section_header() {
        let report = collect_posture(&empty_deps(make_temp_dir("posture-headers"), 1_000_000));
        let markdown = render_markdown(&report);
        for header in [
            "# ADE Posture Report",
            "## Counts",
            "## Capabilities",
            "## Coverage",
            "## Open Issues",
            "## Coverage Matrix",
            "## Job History",
            "## Interventions",
            "## Tier 3 — not yet available",
        ] {
            assert!(markdown.contains(header), "missing header: {header}");
        }
    }
}
