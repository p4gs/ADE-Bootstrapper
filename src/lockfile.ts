/**
 * Deterministic lockfile — `ade.lock.json`.
 *
 * Content-addressed record of everything ADE generated plus environment
 * facts (tool versions). No timestamps: identical inputs → byte-identical
 * lockfile. Environment differences on another machine are DRIFT verdicts
 * (informational), never hard failures; only content tampering fails verify.
 */
import { join } from "node:path";
import { readIfExists, sha256, stableStringify } from "./fsutil.ts";
import { ADE_SCHEMA_VERSION, ADE_VERSION } from "./version.ts";
import type { Ctx, Finding } from "./types.ts";

export const LOCKFILE_NAME = "ade.lock.json";

export interface Lockfile {
  schemaVersion: number;
  adeVersion: string;
  /** Repo-relative path (POSIX separators) → sha256 of file content. */
  files: Record<string, string>;
  environment: {
    os: string;
    arch: string;
    /** Tool name → version string, or null when the tool is absent. */
    tools: Record<string, string | null>;
  };
  harnesses: string[];
}

export async function generateLockfile(ctx: Ctx, generatedPaths: string[]): Promise<Lockfile> {
  const files: Record<string, string> = {};
  for (const relPath of [...generatedPaths].sort()) {
    const content = await readIfExists(join(ctx.targetDir, relPath));
    if (content !== null) {
      files[relPath] = sha256(content);
    }
  }
  const tools: Record<string, string | null> = {};
  for (const info of Object.values(ctx.tools)) {
    tools[info.name] = info.present ? (info.version ?? "present") : null;
  }
  return {
    schemaVersion: ADE_SCHEMA_VERSION,
    adeVersion: ADE_VERSION,
    files,
    environment: { os: ctx.os, arch: ctx.arch, tools },
    harnesses: [...ctx.config.harnesses].sort(),
  };
}

export function serializeLockfile(lock: Lockfile): string {
  return stableStringify(lock);
}

export async function loadLockfile(targetDir: string): Promise<Lockfile | null> {
  const text = await readIfExists(join(targetDir, LOCKFILE_NAME));
  if (text === null) return null;
  try {
    return JSON.parse(text) as Lockfile;
  } catch {
    return null;
  }
}

export interface LockVerifyResult {
  ok: boolean;
  findings: Finding[];
}

/**
 * Verify on-disk state against the lockfile.
 * - missing/modified generated file → error (fails verify, names the file)
 * - tool version drift vs recorded environment → info (never fails)
 */
export async function verifyAgainstLockfile(ctx: Ctx, lock: Lockfile): Promise<LockVerifyResult> {
  const findings: Finding[] = [];
  let ok = true;
  for (const [relPath, expectedHash] of Object.entries(lock.files)) {
    const content = await readIfExists(join(ctx.targetDir, relPath));
    if (content === null) {
      ok = false;
      findings.push({
        level: "error",
        message: `generated file missing: ${relPath}`,
        remediation: "run `ade apply` to regenerate, or `ade lock` to accept the removal",
      });
      continue;
    }
    if (sha256(content) !== expectedHash) {
      ok = false;
      findings.push({
        level: "error",
        message: `generated file modified since lock: ${relPath}`,
        remediation: "run `ade apply` to regenerate, or `ade lock` to accept the change",
      });
    }
  }
  for (const [tool, lockedVersion] of Object.entries(lock.environment.tools)) {
    const current = ctx.tools[tool];
    const currentVersion = current?.present ? (current.version ?? "present") : null;
    if (currentVersion !== lockedVersion) {
      findings.push({
        level: "info",
        message: `environment drift: ${tool} was ${lockedVersion ?? "absent"} at lock time, now ${currentVersion ?? "absent"}`,
      });
    }
  }
  if (ok && findings.length === 0) {
    findings.push({ level: "ok", message: "lockfile verification passed" });
  }
  return { ok, findings };
}
