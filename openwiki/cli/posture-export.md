---
type: Architecture Guide
title: Posture export
description: One collector serving both the CLI and the Control Center, the home-directory redaction guarantee, and what byte-determinism modulo timestamp actually means.
tags: [posture, evidence, determinism, redaction]
sources:
  - id: openwiki-source-a32a65230b52903b572cd3bb
    resource: repo://crates/ade-core/src/gui/posture.rs
generated: {by: "claude-code", at: "2026-08-22T21:06:03.708Z"}
verified:
  - by: openwiki/0.3.3
    at: 2026-08-22T21:06:03.708Z
---

# Posture export

`ade export posture` writes a deterministic markdown and JSON evidence pair under the
machine-level export directory. The interesting property is not the output format; it
is that the desktop Control Center's export button and this command are the **same
collector**.

This surface is Rust-only, with no TypeScript counterpart.

## One collector, two callers

Both the CLI arm and the Control Center construct their own dependency struct and call
`collect_posture`, then render markdown and JSON in the same order. The Control Center
deliberately does **not** serialize its cached state — its comment states the button
performs a fresh, full re-detection rather than a dump of whatever happens to be
cached. That is what makes "the CLI and the app cannot disagree" a real guarantee
rather than a nominal one.

The two callers differ in exactly one field: an injectable clock, and both pass none.
The seam exists only for tests.

`collect_posture` fabricates nothing. It re-runs the same capability detection and
verdict pass as `ade gui health`, the same status report as `ade status`, and reads job
history from the machine-level job file. Every number in the report is re-derived from
an existing subsystem.

## Redaction, precisely

The home directory is redacted at exactly two places inside the collector: the project
path, and the config error string. The second matters because the config loader embeds
the target directory **inline in a sentence** rather than as a separate field.

`redact_home` is a plain substring replace with two guards: it is a no-op when there is
no home to redact against, and a no-op when the home string is empty, so an empty
environment variable can never rewrite every path to a tilde. An unset home is a
documented, deliberate degradation — paths render absolute rather than guessed.

**What is not covered is worth stating.** Redaction does not canonicalize: a project
reached through a symlink, a different spelling of the same directory, or a trailing
slash will not match and renders raw. And only the project path and config error are
redacted — capability version strings, job summaries and verdict prose pass through
untouched, so a tool that printed a home path in its own version output would leak it.

## Determinism, and its one moving part

Byte-determinism is achieved by fixing every collection's order **before** rendering
rather than sorting at render time. Capabilities follow declaration order even though
detection runs in parallel threads, coverage and job history follow the taxonomy order,
issues follow a stable sort, and projects get an explicit sort by path so ordering never
depends on registration time. JSON then goes through the recursive key-sorting
serializer, so struct field order cannot leak into the bytes either.

The generated timestamp is the single intended source of run-to-run difference, which
is why the contract is determinism *modulo timestamp*. The end-to-end test strips the
timestamp line and the timestamp key and compares the remainder, and it includes a
positive control asserting the timestamp exists at all.

## Bounded probes, silent omissions

Every external probe is time-bounded — ten seconds for capability probes, five for the
token-gain probe — and a timeout returns a nonzero code that fails the same gate an
absent tool fails. **A timeout is therefore indistinguishable from absence in the
output.** The token-efficiency line is triple-gated and silently omitted otherwise, so
a freshly installed tool never renders a meaningless zero.

Two further honest limits. The tier-3 status section is hardcoded prose encoding a
researched finding, not runtime-derived, so it will go stale silently if the underlying
module changes what it invokes. And a corrupt machine-state file is invisible in the
export: the loader returns a warning alongside defaulted state, but the collector reads
only the state, so a silently-defaulted machine renders as a legitimately empty posture.

Finally, the two writes are joined by a short-circuiting conjunction, so a failed
markdown write means the JSON file is never attempted and the previous one is left
stale rather than paired.

## Source map and tests

`crates/ade-core/src/gui/posture.rs` and the export arm of `crates/ade/src/main.rs`.
Integration tests in `crates/ade/tests/cli.rs`:
`export_posture_writes_both_files_and_json_names_their_real_paths`,
`export_posture_is_byte_deterministic_across_two_runs_modulo_timestamp`, and
`export_posture_never_leaks_the_real_home_directory_end_to_end`. In-file:
`redact_home_replaces_every_occurrence_and_is_a_noop_without_a_match`,
`collect_posture_is_byte_deterministic_for_identical_state` (with a third run moving
only the clock, as the control that makes the equality meaningful),
`a_bare_machine_with_nothing_registered_fabricates_nothing`, and
`an_unbootstrapped_registered_project_reports_a_redacted_error_not_a_crash`.

The Control Center's own export arm has no test; the shared-collector guarantee is
enforced by construction rather than by an assertion that the two outputs match.
