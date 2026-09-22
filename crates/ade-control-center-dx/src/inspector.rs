//! The inspector — the right pane of a capability group page. Renders
//! `ade_core::gui::views::InspectorModel`; every label, value and button in it
//! was decided in core.
//!
//! Anatomy, from the reference: a 40px tinted state tile beside a large title
//! with the version under it and a one-line description; a row of pill
//! buttons (neutral, neutral, destructive tinted red); an uppercase heading
//! over a panel-filled rounded card of label-over-value pairs in two columns;
//! then any issues, and for manual installs the guidance in place of an
//! Install button.
//!
//! Buttons here mutate NOTHING. This contender exists to be looked at, and
//! running a real install from a bake-off entry is out of scope — a click
//! reports the command it would have run and stops.

use crate::icons::glyph;
use crate::styles::{tone_fill_class, tone_ink_class};
use ade_core::gui::views::{ActionButton, Card, InspectorModel, IssueLine, ProviderAction};
use dioxus::prelude::*;

#[component]
pub fn Inspector(
    model: InspectorModel,
    last_action: Option<String>,
    on_action: EventHandler<ProviderAction>,
) -> Element {
    let tone = model.state.tone();
    let tile_class = format!(
        "glyph insp-tile {} {}",
        tone_fill_class(tone),
        tone_ink_class(tone)
    );
    let mark = glyph(model.state.icon());
    let version = model.version.clone().unwrap_or_default();
    let guidance = model.guidance.clone().unwrap_or_default();
    let last = last_action.clone().unwrap_or_default();
    rsx! {
        aside { class: "inspector", "aria-label": "Inspector",
            div { class: "insp-head",
                span { class: "{tile_class}", "aria-hidden": "true", "{mark}" }
                div { class: "insp-titles",
                    div { class: "insp-title", "{model.title}" }
                    if model.version.is_some() {
                        div { class: "insp-version", "{version}" }
                    }
                }
            }
            div { class: "insp-desc", "{model.description}" }
            div { class: "actions",
                for button in model.actions.iter() {
                    ActionPill { key: "{button.label}", button: button.clone(), on_action }
                }
            }
            if model.guidance.is_some() {
                div { class: "guidance", "{guidance}" }
            }
            // What a click WOULD have done. The bake-off compares rendering,
            // so the action is reported rather than performed.
            if last_action.is_some() {
                div { class: "last-action", "{last}" }
            }
            for (index , card) in model.cards.iter().enumerate() {
                InfoCard { key: "{index}", card: card.clone() }
            }
            if !model.issues.is_empty() {
                div { class: "card-heading", "ISSUES" }
                for (index , issue) in model.issues.iter().enumerate() {
                    IssueRow { key: "{index}", issue: issue.clone() }
                }
            }
        }
    }
}

#[component]
fn ActionPill(button: ActionButton, on_action: EventHandler<ProviderAction>) -> Element {
    let class = if button.destructive {
        "pill-btn destructive"
    } else {
        "pill-btn"
    };
    let action = button.action;
    rsx! {
        button {
            class: "{class}",
            disabled: !button.enabled,
            "aria-label": "{button.label} provider",
            onclick: move |_| on_action.call(action),
            "{button.label}"
        }
    }
}

#[component]
fn InfoCard(card: Card) -> Element {
    let title = card.title.as_deref().unwrap_or_default().to_uppercase();
    rsx! {
        if card.title.is_some() {
            div { class: "card-heading", "{title}" }
        }
        div { class: "card",
            for (index , pair) in card.pairs.iter().enumerate() {
                PairCell { key: "{index}", pair: pair.clone() }
            }
        }
    }
}

#[component]
fn PairCell(pair: ade_core::gui::views::Pair) -> Element {
    rsx! {
        div { class: "pair",
            div { class: "pair-label", "{pair.label}" }
            // A link pair is an anchor in the "this is clickable" role colour
            // — the same `info` token the egui twin paints these with. It
            // navigates nowhere: the bake-off must not open a browser.
            if pair.link {
                a { class: "pair-value link", "aria-label": "Open {pair.label}", "{pair.value}" }
            }
            if !pair.link {
                div { class: "pair-value", "{pair.value}" }
            }
        }
    }
}

#[component]
fn IssueRow(issue: IssueLine) -> Element {
    let level_class = format!("issue-level {}", tone_ink_class(issue.tone));
    let remediation = issue.remediation.clone().unwrap_or_default();
    rsx! {
        div { class: "issue",
            span { class: "{level_class}", "{issue.level_label}" }
            span { class: "issue-message", "{issue.message}" }
            if issue.remediation.is_some() {
                span { class: "issue-fix", "{remediation}" }
            }
        }
    }
}
