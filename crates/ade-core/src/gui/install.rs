//! `ade gui install` / `uninstall` / `status` — put the native apps on the
//! machine (ISC-192/193/194). macOS-only in v0.2.
//!
//! Installs: the `ade` binary into ~/.local/bin, both app bundles into
//! ~/Applications, and ONE LaunchAgent (the tray; there is no server). Every
//! system mutation goes through the injected ExecFn as argv arrays; all
//! directories are injectable so the full flow is testable against temp dirs.

use crate::fsutil::write_ensured;
use crate::types::{ExecFn, ExecOpts};
use std::path::{Path, PathBuf};

pub const STATUS_LABEL: &str = "com.ade-bootstrapper.status";
pub const CONTROL_CENTER_APP: &str = "ADE Control Center.app";
pub const STATUS_APP: &str = "ADE Status.app";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StepReport {
    pub step: String,
    pub ok: bool,
    pub detail: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstallReport {
    pub ok: bool,
    pub steps: Vec<StepReport>,
}

impl InstallReport {
    fn push(&mut self, step: &str, ok: bool, detail: Option<String>) -> bool {
        self.steps.push(StepReport {
            step: step.to_string(),
            ok,
            detail,
        });
        if !ok {
            self.ok = false;
        }
        ok
    }
}

pub struct InstallDeps {
    pub exec: ExecFn,
    /// Repo root (bundle script + release binaries live under it).
    pub repo_root: PathBuf,
    /// ~/Applications equivalent.
    pub apps_dir: PathBuf,
    /// ~/Library/LaunchAgents equivalent.
    pub launch_agents_dir: PathBuf,
    /// ~/.local/bin equivalent (CLI install target).
    pub bin_dir: PathBuf,
    /// $ADE_HOME (logs + state).
    pub ade_home: PathBuf,
    /// launchd domain uid; None → resolved via `id -u`.
    pub uid: Option<String>,
    /// Skip the cargo/bundle build (tests).
    pub skip_build: bool,
    /// Extra PATH entries baked into the agent plist (package-manager dirs).
    pub home_dir: PathBuf,
    /// Pause between launchd settle polls / bootstrap retries (tests pass ZERO).
    pub settle_interval: std::time::Duration,
}

/// `launchctl bootout` returns before the job is actually gone. Bootstrapping
/// into a domain that still holds the tearing-down job fails with
/// "5: Input/output error" — observed on a real reinstall. Poll until the old
/// job disappears (bounded), so a reinstall is not a coin flip.
const SERVICE_GONE_POLLS: u32 = 20;
pub const SERVICE_POLL_INTERVAL: std::time::Duration = std::time::Duration::from_millis(100);
const BOOTSTRAP_ATTEMPTS: u32 = 3;

fn wait_for_service_gone(deps: &InstallDeps, target: &str) {
    for _ in 0..SERVICE_GONE_POLLS {
        if run(&deps.exec, &["launchctl", "print", target]).code != 0 {
            return;
        }
        std::thread::sleep(deps.settle_interval);
    }
}

fn bootstrap_with_retry(
    deps: &InstallDeps,
    domain: &str,
    plist_path: &Path,
) -> crate::types::ExecResult {
    let mut last = run(
        &deps.exec,
        &[
            "launchctl",
            "bootstrap",
            domain,
            &plist_path.to_string_lossy(),
        ],
    );
    for _ in 1..BOOTSTRAP_ATTEMPTS {
        if last.code == 0 {
            break;
        }
        std::thread::sleep(deps.settle_interval);
        last = run(
            &deps.exec,
            &[
                "launchctl",
                "bootstrap",
                domain,
                &plist_path.to_string_lossy(),
            ],
        );
    }
    last
}

fn run(exec: &ExecFn, argv: &[&str]) -> crate::types::ExecResult {
    let owned: Vec<String> = argv.iter().map(|s| s.to_string()).collect();
    exec(&owned, &ExecOpts::default())
}

fn resolve_uid(deps: &InstallDeps) -> String {
    match &deps.uid {
        Some(uid) => uid.clone(),
        None => run(&deps.exec, &["id", "-u"]).stdout.trim().to_string(),
    }
}

/// PATH for the launchd agent: package-manager dirs + the standard system path
/// (launchd's default PATH is only /usr/bin:/bin:/usr/sbin:/sbin — ISC-194).
pub fn agent_path(home_dir: &Path, bin_dir: &Path) -> String {
    let mut entries: Vec<String> = vec![
        bin_dir.to_string_lossy().to_string(),
        "/opt/homebrew/bin".to_string(),
        "/usr/local/bin".to_string(),
        home_dir.join(".local/bin").to_string_lossy().to_string(),
        home_dir
            .join(".npm-global/bin")
            .to_string_lossy()
            .to_string(),
        home_dir.join(".bun/bin").to_string_lossy().to_string(),
        home_dir.join(".cargo/bin").to_string_lossy().to_string(),
        "/usr/bin".to_string(),
        "/bin".to_string(),
        "/usr/sbin".to_string(),
        "/sbin".to_string(),
    ];
    entries.dedup();
    let mut seen = std::collections::BTreeSet::new();
    entries.retain(|entry| seen.insert(entry.clone()));
    entries.join(":")
}

fn tray_plist(deps: &InstallDeps) -> String {
    let program = deps
        .apps_dir
        .join(STATUS_APP)
        .join("Contents/MacOS/ade-status");
    let log_dir = deps.ade_home.join("logs");
    let path = agent_path(&deps.home_dir, &deps.bin_dir);
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key>
  <string>{label}</string>
  <key>ProgramArguments</key>
  <array>
    <string>{program}</string>
  </array>
  <key>RunAtLoad</key>
  <true/>
  <key>KeepAlive</key>
  <dict>
    <key>SuccessfulExit</key>
    <false/>
  </dict>
  <key>EnvironmentVariables</key>
  <dict>
    <key>PATH</key>
    <string>{path}</string>
    <key>ADE_HOME</key>
    <string>{home}</string>
  </dict>
  <key>StandardOutPath</key>
  <string>{logs}/{label}.out.log</string>
  <key>StandardErrorPath</key>
  <string>{logs}/{label}.err.log</string>
</dict>
</plist>
"#,
        label = STATUS_LABEL,
        program = program.display(),
        path = path,
        home = deps.ade_home.display(),
        logs = log_dir.display(),
    )
}

pub fn gui_install(deps: &InstallDeps) -> InstallReport {
    let mut report = InstallReport {
        ok: true,
        steps: Vec::new(),
    };
    if !cfg!(target_os = "macos") {
        report.push(
            "platform",
            false,
            Some("gui install is macOS-only in v0.2".into()),
        );
        return report;
    }

    if !deps.skip_build {
        let bundle = run(
            &deps.exec,
            &[
                "bash",
                &deps
                    .repo_root
                    .join("scripts/bundle-apps.sh")
                    .to_string_lossy(),
            ],
        );
        let ok = bundle.code == 0;
        let detail = if ok {
            None
        } else {
            Some(
                bundle
                    .stderr
                    .chars()
                    .rev()
                    .take(600)
                    .collect::<String>()
                    .chars()
                    .rev()
                    .collect(),
            )
        };
        if !report.push("build bundles", ok, detail) {
            return report;
        }
    }

    // Install the CLI binary.
    {
        let source = deps.repo_root.join("target/release/ade");
        let dest = deps.bin_dir.join("ade");
        let _ = run(
            &deps.exec,
            &["mkdir", "-p", &deps.bin_dir.to_string_lossy()],
        );
        let copy = run(
            &deps.exec,
            &["cp", &source.to_string_lossy(), &dest.to_string_lossy()],
        );
        report.push(
            "install ade binary",
            copy.code == 0,
            Some(if copy.code == 0 {
                dest.display().to_string()
            } else {
                copy.stderr.trim().to_string()
            }),
        );
    }

    // Install both app bundles.
    for app in [CONTROL_CENTER_APP, STATUS_APP] {
        let source = deps.repo_root.join("bundles").join(app);
        let dest = deps.apps_dir.join(app);
        let dest_str = dest.to_string_lossy().to_string();
        let apps_str = deps.apps_dir.to_string_lossy().to_string();
        if !dest_str.starts_with(&apps_str) || !dest_str.ends_with(".app") {
            report.push(
                &format!("install {app}"),
                false,
                Some(format!("refusing suspicious path: {dest_str}")),
            );
            continue;
        }
        let _ = run(&deps.exec, &["mkdir", "-p", &apps_str]);
        let _ = run(&deps.exec, &["rm", "-rf", &dest_str]);
        let copy = run(
            &deps.exec,
            &["cp", "-R", &source.to_string_lossy(), &dest_str],
        );
        report.push(
            &format!("install {app}"),
            copy.code == 0,
            Some(if copy.code == 0 {
                dest_str
            } else {
                copy.stderr.trim().to_string()
            }),
        );
    }

    // Write + (re)bootstrap the tray LaunchAgent.
    {
        let plist_path = deps.launch_agents_dir.join(format!("{STATUS_LABEL}.plist"));
        let log_keep = deps.ade_home.join("logs/.keep");
        let write_ok = write_ensured(&log_keep, "").is_ok()
            && write_ensured(&plist_path, &tray_plist(deps)).is_ok();
        report.push(
            "write tray plist",
            write_ok,
            Some(plist_path.display().to_string()),
        );
        if write_ok {
            let uid = resolve_uid(deps);
            let domain = format!("gui/{uid}");
            let target = format!("{domain}/{STATUS_LABEL}");
            let _ = run(&deps.exec, &["launchctl", "bootout", &target]);
            wait_for_service_gone(deps, &target);
            let boot = bootstrap_with_retry(deps, &domain, &plist_path);
            if boot.code == 0 {
                let _ = run(&deps.exec, &["launchctl", "kickstart", &target]);
                report.push("bootstrap tray agent", true, None);
            } else {
                report.push(
                    "bootstrap tray agent",
                    false,
                    Some(boot.stderr.trim().to_string()),
                );
            }
        }
    }
    report
}

pub fn gui_uninstall(deps: &InstallDeps) -> InstallReport {
    let mut report = InstallReport {
        ok: true,
        steps: Vec::new(),
    };
    let uid = resolve_uid(deps);
    let target = format!("gui/{uid}/{STATUS_LABEL}");
    let bootout = run(&deps.exec, &["launchctl", "bootout", &target]);
    report.push(
        "bootout tray agent",
        true,
        Some(if bootout.code == 0 {
            "removed".into()
        } else {
            "was not loaded".into()
        }),
    );
    let plist_path = deps.launch_agents_dir.join(format!("{STATUS_LABEL}.plist"));
    let _ = run(&deps.exec, &["rm", "-f", &plist_path.to_string_lossy()]);
    report.push("remove tray plist", true, None);
    for app in [CONTROL_CENTER_APP, STATUS_APP] {
        let dest = deps.apps_dir.join(app);
        let dest_str = dest.to_string_lossy().to_string();
        if !dest_str.starts_with(&deps.apps_dir.to_string_lossy().to_string())
            || !dest_str.ends_with(".app")
        {
            report.push(
                &format!("remove {app}"),
                false,
                Some(format!("refusing suspicious path: {dest_str}")),
            );
            continue;
        }
        let _ = run(&deps.exec, &["rm", "-rf", &dest_str]);
        report.push(&format!("remove {app}"), true, None);
    }
    report
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentStatus {
    pub label: String,
    pub loaded: bool,
    pub state: Option<String>,
    pub pid: Option<u32>,
}

pub fn gui_status(deps: &InstallDeps) -> Vec<AgentStatus> {
    let uid = resolve_uid(deps);
    let print = run(
        &deps.exec,
        &["launchctl", "print", &format!("gui/{uid}/{STATUS_LABEL}")],
    );
    if print.code != 0 {
        return vec![AgentStatus {
            label: STATUS_LABEL.into(),
            loaded: false,
            state: None,
            pid: None,
        }];
    }
    let state = print
        .stdout
        .lines()
        .find_map(|line| line.trim().strip_prefix("state = ").map(String::from));
    let pid = print.stdout.lines().find_map(|line| {
        line.trim()
            .strip_prefix("pid = ")
            .and_then(|value| value.trim().parse().ok())
    });
    vec![AgentStatus {
        label: STATUS_LABEL.into(),
        loaded: true,
        state,
        pid,
    }]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::make_temp_dir;
    use crate::types::ExecResult;
    use std::sync::{Arc, Mutex};

    fn recording_exec(fail_prefixes: &[&str]) -> (ExecFn, Arc<Mutex<Vec<String>>>) {
        let calls: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
        let fails: Vec<String> = fail_prefixes.iter().map(|s| s.to_string()).collect();
        let recorded = calls.clone();
        let exec: ExecFn = Arc::new(move |argv, _opts| {
            let joined = argv.join(" ");
            recorded.lock().expect("calls").push(joined.clone());
            if fails
                .iter()
                .any(|prefix| joined.starts_with(prefix.as_str()))
            {
                return ExecResult {
                    code: 1,
                    stdout: String::new(),
                    stderr: "simulated failure".into(),
                };
            }
            if joined == "id -u" {
                return ExecResult {
                    code: 0,
                    stdout: "501\n".into(),
                    stderr: String::new(),
                };
            }
            ExecResult {
                code: 0,
                stdout: String::new(),
                stderr: String::new(),
            }
        });
        (exec, calls)
    }

    fn deps_with(exec: ExecFn, root: &std::path::Path) -> InstallDeps {
        InstallDeps {
            exec,
            repo_root: root.join("repo"),
            apps_dir: root.join("Applications"),
            launch_agents_dir: root.join("LaunchAgents"),
            bin_dir: root.join("bin"),
            ade_home: root.join(".ade"),
            uid: None,
            skip_build: true,
            settle_interval: std::time::Duration::ZERO,
            home_dir: root.to_path_buf(),
        }
    }

    #[test]
    fn install_writes_plist_and_sequences_launchctl() {
        let root = make_temp_dir("install");
        let (exec, calls) = recording_exec(&[]);
        let deps = deps_with(exec, &root);
        let report = gui_install(&deps);
        assert!(report.ok, "{:?}", report.steps);
        let plist =
            std::fs::read_to_string(deps.launch_agents_dir.join(format!("{STATUS_LABEL}.plist")))
                .unwrap();
        assert!(plist.contains("<string>com.ade-bootstrapper.status</string>"));
        assert!(plist.contains("Contents/MacOS/ade-status"));
        assert!(plist.contains("/opt/homebrew/bin"));
        assert!(plist.contains("<key>SuccessfulExit</key>"));
        assert!(plist.contains("<key>RunAtLoad</key>"));
        let joined = calls.lock().expect("calls").join("\n");
        let bootout_at = joined.find("launchctl bootout").expect("bootout");
        let bootstrap_at = joined.find("launchctl bootstrap").expect("bootstrap");
        assert!(
            bootout_at < bootstrap_at,
            "bootout must precede bootstrap (idempotent reinstall)"
        );
        assert!(joined.contains("launchctl kickstart gui/501/com.ade-bootstrapper.status"));
        assert!(joined.contains(&format!(
            "cp -R {}",
            deps.repo_root
                .join("bundles")
                .join(CONTROL_CENTER_APP)
                .display()
        )));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn install_reports_bootstrap_failure() {
        let root = make_temp_dir("install-fail");
        let (exec, _calls) = recording_exec(&["launchctl bootstrap"]);
        let deps = deps_with(exec, &root);
        let report = gui_install(&deps);
        assert!(!report.ok);
        let failed = report
            .steps
            .iter()
            .find(|step| step.step == "bootstrap tray agent")
            .unwrap();
        assert!(!failed.ok);
        assert_eq!(failed.detail.as_deref(), Some("simulated failure"));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn install_waits_for_teardown_and_retries_transient_bootstrap_failure() {
        // Reproduces the real reinstall failure: `launchctl bootout` returns
        // while the job is still up, so the first bootstrap gets EIO.
        let root = make_temp_dir("install-retry");
        let calls: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
        let recorded = calls.clone();
        let exec: ExecFn = Arc::new(move |argv, _opts| {
            let joined = argv.join(" ");
            let seen_before = recorded
                .lock()
                .expect("calls")
                .iter()
                .filter(|call| call.starts_with(&joined))
                .count();
            recorded.lock().expect("calls").push(joined.clone());
            if joined == "id -u" {
                return ExecResult {
                    code: 0,
                    stdout: "501\n".into(),
                    stderr: String::new(),
                };
            }
            // Service still present on the first print, gone afterwards.
            if joined.starts_with("launchctl print") {
                return ExecResult {
                    code: if seen_before == 0 { 0 } else { 1 },
                    stdout: String::new(),
                    stderr: String::new(),
                };
            }
            // First bootstrap fails the way launchd actually fails here.
            if joined.starts_with("launchctl bootstrap") && seen_before == 0 {
                return ExecResult {
                    code: 5,
                    stdout: String::new(),
                    stderr: "Bootstrap failed: 5: Input/output error".into(),
                };
            }
            ExecResult {
                code: 0,
                stdout: String::new(),
                stderr: String::new(),
            }
        });
        let deps = deps_with(exec, &root);
        let report = gui_install(&deps);
        assert!(report.ok, "install should recover: {:?}", report.steps);

        let calls = calls.lock().expect("calls");
        let prints = calls
            .iter()
            .filter(|call| call.starts_with("launchctl print"))
            .count();
        let bootstraps = calls
            .iter()
            .filter(|call| call.starts_with("launchctl bootstrap"))
            .count();
        assert_eq!(prints, 2, "should poll until the old job is gone");
        assert_eq!(bootstraps, 2, "should retry the transient failure once");
        // Ordering matters: bootout → poll → bootstrap, never bootstrap first.
        let bootout_at = calls
            .iter()
            .position(|call| call.starts_with("launchctl bootout"))
            .unwrap();
        let first_print = calls
            .iter()
            .position(|call| call.starts_with("launchctl print"))
            .unwrap();
        let first_bootstrap = calls
            .iter()
            .position(|call| call.starts_with("launchctl bootstrap"))
            .unwrap();
        assert!(bootout_at < first_print && first_print < first_bootstrap);
        drop(calls);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn uninstall_removes_agent_plist_and_apps() {
        let root = make_temp_dir("uninstall");
        let (exec, calls) = recording_exec(&[]);
        let deps = deps_with(exec, &root);
        let report = gui_uninstall(&deps);
        assert!(report.ok);
        let joined = calls.lock().expect("calls").join("\n");
        assert!(joined.contains("launchctl bootout gui/501/com.ade-bootstrapper.status"));
        assert!(joined.contains(&format!(
            "rm -rf {}",
            deps.apps_dir.join(CONTROL_CENTER_APP).display()
        )));
        assert!(joined.contains(&format!(
            "rm -rf {}",
            deps.apps_dir.join(STATUS_APP).display()
        )));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn status_parses_launchctl_print() {
        let root = make_temp_dir("status");
        let running: ExecFn = Arc::new(|argv, _opts| {
            let joined = argv.join(" ");
            if joined == "id -u" {
                return ExecResult {
                    code: 0,
                    stdout: "501\n".into(),
                    stderr: String::new(),
                };
            }
            ExecResult {
                code: 0,
                stdout: "\tstate = running\n\tpid = 4242\n".into(),
                stderr: String::new(),
            }
        });
        let deps = deps_with(running, &root);
        let agents = gui_status(&deps);
        assert!(agents[0].loaded);
        assert_eq!(agents[0].state.as_deref(), Some("running"));
        assert_eq!(agents[0].pid, Some(4242));

        let missing: ExecFn = Arc::new(|argv, _opts| {
            let joined = argv.join(" ");
            if joined == "id -u" {
                return ExecResult {
                    code: 0,
                    stdout: "501\n".into(),
                    stderr: String::new(),
                };
            }
            ExecResult {
                code: 113,
                stdout: String::new(),
                stderr: "Could not find service".into(),
            }
        });
        let deps_missing = deps_with(missing, &root);
        let agents_missing = gui_status(&deps_missing);
        assert!(!agents_missing[0].loaded);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn agent_path_contains_package_manager_dirs_without_duplicates() {
        let path = agent_path(
            Path::new("/Users/tester"),
            Path::new("/Users/tester/.local/bin"),
        );
        assert!(path.contains("/opt/homebrew/bin"));
        assert!(path.contains("/Users/tester/.npm-global/bin"));
        assert!(path.contains("/usr/bin"));
        let occurrences = path.matches("/Users/tester/.local/bin").count();
        assert_eq!(occurrences, 1);
    }
}
