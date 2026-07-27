//! ADE Control Center — application state + UI (data via ade-core in-process).
//!
//! Every actionable control reports a unique AccessKit label (ISC-187) so the
//! Interceptor macOS bridge can drive the app through the AX tree.

use crate::data::{actions_for, Engine, ProjectSummary, Shared};
use crate::theme;
use ade_core::gui::inventory::{
    get_group, CapabilityKind, CapabilityStatus, LifecycleAction, LifecycleMethod,
    CAPABILITY_GROUPS,
};
use ade_core::gui::jobs::{Job, JobStatus};
use ade_core::gui::state::ade_home_from_env;
use ade_core::report::StatusRow;
use ade_core::types::{Finding, FindingLevel};
use egui::{Align, Color32, CornerRadius, Layout, RichText, Stroke, StrokeKind};
use std::collections::HashSet;
use std::sync::Arc;

#[derive(PartialEq, Clone, Copy)]
enum Tab {
    Capabilities,
    Projects,
    Activity,
}

pub struct ControlCenterApp {
    engine: Arc<Engine>,
    tab: Tab,
    project_input: String,
    confirm: Option<(String, LifecycleAction)>,
    /// Project id awaiting a second click before ade is withdrawn from it.
    confirm_remove_ade: Option<String>,
    expanded_issues: HashSet<String>,
    expanded_jobs: HashSet<String>,
}

impl ControlCenterApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        theme::apply(&cc.egui_ctx);
        let engine = Engine::new(ade_home_from_env());
        let ctx = cc.egui_ctx.clone();
        engine.start_poll(move || ctx.request_repaint());
        ControlCenterApp {
            engine,
            tab: Tab::Capabilities,
            project_input: String::new(),
            confirm: None,
            confirm_remove_ade: None,
            expanded_issues: HashSet::new(),
            expanded_jobs: HashSet::new(),
        }
    }

    fn repaint(ctx: &egui::Context) -> impl Fn() + Send + 'static {
        let ctx = ctx.clone();
        move || ctx.request_repaint()
    }
}

// ───────────────────────── level/status helpers ─────────────────────────

fn worst_level(issues: &[Finding]) -> FindingLevel {
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
enum Dot {
    Ok,
    Warn,
    Err,
    Missing,
    Disabled,
}

fn cap_dot(cap: &CapabilityStatus) -> Dot {
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

fn level_color(level: FindingLevel) -> Color32 {
    match level {
        FindingLevel::Error => theme::ERR,
        FindingLevel::Warn | FindingLevel::Degraded => theme::WARN,
        FindingLevel::Info => theme::INFO,
        FindingLevel::Ok => theme::OK,
    }
}

// ───────────────────────── small widgets ─────────────────────────

fn status_dot(ui: &mut egui::Ui, dot: Dot, muted: Color32) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(12.0, 12.0), egui::Sense::hover());
    let center = rect.center();
    match dot {
        Dot::Missing => {
            ui.painter()
                .circle_stroke(center, 4.5, Stroke::new(1.6, muted));
        }
        Dot::Disabled => {
            ui.painter()
                .circle_filled(center, 4.7, Color32::from_gray(120));
        }
        Dot::Ok => {
            ui.painter().circle_filled(center, 4.7, theme::OK);
        }
        Dot::Warn => {
            ui.painter().circle_filled(center, 4.7, theme::WARN);
        }
        Dot::Err => {
            ui.painter().circle_filled(center, 4.7, theme::ERR);
        }
    }
}

fn chip(ui: &mut egui::Ui, text: &str, color: Color32) {
    egui::Frame::new()
        .fill(color.gamma_multiply(0.16))
        .corner_radius(CornerRadius::same(10))
        .inner_margin(egui::Margin::symmetric(8, 3))
        .show(ui, |ui| {
            ui.label(RichText::new(text).color(color).size(11.0));
        });
}

fn toggle(ui: &mut egui::Ui, on: &mut bool, ax_label: &str) -> egui::Response {
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
            .lerp_to_gamma(theme::OK, how_on);
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

fn ax_button(
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
    let mut button = egui::Button::new(rich);
    if let Some(color) = fill {
        button = button.fill(color);
    }
    let response = ui.add_enabled(enabled, button);
    let label = ax_label.to_string();
    response.widget_info(move || {
        egui::WidgetInfo::labeled(egui::WidgetType::Button, true, label.clone())
    });
    response
}

fn issue_line(ui: &mut egui::Ui, issue: &Finding, muted: Color32) {
    ui.horizontal_wrapped(|ui| {
        ui.label(
            RichText::new(level_name(issue.level))
                .color(level_color(issue.level))
                .size(10.5)
                .strong(),
        );
        ui.label(RichText::new(&issue.message).size(12.0));
        if let Some(remediation) = &issue.remediation {
            ui.label(
                RichText::new(format!("— {remediation}"))
                    .color(muted)
                    .size(12.0),
            );
        }
    });
}

// ───────────────────────────── the app ─────────────────────────────

impl eframe::App for ControlCenterApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let dark = ui.visuals().dark_mode;
        let p = theme::palette(dark);

        let snapshot: Shared = {
            let guard = self.engine.shared.lock().expect("shared lock");
            Shared {
                capabilities: guard.capabilities.clone(),
                have_first_detection: guard.have_first_detection,
                group_by_capability: guard.group_by_capability,
                state_warning: guard.state_warning.clone(),
                projects: guard.projects.clone(),
                jobs: guard.jobs.clone(),
                reports: guard.reports.clone(),
                report_errors: guard.report_errors.clone(),
                toast: guard.toast.clone(),
                checking_updates: guard.checking_updates,
                pending: guard.pending.clone(),
            }
        };

        self.header(ui, &p, &snapshot);
        self.body(ui, &p, &snapshot);
    }
}

impl ControlCenterApp {
    fn header(&mut self, ui: &mut egui::Ui, p: &theme::Palette, snapshot: &Shared) {
        egui::Panel::top("header")
            .frame(
                egui::Frame::new()
                    .fill(p.panel)
                    .inner_margin(egui::Margin::symmetric(18, 12)),
            )
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("ADE Control Center").font(theme::semibold(17.0)));
                    ui.label(
                        RichText::new(format!("v{}", ade_core::version::ADE_VERSION))
                            .color(p.faint_text)
                            .size(11.5),
                    );
                    ui.add_space(10.0);
                    if snapshot.have_first_detection {
                        let enabled: Vec<&CapabilityStatus> = snapshot
                            .capabilities
                            .iter()
                            .filter(|cap| cap.enabled)
                            .collect();
                        let installed = enabled.iter().filter(|cap| cap.installed).count();
                        let running = enabled
                            .iter()
                            .filter(|cap| cap.running == Some(true))
                            .count();
                        let errors = enabled
                            .iter()
                            .filter(|cap| worst_level(&cap.issues) == FindingLevel::Error)
                            .count();
                        let warnings = enabled
                            .iter()
                            .filter(|cap| worst_level(&cap.issues) == FindingLevel::Warn)
                            .count();
                        chip(
                            ui,
                            &format!("{installed}/{} installed", enabled.len()),
                            theme::INFO,
                        );
                        chip(ui, &format!("{running} running"), theme::OK);
                        if warnings > 0 {
                            chip(ui, &format!("{warnings} warnings"), theme::WARN);
                        }
                        if errors > 0 {
                            chip(ui, &format!("{errors} errors"), theme::ERR);
                        }
                        let running_jobs = snapshot
                            .jobs
                            .iter()
                            .filter(|job| job.status == JobStatus::Running)
                            .count();
                        if running_jobs > 0 {
                            ui.add(egui::Spinner::new().size(14.0));
                            ui.label(
                                RichText::new(format!("{running_jobs} job(s)"))
                                    .color(p.muted)
                                    .size(11.5),
                            );
                        }
                    }
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        let checking = snapshot.checking_updates;
                        if ax_button(
                            ui,
                            if checking {
                                "Checking…"
                            } else {
                                "Check for Updates"
                            },
                            "Check for updates",
                            Some(theme::ACCENT),
                            !checking,
                        )
                        .clicked()
                        {
                            self.engine.check_updates(Self::repaint(ui.ctx()));
                        }
                        if ax_button(ui, "Refresh", "Refresh state", None, true).clicked() {
                            self.engine.request_refresh();
                        }
                    });
                });
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    for (tab, label) in [
                        (Tab::Capabilities, "Capabilities"),
                        (Tab::Projects, "Projects"),
                        (Tab::Activity, "Activity"),
                    ] {
                        let selected = self.tab == tab;
                        let text = if selected {
                            RichText::new(label)
                                .font(theme::medium(13.0))
                                .color(theme::ACCENT)
                        } else {
                            RichText::new(label)
                                .font(theme::medium(13.0))
                                .color(p.muted)
                        };
                        let response = ui.selectable_label(selected, text);
                        let ax = format!("Tab {label}");
                        response.clone().widget_info(move || {
                            egui::WidgetInfo::selected(
                                egui::WidgetType::SelectableLabel,
                                true,
                                selected,
                                ax.clone(),
                            )
                        });
                        if response.clicked() {
                            self.tab = tab;
                        }
                    }
                    if self.tab == Tab::Capabilities {
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            let mut grouped = snapshot.group_by_capability;
                            ui.label(
                                RichText::new("Group by capability")
                                    .color(p.muted)
                                    .size(12.0),
                            )
                            .on_hover_text(
                                "Group tools under the capability they provide — tools under one \
                                 heading are swappable alternatives for the same job.",
                            );
                            if toggle(ui, &mut grouped, "Group by capability").changed() {
                                self.engine.set_group_by_capability(grouped);
                            }
                        });
                    }
                });
            });
    }

    fn body(&mut self, ui: &mut egui::Ui, p: &theme::Palette, snapshot: &Shared) {
        egui::CentralPanel::default_margins()
            .frame(
                egui::Frame::new()
                    .fill(p.bg)
                    .inner_margin(egui::Margin::symmetric(18, 14)),
            )
            .show(ui, |ui| {
                if let Some(toast) = &snapshot.toast {
                    egui::Frame::new()
                        .fill(theme::INFO.gamma_multiply(0.12))
                        .corner_radius(CornerRadius::same(8))
                        .inner_margin(egui::Margin::symmetric(12, 8))
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.label(RichText::new(toast).size(12.5));
                                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                    if ax_button(ui, "Dismiss", "Dismiss notice", None, true)
                                        .clicked()
                                    {
                                        self.engine.shared.lock().expect("shared lock").toast =
                                            None;
                                    }
                                });
                            });
                        });
                    ui.add_space(8.0);
                }
                if let Some(warning) = &snapshot.state_warning {
                    ui.label(RichText::new(warning).color(theme::WARN).size(12.0));
                    ui.add_space(6.0);
                }

                if !snapshot.have_first_detection {
                    // Cold start (ISC-191): a loading state, never a blank window.
                    ui.add_space(120.0);
                    ui.vertical_centered(|ui| {
                        ui.add(egui::Spinner::new().size(28.0));
                        ui.add_space(10.0);
                        ui.label(RichText::new("Scanning capabilities…").font(theme::medium(14.0)));
                        ui.label(
                            RichText::new(
                                "Detecting installed tools and harnesses on this machine",
                            )
                            .color(p.muted)
                            .size(12.0),
                        );
                    });
                    return;
                }

                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| match self.tab {
                        Tab::Capabilities => self.capabilities_view(ui, p, snapshot),
                        Tab::Projects => self.projects_view(ui, p, snapshot),
                        Tab::Activity => self.activity_view(ui, p, snapshot),
                    });
            });
    }

    fn capabilities_view(&mut self, ui: &mut egui::Ui, p: &theme::Palette, snapshot: &Shared) {
        let busy: HashSet<String> = snapshot
            .jobs
            .iter()
            .filter(|job| job.status == JobStatus::Running)
            .map(|job| job.capability_id.clone())
            .collect();
        if snapshot.group_by_capability {
            self.grouped_capabilities_view(ui, p, snapshot, &busy);
            return;
        }
        for (title, kind) in [
            ("Tools", CapabilityKind::Tool),
            ("Harnesses", CapabilityKind::Harness),
        ] {
            ui.label(
                RichText::new(title.to_uppercase())
                    .color(p.faint_text)
                    .size(10.5)
                    .strong(),
            );
            ui.add_space(4.0);
            egui::Frame::new()
                .fill(p.panel)
                .stroke(Stroke::new(1.0, p.line))
                .corner_radius(CornerRadius::same(10))
                .inner_margin(egui::Margin::symmetric(14, 4))
                .show(ui, |ui| {
                    let caps: Vec<CapabilityStatus> = snapshot
                        .capabilities
                        .iter()
                        .filter(|cap| cap.kind == kind)
                        .cloned()
                        .collect();
                    for (index, cap) in caps.iter().enumerate() {
                        if index > 0 {
                            ui.separator();
                        }
                        self.capability_row(ui, p, cap, busy.contains(&cap.id));
                    }
                });
            ui.add_space(14.0);
        }
    }

    /// Grouped view (ISC-206/207/208): one section per capability; the tools
    /// beneath a heading are swappable providers of the same capability, and
    /// the heading's info glyph reveals the capability's why-explainer.
    fn grouped_capabilities_view(
        &mut self,
        ui: &mut egui::Ui,
        p: &theme::Palette,
        snapshot: &Shared,
        busy: &HashSet<String>,
    ) {
        for group in CAPABILITY_GROUPS.iter() {
            let caps: Vec<CapabilityStatus> = snapshot
                .capabilities
                .iter()
                .filter(|cap| cap.capability == group.id)
                .cloned()
                .collect();
            if caps.is_empty() {
                continue;
            }
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(group.name.to_uppercase())
                        .color(p.faint_text)
                        .size(10.5)
                        .strong(),
                );
                let info = ui
                    .label(RichText::new("(i)").color(theme::INFO).size(11.0))
                    .on_hover_text(group.why);
                let ax = format!("Why {}", group.name);
                info.widget_info(move || {
                    egui::WidgetInfo::labeled(egui::WidgetType::Label, true, ax.clone())
                });
                if caps.len() > 1 {
                    ui.label(
                        RichText::new(format!("{} interchangeable providers", caps.len()))
                            .color(p.faint_text)
                            .size(10.0),
                    );
                }
            });
            ui.add_space(4.0);
            egui::Frame::new()
                .fill(p.panel)
                .stroke(Stroke::new(1.0, p.line))
                .corner_radius(CornerRadius::same(10))
                .inner_margin(egui::Margin::symmetric(14, 4))
                .show(ui, |ui| {
                    for (index, cap) in caps.iter().enumerate() {
                        if index > 0 {
                            ui.separator();
                        }
                        self.capability_row(ui, p, cap, busy.contains(&cap.id));
                    }
                });
            ui.add_space(14.0);
        }
    }

    fn capability_row(
        &mut self,
        ui: &mut egui::Ui,
        p: &theme::Palette,
        cap: &CapabilityStatus,
        busy: bool,
    ) {
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            status_dot(ui, cap_dot(cap), p.muted);
            ui.vertical(|ui| {
                ui.set_width(230.0);
                ui.label(RichText::new(&cap.name).font(theme::medium(13.5)));
                ui.label(RichText::new(&cap.description).color(p.muted).size(11.0));
            });
            ui.vertical(|ui| {
                ui.set_width(150.0);
                match &cap.version {
                    Some(version) => {
                        ui.label(RichText::new(version).color(p.muted).size(11.5));
                    }
                    None => {
                        ui.label(
                            RichText::new(if cap.installed {
                                "installed"
                            } else {
                                "not installed"
                            })
                            .color(p.faint_text)
                            .size(11.5),
                        );
                    }
                }
                if cap.update_available {
                    if let Some(latest) = &cap.latest_version {
                        ui.label(
                            RichText::new(format!("latest {latest}"))
                                .color(theme::INFO)
                                .size(11.0),
                        );
                    }
                }
            });
            if let Some(group) = get_group(&cap.capability) {
                egui::Frame::new()
                    .fill(p.inset)
                    .corner_radius(CornerRadius::same(10))
                    .inner_margin(egui::Margin::symmetric(8, 3))
                    .show(ui, |ui| {
                        ui.label(RichText::new(group.name).color(p.muted).size(11.0));
                    })
                    .response
                    .on_hover_text(group.why);
            }
            if cap.running == Some(true) {
                chip(ui, "running", theme::OK);
            }
            if cap.update_available {
                chip(ui, "update available", theme::INFO);
            }
            if !cap.enabled {
                chip(ui, "disabled", Color32::from_gray(130));
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let mut enabled = cap.enabled;
                if toggle(ui, &mut enabled, &format!("Enable {}", cap.id)).changed() {
                    self.engine.set_capability_enabled(&cap.id, enabled);
                }
                if busy {
                    ui.add(egui::Spinner::new().size(14.0));
                    ui.label(RichText::new("working…").color(p.muted).size(11.5));
                } else if cap.method == LifecycleMethod::Manual {
                    if !cap.installed {
                        if let Some(guidance) = &cap.guidance {
                            ui.label(
                                RichText::new("manual install")
                                    .color(p.faint_text)
                                    .size(11.5),
                            )
                            .on_hover_text(guidance);
                        }
                    }
                } else if let Some((confirm_id, action)) =
                    self.confirm.clone().filter(|(id, _)| id == &cap.id)
                {
                    if ax_button(
                        ui,
                        "Cancel",
                        &format!("Cancel {} {confirm_id}", action.as_str()),
                        None,
                        true,
                    )
                    .clicked()
                    {
                        self.confirm = None;
                    }
                    if ax_button(
                        ui,
                        &format!("Confirm {}", action.as_str()),
                        &format!("Confirm {} {confirm_id}", action.as_str()),
                        Some(theme::ERR),
                        true,
                    )
                    .clicked()
                    {
                        self.confirm = None;
                        self.engine.start_action(&confirm_id, action);
                    }
                } else {
                    for action in actions_for(cap) {
                        let destructive = action == LifecycleAction::Uninstall;
                        let fill = match action {
                            LifecycleAction::Install => Some(theme::ACCENT),
                            LifecycleAction::Update if cap.update_available => Some(theme::ACCENT),
                            _ => None,
                        };
                        let visible = capitalize(action);
                        let label = format!("{visible} {}", cap.id);
                        if ax_button(ui, visible, &label, fill, true).clicked() {
                            if destructive {
                                self.confirm = Some((cap.id.clone(), action));
                            } else {
                                self.engine.start_action(&cap.id, action);
                            }
                        }
                    }
                }
            });
        });
        if !cap.issues.is_empty() {
            let key = format!("issues-{}", cap.id);
            let expanded = self.expanded_issues.contains(&key);
            ui.horizontal(|ui| {
                ui.add_space(22.0);
                let text = format!(
                    "{} {} issue{}",
                    if expanded { "−" } else { "+" },
                    cap.issues.len(),
                    if cap.issues.len() == 1 { "" } else { "s" }
                );
                let response =
                    ui.selectable_label(false, RichText::new(text).color(p.muted).size(11.0));
                let ax = format!("Issues {}", cap.id);
                response.clone().widget_info(move || {
                    egui::WidgetInfo::labeled(egui::WidgetType::Button, true, ax.clone())
                });
                if response.clicked() {
                    if expanded {
                        self.expanded_issues.remove(&key);
                    } else {
                        self.expanded_issues.insert(key.clone());
                    }
                }
            });
            if expanded {
                ui.indent(key.clone(), |ui| {
                    for issue in &cap.issues {
                        issue_line(ui, issue, p.muted);
                    }
                });
            }
        }
        ui.add_space(4.0);
    }

    fn projects_view(&mut self, ui: &mut egui::Ui, p: &theme::Palette, snapshot: &Shared) {
        ui.horizontal(|ui| {
            let edit = egui::TextEdit::singleline(&mut self.project_input)
                .hint_text("/absolute/path/to/an/ade-bootstrapped/repo")
                .desired_width(460.0);
            let response = ui.add(edit);
            response.widget_info(|| {
                egui::WidgetInfo::labeled(
                    egui::WidgetType::TextEdit,
                    true,
                    "Project path".to_string(),
                )
            });
            if ax_button(
                ui,
                "Add Project",
                "Register project",
                Some(theme::ACCENT),
                !self.project_input.trim().is_empty(),
            )
            .clicked()
            {
                let dir = self.project_input.trim().to_string();
                self.project_input.clear();
                self.engine.register_project(dir, Self::repaint(ui.ctx()));
            }
        });
        ui.add_space(10.0);
        if snapshot.projects.is_empty() {
            ui.label(
                RichText::new(
                    "No projects registered — add an ade-bootstrapped repo to manage its modules here.",
                )
                .color(p.muted),
            );
            return;
        }
        let projects = snapshot.projects.clone();
        for project in &projects {
            self.project_card(ui, p, snapshot, project);
            ui.add_space(10.0);
        }
    }

    fn project_card(
        &mut self,
        ui: &mut egui::Ui,
        p: &theme::Palette,
        snapshot: &Shared,
        project: &ProjectSummary,
    ) {
        egui::Frame::new()
            .fill(p.panel)
            .stroke(Stroke::new(1.0, p.line))
            .corner_radius(CornerRadius::same(10))
            .inner_margin(egui::Margin::symmetric(14, 10))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    status_dot(
                        ui,
                        if project.config_ok { Dot::Ok } else { Dot::Err },
                        p.muted,
                    );
                    // The path is added INSIDE the right-to-left layout, after
                    // the buttons, so it truncates into whatever space is left.
                    // Added before them it claimed the full width and the
                    // controls drew straight over the text — real repo paths are
                    // long, so this was not a fixture-only problem.
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        // Two controls that sound alike and are not: "Forget"
                        // only stops tracking the repo here, "Remove ADE"
                        // rewrites the repo. Naming the destructive one after
                        // what it destroys is the whole safety margin.
                        if let Some(pending_id) = self
                            .confirm_remove_ade
                            .clone()
                            .filter(|id| id == &project.id)
                        {
                            if ax_button(
                                ui,
                                "Cancel",
                                &format!("Cancel removing ade from {}", project.dir),
                                None,
                                true,
                            )
                            .clicked()
                            {
                                self.confirm_remove_ade = None;
                            }
                            if ax_button(
                                ui,
                                "Confirm remove ADE",
                                &format!("Withdraw ade from {}", project.dir),
                                Some(theme::ERR),
                                true,
                            )
                            .clicked()
                            {
                                self.confirm_remove_ade = None;
                                self.engine
                                    .remove_ade_from_project(pending_id, Self::repaint(ui.ctx()));
                            }
                        } else {
                            if ax_button(
                                ui,
                                "Forget",
                                &format!(
                                    "Stop tracking {} here — the repo is not changed",
                                    project.dir
                                ),
                                None,
                                true,
                            )
                            .clicked()
                            {
                                self.engine.unregister_project(
                                    project.id.clone(),
                                    Self::repaint(ui.ctx()),
                                );
                            }
                            if ax_button(
                                ui,
                                "Remove ADE…",
                                &format!(
                                    "Withdraw ade's files and managed blocks from {}",
                                    project.dir
                                ),
                                None,
                                project.config_ok,
                            )
                            .clicked()
                            {
                                self.confirm_remove_ade = Some(project.id.clone());
                            }
                        }
                        let has_report = snapshot.reports.contains_key(&project.id);
                        let inspecting =
                            snapshot.pending.contains(&format!("report-{}", project.id));
                        if ax_button(
                            ui,
                            if inspecting {
                                "Loading…"
                            } else if has_report {
                                "Reload"
                            } else {
                                "Inspect"
                            },
                            &format!("Inspect project {}", project.dir),
                            None,
                            !inspecting,
                        )
                        .clicked()
                        {
                            self.engine
                                .load_report(project.id.clone(), Self::repaint(ui.ctx()));
                        }
                        ui.add(
                            egui::Label::new(RichText::new(&project.dir).font(theme::medium(13.0)))
                                .truncate(),
                        )
                        .on_hover_text(&project.dir);
                    });
                });
                if let Some(error) = snapshot.report_errors.get(&project.id) {
                    ui.label(RichText::new(error).color(theme::ERR).size(12.0));
                }
                if let Some(report) = snapshot.reports.get(&project.id) {
                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        if report.verify_ok {
                            chip(ui, "verify PASS", theme::OK);
                        } else {
                            chip(ui, "verify FAIL", theme::ERR);
                        }
                        if let Some(error) = &report.error {
                            ui.label(RichText::new(error).color(theme::ERR).size(12.0));
                        }
                    });
                    ui.add_space(4.0);
                    let modules = report.modules.clone();
                    for module in &modules {
                        self.module_row(ui, p, project, module);
                    }
                }
            });
    }

    fn module_row(
        &mut self,
        ui: &mut egui::Ui,
        p: &theme::Palette,
        project: &ProjectSummary,
        module: &StatusRow,
    ) {
        ui.horizontal(|ui| {
            let mut enabled = module.enabled;
            if toggle(
                ui,
                &mut enabled,
                &format!("Enable module {} in {}", module.id, project.dir),
            )
            .changed()
            {
                self.engine.set_module_enabled(
                    project.id.clone(),
                    module.id.clone(),
                    enabled,
                    Self::repaint(ui.ctx()),
                );
            }
            ui.label(RichText::new(&module.id).font(theme::medium(12.5)));
            ui.label(RichText::new(&module.state).color(p.muted).size(11.0));
            ui.label(RichText::new(&module.title).color(p.faint_text).size(11.0));
        });
        let interesting: Vec<&Finding> = module
            .findings
            .iter()
            .filter(|finding| finding.level != FindingLevel::Ok)
            .collect();
        if !interesting.is_empty() {
            ui.indent(format!("mf-{}-{}", project.id, module.id), |ui| {
                for finding in interesting {
                    issue_line(ui, finding, p.muted);
                }
            });
        }
    }

    fn activity_view(&mut self, ui: &mut egui::Ui, p: &theme::Palette, snapshot: &Shared) {
        if snapshot.jobs.is_empty() {
            ui.label(
                RichText::new("No jobs yet — install/update/uninstall actions appear here.")
                    .color(p.muted),
            );
            return;
        }
        let jobs = snapshot.jobs.clone();
        for job in &jobs {
            self.job_card(ui, p, job);
            ui.add_space(8.0);
        }
    }

    fn job_card(&mut self, ui: &mut egui::Ui, p: &theme::Palette, job: &Job) {
        egui::Frame::new()
            .fill(p.panel)
            .stroke(Stroke::new(1.0, p.line))
            .corner_radius(CornerRadius::same(10))
            .inner_margin(egui::Margin::symmetric(14, 10))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    let dot = match job.status {
                        JobStatus::Ok => Dot::Ok,
                        JobStatus::Error => Dot::Err,
                        JobStatus::Running => Dot::Warn,
                    };
                    status_dot(ui, dot, p.muted);
                    ui.label(RichText::new(&job.capability_id).font(theme::medium(13.0)));
                    ui.label(RichText::new(&job.action).color(p.muted).size(12.0));
                    match job.status {
                        JobStatus::Running => {
                            ui.add(egui::Spinner::new().size(13.0));
                            ui.label(RichText::new("running").color(theme::WARN).size(11.5));
                        }
                        JobStatus::Ok => chip(ui, "ok", theme::OK),
                        JobStatus::Error => chip(
                            ui,
                            &format!("error (exit {})", job.exit_code.unwrap_or(-1)),
                            theme::ERR,
                        ),
                    }
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.label(
                            RichText::new(&job.started_at)
                                .color(p.faint_text)
                                .size(10.5),
                        );
                    });
                });
                let key = format!("job-{}", job.id);
                let expanded = self.expanded_jobs.contains(&key);
                let toggle_text = format!(
                    "{} log ({} lines)",
                    if expanded { "−" } else { "+" },
                    job.log.len()
                );
                let response = ui
                    .selectable_label(false, RichText::new(toggle_text).color(p.muted).size(11.0));
                let ax = format!("Log {}", job.id);
                response.clone().widget_info(move || {
                    egui::WidgetInfo::labeled(egui::WidgetType::Button, true, ax.clone())
                });
                if response.clicked() {
                    if expanded {
                        self.expanded_jobs.remove(&key);
                    } else {
                        self.expanded_jobs.insert(key.clone());
                    }
                }
                if expanded {
                    egui::Frame::new()
                        .fill(p.inset)
                        .corner_radius(CornerRadius::same(8))
                        .inner_margin(egui::Margin::same(10))
                        .show(ui, |ui| {
                            egui::ScrollArea::vertical()
                                .id_salt(&key)
                                .max_height(220.0)
                                .show(ui, |ui| {
                                    ui.label(
                                        RichText::new(job.log.join("\n")).monospace().size(11.0),
                                    );
                                });
                        });
                }
            });
    }
}

fn capitalize(action: LifecycleAction) -> &'static str {
    match action {
        LifecycleAction::Install => "Install",
        LifecycleAction::Uninstall => "Uninstall",
        LifecycleAction::Reinstall => "Reinstall",
        LifecycleAction::Update => "Update",
    }
}
