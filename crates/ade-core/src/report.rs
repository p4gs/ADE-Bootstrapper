//! Shared machine/project reports — port of `src/report.ts` (ISC-166/174).
//! One code path for the CLI (`ade doctor` / `ade status`) and the GUI.

use crate::context::{
    detect_machine_harnesses, detect_repo_harnesses, detect_tools, is_git_repo,
    tools_in_doctor_order,
};
use crate::harness::adapters::HARNESS_ADAPTERS;
use crate::registry::modules;
use crate::types::{Ctx, ExecFn, Finding, ToolInfo, WhichFn};
use std::path::Path;

#[derive(Debug, Clone, serde::Serialize)]
pub struct DoctorHarnesses {
    pub repo: Vec<String>,
    pub machine: Vec<String>,
    pub supported: Vec<String>,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DoctorReport {
    pub target_dir: String,
    pub git_repo: bool,
    pub tools: Vec<ToolInfo>,
    pub harnesses: DoctorHarnesses,
}

/// Machine + repo health facts (the `ade doctor` payload, oracle shape).
pub fn doctor_report(target_dir: &Path, which: &WhichFn, exec: &ExecFn) -> DoctorReport {
    let tools = detect_tools(which, exec);
    DoctorReport {
        target_dir: target_dir.to_string_lossy().to_string(),
        git_repo: is_git_repo(target_dir, exec),
        tools: tools_in_doctor_order(&tools),
        harnesses: DoctorHarnesses {
            repo: detect_repo_harnesses(target_dir),
            machine: detect_machine_harnesses(which),
            supported: HARNESS_ADAPTERS
                .iter()
                .map(|adapter| adapter.id.to_string())
                .collect(),
        },
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct StatusRow {
    pub id: String,
    pub title: String,
    pub enabled: bool,
    pub state: String,
    /// Full findings — consumed by the GUI; the CLI drops this field.
    #[serde(skip)]
    pub findings: Vec<Finding>,
}

/// Per-module status, WITH findings retained for GUI consumers.
pub fn status_report(ctx: &Ctx) -> Vec<StatusRow> {
    let mut rows = Vec::new();
    for module in modules() {
        let enabled = ctx
            .config
            .modules
            .get(module.id())
            .map(|m| m.enabled)
            .unwrap_or(false);
        if !enabled {
            rows.push(StatusRow {
                id: module.id().to_string(),
                title: module.title().to_string(),
                enabled,
                state: "disabled".to_string(),
                findings: Vec::new(),
            });
            continue;
        }
        let verdict = module.verify(ctx);
        let degraded = verdict
            .findings
            .iter()
            .any(|finding| finding.level == crate::types::FindingLevel::Degraded);
        rows.push(StatusRow {
            id: module.id().to_string(),
            title: module.title().to_string(),
            enabled,
            state: if verdict.ok {
                if degraded {
                    "degraded".to_string()
                } else {
                    "applied".to_string()
                }
            } else {
                "not-applied".to_string()
            },
            findings: verdict.findings,
        });
    }
    rows
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::run::{apply_pipeline, PipelineDeps};
    use crate::testutil::{
        fake_exec, fake_which, make_temp_dir, make_test_ctx, test_config, test_config_with,
        TestCtxOptions,
    };
    use crate::types::INTEGRATED_TOOLS;
    use std::fs;
    use std::path::PathBuf;
    use std::sync::Arc;

    /// Fixture matching the TS beforeEach: temp dir + `.git/hooks` + package.json.
    fn fixture_dir(tag: &str) -> PathBuf {
        let dir = make_temp_dir(tag);
        fs::create_dir_all(dir.join(".git").join("hooks")).expect("create .git/hooks");
        fs::write(dir.join("package.json"), "{\"name\":\"fixture\"}\n")
            .expect("write package.json");
        dir
    }

    fn fixture_ctx(dir: &Path, config: crate::types::AdeConfig) -> Ctx {
        make_test_ctx(
            dir,
            TestCtxOptions {
                config: Some(config),
                exec: Some(fake_exec(&[("git -C", (0, "true\n", ""))])),
                ..Default::default()
            },
        )
    }

    #[test]
    fn doctor_reports_every_integrated_tool_and_the_7_supported_harnesses() {
        let dir = fixture_dir("report-doctor");
        let which: WhichFn = Arc::new(|name| (name == "rtk").then(|| "/fake/bin/rtk".to_string()));
        let exec = fake_exec(&[
            ("git -C", (0, "true\n", "")),
            ("/fake/bin/rtk", (0, "rtk 9.9.9\n", "")),
        ]);
        let report = doctor_report(&dir, &which, &exec);
        assert_eq!(report.target_dir, dir.to_string_lossy().to_string());
        assert!(report.git_repo);
        let names: Vec<&str> = report.tools.iter().map(|tool| tool.name.as_str()).collect();
        assert_eq!(
            names,
            INTEGRATED_TOOLS.to_vec(),
            "doctor order = tool order"
        );
        let rtk = report
            .tools
            .iter()
            .find(|tool| tool.name == "rtk")
            .expect("rtk row");
        assert!(rtk.present);
        assert_eq!(rtk.version.as_deref(), Some("rtk 9.9.9"));
        assert_eq!(report.harnesses.supported.len(), 7);
        assert!(report.harnesses.machine.is_empty());
        assert!(report.harnesses.repo.is_empty());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn status_reports_15_rows_after_a_real_apply_with_findings_retained() {
        let dir = fixture_dir("report-status");
        let deps = PipelineDeps {
            exec: fake_exec(&[("git -C", (0, "true\n", ""))]),
            which: fake_which(&[]),
            now: Some(Arc::new(|| "2026-07-12T00:00:00Z".to_string())),
        };
        let apply = apply_pipeline(&fixture_ctx(&dir, test_config()), &deps);
        assert!(apply.ok, "fixture apply must succeed");

        let rows = status_report(&fixture_ctx(&dir, test_config()));
        assert_eq!(rows.len(), 15);
        for row in &rows {
            assert!(!row.id.is_empty());
            assert!(!row.title.is_empty());
            assert!(row.enabled);
            assert!(
                row.state == "applied" || row.state == "degraded",
                "{} unexpectedly {}",
                row.id,
                row.state
            );
        }
        // Findings are retained for GUI consumers (the CLI drops them via #[serde(skip)]).
        assert!(rows.iter().any(|row| !row.findings.is_empty()));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn disabled_module_reports_state_disabled_with_no_findings() {
        let dir = fixture_dir("report-disabled");
        let config = test_config_with(&["sandbox"], &["claude-code", "codex"]);
        let rows = status_report(&fixture_ctx(&dir, config));
        assert_eq!(rows.len(), 15);
        let sandbox = rows
            .iter()
            .find(|row| row.id == "sandbox")
            .expect("sandbox row");
        assert!(!sandbox.enabled);
        assert_eq!(sandbox.state, "disabled");
        assert!(sandbox.findings.is_empty());
        let _ = fs::remove_dir_all(&dir);
    }
}
