import { describe, expect, test } from "bun:test";
import { collectBlocks, composeInstructions } from "../src/instructions.ts";
import { costGovernanceModule } from "../src/modules/cost-governance.ts";
import { secretsModule } from "../src/modules/secrets.ts";

describe("canonical instructions composer", () => {
  test("ISC-53: composition includes header, edit guidance, and enabled module blocks in order", () => {
    const blocks = collectBlocks([secretsModule, costGovernanceModule], new Set(["secrets", "cost-governance"]));
    const composed = composeInstructions(blocks);
    expect(composed).toContain("# ADE Baseline Instructions");
    expect(composed).toContain("ade translate");
    const secretsIndex = composed.indexOf("Secrets & Credential Hygiene");
    const costIndex = composed.indexOf("Cost & Token Budget");
    expect(secretsIndex).toBeGreaterThan(0);
    expect(costIndex).toBeGreaterThan(secretsIndex);
  });

  test("ISC-31-adjacent: disabled modules contribute no blocks", () => {
    const blocks = collectBlocks([secretsModule, costGovernanceModule], new Set(["cost-governance"]));
    const composed = composeInstructions(blocks);
    expect(composed).not.toContain("Secrets & Credential Hygiene");
    expect(composed).toContain("Cost & Token Budget");
  });

  test("composition is deterministic", () => {
    const enabled = new Set(["secrets", "cost-governance"]);
    const first = composeInstructions(collectBlocks([secretsModule, costGovernanceModule], enabled));
    const second = composeInstructions(collectBlocks([secretsModule, costGovernanceModule], enabled));
    expect(second).toBe(first);
  });
});
