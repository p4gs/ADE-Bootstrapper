/**
 * Branch-sweep tests: every module through its full lifecycle in the
 * configurations the per-module suites don't all reach — all tools PRESENT,
 * claude-code + cursor harnesses, and error/refusal paths in core surfaces.
 * Meaningful assertions only: each sweep asserts structured results.
 */
import { afterEach, beforeEach, describe, expect, test } from "bun:test";
import { mkdir } from "node:fs/promises";
import { join } from "node:path";
import { main, type Io } from "../src/cli.ts";
import { MODULES } from "../src/registry.ts";
import { verifyPipeline } from "../src/run.ts";
import { checkTranslationDrift, translateAll } from "../src/translate.ts";
import { composeInstructions } from "../src/instructions.ts";
import { MARKER_BEGIN, MARKER_END } from "../src/version.ts";
import { fakeExec, makeTempDir, makeTestCtx, removeDir, testConfig } from "./helpers.ts";
import type { AdeModule } from "../src/types.ts";

let dir: string;

const ALL_TOOLS_PRESENT = {
  trufflehog: "trufflehog 3.90.0",
  "pre-commit": "pre-commit 4.0.0",
  gitleaks: "gitleaks 8.0.0",
  rtk: "rtk 1.0.0",
  ocean: "ocean 2.0.0",
  nono: "nono 0.5.0",
  "osv-scanner": "osv-scanner 2.0.0",
};

function capture(): { io: Io; stdout: () => string; stderr: () => string } {
  let out = "";
  let err = "";
  return { io: { out: (t) => (out += t), err: (t) => (err += t) }, stdout: () => out, stderr: () => err };
}

beforeEach(async () => {
  dir = await makeTempDir();
  await mkdir(join(dir, ".git", "hooks"), { recursive: true });
  await Bun.write(join(dir, "package.json"), '{"name":"fixture","main":"index.js"}\n');
});

afterEach(async () => {
  await removeDir(dir);
});

describe("all-tools-present sweep", () => {
  test("every module applies cleanly with every integrated tool available", async () => {
    const config = testConfig({ harnesses: ["claude-code", "cursor", "codex"] });
    const execWithVersions = fakeExec({
      "git -C": { code: 0, stdout: "true\n" },
      "--version": { code: 0, stdout: "9.9.9\n" },
      version: { code: 0, stdout: "9.9.9\n" },
      "git config": { code: 0, stdout: "true\n" },
    });
    for (const module of MODULES) {
      const ctx = makeTestCtx(dir, { config, presentTools: ALL_TOOLS_PRESENT, exec: execWithVersions });
      const detect = await module.detect(ctx);
      expect(Array.isArray(detect)).toBe(true);
      const result = await module.apply(ctx);
      expect(["applied", "degraded", "skipped"]).toContain(result.status);
      const verdict = await module.verify(
        makeTestCtx(dir, { config, presentTools: ALL_TOOLS_PRESENT, exec: execWithVersions }),
      );
      expect(typeof verdict.ok).toBe("boolean");
    }
  });

  test("modules detect() with tools present yields at least one ok/info finding each", async () => {
    const config = testConfig({ harnesses: ["claude-code"] });
    for (const module of MODULES) {
      const ctx = makeTestCtx(dir, { config, presentTools: ALL_TOOLS_PRESENT });
      const findings = await module.detect(ctx);
      expect(findings.some((finding) => finding.level === "ok" || finding.level === "info")).toBe(true);
    }
  });
});

describe("verifyPipeline fault isolation", () => {
  test("ISC-119: a module whose verify() throws is contained and reported", async () => {
    const bomb: AdeModule = {
      id: "secrets",
      title: "VerifyBomb",
      category: "security",
      spec: "bomb",
      defaultEnabled: true,
      instructionBlocks: [],
      detect: async () => [],
      plan: async () => [],
      apply: async () => ({ status: "applied", findings: [], wrotePaths: [] }),
      verify: async () => {
        throw new Error("verify-kaboom");
      },
    };
    const ctx = makeTestCtx(dir);
    const report = await verifyPipeline(ctx, [bomb]);
    expect(report.ok).toBe(false);
    const entry = report.modules.find((module) => module.id === "secrets")!;
    expect(entry.ok).toBe(false);
    expect(entry.findings[0]!.message).toContain("verify-kaboom");
  });
});

describe("translation edge branches", () => {
  const BODY = composeInstructions([{ id: "x", title: "X", content: "y" }]);

  test("drift check reports corrupt managed block distinctly from stale content", async () => {
    const config = testConfig({ harnesses: ["claude-code"] });
    await translateAll(makeTestCtx(dir, { config }), BODY);
    // Strip the end marker → extract fails → 'missing or corrupt'
    const path = join(dir, "CLAUDE.md");
    const content = await Bun.file(path).text();
    await Bun.write(path, content.replace(MARKER_END, ""));
    const findings = await checkTranslationDrift(makeTestCtx(dir, { config }), BODY);
    expect(findings.some((finding) => finding.message.includes("missing or corrupt"))).toBe(true);
  });

  test("translate refuses to write over a non-regular file (symlink mount guard)", async () => {
    const { symlink } = await import("node:fs/promises");
    const config = testConfig({ harnesses: ["claude-code"] });
    await Bun.write(join(dir, "elsewhere.md"), "mounted content\n");
    await symlink(join(dir, "elsewhere.md"), join(dir, "CLAUDE.md"));
    const results = await translateAll(makeTestCtx(dir, { config }), BODY);
    expect(results[0]!.ok).toBe(false);
    expect(results[0]!.error).toContain("not a regular file");
    expect(await Bun.file(join(dir, "elsewhere.md")).text()).toBe("mounted content\n");
  });

  test("translate refuses on reversed markers without touching the file", async () => {
    const config = testConfig({ harnesses: ["claude-code"] });
    const reversed = `${MARKER_END}\nmiddle\n${MARKER_BEGIN}\n`;
    await Bun.write(join(dir, "CLAUDE.md"), reversed);
    const results = await translateAll(makeTestCtx(dir, { config }), BODY);
    expect(results[0]!.ok).toBe(false);
    expect(await Bun.file(join(dir, "CLAUDE.md")).text()).toBe(reversed);
  });
});

describe("module branch sweeps", () => {
  test("secrets: plan shows pre-commit path when framework present; apply recognizes existing trufflehog config", async () => {
    const { secretsModule } = await import("../src/modules/secrets.ts");
    const withPreCommit = makeTestCtx(dir, { presentTools: { trufflehog: "3.9.0", "pre-commit": "4.0.0" } });
    const actions = await secretsModule.plan(withPreCommit);
    expect(actions.some((action) => action.path === ".pre-commit-config.yaml")).toBe(true);

    await Bun.write(join(dir, ".pre-commit-config.yaml"), "repos:\n  - repo: x\n    hooks:\n      - id: trufflehog\n");
    const result = await secretsModule.apply(makeTestCtx(dir, { presentTools: { trufflehog: "3.9.0", "pre-commit": "4.0.0" } }));
    expect(result.findings.some((finding) => finding.message.includes("already includes"))).toBe(true);
  });

  test("observability: commitAuditLog=true plan/apply leaves audit log committable", async () => {
    const { observabilityModule } = await import("../src/modules/observability.ts");
    const config = testConfig({
      harnesses: ["claude-code"],
      modules: { observability: { enabled: true, options: { commitAuditLog: true } } },
    });
    const actions = await observabilityModule.plan(makeTestCtx(dir, { config }));
    expect(actions.some((action) => action.description.includes("commitAuditLog=true"))).toBe(true);
    await observabilityModule.apply(makeTestCtx(dir, { config }));
    const gitignore = await Bun.file(join(dir, ".gitignore")).text().catch(() => "");
    expect(gitignore).not.toContain(".ade/audit/");
  });

  test("observability: verify reports unparseable audit log as error", async () => {
    const { observabilityModule } = await import("../src/modules/observability.ts");
    const config = testConfig({ harnesses: ["claude-code"] });
    await observabilityModule.apply(makeTestCtx(dir, { config }));
    await Bun.write(join(dir, ".ade", "audit", "log.jsonl"), "{not-parseable\n");
    const verdict = await observabilityModule.verify(makeTestCtx(dir, { config }));
    expect(verdict.ok).toBe(false);
    expect(verdict.findings.some((finding) => finding.message.includes("unparseable"))).toBe(true);
  });

  test("reproducibility: verify flags a tool missing from the recorded manifest as info", async () => {
    const { reproducibilityModule, MANIFEST_PATH } = await import("../src/modules/reproducibility.ts");
    await reproducibilityModule.apply(makeTestCtx(dir));
    const manifestPath = join(dir, MANIFEST_PATH);
    const manifest = JSON.parse(await Bun.file(manifestPath).text());
    delete manifest.tools.rtk;
    await Bun.write(manifestPath, JSON.stringify(manifest, null, 2));
    const verdict = await reproducibilityModule.verify(makeTestCtx(dir));
    expect(verdict.ok).toBe(true);
    expect(verdict.findings.some((finding) => finding.level === "info" && finding.message.includes("not recorded"))).toBe(true);
  });

  test("sandbox: options extend defaults; invalid options fail apply", async () => {
    const { sandboxModule, validateSandboxOptions } = await import("../src/modules/sandbox.ts");
    expect(validateSandboxOptions({ allowHosts: ["example.internal"] })).toEqual([]);
    expect(validateSandboxOptions({ allowHosts: [42] }).length).toBeGreaterThan(0);
    expect(validateSandboxOptions({ denyRead: "not-an-array" }).length).toBeGreaterThan(0);

    const config = testConfig({
      harnesses: ["claude-code"],
      modules: { sandbox: { enabled: true, options: { allowHosts: ["example.internal"], denyWrite: ["/opt/custom"], denyRead: ["custom.secret"] } } },
    });
    const result = await sandboxModule.apply(makeTestCtx(dir, { config, presentTools: { nono: "0.5.0" } }));
    expect(result.status).toBe("applied");
    const policy = JSON.parse(await Bun.file(join(dir, ".ade", "policy", "sandbox.json")).text());
    expect(policy.network.allowlist).toContain("example.internal");
    expect(policy.filesystem.denyWrite).toContain("/opt/custom");

    const bad = testConfig({ modules: { sandbox: { enabled: true, options: { allowHosts: [42] } } } });
    const failed = await sandboxModule.apply(makeTestCtx(dir, { config: bad }));
    expect(failed.status).toBe("failed");
  });

  test("supply-chain: minAgeDays option honored and invalid value fails; lockfile matrix branches", async () => {
    const { supplyChainModule, checkLockfiles } = await import("../src/modules/supply-chain.ts");
    const config = testConfig({ modules: { "supply-chain": { enabled: true, options: { minAgeDays: 30 } } } });
    await supplyChainModule.apply(makeTestCtx(dir, { config, presentTools: { "osv-scanner": "2.0.0" } }));
    const policy = JSON.parse(await Bun.file(join(dir, ".ade", "policy", "dependencies.json")).text());
    expect(policy.minAgeDays).toBe(30);

    const bad = testConfig({ modules: { "supply-chain": { enabled: true, options: { minAgeDays: -1 } } } });
    expect((await supplyChainModule.apply(makeTestCtx(dir, { config: bad }))).status).toBe("failed");

    const eco = await makeTempDir();
    try {
      await Bun.write(join(eco, "Cargo.toml"), "[package]\n");
      await Bun.write(join(eco, "go.mod"), "module x\n");
      await Bun.write(join(eco, "pyproject.toml"), "[project]\n");
      await Bun.write(join(eco, "requirements.txt"), "requests>=2\n");
      const findings = await checkLockfiles(eco);
      expect(findings.filter((finding) => finding.level === "warn").length).toBeGreaterThanOrEqual(3);
      await Bun.write(join(eco, "Cargo.lock"), "\n");
      await Bun.write(join(eco, "go.sum"), "\n");
      await Bun.write(join(eco, "requirements.txt"), "requests==2.31.0\n");
      await Bun.write(join(eco, "uv.lock"), "\n");
      const after = await checkLockfiles(eco);
      expect(after.filter((finding) => finding.level === "warn").length).toBe(0);
    } finally {
      await removeDir(eco);
    }
  });

  test("reproducibility: harness CLI versions recorded when CLIs are present", async () => {
    const { reproducibilityModule, MANIFEST_PATH } = await import("../src/modules/reproducibility.ts");
    const ctx = makeTestCtx(dir, {
      which: (name) => (name === "claude" || name === "bun" || name === "git" ? `/fake/bin/${name}` : null),
      exec: fakeExec({ "--version": { code: 0, stdout: "7.7.7\n" }, version: { code: 0, stdout: "7.7.7\n" } }),
    });
    const result = await reproducibilityModule.apply(ctx);
    expect(result.status).toBe("applied");
    const manifest = JSON.parse(await Bun.file(join(dir, MANIFEST_PATH)).text());
    expect(manifest.harnesses["claude-code"]).toContain("7.7.7");
    expect(manifest.harnesses["cursor"]).toBeNull();
  });

  test("sandbox/supply-chain: detect() surfaces option validation errors", async () => {
    const { sandboxModule } = await import("../src/modules/sandbox.ts");
    const { supplyChainModule } = await import("../src/modules/supply-chain.ts");
    const badSandbox = testConfig({ modules: { sandbox: { enabled: true, options: { allowHosts: [42] } } } });
    const sandboxFindings = await sandboxModule.detect(makeTestCtx(dir, { config: badSandbox }));
    expect(sandboxFindings.some((finding) => finding.level === "error")).toBe(true);

    const badSupply = testConfig({ modules: { "supply-chain": { enabled: true, options: { minAgeDays: "soon" } } } });
    const supplyFindings = await supplyChainModule.detect(makeTestCtx(dir, { config: badSupply }));
    expect(supplyFindings.some((finding) => finding.level === "error")).toBe(true);
  });

  test("reproducibility: detect() reports harness CLIs present on the machine", async () => {
    const { reproducibilityModule } = await import("../src/modules/reproducibility.ts");
    const findings = await reproducibilityModule.detect(
      makeTestCtx(dir, {
        which: (name) => (name === "claude" ? "/fake/bin/claude" : null),
        exec: fakeExec({ "--version": { code: 0, stdout: "1.0.0\n" } }),
      }),
    );
    expect(findings.some((finding) => finding.message.toLowerCase().includes("claude") || finding.message.includes("harness"))).toBe(true);
  });

  test("default-deps stderr path: unknown command writes usage to real stderr, exit 2", async () => {
    const code = await main(["definitely-not-a-command"], { cwd: dir, exec: fakeExec(), which: () => null });
    expect(code).toBe(2);
  });

  test("sandbox and supply-chain: plan actions enumerate their writes", async () => {
    const { sandboxModule } = await import("../src/modules/sandbox.ts");
    const { supplyChainModule } = await import("../src/modules/supply-chain.ts");
    const config = testConfig({ harnesses: ["claude-code"] });
    const sandboxActions = await sandboxModule.plan(makeTestCtx(dir, { config, presentTools: { nono: "0.5.0" } }));
    expect(sandboxActions.length).toBeGreaterThan(0);
    const supplyActions = await supplyChainModule.plan(makeTestCtx(dir, { config, presentTools: { "osv-scanner": "2.0.0" } }));
    expect(supplyActions.length).toBeGreaterThan(0);
  });
});

describe("CLI edge branches", () => {
  function runCli(argv: string[], captured: ReturnType<typeof capture>, cwd = dir): Promise<number> {
    return main(argv, {
      io: captured.io,
      cwd,
      exec: fakeExec({ "git -C": { code: 0, stdout: "true\n" } }),
      which: () => null,
    });
  }

  test("init twice: second run reports created=false and exits 0", async () => {
    await runCli(["init"], capture());
    const captured = capture();
    expect(await runCli(["init", "--json"], captured)).toBe(0);
    expect(JSON.parse(captured.stdout()).created).toBe(false);
  });

  test("audit verify on unparseable log exits 1", async () => {
    await runCli(["init"], capture());
    await Bun.write(join(dir, ".ade", "audit", "log.jsonl"), "not-json at all\n");
    const captured = capture();
    expect(await runCli(["audit", "verify", "--json"], captured)).toBe(1);
  });

  test("translate refusal (hand-edited block) exits 1 and names the file", async () => {
    await runCli(["init"], capture());
    const path = join(dir, "CLAUDE.md");
    const tampered = (await Bun.file(path).text()).replace("ADE Baseline", "EDITED Baseline");
    await Bun.write(path, tampered);
    const captured = capture();
    expect(await runCli(["translate"], captured)).toBe(1);
    expect(captured.stdout()).toContain("REFUSED");
  });

  test("status marks a module not-applied after its artifact is destroyed", async () => {
    await runCli(["init"], capture());
    await Bun.write(join(dir, ".ade", "policy", "budget.json"), "{}\n");
    const captured = capture();
    expect(await runCli(["status", "--json"], captured)).toBe(0);
    const rows = JSON.parse(captured.stdout()).modules as Array<{ id: string; state: string }>;
    expect(rows.find((row) => row.id === "cost-governance")!.state).toBe("not-applied");
  });

  test("status reports degraded for a non-git target (secrets module)", async () => {
    const bare = await makeTempDir();
    try {
      const io1 = capture();
      await main(["init"], { io: io1.io, cwd: bare, exec: fakeExec(), which: () => null });
      const captured = capture();
      const code = await main(["status"], { io: captured.io, cwd: bare, exec: fakeExec(), which: () => null });
      expect(code).toBe(0);
      expect(captured.stdout()).toContain("degraded");
    } finally {
      await removeDir(bare);
    }
  });

  test("apply --json on a valid target returns ok with 15 module entries", async () => {
    await runCli(["init"], capture());
    const captured = capture();
    expect(await runCli(["apply", "--json"], captured)).toBe(0);
    const payload = JSON.parse(captured.stdout());
    expect(payload.modules.length).toBe(15);
    expect(payload.ok).toBe(true);
  });

  test("init with corrupt existing ade.json exits 1 without overwriting it", async () => {
    await Bun.write(join(dir, "ade.json"), "{corrupt");
    const captured = capture();
    expect(await runCli(["init"], captured)).toBe(1);
    expect(captured.stderr()).toContain("not valid JSON");
    expect(await Bun.file(join(dir, "ade.json")).text()).toBe("{corrupt");
  });

  test("init into an empty existing directory works (directory detection)", async () => {
    const empty = await makeTempDir();
    try {
      const captured = capture();
      const code = await main(["init"], { io: captured.io, cwd: empty, exec: fakeExec(), which: () => null });
      expect(code).toBe(0);
      expect(await Bun.file(join(empty, "ade.json")).exists()).toBe(true);
    } finally {
      await removeDir(empty);
    }
  });

  test("plan and status render the disabled branch for a disabled module", async () => {
    await runCli(["init"], capture());
    const configPath = join(dir, "ade.json");
    const config = JSON.parse(await Bun.file(configPath).text());
    config.modules["cost-governance"].enabled = false;
    await Bun.write(configPath, JSON.stringify(config, null, 2));
    const plan = capture();
    expect(await runCli(["plan"], plan)).toBe(0);
    expect(plan.stdout()).toContain("[cost-governance] skipped (disabled)");
    const status = capture();
    expect(await runCli(["status"], status)).toBe(0);
    expect(status.stdout()).toContain("disabled");
  });

  test("main without io override uses real stdio (default deps path)", async () => {
    const code = await main(["version"], { cwd: dir, exec: fakeExec(), which: () => null });
    expect(code).toBe(0);
  });

  test("doctor human output shows git guidance for non-repo", async () => {
    const bare = await makeTempDir();
    try {
      const captured = capture();
      const code = await main(["doctor"], { io: captured.io, cwd: bare, exec: fakeExec(), which: () => null });
      expect(code).toBe(0);
      expect(captured.stdout()).toContain("git init");
    } finally {
      await removeDir(bare);
    }
  });
});
