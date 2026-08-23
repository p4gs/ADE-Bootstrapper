# ADE Bootstrapper

**Bootstrap a secure, high-quality, operationally trustworthy Agentic Development
Environment (ADE)** for AI coding harnesses — Claude Code, Codex, Cursor, OpenCode,
Antigravity, Hermes, and Pi.

One command stands up opinionated, secure-by-default guardrails, governance, and
verification around your coding harness — as plain files in your repo, with no control
plane, no telemetry, and nothing listening on any port.

**v0.2 is a single static Rust binary** plus two native macOS apps:

- **`ade`** — the CLI (~3 MB, no runtime, no interpreter, no dependencies to install)
- **ADE Control Center.app** — a native GUI (egui) to see every capability on your
  machine: what's installed, what's running, versions, warnings/errors — and to
  install / uninstall / reinstall / update each one, toggle capabilities, and manage
  per-project modules
- **ADE Status.app** — a menu-bar helper showing live aggregate health with a
  per-capability dropdown

```
cargo build --release          # or grab a release binary
./target/release/ade init /path/to/your/repo
./target/release/ade gui install   # macOS: apps + menu-bar agent + ~/.local/bin/ade
```

## What you get from `ade init`

- `ade.json` — unified, schema-versioned config (every module on by default)
- `.ade/policy/*.json` — the policy layer: dependencies, sandbox, approvals, secrets,
  git, budget, context-trust, token-efficiency
- `.ade/guardrails/*.md` — secure-coding rules wired into every harness
- `.ade/instructions.md` — the generated baseline (module blocks), translated into
  `CLAUDE.md`, `AGENTS.md`, and `.cursor/rules/ade.mdc` inside managed markers
  (your content outside the markers is never touched)
- `.ade/instructions.local.md` — **your** project instructions: created once, never
  overwritten, appended to every harness's managed block
- `.ade/audit/log.jsonl` — hash-chained audit log, checkpointed in the lockfile
  (`ade audit verify` detects edits, truncation, and re-forged chains)
- `ade.lock.json` — deterministic lockfile making the whole setup verifiable
  (`ade verify`) on any machine, including files planted into the `.ade/` tree
- A pre-commit secret scan (TruffleHog) that actually blocks committing verified secrets
- **Runtime-free hooks**: harness hooks (audit logging, injection scanning) invoke the
  `ade` binary directly — target repos need no JS runtime, and each hook costs
  milliseconds on the paths that run per-commit and per-tool-call

## Commands

| Command | What it does |
|---------|--------------|
| `ade init [dir]` | Bootstrap: detect environment → config → apply modules → lockfile |
| `ade plan` | Dry-run — prints every action, writes nothing |
| `ade apply` | Idempotent re-apply of all enabled modules |
| `ade verify` | Verify on-disk state against lockfile + canonical instructions + module checks |
| `ade doctor` | Tool/harness/environment health report |
| `ade status` | Per-module state summary |
| `ade modules` | List the 15 modules and enabled state |
| `ade translate` | Regenerate harness instruction files from `.ade/instructions.md` |
| `ade lock` | Regenerate the lockfile |
| `ade audit verify` | Validate the audit log hash chain |
| `ade remove` | Withdraw ade from the repo — prints the plan; `--yes` carries it out |
| `ade gui install` | macOS: install the Control Center + menu-bar apps and agent |
| `ade gui uninstall` | Remove the apps and agent |
| `ade gui status` | Menu-bar agent launchd state |
| `ade hook append` | (wired by modules) append a harness hook event to the audit chain |
| `ade hook scan` | (wired by modules) scan stdin for prompt-injection patterns |

All commands accept `--dir <path>` and `--json` (pure JSON on stdout, logs on stderr).
Exit codes: `0` success · `1` failure · `2` usage error.

## The Control Center

The GUI is a **native Rust app** (egui — no webview, no browser, no Electron) driven
entirely in-process by the same `ade-core` engine as the CLI. **Nothing listens on any
port**: there is no local server, no HTTP, no IPC daemon.

- **Capabilities** — every integrated tool and harness CLI: installed state, version,
  latest available version (checked only when you click *Check for Updates* — never
  automatically), running state, and per-capability warnings/errors with concrete
  remediation. Install / Update / Reinstall / Uninstall run as supervised jobs with
  captured logs (uninstall asks for confirmation). Tools without a trustworthy
  automated recipe are honestly labeled *manual* with guidance instead of guessing
  package names.
- **Machine-level enable/disable** — a per-capability toggle persisted in
  `~/.ade/gui.json`. Honest semantics: this greys the capability and mutes its
  warnings machine-wide; the *enforcing* toggle remains each repo's `ade.json`.
- **Projects** — register any ade-bootstrapped repo: per-module toggles (a toggle
  edits `ade.json` through the validated loader, re-applies, re-locks, and re-verifies),
  module findings, and verify results.
- **Activity** — every job with live status and full captured output.
- **A clean exit** — `ade remove` (and *Remove ADE…* on a project) restores the
  repo to exactly how it was before `ade init`: files deleted, managed blocks
  excised, co-owned JSON un-merged, a chained git hook put back. Anything ade
  cannot prove it wrote — your edits, your files — is kept and reported instead.

The menu-bar helper refreshes detection on a bounded budget (it can never hang the
menu bar), renders a status dot (green/amber/red), and lists every capability with
core-formatted labels — adding a capability never requires touching the apps.

## The 15 modules (spec component → module id)

| Spec component | Module | Integrates |
|----------------|--------|-----------|
| Secure-by-default coding guardrails | `guardrails` | Project CodeGuard-style ruleset |
| Software supply chain security | `supply-chain` | osv-scanner, lockfile policy, AI-native deps |
| AI-native sandboxing | `sandbox` | nono, harness permission surfaces |
| Codebase context management | `context` | OpenWiki + CocoIndex, codemap fallback |
| Performance & quality scaffolding | `scaffolding` | PR/testing/commit conventions |
| Network-syncable agent memory | `memory` | OpenMemory / Mem0 MCP (opt-in) |
| Prompt injection & context poisoning defenses | `injection-defense` | trust policy + scanner hook |
| Harness configuration governance | `config-governance` | managed-block translation + drift detection |
| Tamper-evident observability & audit logging | `observability` | hash-chained JSONL + harness hooks |
| Human-in-the-loop approval gates | `approval-gates` | harness permission mapping |
| Secrets & credential hygiene | `secrets` | TruffleHog, pre-commit |
| Git & repository hygiene | `git-hygiene` | OCEAN, git config, protected-branch policy |
| Cost & token budget governance | `cost-governance` | budget + model-routing policy |
| Reproducible environment & lockfiles | `reproducibility` | environment manifest |
| Token efficiency | `token-efficiency` | RTK at the shell boundary |

Every module is individually disableable in `ade.json` (`modules.<id>.enabled: false`) —
secure-by-default means disabling is the explicit act.

## Architecture (v0.2)

```
crates/
  ade-core/            the engine: config, lockfile, audit chain, managed blocks,
                       instructions/translate, 15 modules, 7 harness adapters,
                       pipelines, reports, GUI data layer (inventory/jobs/state)
  ade/                 the CLI binary
  ade-control-center/  native GUI (egui/eframe + AccessKit)
  ade-status/          menu-bar helper
src/ + tests/          the TypeScript v0.1 reference implementation — kept as the
                       EXECUTABLE SPECIFICATION; scripts/parity-check.sh proves the
                       Rust port produces byte-identical artifacts (modulo a closed,
                       documented allowlist) and that v0.1-bootstrapped repos migrate
                       cleanly under the Rust binary
```

The port is verified three ways: a differential harness (byte-comparing full bootstrap
trees against the oracle), replays of the v0.1 adversarial-audit attacks (audit-log
truncation/tail-drop/re-forge, managed-marker clobbering, planted-file detection), and
cross-version interop (the Rust binary appends to and verifies TS-written audit chains).

## What the guarantees actually mean

Honesty about scope is a feature; these are the limits of each claim:

- **Audit log — hash-chained + lockfile-checkpointed.** Every entry commits to its
  predecessor, and `ade apply` pins the chain's length and head hash into
  `ade.lock.json` (which you commit to git). In-place edits, truncation, tail-drops,
  and chains re-forged from the public genesis anchor are all detected. Residual
  limit: an attacker who rewrites the log *and* the committed lockfile together.
- **Secret blocking is verified-findings only.** The pre-commit gate blocks secrets
  TruffleHog can *verify*; unverifiable candidates warn.
- **Enforcement differs per harness.** Harnesses with permission/hook surfaces
  (Claude Code) get real wiring; the rest get policy files + instruction blocks, and
  the docs say which is which.
- **The GUI never acts on its own.** No auto-updates, no scheduled jobs, no network
  calls except the package-manager subprocesses you explicitly trigger.

## Security

Found a vulnerability? See [SECURITY.md](SECURITY.md) for how to report it privately.
