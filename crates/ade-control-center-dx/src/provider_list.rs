//! The middle pane of a capability group page: the provider list. Renders
//! `ade_core::gui::views::ProviderListModel` and reports what was clicked;
//! nothing here decides what a row says.
//!
//! Anatomy, from the reference: a title with a one-line subtitle, a segmented
//! filter carrying counts, then 48px rows with a 28px rounded status tile on
//! the left, a semibold name followed by small uppercase tag pills, a muted
//! description line, and a right-aligned monospace version. Rows separate
//! with a hairline inset past the tile; the selected row is a neutral wash (a
//! blue field here would fight the status tints in the tiles).

use crate::icons::glyph;
use crate::styles::{tone_fill_class, tone_ink_class};
use ade_core::gui::views::{ProviderFilter, ProviderListModel, ProviderRow};
use dioxus::prelude::*;

/// The empty-state line per filter — the same three sentences the egui twin's
/// `empty_text` returns.
fn empty_text(filter: ProviderFilter) -> &'static str {
    match filter {
        ProviderFilter::All => "No providers in this capability.",
        ProviderFilter::NeedsAttention => "Nothing needs attention here.",
        ProviderFilter::Installed => "Nothing installed yet.",
    }
}

#[component]
pub fn ProviderList(
    model: ProviderListModel,
    on_select: EventHandler<String>,
    on_filter: EventHandler<ProviderFilter>,
) -> Element {
    let last = model.rows.len().saturating_sub(1);
    rsx! {
        section { class: "middle",
            h1 { class: "page-title", "{model.title}" }
            div { class: "page-subtitle", "{model.subtitle}" }
            div { class: "segmented", role: "tablist", "aria-label": "Filter",
                for (index , filter) in ProviderFilter::ALL.iter().enumerate() {
                    FilterSegment {
                        key: "{index}",
                        filter: *filter,
                        count: model.counts[index],
                        active: *filter == model.filter,
                        on_filter,
                    }
                }
            }
            div { class: "rows", role: "listbox", "aria-label": "Providers",
                if model.rows.is_empty() {
                    div { class: "empty", "{empty_text(model.filter)}" }
                }
                for (index , row) in model.rows.iter().enumerate() {
                    ProviderRowView {
                        key: "{row.id}",
                        row: row.clone(),
                        hairline: index < last,
                        on_select,
                    }
                }
            }
        }
    }
}

/// One segment of the filter control. Labels read "All 7" — the filter's own
/// name and its count, so switching filters never resizes the control's text
/// into a different shape.
#[component]
fn FilterSegment(
    filter: ProviderFilter,
    count: usize,
    active: bool,
    on_filter: EventHandler<ProviderFilter>,
) -> Element {
    let class = if active { "segment active" } else { "segment" };
    let label = filter.label();
    rsx! {
        button {
            class: "{class}",
            role: "tab",
            "aria-selected": "{active}",
            "aria-label": "Filter {label}",
            onclick: move |_| on_filter.call(filter),
            "{label} {count}"
        }
    }
}

#[component]
fn ProviderRowView(row: ProviderRow, hairline: bool, on_select: EventHandler<String>) -> Element {
    let class = if row.selected {
        "prow selected"
    } else {
        "prow"
    };
    let tone = row.state.tone();
    let tile_class = format!(
        "glyph tile {} {}",
        tone_fill_class(tone),
        tone_ink_class(tone)
    );
    let mark = glyph(row.state.icon());
    let version = row.version.clone().unwrap_or_default();
    let id = row.id.clone();
    rsx! {
        button {
            class: "{class}",
            role: "option",
            "aria-selected": "{row.selected}",
            "aria-label": "{row.name}",
            onclick: move |_| on_select.call(id.clone()),
            span { class: "{tile_class}", "aria-hidden": "true", "{mark}" }
            span { class: "prow-text",
                span { class: "prow-line1",
                    span { class: "prow-name", "{row.name}" }
                    for (index , tag) in row.tags.iter().enumerate() {
                        TagPill { key: "{index}", tag: tag.clone() }
                    }
                }
                span { class: "prow-desc", "{row.description}" }
            }
            // `None` renders nothing at all, never a dash.
            if row.version.is_some() {
                span { class: "prow-version mono", "{version}" }
            }
        }
        if hairline {
            div { class: "hairline" }
        }
    }
}

#[component]
fn TagPill(tag: ade_core::gui::views::Tag) -> Element {
    let class = format!(
        "tag {} {}",
        tone_fill_class(tag.tone),
        tone_ink_class(tag.tone)
    );
    rsx! {
        span { class: "{class}", "{tag.label}" }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The three empty states are distinct sentences — a filter that showed
    /// the wrong one would tell the owner nothing is installed when in fact
    /// nothing merely needs attention.
    #[test]
    fn each_filter_has_its_own_empty_state() {
        let all: Vec<&str> = ProviderFilter::ALL.iter().map(|f| empty_text(*f)).collect();
        let unique: std::collections::HashSet<&&str> = all.iter().collect();
        assert_eq!(unique.len(), all.len(), "empty states collide: {all:?}");
        assert!(all.iter().all(|s| s.ends_with('.')));
    }
}
