/**
 * Claude Code capability implementations: permission merging, MCP server
 * registration, and hook wiring — the three surfaces the claude-code adapter
 * declares.
 *
 * All merges are additive: ADE only ever ADDS entries (arrays union, objects
 * deep-merge); a pre-existing user entry with the same identity is preserved.
 * An unparseable user file is never clobbered — the merge is refused.
 */
import { join } from "node:path";
import { deepMerge, readIfExists, stableStringify } from "../fsutil.ts";
import type { Ctx, Finding } from "../types.ts";

export const CLAUDE_SETTINGS_PATH = ".claude/settings.json";
export const MCP_CONFIG_PATH = ".mcp.json";

async function mergeJsonFile(
  ctx: Ctx,
  relPath: string,
  patch: Record<string, unknown>,
): Promise<Finding> {
  const absolute = join(ctx.targetDir, relPath);
  const existingText = await readIfExists(absolute);
  let base: Record<string, unknown> = {};
  if (existingText !== null) {
    try {
      const parsed = JSON.parse(existingText);
      if (parsed === null || typeof parsed !== "object" || Array.isArray(parsed)) {
        return { level: "error", message: `${relPath} is not a JSON object — refusing to merge` };
      }
      base = parsed as Record<string, unknown>;
    } catch {
      return {
        level: "error",
        message: `${relPath} is not valid JSON — refusing to merge`,
        remediation: `fix the JSON in ${relPath}, then run \`ade apply\``,
      };
    }
  }
  const merged = deepMerge(base, patch);
  await ctx.artifacts.write(relPath, stableStringify(merged));
  return { level: "ok", message: `merged ADE entries into ${relPath}` };
}

/** Merge permission (or other) settings into `.claude/settings.json`. */
export async function mergeClaudeSettings(
  ctx: Ctx,
  patch: Record<string, unknown>,
): Promise<Finding> {
  return await mergeJsonFile(ctx, CLAUDE_SETTINGS_PATH, patch);
}

/**
 * Register an MCP server in `.mcp.json`. A pre-existing user server with the
 * same name is preserved untouched.
 */
export async function registerMcpServer(
  ctx: Ctx,
  name: string,
  server: Record<string, unknown>,
): Promise<Finding> {
  const existingText = await readIfExists(join(ctx.targetDir, MCP_CONFIG_PATH));
  if (existingText !== null) {
    try {
      const parsed = JSON.parse(existingText) as Record<string, unknown>;
      const servers = parsed["mcpServers"];
      if (servers !== null && typeof servers === "object" && !Array.isArray(servers) && name in (servers as Record<string, unknown>)) {
        return { level: "info", message: `.mcp.json already defines "${name}" — preserved user entry` };
      }
    } catch {
      return {
        level: "error",
        message: `${MCP_CONFIG_PATH} is not valid JSON — refusing to merge`,
        remediation: `fix the JSON in ${MCP_CONFIG_PATH}, then run \`ade apply\``,
      };
    }
  }
  return await mergeJsonFile(ctx, MCP_CONFIG_PATH, { mcpServers: { [name]: server } });
}

/**
 * Wire a hook command into `.claude/settings.json` under the given event.
 * Idempotent — deduped by the array-union merge on identical entries.
 */
export async function wireClaudeHook(
  ctx: Ctx,
  event: string,
  command: string,
  matcher = "*",
): Promise<Finding> {
  return await mergeClaudeSettings(ctx, {
    hooks: {
      [event]: [
        {
          matcher,
          hooks: [{ type: "command", command }],
        },
      ],
    },
  });
}
