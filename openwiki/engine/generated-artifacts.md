---
type: Reference
title: The generated artifact tree
description: Every path ade init writes, classified as generated, user-owned or co-owned, with the module that owns it and the mechanism that protects it.
tags: [artifacts, ownership, lockfile, co-ownership]
sources:
  - id: openwiki-source-9117d90d33bcf47d4a922a73
    resource: repo://crates/ade-core/src/fsutil.rs
  - id: openwiki-source-8483ecf5b49cab9ff96f9ca3
    resource: repo://crates/ade-core/src/lockfile.rs
  - id: openwiki-source-b8c2c363fa9dbf140ad375ad
    resource: repo://crates/ade-core/src/modules/context_mgmt.rs
  - id: openwiki-source-206a6c0d6c2287f8df7f38af
    resource: repo://crates/ade-core/src/modules/guardrails.rs
  - id: openwiki-source-cfbd782bff9bd976c9f7fdec
    resource: repo://crates/ade-core/src/modules/memory.rs
  - id: openwiki-source-4163de8a5fe0298bc5e60732
    resource: repo://crates/ade-core/src/modules/observability.rs
  - id: openwiki-source-e4a8b9d009a1a5f32f017be1
    resource: repo://crates/ade-core/src/modules/secrets.rs
  - id: openwiki-source-c31e0f37a036367c91829e75
    resource: repo://crates/ade-core/src/modules/shared.rs
  - id: openwiki-source-843fcff77dfd32cd7a7ce0c4
    resource: repo://crates/ade-core/src/types.rs
generated: {by: "claude-code", at: "2026-08-22T21:06:03.708Z"}
verified:
  - by: openwiki/0.3.3
    at: 2026-08-22T21:06:03.708Z
---

# The generated artifact tree

This is the object [verify](lockfile-and-verify.md) and [removal](removal.md) both
operate on. Everything below is derived from the module sources and the lockfile
scanner, not from any bootstrapped repository's own output.

## Ownership classes are behavior, not annotations

Nothing on disk is labelled "generated" or "user-owned". The class is simply *which
writer produced it*:

- **Generated** — written through the artifact writer or the policy helper, rewritten
  wholly on every apply, and hash-locked because it lives under `.ade/`.
- **User-owned** — written once, guarded by an existence check, never overwritten.
- **Co-owned** — merged into a file ADE does not own, via deep merge, a managed block,
  or line insertion.

Only the generated class is hash-locked, because lockfile scope is exactly `.ade/`
minus `.ade/audit/`. Co-owned and user-owned files are protected by other mechanisms:
markers and content hashes, subtraction on removal, and existence checks.

## The table

| Path | Class | Owning module |
|---|---|---|
| `ade.json` | user-owned | core (init) |
| `ade.lock.json` | generated (not in its own map) | core (apply) |
| `.ade/instructions.md` | generated | core |
| `.ade/instructions.local.md` | user-owned stub, written once — but see the note below | core |
| `.ade/guardrails/*.md` (7 files) | generated | `guardrails` |
| `.ade/policy/dependencies.json` | generated | `supply-chain` |
| `.ade/policy/sandbox.json` | generated | `sandbox` |
| `.ade/context/codemap.md` | generated, content derived from your tree | `context` |
| `.ade/policy/context-engines.json` | generated | `context` |
| `.ade/templates/*.md` (3 files) | generated | `scaffolding` |
| `.ade/memory.json` | generated | `memory` |
| `.ade/policy/context-trust.json` | generated | `injection-defense` |
| `.ade/hooks/scan-untrusted.ts` | generated | `injection-defense` |
| `.ade/policy/instructions-governance.json` | generated | `config-governance` |
| `.ade/audit/README.md` | generated | `observability` |
| `.ade/hooks/audit-log.ts` | generated, only when claude-code is targeted | `observability` |
| `.ade/audit/log.jsonl` | append-only runtime state, **not** hash-locked | `observability` |
| `.ade/policy/approvals.json` | generated | `approval-gates` |
| `.ade/policy/secrets.json` | generated | `secrets` |
| `.ade/policy/git.json` | generated | `git-hygiene` |
| `.ade/policy/budget.json` | generated | `cost-governance` |
| `.ade/manifest.json` | generated | `reproducibility` |
| `.ade/policy/token-efficiency.json` | generated | `token-efficiency` |
| `CLAUDE.md` | co-owned, managed block | core (translation) |
| `AGENTS.md` | co-owned, managed block, shared by five agents | core (translation) |
| `.cursor/rules/ade.mdc` | co-owned, managed block | core (translation) |
| `.claude/settings.json` | co-owned, additive merge | `approval-gates`, `sandbox`, `observability` |
| `.mcp.json` | co-owned, opt-in only | `memory` |
| `.gitignore` | co-owned, three labelled blocks | `observability`, `memory`, `secrets` |
| `.pre-commit-config.yaml` | written only when absent; yours once it exists | `secrets` |
| `.git/hooks/pre-commit` | co-owned via marker plus chain-aside | `secrets` |
| `.git/hooks/pre-commit.pre-ade` | your original hook, renamed | `secrets` |

## Conditionals

The table is partly configuration-dependent. The audit hook requires claude-code to be
a configured target. `.mcp.json` requires the memory module's opt-in option.
`.pre-commit-config.yaml` requires both the `pre-commit` binary and the file's absence.
The git hook requires an actual git repository. And the `.gitignore` line for the audit
directory is skipped when the observability module is told to commit the log.

`.ade/context/codemap.md` is the one generated artifact whose content depends on **your**
repository tree, so its hash legitimately changes when your code changes even though
nothing about ADE changed.

## Two mismatches worth knowing

A module's reported written paths are **not** the same set as the lockfile covers.
Reported paths include `.claude/settings.json` and `.mcp.json`, neither of which is
hash-locked.

The user-owned `.ade/instructions.local.md` is nonetheless inside lockfile scope,
because the scanner excludes only the audit directory. Editing it fails `ade verify`
with a message calling it a generated file, until the next apply or lock.

A module that fails writes nothing — option validation returns before any write — so a
bad configuration value cannot leave a half-written policy file behind.

## Source map and tests

`crates/ade-core/src/lockfile.rs`, `crates/ade-core/src/remove.rs`,
`crates/ade-core/src/registry.rs`, and each module under
`crates/ade-core/src/modules/`. Focused tests:
`apply_composes_instructions_translates_locks_and_audits` and
`double_apply_is_byte_stable_while_audit_checkpoint_advances` in `run.rs`,
`apply_is_idempotent_second_run_byte_identical_across_all_artifacts` in
`observability.rs`, and `policy_write_read_and_verify_round_trip` in `modules/shared.rs`.
