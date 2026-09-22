//! The root component: state, data flow, and the three-pane composition.
//!
//! Every display decision comes from `ade_core::gui::views`; this file holds
//! the signals those models are built from and routes clicks back into them.
//! It deliberately owns no engine — the contender READS the machine once at
//! launch and never writes to it (see `Inspector`'s action handling below),
//! because a bake-off entry that could install software is a bake-off entry
//! nobody can safely run twice.

use crate::console::Console;
use crate::inspector::Inspector;
use crate::provider_list::ProviderList;
use crate::sidebar::Sidebar;
use crate::styles::stylesheet;
use ade_core::exec::{real_exec, real_which};
use ade_core::gui::inventory::{
    detect_capabilities, CapabilityStatus, DetectOptions, CAPABILITY_GROUPS,
};
use ade_core::gui::jobs::{Job, JobRunner, JobStatus};
use ade_core::gui::state::{ade_home_from_env, SCOPE_ACTIVITY, SCOPE_OVERVIEW, SCOPE_PROJECTS};
use ade_core::gui::verdict::build_verdict;
use ade_core::gui::views::{
    console_model, inspector_model, provider_list_model, sidebar_model, ProviderAction,
    ProviderFilter, SidebarFacts,
};
use dioxus::prelude::*;
use std::collections::{BTreeMap, BTreeSet};
use std::time::{Duration, Instant};

/// How long a single capability probe may take before it is abandoned — the
/// same five seconds the egui twin's detection pass allows.
const PROBE_TIMEOUT: Duration = Duration::from_secs(5);

/// The line a non-group scope shows. Only the capability group page is in the
/// bake-off; the other scopes exist so the sidebar's selection is real.
fn placeholder_for(scope: &str) -> &'static str {
    match scope {
        SCOPE_OVERVIEW => "Overview (not part of the bake-off)",
        SCOPE_PROJECTS => "Projects (not part of the bake-off)",
        SCOPE_ACTIVITY => "Activity (not part of the bake-off)",
        _ => "Unknown section",
    }
}

/// Run detection once, off the UI thread. Mirrors `data.rs`'s call: real
/// `exec`/`which`, nothing disabled, no cached latest versions, process
/// signatures probed.
fn detect() -> Vec<CapabilityStatus> {
    let disabled = BTreeSet::new();
    let last_jobs = BTreeMap::new();
    let latest_versions = BTreeMap::new();
    detect_capabilities(&DetectOptions {
        exec: real_exec(),
        which: real_which(),
        disabled: &disabled,
        last_jobs: &last_jobs,
        latest_versions: &latest_versions,
        probe_running: true,
        probe_timeout: PROBE_TIMEOUT,
    })
}

/// Job history already on disk (`$ADE_HOME/jobs.json`). `JobRunner::new`
/// resumes persisted history, so this is a read: the contender never calls
/// `start`, and the console shows real past runs rather than invented ones.
fn load_jobs() -> Vec<Job> {
    JobRunner::new(real_exec(), Some(ade_home_from_env())).list()
}

#[component]
pub fn App() -> Element {
    // The sheet is generated once — it embeds a ~900 KB font, so rebuilding
    // it every render would be the single most expensive thing on screen.
    let css = use_hook(stylesheet);

    // `use_signal_sync` is the cross-thread signal: the detection thread
    // below writes it directly, which is why detection needs no channel and
    // no polling loop.
    let capabilities = use_signal_sync(Vec::<CapabilityStatus>::new);
    let detecting = use_signal_sync(|| true);
    let finished_at = use_signal_sync(|| Option::<Instant>::None);

    use_hook(|| {
        let mut capabilities = capabilities;
        let mut detecting = detecting;
        let mut finished_at = finished_at;
        std::thread::spawn(move || {
            let found = detect();
            capabilities.set(found);
            finished_at.set(Some(Instant::now()));
            detecting.set(false);
        });
    });

    let jobs = use_hook(load_jobs);

    // The bake-off screen IS the capability group page, so the app opens on
    // the first group rather than on the Overview the shipping app restores.
    let mut scope = use_signal(|| CAPABILITY_GROUPS[0].id.to_string());
    let mut selected_provider = use_signal(|| Option::<String>::None);
    let mut filter = use_signal(ProviderFilter::default);
    let mut console_tab = use_signal(|| Option::<String>::None);
    let mut console_hidden = use_signal(|| false);
    let mut last_action = use_signal(|| Option::<String>::None);

    let caps_guard = capabilities.read();
    let caps: &[CapabilityStatus] = &caps_guard;
    let verdict = build_verdict(caps);

    let running_jobs = jobs
        .iter()
        .filter(|job| job.status == JobStatus::Running)
        .count();
    let facts = SidebarFacts {
        scope: scope(),
        running_jobs,
        // Project registration is a different surface; the bake-off screen
        // never shows a project count, so reporting zero is honest here.
        project_count: 0,
        freshness: finished_at().map(|at| at.elapsed()),
        detecting: detecting(),
    };
    let sidebar = sidebar_model(&CAPABILITY_GROUPS, &verdict, caps, &facts);

    let current_scope = scope();
    let group = CAPABILITY_GROUPS
        .iter()
        .find(|group| group.id == current_scope);

    // The selection must belong to THIS group; otherwise the first provider is
    // shown, so the inspector is never empty while the list has rows.
    let in_group: Vec<&str> = match group {
        Some(group) => caps
            .iter()
            .filter(|cap| cap.capability == group.id)
            .map(|cap| cap.id.as_str())
            .collect(),
        None => Vec::new(),
    };
    let selected: Option<String> = selected_provider
        .read()
        .as_deref()
        .filter(|id| in_group.contains(id))
        .or(in_group.first().copied())
        .map(str::to_string);

    let list = group
        .map(|group| provider_list_model(group, caps, &verdict, filter(), selected.as_deref()));
    let inspected = selected
        .as_deref()
        .and_then(|id| caps.iter().find(|cap| cap.id == id))
        // No job this contender started can be in flight, so nothing is busy.
        .map(|cap| (cap.id.clone(), inspector_model(cap, false)));

    let show_console = !jobs.is_empty() && !console_hidden();
    let console = console_model(&jobs, console_tab.read().as_deref());
    let placeholder = placeholder_for(&current_scope);

    rsx! {
        style { "{css}" }
        div { class: "app",
            Sidebar {
                model: sidebar,
                on_select: move |next: String| {
                    scope.set(next);
                    last_action.set(None);
                },
            }
            div { class: "content",
                div { class: "panes",
                    if let Some(list) = list {
                        ProviderList {
                            model: list,
                            on_select: move |id: String| {
                                selected_provider.set(Some(id));
                                last_action.set(None);
                            },
                            on_filter: move |next: ProviderFilter| filter.set(next),
                        }
                    } else {
                        div { class: "placeholder", "{placeholder}" }
                    }
                    if let Some((id, model)) = inspected {
                        Inspector {
                            model,
                            last_action: last_action(),
                            on_action: move |action: ProviderAction| {
                                last_action.set(Some(describe(action, &id)));
                            },
                        }
                    }
                }
                if show_console {
                    Console {
                        model: console,
                        on_tab: move |id: String| console_tab.set(Some(id)),
                        // NOTE: neither `dioxus-desktop` 0.7.10 nor
                        // `dioxus-native` 0.7.10 exposes a clipboard API, and
                        // `navigator.clipboard` is unavailable on the custom
                        // protocol origin the webview serves from. Copy is a
                        // no-op in this contender rather than a silent lie.
                        on_copy: move |_| {},
                        on_collapse: move |_| console_hidden.set(true),
                    }
                }
            }
        }
    }
}

/// What a button WOULD have run, in the same words the console tabs use.
/// Nothing executes: see the module header.
fn describe(action: ProviderAction, capability_id: &str) -> String {
    let verb = match action {
        ProviderAction::Lifecycle(lifecycle) => lifecycle.as_str(),
        ProviderAction::Enable => "enable",
        ProviderAction::Disable => "disable",
    };
    format!("Would run: {verb} {capability_id}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use ade_core::gui::inventory::LifecycleAction;

    /// The caption names the verb and the capability, matching the console's
    /// own "install nono" phrasing — the owner reads one vocabulary.
    #[test]
    fn a_click_reports_the_command_it_would_have_run() {
        assert_eq!(
            describe(ProviderAction::Lifecycle(LifecycleAction::Install), "nono"),
            "Would run: install nono"
        );
        assert_eq!(
            describe(
                ProviderAction::Lifecycle(LifecycleAction::Uninstall),
                "nono"
            ),
            "Would run: uninstall nono"
        );
        assert_eq!(
            describe(ProviderAction::Enable, "trufflehog"),
            "Would run: enable trufflehog"
        );
        assert_eq!(
            describe(ProviderAction::Disable, "trufflehog"),
            "Would run: disable trufflehog"
        );
    }

    /// Every reserved scope has its own line, and a group id falls through to
    /// the real page rather than a placeholder.
    #[test]
    fn non_group_scopes_each_get_their_own_placeholder() {
        let lines = [
            placeholder_for(SCOPE_OVERVIEW),
            placeholder_for(SCOPE_PROJECTS),
            placeholder_for(SCOPE_ACTIVITY),
        ];
        let unique: std::collections::HashSet<&&str> = lines.iter().collect();
        assert_eq!(unique.len(), lines.len(), "placeholders collide: {lines:?}");
        assert_eq!(
            placeholder_for(SCOPE_OVERVIEW),
            "Overview (not part of the bake-off)"
        );
    }
}
