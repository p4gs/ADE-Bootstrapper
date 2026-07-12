/**
 * Canonical instructions composer — `.ade/instructions.md`.
 *
 * `.ade/instructions.md` is a GENERATED artifact (module blocks, registry order,
 * deterministic). The user-writable surface is `.ade/instructions.local.md`:
 * ADE creates it once, never overwrites it, never hash-locks it, and appends its
 * content to every harness's managed block. That separation is what lets the
 * generated file be lock-verified while project-specific rules still survive
 * `ade apply`.
 */
import type { AdeModule, InstructionBlock } from "./types.ts";

export const INSTRUCTIONS_PATH = ".ade/instructions.md";
export const LOCAL_INSTRUCTIONS_PATH = ".ade/instructions.local.md";

const HEADER = `# ADE Baseline Instructions

> GENERATED FILE — do not edit. Regenerated from the enabled modules on every \`ade apply\` / \`ade translate\`; edits here are overwritten and fail \`ade verify\`.
> Project-specific instructions belong in \`.ade/instructions.local.md\` — that file is yours, is never overwritten, and its content is appended to every harness's managed block.
`;

/** The stub written once at init; never overwritten if the user has edited it. */
export const LOCAL_INSTRUCTIONS_STUB = `# Project Instructions (user-owned)

<!-- This file is YOURS. ADE creates it once and never overwrites it.
     Everything below is appended verbatim to every harness's managed block
     (CLAUDE.md, AGENTS.md, .cursor/rules/ade.mdc) on \`ade apply\` / \`ade translate\`.
     Delete this comment and write your project's rules here. -->
`;

export function collectBlocks(modules: AdeModule[], enabledIds: Set<string>): InstructionBlock[] {
  const blocks: InstructionBlock[] = [];
  for (const module of modules) {
    if (!enabledIds.has(module.id)) continue;
    blocks.push(...module.instructionBlocks);
  }
  return blocks;
}

/** The generated canonical file's content (module blocks only). */
export function composeInstructions(blocks: InstructionBlock[]): string {
  const sections = blocks.map((block) => `### ${block.title}\n\n${block.content.trim()}\n`);
  return `${HEADER}\n${sections.join("\n")}`;
}

/**
 * The body rendered into every harness managed block: the generated baseline
 * plus the user's own project instructions, when they wrote any.
 */
export function composeManagedBody(generated: string, local: string | null): string {
  const userContent = stripStub(local);
  if (userContent === null) return generated;
  return `${generated.trimEnd()}\n\n### Project-Specific Instructions\n\n${userContent}\n`;
}

/** Null when the local file is absent, empty, or still the untouched stub. */
function stripStub(local: string | null): string | null {
  if (local === null) return null;
  const withoutComments = local.replace(/<!--[\s\S]*?-->/g, "");
  const meaningful = withoutComments
    .split("\n")
    .filter((line) => line.trim().length > 0 && line.trim() !== "# Project Instructions (user-owned)")
    .join("\n")
    .trim();
  return meaningful.length > 0 ? meaningful : null;
}
