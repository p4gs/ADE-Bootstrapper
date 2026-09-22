//! The middle pane of a capability group page: the provider list (Taphouse's
//! "Installed" list). Renders `ade_core::gui::views::ProviderListModel` and
//! reports what was clicked; nothing here decides what a row says.
//!
//! Anatomy, from the reference: a title with a one-line subtitle, a
//! segmented filter (Homebrew's All / Formulae / Casks), then 44pt rows with a
//! 28pt rounded status tile on the left, a semibold name followed by small
//! uppercase tag pills, a muted description line, and a right-aligned muted
//! version. Rows separate with a hairline inset past the tile, and the
//! selected row is a neutral wash (a blue field here would fight the status
//! tints in the tiles).

use crate::icons;
use crate::nav::elided;
use crate::theme;
use ade_core::gui::views::{ProviderFilter, ProviderListModel, ProviderRow, Tag, Tone};
use egui::emath::GuiRounding;
use egui::{Color32, FontId, RichText, Sense, StrokeKind};

/// What the owner did in the list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ProviderListEvent {
    Select(String),
    Filter(ProviderFilter),
}

const TILE: f32 = 28.0;

pub(crate) fn provider_list(
    ui: &mut egui::Ui,
    p: &theme::Palette,
    model: &ProviderListModel,
) -> Option<ProviderListEvent> {
    let mut event = None;

    // The toolbar already carries the group's name; the list leads with the
    // coverage line only.
    ui.add_space(theme::Space::S4.px());
    ui.label(
        RichText::new(&model.subtitle)
            .color(p.muted)
            .size(theme::SIZE_CAPTION),
    );
    ui.add_space(theme::Space::S8.px());

    if let Some(filter) = segmented_filter(ui, p, model) {
        event = Some(ProviderListEvent::Filter(filter));
    }
    ui.add_space(theme::Space::S12.px());
    hairline(ui, p, 0.0);

    egui::ScrollArea::vertical()
        .id_salt("provider-list")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            if model.rows.is_empty() {
                ui.add_space(theme::Space::S24.px());
                ui.vertical_centered(|ui| {
                    ui.label(
                        RichText::new(empty_text(model.filter))
                            .color(p.muted)
                            .size(theme::SIZE_BODY),
                    );
                });
                return;
            }
            for row in &model.rows {
                if provider_row(ui, p, row).clicked() {
                    event = Some(ProviderListEvent::Select(row.id.clone()));
                }
                hairline(
                    ui,
                    p,
                    theme::Space::S12.px() + TILE + theme::Space::S12.px(),
                );
            }
        });
    event
}

fn empty_text(filter: ProviderFilter) -> &'static str {
    match filter {
        ProviderFilter::All => "No providers in this capability.",
        ProviderFilter::NeedsAttention => "Nothing needs attention here.",
        ProviderFilter::Installed => "Nothing installed yet.",
    }
}

/// A one-pixel rule from `inset` to the right edge.
fn hairline(ui: &mut egui::Ui, p: &theme::Palette, inset: f32) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 1.0), Sense::hover());
    let ppp = ui.pixels_per_point();
    let y = theme::snap_y(rect.center().y, ppp);
    ui.painter().hline(
        (rect.left() + inset)..=rect.right(),
        y,
        egui::Stroke::new(1.0 / ppp, p.hairline),
    );
}

/// The segmented control: one capsule, the active segment filled white on
/// dark grey (the macOS idiom the reference apps use), counts in the labels.
fn segmented_filter(
    ui: &mut egui::Ui,
    p: &theme::Palette,
    model: &ProviderListModel,
) -> Option<ProviderFilter> {
    let mut chosen = None;
    let height = 24.0;
    let pad = theme::Space::S12.px();
    let font = theme::medium(theme::SIZE_CAPTION);
    // Measure every label first so the segments share one width and the
    // control never resizes when the counts change.
    let labels: Vec<String> = ProviderFilter::ALL
        .iter()
        .zip(model.counts.iter())
        .map(|(f, n)| format!("{} {n}", f.label()))
        .collect();
    let widest = labels
        .iter()
        .map(|l| {
            ui.painter()
                .layout_no_wrap(l.clone(), font.clone(), p.text)
                .size()
                .x
        })
        .fold(0.0_f32, f32::max);
    let seg_w = widest + 2.0 * pad;
    let total = egui::vec2(seg_w * labels.len() as f32 + 4.0, height + 4.0);
    let (outer, _) = ui.allocate_exact_size(total, Sense::hover());
    let ppp = ui.pixels_per_point();
    let radius = egui::CornerRadius::same(theme::RADIUS_CONTROL + 2);
    ui.painter()
        .rect_filled(outer.round_to_pixels(ppp), radius, p.widget);
    for (i, (filter, label)) in ProviderFilter::ALL.iter().zip(labels.iter()).enumerate() {
        let rect = egui::Rect::from_min_size(
            egui::pos2(outer.left() + 2.0 + seg_w * i as f32, outer.top() + 2.0),
            egui::vec2(seg_w, height),
        );
        let response = ui.interact(rect, ui.id().with(("filter", i)), Sense::click());
        let ax = format!("Filter {}", filter.label());
        let active = *filter == model.filter;
        response.widget_info(move || {
            egui::WidgetInfo::selected(egui::WidgetType::RadioButton, true, active, ax.clone())
        });
        if active {
            let snapped = rect.round_to_pixels(ppp);
            ui.painter().rect_filled(
                snapped,
                egui::CornerRadius::same(theme::RADIUS_CONTROL),
                p.bg,
            );
            ui.painter().rect_stroke(
                snapped,
                egui::CornerRadius::same(theme::RADIUS_CONTROL),
                egui::Stroke::new(1.0, p.line),
                StrokeKind::Inside,
            );
        } else if response.hovered() {
            ui.painter().rect_filled(
                rect.round_to_pixels(ppp),
                egui::CornerRadius::same(theme::RADIUS_CONTROL),
                p.row_hover,
            );
        }
        let color = if active { p.text } else { p.muted };
        let galley = ui
            .painter()
            .layout_no_wrap(label.clone(), font.clone(), color);
        ui.painter()
            .galley(rect.center() - galley.size() * 0.5, galley, color);
        if response.clicked() && !active {
            chosen = Some(*filter);
        }
    }
    chosen
}

/// One 44pt provider row.
fn provider_row(ui: &mut egui::Ui, p: &theme::Palette, row: &ProviderRow) -> egui::Response {
    let size = egui::vec2(
        ui.available_width(),
        theme::ROW_HEIGHT + theme::Space::S4.px(),
    );
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    let ax = row.name.clone();
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
    let hover = ui.ctx().animate_bool_with_time(
        response.id.with("hover"),
        response.hovered() && !row.selected,
        theme::DURATION_HOVER,
    );
    if row.selected {
        ui.painter()
            .rect_filled(rect.round_to_pixels(ppp), radius, p.selected);
    } else if hover > 0.0 {
        ui.painter().rect_filled(
            rect.round_to_pixels(ppp),
            radius,
            p.row_hover.gamma_multiply(hover),
        );
    }
    theme::focus_ring(ui, rect, theme::RADIUS_CONTROL, response.has_focus());

    let pad = theme::Space::S12.px();
    // Status tile: a rounded square tinted by the state, its icon in the
    // state's ink colour.
    let (tile_fill, tile_ink) = tone_pair(p, row.state.tone());
    let tile = egui::Rect::from_center_size(
        egui::pos2(rect.left() + pad + TILE * 0.5, rect.center().y),
        egui::vec2(TILE, TILE),
    )
    .round_to_pixels(ppp);
    ui.painter()
        .rect_filled(tile, egui::CornerRadius::same(7), tile_fill);
    let glyph = ui.painter().layout_no_wrap(
        icons::glyph(row.state.icon()).to_string(),
        theme::icon_font(15.0),
        tile_ink,
    );
    ui.painter()
        .galley(tile.center() - glyph.size() * 0.5, glyph, tile_ink);

    // Right column: the version, muted, monospaced like the reference.
    let mut right_edge = rect.right() - pad;
    if let Some(version) = &row.version {
        let galley = ui.painter().layout_no_wrap(
            version.clone(),
            FontId::monospace(theme::SIZE_CAPTION),
            p.muted,
        );
        let width = galley.size().x;
        let pos = egui::pos2(right_edge - width, rect.center().y - galley.size().y * 0.5);
        ui.painter().galley(pos, galley, p.muted);
        right_edge -= width + pad;
    }

    // Left column: name + tags on the first line, description on the second.
    let text_left = tile.right() + pad;
    let line1_y = rect.top() + theme::Space::S8.px();
    let name =
        ui.painter()
            .layout_no_wrap(row.name.clone(), theme::semibold(theme::SIZE_BODY), p.text);
    let name_h = name.size().y;
    ui.painter()
        .galley(egui::pos2(text_left, line1_y), name.clone(), p.text);
    let mut x = text_left + name.size().x + theme::Space::S6.px();
    for tag in &row.tags {
        let w = paint_tag(
            ui,
            p,
            egui::pos2(x, line1_y + name_h * 0.5),
            tag,
            right_edge - x,
        );
        if w == 0.0 {
            break;
        }
        x += w + theme::Space::S4.px();
    }
    let desc_y = line1_y + name_h + theme::Space::S2.px();
    let desc = elided(
        ui,
        &row.description,
        p.muted,
        FontId::proportional(theme::SIZE_CAPTION),
        (right_edge - text_left).max(0.0),
    );
    ui.painter()
        .galley(egui::pos2(text_left, desc_y), desc, p.muted);
    response
}

/// A small uppercase tag pill centred on `center_left.y`; returns its width,
/// or 0 when it would not fit in `max_width` (then nothing is painted).
fn paint_tag(
    ui: &egui::Ui,
    p: &theme::Palette,
    center_left: egui::Pos2,
    tag: &Tag,
    max_width: f32,
) -> f32 {
    let (fill, ink) = tone_pair(p, tag.tone);
    let galley = ui
        .painter()
        .layout_no_wrap(tag.label.to_uppercase(), theme::semibold(9.0), ink);
    let w = galley.size().x + theme::Space::S8.px();
    let h = galley.size().y + theme::Space::S3.px();
    if w > max_width {
        return 0.0;
    }
    let rect = egui::Rect::from_min_size(
        egui::pos2(center_left.x, center_left.y - h * 0.5),
        egui::vec2(w, h),
    )
    .round_to_pixels(ui.pixels_per_point());
    ui.painter()
        .rect_filled(rect, egui::CornerRadius::same((h * 0.5) as u8), fill);
    ui.painter()
        .galley(rect.center() - galley.size() * 0.5, galley, ink);
    w
}

/// (fill, ink) for a tone: status tones use their tint + colour; neutral is
/// the resting widget grey with muted ink; accent uses the info tint.
pub(crate) fn tone_pair(p: &theme::Palette, tone: Tone) -> (Color32, Color32) {
    match tone {
        Tone::Ok => (p.tint_ok, p.ok),
        Tone::Warn => (p.tint_warn, p.warn),
        Tone::Err => (p.tint_err, p.err),
        Tone::Accent => (p.tint_info, p.accent),
        Tone::Neutral => (p.widget, p.muted),
        Tone::Muted => (p.widget, p.faint_text),
    }
}
