/**
 * Tamper-evident audit log — hash-chained JSONL at `.ade/audit/log.jsonl`.
 *
 * Each entry embeds `hash = sha256(prev + canonical(entry-without-hash))`,
 * anchored at a fixed genesis value. Any modification or deletion of a
 * historical entry breaks every subsequent link.
 */
import { mkdir } from "node:fs/promises";
import { dirname } from "node:path";
import { readIfExists, sha256 } from "./fsutil.ts";
import { AUDIT_GENESIS } from "./version.ts";

export interface AuditEventInput {
  ts: string;
  actor: string;
  action: string;
  target: string;
  result: string;
}

export interface AuditEntry extends AuditEventInput {
  prev: string;
  hash: string;
}

/** Canonical serialization of the hashed portion of an entry (fixed key order). */
function canonical(entry: AuditEventInput, prev: string): string {
  return JSON.stringify({
    ts: entry.ts,
    actor: entry.actor,
    action: entry.action,
    target: entry.target,
    result: entry.result,
    prev,
  });
}

export function computeEntry(input: AuditEventInput, prev: string): AuditEntry {
  return { ...input, prev, hash: sha256(prev + canonical(input, prev)) };
}

export function parseLog(text: string): AuditEntry[] {
  return text
    .split("\n")
    .filter((line) => line.trim().length > 0)
    .map((line) => JSON.parse(line) as AuditEntry);
}

export interface ChainVerdict {
  valid: boolean;
  length: number;
  brokenIndex?: number;
  reason?: string;
}

/** Verify the whole chain from genesis. */
export function verifyChain(entries: AuditEntry[]): ChainVerdict {
  let prev = AUDIT_GENESIS;
  for (let index = 0; index < entries.length; index += 1) {
    const entry = entries[index]!;
    if (entry.prev !== prev) {
      return { valid: false, length: entries.length, brokenIndex: index, reason: "prev-mismatch" };
    }
    const expected = sha256(prev + canonical(entry, prev));
    if (entry.hash !== expected) {
      return { valid: false, length: entries.length, brokenIndex: index, reason: "hash-mismatch" };
    }
    prev = entry.hash;
  }
  return { valid: true, length: entries.length };
}

/** Append events to the log file, extending the chain. Creates the file with a genesis event when absent. */
export async function appendEvents(
  logPath: string,
  events: AuditEventInput[],
): Promise<AuditEntry[]> {
  const existingText = (await readIfExists(logPath)) ?? "";
  const existing = parseLog(existingText);
  let prev = existing.length > 0 ? existing[existing.length - 1]!.hash : AUDIT_GENESIS;
  const appended: AuditEntry[] = [];
  for (const event of events) {
    const entry = computeEntry(event, prev);
    appended.push(entry);
    prev = entry.hash;
  }
  const lines = appended.map((entry) => `${JSON.stringify(entry)}\n`).join("");
  await mkdir(dirname(logPath), { recursive: true });
  await Bun.write(logPath, existingText + lines);
  return appended;
}
