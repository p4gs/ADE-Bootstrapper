---
type: Quickstart
title: ADE Bootstrapper
description: What this repository builds, where its two implementations live, and a task-routing map from what you are trying to do to the page and the code that answers it.
tags: [quickstart, orientation, routing]
sources:
  - id: openwiki-source-651d1fb6c9e49916a916ab51
    resource: repo://Cargo.toml
  - id: openwiki-source-9354fd8eff5aedb7910e4dd0
    resource: repo://crates/ade-core/src/registry.rs
  - id: openwiki-source-81dae2a59f861ac36fa4c84b
    resource: repo://crates/ade-core/src/report.rs
  - id: openwiki-source-23775c3de52f3ab95a13cb8b
    resource: repo://README.md
  - id: openwiki-source-f5f66cd5930b37f7b967e230
    resource: repo://scripts/coverage-check.sh
  - id: openwiki-source-19a95c0d3a87a8bdb276e1cf
    resource: repo://scripts/parity-check.sh
generated: {by: "claude-code", at: "2026-08-22T21:06:03.708Z"}
verified:
  - by: openwiki/0.3.3
    at: 2026-08-22T21:06:03.708Z
---

# ADE Bootstrapper

This repository builds a tool that bootstraps a secure, verifiable Agentic Development
Environment into any repository — guardrails, governance and verification as plain files,
with no control plane, no telemetry, and nothing listening on any port.

The shipping artifact is a single static Rust binary plus two native macOS apps.

## Read this first: there are two implementations

`crates/` is the v0.2 Rust implementation and the shipping artifact. `src/` with
`tests/*.test.ts` is the v0.1 TypeScript tree, retained as the **executable oracle** that
a differential harness diffs against. Ported Rust files carry a `port of src/…` header
naming their counterpart.

The practical consequence, and the single most useful thing to know before your first
change: **a Rust-only change that alters an emitted file fails parity.** Start at
[two implementations and the parity harness](verification/dual-implementation-and-parity.md).

## The shape of the thing

Four workspace crates. `ade-core` is the engine — modules, lockfile, audit chain, managed
blocks, agent translation, and the shared logic behind both desktop apps. `ade` is the
CLI. `ade-control-center` and `ade-status` are the native apps, and both are driven
in-process by the same engine, which is why the command line and the apps cannot disagree
about machine state.

Fifteen modules live in one flat directory and are registered in a hand-maintained
vector. That order is load-bearing twice: it is the apply order **and** the
instruction-composition order.

## Task routing

| If you are trying to… | Read | Code |
|---|---|---|
| Understand what a run actually does | [the apply pipeline](engine/pipeline.md) | `ade-core/src/run.rs` |
| Add or change a module | [the module contract](modules/module-contract.md) | `ade-core/src/modules/`, `registry.rs` |
| Work out why verify fails | [lockfile and verify](engine/lockfile-and-verify.md) | `ade-core/src/lockfile.rs` |
| Find who owns a generated file | [the generated artifact tree](engine/generated-artifacts.md) | the table on that page |
| Change what lands in an instruction file | [instructions and managed blocks](engine/instructions-and-managed-blocks.md) | `managed.rs`, `translate.rs` |
| Understand the tamper-evidence story | [the audit chain](engine/audit-chain.md) | `audit.rs`, `hook.rs` |
| Add a coding-agent target | [coding-agent integration](engine/harness-integration.md) | `ade-core/src/harness/` |
| Debug "the tool is installed but not detected" | [detection and the environment layer](engine/detection-and-environment.md) | `envpath.rs`, `exec.rs` |
| Understand why output is byte-stable | [determinism and atomic I/O](engine/determinism-and-io.md) | `fsutil.rs` |
| Add a CLI command | [the command surface](cli/command-surface.md) | `crates/ade/src/main.rs` |
| Work on either desktop app | [capability inventory](desktop/capability-inventory.md), [the Control Center](desktop/control-center.md) | `ade-core/src/gui/`, the two app crates |
| Change a colour, icon or layout | [design system and visual verification](desktop/design-system-and-visual-verification.md) | `gui/tokens.rs`, `theme.rs` |
| Get a change through CI | [testing and CI gates](verification/testing-and-gates.md) | `.github/workflows/ci.yml`, `scripts/` |

## Verify quickly

`cargo test` for Rust and `bun run check` for the oracle tree. Both coverage floors are 95
percent lines and functions, and the project's instruction to itself is that the floor is
a hard gate, never lowered to make a change pass.

## Two things about this repository specifically

**It is bootstrapped with its own tool.** Running `ade init .` here produces the
configuration, lockfile, policy tree and managed instruction blocks that any target
repository gets. Those artifacts are reproducible on demand rather than committed, so
treat them as something you can regenerate rather than as a source to quote.

**`ISA.md` is the product's state-of-record** and is large. Do not read it whole. It is
indexed for semantic search, so query it — asking a question returns the relevant passage
directly, which is far cheaper than scanning it.

## Where to look when this wiki is wrong

Source and tests are authoritative. Every page here names the files and the focused tests
behind its claims, so a disagreement between a page and the code is resolved in the code's
favour — and the page should be corrected.
