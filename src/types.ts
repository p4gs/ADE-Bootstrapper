/**
 * Core contracts for the ADE Bootstrapper.
 *
 * FROZEN INTERFACE — every module in src/modules/ and every harness adapter
 * in src/harness/ implements exactly these shapes. The module interface IS
 * the extension model: adding a component to the ADE means adding one file
 * that implements `AdeModule`; adding a harness means one `HarnessAdapter`.
 */

/** Result of running a subprocess via the injected executor. */
export interface ExecResult {
  code: number;
  stdout: string;
  stderr: string;
}

/**
 * Safe subprocess executor. Always an argv array — never a shell string —
 * so external input can never be interpolated into a shell.
 */
export type ExecFn = (
  argv: string[],
  opts?: { cwd?: string; stdin?: string },
) => Promise<ExecResult>;

/** Resolver from tool name to executable path (Bun.which in production). */
export type WhichFn = (name: string) => string | null;

/** Presence/version info for one integrated tool. */
export interface ToolInfo {
  name: string;
  present: boolean;
  path?: string;
  version?: string;
}

/** Severity ladder for module findings. */
export type FindingLevel = "ok" | "info" | "warn" | "degraded" | "error";

export interface Finding {
  level: FindingLevel;
  message: string;
  /** Actionable next step (install guidance, config fix, …). */
  remediation?: string;
}

/** A single action `apply` would take — returned by `plan`, which never writes. */
export interface PlannedAction {
  kind: "write" | "merge" | "append" | "hook" | "info";
  path?: string;
  description: string;
}

export type ModuleStatus = "applied" | "skipped" | "degraded" | "failed";

export interface ModuleResult {
  status: ModuleStatus;
  findings: Finding[];
  /** Repo-relative paths of files this apply wrote/updated (collected into the lockfile). */
  wrotePaths: string[];
}

export interface VerifyResult {
  ok: boolean;
  findings: Finding[];
}

/** A block of canonical instructions a module contributes to `.ade/instructions.md`. */
export interface InstructionBlock {
  id: string;
  title: string;
  /** Markdown body. No heading — the composer renders `### {title}`. */
  content: string;
}

/** Per-module configuration inside ade.json. */
export interface ModuleConfig {
  enabled: boolean;
  options: Record<string, unknown>;
}

/** The unified project configuration (`ade.json`). */
export interface AdeConfig {
  schemaVersion: number;
  /** Harness adapter ids this repo targets. */
  harnesses: string[];
  modules: Record<string, ModuleConfig>;
}

/** Records files written under the target repo and creates parent dirs. */
export interface ArtifactWriter {
  /** Write `content` at repo-relative `relPath` (deterministic, ensures dirs). Returns true if content changed. */
  write(relPath: string, content: string): Promise<boolean>;
  /** All repo-relative paths written through this writer. */
  written(): string[];
}

/** Everything a module needs to detect/plan/apply/verify. All I/O is injected for testability. */
export interface Ctx {
  /** Absolute path of the repository being bootstrapped. */
  targetDir: string;
  /** Absolute path of `<targetDir>/.ade`. */
  adeDir: string;
  config: AdeConfig;
  /** Detection results for integrated tools, keyed by tool name. */
  tools: Record<string, ToolInfo>;
  /** Harness adapter ids detected as configured in the target repo. */
  repoHarnesses: string[];
  isGitRepo: boolean;
  os: string;
  arch: string;
  env: Record<string, string | undefined>;
  exec: ExecFn;
  which: WhichFn;
  /** Human/progress logging — ALWAYS stderr; stdout is reserved for command output. */
  log: (msg: string) => void;
  artifacts: ArtifactWriter;
}

/**
 * One bootstrapper component. Maps 1:1 to a spec "Core components" bullet.
 * Lifecycle: detect (report environment) → plan (dry-run, NEVER writes) →
 * apply (idempotent) → verify (re-derive state from disk).
 */
export interface AdeModule {
  id: string;
  title: string;
  category: "security" | "governance" | "context" | "efficiency" | "reproducibility";
  /** Spec component this module implements (traceability). */
  spec: string;
  defaultEnabled: boolean;
  /** Static instruction blocks composed into `.ade/instructions.md`. */
  instructionBlocks: InstructionBlock[];
  detect(ctx: Ctx): Promise<Finding[]>;
  plan(ctx: Ctx): Promise<PlannedAction[]>;
  apply(ctx: Ctx): Promise<ModuleResult>;
  verify(ctx: Ctx): Promise<VerifyResult>;
}

/** Capability flags a harness adapter declares. */
export interface HarnessCapabilities {
  hooks: boolean;
  mcp: boolean;
  permissions: boolean;
}

/** One supported coding harness and how ADE integrates with it. */
export interface HarnessAdapter {
  id: string;
  title: string;
  /** Repo-relative instruction file this harness reads (CLAUDE.md, AGENTS.md, .cursorrules …). */
  instructionFile: string;
  /** CLI executable names that indicate the harness is installed on the machine. */
  cliNames: string[];
  /** Repo-relative paths whose presence means the repo is configured for this harness. */
  configSignals: string[];
  capabilities: HarnessCapabilities;
}

/** The tool names `ade doctor` reports on. */
export const INTEGRATED_TOOLS = [
  "trufflehog",
  "pre-commit",
  "gitleaks",
  "rtk",
  "ocean",
  "nono",
  "osv-scanner",
  "openwiki",
  "cocoindex",
  "ccc",
  "serena",
  "sscsb",
] as const;
