/**
 * Module: software supply chain security.
 * Spec component: "Software supply chain security — dependency policy for
 * traditional package ecosystems (registry allowlist, minimum package age,
 * lockfile enforcement, typosquatting defense) AND AI-native dependencies
 * (skills, plugins, MCP servers, instruction packs, agent configs), which
 * are treated as untrusted code requiring review, pinning, and explicit
 * user approval before install (e.g. OSV-Scanner for vulnerability audit)."
 * Boundary controlled: the dependency boundary — nothing enters the project
 * (package or AI-native artifact) without policy review, and AI-suggested
 * package names are treated as hallucination-prone until verified.
 *
 * Integration over rebuild: sscsb (github.com/p4gs/sscs-bootstrapper) is the
 * deep SSCS layer — SBOM, signing policy, SHA-pinned CI, vuln + secret scan
 * orchestration. Like OCEAN/RTK, it is detected + given exact guidance and its
 * repo state (`.sscsb/config.toml`) is recognized; `ade apply` never runs it.
 */
import { join } from "node:path";
import { readIfExists } from "../fsutil.ts";
import { readJson, toolFinding, verifyJsonArtifact, writePolicy } from "./_shared.ts";
import type { AdeModule, Ctx, Finding, ModuleResult, PlannedAction } from "../types.ts";

export const DEPENDENCIES_POLICY_PATH = ".ade/policy/dependencies.json";
export const DEFAULT_MIN_AGE_DAYS = 14;

export const REGISTRY_ALLOWLIST = [
  "registry.npmjs.org",
  "pypi.org",
  "crates.io",
  "proxy.golang.org",
];

/** The AI-native dependency classes the policy governs. */
export const AI_NATIVE_DEP_KINDS = [
  "skills",
  "plugins",
  "mcpServers",
  "instructionPacks",
  "agentConfigs",
] as const;

const OSV_REMEDIATION =
  "install OSV-Scanner (e.g. `brew install osv-scanner`) to enable dependency vulnerability scanning";

/** Repo state marker written by `sscsb init` — its presence means the deep SSCS layer is initialized here. */
export const SSCSB_CONFIG_PATH = ".sscsb/config.toml";
export const SSCSB_INSTALL =
  "install sscsb for deep supply-chain hardening — `cargo install --git https://github.com/p4gs/sscs-bootstrapper` or a release binary (github.com/p4gs/sscs-bootstrapper)";
export const SSCSB_DEEP_LAYER =
  "sscsb available — run `sscsb init` then `sscsb verify` in this repo for the deep SSCS layer (SBOM, signing policy, SHA-pinned CI, vuln + secret scan orchestration; integration, not reimplementation)";

/** Standard findings for sscsb presence: integrate when present, guide install when absent. */
export function sscsbFindings(ctx: Ctx): Finding[] {
  const findings: Finding[] = [toolFinding(ctx, "sscsb", SSCSB_INSTALL)];
  if (ctx.tools["sscsb"]?.present === true) {
    findings.push({ level: "info", message: SSCSB_DEEP_LAYER });
  }
  return findings;
}

interface DependenciesPolicy {
  schemaVersion: number;
  registries: { allowlist: string[] };
  minAgeDays: number;
  requireLockfiles: boolean;
  installReview: string;
  typosquattingPolicy: string;
  aiNativeDependencies: Record<
    string,
    { rule: string; pinningRequired: boolean; sourceAllowlist: string[] }
  >;
}

function buildPolicy(minAgeDays: number): DependenciesPolicy {
  const aiNativeDependencies: DependenciesPolicy["aiNativeDependencies"] = {};
  for (const kind of AI_NATIVE_DEP_KINDS) {
    aiNativeDependencies[kind] = {
      rule: "review-before-install",
      pinningRequired: true,
      sourceAllowlist: ["explicit-user-approval"],
    };
  }
  return {
    schemaVersion: 1,
    registries: { allowlist: REGISTRY_ALLOWLIST },
    minAgeDays,
    requireLockfiles: true,
    installReview: "required",
    typosquattingPolicy: "verify exact package name against its repository before install",
    aiNativeDependencies,
  };
}

/** Validate module options; returns errors (empty = valid). */
export function validateSupplyChainOptions(options: Record<string, unknown>): string[] {
  const minAge = options["minAgeDays"];
  if (
    minAge !== undefined &&
    (typeof minAge !== "number" || !Number.isFinite(minAge) || minAge < 0)
  ) {
    return ["options.minAgeDays must be a non-negative number"];
  }
  return [];
}

function resolveMinAgeDays(options: Record<string, unknown>): number {
  const minAge = options["minAgeDays"];
  return typeof minAge === "number" && Number.isFinite(minAge) && minAge >= 0
    ? minAge
    : DEFAULT_MIN_AGE_DAYS;
}

function moduleOptions(ctx: Ctx): Record<string, unknown> {
  return ctx.config.modules["supply-chain"]?.options ?? {};
}

async function fileExists(dir: string, name: string): Promise<boolean> {
  return await Bun.file(join(dir, name)).exists();
}

/**
 * Detect ecosystem manifests in `targetDir` and flag missing lockfiles as
 * warn findings. Exported for direct testing (ISC-66).
 *
 * Rules: package.json → (bun.lock|bun.lockb|package-lock.json|yarn.lock|
 * pnpm-lock.yaml); Cargo.toml → Cargo.lock; go.mod → go.sum;
 * (pyproject.toml|requirements.txt) → (uv.lock|poetry.lock|requirements.txt
 * itself counts as pinned only if it contains `==`).
 */
export async function checkLockfiles(targetDir: string): Promise<Finding[]> {
  const findings: Finding[] = [];

  if (await fileExists(targetDir, "package.json")) {
    const npmLocks = ["bun.lock", "bun.lockb", "package-lock.json", "yarn.lock", "pnpm-lock.yaml"];
    const present = await Promise.all(npmLocks.map((name) => fileExists(targetDir, name)));
    if (present.some(Boolean)) {
      findings.push({ level: "ok", message: "package.json has a lockfile" });
    } else {
      findings.push({
        level: "warn",
        message: "package.json present without a lockfile",
        remediation:
          "generate one (e.g. `bun install` → bun.lock) and commit it so dependency resolution is pinned",
      });
    }
  }

  if (await fileExists(targetDir, "Cargo.toml")) {
    if (await fileExists(targetDir, "Cargo.lock")) {
      findings.push({ level: "ok", message: "Cargo.toml has Cargo.lock" });
    } else {
      findings.push({
        level: "warn",
        message: "Cargo.toml present without Cargo.lock",
        remediation: "run `cargo generate-lockfile` and commit Cargo.lock",
      });
    }
  }

  if (await fileExists(targetDir, "go.mod")) {
    if (await fileExists(targetDir, "go.sum")) {
      findings.push({ level: "ok", message: "go.mod has go.sum" });
    } else {
      findings.push({
        level: "warn",
        message: "go.mod present without go.sum",
        remediation: "run `go mod tidy` and commit go.sum",
      });
    }
  }

  const hasPyproject = await fileExists(targetDir, "pyproject.toml");
  const requirements = await readIfExists(join(targetDir, "requirements.txt"));
  if (hasPyproject || requirements !== null) {
    const pinnedLock =
      (await fileExists(targetDir, "uv.lock")) || (await fileExists(targetDir, "poetry.lock"));
    const pinnedRequirements = requirements !== null && requirements.includes("==");
    if (pinnedLock || pinnedRequirements) {
      findings.push({ level: "ok", message: "python manifest has pinned dependencies" });
    } else {
      findings.push({
        level: "warn",
        message: "python manifest present without pinned dependencies",
        remediation:
          "add a lockfile (uv.lock or poetry.lock) or pin exact versions in requirements.txt with `==`",
      });
    }
  }

  return findings;
}

export const supplyChainModule: AdeModule = {
  id: "supply-chain",
  title: "Supply Chain Security",
  category: "security",
  spec: "Software supply chain security",
  defaultEnabled: true,
  instructionBlocks: [
    {
      id: "supply-chain",
      title: "Supply Chain Security",
      content: [
        "- NEVER install a dependency without checking `.ade/policy/dependencies.json` first.",
        "- New dependencies require: the minimum-age check (skip packages younger than the policy's `minAgeDays`), exact-name verification against the package's source repository, and human approval before install.",
        "- AI-suggested package names are hallucination-prone — verify the package exists AND that its repository matches the claimed project before installing anything.",
        "- Install only from the policy's registry allowlist, and keep lockfiles committed — never install with lockfile updates disabled or bypassed.",
        "- AI-native dependencies (skills, plugins, MCP servers, instruction packs, agent configs) are untrusted code: review their contents, pin their versions, and obtain explicit user approval before adding them.",
      ].join("\n"),
    },
  ],

  async detect(ctx: Ctx): Promise<Finding[]> {
    const findings: Finding[] = [toolFinding(ctx, "osv-scanner", OSV_REMEDIATION), ...sscsbFindings(ctx)];
    if ((await readIfExists(join(ctx.targetDir, SSCSB_CONFIG_PATH))) !== null) {
      findings.push({ level: "ok", message: `sscsb initialized in this repo (${SSCSB_CONFIG_PATH} present)` });
    }
    const errors = validateSupplyChainOptions(moduleOptions(ctx));
    findings.push(...errors.map((message) => ({ level: "error" as const, message })));
    return findings;
  },

  async plan(ctx: Ctx): Promise<PlannedAction[]> {
    void ctx;
    return [
      {
        kind: "write",
        path: DEPENDENCIES_POLICY_PATH,
        description:
          "write dependency policy (registry allowlist, min package age, lockfile + install review requirements, AI-native dependency rules)",
      },
      {
        kind: "info",
        description: "check ecosystem manifests for missing lockfiles",
      },
    ];
  },

  async apply(ctx: Ctx): Promise<ModuleResult> {
    const options = moduleOptions(ctx);
    const errors = validateSupplyChainOptions(options);
    if (errors.length > 0) {
      return {
        status: "failed",
        findings: errors.map((message) => ({ level: "error" as const, message })),
        wrotePaths: [],
      };
    }

    await writePolicy(ctx, DEPENDENCIES_POLICY_PATH, buildPolicy(resolveMinAgeDays(options)));
    const findings: Finding[] = [
      { level: "ok", message: `wrote ${DEPENDENCIES_POLICY_PATH}` },
      ...(await checkLockfiles(ctx.targetDir)),
      ...sscsbFindings(ctx),
    ];

    if (ctx.tools["osv-scanner"]?.present !== true) {
      findings.push({
        level: "degraded",
        message: "osv-scanner not installed — dependency vulnerability scanning unavailable",
        remediation: OSV_REMEDIATION,
      });
      return { status: "degraded", findings, wrotePaths: [DEPENDENCIES_POLICY_PATH] };
    }
    return { status: "applied", findings, wrotePaths: [DEPENDENCIES_POLICY_PATH] };
  },

  async verify(ctx: Ctx) {
    const artifact = await verifyJsonArtifact(ctx, DEPENDENCIES_POLICY_PATH);
    if (artifact.level !== "ok") return { ok: false, findings: [artifact] };

    const parsed = (await readJson(ctx, DEPENDENCIES_POLICY_PATH)) as Partial<DependenciesPolicy> | null;
    if (parsed === null) return { ok: false, findings: [artifact] };

    const findings: Finding[] = [artifact];
    let ok = true;

    const traditionalValid =
      Array.isArray(parsed.registries?.allowlist) &&
      parsed.registries.allowlist.length > 0 &&
      typeof parsed.minAgeDays === "number" &&
      parsed.requireLockfiles === true &&
      typeof parsed.installReview === "string" &&
      typeof parsed.typosquattingPolicy === "string";
    if (!traditionalValid) {
      ok = false;
      findings.push({
        level: "error",
        message: `${DEPENDENCIES_POLICY_PATH} missing or malformed traditional-dependency section`,
        remediation: "run `ade apply` to regenerate",
      });
    }

    const aiNative = parsed.aiNativeDependencies;
    const aiNativeValid =
      aiNative !== null &&
      typeof aiNative === "object" &&
      AI_NATIVE_DEP_KINDS.every((kind) => {
        const entry = (aiNative as Record<string, unknown>)[kind] as
          | { rule?: unknown; pinningRequired?: unknown; sourceAllowlist?: unknown }
          | undefined;
        return (
          entry !== undefined &&
          typeof entry.rule === "string" &&
          entry.pinningRequired === true &&
          Array.isArray(entry.sourceAllowlist)
        );
      });
    if (!aiNativeValid) {
      ok = false;
      findings.push({
        level: "error",
        message: `${DEPENDENCIES_POLICY_PATH} missing or malformed aiNativeDependencies section`,
        remediation: "run `ade apply` to regenerate",
      });
    }

    // Deep SSCS layer state (recognition only — never a failure): sscsb owns
    // its own verification via `sscsb verify`.
    if ((await readIfExists(join(ctx.targetDir, SSCSB_CONFIG_PATH))) !== null) {
      findings.push({ level: "ok", message: `sscsb initialized in this repo (${SSCSB_CONFIG_PATH} present)` });
    } else if (ctx.tools["sscsb"]?.present === true) {
      findings.push({
        level: "info",
        message: "sscsb installed but this repo is not sscsb-initialized",
        remediation: "run `sscsb init` then `sscsb verify` in the target repo for the deep SSCS layer",
      });
    }

    return { ok, findings };
  },
};
