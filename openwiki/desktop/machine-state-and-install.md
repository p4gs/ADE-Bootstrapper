---
type: Architecture Guide
title: Machine state and installation
description: Why machine-level state lives outside every project tree, a file-based channel that replaces IPC, and what the installer learned about launchd teardown.
tags: [state, jobs, install, launchd]
sources:
  - id: openwiki-source-21f79701116f984b0916887c
    resource: repo://crates/ade-core/src/gui/install.rs
  - id: openwiki-source-7358428d422bd02e17be812e
    resource: repo://crates/ade-core/src/gui/inventory.rs
  - id: openwiki-source-43e8308c040f0a28e19dfe28
    resource: repo://crates/ade-core/src/gui/jobs.rs
  - id: openwiki-source-ac7808e8ba9fcfb2576cc317
    resource: repo://crates/ade-core/src/gui/state.rs
generated: {by: "claude-code", at: "2026-08-22T21:06:03.708Z"}
verified:
  - by: openwiki/0.3.3
    at: 2026-08-22T21:06:03.708Z
---

# Machine state and installation

Rust-only; no TypeScript counterpart.

## The invariant, and why it is real

Machine-level state lives in a home directory outside any project's ADE-owned tree. The
module header states it, and it is load-bearing rather than decorative: the
[lockfile](../engine/lockfile-and-verify.md) walks every file under that tree and treats
anything it does not know as an error. Planting machine state there would break
verification on every project.

**Machine-level disable is a display preference; the project configuration remains the
enforcing lever.** The finding a disabled capability produces says exactly that.

State loading degrades to defaults with a warning for every corruption class and never
panics — unreadable, invalid, wrong root type, unsupported schema. The invalid path
deliberately leaves the file untouched until the next change rather than rewriting it.

Fields are read by name, so keys a newer version no longer defines are silently ignored
and disappear on the next save; a retired toggle is the named precedent. Selection is
validated against the live capability table, but dismissed insight identifiers
deliberately are **not**, because a retired rule should simply never match again rather
than needing a migration. An unknown scope falls back to the overview rather than
rendering an empty window.

## A file instead of a daemon

The job file is a channel, and the module header states its purpose: the menu-bar helper
reflects activity **with no IPC**. There is no daemon and no socket. Three separate
binaries read it.

One reader exists specifically to prevent two surfaces disagreeing: without it, the tray
would report a capability whose last install failed as merely "not installed" while the
desktop app showed an error.

A subtle ordering contract: the persisted file is newest-first while the in-memory list is
oldest-first, and **both readers depend on that** in opposite ways. Reversing either
silently inverts "most recent".

The runner **resumes** persisted history on construction, and the header records exactly
what starting empty cost: persistence writes only what is in memory and the identifier
counter restarted at one, so the first action after a restart both reused an existing
identifier and overwrote the entire recorded history, while the activity view came back
blank.

Any job still marked running at startup is retired as an error saying the app exited while
it was running, and the active set is deliberately re-created empty — re-locking that
capability would wedge it forever. The honesty note is explicit: we do not know whether
the underlying install finished.

One job per capability at a time; different capabilities run freely. History and logs are
bounded and trimmed from the front, so a long install's opening output is unrecoverable.
A multi-step recipe stops at the first failure, and completion persists **before**
releasing the capability lock, because the settle signal fires the moment the active set
empties.

Timestamps are written and parsed by hand with no date dependency, and malformed input
yields a display gap rather than a panic.

## The install layout

The binary goes to a local bin directory, both application bundles to the user's
applications directory, and there is exactly **one** launch agent — the tray. There is no
server.

Both install and uninstall refuse to touch any destination that does not both live under
the applications directory and end in the bundle suffix, because the removal path is a
recursive delete.

**The teardown lesson is recorded in a comment**: the unload command returns before the
job is actually gone, so bootstrapping into a domain still tearing down fails with an I/O
error and makes reinstall a coin flip. The installer therefore polls for the job's
disappearance and retries the bootstrap.

The agent definition bakes in an explicit search path, because launchd's default is
minimal — the same root cause as the
[detection layer](../engine/detection-and-environment.md)'s. It also sets the state
directory so the tray reads the same location the installer used, and uses a keep-alive
policy that restarts on crash while letting a clean quit stay quit.

The bundles are ad-hoc code-signed for a **stable code identity**, so system permission
grants survive rebuilds instead of evaporating with each binary. Executable names are
deliberately space-free while display names carry spaces, because spaces propagate into
launch arguments and process-matching patterns.

## What is not guaranteed

The job file is single-writer by convention — nothing arbitrates two app instances, and
the persist call ignores its own write error. Uninstall reports success regardless for
several steps. The bundles are ad-hoc signed only, not notarized, so distribution
behavior on another machine is out of scope here. The teardown poll gives up silently
after a bounded number of attempts and proceeds anyway. And the packaging scripts have no
automated tests: the install tests all skip the build step.

## Source map and tests

`crates/ade-core/src/gui/{state,jobs,install}.rs`, plus `scripts/bundle-apps.sh` and
`scripts/make-icon.ts`. Focused tests:
`corrupt_variants_degrade_with_warning_never_crash`,
`a_state_file_with_the_retired_grouping_field_still_loads_cleanly`,
`a_new_runner_resumes_persisted_history_instead_of_clobbering_it`,
`resume_retires_a_job_that_was_running_when_the_app_died`,
`install_waits_for_teardown_and_retries_transient_bootstrap_failure`, and
`agent_path_contains_package_manager_dirs_without_duplicates`.
