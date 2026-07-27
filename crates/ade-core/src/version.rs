//! Single source of the tool version and the config/lockfile schema version.
//! Port of `src/version.ts` — the schema version, markers, and genesis anchor
//! MUST stay identical to the TS oracle for cross-version compatibility
//! (ISC-162); only ADE_VERSION legitimately differs (allowlisted divergence).

pub const ADE_VERSION: &str = "0.2.0";

/// Schema version for ade.json and ade.lock.json. Loaders MUST reject files
/// with a schemaVersion greater than this with upgrade guidance.
pub const ADE_SCHEMA_VERSION: u64 = 1;

/// Managed-block markers for user-owned files (CLAUDE.md, AGENTS.md, …).
pub const MARKER_BEGIN: &str = "<!-- ade:begin -->";
pub const MARKER_END: &str = "<!-- ade:end -->";

/// Fixed genesis value anchoring the audit hash chain.
pub const AUDIT_GENESIS: &str = "ade-genesis-v1";
