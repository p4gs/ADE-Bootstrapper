import { afterEach, beforeEach, describe, expect, test } from "bun:test";
import { join } from "node:path";
import { deepMerge, ensureLines, makeArtifactWriter, readIfExists, sha256, stableStringify, writeEnsured } from "../src/fsutil.ts";
import { makeTempDir, removeDir } from "./helpers.ts";

let dir: string;

beforeEach(async () => {
  dir = await makeTempDir();
});

afterEach(async () => {
  await removeDir(dir);
});

describe("stableStringify", () => {
  test("ISC-34/37: key order is canonical regardless of insertion order", () => {
    const a = stableStringify({ b: 1, a: { d: 2, c: 3 }, list: [{ z: 1, y: 2 }] });
    const b = stableStringify({ list: [{ y: 2, z: 1 }], a: { c: 3, d: 2 }, b: 1 });
    expect(a).toBe(b);
    expect(a.endsWith("\n")).toBe(true);
  });

  test("sha256 is stable and hex", () => {
    expect(sha256("ade")).toBe(sha256("ade"));
    expect(sha256("ade")).toMatch(/^[0-9a-f]{64}$/);
    expect(sha256("ade")).not.toBe(sha256("ade2"));
  });
});

describe("writeEnsured / readIfExists / artifact writer", () => {
  test("creates parent dirs and reports change status", async () => {
    const path = join(dir, "deep", "nested", "file.txt");
    expect(await writeEnsured(path, "one")).toBe(true);
    expect(await writeEnsured(path, "one")).toBe(false);
    expect(await writeEnsured(path, "two")).toBe(true);
    expect(await readIfExists(path)).toBe("two");
    expect(await readIfExists(join(dir, "absent"))).toBeNull();
  });

  test("artifact writer records sorted repo-relative paths", async () => {
    const writer = makeArtifactWriter(dir);
    await writer.write("b/two.json", "{}");
    await writer.write("a/one.json", "{}");
    await writer.write("b/two.json", "{}");
    expect(writer.written()).toEqual(["a/one.json", "b/two.json"]);
    expect(await readIfExists(join(dir, "a", "one.json"))).toBe("{}");
  });
});

describe("ensureLines", () => {
  test("ISC-96: appends only missing lines, idempotently, preserving user content", async () => {
    const path = join(dir, ".gitignore");
    await Bun.write(path, "node_modules/\n.env\n");
    expect(await ensureLines(path, [".env", "*.pem"], "test label")).toBe(true);
    const first = await readIfExists(path);
    expect(first).toContain("node_modules/");
    expect(first).toContain("*.pem");
    expect(first!.match(/\.env$/gm)!.length).toBe(1);
    expect(await ensureLines(path, [".env", "*.pem"], "test label")).toBe(false);
    expect(await readIfExists(path)).toBe(first);
  });

  test("creates the file when absent", async () => {
    const path = join(dir, "fresh", ".gitignore");
    expect(await ensureLines(path, ["a", "b"], "label")).toBe(true);
    expect(await readIfExists(path)).toContain("a\nb\n");
  });
});

describe("deepMerge", () => {
  test("objects merge recursively, scalars from patch win", () => {
    const merged = deepMerge({ a: { b: 1, keep: true } }, { a: { b: 2 } });
    expect(merged).toEqual({ a: { b: 2, keep: true } });
  });

  test("ISC-50/51: arrays union with dedupe — user entries preserved", () => {
    const merged = deepMerge(
      { permissions: { deny: ["UserRule(1)"] } },
      { permissions: { deny: ["UserRule(1)", "AdeRule(2)"] } },
    );
    expect((merged as { permissions: { deny: string[] } }).permissions.deny).toEqual([
      "UserRule(1)",
      "AdeRule(2)",
    ]);
  });

  test("object-array entries dedupe structurally", () => {
    const entry = { matcher: "*", hooks: [{ type: "command", command: "x" }] };
    const merged = deepMerge({ hooks: { PostToolUse: [entry] } }, { hooks: { PostToolUse: [{ ...entry }] } });
    expect((merged as { hooks: { PostToolUse: unknown[] } }).hooks.PostToolUse.length).toBe(1);
  });
});
