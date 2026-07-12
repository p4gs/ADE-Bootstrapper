import { afterEach, beforeEach, describe, expect, test } from "bun:test";
import { join } from "node:path";
import {
  tokenEfficiencyModule,
  TOKEN_EFFICIENCY_POLICY_PATH,
} from "../../src/modules/token-efficiency.ts";
import { makeTempDir, makeTestCtx, removeDir } from "../helpers.ts";
import { sha256 } from "../../src/fsutil.ts";

let dir: string;

beforeEach(async () => {
  dir = await makeTempDir();
});

afterEach(async () => {
  await removeDir(dir);
});

describe("token-efficiency module", () => {
  test("ISC-111: rtk present → applied with enabled policy at the shell boundary", async () => {
    const ctx = makeTestCtx(dir, { presentTools: { rtk: "rtk 0.9.2" } });
    const result = await tokenEfficiencyModule.apply(ctx);
    expect(result.status).toBe("applied");
    expect(result.wrotePaths).toContain(TOKEN_EFFICIENCY_POLICY_PATH);
    const policy = JSON.parse(await Bun.file(join(dir, TOKEN_EFFICIENCY_POLICY_PATH)).text());
    expect(policy.enabled).toBe(true);
    expect(policy.tool).toBe("rtk");
    expect(policy.integration).toBe("shell-boundary");
    expect(policy.mechanisms).toEqual(["filtering", "grouping", "truncation", "deduplication"]);
    expect(policy.guarantee).toContain("reversible/semantically-lossless");
  });

  test("ISC-112: rtk absent → policy still written with enabled:false, degraded finding with install guidance", async () => {
    const ctx = makeTestCtx(dir);
    const result = await tokenEfficiencyModule.apply(ctx);
    expect(result.status).toBe("degraded");
    expect(result.wrotePaths).toContain(TOKEN_EFFICIENCY_POLICY_PATH);
    const policy = JSON.parse(await Bun.file(join(dir, TOKEN_EFFICIENCY_POLICY_PATH)).text());
    expect(policy.enabled).toBe(false);
    const degraded = result.findings.find((finding) => finding.level === "degraded");
    expect(degraded).toBeDefined();
    expect(degraded?.remediation).toContain("github.com/rtk-ai/rtk");
    expect(degraded?.remediation).toContain("60-90%");
  });

  test("ISC-113: policy records the detected rtk version", async () => {
    const ctx = makeTestCtx(dir, { presentTools: { rtk: "rtk 1.4.7" } });
    await tokenEfficiencyModule.apply(ctx);
    const policy = JSON.parse(await Bun.file(join(dir, TOKEN_EFFICIENCY_POLICY_PATH)).text());
    expect(policy.toolVersion).toBe("rtk 1.4.7");
  });

  test("ISC-116: plan writes nothing to disk", async () => {
    const ctx = makeTestCtx(dir, { presentTools: { rtk: "rtk 0.9.2" } });
    const actions = await tokenEfficiencyModule.plan(ctx);
    expect(actions.length).toBeGreaterThan(0);
    expect(actions[0]?.path).toBe(TOKEN_EFFICIENCY_POLICY_PATH);
    expect(await Bun.file(join(dir, TOKEN_EFFICIENCY_POLICY_PATH)).exists()).toBe(false);
  });

  test("ISC-117: apply is idempotent — second run byte-identical", async () => {
    await tokenEfficiencyModule.apply(makeTestCtx(dir, { presentTools: { rtk: "rtk 0.9.2" } }));
    const first = sha256(await Bun.file(join(dir, TOKEN_EFFICIENCY_POLICY_PATH)).text());
    await tokenEfficiencyModule.apply(makeTestCtx(dir, { presentTools: { rtk: "rtk 0.9.2" } }));
    const second = sha256(await Bun.file(join(dir, TOKEN_EFFICIENCY_POLICY_PATH)).text());
    expect(second).toBe(first);
  });

  test("instruction block carries rtk-wrapping and verbatim-failure guidance", () => {
    expect(tokenEfficiencyModule.instructionBlocks.length).toBeGreaterThan(0);
    const block = tokenEfficiencyModule.instructionBlocks[0]!;
    expect(block.content).toContain(".ade/policy/token-efficiency.json");
    expect(block.content).toContain("rtk-wrapped");
    expect(block.content.toLowerCase()).toContain("verbatim");
  });

  test("detect reports rtk presence and absence with remediation", async () => {
    const present = await tokenEfficiencyModule.detect(
      makeTestCtx(dir, { presentTools: { rtk: "rtk 0.9.2" } }),
    );
    expect(present.every((finding) => finding.level === "ok")).toBe(true);
    const absent = await tokenEfficiencyModule.detect(makeTestCtx(dir));
    expect(
      absent.some(
        (finding) =>
          finding.level === "degraded" && finding.remediation?.includes("github.com/rtk-ai/rtk"),
      ),
    ).toBe(true);
  });

  test("verify: passes after apply, fails on missing or corrupted policy", async () => {
    const ctx = makeTestCtx(dir, { presentTools: { rtk: "rtk 0.9.2" } });
    expect((await tokenEfficiencyModule.verify(ctx)).ok).toBe(false);
    await tokenEfficiencyModule.apply(ctx);
    expect((await tokenEfficiencyModule.verify(ctx)).ok).toBe(true);

    await Bun.write(join(dir, TOKEN_EFFICIENCY_POLICY_PATH), "{not json");
    expect((await tokenEfficiencyModule.verify(ctx)).ok).toBe(false);

    await Bun.write(join(dir, TOKEN_EFFICIENCY_POLICY_PATH), JSON.stringify({ enabled: "yes" }));
    expect((await tokenEfficiencyModule.verify(ctx)).ok).toBe(false);
  });

  test("verify: enabled must equal rtk presence in the CURRENT ctx (re-derived)", async () => {
    // Applied with rtk present, then verified on a machine without rtk → stale policy fails.
    await tokenEfficiencyModule.apply(makeTestCtx(dir, { presentTools: { rtk: "rtk 0.9.2" } }));
    const withoutRtk = makeTestCtx(dir);
    const staleEnabled = await tokenEfficiencyModule.verify(withoutRtk);
    expect(staleEnabled.ok).toBe(false);
    expect(staleEnabled.findings.some((finding) => finding.level === "error")).toBe(true);

    // Applied without rtk, then verified on a machine with rtk → also stale, also fails.
    await tokenEfficiencyModule.apply(withoutRtk);
    const staleDisabled = await tokenEfficiencyModule.verify(
      makeTestCtx(dir, { presentTools: { rtk: "rtk 0.9.2" } }),
    );
    expect(staleDisabled.ok).toBe(false);

    // Re-applying on the current machine repairs verification.
    expect((await tokenEfficiencyModule.verify(withoutRtk)).ok).toBe(true);
  });
});
