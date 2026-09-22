//! The source-list sidebar (the reference apps' left pane: Taphouse,
//! Updatest). Renders `ade_core::gui::views::SidebarModel` and nothing else:
//! which rows exist, what they say, which badge they carry and whether they
//! are selected is all decided in core. This file only paints.
//!
//! Anatomy, from the reference screenshots: a small uppercase eyebrow per
//! section; 28pt rows with a 16pt icon in the accent colour, a 13pt label,
//! and a right-aligned count pill; the selected row is a filled accent
//! capsule with white label, icon and pill (the macOS 14+ key-window idiom
//! Updatest uses); a muted footer line at the bottom.

use crate::icons;
use crate::nav::elided;
use crate::theme;
use ade_core::gui::views::{Badge, SidebarModel, SidebarRow, Tone};
use egui::emath::GuiRounding;
use egui::{Color32, FontId, RichText, Sense, StrokeKind};

/// Paints the sidebar; returns the scope the user clicked, if any.
pub(crate) fn sidebar(
    ui: &mut egui::Ui,
    p: &theme::Palette,
    model: &SidebarModel,
) -> Option<String> {
    let mut selected = None;
    // The freshness line (`model.footer`) is painted by the shell's footer
    // strip, not here: the sidebar is only the source list.
    egui::ScrollArea::vertical()
        .id_salt("sidebar")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            for (index, section) in model.sections.iter().enumerate() {
                if let Some(title) = &section.title {
                    ui.add_space(if index == 0 {
                        theme::Space::S4.px()
                    } else {
                        theme::Space::S12.px()
                    });
                    section_eyebrow(ui, p, title);
                } else if index > 0 {
                    ui.add_space(theme::Space::S12.px());
                }
                for row in &section.rows {
                    if sidebar_row(ui, p, row).clicked() {
                        selected = Some(row.scope.clone());
                    }
                }
            }
        });
    selected
}

/// 11pt semibold uppercase muted eyebrow, tracked a little.
fn section_eyebrow(ui: &mut egui::Ui, p: &theme::Palette, text: &str) {
    ui.label(
        RichText::new(text.to_uppercase())
            .color(p.muted)
            .font(theme::semibold(theme::SIZE_CAPTION))
            .extra_letter_spacing(0.8),
    );
    ui.add_space(theme::Space::S2.px());
}

/// One row. Selection is instant (a filled accent capsule), hover fades in
/// over `DURATION_HOVER`, keyboard focus is a ring whose box is reserved so
/// focus arriving never shifts a pixel.
fn sidebar_row(ui: &mut egui::Ui, p: &theme::Palette, row: &SidebarRow) -> egui::Response {
    let size = egui::vec2(ui.available_width(), theme::NAV_ROW_HEIGHT);
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    let ax = row.label.clone();
    let is_selected = row.selected;
    response.widget_info(move || {
        egui::WidgetInfo::selected(
            egui::WidgetType::SelectableLabel,
            true,
            is_selected,
            ax.clone(),
        )
    });
    if !ui.is_rect_visible(rect) {
        return response;
    }
    let ppp = ui.pixels_per_point();
    let radius = egui::CornerRadius::same(theme::RADIUS_CONTROL);
    let snapped = rect.round_to_pixels(ppp);

    let hover = ui.ctx().animate_bool_with_time(
        response.id.with("hover"),
        response.hovered() && !row.selected,
        theme::DURATION_HOVER,
    );
    if row.selected {
        ui.painter().rect_filled(snapped, radius, p.accent);
    } else if response.is_pointer_button_down_on() {
        ui.painter().rect_filled(snapped, radius, p.ghost_active);
    } else if hover > 0.0 {
        ui.painter()
            .rect_filled(snapped, radius, p.row_hover.gamma_multiply(hover));
    }
    ui.painter().rect_stroke(
        snapped,
        radius,
        theme::focus_stroke(response.has_focus()),
        StrokeKind::Inside,
    );
    theme::focus_ring(ui, rect, theme::RADIUS_CONTROL, response.has_focus());

    // On the accent capsule everything is white; otherwise the icon takes the
    // accent and the label the text colour, dimmed when the group is off.
    let (icon_color, text_color) = if row.selected {
        (Color32::WHITE, Color32::WHITE)
    } else if row.dimmed {
        (p.disabled_text, p.disabled_text)
    } else {
        (p.accent, p.text)
    };

    let pad = theme::Space::S8.px();
    let icon_size = 16.0;
    let icon_slot = icon_size + theme::Space::S8.px();
    let icon_galley = ui.painter().layout_no_wrap(
        icons::glyph(row.icon).to_string(),
        theme::icon_font(icon_size),
        icon_color,
    );
    let icon_pos = egui::pos2(
        rect.left() + pad + (icon_size - icon_galley.size().x) * 0.5,
        rect.center().y - icon_galley.size().y * 0.5,
    );
    ui.painter().galley(icon_pos, icon_galley, icon_color);

    let mut badge_width = 0.0;
    if let Some(badge) = &row.badge {
        badge_width = paint_badge(ui, p, rect, badge, row.selected) + pad;
    }

    let font = if row.selected {
        theme::semibold(theme::SIZE_BODY)
    } else {
        FontId::proportional(theme::SIZE_BODY)
    };
    let label_left = rect.left() + pad + icon_slot;
    let max_width = (rect.right() - pad - badge_width - label_left).max(0.0);
    let galley = elided(ui, &row.label, text_color, font, max_width);
    let label_pos = egui::pos2(label_left, rect.center().y - galley.size().y * 0.5);
    ui.painter().galley(label_pos, galley, text_color);
    response
}

/// The count pill, right-aligned inside `rect`. Returns its width. A true
/// circle for one digit, a capsule for more; the numeral is centred
/// geometrically on the pill, never by layout.
fn paint_badge(
    ui: &egui::Ui,
    p: &theme::Palette,
    rect: egui::Rect,
    badge: &Badge,
    on_accent: bool,
) -> f32 {
    let (fill, ink) = if on_accent {
        (Color32::WHITE.gamma_multiply(0.22), Color32::WHITE)
    } else {
        match badge.tone {
            Tone::Warn => (p.tint_warn, p.warn),
            Tone::Err => (p.tint_err, p.err),
            Tone::Ok => (p.tint_ok, p.ok),
            Tone::Accent => (p.tint_info, p.accent),
            Tone::Neutral | Tone::Muted => (p.widget, p.muted),
        }
    };
    let galley = ui.painter().layout_no_wrap(
        badge.count.to_string(),
        theme::medium(theme::SIZE_CAPTION),
        ink,
    );
    let diameter: f32 = 17.0;
    let width = diameter.max(galley.size().x + theme::Space::S8.px());
    let pad = theme::Space::S8.px();
    let center = egui::pos2(rect.right() - pad - width * 0.5, rect.center().y);
    let badge_rect = egui::Rect::from_center_size(center, egui::vec2(width, diameter))
        .round_to_pixels(ui.pixels_per_point());
    ui.painter().rect_filled(
        badge_rect,
        egui::CornerRadius::same((diameter * 0.5) as u8),
        fill,
    );
    ui.painter()
        .galley(badge_rect.center() - galley.size() * 0.5, galley, ink);
    width
}
