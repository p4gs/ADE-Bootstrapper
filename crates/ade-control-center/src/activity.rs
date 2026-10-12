//! The Activity scope: one card per job, newest history the engine kept.
//! Pure view — it makes zero engine calls; expanding a log is the only thing
//! it reports.

use crate::app::{chip, status_dot_at, Dot};
use crate::overview::plural;
use crate::theme;
use ade_core::gui::jobs::{parse_utc_seconds, Job, JobStatus};
use egui::{Align, CornerRadius, Layout, RichText};
use std::collections::HashSet;
use std::time::Duration;

/// How much of a job's log the tail panel shows. The full log (bounded at
/// `MAX_LOG_LINES` = 400 by the runner) stays in `Job.log`; the panel shows
/// the newest slice and sticks to the bottom, so a running job reads as a
/// live stream under the ~1.2s busy repoll.
const LOG_TAIL_LINES: usize = 200;

/// The newest `LOG_TAIL_LINES` of a log — pure, so the window is testable
/// without a UI.
fn log_tail(log: &[String]) -> &[String] {
    &log[log.len().saturating_sub(LOG_TAIL_LINES)..]
}

/// How many commands a job has executed so far: the runner logs each one as a
/// `$ `-prefixed line before its output.
fn command_count(log: &[String]) -> usize {
    log.iter().filter(|line| line.starts_with("$ ")).count()
}

/// "5s ago" / "2m ago" / "3h ago" for a job's start time, in the same coarse
/// bucketing style as `nav.rs`'s `freshness_caption` — just-now / Ns / Nm /
/// Nh. Past 24h a relative count stops being useful, so it falls back to a
/// clock time instead of growing into "37h ago". That fallback is UTC, not
/// local: the app has no timezone source without a new dependency (no
/// chrono/time crate anywhere in this workspace — the same constraint
/// `now_utc_seconds` was written under), and a value merely LABELED "local"
/// while actually being UTC would be worse than an honest one.
fn job_age_caption(age: Duration, epoch_seconds: u64) -> String {
    let secs = age.as_secs();
    if secs < 5 {
        "just now".to_string()
    } else if secs < 60 {
        format!("{secs}s ago")
    } else if secs < 3600 {
        format!("{}m ago", secs / 60)
    } else if secs < 86_400 {
        format!("{}h ago", secs / 3600)
    } else {
        let of_day = epoch_seconds % 86_400;
        format!("{:02}:{:02} UTC", of_day / 3600, (of_day % 3600) / 60)
    }
}

/// The job-card timestamp, humanized. `started_at` is the RFC3339-ish UTC
/// string the runner writes (`JobRunner::now`); anything that doesn't parse
/// as that exact shape (an old build's format, hand-edited state) is shown
/// verbatim rather than hidden behind a parse failure — a raw string a human
/// can still read beats a silently blank timestamp.
fn started_at_caption(started_at: &str) -> String {
    let Some(epoch_seconds) = parse_utc_seconds(started_at) else {
        return started_at.to_string();
    };
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let age = Duration::from_secs(now.saturating_sub(epoch_seconds));
    job_age_caption(age, epoch_seconds)
}

/// What the owner did in the Activity list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ActivityEvent {
    /// Expand or collapse this job's log.
    ToggleLog(String),
}

pub(crate) fn activity_view(
    ui: &mut egui::Ui,
    p: &theme::Palette,
    jobs: &[Job],
    expanded_jobs: &HashSet<String>,
) -> Option<ActivityEvent> {
    if jobs.is_empty() {
        ui.label(
            RichText::new("No jobs yet — install/update/uninstall actions appear here.")
                .color(p.muted),
        );
        return None;
    }
    let mut event = None;
    for job in jobs {
        if let Some(found) = job_card(ui, p, job, expanded_jobs) {
            event = Some(found);
        }
        ui.add_space(theme::Space::S8.px());
    }
    event
}

fn job_card(
    ui: &mut egui::Ui,
    p: &theme::Palette,
    job: &Job,
    expanded_jobs: &HashSet<String>,
) -> Option<ActivityEvent> {
    let mut event = None;
    // The system card (ISC-311): rim light + S12 breathing room. Only a
    // FAILED job carries the error tint — ok stays silent (no green wall)
    // and running stays neutral (a transient state shouldn't flash an
    // amber surface at the reader on every poll).
    let dark_mode = ui.visuals().dark_mode;
    let tint = matches!(job.status, JobStatus::Error).then_some(p.tint_err);
    theme::elevated_card(ui, p, dark_mode, tint, theme::Space::S12, |ui| {
        ui.horizontal(|ui| {
            let dot = match job.status {
                JobStatus::Ok => Dot::Ok,
                JobStatus::Error => Dot::Err,
                JobStatus::Running => Dot::Warn,
            };
            // Optical anchor, same as the Overview coverage rows: a mark at
            // galley center reads ~1.5px high of the text's ink mass
            // (measured -1.0..-1.5px on this very golden before this fix).
            // Half the body-size descent drops it onto the ink.
            let (slot, _) = ui.allocate_exact_size(egui::vec2(12.0, 12.0), egui::Sense::hover());
            let mark = egui::Rect::from_center_size(
                egui::pos2(slot.center().x, slot.center().y + 1.5),
                egui::Vec2::splat(12.0),
            );
            status_dot_at(ui, mark, dot, p);
            ui.label(RichText::new(&job.capability_id).font(theme::medium(theme::SIZE_BODY)));
            ui.label(
                RichText::new(&job.action)
                    .color(p.muted)
                    .size(theme::SIZE_CAPTION),
            );
            match job.status {
                JobStatus::Running => {
                    ui.add(egui::Spinner::new().size(theme::SIZE_BODY).color(p.muted));
                    ui.label(
                        RichText::new("running")
                            .color(p.warn)
                            .size(theme::SIZE_CAPTION),
                    );
                }
                JobStatus::Ok => chip(ui, "ok", p.ok),
                JobStatus::Error => chip(
                    ui,
                    &format!("error (exit {})", job.exit_code.unwrap_or(-1)),
                    p.err,
                ),
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.label(
                    RichText::new(started_at_caption(&job.started_at))
                        .color(p.faint_text)
                        .size(theme::SIZE_CAPTION),
                );
            });
        });
        let key = format!("job-{}", job.id);
        let expanded = expanded_jobs.contains(&key);
        // Words, not glyphs: the "+"/"−" prefixes were text characters
        // doing icon work.
        let toggle_text = format!(
            "{} log ({} lines)",
            if expanded { "Hide" } else { "Show" },
            job.log.len()
        );
        let response = ui.selectable_label(
            false,
            RichText::new(toggle_text)
                .color(p.muted)
                .size(theme::SIZE_CAPTION),
        );
        let ax = format!("Log {}", job.id);
        response.clone().widget_info(move || {
            egui::WidgetInfo::labeled(egui::WidgetType::Button, true, ax.clone())
        });
        if response.clicked() {
            event = Some(ActivityEvent::ToggleLog(job.id.clone()));
        }
        if expanded {
            // The command log, verbatim, on an ink surface: every command
            // a job ran is one expand away — trust through transparency.
            // Deliberately NOT `elevated_card` (the ISC-311 exception
            // rule): this is a SUNKEN inset panel inside a card — the
            // code-panel idiom — not an elevated surface; a rim light
            // and shadow here would invert its depth.
            egui::Frame::new()
                .fill(p.inset)
                .corner_radius(CornerRadius::same(theme::RADIUS_CONTAINER))
                .inner_margin(theme::margin(theme::Space::S8, theme::Space::S8))
                .show(ui, |ui| {
                    // The panel is an inset surface spanning the card, not
                    // a sticker hugging the longest log line.
                    ui.set_width(ui.available_width());
                    match job.status {
                        JobStatus::Running => {
                            // Still streaming: the header fact that exists
                            // is "in flight", not a final count.
                            ui.add(
                                egui::Spinner::new()
                                    .size(theme::SIZE_CAPTION)
                                    .color(p.muted),
                            );
                        }
                        _ => {
                            let commands = command_count(&job.log);
                            let mut header =
                                format!("{commands} {}", plural(commands, "command", "commands"));
                            // A job with no recorded exit code states no
                            // exit code — absent, never invented.
                            if let Some(code) = job.exit_code {
                                header.push_str(&format!(" · exit {code}"));
                            }
                            ui.label(
                                RichText::new(header)
                                    .color(p.muted)
                                    .size(theme::SIZE_CAPTION),
                            );
                        }
                    }
                    egui::ScrollArea::vertical()
                        .id_salt(&key)
                        .max_height(220.0)
                        .stick_to_bottom(true)
                        .show(ui, |ui| {
                            ui.label(
                                RichText::new(log_tail(&job.log).join("\n"))
                                    .monospace()
                                    .size(theme::SIZE_CAPTION),
                            );
                        });
                });
        }
    });
    event
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testkit::harness_themed;
    use egui_kittest::kittest::Queryable;

    fn job(id: &str, capability: &str, action: &str, status: JobStatus) -> Job {
        Job {
            id: id.to_string(),
            capability_id: capability.to_string(),
            action: action.to_string(),
            status,
            // Fixed and safely >24h in the past relative to any real test
            // run — `started_at_caption` falls back to a UTC clock time past
            // 24h, so this fixture renders the SAME caption ("09:30 UTC")
            // regardless of when `cargo test` actually executes. A
            // near-"now" fixture would drift between the Ns/Nm/Nh buckets
            // run to run and make the snapshot flaky.
            started_at: "2026-01-15T09:30:00Z".to_string(),
            finished_at: (status != JobStatus::Running).then(|| "2026-07-31T18:02:40Z".to_string()),
            exit_code: match status {
                JobStatus::Ok => Some(0),
                JobStatus::Error => Some(1),
                JobStatus::Running => None,
            },
            log: vec![
                "$ brew install trufflehog".to_string(),
                "==> Downloading …".to_string(),
            ],
        }
    }

    fn paint(
        dark: bool,
        expanded: HashSet<String>,
        sink: std::sync::Arc<std::sync::Mutex<Vec<ActivityEvent>>>,
    ) -> impl FnMut(&mut egui::Ui) + 'static {
        let jobs = vec![
            job("job-3", "trufflehog", "install", JobStatus::Running),
            job("job-2", "gitleaks", "update", JobStatus::Ok),
            job("job-1", "nono", "install", JobStatus::Error),
        ];
        move |ui| {
            let p = crate::theme::palette(dark);
            if let Some(event) = activity_view(ui, &p, &jobs, &expanded) {
                sink.lock().expect("sink").push(event);
            }
        }
    }

    /// Running, succeeded and failed cards side by side, one log expanded —
    /// through the same function the app calls.
    #[test]
    fn snapshot_the_activity_list_with_an_expanded_log() {
        let sink = std::sync::Arc::default();
        let expanded: HashSet<String> = ["job-job-2".to_string()].into();
        let mut h = harness_themed(egui::vec2(700.0, 420.0), true, paint(true, expanded, sink));
        h.snapshot("activity");
    }

    /// The same list in LIGHT — first light golden for this screen; the
    /// error tint, chips, and inset log all resolve through the token layer.
    #[test]
    fn snapshot_the_activity_list_light() {
        let sink = std::sync::Arc::default();
        let expanded: HashSet<String> = ["job-job-2".to_string()].into();
        let mut h = harness_themed(
            egui::vec2(700.0, 420.0),
            false,
            paint(false, expanded, sink),
        );
        h.snapshot("activity_light");
    }

    /// The log tail as a trust surface: a RUNNING job streams under a spinner
    /// with no final facts claimed, and a FAILED one states its command count
    /// and exit code over the verbatim output.
    #[test]
    fn snapshot_the_log_tail_for_a_running_and_a_failed_job() {
        let mut running = job("job-9", "trufflehog", "install", JobStatus::Running);
        running.log = vec![
            "$ brew install trufflehog".to_string(),
            "==> Downloading https://ghcr.io/v2/homebrew/core/trufflehog/manifests/3.96.0"
                .to_string(),
            "==> Fetching trufflehog".to_string(),
            "==> Downloading https://ghcr.io/v2/homebrew/core/trufflehog/blobs/sha256:8c2b"
                .to_string(),
        ];
        let mut failed = job("job-8", "openwiki", "install", JobStatus::Error);
        failed.log = vec![
            "$ npm install -g openwiki".to_string(),
            "npm ERR! code E403".to_string(),
            "npm ERR! 403 403 Forbidden - GET https://registry.npmjs.org/openwiki".to_string(),
            "npm ERR! 403 In most cases, you or one of your dependencies are".to_string(),
            "npm ERR! 403 requesting a package version that is forbidden.".to_string(),
        ];
        let jobs = vec![running, failed];
        let expanded: HashSet<String> = ["job-job-9".to_string(), "job-job-8".to_string()].into();
        let sink: std::sync::Arc<std::sync::Mutex<Vec<ActivityEvent>>> = Default::default();
        let mut h = harness_themed(egui::vec2(700.0, 560.0), true, move |ui| {
            let p = crate::theme::palette(true);
            if let Some(event) = activity_view(ui, &p, &jobs, &expanded) {
                sink.lock().expect("sink").push(event);
            }
        });
        // The finished job's header states its real facts…
        assert!(h.query_by_label("1 command · exit 1").is_some());
        // …and the running job claims none (a finished-looking header for it
        // would read "1 command", exit code being honestly absent).
        assert!(h.query_by_label("1 command").is_none());
        h.snapshot("activity_log_tail");
    }

    /// The tail window is pure and bounded: the newest 200 lines, everything
    /// when shorter, and the command count reads only the runner's own
    /// `$ `-prefixed lines.
    #[test]
    fn the_tail_window_and_command_count_are_exact() {
        let log: Vec<String> = (0..350).map(|index| format!("line {index}")).collect();
        let tail = log_tail(&log);
        assert_eq!(tail.len(), LOG_TAIL_LINES);
        assert_eq!(tail.first().map(String::as_str), Some("line 150"));
        assert_eq!(tail.last().map(String::as_str), Some("line 349"));
        let short = vec!["only".to_string()];
        assert_eq!(log_tail(&short), short.as_slice());
        assert!(log_tail(&[]).is_empty());
        assert_eq!(
            command_count(&[
                "$ brew install nono".to_string(),
                "==> output mentioning $ mid-line".to_string(),
                "$ brew link nono".to_string(),
            ]),
            2
        );
        assert_eq!(command_count(&[]), 0);
    }

    /// The bucketing is coarse and honest, same discipline as `nav.rs`'s
    /// `freshness_caption` — and past 24h it falls back to a UTC clock time
    /// instead of growing into "37h ago".
    #[test]
    fn job_age_caption_buckets_coarsely_and_falls_back_to_a_clock_time() {
        assert_eq!(job_age_caption(Duration::from_secs(0), 0), "just now");
        assert_eq!(job_age_caption(Duration::from_secs(4), 0), "just now");
        assert_eq!(job_age_caption(Duration::from_secs(5), 0), "5s ago");
        assert_eq!(job_age_caption(Duration::from_secs(59), 0), "59s ago");
        assert_eq!(job_age_caption(Duration::from_secs(125), 0), "2m ago");
        assert_eq!(
            job_age_caption(Duration::from_secs(2 * 3600 + 60), 0),
            "2h ago"
        );
        // Past 24h: the RELATIVE age no longer drives the text at all — only
        // the ABSOLUTE time-of-day the job started does (9:30 in the day).
        let started_at_930 = 9 * 3600 + 30 * 60;
        assert_eq!(
            job_age_caption(Duration::from_secs(90_000), started_at_930),
            "09:30 UTC"
        );
        assert_eq!(
            job_age_caption(Duration::from_secs(10 * 86_400), started_at_930),
            "09:30 UTC",
            "a job ten days old reads the same as one thirty hours old — both just state the clock time"
        );
    }

    /// The card's real entry point: a value this module wrote round-trips to
    /// its humanized form, and a value it did NOT write (old format,
    /// hand-edited state) shows verbatim rather than vanishing behind a
    /// silent parse failure.
    #[test]
    fn started_at_caption_humanizes_real_timestamps_and_shows_unparseable_ones_verbatim() {
        assert_eq!(started_at_caption("2026-01-15T09:30:00Z"), "09:30 UTC");
        assert_eq!(
            started_at_caption("not a timestamp"),
            "not a timestamp",
            "unparseable input is shown verbatim, never hidden"
        );
    }

    #[test]
    fn expanding_a_log_is_reported_not_applied() {
        let seen: std::sync::Arc<std::sync::Mutex<Vec<ActivityEvent>>> = Default::default();
        let sink = std::sync::Arc::clone(&seen);
        let mut h = harness_themed(
            egui::vec2(700.0, 420.0),
            true,
            paint(true, HashSet::new(), sink),
        );
        assert!(h.query_by_label("Log job-2").is_some());
        h.get_by_label("Log job-2").click();
        h.run_steps(2);
        assert_eq!(
            seen.lock().expect("sink").clone(),
            vec![ActivityEvent::ToggleLog("job-2".to_string())]
        );
    }
}
