//! ADE Control Center — the shell. Holds the engine and the per-window UI
//! state, composes the sidebar + content views (each a pure function in its
//! own module), and applies the events they report. No view logic lives here.
//!
//! Every actionable control reports a unique AccessKit label (ISC-187) so the
//! Interceptor macOS bridge can drive the app through the AX tree.

use crate::activity::{activity_view, ActivityEvent};
use crate::capability_page::{capability_page, CapabilityPageEvent, CapabilityPageState};
use crate::data::{lock_shared, Engine, Shared};
use crate::header::{toolbar, HeaderEvent};
use crate::modals::{
    bulk_modal, confirm_modal, confirmed_ids, default_selection, BulkEvent, ConfirmEvent,
};
use crate::nav::{sidebar, NavEvent, NavState};
use crate::overview::{overview, OverviewEvent, OverviewState};
use crate::projects::{projects_view, ProjectsEvent};
use crate::row::RowEvent;
use crate::theme;
use ade_core::gui::inventory::{get_group, CapabilityStatus, LifecycleAction};
use ade_core::gui::jobs::JobStatus;
use ade_core::gui::state::{ade_home_from_env, SCOPE_ACTIVITY, SCOPE_PROJECTS};
use ade_core::gui::verdict::{build_verdict, bulk_install_candidates};
use ade_core::types::{Finding, FindingLevel};
use egui::{Align, Color32, CornerRadius, Layout, RichText, Stroke, StrokeKind};
use std::collections::HashSet;
use std::sync::Arc;

pub struct ControlCenterApp {
    engine: Arc<Engine>,
    local: LocalState,
    /// The chrome tier actually achieved at launch (Phase E). When it is not
    /// `Opaque`, the sidebar panel paints a transparent fill so the native
    /// material behind the strip shows through.
    chrome_transparent: bool,
}

/// Per-window UI state that is not persisted: expansions and armed
/// confirmations. The navigation scope is NOT here — it lives in `Shared`,
/// mirrored from gui.json, so it survives a relaunch.
#[derive(Default)]
pub(crate) struct LocalState {
    pub project_input: String,
    /// A destructive action awaiting the coverage-consequence confirm modal.
    pub confirm: Option<(String, LifecycleAction)>,
    /// Project id awaiting a second click before ade is withdrawn from it.
    pub confirm_remove_ade: Option<String>,
    /// The bulk-install preview's selection; `Some` while the modal is open.
    pub bulk_selected: Option<HashSet<String>>,
    pub expanded_issues: HashSet<String>,
    pub expanded_jobs: HashSet<String>,
}

/// Everything the views can ask the engine to do. The frame is pure — it
/// draws and returns these; the shell applies them. A test can therefore
/// assert "clicking X asks for Y" with no engine at all.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum EngineCmd {
    SetScope(String),
    CheckUpdates,
    Refresh,
    SetCapabilityEnabled {
        id: String,
        enabled: bool,
    },
    StartAction {
        id: String,
        action: LifecycleAction,
    },
    RegisterProject(String),
    UnregisterProject(String),
    RemoveAde(String),
    LoadReport(String),
    SetModuleEnabled {
        project_id: String,
        module_id: String,
        enabled: bool,
    },
    DismissToast,
    /// Phase H: persist that this insight has been seen and stop showing it.
    DismissInsight(String),
    /// Phase I: write the posture evidence report to `$ADE_HOME/export/`.
    ExportPosture,
}

impl ControlCenterApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        theme::apply(&cc.egui_ctx);
        let tier = crate::chrome::attach_sidebar_chrome(cc);
        let engine = Engine::new(ade_home_from_env());
        let ctx = cc.egui_ctx.clone();
        engine.start_poll(move || ctx.request_repaint());
        ControlCenterApp {
            engine,
            local: LocalState::default(),
            chrome_transparent: tier.wants_transparent_sidebar(),
        }
    }

    fn repaint(ctx: &egui::Context) -> impl Fn() + Send + 'static {
        let ctx = ctx.clone();
        move || ctx.request_repaint()
    }

    fn dispatch(&self, cmd: EngineCmd, ctx: &egui::Context) {
        match cmd {
            EngineCmd::SetScope(scope) => self.engine.set_scope(&scope),
            EngineCmd::CheckUpdates => self.engine.check_updates(Self::repaint(ctx)),
            EngineCmd::Refresh => self.engine.request_refresh(),
            EngineCmd::SetCapabilityEnabled { id, enabled } => {
                self.engine.set_capability_enabled(&id, enabled)
            }
            EngineCmd::StartAction { id, action } => self.engine.start_action(&id, action),
            EngineCmd::RegisterProject(dir) => {
                self.engine.register_project(dir, Self::repaint(ctx))
            }
            EngineCmd::UnregisterProject(id) => {
                self.engine.unregister_project(id, Self::repaint(ctx))
            }
            EngineCmd::RemoveAde(id) => self.engine.remove_ade_from_project(id, Self::repaint(ctx)),
            EngineCmd::LoadReport(id) => self.engine.load_report(id, Self::repaint(ctx)),
            EngineCmd::SetModuleEnabled {
                project_id,
                module_id,
                enabled,
            } => self
                .engine
                .set_module_enabled(project_id, module_id, enabled, Self::repaint(ctx)),
            EngineCmd::DismissToast => self.engine.dismiss_toast(),
            EngineCmd::DismissInsight(id) => self.engine.dismiss_insight(&id),
            EngineCmd::ExportPosture => self.engine.export_posture(Self::repaint(ctx)),
        }
    }
}

impl eframe::App for ControlCenterApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let dark = ui.visuals().dark_mode;
        let p = theme::palette(dark);

        // One clone under the lock: a field added to Shared can never again be
        // silently absent from the frame the views render. Poison-recovering:
        // this runs as the FIRST statement of every single frame, so a
        // panicked background thread must never be able to wedge every
        // future frame blank by poisoning this lock.
        let snapshot: Shared = lock_shared(&self.engine.shared).clone();

        let ctx = ui.ctx().clone();
        for cmd in frame(ui, &p, &snapshot, &mut self.local, self.chrome_transparent) {
            self.dispatch(cmd, &ctx);
        }
    }

    /// With a native material behind the sidebar strip, the GL surface must
    /// clear to transparent there — the panels paint every opaque pixel the
    /// design wants. On the Opaque tier the clear color is the content plane,
    /// exactly what an unpainted edge should read as.
    fn clear_color(&self, visuals: &egui::Visuals) -> [f32; 4] {
        if self.chrome_transparent {
            [0.0, 0.0, 0.0, 0.0]
        } else {
            theme::palette(visuals.dark_mode)
                .bg
                .to_normalized_gamma_f32()
        }
    }
}

/// One whole frame: sidebar, toolbar, and the single scope the sidebar has
/// selected. Pure — the same function the shell calls is the one the
/// whole-screen snapshot renders.
pub(crate) fn frame(
    ui: &mut egui::Ui,
    p: &theme::Palette,
    snapshot: &Shared,
    local: &mut LocalState,
    chrome_transparent: bool,
) -> Vec<EngineCmd> {
    let mut cmds = Vec::new();
    let verdict = build_verdict(&snapshot.capabilities);
    // The bulk-preview population, computed once per frame from the same
    // snapshot everything else renders.
    let bulk = bulk_install_candidates(&snapshot.capabilities);
    let bulk_installable = bulk
        .iter()
        .filter(|candidate| candidate.automatable())
        .count();
    let running_jobs = snapshot
        .jobs
        .iter()
        .filter(|job| job.status == JobStatus::Running)
        .count();
    // Which capabilities have a job in flight right now — computed once per
    // frame from the same snapshot, and shared by both the capability-page
    // rows and the Overview's attention actions (item 8: an Overview action
    // used to give zero feedback while its job ran).
    let busy: HashSet<String> = snapshot
        .jobs
        .iter()
        .filter(|job| job.status == JobStatus::Running)
        .map(|job| job.capability_id.clone())
        .collect();

    // Glass (Phase E): when a native material sits behind the sidebar strip,
    // the panel paints NO fill of its own — the material is the surface, and
    // text/washes composite on top. Opaque tier keeps the step-2 fill.
    let sidebar_fill = if chrome_transparent {
        Color32::TRANSPARENT
    } else {
        p.panel
    };
    egui::Panel::left("nav")
        .exact_size(theme::SIDEBAR_WIDTH)
        .resizable(false)
        .frame(
            egui::Frame::new()
                .fill(sidebar_fill)
                .inner_margin(theme::margin(theme::Space::S8, theme::Space::S8)),
        )
        .show(ui, |ui| {
            let nav_state = NavState {
                scope: &snapshot.scope,
                verdict: &verdict,
                capabilities: &snapshot.capabilities,
                running_jobs,
                refreshing: snapshot.detecting || snapshot.checking_updates,
                checked_ago: snapshot.last_detection_at.map(|at| at.elapsed()),
            };
            if let Some(NavEvent::Select(scope)) = sidebar(ui, p, &nav_state) {
                cmds.push(EngineCmd::SetScope(scope));
            }
        });

    egui::CentralPanel::default_margins()
        .frame(
            egui::Frame::new()
                .fill(p.bg)
                .inner_margin(theme::margin(theme::Space::S16, theme::Space::S12)),
        )
        .show(ui, |ui| {
            if let Some(event) = toolbar(ui, p, snapshot.checking_updates) {
                cmds.push(match event {
                    HeaderEvent::CheckUpdates => EngineCmd::CheckUpdates,
                    HeaderEvent::Refresh => EngineCmd::Refresh,
                });
            }
            ui.add_space(theme::Space::S6.px());

            if let Some(toast) = &snapshot.toast {
                egui::Frame::new()
                    .fill(p.info.gamma_multiply(0.12))
                    .corner_radius(CornerRadius::same(theme::RADIUS_CONTROL))
                    .inner_margin(theme::margin(theme::Space::S12, theme::Space::S8))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(toast).size(theme::SIZE_BODY));
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                if ax_button(ui, "Dismiss", "Dismiss notice", None, true).clicked()
                                {
                                    cmds.push(EngineCmd::DismissToast);
                                }
                            });
                        });
                    });
                ui.add_space(theme::Space::S8.px());
            }
            if let Some(warning) = &snapshot.state_warning {
                ui.label(
                    RichText::new(warning)
                        .color(p.warn)
                        .size(theme::SIZE_CAPTION),
                );
                ui.add_space(theme::Space::S6.px());
            }

            // Scopes with their own designed pre-detection state skip the
            // centered spinner: the Overview's attention box has a checking
            // state (Phase C), so the landing page is never a blocking screen.
            let scope_resolves_elsewhere = snapshot.scope == SCOPE_PROJECTS
                || snapshot.scope == SCOPE_ACTIVITY
                || get_group(&snapshot.scope).is_some();
            if !snapshot.have_first_detection && scope_resolves_elsewhere {
                // Cold start (ISC-191): a loading state, never a blank window.
                ui.add_space(120.0);
                ui.vertical_centered(|ui| {
                    ui.add(egui::Spinner::new().size(28.0));
                    ui.add_space(theme::Space::S8.px());
                    ui.label(
                        RichText::new("Scanning capabilities…")
                            .font(theme::medium(theme::SIZE_SECTION)),
                    );
                    ui.label(
                        RichText::new("Detecting installed tools and harnesses on this machine")
                            .color(p.muted)
                            .size(theme::SIZE_CAPTION),
                    );
                });
                return;
            }

            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    let scope = snapshot.scope.as_str();
                    if scope == SCOPE_PROJECTS {
                        if let Some(event) = projects_view(
                            ui,
                            p,
                            snapshot,
                            &mut local.project_input,
                            &local.confirm_remove_ade,
                        ) {
                            apply_projects_event(event, local, &mut cmds);
                        }
                    } else if scope == SCOPE_ACTIVITY {
                        if let Some(ActivityEvent::ToggleLog(job_id)) =
                            activity_view(ui, p, &snapshot.jobs, &local.expanded_jobs)
                        {
                            toggle_key(&mut local.expanded_jobs, format!("job-{job_id}"));
                        }
                    } else if let Some(group) = get_group(scope) {
                        let caps: Vec<&CapabilityStatus> = snapshot
                            .capabilities
                            .iter()
                            .filter(|cap| cap.capability == group.id)
                            .collect();
                        let state = CapabilityPageState {
                            group,
                            caps,
                            busy: &busy,
                            expanded_issues: &local.expanded_issues,
                            stats: snapshot.group_stats.get(group.id),
                            checked_ago: snapshot.last_detection_at.map(|at| at.elapsed()),
                            insights: &snapshot.insights,
                            dismissed_insights: &snapshot.dismissed_insights,
                        };
                        if let Some(event) = capability_page(ui, p, &state) {
                            apply_capability_event(event, local, &mut cmds);
                        }
                    } else {
                        // The Overview — also the fallback for a scope that no
                        // longer resolves (a capability retired between runs).
                        let state = OverviewState {
                            verdict: &verdict,
                            checking: snapshot.detecting && !snapshot.have_first_detection,
                            // detect_capabilities is total: probe failures and
                            // timeouts become Findings on their capability, so
                            // the engine has no pass-level failure to report.
                            // The renderer exists (tested); this stays None
                            // until a real producer does.
                            check_failed: None,
                            bulk_installable,
                            busy: &busy,
                            insights: &snapshot.insights,
                            dismissed: &snapshot.dismissed_insights,
                        };
                        match overview(ui, p, &state) {
                            Some(OverviewEvent::Start {
                                capability_id,
                                action,
                            }) => cmds.push(EngineCmd::StartAction {
                                id: capability_id,
                                action,
                            }),
                            Some(OverviewEvent::Open(group_id)) => {
                                cmds.push(EngineCmd::SetScope(group_id))
                            }
                            Some(OverviewEvent::Retry) => cmds.push(EngineCmd::Refresh),
                            Some(OverviewEvent::AskBulkInstall) => {
                                local.bulk_selected = Some(default_selection(&bulk));
                            }
                            Some(OverviewEvent::DismissInsight(id)) => {
                                cmds.push(EngineCmd::DismissInsight(id));
                            }
                            Some(OverviewEvent::ExportReport) => {
                                cmds.push(EngineCmd::ExportPosture);
                            }
                            None => {}
                        }
                    }
                });
        });

    // The modal trust surfaces, over everything. Rendered last so the frame
    // beneath them is the one the owner was just looking at.
    if let Some((id, action)) = local.confirm.clone() {
        match confirm_modal(ui.ctx(), p, &snapshot.capabilities, &id, action) {
            Some(ConfirmEvent::Confirm) => {
                local.confirm = None;
                cmds.push(EngineCmd::StartAction { id, action });
            }
            Some(ConfirmEvent::Cancel) => local.confirm = None,
            None => {}
        }
    }
    if let Some(selected) = local.bulk_selected.clone() {
        match bulk_modal(ui.ctx(), p, &bulk, &selected) {
            Some(BulkEvent::Toggle(id)) => {
                if let Some(set) = local.bulk_selected.as_mut() {
                    toggle_key(set, id);
                }
            }
            Some(BulkEvent::Confirm) => {
                for id in confirmed_ids(&bulk, &selected) {
                    cmds.push(EngineCmd::StartAction {
                        id,
                        action: LifecycleAction::Install,
                    });
                }
                local.bulk_selected = None;
            }
            Some(BulkEvent::Cancel) => local.bulk_selected = None,
            None => {}
        }
    }

    cmds
}

fn toggle_key(set: &mut HashSet<String>, key: String) {
    if !set.remove(&key) {
        set.insert(key);
    }
}

fn apply_capability_event(
    event: CapabilityPageEvent,
    local: &mut LocalState,
    cmds: &mut Vec<EngineCmd>,
) {
    match event {
        CapabilityPageEvent::Row {
            capability_id,
            event,
        } => apply_row_event(capability_id, event, local, cmds),
        CapabilityPageEvent::DismissInsight(id) => cmds.push(EngineCmd::DismissInsight(id)),
    }
}

fn apply_row_event(id: String, event: RowEvent, local: &mut LocalState, cmds: &mut Vec<EngineCmd>) {
    match event {
        RowEvent::SetEnabled(enabled) => cmds.push(EngineCmd::SetCapabilityEnabled { id, enabled }),
        RowEvent::Start(action) => {
            local.confirm = None;
            cmds.push(EngineCmd::StartAction { id, action });
        }
        // Arm the coverage-consequence confirm modal; the row never runs a
        // destructive action itself.
        RowEvent::AskConfirm(action) => local.confirm = Some((id, action)),
        RowEvent::ToggleIssues => {
            toggle_key(&mut local.expanded_issues, format!("issues-{id}"));
        }
    }
}

fn apply_projects_event(event: ProjectsEvent, local: &mut LocalState, cmds: &mut Vec<EngineCmd>) {
    match event {
        ProjectsEvent::Register(dir) => cmds.push(EngineCmd::RegisterProject(dir)),
        ProjectsEvent::Forget(id) => cmds.push(EngineCmd::UnregisterProject(id)),
        ProjectsEvent::AskRemoveAde(id) => local.confirm_remove_ade = Some(id),
        ProjectsEvent::CancelRemoveAde => local.confirm_remove_ade = None,
        ProjectsEvent::ConfirmRemoveAde(id) => {
            local.confirm_remove_ade = None;
            cmds.push(EngineCmd::RemoveAde(id));
        }
        ProjectsEvent::Inspect(id) => cmds.push(EngineCmd::LoadReport(id)),
        ProjectsEvent::SetModuleEnabled {
            project_id,
            module_id,
            enabled,
        } => cmds.push(EngineCmd::SetModuleEnabled {
            project_id,
            module_id,
            enabled,
        }),
        ProjectsEvent::OpenGroup(group_id) => cmds.push(EngineCmd::SetScope(group_id)),
    }
}

// ───────────────────────── level/status helpers ─────────────────────────

pub(crate) fn worst_level(issues: &[Finding]) -> FindingLevel {
    issues
        .iter()
        .map(|issue| match issue.level {
            FindingLevel::Degraded => FindingLevel::Warn,
            other => other,
        })
        .max()
        .unwrap_or(FindingLevel::Ok)
}

#[derive(Clone, Copy, PartialEq)]
pub(crate) enum Dot {
    Ok,
    Warn,
    Err,
    Missing,
    Disabled,
}

pub(crate) fn cap_dot(cap: &CapabilityStatus) -> Dot {
    if !cap.enabled {
        return Dot::Disabled;
    }
    if !cap.installed {
        return Dot::Missing;
    }
    match worst_level(&cap.issues) {
        FindingLevel::Error => Dot::Err,
        FindingLevel::Warn | FindingLevel::Degraded => Dot::Warn,
        _ => Dot::Ok,
    }
}

fn level_name(level: FindingLevel) -> &'static str {
    match level {
        FindingLevel::Ok => "OK",
        FindingLevel::Info => "INFO",
        FindingLevel::Warn => "WARN",
        FindingLevel::Degraded => "DEGRADED",
        FindingLevel::Error => "ERROR",
    }
}

pub(crate) fn level_color(p: &theme::Palette, level: FindingLevel) -> Color32 {
    match level {
        FindingLevel::Error => p.err,
        FindingLevel::Warn | FindingLevel::Degraded => p.warn,
        FindingLevel::Info => p.info,
        FindingLevel::Ok => p.ok,
    }
}

// ───────────────────────── small widgets ─────────────────────────

pub(crate) fn status_dot(ui: &mut egui::Ui, dot: Dot, p: &theme::Palette) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(12.0, 12.0), egui::Sense::hover());
    status_dot_at(ui, rect, dot, p);
}

/// Paint a status mark into an already-allocated rect — for callers that
/// reserve the slot unconditionally (so names align) and paint conditionally
/// (so healthy stays silent). The vocabulary is the painted icon set:
/// attention is a filled circle, broken is the rounded-join warning triangle
/// (never U+26A0 — macOS renders that as an orange emoji), missing is the
/// open circle-dot "empty slot" mark.
pub(crate) fn status_dot_at(ui: &egui::Ui, rect: egui::Rect, dot: Dot, p: &theme::Palette) {
    let center = rect.center();
    let icon =
        egui::Rect::from_center_size(center, egui::Vec2::splat(crate::icons::IconSize::S10.px()));
    match dot {
        Dot::Missing => {
            crate::icons::circle_dot(ui, icon, p.muted);
        }
        Dot::Disabled => {
            ui.painter().circle_filled(center, 4.7, p.disabled_text);
        }
        Dot::Ok => {
            ui.painter().circle_filled(center, 4.7, p.ok);
        }
        Dot::Warn => {
            ui.painter().circle_filled(center, 4.7, p.warn);
        }
        Dot::Err => {
            crate::icons::warning_triangle(ui, icon, p.err);
        }
    }
}

/// The capability an item provides — an eyebrow, not a badge.
///
/// It was a filled, outlined box: a container competing with the row's real
/// content for attention when it is only a label saying what kind of thing this
/// is. Uppercase, small and faint says the same thing and asks for nothing.
pub(crate) fn capability_label(ui: &mut egui::Ui, p: &theme::Palette, name: &str, why: &str) {
    ui.add(
        egui::Label::new(
            RichText::new(name.to_uppercase())
                .color(p.faint_text)
                .font(theme::medium(9.0)),
        )
        .truncate(),
    )
    .on_hover_text(why);
}

/// A separator that suggests rather than rules. `ui.separator()` drew a full
/// grid line between every row, which is a lot of ink for "these are different
/// items" when uniform height and a hover state already say it.
pub(crate) fn row_hairline(ui: &mut egui::Ui, p: &theme::Palette) {
    let (rect, _) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), 1.0), egui::Sense::hover());
    // Snapped to the physical pixel grid: a hairline at a fractional device
    // coordinate blurs across two pixels, which reads as smudge, not line.
    let y = theme::snap_y(rect.center().y, ui.ctx().pixels_per_point());
    ui.painter().hline(
        (rect.left() + 22.0)..=rect.right(),
        y,
        Stroke::new(1.0, p.hairline),
    );
}

pub(crate) fn chip(ui: &mut egui::Ui, text: &str, color: Color32) {
    egui::Frame::new()
        .fill(color.gamma_multiply(0.16))
        .corner_radius(CornerRadius::same(10))
        .inner_margin(egui::Margin::symmetric(8, 3))
        .show(ui, |ui| {
            ui.label(RichText::new(text).color(color).size(11.0));
        });
}

pub(crate) fn toggle(
    ui: &mut egui::Ui,
    on: &mut bool,
    p: &theme::Palette,
    ax_label: &str,
) -> egui::Response {
    let size = egui::vec2(36.0, 20.0);
    let (rect, mut response) = ui.allocate_exact_size(size, egui::Sense::click());
    if response.clicked() {
        *on = !*on;
        response.mark_changed();
    }
    let label = ax_label.to_string();
    let selected = *on;
    response.widget_info(move || {
        egui::WidgetInfo::selected(egui::WidgetType::Checkbox, true, selected, label.clone())
    });
    if ui.is_rect_visible(rect) {
        let how_on = ui.ctx().animate_bool_responsive(response.id, *on);
        let bg = Color32::from_gray(if ui.visuals().dark_mode { 70 } else { 190 })
            .lerp_to_gamma(p.ok, how_on);
        let radius = 0.5 * rect.height();
        ui.painter()
            .rect(rect, radius, bg, Stroke::NONE, StrokeKind::Inside);
        let circle_x = egui::lerp((rect.left() + radius)..=(rect.right() - radius), how_on);
        ui.painter().circle_filled(
            egui::pos2(circle_x, rect.center().y),
            radius - 2.5,
            Color32::WHITE,
        );
    }
    response
}

/// A secondary action: same hit target and same accessibility label as a normal
/// button, without the filled slab. Three equal-weight buttons per row gave
/// every action the same shout; the destructive one should not compete with the
/// one you actually want.
///
/// Hand-painted rather than a stock frameless `egui::Button`: the latter had
/// literally no hover/press feedback at all (an explicit `RichText` color
/// suppresses the widget's own text-color resolution, and `frame(false)`
/// suppresses its fill), while every hand-painted row in the app fades over
/// `DURATION_HOVER`. The text strength now uses the exact curve
/// `ellipsis_button`/the coverage chevron use — rest already clears the AA
/// floor, hover brightens to full strength, a press settles just under it so
/// the click still reads.
pub(crate) fn ax_ghost_button(
    ui: &mut egui::Ui,
    text: &str,
    ax_label: &str,
    color: Color32,
) -> egui::Response {
    let font = egui::FontId::proportional(12.0);
    let sizing = ui
        .painter()
        .layout_no_wrap(text.to_string(), font.clone(), color);
    let size = egui::vec2(sizing.size().x, sizing.size().y.max(26.0));
    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click());
    let label = ax_label.to_string();
    response.widget_info(move || {
        egui::WidgetInfo::labeled(egui::WidgetType::Button, true, label.clone())
    });
    if ui.is_rect_visible(rect) {
        let hover =
            ui.ctx()
                .animate_bool_with_time(response.id, response.hovered(), theme::DURATION_HOVER);
        let press = ui.ctx().animate_bool_with_time(
            response.id.with("press"),
            response.is_pointer_button_down_on(),
            theme::DURATION_HOVER,
        );
        let strength = (0.85 + 0.15 * hover) * (1.0 - 0.1 * press);
        let tinted = color.gamma_multiply(strength);
        let galley = ui.painter().layout_no_wrap(text.to_string(), font, tinted);
        let pos = egui::pos2(rect.left(), rect.center().y - galley.size().y * 0.5);
        ui.painter().galley(pos, galley, tinted);
    }
    response
}

/// The one filled/bordered button shape in the app. Hand-painted for the same
/// reason as `ax_ghost_button`: an `egui::Button` resolves its hover/press
/// fill through `Visuals` state, which snaps instantly, while every
/// hand-painted control here fades. `egui::Button` still owns text layout,
/// sizing, the disabled path, and the AccessKit node — only the fill is
/// intercepted, via the same reserve-then-set-behind trick `row.rs`'s hover
/// backdrop already uses, so the animated color paints BEHIND the (fully
/// transparent) stock button rather than replacing it.
pub(crate) fn ax_button(
    ui: &mut egui::Ui,
    text: &str,
    ax_label: &str,
    fill: Option<Color32>,
    enabled: bool,
) -> egui::Response {
    let rich = match fill {
        Some(_) => RichText::new(text).color(Color32::WHITE),
        None => RichText::new(text),
    };
    let backdrop = ui.painter().add(egui::Shape::Noop);
    let button = egui::Button::new(rich).fill(Color32::TRANSPARENT);
    let response = ui.add_enabled(enabled, button);
    let label = ax_label.to_string();
    response.widget_info(move || {
        egui::WidgetInfo::labeled(egui::WidgetType::Button, true, label.clone())
    });
    if !ui.is_rect_visible(response.rect) {
        return response;
    }
    let radius = CornerRadius::same(theme::RADIUS_CONTROL);
    let widget_fill = if enabled {
        let widget = ui.visuals().widgets.inactive.bg_fill;
        let widget_hover = ui.visuals().widgets.hovered.bg_fill;
        let widget_active = ui.visuals().widgets.active.bg_fill;
        let hover =
            ui.ctx()
                .animate_bool_with_time(response.id, response.hovered(), theme::DURATION_HOVER);
        let press = ui.ctx().animate_bool_with_time(
            response.id.with("press"),
            response.is_pointer_button_down_on(),
            theme::DURATION_HOVER,
        );
        fill.unwrap_or(widget)
            .lerp_to_gamma(widget_hover, hover)
            .lerp_to_gamma(widget_active, press)
    } else {
        // Disabled: `add_enabled` restores full painter opacity before
        // returning, so the dimming has to be applied by hand here too —
        // the exact flat, unanimated fill the stock resolution used to draw.
        ui.visuals()
            .disable(fill.unwrap_or(ui.visuals().widgets.inactive.bg_fill))
    };
    ui.painter().set(
        backdrop,
        egui::epaint::RectShape::filled(response.rect, radius, widget_fill),
    );
    response
}

pub(crate) fn issue_line(ui: &mut egui::Ui, issue: &Finding, p: &theme::Palette) {
    ui.horizontal_wrapped(|ui| {
        ui.label(
            RichText::new(level_name(issue.level))
                .color(level_color(p, issue.level))
                .size(10.5)
                .strong(),
        );
        ui.label(RichText::new(&issue.message).size(12.0));
        if let Some(remediation) = &issue.remediation {
            ui.label(
                RichText::new(format!("— {remediation}"))
                    .color(p.muted)
                    .size(12.0),
            );
        }
    });
}

/// Keep the content in a readable column. Stretched to a wide window the
/// row put a name on the far left and a button on the far right with a
/// void between them, which is most of why the screen read as broken.
pub(crate) fn content_column<R>(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    const MAX_CONTENT: f32 = 1020.0;
    let width = ui.available_width().min(MAX_CONTENT);
    let gutter = ((ui.available_width() - width) / 2.0).max(0.0);
    ui.horizontal(|ui| {
        ui.add_space(gutter);
        ui.allocate_ui_with_layout(
            egui::vec2(width, ui.available_height()),
            Layout::top_down(Align::Min),
            add,
        )
        .inner
    })
    .inner
}

pub(crate) fn capitalize(action: LifecycleAction) -> &'static str {
    match action {
        LifecycleAction::Install => "Install",
        LifecycleAction::Uninstall => "Uninstall",
        LifecycleAction::Reinstall => "Reinstall",
        LifecycleAction::Update => "Update",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testkit::{cap, harness_themed};
    use ade_core::gui::inventory::CAPABILITIES;
    use egui_kittest::kittest::Queryable;

    /// A realistic full snapshot of shared state: one coverage gap, one
    /// running job, a selected capability scope.
    fn shared(scope: &str) -> Shared {
        let mut caps: Vec<CapabilityStatus> = CAPABILITIES.iter().map(|def| cap(def.id)).collect();
        let hole = caps.iter_mut().find(|c| c.id == "nono").unwrap();
        hole.installed = false;
        hole.version = None;
        Shared {
            capabilities: caps,
            have_first_detection: true,
            scope: scope.to_string(),
            // NOT `Some(Instant::now())`: `checked_ago` is read via
            // `.elapsed()` on every render frame, including the frames a
            // snapshot test captures, so a live `Instant` makes the
            // freshness-footer text ("Checked Ns ago") drift by however long
            // the render actually took — a `.diff.png` of one differing digit
            // that reproduced under `--workspace`'s heavier parallel load but
            // not in isolation (visible through the modal backdrop, at the
            // bottom-left of every screen/modal snapshot). `None` renders the
            // idle icon with a fixed "Checking…" caption instead — no
            // wall-clock read, so nothing here can drift between the run that
            // records a snapshot and the run that verifies it.
            last_detection_at: None,
            ..Shared::default()
        }
    }

    /// Mark the given capabilities missing in a fixture.
    fn make_missing(shared: &mut Shared, ids: &[&str]) {
        for id in ids {
            let gone = shared
                .capabilities
                .iter_mut()
                .find(|cap| &cap.id == id)
                .unwrap();
            gone.installed = false;
            gone.version = None;
        }
    }

    /// The full frame with an explicit starting fixture and local state — the
    /// modal tests pre-arm `LocalState` exactly as a click would.
    ///
    /// Chrome honesty: every test renders `chrome_transparent = false` (the
    /// Opaque tier). kittest paints egui alone — it cannot see native AppKit
    /// views — so the transparent-sidebar path would snapshot as a hole where
    /// the real window shows glass. The Opaque path IS the path these
    /// snapshots can verify; the glass path is verified against the live
    /// window (Phase E capture protocol).
    fn paint_with(
        shared: Shared,
        dark: bool,
        local: LocalState,
        sink: std::sync::Arc<std::sync::Mutex<Vec<EngineCmd>>>,
    ) -> impl FnMut(&mut egui::Ui) + 'static {
        let mut local = local;
        move |ui| {
            let p = crate::theme::palette(dark);
            let cmds = frame(ui, &p, &shared, &mut local, false);
            sink.lock().expect("sink").extend(cmds);
        }
    }

    fn paint(
        scope: &'static str,
        dark: bool,
        sink: std::sync::Arc<std::sync::Mutex<Vec<EngineCmd>>>,
    ) -> impl FnMut(&mut egui::Ui) + 'static {
        paint_with(shared(scope), dark, LocalState::default(), sink)
    }

    /// The whole screen at 900pt — sidebar, toolbar, and a capability page —
    /// rendered through the SAME `frame` function the app calls. The previous
    /// screen snapshot drew its heading by hand and was evidence about
    /// nothing.
    #[test]
    fn snapshot_the_whole_screen_at_900() {
        let sink = std::sync::Arc::default();
        let mut h = harness_themed(
            egui::vec2(900.0, 560.0),
            true,
            paint("secret-scanning", true, sink),
        );
        h.snapshot("screen_900");
    }

    /// The Overview is both the landing scope and the fallback for anything
    /// that no longer resolves.
    #[test]
    fn snapshot_the_whole_screen_on_the_overview() {
        let sink = std::sync::Arc::default();
        let mut h = harness_themed(
            egui::vec2(900.0, 720.0),
            false,
            paint("overview", false, sink),
        );
        h.snapshot("screen_overview_light");
    }

    #[test]
    fn snapshot_the_whole_screen_on_the_overview_dark() {
        let sink = std::sync::Arc::default();
        let mut h = harness_themed(
            egui::vec2(900.0, 720.0),
            true,
            paint("overview", true, sink),
        );
        h.snapshot("screen_overview_dark");
    }

    /// Overview actions flow through the same command channel as everything
    /// else: an attention button asks for the lifecycle action the verdict
    /// computed, and a coverage row asks for a scope change.
    #[test]
    fn overview_events_flow_to_the_engine_as_commands() {
        let seen: std::sync::Arc<std::sync::Mutex<Vec<EngineCmd>>> = Default::default();
        let sink = std::sync::Arc::clone(&seen);
        let mut h = harness_themed(
            egui::vec2(900.0, 720.0),
            true,
            paint("overview", true, sink),
        );
        h.get_by_label("Install nono").click();
        h.run_steps(2);
        h.get_by_label("Open Secret Scanning").click();
        h.run_steps(2);
        assert_eq!(
            seen.lock().expect("sink").clone(),
            vec![
                EngineCmd::StartAction {
                    id: "nono".to_string(),
                    action: LifecycleAction::Install,
                },
                EngineCmd::SetScope("secret-scanning".to_string()),
            ]
        );
    }

    fn paint_cold(
        scope: &'static str,
        sink: std::sync::Arc<std::sync::Mutex<Vec<EngineCmd>>>,
    ) -> impl FnMut(&mut egui::Ui) + 'static {
        let shared = Shared {
            have_first_detection: false,
            detecting: true,
            scope: scope.to_string(),
            ..Shared::default()
        };
        let mut local = LocalState::default();
        move |ui| {
            let p = crate::theme::palette(true);
            let cmds = frame(ui, &p, &shared, &mut local, false);
            sink.lock().expect("sink").extend(cmds);
        }
    }

    /// Cold start on the Overview renders the designed checking state, not the
    /// blocking spinner screen — and a scope with no such state still gets the
    /// spinner screen (the positive control for the absence).
    #[test]
    fn cold_start_routes_the_overview_to_its_checking_state() {
        let sink = std::sync::Arc::default();
        let h = harness_themed(egui::vec2(900.0, 560.0), true, paint_cold("overview", sink));
        // The headline and the box line share this text pre-detection, so
        // count them rather than expecting a unique node.
        assert!(h
            .get_all_by_label("Checking your environment…")
            .next()
            .is_some());
        assert!(h.query_by_label("Scanning capabilities…").is_none());

        let sink = std::sync::Arc::default();
        let h = harness_themed(
            egui::vec2(900.0, 560.0),
            true,
            paint_cold("secret-scanning", sink),
        );
        assert!(h.query_by_label("Scanning capabilities…").is_some());
    }

    /// Navigation goes through the engine command channel — clicking a nav
    /// row asks for a scope change and mutates nothing itself.
    #[test]
    fn choosing_a_scope_is_a_command_not_a_mutation() {
        let seen: std::sync::Arc<std::sync::Mutex<Vec<EngineCmd>>> = Default::default();
        let sink = std::sync::Arc::clone(&seen);
        let mut h = harness_themed(
            egui::vec2(900.0, 560.0),
            true,
            paint("overview", false, sink),
        );
        h.get_by_label("Projects").click();
        h.run_steps(2);
        assert_eq!(
            seen.lock().expect("sink").clone(),
            vec![EngineCmd::SetScope("projects".to_string())]
        );
    }

    /// The whole destructive path (ISC: confirm modal names the capability
    /// and states the coverage consequence): row menu → Uninstall → the modal
    /// appears over the page → confirming emits the start and closes it.
    #[test]
    fn uninstall_goes_through_the_coverage_consequence_modal() {
        let seen: std::sync::Arc<std::sync::Mutex<Vec<EngineCmd>>> = Default::default();
        let sink = std::sync::Arc::clone(&seen);
        let mut h = harness_themed(
            egui::vec2(900.0, 560.0),
            true,
            paint("secret-scanning", true, sink),
        );
        // Closed at rest — the destructive path is two deliberate clicks away.
        assert!(h.query_by_label("Confirm uninstall trufflehog").is_none());
        h.get_by_label("More actions trufflehog").click();
        h.run_steps(2);
        h.get_by_label("Uninstall trufflehog").click();
        // A centered Area sizes itself on its first frame and settles on the
        // next; clicking against the pre-settle rect misses. One extra step.
        h.run_steps(3);
        // The modal is up, with both of its labelled actions.
        assert!(h.query_by_label("Confirm uninstall trufflehog").is_some());
        assert!(h.query_by_label("Cancel uninstall trufflehog").is_some());
        // Nothing has been asked of the engine yet.
        assert!(seen.lock().expect("sink").is_empty());
        h.get_by_label("Confirm uninstall trufflehog").click();
        h.run_steps(2);
        assert_eq!(
            seen.lock().expect("sink").clone(),
            vec![EngineCmd::StartAction {
                id: "trufflehog".to_string(),
                action: LifecycleAction::Uninstall,
            }]
        );
        assert!(
            h.query_by_label("Confirm uninstall trufflehog").is_none(),
            "the modal closes once the action is handed to the engine"
        );
    }

    /// Cancel leaves everything unchanged: no command, no modal, and the row
    /// is back to rest.
    #[test]
    fn cancelling_the_confirm_modal_changes_nothing() {
        let seen: std::sync::Arc<std::sync::Mutex<Vec<EngineCmd>>> = Default::default();
        let sink = std::sync::Arc::clone(&seen);
        let mut h = harness_themed(
            egui::vec2(900.0, 560.0),
            true,
            paint("secret-scanning", true, sink),
        );
        h.get_by_label("More actions trufflehog").click();
        h.run_steps(2);
        h.get_by_label("Uninstall trufflehog").click();
        // Settle frame for the centering Area (see the walk test above).
        h.run_steps(3);
        h.get_by_label("Cancel uninstall trufflehog").click();
        h.run_steps(2);
        assert!(seen.lock().expect("sink").is_empty());
        assert!(h.query_by_label("Confirm uninstall trufflehog").is_none());
        assert!(h.query_by_label("More actions trufflehog").is_some());
    }

    /// The confirm modal over a realistic capability page, dark: gitleaks is
    /// missing, so removing TruffleHog states the hole it would leave.
    #[test]
    fn snapshot_the_confirm_modal_stating_a_hole() {
        let mut fixture = shared("secret-scanning");
        make_missing(&mut fixture, &["gitleaks"]);
        let sink = std::sync::Arc::default();
        let local = LocalState {
            confirm: Some(("trufflehog".to_string(), LifecycleAction::Uninstall)),
            ..LocalState::default()
        };
        let mut h = harness_themed(
            egui::vec2(900.0, 560.0),
            true,
            paint_with(fixture, true, local, sink),
        );
        h.snapshot("modal_confirm");
    }

    /// The same modal in the light appearance, with a working sibling: the
    /// body names who keeps covering the capability.
    #[test]
    fn snapshot_the_confirm_modal_with_a_covering_sibling() {
        let sink = std::sync::Arc::default();
        let local = LocalState {
            confirm: Some(("trufflehog".to_string(), LifecycleAction::Uninstall)),
            ..LocalState::default()
        };
        let mut h = harness_themed(
            egui::vec2(900.0, 560.0),
            false,
            paint_with(shared("secret-scanning"), false, local, sink),
        );
        h.snapshot("modal_confirm_light");
    }

    /// The bulk path end to end: the Overview offer opens the preview,
    /// deselecting narrows it, and confirming starts exactly the selected
    /// installs — in taxonomy order, manual rows never included.
    #[test]
    fn the_bulk_preview_starts_exactly_the_selected_installs() {
        let mut fixture = shared("overview");
        make_missing(&mut fixture, &["gitleaks", "ocean"]);
        let seen: std::sync::Arc<std::sync::Mutex<Vec<EngineCmd>>> = Default::default();
        let sink = std::sync::Arc::clone(&seen);
        let mut h = harness_themed(
            egui::vec2(900.0, 720.0),
            true,
            paint_with(fixture, true, LocalState::default(), sink),
        );
        h.get_by_label("Install all missing").click();
        // Settle frame for the centering Area (see the confirm walk test).
        h.run_steps(3);
        // The checklist is honest about scope: the manual candidate is
        // present (disabled), not silently absent.
        assert!(h.query_by_label("Select ocean").is_some());
        assert!(h.query_by_label("Confirm bulk install").is_some());
        h.get_by_label("Select gitleaks").click();
        h.run_steps(2);
        h.get_by_label("Confirm bulk install").click();
        h.run_steps(2);
        assert_eq!(
            seen.lock().expect("sink").clone(),
            vec![EngineCmd::StartAction {
                id: "nono".to_string(),
                action: LifecycleAction::Install,
            }],
            "deselected gitleaks and manual ocean must not start"
        );
        assert!(
            h.query_by_label("Confirm bulk install").is_none(),
            "the preview closes on confirm"
        );
    }

    /// Cancelling the preview starts nothing and closes it.
    #[test]
    fn cancelling_the_bulk_preview_starts_nothing() {
        let mut fixture = shared("overview");
        make_missing(&mut fixture, &["gitleaks"]);
        let seen: std::sync::Arc<std::sync::Mutex<Vec<EngineCmd>>> = Default::default();
        let sink = std::sync::Arc::clone(&seen);
        let mut h = harness_themed(
            egui::vec2(900.0, 720.0),
            true,
            paint_with(fixture, true, LocalState::default(), sink),
        );
        h.get_by_label("Install all missing").click();
        // Settle frame for the centering Area (see the confirm walk test).
        h.run_steps(3);
        h.get_by_label("Cancel bulk install").click();
        h.run_steps(2);
        assert!(seen.lock().expect("sink").is_empty());
        assert!(h.query_by_label("Confirm bulk install").is_none());
    }

    /// The open preview, dark: two automatable rows checked, the manual row
    /// disabled with its honest caption, the footer counting the selection.
    #[test]
    fn snapshot_the_bulk_preview() {
        let mut fixture = shared("overview");
        make_missing(&mut fixture, &["gitleaks", "ocean"]);
        let selected = crate::modals::default_selection(
            &ade_core::gui::verdict::bulk_install_candidates(&fixture.capabilities),
        );
        let sink = std::sync::Arc::default();
        let local = LocalState {
            bulk_selected: Some(selected),
            ..LocalState::default()
        };
        let mut h = harness_themed(
            egui::vec2(900.0, 720.0),
            true,
            paint_with(fixture, true, local, sink),
        );
        h.snapshot("modal_bulk");
    }

    /// The retired chrome stays retired: no tab strip, no grouping toggle,
    /// no in-window app title — the titlebar already names the app.
    #[test]
    fn the_retired_chrome_is_absent() {
        let sink = std::sync::Arc::default();
        let h = harness_themed(
            egui::vec2(900.0, 560.0),
            true,
            paint("overview", false, sink),
        );
        for label in ["Tab Capabilities", "Tab Projects", "Group by capability"] {
            assert!(
                h.query_by_label(label).is_none(),
                "retired control still present: {label}"
            );
        }
        assert!(h.query_by_label("Check for updates").is_some());
        assert!(h.query_by_label("Refresh state").is_some());
    }
}
