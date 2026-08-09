import { afterEach, beforeEach, describe, expect, test } from "bun:test";
import { join } from "node:path";
import {
  supplyChainModule,
  checkLockfiles,
  validateSupplyChainOptions,
  AI_NATIVE_DEP_KINDS,
  DEPENDENCIES_POLICY_PATH,
  DEFAULT_MIN_AGE_DAYS,
  REGISTRY_ALLOWLIST,
  SSCSB_CONFIG_PATH,
  SSCSB_DEEP_LAYER,
  SSCSB_INSTALL,
} from "../../src/modules/supply-chain.ts";
import { makeTempDir, makeTestCtx, removeDir, testConfig } from "../helpers.ts";
import { sha256 } from "../../src/fsutil.ts";

let dir: string;

beforeEach(async () => {
  dir = await makeTempDir();
});

afterEach(async () => {
  await removeDir(dir);
});

async function readPolicy(): Promise<Record<string, any>> {
  return JSON.parse(await Bun.file(join(dir, DEPENDENCIES_POLICY_PATH)).text());
}

describe("supply-chain module", () => {
  test("ISC-63: apply writes dependency policy with registry allowlist, min age, lockfile + review requirements, and typosquatting policy", async () => {
    const ctx = makeTestCtx(dir, { presentTools: { "osv-scanner": "osv-scanner 2.0.0" } });
    const result = await supplyChainModule.apply(ctx);
    expect(result.wrotePaths).toContain(DEPENDENCIES_POLICY_PATH);
    const policy = await readPolicy();
    expect(policy.registries.allowlist).toEqual(
      expect.arrayContaining(["registry.npmjs.org", "pypi.org", "crates.io", "proxy.golang.org"]),
    );
    expect(policy.registries.allowlist.length).toBe(REGISTRY_ALLOWLIST.length);
    expect(policy.minAgeDays).toBe(DEFAULT_MIN_AGE_DAYS);
    expect(policy.requireLockfiles).toBe(true);
    expect(policy.installReview).toBe("required");
    expect(policy.typosquattingPolicy).toBe(
      "verify exact package name against its repository before install",
    );
  });

  test("ISC-63: minAgeDays option overrides the 14-day default", async () => {
    const config = testConfig({
      modules: { "supply-chain": { enabled: true, options: { minAgeDays: 30 } } },
    });
    await supplyChainModule.apply(
      makeTestCtx(dir, { config, presentTools: { "osv-scanner": "2.0.0" } }),
    );
    const policy = await readPolicy();
    expect(policy.minAgeDays).toBe(30);
  });

  test("ISC-63: malformed minAgeDays option → apply fails, nothing written", async () => {
    const config = testConfig({
      modules: { "supply-chain": { enabled: true, options: { minAgeDays: "soon" } } },
    });
    const result = await supplyChainModule.apply(makeTestCtx(dir, { config }));
    expect(result.status).toBe("failed");
    expect(result.findings.some((finding) => finding.level === "error")).toBe(true);
    expect(await Bun.file(join(dir, DEPENDENCIES_POLICY_PATH)).exists()).toBe(false);
    expect(validateSupplyChainOptions({ minAgeDays: -1 }).length).toBe(1);
    expect(validateSupplyChainOptions({ minAgeDays: 7 })).toEqual([]);
    expect(validateSupplyChainOptions({})).toEqual([]);
  });

  test("ISC-64: policy has a first-class aiNativeDependencies section treating all five kinds as untrusted code", async () => {
    await supplyChainModule.apply(makeTestCtx(dir, { presentTools: { "osv-scanner": "2.0.0" } }));
    const policy = await readPolicy();
    expect(AI_NATIVE_DEP_KINDS).toEqual([
      "skills",
      "plugins",
      "mcpServers",
      "instructionPacks",
      "agentConfigs",
    ] as any);
    for (const kind of ["skills", "plugins", "mcpServers", "instructionPacks", "agentConfigs"]) {
      const entry = policy.aiNativeDependencies[kind];
      expect(entry.rule).toBe("review-before-install");
      expect(entry.pinningRequired).toBe(true);
      expect(entry.sourceAllowlist).toEqual(["explicit-user-approval"]);
    }
  });

  test("ISC-65: detect flags absent osv-scanner as degraded with install remediation, ok when present", async () => {
    const absent = await supplyChainModule.detect(makeTestCtx(dir));
    const degraded = absent.find((finding) => finding.level === "degraded");
    expect(degraded).toBeDefined();
    expect(degraded!.remediation).toContain("brew install osv-scanner");

    const present = await supplyChainModule.detect(
      makeTestCtx(dir, { presentTools: { "osv-scanner": "osv-scanner 2.0.0", sscsb: "sscsb 0.1.0" } }),
    );
    expect(present.some((finding) => finding.level === "ok" && finding.message.includes("osv-scanner"))).toBe(true);
    expect(present.some((finding) => finding.level === "degraded")).toBe(false);
  });

  test("sscsb present → detect reports ok + the deep-SSCS-layer guidance (init + verify)", async () => {
    const findings = await supplyChainModule.detect(makeTestCtx(dir, { presentTools: { sscsb: "sscsb 0.1.0" } }));
    expect(findings.some((finding) => finding.level === "ok" && finding.message.includes("sscsb present (sscsb 0.1.0)"))).toBe(true);
    const deep = findings.find((finding) => finding.level === "info" && finding.message === SSCSB_DEEP_LAYER);
    expect(deep).toBeDefined();
    expect(deep!.message).toContain("`sscsb init`");
    expect(deep!.message).toContain("`sscsb verify`");
    expect(deep!.message).toContain("SBOM");
  });

  test("sscsb absent → detect carries degraded-level install guidance (cargo install / release binary)", async () => {
    const findings = await supplyChainModule.detect(makeTestCtx(dir));
    const degraded = findings.find((finding) => finding.level === "degraded" && finding.message.includes("sscsb"));
    expect(degraded).toBeDefined();
    expect(degraded!.remediation).toBe(SSCSB_INSTALL);
    expect(degraded!.remediation).toContain("cargo install --git https://github.com/p4gs/sscs-bootstrapper");
    // Guidance only — detect never runs sscsb and no deep-layer message appears.
    expect(findings.some((finding) => finding.message === SSCSB_DEEP_LAYER)).toBe(false);
  });

  test("sscsb-initialized repo (.sscsb/config.toml) → detect and verify report it as present/ok", async () => {
    const ctx = makeTestCtx(dir, { presentTools: { "osv-scanner": "2.0.0", sscsb: "sscsb 0.1.0" } });
    await Bun.write(join(dir, SSCSB_CONFIG_PATH), "[sscsb]\nversion = 1\n");
    const detected = await supplyChainModule.detect(ctx);
    expect(detected.some((finding) => finding.level === "ok" && finding.message.includes(SSCSB_CONFIG_PATH))).toBe(true);

    await supplyChainModule.apply(ctx);
    const verdict = await supplyChainModule.verify(ctx);
    expect(verdict.ok).toBe(true);
    expect(verdict.findings.some((finding) => finding.level === "ok" && finding.message.includes(SSCSB_CONFIG_PATH))).toBe(true);
  });

  test("sscsb installed but repo not initialized → verify stays ok with info guidance; neither state fails verify", async () => {
    const ctx = makeTestCtx(dir, { presentTools: { "osv-scanner": "2.0.0", sscsb: "sscsb 0.1.0" } });
    await supplyChainModule.apply(ctx);
    const verdict = await supplyChainModule.verify(ctx);
    expect(verdict.ok).toBe(true);
    const info = verdict.findings.find((finding) => finding.level === "info" && finding.message.includes("not sscsb-initialized"));
    expect(info).toBeDefined();
    expect(info!.remediation).toContain("sscsb init");

    // Tool absent + repo uninitialized → verify adds no sscsb finding at all and stays ok.
    const absentCtx = makeTestCtx(dir, { presentTools: { "osv-scanner": "2.0.0" } });
    const absentVerdict = await supplyChainModule.verify(absentCtx);
    expect(absentVerdict.ok).toBe(true);
    expect(absentVerdict.findings.some((finding) => finding.message.toLowerCase().includes("sscsb"))).toBe(false);
  });

  test("sscsb apply integration is guidance-only: findings surface, status still driven by osv-scanner, nothing executed", async () => {
    const result = await supplyChainModule.apply(
      makeTestCtx(dir, { presentTools: { "osv-scanner": "2.0.0", sscsb: "sscsb 0.1.0" } }),
    );
    expect(result.status).toBe("applied");
    expect(result.findings.some((finding) => finding.message === SSCSB_DEEP_LAYER)).toBe(true);
    // Detection + guidance only — apply never runs `sscsb init` (no .sscsb tree appears).
    expect(await Bun.file(join(dir, SSCSB_CONFIG_PATH)).exists()).toBe(false);
    expect(result.wrotePaths).toEqual([DEPENDENCIES_POLICY_PATH]);
  });

  test("ISC-65: osv-scanner absent → apply still writes policy and returns degraded", async () => {
    const result = await supplyChainModule.apply(makeTestCtx(dir));
    expect(result.status).toBe("degraded");
    expect(result.wrotePaths).toContain(DEPENDENCIES_POLICY_PATH);
    expect(await Bun.file(join(dir, DEPENDENCIES_POLICY_PATH)).exists()).toBe(true);
    expect(result.findings.some((finding) => finding.remediation?.includes("osv-scanner"))).toBe(true);
  });

  test("ISC-65: osv-scanner present → apply returns applied", async () => {
    const result = await supplyChainModule.apply(
      makeTestCtx(dir, { presentTools: { "osv-scanner": "osv-scanner 2.0.0" } }),
    );
    expect(result.status).toBe("applied");
  });

  test("ISC-66: missing lockfiles per ecosystem flagged as warn findings", async () => {
    await Bun.write(join(dir, "package.json"), "{}\n");
    await Bun.write(join(dir, "Cargo.toml"), "[package]\n");
    await Bun.write(join(dir, "go.mod"), "module example.com/m\n");
    await Bun.write(join(dir, "pyproject.toml"), "[project]\n");
    const findings = await checkLockfiles(dir);
    const warns = findings.filter((finding) => finding.level === "warn");
    expect(warns.length).toBe(4);
    expect(warns.some((finding) => finding.message.includes("package.json"))).toBe(true);
    expect(warns.some((finding) => finding.message.includes("Cargo.toml"))).toBe(true);
    expect(warns.some((finding) => finding.message.includes("go.mod"))).toBe(true);
    expect(warns.some((finding) => finding.message.includes("python"))).toBe(true);
  });

  test("ISC-66: any accepted lockfile satisfies its ecosystem", async () => {
    await Bun.write(join(dir, "package.json"), "{}\n");
    await Bun.write(join(dir, "pnpm-lock.yaml"), "lockfileVersion: 9\n");
    await Bun.write(join(dir, "Cargo.toml"), "[package]\n");
    await Bun.write(join(dir, "Cargo.lock"), "version = 4\n");
    await Bun.write(join(dir, "go.mod"), "module example.com/m\n");
    await Bun.write(join(dir, "go.sum"), "example.com/dep v1.0.0 h1:abc=\n");
    const findings = await checkLockfiles(dir);
    expect(findings.filter((finding) => finding.level === "warn").length).toBe(0);
    expect(findings.filter((finding) => finding.level === "ok").length).toBe(3);
  });

  test("ISC-66: requirements.txt counts as pinned only when it contains '=='", async () => {
    await Bun.write(join(dir, "requirements.txt"), "flask>=2.0\nrequests\n");
    const unpinned = await checkLockfiles(dir);
    expect(unpinned.some((finding) => finding.level === "warn" && finding.message.includes("python"))).toBe(true);

    await Bun.write(join(dir, "requirements.txt"), "flask==2.3.0\nrequests==2.31.0\n");
    const pinned = await checkLockfiles(dir);
    expect(pinned.filter((finding) => finding.level === "warn").length).toBe(0);
    expect(pinned.some((finding) => finding.level === "ok")).toBe(true);
  });

  test("ISC-66: uv.lock or poetry.lock satisfies an unpinned python manifest", async () => {
    await Bun.write(join(dir, "pyproject.toml"), "[project]\n");
    await Bun.write(join(dir, "uv.lock"), "version = 1\n");
    const findings = await checkLockfiles(dir);
    expect(findings.filter((finding) => finding.level === "warn").length).toBe(0);
  });

  test("ISC-66: no manifests → no lockfile findings at all", async () => {
    expect(await checkLockfiles(dir)).toEqual([]);
  });

  test("ISC-66: apply surfaces lockfile warns in its findings", async () => {
    await Bun.write(join(dir, "package.json"), "{}\n");
    const result = await supplyChainModule.apply(
      makeTestCtx(dir, { presentTools: { "osv-scanner": "2.0.0" } }),
    );
    expect(
      result.findings.some(
        (finding) => finding.level === "warn" && finding.message.includes("package.json"),
      ),
    ).toBe(true);
  });

  test("ISC-67: instruction block mandates policy check, min-age, exact-name verification, human approval, and hallucination warning", () => {
    expect(supplyChainModule.instructionBlocks.length).toBeGreaterThan(0);
    const content = supplyChainModule.instructionBlocks[0]!.content;
    expect(content).toContain(".ade/policy/dependencies.json");
    expect(content).toContain("NEVER install a dependency without checking");
    expect(content).toContain("minimum-age check");
    expect(content).toContain("exact-name verification");
    expect(content.toLowerCase()).toContain("human approval");
    expect(content.toLowerCase()).toContain("hallucination-prone");
  });

  test("ISC-116: plan writes nothing to disk", async () => {
    const actions = await supplyChainModule.plan(makeTestCtx(dir));
    expect(actions.length).toBeGreaterThan(0);
    expect(actions.some((action) => action.kind === "write" && action.path === DEPENDENCIES_POLICY_PATH)).toBe(true);
    expect(await Bun.file(join(dir, DEPENDENCIES_POLICY_PATH)).exists()).toBe(false);
  });

  test("ISC-117: apply is idempotent — second run byte-identical", async () => {
    await supplyChainModule.apply(makeTestCtx(dir, { presentTools: { "osv-scanner": "2.0.0" } }));
    const first = sha256(await Bun.file(join(dir, DEPENDENCIES_POLICY_PATH)).text());
    await supplyChainModule.apply(makeTestCtx(dir, { presentTools: { "osv-scanner": "2.0.0" } }));
    const second = sha256(await Bun.file(join(dir, DEPENDENCIES_POLICY_PATH)).text());
    expect(second).toBe(first);
  });

  test("verify passes after apply; fails on missing/invalid file, gutted traditional section, or missing aiNativeDependencies kind", async () => {
    const ctx = makeTestCtx(dir, { presentTools: { "osv-scanner": "2.0.0" } });
    expect((await supplyChainModule.verify(ctx)).ok).toBe(false);

    await supplyChainModule.apply(ctx);
    expect((await supplyChainModule.verify(ctx)).ok).toBe(true);

    const good = await readPolicy();

    await Bun.write(join(dir, DEPENDENCIES_POLICY_PATH), "{not json");
    expect((await supplyChainModule.verify(ctx)).ok).toBe(false);

    const noTraditional = { ...good, registries: { allowlist: [] } };
    await Bun.write(join(dir, DEPENDENCIES_POLICY_PATH), JSON.stringify(noTraditional));
    const traditionalResult = await supplyChainModule.verify(ctx);
    expect(traditionalResult.ok).toBe(false);
    expect(traditionalResult.findings.some((finding) => finding.message.includes("traditional"))).toBe(true);

    const droppedKind = {
      ...good,
      aiNativeDependencies: { ...good["aiNativeDependencies"] },
    } as Record<string, any>;
    delete droppedKind["aiNativeDependencies"].mcpServers;
    await Bun.write(join(dir, DEPENDENCIES_POLICY_PATH), JSON.stringify(droppedKind));
    const aiResult = await supplyChainModule.verify(ctx);
    expect(aiResult.ok).toBe(false);
    expect(aiResult.findings.some((finding) => finding.message.includes("aiNativeDependencies"))).toBe(true);
  });
});
