//! The console drawer along the bottom. Renders
//! `ade_core::gui::views::ConsoleModel`: one tab per recent job with a status
//! dot and a monospace command label, the run status on the right, Copy, a
//! chevron that collapses the drawer, and the active job's log in a sunken
//! monospace area pinned to its newest line.

use crate::icons::glyph;
use crate::styles::{tone_fill_class, tone_ink_class};
use ade_core::gui::views::{ConsoleModel, ConsoleTab, Icon};
use dioxus::prelude::*;

#[component]
pub fn Console(
    model: ConsoleModel,
    on_tab: EventHandler<String>,
    on_copy: EventHandler<()>,
    on_collapse: EventHandler<()>,
) -> Element {
    let status_class = format!("console-status {}", tone_ink_class(model.status_tone));
    let chevron = glyph(Icon::ChevronRight);
    let last = model.lines.len().saturating_sub(1);
    rsx! {
        section { class: "console", "aria-label": "Console",
            div { class: "console-bar",
                div { class: "console-tabs", role: "tablist", "aria-label": "Jobs",
                    for tab in model.tabs.iter() {
                        ConsoleTabButton { key: "{tab.job_id}", tab: tab.clone(), on_tab }
                    }
                }
                span { class: "{status_class}", "{model.status}" }
                button {
                    class: "ghost-btn",
                    "aria-label": "Copy console log",
                    onclick: move |_| on_copy.call(()),
                    "Copy"
                }
                button {
                    class: "glyph icon-btn",
                    "aria-label": "Hide console",
                    onclick: move |_| on_collapse.call(()),
                    "{chevron}"
                }
            }
            div { class: "console-body mono", role: "log",
                if model.lines.is_empty() {
                    div { class: "console-line console-empty", "No output yet." }
                }
                for (index , line) in model.lines.iter().enumerate() {
                    ConsoleLine { key: "{index}", line: line.clone(), last: index == last }
                }
            }
        }
    }
}

#[component]
fn ConsoleTabButton(tab: ConsoleTab, on_tab: EventHandler<String>) -> Element {
    // The active tab is filled with the tone's tint; the dot carries the
    // tone's ink in both states, so a backgrounded job still shows its status.
    let class = if tab.active {
        format!("ctab active {}", tone_fill_class(tab.tone))
    } else {
        "ctab".to_string()
    };
    let dot_class = format!("dot {}", tone_ink_class(tab.tone));
    let job_id = tab.job_id.clone();
    rsx! {
        button {
            class: "{class}",
            role: "tab",
            "aria-selected": "{tab.active}",
            "aria-label": "Console tab {tab.label}",
            onclick: move |_| on_tab.call(job_id.clone()),
            span { class: "{dot_class}", "aria-hidden": "true" }
            span { class: "mono", "{tab.label}" }
        }
    }
}

/// One log line. The newest line asks to be scrolled into view as it mounts,
/// which is how the drawer stays pinned to the bottom as a job streams —
/// there is no imperative scroll position to keep in sync with the model.
#[component]
fn ConsoleLine(line: String, last: bool) -> Element {
    rsx! {
        div {
            class: "console-line",
            onmounted: move |event| {
                if last {
                    spawn(async move {
                        let _ = event.data().scroll_to(ScrollBehavior::Instant).await;
                    });
                }
            },
            "{line}"
        }
    }
}
