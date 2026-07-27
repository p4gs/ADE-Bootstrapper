//! Harness adapter registry — port of `src/harness/adapters.ts`.
//! Seven harnesses, capability flags, instruction-file targets.

use crate::types::{HarnessAdapter, HarnessCapabilities};

pub const HARNESS_ADAPTERS: [HarnessAdapter; 7] = [
    HarnessAdapter {
        id: "claude-code",
        title: "Claude Code",
        instruction_file: "CLAUDE.md",
        cli_names: &["claude"],
        config_signals: &["CLAUDE.md", ".claude/settings.json", ".claude"],
        capabilities: HarnessCapabilities {
            hooks: true,
            mcp: true,
            permissions: true,
        },
    },
    HarnessAdapter {
        id: "codex",
        title: "Codex",
        instruction_file: "AGENTS.md",
        cli_names: &["codex"],
        config_signals: &["AGENTS.md", ".codex"],
        capabilities: HarnessCapabilities {
            hooks: false,
            mcp: true,
            permissions: false,
        },
    },
    HarnessAdapter {
        id: "cursor",
        title: "Cursor",
        instruction_file: ".cursor/rules/ade.mdc",
        cli_names: &["cursor"],
        config_signals: &[".cursor/rules", ".cursorrules", ".cursor"],
        capabilities: HarnessCapabilities {
            hooks: false,
            mcp: true,
            permissions: false,
        },
    },
    HarnessAdapter {
        id: "opencode",
        title: "OpenCode",
        instruction_file: "AGENTS.md",
        cli_names: &["opencode"],
        config_signals: &["AGENTS.md", "opencode.json"],
        capabilities: HarnessCapabilities {
            hooks: false,
            mcp: true,
            permissions: true,
        },
    },
    HarnessAdapter {
        id: "antigravity",
        title: "Antigravity",
        instruction_file: "AGENTS.md",
        cli_names: &["antigravity"],
        config_signals: &["AGENTS.md", ".antigravity"],
        capabilities: HarnessCapabilities {
            hooks: false,
            mcp: false,
            permissions: false,
        },
    },
    HarnessAdapter {
        id: "hermes",
        title: "Hermes",
        instruction_file: "AGENTS.md",
        cli_names: &["hermes"],
        config_signals: &["AGENTS.md", ".hermes"],
        capabilities: HarnessCapabilities {
            hooks: false,
            mcp: false,
            permissions: false,
        },
    },
    HarnessAdapter {
        id: "pi",
        title: "Pi",
        instruction_file: "AGENTS.md",
        cli_names: &["pi"],
        config_signals: &["AGENTS.md", ".pi"],
        capabilities: HarnessCapabilities {
            hooks: false,
            mcp: false,
            permissions: false,
        },
    },
];

pub fn get_adapter(id: &str) -> Option<&'static HarnessAdapter> {
    HARNESS_ADAPTERS.iter().find(|adapter| adapter.id == id)
}

pub struct RenderedInstructionFile {
    pub path: &'static str,
    pub fresh_file_prefix: &'static str,
}

/// Per-harness render target: cursor gets `.mdc` frontmatter on fresh files.
pub fn render_target(adapter: &HarnessAdapter) -> RenderedInstructionFile {
    if adapter.id == "cursor" {
        RenderedInstructionFile {
            path: adapter.instruction_file,
            fresh_file_prefix:
                "---\ndescription: ADE Bootstrapper baseline (managed)\nalwaysApply: true\n---\n\n",
        }
    } else {
        RenderedInstructionFile {
            path: adapter.instruction_file,
            fresh_file_prefix: "",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seven_adapters_with_unique_ids() {
        assert_eq!(HARNESS_ADAPTERS.len(), 7);
        let mut ids: Vec<&str> = HARNESS_ADAPTERS.iter().map(|adapter| adapter.id).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), 7);
        assert!(get_adapter("claude-code").is_some());
        assert!(get_adapter("emacs").is_none());
    }

    #[test]
    fn cursor_gets_mdc_frontmatter_others_do_not() {
        let cursor = render_target(get_adapter("cursor").unwrap());
        assert!(cursor.fresh_file_prefix.starts_with("---\n"));
        assert_eq!(cursor.path, ".cursor/rules/ade.mdc");
        let codex = render_target(get_adapter("codex").unwrap());
        assert_eq!(codex.fresh_file_prefix, "");
        assert_eq!(codex.path, "AGENTS.md");
    }

    #[test]
    fn only_claude_code_has_hooks_and_permissions() {
        let hooks: Vec<&str> = HARNESS_ADAPTERS
            .iter()
            .filter(|adapter| adapter.capabilities.hooks)
            .map(|adapter| adapter.id)
            .collect();
        assert_eq!(hooks, vec!["claude-code"]);
    }
}
