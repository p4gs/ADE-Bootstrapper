//! The Projects scope — Phase H restructure: a projects × capability-groups
//! **coverage matrix** is now the page's headline content (the plan's own
//! words: "a real restructure, not an addition"). Each cell reflects the
//! exact same `StatusRow.state` the CLI's `ade status` prints, via the same
//! `ade_core::gui::stats::{module_for_group, module_state_covers}` helpers
//! Phase G's stats strip already calls — so the matrix and the CLI can never
//! silently disagree about what "wired" means; they read the identical
//! function on the identical data. Below the matrix, the per-project
//! management cards (register/forget/withdraw ADE/inspect/toggle a module)
//! remain — real, tested functionality the matrix does not replace, only
//! summarizes.
//!
//! Pure view but for one sanctioned escape hatch: the path input needs
//! `&mut String`.

use crate::app::{ax_button, chip, issue_line, status_dot, status_dot_at, toggle, Dot};
use crate::data::{ProjectReport, ProjectSummary, Shared};
use crate::theme;
use ade_core::gui::inventory::{CapabilityGroup, CAPABILITY_GROUPS};
use ade_core::gui::stats::{module_for_group, module_state_covers};
use ade_core::report::StatusRow;
use ade_core::types::{Finding, FindingLevel};
use egui::{Align, CornerRadius, Layout, RichText, Stroke};

/// What the owner did in the Projects list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ProjectsEvent {
    Register(String),
    Forget(String),
    /// Arm the second-click confirmation for withdrawing ADE.
    AskRemoveAde(String),
    CancelRemoveAde,
    ConfirmRemoveAde(String),
    Inspect(String),
    SetModuleEnabled {
        project_id: String,
        module_id: String,
        enabled: bool,
    },
    /// A coverage-matrix cell was clicked — open that capability group's page.
    OpenGroup(String),
}

/// One coverage-matrix cell — reflects the exact same fact the CLI's
/// `ade status` prints for the same project, or an honest absence when there
/// is nothing to report.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MatrixCell {
    /// The group's module is applied or degraded for this project —
    /// `module_state_covers` says so, the exact test the stats strip's own
    /// "project coverage" tile already uses.
    Wired,
    /// The group HAS a governing module, but this project's report says it
    /// is not covering (not-applied, or disabled).
    NotWired,
    /// No `ade.json` module governs this capability group at all — the
    /// honest absence `stats::module_for_group` already documents for the
    /// other five groups (agent-security-rules, hook-orchestration,
    /// codebase-wiki, semantic-search, coding-harness).
    NotApplicable,
    /// The group HAS a governing module, but this project has not been
    /// inspected yet (no report loaded) — genuinely unknown, not a gap.
    Unknown,
}

/// Pure: the same `module_for_group`/`module_state_covers` functions Phase
/// G's `refresh_group_stats` already calls for the "project coverage"
/// stats-strip tile, so this can never classify a cell differently than that
/// tile counts the same project. `report` is `None` when the project has
/// never been inspected in this session.
pub(crate) fn matrix_cell(report: Option<&ProjectReport>, group_id: &str) -> MatrixCell {
    let Some(module_id) = module_for_group(group_id) else {
        return MatrixCell::NotApplicable;
    };
    let Some(report) = report else {
        return MatrixCell::Unknown;
    };
    match report.modules.iter().find(|row| row.id == module_id) {
        Some(row) if module_state_covers(&row.state) => MatrixCell::Wired,
        Some(_) => MatrixCell::NotWired,
        // status_report emits all 15 modules unconditionally, so this should
        // never happen in practice — never assumed, though.
        None => MatrixCell::Unknown,
    }
}

fn cell_hover_text(cell: MatrixCell) -> &'static str {
    match cell {
        MatrixCell::Wired => "wired",
        MatrixCell::NotWired => "not wired",
        MatrixCell::NotApplicable => "not applicable — no module governs this capability",
        MatrixCell::Unknown => "not yet known — inspect this project to check",
    }
}

/// The last path component, for a compact row label — the full path is a
/// hover away.
fn project_short_name(dir: &str) -> &str {
    dir.rsplit('/').find(|part| !part.is_empty()).unwrap_or(dir)
}

pub(crate) fn projects_view(
    ui: &mut egui::Ui,
    p: &theme::Palette,
    snapshot: &Shared,
    project_input: &mut String,
    confirm_remove_ade: &Option<String>,
) -> Option<ProjectsEvent> {
    let mut event = None;
    ui.horizontal(|ui| {
        let edit = egui::TextEdit::singleline(project_input)
            .hint_text("/absolute/path/to/an/ade-bootstrapped/repo")
            .desired_width(460.0);
        let response = ui.add(edit);
        response.widget_info(|| {
            egui::WidgetInfo::labeled(egui::WidgetType::TextEdit, true, "Project path".to_string())
        });
        if ax_button(
            ui,
            "Add project",
            "Register project",
            Some(p.accent),
            !project_input.trim().is_empty(),
        )
        .clicked()
        {
            let dir = project_input.trim().to_string();
            project_input.clear();
            event = Some(ProjectsEvent::Register(dir));
        }
    });
    ui.add_space(theme::Space::S8.px());
    if snapshot.projects.is_empty() {
        ui.label(
            RichText::new(
                "No projects registered — add an ade-bootstrapped repo to manage its modules here.",
            )
            .color(p.muted),
        );
        return event;
    }

    theme::section_heading(ui, p, "Coverage Matrix");
    ui.add_space(theme::Space::S4.px());
    if let Some(found) = coverage_matrix(ui, p, snapshot) {
        event = Some(found);
    }

    ui.add_space(theme::Space::S20.px());
    theme::section_heading(ui, p, "Registered Projects");
    ui.add_space(theme::Space::S4.px());
    for project in &snapshot.projects {
        if let Some(found) = project_card(ui, p, snapshot, project, confirm_remove_ade) {
            event = Some(found);
        }
        ui.add_space(theme::Space::S8.px());
    }
    event
}

// ───────────────────────── Phase H — the coverage matrix ─────────────────────────

/// Rows = registered projects (registration order), columns = the ten
/// capability groups (taxonomy order) — horizontally scrollable, since ten
/// full group names do not fit a 1020pt content column. Healthy cells are
/// silent (a small muted mark, never a green wall); gaps carry the attention
/// tint; not-applicable/unknown cells are a plain muted dash. Every cell is
/// clickable — cell-click wiring to the relevant capability scope, per the
/// redesign plan — regardless of its own state, since "go look at this
/// group's page" is always a valid, honest action.
/// Wide enough for a realistic repo name ("nthpartyfinder", "grce-design-system")
/// without truncating to a handful of letters — the full path is always a
/// hover away regardless.
const PROJECT_COLUMN_WIDTH: f32 = 160.0;

fn coverage_matrix(
    ui: &mut egui::Ui,
    p: &theme::Palette,
    snapshot: &Shared,
) -> Option<ProjectsEvent> {
    let mut event = None;
    egui::ScrollArea::horizontal()
        .id_salt("coverage-matrix-scroll")
        .show(ui, |ui| {
            egui::Grid::new("coverage-matrix")
                .num_columns(CAPABILITY_GROUPS.len() + 1)
                .spacing(egui::vec2(theme::Space::S16.px(), theme::Space::S6.px()))
                .show(ui, |ui| {
                    ui.label(
                        RichText::new("PROJECT")
                            .color(p.faint_text)
                            .font(theme::medium(9.5)),
                    );
                    for group in CAPABILITY_GROUPS.iter() {
                        ui.label(
                            RichText::new(group.name.to_uppercase())
                                .color(p.faint_text)
                                .font(theme::medium(9.5)),
                        );
                    }
                    ui.end_row();
                    for project in &snapshot.projects {
                        // A fixed minimum width: without it, Grid sizes the
                        // PROJECT column to the narrow "PROJECT" header and
                        // every real repo name gets truncated to a few
                        // letters — found in the mandated visual pass.
                        ui.allocate_ui(
                            egui::vec2(PROJECT_COLUMN_WIDTH, ui.spacing().interact_size.y),
                            |ui| {
                                ui.set_width(PROJECT_COLUMN_WIDTH);
                                ui.add(
                                    egui::Label::new(
                                        RichText::new(project_short_name(&project.dir))
                                            .font(theme::medium(theme::SIZE_BODY)),
                                    )
                                    .truncate(),
                                )
                                .on_hover_text(&project.dir);
                            },
                        );
                        let report = snapshot.reports.get(&project.id);
                        for group in CAPABILITY_GROUPS.iter() {
                            let cell = matrix_cell(report, group.id);
                            if matrix_cell_button(ui, p, project, group, cell) {
                                event = Some(ProjectsEvent::OpenGroup(group.id.to_string()));
                            }
                        }
                        ui.end_row();
                    }
                });
        });
    event
}

/// One matrix cell: a fixed-size hit target, a hover wash matching every
/// other pointable row in the app, and a painted mark whose shape/color
/// (never absent) states the cell's real classification.
fn matrix_cell_button(
    ui: &mut egui::Ui,
    p: &theme::Palette,
    project: &ProjectSummary,
    group: &CapabilityGroup,
    cell: MatrixCell,
) -> bool {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(22.0, 22.0), egui::Sense::click());
    let ax = format!("Open {} for {}", group.name, project.dir);
    response
        .widget_info(move || egui::WidgetInfo::labeled(egui::WidgetType::Button, true, ax.clone()));
    if ui.is_rect_visible(rect) {
        let wash = ui.ctx().animate_bool_with_time(
            egui::Id::new(("matrix-cell-hover", &project.id, group.id)),
            response.hovered(),
            theme::DURATION_HOVER,
        );
        if wash > 0.0 {
            ui.painter().rect_filled(
                rect,
                egui::CornerRadius::same(theme::RADIUS_CONTROL),
                p.row_hover.gamma_multiply(wash),
            );
        }
        let center = rect.center();
        match cell {
            // Healthy is silent: a small muted mark, never the green `Dot::Ok`
            // pigment — "no green wall" is the design's own phrase for this.
            MatrixCell::Wired => {
                ui.painter().circle_filled(center, 3.0, p.muted);
            }
            // A real gap — the attention tint, same painted mark every other
            // warning uses.
            MatrixCell::NotWired => status_dot_at(ui, rect, Dot::Warn, p),
            // Honest absence, either kind: a plain muted dash, never a guess.
            MatrixCell::NotApplicable | MatrixCell::Unknown => {
                ui.painter().line_segment(
                    [center - egui::vec2(3.5, 0.0), center + egui::vec2(3.5, 0.0)],
                    Stroke::new(1.6, p.faint_text),
                );
            }
        }
    }
    response.on_hover_text(cell_hover_text(cell)).clicked()
}

fn project_card(
    ui: &mut egui::Ui,
    p: &theme::Palette,
    snapshot: &Shared,
    project: &ProjectSummary,
    confirm_remove_ade: &Option<String>,
) -> Option<ProjectsEvent> {
    let mut event = None;
    egui::Frame::new()
        .fill(p.panel)
        .stroke(Stroke::new(1.0, p.line))
        .corner_radius(CornerRadius::same(theme::RADIUS_CONTAINER))
        .inner_margin(theme::margin(theme::Space::S12, theme::Space::S8))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                status_dot(ui, if project.config_ok { Dot::Ok } else { Dot::Err }, p);
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
                    if let Some(pending_id) =
                        confirm_remove_ade.clone().filter(|id| id == &project.id)
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
                            event = Some(ProjectsEvent::CancelRemoveAde);
                        }
                        if ax_button(
                            ui,
                            "Confirm remove ADE",
                            &format!("Withdraw ade from {}", project.dir),
                            Some(p.err),
                            true,
                        )
                        .clicked()
                        {
                            event = Some(ProjectsEvent::ConfirmRemoveAde(pending_id));
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
                            event = Some(ProjectsEvent::Forget(project.id.clone()));
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
                            event = Some(ProjectsEvent::AskRemoveAde(project.id.clone()));
                        }
                    }
                    let has_report = snapshot.reports.contains_key(&project.id);
                    let inspecting = snapshot.pending.contains(&format!("report-{}", project.id));
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
                        event = Some(ProjectsEvent::Inspect(project.id.clone()));
                    }
                    ui.add(
                        egui::Label::new(
                            RichText::new(&project.dir).font(theme::medium(theme::SIZE_BODY)),
                        )
                        .truncate(),
                    )
                    .on_hover_text(&project.dir);
                });
            });
            if let Some(error) = snapshot.report_errors.get(&project.id) {
                ui.label(RichText::new(error).color(p.err).size(theme::SIZE_CAPTION));
            }
            if let Some(report) = snapshot.reports.get(&project.id) {
                ui.add_space(theme::Space::S6.px());
                ui.horizontal(|ui| {
                    if report.verify_ok {
                        chip(ui, "verify PASS", p.ok);
                    } else {
                        chip(ui, "verify FAIL", p.err);
                    }
                });
                ui.add_space(theme::Space::S4.px());
                for module in &report.modules {
                    if let Some(found) = module_row(ui, p, project, module) {
                        event = Some(found);
                    }
                }
            }
        });
    event
}

fn module_row(
    ui: &mut egui::Ui,
    p: &theme::Palette,
    project: &ProjectSummary,
    module: &StatusRow,
) -> Option<ProjectsEvent> {
    let mut event = None;
    ui.horizontal(|ui| {
        let mut enabled = module.enabled;
        if toggle(
            ui,
            &mut enabled,
            p,
            &format!("Enable module {} in {}", module.id, project.dir),
        )
        .changed()
        {
            event = Some(ProjectsEvent::SetModuleEnabled {
                project_id: project.id.clone(),
                module_id: module.id.clone(),
                enabled,
            });
        }
        ui.label(RichText::new(&module.id).font(theme::medium(theme::SIZE_BODY)));
        ui.label(
            RichText::new(&module.state)
                .color(p.muted)
                .size(theme::SIZE_CAPTION),
        );
        ui.label(
            RichText::new(&module.title)
                .color(p.faint_text)
                .size(theme::SIZE_CAPTION),
        );
    });
    let interesting: Vec<&Finding> = module
        .findings
        .iter()
        .filter(|finding| finding.level != FindingLevel::Ok)
        .collect();
    if !interesting.is_empty() {
        ui.indent(format!("mf-{}-{}", project.id, module.id), |ui| {
            for finding in interesting {
                issue_line(ui, finding, p);
            }
        });
    }
    event
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::ProjectReport;
    use crate::testkit::harness_themed;
    use egui_kittest::kittest::Queryable;

    fn shared_with_project() -> Shared {
        let project = ProjectSummary {
            id: "abc123def456".to_string(),
            dir: "/Users/owner/code/example-repo".to_string(),
            config_ok: true,
        };
        let report = ProjectReport {
            verify_ok: true,
            modules: vec![
                StatusRow {
                    id: "secrets".to_string(),
                    title: "Secret scanning at the commit boundary".to_string(),
                    enabled: true,
                    state: "applied".to_string(),
                    findings: Vec::new(),
                },
                StatusRow {
                    id: "sandbox".to_string(),
                    title: "Kernel-enforced sandboxing".to_string(),
                    enabled: false,
                    state: "disabled".to_string(),
                    findings: vec![Finding::warn("sandbox module is disabled", "enable it")],
                },
            ],
        };
        let mut shared = Shared {
            projects: vec![project],
            ..Shared::default()
        };
        shared.reports.insert("abc123def456".to_string(), report);
        shared
    }

    fn paint(
        sink: std::sync::Arc<std::sync::Mutex<Vec<ProjectsEvent>>>,
    ) -> impl FnMut(&mut egui::Ui) + 'static {
        let shared = shared_with_project();
        let mut input = String::new();
        move |ui| {
            let p = crate::theme::palette(true);
            let confirm = None;
            if let Some(event) = projects_view(ui, &p, &shared, &mut input, &confirm) {
                sink.lock().expect("sink").push(event);
            }
        }
    }

    /// The add-project row, the coverage matrix, a project card with its
    /// report, module toggles, and a warning finding — through the same
    /// function the app calls. Wider/taller than before: the matrix is real
    /// new content, not a cosmetic tweak to the old per-project cards.
    #[test]
    fn snapshot_the_projects_page_with_a_report() {
        let sink = std::sync::Arc::default();
        let mut h = harness_themed(egui::vec2(900.0, 760.0), true, paint(sink));
        h.snapshot("projects");
    }

    /// Destructive withdrawal takes two clicks: the first only arms the
    /// confirmation; nothing runs until the confirm is clicked.
    #[test]
    fn remove_ade_is_armed_then_confirmed_never_direct() {
        let seen: std::sync::Arc<std::sync::Mutex<Vec<ProjectsEvent>>> = Default::default();
        let sink = std::sync::Arc::clone(&seen);
        let shared = shared_with_project();
        let armed: std::sync::Arc<std::sync::Mutex<Option<String>>> = Default::default();
        let confirm_state = std::sync::Arc::clone(&armed);
        let mut input = String::new();
        let mut h = harness_themed(egui::vec2(700.0, 380.0), true, move |ui| {
            let p = crate::theme::palette(true);
            let confirm = confirm_state.lock().expect("confirm").clone();
            if let Some(event) = projects_view(ui, &p, &shared, &mut input, &confirm) {
                // Mirror the shell's bookkeeping so the second frame renders
                // the armed state.
                match &event {
                    ProjectsEvent::AskRemoveAde(id) => {
                        *confirm_state.lock().expect("confirm") = Some(id.clone());
                    }
                    ProjectsEvent::CancelRemoveAde | ProjectsEvent::ConfirmRemoveAde(_) => {
                        *confirm_state.lock().expect("confirm") = None;
                    }
                    _ => {}
                }
                sink.lock().expect("sink").push(event);
            }
        });
        h.get_by_label(
            "Withdraw ade's files and managed blocks from /Users/owner/code/example-repo",
        )
        .click();
        h.run_steps(2);
        h.get_by_label("Withdraw ade from /Users/owner/code/example-repo")
            .click();
        h.run_steps(2);
        assert_eq!(
            seen.lock().expect("sink").clone(),
            vec![
                ProjectsEvent::AskRemoveAde("abc123def456".to_string()),
                ProjectsEvent::ConfirmRemoveAde("abc123def456".to_string()),
            ]
        );
        // The empty-input Add button stays disabled: no event from clicking it.
        let quiet: std::sync::Arc<std::sync::Mutex<Vec<ProjectsEvent>>> = Default::default();
        let sink = std::sync::Arc::clone(&quiet);
        let mut idle = harness_themed(egui::vec2(700.0, 380.0), true, paint(sink));
        assert!(idle.query_by_label("Register project").is_some());
        idle.get_by_label("Register project").click();
        idle.run_steps(2);
        assert!(quiet.lock().expect("sink").is_empty());
    }

    // ───────────────────────── Phase H — the coverage matrix ─────────────────────────

    fn report_with(rows: &[(&str, &str)]) -> ProjectReport {
        ProjectReport {
            verify_ok: true,
            modules: rows
                .iter()
                .map(|(id, state)| StatusRow {
                    id: id.to_string(),
                    title: format!("{id} title"),
                    enabled: state != &"disabled",
                    state: state.to_string(),
                    findings: Vec::new(),
                })
                .collect(),
        }
    }

    /// The parity proof: `matrix_cell` classifies a `StatusRow` exactly the
    /// way `ade status` (the CLI) would print it for the same row — both
    /// read `row.state` through `module_state_covers`, the identical
    /// function `stats::project_coverage`'s own Tier-1 tile already uses.
    /// `ade status`'s own human line is `"  {state:<12} {id} — {title}"`
    /// (`crates/ade/src/main.rs`) — printing "applied"/"degraded" verbatim
    /// is exactly what `Wired` asserts here, and "not-applied"/"disabled"
    /// verbatim is exactly what `NotWired` asserts.
    #[test]
    fn matrix_cell_classifies_exactly_like_ade_status_prints_the_same_row() {
        for state in ["applied", "degraded"] {
            let report = report_with(&[("secrets", state)]);
            assert_eq!(
                matrix_cell(Some(&report), "secret-scanning"),
                MatrixCell::Wired,
                "ade status would print '{state}' for secrets — a covering state"
            );
        }
        for state in ["not-applied", "disabled"] {
            let report = report_with(&[("secrets", state)]);
            assert_eq!(
                matrix_cell(Some(&report), "secret-scanning"),
                MatrixCell::NotWired,
                "ade status would print '{state}' for secrets — a non-covering state"
            );
        }
    }

    #[test]
    fn matrix_cell_is_not_applicable_for_groups_with_no_governing_module() {
        let report = report_with(&[("secrets", "applied")]);
        // codebase-wiki, semantic-search, coding-harness, hook-orchestration,
        // and agent-security-rules have no `module_for_group` mapping at all
        // — the exact five `stats::module_for_group`'s own doc table names.
        for group_id in [
            "codebase-wiki",
            "semantic-search",
            "coding-harness",
            "hook-orchestration",
            "agent-security-rules",
        ] {
            assert_eq!(
                matrix_cell(Some(&report), group_id),
                MatrixCell::NotApplicable,
                "{group_id} has no governing module"
            );
        }
    }

    #[test]
    fn matrix_cell_is_unknown_before_a_project_is_inspected() {
        // A group WITH a real module mapping, but no report at all — the
        // project has never been inspected this session.
        assert_eq!(matrix_cell(None, "secret-scanning"), MatrixCell::Unknown);
    }

    #[test]
    fn project_short_name_takes_the_last_path_component() {
        assert_eq!(
            project_short_name("/Users/owner/code/example-repo"),
            "example-repo"
        );
        assert_eq!(
            project_short_name("/Users/owner/code/example-repo/"),
            "example-repo"
        );
        assert_eq!(project_short_name("no-slashes"), "no-slashes");
    }

    /// Clicking any cell in a column asks to open that column's group page —
    /// independent of the cell's own wired/not-wired/unknown state, and
    /// independent of which project's row it was clicked on.
    #[test]
    fn clicking_a_matrix_cell_opens_its_column_group() {
        let seen: std::sync::Arc<std::sync::Mutex<Vec<ProjectsEvent>>> = Default::default();
        let sink = std::sync::Arc::clone(&seen);
        let shared = shared_with_project();
        let mut input = String::new();
        let mut h = harness_themed(egui::vec2(900.0, 760.0), true, move |ui| {
            let p = crate::theme::palette(true);
            if let Some(event) = projects_view(ui, &p, &shared, &mut input, &None) {
                sink.lock().expect("sink").push(event);
            }
        });
        h.get_by_label("Open Secret Scanning for /Users/owner/code/example-repo")
            .click();
        h.run_steps(2);
        assert_eq!(
            seen.lock().expect("sink").clone(),
            vec![ProjectsEvent::OpenGroup("secret-scanning".to_string())]
        );
    }

    /// A registered project that has never been inspected: every cell is
    /// honestly unknown (a dash), never a guessed wired/not-wired.
    #[test]
    fn snapshot_the_matrix_before_any_project_is_inspected() {
        let project = ProjectSummary {
            id: "not-yet".to_string(),
            dir: "/Users/owner/code/uninspected-repo".to_string(),
            config_ok: true,
        };
        let shared = Shared {
            projects: vec![project],
            ..Shared::default()
        };
        let mut input = String::new();
        let mut h = harness_themed(egui::vec2(900.0, 500.0), true, move |ui| {
            let p = crate::theme::palette(true);
            projects_view(ui, &p, &shared, &mut input, &None);
        });
        h.snapshot("projects_matrix_unknown");
    }

    /// Two projects, mixed coverage: one wired capability, one gap, and
    /// (for the uninspected second project) a full row of unknowns — every
    /// real matrix state visible in one image.
    #[test]
    fn snapshot_the_matrix_with_mixed_coverage() {
        let inspected = ProjectSummary {
            id: "abc123def456".to_string(),
            dir: "/Users/owner/code/example-repo".to_string(),
            config_ok: true,
        };
        let uninspected = ProjectSummary {
            id: "not-yet".to_string(),
            dir: "/Users/owner/code/other-repo".to_string(),
            config_ok: true,
        };
        let mut shared = Shared {
            projects: vec![inspected, uninspected],
            ..Shared::default()
        };
        shared.reports.insert(
            "abc123def456".to_string(),
            report_with(&[("secrets", "applied"), ("sandbox", "not-applied")]),
        );
        let mut input = String::new();
        let mut h = harness_themed(egui::vec2(900.0, 700.0), true, move |ui| {
            let p = crate::theme::palette(true);
            projects_view(ui, &p, &shared, &mut input, &None);
        });
        h.snapshot("projects_matrix_mixed");
    }
}
