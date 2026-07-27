//! Module: codebase context management — port of `src/modules/context-mgmt.ts`.
//! Spec component: "Codebase context management — an always-current codebase
//! understanding layer for coding harnesses."
//!
//! Two tiers, following ADE's integrate-before-rebuild convention:
//!   1. A zero-dependency structural codemap (`.ade/context/codemap.md`),
//!      re-derived from the tree on every `ade apply` — always present.
//!   2. Detected best-of-breed engines, wired in when installed: OpenWiki
//!      (Code Brain), CocoIndex (either the `cocoindex` framework or the `ccc`
//!      CLI), and the opt-in OpenWiki Personal Brain (options.enableBrain).
//!
//! Convention: engines are detected + wired + given exact install guidance;
//! `ade apply` NEVER force-installs a tool.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use crate::fsutil::read_if_exists;
use crate::modules::shared::{read_json, verify_json_artifact, write_policy};
use crate::types::{
    ActionKind, AdeModule, Ctx, Finding, FindingLevel, InstructionBlock, ModuleResult,
    ModuleStatus, PlannedAction, VerifyResult,
};

pub const CODEMAP_PATH: &str = ".ade/context/codemap.md";
pub const CONTEXT_ENGINES_PATH: &str = ".ade/policy/context-engines.json";
pub const OPENWIKI_DIR: &str = "openwiki/";

pub const OPENWIKI_INSTALL: &str = "install OpenWiki (`npm install -g openwiki`) for an auto-maintained, navigable codebase wiki — MIT-licensed, BYO model key; init with `openwiki --init`";
pub const COCOINDEX_INSTALL: &str = "install CocoIndex for AST-based semantic code search — `pip install cocoindex` (framework) or the `cocoindex-code` CLI (github.com/cocoindex-io/cocoindex-code); Apache-2.0";
pub const BRAIN_ACTIVATION: &str = "to enable OpenWiki Personal Brain, set modules.context.options.enableBrain = true in ade.json, then run `ade apply`; initialise with `openwiki personal --init`";

/// Directory names excluded from the codemap scan (any path segment).
pub const SKIPPED_DIRS: [&str; 8] = [
    "node_modules",
    ".git",
    ".ade",
    ".claude",
    ".cursor",
    "dist",
    "build",
    "coverage",
];

/// ADE-managed root files excluded from the codemap: they are generated/updated at
/// different points of the apply pipeline, so including them would make the codemap
/// order-dependent (init-vs-reapply drift). The codemap maps the USER's codebase.
pub const SKIPPED_FILES: [&str; 7] = [
    "ade.json",
    "ade.lock.json",
    "CLAUDE.md",
    "AGENTS.md",
    ".gitignore",
    ".mcp.json",
    ".pre-commit-config.yaml",
];

/// Structure markers verify() requires — regenerating always emits all of them.
pub const SECTION_MARKERS: [&str; 5] = [
    "# Codemap",
    "## Top-Level Directories",
    "## File Counts by Extension",
    "## Entry Points",
    "## Refresh",
];

/// The exact refresh-contract sentence fragment (ISC-74).
pub const REFRESH_SENTENCE: &str = "regenerated on every `ade apply`";

/// Pure input to the codemap builder — everything repo-relative, nothing absolute.
#[derive(Debug, Clone, PartialEq)]
pub struct TreeSnapshot {
    /// Repo-relative file paths, forward-slash separated, skip-dirs excluded, sorted.
    pub files: Vec<String>,
    /// Parsed package.json (object form) or None when absent/invalid.
    pub package_json: Option<serde_json::Map<String, serde_json::Value>>,
    /// Raw Cargo.toml text or None when absent.
    pub cargo_toml: Option<String>,
}

fn include_path(rel_path: &str) -> bool {
    if SKIPPED_FILES.contains(&rel_path) {
        return false;
    }
    !rel_path
        .split('/')
        .any(|segment| SKIPPED_DIRS.contains(&segment))
}

fn walk_tree(dir: &Path, prefix: &str, files: &mut Vec<String>) -> std::io::Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().into_owned();
        let rel = if prefix.is_empty() {
            name.clone()
        } else {
            format!("{prefix}/{name}")
        };
        let file_type = entry.file_type()?;
        if file_type.is_dir() {
            if !SKIPPED_DIRS.contains(&name.as_str()) {
                walk_tree(&entry.path(), &rel, files)?;
            }
        } else if file_type.is_file() && include_path(&rel) {
            files.push(rel);
        }
    }
    Ok(())
}

/// Scan the target tree into a deterministic snapshot (sorted, repo-relative only).
pub fn snapshot_tree(target_dir: &Path) -> std::io::Result<TreeSnapshot> {
    let mut files: Vec<String> = Vec::new();
    walk_tree(target_dir, "", &mut files)?;
    files.sort();

    let mut package_json: Option<serde_json::Map<String, serde_json::Value>> = None;
    if let Some(pkg_text) = read_if_exists(&target_dir.join("package.json")) {
        if let Ok(serde_json::Value::Object(map)) =
            serde_json::from_str::<serde_json::Value>(&pkg_text)
        {
            package_json = Some(map);
        }
    }
    let cargo_toml = read_if_exists(&target_dir.join("Cargo.toml"));
    Ok(TreeSnapshot {
        files,
        package_json,
        cargo_toml,
    })
}

/// One `[[bin]]` block extracted from Cargo.toml.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CargoBin {
    pub name: Option<String>,
    pub path: Option<String>,
}

/// Mirror of the oracle's `^(name|path)\s*=\s*"([^"]*)"` string-literal matcher.
fn parse_string_assignment(line: &str) -> Option<(&'static str, String)> {
    for key in ["name", "path"] {
        let Some(rest) = line.strip_prefix(key) else {
            continue;
        };
        let rest = rest.trim_start();
        let Some(rest) = rest.strip_prefix('=') else {
            continue;
        };
        let rest = rest.trim_start();
        let Some(rest) = rest.strip_prefix('"') else {
            continue;
        };
        let Some(end) = rest.find('"') else { continue };
        return Some((key, rest[..end].to_string()));
    }
    None
}

/// Minimal `[[bin]]` extraction from Cargo.toml (string-literal name/path only).
pub fn parse_cargo_bins(toml: &str) -> Vec<CargoBin> {
    let mut bins: Vec<CargoBin> = Vec::new();
    let mut in_bin = false;
    for raw in toml.split('\n') {
        let line = raw.trim();
        if line == "[[bin]]" {
            bins.push(CargoBin {
                name: None,
                path: None,
            });
            in_bin = true;
            continue;
        }
        if line.starts_with('[') {
            in_bin = false;
            continue;
        }
        if !in_bin {
            continue;
        }
        if let Some((key, value)) = parse_string_assignment(line) {
            if let Some(current) = bins.last_mut() {
                match key {
                    "name" => current.name = Some(value),
                    _ => current.path = Some(value),
                }
            }
        }
    }
    bins
}

/// File counts by extension: top 10, count-descending then extension-ascending.
fn extension_counts(files: &[String]) -> Vec<(String, usize)> {
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    for file in files {
        let base = file.rsplit('/').next().unwrap_or(file);
        let ext = match base.rfind('.') {
            Some(idx) if idx > 0 => base[idx..].to_string(),
            _ => "(no extension)".to_string(),
        };
        *counts.entry(ext).or_insert(0) += 1;
    }
    let mut entries: Vec<(String, usize)> = counts.into_iter().collect();
    entries.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    entries.truncate(10);
    entries
}

/// Mirror of the oracle's `^src\/(index|cli)\.[^/]+$` entry-point matcher.
fn is_src_entry(file: &str) -> bool {
    let Some(rest) = file.strip_prefix("src/") else {
        return false;
    };
    for stem in ["index.", "cli."] {
        if let Some(tail) = rest.strip_prefix(stem) {
            if !tail.is_empty() && !tail.contains('/') {
                return true;
            }
        }
    }
    false
}

/// Detected entry points, in a fixed deterministic order.
fn entry_points(tree: &TreeSnapshot) -> Vec<String> {
    let mut entries: Vec<String> = Vec::new();
    if let Some(pkg) = &tree.package_json {
        if let Some(serde_json::Value::String(main)) = pkg.get("main") {
            entries.push(format!("package.json main: `{main}`"));
        }
        match pkg.get("bin") {
            Some(serde_json::Value::String(bin)) => {
                entries.push(format!("package.json bin: `{bin}`"));
            }
            Some(serde_json::Value::Object(bin_map)) => {
                let mut names: Vec<&String> = bin_map.keys().collect();
                names.sort();
                for name in names {
                    if let Some(serde_json::Value::String(target)) = bin_map.get(name) {
                        entries.push(format!("package.json bin: {name} → `{target}`"));
                    }
                }
            }
            _ => {}
        }
        if let Some(serde_json::Value::Object(scripts)) = pkg.get("scripts") {
            let mut names: Vec<&String> = scripts.keys().collect();
            names.sort();
            if !names.is_empty() {
                let joined = names
                    .iter()
                    .map(|name| name.as_str())
                    .collect::<Vec<&str>>()
                    .join(", ");
                entries.push(format!("package.json scripts: {joined}"));
            }
        }
    }
    for file in &tree.files {
        if is_src_entry(file) {
            entries.push(format!("`{file}`"));
        }
    }
    if tree.files.iter().any(|file| file == "main.go") {
        entries.push("`main.go`".to_string());
    }
    if let Some(toml) = &tree.cargo_toml {
        for bin in parse_cargo_bins(toml) {
            let name = bin.name.unwrap_or_else(|| "(unnamed)".to_string());
            let path = bin
                .path
                .unwrap_or_else(|| "src/bin (cargo default)".to_string());
            entries.push(format!("Cargo.toml [[bin]]: {name} (`{path}`)"));
        }
    }
    entries
}

/// Codemap builder: tree snapshot → markdown. Pure and deterministic — sorted
/// everywhere, repo-relative paths only, no timestamps. Exported for direct testing.
pub fn build_codemap(tree: &TreeSnapshot) -> String {
    let dirs: Vec<String> = tree
        .files
        .iter()
        .filter(|file| file.contains('/'))
        .map(|file| file.split('/').next().unwrap_or(file).to_string())
        .collect::<BTreeSet<String>>()
        .into_iter()
        .collect();

    let mut lines: Vec<String> = vec![
        "# Codemap".to_string(),
        String::new(),
        "Machine-generated map of this repository (repo-relative paths only).".to_string(),
        "Consult it before scanning the tree.".to_string(),
        String::new(),
        "## Top-Level Directories".to_string(),
        String::new(),
    ];
    if dirs.is_empty() {
        lines.push("- (none)".to_string());
    } else {
        for dir in &dirs {
            lines.push(format!("- `{dir}/`"));
        }
    }

    let exts = extension_counts(&tree.files);
    lines.push(String::new());
    lines.push("## File Counts by Extension".to_string());
    lines.push(String::new());
    lines.push(format!(
        "{} files total (skipping {}). Top {} extensions:",
        tree.files.len(),
        SKIPPED_DIRS.join(", "),
        exts.len()
    ));
    lines.push(String::new());
    lines.push("| Extension | Files |".to_string());
    lines.push("| --- | --- |".to_string());
    for (ext, count) in &exts {
        lines.push(format!("| {ext} | {count} |"));
    }

    lines.push(String::new());
    lines.push("## Entry Points".to_string());
    lines.push(String::new());
    let entries = entry_points(tree);
    if entries.is_empty() {
        lines.push("- (none detected)".to_string());
    } else {
        for entry in entries {
            lines.push(format!("- {entry}"));
        }
    }

    lines.push(String::new());
    lines.push("## Refresh".to_string());
    lines.push(String::new());
    lines.push(format!(
        "This codemap is {REFRESH_SENTENCE}. Do not edit it by hand — when the repository structure changes, run `ade apply` to refresh it."
    ));
    lines.push(String::new());
    lines.join("\n")
}

/// Presence + version of one context engine, resolved from the live machine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EngineState {
    pub present: bool,
    pub version: Option<String>,
}

/// OpenWiki (Code Brain + Personal Brain share the one `openwiki` binary).
pub fn openwiki_state(ctx: &Ctx) -> EngineState {
    match ctx.tools.get("openwiki") {
        Some(info) if info.present => EngineState {
            present: true,
            version: info.version.clone(),
        },
        _ => EngineState {
            present: false,
            version: None,
        },
    }
}

/// CocoIndex is present if EITHER the framework (`cocoindex`) or the `ccc` CLI resolves.
pub fn cocoindex_state(ctx: &Ctx) -> EngineState {
    for name in ["cocoindex", "ccc"] {
        if let Some(info) = ctx.tools.get(name) {
            if info.present {
                return EngineState {
                    present: true,
                    version: info.version.clone(),
                };
            }
        }
    }
    EngineState {
        present: false,
        version: None,
    }
}

/// Personal Brain is opt-in (reaches outside the repo) — off unless explicitly enabled.
pub fn brain_opt_in(ctx: &Ctx) -> bool {
    ctx.module_options("context").get("enableBrain") == Some(&serde_json::Value::Bool(true))
}

/// Build the context-engines policy — deterministic, re-derivable from the live machine.
pub fn build_engines_policy(ctx: &Ctx) -> serde_json::Value {
    let wiki = openwiki_state(ctx);
    let coco = cocoindex_state(ctx);
    let brain_on = brain_opt_in(ctx);
    let mut codebase_wiki = serde_json::json!({
        "tool": "openwiki",
        "role": "auto-maintained navigable codebase wiki (prose + Mermaid); read FIRST for architecture understanding",
        "present": wiki.present,
        "enabled": wiki.present,
        "wikiDir": OPENWIKI_DIR,
        "autoRefresh": "regenerates from git diffs on re-run; enable continuous updates via OpenWiki's scheduled GitHub Action (openwiki-update.yml — daily update PR)",
        "install": OPENWIKI_INSTALL,
    });
    if let Some(version) = &wiki.version {
        codebase_wiki["version"] = serde_json::json!(version);
    }
    let mut semantic_index = serde_json::json!({
        "tool": "cocoindex",
        "role": "AST-based semantic code search / natural-language retrieval; use instead of whole-tree grep",
        "present": coco.present,
        "enabled": coco.present,
        "install": COCOINDEX_INSTALL,
    });
    if let Some(version) = &coco.version {
        semantic_index["version"] = serde_json::json!(version);
    }
    serde_json::json!({
        "schemaVersion": 1,
        "fallback": CODEMAP_PATH,
        "engines": {
            "codebaseWiki": codebase_wiki,
            "semanticIndex": semantic_index,
            "personalBrain": {
                "tool": "openwiki",
                "subCapabilityOf": "openwiki",
                "role": "general-purpose agent memory synthesised from external sources (email, notes, web); complementary to the codebase wiki, NOT the same thing",
                "present": wiki.present,
                "optIn": brain_on,
                "enabled": wiki.present && brain_on,
                "install": OPENWIKI_INSTALL,
                "activation": BRAIN_ACTIVATION,
            }
        }
    })
}

/// Advisory findings describing which engines are wired vs. absent.
fn engine_findings(ctx: &Ctx) -> Vec<Finding> {
    let wiki = openwiki_state(ctx);
    let coco = cocoindex_state(ctx);
    let brain_on = brain_opt_in(ctx);
    let mut findings: Vec<Finding> = Vec::new();
    findings.push(if wiki.present {
        Finding::ok(format!(
            "OpenWiki codebase wiki wired{}",
            wiki.version
                .as_ref()
                .map(|version| format!(" ({version})"))
                .unwrap_or_default()
        ))
    } else {
        Finding::degraded(
            "OpenWiki not installed — codebase wiki unavailable; using codemap fallback",
            OPENWIKI_INSTALL,
        )
    });
    findings.push(if coco.present {
        Finding::ok(format!(
            "CocoIndex semantic search wired{}",
            coco.version
                .as_ref()
                .map(|version| format!(" ({version})"))
                .unwrap_or_default()
        ))
    } else {
        Finding::degraded(
            "CocoIndex not installed — semantic code search unavailable",
            COCOINDEX_INSTALL,
        )
    });
    let brain_enabled = wiki.present && brain_on;
    let state = if brain_enabled {
        "enabled"
    } else if brain_on {
        "opted-in but OpenWiki absent"
    } else {
        "off (opt-in sub-capability)"
    };
    findings.push(Finding {
        level: FindingLevel::Info,
        message: format!("OpenWiki Personal Brain {state}"),
        remediation: if brain_enabled {
            None
        } else {
            Some(BRAIN_ACTIVATION.to_string())
        },
    });
    findings
}

fn apply_inner(ctx: &Ctx) -> std::io::Result<ModuleResult> {
    let tree = snapshot_tree(&ctx.target_dir)?;
    ctx.artifacts.write(CODEMAP_PATH, &build_codemap(&tree))?;
    write_policy(ctx, CONTEXT_ENGINES_PATH, &build_engines_policy(ctx))?;
    let mut findings = vec![
        Finding::ok(format!(
            "wrote {CODEMAP_PATH} ({} files mapped)",
            tree.files.len()
        )),
        Finding::ok(format!("wrote {CONTEXT_ENGINES_PATH}")),
    ];
    findings.extend(engine_findings(ctx));
    Ok(ModuleResult {
        status: ModuleStatus::Applied,
        findings,
        wrote_paths: vec![CODEMAP_PATH.to_string(), CONTEXT_ENGINES_PATH.to_string()],
    })
}

pub struct ContextMgmtModule;
pub static MODULE: ContextMgmtModule = ContextMgmtModule;

impl AdeModule for ContextMgmtModule {
    fn id(&self) -> &'static str {
        "context"
    }
    fn title(&self) -> &'static str {
        "Codebase Context Management"
    }
    fn category(&self) -> &'static str {
        "context"
    }
    fn spec(&self) -> &'static str {
        "Codebase context management"
    }

    fn instruction_blocks(&self) -> Vec<InstructionBlock> {
        vec![InstructionBlock {
            id: "context",
            title: "Codebase Context",
            content: [
                "Codebase-understanding sources, in priority order (see `.ade/policy/context-engines.json` for which are live):",
                "- **OpenWiki codebase wiki** (`openwiki/`) when present — read it FIRST for prose + Mermaid architecture understanding. It is auto-maintained; never hand-edit generated pages.",
                "- **CocoIndex semantic search** when present — use natural-language code retrieval instead of grepping the whole tree.",
                "- **`.ade/context/codemap.md`** — the always-present zero-dependency structural fallback; consult BEFORE any whole-repo scan. Regenerated on every `ade apply`.",
                "- **OpenWiki Personal Brain** (opt-in, `modules.context.options.enableBrain`) — general-purpose project/research memory across tools (email, notes, web). Distinct from the codebase wiki. NEVER write secrets or credentials into it.",
                "- Prefer targeted reads over directory dumps; after structural changes run `ade apply` (and re-run OpenWiki) rather than re-walking the tree.",
            ]
            .join("\n"),
        }]
    }

    fn detect(&self, ctx: &Ctx) -> Vec<Finding> {
        let existing = read_if_exists(&ctx.target_dir.join(CODEMAP_PATH));
        let codemap = match existing {
            None => Finding {
                level: FindingLevel::Info,
                message: format!("{CODEMAP_PATH} not yet generated"),
                remediation: Some("run `ade apply`".to_string()),
            },
            Some(_) => Finding::ok(format!("{CODEMAP_PATH} present")),
        };
        let mut findings = vec![codemap];
        findings.extend(engine_findings(ctx));
        findings
    }

    fn plan(&self, ctx: &Ctx) -> Vec<PlannedAction> {
        vec![
            PlannedAction {
                kind: ActionKind::Write,
                path: Some(CODEMAP_PATH.to_string()),
                description:
                    "generate codebase map (top-level dirs, extension counts, entry points) from the target tree"
                        .to_string(),
            },
            PlannedAction {
                kind: ActionKind::Write,
                path: Some(CONTEXT_ENGINES_PATH.to_string()),
                description: format!(
                    "record detected context engines (OpenWiki{}, CocoIndex) and the codemap fallback",
                    if brain_opt_in(ctx) { " + Personal Brain" } else { "" }
                ),
            },
        ]
    }

    fn apply(&self, ctx: &Ctx) -> ModuleResult {
        match apply_inner(ctx) {
            Ok(result) => result,
            Err(err) => ModuleResult {
                status: ModuleStatus::Failed,
                findings: vec![Finding::error(format!("module threw: {err}"))],
                wrote_paths: vec![],
            },
        }
    }

    fn verify(&self, ctx: &Ctx) -> VerifyResult {
        let mut findings: Vec<Finding> = Vec::new();
        let mut ok = true;

        let Some(content) = read_if_exists(&ctx.target_dir.join(CODEMAP_PATH)) else {
            return VerifyResult {
                ok: false,
                findings: vec![Finding::error_with(
                    format!("{CODEMAP_PATH} missing"),
                    "run `ade apply`",
                )],
            };
        };
        let mut missing: Vec<String> = SECTION_MARKERS
            .iter()
            .filter(|marker| !content.contains(**marker))
            .map(|marker| marker.to_string())
            .collect();
        if !content.contains(REFRESH_SENTENCE) {
            missing.push(format!("Refresh contract (\"{REFRESH_SENTENCE}\")"));
        }
        if !missing.is_empty() {
            return VerifyResult {
                ok: false,
                findings: vec![Finding::error_with(
                    format!(
                        "{CODEMAP_PATH} missing structure markers: {}",
                        missing.join(", ")
                    ),
                    "run `ade apply` to regenerate",
                )],
            };
        }
        findings.push(Finding::ok(format!(
            "{CODEMAP_PATH} present with expected sections"
        )));

        let engines = verify_json_artifact(ctx, CONTEXT_ENGINES_PATH);
        let engines_ok = engines.level == FindingLevel::Ok;
        findings.push(engines);
        if !engines_ok {
            return VerifyResult {
                ok: false,
                findings,
            };
        }

        let parsed = read_json(ctx, CONTEXT_ENGINES_PATH);
        let expected = build_engines_policy(ctx);
        let matches = |pointer: &str| -> bool {
            parsed.as_ref().and_then(|value| value.pointer(pointer)) == expected.pointer(pointer)
        };
        if !(matches("/engines/codebaseWiki/enabled")
            && matches("/engines/semanticIndex/enabled")
            && matches("/engines/personalBrain/enabled"))
        {
            ok = false;
            findings.push(Finding::error_with(
                format!(
                    "{CONTEXT_ENGINES_PATH} engine state does not match the current machine (installed tools / Brain opt-in changed)"
                ),
                "run `ade apply` to re-derive the context-engines policy",
            ));
        } else {
            findings.push(Finding::ok(format!(
                "{CONTEXT_ENGINES_PATH} matches detected engines"
            )));
        }

        VerifyResult { ok, findings }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fsutil::{sha256_hex, write_ensured};
    use crate::testutil::{make_temp_dir, make_test_ctx, test_config_with, TestCtxOptions};
    use crate::types::AdeConfig;
    use std::path::{Path, PathBuf};

    fn read_file(dir: &Path, rel: &str) -> Option<String> {
        read_if_exists(&dir.join(rel))
    }

    /// Parse the written context-engines.json for a target dir.
    fn read_engines(dir: &Path) -> serde_json::Value {
        let text = read_file(dir, CONTEXT_ENGINES_PATH).expect("context-engines.json not written");
        serde_json::from_str(&text).expect("valid JSON")
    }

    fn config_with_brain() -> AdeConfig {
        let mut config = test_config_with(&[], &["claude-code", "codex"]);
        if let Some(module) = config.modules.get_mut("context") {
            module.options = serde_json::json!({"enableBrain": true});
        }
        config
    }

    /// A polyglot fixture tree exercising every entry-point detector and skip rule.
    fn write_fixture_tree(root: &Path) {
        let package_json = serde_json::json!({
            "name": "fixture",
            "main": "src/index.ts",
            "bin": { "fixture": "src/cli.ts" },
            "scripts": { "build": "tsc", "test": "bun test" },
        });
        let cargo_toml = [
            "[package]",
            "name = \"fixture\"",
            "",
            "[[bin]]",
            "name = \"fixture-bin\"",
            "path = \"src/bin/fixture.rs\"",
            "",
        ]
        .join("\n");
        let files: Vec<(&str, String)> = vec![
            (
                "package.json",
                serde_json::to_string(&package_json).unwrap(),
            ),
            ("src/index.ts", "export {};\n".to_string()),
            ("src/cli.ts", "export {};\n".to_string()),
            ("src/util/math.ts", "export {};\n".to_string()),
            ("tests/index.test.ts", "// t\n".to_string()),
            ("readme.md", "# fixture\n".to_string()),
            ("main.go", "package main\n".to_string()),
            ("Cargo.toml", cargo_toml),
            // Everything below must be SKIPPED by the scan (ISC-72).
            ("node_modules/dep/index.js", "// dep\n".to_string()),
            (".git/HEAD", "ref: refs/heads/main\n".to_string()),
            (".ade/policy/old.json", "{}\n".to_string()),
            ("dist/out.js", "// built\n".to_string()),
            ("build/artifact.js", "// built\n".to_string()),
            ("coverage/lcov.info", "TN:\n".to_string()),
        ];
        for (rel, content) in files {
            write_ensured(&root.join(rel), &content).unwrap();
        }
    }

    #[test]
    fn isc_72_apply_generates_codemap_with_dirs_extension_counts_and_all_entry_point_kinds() {
        let dir = make_temp_dir("ctx-codemap");
        write_fixture_tree(&dir);
        let ctx = make_test_ctx(&dir, TestCtxOptions::default());
        let result = MODULE.apply(&ctx);
        assert_eq!(result.status, ModuleStatus::Applied);
        assert!(result.wrote_paths.contains(&CODEMAP_PATH.to_string()));

        let map = read_file(&dir, CODEMAP_PATH).unwrap();
        // Top-level directory listing (only unskipped dirs).
        assert!(map.contains("- `src/`"));
        assert!(map.contains("- `tests/`"));
        // Extension counts: 4 .ts files live outside skipped dirs.
        assert!(map.contains("| .ts | 4 |"));
        assert!(map.contains("| .md | 1 |"));
        // Entry points: package.json main/bin/scripts, src/index.*, src/cli.*, main.go, Cargo [[bin]].
        assert!(map.contains("package.json main: `src/index.ts`"));
        assert!(map.contains("package.json bin: fixture → `src/cli.ts`"));
        assert!(map.contains("package.json scripts: build, test"));
        assert!(map.contains("- `src/index.ts`"));
        assert!(map.contains("- `src/cli.ts`"));
        assert!(map.contains("- `main.go`"));
        assert!(map.contains("Cargo.toml [[bin]]: fixture-bin (`src/bin/fixture.rs`)"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn isc_72_skipped_dirs_are_excluded_from_scan_and_counts() {
        let dir = make_temp_dir("ctx-skip");
        write_fixture_tree(&dir);
        let tree = snapshot_tree(&dir).unwrap();
        for skipped in [
            "node_modules/",
            ".git/",
            ".ade/",
            "dist/",
            "build/",
            "coverage/",
        ] {
            assert!(!tree.files.iter().any(|file| file.starts_with(skipped)));
        }
        let map = build_codemap(&tree);
        // Skipped dirs never appear as listed directories (the prose skip-list note is expected).
        assert!(!map.contains("- `node_modules/`"));
        assert!(!map.contains("- `.git/`"));
        // The only .js and .info files live inside skipped dirs → no such extension rows.
        assert!(!map.contains("| .js |"));
        assert!(!map.contains("| .info |"));
        assert!(!map.contains("- `dist/`"));
        assert!(!map.contains("- `coverage/`"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn isc_73_codemap_generation_is_deterministic_across_identical_trees() {
        let dir = make_temp_dir("ctx-det-a");
        let other = make_temp_dir("ctx-det-b");
        write_fixture_tree(&dir);
        write_fixture_tree(&other);
        MODULE.apply(&make_test_ctx(&dir, TestCtxOptions::default()));
        MODULE.apply(&make_test_ctx(&other, TestCtxOptions::default()));
        let a = read_file(&dir, CODEMAP_PATH).unwrap();
        let b = read_file(&other, CODEMAP_PATH).unwrap();
        assert_eq!(a, b);
        // No absolute paths leak into the artifact.
        assert!(!a.contains(dir.to_str().unwrap()));
        assert!(!b.contains(other.to_str().unwrap()));
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::remove_dir_all(&other);
    }

    #[test]
    fn isc_73_isc_117_double_apply_is_idempotent() {
        let dir = make_temp_dir("ctx-idem");
        write_fixture_tree(&dir);
        MODULE.apply(&make_test_ctx(&dir, TestCtxOptions::default()));
        let first = sha256_hex(&read_file(&dir, CODEMAP_PATH).unwrap());
        MODULE.apply(&make_test_ctx(&dir, TestCtxOptions::default()));
        let second = sha256_hex(&read_file(&dir, CODEMAP_PATH).unwrap());
        assert_eq!(second, first);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn isc_74_refresh_section_names_ade_apply_and_verify_checks_structure_markers() {
        let dir = make_temp_dir("ctx-refresh");
        write_fixture_tree(&dir);
        let ctx = make_test_ctx(&dir, TestCtxOptions::default());
        MODULE.apply(&ctx);
        let map = read_file(&dir, CODEMAP_PATH).unwrap();
        assert!(map.contains("## Refresh"));
        assert!(map.contains(REFRESH_SENTENCE));
        assert!(REFRESH_SENTENCE.contains("`ade apply`"));
        assert!(MODULE.verify(&ctx).ok);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn isc_74_verify_fails_when_codemap_missing_or_section_header_tampered() {
        let dir = make_temp_dir("ctx-tamper");
        let ctx = make_test_ctx(&dir, TestCtxOptions::default());
        // Missing entirely.
        let missing = MODULE.verify(&ctx);
        assert!(!missing.ok);
        assert!(missing.findings.iter().any(|finding| finding
            .remediation
            .as_deref()
            .map(|remediation| remediation.contains("ade apply"))
            .unwrap_or(false)));

        // Present but a structure marker removed.
        MODULE.apply(&ctx);
        let map = read_file(&dir, CODEMAP_PATH).unwrap();
        std::fs::write(
            dir.join(CODEMAP_PATH),
            map.replace("## Entry Points", "## Something Else"),
        )
        .unwrap();
        let tampered = MODULE.verify(&ctx);
        assert!(!tampered.ok);
        assert!(tampered
            .findings
            .iter()
            .any(|finding| finding.message.contains("## Entry Points")));

        // Refresh contract sentence removed also fails.
        MODULE.apply(&make_test_ctx(&dir, TestCtxOptions::default()));
        let regenerated = read_file(&dir, CODEMAP_PATH).unwrap();
        std::fs::write(
            dir.join(CODEMAP_PATH),
            regenerated.replace(REFRESH_SENTENCE, "hand-maintained"),
        )
        .unwrap();
        assert!(!MODULE.verify(&ctx).ok);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn isc_75_instruction_block_directs_harnesses_to_codemap_targeted_reads_and_ade_apply() {
        let blocks = MODULE.instruction_blocks();
        assert!(!blocks.is_empty());
        let block = &blocks[0];
        assert!(block.content.contains(".ade/context/codemap.md"));
        assert!(block.content.contains("whole-repo"));
        assert!(block.content.to_lowercase().contains("targeted"));
        assert!(block.content.contains("`ade apply`"));
    }

    #[test]
    fn isc_116_plan_writes_nothing_to_disk() {
        let dir = make_temp_dir("ctx-plan");
        write_fixture_tree(&dir);
        let ctx = make_test_ctx(&dir, TestCtxOptions::default());
        let actions = MODULE.plan(&ctx);
        assert!(!actions.is_empty());
        assert_eq!(actions[0].path.as_deref(), Some(CODEMAP_PATH));
        assert!(read_file(&dir, CODEMAP_PATH).is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn detect_reports_codemap_absence_with_remediation_presence_after_apply() {
        let dir = make_temp_dir("ctx-detect");
        let ctx = make_test_ctx(&dir, TestCtxOptions::default());
        let before = MODULE.detect(&ctx);
        // Codemap not yet generated → its finding carries an `ade apply` remediation.
        assert!(before
            .iter()
            .any(|finding| finding.message.contains(CODEMAP_PATH)
                && finding
                    .remediation
                    .as_deref()
                    .map(|remediation| remediation.contains("ade apply"))
                    .unwrap_or(false)));
        MODULE.apply(&ctx);
        let after = MODULE.detect(&ctx);
        // The codemap finding flips to ok; engine findings stay advisory (engines absent in the test env).
        assert!(after.iter().any(|finding| finding.level == FindingLevel::Ok
            && finding.message == format!("{CODEMAP_PATH} present")));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Mirror of the TS test's `^\| \.e\d\d \| \d+ \|$` row matcher.
    fn is_ext_row(line: &str) -> bool {
        let Some(rest) = line.strip_prefix("| .e") else {
            return false;
        };
        let bytes = rest.as_bytes();
        if bytes.len() < 2 || !bytes[0].is_ascii_digit() || !bytes[1].is_ascii_digit() {
            return false;
        }
        let Some(rest) = rest[2..].strip_prefix(" | ") else {
            return false;
        };
        let Some(digits) = rest.strip_suffix(" |") else {
            return false;
        };
        !digits.is_empty() && digits.bytes().all(|byte| byte.is_ascii_digit())
    }

    #[test]
    fn build_codemap_extension_table_capped_at_top_10_count_desc_then_ext_asc() {
        let mut files: Vec<String> = Vec::new();
        // 12 distinct extensions; .e00 has 3 files, .e01 has 2, the rest 1 each.
        for i in 0..12 {
            let ext = format!("e{i:02}");
            files.push(format!("src/a.{ext}"));
            if i == 0 {
                files.push(format!("src/b.{ext}"));
                files.push(format!("src/c.{ext}"));
            }
            if i == 1 {
                files.push(format!("src/b.{ext}"));
            }
        }
        files.sort();
        let map = build_codemap(&TreeSnapshot {
            files,
            package_json: None,
            cargo_toml: None,
        });
        let rows: Vec<&str> = map.split('\n').filter(|line| is_ext_row(line)).collect();
        assert_eq!(rows.len(), 10);
        assert_eq!(rows[0], "| .e00 | 3 |");
        assert_eq!(rows[1], "| .e01 | 2 |");
        // Ties (count 1) resolve alphabetically; the two largest extensions drop off.
        assert_eq!(rows[2], "| .e02 | 1 |");
        assert!(!map.contains("| .e10 |"));
        assert!(!map.contains("| .e11 |"));
    }

    #[test]
    fn build_codemap_empty_tree_renders_placeholder_sections_deterministically() {
        let empty = TreeSnapshot {
            files: vec![],
            package_json: None,
            cargo_toml: None,
        };
        let map = build_codemap(&empty);
        for marker in SECTION_MARKERS {
            assert!(map.contains(marker));
        }
        assert!(map.contains("- (none)"));
        assert!(map.contains("- (none detected)"));
        assert_eq!(build_codemap(&empty), map);
    }

    #[test]
    fn build_codemap_package_json_string_form_bin_detected_as_entry_point() {
        let package_json = match serde_json::json!({"bin": "cli.js"}) {
            serde_json::Value::Object(map) => Some(map),
            _ => None,
        };
        let map = build_codemap(&TreeSnapshot {
            files: vec!["cli.js".to_string()],
            package_json,
            cargo_toml: None,
        });
        assert!(map.contains("package.json bin: `cli.js`"));
    }

    #[test]
    fn parse_cargo_bins_extracts_every_bin_block_and_ignores_other_sections() {
        let toml = [
            "[package]",
            "name = \"not-a-bin\"",
            "",
            "[[bin]]",
            "name = \"alpha\"",
            "path = \"src/bin/alpha.rs\"",
            "",
            "[dependencies]",
            "serde = \"1\"",
            "",
            "[[bin]]",
            "name = \"beta\"",
            "",
        ]
        .join("\n");
        let bins = parse_cargo_bins(&toml);
        assert_eq!(
            bins,
            vec![
                CargoBin {
                    name: Some("alpha".to_string()),
                    path: Some("src/bin/alpha.rs".to_string())
                },
                CargoBin {
                    name: Some("beta".to_string()),
                    path: None
                },
            ]
        );
    }

    // ——— OpenWiki + CocoIndex + Personal Brain integration ———

    #[test]
    fn apply_writes_context_engines_json_alongside_codemap_both_in_wrote_paths() {
        let dir = make_temp_dir("ctx-engines");
        let ctx = make_test_ctx(&dir, TestCtxOptions::default());
        let result = MODULE.apply(&ctx);
        assert_eq!(result.status, ModuleStatus::Applied);
        assert!(result.wrote_paths.contains(&CODEMAP_PATH.to_string()));
        assert!(result
            .wrote_paths
            .contains(&CONTEXT_ENGINES_PATH.to_string()));
        let engines = read_engines(&dir);
        assert_eq!(engines["fallback"], serde_json::json!(CODEMAP_PATH));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn engines_absent_all_disabled_brain_off_verify_ok() {
        let dir = make_temp_dir("ctx-absent");
        let ctx = make_test_ctx(&dir, TestCtxOptions::default()); // no tools present
        MODULE.apply(&ctx);
        let policy = build_engines_policy(&ctx);
        assert_eq!(
            policy["engines"]["codebaseWiki"]["enabled"],
            serde_json::json!(false)
        );
        assert_eq!(
            policy["engines"]["semanticIndex"]["enabled"],
            serde_json::json!(false)
        );
        assert_eq!(
            policy["engines"]["personalBrain"]["enabled"],
            serde_json::json!(false)
        );
        assert_eq!(
            policy["engines"]["personalBrain"]["optIn"],
            serde_json::json!(false)
        );
        // Install guidance is carried in the policy so the contract is explicit.
        assert_eq!(
            policy["engines"]["codebaseWiki"]["install"],
            serde_json::json!(OPENWIKI_INSTALL)
        );
        assert_eq!(
            policy["engines"]["semanticIndex"]["install"],
            serde_json::json!(COCOINDEX_INSTALL)
        );
        let verdict = MODULE.verify(&ctx);
        assert!(verdict.ok);
        // detect surfaces advisory (non-error) findings for the absent engines.
        let detected = MODULE.detect(&ctx);
        assert!(detected
            .iter()
            .any(|finding| finding.level == FindingLevel::Degraded
                && finding.remediation.as_deref() == Some(OPENWIKI_INSTALL)));
        assert!(detected
            .iter()
            .any(|finding| finding.level == FindingLevel::Degraded
                && finding.remediation.as_deref() == Some(COCOINDEX_INSTALL)));
        assert!(detected
            .iter()
            .any(|finding| finding.remediation.as_deref() == Some(BRAIN_ACTIVATION)));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn openwiki_present_wiki_enabled_with_version_brain_present_but_off() {
        let dir = make_temp_dir("ctx-openwiki");
        let ctx = make_test_ctx(
            &dir,
            TestCtxOptions {
                present_tools: &[("openwiki", "openwiki 1.4.0")],
                ..Default::default()
            },
        );
        assert_eq!(
            openwiki_state(&ctx),
            EngineState {
                present: true,
                version: Some("openwiki 1.4.0".to_string())
            }
        );
        let policy = build_engines_policy(&ctx);
        assert_eq!(
            policy["engines"]["codebaseWiki"]["enabled"],
            serde_json::json!(true)
        );
        assert_eq!(
            policy["engines"]["codebaseWiki"]["version"],
            serde_json::json!("openwiki 1.4.0")
        );
        // Brain shares the binary (present) but stays disabled until opted in.
        assert_eq!(
            policy["engines"]["personalBrain"]["present"],
            serde_json::json!(true)
        );
        assert_eq!(
            policy["engines"]["personalBrain"]["enabled"],
            serde_json::json!(false)
        );
        MODULE.apply(&ctx);
        assert!(MODULE.verify(&ctx).ok);
        let detected = MODULE.detect(&ctx);
        assert!(detected
            .iter()
            .any(|finding| finding.level == FindingLevel::Ok
                && finding.message.starts_with("OpenWiki codebase wiki wired")));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn cocoindex_present_via_framework_semantic_index_enabled() {
        let dir = make_temp_dir("ctx-coco");
        let ctx = make_test_ctx(
            &dir,
            TestCtxOptions {
                present_tools: &[("cocoindex", "cocoindex 0.9")],
                ..Default::default()
            },
        );
        assert_eq!(
            cocoindex_state(&ctx),
            EngineState {
                present: true,
                version: Some("cocoindex 0.9".to_string())
            }
        );
        assert_eq!(
            build_engines_policy(&ctx)["engines"]["semanticIndex"]["enabled"],
            serde_json::json!(true)
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn cocoindex_present_via_ccc_cli_alias_either_binary_counts() {
        let dir = make_temp_dir("ctx-ccc");
        let ctx = make_test_ctx(
            &dir,
            TestCtxOptions {
                present_tools: &[("ccc", "ccc 0.3.2")],
                ..Default::default()
            },
        );
        assert_eq!(
            cocoindex_state(&ctx),
            EngineState {
                present: true,
                version: Some("ccc 0.3.2".to_string())
            }
        );
        assert_eq!(
            build_engines_policy(&ctx)["engines"]["semanticIndex"]["version"],
            serde_json::json!("ccc 0.3.2")
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn personal_brain_opt_in_with_openwiki_enabled_without_openwiki_not_enabled() {
        let dir = make_temp_dir("ctx-brain");
        let on = make_test_ctx(
            &dir,
            TestCtxOptions {
                config: Some(config_with_brain()),
                present_tools: &[("openwiki", "openwiki 1.4.0")],
                ..Default::default()
            },
        );
        assert!(brain_opt_in(&on));
        assert_eq!(
            build_engines_policy(&on)["engines"]["personalBrain"]["enabled"],
            serde_json::json!(true)
        );

        // Opted in, OpenWiki absent.
        let opt_in_no_tool = make_test_ctx(
            &dir,
            TestCtxOptions {
                config: Some(config_with_brain()),
                ..Default::default()
            },
        );
        let policy = build_engines_policy(&opt_in_no_tool);
        assert_eq!(
            policy["engines"]["personalBrain"]["optIn"],
            serde_json::json!(true)
        );
        assert_eq!(
            policy["engines"]["personalBrain"]["enabled"],
            serde_json::json!(false)
        );
        let detected = MODULE.detect(&opt_in_no_tool);
        assert!(detected
            .iter()
            .any(|finding| finding.message.contains("opted-in but OpenWiki absent")));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn verify_fails_when_recorded_engine_state_drifts_from_live_machine() {
        let dir = make_temp_dir("ctx-drift");
        // Apply with no engines, then a tool appears — recorded enabled=false ≠ live present.
        let apply_ctx = make_test_ctx(&dir, TestCtxOptions::default());
        MODULE.apply(&apply_ctx);
        let drifted_ctx = make_test_ctx(
            &dir,
            TestCtxOptions {
                present_tools: &[("openwiki", "openwiki 1.4.0")],
                ..Default::default()
            },
        );
        let verdict = MODULE.verify(&drifted_ctx);
        assert!(!verdict.ok);
        assert!(verdict
            .findings
            .iter()
            .any(|finding| finding.level == FindingLevel::Error
                && finding
                    .remediation
                    .as_deref()
                    .map(|remediation| remediation.contains("re-derive"))
                    .unwrap_or(false)));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn verify_fails_when_context_engines_json_missing_after_codemap_exists() {
        let dir = make_temp_dir("ctx-missing-engines");
        let ctx = make_test_ctx(&dir, TestCtxOptions::default());
        MODULE.apply(&ctx);
        std::fs::remove_file(dir.join(CONTEXT_ENGINES_PATH)).unwrap();
        let verdict = MODULE.verify(&ctx);
        assert!(!verdict.ok);
        assert!(verdict
            .findings
            .iter()
            .any(|finding| finding.message.contains(CONTEXT_ENGINES_PATH)));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn plan_lists_engines_policy_and_reflects_brain_opt_in_in_description() {
        let dir: PathBuf = make_temp_dir("ctx-plan-brain");
        let off = MODULE.plan(&make_test_ctx(&dir, TestCtxOptions::default()));
        assert!(off.iter().any(
            |action| action.path.as_deref() == Some(CONTEXT_ENGINES_PATH)
                && !action.description.contains("Personal Brain")
        ));
        let on_ctx = make_test_ctx(
            &dir,
            TestCtxOptions {
                config: Some(config_with_brain()),
                ..Default::default()
            },
        );
        let on = MODULE.plan(&on_ctx);
        assert!(on.iter().any(
            |action| action.path.as_deref() == Some(CONTEXT_ENGINES_PATH)
                && action.description.contains("Personal Brain")
        ));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn instruction_block_names_all_four_sources_including_brain_and_no_secrets_rule() {
        let blocks = MODULE.instruction_blocks();
        let block = &blocks[0];
        assert!(block.content.contains("OpenWiki codebase wiki"));
        assert!(block.content.contains("CocoIndex semantic search"));
        assert!(block.content.contains("OpenWiki Personal Brain"));
        assert!(block.content.contains(CODEMAP_PATH));
        assert!(block.content.to_lowercase().contains("never write secrets"));
    }
}
