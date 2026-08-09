import { afterEach, beforeEach, describe, expect, test } from "bun:test";
import { join } from "node:path";
import {
  contextMgmtModule,
  buildCodemap,
  buildEnginesPolicy,
  brainOptIn,
  cocoindexState,
  openwikiState,
  serenaState,
  serenaMcpOptIn,
  serenaMcpEnabled,
  parseCargoBins,
  snapshotTree,
  CODEMAP_PATH,
  CONTEXT_ENGINES_PATH,
  OPENWIKI_INSTALL,
  COCOINDEX_INSTALL,
  BRAIN_ACTIVATION,
  SERENA_INSTALL,
  SERENA_MCP_ACTIVATION,
  SERENA_MCP_SERVER_NAME,
  REFRESH_SENTENCE,
  SECTION_MARKERS,
  type TreeSnapshot,
} from "../../src/modules/context-mgmt.ts";
import { makeTempDir, makeTestCtx, removeDir, testConfig } from "../helpers.ts";
import { readIfExists, sha256 } from "../../src/fsutil.ts";

/** Parse the written context-engines.json for a target dir. */
async function readEngines(dir: string): Promise<Record<string, unknown>> {
  const text = await readIfExists(join(dir, CONTEXT_ENGINES_PATH));
  if (text === null) throw new Error("context-engines.json not written");
  return JSON.parse(text) as Record<string, unknown>;
}

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
    // Codemap not yet generated → its finding carries an `ade apply` remediation.
    expect(before.some((finding) => finding.message.includes(CODEMAP_PATH) && finding.remediation?.includes("ade apply"))).toBe(
      true,
    );
    await contextMgmtModule.apply(ctx);
    const after = await contextMgmtModule.detect(ctx);
    // The codemap finding flips to ok; engine findings stay advisory (engines absent in the test env).
    expect(after.some((finding) => finding.level === "ok" && finding.message === `${CODEMAP_PATH} present`)).toBe(true);
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

describe("context-mgmt: OpenWiki + CocoIndex + Personal Brain integration", () => {
  let dir: string;
  beforeEach(async () => {
    dir = await makeTempDir();
  });
  afterEach(async () => {
    await removeDir(dir);
  });

  test("apply writes context-engines.json alongside the codemap; both in wrotePaths", async () => {
    const ctx = makeTestCtx(dir);
    const result = await contextMgmtModule.apply(ctx);
    expect(result.status).toBe("applied");
    expect(result.wrotePaths).toContain(CODEMAP_PATH);
    expect(result.wrotePaths).toContain(CONTEXT_ENGINES_PATH);
    const engines = await readEngines(dir);
    expect((engines as { fallback: string }).fallback).toBe(CODEMAP_PATH);
  });

  test("engines absent → all engines disabled, brain off, verify OK (state matches machine)", async () => {
    const ctx = makeTestCtx(dir); // no tools present
    await contextMgmtModule.apply(ctx);
    const policy = buildEnginesPolicy(ctx);
    expect(policy.engines.codebaseWiki.enabled).toBe(false);
    expect(policy.engines.semanticIndex.enabled).toBe(false);
    expect(policy.engines.personalBrain.enabled).toBe(false);
    expect(policy.engines.personalBrain.optIn).toBe(false);
    // Install guidance is carried in the policy so the contract is explicit.
    expect(policy.engines.codebaseWiki.install).toBe(OPENWIKI_INSTALL);
    expect(policy.engines.semanticIndex.install).toBe(COCOINDEX_INSTALL);
    const verdict = await contextMgmtModule.verify(ctx);
    expect(verdict.ok).toBe(true);
    // detect surfaces advisory (non-error) findings for the absent engines.
    const detected = await contextMgmtModule.detect(ctx);
    expect(detected.some((f) => f.level === "degraded" && f.remediation === OPENWIKI_INSTALL)).toBe(true);
    expect(detected.some((f) => f.level === "degraded" && f.remediation === COCOINDEX_INSTALL)).toBe(true);
    expect(detected.some((f) => f.remediation === BRAIN_ACTIVATION)).toBe(true);
  });

  test("OpenWiki present → codebase wiki enabled with version; Personal Brain present-but-off (opt-in default)", async () => {
    const ctx = makeTestCtx(dir, { presentTools: { openwiki: "openwiki 1.4.0" } });
    expect(openwikiState(ctx)).toEqual({ present: true, version: "openwiki 1.4.0" });
    const policy = buildEnginesPolicy(ctx);
    expect(policy.engines.codebaseWiki.enabled).toBe(true);
    expect(policy.engines.codebaseWiki.version).toBe("openwiki 1.4.0");
    // Brain shares the binary (present) but stays disabled until opted in.
    expect(policy.engines.personalBrain.present).toBe(true);
    expect(policy.engines.personalBrain.enabled).toBe(false);
    await contextMgmtModule.apply(ctx);
    expect((await contextMgmtModule.verify(ctx)).ok).toBe(true);
    const detected = await contextMgmtModule.detect(ctx);
    expect(detected.some((f) => f.level === "ok" && f.message.startsWith("OpenWiki codebase wiki wired"))).toBe(true);
  });

  test("CocoIndex present via the `cocoindex` framework → semantic index enabled", async () => {
    const ctx = makeTestCtx(dir, { presentTools: { cocoindex: "cocoindex 0.9" } });
    expect(cocoindexState(ctx)).toEqual({ present: true, version: "cocoindex 0.9" });
    expect(buildEnginesPolicy(ctx).engines.semanticIndex.enabled).toBe(true);
  });

  test("CocoIndex present via the `ccc` CLI alias → semantic index enabled (either binary counts)", async () => {
    const ctx = makeTestCtx(dir, { presentTools: { ccc: "ccc 0.3.2" } });
    expect(cocoindexState(ctx)).toEqual({ present: true, version: "ccc 0.3.2" });
    expect(buildEnginesPolicy(ctx).engines.semanticIndex.version).toBe("ccc 0.3.2");
  });

  test("Personal Brain opt-in + OpenWiki present → enabled; opt-in without OpenWiki → not enabled", async () => {
    const optIn = testConfig({ modules: { context: { options: { enableBrain: true } } } });
    const on = makeTestCtx(dir, { config: optIn, presentTools: { openwiki: "openwiki 1.4.0" } });
    expect(brainOptIn(on)).toBe(true);
    expect(buildEnginesPolicy(on).engines.personalBrain.enabled).toBe(true);

    const optInNoTool = makeTestCtx(dir, { config: optIn }); // opted in, OpenWiki absent
    const p = buildEnginesPolicy(optInNoTool);
    expect(p.engines.personalBrain.optIn).toBe(true);
    expect(p.engines.personalBrain.enabled).toBe(false);
    const detected = await contextMgmtModule.detect(optInNoTool);
    expect(detected.some((f) => f.message.includes("opted-in but OpenWiki absent"))).toBe(true);
  });

  test("verify FAILS when the recorded engine state drifts from the live machine", async () => {
    // Apply with no engines, then a tool appears — recorded enabled=false ≠ live present.
    const applyCtx = makeTestCtx(dir);
    await contextMgmtModule.apply(applyCtx);
    const driftedCtx = makeTestCtx(dir, { presentTools: { openwiki: "openwiki 1.4.0" } });
    const verdict = await contextMgmtModule.verify(driftedCtx);
    expect(verdict.ok).toBe(false);
    expect(verdict.findings.some((f) => f.level === "error" && f.remediation?.includes("re-derive"))).toBe(true);
  });

  test("verify FAILS when context-engines.json is missing after the codemap exists", async () => {
    const ctx = makeTestCtx(dir);
    await contextMgmtModule.apply(ctx);
    await Bun.file(join(dir, CONTEXT_ENGINES_PATH)).unlink();
    const verdict = await contextMgmtModule.verify(ctx);
    expect(verdict.ok).toBe(false);
    expect(verdict.findings.some((f) => f.message.includes(CONTEXT_ENGINES_PATH))).toBe(true);
  });

  test("plan lists the engines policy and reflects the Brain opt-in in its description", async () => {
    const off = await contextMgmtModule.plan(makeTestCtx(dir));
    expect(off.some((a) => a.path === CONTEXT_ENGINES_PATH && !a.description.includes("Personal Brain"))).toBe(true);
    const onCtx = makeTestCtx(dir, {
      config: testConfig({ modules: { context: { options: { enableBrain: true } } } }),
    });
    const on = await contextMgmtModule.plan(onCtx);
    expect(on.some((a) => a.path === CONTEXT_ENGINES_PATH && a.description.includes("Personal Brain"))).toBe(true);
  });

  test("instruction block names all four sources including Personal Brain and the no-secrets rule", () => {
    const block = contextMgmtModule.instructionBlocks[0];
    expect(block?.content).toContain("OpenWiki codebase wiki");
    expect(block?.content).toContain("CocoIndex semantic search");
    expect(block?.content).toContain("Serena semantic retrieval");
    expect(block?.content).toContain("OpenWiki Personal Brain");
    expect(block?.content).toContain(CODEMAP_PATH);
    expect(block?.content.toLowerCase()).toContain("never write secrets");
  });
});

describe("context-mgmt: Serena integration (third engine + opt-in MCP registration)", () => {
  let dir: string;
  beforeEach(async () => {
    dir = await makeTempDir();
  });
  afterEach(async () => {
    await removeDir(dir);
  });

  const serenaMcpConfig = (enableSerenaMcp: unknown, harnesses?: string[]) =>
    testConfig({
      harnesses,
      modules: { context: { options: { enableSerenaMcp } } },
    });

  test("Serena absent → engine disabled with install guidance carried in policy and detect", async () => {
    const ctx = makeTestCtx(dir); // no tools present
    expect(serenaState(ctx)).toEqual({ present: false });
    const policy = buildEnginesPolicy(ctx);
    expect(policy.engines.semanticRetrieval.enabled).toBe(false);
    expect(policy.engines.semanticRetrieval.install).toBe(SERENA_INSTALL);
    expect(policy.engines.semanticRetrieval.mcpOptIn).toBe(false);
    expect(policy.engines.semanticRetrieval.mcpEnabled).toBe(false);
    const detected = await contextMgmtModule.detect(ctx);
    expect(detected.some((f) => f.level === "degraded" && f.remediation === SERENA_INSTALL)).toBe(true);
    expect(detected.some((f) => f.remediation === SERENA_MCP_ACTIVATION)).toBe(true);
  });

  test("Serena present → semantic retrieval enabled with version; verify ok after apply", async () => {
    const ctx = makeTestCtx(dir, { presentTools: { serena: "Serena 1.6.2" } });
    expect(serenaState(ctx)).toEqual({ present: true, version: "Serena 1.6.2" });
    const policy = buildEnginesPolicy(ctx);
    expect(policy.engines.semanticRetrieval.enabled).toBe(true);
    expect(policy.engines.semanticRetrieval.version).toBe("Serena 1.6.2");
    await contextMgmtModule.apply(ctx);
    expect((await contextMgmtModule.verify(ctx)).ok).toBe(true);
    const detected = await contextMgmtModule.detect(ctx);
    expect(detected.some((f) => f.level === "ok" && f.message.startsWith("Serena semantic retrieval wired"))).toBe(true);
  });

  test("default apply NEVER touches .mcp.json — Serena MCP registration is strictly opt-in", async () => {
    const result = await contextMgmtModule.apply(makeTestCtx(dir, { presentTools: { serena: "Serena 1.6.2" } }));
    expect(result.status).toBe("applied");
    expect(await Bun.file(join(dir, ".mcp.json")).exists()).toBe(false);
    expect(result.wrotePaths).not.toContain(".mcp.json");
  });

  test("enableSerenaMcp = true + claude-code harness → apply registers the serena MCP server (argv array, project dir)", async () => {
    const ctx = makeTestCtx(dir, { config: serenaMcpConfig(true) });
    expect(serenaMcpOptIn(ctx)).toBe(true);
    expect(serenaMcpEnabled(ctx)).toBe(true);
    const result = await contextMgmtModule.apply(ctx);
    expect(result.status).toBe("applied");
    expect(result.wrotePaths).toContain(".mcp.json");
    const mcp = JSON.parse(await Bun.file(join(dir, ".mcp.json")).text());
    const server = mcp.mcpServers[SERENA_MCP_SERVER_NAME];
    expect(server.command).toBe("serena");
    expect(server.args).toEqual(["start-mcp-server", "--context", "ide-assistant", "--project", dir]);
    expect(server.env).toEqual({});
    // Verify stays green with the opt-in recorded in the engines policy.
    expect(buildEnginesPolicy(ctx).engines.semanticRetrieval.mcpEnabled).toBe(true);
    expect((await contextMgmtModule.verify(ctx)).ok).toBe(true);
  });

  test("enableSerenaMcp without the claude-code harness → .mcp.json untouched", async () => {
    const ctx = makeTestCtx(dir, { config: serenaMcpConfig(true, ["codex"]) });
    expect(serenaMcpOptIn(ctx)).toBe(true);
    expect(serenaMcpEnabled(ctx)).toBe(false);
    await contextMgmtModule.apply(ctx);
    expect(await Bun.file(join(dir, ".mcp.json")).exists()).toBe(false);
    const detected = await contextMgmtModule.detect(ctx);
    expect(detected.some((f) => f.message.includes("opted-in but claude-code harness not targeted"))).toBe(true);
  });

  test("enableSerenaMcp = false behaves exactly like the default (opt-in info finding, no merge)", async () => {
    const result = await contextMgmtModule.apply(makeTestCtx(dir, { config: serenaMcpConfig(false) }));
    expect(await Bun.file(join(dir, ".mcp.json")).exists()).toBe(false);
    const info = result.findings.find((f) => f.level === "info" && f.message.includes("Serena MCP registration off"));
    expect(info).toBeDefined();
    expect(info!.remediation).toBe(SERENA_MCP_ACTIVATION);
  });

  test("pre-existing user serena entry in .mcp.json is preserved untouched", async () => {
    const userServer = { command: "/usr/local/bin/my-serena", args: ["--custom"], env: { PORT: "9999" } };
    await Bun.write(join(dir, ".mcp.json"), JSON.stringify({ mcpServers: { serena: userServer } }, null, 2));
    const result = await contextMgmtModule.apply(makeTestCtx(dir, { config: serenaMcpConfig(true) }));
    expect(result.status).toBe("applied");
    const mcp = JSON.parse(await Bun.file(join(dir, ".mcp.json")).text());
    expect(mcp.mcpServers.serena).toEqual(userServer);
    expect(result.findings.some((f) => f.level === "info" && f.message.includes("preserved"))).toBe(true);
  });

  test("invalid user .mcp.json → apply degrades with fix remediation, file not clobbered", async () => {
    const broken = "{ not json !!!";
    await Bun.write(join(dir, ".mcp.json"), broken);
    const result = await contextMgmtModule.apply(makeTestCtx(dir, { config: serenaMcpConfig(true) }));
    expect(result.status).toBe("degraded");
    expect(result.findings.some((f) => f.level === "error" && f.message.includes(".mcp.json"))).toBe(true);
    expect(await Bun.file(join(dir, ".mcp.json")).text()).toBe(broken);
  });

  test("verify FAILS when the recorded Serena MCP opt-in drifts from the live config", async () => {
    const applyCtx = makeTestCtx(dir); // opt-in off at apply time
    await contextMgmtModule.apply(applyCtx);
    const driftedCtx = makeTestCtx(dir, { config: serenaMcpConfig(true) });
    const verdict = await contextMgmtModule.verify(driftedCtx);
    expect(verdict.ok).toBe(false);
    expect(verdict.findings.some((f) => f.level === "error" && f.remediation?.includes("re-derive"))).toBe(true);
  });

  test("plan reflects the MCP opt-in: merge action when on, skip info when off — never writes", async () => {
    const off = await contextMgmtModule.plan(makeTestCtx(dir));
    expect(off.some((a) => a.kind === "info" && a.description.includes("enableSerenaMcp"))).toBe(true);
    expect(off.some((a) => a.path === ".mcp.json")).toBe(false);
    const on = await contextMgmtModule.plan(makeTestCtx(dir, { config: serenaMcpConfig(true) }));
    expect(on.some((a) => a.kind === "merge" && a.path === ".mcp.json" && a.description.includes(SERENA_MCP_SERVER_NAME))).toBe(
      true,
    );
    expect(await Bun.file(join(dir, ".mcp.json")).exists()).toBe(false);
  });
});
