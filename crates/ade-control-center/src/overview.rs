//! The Overview — the landing page, on the Cork Start Page pattern (Phase C).
//!
//! One screen that answers, in order: the verdict sentence, what needs
//! attention (a box with a designed view for every one of its four states —
//! checking / has-items / all-clear / check-failed), and how each capability
//! group is covered. Healthy is silent: a covered coverage row is plain text
//! plus a muted version, with zero status pigment — color and marks are spent
//! only where something is wrong.
//!
//! Pure view: everything rendered here is computed by `build_verdict` in
//! ade-core; this module draws it and reports what the owner did. It never
//! recomputes health.

use crate::app::{ax_button, ax_ghost_button, content_column, row_hairline, status_dot_at, Dot};
use crate::theme;
use ade_core::gui::insights::Insight;
use ade_core::gui::inventory::LifecycleAction;
use ade_core::gui::verdict::{
    AttentionItem, CoverageRow, CoverageState, HealthCounts, HealthVerdict, Verdict,
};
use ade_core::types::FindingLevel;
use egui::RichText;
use std::collections::HashSet;

/// What the owner did on the Overview. The shell decides what each means.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum OverviewEvent {
    /// Run the one action an attention item offers.
    Start {
        capability_id: String,
        action: LifecycleAction,
    },
    /// Show this capability group's page.
    Open(String),
    /// Re-run detection after a failed pass.
    Retry,
    /// Open the bulk-install preview checklist.
    AskBulkInstall,
    /// Phase H: dismiss one insight, by its stable id.
    DismissInsight(String),
    /// Phase I: write the posture evidence report to disk.
    ExportReport,
}

/// Everything the Overview needs to draw itself.
pub(crate) struct OverviewState<'a> {
    pub verdict: &'a HealthVerdict,
    /// A detection pass is running with no completed pass yet. The attention
    /// box also treats a `Verdict::Unknown` (no data at all) as checking, so
    /// the moment before the first pass starts cannot render a false
    /// all-clear.
    pub checking: bool,
    /// The last detection pass failed outright. NO live producer exists yet:
    /// `detect_capabilities` is total — every probe failure or timeout becomes
    /// a `Finding` on its capability, never a pass-level error — so live
    /// callers pass `None`. The renderer ships first; the engine gains a
    /// producer only if a real pass-level failure path ever exists.
    pub check_failed: Option<&'a str>,
    /// How many capabilities are missing machine-wide AND have an automated
    /// install recipe — the population "Install all missing…" would act on.
    /// The action appears only at two or more: for a single gap the attention
    /// row's own Install button already is the bulk action.
    pub bulk_installable: usize,
    /// Capability ids with a job currently running — an attention action for
    /// one of these renders a busy state instead of its normal button, the
    /// same slot `row.rs`'s `primary_action` already uses.
    pub busy: &'a HashSet<String>,
    /// Phase H — every real insight the engine has computed this poll,
    /// worst/most-actionable first. NOT pre-filtered: `dismissed` below is
    /// applied here, at render time, same as everywhere else in the app that
    /// separates "the facts" from "what the owner chose to hide."
    pub insights: &'a [Insight],
    /// Phase H — dismissed `Insight::id`s.
    pub dismissed: &'a HashSet<String>,
}

/// Which of the four designed states the attention box is in.
enum AttentionBox<'a> {
    /// Detection running, nothing (or only stale data) to show. Prior items
    /// from the last completed pass render dimmed rather than blanking.
    Checking { prior: Vec<&'a AttentionItem> },
    /// The ranked action list.
    Items(Vec<&'a AttentionItem>),
    /// Designed quiet state — not an empty box.
    AllClear,
    /// The last pass errored; the box says so and offers a retry.
    CheckFailed(&'a str),
}

/// The one decision this view makes: which designed state applies. Pure and
/// unit-tested; everything else is drawing.
fn attention_box<'a>(state: &OverviewState<'a>) -> AttentionBox<'a> {
    if let Some(error) = state.check_failed {
        return AttentionBox::CheckFailed(error);
    }
    let items: Vec<&AttentionItem> = state.verdict.action_items().collect();
    if state.checking || state.verdict.verdict == Verdict::Unknown {
        return AttentionBox::Checking { prior: items };
    }
    if items.is_empty() {
        AttentionBox::AllClear
    } else {
        AttentionBox::Items(items)
    }
}

/// Phase I — the first-run guided path (Davit's lesson): once detection has
/// actually run, a machine where 80%+ of enabled capabilities are missing or
/// broken reads as "eighteen 'Not installed' rows" — the wall of red the
/// plan's own research explicitly warned against — rather than "a couple of
/// things need attention." At that point (and only that point) a short
/// welcome sentence renders ABOVE the existing attention box, offering the
/// SAME bulk-install-preview trigger the "Install all missing…" footer offer
/// already uses (`OverviewEvent::AskBulkInstall`, Phase D's modal — not a
/// second one). Every other state, including a handful of real gaps, is
/// unaffected: the attention box beneath the banner still renders exactly as
/// it always has, so nothing real is hidden.
const FRESH_MACHINE_OK_RATIO: f64 = 0.2;

fn is_fresh_machine(counts: &HealthCounts) -> bool {
    counts.enabled > 0 && (counts.ok as f64) <= (counts.enabled as f64) * FRESH_MACHINE_OK_RATIO
}

pub(crate) fn overview(
    ui: &mut egui::Ui,
    p: &theme::Palette,
    state: &OverviewState<'_>,
) -> Option<OverviewEvent> {
    let mut event = None;
    content_column(ui, |ui| {
        // A confidently-worded headline computed from STALE data can outweigh
        // the "the last check failed" notice beneath it — when the pass that
        // produced this headline errored, its visual weight now honestly
        // matches how stale the claim is instead of staying at full strength.
        let headline_color = if state.check_failed.is_some() {
            p.muted
        } else {
            p.text
        };
        ui.label(
            RichText::new(&state.verdict.headline)
                .color(headline_color)
                .font(theme::semibold(theme::SIZE_TITLE)),
        );
        if !state.verdict.detail.is_empty() {
            ui.add_space(theme::Space::S4.px());
            ui.label(
                RichText::new(&state.verdict.detail)
                    .color(p.muted)
                    .size(theme::SIZE_BODY),
            );
        }
        ui.add_space(theme::Space::S16.px());

        let box_state = attention_box(state);
        // Settles TRUE only while the box is showing the designed all-clear
        // state — read every frame (not just from inside `all_clear`) so a
        // later re-entry replays the payoff instead of reusing a stale,
        // already-settled animation value.
        let just_cleared = ui.ctx().animate_bool_with_time(
            egui::Id::new("attention-all-clear"),
            matches!(box_state, AttentionBox::AllClear),
            theme::DURATION_PAYOFF,
        );
        // The bulk preview is offered only from a settled box: while checking
        // the population is stale, and after a failed pass it is unknown.
        let offer_bulk = state.bulk_installable >= 2
            && matches!(box_state, AttentionBox::Items(_) | AttentionBox::AllClear);
        let dark_mode = ui.visuals().dark_mode;

        // Phase I: the fresh-machine welcome — only ever true alongside a
        // real ranked Items list (a machine this bare cannot be AllClear,
        // and Checking/CheckFailed already returned their own designed
        // state above), so this never fights the other three attention-box
        // states for the same screen space.
        if matches!(box_state, AttentionBox::Items(_)) && is_fresh_machine(&state.verdict.counts) {
            welcome_banner(ui, p, dark_mode, state.bulk_installable, &mut event);
            ui.add_space(theme::Space::S16.px());
        }

        theme::section_surface(p, dark_mode).show(ui, |ui| {
            match box_state {
                AttentionBox::Checking { prior } => {
                    ui.horizontal(|ui| {
                        ui.add(egui::Spinner::new().size(theme::SIZE_BODY).color(p.muted));
                        ui.label(
                            RichText::new("Checking your environment…")
                                .color(p.muted)
                                .size(theme::SIZE_BODY),
                        );
                    });
                    if !prior.is_empty() {
                        ui.add_space(theme::Space::S4.px());
                        // A re-check: the last pass's items stay visible, dimmed —
                        // stale beats blank, and the dimming says stale.
                        ui.scope(|ui| {
                            ui.set_opacity(0.55);
                            attention_rows(ui, p, &prior, state.busy, &mut event);
                        });
                    }
                }
                AttentionBox::Items(items) => attention_rows(ui, p, &items, state.busy, &mut event),
                AttentionBox::AllClear => all_clear(ui, p, &state.verdict.counts, just_cleared),
                AttentionBox::CheckFailed(error) => check_failed(ui, p, error, &mut event),
            }
            if offer_bulk {
                row_hairline(ui, p);
                // Label + action, not a button floating on empty shelf: the
                // leading caption states the population the trailing ghost
                // acts on, colored INFO — the app's one "this is clickable"
                // signal, matching the "How to install …" ghosts above it.
                egui::containers::Sides::new().show(
                    ui,
                    |ui| {
                        ui.label(
                            RichText::new(format!(
                                "{} {} missing",
                                state.bulk_installable,
                                plural(state.bulk_installable, "capability", "capabilities"),
                            ))
                            .color(p.muted)
                            .size(theme::SIZE_CAPTION),
                        );
                    },
                    |ui| {
                        if ax_ghost_button(
                            ui,
                            "Install all missing…",
                            "Install all missing",
                            p.info,
                        )
                        .clicked()
                        {
                            event = Some(OverviewEvent::AskBulkInstall);
                        }
                    },
                );
            }
        });

        // Phase H: insights render below the hard-failure attention box —
        // "broken > uncovered > insight" as a placement rule, not a merged
        // sort — capped to the top 5, and never the ones the owner already
        // dismissed.
        let undismissed: Vec<&Insight> = state
            .insights
            .iter()
            .filter(|insight| !state.dismissed.contains(&insight.id))
            .take(5)
            .collect();
        insights_section(ui, p, &undismissed, dark_mode, &mut event);

        if !state.verdict.coverage.is_empty() {
            ui.add_space(theme::Space::S20.px());
            theme::section_heading(ui, p, "Coverage");
            ui.add_space(theme::Space::S4.px());
            theme::section_surface(p, dark_mode).show(ui, |ui| {
                for (index, row) in state.verdict.coverage.iter().enumerate() {
                    if index > 0 {
                        row_hairline(ui, p);
                    }
                    if coverage_row(ui, p, row) {
                        event = Some(OverviewEvent::Open(row.group_id.clone()));
                    }
                }
            });
        }

        ui.add_space(theme::Space::S20.px());
        // Phase I — a quiet, secondary footer action: exporting evidence is
        // a rarely-used action that should never compete with "what needs
        // attention" for top-of-page weight, so it sits last, after
        // Coverage, on every verdict state (including a broken one, where
        // attaching the export as evidence of the break is the more useful
        // moment, not less).
        egui::containers::Sides::new().show(
            ui,
            |_ui| {},
            |ui| {
                if ax_ghost_button(ui, "Export report", "Export posture report", p.muted).clicked()
                {
                    event = Some(OverviewEvent::ExportReport);
                }
            },
        );
    });
    event
}

/// Phase I — the fresh-machine welcome: one sentence plus the SAME
/// bulk-install-preview trigger the "Install all missing…" footer offer
/// uses. The button only renders when there is something automatable to set
/// up (`bulk_installable > 0`) — a fresh machine where every gap happens to
/// be manual-method-only still gets the welcome sentence, just no button
/// that could not do anything.
fn welcome_banner(
    ui: &mut egui::Ui,
    p: &theme::Palette,
    dark_mode: bool,
    bulk_installable: usize,
    event: &mut Option<OverviewEvent>,
) {
    theme::section_surface(p, dark_mode).show(ui, |ui| {
        egui::containers::Sides::new()
            .height(theme::ROW_HEIGHT)
            .shrink_left()
            .show(
                ui,
                |ui| {
                    ui.vertical(|ui| {
                        ui.spacing_mut().item_spacing.y = theme::Space::S2.px();
                        ui.label(
                            RichText::new("Welcome to ADE.")
                                .font(theme::semibold(theme::SIZE_BODY)),
                        );
                        ui.label(
                            RichText::new("Let's get your development environment covered.")
                                .color(p.muted)
                                .size(theme::SIZE_CAPTION),
                        );
                    });
                },
                |ui| {
                    if bulk_installable > 0
                        && ax_button(
                            ui,
                            "Set up your environment",
                            "Set up your environment",
                            Some(p.accent),
                            true,
                        )
                        .clicked()
                    {
                        *event = Some(OverviewEvent::AskBulkInstall);
                    }
                },
            );
    });
}

fn attention_rows(
    ui: &mut egui::Ui,
    p: &theme::Palette,
    items: &[&AttentionItem],
    busy: &HashSet<String>,
    event: &mut Option<OverviewEvent>,
) {
    for (index, item) in items.iter().enumerate() {
        if index > 0 {
            row_hairline(ui, p);
        }
        attention_row(ui, p, item, busy, event);
    }
}

/// One attention row: painted status mark, the computed sentence, its context,
/// and the ONE action the verdict derived for it.
fn attention_row(
    ui: &mut egui::Ui,
    p: &theme::Palette,
    item: &AttentionItem,
    busy: &HashSet<String>,
    event: &mut Option<OverviewEvent>,
) {
    egui::containers::Sides::new()
        .height(theme::ROW_HEIGHT)
        .shrink_left()
        .show(
            ui,
            |ui| {
                let (rect, _) =
                    ui.allocate_exact_size(egui::vec2(12.0, 12.0), egui::Sense::hover());
                // Only items at warning level or worse reach this list, so the
                // mark is a two-way split: red for an error, amber otherwise.
                let dot = if item.level == FindingLevel::Error {
                    Dot::Err
                } else {
                    Dot::Warn
                };
                status_dot_at(ui, rect, dot, p);
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = theme::Space::S2.px();
                    ui.label(RichText::new(&item.title).font(theme::semibold(theme::SIZE_BODY)));
                    let caption = item.note.as_deref().unwrap_or(&item.why);
                    if !caption.is_empty() {
                        ui.add(
                            egui::Label::new(
                                RichText::new(caption)
                                    .color(p.muted)
                                    .size(theme::SIZE_CAPTION),
                            )
                            .truncate(),
                        )
                        .on_hover_text(caption);
                    }
                });
            },
            |ui| {
                let Some(action) = &item.action else { return };
                // The first click a user can make in the app used to give
                // zero feedback while its job ran — this is `row.rs`'s
                // `primary_action` busy slot, ported verbatim.
                if busy.contains(&action.capability_id) {
                    ui.add(egui::Spinner::new().size(14.0).color(p.muted));
                    ui.label(
                        RichText::new("working…")
                            .color(p.muted)
                            .size(theme::SIZE_CAPTION),
                    );
                    return;
                }
                match &action.guidance {
                    // No automated recipe exists, so the honest offer is the
                    // instructions — a ghost button, not a primary that
                    // cannot work. Clicking runs nothing.
                    Some(guidance) => {
                        ax_ghost_button(ui, &action.label, &action.label, p.info)
                            .on_hover_text(guidance);
                    }
                    None => {
                        if ax_button(ui, &action.label, &action.label, Some(p.accent), true)
                            .clicked()
                        {
                            *event = Some(OverviewEvent::Start {
                                capability_id: action.capability_id.clone(),
                                action: action.lifecycle,
                            });
                        }
                    }
                }
            },
        );
}

// ───────────────────────── Phase H — insights ─────────────────────────

/// The insights block: up to 5 real, undismissed sentences, each with its own
/// evidence already named in the text and at most one action. Absent
/// entirely when there is nothing real to show — no placeholder card, per
/// the same honesty rule the stats strip holds.
fn insights_section(
    ui: &mut egui::Ui,
    p: &theme::Palette,
    insights: &[&Insight],
    dark_mode: bool,
    event: &mut Option<OverviewEvent>,
) {
    if insights.is_empty() {
        return;
    }
    ui.add_space(theme::Space::S20.px());
    theme::section_heading(ui, p, "Insights");
    ui.add_space(theme::Space::S4.px());
    theme::section_surface(p, dark_mode).show(ui, |ui| {
        for (index, insight) in insights.iter().enumerate() {
            if index > 0 {
                row_hairline(ui, p);
            }
            insight_row(ui, p, insight, event);
        }
    });
}

/// One insight row: the sentence, its one action (if it has one — the
/// rightmost, macOS-primary slot), and Dismiss. AX labels for both controls
/// embed the insight's own stable id: two insights about the same group can
/// both offer "Open X", and the id is what keeps their labels unique.
fn insight_row(
    ui: &mut egui::Ui,
    p: &theme::Palette,
    insight: &Insight,
    event: &mut Option<OverviewEvent>,
) {
    egui::containers::Sides::new()
        .height(theme::ROW_HEIGHT)
        .shrink_left()
        .show(
            ui,
            |ui| {
                ui.add(
                    egui::Label::new(RichText::new(&insight.headline).size(theme::SIZE_BODY))
                        .truncate(),
                )
                .on_hover_text(&insight.headline);
            },
            |ui| {
                // Right-to-left: the action (if any) is added first and
                // holds the rightmost slot; Dismiss always sits to its left.
                if let Some(action) = &insight.action {
                    let ax = format!("Insight {}: {}", insight.id, action.label);
                    if ax_ghost_button(ui, &action.label, &ax, p.info).clicked() {
                        *event = Some(OverviewEvent::Open(action.group_id.clone()));
                    }
                }
                if ax_ghost_button(
                    ui,
                    "Dismiss",
                    &format!("Dismiss insight {}", insight.id),
                    p.muted,
                )
                .clicked()
                {
                    *event = Some(OverviewEvent::DismissInsight(insight.id.clone()));
                }
            },
        );
}

/// The all-clear state: a painted check stroke, the sentence, and the real
/// numbers behind it. Counts animate on change (~260ms) but always settle on
/// the true value. `just_cleared` (0..1) is the payoff animation — settled at
/// 1.0 once the box has been showing this state for `DURATION_PAYOFF`, so
/// transitioning INTO all-clear pops the check in rather than swapping it
/// straight from the previous content.
fn all_clear(ui: &mut egui::Ui, p: &theme::Palette, counts: &HealthCounts, just_cleared: f32) {
    egui::containers::Sides::new()
        .height(theme::ROW_HEIGHT)
        .shrink_left()
        .show(
            ui,
            |ui| {
                // The payoff moment gets the icon set's largest size — a
                // painted check, never a text glyph (those land on the emoji
                // path on macOS).
                let side = crate::icons::IconSize::S16.px();
                let (rect, _) =
                    ui.allocate_exact_size(egui::vec2(side, side), egui::Sense::hover());
                let icon_side = side * egui::lerp(0.85..=1.0, just_cleared);
                let icon_rect =
                    egui::Rect::from_center_size(rect.center(), egui::Vec2::splat(icon_side));
                crate::icons::check(ui, icon_rect, p.ok.gamma_multiply(just_cleared));
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = theme::Space::S2.px();
                    ui.label(
                        RichText::new("Everything is covered.")
                            .font(theme::semibold(theme::SIZE_BODY)),
                    );
                    let capabilities =
                        animated_count(ui, "overview-count-capabilities", counts.enabled as usize);
                    let areas = animated_count(ui, "overview-count-areas", counts.groups_covered);
                    ui.label(
                        RichText::new(format!(
                            "{capabilities} {} across {areas} {}",
                            plural(capabilities, "capability", "capabilities"),
                            plural(areas, "area", "areas"),
                        ))
                        .color(p.muted)
                        .size(theme::SIZE_CAPTION),
                    );
                });
            },
            |_ui| {},
        );
}

/// The check-failed state: what happened, verbatim, and one way forward.
fn check_failed(
    ui: &mut egui::Ui,
    p: &theme::Palette,
    error: &str,
    event: &mut Option<OverviewEvent>,
) {
    egui::containers::Sides::new()
        .height(theme::ROW_HEIGHT)
        .shrink_left()
        .show(
            ui,
            |ui| {
                let (rect, _) =
                    ui.allocate_exact_size(egui::vec2(12.0, 12.0), egui::Sense::hover());
                status_dot_at(ui, rect, Dot::Err, p);
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = theme::Space::S2.px();
                    ui.label(
                        RichText::new("The last check failed.")
                            .font(theme::semibold(theme::SIZE_BODY)),
                    );
                    ui.add(
                        egui::Label::new(
                            RichText::new(error)
                                .color(p.muted)
                                .size(theme::SIZE_CAPTION),
                        )
                        .truncate(),
                    )
                    .on_hover_text(error);
                });
            },
            |ui| {
                if ax_button(ui, "Retry", "Retry check", Some(p.accent), true).clicked() {
                    *event = Some(OverviewEvent::Retry);
                }
            },
        );
}

/// One coverage row. Healthy is silent: no mark, no tint — the group's name
/// and its provider fact. Only Broken/Uncovered rows spend pigment, and an
/// off group recedes into disabled text. Returns true when clicked.
///
/// The row's own hover truth is settled BEFORE anything paints (an
/// `allocate_exact_size` + `interact` pre-pass, mirroring `nav_row`) rather
/// than after, the way `capability_row`'s backdrop still does — that's what
/// lets the chevron below share the exact same `wash` value as the backdrop
/// instead of painting at a static dim that never brightened on hover.
fn coverage_row(ui: &mut egui::Ui, p: &theme::Palette, row: &CoverageRow) -> bool {
    let size = egui::vec2(ui.available_width(), theme::COVERAGE_ROW_HEIGHT);
    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click());
    let ax = format!("Open {}", row.group_name);
    response
        .clone()
        .widget_info(move || egui::WidgetInfo::labeled(egui::WidgetType::Button, true, ax.clone()));
    // ~100ms wash fade, matching every other pointable row.
    let wash = ui.ctx().animate_bool_with_time(
        egui::Id::new(("coverage-hover-wash", &row.group_id)),
        response.hovered(),
        theme::DURATION_HOVER,
    );
    if wash > 0.0 {
        ui.painter().rect_filled(
            rect.expand2(egui::vec2(8.0, 0.0)),
            egui::CornerRadius::same(theme::RADIUS_CONTROL),
            p.row_hover.gamma_multiply(wash),
        );
    }
    let mut content = ui.new_child(egui::UiBuilder::new().max_rect(rect));
    egui::containers::Sides::new()
        .height(theme::COVERAGE_ROW_HEIGHT)
        .shrink_left()
        .show(
            &mut content,
            |ui| {
                // The mark slot is reserved on every row so names align,
                // and painted only where something is wrong.
                let (dot, _) = ui.allocate_exact_size(egui::vec2(12.0, 12.0), egui::Sense::hover());
                match row.state {
                    CoverageState::Broken => status_dot_at(ui, dot, Dot::Err, p),
                    CoverageState::Uncovered => status_dot_at(ui, dot, Dot::Warn, p),
                    CoverageState::Covered | CoverageState::Off => {}
                }
                let name_color = if row.state == CoverageState::Off {
                    p.disabled_text
                } else {
                    p.text
                };
                ui.label(
                    RichText::new(&row.group_name)
                        .color(name_color)
                        .size(theme::SIZE_BODY),
                );
            },
            |ui| {
                // Right-to-left: the rightmost element is added first — the
                // chevron, a quiet navigation affordance saying this row
                // opens the group's page. Neutral ink, not status pigment,
                // so healthy rows stay silent — but it now clears the app's
                // own AA floor at rest and brightens with the row's hover,
                // instead of sitting under it permanently. 12x12 to match
                // the leading status-dot slot (was 10x10).
                let (chevron, _) =
                    ui.allocate_exact_size(egui::vec2(12.0, 12.0), egui::Sense::hover());
                crate::icons::chevron_right(
                    ui,
                    chevron,
                    p.muted.gamma_multiply(0.85 + 0.15 * wash),
                );
                match row.state {
                    CoverageState::Covered => {
                        if let Some(version) = &row.provider_version {
                            ui.label(
                                RichText::new(version)
                                    .color(p.muted)
                                    .font(egui::FontId::monospace(theme::SIZE_CAPTION)),
                            );
                        }
                        if let Some(provider) = &row.provider {
                            ui.label(RichText::new(provider).color(p.text).size(theme::SIZE_BODY));
                        }
                    }
                    CoverageState::Broken => {
                        ui.label(
                            RichText::new(&row.summary)
                                .color(p.err)
                                .size(theme::SIZE_BODY),
                        );
                    }
                    CoverageState::Uncovered => {
                        ui.label(
                            RichText::new(&row.summary)
                                .color(p.warn)
                                .size(theme::SIZE_BODY),
                        );
                    }
                    CoverageState::Off => {
                        ui.label(
                            RichText::new(&row.summary)
                                .color(p.disabled_text)
                                .size(theme::SIZE_BODY),
                        );
                    }
                }
            },
        );
    response.clicked()
}

/// Interpolate a displayed count toward its true value (~260ms). The first
/// observation lands instantly and the settled value is always the true one —
/// only the transition is visual. Shared with the bulk-preview footer.
pub(crate) fn animated_count(ui: &egui::Ui, id: &str, value: usize) -> usize {
    ui.ctx()
        .animate_value_with_time(egui::Id::new(id), value as f32, theme::DURATION_VALUE)
        .round() as usize
}

pub(crate) fn plural<'a>(n: usize, one: &'a str, many: &'a str) -> &'a str {
    if n == 1 {
        one
    } else {
        many
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testkit::{cap, harness_themed};
    use ade_core::gui::inventory::{CapabilityStatus, CAPABILITIES};
    use ade_core::gui::verdict::build_verdict;
    use egui_kittest::kittest::Queryable;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Mutex};

    fn all_healthy() -> Vec<CapabilityStatus> {
        CAPABILITIES.iter().map(|def| cap(def.id)).collect()
    }

    /// One gap (nono), one broken tool (rtk), one manual gap (semantic search)
    /// — the attention list's three action shapes side by side.
    fn mixed() -> Vec<CapabilityStatus> {
        let mut caps = all_healthy();
        let hole = caps.iter_mut().find(|c| c.id == "nono").unwrap();
        hole.installed = false;
        hole.version = None;
        let broken = caps.iter_mut().find(|c| c.id == "rtk").unwrap();
        broken.version = None;
        broken.issues.push(ade_core::types::Finding::error(
            "RTK is on PATH but its version probe failed",
        ));
        for id in ["cocoindex", "ccc"] {
            let gone = caps.iter_mut().find(|c| c.id == id).unwrap();
            gone.installed = false;
            gone.version = None;
        }
        caps
    }

    fn paint(
        caps: Vec<CapabilityStatus>,
        dark: bool,
        checking: bool,
        check_failed: Option<&'static str>,
        sink: Arc<Mutex<Vec<OverviewEvent>>>,
    ) -> impl FnMut(&mut egui::Ui) + 'static {
        paint_busy(caps, dark, checking, check_failed, HashSet::new(), sink)
    }

    fn paint_busy(
        caps: Vec<CapabilityStatus>,
        dark: bool,
        checking: bool,
        check_failed: Option<&'static str>,
        busy: HashSet<String>,
        sink: Arc<Mutex<Vec<OverviewEvent>>>,
    ) -> impl FnMut(&mut egui::Ui) + 'static {
        paint_full(
            caps,
            dark,
            checking,
            check_failed,
            busy,
            Vec::new(),
            HashSet::new(),
            sink,
        )
    }

    /// The full constructor every `paint*` helper above delegates to —
    /// Phase H's `insights`/`dismissed` are real parameters here, defaulted
    /// to empty everywhere except the insight-specific tests below.
    #[allow(clippy::too_many_arguments)]
    fn paint_full(
        caps: Vec<CapabilityStatus>,
        dark: bool,
        checking: bool,
        check_failed: Option<&'static str>,
        busy: HashSet<String>,
        insights: Vec<Insight>,
        dismissed: HashSet<String>,
        sink: Arc<Mutex<Vec<OverviewEvent>>>,
    ) -> impl FnMut(&mut egui::Ui) + 'static {
        move |ui| {
            let p = crate::theme::palette(dark);
            let verdict = build_verdict(&caps);
            // The live shell computes this the same way, from the same slice.
            let bulk_installable = ade_core::gui::verdict::bulk_install_candidates(&caps)
                .iter()
                .filter(|candidate| candidate.automatable())
                .count();
            let state = OverviewState {
                verdict: &verdict,
                checking,
                check_failed,
                bulk_installable,
                busy: &busy,
                insights: &insights,
                dismissed: &dismissed,
            };
            if let Some(event) = overview(ui, &p, &state) {
                sink.lock().expect("sink").push(event);
            }
        }
    }

    /// State (b): the ranked list — a broken tool, a plain gap, a manual gap —
    /// each with its one action, over the silent-healthy coverage list.
    #[test]
    fn snapshot_the_attention_items_state() {
        let sink = Arc::default();
        let mut h = harness_themed(
            egui::vec2(760.0, 820.0),
            true,
            paint(mixed(), true, false, None, sink),
        );
        h.snapshot("overview_attention");
    }

    /// State (a): a re-check in flight — spinner line on top, the last pass's
    /// items dimmed underneath rather than blanked.
    #[test]
    fn snapshot_the_checking_state_with_prior_items_dimmed() {
        let sink = Arc::default();
        let mut h = harness_themed(
            egui::vec2(760.0, 820.0),
            true,
            paint(mixed(), true, true, None, sink),
        );
        // The prior items stay in the tree while dimmed.
        assert!(h.query_by_label("Install nono").is_some());
        h.snapshot("overview_checking");
    }

    /// State (c): the designed quiet state, in the light appearance — painted
    /// check stroke, the sentence, and the true counts.
    #[test]
    fn snapshot_the_all_clear_state_light() {
        let sink = Arc::default();
        let mut h = harness_themed(
            egui::vec2(760.0, 620.0),
            false,
            paint(all_healthy(), false, false, None, sink),
        );
        assert!(h
            .query_by_label("18 capabilities across 10 areas")
            .is_some());
        h.snapshot("overview_all_clear");
    }

    /// State (d): the last pass failed — stated verbatim, with a retry. Driven
    /// by the constructor: the engine has no pass-level failure to report yet
    /// (`detect_capabilities` is total), so live callers pass `None`.
    #[test]
    fn snapshot_the_check_failed_state() {
        let sink = Arc::default();
        let mut h = harness_themed(
            egui::vec2(760.0, 620.0),
            true,
            paint(
                all_healthy(),
                true,
                false,
                Some("detection did not complete: worker thread panicked"),
                sink,
            ),
        );
        h.snapshot("overview_check_failed");
    }

    #[test]
    fn the_attention_box_picks_the_right_state() {
        let healthy = build_verdict(&all_healthy());
        let problems = build_verdict(&mixed());
        let unknown = build_verdict(&[]);
        let busy = HashSet::new();
        let insights: Vec<Insight> = Vec::new();
        let dismissed: HashSet<String> = HashSet::new();
        let state = |verdict, checking, check_failed| OverviewState {
            verdict,
            checking,
            check_failed,
            bulk_installable: 0,
            busy: &busy,
            insights: &insights,
            dismissed: &dismissed,
        };
        assert!(matches!(
            attention_box(&state(&healthy, false, None)),
            AttentionBox::AllClear
        ));
        assert!(matches!(
            attention_box(&state(&problems, false, None)),
            AttentionBox::Items(items) if items.len() == 3
        ));
        // Checking keeps the prior items rather than blanking them.
        assert!(matches!(
            attention_box(&state(&problems, true, None)),
            AttentionBox::Checking { prior } if prior.len() == 3
        ));
        // No data at all is checking, never a false all-clear.
        assert!(matches!(
            attention_box(&state(&unknown, false, None)),
            AttentionBox::Checking { prior } if prior.is_empty()
        ));
        // A failed pass outranks everything else.
        assert!(matches!(
            attention_box(&state(&problems, true, Some("boom"))),
            AttentionBox::CheckFailed("boom")
        ));
    }

    /// Clicking an attention action reports the exact capability and lifecycle
    /// the verdict computed — the view adds nothing of its own.
    #[test]
    fn an_attention_action_reports_the_verdicts_command() {
        let seen: Arc<Mutex<Vec<OverviewEvent>>> = Default::default();
        let sink = Arc::clone(&seen);
        let mut h = harness_themed(
            egui::vec2(760.0, 820.0),
            true,
            paint(mixed(), true, false, None, sink),
        );
        h.get_by_label("Reinstall RTK").click();
        h.run_steps(2);
        h.get_by_label("Install nono").click();
        h.run_steps(2);
        assert_eq!(
            seen.lock().expect("sink").clone(),
            vec![
                OverviewEvent::Start {
                    capability_id: "rtk".to_string(),
                    action: LifecycleAction::Reinstall,
                },
                OverviewEvent::Start {
                    capability_id: "nono".to_string(),
                    action: LifecycleAction::Install,
                },
            ]
        );
    }

    /// An attention action for a capability with a job already running
    /// renders the busy slot (spinner + "working…") instead of its normal
    /// button — and the button's accessible label leaves the tree, so a
    /// second click can't queue a second job for the same capability.
    #[test]
    fn a_busy_attention_action_shows_working_instead_of_its_button() {
        let seen: Arc<Mutex<Vec<OverviewEvent>>> = Default::default();
        let sink = Arc::clone(&seen);
        let busy: HashSet<String> = ["nono".to_string()].into();
        let mut h = harness_themed(
            egui::vec2(760.0, 820.0),
            true,
            paint_busy(mixed(), true, false, None, busy, sink),
        );
        assert!(h.query_by_label("Install nono").is_none());
        assert!(h.get_all_by_label("working…").next().is_some());
        // The OTHER attention action (rtk's Reinstall) is untouched — busy is
        // per-capability, not a blanket freeze of the whole list.
        assert!(h.query_by_label("Reinstall RTK").is_some());
        h.snapshot("overview_attention_busy");
        assert!(seen.lock().expect("sink").is_empty());
    }

    /// A manual gap offers instructions, not a command: the ghost button is in
    /// the tree, and clicking it starts nothing.
    #[test]
    fn a_manual_gap_offers_guidance_not_a_command() {
        let seen: Arc<Mutex<Vec<OverviewEvent>>> = Default::default();
        let sink = Arc::clone(&seen);
        let mut h = harness_themed(
            egui::vec2(760.0, 820.0),
            true,
            paint(mixed(), true, false, None, sink),
        );
        h.get_by_label("How to install CocoIndex").click();
        h.run_steps(2);
        assert!(seen.lock().expect("sink").is_empty());
    }

    /// Clicking a coverage row asks to open that group's page.
    #[test]
    fn a_coverage_row_opens_its_group() {
        let seen: Arc<Mutex<Vec<OverviewEvent>>> = Default::default();
        let sink = Arc::clone(&seen);
        let mut h = harness_themed(
            egui::vec2(760.0, 820.0),
            true,
            paint(all_healthy(), true, false, None, sink),
        );
        h.get_by_label("Open Secret Scanning").click();
        h.run_steps(2);
        assert_eq!(
            seen.lock().expect("sink").clone(),
            vec![OverviewEvent::Open("secret-scanning".to_string())]
        );
    }

    #[test]
    fn retry_after_a_failed_check_reports_a_refresh() {
        let seen: Arc<Mutex<Vec<OverviewEvent>>> = Default::default();
        let sink = Arc::clone(&seen);
        let mut h = harness_themed(
            egui::vec2(760.0, 620.0),
            true,
            paint(all_healthy(), true, false, Some("boom"), sink),
        );
        h.get_by_label("Retry check").click();
        h.run_steps(2);
        assert_eq!(
            seen.lock().expect("sink").clone(),
            vec![OverviewEvent::Retry]
        );
    }

    /// The bulk-preview offer appears only when it would act on two or more
    /// automatable gaps, and clicking it asks for the preview — it runs
    /// nothing itself. `mixed()` (one automatable gap) is the positive
    /// control for the absence.
    #[test]
    fn install_all_missing_appears_at_two_and_asks_for_the_preview() {
        let seen: Arc<Mutex<Vec<OverviewEvent>>> = Default::default();
        let sink = Arc::clone(&seen);
        let mut caps = all_healthy();
        for id in ["nono", "gitleaks", "openwiki"] {
            let gone = caps.iter_mut().find(|c| c.id == id).unwrap();
            gone.installed = false;
            gone.version = None;
        }
        let mut h = harness_themed(
            egui::vec2(760.0, 820.0),
            true,
            paint(caps, true, false, None, sink),
        );
        assert!(h.query_by_label("Install all missing").is_some());
        h.get_by_label("Install all missing").click();
        h.run_steps(2);
        assert_eq!(
            seen.lock().expect("sink").clone(),
            vec![OverviewEvent::AskBulkInstall]
        );

        // One automatable gap: the row's own Install button IS the bulk
        // action, so the extra offer stays absent.
        let sink = Arc::default();
        let h = harness_themed(
            egui::vec2(760.0, 820.0),
            true,
            paint(mixed(), true, false, None, sink),
        );
        assert!(h.query_by_label("Install all missing").is_none());
    }

    /// The animated caption always settles on the TRUE value after a change —
    /// the interpolation is visual only.
    #[test]
    fn animated_counts_settle_on_the_true_value() {
        let grow = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&grow);
        let mut h = harness_themed(egui::vec2(760.0, 620.0), true, move |ui| {
            let p = crate::theme::palette(true);
            let mut caps = all_healthy();
            if !flag.load(Ordering::Relaxed) {
                // One capability switched off: 17 enabled, still all clear.
                caps.iter_mut()
                    .find(|c| c.id == "gitleaks")
                    .unwrap()
                    .enabled = false;
            }
            let verdict = build_verdict(&caps);
            let busy = HashSet::new();
            let insights: Vec<Insight> = Vec::new();
            let dismissed: HashSet<String> = HashSet::new();
            let state = OverviewState {
                verdict: &verdict,
                checking: false,
                check_failed: None,
                bulk_installable: 0,
                busy: &busy,
                insights: &insights,
                dismissed: &dismissed,
            };
            overview(ui, &p, &state);
        });
        assert!(h
            .query_by_label("17 capabilities across 10 areas")
            .is_some());
        grow.store(true, Ordering::Relaxed);
        // Each kittest step advances time by 250ms — far past the 260ms
        // animation — so the caption must read the true value again.
        h.run_steps(6);
        assert!(h
            .query_by_label("18 capabilities across 10 areas")
            .is_some());
    }

    // ───────────────────────── Phase H — insights ─────────────────────────

    /// `action_group`'s label uses the REAL capability-group name (the same
    /// `ade_core::gui::inventory::get_group` lookup production insights are
    /// built from) so a snapshot of this fixture reads exactly like a real
    /// insight — "Open Secret Scanning", never "Open secret-scanning".
    fn insight(id: &str, headline: &str, action_group: Option<&str>) -> Insight {
        Insight {
            id: id.to_string(),
            group_id: "secret-scanning",
            group_name: "Secret Scanning",
            headline: headline.to_string(),
            evidence: "test fixture".to_string(),
            action: action_group.map(|group_id| {
                let name = ade_core::gui::inventory::get_group(group_id)
                    .map(|group| group.name)
                    .unwrap_or(group_id);
                ade_core::gui::insights::InsightAction {
                    label: format!("Open {name}"),
                    group_id: group_id.to_string(),
                }
            }),
            rank: ade_core::gui::verdict::AttentionRank::Insight,
        }
    }

    /// A real (all-healthy) environment still shows its insights — the
    /// section is orthogonal to the verdict, not gated on anything being
    /// wrong.
    #[test]
    fn snapshot_the_overview_with_insights() {
        let insights = vec![
            insight(
                "project-coverage:secret-scanning",
                "2 of 5 inspected projects have no Secret Scanning wired.",
                Some("secret-scanning"),
            ),
            insight(
                "token-gain:token-efficiency",
                "Token Efficiency saved 310.4M tokens across 12129 commands (81% average reduction).",
                None,
            ),
        ];
        let sink = Arc::default();
        let mut h = harness_themed(
            egui::vec2(760.0, 820.0),
            true,
            paint_full(
                all_healthy(),
                true,
                false,
                None,
                HashSet::new(),
                insights,
                HashSet::new(),
                sink,
            ),
        );
        assert!(h
            .query_by_label("2 of 5 inspected projects have no Secret Scanning wired.")
            .is_some());
        h.snapshot("overview_insights");
    }

    /// Clicking an insight's action reports the exact group its own action
    /// carries; clicking Dismiss reports the insight's own stable id — the
    /// view never invents either.
    #[test]
    fn insight_actions_report_the_real_group_and_the_real_id() {
        let seen: Arc<Mutex<Vec<OverviewEvent>>> = Default::default();
        let sink = Arc::clone(&seen);
        let insights = vec![insight(
            "osv-db-age:dependency-scanning",
            "The OSV vulnerability database is 21 days old.",
            Some("dependency-scanning"),
        )];
        let mut h = harness_themed(
            egui::vec2(760.0, 820.0),
            true,
            paint_full(
                all_healthy(),
                true,
                false,
                None,
                HashSet::new(),
                insights,
                HashSet::new(),
                sink,
            ),
        );
        h.get_by_label("Insight osv-db-age:dependency-scanning: Open Dependency Scanning")
            .click();
        h.run_steps(2);
        h.get_by_label("Dismiss insight osv-db-age:dependency-scanning")
            .click();
        h.run_steps(2);
        assert_eq!(
            seen.lock().expect("sink").clone(),
            vec![
                OverviewEvent::Open("dependency-scanning".to_string()),
                OverviewEvent::DismissInsight("osv-db-age:dependency-scanning".to_string()),
            ]
        );
    }

    /// A dismissed insight never renders again, and the section itself
    /// disappears once every insight has been dismissed — no empty card.
    #[test]
    fn a_dismissed_insight_is_never_rendered_again() {
        let insights = vec![insight(
            "hardened-repos:repo-hygiene",
            "1 of 3 registered projects lack commit signing enabled.",
            Some("repo-hygiene"),
        )];
        let dismissed: HashSet<String> = ["hardened-repos:repo-hygiene".to_string()].into();
        let sink = Arc::default();
        let h = harness_themed(
            egui::vec2(760.0, 820.0),
            true,
            paint_full(
                all_healthy(),
                true,
                false,
                None,
                HashSet::new(),
                insights,
                dismissed,
                sink,
            ),
        );
        assert!(h
            .query_by_label("1 of 3 registered projects lack commit signing enabled.")
            .is_none());
        assert!(h.query_by_label("INSIGHTS").is_none());
    }

    /// More than 5 real insights: only the top 5 render on the Overview —
    /// the rest are still real (a capability page can show them), just not
    /// here.
    #[test]
    fn only_the_top_five_insights_render_on_the_overview() {
        let insights: Vec<Insight> = (0..7)
            .map(|n| {
                insight(
                    &format!("fixture-{n}"),
                    &format!("Fixture insight {n}."),
                    None,
                )
            })
            .collect();
        let sink = Arc::default();
        let h = harness_themed(
            egui::vec2(760.0, 1200.0),
            true,
            paint_full(
                all_healthy(),
                true,
                false,
                None,
                HashSet::new(),
                insights,
                HashSet::new(),
                sink,
            ),
        );
        for n in 0..5 {
            assert!(
                h.query_by_label(&format!("Fixture insight {n}.")).is_some(),
                "insight {n} should be in the visible top 5"
            );
        }
        for n in 5..7 {
            assert!(
                h.query_by_label(&format!("Fixture insight {n}.")).is_none(),
                "insight {n} is beyond the top-5 cap"
            );
        }
    }

    /// No real insights at all: the section draws nothing — no placeholder
    /// heading, no empty card.
    #[test]
    fn no_insights_section_renders_when_there_are_no_real_insights() {
        let sink = Arc::default();
        let h = harness_themed(
            egui::vec2(760.0, 620.0),
            true,
            paint(all_healthy(), true, false, None, sink),
        );
        assert!(h.query_by_label("INSIGHTS").is_none());
    }

    // ───────────────────────── Phase I — export + first-run welcome ─────────────────────────

    /// A near-empty fresh machine: 18 enabled, 2 installed — 11% ok, well
    /// under the 20% threshold. Deliberately leaves two healthy so the
    /// fixture also proves the welcome is not gated on a literal zero.
    fn fresh_machine() -> Vec<CapabilityStatus> {
        let mut caps = all_healthy();
        for cap in caps.iter_mut() {
            if cap.id != "trufflehog" && cap.id != "claude-code" {
                cap.installed = false;
                cap.version = None;
            }
        }
        caps
    }

    #[test]
    fn is_fresh_machine_uses_the_documented_ratio_not_a_bare_zero() {
        let counts = |ok: u32, enabled: u32| HealthCounts {
            groups_total: 10,
            groups_covered: 0,
            enabled,
            ok,
            warnings: 0,
            errors: 0,
            missing: enabled - ok,
        };
        // Literally nothing: fresh.
        assert!(is_fresh_machine(&counts(0, 18)));
        // Exactly at the 20% boundary: still fresh (<=).
        assert!(is_fresh_machine(&counts(3, 15)));
        // One tool already present (e.g. the owner already had a harness
        // CLI) out of 18 is well under 20% — still reads as "many missing".
        assert!(is_fresh_machine(&counts(1, 18)));
        // Just over the boundary: not fresh — this is "a handful of gaps",
        // the state the existing attention-box design already handles.
        assert!(!is_fresh_machine(&counts(4, 15)));
        // A fully healthy or empty machine is never "fresh" in this sense.
        assert!(!is_fresh_machine(&counts(18, 18)));
        assert!(!is_fresh_machine(&counts(0, 0)));
    }

    /// Below the threshold (the existing `mixed()`/`all_healthy()` fixtures,
    /// both well over 20% ok): the welcome never renders, and every existing
    /// Overview state is visually unchanged by Phase I's addition.
    #[test]
    fn the_welcome_banner_never_renders_outside_the_fresh_machine_threshold() {
        for caps in [all_healthy(), mixed()] {
            let sink = Arc::default();
            let h = harness_themed(
                egui::vec2(760.0, 820.0),
                true,
                paint(caps, true, false, None, sink),
            );
            assert!(h.query_by_label("Welcome to ADE.").is_none());
        }
    }

    /// The fresh-machine state: the welcome sentence renders above the
    /// attention box, and its button reuses the EXACT SAME
    /// `AskBulkInstall` event the "Install all missing…" footer offer
    /// dispatches — no second flow.
    #[test]
    fn snapshot_the_overview_welcome_state_and_it_reuses_the_bulk_flow() {
        let seen: Arc<Mutex<Vec<OverviewEvent>>> = Default::default();
        let sink = Arc::clone(&seen);
        let mut h = harness_themed(
            egui::vec2(760.0, 900.0),
            true,
            paint(fresh_machine(), true, false, None, sink),
        );
        assert!(h.query_by_label("Welcome to ADE.").is_some());
        h.snapshot("overview_welcome");
        h.get_by_label("Set up your environment").click();
        h.run_steps(2);
        assert_eq!(
            seen.lock().expect("sink").clone(),
            vec![OverviewEvent::AskBulkInstall]
        );
    }

    /// "Export report" is present and reports its own event on every
    /// verdict shape — a rarely-used but always-available action.
    ///
    /// Height 1400 (vs. the ~760-820 other Overview tests use): the button
    /// sits below Coverage's full 10-row list, and this harness paints
    /// `overview()` directly with no `ScrollArea` — the live app wraps the
    /// same content in `egui::ScrollArea::vertical()` (`app.rs`), which is
    /// what makes the button reachable there on any window size. Without
    /// enough logical height here, egui lays the button out beyond the
    /// harness's clip rect and a synthetic click on it silently hits
    /// nothing — `get_by_label` still finds the widget (accessibility
    /// nodes are clip-independent) but `.clicked()` never fires. Tall
    /// enough for all three fixtures, including `all_healthy()`'s 10
    /// silent-but-present coverage rows.
    #[test]
    fn export_report_is_always_offered_and_reports_its_own_event() {
        for caps in [all_healthy(), mixed(), fresh_machine()] {
            let seen: Arc<Mutex<Vec<OverviewEvent>>> = Default::default();
            let sink = Arc::clone(&seen);
            let mut h = harness_themed(
                egui::vec2(760.0, 1400.0),
                true,
                paint(caps, true, false, None, sink),
            );
            h.get_by_label("Export posture report").click();
            h.run_steps(2);
            assert_eq!(
                seen.lock().expect("sink").clone(),
                vec![OverviewEvent::ExportReport]
            );
        }
    }
}
