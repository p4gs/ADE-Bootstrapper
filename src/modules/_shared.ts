/**
 * Shared helpers for ADE modules. Every module writes policy artifacts through
 * these so outputs stay deterministic and lockfile-recorded.
 */
import { join } from "node:path";
import { readIfExists, stableStringify } from "../fsutil.ts";
import type { Ctx, Finding } from "../types.ts";

/** Write a JSON policy artifact deterministically via the recorded writer. */
export async function writePolicy(ctx: Ctx, relPath: string, policy: unknown): Promise<void> {
  await ctx.artifacts.write(relPath, stableStringify(policy));
}

/** Read + parse a JSON artifact from the target repo; null when absent/invalid. */
export async function readJson(ctx: Ctx, relPath: string): Promise<unknown | null> {
  const text = await readIfExists(join(ctx.targetDir, relPath));
  if (text === null) return null;
  try {
    return JSON.parse(text);
  } catch {
    return null;
  }
}

/** Standard finding for an integrated tool's presence/absence. */
export function toolFinding(ctx: Ctx, tool: string, whenAbsent: string): Finding {
  const info = ctx.tools[tool];
  if (info?.present) {
    return {
      level: "ok",
      message: `${tool} present${info.version !== undefined ? ` (${info.version})` : ""}`,
    };
  }
  return { level: "degraded", message: `${tool} not installed`, remediation: whenAbsent };
}

/** Verify helper: artifact exists and parses as JSON. */
export async function verifyJsonArtifact(ctx: Ctx, relPath: string): Promise<Finding> {
  const parsed = await readJson(ctx, relPath);
  if (parsed === null) {
    return {
      level: "error",
      message: `${relPath} missing or invalid JSON`,
      remediation: "run `ade apply`",
    };
  }
  return { level: "ok", message: `${relPath} present and valid` };
}
