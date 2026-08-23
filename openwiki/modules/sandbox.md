---
type: Architecture Guide
title: Sandboxing
description: A declarative policy that says honestly whether anything enforces it, and the one place ADE maps its own contract onto a coding agent's real permission system.
tags: [sandbox, permissions, network, credentials]
sources:
  - id: openwiki-source-512c6f1fe64cfb7e79b3c37b
    resource: repo://crates/ade-core/src/modules/sandbox.rs
generated: {by: "claude-code", at: "2026-08-22T21:06:03.708Z"}
verified:
  - by: openwiki/0.3.3
    at: 2026-08-22T21:06:03.708Z
---

# Sandboxing

The module writes one always-present artifact, `.ade/policy/sandbox.json`, and
conditionally merges a second file it does not own.

## The honest field

`enforcement` has exactly two values: the enforcer's name when the tool is present, and
`advisory` otherwise. It is the only field in the whole policy that varies with the
machine, and it exists so the artifact cannot pretend something is enforcing it when
nothing is. Without the enforcer the module reports `Degraded` rather than claiming
success.

Policy paths are placeholders rather than machine-absolute paths, with its own test
asserting no absolute home path appears in the file. That is what keeps the artifact
portable and the lockfile stable across machines.

Network policy is deny-by-default with a five-host allowlist — the four package
registries plus the source forge, one host wider than
[supply chain](supply-chain.md)'s, because verification needs source access.

Three list options extend rather than replace the built-in lists. Extension copies the
base in order and appends only entries not already present, so defaults can never be
displaced and output stays deterministic.

## The one piece of real enforcement

On the claude-code path, the deny-read surface is mapped into that agent's own
permission system, so the agent itself refuses the read. Everywhere else this module is
declarative only.

The merge is additive-union and refuses to clobber an unparseable user file: a parse
error or a non-object root returns an error finding and writes nothing. A refused merge
degrades the module rather than failing it, and the settings path is excluded from the
reported written paths in that case.

Two independent conditions produce `Degraded` and they compose: a failed merge, and an
absent enforcer.

## What verify does and does not watch

`verify` requires the network default to still be deny with an array allowlist, and —
only when claude-code is targeted — that every deny-read entry is still present, naming
the missing ones.

It does **not** structurally re-check the filesystem or credentials sections, so a policy
with those gutted still passes this module's verify and is caught only by the lockfile
hash. And because the settings file lives outside the ADE-owned tree, this module's
verify is the only thing watching it at all.

Note that plan, apply and verify all key on the *configured* agent list rather than
detected agents. A repository with an instruction file present but the agent absent from
`ade.json` gets no permission wiring and no permission verification.

The instruction block's operative clause is not a prohibition but an escalation rule: if
a task needs access outside this policy, stop and ask a human rather than working around
the sandbox.

## Source map and tests

`crates/ade-core/src/modules/sandbox.rs`. Focused tests:
`policy_contains_no_absolute_machine_paths`,
`nono_absent_degraded_but_policy_still_written`,
`pre_existing_user_deny_entry_is_preserved_array_union`,
`options_allow_hosts_extends_the_network_allowlist_without_displacing_defaults`,
`apply_refuses_to_clobber_unparseable_user_settings_and_degrades`, and
`verify_fails_when_policy_missing_or_a_claude_deny_entry_is_removed`.
