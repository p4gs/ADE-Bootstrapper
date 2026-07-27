//! `ade hook append` core — the runtime-free replacement for the oracle's
//! generated bun audit-hook script (ISC-165). Entry semantics are ported
//! byte-faithfully from `auditHookScript()` in `src/modules/observability.ts`:
//! actor "harness-hook", action "tool.<name>", target = redacted+capped
//! serialization of tool_input, result "observed". NEVER blocks the harness.

use crate::audit::{append_events, AuditEventInput};
use crate::gui::jobs::now_utc_seconds;
use std::path::Path;

pub const HOOK_LOG_PATH: &str = ".ade/audit/log.jsonl";

/// One-line safe summary: redact credential-shaped values, cap at 120 chars.
/// (Oracle: serialized.replace(/[A-Za-z0-9_\-]{20,}/g, "[redacted]").slice(0, 120))
pub fn safe_target(tool_input: Option<&serde_json::Value>) -> String {
    let serialized = serde_json::to_string(tool_input.unwrap_or(&serde_json::json!({})))
        .unwrap_or_else(|_| "{}".to_string());
    let re = regex::Regex::new(r"[A-Za-z0-9_\-]{20,}").expect("static redaction pattern");
    let redacted = re.replace_all(&serialized, "[redacted]").to_string();
    redacted.chars().take(120).collect()
}

fn now_iso_millis() -> String {
    let duration = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let base = now_utc_seconds(duration.as_secs());
    format!(
        "{}.{:03}Z",
        base.trim_end_matches('Z'),
        duration.subsec_millis()
    )
}

/// Append one observability entry from a harness hook event. Malformed or
/// non-object payloads are silently ignored; io failures are swallowed —
/// the hook must never block the harness (always "succeeds").
pub fn hook_append(dir: &Path, stdin_text: &str) {
    let Ok(parsed) = serde_json::from_str::<serde_json::Value>(stdin_text) else {
        return;
    };
    let Some(event) = parsed.as_object() else {
        return;
    };
    let tool_name = event
        .get("tool_name")
        .and_then(|value| value.as_str())
        .unwrap_or("unknown");
    let input = AuditEventInput {
        ts: now_iso_millis(),
        actor: "harness-hook".to_string(),
        action: format!("tool.{tool_name}"),
        target: safe_target(event.get("tool_input")),
        result: "observed".to_string(),
    };
    let _ = append_events(&dir.join(HOOK_LOG_PATH), &[input]);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audit::{parse_log, verify_chain};
    use crate::fsutil::read_if_exists;
    use crate::testutil::make_temp_dir;

    #[test]
    fn safe_target_redacts_and_caps() {
        let value = serde_json::json!({
            "cmd": "deploy",
            "token": "AKIAIOSFODNN7EXAMPLEKEYLONGLONG"
        });
        let target = safe_target(Some(&value));
        assert!(target.contains("[redacted]"));
        assert!(!target.contains("AKIAIOSFODNN7EXAMPLEKEYLONGLONG"));
        assert!(target.chars().count() <= 120);
        assert_eq!(safe_target(None), "{}");
        let huge = serde_json::json!({"a": "x ".repeat(400)});
        assert!(safe_target(Some(&huge)).chars().count() == 120);
    }

    #[test]
    fn hook_append_builds_a_valid_chain_and_never_blocks() {
        let dir = make_temp_dir("hook");
        hook_append(
            &dir,
            r#"{"tool_name": "Bash", "tool_input": {"command": "ls"}}"#,
        );
        hook_append(
            &dir,
            r#"{"tool_name": "Edit", "tool_input": {"file": "x.rs"}}"#,
        );
        // Malformed / non-object payloads are ignored without error.
        hook_append(&dir, "not json at all");
        hook_append(&dir, "[1, 2, 3]");
        let text = read_if_exists(&dir.join(HOOK_LOG_PATH)).expect("log written");
        let entries = parse_log(&text).expect("parseable chain");
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].actor, "harness-hook");
        assert_eq!(entries[0].action, "tool.Bash");
        assert_eq!(entries[0].result, "observed");
        assert_eq!(entries[1].action, "tool.Edit");
        let verdict = verify_chain(&entries, None);
        assert!(
            verdict.valid,
            "hook-written chain must verify: {:?}",
            verdict.reason
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn missing_tool_name_becomes_unknown() {
        let dir = make_temp_dir("hook-unknown");
        hook_append(&dir, r#"{"something": 1}"#);
        let text = read_if_exists(&dir.join(HOOK_LOG_PATH)).expect("log written");
        let entries = parse_log(&text).expect("parseable");
        assert_eq!(entries[0].action, "tool.unknown");
        assert_eq!(entries[0].target, "{}");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
