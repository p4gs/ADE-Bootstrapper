//! The console drawer — Homebrew.app's bottom pane. Renders
//! `ade_core::gui::views::ConsoleModel`: one tab per recent job with a status
//! dot and a monospace command label, Copy on the right, a collapse chevron,
//! and the active job's log in a sunken monospace area.

use crate::app::ax_ghost_button;
use crate::icons;
use crate::provider_list::tone_pair;
use crate::theme;
use ade_core::gui::views::{ConsoleModel, ConsoleTab, Icon};
use egui::{Align, CornerRadius, FontId, Layout, RichText, Sense};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ConsoleEvent {
    SelectTab(String),
    Copy,
    Collapse,
}

pub(crate) const CONSOLE_HEIGHT: f32 = 200.0;

pub(crate) fn console(
    ui: &mut egui::Ui,
    p: &theme::Palette,
    model: &ConsoleModel,
) -> Option<ConsoleEvent> {
    let mut event = None;
    // The right cluster (status, Copy, hide) is placed FIRST, from the right
    // edge, and the tabs get whatever width remains in a clipped horizontal
    // scroll. Laid out the other way round, eight tabs pushed Copy and the
    // chevron clean off the window (found by the stress harness, ISC-28).
    ui.horizontal(|ui| {
        ui.set_width(ui.available_width());
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.spacing_mut().item_spacing.x = theme::Space::S6.px();
            if icon_button(ui, p, Icon::ChevronRight, "Hide console").clicked() {
                event = Some(ConsoleEvent::Collapse);
            }
            if ax_ghost_button(ui, "Copy", "Copy console log", p.muted).clicked() {
                event = Some(ConsoleEvent::Copy);
            }
            let (_, ink) = tone_pair(p, model.status_tone);
            ui.label(
                RichText::new(&model.status)
                    .color(ink)
                    .size(theme::SIZE_CAPTION),
            );
            ui.add_space(theme::Space::S8.px());
            let tabs_width = ui.available_width().max(0.0);
            ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = theme::Space::S6.px();
                let (shown, hidden) = tabs_that_fit(ui, &model.tabs, tabs_width);
                for tab in shown {
                    if console_tab(ui, p, tab).clicked() && !tab.active {
                        event = Some(ConsoleEvent::SelectTab(tab.job_id.clone()));
                    }
                }
                if !hidden.is_empty() {
                    let more = ax_ghost_button(
                        ui,
                        &format!("+{}", hidden.len()),
                        "More console tabs",
                        p.muted,
                    );
                    egui::Popup::menu(&more).show(|ui| {
                        for tab in hidden {
                            if ui
                                .selectable_label(
                                    tab.active,
                                    RichText::new(&tab.label)
                                        .font(FontId::monospace(theme::SIZE_CAPTION)),
                                )
                                .clicked()
                            {
                                event = Some(ConsoleEvent::SelectTab(tab.job_id.clone()));
                            }
                        }
                    });
                }
            });
        });
    });
    ui.add_space(theme::Space::S6.px());
    egui::Frame::new()
        .fill(p.inset)
        .corner_radius(CornerRadius::same(theme::RADIUS_CONTROL))
        .inner_margin(theme::margin(theme::Space::S12, theme::Space::S8))
        .show(ui, |ui| {
            ui.set_min_size(ui.available_size());
            egui::ScrollArea::vertical()
                .id_salt("console")
                .auto_shrink([false, false])
                .stick_to_bottom(true)
                .show(ui, |ui| {
                    if model.lines.is_empty() {
                        ui.label(
                            RichText::new("No output yet.")
                                .color(p.faint_text)
                                .font(FontId::monospace(theme::SIZE_CAPTION)),
                        );
                    }
                    for line in &model.lines {
                        ui.label(
                            RichText::new(line)
                                .color(p.text)
                                .font(FontId::monospace(theme::SIZE_CAPTION)),
                        );
                    }
                });
        });
    event
}

/// Which tabs fit in `width` (newest first, in order), and which fold into
/// the "+N" menu. The active tab always stays on the strip: when it would
/// fold, it takes the last visible slot instead. Nothing here scrolls —
/// a hidden-scrollbar strip has no affordance, and its off-screen tabs are
/// exactly the overflow the sweep's invariant flags.
fn tabs_that_fit<'a>(
    ui: &egui::Ui,
    tabs: &'a [ConsoleTab],
    width: f32,
) -> (Vec<&'a ConsoleTab>, Vec<&'a ConsoleTab>) {
    let gap = theme::Space::S6.px();
    let more_reserve = 40.0;
    let widths: Vec<f32> = tabs.iter().map(|t| tab_width(ui, t)).collect();
    let mut used = 0.0;
    let mut cut = tabs.len();
    for (i, w) in widths.iter().enumerate() {
        let need = used
            + w
            + if i + 1 < tabs.len() {
                gap + more_reserve
            } else {
                0.0
            };
        if need > width && i > 0 {
            cut = i;
            break;
        }
        used += w + gap;
    }
    let mut shown: Vec<&ConsoleTab> = tabs[..cut].iter().collect();
    let mut hidden: Vec<&ConsoleTab> = tabs[cut..].iter().collect();
    if let Some(pos) = hidden.iter().position(|t| t.active) {
        let active = hidden.remove(pos);
        if let Some(last) = shown.pop() {
            hidden.insert(0, last);
        }
        shown.push(active);
    }
    (shown, hidden)
}

fn tab_width(ui: &egui::Ui, tab: &ConsoleTab) -> f32 {
    let galley = ui.painter().layout_no_wrap(
        tab.label.clone(),
        FontId::monospace(theme::SIZE_CAPTION),
        egui::Color32::WHITE,
    );
    galley.size().x + 6.0 + theme::Space::S8.px() * 2.0 + theme::Space::S6.px()
}

/// A tab: status dot + monospace label in a pill; the active tab is filled.
fn console_tab(ui: &mut egui::Ui, p: &theme::Palette, tab: &ConsoleTab) -> egui::Response {
    let font = FontId::monospace(theme::SIZE_CAPTION);
    let galley = ui
        .painter()
        .layout_no_wrap(tab.label.clone(), font.clone(), p.text);
    let dot = 6.0;
    let pad = theme::Space::S8.px();
    let size = egui::vec2(
        galley.size().x + dot + pad * 2.0 + theme::Space::S6.px(),
        galley.size().y + theme::Space::S6.px(),
    );
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    let ax = format!("Console tab {}", tab.label);
    let active = tab.active;
    response.widget_info(move || {
        egui::WidgetInfo::selected(egui::WidgetType::SelectableLabel, true, active, ax.clone())
    });
    let (fill, ink) = tone_pair(p, tab.tone);
    if tab.active {
        ui.painter()
            .rect_filled(rect, CornerRadius::same(theme::RADIUS_CONTROL), fill);
    } else if response.hovered() {
        ui.painter()
            .rect_filled(rect, CornerRadius::same(theme::RADIUS_CONTROL), p.row_hover);
    }
    ui.painter().circle_filled(
        egui::pos2(rect.left() + pad + dot * 0.5, rect.center().y),
        dot * 0.5,
        ink,
    );
    let text_color = if tab.active { p.text } else { p.muted };
    let galley = ui
        .painter()
        .layout_no_wrap(tab.label.clone(), font, text_color);
    ui.painter().galley(
        egui::pos2(
            rect.left() + pad + dot + theme::Space::S6.px(),
            rect.center().y - galley.size().y * 0.5,
        ),
        galley,
        text_color,
    );
    response
}

/// A bare icon button (the toolbar idiom): glyph only, hover wash, AX label.
pub(crate) fn icon_button(
    ui: &mut egui::Ui,
    p: &theme::Palette,
    icon: Icon,
    ax_label: &str,
) -> egui::Response {
    let side = 24.0;
    let (rect, response) = ui.allocate_exact_size(egui::vec2(side, side), Sense::click());
    let label = ax_label.to_string();
    response.widget_info(move || {
        egui::WidgetInfo::labeled(egui::WidgetType::Button, true, label.clone())
    });
    if response.hovered() {
        ui.painter()
            .rect_filled(rect, CornerRadius::same(theme::RADIUS_CONTROL), p.row_hover);
    }
    let galley = ui.painter().layout_no_wrap(
        icons::glyph(icon).to_string(),
        theme::icon_font(14.0),
        p.muted,
    );
    ui.painter()
        .galley(rect.center() - galley.size() * 0.5, galley, p.muted);
    response
}
