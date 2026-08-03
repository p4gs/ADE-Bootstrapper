//! Data engine for the Control Center — links ade-core directly (no server,
//! no IPC; ISC-175). A background thread refreshes detection; actions run on
//! worker threads through the shared in-process JobRunner (ISC-171).

use ade_core::config::{load_config, serialize_config, CONFIG_FILE};
use ade_core::exec::{real_exec, real_which};
use ade_core::fsutil::{sha256_hex, write_ensured};
use ade_core::gui::insights::{self, Insight};
use ade_core::gui::inventory::{
    action_argvs, detect_capabilities, get_capability, latest_version_argv, parse_latest_version,
    with_timeout, CapabilityStatus, DetectOptions, GroupCoverage, LifecycleAction,
    CAPABILITY_GROUPS,
};
use ade_core::gui::jobs::{Job, JobRunner, StartOutcome};
use ade_core::gui::projects::remove_ade_from_project;
use ade_core::gui::state::{load_gui_state, save_gui_state};
use ade_core::gui::stats;
use ade_core::harness::adapters::HARNESS_ADAPTERS;
use ade_core::registry::module_ids;
use ade_core::report::{status_report, StatusRow};
use ade_core::run::{apply_pipeline, make_ctx, verify_pipeline, PipelineDeps};
use ade_core::types::{ExecFn, ExecOpts, ModuleConfig, WhichFn};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Phase G — the metrics foundation: per-capability-group facts for the
/// capability page's stats strip. Every field is real, probed data or
/// absent — never a placeholder. See `ade_core::gui::stats` for how each
/// field is computed and what its real data source is.
#[derive(Debug, Clone, Default)]
pub struct GroupStats {
    /// Tier 1 — the most recently finished job among this group's providers.
    pub last_job: Option<stats::JobRecency>,
    /// Tier 1 — of the registered projects this session has inspected, how
    /// many have this group's corresponding `ade.json` module applied.
    /// Absent for groups with no corresponding module at all.
    pub project_coverage: Option<stats::ProjectCoverage>,
    /// Tier 2 — token-efficiency only: `rtk gain --format json`.
    pub token_gain: Option<stats::TokenGain>,
    /// Tier 2 — dependency-scanning only: OSV local-database cache age.
    pub osv_db_age: Option<stats::OsvDbAge>,
    /// Tier 2 — semantic-search only: the most-stale registered project's
    /// CocoIndex index vs. its own HEAD commit, in days.
    pub index_staleness_days: Option<i64>,
    /// Tier 2 — hook-orchestration only: the slowest measured real
    /// pre-commit hook run among registered projects, in milliseconds.
    pub hook_latency_millis: Option<u64>,
    /// Tier 2 — agent-security-rules only: which harness surfaces (Claude
    /// Code + whichever of Codex/OpenCode/Cursor/Antigravity have a real,
    /// on-disk CodeGuard install) are actually covered right now.
    pub harness_surfaces: Vec<stats::HarnessSurface>,
    /// Tier 2 — repo-hygiene only: of the registered projects, how many
    /// have `commit.gpgsign` actually enabled.
    pub hardened_repos: Option<stats::HardenedRepoCoverage>,
    /// Tier 3 — secret-scanning only: what the pre-commit boundary actually
    /// caught, summed across registered projects' count-only scan logs
    /// (ISC-301; the log is deliberately redacted — see `stats`).
    pub secrets_catches: Option<stats::SecretsCatchLog>,
}

#[derive(Debug, Clone)]
pub struct ProjectSummary {
    pub id: String,
    pub dir: String,
    pub config_ok: bool,
}

#[derive(Debug, Clone)]
pub struct ProjectReport {
    pub verify_ok: bool,
    pub modules: Vec<StatusRow>,
}

/// `Clone` so the UI can snapshot the whole state in one `guard.clone()` —
/// the previous hand-copied field list silently dropped any field added later.
#[derive(Default, Clone)]
pub struct Shared {
    pub capabilities: Vec<CapabilityStatus>,
    pub have_first_detection: bool,
    /// Navigation scope mirrored from gui.json — a capability-group id or one
    /// of the reserved `SCOPE_*` sections. Restored on launch.
    pub scope: String,
    /// When the last detection pass finished — the freshness footer's source.
    pub last_detection_at: Option<std::time::Instant>,
    /// True while a detection pass is actually probing binaries.
    pub detecting: bool,
    pub state_warning: Option<String>,
    pub projects: Vec<ProjectSummary>,
    pub jobs: Vec<Job>,
    pub reports: HashMap<String, ProjectReport>,
    pub report_errors: HashMap<String, String>,
    pub toast: Option<String>,
    pub checking_updates: bool,
    pub pending: HashSet<String>,
    /// Phase G — per-capability-group stats-strip facts, keyed by group id.
    /// Recomputed alongside every detection pass (except `hook_latency_millis`,
    /// which is measured at most once per project per session — see `Engine`).
    pub group_stats: HashMap<String, GroupStats>,
    /// Phase H — every real insight the current facts support, worst/most-
    /// actionable first (`ade_core::gui::insights::build_insights`). NOT
    /// pre-filtered for dismissal — `dismissed_insights` below is the filter
    /// every view applies at render time, the same split `disabled`
    /// capabilities already use (present in the facts, filtered in the view).
    pub insights: Vec<Insight>,
    /// Phase H — dismissed `Insight::id`s, mirrored from gui.json on every
    /// poll AND updated immediately on `Engine::dismiss_insight` so a click
    /// never waits for the next ~1-4s tick to disappear.
    pub dismissed_insights: HashSet<String>,
}

/// Lock `Shared`, recovering from poison instead of propagating it.
///
/// `Shared` is read on EVERY frame as the very first thing `ui()` does
/// (`app.rs`'s `Shared::clone()` call). A `Mutex` poisons itself the moment
/// any thread panics while holding the lock — and once poisoned, every
/// future `.lock().expect(...)` on it panics too, on every subsequent
/// frame, forever, before a single pixel of that frame's content is drawn.
/// One unlucky panic on a background detection/action thread would
/// otherwise wedge the entire window blank for the rest of the process's
/// life. A UI's shared render state must survive a poisoned lock; the data
/// inside is merely stale for one frame, not unusable.
pub fn lock_shared(shared: &Mutex<Shared>) -> std::sync::MutexGuard<'_, Shared> {
    shared
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

pub struct Engine {
    pub shared: Arc<Mutex<Shared>>,
    pub runner: JobRunner,
    home: PathBuf,
    latest: Arc<Mutex<BTreeMap<String, String>>>,
    refresh_now: Arc<AtomicBool>,
    /// Detection's process seams, injected so the poll → `Shared` refresh is
    /// testable without probing the real machine. Production passes the real
    /// ones; behaviour there is unchanged.
    exec: ExecFn,
    which: WhichFn,
    /// The real user home directory (`$HOME`, distinct from `$ADE_HOME`) —
    /// where the Tier-2 machine-wide probes (the OSV database cache, the
    /// CodeGuard per-harness surfaces) look. `None` only in the pathological
    /// case where `$HOME` itself is unset; those probes then simply report
    /// absent rather than guessing a path.
    user_home: Option<PathBuf>,
    /// Hook-chain latency is measured by actually running a project's
    /// installed hook — real signal, but potentially slow on a first run (a
    /// fresh `pre-commit` virtualenv bootstrap can take tens of seconds) — so
    /// each project is timed AT MOST ONCE per app session (keyed by project
    /// id), never re-measured on every ~1-4s poll tick.
    hook_latency_cache: Arc<Mutex<BTreeMap<String, u64>>>,
}

pub fn project_id(dir: &str) -> String {
    sha256_hex(dir)[..12].to_string()
}

fn harness_ids() -> Vec<&'static str> {
    HARNESS_ADAPTERS.iter().map(|adapter| adapter.id).collect()
}

impl Engine {
    pub fn new(home: PathBuf) -> Arc<Engine> {
        Engine::with_deps(
            home,
            real_exec(),
            real_which(),
            std::env::var("HOME").ok().map(PathBuf::from),
        )
    }

    /// The injectable constructor `new` delegates to. Tests hand it a fake
    /// exec/which pair (and a deterministic `user_home` — often `None`, so
    /// the Tier-2 machine-wide probes that read `$HOME` never touch the
    /// real test-running machine's files); everything downstream (runner,
    /// detection, `Shared`) behaves identically.
    fn with_deps(
        home: PathBuf,
        exec: ExecFn,
        which: WhichFn,
        user_home: Option<PathBuf>,
    ) -> Arc<Engine> {
        let runner = JobRunner::new(exec.clone(), Some(home.clone()));
        // Seed the persisted view preferences and job history BEFORE the first
        // paint. Detection is slow (it probes real binaries), but reading
        // gui.json is not — and defaulting until it finishes made the launch
        // frame show settings the user never chose (the window opening on the
        // Overview for a second before jumping to where it was left).
        let load = load_gui_state(&home);
        let shared = Shared {
            scope: load.state.scope.clone(),
            state_warning: load.warning,
            jobs: runner.list(),
            dismissed_insights: load.state.dismissed_insights.into_iter().collect(),
            ..Shared::default()
        };
        Arc::new(Engine {
            shared: Arc::new(Mutex::new(shared)),
            runner,
            home,
            latest: Arc::new(Mutex::new(BTreeMap::new())),
            refresh_now: Arc::new(AtomicBool::new(false)),
            exec,
            which,
            user_home,
            hook_latency_cache: Arc::new(Mutex::new(BTreeMap::new())),
        })
    }

    pub fn request_refresh(&self) {
        self.refresh_now.store(true, Ordering::Relaxed);
    }

    fn detect_once(&self) {
        lock_shared(&self.shared).detecting = true;
        let load = load_gui_state(&self.home);
        let latest = self
            .latest
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone();
        let last_jobs = self.runner.last_finished();
        let capabilities = detect_capabilities(&DetectOptions {
            exec: self.exec.clone(),
            which: self.which.clone(),
            disabled: &load.state.disabled,
            last_jobs: &last_jobs,
            latest_versions: &latest,
            probe_running: true,
            probe_timeout: Duration::from_secs(5),
        });
        let projects: Vec<ProjectSummary> = load
            .state
            .projects
            .iter()
            .map(|dir| ProjectSummary {
                id: project_id(dir),
                dir: dir.clone(),
                config_ok: Path::new(dir).join(CONFIG_FILE).is_file(),
            })
            .collect();
        let jobs = self.runner.list();
        let scope = load.state.scope.clone();
        // A snapshot, not a held lock: `refresh_group_stats` below makes
        // several real subprocess calls (git, rtk) and must not hold
        // `Shared`'s mutex — read every frame elsewhere — for the duration.
        let reports_snapshot = lock_shared(&self.shared).reports.clone();
        let group_stats =
            self.refresh_group_stats(&capabilities, &jobs, &reports_snapshot, &projects);
        // Phase H: every real insight the just-computed stats support, built
        // from the SAME `group_stats` the strip renders — an insight can
        // never claim a fact the strip does not already show.
        let insights = insights::build_insights(&group_facts(&group_stats));
        let mut shared = lock_shared(&self.shared);
        shared.capabilities = capabilities;
        shared.have_first_detection = true;
        shared.scope = scope;
        shared.state_warning = load.warning;
        shared.projects = projects;
        shared.jobs = jobs;
        shared.group_stats = group_stats;
        shared.insights = insights;
        shared.dismissed_insights = load.state.dismissed_insights.into_iter().collect();
        shared.detecting = false;
        shared.last_detection_at = Some(std::time::Instant::now());
    }

    /// Phase G: compute every capability group's stats-strip facts from
    /// this pass's fresh data. Tier 1 (`last_job`, `project_coverage`) is
    /// pure arithmetic over facts already in hand. Tier 2 runs the six
    /// probes researched and grounded in `ade_core::gui::stats`'s doc
    /// comments — each scoped to the one group it is real evidence for.
    fn refresh_group_stats(
        &self,
        capabilities: &[CapabilityStatus],
        jobs: &[Job],
        reports: &HashMap<String, ProjectReport>,
        projects: &[ProjectSummary],
    ) -> HashMap<String, GroupStats> {
        let now_seconds = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let mut out: HashMap<String, GroupStats> = HashMap::new();
        for group in CAPABILITY_GROUPS.iter() {
            let mut group_stat = GroupStats {
                last_job: stats::last_group_job(jobs, group.id, now_seconds),
                ..GroupStats::default()
            };
            if let Some(module_id) = stats::module_for_group(group.id) {
                let flags: Vec<bool> = projects
                    .iter()
                    .filter_map(|project| reports.get(&project.id))
                    .filter_map(|report| {
                        report
                            .modules
                            .iter()
                            .find(|row| row.id == module_id)
                            .map(|row| stats::module_state_covers(&row.state))
                    })
                    .collect();
                group_stat.project_coverage = stats::project_coverage(&flags);
            }
            out.insert(group.id.to_string(), group_stat);
        }

        // Token efficiency: rtk gain — only if rtk is actually installed
        // (an absent binary must not spend a probe finding that out again).
        if let Some(rtk) = capabilities.iter().find(|cap| cap.id == "rtk") {
            if rtk.installed {
                let bounded = with_timeout(self.exec.clone(), Duration::from_secs(5));
                let result = bounded(&stats::rtk_gain_argv(), &ExecOpts::default());
                if result.code == 0 {
                    if let Some(entry) = out.get_mut("token-efficiency") {
                        entry.token_gain = stats::parse_rtk_gain(&result.stdout);
                    }
                }
            }
        }

        // Dependency scanning: OSV local-database cache age (pure fs read).
        if let Some(home) = &self.user_home {
            let cache_dir = stats::osv_scanner_cache_dir(home);
            if let Some(entry) = out.get_mut("dependency-scanning") {
                entry.osv_db_age = stats::osv_db_age(&cache_dir, now_seconds);
            }
        }

        // Agent security rules: CodeGuard surfaces beyond Claude Code (whose
        // own coverage is already the `codeguard` capability's own fact).
        if let Some(home) = &self.user_home {
            let mut surfaces = stats::codeguard_harness_surfaces(home);
            let claude_covered = capabilities
                .iter()
                .find(|cap| cap.id == "codeguard")
                .map(|cap| {
                    GroupCoverage::provider_works(cap.enabled, cap.installed, cap.is_healthy())
                })
                .unwrap_or(false);
            surfaces.insert(
                0,
                stats::HarnessSurface {
                    harness_id: "claude-code",
                    harness_name: "Claude Code",
                    covered: claude_covered,
                },
            );
            if let Some(entry) = out.get_mut("agent-security-rules") {
                entry.harness_surfaces = surfaces;
            }
        }

        // Secret scanning: what the boundary actually caught — pure fs reads
        // of each registered project's redacted count-only scan log.
        {
            let logs: Vec<stats::SecretsCatchLog> = projects
                .iter()
                .filter_map(|project| {
                    std::fs::read_to_string(stats::secrets_log_path(Path::new(&project.dir))).ok()
                })
                .filter_map(|content| stats::parse_secrets_log(&content))
                .collect();
            if let Some(entry) = out.get_mut("secret-scanning") {
                entry.secrets_catches = stats::merge_secrets_logs(&logs);
            }
        }

        // Repo hygiene: hardened git config (`commit.gpgsign`) per registered project.
        {
            let bounded = with_timeout(self.exec.clone(), Duration::from_secs(3));
            let flags: Vec<bool> = projects
                .iter()
                .map(|project| {
                    let result =
                        bounded(&stats::git_signing_argv(&project.dir), &ExecOpts::default());
                    stats::parse_git_signing(&result)
                })
                .collect();
            if let Some(entry) = out.get_mut("repo-hygiene") {
                entry.hardened_repos = stats::hardened_repo_coverage(&flags);
            }
        }

        // Semantic search: CocoIndex index staleness vs HEAD — the worst
        // (most stale) among registered projects that actually have an index.
        {
            let bounded = with_timeout(self.exec.clone(), Duration::from_secs(3));
            let mut worst: Option<i64> = None;
            for project in projects {
                let index_path = stats::cocoindex_index_path(Path::new(&project.dir));
                let Ok(meta) = std::fs::metadata(&index_path) else {
                    continue;
                };
                let Ok(modified) = meta.modified() else {
                    continue;
                };
                let Ok(index_elapsed) = modified.duration_since(std::time::UNIX_EPOCH) else {
                    continue;
                };
                let head_result = bounded(
                    &stats::git_head_commit_argv(&project.dir),
                    &ExecOpts::default(),
                );
                let Some(head_seconds) = stats::parse_unix_seconds(&head_result) else {
                    continue;
                };
                let staleness = stats::index_staleness_days(index_elapsed.as_secs(), head_seconds);
                worst = Some(worst.map_or(staleness, |current: i64| current.max(staleness)));
            }
            if let Some(entry) = out.get_mut("semantic-search") {
                entry.index_staleness_days = worst;
            }
        }

        // Hook orchestration: hook-chain latency, measured at most once per
        // project per session (see `hook_latency_cache`'s doc comment).
        {
            let mut cache = self
                .hook_latency_cache
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            let mut worst_millis: Option<u64> = None;
            for project in projects {
                let millis = if let Some(cached) = cache.get(&project.id) {
                    Some(*cached)
                } else {
                    let hook_path = stats::pre_commit_hook_path(Path::new(&project.dir));
                    if is_executable_file(&hook_path) {
                        let measured = stats::time_hook_run(
                            &self.exec,
                            &hook_path.to_string_lossy(),
                            Path::new(&project.dir),
                            Duration::from_secs(8),
                        );
                        if let Some(latency) = &measured {
                            cache.insert(project.id.clone(), latency.millis);
                        }
                        measured.map(|latency| latency.millis)
                    } else {
                        None
                    }
                };
                if let Some(millis) = millis {
                    worst_millis = Some(worst_millis.map_or(millis, |current| current.max(millis)));
                }
            }
            if let Some(entry) = out.get_mut("hook-orchestration") {
                entry.hook_latency_millis = worst_millis;
            }
        }

        out
    }

    /// Spawn the background refresh loop; repaints via the given callback.
    pub fn start_poll(self: &Arc<Self>, repaint: impl Fn() + Send + 'static) {
        let engine = Arc::clone(self);
        std::thread::spawn(move || loop {
            // Repaint as detection starts so the freshness footer's spinner
            // reflects a pass that is actually running, not one that finished.
            repaint();
            engine.detect_once();
            repaint();
            let busy = engine.runner.running_count() > 0;
            let interval_ms: u64 = if busy { 1200 } else { 4000 };
            let mut slept = 0;
            while slept < interval_ms {
                if engine.refresh_now.swap(false, Ordering::Relaxed) {
                    break;
                }
                std::thread::sleep(Duration::from_millis(100));
                slept += 100;
            }
        });
    }

    fn toast(&self, message: String) {
        lock_shared(&self.shared).toast = Some(message);
    }

    /// The one sanctioned way for the UI to clear a notice — views never
    /// reach into the lock themselves.
    pub fn dismiss_toast(&self) {
        lock_shared(&self.shared).toast = None;
    }

    fn with_pending(
        self: &Arc<Self>,
        key: String,
        repaint: impl Fn() + Send + 'static,
        work: impl FnOnce(&Engine) -> Result<String, String> + Send + 'static,
    ) {
        {
            let mut shared = lock_shared(&self.shared);
            if shared.pending.contains(&key) {
                return;
            }
            shared.pending.insert(key.clone());
        }
        let engine = Arc::clone(self);
        std::thread::spawn(move || {
            let outcome = work(&engine);
            {
                let mut shared = lock_shared(&engine.shared);
                shared.pending.remove(&key);
                match outcome {
                    Ok(message) => {
                        if !message.is_empty() {
                            shared.toast = Some(message);
                        }
                    }
                    Err(error) => shared.toast = Some(error),
                }
            }
            engine.request_refresh();
            repaint();
        });
    }

    /// Start a lifecycle action job (ISC-171: per-capability lock inside).
    pub fn start_action(&self, capability_id: &str, action: LifecycleAction) {
        let Some(def) = get_capability(capability_id) else {
            self.toast(format!("unknown capability: {capability_id}"));
            return;
        };
        let Some(argvs) = action_argvs(def, action) else {
            self.toast(format!(
                "{} has no automated {} recipe",
                def.name,
                action.as_str()
            ));
            return;
        };
        match self.runner.start(def.id, action, argvs) {
            StartOutcome::Started(job) => self.toast(format!(
                "{} started for {} ({})",
                action.as_str(),
                def.id,
                job.id
            )),
            StartOutcome::Busy => self.toast(format!("a job for {} is already running", def.name)),
        }
        self.request_refresh();
    }

    /// Persist the navigation scope so the window reopens where it was left.
    pub fn set_scope(&self, scope: &str) {
        let resolved = ade_core::gui::state::resolve_scope(scope);
        let mut load = load_gui_state(&self.home);
        load.state.scope = resolved.clone();
        if let Err(error) = save_gui_state(&self.home, &load.state) {
            self.toast(format!("could not save state: {error}"));
        }
        lock_shared(&self.shared).scope = resolved;
    }

    /// Phase H: persist a dismissal and reflect it in `Shared` immediately —
    /// the same "save then update the live lock" shape `set_scope` and
    /// `set_capability_enabled` already use, so a dismissed insight vanishes
    /// on the very next frame instead of waiting for the ~1-4s poll tick.
    pub fn dismiss_insight(&self, id: &str) {
        let mut load = load_gui_state(&self.home);
        load.state.dismissed_insights.insert(id.to_string());
        if let Err(error) = save_gui_state(&self.home, &load.state) {
            self.toast(format!("could not save state: {error}"));
        }
        lock_shared(&self.shared).dismissed_insights =
            load.state.dismissed_insights.into_iter().collect();
    }

    pub fn set_capability_enabled(&self, capability_id: &str, enabled: bool) {
        let mut load = load_gui_state(&self.home);
        if enabled {
            load.state.disabled.remove(capability_id);
        } else {
            load.state.disabled.insert(capability_id.to_string());
        }
        if let Err(error) = save_gui_state(&self.home, &load.state) {
            self.toast(format!("could not save state: {error}"));
        }
        self.request_refresh();
    }

    /// User-invoked ONLY (ISC-173): package-manager lookups happen here and nowhere else.
    pub fn check_updates(self: &Arc<Self>, repaint: impl Fn() + Send + 'static) {
        lock_shared(&self.shared).checking_updates = true;
        let engine = Arc::clone(self);
        self.with_pending("check-updates".into(), repaint, move |inner| {
            let lookup = with_timeout(real_exec(), Duration::from_secs(30));
            let mut found: BTreeMap<String, String> = BTreeMap::new();
            for def in ade_core::gui::inventory::CAPABILITIES.iter() {
                if let Some(argv) = latest_version_argv(def) {
                    let result = lookup(&argv, &ExecOpts::default());
                    if let Some(latest) = parse_latest_version(def, &result) {
                        found.insert(def.id.to_string(), latest);
                    }
                }
            }
            let count = found.len();
            inner
                .latest
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .extend(found);
            lock_shared(&engine.shared).checking_updates = false;
            Ok(format!("update check complete ({count} versions fetched)"))
        });
    }

    pub fn register_project(self: &Arc<Self>, dir: String, repaint: impl Fn() + Send + 'static) {
        self.with_pending("add-project".into(), repaint, move |engine| {
            let resolved = std::fs::canonicalize(&dir)
                .map(|path| path.to_string_lossy().to_string())
                .unwrap_or(dir.clone());
            load_config(Path::new(&resolved), &module_ids(), &harness_ids())
                .map_err(|error| format!("not an ADE project: {error}"))?;
            let mut load = load_gui_state(&engine.home);
            if !load.state.projects.contains(&resolved) {
                load.state.projects.push(resolved.clone());
                save_gui_state(&engine.home, &load.state).map_err(|error| error.to_string())?;
            }
            Ok(format!("registered {resolved}"))
        });
    }

    pub fn unregister_project(self: &Arc<Self>, id: String, repaint: impl Fn() + Send + 'static) {
        self.with_pending(format!("rm-{id}"), repaint, move |engine| {
            let mut load = load_gui_state(&engine.home);
            let before = load.state.projects.len();
            load.state.projects.retain(|dir| project_id(dir) != id);
            if load.state.projects.len() == before {
                return Err("unknown project".to_string());
            }
            save_gui_state(&engine.home, &load.state).map_err(|error| error.to_string())?;
            engine
                .shared
                .lock()
                .expect("shared lock")
                .reports
                .remove(&id);
            Ok("project removed".to_string())
        });
    }

    fn pipeline_deps() -> PipelineDeps {
        PipelineDeps {
            exec: real_exec(),
            which: real_which(),
            now: None,
        }
    }

    fn find_project_dir(&self, id: &str) -> Option<String> {
        load_gui_state(&self.home)
            .state
            .projects
            .iter()
            .find(|dir| project_id(dir) == id)
            .cloned()
    }

    pub fn load_report(self: &Arc<Self>, id: String, repaint: impl Fn() + Send + 'static) {
        self.with_pending(format!("report-{id}"), repaint, move |engine| {
            let Some(dir) = engine.find_project_dir(&id) else {
                return Err("unknown project".to_string());
            };
            let report = build_project_report(Path::new(&dir));
            let mut shared = lock_shared(&engine.shared);
            match report {
                Ok(report) => {
                    shared.report_errors.remove(&id);
                    shared.reports.insert(id.clone(), report);
                    Ok(String::new())
                }
                Err(error) => {
                    shared.report_errors.insert(id.clone(), error.clone());
                    Err(error)
                }
            }
        });
    }

    /// Module toggle: validated config edit → re-apply → re-lock → verify (ISC-174).
    pub fn set_module_enabled(
        self: &Arc<Self>,
        id: String,
        module_id: String,
        enabled: bool,
        repaint: impl Fn() + Send + 'static,
    ) {
        self.with_pending(format!("mod-{id}-{module_id}"), repaint, move |engine| {
            let Some(dir) = engine.find_project_dir(&id) else {
                return Err("unknown project".to_string());
            };
            let dir_path = Path::new(&dir);
            let mut config =
                load_config(dir_path, &module_ids(), &harness_ids()).map_err(|e| e.to_string())?;
            let options = config
                .modules
                .get(&module_id)
                .map(|module| module.options.clone())
                .unwrap_or_else(|| serde_json::json!({}));
            config
                .modules
                .insert(module_id.clone(), ModuleConfig { enabled, options });
            write_ensured(&dir_path.join(CONFIG_FILE), &serialize_config(&config))
                .map_err(|error| error.to_string())?;
            let deps = Engine::pipeline_deps();
            let ctx = make_ctx(dir_path, config.clone(), &deps);
            let apply = apply_pipeline(&ctx, &deps);
            let verify_ctx = make_ctx(dir_path, config, &deps);
            let verify = verify_pipeline(&verify_ctx);
            if let Ok(report) = build_project_report(dir_path) {
                engine
                    .shared
                    .lock()
                    .expect("shared lock")
                    .reports
                    .insert(id.clone(), report);
            }
            Ok(format!(
                "module {module_id} {} — apply {}, verify {}",
                if enabled { "enabled" } else { "disabled" },
                if apply.ok { "OK" } else { "FAILED" },
                if verify.ok { "PASS" } else { "FAIL" }
            ))
        });
    }

    /// Phase I: write the posture evidence report to `$ADE_HOME/export/` —
    /// calling `ade_core::gui::posture::collect_posture` DIRECTLY (a fresh,
    /// full re-detection, not a serialization of whatever happens to be
    /// cached in `Shared`) so the button and `ade export posture` can never
    /// silently disagree about what shipped: both call the identical
    /// function with the engine's own real deps.
    pub fn export_posture(self: &Arc<Self>, repaint: impl Fn() + Send + 'static) {
        self.with_pending("export-posture".into(), repaint, move |engine| {
            let deps = ade_core::gui::posture::PostureDeps {
                exec: engine.exec.clone(),
                which: engine.which.clone(),
                ade_home: engine.home.clone(),
                user_home: engine.user_home.clone(),
                now_seconds: None,
            };
            let report = ade_core::gui::posture::collect_posture(&deps);
            let markdown = ade_core::gui::posture::render_markdown(&report);
            let json_text = ade_core::gui::posture::render_json(&report);
            let export_dir = engine.home.join("export");
            write_ensured(&export_dir.join("posture.md"), &markdown)
                .map_err(|error| format!("could not write posture.md: {error}"))?;
            write_ensured(&export_dir.join("posture.json"), &json_text)
                .map_err(|error| format!("could not write posture.json: {error}"))?;
            let display = ade_core::gui::posture::redact_home(
                &export_dir.display().to_string(),
                engine.user_home.as_deref(),
            );
            Ok(format!(
                "exported posture report ({}) to {display}",
                report.verdict
            ))
        });
    }

    /// Withdraw ADE from a registered repo and stop tracking it here.
    /// The decision logic lives in `ade_core::gui::projects` — this is wiring.
    pub fn remove_ade_from_project(
        self: &Arc<Self>,
        id: String,
        repaint: impl Fn() + Send + 'static,
    ) {
        self.with_pending(format!("rm-ade-{id}"), repaint, move |engine| {
            let Some(dir) = engine.find_project_dir(&id) else {
                return Err("unknown project".to_string());
            };
            let message = remove_ade_from_project(&engine.home, &dir, &Engine::pipeline_deps())?;
            engine
                .shared
                .lock()
                .expect("shared lock")
                .reports
                .remove(&id);
            Ok(message)
        });
    }
}

/// Phase H: the ade-core-side `insights::GroupFacts` for every capability
/// group, built from the exact same `GroupStats` map the stats strip reads —
/// taxonomy order (not the map's own undefined iteration order), so an
/// insight can never claim a fact the strip does not already show, and
/// `build_insights`'s own order-independence is a belt-and-braces guarantee
/// rather than something this function needs to get right on its own.
fn group_facts(group_stats: &HashMap<String, GroupStats>) -> Vec<insights::GroupFacts> {
    CAPABILITY_GROUPS
        .iter()
        .map(|group| {
            let stats = group_stats.get(group.id);
            insights::GroupFacts {
                group_id: group.id,
                group_name: group.name,
                last_job: stats.and_then(|s| s.last_job.clone()),
                project_coverage: stats.and_then(|s| s.project_coverage),
                token_gain: stats.and_then(|s| s.token_gain.clone()),
                osv_db_age: stats.and_then(|s| s.osv_db_age),
                index_staleness_days: stats.and_then(|s| s.index_staleness_days),
                hook_latency_millis: stats.and_then(|s| s.hook_latency_millis),
                harness_surfaces: stats
                    .map(|s| s.harness_surfaces.clone())
                    .unwrap_or_default(),
                hardened_repos: stats.and_then(|s| s.hardened_repos),
            }
        })
        .collect()
}

/// Whether a real git hook is actually installed and runnable — the gate
/// before `stats::time_hook_run` spends a real subprocess call: a
/// non-executable or absent file would just report a spawn failure's
/// (near-zero, meaningless) wall time as if it were a genuine hook cost.
fn is_executable_file(path: &Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::metadata(path)
            .map(|meta| meta.is_file() && meta.permissions().mode() & 0o111 != 0)
            .unwrap_or(false)
    }
    #[cfg(not(unix))]
    {
        path.is_file()
    }
}

fn build_project_report(dir: &Path) -> Result<ProjectReport, String> {
    let config =
        load_config(dir, &module_ids(), &harness_ids()).map_err(|error| error.to_string())?;
    let deps = Engine::pipeline_deps();
    let ctx = make_ctx(dir, config, &deps);
    let modules = status_report(&ctx);
    let verify = verify_pipeline(&ctx);
    Ok(ProjectReport {
        verify_ok: verify.ok,
        modules,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use ade_core::gui::jobs::{JobStatus, StartOutcome};
    use ade_core::testutil::make_temp_dir;
    use ade_core::types::ExecResult;
    use std::sync::mpsc;

    /// The seam the Activity log tail's liveness rests on: while a job is
    /// running the busy repoll (`start_poll` → `detect_once` every ~1.2s)
    /// copies the runner's CURRENT jobs — captured log lines included — into
    /// `Shared.jobs`, and again once the job finishes. Hermetic: detection
    /// runs against an injected exec/which pair, never the real machine.
    #[test]
    fn a_detection_pass_refreshes_shared_jobs_mid_run() {
        let home = make_temp_dir("engine-poll");
        let (release, gate) = mpsc::channel::<()>();
        let gate = Arc::new(Mutex::new(gate));
        let exec: ExecFn = Arc::new(move |argv: &[String], _opts: &ExecOpts| {
            if argv.first().map(String::as_str) == Some("fake-install") {
                // The job under test holds here until the test releases it.
                let _ = gate.lock().expect("gate").recv();
                return ExecResult {
                    code: 0,
                    stdout: "installed\n".to_string(),
                    stderr: String::new(),
                };
            }
            // Every incidental detection probe answers instantly and
            // negatively — the pass must never block on the gate.
            ExecResult {
                code: 1,
                stdout: String::new(),
                stderr: String::new(),
            }
        });
        let which: WhichFn = Arc::new(|_bin: &str| None);
        // `user_home: None` — this test's `which` fake answers everything
        // negatively; a real `$HOME` would break that hermeticity by letting
        // the Tier-2 machine-wide probes read the real test-running
        // machine's files.
        let engine = Engine::with_deps(home.clone(), exec, which, None);

        let StartOutcome::Started(job) = engine.runner.start(
            "nono",
            LifecycleAction::Install,
            vec![vec!["fake-install".to_string()]],
        ) else {
            panic!("expected start");
        };
        // The worker thread logs the command before exec blocks it; wait for
        // that line so the mid-run snapshot below is deterministic.
        while engine
            .runner
            .get(&job.id)
            .map(|running| running.log.is_empty())
            .unwrap_or(true)
        {
            std::thread::sleep(Duration::from_millis(5));
        }

        engine.detect_once();
        {
            let shared = engine.shared.lock().expect("shared lock");
            assert!(shared.have_first_detection);
            let seen = shared
                .jobs
                .iter()
                .find(|entry| entry.id == job.id)
                .expect("the running job is visible to the views mid-run");
            assert_eq!(seen.status, JobStatus::Running);
            assert_eq!(seen.log, vec!["$ fake-install".to_string()]);
        }

        release.send(()).expect("release the gated job");
        engine.runner.settle();
        engine.detect_once();
        {
            let shared = engine.shared.lock().expect("shared lock");
            let done = shared
                .jobs
                .iter()
                .find(|entry| entry.id == job.id)
                .expect("the finished job is visible");
            assert_eq!(done.status, JobStatus::Ok);
            assert_eq!(done.exit_code, Some(0));
            assert!(done.log.contains(&"installed".to_string()));
        }
        let _ = std::fs::remove_dir_all(&home);
    }
}
