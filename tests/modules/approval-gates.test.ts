import { afterEach, beforeEach, describe, expect, test } from "bun:test";
import { join } from "node:path";
import {
  approvalGatesModule,
  validateApprovalOptions,
  ACTION_CLASSES,
  NEVER_ALLOW,
  CLAUDE_DENY_RULES,
  CLAUDE_ASK_RULES,
  APPROVALS_POLICY_PATH,
} from "../../src/modules/approval-gates.ts";
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

async function readPolicy(): Promise<{
  neverAllow: string[];
  actions: Record<string, { decision: string; rationale: string; examples: string[] }>;
}> {
  return JSON.parse(await Bun.file(join(dir, APPROVALS_POLICY_PATH)).text());
}

describe("approval-gates module", () => {
  test("ISC-89: apply writes approvals.json enumerating EXACTLY the eight action classes", async () => {
    const ctx = makeTestCtx(dir, { config: testConfig({ harnesses: ["codex"] }) });
    const result = await approvalGatesModule.apply(ctx);
    expect(result.status).toBe("applied");
    expect(result.wrotePaths).toContain(APPROVALS_POLICY_PATH);
    const policy = await readPolicy();
    expect(Object.keys(policy.actions).sort()).toEqual([...ACTION_CLASSES].sort());
    for (const cls of ACTION_CLASSES) {
      const entry = policy.actions[cls]!;
      expect(["ask", "allow", "deny"]).toContain(entry.decision);
      expect(entry.rationale.length).toBeGreaterThan(0);
      expect(Array.isArray(entry.examples)).toBe(true);
      expect(entry.examples.length).toBeGreaterThan(0);
    }
  });

  test("ISC-90: secure defaults are exactly ask×7 + productionAffecting deny", async () => {
    const ctx = makeTestCtx(dir, { config: testConfig({ harnesses: ["codex"] }) });
    await approvalGatesModule.apply(ctx);
    const policy = await readPolicy();
    expect(policy.actions["productionAffecting"]!.decision).toBe("deny");
    for (const cls of ACTION_CLASSES.filter((c) => c !== "productionAffecting")) {
      expect(policy.actions[cls]!.decision).toBe("ask");
    }
  });

  test("ISC-90: options override decisions for non-protected classes", async () => {
    const config = testConfig({
      harnesses: ["codex"],
      modules: {
        "approval-gates": {
          enabled: true,
          options: { externalNetwork: "allow", merge: "deny" },
        },
      },
    });
    const ctx = makeTestCtx(dir, { config });
    const result = await approvalGatesModule.apply(ctx);
    expect(result.status).toBe("applied");
    const policy = await readPolicy();
    expect(policy.actions["externalNetwork"]!.decision).toBe("allow");
    expect(policy.actions["merge"]!.decision).toBe("deny");
    expect(policy.actions["destructiveShell"]!.decision).toBe("ask");
  });

  test("ISC-90: never-allow classes cannot become 'allow' — apply fails, nothing written", async () => {
    for (const cls of NEVER_ALLOW) {
      const scratch = await makeTempDir();
      try {
        const config = testConfig({
          modules: { "approval-gates": { enabled: true, options: { [cls]: "allow" } } },
        });
        const ctx = makeTestCtx(scratch, { config });
        const result = await approvalGatesModule.apply(ctx);
        expect(result.status).toBe("failed");
        expect(result.findings.some((finding) => finding.level === "error" && finding.message.includes(cls))).toBe(true);
        expect(await Bun.file(join(scratch, APPROVALS_POLICY_PATH)).exists()).toBe(false);
      } finally {
        await removeDir(scratch);
      }
    }
  });

  test("ISC-90: validateApprovalOptions rejects never-allow escalation and malformed decisions", () => {
    expect(validateApprovalOptions({})).toEqual([]);
    expect(validateApprovalOptions({ merge: "allow", branchOps: "deny" })).toEqual([]);
    expect(validateApprovalOptions({ destructiveShell: "allow" }).length).toBe(1);
    expect(validateApprovalOptions({ credentialUse: "allow" }).length).toBe(1);
    expect(validateApprovalOptions({ productionAffecting: "allow" }).length).toBe(1);
    // never-allow classes may still be tightened
    expect(validateApprovalOptions({ destructiveShell: "deny" })).toEqual([]);
    expect(validateApprovalOptions({ merge: "yolo" }).length).toBe(1);
    expect(validateApprovalOptions({ merge: 42 }).length).toBe(1);
  });

  test("ISC-91: claude-code targeted → deny/ask permission rules merged into settings.json", async () => {
    const ctx = makeTestCtx(dir, { config: testConfig({ harnesses: ["claude-code"] }) });
    const result = await approvalGatesModule.apply(ctx);
    expect(result.status).toBe("applied");
    expect(result.wrotePaths).toContain(CLAUDE_SETTINGS_PATH);
    const settings = JSON.parse(await Bun.file(join(dir, CLAUDE_SETTINGS_PATH)).text());
    for (const rule of CLAUDE_DENY_RULES) expect(settings.permissions.deny).toContain(rule);
    for (const rule of CLAUDE_ASK_RULES) expect(settings.permissions.ask).toContain(rule);
    expect(settings.permissions.deny).toContain("Bash(rm -rf /:*)");
    expect(settings.permissions.ask).toContain("Bash(git push:*)");
  });

  test("ISC-91: pre-seeded user deny entry is preserved after merge", async () => {
    await Bun.write(
      join(dir, CLAUDE_SETTINGS_PATH),
      JSON.stringify({ permissions: { deny: ["Bash(curl:*)"] }, model: "user-choice" }),
    );
    const ctx = makeTestCtx(dir, { config: testConfig({ harnesses: ["claude-code"] }) });
    await approvalGatesModule.apply(ctx);
    const settings = JSON.parse(await Bun.file(join(dir, CLAUDE_SETTINGS_PATH)).text());
    expect(settings.permissions.deny).toContain("Bash(curl:*)");
    expect(settings.model).toBe("user-choice");
    for (const rule of CLAUDE_DENY_RULES) expect(settings.permissions.deny).toContain(rule);
  });

  test("ISC-91: non-claude harness → settings.json not written", async () => {
    const ctx = makeTestCtx(dir, { config: testConfig({ harnesses: ["codex"] }) });
    const result = await approvalGatesModule.apply(ctx);
    expect(result.wrotePaths).not.toContain(CLAUDE_SETTINGS_PATH);
    expect(await Bun.file(join(dir, CLAUDE_SETTINGS_PATH)).exists()).toBe(false);
  });

  test("ISC-91: unparseable user settings.json → degraded, user file never clobbered", async () => {
    const broken = "{not json";
    await Bun.write(join(dir, CLAUDE_SETTINGS_PATH), broken);
    const ctx = makeTestCtx(dir, { config: testConfig({ harnesses: ["claude-code"] }) });
    const result = await approvalGatesModule.apply(ctx);
    expect(result.status).toBe("degraded");
    expect(result.findings.some((finding) => finding.level === "error")).toBe(true);
    expect(await Bun.file(join(dir, CLAUDE_SETTINGS_PATH)).text()).toBe(broken);
    // the policy itself still lands
    expect(await Bun.file(join(dir, APPROVALS_POLICY_PATH)).exists()).toBe(true);
  });

  test("ISC-92: instruction block requires human approval for the eight classes and denies production changes", () => {
    expect(approvalGatesModule.instructionBlocks.length).toBeGreaterThan(0);
    const block = approvalGatesModule.instructionBlocks[0]!;
    expect(block.content).toContain("REQUIRE explicit human approval");
    expect(block.content.toLowerCase()).toContain("when in doubt");
    expect(block.content).toContain("DENIED without a human decision");
    for (const phrase of ["destructive shell", "credential use", "external network", "dependency installs", "branch operations", "PR creation", "merges", "production-affecting"]) {
      expect(block.content).toContain(phrase);
    }
  });

  test("ISC-116: plan writes nothing to disk", async () => {
    const ctx = makeTestCtx(dir, { config: testConfig({ harnesses: ["claude-code"] }) });
    const actions = await approvalGatesModule.plan(ctx);
    expect(actions.length).toBe(2);
    expect(actions.some((action) => action.kind === "merge" && action.path === CLAUDE_SETTINGS_PATH)).toBe(true);
    expect(await Bun.file(join(dir, APPROVALS_POLICY_PATH)).exists()).toBe(false);
    expect(await Bun.file(join(dir, CLAUDE_SETTINGS_PATH)).exists()).toBe(false);
  });

  test("ISC-117: apply is idempotent — second run byte-identical (policy + settings)", async () => {
    const config = testConfig({ harnesses: ["claude-code"] });
    await approvalGatesModule.apply(makeTestCtx(dir, { config }));
    const firstPolicy = sha256(await Bun.file(join(dir, APPROVALS_POLICY_PATH)).text());
    const firstSettings = sha256(await Bun.file(join(dir, CLAUDE_SETTINGS_PATH)).text());
    await approvalGatesModule.apply(makeTestCtx(dir, { config }));
    expect(sha256(await Bun.file(join(dir, APPROVALS_POLICY_PATH)).text())).toBe(firstPolicy);
    expect(sha256(await Bun.file(join(dir, CLAUDE_SETTINGS_PATH)).text())).toBe(firstSettings);
  });

  test("verify: passes after apply (claude-code + policy checks)", async () => {
    const ctx = makeTestCtx(dir, { config: testConfig({ harnesses: ["claude-code"] }) });
    await approvalGatesModule.apply(ctx);
    const result = await approvalGatesModule.verify(ctx);
    expect(result.ok).toBe(true);
    expect(result.findings.some((finding) => finding.message.includes("eight gated action classes"))).toBe(true);
    expect(result.findings.some((finding) => finding.message.includes("deny/ask permission rules"))).toBe(true);
  });

  test("verify: fails when policy missing or invalid JSON", async () => {
    const ctx = makeTestCtx(dir, { config: testConfig({ harnesses: ["codex"] }) });
    expect((await approvalGatesModule.verify(ctx)).ok).toBe(false);
    await Bun.write(join(dir, APPROVALS_POLICY_PATH), "{not json");
    expect((await approvalGatesModule.verify(ctx)).ok).toBe(false);
  });

  test("verify: fails when a never-allow class was tampered to 'allow'", async () => {
    const ctx = makeTestCtx(dir, { config: testConfig({ harnesses: ["codex"] }) });
    await approvalGatesModule.apply(ctx);
    const policy = await readPolicy();
    policy.actions["productionAffecting"]!.decision = "allow";
    await Bun.write(join(dir, APPROVALS_POLICY_PATH), JSON.stringify(policy));
    const result = await approvalGatesModule.verify(ctx);
    expect(result.ok).toBe(false);
    expect(result.findings.some((finding) => finding.message.includes("productionAffecting"))).toBe(true);
  });

  test("verify: fails when an action class was removed from the policy", async () => {
    const ctx = makeTestCtx(dir, { config: testConfig({ harnesses: ["codex"] }) });
    await approvalGatesModule.apply(ctx);
    const policy = await readPolicy();
    delete (policy.actions as Record<string, unknown>)["credentialUse"];
    await Bun.write(join(dir, APPROVALS_POLICY_PATH), JSON.stringify(policy));
    const result = await approvalGatesModule.verify(ctx);
    expect(result.ok).toBe(false);
    expect(result.findings.some((finding) => finding.message.includes("credentialUse"))).toBe(true);
  });

  test("verify: fails when claude settings lose a deny rule after apply", async () => {
    const ctx = makeTestCtx(dir, { config: testConfig({ harnesses: ["claude-code"] }) });
    await approvalGatesModule.apply(ctx);
    const settings = JSON.parse(await Bun.file(join(dir, CLAUDE_SETTINGS_PATH)).text());
    settings.permissions.deny = settings.permissions.deny.filter(
      (rule: string) => rule !== "Bash(git push --force:*)",
    );
    await Bun.write(join(dir, CLAUDE_SETTINGS_PATH), JSON.stringify(settings));
    const result = await approvalGatesModule.verify(ctx);
    expect(result.ok).toBe(false);
    expect(result.findings.some((finding) => finding.message.includes("git push --force"))).toBe(true);
  });

  test("detect reports option validity", async () => {
    const ok = await approvalGatesModule.detect(makeTestCtx(dir));
    expect(ok.every((finding) => finding.level === "ok")).toBe(true);
    const bad = await approvalGatesModule.detect(
      makeTestCtx(dir, {
        config: testConfig({
          modules: { "approval-gates": { enabled: true, options: { credentialUse: "allow" } } },
        }),
      }),
    );
    expect(bad.some((finding) => finding.level === "error")).toBe(true);
  });
});
