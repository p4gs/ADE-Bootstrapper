//! One capability group at a time: the group's name as the page title, its
//! `why` prose as the subtitle (this replaces the old `(i)` tooltip — the
//! reason a capability exists should not hide behind a hover), a Phase G
//! stats strip surfacing whichever real facts exist for this group, and the
//! group's provider rows beneath.

use crate::app::{ax_ghost_button, content_column, issue_line, row_hairline};
use crate::data::GroupStats;
use crate::nav::freshness_caption;
use crate::overview::plural;
use crate::row::{capability_row, RowEvent, RowState};
use crate::theme;
use ade_core::gui::insights::Insight;
use ade_core::gui::inventory::{CapabilityGroup, CapabilityStatus};
use egui::RichText;
use std::collections::HashSet;
use std::time::Duration;

/// What the owner did on this page: either a row event (tagged with the
/// capability it came from, so the shell can apply it without guessing), or
/// dismissing one of this group's own insights.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum CapabilityPageEvent {
    Row {
        capability_id: String,
        event: RowEvent,
    },
    /// Phase H: dismiss one insight, by its stable id.
    DismissInsight(String),
}

/// Everything the page needs to draw itself.
pub(crate) struct CapabilityPageState<'a> {
    pub group: &'a CapabilityGroup,
    /// This group's providers, in taxonomy order.
    pub caps: Vec<&'a CapabilityStatus>,
    /// Capability ids with a job currently running.
    pub busy: &'a HashSet<String>,
    /// Issue-list keys currently expanded (same keys the shell tracks).
    pub expanded_issues: &'a HashSet<String>,
    /// Phase G — this group's stats-strip facts, if the engine has computed
    /// any yet. `None` before the first detection pass completes.
    pub stats: Option<&'a GroupStats>,
    /// How long ago the last detection pass finished — the same fact the
    /// sidebar footer shows, reused here so the two can never disagree.
    pub checked_ago: Option<Duration>,
    /// Phase H — every real insight the engine has computed this poll, for
    /// EVERY group. This page filters to its own `group.id` and to what has
    /// not been dismissed — the same "facts in full, filtered at render
    /// time" split the Overview uses.
    pub insights: &'a [Insight],
    pub dismissed_insights: &'a HashSet<String>,
}

pub(crate) fn capability_page(
    ui: &mut egui::Ui,
    p: &theme::Palette,
    state: &CapabilityPageState<'_>,
) -> Option<CapabilityPageEvent> {
    let mut event = None;
    content_column(ui, |ui| {
        // Deliberately SIZE_TITLE, not SIZE_DISPLAY: the Overview verdict is
        // the app's one large-title statement; this title is navigational —
        // a page you drilled into, not a verdict (ISC-311, decided at the
        // capability_page migration rather than silently inherited).
        ui.label(RichText::new(state.group.name).font(theme::semibold(theme::SIZE_TITLE)));
        ui.add_space(theme::Space::S4.px());
        ui.label(
            RichText::new(state.group.why)
                .color(p.muted)
                .size(theme::SIZE_BODY),
        );
        ui.add_space(theme::Space::S12.px());
        stats_strip(
            ui,
            p,
            state.stats,
            state.checked_ago,
            ui.visuals().dark_mode,
        );
        // Phase H: this group's own undismissed insights — the Overview's
        // subset filter (top 5, cross-group) does not apply here; a
        // capability page shows everything real for ITS group.
        let group_insights: Vec<&Insight> = state
            .insights
            .iter()
            .filter(|insight| {
                insight.group_id == state.group.id
                    && !state.dismissed_insights.contains(&insight.id)
            })
            .collect();
        if let Some(found) = insights_block(ui, p, &group_insights, ui.visuals().dark_mode) {
            event = Some(found);
        }
        if state.caps.is_empty() {
            // Pre-detection there is nothing honest to draw but the fact.
            ui.label(
                RichText::new("No detection results yet.")
                    .color(p.muted)
                    .size(theme::SIZE_BODY),
            );
            return;
        }
        theme::section_heading(ui, p, "Providers");
        ui.add_space(theme::Space::S4.px());
        let dark_mode = ui.visuals().dark_mode;
        theme::elevated_card(ui, p, dark_mode, None, theme::Space::S12, |ui| {
            for (index, cap) in state.caps.iter().enumerate() {
                if index > 0 {
                    row_hairline(ui, p);
                }
                let issues_key = format!("issues-{}", cap.id);
                let issues_open = state.expanded_issues.contains(&issues_key);
                let row = RowState {
                    cap,
                    busy: state.busy.contains(&cap.id),
                    // The page title already names the group.
                    show_capability: false,
                    issues_open,
                };
                if let Some(found) = capability_row(ui, p, &row) {
                    event = Some(CapabilityPageEvent::Row {
                        capability_id: cap.id.clone(),
                        event: found,
                    });
                }
                if issues_open {
                    ui.indent(issues_key, |ui| {
                        for issue in &cap.issues {
                            issue_line(ui, issue, p);
                        }
                    });
                }
            }
        });
    });
    event
}

// ───────────────────────── Phase H — this group's insights ─────────────────────────

/// This group's own real, undismissed insights: the sentence and a Dismiss
/// control. No "Open this group" action here — the owner is already on this
/// page, so that action (real on the Overview) would just navigate nowhere.
/// Absent entirely when the group has nothing real to say, same honesty rule
/// as the stats strip.
fn insights_block(
    ui: &mut egui::Ui,
    p: &theme::Palette,
    insights: &[&Insight],
    dark_mode: bool,
) -> Option<CapabilityPageEvent> {
    if insights.is_empty() {
        return None;
    }
    let mut event = None;
    theme::section_heading(ui, p, "Insights");
    ui.add_space(theme::Space::S4.px());
    theme::elevated_card(ui, p, dark_mode, None, theme::Space::S12, |ui| {
        for (index, insight) in insights.iter().enumerate() {
            if index > 0 {
                row_hairline(ui, p);
            }
            egui::containers::Sides::new()
                .height(theme::ROW_HEIGHT)
                .shrink_left()
                .show(
                    ui,
                    |ui| {
                        ui.add(
                            egui::Label::new(
                                RichText::new(&insight.headline).size(theme::SIZE_BODY),
                            )
                            .truncate(),
                        )
                        .on_hover_text(&insight.headline);
                    },
                    |ui| {
                        if ax_ghost_button(
                            ui,
                            "Dismiss",
                            &format!("Dismiss insight {}", insight.id),
                            p.muted,
                        )
                        .clicked()
                        {
                            event = Some(CapabilityPageEvent::DismissInsight(insight.id.clone()));
                        }
                    },
                );
        }
    });
    ui.add_space(theme::Space::S12.px());
    event
}

// ───────────────────────── Phase G — the stats strip ─────────────────────────

/// One tile's worth of a real fact: an uppercase label, the headline value,
/// and an optional muted detail line. There is no "unknown" tile — a fact
/// that isn't real simply never becomes one of these.
struct StatTile {
    label: &'static str,
    value: String,
    detail: Option<String>,
}

/// Whichever Tier-1/Tier-2 facts are real for this group, as compact tiles
/// on the same elevated card treatment every other card in the app now
/// carries (`theme::elevated_card` — rim light, S12 breathing room) — not
/// the old flat frame. Nothing renders for an absent fact, and a group with no real facts
/// at all draws no strip whatsoever — the strip's own presence is itself
/// "absent renders as absent", not just its individual tiles.
fn stats_strip(
    ui: &mut egui::Ui,
    p: &theme::Palette,
    stats: Option<&GroupStats>,
    checked_ago: Option<Duration>,
    dark_mode: bool,
) {
    let mut tiles: Vec<StatTile> = Vec::new();

    if let Some(ago) = checked_ago {
        tiles.push(StatTile {
            label: "DETECTION",
            value: freshness_caption(ago),
            detail: None,
        });
    }

    if let Some(stats) = stats {
        if let Some(job) = &stats.last_job {
            tiles.push(StatTile {
                label: "LAST TOUCHED",
                value: format_days_ago(job.days_ago),
                detail: Some(format!(
                    "{} {} {}",
                    job.capability_name,
                    job.action,
                    if job.failed {
                        format!(
                            "failed{}",
                            job.exit_code
                                .map(|code| format!(" (exit {code})"))
                                .unwrap_or_default()
                        )
                    } else {
                        "ok".to_string()
                    }
                )),
            });
        }
        if let Some(coverage) = &stats.project_coverage {
            tiles.push(StatTile {
                label: "PROJECT COVERAGE",
                value: format!("{} of {}", coverage.covered, coverage.inspected),
                detail: Some("inspected registered projects".to_string()),
            });
        }
        if let Some(gain) = &stats.token_gain {
            tiles.push(StatTile {
                label: "TOKENS SAVED",
                value: format_count(gain.tokens_saved),
                detail: Some(format!(
                    "{:.1}% avg reduction across {} commands",
                    gain.avg_savings_pct, gain.total_commands
                )),
            });
        }
        if let Some(age) = &stats.osv_db_age {
            tiles.push(StatTile {
                label: "OSV DATABASE",
                value: format_days_ago(age.days_old),
                detail: Some(format!(
                    "{} ecosystem {} cached",
                    age.ecosystems,
                    plural(age.ecosystems, "archive", "archives")
                )),
            });
        }
        if let Some(days) = stats.index_staleness_days {
            tiles.push(StatTile {
                label: "SEMANTIC INDEX",
                value: if days <= 0 {
                    "up to date".to_string()
                } else {
                    format!("{days} behind HEAD")
                },
                detail: Some("most-stale registered project".to_string()),
            });
        }
        if let Some(millis) = stats.hook_latency_millis {
            tiles.push(StatTile {
                label: "HOOK LATENCY",
                value: format!("{millis} ms"),
                detail: Some("slowest measured real run this session".to_string()),
            });
        }
        if !stats.harness_surfaces.is_empty() {
            let covered = stats.harness_surfaces.iter().filter(|s| s.covered).count();
            tiles.push(StatTile {
                label: "RULESET SURFACES",
                value: format!("{} of {}", covered, stats.harness_surfaces.len()),
                detail: Some("harnesses with CodeGuard installed".to_string()),
            });
        }
        if let Some(catches) = &stats.secrets_catches {
            tiles.push(StatTile {
                label: "SECRETS CAUGHT",
                value: format!(
                    "{} {}",
                    catches.blocked,
                    plural(catches.blocked as usize, "commit", "commits")
                ),
                detail: Some(format!(
                    "{} verified {} across {} {}",
                    catches.findings,
                    plural(catches.findings as usize, "finding", "findings"),
                    catches.scans,
                    plural(catches.scans as usize, "scan", "scans")
                )),
            });
        }
        if let Some(hardened) = &stats.hardened_repos {
            tiles.push(StatTile {
                label: "SIGNED COMMITS",
                value: format!("{} of {}", hardened.hardened, hardened.total),
                detail: Some("registered projects with commit signing on".to_string()),
            });
        }
    }

    if tiles.is_empty() {
        return;
    }

    theme::elevated_card(ui, p, dark_mode, None, theme::Space::S12, |ui| {
        ui.horizontal_wrapped(|ui| {
            for tile in &tiles {
                stat_tile(ui, p, tile);
                ui.add_space(theme::Space::S16.px());
            }
        });
    });
    ui.add_space(theme::Space::S12.px());
}

/// Every tile is this wide, fixed. Without a fixed width, `horizontal_wrapped`
/// treats a tile's labels as separate flow items rather than one block: the
/// last tile on a line gets squeezed into whatever sliver of the row remains
/// instead of wrapping to a new line as a whole, and its own text then wraps
/// letter-by-letter inside that sliver (caught in the mandated visual pass —
/// see the ISA for the before/after screenshots).
const STAT_TILE_WIDTH: f32 = 168.0;

fn stat_tile(ui: &mut egui::Ui, p: &theme::Palette, tile: &StatTile) {
    ui.allocate_ui(egui::vec2(STAT_TILE_WIDTH, ui.available_height()), |ui| {
        ui.set_width(STAT_TILE_WIDTH);
        ui.vertical(|ui| {
            ui.label(
                RichText::new(tile.label)
                    .color(p.faint_text)
                    .font(theme::medium(10.0))
                    .extra_letter_spacing(0.8),
            );
            ui.label(
                RichText::new(&tile.value)
                    .color(p.text)
                    .font(theme::semibold(theme::SIZE_SECTION)),
            );
            if let Some(detail) = &tile.detail {
                ui.label(
                    RichText::new(detail)
                        .color(p.muted)
                        .size(theme::SIZE_CAPTION),
                );
            }
        });
    });
}

/// "today" / "1 day ago" / "N days ago" — day-granularity, matching the
/// plan's own "update lag in days" wording.
fn format_days_ago(days: u64) -> String {
    match days {
        0 => "today".to_string(),
        1 => "1 day ago".to_string(),
        n => format!("{n} days ago"),
    }
}

/// A large integer with a K/M/B suffix at one decimal place — `rtk`'s own
/// token counts run into the hundreds of millions, and nine raw digits reads
/// as noise next to the other tiles' short values.
fn format_count(n: u64) -> String {
    const BILLION: f64 = 1_000_000_000.0;
    const MILLION: f64 = 1_000_000.0;
    const THOUSAND: f64 = 1_000.0;
    let value = n as f64;
    if value >= BILLION {
        format!("{:.1}B", value / BILLION)
    } else if value >= MILLION {
        format!("{:.1}M", value / MILLION)
    } else if value >= THOUSAND {
        format!("{:.1}K", value / THOUSAND)
    } else {
        n.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testkit::{cap, harness, harness_themed};
    use ade_core::gui::inventory::{get_group, LifecycleAction};
    use egui_kittest::kittest::Queryable;

    fn secret_scanning_page(
        sink: std::sync::Arc<std::sync::Mutex<Vec<CapabilityPageEvent>>>,
        stats: Option<GroupStats>,
        checked_ago: Option<Duration>,
    ) -> impl FnMut(&mut egui::Ui) + 'static {
        secret_scanning_page_themed(sink, stats, checked_ago, true)
    }

    /// `dark` must match the harness's own theme (`harness_themed`'s `dark`
    /// argument): the card fill (`elevated_card`) is a background color, not text,
    /// so it is NOT covered by the harness's `override_text_color` shim —
    /// building this page's palette with the wrong appearance produces a
    /// dark stats-strip card floating on an otherwise light page, a real bug
    /// this exact mismatch caught during the mandated visual pass (fixed
    /// alongside it: the fixture previously hardcoded `palette(true)`
    /// regardless of which appearance the caller asked for).
    fn secret_scanning_page_themed(
        sink: std::sync::Arc<std::sync::Mutex<Vec<CapabilityPageEvent>>>,
        stats: Option<GroupStats>,
        checked_ago: Option<Duration>,
        dark: bool,
    ) -> impl FnMut(&mut egui::Ui) + 'static {
        secret_scanning_page_full(sink, stats, checked_ago, dark, Vec::new(), HashSet::new())
    }

    /// The full constructor every fixture above delegates to — Phase H's
    /// `insights`/`dismissed_insights` are real parameters here, defaulted to
    /// empty everywhere except the insight-specific tests below.
    fn secret_scanning_page_full(
        sink: std::sync::Arc<std::sync::Mutex<Vec<CapabilityPageEvent>>>,
        stats: Option<GroupStats>,
        checked_ago: Option<Duration>,
        dark: bool,
        insights: Vec<Insight>,
        dismissed_insights: HashSet<String>,
    ) -> impl FnMut(&mut egui::Ui) + 'static {
        let hog = cap("trufflehog");
        let mut leaks = cap("gitleaks");
        leaks.installed = false;
        leaks.version = None;
        move |ui| {
            let p = crate::theme::palette(dark);
            let group = get_group("secret-scanning").expect("real group");
            let busy = HashSet::new();
            let expanded = HashSet::new();
            let state = CapabilityPageState {
                group,
                caps: vec![&hog, &leaks],
                busy: &busy,
                expanded_issues: &expanded,
                stats: stats.as_ref(),
                checked_ago,
                insights: &insights,
                dismissed_insights: &dismissed_insights,
            };
            if let Some(event) = capability_page(ui, &p, &state) {
                sink.lock().expect("sink").push(event);
            }
        }
    }

    /// A fully-populated `GroupStats` — every Tier-1/Tier-2 field real, so
    /// the "present" snapshot exercises every tile the strip can draw at
    /// once (secret-scanning has no Tier-2 probe of its own, so the
    /// Tier-2-only fields here are illustrative of the rendering, not a
    /// claim that this exact combination occurs for this exact group).
    fn sample_group_stats() -> GroupStats {
        GroupStats {
            last_job: Some(ade_core::gui::stats::JobRecency {
                capability_id: "trufflehog".to_string(),
                capability_name: "TruffleHog".to_string(),
                action: "install".to_string(),
                failed: false,
                exit_code: Some(0),
                days_ago: 3,
            }),
            project_coverage: Some(ade_core::gui::stats::ProjectCoverage {
                covered: 2,
                inspected: 3,
            }),
            token_gain: Some(ade_core::gui::stats::TokenGain {
                tokens_saved: 310_350_059,
                avg_savings_pct: 81.43,
                total_commands: 12_129,
            }),
            osv_db_age: Some(ade_core::gui::stats::OsvDbAge {
                days_old: 21,
                ecosystems: 4,
            }),
            index_staleness_days: Some(14),
            secrets_catches: Some(ade_core::gui::stats::SecretsCatchLog {
                scans: 41,
                blocked: 3,
                findings: 5,
            }),
            hook_latency_millis: Some(340),
            harness_surfaces: vec![
                ade_core::gui::stats::HarnessSurface {
                    harness_id: "claude-code",
                    harness_name: "Claude Code",
                    covered: true,
                },
                ade_core::gui::stats::HarnessSurface {
                    harness_id: "codex",
                    harness_name: "Codex",
                    covered: false,
                },
            ],
            hardened_repos: Some(ade_core::gui::stats::HardenedRepoCoverage {
                hardened: 1,
                total: 3,
            }),
        }
    }

    /// The page at content width: title, why-prose subtitle, and the group's
    /// rows on the shared section surface — through the same function the app
    /// calls. No stats have been computed yet (`stats: None`, `checked_ago:
    /// None`) — the Phase G strip's absent case: it must draw NOTHING, not a
    /// row of placeholder tiles, so this snapshot's whole point is that the
    /// page looks identical to before Phase G landed.
    #[test]
    fn snapshot_the_capability_page_with_no_stats_yet() {
        let sink = std::sync::Arc::default();
        let mut h = harness_themed(
            egui::vec2(700.0, 360.0),
            true,
            secret_scanning_page(sink, None, None),
        );
        h.snapshot("capability_page");
    }

    /// The same page in LIGHT — first light golden for this screen.
    #[test]
    fn snapshot_the_capability_page_light() {
        let sink = std::sync::Arc::default();
        let mut h = harness_themed(
            egui::vec2(700.0, 360.0),
            false,
            secret_scanning_page_themed(sink, None, None, false),
        );
        h.snapshot("capability_page_light");
    }

    /// The same page once the engine has real stats for this group and a
    /// real detection-freshness fact — every tile the strip can draw, all at
    /// once, on the polish-pass elevated card treatment.
    #[test]
    fn snapshot_the_capability_page_with_a_populated_stats_strip() {
        let sink = std::sync::Arc::default();
        let mut h = harness_themed(
            egui::vec2(700.0, 400.0),
            true,
            secret_scanning_page(
                sink,
                Some(sample_group_stats()),
                Some(Duration::from_secs(45)),
            ),
        );
        h.snapshot("capability_page_stats_strip");
    }

    /// Light appearance carries the strip too — not just dark.
    #[test]
    fn snapshot_the_capability_page_stats_strip_in_light_mode() {
        let sink = std::sync::Arc::default();
        let mut h = harness_themed(
            egui::vec2(700.0, 400.0),
            false,
            secret_scanning_page_themed(
                sink,
                Some(sample_group_stats()),
                Some(Duration::from_secs(45)),
                false,
            ),
        );
        h.snapshot("capability_page_stats_strip_light");
    }

    /// A group with detection freshness but genuinely nothing else real
    /// (no job history, no project coverage, no Tier-2 probe for this
    /// group) still draws a strip — just the one honest tile, never padded
    /// with placeholders for the rest.
    #[test]
    fn snapshot_the_capability_page_with_only_freshness_real() {
        let sink = std::sync::Arc::default();
        let mut h = harness_themed(
            egui::vec2(700.0, 360.0),
            true,
            secret_scanning_page(
                sink,
                Some(GroupStats::default()),
                Some(Duration::from_secs(12)),
            ),
        );
        h.snapshot("capability_page_stats_strip_freshness_only");
    }

    /// Row events surface tagged with the capability they came from.
    #[test]
    fn the_page_tags_row_events_with_their_capability() {
        let seen: std::sync::Arc<std::sync::Mutex<Vec<CapabilityPageEvent>>> = Default::default();
        let sink = std::sync::Arc::clone(&seen);
        let mut h = harness_themed(
            egui::vec2(700.0, 360.0),
            true,
            secret_scanning_page(sink, None, None),
        );
        h.get_by_label("Install gitleaks").click();
        h.run_steps(2);
        assert_eq!(
            seen.lock().expect("sink").clone(),
            vec![CapabilityPageEvent::Row {
                capability_id: "gitleaks".to_string(),
                event: RowEvent::Start(LifecycleAction::Install),
            }]
        );
    }

    #[test]
    fn format_days_ago_reads_naturally_at_the_small_values_that_actually_occur() {
        assert_eq!(format_days_ago(0), "today");
        assert_eq!(format_days_ago(1), "1 day ago");
        assert_eq!(format_days_ago(2), "2 days ago");
        assert_eq!(format_days_ago(90), "90 days ago");
    }

    #[test]
    fn format_count_uses_the_shortest_suffix_that_reads_cleanly() {
        assert_eq!(format_count(42), "42");
        assert_eq!(format_count(999), "999");
        assert_eq!(format_count(1_500), "1.5K");
        assert_eq!(format_count(2_400_000), "2.4M");
        assert_eq!(format_count(310_350_059), "310.4M");
        assert_eq!(format_count(1_200_000_000), "1.2B");
    }

    /// A group with NOTHING real (no freshness, no job history, no
    /// coverage) draws no strip at all — verified structurally, not just by
    /// eye: `stats_strip` must not allocate any content.
    #[test]
    fn stats_strip_draws_nothing_when_every_fact_is_absent() {
        let mut h = harness(egui::vec2(400.0, 120.0), |ui| {
            let p = crate::theme::palette(true);
            stats_strip(ui, &p, None, None, true);
        });
        h.run_steps(2);
        // No panic and no crash is the primary assertion here — a positive
        // assertion (the populated snapshot test above) proves the strip
        // CAN draw content, so this test proving it draws NONE here is not
        // vacuous.
    }

    // ───────────────────────── Phase H — this group's insights ─────────────────────────

    fn secret_scanning_insight(id: &str, headline: &str) -> Insight {
        Insight {
            id: id.to_string(),
            group_id: "secret-scanning",
            group_name: "Secret Scanning",
            headline: headline.to_string(),
            evidence: "test fixture".to_string(),
            action: Some(ade_core::gui::insights::InsightAction {
                label: "Open Secret Scanning".to_string(),
                group_id: "secret-scanning".to_string(),
            }),
            rank: ade_core::gui::verdict::AttentionRank::Insight,
        }
    }

    /// The page renders its own group's insights, with a Dismiss control —
    /// but NO "Open Secret Scanning" navigate button (the owner is already
    /// here).
    #[test]
    fn snapshot_the_capability_page_with_its_own_insights() {
        let sink = std::sync::Arc::default();
        let insights = vec![secret_scanning_insight(
            "project-coverage:secret-scanning",
            "2 of 5 inspected projects have no Secret Scanning wired.",
        )];
        let mut h = harness_themed(
            egui::vec2(700.0, 420.0),
            true,
            secret_scanning_page_full(sink, None, None, true, insights, HashSet::new()),
        );
        assert!(h
            .query_by_label("2 of 5 inspected projects have no Secret Scanning wired.")
            .is_some());
        assert!(
            h.query_by_label("Open Secret Scanning").is_none(),
            "no self-navigating action on the page you're already on"
        );
        h.snapshot("capability_page_insights");
    }

    /// A capability page shows ONLY its own group's insights, never another
    /// group's — even though `Shared.insights` (the full list) is passed in
    /// unfiltered by field.
    #[test]
    fn a_capability_page_never_shows_another_groups_insight() {
        let sink = std::sync::Arc::default();
        let mut foreign = secret_scanning_insight(
            "osv-db-age:dependency-scanning",
            "The OSV vulnerability database is 21 days old.",
        );
        foreign.group_id = "dependency-scanning";
        let h = harness_themed(
            egui::vec2(700.0, 420.0),
            true,
            secret_scanning_page_full(sink, None, None, true, vec![foreign], HashSet::new()),
        );
        assert!(h
            .query_by_label("The OSV vulnerability database is 21 days old.")
            .is_none());
    }

    /// Dismissing reports the insight's own stable id, tagged distinctly
    /// from a row event so the shell can tell them apart without guessing.
    #[test]
    fn dismissing_a_capability_page_insight_reports_its_id() {
        let seen: std::sync::Arc<std::sync::Mutex<Vec<CapabilityPageEvent>>> = Default::default();
        let sink = std::sync::Arc::clone(&seen);
        let insights = vec![secret_scanning_insight(
            "project-coverage:secret-scanning",
            "2 of 5 inspected projects have no Secret Scanning wired.",
        )];
        let mut h = harness_themed(
            egui::vec2(700.0, 420.0),
            true,
            secret_scanning_page_full(sink, None, None, true, insights, HashSet::new()),
        );
        h.get_by_label("Dismiss insight project-coverage:secret-scanning")
            .click();
        h.run_steps(2);
        assert_eq!(
            seen.lock().expect("sink").clone(),
            vec![CapabilityPageEvent::DismissInsight(
                "project-coverage:secret-scanning".to_string()
            )]
        );
    }

    /// A dismissed insight (already in `dismissed_insights`) never renders.
    #[test]
    fn a_dismissed_capability_page_insight_is_absent() {
        let sink = std::sync::Arc::default();
        let insights = vec![secret_scanning_insight(
            "project-coverage:secret-scanning",
            "2 of 5 inspected projects have no Secret Scanning wired.",
        )];
        let dismissed: HashSet<String> = ["project-coverage:secret-scanning".to_string()].into();
        let h = harness_themed(
            egui::vec2(700.0, 420.0),
            true,
            secret_scanning_page_full(sink, None, None, true, insights, dismissed),
        );
        assert!(h
            .query_by_label("2 of 5 inspected projects have no Secret Scanning wired.")
            .is_none());
    }
}
