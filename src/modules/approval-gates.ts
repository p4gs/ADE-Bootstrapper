/**
 * Module: human-in-the-loop approval gates.
 * Spec component: "Human-in-the-loop approval gates — explicit human approval
 * requirements for high-risk actions (destructive shell commands, credential
 * use, external network access, dependency installs, branch operations, PR
 * creation, merges, and production-affecting changes)."
 * Boundary controlled: the action-authorization boundary — no high-risk action
 * executes without an explicit human decision, and production-affecting
 * changes are denied outright absent one.
 *
 * The policy is enforced at two layers: a machine-readable contract at
 * `.ade/policy/approvals.json` (consumed by harnesses and tooling), and — for
 * Claude Code — native permission deny/ask rules merged additively into
 * `.claude/settings.json` (user entries are always preserved).
 */
import { readJson, verifyJsonArtifact, writePolicy } from "./_shared.ts";
import { mergeClaudeSettings, CLAUDE_SETTINGS_PATH } from "../harness/claude.ts";
import type { AdeModule, Ctx, Finding, ModuleResult, PlannedAction } from "../types.ts";

export const APPROVALS_POLICY_PATH = ".ade/policy/approvals.json";

/** The eight gated action classes — the policy enumerates EXACTLY these. */
export const ACTION_CLASSES = [
  "destructiveShell",
  "credentialUse",
  "externalNetwork",
  "dependencyInstall",
  "branchOps",
  "prCreation",
  "merge",
  "productionAffecting",
] as const;

export type ActionClass = (typeof ACTION_CLASSES)[number];
export type Decision = "ask" | "allow" | "deny";

/** Classes that can NEVER be configured to 'allow' (ISC-126 anti-criterion, enforced at the writer). */
export const NEVER_ALLOW: readonly ActionClass[] = [
  "destructiveShell",
  "credentialUse",
  "productionAffecting",
];

/** Secure defaults: everything asks; production-affecting changes are denied. */
const DEFAULT_DECISIONS: Record<ActionClass, Decision> = {
  destructiveShell: "ask",
  credentialUse: "ask",
  externalNetwork: "ask",
  dependencyInstall: "ask",
  branchOps: "ask",
  prCreation: "ask",
  merge: "ask",
  productionAffecting: "deny",
};

const RATIONALES: Record<ActionClass, string> = {
  destructiveShell: "destructive shell commands can irreversibly delete work or system state",
  credentialUse: "credential use can exfiltrate or misuse secrets beyond the task's scope",
  externalNetwork: "external network access can leak repository content or fetch untrusted code",
  dependencyInstall: "installing dependencies executes third-party code inside the project",
  branchOps: "branch deletion and force operations can discard unreviewed history",
  prCreation: "opening a PR publishes work product and triggers CI under the human's identity",
  merge: "merging lands changes on shared branches other collaborators build on",
  productionAffecting: "production-affecting changes have blast radius beyond the repository",
};

const EXAMPLES: Record<ActionClass, string[]> = {
  destructiveShell: ["rm -rf", "git reset --hard", "sudo rm", "dd of=/dev/…", "DROP TABLE"],
  credentialUse: ["reading .env values", "using AWS/GH tokens", "authenticating to external services"],
  externalNetwork: ["curl/fetch to non-allowlisted hosts", "uploading files", "calling third-party APIs"],
  dependencyInstall: ["npm install <pkg>", "bun add <pkg>", "pip install <pkg>", "cargo add <pkg>"],
  branchOps: ["git branch -D", "git push --force", "rewriting published history"],
  prCreation: ["gh pr create", "opening a merge request"],
  merge: ["gh pr merge", "git merge into main", "clicking the merge button"],
  productionAffecting: ["deploys", "database migrations", "infra changes", "editing prod config or feature flags"],
};

/** Claude Code permission rules mapped from the action classes. */
export const CLAUDE_DENY_RULES = [
  "Bash(rm -rf /:*)",
  "Bash(git push --force:*)",
  "Bash(sudo rm:*)",
];

export const CLAUDE_ASK_RULES = [
  "Bash(rm -rf:*)",
  "Bash(git push:*)",
  "Bash(npm install:*)",
  "Bash(bun install:*)",
  "Bash(pip install:*)",
  "Bash(git branch -D:*)",
];

interface ApprovalEntry {
  decision: Decision;
  rationale: string;
  examples: string[];
}

interface ApprovalsPolicy {
  schemaVersion: number;
  neverAllow: string[];
  actions: Record<ActionClass, ApprovalEntry>;
}

const DECISIONS: readonly string[] = ["ask", "allow", "deny"];

/**
 * Validate decision overrides; returns errors (empty = valid). The never-allow
 * trio (destructiveShell, credentialUse, productionAffecting) is rejected at
 * this writer if any option attempts to set it to 'allow'.
 */
export function validateApprovalOptions(options: Record<string, unknown>): string[] {
  const errors: string[] = [];
  for (const cls of ACTION_CLASSES) {
    const value = options[cls];
    if (value === undefined) continue;
    if (typeof value !== "string" || !DECISIONS.includes(value)) {
      errors.push(`options.${cls} must be one of "ask" | "allow" | "deny"`);
      continue;
    }
    if (value === "allow" && NEVER_ALLOW.includes(cls)) {
      errors.push(`options.${cls} can never be "allow" — this class always requires a human decision`);
    }
  }
  return errors;
}

function buildPolicy(options: Record<string, unknown>): ApprovalsPolicy {
  const actions = {} as Record<ActionClass, ApprovalEntry>;
  for (const cls of ACTION_CLASSES) {
    const override = options[cls];
    const decision =
      typeof override === "string" && DECISIONS.includes(override)
        ? (override as Decision)
        : DEFAULT_DECISIONS[cls];
    actions[cls] = { decision, rationale: RATIONALES[cls], examples: EXAMPLES[cls] };
  }
  return {
    schemaVersion: 1,
    neverAllow: [...NEVER_ALLOW],
    actions,
  };
}

function moduleOptions(ctx: Ctx): Record<string, unknown> {
  return ctx.config.modules["approval-gates"]?.options ?? {};
}

function targetsClaudeCode(ctx: Ctx): boolean {
  return ctx.config.harnesses.includes("claude-code");
}

export const approvalGatesModule: AdeModule = {
  id: "approval-gates",
  title: "Human-in-the-Loop Approval Gates",
  category: "security",
  spec: "Human-in-the-loop approval gates",
  defaultEnabled: true,
  instructionBlocks: [
    {
      id: "approval-gates",
      title: "Human Approval Gates",
      content: [
        "This project gates high-risk actions behind explicit human approval (`.ade/policy/approvals.json`).",
        "- Eight action classes REQUIRE explicit human approval before execution: destructive shell commands, credential use, external network access, dependency installs, branch operations, PR creation, merges, and production-affecting changes.",
        "- Never execute an action in these classes on your own authority; state what you intend to do and wait for the human's decision.",
        "- When in doubt whether an action falls into a gated class, ASK — treat ambiguity as gated.",
        "- Production-affecting changes are DENIED without a human decision; there is no default-approve path for them.",
      ].join("\n"),
    },
  ],

  async detect(ctx: Ctx): Promise<Finding[]> {
    const errors = validateApprovalOptions(moduleOptions(ctx));
    if (errors.length > 0) {
      return errors.map((message): Finding => ({ level: "error", message }));
    }
    return [{ level: "ok", message: "approval-gate options valid" }];
  },

  async plan(ctx: Ctx): Promise<PlannedAction[]> {
    const actions: PlannedAction[] = [
      {
        kind: "write",
        path: APPROVALS_POLICY_PATH,
        description: "write approval-gate policy (eight gated action classes with decisions)",
      },
    ];
    if (targetsClaudeCode(ctx)) {
      actions.push({
        kind: "merge",
        path: CLAUDE_SETTINGS_PATH,
        description: "merge deny/ask permission rules for high-risk commands into Claude Code settings",
      });
    }
    return actions;
  },

  async apply(ctx: Ctx): Promise<ModuleResult> {
    const options = moduleOptions(ctx);
    const errors = validateApprovalOptions(options);
    if (errors.length > 0) {
      return {
        status: "failed",
        findings: errors.map((message): Finding => ({ level: "error", message })),
        wrotePaths: [],
      };
    }

    const findings: Finding[] = [];
    const wrotePaths: string[] = [];

    await writePolicy(ctx, APPROVALS_POLICY_PATH, buildPolicy(options));
    wrotePaths.push(APPROVALS_POLICY_PATH);
    findings.push({ level: "ok", message: `wrote ${APPROVALS_POLICY_PATH}` });

    let degraded = false;
    if (targetsClaudeCode(ctx)) {
      const merge = await mergeClaudeSettings(ctx, {
        permissions: { deny: CLAUDE_DENY_RULES, ask: CLAUDE_ASK_RULES },
      });
      findings.push(merge);
      if (merge.level === "ok") {
        wrotePaths.push(CLAUDE_SETTINGS_PATH);
      } else {
        degraded = true;
      }
    }
    return { status: degraded ? "degraded" : "applied", findings, wrotePaths };
  },

  async verify(ctx: Ctx) {
    const findings: Finding[] = [];
    const artifact = await verifyJsonArtifact(ctx, APPROVALS_POLICY_PATH);
    findings.push(artifact);
    if (artifact.level !== "ok") return { ok: false, findings };

    let ok = true;
    const policy = (await readJson(ctx, APPROVALS_POLICY_PATH)) as ApprovalsPolicy | null;
    const actions = policy?.actions ?? ({} as Record<string, ApprovalEntry | undefined>);

    const missing = ACTION_CLASSES.filter((cls) => {
      const entry = actions[cls];
      return (
        entry === undefined ||
        typeof entry.decision !== "string" ||
        !DECISIONS.includes(entry.decision) ||
        typeof entry.rationale !== "string" ||
        !Array.isArray(entry.examples)
      );
    });
    if (missing.length > 0) {
      ok = false;
      findings.push({
        level: "error",
        message: `${APPROVALS_POLICY_PATH} missing or malformed action classes: ${missing.join(", ")}`,
        remediation: "run `ade apply` to regenerate",
      });
    } else {
      findings.push({ level: "ok", message: "all eight gated action classes present" });
    }

    const escaped = NEVER_ALLOW.filter((cls) => actions[cls]?.decision === "allow");
    if (escaped.length > 0) {
      ok = false;
      findings.push({
        level: "error",
        message: `${APPROVALS_POLICY_PATH} sets never-allow classes to "allow": ${escaped.join(", ")}`,
        remediation: "run `ade apply` to restore the secure decisions",
      });
    }

    if (targetsClaudeCode(ctx)) {
      const settings = (await readJson(ctx, CLAUDE_SETTINGS_PATH)) as {
        permissions?: { deny?: unknown; ask?: unknown };
      } | null;
      const deny = Array.isArray(settings?.permissions?.deny) ? (settings.permissions.deny as unknown[]) : [];
      const ask = Array.isArray(settings?.permissions?.ask) ? (settings.permissions.ask as unknown[]) : [];
      const absent = [
        ...CLAUDE_DENY_RULES.filter((rule) => !deny.includes(rule)),
        ...CLAUDE_ASK_RULES.filter((rule) => !ask.includes(rule)),
      ];
      if (absent.length > 0) {
        ok = false;
        findings.push({
          level: "error",
          message: `${CLAUDE_SETTINGS_PATH} missing approval permission rules: ${absent.join(", ")}`,
          remediation: "run `ade apply` to re-merge the deny/ask rules",
        });
      } else {
        findings.push({ level: "ok", message: "Claude Code deny/ask permission rules present" });
      }
    }

    return { ok, findings };
  },
};
