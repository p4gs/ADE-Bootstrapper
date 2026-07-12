import { afterEach, beforeEach, describe, expect, test } from "bun:test";
import { join } from "node:path";
import { defaultConfig, loadConfig, serializeConfig, validateConfig } from "../src/config.ts";
import { ADE_SCHEMA_VERSION } from "../src/version.ts";
import { ALL_MODULE_IDS, makeTempDir, removeDir } from "./helpers.ts";

const MODULE_IDS = [...ALL_MODULE_IDS];
const HARNESS_IDS = ["claude-code", "codex", "cursor", "opencode", "antigravity", "hermes", "pi"];

let dir: string;

beforeEach(async () => {
  dir = await makeTempDir();
});

afterEach(async () => {
  await removeDir(dir);
});

describe("config model", () => {
  test("ISC-32: default config enables every module (secure-by-default)", () => {
    const config = defaultConfig(MODULE_IDS, ["claude-code"]);
    expect(Object.keys(config.modules).length).toBe(MODULE_IDS.length);
    expect(Object.values(config.modules).every((module) => module.enabled)).toBe(true);
    expect(config.schemaVersion).toBe(ADE_SCHEMA_VERSION);
  });

  test("ISC-34: serialization is deterministic", () => {
    const a = serializeConfig(defaultConfig(MODULE_IDS, ["codex", "claude-code"]));
    const b = serializeConfig(defaultConfig(MODULE_IDS, ["codex", "claude-code"]));
    expect(a).toBe(b);
  });

  test("ISC-29: malformed JSON rejected with error naming the file", async () => {
    await Bun.write(join(dir, "ade.json"), "{nope");
    const result = await loadConfig(dir, MODULE_IDS, HARNESS_IDS);
    expect(result.ok).toBe(false);
    if (result.ok) return;
    expect(result.error).toContain("ade.json");
    expect(result.error).toContain("not valid JSON");
  });

  test("missing config names the init remediation", async () => {
    const result = await loadConfig(dir, MODULE_IDS, HARNESS_IDS);
    expect(result.ok).toBe(false);
    if (result.ok) return;
    expect(result.error).toContain("ade init");
  });

  test("ISC-30: unknown module id rejected, naming the offender", () => {
    const raw = { schemaVersion: 1, harnesses: [], modules: { bogus: { enabled: true, options: {} } } };
    const result = validateConfig(raw, MODULE_IDS, HARNESS_IDS, "ade.json");
    expect(result.ok).toBe(false);
    if (result.ok) return;
    expect(result.error).toContain('"bogus"');
  });

  test("unknown harness id rejected", () => {
    const raw = { schemaVersion: 1, harnesses: ["vscode-classic"], modules: {} };
    const result = validateConfig(raw, MODULE_IDS, HARNESS_IDS, "ade.json");
    expect(result.ok).toBe(false);
    if (result.ok) return;
    expect(result.error).toContain("vscode-classic");
  });

  test("ISC-28.1: newer schemaVersion rejected with upgrade guidance", () => {
    const raw = { schemaVersion: ADE_SCHEMA_VERSION + 1, harnesses: [], modules: {} };
    const result = validateConfig(raw, MODULE_IDS, HARNESS_IDS, "ade.json");
    expect(result.ok).toBe(false);
    if (result.ok) return;
    expect(result.error).toContain("upgrade");
  });

  test("ISC-31: enabled=false and options round-trip through load", async () => {
    const config = defaultConfig(MODULE_IDS, ["claude-code"]);
    config.modules["cost-governance"] = { enabled: false, options: { perSessionCostUsd: 3 } };
    await Bun.write(join(dir, "ade.json"), serializeConfig(config));
    const result = await loadConfig(dir, MODULE_IDS, HARNESS_IDS);
    expect(result.ok).toBe(true);
    if (!result.ok) return;
    expect(result.config.modules["cost-governance"]!.enabled).toBe(false);
    expect(result.config.modules["cost-governance"]!.options["perSessionCostUsd"]).toBe(3);
  });

  test("field-shape violations produce precise errors", () => {
    expect(validateConfig([], MODULE_IDS, HARNESS_IDS, "f").ok).toBe(false);
    expect(validateConfig({ schemaVersion: "1", harnesses: [], modules: {} }, MODULE_IDS, HARNESS_IDS, "f").ok).toBe(false);
    expect(validateConfig({ schemaVersion: 1, harnesses: "all", modules: {} }, MODULE_IDS, HARNESS_IDS, "f").ok).toBe(false);
    expect(validateConfig({ schemaVersion: 1, harnesses: [], modules: [] }, MODULE_IDS, HARNESS_IDS, "f").ok).toBe(false);
    expect(
      validateConfig(
        { schemaVersion: 1, harnesses: [], modules: { secrets: { enabled: "yes" } } },
        MODULE_IDS,
        HARNESS_IDS,
        "f",
      ).ok,
    ).toBe(false);
    expect(
      validateConfig(
        { schemaVersion: 1, harnesses: [], modules: { secrets: { enabled: true, options: [] } } },
        MODULE_IDS,
        HARNESS_IDS,
        "f",
      ).ok,
    ).toBe(false);
  });
});
