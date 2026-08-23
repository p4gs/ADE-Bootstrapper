---
type: Architecture Guide
title: The status menu-bar helper
description: A refresh budget bounded by construction, a degraded menu whose actions always work, and why the icon is drawn geometry rather than a glyph.
tags: [menubar, tray, timeout, resilience]
sources:
  - id: openwiki-source-7358428d422bd02e17be812e
    resource: repo://crates/ade-core/src/gui/inventory.rs
  - id: openwiki-source-7550f6df90b28a8f8ec31bc1
    resource: repo://crates/ade-core/src/gui/menubar.rs
  - id: openwiki-source-8c91467f092d0de6d2acd05e
    resource: repo://crates/ade-status/src/main.rs
generated: {by: "claude-code", at: "2026-08-22T21:06:03.708Z"}
verified:
  - by: openwiki/0.3.3
    at: 2026-08-22T21:06:03.708Z
---

# The status menu-bar helper

Rust-only; no TypeScript counterpart. This crate has no `tests/` directory at all — every
test is in-file, and the header explains why: everything testable here is pure Rust, so
the tests stop at that boundary deliberately.

## The budget is bounded by construction

The per-probe timeout is strictly shorter than the poll interval, and the main-thread tick
**does no work at all** unless a generation counter moved. On a quiet machine the cost is
one integer comparison every couple of seconds.

No platform API is ever touched off the main thread, and no detection ever runs on it. The
poll loop and the manual refresh both spawn; the timer handler only compares generations
and renders, and rendering is a method on a main-thread-only type — **the type system
proves the thread**, rather than a comment asserting it.

A panic-catching wrapper is the whole resilience contract in one call: any panic inside
detection becomes an error value rather than propagating, which is what lets the poll loop
survive to the next cycle and gives rendering a result to match on instead of crashing.

Parallelism means the bound is **per probe, not per capability** — one scoped thread per
capability, all joined, with the timeout wrapping only the subprocess seam.

**"Cannot hang the menu bar" is proven with real wall-clock rather than a mock.** One test
injects a subprocess that sleeps for an hour on *every* probe and asserts the whole pass
completes successfully in under twice the timeout. Another injects a panicking subprocess
and asserts the pass resolves quickly rather than waiting the timeout out, because the
dying thread drops its channel.

## One rollup, adopted after a disagreement

The status derivation lives entirely in the shared core and is the same rollup the desktop
app uses. That was adopted after the tray reported one error while the app said two — and
a test now asserts payload equality between the two entry points field by field.

The three-state aggregate collapses four verdicts, and the pre-detection state deliberately
maps to healthy rather than to a warning, because not having looked yet is not a failure.

The title is silent, counted, or a bang: healthy shows nothing at all. A tray with no
payload shows a question mark.

## No glyph anywhere

The icon is a **drawn template image** — geometry, not text — so nothing can land on the
platform's emoji rendering path. It uses the redrawing form so the block re-runs at every
backing scale and stays crisp on high-density displays, where the older rasterize-once
path would not.

The per-item marks are plain ASCII for the same reason: the platform draws the obvious
warning glyph set as emoji in the menu bar. That is the same reason the repository-wide
ban covers geometric dots.

**Adding a capability never requires touching this binary** — the core formats everything,
glyphs and labels and counts included, and this crate only renders the resulting items.

## The degraded-menu contract

The dropdown's content is a pure value computed with **zero platform API**, which is
exactly what makes the contract provable with an ordinary test rather than only a live
accessibility walk.

The three action rows are appended unconditionally, outside the payload match: whatever
detection did, refresh, open, and quit keep working. There is no disabled-action state —
a degraded snapshot changes what is shown *above* those rows, never whether they work.

## Where the ceiling is

The dropdown truncates at a fixed row cap with no overflow row, and the current capability
count leaves a margin of two. This is the one place where "adding a capability never
requires touching the app" has a limit: past the cap, a capability would be silently
invisible in the menu though still counted in the label and the verdict. Raising it does
require editing this crate.

No platform path is unit-tested at all — the tests stop at the pure boundary by design,
and the header documents the live-verification method instead, including why the obvious
window-list probe finds nothing even for a working item.

The helper never writes state; it reads, and refresh only re-polls its own snapshot.

## Source map and tests

`crates/ade-status/src/main.rs` and `crates/ade-core/src/gui/menubar.rs`. Focused tests:
`menu_always_carries_working_actions_even_when_detection_failed`,
`a_healthy_payload_carries_the_same_three_actions` (a regression guard so nobody makes the
actions conditional while fixing the degraded case),
`poll_once_is_bounded_even_when_every_probe_hangs_forever`,
`poll_once_isolates_a_single_dead_probe_without_failing_the_whole_pass`, and
`the_tray_and_the_control_center_read_from_one_rollup`.
