//! Canonical test helpers — port of `tests/helpers.ts`. All module and core
//! tests build fixtures through these so behavior under test is uniform.
//! Always compiled (used by unit tests, integration tests, and the parity
//! harness); contains no production logic.

use crate::types::{
    AdeConfig, ArtifactWriter, Ctx, ExecFn, ExecResult, ModuleConfig, ToolInfo, WhichFn,
    INTEGRATED_TOOLS,
};
use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;

/// The fifteen module ids (kept literal so helpers don't depend on the registry).
pub const ALL_MODULE_IDS: [&str; 15] = [
    "guardrails",
    "supply-chain",
    "sandbox",
    "context",
    "scaffolding",
    "memory",
    "injection-defense",
    "config-governance",
    "observability",
    "approval-gates",
    "secrets",
    "git-hygiene",
    "cost-governance",
    "reproducibility",
    "token-efficiency",
];

/// Exec fake: rules keyed by joined-argv prefix/substring; unmatched → 127.
pub fn fake_exec(rules: &[(&str, (i32, &str, &str))]) -> ExecFn {
    let owned: Vec<(String, (i32, String, String))> = rules
        .iter()
        .map(|(prefix, (code, stdout, stderr))| {
            (
                (*prefix).to_string(),
                (*code, (*stdout).to_string(), (*stderr).to_string()),
            )
        })
        .collect();
    Arc::new(move |argv, _opts| {
        let joined = argv.join(" ");
        for (prefix, (code, stdout, stderr)) in &owned {
            if joined.starts_with(prefix.as_str()) || joined.contains(prefix.as_str()) {
                return ExecResult {
                    code: *code,
                    stdout: stdout.clone(),
                    stderr: stderr.clone(),
                };
            }
        }
        ExecResult {
            code: 127,
            stdout: String::new(),
            stderr: format!("not found: {}", argv.first().cloned().unwrap_or_default()),
        }
    })
}

/// Which fake: names in `present` resolve to /fake/bin/<name>.
pub fn fake_which(present: &[&str]) -> WhichFn {
    let owned: Vec<String> = present.iter().map(|name| name.to_string()).collect();
    Arc::new(move |name| {
        owned
            .iter()
            .any(|candidate| candidate == name)
            .then(|| format!("/fake/bin/{name}"))
    })
}

/// Default test config: every module enabled, claude-code + codex targeted.
pub fn test_config() -> AdeConfig {
    test_config_with(&[], &["claude-code", "codex"])
}

/// Test config with specific modules disabled.
pub fn test_config_with(disabled: &[&str], harnesses: &[&str]) -> AdeConfig {
    let mut modules = BTreeMap::new();
    for id in ALL_MODULE_IDS {
        modules.insert(
            id.to_string(),
            ModuleConfig {
                enabled: !disabled.contains(&id),
                options: serde_json::json!({}),
            },
        );
    }
    AdeConfig {
        schema_version: 1,
        harnesses: harnesses.iter().map(|h| h.to_string()).collect(),
        modules,
    }
}

pub struct TestCtxOptions<'a> {
    pub config: Option<AdeConfig>,
    /// Tools to mark present: (name, version).
    pub present_tools: &'a [(&'a str, &'a str)],
    pub is_git_repo: bool,
    pub repo_harnesses: &'a [&'a str],
    pub exec: Option<ExecFn>,
    pub which: Option<WhichFn>,
    pub env: BTreeMap<String, String>,
}

impl Default for TestCtxOptions<'_> {
    fn default() -> Self {
        TestCtxOptions {
            config: None,
            present_tools: &[],
            is_git_repo: true,
            repo_harnesses: &[],
            exec: None,
            which: None,
            env: BTreeMap::new(),
        }
    }
}

/// Fully deterministic Ctx over a real temp directory — no machine probing.
pub fn make_test_ctx(target_dir: &Path, options: TestCtxOptions<'_>) -> Ctx {
    let mut tools = BTreeMap::new();
    for name in INTEGRATED_TOOLS {
        let version = options
            .present_tools
            .iter()
            .find(|(tool, _)| *tool == name)
            .map(|(_, version)| version.to_string());
        tools.insert(
            name.to_string(),
            match version {
                Some(version) => ToolInfo {
                    name: name.to_string(),
                    present: true,
                    path: Some(format!("/fake/bin/{name}")),
                    version: Some(version),
                },
                None => ToolInfo {
                    name: name.to_string(),
                    present: false,
                    path: None,
                    version: None,
                },
            },
        );
    }
    let present_names: Vec<&str> = options
        .present_tools
        .iter()
        .map(|(name, _)| *name)
        .collect();
    Ctx {
        target_dir: target_dir.to_path_buf(),
        ade_dir: target_dir.join(".ade"),
        config: options.config.unwrap_or_else(test_config),
        tools,
        repo_harnesses: options
            .repo_harnesses
            .iter()
            .map(|h| h.to_string())
            .collect(),
        is_git_repo: options.is_git_repo,
        os: "test-os".to_string(),
        arch: "test-arch".to_string(),
        env: options.env,
        exec: options.exec.unwrap_or_else(|| fake_exec(&[])),
        which: options.which.unwrap_or_else(|| fake_which(&present_names)),
        artifacts: ArtifactWriter::new(target_dir),
    }
}

/// Fresh temp dir per test (caller removes).
pub fn make_temp_dir(tag: &str) -> std::path::PathBuf {
    // A monotonic counter, not just a timestamp: many tests share a tag, they
    // run in parallel, and macOS `SystemTime` is coarser than its nanosecond
    // units — two tests CAN stamp the same value, land in the same directory,
    // and then read each other's half-written files. That is a random red CI
    // run with no reproducible cause, so the collision is designed out.
    static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let unique = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!(
        "ade-core-{tag}-{}-{}-{unique}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or_default()
    ));
    std::fs::create_dir_all(&dir).expect("create temp dir");
    dir
}
