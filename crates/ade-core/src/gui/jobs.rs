//! In-process job runner for capability lifecycle actions (ISC-171).
//! One job per capability at a time; every subprocess is an argv array via the
//! injected ExecFn; full stdout/stderr captured. Job records persist to
//! `$ADE_HOME/jobs.json` so the tray reflects activity without any IPC.

use crate::fsutil::{read_if_exists, stable_stringify, write_ensured};
use crate::gui::inventory::{LastJobSummary, LifecycleAction};
use crate::gui::state::JOBS_FILE;
use crate::types::{ExecFn, ExecOpts};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

pub const MAX_JOBS_KEPT: usize = 50;
pub const MAX_LOG_LINES: usize = 400;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum JobStatus {
    Running,
    Ok,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Job {
    pub id: String,
    pub capability_id: String,
    pub action: String,
    pub status: JobStatus,
    pub started_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub finished_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
    pub log: Vec<String>,
}

struct RunnerInner {
    jobs: Vec<Job>,
    active: BTreeSet<String>,
    counter: u64,
}

/// Thread-safe job runner. Clone freely; state is shared.
#[derive(Clone)]
pub struct JobRunner {
    inner: Arc<Mutex<RunnerInner>>,
    exec: ExecFn,
    /// When set, finished/running job snapshots persist here (jobs.json).
    persist_dir: Option<PathBuf>,
}

pub enum StartOutcome {
    Started(Job),
    Busy,
}

impl JobRunner {
    /// Build a runner, RESUMING any history already persisted in `persist_dir`.
    ///
    /// Starting empty looked harmless and was not: `persist` writes only what is
    /// in memory, and the id counter restarted at 1 — so the first action after
    /// an app restart both re-used `job-1` and overwrote every previously
    /// recorded job. The Activity view also came back blank while the file still
    /// held the history. Resuming fixes all three.
    pub fn new(exec: ExecFn, persist_dir: Option<PathBuf>) -> Self {
        let (jobs, counter) = match &persist_dir {
            Some(dir) => resume_jobs(dir),
            None => (Vec::new(), 0),
        };
        JobRunner {
            inner: Arc::new(Mutex::new(RunnerInner {
                jobs,
                // Deliberately empty: a job recorded as running belonged to a
                // process that is gone. Re-locking its capability here would
                // wedge it forever, so `resume_jobs` retires those instead.
                active: BTreeSet::new(),
                counter,
            })),
            exec,
            persist_dir,
        }
    }

    fn now() -> String {
        let duration = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default();
        now_utc_seconds(duration.as_secs())
    }

    pub fn list(&self) -> Vec<Job> {
        let inner = self.inner.lock().expect("job lock");
        let mut jobs = inner.jobs.clone();
        jobs.reverse();
        jobs
    }

    pub fn get(&self, id: &str) -> Option<Job> {
        self.inner
            .lock()
            .expect("job lock")
            .jobs
            .iter()
            .find(|job| job.id == id)
            .cloned()
    }

    pub fn running_count(&self) -> usize {
        self.inner.lock().expect("job lock").active.len()
    }

    pub fn is_busy(&self, capability_id: &str) -> bool {
        self.inner
            .lock()
            .expect("job lock")
            .active
            .contains(capability_id)
    }

    /// Most recent FINISHED job per capability, as inventory summaries.
    pub fn last_finished(&self) -> BTreeMap<String, LastJobSummary> {
        let inner = self.inner.lock().expect("job lock");
        let mut map = BTreeMap::new();
        // `inner.jobs` is oldest-first, so a later insert overwrites an older one.
        for job in &inner.jobs {
            if job.status != JobStatus::Running {
                map.insert(job.capability_id.clone(), summarize(job));
            }
        }
        map
    }

    fn persist(&self) {
        let Some(dir) = &self.persist_dir else { return };
        let jobs = self.list();
        let running = jobs
            .iter()
            .filter(|job| job.status == JobStatus::Running)
            .count();
        let value = serde_json::json!({
            "schemaVersion": 1,
            "running": running,
            "jobs": serde_json::to_value(&jobs).unwrap_or(serde_json::Value::Null),
        });
        let _ = write_ensured(&dir.join(JOBS_FILE), &stable_stringify(&value));
    }

    /// Start a job executing argv sequences in order (stops at first failure).
    /// Runs on a background thread; job state is observable via list()/get().
    pub fn start(
        &self,
        capability_id: &str,
        action: LifecycleAction,
        argvs: Vec<Vec<String>>,
    ) -> StartOutcome {
        let job = {
            let mut inner = self.inner.lock().expect("job lock");
            if inner.active.contains(capability_id) {
                return StartOutcome::Busy;
            }
            inner.counter += 1;
            let job = Job {
                id: format!("job-{}", inner.counter),
                capability_id: capability_id.to_string(),
                action: action.as_str().to_string(),
                status: JobStatus::Running,
                started_at: Self::now(),
                finished_at: None,
                exit_code: None,
                log: Vec::new(),
            };
            inner.jobs.push(job.clone());
            if inner.jobs.len() > MAX_JOBS_KEPT {
                let excess = inner.jobs.len() - MAX_JOBS_KEPT;
                inner.jobs.drain(0..excess);
            }
            inner.active.insert(capability_id.to_string());
            job
        };
        self.persist();
        let runner = self.clone();
        let job_id = job.id.clone();
        let capability = capability_id.to_string();
        std::thread::spawn(move || {
            runner.run_job(&job_id, &capability, argvs);
        });
        StartOutcome::Started(job)
    }

    fn append_log(&self, job_id: &str, text: &str) {
        let mut inner = self.inner.lock().expect("job lock");
        if let Some(job) = inner.jobs.iter_mut().find(|job| job.id == job_id) {
            for line in text.split('\n') {
                let trimmed = line.trim_end();
                if !trimmed.is_empty() {
                    job.log.push(trimmed.to_string());
                }
            }
            if job.log.len() > MAX_LOG_LINES {
                let excess = job.log.len() - MAX_LOG_LINES;
                job.log.drain(0..excess);
            }
        }
    }

    fn finish(&self, job_id: &str, capability: &str, status: JobStatus, code: Option<i32>) {
        {
            let mut inner = self.inner.lock().expect("job lock");
            if let Some(job) = inner.jobs.iter_mut().find(|job| job.id == job_id) {
                job.status = status;
                job.finished_at = Some(Self::now());
                if code.is_some() {
                    job.exit_code = code;
                }
            }
        }
        // Persist BEFORE releasing the capability lock: `settle()` returns the
        // moment `active` empties, so the final snapshot must already be on disk.
        self.persist();
        self.inner
            .lock()
            .expect("job lock")
            .active
            .remove(capability);
    }

    fn run_job(&self, job_id: &str, capability: &str, argvs: Vec<Vec<String>>) {
        for argv in &argvs {
            self.append_log(job_id, &format!("$ {}", argv.join(" ")));
            let result = (self.exec)(argv, &ExecOpts::default());
            self.append_log(job_id, &result.stdout);
            self.append_log(job_id, &result.stderr);
            {
                let mut inner = self.inner.lock().expect("job lock");
                if let Some(job) = inner.jobs.iter_mut().find(|job| job.id == job_id) {
                    job.exit_code = Some(result.code);
                }
            }
            if result.code != 0 {
                self.finish(job_id, capability, JobStatus::Error, Some(result.code));
                return;
            }
            self.persist();
        }
        self.finish(job_id, capability, JobStatus::Ok, None);
    }

    /// Block until no jobs are running (test convenience).
    pub fn settle(&self) {
        while self.running_count() > 0 {
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    }
}

/// Load persisted jobs (oldest-first) and the id counter to continue from.
///
/// A job still marked `running` cannot be running any more — the process that
/// owned it exited. It is retired as an error saying exactly that, which is
/// honest (we do not know whether the underlying `brew install` finished) and
/// keeps the capability unlocked.
fn resume_jobs(dir: &std::path::Path) -> (Vec<Job>, u64) {
    let Some(text) = read_if_exists(&dir.join(JOBS_FILE)) else {
        return (Vec::new(), 0);
    };
    let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) else {
        return (Vec::new(), 0);
    };
    let Some(array) = value.get("jobs").and_then(|value| value.as_array()) else {
        return (Vec::new(), 0);
    };
    let mut jobs: Vec<Job> = array
        .iter()
        .filter_map(|entry| serde_json::from_value::<Job>(entry.clone()).ok())
        .collect();
    // Persisted newest-first; in memory we keep oldest-first.
    jobs.reverse();
    let mut counter = 0;
    for job in &mut jobs {
        if let Some(number) = job
            .id
            .strip_prefix("job-")
            .and_then(|rest| rest.parse::<u64>().ok())
        {
            counter = counter.max(number);
        }
        if job.status == JobStatus::Running {
            job.status = JobStatus::Error;
            job.log
                .push("interrupted — the app exited while this job was running".to_string());
        }
    }
    if jobs.len() > MAX_JOBS_KEPT {
        let excess = jobs.len() - MAX_JOBS_KEPT;
        jobs.drain(0..excess);
    }
    (jobs, counter)
}

/// One capability's last-job summary, derived identically wherever it is built.
fn summarize(job: &Job) -> LastJobSummary {
    LastJobSummary {
        action: job.action.clone(),
        failed: job.status == JobStatus::Error,
        exit_code: job.exit_code,
        log_tail: job
            .log
            .iter()
            .rev()
            .take(3)
            .rev()
            .cloned()
            .collect::<Vec<_>>()
            .join(" · "),
    }
}

/// Most recent FINISHED job per capability, read from the persisted jobs.json.
///
/// The menu-bar helper owns no JobRunner, so without this it would report a
/// capability whose last install/update FAILED as merely "not installed" while
/// the Control Center showed an error — the two surfaces disagreeing about the
/// same machine. Persisted jobs are newest-first (see `list`), so the FIRST
/// finished entry per capability is the current one.
pub fn read_last_finished(dir: &std::path::Path) -> BTreeMap<String, LastJobSummary> {
    let mut map = BTreeMap::new();
    let Some(text) = read_if_exists(&dir.join(JOBS_FILE)) else {
        return map;
    };
    let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) else {
        return map;
    };
    let Some(array) = value.get("jobs").and_then(|value| value.as_array()) else {
        return map;
    };
    for entry in array {
        let Ok(job) = serde_json::from_value::<Job>(entry.clone()) else {
            continue;
        };
        if job.status == JobStatus::Running {
            continue;
        }
        map.entry(job.capability_id.clone())
            .or_insert_with(|| summarize(&job));
    }
    map
}

/// Read the persisted jobs.json summary (tray side): (running, total).
pub fn read_jobs_summary(dir: &std::path::Path) -> Option<(u64, u64)> {
    let text = read_if_exists(&dir.join(JOBS_FILE))?;
    let value: serde_json::Value = serde_json::from_str(&text).ok()?;
    let running = value.get("running")?.as_u64()?;
    let total = value.get("jobs")?.as_array()?.len() as u64;
    Some((running, total))
}

/// RFC3339-ish UTC timestamp (second resolution) without a chrono dependency.
pub fn now_utc_seconds(seconds: u64) -> String {
    let days = seconds / 86_400;
    let (year, month, day) = civil_from_days(days as i64);
    let secs_of_day = seconds % 86_400;
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        year,
        month,
        day,
        secs_of_day / 3600,
        (secs_of_day % 3600) / 60,
        secs_of_day % 60
    )
}

/// Days-since-epoch → (y, m, d). Howard Hinnant's civil_from_days algorithm.
fn civil_from_days(days: i64) -> (i64, u64, u64) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let year = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    (if month <= 2 { year + 1 } else { year }, month, day)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::{fake_exec, make_temp_dir};
    use crate::types::ExecResult;
    use std::sync::mpsc;

    #[test]
    fn successful_job_logs_commands_and_output() {
        let runner = JobRunner::new(
            fake_exec(&[("brew install", (0, "installed!\n", ""))]),
            None,
        );
        let StartOutcome::Started(job) = runner.start(
            "pre-commit",
            LifecycleAction::Install,
            vec![vec!["brew".into(), "install".into(), "pre-commit".into()]],
        ) else {
            panic!("expected start");
        };
        runner.settle();
        let finished = runner.get(&job.id).unwrap();
        assert_eq!(finished.status, JobStatus::Ok);
        assert_eq!(finished.exit_code, Some(0));
        assert!(finished.finished_at.is_some());
        assert!(finished
            .log
            .contains(&"$ brew install pre-commit".to_string()));
        assert!(finished.log.contains(&"installed!".to_string()));
    }

    #[test]
    fn failing_step_stops_chain_and_marks_error() {
        let runner = JobRunner::new(
            fake_exec(&[
                ("brew uninstall", (1, "", "nope")),
                ("brew install", (0, "", "")),
            ]),
            None,
        );
        let StartOutcome::Started(job) = runner.start(
            "pre-commit",
            LifecycleAction::Reinstall,
            vec![
                vec!["brew".into(), "uninstall".into(), "pre-commit".into()],
                vec!["brew".into(), "install".into(), "pre-commit".into()],
            ],
        ) else {
            panic!("expected start");
        };
        runner.settle();
        let finished = runner.get(&job.id).unwrap();
        assert_eq!(finished.status, JobStatus::Error);
        assert_eq!(finished.exit_code, Some(1));
        assert!(finished.log.iter().any(|line| line.contains("nope")));
        // Second command never ran.
        assert!(!finished
            .log
            .iter()
            .any(|line| line.contains("$ brew install")));
    }

    #[test]
    fn per_capability_lock_and_cross_capability_freedom() {
        let (sender, receiver) = mpsc::channel::<()>();
        let receiver = std::sync::Arc::new(Mutex::new(receiver));
        let gated: ExecFn = {
            let receiver = receiver.clone();
            std::sync::Arc::new(move |_argv, _opts| {
                let _ = receiver.lock().expect("gate").recv();
                ExecResult {
                    code: 0,
                    stdout: String::new(),
                    stderr: String::new(),
                }
            })
        };
        let runner = JobRunner::new(gated, None);
        let StartOutcome::Started(_) = runner.start(
            "pre-commit",
            LifecycleAction::Install,
            vec![vec!["x".into()]],
        ) else {
            panic!("expected start");
        };
        assert!(matches!(
            runner.start(
                "pre-commit",
                LifecycleAction::Update,
                vec![vec!["y".into()]]
            ),
            StartOutcome::Busy
        ));
        assert!(runner.is_busy("pre-commit"));
        assert!(matches!(
            runner.start("gitleaks", LifecycleAction::Install, vec![vec!["z".into()]]),
            StartOutcome::Started(_)
        ));
        sender.send(()).unwrap();
        sender.send(()).unwrap();
        runner.settle();
        assert_eq!(runner.running_count(), 0);
    }

    #[test]
    fn last_finished_maps_latest_and_list_is_newest_first() {
        let runner = JobRunner::new(fake_exec(&[("brew", (0, "", ""))]), None);
        for (cap, action) in [
            ("a", LifecycleAction::Install),
            ("b", LifecycleAction::Install),
            ("a", LifecycleAction::Uninstall),
        ] {
            let StartOutcome::Started(_) = runner.start(cap, action, vec![vec!["brew".into()]])
            else {
                panic!("start");
            };
            runner.settle();
        }
        let last = runner.last_finished();
        assert_eq!(last.get("a").unwrap().action, "uninstall");
        assert!(!last.get("a").unwrap().failed);
        assert_eq!(runner.list()[0].action, "uninstall");
        assert!(runner.get("job-nope").is_none());
    }

    #[test]
    fn history_and_log_are_bounded_and_jobs_persist() {
        let dir = make_temp_dir("jobs");
        let long_output = "line\n".repeat(600);
        let rules = [("echo", (0i32, long_output.as_str(), ""))];
        let runner = JobRunner::new(fake_exec(&rules), Some(dir.clone()));
        for index in 0..55 {
            let StartOutcome::Started(_) = runner.start(
                &format!("cap-{index}"),
                LifecycleAction::Install,
                vec![vec!["echo".into(), index.to_string()]],
            ) else {
                panic!("start");
            };
            runner.settle();
        }
        assert!(runner.list().len() <= MAX_JOBS_KEPT);
        assert!(runner.list()[0].log.len() <= MAX_LOG_LINES);
        let (running, total) = read_jobs_summary(&dir).unwrap();
        assert_eq!(running, 0);
        assert!(total > 0);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn disk_last_finished_matches_in_memory_including_newest_wins() {
        let dir = make_temp_dir("jobs-last-finished");
        let runner = JobRunner::new(
            fake_exec(&[("boom", (1, "", "nope\n")), ("ok-now", (0, "fine\n", ""))]),
            Some(dir.clone()),
        );
        // Same capability twice: a FAILED job, then a later successful one.
        for argv in [vec!["boom".to_string()], vec!["ok-now".to_string()]] {
            let StartOutcome::Started(_) =
                runner.start("openwiki", LifecycleAction::Install, vec![argv])
            else {
                panic!("start");
            };
            runner.settle();
        }
        // A second capability whose only job failed — the tray must see it.
        let StartOutcome::Started(_) = runner.start(
            "ccc",
            LifecycleAction::Update,
            vec![vec!["boom".to_string()]],
        ) else {
            panic!("start");
        };
        runner.settle();

        let from_disk = read_last_finished(&dir);
        assert_eq!(from_disk, runner.last_finished());
        // Newest job wins: openwiki's later success must not be masked by the
        // earlier failure sitting further down the newest-first array.
        assert!(!from_disk["openwiki"].failed);
        assert!(from_disk["ccc"].failed);
        assert_eq!(from_disk["ccc"].exit_code, Some(1));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A restart must not lose history, re-use ids, or leave a capability locked.
    #[test]
    fn a_new_runner_resumes_persisted_history_instead_of_clobbering_it() {
        let dir = make_temp_dir("jobs-resume");
        let first = JobRunner::new(fake_exec(&[("ok", (0, "done\n", ""))]), Some(dir.clone()));
        for _ in 0..3 {
            let StartOutcome::Started(_) = first.start(
                "trufflehog",
                LifecycleAction::Install,
                vec![vec!["ok".into()]],
            ) else {
                panic!("start");
            };
            first.settle();
        }
        let before = first.list();
        assert_eq!(before.len(), 3);
        drop(first);

        // A fresh process opening the same home.
        let resumed = JobRunner::new(fake_exec(&[("ok", (0, "done\n", ""))]), Some(dir.clone()));
        assert_eq!(resumed.list(), before, "history must come back intact");
        assert_eq!(resumed.running_count(), 0);
        assert!(
            !resumed.is_busy("trufflehog"),
            "capability must not be locked"
        );

        let StartOutcome::Started(next) =
            resumed.start("gitleaks", LifecycleAction::Update, vec![vec!["ok".into()]])
        else {
            panic!("start");
        };
        assert_eq!(next.id, "job-4", "ids must continue, not restart");
        resumed.settle();
        assert_eq!(resumed.list().len(), 4, "the old jobs must still be there");

        // And the file agrees — the whole point.
        let reread = JobRunner::new(fake_exec(&[]), Some(dir.clone()));
        assert_eq!(reread.list().len(), 4);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A job left `running` by a killed process is retired, not resurrected.
    #[test]
    fn resume_retires_a_job_that_was_running_when_the_app_died() {
        let dir = make_temp_dir("jobs-resume-stale");
        std::fs::write(
            dir.join(JOBS_FILE),
            r#"{"schemaVersion":1,"running":1,"jobs":[
                {"id":"job-7","capabilityId":"rtk","action":"install","status":"running",
                 "startedAt":"2026-07-26T00:00:00Z","log":["$ brew install rtk"]}
            ]}"#,
        )
        .unwrap();
        let runner = JobRunner::new(fake_exec(&[]), Some(dir.clone()));
        let jobs = runner.list();
        assert_eq!(jobs.len(), 1);
        assert_eq!(jobs[0].status, JobStatus::Error);
        assert!(jobs[0].log.last().unwrap().contains("interrupted"));
        assert!(!runner.is_busy("rtk"), "a dead job must not hold the lock");

        let StartOutcome::Started(next) =
            runner.start("rtk", LifecycleAction::Install, vec![vec!["ok".into()]])
        else {
            panic!("the capability must be startable again");
        };
        assert_eq!(next.id, "job-8");
        runner.settle();
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn disk_last_finished_degrades_on_missing_or_corrupt_file() {
        let dir = make_temp_dir("jobs-last-finished-bad");
        assert!(read_last_finished(&dir).is_empty());
        std::fs::write(dir.join(JOBS_FILE), "{not json").unwrap();
        assert!(read_last_finished(&dir).is_empty());
        std::fs::write(dir.join(JOBS_FILE), r#"{"jobs": "nope"}"#).unwrap();
        assert!(read_last_finished(&dir).is_empty());
        std::fs::write(dir.join(JOBS_FILE), r#"{"jobs": [{"bogus": 1}]}"#).unwrap();
        assert!(read_last_finished(&dir).is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn timestamps_are_rfc3339_utc_shaped() {
        let now = JobRunner::now();
        assert_eq!(now.len(), 20);
        assert!(now.ends_with('Z'));
        assert_eq!(&now[4..5], "-");
        assert_eq!(&now[10..11], "T");
        let year: i64 = now[0..4].parse().unwrap();
        assert!(year >= 2026);
    }
}
