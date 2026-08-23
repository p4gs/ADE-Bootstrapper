---
type: Architecture Guide
title: Metrics and insights
description: A two-tier stats model with a hard no-fabrication rule, and the shared functions that stop the desktop matrix and the CLI from ever disagreeing.
tags: [stats, insights, tiers, honesty]
sources:
  - id: openwiki-source-b14a375c20d875c8ccf41b67
    resource: repo://crates/ade-core/src/gui/insights.rs
  - id: openwiki-source-a32a65230b52903b572cd3bb
    resource: repo://crates/ade-core/src/gui/posture.rs
  - id: openwiki-source-061e523422fcc89abd98a9e0
    resource: repo://crates/ade-core/src/gui/stats.rs
generated: {by: "claude-code", at: "2026-08-22T21:06:03.708Z"}
verified:
  - by: openwiki/0.3.3
    at: 2026-08-22T21:06:03.708Z
---

# Metrics and insights

Rust-only; no TypeScript counterpart. The house rule is stated in the module header and
enforced by return type: **every function returns an optional when its real source is
absent — there is no placeholder-zero path anywhere in the file.**

Tier 1 is facts already on disk: job history and per-project status reports, with no new
probing. Tier 2 is a small set of narrowly-scoped probes, each documenting the exact
command or file it was verified against. A seventh candidate was considered and
deliberately excluded because it would have required inventing an association the codebase
does not itself make.

## Why the CLI and the desktop cannot disagree

Five capability groups map to modules, and **every mapping is grounded in that module's
own tool-presence check** rather than assumed. The state predicate accepts only applied
and degraded — never registered, never disabled.

The desktop coverage matrix, the headless posture report, and `ade status` all call the
identical two functions on the identical status value the CLI prints. That is a
structural guarantee rather than a convention, and it is asserted by a cross-surface test.

The matrix distinguishes four honest outcomes including *unknown* for "not inspected yet",
because reports are populated on demand rather than eagerly. Coverage is likewise scoped
to the projects actually looked at, never padded to all registered projects.

## Probes that refuse to fabricate

Each names a verified source. The database-age probe reports the **oldest** ecosystem
archive rather than an average, because a fresh archive beside a month-old one means that
ecosystem's advisories are a month stale. The index-staleness probe compares a database
timestamp against the last commit, because the tool prints no timestamp at all.

The hook-latency probe **refuses to report a timeout as a measurement**: it checks for the
timeout sentinel and returns nothing, because the elapsed time in that case is the timeout
rather than the hook's cost.

The coding-agent surface probe deliberately omits two agents from the denominator rather
than counting them as uncovered, because no vendor-documented install path was found and
guessing one is exactly the fabrication the house rule forbids.

Tier-3 secret-scanning statistics read a deliberately counts-only log; retaining the
scanner's raw output would turn a catching tool into a secrets-retention bug. Malformed
lines are skipped rather than failing, because the log is appended from a shell hook and
a torn write must not blank the metric.

## Insights are rules over facts, not new probes

The insight layer reads no probe directly — it consumes the same facts the metrics strip
renders, in the same pass, so **an insight can never claim a fact the strip does not
already show**.

"At most one action per insight" is enforced by the type rather than by a runtime check.
Identifiers are rule-plus-group tags, never random and never value-derived, so dismissing
a stale-database insight keeps suppressing that rule for that group even as the age
changes on every poll. Every insight carries an evidence string naming the probe and field
it came from — not rendered as prose, existing as proof.

Rules fire only on real gaps and stay silent on the already-stated: a fully covered group
produces no coverage insight, and the token-gain affirmation is gated on a nonzero command
count so a freshly installed tool cannot emit "saved zero tokens across zero commands".

Rule output is sorted deterministically because the real caller builds its input from
unordered map iteration, and the function is unbounded — the overview's top-five is a
presentation slice, not this layer's decision.

## Honest limits

The cache-directory probe does not honor the underlying tool's own override variable, so
on a machine using it the age reads absent. The latency probe executes the project's real
pre-commit hook as a side-effecting subprocess, bounded but not sandboxed, and it is a
single sample rather than a distribution. Dismissed identifiers are never validated
against a live rule table, so a retired rule's dismissal simply never matches again.

## Source map and tests

`crates/ade-core/src/gui/{stats,insights}.rs`. Focused tests:
`last_group_job_ignores_other_groups_and_running_jobs_and_picks_the_newest`,
`module_for_group_covers_the_five_grounded_mappings_and_nothing_else`,
`osv_db_age_reports_the_oldest_ecosystem_archive`,
`time_hook_run_reports_absent_rather_than_a_fabricated_duration_on_timeout`,
`secrets_log_parses_counts_and_skips_torn_lines`,
`build_insights_is_order_independent_and_actionable_first`, and the cross-surface
`matrix_cell_classifies_exactly_like_ade_status_prints_the_same_row`.
