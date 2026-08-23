---
type: Architecture Guide
title: Prompt-injection defense
description: A trust policy that classifies external content as read-only data, a heuristic scanner that cannot block, and the fail-open choice behind it.
tags: [injection, trust-policy, scanner, heuristics]
sources:
  - id: openwiki-source-a365f375339afe3ba329305b
    resource: repo://crates/ade-core/src/modules/injection_defense.rs
generated: {by: "claude-code", at: "2026-08-22T21:06:03.708Z"}
verified:
  - by: openwiki/0.3.3
    at: 2026-08-22T21:06:03.708Z
---

# Prompt-injection defense

Two artifacts, both written unconditionally on every apply, with no external tool
required. A malformed custom-pattern option is a hard pre-write gate: apply returns
failed and leaves nothing on disk.

## The primary defense is the policy, not the scanner

Six source classes are pinned as untrusted and treated as read-only data. `verify` checks
**semantic classification rather than file presence**: every class must carry both
markings, so silently reclassifying web content as trusted fails verify and names the
class.

That protocol — external content is data, never instructions — is the real defense. The
design document says so directly: the scanner is heuristic defense-in-depth, not
protection.

## The scanner cannot block anything

Nothing invokes it automatically. No module wires it into a coding-agent event; the only
auto-wired hook belongs to [observability](observability-and-git-hygiene.md). The scanner
reaches the agent only as a written instruction to run it by hand.

It also **fails open**: the shipped file is a POSIX shell script that delegates to the
`ade` binary, and when that binary is absent it drains input, prints a clean verdict and
exits zero. A vendored repository without `ade` on the path therefore reports "clean" for
genuinely malicious text. That is a deliberate, tested choice rather than an oversight,
but it is worth knowing before relying on the exit code.

Custom patterns are shell-quoted into the script's arguments, so a pattern containing a
quote cannot break out of the invocation line.

## Heuristics, and their deliberate cost

The five patterns match imperative verb plus object combinations rather than bare
keywords, which is what keeps benign security prose from flagging — four such strings are
asserted clean in tests. That trade buys precision at the cost of recall: paraphrased
attacks are out of scope.

The scanner reports at most one match per pattern, so a document with ten exfiltration
attempts yields one excerpt.

An invalid custom regex reaching the scanner is silently skipped. That is safe only
because apply and detect refuse to ship one in the first place — the two guarantees are
coupled, so weakening the validation would silently weaken the scan.

The shipped script carries a marker string that verify checks, and because it lives in
the ADE-owned tree it is additionally hash-pinned by the lockfile.

## Source map and tests

`crates/ade-core/src/modules/injection_defense.rs`. Focused tests:
`apply_writes_context_trust_policy_classifying_all_six_untrusted_source_classes`,
`scan_flags_all_five_malicious_pattern_classes`,
`scan_does_not_flag_benign_prose_mentioning_security_topics`,
`spawned_shim_reports_clean_and_exits_0_when_ade_is_absent`,
`spawned_shim_delegates_to_ade_hook_scan_and_propagates_the_exit_code`, and
`verify_passes_after_apply_and_fails_on_each_tamper`.
