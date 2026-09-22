//! Shared snapshot-test scaffolding. Test-only: compiled under `#[cfg(test)]`
//! from `main.rs`, so every view module renders through the same harness and
//! the same fixtures.

use crate::theme;
use ade_core::gui::inventory::{get_capability, CapabilityStatus};
use egui_kittest::Harness;

/// Build a harness whose first frame only registers the custom fonts.
///
/// `Context::set_fonts` takes effect on the FOLLOWING frame, so painting
/// text in a named family on frame one panics inside epaint. Priming once
/// and requesting a repaint keeps the snapshot on the real typography
/// rather than falling back to egui's defaults.
pub(crate) fn harness(
    size: egui::Vec2,
    paint: impl FnMut(&mut egui::Ui) + 'static,
) -> Harness<'static> {
    harness_themed(size, true, paint)
}

pub(crate) fn harness_themed(
    size: egui::Vec2,
    dark: bool,
    mut paint: impl FnMut(&mut egui::Ui) + 'static,
) -> Harness<'static> {
    let mut frame = 0u32;
    let mut built = Harness::builder().with_size(size).build_ui(move |ui| {
        theme::apply(ui.ctx());
        ui.ctx().set_theme(if dark {
            egui::Theme::Dark
        } else {
            egui::Theme::Light
        });
        frame += 1;
        if frame == 1 {
            ui.ctx().request_repaint();
            return;
        }
        let p = theme::palette(dark);
        ui.visuals_mut().override_text_color = Some(p.text);
        egui::Frame::new()
            .fill(p.panel)
            .inner_margin(egui::Margin::symmetric(14, 8))
            .show(ui, |ui| paint(ui));
    });
    // A row with a job in flight paints a Spinner, which requests a repaint
    // every frame by design — so the UI never goes quiescent and `run()`
    // hits its step limit. A fixed number of frames is the right contract
    // for a snapshot anyway: deterministic, and enough for fonts to land.
    built.run_steps(4);
    built
}

/// A healthy, installed status row for a real capability id, so taxonomy
/// lookups resolve. Tests mutate the fields they care about.
pub(crate) fn cap(id: &str) -> CapabilityStatus {
    let def = get_capability(id).expect("real capability id");
    CapabilityStatus {
        id: def.id.to_string(),
        name: def.name.to_string(),
        kind: def.kind,
        capability: def.capability.to_string(),
        description: def.description.to_string(),
        method: def.method,
        pkg: def.pkg.map(String::from),
        guidance: def.guidance.map(String::from),
        installed: true,
        path: Some(format!("/opt/homebrew/bin/{id}")),
        version: Some("3.96.0".to_string()),
        reports_version: !def.version_args.is_empty(),
        inactive: None,
        running: None,
        enabled: true,
        latest_version: None,
        update_available: false,
        issues: Vec::new(),
    }
}
