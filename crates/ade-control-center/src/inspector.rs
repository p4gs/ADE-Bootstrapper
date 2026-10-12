//! The inspector — the right pane of a capability group page (Updatest's
//! detail view). Renders `ade_core::gui::views::InspectorModel`; every label,
//! value and button in it was decided in core.
//!
//! Anatomy, from the reference: a large title with the version under it in
//! muted text and a one-line description; a row of pill buttons (neutral,
//! neutral, destructive tinted red); an "Information" heading over a grey
//! rounded card of label-over-value pairs in two columns; then any issues
//! and, for manual installs, the guidance in place of an Install button.

use crate::app::ax_button;
use crate::icons;
use crate::provider_list::tone_pair;
use crate::theme;
use ade_core::gui::views::{ActionButton, Card, InspectorModel, Pair, ProviderAction, Tone};
use egui::{Color32, CornerRadius, RichText};

pub(crate) fn inspector(
    ui: &mut egui::Ui,
    p: &theme::Palette,
    model: &InspectorModel,
) -> Option<ProviderAction> {
    let mut chosen = None;
    egui::ScrollArea::vertical()
        .id_salt("inspector")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.add_space(theme::Space::S4.px());
            header(ui, p, model);
            ui.add_space(theme::Space::S12.px());
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing.x = theme::Space::S6.px();
                for button in &model.actions {
                    if action_pill(ui, p, button).clicked() {
                        chosen = Some(button.action);
                    }
                }
            });
            if let Some(guidance) = &model.guidance {
                ui.add_space(theme::Space::S8.px());
                ui.label(
                    RichText::new(guidance)
                        .color(p.muted)
                        .size(theme::SIZE_CAPTION),
                );
            }
            ui.add_space(theme::Space::S20.px());
            for card in &model.cards {
                info_card(ui, p, card);
                ui.add_space(theme::Space::S16.px());
            }
            if !model.issues.is_empty() {
                theme::section_heading(ui, p, "Issues");
                for issue in &model.issues {
                    let (_, ink) = tone_pair(p, issue.tone);
                    // Three stacked lines: the level as an eyebrow, the
                    // message, the remediation. As one wrapped row the three
                    // ran into a single line with the fix starting
                    // mid-sentence, and a wrapped message's box sat under
                    // the level label.
                    ui.spacing_mut().item_spacing.y = theme::Space::S2.px();
                    ui.label(
                        RichText::new(&issue.level_label)
                            .color(ink)
                            .font(theme::semibold(theme::SIZE_CAPTION)),
                    );
                    ui.add(
                        egui::Label::new(RichText::new(&issue.message).size(theme::SIZE_BODY))
                            .wrap(),
                    );
                    if let Some(fix) = &issue.remediation {
                        ui.add(
                            egui::Label::new(
                                RichText::new(fix).color(p.muted).size(theme::SIZE_CAPTION),
                            )
                            .wrap(),
                        );
                    }
                    ui.add_space(theme::Space::S8.px());
                }
            }
        });
    chosen
}

fn header(ui: &mut egui::Ui, p: &theme::Palette, model: &InspectorModel) {
    ui.horizontal(|ui| {
        let (fill, ink) = tone_pair(p, model.state.tone());
        let side = 40.0;
        let (rect, _) = ui.allocate_exact_size(egui::vec2(side, side), egui::Sense::hover());
        ui.painter().rect_filled(rect, CornerRadius::same(10), fill);
        let glyph = ui.painter().layout_no_wrap(
            icons::glyph(model.state.icon()).to_string(),
            theme::icon_font(20.0),
            ink,
        );
        ui.painter()
            .galley(rect.center() - glyph.size() * 0.5, glyph, ink);
        ui.add_space(theme::Space::S4.px());
        // The text block is pinned to the width left of the tile: the title
        // elides (full name on hover), the version wraps. Left unbounded, a
        // long version ran past the pane and was clipped mid-glyph.
        let width = ui.available_width().max(0.0);
        ui.vertical(|ui| {
            ui.set_width(width);
            ui.spacing_mut().item_spacing.y = theme::Space::S2.px();
            ui.add(
                egui::Label::new(
                    RichText::new(&model.title)
                        .font(theme::semibold(theme::SIZE_TITLE))
                        .color(p.text),
                )
                .truncate(),
            )
            .on_hover_text(&model.title);
            if let Some(version) = &model.version {
                ui.add(
                    egui::Label::new(RichText::new(version).color(p.muted).size(theme::SIZE_BODY))
                        .wrap(),
                );
            }
        });
    });
    ui.add_space(theme::Space::S6.px());
    ui.label(
        RichText::new(&model.description)
            .color(p.muted)
            .size(theme::SIZE_BODY),
    );
}

/// A pill button: neutral rest fill, or the error tint with red text for a
/// destructive action; disabled while a job for this capability runs.
fn action_pill(ui: &mut egui::Ui, p: &theme::Palette, button: &ActionButton) -> egui::Response {
    let ax = format!("{} {}", button.label, "provider");
    if button.destructive {
        let response = ui.add_enabled_ui(button.enabled, |ui| {
            destructive_pill(ui, p, &button.label, &ax)
        });
        response.inner
    } else {
        ax_button(ui, &button.label, &ax, None, button.enabled)
    }
}

fn destructive_pill(ui: &mut egui::Ui, p: &theme::Palette, text: &str, ax: &str) -> egui::Response {
    let (fill, ink) = tone_pair(p, Tone::Err);
    let button = egui::Button::new(RichText::new(text).color(ink).size(theme::SIZE_BODY))
        .fill(fill)
        .stroke(egui::Stroke::NONE)
        .wrap_mode(egui::TextWrapMode::Extend)
        .corner_radius(CornerRadius::same(theme::RADIUS_CONTROL));
    let response = ui.add(button);
    let label = ax.to_string();
    response.widget_info(move || {
        egui::WidgetInfo::labeled(egui::WidgetType::Button, true, label.clone())
    });
    response
}

/// The grey rounded card of label-over-value pairs, two per row.
fn info_card(ui: &mut egui::Ui, p: &theme::Palette, card: &Card) {
    if let Some(title) = &card.title {
        theme::section_heading(ui, p, title);
    }
    egui::Frame::new()
        .fill(p.panel)
        .corner_radius(CornerRadius::same(theme::RADIUS_CONTAINER))
        .inner_margin(theme::margin(theme::Space::S16, theme::Space::S12))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            let full_w = ui.available_width();
            let gap = theme::Space::S16.px();
            let col_w = (full_w - gap) * 0.5;
            // Pairs whose value fits half the card share a row, two up; a
            // value that does not fit takes the full width on its own row.
            // Wrapping a long version or package name inside a half-width
            // column broke it mid-token at the pane's 300pt minimum.
            let fits = |pair: &Pair| {
                ui.painter()
                    .layout_no_wrap(
                        pair.value.clone(),
                        egui::FontId::proportional(theme::SIZE_BODY),
                        p.text,
                    )
                    .size()
                    .x
                    <= col_w
            };
            let mut rows: Vec<Vec<&Pair>> = Vec::new();
            for pair in &card.pairs {
                let wide = !fits(pair);
                match rows.last_mut() {
                    Some(row) if !wide && row.len() == 1 && fits(row[0]) => row.push(pair),
                    _ => rows.push(vec![pair]),
                }
            }
            ui.spacing_mut().item_spacing.y = theme::Space::S12.px();
            for row in rows {
                ui.horizontal_top(|ui| {
                    ui.spacing_mut().item_spacing.x = gap;
                    let width = if row.len() == 2 { col_w } else { full_w };
                    for pair in row {
                        info_pair(ui, p, pair, width);
                    }
                });
            }
        });
}

/// One label-over-value pair, `width` wide. A link value elides with the
/// full text on hover; a plain value wraps.
fn info_pair(ui: &mut egui::Ui, p: &theme::Palette, pair: &Pair, width: f32) {
    ui.vertical(|ui| {
        ui.set_width(width);
        ui.set_max_width(width);
        ui.spacing_mut().item_spacing.y = theme::Space::S2.px();
        ui.label(
            RichText::new(&pair.label)
                .color(p.muted)
                .size(theme::SIZE_CAPTION),
        );
        if pair.link {
            let ax = format!("Open {}", pair.label);
            let response = ui
                .add(
                    egui::Label::new(
                        RichText::new(&pair.value)
                            .color(p.info)
                            .size(theme::SIZE_BODY),
                    )
                    .truncate()
                    .sense(egui::Sense::click()),
                )
                .on_hover_text(&pair.value);
            response.widget_info(move || {
                egui::WidgetInfo::labeled(egui::WidgetType::Link, true, ax.clone())
            });
        } else {
            ui.add(
                egui::Label::new(
                    RichText::new(&pair.value)
                        .color(p.text)
                        .size(theme::SIZE_BODY),
                )
                .wrap(),
            );
        }
    });
}

/// Ink for a tone, for callers that only need text colour.
#[allow(dead_code)]
pub(crate) fn tone_ink(p: &theme::Palette, tone: Tone) -> Color32 {
    tone_pair(p, tone).1
}
