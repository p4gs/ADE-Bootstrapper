//! One capability row, rendered as a pure function of its inputs.
//!
//! The row changes nothing: it draws, and reports what the owner did. That is
//! what lets it be rendered in a test with no engine, no background threads and
//! no window — and therefore what lets a layout change be *looked at* before it
//! ships. Every visual defect in this app so far reached the screen because
//! nobody saw the frame first.

use crate::app::{
    ax_button, ax_ghost_button, cap_dot, capability_label, chip, status_dot_at, worst_level, Dot,
};
use crate::theme;
use ade_core::gui::inventory::{
    get_group, short_version, CapabilityStatus, LifecycleAction, LifecycleMethod,
};
use egui::RichText;

/// What the owner did in this row. The caller decides what it means.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RowEvent {
    SetEnabled(bool),
    /// Run this action now.
    Start(LifecycleAction),
    /// Destructive — the shell opens the coverage-consequence confirm modal;
    /// the row itself never runs a destructive action.
    AskConfirm(LifecycleAction),
    ToggleIssues,
}

/// Everything the row needs to draw itself.
pub(crate) struct RowState<'a> {
    pub cap: &'a CapabilityStatus,
    /// A job is already running for this capability.
    pub busy: bool,
    /// False under a capability heading, which already names the group.
    pub show_capability: bool,
    pub issues_open: bool,
}

pub(crate) fn capability_row(
    ui: &mut egui::Ui,
    p: &theme::Palette,
    state: &RowState<'_>,
) -> Option<RowEvent> {
    let cap = state.cap;
    let mut event = None;
    // A capability the owner switched off should recede, not vanish. Eases
    // toward the app's own disabled role over DURATION_HOVER instead of an
    // instant opacity blend — the blend read visibly BOLDER (5.37:1) than
    // every other "off" text in the app (`p.disabled_text`, 3.4:1 by design:
    // disabled text is deliberately exempt from AA).
    let disabled_t = ui.ctx().animate_bool_with_time(
        egui::Id::new(("row-disabled", &cap.id)),
        !cap.enabled,
        theme::DURATION_HOVER,
    );
    let title_color = p.text.lerp_to_gamma(p.disabled_text, disabled_t);
    let description_color = p.muted.lerp_to_gamma(p.faint_text, disabled_t);
    // Reserve a slot to paint the hover background BEHIND the row's content.
    let backdrop = ui.painter().add(egui::Shape::Noop);
    // `Sides` centres both clusters at a fixed height and pins the trailing
    // group to the edge — the hand-computed column constants this replaces
    // were the actual cause of the horizontal voids in every wide window.
    let inner = ui.scope(|ui| {
        egui::containers::Sides::new()
            .height(theme::ROW_HEIGHT)
            .shrink_left()
            .show(
                ui,
                |ui| {
                    // Healthy is silent: the glyph slot is reserved so names
                    // align, but pigment is spent only where something is
                    // wrong. A green dot on sixteen healthy rows spends the
                    // whole attention budget saying "nothing is wrong".
                    let (rect, _) =
                        ui.allocate_exact_size(egui::vec2(12.0, 12.0), egui::Sense::hover());
                    match cap_dot(cap) {
                        Dot::Ok | Dot::Disabled => {}
                        dot => status_dot_at(ui, rect, dot, p),
                    }
                    ui.vertical(|ui| {
                        ui.spacing_mut().item_spacing.y = theme::Space::S2.px();
                        ui.label(
                            RichText::new(&cap.name)
                                .color(title_color)
                                .font(theme::semibold(theme::SIZE_BODY)),
                        );
                        // One line, always. The full text is a hover away, and
                        // the row keeps its height so the list holds its rhythm.
                        ui.add(
                            egui::Label::new(
                                RichText::new(&cap.description)
                                    .color(description_color)
                                    .size(theme::SIZE_CAPTION),
                            )
                            .truncate(),
                        )
                        .on_hover_text(&cap.description);
                    });
                },
                |ui| {
                    // Laid right-to-left: the overflow menu holds the edge,
                    // then the one action this row's state actually calls
                    // for, then the version fact.
                    if let Some(found) = row_menu(ui, p, state) {
                        event = Some(found);
                    }
                    if let Some(found) = primary_action(ui, p, state) {
                        event = Some(found);
                    }
                    version_block(ui, p, cap);
                    if cap.running == Some(true) {
                        chip(ui, "running", p.ok);
                    }
                    if !cap.issues.is_empty() {
                        let count = cap.issues.len();
                        // No glyph prefix: the tinted count is the signal and
                        // the expansion below is its own open/closed state.
                        let text = if state.issues_open {
                            format!("Hide {count} issue{}", if count == 1 { "" } else { "s" })
                        } else {
                            format!("{count} issue{}", if count == 1 { "" } else { "s" })
                        };
                        if ax_ghost_button(
                            ui,
                            &text,
                            &format!("Issues {}", cap.id),
                            crate::app::level_color(p, worst_level(&cap.issues)),
                        )
                        .clicked()
                        {
                            event = Some(RowEvent::ToggleIssues);
                        }
                    }
                    if state.show_capability {
                        if let Some(group) = get_group(&cap.capability) {
                            capability_label(ui, p, group.name, group.why);
                        }
                    }
                },
            );
    });
    // A list you can point at should acknowledge the pointer. Painted after
    // layout because the row's rect is only known once it has been laid out.
    // The wash fades in and out (~100ms) — motion that says "this row heard
    // you", nothing else moves.
    let rect = inner.response.rect;
    let wash = ui.ctx().animate_bool_with_time(
        egui::Id::new(("row-hover-wash", &cap.id)),
        ui.rect_contains_pointer(rect),
        0.1,
    );
    if wash > 0.0 {
        ui.painter().set(
            backdrop,
            egui::epaint::RectShape::filled(
                rect.expand2(egui::vec2(8.0, 0.0)),
                egui::CornerRadius::same(theme::RADIUS_CONTROL),
                p.row_hover.gamma_multiply(wash),
            ),
        );
    }
    event
}

/// The version fact, one line, right-aligned ahead of the actions. Monospace
/// digits so a refresh never makes the column breathe. NEVER a `ui.vertical`:
/// inside a right-to-left layout a vertical claims the entire remaining
/// width and crushes everything to its left.
fn version_block(ui: &mut egui::Ui, p: &theme::Palette, cap: &CapabilityStatus) {
    match cap.short_version() {
        Some(version) => {
            let full = cap.version.clone().unwrap_or_default();
            let text = match (&cap.latest_version, cap.update_available) {
                (Some(latest), true) => format!("{version} → {}", short_version(latest)),
                _ => version,
            };
            let tint = if cap.update_available {
                p.info
            } else {
                p.muted
            };
            ui.label(
                RichText::new(text)
                    .color(tint)
                    .font(egui::FontId::monospace(theme::SIZE_CAPTION)),
            )
            .on_hover_text(full);
        }
        None => {
            ui.label(
                RichText::new(if cap.installed {
                    "installed"
                } else {
                    "not installed"
                })
                .color(p.muted)
                .size(theme::SIZE_CAPTION),
            );
        }
    }
}

/// Exactly one emphasised action — the one the row's state calls for. The
/// busy state occupies the same slot so the row never grows sideways.
/// Destructive confirmation is NOT here: `AskConfirm` bubbles to the shell,
/// which opens the coverage-consequence modal.
fn primary_action(ui: &mut egui::Ui, p: &theme::Palette, state: &RowState<'_>) -> Option<RowEvent> {
    let cap = state.cap;
    if state.busy {
        ui.add(egui::Spinner::new().size(14.0).color(p.muted));
        ui.label(
            RichText::new("working…")
                .color(p.muted)
                .size(theme::SIZE_CAPTION),
        );
        return None;
    }
    if cap.method == LifecycleMethod::Manual {
        if !cap.installed {
            if let Some(guidance) = cap.guidance.clone() {
                // Nothing packages this one, so the honest offer is the
                // instructions — not a button that cannot work.
                ax_ghost_button(
                    ui,
                    "How to install",
                    &format!("How to install {}", cap.id),
                    p.info,
                )
                .on_hover_text(guidance);
            }
        }
        return None;
    }
    let primary = if !cap.installed {
        Some(LifecycleAction::Install)
    } else if cap.update_available {
        Some(LifecycleAction::Update)
    } else {
        None
    };
    let action = primary?;
    let visible = crate::app::capitalize(action);
    if ax_button(
        ui,
        visible,
        &format!("{visible} {}", cap.id),
        Some(p.accent),
        true,
    )
    .clicked()
    {
        return Some(RowEvent::Start(action));
    }
    None
}

/// The overflow menu. Uninstall and Reinstall stop appearing eighteen times;
/// they live one deliberate click away, disabled rather than absent when not
/// applicable, so the menu's shape (and its AccessKit tree) never changes.
/// The trigger is the PAINTED ellipsis (Phase F) — the "…" text glyph sat on
/// a text baseline and read as typography, not as a control.
fn row_menu(ui: &mut egui::Ui, p: &theme::Palette, state: &RowState<'_>) -> Option<RowEvent> {
    let cap = state.cap;
    let more = crate::icons::ellipsis_button(ui, &format!("More actions {}", cap.id), p.muted);
    let mut event = None;
    egui::Popup::menu(&more).show(|ui| {
        ui.set_min_width(160.0);
        let automated = cap.method != LifecycleMethod::Manual;
        let reinstall = ax_menu_item(ui, "Reinstall", &format!("Reinstall {}", cap.id), {
            cap.installed && automated && !state.busy
        });
        if reinstall.clicked() {
            event = Some(RowEvent::Start(LifecycleAction::Reinstall));
        }
        let uninstall = ax_menu_item(ui, "Uninstall…", &format!("Uninstall {}", cap.id), {
            cap.installed && automated && !state.busy
        });
        if uninstall.clicked() {
            event = Some(RowEvent::AskConfirm(LifecycleAction::Uninstall));
        }
        ui.separator();
        let (label, ax) = if cap.enabled {
            ("Disable", format!("Disable {}", cap.id))
        } else {
            ("Enable", format!("Enable {}", cap.id))
        };
        if ax_menu_item(ui, label, &ax, true).clicked() {
            event = Some(RowEvent::SetEnabled(!cap.enabled));
        }
    });
    event
}

/// A menu row with an explicit AccessKit label, disabled rather than hidden
/// when not applicable.
fn ax_menu_item(ui: &mut egui::Ui, text: &str, ax_label: &str, enabled: bool) -> egui::Response {
    let response = ui.add_enabled(
        enabled,
        egui::Button::new(RichText::new(text).size(theme::SIZE_BODY)),
    );
    let label = ax_label.to_string();
    response.widget_info(move || {
        egui::WidgetInfo::labeled(egui::WidgetType::Button, true, label.clone())
    });
    response
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testkit::{cap, harness, harness_themed};
    use ade_core::gui::inventory::CapabilityStatus;
    use egui_kittest::kittest::Queryable;

    fn state<'a>(cap: &'a CapabilityStatus) -> RowState<'a> {
        RowState {
            cap,
            busy: false,
            show_capability: true,
            issues_open: false,
        }
    }

    /// Render a set of rows to a PNG that a human (or I) can actually look at.
    fn render(name: &str, rows: Vec<(CapabilityStatus, bool)>) {
        let mut h = harness(
            egui::vec2(1000.0, 40.0 + 56.0 * rows.len() as f32),
            move |ui| {
                let p = theme::palette(true);
                for (index, (cap, busy)) in rows.iter().enumerate() {
                    if index > 0 {
                        crate::app::row_hairline(ui, &p);
                    }
                    let mut row = state(cap);
                    row.busy = *busy;
                    capability_row(ui, &p, &row);
                }
            },
        );
        h.snapshot(name);
    }

    #[test]
    fn snapshot_every_row_state_side_by_side() {
        // One image showing each state a row can be in. Rendering them together
        // is the point: misalignment between rows is invisible one at a time.
        let healthy = cap("trufflehog");
        let mut stale = cap("gitleaks");
        stale.version = Some("8.30.1".into());
        stale.latest_version = Some("8.31.0".into());
        stale.update_available = true;
        let mut missing = cap("pre-commit");
        missing.installed = false;
        missing.version = None;
        missing.path = None;
        missing.issues = vec![ade_core::types::Finding::warn(
            "pre-commit is not installed",
            "Use Install here",
        )];
        let mut manual = cap("ocean");
        manual.installed = false;
        manual.version = None;
        let mut broken = cap("rtk");
        broken.version = None;
        broken.issues = vec![ade_core::types::Finding::error(
            "RTK is on PATH but its version probe failed",
        )];
        let mut plugin = cap("codeguard");
        plugin.version = Some("1.4.0".into());
        let mut off = cap("osv-scanner");
        off.enabled = false;
        render(
            "rows_all_states",
            vec![
                (healthy, false),
                (stale, false),
                (missing, false),
                (manual, false),
                (broken, false),
                (plugin, false),
                (off, false),
                (cap("nono"), true),
            ],
        );
    }

    /// The screen as it is actually shaped: a real window width, a section
    /// heading, and a group surface — not a row in a tight 1000px frame.
    /// Snapshotting the component and not the screen is why six passes of row
    /// polish left the window still looking wrong.
    #[test]
    fn snapshot_the_screen_at_real_window_width() {
        let mut hog = cap("trufflehog");
        hog.version = Some("3.96.0".into());
        let mut leaks = cap("gitleaks");
        leaks.version = Some("8.30.1".into());
        let mut missing = cap("nono");
        missing.installed = false;
        missing.version = None;
        missing.issues = vec![ade_core::types::Finding::warn(
            "nono is not installed",
            "Use Install here",
        )];
        let mut h = harness_themed(egui::vec2(900.0, 300.0), false, move |ui| {
            let p = theme::palette(false);
            crate::app::content_column(ui, |ui| {
                for (heading, rows) in [
                    ("Secret Scanning", vec![&hog, &leaks]),
                    ("Execution Sandboxing", vec![&missing]),
                ] {
                    // Same heading + surface code path the app itself calls —
                    // a hand-drawn copy here is evidence about nothing.
                    theme::section_heading(ui, &p, heading);
                    ui.add_space(4.0);
                    theme::section_surface(&p, false).show(ui, |ui| {
                        for (index, c) in rows.iter().enumerate() {
                            if index > 0 {
                                crate::app::row_hairline(ui, &p);
                            }
                            let mut st = state(c);
                            st.show_capability = false;
                            capability_row(ui, &p, &st);
                        }
                    });
                    ui.add_space(16.0);
                }
            });
        });
        h.snapshot("screen_grouped_light");
    }

    #[test]
    fn snapshot_light_appearance_holds_its_contrast() {
        // The app follows the OS appearance, so a palette tuned only against a
        // dark background is half-verified. Faint metadata is exactly what goes
        // illegible when the surface flips.
        let mut missing = cap("pre-commit");
        missing.installed = false;
        missing.version = None;
        missing.issues = vec![ade_core::types::Finding::warn(
            "not installed",
            "install it",
        )];
        let mut off = cap("osv-scanner");
        off.enabled = false;
        let rows = [cap("trufflehog"), missing, off];
        let mut h = harness_themed(egui::vec2(1000.0, 200.0), false, move |ui| {
            let p = theme::palette(false);
            for (index, c) in rows.iter().enumerate() {
                if index > 0 {
                    crate::app::row_hairline(ui, &p);
                }
                capability_row(ui, &p, &state(c));
            }
        });
        h.snapshot("rows_light");
    }

    #[test]
    fn snapshot_grouped_view_drops_the_capability_chip() {
        // Under a capability heading the chip is redundant; this proves the
        // column really is reclaimed rather than left blank.
        let rows = vec![cap("trufflehog"), cap("gitleaks")];
        let mut h = harness(egui::vec2(1000.0, 150.0), move |ui| {
            let p = theme::palette(true);
            ui.label(
                RichText::new("SECRET SCANNING")
                    .color(p.faint_text)
                    .font(theme::medium(10.0)),
            );
            for c in &rows {
                let mut row = state(c);
                row.show_capability = false;
                capability_row(ui, &p, &row);
            }
        });
        h.snapshot("rows_grouped");
    }

    #[test]
    fn a_row_reports_what_was_clicked_and_changes_nothing_itself() {
        // The row is pure: it draws, and reports what the owner did through
        // its return value. The app decides what a click means. This also
        // guards the AccessKit surface the Interceptor drive depends on.
        let subject = cap("trufflehog");
        let seen: std::sync::Arc<std::sync::Mutex<Vec<RowEvent>>> = Default::default();
        let sink = std::sync::Arc::clone(&seen);
        let mut h = harness(egui::vec2(1000.0, 120.0), move |ui| {
            let p = theme::palette(true);
            if let Some(event) = capability_row(ui, &p, &state(&subject)) {
                sink.lock().expect("event sink").push(event);
            }
        });
        // Closed: the overflow button is labelled; its items are not yet in
        // the tree — destructive actions are one deliberate click away.
        assert!(h.query_by_label("More actions trufflehog").is_some());
        assert!(h.query_by_label("Uninstall trufflehog").is_none());
        h.get_by_label("More actions trufflehog").click();
        h.run_steps(2);
        // Open: every item present with a unique label, disabled rather than
        // absent when not applicable — the menu's shape never changes.
        for label in [
            "Reinstall trufflehog",
            "Uninstall trufflehog",
            "Disable trufflehog",
        ] {
            assert!(
                h.query_by_label(label).is_some(),
                "missing accessible menu item: {label}"
            );
        }
        // Uninstall asks for confirmation; it never starts work directly.
        h.get_by_label("Uninstall trufflehog").click();
        h.run_steps(2);
        let events = seen.lock().expect("event sink").clone();
        assert_eq!(
            events,
            vec![RowEvent::AskConfirm(LifecycleAction::Uninstall)]
        );
        // The row never renders an inline confirm any more: the shell-owned
        // modal is the only place a destructive action can be confirmed.
        h.run_steps(2);
        assert!(h.query_by_label("Confirm uninstall trufflehog").is_none());
    }
}
