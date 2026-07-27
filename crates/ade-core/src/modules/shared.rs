//! Shared helpers for ADE modules — port of `src/modules/_shared.ts`.
//! Every module writes policy artifacts through these so outputs stay
//! deterministic and lockfile-recorded. Message strings are oracle-identical.

use crate::fsutil::{read_if_exists, stable_stringify};
use crate::types::{Ctx, Finding};

/// Write a JSON policy artifact deterministically via the recorded writer.
pub fn write_policy(ctx: &Ctx, rel_path: &str, policy: &serde_json::Value) -> std::io::Result<()> {
    ctx.artifacts.write(rel_path, &stable_stringify(policy))?;
    Ok(())
}

/// Read + parse a JSON artifact from the target repo; None when absent/invalid.
pub fn read_json(ctx: &Ctx, rel_path: &str) -> Option<serde_json::Value> {
    let text = read_if_exists(&ctx.target_dir.join(rel_path))?;
    serde_json::from_str(&text).ok()
}

/// Standard finding for an integrated tool's presence/absence.
pub fn tool_finding(ctx: &Ctx, tool: &str, when_absent: &str) -> Finding {
    match ctx.tools.get(tool) {
        Some(info) if info.present => Finding::ok(match &info.version {
            Some(version) => format!("{tool} present ({version})"),
            None => format!("{tool} present"),
        }),
        _ => Finding::degraded(format!("{tool} not installed"), when_absent),
    }
}

/// Verify helper: artifact exists and parses as JSON.
pub fn verify_json_artifact(ctx: &Ctx, rel_path: &str) -> Finding {
    match read_json(ctx, rel_path) {
        None => Finding::error_with(
            format!("{rel_path} missing or invalid JSON"),
            "run `ade apply`",
        ),
        Some(_) => Finding::ok(format!("{rel_path} present and valid")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::{make_temp_dir, make_test_ctx, TestCtxOptions};
    use crate::types::FindingLevel;

    #[test]
    fn policy_write_read_and_verify_round_trip() {
        let dir = make_temp_dir("shared");
        let ctx = make_test_ctx(&dir, TestCtxOptions::default());
        write_policy(
            &ctx,
            ".ade/policy/test.json",
            &serde_json::json!({"b": 1, "a": 2}),
        )
        .unwrap();
        let loaded = read_json(&ctx, ".ade/policy/test.json").unwrap();
        assert_eq!(loaded, serde_json::json!({"a": 2, "b": 1}));
        let ok = verify_json_artifact(&ctx, ".ade/policy/test.json");
        assert_eq!(ok.level, FindingLevel::Ok);
        assert_eq!(ok.message, ".ade/policy/test.json present and valid");
        let missing = verify_json_artifact(&ctx, ".ade/policy/nope.json");
        assert_eq!(missing.level, FindingLevel::Error);
        assert_eq!(
            missing.message,
            ".ade/policy/nope.json missing or invalid JSON"
        );
        assert_eq!(missing.remediation.as_deref(), Some("run `ade apply`"));
        assert!(ctx
            .artifacts
            .written()
            .contains(&".ade/policy/test.json".to_string()));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn tool_finding_matches_oracle_strings() {
        let dir = make_temp_dir("shared-tf");
        let ctx = make_test_ctx(
            &dir,
            TestCtxOptions {
                present_tools: &[("rtk", "rtk 0.43.0")],
                ..Default::default()
            },
        );
        let present = tool_finding(&ctx, "rtk", "install rtk");
        assert_eq!(present.level, FindingLevel::Ok);
        assert_eq!(present.message, "rtk present (rtk 0.43.0)");
        let absent = tool_finding(&ctx, "nono", "install nono from https://nono.sh");
        assert_eq!(absent.level, FindingLevel::Degraded);
        assert_eq!(absent.message, "nono not installed");
        assert_eq!(
            absent.remediation.as_deref(),
            Some("install nono from https://nono.sh")
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
