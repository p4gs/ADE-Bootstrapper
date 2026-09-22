//! The modal trust surfaces (Phase D): the coverage-consequence confirm
//! dialog and the bulk-install preview checklist.
//!
//! Both are pure views on the same contract as every other module here: they
//! draw, and report what the owner did. The shell holds the pending state and
//! applies the events. Both render through one painted surface —
//! `RADIUS_POPOVER` plus the four-layer `modal_shadows` recipe, painted by
//! hand because `egui::Frame` carries only a single shadow.

use crate::app::{ax_button, ax_ghost_button, row_hairline};
use crate::overview::{animated_count, plural};
use crate::theme;
use ade_core::gui::inventory::{get_capability, CapabilityStatus, LifecycleAction};
use ade_core::gui::verdict::{removal_consequence, BulkCandidate};
use egui::{CornerRadius, RichText, Stroke, StrokeKind};
use std::collections::HashSet;

/// What the owner did in the confirm dialog.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ConfirmEvent {
    /// Run the destructive action that was pending.
    Confirm,
    /// Close without doing anything (button, backdrop, or Escape).
    Cancel,
}

/// What the owner did in the bulk-install preview.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum BulkEvent {
    /// Flip one candidate in or out of the selection.
    Toggle(String),
    /// Install the current selection.
    Confirm,
    Cancel,
}

/// The default selection when the preview opens: every automatable candidate.
/// Manual-method candidates are listed but never selectable, so they start
/// (and stay) out.
pub(crate) fn default_selection(candidates: &[BulkCandidate]) -> HashSet<String> {
    candidates
        .iter()
        .filter(|candidate| candidate.automatable())
        .map(|candidate| candidate.capability_id.clone())
        .collect()
}

/// The ids a confirm would actually install: the selection intersected with
/// the automatable candidates, in taxonomy order. A stale id (its capability
/// got installed while the modal was open) drops out; a manual id can never
/// slip through even if the set somehow contains one.
pub(crate) fn confirmed_ids(
    candidates: &[BulkCandidate],
    selected: &HashSet<String>,
) -> Vec<String> {
    candidates
        .iter()
        .filter(|candidate| candidate.automatable() && selected.contains(&candidate.capability_id))
        .map(|candidate| candidate.capability_id.clone())
        .collect()
}

/// One modal surface: `egui::Modal` for the backdrop/input-blocking/Escape
/// mechanics, with the visual surface painted by hand — `Modal`'s default
/// frame carries a single shadow, and the design calls for the four-layer
/// modal stack. The shapes go into a slot reserved BEFORE the content lays
/// out, so they paint behind it.
///
/// Entrance (item 9): the app's highest-stakes surfaces used to pop in at
/// full opacity on their very first frame. `open` eases 0→1 over
/// `DURATION_ENTRANCE` starting the frame the modal's `id` first exists —
/// egui returns the target instantly on an id's first-ever query, so this
/// still needs to run every frame the modal is open (which it always is:
/// nothing here ever asks `animate_bool_with_time` for `false`) for the ease
/// to actually play rather than skip straight to 1.0.
fn show_modal<T>(
    ctx: &egui::Context,
    id: egui::Id,
    p: &theme::Palette,
    width: f32,
    content: impl FnOnce(&mut egui::Ui) -> T,
) -> (T, bool) {
    let open = ctx.animate_bool_with_time(id.with("open"), true, theme::DURATION_ENTRANCE);
    let backdrop_color = theme::alpha_scaled(p.scrim, open);
    let response = egui::Modal::new(id)
        .frame(egui::Frame::new())
        .backdrop_color(backdrop_color)
        .show(ctx, |ui| {
            let surface = ui.painter().add(egui::Shape::Noop);
            let inner = egui::Frame::new()
                .inner_margin(theme::margin(theme::Space::S20, theme::Space::S16))
                .show(ui, |ui| {
                    ui.set_width(width);
                    ui.spacing_mut().item_spacing.y = theme::Space::S8.px();
                    content(ui)
                });
            // Scale about the panel's own center — a pop-in, not a slide.
            let full_rect = inner.response.rect;
            let rect = egui::Rect::from_center_size(
                full_rect.center(),
                full_rect.size() * egui::lerp(0.96..=1.0, open),
            );
            let radius = CornerRadius::same(theme::RADIUS_POPOVER);
            let mut shapes: Vec<egui::Shape> = theme::modal_shadows(ui.visuals().dark_mode)
                .iter()
                .map(|shadow| {
                    let mut shadow = *shadow;
                    // Pure black — gamma_multiply only touches alpha here.
                    shadow.color = shadow.color.gamma_multiply(open);
                    egui::Shape::from(shadow.as_shape(rect, radius))
                })
                .collect();
            shapes.push(
                egui::epaint::RectShape::new(
                    rect,
                    radius,
                    theme::alpha_scaled(p.panel, open),
                    Stroke::new(1.0, theme::alpha_scaled(p.line, open)),
                    StrokeKind::Inside,
                )
                .into(),
            );
            ui.painter().set(surface, egui::Shape::Vec(shapes));
            inner.inner
        });
    let close = response.should_close();
    (response.inner, close)
}

/// The destructive-action confirm dialog. The title and the destructive
/// button both name the full target ("Uninstall TruffleHog" — never a bare
/// "Uninstall"), and the body states the coverage consequence computed by
/// ade-core, so confirming is an informed act rather than a reflex.
pub(crate) fn confirm_modal(
    ctx: &egui::Context,
    p: &theme::Palette,
    capabilities: &[CapabilityStatus],
    capability_id: &str,
    action: LifecycleAction,
) -> Option<ConfirmEvent> {
    let name = capabilities
        .iter()
        .find(|cap| cap.id == capability_id)
        .map(|cap| cap.name.clone())
        .or_else(|| get_capability(capability_id).map(|def| def.name.to_string()))
        .unwrap_or_else(|| capability_id.to_string());
    let title = format!("{} {name}", crate::app::capitalize(action));
    // Only removal has a coverage consequence to state; anything else renders
    // no body rather than an invented one.
    let consequence = match action {
        LifecycleAction::Uninstall => removal_consequence(capabilities, capability_id),
        _ => String::new(),
    };
    let mut event = None;
    let (_, close) = show_modal(
        ctx,
        egui::Id::new(("confirm-modal", capability_id)),
        p,
        theme::MODAL_CONFIRM_WIDTH,
        |ui| {
            ui.label(RichText::new(&title).font(theme::semibold(theme::SIZE_SECTION)));
            if !consequence.is_empty() {
                ui.label(
                    RichText::new(&consequence)
                        .color(p.muted)
                        .size(theme::SIZE_BODY),
                );
            }
            ui.add_space(theme::Space::S8.px());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                // Right-to-left: the destructive action holds the trailing
                // (macOS-primary) slot, Cancel sits to its left.
                if ax_button(
                    ui,
                    &title,
                    &format!("Confirm {} {capability_id}", action.as_str()),
                    Some(p.err),
                    true,
                )
                .clicked()
                {
                    event = Some(ConfirmEvent::Confirm);
                }
                if ax_ghost_button(
                    ui,
                    "Cancel",
                    &format!("Cancel {} {capability_id}", action.as_str()),
                    p.muted,
                )
                .clicked()
                {
                    event = Some(ConfirmEvent::Cancel);
                }
            });
        },
    );
    if close && event.is_none() {
        event = Some(ConfirmEvent::Cancel);
    }
    event
}

/// The bulk-install preview: a deselectable checklist of everything missing,
/// each row stating the exact command a confirm would run and the capability
/// it fixes. Manual-method rows are present but disabled — the list is honest
/// about its scope instead of quietly narrowing it.
pub(crate) fn bulk_modal(
    ctx: &egui::Context,
    p: &theme::Palette,
    candidates: &[BulkCandidate],
    selected: &HashSet<String>,
) -> Option<BulkEvent> {
    let total = candidates
        .iter()
        .filter(|candidate| candidate.automatable())
        .count();
    let chosen = confirmed_ids(candidates, selected).len();
    let mut event = None;
    let (_, close) = show_modal(
        ctx,
        egui::Id::new("bulk-install-modal"),
        p,
        theme::MODAL_LIST_WIDTH,
        |ui| {
            ui.label(
                RichText::new(format!(
                    "Install {total} {}",
                    plural(total, "tool", "tools")
                ))
                .font(theme::semibold(theme::SIZE_SECTION)),
            );
            ui.add_space(theme::Space::S4.px());
            egui::ScrollArea::vertical()
                .id_salt("bulk-install-list")
                .max_height(360.0)
                .show(ui, |ui| {
                    for (index, candidate) in candidates.iter().enumerate() {
                        if index > 0 {
                            row_hairline(ui, p);
                        }
                        if let Some(found) = bulk_row(ui, p, candidate, selected) {
                            event = Some(found);
                        }
                    }
                });
            ui.add_space(theme::Space::S8.px());
            egui::containers::Sides::new().show(
                ui,
                |ui| {
                    // The displayed count animates on change but always
                    // settles on — and acts on — the true value.
                    let shown = animated_count(ui, "bulk-selected-count", chosen);
                    ui.label(
                        RichText::new(format!("{shown} selected"))
                            .color(p.muted)
                            .size(theme::SIZE_CAPTION),
                    );
                },
                |ui| {
                    let shown = animated_count(ui, "bulk-selected-count", chosen);
                    if ax_button(
                        ui,
                        &format!("Install {shown} {}", plural(shown, "tool", "tools")),
                        "Confirm bulk install",
                        Some(p.accent),
                        chosen > 0,
                    )
                    .clicked()
                    {
                        event = Some(BulkEvent::Confirm);
                    }
                    if ax_ghost_button(ui, "Cancel", "Cancel bulk install", p.muted).clicked() {
                        event = Some(BulkEvent::Cancel);
                    }
                },
            );
        },
    );
    if close && event.is_none() {
        event = Some(BulkEvent::Cancel);
    }
    event
}

/// One checklist row: checkbox · name over its real install command · the
/// group it fixes. A manual-method candidate renders the same shape, disabled
/// and unchecked, with "manual install" where the command would be.
fn bulk_row(
    ui: &mut egui::Ui,
    p: &theme::Palette,
    candidate: &BulkCandidate,
    selected: &HashSet<String>,
) -> Option<BulkEvent> {
    let automatable = candidate.automatable();
    let mut event = None;
    egui::containers::Sides::new()
        .height(theme::ROW_HEIGHT)
        .shrink_left()
        .show(
            ui,
            |ui| {
                let mut on = automatable && selected.contains(&candidate.capability_id);
                // The macOS checkbox idiom (ISC-311): `ax_checkbox` — an
                // accent box with a white check when on, a bordered box when
                // off. See its doc for why stock `egui::Checkbox` could not
                // be restyled to do this.
                let response = crate::app::ax_checkbox(
                    ui,
                    &mut on,
                    automatable,
                    p,
                    &format!("Select {}", candidate.capability_id),
                );
                // ax_checkbox carries the WidgetInfo itself.
                if response.clicked() {
                    event = Some(BulkEvent::Toggle(candidate.capability_id.clone()));
                }
                // Centered as one unit in the row band (ISC-311) — the
                // nested-vertical layout top-aligned this block, the same
                // class the Overview attention rows had.
                let caption = match &candidate.command {
                    Some(command) => RichText::new(command)
                        .color(p.muted)
                        .font(egui::FontId::monospace(theme::SIZE_CAPTION)),
                    None => RichText::new("manual install")
                        .color(p.disabled_text)
                        .size(theme::SIZE_CAPTION),
                };
                crate::app::centered_text_block(
                    ui,
                    RichText::new(&candidate.name).color(if automatable {
                        p.text
                    } else {
                        p.disabled_text
                    }),
                    theme::semibold(theme::SIZE_BODY),
                    Some(caption),
                    None,
                );
            },
            |ui| {
                ui.label(
                    RichText::new(&candidate.group_name)
                        .color(p.muted)
                        .size(theme::SIZE_CAPTION),
                );
            },
        );
    event
}

#[cfg(test)]
mod tests {
    use super::*;

    fn candidate(id: &str, command: Option<&str>) -> BulkCandidate {
        BulkCandidate {
            capability_id: id.to_string(),
            name: id.to_string(),
            group_name: "Group".to_string(),
            command: command.map(String::from),
        }
    }

    /// The selection-state contract, pure: automatable candidates start
    /// selected, manual ones never enter, and a confirm returns exactly the
    /// selected ∩ automatable set in candidate order.
    #[test]
    fn selection_state_admits_only_automatable_candidates() {
        let candidates = vec![
            candidate("gitleaks", Some("brew install gitleaks")),
            candidate("ocean", None),
            candidate("nono", Some("brew install nono")),
        ];
        let selection = default_selection(&candidates);
        assert_eq!(
            selection,
            HashSet::from(["gitleaks".to_string(), "nono".to_string()]),
            "manual candidates start (and stay) unselected"
        );
        assert_eq!(
            confirmed_ids(&candidates, &selection),
            vec!["gitleaks".to_string(), "nono".to_string()]
        );

        // Deselecting one narrows the confirm to what remains.
        let mut narrowed = selection.clone();
        narrowed.remove("gitleaks");
        assert_eq!(
            confirmed_ids(&candidates, &narrowed),
            vec!["nono".to_string()]
        );

        // A manual id in the set (impossible via the UI, but the writer is
        // the last line of defence) never reaches the confirm; neither does a
        // stale id whose candidate has disappeared.
        let mut poisoned = selection;
        poisoned.insert("ocean".to_string());
        poisoned.insert("no-longer-missing".to_string());
        assert_eq!(
            confirmed_ids(&candidates, &poisoned),
            vec!["gitleaks".to_string(), "nono".to_string()]
        );
        assert!(confirmed_ids(&candidates, &HashSet::new()).is_empty());
    }
}
