/** Single source of the tool version and the config/lockfile schema version. */
export const ADE_VERSION = "0.1.0";

/**
 * Schema version for ade.json and ade.lock.json.
 * Loaders MUST reject files with a schemaVersion greater than this with
 * upgrade guidance (mixed-version team safety).
 */
export const ADE_SCHEMA_VERSION = 1;

/** Managed-block markers for user-owned files (CLAUDE.md, AGENTS.md, …). */
export const MARKER_BEGIN = "<!-- ade:begin -->";
export const MARKER_END = "<!-- ade:end -->";

/** Fixed genesis value anchoring the audit hash chain. */
export const AUDIT_GENESIS = "ade-genesis-v1";
