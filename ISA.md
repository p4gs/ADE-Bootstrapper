---
task: "Create ADE Bootstrapper toolkit from owner's spec"
slug: 20260712-084930_ade-bootstrapper
project: ADE-Bootstrapper
effort: E4
effort_source: classifier
phase: execute
progress: 12/146
mode: interactive
started: 2026-07-12T08:49:30Z
updated: 2026-07-12T08:49:30Z
---

# ADE Bootstrapper — Project ISA

## Problem

AI coding harnesses (Claude Code, Codex, Cursor, OpenCode, Antigravity, Hermes, Pi) are deployed today with ad-hoc, insecure, inconsistent environments: no unified policy model, no supply-chain gate on AI-native dependencies, no tamper-evident audit trail, no approval gates, no secrets hygiene at the git boundary, no reproducible setup, and per-harness config files (CLAUDE.md, AGENTS.md, .cursorrules) that drift apart silently. Each developer re-solves these problems badly, or not at all. The spec at `ADE Bootstrapper.md` names sixteen component areas and a design contract (opinionated, modular, composable, harness-native, local-first, secure-by-default, cross-platform); no implementation exists — the repo contains only the spec.

## Vision

A developer runs `ade init` in any repository and thirty seconds later has a hardened, governed, reproducible Agentic Development Environment: secure-coding guardrails wired into every harness's instruction file, secrets scanning at the commit boundary, a tamper-evident audit chain, approval-gate policy mapped to the harness's permission surface, one canonical instruction source translated to every harness without drift, and a lockfile that makes the whole setup verifiable on any machine. The euphoric surprise: it isn't a checklist document — it's a working tool where `ade doctor` and `ade verify` prove the environment is what it claims to be, and every integrated tool (TruffleHog, RTK, OCEAN, nono, OpenMemory) either lights up when present or degrades to actionable guidance when absent.

## Out of Scope

Per the spec's non-goals: no general-purpose agent framework, no multi-agent runtime or orchestration SDK, no ADK for bespoke agents, no replacement of the coding harness itself, and no reimplementation of best-of-breed tools where integration is the better path (we integrate TruffleHog, we do not write a secret scanner; we integrate RTK, we do not write an output compressor). Also out of v0.1 scope: a hosted/network control plane (local-first only), Windows-native testing (code is written cross-platform-aware but v0.1 is verified on macOS/Linux), a GUI, telemetry of any kind, automatic remote sync of memory content (v0.1 wires MCP config for OpenMemory; it does not implement a sync server), and live enforcement inside harnesses we cannot hook (policy files + instruction blocks are the mechanism for harnesses without hook surfaces).

## Principles

- **Push controls to the boundary where risk occurs**: the shell boundary, dependency boundary, prompt/context boundary, git boundary, network boundary — never in a distant abstraction layer.
- **Integrate before rebuilding**: every component wraps or wires a best-in-class tool when one exists; the bootstrapper's own code is glue, policy, and verification.
- **Plain files over control planes**: every artifact the tool writes is a human-readable, diffable, version-controllable file (JSON/Markdown/YAML) in the target repo.
- **Secure-by-default with explicit opt-outs**: every module defaults to its safest posture; weakening requires an explicit config edit that survives in git history.
- **Deterministic outputs**: identical inputs produce byte-identical generated files; anything machine-dependent (tool versions) lives only in the lockfile/manifest.
- **Graceful degradation is a feature**: an absent tool never crashes a flow; it produces a precise finding with installation guidance.
- **Verification over assertion**: `ade verify` re-derives state from disk and compares against the lockfile; nothing is trusted because it was once written.

## Constraints

- **Bun + TypeScript only** (owner's global rule); zero runtime dependencies — dev-deps limited to `typescript` for typechecking.
- **No shell interpolation with external input**: all subprocess execution via argument arrays (`Bun.spawn` with argv), never string concatenation into a shell.
- **95% line + 95% function coverage floor**, enforced by a local check target and CI gate, meaningful assertions only (owner's global rule); structurally untestable entry-point lines documented via ignore pattern.
- **Managed-block editing only** for user-owned files (CLAUDE.md, AGENTS.md, .cursorrules, settings.json): ADE content lives between explicit markers; content outside markers is never modified or deleted.
- **No secrets in generated files, logs, or code** — the tool that enforces secret hygiene must itself be clean.
- **No network calls at bootstrap time** in v0.1 (deterministic, offline-capable); tool installation is guidance, not automatic download.
- **The spec file `ADE Bootstrapper.md` is owner-authored and read-only** — never edited by this work.

## Goal

Ship ADE Bootstrapper v0.1: a zero-runtime-dependency Bun/TypeScript CLI (`ade`) that implements all fifteen spec component areas as detect/plan/apply/verify modules over a unified config (`ade.json`) + lockfile (`ade.lock.json`) model with harness adapters for seven harnesses — verified by a test suite at ≥95% line/function coverage, an end-to-end bootstrap of a fixture repository, and live integration probes against the tools actually present on this machine (TruffleHog, RTK, OCEAN, git).

## Criteria

### Foundation & repo hygiene

- [ ] ISC-1: Repo is a git repository on branch `main` with ≥1 commit containing the full v0.1 tree
- [ ] ISC-2: `package.json` exists with `"type": "module"`, bun-targeted scripts (`test`, `typecheck`, `check`), and zero entries in `dependencies`
- [ ] ISC-3: `tsconfig.json` exists with `"strict": true`
- [ ] ISC-4: `bunx tsc --noEmit` exits 0 (typecheck clean)
- [ ] ISC-5: `bun test` exits 0 with 0 failures
- [ ] ISC-6: Coverage gate: `bun test --coverage` reports ≥95% line AND ≥95% function coverage, enforced via `bunfig.toml` coverageThreshold (run exits non-zero below threshold)
- [ ] ISC-7: `.github/workflows/ci.yml` exists running typecheck + coverage-gated tests on push/PR
- [ ] ISC-8: `.gitignore` excludes `node_modules`, coverage artifacts, and `.env*`
- [ ] ISC-9: `README.md` documents install, quickstart (`ade init`), every CLI command, the module table, and the design contract from the spec
- [ ] ISC-10: The spec file `ADE Bootstrapper.md` is byte-identical to its pre-task state
- [ ] ISC-11: `trufflehog filesystem` (or git mode) scan of the repo reports 0 verified secrets
- [ ] ISC-12: No source file contains a hardcoded absolute user-home path (`/Users/`) — portable paths only

### CLI surface

- [ ] ISC-13: `bun run src/cli.ts --help` exits 0 and lists every command with one-line descriptions
- [ ] ISC-14: `ade version` prints the version matching `package.json`
- [ ] ISC-15: `ade init <dir>` bootstraps a target repo end-to-end: writes `ade.json`, applies default-enabled modules, writes `ade.lock.json`, exits 0
- [ ] ISC-15.1: `ade init` on a non-git directory completes with git-dependent modules degraded (no crash, precise findings)
- [ ] ISC-16: `ade plan` performs a dry-run: prints every action apply would take and writes NOTHING to disk (verified by before/after tree hash)
- [ ] ISC-17: `ade apply` is idempotent: second consecutive run reports zero changes and target tree is byte-identical
- [ ] ISC-18: `ade doctor` reports tool presence/absence for every integrated tool (trufflehog, pre-commit, gitleaks, rtk, ocean, nono, osv-scanner) plus detected harnesses, exit 0
- [ ] ISC-19: `ade verify` exits 0 on an untampered bootstrapped repo and non-zero after a managed file is tampered
- [ ] ISC-20: `ade status` prints per-module state (enabled/disabled/applied/degraded)
- [ ] ISC-21: `ade modules` lists all 15 modules with id, title, and enabled state
- [ ] ISC-22: `ade translate` regenerates all harness instruction files from the canonical source
- [ ] ISC-23: `ade lock` regenerates `ade.lock.json`
- [ ] ISC-24: `ade audit verify` validates the audit log hash chain, exit 0 when intact and non-zero when tampered
- [ ] ISC-25: Every command supports `--json` and emits parseable JSON to stdout (validated by JSON.parse in tests)
- [ ] ISC-25.1: In `--json` mode stdout is pure JSON — human/progress output goes to stderr only
- [ ] ISC-26: Unknown command or bad flags exit non-zero with a usage message on stderr
- [ ] ISC-27: All CLI exit codes follow 0=success, 1=failure, 2=usage-error convention

### Config model (`ade.json`)

- [ ] ISC-28: `ade init` writes `ade.json` containing schema version, enabled module list, and harness targets
- [ ] ISC-28.1: Config and lockfile carry `schemaVersion`; loader rejects a newer-than-known schemaVersion with upgrade guidance (mixed-version team safety)
- [ ] ISC-29: Config loader rejects malformed JSON with a precise error naming the file
- [ ] ISC-30: Config loader rejects unknown module ids with an error naming the offending id
- [ ] ISC-31: Every module is individually disableable via `ade.json` `modules.<id>.enabled: false` and apply then skips it
- [ ] ISC-32: Default config enables all security-relevant modules (secure-by-default) — disabling is the explicit act
- [ ] ISC-33: Config supports per-module `options` object passed through to the module
- [ ] ISC-34: Generated `ade.json` is deterministic: two inits of identical fixtures produce byte-identical files

### Lockfile & reproducibility

- [ ] ISC-35: `ade.lock.json` records sha256 of every ADE-generated file
- [ ] ISC-36: `ade.lock.json` records detected tool versions (environment manifest) for present tools
- [ ] ISC-37: Lockfile serialization is deterministic (sorted keys, stable array order): regenerating without changes is byte-identical
- [ ] ISC-38: `ade verify` detects a modified generated file via lockfile hash mismatch and names the file
- [ ] ISC-39: `ade verify` detects a deleted generated file and names it
- [ ] ISC-40: Lockfile records the ade version that generated it

### Tamper-evident audit log

- [ ] ISC-41: Apply operations append structured JSONL events to `.ade/audit/log.jsonl` (timestamp, actor, action, target, result)
- [ ] ISC-42: Each audit entry embeds sha256(prevHash + canonicalized entry) forming a hash chain from a fixed genesis value
- [ ] ISC-43: `verifyChain` returns valid=true for an untampered log
- [ ] ISC-44: Modifying any historical entry causes `verifyChain` to return valid=false naming the first broken index
- [ ] ISC-45: Deleting a mid-chain entry causes `verifyChain` to fail
- [ ] ISC-46: Audit entries never contain secret values (redaction test with a planted env-style value)

### Harness adapters

- [ ] ISC-47: Adapter registry covers all 7 spec harnesses: claude-code, codex, cursor, opencode, antigravity, hermes, pi
- [ ] ISC-48: Each adapter declares its instruction file path (CLAUDE.md, AGENTS.md, .cursorrules, …) and capability flags (hooks, mcp, permissions)
- [ ] ISC-49: Harness detection identifies which harnesses are configured in a target repo (by existing config files) and on the machine (by CLI presence)
- [ ] ISC-50: claude-code adapter maps approval-gate policy into `.claude/settings.json` `permissions.deny`/`permissions.ask` via managed merge
- [ ] ISC-51: claude-code adapter can register MCP servers in `.mcp.json` via managed merge preserving pre-existing user entries
- [ ] ISC-52: Adapters for harnesses without hook surfaces still receive the instruction-block translation (degraded but functional)

### Config governance & translation

- [ ] ISC-53: `ade init` creates canonical instructions at `.ade/instructions.md` seeded with the opinionated ADE baseline blocks
- [ ] ISC-54: `ade translate` renders the canonical source into each target harness's instruction file inside `<!-- ade:begin -->` / `<!-- ade:end -->` markers
- [ ] ISC-55: Translation preserves user content outside markers byte-for-byte (test: pre-seeded CLAUDE.md with user text above and below markers)
- [ ] ISC-55.1: Corrupt/unbalanced managed markers in a target file cause translate to abort for that file with an error — the file is never rewritten
- [ ] ISC-55.2: A user edit inside the managed block (embedded content-hash mismatch) causes translate to refuse for that file with remediation guidance instead of clobbering
- [ ] ISC-56: Re-running translate with unchanged canonical source is a no-op (byte-identical files)
- [ ] ISC-57: `ade verify` flags drift when a harness file's managed block no longer matches the canonical source rendering
- [ ] ISC-58: Managed block includes a provenance line naming the canonical source and warning against hand-edits

### Module: secure-coding guardrails

- [ ] ISC-59: Guardrails module writes an opinionated secure-coding ruleset to `.ade/guardrails/` (input validation, injection, secrets, authz, crypto, error handling — ≥6 rule files)
- [ ] ISC-60: Guardrails content is wired into the canonical instructions so every harness receives it via translate
- [ ] ISC-61: Guardrails module detects Project CodeGuard availability and reports integration guidance when absent
- [ ] ISC-62: Rule files include machine-checkable frontmatter (id, severity, applies_to)

### Module: supply-chain security

- [ ] ISC-63: Supply-chain module writes `.ade/policy/dependencies.json` with registry allowlist, minimum-age policy, and install-review requirement
- [ ] ISC-64: Policy explicitly covers AI-native dependencies (skills, plugins, MCP servers, instruction packs, agent configs) as a first-class dependency class
- [ ] ISC-65: Module detects osv-scanner presence; absent → degraded finding with install guidance
- [ ] ISC-66: Module detects ecosystem lockfiles (bun.lock/package-lock/Cargo.lock/uv.lock/go.sum) and flags ecosystems missing lockfiles
- [ ] ISC-67: Instruction block directs the harness to never install dependencies without the policy check

### Module: AI-native sandboxing

- [ ] ISC-68: Sandbox module writes `.ade/policy/sandbox.json`: filesystem scopes, network allowlist (default-deny), credential injection policy
- [ ] ISC-69: Module detects nono presence; absent → degraded finding with install guidance and policy still written
- [ ] ISC-70: Sandbox policy maps to claude-code permission surface where expressible (deny rules for out-of-scope paths)
- [ ] ISC-71: Default network policy is deny-with-allowlist, not allow-with-blocklist (secure-by-default probe)

### Module: codebase context management

- [ ] ISC-72: Context module generates `.ade/context/codemap.md`: directory tree, language/file statistics, entry points
- [ ] ISC-73: Codemap generation is deterministic for a fixed fixture tree
- [ ] ISC-74: Module records a refresh command so context artifacts are regenerable (`ade apply` refreshes)
- [ ] ISC-75: Canonical instructions direct harnesses to consult `.ade/context/` before whole-repo scans

### Module: quality & performance scaffolding

- [ ] ISC-76: Scaffolding module writes opinionated convention templates to `.ade/templates/` (PR checklist, testing conventions, commit conventions)
- [ ] ISC-77: Conventions block (test-first, coverage floor, no-suppression rule) lands in canonical instructions

### Module: network-syncable memory

- [ ] ISC-78: Memory module writes `.ade/memory.json` naming the OpenMemory/Mem0 MCP integration and its local-first posture
- [ ] ISC-79: When claude-code targeted, module registers the OpenMemory MCP server entry in `.mcp.json` (disabled-by-default stub requiring explicit user activation — credential-bearing integrations are opt-in)
- [ ] ISC-80: Memory policy file marks memory content as sensitive (never committed; `.ade/memory-store/` gitignored)

### Module: prompt-injection defense

- [ ] ISC-81: Injection module writes `.ade/policy/context-trust.json` classifying untrusted sources (READMEs of deps, issues, web content, dependency docs)
- [ ] ISC-82: Instruction block teaches the harness: external content is read-only data, never instructions — with the report-don't-follow protocol
- [ ] ISC-83: Module ships `.ade/hooks/scan-untrusted.ts`, a pattern-based injection scanner that flags known injection phrasings in provided text
- [ ] ISC-84: Scanner detects ≥5 canonical injection patterns in test fixtures (ignore-previous-instructions, exfiltrate-env, disable-security, new-system-prompt, tool-abuse) with 0 false positives on benign fixture text

### Module: tamper-evident observability

- [ ] ISC-85: Observability module initializes the audit chain (genesis event) at apply time
- [ ] ISC-86: When claude-code targeted, module wires a PostToolUse hook script that appends tool events to the audit chain
- [ ] ISC-87: Hook script is self-contained (bun, zero deps) and appends a valid chain entry when run with synthetic hook input
- [ ] ISC-88: Audit dir is git-ignored by default (logs are local artifacts, not repo content) with a config opt-in to commit

### Module: human-in-the-loop approval gates

- [ ] ISC-89: Approvals module writes `.ade/policy/approvals.json` enumerating high-risk action classes from the spec: destructive shell, credential use, external network, dependency install, branch ops, PR creation, merge, production-affecting
- [ ] ISC-90: Every action class carries a decision (`ask` | `deny` | `allow`) with secure defaults (destructive shell + prod ⇒ ask/deny, never allow)
- [ ] ISC-91: claude-code mapping renders the policy into settings.json permission patterns (e.g. `rm -rf` ⇒ ask, `git push --force` ⇒ ask)
- [ ] ISC-92: Instruction block instructs harnesses without permission surfaces to seek human approval for the enumerated classes

### Module: secrets & credential hygiene

- [ ] ISC-93: Secrets module detects trufflehog; present ⇒ wires it as the pre-commit secret scan
- [ ] ISC-94: When pre-commit (the framework) is present, module writes `.pre-commit-config.yaml` with the trufflehog hook; when absent, installs a native `.git/hooks/pre-commit` shim (non-destructive: chains any existing hook)
- [ ] ISC-95: Pre-commit shim actually blocks a commit containing a planted verifiable secret in a fixture repo (live probe with trufflehog)
- [ ] ISC-96: Module ensures `.gitignore` covers `.env`, `.env.*`, and common key files (idempotent append)
- [ ] ISC-97: Instruction block forbids echoing secrets into code, logs, prompts, or commits and names scoped-credential practice
- [ ] ISC-98: Module never prints values of environment variables it inspects (redaction probe)

### Module: git & repository hygiene

- [ ] ISC-99: Git module verifies target is a git repo; non-repo ⇒ precise degraded finding (init guidance), no crash
- [ ] ISC-100: Module detects commit-signing configuration and reports state (enabled/disabled + guidance)
- [ ] ISC-101: Module writes `.ade/policy/git.json`: protected branches, force-push policy, required PR flow, branch naming
- [ ] ISC-102: Module detects OCEAN presence and reports `ocean harden` as the deep-hardening path when present (integration, not reimplementation)
- [ ] ISC-103: Instruction block: never force-push protected branches, never rewrite pushed history, PR flow for protected branches

### Module: cost & token budget governance

- [ ] ISC-104: Cost module writes `.ade/policy/budget.json`: per-session and per-project token/cost limits, alert thresholds, model routing preferences
- [ ] ISC-105: Budget policy renders into the canonical instructions (harness-visible budget contract)
- [ ] ISC-106: Policy file validates numerically (limits are positive numbers; malformed budget rejected at load)

### Module: reproducible environment

- [ ] ISC-107: Repro module writes `.ade/manifest.json` capturing OS, arch, and versions of detected tools (bun, git, harnesses, integrated tools)
- [ ] ISC-108: Manifest generation tolerates missing tools (records absence, never crashes)
- [ ] ISC-109: `ade verify` re-derives the manifest and reports version drift against the lockfile-recorded environment as findings (informational, not failure)
- [ ] ISC-110: Harness version pinning: manifest records detected harness CLI versions when present

### Module: token efficiency

- [ ] ISC-111: Token module detects RTK; present ⇒ writes `.ade/policy/token-efficiency.json` with wiring status and documents the shell-boundary integration for the harness
- [ ] ISC-112: RTK absent ⇒ degraded finding with install guidance (policy still written, enabled=false)
- [ ] ISC-113: Live probe on this machine: module detects the installed rtk and reports its version in the manifest

### Module framework invariants (cross-cutting)

- [ ] ISC-114: Every module implements the same interface: `id`, `title`, `category`, `defaultEnabled`, `detect(ctx)`, `plan(ctx)`, `apply(ctx)`, `verify(ctx)`
- [ ] ISC-115: All 15 modules are registered in the registry and `ade modules` count equals 15
- [ ] ISC-116: `plan()` never writes to disk for ANY module (framework-level test iterating all modules against a fixture)
- [ ] ISC-117: `apply()` is idempotent for ANY module (framework-level double-apply test iterating all modules)
- [ ] ISC-118: `verify()` returns structured findings `{ok, findings[]}` for every module post-apply
- [ ] ISC-119: A module throwing is contained: the run reports the module as failed and continues with others (fault isolation test)
- [ ] ISC-120: Absent optional tooling downgrades a module to `degraded` state, never `error` (probe with empty PATH context)

### Security anti-criteria

- [ ] ISC-121: Anti: No subprocess is ever spawned via shell string interpolation — grep proves no `sh -c` with template-interpolated external input in src/
- [ ] ISC-122: Anti: `ade` never deletes or overwrites user content outside managed markers (translation preservation test is the probe)
- [ ] ISC-123: Anti: no generated file, log line, or audit entry contains a value sourced from process env secrets (planted-secret test)
- [ ] ISC-124: Anti: `ade init` on a dirty pre-existing repo never touches files outside `ade.json`, `ade.lock.json`, `.ade/`, harness managed blocks, `.gitignore` appends, and git hooks dir (tree-diff probe)
- [ ] ISC-125: Anti: no module makes a network call during init/apply/verify (offline probe: run with network-guard env and assert no fetch)
- [ ] ISC-126: Anti: default config never sets an approval-gate action class to `allow` for destructive shell, credential use, or production-affecting changes
- [ ] ISC-127: Anti: the repo ships zero runtime dependencies (`dependencies` absent/empty in package.json)
- [ ] ISC-128: Anti: no test asserts nothing (assertion-free coverage padding) — spot-audit probe on the test suite

### End-to-end & live verification

- [ ] ISC-129: E2E: `ade init` on a fresh fixture repo (with package.json + git) produces a complete bootstrap: ade.json + lockfile + .ade tree + CLAUDE.md/AGENTS.md managed blocks — single test proving the full flow
- [ ] ISC-130: E2E: `ade verify` passes immediately after init on the fixture
- [ ] ISC-131: E2E: tampering with a generated policy file then `ade verify` fails naming the file
- [ ] ISC-132: Live: `ade doctor` on THIS machine correctly reports trufflehog=present, rtk=present, ocean=present, nono=absent, pre-commit=absent
- [ ] ISC-133: Live: full bootstrap of a real temp git repo on this machine, then a commit with a planted test secret is BLOCKED by the installed hook (trufflehog live)
- [ ] ISC-134: Live: benign commit in the same repo SUCCEEDS (hook does not false-positive block)
- [ ] ISC-135: JSON output of `ade doctor --json` parses and includes a `tools` array with ≥7 entries

### Documentation & spec fidelity

- [ ] ISC-136: README module table maps every spec component bullet (all 15) to its module id — full spec coverage traceable
- [ ] ISC-137: README states the non-goals verbatim-faithfully from the spec
- [ ] ISC-138: Each module file has a header comment naming the spec component it implements and the boundary it controls
- [ ] ISC-139: `docs/DESIGN.md` records the architecture: config model, lockfile, audit chain, adapter capabilities, module lifecycle
- [ ] ISC-140: ISA (this file) committed to the repo as system of record
- [ ] ISC-141: All work committed; working tree clean at completion (`git status --porcelain` empty)

## Test Strategy

| isc | type | check | threshold | tool |
|-----|------|-------|-----------|------|
| 1-12 | foundation | repo state, gates, scans | exact | Bash(git/bun/trufflehog), Read |
| 13-27 | CLI behavior | run each command against fixtures | exit codes + output shape | bun test (spawn), Bash |
| 28-34 | config | loader unit tests, determinism byte-compare | exact | bun test |
| 35-40 | lockfile | hash recording, determinism, drift detection | exact | bun test |
| 41-46 | audit | chain build/verify/tamper unit tests | exact | bun test |
| 47-52 | adapters | registry shape, managed merges on fixtures | exact | bun test |
| 53-58 | translate | render, preserve, no-op, drift tests | byte-identical | bun test |
| 59-113 | modules | per-module unit tests: detect/plan/apply/verify on fixtures with stubbed tool contexts | per-ISC | bun test |
| 114-120 | framework | invariant tests iterating full registry | all modules | bun test |
| 121-128 | anti | grep probes, planted-secret tests, tree-diff, offline probe | zero violations | bun test, Grep |
| 129-135 | e2e/live | real CLI runs in temp dirs; live tool probes on this machine | exit codes + artifacts | Bash |
| 136-141 | docs | Read/grep README, DESIGN, module headers; git status | present + accurate | Read, Grep, Bash |

## Features

| name | description | satisfies | depends_on | parallelizable |
|------|-------------|-----------|------------|----------------|
| foundation | repo scaffold: package.json, tsconfig, bunfig, gitignore, CI | ISC-1..8 | — | no (first) |
| core-types | Module/Ctx/Adapter/Finding interfaces + exec wrapper | ISC-114, 121 | foundation | no |
| core-context | environment/tool/harness detection | ISC-18, 49, 108 | core-types | no |
| core-config | ade.json load/validate/defaults | ISC-28..34 | core-types | no |
| core-lockfile | deterministic lock + verify | ISC-35..40 | core-types | no |
| core-audit | hash-chain JSONL audit log | ISC-41..46 | core-types | no |
| core-adapters | 7 harness adapters + managed-block/merge engine | ISC-47..52 | core-types | no |
| core-translate | canonical instructions → harness files | ISC-53..58 | core-adapters | no |
| core-cli | arg parsing, command dispatch, run orchestration | ISC-13..27 | all core | no |
| mod-guardrails | secure-coding rules module | ISC-59..62 | core | yes |
| mod-supplychain | dependency policy module | ISC-63..67 | core | yes |
| mod-sandbox | sandbox policy module | ISC-68..71 | core | yes |
| mod-context | codemap module | ISC-72..75 | core | yes |
| mod-scaffolding | quality templates module | ISC-76..77 | core | yes |
| mod-memory | memory MCP wiring module | ISC-78..80 | core | yes |
| mod-injection | injection defense module | ISC-81..84 | core | yes |
| mod-observability | audit wiring + hook module | ISC-85..88 | core | yes |
| mod-approvals | approval gates module | ISC-89..92 | core | yes |
| mod-secrets | secrets hygiene module | ISC-93..98 | core | yes |
| mod-git | git hygiene module | ISC-99..103 | core | yes |
| mod-cost | budget governance module | ISC-104..106 | core | yes |
| mod-repro | manifest module | ISC-107..110 | core | yes |
| mod-token | token efficiency module | ISC-111..113 | core | yes |
| framework-tests | cross-module invariant tests | ISC-114..120 | all modules | no |
| anti-probes | security anti-criteria tests | ISC-121..128 | all modules | no |
| e2e-live | end-to-end + live machine probes | ISC-129..135 | all | no (last) |
| docs | README, DESIGN, module headers | ISC-136..139 | all | partially |
| ship | commit clean tree with ISA | ISC-140..141 | all | no (last) |

## Decisions

- 2026-07-12T08:49Z — **Project ISA at `<project>/ISA.md`** (not task ISA): ADE Bootstrapper is a persistent thing; this file is its system of record.
- 2026-07-12T08:49Z — **Zero runtime dependencies.** The tool that governs supply-chain risk must itself have a null dependency surface. Bun stdlib covers everything needed (spawn, hashing via crypto, fs). Dev-dep: typescript only.
- 2026-07-12T08:49Z — **JSON over YAML for generated policy files**: parseable with zero deps, schema-checkable, diffable. Exception: `.pre-commit-config.yaml` because the pre-commit framework requires YAML (emitted as a static template string, no YAML library needed).
- 2026-07-12T08:49Z — **Managed-block markers** (`<!-- ade:begin/end -->`) for all user-owned files: the only safe way to co-own CLAUDE.md/AGENTS.md with the user; deletion/rewrite of user content is a constitutional-level failure for this tool.
- 2026-07-12T08:49Z — **Forge auto-include impossible (show-your-math)**: codex CLI absent on this machine AND MOONSHOT_API_KEY unset ⇒ Forge, Cato, and Anvil all unavailable. Delegation floor met instead via ultracode Workflow fan-out (parallel module authorship) + in-family adversarial review substituting for Cato at VERIFY. TF-CATO remains open.
- 2026-07-12T08:49Z — **EnterPlanMode skipped**: autonomous /goal session; plan-mode exit requires interactive approval which would stall the Stop-hook-driven run. PLAN phase artifacts land in this ISA instead.
- 2026-07-12T08:49Z — **v0.1 enforcement honesty**: for harnesses with no hook/permission surface, modules ship policy files + instruction blocks and say so (degraded, documented) rather than pretending enforcement exists.
- 2026-07-12T09:05Z — **Advisor findings adopted** (commitment-boundary call): (1) managed blocks embed a body content-hash; translate REFUSES on in-block user edits (ISC-55.2); (2) cursor adapter targets modern `.cursor/rules/ade.mdc` with frontmatter, `.cursorrules` treated as legacy detection signal — adapters gain a per-harness render capability; (3) lockfile carries NO timestamps (pure content addressing + environment facts); tool-version drift and missing binaries produce drift/degraded verdicts, never verify failures; (4) probe-first sequencing: secrets (heavy-IO) + cost (pure policy) modules built by hand to validate the frozen interface before the 13-module fan-out; (5) conformance suite is executor-authored and mandatory — counter to coverage gaming by fan-out agents. Advisor's "current ISA is wrong" line disregarded: `--auto-state` attached stale prior-task context (known tool limitation, 2026-07-10). Advisor's modules-do-no-IO/action-executor architecture deferred to v0.2: v0.1 modules write only through the recorded `ctx.artifacts` writer, which core owns — the audit/lock surface is already centralized.
- 2026-07-12T09:05Z — **Scope held at 15 real modules** (advisor suggested 5 + stubs): most modules are policy-file writers over the same helpers — the heavy interface risk the advisor priced in is concentrated in the 2 probe modules I hand-write first. If fan-out quality fails gates, fallback is stub-and-defer per module.

## Changelog

*(entries appended at LEARN in conjecture/refutation/learning format)*

## Verification

*(evidence appended per ISC at EXECUTE/VERIFY)*
