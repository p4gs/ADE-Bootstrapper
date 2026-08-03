//! Painted-geometry icons — the app's entire icon language (Phase F).
//!
//! egui cannot load SF Symbols (no system font file exists) and emoji are
//! banned, so every mark is epaint strokes and fills: one stroke weight,
//! three sizes aligned to the type scale (10/13/16). Text glyphs doing icon
//! work land on the macOS emoji path and jitter across baselines; these
//! cannot.
//!
//! Rounded joins and caps are emulated the way epaint allows: a filled
//! circle of half the stroke width at every vertex and endpoint.

use crate::theme;
use egui::{Color32, Pos2, Rect, Response, Sense, Stroke, Ui, Vec2};

/// The three icon sizes, aligned to the type scale (caption/body/section).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum IconSize {
    S10,
    S13,
    S16,
}

impl IconSize {
    pub(crate) const fn px(self) -> f32 {
        match self {
            IconSize::S10 => 10.0,
            IconSize::S13 => 13.0,
            IconSize::S16 => 16.0,
        }
    }
}

/// The single stroke weight of the whole icon language.
pub(crate) const STROKE_WEIGHT: f32 = 1.6;

/// A point at normalized (0..1) coordinates inside the icon rect.
fn at(rect: Rect, x: f32, y: f32) -> Pos2 {
    Pos2::new(
        rect.left() + rect.width() * x,
        rect.top() + rect.height() * y,
    )
}

/// Stroke a polyline with rounded caps and joins (endpoint circles at half
/// the stroke width — epaint's PathShape has neither caps nor joins).
fn rounded_polyline(ui: &Ui, points: &[Pos2], color: Color32, closed: bool) {
    let painter = ui.painter();
    let stroke = Stroke::new(STROKE_WEIGHT, color);
    let segments = if closed {
        points.len()
    } else {
        points.len() - 1
    };
    for index in 0..segments {
        let next = (index + 1) % points.len();
        painter.line_segment([points[index], points[next]], stroke);
    }
    for point in points {
        painter.circle_filled(*point, STROKE_WEIGHT * 0.5, color);
    }
}

/// A check stroke. Used sparingly — healthy is silent; the check appears
/// only in the all-clear attention box and the confirm of a completed job.
pub(crate) fn check(ui: &Ui, rect: Rect, color: Color32) {
    rounded_polyline(
        ui,
        &[
            at(rect, 0.12, 0.55),
            at(rect, 0.38, 0.82),
            at(rect, 0.90, 0.18),
        ],
        color,
        false,
    );
}

/// The broken-state mark: a rounded-join triangle with an exclamation
/// stroke. Painted geometry, never U+26A0 — macOS renders that codepoint as
/// an orange emoji.
pub(crate) fn warning_triangle(ui: &Ui, rect: Rect, color: Color32) {
    rounded_polyline(
        ui,
        &[
            at(rect, 0.50, 0.08),
            at(rect, 0.94, 0.88),
            at(rect, 0.06, 0.88),
        ],
        color,
        true,
    );
    let painter = ui.painter();
    painter.line_segment(
        [at(rect, 0.50, 0.36), at(rect, 0.50, 0.60)],
        Stroke::new(STROKE_WEIGHT, color),
    );
    painter.circle_filled(at(rect, 0.50, 0.36), STROKE_WEIGHT * 0.5, color);
    painter.circle_filled(at(rect, 0.50, 0.60), STROKE_WEIGHT * 0.5, color);
    painter.circle_filled(at(rect, 0.50, 0.76), STROKE_WEIGHT * 0.62, color);
}

/// An open circle with a centered dot — the "empty slot" mark for a missing
/// provider: the place exists, nothing occupies it.
pub(crate) fn circle_dot(ui: &Ui, rect: Rect, color: Color32) {
    let painter = ui.painter();
    let center = rect.center();
    let radius = rect.width() * 0.38;
    painter.circle_stroke(center, radius, Stroke::new(STROKE_WEIGHT, color));
    painter.circle_filled(center, rect.width() * 0.13, color);
}

/// A rightward chevron — the navigation affordance on rows that open a page.
pub(crate) fn chevron_right(ui: &Ui, rect: Rect, color: Color32) {
    rounded_polyline(
        ui,
        &[
            at(rect, 0.36, 0.20),
            at(rect, 0.66, 0.50),
            at(rect, 0.36, 0.80),
        ],
        color,
        false,
    );
}

/// Three painted dots — the overflow-menu mark (never the "…" text glyph,
/// whose baseline sits wrong for an icon slot).
pub(crate) fn ellipsis(ui: &Ui, rect: Rect, color: Color32) {
    let painter = ui.painter();
    let radius = rect.width() * 0.09;
    for x in [0.18, 0.50, 0.82] {
        painter.circle_filled(at(rect, x, 0.5), radius, color);
    }
}

/// The overflow-menu button: same hit target and AccessKit surface as a text
/// button, with the painted ellipsis where the glyph was. Rests dimmed and
/// comes to full strength under the pointer (the ~120ms action reveal) — the
/// AX tree is identical in both states.
pub(crate) fn ellipsis_button(ui: &mut Ui, ax_label: &str, color: Color32) -> Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::new(26.0, 26.0), Sense::click());
    let label = ax_label.to_string();
    response.widget_info(move || {
        egui::WidgetInfo::labeled(egui::WidgetType::Button, true, label.clone())
    });
    if ui.is_rect_visible(rect) {
        let reveal =
            ui.ctx()
                .animate_bool_with_time(response.id, response.hovered(), theme::DURATION_HOVER);
        // Same floor-plus-reveal curve as the coverage-row chevron (item 2 of
        // the punch list): rest already clears the AA floor instead of
        // sitting under it, hover brightens to full strength.
        let strength = 0.85 + 0.15 * reveal;
        let icon = Rect::from_center_size(rect.center(), Vec2::splat(IconSize::S13.px()));
        ellipsis(ui, icon, color.gamma_multiply(strength));
    }
    response
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The size scale is the type scale — no intermediate icon sizes exist.
    #[test]
    fn icon_sizes_align_to_the_type_scale() {
        assert_eq!(IconSize::S10.px(), 10.0);
        assert_eq!(IconSize::S13.px(), 13.0);
        assert_eq!(IconSize::S16.px(), 16.0);
        assert_eq!(STROKE_WEIGHT, 1.6, "one stroke weight, everywhere");
    }

    /// Normalized coordinates land inside the rect they are asked about.
    #[test]
    fn icon_geometry_stays_inside_its_rect() {
        let rect = Rect::from_min_size(Pos2::new(10.0, 20.0), Vec2::splat(16.0));
        assert_eq!(at(rect, 0.0, 0.0), Pos2::new(10.0, 20.0));
        assert_eq!(at(rect, 1.0, 1.0), Pos2::new(26.0, 36.0));
        assert_eq!(at(rect, 0.5, 0.5), rect.center());
    }
}
