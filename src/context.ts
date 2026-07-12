/**
 * Environment/context detection: OS, integrated tools, harnesses, git state.
 * All probes go through injected `which`/`exec` so tests can simulate any machine.
 */
import { join } from "node:path";
import { HARNESS_ADAPTERS } from "./harness/adapters.ts";
import { makeArtifactWriter } from "./fsutil.ts";
import { INTEGRATED_TOOLS } from "./types.ts";
import type { AdeConfig, Ctx, ExecFn, ToolInfo, WhichFn } from "./types.ts";

/** Version-flag lookup for tools whose version output we can cheaply parse. */
const VERSION_ARGS: Record<string, string[]> = {
  trufflehog: ["--version"],
  "pre-commit": ["--version"],
  gitleaks: ["version"],
  rtk: ["--version"],
  ocean: ["--version"],
  nono: ["--version"],
  "osv-scanner": ["--version"],
  bun: ["--version"],
  git: ["--version"],
};

/** First line of version output, stripped of common prefixes. Never throws. */
export function parseVersionLine(raw: string): string | undefined {
  const line = raw.split("\n").find((candidate) => candidate.trim().length > 0);
  if (line === undefined) return undefined;
  return line.trim();
}

export async function detectTool(name: string, which: WhichFn, exec: ExecFn): Promise<ToolInfo> {
  const path = which(name);
  if (path === null) return { name, present: false };
  const args = VERSION_ARGS[name] ?? ["--version"];
  const result = await exec([path, ...args]);
  const versionSource = result.stdout.trim().length > 0 ? result.stdout : result.stderr;
  return {
    name,
    present: true,
    path,
    version: result.code === 0 ? parseVersionLine(versionSource) : undefined,
  };
}

export async function detectTools(which: WhichFn, exec: ExecFn): Promise<Record<string, ToolInfo>> {
  const tools: Record<string, ToolInfo> = {};
  for (const name of INTEGRATED_TOOLS) {
    tools[name] = await detectTool(name, which, exec);
  }
  return tools;
}

/** Harness ids configured in the target repo (by config-signal file presence). */
export async function detectRepoHarnesses(targetDir: string): Promise<string[]> {
  const found: string[] = [];
  for (const adapter of HARNESS_ADAPTERS) {
    for (const signal of adapter.configSignals) {
      const stat = await Bun.file(join(targetDir, signal)).exists();
      const dirExists = !stat && (await directoryExists(join(targetDir, signal)));
      if (stat || dirExists) {
        found.push(adapter.id);
        break;
      }
    }
  }
  return found;
}

async function directoryExists(path: string): Promise<boolean> {
  try {
    const glob = new Bun.Glob("*");
    // Scanning a non-directory throws; an empty directory yields nothing but doesn't throw.
    for await (const _ of glob.scan({ cwd: path, onlyFiles: false })) {
      return true;
    }
    return true;
  } catch {
    return false;
  }
}

/** Harness ids whose CLI is installed on this machine. */
export function detectMachineHarnesses(which: WhichFn): string[] {
  return HARNESS_ADAPTERS.filter((adapter) =>
    adapter.cliNames.some((cli) => which(cli) !== null),
  ).map((adapter) => adapter.id);
}

export async function isGitRepo(targetDir: string, exec: ExecFn): Promise<boolean> {
  const result = await exec(["git", "-C", targetDir, "rev-parse", "--is-inside-work-tree"]);
  return result.code === 0 && result.stdout.trim() === "true";
}

export interface BuildCtxOptions {
  targetDir: string;
  config: AdeConfig;
  exec: ExecFn;
  which: WhichFn;
  log?: (msg: string) => void;
  env?: Record<string, string | undefined>;
}

export async function buildCtx(options: BuildCtxOptions): Promise<Ctx> {
  const { targetDir, config, exec, which } = options;
  const tools = await detectTools(which, exec);
  return {
    targetDir,
    adeDir: join(targetDir, ".ade"),
    config,
    tools,
    repoHarnesses: await detectRepoHarnesses(targetDir),
    isGitRepo: await isGitRepo(targetDir, exec),
    os: process.platform,
    arch: process.arch,
    env: options.env ?? process.env,
    exec,
    which,
    log: options.log ?? ((msg: string) => console.error(msg)),
    artifacts: makeArtifactWriter(targetDir),
  };
}
