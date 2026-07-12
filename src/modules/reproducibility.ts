/**
 * Module: reproducible environment & manifest management.
 * Spec component: "Reproducible environment and lockfile management" — an
 * environment manifest (`.ade/manifest.json`) recording os/arch, integrated
 * tool versions, core toolchain versions (bun, git), and harness CLI versions
 * so any drift between machines is visible instead of silent.
 * Boundary controlled: the environment boundary (works-on-my-machine drift).
 *
 * Machine-varying values (versions) live ONLY here and in the lockfile's
 * environment section — that is their designed home. The manifest content is
 * still deterministically ORDERED (writePolicy → stableStringify), and version
 * drift is informational: verify reports it, never fails on it.
 */
import { HARNESS_ADAPTERS } from "../harness/adapters.ts";
import { readJson, verifyJsonArtifact, writePolicy } from "./_shared.ts";
import type { AdeModule, Ctx, Finding, ModuleResult, PlannedAction } from "../types.ts";

export const MANIFEST_PATH = ".ade/manifest.json";

/** Core toolchain executables always probed for the manifest. */
export const CORE_TOOLS = ["bun", "git"] as const;

interface EnvironmentManifest {
  schemaVersion: number;
  os: string;
  arch: string;
  /** Core toolchain versions (bun, git) — null when unresolvable. */
  core: Record<string, string | null>;
  /** Integrated tool versions from detection — null when absent. */
  tools: Record<string, string | null>;
  /** Harness CLI versions keyed by adapter id — null when the CLI is absent. */
  harnesses: Record<string, string | null>;
}

/**
 * Probe `<cmd> --version` and return the first output line, or null on ANY
 * failure (missing binary, nonzero exit, empty output, exec throw). Absence
 * is a fact the manifest records — never an error.
 */
export async function probeVersion(ctx: Ctx, cmd: string): Promise<string | null> {
  try {
    const result = await ctx.exec([cmd, "--version"]);
    if (result.code !== 0) return null;
    const firstLine = result.stdout.split("\n")[0]?.trim();
    return firstLine !== undefined && firstLine.length > 0 ? firstLine : null;
  } catch {
    return null;
  }
}

/** Current integrated-tool versions from detection (name → version|null). */
function currentToolVersions(ctx: Ctx): Record<string, string | null> {
  const tools: Record<string, string | null> = {};
  for (const [name, info] of Object.entries(ctx.tools)) {
    tools[name] = info.present ? (info.version ?? null) : null;
  }
  return tools;
}

async function buildManifest(ctx: Ctx): Promise<EnvironmentManifest> {
  const core: Record<string, string | null> = {};
  for (const name of CORE_TOOLS) {
    core[name] = await probeVersion(ctx, name);
  }

  const harnesses: Record<string, string | null> = {};
  for (const adapter of HARNESS_ADAPTERS) {
    const cli = adapter.cliNames.find((name) => ctx.which(name) !== null);
    harnesses[adapter.id] = cli !== undefined ? await probeVersion(ctx, cli) : null;
  }

  return {
    schemaVersion: 1,
    os: ctx.os,
    arch: ctx.arch,
    core,
    tools: currentToolVersions(ctx),
    harnesses,
  };
}

export const reproducibilityModule: AdeModule = {
  id: "reproducibility",
  title: "Reproducible Environment",
  category: "reproducibility",
  spec: "Reproducible environment and lockfile management",
  defaultEnabled: true,
  instructionBlocks: [
    {
      id: "reproducibility",
      title: "Reproducible Environment",
      content: [
        "The environment manifest is at `.ade/manifest.json` (os/arch, tool and harness CLI versions).",
        "- Before assuming a tool exists, check the manifest; a `null` version means it was absent at bootstrap time.",
        "- Report version drift between the manifest and the live environment to the human rather than working around it silently.",
        "- Do not hand-edit the manifest; regenerate it with `ade apply` so it reflects the real environment.",
      ].join("\n"),
    },
  ],

  async detect(ctx: Ctx): Promise<Finding[]> {
    const findings: Finding[] = [
      { level: "info", message: `environment: ${ctx.os}/${ctx.arch}` },
    ];
    for (const name of CORE_TOOLS) {
      findings.push(
        ctx.which(name) !== null
          ? { level: "ok", message: `${name} resolvable on PATH` }
          : { level: "info", message: `${name} not on PATH — manifest will record null` },
      );
    }
    const detectedHarnesses = HARNESS_ADAPTERS.filter((adapter) =>
      adapter.cliNames.some((name) => ctx.which(name) !== null),
    ).map((adapter) => adapter.id);
    findings.push({
      level: "info",
      message:
        detectedHarnesses.length > 0
          ? `harness CLIs detected: ${detectedHarnesses.join(", ")}`
          : "no harness CLIs detected — manifest will record null versions",
    });
    return findings;
  },

  async plan(ctx: Ctx): Promise<PlannedAction[]> {
    void ctx;
    return [
      {
        kind: "write",
        path: MANIFEST_PATH,
        description: "write environment manifest (os/arch, core + integrated tool versions, harness CLI versions)",
      },
    ];
  },

  async apply(ctx: Ctx): Promise<ModuleResult> {
    // ISC-108: tolerates everything missing — every probe failure becomes a
    // recorded null, and a fully-null manifest is still a valid, applied state.
    const manifest = await buildManifest(ctx);
    await writePolicy(ctx, MANIFEST_PATH, manifest);
    return {
      status: "applied",
      findings: [{ level: "ok", message: `wrote ${MANIFEST_PATH}` }],
      wrotePaths: [MANIFEST_PATH],
    };
  },

  async verify(ctx: Ctx) {
    const artifact = await verifyJsonArtifact(ctx, MANIFEST_PATH);
    if (artifact.level !== "ok") return { ok: false, findings: [artifact] };

    const parsed = (await readJson(ctx, MANIFEST_PATH)) as Partial<EnvironmentManifest> | null;
    if (
      parsed === null ||
      typeof parsed.os !== "string" ||
      typeof parsed.arch !== "string" ||
      parsed.tools === null ||
      typeof parsed.tools !== "object"
    ) {
      return {
        ok: false,
        findings: [
          {
            level: "error",
            message: `${MANIFEST_PATH} missing required keys (os, arch, tools)`,
            remediation: "run `ade apply` to regenerate the manifest",
          },
        ],
      };
    }

    // ISC-109: drift between the recorded manifest and the live environment
    // is informational — surfaced to the human, never a verification failure.
    const findings: Finding[] = [artifact];
    const recorded = parsed.tools as Record<string, string | null>;
    for (const [name, current] of Object.entries(currentToolVersions(ctx))) {
      const stored = name in recorded ? recorded[name] ?? null : undefined;
      if (stored === undefined) {
        findings.push({
          level: "info",
          message: `tool ${name} not recorded in manifest (current: ${current ?? "absent"})`,
          remediation: "run `ade apply` to refresh the manifest",
        });
      } else if (stored !== current) {
        findings.push({
          level: "info",
          message: `version drift for ${name}: manifest has ${stored ?? "absent"}, environment has ${current ?? "absent"}`,
          remediation: "run `ade apply` to refresh the manifest after confirming the drift is intended",
        });
      }
    }
    return { ok: true, findings };
  },
};
