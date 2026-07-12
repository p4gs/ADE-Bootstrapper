import { afterEach, beforeEach, describe, expect, test } from "bun:test";
import { join } from "node:path";
import { costGovernanceModule, BUDGET_POLICY_PATH, validateBudgetOptions } from "../../src/modules/cost-governance.ts";
import { makeTempDir, makeTestCtx, removeDir, testConfig } from "../helpers.ts";
import { sha256 } from "../../src/fsutil.ts";

let dir: string;

beforeEach(async () => {
  dir = await makeTempDir();
});

afterEach(async () => {
  await removeDir(dir);
});

describe("cost-governance module", () => {
  test("ISC-104: apply writes budget policy with limits, alerts, and model routing", async () => {
    const ctx = makeTestCtx(dir);
    const result = await costGovernanceModule.apply(ctx);
    expect(result.status).toBe("applied");
    expect(result.wrotePaths).toContain(BUDGET_POLICY_PATH);
    const policy = JSON.parse(await Bun.file(join(dir, BUDGET_POLICY_PATH)).text());
    expect(policy.limits.perSessionTokens).toBeGreaterThan(0);
    expect(policy.limits.perProjectDailyCostUsd).toBeGreaterThan(0);
    expect(policy.alerts.warnAtFraction).toBeGreaterThan(0);
    expect(policy.routing.policy).toContain("cheapest");
  });

  test("ISC-105: instruction block carries the budget contract to harnesses", () => {
    expect(costGovernanceModule.instructionBlocks.length).toBeGreaterThan(0);
    const block = costGovernanceModule.instructionBlocks[0]!;
    expect(block.content).toContain(".ade/policy/budget.json");
  });

  test("ISC-106: malformed budget options are rejected — apply fails, nothing written", async () => {
    const config = testConfig({
      modules: { "cost-governance": { enabled: true, options: { perSessionTokens: -5 } } },
    });
    const ctx = makeTestCtx(dir, { config });
    const result = await costGovernanceModule.apply(ctx);
    expect(result.status).toBe("failed");
    expect(result.findings.some((finding) => finding.level === "error")).toBe(true);
    expect(await Bun.file(join(dir, BUDGET_POLICY_PATH)).exists()).toBe(false);
  });

  test("ISC-106: validateBudgetOptions catches every malformed numeric", () => {
    expect(validateBudgetOptions({})).toEqual([]);
    expect(validateBudgetOptions({ perSessionTokens: 100 })).toEqual([]);
    expect(validateBudgetOptions({ perSessionTokens: 0 }).length).toBe(1);
    expect(validateBudgetOptions({ perSessionCostUsd: "many" }).length).toBe(1);
    expect(validateBudgetOptions({ warnAtFraction: 1.5 }).length).toBe(1);
  });

  test("ISC-116: plan writes nothing to disk", async () => {
    const ctx = makeTestCtx(dir);
    const actions = await costGovernanceModule.plan(ctx);
    expect(actions.length).toBeGreaterThan(0);
    expect(await Bun.file(join(dir, BUDGET_POLICY_PATH)).exists()).toBe(false);
  });

  test("ISC-117: apply is idempotent — second run byte-identical", async () => {
    const ctx = makeTestCtx(dir);
    await costGovernanceModule.apply(ctx);
    const first = sha256(await Bun.file(join(dir, BUDGET_POLICY_PATH)).text());
    await costGovernanceModule.apply(makeTestCtx(dir));
    const second = sha256(await Bun.file(join(dir, BUDGET_POLICY_PATH)).text());
    expect(second).toBe(first);
  });

  test("ISC-33: options override default limits", async () => {
    const config = testConfig({
      modules: { "cost-governance": { enabled: true, options: { perSessionCostUsd: 5 } } },
    });
    const ctx = makeTestCtx(dir, { config });
    await costGovernanceModule.apply(ctx);
    const policy = JSON.parse(await Bun.file(join(dir, BUDGET_POLICY_PATH)).text());
    expect(policy.limits.perSessionCostUsd).toBe(5);
  });

  test("verify passes after apply and fails on corrupted policy", async () => {
    const ctx = makeTestCtx(dir);
    await costGovernanceModule.apply(ctx);
    expect((await costGovernanceModule.verify(ctx)).ok).toBe(true);

    await Bun.write(join(dir, BUDGET_POLICY_PATH), "{not json");
    expect((await costGovernanceModule.verify(ctx)).ok).toBe(false);

    await Bun.write(
      join(dir, BUDGET_POLICY_PATH),
      JSON.stringify({ limits: { perSessionTokens: -1 } }),
    );
    expect((await costGovernanceModule.verify(ctx)).ok).toBe(false);
  });

  test("detect reports option validity", async () => {
    const ok = await costGovernanceModule.detect(makeTestCtx(dir));
    expect(ok.every((finding) => finding.level === "ok")).toBe(true);
    const bad = await costGovernanceModule.detect(
      makeTestCtx(dir, {
        config: testConfig({ modules: { "cost-governance": { enabled: true, options: { perSessionTokens: -1 } } } }),
      }),
    );
    expect(bad.some((finding) => finding.level === "error")).toBe(true);
  });
});
