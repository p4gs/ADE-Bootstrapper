import { describe, expect, test } from "bun:test";
import { extractManagedBlock, renderManagedBlock, upsertManagedBlock } from "../src/managed.ts";
import { MARKER_BEGIN, MARKER_END } from "../src/version.ts";

const BODY_A = "# Instructions\n\nDo the right thing.";
const BODY_B = "# Instructions v2\n\nDo the even righter thing.";

describe("managed block engine", () => {
  test("ISC-54: fresh file gets markers + provenance + body", () => {
    const result = upsertManagedBlock("", BODY_A);
    expect(result.ok).toBe(true);
    if (!result.ok) return;
    expect(result.content).toContain(MARKER_BEGIN);
    expect(result.content).toContain(MARKER_END);
    expect(result.content).toContain(BODY_A);
    expect(result.content).toContain("content-hash:");
  });

  test("ISC-55: user content above and below markers preserved byte-for-byte", () => {
    const above = "# User heading\n\nuser prose that must survive.\n";
    const below = "\n## User appendix\nmore user prose.\n";
    const seeded = upsertManagedBlock(above, BODY_A);
    expect(seeded.ok).toBe(true);
    if (!seeded.ok) return;
    const withBelow = seeded.content + below;
    const updated = upsertManagedBlock(withBelow, BODY_B);
    expect(updated.ok).toBe(true);
    if (!updated.ok) return;
    expect(updated.content.startsWith(above)).toBe(true);
    expect(updated.content.endsWith(below)).toBe(true);
    expect(updated.content).toContain(BODY_B);
    expect(updated.content).not.toContain("Do the right thing.");
  });

  test("ISC-56: re-upserting identical body is a no-op (changed=false, byte-identical)", () => {
    const first = upsertManagedBlock("user text\n", BODY_A);
    expect(first.ok).toBe(true);
    if (!first.ok) return;
    const second = upsertManagedBlock(first.content, BODY_A);
    expect(second.ok).toBe(true);
    if (!second.ok) return;
    expect(second.changed).toBe(false);
    expect(second.content).toBe(first.content);
  });

  test("ISC-55.1: duplicated markers refuse rewrite", () => {
    const corrupt = `${MARKER_BEGIN}\nx\n${MARKER_END}\n${MARKER_BEGIN}\ny\n${MARKER_END}\n`;
    const result = upsertManagedBlock(corrupt, BODY_A);
    expect(result.ok).toBe(false);
    if (result.ok) return;
    expect(result.error).toContain("corrupt");
  });

  test("ISC-55.1: unterminated marker refuses rewrite", () => {
    const corrupt = `user\n${MARKER_BEGIN}\ndangling`;
    const result = upsertManagedBlock(corrupt, BODY_A);
    expect(result.ok).toBe(false);
  });

  test("ISC-55.1: reversed markers refuse rewrite", () => {
    const corrupt = `${MARKER_END}\nmiddle\n${MARKER_BEGIN}\n`;
    const result = upsertManagedBlock(corrupt, BODY_A);
    expect(result.ok).toBe(false);
    if (result.ok) return;
    expect(result.error.toLowerCase()).toContain("revers");
  });

  test("ISC-55.2: hand-edit inside the block (hash mismatch) is refused with remediation", () => {
    const seeded = upsertManagedBlock("", BODY_A);
    expect(seeded.ok).toBe(true);
    if (!seeded.ok) return;
    const tampered = seeded.content.replace("Do the right thing.", "Do the WRONG thing.");
    const result = upsertManagedBlock(tampered, BODY_B);
    expect(result.ok).toBe(false);
    if (result.ok) return;
    expect(result.error).toContain("hand-edited");
    expect(result.error).toContain("ade translate");
  });

  test("ISC-58: provenance line names the canonical source and warns against hand-edits", () => {
    const block = renderManagedBlock(BODY_A);
    expect(block).toContain(".ade/instructions.md");
    expect(block).toContain("do not hand-edit");
  });

  test("extractManagedBlock roundtrips render output and rejects corrupt input", () => {
    const seeded = upsertManagedBlock("above\n", BODY_A);
    expect(seeded.ok).toBe(true);
    if (!seeded.ok) return;
    const extracted = extractManagedBlock(seeded.content);
    expect(extracted).toBe(renderManagedBlock(BODY_A));
    expect(extractManagedBlock("no markers here")).toBeNull();
    expect(extractManagedBlock(`${MARKER_BEGIN} only`)).toBeNull();
  });
});
