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

// ── CG-7: provenance — never overwrite a user-modified file ────────────────

use crate::fsutil::{sha256_hex, write_ensured};

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
                if joined.starts_with(prefix.as_str()) {
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
}
