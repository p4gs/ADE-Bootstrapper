import { afterEach, beforeEach, describe, expect, test } from "bun:test";
import { join } from "node:path";
import {
  reproducibilityModule,
  probeVersion,
  MANIFEST_PATH,
} from "../../src/modules/reproducibility.ts";
import { makeTempDir, makeTestCtx, removeDir, fakeExec } from "../helpers.ts";
import { sha256 } from "../../src/fsutil.ts";

let dir: string;

beforeEach(async () => {
  dir = await makeTempDir();
});

afterEach(async () => {
  await removeDir(dir);
});

async function readManifest(): Promise<Record<string, any>> {
  return JSON.parse(await Bun.file(join(dir, MANIFEST_PATH)).text());
}

describe("reproducibility module", () => {
  test("ISC-107: apply writes manifest with os/arch from ctx, tools from detection, and core bun+git versions", async () => {
    const ctx = makeTestCtx(dir, {
      presentTools: { trufflehog: "3.90.0" },
      exec: fakeExec({
        "bun --version": { stdout: "1.2.0\n" },
        "git --version": { stdout: "git version 2.44.0\nbuilt from source\n" },
      }),
    });
    const result = await reproducibilityModule.apply(ctx);
    expect(result.status).toBe("applied");
    expect(result.wrotePaths).toContain(MANIFEST_PATH);
    const manifest = await readManifest();
    expect(manifest.os).toBe("test-os");
    expect(manifest.arch).toBe("test-arch");
    expect(manifest.tools.trufflehog).toBe("3.90.0");
    expect(manifest.tools.gitleaks).toBe(null);
    // first line only, trimmed — multi-line --version output is truncated
    expect(manifest.core.bun).toBe("1.2.0");
    expect(manifest.core.git).toBe("git version 2.44.0");
  });

  test("ISC-107: core tool --version failure is tolerated and recorded as null", async () => {
    const ctx = makeTestCtx(dir, {
      exec: fakeExec({
        "bun --version": { stdout: "1.2.0\n" },
        "git --version": { code: 1, stderr: "boom" },
      }),
    });
    await reproducibilityModule.apply(ctx);
    const manifest = await readManifest();
    expect(manifest.core.bun).toBe("1.2.0");
    expect(manifest.core.git).toBe(null);
  });

  test("ISC-108: everything missing — all tools absent + exec always failing → valid manifest of nulls, status applied", async () => {
    const ctx = makeTestCtx(dir); // no presentTools, default fakeExec → 127 for everything, which → null
    const result = await reproducibilityModule.apply(ctx);
    expect(result.status).toBe("applied");
    const manifest = await readManifest();
    expect(manifest.os).toBe("test-os");
    expect(manifest.arch).toBe("test-arch");
    expect(Object.values(manifest.core).every((value) => value === null)).toBe(true);
    expect(Object.values(manifest.tools).every((value) => value === null)).toBe(true);
    expect(Object.values(manifest.harnesses).every((value) => value === null)).toBe(true);
  });

  test("ISC-108: exec that THROWS never propagates — manifest still written, applied", async () => {
    const ctx = makeTestCtx(dir, {
      exec: async () => {
        throw new Error("spawn refused");
      },
    });
    const result = await reproducibilityModule.apply(ctx);
    expect(result.status).toBe("applied");
    const manifest = await readManifest();
    expect(manifest.core.bun).toBe(null);
    expect(manifest.core.git).toBe(null);
  });

  test("ISC-110: harness CLI hit via which → --version recorded under adapter id; absent CLIs recorded as null", async () => {
    const ctx = makeTestCtx(dir, {
      presentTools: { claude: "present" }, // makes which("claude") resolve
      exec: fakeExec({
        "claude --version": { stdout: "2.1.0 (Claude Code)\n" },
      }),
    });
    await reproducibilityModule.apply(ctx);
    const manifest = await readManifest();
    expect(manifest.harnesses["claude-code"]).toBe("2.1.0 (Claude Code)");
    expect(manifest.harnesses["codex"]).toBe(null);
    expect(manifest.harnesses["cursor"]).toBe(null);
  });

  test("ISC-110: harness CLI present but --version fails → recorded as null, not an error", async () => {
    const ctx = makeTestCtx(dir, {
      presentTools: { codex: "present" },
      // default responses: codex --version unmatched → exit 127
      exec: fakeExec({}),
    });
    const result = await reproducibilityModule.apply(ctx);
    expect(result.status).toBe("applied");
    const manifest = await readManifest();
    expect(manifest.harnesses["codex"]).toBe(null);
  });

  test("ISC-109: verify reports version drift vs stored manifest as INFO findings, never a failure", async () => {
    const applyCtx = makeTestCtx(dir, { presentTools: { trufflehog: "3.90.0" } });
    await reproducibilityModule.apply(applyCtx);

    // Same environment → no drift findings.
    const clean = await reproducibilityModule.verify(
      makeTestCtx(dir, { presentTools: { trufflehog: "3.90.0" } }),
    );
    expect(clean.ok).toBe(true);
    expect(clean.findings.some((finding) => finding.message.includes("drift"))).toBe(false);

    // Upgraded tool → drift surfaced as info, verify still passes.
    const drifted = await reproducibilityModule.verify(
      makeTestCtx(dir, { presentTools: { trufflehog: "3.91.0" } }),
    );
    expect(drifted.ok).toBe(true);
    const drift = drifted.findings.find((finding) => finding.message.includes("drift"));
    expect(drift?.level).toBe("info");
    expect(drift?.message).toContain("trufflehog");
    expect(drift?.message).toContain("3.90.0");
    expect(drift?.message).toContain("3.91.0");

    // Tool disappeared entirely → also informational drift.
    const removed = await reproducibilityModule.verify(makeTestCtx(dir));
    expect(removed.ok).toBe(true);
    expect(
      removed.findings.some(
        (finding) => finding.level === "info" && finding.message.includes("trufflehog"),
      ),
    ).toBe(true);
  });

  test("ISC-109: manifest missing or corrupt → verify error", async () => {
    const ctx = makeTestCtx(dir);
    const missing = await reproducibilityModule.verify(ctx);
    expect(missing.ok).toBe(false);
    expect(missing.findings.some((finding) => finding.level === "error")).toBe(true);

    await reproducibilityModule.apply(ctx);
    expect((await reproducibilityModule.verify(ctx)).ok).toBe(true);

    await Bun.write(join(dir, MANIFEST_PATH), "{not json");
    expect((await reproducibilityModule.verify(ctx)).ok).toBe(false);
  });

  test("verify fails when manifest parses but lacks os/arch/tools keys", async () => {
    await Bun.write(join(dir, MANIFEST_PATH), JSON.stringify({ os: "test-os" }));
    const result = await reproducibilityModule.verify(makeTestCtx(dir));
    expect(result.ok).toBe(false);
    expect(result.findings.some((finding) => finding.message.includes("os, arch, tools"))).toBe(true);
  });

  test("ISC-116: plan writes nothing to disk", async () => {
    const ctx = makeTestCtx(dir);
    const actions = await reproducibilityModule.plan(ctx);
    expect(actions.length).toBeGreaterThan(0);
    expect(actions[0]!.path).toBe(MANIFEST_PATH);
    expect(await Bun.file(join(dir, MANIFEST_PATH)).exists()).toBe(false);
  });

  test("ISC-117: apply is idempotent — second run byte-identical", async () => {
    const exec = fakeExec({
      "bun --version": { stdout: "1.2.0\n" },
      "git --version": { stdout: "git version 2.44.0\n" },
    });
    await reproducibilityModule.apply(
      makeTestCtx(dir, { presentTools: { trufflehog: "3.90.0" }, exec }),
    );
    const first = sha256(await Bun.file(join(dir, MANIFEST_PATH)).text());
    await reproducibilityModule.apply(
      makeTestCtx(dir, { presentTools: { trufflehog: "3.90.0" }, exec }),
    );
    const second = sha256(await Bun.file(join(dir, MANIFEST_PATH)).text());
    expect(second).toBe(first);
  });

  test("manifest contains no timestamps or absolute paths", async () => {
    const ctx = makeTestCtx(dir, { presentTools: { trufflehog: "3.90.0" } });
    await reproducibilityModule.apply(ctx);
    const text = await Bun.file(join(dir, MANIFEST_PATH)).text();
    expect(text).not.toContain(dir);
    expect(text).not.toContain("/fake/bin");
    expect(text).not.toMatch(/\d{4}-\d{2}-\d{2}T/);
  });

  test("instruction block points harnesses at the manifest and mandates drift reporting", () => {
    expect(reproducibilityModule.instructionBlocks.length).toBeGreaterThan(0);
    const block = reproducibilityModule.instructionBlocks[0]!;
    expect(block.content).toContain(".ade/manifest.json");
    expect(block.content.toLowerCase()).toContain("drift");
  });

  test("detect reports environment facts without degradation when tools are absent", async () => {
    const findings = await reproducibilityModule.detect(makeTestCtx(dir));
    expect(findings.some((finding) => finding.message.includes("test-os/test-arch"))).toBe(true);
    expect(findings.every((finding) => finding.level === "ok" || finding.level === "info")).toBe(true);
    expect(await Bun.file(join(dir, MANIFEST_PATH)).exists()).toBe(false);
  });

  test("probeVersion returns null on empty stdout and first trimmed line on success", async () => {
    const emptyCtx = makeTestCtx(dir, { exec: fakeExec({ "bun --version": { stdout: "\n" } }) });
    expect(await probeVersion(emptyCtx, "bun")).toBe(null);
    const okCtx = makeTestCtx(dir, { exec: fakeExec({ "bun --version": { stdout: "  1.2.0  \nextra" } }) });
    expect(await probeVersion(okCtx, "bun")).toBe("1.2.0");
  });
});
