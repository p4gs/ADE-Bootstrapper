import { afterEach, beforeEach, describe, expect, test } from "bun:test";
import { join } from "node:path";
import { gitHygieneModule, GIT_POLICY_PATH } from "../../src/modules/git-hygiene.ts";
import { fakeExec, makeTempDir, makeTestCtx, removeDir } from "../helpers.ts";
import { sha256 } from "../../src/fsutil.ts";

let dir: string;

beforeEach(async () => {
  dir = await makeTempDir();
});

afterEach(async () => {
  await removeDir(dir);
});

describe("git-hygiene module", () => {
  test("ISC-99: non-git target → detect degrades with git init guidance, no crash", async () => {
    const ctx = makeTestCtx(dir, { isGitRepo: false });
    const findings = await gitHygieneModule.detect(ctx);
    const degraded = findings.find(
      (finding) => finding.level === "degraded" && finding.message.includes("not a git repository"),
    );
    expect(degraded).toBeDefined();
    expect(degraded?.remediation).toContain("git init");
  });

  test("ISC-99: non-git target → apply degrades (never failed) with git init guidance", async () => {
    const ctx = makeTestCtx(dir, { isGitRepo: false, presentTools: { ocean: "ocean 1.0.0" } });
    const result = await gitHygieneModule.apply(ctx);
    expect(result.status).toBe("degraded");
    expect(result.findings.some((finding) => finding.remediation?.includes("git init"))).toBe(true);
  });

  test("ISC-100: commit signing enabled → detect reports ok 'commit signing enabled'", async () => {
    const ctx = makeTestCtx(dir, {
      presentTools: { ocean: "ocean 1.0.0" },
      exec: fakeExec({ "config --get commit.gpgsign": { code: 0, stdout: "true\n" } }),
    });
    const findings = await gitHygieneModule.detect(ctx);
    expect(findings.some((finding) => finding.level === "ok" && finding.message === "commit signing enabled")).toBe(true);
  });

  test("ISC-100: commit signing not configured → info finding with enable guidance", async () => {
    const ctx = makeTestCtx(dir, {
      presentTools: { ocean: "ocean 1.0.0" },
      // git config --get on an unset key exits 1 with empty stdout
      exec: fakeExec({ "config --get commit.gpgsign": { code: 1, stdout: "" } }),
    });
    const findings = await gitHygieneModule.detect(ctx);
    const signing = findings.find((finding) => finding.message.includes("commit signing not enabled"));
    expect(signing?.level).toBe("info");
    expect(signing?.remediation).toContain("git config commit.gpgsign true");
    expect(signing?.remediation).toContain("user.signingkey");
  });

  test("ISC-100: commit.gpgsign explicitly false → info finding, not ok", async () => {
    const ctx = makeTestCtx(dir, {
      presentTools: { ocean: "ocean 1.0.0" },
      exec: fakeExec({ "config --get commit.gpgsign": { code: 0, stdout: "false\n" } }),
    });
    const findings = await gitHygieneModule.detect(ctx);
    expect(findings.some((finding) => finding.message === "commit signing enabled")).toBe(false);
    expect(findings.some((finding) => finding.level === "info" && finding.message.includes("commit signing"))).toBe(true);
  });

  test("ISC-101: apply writes git.json with the full repo integrity contract", async () => {
    const ctx = makeTestCtx(dir, { presentTools: { ocean: "ocean 1.0.0" } });
    const result = await gitHygieneModule.apply(ctx);
    expect(result.status).toBe("applied");
    expect(result.wrotePaths).toContain(GIT_POLICY_PATH);
    const policy = JSON.parse(await Bun.file(join(dir, GIT_POLICY_PATH)).text());
    expect(policy.protectedBranches).toEqual(["main", "master"]);
    expect(policy.forcePushToProtected).toBe("deny");
    expect(policy.requirePrForProtected).toBe(true);
    expect(policy.historyRewriteOfPushed).toBe("deny");
    expect(policy.branchNaming).toBe("type/short-kebab-description (feat/fix/chore/docs)");
    expect(policy.commitStyle).toBe("conventional");
  });

  test("ISC-102: ocean present → detect recommends `ocean harden --tags baseline`", async () => {
    const ctx = makeTestCtx(dir, {
      presentTools: { ocean: "ocean 1.0.0" },
      exec: fakeExec({ "config --get commit.gpgsign": { code: 0, stdout: "true\n" } }),
    });
    const findings = await gitHygieneModule.detect(ctx);
    expect(findings.some((finding) => finding.level === "ok" && finding.message.includes("ocean present"))).toBe(true);
    expect(
      findings.some((finding) => finding.level === "ok" && finding.message.includes("ocean harden --tags baseline")),
    ).toBe(true);
  });

  test("ISC-102: ocean absent → degraded finding with install guidance, apply degrades not fails", async () => {
    const ctx = makeTestCtx(dir);
    const findings = await gitHygieneModule.detect(ctx);
    const ocean = findings.find((finding) => finding.message.includes("ocean not installed"));
    expect(ocean?.level).toBe("degraded");
    expect(ocean?.remediation).toContain("github.com/grcengineering/OCEAN");

    const result = await gitHygieneModule.apply(ctx);
    expect(result.status).toBe("degraded");
    expect(result.findings.some((finding) => finding.remediation?.includes("github.com/grcengineering/OCEAN"))).toBe(true);
    // policy artifact is still written — degradation is about deep hardening, not the contract
    expect(await Bun.file(join(dir, GIT_POLICY_PATH)).exists()).toBe(true);
  });

  test("ISC-103: instruction block forbids force-push/history rewrite and mandates PRs, naming, branch safety", () => {
    expect(gitHygieneModule.instructionBlocks.length).toBeGreaterThan(0);
    const block = gitHygieneModule.instructionBlocks[0]!;
    expect(block.content).toContain("NEVER force-push");
    expect(block.content).toContain("NEVER rewrite pushed history");
    expect(block.content).toContain("pull request");
    expect(block.content).toContain("type/short-kebab-description");
    expect(block.content.toLowerCase()).toContain("conventional commit");
    expect(block.content).toContain("NEVER delete branches you did not create");
  });

  test("ISC-116: plan writes nothing to disk", async () => {
    const ctx = makeTestCtx(dir, { presentTools: { ocean: "ocean 1.0.0" } });
    const actions = await gitHygieneModule.plan(ctx);
    expect(actions.length).toBeGreaterThan(0);
    expect(actions.some((action) => action.kind === "write" && action.path === GIT_POLICY_PATH)).toBe(true);
    expect(await Bun.file(join(dir, GIT_POLICY_PATH)).exists()).toBe(false);
  });

  test("ISC-117: apply is idempotent — second run byte-identical", async () => {
    await gitHygieneModule.apply(makeTestCtx(dir, { presentTools: { ocean: "ocean 1.0.0" } }));
    const first = sha256(await Bun.file(join(dir, GIT_POLICY_PATH)).text());
    await gitHygieneModule.apply(makeTestCtx(dir, { presentTools: { ocean: "ocean 1.0.0" } }));
    const second = sha256(await Bun.file(join(dir, GIT_POLICY_PATH)).text());
    expect(second).toBe(first);
  });

  test("verify passes after apply and re-derives the contract from disk", async () => {
    const ctx = makeTestCtx(dir, { presentTools: { ocean: "ocean 1.0.0" } });
    await gitHygieneModule.apply(ctx);
    const result = await gitHygieneModule.verify(ctx);
    expect(result.ok).toBe(true);
    expect(result.findings.some((finding) => finding.level === "ok")).toBe(true);
  });

  test("verify fails on missing, invalid, or tampered git.json", async () => {
    const ctx = makeTestCtx(dir);
    expect((await gitHygieneModule.verify(ctx)).ok).toBe(false);

    await gitHygieneModule.apply(ctx);
    await Bun.write(join(dir, GIT_POLICY_PATH), "{not json");
    expect((await gitHygieneModule.verify(ctx)).ok).toBe(false);

    // tampered: force-push allowed
    await Bun.write(
      join(dir, GIT_POLICY_PATH),
      JSON.stringify({ protectedBranches: ["main"], forcePushToProtected: "allow" }),
    );
    expect((await gitHygieneModule.verify(ctx)).ok).toBe(false);

    // tampered: no protected branches left
    await Bun.write(
      join(dir, GIT_POLICY_PATH),
      JSON.stringify({ protectedBranches: [], forcePushToProtected: "deny" }),
    );
    expect((await gitHygieneModule.verify(ctx)).ok).toBe(false);
  });

  test("verify degrades (not failed) when target is not a git repo", async () => {
    const ctx = makeTestCtx(dir, { isGitRepo: false });
    await gitHygieneModule.apply(ctx);
    const result = await gitHygieneModule.verify(ctx);
    expect(result.ok).toBe(true);
    expect(result.findings.some((finding) => finding.level === "degraded")).toBe(true);
    expect(result.findings.some((finding) => finding.level === "error")).toBe(false);
  });

  test("generated git.json is deterministic — no timestamps, env values, or absolute paths", async () => {
    const planted = "PLANTED_ENV_SECRET_VALUE_98765";
    const ctx = makeTestCtx(dir, {
      presentTools: { ocean: "ocean 1.0.0" },
      env: { GITHUB_TOKEN: planted },
    });
    await gitHygieneModule.apply(ctx);
    const text = await Bun.file(join(dir, GIT_POLICY_PATH)).text();
    expect(text).not.toContain(planted);
    expect(text).not.toContain(dir);
  });
});
