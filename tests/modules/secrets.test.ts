import { afterEach, beforeEach, describe, expect, test } from "bun:test";
import { mkdir } from "node:fs/promises";
import { join } from "node:path";
import {
  secretsModule,
  hookScript,
  preCommitConfig,
  HOOK_MARKER,
  CHAINED_HOOK_NAME,
  SECRETS_POLICY_PATH,
  PRECOMMIT_CONFIG_PATH,
} from "../../src/modules/secrets.ts";
import { makeTempDir, makeTestCtx, removeDir } from "../helpers.ts";

let dir: string;

beforeEach(async () => {
  dir = await makeTempDir();
  await mkdir(join(dir, ".git", "hooks"), { recursive: true });
});

afterEach(async () => {
  await removeDir(dir);
});

describe("secrets module", () => {
  test("ISC-93: trufflehog present → applied with native hook installed", async () => {
    const ctx = makeTestCtx(dir, { presentTools: { trufflehog: "trufflehog 3.90.0" } });
    const result = await secretsModule.apply(ctx);
    expect(result.status).toBe("applied");
    const hook = await Bun.file(join(dir, ".git", "hooks", "pre-commit")).text();
    expect(hook).toContain(HOOK_MARKER);
    expect(hook).toContain("trufflehog git");
  });

  test("ISC-93: trufflehog absent → degraded with install remediation", async () => {
    const ctx = makeTestCtx(dir);
    const result = await secretsModule.apply(ctx);
    expect(result.status).toBe("degraded");
    expect(result.findings.some((finding) => finding.remediation?.includes("brew install trufflehog"))).toBe(true);
  });

  test("ISC-94: pre-commit framework present → writes .pre-commit-config.yaml with trufflehog hook", async () => {
    const ctx = makeTestCtx(dir, {
      presentTools: { trufflehog: "3.90.0", "pre-commit": "pre-commit 4.0.0" },
    });
    const result = await secretsModule.apply(ctx);
    expect(result.status).toBe("applied");
    const config = await Bun.file(join(dir, PRECOMMIT_CONFIG_PATH)).text();
    expect(config).toContain("trufflehog");
    expect(await Bun.file(join(dir, ".git", "hooks", "pre-commit")).exists()).toBe(false);
  });

  test("ISC-94: existing user .pre-commit-config.yaml without trufflehog is NOT modified", async () => {
    const userConfig = "repos: []\n# user-owned\n";
    await Bun.write(join(dir, PRECOMMIT_CONFIG_PATH), userConfig);
    const ctx = makeTestCtx(dir, {
      presentTools: { trufflehog: "3.90.0", "pre-commit": "4.0.0" },
    });
    const result = await secretsModule.apply(ctx);
    expect(await Bun.file(join(dir, PRECOMMIT_CONFIG_PATH)).text()).toBe(userConfig);
    expect(result.findings.some((finding) => finding.level === "degraded")).toBe(true);
  });

  test("ISC-94: pre-existing non-ADE git hook is chained, not destroyed", async () => {
    const userHook = "#!/bin/sh\necho user-hook\n";
    await Bun.write(join(dir, ".git", "hooks", "pre-commit"), userHook);
    const ctx = makeTestCtx(dir, { presentTools: { trufflehog: "3.90.0" } });
    await secretsModule.apply(ctx);
    const preserved = await Bun.file(join(dir, ".git", "hooks", CHAINED_HOOK_NAME)).text();
    expect(preserved).toBe(userHook);
    const hook = await Bun.file(join(dir, ".git", "hooks", "pre-commit")).text();
    expect(hook).toContain(CHAINED_HOOK_NAME);
  });

  test("ISC-96: .gitignore gains secret-bearing paths, idempotently", async () => {
    await Bun.write(join(dir, ".gitignore"), "node_modules/\n");
    const ctx = makeTestCtx(dir, { presentTools: { trufflehog: "3.90.0" } });
    await secretsModule.apply(ctx);
    const first = await Bun.file(join(dir, ".gitignore")).text();
    expect(first).toContain("node_modules/");
    expect(first).toContain(".env");
    expect(first).toContain("*.pem");
    await secretsModule.apply(makeTestCtx(dir, { presentTools: { trufflehog: "3.90.0" } }));
    const second = await Bun.file(join(dir, ".gitignore")).text();
    expect(second).toBe(first);
  });

  test("ISC-97: instruction block forbids secret echoing and names scoped credentials", () => {
    const block = secretsModule.instructionBlocks[0]!;
    expect(block.content).toContain("NEVER");
    expect(block.content.toLowerCase()).toContain("rotate");
    expect(block.content.toLowerCase()).toContain("scoped");
  });

  test("ISC-98/ISC-123: planted env secret never appears in any generated artifact", async () => {
    const planted = "AKIA_PLANTED_TEST_SECRET_VALUE_12345";
    const ctx = makeTestCtx(dir, {
      presentTools: { trufflehog: "3.90.0" },
      env: { AWS_SECRET_ACCESS_KEY: planted },
    });
    await secretsModule.apply(ctx);
    for (const rel of [SECRETS_POLICY_PATH, ".gitignore", ".git/hooks/pre-commit"]) {
      const file = Bun.file(join(dir, rel));
      if (await file.exists()) {
        expect(await file.text()).not.toContain(planted);
      }
    }
  });

  test("ISC-15.1: non-git directory degrades without crashing and skips hook install", async () => {
    const bare = await makeTempDir();
    try {
      const ctx = makeTestCtx(bare, { isGitRepo: false, presentTools: { trufflehog: "3.90.0" } });
      const result = await secretsModule.apply(ctx);
      expect(result.status).toBe("degraded");
      expect(result.findings.some((finding) => finding.remediation?.includes("git init"))).toBe(true);
      expect(await Bun.file(join(bare, ".git", "hooks", "pre-commit")).exists()).toBe(false);
    } finally {
      await removeDir(bare);
    }
  });

  test("ISC-116: plan writes nothing", async () => {
    const ctx = makeTestCtx(dir, { presentTools: { trufflehog: "3.90.0" } });
    const actions = await secretsModule.plan(ctx);
    expect(actions.some((action) => action.kind === "hook")).toBe(true);
    expect(await Bun.file(join(dir, SECRETS_POLICY_PATH)).exists()).toBe(false);
    expect(await Bun.file(join(dir, ".git", "hooks", "pre-commit")).exists()).toBe(false);
  });

  test("verify: passes after apply, fails when hook removed", async () => {
    const ctx = makeTestCtx(dir, { presentTools: { trufflehog: "3.90.0" } });
    await secretsModule.apply(ctx);
    expect((await secretsModule.verify(ctx)).ok).toBe(true);
    await removeDir(join(dir, ".git", "hooks"));
    await mkdir(join(dir, ".git", "hooks"), { recursive: true });
    expect((await secretsModule.verify(ctx)).ok).toBe(false);
  });

  test("detect reports tool presence and non-git degradation", async () => {
    const findings = await secretsModule.detect(makeTestCtx(dir, { isGitRepo: false }));
    expect(findings.some((finding) => finding.message.includes("trufflehog"))).toBe(true);
    expect(findings.some((finding) => finding.level === "degraded" && finding.message.includes("git"))).toBe(true);
  });

  test("hook script warns-and-passes when trufflehog missing (never bricks commits)", () => {
    const script = hookScript();
    expect(script).toContain("command -v trufflehog");
    expect(script).toContain("exit 0");
    expect(script).toContain("BLOCKED");
    expect(preCommitConfig()).toContain("trufflesecurity/trufflehog");
  });
});
