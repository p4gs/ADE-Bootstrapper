/**
 * Module: token efficiency (RTK integration).
 * Spec component: "Lossless or minimally lossy token efficiency mechanisms —
 * shell-output filtering, grouping, truncation, and deduplication so
 * high-volume command output does not flood the context window (e.g. RTK)."
 * Boundary controlled: the shell-output/context-window boundary (token waste).
 *
 * Integration over rebuild: RTK is the reducer. When present, the policy
 * records the detected version and enables shell-boundary wrapping; when
 * absent, the policy is still written (enabled:false) so the contract is
 * explicit and verify can re-derive expectations from the live machine.
 */
import { readJson, toolFinding, verifyJsonArtifact, writePolicy } from "./_shared.ts";
import type { AdeModule, Ctx, Finding, ModuleResult, PlannedAction } from "../types.ts";

export const TOKEN_EFFICIENCY_POLICY_PATH = ".ade/policy/token-efficiency.json";

export const RTK_INSTALL_GUIDANCE =
  "install rtk (github.com/rtk-ai/rtk — cuts common dev-command output 60-90%)";

interface TokenEfficiencyPolicy {
  schemaVersion: number;
  enabled: boolean;
  tool: string;
  toolVersion?: string;
  integration: string;
  mechanisms: string[];
  guarantee: string;
}

function buildPolicy(ctx: Ctx): TokenEfficiencyPolicy {
  const rtk = ctx.tools["rtk"];
  const present = rtk?.present === true;
  const policy: TokenEfficiencyPolicy = {
    schemaVersion: 1,
    enabled: present,
    tool: present ? "rtk" : "none",
    integration: "shell-boundary",
    mechanisms: ["filtering", "grouping", "truncation", "deduplication"],
    guarantee:
      "reversible/semantically-lossless transforms preferred over naive summarization",
  };
  if (present && rtk?.version !== undefined) {
    policy.toolVersion = rtk.version;
  }
  return policy;
}

export const tokenEfficiencyModule: AdeModule = {
  id: "token-efficiency",
  title: "Token Efficiency",
  category: "efficiency",
  spec: "Lossless or minimally lossy token efficiency mechanisms",
  defaultEnabled: true,
  instructionBlocks: [
    {
      id: "token-efficiency",
      title: "Token Efficiency",
      content: [
        "This project has a token-efficiency contract at `.ade/policy/token-efficiency.json`.",
        "- Prefer rtk-wrapped commands for high-volume output: test runs, builds, logs, diffs, and file listings.",
        "- Never paste multi-hundred-line raw output into context when a filtered form answers the question.",
        "- Token efficiency must never drop error details — keep failures verbatim.",
      ].join("\n"),
    },
  ],

  async detect(ctx: Ctx): Promise<Finding[]> {
    return [toolFinding(ctx, "rtk", RTK_INSTALL_GUIDANCE)];
  },

  async plan(ctx: Ctx): Promise<PlannedAction[]> {
    void ctx;
    return [
      {
        kind: "write" as const,
        path: TOKEN_EFFICIENCY_POLICY_PATH,
        description: "write token-efficiency policy (rtk shell-boundary integration)",
      },
    ];
  },

  async apply(ctx: Ctx): Promise<ModuleResult> {
    const policy = buildPolicy(ctx);
    await writePolicy(ctx, TOKEN_EFFICIENCY_POLICY_PATH, policy);
    if (!policy.enabled) {
      return {
        status: "degraded" as const,
        findings: [
          { level: "ok" as const, message: `wrote ${TOKEN_EFFICIENCY_POLICY_PATH} (disabled)` },
          {
            level: "degraded" as const,
            message: "rtk not installed — shell-output token reduction unavailable",
            remediation: RTK_INSTALL_GUIDANCE,
          },
        ],
        wrotePaths: [TOKEN_EFFICIENCY_POLICY_PATH],
      };
    }
    return {
      status: "applied" as const,
      findings: [{ level: "ok" as const, message: `wrote ${TOKEN_EFFICIENCY_POLICY_PATH}` }],
      wrotePaths: [TOKEN_EFFICIENCY_POLICY_PATH],
    };
  },

  async verify(ctx: Ctx) {
    const artifact = await verifyJsonArtifact(ctx, TOKEN_EFFICIENCY_POLICY_PATH);
    if (artifact.level !== "ok") return { ok: false, findings: [artifact] };
    const parsed = (await readJson(ctx, TOKEN_EFFICIENCY_POLICY_PATH)) as
      | Partial<TokenEfficiencyPolicy>
      | null;
    if (parsed === null || typeof parsed !== "object" || typeof parsed.enabled !== "boolean") {
      return {
        ok: false,
        findings: [
          {
            level: "error",
            message: `${TOKEN_EFFICIENCY_POLICY_PATH} missing boolean 'enabled'`,
            remediation: "run `ade apply` to regenerate",
          },
        ],
      };
    }
    const rtkPresent = ctx.tools["rtk"]?.present === true;
    if (parsed.enabled !== rtkPresent) {
      return {
        ok: false,
        findings: [
          {
            level: "error",
            message: `${TOKEN_EFFICIENCY_POLICY_PATH} enabled=${parsed.enabled} but rtk ${rtkPresent ? "is" : "is not"} installed`,
            remediation: "run `ade apply` to re-derive the policy from the current machine",
          },
        ],
      };
    }
    return { ok: true, findings: [artifact] };
  },
};
