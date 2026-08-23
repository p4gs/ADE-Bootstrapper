---
type: Architecture Guide
title: Secure-coding guardrails
description: Seven rule files with machine-checkable frontmatter, why verify re-parses them, and the vendoring seam that collides with the lockfile.
tags: [guardrails, rules, frontmatter, codeguard]
sources:
  - id: openwiki-source-8483ecf5b49cab9ff96f9ca3
    resource: repo://crates/ade-core/src/lockfile.rs
  - id: openwiki-source-206a6c0d6c2287f8df7f38af
    resource: repo://crates/ade-core/src/modules/guardrails.rs
generated: {by: "claude-code", at: "2026-08-22T21:06:03.708Z"}
verified:
  - by: openwiki/0.3.3
    at: 2026-08-22T21:06:03.708Z
---

# Secure-coding guardrails

The module emits seven rule files into `.ade/guardrails/`, sorted by name, each a static
template with no interpolation of machine state. The seven and their severities are fixed
in code: injection, secrets-handling and authn-authz are critical; input-validation,
crypto and dependencies are high; error-handling is medium.

The severity string is only ever validated, never acted upon by ADE. It is guidance for
the coding agent reading the file.

## Frontmatter is the tamper detector

Every rule carries machine-checkable frontmatter, and `verify` re-parses it. A rule must
have a leading and terminating delimiter, a kebab-case id, a severity from the
three-member allowlist, and a non-empty `applies_to` array. `verify` additionally
enforces that the frontmatter id equals the file's basename, which closes the gap where
a valid-but-mislabelled rule could be swapped in.

This sits on top of the lockfile hash rather than replacing it. Content tampering inside
a rule body — which frontmatter parsing would not catch — is caught by the hash. Between
the two, both the structure and the bytes are pinned.

`verify` does not short-circuit: it reports one finding per file and names precisely
which rules are broken.

## Apply cannot degrade

There is no external tool to detect, so the only failure mode is I/O. `detect` is purely
informational and deliberately does **not** degrade when no CodeGuard binary exists,
because Project CodeGuard is a ruleset framework rather than a CLI. A missing binary is
not a capability gap.

## The vendoring seam, and its collision

Vendoring CodeGuard's full rules into the same directory is the documented extension
path, and the binding instruction block covers whatever is in the directory. But
[verify](../engine/lockfile-and-verify.md)'s unknown-file rule errors on any file under
the ADE-owned tree the lockfile does not know.

The intended sequence is therefore vendor, then `ade lock` to adopt the addition
deliberately. Dropping files in without re-locking fails verify. That is the interaction
to remember.

## What is not guaranteed

ADE does not enforce these rules. It guarantees the files are present, well-formed and
unmodified. There is no scanner, no glob matcher for `applies_to`, and no
severity-driven behavior anywhere in the codebase. One cosmetic wrinkle: the
`dependencies` rule ships as a file but appears only parenthetically in the instruction
block's enumeration, so a reader sees six ids for seven files.

The instruction block itself is uncompromising, forbidding suppression outright: security
findings are fixed at code level, never suppressed through scanner exclusions, lint
suppressions, or severity downgrades, with an escalate-on-conflict clause.

## Source map and tests

`crates/ade-core/src/modules/guardrails.rs`. Focused tests, all in-file:
`apply_writes_required_rule_files_with_real_do_dont_content`,
`every_rule_file_starts_with_parseable_frontmatter`,
`parse_rule_frontmatter_rejects_malformed_frontmatter` (seven negative cases),
`verify_fails_when_a_rule_file_is_deleted`,
`verify_fails_when_frontmatter_is_tampered_invalid_severity`,
`verify_fails_when_frontmatter_id_diverges_from_file_name`, and
`determinism_no_timestamps_or_absolute_paths_in_generated_rules`.
