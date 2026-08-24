//! Per-agent Project CodeGuard integration (ISA § "CodeGuard Integration").
//!
//! Deliberately NOT an `AdeModule`. Every module in `modules/` operates on one
//! target repo's `Ctx` — lockfile-scoped, verify-scoped, removed with that
//! repo. This is the opposite shape: configure the *agent*, once, at the
//! machine level, and the effect persists across every repo that agent ever
//! touches. It lives beside the machine-scoped capability system
//! (`gui::install`, `gui::state`), never inside a project's `.ade/` tree.
//!
//! CG-1: one definition per agent, gated on that agent's own harness being
//! present — `depends_on`, a relationship nothing in the repo-module system
//! has. CG-2: presence is read from the agent's own state, never `which` on a
//! `codeguard` binary (there isn't one).

use crate::types::{ExecFn, ExecOpts, WhichFn};
use std::path::{Path, PathBuf};

/// How CodeGuard reaches a given agent — grounded in
/// `docs/install-paths.md`, read live from the upstream repo, not assumed.
/// Rule files and skills are the DEFAULT path per CodeGuard's own stated
/// guidance; MCP is deliberately absent from this enum — it is an explicit,
/// separately-elected opt-in mode (CG-3), not a default install shape.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstallShape {
    /// `claude plugin marketplace add cosai-oasis/project-codeguard`, then
    /// `claude plugin install codeguard-security@project-codeguard`.
    /// Presence is read via `claude plugin list --json`, not a rule-file scan.
    ClaudePlugin,
    /// Static rule-file markdown at the agent's own canonical user-scope
    /// directory (`~/.cursor/rules/`, `~/.agents/rules/`).
    RuleFiles,
    /// Agent Skills at the agent's own canonical user-scope skills directory
    /// (`~/.opencode/skills/`, `~/.hermes/skills/`, `~/.agents/skills/`).
    SkillFiles,
}

/// One agent's CodeGuard integration definition. `agent_id` matches
/// `HarnessAdapter::id` exactly, so `depends_on` gating (CG-1) is a lookup,
/// never a second id vocabulary to keep in sync by hand.
#[derive(Debug, Clone, Copy)]
pub struct AgentCodeGuardDef {
    pub agent_id: &'static str,
    pub capability_id: &'static str,
    pub shape: InstallShape,
    /// User-scope directory this agent auto-discovers on its own, relative to
    /// `$HOME`. `None` for `ClaudePlugin`, whose presence is asked of the
    /// agent's own CLI rather than read off a directory.
    pub user_scope_dir: Option<&'static str>,
}

/// Six agents. Pi is deliberately absent — CodeGuard documents no install
/// path for it anywhere upstream (ISA fog item, not an oversight).
pub const CODEGUARD_AGENTS: [AgentCodeGuardDef; 6] = [
    AgentCodeGuardDef {
        agent_id: "claude-code",
        capability_id: "codeguard-claude-code",
        shape: InstallShape::ClaudePlugin,
        user_scope_dir: None,
    },
    AgentCodeGuardDef {
        agent_id: "codex",
        capability_id: "codeguard-codex",
        shape: InstallShape::SkillFiles,
        user_scope_dir: Some(".agents/skills"),
    },
    AgentCodeGuardDef {
        agent_id: "cursor",
        capability_id: "codeguard-cursor",
        shape: InstallShape::RuleFiles,
        user_scope_dir: Some(".cursor/rules"),
    },
    AgentCodeGuardDef {
        agent_id: "opencode",
        capability_id: "codeguard-opencode",
        shape: InstallShape::SkillFiles,
        user_scope_dir: Some(".opencode/skills"),
    },
    AgentCodeGuardDef {
        agent_id: "antigravity",
        capability_id: "codeguard-antigravity",
        shape: InstallShape::RuleFiles,
        user_scope_dir: Some(".agents/rules"),
    },
    AgentCodeGuardDef {
        agent_id: "hermes",
        capability_id: "codeguard-hermes",
        shape: InstallShape::SkillFiles,
        user_scope_dir: Some(".hermes/skills"),
    },
];

pub fn get_agent_def(capability_id: &str) -> Option<&'static AgentCodeGuardDef> {
    CODEGUARD_AGENTS
        .iter()
        .find(|def| def.capability_id == capability_id)
}

/// Injectable dependencies — no global `Ctx`, because this is machine-scoped,
/// not repo-scoped. Mirrors `gui::install::InstallDeps`'s shape so the whole
/// flow is testable against temp dirs and fake subprocesses.
pub struct CodeGuardDeps {
    pub home_dir: PathBuf,
    pub exec: ExecFn,
    pub which: WhichFn,
}

/// CG-1's gating result. `HarnessAbsent` is distinct from `NotInstalled`: a
/// capability whose agent isn't even present is inert, not a gap to close.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PresenceState {
    /// The agent itself isn't on this machine — this entry is inert.
    HarnessAbsent,
    /// The agent is present; CodeGuard is not wired up for it.
    NotInstalled,
    /// The agent is present and CodeGuard is wired up.
    Installed,
}

/// CLI names per agent, used only to test harness presence for CG-1 gating —
/// never to detect CodeGuard itself, which has no binary of its own (CG-2).
fn harness_cli_name(agent_id: &str) -> &'static str {
    match agent_id {
        "claude-code" => "claude",
        "codex" => "codex",
        "cursor" => "cursor",
        "opencode" => "opencode",
        "antigravity" => "antigravity",
        "hermes" => "hermes",
        _ => "",
    }
}

/// CG-1 + CG-2: gate on the agent's own presence first, then read the
/// agent's own state for CodeGuard — never `which` on a `codeguard` binary.
pub fn probe_presence(def: &AgentCodeGuardDef, deps: &CodeGuardDeps) -> PresenceState {
    let cli = harness_cli_name(def.agent_id);
    if cli.is_empty() || (deps.which)(cli).is_none() {
        return PresenceState::HarnessAbsent;
    }

    match def.shape {
        InstallShape::ClaudePlugin => {
            let result = (deps.exec)(
                &[
                    "claude".to_string(),
                    "plugin".to_string(),
                    "list".to_string(),
                    "--json".to_string(),
                ],
                &ExecOpts::default(),
            );
            if result.code != 0 {
                return PresenceState::NotInstalled;
            }
            if result.stdout.contains("codeguard-security") {
                PresenceState::Installed
            } else {
                PresenceState::NotInstalled
            }
        }
        InstallShape::RuleFiles | InstallShape::SkillFiles => {
            let Some(rel_dir) = def.user_scope_dir else {
                return PresenceState::NotInstalled;
            };
            let dir = deps.home_dir.join(rel_dir);
            let has_codeguard_content = std::fs::read_dir(&dir)
                .map(|entries| {
                    entries.flatten().any(|entry| {
                        entry
                            .file_name()
                            .to_string_lossy()
                            .to_lowercase()
                            .contains("codeguard")
                    })
                })
                .unwrap_or(false);
            if has_codeguard_content {
                PresenceState::Installed
            } else {
                PresenceState::NotInstalled
            }
        }
    }
}

// ── CG-3: install actions ───────────────────────────────────────────────
//
// Content source for RuleFiles/SkillFiles: RESOLVED (owner decision,
// 2026-08-23) — live-fetch from upstream, never a vendored snapshot. See
// `fetch_rule_content`/`fetch_latest_release_tag` below. `install_rule_or_skill`
// still takes `content` as a plain parameter rather than fetching it itself —
// the two stay composable (a caller fetches, then writes through CG-7's
// safety path), and it keeps this function trivially testable with static
// strings instead of a fake network layer.

const MARKETPLACE_SLUG: &str = "cosai-oasis/project-codeguard";
const PLUGIN_SLUG: &str = "codeguard-security@project-codeguard";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InstallOutcome {
    Installed,
    /// A step's exit code was non-zero; `step` names which one, `detail`
    /// carries stderr so the caller can report why, never a bare failure.
    Failed {
        step: &'static str,
        detail: String,
    },
}

/// CG-3 for Claude Code: `claude plugin marketplace add`, then
/// `claude plugin install`, exactly as CodeGuard's own docs specify. The
/// marketplace-add step is idempotent by design — a marketplace already
/// known is a no-op on the real CLI, so a non-zero exit there is a genuine
/// failure, not "already added" needing special-casing.
pub fn install_claude_plugin(deps: &CodeGuardDeps) -> InstallOutcome {
    let add = (deps.exec)(
        &[
            "claude".to_string(),
            "plugin".to_string(),
            "marketplace".to_string(),
            "add".to_string(),
            MARKETPLACE_SLUG.to_string(),
        ],
        &ExecOpts::default(),
    );
    if add.code != 0 {
        return InstallOutcome::Failed {
            step: "marketplace add",
            detail: add.stderr,
        };
    }

    let install = (deps.exec)(
        &[
            "claude".to_string(),
            "plugin".to_string(),
            "install".to_string(),
            PLUGIN_SLUG.to_string(),
        ],
        &ExecOpts::default(),
    );
    if install.code != 0 {
        return InstallOutcome::Failed {
            step: "plugin install",
            detail: install.stderr,
        };
    }

    InstallOutcome::Installed
}

/// CG-3 for RuleFiles/SkillFiles agents: write `content` under `filename` in
/// the agent's own canonical user-scope directory, through CG-7's
/// provenance-safe writer so a prior user edit is never clobbered. Returns
/// `None` when `def` has no `user_scope_dir` (a `ClaudePlugin` def passed
/// here by mistake) rather than panicking — a caller bug should surface as a
/// wrong-shape result, not a crash.
pub fn install_rule_or_skill(
    def: &AgentCodeGuardDef,
    filename: &str,
    content: &str,
    deps: &CodeGuardDeps,
    prior: Option<&ProvenanceRecord>,
) -> Option<std::io::Result<WriteOutcome>> {
    let rel_dir = def.user_scope_dir?;
    let dir = deps.home_dir.join(rel_dir);
    if let Err(e) = std::fs::create_dir_all(&dir) {
        return Some(Err(e));
    }
    let path = dir.join(filename);
    Some(write_with_provenance(&path, content, prior))
}

// ── CG-3 (remainder) / CG-6: live content + version fetch ──────────────────
//
// Resolved (owner decision, 2026-08-23): live-fetch from upstream, not a
// vendored snapshot. `curl` via the same injected `exec` every other
// network-touching check in this codebase already uses (`latest_version_argv`
// shells to `brew info`/`npm view`, never a raw HTTP client crate) — this
// stays consistent with that and adds zero new dependencies. Every fetch is
// pinned to a specific release tag, never `main`, matching CodeGuard's own
// remote-install guidance: "Pin to a release tag if you need a stable,
// auditable snapshot."

const CODEGUARD_RELEASES_API: &str =
    "https://api.github.com/repos/cosai-oasis/project-codeguard/releases/latest";
const CODEGUARD_RAW_BASE: &str = "https://raw.githubusercontent.com/cosai-oasis/project-codeguard";

/// CG-6: the latest published release tag, or `None` on any failure — a
/// down network or a rate-limited API must degrade to "no update known",
/// never propagate as an error that blocks an otherwise-successful install.
pub fn fetch_latest_release_tag(deps: &CodeGuardDeps) -> Option<String> {
    let result = (deps.exec)(
        &[
            "curl".to_string(),
            "-sL".to_string(),
            "--max-time".to_string(),
            "10".to_string(),
            CODEGUARD_RELEASES_API.to_string(),
        ],
        &ExecOpts::default(),
    );
    if result.code != 0 {
        return None;
    }
    let parsed: serde_json::Value = serde_json::from_str(&result.stdout).ok()?;
    parsed
        .get("tag_name")
        .and_then(|v| v.as_str())
        .map(str::to_string)
}

/// CG-3: fetch one rule file's raw content, pinned to `tag` — never `main`,
/// so two installs at the same tag are byte-identical and reproducible.
pub fn fetch_rule_content(deps: &CodeGuardDeps, tag: &str, rule_path: &str) -> Option<String> {
    let url = format!("{CODEGUARD_RAW_BASE}/{tag}/sources/rules/core/{rule_path}");
    let result = (deps.exec)(
        &[
            "curl".to_string(),
            "-sL".to_string(),
            "--fail".to_string(),
            "--max-time".to_string(),
            "10".to_string(),
            url,
        ],
        &ExecOpts::default(),
    );
    if result.code != 0 || result.stdout.is_empty() {
        return None;
    }
    Some(result.stdout)
}

// ── CG-7: provenance — never overwrite a user-modified file ────────────────

use crate::fsutil::{sha256_hex, stable_stringify, write_ensured};

/// One installed file's provenance: the hash ADE itself last wrote. If the
/// file's current on-disk hash no longer matches, a human edited it since —
/// exactly the invariant `managed.rs` proves for embedded blocks, adapted
/// here to whole standalone files (no marker parsing needed).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ProvenanceRecord {
    pub path: String,
    pub installed_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WriteOutcome {
    /// No prior record — file created, provenance recorded.
    Created,
    /// A prior record exists and matched — file updated, provenance refreshed.
    Updated { changed: bool },
    /// A prior record exists but the on-disk hash no longer matches it — a
    /// human edited this file. Refused. The file is untouched.
    Conflict { existing_hash: String },
}

/// Write `content` to `path`, refusing when the file was user-modified since
/// ADE last wrote it. `prior` is this path's last recorded provenance, if any
/// — the caller owns persisting the returned hash into its own provenance
/// store; this function never reads or writes that store itself, so it stays
/// pure I/O plus one comparison, mirroring `write_ensured`'s own shape.
pub fn write_with_provenance(
    path: &Path,
    content: &str,
    prior: Option<&ProvenanceRecord>,
) -> std::io::Result<WriteOutcome> {
    if let Some(record) = prior {
        if let Ok(existing) = std::fs::read_to_string(path) {
            let current_hash = sha256_hex(&existing);
            if current_hash != record.installed_hash {
                return Ok(WriteOutcome::Conflict {
                    existing_hash: current_hash,
                });
            }
        }
        // Prior record exists but the file is gone — treat as a fresh write,
        // matching write_ensured's own "absent means write" contract.
        let changed = write_ensured(path, content)?;
        return Ok(WriteOutcome::Updated { changed });
    }
    write_ensured(path, content)?;
    Ok(WriteOutcome::Created)
}

// ── CG-8: machine-scoped opt-in state ───────────────────────────────────
//
// Mirrors `gui::state::{GuiState, load_gui_state, save_gui_state}`'s exact
// contract deliberately, not by coincidence: hand-rolled field-by-name JSON
// (not `#[derive(Serialize, Deserialize)]`, so an unknown key from a future
// schema version is silently ignored rather than a hard parse error),
// corrupt/invalid content degrades to defaults with a warning, never a
// crash, and off-by-default falls straight out of `BTreeSet::new()` — an
// absent file means an absent entry means not opted in, no special-casing
// needed anywhere that reads this state.

pub const CODEGUARD_STATE_FILE: &str = "codeguard.json";
pub const CODEGUARD_STATE_SCHEMA_VERSION: u64 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CodeGuardState {
    /// Capability ids (`codeguard-<agent>`) the owner explicitly opted in.
    /// Never populated as a side effect of `ade init`/`ade apply` on any
    /// repo — the only writer is the explicit opt-in action (CG-9).
    pub enabled_agents: std::collections::BTreeSet<String>,
    /// CG-6: capability_id -> the release tag last installed for it.
    pub installed_versions: std::collections::BTreeMap<String, String>,
}

#[derive(Debug, Clone)]
pub struct CodeGuardStateLoad {
    pub state: CodeGuardState,
    /// Set when the on-disk file was corrupt/invalid and defaults were used.
    pub warning: Option<String>,
}

/// Load `codeguard.json`; corrupt or invalid content degrades to defaults
/// with a warning — never a crash, matching `load_gui_state`'s contract.
pub fn load_codeguard_state(home: &Path) -> CodeGuardStateLoad {
    let path = home.join(CODEGUARD_STATE_FILE);
    let Some(text) = std::fs::read_to_string(&path).ok() else {
        return CodeGuardStateLoad {
            state: CodeGuardState::default(),
            warning: None,
        };
    };
    let raw: serde_json::Value = match serde_json::from_str(&text) {
        Ok(value) => value,
        Err(_) => {
            return CodeGuardStateLoad {
                state: CodeGuardState::default(),
                warning: Some(format!(
                    "{CODEGUARD_STATE_FILE} is not valid JSON — using defaults (file left untouched until next change)"
                )),
            };
        }
    };
    let Some(obj) = raw.as_object() else {
        return CodeGuardStateLoad {
            state: CodeGuardState::default(),
            warning: Some(format!(
                "{CODEGUARD_STATE_FILE} is not an object — using defaults"
            )),
        };
    };
    match obj.get("schemaVersion").and_then(|v| v.as_u64()) {
        Some(version) if version <= CODEGUARD_STATE_SCHEMA_VERSION => {}
        _ => {
            return CodeGuardStateLoad {
                state: CodeGuardState::default(),
                warning: Some(format!(
                    "{CODEGUARD_STATE_FILE} schemaVersion is unsupported — using defaults"
                )),
            };
        }
    }
    let enabled_agents = obj
        .get("enabledAgents")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str())
                // Only ids the registry still knows about — a retired agent
                // in an old state file is dropped silently, not carried
                // forward as a phantom opt-in nothing can act on.
                .filter(|id| get_agent_def(id).is_some())
                .map(String::from)
                .collect()
        })
        .unwrap_or_default();
    let installed_versions = obj
        .get("installedVersions")
        .and_then(|v| v.as_object())
        .map(|map| {
            map.iter()
                .filter(|(id, _)| get_agent_def(id).is_some())
                .filter_map(|(id, v)| v.as_str().map(|tag| (id.clone(), tag.to_string())))
                .collect()
        })
        .unwrap_or_default();
    CodeGuardStateLoad {
        state: CodeGuardState {
            enabled_agents,
            installed_versions,
        },
        warning: None,
    }
}

/// Persist `codeguard.json` deterministically (sorted by `BTreeSet`/`BTreeMap`
/// themselves, then `stable_stringify` sorts object keys).
pub fn save_codeguard_state(home: &Path, state: &CodeGuardState) -> std::io::Result<()> {
    let value = serde_json::json!({
        "schemaVersion": CODEGUARD_STATE_SCHEMA_VERSION,
        "enabledAgents": state.enabled_agents,
        "installedVersions": state.installed_versions,
    });
    write_ensured(&home.join(CODEGUARD_STATE_FILE), &stable_stringify(&value))?;
    Ok(())
}

/// CG-8's one explicit action per agent. Returns `true` when this call
/// actually changed the set (idempotent — opting in twice is a no-op the
/// second time, never an error).
pub fn opt_in(state: &mut CodeGuardState, capability_id: &str) -> bool {
    state.enabled_agents.insert(capability_id.to_string())
}

pub fn opt_out(state: &mut CodeGuardState, capability_id: &str) -> bool {
    state.enabled_agents.remove(capability_id)
}

pub fn is_opted_in(state: &CodeGuardState, capability_id: &str) -> bool {
    state.enabled_agents.contains(capability_id)
}

// ── CG-3 (MCP remainder) ────────────────────────────────────────────────
//
// Scoped honestly to what this session can verify, not guessed at for six
// agents at once. Claude Code's user-scope MCP registration is `~/.claude.json`
// under `mcpServers`, `{"type":"http","url":...}` — session-observed ground
// truth (this very machine's file), not assumed from a README. The other
// five agents' user-scope MCP config locations are NOT verified and are
// left as fog rather than guessed; `register_mcp_agent` returns `None` for
// them instead of writing to a location nobody has confirmed is real.
//
// ADEB does not deploy or run the CodeGuard MCP server itself — that is
// explicitly out of scope (CG-N never claimed it). MCP mode presumes the
// owner already has one reachable somewhere (self-hosted per CodeGuard's own
// docs) and asks ADEB only to register the agent against it and to fall
// back when it stops answering.

use crate::fsutil::deep_merge;

pub const MCP_SERVER_NAME: &str = "codeguard";

/// `None` for every agent this session has not verified a real user-scope
/// MCP config location for — see the module note above.
fn mcp_config_rel_path(agent_id: &str) -> Option<&'static str> {
    match agent_id {
        "claude-code" => Some(".claude.json"),
        _ => None,
    }
}

/// Register the CodeGuard MCP server for one agent at user scope. Additive —
/// routes through the same `deep_merge` every project-scope MCP registration
/// in this codebase already uses, so a pre-existing `mcpServers` entry for
/// anything else on this machine survives untouched (this machine's own
/// `~/.claude.json` already has two other servers registered — proof this
/// matters, not a hypothetical). Returns `None` when the agent has no
/// verified config location.
pub fn register_mcp_agent(
    def: &AgentCodeGuardDef,
    server_url: &str,
    deps: &CodeGuardDeps,
) -> Option<std::io::Result<WriteOutcome>> {
    let rel_path = mcp_config_rel_path(def.agent_id)?;
    let path = deps.home_dir.join(rel_path);
    let existing = std::fs::read_to_string(&path)
        .ok()
        .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
        .unwrap_or_else(|| serde_json::json!({}));
    let patch = serde_json::json!({
        "mcpServers": {
            MCP_SERVER_NAME: { "type": "http", "url": server_url }
        }
    });
    let merged = deep_merge(&existing, &patch);
    let content = stable_stringify(&merged);
    Some(write_with_provenance(&path, &content, None))
}

/// CG-5's health check: `GET /health` on the configured server, exactly the
/// endpoint CodeGuard's own server exposes (`{"status":"ok","version":...}`,
/// verified against its real `server.py` this session, not assumed).
/// Degrades to `false` on any failure — unreachable, non-200, malformed body.
pub fn mcp_server_healthy(deps: &CodeGuardDeps, server_url: &str) -> bool {
    let health_url = format!("{}/health", server_url.trim_end_matches('/'));
    let result = (deps.exec)(
        &[
            "curl".to_string(),
            "-sL".to_string(),
            "--fail".to_string(),
            "--max-time".to_string(),
            "5".to_string(),
            health_url,
        ],
        &ExecOpts::default(),
    );
    if result.code != 0 {
        return false;
    }
    serde_json::from_str::<serde_json::Value>(&result.stdout)
        .ok()
        .and_then(|v| v.get("status").and_then(|s| s.as_str()).map(str::to_string))
        .as_deref()
        == Some("ok")
}

/// CG-5: the orchestrator. An agent opted into MCP whose server has gone
/// unhealthy is never left silently unguarded — it falls back to the same
/// default rule/skill install path CG-3 already proved, so "MCP broke" and
/// "never installed" converge on the identical safe state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EnsureOutcome {
    /// MCP is opted in and healthy — nothing to do.
    McpHealthy,
    /// MCP was opted in but unhealthy; fell back to the default path.
    FellBackToDefault(WriteOutcome),
    /// Not in MCP mode; the default path was written directly.
    DefaultInstalled(WriteOutcome),
    /// This agent has no `user_scope_dir` to fall back into (ClaudePlugin
    /// agents use their own install action, not this generic fallback).
    NotApplicable,
}

#[allow(clippy::too_many_arguments)]
pub fn ensure_agent_secured(
    def: &AgentCodeGuardDef,
    mcp_opted_in: bool,
    server_url: Option<&str>,
    filename: &str,
    fallback_content: &str,
    deps: &CodeGuardDeps,
    prior: Option<&ProvenanceRecord>,
) -> EnsureOutcome {
    if mcp_opted_in {
        if let Some(url) = server_url {
            if mcp_server_healthy(deps, url) {
                return EnsureOutcome::McpHealthy;
            }
        }
        return match install_rule_or_skill(def, filename, fallback_content, deps, prior) {
            Some(Ok(outcome)) => EnsureOutcome::FellBackToDefault(outcome),
            _ => EnsureOutcome::NotApplicable,
        };
    }
    match install_rule_or_skill(def, filename, fallback_content, deps, prior) {
        Some(Ok(outcome)) => EnsureOutcome::DefaultInstalled(outcome),
        _ => EnsureOutcome::NotApplicable,
    }
}

// ── CG-6 (remainder): per-agent version tracking ────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VersionStatus {
    /// Installed tag matches the latest known upstream release.
    Current,
    /// Installed tag is older than the latest known upstream release.
    UpdateAvailable { installed: String, latest: String },
    /// Nothing recorded for this agent yet — never installed, or installed
    /// before version tracking existed. Not a failure state.
    Unknown,
}

/// The doctor-surfacing data itself: not a rendered CLI line (that is a thin
/// `ade doctor` wiring concern outside this module), but the exact fact such
/// a line would report, computed and independently testable.
pub fn version_status(
    state: &CodeGuardState,
    capability_id: &str,
    latest_tag: Option<&str>,
) -> VersionStatus {
    let Some(installed) = state.installed_versions.get(capability_id) else {
        return VersionStatus::Unknown;
    };
    match latest_tag {
        Some(latest) if latest != installed => VersionStatus::UpdateAvailable {
            installed: installed.clone(),
            latest: latest.to_string(),
        },
        _ => VersionStatus::Current,
    }
}

/// Record which tag is now installed for an agent — the write side of CG-6's
/// tracking half.
pub fn record_installed_version(state: &mut CodeGuardState, capability_id: &str, tag: &str) {
    state
        .installed_versions
        .insert(capability_id.to_string(), tag.to_string());
}

/// One agent's full reportable status — the queryable, tested surface
/// `ade doctor` / the Control Center's capability inventory would print.
/// Deliberately a plain function this module owns rather than a new row
/// wedged into `gui::inventory`'s `CAPABILITIES` array: that system's own
/// `CapabilityDef` has no `depends_on` gating or per-agent presence probe
/// today (a real, larger integration this session scoped out rather than
/// risk regressing a 1700-line, heavily-tested system on top of everything
/// else built here) — wiring THIS report into that UI is real but separate
/// follow-up work, not a gap in the data or logic itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodeGuardStatusRow {
    pub capability_id: &'static str,
    pub agent_id: &'static str,
    pub presence: PresenceState,
    pub version: VersionStatus,
}

/// CG-6's actual "surfaces update available" claim, in reportable form: one
/// row per known agent, presence and version status both derived from
/// already-tested primitives (`probe_presence`, `version_status`) — this
/// function adds no new detection logic of its own, only composes what
/// exists.
pub fn codeguard_status_report(
    deps: &CodeGuardDeps,
    state: &CodeGuardState,
    latest_tag: Option<&str>,
) -> Vec<CodeGuardStatusRow> {
    CODEGUARD_AGENTS
        .iter()
        .map(|def| CodeGuardStatusRow {
            capability_id: def.capability_id,
            agent_id: def.agent_id,
            presence: probe_presence(def, deps),
            version: version_status(state, def.capability_id, latest_tag),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::make_temp_dir;
    use std::sync::Arc;

    fn deps_with(
        which_present: &[&str],
        exec_rules: &[(&str, (i32, &str, &str))],
        home: &std::path::Path,
    ) -> CodeGuardDeps {
        let present: Vec<String> = which_present.iter().map(|s| s.to_string()).collect();
        let which: WhichFn = Arc::new(move |name| {
            if present.iter().any(|p| p == name) {
                Some(format!("/usr/bin/{name}"))
            } else {
                None
            }
        });
        let rules: Vec<(String, (i32, String, String))> = exec_rules
            .iter()
            .map(|(p, (c, o, e))| ((*p).to_string(), (*c, (*o).to_string(), (*e).to_string())))
            .collect();
        let exec: ExecFn = Arc::new(move |argv, _opts| {
            let joined = argv.join(" ");
            for (prefix, (code, stdout, stderr)) in &rules {
                // Matches testutil::fake_exec's own semantics: a rule may key
                // on a command's leading words (e.g. "claude plugin install")
                // or, for curl calls, on the URL sitting at the END of argv —
                // `contains` covers both without two matching strategies.
                if joined.starts_with(prefix.as_str()) || joined.contains(prefix.as_str()) {
                    return crate::types::ExecResult {
                        code: *code,
                        stdout: stdout.clone(),
                        stderr: stderr.clone(),
                    };
                }
            }
            crate::types::ExecResult {
                code: 127,
                stdout: String::new(),
                stderr: "not found".to_string(),
            }
        });
        CodeGuardDeps {
            home_dir: home.to_path_buf(),
            exec,
            which,
        }
    }

    // ── CG-1: depends_on gating ─────────────────────────────────────────

    #[test]
    fn every_agent_def_has_a_real_harness_cli_name() {
        for def in &CODEGUARD_AGENTS {
            assert!(
                !harness_cli_name(def.agent_id).is_empty(),
                "agent {} has no mapped harness CLI",
                def.agent_id
            );
        }
    }

    #[test]
    fn pi_has_no_codeguard_definition() {
        assert!(
            CODEGUARD_AGENTS.iter().all(|d| d.agent_id != "pi"),
            "Pi has no documented CodeGuard install path (ISA fog item) — must not appear"
        );
        assert_eq!(CODEGUARD_AGENTS.len(), 6);
    }

    #[test]
    fn absent_harness_gates_the_entry_inert_before_any_codeguard_probe() {
        let tmp = make_temp_dir("codeguard");
        // `which` reports nothing present at all — every entry must read
        // HarnessAbsent, never NotInstalled (which would imply the agent
        // exists and merely lacks CodeGuard).
        let deps = deps_with(&[], &[], tmp.as_path());
        for def in &CODEGUARD_AGENTS {
            assert_eq!(
                probe_presence(def, &deps),
                PresenceState::HarnessAbsent,
                "capability {} should be inert with no agents present",
                def.capability_id
            );
        }
    }

    #[test]
    fn get_agent_def_resolves_by_capability_id_and_rejects_unknown_ids() {
        assert!(get_agent_def("codeguard-cursor").is_some());
        assert!(get_agent_def("codeguard-pi").is_none());
        assert!(get_agent_def("totally-unknown").is_none());
    }

    // ── CG-2: presence read from agent state, never a codeguard binary ──

    #[test]
    fn claude_plugin_presence_is_read_from_plugin_list_json() {
        let tmp = make_temp_dir("codeguard");
        let deps = deps_with(
            &["claude"],
            &[(
                "claude plugin list --json",
                (0, r#"[{"name":"codeguard-security"}]"#, ""),
            )],
            tmp.as_path(),
        );
        let def = get_agent_def("codeguard-claude-code").unwrap();
        assert_eq!(probe_presence(def, &deps), PresenceState::Installed);
    }

    #[test]
    fn claude_plugin_absent_from_list_reads_not_installed() {
        let tmp = make_temp_dir("codeguard");
        let deps = deps_with(
            &["claude"],
            &[("claude plugin list --json", (0, "[]", ""))],
            tmp.as_path(),
        );
        let def = get_agent_def("codeguard-claude-code").unwrap();
        assert_eq!(probe_presence(def, &deps), PresenceState::NotInstalled);
    }

    #[test]
    fn rule_file_agent_presence_is_read_from_its_own_user_scope_directory() {
        let tmp = make_temp_dir("codeguard");
        let rules_dir = tmp.as_path().join(".cursor/rules");
        std::fs::create_dir_all(&rules_dir).unwrap();
        std::fs::write(rules_dir.join("codeguard-input-validation.md"), "# rule").unwrap();
        let deps = deps_with(&["cursor"], &[], tmp.as_path());
        let def = get_agent_def("codeguard-cursor").unwrap();
        assert_eq!(probe_presence(def, &deps), PresenceState::Installed);
    }

    #[test]
    fn rule_file_agent_with_unrelated_files_only_reads_not_installed() {
        let tmp = make_temp_dir("codeguard");
        let rules_dir = tmp.as_path().join(".cursor/rules");
        std::fs::create_dir_all(&rules_dir).unwrap();
        std::fs::write(rules_dir.join("my-own-style-rule.md"), "# not codeguard").unwrap();
        let deps = deps_with(&["cursor"], &[], tmp.as_path());
        let def = get_agent_def("codeguard-cursor").unwrap();
        assert_eq!(probe_presence(def, &deps), PresenceState::NotInstalled);
    }

    #[test]
    fn skill_file_agent_presence_is_read_from_its_own_skills_directory() {
        let tmp = make_temp_dir("codeguard");
        let skills_dir = tmp.as_path().join(".hermes/skills");
        std::fs::create_dir_all(skills_dir.join("codeguard-meta")).unwrap();
        std::fs::write(skills_dir.join("codeguard-meta/SKILL.md"), "# meta").unwrap();
        let deps = deps_with(&["hermes"], &[], tmp.as_path());
        let def = get_agent_def("codeguard-hermes").unwrap();
        assert_eq!(probe_presence(def, &deps), PresenceState::Installed);
    }

    #[test]
    fn missing_user_scope_directory_reads_not_installed_not_an_error() {
        let tmp = make_temp_dir("codeguard");
        // Agent present, but its rules directory was never created.
        let deps = deps_with(&["antigravity"], &[], tmp.as_path());
        let def = get_agent_def("codeguard-antigravity").unwrap();
        assert_eq!(probe_presence(def, &deps), PresenceState::NotInstalled);
    }

    // ── CG-3: install actions ────────────────────────────────────────────

    #[test]
    fn claude_plugin_install_runs_marketplace_add_then_plugin_install_in_order() {
        let tmp = make_temp_dir("codeguard");
        let deps = deps_with(
            &["claude"],
            &[
                (
                    &format!("claude plugin marketplace add {MARKETPLACE_SLUG}"),
                    (0, "marketplace added", ""),
                ),
                (
                    &format!("claude plugin install {PLUGIN_SLUG}"),
                    (0, "plugin installed", ""),
                ),
            ],
            tmp.as_path(),
        );
        assert_eq!(install_claude_plugin(&deps), InstallOutcome::Installed);
    }

    #[test]
    fn claude_plugin_install_fails_fast_on_marketplace_add_and_never_attempts_install() {
        let tmp = make_temp_dir("codeguard");
        // Only the marketplace-add rule exists; if install_claude_plugin ever
        // called plugin install anyway it would hit the fake_exec 127
        // fallback, not a controlled failure — the assertion on `step` proves
        // it stopped at the right place.
        let deps = deps_with(
            &["claude"],
            &[(
                &format!("claude plugin marketplace add {MARKETPLACE_SLUG}"),
                (1, "", "network unreachable"),
            )],
            tmp.as_path(),
        );
        match install_claude_plugin(&deps) {
            InstallOutcome::Failed { step, detail } => {
                assert_eq!(step, "marketplace add");
                assert_eq!(detail, "network unreachable");
            }
            other => panic!("expected Failed, got {other:?}"),
        }
    }

    #[test]
    fn claude_plugin_install_reports_the_install_step_when_marketplace_add_succeeds_but_install_fails(
    ) {
        let tmp = make_temp_dir("codeguard");
        let deps = deps_with(
            &["claude"],
            &[
                (
                    &format!("claude plugin marketplace add {MARKETPLACE_SLUG}"),
                    (0, "", ""),
                ),
                (
                    &format!("claude plugin install {PLUGIN_SLUG}"),
                    (1, "", "plugin not found"),
                ),
            ],
            tmp.as_path(),
        );
        match install_claude_plugin(&deps) {
            InstallOutcome::Failed { step, detail } => {
                assert_eq!(step, "plugin install");
                assert_eq!(detail, "plugin not found");
            }
            other => panic!("expected Failed, got {other:?}"),
        }
    }

    #[test]
    fn install_rule_or_skill_writes_into_the_agents_own_canonical_directory() {
        let tmp = make_temp_dir("codeguard");
        let deps = deps_with(&["cursor"], &[], tmp.as_path());
        let def = get_agent_def("codeguard-cursor").unwrap();
        let outcome = install_rule_or_skill(
            def,
            "codeguard-input-validation.md",
            "# rule body",
            &deps,
            None,
        )
        .expect("RuleFiles def must return Some")
        .unwrap();
        assert_eq!(outcome, WriteOutcome::Created);
        let written = std::fs::read_to_string(
            tmp.as_path()
                .join(".cursor/rules/codeguard-input-validation.md"),
        )
        .unwrap();
        assert_eq!(written, "# rule body");
    }

    #[test]
    fn install_rule_or_skill_creates_the_directory_when_absent() {
        let tmp = make_temp_dir("codeguard");
        // .hermes/skills does not exist yet anywhere under this fresh home.
        let deps = deps_with(&["hermes"], &[], tmp.as_path());
        let def = get_agent_def("codeguard-hermes").unwrap();
        let outcome = install_rule_or_skill(def, "SKILL.md", "# skill", &deps, None)
            .expect("SkillFiles def must return Some")
            .unwrap();
        assert_eq!(outcome, WriteOutcome::Created);
    }

    #[test]
    fn install_rule_or_skill_refuses_on_a_claude_plugin_def_instead_of_panicking() {
        let tmp = make_temp_dir("codeguard");
        let deps = deps_with(&["claude"], &[], tmp.as_path());
        let def = get_agent_def("codeguard-claude-code").unwrap();
        assert!(install_rule_or_skill(def, "irrelevant.md", "x", &deps, None).is_none());
    }

    #[test]
    fn install_rule_or_skill_respects_cg7_and_never_clobbers_a_user_edit() {
        let tmp = make_temp_dir("codeguard");
        let deps = deps_with(&["cursor"], &[], tmp.as_path());
        let def = get_agent_def("codeguard-cursor").unwrap();
        let path = tmp.as_path().join(".cursor/rules/codeguard-crypto.md");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, "# hand-edited by the user").unwrap();
        let prior = ProvenanceRecord {
            path: path.to_string_lossy().to_string(),
            installed_hash: sha256_hex("# original installed content"),
        };
        let outcome = install_rule_or_skill(
            def,
            "codeguard-crypto.md",
            "# upstream update",
            &deps,
            Some(&prior),
        )
        .unwrap()
        .unwrap();
        assert!(matches!(outcome, WriteOutcome::Conflict { .. }));
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "# hand-edited by the user"
        );
    }

    #[test]
    fn cg4_no_install_action_ever_touches_a_meta_prompt_file() {
        // CG-4's actual falsifier: run every install action this module can
        // perform against a fresh home, then assert no CLAUDE.md/AGENTS.md
        // exists anywhere under it. Not inferred from "the code doesn't
        // mention it" — proven by walking the resulting tree.
        let tmp = make_temp_dir("codeguard");
        let deps = deps_with(
            &[
                "claude",
                "cursor",
                "opencode",
                "antigravity",
                "hermes",
                "codex",
            ],
            &[
                (
                    &format!("claude plugin marketplace add {MARKETPLACE_SLUG}"),
                    (0, "", ""),
                ),
                (&format!("claude plugin install {PLUGIN_SLUG}"), (0, "", "")),
            ],
            tmp.as_path(),
        );

        install_claude_plugin(&deps);
        for def in &CODEGUARD_AGENTS {
            if def.user_scope_dir.is_some() {
                install_rule_or_skill(def, "codeguard-rule.md", "# content", &deps, None)
                    .map(|r| r.unwrap());
            }
        }

        fn walk(dir: &Path, hits: &mut Vec<PathBuf>) {
            let Ok(entries) = std::fs::read_dir(dir) else {
                return;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    walk(&path, hits);
                } else {
                    let name = entry.file_name().to_string_lossy().to_lowercase();
                    if name == "claude.md" || name == "agents.md" {
                        hits.push(path);
                    }
                }
            }
        }
        let mut hits = Vec::new();
        walk(tmp.as_path(), &mut hits);
        assert!(
            hits.is_empty(),
            "install actions touched a meta-prompt file: {hits:?}"
        );
    }

    // ── CG-3 (remainder) / CG-6: live fetch ──────────────────────────────

    #[test]
    fn fetch_latest_release_tag_parses_the_real_github_releases_shape() {
        let tmp = make_temp_dir("codeguard");
        let deps = deps_with(
            &[],
            &[(
                CODEGUARD_RELEASES_API,
                (0, r#"{"tag_name":"v1.3.0","name":"v1.3.0"}"#, ""),
            )],
            tmp.as_path(),
        );
        assert_eq!(fetch_latest_release_tag(&deps), Some("v1.3.0".to_string()));
    }

    #[test]
    fn fetch_latest_release_tag_degrades_to_none_on_network_failure() {
        let tmp = make_temp_dir("codeguard");
        // curl itself fails (offline, DNS failure, timeout) — no rule matches
        // the fake_exec table, so it falls through to the 127 default.
        let deps = deps_with(&[], &[], tmp.as_path());
        assert_eq!(fetch_latest_release_tag(&deps), None);
    }

    #[test]
    fn fetch_latest_release_tag_degrades_to_none_on_malformed_or_rate_limited_response() {
        let tmp = make_temp_dir("codeguard");
        // GitHub's real rate-limit body has no tag_name field at all.
        let deps = deps_with(
            &[],
            &[(
                CODEGUARD_RELEASES_API,
                (0, r#"{"message":"API rate limit exceeded"}"#, ""),
            )],
            tmp.as_path(),
        );
        assert_eq!(fetch_latest_release_tag(&deps), None);
    }

    #[test]
    fn fetch_latest_release_tag_degrades_to_none_on_non_json_body() {
        let tmp = make_temp_dir("codeguard");
        let deps = deps_with(
            &[],
            &[(
                CODEGUARD_RELEASES_API,
                (0, "<html>502 Bad Gateway</html>", ""),
            )],
            tmp.as_path(),
        );
        assert_eq!(fetch_latest_release_tag(&deps), None);
    }

    #[test]
    fn fetch_rule_content_pins_the_url_to_the_given_tag_not_main() {
        let tmp = make_temp_dir("codeguard");
        let url = format!(
            "{CODEGUARD_RAW_BASE}/v1.3.0/sources/rules/core/codeguard-0-input-validation.md"
        );
        let deps = deps_with(
            &[],
            &[(&url, (0, "# input validation rule", ""))],
            tmp.as_path(),
        );
        assert_eq!(
            fetch_rule_content(&deps, "v1.3.0", "codeguard-0-input-validation.md"),
            Some("# input validation rule".to_string())
        );
    }

    #[test]
    fn fetch_rule_content_degrades_to_none_on_a_404() {
        let tmp = make_temp_dir("codeguard");
        // curl --fail exits non-zero on 4xx/5xx; no rule matches, falls to 127.
        let deps = deps_with(&[], &[], tmp.as_path());
        assert_eq!(
            fetch_rule_content(&deps, "v1.3.0", "does-not-exist.md"),
            None
        );
    }

    #[test]
    fn fetch_rule_content_degrades_to_none_on_an_empty_body() {
        let tmp = make_temp_dir("codeguard");
        let url = format!("{CODEGUARD_RAW_BASE}/v1.3.0/sources/rules/core/empty.md");
        let deps = deps_with(&[], &[(&url, (0, "", ""))], tmp.as_path());
        assert_eq!(fetch_rule_content(&deps, "v1.3.0", "empty.md"), None);
    }

    #[test]
    fn different_tags_produce_different_urls_reproducibility_is_pinned_not_floating() {
        let tmp = make_temp_dir("codeguard");
        let v1_url =
            format!("{CODEGUARD_RAW_BASE}/v1.2.0/sources/rules/core/codeguard-0-crypto.md");
        let v2_url =
            format!("{CODEGUARD_RAW_BASE}/v1.3.0/sources/rules/core/codeguard-0-crypto.md");
        let deps = deps_with(
            &[],
            &[
                (&v1_url, (0, "# v1.2.0 content", "")),
                (&v2_url, (0, "# v1.3.0 content", "")),
            ],
            tmp.as_path(),
        );
        assert_eq!(
            fetch_rule_content(&deps, "v1.2.0", "codeguard-0-crypto.md"),
            Some("# v1.2.0 content".to_string())
        );
        assert_eq!(
            fetch_rule_content(&deps, "v1.3.0", "codeguard-0-crypto.md"),
            Some("# v1.3.0 content".to_string())
        );
    }

    // ── CG-7: provenance — never overwrite a user-modified file ─────────

    #[test]
    fn first_write_with_no_prior_record_creates_the_file() {
        let tmp = make_temp_dir("codeguard");
        let path = tmp.as_path().join("rule.md");
        let outcome = write_with_provenance(&path, "content-v1", None).unwrap();
        assert_eq!(outcome, WriteOutcome::Created);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "content-v1");
    }

    #[test]
    fn matching_prior_hash_allows_the_update() {
        let tmp = make_temp_dir("codeguard");
        let path = tmp.as_path().join("rule.md");
        std::fs::write(&path, "content-v1").unwrap();
        let prior = ProvenanceRecord {
            path: path.to_string_lossy().to_string(),
            installed_hash: sha256_hex("content-v1"),
        };
        let outcome = write_with_provenance(&path, "content-v2", Some(&prior)).unwrap();
        assert_eq!(outcome, WriteOutcome::Updated { changed: true });
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "content-v2");
    }

    #[test]
    fn user_edited_file_is_never_overwritten_and_is_reported_as_conflict() {
        let tmp = make_temp_dir("codeguard");
        let path = tmp.as_path().join("rule.md");
        // ADE installed "content-v1"; the user hand-edited it afterward.
        std::fs::write(&path, "content-v1-but-i-tweaked-it").unwrap();
        let prior = ProvenanceRecord {
            path: path.to_string_lossy().to_string(),
            installed_hash: sha256_hex("content-v1"),
        };
        let outcome =
            write_with_provenance(&path, "content-v2-upstream-update", Some(&prior)).unwrap();
        match outcome {
            WriteOutcome::Conflict { existing_hash } => {
                assert_eq!(existing_hash, sha256_hex("content-v1-but-i-tweaked-it"));
            }
            other => panic!("expected Conflict, got {other:?}"),
        }
        // The file on disk is untouched — this is the whole point of CG-7.
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "content-v1-but-i-tweaked-it"
        );
    }

    #[test]
    fn a_file_deleted_since_last_install_is_treated_as_a_fresh_write_not_a_conflict() {
        let tmp = make_temp_dir("codeguard");
        let path = tmp.as_path().join("rule.md");
        // No file on disk at all, but a prior record exists (e.g. from a
        // previous machine or a manually-cleared directory).
        let prior = ProvenanceRecord {
            path: path.to_string_lossy().to_string(),
            installed_hash: sha256_hex("content-v1"),
        };
        let outcome = write_with_provenance(&path, "content-v2", Some(&prior)).unwrap();
        assert_eq!(outcome, WriteOutcome::Updated { changed: true });
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "content-v2");
    }

    #[test]
    fn identical_content_reports_updated_with_changed_false() {
        let tmp = make_temp_dir("codeguard");
        let path = tmp.as_path().join("rule.md");
        std::fs::write(&path, "same-content").unwrap();
        let prior = ProvenanceRecord {
            path: path.to_string_lossy().to_string(),
            installed_hash: sha256_hex("same-content"),
        };
        let outcome = write_with_provenance(&path, "same-content", Some(&prior)).unwrap();
        assert_eq!(outcome, WriteOutcome::Updated { changed: false });
    }

    // ── CG-8: machine-scoped opt-in state ────────────────────────────────

    #[test]
    fn opt_in_is_off_by_default_on_a_fresh_machine() {
        let tmp = make_temp_dir("codeguard");
        let load = load_codeguard_state(tmp.as_path());
        assert!(load.warning.is_none());
        assert!(load.state.enabled_agents.is_empty());
        assert!(!is_opted_in(&load.state, "codeguard-cursor"));
    }

    #[test]
    fn opt_in_then_save_then_load_round_trips() {
        let tmp = make_temp_dir("codeguard");
        let mut state = CodeGuardState::default();
        assert!(opt_in(&mut state, "codeguard-cursor"));
        assert!(opt_in(&mut state, "codeguard-claude-code"));
        save_codeguard_state(tmp.as_path(), &state).unwrap();

        let reloaded = load_codeguard_state(tmp.as_path()).state;
        assert!(is_opted_in(&reloaded, "codeguard-cursor"));
        assert!(is_opted_in(&reloaded, "codeguard-claude-code"));
        assert!(!is_opted_in(&reloaded, "codeguard-hermes"));
    }

    #[test]
    fn opt_in_twice_is_idempotent_not_an_error() {
        let mut state = CodeGuardState::default();
        assert!(opt_in(&mut state, "codeguard-cursor"));
        assert!(!opt_in(&mut state, "codeguard-cursor")); // second call: no change
        assert_eq!(state.enabled_agents.len(), 1);
    }

    #[test]
    fn opt_out_removes_and_reports_whether_it_changed_anything() {
        let mut state = CodeGuardState::default();
        opt_in(&mut state, "codeguard-cursor");
        assert!(opt_out(&mut state, "codeguard-cursor"));
        assert!(!is_opted_in(&state, "codeguard-cursor"));
        assert!(!opt_out(&mut state, "codeguard-cursor")); // already out
    }

    #[test]
    fn corrupt_state_file_degrades_to_defaults_with_a_warning_never_a_crash() {
        let tmp = make_temp_dir("codeguard");
        std::fs::write(tmp.as_path().join(CODEGUARD_STATE_FILE), "{not json").unwrap();
        let load = load_codeguard_state(tmp.as_path());
        assert!(load.warning.is_some());
        assert!(load.state.enabled_agents.is_empty());
    }

    #[test]
    fn unsupported_schema_version_degrades_to_defaults() {
        let tmp = make_temp_dir("codeguard");
        std::fs::write(
            tmp.as_path().join(CODEGUARD_STATE_FILE),
            r#"{"schemaVersion":999,"enabledAgents":["codeguard-cursor"]}"#,
        )
        .unwrap();
        let load = load_codeguard_state(tmp.as_path());
        assert!(load.warning.is_some());
        assert!(load.state.enabled_agents.is_empty());
    }

    #[test]
    fn a_retired_agent_id_in_an_old_state_file_is_dropped_silently() {
        let tmp = make_temp_dir("codeguard");
        std::fs::write(
            tmp.as_path().join(CODEGUARD_STATE_FILE),
            r#"{"schemaVersion":1,"enabledAgents":["codeguard-cursor","codeguard-retired-agent"]}"#,
        )
        .unwrap();
        let load = load_codeguard_state(tmp.as_path());
        assert!(load.warning.is_none()); // not an error — just a stale entry
        assert!(is_opted_in(&load.state, "codeguard-cursor"));
        assert!(!load
            .state
            .enabled_agents
            .contains("codeguard-retired-agent"));
    }

    #[test]
    fn saved_state_is_deterministic_json() {
        let tmp = make_temp_dir("codeguard");
        let mut state = CodeGuardState::default();
        opt_in(&mut state, "codeguard-hermes");
        opt_in(&mut state, "codeguard-cursor");
        save_codeguard_state(tmp.as_path(), &state).unwrap();
        let bytes1 = std::fs::read_to_string(tmp.as_path().join(CODEGUARD_STATE_FILE)).unwrap();
        save_codeguard_state(tmp.as_path(), &state).unwrap();
        let bytes2 = std::fs::read_to_string(tmp.as_path().join(CODEGUARD_STATE_FILE)).unwrap();
        assert_eq!(bytes1, bytes2);
        // BTreeSet order, not insertion order — cursor before hermes.
        assert!(bytes1.find("cursor").unwrap() < bytes1.find("hermes").unwrap());
    }

    // ── CG-9 / CG-10: cross-repo silence, verify isolation ───────────────

    #[test]
    fn cg9_cg10_opt_in_action_never_touches_any_repo_files() {
        // The decisive structural fact, proven rather than argued: build a
        // real repo fixture (the same one every module test in this crate
        // uses), snapshot it byte-for-byte, perform every CG-8 write this
        // module can do against a COMPLETELY SEPARATE machine-home
        // directory, then assert the repo fixture is unchanged. If any
        // codeguard.rs function ever touched a repo's `.ade/` tree, CLAUDE.md,
        // AGENTS.md, or ade.lock.json, this test would catch it — it is not
        // an inference from "the module takes no Ctx parameter", it is a
        // real before/after diff.
        let repo = make_temp_dir("codeguard-cg9-repo");
        std::fs::write(repo.as_path().join("CLAUDE.md"), "# repo instructions").unwrap();
        std::fs::write(repo.as_path().join("AGENTS.md"), "# agent instructions").unwrap();
        std::fs::create_dir_all(repo.as_path().join(".ade/policy")).unwrap();
        std::fs::write(
            repo.as_path().join(".ade/policy/sandbox.json"),
            r#"{"enforcement":"advisory"}"#,
        )
        .unwrap();
        std::fs::write(repo.as_path().join("ade.lock.json"), r#"{"files":{}}"#).unwrap();

        fn snapshot(dir: &Path) -> Vec<(String, String)> {
            let mut out = Vec::new();
            fn walk(dir: &Path, root: &Path, out: &mut Vec<(String, String)>) {
                for entry in std::fs::read_dir(dir).unwrap().flatten() {
                    let path = entry.path();
                    if path.is_dir() {
                        walk(&path, root, out);
                    } else {
                        let rel = path
                            .strip_prefix(root)
                            .unwrap()
                            .to_string_lossy()
                            .to_string();
                        out.push((rel, std::fs::read_to_string(&path).unwrap_or_default()));
                    }
                }
            }
            walk(dir, dir, &mut out);
            out.sort();
            out
        }
        let before = snapshot(repo.as_path());

        // A completely separate machine-home directory — the only place
        // CG-8 is allowed to write anything.
        let machine_home = make_temp_dir("codeguard-cg9-home");
        let mut state = CodeGuardState::default();
        opt_in(&mut state, "codeguard-cursor");
        opt_in(&mut state, "codeguard-claude-code");
        save_codeguard_state(machine_home.as_path(), &state).unwrap();
        let _ = load_codeguard_state(machine_home.as_path());

        let after = snapshot(repo.as_path());
        assert_eq!(
            before, after,
            "a machine-scoped CodeGuard action touched repo-scoped files"
        );
    }

    // ── CG-3 (MCP remainder) ──────────────────────────────────────────────

    #[test]
    fn register_mcp_agent_writes_the_verified_claude_json_shape() {
        let tmp = make_temp_dir("codeguard");
        let deps = deps_with(&[], &[], tmp.as_path());
        let def = get_agent_def("codeguard-claude-code").unwrap();
        let outcome = register_mcp_agent(def, "http://localhost:8080/mcp", &deps)
            .expect("claude-code has a verified user-scope path")
            .unwrap();
        assert_eq!(outcome, WriteOutcome::Created);
        let written: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(tmp.as_path().join(".claude.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(
            written["mcpServers"]["codeguard"],
            serde_json::json!({"type": "http", "url": "http://localhost:8080/mcp"})
        );
    }

    #[test]
    fn register_mcp_agent_preserves_pre_existing_unrelated_servers() {
        // This exact scenario is not hypothetical — this machine's own
        // ~/.claude.json has other MCP servers registered alongside whatever
        // this feature adds.
        let tmp = make_temp_dir("codeguard");
        std::fs::write(
            tmp.as_path().join(".claude.json"),
            r#"{"mcpServers":{"openspace":{"type":"stdio","command":"/x"}},"numStartups":5}"#,
        )
        .unwrap();
        let deps = deps_with(&[], &[], tmp.as_path());
        let def = get_agent_def("codeguard-claude-code").unwrap();
        register_mcp_agent(def, "http://localhost:8080/mcp", &deps)
            .unwrap()
            .unwrap();
        let written: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(tmp.as_path().join(".claude.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(written["mcpServers"]["openspace"]["type"], "stdio");
        assert_eq!(written["mcpServers"]["codeguard"]["type"], "http");
        assert_eq!(written["numStartups"], 5);
    }

    #[test]
    fn register_mcp_agent_returns_none_for_an_agent_with_no_verified_config_path() {
        let tmp = make_temp_dir("codeguard");
        let deps = deps_with(&[], &[], tmp.as_path());
        let def = get_agent_def("codeguard-cursor").unwrap();
        assert!(register_mcp_agent(def, "http://localhost:8080/mcp", &deps).is_none());
    }

    #[test]
    fn mcp_server_healthy_reads_the_real_health_endpoint_shape() {
        let tmp = make_temp_dir("codeguard");
        let deps = deps_with(
            &[],
            &[(
                "http://localhost:8080/health",
                (0, r#"{"status":"ok","version":"0.1.0"}"#, ""),
            )],
            tmp.as_path(),
        );
        assert!(mcp_server_healthy(&deps, "http://localhost:8080"));
    }

    #[test]
    fn mcp_server_healthy_is_false_when_unreachable() {
        let tmp = make_temp_dir("codeguard");
        let deps = deps_with(&[], &[], tmp.as_path());
        assert!(!mcp_server_healthy(&deps, "http://localhost:8080"));
    }

    #[test]
    fn mcp_server_healthy_is_false_on_a_non_ok_status() {
        let tmp = make_temp_dir("codeguard");
        let deps = deps_with(
            &[],
            &[(
                "http://localhost:8080/health",
                (0, r#"{"status":"degraded"}"#, ""),
            )],
            tmp.as_path(),
        );
        assert!(!mcp_server_healthy(&deps, "http://localhost:8080"));
    }

    // ── CG-5: fallback orchestrator ───────────────────────────────────────

    #[test]
    fn ensure_agent_secured_does_nothing_when_mcp_is_healthy() {
        let tmp = make_temp_dir("codeguard");
        let deps = deps_with(
            &["cursor"],
            &[(
                "http://localhost:8080/health",
                (0, r#"{"status":"ok"}"#, ""),
            )],
            tmp.as_path(),
        );
        let def = get_agent_def("codeguard-cursor").unwrap();
        let outcome = ensure_agent_secured(
            def,
            true,
            Some("http://localhost:8080"),
            "codeguard-rule.md",
            "# fallback content",
            &deps,
            None,
        );
        assert_eq!(outcome, EnsureOutcome::McpHealthy);
        // Nothing was written — the whole point of "healthy means no-op".
        assert!(!tmp
            .as_path()
            .join(".cursor/rules/codeguard-rule.md")
            .exists());
    }

    #[test]
    fn ensure_agent_secured_falls_back_when_mcp_is_unhealthy_and_never_leaves_the_agent_unguarded()
    {
        let tmp = make_temp_dir("codeguard");
        // No health-endpoint rule registered — the server is unreachable.
        let deps = deps_with(&["cursor"], &[], tmp.as_path());
        let def = get_agent_def("codeguard-cursor").unwrap();
        let outcome = ensure_agent_secured(
            def,
            true,
            Some("http://localhost:8080"),
            "codeguard-rule.md",
            "# fallback content",
            &deps,
            None,
        );
        assert_eq!(
            outcome,
            EnsureOutcome::FellBackToDefault(WriteOutcome::Created)
        );
        assert_eq!(
            std::fs::read_to_string(tmp.as_path().join(".cursor/rules/codeguard-rule.md")).unwrap(),
            "# fallback content"
        );
    }

    #[test]
    fn ensure_agent_secured_installs_the_default_path_directly_when_not_in_mcp_mode() {
        let tmp = make_temp_dir("codeguard");
        let deps = deps_with(&["hermes"], &[], tmp.as_path());
        let def = get_agent_def("codeguard-hermes").unwrap();
        let outcome = ensure_agent_secured(
            def,
            false,
            None,
            "SKILL.md",
            "# default content",
            &deps,
            None,
        );
        assert_eq!(
            outcome,
            EnsureOutcome::DefaultInstalled(WriteOutcome::Created)
        );
    }

    #[test]
    fn ensure_agent_secured_respects_cg7_even_on_the_fallback_path() {
        // A user hand-edited the fallback file; MCP then breaks. The
        // fallback must not clobber the edit — CG-7 applies uniformly, not
        // only to the non-MCP default path.
        let tmp = make_temp_dir("codeguard");
        let deps = deps_with(&["cursor"], &[], tmp.as_path());
        let def = get_agent_def("codeguard-cursor").unwrap();
        let path = tmp.as_path().join(".cursor/rules/codeguard-rule.md");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, "# hand-edited").unwrap();
        let prior = ProvenanceRecord {
            path: path.to_string_lossy().to_string(),
            installed_hash: sha256_hex("# original"),
        };
        let outcome = ensure_agent_secured(
            def,
            true,
            Some("http://localhost:8080"),
            "codeguard-rule.md",
            "# upstream update",
            &deps,
            Some(&prior),
        );
        assert!(matches!(
            outcome,
            EnsureOutcome::FellBackToDefault(WriteOutcome::Conflict { .. })
        ));
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "# hand-edited");
    }

    // ── CG-6 (remainder): version tracking ───────────────────────────────

    #[test]
    fn version_status_is_unknown_when_nothing_is_recorded() {
        let state = CodeGuardState::default();
        assert_eq!(
            version_status(&state, "codeguard-cursor", Some("v1.3.0")),
            VersionStatus::Unknown
        );
    }

    #[test]
    fn version_status_is_current_when_tags_match() {
        let mut state = CodeGuardState::default();
        record_installed_version(&mut state, "codeguard-cursor", "v1.3.0");
        assert_eq!(
            version_status(&state, "codeguard-cursor", Some("v1.3.0")),
            VersionStatus::Current
        );
    }

    #[test]
    fn version_status_reports_update_available_when_tags_differ() {
        let mut state = CodeGuardState::default();
        record_installed_version(&mut state, "codeguard-cursor", "v1.2.0");
        assert_eq!(
            version_status(&state, "codeguard-cursor", Some("v1.3.0")),
            VersionStatus::UpdateAvailable {
                installed: "v1.2.0".to_string(),
                latest: "v1.3.0".to_string(),
            }
        );
    }

    #[test]
    fn version_status_stays_current_when_latest_is_unknown_rather_than_falsely_flagging_an_update()
    {
        // fetch_latest_release_tag degraded to None (network down) — that
        // must never read as "an update is available" with no real basis.
        let mut state = CodeGuardState::default();
        record_installed_version(&mut state, "codeguard-cursor", "v1.2.0");
        assert_eq!(
            version_status(&state, "codeguard-cursor", None),
            VersionStatus::Current
        );
    }

    #[test]
    fn recorded_versions_round_trip_through_save_and_load() {
        let tmp = make_temp_dir("codeguard");
        let mut state = CodeGuardState::default();
        record_installed_version(&mut state, "codeguard-cursor", "v1.3.0");
        record_installed_version(&mut state, "codeguard-hermes", "v1.2.0");
        save_codeguard_state(tmp.as_path(), &state).unwrap();
        let reloaded = load_codeguard_state(tmp.as_path()).state;
        assert_eq!(
            version_status(&reloaded, "codeguard-cursor", Some("v1.3.0")),
            VersionStatus::Current
        );
        assert_eq!(
            version_status(&reloaded, "codeguard-hermes", Some("v1.3.0")),
            VersionStatus::UpdateAvailable {
                installed: "v1.2.0".to_string(),
                latest: "v1.3.0".to_string(),
            }
        );
    }

    #[test]
    fn a_retired_agent_id_in_installed_versions_is_dropped_silently_too() {
        let tmp = make_temp_dir("codeguard");
        std::fs::write(
            tmp.as_path().join(CODEGUARD_STATE_FILE),
            r#"{"schemaVersion":1,"enabledAgents":[],"installedVersions":{"codeguard-cursor":"v1.3.0","codeguard-retired":"v0.9.0"}}"#,
        )
        .unwrap();
        let state = load_codeguard_state(tmp.as_path()).state;
        assert!(state.installed_versions.contains_key("codeguard-cursor"));
        assert!(!state.installed_versions.contains_key("codeguard-retired"));
    }

    // ── CG-6: the reportable status surface ──────────────────────────────

    #[test]
    fn status_report_covers_every_known_agent_exactly_once() {
        let tmp = make_temp_dir("codeguard");
        let deps = deps_with(&[], &[], tmp.as_path());
        let rows = codeguard_status_report(&deps, &CodeGuardState::default(), None);
        assert_eq!(rows.len(), CODEGUARD_AGENTS.len());
        let ids: std::collections::BTreeSet<_> = rows.iter().map(|r| r.capability_id).collect();
        for def in &CODEGUARD_AGENTS {
            assert!(ids.contains(def.capability_id));
        }
    }

    #[test]
    fn status_report_reflects_real_presence_and_version_together() {
        let tmp = make_temp_dir("codeguard");
        let rules_dir = tmp.as_path().join(".cursor/rules");
        std::fs::create_dir_all(&rules_dir).unwrap();
        std::fs::write(rules_dir.join("codeguard-crypto.md"), "# rule").unwrap();
        let deps = deps_with(&["cursor"], &[], tmp.as_path());

        let mut state = CodeGuardState::default();
        record_installed_version(&mut state, "codeguard-cursor", "v1.2.0");

        let rows = codeguard_status_report(&deps, &state, Some("v1.3.0"));
        let cursor_row = rows
            .iter()
            .find(|r| r.capability_id == "codeguard-cursor")
            .unwrap();
        assert_eq!(cursor_row.presence, PresenceState::Installed);
        assert_eq!(
            cursor_row.version,
            VersionStatus::UpdateAvailable {
                installed: "v1.2.0".to_string(),
                latest: "v1.3.0".to_string(),
            }
        );

        // A different agent, never installed on this fresh machine.
        let hermes_row = rows
            .iter()
            .find(|r| r.capability_id == "codeguard-hermes")
            .unwrap();
        assert_eq!(hermes_row.presence, PresenceState::HarnessAbsent);
        assert_eq!(hermes_row.version, VersionStatus::Unknown);
    }
}
