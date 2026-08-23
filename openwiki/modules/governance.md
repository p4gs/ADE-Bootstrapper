---
type: Architecture Guide
title: Governance modules
description: Configuration governance, approval gates and cost governance — three declarative contracts, and an honest account of how little of it is mechanically enforced.
tags: [governance, approvals, budget, drift]
sources:
  - id: openwiki-source-982342659b3afdbfd8dfc07c
    resource: repo://crates/ade-core/src/modules/approval_gates.rs
  - id: openwiki-source-fcfab0395886342324d1e2af
    resource: repo://crates/ade-core/src/modules/config_governance.rs
  - id: openwiki-source-a9528310de9f530f07066c6f
    resource: repo://crates/ade-core/src/modules/cost_governance.rs
generated: {by: "claude-code", at: "2026-08-22T21:06:03.708Z"}
verified:
  - by: openwiki/0.3.3
    at: 2026-08-22T21:06:03.708Z
---

# Governance modules

Three modules share this page: config-governance, approval-gates and cost-governance.
What unites them is that each writes a contract, and each is much weaker as an enforcer
than its policy file reads.

## Configuration governance

It writes one artifact and **never performs the translation it governs** — the rendering
of managed blocks is core-owned in the apply pipeline. Its managed-file list is derived
from the configured agents rather than from disk, deduplicated by path and sorted, which
is what makes the artifact byte-stable.

Its `verify` checks marker **presence, not content**. It requires the canonical
instruction file to exist and each target to yield an extractable block, but never
compares the block against the canonical rendering. Content drift is detected one layer
up, by the pipeline's own drift check. So this module passing does not mean your managed
blocks are in sync.

The "hand-edits are refused" guarantee its policy advertises is implemented in the
[managed-block engine](../engine/instructions-and-managed-blocks.md), not here.

## Approval gates

Eight action classes, three of which can never be set to allow: destructive shell,
credential use, and production-affecting. That prohibition is enforced at the writer, and
an escalation attempt makes apply return `Failed` with zero writes — including no
settings merge.

Secure defaults are seven ask plus production-affecting deny.

**The mapping onto a real permission system is narrow, and this is the most important
thing on the page.** The coding-agent projection is nine literal shell-command patterns
covering destructive removal, force push, sudo removal, dependency installs and branch
deletion. There is **no** rule for credential use, external network, PR creation, merge,
or production-affecting — notably, the one class the policy *denies* has no corresponding
deny rule at all.

So approval gates do not block anything by themselves. The policy is a contract read by
humans and by instruction blocks; the only mechanical enforcement is those nine patterns,
in one agent. They are literal prefixes, so they do not catch an equivalent command run
through a wrapper or a different tool.

`verify` is stronger than apply's surface suggests: it re-derives the whole contract from
disk, fails when any never-allow class reads allow, and fails when any of the nine
permission rules has gone missing.

## Cost governance

It writes a declarative contract only. **It measures nothing.** No token counter, no cost
meter, and no runtime consumer of the artifact exists anywhere in the codebase. The
warn-at fraction has no consumer. `verify` checks only that the four limits are present
and positive.

## A shared limitation

None of these modules' option validators is invoked at configuration load. A malformed
option surfaces when the module runs, not when `ade.json` is parsed.

## Source map and tests

`crates/ade-core/src/modules/{config_governance,approval_gates,cost_governance}.rs`.
Focused tests: `apply_writes_approvals_json_enumerating_exactly_the_eight_action_classes`,
`secure_defaults_are_exactly_ask_x7_plus_production_affecting_deny`,
`never_allow_classes_cannot_become_allow_apply_fails_nothing_written`,
`verify_fails_when_claude_settings_lose_a_deny_rule_after_apply`,
`managed_files_dedupe_shared_agents_md_across_harnesses`, and
`validate_budget_options_catches_every_malformed_numeric`.
