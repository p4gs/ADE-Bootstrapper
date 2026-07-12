import { afterEach, beforeEach, describe, expect, test } from "bun:test";
import { mkdir } from "node:fs/promises";
import { join } from "node:path";
import { main, parseArgs, usage, type Io } from "../src/cli.ts";
import { fakeExec, makeTempDir, removeDir } from "./helpers.ts";
import { ADE_VERSION } from "../src/version.ts";
import { sha256 } from "../src/fsutil.ts";

let dir: string;

interface Captured {
  io: Io;
  stdout: () => string;
  stderr: () => string;
}

function capture(): Captured {
  let out = "";
  let err = "";
  return {
    io: { out: (text) => (out += text), err: (text) => (err += text) },
    stdout: () => out,
    stderr: () => err,
  };
}

function run(argv: string[], captured: Captured): Promise<number> {
  return main(argv, {
    io: captured.io,
    cwd: dir,
    exec: fakeExec({ "git -C": { code: 0, stdout: "true\n" } }),
    which: () => null,
  });
}

beforeEach(async () => {
  dir = await makeTempDir();
  await mkdir(join(dir, ".git", "hooks"), { recursive: true });
  await Bun.write(join(dir, "package.json"), '{"name":"fixture"}\n');
});

afterEach(async () => {
  await removeDir(dir);
});

async function treeSnapshot(root: string): Promise<string> {
  const glob = new Bun.Glob("**/*");
  const entries: string[] = [];
  for await (const path of glob.scan({ cwd: root, onlyFiles: true, dot: true })) {
    const content = await Bun.file(join(root, path)).text().catch(() => "<binary>");
    entries.push(`${path}:${sha256(content)}`);
  }
  return entries.sort().join("\n");
}

describe("ade CLI", () => {
  test("ISC-13: help lists every command and exits 0", async () => {
    const captured = capture();
    expect(await run(["help"], captured)).toBe(0);
    for (const command of ["init", "plan", "apply", "verify", "status", "doctor", "modules", "translate", "lock", "audit", "version"]) {
      expect(captured.stdout()).toContain(`ade ${command}`);
    }
    expect(usage()).toContain("Usage: ade");
  });

  test("ISC-14: version matches package.json", async () => {
    const captured = capture();
    expect(await run(["version"], captured)).toBe(0);
    expect(captured.stdout().trim()).toBe(ADE_VERSION);
    const pkg = JSON.parse(await Bun.file(new URL("../package.json", import.meta.url).pathname).text());
    expect(pkg.version).toBe(ADE_VERSION);
  });

  test("ISC-26/27: unknown command exits 2 with usage on stderr", async () => {
    const captured = capture();
    expect(await run(["frobnicate"], captured)).toBe(2);
    expect(captured.stderr()).toContain("unknown command");
    const flagged = capture();
    expect(await run(["--bogus-flag"], flagged)).toBe(2);
  });

  test("ISC-15: init bootstraps end-to-end (config + modules + lockfile + audit)", async () => {
    const captured = capture();
    expect(await run(["init"], captured)).toBe(0);
    expect(await Bun.file(join(dir, "ade.json")).exists()).toBe(true);
    expect(await Bun.file(join(dir, "ade.lock.json")).exists()).toBe(true);
    expect(await Bun.file(join(dir, ".ade", "instructions.md")).exists()).toBe(true);
    expect(await Bun.file(join(dir, ".ade", "audit", "log.jsonl")).exists()).toBe(true);
    expect(await Bun.file(join(dir, "CLAUDE.md")).exists()).toBe(true);
  });

  test("ISC-15.1: init on a non-git directory completes degraded, exit 0", async () => {
    const bare = await makeTempDir();
    try {
      const captured = capture();
      const code = await main(["init"], {
        io: captured.io,
        cwd: bare,
        exec: fakeExec(),
        which: () => null,
      });
      expect(code).toBe(0);
      expect(await Bun.file(join(bare, "ade.json")).exists()).toBe(true);
    } finally {
      await removeDir(bare);
    }
  });

  test("ISC-16: plan after init writes nothing (tree-hash identical)", async () => {
    await run(["init"], capture());
    const before = await treeSnapshot(dir);
    const captured = capture();
    expect(await run(["plan"], captured)).toBe(0);
    expect(await treeSnapshot(dir)).toBe(before);
    expect(captured.stdout()).toContain("dry-run");
  });

  test("ISC-17: second apply leaves every generated file byte-identical (audit surface excepted)", async () => {
    await run(["init"], capture());
    const before = await treeSnapshot(dir);
    const lockBefore = JSON.parse(await Bun.file(join(dir, "ade.lock.json")).text());
    expect(await run(["apply"], capture())).toBe(0);
    const after = await treeSnapshot(dir);
    const lockAfter = JSON.parse(await Bun.file(join(dir, "ade.lock.json")).text());

    // Everything except the audit log and the lockfile (whose audit checkpoint
    // advances because the second apply was itself logged) is untouched.
    const strip = (snapshot: string): string =>
      snapshot
        .split("\n")
        .filter((line) => !line.startsWith(".ade/audit/") && !line.startsWith("ade.lock.json:"))
        .join("\n");
    expect(strip(after)).toBe(strip(before));
    expect(lockAfter.files).toEqual(lockBefore.files);
    expect(lockAfter.audit.length).toBeGreaterThan(lockBefore.audit.length);
  });

  test("ISC-19: verify passes clean and fails after tamper, exit codes 0/1", async () => {
    await run(["init"], capture());
    expect(await run(["verify"], capture())).toBe(0);
    await Bun.write(join(dir, ".ade", "policy", "budget.json"), "{}\n");
    const captured = capture();
    expect(await run(["verify"], captured)).toBe(1);
  });

  test("ISC-20/21: status and modules list all 15 modules", async () => {
    await run(["init"], capture());
    const status = capture();
    expect(await run(["status"], status)).toBe(0);
    const modules = capture();
    expect(await run(["modules"], modules)).toBe(0);
    for (const id of ["guardrails", "secrets", "token-efficiency", "cost-governance"]) {
      expect(status.stdout()).toContain(id);
      expect(modules.stdout()).toContain(id);
    }
    expect(modules.stdout().split("\n").filter((line) => line.match(/^  (on |off)/)).length).toBe(15);
  });

  test("ISC-22: translate regenerates harness files", async () => {
    await run(["init"], capture());
    await removeDir(join(dir, "CLAUDE.md"));
    expect(await run(["translate"], capture())).toBe(0);
    expect(await Bun.file(join(dir, "CLAUDE.md")).exists()).toBe(true);
  });

  test("ISC-23: lock regenerates the lockfile deterministically", async () => {
    await run(["init"], capture());
    const before = await Bun.file(join(dir, "ade.lock.json")).text();
    expect(await run(["lock"], capture())).toBe(0);
    expect(await Bun.file(join(dir, "ade.lock.json")).text()).toBe(before);
  });

  test("ISC-24: audit verify detects tampering, exit 1", async () => {
    await run(["init"], capture());
    expect(await run(["audit", "verify"], capture())).toBe(0);
    const logPath = join(dir, ".ade", "audit", "log.jsonl");
    const lines = (await Bun.file(logPath).text()).trim().split("\n");
    const entry = JSON.parse(lines[0]!);
    entry.result = "tampered";
    lines[0] = JSON.stringify(entry);
    await Bun.write(logPath, `${lines.join("\n")}\n`);
    const captured = capture();
    expect(await run(["audit", "verify"], captured)).toBe(1);
    expect(await run(["audit", "show"], capture())).toBe(0);
  });

  test("ISC-25/25.1: --json output is pure parseable JSON on stdout", async () => {
    await run(["init"], capture());
    for (const argv of [["doctor", "--json"], ["modules", "--json"], ["status", "--json"], ["verify", "--json"], ["version", "--json"], ["plan", "--json"]]) {
      const captured = capture();
      await run(argv, captured);
      expect(() => JSON.parse(captured.stdout())).not.toThrow();
    }
  });

  test("ISC-135-shape: doctor --json includes a tools array with ≥7 entries", async () => {
    const captured = capture();
    expect(await run(["doctor", "--json"], captured)).toBe(0);
    const payload = JSON.parse(captured.stdout());
    expect(payload.tools.length).toBeGreaterThanOrEqual(7);
    expect(payload.harnesses.supported.length).toBe(7);
  });

  test("parseArgs handles flags, --dir, and errors", () => {
    expect(parseArgs([]).command).toBe("help");
    expect(parseArgs(["init", "--json"]).json).toBe(true);
    expect(parseArgs(["apply", "--dir", "/x"]).dir).toBe("/x");
    expect(parseArgs(["apply", "--dir"]).error).toContain("--dir");
    expect(parseArgs(["-h"]).command).toBe("help");
  });

  test("apply/verify without config exit 1 with init guidance", async () => {
    const captured = capture();
    expect(await run(["apply"], captured)).toBe(1);
    expect(captured.stderr()).toContain("ade init");
    const jsonCaptured = capture();
    expect(await run(["verify", "--json"], jsonCaptured)).toBe(1);
    expect(JSON.parse(jsonCaptured.stdout()).ok).toBe(false);
  });

  test("init on a missing directory exits 1", async () => {
    const captured = capture();
    const code = await main(["init", "/nonexistent/definitely/absent"], {
      io: captured.io,
      cwd: dir,
      exec: fakeExec(),
      which: () => null,
    });
    expect(code).toBe(1);
    expect(captured.stderr()).toContain("does not exist");
  });

  test("audit before init exits 1 with guidance", async () => {
    const captured = capture();
    expect(await run(["audit", "verify"], captured)).toBe(1);
    expect(captured.stderr()).toContain("ade init");
  });
});
