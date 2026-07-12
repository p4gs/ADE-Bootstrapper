import { afterEach, beforeEach, describe, expect, test } from "bun:test";
import { rm } from "node:fs/promises";
import { join } from "node:path";
import {
  guardrailsModule,
  parseRuleFrontmatter,
  GUARDRAILS_DIR,
  RULE_FILES,
  RULE_IDS,
} from "../../src/modules/guardrails.ts";
import { makeTempDir, makeTestCtx, removeDir } from "../helpers.ts";
import { sha256 } from "../../src/fsutil.ts";

const REQUIRED_FILES = [
  "input-validation.md",
  "injection.md",
  "secrets-handling.md",
  "authn-authz.md",
  "crypto.md",
  "error-handling.md",
];

let dir: string;

beforeEach(async () => {
  dir = await makeTempDir();
});

afterEach(async () => {
  await removeDir(dir);
});

describe("guardrails module", () => {
  test("ISC-59: apply writes at least the 6 required rule files with real DO/DON'T content", async () => {
    const ctx = makeTestCtx(dir);
    const result = await guardrailsModule.apply(ctx);
    expect(result.status).toBe("applied");
    expect(Object.keys(RULE_FILES).length).toBeGreaterThanOrEqual(6);
    for (const fileName of REQUIRED_FILES) {
      const relPath = `${GUARDRAILS_DIR}/${fileName}`;
      expect(result.wrotePaths).toContain(relPath);
      const text = await Bun.file(join(dir, relPath)).text();
      expect(text).toContain("DO:");
      expect(text).toContain("DON'T:");
      // Real content, not placeholders: ~15-40 lines of body after frontmatter.
      const bodyLines = text.split("\n").slice(5).filter((line) => line.trim().length > 0);
      expect(bodyLines.length).toBeGreaterThanOrEqual(15);
      expect(bodyLines.length).toBeLessThanOrEqual(40);
      expect(text.toLowerCase()).not.toContain("lorem");
      expect(text.toLowerCase()).not.toContain("tbd");
      expect(text.toLowerCase()).not.toContain("todo");
    }
  });

  test("ISC-59: injection rule concretely covers SQL, command, path, and XSS", async () => {
    await guardrailsModule.apply(makeTestCtx(dir));
    const text = await Bun.file(join(dir, GUARDRAILS_DIR, "injection.md")).text();
    expect(text).toContain("parameterized");
    expect(text.toLowerCase()).toContain("command");
    expect(text.toLowerCase()).toContain("path");
    expect(text).toContain("XSS");
  });

  test("ISC-62: every rule file starts with parseable frontmatter — kebab id, valid severity, applies_to array", async () => {
    await guardrailsModule.apply(makeTestCtx(dir));
    for (const fileName of Object.keys(RULE_FILES)) {
      const text = await Bun.file(join(dir, GUARDRAILS_DIR, fileName)).text();
      expect(text.startsWith("---\n")).toBe(true);
      const parsed = parseRuleFrontmatter(text);
      expect(parsed).not.toBeNull();
      expect(parsed!.id).toBe(fileName.replace(/\.md$/, ""));
      expect(parsed!.id).toMatch(/^[a-z][a-z0-9-]*$/);
      expect(["critical", "high", "medium"]).toContain(parsed!.severity);
      expect(Array.isArray(parsed!.applies_to)).toBe(true);
      expect(parsed!.applies_to.length).toBeGreaterThan(0);
    }
  });

  test("ISC-62: parseRuleFrontmatter rejects malformed frontmatter", () => {
    expect(parseRuleFrontmatter("# no frontmatter\n")).toBeNull();
    expect(parseRuleFrontmatter("---\nid: x\nseverity: high\n")).toBeNull(); // unterminated
    expect(parseRuleFrontmatter("---\nid: Not-Kebab\nseverity: high\napplies_to: [all]\n---\n")).toBeNull();
    expect(parseRuleFrontmatter("---\nid: x\nseverity: nuclear\napplies_to: [all]\n---\n")).toBeNull();
    expect(parseRuleFrontmatter("---\nid: x\nseverity: high\napplies_to: []\n---\n")).toBeNull();
    expect(parseRuleFrontmatter("---\nid: x\nseverity: high\napplies_to: all\n---\n")).toBeNull(); // not an array
    expect(parseRuleFrontmatter("---\nid: x\nseverity: high\n---\n")).toBeNull(); // missing applies_to
    const ok = parseRuleFrontmatter('---\nid: my-rule\nseverity: medium\napplies_to: ["*.ts", *.py]\n---\nbody\n');
    expect(ok).toEqual({ id: "my-rule", severity: "medium", applies_to: ["*.ts", "*.py"] });
  });

  test("ISC-60: instruction block binds the ruleset, lists the 6 rule ids, and forbids suppression", () => {
    expect(guardrailsModule.instructionBlocks.length).toBeGreaterThan(0);
    const block = guardrailsModule.instructionBlocks[0]!;
    expect(block.content).toContain("BINDING");
    expect(block.content).toContain(".ade/guardrails/");
    for (const id of ["input-validation", "injection", "secrets-handling", "authn-authz", "crypto", "error-handling"]) {
      expect(block.content).toContain(id);
    }
    expect(block.content).toContain("fixed at code level, never suppressed");
  });

  test("ISC-61: detect reports CodeGuard framework status as info (never degraded), CLI absent", async () => {
    const findings = await guardrailsModule.detect(makeTestCtx(dir));
    expect(findings.length).toBeGreaterThanOrEqual(2);
    expect(findings.every((finding) => finding.level === "info")).toBe(true);
    expect(findings.some((finding) => finding.message.includes("cosai-oasis/project-codeguard"))).toBe(true);
    expect(findings.some((finding) => finding.remediation?.includes(GUARDRAILS_DIR))).toBe(true);
    expect(findings.some((finding) => finding.message.includes("no codeguard CLI"))).toBe(true);
  });

  test("ISC-61: detect notices a codeguard CLI when one exists on PATH", async () => {
    const ctx = makeTestCtx(dir, { which: (name) => (name === "codeguard" ? "/fake/bin/codeguard" : null) });
    const findings = await guardrailsModule.detect(ctx);
    expect(findings.some((finding) => finding.message.includes("codeguard CLI found"))).toBe(true);
    expect(findings.every((finding) => finding.level === "info")).toBe(true);
  });

  test("ISC-116: plan lists one write per rule file but writes nothing to disk", async () => {
    const ctx = makeTestCtx(dir);
    const actions = await guardrailsModule.plan(ctx);
    expect(actions.length).toBe(Object.keys(RULE_FILES).length);
    expect(actions.every((action) => action.kind === "write")).toBe(true);
    expect(await Bun.file(join(dir, GUARDRAILS_DIR, "injection.md")).exists()).toBe(false);
    expect(ctx.artifacts.written().length).toBe(0);
  });

  test("ISC-117: apply is idempotent — second run byte-identical across all rule files", async () => {
    await guardrailsModule.apply(makeTestCtx(dir));
    const first: Record<string, string> = {};
    for (const fileName of Object.keys(RULE_FILES)) {
      first[fileName] = sha256(await Bun.file(join(dir, GUARDRAILS_DIR, fileName)).text());
    }
    await guardrailsModule.apply(makeTestCtx(dir));
    for (const fileName of Object.keys(RULE_FILES)) {
      const second = sha256(await Bun.file(join(dir, GUARDRAILS_DIR, fileName)).text());
      expect(second).toBe(first[fileName]!);
    }
  });

  test("determinism: no timestamps or absolute paths embedded in generated rules", async () => {
    const ctx = makeTestCtx(dir);
    await guardrailsModule.apply(ctx);
    for (const fileName of Object.keys(RULE_FILES)) {
      const text = await Bun.file(join(dir, GUARDRAILS_DIR, fileName)).text();
      expect(text).not.toContain(dir);
      expect(text).not.toMatch(/\d{4}-\d{2}-\d{2}T\d{2}:\d{2}/);
    }
  });

  test("verify: passes after apply", async () => {
    const ctx = makeTestCtx(dir);
    await guardrailsModule.apply(ctx);
    const result = await guardrailsModule.verify(ctx);
    expect(result.ok).toBe(true);
    expect(result.findings.every((finding) => finding.level === "ok")).toBe(true);
  });

  test("verify: fails when a rule file is deleted", async () => {
    const ctx = makeTestCtx(dir);
    await guardrailsModule.apply(ctx);
    await rm(join(dir, GUARDRAILS_DIR, "crypto.md"));
    const result = await guardrailsModule.verify(ctx);
    expect(result.ok).toBe(false);
    expect(
      result.findings.some(
        (finding) => finding.level === "error" && finding.message.includes("crypto.md") && finding.remediation?.includes("ade apply"),
      ),
    ).toBe(true);
  });

  test("verify: fails when frontmatter is tampered (invalid severity)", async () => {
    const ctx = makeTestCtx(dir);
    await guardrailsModule.apply(ctx);
    const path = join(dir, GUARDRAILS_DIR, "injection.md");
    const tampered = (await Bun.file(path).text()).replace("severity: critical", "severity: whatever");
    await Bun.write(path, tampered);
    const result = await guardrailsModule.verify(ctx);
    expect(result.ok).toBe(false);
    expect(result.findings.some((finding) => finding.level === "error" && finding.message.includes("injection.md"))).toBe(true);
  });

  test("verify: fails when frontmatter id diverges from the file name", async () => {
    const ctx = makeTestCtx(dir);
    await guardrailsModule.apply(ctx);
    const path = join(dir, GUARDRAILS_DIR, "crypto.md");
    const tampered = (await Bun.file(path).text()).replace("id: crypto", "id: something-else");
    await Bun.write(path, tampered);
    const result = await guardrailsModule.verify(ctx);
    expect(result.ok).toBe(false);
    expect(result.findings.some((finding) => finding.message.includes("does not match"))).toBe(true);
  });

  test("RULE_IDS covers the 6 required rule ids", () => {
    for (const id of ["input-validation", "injection", "secrets-handling", "authn-authz", "crypto", "error-handling"]) {
      expect(RULE_IDS).toContain(id);
    }
  });
});
