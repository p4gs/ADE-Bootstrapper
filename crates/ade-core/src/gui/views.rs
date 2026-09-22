//! View models for the Control Center — every display decision as plain data.
//!
//! The GUI crates are render loops excluded from the coverage gate, so any
//! choice that can be got wrong (what a row says, which badge it carries,
//! which button is destructive, what command a fix panel shows) is computed
//! HERE, tested here, and handed to whichever toolkit is painting the
//! screen. Two contenders (egui and Dioxus) render these same structs during
//! the GUI bake-off, which is only a fair comparison if neither one gets to
//! decide anything the other doesn't.
//!
//! The shapes mirror the reference apps the owner picked (Taphouse, Updatest,
//! Homebrew.app): a source-list sidebar with count pills, a list pane of rows
//! with tag pills, an inspector with grouped label/value cards and a pill
//! action row, a fix panel with a terminal-command block, and a console
//! drawer with one tab per job.

use std::time::Duration;

use crate::gui::inventory::{
    action_argvs, CapabilityGroup, CapabilityStatus, LifecycleAction, LifecycleMethod,
};
use crate::gui::jobs::{Job, JobStatus};
use crate::gui::state::{SCOPE_ACTIVITY, SCOPE_OVERVIEW, SCOPE_PROJECTS};
use crate::gui::verdict::{AttentionItem, HealthVerdict};
use crate::types::FindingLevel;

/// Semantic colour intent. The toolkit resolves each tone through the token
/// layer (`gui::tokens`); no view model ever names a colour.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    Neutral,
    Muted,
    Accent,
    Ok,
    Warn,
    Err,
}

/// Icons by Lucide name. The GUI crate maps a name to a glyph through the
/// shipped `codepoints.json`; core only decides WHICH icon a thing gets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Icon {
    LayoutDashboard,
    ShieldCheck,
    KeyRound,
    GitCommitHorizontal,
    PackageSearch,
    PackageCheck,
    Container,
    Brush,
    Gauge,
    BookOpen,
    SearchCode,
    Terminal,
    FolderKanban,
    Activity,
    Settings,
    RefreshCw,
    CloudDownload,
    PanelLeft,
    Search,
    Check,
    TriangleAlert,
    CircleAlert,
    Circle,
    CircleDashed,
    CircleOff,
    ChevronRight,
    Ellipsis,
    Copy,
    Play,
    Trash2,
    X,
    Minus,
    ExternalLink,
    Info,
    Clock,
    SquareTerminal,
    RotateCw,
    CircleArrowUp,
    Power,
    PowerOff,
    Save,
    Eraser,
    ListFilter,
}

impl Icon {
    /// The Lucide icon name, exactly as it appears in `codepoints.json`.
    pub fn lucide_name(self) -> &'static str {
        match self {
            Icon::LayoutDashboard => "layout-dashboard",
            Icon::ShieldCheck => "shield-check",
            Icon::KeyRound => "key-round",
            Icon::GitCommitHorizontal => "git-commit-horizontal",
            Icon::PackageSearch => "package-search",
            Icon::PackageCheck => "package-check",
            Icon::Container => "container",
            Icon::Brush => "brush",
            Icon::Gauge => "gauge",
            Icon::BookOpen => "book-open",
            Icon::SearchCode => "search-code",
            Icon::Terminal => "terminal",
            Icon::FolderKanban => "folder-kanban",
            Icon::Activity => "activity",
            Icon::Settings => "settings",
            Icon::RefreshCw => "refresh-cw",
            Icon::CloudDownload => "cloud-download",
            Icon::PanelLeft => "panel-left",
            Icon::Search => "search",
            Icon::Check => "check",
            Icon::TriangleAlert => "triangle-alert",
            Icon::CircleAlert => "circle-alert",
            Icon::Circle => "circle",
            Icon::CircleDashed => "circle-dashed",
            Icon::CircleOff => "circle-off",
            Icon::ChevronRight => "chevron-right",
            Icon::Ellipsis => "ellipsis",
            Icon::Copy => "copy",
            Icon::Play => "play",
            Icon::Trash2 => "trash-2",
            Icon::X => "x",
            Icon::Minus => "minus",
            Icon::ExternalLink => "external-link",
            Icon::Info => "info",
            Icon::Clock => "clock",
            Icon::SquareTerminal => "square-terminal",
            Icon::RotateCw => "rotate-cw",
            Icon::CircleArrowUp => "circle-arrow-up",
            Icon::Power => "power",
            Icon::PowerOff => "power-off",
            Icon::Save => "save",
            Icon::Eraser => "eraser",
            Icon::ListFilter => "list-filter",
        }
    }

    /// Every variant, so a GUI-side test can prove each one has a glyph.
    pub const ALL: [Icon; 43] = [
        Icon::LayoutDashboard,
        Icon::ShieldCheck,
        Icon::KeyRound,
        Icon::GitCommitHorizontal,
        Icon::PackageSearch,
        Icon::PackageCheck,
        Icon::Container,
        Icon::Brush,
        Icon::Gauge,
        Icon::BookOpen,
        Icon::SearchCode,
        Icon::Terminal,
        Icon::FolderKanban,
        Icon::Activity,
        Icon::Settings,
        Icon::RefreshCw,
        Icon::CloudDownload,
        Icon::PanelLeft,
        Icon::Search,
        Icon::Check,
        Icon::TriangleAlert,
        Icon::CircleAlert,
        Icon::Circle,
        Icon::CircleDashed,
        Icon::CircleOff,
        Icon::ChevronRight,
        Icon::Ellipsis,
        Icon::Copy,
        Icon::Play,
        Icon::Trash2,
        Icon::X,
        Icon::Minus,
        Icon::ExternalLink,
        Icon::Info,
        Icon::Clock,
        Icon::SquareTerminal,
        Icon::RotateCw,
        Icon::CircleArrowUp,
        Icon::Power,
        Icon::PowerOff,
        Icon::Save,
        Icon::Eraser,
        Icon::ListFilter,
    ];
}

/// The icon a capability group carries in the sidebar and on its page.
/// Unknown ids get a neutral fallback rather than a panic — a new group must
/// never crash the sidebar.
pub fn group_icon(group_id: &str) -> Icon {
    match group_id {
        "agent-security-rules" => Icon::ShieldCheck,
        "secret-scanning" => Icon::KeyRound,
        "hook-orchestration" => Icon::GitCommitHorizontal,
        "dependency-scanning" => Icon::PackageSearch,
        "supply-chain-hardening" => Icon::PackageCheck,
        "sandboxing" => Icon::Container,
        "repo-hygiene" => Icon::Brush,
        "token-efficiency" => Icon::Gauge,
        "codebase-wiki" => Icon::BookOpen,
        "semantic-search" => Icon::SearchCode,
        "coding-harness" => Icon::Terminal,
        _ => Icon::Circle,
    }
}

/// The status a provider row leads with: one dot, one meaning.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderState {
    /// Enabled, installed, healthy.
    Working,
    /// Installed but with warnings (or an update available).
    Attention,
    /// Installed but not working: the guardrail that is not guarding.
    Broken,
    /// Not installed.
    Missing,
    /// Switched off machine-wide by the owner.
    Disabled,
}

impl ProviderState {
    pub fn tone(self) -> Tone {
        match self {
            ProviderState::Working => Tone::Ok,
            ProviderState::Attention => Tone::Warn,
            ProviderState::Broken => Tone::Err,
            ProviderState::Missing => Tone::Muted,
            ProviderState::Disabled => Tone::Muted,
        }
    }

    pub fn icon(self) -> Icon {
        match self {
            ProviderState::Working => Icon::Check,
            ProviderState::Attention => Icon::TriangleAlert,
            ProviderState::Broken => Icon::CircleAlert,
            ProviderState::Missing => Icon::CircleDashed,
            ProviderState::Disabled => Icon::CircleOff,
        }
    }
}

/// Classify one capability. Pure; the same status always yields the same
/// state, and `Disabled` wins over everything because it is the owner's call.
pub fn provider_state(cap: &CapabilityStatus) -> ProviderState {
    if !cap.enabled {
        return ProviderState::Disabled;
    }
    if !cap.installed {
        return ProviderState::Missing;
    }
    let worst = cap
        .issues
        .iter()
        .map(|f| f.level)
        .max()
        .unwrap_or(FindingLevel::Ok);
    if cap.inactive.is_some() || worst >= FindingLevel::Degraded {
        return ProviderState::Broken;
    }
    if worst >= FindingLevel::Warn || cap.update_available {
        return ProviderState::Attention;
    }
    ProviderState::Working
}

// ---------------------------------------------------------------------------
// Sidebar (Taphouse / Updatest source list)
// ---------------------------------------------------------------------------

/// A right-aligned count pill. Zero renders no badge at all: healthy is silent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Badge {
    pub count: usize,
    pub tone: Tone,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SidebarRow {
    /// The scope id this row selects: a group id or one of the `SCOPE_*`.
    pub scope: String,
    pub label: String,
    pub icon: Icon,
    pub badge: Option<Badge>,
    pub selected: bool,
    /// True when every provider in the group is disabled — the row stays,
    /// muted, because the owner turned it off and should still see it.
    pub dimmed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SidebarSection {
    /// The small gray eyebrow above the rows; `None` for the top group.
    pub title: Option<String>,
    pub rows: Vec<SidebarRow>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SidebarModel {
    pub sections: Vec<SidebarSection>,
    /// Footer strip text, e.g. "Checked just now" / "Checked 14 min ago" /
    /// "Checking…".
    pub footer: String,
}

/// Facts the sidebar needs that are not in the verdict.
#[derive(Debug, Clone, Default)]
pub struct SidebarFacts {
    pub scope: String,
    pub running_jobs: usize,
    pub project_count: usize,
    /// Time since the last detection pass finished; `None` before the first.
    pub freshness: Option<Duration>,
    pub detecting: bool,
}

/// Three sections: Overview alone at the top, one row per capability group
/// under "Capabilities" (taxonomy order), then Projects and Activity under
/// "Workspace". Badges: Overview carries the count of action items, a group
/// carries its own action items, Activity carries running jobs, Projects
/// carries nothing (a count of projects is not a call to action).
pub fn sidebar_model(
    groups: &[CapabilityGroup],
    verdict: &HealthVerdict,
    capabilities: &[CapabilityStatus],
    facts: &SidebarFacts,
) -> SidebarModel {
    let action_items: Vec<&AttentionItem> = verdict.action_items().collect();
    let badge_for = |count: usize, has_error: bool| -> Option<Badge> {
        (count > 0).then_some(Badge {
            count,
            tone: if has_error { Tone::Err } else { Tone::Warn },
        })
    };
    let overview_errors = action_items
        .iter()
        .any(|item| item.level == FindingLevel::Error);
    let overview = SidebarSection {
        title: None,
        rows: vec![SidebarRow {
            scope: SCOPE_OVERVIEW.to_string(),
            label: "Overview".to_string(),
            icon: Icon::LayoutDashboard,
            badge: badge_for(action_items.len(), overview_errors),
            selected: facts.scope == SCOPE_OVERVIEW,
            dimmed: false,
        }],
    };
    let groups_section = SidebarSection {
        title: Some("Capabilities".to_string()),
        rows: groups
            .iter()
            .map(|group| {
                let own: Vec<&&AttentionItem> = action_items
                    .iter()
                    .filter(|item| item.group_id == group.id)
                    .collect();
                let has_error = own.iter().any(|item| item.level == FindingLevel::Error);
                let members: Vec<&CapabilityStatus> = capabilities
                    .iter()
                    .filter(|cap| cap.capability == group.id)
                    .collect();
                let dimmed = !members.is_empty() && members.iter().all(|cap| !cap.enabled);
                SidebarRow {
                    scope: group.id.to_string(),
                    label: group.name.to_string(),
                    icon: group_icon(group.id),
                    badge: badge_for(own.len(), has_error),
                    selected: facts.scope == group.id,
                    dimmed,
                }
            })
            .collect(),
    };
    let workspace = SidebarSection {
        title: Some("Workspace".to_string()),
        rows: vec![
            SidebarRow {
                scope: SCOPE_PROJECTS.to_string(),
                label: "Projects".to_string(),
                icon: Icon::FolderKanban,
                badge: None,
                selected: facts.scope == SCOPE_PROJECTS,
                dimmed: false,
            },
            SidebarRow {
                scope: SCOPE_ACTIVITY.to_string(),
                label: "Activity".to_string(),
                icon: Icon::Activity,
                badge: (facts.running_jobs > 0).then_some(Badge {
                    count: facts.running_jobs,
                    tone: Tone::Accent,
                }),
                selected: facts.scope == SCOPE_ACTIVITY,
                dimmed: false,
            },
        ],
    };
    SidebarModel {
        sections: vec![overview, groups_section, workspace],
        footer: freshness_text(facts.freshness, facts.detecting),
    }
}

/// "Checked just now", "Checked 3 min ago", "Checked 2 h ago", or "Checking…"
/// while a pass is running and nothing has finished yet.
pub fn freshness_text(freshness: Option<Duration>, detecting: bool) -> String {
    match freshness {
        None if detecting => "Checking\u{2026}".to_string(),
        None => "Not checked yet".to_string(),
        Some(ago) => {
            let secs = ago.as_secs();
            if secs < 5 {
                "Checked just now".to_string()
            } else if secs < 60 {
                format!("Checked {secs}s ago")
            } else if secs < 3600 {
                format!("Checked {} min ago", secs / 60)
            } else {
                format!("Checked {} h ago", secs / 3600)
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Toolbar (page title + subtitle, the reference apps' unified toolbar)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolbarModel {
    pub title: String,
    /// One muted line under the title; empty renders nothing.
    pub subtitle: String,
}

/// The title pair for a scope: a group's name and its explainer, or the
/// reserved sections' fixed names. An unknown scope reads as the Overview,
/// the same fallback the shell uses for a scope that no longer resolves.
pub fn toolbar_model(
    scope: &str,
    groups: &[CapabilityGroup],
    verdict: &HealthVerdict,
) -> ToolbarModel {
    if let Some(group) = groups.iter().find(|g| g.id == scope) {
        return ToolbarModel {
            title: group.name.to_string(),
            subtitle: group.why.to_string(),
        };
    }
    match scope {
        SCOPE_PROJECTS => ToolbarModel {
            title: "Projects".to_string(),
            subtitle: "Repositories ADE is applied to".to_string(),
        },
        SCOPE_ACTIVITY => ToolbarModel {
            title: "Activity".to_string(),
            subtitle: "Install and update jobs".to_string(),
        },
        _ => ToolbarModel {
            title: "Overview".to_string(),
            subtitle: verdict.headline.clone(),
        },
    }
}

// ---------------------------------------------------------------------------
// Provider list (Taphouse Installed list)
// ---------------------------------------------------------------------------

/// The segmented control above the list (Homebrew's All / Formulae / Casks).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ProviderFilter {
    #[default]
    All,
    NeedsAttention,
    Installed,
}

impl ProviderFilter {
    pub const ALL: [ProviderFilter; 3] = [
        ProviderFilter::All,
        ProviderFilter::NeedsAttention,
        ProviderFilter::Installed,
    ];

    pub fn label(self) -> &'static str {
        match self {
            ProviderFilter::All => "All",
            ProviderFilter::NeedsAttention => "Needs attention",
            ProviderFilter::Installed => "Installed",
        }
    }
}

/// A small uppercase tag pill beside a name ("CASK", "DEPENDENCY" in Taphouse).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tag {
    pub label: String,
    pub tone: Tone,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderRow {
    pub id: String,
    pub name: String,
    pub description: String,
    /// Right-aligned version text; `None` renders nothing, never a dash.
    pub version: Option<String>,
    pub state: ProviderState,
    pub tags: Vec<Tag>,
    pub selected: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderListModel {
    pub title: String,
    /// One line under the title: the coverage summary for the group.
    pub subtitle: String,
    pub filter: ProviderFilter,
    /// Counts per filter in `ProviderFilter::ALL` order, for the segment labels.
    pub counts: [usize; 3],
    pub rows: Vec<ProviderRow>,
}

/// Rows for one group, in taxonomy order, filtered. Tags: the install method
/// as a neutral tag ("BREW", "CASK", "NPM", "PLUGIN", "MANUAL"), "UPDATE" in
/// accent when one is available, "OFF" muted when disabled, "RUNNING" in ok
/// when the process signature is live.
pub fn provider_list_model(
    group: &CapabilityGroup,
    capabilities: &[CapabilityStatus],
    verdict: &HealthVerdict,
    filter: ProviderFilter,
    selected: Option<&str>,
    query: &str,
) -> ProviderListModel {
    // The toolbar search narrows the population BEFORE the segment counts,
    // so "All 1 · Installed 1" always describes what the list can show.
    let needle = query.trim().to_lowercase();
    let members: Vec<&CapabilityStatus> = capabilities
        .iter()
        .filter(|cap| cap.capability == group.id)
        .filter(|cap| {
            needle.is_empty()
                || cap.name.to_lowercase().contains(&needle)
                || cap.id.to_lowercase().contains(&needle)
                || cap.description.to_lowercase().contains(&needle)
        })
        .collect();
    let matches = |cap: &CapabilityStatus, filter: ProviderFilter| match filter {
        ProviderFilter::All => true,
        ProviderFilter::NeedsAttention => matches!(
            provider_state(cap),
            ProviderState::Attention | ProviderState::Broken | ProviderState::Missing
        ),
        ProviderFilter::Installed => cap.installed,
    };
    let mut counts = [0usize; 3];
    for (slot, f) in ProviderFilter::ALL.iter().enumerate() {
        counts[slot] = members.iter().filter(|cap| matches(cap, *f)).count();
    }
    let rows = members
        .iter()
        .filter(|cap| matches(cap, filter))
        .map(|cap| {
            let mut tags = vec![Tag {
                label: method_tag(cap.method).to_string(),
                tone: Tone::Neutral,
            }];
            if cap.update_available {
                tags.push(Tag {
                    label: "UPDATE".to_string(),
                    tone: Tone::Accent,
                });
            }
            if !cap.enabled {
                tags.push(Tag {
                    label: "OFF".to_string(),
                    tone: Tone::Muted,
                });
            }
            if cap.running == Some(true) {
                tags.push(Tag {
                    label: "RUNNING".to_string(),
                    tone: Tone::Ok,
                });
            }
            ProviderRow {
                id: cap.id.clone(),
                name: cap.name.clone(),
                description: cap.description.clone(),
                version: cap.version.clone(),
                state: provider_state(cap),
                tags,
                selected: selected == Some(cap.id.as_str()),
            }
        })
        .collect();
    let subtitle = verdict
        .coverage
        .iter()
        .find(|row| row.group_id == group.id)
        .map(|row| row.summary.clone())
        .unwrap_or_default();
    ProviderListModel {
        title: group.name.to_string(),
        subtitle,
        filter,
        counts,
        rows,
    }
}

/// The uppercase tag text for an install method.
pub fn method_tag(method: LifecycleMethod) -> &'static str {
    match method {
        LifecycleMethod::Brew => "BREW",
        LifecycleMethod::BrewCask => "CASK",
        LifecycleMethod::Npm => "NPM",
        LifecycleMethod::ClaudePlugin => "PLUGIN",
        LifecycleMethod::Manual => "MANUAL",
    }
}

// ---------------------------------------------------------------------------
// Inspector (Updatest detail pane)
// ---------------------------------------------------------------------------

/// What a button in the inspector asks the engine to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderAction {
    Lifecycle(LifecycleAction),
    Enable,
    Disable,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActionButton {
    pub label: String,
    pub icon: Icon,
    pub action: ProviderAction,
    /// Red-tinted pill (Updatest's "Uninstall").
    pub destructive: bool,
    /// False while a job for this capability is running.
    pub enabled: bool,
}

/// One label-over-value pair inside a grouped card.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pair {
    pub label: String,
    pub value: String,
    /// Render as a link (homepage, path) rather than plain text.
    pub link: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Card {
    pub title: Option<String>,
    pub pairs: Vec<Pair>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IssueLine {
    pub tone: Tone,
    pub level_label: String,
    pub message: String,
    pub remediation: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InspectorModel {
    pub title: String,
    pub version: Option<String>,
    pub description: String,
    pub state: ProviderState,
    /// Pill row under the header, in display order; destructive last.
    pub actions: Vec<ActionButton>,
    /// "Information" card: Version, Latest, Install method, Location, Capability.
    pub cards: Vec<Card>,
    pub issues: Vec<IssueLine>,
    /// Guidance for manual installs, shown instead of an Install button.
    pub guidance: Option<String>,
}

/// The inspector for one capability. Action rules (moved here from the GUI
/// crate's `actions_for`): missing → Install (unless manual, then guidance);
/// installed → Update when `update_available`, Reinstall, Uninstall
/// (destructive); Enable/Disable always offered, mirroring `enabled`.
/// `busy` disables every button.
pub fn inspector_model(cap: &CapabilityStatus, busy: bool) -> InspectorModel {
    let automated = cap.method != LifecycleMethod::Manual;
    let mut actions = Vec::new();
    let mut push = |label: &str, icon: Icon, action: ProviderAction, destructive: bool| {
        actions.push(ActionButton {
            label: label.to_string(),
            icon,
            action,
            destructive,
            enabled: !busy,
        });
    };
    if !cap.installed {
        if automated {
            push(
                "Install",
                Icon::CloudDownload,
                ProviderAction::Lifecycle(LifecycleAction::Install),
                false,
            );
        }
    } else {
        if cap.update_available && automated {
            push(
                "Update",
                Icon::CircleArrowUp,
                ProviderAction::Lifecycle(LifecycleAction::Update),
                false,
            );
        }
        if automated {
            push(
                "Reinstall",
                Icon::RotateCw,
                ProviderAction::Lifecycle(LifecycleAction::Reinstall),
                false,
            );
        }
    }
    if cap.enabled {
        push("Disable", Icon::PowerOff, ProviderAction::Disable, false);
    } else {
        push("Enable", Icon::Power, ProviderAction::Enable, false);
    }
    if cap.installed && automated {
        push(
            "Uninstall",
            Icon::Trash2,
            ProviderAction::Lifecycle(LifecycleAction::Uninstall),
            true,
        );
    }

    let mut pairs = Vec::new();
    let mut pair = |label: &str, value: Option<String>, link: bool| {
        if let Some(value) = value {
            pairs.push(Pair {
                label: label.to_string(),
                value,
                link,
            });
        }
    };
    pair("Version", cap.version.clone(), false);
    pair("Latest", cap.latest_version.clone(), false);
    pair(
        "Install method",
        Some(method_name(cap.method).to_string()),
        false,
    );
    pair("Package", cap.pkg.clone(), false);
    pair("Location", cap.path.clone(), true);
    pair(
        "Capability",
        crate::gui::inventory::get_group(&cap.capability).map(|g| g.name.to_string()),
        false,
    );
    let cards = vec![Card {
        title: Some("Information".to_string()),
        pairs,
    }];

    let issues = cap
        .issues
        .iter()
        .map(|finding| IssueLine {
            tone: level_tone(finding.level),
            level_label: level_label(finding.level).to_string(),
            message: finding.message.clone(),
            remediation: finding.remediation.clone(),
        })
        .collect();

    InspectorModel {
        title: cap.name.clone(),
        version: cap.version.clone(),
        description: cap.description.clone(),
        state: provider_state(cap),
        actions,
        cards,
        issues,
        guidance: if cap.installed || automated {
            None
        } else {
            cap.guidance.clone()
        },
    }
}

/// Human-readable install method for the inspector card.
pub fn method_name(method: LifecycleMethod) -> &'static str {
    match method {
        LifecycleMethod::Brew => "Homebrew",
        LifecycleMethod::BrewCask => "Homebrew cask",
        LifecycleMethod::Npm => "npm",
        LifecycleMethod::ClaudePlugin => "Claude Code plugin",
        LifecycleMethod::Manual => "Manual",
    }
}

/// The tone a finding level renders in.
pub fn level_tone(level: FindingLevel) -> Tone {
    match level {
        FindingLevel::Error => Tone::Err,
        FindingLevel::Warn | FindingLevel::Degraded => Tone::Warn,
        FindingLevel::Info => Tone::Accent,
        FindingLevel::Ok => Tone::Ok,
    }
}

/// The uppercase level label for an issue line.
pub fn level_label(level: FindingLevel) -> &'static str {
    match level {
        FindingLevel::Error => "ERROR",
        FindingLevel::Warn => "WARN",
        FindingLevel::Degraded => "DEGRADED",
        FindingLevel::Info => "INFO",
        FindingLevel::Ok => "OK",
    }
}

// ---------------------------------------------------------------------------
// Fix panel (Homebrew Doctor right pane)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FixPanelModel {
    /// Small tinted eyebrow: "Warning" / "Error" / "Gap".
    pub eyebrow: String,
    pub tone: Tone,
    pub headline: String,
    pub why: String,
    pub note: Option<String>,
    /// The exact command(s) the Run button will execute, for the terminal
    /// block; `None` when the fix is manual.
    pub command: Option<String>,
    pub argvs: Vec<Vec<String>>,
    /// Manual guidance shown in place of the command block.
    pub guidance: Option<String>,
    /// Primary button label ("Install nono"); `None` when there is no
    /// automated action.
    pub run_label: Option<String>,
    pub capability_id: Option<String>,
    pub lifecycle: Option<LifecycleAction>,
}

/// The fix panel for one attention item. The command block shows exactly what
/// `action_argvs` would run — the same argv the job runner executes, so the
/// panel can never describe a different command from the one that runs.
pub fn fix_panel_model(item: &AttentionItem) -> FixPanelModel {
    let (eyebrow, tone) = match item.level {
        FindingLevel::Error => ("Error", Tone::Err),
        FindingLevel::Warn | FindingLevel::Degraded => ("Warning", Tone::Warn),
        FindingLevel::Info | FindingLevel::Ok => ("Note", Tone::Accent),
    };
    let mut model = FixPanelModel {
        eyebrow: eyebrow.to_string(),
        tone,
        headline: item.title.clone(),
        why: item.why.clone(),
        note: item.note.clone(),
        command: None,
        argvs: Vec::new(),
        guidance: None,
        run_label: None,
        capability_id: None,
        lifecycle: None,
    };
    let Some(action) = &item.action else {
        return model;
    };
    model.capability_id = Some(action.capability_id.clone());
    model.lifecycle = Some(action.lifecycle);
    if let Some(guidance) = &action.guidance {
        model.guidance = Some(guidance.clone());
        return model;
    }
    let argvs = crate::gui::inventory::get_capability(&action.capability_id)
        .and_then(|def| action_argvs(def, action.lifecycle))
        .unwrap_or_default();
    if argvs.is_empty() {
        return model;
    }
    model.command = Some(
        argvs
            .iter()
            .map(|argv| shell_display(argv))
            .collect::<Vec<_>>()
            .join(" && "),
    );
    model.argvs = argvs;
    model.run_label = Some(action.label.clone());
    model
}

/// A shell-readable rendering of one argv: arguments containing whitespace or
/// shell metacharacters are single-quoted. Display only; nothing executes
/// this string.
pub fn shell_display(argv: &[String]) -> String {
    argv.iter()
        .map(|arg| {
            let plain = !arg.is_empty()
                && arg
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || "-_./=:@+,%".contains(c));
            if plain {
                arg.clone()
            } else {
                format!("'{}'", arg.replace('\'', "'\\''"))
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

// ---------------------------------------------------------------------------
// Console drawer (Homebrew bottom console)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConsoleTab {
    pub job_id: String,
    /// "brew install nono" — the action and capability, not the job id.
    pub label: String,
    pub tone: Tone,
    pub active: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConsoleModel {
    /// Newest first, capped at `CONSOLE_TAB_LIMIT`.
    pub tabs: Vec<ConsoleTab>,
    /// The active job's log; empty when there is no job.
    pub lines: Vec<String>,
    /// "Running…", "Finished (exit 0)", "Failed (exit 1)", or "No jobs yet".
    pub status: String,
    pub status_tone: Tone,
}

pub const CONSOLE_TAB_LIMIT: usize = 8;

/// Tabs for the most recent jobs, the active one's log. When `active` names
/// no job, the newest job is active. Tone: running → accent, ok → ok,
/// error → err.
pub fn console_model(jobs: &[Job], active: Option<&str>) -> ConsoleModel {
    // Newest first: started_at is the tool's own sortable UTC stamp.
    let mut ordered: Vec<&Job> = jobs.iter().collect();
    ordered.sort_by(|a, b| b.started_at.cmp(&a.started_at));
    ordered.truncate(CONSOLE_TAB_LIMIT);
    let active_id = active
        .filter(|id| ordered.iter().any(|job| job.id == *id))
        .or(ordered.first().map(|job| job.id.as_str()))
        .map(str::to_string);
    let tone_of = |status: JobStatus| match status {
        JobStatus::Running => Tone::Accent,
        JobStatus::Ok => Tone::Ok,
        JobStatus::Error => Tone::Err,
    };
    let tabs = ordered
        .iter()
        .map(|job| ConsoleTab {
            job_id: job.id.clone(),
            label: job_label(job),
            tone: tone_of(job.status),
            active: active_id.as_deref() == Some(job.id.as_str()),
        })
        .collect();
    let active_job = ordered
        .iter()
        .find(|job| active_id.as_deref() == Some(job.id.as_str()));
    let (status, status_tone) = match active_job {
        None => ("No jobs yet".to_string(), Tone::Muted),
        Some(job) => match job.status {
            JobStatus::Running => ("Running\u{2026}".to_string(), Tone::Accent),
            JobStatus::Ok => (
                match job.exit_code {
                    Some(code) => format!("Finished (exit {code})"),
                    None => "Finished".to_string(),
                },
                Tone::Ok,
            ),
            JobStatus::Error => (
                match job.exit_code {
                    Some(code) => format!("Failed (exit {code})"),
                    None => "Failed".to_string(),
                },
                Tone::Err,
            ),
        },
    };
    ConsoleModel {
        tabs,
        lines: active_job.map(|job| job.log.clone()).unwrap_or_default(),
        status,
        status_tone,
    }
}

/// The one-line label for a job tab: "<action> <capability>" in lowercase,
/// e.g. "install nono", "update trufflehog".
pub fn job_label(job: &Job) -> String {
    format!("{} {}", job.action.to_lowercase(), job.capability_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gui::inventory::{get_capability, get_group, CAPABILITIES, CAPABILITY_GROUPS};
    use crate::gui::verdict::build_verdict;
    use crate::types::Finding;

    /// A healthy, installed, enabled status for a real capability id.
    fn status(id: &str) -> CapabilityStatus {
        let def = get_capability(id).expect("known capability");
        CapabilityStatus {
            id: def.id.to_string(),
            name: def.name.to_string(),
            kind: def.kind,
            capability: def.capability.to_string(),
            description: def.description.to_string(),
            method: def.method,
            pkg: def.pkg.map(str::to_string),
            guidance: def.guidance.map(str::to_string),
            installed: true,
            path: Some(format!("/opt/homebrew/bin/{}", def.id)),
            version: Some("1.2.3".to_string()),
            reports_version: true,
            inactive: None,
            running: None,
            enabled: true,
            latest_version: None,
            update_available: false,
            issues: Vec::new(),
        }
    }

    fn all_healthy() -> Vec<CapabilityStatus> {
        CAPABILITIES.iter().map(|def| status(def.id)).collect()
    }

    fn find<'a>(caps: &'a mut [CapabilityStatus], id: &str) -> &'a mut CapabilityStatus {
        caps.iter_mut().find(|c| c.id == id).expect("present")
    }

    fn job(id: &str, cap: &str, action: &str, status: JobStatus, started: &str) -> Job {
        Job {
            id: id.to_string(),
            capability_id: cap.to_string(),
            action: action.to_string(),
            status,
            started_at: started.to_string(),
            finished_at: None,
            exit_code: match status {
                JobStatus::Running => None,
                JobStatus::Ok => Some(0),
                JobStatus::Error => Some(1),
            },
            log: vec![format!("{action} {cap}")],
        }
    }

    // ── provider_state ──────────────────────────────────────────────

    #[test]
    fn disabled_wins_over_every_other_state() {
        let mut cap = status("nono");
        cap.enabled = false;
        cap.installed = false;
        cap.issues.push(Finding {
            level: FindingLevel::Error,
            message: "broken".into(),
            remediation: None,
        });
        assert_eq!(provider_state(&cap), ProviderState::Disabled);
        assert_eq!(ProviderState::Disabled.icon(), Icon::CircleOff);
    }

    #[test]
    fn missing_broken_attention_and_working_are_distinct() {
        let mut cap = status("nono");
        assert_eq!(provider_state(&cap), ProviderState::Working);
        cap.update_available = true;
        assert_eq!(provider_state(&cap), ProviderState::Attention);
        cap.update_available = false;
        cap.inactive = Some("switched off in config".into());
        assert_eq!(provider_state(&cap), ProviderState::Broken);
        cap.inactive = None;
        cap.issues.push(Finding {
            level: FindingLevel::Degraded,
            message: "version probe failed".into(),
            remediation: None,
        });
        assert_eq!(provider_state(&cap), ProviderState::Broken);
        cap.issues.clear();
        cap.installed = false;
        assert_eq!(provider_state(&cap), ProviderState::Missing);
        assert_eq!(ProviderState::Missing.tone(), Tone::Muted);
    }

    // ── sidebar ─────────────────────────────────────────────────────

    #[test]
    fn sidebar_has_three_sections_in_taxonomy_order_and_badges_only_where_needed() {
        let mut caps = all_healthy();
        let hole = find(&mut caps, "nono");
        hole.installed = false;
        hole.version = None;
        for cap in caps
            .iter_mut()
            .filter(|c| c.capability == "token-efficiency")
        {
            cap.enabled = false;
        }
        let verdict = build_verdict(&caps);
        let facts = SidebarFacts {
            scope: "sandboxing".to_string(),
            running_jobs: 2,
            project_count: 3,
            freshness: Some(Duration::from_secs(125)),
            detecting: false,
        };
        let model = sidebar_model(&CAPABILITY_GROUPS, &verdict, &caps, &facts);
        assert_eq!(model.sections.len(), 3);
        assert_eq!(model.sections[0].title, None);
        assert_eq!(model.sections[1].title.as_deref(), Some("Capabilities"));
        assert_eq!(model.sections[2].title.as_deref(), Some("Workspace"));

        let overview = &model.sections[0].rows[0];
        assert_eq!(overview.scope, SCOPE_OVERVIEW);
        let expected = verdict.action_items().count();
        assert!(expected > 0, "the hole must be an action item");
        assert_eq!(overview.badge.as_ref().map(|b| b.count), Some(expected));

        let group_rows = &model.sections[1].rows;
        assert_eq!(group_rows.len(), CAPABILITY_GROUPS.len());
        for (row, group) in group_rows.iter().zip(CAPABILITY_GROUPS.iter()) {
            assert_eq!(row.scope, group.id);
            assert_eq!(row.icon, group_icon(group.id));
        }
        let sandboxing = group_rows.iter().find(|r| r.scope == "sandboxing").unwrap();
        assert!(sandboxing.selected);
        assert_eq!(sandboxing.badge.as_ref().map(|b| b.count), Some(1));
        let secret = group_rows
            .iter()
            .find(|r| r.scope == "secret-scanning")
            .unwrap();
        assert_eq!(secret.badge, None, "healthy is silent");
        let off = group_rows
            .iter()
            .find(|r| r.scope == "token-efficiency")
            .unwrap();
        assert!(off.dimmed, "an all-disabled group is dimmed, not hidden");

        let workspace = &model.sections[2].rows;
        assert_eq!(workspace[0].scope, SCOPE_PROJECTS);
        assert_eq!(
            workspace[0].badge, None,
            "a project count is not a call to action"
        );
        assert_eq!(workspace[1].scope, SCOPE_ACTIVITY);
        assert_eq!(
            workspace[1].badge,
            Some(Badge {
                count: 2,
                tone: Tone::Accent
            })
        );
        assert_eq!(model.footer, "Checked 2 min ago");
    }

    #[test]
    fn overview_badge_goes_red_when_an_action_item_is_an_error() {
        let mut caps = all_healthy();
        let broken = find(&mut caps, "openwiki");
        broken.issues.push(Finding {
            level: FindingLevel::Error,
            message: "version probe failed".into(),
            remediation: None,
        });
        let verdict = build_verdict(&caps);
        let model = sidebar_model(
            &CAPABILITY_GROUPS,
            &verdict,
            &caps,
            &SidebarFacts::default(),
        );
        let badge = model.sections[0].rows[0].badge.clone();
        assert_eq!(badge.map(|b| b.tone), Some(Tone::Err));
    }

    #[test]
    fn freshness_text_is_coarse_and_honest() {
        assert_eq!(freshness_text(None, true), "Checking\u{2026}");
        assert_eq!(freshness_text(None, false), "Not checked yet");
        assert_eq!(
            freshness_text(Some(Duration::from_secs(3)), false),
            "Checked just now"
        );
        assert_eq!(
            freshness_text(Some(Duration::from_secs(45)), false),
            "Checked 45s ago"
        );
        assert_eq!(
            freshness_text(Some(Duration::from_secs(7200)), true),
            "Checked 2 h ago"
        );
    }

    // ── provider list ───────────────────────────────────────────────

    #[test]
    fn provider_list_filters_counts_and_tags_follow_the_status() {
        let mut caps = all_healthy();
        let group = get_group("secret-scanning").unwrap();
        let gitleaks = find(&mut caps, "gitleaks");
        gitleaks.installed = false;
        gitleaks.version = None;
        let trufflehog = find(&mut caps, "trufflehog");
        trufflehog.update_available = true;
        trufflehog.running = Some(true);
        let verdict = build_verdict(&caps);

        let all = provider_list_model(
            group,
            &caps,
            &verdict,
            ProviderFilter::All,
            Some("gitleaks"),
            "",
        );
        assert_eq!(all.title, "Secret Scanning");
        assert_eq!(all.rows.len(), 2);
        assert_eq!(all.counts, [2, 2, 1]);
        assert_eq!(all.rows[0].id, "trufflehog", "taxonomy order");
        assert!(all.rows[1].selected);
        assert_eq!(
            all.rows[1].version, None,
            "an absent version renders as absent"
        );
        let labels: Vec<&str> = all.rows[0].tags.iter().map(|t| t.label.as_str()).collect();
        assert_eq!(labels, vec!["BREW", "UPDATE", "RUNNING"]);
        assert_eq!(all.rows[0].tags[1].tone, Tone::Accent);
        assert_eq!(all.rows[0].state, ProviderState::Attention);
        assert_eq!(
            all.subtitle,
            verdict
                .coverage
                .iter()
                .find(|r| r.group_id == "secret-scanning")
                .unwrap()
                .summary
        );

        let installed =
            provider_list_model(group, &caps, &verdict, ProviderFilter::Installed, None, "");
        assert_eq!(installed.rows.len(), 1);
        assert_eq!(installed.rows[0].id, "trufflehog");
        assert!(!installed.rows[0].selected);

        let off = find(&mut caps, "gitleaks");
        off.enabled = false;
        let verdict = build_verdict(&caps);
        let attention = provider_list_model(
            group,
            &caps,
            &verdict,
            ProviderFilter::NeedsAttention,
            None,
            "",
        );
        assert_eq!(
            attention.rows.len(),
            1,
            "a disabled provider is the owner's call, not attention"
        );
        let all = provider_list_model(group, &caps, &verdict, ProviderFilter::All, None, "");
        assert!(all.rows[1]
            .tags
            .iter()
            .any(|t| t.label == "OFF" && t.tone == Tone::Muted));
    }

    #[test]
    fn method_tags_cover_every_method() {
        assert_eq!(method_tag(LifecycleMethod::BrewCask), "CASK");
        assert_eq!(method_tag(LifecycleMethod::ClaudePlugin), "PLUGIN");
        assert_eq!(method_name(LifecycleMethod::Npm), "npm");
        assert_eq!(method_name(LifecycleMethod::Manual), "Manual");
    }

    // ── inspector ───────────────────────────────────────────────────

    fn action_labels(model: &InspectorModel) -> Vec<String> {
        model.actions.iter().map(|a| a.label.clone()).collect()
    }

    #[test]
    fn a_missing_automated_provider_offers_install_and_enable_state() {
        let mut cap = status("nono");
        cap.installed = false;
        cap.version = None;
        cap.path = None;
        let model = inspector_model(&cap, false);
        assert_eq!(action_labels(&model), vec!["Install", "Disable"]);
        assert_eq!(
            model.actions[0].action,
            ProviderAction::Lifecycle(LifecycleAction::Install)
        );
        assert!(!model.actions[0].destructive);
        assert_eq!(model.guidance, None);
        assert_eq!(model.version, None);
        let labels: Vec<&str> = model.cards[0]
            .pairs
            .iter()
            .map(|p| p.label.as_str())
            .collect();
        assert_eq!(labels, vec!["Install method", "Package", "Capability"]);
    }

    #[test]
    fn a_missing_manual_provider_shows_guidance_instead_of_a_button() {
        let mut cap = status("ocean");
        cap.installed = false;
        let model = inspector_model(&cap, false);
        assert_eq!(action_labels(&model), vec!["Disable"]);
        assert!(model.guidance.is_some());
    }

    #[test]
    fn an_installed_provider_with_an_update_offers_update_reinstall_uninstall() {
        let mut cap = status("trufflehog");
        cap.update_available = true;
        cap.latest_version = Some("2.0.0".into());
        cap.enabled = false;
        cap.issues.push(Finding {
            level: FindingLevel::Warn,
            message: "old".into(),
            remediation: Some("update it".into()),
        });
        let model = inspector_model(&cap, false);
        assert_eq!(
            action_labels(&model),
            vec!["Update", "Reinstall", "Enable", "Uninstall"]
        );
        let uninstall = model.actions.last().unwrap();
        assert!(uninstall.destructive);
        assert_eq!(
            uninstall.action,
            ProviderAction::Lifecycle(LifecycleAction::Uninstall)
        );
        let pairs = &model.cards[0].pairs;
        assert!(pairs
            .iter()
            .any(|p| p.label == "Latest" && p.value == "2.0.0"));
        assert!(pairs.iter().any(|p| p.label == "Location" && p.link));
        assert_eq!(model.issues.len(), 1);
        assert_eq!(model.issues[0].level_label, "WARN");
        assert_eq!(model.issues[0].tone, Tone::Warn);
        assert_eq!(model.issues[0].remediation.as_deref(), Some("update it"));
    }

    #[test]
    fn busy_disables_every_button() {
        let model = inspector_model(&status("rtk"), true);
        assert!(!model.actions.is_empty());
        assert!(model.actions.iter().all(|a| !a.enabled));
    }

    #[test]
    fn level_tones_and_labels_cover_the_ladder() {
        assert_eq!(level_tone(FindingLevel::Error), Tone::Err);
        assert_eq!(level_tone(FindingLevel::Degraded), Tone::Warn);
        assert_eq!(level_tone(FindingLevel::Info), Tone::Accent);
        assert_eq!(level_tone(FindingLevel::Ok), Tone::Ok);
        assert_eq!(level_label(FindingLevel::Degraded), "DEGRADED");
        assert_eq!(level_label(FindingLevel::Ok), "OK");
        assert_eq!(level_label(FindingLevel::Info), "INFO");
    }

    // ── fix panel ───────────────────────────────────────────────────

    #[test]
    fn the_fix_panel_shows_exactly_the_argv_the_runner_would_execute() {
        let mut caps = all_healthy();
        let hole = find(&mut caps, "nono");
        hole.installed = false;
        hole.version = None;
        let verdict = build_verdict(&caps);
        let item = verdict
            .action_items()
            .find(|i| i.group_id == "sandboxing")
            .expect("the gap is an action item");
        let panel = fix_panel_model(item);
        assert_eq!(panel.eyebrow, "Warning");
        assert_eq!(panel.tone, Tone::Warn);
        assert_eq!(panel.headline, item.title);
        let def = get_capability("nono").unwrap();
        let expected = action_argvs(def, LifecycleAction::Install).unwrap();
        assert_eq!(panel.argvs, expected);
        assert_eq!(
            panel.command.as_deref(),
            Some(
                expected
                    .iter()
                    .map(|a| shell_display(a))
                    .collect::<Vec<_>>()
                    .join(" && ")
                    .as_str()
            )
        );
        assert_eq!(
            panel.run_label.as_deref(),
            Some(item.action.as_ref().unwrap().label.as_str())
        );
        assert_eq!(panel.capability_id.as_deref(), Some("nono"));
        assert_eq!(panel.lifecycle, Some(LifecycleAction::Install));
        assert_eq!(panel.guidance, None);
    }

    #[test]
    fn a_manual_gap_shows_guidance_and_no_command() {
        let mut caps = all_healthy();
        for id in ["cocoindex", "ccc", "serena"] {
            let cap = find(&mut caps, id);
            cap.installed = false;
            cap.version = None;
        }
        let verdict = build_verdict(&caps);
        let item = verdict
            .action_items()
            .find(|i| i.group_id == "semantic-search")
            .expect("the gap is an action item");
        let panel = fix_panel_model(item);
        assert!(panel.guidance.is_some());
        assert_eq!(panel.command, None);
        assert!(panel.argvs.is_empty());
        assert_eq!(panel.run_label, None);
    }

    #[test]
    fn a_broken_provider_is_an_error_eyebrow() {
        let mut caps = all_healthy();
        let broken = find(&mut caps, "openwiki");
        broken.issues.push(Finding {
            level: FindingLevel::Error,
            message: "version probe failed".into(),
            remediation: None,
        });
        let verdict = build_verdict(&caps);
        let item = verdict
            .action_items()
            .find(|i| i.capability_id == "openwiki")
            .expect("broken is an action item");
        let panel = fix_panel_model(item);
        assert_eq!(panel.eyebrow, "Error");
        assert_eq!(panel.tone, Tone::Err);
    }

    #[test]
    fn shell_display_quotes_only_what_needs_it() {
        let argv: Vec<String> = ["brew", "install", "--cask", "my app", "it's"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        assert_eq!(
            shell_display(&argv),
            "brew install --cask 'my app' 'it'\\''s'"
        );
        assert_eq!(shell_display(&[String::new()]), "''");
    }

    // ── console ─────────────────────────────────────────────────────

    #[test]
    fn console_picks_the_newest_job_when_nothing_is_active() {
        let jobs = vec![
            job(
                "j1",
                "nono",
                "Install",
                JobStatus::Ok,
                "2026-09-21T10:00:00Z",
            ),
            job(
                "j2",
                "rtk",
                "Update",
                JobStatus::Running,
                "2026-09-21T11:00:00Z",
            ),
            job(
                "j3",
                "gitleaks",
                "Uninstall",
                JobStatus::Error,
                "2026-09-21T09:00:00Z",
            ),
        ];
        let model = console_model(&jobs, None);
        let ids: Vec<&str> = model.tabs.iter().map(|t| t.job_id.as_str()).collect();
        assert_eq!(ids, vec!["j2", "j1", "j3"], "newest first");
        assert!(model.tabs[0].active);
        assert_eq!(model.tabs[0].label, "update rtk");
        assert_eq!(model.tabs[0].tone, Tone::Accent);
        assert_eq!(model.tabs[2].tone, Tone::Err);
        assert_eq!(model.lines, vec!["Update rtk".to_string()]);
        assert_eq!(model.status, "Running\u{2026}");

        let model = console_model(&jobs, Some("j3"));
        assert!(model.tabs[2].active);
        assert_eq!(model.status, "Failed (exit 1)");
        assert_eq!(model.status_tone, Tone::Err);

        let model = console_model(&jobs, Some("missing"));
        assert!(
            model.tabs[0].active,
            "an unknown id falls back to the newest job"
        );

        let model = console_model(&jobs, Some("j1"));
        assert_eq!(model.status, "Finished (exit 0)");
        assert_eq!(model.status_tone, Tone::Ok);
    }

    #[test]
    fn console_without_jobs_says_so_and_caps_its_tabs() {
        let empty = console_model(&[], None);
        assert!(empty.tabs.is_empty());
        assert!(empty.lines.is_empty());
        assert_eq!(empty.status, "No jobs yet");
        assert_eq!(empty.status_tone, Tone::Muted);

        let many: Vec<Job> = (0..12)
            .map(|i| {
                job(
                    &format!("j{i}"),
                    "nono",
                    "Install",
                    JobStatus::Ok,
                    &format!("2026-09-21T{:02}:00:00Z", i),
                )
            })
            .collect();
        let model = console_model(&many, None);
        assert_eq!(model.tabs.len(), CONSOLE_TAB_LIMIT);
        assert_eq!(model.tabs[0].job_id, "j11");
    }

    #[test]
    fn the_search_query_narrows_rows_before_the_counts() {
        let caps = all_healthy();
        let group = get_group("secret-scanning").unwrap();
        let verdict = build_verdict(&caps);
        let hit = provider_list_model(
            group,
            &caps,
            &verdict,
            ProviderFilter::All,
            None,
            " Truffle ",
        );
        assert_eq!(hit.rows.len(), 1);
        assert_eq!(hit.rows[0].id, "trufflehog");
        assert_eq!(hit.counts, [1, 0, 1]);
        let by_description = provider_list_model(
            group,
            &caps,
            &verdict,
            ProviderFilter::All,
            None,
            "optional",
        );
        assert_eq!(by_description.rows[0].id, "gitleaks");
        let none = provider_list_model(group, &caps, &verdict, ProviderFilter::All, None, "zzz");
        assert!(none.rows.is_empty());
        assert_eq!(none.counts, [0, 0, 0]);
    }

    #[test]
    fn toolbar_titles_follow_the_scope() {
        let caps = all_healthy();
        let verdict = build_verdict(&caps);
        let group = toolbar_model("sandboxing", &CAPABILITY_GROUPS, &verdict);
        assert_eq!(group.title, "Execution Sandboxing");
        assert_eq!(group.subtitle, get_group("sandboxing").unwrap().why);
        assert_eq!(
            toolbar_model(SCOPE_PROJECTS, &CAPABILITY_GROUPS, &verdict).title,
            "Projects"
        );
        assert_eq!(
            toolbar_model(SCOPE_ACTIVITY, &CAPABILITY_GROUPS, &verdict).title,
            "Activity"
        );
        let overview = toolbar_model("no-such-scope", &CAPABILITY_GROUPS, &verdict);
        assert_eq!(overview.title, "Overview");
        assert_eq!(overview.subtitle, verdict.headline);
    }

    #[test]
    fn every_icon_name_is_lowercase_kebab() {
        for icon in Icon::ALL {
            let name = icon.lucide_name();
            assert!(
                name.chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-'),
                "{name}"
            );
        }
        assert_eq!(group_icon("no-such-group"), Icon::Circle);
    }
}
