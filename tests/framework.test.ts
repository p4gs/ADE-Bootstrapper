/**
 * Framework invariant tests (ISC-114..120) — run against the FULL registry,
 * so every module (including fan-out authored ones) must satisfy them.
 * This is the conformance suite: module-specific tests cannot substitute for it.
 */
import { afterEach, beforeEach, describe, expect, test } from "bun:test";
import { mkdir } from "node:fs/promises";
import { join } from "node:path";
import { MODULES, getModule, moduleIds } from "../src/registry.ts";
import { sha256 } from "../src/fsutil.ts";
import { ALL_MODULE_IDS, makeTempDir, makeTestCtx, removeDir } from "./helpers.ts";

let dir: string;

beforeEach(async () => {
  dir = await makeTempDir();
  await mkdir(join(dir, ".git", "hooks"), { recursive: true });
  await Bun.write(join(dir, "package.json"), '{"name":"fixture"}\n');
});

afterEach(async () => {
  await removeDir(dir);
});

async function treeSnapshot(root: string): Promise<string> {
  const glob = new Bun.Glob("**/*");
  const entries: string[] = [];
  for await (const path of glob.scan({ cwd: root, onlyFiles: true, dot: true })) {
    const content = await Bun.file(join(root, path)).text().catch(() => "<binary>");
    entries.push(`${path}:${sha256(content)}`);
  }
  return entries.sort().join("\n");
}

describe("module framework invariants", () => {
  test("ISC-115: registry holds exactly the 15 spec modules", () => {
    expect(MODULES.length).toBe(15);
    expect(moduleIds().sort()).toEqual([...ALL_MODULE_IDS].sort());
    expect(getModule("secrets")?.id).toBe("secrets");
    expect(getModule("nonexistent")).toBeUndefined();
  });

  test("ISC-114: every module implements the full interface with correct identity fields", () => {
    for (const module of MODULES) {
      expect(typeof module.id).toBe("string");
      expect(module.title.length).toBeGreaterThan(0);
      expect(["security", "governance", "context", "efficiency", "reproducibility"]).toContain(module.category);
      expect(module.spec.length).toBeGreaterThan(0);
      expect(typeof module.defaultEnabled).toBe("boolean");
      expect(Array.isArray(module.instructionBlocks)).toBe(true);
      expect(typeof module.detect).toBe("function");
      expect(typeof module.plan).toBe("function");
      expect(typeof module.apply).toBe("function");
      expect(typeof module.verify).toBe("function");
    }
  });

  test("ISC-116: plan() never writes to disk — any module (tree-hash proof)", async () => {
    const before = await treeSnapshot(dir);
    for (const module of MODULES) {
      await module.plan(makeTestCtx(dir));
    }
    expect(await treeSnapshot(dir)).toBe(before);
  });

  test("ISC-117: apply() is idempotent for every module (double-apply tree-identical)", async () => {
    for (const module of MODULES) {
      await module.apply(makeTestCtx(dir));
    }
    const afterFirst = await treeSnapshot(dir);
    for (const module of MODULES) {
      await module.apply(makeTestCtx(dir));
    }
    expect(await treeSnapshot(dir)).toBe(afterFirst);
  });

  test("ISC-118: verify() returns structured findings for every module post-apply", async () => {
    for (const module of MODULES) {
      await module.apply(makeTestCtx(dir));
    }
    for (const module of MODULES) {
      const verdict = await module.verify(makeTestCtx(dir));
      expect(typeof verdict.ok).toBe("boolean");
      expect(Array.isArray(verdict.findings)).toBe(true);
      expect(verdict.findings.length).toBeGreaterThan(0);
      for (const finding of verdict.findings) {
        expect(["ok", "info", "warn", "degraded", "error"]).toContain(finding.level);
        expect(finding.message.length).toBeGreaterThan(0);
      }
    }
  });

  test("ISC-120: with EVERY optional tool absent, no module errors or throws — degraded at worst", async () => {
    for (const module of MODULES) {
      const result = await module.apply(makeTestCtx(dir));
      expect(["applied", "skipped", "degraded"]).toContain(result.status);
    }
  });

  test("ISC-138: every module names its spec component", () => {
    const specs = new Set(MODULES.map((module) => module.spec));
    expect(specs.size).toBe(15);
  });

  test("instruction blocks are static and deterministic across reads", () => {
    for (const module of MODULES) {
      const first = JSON.stringify(module.instructionBlocks);
      const second = JSON.stringify(module.instructionBlocks);
      expect(second).toBe(first);
    }
  });
});
