//! Health verdict — the single answer to "is my agent environment sound, and
//! what should I do about it?" (ISC-222…).
//!
//! This module exists because that question was previously answered twice: the
//! tray computed one rollup and the Control Center header computed another, and
//! they disagreed twice in one week. Both surfaces now derive from
//! `build_verdict`, so a disagreement is a compile-time impossibility rather
//! than a bug waiting to be found.
//!
//! The severity model is coverage-aware. "Gitleaks is not installed" while
//! TruffleHog is already scanning is a spare tyre; "nono is not installed" with
//! nothing else sandboxing is a hole in the floor. Ranking them identically —
//! which is what a flat per-capability warning does — makes the list unusable
//! precisely when it matters.

use crate::gui::inventory::{
    get_group, join_human, CapabilityStatus, GroupCoverage, LifecycleAction, CAPABILITIES,
    CAPABILITY_GROUPS,
};
use crate::types::FindingLevel;

/// The one-glance answer. Drives the Overview icon and the tray glyph.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    /// Every capability the owner left enabled has a working provider.
    Sound,
    /// Nothing is broken, but at least one capability has no provider.
    NeedsAttention,
    /// Something is wrong: a tool that is installed but does not work, or an
    /// action the owner ran that failed. Either way the environment is not in
    /// the state it reports.
    NotWorking,
    /// Detection has not run yet (first paint).
    Unknown,
}

/// Why an item is in the attention list. The discriminant IS the sort order:
/// a broken tool outranks a hole, a hole outranks a spare, a spare outranks an
/// update. Ties break on taxonomy order so a 4-second poll never reshuffles.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum AttentionRank {
    /// Installed but not working. First, because it is the only class where
    /// the environment reports a guardrail that is not actually guarding.
    Broken,
    /// A capability with no working provider — a genuine gap in coverage.
    Uncovered,
    /// Phase H (`gui::insights`): a metrics-derived observation — a stale
    /// database, an adoption gap, a value affirmation. Every `insights::Insight`
    /// carries this exact variant as its own `rank`, so the ordinal below is a
    /// real, load-bearing fact (`ordinal(Insight) > ordinal(Uncovered)`, held
    /// by a test in `insights.rs`), not decoration: it is what lets the
    /// Overview's "broken > uncovered > insight" placement be stated as a
    /// comparison over one shared type instead of two independently-agreed
    /// conventions that could drift apart the way the tray and the header
    /// once did.
    Insight,
    /// A provider is missing but its capability is already covered.
    Spare,
    /// A working provider has a newer version available.
    Update,
}

impl AttentionRank {
    /// `pub(crate)`, not private: `insights.rs` (a sibling module) reads this
    /// directly in a test to hold the "broken > uncovered > insight" ordering
    /// claim against the one real ordinal table, instead of re-deriving it.
    pub(crate) fn ordinal(self) -> u8 {
        match self {
            AttentionRank::Broken => 0,
            AttentionRank::Uncovered => 1,
            AttentionRank::Insight => 2,
            AttentionRank::Spare => 3,
            AttentionRank::Update => 4,
        }
    }
    /// Ranks that represent something the owner should act on. `Spare` and
    /// `Update` are deliberately excluded: neither means the environment is
    /// unsound, and putting them on the Overview is how a health surface turns
    /// back into a to-do list nobody reads.
    pub fn needs_action(self) -> bool {
        matches!(self, AttentionRank::Broken | AttentionRank::Uncovered)
    }
}

/// The single next step for an attention item.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttentionAction {
    /// Button label — an imperative naming its object ("Install nono").
    pub label: String,
    /// Capability the action runs against.
    pub capability_id: String,
    pub lifecycle: LifecycleAction,
    /// Set when there is no automated recipe; the UI shows this instead of
    /// running anything.
    pub guidance: Option<String>,
}

/// One row of "what's wrong → why you care → one action".
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttentionItem {
    pub rank: AttentionRank,
    pub group_id: String,
    pub group_name: String,
    /// The capability this concerns. Empty for a group-level gap, where the
    /// point is that *no* provider is present.
    pub capability_id: String,
    /// What is wrong — one sentence, no `WARN:`/`ERROR:` prefix.
    pub title: String,
    /// Why the owner should care, from the capability group's own explainer.
    pub why: String,
    /// Extra context that is not the group explainer — most often the reason a
    /// previous attempt to fix this did not take.
    pub note: Option<String>,
    pub action: Option<AttentionAction>,
    pub level: FindingLevel,
    /// (rank, group taxonomy index, provider taxonomy index) — stable ordering.
    sort: (u8, usize, usize),
}

/// How one capability group stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoverageState {
    /// At least one enabled provider is installed and working.
    Covered,
    /// A provider is installed but none of them work.
    Broken,
    /// No provider is installed.
    Uncovered,
    /// Every provider is disabled machine-wide — the owner's explicit choice.
    Off,
}

/// One line of the Overview coverage list — nine rows, one per capability.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoverageRow {
    pub group_id: String,
    pub group_name: String,
    pub state: CoverageState,
    /// Names of enabled providers that are installed and working.
    pub working: Vec<String>,
    /// Providers the owner has left enabled, installed or not.
    pub enabled_total: usize,
    /// Right-hand status text, e.g. "TruffleHog · 1 of 2 providers".
    pub summary: String,
    /// The provider fact a covered group leads with: the first working
    /// provider's name. `None` whenever the group is not covered — an absent
    /// fact renders as absent, never as a placeholder.
    pub provider: Option<String>,
    /// That provider's extracted version number, when the tool reports one.
    /// A tool with no version command (a real case) leaves this `None`.
    pub provider_version: Option<String>,
}

/// Per-capability rollup counts. `MenubarCounts` is built from this so the tray
/// and the Control Center cannot count differently.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct HealthCounts {
    pub groups_total: usize,
    pub groups_covered: usize,
    /// Capabilities the owner has left enabled — the population every other
    /// count is drawn from, and the number the all-clear caption reports.
    pub enabled: u32,
    pub ok: u32,
    pub warnings: u32,
    pub errors: u32,
    pub missing: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HealthVerdict {
    pub verdict: Verdict,
    /// One sentence stating the situation.
    pub headline: String,
    /// One sentence NAMING what is wrong. The old header counted "2 errors" and
    /// then refused to say which two; that is the defect this field closes.
    pub detail: String,
    /// Every item, sorted worst-first. Filter with `AttentionRank::needs_action`
    /// for the Overview; capability pages show all of them.
    pub attention: Vec<AttentionItem>,
    pub coverage: Vec<CoverageRow>,
    pub counts: HealthCounts,
}

impl HealthVerdict {
    /// The items that belong on the Overview.
    pub fn action_items(&self) -> impl Iterator<Item = &AttentionItem> {
        self.attention
            .iter()
            .filter(|item| item.rank.needs_action())
    }
    /// Available updates, which are informational and never affect the verdict.
    pub fn updates(&self) -> impl Iterator<Item = &AttentionItem> {
        self.attention
            .iter()
            .filter(|item| item.rank == AttentionRank::Update)
    }
}

fn group_index(id: &str) -> usize {
    CAPABILITY_GROUPS
        .iter()
        .position(|group| group.id == id)
        .unwrap_or(usize::MAX)
}

fn provider_index(id: &str) -> usize {
    CAPABILITIES
        .iter()
        .position(|def| def.id == id)
        .unwrap_or(usize::MAX)
}

/// The group `why` strings are two sentences: the problem, then the mechanism.
/// An attention row has space for the problem.
fn first_sentence(text: &str) -> String {
    match text.find(". ") {
        Some(end) => text[..=end].trim().to_string(),
        None => text.trim().to_string(),
    }
}

fn count_word(n: usize) -> String {
    match n {
        1 => "One".to_string(),
        2 => "Two".to_string(),
        3 => "Three".to_string(),
        4 => "Four".to_string(),
        5 => "Five".to_string(),
        6 => "Six".to_string(),
        7 => "Seven".to_string(),
        8 => "Eight".to_string(),
        9 => "Nine".to_string(),
        other => other.to_string(),
    }
}

fn worst_level(cap: &CapabilityStatus) -> FindingLevel {
    cap.issues
        .iter()
        .map(|issue| match issue.level {
            FindingLevel::Degraded => FindingLevel::Warn,
            other => other,
        })
        .max()
        .unwrap_or(FindingLevel::Ok)
}

fn has_error(cap: &CapabilityStatus) -> bool {
    cap.issues
        .iter()
        .any(|issue| issue.level == FindingLevel::Error)
}

/// The remediation an action row offers for a capability, and whether it has to
/// be done by hand.
fn action_for(
    cap: &CapabilityStatus,
    lifecycle: LifecycleAction,
    label: String,
) -> AttentionAction {
    let automated = crate::gui::inventory::get_capability(&cap.id)
        .and_then(|def| crate::gui::inventory::action_argvs(def, lifecycle))
        .is_some();
    AttentionAction {
        label,
        capability_id: cap.id.clone(),
        lifecycle,
        guidance: if automated {
            None
        } else {
            Some(cap.guidance.clone().unwrap_or_else(|| {
                "no automated recipe — see the project's documentation".to_string()
            }))
        },
    }
}

/// A capability's error messages, each as a self-contained sentence. Detection
/// writes some of them as fragments ("last install failed (exit 1)") because
/// they are read directly under the capability's own name; in a mixed list they
/// have to say what they are about.
fn error_sentences(cap: &CapabilityStatus) -> Vec<String> {
    cap.issues
        .iter()
        .filter(|issue| issue.level == FindingLevel::Error)
        .map(|issue| {
            let body = if issue.message.starts_with(&cap.name) {
                issue.message.clone()
            } else {
                format!("{}: {}", cap.name, issue.message)
            };
            if body.ends_with(['.', '!', '?']) {
                body
            } else {
                format!("{body}.")
            }
        })
        .collect()
}

fn broken_item(cap: &CapabilityStatus, group_name: &str, why: &str) -> AttentionItem {
    let mut sentences = error_sentences(cap);
    let title = if sentences.is_empty() {
        format!("{} is not working.", cap.name)
    } else {
        sentences.remove(0)
    };
    AttentionItem {
        rank: AttentionRank::Broken,
        group_id: cap.capability.clone(),
        group_name: group_name.to_string(),
        capability_id: cap.id.clone(),
        title,
        why: why.to_string(),
        note: (!sentences.is_empty()).then(|| sentences.join(" ")),
        // Reinstall, never install: this item exists only for a tool that is
        // already on PATH, so the copy that is there is the thing to replace.
        action: Some(action_for(
            cap,
            LifecycleAction::Reinstall,
            format!("Reinstall {}", cap.name),
        )),
        level: FindingLevel::Error,
        sort: (
            AttentionRank::Broken.ordinal(),
            group_index(&cap.capability),
            provider_index(&cap.id),
        ),
    }
}

/// Build the verdict. Pure: same capabilities in, same verdict out.
pub fn build_verdict(capabilities: &[CapabilityStatus]) -> HealthVerdict {
    if capabilities.is_empty() {
        return HealthVerdict {
            verdict: Verdict::Unknown,
            headline: "Checking your environment…".to_string(),
            detail: String::new(),
            attention: Vec::new(),
            coverage: Vec::new(),
            counts: HealthCounts::default(),
        };
    }

    let coverage_map = GroupCoverage::from_statuses(capabilities);
    let enabled: Vec<&CapabilityStatus> = capabilities.iter().filter(|cap| cap.enabled).collect();

    let mut attention: Vec<AttentionItem> = Vec::new();
    let mut rows: Vec<CoverageRow> = Vec::new();
    let mut broken_names: Vec<String> = Vec::new();
    let mut uncovered_names: Vec<String> = Vec::new();
    let mut off_names: Vec<String> = Vec::new();

    for group in &CAPABILITY_GROUPS {
        let providers: Vec<&CapabilityStatus> = capabilities
            .iter()
            .filter(|cap| cap.capability == group.id)
            .collect();
        let enabled_providers: Vec<&&CapabilityStatus> =
            providers.iter().filter(|cap| cap.enabled).collect();
        let working = coverage_map.working(group.id).to_vec();
        let installed_enabled = enabled_providers.iter().filter(|cap| cap.installed).count();

        let state = if enabled_providers.is_empty() {
            CoverageState::Off
        } else if !working.is_empty() {
            CoverageState::Covered
        } else if installed_enabled > 0 {
            CoverageState::Broken
        } else {
            CoverageState::Uncovered
        };

        let summary = match state {
            CoverageState::Off => "turned off".to_string(),
            CoverageState::Covered => {
                if enabled_providers.len() > 1 {
                    format!(
                        "{} · {} of {} providers",
                        join_human(&working),
                        working.len(),
                        enabled_providers.len()
                    )
                } else {
                    join_human(&working)
                }
            }
            CoverageState::Broken => "installed but not working".to_string(),
            CoverageState::Uncovered => "no provider installed".to_string(),
        };

        match state {
            CoverageState::Off => off_names.push(group.name.to_string()),
            CoverageState::Uncovered => {
                uncovered_names.push(group.name.to_string());
                // A failed install of an absent tool is not a second problem —
                // it is the reason this one is still open. Carried as context
                // on the gap rather than raised as its own row.
                let failures: Vec<String> = enabled_providers
                    .iter()
                    .flat_map(|cap| error_sentences(cap))
                    .collect();
                // One row per gap, not one per candidate provider: the point is
                // that the capability is absent, and offering three equivalent
                // "install this instead" buttons is not a decision aid.
                let primary = enabled_providers
                    .iter()
                    .copied()
                    .min_by_key(|cap| provider_index(&cap.id));
                let action = primary.map(|cap| {
                    let automated = crate::gui::inventory::get_capability(&cap.id)
                        .and_then(|def| {
                            crate::gui::inventory::action_argvs(def, LifecycleAction::Install)
                        })
                        .is_some();
                    action_for(
                        cap,
                        LifecycleAction::Install,
                        if automated {
                            format!("Install {}", cap.name)
                        } else {
                            format!("How to install {}", cap.name)
                        },
                    )
                });
                attention.push(AttentionItem {
                    rank: AttentionRank::Uncovered,
                    group_id: group.id.to_string(),
                    group_name: group.name.to_string(),
                    capability_id: String::new(),
                    title: format!("{} has no provider installed.", group.name),
                    why: first_sentence(group.why),
                    note: (!failures.is_empty()).then(|| failures.join(" ")),
                    action,
                    // A gap the owner already tried to close is worse than one
                    // never attempted, and it is why the overall verdict is
                    // red — the row has to say so, or the icon and the list
                    // disagree about the same fact.
                    level: if failures.is_empty() {
                        FindingLevel::Warn
                    } else {
                        FindingLevel::Error
                    },
                    sort: (
                        AttentionRank::Uncovered.ordinal(),
                        group_index(group.id),
                        primary.map(|cap| provider_index(&cap.id)).unwrap_or(0),
                    ),
                });
            }
            CoverageState::Broken | CoverageState::Covered => {}
        }

        // The provider fact a covered group leads with. Computed only for a
        // covered group: a broken or absent provider has no fact to state.
        let provider = (state == CoverageState::Covered)
            .then(|| working.first().cloned())
            .flatten();
        let provider_version = provider.as_ref().and_then(|name| {
            providers
                .iter()
                .find(|cap| &cap.name == name)
                .and_then(|cap| cap.short_version())
        });

        rows.push(CoverageRow {
            group_id: group.id.to_string(),
            group_name: group.name.to_string(),
            state,
            working,
            enabled_total: enabled_providers.len(),
            summary,
            provider,
            provider_version,
        });
    }

    for cap in &enabled {
        let group = get_group(&cap.capability);
        let group_name = group.map(|g| g.name).unwrap_or("");
        let why = group.map(|g| first_sentence(g.why)).unwrap_or_default();
        // "Broken" means installed and not working. A tool that never arrived
        // cannot be broken; its errors are the history of trying to install it,
        // and they belong on the gap or the spare row instead — otherwise one
        // problem is counted twice and the headline overstates the work.
        if cap.installed && has_error(cap) {
            broken_names.push(cap.name.clone());
            attention.push(broken_item(cap, group_name, &why));
            continue;
        }
        if !cap.installed && coverage_map.is_covered(&cap.capability) {
            let covered_by = join_human(coverage_map.working(&cap.capability));
            attention.push(AttentionItem {
                rank: AttentionRank::Spare,
                group_id: cap.capability.clone(),
                group_name: group_name.to_string(),
                capability_id: cap.id.clone(),
                title: format!("{} is not installed.", cap.name),
                why: format!(
                    "{group_name} is already covered by {covered_by}. A second provider catches \
                     what the first one misses, at the cost of a slower commit."
                ),
                note: {
                    let failures = error_sentences(cap);
                    (!failures.is_empty()).then(|| failures.join(" "))
                },
                action: Some(action_for(
                    cap,
                    LifecycleAction::Install,
                    format!("Install {}", cap.name),
                )),
                level: FindingLevel::Info,
                sort: (
                    AttentionRank::Spare.ordinal(),
                    group_index(&cap.capability),
                    provider_index(&cap.id),
                ),
            });
        }
        if cap.update_available {
            let latest = cap.latest_version.clone().unwrap_or_default();
            attention.push(AttentionItem {
                rank: AttentionRank::Update,
                group_id: cap.capability.clone(),
                group_name: group_name.to_string(),
                capability_id: cap.id.clone(),
                title: format!("{} can update to {latest}.", cap.name),
                why: String::new(),
                note: None,
                action: Some(action_for(
                    cap,
                    LifecycleAction::Update,
                    format!("Update to {latest}"),
                )),
                level: FindingLevel::Info,
                sort: (
                    AttentionRank::Update.ordinal(),
                    group_index(&cap.capability),
                    provider_index(&cap.id),
                ),
            });
        }
    }

    attention.sort_by_key(|item| item.sort);

    let errors = enabled.iter().filter(|cap| has_error(cap)).count() as u32;
    let warnings = enabled
        .iter()
        .filter(|cap| !has_error(cap) && worst_level(cap) == FindingLevel::Warn)
        .count() as u32;
    let missing = enabled.iter().filter(|cap| !cap.installed).count() as u32;
    let ok = enabled
        .iter()
        .filter(|cap| cap.installed && !has_error(cap))
        .count() as u32;
    let groups_covered = rows
        .iter()
        .filter(|row| matches!(row.state, CoverageState::Covered))
        .count();

    let action_count = attention
        .iter()
        .filter(|item| item.rank.needs_action())
        .count();
    let verdict = if errors > 0 || rows.iter().any(|row| row.state == CoverageState::Broken) {
        Verdict::NotWorking
    } else if action_count > 0 || warnings > 0 {
        Verdict::NeedsAttention
    } else {
        Verdict::Sound
    };

    let headline = match verdict {
        Verdict::Sound => "Your agent environment is sound.".to_string(),
        Verdict::Unknown => "Checking your environment…".to_string(),
        _ => {
            let n = action_count.max(1);
            format!(
                "{} thing{} need{} your attention.",
                count_word(n),
                if n == 1 { "" } else { "s" },
                if n == 1 { "s" } else { "" }
            )
        }
    };

    let mut sentences: Vec<String> = Vec::new();
    if !broken_names.is_empty() {
        sentences.push(format!(
            "{} {} working as expected.",
            join_human(&broken_names),
            if broken_names.len() == 1 {
                "isn't"
            } else {
                "aren't"
            }
        ));
    }
    if !uncovered_names.is_empty() {
        sentences.push(format!(
            "{} {} no provider installed.",
            join_human(&uncovered_names),
            if uncovered_names.len() == 1 {
                "has"
            } else {
                "have"
            }
        ));
    }
    if sentences.is_empty() {
        let total = rows.len();
        sentences.push(if groups_covered == total {
            format!("All {total} capabilities have a working provider.")
        } else {
            format!("{groups_covered} of {total} capabilities have a working provider.")
        });
    }
    // Naming what the owner switched off keeps a deliberately narrowed
    // environment from reading as an accidentally complete one.
    if !off_names.is_empty() {
        sentences.push(format!(
            "{} {} turned off.",
            join_human(&off_names),
            if off_names.len() == 1 { "is" } else { "are" }
        ));
    }

    HealthVerdict {
        verdict,
        headline,
        detail: sentences.join(" "),
        attention,
        coverage: rows,
        counts: HealthCounts {
            groups_total: CAPABILITY_GROUPS.len(),
            groups_covered,
            enabled: enabled.len() as u32,
            ok,
            warnings,
            errors,
            missing,
        },
    }
}

/// The coverage consequence of uninstalling one provider, as a sentence the
/// confirm dialog can print verbatim. Derived from the same `GroupCoverage`
/// the verdict uses, so the dialog and the Overview cannot disagree about what
/// a removal costs. Pure: same statuses in, same sentence out. An id that
/// resolves to nothing yields an empty string — the caller renders nothing
/// rather than a guess.
pub fn removal_consequence(capabilities: &[CapabilityStatus], id: &str) -> String {
    let Some(cap) = capabilities.iter().find(|cap| cap.id == id) else {
        return String::new();
    };
    let Some(group) = get_group(&cap.capability) else {
        return String::new();
    };
    let coverage = GroupCoverage::from_statuses(capabilities);
    let working = coverage.working(group.id);
    let others: Vec<String> = working
        .iter()
        .filter(|name| *name != &cap.name)
        .cloned()
        .collect();
    if working.iter().any(|name| name == &cap.name) {
        // Removing a provider that is doing the covering.
        if others.is_empty() {
            format!("{} will have no working provider.", group.name)
        } else {
            format!(
                "{} also {} {}, so coverage remains.",
                join_human(&others),
                if others.len() == 1 { "covers" } else { "cover" },
                group.name
            )
        }
    } else if others.is_empty() {
        // The tool being removed was not covering anything, and nothing else
        // is either — stated plainly so the dialog never implies the removal
        // creates a gap that already exists.
        format!("{} already has no working provider.", group.name)
    } else {
        format!("{} remains covered by {}.", group.name, join_human(&others))
    }
}

/// One row of the bulk-install preview: a capability that is enabled but not
/// installed machine-wide, with the REAL command its install job would run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BulkCandidate {
    pub capability_id: String,
    pub name: String,
    /// What installing this fixes — the capability group's name.
    pub group_name: String,
    /// The exact install command, taken from the same recipe table the job
    /// runner executes ("brew install nono") — never composed by hand. `None`
    /// means no automated recipe exists (a manual-method capability), which
    /// the preview shows as disabled rather than omitting.
    pub command: Option<String>,
}

impl BulkCandidate {
    pub fn automatable(&self) -> bool {
        self.command.is_some()
    }
}

/// Every enabled capability missing machine-wide, in taxonomy order. The
/// command fact comes from `action_argvs` — the argv the job would actually
/// execute — so the preview cannot drift from what confirming it runs. For a
/// multi-step recipe the fact is the final (installing) command.
pub fn bulk_install_candidates(capabilities: &[CapabilityStatus]) -> Vec<BulkCandidate> {
    let mut missing: Vec<&CapabilityStatus> = capabilities
        .iter()
        .filter(|cap| cap.enabled && !cap.installed)
        .collect();
    missing.sort_by_key(|cap| provider_index(&cap.id));
    missing
        .iter()
        .map(|cap| BulkCandidate {
            capability_id: cap.id.clone(),
            name: cap.name.clone(),
            group_name: get_group(&cap.capability)
                .map(|group| group.name.to_string())
                .unwrap_or_default(),
            command: crate::gui::inventory::get_capability(&cap.id)
                .and_then(|def| crate::gui::inventory::action_argvs(def, LifecycleAction::Install))
                .and_then(|argvs| argvs.last().map(|argv| argv.join(" "))),
        })
        .collect()
}

impl CoverageState {
    /// Terminal glyph — plain ASCII (Phase F). The Control Center paints its
    /// own geometry; the CLI states the same four states in characters that
    /// can never land on a terminal's emoji rendering path (macOS draws
    /// U+26A0 as an orange emoji triangle).
    pub fn glyph(self) -> &'static str {
        match self {
            CoverageState::Covered => "ok",
            CoverageState::Broken => "x",
            CoverageState::Uncovered => "!",
            CoverageState::Off => "-",
        }
    }
}

/// Render the verdict for a terminal. Lives in core, and is tested, so the CLI
/// surface stays a two-line dispatch and the wording is verified once.
pub fn render_health_text(health: &HealthVerdict) -> String {
    let mut out = vec![health.headline.clone()];
    if !health.detail.is_empty() {
        out.push(health.detail.clone());
    }

    if !health.coverage.is_empty() {
        out.push(String::new());
        let width = health
            .coverage
            .iter()
            .map(|row| row.group_name.chars().count())
            .max()
            .unwrap_or(0);
        for row in &health.coverage {
            let pad = " ".repeat(width - row.group_name.chars().count());
            // The glyph column is two characters wide ("ok" is the longest),
            // so the group-name column stays aligned.
            out.push(format!(
                "  {:<2} {}{pad}   {}",
                row.state.glyph(),
                row.group_name,
                row.summary
            ));
        }
    }

    let actions: Vec<&AttentionItem> = health.action_items().collect();
    if !actions.is_empty() {
        out.push(String::new());
        out.push("Needs your attention".to_string());
        for item in actions {
            out.push(format!("  {} {}", item.level_glyph(), item.title));
            if let Some(note) = &item.note {
                out.push(format!("    {note}"));
            }
            if !item.why.is_empty() {
                out.push(format!("    {}", item.why));
            }
            if let Some(action) = &item.action {
                out.push(format!("    -> {}", action.label));
                // A manual capability has no recipe to run, so the step itself
                // is the instruction — printed under the label, not instead of
                // it, or the line reads "Install" for something already there.
                if let Some(guidance) = &action.guidance {
                    out.push(format!("      {guidance}"));
                }
            }
        }
    }

    let updates: Vec<&AttentionItem> = health.updates().collect();
    if !updates.is_empty() {
        out.push(String::new());
        out.push("Updates available".to_string());
        for item in updates {
            out.push(format!("  - {}", item.title));
        }
    }

    out.join("\n")
}

impl AttentionItem {
    /// Terminal glyph for this item's severity — plain ASCII (Phase F).
    pub fn level_glyph(&self) -> &'static str {
        match self.level {
            FindingLevel::Error => "x",
            FindingLevel::Warn | FindingLevel::Degraded => "!",
            _ => "-",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gui::inventory::{CapabilityKind, LifecycleMethod};
    use crate::types::Finding;

    /// Build a status row for a real capability id so taxonomy lookups resolve.
    fn cap(id: &str, installed: bool, working: bool) -> CapabilityStatus {
        let def = crate::gui::inventory::get_capability(id).expect("real capability id");
        CapabilityStatus {
            id: def.id.to_string(),
            name: def.name.to_string(),
            kind: def.kind,
            capability: def.capability.to_string(),
            description: def.description.to_string(),
            method: def.method,
            pkg: def.pkg.map(String::from),
            guidance: def.guidance.map(String::from),
            installed,
            path: installed.then(|| format!("/opt/homebrew/bin/{id}")),
            version: (installed && working).then(|| "1.0.0".to_string()),
            reports_version: !def.version_args.is_empty(),
            inactive: None,
            running: None,
            enabled: true,
            latest_version: None,
            update_available: false,
            issues: Vec::new(),
        }
    }

    /// Every capability installed and working — the all-green baseline.
    fn all_healthy() -> Vec<CapabilityStatus> {
        CAPABILITIES
            .iter()
            .map(|def| cap(def.id, true, true))
            .collect()
    }

    fn find<'a>(caps: &'a mut [CapabilityStatus], id: &str) -> &'a mut CapabilityStatus {
        caps.iter_mut().find(|cap| cap.id == id).expect(id)
    }

    #[test]
    fn an_empty_slice_is_unknown_not_sound() {
        let verdict = build_verdict(&[]);
        assert_eq!(verdict.verdict, Verdict::Unknown);
        assert!(verdict.coverage.is_empty());
        assert!(verdict.detail.is_empty());
        assert_eq!(verdict.counts, HealthCounts::default());
        assert_eq!(verdict.action_items().count(), 0);
    }

    #[test]
    fn a_fully_installed_environment_is_sound_with_no_action_items() {
        let verdict = build_verdict(&all_healthy());
        assert_eq!(verdict.verdict, Verdict::Sound);
        assert_eq!(verdict.headline, "Your agent environment is sound.");
        assert_eq!(
            verdict.detail,
            format!(
                "All {} capabilities have a working provider.",
                CAPABILITY_GROUPS.len()
            )
        );
        assert_eq!(verdict.action_items().count(), 0);
        assert_eq!(verdict.counts.groups_covered, CAPABILITY_GROUPS.len());
        assert_eq!(verdict.counts.enabled, CAPABILITIES.len() as u32);
        assert!(verdict
            .coverage
            .iter()
            .all(|row| row.state == CoverageState::Covered));
        // Every covered row leads with a concrete provider fact.
        assert!(verdict.coverage.iter().all(|row| row.provider.is_some()));
        let secrets = verdict
            .coverage
            .iter()
            .find(|row| row.group_id == "secret-scanning")
            .unwrap();
        assert_eq!(secrets.provider.as_deref(), Some("TruffleHog"));
        assert_eq!(secrets.provider_version.as_deref(), Some("1.0.0"));
    }

    #[test]
    fn provider_facts_exist_only_where_a_group_is_actually_covered() {
        let mut caps = all_healthy();
        // A gap: no fact to state.
        let hole = find(&mut caps, "nono");
        hole.installed = false;
        hole.version = None;
        // A broken group: rtk is Token Efficiency's only provider.
        let broken = find(&mut caps, "rtk");
        broken.version = None;
        broken.issues.push(Finding::error("probe failed"));
        // A group covered by a tool with NO version command (ccc): the name is
        // a fact, the version honestly is not.
        let manual = find(&mut caps, "cocoindex");
        manual.installed = false;
        manual.version = None;
        let ccc = find(&mut caps, "ccc");
        ccc.version = None;
        assert!(!ccc.reports_version, "ccc has no version command");
        // An off group states nothing either.
        let off = find(&mut caps, "openwiki");
        off.enabled = false;
        let verdict = build_verdict(&caps);
        let row = |id: &str| {
            verdict
                .coverage
                .iter()
                .find(|row| row.group_id == id)
                .unwrap()
        };
        assert_eq!(row("sandboxing").state, CoverageState::Uncovered);
        assert_eq!(row("sandboxing").provider, None);
        assert_eq!(row("sandboxing").provider_version, None);
        assert_eq!(row("token-efficiency").state, CoverageState::Broken);
        assert_eq!(row("token-efficiency").provider, None);
        assert_eq!(row("codebase-wiki").state, CoverageState::Off);
        assert_eq!(row("codebase-wiki").provider, None);
        let search = row("semantic-search");
        assert_eq!(search.state, CoverageState::Covered);
        assert_eq!(search.provider.as_deref(), Some("CocoIndex Code CLI (ccc)"));
        assert_eq!(
            search.provider_version, None,
            "a tool with no version command reports no version — absent, not zero"
        );
        // A covered group with a reporting provider carries the number.
        let hooks = row("hook-orchestration");
        assert_eq!(hooks.provider.as_deref(), Some("pre-commit"));
        assert_eq!(hooks.provider_version.as_deref(), Some("1.0.0"));
    }

    #[test]
    fn a_missing_spare_provider_is_never_ranked_as_a_gap() {
        // The defect this module exists to fix: TruffleHog is scanning, so a
        // missing Gitleaks must not rank alongside a capability with nothing.
        let mut caps = all_healthy();
        find(&mut caps, "gitleaks").installed = false;
        find(&mut caps, "gitleaks").version = None;
        let verdict = build_verdict(&caps);
        assert_eq!(verdict.verdict, Verdict::Sound);
        assert_eq!(verdict.action_items().count(), 0);
        let spare = verdict
            .attention
            .iter()
            .find(|item| item.capability_id == "gitleaks")
            .expect("gitleaks listed as a spare");
        assert_eq!(spare.rank, AttentionRank::Spare);
        assert!(!spare.rank.needs_action());
        assert_eq!(spare.level, FindingLevel::Info);
        assert!(spare.why.contains("already covered by TruffleHog"));
        // Secret Scanning is still covered, and says by whom.
        let row = verdict
            .coverage
            .iter()
            .find(|row| row.group_id == "secret-scanning")
            .unwrap();
        assert_eq!(row.state, CoverageState::Covered);
        assert_eq!(row.summary, "TruffleHog · 1 of 2 providers");
    }

    #[test]
    fn a_capability_with_no_provider_is_a_gap_that_names_itself() {
        let mut caps = all_healthy();
        let hole = find(&mut caps, "nono");
        hole.installed = false;
        hole.version = None;
        let verdict = build_verdict(&caps);
        assert_eq!(verdict.verdict, Verdict::NeedsAttention);
        assert_eq!(verdict.headline, "One thing needs your attention.");
        assert_eq!(
            verdict.detail,
            "Execution Sandboxing has no provider installed."
        );
        let items: Vec<&AttentionItem> = verdict.action_items().collect();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].rank, AttentionRank::Uncovered);
        assert_eq!(items[0].group_id, "sandboxing");
        // Group-level: the point is that nothing is present, not that one
        // particular tool is absent.
        assert!(items[0].capability_id.is_empty());
        assert!(items[0].why.starts_with("Coding agents run shell commands"));
        // nono installs through Homebrew, so the gap offers a one-click recipe.
        let action = items[0].action.as_ref().unwrap();
        assert_eq!(action.label, "Install nono");
        assert_eq!(action.capability_id, "nono");
        assert!(action.guidance.is_none());
    }

    #[test]
    fn a_gap_offers_the_first_provider_in_taxonomy_order() {
        let mut caps = all_healthy();
        for id in ["cocoindex", "ccc"] {
            find(&mut caps, id).installed = false;
            find(&mut caps, id).version = None;
        }
        let verdict = build_verdict(&caps);
        let gap = verdict
            .action_items()
            .find(|item| item.group_id == "semantic-search")
            .unwrap();
        // One row for the gap, not one per candidate provider.
        assert_eq!(
            verdict
                .action_items()
                .filter(|item| item.group_id == "semantic-search")
                .count(),
            1
        );
        let action = gap.action.as_ref().unwrap();
        assert_eq!(action.capability_id, "cocoindex");
        // CocoIndex is a Python framework nothing packages, so this gap really
        // does need a human — the label says so rather than offering a button
        // that cannot work.
        assert_eq!(action.label, "How to install CocoIndex");
        assert!(action.guidance.as_deref().unwrap().contains("cocoindex.io"));
    }

    #[test]
    fn an_installed_but_broken_provider_outranks_every_gap() {
        let mut caps = all_healthy();
        let hole = find(&mut caps, "nono");
        hole.installed = false;
        hole.version = None;
        let broken = find(&mut caps, "rtk");
        broken.version = None;
        broken.issues.push(Finding::error_with(
            "RTK is on PATH but its version probe failed",
            "Run `rtk --version` in a terminal to inspect",
        ));
        let verdict = build_verdict(&caps);
        assert_eq!(verdict.verdict, Verdict::NotWorking);
        assert_eq!(verdict.headline, "Two things need your attention.");
        assert_eq!(
            verdict.detail,
            "RTK isn't working as expected. Execution Sandboxing has no provider installed."
        );
        let items: Vec<&AttentionItem> = verdict.action_items().collect();
        assert_eq!(items[0].rank, AttentionRank::Broken);
        assert_eq!(items[0].capability_id, "rtk");
        assert!(items[0].title.contains("version probe failed"));
        assert_eq!(
            items[0].action.as_ref().unwrap().label,
            "Reinstall RTK",
            "an installed-but-broken tool is reinstalled, not installed"
        );
        assert_eq!(items[1].rank, AttentionRank::Uncovered);
        assert_eq!(verdict.counts.errors, 1);
    }

    #[test]
    fn ordering_is_stable_across_identical_polls_and_ties_break_on_taxonomy() {
        let mut caps = all_healthy();
        // Two gaps, in reverse taxonomy order relative to the vector order.
        for id in ["nono", "osv-scanner"] {
            find(&mut caps, id).installed = false;
            find(&mut caps, id).version = None;
        }
        caps.reverse();
        let first = build_verdict(&caps);
        let groups: Vec<&str> = first
            .action_items()
            .map(|item| item.group_id.as_str())
            .collect();
        // dependency-scanning precedes sandboxing in CAPABILITY_GROUPS.
        assert_eq!(groups, vec!["dependency-scanning", "sandboxing"]);
        caps.reverse();
        let second = build_verdict(&caps);
        assert_eq!(
            first.attention, second.attention,
            "reordering the input must not reorder the output"
        );
    }

    #[test]
    fn updates_are_listed_without_making_the_environment_unsound() {
        let mut caps = all_healthy();
        let stale = find(&mut caps, "trufflehog");
        stale.version = Some("3.95.9".into());
        stale.latest_version = Some("3.96.0".into());
        stale.update_available = true;
        let verdict = build_verdict(&caps);
        assert_eq!(verdict.verdict, Verdict::Sound);
        assert_eq!(verdict.action_items().count(), 0);
        let update: Vec<&AttentionItem> = verdict.updates().collect();
        assert_eq!(update.len(), 1);
        assert_eq!(update[0].rank, AttentionRank::Update);
        assert_eq!(update[0].title, "TruffleHog can update to 3.96.0.");
        assert_eq!(update[0].action.as_ref().unwrap().label, "Update to 3.96.0");
        assert_eq!(
            update[0].action.as_ref().unwrap().lifecycle,
            LifecycleAction::Update
        );
    }

    #[test]
    fn disabling_every_provider_cannot_manufacture_a_silent_sound_verdict() {
        let mut caps = all_healthy();
        let off = find(&mut caps, "nono");
        off.installed = false;
        off.version = None;
        off.enabled = false;
        let verdict = build_verdict(&caps);
        // The owner's explicit choice is honoured — but it is stated, not hidden.
        assert_eq!(verdict.verdict, Verdict::Sound);
        assert_eq!(verdict.action_items().count(), 0);
        assert!(
            verdict
                .detail
                .contains("Execution Sandboxing is turned off."),
            "detail must name what was switched off, got: {}",
            verdict.detail
        );
        let row = verdict
            .coverage
            .iter()
            .find(|row| row.group_id == "sandboxing")
            .unwrap();
        assert_eq!(row.state, CoverageState::Off);
        assert_eq!(row.enabled_total, 0);
        assert_eq!(row.summary, "turned off");
        // A disabled capability never produces an attention row of its own.
        assert!(verdict
            .attention
            .iter()
            .all(|item| item.capability_id != "nono"));
    }

    #[test]
    fn a_group_whose_only_installed_provider_is_broken_reads_as_not_working() {
        let mut caps = all_healthy();
        let broken = find(&mut caps, "nono");
        broken.version = None;
        broken.issues.push(Finding::error(
            "nono is on PATH but its version probe failed",
        ));
        let verdict = build_verdict(&caps);
        assert_eq!(verdict.verdict, Verdict::NotWorking);
        let row = verdict
            .coverage
            .iter()
            .find(|row| row.group_id == "sandboxing")
            .unwrap();
        assert_eq!(row.state, CoverageState::Broken);
        assert_eq!(row.summary, "installed but not working");
        assert!(row.working.is_empty());
        // Not double-counted as a gap: it is present, it just does not work.
        assert!(verdict
            .action_items()
            .all(|item| item.rank == AttentionRank::Broken));
    }

    #[test]
    fn coverage_rows_cover_the_whole_taxonomy_in_order() {
        let verdict = build_verdict(&all_healthy());
        let ids: Vec<&str> = verdict
            .coverage
            .iter()
            .map(|row| row.group_id.as_str())
            .collect();
        let expected: Vec<&str> = CAPABILITY_GROUPS.iter().map(|group| group.id).collect();
        assert_eq!(ids, expected);
        let harness = verdict
            .coverage
            .iter()
            .find(|row| row.group_id == "coding-harness")
            .unwrap();
        assert_eq!(harness.enabled_total, 7);
        assert_eq!(harness.working.len(), 7);
        assert!(harness.summary.ends_with("7 of 7 providers"));
    }

    #[test]
    fn headline_pluralisation_and_number_words() {
        assert_eq!(count_word(1), "One");
        assert_eq!(count_word(9), "Nine");
        assert_eq!(count_word(10), "10");
        let mut caps = all_healthy();
        for id in ["nono", "osv-scanner", "ocean"] {
            find(&mut caps, id).installed = false;
            find(&mut caps, id).version = None;
        }
        assert_eq!(
            build_verdict(&caps).headline,
            "Three things need your attention."
        );
    }

    #[test]
    fn sentence_and_list_helpers_handle_every_arity() {
        assert_eq!(join_human(&[]), "");
        assert_eq!(join_human(&["A".into()]), "A");
        assert_eq!(join_human(&["A".into(), "B".into()]), "A and B");
        assert_eq!(
            join_human(&["A".into(), "B".into(), "C".into()]),
            "A, B and C"
        );
        assert_eq!(first_sentence("One. Two."), "One.");
        assert_eq!(first_sentence("No terminator"), "No terminator");
        assert_eq!(first_sentence("  padded. tail "), "padded.");
        // Every group explainer must yield a usable first sentence.
        for group in &CAPABILITY_GROUPS {
            let opener = first_sentence(group.why);
            assert!(opener.ends_with('.'), "{} why has no sentence", group.id);
            assert!(opener.len() < group.why.len(), "{} not split", group.id);
        }
    }

    #[test]
    fn degraded_findings_rank_as_warnings_not_errors() {
        let mut caps = all_healthy();
        find(&mut caps, "rtk")
            .issues
            .push(Finding::degraded("partially configured", "finish setup"));
        let verdict = build_verdict(&caps);
        assert_eq!(verdict.counts.errors, 0);
        assert_eq!(verdict.counts.warnings, 1);
        assert_eq!(verdict.verdict, Verdict::NeedsAttention);
    }

    #[test]
    fn counts_track_enabled_capabilities_only() {
        let mut caps = all_healthy();
        find(&mut caps, "gitleaks").enabled = false;
        let missing = find(&mut caps, "nono");
        missing.installed = false;
        missing.version = None;
        let verdict = build_verdict(&caps);
        assert_eq!(verdict.counts.enabled, (CAPABILITIES.len() - 1) as u32);
        assert_eq!(verdict.counts.ok, (CAPABILITIES.len() - 2) as u32);
        assert_eq!(verdict.counts.missing, 1);
        assert_eq!(verdict.counts.errors, 0);
        assert_eq!(verdict.counts.groups_total, CAPABILITY_GROUPS.len());
    }

    #[test]
    fn the_text_rendering_states_the_verdict_then_names_the_work() {
        let mut caps = all_healthy();
        let hole = find(&mut caps, "nono");
        hole.installed = false;
        hole.version = None;
        let stale = find(&mut caps, "trufflehog");
        stale.latest_version = Some("3.96.0".into());
        stale.update_available = true;
        let text = render_health_text(&build_verdict(&caps));
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines[0], "One thing needs your attention.");
        assert_eq!(lines[1], "Execution Sandboxing has no provider installed.");
        // Coverage list: one aligned row per capability, worst state visible.
        // The glyph column is two wide, so "!" carries a trailing pad space
        // and the group names line up with "ok" rows.
        assert!(text.contains("  !  Execution Sandboxing"));
        assert!(text.contains("  ok Secret Scanning"));
        // The action block names the problem, the reason, and the next step.
        assert!(text.contains("Needs your attention"));
        assert!(text.contains("    -> Install nono"));
        // A capability nothing packages prints its documentation under the
        // label instead of pretending a recipe exists.
        let mut manual = all_healthy();
        let ocean = find(&mut manual, "ocean");
        ocean.installed = false;
        ocean.version = None;
        let manual_text = render_health_text(&build_verdict(&manual));
        assert!(manual_text.contains("    -> How to install OCEAN"));
        assert!(manual_text.contains("      Install from https://github.com/grcengineering/OCEAN"));
        // Updates are listed apart from the things that need doing.
        let attention_at = text.find("Needs your attention").unwrap();
        let updates_at = text.find("Updates available").unwrap();
        assert!(attention_at < updates_at);
        assert!(text.contains("- TruffleHog can update to 3.96.0."));
    }

    #[test]
    fn the_text_rendering_stays_quiet_when_nothing_is_wrong() {
        let text = render_health_text(&build_verdict(&all_healthy()));
        assert!(text.starts_with("Your agent environment is sound."));
        assert!(!text.contains("Needs your attention"));
        assert!(!text.contains("Updates available"));
        // Pre-detection renders the headline alone, with no empty scaffolding.
        assert_eq!(
            render_health_text(&build_verdict(&[])),
            "Checking your environment…"
        );
    }

    #[test]
    fn every_state_and_severity_has_an_ascii_glyph() {
        // ASCII only (Phase F): terminals render the old set as emoji.
        assert_eq!(CoverageState::Covered.glyph(), "ok");
        assert_eq!(CoverageState::Broken.glyph(), "x");
        assert_eq!(CoverageState::Uncovered.glyph(), "!");
        assert_eq!(CoverageState::Off.glyph(), "-");
        let mut caps = all_healthy();
        let broken = find(&mut caps, "rtk");
        broken.version = None;
        broken.issues.push(Finding::error("probe failed"));
        let spare = find(&mut caps, "gitleaks");
        spare.installed = false;
        spare.version = None;
        let health = build_verdict(&caps);
        let glyphs: Vec<&str> = health
            .attention
            .iter()
            .map(|item| item.level_glyph())
            .collect();
        assert!(glyphs.contains(&"x"));
        assert!(glyphs.contains(&"-"));
        // A broken tool renders with its error glyph in the action block.
        assert!(render_health_text(&health).contains("  x "));
        // Nothing outside printable ASCII in any glyph.
        for glyph in [
            CoverageState::Covered.glyph(),
            CoverageState::Broken.glyph(),
            CoverageState::Uncovered.glyph(),
            CoverageState::Off.glyph(),
        ] {
            assert!(glyph.is_ascii(), "non-ASCII coverage glyph: {glyph}");
        }
    }

    #[test]
    fn a_failed_install_of_an_absent_tool_is_one_problem_not_two() {
        // Found on a real machine: OpenWiki's install had failed and it was the
        // only Codebase Wiki provider, so it appeared BOTH as a broken tool and
        // as an uncovered capability — and the headline said five things needed
        // attention when four did.
        let mut caps = all_healthy();
        let absent = find(&mut caps, "openwiki");
        absent.installed = false;
        absent.version = None;
        absent.path = None;
        absent.issues.push(Finding::error_with(
            "last install failed (exit 1)",
            "npm ERR! 403 Forbidden",
        ));
        let verdict = build_verdict(&caps);
        let items: Vec<&AttentionItem> = verdict.action_items().collect();
        assert_eq!(items.len(), 1, "one gap, stated once");
        assert_eq!(items[0].rank, AttentionRank::Uncovered);
        assert_eq!(items[0].title, "Codebase Wiki has no provider installed.");
        // The failure is not dropped — it becomes the reason the gap is open,
        // and it names its own subject rather than reading as a fragment.
        assert_eq!(
            items[0].note.as_deref(),
            Some("OpenWiki: last install failed (exit 1).")
        );
        assert_eq!(verdict.headline, "One thing needs your attention.");
        // Still red: an action the owner ran did fail. The row is red too, so
        // the overall icon and the list cannot disagree about the same fact.
        assert_eq!(verdict.verdict, Verdict::NotWorking);
        assert_eq!(verdict.counts.errors, 1);
        assert_eq!(items[0].level, FindingLevel::Error);
        assert_eq!(items[0].level_glyph(), "x");
        // A gap nobody has attempted stays amber.
        let mut untouched = all_healthy();
        let never_tried = find(&mut untouched, "nono");
        never_tried.installed = false;
        never_tried.version = None;
        let calm = build_verdict(&untouched);
        assert_eq!(calm.verdict, Verdict::NeedsAttention);
        assert_eq!(
            calm.action_items().next().unwrap().level,
            FindingLevel::Warn
        );
    }

    #[test]
    fn a_failed_install_on_a_covered_capability_rides_along_with_the_spare() {
        let mut caps = all_healthy();
        let absent = find(&mut caps, "gitleaks");
        absent.installed = false;
        absent.version = None;
        absent
            .issues
            .push(Finding::error("last install failed (exit 1)"));
        let verdict = build_verdict(&caps);
        assert_eq!(verdict.action_items().count(), 0, "a spare is not a gap");
        let spare = verdict
            .attention
            .iter()
            .find(|item| item.capability_id == "gitleaks")
            .unwrap();
        assert_eq!(spare.rank, AttentionRank::Spare);
        assert_eq!(
            spare.note.as_deref(),
            Some("Gitleaks: last install failed (exit 1)."),
            "the failure stays visible even where it is not urgent"
        );
    }

    #[test]
    fn error_sentences_name_their_subject_and_terminate() {
        let mut cap = cap("rtk", true, false);
        cap.issues = vec![
            Finding::error("RTK is on PATH but its version probe failed"),
            Finding::error("last update failed (exit 2)"),
            Finding::warn("ignored", "not an error"),
        ];
        assert_eq!(
            error_sentences(&cap),
            vec![
                "RTK is on PATH but its version probe failed.".to_string(),
                "RTK: last update failed (exit 2).".to_string(),
            ]
        );
        // The first becomes the title, the rest become the note.
        let item = broken_item(&cap, "Token Efficiency", "why");
        assert_eq!(item.title, "RTK is on PATH but its version probe failed.");
        assert_eq!(
            item.note.as_deref(),
            Some("RTK: last update failed (exit 2).")
        );
        assert_eq!(item.action.as_ref().unwrap().label, "Reinstall RTK");
        // A capability with no error at all still yields a usable sentence.
        cap.issues.clear();
        assert_eq!(
            broken_item(&cap, "Token Efficiency", "why").title,
            "RTK is not working."
        );
    }

    #[test]
    fn removing_the_last_working_provider_states_the_hole_it_leaves() {
        let mut caps = all_healthy();
        // nono is Execution Sandboxing's only provider.
        assert_eq!(
            removal_consequence(&caps, "nono"),
            "Execution Sandboxing will have no working provider."
        );
        // With the sibling gone, TruffleHog is the last secret scanner too.
        let spare = find(&mut caps, "gitleaks");
        spare.installed = false;
        spare.version = None;
        assert_eq!(
            removal_consequence(&caps, "trufflehog"),
            "Secret Scanning will have no working provider."
        );
    }

    #[test]
    fn removing_a_provider_with_a_working_sibling_names_the_sibling() {
        let caps = all_healthy();
        assert_eq!(
            removal_consequence(&caps, "gitleaks"),
            "TruffleHog also covers Secret Scanning, so coverage remains."
        );
        assert_eq!(
            removal_consequence(&caps, "trufflehog"),
            "Gitleaks also covers Secret Scanning, so coverage remains."
        );
        // Several siblings pluralise the verb and keep taxonomy order.
        assert_eq!(
            removal_consequence(&caps, "claude-code"),
            "Codex, Cursor, OpenCode, Antigravity, Hermes and Pi also cover \
             Coding Harness, so coverage remains."
        );
    }

    #[test]
    fn removing_a_provider_that_covers_nothing_never_claims_a_new_gap() {
        let mut caps = all_healthy();
        // Broken sole provider: the gap predates the removal, and the
        // sentence must not pretend the uninstall creates it.
        let broken = find(&mut caps, "rtk");
        broken.version = None;
        broken.issues.push(Finding::error("probe failed"));
        assert_eq!(
            removal_consequence(&caps, "rtk"),
            "Token Efficiency already has no working provider."
        );
        // Broken provider with a working sibling: coverage is elsewhere.
        let sick = find(&mut caps, "gitleaks");
        sick.version = None;
        sick.issues.push(Finding::error("probe failed"));
        assert_eq!(
            removal_consequence(&caps, "gitleaks"),
            "Secret Scanning remains covered by TruffleHog."
        );
        // A disabled provider is not covering anything either.
        let mut off = all_healthy();
        find(&mut off, "gitleaks").enabled = false;
        assert_eq!(
            removal_consequence(&off, "gitleaks"),
            "Secret Scanning remains covered by TruffleHog."
        );
        // An id that resolves to nothing yields nothing — never a guess.
        assert_eq!(removal_consequence(&all_healthy(), "no-such-tool"), "");
        assert_eq!(removal_consequence(&[], "nono"), "");
    }

    #[test]
    fn bulk_candidates_list_the_missing_with_their_real_install_commands() {
        let mut caps = all_healthy();
        for id in [
            "nono",
            "gitleaks",
            "openwiki",
            "codex",
            "codeguard",
            "ocean",
        ] {
            let gone = find(&mut caps, id);
            gone.installed = false;
            gone.version = None;
        }
        let candidates = bulk_install_candidates(&caps);
        let ids: Vec<&str> = candidates
            .iter()
            .map(|candidate| candidate.capability_id.as_str())
            .collect();
        // Taxonomy order, independent of input order.
        assert_eq!(
            ids,
            vec![
                "gitleaks",
                "ocean",
                "nono",
                "openwiki",
                "codeguard",
                "codex"
            ]
        );
        let by_id = |id: &str| {
            candidates
                .iter()
                .find(|candidate| candidate.capability_id == id)
                .unwrap()
        };
        // Command facts are the recipe table's own argv, joined — one per
        // method family, pinned so a recipe change breaks this test.
        assert_eq!(by_id("nono").command.as_deref(), Some("brew install nono"));
        assert_eq!(
            by_id("openwiki").command.as_deref(),
            Some("npm install -g openwiki")
        );
        assert_eq!(
            by_id("codex").command.as_deref(),
            Some("brew install --cask codex")
        );
        // Multi-step recipe: the fact is the final, installing command.
        assert_eq!(
            by_id("codeguard").command.as_deref(),
            Some("claude plugin install codeguard-security@project-codeguard")
        );
        // Manual method: present, honest about having no recipe.
        let manual = by_id("ocean");
        assert_eq!(manual.command, None);
        assert!(!manual.automatable());
        assert_eq!(manual.group_name, "Repository Hygiene");
        assert_eq!(by_id("nono").group_name, "Execution Sandboxing");
        assert!(by_id("nono").automatable());
    }

    #[test]
    fn bulk_candidates_exclude_the_installed_and_the_switched_off() {
        let mut caps = all_healthy();
        assert!(bulk_install_candidates(&caps).is_empty());
        let hole = find(&mut caps, "nono");
        hole.installed = false;
        hole.version = None;
        let off = find(&mut caps, "gitleaks");
        off.installed = false;
        off.version = None;
        off.enabled = false;
        let candidates = bulk_install_candidates(&caps);
        // The owner's explicit off-switch is honoured: gitleaks is missing but
        // deliberately so, and a bulk install must not override that choice.
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].capability_id, "nono");
    }

    /// Phase H: `AttentionRank::Insight` sits strictly between `Uncovered`
    /// and `Spare` — the "broken > uncovered > insight" placement rule
    /// `insights.rs` relies on, held here against the one real ordinal table
    /// rather than trusted from the enum's declaration order alone. Also:
    /// an insight never counts toward `needs_action` — unchanged behaviour
    /// for `action_items()`/nav badges/verdict counts, all of which must stay
    /// exactly as they were before this variant existed.
    #[test]
    fn insight_rank_sits_between_uncovered_and_spare_and_never_needs_action() {
        assert!(AttentionRank::Broken.ordinal() < AttentionRank::Uncovered.ordinal());
        assert!(AttentionRank::Uncovered.ordinal() < AttentionRank::Insight.ordinal());
        assert!(AttentionRank::Insight.ordinal() < AttentionRank::Spare.ordinal());
        assert!(AttentionRank::Spare.ordinal() < AttentionRank::Update.ordinal());
        assert!(!AttentionRank::Insight.needs_action());
    }

    #[test]
    fn kind_is_carried_through_untouched() {
        // Guards against a future refactor collapsing tools and harnesses.
        let caps = all_healthy();
        let harness = caps.iter().find(|cap| cap.id == "claude-code").unwrap();
        assert_eq!(harness.kind, CapabilityKind::Harness);
        let tool = caps.iter().find(|cap| cap.id == "trufflehog").unwrap();
        assert_eq!(tool.kind, CapabilityKind::Tool);
        assert_eq!(tool.method, LifecycleMethod::Brew);
    }
}
