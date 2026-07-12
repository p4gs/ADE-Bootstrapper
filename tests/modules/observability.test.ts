import { afterEach, beforeEach, describe, expect, test } from "bun:test";
import { join } from "node:path";
import { rm } from "node:fs/promises";
import {
  observabilityModule,
  auditHookScript,
  AUDIT_README_PATH,
  AUDIT_HOOK_PATH,
  AUDIT_LOG_PATH,
  AUDIT_HOOK_COMMAND,
} from "../../src/modules/observability.ts";
import { parseLog, verifyChain, type AuditEntry } from "../../src/audit.ts";
import { AUDIT_GENESIS } from "../../src/version.ts";
import { CLAUDE_SETTINGS_PATH } from "../../src/harness/claude.ts";
import { sha256 } from "../../src/fsutil.ts";
import { makeTempDir, makeTestCtx, removeDir, testConfig } from "../helpers.ts";

let dir: string;

beforeEach(async () => {
  dir = await makeTempDir();
});

afterEach(async () => {
  await removeDir(dir);
});

/** Run the shipped hook script exactly as Claude Code would: bun + stdin JSON, cwd = repo. */
async function runHook(cwd: string, payload: string): Promise<number> {
  const proc = Bun.spawn(["bun", AUDIT_HOOK_PATH], {
    cwd,
    stdin: Buffer.from(payload),
    stdout: "ignore",
    stderr: "ignore",
  });
  return await proc.exited;
}

async function readLog(cwd: string): Promise<AuditEntry[]> {
  return parseLog(await Bun.file(join(cwd, AUDIT_LOG_PATH)).text());
}

describe("observability module", () => {
  test("ISC-85: apply writes the audit README (chain, genesis, verify command) but never log.jsonl", async () => {
    const ctx = makeTestCtx(dir);
    const result = await observabilityModule.apply(ctx);
    expect(result.status).toBe("applied");
    expect(result.wrotePaths).toContain(AUDIT_README_PATH);
    const readme = await Bun.file(join(dir, AUDIT_README_PATH)).text();
    expect(readme).toContain(AUDIT_GENESIS);
    expect(readme).toContain("ade audit verify");
    expect(readme.toLowerCase()).toContain("hash");
    // The chain is initialized by the pipeline — the module must not create it.
    expect(await Bun.file(join(dir, AUDIT_LOG_PATH)).exists()).toBe(false);
  });

  test("ISC-86: claude-code targeted → hook script shipped and PostToolUse wired in settings.json", async () => {
    const ctx = makeTestCtx(dir);
    const result = await observabilityModule.apply(ctx);
    expect(result.status).toBe("applied");
    expect(result.wrotePaths).toContain(AUDIT_HOOK_PATH);
    expect(result.wrotePaths).toContain(CLAUDE_SETTINGS_PATH);
    expect(await Bun.file(join(dir, AUDIT_HOOK_PATH)).exists()).toBe(true);
    const settings = JSON.parse(await Bun.file(join(dir, CLAUDE_SETTINGS_PATH)).text());
    const post = settings.hooks.PostToolUse;
    expect(Array.isArray(post)).toBe(true);
    expect(JSON.stringify(post)).toContain(AUDIT_HOOK_COMMAND);
  });

  test("ISC-86: claude-code NOT targeted → no hook script, no settings.json touched", async () => {
    const ctx = makeTestCtx(dir, { config: testConfig({ harnesses: ["codex"] }) });
    const result = await observabilityModule.apply(ctx);
    expect(result.status).toBe("applied");
    expect(await Bun.file(join(dir, AUDIT_HOOK_PATH)).exists()).toBe(false);
    expect(await Bun.file(join(dir, CLAUDE_SETTINGS_PATH)).exists()).toBe(false);
  });

  test("ISC-86: pre-existing user hooks in settings.json are preserved by the additive merge", async () => {
    const userSettings = {
      hooks: { PostToolUse: [{ matcher: "*", hooks: [{ type: "command", command: "bun user-hook.ts" }] }] },
    };
    await Bun.write(join(dir, CLAUDE_SETTINGS_PATH), JSON.stringify(userSettings));
    await observabilityModule.apply(makeTestCtx(dir));
    const merged = JSON.parse(await Bun.file(join(dir, CLAUDE_SETTINGS_PATH)).text());
    const serialized = JSON.stringify(merged.hooks.PostToolUse);
    expect(serialized).toContain("bun user-hook.ts");
    expect(serialized).toContain(AUDIT_HOOK_COMMAND);
  });

  test("ISC-87: hook script run twice via Bun.spawn produces a verifiable 2-entry chain in the src/audit.ts format", async () => {
    await observabilityModule.apply(makeTestCtx(dir));
    expect(
      await runHook(dir, JSON.stringify({ tool_name: "Bash", tool_input: { command: "ls -la" } })),
    ).toBe(0);
    expect(
      await runHook(dir, JSON.stringify({ tool_name: "Read", tool_input: { file_path: "src/app.ts" } })),
    ).toBe(0);

    const entries = await readLog(dir);
    const verdict = verifyChain(entries);
    expect(verdict.valid).toBe(true);
    expect(verdict.length).toBe(2);

    expect(entries[0]!.prev).toBe(AUDIT_GENESIS);
    expect(entries[0]!.actor).toBe("harness-hook");
    expect(entries[0]!.action).toBe("tool.Bash");
    expect(entries[0]!.result).toBe("observed");
    expect(entries[1]!.prev).toBe(entries[0]!.hash);
    expect(entries[1]!.action).toBe("tool.Read");
  });

  test("ISC-87: credential-shaped values in tool_input are redacted and target is capped at 120 chars", async () => {
    await observabilityModule.apply(makeTestCtx(dir));
    const planted = "ghp_abcdefghijklmnopqrstuvwxyz0123456789";
    await runHook(
      dir,
      JSON.stringify({
        tool_name: "Bash",
        tool_input: { command: `export TOKEN=${planted}`, note: "x".repeat(500) },
      }),
    );
    const raw = await Bun.file(join(dir, AUDIT_LOG_PATH)).text();
    expect(raw).not.toContain(planted);
    const entries = await readLog(dir);
    expect(entries[0]!.target).toContain("[redacted]");
    expect(entries[0]!.target.length).toBeLessThanOrEqual(120);
    // Redaction must not break the chain.
    expect(verifyChain(entries).valid).toBe(true);
  });

  test("ISC-87: malformed stdin exits 0 and appends nothing (hook never blocks the harness)", async () => {
    await observabilityModule.apply(makeTestCtx(dir));
    expect(await runHook(dir, "this is not json")).toBe(0);
    expect(await Bun.file(join(dir, AUDIT_LOG_PATH)).exists()).toBe(false);
  });

  test("ISC-87: shipped hook script is self-contained — no repo-relative imports", () => {
    const script = auditHookScript();
    expect(script).not.toContain("../");
    expect(script).not.toContain("./src");
    expect(script).toContain("Bun.CryptoHasher");
    expect(script).toContain(AUDIT_GENESIS);
  });

  test("ISC-88: audit dir git-ignored by default", async () => {
    await Bun.write(join(dir, ".gitignore"), "node_modules/\n");
    await observabilityModule.apply(makeTestCtx(dir));
    const gitignore = await Bun.file(join(dir, ".gitignore")).text();
    expect(gitignore).toContain("node_modules/");
    expect(gitignore.split("\n").map((line) => line.trim())).toContain(".ade/audit/");
  });

  test("ISC-88: commitAuditLog=true skips the .gitignore entry and emits an info finding", async () => {
    const config = testConfig({
      modules: { observability: { enabled: true, options: { commitAuditLog: true } } },
    });
    const result = await observabilityModule.apply(makeTestCtx(dir, { config }));
    expect(result.status).toBe("applied");
    expect(
      result.findings.some((finding) => finding.level === "info" && finding.message.includes("commitAuditLog")),
    ).toBe(true);
    const gitignore = Bun.file(join(dir, ".gitignore"));
    if (await gitignore.exists()) {
      expect((await gitignore.text()).split("\n").map((line) => line.trim())).not.toContain(".ade/audit/");
    }
  });

  test("ISC-116: plan writes nothing to disk", async () => {
    const ctx = makeTestCtx(dir);
    const actions = await observabilityModule.plan(ctx);
    expect(actions.length).toBeGreaterThan(0);
    expect(actions.some((action) => action.kind === "hook")).toBe(true);
    expect(await Bun.file(join(dir, AUDIT_README_PATH)).exists()).toBe(false);
    expect(await Bun.file(join(dir, AUDIT_HOOK_PATH)).exists()).toBe(false);
    expect(await Bun.file(join(dir, CLAUDE_SETTINGS_PATH)).exists()).toBe(false);
  });

  test("ISC-117: apply is idempotent — second run byte-identical across all artifacts", async () => {
    await observabilityModule.apply(makeTestCtx(dir));
    const firstHashes = await Promise.all(
      [AUDIT_README_PATH, AUDIT_HOOK_PATH, CLAUDE_SETTINGS_PATH, ".gitignore"].map(async (rel) =>
        sha256(await Bun.file(join(dir, rel)).text()),
      ),
    );
    await observabilityModule.apply(makeTestCtx(dir));
    const secondHashes = await Promise.all(
      [AUDIT_README_PATH, AUDIT_HOOK_PATH, CLAUDE_SETTINGS_PATH, ".gitignore"].map(async (rel) =>
        sha256(await Bun.file(join(dir, rel)).text()),
      ),
    );
    expect(secondHashes).toEqual(firstHashes);
  });

  test("degraded path: unparseable user .claude/settings.json → degraded with remediation, file untouched", async () => {
    const broken = "{not valid json";
    await Bun.write(join(dir, CLAUDE_SETTINGS_PATH), broken);
    const result = await observabilityModule.apply(makeTestCtx(dir));
    expect(result.status).toBe("degraded");
    expect(result.findings.some((finding) => finding.remediation !== undefined && finding.level === "error")).toBe(true);
    expect(await Bun.file(join(dir, CLAUDE_SETTINGS_PATH)).text()).toBe(broken);
  });

  test("verify: passes after apply; fails when README, hook script, or wiring is removed", async () => {
    const ctx = makeTestCtx(dir);
    await observabilityModule.apply(ctx);
    expect((await observabilityModule.verify(ctx)).ok).toBe(true);

    await rm(join(dir, AUDIT_HOOK_PATH));
    expect((await observabilityModule.verify(ctx)).ok).toBe(false);
    await observabilityModule.apply(makeTestCtx(dir));

    await Bun.write(join(dir, CLAUDE_SETTINGS_PATH), "{}\n");
    expect((await observabilityModule.verify(ctx)).ok).toBe(false);
    await observabilityModule.apply(makeTestCtx(dir));

    await rm(join(dir, AUDIT_README_PATH));
    expect((await observabilityModule.verify(ctx)).ok).toBe(false);
  });

  test("verify: detects a tampered audit chain", async () => {
    const ctx = makeTestCtx(dir);
    await observabilityModule.apply(ctx);
    await runHook(dir, JSON.stringify({ tool_name: "Bash", tool_input: { command: "ls" } }));
    await runHook(dir, JSON.stringify({ tool_name: "Write", tool_input: { file_path: "a.ts" } }));
    expect((await observabilityModule.verify(ctx)).ok).toBe(true);

    // Tamper with a historical entry — every subsequent link must break.
    const entries = await readLog(dir);
    entries[0]!.target = "something-else-entirely";
    await Bun.write(
      join(dir, AUDIT_LOG_PATH),
      entries.map((entry) => `${JSON.stringify(entry)}\n`).join(""),
    );
    const verdict = await observabilityModule.verify(ctx);
    expect(verdict.ok).toBe(false);
    expect(verdict.findings.some((finding) => finding.message.includes("BROKEN"))).toBe(true);
  });

  test("instruction block declares the tamper-evident chain and forbids touching log.jsonl", () => {
    expect(observabilityModule.instructionBlocks.length).toBeGreaterThan(0);
    const block = observabilityModule.instructionBlocks[0]!;
    expect(block.content).toContain(".ade/audit/log.jsonl");
    expect(block.content).toContain("NEVER");
    expect(block.content.toLowerCase()).toContain("tamper-evident");
  });

  test("detect reports harness hook availability", async () => {
    const withClaude = await observabilityModule.detect(makeTestCtx(dir));
    expect(withClaude.some((finding) => finding.message.includes("claude-code"))).toBe(true);
    const without = await observabilityModule.detect(
      makeTestCtx(dir, { config: testConfig({ harnesses: ["codex"] }) }),
    );
    expect(without.some((finding) => finding.level === "info" && finding.remediation !== undefined)).toBe(true);
  });
});
