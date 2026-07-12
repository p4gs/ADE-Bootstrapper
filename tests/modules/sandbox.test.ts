import { afterEach, beforeEach, describe, expect, test } from "bun:test";
import { join } from "node:path";
import {
  sandboxModule,
  validateSandboxOptions,
  SANDBOX_POLICY_PATH,
  CLAUDE_DENY_READ,
  DEFAULT_NETWORK_ALLOWLIST,
} from "../../src/modules/sandbox.ts";
import { CLAUDE_SETTINGS_PATH } from "../../src/harness/claude.ts";
import { makeTempDir, makeTestCtx, removeDir, testConfig } from "../helpers.ts";
import { sha256 } from "../../src/fsutil.ts";

let dir: string;

beforeEach(async () => {
  dir = await makeTempDir();
});

afterEach(async () => {
  await removeDir(dir);
});

async function readPolicy(): Promise<any> {
  return JSON.parse(await Bun.file(join(dir, SANDBOX_POLICY_PATH)).text());
}

describe("sandbox module", () => {
  test("ISC-68: apply writes sandbox policy with filesystem, network, and credential contract", async () => {
    const ctx = makeTestCtx(dir, { presentTools: { nono: "nono 0.3.0" } });
    const result = await sandboxModule.apply(ctx);
    expect(result.status).toBe("applied");
    expect(result.wrotePaths).toContain(SANDBOX_POLICY_PATH);
    const policy = await readPolicy();
    expect(policy.filesystem.writeScope).toEqual(["<repo>"]);
    expect(policy.filesystem.denyWrite).toEqual(["~/.ssh", "~/.aws", "~/.claude", "system paths"]);
    expect(policy.filesystem.denyRead).toEqual([".env", ".env.*", "~/.ssh/**"]);
    expect(policy.network.allowlist).toEqual(DEFAULT_NETWORK_ALLOWLIST);
    expect(policy.network.allowlist).toContain("registry.npmjs.org");
    expect(policy.credentials.injection).toBe("at-boundary");
    expect(policy.credentials.rule).toContain("injected by the sandbox at exec time");
  });

  test("ISC-68: policy contains no absolute machine paths", async () => {
    await sandboxModule.apply(makeTestCtx(dir, { presentTools: { nono: "0.3.0" } }));
    const raw = await Bun.file(join(dir, SANDBOX_POLICY_PATH)).text();
    expect(raw).not.toContain(dir);
    expect(raw).not.toContain("/Users/");
    expect(raw).not.toContain("/home/");
  });

  test("ISC-71: default network policy is deny-with-allowlist", async () => {
    await sandboxModule.apply(makeTestCtx(dir, { presentTools: { nono: "0.3.0" } }));
    const policy = await readPolicy();
    expect(policy.network.default).toBe("deny");
    expect(Array.isArray(policy.network.allowlist)).toBe(true);
  });

  test("ISC-69: detect reports nono with install remediation when absent", async () => {
    const findings = await sandboxModule.detect(makeTestCtx(dir));
    const nono = findings.find((finding) => finding.message.includes("nono"));
    expect(nono?.level).toBe("degraded");
    expect(nono?.remediation).toContain("nono.sh");
    const present = await sandboxModule.detect(makeTestCtx(dir, { presentTools: { nono: "0.3.0" } }));
    expect(present.some((finding) => finding.level === "ok" && finding.message.includes("nono"))).toBe(true);
  });

  test("ISC-69: nono absent → degraded but the policy is still written", async () => {
    const ctx = makeTestCtx(dir);
    const result = await sandboxModule.apply(ctx);
    expect(result.status).toBe("degraded");
    expect(result.findings.some((finding) => finding.remediation?.includes("nono.sh"))).toBe(true);
    const policy = await readPolicy();
    expect(policy.network.default).toBe("deny");
    expect(policy.enforcement).toBe("advisory");
  });

  test("ISC-70: claude-code targeted → deny-read surface mapped into .claude/settings.json", async () => {
    const ctx = makeTestCtx(dir, { presentTools: { nono: "0.3.0" } });
    const result = await sandboxModule.apply(ctx);
    expect(result.wrotePaths).toContain(CLAUDE_SETTINGS_PATH);
    const settings = JSON.parse(await Bun.file(join(dir, CLAUDE_SETTINGS_PATH)).text());
    for (const entry of CLAUDE_DENY_READ) {
      expect(settings.permissions.deny).toContain(entry);
    }
  });

  test("ISC-70: pre-existing user permissions.deny entry is preserved (array union)", async () => {
    await Bun.write(
      join(dir, CLAUDE_SETTINGS_PATH),
      JSON.stringify({ permissions: { deny: ["Bash(rm -rf *)"] }, model: "user-choice" }),
    );
    const ctx = makeTestCtx(dir, { presentTools: { nono: "0.3.0" } });
    await sandboxModule.apply(ctx);
    const settings = JSON.parse(await Bun.file(join(dir, CLAUDE_SETTINGS_PATH)).text());
    expect(settings.permissions.deny).toContain("Bash(rm -rf *)");
    expect(settings.model).toBe("user-choice");
    for (const entry of CLAUDE_DENY_READ) {
      expect(settings.permissions.deny).toContain(entry);
    }
  });

  test("ISC-70: claude-code NOT targeted → no .claude/settings.json written", async () => {
    const config = testConfig({ harnesses: ["codex"] });
    const ctx = makeTestCtx(dir, { config, presentTools: { nono: "0.3.0" } });
    const result = await sandboxModule.apply(ctx);
    expect(result.status).toBe("applied");
    expect(await Bun.file(join(dir, CLAUDE_SETTINGS_PATH)).exists()).toBe(false);
  });

  test("ISC-116: plan writes nothing to disk", async () => {
    const ctx = makeTestCtx(dir, { presentTools: { nono: "0.3.0" } });
    const actions = await sandboxModule.plan(ctx);
    expect(actions.some((action) => action.path === SANDBOX_POLICY_PATH)).toBe(true);
    expect(actions.some((action) => action.path === CLAUDE_SETTINGS_PATH)).toBe(true);
    expect(await Bun.file(join(dir, SANDBOX_POLICY_PATH)).exists()).toBe(false);
    expect(await Bun.file(join(dir, CLAUDE_SETTINGS_PATH)).exists()).toBe(false);
  });

  test("ISC-117: apply is idempotent — second run byte-identical for policy and settings", async () => {
    const ctx = makeTestCtx(dir, { presentTools: { nono: "0.3.0" } });
    await sandboxModule.apply(ctx);
    const firstPolicy = sha256(await Bun.file(join(dir, SANDBOX_POLICY_PATH)).text());
    const firstSettings = sha256(await Bun.file(join(dir, CLAUDE_SETTINGS_PATH)).text());
    await sandboxModule.apply(makeTestCtx(dir, { presentTools: { nono: "0.3.0" } }));
    expect(sha256(await Bun.file(join(dir, SANDBOX_POLICY_PATH)).text())).toBe(firstPolicy);
    expect(sha256(await Bun.file(join(dir, CLAUDE_SETTINGS_PATH)).text())).toBe(firstSettings);
  });

  test("options: allowHosts extends the network allowlist without displacing defaults", async () => {
    const config = testConfig({
      modules: { sandbox: { enabled: true, options: { allowHosts: ["internal.example.test"] } } },
    });
    await sandboxModule.apply(makeTestCtx(dir, { config, presentTools: { nono: "0.3.0" } }));
    const policy = await readPolicy();
    expect(policy.network.default).toBe("deny");
    for (const host of DEFAULT_NETWORK_ALLOWLIST) {
      expect(policy.network.allowlist).toContain(host);
    }
    expect(policy.network.allowlist).toContain("internal.example.test");
  });

  test("options: malformed option arrays are rejected — apply fails, nothing written", async () => {
    const config = testConfig({
      modules: { sandbox: { enabled: true, options: { allowHosts: "not-an-array" } } },
    });
    const result = await sandboxModule.apply(makeTestCtx(dir, { config }));
    expect(result.status).toBe("failed");
    expect(result.findings.some((finding) => finding.level === "error")).toBe(true);
    expect(await Bun.file(join(dir, SANDBOX_POLICY_PATH)).exists()).toBe(false);
    expect(validateSandboxOptions({ denyRead: [""] }).length).toBe(1);
    expect(validateSandboxOptions({ allowHosts: ["ok.example"] })).toEqual([]);
  });

  test("apply refuses to clobber unparseable user settings.json and degrades", async () => {
    await Bun.write(join(dir, CLAUDE_SETTINGS_PATH), "{not json");
    const ctx = makeTestCtx(dir, { presentTools: { nono: "0.3.0" } });
    const result = await sandboxModule.apply(ctx);
    expect(result.status).toBe("degraded");
    expect(await Bun.file(join(dir, CLAUDE_SETTINGS_PATH)).text()).toBe("{not json");
    expect(await Bun.file(join(dir, SANDBOX_POLICY_PATH)).exists()).toBe(true);
  });

  test("verify: passes after apply; fails when network.default tampered to allow", async () => {
    const ctx = makeTestCtx(dir, { presentTools: { nono: "0.3.0" } });
    await sandboxModule.apply(ctx);
    expect((await sandboxModule.verify(ctx)).ok).toBe(true);

    const policy = await readPolicy();
    policy.network.default = "allow";
    await Bun.write(join(dir, SANDBOX_POLICY_PATH), JSON.stringify(policy));
    const tampered = await sandboxModule.verify(ctx);
    expect(tampered.ok).toBe(false);
    expect(tampered.findings.some((finding) => finding.level === "error")).toBe(true);
  });

  test("verify: fails when policy missing or a claude deny entry is removed", async () => {
    const ctx = makeTestCtx(dir, { presentTools: { nono: "0.3.0" } });
    expect((await sandboxModule.verify(ctx)).ok).toBe(false);

    await sandboxModule.apply(ctx);
    const settings = JSON.parse(await Bun.file(join(dir, CLAUDE_SETTINGS_PATH)).text());
    settings.permissions.deny = settings.permissions.deny.filter(
      (entry: string) => entry !== "Read(~/.ssh/**)",
    );
    await Bun.write(join(dir, CLAUDE_SETTINGS_PATH), JSON.stringify(settings));
    const result = await sandboxModule.verify(ctx);
    expect(result.ok).toBe(false);
    expect(result.findings.some((finding) => finding.message.includes("Read(~/.ssh/**)"))).toBe(true);
  });

  test("instruction block tells harnesses to stay inside the sandbox and escalate to a human", () => {
    expect(sandboxModule.instructionBlocks.length).toBeGreaterThan(0);
    const block = sandboxModule.instructionBlocks[0]!;
    expect(block.content).toContain(".ade/policy/sandbox.json");
    expect(block.content.toLowerCase()).toContain("never attempt to read credential files");
    expect(block.content.toLowerCase()).toContain("ask a human");
    expect(block.content.toLowerCase()).toContain("bypass");
  });
});
