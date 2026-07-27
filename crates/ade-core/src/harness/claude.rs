//! Claude Code capability implementations: permission merging, MCP server
//! registration, and hook wiring — the three surfaces the claude-code adapter
//! declares. Port of `src/harness/claude.ts`.
//!
//! All merges are additive: ADE only ever ADDS entries (arrays union, objects
//! deep-merge); a pre-existing user entry with the same identity is preserved.
//! An unparseable user file is never clobbered — the merge is refused.

use crate::fsutil::{deep_merge, read_if_exists, stable_stringify};
use crate::types::{Ctx, Finding};

pub const CLAUDE_SETTINGS_PATH: &str = ".claude/settings.json";
pub const MCP_CONFIG_PATH: &str = ".mcp.json";

fn merge_json_file(
    ctx: &Ctx,
    rel_path: &str,
    patch: &serde_json::Value,
) -> std::io::Result<Finding> {
    let absolute = ctx.target_dir.join(rel_path);
    let mut base = serde_json::Value::Object(serde_json::Map::new());
    if let Some(existing_text) = read_if_exists(&absolute) {
        match serde_json::from_str::<serde_json::Value>(&existing_text) {
            Ok(parsed) => {
                if !parsed.is_object() {
                    return Ok(Finding::error(format!(
                        "{rel_path} is not a JSON object — refusing to merge"
                    )));
                }
                base = parsed;
            }
            Err(_) => {
                return Ok(Finding::error_with(
                    format!("{rel_path} is not valid JSON — refusing to merge"),
                    format!("fix the JSON in {rel_path}, then run `ade apply`"),
                ));
            }
        }
    }
    let merged = deep_merge(&base, patch);
    ctx.artifacts.write(rel_path, &stable_stringify(&merged))?;
    Ok(Finding::ok(format!("merged ADE entries into {rel_path}")))
}

/// Merge permission (or other) settings into `.claude/settings.json`.
pub fn merge_claude_settings(ctx: &Ctx, patch: &serde_json::Value) -> std::io::Result<Finding> {
    merge_json_file(ctx, CLAUDE_SETTINGS_PATH, patch)
}

/// Register an MCP server in `.mcp.json`. A pre-existing user server with the
/// same name is preserved untouched.
pub fn register_mcp_server(
    ctx: &Ctx,
    name: &str,
    server: &serde_json::Value,
) -> std::io::Result<Finding> {
    if let Some(existing_text) = read_if_exists(&ctx.target_dir.join(MCP_CONFIG_PATH)) {
        match serde_json::from_str::<serde_json::Value>(&existing_text) {
            Ok(parsed) => {
                let already_defined = parsed
                    .get("mcpServers")
                    .and_then(|servers| servers.as_object())
                    .is_some_and(|servers| servers.contains_key(name));
                if already_defined {
                    return Ok(Finding::info(format!(
                        ".mcp.json already defines \"{name}\" — preserved user entry"
                    )));
                }
            }
            Err(_) => {
                return Ok(Finding::error_with(
                    format!("{MCP_CONFIG_PATH} is not valid JSON — refusing to merge"),
                    format!("fix the JSON in {MCP_CONFIG_PATH}, then run `ade apply`"),
                ));
            }
        }
    }
    let mut servers = serde_json::Map::new();
    servers.insert(name.to_string(), server.clone());
    merge_json_file(
        ctx,
        MCP_CONFIG_PATH,
        &serde_json::json!({ "mcpServers": servers }),
    )
}

/// Wire a hook command into `.claude/settings.json` under the given event with
/// the default `"*"` matcher. Idempotent — deduped by the array-union merge on
/// identical entries.
pub fn wire_claude_hook(ctx: &Ctx, event: &str, command: &str) -> std::io::Result<Finding> {
    wire_claude_hook_with_matcher(ctx, event, command, "*")
}

/// Wire a hook command into `.claude/settings.json` under the given event.
/// Idempotent — deduped by the array-union merge on identical entries.
pub fn wire_claude_hook_with_matcher(
    ctx: &Ctx,
    event: &str,
    command: &str,
    matcher: &str,
) -> std::io::Result<Finding> {
    let mut hooks_by_event = serde_json::Map::new();
    hooks_by_event.insert(
        event.to_string(),
        serde_json::json!([
            {
                "matcher": matcher,
                "hooks": [{ "type": "command", "command": command }],
            }
        ]),
    );
    merge_claude_settings(ctx, &serde_json::json!({ "hooks": hooks_by_event }))
}

/// The Claude Code PostToolUse audit hook script body.
///
/// SANCTIONED DIVERGENCE (ISC-165): the TS oracle ships a self-contained bun
/// script at `.ade/hooks/audit-log.ts`; the Rust port ships the SAME script
/// path, but its content is a POSIX sh script that delegates to the installed
/// `ade` binary (`ade hook append` reads the PostToolUse event JSON on stdin
/// and appends the hash-chained entry itself). When `ade` is not on PATH the
/// script drains stdin and exits 0 — it never blocks the harness.
pub fn ade_hook_script(repo_dir: &str) -> String {
    format!(
        r#"#!/bin/sh
# ADE audit hook — Claude Code PostToolUse.
# Installed by ADE Bootstrapper (observability module). Reads the PostToolUse
# hook event JSON from stdin and appends one hash-chained entry to
# .ade/audit/log.jsonl via the installed `ade` binary. Never blocks the
# harness: exits 0 even when `ade` is not on PATH.
if command -v ade >/dev/null 2>&1; then
  exec ade hook append --dir "{repo_dir}"
fi
cat >/dev/null 2>&1
exit 0
"#
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::{make_temp_dir, make_test_ctx, TestCtxOptions};
    use crate::types::FindingLevel;
    use serde_json::json;
    use std::fs;

    fn read_json_file(path: &std::path::Path) -> serde_json::Value {
        serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap()
    }

    #[test]
    fn isc_50_permission_patch_merges_additively_preserving_user_entries() {
        let dir = make_temp_dir("claude-settings");
        let settings_path = dir.join(CLAUDE_SETTINGS_PATH);
        fs::create_dir_all(settings_path.parent().unwrap()).unwrap();
        fs::write(
            &settings_path,
            r#"{"permissions":{"deny":["Bash(user-rule:*)"]},"model":"user-choice"}"#,
        )
        .unwrap();
        let ctx = make_test_ctx(&dir, TestCtxOptions::default());
        let finding = merge_claude_settings(
            &ctx,
            &json!({ "permissions": { "deny": ["Bash(rm -rf /:*)"] } }),
        )
        .unwrap();
        assert_eq!(finding.level, FindingLevel::Ok);
        assert_eq!(
            finding.message,
            "merged ADE entries into .claude/settings.json"
        );
        let settings = read_json_file(&settings_path);
        let deny = settings["permissions"]["deny"].as_array().unwrap();
        assert!(deny.contains(&json!("Bash(user-rule:*)")));
        assert!(deny.contains(&json!("Bash(rm -rf /:*)")));
        assert_eq!(settings["model"], "user-choice");
        assert!(ctx
            .artifacts
            .written()
            .contains(&CLAUDE_SETTINGS_PATH.to_string()));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn unparseable_settings_json_is_refused_never_clobbered() {
        let dir = make_temp_dir("claude-broken");
        let settings_path = dir.join(CLAUDE_SETTINGS_PATH);
        fs::create_dir_all(settings_path.parent().unwrap()).unwrap();
        fs::write(&settings_path, "{broken json").unwrap();
        let ctx = make_test_ctx(&dir, TestCtxOptions::default());
        let finding = merge_claude_settings(&ctx, &json!({ "permissions": {} })).unwrap();
        assert_eq!(finding.level, FindingLevel::Error);
        assert_eq!(
            finding.message,
            ".claude/settings.json is not valid JSON — refusing to merge"
        );
        assert_eq!(
            finding.remediation.as_deref(),
            Some("fix the JSON in .claude/settings.json, then run `ade apply`")
        );
        assert_eq!(fs::read_to_string(&settings_path).unwrap(), "{broken json");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn non_object_settings_json_is_refused_never_clobbered() {
        let dir = make_temp_dir("claude-nonobj");
        let settings_path = dir.join(CLAUDE_SETTINGS_PATH);
        fs::create_dir_all(settings_path.parent().unwrap()).unwrap();
        fs::write(&settings_path, "[1, 2]").unwrap();
        let ctx = make_test_ctx(&dir, TestCtxOptions::default());
        let finding = merge_claude_settings(&ctx, &json!({ "permissions": {} })).unwrap();
        assert_eq!(finding.level, FindingLevel::Error);
        assert_eq!(
            finding.message,
            ".claude/settings.json is not a JSON object — refusing to merge"
        );
        assert_eq!(finding.remediation, None);
        assert_eq!(fs::read_to_string(&settings_path).unwrap(), "[1, 2]");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn isc_51_mcp_registration_preserves_preexisting_user_server_of_same_name() {
        let dir = make_temp_dir("claude-mcp-preserve");
        let mcp_path = dir.join(MCP_CONFIG_PATH);
        fs::write(
            &mcp_path,
            r#"{"mcpServers":{"openmemory":{"command":"user-custom"}}}"#,
        )
        .unwrap();
        let ctx = make_test_ctx(&dir, TestCtxOptions::default());
        let finding = register_mcp_server(
            &ctx,
            "openmemory",
            &json!({ "command": "npx", "args": ["-y", "openmemory"] }),
        )
        .unwrap();
        assert_eq!(finding.level, FindingLevel::Info);
        assert_eq!(
            finding.message,
            ".mcp.json already defines \"openmemory\" — preserved user entry"
        );
        let mcp = read_json_file(&mcp_path);
        assert_eq!(mcp["mcpServers"]["openmemory"]["command"], "user-custom");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn isc_51_mcp_registration_adds_new_server_beside_user_servers() {
        let dir = make_temp_dir("claude-mcp-add");
        let mcp_path = dir.join(MCP_CONFIG_PATH);
        fs::write(&mcp_path, r#"{"mcpServers":{"mine":{"command":"keep"}}}"#).unwrap();
        let ctx = make_test_ctx(&dir, TestCtxOptions::default());
        let finding =
            register_mcp_server(&ctx, "openmemory", &json!({ "command": "npx" })).unwrap();
        assert_eq!(finding.level, FindingLevel::Ok);
        let mcp = read_json_file(&mcp_path);
        assert_eq!(mcp["mcpServers"]["mine"]["command"], "keep");
        assert_eq!(mcp["mcpServers"]["openmemory"]["command"], "npx");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn invalid_mcp_json_is_refused_never_clobbered() {
        let dir = make_temp_dir("claude-mcp-broken");
        let mcp_path = dir.join(MCP_CONFIG_PATH);
        fs::write(&mcp_path, "not json at all").unwrap();
        let ctx = make_test_ctx(&dir, TestCtxOptions::default());
        let finding =
            register_mcp_server(&ctx, "openmemory", &json!({ "command": "npx" })).unwrap();
        assert_eq!(finding.level, FindingLevel::Error);
        assert_eq!(
            finding.message,
            ".mcp.json is not valid JSON — refusing to merge"
        );
        assert_eq!(
            finding.remediation.as_deref(),
            Some("fix the JSON in .mcp.json, then run `ade apply`")
        );
        assert_eq!(fs::read_to_string(&mcp_path).unwrap(), "not json at all");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn isc_86_adjacent_hook_wiring_is_idempotent_deduped_on_identical_entries() {
        let dir = make_temp_dir("claude-hook");
        let ctx = make_test_ctx(&dir, TestCtxOptions::default());
        wire_claude_hook(&ctx, "PostToolUse", "bun .ade/hooks/audit-log.ts").unwrap();
        let ctx2 = make_test_ctx(&dir, TestCtxOptions::default());
        wire_claude_hook(&ctx2, "PostToolUse", "bun .ade/hooks/audit-log.ts").unwrap();
        let settings = read_json_file(&dir.join(CLAUDE_SETTINGS_PATH));
        let post = settings["hooks"]["PostToolUse"].as_array().unwrap();
        assert_eq!(post.len(), 1);
        assert_eq!(post[0]["matcher"], "*");
        assert_eq!(post[0]["hooks"][0]["type"], "command");
        assert_eq!(
            post[0]["hooks"][0]["command"],
            "bun .ade/hooks/audit-log.ts"
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn isc_165_ade_hook_script_is_posix_sh_delegating_to_ade_with_graceful_noop() {
        let script = ade_hook_script("/repo/target");
        assert!(script.starts_with("#!/bin/sh\n"));
        assert!(script.contains("if command -v ade >/dev/null 2>&1; then"));
        assert!(script.contains("exec ade hook append --dir \"/repo/target\""));
        // Graceful no-op when `ade` is absent: drain stdin, exit 0.
        assert!(script.contains("cat >/dev/null 2>&1\nexit 0\n"));
        assert!(script.ends_with("\n"));
    }
}
