import { afterEach, beforeEach, describe, expect, test } from "bun:test";
import { join } from "node:path";
import { generateLockfile, loadLockfile, serializeLockfile, verifyAgainstLockfile, LOCKFILE_NAME } from "../src/lockfile.ts";
import { makeTempDir, makeTestCtx, removeDir } from "./helpers.ts";

let dir: string;

beforeEach(async () => {
  dir = await makeTempDir();
  await Bun.write(join(dir, ".ade", "policy", "a.json"), '{"a":1}\n');
  await Bun.write(join(dir, ".ade", "instructions.md"), "# canonical\n");
});

afterEach(async () => {
  await removeDir(dir);
});

const PATHS = [".ade/policy/a.json", ".ade/instructions.md"];

describe("lockfile", () => {
  test("ISC-35/36/40: records file hashes, environment tools, and ade version", async () => {
    const ctx = makeTestCtx(dir, { presentTools: { rtk: "rtk 0.9.0" } });
    const lock = await generateLockfile(ctx, PATHS);
    expect(Object.keys(lock.files).sort()).toEqual([...PATHS].sort());
    expect(lock.files[".ade/policy/a.json"]).toMatch(/^[0-9a-f]{64}$/);
    expect(lock.environment.tools["rtk"]).toBe("rtk 0.9.0");
    expect(lock.environment.tools["nono"]).toBeNull();
    expect(lock.adeVersion).toMatch(/^\d+\.\d+\.\d+$/);
  });

  test("ISC-37: serialization is byte-deterministic and timestamp-free", async () => {
    const ctx = makeTestCtx(dir);
    const first = serializeLockfile(await generateLockfile(ctx, PATHS));
    const second = serializeLockfile(await generateLockfile(makeTestCtx(dir), [...PATHS].reverse()));
    expect(first).toBe(second);
    expect(first).not.toMatch(/20\d{2}-\d{2}-\d{2}T/);
  });

  test("ISC-38: modified generated file fails verify, naming the file", async () => {
    const ctx = makeTestCtx(dir);
    const lock = await generateLockfile(ctx, PATHS);
    await Bun.write(join(dir, ".ade", "policy", "a.json"), '{"a":2}\n');
    const result = await verifyAgainstLockfile(ctx, lock);
    expect(result.ok).toBe(false);
    expect(result.findings.some((finding) => finding.level === "error" && finding.message.includes(".ade/policy/a.json"))).toBe(true);
  });

  test("ISC-39: deleted generated file fails verify, naming the file", async () => {
    const ctx = makeTestCtx(dir);
    const lock = await generateLockfile(ctx, PATHS);
    await removeDir(join(dir, ".ade", "policy"));
    const result = await verifyAgainstLockfile(ctx, lock);
    expect(result.ok).toBe(false);
    expect(result.findings.some((finding) => finding.message.includes("missing: .ade/policy/a.json"))).toBe(true);
  });

  test("ISC-109: tool version drift is informational, never a failure", async () => {
    const lockCtx = makeTestCtx(dir, { presentTools: { trufflehog: "3.0.0" } });
    const lock = await generateLockfile(lockCtx, PATHS);
    const nowCtx = makeTestCtx(dir, { presentTools: { trufflehog: "3.9.9" } });
    const result = await verifyAgainstLockfile(nowCtx, lock);
    expect(result.ok).toBe(true);
    expect(result.findings.some((finding) => finding.level === "info" && finding.message.includes("drift"))).toBe(true);
  });

  test("untampered state passes with an ok finding", async () => {
    const ctx = makeTestCtx(dir);
    const lock = await generateLockfile(ctx, PATHS);
    const result = await verifyAgainstLockfile(ctx, lock);
    expect(result.ok).toBe(true);
    expect(result.findings.some((finding) => finding.level === "ok")).toBe(true);
  });

  test("loadLockfile: absent and corrupt both return null", async () => {
    expect(await loadLockfile(dir)).toBeNull();
    await Bun.write(join(dir, LOCKFILE_NAME), "{broken");
    expect(await loadLockfile(dir)).toBeNull();
    const ctx = makeTestCtx(dir);
    await Bun.write(join(dir, LOCKFILE_NAME), serializeLockfile(await generateLockfile(ctx, PATHS)));
    expect(await loadLockfile(dir)).not.toBeNull();
  });
});
