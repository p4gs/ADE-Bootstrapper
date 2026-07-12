/**
 * Unified project configuration — `ade.json`.
 * Zero-dep validation lives here ONCE (core-owned); modules never re-validate.
 */
import { join } from "node:path";
import { readIfExists, stableStringify } from "./fsutil.ts";
import { ADE_SCHEMA_VERSION } from "./version.ts";
import type { AdeConfig, ModuleConfig } from "./types.ts";

export const CONFIG_FILE = "ade.json";

export interface ConfigError {
  ok: false;
  error: string;
}

export interface ConfigOk {
  ok: true;
  config: AdeConfig;
}

export type ConfigResult = ConfigOk | ConfigError;

/** Build the default (secure-by-default) config: every module enabled. */
export function defaultConfig(moduleIds: string[], harnesses: string[]): AdeConfig {
  const modules: Record<string, ModuleConfig> = {};
  for (const id of moduleIds) {
    modules[id] = { enabled: true, options: {} };
  }
  return { schemaVersion: ADE_SCHEMA_VERSION, harnesses, modules };
}

/** Serialize a config deterministically. */
export function serializeConfig(config: AdeConfig): string {
  return stableStringify(config);
}

/** Validate a parsed object as AdeConfig against the known module ids. */
export function validateConfig(raw: unknown, knownModuleIds: string[], knownHarnessIds: string[], fileLabel: string): ConfigResult {
  if (raw === null || typeof raw !== "object" || Array.isArray(raw)) {
    return { ok: false, error: `${fileLabel}: config must be a JSON object` };
  }
  const obj = raw as Record<string, unknown>;
  const schemaVersion = obj["schemaVersion"];
  if (typeof schemaVersion !== "number" || !Number.isInteger(schemaVersion) || schemaVersion < 1) {
    return { ok: false, error: `${fileLabel}: schemaVersion must be a positive integer` };
  }
  if (schemaVersion > ADE_SCHEMA_VERSION) {
    return {
      ok: false,
      error: `${fileLabel}: schemaVersion ${schemaVersion} is newer than this ade understands (${ADE_SCHEMA_VERSION}) — upgrade ade-bootstrapper to work with this repository`,
    };
  }
  const harnesses = obj["harnesses"];
  if (!Array.isArray(harnesses) || !harnesses.every((entry) => typeof entry === "string")) {
    return { ok: false, error: `${fileLabel}: harnesses must be an array of strings` };
  }
  for (const harness of harnesses as string[]) {
    if (!knownHarnessIds.includes(harness)) {
      return { ok: false, error: `${fileLabel}: unknown harness id "${harness}" (known: ${knownHarnessIds.join(", ")})` };
    }
  }
  const modules = obj["modules"];
  if (modules === null || typeof modules !== "object" || Array.isArray(modules)) {
    return { ok: false, error: `${fileLabel}: modules must be an object` };
  }
  const parsedModules: Record<string, ModuleConfig> = {};
  for (const [id, value] of Object.entries(modules as Record<string, unknown>)) {
    if (!knownModuleIds.includes(id)) {
      return { ok: false, error: `${fileLabel}: unknown module id "${id}" (known: ${knownModuleIds.join(", ")})` };
    }
    if (value === null || typeof value !== "object" || Array.isArray(value)) {
      return { ok: false, error: `${fileLabel}: modules.${id} must be an object` };
    }
    const moduleValue = value as Record<string, unknown>;
    const enabled = moduleValue["enabled"];
    if (typeof enabled !== "boolean") {
      return { ok: false, error: `${fileLabel}: modules.${id}.enabled must be a boolean` };
    }
    const options = moduleValue["options"] ?? {};
    if (options === null || typeof options !== "object" || Array.isArray(options)) {
      return { ok: false, error: `${fileLabel}: modules.${id}.options must be an object` };
    }
    parsedModules[id] = { enabled, options: options as Record<string, unknown> };
  }
  return {
    ok: true,
    config: { schemaVersion, harnesses: harnesses as string[], modules: parsedModules },
  };
}

/** Load and validate `<dir>/ade.json`. */
export async function loadConfig(
  targetDir: string,
  knownModuleIds: string[],
  knownHarnessIds: string[],
): Promise<ConfigResult> {
  const path = join(targetDir, CONFIG_FILE);
  const text = await readIfExists(path);
  if (text === null) {
    return { ok: false, error: `${CONFIG_FILE} not found in ${targetDir} — run \`ade init\` first` };
  }
  let raw: unknown;
  try {
    raw = JSON.parse(text);
  } catch (error) {
    return { ok: false, error: `${CONFIG_FILE} is not valid JSON: ${String(error)}` };
  }
  return validateConfig(raw, knownModuleIds, knownHarnessIds, CONFIG_FILE);
}
