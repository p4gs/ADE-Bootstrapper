//! The collision sweep (ISC-27/28/29): every scope rendered through the real
//! `frame` at the window's MINIMUM size, light and dark, over a fixture built
//! to hurt — long versions, long paths, long descriptions, nine jobs with
//! long labels, a toast AND a state warning at once, findings with long
//! remediation, a non-empty search. Two kinds of evidence come out:
//!
//! - snapshots (`stress_*`) for the eye, and
//! - a structural invariant over the AccessKit tree: no labelled node may
//!   run past the window's right edge, and no two text nodes may overlap.
//!
//! Painter-drawn text (rows, tiles) is not in the AX tree, so the snapshots
//! remain the evidence for those; the invariant catches the widget layer.

#![cfg(test)]

use crate::app::{frame, LocalState};
use crate::data::{ProjectSummary, Shared};
use crate::testkit::{cap, harness_themed};
use ade_core::gui::insights::{Insight, InsightAction};
use ade_core::gui::inventory::{CapabilityStatus, CAPABILITIES, CAPABILITY_GROUPS};
use ade_core::gui::jobs::{Job, JobStatus};
use ade_core::gui::verdict::AttentionRank;
use ade_core::types::{Finding, FindingLevel};
use egui::accesskit::Role;
use egui_kittest::kittest::NodeT;
use egui_kittest::Harness;

/// The window's minimum inner size (`main.rs` `with_min_inner_size`).
const MIN_WINDOW: egui::Vec2 = egui::vec2(860.0, 560.0);
const DEFAULT_WINDOW: egui::Vec2 = egui::vec2(1100.0, 740.0);

const LONG_PATH: &str =
    "/var/folders/7f/jzgryp5n1_j2hjp_x2d1z36c0000gn/T/claude-code-2.1.278-arm64/bin/claude";
const LONG_DESCRIPTION: &str =
    "Anthropic's coding harness CLI, installed globally through npm with \
    the bundled ripgrep, the managed settings overlay and every plugin this machine has ever \
    enabled, which makes this description far longer than any row was designed for";

fn job(n: usize, action: &str, capability_id: &str, status: JobStatus) -> Job {
    // Few enough lines to fit the drawer unscrolled: content scrolled out
    // of a ScrollArea keeps its AccessKit bounds (egui exposes no clip), and
    // would read as overlapping whatever it scrolled behind.
    let mut log: Vec<String> = (0..6)
        .map(|i| format!("npm notice line {i} of the {action} log for {capability_id}"))
        .collect();
    log.push(format!(
        "npm error A complete log of this run can be found in: /Users/owner/.npm/_logs/2026-08-01T16_56_00_031Z-debug-{n}.log and it keeps going past any sane width"
    ));
    Job {
        id: format!("job-{n:02}"),
        capability_id: capability_id.to_string(),
        action: action.to_string(),
        status,
        started_at: format!("2026-09-22T10:{n:02}:00Z"),
        finished_at: (status != JobStatus::Running).then(|| format!("2026-09-22T10:{n:02}:30Z")),
        exit_code: match status {
            JobStatus::Running => None,
            JobStatus::Ok => Some(0),
            JobStatus::Error => Some(243),
        },
        log,
    }
}

/// The fixture: every capability, with the worst strings the real world has
/// produced (and a little worse), on the scope under test.
pub(crate) fn stress_shared(scope: &str) -> Shared {
    let mut caps: Vec<CapabilityStatus> = CAPABILITIES.iter().map(|def| cap(def.id)).collect();
    for c in caps.iter_mut() {
        match c.id.as_str() {
            "claude-code" => {
                c.version =
                    Some("2.1.278 (Claude Code, build 20260901.1, arm64-apple-darwin)".into());
                c.path = Some(LONG_PATH.to_string());
                c.pkg =
                    Some("@anthropic-ai/claude-code-with-an-unreasonably-long-package-name".into());
                c.description = LONG_DESCRIPTION.to_string();
                c.running = Some(true);
                c.latest_version = Some("2.1.300".into());
                c.update_available = true;
                c.issues.push(Finding {
                    level: FindingLevel::Warn,
                    message: "The managed settings overlay at /Library/Application Support/ClaudeCode/managed-settings.json disagrees with the user settings about permissions.deny".into(),
                    remediation: Some("Reconcile the two files, then run `claude doctor` to confirm the merged view is the one you expect before the next session starts.".into()),
                });
            }
            "nono" => {
                c.installed = false;
                c.version = None;
            }
            "openwiki" => {
                c.version = None;
                c.issues.push(Finding {
                    level: FindingLevel::Error,
                    message: "OpenWiki is on PATH but its version probe failed (exit 243) after the last install attempt wrote a partial node_modules tree".into(),
                    remediation: Some("Reinstall: npm install -g openwiki".into()),
                });
            }
            _ => {}
        }
    }
    let jobs = vec![
        job(1, "install", "openwiki", JobStatus::Error),
        job(2, "update", "pi", JobStatus::Ok),
        job(3, "install", "antigravity", JobStatus::Ok),
        job(4, "update", "codex", JobStatus::Ok),
        job(5, "update", "claude-code", JobStatus::Running),
        job(6, "install", "cursor", JobStatus::Error),
        job(7, "install", "hermes", JobStatus::Ok),
        job(8, "update", "opencode", JobStatus::Ok),
        job(9, "install", "nono", JobStatus::Error),
    ];
    let insight = Insight {
        id: "stress:token-efficiency".to_string(),
        group_id: "token-efficiency",
        group_name: "Token Efficiency",
        headline: "Token Efficiency saved 310.4M tokens across 371,700 commands this quarter (49.3% average reduction), which is more than the whole context budget of every session combined.".to_string(),
        evidence: "rtk gain --format json".to_string(),
        action: Some(InsightAction {
            label: "Open Token Efficiency".to_string(),
            group_id: "token-efficiency".to_string(),
        }),
        rank: AttentionRank::Insight,
    };
    Shared {
        capabilities: caps,
        have_first_detection: true,
        scope: scope.to_string(),
        last_detection_at: None,
        projects: vec![
            ProjectSummary {
                id: "ade-bootstrapper".into(),
                dir: "/Users/owner/Code/p4gs/ADE-Bootstrapper".into(),
                config_ok: true,
            },
            ProjectSummary {
                id: "a-project-whose-directory-is-nested-very-deep".into(),
                dir: "/Users/owner/Code/clients/enterprise/platform/services/identity/provider/gateway/edge".into(),
                config_ok: false,
            },
        ],
        jobs,
        toast: Some(
            "Checked for updates: 3 providers can be updated, 1 provider could not be reached (network timeout after 30s)"
                .into(),
        ),
        state_warning: Some(
            "gui.json could not be written: Permission denied (os error 13) at /Users/owner/Library/Application Support/ade/gui.json"
                .into(),
        ),
        insights: vec![insight],
        ..Shared::default()
    }
}

fn stress_local() -> LocalState {
    LocalState {
        query: "c".to_string(),
        ..LocalState::default()
    }
}

fn paint(scope: String, dark: bool) -> impl FnMut(&mut egui::Ui) + 'static {
    let shared = stress_shared(&scope);
    let mut local = stress_local();
    move |ui| {
        let p = crate::theme::palette(dark);
        let _ = frame(ui, &p, &shared, &mut local, false);
    }
}

fn render(scope: &str, dark: bool, size: egui::Vec2) -> Harness<'static> {
    harness_themed(size, dark, paint(scope.to_string(), dark))
}

// ───────────────────────── the structural invariant ─────────────────────────

struct Box_ {
    role: Role,
    label: String,
    rect: egui::Rect,
    /// The nearest container the node sits in. Overlaps are judged between
    /// siblings only: egui exposes no clip rects to AccessKit, so content
    /// scrolled out of a pane keeps its bounds and would read as colliding
    /// with whatever pane it scrolled behind. Siblings share one layout and
    /// one clip, so a sibling overlap is always real.
    parent: String,
}

fn collect(node: egui_kittest::Node<'_>, out: &mut Vec<Box_>) {
    collect_under(node, String::from("root"), out);
}

fn collect_under(node: egui_kittest::Node<'_>, parent: String, out: &mut Vec<Box_>) {
    let ak = node.accesskit_node();
    if let Some(b) = ak.raw_bounds().or_else(|| ak.bounding_box()) {
        let rect = egui::Rect::from_min_max(
            egui::pos2(b.x0 as f32, b.y0 as f32),
            egui::pos2(b.x1 as f32, b.y1 as f32),
        );
        // egui files a Label node's text under `value`, everything else under `label`.
        let label = ak.label().or_else(|| ak.value()).unwrap_or_default();
        if !label.is_empty() {
            out.push(Box_ {
                role: ak.role(),
                label,
                rect,
                parent: parent.clone(),
            });
        }
    }
    let id = format!("{:?}", ak.id());
    for child in node.children() {
        collect_under(child, id.clone(), out);
    }
}

/// Every labelled node stays inside the window horizontally, and no two
/// sibling text nodes overlap by more than a pixel in both axes. Returns the
/// violations as human-readable lines so one run reports everything.
fn violations(h: &Harness<'_>, width: f32, height: f32) -> Vec<String> {
    let mut boxes = Vec::new();
    collect(h.root(), &mut boxes);
    // Vertical scroll content past the window's bottom is legitimately
    // hidden; only what can be seen can collide.
    boxes.retain(|b| b.rect.top() < height && b.rect.bottom() > 0.0);
    let mut out = Vec::new();
    for b in &boxes {
        if b.rect.right() > width + 0.5 || b.rect.left() < -0.5 {
            out.push(format!(
                "OVERFLOW {:?} {:?} spans x {:.0}..{:.0} in a {width:.0}pt window",
                b.role,
                b.label,
                b.rect.left(),
                b.rect.right()
            ));
        }
    }
    let text = |r: Role| matches!(r, Role::Label);
    for (i, a) in boxes.iter().enumerate() {
        if !text(a.role) {
            continue;
        }
        for b in boxes.iter().skip(i + 1) {
            if !text(b.role) || b.parent != a.parent {
                continue;
            }
            let x = a.rect.intersect(b.rect);
            if x.width() > 1.0 && x.height() > 1.0 {
                out.push(format!(
                    "OVERLAP {:?} × {:?} by {:.0}×{:.0}pt",
                    a.label,
                    b.label,
                    x.width(),
                    x.height()
                ));
            }
        }
    }
    out
}

fn assert_clean(h: &Harness<'_>, size: egui::Vec2, what: &str) {
    let v = violations(h, size.x, size.y);
    assert!(
        v.is_empty(),
        "{what}: {} violation(s)\n{}",
        v.len(),
        v.join("\n")
    );
}

// ───────────────────────── every scope, both themes ─────────────────────────

fn every_scope() -> Vec<&'static str> {
    let mut scopes: Vec<&'static str> = vec!["overview", "projects", "activity"];
    scopes.extend(CAPABILITY_GROUPS.iter().map(|g| g.id));
    scopes
}

#[test]
fn no_widget_overflows_or_overlaps_on_any_scope_at_the_minimum_window() {
    let mut failures = Vec::new();
    for scope in every_scope() {
        for dark in [true, false] {
            let h = render(scope, dark, MIN_WINDOW);
            for v in violations(&h, MIN_WINDOW.x, MIN_WINDOW.y) {
                failures.push(format!(
                    "[{scope} {}] {v}",
                    if dark { "dark" } else { "light" }
                ));
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{} violation(s) across scopes\n{}",
        failures.len(),
        failures.join("\n")
    );
}

#[test]
fn no_widget_overflows_or_overlaps_on_any_scope_at_the_default_window() {
    let mut failures = Vec::new();
    for scope in every_scope() {
        let h = render(scope, false, DEFAULT_WINDOW);
        for v in violations(&h, DEFAULT_WINDOW.x, DEFAULT_WINDOW.y) {
            failures.push(format!("[{scope}] {v}"));
        }
    }
    assert!(
        failures.is_empty(),
        "{} violation(s) across scopes\n{}",
        failures.len(),
        failures.join("\n")
    );
}

/// The invariant must be able to fail: a label wider than the window trips it.
#[test]
fn the_invariant_catches_an_overflowing_label() {
    let h = harness_themed(egui::vec2(200.0, 60.0), true, |ui| {
        ui.add(egui::Label::new("x".repeat(400)).wrap_mode(egui::TextWrapMode::Extend));
    });
    assert!(!violations(&h, 200.0, 60.0).is_empty());
}

/// …and two labels painted over each other.
#[test]
fn the_invariant_catches_overlapping_labels() {
    let h = harness_themed(egui::vec2(300.0, 60.0), true, |ui| {
        ui.horizontal(|ui| {
            // Siblings in one row, pulled back over each other.
            ui.spacing_mut().item_spacing.x = -30.0;
            ui.label("first label");
            ui.label("second label");
        });
    });
    assert!(violations(&h, 300.0, 60.0)
        .iter()
        .any(|v| v.starts_with("OVERLAP")));
}

// ───────────────────────── snapshots for the eye ─────────────────────────

#[test]
fn snapshot_stress_group_min_dark() {
    let mut h = render("coding-harness", true, MIN_WINDOW);
    assert_clean(&h, MIN_WINDOW, "coding-harness dark min");
    h.snapshot("stress_group_min_dark");
}

#[test]
fn snapshot_stress_group_min_light() {
    let mut h = render("coding-harness", false, MIN_WINDOW);
    h.snapshot("stress_group_min_light");
}

#[test]
fn snapshot_stress_group_default_light() {
    let mut h = render("coding-harness", false, DEFAULT_WINDOW);
    h.snapshot("stress_group_default_light");
}

#[test]
fn snapshot_stress_semantic_search_min_light() {
    let mut h = render("semantic-search", false, MIN_WINDOW);
    h.snapshot("stress_semantic_min_light");
}

#[test]
fn snapshot_stress_overview_min_light() {
    let mut h = render("overview", false, MIN_WINDOW);
    h.snapshot("stress_overview_min_light");
}

#[test]
fn snapshot_stress_overview_min_dark() {
    let mut h = render("overview", true, MIN_WINDOW);
    h.snapshot("stress_overview_min_dark");
}

#[test]
fn snapshot_stress_projects_min_light() {
    let mut h = render("projects", false, MIN_WINDOW);
    h.snapshot("stress_projects_min_light");
}

#[test]
fn snapshot_stress_activity_min_light() {
    let mut h = render("activity", false, MIN_WINDOW);
    h.snapshot("stress_activity_min_light");
}

#[test]
fn snapshot_stress_sidebar_hidden_min_light() {
    let shared = stress_shared("coding-harness");
    let mut local = stress_local();
    local.sidebar_hidden = true;
    let mut h = harness_themed(MIN_WINDOW, false, move |ui| {
        let p = crate::theme::palette(false);
        let _ = frame(ui, &p, &shared, &mut local, false);
    });
    h.snapshot("stress_group_nosidebar_min_light");
}
