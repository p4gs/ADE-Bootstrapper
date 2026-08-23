---
type: Architecture Guide
title: Codebase context management
description: A two-tier model — an always-present structural codemap plus detected engines — and the honest limits of both, including a real defect this repository demonstrates.
tags: [context, codemap, openwiki, cocoindex]
sources:
  - id: openwiki-source-b8c2c363fa9dbf140ad375ad
    resource: repo://crates/ade-core/src/modules/context_mgmt.rs
generated: {by: "claude-code", at: "2026-08-22T21:06:03.708Z"}
verified:
  - by: openwiki/0.3.3
    at: 2026-08-22T21:06:03.708Z
---

# Codebase context management

Two tiers with different guarantees. The codemap is unconditionally re-derived on every
apply. The engines policy only *records* what is installed — it never installs anything.

## The convention: detect and wire, never install

Stated in the module header and realized as install-guidance strings carried inside the
policy itself. `enabled` for each engine is derived from **binary presence**, never from
repository contents: the wiki directory name is a convention string, and the module does
not check whether that directory exists.

CocoIndex counts as present if either the framework or the CLI resolves, first match
winning. The Personal Brain requires a conjunction — the tool present **and** an explicit
opt-in — because that capability reaches outside the repository. The opt-in flag is
recorded separately so "opted in but tool absent" stays distinguishable.

## Codemap determinism

Four mechanisms: a sorted file list, ordered-map accumulation, repo-relative paths only,
and no timestamps. Two identical trees at different absolute paths produce byte-identical
codemaps.

The skipped-files list exists to defeat **apply-order drift**, not to reduce noise. Those
root files are generated at different points of the pipeline, so including them would
make the codemap differ between a first init and a re-apply. The pre-commit config is on
that list precisely because the secrets module runs *after* this one.

## Two honest limits, one of them a live defect

**The codemap is not "always current" — it is current as of the last apply.** Nothing
watches the tree.

**Build output is not excluded, and it swamps the result.** The skipped-directory list
does not contain `target/`. In this repository the generated codemap reports 136,831
files, of which 117,667 are `.o` object files, and the entire top-ten extension table is
Rust build artifacts — `.o`, `.d`, `.rmeta`, `.rlib`, `.dylib`. Not one source language
appears. A document whose stated purpose is "consult it before scanning the tree"
therefore conveys nothing about the source tree in any Rust project that has been built.
Verified against this repository's own generated codemap.

Beyond that, the codemap carries no semantic content — directories, extension counts and
entry points only, with no symbols, imports or call graph. The Cargo entry-point scanner
is a line scanner rather than a parser, so it handles neither inline tables nor
workspace member manifests.

## What verify actually gates

A three-gate chain with early returns: codemap presence, then all five section markers
plus the exact refresh sentence, then engines-policy validity, then engine-state drift.

Drift detection compares exactly three booleans — the three engines' `enabled` values —
against a freshly rebuilt policy, not the whole document. So installing an engine after
apply correctly fails verify, while a changed version string alone does not.

**Verify cannot detect a stale codemap.** The marker and sentence checks are structural;
a codemap describing a tree that no longer exists passes green.

Absent engines produce degraded or informational findings carrying the exact install
command, never errors, so an engine-free machine still verifies clean.

The instruction block encodes a priority order — wiki, then semantic search, then codemap,
then brain — and one prohibition: never write secrets into the Personal Brain.

## Source map and tests

`crates/ade-core/src/modules/context_mgmt.rs`. Focused tests:
`codemap_generation_is_deterministic_across_identical_trees`,
`skipped_dirs_are_excluded_from_scan_and_counts`,
`build_codemap_extension_table_capped_at_top_10_count_desc_then_ext_asc`,
`cocoindex_present_via_ccc_cli_alias_either_binary_counts`,
`personal_brain_opt_in_with_openwiki_enabled_without_openwiki_not_enabled`, and
`verify_fails_when_recorded_engine_state_drifts_from_live_machine`.
