/**
 * Module: Git & Repository Hygiene.
 * Spec component: "Git and repository hygiene enforcement — protected-branch
 * policy (no force-push, no rewrite of pushed history, PR-only changes),
 * branch naming and commit-style conventions, commit-signing posture, and
 * deep repo hardening via OCEAN integration."
 * Boundary controlled: the git boundary (repo integrity).
 *
 * Integration over rebuild: OCEAN is the deep-hardening engine. This module
 * writes the machine-readable hygiene contract and points at
 * `ocean harden --tags baseline` for enforcement beyond the contract —
 * it never reimplements OCEAN's checks.
 */
import { readJson, toolFinding, verifyJsonArtifact, writePolicy } from "./_shared.ts";
import type { AdeModule, Ctx, Finding, ModuleResult, PlannedAction } from "../types.ts";

export const GIT_POLICY_PATH = ".ade/policy/git.json";

interface GitPolicy {
  schemaVersion: number;
  protectedBranches: string[];
  forcePushToProtected: string;
  requirePrForProtected: boolean;
  historyRewriteOfPushed: string;
  branchNaming: string;
  commitStyle: string;
}

function gitPolicy(): GitPolicy {
  return {
    schemaVersion: 1,
    protectedBranches: ["main", "master"],
    forcePushToProtected: "deny",
    requirePrForProtected: true,
    historyRewriteOfPushed: "deny",
    branchNaming: "type/short-kebab-description (feat/fix/chore/docs)",
    commitStyle: "conventional",
  };
}

const NOT_GIT_FINDING: Finding = {
  level: "degraded",
  message: "target is not a git repository — git hygiene enforcement unavailable",
  remediation: "run `git init` in the target, then `ade apply`",
};

/** Standard finding for OCEAN presence: integrate when present, guide install when absent. */
function oceanFindings(ctx: Ctx): Finding[] {
  const findings: Finding[] = [
    toolFinding(ctx, "ocean", "install OCEAN for deep repo hardening — see github.com/grcengineering/OCEAN"),
  ];
  if (ctx.tools["ocean"]?.present === true) {
    findings.push({
      level: "ok",
      message: "OCEAN available — run `ocean harden --tags baseline` for deep repository hardening (integration, not reimplementation)",
    });
  }
  return findings;
}

/** Check the repo's commit-signing configuration via `git config`. */
async function signingFinding(ctx: Ctx): Promise<Finding> {
  const result = await ctx.exec(["git", "-C", ctx.targetDir, "config", "--get", "commit.gpgsign"]);
  if (result.code === 0 && result.stdout.trim() === "true") {
    return { level: "ok", message: "commit signing enabled" };
  }
  return {
    level: "info",
    message: "commit signing not enabled",
    remediation: "enable with `git config commit.gpgsign true` and set `user.signingkey`",
  };
}

export const gitHygieneModule: AdeModule = {
  id: "git-hygiene",
  title: "Git & Repository Hygiene",
  category: "security",
  spec: "Git and repository hygiene enforcement",
  defaultEnabled: true,
  instructionBlocks: [
    {
      id: "git-hygiene",
      title: "Git & Repository Hygiene",
      content: [
        "The repo integrity contract lives at `.ade/policy/git.json`.",
        "- NEVER force-push protected branches (`main`, `master`).",
        "- NEVER rewrite pushed history (no rebase/amend of commits that exist on a remote).",
        "- Route all protected-branch changes through pull requests — never commit to them directly.",
        "- Name branches `type/short-kebab-description` (types: feat/fix/chore/docs).",
        "- Write conventional commit messages.",
        "- NEVER delete branches you did not create without explicit approval.",
      ].join("\n"),
    },
  ],

  async detect(ctx: Ctx): Promise<Finding[]> {
    const findings: Finding[] = oceanFindings(ctx);
    if (!ctx.isGitRepo) {
      findings.push(NOT_GIT_FINDING);
      return findings;
    }
    findings.push(await signingFinding(ctx));
    return findings;
  },

  async plan(ctx: Ctx): Promise<PlannedAction[]> {
    void ctx;
    return [
      {
        kind: "write",
        path: GIT_POLICY_PATH,
        description: "write git hygiene policy (protected branches, force-push/rewrite denial, PR requirement, naming + commit conventions)",
      },
    ];
  },

  async apply(ctx: Ctx): Promise<ModuleResult> {
    const findings: Finding[] = [];
    const wrotePaths: string[] = [];

    await writePolicy(ctx, GIT_POLICY_PATH, gitPolicy());
    wrotePaths.push(GIT_POLICY_PATH);
    findings.push({ level: "ok", message: `wrote ${GIT_POLICY_PATH}` });

    if (!ctx.isGitRepo) {
      findings.push(NOT_GIT_FINDING);
      return { status: "degraded", findings, wrotePaths };
    }

    findings.push(...oceanFindings(ctx));
    if (ctx.tools["ocean"]?.present !== true) {
      return { status: "degraded", findings, wrotePaths };
    }
    return { status: "applied", findings, wrotePaths };
  },

  async verify(ctx: Ctx) {
    const findings: Finding[] = [await verifyJsonArtifact(ctx, GIT_POLICY_PATH)];
    let ok = findings.every((finding) => finding.level === "ok");

    if (ok) {
      const parsed = (await readJson(ctx, GIT_POLICY_PATH)) as GitPolicy | null;
      const branchesValid =
        parsed !== null && Array.isArray(parsed.protectedBranches) && parsed.protectedBranches.length > 0;
      if (!branchesValid || parsed?.forcePushToProtected !== "deny") {
        ok = false;
        findings.push({
          level: "error",
          message: `${GIT_POLICY_PATH} must list protected branches and deny force-push to them`,
          remediation: "run `ade apply` to regenerate",
        });
      }
    }

    if (!ctx.isGitRepo) {
      findings.push({
        level: "degraded",
        message: "not a git repository — git-boundary enforcement not verifiable",
        remediation: "run `git init`, then `ade apply`",
      });
    }
    return { ok, findings };
  },
};
