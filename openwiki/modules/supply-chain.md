---
type: Architecture Guide
title: Supply chain security
description: One policy artifact, a hardcoded registry allowlist, AI-native dependencies as first-class governed classes, and a lockfile heuristic worth knowing.
tags: [supply-chain, dependencies, osv, slopsquatting]
sources:
  - id: openwiki-source-cf344a1dea6f0dd5e50e1126
    resource: repo://crates/ade-core/src/modules/supply_chain.rs
generated: {by: "claude-code", at: "2026-08-22T21:06:03.708Z"}
verified:
  - by: openwiki/0.3.3
    at: 2026-08-22T21:06:03.708Z
---

# Supply chain security

The module owns exactly one artifact, `.ade/policy/dependencies.json`, written through
the deterministic policy writer so it is key-sorted and byte-stable.

The registry allowlist is a hardcoded four-host constant with **no option to extend it**.
Unlike the [sandbox](sandbox.md)'s network allowlist, there is no extension path here.
The only configurable field is a minimum package age, defaulting to fourteen days; a
non-numeric or negative value fails apply before anything is written.

## AI-native dependencies are first-class

Five classes are governed with identical rules — skills, plugins, MCP servers,
instruction packs and agent configs — each requiring review before install, pinned
versions, and explicit user approval as the only allowed source. This is the spec's
"AI-native dependencies treated as untrusted code" rendered as machine-readable policy.

## Lockfile checking is a probe, not a gate

A filesystem probe covers four ecosystems and produces findings only for manifests that
actually exist. **Missing lockfiles are warnings and never fatal** — they appear in
apply's findings and never affect module status or verify.

Two honest limits. The Python pinning check is substring-based: a `requirements.txt`
counts as pinned if `==` appears anywhere in it, including inside a comment. And the
probe looks only at the repository root, so a monorepo with nested manifests is invisible
to it.

## Enforcement absence never blocks policy publication

An absent scanner yields `Degraded` rather than `Failed`, and the policy is still
written. Apply never invokes the scanner; it only records presence.

`verify` performs a two-section structural check and fails closed on either: the
traditional section needs a non-empty allowlist, a numeric age, lockfiles required, and
string install-review and typosquatting fields; the AI-native section needs all five
kinds present with pinning required. It deliberately does **not** re-check lockfiles or
scanner presence, so a repository that acquires a manifest after apply verifies clean.

The instruction block leads with the slopsquatting defense: never install a dependency
without checking the policy first, verify the exact name against the source repository,
and treat AI-suggested package names as hallucination-prone until verified. All of that
is a directive to the coding agent, enforced only by its compliance.

## Source map and tests

`crates/ade-core/src/modules/supply_chain.rs`. Focused tests:
`apply_writes_dependency_policy_with_all_traditional_requirements`,
`malformed_min_age_days_apply_fails_nothing_written`,
`policy_has_first_class_ai_native_dependencies_section`,
`requirements_txt_counts_as_pinned_only_with_double_equals`,
`no_manifests_no_lockfile_findings_at_all`, and
`verify_passes_after_apply_and_fails_on_each_tampering_class`.
