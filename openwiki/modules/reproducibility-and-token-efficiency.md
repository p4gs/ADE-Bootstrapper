---
type: Architecture Guide
title: Reproducibility and token efficiency
description: Two modules at the tail of the registry with exactly opposite stances on environment drift, and neither one enforcing what its name suggests.
tags: [reproducibility, manifest, drift, tokens]
sources:
  - id: openwiki-source-1df4817b0928a66819f67546
    resource: repo://crates/ade-core/src/modules/reproducibility.rs
  - id: openwiki-source-6b6849001d315a5f799a4deb
    resource: repo://crates/ade-core/src/modules/token_efficiency.rs
generated: {by: "claude-code", at: "2026-08-22T21:06:03.708Z"}
verified:
  - by: openwiki/0.3.3
    at: 2026-08-22T21:06:03.708Z
---

# Reproducibility and token efficiency

Both sit at the tail of the registry, so they observe a machine already touched by every
other module. Their most instructive property is that they take **opposite positions on
environment drift**.

## Reproducibility

`.ade/manifest.json` is the one artifact deliberately allowed to contain machine-varying
values. Content is still key-ordered, so ordering is stable even though the values are
not. Because it is written through the recorded artifact writer, it is inside lockfile
scope, and its machine-varying content is therefore expected to change the lockfile
between machines.

Absence is recorded as null, never as an error. A tool present on the path whose version
probe fails is recorded as null; a fully null manifest is still a successful apply. The
harness section always carries one key per adapter rather than per configured agent.

**Version drift is informational and never fails verification.** The only way this
module's verify fails is a missing or unparseable manifest, or one lacking its required
keys.

What it does not do: it does not pin, lock or install anything. It is a snapshot for
comparison; nothing consumes it to constrain a build, and ADE will not refuse to run
against a drifted environment. The probe captures only the first line of a version
command, and comparison is exact string inequality with no semantic version parsing. It
also records what was on the path at apply time, not what a coding agent will actually
invoke — a shim or shell alias can diverge from the probed binary.

## Token efficiency

**It measures no tokens.** It writes a static contract whose only dynamic field is
whether the tool was detected. With the tool absent it still writes the artifact, marks
itself disabled, and returns degraded with install guidance.

Its verify re-derives that flag from the live machine and **fails on mismatch**, making
it the one module here that hard-fails on environment drift — the exact opposite of
reproducibility's informational stance. Installing or removing the tool after apply
therefore breaks verification until the next apply.

Neither module wires anything into a coding agent: no settings merge, no hook, no MCP
entry. Their instruction blocks are the entire behavioral channel. Wrapping is
*instructed*, not installed or intercepted — no shell wrapper, alias or hook is created,
so an enabled flag means only that the tool was detected on this machine. The efficiency
policy has no consumer anywhere in the codebase beyond its own verify.

## Source map and tests

`crates/ade-core/src/modules/{reproducibility,token_efficiency}.rs`. Focused tests:
`apply_writes_manifest_with_os_arch_tools_and_core_bun_git_versions`,
`everything_missing_valid_manifest_of_nulls_status_applied`,
`exec_hard_failure_never_propagates_manifest_still_written_applied`,
`verify_reports_version_drift_as_info_findings_never_a_failure`,
`rtk_absent_policy_still_written_enabled_false_degraded_with_guidance`, and
`verify_enabled_must_equal_rtk_presence_in_the_current_ctx_re_derived`.
