/**
 * Canonical test helpers — ALL module and core tests build their fixtures and
 * fake contexts through these, so behavior under test is uniform.
 */
import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { makeArtifactWriter } from "../src/fsutil.ts";
import type { AdeConfig, Ctx, ExecFn, ExecResult, ModuleConfig, ToolInfo, WhichFn } from "../src/types.ts";
import { INTEGRATED_TOOLS } from "../src/types.ts";

/** The fifteen module ids (kept literal so helpers don't import the registry). */
export const ALL_MODULE_IDS = [
  "guardrails",
  "supply-chain",
  "sandbox",
  "context",
  "scaffolding",
  "memory",
  "injection-defense",
  "config-governance",
  "observability",
  "approval-gates",
  "secrets",
  "git-hygiene",
  "cost-governance",
  "reproducibility",
  "token-efficiency",
] as const;

export async function makeTempDir(): Promise<string> {
  return await mkdtemp(join(tmpdir(), "ade-test-"));
}

export async function removeDir(path: string): Promise<void> {
  await rm(path, { recursive: true, force: true });
}

export function testConfig(overrides?: {
  harnesses?: string[];
  modules?: Record<string, Partial<ModuleConfig>>;
}): AdeConfig {
  const modules: Record<string, ModuleConfig> = {};
  for (const id of ALL_MODULE_IDS) {
    modules[id] = {
      enabled: overrides?.modules?.[id]?.enabled ?? true,
      options: overrides?.modules?.[id]?.options ?? {},
    };
  }
  return {
    schemaVersion: 1,
    harnesses: overrides?.harnesses ?? ["claude-code", "codex"],
    modules,
  };
}

/** Exec fake: responses keyed by joined argv prefix; anything unmatched → 127. */
export function fakeExec(responses: Record<string, Partial<ExecResult>> = {}): ExecFn {
  return async (argv) => {
    const joined = argv.join(" ");
    for (const [prefix, response] of Object.entries(responses)) {
      if (joined.startsWith(prefix) || joined.includes(prefix)) {
        return { code: response.code ?? 0, stdout: response.stdout ?? "", stderr: response.stderr ?? "" };
      }
    }
    return { code: 127, stdout: "", stderr: `not found: ${argv[0]}` };
  };
}

export interface TestCtxOptions {
  config?: AdeConfig;
  /** Tools to mark present, e.g. { trufflehog: "trufflehog 3.90.0" }. */
  presentTools?: Record<string, string>;
  isGitRepo?: boolean;
  repoHarnesses?: string[];
  exec?: ExecFn;
  which?: WhichFn;
  env?: Record<string, string | undefined>;
}

/** Build a fully deterministic Ctx over a real temp directory — no machine probing. */
export function makeTestCtx(targetDir: string, options: TestCtxOptions = {}): Ctx {
  const present = options.presentTools ?? {};
  const tools: Record<string, ToolInfo> = {};
  for (const name of INTEGRATED_TOOLS) {
    const version = present[name];
    tools[name] =
      version !== undefined
        ? { name, present: true, path: `/fake/bin/${name}`, version }
        : { name, present: false };
  }
  return {
    targetDir,
    adeDir: join(targetDir, ".ade"),
    config: options.config ?? testConfig(),
    tools,
    repoHarnesses: options.repoHarnesses ?? [],
    isGitRepo: options.isGitRepo ?? true,
    os: "test-os",
    arch: "test-arch",
    env: options.env ?? {},
    exec: options.exec ?? fakeExec(),
    which: options.which ?? ((name) => (present[name] !== undefined ? `/fake/bin/${name}` : null)),
    log: () => {},
    artifacts: makeArtifactWriter(targetDir),
  };
}
