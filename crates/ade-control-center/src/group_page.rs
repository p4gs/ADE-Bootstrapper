//! A capability group page in the reference apps' three-pane shape: the
//! provider list in the middle, the inspector on the right, and the console
//! drawer along the bottom. Composes `provider_list`, `inspector` and
//! `console`, each of which renders a core view model; this file only decides
//! where the panes go and turns their events into engine commands.
//!
//! The panes are explicit rects carved out of the page, not nested egui
//! panels: a `Panel` shown inside another panel's `Ui` does not shrink that
//! `Ui`'s width for what follows it, which is exactly how the first render
//! put list rows underneath the inspector.

use crate::app::{EngineCmd, LocalState};
use crate::capability_page::{insights_block, stats_strip, CapabilityPageEvent};
use crate::console::{console, ConsoleEvent, CONSOLE_HEIGHT};
use crate::data::Shared;
use crate::inspector::inspector;
use crate::provider_list::{provider_list, ProviderListEvent};
use crate::theme;
use ade_core::gui::insights::Insight;
use ade_core::gui::inventory::{CapabilityGroup, LifecycleAction};
use ade_core::gui::verdict::HealthVerdict;
use ade_core::gui::views::{console_model, inspector_model, provider_list_model, ProviderAction};
use egui::{pos2, Rect, RichText, Sense, UiBuilder};
use std::collections::HashSet;

/// The inspector takes a share of the content width, bounded so it neither
/// starves the list on a small window nor sprawls on a wide one.
pub(crate) const INSPECTOR_MIN_WIDTH: f32 = 300.0;
pub(crate) const INSPECTOR_MAX_WIDTH: f32 = 380.0;
pub(crate) const INSPECTOR_SHARE: f32 = 0.42;

pub(crate) fn inspector_width(content_width: f32) -> f32 {
    (content_width * INSPECTOR_SHARE).clamp(INSPECTOR_MIN_WIDTH, INSPECTOR_MAX_WIDTH)
}

/// The drawer's full height on a tall page, but never more than ~38% of
/// the page: at the 560pt minimum window a fixed 200pt drawer left the
/// list one half-clipped row and the inspector its title alone.
pub(crate) fn console_height(page_height: f32) -> f32 {
    CONSOLE_HEIGHT.min(page_height * 0.38).max(120.0)
}

pub(crate) fn group_page(
    ui: &mut egui::Ui,
    p: &theme::Palette,
    snapshot: &Shared,
    local: &mut LocalState,
    group: &CapabilityGroup,
    verdict: &HealthVerdict,
    busy: &HashSet<String>,
) -> Vec<EngineCmd> {
    let mut cmds = Vec::new();

    // The selection must belong to THIS group; otherwise the first provider
    // is shown, so the inspector is never empty while the list has rows.
    let in_group: Vec<&str> = snapshot
        .capabilities
        .iter()
        .filter(|cap| cap.capability == group.id)
        .map(|cap| cap.id.as_str())
        .collect();
    let selected: Option<String> = local
        .selected_provider
        .as_deref()
        .filter(|id| in_group.contains(id))
        .or(in_group.first().copied())
        .map(str::to_string);

    let list = provider_list_model(
        group,
        &snapshot.capabilities,
        verdict,
        local.filter,
        selected.as_deref(),
        &local.query,
    );

    // Geometry: list | inspector on top, console drawer below both.
    let full = ui.available_rect_before_wrap();
    let console_visible = !local.console_hidden && !snapshot.jobs.is_empty();
    let content_bottom = if console_visible {
        full.bottom() - console_height(full.height())
    } else {
        full.bottom()
    };
    let split_x = full.right() - inspector_width(full.width());
    let list_rect = Rect::from_min_max(full.min, pos2(split_x, content_bottom));
    let inspector_rect = Rect::from_min_max(
        pos2(split_x, full.top()),
        pos2(full.right(), content_bottom),
    );
    let console_rect = Rect::from_min_max(pos2(full.left(), content_bottom), full.max);
    let ppp = ui.pixels_per_point();
    let hairline = egui::Stroke::new(1.0 / ppp, p.hairline);

    // Middle: the list, with the group's stats strip and insights above it.
    pane(ui, list_rect, |ui| {
        egui::Frame::new()
            .inner_margin(theme::margin(theme::Space::S16, theme::Space::S4))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                let dark_mode = ui.visuals().dark_mode;
                stats_strip(
                    ui,
                    p,
                    snapshot.group_stats.get(group.id),
                    snapshot.last_detection_at.map(|at| at.elapsed()),
                    dark_mode,
                );
                let group_insights: Vec<&Insight> = snapshot
                    .insights
                    .iter()
                    .filter(|insight| {
                        insight.group_id == group.id
                            && !snapshot.dismissed_insights.contains(&insight.id)
                    })
                    .collect();
                if let Some(CapabilityPageEvent::DismissInsight(id)) =
                    insights_block(ui, p, &group_insights, dark_mode)
                {
                    cmds.push(EngineCmd::DismissInsight(id));
                }
                match provider_list(ui, p, &list) {
                    Some(ProviderListEvent::Select(id)) => local.selected_provider = Some(id),
                    Some(ProviderListEvent::Filter(filter)) => local.filter = filter,
                    None => {}
                }
            });
    });

    // Right: the inspector, behind a vertical hairline.
    ui.painter().vline(
        theme::snap_y(split_x, ppp),
        inspector_rect.y_range(),
        hairline,
    );
    pane(ui, inspector_rect, |ui| {
        egui::Frame::new()
            .fill(p.bg)
            .inner_margin(theme::margin(theme::Space::S20, theme::Space::S12))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                let Some(id) = selected.as_deref() else {
                    ui.add_space(theme::Space::S24.px());
                    ui.vertical_centered(|ui| {
                        ui.label(
                            RichText::new("Select a provider")
                                .color(p.muted)
                                .size(theme::SIZE_BODY),
                        );
                    });
                    return;
                };
                let Some(cap) = snapshot.capabilities.iter().find(|cap| cap.id == id) else {
                    return;
                };
                let model = inspector_model(cap, busy.contains(id));
                match inspector(ui, p, &model) {
                    Some(ProviderAction::Lifecycle(LifecycleAction::Uninstall)) => {
                        // Destructive: the coverage-consequence confirm owns it.
                        local.confirm = Some((cap.id.clone(), LifecycleAction::Uninstall));
                    }
                    Some(ProviderAction::Lifecycle(action)) => cmds.push(EngineCmd::StartAction {
                        id: cap.id.clone(),
                        action,
                    }),
                    Some(ProviderAction::Enable) => cmds.push(EngineCmd::SetCapabilityEnabled {
                        id: cap.id.clone(),
                        enabled: true,
                    }),
                    Some(ProviderAction::Disable) => cmds.push(EngineCmd::SetCapabilityEnabled {
                        id: cap.id.clone(),
                        enabled: false,
                    }),
                    None => {}
                }
            });
    });

    // Bottom: the console drawer, only while there is a job to show and the
    // owner has not hidden it.
    if console_visible {
        ui.painter().hline(
            console_rect.x_range(),
            theme::snap_y(content_bottom, ppp),
            hairline,
        );
        let model = console_model(&snapshot.jobs, local.console_tab.as_deref());
        pane(ui, console_rect, |ui| {
            egui::Frame::new()
                .fill(p.panel)
                .inner_margin(theme::margin(theme::Space::S12, theme::Space::S8))
                .show(ui, |ui| {
                    ui.set_min_size(ui.available_size());
                    match console(ui, p, &model) {
                        Some(ConsoleEvent::SelectTab(id)) => local.console_tab = Some(id),
                        Some(ConsoleEvent::Copy) => ui.ctx().copy_text(model.lines.join("\n")),
                        Some(ConsoleEvent::Collapse) => local.console_hidden = true,
                        None => {}
                    }
                });
        });
    }

    // Claim the whole page so nothing below lays out into these rects.
    ui.allocate_rect(full, Sense::hover());
    cmds
}

/// A child `Ui` confined and clipped to `rect`.
fn pane(ui: &mut egui::Ui, rect: Rect, add: impl FnOnce(&mut egui::Ui)) {
    ui.scope_builder(UiBuilder::new().max_rect(rect), |ui| {
        ui.set_clip_rect(rect.intersect(ui.clip_rect()));
        add(ui);
    });
}
