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
- `.ade/instructions.md` — the generated baseline (module blocks), translated into
  `CLAUDE.md`, `AGENTS.md`, and `.cursor/rules/ade.mdc` inside managed markers
  (your content outside the markers is never touched)
- `.ade/instructions.local.md` — **your** project instructions: created once, never
  overwritten, appended to every harness's managed block
- `.ade/audit/log.jsonl` — hash-chained audit log, committed in the lockfile
  (`ade audit verify` detects edits, truncation, and re-forged chains)
- `ade.lock.json` — deterministic lockfile making the whole setup verifiable
  (`ade verify`) on any machine, including files planted into the `.ade/` tree
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
| Codebase context management | `context` | OpenWiki (auto-maintained wiki) + CocoIndex (semantic search), codemap fallback |
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

### Codebase context: three tiers

The `context` module gives harnesses an always-current understanding layer, wired
like every other engine integration (detect + wire + install guidance; `ade apply`
never force-installs):

1. **Codemap** (`.ade/context/codemap.md`) — a zero-dependency structural map,
   always present, regenerated on every `ade apply`. The fallback that never fails.
2. **OpenWiki** (MIT) — when installed, an auto-maintained, navigable prose + Mermaid
   **codebase wiki** (`openwiki/`), refreshed from git diffs. `npm i -g openwiki`.
3. **CocoIndex** (Apache-2.0) — when installed (`cocoindex` or the `ccc` CLI), AST-based
   **semantic code search** for natural-language retrieval instead of whole-tree grep.

**OpenWiki Personal Brain** is modeled as a **distinct opt-in sub-capability** of
OpenWiki (`modules.context.options.enableBrain = true`): general-purpose agent memory
synthesized from external sources (email, notes, web) — complementary to, and separate
from, the codebase wiki. Off by default because it reaches outside the repo; never write
secrets into it. Detected engines and their live state are recorded in
`.ade/policy/context-engines.json`, which `ade verify` re-derives from the machine.

Every module is individually disableable in `ade.json` (`modules.<id>.enabled: false`) —
secure-by-default means disabling is the explicit act.

## What the guarantees actually mean

Honesty about scope is a feature; these are the limits of each claim:

- **Audit log — hash-chained + lockfile-committed.** Every entry commits to its
  predecessor, and `ade apply` pins the chain's length and head hash into
  `ade.lock.json` (which you commit to git). That makes in-place edits, truncation,
  tail-dropping, and a chain re-forged from the public genesis anchor all detectable.
  It is *not* cryptographically signed: an attacker who can rewrite both the log and
  the committed lockfile can still produce a consistent story. External anchoring is
  a v0.2 item.
- **Secret scanning blocks *verified* secrets.** TruffleHog verifies credentials
  against the live provider; an unverifiable key (offline machine, unreachable
  endpoint, offline-only key type) is not blocked. This is a deliberate trade against
  false positives bricking every commit. The hook fails *closed* if it cannot scan.
- **Enforcement vs. instruction.** Claude Code gets real enforcement (permission
  rules, hooks, MCP). The other six harnesses get policy files plus instruction
  blocks — a contract the agent is told to follow, not a mechanism that stops it.

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
