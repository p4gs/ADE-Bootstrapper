import { afterEach, beforeEach, describe, expect, test } from "bun:test";
import { join } from "node:path";
import {
  contextMgmtModule,
  buildCodemap,
  parseCargoBins,
  snapshotTree,
  CODEMAP_PATH,
  REFRESH_SENTENCE,
  SECTION_MARKERS,
  type TreeSnapshot,
} from "../../src/modules/context-mgmt.ts";
import { makeTempDir, makeTestCtx, removeDir } from "../helpers.ts";
import { sha256 } from "../../src/fsutil.ts";

let dir: string;

beforeEach(async () => {
  dir = await makeTempDir();
});

afterEach(async () => {
  await removeDir(dir);
});

/** A polyglot fixture tree exercising every entry-point detector and skip rule. */
async function writeFixtureTree(root: string): Promise<void> {
  const files: Record<string, string> = {
    "package.json": JSON.stringify({
      name: "fixture",
      main: "src/index.ts",
      bin: { fixture: "src/cli.ts" },
      scripts: { build: "tsc", test: "bun test" },
    }),
    "src/index.ts": "export {};\n",
    "src/cli.ts": "export {};\n",
    "src/util/math.ts": "export {};\n",
    "tests/index.test.ts": "// t\n",
    "readme.md": "# fixture\n",
    "main.go": "package main\n",
    "Cargo.toml": [
      "[package]",
      'name = "fixture"',
      "",
      "[[bin]]",
      'name = "fixture-bin"',
      'path = "src/bin/fixture.rs"',
      "",
    ].join("\n"),
    // Everything below must be SKIPPED by the scan (ISC-72).
    "node_modules/dep/index.js": "// dep\n",
    ".git/HEAD": "ref: refs/heads/main\n",
    ".ade/policy/old.json": "{}\n",
    "dist/out.js": "// built\n",
    "build/artifact.js": "// built\n",
    "coverage/lcov.info": "TN:\n",
  };
  for (const [rel, content] of Object.entries(files)) {
    await Bun.write(join(root, rel), content);
  }
}

describe("context module", () => {
  test("ISC-72: apply generates codemap with top-level dirs, extension counts, and all entry-point kinds", async () => {
    await writeFixtureTree(dir);
    const ctx = makeTestCtx(dir);
    const result = await contextMgmtModule.apply(ctx);
    expect(result.status).toBe("applied");
    expect(result.wrotePaths).toContain(CODEMAP_PATH);

    const map = await Bun.file(join(dir, CODEMAP_PATH)).text();
    // Top-level directory listing (only unskipped dirs).
    expect(map).toContain("- `src/`");
    expect(map).toContain("- `tests/`");
    // Extension counts: 4 .ts files live outside skipped dirs.
    expect(map).toContain("| .ts | 4 |");
    expect(map).toContain("| .md | 1 |");
    // Entry points: package.json main/bin/scripts, src/index.*, src/cli.*, main.go, Cargo [[bin]].
    expect(map).toContain("package.json main: `src/index.ts`");
    expect(map).toContain("package.json bin: fixture → `src/cli.ts`");
    expect(map).toContain("package.json scripts: build, test");
    expect(map).toContain("- `src/index.ts`");
    expect(map).toContain("- `src/cli.ts`");
    expect(map).toContain("- `main.go`");
    expect(map).toContain("Cargo.toml [[bin]]: fixture-bin (`src/bin/fixture.rs`)");
  });

  test("ISC-72: node_modules/.git/.ade/dist/build/coverage are excluded from scan and counts", async () => {
    await writeFixtureTree(dir);
    const tree = await snapshotTree(dir);
    for (const skipped of ["node_modules/", ".git/", ".ade/", "dist/", "build/", "coverage/"]) {
      expect(tree.files.some((file) => file.startsWith(skipped))).toBe(false);
    }
    const map = buildCodemap(tree);
    // Skipped dirs never appear as listed directories (the prose skip-list note is expected).
    expect(map).not.toContain("- `node_modules/`");
    expect(map).not.toContain("- `.git/`");
    // The only .js and .info files live inside skipped dirs → no such extension rows.
    expect(map).not.toContain("| .js |");
    expect(map).not.toContain("| .info |");
    expect(map).not.toContain("- `dist/`");
    expect(map).not.toContain("- `coverage/`");
  });

  test("ISC-73: codemap generation is deterministic — identical trees in two temp copies produce byte-identical output", async () => {
    const other = await makeTempDir();
    try {
      await writeFixtureTree(dir);
      await writeFixtureTree(other);
      await contextMgmtModule.apply(makeTestCtx(dir));
      await contextMgmtModule.apply(makeTestCtx(other));
      const a = await Bun.file(join(dir, CODEMAP_PATH)).text();
      const b = await Bun.file(join(other, CODEMAP_PATH)).text();
      expect(a).toBe(b);
      // No absolute paths leak into the artifact.
      expect(a).not.toContain(dir);
      expect(b).not.toContain(other);
    } finally {
      await removeDir(other);
    }
  });

  test("ISC-73/ISC-117: double apply is idempotent — second run byte-identical", async () => {
    await writeFixtureTree(dir);
    await contextMgmtModule.apply(makeTestCtx(dir));
    const first = sha256(await Bun.file(join(dir, CODEMAP_PATH)).text());
    await contextMgmtModule.apply(makeTestCtx(dir));
    const second = sha256(await Bun.file(join(dir, CODEMAP_PATH)).text());
    expect(second).toBe(first);
  });

  test("ISC-74: Refresh section names `ade apply` and verify checks structure markers", async () => {
    await writeFixtureTree(dir);
    const ctx = makeTestCtx(dir);
    await contextMgmtModule.apply(ctx);
    const map = await Bun.file(join(dir, CODEMAP_PATH)).text();
    expect(map).toContain("## Refresh");
    expect(map).toContain(REFRESH_SENTENCE);
    expect(REFRESH_SENTENCE).toContain("`ade apply`");
    expect((await contextMgmtModule.verify(ctx)).ok).toBe(true);
  });

  test("ISC-74: verify fails when codemap is missing or a section header is tampered away", async () => {
    const ctx = makeTestCtx(dir);
    // Missing entirely.
    const missing = await contextMgmtModule.verify(ctx);
    expect(missing.ok).toBe(false);
    expect(missing.findings.some((finding) => finding.remediation?.includes("ade apply"))).toBe(true);

    // Present but a structure marker removed.
    await contextMgmtModule.apply(ctx);
    const map = await Bun.file(join(dir, CODEMAP_PATH)).text();
    await Bun.write(join(dir, CODEMAP_PATH), map.replace("## Entry Points", "## Something Else"));
    const tampered = await contextMgmtModule.verify(ctx);
    expect(tampered.ok).toBe(false);
    expect(tampered.findings.some((finding) => finding.message.includes("## Entry Points"))).toBe(true);

    // Refresh contract sentence removed also fails.
    await contextMgmtModule.apply(makeTestCtx(dir));
    const regenerated = await Bun.file(join(dir, CODEMAP_PATH)).text();
    await Bun.write(join(dir, CODEMAP_PATH), regenerated.replace(REFRESH_SENTENCE, "hand-maintained"));
    expect((await contextMgmtModule.verify(ctx)).ok).toBe(false);
  });

  test("ISC-75: instruction block directs harnesses to the codemap, targeted reads, and ade apply regeneration", () => {
    expect(contextMgmtModule.instructionBlocks.length).toBeGreaterThan(0);
    const block = contextMgmtModule.instructionBlocks[0]!;
    expect(block.content).toContain(".ade/context/codemap.md");
    expect(block.content).toContain("whole-repo");
    expect(block.content.toLowerCase()).toContain("targeted");
    expect(block.content).toContain("`ade apply`");
  });

  test("ISC-116: plan writes nothing to disk", async () => {
    await writeFixtureTree(dir);
    const ctx = makeTestCtx(dir);
    const actions = await contextMgmtModule.plan(ctx);
    expect(actions.length).toBeGreaterThan(0);
    expect(actions[0]!.path).toBe(CODEMAP_PATH);
    expect(await Bun.file(join(dir, CODEMAP_PATH)).exists()).toBe(false);
  });

  test("detect reports codemap absence with remediation, presence after apply", async () => {
    const ctx = makeTestCtx(dir);
    const before = await contextMgmtModule.detect(ctx);
    expect(before.some((finding) => finding.remediation?.includes("ade apply"))).toBe(true);
    await contextMgmtModule.apply(ctx);
    const after = await contextMgmtModule.detect(ctx);
    expect(after.every((finding) => finding.level === "ok")).toBe(true);
  });

  test("buildCodemap: extension table is capped at top 10, ordered count-desc then extension-asc", () => {
    const files: string[] = [];
    // 12 distinct extensions; .e00 has 3 files, .e01 has 2, the rest 1 each.
    for (let i = 0; i < 12; i++) {
      const ext = `e${String(i).padStart(2, "0")}`;
      files.push(`src/a.${ext}`);
      if (i === 0) files.push(`src/b.${ext}`, `src/c.${ext}`);
      if (i === 1) files.push(`src/b.${ext}`);
    }
    const map = buildCodemap({ files: files.sort(), packageJson: null, cargoToml: null });
    const rows = map.split("\n").filter((line) => /^\| \.e\d\d \| \d+ \|$/.test(line));
    expect(rows.length).toBe(10);
    expect(rows[0]).toBe("| .e00 | 3 |");
    expect(rows[1]).toBe("| .e01 | 2 |");
    // Ties (count 1) resolve alphabetically; the two largest extensions drop off.
    expect(rows[2]).toBe("| .e02 | 1 |");
    expect(map).not.toContain("| .e10 |");
    expect(map).not.toContain("| .e11 |");
  });

  test("buildCodemap: empty tree and no manifests render placeholder sections deterministically", () => {
    const empty: TreeSnapshot = { files: [], packageJson: null, cargoToml: null };
    const map = buildCodemap(empty);
    for (const marker of SECTION_MARKERS) expect(map).toContain(marker);
    expect(map).toContain("- (none)");
    expect(map).toContain("- (none detected)");
    expect(buildCodemap(empty)).toBe(map);
  });

  test("buildCodemap: package.json string-form bin is detected as an entry point", () => {
    const map = buildCodemap({
      files: ["cli.js"],
      packageJson: { bin: "cli.js" },
      cargoToml: null,
    });
    expect(map).toContain("package.json bin: `cli.js`");
  });

  test("parseCargoBins: extracts every [[bin]] block and ignores other sections", () => {
    const toml = [
      "[package]",
      'name = "not-a-bin"',
      "",
      "[[bin]]",
      'name = "alpha"',
      'path = "src/bin/alpha.rs"',
      "",
      "[dependencies]",
      'serde = "1"',
      "",
      "[[bin]]",
      'name = "beta"',
      "",
    ].join("\n");
    const bins = parseCargoBins(toml);
    expect(bins).toEqual([
      { name: "alpha", path: "src/bin/alpha.rs" },
      { name: "beta", path: null },
    ]);
  });
});
