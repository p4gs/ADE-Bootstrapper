/**
 * Managed-block engine for user-owned files (CLAUDE.md, AGENTS.md, …).
 *
 * ADE content lives strictly between `<!-- ade:begin -->` and `<!-- ade:end -->`.
 * Content outside the markers is user-owned and is never modified or deleted.
 * The block's provenance line embeds a sha256 of the block body so that a
 * hand-edit INSIDE the block is detected and refused rather than clobbered.
 * Corrupt marker states abort — the file is never rewritten on ambiguity.
 */
import { sha256 } from "./fsutil.ts";
import { MARKER_BEGIN, MARKER_END } from "./version.ts";

export interface UpsertOk {
  ok: true;
  content: string;
  changed: boolean;
}

export interface UpsertError {
  ok: false;
  error: string;
}

export type UpsertResult = UpsertOk | UpsertError;

const HASH_RE = /content-hash:([0-9a-f]{64})/;

function countOccurrences(haystack: string, needle: string): number {
  let count = 0;
  let index = haystack.indexOf(needle);
  while (index !== -1) {
    count += 1;
    index = haystack.indexOf(needle, index + needle.length);
  }
  return count;
}

/** Render the full managed block (markers + hash-bearing provenance + body). */
export function renderManagedBlock(body: string): string {
  const normalized = body.trimEnd();
  const provenance = `<!-- Managed by ADE Bootstrapper — generated from .ade/instructions.md; do not hand-edit this block; run \`ade translate\` to regenerate. content-hash:${sha256(normalized)} -->`;
  return `${MARKER_BEGIN}\n${provenance}\n\n${normalized}\n${MARKER_END}`;
}

interface ParsedBlock {
  beginIndex: number;
  endIndex: number;
  /** Body between provenance line and end marker, trimmed like render's input. */
  body: string;
  declaredHash: string | null;
}

function parseBlock(existing: string): ParsedBlock | null {
  const beginIndex = existing.indexOf(MARKER_BEGIN);
  const endIndex = existing.indexOf(MARKER_END);
  if (beginIndex === -1 || endIndex === -1 || beginIndex >= endIndex) return null;
  const inner = existing.slice(beginIndex + MARKER_BEGIN.length, endIndex);
  const hashMatch = HASH_RE.exec(inner);
  const provenanceEnd = inner.indexOf("-->");
  const body =
    provenanceEnd === -1
      ? inner.trim()
      : inner.slice(provenanceEnd + "-->".length).trim();
  return {
    beginIndex,
    endIndex,
    body,
    declaredHash: hashMatch ? hashMatch[1]! : null,
  };
}

/**
 * Insert or replace the managed block in `existing`.
 * - no markers → append (with separating blank line)
 * - one balanced block, body hash intact → replace
 * - one balanced block, body hash MISMATCH → error (user edited inside; refuse)
 * - anything else (unbalanced, duplicated, reversed) → error, file untouched
 */
export function upsertManagedBlock(existing: string, body: string): UpsertResult {
  const block = renderManagedBlock(body);
  const begins = countOccurrences(existing, MARKER_BEGIN);
  const ends = countOccurrences(existing, MARKER_END);

  if (begins === 0 && ends === 0) {
    const separator = existing.length === 0 ? "" : existing.endsWith("\n") ? "\n" : "\n\n";
    return { ok: true, content: `${existing}${separator}${block}\n`, changed: true };
  }

  if (begins === 1 && ends === 1) {
    const parsed = parseBlock(existing);
    if (parsed === null) {
      return {
        ok: false,
        error:
          "managed markers are reversed (end before begin) — refusing to rewrite; fix the markers manually",
      };
    }
    if (parsed.declaredHash === null) {
      // Every block ADE writes carries a content-hash provenance line. Markers with
      // no hash are NOT our block — they are user (or foreign-tool) content that
      // happens to use the same marker strings. Replacing it would destroy content
      // outside our ownership, which is the one thing this engine must never do.
      return {
        ok: false,
        error:
          "found ade markers with no ADE provenance line — this content was not written by ade; refusing to overwrite it. Remove or rename the markers if you want ade to manage this file.",
      };
    }
    if (sha256(parsed.body) !== parsed.declaredHash) {
      return {
        ok: false,
        error:
          "managed block was hand-edited (content-hash mismatch) — refusing to overwrite; move your changes outside the ade markers (or into .ade/instructions.local.md) and re-run `ade translate`",
      };
    }
    const before = existing.slice(0, parsed.beginIndex);
    const after = existing.slice(parsed.endIndex + MARKER_END.length);
    const next = `${before}${block}${after}`;
    return { ok: true, content: next, changed: next !== existing };
  }

  return {
    ok: false,
    error: `managed markers are corrupt (${begins} begin, ${ends} end) — refusing to rewrite; fix the markers manually`,
  };
}

/** Extract the current managed block text (markers inclusive), or null when absent/corrupt. */
export function extractManagedBlock(existing: string): string | null {
  const begins = countOccurrences(existing, MARKER_BEGIN);
  const ends = countOccurrences(existing, MARKER_END);
  if (begins !== 1 || ends !== 1) return null;
  const parsed = parseBlock(existing);
  if (parsed === null) return null;
  return existing.slice(parsed.beginIndex, parsed.endIndex + MARKER_END.length);
}
