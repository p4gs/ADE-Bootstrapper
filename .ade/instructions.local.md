# Project Instructions (user-owned)

## Orienting in this repo, fast

Read in this order and stop as soon as you have what you need:

1. `.ade/policy/context-engines.json` — which context engines are live on this machine.
2. `ccc search "<natural-language question>"` — the CocoIndex semantic index over this
   repo (189 files / ~3.4k chunks). Use it INSTEAD of grepping the tree. Re-run
   `ccc index` after structural changes; the index DB is gitignored.
3. `openwiki/` — the generated codebase wiki, when present. Read it FIRST for prose and
   Mermaid architecture. Never hand-edit generated pages; regenerate instead.
4. `.ade/context/codemap.md` — zero-dependency structural fallback, regenerated on every
   `ade apply`.
5. `ISA.md` (~238 KB) — the product's state-of-record. Do NOT read it whole; query it
   through `ccc search`, which chunks and ranks it.

## Two implementations live here — know which one you are editing

- `crates/` is the **v0.2 Rust implementation** and the shipping artifact:
  `ade-core` (engine: modules, lockfile, audit, managed blocks, harness translation),
  `ade` (CLI), `ade-control-center` + `ade-status` (native egui macOS apps).
- `src/` and `tests/*.test.ts` are the **v0.1 TypeScript tree, retained as the executable
  oracle**. `scripts/parity-check.sh` bootstraps a repo with each implementation and
  diffs the trees; parity must hold. Rust modules carry `port of src/modules/<x>.ts`
  headers naming their oracle.
- Changing behavior therefore usually means changing BOTH trees, or consciously moving
  the allowlist in `scripts/parity-check.sh`. A Rust-only change that alters emitted
  files will fail parity.

## Local conventions worth knowing before you touch anything

- `bun` / `bunx`, never npm / npx. `bun run check` = typecheck + coverage for the TS tree;
  `cargo test` for Rust.
- `scripts/emoji-ban.sh` gates CLI glyphs; `scripts/coverage-check.sh` gates coverage.
- The 15 modules live in `crates/ade-core/src/modules/` — one file per module, each
  implementing detect / apply / verify plus its instruction block.
- Modules DETECT and WIRE tools; `ade apply` must never force-install anything.
