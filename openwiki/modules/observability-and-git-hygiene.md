---
type: Architecture Guide
title: Observability and git hygiene
description: Where the audit chain gets initialized and wired, and a module that writes a git contract it cannot enforce.
tags: [observability, audit, git, hooks]
sources:
  - id: openwiki-source-0c51efc29b827ca07318fbec
    resource: repo://crates/ade-core/src/modules/git_hygiene.rs
  - id: openwiki-source-4163de8a5fe0298bc5e60732
    resource: repo://crates/ade-core/src/modules/observability.rs
generated: {by: "claude-code", at: "2026-08-22T21:06:03.708Z"}
verified:
  - by: openwiki/0.3.3
    at: 2026-08-22T21:06:03.708Z
---

# Observability and git hygiene

## Observability

This module performs the [audit chain](../engine/audit-chain.md)'s genesis
initialization, and only when the log is absent. The write goes **directly to disk**
rather than through the recorded artifact writer, deliberately, because the log is
mutable runtime state that must stay outside lockfile scope. The whole audit directory is
excluded from hashing; chain integrity is committed as a checkpoint in the lockfile
instead.

The shipped hook keeps a TypeScript filename but is POSIX shell — a documented divergence
from the oracle. It is wired only when claude-code is a configured target, uses a
wildcard matcher, and is idempotent through array deduplication rather than replacement.
With no hook-capable agent configured, the chain records pipeline events only, and the
module says so.

Hook-appended entries are redacted and capped at the source before they are ever written.

**The module's own verify is weaker than the pipeline's**: it checks the chain without a
checkpoint, so it would accept a truncated log. Only the full verify pipeline and the
audit command pass the lockfile's checkpoint. Worth knowing when testing tamper
detection.

The audit directory is git-ignored by default, and the only escape is an option that must
be exactly boolean true.

## Git hygiene

This module writes a **fully hardcoded contract with no option surface at all**: the
protected branches are literally `main` and `master`, force-push and history rewrite are
denied, pull requests are required, and commit style is conventional.

**It reports; it does not enforce.** It installs no git hook, sets no git configuration,
and configures no server-side branch protection. It cannot stop a force push and does not
read the actual remote's protection settings. Enforcement beyond the written contract is
explicitly delegated to an external tool as an integration.

Commit-signing posture is observed by reading git configuration, is informational when
off, and is checked **only in detect, never in verify** — so a repository with signing
disabled never fails verification.

The module degrades rather than fails on a non-git target or a missing integration tool,
and writes the policy first either way. Its verify re-derives only two invariants: that
protected branches is a non-empty array and that force-push is denied. The pull-request
requirement, history-rewrite rule, branch naming and commit style are written but never
re-verified.

A repository whose default branch is neither `main` nor `master` is not covered by the
written contract at all.

## Source map and tests

`crates/ade-core/src/modules/{observability,git_hygiene}.rs`. Focused tests:
`apply_writes_audit_readme_and_genesis_initializes_the_chain`,
`claude_code_not_targeted_no_hook_script_no_settings_touched`,
`preexisting_user_hooks_preserved_by_the_additive_merge`,
`hook_shim_exits_0_and_appends_nothing_when_ade_is_absent`,
`verify_detects_a_tampered_audit_chain`,
`non_git_target_apply_degrades_never_failed_with_git_init_guidance`, and
`generated_git_json_is_deterministic_no_timestamps_env_values_or_absolute_paths`.
