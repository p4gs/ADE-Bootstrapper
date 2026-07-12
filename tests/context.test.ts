import { afterEach, beforeEach, describe, expect, test } from "bun:test";
import { mkdir } from "node:fs/promises";
import { join } from "node:path";
import {
  buildCtx,
  detectMachineHarnesses,
  detectRepoHarnesses,
  detectTool,
  detectTools,
  isGitRepo,
  parseVersionLine,
} from "../src/context.ts";
import { realExec, realWhich } from "../src/exec.ts";
import { defaultConfig } from "../src/config.ts";
import { ALL_MODULE_IDS, fakeExec, makeTempDir, removeDir } from "./helpers.ts";

let dir: string;

beforeEach(async () => {
  dir = await makeTempDir();
});

afterEach(async () => {
  await removeDir(dir);
});

describe("environment detection", () => {
  test("parseVersionLine extracts the first non-empty line", () => {
    expect(parseVersionLine("trufflehog 3.90.0\nextra")).toBe("trufflehog 3.90.0");
    expect(parseVersionLine("\n\nv2\n")).toBe("v2");
    expect(parseVersionLine("")).toBeUndefined();
  });

  test("detectTool: present with version, present with failing --version, absent", async () => {
    const present = await detectTool(
      "rtk",
      () => "/fake/bin/rtk",
      fakeExec({ "/fake/bin/rtk --version": { code: 0, stdout: "rtk 1.0.0\n" } }),
    );
    expect(present.present).toBe(true);
    expect(present.version).toBe("rtk 1.0.0");

    const broken = await detectTool("rtk", () => "/fake/bin/rtk", fakeExec());
    expect(broken.present).toBe(true);
    expect(broken.version).toBeUndefined();

    const absent = await detectTool("rtk", () => null, fakeExec());
    expect(absent.present).toBe(false);
  });

  test("ISC-18-shape: detectTools covers all seven integrated tools", async () => {
    const tools = await detectTools(() => null, fakeExec());
    expect(Object.keys(tools).sort()).toEqual(
      ["gitleaks", "nono", "ocean", "osv-scanner", "pre-commit", "rtk", "trufflehog"].sort(),
    );
    expect(Object.values(tools).every((tool) => !tool.present)).toBe(true);
  });

  test("ISC-49: repo harness detection by config signals", async () => {
    await Bun.write(join(dir, "CLAUDE.md"), "x");
    await mkdir(join(dir, ".cursor", "rules"), { recursive: true });
    await Bun.write(join(dir, ".cursor", "rules", "user.mdc"), "x");
    const harnesses = await detectRepoHarnesses(dir);
    expect(harnesses).toContain("claude-code");
    expect(harnesses).toContain("cursor");
    expect(harnesses).not.toContain("opencode");
  });

  test("ISC-49: machine harness detection by CLI presence", () => {
    const found = detectMachineHarnesses((name) => (name === "claude" ? "/bin/claude" : null));
    expect(found).toEqual(["claude-code"]);
    expect(detectMachineHarnesses(() => null)).toEqual([]);
  });

  test("isGitRepo true/false paths", async () => {
    expect(await isGitRepo(dir, fakeExec({ "git -C": { code: 0, stdout: "true\n" } }))).toBe(true);
    expect(await isGitRepo(dir, fakeExec({ "git -C": { code: 128, stderr: "fatal" } }))).toBe(false);
  });

  test("buildCtx assembles a complete context", async () => {
    const ctx = await buildCtx({
      targetDir: dir,
      config: defaultConfig([...ALL_MODULE_IDS], ["codex"]),
      exec: fakeExec({ "git -C": { code: 0, stdout: "true\n" } }),
      which: () => null,
    });
    expect(ctx.adeDir).toBe(join(dir, ".ade"));
    expect(ctx.isGitRepo).toBe(true);
    expect(Object.keys(ctx.tools).length).toBe(7);
    expect(ctx.config.harnesses).toEqual(["codex"]);
    ctx.log("no-op logger works");
  });
});

describe("real executor", () => {
  test("realExec runs argv arrays and captures output", async () => {
    const result = await realExec(["echo", "ade-test"]);
    expect(result.code).toBe(0);
    expect(result.stdout.trim()).toBe("ade-test");
  });

  test("realExec: empty argv and missing binary return 127, never throw", async () => {
    expect((await realExec([])).code).toBe(127);
    expect((await realExec(["definitely-not-a-real-binary-xyzzy"])).code).toBe(127);
  });

  test("realWhich finds git, misses nonsense", () => {
    expect(realWhich("git")).not.toBeNull();
    expect(realWhich("definitely-not-a-real-binary-xyzzy")).toBeNull();
  });
});
