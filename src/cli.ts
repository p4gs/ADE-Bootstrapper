#!/usr/bin/env bun
/**
 * `ade` — ADE Bootstrapper CLI.
 * Exit codes: 0 success · 1 failure · 2 usage error.
 * `--json` emits a single JSON document on stdout; human output goes to stderr.
 */
import { resolve, join } from "node:path";
import { loadConfig } from "./config.ts";
import { detectMachineHarnesses, detectRepoHarnesses, isGitRepo } from "./context.ts";
import { realExec, realWhich } from "./exec.ts";
import { HARNESS_ADAPTERS } from "./harness/adapters.ts";
import { collectBlocks, composeInstructions } from "./instructions.ts";
import { generateLockfile, LOCKFILE_NAME, serializeLockfile } from "./lockfile.ts";
import { parseLog, verifyChain } from "./audit.ts";
import { readIfExists, writeEnsured } from "./fsutil.ts";
import { MODULES, moduleIds } from "./registry.ts";
import {
  applyPipeline,
  AUDIT_LOG_PATH,
  initTarget,
  lockfileScope,
  makeCtx,
  planPipeline,
  verifyPipeline,
  type PipelineDeps,
} from "./run.ts";
import { translateAll } from "./translate.ts";
import { ADE_VERSION } from "./version.ts";
import type { AdeConfig, Ctx, ExecFn, WhichFn } from "./types.ts";

export interface Io {
  out: (text: string) => void;
  err: (text: string) => void;
}

export interface CliDeps {
  exec: ExecFn;
  which: WhichFn;
  io: Io;
  cwd: string;
}

const COMMANDS = [
  ["init [dir]", "bootstrap an ADE in the target repository (config + modules + lockfile)"],
  ["plan", "dry-run: show every action apply would take (writes nothing)"],
  ["apply", "apply all enabled modules, retranslate instructions, refresh lockfile"],
  ["verify", "verify on-disk state against the lockfile, canonical instructions, and module checks"],
  ["status", "per-module status summary"],
  ["doctor", "report integrated tools, harnesses, and environment health"],
  ["modules", "list all modules with enabled state"],
  ["translate", "regenerate harness instruction files from .ade/instructions.md"],
  ["lock", "regenerate ade.lock.json from current on-disk artifacts"],
  ["audit verify", "validate the tamper-evident audit log hash chain"],
  ["version", "print the ade version"],
  ["help", "show this help"],
] as const;

export function usage(): string {
  const lines = COMMANDS.map(([cmd, desc]) => `  ade ${cmd.padEnd(14)} ${desc}`);
  return [
    `ade ${ADE_VERSION} — bootstrap a secure Agentic Development Environment`,
    "",
    "Usage: ade <command> [--dir <path>] [--json]",
    "",
    ...lines,
    "",
  ].join("\n");
}

interface ParsedArgs {
  command: string;
  positionals: string[];
  json: boolean;
  dir: string | null;
  error?: string;
}

export function parseArgs(argv: string[]): ParsedArgs {
  const positionals: string[] = [];
  let json = false;
  let dir: string | null = null;
  let error: string | undefined;
  for (let index = 0; index < argv.length; index += 1) {
    const arg = argv[index]!;
    if (arg === "--json") json = true;
    else if (arg === "--dir" || arg === "-C") {
      const value = argv[index + 1];
      if (value === undefined) error = "--dir requires a path argument";
      else {
        dir = value;
        index += 1;
      }
    } else if (arg === "--help" || arg === "-h") positionals.unshift("help");
    else if (arg.startsWith("-")) error = `unknown flag: ${arg}`;
    else positionals.push(arg);
  }
  const command = positionals.shift() ?? "help";
  return { command, positionals, json, dir, error };
}

function emit(io: Io, json: boolean, payload: unknown, human: string): void {
  if (json) io.out(`${JSON.stringify(payload, null, 2)}\n`);
  else io.out(`${human}\n`);
}

async function loadCtx(targetDir: string, deps: PipelineDeps, io: Io, json: boolean): Promise<Ctx | null> {
  const config = await loadConfig(
    targetDir,
    moduleIds(),
    HARNESS_ADAPTERS.map((adapter) => adapter.id),
  );
  if (!config.ok) {
    if (json) io.out(`${JSON.stringify({ ok: false, error: config.error }, null, 2)}\n`);
    else io.err(`ade: ${config.error}\n`);
    return null;
  }
  return await makeCtx(targetDir, config.config, deps);
}

function summarizeFindings(prefix: string, findings: Array<{ level: string; message: string; remediation?: string }>): string {
  return findings
    .map((finding) => `${prefix}[${finding.level}] ${finding.message}${finding.remediation !== undefined ? ` → ${finding.remediation}` : ""}`)
    .join("\n");
}

export async function main(argv: string[], depsIn?: Partial<CliDeps>): Promise<number> {
  const io: Io = depsIn?.io ?? {
    out: (text) => process.stdout.write(text),
    err: (text) => process.stderr.write(text),
  };
  const deps: CliDeps = {
    exec: depsIn?.exec ?? realExec,
    which: depsIn?.which ?? realWhich,
    io,
    cwd: depsIn?.cwd ?? process.cwd(),
  };
  const parsed = parseArgs(argv);
  if (parsed.error !== undefined) {
    io.err(`ade: ${parsed.error}\n\n${usage()}`);
    return 2;
  }
  const pipelineDeps: PipelineDeps = {
    exec: deps.exec,
    which: deps.which,
    log: (msg) => io.err(`${msg}\n`),
  };
  // Only `init` accepts a positional directory; other commands take subcommands positionally.
  const positionalDir = parsed.command === "init" ? parsed.positionals[0] : undefined;
  const targetDir = resolve(deps.cwd, parsed.dir ?? positionalDir ?? ".");
  const json = parsed.json;

  switch (parsed.command) {
    case "help": {
      io.out(usage());
      return 0;
    }
    case "version": {
      emit(io, json, { version: ADE_VERSION }, ADE_VERSION);
      return 0;
    }
    case "init": {
      if (!(await Bun.file(targetDir).exists()) && !(await isDirectory(targetDir))) {
        io.err(`ade: target directory does not exist: ${targetDir}\n`);
        return 1;
      }
      const initResult = await initTarget(targetDir, pipelineDeps);
      if ("error" in initResult) {
        io.err(`ade: ${initResult.error}\n`);
        return 1;
      }
      const ctx = await makeCtx(targetDir, initResult.config, pipelineDeps);
      const report = await applyPipeline(ctx, pipelineDeps);
      const payload = { ok: report.ok, created: initResult.created, ...reportPayload(report) };
      emit(io, json, payload, humanApply("init", report, initResult.created));
      return report.ok ? 0 : 1;
    }
    case "apply": {
      const ctx = await loadCtx(targetDir, pipelineDeps, io, json);
      if (ctx === null) return 1;
      const report = await applyPipeline(ctx, pipelineDeps);
      emit(io, json, { ok: report.ok, ...reportPayload(report) }, humanApply("apply", report, false));
      return report.ok ? 0 : 1;
    }
    case "plan": {
      const ctx = await loadCtx(targetDir, pipelineDeps, io, json);
      if (ctx === null) return 1;
      const report = await planPipeline(ctx);
      const human = [
        "ade plan (dry-run — nothing written):",
        ...report.modules.flatMap((module) =>
          module.enabled
            ? module.actions.map((action) => `  [${module.id}] ${action.kind}${action.path !== undefined ? ` ${action.path}` : ""} — ${action.description}`)
            : [`  [${module.id}] skipped (disabled)`],
        ),
        ...report.coreActions.map((action) => `  [core] ${action.kind}${action.path !== undefined ? ` ${action.path}` : ""} — ${action.description}`),
      ].join("\n");
      emit(io, json, report, human);
      return 0;
    }
    case "verify": {
      const ctx = await loadCtx(targetDir, pipelineDeps, io, json);
      if (ctx === null) return 1;
      const report = await verifyPipeline(ctx);
      const human = [
        `ade verify: ${report.ok ? "PASS" : "FAIL"}`,
        summarizeFindings("  lock: ", report.lockfile),
        summarizeFindings("  instructions: ", report.translation),
        ...report.modules.map((module) => `  ${module.ok ? "✓" : "✗"} ${module.id}\n${summarizeFindings("    ", module.findings)}`),
      ].join("\n");
      emit(io, json, report, human);
      return report.ok ? 0 : 1;
    }
    case "status": {
      const ctx = await loadCtx(targetDir, pipelineDeps, io, json);
      if (ctx === null) return 1;
      const rows: Array<{ id: string; title: string; enabled: boolean; state: string }> = [];
      for (const module of MODULES) {
        const enabled = ctx.config.modules[module.id]?.enabled === true;
        if (!enabled) {
          rows.push({ id: module.id, title: module.title, enabled, state: "disabled" });
          continue;
        }
        const verdict = await module.verify(ctx);
        const degraded = verdict.findings.some((finding) => finding.level === "degraded");
        rows.push({ id: module.id, title: module.title, enabled, state: verdict.ok ? (degraded ? "degraded" : "applied") : "not-applied" });
      }
      const human = ["ade status:", ...rows.map((row) => `  ${row.state.padEnd(12)} ${row.id} — ${row.title}`)].join("\n");
      emit(io, json, { modules: rows }, human);
      return 0;
    }
    case "doctor": {
      const { detectTools } = await import("./context.ts");
      const tools = await detectTools(deps.which, deps.exec);
      const repoHarnesses = await detectRepoHarnesses(targetDir);
      const machineHarnesses = detectMachineHarnesses(deps.which);
      const git = await isGitRepo(targetDir, deps.exec);
      const payload = {
        targetDir,
        gitRepo: git,
        tools: Object.values(tools),
        harnesses: {
          repo: repoHarnesses,
          machine: machineHarnesses,
          supported: HARNESS_ADAPTERS.map((adapter) => adapter.id),
        },
      };
      const human = [
        `ade doctor — ${targetDir}`,
        `  git repository: ${git ? "yes" : "NO — run git init"}`,
        "  tools:",
        ...Object.values(tools).map(
          (tool) => `    ${tool.present ? "✓" : "✗"} ${tool.name}${tool.version !== undefined ? ` (${tool.version})` : ""}`,
        ),
        `  harnesses configured in repo: ${repoHarnesses.join(", ") || "none"}`,
        `  harness CLIs on machine: ${machineHarnesses.join(", ") || "none"}`,
      ].join("\n");
      emit(io, json, payload, human);
      return 0;
    }
    case "modules": {
      const configResult = await loadConfig(targetDir, moduleIds(), HARNESS_ADAPTERS.map((adapter) => adapter.id));
      const enabledMap: Record<string, boolean> = {};
      for (const module of MODULES) {
        enabledMap[module.id] = configResult.ok
          ? configResult.config.modules[module.id]?.enabled === true
          : module.defaultEnabled;
      }
      const payload = MODULES.map((module) => ({
        id: module.id,
        title: module.title,
        category: module.category,
        spec: module.spec,
        enabled: enabledMap[module.id],
      }));
      const human = ["ade modules:", ...payload.map((module) => `  ${module.enabled === true ? "on " : "off"} ${module.id.padEnd(18)} ${module.title}`)].join("\n");
      emit(io, json, { modules: payload }, human);
      return 0;
    }
    case "translate": {
      const ctx = await loadCtx(targetDir, pipelineDeps, io, json);
      if (ctx === null) return 1;
      const enabled = new Set(Object.entries(ctx.config.modules).filter(([, m]) => m.enabled).map(([id]) => id));
      const body = composeInstructions(collectBlocks(MODULES, enabled));
      await ctx.artifacts.write(".ade/instructions.md", body);
      const results = await translateAll(ctx, body);
      const ok = results.every((result) => result.ok);
      const human = [
        `ade translate: ${ok ? "OK" : "REFUSED"}`,
        ...results.map((result) => `  ${result.ok ? (result.changed ? "updated " : "unchanged") : "REFUSED "} ${result.path}${result.error !== undefined ? ` — ${result.error}` : ""}`),
      ].join("\n");
      emit(io, json, { ok, files: results }, human);
      return ok ? 0 : 1;
    }
    case "lock": {
      const ctx = await loadCtx(targetDir, pipelineDeps, io, json);
      if (ctx === null) return 1;
      const scan = new Bun.Glob("**/*");
      const adePaths: string[] = [];
      for await (const path of scan.scan({ cwd: join(targetDir, ".ade"), onlyFiles: true })) {
        adePaths.push(`.ade/${path}`);
      }
      const lock = await generateLockfile(ctx, lockfileScope(adePaths.sort()));
      await writeEnsured(join(targetDir, LOCKFILE_NAME), serializeLockfile(lock));
      emit(io, json, { ok: true, files: Object.keys(lock.files).length }, `ade lock: recorded ${Object.keys(lock.files).length} files`);
      return 0;
    }
    case "audit": {
      const sub = parsed.positionals[0];
      const text = await readIfExists(join(targetDir, AUDIT_LOG_PATH));
      if (text === null) {
        io.err(`ade: audit log not found at ${AUDIT_LOG_PATH} — run \`ade init\` first\n`);
        return 1;
      }
      let entries;
      try {
        entries = parseLog(text);
      } catch {
        emit(io, json, { valid: false, reason: "unparseable" }, "ade audit: INVALID — log is not parseable JSONL");
        return 1;
      }
      if (sub === "show") {
        emit(io, json, { entries }, entries.map((entry) => `${entry.ts} ${entry.action} ${entry.target} → ${entry.result}`).join("\n"));
        return 0;
      }
      const verdict = verifyChain(entries);
      emit(
        io,
        json,
        verdict,
        verdict.valid
          ? `ade audit: chain VALID (${verdict.length} entries)`
          : `ade audit: chain BROKEN at entry ${verdict.brokenIndex} (${verdict.reason})`,
      );
      return verdict.valid ? 0 : 1;
    }
    default: {
      io.err(`ade: unknown command "${parsed.command}"\n\n${usage()}`);
      return 2;
    }
  }
}

function reportPayload(report: Awaited<ReturnType<typeof applyPipeline>>) {
  return {
    modules: report.modules.map((module) => ({
      id: module.id,
      enabled: module.enabled,
      status: module.result.status,
      findings: module.result.findings,
    })),
    translate: report.translate,
    lockfile: report.lockfilePath,
    auditEventsAppended: report.auditAppended,
  };
}

function humanApply(verb: string, report: Awaited<ReturnType<typeof applyPipeline>>, created: boolean): string {
  const lines = [
    `ade ${verb}: ${report.ok ? "OK" : "FAILED"}${created ? " (created ade.json)" : ""}`,
    ...report.modules.map((module) => {
      const marker = module.result.status === "failed" ? "✗" : module.result.status === "degraded" ? "◐" : module.enabled ? "✓" : "·";
      return `  ${marker} ${module.result.status.padEnd(9)} ${module.id}`;
    }),
    ...report.translate.map((file) => `  ${file.ok ? "✓" : "✗"} translate ${file.path}${file.error !== undefined ? ` — ${file.error}` : ""}`),
    `  lockfile: ${report.lockfilePath} · audit: +${report.auditAppended} events`,
  ];
  return lines.join("\n");
}

async function isDirectory(path: string): Promise<boolean> {
  try {
    const glob = new Bun.Glob("*");
    for await (const _ of glob.scan({ cwd: path, onlyFiles: false })) return true;
    return true;
  } catch {
    return false;
  }
}

if (import.meta.main) {
  process.exit(await main(Bun.argv.slice(2)));
}
