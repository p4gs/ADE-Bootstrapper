/**
 * Module: network-syncable agent memory.
 * Spec component: "Network-syncable agent memory — persistent, portable agent
 * memory via the OpenMemory/Mem0 MCP server, local-first by default with
 * user-controlled sync."
 * Boundary controlled: the memory/data boundary (sensitive context leaving
 * the machine). Memory content is sensitive user data: the store is always
 * git-ignored, and the credential-bearing MCP integration is strictly OPT-IN
 * (`options.enableMcp = true`) — the default apply never touches `.mcp.json`.
 */
import { join } from "node:path";
import { ensureLines, readIfExists } from "../fsutil.ts";
import { readJson, verifyJsonArtifact, writePolicy } from "./_shared.ts";
import { registerMcpServer } from "../harness/claude.ts";
import type { AdeModule, Ctx, Finding, ModuleResult, PlannedAction } from "../types.ts";

export const MEMORY_POLICY_PATH = ".ade/memory.json";
export const MEMORY_STORE_PATH = ".ade/memory-store/";
export const MCP_SERVER_NAME = "openmemory";

const ACTIVATION_INSTRUCTIONS =
  "to enable the OpenMemory MCP server, set modules.memory.options.enableMcp = true in ade.json, then run `ade apply`";

interface MemoryPolicy {
  schemaVersion: number;
  provider: string;
  project: string;
  posture: string;
  storePath: string;
  sensitivity: {
    classification: string;
    rule: string;
    gitIgnored: boolean;
  };
  sync: string;
  activation: { mcp: string };
}

function memoryPolicy(): MemoryPolicy {
  return {
    schemaVersion: 1,
    provider: "openmemory",
    project: "mem0.ai/openmemory",
    posture: "local-first",
    storePath: MEMORY_STORE_PATH,
    sensitivity: {
      classification: "sensitive-user-data",
      rule: "memory content is never committed to version control and never leaves the machine without explicit user-configured sync",
      gitIgnored: true,
    },
    sync: "the memory server runs locally; network sync is user-controlled and off until the user configures it",
    activation: { mcp: ACTIVATION_INSTRUCTIONS },
  };
}

function moduleOptions(ctx: Ctx): Record<string, unknown> {
  return ctx.config.modules["memory"]?.options ?? {};
}

function mcpEnabled(ctx: Ctx): boolean {
  return (
    ctx.config.harnesses.includes("claude-code") &&
    moduleOptions(ctx)["enableMcp"] === true
  );
}

export const memoryModule: AdeModule = {
  id: "memory",
  title: "Network-Syncable Agent Memory",
  category: "context",
  spec: "Network-syncable agent memory",
  defaultEnabled: true,
  instructionBlocks: [
    {
      id: "memory",
      title: "Agent Memory",
      content: [
        "Persistent agent memory lives in the OpenMemory MCP server when activated (see `.ade/memory.json`).",
        "- Memory content is sensitive user data — the store (`.ade/memory-store/`) is git-ignored; never commit it or copy it into tracked files.",
        "- NEVER write secrets, credentials, or tokens into memory.",
        "- Memory is local-first; do not configure network sync on the user's behalf — sync is user-controlled.",
      ].join("\n"),
    },
  ],

  async detect(ctx: Ctx): Promise<Finding[]> {
    if (mcpEnabled(ctx)) {
      return [
        {
          level: "ok",
          message: "OpenMemory MCP integration enabled (options.enableMcp = true, claude-code harness targeted)",
        },
      ];
    }
    return [
      {
        level: "info",
        message: "OpenMemory MCP integration is opt-in and currently OFF — memory policy is documentation-only",
        remediation: ACTIVATION_INSTRUCTIONS,
      },
    ];
  },

  async plan(ctx: Ctx): Promise<PlannedAction[]> {
    const actions: PlannedAction[] = [
      {
        kind: "write",
        path: MEMORY_POLICY_PATH,
        description: "write agent memory policy (openmemory provider, local-first posture, sensitivity rules)",
      },
      {
        kind: "append",
        path: ".gitignore",
        description: `ensure the memory store (${MEMORY_STORE_PATH}) is git-ignored`,
      },
    ];
    if (mcpEnabled(ctx)) {
      actions.push({
        kind: "merge",
        path: ".mcp.json",
        description: `register the "${MCP_SERVER_NAME}" MCP server (opt-in, preserves user entries)`,
      });
    } else {
      actions.push({
        kind: "info",
        description: `OpenMemory MCP registration skipped — opt-in via options.enableMcp (.mcp.json untouched)`,
      });
    }
    return actions;
  },

  async apply(ctx: Ctx): Promise<ModuleResult> {
    const findings: Finding[] = [];
    const wrotePaths: string[] = [];

    await writePolicy(ctx, MEMORY_POLICY_PATH, memoryPolicy());
    wrotePaths.push(MEMORY_POLICY_PATH);
    findings.push({ level: "ok", message: `wrote ${MEMORY_POLICY_PATH}` });

    await ensureLines(
      join(ctx.targetDir, ".gitignore"),
      [MEMORY_STORE_PATH],
      "ADE Bootstrapper — agent memory (sensitive, never committed)",
    );
    findings.push({ level: "ok", message: `.gitignore covers ${MEMORY_STORE_PATH}` });

    if (mcpEnabled(ctx)) {
      const registration = await registerMcpServer(ctx, MCP_SERVER_NAME, {
        command: "npx",
        args: ["-y", "openmemory"],
        env: {},
      });
      findings.push(registration);
      if (registration.level === "error") {
        return { status: "degraded", findings, wrotePaths };
      }
      if (registration.level === "ok") wrotePaths.push(".mcp.json");
    } else {
      findings.push({
        level: "info",
        message:
          "OpenMemory MCP server NOT registered — credential-bearing integrations are opt-in; .mcp.json untouched",
        remediation: ACTIVATION_INSTRUCTIONS,
      });
    }

    return { status: "applied", findings, wrotePaths };
  },

  async verify(ctx: Ctx) {
    const findings: Finding[] = [await verifyJsonArtifact(ctx, MEMORY_POLICY_PATH)];
    let ok = findings.every((finding) => finding.level === "ok");

    if (ok) {
      const parsed = (await readJson(ctx, MEMORY_POLICY_PATH)) as Partial<MemoryPolicy> | null;
      if (
        parsed === null ||
        parsed.provider !== "openmemory" ||
        parsed.posture !== "local-first" ||
        parsed.storePath !== MEMORY_STORE_PATH
      ) {
        ok = false;
        findings.push({
          level: "error",
          message: `${MEMORY_POLICY_PATH} does not declare the openmemory/local-first contract`,
          remediation: "run `ade apply` to regenerate",
        });
      }
    }

    const gitignore = (await readIfExists(join(ctx.targetDir, ".gitignore"))) ?? "";
    const covered = gitignore
      .split("\n")
      .map((line) => line.trim())
      .includes(MEMORY_STORE_PATH);
    if (!covered) {
      ok = false;
      findings.push({
        level: "error",
        message: `.gitignore does not cover ${MEMORY_STORE_PATH} — memory content could be committed`,
        remediation: "run `ade apply`",
      });
    } else {
      findings.push({ level: "ok", message: `.gitignore covers ${MEMORY_STORE_PATH}` });
    }

    return { ok, findings };
  },
};
