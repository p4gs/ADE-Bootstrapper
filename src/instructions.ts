/**
 * Canonical instructions composer — `.ade/instructions.md`.
 *
 * One source of truth for harness instructions. Modules contribute static
 * blocks; composition order follows registry order (deterministic). The
 * translate step renders this file into every target harness's instruction
 * surface inside managed markers.
 */
import type { AdeModule, InstructionBlock } from "./types.ts";

export const INSTRUCTIONS_PATH = ".ade/instructions.md";

const HEADER = `# ADE Baseline Instructions

> Canonical source: \`.ade/instructions.md\` — managed by ADE Bootstrapper.
> Edit THIS file, then run \`ade translate\` to propagate to every harness.
> Direct edits to the generated blocks in CLAUDE.md / AGENTS.md / .cursor rules will be refused by \`ade translate\`.
`;

export function collectBlocks(modules: AdeModule[], enabledIds: Set<string>): InstructionBlock[] {
  const blocks: InstructionBlock[] = [];
  for (const module of modules) {
    if (!enabledIds.has(module.id)) continue;
    blocks.push(...module.instructionBlocks);
  }
  return blocks;
}

export function composeInstructions(blocks: InstructionBlock[]): string {
  const sections = blocks.map(
    (block) => `### ${block.title}\n\n${block.content.trim()}\n`,
  );
  return `${HEADER}\n${sections.join("\n")}`;
}
