/**
 * Module: tamper-evident observability & audit logging.
 * Spec component: "Tamper-evident observability and audit logging — a
 * hash-chained, append-only audit trail of all harness tool activity so that
 * what an agent did is reconstructable and undetectable modification is
 * impossible."
 * Boundary controlled: the accountability boundary (forensics/compliance) —
 * every tool invocation leaves a chained record; editing or deleting history
 * breaks every subsequent link and is caught by `ade audit verify`.
 *
 * The chain itself (`.ade/audit/log.jsonl`) is initialized by the ADE
 * pipeline; this module ships the surface around it: the audit README, the
 * self-contained Claude Code PostToolUse hook that appends entries in the
 * exact `src/audit.ts` format, and the git-ignore default for the log.
 */
import { join } from "node:path";
import { ensureLines, readIfExists } from "../fsutil.ts";
import { readJson } from "./_shared.ts";
import { parseLog, verifyChain } from "../audit.ts";
import { AUDIT_GENESIS } from "../version.ts";
import { CLAUDE_SETTINGS_PATH, wireClaudeHook } from "../harness/claude.ts";
import type { AdeModule, Ctx, Finding, ModuleResult, PlannedAction } from "../types.ts";

export const AUDIT_README_PATH = ".ade/audit/README.md";
export const AUDIT_HOOK_PATH = ".ade/hooks/audit-log.ts";
export const AUDIT_LOG_PATH = ".ade/audit/log.jsonl";
export const AUDIT_HOOK_COMMAND = "bun .ade/hooks/audit-log.ts";
const GITIGNORE_LABEL = "ADE Bootstrapper — observability";

/** README shipped into `.ade/audit/` documenting the chain contract. */
export function auditReadme(): string {
  return `# ADE Audit Log

Managed by ADE Bootstrapper (observability module).

\`log.jsonl\` in this directory is a **tamper-evident, hash-chained audit log**
of ADE pipeline events and harness tool activity.

- Each line is one JSON entry: \`{ts, actor, action, target, result, prev, hash}\`.
- \`hash = sha256(prev + JSON.stringify({ts, actor, action, target, result, prev}))\`
  with that exact key order.
- The first entry chains from the fixed genesis value \`${AUDIT_GENESIS}\`.
- Any edit, deletion, or reordering of a historical entry breaks every
  subsequent link in the chain.

Verify the chain at any time:

\`\`\`sh
ade audit verify
\`\`\`

Do NOT edit or delete \`log.jsonl\`. The chain is initialized by the ADE
pipeline and extended by harness hooks (e.g. the Claude Code PostToolUse hook
at \`.ade/hooks/audit-log.ts\`).
`;
}

/**
 * The Claude Code PostToolUse hook script. SELF-CONTAINED by design — this
 * file deploys into target repos, so it must import nothing from ADE and
 * produce entries byte-compatible with `src/audit.ts` (`ade audit verify`).
 * String.raw keeps the embedded escapes (\n, \-) literal in the emitted file.
 */
export function auditHookScript(): string {
  return String.raw`#!/usr/bin/env bun
/**
 * ADE audit hook — Claude Code PostToolUse.
 * Installed by ADE Bootstrapper (observability module). SELF-CONTAINED:
 * deployed into target repos; imports nothing from ADE.
 *
 * Reads the PostToolUse hook event JSON from stdin and appends one
 * hash-chained entry to .ade/audit/log.jsonl (cwd = project root when Claude
 * Code runs hooks). Chain format, verified by "ade audit verify":
 *   hash = sha256(prev + JSON.stringify({ts, actor, action, target, result, prev}))
 * with genesis prev "ade-genesis-v1". Never blocks the harness: always exits 0.
 */
import { mkdir } from "node:fs/promises";

const LOG_PATH = ".ade/audit/log.jsonl";
const GENESIS = "ade-genesis-v1";

function sha256Hex(text: string): string {
  const hasher = new Bun.CryptoHasher("sha256");
  hasher.update(text);
  return hasher.digest("hex");
}

/** One-line safe summary: redact credential-shaped values, cap at 120 chars. */
function safeTarget(toolInput: unknown): string {
  const serialized = JSON.stringify(toolInput === undefined ? {} : toolInput) ?? "{}";
  return serialized.replace(/[A-Za-z0-9_\-]{20,}/g, "[redacted]").slice(0, 120);
}

async function main(): Promise<void> {
  let event: Record<string, unknown> = {};
  try {
    const parsed: unknown = JSON.parse(await Bun.stdin.text());
    if (parsed === null || typeof parsed !== "object" || Array.isArray(parsed)) return;
    event = parsed as Record<string, unknown>;
  } catch {
    return; // malformed hook payload — never block the harness
  }
  const toolName = typeof event["tool_name"] === "string" ? (event["tool_name"] as string) : "unknown";

  const logFile = Bun.file(LOG_PATH);
  const existing = (await logFile.exists()) ? await logFile.text() : "";
  let prev = GENESIS;
  const lines = existing.split("\n").filter((line) => line.trim().length > 0);
  if (lines.length > 0) {
    try {
      const last = JSON.parse(lines[lines.length - 1] as string) as { hash?: unknown };
      if (typeof last.hash === "string") prev = last.hash;
    } catch {
      // unreadable tail — chain from genesis; verification will flag the break
    }
  }

  const body = {
    ts: new Date().toISOString(),
    actor: "harness-hook",
    action: "tool." + toolName,
    target: safeTarget(event["tool_input"]),
    result: "observed",
    prev,
  };
  const entry = { ...body, hash: sha256Hex(prev + JSON.stringify(body)) };
  await mkdir(".ade/audit", { recursive: true });
  await Bun.write(LOG_PATH, existing + JSON.stringify(entry) + "\n");
}

await main();
`;
}

function moduleOptions(ctx: Ctx): Record<string, unknown> {
  return ctx.config.modules["observability"]?.options ?? {};
}

function targetsClaudeCode(ctx: Ctx): boolean {
  return ctx.config.harnesses.includes("claude-code");
}

/** Structural check that `.claude/settings.json` wires the PostToolUse audit hook. */
function settingsWireHook(parsed: unknown): boolean {
  if (parsed === null || typeof parsed !== "object" || Array.isArray(parsed)) return false;
  const hooks = (parsed as Record<string, unknown>)["hooks"];
  if (hooks === null || typeof hooks !== "object" || Array.isArray(hooks)) return false;
  const post = (hooks as Record<string, unknown>)["PostToolUse"];
  return Array.isArray(post) && JSON.stringify(post).includes(AUDIT_HOOK_COMMAND);
}

export const observabilityModule: AdeModule = {
  id: "observability",
  title: "Tamper-Evident Observability",
  category: "governance",
  spec: "Tamper-evident observability and audit logging",
  defaultEnabled: true,
  instructionBlocks: [
    {
      id: "observability",
      title: "Audit Logging",
      content: [
        "- All tool activity in this repo is audit-logged to a tamper-evident hash chain at `.ade/audit/log.jsonl`.",
        "- NEVER edit, delete, truncate, or reorder `.ade/audit/log.jsonl` — any change breaks the chain and is flagged by `ade audit verify`.",
        "- Treat the audit log as append-only forensic evidence; only the ADE pipeline and installed hooks write it. If it interferes with a task, surface that to the human instead of touching it.",
      ].join("\n"),
    },
  ],

  async detect(ctx: Ctx): Promise<Finding[]> {
    const findings: Finding[] = [
      { level: "ok", message: "audit chain requires no external tools (Bun-native sha256)" },
    ];
    if (targetsClaudeCode(ctx)) {
      findings.push({
        level: "ok",
        message: "claude-code targeted — per-tool activity logging via PostToolUse hook available",
      });
    } else {
      findings.push({
        level: "info",
        message: "no hook-capable harness targeted — audit chain records pipeline events only",
        remediation: 'add "claude-code" to harnesses in ade.json to capture per-tool activity',
      });
    }
    return findings;
  },

  async plan(ctx: Ctx): Promise<PlannedAction[]> {
    const actions: PlannedAction[] = [
      {
        kind: "write",
        path: AUDIT_README_PATH,
        description: "write audit-chain README (hash-chain contract, genesis, `ade audit verify`)",
      },
    ];
    if (targetsClaudeCode(ctx)) {
      actions.push(
        {
          kind: "write",
          path: AUDIT_HOOK_PATH,
          description: "ship self-contained PostToolUse audit-log hook script",
        },
        {
          kind: "hook",
          path: CLAUDE_SETTINGS_PATH,
          description: "wire PostToolUse hook to append hash-chain entries for every tool call",
        },
      );
    }
    if (moduleOptions(ctx)["commitAuditLog"] === true) {
      actions.push({
        kind: "info",
        description: "commitAuditLog=true — leaving .ade/audit/ un-ignored so the log can be committed",
      });
    } else {
      actions.push({
        kind: "append",
        path: ".gitignore",
        description: "git-ignore .ade/audit/ (audit log stays local by default)",
      });
    }
    return actions;
  },

  async apply(ctx: Ctx): Promise<ModuleResult> {
    const findings: Finding[] = [];
    const wrotePaths: string[] = [];
    let degraded = false;

    // ISC-85: audit surface — README only; log.jsonl is initialized by the pipeline.
    await ctx.artifacts.write(AUDIT_README_PATH, auditReadme());
    wrotePaths.push(AUDIT_README_PATH);
    findings.push({ level: "ok", message: `wrote ${AUDIT_README_PATH}` });

    // ISC-86/87: ship the self-contained hook and wire it into Claude Code.
    if (targetsClaudeCode(ctx)) {
      await ctx.artifacts.write(AUDIT_HOOK_PATH, auditHookScript());
      wrotePaths.push(AUDIT_HOOK_PATH);
      const wired = await wireClaudeHook(ctx, "PostToolUse", AUDIT_HOOK_COMMAND);
      if (wired.level === "ok") {
        wrotePaths.push(CLAUDE_SETTINGS_PATH);
        findings.push({
          level: "ok",
          message: `wired PostToolUse audit hook into ${CLAUDE_SETTINGS_PATH}`,
        });
      } else {
        degraded = true;
        findings.push(wired);
      }
    }

    // ISC-88: audit dir git-ignored by default.
    if (moduleOptions(ctx)["commitAuditLog"] === true) {
      findings.push({
        level: "info",
        message: "commitAuditLog=true — .ade/audit/ left un-ignored so the audit log can be committed",
      });
    } else {
      await ensureLines(join(ctx.targetDir, ".gitignore"), [".ade/audit/"], GITIGNORE_LABEL);
      findings.push({ level: "ok", message: ".gitignore covers .ade/audit/" });
    }

    return { status: degraded ? "degraded" : "applied", findings, wrotePaths };
  },

  async verify(ctx: Ctx) {
    const findings: Finding[] = [];
    let ok = true;

    const readme = await readIfExists(join(ctx.targetDir, AUDIT_README_PATH));
    if (readme === null || !readme.includes(AUDIT_GENESIS)) {
      ok = false;
      findings.push({
        level: "error",
        message: `${AUDIT_README_PATH} missing or does not document the audit chain`,
        remediation: "run `ade apply`",
      });
    } else {
      findings.push({ level: "ok", message: `${AUDIT_README_PATH} documents the audit chain` });
    }

    if (targetsClaudeCode(ctx)) {
      const hook = await readIfExists(join(ctx.targetDir, AUDIT_HOOK_PATH));
      if (hook === null) {
        ok = false;
        findings.push({
          level: "error",
          message: `${AUDIT_HOOK_PATH} missing`,
          remediation: "run `ade apply`",
        });
      } else {
        findings.push({ level: "ok", message: `${AUDIT_HOOK_PATH} present` });
      }
      const settings = await readJson(ctx, CLAUDE_SETTINGS_PATH);
      if (!settingsWireHook(settings)) {
        ok = false;
        findings.push({
          level: "error",
          message: `${CLAUDE_SETTINGS_PATH} does not wire the PostToolUse audit hook`,
          remediation: "run `ade apply`",
        });
      } else {
        findings.push({ level: "ok", message: "PostToolUse audit hook wired" });
      }
    }

    // Integrity of the chain itself, when one exists.
    const logText = await readIfExists(join(ctx.targetDir, AUDIT_LOG_PATH));
    if (logText !== null) {
      try {
        const verdict = verifyChain(parseLog(logText));
        if (verdict.valid) {
          findings.push({ level: "ok", message: `audit chain valid (${verdict.length} entries)` });
        } else {
          ok = false;
          findings.push({
            level: "error",
            message: `audit chain BROKEN at entry ${verdict.brokenIndex} (${verdict.reason})`,
            remediation: "investigate tampering — do not edit .ade/audit/log.jsonl",
          });
        }
      } catch {
        ok = false;
        findings.push({
          level: "error",
          message: `${AUDIT_LOG_PATH} contains unparseable entries`,
          remediation: "investigate tampering — do not edit .ade/audit/log.jsonl",
        });
      }
    }

    return { ok, findings };
  },
};
