# ADEB Design-System Rebuild — Fable 5 `/goal` Prompt

> **Use:** paste everything inside the code block below, starting at the `/goal` line.
> **Suggested launch:** `/goal effort=xhigh`
> **No placeholders to fill** — every path and decision is already resolved.
>
> **Two files, on purpose.** The `/goal` field itself is capped at roughly 4000 characters (this
> session hit that exact limit earlier today drafting a much shorter prompt) — the full
> operating detail that makes this safe to run unattended does not fit in it. This file's fenced
> block is the short pointer version, under the cap; `Plans/resolute-charting-heron-operating-
> instructions.md` carries the actual verification design, the Fable-5-specific techniques, the
> stop conditions, and the done-state definition. Fable reads both before doing anything — the
> pointer prompt says so explicitly, and it is not optional reading.
>
> **Before launching:** drafted by an authoring-only session (Sonnet 5, ultracode) per
> `~/Code/p4gs/ADE-Bootstrapper/ISA.md`'s `#### Phase J` and its 2026-08-02 `## Decisions`
> entries — four of them, including one recording a two-round review (one in-family, one
> attempted cross-vendor and honestly labeled as such when the cross-vendor path itself failed)
> that caught real gaps in an earlier draft, including two structural ones: ISC-309's original
> wording asked for a kittest snapshot of three surfaces kittest cannot render, and both
> checkpoint end-states originally assumed a screenshot capture path this same ISA's own
> ISC-303 already documents as unreliable on this machine. Both are fixed in what you're about
> to read. Fable is not launched automatically by any prior session — you launch this yourself.

---

```
/goal

# ROLE & INTENT (WHY)
You are closing a specific, named gap in ADE Control Center (ADEB) — a ~100% Rust, egui/eframe,
macOS-first desktop app. Every visual-polish phase shipped so far passed every gate that exists,
and the owner still said "Geez this UI still looks sloppy." Research into 9 real shipped Rust
desktop apps' design systems returned a specific verdict: stay on egui, the actual gap is
design-token discipline and native-macOS-API depth, not the rendering framework. This run builds
that, and closes the loop prior phases couldn't: a "looks good" claim that isn't the same agent
grading its own work.

# READ BOTH OF THESE IN FULL BEFORE ANYTHING ELSE
1. ~/Code/p4gs/ADE-Bootstrapper/ISA.md — specifically `#### Phase J — Design System Rebuild` in
   `## Criteria` (ISC-304 through ISC-313) and EVERY `2026-08-02` entry in `## Decisions` (four
   of them, immediately above `## Changelog`) — they contain the reasoning and two rounds of
   correction behind every rule below. Also read the closing narrative section at the end of the
   file for the honest account of why this phase exists.
2. ~/Code/p4gs/ADE-Bootstrapper/Plans/resolute-charting-heron-operating-instructions.md — the
   order of operations, stack details, the reuse/licensing decisions, the Fable-5-specific
   operating instructions, the full per-ISC VERIFICATION design, stop conditions, and the two
   valid DONE states. This is not supplementary — it is the other half of this prompt, split out
   only because of a character-count cap on this field.

Where the ISA and either prompt file disagree, THE ISA WINS — flag the conflict, don't resolve
it by picking whichever reading is more convenient.

# TASK & SCOPE
Implement Phase J of the ADEB ISA on a feature branch, never `main`. In scope: ISC-304 through
ISC-311, in the order the operating-instructions file states. ISC-312 and ISC-313 are already
closed by the authoring session — not yours to re-verify. Out of scope: anything not named in
Phase J — don't touch `ade-core`'s non-GUI modules, `ade`'s CLI surface, or `ade-status` beyond
what a shared token crate genuinely requires.

# THE THREE RULES THAT MATTER MOST — everything else is in the operating-instructions file
1. **Stay on egui/eframe.** Not up for reconsideration in this run — the ISA's Decisions already
   settled it.
2. **ISC-310 (the human checkpoint before ISC-311's rollout) is a genuine, three-branch stop —
   not a formality, and never satisfiable by kittest-green or your own narrative claim alone.**
   The operating-instructions file names all three branches exactly, including what to do when
   live screenshot capture is unreliable (a known, documented condition on this machine) and
   what to do when no human-response mechanism exists at all. Read it before you reach this
   point, not when you get there.
3. **Ground every progress claim in a tool result before reporting it, every time.** This project
   has repeatedly declared "verified good" work that wasn't. The operating-instructions file
   gives you Anthropic's own tested technique for this — use it verbatim, not paraphrased.

You are operating autonomously — no one is watching in real time, don't ask permission for
reversible actions, don't end a turn on a plan or promise instead of doing the work. Effort:
`xhigh`. Use fresh-context subagents readily and at fixed verification intervals, per the
operating-instructions file.

Begin now: read both files in full, then start on ISC-304.
```
