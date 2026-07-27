/**
 * Shared machine/project report functions — ONE code path for the CLI
 * (`ade doctor` / `ade status`) and the Control Center GUI (v0.2, ISC-174).
 * The CLI's `--json` output shapes remain byte-compatible with v0.1: doctor
 * emits DoctorReport verbatim; status strips `findings` before emitting.
 */
import {
  detectMachineHarnesses,
  detectRepoHarnesses,
  detectTools,
  isGitRepo,
} from "./context.ts";
import { HARNESS_ADAPTERS } from "./harness/adapters.ts";
import { MODULES } from "./registry.ts";
import type { Ctx, ExecFn, Finding, ToolInfo, WhichFn } from "./types.ts";

export interface DoctorReport {
  targetDir: string;
  gitRepo: boolean;
  tools: ToolInfo[];
  harnesses: { repo: string[]; machine: string[]; supported: string[] };
}

/** Machine + repo health facts (the `ade doctor` payload, v0.1-shape). */
export async function doctorReport(targetDir: string, which: WhichFn, exec: ExecFn): Promise<DoctorReport> {
  const tools = await detectTools(which, exec);
  return {
    targetDir,
    gitRepo: await isGitRepo(targetDir, exec),
    tools: Object.values(tools),
    harnesses: {
      repo: await detectRepoHarnesses(targetDir),
      machine: detectMachineHarnesses(which),
      supported: HARNESS_ADAPTERS.map((adapter) => adapter.id),
    },
  };
}

export interface StatusRow {
  id: string;
  title: string;
  enabled: boolean;
  state: "disabled" | "applied" | "degraded" | "not-applied";
  /** Full findings — consumed by the GUI; the CLI drops this field (v0.1 shape). */
  findings: Finding[];
}

/** Per-module status, WITH findings retained for GUI consumers. */
export async function statusReport(ctx: Ctx): Promise<StatusRow[]> {
  const rows: StatusRow[] = [];
  for (const module of MODULES) {
    const enabled = ctx.config.modules[module.id]?.enabled === true;
    if (!enabled) {
      rows.push({ id: module.id, title: module.title, enabled, state: "disabled", findings: [] });
      continue;
    }
    const verdict = await module.verify(ctx);
    const degraded = verdict.findings.some((finding) => finding.level === "degraded");
    rows.push({
      id: module.id,
      title: module.title,
      enabled,
      state: verdict.ok ? (degraded ? "degraded" : "applied") : "not-applied",
      findings: verdict.findings,
    });
  }
  return rows;
}
