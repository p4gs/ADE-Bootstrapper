import { afterEach, beforeEach, describe, expect, test } from "bun:test";
import { join } from "node:path";
import { mergeClaudeSettings, registerMcpServer, wireClaudeHook, CLAUDE_SETTINGS_PATH, MCP_CONFIG_PATH } from "../src/harness/claude.ts";
import { makeTempDir, makeTestCtx, removeDir } from "./helpers.ts";

let dir: string;

beforeEach(async () => {
  dir = await makeTempDir();
});

afterEach(async () => {
  await removeDir(dir);
});

describe("claude-code capability helpers", () => {
  test("ISC-50: permission patch merges additively, preserving user entries", async () => {
    await Bun.write(
      join(dir, CLAUDE_SETTINGS_PATH),
      JSON.stringify({ permissions: { deny: ["Bash(user-rule:*)"] }, model: "user-choice" }),
    );
    const ctx = makeTestCtx(dir);
    const finding = await mergeClaudeSettings(ctx, { permissions: { deny: ["Bash(rm -rf /:*)"] } });
    expect(finding.level).toBe("ok");
    const settings = JSON.parse(await Bun.file(join(dir, CLAUDE_SETTINGS_PATH)).text());
    expect(settings.permissions.deny).toContain("Bash(user-rule:*)");
    expect(settings.permissions.deny).toContain("Bash(rm -rf /:*)");
    expect(settings.model).toBe("user-choice");
  });

  test("unparseable settings.json is refused, never clobbered", async () => {
    await Bun.write(join(dir, CLAUDE_SETTINGS_PATH), "{broken json");
    const ctx = makeTestCtx(dir);
    const finding = await mergeClaudeSettings(ctx, { permissions: {} });
    expect(finding.level).toBe("error");
    expect(await Bun.file(join(dir, CLAUDE_SETTINGS_PATH)).text()).toBe("{broken json");
  });

  test("ISC-51: MCP registration preserves a pre-existing user server of the same name", async () => {
    await Bun.write(
      join(dir, MCP_CONFIG_PATH),
      JSON.stringify({ mcpServers: { openmemory: { command: "user-custom" } } }),
    );
    const ctx = makeTestCtx(dir);
    const finding = await registerMcpServer(ctx, "openmemory", { command: "npx", args: ["-y", "openmemory"] });
    expect(finding.level).toBe("info");
    const mcp = JSON.parse(await Bun.file(join(dir, MCP_CONFIG_PATH)).text());
    expect(mcp.mcpServers.openmemory.command).toBe("user-custom");
  });

  test("ISC-51: MCP registration adds a new server beside user servers", async () => {
    await Bun.write(join(dir, MCP_CONFIG_PATH), JSON.stringify({ mcpServers: { mine: { command: "keep" } } }));
    const ctx = makeTestCtx(dir);
    const finding = await registerMcpServer(ctx, "openmemory", { command: "npx" });
    expect(finding.level).toBe("ok");
    const mcp = JSON.parse(await Bun.file(join(dir, MCP_CONFIG_PATH)).text());
    expect(mcp.mcpServers.mine.command).toBe("keep");
    expect(mcp.mcpServers.openmemory.command).toBe("npx");
  });

  test("ISC-86-adjacent: hook wiring is idempotent (deduped on identical entries)", async () => {
    const ctx = makeTestCtx(dir);
    await wireClaudeHook(ctx, "PostToolUse", "bun .ade/hooks/audit-log.ts");
    await wireClaudeHook(makeTestCtx(dir), "PostToolUse", "bun .ade/hooks/audit-log.ts");
    const settings = JSON.parse(await Bun.file(join(dir, CLAUDE_SETTINGS_PATH)).text());
    expect(settings.hooks.PostToolUse.length).toBe(1);
    expect(settings.hooks.PostToolUse[0].hooks[0].command).toBe("bun .ade/hooks/audit-log.ts");
  });
});
