ROLE: Continue ADEB's Phase J on branch `feat/phase-j-design-system` (never `main`).
ISC-304..310 are genuinely done — do not redo them. **ISC-311 was never started: this
is the actual task.** Read `ISA.md`'s Phase J criteria + EVERY 2026-08-02 Decisions
entry, then read `Plans/verdant-forging-osprey-operating-instructions.md` in full
before touching code — it has the per-screen checklist, the two proven bug classes to
sweep for, and why ISC-310's own criterion text needs a rewrite before you start.

THE GAP, STATED PLAINLY: only Overview, the nav sidebar, and the gallery got the
Phase J design system. `capability_page.rs`, `projects.rs`, `activity.rs`, and
`modals.rs` are UNCHANGED — `git diff --stat` against `main` proves it, check
yourself before starting. They still use the old `theme::section_surface` or raw
hand-built `egui::Frame`s: no rim light, no status tints, no S12 padding, unswept for
the misalignment bugs found twice on Overview. Right now the app is visually
INCONSISTENT, not merely unfinished — fix that, screen by screen, each its own gated
commit, per ISC-311's own re-baselining rule (old+new goldens both retained, diff
summarized in the commit body).

THE THREE RULES THAT MATTER MOST:
1. Don't assume a screen is fine because its layout LOOKS like Overview's old code —
   MEASURE. The coverage-row bug was invisible until pixel-measured; verify every
   dot-beside-text site on every remaining screen the same way (dot-center vs
   text-ink-center in the regenerated golden), not by eyeballing.
2. ISC-310 is now a STANDING iterate loop, not a 2-bounded gate — the owner said
   "Keep iterating" after cycle 2. Update ISC-310's own criterion text in the ISA to
   say so explicitly (not just leave it in a Decisions entry) before a future fresh
   context trips on the stale "bounded at 2, 3rd rejection HALTS" language. Still
   present each screen's migration for a checkpoint before calling ISC-311 done.
3. Ground every "this screen is now consistent" claim in a tool result — a grep
   proving zero remaining `section_surface`/raw-`Frame` sites on that screen, a
   pixel measurement, a passing gate — never a narrative claim alone.

Effort xhigh. Autonomous — don't stop to ask permission for reversible work, don't
end a turn on a plan instead of doing it. Fresh-context verification agent after
each screen's commit, per the operating-instructions file.
