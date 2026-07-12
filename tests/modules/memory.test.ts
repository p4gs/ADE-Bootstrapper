import { afterEach, beforeEach, describe, expect, test } from "bun:test";
import { join } from "node:path";
import {
  memoryModule,
  MEMORY_POLICY_PATH,
  MEMORY_STORE_PATH,
  MCP_SERVER_NAME,
} from "../../src/modules/memory.ts";
import { sha256 } from "../../src/fsutil.ts";
import { makeTempDir, makeTestCtx, removeDir, testConfig } from "../helpers.ts";
import type { Ctx } from "../../src/types.ts";

let dir: string;

beforeEach(async () => {
  dir = await makeTempDir();
});

afterEach(async () => {
  await removeDir(dir);
});

function ctxWithMcp(enableMcp: unknown, harnesses = ["claude-code", "codex"]): Ctx {
  return makeTestCtx(dir, {
    config: testConfig({
      harnesses,
      modules: enableMcp === undefined ? {} : { memory: { options: { enableMcp } } },
    }),
  });
}

async function readArtifact(rel: string): Promise<string | null> {
  const file = Bun.file(join(dir, rel));
  return (await file.exists()) ? await file.text() : null;
}

describe("memory module", () => {
  test("ISC-78: apply writes .ade/memory.json with openmemory provider, local-first posture, store path, and activation instructions", async () => {
    const result = await memoryModule.apply(ctxWithMcp(undefined));
    expect(result.status).toBe("applied");
    expect(result.wrotePaths).toContain(MEMORY_POLICY_PATH);
    const policy = JSON.parse((await readArtifact(MEMORY_POLICY_PATH))!) as Record<string, unknown>;
    expect(policy["provider"]).toBe("openmemory");
    expect(policy["project"]).toBe("mem0.ai/openmemory");
    expect(policy["posture"]).toBe("local-first");
    expect(policy["storePath"]).toBe(MEMORY_STORE_PATH);
    const activation = policy["activation"] as Record<string, string>;
    expect(activation["mcp"]).toContain("enableMcp");
    expect(activation["mcp"]).toContain("ade.json");
    expect(activation["mcp"]).toContain("ade apply");
  });

  test("ISC-79: enableMcp=true with claude-code harness registers the openmemory MCP server via npx", async () => {
    const result = await memoryModule.apply(ctxWithMcp(true));
    expect(result.status).toBe("applied");
    expect(result.wrotePaths).toContain(".mcp.json");
    const mcp = JSON.parse((await readArtifact(".mcp.json"))!) as {
      mcpServers: Record<string, { command: string; args: string[]; env: Record<string, string> }>;
    };
    const server = mcp.mcpServers[MCP_SERVER_NAME]!;
    expect(server.command).toBe("npx");
    expect(server.args).toEqual(["-y", "openmemory"]);
    expect(server.env).toEqual({});
  });

  test("ISC-79: default (enableMcp absent) → info finding with activation guidance, .mcp.json NOT touched", async () => {
    const result = await memoryModule.apply(ctxWithMcp(undefined));
    expect(result.status).toBe("applied");
    expect(await readArtifact(".mcp.json")).toBeNull();
    expect(result.wrotePaths).not.toContain(".mcp.json");
    const info = result.findings.find(
      (finding) => finding.level === "info" && finding.message.includes("NOT registered"),
    );
    expect(info).toBeDefined();
    expect(info!.remediation).toContain("enableMcp");
  });

  test("ISC-79: enableMcp=false → .mcp.json NOT touched", async () => {
    await memoryModule.apply(ctxWithMcp(false));
    expect(await readArtifact(".mcp.json")).toBeNull();
  });

  test("ISC-79: enableMcp=true WITHOUT claude-code harness → .mcp.json NOT touched", async () => {
    const result = await memoryModule.apply(ctxWithMcp(true, ["codex"]));
    expect(result.status).toBe("applied");
    expect(await readArtifact(".mcp.json")).toBeNull();
  });

  test("ISC-79: pre-existing user 'openmemory' entry in .mcp.json is preserved untouched", async () => {
    const userServer = { command: "/usr/local/bin/my-openmemory", args: ["--custom"], env: { PORT: "9999" } };
    await Bun.write(
      join(dir, ".mcp.json"),
      JSON.stringify({ mcpServers: { openmemory: userServer } }, null, 2),
    );
    const result = await memoryModule.apply(ctxWithMcp(true));
    expect(result.status).toBe("applied");
    const mcp = JSON.parse((await readArtifact(".mcp.json"))!) as {
      mcpServers: Record<string, unknown>;
    };
    expect(mcp.mcpServers[MCP_SERVER_NAME]).toEqual(userServer);
    expect(
      result.findings.some((finding) => finding.level === "info" && finding.message.includes("preserved")),
    ).toBe(true);
  });

  test("ISC-79: invalid user .mcp.json → degraded with fix remediation, file not clobbered", async () => {
    const broken = "{ not json !!!";
    await Bun.write(join(dir, ".mcp.json"), broken);
    const result = await memoryModule.apply(ctxWithMcp(true));
    expect(result.status).toBe("degraded");
    expect(result.findings.some((finding) => finding.level === "error" && finding.message.includes(".mcp.json"))).toBe(true);
    expect(await readArtifact(".mcp.json")).toBe(broken);
  });

  test("ISC-80: apply git-ignores the memory store, idempotently, preserving user content", async () => {
    await Bun.write(join(dir, ".gitignore"), "node_modules/\n");
    await memoryModule.apply(ctxWithMcp(undefined));
    const first = (await readArtifact(".gitignore"))!;
    expect(first).toContain("node_modules/");
    expect(first.split("\n").map((line) => line.trim())).toContain(MEMORY_STORE_PATH);
    await memoryModule.apply(ctxWithMcp(undefined));
    expect(await readArtifact(".gitignore")).toBe(first);
  });

  test("ISC-80: policy declares memory as sensitive and never committed", async () => {
    await memoryModule.apply(ctxWithMcp(undefined));
    const policy = JSON.parse((await readArtifact(MEMORY_POLICY_PATH))!) as {
      sensitivity: { rule: string; gitIgnored: boolean };
    };
    expect(policy.sensitivity.gitIgnored).toBe(true);
    expect(policy.sensitivity.rule).toContain("never committed");
  });

  test("instruction block marks memory sensitive and forbids secrets in memory", () => {
    const block = memoryModule.instructionBlocks[0]!;
    expect(block.content).toContain("OpenMemory MCP server");
    expect(block.content.toLowerCase()).toContain("never write secrets");
    expect(block.content.toLowerCase()).toContain("sensitive user data");
  });

  test("plan writes nothing to disk", async () => {
    const ctx = ctxWithMcp(true);
    const actions = await memoryModule.plan(ctx);
    expect(actions.some((action) => action.path === MEMORY_POLICY_PATH)).toBe(true);
    expect(actions.some((action) => action.kind === "merge" && action.path === ".mcp.json")).toBe(true);
    expect(await readArtifact(MEMORY_POLICY_PATH)).toBeNull();
    expect(await readArtifact(".mcp.json")).toBeNull();
    expect(await readArtifact(".gitignore")).toBeNull();
  });

  test("plan without enableMcp reports MCP registration as skipped info, no .mcp.json action", async () => {
    const actions = await memoryModule.plan(ctxWithMcp(undefined));
    expect(actions.some((action) => action.path === ".mcp.json")).toBe(false);
    expect(actions.some((action) => action.kind === "info" && action.description.includes("opt-in"))).toBe(true);
  });

  test("double apply is byte-identical (memory.json, .mcp.json, .gitignore)", async () => {
    await memoryModule.apply(ctxWithMcp(true));
    const hashes = async (): Promise<string[]> => {
      const out: string[] = [];
      for (const rel of [MEMORY_POLICY_PATH, ".mcp.json", ".gitignore"]) {
        out.push(sha256((await readArtifact(rel))!));
      }
      return out;
    };
    const first = await hashes();
    await memoryModule.apply(ctxWithMcp(true));
    expect(await hashes()).toEqual(first);
  });

  test("verify passes after apply", async () => {
    const ctx = ctxWithMcp(undefined);
    await memoryModule.apply(ctx);
    const result = await memoryModule.verify(ctx);
    expect(result.ok).toBe(true);
  });

  test("verify fails when memory.json is tampered into invalid JSON", async () => {
    const ctx = ctxWithMcp(undefined);
    await memoryModule.apply(ctx);
    await Bun.write(join(dir, MEMORY_POLICY_PATH), "{ tampered");
    const result = await memoryModule.verify(ctx);
    expect(result.ok).toBe(false);
    expect(result.findings.some((finding) => finding.level === "error")).toBe(true);
  });

  test("verify fails when the policy no longer declares the local-first openmemory contract", async () => {
    const ctx = ctxWithMcp(undefined);
    await memoryModule.apply(ctx);
    await Bun.write(
      join(dir, MEMORY_POLICY_PATH),
      JSON.stringify({ provider: "other", posture: "cloud", storePath: "/elsewhere" }),
    );
    const result = await memoryModule.verify(ctx);
    expect(result.ok).toBe(false);
    expect(result.findings.some((finding) => finding.message.includes("local-first"))).toBe(true);
  });

  test("verify fails when .gitignore no longer covers the memory store", async () => {
    const ctx = ctxWithMcp(undefined);
    await memoryModule.apply(ctx);
    await Bun.write(join(dir, ".gitignore"), "node_modules/\n");
    const result = await memoryModule.verify(ctx);
    expect(result.ok).toBe(false);
    expect(
      result.findings.some((finding) => finding.level === "error" && finding.message.includes(MEMORY_STORE_PATH)),
    ).toBe(true);
  });

  test("detect reports opt-in status: off by default, on when enabled", async () => {
    const off = await memoryModule.detect(ctxWithMcp(undefined));
    expect(off.some((finding) => finding.level === "info" && finding.remediation?.includes("enableMcp"))).toBe(true);
    const on = await memoryModule.detect(ctxWithMcp(true));
    expect(on.some((finding) => finding.level === "ok" && finding.message.includes("enabled"))).toBe(true);
  });

  test("no environment variable values leak into generated artifacts", async () => {
    const planted = "PLANTED_MEMORY_TEST_SECRET_98765";
    const ctx = makeTestCtx(dir, {
      config: testConfig({ modules: { memory: { options: { enableMcp: true } } } }),
      env: { OPENMEMORY_API_KEY: planted },
    });
    await memoryModule.apply(ctx);
    for (const rel of [MEMORY_POLICY_PATH, ".mcp.json", ".gitignore"]) {
      const text = await readArtifact(rel);
      if (text !== null) expect(text).not.toContain(planted);
    }
  });
});
