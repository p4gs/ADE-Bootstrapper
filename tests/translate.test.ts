import { afterEach, beforeEach, describe, expect, test } from "bun:test";
import { join } from "node:path";
import { checkTranslationDrift, translateAll, translationTargets } from "../src/translate.ts";
import { composeInstructions } from "../src/instructions.ts";
import { makeTempDir, makeTestCtx, removeDir } from "./helpers.ts";

let dir: string;

beforeEach(async () => {
  dir = await makeTempDir();
});

afterEach(async () => {
  await removeDir(dir);
});

const BODY = composeInstructions([
  { id: "t", title: "Test Block", content: "Do the test thing." },
]);

describe("translation", () => {
  test("ISC-47-adjacent: targets dedupe AGENTS.md across the open-standard harnesses", () => {
    const targets = translationTargets(["codex", "opencode", "hermes", "pi", "antigravity"]);
    expect(targets.length).toBe(1);
    expect(targets[0]!.path).toBe("AGENTS.md");
    expect(targets[0]!.harnesses.sort()).toEqual(["antigravity", "codex", "hermes", "opencode", "pi"]);
  });

  test("cursor targets modern .cursor/rules path with frontmatter prefix", () => {
    const targets = translationTargets(["cursor"]);
    expect(targets[0]!.path).toBe(".cursor/rules/ade.mdc");
    expect(targets[0]!.freshFilePrefix).toContain("alwaysApply: true");
  });

  test("ISC-54: translateAll writes managed blocks into every target", async () => {
    const ctx = makeTestCtx(dir, { config: { ...makeTestCtx(dir).config, harnesses: ["claude-code", "codex", "cursor"] } });
    const results = await translateAll(ctx, BODY);
    expect(results.length).toBe(3);
    expect(results.every((result) => result.ok)).toBe(true);
    const cursorFile = await Bun.file(join(dir, ".cursor", "rules", "ade.mdc")).text();
    expect(cursorFile.startsWith("---\n")).toBe(true);
    expect(cursorFile).toContain("Do the test thing.");
    const claude = await Bun.file(join(dir, "CLAUDE.md")).text();
    expect(claude).toContain("<!-- ade:begin -->");
  });

  test("ISC-55: pre-existing user content survives translation byte-for-byte", async () => {
    await Bun.write(join(dir, "CLAUDE.md"), "# Mine\n\nkeep me.\n");
    const ctx = makeTestCtx(dir, { config: { ...makeTestCtx(dir).config, harnesses: ["claude-code"] } });
    await translateAll(ctx, BODY);
    const content = await Bun.file(join(dir, "CLAUDE.md")).text();
    expect(content.startsWith("# Mine\n\nkeep me.\n")).toBe(true);
  });

  test("ISC-56: repeat translate with unchanged canonical body is byte-identical", async () => {
    const ctx = makeTestCtx(dir, { config: { ...makeTestCtx(dir).config, harnesses: ["claude-code"] } });
    await translateAll(ctx, BODY);
    const first = await Bun.file(join(dir, "CLAUDE.md")).text();
    const again = await translateAll(makeTestCtx(dir, { config: { ...makeTestCtx(dir).config, harnesses: ["claude-code"] } }), BODY);
    expect(again[0]!.changed).toBe(false);
    expect(await Bun.file(join(dir, "CLAUDE.md")).text()).toBe(first);
  });

  test("ISC-55.1/55.2: corrupt markers and hand-edited blocks are refused per-file", async () => {
    const ctx = makeTestCtx(dir, { config: { ...makeTestCtx(dir).config, harnesses: ["claude-code", "codex"] } });
    await translateAll(ctx, BODY);
    const claudePath = join(dir, "CLAUDE.md");
    const tampered = (await Bun.file(claudePath).text()).replace("Do the test thing.", "corrupted by user");
    await Bun.write(claudePath, tampered);
    const results = await translateAll(makeTestCtx(dir, { config: { ...ctx.config } }), BODY);
    const claudeResult = results.find((result) => result.path === "CLAUDE.md")!;
    const agentsResult = results.find((result) => result.path === "AGENTS.md")!;
    expect(claudeResult.ok).toBe(false);
    expect(claudeResult.error).toContain("hand-edited");
    expect(agentsResult.ok).toBe(true);
    expect(await Bun.file(claudePath).text()).toBe(tampered);
  });

  test("ISC-57: drift check flags stale and missing managed blocks", async () => {
    const ctx = makeTestCtx(dir, { config: { ...makeTestCtx(dir).config, harnesses: ["claude-code", "codex"] } });
    await translateAll(ctx, BODY);
    let findings = await checkTranslationDrift(ctx, BODY);
    expect(findings.every((finding) => finding.level === "ok")).toBe(true);

    const staleBody = composeInstructions([{ id: "t", title: "Test Block", content: "OLD content." }]);
    findings = await checkTranslationDrift(ctx, staleBody);
    expect(findings.filter((finding) => finding.level === "error").length).toBe(2);

    await removeDir(join(dir, "AGENTS.md"));
    findings = await checkTranslationDrift(ctx, BODY);
    expect(findings.some((finding) => finding.message.includes("missing: AGENTS.md") || finding.message.includes("instruction file missing"))).toBe(true);
  });
});
