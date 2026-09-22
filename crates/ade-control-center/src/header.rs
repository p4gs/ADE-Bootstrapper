//! The slim toolbar at the top of the content area. The window titlebar
//! already names the app, so this row carries only the two machine-wide
//! controls, right-aligned.
//!
//! Known trap, paid for once already: inside a right-to-left layout a
//! `ui.vertical` claims the entire remaining width and crushes everything to
//! its left — this row is single-line labels only.

use crate::app::ax_button;
use crate::theme;
use egui::{Align, Layout};

/// What the owner did in the toolbar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum HeaderEvent {
    CheckUpdates,
    Refresh,
}

pub(crate) fn toolbar(
    ui: &mut egui::Ui,
    p: &theme::Palette,
    checking_updates: bool,
) -> Option<HeaderEvent> {
    let mut event = None;
    // The right-to-left cluster lives INSIDE a horizontal row: handed the
    // panel's ui directly, `with_layout` claims ALL remaining height and the
    // cross-axis centring floats the buttons into the middle of an otherwise
    // empty content area (found by the whole-screen snapshot).
    ui.horizontal(|ui| {
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if ax_button(
                ui,
                if checking_updates {
                    "Checking…"
                } else {
                    "Check for updates"
                },
                "Check for updates",
                Some(p.accent),
                !checking_updates,
            )
            .clicked()
            {
                event = Some(HeaderEvent::CheckUpdates);
            }
            if ax_button(ui, "Refresh", "Refresh state", None, true).clicked() {
                event = Some(HeaderEvent::Refresh);
            }
        });
    });
    event
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testkit::harness_themed;
    use egui_kittest::kittest::Queryable;

    fn paint(
        checking: bool,
        sink: std::sync::Arc<std::sync::Mutex<Vec<HeaderEvent>>>,
    ) -> impl FnMut(&mut egui::Ui) + 'static {
        move |ui| {
            let p = crate::theme::palette(true);
            if let Some(event) = toolbar(ui, &p, checking) {
                sink.lock().expect("sink").push(event);
            }
        }
    }

    #[test]
    fn snapshot_the_toolbar_at_content_width() {
        let sink = std::sync::Arc::default();
        let mut h = harness_themed(egui::vec2(700.0, 60.0), true, paint(false, sink));
        h.snapshot("toolbar");
    }

    #[test]
    fn the_toolbar_reports_clicks_and_disables_while_checking() {
        let seen: std::sync::Arc<std::sync::Mutex<Vec<HeaderEvent>>> = Default::default();
        let sink = std::sync::Arc::clone(&seen);
        let mut h = harness_themed(egui::vec2(700.0, 60.0), true, paint(false, sink));
        h.get_by_label("Refresh state").click();
        h.run_steps(2);
        h.get_by_label("Check for updates").click();
        h.run_steps(2);
        assert_eq!(
            seen.lock().expect("sink").clone(),
            vec![HeaderEvent::Refresh, HeaderEvent::CheckUpdates]
        );

        // While a check runs the button is present but disabled — the AX tree
        // keeps its shape.
        let quiet: std::sync::Arc<std::sync::Mutex<Vec<HeaderEvent>>> = Default::default();
        let sink = std::sync::Arc::clone(&quiet);
        let mut busy = harness_themed(egui::vec2(700.0, 60.0), true, paint(true, sink));
        assert!(busy.query_by_label("Check for updates").is_some());
        busy.get_by_label("Check for updates").click();
        busy.run_steps(2);
        assert!(quiet.lock().expect("sink").is_empty());
    }
}
