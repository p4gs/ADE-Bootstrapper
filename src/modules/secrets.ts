/**
 * Module: secrets & credential hygiene.
 * Spec component: "Secrets and credential hygiene — secure bootstrap-time
 * secret provisioning, scoped credential access, secret leak prevention,
 * environment scrubbing, and protection against accidental inclusion of
 * secrets in code, logs, prompts, or commits (e.g. TruffleHog with
 * pre-commit hook)."
 * Boundary controlled: the git commit boundary + the prompt/log boundary.
 *
 * Integration over rebuild: TruffleHog is the scanner. When the pre-commit
 * framework is present we emit a `.pre-commit-config.yaml`; otherwise we
 * install a native `.git/hooks/pre-commit` shim that chains any pre-existing
 * hook (non-destructive) and runs TruffleHog on the staged range.
 */
import { join } from "node:path";
import { chmod, mkdir, rename } from "node:fs/promises";
import { ensureLines, readIfExists } from "../fsutil.ts";
import { toolFinding, verifyJsonArtifact, writePolicy } from "./_shared.ts";
import type { AdeModule, Ctx, Finding, ModuleResult, PlannedAction } from "../types.ts";

export const SECRETS_POLICY_PATH = ".ade/policy/secrets.json";
export const PRECOMMIT_CONFIG_PATH = ".pre-commit-config.yaml";
export const HOOK_MARKER = "# ade-secrets-hook v1";
export const CHAINED_HOOK_NAME = "pre-commit.pre-ade";

const GITIGNORE_LINES = [
  ".env",
  ".env.*",
  "*.pem",
  "*.key",
  "id_rsa",
  "id_ed25519",
  ".ade/memory-store/",
  ".ade/audit/",
];

/** The native pre-commit shim. Warns-and-passes when trufflehog is missing (never bricks commits). */
export function hookScript(): string {
  return `#!/bin/sh
${HOOK_MARKER}
# Installed by ADE Bootstrapper (secrets module). Chains any pre-existing hook.
# Blocks commits containing verified secrets using TruffleHog.

if [ -x "$(dirname "$0")/${CHAINED_HOOK_NAME}" ]; then
  "$(dirname "$0")/${CHAINED_HOOK_NAME}" "$@" || exit $?
fi

if command -v trufflehog >/dev/null 2>&1; then
  trufflehog git "file://$(git rev-parse --show-toplevel)" --since-commit HEAD --results=verified --fail --no-update >/dev/null 2>&1
  status=$?
  if [ $status -ne 0 ]; then
    echo "ade: commit BLOCKED — TruffleHog found a verified secret in the staged changes." >&2
    echo "ade: remove the secret (and rotate it), then commit again." >&2
    exit 1
  fi
else
  echo "ade: warning — trufflehog not installed; secret scan skipped (install: brew install trufflehog)" >&2
fi
exit 0
`;
}

/** pre-commit framework config (static template — the framework requires YAML). */
export function preCommitConfig(): string {
  return `# Managed by ADE Bootstrapper (secrets module).
repos:
  - repo: https://github.com/trufflesecurity/trufflehog
    rev: main
    hooks:
      - id: trufflehog
        name: TruffleHog secret scan
        entry: trufflehog git file://. --since-commit HEAD --results=verified --fail
        language: system
        stages: ["pre-commit"]
`;
}

function secretsPolicy(scanner: string | null): Record<string, unknown> {
  return {
    schemaVersion: 1,
    scanner: scanner ?? "none",
    scanScope: "staged-changes",
    blockOn: "verified-secrets",
    environmentScrubbing: {
      neverEcho: ["*_KEY", "*_TOKEN", "*_SECRET", "*_PASSWORD", "AWS_*", "GITHUB_TOKEN"],
      rule: "never print environment variable values into code, logs, prompts, or commits",
    },
    credentialScoping: {
      rule: "prefer short-lived, least-privilege credentials injected at the tool boundary; never commit long-lived credentials",
    },
  };
}

export const secretsModule: AdeModule = {
  id: "secrets",
  title: "Secrets & Credential Hygiene",
  category: "security",
  spec: "Secrets and credential hygiene",
  defaultEnabled: true,
  instructionBlocks: [
    {
      id: "secrets",
      title: "Secrets & Credential Hygiene",
      content: [
        "- NEVER write secret values into code, logs, prompts, commit messages, or generated files.",
        "- NEVER echo environment variables that look like credentials (`*_KEY`, `*_TOKEN`, `*_SECRET`, `*_PASSWORD`).",
        "- A pre-commit secret scan (TruffleHog) guards this repo; if it blocks a commit, remove AND rotate the secret — do not bypass the hook.",
        "- Use scoped, short-lived credentials; request the human provision them at the boundary (env injection), never inline.",
      ].join("\n"),
    },
  ],

  async detect(ctx: Ctx): Promise<Finding[]> {
    const findings: Finding[] = [
      toolFinding(ctx, "trufflehog", "install TruffleHog (e.g. `brew install trufflehog`) to enable commit-boundary secret scanning"),
      toolFinding(ctx, "pre-commit", "optional: install the pre-commit framework to manage hooks declaratively"),
    ];
    if (!ctx.isGitRepo) {
      findings.push({
        level: "degraded",
        message: "target is not a git repository — commit-boundary scanning unavailable",
        remediation: "run `git init` in the target, then `ade apply`",
      });
    }
    return findings;
  },

  async plan(ctx: Ctx): Promise<PlannedAction[]> {
    const actions: PlannedAction[] = [
      { kind: "write", path: SECRETS_POLICY_PATH, description: "write secrets hygiene policy" },
      { kind: "append", path: ".gitignore", description: "ensure secret-bearing paths are git-ignored" },
    ];
    if (ctx.isGitRepo) {
      if (ctx.tools["pre-commit"]?.present === true) {
        actions.push({ kind: "write", path: PRECOMMIT_CONFIG_PATH, description: "write pre-commit framework config with TruffleHog hook" });
      } else {
        actions.push({ kind: "hook", path: ".git/hooks/pre-commit", description: "install native pre-commit shim (chains existing hook) running TruffleHog" });
      }
    }
    return actions;
  },

  async apply(ctx: Ctx): Promise<ModuleResult> {
    const findings: Finding[] = [];
    const wrotePaths: string[] = [];
    const scanner = ctx.tools["trufflehog"]?.present === true ? "trufflehog" : null;

    await writePolicy(ctx, SECRETS_POLICY_PATH, secretsPolicy(scanner));
    wrotePaths.push(SECRETS_POLICY_PATH);

    await ensureLines(join(ctx.targetDir, ".gitignore"), GITIGNORE_LINES, "ADE Bootstrapper — secrets hygiene");
    findings.push({ level: "ok", message: ".gitignore covers secret-bearing paths" });

    if (!ctx.isGitRepo) {
      findings.push({
        level: "degraded",
        message: "not a git repository — skipped commit-hook installation",
        remediation: "run `git init`, then `ade apply`",
      });
      return { status: "degraded", findings, wrotePaths };
    }

    if (ctx.tools["pre-commit"]?.present === true) {
      const existing = await readIfExists(join(ctx.targetDir, PRECOMMIT_CONFIG_PATH));
      if (existing === null) {
        await ctx.artifacts.write(PRECOMMIT_CONFIG_PATH, preCommitConfig());
        wrotePaths.push(PRECOMMIT_CONFIG_PATH);
        findings.push({ level: "ok", message: "wrote .pre-commit-config.yaml with TruffleHog hook (run `pre-commit install`)" });
      } else if (existing.includes("trufflehog")) {
        findings.push({ level: "ok", message: ".pre-commit-config.yaml already includes a trufflehog hook" });
      } else {
        findings.push({
          level: "degraded",
          message: ".pre-commit-config.yaml exists without a trufflehog hook — not modifying a user-owned YAML file",
          remediation: "add the trufflehog hook to your .pre-commit-config.yaml (see .ade/policy/secrets.json)",
        });
      }
    } else {
      const hooksDir = join(ctx.targetDir, ".git", "hooks");
      const hookPath = join(hooksDir, "pre-commit");
      const existing = await readIfExists(hookPath);
      if (existing !== null && !existing.includes(HOOK_MARKER)) {
        await rename(hookPath, join(hooksDir, CHAINED_HOOK_NAME));
        findings.push({ level: "info", message: `existing pre-commit hook preserved as ${CHAINED_HOOK_NAME} and chained` });
      }
      if (existing === null || !existing.includes(HOOK_MARKER)) {
        await mkdir(hooksDir, { recursive: true });
        await Bun.write(hookPath, hookScript());
        await chmod(hookPath, 0o755);
        findings.push({ level: "ok", message: "installed .git/hooks/pre-commit secret-scan shim" });
      } else {
        await Bun.write(hookPath, hookScript());
        await chmod(hookPath, 0o755);
        findings.push({ level: "ok", message: "refreshed .git/hooks/pre-commit secret-scan shim" });
      }
    }

    if (scanner === null) {
      findings.push({
        level: "degraded",
        message: "trufflehog not installed — hook will warn instead of scanning",
        remediation: "install TruffleHog (e.g. `brew install trufflehog`)",
      });
      return { status: "degraded", findings, wrotePaths };
    }
    return { status: "applied", findings, wrotePaths };
  },

  async verify(ctx: Ctx) {
    const findings: Finding[] = [await verifyJsonArtifact(ctx, SECRETS_POLICY_PATH)];
    let ok = findings.every((finding) => finding.level === "ok");

    const gitignore = (await readIfExists(join(ctx.targetDir, ".gitignore"))) ?? "";
    const missing = GITIGNORE_LINES.filter((line) => !gitignore.split("\n").map((l) => l.trim()).includes(line));
    if (missing.length > 0) {
      ok = false;
      findings.push({ level: "error", message: `.gitignore missing entries: ${missing.join(", ")}`, remediation: "run `ade apply`" });
    }

    if (ctx.isGitRepo && ctx.tools["pre-commit"]?.present !== true) {
      const hook = await readIfExists(join(ctx.targetDir, ".git", "hooks", "pre-commit"));
      if (hook === null || !hook.includes(HOOK_MARKER)) {
        ok = false;
        findings.push({ level: "error", message: "pre-commit secret-scan shim not installed", remediation: "run `ade apply`" });
      } else {
        findings.push({ level: "ok", message: "pre-commit secret-scan shim installed" });
      }
    }
    if (!ctx.isGitRepo) {
      findings.push({ level: "degraded", message: "not a git repository — commit-boundary scan not verifiable" });
    }
    return { ok, findings };
  },
};
