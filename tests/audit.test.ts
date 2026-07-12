import { afterEach, beforeEach, describe, expect, test } from "bun:test";
import { join } from "node:path";
import { appendEvents, computeEntry, parseLog, verifyChain, type AuditEventInput } from "../src/audit.ts";
import { AUDIT_GENESIS } from "../src/version.ts";
import { makeTempDir, removeDir } from "./helpers.ts";

let dir: string;

beforeEach(async () => {
  dir = await makeTempDir();
});

afterEach(async () => {
  await removeDir(dir);
});

function event(action: string): AuditEventInput {
  return { ts: "2026-07-12T00:00:00Z", actor: "ade", action, target: "test", result: "ok" };
}

describe("tamper-evident audit chain", () => {
  test("ISC-42: entries chain from the fixed genesis value", () => {
    const first = computeEntry(event("one"), AUDIT_GENESIS);
    expect(first.prev).toBe(AUDIT_GENESIS);
    expect(first.hash).toMatch(/^[0-9a-f]{64}$/);
    const second = computeEntry(event("two"), first.hash);
    expect(second.prev).toBe(first.hash);
  });

  test("ISC-41/43: appendEvents builds a valid chain on disk", async () => {
    const logPath = join(dir, "log.jsonl");
    await appendEvents(logPath, [event("a"), event("b")]);
    await appendEvents(logPath, [event("c")]);
    const entries = parseLog(await Bun.file(logPath).text());
    expect(entries.length).toBe(3);
    const verdict = verifyChain(entries);
    expect(verdict.valid).toBe(true);
    expect(verdict.length).toBe(3);
  });

  test("ISC-44: modifying a historical entry breaks the chain at that index", async () => {
    const logPath = join(dir, "log.jsonl");
    await appendEvents(logPath, [event("a"), event("b"), event("c")]);
    const entries = parseLog(await Bun.file(logPath).text());
    entries[1]!.result = "tampered";
    const verdict = verifyChain(entries);
    expect(verdict.valid).toBe(false);
    expect(verdict.brokenIndex).toBe(1);
    expect(verdict.reason).toBe("hash-mismatch");
  });

  test("ISC-45: deleting a mid-chain entry breaks the chain", async () => {
    const logPath = join(dir, "log.jsonl");
    await appendEvents(logPath, [event("a"), event("b"), event("c")]);
    const entries = parseLog(await Bun.file(logPath).text());
    entries.splice(1, 1);
    const verdict = verifyChain(entries);
    expect(verdict.valid).toBe(false);
    expect(verdict.brokenIndex).toBe(1);
    expect(verdict.reason).toBe("prev-mismatch");
  });

  test("empty log verifies as valid genesis state", () => {
    expect(verifyChain([]).valid).toBe(true);
    expect(parseLog("")).toEqual([]);
  });

  test("forged prev pointer is caught even with recomputed hash shape", async () => {
    const logPath = join(dir, "log.jsonl");
    await appendEvents(logPath, [event("a"), event("b")]);
    const entries = parseLog(await Bun.file(logPath).text());
    entries[1]!.prev = AUDIT_GENESIS;
    const verdict = verifyChain(entries);
    expect(verdict.valid).toBe(false);
  });
});
