//! Capability inventory — every tool and harness the Control Center manages
//! (ISC-167/168/169/170). Recipes are STATIC and verified; request/UI input
//! never reaches exec argv — ids resolve against this table and actions
//! against a closed enum (ISC-176). All subprocess work is argv arrays.

use crate::types::{ExecFn, ExecOpts, ExecResult, Finding, WhichFn};
use std::sync::mpsc;
use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CapabilityKind {
    Tool,
    Harness,
}

impl CapabilityKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            CapabilityKind::Tool => "tool",
            CapabilityKind::Harness => "harness",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LifecycleMethod {
    Brew,
    BrewCask,
    Npm,
    Manual,
}

impl LifecycleMethod {
    pub fn as_str(&self) -> &'static str {
        match self {
            LifecycleMethod::Brew => "brew",
            LifecycleMethod::BrewCask => "brew-cask",
            LifecycleMethod::Npm => "npm",
            LifecycleMethod::Manual => "manual",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LifecycleAction {
    Install,
    Uninstall,
    Reinstall,
    Update,
}

pub const LIFECYCLE_ACTIONS: [LifecycleAction; 4] = [
    LifecycleAction::Install,
    LifecycleAction::Uninstall,
    LifecycleAction::Reinstall,
    LifecycleAction::Update,
];

impl LifecycleAction {
    pub fn as_str(&self) -> &'static str {
        match self {
            LifecycleAction::Install => "install",
            LifecycleAction::Uninstall => "uninstall",
            LifecycleAction::Reinstall => "reinstall",
            LifecycleAction::Update => "update",
        }
    }
    pub fn parse(raw: &str) -> Option<LifecycleAction> {
        LIFECYCLE_ACTIONS
            .iter()
            .copied()
            .find(|action| action.as_str() == raw)
    }
}

pub struct CapabilityDef {
    pub id: &'static str,
    pub name: &'static str,
    pub kind: CapabilityKind,
    /// Capability group this tool provides (taxonomy id — see CAPABILITY_GROUPS).
    pub capability: &'static str,
    pub description: &'static str,
    /// CLI names probed for presence (first `which` hit wins).
    pub bins: &'static [&'static str],
    /// Version argv appended to the resolved binary.
    pub version_args: &'static [&'static str],
    pub method: LifecycleMethod,
    /// brew formula / cask / npm package (non-manual only).
    pub pkg: Option<&'static str>,
    /// Install guidance for `manual` capabilities.
    pub guidance: Option<&'static str>,
    /// Exact process names (pgrep -x) that mean this capability is running.
    pub process_names: &'static [&'static str],
}

const V: &[&str] = &["--version"];

pub const CAPABILITIES: [CapabilityDef; 17] = [
    // ── Integrated tools (mirrors INTEGRATED_TOOLS — asserted by test) ──
    CapabilityDef { id: "trufflehog", name: "TruffleHog", kind: CapabilityKind::Tool, capability: "secret-scanning", description: "Secret scanner wired into the pre-commit boundary by the secrets module", bins: &["trufflehog"], version_args: V, method: LifecycleMethod::Brew, pkg: Some("trufflehog"), guidance: None, process_names: &[] },
    CapabilityDef { id: "pre-commit", name: "pre-commit", kind: CapabilityKind::Tool, capability: "hook-orchestration", description: "Git hook framework the secrets module uses when present", bins: &["pre-commit"], version_args: V, method: LifecycleMethod::Brew, pkg: Some("pre-commit"), guidance: None, process_names: &[] },
    CapabilityDef { id: "gitleaks", name: "Gitleaks", kind: CapabilityKind::Tool, capability: "secret-scanning", description: "Complementary secret scanner (detected, optional)", bins: &["gitleaks"], version_args: &["version"], method: LifecycleMethod::Brew, pkg: Some("gitleaks"), guidance: None, process_names: &[] },
    CapabilityDef { id: "rtk", name: "RTK", kind: CapabilityKind::Tool, capability: "token-efficiency", description: "Token-efficiency proxy at the shell boundary (token-efficiency module)", bins: &["rtk"], version_args: V, method: LifecycleMethod::Brew, pkg: Some("rtk"), guidance: None, process_names: &[] },
    CapabilityDef { id: "ocean", name: "OCEAN", kind: CapabilityKind::Tool, capability: "repo-hygiene", description: "Git & repository hygiene hardening (git-hygiene module)", bins: &["ocean"], version_args: V, method: LifecycleMethod::Manual, pkg: None, guidance: Some("Install from https://github.com/grcengineering/OCEAN (installer script / release binary)"), process_names: &[] },
    CapabilityDef { id: "nono", name: "nono", kind: CapabilityKind::Tool, capability: "sandboxing", description: "Kernel-enforced sandboxing for terminal agents (sandbox module)", bins: &["nono"], version_args: V, method: LifecycleMethod::Manual, pkg: None, guidance: Some("Install from https://nono.sh"), process_names: &[] },
    CapabilityDef { id: "osv-scanner", name: "OSV-Scanner", kind: CapabilityKind::Tool, capability: "dependency-scanning", description: "Dependency vulnerability scanner (supply-chain module)", bins: &["osv-scanner"], version_args: V, method: LifecycleMethod::Brew, pkg: Some("osv-scanner"), guidance: None, process_names: &[] },
    CapabilityDef { id: "openwiki", name: "OpenWiki", kind: CapabilityKind::Tool, capability: "codebase-wiki", description: "Auto-maintained codebase wiki + opt-in Personal Brain (context module)", bins: &["openwiki"], version_args: V, method: LifecycleMethod::Npm, pkg: Some("openwiki"), guidance: None, process_names: &[] },
    CapabilityDef { id: "cocoindex", name: "CocoIndex", kind: CapabilityKind::Tool, capability: "semantic-search", description: "AST-based semantic code search framework (context module)", bins: &["cocoindex"], version_args: V, method: LifecycleMethod::Manual, pkg: None, guidance: Some("Install from https://cocoindex.io (Python framework — pip/uv)"), process_names: &[] },
    CapabilityDef { id: "ccc", name: "CocoIndex Code CLI (ccc)", kind: CapabilityKind::Tool, capability: "semantic-search", description: "cocoindex-code CLI — alternate CocoIndex entry point (context module)", bins: &["ccc"], version_args: V, method: LifecycleMethod::Manual, pkg: None, guidance: Some("Install from https://github.com/cocoindex-io/cocoindex-code"), process_names: &[] },
    // ── Harness CLIs (mirrors HARNESS_ADAPTERS — asserted by test) ──
    CapabilityDef { id: "claude-code", name: "Claude Code", kind: CapabilityKind::Harness, capability: "coding-harness", description: "Anthropic's coding harness CLI", bins: &["claude"], version_args: V, method: LifecycleMethod::Npm, pkg: Some("@anthropic-ai/claude-code"), guidance: None, process_names: &["claude"] },
    CapabilityDef { id: "codex", name: "Codex", kind: CapabilityKind::Harness, capability: "coding-harness", description: "OpenAI's coding harness CLI", bins: &["codex"], version_args: V, method: LifecycleMethod::BrewCask, pkg: Some("codex"), guidance: None, process_names: &["codex"] },
    CapabilityDef { id: "cursor", name: "Cursor", kind: CapabilityKind::Harness, capability: "coding-harness", description: "Cursor editor + CLI", bins: &["cursor"], version_args: V, method: LifecycleMethod::BrewCask, pkg: Some("cursor"), guidance: None, process_names: &["Cursor"] },
    CapabilityDef { id: "opencode", name: "OpenCode", kind: CapabilityKind::Harness, capability: "coding-harness", description: "OpenCode coding harness CLI", bins: &["opencode"], version_args: V, method: LifecycleMethod::Brew, pkg: Some("opencode"), guidance: None, process_names: &["opencode"] },
    CapabilityDef { id: "antigravity", name: "Antigravity", kind: CapabilityKind::Harness, capability: "coding-harness", description: "Google's agentic development environment", bins: &["antigravity"], version_args: V, method: LifecycleMethod::BrewCask, pkg: Some("antigravity"), guidance: None, process_names: &["Antigravity"] },
    CapabilityDef { id: "hermes", name: "Hermes", kind: CapabilityKind::Harness, capability: "coding-harness", description: "Nous Research's Hermes agent CLI", bins: &["hermes"], version_args: V, method: LifecycleMethod::Manual, pkg: None, guidance: Some("Install via the official Hermes installer (https://hermes.nousresearch.com)"), process_names: &["hermes"] },
    CapabilityDef { id: "pi", name: "Pi", kind: CapabilityKind::Harness, capability: "coding-harness", description: "Pi.dev coding agent CLI", bins: &["pi"], version_args: V, method: LifecycleMethod::Npm, pkg: Some("@earendil-works/pi-coding-agent"), guidance: None, process_names: &["pi"] },
];

/// The capability taxonomy — the durable functions ADE manages. Tools are
/// swappable PROVIDERS of these (owner intent: e.g. Gitleaks ⇄ TruffleHog
/// under Secret Scanning). `why` is the user-facing explainer: what problem
/// this capability solves.
pub struct CapabilityGroup {
    pub id: &'static str,
    pub name: &'static str,
    pub why: &'static str,
}

pub const CAPABILITY_GROUPS: [CapabilityGroup; 9] = [
    CapabilityGroup {
        id: "secret-scanning",
        name: "Secret Scanning",
        why: "Leaked credentials in commits are among the most exploited failure modes of AI-assisted coding — agents paste keys into code, configs, and logs. Scanning at the commit boundary catches them before they enter history that is painful to scrub.",
    },
    CapabilityGroup {
        id: "hook-orchestration",
        name: "Git Hook Orchestration",
        why: "Guardrails only protect you if they actually run. A hook framework wires scanners and checks into every commit, on every clone, without relying on anyone remembering to run them.",
    },
    CapabilityGroup {
        id: "dependency-scanning",
        name: "Dependency Vulnerability Scanning",
        why: "AI agents add dependencies fast — including typosquatted, hallucinated, or known-vulnerable packages. Scanning against the OSV database catches known-bad versions before they ship.",
    },
    CapabilityGroup {
        id: "sandboxing",
        name: "Execution Sandboxing",
        why: "Coding agents run shell commands with your full user privileges. Kernel-level sandboxing bounds the blast radius: filesystem scopes, network egress control, and rollback.",
    },
    CapabilityGroup {
        id: "repo-hygiene",
        name: "Repository Hygiene",
        why: "Agents can force-push, rewrite history, or commit to protected branches at machine speed. Hardened git configuration and branch policy make repository damage structurally harder.",
    },
    CapabilityGroup {
        id: "token-efficiency",
        name: "Token Efficiency",
        why: "Terminal output, logs, and file listings burn context tokens that crowd out actual reasoning. Compressing at the shell boundary cuts token spend heavily without losing signal.",
    },
    CapabilityGroup {
        id: "codebase-wiki",
        name: "Codebase Wiki",
        why: "Agents re-derive your architecture from scratch every session. An auto-maintained wiki gives them (and you) a current, navigable map — fewer whole-tree scans, better decisions.",
    },
    CapabilityGroup {
        id: "semantic-search",
        name: "Semantic Code Search",
        why: "Grep finds strings; agents need concepts. AST-based semantic indexing retrieves code by meaning, shrinking context waste and wrong-file edits.",
    },
    CapabilityGroup {
        id: "coding-harness",
        name: "Coding Harness",
        why: "The agent itself. ADE treats harnesses as swappable: one governed environment with consistent guardrails and instructions, whichever CLI you run today.",
    },
];

pub fn get_group(id: &str) -> Option<&'static CapabilityGroup> {
    CAPABILITY_GROUPS.iter().find(|group| group.id == id)
}

pub fn get_capability(id: &str) -> Option<&'static CapabilityDef> {
    CAPABILITIES.iter().find(|def| def.id == id)
}

/// Argv sequences for a lifecycle action; None when the method is manual.
pub fn action_argvs(def: &CapabilityDef, action: LifecycleAction) -> Option<Vec<Vec<String>>> {
    let pkg = def.pkg?;
    let argv: Vec<String> = match (def.method, action) {
        (LifecycleMethod::Manual, _) => return None,
        (LifecycleMethod::Brew, LifecycleAction::Install) => {
            vec!["brew".into(), "install".into(), pkg.into()]
        }
        (LifecycleMethod::Brew, LifecycleAction::Uninstall) => {
            vec!["brew".into(), "uninstall".into(), pkg.into()]
        }
        (LifecycleMethod::Brew, LifecycleAction::Reinstall) => {
            vec!["brew".into(), "reinstall".into(), pkg.into()]
        }
        (LifecycleMethod::Brew, LifecycleAction::Update) => {
            vec!["brew".into(), "upgrade".into(), pkg.into()]
        }
        (LifecycleMethod::BrewCask, LifecycleAction::Install) => {
            vec!["brew".into(), "install".into(), "--cask".into(), pkg.into()]
        }
        (LifecycleMethod::BrewCask, LifecycleAction::Uninstall) => vec![
            "brew".into(),
            "uninstall".into(),
            "--cask".into(),
            pkg.into(),
        ],
        (LifecycleMethod::BrewCask, LifecycleAction::Reinstall) => vec![
            "brew".into(),
            "reinstall".into(),
            "--cask".into(),
            pkg.into(),
        ],
        (LifecycleMethod::BrewCask, LifecycleAction::Update) => {
            vec!["brew".into(), "upgrade".into(), "--cask".into(), pkg.into()]
        }
        (LifecycleMethod::Npm, LifecycleAction::Install | LifecycleAction::Reinstall) => {
            vec!["npm".into(), "install".into(), "-g".into(), pkg.into()]
        }
        (LifecycleMethod::Npm, LifecycleAction::Uninstall) => {
            vec!["npm".into(), "uninstall".into(), "-g".into(), pkg.into()]
        }
        (LifecycleMethod::Npm, LifecycleAction::Update) => vec![
            "npm".into(),
            "install".into(),
            "-g".into(),
            format!("{pkg}@latest"),
        ],
    };
    Some(vec![argv])
}

/// Argv for the on-demand latest-version lookup; None for manual capabilities.
pub fn latest_version_argv(def: &CapabilityDef) -> Option<Vec<String>> {
    let pkg = def.pkg?;
    Some(match def.method {
        LifecycleMethod::Manual => return None,
        LifecycleMethod::Brew => vec!["brew".into(), "info".into(), "--json=v2".into(), pkg.into()],
        LifecycleMethod::BrewCask => vec![
            "brew".into(),
            "info".into(),
            "--json=v2".into(),
            "--cask".into(),
            pkg.into(),
        ],
        LifecycleMethod::Npm => vec!["npm".into(), "view".into(), pkg.into(), "version".into()],
    })
}

/// Parse the latest-version lookup output. Never fails hard.
pub fn parse_latest_version(def: &CapabilityDef, result: &ExecResult) -> Option<String> {
    if result.code != 0 {
        return None;
    }
    match def.method {
        LifecycleMethod::Npm => {
            let line = result.stdout.trim().split('\n').next().unwrap_or("").trim();
            (!line.is_empty()).then(|| line.to_string())
        }
        LifecycleMethod::Brew => {
            let parsed: serde_json::Value = serde_json::from_str(&result.stdout).ok()?;
            parsed["formulae"][0]["versions"]["stable"]
                .as_str()
                .map(String::from)
        }
        LifecycleMethod::BrewCask => {
            let parsed: serde_json::Value = serde_json::from_str(&result.stdout).ok()?;
            parsed["casks"][0]["version"].as_str().map(String::from)
        }
        LifecycleMethod::Manual => None,
    }
}

/// Wrap an ExecFn with a per-call timeout so a hung probe cannot hang the GUI
/// or tray (ISC-170). The subprocess thread is detached on timeout.
pub fn with_timeout(exec: ExecFn, timeout: Duration) -> ExecFn {
    std::sync::Arc::new(move |argv: &[String], opts: &ExecOpts| -> ExecResult {
        let (sender, receiver) = mpsc::channel::<ExecResult>();
        let exec_inner = exec.clone();
        let argv_owned: Vec<String> = argv.to_vec();
        let opts_owned = ExecOpts {
            cwd: opts.cwd.clone(),
            stdin: opts.stdin.clone(),
        };
        std::thread::spawn(move || {
            let result = exec_inner(&argv_owned, &opts_owned);
            let _ = sender.send(result);
        });
        receiver
            .recv_timeout(timeout)
            .unwrap_or_else(|_| ExecResult {
                code: 124,
                stdout: String::new(),
                stderr: format!("probe timed out after {}ms", timeout.as_millis()),
            })
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LastJobSummary {
    pub action: String,
    pub failed: bool,
    pub exit_code: Option<i32>,
    pub log_tail: String,
}

#[derive(Debug, Clone)]
pub struct CapabilityStatus {
    pub id: String,
    pub name: String,
    pub kind: CapabilityKind,
    /// Capability-group id (taxonomy).
    pub capability: String,
    pub description: String,
    pub method: LifecycleMethod,
    pub pkg: Option<String>,
    pub guidance: Option<String>,
    pub installed: bool,
    pub path: Option<String>,
    pub version: Option<String>,
    /// Only Some for capabilities with a process signature.
    pub running: Option<bool>,
    pub enabled: bool,
    pub latest_version: Option<String>,
    pub update_available: bool,
    pub issues: Vec<Finding>,
}

pub struct DetectOptions<'a> {
    pub exec: ExecFn,
    pub which: WhichFn,
    pub disabled: &'a std::collections::BTreeSet<String>,
    /// Last finished job per capability (error surfacing).
    pub last_jobs: &'a std::collections::BTreeMap<String, LastJobSummary>,
    /// Cached latest versions from an explicit refresh (never auto-populated).
    pub latest_versions: &'a std::collections::BTreeMap<String, String>,
    /// Probe running state via pgrep (disable in tests).
    pub probe_running: bool,
    pub probe_timeout: Duration,
}

fn detect_one(def: &'static CapabilityDef, opts: &DetectOptions<'_>) -> CapabilityStatus {
    let enabled = !opts.disabled.contains(def.id);
    let mut path: Option<String> = None;
    for bin in def.bins {
        if let Some(hit) = (opts.which)(bin) {
            path = Some(hit);
            break;
        }
    }
    let installed = path.is_some();
    let mut version: Option<String> = None;
    if let Some(bin_path) = &path {
        let mut argv: Vec<String> = vec![bin_path.clone()];
        argv.extend(def.version_args.iter().map(|arg| arg.to_string()));
        let result = (opts.exec)(&argv, &ExecOpts::default());
        let source = if result.stdout.trim().is_empty() {
            &result.stderr
        } else {
            &result.stdout
        };
        if result.code == 0 {
            version = crate::context::parse_version_line(source);
        }
    }
    let mut running: Option<bool> = None;
    if installed && !def.process_names.is_empty() && opts.probe_running {
        let mut is_running = false;
        for process in def.process_names {
            let argv: Vec<String> = vec!["pgrep".into(), "-x".into(), process.to_string()];
            let result = (opts.exec)(&argv, &ExecOpts::default());
            if result.code == 0 && !result.stdout.trim().is_empty() {
                is_running = true;
                break;
            }
        }
        running = Some(is_running);
    }
    let latest_version = opts.latest_versions.get(def.id).cloned();
    let update_available = match (&latest_version, &version) {
        (Some(latest), Some(current)) => !current.contains(latest.as_str()),
        _ => false,
    };

    let mut issues: Vec<Finding> = Vec::new();
    if !enabled {
        issues.push(Finding::info(
            "disabled in ADE GUI (machine-level preference — excluded from health rollups; project policy lives in each repo's ade.json)",
        ));
    } else {
        if !installed {
            let remediation = match action_argvs(def, LifecycleAction::Install) {
                Some(argvs) => format!(
                    "Use Install here, or run: {}",
                    argvs
                        .iter()
                        .map(|argv| argv.join(" "))
                        .collect::<Vec<_>>()
                        .join(" && ")
                ),
                None => def.guidance.map(String::from).unwrap_or_else(|| {
                    "no automated install recipe — see the project's documentation".to_string()
                }),
            };
            issues.push(Finding::warn(
                format!("{} is not installed", def.name),
                remediation,
            ));
        }
        if installed && version.is_none() {
            issues.push(Finding::error_with(
                format!("{} is on PATH but its version probe failed", def.name),
                format!(
                    "Run `{} {}` in a terminal to inspect",
                    path.as_deref().unwrap_or(def.bins[0]),
                    def.version_args.join(" ")
                ),
            ));
        }
        if let Some(last) = opts.last_jobs.get(def.id) {
            if last.failed {
                issues.push(Finding::error_with(
                    format!(
                        "last {} failed (exit {})",
                        last.action,
                        last.exit_code
                            .map(|code| code.to_string())
                            .unwrap_or_else(|| "?".into())
                    ),
                    if last.log_tail.is_empty() {
                        "open the job log for details".to_string()
                    } else {
                        last.log_tail.clone()
                    },
                ));
            }
        }
        if update_available {
            issues.push(Finding {
                level: crate::types::FindingLevel::Info,
                message: format!(
                    "update available: {} → {}",
                    version.as_deref().unwrap_or("installed"),
                    latest_version.as_deref().unwrap_or("")
                ),
                remediation: Some("Use Update here to move to the latest version".to_string()),
            });
        }
    }

    CapabilityStatus {
        id: def.id.to_string(),
        name: def.name.to_string(),
        kind: def.kind,
        capability: def.capability.to_string(),
        description: def.description.to_string(),
        method: def.method,
        pkg: def.pkg.map(String::from),
        guidance: def.guidance.map(String::from),
        installed,
        path,
        version,
        running,
        enabled,
        latest_version,
        update_available,
        issues,
    }
}

/// Detect all capabilities concurrently; each probe is timeout-bounded (ISC-170).
pub fn detect_capabilities(opts: &DetectOptions<'_>) -> Vec<CapabilityStatus> {
    let bounded = DetectOptions {
        exec: with_timeout(opts.exec.clone(), opts.probe_timeout),
        which: opts.which.clone(),
        disabled: opts.disabled,
        last_jobs: opts.last_jobs,
        latest_versions: opts.latest_versions,
        probe_running: opts.probe_running,
        probe_timeout: opts.probe_timeout,
    };
    std::thread::scope(|scope| {
        let handles: Vec<_> = CAPABILITIES
            .iter()
            .map(|def| scope.spawn(|| detect_one(def, &bounded)))
            .collect();
        handles
            .into_iter()
            .map(|handle| handle.join().expect("detect thread"))
            .collect()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::harness::adapters::HARNESS_ADAPTERS;
    use crate::testutil::{fake_exec, fake_which};
    use crate::types::{FindingLevel, INTEGRATED_TOOLS};
    use std::collections::{BTreeMap, BTreeSet};

    fn base_opts<'a>(
        disabled: &'a BTreeSet<String>,
        last_jobs: &'a BTreeMap<String, LastJobSummary>,
        latest: &'a BTreeMap<String, String>,
        exec: ExecFn,
        which: WhichFn,
    ) -> DetectOptions<'a> {
        DetectOptions {
            exec,
            which,
            disabled,
            last_jobs,
            latest_versions: latest,
            probe_running: false,
            probe_timeout: Duration::from_secs(2),
        }
    }

    #[test]
    fn inventory_covers_all_tools_and_harnesses_with_unique_ids() {
        let tool_ids: Vec<&str> = CAPABILITIES
            .iter()
            .filter(|def| def.kind == CapabilityKind::Tool)
            .map(|def| def.id)
            .collect();
        for tool in INTEGRATED_TOOLS {
            assert!(tool_ids.contains(&tool), "missing tool capability: {tool}");
        }
        for adapter in &HARNESS_ADAPTERS {
            let def = get_capability(adapter.id).expect(adapter.id);
            assert_eq!(def.kind, CapabilityKind::Harness);
            assert_eq!(def.bins, adapter.cli_names);
        }
        let mut ids: Vec<&str> = CAPABILITIES.iter().map(|def| def.id).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), CAPABILITIES.len());
        assert!(CAPABILITIES.len() >= 17);
        for def in &CAPABILITIES {
            match def.method {
                LifecycleMethod::Manual => {
                    assert!(def.guidance.is_some(), "{} needs guidance", def.id)
                }
                _ => assert!(def.pkg.is_some(), "{} needs pkg", def.id),
            }
        }
    }

    #[test]
    fn action_recipes_per_method() {
        let brew = get_capability("pre-commit").unwrap();
        assert_eq!(
            action_argvs(brew, LifecycleAction::Install).unwrap(),
            vec![vec![
                "brew".to_string(),
                "install".into(),
                "pre-commit".into()
            ]]
        );
        assert_eq!(
            action_argvs(brew, LifecycleAction::Update).unwrap(),
            vec![vec![
                "brew".to_string(),
                "upgrade".into(),
                "pre-commit".into()
            ]]
        );
        let cask = get_capability("codex").unwrap();
        assert_eq!(
            action_argvs(cask, LifecycleAction::Uninstall).unwrap(),
            vec![vec![
                "brew".to_string(),
                "uninstall".into(),
                "--cask".into(),
                "codex".into()
            ]]
        );
        let npm = get_capability("openwiki").unwrap();
        assert_eq!(
            action_argvs(npm, LifecycleAction::Update).unwrap(),
            vec![vec![
                "npm".to_string(),
                "install".into(),
                "-g".into(),
                "openwiki@latest".into()
            ]]
        );
        assert_eq!(
            action_argvs(npm, LifecycleAction::Reinstall).unwrap(),
            action_argvs(npm, LifecycleAction::Install).unwrap()
        );
        let manual = get_capability("ocean").unwrap();
        for action in LIFECYCLE_ACTIONS {
            assert!(action_argvs(manual, action).is_none());
        }
    }

    #[test]
    fn latest_version_lookup_and_parsing() {
        let brew = get_capability("pre-commit").unwrap();
        assert_eq!(
            latest_version_argv(brew).unwrap(),
            vec![
                "brew".to_string(),
                "info".into(),
                "--json=v2".into(),
                "pre-commit".into()
            ]
        );
        assert!(latest_version_argv(get_capability("ocean").unwrap()).is_none());
        let ok = ExecResult {
            code: 0,
            stdout: r#"{"formulae":[{"versions":{"stable":"4.6.1"}}]}"#.into(),
            stderr: String::new(),
        };
        assert_eq!(parse_latest_version(brew, &ok).as_deref(), Some("4.6.1"));
        let cask = get_capability("codex").unwrap();
        let cask_ok = ExecResult {
            code: 0,
            stdout: r#"{"casks":[{"version":"0.145.0"}]}"#.into(),
            stderr: String::new(),
        };
        assert_eq!(
            parse_latest_version(cask, &cask_ok).as_deref(),
            Some("0.145.0")
        );
        let npm = get_capability("openwiki").unwrap();
        let npm_ok = ExecResult {
            code: 0,
            stdout: "0.2.3\n".into(),
            stderr: String::new(),
        };
        assert_eq!(parse_latest_version(npm, &npm_ok).as_deref(), Some("0.2.3"));
        let bad = ExecResult {
            code: 1,
            stdout: String::new(),
            stderr: "boom".into(),
        };
        assert!(parse_latest_version(brew, &bad).is_none());
        let garbage = ExecResult {
            code: 0,
            stdout: "not json".into(),
            stderr: String::new(),
        };
        assert!(parse_latest_version(brew, &garbage).is_none());
    }

    #[test]
    fn with_timeout_bounds_hung_probes() {
        let hung: ExecFn = std::sync::Arc::new(|_argv, _opts| {
            std::thread::sleep(Duration::from_secs(60));
            ExecResult {
                code: 0,
                stdout: String::new(),
                stderr: String::new(),
            }
        });
        let bounded = with_timeout(hung, Duration::from_millis(50));
        let started = std::time::Instant::now();
        let result = bounded(&["sleep".to_string()], &ExecOpts::default());
        assert_eq!(result.code, 124);
        assert!(result.stderr.contains("timed out"));
        assert!(started.elapsed() < Duration::from_secs(5));
        let fast = with_timeout(
            fake_exec(&[("echo", (0, "hi", ""))]),
            Duration::from_secs(2),
        );
        assert_eq!(
            fast(&["echo".to_string()], &ExecOpts::default()).stdout,
            "hi"
        );
    }

    #[test]
    fn detection_issue_model() {
        let disabled = BTreeSet::new();
        let jobs = BTreeMap::new();
        let latest = BTreeMap::new();
        let caps = detect_capabilities(&base_opts(
            &disabled,
            &jobs,
            &latest,
            fake_exec(&[(
                "/fake/bin/trufflehog --version",
                (0, "trufflehog 3.94.3\n", ""),
            )]),
            fake_which(&["trufflehog"]),
        ));
        assert_eq!(caps.len(), CAPABILITIES.len());
        let trufflehog = caps.iter().find(|cap| cap.id == "trufflehog").unwrap();
        assert!(trufflehog.installed);
        assert_eq!(trufflehog.version.as_deref(), Some("trufflehog 3.94.3"));
        assert!(trufflehog.issues.is_empty());
        let pre_commit = caps.iter().find(|cap| cap.id == "pre-commit").unwrap();
        assert!(!pre_commit.installed);
        let warn = pre_commit
            .issues
            .iter()
            .find(|issue| issue.level == FindingLevel::Warn)
            .unwrap();
        assert!(warn.message.contains("not installed"));
        assert!(warn
            .remediation
            .as_deref()
            .unwrap()
            .contains("brew install pre-commit"));
        let nono = caps.iter().find(|cap| cap.id == "nono").unwrap();
        assert!(nono.issues.iter().any(|issue| issue
            .remediation
            .as_deref()
            .unwrap_or("")
            .contains("nono.sh")));
    }

    #[test]
    fn disabled_capability_is_info_only() {
        let mut disabled = BTreeSet::new();
        disabled.insert("pre-commit".to_string());
        let jobs = BTreeMap::new();
        let latest = BTreeMap::new();
        let caps = detect_capabilities(&base_opts(
            &disabled,
            &jobs,
            &latest,
            fake_exec(&[]),
            fake_which(&[]),
        ));
        let pre_commit = caps.iter().find(|cap| cap.id == "pre-commit").unwrap();
        assert!(!pre_commit.enabled);
        assert_eq!(pre_commit.issues.len(), 1);
        assert_eq!(pre_commit.issues[0].level, FindingLevel::Info);
        assert!(pre_commit.issues[0].message.contains("disabled in ADE GUI"));
    }

    #[test]
    fn version_probe_failure_is_error_and_failed_job_surfaces() {
        let disabled = BTreeSet::new();
        let mut jobs = BTreeMap::new();
        jobs.insert(
            "pre-commit".to_string(),
            LastJobSummary {
                action: "install".into(),
                failed: true,
                exit_code: Some(1),
                log_tail: "Error: some brew failure".into(),
            },
        );
        let latest = BTreeMap::new();
        let caps = detect_capabilities(&base_opts(
            &disabled,
            &jobs,
            &latest,
            fake_exec(&[("/fake/bin/trufflehog --version", (1, "", "segfault"))]),
            fake_which(&["trufflehog"]),
        ));
        let trufflehog = caps.iter().find(|cap| cap.id == "trufflehog").unwrap();
        assert!(trufflehog.version.is_none());
        assert!(trufflehog
            .issues
            .iter()
            .any(|issue| issue.level == FindingLevel::Error));
        let pre_commit = caps.iter().find(|cap| cap.id == "pre-commit").unwrap();
        let job_error = pre_commit
            .issues
            .iter()
            .find(|issue| issue.message.contains("last install failed"))
            .unwrap();
        assert_eq!(job_error.level, FindingLevel::Error);
        assert!(job_error
            .remediation
            .as_deref()
            .unwrap()
            .contains("brew failure"));
    }

    #[test]
    fn update_available_only_when_versions_differ() {
        let disabled = BTreeSet::new();
        let jobs = BTreeMap::new();
        let mut latest = BTreeMap::new();
        latest.insert("trufflehog".to_string(), "3.95.9".to_string());
        let stale = detect_capabilities(&base_opts(
            &disabled,
            &jobs,
            &latest,
            fake_exec(&[(
                "/fake/bin/trufflehog --version",
                (0, "trufflehog 3.94.3\n", ""),
            )]),
            fake_which(&["trufflehog"]),
        ));
        let cap = stale.iter().find(|cap| cap.id == "trufflehog").unwrap();
        assert!(cap.update_available);
        assert!(cap
            .issues
            .iter()
            .any(|issue| issue.message.contains("update available")));
        let current = detect_capabilities(&base_opts(
            &disabled,
            &jobs,
            &latest,
            fake_exec(&[(
                "/fake/bin/trufflehog --version",
                (0, "trufflehog 3.95.9\n", ""),
            )]),
            fake_which(&["trufflehog"]),
        ));
        assert!(
            !current
                .iter()
                .find(|cap| cap.id == "trufflehog")
                .unwrap()
                .update_available
        );
    }

    #[test]
    fn running_detection_via_pgrep_when_enabled() {
        let disabled = BTreeSet::new();
        let jobs = BTreeMap::new();
        let latest = BTreeMap::new();
        let mut opts = base_opts(
            &disabled,
            &jobs,
            &latest,
            fake_exec(&[
                ("/fake/bin/claude --version", (0, "2.1.220\n", "")),
                ("/fake/bin/codex --version", (0, "0.145.0\n", "")),
                ("pgrep -x claude", (0, "1234\n", "")),
                ("pgrep -x codex", (1, "", "")),
            ]),
            fake_which(&["claude", "codex", "trufflehog"]),
        );
        opts.probe_running = true;
        let caps = detect_capabilities(&opts);
        assert_eq!(
            caps.iter()
                .find(|cap| cap.id == "claude-code")
                .unwrap()
                .running,
            Some(true)
        );
        assert_eq!(
            caps.iter().find(|cap| cap.id == "codex").unwrap().running,
            Some(false)
        );
        // Tools without a process signature never report running.
        assert_eq!(
            caps.iter()
                .find(|cap| cap.id == "trufflehog")
                .unwrap()
                .running,
            None
        );
    }

    #[test]
    fn capability_taxonomy_integrity() {
        // Every tool/harness maps to a real group; every group has providers.
        let mut seen_groups = std::collections::BTreeSet::new();
        for def in &CAPABILITIES {
            let group = get_group(def.capability)
                .unwrap_or_else(|| panic!("{} maps to unknown group {}", def.id, def.capability));
            assert!(!group.why.trim().is_empty(), "{} why is empty", group.id);
            assert!(!group.name.trim().is_empty());
            seen_groups.insert(def.capability);
        }
        for group in &CAPABILITY_GROUPS {
            assert!(
                seen_groups.contains(group.id),
                "group {} has no providers",
                group.id
            );
        }
        let mut ids: Vec<&str> = CAPABILITY_GROUPS.iter().map(|g| g.id).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), CAPABILITY_GROUPS.len(), "duplicate group ids");
    }

    #[test]
    fn swappable_capabilities_have_multiple_providers() {
        let providers = |group: &str| -> Vec<&str> {
            CAPABILITIES
                .iter()
                .filter(|d| d.capability == group)
                .map(|d| d.id)
                .collect()
        };
        assert_eq!(providers("secret-scanning"), vec!["trufflehog", "gitleaks"]);
        assert_eq!(providers("semantic-search"), vec!["cocoindex", "ccc"]);
        assert_eq!(providers("coding-harness").len(), 7);
        // Status rows carry the group id through detection.
        let disabled = std::collections::BTreeSet::new();
        let jobs = std::collections::BTreeMap::new();
        let latest = std::collections::BTreeMap::new();
        let caps = detect_capabilities(&base_opts(
            &disabled,
            &jobs,
            &latest,
            fake_exec(&[]),
            fake_which(&[]),
        ));
        assert!(caps.iter().all(|cap| get_group(&cap.capability).is_some()));
    }

    #[test]
    fn unknown_ids_resolve_to_none() {
        assert!(get_capability("../../bin/sh").is_none());
        assert!(get_capability("rm").is_none());
        assert!(LifecycleAction::parse("explode").is_none());
        assert_eq!(
            LifecycleAction::parse("reinstall"),
            Some(LifecycleAction::Reinstall)
        );
    }
}
