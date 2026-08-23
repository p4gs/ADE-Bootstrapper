---
type: Architecture Guide
title: Scaffolding and agent memory
description: Three quality templates that are documents rather than gates, and a memory module whose MCP registration is a deliberate credential boundary.
tags: [scaffolding, templates, memory, mcp]
sources:
  - id: openwiki-source-cfbd782bff9bd976c9f7fdec
    resource: repo://crates/ade-core/src/modules/memory.rs
  - id: openwiki-source-8207a8ac3a8ab35a20802648
    resource: repo://crates/ade-core/src/modules/scaffolding.rs
generated: {by: "claude-code", at: "2026-08-22T21:06:03.708Z"}
verified:
  - by: openwiki/0.3.3
    at: 2026-08-22T21:06:03.708Z
---

# Scaffolding and agent memory

## Scaffolding

Three templates under `.ade/templates/` and no policy file. The single option is a
coverage floor, defaulting to 95, interpolated into two of the three; the commit
conventions template takes no argument and is therefore invariant across configurations.

`verify` is a **heading check, not a content check**. It requires each file to exist, be
non-blank, and contain the expected top-level heading. The body — including the coverage
number — is never re-verified, so a repository can pass verification with a checklist
whose coverage line was silently lowered.

More fundamentally: **the templates are documents, not gates.** Nothing in ADE runs the
checklist, measures coverage against the floor, or lints commit messages. The number is
advisory text a coding agent is instructed to honor.

## Memory

The MCP registration is treated as a credential boundary. The opt-in requires **two**
conditions: an explicit boolean option and claude-code being a configured target. The
comparison is by exact value, so the string `"true"` does not qualify.

By default the MCP configuration file is never touched, and the module says so out loud
with an informational finding naming the activation step. When opted in, the registered
server is launched with an **empty environment map** — ADE writes no credential. A
pre-existing server of the same name is preserved rather than overwritten, and the file
is claimed as written only when the registration actually succeeded.

The memory store is git-ignored **unconditionally on every apply**, regardless of the
opt-in, and verify hard-fails if that line is gone. Note the match is exact-line equality
after trimming: a semantically broader ignore rule that would also cover the store does
**not** satisfy it.

What ADE does not do: it never installs, runs, or health-checks the memory server. It
writes an invocation; whether that package resolves and what it does with data is outside
this codebase. Sync is documented as user-controlled and off, and no code path here
configures it. And the store is git-ignored but not encrypted or permission-restricted —
"never leaves the machine" is a policy statement rather than an enforced boundary.

Verify re-derives a three-field contract from the policy and never inspects the MCP
configuration, so an opted-in registration later removed by hand goes unreported.

## Source map and tests

`crates/ade-core/src/modules/{scaffolding,memory}.rs`. Focused tests:
`apply_writes_all_three_templates_with_real_content_15_to_30_lines_each`,
`malformed_coverage_floor_pct_rejected_apply_fails_nothing_written`,
`verify_fails_on_emptied_template_and_on_stripped_h1_header`,
`default_enable_mcp_absent_info_finding_with_activation_mcp_json_not_touched`,
`enable_mcp_true_without_claude_code_harness_mcp_json_not_touched`,
`pre_existing_user_openmemory_entry_in_mcp_json_is_preserved_untouched`, and
`verify_fails_when_gitignore_no_longer_covers_memory_store`.
