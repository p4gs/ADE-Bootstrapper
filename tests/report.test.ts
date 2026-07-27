/**
 * Shared report module (ISC-174) — doctor/status one code path for CLI + GUI.
 */
import { afterEach, beforeEach, describe, expect, test } from "bun:test";
import { mkdir } from "node:fs/promises";
import { join } from "node:path";
import { main } from "../src/cli.ts";
import { doctorReport, statusReport } from "../src/report.ts";
import { INTEGRATED_TOOLS } from "../src/types.ts";
import { fakeExec, makeTempDir, makeTestCtx, removeDir } from "./helpers.ts";

let dir: string;

beforeEach(async () => {
  dir = await makeTempDir();
  await mkdir(join(dir, ".git", "hooks"), { recursive: true });
  await Bun.write(join(dir, "package.json"), '{"name":"fixture"}\n');
});

afterEach(async () => {
  await removeDir(dir);
});

describe("doctorReport", () => {
  test("reports every integrated tool and the 7 supported harnesses", async () => {
    const report = await doctorReport(
      dir,
      (name) => (name === "rtk" ? "/fake/bin/rtk" : null),
      fakeExec({ "git -C": { code: 0, stdout: "true\n" }, "/fake/bin/rtk": { code: 0, stdout: "rtk 9.9.9\n" } }),
    );
    expect(report.targetDir).toBe(dir);
    expect(report.gitRepo).toBe(true);
    expect(report.tools.map((tool) => tool.name)).toEqual([...INTEGRATED_TOOLS]);
    const rtk = report.tools.find((tool) => tool.name === "rtk");
    expect(rtk?.present).toBe(true);
    expect(rtk?.version).toBe("rtk 9.9.9");
    expect(report.harnesses.supported.length).toBe(7);
    expect(report.harnesses.machine).toEqual([]);
  });
});

describe("statusReport", () => {
  test("retains findings for GUI consumers while CLI strips them", async () => {
    const io = { outText: "", errText: "" };
    const code = await main(["init"], {
      io: { out: (t) => (io.outText += t), err: (t) => (io.errText += t) },
      cwd: dir,
      exec: fakeExec({ "git -C": { code: 0, stdout: "true\n" } }),
      which: () => null,
    });
    expect(code).toBe(0);

    const ctx = makeTestCtx(dir);
    const rows = await statusReport(ctx);
    expect(rows.length).toBe(15);
    for (const row of rows) {
      expect(typeof row.id).toBe("string");
      expect(Array.isArray(row.findings)).toBe(true);
    }
    const anyFindings = rows.some((row) => row.findings.length > 0);
    expect(anyFindings).toBe(true);

    // CLI status --json must NOT include findings (v0.1 byte-shape).
    const captured = { outText: "" };
    const statusCode = await main(["status", "--json"], {
      io: { out: (t) => (captured.outText += t), err: () => {} },
      cwd: dir,
      exec: fakeExec({ "git -C": { code: 0, stdout: "true\n" } }),
      which: () => null,
    });
    expect(statusCode).toBe(0);
    const parsed = JSON.parse(captured.outText);
    expect(parsed.modules.length).toBe(15);
    for (const row of parsed.modules) {
      expect(Object.keys(row).sort()).toEqual(["enabled", "id", "state", "title"]);
    }
  });

  test("disabled module reports state disabled with no verify call", async () => {
    const ctx = makeTestCtx(dir, {
      config: (await import("./helpers.ts")).testConfig({ modules: { sandbox: { enabled: false } } }),
    });
    const rows = await statusReport(ctx);
    const sandbox = rows.find((row) => row.id === "sandbox");
    expect(sandbox?.state).toBe("disabled");
    expect(sandbox?.findings).toEqual([]);
  });
});
