# ADEB Design-System Rebuild — Fable 5 Operating Instructions (detail file)

> Referenced by the short `/goal` prompt in `Plans/resolute-charting-heron.md`. Read this file
> in full before starting ISC-304. This file holds everything the `/goal` field itself couldn't
> fit under the harness's ~4000-character cap — do not treat it as optional supplementary
> reading, it carries the actual verification design, not just color.

## The order of operations, decided — not yours to redesign

1. Build the token crate (ISC-304-307) first.
2. Build the in-app component gallery (ISC-308) — NOT a throwaway mockup. This project has three
   prior recorded instances of validating something other than the real screen (read the ISA's
   2026-07-30 narrative sections if you want the specifics); a gallery that renders inside the
   real running app avoids repeating that class of mistake, and survives afterward as a permanent
   QA asset instead of being thrown away. **Boundary:** the gallery may extract and wrap existing
   render functions as reusable components — it may NOT restyle live screens ahead of step 4's
   checkpoint. Doing so pre-empts the exact gate the checkpoint exists to enforce; keep screen
   restyling inside ISC-311, after approval.
3. Deepen native-macOS-API investment on the specific chrome surfaces named (ISC-309) —
   traffic-light precision, vibrancy/materials, menu-bar integration, focus rings. This is not a
   rendering-engine swap; it's going further into `objc2`/AppKit on the surfaces where every
   comparable team that closed this gap did the same thing. **These four surfaces do NOT share
   one verification path** — see VERIFICATION below for the split; don't assume a single
   snapshot approach covers all four, it can't by construction.
4. STOP before rollout. Closing ISC-310 (the human checkpoint) is a hard gate before ISC-311
   begins. This is the single most important sequencing rule in this entire run.
5. Only then: full ADEB rollout (ISC-311), incremental, Overview first as the acid test, then the
   remaining screens (Capability detail, Projects, Activity, modals, nav chrome) one at a time.
   Not big-bang. Each screen's migration ends at its own green, gated commit.

## Stack & architecture

- Rust, workspace at repo root: `ade-core` (engine/pure logic), `ade` (CLI), `ade-control-center`
  (egui/eframe GUI), `ade-status` (objc2 AppKit menu-bar tray). **Place the token crate in
  `ade-core`** (e.g. `ade-core/src/gui/tokens/`), not `ade-control-center`. This is not "your
  call" — `scripts/coverage-check.sh`'s `IGNORE` regex excludes BOTH `ade-control-center/` and
  `ade-status/` from the coverage floor entirely (verified directly against the script). Placing
  new, genuinely testable pure logic (the semantic color enum, the WCAG contrast function) inside
  either excluded crate would make the "95/95 floor" gate silently vacuous for the most important
  logic this phase adds. `ade-core` is coverage-counted; let `ade-control-center`/`ade-status`
  re-export or thinly wrap it for rendering.
- Existing `theme.rs` (in `ade-control-center`) already has `elevated_shadows()` and
  `DURATION_HOVER`/`DURATION_VALUE`/`DURATION_ENTRANCE`/`DURATION_PAYOFF`. EXTEND these, do not
  replace them — ISC-307 requires a diff proving every pre-existing call site still resolves to
  the same values it did at close of the 2026-08-01/02 Polish phase.
- egui_kittest is already wired, but NOT as separate files under `crates/ade-control-center/
  tests/` — that directory holds only the `snapshots/*.png` goldens. The actual test harnesses
  are inline `#[cfg(test)]` modules inside the `src/*.rs` files themselves (`activity.rs`,
  `app.rs`, `overview.rs`, `theme.rs`, etc.), sharing `src/testkit.rs`. Extend that existing
  pattern for new tests; don't build a parallel harness in `tests/` expecting to find one there.
  Use kittest as your primary, always-on evidence layer for anything genuinely egui-rendered.
- The existing ADEB gate suite (run all of these, every phase, no exceptions): `cargo fmt --all
  --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test --workspace`,
  `scripts/coverage-check.sh` (95/95 floor), `cargo deny check`, `scripts/parity-check.sh`,
  `scripts/emoji-ban.sh` (yes, this is real — no emoji anywhere in the crates/ tree, by design).

## Reuse decisions, resolved — don't re-derive

- **Stay on egui/eframe.** Not up for reconsideration in this run.
- **cosmic-theme (`pop-os/libcosmic`, MPL-2.0) is your token-crate's starting point.** Confirmed
  dependency-clean of Iced. Fork/vendor its `Spacing`, `CornerRadii`, `Density`, and
  `Container`/`Component` structs rather than reinventing the shape. MPL-2.0 is file-level weak
  copyleft, compatible with combination into ADEB's MIT-licensed "Larger Work" — keep an MPL-2.0
  notice on the vendored file(s), everything else stays MIT. **`deny.toml`'s `[licenses] allow`
  list is permissive-only today and does NOT include MPL-2.0** (verified directly against the
  file) — if you vendor, adding `"MPL-2.0"` to that list, scoped as narrowly as `cargo-deny`
  supports, is part of doing ISC-305, not a surprise blocker. **Named fallback, trigger covers
  BOTH real failure modes:** if the `palette`-crate color types don't adapt cleanly to
  `egui::Color32` within one bounded attempt, OR the `deny.toml` addition is the wrong call for
  this repo's policy, stop vendoring and reimplement the same shape instead — with attribution,
  recorded in `## Decisions`. A clean-room reimplementation carries no MPL obligation and needs
  no `deny.toml` change.
- **Do NOT depend on or vendor from GPUI** (`zed-industries/zed`). Issue #55470 (GPL-3.0
  contamination via `sum_tree→ztracing`) is confirmed still open, no maintainer resolution.
  Reference its patterns only: the semantic `Color` enum indirection, the `component_preview`
  in-app gallery pattern, the closed-form shadow math already partially adopted in `theme.rs`.
- **Do NOT depend on or vendor from WarpUI/warpui_core** (`warpdotdev/Warp`, MIT but not
  standalone-usable — 25+ Warp-internal workspace dependencies confirmed via Cargo.toml).
  Reference its patterns only: the WCAG-luminance auto-contrast repair algorithm, the
  opacity-derivation scale for hover/pressed/disabled states.

## Fable-5-specific operating instructions

Anthropic's own current guidance, chosen because this project has a documented, repeated failure
mode they directly target.

**Ground every progress claim in evidence — the single highest-priority instruction here:**

> Before reporting progress, audit each claim against a tool result from this session. Only
> report work you can point to evidence for; if something is not yet verified, say so
> explicitly. Report outcomes faithfully: if tests fail, say so with the output; if a step was
> skipped, say that; when something is done and verified, state it plainly without hedging.

**You are operating autonomously.** The user is not watching in real time and cannot answer
questions mid-task, so asking "Want me to…?" will block the work. For reversible actions that
follow from the original request, proceed without asking. Before ending your turn, check your
last paragraph — if it's a plan, a question, or a promise about work you haven't done, do that
work now with tool calls. End your turn only when the task is complete or you're blocked on
input only the user can provide — ISC-310's checkpoint is the one genuine such block.

**When you have enough information to act, act.** Don't re-derive facts already established
here or in the ISA, re-litigate a decision already made, or narrate options you won't pursue.

**Fresh-context subagent verification at a fixed interval, not just at the end:** after each of
ISC-304 through ISC-309 closes, and again before ISC-311's rollout begins per screen, dispatch a
non-forked subagent carrying the ISA's Phase J section and the specific ISC just closed, and have
it verify the claim against real tool evidence independently of your own build context. This
catches describable defects — it does not replace ISC-310's human checkpoint, which exists
specifically because this project's own adversarial-critique passes already proved subagent
review alone is insufficient for "does this actually look considered."

**Effort: `xhigh` by default** — Anthropic's own named tier for "the most capability-sensitive
workloads," which a cross-cutting refactor combined with aesthetic judgment, on a project with a
documented history of confidently-wrong self-assessment, is exactly.

**Ample context remains (1M-token window).** Do not stop, summarize, or suggest a new session on
account of context limits. If you hit a genuine context reset, the ISA on disk is what's true —
re-read `#### Phase J` and its Decisions entries before continuing.

**Lessons file:** ADEB's ISA already implements the pattern Anthropic recommends for this
(Decisions entries including dead ends, a Learning/Changelog trail). Extend it; don't build a
parallel mechanism.

**Parallel subagents:** use them readily for independent subtasks, prefer async over blocking,
keep long-lived subagents where they'd otherwise redo expensive context-gathering.

## Verification — the part that must not be softened

Every ISC in Phase J reduces to one of two things: a machine check, or the one deliberately
non-machine gate this run has (ISC-310). Do not blur the two.

- **ISC-304 (semantic color enum):** `grep -rno "Color32::[A-Za-z_]*" crates/ade-control-center/
  src/` (and `crates/ade-status/src/` if the token crate is shared), read the actual matches and
  exclude comments and the token layer's own definitions — must return ZERO real hits on
  raw-literal constructors: `from_rgb`, `from_rgba_unmultiplied`, `from_rgba_unmultiplied_const`,
  `from_gray`, `from_black_alpha`, named constants `WHITE`/`TRANSPARENT` used outside the theme's
  own token definitions. **`gamma_multiply` is explicitly OUT of scope** — it's a transform on an
  already-resolved color, not a raw-color source. Re-run the grep yourself before relying on this
  list; don't trust it from memory.
- **ISC-305 (token structs + license):** the vendored file(s) — or clean-room reimplementation —
  exist, compile, carry an MPL-2.0 notice if vendored, and (if vendored) `deny.toml` has been
  updated; `cargo deny check` passes with the new dependency graph either way.
- **ISC-306 (WCAG auto-contrast):** a real unit test asserting ≥4.5:1 against `p.bg` and
  `p.panel` in both appearances, for at least 5 arbitrary accent colors.
- **ISC-307 (extend, don't replace):** diff against the PINNED baseline commit
  `d5b1ba88afc97bf93c3327a2aeea800bd4252eb6` (Phase I close) — not your own already-modified
  branch, which would make this check pass vacuously. Every pre-existing shadow/duration call
  site must resolve unchanged from that exact SHA. Regression here is a hard stop, not a note.
- **ISC-308 (gallery, reachable):** a real menu action or keybinding opens it; a kittest snapshot
  of the gallery exists; grep or review confirms no live screen was restyled to build it.
- **ISC-309 (native chrome depth) — split verification, one mechanism per surface-class:**
  - Focus rings — genuinely egui-drawn, a real kittest snapshot is required.
  - Traffic lights + vibrancy — live native capture (`interceptor macos screenshot`) when
    reliable; when it isn't, a precise written before/after description plus whatever kittest
    golden exists for anything egui-rendered nearby (ISC-310 branch 2).
  - Menu bar — `interceptor macos` AX-tree reads against the live `ade-status` process, this
    ISA's own established pattern for tray verification elsewhere. Not kittest.
  No surface closes by snapshotting something adjacent and calling it done.
- **ISC-310 — READ THIS TWICE. This is a genuine stop, not a formality.** You may NOT mark this
  satisfied on kittest-green alone, on a screenshot alone, or on your own narrative claim that it
  "looks good." **Three branches — pick the one that's honestly true, never force branch 1:**
  1. **Full live checkpoint** (genuine mid-task human-response mechanism AND reliable capture
     both exist): every one of ISC-304 through ISC-309 closed with real evidence, a fresh build
     packaged the way `scripts/bundle-apps.sh` already does it, a message to the user presenting
     real live screenshots — both themes, Overview, one other real screen, every ISC-309 surface
     reachable by live capture — saved to `scratchpad/phase-j-checkpoint/` with absolute paths
     named in your message. Ask directly: does this look right to proceed, or does it need
     another pass. Wait for the actual answer.
  2. **Degraded checkpoint** (response mechanism exists, capture is unreliable — this project's
     own ISC-303 already documents exactly this failure mode on this machine under load, treat
     it as expected, not exceptional): present the kittest suite plus a precise written
     description of every native-chrome change, and say explicitly that live capture couldn't be
     obtained. This is a legitimate, named path to a real checkpoint.
  3. **No mechanism at all:** does NOT default to self-closure. HALT. Write an
     `AWAITING-HUMAN-CHECKPOINT` row to `## Decisions` naming whatever evidence exists at halt
     time, commit everything through ISC-309 cleanly, end your turn.

  Log the outcome as a `## Decisions` row quoting the owner's actual response where one exists —
  Fable transcribes the human's words, it doesn't write the verdict itself. Do not proceed to
  ISC-311 without a genuine approved verdict. Branch 2 or 3, landed honestly, is a SUCCESSFUL
  run — never fabricate branch-1 evidence to avoid them. This is the one place in this run where
  "operating autonomously, don't ask" is deliberately overridden, on purpose. **Revise-and-
  resubmit is bounded at 2 cycles.** A first "not yet" is the gate working as designed. A THIRD
  rejection HALTS with a `BLOCKED:` row rather than looping indefinitely.
- **ISC-311 (rollout):** each screen's migration is its own commit, passing the full existing
  gate list plus ISC-304's and ISC-306's new gates. A screen that fails a gate does not move on
  until fixed. Each phase re-baselines its own kittest goldens EXPLICITLY: keep both old and new
  golden in that commit, summarize the visual diff in the commit body.

The instant an ISC's checkbox flips `[ ]` → `[x]` in the ISA, append the evidence to
`## Verification` in the same or next tool call — a one-line provenance stub, per the ISA
format's own close-time convention. A checked box with no evidence line is not done.

## Stop conditions

If the same gate (build/test/clippy/coverage) fails twice in direct succession on the same ISC,
write a `BLOCKED:` entry to the ISA's `## Decisions` with the exact failing output and HALT that
ISC — move to something else in scope if anything is unblocked, otherwise stop the run. No third
retry. Never weaken a gate to escape it, and never skip ISC-310's checkpoint to keep momentum. A
third revise-and-resubmit rejection, or the absence of a human-response mechanism, are BOTH named
stop conditions in their own right — neither is a failure of this run.

## Done

Two valid end states, both successful:

1. **Full completion:** ISC-304 through ISC-309 closed with real evidence; ISC-310's checkpoint
   logged with an approved verdict (the owner's actual quoted words); ISC-311 closed
   screen-by-screen with every screen's own gated commit and re-baseline note; every checkbox
   flip has a matching `## Verification` line.
2. **Correctly blocked:** ISC-304 through ISC-309 closed with real evidence, commits clean, and
   an `AWAITING-HUMAN-CHECKPOINT` row logged — because no human-response mechanism was available,
   or a revise cycle is in progress. Not "incomplete work" — the checkpoint doing its job. Do not
   manufacture a fake approval to reach end-state 1 instead.

If ISC-310 comes back "revise-and-resubmit" (cycle 1 or 2), done for that cycle is the revision
addressed and the next checkpoint requested. A 3rd rejection is end-state 2's `BLOCKED:` variant.
