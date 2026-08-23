---
type: Architecture Guide
title: The command surface
description: One match arm per command, a three-valued exit contract, and the structural reason --json output can never be contaminated by library logging.
tags: [cli, exit-codes, json, arguments]
sources:
  - id: openwiki-source-81dae2a59f861ac36fa4c84b
    resource: repo://crates/ade-core/src/report.rs
  - id: openwiki-source-fdd192eaa25deaf8ab61ea29
    resource: repo://crates/ade/src/main.rs
generated: {by: "claude-code", at: "2026-08-22T21:06:03.708Z"}
verified:
  - by: openwiki/0.3.3
    at: 2026-08-22T21:06:03.708Z
---

# The command surface

`crates/ade/src/main.rs` is the whole CLI. There is no argument-parsing dependency: the
binary is a single non-recursive `match` on one command string, with sub-verbs resolved
by inspecting the first positional inside the arm. Adding a command means adding an arm
plus a row in the help table, and **nothing checks that those two stay in sync**.

This surface is Rust-only. The TypeScript file under `tests/` exercises the
[v0.1 oracle's](../verification/dual-implementation-and-parity.md) own CLI and is not
coverage for this binary.

## Exit codes

Three values, enforced only by what `run` returns:

- `2` — argument or usage error: a parse failure, an unknown command, or an unknown
  sub-verb of `gui`, `hook` or `export`.
- `1` — operational failure.
- `0` — success.

**`audit` deliberately breaks the sub-verb pattern.** `gui`, `hook` and `export` each
end in a catch-all arm returning `2`, but `audit` special-cases only `show` and lets
everything else — including `ade audit bogus` and a bare `ade audit` — fall through to
chain verification, returning `0` or `1`. If you are scripting against exit codes, that
is the one command whose unknown sub-verb is not a usage error.

Several commands are reports rather than gates and can never return non-zero on
content: `plan`, `status`, `doctor`, `modules`, `remove`, `lock` and `audit show`. The
`gui health` arm carries the reasoning explicitly — a broken environment is still a
successful description of one.

`hook append` is contractually incapable of failing. It discards its outcome and
returns `0`, so a malformed payload or an unwritable log can never block the coding
agent that called it.

## JSON purity is structural

`ade-core` contains **zero** print statements. Every byte the process emits originates
in `main.rs`; library code returns data. No amount of library-side logging can
contaminate a `--json` stdout, because there is none to begin with.

`emit` is the single stdout writer and is mode-exclusive: with `--json` it prints
exactly one pretty document and nothing else. Diagnostics go to stderr.

Two writers bypass `emit`, and they are the exceptions to audit when changing output.
The config-loading path prints its own `{"ok":false,...}` so that stdout stays pure JSON
even on the missing-config path, and `hook scan` always prints compact JSON regardless
of `--json`, because its consumer is a hook rather than a person.

One sharp edge: `emit` degrades silently on serialization failure, and the report
commands fall back to a null value. A serialization defect surfaces as the literal
document `{}` or `null` with exit `0`, never as an error.

## Flags are global, not per-command

`--json`, `--yes`, `--extra` and `--dir` are accepted after any command. Nothing rejects
`--yes` on `status` or `--extra` on `apply`. A repeated `--dir` is last-wins, and only
the *last* parse error survives, so a command with two bad flags reports one.

`--help` is implemented as a positional injection rather than a flag, so `ade apply -h`
becomes the `help` command with `apply` demoted to a positional.

`--dir` and the `init` positional resolve identically and `--dir` wins. But `--dir` is
**silently ignored** by the four commands scoped to the machine-level home directory —
`export posture`, `gui health`, `gui status`, and `gui install`/`uninstall`. It is
accepted and has no effect.

## Two wire contracts

`report.rs` is the shared layer with the desktop apps, and `StatusRow.findings` is
skipped during serialization: findings are computed and retained for the apps but
structurally cannot reach the CLI's JSON. The doctor payload uses camelCase renaming
while the status payload does not, and the status arm hand-builds its output from four
fields, so that shape is pinned twice.

## Source map and tests

`crates/ade/src/main.rs` and `crates/ade-core/src/report.rs`. `main.rs` has no in-file
test module; all coverage is the integration suite in `crates/ade/tests/cli.rs`, which
spawns the real compiled binary. Key tests:
`unknown_command_and_unknown_flag_exit_2_with_usage_on_stderr`,
`flag_value_errors_exit_2_and_dash_h_shows_help`,
`hook_and_gui_unknown_subcommands_exit_2`,
`init_bootstraps_with_pure_json_stdout_and_second_apply_succeeds` (the entire stdout is
parsed as one document), `commands_without_config_exit_1_with_init_guidance`, and
`status_json_yields_15_modules_with_exactly_the_v01_keys`.

Known coverage gaps: `remove` in all three outcomes, `gui health`, `gui install`,
`gui uninstall`, and the serialization-failure fallback are untested. The help-table
test also uses its own eighteen-entry list, so drift on the `remove` and `gui health`
rows is unpoliced.
