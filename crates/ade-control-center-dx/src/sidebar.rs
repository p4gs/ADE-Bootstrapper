//! The source-list sidebar. Renders `ade_core::gui::views::SidebarModel` and
//! nothing else — which rows exist, what they say, which badge they carry and
//! whether they are selected is all decided in core, exactly as it is for the
//! egui twin.
//!
//! Anatomy, from the reference screenshots: a small uppercase eyebrow per
//! section; 28px rows with a 16px Lucide icon in the accent colour, a 13px
//! label, and a right-aligned count pill; the selected row is a filled accent
//! capsule with white icon, label and pill; a faint footer line at the bottom.
//!
//! Every row is a real `<button role="option">` carrying `aria-selected` and
//! an `aria-label` equal to the egui `WidgetInfo` label (the row's label), so
//! `interceptor macos tree` sees the same thing in both contenders.

use crate::icons::glyph;
use crate::styles::{tone_fill_class, tone_ink_class};
use ade_core::gui::views::{SidebarModel, SidebarRow};
use dioxus::prelude::*;

#[component]
pub fn Sidebar(model: SidebarModel, on_select: EventHandler<String>) -> Element {
    rsx! {
        nav { class: "sidebar", role: "navigation",
            div { class: "sidebar-scroll", role: "listbox", "aria-label": "Sections",
                for (index , section) in model.sections.iter().enumerate() {
                    SidebarSection {
                        key: "{index}",
                        section: section.clone(),
                        first: index == 0,
                        on_select,
                    }
                }
            }
            div { class: "sidebar-footer", "{model.footer}" }
        }
    }
}

#[component]
fn SidebarSection(
    section: ade_core::gui::views::SidebarSection,
    first: bool,
    on_select: EventHandler<String>,
) -> Element {
    // The eyebrow is uppercased here rather than by CSS alone so the string
    // the accessibility tree reports matches what is painted.
    let title = section.title.as_deref().unwrap_or_default().to_uppercase();
    let eyebrow_class = if first { "eyebrow first" } else { "eyebrow" };
    rsx! {
        if section.title.is_some() {
            div { class: "{eyebrow_class}", "{title}" }
        }
        for row in section.rows.iter() {
            NavRow { key: "{row.scope}", row: row.clone(), on_select }
        }
    }
}

#[component]
fn NavRow(row: SidebarRow, on_select: EventHandler<String>) -> Element {
    let mut class = String::from("nav-row");
    if row.selected {
        class.push_str(" selected");
    }
    if row.dimmed {
        class.push_str(" dimmed");
    }
    let mark = glyph(row.icon);
    let scope = row.scope.clone();
    // A zero count renders no badge at all: healthy is silent (core already
    // encodes that by handing us `None`, but the tone classes are ours).
    let badge_class = row
        .badge
        .as_ref()
        .map(|b| {
            format!(
                "pill {} {}",
                tone_fill_class(b.tone),
                tone_ink_class(b.tone)
            )
        })
        .unwrap_or_default();
    let badge_count = row.badge.as_ref().map(|b| b.count).unwrap_or_default();
    rsx! {
        button {
            class: "{class}",
            role: "option",
            "aria-selected": "{row.selected}",
            "aria-label": "{row.label}",
            onclick: move |_| on_select.call(scope.clone()),
            span { class: "glyph nav-icon", "aria-hidden": "true", "{mark}" }
            span { class: "nav-label", "{row.label}" }
            if row.badge.is_some() {
                span { class: "{badge_class}", "{badge_count}" }
            }
        }
    }
}
