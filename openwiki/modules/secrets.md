---
type: Architecture Guide
title: Secrets and credential hygiene
description: The scan-nothing invocation this project found and enforces against, how an existing git hook is chained rather than destroyed, and where the guarantee actually stops.
tags: [secrets, trufflehog, pre-commit, git-hooks]
sources:
  - id: openwiki-source-8483ecf5b49cab9ff96f9ca3
    resource: repo://crates/ade-core/src/lockfile.rs
  - id: openwiki-source-e4a8b9d009a1a5f32f017be1
    resource: repo://crates/ade-core/src/modules/secrets.rs
generated: {by: "claude-code", at: "2026-08-22T21:06:03.708Z"}
verified:
  - by: openwiki/0.3.3
    at: 2026-08-22T21:06:03.708Z
---

# Secrets and credential hygiene

The most valuable thing this module knows is a negative result, recorded in its header as
live-probe-verified: **the commonly-documented `trufflehog git file://. --since-commit
HEAD` invocation scans the committed range, which is empty at pre-commit time. It blocks
nothing.**

ADE materializes the index into a temporary snapshot and scans that instead.

The finding is enforced rather than merely avoided. Adopting a repository whose existing
pre-commit configuration uses that invocation produces an error from apply and fails
verify, and ADE never rewrites the user's file. The reasoning is stated plainly: a
scanner configured to find nothing is the same failure class as a suppression. A
rot-guard covers ADE's own shim too, so an old version's hook fails verify.

## Two mutually exclusive install paths

The branch is keyed solely on whether the pre-commit framework is present. With it, ADE
writes a configuration file and installs **no** git hook. Without it, ADE installs a
native shim.

**On the framework path, ADE never runs the framework's install step, so writing the
configuration alone installs no git hook.** Verify on that path only checks the file for
the broken invocation; it never asserts a hook exists. A repository can therefore pass
`ade verify` with zero commit-boundary scanning. This is the single most important
caveat on the page — and it is exactly what this repository hit when it was bootstrapped.

The configuration file is written on exactly one condition: framework present and no file
existing. Every other case leaves the user's file untouched, with a finding that names
what it found.

## Chaining, not destroying

A pre-existing hook lacking ADE's marker is renamed aside, and the shim runs it first and
propagates its exit code before scanning. Re-running apply refreshes the shim without
re-chaining. On [removal](../engine/removal.md), the chained hook is renamed back rather
than orphaned.

## Where the shim fails open

It fails **closed** only on temporary-directory creation failure, blocking the commit
with an explicit message. If materializing the index fails, the scanner runs over an
empty or partial directory, exits zero, and the commit proceeds. That path is unguarded.

It also warns and passes when the scanner is absent, so it never bricks commits, and apply
returns degraded rather than failed.

The retained log is counts-only by design — keeping the scanner's raw JSON would turn a
catching tool into a secrets-retention bug — and metric failure is swallowed so it can
never block a commit.

## What is not covered

None of the three git-boundary artifacts are hash-protected: the hook, the pre-commit
configuration and the ignore file all sit outside the ADE-owned tree, so their only
integrity check is marker and content inspection in verify.

The index snapshot materializes the **entire** index rather than only changed paths, so
the scan is broader than the policy's staged-changes label suggests.

One derived interaction worth knowing, with no test covering it: the shim writes its
metric log under the ADE-owned tree but outside the audit exclusion, so the
[lockfile](../engine/lockfile-and-verify.md)'s unknown-file rule will flag it on the
first verify after the first native-shim commit, until the next apply or lock.

## Source map and tests

`crates/ade-core/src/modules/secrets.rs`. Focused tests:
`trufflehog_present_applied_with_native_hook_installed`,
`pre_commit_framework_present_writes_config_with_trufflehog_hook`,
`existing_user_pre_commit_config_without_trufflehog_is_not_modified`,
`pre_existing_non_ade_git_hook_is_chained_not_destroyed`,
`broken_scanner_remediation_since_commit_invocations_flagged_as_errors`,
`planted_env_secret_never_appears_in_any_generated_artifact`, and
`hook_script_matches_oracle_bytes`.
