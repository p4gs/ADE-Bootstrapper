/**
 * Translate the canonical instructions into every target harness's
 * instruction file, inside managed markers. User content outside the
 * markers is preserved byte-for-byte; ambiguous or hand-edited blocks
 * are refused, never clobbered.
 */
import { lstat } from "node:fs/promises";
import { join } from "node:path";
import { getAdapter, renderTarget } from "./harness/adapters.ts";
import { extractManagedBlock, renderManagedBlock, upsertManagedBlock } from "./managed.ts";
import { readIfExists } from "./fsutil.ts";
import type { Ctx, Finding } from "./types.ts";

export interface TranslateFileResult {
  path: string;
  harnesses: string[];
  ok: boolean;
  changed: boolean;
  error?: string;
}

/** Unique instruction-file targets for the configured harnesses (deduped by path). */
export function translationTargets(harnessIds: string[]): Array<{ path: string; freshFilePrefix: string; harnesses: string[] }> {
  const byPath = new Map<string, { path: string; freshFilePrefix: string; harnesses: string[] }>();
  for (const id of harnessIds) {
    const adapter = getAdapter(id);
    if (adapter === undefined) continue;
    const target = renderTarget(adapter);
    const existing = byPath.get(target.path);
    if (existing === undefined) {
      byPath.set(target.path, { ...target, harnesses: [id] });
    } else {
      existing.harnesses.push(id);
    }
  }
  return [...byPath.values()].sort((a, b) => a.path.localeCompare(b.path));
}

/**
 * Guard: only ever write over a regular file (or nothing). FIFOs, sockets,
 * device nodes, and symlinks in config paths are frequently intentional
 * secret mounts (1Password/sops) — clobbering one destroys the mount.
 */
async function isSafeWriteTarget(absolute: string): Promise<boolean> {
  try {
    const stats = await lstat(absolute);
    return stats.isFile();
  } catch {
    return true; // absent → safe to create
  }
}

export async function translateAll(ctx: Ctx, canonicalBody: string): Promise<TranslateFileResult[]> {
  const results: TranslateFileResult[] = [];
  for (const target of translationTargets(ctx.config.harnesses)) {
    const absolute = join(ctx.targetDir, target.path);
    if (!(await isSafeWriteTarget(absolute))) {
      results.push({
        path: target.path,
        harnesses: target.harnesses,
        ok: false,
        changed: false,
        error: "target exists but is not a regular file (symlink/FIFO/socket) — refusing to write; it may be an intentional mount",
      });
      continue;
    }
    const existing = await readIfExists(absolute);
    const base = existing ?? target.freshFilePrefix;
    const upsert = upsertManagedBlock(base, canonicalBody);
    if (!upsert.ok) {
      results.push({ path: target.path, harnesses: target.harnesses, ok: false, changed: false, error: upsert.error });
      continue;
    }
    if (upsert.changed) {
      await ctx.artifacts.write(target.path, upsert.content);
    } else {
      // Unchanged content still belongs to the generated set (lockfile coverage).
      await ctx.artifacts.write(target.path, upsert.content);
    }
    results.push({ path: target.path, harnesses: target.harnesses, ok: true, changed: upsert.changed });
  }
  return results;
}

/** Drift check: every target file's managed block matches the canonical rendering. */
export async function checkTranslationDrift(ctx: Ctx, canonicalBody: string): Promise<Finding[]> {
  const findings: Finding[] = [];
  const expected = renderManagedBlock(canonicalBody);
  for (const target of translationTargets(ctx.config.harnesses)) {
    const existing = await readIfExists(join(ctx.targetDir, target.path));
    if (existing === null) {
      findings.push({
        level: "error",
        message: `instruction file missing: ${target.path}`,
        remediation: "run `ade translate`",
      });
      continue;
    }
    const block = extractManagedBlock(existing);
    if (block === null) {
      findings.push({
        level: "error",
        message: `managed block missing or corrupt in ${target.path}`,
        remediation: "run `ade translate` (or fix the markers manually)",
      });
      continue;
    }
    if (block !== expected) {
      findings.push({
        level: "error",
        message: `instruction drift in ${target.path}: managed block does not match .ade/instructions.md`,
        remediation: "run `ade translate` to regenerate",
      });
    }
  }
  if (findings.length === 0) {
    findings.push({ level: "ok", message: "instruction files match canonical source" });
  }
  return findings;
}
