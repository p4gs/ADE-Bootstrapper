//! Data engine for the Control Center — links ade-core directly (no server,
//! no IPC; ISC-175). A background thread refreshes detection; actions run on
//! worker threads through the shared in-process JobRunner (ISC-171).

use ade_core::config::{load_config, serialize_config, CONFIG_FILE};
use ade_core::exec::{real_exec, real_which};
use ade_core::fsutil::{sha256_hex, write_ensured};
use ade_core::gui::inventory::{
    action_argvs, detect_capabilities, get_capability, latest_version_argv, parse_latest_version,
    with_timeout, CapabilityStatus, DetectOptions, LifecycleAction,
};
use ade_core::gui::jobs::{Job, JobRunner, StartOutcome};
use ade_core::gui::projects::remove_ade_from_project;
use ade_core::gui::state::{load_gui_state, save_gui_state};
use ade_core::harness::adapters::HARNESS_ADAPTERS;
use ade_core::registry::module_ids;
use ade_core::report::{status_report, StatusRow};
use ade_core::run::{apply_pipeline, make_ctx, verify_pipeline, PipelineDeps};
use ade_core::types::{ExecOpts, ModuleConfig};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct ProjectSummary {
    pub id: String,
    pub dir: String,
    pub config_ok: bool,
}

#[derive(Debug, Clone)]
pub struct ProjectReport {
    pub verify_ok: bool,
    pub error: Option<String>,
    pub modules: Vec<StatusRow>,
}

#[derive(Default)]
pub struct Shared {
    pub capabilities: Vec<CapabilityStatus>,
    pub have_first_detection: bool,
    /// View preference mirrored from gui.json (ISC-206).
    pub group_by_capability: bool,
    pub state_warning: Option<String>,
    pub projects: Vec<ProjectSummary>,
    pub jobs: Vec<Job>,
    pub reports: HashMap<String, ProjectReport>,
    pub report_errors: HashMap<String, String>,
    pub toast: Option<String>,
    pub checking_updates: bool,
    pub pending: HashSet<String>,
}

pub struct Engine {
    pub shared: Arc<Mutex<Shared>>,
    pub runner: JobRunner,
    home: PathBuf,
    latest: Arc<Mutex<BTreeMap<String, String>>>,
    refresh_now: Arc<AtomicBool>,
}

pub fn project_id(dir: &str) -> String {
    sha256_hex(dir)[..12].to_string()
}

fn harness_ids() -> Vec<&'static str> {
    HARNESS_ADAPTERS.iter().map(|adapter| adapter.id).collect()
}

impl Engine {
    pub fn new(home: PathBuf) -> Arc<Engine> {
        let runner = JobRunner::new(real_exec(), Some(home.clone()));
        // Seed the persisted view preferences and job history BEFORE the first
        // paint. Detection is slow (it probes real binaries), but reading
        // gui.json is not — and defaulting until it finishes made the launch
        // frame show settings the user never chose (the grouped-view toggle
        // rendering OFF for a second on every start).
        let load = load_gui_state(&home);
        let shared = Shared {
            group_by_capability: load.state.group_by_capability,
            state_warning: load.warning,
            jobs: runner.list(),
            ..Shared::default()
        };
        Arc::new(Engine {
            shared: Arc::new(Mutex::new(shared)),
            runner,
            home,
            latest: Arc::new(Mutex::new(BTreeMap::new())),
            refresh_now: Arc::new(AtomicBool::new(false)),
        })
    }

    pub fn request_refresh(&self) {
        self.refresh_now.store(true, Ordering::Relaxed);
    }

    fn detect_once(&self) {
        let load = load_gui_state(&self.home);
        let latest = self.latest.lock().expect("latest lock").clone();
        let last_jobs = self.runner.last_finished();
        let capabilities = detect_capabilities(&DetectOptions {
            exec: real_exec(),
            which: real_which(),
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
        let group_by_capability = load.state.group_by_capability;
        let mut shared = self.shared.lock().expect("shared lock");
        shared.capabilities = capabilities;
        shared.have_first_detection = true;
        shared.group_by_capability = group_by_capability;
        shared.state_warning = load.warning;
        shared.projects = projects;
        shared.jobs = jobs;
    }

    /// Spawn the background refresh loop; repaints via the given callback.
    pub fn start_poll(self: &Arc<Self>, repaint: impl Fn() + Send + 'static) {
        let engine = Arc::clone(self);
        std::thread::spawn(move || loop {
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
        self.shared.lock().expect("shared lock").toast = Some(message);
    }

    fn with_pending(
        self: &Arc<Self>,
        key: String,
        repaint: impl Fn() + Send + 'static,
        work: impl FnOnce(&Engine) -> Result<String, String> + Send + 'static,
    ) {
        {
            let mut shared = self.shared.lock().expect("shared lock");
            if shared.pending.contains(&key) {
                return;
            }
            shared.pending.insert(key.clone());
        }
        let engine = Arc::clone(self);
        std::thread::spawn(move || {
            let outcome = work(&engine);
            {
                let mut shared = engine.shared.lock().expect("shared lock");
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

    /// Persist the grouped-view preference (ISC-206).
    pub fn set_group_by_capability(&self, grouped: bool) {
        let mut load = load_gui_state(&self.home);
        load.state.group_by_capability = grouped;
        if let Err(error) = save_gui_state(&self.home, &load.state) {
            self.toast(format!("could not save state: {error}"));
        }
        self.shared.lock().expect("shared lock").group_by_capability = grouped;
        self.request_refresh();
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
        self.shared.lock().expect("shared lock").checking_updates = true;
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
            inner.latest.lock().expect("latest lock").extend(found);
            engine.shared.lock().expect("shared lock").checking_updates = false;
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
            let mut shared = engine.shared.lock().expect("shared lock");
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

fn build_project_report(dir: &Path) -> Result<ProjectReport, String> {
    let config =
        load_config(dir, &module_ids(), &harness_ids()).map_err(|error| error.to_string())?;
    let deps = Engine::pipeline_deps();
    let ctx = make_ctx(dir, config, &deps);
    let modules = status_report(&ctx);
    let verify = verify_pipeline(&ctx);
    Ok(ProjectReport {
        verify_ok: verify.ok,
        error: None,
        modules,
    })
}

/// UI-facing action list for a capability in its current state.
pub fn actions_for(cap: &CapabilityStatus) -> Vec<LifecycleAction> {
    use ade_core::gui::inventory::LifecycleMethod;
    if cap.method == LifecycleMethod::Manual {
        return Vec::new();
    }
    if cap.installed {
        vec![
            LifecycleAction::Update,
            LifecycleAction::Reinstall,
            LifecycleAction::Uninstall,
        ]
    } else {
        vec![LifecycleAction::Install]
    }
}
