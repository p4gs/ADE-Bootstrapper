/**
 * Pipeline orchestration: init / plan / apply / verify across all enabled
 * modules, plus the core-owned steps (instruction composition, translation,
 * lockfile, audit). Modules are fault-isolated: one throwing module is
 * reported as failed and the run continues.
 */
import { join } from "node:path";
import { appendEvents, type AuditEventInput } from "./audit.ts";
import { CONFIG_FILE, defaultConfig, loadConfig, serializeConfig } from "./config.ts";
import { buildCtx } from "./context.ts";
import { writeEnsured } from "./fsutil.ts";
import { HARNESS_ADAPTERS } from "./harness/adapters.ts";
import { collectBlocks, composeInstructions, INSTRUCTIONS_PATH } from "./instructions.ts";
import { generateLockfile, LOCKFILE_NAME, loadLockfile, serializeLockfile, verifyAgainstLockfile } from "./lockfile.ts";
import { MODULES, moduleIds } from "./registry.ts";
import { checkTranslationDrift, translateAll, type TranslateFileResult } from "./translate.ts";
import type { AdeConfig, AdeModule, Ctx, ExecFn, Finding, ModuleResult, PlannedAction, WhichFn } from "./types.ts";

export const AUDIT_LOG_PATH = ".ade/audit/log.jsonl";

export interface PipelineDeps {
  exec: ExecFn;
  which: WhichFn;
  log: (msg: string) => void;
  /** Clock injected so audit timestamps are testable; generated FILES never embed time. */
  now?: () => string;
}

export interface ModuleRunReport {
  id: string;
  title: string;
  enabled: boolean;
  result: ModuleResult;
}

export interface ApplyReport {
  ok: boolean;
  modules: ModuleRunReport[];
  translate: TranslateFileResult[];
  lockfilePath: string;
  auditAppended: number;
}

export interface VerifyReport {
  ok: boolean;
  lockfile: Finding[];
  translation: Finding[];
  modules: Array<{ id: string; ok: boolean; findings: Finding[] }>;
}

export interface PlanReport {
  modules: Array<{ id: string; enabled: boolean; actions: PlannedAction[] }>;
  coreActions: PlannedAction[];
}

function enabledIds(config: AdeConfig): Set<string> {
  return new Set(
    Object.entries(config.modules)
      .filter(([, moduleConfig]) => moduleConfig.enabled)
      .map(([id]) => id),
  );
}

/** Lockfile scope: fully ADE-owned generated files (`.ade/**`), excluding the mutable audit log. */
export function lockfileScope(paths: string[]): string[] {
  return paths.filter((path) => path.startsWith(".ade/") && !path.startsWith(".ade/audit/"));
}

export async function makeCtx(targetDir: string, config: AdeConfig, deps: PipelineDeps): Promise<Ctx> {
  return await buildCtx({ targetDir, config, exec: deps.exec, which: deps.which, log: deps.log });
}

/** Default harness targets for a fresh init: detected in-repo harnesses, else the opinionated pair. */
export function defaultHarnessTargets(repoHarnesses: string[]): string[] {
  if (repoHarnesses.length > 0) return [...repoHarnesses].sort();
  return ["claude-code", "codex"];
}

export async function initTarget(targetDir: string, deps: PipelineDeps): Promise<{ created: boolean; config: AdeConfig } | { error: string }> {
  const configPath = join(targetDir, CONFIG_FILE);
  const existing = await loadConfig(targetDir, moduleIds(), HARNESS_ADAPTERS.map((adapter) => adapter.id));
  if (existing.ok) {
    return { created: false, config: existing.config };
  }
  if (await Bun.file(configPath).exists()) {
    // Present but invalid — surface the validation error rather than overwrite.
    return { error: existing.error };
  }
  const probeCtx = await makeCtx(targetDir, defaultConfig(moduleIds(), []), deps);
  const config = defaultConfig(moduleIds(), defaultHarnessTargets(probeCtx.repoHarnesses));
  await writeEnsured(configPath, serializeConfig(config));
  return { created: true, config };
}

export async function planPipeline(ctx: Ctx, modules: AdeModule[] = MODULES): Promise<PlanReport> {
  const enabled = enabledIds(ctx.config);
  const moduleReports: PlanReport["modules"] = [];
  for (const module of modules) {
    const isEnabled = enabled.has(module.id);
    moduleReports.push({
      id: module.id,
      enabled: isEnabled,
      actions: isEnabled ? await module.plan(ctx) : [],
    });
  }
  return {
    modules: moduleReports,
    coreActions: [
      { kind: "write", path: INSTRUCTIONS_PATH, description: "compose canonical instructions from enabled modules" },
      { kind: "merge", description: "translate canonical instructions into harness instruction files (managed blocks)" },
      { kind: "write", path: LOCKFILE_NAME, description: "write deterministic lockfile over generated .ade artifacts" },
      { kind: "append", path: AUDIT_LOG_PATH, description: "append hash-chained audit events" },
    ],
  };
}

export async function applyPipeline(ctx: Ctx, deps: PipelineDeps, modules: AdeModule[] = MODULES): Promise<ApplyReport> {
  const enabled = enabledIds(ctx.config);
  const now = deps.now ?? (() => new Date().toISOString());
  const moduleReports: ModuleRunReport[] = [];
  const auditEvents: AuditEventInput[] = [];

  for (const module of modules) {
    const isEnabled = enabled.has(module.id);
    if (!isEnabled) {
      moduleReports.push({
        id: module.id,
        title: module.title,
        enabled: false,
        result: { status: "skipped", findings: [{ level: "info", message: "disabled in ade.json" }], wrotePaths: [] },
      });
      continue;
    }
    let result: ModuleResult;
    try {
      result = await module.apply(ctx);
    } catch (error) {
      result = {
        status: "failed",
        findings: [{ level: "error", message: `module threw: ${String(error)}` }],
        wrotePaths: [],
      };
    }
    moduleReports.push({ id: module.id, title: module.title, enabled: true, result });
    auditEvents.push({
      ts: now(),
      actor: "ade",
      action: "module.apply",
      target: module.id,
      result: result.status,
    });
  }

  // Core-owned: canonical instructions + translation.
  const blocks = collectBlocks(modules, enabled);
  await ctx.artifacts.write(INSTRUCTIONS_PATH, composeInstructions(blocks));
  const canonicalBody = composeInstructions(blocks);
  const translate = await translateAll(ctx, canonicalBody);
  for (const file of translate) {
    auditEvents.push({
      ts: now(),
      actor: "ade",
      action: "translate",
      target: file.path,
      result: file.ok ? (file.changed ? "updated" : "unchanged") : `refused: ${file.error ?? "unknown"}`,
    });
  }

  // Core-owned: lockfile over ADE-owned artifacts.
  const lock = await generateLockfile(ctx, lockfileScope(ctx.artifacts.written()));
  await writeEnsured(join(ctx.targetDir, LOCKFILE_NAME), serializeLockfile(lock));
  auditEvents.push({ ts: now(), actor: "ade", action: "lockfile.write", target: LOCKFILE_NAME, result: "ok" });

  const appended = await appendEvents(join(ctx.targetDir, AUDIT_LOG_PATH), auditEvents);

  const ok =
    moduleReports.every((report) => report.result.status !== "failed") &&
    translate.every((file) => file.ok);
  return { ok, modules: moduleReports, translate, lockfilePath: LOCKFILE_NAME, auditAppended: appended.length };
}

export async function verifyPipeline(ctx: Ctx, modules: AdeModule[] = MODULES): Promise<VerifyReport> {
  const enabled = enabledIds(ctx.config);

  const lock = await loadLockfile(ctx.targetDir);
  let lockFindings: Finding[];
  let lockOk: boolean;
  if (lock === null) {
    lockOk = false;
    lockFindings = [{ level: "error", message: `${LOCKFILE_NAME} missing or invalid`, remediation: "run `ade apply`" }];
  } else {
    const result = await verifyAgainstLockfile(ctx, lock);
    lockOk = result.ok;
    lockFindings = result.findings;
  }

  const blocks = collectBlocks(modules, enabled);
  const translation = await checkTranslationDrift(ctx, composeInstructions(blocks));
  const translationOk = translation.every((finding) => finding.level !== "error");

  const moduleResults: VerifyReport["modules"] = [];
  let modulesOk = true;
  for (const module of modules) {
    if (!enabled.has(module.id)) continue;
    try {
      const verdict = await module.verify(ctx);
      moduleResults.push({ id: module.id, ok: verdict.ok, findings: verdict.findings });
      if (!verdict.ok) modulesOk = false;
    } catch (error) {
      moduleResults.push({
        id: module.id,
        ok: false,
        findings: [{ level: "error", message: `verify threw: ${String(error)}` }],
      });
      modulesOk = false;
    }
  }

  return { ok: lockOk && translationOk && modulesOk, lockfile: lockFindings, translation, modules: moduleResults };
}
