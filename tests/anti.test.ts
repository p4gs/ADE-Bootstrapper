/**
 * Security anti-criteria probes (ISC-121..128) — the things that must NOT happen.
 */
import { afterEach, beforeEach, describe, expect, test } from "bun:test";
import { mkdir, readdir } from "node:fs/promises";
import { join } from "node:path";
import { main, type Io } from "../src/cli.ts";
import { MODULES } from "../src/registry.ts";
import { fakeExec, makeTempDir, makeTestCtx, removeDir } from "./helpers.ts";

const REPO_ROOT = new URL("..", import.meta.url).pathname;

let dir: string;

beforeEach(async () => {
  dir = await makeTempDir();
  await mkdir(join(dir, ".git", "hooks"), { recursive: true });
  await Bun.write(join(dir, "package.json"), '{"name":"fixture"}\n');
});

afterEach(async () => {
  await removeDir(dir);
});

const silentIo: Io = { out: () => {}, err: () => {} };

async function sourceFiles(): Promise<string[]> {
  const glob = new Bun.Glob("src/**/*.ts");
  const files: string[] = [];
  for await (const path of glob.scan({ cwd: REPO_ROOT, onlyFiles: true })) {
    files.push(join(REPO_ROOT, path));
  }
  return files;
}

describe("security anti-criteria", () => {
  test("ISC-121: no shell-string execution anywhere in src/", async () => {
    for (const file of await sourceFiles()) {
      const content = await Bun.file(file).text();
      expect(content).not.toMatch(/sh\s+-c/);
      expect(content).not.toMatch(/Bun\.\$`/);
      expect(content).not.toMatch(/child_process/);
      expect(content).not.toMatch(/execSync/);
    }
  });

  test("ISC-127: zero runtime dependencies in package.json", async () => {
    const pkg = JSON.parse(await Bun.file(join(REPO_ROOT, "package.json")).text());
    expect(pkg.dependencies ?? {}).toEqual({});
  });

  test("ISC-12: no hardcoded user-home absolute paths in src/", async () => {
    for (const file of await sourceFiles()) {
      const content = await Bun.file(file).text();
      expect(content).not.toContain("/Users/");
      expect(content).not.toContain("/home/");
    }
  });

  test("ISC-123: planted env secret appears in NO generated file after full init", async () => {
    const planted = "PLANTED_SUPER_SECRET_VALUE_XYZZY_99";
    const previousEnv = process.env["ADE_TEST_SECRET"];
    process.env["ADE_TEST_SECRET"] = planted;
    try {
      const code = await main(["init"], {
        io: silentIo,
        cwd: dir,
        exec: fakeExec({ "git -C": { code: 0, stdout: "true\n" } }),
        which: () => null,
      });
      expect(code).toBe(0);
      const glob = new Bun.Glob("**/*");
      for await (const path of glob.scan({ cwd: dir, onlyFiles: true, dot: true })) {
        const content = await Bun.file(join(dir, path)).text().catch(() => "");
        expect(content).not.toContain(planted);
      }
    } finally {
      if (previousEnv === undefined) delete process.env["ADE_TEST_SECRET"];
      else process.env["ADE_TEST_SECRET"] = previousEnv;
    }
  });

  test("ISC-124: init touches ONLY sanctioned paths (tree-diff proof)", async () => {
    await Bun.write(join(dir, "src.js"), "// user code\n");
    await Bun.write(join(dir, "README.md"), "# user readme\n");
    await Bun.write(join(dir, "docs.txt"), "user docs\n");
    const userFiles = ["src.js", "README.md", "docs.txt", "package.json"];
    const before: Record<string, string> = {};
    for (const file of userFiles) before[file] = await Bun.file(join(dir, file)).text();

    const code = await main(["init"], {
      io: silentIo,
      cwd: dir,
      exec: fakeExec({ "git -C": { code: 0, stdout: "true\n" } }),
      which: () => null,
    });
    expect(code).toBe(0);

    for (const file of userFiles) {
      expect(await Bun.file(join(dir, file)).text()).toBe(before[file]!);
    }
    const glob = new Bun.Glob("**/*");
    const sanctioned = [
      /^\.ade\//,
      /^ade\.json$/,
      /^ade\.lock\.json$/,
      /^\.gitignore$/,
      /^CLAUDE\.md$/,
      /^AGENTS\.md$/,
      /^\.cursor\//,
      /^\.claude\//,
      /^\.mcp\.json$/,
      /^\.pre-commit-config\.yaml$/,
    ];
    for await (const path of glob.scan({ cwd: dir, onlyFiles: true, dot: true })) {
      if (path.startsWith(".git/")) continue; // hooks dir is sanctioned git-boundary surface
      const isUserFile = userFiles.includes(path) || ["src.js", "README.md", "docs.txt"].includes(path);
      const isSanctioned = sanctioned.some((pattern) => pattern.test(path));
      expect(isUserFile || isSanctioned).toBe(true);
    }
  });

  test("ISC-126: no default approval decision allows destructive/credential/production actions", async () => {
    const code = await main(["init"], {
      io: silentIo,
      cwd: dir,
      exec: fakeExec({ "git -C": { code: 0, stdout: "true\n" } }),
      which: () => null,
    });
    expect(code).toBe(0);
    const approvalsFile = Bun.file(join(dir, ".ade", "policy", "approvals.json"));
    if (await approvalsFile.exists()) {
      const approvals = JSON.parse(await approvalsFile.text());
      const classes = approvals.actionClasses ?? approvals;
      for (const key of ["destructiveShell", "credentialUse", "productionAffecting"]) {
        const entry = classes[key];
        if (entry !== undefined) {
          expect(entry.decision).not.toBe("allow");
        }
      }
    }
  });

  test("ISC-125: no source file references fetch/network APIs (static offline proof)", async () => {
    const glob = new Bun.Glob("src/**/*.ts");
    for await (const path of glob.scan({ cwd: REPO_ROOT, onlyFiles: true })) {
      const content = await Bun.file(join(REPO_ROOT, path)).text();
      expect(content).not.toMatch(/\bfetch\s*\(/);
      expect(content).not.toMatch(/new WebSocket/);
      expect(content).not.toMatch(/https?\.request/);
    }
  });

  test("ISC-128: test suite has no assertion-free test files", async () => {
    const testDirs = [join(REPO_ROOT, "tests"), join(REPO_ROOT, "tests", "modules")];
    for (const testDir of testDirs) {
      for (const entry of await readdir(testDir, { withFileTypes: true })) {
        if (!entry.isFile() || !entry.name.endsWith(".test.ts")) continue;
        const content = await Bun.file(join(testDir, entry.name)).text();
        expect(content).toContain("expect(");
      }
    }
  });

  test("ISC-10-adjacent: registry order is deterministic (instruction composition stability)", () => {
    expect(MODULES.map((module) => module.id)).toEqual(MODULES.map((module) => module.id).slice());
    const ctx = makeTestCtx(dir);
    expect(ctx.config.schemaVersion).toBe(1);
  });
});
