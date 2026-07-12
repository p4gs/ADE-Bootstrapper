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

Content-addressed record of every fully-ADE-owned artifact (`.ade/**`, excluding the
mutable audit log) plus environment facts (tool versions, OS/arch).

- **No timestamps.** Identical inputs → byte-identical lockfile.
- Content tampering (hash mismatch / missing file) **fails** `ade verify`.
- Environment drift (tool versions differ from lock time) is an **informational**
  verdict, never a failure — a teammate on another machine gets facts, not errors.
- User-owned files (CLAUDE.md, .gitignore, settings.json) are NOT hash-locked; their
  ADE-managed regions are verified structurally instead (see managed blocks).

### Managed blocks (co-owning user files)

ADE content in user-owned files lives strictly between `<!-- ade:begin -->` and
`<!-- ade:end -->`. The provenance line embeds a `content-hash` of the block body.

- Content outside markers is never modified — translation preserves it byte-for-byte.
- A hand-edit **inside** the block (hash mismatch) is refused, not clobbered.
- Corrupt marker states (duplicated / unterminated / reversed) refuse the file.

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

**Threat model note:** repo-local hook scripts wired into a harness are a tamper
target. `ade verify` hash-checks them via the lockfile; a modified hook fails verify.

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

## Testing

`bun test --coverage` enforces 95% line / 95% function (bunfig threshold). The framework
conformance suite (`tests/framework.test.ts`) runs every module through interface,
plan-never-writes, double-apply-idempotency, absent-tooling, and structured-verify
invariants — module-specific tests cannot substitute for it. Security anti-criteria
live in `tests/anti.test.ts`. Live integration against real tools (TruffleHog, RTK,
OCEAN, git) is exercised in `tests/live.e2e.test.ts` where present on the machine.
