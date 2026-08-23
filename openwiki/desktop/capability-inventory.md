---
type: Architecture Guide
title: Capability inventory and health verdict
description: A fixed capability table, two-phase detection that has to be two-phase, and coverage-aware severity that distinguishes a spare tyre from a hole in the floor.
tags: [capabilities, detection, coverage, verdict]
sources:
  - id: openwiki-source-7358428d422bd02e17be812e
    resource: repo://crates/ade-core/src/gui/inventory.rs
  - id: openwiki-source-8a1b8daaaa14442124a56768
    resource: repo://crates/ade-core/src/gui/verdict.rs
generated: {by: "claude-code", at: "2026-08-22T21:06:03.708Z"}
verified:
  - by: openwiki/0.3.3
    at: 2026-08-22T21:06:03.708Z
---

# Capability inventory and health verdict

Rust-only; no TypeScript counterpart exists. Every test cited is an in-file test module —
the desktop crate's `tests/` directory contains only snapshot images.

The capability table and the group taxonomy are **fixed-size arrays rather than growable
vectors**, so adding a capability is a compile-time edit that forces the arity to be
updated. No request or UI input ever reaches a subprocess argument list: identifiers
resolve against the static table, and actions against a closed four-variant enum.

## An empty version-argument list is load-bearing

It means "this tool has no version command at all", not "we forgot". It is used in three
places: the probe is skipped, the capability is marked as not reporting a version, and the
health check treats it as healthy anyway. One real tool is in that state.

Presence is resolved by path lookup over candidate binary names, first hit winning —
except for the plugin-shaped capability, which never lands on the path and is resolved by
asking the coding agent itself.

**A plugin that is installed but switched off is worse than absent**, and the code says
so: it becomes an error rather than a warning and is excluded from coverage, because
"installed, reports a version, protecting nothing" is the more dangerous state. When the
enabled flag is missing entirely it defaults to off, because absence cannot show the
thing is live.

## Detection is genuinely two-phase

Probes fan out across scoped threads, **then** coverage is computed across the whole
taxonomy, **then** each capability is judged against it. The split exists because no
single capability can know whether its group is already covered by something else.

That is what makes severity coverage-aware: a missing provider whose group already has a
working one is informational — a spare tyre — while a missing provider in an uncovered
group is a warning, a hole in the floor. The informational variant names the providers
that do cover it.

One definition of "works" serves both entry points — from raw facts during detection, and
from finished statuses during verdict and the removal dialog — and a test asserts the two
produce identical output.

## The verdict is pure and total

An empty input yields an unknown verdict with empty detail, never a sound one. First paint
cannot masquerade as a clean bill of health.

Attention items sort by an ordinal that *is* the enum discriminant, and the action-needed
predicate deliberately excludes spares and available updates, so the overview lists only
broken and uncovered while capability pages show everything.

"Broken" requires installed **and** erroring: a tool that never arrived cannot be broken,
so its install failures ride on the gap row as a note rather than being counted twice. An
uncovered group emits exactly one attention row rather than one per candidate provider.

## Bounded, and honest about it

Every probe is wrapped in a timeout that returns a synthetic exit code with a
"probe timed out" message — a sentinel consumed by name elsewhere in the tree, so it is a
contract rather than an implementation detail. The timeout **abandons** the subprocess
rather than killing it, so a hung installer keeps running after the probe gives up.

Update availability is a substring comparison rather than a semantic version comparison,
and latest versions are never auto-populated — they arrive only from an explicit refresh,
so update rows are absent by default. And a panicking probe thread aborts the whole
detection pass in the core; only the menu-bar helper wraps the call defensively.

One documentation drift worth knowing before editing: a comment describes the coverage
rows as nine, one per capability, while the taxonomy holds ten and the loop iterates all
of them. The comment is stale; the code is not.

## Source map and tests

`crates/ade-core/src/gui/{inventory,verdict}.rs`. Focused tests:
`a_missing_provider_only_warns_when_its_capability_has_no_cover`,
`a_plugin_that_is_installed_but_switched_off_protects_nothing_and_says_so`,
`a_tool_with_no_version_command_is_working_not_broken`,
`coverage_is_computed_the_same_way_from_facts_and_from_statuses`,
`an_empty_slice_is_unknown_not_sound`,
`a_failed_install_of_an_absent_tool_is_one_problem_not_two`, and
`with_timeout_bounds_hung_probes`.
