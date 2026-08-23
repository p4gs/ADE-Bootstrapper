---
type: Architecture Guide
title: The Control Center
description: Native rendering with nothing listening on any port, a pure frame function returning commands, and a poison-recovering lock that keeps the window alive.
tags: [control-center, egui, engine, jobs]
sources:
  - id: openwiki-source-54be6a84896e426dce64a3ad
    resource: repo://crates/ade-control-center/Cargo.toml
  - id: openwiki-source-91d2e373106a243b2d3dfae0
    resource: repo://crates/ade-control-center/src/app.rs
  - id: openwiki-source-c530fad9e5677ace214d53ab
    resource: repo://crates/ade-control-center/src/data.rs
  - id: openwiki-source-0f69b412975f8bbe8103d551
    resource: repo://crates/ade-control-center/src/modals.rs
  - id: openwiki-source-213888e3072e7a4ff6f63e3d
    resource: repo://crates/ade-control-center/src/nav.rs
generated: {by: "claude-code", at: "2026-08-22T21:06:03.708Z"}
verified:
  - by: openwiki/0.3.3
    at: 2026-08-22T21:06:03.708Z
---

# The Control Center

Native immediate-mode rendering with **no webview and nothing listening on any port**.
A search of both desktop crates for socket binding, embedded browsers or HTTP clients
returns exactly one hit, and it is a colour setting for hyperlink text.

The engine is linked **in-process**: the data layer calls engine functions directly with
no transport layer of any kind. And it is the *same* engine crate the CLI uses — one
crate, two front ends, which is why the two can never disagree about machine state.

Rust-only; no TypeScript counterpart. The crate's `tests/` directory contains only
snapshot images.

## The frame is pure

The frame function draws and returns a list of commands; the shell is the only thing that
touches the engine. That is what lets a test assert "clicking this asks for that" with no
engine at all — and most of this crate's tests are exactly that shape.

Every frame begins with one whole-state clone under the lock, and that clone is a derived
implementation rather than a hand-copied field list, because the hand-copied version
silently dropped any field added later.

**The shared-state lock is poison-recovering, and the reason is precise**: it is read as
the first statement of every frame, so one panic on a background thread would otherwise
wedge the window blank for the rest of the process's life, every frame, forever.

## Panes come from the core taxonomy

The sidebar iterates the capability groups rather than a hardcoded list, so adding a
capability propagates without touching the app. Scope routing falls back to the overview
for a scope that no longer resolves.

Scope is restored from persisted state **before the first paint**, deliberately: detection
is slow but reading the state file is not, and defaulting until detection finished made
the launch frame show settings the user never chose.

A single background thread drives everything, polling faster while jobs run, with an
early-wake path. It repaints both before and after each pass so the freshness indicator
reflects a pass that is actually running.

## Jobs, and two-step destruction

Jobs are supervised in-process with full output capture, one per capability at a time, and
a command chain that stops at the first failure. Records persist to a file so the menu-bar
helper sees activity with no IPC.

**Destructive actions cannot be run directly by a row.** A row can only arm a modal; only
the modal's confirmation emits the action. Bulk install intersects the live candidate set
at confirm time, so a stale selection cannot install the wrong thing, and a
manual-install capability can never enter the selection at all.

Insights are stored unfiltered and filtered at render time, matching the existing
enabled/disabled split, and the dismissal set updates immediately so a click never waits
for the next poll.

## Honest limits

**Detection failure is not reportable at pass level.** The overview's failure state is
hardcoded to none, with a comment explaining why: detection is total, probe failures
become findings on their capability, so the engine has no pass-level failure to report.
The renderer exists and is snapshot-tested, but the running app cannot currently enter
that state.

A timed-out probe subprocess is **not killed** — the timeout abandons its thread while the
child runs to completion off-screen. Job logs truncate silently from the front and the
activity view shows only a tail of what survives. Timestamps past a day are shown in UTC
rather than local, stated as a deliberate honesty choice given no date dependency in the
workspace. Hook latency is measured at most once per project per session and can be
arbitrarily stale within it. Project reports are session-scoped, so coverage counts only
what this session inspected.

The crate is excluded from the coverage gate on the stated ground that every decision it
displays is computed in the shared core, which is covered. Its tests still run; they just
do not gate.

## Source map and tests

`crates/ade-control-center/src/`. The load-bearing test is
`a_detection_pass_refreshes_shared_jobs_mid_run`, which gates a fake subprocess on a
channel so a job is provably in flight, then asserts the running job and its captured log
line are visible in shared state. Also
`uninstall_goes_through_the_coverage_consequence_modal`,
`the_bulk_preview_starts_exactly_the_selected_installs`,
`cancelling_the_confirm_modal_changes_nothing`,
`a_row_reports_what_was_clicked_and_changes_nothing_itself`, and
`matrix_cell_classifies_exactly_like_ade_status_prints_the_same_row`.
