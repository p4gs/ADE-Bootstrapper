# Phase J, ISC-311 — the actual full rollout — operating instructions

This supplements `Plans/verdant-forging-osprey.md`, which is the short pointer to
paste into `/goal` (char-cap-safe). Read that file first for the three rules that
matter most; this file is the detail.

## Why this file exists

A prior session (this same branch) closed ISC-304 through ISC-309 correctly, ran
three real ISC-310 checkpoint cycles with the owner (quoted verdicts in `ISA.md`
`## Decisions`, timestamps `2026-08-02T19:45Z` / `T21:15Z` / and the cycle-3 response),
and then reported "next levers" language that read, to the owner, like the phase was
close to finished. **It was not.** `git diff --stat main` at the point that session
ended shows these files at ZERO lines changed since Phase J began:

- `crates/ade-control-center/src/capability_page.rs`
- `crates/ade-control-center/src/projects.rs`
- `crates/ade-control-center/src/activity.rs`

And `crates/ade-control-center/src/modals.rs` changed only 4/14 lines (unrelated
cleanup, not a design-system migration). ISC-311 explicitly requires "every existing
screen (Overview, Capability detail, Projects, Activity, all modals, the
sidebar/nav chrome) migrated" — four of those six are untouched. Do not repeat the
same mistake: do not report progress language that could read as "done" until every
screen in that list has a real commit against it.

## What "done" already looks like — copy this pattern, don't reinvent it

Read `overview.rs` and `theme.rs` on this branch before writing anything. The
pattern that shipped and was owner-approved:

- Card surfaces go through `theme::elevated_card(ui, p, dark_mode, tint, y_margin, add)`
  — NOT the old `theme::section_surface`. `tint` is `Some(p.tint_ok/warn/err/info)`
  when the card's content has a status meaning, `None` for a neutral list container
  (see `overview.rs`'s attention box vs. its coverage box for both cases).
- `y_margin` is `theme::Space::S12` for anything with a headline — the "not enough
  empty space buffer" nit was real and the fix was measured, not guessed.
- Dot-beside-text clusters (a status mark next to a title, or a title+caption
  block) go through `crate::app::mark_beside_block` — read its doc comment in
  `app.rs`, it explains the exact egui nested-`ui.vertical` top-align bug it exists
  to prevent. Do NOT hand-roll a new `ui.vertical` beside a status dot on any
  screen; use the helper, or if the row genuinely doesn't fit its shape, extend the
  helper rather than duplicating the bug class it was built to kill.
- A single-line dot-and-label row (no caption underneath) inside a flat
  `ui.horizontal` — like `activity.rs`'s `job_card` and `projects.rs`'s
  `project_card` currently are — is NOT automatically safe just because it's
  flat. `ui.horizontal`'s default vertical alignment centers by widget bounding
  box, not by cap-height/baseline, which is exactly the kind of thing that reads
  fine in code review and off by a pixel or two on screen. VERIFY it with the same
  pixel-measurement method used on the coverage row (crop the regenerated golden,
  measure dot-ink-center vs text-ink-center in Python/PIL, confirm the delta before
  claiming it's fine) — don't assume flat means correct.
- Selected/emphasized state gets `p.selected_accent` + semibold, not the old
  neutral `p.selected` wash — see `nav.rs`'s nav row.
- Uppercase eyebrows get `.extra_letter_spacing(0.8)` — see `theme::section_heading`
  and `nav.rs`'s `section_header`.
- Light appearance cards render on the step-3 sand canvas with pure-white panels
  (`ColorRole::Bg`/`ColorRole::Panel` in `tokens.rs`) — this is automatic once a
  screen goes through `Palette`/`elevated_card`, nothing screen-specific to do.

## Per-screen checklist — work in this order, each its own gated commit

For EACH of the four screens below: grep the file for `section_surface` and raw
`egui::Frame::new()` card construction first — that count must be zero when you're
done (or explicitly justified in a code comment if a genuine exception exists, e.g.
a popup that intentionally looks different). Migrate every card to `elevated_card`.
Check every dot-beside-text site against `mark_beside_block` or explicit pixel
measurement. Regenerate goldens (`UPDATE_SNAPSHOTS=1 cargo test --workspace`), run a
DETERMINISTIC re-run after (no env var) to prove stability, visually review the
regenerated goldens yourself (Read tool on the PNG) before claiming done, run all
seven gates (fmt, clippy -D warnings, workspace tests, ISC-304 grep, coverage-check.sh
≥95/95, cargo deny, parity-check.sh, emoji-ban.sh), retain old+new goldens per
ISC-311's own re-baselining rule, write a commit message summarizing the visual
diff, queue it in `scratchpad/phase-j-commits/` (tap-blocked — never self-sign, see
CLAUDE.md's Secretive signing rule), update ISA.md with a real Verification line and
a Decisions entry.

1. **`capability_page.rs`** — 3 `section_surface` call sites (stats strip context,
   the main content card, one more — grep to find the current line numbers, they
   will have shifted). Check the headline: it's currently `SIZE_TITLE` (20pt), one
   step below Overview's new `SIZE_DISPLAY` (26pt). Decide explicitly whether a
   capability-detail page title should also get the display treatment or stay at
   `SIZE_TITLE` as a deliberate hierarchy choice (Overview's headline is the app's
   one "big statement," this page's title is navigational) — write down which you
   chose and why in a code comment, don't leave it silently inherited from before
   this phase existed.
2. **`projects.rs`** — `project_card`'s raw `Frame` → `elevated_card`. Its
   `MatrixCell` grid dots (`status_dot_at` on a fixed rect) are a different,
   probably-fine pattern (single glyph in a grid cell, no adjacent text block) —
   confirm with a quick pixel check rather than skipping silently.
3. **`activity.rs`** — `job_card`'s raw `Frame` → `elevated_card`. Its dot+labels
   are inside one flat `ui.horizontal`, not a vertical block — still pixel-verify
   per the checklist above rather than assuming the flat-layout exemption applies.
4. **`modals.rs`** — currently hand-paints `theme::modal_shadows` directly rather
   than using any shared card helper (this is correct — modals are a different
   elevation tier than cards, don't force them onto `elevated_card`) — but check
   whether modal titles/content should pick up the letterspaced-eyebrow and
   semibold-emphasis conventions now established elsewhere, for consistency, and
   whether any modal has a dot-beside-text row that needs the same sweep.

## ISC-310's criterion text is now stale — fix it before or during this work

`ISA.md`'s ISC-310 criterion still reads "A revise-and-resubmit cycle is bounded at
2; a 3rd rejection HALTS with a `BLOCKED:` row." That is no longer what's happening:
the owner's actual verdict at cycle 2 (quoted in Decisions, `2026-08-02T21:15Z`) was
approval of the direction plus an explicit standing instruction to keep iterating —
which turned the bounded gate into an open loop by his own choice, not a failure to
converge. Rewrite the ISC-310 criterion text itself (not just another Decisions
entry) to state the current, real process: the checkpoint stays open across
per-screen ISC-311 commits too, at the owner's discretion, until he says the
rollout is done — a fresh-context agent reading only the checkbox line should not
be misled into thinking a 3rd checkpoint is a hard failure state.

## Stop conditions

Same as the original Phase J prompt: if a genuine mid-task human-response mechanism
is unavailable when you reach a screen's checkpoint, use ISC-310's degraded or halt
branches (still valid, unchanged) rather than self-closing. Never claim a screen is
"done" without the grep-zero + pixel-measurement + all-seven-gates evidence named
above attached to the ISA's Verification line for it.

## DONE, for this specific run

Either: all four screens migrated, each with its own gated commit, ISC-311 checked
`[x]` with real per-screen Verification evidence, ISC-310's criterion text
rewritten to match reality, and a fresh checkpoint presented to the owner covering
all four newly-migrated screens together — OR a correctly-logged `AWAITING-HUMAN-
CHECKPOINT` / `BLOCKED:` halt naming exactly which screens are done and which
remain, per the same honesty standard the original Phase J prompt set. Do not stop
partway through the four screens without a halt row naming the remainder explicitly.
