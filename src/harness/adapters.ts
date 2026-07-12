/**
 * Harness adapters — one per supported coding harness (spec: Claude Code,
 * Codex, Antigravity, Cursor, Hermes, Pi, OpenCode).
 *
 * Adapters are declarative: instruction file, detection signals, capability
 * flags, and an optional per-harness rendering of the canonical instructions
 * (Cursor needs `.cursor/rules/*.mdc` frontmatter; plain-markdown harnesses
 * share the managed-block rendering).
 *
 * Several harnesses read the open-standard AGENTS.md; translation dedupes by
 * instruction file path so shared files are written once.
 */
import type { HarnessAdapter } from "../types.ts";

export interface RenderedInstructionFile {
  /** Repo-relative path to write. */
  path: string;
  /** Prefix written before the managed block (e.g. .mdc frontmatter) when the file is created fresh. */
  freshFilePrefix: string;
}

export const HARNESS_ADAPTERS: HarnessAdapter[] = [
  {
    id: "claude-code",
    title: "Claude Code",
    instructionFile: "CLAUDE.md",
    cliNames: ["claude"],
    configSignals: ["CLAUDE.md", ".claude/settings.json", ".claude"],
    capabilities: { hooks: true, mcp: true, permissions: true },
  },
  {
    id: "codex",
    title: "Codex",
    instructionFile: "AGENTS.md",
    cliNames: ["codex"],
    configSignals: ["AGENTS.md", ".codex"],
    capabilities: { hooks: false, mcp: true, permissions: false },
  },
  {
    id: "cursor",
    title: "Cursor",
    instructionFile: ".cursor/rules/ade.mdc",
    cliNames: ["cursor"],
    // .cursorrules is the legacy single-file surface — a detection signal, not a write target.
    configSignals: [".cursor/rules", ".cursorrules", ".cursor"],
    capabilities: { hooks: false, mcp: true, permissions: false },
  },
  {
    id: "opencode",
    title: "OpenCode",
    instructionFile: "AGENTS.md",
    cliNames: ["opencode"],
    configSignals: ["AGENTS.md", "opencode.json"],
    capabilities: { hooks: false, mcp: true, permissions: true },
  },
  {
    id: "antigravity",
    title: "Antigravity",
    instructionFile: "AGENTS.md",
    cliNames: ["antigravity"],
    configSignals: ["AGENTS.md", ".antigravity"],
    capabilities: { hooks: false, mcp: false, permissions: false },
  },
  {
    id: "hermes",
    title: "Hermes",
    instructionFile: "AGENTS.md",
    cliNames: ["hermes"],
    configSignals: ["AGENTS.md", ".hermes"],
    capabilities: { hooks: false, mcp: false, permissions: false },
  },
  {
    id: "pi",
    title: "Pi",
    instructionFile: "AGENTS.md",
    cliNames: ["pi"],
    configSignals: ["AGENTS.md", ".pi"],
    capabilities: { hooks: false, mcp: false, permissions: false },
  },
];

export function getAdapter(id: string): HarnessAdapter | undefined {
  return HARNESS_ADAPTERS.find((adapter) => adapter.id === id);
}

/**
 * Per-harness rendering details for the canonical instructions.
 * Cursor project rules are `.mdc` with frontmatter; everything else is a
 * plain markdown managed block.
 */
export function renderTarget(adapter: HarnessAdapter): RenderedInstructionFile {
  if (adapter.id === "cursor") {
    return {
      path: adapter.instructionFile,
      freshFilePrefix: "---\ndescription: ADE Bootstrapper baseline (managed)\nalwaysApply: true\n---\n\n",
    };
  }
  return { path: adapter.instructionFile, freshFilePrefix: "" };
}
