//! Environment/context detection — port of `src/context.ts`.
//! All probes go through injected which/exec so tests can simulate any machine.

use crate::harness::adapters::HARNESS_ADAPTERS;
use crate::types::{
    AdeConfig, ArtifactWriter, Ctx, ExecFn, ExecOpts, ToolInfo, WhichFn, INTEGRATED_TOOLS,
};
use std::collections::BTreeMap;
use std::path::Path;

/// Version-flag lookup for tools whose version output we can cheaply parse.
fn version_args(name: &str) -> &'static [&'static str] {
    match name {
        "gitleaks" => &["version"],
        _ => &["--version"],
    }
}

/// First non-blank line of version output, trimmed. Never fails.
pub fn parse_version_line(raw: &str) -> Option<String> {
    raw.split('\n')
        .find(|candidate| !candidate.trim().is_empty())
        .map(|line| line.trim().to_string())
}

pub fn detect_tool(name: &str, which: &WhichFn, exec: &ExecFn) -> ToolInfo {
    let Some(path) = which(name) else {
        return ToolInfo {
            name: name.to_string(),
            present: false,
            path: None,
            version: None,
        };
    };
    let mut argv: Vec<String> = vec![path.clone()];
    argv.extend(version_args(name).iter().map(|arg| arg.to_string()));
    let result = exec(&argv, &ExecOpts::default());
    let source = if result.stdout.trim().is_empty() {
        &result.stderr
    } else {
        &result.stdout
    };
    ToolInfo {
        name: name.to_string(),
        present: true,
        path: Some(path),
        version: if result.code == 0 {
            parse_version_line(source)
        } else {
            None
        },
    }
}

pub fn detect_tools(which: &WhichFn, exec: &ExecFn) -> BTreeMap<String, ToolInfo> {
    let mut tools = BTreeMap::new();
    for name in INTEGRATED_TOOLS {
        tools.insert(name.to_string(), detect_tool(name, which, exec));
    }
    tools
}

/// Order-preserving tool list for doctor output (INTEGRATED_TOOLS order,
/// matching the oracle's insertion-ordered Object.values).
pub fn tools_in_doctor_order(tools: &BTreeMap<String, ToolInfo>) -> Vec<ToolInfo> {
    INTEGRATED_TOOLS
        .iter()
        .filter_map(|name| tools.get(*name).cloned())
        .collect()
}

/// Harness ids configured in the target repo (by config-signal presence).
pub fn detect_repo_harnesses(target_dir: &Path) -> Vec<String> {
    let mut found = Vec::new();
    for adapter in &HARNESS_ADAPTERS {
        for signal in adapter.config_signals {
            if target_dir.join(signal).exists() {
                found.push(adapter.id.to_string());
                break;
            }
        }
    }
    found
}

/// Harness ids whose CLI is installed on this machine.
pub fn detect_machine_harnesses(which: &WhichFn) -> Vec<String> {
    HARNESS_ADAPTERS
        .iter()
        .filter(|adapter| adapter.cli_names.iter().any(|cli| which(cli).is_some()))
        .map(|adapter| adapter.id.to_string())
        .collect()
}

pub fn is_git_repo(target_dir: &Path, exec: &ExecFn) -> bool {
    let argv: Vec<String> = vec![
        "git".into(),
        "-C".into(),
        target_dir.to_string_lossy().to_string(),
        "rev-parse".into(),
        "--is-inside-work-tree".into(),
    ];
    let result = exec(&argv, &ExecOpts::default());
    result.code == 0 && result.stdout.trim() == "true"
}

pub struct BuildCtxOptions {
    pub target_dir: std::path::PathBuf,
    pub config: AdeConfig,
    pub exec: ExecFn,
    pub which: WhichFn,
    pub env: Option<BTreeMap<String, String>>,
}

/// Node/Bun platform names (the oracle's `process.platform`) — generated
/// artifacts must stay byte-compatible across the TS and Rust implementations.
pub fn node_os_name(rust_os: &str) -> String {
    match rust_os {
        "macos" => "darwin".to_string(),
        "windows" => "win32".to_string(),
        other => other.to_string(),
    }
}

/// Node/Bun arch names (the oracle's `process.arch`).
pub fn node_arch_name(rust_arch: &str) -> String {
    match rust_arch {
        "aarch64" => "arm64".to_string(),
        "x86_64" => "x64".to_string(),
        other => other.to_string(),
    }
}

pub fn build_ctx(options: BuildCtxOptions) -> Ctx {
    let tools = detect_tools(&options.which, &options.exec);
    let repo_harnesses = detect_repo_harnesses(&options.target_dir);
    let git = is_git_repo(&options.target_dir, &options.exec);
    let env = options
        .env
        .unwrap_or_else(|| std::env::vars().collect::<BTreeMap<String, String>>());
    Ctx {
        ade_dir: options.target_dir.join(".ade"),
        artifacts: ArtifactWriter::new(&options.target_dir),
        target_dir: options.target_dir,
        config: options.config,
        tools,
        repo_harnesses,
        is_git_repo: git,
        os: node_os_name(std::env::consts::OS),
        arch: node_arch_name(std::env::consts::ARCH),
        env,
        exec: options.exec,
        which: options.which,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::{fake_exec, fake_which};

    #[test]
    fn parse_version_line_takes_first_nonblank() {
        assert_eq!(
            parse_version_line("\n\n rtk 1.0 \nmore"),
            Some("rtk 1.0".to_string())
        );
        assert_eq!(parse_version_line("\n \n"), None);
    }

    #[test]
    fn detect_tool_absent_present_and_probe_failure() {
        let absent = detect_tool("rtk", &fake_which(&[]), &fake_exec(&[]));
        assert!(!absent.present);

        let present = detect_tool(
            "rtk",
            &fake_which(&["rtk"]),
            &fake_exec(&[("/fake/bin/rtk --version", (0, "rtk 0.43.0\n", ""))]),
        );
        assert!(present.present);
        assert_eq!(present.version.as_deref(), Some("rtk 0.43.0"));

        let broken = detect_tool(
            "rtk",
            &fake_which(&["rtk"]),
            &fake_exec(&[("/fake/bin/rtk --version", (1, "", "boom"))]),
        );
        assert!(broken.present);
        assert!(broken.version.is_none());

        // stderr is the version source when stdout is empty (some tools do this).
        let stderr_version = detect_tool(
            "rtk",
            &fake_which(&["rtk"]),
            &fake_exec(&[("/fake/bin/rtk --version", (0, "", "rtk 9.9\n"))]),
        );
        assert_eq!(stderr_version.version.as_deref(), Some("rtk 9.9"));
    }

    #[test]
    fn gitleaks_uses_bare_version_argument() {
        let info = detect_tool(
            "gitleaks",
            &fake_which(&["gitleaks"]),
            &fake_exec(&[("/fake/bin/gitleaks version", (0, "8.30.1\n", ""))]),
        );
        assert_eq!(info.version.as_deref(), Some("8.30.1"));
    }

    #[test]
    fn detect_tools_covers_all_integrated_tools_in_order() {
        let tools = detect_tools(&fake_which(&[]), &fake_exec(&[]));
        assert_eq!(tools.len(), INTEGRATED_TOOLS.len());
        let ordered = tools_in_doctor_order(&tools);
        let names: Vec<&str> = ordered.iter().map(|tool| tool.name.as_str()).collect();
        assert_eq!(names, INTEGRATED_TOOLS.to_vec());
    }

    #[test]
    fn repo_and_machine_harness_detection() {
        let dir = std::env::temp_dir().join(format!("ade-core-ctx-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join(".cursor/rules")).unwrap();
        std::fs::write(dir.join("CLAUDE.md"), "x").unwrap();
        let repo = detect_repo_harnesses(&dir);
        assert!(repo.contains(&"claude-code".to_string()));
        assert!(repo.contains(&"cursor".to_string()));
        assert!(!repo.contains(&"codex".to_string()));

        let machine = detect_machine_harnesses(&fake_which(&["codex", "pi"]));
        assert_eq!(machine, vec!["codex".to_string(), "pi".to_string()]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn git_detection_requires_true_stdout() {
        let dir = std::env::temp_dir();
        assert!(is_git_repo(
            &dir,
            &fake_exec(&[("git -C", (0, "true\n", ""))])
        ));
        assert!(!is_git_repo(
            &dir,
            &fake_exec(&[("git -C", (0, "false\n", ""))])
        ));
        assert!(!is_git_repo(&dir, &fake_exec(&[])));
    }
}
