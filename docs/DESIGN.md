# ADE Bootstrapper — Design

This document records the v0.1 architecture. The spec (`ADE Bootstrapper.md`) defines
*what*; this defines *how*. The project ISA (`ISA.md`) is the verifiable system of record.

## Shape

A zero-runtime-dependency Bun/TypeScript CLI (`ade`) that bootstraps, governs, and
verifies an Agentic Development Environment inside a target repository. Everything the
tool produces is a plain, diffable file in the target repo — there is no control plane,
no daemon, no network I/O at bootstrap time.

```
ade init <dir>       # detect environment → write ade.json → apply modules → lockfile
ade plan | apply     # dry-run / idempotent re-apply
ade verify           # re-derive state from disk, compare against the lockfile
ade doctor | status  # environment + module health
ade translate | lock # regenerate instruction files / lockfile
ade audit verify     # validate the tamper-evident audit chain
```

## Core concepts

### Modules (the extension model)

Each spec component is one module implementing the frozen `AdeModule` interface
(`src/types.ts`): `detect → plan → apply → verify`, plus static `instructionBlocks`.

- `plan()` never writes. `apply()` is idempotent. `verify()` re-derives from disk.
- Absent optional tooling degrades a module (precise finding + remediation), never fails it.
- A throwing module is fault-isolated by the pipeline: reported `failed`, run continues.
- Adding a component = one file implementing `AdeModule` + one registry entry.
  There is deliberately NO dynamic plugin loader in v0.1 — a plugin loader is a
  supply-chain attack surface, and the module registry is compiled code.

### Unified config — `ade.json`

Schema-versioned (`schemaVersion`), validated once in core (`src/config.ts`).
Secure-by-default: `ade init` enables every module; disabling is an explicit,
git-visible act. A config written by a NEWER ade is refused with upgrade guidance.

### Lockfile — `ade.lock.json`

Content-addressed record of the **entire** ADE-owned tree (`.ade/**`, excluding the
append-only audit log) plus environment facts and the audit-chain checkpoint.

- **No timestamps.** Identical inputs → byte-identical lockfile (audit checkpoint aside).
- Content tampering (hash mismatch / missing file) **fails** `ade verify`.
- **Unknown files under `.ade/` fail verify.** The lock enumerates the tree, not just
  what the last run wrote — otherwise a rule file planted into `.ade/guardrails/`
  would be BINDING on the harness yet invisible to verification. Adopting a new file
  requires an explicit `ade lock`.
- **Audit checkpoint** (`audit: {length, headHash}`): pins the chain at apply time.
  Because the lockfile is git-committed, this is what makes truncation, tail-drop,
  and re-forging from the public genesis anchor detectable.
- Environment drift (tool versions differ from lock time) is an **informational**
  verdict, never a failure — a teammate on another machine gets facts, not errors.
- User-owned files (CLAUDE.md, `.ade/instructions.local.md`, .gitignore,
  settings.json) are NOT hash-locked; their ADE-managed regions are verified
  structurally instead (see managed blocks).

### Instructions: generated vs. user-owned

`.ade/instructions.md` is **generated** (module blocks, registry order) and is
hash-locked. `.ade/instructions.local.md` is **yours**: created once at init, never
overwritten, never hash-locked, and appended to every harness's managed block under
"Project-Specific Instructions". That split is what lets the baseline be
lock-verified while project rules still survive `ade apply`.

### Managed blocks (co-owning user files)

ADE content in user-owned files lives strictly between `<!-- ade:begin -->` and
`<!-- ade:end -->`. The provenance line embeds a `content-hash` of the block body.

- Content outside markers is never modified — translation preserves it byte-for-byte.
- A hand-edit **inside** the block (hash mismatch) is refused, not clobbered.
- Markers with **no ADE provenance line** are not our block — they are user (or
  foreign-tool) content that happens to use the same strings, and are refused rather
  than replaced. Without this check, a repo whose docs merely *quote* the markers
  would have that content silently destroyed on the first `ade apply`.
- Corrupt marker states (duplicated / unterminated / reversed) refuse the file.
- Non-regular files (symlink / FIFO / socket) at a target path are refused — they are
  frequently intentional secret mounts.

### Canonical instructions → translation

`.ade/instructions.md` is the single source of instruction truth, composed from module
`instructionBlocks` in registry order. `ade translate` renders it into each configured
harness's surface: `CLAUDE.md` (Claude Code), `AGENTS.md` (Codex/OpenCode/Antigravity/
Hermes/Pi — deduped by path), `.cursor/rules/ade.mdc` (Cursor, with `.mdc` frontmatter;
`.cursorrules` is treated as a legacy detection signal only).

### Tamper-evident audit chain

`.ade/audit/log.jsonl` is hash-chained: each entry embeds
`sha256(prev + canonical(entry))`, anchored at a fixed genesis value. Any modification
or deletion of a historical entry breaks every subsequent link (`ade audit verify`).
The Claude Code integration wires a PostToolUse hook (`.ade/hooks/audit-log.ts`,
self-contained) appending tool events to the same chain. The log is machine-local
(git-ignored) by default.

**Threat model notes:**
- Repo-local hook scripts wired into a harness are a tamper target. `ade verify`
  hash-checks them via the lockfile; a modified hook fails verify.
- The chain detects in-place edits, truncation-to-empty, tail-dropping, and a chain
  re-forged from the public genesis anchor — the last three only because the lockfile
  (git-committed) pins the chain's length and head hash. An attacker who rewrites the
  log **and** the committed lockfile in the same change can still tell a consistent
  story; the chain is not signed. External anchoring/signing is a v0.2 item.
- The commonly-documented pre-commit invocation `trufflehog git file://. --since-commit HEAD`
  scans an EMPTY range at commit time (live-probe-verified) — a scanner that finds
  nothing. ADE installs a staged-index scan instead, and both `apply` and `verify`
  flag the broken invocation when adopting a repo that already has it.

### Harness adapters

Declarative capability model per harness (`src/harness/adapters.ts`): instruction file,
detection signals, and capability flags (hooks / mcp / permissions). Claude Code is the
fully-capable adapter in v0.1 (`src/harness/claude.ts`): additive permission merges,
MCP registration that never overwrites user entries, idempotent hook wiring. Harnesses
without hook/permission surfaces receive policy files + instruction blocks — enforced
by convention, and the docs say so honestly rather than pretending enforcement exists.

## Security posture

- Subprocesses run via argv arrays only (`Bun.spawn`) — no shell interpolation, ever.
- No network calls during init/apply/verify. Tool installation is guidance, not download.
- Env values never appear in generated files, logs, or audit entries (probe-tested).
- Approval-gate defaults: destructive shell / credential use / production-affecting can
  never be configured to `allow` — the writer refuses.
- The injection scanner (`scan-untrusted.ts`) is heuristic defense-in-depth, not
  protection; the primary defense is the read-only-data protocol in the instructions.

## Known v0.1 limitations (honest ledger)

- **Atomicity:** `apply` writes files one at a time (whole-content per write). An
  interruption mid-apply leaves a partial state — `ade verify` detects it and a
  re-run of `ade apply` (idempotent) completes it, but there is no staged-then-swap
  transaction or rollback. Planned for v0.2, as is `ade remove` (clean uninstall of
  managed blocks and generated artifacts).
- **Harness coverage split:** claude-code is exercised live (hooks, permissions,
  MCP merges, real machine probes). The other six harnesses are covered by
  fixture-based tests of their instruction-file surfaces only.
- **Enforcement honesty:** policy files are contracts consumed by harnesses via
  instructions (and by future tooling); only surfaces with real hook/permission
  mechanisms (claude-code in v0.1) get hard enforcement.
- **Extension model:** the module registry is compiled-in by design (a dynamic
  plugin loader is a supply-chain attack surface). "Extension" today means adding
  a file + registry entry in a fork/PR, not dropping a plugin into a directory.

## Testing

`bun test --coverage` enforces 95% line / 95% function (bunfig threshold). The framework
conformance suite (`tests/framework.test.ts`) runs every module through interface,
plan-never-writes, double-apply-idempotency, absent-tooling, and structured-verify
invariants — module-specific tests cannot substitute for it. Security anti-criteria
live in `tests/anti.test.ts`. Live integration against real tools (TruffleHog, RTK,
OCEAN, git) is exercised in `tests/live.e2e.test.ts` where present on the machine.
