//! Core contracts — port of `src/types.ts` (the frozen v0.1 interface).
//! Field names serialize camelCase to keep JSON shapes oracle-identical.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use crate::fsutil::write_ensured;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecResult {
    pub code: i32,
    pub stdout: String,
    pub stderr: String,
}

impl ExecResult {
    pub fn failure(code: i32, stderr: &str) -> Self {
        ExecResult {
            code,
            stdout: String::new(),
            stderr: stderr.to_string(),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct ExecOpts {
    pub cwd: Option<PathBuf>,
    pub stdin: Option<String>,
}

/// Subprocess execution — ALWAYS argv arrays, never shell strings (ISC-179).
pub type ExecFn = Arc<dyn Fn(&[String], &ExecOpts) -> ExecResult + Send + Sync>;
/// Binary lookup on PATH.
pub type WhichFn = Arc<dyn Fn(&str) -> Option<String> + Send + Sync>;

/// Convenience: run an ExecFn from &str argv.
pub fn run_argv(exec: &ExecFn, argv: &[&str]) -> ExecResult {
    let owned: Vec<String> = argv.iter().map(|s| s.to_string()).collect();
    exec(&owned, &ExecOpts::default())
}

/// Presence/version info for one integrated tool.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ToolInfo {
    pub name: String,
    pub present: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
}

/// Severity ladder for findings.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "lowercase")]
pub enum FindingLevel {
    Ok,
    Info,
    Warn,
    Degraded,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Finding {
    pub level: FindingLevel,
    pub message: String,
    /// Actionable next step (install guidance, config fix, …).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remediation: Option<String>,
}

impl Finding {
    pub fn ok(message: impl Into<String>) -> Self {
        Finding {
            level: FindingLevel::Ok,
            message: message.into(),
            remediation: None,
        }
    }
    pub fn info(message: impl Into<String>) -> Self {
        Finding {
            level: FindingLevel::Info,
            message: message.into(),
            remediation: None,
        }
    }
    pub fn warn(message: impl Into<String>, remediation: impl Into<String>) -> Self {
        Finding {
            level: FindingLevel::Warn,
            message: message.into(),
            remediation: Some(remediation.into()),
        }
    }
    pub fn degraded(message: impl Into<String>, remediation: impl Into<String>) -> Self {
        Finding {
            level: FindingLevel::Degraded,
            message: message.into(),
            remediation: Some(remediation.into()),
        }
    }
    pub fn error(message: impl Into<String>) -> Self {
        Finding {
            level: FindingLevel::Error,
            message: message.into(),
            remediation: None,
        }
    }
    pub fn error_with(message: impl Into<String>, remediation: impl Into<String>) -> Self {
        Finding {
            level: FindingLevel::Error,
            message: message.into(),
            remediation: Some(remediation.into()),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ActionKind {
    Write,
    Merge,
    Append,
    Hook,
    Info,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PlannedAction {
    pub kind: ActionKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    pub description: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ModuleStatus {
    Applied,
    Skipped,
    Degraded,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ModuleResult {
    pub status: ModuleStatus,
    pub findings: Vec<Finding>,
    /// Repo-relative paths this apply wrote (collected into the lockfile).
    pub wrote_paths: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct VerifyResult {
    pub ok: bool,
    pub findings: Vec<Finding>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstructionBlock {
    pub id: &'static str,
    pub title: &'static str,
    /// Markdown body. No heading — the composer renders `### {title}`.
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ModuleConfig {
    pub enabled: bool,
    pub options: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AdeConfig {
    pub schema_version: u64,
    /// Harness adapter ids this repo targets.
    pub harnesses: Vec<String>,
    pub modules: BTreeMap<String, ModuleConfig>,
}

/// Records everything written through it (lockfile scope).
pub struct ArtifactWriter {
    target_dir: PathBuf,
    written: Mutex<BTreeSet<String>>,
}

impl ArtifactWriter {
    pub fn new(target_dir: &Path) -> Self {
        ArtifactWriter {
            target_dir: target_dir.to_path_buf(),
            written: Mutex::new(BTreeSet::new()),
        }
    }

    pub fn write(&self, rel_path: &str, content: &str) -> std::io::Result<bool> {
        let changed = write_ensured(&self.target_dir.join(rel_path), content)?;
        self.written
            .lock()
            .expect("artifact lock")
            .insert(rel_path.to_string());
        Ok(changed)
    }

    pub fn written(&self) -> Vec<String> {
        self.written
            .lock()
            .expect("artifact lock")
            .iter()
            .cloned()
            .collect()
    }
}

/// Everything a module needs — port of the TS `Ctx`.
pub struct Ctx {
    pub target_dir: PathBuf,
    pub ade_dir: PathBuf,
    pub config: AdeConfig,
    pub tools: BTreeMap<String, ToolInfo>,
    pub repo_harnesses: Vec<String>,
    pub is_git_repo: bool,
    pub os: String,
    pub arch: String,
    pub env: BTreeMap<String, String>,
    pub exec: ExecFn,
    pub which: WhichFn,
    pub artifacts: ArtifactWriter,
}

impl Ctx {
    pub fn tool(&self, name: &str) -> Option<&ToolInfo> {
        self.tools.get(name)
    }
    pub fn tool_present(&self, name: &str) -> bool {
        self.tools
            .get(name)
            .map(|tool| tool.present)
            .unwrap_or(false)
    }
    pub fn module_options(&self, module_id: &str) -> serde_json::Value {
        self.config
            .modules
            .get(module_id)
            .map(|module| module.options.clone())
            .unwrap_or_else(|| serde_json::json!({}))
    }
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
pub struct HarnessCapabilities {
    pub hooks: bool,
    pub mcp: bool,
    pub permissions: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct HarnessAdapter {
    pub id: &'static str,
    pub title: &'static str,
    #[serde(rename = "instructionFile")]
    pub instruction_file: &'static str,
    #[serde(rename = "cliNames")]
    pub cli_names: &'static [&'static str],
    #[serde(rename = "configSignals")]
    pub config_signals: &'static [&'static str],
    pub capabilities: HarnessCapabilities,
}

/// The module contract every ADE module implements.
pub trait AdeModule: Send + Sync {
    fn id(&self) -> &'static str;
    fn title(&self) -> &'static str;
    fn category(&self) -> &'static str;
    /// Spec component this module implements (traceability).
    fn spec(&self) -> &'static str;
    fn default_enabled(&self) -> bool {
        true
    }
    /// Static instruction blocks composed into `.ade/instructions.md`.
    fn instruction_blocks(&self) -> Vec<InstructionBlock>;
    fn detect(&self, ctx: &Ctx) -> Vec<Finding>;
    fn plan(&self, ctx: &Ctx) -> Vec<PlannedAction>;
    fn apply(&self, ctx: &Ctx) -> ModuleResult;
    fn verify(&self, ctx: &Ctx) -> VerifyResult;
}

/// The twelve integrated tools ADE detects (order = doctor output order).
pub const INTEGRATED_TOOLS: [&str; 12] = [
    "trufflehog",
    "pre-commit",
    "gitleaks",
    "rtk",
    "ocean",
    "nono",
    "osv-scanner",
    "openwiki",
    "cocoindex",
    "ccc",
    "serena",
    "sscsb",
];
