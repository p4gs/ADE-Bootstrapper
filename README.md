# ADE Bootstrapper

**Bootstrap a secure, high-quality, operationally trustworthy Agentic Development
Environment (ADE)** for AI coding harnesses — Claude Code, Codex, Cursor, OpenCode,
Antigravity, Hermes, and Pi.

One command stands up opinionated, secure-by-default guardrails, governance, and
verification around your coding harness — as plain files in your repo, with no control
plane and no network calls.

```
bun install        # dev deps only — the tool itself has ZERO runtime dependencies
bun run src/cli.ts init /path/to/your/repo
```

## What you get from `ade init`

- `ade.json` — unified, schema-versioned config (every module on by default)
- `.ade/policy/*.json` — the policy layer: dependencies, sandbox, approvals, secrets,
  git, budget, context-trust, token-efficiency
- `.ade/guardrails/*.md` — secure-coding rules wired into every harness
- `.ade/instructions.md` — ONE canonical instruction source, translated into
  `CLAUDE.md`, `AGENTS.md`, and `.cursor/rules/ade.mdc` inside managed markers
  (your content outside the markers is never touched)
- `.ade/audit/log.jsonl` — tamper-evident, hash-chained audit log (`ade audit verify`)
- `ade.lock.json` — deterministic lockfile making the whole setup verifiable
  (`ade verify`) on any machine
- A pre-commit secret scan (TruffleHog) that actually blocks committing verified secrets

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

All commands accept `--dir <path>` and `--json` (pure JSON on stdout, logs on stderr).
Exit codes: `0` success · `1` failure · `2` usage error.

## The 15 modules (spec component → module id)

| Spec component | Module | Integrates |
|----------------|--------|-----------|
| Secure-by-default coding guardrails | `guardrails` | Project CodeGuard-style ruleset |
| Software supply chain security | `supply-chain` | osv-scanner, lockfile policy, AI-native deps |
| AI-native sandboxing | `sandbox` | nono, harness permission surfaces |
| Codebase context management | `context` | generated codemap artifacts |
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

## Design contract (from the spec)

**Opinionated** defaults · **Modular** enable/disable/swap · **Composable** — integrates
best-in-class open source (TruffleHog, RTK, OCEAN, nono, OpenMemory, CodeGuard) rather
than reimplementing it · **Harness-native** abstractions (config surfaces, tool
boundaries, hooks) · **Local-first** — plain files, no hosted control plane ·
**Secure-by-default** with explicit opt-outs · **Cross-platform-aware** (verified on
macOS/Linux in v0.1).

### Non-goals

- Building a general-purpose agent framework.
- Building a custom multi-agent runtime or orchestration SDK.
- Building an ADK for creating bespoke agents from scratch.
- Replacing the coding harness itself.
- Replacing existing best-of-breed open-source tools when integration is the better path.

## Development

```
bun run check      # typecheck + tests with the 95%/95% coverage gate
bun test           # fast test run
```

Architecture: [`docs/DESIGN.md`](docs/DESIGN.md). Verifiable system of record:
[`ISA.md`](ISA.md). Source spec: `ADE Bootstrapper.md`.
