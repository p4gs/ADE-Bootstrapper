import { afterEach, beforeEach, describe, expect, test } from "bun:test";
import { rm } from "node:fs/promises";
import { join } from "node:path";
import {
  configGovernanceModule,
  GOVERNANCE_POLICY_PATH,
} from "../../src/modules/config-governance.ts";
import { upsertManagedBlock } from "../../src/managed.ts";
import { sha256 } from "../../src/fsutil.ts";
import { makeTempDir, makeTestCtx, removeDir, testConfig } from "../helpers.ts";

const CANONICAL_BODY = "# ADE Baseline Instructions\n\n- Follow the ADE baseline.\n";

/** Seed the canonical source + translated instruction files for claude-code + codex. */
async function seedTranslatedFixture(dir: string): Promise<void> {
  await Bun.write(join(dir, ".ade", "instructions.md"), CANONICAL_BODY);
  for (const rel of ["CLAUDE.md", "AGENTS.md"]) {
    const upsert = upsertManagedBlock("", CANONICAL_BODY);
    if (!upsert.ok) throw new Error(upsert.error);
    await Bun.write(join(dir, rel), upsert.content);
  }
}

let dir: string;

beforeEach(async () => {
  dir = await makeTempDir();
});

afterEach(async () => {
  await removeDir(dir);
});

describe("config-governance module", () => {
  test("ISC-58: apply writes instructions-governance policy with canonical source, managed files, drift + ownership rules", async () => {
    const ctx = makeTestCtx(dir, { config: testConfig({ harnesses: ["claude-code", "codex"] }) });
    const result = await configGovernanceModule.apply(ctx);
    expect(result.status).toBe("applied");
    expect(result.wrotePaths).toContain(GOVERNANCE_POLICY_PATH);

    const policy = JSON.parse(await Bun.file(join(dir, GOVERNANCE_POLICY_PATH)).text());
    expect(policy.canonicalSource).toBe(".ade/instructions.md");
    expect(policy.managedFiles).toEqual([
      { harnesses: ["codex"], path: "AGENTS.md" },
      { harnesses: ["claude-code"], path: "CLAUDE.md" },
    ]);
    expect(policy.driftPolicy).toContain("ade translate");
    expect(policy.driftPolicy).toContain("refused");
    expect(policy.ownership).toContain("user-owned");
  });

  test("ISC-58: managedFiles dedupe shared AGENTS.md across harnesses", async () => {
    const ctx = makeTestCtx(dir, { config: testConfig({ harnesses: ["codex", "opencode"] }) });
    await configGovernanceModule.apply(ctx);
    const policy = JSON.parse(await Bun.file(join(dir, GOVERNANCE_POLICY_PATH)).text());
    expect(policy.managedFiles).toEqual([{ harnesses: ["codex", "opencode"], path: "AGENTS.md" }]);
  });

  test("ISC-116: plan writes nothing to disk", async () => {
    const ctx = makeTestCtx(dir);
    const actions = await configGovernanceModule.plan(ctx);
    expect(actions.length).toBeGreaterThan(0);
    expect(actions[0]!.path).toBe(GOVERNANCE_POLICY_PATH);
    expect(await Bun.file(join(dir, GOVERNANCE_POLICY_PATH)).exists()).toBe(false);
  });

  test("ISC-117: apply is idempotent — second run byte-identical", async () => {
    await configGovernanceModule.apply(makeTestCtx(dir));
    const first = sha256(await Bun.file(join(dir, GOVERNANCE_POLICY_PATH)).text());
    await configGovernanceModule.apply(makeTestCtx(dir));
    const second = sha256(await Bun.file(join(dir, GOVERNANCE_POLICY_PATH)).text());
    expect(second).toBe(first);
  });

  test("verify passes on a fully translated repo (claude-code + codex)", async () => {
    const ctx = makeTestCtx(dir, { config: testConfig({ harnesses: ["claude-code", "codex"] }) });
    await seedTranslatedFixture(dir);
    await configGovernanceModule.apply(ctx);
    const result = await configGovernanceModule.verify(ctx);
    expect(result.ok).toBe(true);
    expect(result.findings.every((finding) => finding.level === "ok")).toBe(true);
  });

  test("verify fails naming AGENTS.md when it is deleted, with ade translate remediation", async () => {
    const ctx = makeTestCtx(dir, { config: testConfig({ harnesses: ["claude-code", "codex"] }) });
    await seedTranslatedFixture(dir);
    await configGovernanceModule.apply(ctx);
    await rm(join(dir, "AGENTS.md"));
    const result = await configGovernanceModule.verify(ctx);
    expect(result.ok).toBe(false);
    const missing = result.findings.find(
      (finding) => finding.level === "error" && finding.message.includes("AGENTS.md"),
    );
    expect(missing).toBeDefined();
    expect(missing!.remediation).toContain("ade translate");
  });

  test("verify fails naming CLAUDE.md when its managed markers are corrupted", async () => {
    const ctx = makeTestCtx(dir, { config: testConfig({ harnesses: ["claude-code", "codex"] }) });
    await seedTranslatedFixture(dir);
    await configGovernanceModule.apply(ctx);
    const claudeMd = await Bun.file(join(dir, "CLAUDE.md")).text();
    await Bun.write(join(dir, "CLAUDE.md"), `${claudeMd}\n<!-- ade:begin -->\n`);
    const result = await configGovernanceModule.verify(ctx);
    expect(result.ok).toBe(false);
    const corrupt = result.findings.find(
      (finding) => finding.level === "error" && finding.message.includes("CLAUDE.md"),
    );
    expect(corrupt).toBeDefined();
    expect(corrupt!.message).toContain("managed block");
    expect(corrupt!.remediation).toContain("ade translate");
  });

  test("verify fails when the canonical .ade/instructions.md is missing", async () => {
    const ctx = makeTestCtx(dir, { config: testConfig({ harnesses: ["claude-code", "codex"] }) });
    await seedTranslatedFixture(dir);
    await configGovernanceModule.apply(ctx);
    await rm(join(dir, ".ade", "instructions.md"));
    const result = await configGovernanceModule.verify(ctx);
    expect(result.ok).toBe(false);
    expect(
      result.findings.some(
        (finding) => finding.level === "error" && finding.message.includes(".ade/instructions.md"),
      ),
    ).toBe(true);
  });

  test("verify fails when the governance policy artifact is missing or corrupt", async () => {
    const ctx = makeTestCtx(dir, { config: testConfig({ harnesses: ["claude-code", "codex"] }) });
    await seedTranslatedFixture(dir);
    // Never applied → policy artifact absent.
    expect((await configGovernanceModule.verify(ctx)).ok).toBe(false);

    await configGovernanceModule.apply(ctx);
    await Bun.write(join(dir, GOVERNANCE_POLICY_PATH), "{not json");
    expect((await configGovernanceModule.verify(ctx)).ok).toBe(false);
  });

  test("instruction block names the canonical source and the translate workflow", () => {
    expect(configGovernanceModule.instructionBlocks.length).toBeGreaterThan(0);
    const block = configGovernanceModule.instructionBlocks[0]!;
    expect(block.content).toContain(".ade/instructions.md");
    expect(block.content).toContain("ade translate");
    expect(block.content).toContain("content-hash");
  });

  test("detect reports the governed instruction files, and warns when no harness resolves", async () => {
    const ok = await configGovernanceModule.detect(
      makeTestCtx(dir, { config: testConfig({ harnesses: ["claude-code", "codex"] }) }),
    );
    expect(ok.some((finding) => finding.level === "ok" && finding.message.includes("CLAUDE.md"))).toBe(true);
    const none = await configGovernanceModule.detect(
      makeTestCtx(dir, { config: testConfig({ harnesses: [] }) }),
    );
    expect(none.some((finding) => finding.level === "warn")).toBe(true);
  });
});
