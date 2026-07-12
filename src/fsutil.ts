/**
 * Deterministic filesystem + serialization helpers.
 * Everything the bootstrapper generates flows through these so that
 * identical inputs produce byte-identical outputs (design goal: determinism).
 */
import { mkdir } from "node:fs/promises";
import { dirname, join } from "node:path";
import type { ArtifactWriter } from "./types.ts";

/** JSON.stringify with recursively sorted object keys + trailing newline. */
export function stableStringify(value: unknown): string {
  return `${JSON.stringify(sortValue(value), null, 2)}\n`;
}

function sortValue(value: unknown): unknown {
  if (Array.isArray(value)) return value.map(sortValue);
  if (value !== null && typeof value === "object") {
    const out: Record<string, unknown> = {};
    for (const key of Object.keys(value as Record<string, unknown>).sort()) {
      out[key] = sortValue((value as Record<string, unknown>)[key]);
    }
    return out;
  }
  return value;
}

/** sha256 hex digest of a string. */
export function sha256(text: string): string {
  const hasher = new Bun.CryptoHasher("sha256");
  hasher.update(text);
  return hasher.digest("hex");
}

/** Read a file; null when absent. */
export async function readIfExists(path: string): Promise<string | null> {
  const file = Bun.file(path);
  if (!(await file.exists())) return null;
  return await file.text();
}

/** Write a file, creating parent directories. Returns true when content changed. */
export async function writeEnsured(path: string, content: string): Promise<boolean> {
  const existing = await readIfExists(path);
  if (existing === content) return false;
  await mkdir(dirname(path), { recursive: true });
  await Bun.write(path, content);
  return true;
}

/** ArtifactWriter rooted at a target repo — records everything written through it. */
export function makeArtifactWriter(targetDir: string): ArtifactWriter {
  const paths = new Set<string>();
  return {
    async write(relPath: string, content: string): Promise<boolean> {
      const changed = await writeEnsured(join(targetDir, relPath), content);
      paths.add(relPath);
      return changed;
    },
    written(): string[] {
      return [...paths].sort();
    },
  };
}

/**
 * Idempotently ensure lines exist in a line-oriented file (e.g. .gitignore).
 * Appends only the missing lines under a labelled comment; never reorders
 * or removes user content. Returns true when the file changed.
 */
export async function ensureLines(
  path: string,
  lines: string[],
  label: string,
): Promise<boolean> {
  const existing = (await readIfExists(path)) ?? "";
  const present = new Set(
    existing.split("\n").map((line) => line.trim()),
  );
  const missing = lines.filter((line) => !present.has(line.trim()));
  if (missing.length === 0) return false;
  const prefix = existing.length > 0 && !existing.endsWith("\n") ? "\n" : "";
  const block = `${prefix}# ${label}\n${missing.join("\n")}\n`;
  await mkdir(dirname(path), { recursive: true });
  await Bun.write(path, existing + block);
  return true;
}

/**
 * Deep-merge `patch` into `base` for ADE-owned JSON surfaces.
 * Objects merge recursively; arrays union with deduplication (user entries
 * are preserved, ADE entries appended); scalars from patch win.
 */
export function deepMerge(
  base: Record<string, unknown>,
  patch: Record<string, unknown>,
): Record<string, unknown> {
  const out: Record<string, unknown> = { ...base };
  for (const [key, patchValue] of Object.entries(patch)) {
    const baseValue = out[key];
    if (Array.isArray(baseValue) && Array.isArray(patchValue)) {
      const seen = new Set(baseValue.map((entry) => JSON.stringify(sortValue(entry))));
      const merged = [...baseValue];
      for (const entry of patchValue) {
        const sig = JSON.stringify(sortValue(entry));
        if (!seen.has(sig)) {
          seen.add(sig);
          merged.push(entry);
        }
      }
      out[key] = merged;
    } else if (
      baseValue !== null &&
      typeof baseValue === "object" &&
      !Array.isArray(baseValue) &&
      patchValue !== null &&
      typeof patchValue === "object" &&
      !Array.isArray(patchValue)
    ) {
      out[key] = deepMerge(
        baseValue as Record<string, unknown>,
        patchValue as Record<string, unknown>,
      );
    } else {
      out[key] = patchValue;
    }
  }
  return out;
}
