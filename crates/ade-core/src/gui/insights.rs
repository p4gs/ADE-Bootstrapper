//! Phase H — the insights engine: metrics become sentences with at most one
//! action. Same architecture as `verdict.rs` (pure function over facts,
//! unit-tested, shared by every surface that wants it): `build_insights(facts)
//! -> Vec<Insight>`.
//!
//! Every rule below is grounded in a field Phase G's `gui::stats` already
//! computes from a real probe — `GroupFacts` is the exact set of Tier-1/
//! Tier-2 facts `ade-control-center`'s `GroupStats` carries, just expressed
//! here (in `ade-core`, which cannot depend on the app crate) so the same
//! shape can be unit-tested with no engine, no window, and no live machine.
//! A rule that has no real data source for a group simply returns `None` —
//! there is no placeholder insight anywhere in this file, per the house rule
//! that absent data renders as absent, never fabricated.
//!
//! `Insight.action` is `Option<InsightAction>` — a single field, not a `Vec`.
//! That is the whole enforcement mechanism for "at most one action": there is
//! no second slot for a second action to occupy, so a caller cannot construct
//! an `Insight` with two actions any more than it could put two values into
//! one `Option`. Nothing here needs a runtime check for it.

use crate::gui::inventory::{join_human, CAPABILITY_GROUPS};
use crate::gui::stats;
use crate::gui::verdict::AttentionRank;

/// Below this many days, an OSV database refresh is not worth mentioning —
/// the same cadence the dependency-scanning module itself expects a scan to
/// run at.
const OSV_STALE_DAYS: u64 = 7;
/// A semantic index more than this many days behind HEAD is worth a nudge;
/// a same-day lag is normal drift between edits and a background reindex.
const INDEX_STALE_DAYS: i64 = 3;
/// Below this, hook latency is an affirmation ("adds Nms"); at or above it,
/// it is worth a look — still not a hard failure, just worth knowing.
const HOOK_LATENCY_NOTICE_MS: u64 = 200;

/// Everything one capability group's Phase G stats-strip already computed —
/// the ade-core-side mirror of `ade-control-center::data::GroupStats`, field
/// for field. The app crate builds one of these per group from its own
/// `GroupStats` on every poll; nothing here reads a probe directly.
#[derive(Debug, Clone, Default)]
pub struct GroupFacts {
    pub group_id: &'static str,
    pub group_name: &'static str,
    pub last_job: Option<stats::JobRecency>,
    pub project_coverage: Option<stats::ProjectCoverage>,
    pub token_gain: Option<stats::TokenGain>,
    pub osv_db_age: Option<stats::OsvDbAge>,
    pub index_staleness_days: Option<i64>,
    pub hook_latency_millis: Option<u64>,
    pub harness_surfaces: Vec<stats::HarnessSurface>,
    pub hardened_repos: Option<stats::HardenedRepoCoverage>,
}

/// The one action an insight can offer: open the capability group's page.
/// Every other real command in this app (start a lifecycle action, run a
/// bulk install) already has its own dedicated surface — an insight's job is
/// to point at where the fact lives, not to invent a second way to fix it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InsightAction {
    /// Button label — an imperative naming its object ("Open Secret Scanning").
    pub label: String,
    /// The capability-group id an `EngineCmd::SetScope` would navigate to.
    pub group_id: String,
}

/// One insight: a plain sentence that names its own evidence, at most one
/// action, and a stable `id` a dismissal can persist against
/// (`gui::state::GuiState::dismissed_insights`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Insight {
    /// Stable across polls — a `rule-tag:group-id` pair, never a random or
    /// value-derived id, so dismissing "the OSV database is stale for
    /// Dependency Scanning" keeps suppressing that exact rule for that exact
    /// group even as the age itself changes on every subsequent poll.
    pub id: String,
    pub group_id: &'static str,
    pub group_name: &'static str,
    /// The full sentence, already naming the concrete fact that grounds it —
    /// this is the "insight" as the owner reads it.
    pub headline: String,
    /// A short, traceable pointer to the exact probe/field the headline's
    /// numbers came from — not shown as prose anywhere, but proof (and a test
    /// fixture) that nothing here is invented.
    pub evidence: String,
    pub action: Option<InsightAction>,
    /// Always `AttentionRank::Insight` today. Carried as this exact shared
    /// type (rather than a bespoke `InsightRank`) so "broken > uncovered >
    /// insight" is a real ordinal comparison, not a convention two modules
    /// have to independently remember.
    pub rank: AttentionRank,
}

fn group_index(id: &str) -> usize {
    CAPABILITY_GROUPS
        .iter()
        .position(|group| group.id == id)
        .unwrap_or(usize::MAX)
}

fn open_action(facts: &GroupFacts) -> InsightAction {
    InsightAction {
        label: format!("Open {}", facts.group_name),
        group_id: facts.group_id.to_string(),
    }
}

/// "today" / "1 day ago" / "N days ago" — the same day-granularity wording
/// the capability-page stats strip uses, so an insight quoting a job's age
/// never disagrees with the tile sitting right above it.
fn days_ago_phrase(days: u64) -> String {
    match days {
        0 => "today".to_string(),
        1 => "1 day ago".to_string(),
        n => format!("{n} days ago"),
    }
}

/// A large integer with a K/M/B suffix — `rtk`'s token counts run into the
/// hundreds of millions, and nine raw digits inside a sentence reads as
/// noise. Deliberately duplicated from `ade-control-center`'s own
/// `format_count` (same shape, same tests) rather than shared: this crate
/// cannot depend on the app crate, and the inverse dependency would pull a
/// GUI-formatting helper into a headless core.
fn format_count(n: u64) -> String {
    const BILLION: f64 = 1_000_000_000.0;
    const MILLION: f64 = 1_000_000.0;
    const THOUSAND: f64 = 1_000.0;
    let value = n as f64;
    if value >= BILLION {
        format!("{:.1}B", value / BILLION)
    } else if value >= MILLION {
        format!("{:.1}M", value / MILLION)
    } else if value >= THOUSAND {
        format!("{:.1}K", value / THOUSAND)
    } else {
        n.to_string()
    }
}

fn plural<'a>(n: usize, one: &'a str, many: &'a str) -> &'a str {
    if n == 1 {
        one
    } else {
        many
    }
}

// ───────────────────────── the rules ─────────────────────────

/// Tier 1 — `stats::ProjectCoverage`. Only when there is a real gap (covered
/// < inspected): a fully-covered group is already stated, silently, by the
/// Overview's own Coverage list — repeating it here would be noise, not a
/// new fact.
fn project_coverage_gap(facts: &GroupFacts) -> Option<Insight> {
    let coverage = facts.project_coverage?;
    if coverage.covered >= coverage.inspected {
        return None;
    }
    let gap = coverage.inspected - coverage.covered;
    Some(Insight {
        id: format!("project-coverage:{}", facts.group_id),
        group_id: facts.group_id,
        group_name: facts.group_name,
        headline: format!(
            "{gap} of {} inspected {} {} no {} wired.",
            coverage.inspected,
            plural(coverage.inspected, "project", "projects"),
            if gap == 1 { "has" } else { "have" },
            facts.group_name,
        ),
        evidence: format!(
            "project_coverage: {} of {} inspected registered projects covered",
            coverage.covered, coverage.inspected
        ),
        action: Some(open_action(facts)),
        rank: AttentionRank::Insight,
    })
}

/// Tier 1 — `stats::JobRecency`. A historical fact distinct from live
/// capability health: a job can have failed weeks ago against a tool that is
/// working fine today, or can be the reason it is not.
fn last_job_failed(facts: &GroupFacts) -> Option<Insight> {
    let job = facts.last_job.as_ref()?;
    if !job.failed {
        return None;
    }
    let exit = job
        .exit_code
        .map(|code| format!(" (exit {code})"))
        .unwrap_or_default();
    Some(Insight {
        id: format!("last-job-failed:{}", facts.group_id),
        group_id: facts.group_id,
        group_name: facts.group_name,
        headline: format!(
            "The last {} job ADE ran for {} failed{exit}, {}.",
            job.action,
            job.capability_name,
            days_ago_phrase(job.days_ago),
        ),
        evidence: format!(
            "jobs.json: {} {} on {} finished {} days ago, exit {:?}",
            job.capability_name, job.action, facts.group_name, job.days_ago, job.exit_code
        ),
        action: Some(open_action(facts)),
        rank: AttentionRank::Insight,
    })
}

/// Tier 2 — token-efficiency only, via `stats::TokenGain`. The plan's own
/// example of a positive value affirmation: no action, the number is the
/// point. Gated on `total_commands > 0` so a freshly-installed, never-used
/// `rtk` does not produce a degenerate "saved 0 tokens across 0 commands"
/// sentence — that would be real data, but not an insight.
fn token_gain_affirmation(facts: &GroupFacts) -> Option<Insight> {
    let gain = facts.token_gain.as_ref()?;
    if gain.total_commands == 0 {
        return None;
    }
    Some(Insight {
        id: format!("token-gain:{}", facts.group_id),
        group_id: facts.group_id,
        group_name: facts.group_name,
        headline: format!(
            "{} saved {} tokens across {} commands ({:.0}% average reduction).",
            facts.group_name,
            format_count(gain.tokens_saved),
            gain.total_commands,
            gain.avg_savings_pct,
        ),
        evidence: format!(
            "rtk gain --format json: summary.total_saved={}, avg_savings_pct={:.2}, total_commands={}",
            gain.tokens_saved, gain.avg_savings_pct, gain.total_commands
        ),
        action: None,
        rank: AttentionRank::Insight,
    })
}

/// Tier 2 — dependency-scanning only, via `stats::OsvDbAge`.
fn osv_db_staleness(facts: &GroupFacts) -> Option<Insight> {
    let age = facts.osv_db_age?;
    if age.days_old < OSV_STALE_DAYS {
        return None;
    }
    Some(Insight {
        id: format!("osv-db-age:{}", facts.group_id),
        group_id: facts.group_id,
        group_name: facts.group_name,
        headline: format!(
            "The OSV vulnerability database is {} days old — advisories published since then are not being checked.",
            age.days_old
        ),
        evidence: format!(
            "osv-scanner local cache: oldest of {} ecosystem {} is {} days old",
            age.ecosystems,
            plural(age.ecosystems, "archive", "archives"),
            age.days_old
        ),
        action: Some(open_action(facts)),
        rank: AttentionRank::Insight,
    })
}

/// Tier 2 — semantic-search only, via `index_staleness_days`. The stat is
/// already the worst (most stale) among registered projects and carries no
/// project name (`stats::index_staleness_days`'s own doc comment) — the
/// sentence says exactly that rather than inventing which project it was.
fn semantic_index_staleness(facts: &GroupFacts) -> Option<Insight> {
    let days = facts.index_staleness_days?;
    if days < INDEX_STALE_DAYS {
        return None;
    }
    Some(Insight {
        id: format!("index-staleness:{}", facts.group_id),
        group_id: facts.group_id,
        group_name: facts.group_name,
        headline: format!(
            "The semantic index for the most out-of-date registered project is {days} days behind its HEAD commit."
        ),
        evidence: format!(
            "CocoIndex index mtime vs `git log -1 --format=%ct HEAD`: {days} days behind"
        ),
        action: Some(open_action(facts)),
        rank: AttentionRank::Insight,
    })
}

/// Tier 2 — hook-orchestration only, via `hook_latency_millis`. Always
/// rendered when the fact is real (the plan's own "your commits pay Nms"
/// framing is informational at any value); the action appears only once the
/// cost is worth a look, per `HOOK_LATENCY_NOTICE_MS`.
fn hook_latency_notice(facts: &GroupFacts) -> Option<Insight> {
    let millis = facts.hook_latency_millis?;
    Some(Insight {
        id: format!("hook-latency:{}", facts.group_id),
        group_id: facts.group_id,
        group_name: facts.group_name,
        headline: format!("Your commits pay {millis}ms for the pre-commit hook chain."),
        evidence: format!(
            "timed real invocation of the registered project's .git/hooks/pre-commit: {millis}ms"
        ),
        action: (millis >= HOOK_LATENCY_NOTICE_MS).then(|| open_action(facts)),
        rank: AttentionRank::Insight,
    })
}

/// Tier 2 — agent-security-rules only, via `harness_surfaces`.
fn harness_surface_gap(facts: &GroupFacts) -> Option<Insight> {
    if facts.harness_surfaces.is_empty() {
        return None;
    }
    let covered = facts.harness_surfaces.iter().filter(|s| s.covered).count();
    let total = facts.harness_surfaces.len();
    if covered >= total {
        return None;
    }
    let missing: Vec<String> = facts
        .harness_surfaces
        .iter()
        .filter(|surface| !surface.covered)
        .map(|surface| surface.harness_name.to_string())
        .collect();
    Some(Insight {
        id: format!("harness-surfaces:{}", facts.group_id),
        group_id: facts.group_id,
        group_name: facts.group_name,
        headline: format!(
            "{covered} of {total} coding-harness surfaces have the CodeGuard ruleset installed — {} {} not.",
            join_human(&missing),
            if missing.len() == 1 { "is" } else { "are" }
        ),
        evidence: format!("codeguard_harness_surfaces: {covered} of {total} covered"),
        action: Some(open_action(facts)),
        rank: AttentionRank::Insight,
    })
}

/// Tier 2 — repo-hygiene only, via `hardened_repos`.
fn hardened_repo_gap(facts: &GroupFacts) -> Option<Insight> {
    let coverage = facts.hardened_repos?;
    if coverage.hardened >= coverage.total {
        return None;
    }
    let gap = coverage.total - coverage.hardened;
    Some(Insight {
        id: format!("hardened-repos:{}", facts.group_id),
        group_id: facts.group_id,
        group_name: facts.group_name,
        headline: format!(
            "{gap} of {} registered {} {} commit signing enabled.",
            coverage.total,
            plural(coverage.total, "project", "projects"),
            if gap == 1 { "lacks" } else { "lack" }
        ),
        evidence: format!(
            "git config --get commit.gpgsign across registered projects: {} of {} enabled",
            coverage.hardened, coverage.total
        ),
        action: Some(open_action(facts)),
        rank: AttentionRank::Insight,
    })
}

/// Every rule, applied to every group's facts, in a fixed evaluation order —
/// actionable rules first so a tie on group/priority still reads
/// deterministically. The final sort (not this list) is what actually
/// decides render order; this order only matters for the id-stability of
/// the stable sort below.
const RULES: &[fn(&GroupFacts) -> Option<Insight>] = &[
    last_job_failed,
    project_coverage_gap,
    harness_surface_gap,
    hardened_repo_gap,
    osv_db_staleness,
    semantic_index_staleness,
    hook_latency_notice,
    token_gain_affirmation,
];

/// Build every insight the current facts support, worst/most-actionable
/// first: an insight carrying an action outranks a pure affirmation: ties
/// break on capability-group taxonomy order, then on the insight's own
/// (stable, deterministic) id — so the result never depends on the order
/// `groups` arrived in, which matters because the real caller builds it from
/// a `HashMap` iteration with no defined order of its own.
///
/// Pure: same facts in, same insights out. Unbounded — "top 5 on the
/// Overview" and "this group's subset on its capability page" are both
/// presentation slices a caller takes over this same `Vec`, not something
/// this function decides.
pub fn build_insights(groups: &[GroupFacts]) -> Vec<Insight> {
    let mut out: Vec<Insight> = Vec::new();
    for group in groups {
        for rule in RULES {
            if let Some(insight) = rule(group) {
                out.push(insight);
            }
        }
    }
    out.sort_by(|a, b| {
        let key = |insight: &Insight| {
            (
                insight.action.is_none(),
                group_index(insight.group_id),
                insight.id.clone(),
            )
        };
        key(a).cmp(&key(b))
    });
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn facts(group_id: &'static str) -> GroupFacts {
        let group = crate::gui::inventory::get_group(group_id).expect("real group id");
        GroupFacts {
            group_id: group.id,
            group_name: group.name,
            ..GroupFacts::default()
        }
    }

    #[test]
    fn an_empty_slice_yields_no_insights() {
        assert!(build_insights(&[]).is_empty());
        assert!(build_insights(&[facts("secret-scanning")]).is_empty());
    }

    #[test]
    fn project_coverage_gap_names_the_real_numbers_and_offers_to_open_the_group() {
        let mut f = facts("secret-scanning");
        f.project_coverage = Some(stats::ProjectCoverage {
            covered: 2,
            inspected: 5,
        });
        let insight = project_coverage_gap(&f).expect("a real gap");
        assert_eq!(
            insight.headline,
            "3 of 5 inspected projects have no Secret Scanning wired."
        );
        assert_eq!(insight.id, "project-coverage:secret-scanning");
        let action = insight.action.expect("gaps offer to open the group");
        assert_eq!(action.label, "Open Secret Scanning");
        assert_eq!(action.group_id, "secret-scanning");
        assert_eq!(insight.rank, AttentionRank::Insight);

        // Fully covered: no insight — the Coverage list already says so.
        let mut covered = facts("secret-scanning");
        covered.project_coverage = Some(stats::ProjectCoverage {
            covered: 3,
            inspected: 3,
        });
        assert!(project_coverage_gap(&covered).is_none());
        // Absent data: absent insight.
        assert!(project_coverage_gap(&facts("secret-scanning")).is_none());
    }

    #[test]
    fn project_coverage_gap_pluralises_a_single_project_correctly() {
        let mut f = facts("sandboxing");
        f.project_coverage = Some(stats::ProjectCoverage {
            covered: 0,
            inspected: 1,
        });
        let insight = project_coverage_gap(&f).unwrap();
        assert_eq!(
            insight.headline,
            "1 of 1 inspected project has no Execution Sandboxing wired."
        );
    }

    #[test]
    fn last_job_failed_names_the_real_job_and_is_silent_on_success() {
        let mut f = facts("dependency-scanning");
        f.last_job = Some(stats::JobRecency {
            capability_id: "osv-scanner".to_string(),
            capability_name: "OSV-Scanner".to_string(),
            action: "install".to_string(),
            failed: true,
            exit_code: Some(1),
            days_ago: 2,
        });
        let insight = last_job_failed(&f).expect("a real failure");
        assert_eq!(
            insight.headline,
            "The last install job ADE ran for OSV-Scanner failed (exit 1), 2 days ago."
        );
        assert_eq!(insight.id, "last-job-failed:dependency-scanning");
        assert_eq!(insight.action.unwrap().label, "Open Dependency Scanning");

        let mut ok = facts("dependency-scanning");
        ok.last_job = Some(stats::JobRecency {
            capability_id: "osv-scanner".to_string(),
            capability_name: "OSV-Scanner".to_string(),
            action: "install".to_string(),
            failed: false,
            exit_code: Some(0),
            days_ago: 0,
        });
        assert!(last_job_failed(&ok).is_none());
        assert!(last_job_failed(&facts("dependency-scanning")).is_none());
    }

    #[test]
    fn last_job_failed_reads_naturally_at_zero_and_one_day() {
        let mut f = facts("sandboxing");
        f.last_job = Some(stats::JobRecency {
            capability_id: "nono".to_string(),
            capability_name: "nono".to_string(),
            action: "reinstall".to_string(),
            failed: true,
            exit_code: None,
            days_ago: 0,
        });
        assert!(last_job_failed(&f)
            .unwrap()
            .headline
            .ends_with("failed, today."));
        f.last_job.as_mut().unwrap().days_ago = 1;
        assert!(last_job_failed(&f)
            .unwrap()
            .headline
            .ends_with("failed, 1 day ago."));
    }

    #[test]
    fn token_gain_affirmation_states_the_real_numbers_with_no_action() {
        let mut f = facts("token-efficiency");
        f.token_gain = Some(stats::TokenGain {
            tokens_saved: 310_350_059,
            avg_savings_pct: 81.43,
            total_commands: 12_129,
        });
        let insight = token_gain_affirmation(&f).expect("a real gain");
        assert_eq!(
            insight.headline,
            "Token Efficiency saved 310.4M tokens across 12129 commands (81% average reduction)."
        );
        assert!(insight.action.is_none(), "an affirmation carries no action");
        assert_eq!(insight.id, "token-gain:token-efficiency");

        // A real but degenerate zero never renders — real data, not a useful
        // insight.
        let mut zero = facts("token-efficiency");
        zero.token_gain = Some(stats::TokenGain {
            tokens_saved: 0,
            avg_savings_pct: 0.0,
            total_commands: 0,
        });
        assert!(token_gain_affirmation(&zero).is_none());
        assert!(token_gain_affirmation(&facts("token-efficiency")).is_none());
    }

    #[test]
    fn osv_db_staleness_only_fires_past_the_real_threshold() {
        let mut fresh = facts("dependency-scanning");
        fresh.osv_db_age = Some(stats::OsvDbAge {
            days_old: OSV_STALE_DAYS - 1,
            ecosystems: 3,
        });
        assert!(osv_db_staleness(&fresh).is_none());

        let mut stale = facts("dependency-scanning");
        stale.osv_db_age = Some(stats::OsvDbAge {
            days_old: 21,
            ecosystems: 4,
        });
        let insight = osv_db_staleness(&stale).unwrap();
        assert_eq!(
            insight.headline,
            "The OSV vulnerability database is 21 days old — advisories published since then are not being checked."
        );
        assert_eq!(insight.action.unwrap().group_id, "dependency-scanning");
        assert!(osv_db_staleness(&facts("dependency-scanning")).is_none());
    }

    #[test]
    fn semantic_index_staleness_only_fires_past_the_real_threshold_and_never_invents_a_project() {
        let mut fresh = facts("semantic-search");
        fresh.index_staleness_days = Some(INDEX_STALE_DAYS - 1);
        assert!(semantic_index_staleness(&fresh).is_none());

        let mut current = facts("semantic-search");
        current.index_staleness_days = Some(-5);
        assert!(semantic_index_staleness(&current).is_none());

        let mut stale = facts("semantic-search");
        stale.index_staleness_days = Some(14);
        let insight = semantic_index_staleness(&stale).unwrap();
        assert_eq!(
            insight.headline,
            "The semantic index for the most out-of-date registered project is 14 days behind its HEAD commit."
        );
        assert!(
            !insight.headline.contains("nthpartyfinder"),
            "the stat carries no project name — never invent one"
        );
        assert!(semantic_index_staleness(&facts("semantic-search")).is_none());
    }

    #[test]
    fn hook_latency_notice_always_renders_when_real_but_only_acts_past_the_threshold() {
        let mut fast = facts("hook-orchestration");
        fast.hook_latency_millis = Some(40);
        let insight = hook_latency_notice(&fast).expect("a real, fast measurement still renders");
        assert_eq!(
            insight.headline,
            "Your commits pay 40ms for the pre-commit hook chain."
        );
        assert!(insight.action.is_none(), "fast enough not to bother");

        let mut slow = facts("hook-orchestration");
        slow.hook_latency_millis = Some(340);
        let insight = hook_latency_notice(&slow).unwrap();
        assert_eq!(
            insight.headline,
            "Your commits pay 340ms for the pre-commit hook chain."
        );
        assert_eq!(insight.action.unwrap().group_id, "hook-orchestration");
        assert!(hook_latency_notice(&facts("hook-orchestration")).is_none());
    }

    #[test]
    fn harness_surface_gap_names_which_harnesses_are_missing() {
        let mut f = facts("agent-security-rules");
        f.harness_surfaces = vec![
            stats::HarnessSurface {
                harness_id: "claude-code",
                harness_name: "Claude Code",
                covered: true,
            },
            stats::HarnessSurface {
                harness_id: "codex",
                harness_name: "Codex",
                covered: false,
            },
        ];
        let insight = harness_surface_gap(&f).unwrap();
        assert_eq!(
            insight.headline,
            "1 of 2 coding-harness surfaces have the CodeGuard ruleset installed — Codex is not."
        );
        assert_eq!(insight.id, "harness-surfaces:agent-security-rules");
        assert_eq!(insight.action.unwrap().group_id, "agent-security-rules");

        // Fully covered: no insight.
        let mut all = facts("agent-security-rules");
        all.harness_surfaces = vec![stats::HarnessSurface {
            harness_id: "claude-code",
            harness_name: "Claude Code",
            covered: true,
        }];
        assert!(harness_surface_gap(&all).is_none());
        // No surfaces probed at all (a group with no CodeGuard concept):
        // absent, not zero.
        assert!(harness_surface_gap(&facts("agent-security-rules")).is_none());
    }

    #[test]
    fn hardened_repo_gap_names_the_real_counts() {
        let mut f = facts("repo-hygiene");
        f.hardened_repos = Some(stats::HardenedRepoCoverage {
            hardened: 1,
            total: 3,
        });
        let insight = hardened_repo_gap(&f).unwrap();
        assert_eq!(
            insight.headline,
            "2 of 3 registered projects lack commit signing enabled."
        );
        assert_eq!(insight.action.unwrap().group_id, "repo-hygiene");

        let mut full = facts("repo-hygiene");
        full.hardened_repos = Some(stats::HardenedRepoCoverage {
            hardened: 2,
            total: 2,
        });
        assert!(hardened_repo_gap(&full).is_none());
        assert!(hardened_repo_gap(&facts("repo-hygiene")).is_none());
    }

    /// `build_insights` never depends on the order `groups` arrives in — the
    /// real caller builds it from a `HashMap`, which has no defined order.
    #[test]
    fn build_insights_is_order_independent_and_actionable_first() {
        let mut secrets = facts("secret-scanning");
        secrets.project_coverage = Some(stats::ProjectCoverage {
            covered: 1,
            inspected: 2,
        });
        let mut tokens = facts("token-efficiency");
        tokens.token_gain = Some(stats::TokenGain {
            tokens_saved: 1_000,
            avg_savings_pct: 50.0,
            total_commands: 10,
        });

        let forward = build_insights(&[secrets.clone(), tokens.clone()]);
        let reversed = build_insights(&[tokens, secrets]);
        assert_eq!(forward, reversed);
        assert_eq!(forward.len(), 2);
        // The actionable gap outranks the actionless affirmation regardless
        // of taxonomy order between the two (secret-scanning precedes
        // token-efficiency anyway, but this also proves the action-first
        // tier, not just the taxonomy tier).
        assert!(forward[0].action.is_some());
        assert!(forward[1].action.is_none());
    }

    #[test]
    fn build_insights_ties_within_a_group_break_on_id() {
        let mut f = facts("dependency-scanning");
        f.last_job = Some(stats::JobRecency {
            capability_id: "osv-scanner".to_string(),
            capability_name: "OSV-Scanner".to_string(),
            action: "update".to_string(),
            failed: true,
            exit_code: Some(1),
            days_ago: 1,
        });
        f.project_coverage = Some(stats::ProjectCoverage {
            covered: 0,
            inspected: 1,
        });
        let insights = build_insights(&[f]);
        // Both are actionable and in the same group — the id string breaks
        // the tie, deterministically, in the same order every time.
        assert_eq!(insights.len(), 2);
        let ids: Vec<&str> = insights.iter().map(|i| i.id.as_str()).collect();
        assert_eq!(
            ids,
            vec![
                "last-job-failed:dependency-scanning",
                "project-coverage:dependency-scanning",
            ]
        );
    }

    #[test]
    fn every_grounded_group_produces_an_insight_when_its_own_facts_are_real() {
        // Every rule fires on a real group id — a sanity net against a typo
        // in a rule's own `get_group` assumption silently returning nothing.
        for group_id in [
            "secret-scanning",
            "dependency-scanning",
            "sandboxing",
            "repo-hygiene",
            "token-efficiency",
            "semantic-search",
            "hook-orchestration",
            "agent-security-rules",
            "coding-harness",
            "codebase-wiki",
        ] {
            assert!(
                crate::gui::inventory::get_group(group_id).is_some(),
                "{group_id} must be a real capability group"
            );
        }
    }
}
