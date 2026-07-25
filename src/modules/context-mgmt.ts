/**
 * Module: codebase context management.
 * Spec component: "Codebase context management — an always-current codebase
 * understanding layer for coding harnesses."
 *
 * Two tiers, following ADE's integrate-before-rebuild convention:
 *   1. A zero-dependency structural codemap (.ade/context/codemap.md),
 *      re-derived from the tree on every `ade apply` — always present.
 *   2. Detected best-of-breed engines, wired in when installed:
 *        - OpenWiki (Code Brain): an auto-maintained, navigable prose+Mermaid
 *          codebase wiki (MIT). `openwiki/` dir; refreshes from git diffs.
 *        - CocoIndex: AST-based semantic code search / retrieval (Apache-2.0).
 *        - OpenWiki Personal Brain: a DISTINCT opt-in sub-capability of
 *          OpenWiki — general-purpose agent memory synthesised from external
 *          sources (email, notes, web), complementary to (not the same as)
 *          the codebase wiki. Opt-in via options.enableBrain because it
 *          reaches outside the repo.
 * Boundary controlled: the context/retrieval boundary — token waste from
 * whole-repo scans and stale context from hand-maintained maps.
 *
 * Convention: like RTK/nono/OpenMemory, engines are detected + wired + given
 * exact install guidance; `ade apply` NEVER force-installs a tool.
 */
import { join } from "node:path";
import { readIfExists } from "../fsutil.ts";
import { readJson, verifyJsonArtifact, writePolicy } from "./_shared.ts";
import type { AdeModule, Ctx, Finding, ModuleResult, PlannedAction } from "../types.ts";

export const CODEMAP_PATH = ".ade/context/codemap.md";
export const CONTEXT_ENGINES_PATH = ".ade/policy/context-engines.json";
export const OPENWIKI_DIR = "openwiki/";

export const OPENWIKI_INSTALL =
  "install OpenWiki (`npm install -g openwiki`) for an auto-maintained, navigable codebase wiki — MIT-licensed, BYO model key; init with `openwiki --init`";
export const COCOINDEX_INSTALL =
  "install CocoIndex for AST-based semantic code search — `pip install cocoindex` (framework) or the `cocoindex-code` CLI (github.com/cocoindex-io/cocoindex-code); Apache-2.0";
export const BRAIN_ACTIVATION =
  "to enable OpenWiki Personal Brain, set modules.context.options.enableBrain = true in ade.json, then run `ade apply`; initialise with `openwiki personal --init`";

/** Directory names excluded from the codemap scan (any path segment). */
export const SKIPPED_DIRS = ["node_modules", ".git", ".ade", ".claude", ".cursor", "dist", "build", "coverage"] as const;

/**
 * ADE-managed root files excluded from the codemap: they are generated/updated at
 * different points of the apply pipeline, so including them would make the codemap
 * order-dependent (init-vs-reapply drift). The codemap maps the USER's codebase.
 */
export const SKIPPED_FILES = [
  "ade.json",
  "ade.lock.json",
  "CLAUDE.md",
  "AGENTS.md",
  ".gitignore",
  ".mcp.json",
  ".pre-commit-config.yaml",
] as const;

/** Structure markers verify() requires — regenerating always emits all of them. */
export const SECTION_MARKERS = [
  "# Codemap",
  "## Top-Level Directories",
  "## File Counts by Extension",
  "## Entry Points",
  "## Refresh",
] as const;

/** The exact refresh-contract sentence fragment (ISC-74). */
export const REFRESH_SENTENCE = "regenerated on every `ade apply`";

const SKIP = new Set<string>(SKIPPED_DIRS);
const SKIP_FILES = new Set<string>(SKIPPED_FILES);

/** Pure input to the codemap builder — everything repo-relative, nothing absolute. */
export interface TreeSnapshot {
  /** Repo-relative file paths, forward-slash separated, skip-dirs excluded, sorted. */
  files: string[];
  /** Parsed package.json (object form) or null when absent/invalid. */
  packageJson: Record<string, unknown> | null;
  /** Raw Cargo.toml text or null when absent. */
  cargoToml: string | null;
}

function includePath(relPath: string): boolean {
  if (SKIP_FILES.has(relPath)) return false;
  return !relPath.split("/").some((segment) => SKIP.has(segment));
}

/** Scan the target tree into a deterministic snapshot (sorted, repo-relative only). */
export async function snapshotTree(targetDir: string): Promise<TreeSnapshot> {
  const glob = new Bun.Glob("**/*");
  const files: string[] = [];
  for await (const entry of glob.scan({ cwd: targetDir, onlyFiles: true, dot: true })) {
    const rel = entry.replaceAll("\\", "/");
    if (includePath(rel)) files.push(rel);
  }
  files.sort();

  let packageJson: Record<string, unknown> | null = null;
  const pkgText = await readIfExists(join(targetDir, "package.json"));
  if (pkgText !== null) {
    try {
      const parsed: unknown = JSON.parse(pkgText);
      if (parsed !== null && typeof parsed === "object" && !Array.isArray(parsed)) {
        packageJson = parsed as Record<string, unknown>;
      }
    } catch {
      packageJson = null;
    }
  }
  const cargoToml = await readIfExists(join(targetDir, "Cargo.toml"));
  return { files, packageJson, cargoToml };
}

/** Minimal `[[bin]]` extraction from Cargo.toml (string-literal name/path only). */
export function parseCargoBins(toml: string): { name: string | null; path: string | null }[] {
  const bins: { name: string | null; path: string | null }[] = [];
  let current: { name: string | null; path: string | null } | null = null;
  for (const raw of toml.split("\n")) {
    const line = raw.trim();
    if (/^\[\[bin\]\]$/.test(line)) {
      current = { name: null, path: null };
      bins.push(current);
      continue;
    }
    if (line.startsWith("[")) {
      current = null;
      continue;
    }
    if (current === null) continue;
    const match = /^(name|path)\s*=\s*"([^"]*)"/.exec(line);
    if (match !== null) {
      if (match[1] === "name") current.name = match[2] ?? null;
      else current.path = match[2] ?? null;
    }
  }
  return bins;
}

/** File counts by extension: top 10, count-descending then extension-ascending. */
function extensionCounts(files: string[]): [string, number][] {
  const counts = new Map<string, number>();
  for (const file of files) {
    const base = file.split("/").pop() ?? file;
    const idx = base.lastIndexOf(".");
    const ext = idx > 0 ? base.slice(idx) : "(no extension)";
    counts.set(ext, (counts.get(ext) ?? 0) + 1);
  }
  return [...counts.entries()]
    .sort((a, b) => b[1] - a[1] || (a[0] < b[0] ? -1 : 1))
    .slice(0, 10);
}

/** Detected entry points, in a fixed deterministic order. */
function entryPoints(tree: TreeSnapshot): string[] {
  const entries: string[] = [];
  const pkg = tree.packageJson;
  if (pkg !== null) {
    if (typeof pkg["main"] === "string") {
      entries.push(`package.json main: \`${pkg["main"]}\``);
    }
    const bin = pkg["bin"];
    if (typeof bin === "string") {
      entries.push(`package.json bin: \`${bin}\``);
    } else if (bin !== null && typeof bin === "object" && !Array.isArray(bin)) {
      const binMap = bin as Record<string, unknown>;
      for (const name of Object.keys(binMap).sort()) {
        const target = binMap[name];
        if (typeof target === "string") {
          entries.push(`package.json bin: ${name} → \`${target}\``);
        }
      }
    }
    const scripts = pkg["scripts"];
    if (scripts !== null && typeof scripts === "object" && !Array.isArray(scripts)) {
      const names = Object.keys(scripts as Record<string, unknown>).sort();
      if (names.length > 0) entries.push(`package.json scripts: ${names.join(", ")}`);
    }
  }
  for (const file of tree.files) {
    if (/^src\/(index|cli)\.[^/]+$/.test(file)) entries.push(`\`${file}\``);
  }
  if (tree.files.includes("main.go")) entries.push("`main.go`");
  if (tree.cargoToml !== null) {
    for (const bin of parseCargoBins(tree.cargoToml)) {
      const name = bin.name ?? "(unnamed)";
      const path = bin.path ?? "src/bin (cargo default)";
      entries.push(`Cargo.toml [[bin]]: ${name} (\`${path}\`)`);
    }
  }
  return entries;
}

/**
 * Codemap builder: tree snapshot → markdown. Pure and deterministic — sorted
 * everywhere, repo-relative paths only, no timestamps. Exported for direct testing.
 */
export function buildCodemap(tree: TreeSnapshot): string {
  const dirs = [
    ...new Set(
      tree.files
        .filter((file) => file.includes("/"))
        .map((file) => file.split("/")[0] ?? file),
    ),
  ].sort();

  const lines: string[] = [
    "# Codemap",
    "",
    "Machine-generated map of this repository (repo-relative paths only).",
    "Consult it before scanning the tree.",
    "",
    "## Top-Level Directories",
    "",
  ];
  if (dirs.length === 0) lines.push("- (none)");
  else for (const dir of dirs) lines.push(`- \`${dir}/\``);

  const exts = extensionCounts(tree.files);
  lines.push(
    "",
    "## File Counts by Extension",
    "",
    `${tree.files.length} files total (skipping ${SKIPPED_DIRS.join(", ")}). Top ${exts.length} extensions:`,
    "",
    "| Extension | Files |",
    "| --- | --- |",
  );
  for (const [ext, count] of exts) lines.push(`| ${ext} | ${count} |`);

  lines.push("", "## Entry Points", "");
  const entries = entryPoints(tree);
  if (entries.length === 0) lines.push("- (none detected)");
  else for (const entry of entries) lines.push(`- ${entry}`);

  lines.push(
    "",
    "## Refresh",
    "",
    `This codemap is ${REFRESH_SENTENCE}. Do not edit it by hand — when the repository structure changes, run \`ade apply\` to refresh it.`,
    "",
  );
  return lines.join("\n");
}

/** Presence + version of one context engine, resolved from the live machine. */
interface EngineState {
  present: boolean;
  version?: string;
}

/** OpenWiki (Code Brain + Personal Brain share the one `openwiki` binary). */
export function openwikiState(ctx: Ctx): EngineState {
  const info = ctx.tools["openwiki"];
  return info?.present === true ? { present: true, version: info.version } : { present: false };
}

/** CocoIndex is present if EITHER the framework (`cocoindex`) or the `ccc` CLI resolves. */
export function cocoindexState(ctx: Ctx): EngineState {
  for (const name of ["cocoindex", "ccc"] as const) {
    const info = ctx.tools[name];
    if (info?.present === true) return { present: true, version: info.version };
  }
  return { present: false };
}

/** Personal Brain is opt-in (reaches outside the repo) — off unless explicitly enabled. */
export function brainOptIn(ctx: Ctx): boolean {
  return ctx.config.modules["context"]?.options?.["enableBrain"] === true;
}

interface ContextEnginesPolicy {
  schemaVersion: number;
  fallback: string;
  engines: {
    codebaseWiki: {
      tool: "openwiki";
      role: string;
      present: boolean;
      enabled: boolean;
      version?: string;
      wikiDir: string;
      autoRefresh: string;
      install: string;
    };
    semanticIndex: {
      tool: "cocoindex";
      role: string;
      present: boolean;
      enabled: boolean;
      version?: string;
      install: string;
    };
    personalBrain: {
      tool: "openwiki";
      subCapabilityOf: "openwiki";
      role: string;
      present: boolean;
      optIn: boolean;
      enabled: boolean;
      install: string;
      activation: string;
    };
  };
}

/** Build the context-engines policy — deterministic, re-derivable from the live machine. */
export function buildEnginesPolicy(ctx: Ctx): ContextEnginesPolicy {
  const wiki = openwikiState(ctx);
  const coco = cocoindexState(ctx);
  const brainOn = brainOptIn(ctx);
  const codebaseWiki: ContextEnginesPolicy["engines"]["codebaseWiki"] = {
    tool: "openwiki",
    role: "auto-maintained navigable codebase wiki (prose + Mermaid); read FIRST for architecture understanding",
    present: wiki.present,
    enabled: wiki.present,
    wikiDir: OPENWIKI_DIR,
    autoRefresh:
      "regenerates from git diffs on re-run; enable continuous updates via OpenWiki's scheduled GitHub Action (openwiki-update.yml — daily update PR)",
    install: OPENWIKI_INSTALL,
  };
  if (wiki.version !== undefined) codebaseWiki.version = wiki.version;
  const semanticIndex: ContextEnginesPolicy["engines"]["semanticIndex"] = {
    tool: "cocoindex",
    role: "AST-based semantic code search / natural-language retrieval; use instead of whole-tree grep",
    present: coco.present,
    enabled: coco.present,
    install: COCOINDEX_INSTALL,
  };
  if (coco.version !== undefined) semanticIndex.version = coco.version;
  return {
    schemaVersion: 1,
    fallback: CODEMAP_PATH,
    engines: {
      codebaseWiki,
      semanticIndex,
      personalBrain: {
        tool: "openwiki",
        subCapabilityOf: "openwiki",
        role: "general-purpose agent memory synthesised from external sources (email, notes, web); complementary to the codebase wiki, NOT the same thing",
        present: wiki.present,
        optIn: brainOn,
        enabled: wiki.present && brainOn,
        install: OPENWIKI_INSTALL,
        activation: BRAIN_ACTIVATION,
      },
    },
  };
}

/** Advisory findings describing which engines are wired vs. absent. */
function engineFindings(ctx: Ctx): Finding[] {
  const policy = buildEnginesPolicy(ctx);
  const findings: Finding[] = [];
  const wiki = policy.engines.codebaseWiki;
  findings.push(
    wiki.present
      ? { level: "ok", message: `OpenWiki codebase wiki wired${wiki.version !== undefined ? ` (${wiki.version})` : ""}` }
      : { level: "degraded", message: "OpenWiki not installed — codebase wiki unavailable; using codemap fallback", remediation: OPENWIKI_INSTALL },
  );
  const idx = policy.engines.semanticIndex;
  findings.push(
    idx.present
      ? { level: "ok", message: `CocoIndex semantic search wired${idx.version !== undefined ? ` (${idx.version})` : ""}` }
      : { level: "degraded", message: "CocoIndex not installed — semantic code search unavailable", remediation: COCOINDEX_INSTALL },
  );
  const brain = policy.engines.personalBrain;
  findings.push({
    level: "info",
    message: `OpenWiki Personal Brain ${brain.enabled ? "enabled" : brain.optIn ? "opted-in but OpenWiki absent" : "off (opt-in sub-capability)"}`,
    remediation: brain.enabled ? undefined : BRAIN_ACTIVATION,
  });
  return findings;
}

export const contextMgmtModule: AdeModule = {
  id: "context",
  title: "Codebase Context Management",
  category: "context",
  spec: "Codebase context management",
  defaultEnabled: true,
  instructionBlocks: [
    {
      id: "context",
      title: "Codebase Context",
      content: [
        "Codebase-understanding sources, in priority order (see `.ade/policy/context-engines.json` for which are live):",
        "- **OpenWiki codebase wiki** (`openwiki/`) when present — read it FIRST for prose + Mermaid architecture understanding. It is auto-maintained; never hand-edit generated pages.",
        "- **CocoIndex semantic search** when present — use natural-language code retrieval instead of grepping the whole tree.",
        "- **`.ade/context/codemap.md`** — the always-present zero-dependency structural fallback; consult BEFORE any whole-repo scan. Regenerated on every `ade apply`.",
        "- **OpenWiki Personal Brain** (opt-in, `modules.context.options.enableBrain`) — general-purpose project/research memory across tools (email, notes, web). Distinct from the codebase wiki. NEVER write secrets or credentials into it.",
        "- Prefer targeted reads over directory dumps; after structural changes run `ade apply` (and re-run OpenWiki) rather than re-walking the tree.",
      ].join("\n"),
    },
  ],

  async detect(ctx: Ctx): Promise<Finding[]> {
    const existing = await readIfExists(join(ctx.targetDir, CODEMAP_PATH));
    const codemap: Finding = existing === null
      ? { level: "info", message: `${CODEMAP_PATH} not yet generated`, remediation: "run `ade apply`" }
      : { level: "ok", message: `${CODEMAP_PATH} present` };
    return [codemap, ...engineFindings(ctx)];
  },

  async plan(ctx: Ctx): Promise<PlannedAction[]> {
    return [
      {
        kind: "write" as const,
        path: CODEMAP_PATH,
        description:
          "generate codebase map (top-level dirs, extension counts, entry points) from the target tree",
      },
      {
        kind: "write" as const,
        path: CONTEXT_ENGINES_PATH,
        description: `record detected context engines (OpenWiki${brainOptIn(ctx) ? " + Personal Brain" : ""}, CocoIndex) and the codemap fallback`,
      },
    ];
  },

  async apply(ctx: Ctx): Promise<ModuleResult> {
    const tree = await snapshotTree(ctx.targetDir);
    await ctx.artifacts.write(CODEMAP_PATH, buildCodemap(tree));
    await writePolicy(ctx, CONTEXT_ENGINES_PATH, buildEnginesPolicy(ctx));
    return {
      status: "applied" as const,
      findings: [
        { level: "ok" as const, message: `wrote ${CODEMAP_PATH} (${tree.files.length} files mapped)` },
        { level: "ok" as const, message: `wrote ${CONTEXT_ENGINES_PATH}` },
        ...engineFindings(ctx),
      ],
      wrotePaths: [CODEMAP_PATH, CONTEXT_ENGINES_PATH],
    };
  },

  async verify(ctx: Ctx) {
    const findings: Finding[] = [];
    let ok = true;

    const content = await readIfExists(join(ctx.targetDir, CODEMAP_PATH));
    if (content === null) {
      return {
        ok: false,
        findings: [{ level: "error", message: `${CODEMAP_PATH} missing`, remediation: "run `ade apply`" }],
      };
    }
    const missing: string[] = SECTION_MARKERS.filter((marker) => !content.includes(marker));
    if (!content.includes(REFRESH_SENTENCE)) missing.push(`Refresh contract ("${REFRESH_SENTENCE}")`);
    if (missing.length > 0) {
      return {
        ok: false,
        findings: [
          {
            level: "error",
            message: `${CODEMAP_PATH} missing structure markers: ${missing.join(", ")}`,
            remediation: "run `ade apply` to regenerate",
          },
        ],
      };
    }
    findings.push({ level: "ok", message: `${CODEMAP_PATH} present with expected sections` });

    const engines = await verifyJsonArtifact(ctx, CONTEXT_ENGINES_PATH);
    findings.push(engines);
    if (engines.level !== "ok") return { ok: false, findings };

    const parsed = (await readJson(ctx, CONTEXT_ENGINES_PATH)) as ContextEnginesPolicy | null;
    const expected = buildEnginesPolicy(ctx);
    const e = parsed?.engines;
    if (
      e === undefined ||
      e.codebaseWiki?.enabled !== expected.engines.codebaseWiki.enabled ||
      e.semanticIndex?.enabled !== expected.engines.semanticIndex.enabled ||
      e.personalBrain?.enabled !== expected.engines.personalBrain.enabled
    ) {
      ok = false;
      findings.push({
        level: "error",
        message: `${CONTEXT_ENGINES_PATH} engine state does not match the current machine (installed tools / Brain opt-in changed)`,
        remediation: "run `ade apply` to re-derive the context-engines policy",
      });
    } else {
      findings.push({ level: "ok", message: `${CONTEXT_ENGINES_PATH} matches detected engines` });
    }

    return { ok, findings };
  },
};
