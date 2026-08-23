---
type: Reference
title: Testing and CI gates
description: The six gates CI enforces, both 95 percent coverage floors and their exclusions, and an honest account of what none of it catches.
tags: [ci, coverage, gates, supply-chain]
sources:
  - id: openwiki-source-164e2da859b5277df81c7d94
    resource: repo://.github/workflows/ci.yml
  - id: openwiki-source-7dc952d611a75d93fb9b2fb5
    resource: repo://bunfig.toml
  - id: openwiki-source-b87df2ba96cb4e359e448b6b
    resource: repo://crates/ade-core/src/run.rs
  - id: openwiki-source-ecfd9644f06b8f47a0aa7c14
    resource: repo://crates/ade/tests/cli.rs
  - id: openwiki-source-845ef3fba7d5519c73daee5e
    resource: repo://deny.toml
  - id: openwiki-source-f5f66cd5930b37f7b967e230
    resource: repo://scripts/coverage-check.sh
  - id: openwiki-source-480cd6b9df05eb395fc288cc
    resource: repo://scripts/emoji-ban.sh
generated: {by: "claude-code", at: "2026-08-22T21:06:03.708Z"}
verified:
  - by: openwiki/0.3.3
    at: 2026-08-22T21:06:03.708Z
---

# Testing and CI gates

Three parallel jobs with no dependencies between them, so any one can fail the run: a
Rust job and a parity job on macOS, and the TypeScript oracle job on Linux. Triggers are
pushes to the main branch and every pull request.

## The Rust gates, in order

Formatting, then clippy with **every lint promoted to an error across all targets
including tests**, then the full workspace test run, then the dependency policy check,
then the glyph ban, then coverage. The order matters: cheap gates fail first, and
coverage runs last because it needs a second instrumented build.

The workspace test run covers all four crates — 631 test functions. The desktop crates
are **tested** even though they are **excluded from coverage measurement**.

## Both floors are 95 percent, enforced differently

Rust: 95 percent lines *and* 95 percent functions, with exactly two crates excluded by
filename regex. The exclusion is documented rather than silent: those two are an egui
render loop and an AppKit run loop, the structurally-untestable entry-point class, and
every decision they *display* is computed in the shared core, which is covered.

TypeScript: the same 95/95, but enforced by the runner's configuration and **opt-in per
run**. Plain `bun test` runs fast without the gate; only the coverage variant applies it.
So a contributor can see green while below the floor. The only exclusion is the test
helper module.

The project's own instruction to itself is explicit: the coverage floor is a hard gate,
never lowered to make a change pass.

## The glyph ban is broader than emoji

It bans the emoji ranges and the variation selector, **and** the geometric status dots and
several literal marks. The header records that the last sanctioned exception was removed
when the menu-bar dot became drawn geometry.

Its scope is deliberately narrow — the Rust tree and one packaging script — because the
TypeScript tree is a frozen reference implementation whose CLI glyphs are fixture output
rather than product UI. That exclusion is load-bearing, and it has a visible consequence:
the two implementations print different human output by design, and no gate compares
stdout across them.

The script distinguishes all three search-tool exit codes and hard-fails when the tool is
absent, so a missing dependency cannot produce a false pass.

## Dependency policy

All four sub-checks run: licenses restricted to a permissive allowlist, yanked crates
denied, only the canonical registry permitted, and wildcard versions denied. Multiple
versions are a warning rather than an error because the desktop stack pulls its own
transitive graph.

The evaluated graph is **restricted to the two macOS targets**, which suppresses one
specific unmaintained-dependency advisory that reaches the tree only through a
Linux-only path. The policy file states the condition for removing that restriction:
when the desktop app ports to another platform, delete the section and deal with the
result honestly.

## Hermetic invocation: there is no such requirement here

Worth stating because it is a reasonable thing to assume. No environment variables are
required to run either suite — a repository-wide search finds none, and no test performs
a real commit. The isolation that exists is built into the tests rather than demanded of
the developer: the integration suite points the machine-state and home directories at
disposable fixtures.

The residual is honest: those overrides layer on top of the inherited environment rather
than starting clean, so the developer's global git configuration is visible to fixtures.
It reaches findings, never emitted artifacts.

Secret leakage into generated files is gated by dedicated anti-tests that plant a
sentinel in the environment and assert no generated file contains it — a real gate rather
than environment discipline.

## What none of it catches

Assertion quality is not machine-checked: both floors count lines and functions, and
nothing enforces that a covered line is asserted on. The pre-commit secret scan is a
local hook, not a CI gate. And the parity job builds only the CLI, so desktop compilation
is proven solely by the workspace test run.

## Source map

`.github/workflows/ci.yml`, `scripts/coverage-check.sh`, `scripts/emoji-ban.sh`,
`bunfig.toml`, `package.json`, `deny.toml`. Local entry points are `bun run check` for
the oracle tree and `cargo test` for Rust.
