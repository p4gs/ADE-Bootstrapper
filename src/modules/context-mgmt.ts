/**
 * Module: codebase context management.
 * Spec component: "Codebase context management — a generated, always-current
 * codebase map (.ade/context/codemap.md) that gives coding harnesses a cheap
 * structural summary of the repository: top-level layout, file counts by
 * extension, and detected entry points."
 * Boundary controlled: the context/retrieval boundary — token waste from
 * whole-repo scans and stale context from hand-maintained maps. The codemap
 * is re-derived from the tree on every `ade apply`, so harnesses consult a
 * current summary instead of re-walking (and re-tokenizing) the repository.
 */
import { join } from "node:path";
import { readIfExists } from "../fsutil.ts";
import type { AdeModule, Ctx, Finding } from "../types.ts";

export const CODEMAP_PATH = ".ade/context/codemap.md";

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
        "A generated codebase map lives at `.ade/context/codemap.md` (top-level layout, file counts by extension, entry points).",
        "- Consult the codemap BEFORE any whole-repo scan or directory dump.",
        "- Prefer targeted file reads over directory dumps; read only the files the task needs.",
        "- The codemap is regenerated on every `ade apply` — after structural changes, run `ade apply` instead of re-walking the tree.",
      ].join("\n"),
    },
  ],

  async detect(ctx: Ctx): Promise<Finding[]> {
    const existing = await readIfExists(join(ctx.targetDir, CODEMAP_PATH));
    if (existing === null) {
      return [
        {
          level: "info",
          message: `${CODEMAP_PATH} not yet generated`,
          remediation: "run `ade apply`",
        },
      ];
    }
    return [{ level: "ok", message: `${CODEMAP_PATH} present` }];
  },

  async plan(ctx: Ctx) {
    void ctx;
    return [
      {
        kind: "write" as const,
        path: CODEMAP_PATH,
        description:
          "generate codebase map (top-level dirs, extension counts, entry points) from the target tree",
      },
    ];
  },

  async apply(ctx: Ctx) {
    const tree = await snapshotTree(ctx.targetDir);
    await ctx.artifacts.write(CODEMAP_PATH, buildCodemap(tree));
    return {
      status: "applied" as const,
      findings: [
        {
          level: "ok" as const,
          message: `wrote ${CODEMAP_PATH} (${tree.files.length} files mapped)`,
        },
      ],
      wrotePaths: [CODEMAP_PATH],
    };
  },

  async verify(ctx: Ctx) {
    const content = await readIfExists(join(ctx.targetDir, CODEMAP_PATH));
    if (content === null) {
      return {
        ok: false,
        findings: [
          {
            level: "error" as const,
            message: `${CODEMAP_PATH} missing`,
            remediation: "run `ade apply`",
          },
        ],
      };
    }
    const missing: string[] = SECTION_MARKERS.filter((marker) => !content.includes(marker));
    if (!content.includes(REFRESH_SENTENCE)) {
      missing.push(`Refresh contract ("${REFRESH_SENTENCE}")`);
    }
    if (missing.length > 0) {
      return {
        ok: false,
        findings: [
          {
            level: "error" as const,
            message: `${CODEMAP_PATH} missing structure markers: ${missing.join(", ")}`,
            remediation: "run `ade apply` to regenerate",
          },
        ],
      };
    }
    return {
      ok: true,
      findings: [{ level: "ok" as const, message: `${CODEMAP_PATH} present with expected sections` }],
    };
  },
};
