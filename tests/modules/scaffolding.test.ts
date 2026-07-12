import { afterEach, beforeEach, describe, expect, test } from "bun:test";
import { join } from "node:path";
import {
  scaffoldingModule,
  validateScaffoldingOptions,
  PR_CHECKLIST_PATH,
  TESTING_CONVENTIONS_PATH,
  COMMIT_CONVENTIONS_PATH,
  TEMPLATE_HEADERS,
} from "../../src/modules/scaffolding.ts";
import { makeTempDir, makeTestCtx, removeDir, testConfig } from "../helpers.ts";
import { sha256 } from "../../src/fsutil.ts";

const ALL_TEMPLATE_PATHS = [PR_CHECKLIST_PATH, TESTING_CONVENTIONS_PATH, COMMIT_CONVENTIONS_PATH];

let dir: string;

beforeEach(async () => {
  dir = await makeTempDir();
});

afterEach(async () => {
  await removeDir(dir);
});

describe("scaffolding module", () => {
  test("ISC-76: apply writes all three templates with real content, 15-30 lines each", async () => {
    const ctx = makeTestCtx(dir);
    const result = await scaffoldingModule.apply(ctx);
    expect(result.status).toBe("applied");
    for (const relPath of ALL_TEMPLATE_PATHS) {
      expect(result.wrotePaths).toContain(relPath);
      const text = await Bun.file(join(dir, relPath)).text();
      const lineCount = text.trimEnd().split("\n").length;
      expect(lineCount).toBeGreaterThanOrEqual(15);
      expect(lineCount).toBeLessThanOrEqual(30);
      expect(text).toContain(TEMPLATE_HEADERS[relPath]!);
    }
  });

  test("ISC-76: pr-checklist covers tests, coverage floor, security review, secrets, docs, suppression", async () => {
    await scaffoldingModule.apply(makeTestCtx(dir));
    const checklist = await Bun.file(join(dir, PR_CHECKLIST_PATH)).text();
    expect(checklist).toContain("Tests added");
    expect(checklist).toContain("test suite passes");
    expect(checklist).toContain("Coverage floor met");
    expect(checklist.toLowerCase()).toContain("security review");
    expect(checklist).toContain("No secrets");
    expect(checklist).toContain("Docs updated");
    expect(checklist).toContain("suppressed");
  });

  test("ISC-76: testing-conventions mandate test-first and forbid coverage padding", async () => {
    await scaffoldingModule.apply(makeTestCtx(dir));
    const conventions = await Bun.file(join(dir, TESTING_CONVENTIONS_PATH)).text();
    expect(conventions).toContain("BEFORE the behavior change");
    expect(conventions).toContain("Coverage padding is a defect");
    expect(conventions).toContain("intended use case");
    expect(conventions.toLowerCase()).toContain("integration over mocks");
  });

  test("ISC-76: commit-conventions specify conventional types, imperative ≤72-char subject, WHY body, atomic commits, no secrets", async () => {
    await scaffoldingModule.apply(makeTestCtx(dir));
    const conventions = await Bun.file(join(dir, COMMIT_CONVENTIONS_PATH)).text();
    expect(conventions).toContain("type(scope): subject");
    expect(conventions.toLowerCase()).toContain("imperative");
    expect(conventions).toContain("72 characters");
    expect(conventions).toContain("WHY");
    expect(conventions).toContain("One logical change per commit");
    expect(conventions).toContain("Secrets");
  });

  test("ISC-77: instruction block carries test-first, hard coverage gate, never-suppress, and .ade/templates/ pointers", () => {
    expect(scaffoldingModule.instructionBlocks.length).toBeGreaterThan(0);
    const block = scaffoldingModule.instructionBlocks[0]!;
    expect(block.content).toContain("Test-first");
    expect(block.content).toContain("HARD gate");
    expect(block.content.toLowerCase()).toContain("never suppress");
    expect(block.content).toContain(".ade/templates/pr-checklist.md");
    expect(block.content).toContain(".ade/templates/commit-conventions.md");
  });

  test("ISC-116: plan writes nothing to disk", async () => {
    const ctx = makeTestCtx(dir);
    const actions = await scaffoldingModule.plan(ctx);
    expect(actions.length).toBe(3);
    for (const relPath of ALL_TEMPLATE_PATHS) {
      expect(await Bun.file(join(dir, relPath)).exists()).toBe(false);
    }
  });

  test("ISC-117: apply is idempotent — second run byte-identical for all templates", async () => {
    await scaffoldingModule.apply(makeTestCtx(dir));
    const first: Record<string, string> = {};
    for (const relPath of ALL_TEMPLATE_PATHS) {
      first[relPath] = sha256(await Bun.file(join(dir, relPath)).text());
    }
    await scaffoldingModule.apply(makeTestCtx(dir));
    for (const relPath of ALL_TEMPLATE_PATHS) {
      expect(sha256(await Bun.file(join(dir, relPath)).text())).toBe(first[relPath]!);
    }
  });

  test("coverageFloorPct option is rendered into checklist and testing conventions", async () => {
    const config = testConfig({
      modules: { scaffolding: { enabled: true, options: { coverageFloorPct: 80 } } },
    });
    await scaffoldingModule.apply(makeTestCtx(dir, { config }));
    expect(await Bun.file(join(dir, PR_CHECKLIST_PATH)).text()).toContain("80%");
    expect(await Bun.file(join(dir, TESTING_CONVENTIONS_PATH)).text()).toContain("80%");
  });

  test("malformed coverageFloorPct rejected — apply fails, nothing written", async () => {
    const config = testConfig({
      modules: { scaffolding: { enabled: true, options: { coverageFloorPct: 150 } } },
    });
    const result = await scaffoldingModule.apply(makeTestCtx(dir, { config }));
    expect(result.status).toBe("failed");
    expect(result.findings.some((finding) => finding.level === "error")).toBe(true);
    for (const relPath of ALL_TEMPLATE_PATHS) {
      expect(await Bun.file(join(dir, relPath)).exists()).toBe(false);
    }
  });

  test("validateScaffoldingOptions catches every malformed floor value", () => {
    expect(validateScaffoldingOptions({})).toEqual([]);
    expect(validateScaffoldingOptions({ coverageFloorPct: 95 })).toEqual([]);
    expect(validateScaffoldingOptions({ coverageFloorPct: 100 })).toEqual([]);
    expect(validateScaffoldingOptions({ coverageFloorPct: 0 }).length).toBe(1);
    expect(validateScaffoldingOptions({ coverageFloorPct: 101 }).length).toBe(1);
    expect(validateScaffoldingOptions({ coverageFloorPct: "high" }).length).toBe(1);
    expect(validateScaffoldingOptions({ coverageFloorPct: Number.NaN }).length).toBe(1);
  });

  test("detect reports option validity", async () => {
    const ok = await scaffoldingModule.detect(makeTestCtx(dir));
    expect(ok.every((finding) => finding.level === "ok")).toBe(true);
    const bad = await scaffoldingModule.detect(
      makeTestCtx(dir, {
        config: testConfig({ modules: { scaffolding: { enabled: true, options: { coverageFloorPct: -1 } } } }),
      }),
    );
    expect(bad.some((finding) => finding.level === "error")).toBe(true);
  });

  test("verify passes after apply — all three templates present with h1 headers", async () => {
    const ctx = makeTestCtx(dir);
    await scaffoldingModule.apply(ctx);
    const result = await scaffoldingModule.verify(ctx);
    expect(result.ok).toBe(true);
    expect(result.findings.filter((finding) => finding.level === "ok").length).toBe(3);
  });

  test("verify fails when a template is missing", async () => {
    const ctx = makeTestCtx(dir);
    await scaffoldingModule.apply(ctx);
    await removeDir(join(dir, ".ade", "templates"));
    const result = await scaffoldingModule.verify(ctx);
    expect(result.ok).toBe(false);
    expect(result.findings.some((finding) => finding.remediation?.includes("ade apply"))).toBe(true);
  });

  test("verify fails on emptied template and on stripped h1 header", async () => {
    const ctx = makeTestCtx(dir);
    await scaffoldingModule.apply(ctx);

    await Bun.write(join(dir, PR_CHECKLIST_PATH), "   \n");
    expect((await scaffoldingModule.verify(ctx)).ok).toBe(false);

    await scaffoldingModule.apply(makeTestCtx(dir));
    const original = await Bun.file(join(dir, TESTING_CONVENTIONS_PATH)).text();
    await Bun.write(
      join(dir, TESTING_CONVENTIONS_PATH),
      original.replace(TEMPLATE_HEADERS[TESTING_CONVENTIONS_PATH]!, "tampered"),
    );
    const result = await scaffoldingModule.verify(ctx);
    expect(result.ok).toBe(false);
    expect(result.findings.some((finding) => finding.message.includes(TESTING_CONVENTIONS_PATH))).toBe(true);
  });
});
