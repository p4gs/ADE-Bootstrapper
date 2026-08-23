---
type: Architecture Guide
title: Two implementations and the parity harness
description: Why a TypeScript tree still lives here, what the differential harness actually compares, and two verified bugs that make it weaker than it looks.
tags: [parity, oracle, rust, typescript]
sources:
  - id: openwiki-source-c9b3c3d031a5755aaf737bda
    resource: repo://crates/ade-core/src/harness/claude.rs
  - id: openwiki-source-de8b119079c26abbfe93ce68
    resource: repo://crates/ade-core/src/lib.rs
  - id: openwiki-source-fdd192eaa25deaf8ab61ea29
    resource: repo://crates/ade/src/main.rs
  - id: openwiki-source-19a95c0d3a87a8bdb276e1cf
    resource: repo://scripts/parity-check.sh
generated: {by: "claude-code", at: "2026-08-22T21:06:03.708Z"}
verified:
  - by: openwiki/0.3.3
    at: 2026-08-22T21:06:03.708Z
---

# Two implementations and the parity harness

This is the page to read first if you are changing behavior. Two implementations of the
same product live in one repository: `crates/` is the v0.2 Rust implementation and the
shipping artifact; `src/` plus `tests/*.test.ts` is the v0.1 TypeScript tree, retained as
the **executable oracle**. That split is a stated contributor invariant, not folklore, and
every ported Rust file carries a `port of src/…` header naming its counterpart.

**The practical consequence: a Rust-only change that alters an emitted file fails parity.**
Changing behavior means changing both trees, or consciously moving the allowlist.

## What the harness actually compares

`scripts/parity-check.sh` is a differential harness over **emitted files** — not over
code, not over stdout. It builds two identical minimal fixtures, bootstraps one with the
oracle and one with the compiled binary, and compares the resulting trees file by file.

The fixture emits 37 files. Exactly 8 differ, and all 8 are allowlisted through a regex
anchored at both ends. The other 29 — every guardrail document, every policy file, the
manifest, the codemap, the templates — are byte-identical.

Every sanctioned divergence traces to **one root cause**: the Rust port refuses to require
a JavaScript runtime in the bootstrapped repository. The oracle writes self-contained bun
scripts; Rust writes POSIX shell shims at the same paths that delegate to the `ade`
binary. That single swap cascades into the instruction file, both instruction targets
(whose managed-block content hash covers the changed line), and the settings file.

The harness also asserts **cross-version interoperability**: the Rust binary is pointed at
the TypeScript-bootstrapped fixture and must verify, apply, verify again, and validate the
audit chain — with only instruction drift tolerated on the first pass.

## Two verified bugs in the harness

Both were found by reading the script rather than by it failing, and both were reproduced.

**The one-sided-file check cannot fail the run.** It pipes into a `while` loop, so the
loop body runs in a subshell and its failure flag never reaches the parent scope. The
message prints and the script still exits zero. The first comparison loop uses process
substitution and *does* propagate — the two loops differ in exactly that way. Practical
effect: **a file emitted by only one implementation is reported but not enforced.**

**The instructions-file content assertion is dead** under the script's own pipefail
setting. The diff command returns non-zero whenever the files differ, which is the only
case that matters, and pipefail propagates that through the pipeline even when the final
stage succeeds — so the negation always selects the no-op branch. The claim that the
delta is only the runtime swap is therefore asserted in a comment, not enforced.

## What is outside parity entirely

The harness only exercises bootstrap. Commands present on both sides — plan, status,
doctor, modules, translate, lock — are never compared. And a large Rust-only surface has
no counterpart at all: removal, the whole desktop tree, the environment-path layer, the
hook subcommands, posture export, and both desktop crates. Nothing in the oracle
constrains any of it.

Two further honest limits. **Parity never compares stdout or exit codes** — and the two
CLIs demonstrably differ in human output, because the emoji ban applies to the Rust tree
and not the frozen oracle. And the manifest is required byte-identical while recording the
machine's real tool versions; it passes because both sides observe the same host in the
same run, which is symmetric non-determinism rather than a deterministic artifact. A tool
upgrade landing between the two bootstraps would break parity for a reason unrelated to
either implementation.

## Source map

`scripts/parity-check.sh` is the whole gate, run in CI against a debug build. The paired
fixture builders — `tests/helpers.ts` and `crates/ade-core/src/testutil.rs` — both
hardcode the same fifteen module ids so neither depends on the registry.
