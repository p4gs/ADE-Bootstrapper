//! The unified toolbar across the top of the content area, in the reference
//! apps' shape: a sidebar toggle at the left edge, the page title with a
//! muted subtitle, and a right-hand cluster of a search field and circular
//! icon buttons (Refresh, Check for updates). The window titlebar is melted
//! into the window, so this row is the only header the owner sees.
//!
//! Known trap, paid for once already: inside a right-to-left layout a
//! `ui.vertical` claims the entire remaining width and crushes everything to
//! its left — the title block is laid out first, in a left-to-right row.

use crate::icons;
use crate::nav::elided;
use crate::theme;
use ade_core::gui::views::{Icon, ToolbarModel};
use egui::emath::GuiRounding;
use egui::{Align, CornerRadius, FontId, Layout, Sense, Stroke};

/// What the owner did in the toolbar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum HeaderEvent {
    CheckUpdates,
    Refresh,
    ToggleSidebar,
}

/// Everything the toolbar needs to draw itself.
pub(crate) struct ToolbarState<'a> {
    pub model: &'a ToolbarModel,
    pub checking: bool,
    /// The search text, owned by the shell; edited in place.
    pub query: &'a mut String,
    /// Extra space before the sidebar toggle — the shell sets it while the
    /// sidebar is collapsed so the row clears the floating traffic lights.
    pub left_inset: f32,
}

pub(crate) const TOOLBAR_HEIGHT: f32 = 48.0;
const ICON_BUTTON: f32 = 28.0;
const SEARCH_WIDTH: f32 = 180.0;

pub(crate) fn toolbar(
    ui: &mut egui::Ui,
    p: &theme::Palette,
    state: &mut ToolbarState<'_>,
) -> Option<HeaderEvent> {
    let mut event = None;
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), TOOLBAR_HEIGHT),
        Sense::hover(),
    );
    let mut row = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(rect)
            .layout(Layout::left_to_right(Align::Center)),
    );
    row.spacing_mut().item_spacing.x = theme::Space::S8.px();
    if state.left_inset > 0.0 {
        row.add_space(state.left_inset);
    }

    // Left: the sidebar toggle, then the title block.
    if circle_icon_button(&mut row, p, Icon::PanelLeft, "Toggle sidebar", false).clicked() {
        event = Some(HeaderEvent::ToggleSidebar);
    }
    row.add_space(theme::Space::S4.px());
    // Reserve the right cluster's width so the title elides instead of
    // colliding with it.
    let cluster_width = SEARCH_WIDTH + 2.0 * ICON_BUTTON + 4.0 * theme::Space::S8.px();
    let title_width = (row.available_width() - cluster_width).max(80.0);
    row.allocate_ui_with_layout(
        egui::vec2(title_width, TOOLBAR_HEIGHT),
        Layout::top_down(Align::Min),
        |ui| {
            ui.spacing_mut().item_spacing.y = theme::Space::S1.px();
            ui.add_space((TOOLBAR_HEIGHT - 34.0) * 0.5);
            let title = elided(
                ui,
                &state.model.title,
                p.text,
                theme::semibold(theme::SIZE_SECTION),
                title_width,
            );
            let (r, _) = ui.allocate_exact_size(title.size(), Sense::hover());
            ui.painter().galley(r.min, title, p.text);
            if !state.model.subtitle.is_empty() {
                let sub = elided(
                    ui,
                    &state.model.subtitle,
                    p.muted,
                    FontId::proportional(theme::SIZE_CAPTION),
                    title_width,
                );
                let (r, _) = ui.allocate_exact_size(sub.size(), Sense::hover());
                ui.painter().galley(r.min, sub, p.muted);
            }
        },
    );

    // Right: search, refresh, check for updates.
    row.with_layout(Layout::right_to_left(Align::Center), |ui| {
        ui.spacing_mut().item_spacing.x = theme::Space::S8.px();
        if circle_icon_button(
            ui,
            p,
            Icon::CloudDownload,
            "Check for updates",
            state.checking,
        )
        .clicked()
            && !state.checking
        {
            event = Some(HeaderEvent::CheckUpdates);
        }
        if circle_icon_button(ui, p, Icon::RefreshCw, "Refresh state", false).clicked() {
            event = Some(HeaderEvent::Refresh);
        }
        search_field(ui, p, state.query);
    });
    event
}

/// A 28pt circular icon button: resting widget fill with a hairline ring,
/// lifts on hover, shows a spinner instead of its glyph while `busy`.
pub(crate) fn circle_icon_button(
    ui: &mut egui::Ui,
    p: &theme::Palette,
    icon: Icon,
    ax_label: &str,
    busy: bool,
) -> egui::Response {
    let (rect, response) =
        ui.allocate_exact_size(egui::vec2(ICON_BUTTON, ICON_BUTTON), Sense::click());
    let label = ax_label.to_string();
    response.widget_info(move || {
        egui::WidgetInfo::labeled(egui::WidgetType::Button, !busy, label.clone())
    });
    let ppp = ui.pixels_per_point();
    let rect = rect.round_to_pixels(ppp);
    let fill = if response.is_pointer_button_down_on() {
        p.widget_active
    } else if response.hovered() {
        p.widget_hover
    } else {
        p.widget
    };
    let radius = ICON_BUTTON * 0.5;
    ui.painter()
        .circle(rect.center(), radius, fill, Stroke::new(1.0, p.line));
    theme::focus_ring(ui, rect, ICON_BUTTON as u8 / 2, response.has_focus());
    if busy {
        egui::Spinner::new().size(14.0).color(p.muted).paint_at(
            ui,
            egui::Rect::from_center_size(rect.center(), egui::vec2(14.0, 14.0)),
        );
    } else {
        let ink = if response.hovered() { p.text } else { p.muted };
        let galley = ui.painter().layout_no_wrap(
            icons::glyph(icon).to_string(),
            theme::icon_font(14.0),
            ink,
        );
        ui.painter()
            .galley(rect.center() - galley.size() * 0.5, galley, ink);
    }
    response
}

/// The rounded search field with a magnifier glyph, macOS toolbar style.
fn search_field(ui: &mut egui::Ui, p: &theme::Palette, query: &mut String) {
    let height = ICON_BUTTON;
    let (rect, _) = ui.allocate_exact_size(egui::vec2(SEARCH_WIDTH, height), Sense::hover());
    let ppp = ui.pixels_per_point();
    let rect = rect.round_to_pixels(ppp);
    ui.painter().rect(
        rect,
        CornerRadius::same((height * 0.5) as u8),
        p.widget,
        Stroke::new(1.0, p.line),
        egui::StrokeKind::Inside,
    );
    let glyph = ui.painter().layout_no_wrap(
        icons::glyph(Icon::Search).to_string(),
        theme::icon_font(13.0),
        p.muted,
    );
    let glyph_x = rect.left() + theme::Space::S8.px();
    ui.painter().galley(
        egui::pos2(glyph_x, rect.center().y - glyph.size().y * 0.5),
        glyph,
        p.muted,
    );
    let field = egui::Rect::from_min_max(
        egui::pos2(glyph_x + 13.0 + theme::Space::S6.px(), rect.top() + 4.0),
        egui::pos2(rect.right() - theme::Space::S8.px(), rect.bottom() - 4.0),
    );
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(field)
            .layout(Layout::left_to_right(Align::Center)),
    );
    child.visuals_mut().extreme_bg_color = egui::Color32::TRANSPARENT;
    child.visuals_mut().widgets.inactive.bg_stroke = Stroke::NONE;
    child.visuals_mut().widgets.hovered.bg_stroke = Stroke::NONE;
    child.visuals_mut().widgets.active.bg_stroke = Stroke::NONE;
    child.visuals_mut().selection.stroke = Stroke::NONE;
    child.add(
        egui::TextEdit::singleline(query)
            .id_salt("toolbar-search")
            .hint_text("Search")
            .font(FontId::proportional(theme::SIZE_BODY))
            .desired_width(field.width())
            .frame(egui::Frame::new()),
    );
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
        let mut query = String::new();
        move |ui| {
            let p = crate::theme::palette(true);
            let model = ToolbarModel {
                title: "Secret Scanning".to_string(),
                subtitle: "Leaked credentials in commits are among the most exploited failure modes of AI-assisted coding."
                    .to_string(),
            };
            let mut state = ToolbarState {
                model: &model,
                checking,
                query: &mut query,
                left_inset: 0.0,
            };
            if let Some(event) = toolbar(ui, &p, &mut state) {
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
        h.get_by_label("Toggle sidebar").click();
        h.run_steps(2);
        assert_eq!(
            seen.lock().expect("sink").clone(),
            vec![
                HeaderEvent::Refresh,
                HeaderEvent::CheckUpdates,
                HeaderEvent::ToggleSidebar
            ]
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
