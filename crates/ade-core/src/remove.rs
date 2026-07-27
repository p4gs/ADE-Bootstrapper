//! `ade remove` — withdraw ADE from a repository (ISC-210..218).
//!
//! A tool that writes into someone's repo owes them a clean way out. Without
//! this, uninstalling means hand-deleting `.ade/`, `ade.json`, `ade.lock.json`
//! and then performing surgery on CLAUDE.md / AGENTS.md / settings.json to pull
//! ADE's content back out without damaging your own — which is exactly the
//! error-prone editing the managed-block engine exists to prevent.
//!
//! The governing rule is the same one `apply` follows, pointed the other way:
//! **never destroy what we cannot prove we wrote.** A hand-edited ADE file, a
//! block with no ADE provenance line, a file planted under `.ade/` — each is
//! KEPT and reported, never deleted. The removal is driven by the lockfile,
//! which already enumerates the whole ADE-owned tree with content hashes, so
//! "did we write this, and is it still what we wrote?" is answerable per file
//! rather than guessed from paths.

use crate::config::CONFIG_FILE;
use crate::fsutil::{read_if_exists, sha256_hex, stable_stringify, subtract_json};
use crate::harness::adapters::get_adapter;
use crate::harness::claude::{CLAUDE_SETTINGS_PATH, MCP_CONFIG_PATH};
use crate::instructions::{LOCAL_INSTRUCTIONS_PATH, LOCAL_INSTRUCTIONS_STUB};
use crate::lockfile::{load_lockfile, scan_ade_tree, LOCKFILE_NAME};
use crate::managed::{remove_managed_block, RemoveResult};
use crate::modules::approval_gates::{CLAUDE_ASK_RULES, CLAUDE_DENY_RULES};
use crate::modules::memory::{MCP_SERVER_NAME, MEMORY_STORE_PATH};
use crate::modules::observability::{AUDIT_HOOK_COMMAND, AUDIT_LOG_PATH, AUDIT_README_PATH};
use crate::modules::sandbox::CLAUDE_DENY_READ;
use crate::modules::secrets::{
    pre_commit_config, CHAINED_HOOK_NAME, GITIGNORE_LINES, HOOK_MARKER, PRECOMMIT_CONFIG_PATH,
};
use crate::types::Ctx;
use serde_json::json;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RemoveOutcome {
    /// File deleted (we wrote it and it is still exactly what we wrote).
    Deleted,
    /// Our managed block / merged entries / appended lines withdrawn; file kept.
    Excised,
    /// Deliberately left in place — reason in `detail`. Never a failure.
    Kept,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoveAction {
    /// Repo-relative path, POSIX separators.
    pub path: String,
    pub outcome: RemoveOutcome,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoveReport {
    /// True when the plan was actually executed (false for a dry run).
    pub executed: bool,
    pub actions: Vec<RemoveAction>,
}

impl RemoveReport {
    pub fn deleted(&self) -> usize {
        self.count(RemoveOutcome::Deleted)
    }
    pub fn excised(&self) -> usize {
        self.count(RemoveOutcome::Excised)
    }
    pub fn kept(&self) -> usize {
        self.count(RemoveOutcome::Kept)
    }
    fn count(&self, outcome: RemoveOutcome) -> usize {
        self.actions
            .iter()
            .filter(|action| action.outcome == outcome)
            .count()
    }
    /// Nothing of ADE's was found — a second `ade remove` on a clean repo.
    pub fn is_noop(&self) -> bool {
        self.deleted() == 0 && self.excised() == 0
    }
}

/// Exactly the patch ADE merges into `.claude/settings.json`, rebuilt from the
/// same constants the modules merge. Drift here is caught by the round-trip
/// test, which fails if a module starts merging something removal doesn't know.
fn claude_settings_patch() -> serde_json::Value {
    json!({
        "permissions": {
            "deny": CLAUDE_DENY_READ
                .iter()
                .chain(CLAUDE_DENY_RULES.iter())
                .collect::<Vec<_>>(),
            "ask": CLAUDE_ASK_RULES,
        },
        "hooks": {
            "PostToolUse": [{
                "matcher": "*",
                "hooks": [{ "type": "command", "command": AUDIT_HOOK_COMMAND }],
            }],
        },
    })
}

fn mcp_patch() -> serde_json::Value {
    json!({ "mcpServers": { MCP_SERVER_NAME: {
        "command": "npx",
        "args": ["-y", "openmemory"],
        "env": {},
    }}})
}

/// Files ADE co-owns via managed markers, for the harnesses this repo configures.
fn instruction_targets(ctx: &Ctx) -> Vec<String> {
    let mut paths: Vec<String> = Vec::new();
    for harness in &ctx.config.harnesses {
        if let Some(adapter) = get_adapter(harness) {
            let path = adapter.instruction_file.to_string();
            if !paths.contains(&path) {
                paths.push(path);
            }
        }
    }
    paths
}

/// Remove ADE's appended block from a `.gitignore`-style file.
///
/// `ensure_lines` writes `# <label>` followed by its lines; withdrawing means
/// dropping that comment and the OWNED lines directly under it, and nothing
/// else — a user line that happens to sit in the block is left behind.
fn strip_ensure_lines_block(existing: &str, label: &str, owned: &[&str]) -> Option<String> {
    let header = format!("# {label}");
    let lines: Vec<&str> = existing.split('\n').collect();
    let start = lines.iter().position(|line| line.trim() == header)?;
    let mut end = start + 1;
    while end < lines.len() && owned.contains(&lines[end].trim()) {
        end += 1;
    }
    let mut kept: Vec<&str> = Vec::new();
    kept.extend_from_slice(&lines[..start]);
    kept.extend_from_slice(&lines[end..]);
    let mut out = kept.join("\n");
    while out.ends_with("\n\n") {
        out.pop();
    }
    if out.trim().is_empty() {
        out = String::new();
    }
    Some(out)
}

/// Every `ensure_lines` block ADE appends to `.gitignore`, by the label it
/// writes and the exact lines it owns under that label.
fn gitignore_blocks() -> Vec<(&'static str, Vec<&'static str>)> {
    vec![
        ("ADE Bootstrapper — observability", vec![".ade/audit/"]),
        (
            "ADE Bootstrapper — agent memory (sensitive, never committed)",
            vec![MEMORY_STORE_PATH],
        ),
        (
            "ADE Bootstrapper — secrets hygiene",
            GITIGNORE_LINES.to_vec(),
        ),
    ]
}

/// What the plan intends to do to a path. Computed BEFORE anything is written
/// so a dry run and a real run cannot disagree: the real run is the dry run
/// plus execution, never a second, differently-informed pass over a tree that
/// has already changed underneath it.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Mutation {
    Delete,
    Write(String),
    /// Move a file back over another — how a chained git hook is restored.
    Restore {
        from: String,
        to: String,
    },
    None,
}

struct PlannedAction {
    action: RemoveAction,
    mutation: Mutation,
}

fn plan(
    actions: &mut Vec<PlannedAction>,
    path: &str,
    outcome: RemoveOutcome,
    detail: impl Into<String>,
    mutation: Mutation,
) {
    actions.push(PlannedAction {
        action: RemoveAction {
            path: path.to_string(),
            outcome,
            detail: detail.into(),
        },
        mutation,
    });
}

/// Decide the whole withdrawal without touching the filesystem.
fn plan_remove(ctx: &Ctx) -> Vec<PlannedAction> {
    let mut actions: Vec<PlannedAction> = Vec::new();
    let root = &ctx.target_dir;

    // 1. The ADE-owned tree, driven by the lockfile's content hashes.
    let lock = load_lockfile(root);
    match &lock {
        Some(lock) => {
            for (rel, expected_hash) in &lock.files {
                match read_if_exists(&root.join(rel)) {
                    None => plan(
                        &mut actions,
                        rel,
                        RemoveOutcome::Kept,
                        "already gone",
                        Mutation::None,
                    ),
                    Some(content) if &sha256_hex(&content) == expected_hash => plan(
                        &mut actions,
                        rel,
                        RemoveOutcome::Deleted,
                        "matched the lockfile hash",
                        Mutation::Delete,
                    ),
                    Some(_) => plan(
                        &mut actions,
                        rel,
                        RemoveOutcome::Kept,
                        "edited since ade wrote it (hash mismatch) — left for you to review",
                        Mutation::None,
                    ),
                }
            }
        }
        None => plan(
            &mut actions,
            LOCKFILE_NAME,
            RemoveOutcome::Kept,
            "no lockfile — cannot prove which files under .ade/ are ade's, so none were deleted",
            Mutation::None,
        ),
    }

    // 2. Anything else under .ade/ was not written by us.
    if let Some(lock) = &lock {
        for rel in scan_ade_tree(root) {
            if !lock.files.contains_key(&rel) {
                plan(
                    &mut actions,
                    &rel,
                    RemoveOutcome::Kept,
                    "not written by ade (absent from the lockfile) — left in place",
                    Mutation::None,
                );
            }
        }
        // The audit log is ADE's and machine-local; the lockfile pins it by
        // checkpoint rather than by hash, so it is removed explicitly.
        if root.join(AUDIT_LOG_PATH).exists() {
            plan(
                &mut actions,
                AUDIT_LOG_PATH,
                RemoveOutcome::Deleted,
                "ade's machine-local audit chain",
                Mutation::Delete,
            );
        }
    }

    // 3. instructions.local.md is YOURS the moment you edit it.
    if let Some(content) = read_if_exists(&root.join(LOCAL_INSTRUCTIONS_PATH)) {
        if content == LOCAL_INSTRUCTIONS_STUB {
            plan(
                &mut actions,
                LOCAL_INSTRUCTIONS_PATH,
                RemoveOutcome::Deleted,
                "untouched stub",
                Mutation::Delete,
            );
        } else {
            plan(
                &mut actions,
                LOCAL_INSTRUCTIONS_PATH,
                RemoveOutcome::Kept,
                "your project instructions — ade created the file but the words are yours",
                Mutation::None,
            );
        }
    }

    // 4. Managed blocks in co-owned instruction files.
    for rel in instruction_targets(ctx) {
        let Some(existing) = read_if_exists(&root.join(&rel)) else {
            continue;
        };
        match remove_managed_block(&existing) {
            RemoveResult::Absent => {}
            RemoveResult::Refused { error } => plan(
                &mut actions,
                &rel,
                RemoveOutcome::Kept,
                error,
                Mutation::None,
            ),
            RemoveResult::Removed { content } if content.is_empty() => plan(
                &mut actions,
                &rel,
                RemoveOutcome::Deleted,
                "contained only ade's block — nothing of yours was in it",
                Mutation::Delete,
            ),
            RemoveResult::Removed { content } => plan(
                &mut actions,
                &rel,
                RemoveOutcome::Excised,
                "ade block removed; your content untouched",
                Mutation::Write(content),
            ),
        }
    }

    // 5. Co-owned JSON: subtract exactly what we merged.
    for (rel, patch) in [
        (CLAUDE_SETTINGS_PATH, claude_settings_patch()),
        (MCP_CONFIG_PATH, mcp_patch()),
    ] {
        let Some(existing) = read_if_exists(&root.join(rel)) else {
            continue;
        };
        let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&existing) else {
            plan(
                &mut actions,
                rel,
                RemoveOutcome::Kept,
                "not valid JSON — refusing to edit it",
                Mutation::None,
            );
            continue;
        };
        let reduced = subtract_json(&parsed, &patch);
        if reduced == parsed {
            continue;
        }
        if reduced.as_object().is_some_and(|object| object.is_empty()) {
            plan(
                &mut actions,
                rel,
                RemoveOutcome::Deleted,
                "held only ade's entries — nothing of yours was in it",
                Mutation::Delete,
            );
        } else {
            plan(
                &mut actions,
                rel,
                RemoveOutcome::Excised,
                "ade entries withdrawn; your settings untouched",
                Mutation::Write(stable_stringify(&reduced)),
            );
        }
    }

    // 6. .gitignore lines.
    if let Some(existing) = read_if_exists(&root.join(".gitignore")) {
        let mut current = existing.clone();
        for (label, owned) in gitignore_blocks() {
            if let Some(next) = strip_ensure_lines_block(&current, label, &owned) {
                current = next;
            }
        }
        if current != existing {
            if current.is_empty() {
                plan(
                    &mut actions,
                    ".gitignore",
                    RemoveOutcome::Deleted,
                    "held only ade's entries",
                    Mutation::Delete,
                );
            } else {
                plan(
                    &mut actions,
                    ".gitignore",
                    RemoveOutcome::Excised,
                    "ade entries removed; your rules untouched",
                    Mutation::Write(current),
                );
            }
        }
    }

    // 7. `.ade/audit/` sits outside the lockfile by design (the log grows after
    //    apply, so it cannot be content-hashed) — its two ADE-written files are
    //    named explicitly rather than inferred from the directory, so anything
    //    else in there is still treated as somebody else's.
    if root.join(AUDIT_README_PATH).exists() {
        plan(
            &mut actions,
            AUDIT_README_PATH,
            RemoveOutcome::Deleted,
            "ade's audit-log readme",
            Mutation::Delete,
        );
    }

    // 8. The git pre-commit hook. `apply` RENAMES a pre-existing hook aside and
    //    chains it, so withdrawing has to put it back — deleting ours and
    //    leaving `pre-commit.pre-ade` orphaned would silently disable the
    //    user's own hook, which is worse than never having installed.
    let hook_path = root.join(".git/hooks/pre-commit");
    let chained_rel = format!(".git/hooks/{CHAINED_HOOK_NAME}");
    if let Some(existing) = read_if_exists(&hook_path) {
        if existing.contains(HOOK_MARKER) {
            if root.join(&chained_rel).exists() {
                plan(
                    &mut actions,
                    ".git/hooks/pre-commit",
                    RemoveOutcome::Excised,
                    "restored the hook ade had chained aside",
                    Mutation::Restore {
                        from: chained_rel.clone(),
                        to: ".git/hooks/pre-commit".to_string(),
                    },
                );
            } else {
                plan(
                    &mut actions,
                    ".git/hooks/pre-commit",
                    RemoveOutcome::Deleted,
                    "ade's secret-scan shim",
                    Mutation::Delete,
                );
            }
        } else {
            plan(
                &mut actions,
                ".git/hooks/pre-commit",
                RemoveOutcome::Kept,
                "not ade's hook (no ade marker) — left alone",
                Mutation::None,
            );
        }
    }

    // 9. `.pre-commit-config.yaml` is only ours when it is byte-for-byte what we
    //    would write; the moment it differs it is the user's file.
    if let Some(existing) = read_if_exists(&root.join(PRECOMMIT_CONFIG_PATH)) {
        if existing == pre_commit_config() {
            plan(
                &mut actions,
                PRECOMMIT_CONFIG_PATH,
                RemoveOutcome::Deleted,
                "exactly the config ade wrote",
                Mutation::Delete,
            );
        } else {
            plan(
                &mut actions,
                PRECOMMIT_CONFIG_PATH,
                RemoveOutcome::Kept,
                "differs from what ade writes — treated as yours",
                Mutation::None,
            );
        }
    }

    // 10. ADE's own top-level files.
    for rel in [CONFIG_FILE, LOCKFILE_NAME] {
        if root.join(rel).exists() {
            plan(
                &mut actions,
                rel,
                RemoveOutcome::Deleted,
                "ade's own file",
                Mutation::Delete,
            );
        }
    }

    actions
}

/// Roots ADE may have created directories under. Everything below them is
/// pruned bottom-up and ONLY when genuinely empty — never a recursive delete,
/// so a single kept file keeps its whole directory chain alive with it.
const PRUNE_ROOTS: [&str; 3] = [".ade", ".claude", ".cursor"];

/// Remove empty directories under `root`, deepest first. A directory that still
/// holds anything survives, and so does every directory above it.
fn prune_empty_dirs(root: &std::path::Path) {
    if !root.is_dir() {
        return;
    }
    let mut children: Vec<std::path::PathBuf> = Vec::new();
    if let Ok(entries) = std::fs::read_dir(root) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                children.push(path);
            }
        }
    }
    for child in children {
        prune_empty_dirs(&child);
    }
    let _ = std::fs::remove_dir(root);
}

/// Plan (and optionally perform) the withdrawal of ADE from `ctx.target_dir`.
///
/// `apply == false` is a pure dry run: it reads, decides, and writes nothing.
/// Both modes return the same actions, because both run the same planner.
pub fn remove_pipeline(ctx: &Ctx, apply: bool) -> RemoveReport {
    let planned = plan_remove(ctx);
    if apply {
        for item in &planned {
            let path = ctx.target_dir.join(&item.action.path);
            match &item.mutation {
                Mutation::Delete => {
                    let _ = std::fs::remove_file(&path);
                }
                Mutation::Write(content) => {
                    let _ = std::fs::write(&path, content);
                }
                Mutation::Restore { from, to } => {
                    let _ = std::fs::rename(ctx.target_dir.join(from), ctx.target_dir.join(to));
                }
                Mutation::None => {}
            }
        }
        for rel in PRUNE_ROOTS {
            prune_empty_dirs(&ctx.target_dir.join(rel));
        }
    }
    RemoveReport {
        executed: apply,
        actions: planned.into_iter().map(|item| item.action).collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::managed::render_managed_block;

    #[test]
    fn strip_ensure_lines_block_removes_only_owned_lines() {
        let existing = "node_modules/\n# ADE Bootstrapper — observability\n.ade/audit/\ndist/\n";
        let stripped = strip_ensure_lines_block(
            existing,
            "ADE Bootstrapper — observability",
            &[".ade/audit/"],
        )
        .expect("block present");
        assert_eq!(stripped, "node_modules/\ndist/\n");

        // A user line inside the block survives (we stop at the first line we
        // do not own).
        let interleaved = "# ADE Bootstrapper — observability\n.ade/audit/\nmine.txt\n";
        let stripped = strip_ensure_lines_block(
            interleaved,
            "ADE Bootstrapper — observability",
            &[".ade/audit/"],
        )
        .expect("block present");
        assert_eq!(stripped, "mine.txt\n");

        assert!(strip_ensure_lines_block(
            "node_modules/\n",
            "ADE Bootstrapper — observability",
            &[]
        )
        .is_none());
    }

    #[test]
    fn managed_block_removal_refuses_what_it_cannot_prove_it_wrote() {
        let body = "rule one";
        let file = format!("# My notes\n\n{}\n", render_managed_block(body));
        match remove_managed_block(&file) {
            RemoveResult::Removed { content } => assert_eq!(content, "# My notes\n"),
            other => panic!("expected removal, got {other:?}"),
        }

        // Hand-edited inside the block → refused, nothing lost.
        let edited = file.replace("rule one", "rule one plus MY EDIT");
        assert!(matches!(
            remove_managed_block(&edited),
            RemoveResult::Refused { .. }
        ));

        // Markers without an ADE provenance line are somebody else's.
        let foreign = "<!-- ade:begin -->\nnot ours\n<!-- ade:end -->\n";
        assert!(matches!(
            remove_managed_block(foreign),
            RemoveResult::Refused { .. }
        ));

        assert!(matches!(
            remove_managed_block("# nothing here\n"),
            RemoveResult::Absent
        ));
    }

    #[test]
    fn block_only_file_removes_to_empty_so_the_file_can_go() {
        let file = format!("{}\n", render_managed_block("just ade"));
        match remove_managed_block(&file) {
            RemoveResult::Removed { content } => assert!(content.is_empty()),
            other => panic!("expected removal, got {other:?}"),
        }
    }
}
