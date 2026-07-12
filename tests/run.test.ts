import { afterEach, beforeEach, describe, expect, test } from "bun:test";
import { mkdir } from "node:fs/promises";
import { join } from "node:path";
import { parseLog, verifyChain } from "../src/audit.ts";
import { costGovernanceModule } from "../src/modules/cost-governance.ts";
import { secretsModule } from "../src/modules/secrets.ts";
import {
  applyPipeline,
  AUDIT_LOG_PATH,
  defaultHarnessTargets,
  initTarget,
  lockfileScope,
  planPipeline,
  verifyPipeline,
  type PipelineDeps,
} from "../src/run.ts";
import { fakeExec, makeTempDir, makeTestCtx, removeDir, testConfig } from "./helpers.ts";
import type { AdeModule } from "../src/types.ts";

let dir: string;

const deps: PipelineDeps = {
  exec: fakeExec(),
  which: () => null,
  log: () => {},
  now: () => "2026-07-12T00:00:00Z",
};

const PROBES: AdeModule[] = [secretsModule, costGovernanceModule];

function probeConfig() {
  const config = testConfig({ harnesses: ["claude-code", "codex"] });
  return config;
}

beforeEach(async () => {
  dir = await makeTempDir();
  await mkdir(join(dir, ".git", "hooks"), { recursive: true });
});

afterEach(async () => {
  await removeDir(dir);
});

describe("pipeline orchestration", () => {
  test("lockfileScope keeps .ade artifacts, drops audit log and user files", () => {
    expect(
      lockfileScope([".ade/policy/a.json", ".ade/audit/log.jsonl", "CLAUDE.md", ".ade/instructions.md", ".gitignore"]),
    ).toEqual([".ade/policy/a.json", ".ade/instructions.md"]);
  });

  test("defaultHarnessTargets: detected harnesses win, else opinionated pair", () => {
    expect(defaultHarnessTargets(["cursor"])).toEqual(["cursor"]);
    expect(defaultHarnessTargets([])).toEqual(["claude-code", "codex"]);
  });

  test("ISC-53/54: apply composes canonical instructions and translates to harness files", async () => {
    const ctx = makeTestCtx(dir, { config: probeConfig() });
    const report = await applyPipeline(ctx, deps, PROBES);
    expect(report.ok).toBe(true);
    const canonical = await Bun.file(join(dir, ".ade", "instructions.md")).text();
    expect(canonical).toContain("# ADE Baseline Instructions");
    expect(canonical).toContain("Secrets & Credential Hygiene");
    const claude = await Bun.file(join(dir, "CLAUDE.md")).text();
    expect(claude).toContain("<!-- ade:begin -->");
    const agents = await Bun.file(join(dir, "AGENTS.md")).text();
    expect(agents).toContain("Cost & Token Budget");
  });

  test("ISC-41: apply appends audit events and the chain verifies", async () => {
    const ctx = makeTestCtx(dir, { config: probeConfig() });
    const report = await applyPipeline(ctx, deps, PROBES);
    expect(report.auditAppended).toBeGreaterThan(0);
    const entries = parseLog(await Bun.file(join(dir, AUDIT_LOG_PATH)).text());
    expect(verifyChain(entries).valid).toBe(true);
    expect(entries.some((entry) => entry.action === "module.apply" && entry.target === "secrets")).toBe(true);
    expect(entries.some((entry) => entry.action === "lockfile.write")).toBe(true);
  });

  test("ISC-119: a throwing module is contained — run continues, module reported failed", async () => {
    const bomb: AdeModule = {
      id: "cost-governance",
      title: "Bomb",
      category: "governance",
      spec: "test bomb",
      defaultEnabled: true,
      instructionBlocks: [],
      detect: async () => [],
      plan: async () => [],
      apply: async () => {
        throw new Error("kaboom");
      },
      verify: async () => ({ ok: true, findings: [] }),
    };
    const ctx = makeTestCtx(dir, { config: probeConfig() });
    const report = await applyPipeline(ctx, deps, [bomb, secretsModule]);
    expect(report.ok).toBe(false);
    const bombReport = report.modules.find((module) => module.title === "Bomb")!;
    expect(bombReport.result.status).toBe("failed");
    expect(bombReport.result.findings[0]!.message).toContain("kaboom");
    const secretsReport = report.modules.find((module) => module.id === "secrets")!;
    expect(secretsReport.result.status).not.toBe("failed");
  });

  test("ISC-31: disabled module is skipped by apply", async () => {
    const config = probeConfig();
    config.modules["cost-governance"]!.enabled = false;
    const ctx = makeTestCtx(dir, { config });
    const report = await applyPipeline(ctx, deps, PROBES);
    const skipped = report.modules.find((module) => module.id === "cost-governance")!;
    expect(skipped.result.status).toBe("skipped");
    expect(await Bun.file(join(dir, ".ade", "policy", "budget.json")).exists()).toBe(false);
  });

  test("ISC-130/131: verify passes after apply and fails after tampering, naming the file", async () => {
    const ctx = makeTestCtx(dir, { config: probeConfig(), presentTools: { trufflehog: "3.0.0" } });
    await applyPipeline(ctx, deps, PROBES);
    const clean = await verifyPipeline(makeTestCtx(dir, { config: probeConfig(), presentTools: { trufflehog: "3.0.0" } }), PROBES);
    expect(clean.ok).toBe(true);

    await Bun.write(join(dir, ".ade", "policy", "budget.json"), '{"tampered":true}\n');
    const tampered = await verifyPipeline(makeTestCtx(dir, { config: probeConfig(), presentTools: { trufflehog: "3.0.0" } }), PROBES);
    expect(tampered.ok).toBe(false);
    expect(tampered.lockfile.some((finding) => finding.message.includes("budget.json"))).toBe(true);
  });

  test("verify fails with missing lockfile", async () => {
    const ctx = makeTestCtx(dir, { config: probeConfig() });
    const report = await verifyPipeline(ctx, PROBES);
    expect(report.ok).toBe(false);
    expect(report.lockfile[0]!.message).toContain("ade.lock.json");
  });

  test("ISC-17: double apply is a no-op for all generated content (audit surface excepted — apply is itself an audited event)", async () => {
    await applyPipeline(makeTestCtx(dir, { config: probeConfig() }), deps, PROBES);
    const lockFirst = JSON.parse(await Bun.file(join(dir, "ade.lock.json")).text());
    const claudeFirst = await Bun.file(join(dir, "CLAUDE.md")).text();
    await applyPipeline(makeTestCtx(dir, { config: probeConfig() }), deps, PROBES);
    const lockSecond = JSON.parse(await Bun.file(join(dir, "ade.lock.json")).text());

    // Content hashes, environment, and harnesses are byte-stable...
    expect(lockSecond.files).toEqual(lockFirst.files);
    expect(lockSecond.environment).toEqual(lockFirst.environment);
    expect(lockSecond.harnesses).toEqual(lockFirst.harnesses);
    expect(await Bun.file(join(dir, "CLAUDE.md")).text()).toBe(claudeFirst);

    // ...and the audit checkpoint legitimately advances, because the second apply
    // was itself logged. A chain that did NOT grow would mean apply went unaudited.
    expect(lockSecond.audit.length).toBeGreaterThan(lockFirst.audit.length);
    expect(lockSecond.audit.headHash).not.toBe(lockFirst.audit.headHash);
  });

  test("initTarget: creates default config once, reuses it after, refuses invalid", async () => {
    const first = await initTarget(dir, deps);
    expect("created" in first && first.created).toBe(true);
    const second = await initTarget(dir, deps);
    expect("created" in second && !second.created).toBe(true);

    const bad = await makeTempDir();
    try {
      await Bun.write(join(bad, "ade.json"), "{broken");
      const result = await initTarget(bad, deps);
      expect("error" in result).toBe(true);
    } finally {
      await removeDir(bad);
    }
  });

  test("plan reports module actions plus core actions and writes nothing", async () => {
    const ctx = makeTestCtx(dir, { config: probeConfig() });
    const report = await planPipeline(ctx, PROBES);
    expect(report.modules.length).toBe(2);
    expect(report.coreActions.some((action) => action.path === "ade.lock.json")).toBe(true);
    expect(await Bun.file(join(dir, ".ade", "instructions.md")).exists()).toBe(false);
  });
});
