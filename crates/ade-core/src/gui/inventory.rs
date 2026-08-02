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
    /// A Claude Code plugin installed at user scope, so it applies to every
    /// project the harness opens rather than being wired per repo.
    ClaudePlugin,
    Manual,
}

impl LifecycleMethod {
    pub fn as_str(&self) -> &'static str {
        match self {
            LifecycleMethod::Brew => "brew",
            LifecycleMethod::BrewCask => "brew-cask",
            LifecycleMethod::Npm => "npm",
            LifecycleMethod::ClaudePlugin => "claude-plugin",
            LifecycleMethod::Manual => "manual",
        }
    }
}

/// The marketplace a Claude Code plugin is installed from. Adding it is
/// idempotent, and `claude plugin install` cannot resolve `name@marketplace`
/// until the marketplace is known, so install is a two-step sequence.
pub const CODEGUARD_MARKETPLACE: &str = "cosai-oasis/project-codeguard";

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
    /// Version argv appended to the resolved binary. EMPTY means the tool has
    /// no version command at all — a real case, not an oversight. Probing one
    /// anyway reports a permanent failure for a question the tool cannot answer.
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

pub const CAPABILITIES: [CapabilityDef; 18] = [
    // ── Integrated tools (mirrors INTEGRATED_TOOLS — asserted by test) ──
    CapabilityDef { id: "trufflehog", name: "TruffleHog", kind: CapabilityKind::Tool, capability: "secret-scanning", description: "Secret scanner wired into the pre-commit boundary by the secrets module", bins: &["trufflehog"], version_args: V, method: LifecycleMethod::Brew, pkg: Some("trufflehog"), guidance: None, process_names: &[] },
    CapabilityDef { id: "pre-commit", name: "pre-commit", kind: CapabilityKind::Tool, capability: "hook-orchestration", description: "Git hook framework the secrets module uses when present", bins: &["pre-commit"], version_args: V, method: LifecycleMethod::Brew, pkg: Some("pre-commit"), guidance: None, process_names: &[] },
    CapabilityDef { id: "gitleaks", name: "Gitleaks", kind: CapabilityKind::Tool, capability: "secret-scanning", description: "Complementary secret scanner (detected, optional)", bins: &["gitleaks"], version_args: &["version"], method: LifecycleMethod::Brew, pkg: Some("gitleaks"), guidance: None, process_names: &[] },
    CapabilityDef { id: "rtk", name: "RTK", kind: CapabilityKind::Tool, capability: "token-efficiency", description: "Token-efficiency proxy at the shell boundary (token-efficiency module)", bins: &["rtk"], version_args: V, method: LifecycleMethod::Brew, pkg: Some("rtk"), guidance: None, process_names: &[] },
    CapabilityDef { id: "ocean", name: "OCEAN", kind: CapabilityKind::Tool, capability: "repo-hygiene", description: "Git & repository hygiene hardening (git-hygiene module)", bins: &["ocean"], version_args: V, method: LifecycleMethod::Manual, pkg: None, guidance: Some("Install from https://github.com/grcengineering/OCEAN (installer script / release binary)"), process_names: &[] },
    CapabilityDef { id: "nono", name: "nono", kind: CapabilityKind::Tool, capability: "sandboxing", description: "Kernel-enforced sandboxing for terminal agents (sandbox module)", bins: &["nono"], version_args: V, method: LifecycleMethod::Brew, pkg: Some("nono"), guidance: None, process_names: &[] },
    CapabilityDef { id: "osv-scanner", name: "OSV-Scanner", kind: CapabilityKind::Tool, capability: "dependency-scanning", description: "Dependency vulnerability scanner (supply-chain module)", bins: &["osv-scanner"], version_args: V, method: LifecycleMethod::Brew, pkg: Some("osv-scanner"), guidance: None, process_names: &[] },
    CapabilityDef { id: "openwiki", name: "OpenWiki", kind: CapabilityKind::Tool, capability: "codebase-wiki", description: "Auto-maintained codebase wiki + opt-in Personal Brain (context module)", bins: &["openwiki"], version_args: V, method: LifecycleMethod::Npm, pkg: Some("openwiki"), guidance: None, process_names: &[] },
    CapabilityDef { id: "cocoindex", name: "CocoIndex", kind: CapabilityKind::Tool, capability: "semantic-search", description: "AST-based semantic code search framework (context module)", bins: &["cocoindex"], version_args: V, method: LifecycleMethod::Manual, pkg: None, guidance: Some("Install from https://cocoindex.io (Python framework — pip/uv)"), process_names: &[] },
    // ccc exposes no version command (verified against the shipped CLI: its
    // only global flags are --install-completion/--show-completion/--help).
    CapabilityDef { id: "ccc", name: "CocoIndex Code CLI (ccc)", kind: CapabilityKind::Tool, capability: "semantic-search", description: "cocoindex-code CLI — alternate CocoIndex entry point (context module)", bins: &["ccc"], version_args: &[], method: LifecycleMethod::Manual, pkg: None, guidance: Some("Install from https://github.com/cocoindex-io/cocoindex-code"), process_names: &[] },
    // Not a CLI: a user-scope Claude Code plugin, so `bins` is empty and
    // presence comes from the harness's own plugin list. Installing it once
    // covers every project the harness opens, which is the point.
    CapabilityDef { id: "codeguard", name: "CodeGuard", kind: CapabilityKind::Tool, capability: "agent-security-rules", description: "CoSAI/OASIS secure-coding ruleset the agent follows while writing code", bins: &[], version_args: &[], method: LifecycleMethod::ClaudePlugin, pkg: Some("codeguard-security@project-codeguard"), guidance: None, process_names: &[] },
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

pub const CAPABILITY_GROUPS: [CapabilityGroup; 10] = [
    CapabilityGroup {
        id: "agent-security-rules",
        name: "Agent Security Rules",
        why: "Scanners catch insecure code after it is written; rules stop it being written. A ruleset loaded into the agent itself shapes every suggestion it makes — injection-safe queries, real authorization checks, sound crypto — across every project, without anyone remembering to ask for it.",
    },
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
        name: "Dependency Scanning",
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

/// "A", "A and B", "A, B and C" — no Oxford comma, matching Apple's copy.
pub(crate) fn join_human(parts: &[String]) -> String {
    match parts {
        [] => String::new(),
        [one] => one.clone(),
        [first, second] => format!("{first} and {second}"),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    }
}

/// One provider's contribution to its capability group's coverage.
pub struct ProviderFact<'a> {
    pub group: &'a str,
    pub id: &'a str,
    pub name: &'a str,
    pub works: bool,
}

/// Which capability groups actually have a working provider.
///
/// Defined once, here, because both the detection pass and the health verdict
/// need it — and a second definition is exactly how the tray and the Control
/// Center learned to disagree with each other.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GroupCoverage {
    working: std::collections::BTreeMap<String, Vec<String>>,
}

impl GroupCoverage {
    /// The one definition of "this provider is actually working": the owner
    /// left it enabled, it resolved on PATH, and its version story is sound —
    /// it either answered a version probe or has no version command to answer
    /// with — and it is actually in force. A tool that is asked what it is and
    /// cannot say, or one that is installed but switched off, does not count as
    /// coverage; those are the cases where the environment is lying to you.
    pub fn provider_works(enabled: bool, installed: bool, healthy: bool) -> bool {
        enabled && installed && healthy
    }

    pub fn compute<'a>(providers: impl IntoIterator<Item = ProviderFact<'a>>) -> Self {
        let mut working: std::collections::BTreeMap<String, Vec<(usize, String)>> =
            std::collections::BTreeMap::new();
        for fact in providers {
            let entry = working.entry(fact.group.to_string()).or_default();
            if fact.works {
                let order = CAPABILITIES
                    .iter()
                    .position(|def| def.id == fact.id)
                    .unwrap_or(usize::MAX);
                entry.push((order, fact.name.to_string()));
            }
        }
        GroupCoverage {
            // Taxonomy order, so the input's order cannot change the output.
            working: working
                .into_iter()
                .map(|(group, mut names)| {
                    names.sort_by_key(|(order, _)| *order);
                    (group, names.into_iter().map(|(_, name)| name).collect())
                })
                .collect(),
        }
    }

    pub fn from_statuses(capabilities: &[CapabilityStatus]) -> Self {
        Self::compute(capabilities.iter().map(|cap| ProviderFact {
            group: cap.capability.as_str(),
            id: cap.id.as_str(),
            name: cap.name.as_str(),
            works: Self::provider_works(cap.enabled, cap.installed, cap.is_healthy()),
        }))
    }

    pub fn is_covered(&self, group: &str) -> bool {
        !self.working(group).is_empty()
    }

    /// Names of the working providers in a group, in taxonomy order.
    pub fn working(&self, group: &str) -> &[String] {
        self.working
            .get(group)
            .map(Vec::as_slice)
            .unwrap_or_default()
    }
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
        // Install is the one action that needs two steps: the marketplace has
        // to be known before `plugin@marketplace` can resolve. Adding an
        // already-known marketplace is a no-op, so this stays idempotent.
        (LifecycleMethod::ClaudePlugin, LifecycleAction::Install | LifecycleAction::Reinstall) => {
            return Some(vec![
                vec![
                    "claude".into(),
                    "plugin".into(),
                    "marketplace".into(),
                    "add".into(),
                    CODEGUARD_MARKETPLACE.into(),
                ],
                vec![
                    "claude".into(),
                    "plugin".into(),
                    "install".into(),
                    pkg.into(),
                ],
            ]);
        }
        (LifecycleMethod::ClaudePlugin, LifecycleAction::Uninstall) => vec![
            "claude".into(),
            "plugin".into(),
            "uninstall".into(),
            pkg.into(),
        ],
        (LifecycleMethod::ClaudePlugin, LifecycleAction::Update) => vec![
            "claude".into(),
            "plugin".into(),
            "update".into(),
            pkg.into(),
        ],
    };
    Some(vec![argv])
}

/// One entry of `claude plugin list --json`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstalledPlugin {
    pub version: Option<String>,
    pub install_path: Option<String>,
    /// A plugin can be installed and switched off, in which case it is loaded
    /// by nothing and protects nothing.
    pub enabled: bool,
}

/// Find one plugin in `claude plugin list --json` output. Tolerates every
/// failure mode — bad JSON, a changed shape, a missing entry — by reporting
/// absence rather than guessing.
pub fn parse_installed_plugin(stdout: &str, id: &str) -> Option<InstalledPlugin> {
    let parsed: serde_json::Value = serde_json::from_str(stdout).ok()?;
    let entries = parsed
        .as_array()
        .cloned()
        .or_else(|| parsed["plugins"].as_array().cloned())?;
    let entry = entries
        .into_iter()
        .find(|entry| entry["id"].as_str() == Some(id))?;
    Some(InstalledPlugin {
        version: entry["version"]
            .as_str()
            .filter(|version| !version.is_empty() && *version != "unknown")
            .map(String::from),
        install_path: entry["installPath"].as_str().map(String::from),
        // Absent means we cannot show it is live, so do not claim it is.
        enabled: entry["enabled"].as_bool().unwrap_or(false),
    })
}

/// Argv for the on-demand latest-version lookup; None for manual capabilities.
pub fn latest_version_argv(def: &CapabilityDef) -> Option<Vec<String>> {
    let pkg = def.pkg?;
    Some(match def.method {
        LifecycleMethod::Manual => return None,
        // The harness exposes no marketplace-version query (`plugin list
        // --available --json` returns an empty set), so there is no honest way
        // to say an update exists. Update still runs; it just cannot be
        // predicted, and claiming otherwise would be inventing a fact.
        LifecycleMethod::ClaudePlugin => return None,
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

/// Ask the harness which plugins it has. One call answers presence, version and
/// whether the plugin is actually switched on.
pub fn claude_plugin_list_argv() -> Vec<String> {
    vec![
        "claude".into(),
        "plugin".into(),
        "list".into(),
        "--json".into(),
    ]
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
        // Neither has a queryable "latest", so neither ever reaches here —
        // `latest_version_argv` returns None for both.
        LifecycleMethod::Manual | LifecycleMethod::ClaudePlugin => None,
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
    /// False when the tool has no version command, so a missing `version` is
    /// expected rather than a fault.
    pub reports_version: bool,
    /// Set when the capability is installed but not actually in force, so it
    /// must not be counted as covering anything.
    pub inactive: Option<String>,
    /// Only Some for capabilities with a process signature.
    pub running: Option<bool>,
    pub enabled: bool,
    pub latest_version: Option<String>,
    pub update_available: bool,
    pub issues: Vec<Finding>,
}

impl CapabilityStatus {
    /// Whether the version story is satisfactory: either the tool reported one,
    /// or it has no version command to report with.
    pub fn version_ok(&self) -> bool {
        self.version.is_some() || !self.reports_version
    }

    /// Installed, able to say what it is, and actually in force. This is the
    /// bar for counting as coverage — anything less is a guardrail on paper.
    pub fn is_healthy(&self) -> bool {
        self.version_ok() && self.inactive.is_none()
    }

    /// The version number alone, for a column where every row must line up.
    pub fn short_version(&self) -> Option<String> {
        self.version.as_deref().map(short_version)
    }
}

/// Pull the version number out of whatever a tool prints for `--version`.
///
/// Every CLI answers differently — `trufflehog 3.96.0`, `osv-scanner version:
/// 2.4.0`, `8.30.1`, `Hermes Agent v0.18.2 (2026.7.7.2) · upstream 0fa5e41c` —
/// so a column of raw strings reads as noise even though the data is fine. Take
/// the first dotted-numeric token; the full string stays available on hover.
pub fn short_version(raw: &str) -> String {
    let is_version = |token: &str| {
        let body = token.trim_start_matches(['v', 'V']);
        let mut parts = body.split('.');
        let first = parts.next().unwrap_or("");
        !first.is_empty()
            && first.chars().all(|c| c.is_ascii_digit())
            && parts.clone().count() >= 1
            && parts.all(|part| {
                !part.is_empty() && part.chars().next().is_some_and(|c| c.is_ascii_digit())
            })
    };
    raw.split_whitespace()
        .map(|token| token.trim_matches(|c: char| !c.is_alphanumeric()))
        .find(|token| is_version(token))
        .map(|token| token.trim_start_matches(['v', 'V']).to_string())
        .unwrap_or_else(|| raw.trim().to_string())
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

/// What a probe learned about one capability, before any judgement is applied.
/// Splitting the facts from the verdict is what lets severity be coverage-aware:
/// no single capability can know whether its group is already covered, so the
/// judgement has to happen after every probe has reported.
struct CapabilityFacts {
    enabled: bool,
    installed: bool,
    path: Option<String>,
    version: Option<String>,
    running: Option<bool>,
    latest_version: Option<String>,
    update_available: bool,
    /// Present when the thing is installed but not actually in force. A plugin
    /// the harness has switched off is the case this exists for: it is on disk,
    /// it reports a version, and it is protecting nothing.
    inactive: Option<String>,
}

/// Assemble the facts that do not depend on how presence was established.
fn base_facts(
    def: &'static CapabilityDef,
    opts: &DetectOptions<'_>,
    enabled: bool,
    installed: bool,
    path: Option<String>,
    version: Option<String>,
) -> CapabilityFacts {
    let latest_version = opts.latest_versions.get(def.id).cloned();
    let update_available = match (&latest_version, &version) {
        (Some(latest), Some(current)) => !current.contains(latest.as_str()),
        _ => false,
    };
    CapabilityFacts {
        enabled,
        installed,
        path,
        version,
        running: None,
        latest_version,
        update_available,
        inactive: None,
    }
}

fn probe_one(def: &'static CapabilityDef, opts: &DetectOptions<'_>) -> CapabilityFacts {
    let enabled = !opts.disabled.contains(def.id);
    let mut path: Option<String> = None;
    for bin in def.bins {
        if let Some(hit) = (opts.which)(bin) {
            path = Some(hit);
            break;
        }
    }
    let mut installed = path.is_some();
    let mut inactive: Option<String> = None;
    // A harness plugin never lands on PATH, so presence comes from the harness
    // itself rather than from `which`.
    if def.method == LifecycleMethod::ClaudePlugin {
        if let Some(id) = def.pkg {
            let result = (opts.exec)(&claude_plugin_list_argv(), &ExecOpts::default());
            if result.code == 0 {
                if let Some(plugin) = parse_installed_plugin(&result.stdout, id) {
                    installed = true;
                    path = plugin.install_path.clone();
                    if !plugin.enabled {
                        inactive = Some("installed but switched off in Claude Code".to_string());
                    }
                    let mut facts = base_facts(def, opts, enabled, installed, path, plugin.version);
                    facts.inactive = inactive;
                    return facts;
                }
            }
        }
        return base_facts(def, opts, enabled, false, None, None);
    }
    let mut version: Option<String> = None;
    if let (Some(bin_path), false) = (&path, def.version_args.is_empty()) {
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
    let mut facts = base_facts(def, opts, enabled, installed, path, version);
    facts.running = running;
    facts
}

/// Turn probe facts into findings. Pure — no I/O, no clock, no environment.
fn assess(
    def: &'static CapabilityDef,
    facts: CapabilityFacts,
    coverage: &GroupCoverage,
    last_jobs: &std::collections::BTreeMap<String, LastJobSummary>,
) -> CapabilityStatus {
    let CapabilityFacts {
        enabled,
        installed,
        path,
        version,
        running,
        latest_version,
        update_available,
        inactive,
    } = facts;

    let mut issues: Vec<Finding> = Vec::new();
    if !enabled {
        issues.push(Finding::info(
            "disabled in ADE GUI (machine-level preference — excluded from health rollups; project policy lives in each repo's ade.json)",
        ));
    } else {
        if !installed {
            let how = match action_argvs(def, LifecycleAction::Install) {
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
            // A missing provider whose capability is already covered is a spare
            // tyre, not a hole in the floor. Ranking the two identically is what
            // made the old warning list unreadable — the urgent items were
            // indistinguishable from the optional ones.
            let covered_by = coverage.working(def.capability);
            if covered_by.is_empty() {
                issues.push(Finding::warn(format!("{} is not installed", def.name), how));
            } else {
                let group_name = get_group(def.capability)
                    .map(|group| group.name)
                    .unwrap_or("This capability");
                issues.push(Finding {
                    level: crate::types::FindingLevel::Info,
                    message: format!("{} is not installed", def.name),
                    remediation: Some(format!(
                        "{group_name} is already covered by {}. {how}",
                        join_human(covered_by)
                    )),
                });
            }
        }
        // Present but not in force. Worth an error rather than a warning: the
        // environment is reporting a guardrail that is not guarding.
        if let Some(reason) = &inactive {
            issues.push(Finding::error_with(
                format!("{} is {reason}", def.name),
                match def.method {
                    LifecycleMethod::ClaudePlugin => {
                        format!("Run: claude plugin enable {}", def.pkg.unwrap_or(def.id))
                    }
                    _ => "re-enable it where it was switched off".to_string(),
                },
            ));
        }
        if installed && version.is_none() && !def.version_args.is_empty() {
            issues.push(Finding::error_with(
                format!("{} is on PATH but its version probe failed", def.name),
                format!(
                    "Run `{} {}` in a terminal to inspect",
                    path.as_deref().unwrap_or(def.bins[0]),
                    def.version_args.join(" ")
                ),
            ));
        }
        if let Some(last) = last_jobs.get(def.id) {
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
        reports_version: !def.version_args.is_empty(),
        inactive,
        running,
        enabled,
        latest_version,
        update_available,
        issues,
    }
}

/// Detect all capabilities concurrently; each probe is timeout-bounded (ISC-170).
/// Probes run in parallel, then coverage is computed across the whole taxonomy,
/// then each capability is judged against it.
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
    let facts: Vec<CapabilityFacts> = std::thread::scope(|scope| {
        let handles: Vec<_> = CAPABILITIES
            .iter()
            .map(|def| scope.spawn(|| probe_one(def, &bounded)))
            .collect();
        handles
            .into_iter()
            .map(|handle| handle.join().expect("detect thread"))
            .collect()
    });
    let coverage =
        GroupCoverage::compute(
            CAPABILITIES
                .iter()
                .zip(&facts)
                .map(|(def, fact)| ProviderFact {
                    group: def.capability,
                    id: def.id,
                    name: def.name,
                    works: GroupCoverage::provider_works(
                        fact.enabled,
                        fact.installed,
                        (fact.version.is_some() || def.version_args.is_empty())
                            && fact.inactive.is_none(),
                    ),
                }),
        );
    CAPABILITIES
        .iter()
        .zip(facts)
        .map(|(def, fact)| assess(def, fact, &coverage, opts.last_jobs))
        .collect()
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
        // A manual capability points at its own documentation instead of a recipe.
        let ocean = caps.iter().find(|cap| cap.id == "ocean").unwrap();
        assert!(ocean.issues.iter().any(|issue| issue
            .remediation
            .as_deref()
            .unwrap_or("")
            .contains("github.com/grcengineering/OCEAN")));
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
    fn a_missing_provider_only_warns_when_its_capability_has_no_cover() {
        let disabled = BTreeSet::new();
        let jobs = BTreeMap::new();
        let latest = BTreeMap::new();
        let caps = detect_capabilities(&base_opts(
            &disabled,
            &jobs,
            &latest,
            fake_exec(&[(
                "/fake/bin/trufflehog --version",
                (0, "trufflehog 3.95.9\n", ""),
            )]),
            fake_which(&["trufflehog"]),
        ));
        // Spare tyre: TruffleHog is already scanning, so a missing Gitleaks is
        // informational and says what covers it.
        let gitleaks = caps.iter().find(|cap| cap.id == "gitleaks").unwrap();
        let spare = &gitleaks.issues[0];
        assert_eq!(spare.level, FindingLevel::Info);
        assert!(spare.message.contains("Gitleaks is not installed"));
        let remediation = spare.remediation.as_deref().unwrap();
        assert!(remediation.starts_with("Secret Scanning is already covered by TruffleHog."));
        assert!(
            remediation.contains("brew install gitleaks"),
            "the recipe stays available even when it is optional"
        );
        // Hole in the floor: nothing else provides sandboxing.
        let nono = caps.iter().find(|cap| cap.id == "nono").unwrap();
        assert_eq!(nono.issues[0].level, FindingLevel::Warn);
    }

    #[test]
    fn an_installed_provider_that_cannot_report_a_version_does_not_count_as_cover() {
        // The dangerous case: TruffleHog is on PATH, so a naive check calls
        // Secret Scanning covered — but it fails its own version probe, so it
        // may not be scanning anything. A missing Gitleaks must stay a warning.
        let disabled = BTreeSet::new();
        let jobs = BTreeMap::new();
        let latest = BTreeMap::new();
        let caps = detect_capabilities(&base_opts(
            &disabled,
            &jobs,
            &latest,
            fake_exec(&[("/fake/bin/trufflehog --version", (1, "", "killed"))]),
            fake_which(&["trufflehog"]),
        ));
        let trufflehog = caps.iter().find(|cap| cap.id == "trufflehog").unwrap();
        assert!(trufflehog.installed && trufflehog.version.is_none());
        let gitleaks = caps.iter().find(|cap| cap.id == "gitleaks").unwrap();
        assert_eq!(gitleaks.issues[0].level, FindingLevel::Warn);
        assert!(gitleaks.issues[0]
            .remediation
            .as_deref()
            .unwrap()
            .starts_with("Use Install here"));
    }

    #[test]
    fn a_disabled_provider_never_counts_as_cover_for_its_capability() {
        let mut disabled = BTreeSet::new();
        disabled.insert("trufflehog".to_string());
        let jobs = BTreeMap::new();
        let latest = BTreeMap::new();
        let caps = detect_capabilities(&base_opts(
            &disabled,
            &jobs,
            &latest,
            fake_exec(&[(
                "/fake/bin/trufflehog --version",
                (0, "trufflehog 3.95.9\n", ""),
            )]),
            fake_which(&["trufflehog"]),
        ));
        let gitleaks = caps.iter().find(|cap| cap.id == "gitleaks").unwrap();
        assert_eq!(
            gitleaks.issues[0].level,
            FindingLevel::Warn,
            "a capability the owner switched off cannot silently cover the gap"
        );
    }

    #[test]
    fn version_strings_reduce_to_a_number_a_column_can_align() {
        // Every one of these is a real string this machine's tools printed.
        for (raw, expected) in [
            ("trufflehog 3.96.0", "3.96.0"),
            ("pre-commit 4.6.1", "4.6.1"),
            ("8.30.1", "8.30.1"),
            ("rtk 0.29.0", "0.29.0"),
            ("osv-scanner version: 2.4.0", "2.4.0"),
            ("codex-cli 0.144.5", "0.144.5"),
            ("2.1.220 (Claude Code)", "2.1.220"),
            (
                "Hermes Agent v0.18.2 (2026.7.7.2) · upstream 0fa5e41c · local 4c96172d",
                "0.18.2",
            ),
        ] {
            assert_eq!(short_version(raw), expected, "for {raw:?}");
        }
        // Nothing version-shaped: keep the original rather than invent one.
        assert_eq!(short_version("unknown build"), "unknown build");
        assert_eq!(short_version("  spaced  "), "spaced");
        // A bare integer is not a version — "42" alone would be a false read.
        assert_eq!(short_version("tool 42"), "tool 42");
        let mut cap = detect_capabilities(&base_opts(
            &BTreeSet::new(),
            &BTreeMap::new(),
            &BTreeMap::new(),
            fake_exec(&[(
                "/fake/bin/trufflehog --version",
                (0, "trufflehog 3.96.0\n", ""),
            )]),
            fake_which(&["trufflehog"]),
        ));
        let hog = cap.iter_mut().find(|c| c.id == "trufflehog").unwrap();
        assert_eq!(hog.short_version().as_deref(), Some("3.96.0"));
        hog.version = None;
        assert!(hog.short_version().is_none());
    }

    /// Real `claude plugin list --json` output, trimmed to the shape that matters.
    const PLUGIN_LIST: &str = r#"[
      {"id":"repowise@repowise","version":"0.32.0","scope":"user","enabled":true,
       "installPath":"/x/repowise"},
      {"id":"codeguard-security@project-codeguard","version":"1.4.0","scope":"user",
       "enabled":true,"installPath":"/x/codeguard/1.4.0"},
      {"id":"code-review@claude-plugins-official","version":"unknown","scope":"user",
       "enabled":true,"installPath":"/x/cr"}
    ]"#;

    #[test]
    fn a_harness_plugin_is_found_through_the_harness_not_the_path() {
        // CodeGuard is a Claude Code plugin: nothing lands on PATH, so `which`
        // can never see it and the harness's own list is the source of truth.
        let codeguard = get_capability("codeguard").unwrap();
        assert!(codeguard.bins.is_empty());
        assert_eq!(codeguard.method, LifecycleMethod::ClaudePlugin);

        let found = parse_installed_plugin(PLUGIN_LIST, "codeguard-security@project-codeguard")
            .expect("present in the list");
        assert_eq!(found.version.as_deref(), Some("1.4.0"));
        assert_eq!(found.install_path.as_deref(), Some("/x/codeguard/1.4.0"));
        assert!(found.enabled);
        // A plugin the harness reports as "unknown" has no version to show.
        let unknown =
            parse_installed_plugin(PLUGIN_LIST, "code-review@claude-plugins-official").unwrap();
        assert!(unknown.version.is_none());
        // Absent, malformed and unparseable all mean "not found", never a guess.
        assert!(parse_installed_plugin(PLUGIN_LIST, "nope@nowhere").is_none());
        assert!(parse_installed_plugin("not json", "any").is_none());
        assert!(parse_installed_plugin("{}", "any").is_none());
        assert!(parse_installed_plugin(r#"{"plugins":[{"id":"a@b"}]}"#, "a@b").is_some());
        // enabled absent → assume not live rather than assume protection.
        let cautious = parse_installed_plugin(r#"[{"id":"a@b"}]"#, "a@b").unwrap();
        assert!(!cautious.enabled);
    }

    #[test]
    fn detection_reads_codeguard_out_of_the_plugin_list() {
        let disabled = BTreeSet::new();
        let jobs = BTreeMap::new();
        let latest = BTreeMap::new();
        let caps = detect_capabilities(&base_opts(
            &disabled,
            &jobs,
            &latest,
            fake_exec(&[("claude plugin list --json", (0, PLUGIN_LIST, ""))]),
            fake_which(&[]),
        ));
        let codeguard = caps.iter().find(|cap| cap.id == "codeguard").unwrap();
        assert!(codeguard.installed);
        assert_eq!(codeguard.version.as_deref(), Some("1.4.0"));
        assert_eq!(codeguard.path.as_deref(), Some("/x/codeguard/1.4.0"));
        assert!(codeguard.inactive.is_none());
        assert!(codeguard.issues.is_empty(), "{:?}", codeguard.issues);
        assert!(GroupCoverage::from_statuses(&caps).is_covered("agent-security-rules"));
    }

    #[test]
    fn a_plugin_that_is_installed_but_switched_off_protects_nothing_and_says_so() {
        // The dangerous state: on disk, reporting a version, loaded by nothing.
        // Counting it as coverage would be the exact lie this product exists to
        // avoid.
        let off = r#"[{"id":"codeguard-security@project-codeguard","version":"1.4.0",
                       "enabled":false,"installPath":"/x/cg"}]"#;
        let disabled = BTreeSet::new();
        let jobs = BTreeMap::new();
        let latest = BTreeMap::new();
        let caps = detect_capabilities(&base_opts(
            &disabled,
            &jobs,
            &latest,
            fake_exec(&[("claude plugin list --json", (0, off, ""))]),
            fake_which(&[]),
        ));
        let codeguard = caps.iter().find(|cap| cap.id == "codeguard").unwrap();
        assert!(codeguard.installed, "it really is on disk");
        assert_eq!(
            codeguard.inactive.as_deref(),
            Some("installed but switched off in Claude Code")
        );
        assert!(!codeguard.is_healthy());
        let issue = &codeguard.issues[0];
        assert_eq!(issue.level, FindingLevel::Error);
        assert!(issue.message.contains("switched off"));
        assert!(issue
            .remediation
            .as_deref()
            .unwrap()
            .contains("claude plugin enable codeguard-security@project-codeguard"));
        assert!(!GroupCoverage::from_statuses(&caps).is_covered("agent-security-rules"));
    }

    #[test]
    fn a_missing_or_failing_harness_leaves_codeguard_absent_not_installed() {
        let disabled = BTreeSet::new();
        let jobs = BTreeMap::new();
        let latest = BTreeMap::new();
        // `claude` not on PATH at all → the exec fake returns a failure.
        let caps = detect_capabilities(&base_opts(
            &disabled,
            &jobs,
            &latest,
            fake_exec(&[("claude plugin list --json", (127, "", "not found"))]),
            fake_which(&[]),
        ));
        let codeguard = caps.iter().find(|cap| cap.id == "codeguard").unwrap();
        assert!(!codeguard.installed);
        assert!(codeguard.version.is_none());
        // Sole provider of its capability, so this is a real gap, not a spare.
        assert_eq!(codeguard.issues[0].level, FindingLevel::Warn);
        assert!(codeguard.issues[0]
            .remediation
            .as_deref()
            .unwrap()
            .contains("claude plugin install codeguard-security@project-codeguard"));
    }

    #[test]
    fn installing_a_plugin_adds_its_marketplace_first() {
        let codeguard = get_capability("codeguard").unwrap();
        // Two steps: `install` cannot resolve name@marketplace until the
        // marketplace is known, and adding a known one is a no-op.
        assert_eq!(
            action_argvs(codeguard, LifecycleAction::Install).unwrap(),
            vec![
                vec![
                    "claude".to_string(),
                    "plugin".into(),
                    "marketplace".into(),
                    "add".into(),
                    "cosai-oasis/project-codeguard".into()
                ],
                vec![
                    "claude".to_string(),
                    "plugin".into(),
                    "install".into(),
                    "codeguard-security@project-codeguard".into()
                ],
            ]
        );
        assert_eq!(
            action_argvs(codeguard, LifecycleAction::Reinstall).unwrap(),
            action_argvs(codeguard, LifecycleAction::Install).unwrap()
        );
        assert_eq!(
            action_argvs(codeguard, LifecycleAction::Uninstall).unwrap(),
            vec![vec![
                "claude".to_string(),
                "plugin".into(),
                "uninstall".into(),
                "codeguard-security@project-codeguard".into()
            ]]
        );
        assert_eq!(
            action_argvs(codeguard, LifecycleAction::Update).unwrap(),
            vec![vec![
                "claude".to_string(),
                "plugin".into(),
                "update".into(),
                "codeguard-security@project-codeguard".into()
            ]]
        );
        // No marketplace-version query exists, so no update is ever predicted.
        assert!(latest_version_argv(codeguard).is_none());
        assert!(parse_latest_version(
            codeguard,
            &ExecResult {
                code: 0,
                stdout: "1.5.0".into(),
                stderr: String::new()
            }
        )
        .is_none());
    }

    #[test]
    fn nono_installs_through_homebrew_like_every_other_formula_backed_tool() {
        // It was marked manual, which read as "this one is harder" when the
        // formula has existed all along (homebrew-core Formula/n/nono.rb).
        let nono = get_capability("nono").unwrap();
        assert_eq!(nono.method, LifecycleMethod::Brew);
        assert_eq!(nono.pkg, Some("nono"));
        assert!(nono.guidance.is_none(), "an automated tool needs no prose");
        assert_eq!(
            action_argvs(nono, LifecycleAction::Install).unwrap(),
            vec![vec!["brew".to_string(), "install".into(), "nono".into()]]
        );
        // The remaining manual entries are manual because nothing packages
        // them, not because nobody checked.
        let manual: Vec<&str> = CAPABILITIES
            .iter()
            .filter(|def| def.method == LifecycleMethod::Manual)
            .map(|def| def.id)
            .collect();
        assert_eq!(manual, vec!["ocean", "cocoindex", "ccc", "hermes"]);
    }

    #[test]
    fn a_tool_with_no_version_command_is_working_not_broken() {
        // Found on a real machine: `ccc` has no --version at all (its only
        // global flags are --install-completion/--show-completion/--help), so
        // probing one reported a permanent failure for a question the tool
        // cannot answer — and Semantic Code Search read "installed but not
        // working" while ccc was installed and fine.
        let ccc = get_capability("ccc").unwrap();
        assert!(
            ccc.version_args.is_empty(),
            "ccc declares no version command"
        );
        let disabled = BTreeSet::new();
        let jobs = BTreeMap::new();
        let latest = BTreeMap::new();
        let caps = detect_capabilities(&base_opts(
            &disabled,
            &jobs,
            &latest,
            // Any exec call for ccc would be a bug: there is nothing to run.
            fake_exec(&[]),
            fake_which(&["ccc"]),
        ));
        let status = caps.iter().find(|cap| cap.id == "ccc").unwrap();
        assert!(status.installed);
        assert!(status.version.is_none());
        assert!(!status.reports_version);
        assert!(status.version_ok(), "a missing version is expected here");
        assert!(
            status.issues.is_empty(),
            "no fault to report, got {:?}",
            status.issues
        );
        assert!(GroupCoverage::from_statuses(&caps).is_covered("semantic-search"));
        // Every other capability still expects a version, so the exemption
        // cannot be claimed by accident. Only two may skip the probe: ccc has
        // no version command, and CodeGuard is not a CLI at all — its version
        // comes from the harness that hosts it.
        for def in &CAPABILITIES {
            if !matches!(def.id, "ccc" | "codeguard") {
                assert!(!def.version_args.is_empty(), "{} lost its probe", def.id);
            }
        }
    }

    #[test]
    fn coverage_is_computed_the_same_way_from_facts_and_from_statuses() {
        let disabled = BTreeSet::new();
        let jobs = BTreeMap::new();
        let latest = BTreeMap::new();
        let caps = detect_capabilities(&base_opts(
            &disabled,
            &jobs,
            &latest,
            fake_exec(&[
                ("/fake/bin/trufflehog --version", (0, "3.95.9\n", "")),
                ("/fake/bin/gitleaks version", (0, "8.30.0\n", "")),
                ("/fake/bin/claude --version", (0, "2.1.220\n", "")),
            ]),
            fake_which(&["trufflehog", "gitleaks", "claude"]),
        ));
        let from_statuses = GroupCoverage::from_statuses(&caps);
        let from_facts = GroupCoverage::compute(caps.iter().map(|cap| ProviderFact {
            group: cap.capability.as_str(),
            id: cap.id.as_str(),
            name: cap.name.as_str(),
            works: GroupCoverage::provider_works(cap.enabled, cap.installed, cap.version.is_some()),
        }));
        assert_eq!(from_statuses, from_facts);
        assert!(from_statuses.is_covered("secret-scanning"));
        assert!(!from_statuses.is_covered("sandboxing"));
        assert_eq!(
            from_statuses.working("secret-scanning"),
            ["TruffleHog".to_string(), "Gitleaks".to_string()],
            "providers list in taxonomy order, primary first"
        );
        assert!(from_statuses.working("nothing-like-this").is_empty());
        // Input order must not change the answer.
        let mut reversed = caps.clone();
        reversed.reverse();
        assert_eq!(GroupCoverage::from_statuses(&reversed), from_statuses);
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
