/**
 * Module: cost & token budget governance.
 * Spec component: "Cost and token budget governance — model routing controls,
 * per-session and per-project token/cost limits, budget alerts, and policies
 * to prevent runaway harness activity and uncontrolled spend."
 * Boundary controlled: the spend boundary (model routing + budget contract).
 */
import { readJson, verifyJsonArtifact, writePolicy } from "./_shared.ts";
import type { AdeModule, Ctx, Finding } from "../types.ts";

export const BUDGET_POLICY_PATH = ".ade/policy/budget.json";

interface BudgetPolicy {
  schemaVersion: number;
  limits: {
    perSessionTokens: number;
    perProjectDailyTokens: number;
    perSessionCostUsd: number;
    perProjectDailyCostUsd: number;
  };
  alerts: { warnAtFraction: number };
  routing: {
    default: string;
    cheap: string;
    escalation: string;
    policy: string;
  };
}

function defaultPolicy(): BudgetPolicy {
  return {
    schemaVersion: 1,
    limits: {
      perSessionTokens: 2_000_000,
      perProjectDailyTokens: 10_000_000,
      perSessionCostUsd: 25,
      perProjectDailyCostUsd: 100,
    },
    alerts: { warnAtFraction: 0.8 },
    routing: {
      default: "balanced",
      cheap: "small-model for mechanical edits, summaries, and classification",
      escalation: "large-model only for architecture, security, and cross-cutting design",
      policy: "route to the cheapest model that meets the task's quality bar",
    },
  };
}

/** Validate numeric budget options; returns errors (empty = valid). */
export function validateBudgetOptions(options: Record<string, unknown>): string[] {
  const errors: string[] = [];
  const numericKeys = [
    "perSessionTokens",
    "perProjectDailyTokens",
    "perSessionCostUsd",
    "perProjectDailyCostUsd",
  ];
  for (const key of numericKeys) {
    const value = options[key];
    if (value !== undefined && (typeof value !== "number" || !Number.isFinite(value) || value <= 0)) {
      errors.push(`options.${key} must be a positive number`);
    }
  }
  const warn = options["warnAtFraction"];
  if (warn !== undefined && (typeof warn !== "number" || warn <= 0 || warn >= 1)) {
    errors.push("options.warnAtFraction must be a number between 0 and 1");
  }
  return errors;
}

function buildPolicy(options: Record<string, unknown>): BudgetPolicy {
  const policy = defaultPolicy();
  const limits = policy.limits as unknown as Record<string, number>;
  for (const key of Object.keys(policy.limits)) {
    const value = options[key];
    if (typeof value === "number") limits[key] = value;
  }
  if (typeof options["warnAtFraction"] === "number") {
    policy.alerts.warnAtFraction = options["warnAtFraction"] as number;
  }
  return policy;
}

function moduleOptions(ctx: Ctx): Record<string, unknown> {
  return ctx.config.modules["cost-governance"]?.options ?? {};
}

export const costGovernanceModule: AdeModule = {
  id: "cost-governance",
  title: "Cost & Token Budget Governance",
  category: "governance",
  spec: "Cost and token budget governance",
  defaultEnabled: true,
  instructionBlocks: [
    {
      id: "cost-governance",
      title: "Cost & Token Budget",
      content: [
        "This project has a token/cost budget contract at `.ade/policy/budget.json`.",
        "- Prefer the cheapest model that meets the task's quality bar; escalate model tier only for architecture, security, or cross-cutting design work.",
        "- Avoid re-reading large files you have already read; use the context artifacts in `.ade/context/` first.",
        "- Stop and surface a budget warning instead of looping when a task repeatedly fails the same way.",
      ].join("\n"),
    },
  ],

  async detect(ctx: Ctx): Promise<Finding[]> {
    const errors = validateBudgetOptions(moduleOptions(ctx));
    if (errors.length > 0) {
      return errors.map((message) => ({ level: "error" as const, message }));
    }
    return [{ level: "ok", message: "budget options valid" }];
  },

  async plan(ctx: Ctx) {
    void ctx;
    return [
      {
        kind: "write" as const,
        path: BUDGET_POLICY_PATH,
        description: "write token/cost budget policy (limits, alerts, model routing)",
      },
    ];
  },

  async apply(ctx: Ctx) {
    const options = moduleOptions(ctx);
    const errors = validateBudgetOptions(options);
    if (errors.length > 0) {
      return {
        status: "failed" as const,
        findings: errors.map((message) => ({ level: "error" as const, message })),
        wrotePaths: [],
      };
    }
    await writePolicy(ctx, BUDGET_POLICY_PATH, buildPolicy(options));
    return {
      status: "applied" as const,
      findings: [{ level: "ok" as const, message: `wrote ${BUDGET_POLICY_PATH}` }],
      wrotePaths: [BUDGET_POLICY_PATH],
    };
  },

  async verify(ctx: Ctx) {
    const artifact = await verifyJsonArtifact(ctx, BUDGET_POLICY_PATH);
    if (artifact.level !== "ok") return { ok: false, findings: [artifact] };
    const parsed = (await readJson(ctx, BUDGET_POLICY_PATH)) as BudgetPolicy | null;
    if (parsed === null) return { ok: false, findings: [artifact] };
    const values = Object.values(parsed.limits ?? {});
    const numbersValid =
      values.length === 4 && values.every((value) => typeof value === "number" && value > 0);
    if (!numbersValid) {
      return {
        ok: false,
        findings: [
          {
            level: "error",
            message: `${BUDGET_POLICY_PATH} contains non-positive or missing limits`,
            remediation: "run `ade apply` to regenerate",
          },
        ],
      };
    }
    return { ok: true, findings: [artifact] };
  },
};
