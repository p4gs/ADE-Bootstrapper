/**
 * Module: AI-native sandboxing.
 * Spec component: "AI-native sandboxing — declarative filesystem/network/
 * credential policy for harness-driven execution, with kernel-level
 * enforcement via nono (nono.sh) when installed."
 * Boundary controlled: the shell/filesystem/network boundary.
 *
 * Integration over rebuild: nono is the enforcer. The module always writes
 * the declarative policy (`.ade/policy/sandbox.json`) so harnesses and
 * humans share one contract; when nono is absent the policy is advisory and
 * the module reports 'degraded' with install guidance. For claude-code
 * targets, the deny-read surface is additionally mapped into
 * `.claude/settings.json` permissions so the harness itself refuses
 * credential-file reads.
 */
import { mergeClaudeSettings, CLAUDE_SETTINGS_PATH } from "../harness/claude.ts";
import { readJson, toolFinding, verifyJsonArtifact, writePolicy } from "./_shared.ts";
import type { AdeModule, Ctx, Finding, ModuleResult, PlannedAction } from "../types.ts";

export const SANDBOX_POLICY_PATH = ".ade/policy/sandbox.json";

/** Default egress allowlist: package registries + source forge only. */
export const DEFAULT_NETWORK_ALLOWLIST = [
  "registry.npmjs.org",
  "pypi.org",
  "crates.io",
  "proxy.golang.org",
  "github.com",
];

/** Claude Code permission entries denying reads of credential-bearing files. */
export const CLAUDE_DENY_READ = [
  "Read(./.env)",
  "Read(./.env.*)",
  "Read(~/.ssh/**)",
  "Read(~/.aws/**)",
];

const NONO_REMEDIATION = "install nono (nono.sh) for kernel-enforced sandboxing";

interface SandboxPolicy {
  schemaVersion: number;
  enforcement: string;
  filesystem: {
    writeScope: string[];
    denyWrite: string[];
    denyRead: string[];
  };
  network: {
    default: string;
    allowlist: string[];
  };
  credentials: {
    injection: string;
    rule: string;
  };
}

/** Validate sandbox options; returns errors (empty = valid). */
export function validateSandboxOptions(options: Record<string, unknown>): string[] {
  const errors: string[] = [];
  for (const key of ["allowHosts", "denyRead", "denyWrite"]) {
    const value = options[key];
    if (
      value !== undefined &&
      (!Array.isArray(value) || !value.every((entry) => typeof entry === "string" && entry.length > 0))
    ) {
      errors.push(`options.${key} must be an array of non-empty strings`);
    }
  }
  return errors;
}

/** Append extras to a base list, deduplicated, base order preserved (deterministic). */
function withExtras(base: string[], extras: unknown): string[] {
  const out = [...base];
  if (Array.isArray(extras)) {
    for (const entry of extras) {
      if (typeof entry === "string" && !out.includes(entry)) out.push(entry);
    }
  }
  return out;
}

/** Declarative policy — paths are placeholders (`<repo>`, `~`), never machine-absolute. */
function buildPolicy(options: Record<string, unknown>, enforcer: string | null): SandboxPolicy {
  return {
    schemaVersion: 1,
    enforcement: enforcer ?? "advisory",
    filesystem: {
      writeScope: ["<repo>"],
      denyWrite: withExtras(["~/.ssh", "~/.aws", "~/.claude", "system paths"], options["denyWrite"]),
      denyRead: withExtras([".env", ".env.*", "~/.ssh/**"], options["denyRead"]),
    },
    network: {
      default: "deny",
      allowlist: withExtras(DEFAULT_NETWORK_ALLOWLIST, options["allowHosts"]),
    },
    credentials: {
      injection: "at-boundary",
      rule: "secrets are injected by the sandbox at exec time, never stored in the environment or files",
    },
  };
}

function moduleOptions(ctx: Ctx): Record<string, unknown> {
  return ctx.config.modules["sandbox"]?.options ?? {};
}

function targetsClaudeCode(ctx: Ctx): boolean {
  return ctx.config.harnesses.includes("claude-code");
}

export const sandboxModule: AdeModule = {
  id: "sandbox",
  title: "AI-Native Sandboxing",
  category: "security",
  spec: "AI-native sandboxing",
  defaultEnabled: true,
  instructionBlocks: [
    {
      id: "sandbox",
      title: "Sandbox Policy",
      content: [
        "This project has a sandbox contract at `.ade/policy/sandbox.json`. Operate inside it.",
        "- Write only inside this repository; NEVER write to `~/.ssh`, `~/.aws`, `~/.claude`, or system paths.",
        "- NEVER attempt to read credential files (`.env`, `.env.*`, `~/.ssh/**`, `~/.aws/**`).",
        "- Network egress is deny-by-default with a package-registry allowlist; never attempt to bypass, tunnel, or proxy around network controls.",
        "- Secrets are injected at the sandbox boundary at exec time — never persist them to the environment or files.",
        "- If a task needs access outside this policy, STOP and ask a human — do not work around the sandbox.",
      ].join("\n"),
    },
  ],

  async detect(ctx: Ctx): Promise<Finding[]> {
    const findings: Finding[] = [toolFinding(ctx, "nono", NONO_REMEDIATION)];
    const errors = validateSandboxOptions(moduleOptions(ctx));
    findings.push(
      ...errors.map((message): Finding => ({ level: "error", message })),
    );
    return findings;
  },

  async plan(ctx: Ctx): Promise<PlannedAction[]> {
    const actions: PlannedAction[] = [
      {
        kind: "write",
        path: SANDBOX_POLICY_PATH,
        description: "write sandbox policy (filesystem scope, deny-by-default network, credential injection)",
      },
    ];
    if (targetsClaudeCode(ctx)) {
      actions.push({
        kind: "merge",
        path: CLAUDE_SETTINGS_PATH,
        description: "merge credential-file deny-read permissions into Claude Code settings",
      });
    }
    return actions;
  },

  async apply(ctx: Ctx): Promise<ModuleResult> {
    const options = moduleOptions(ctx);
    const errors = validateSandboxOptions(options);
    if (errors.length > 0) {
      return {
        status: "failed",
        findings: errors.map((message): Finding => ({ level: "error", message })),
        wrotePaths: [],
      };
    }

    const findings: Finding[] = [];
    const wrotePaths: string[] = [];
    const enforcer = ctx.tools["nono"]?.present === true ? "nono" : null;

    await writePolicy(ctx, SANDBOX_POLICY_PATH, buildPolicy(options, enforcer));
    wrotePaths.push(SANDBOX_POLICY_PATH);
    findings.push({ level: "ok", message: `wrote ${SANDBOX_POLICY_PATH}` });

    let degraded = false;
    if (targetsClaudeCode(ctx)) {
      const merge = await mergeClaudeSettings(ctx, { permissions: { deny: CLAUDE_DENY_READ } });
      findings.push(merge);
      if (merge.level === "ok") {
        wrotePaths.push(CLAUDE_SETTINGS_PATH);
      } else {
        degraded = true;
      }
    }

    if (enforcer === null) {
      degraded = true;
      findings.push({
        level: "degraded",
        message: "nono not installed — sandbox policy is advisory (harness-instruction level only)",
        remediation: NONO_REMEDIATION,
      });
    }
    return { status: degraded ? "degraded" : "applied", findings, wrotePaths };
  },

  async verify(ctx: Ctx) {
    const findings: Finding[] = [];
    const artifact = await verifyJsonArtifact(ctx, SANDBOX_POLICY_PATH);
    findings.push(artifact);
    if (artifact.level !== "ok") return { ok: false, findings };

    let ok = true;
    const policy = (await readJson(ctx, SANDBOX_POLICY_PATH)) as SandboxPolicy | null;
    if (
      policy?.network?.default !== "deny" ||
      !Array.isArray(policy.network.allowlist)
    ) {
      ok = false;
      findings.push({
        level: "error",
        message: `${SANDBOX_POLICY_PATH} network policy is not deny-with-allowlist`,
        remediation: "run `ade apply` to regenerate",
      });
    } else {
      findings.push({ level: "ok", message: "network policy is deny-by-default with allowlist" });
    }

    if (targetsClaudeCode(ctx)) {
      const settings = (await readJson(ctx, CLAUDE_SETTINGS_PATH)) as
        | { permissions?: { deny?: unknown } }
        | null;
      const deny = settings?.permissions?.deny;
      const missing = CLAUDE_DENY_READ.filter(
        (entry) => !Array.isArray(deny) || !deny.includes(entry),
      );
      if (missing.length > 0) {
        ok = false;
        findings.push({
          level: "error",
          message: `${CLAUDE_SETTINGS_PATH} missing deny-read entries: ${missing.join(", ")}`,
          remediation: "run `ade apply`",
        });
      } else {
        findings.push({ level: "ok", message: `${CLAUDE_SETTINGS_PATH} denies credential-file reads` });
      }
    }
    return { ok, findings };
  },
};
