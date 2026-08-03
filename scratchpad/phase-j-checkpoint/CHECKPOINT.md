# ISC-310 Human Checkpoint — Phase J, pre-rollout (branch 2: degraded, capture-unavailable)

Date: 2026-08-02. Branch: `feat/phase-j-design-system`. ISC-304..309 closed with evidence,
each independently verified by a fresh-context agent.

## Why this is the degraded branch, stated plainly

Live screenshot capture on this machine is currently unreliable under load — ISC-303
(same ISA, documented 2026-08-02) recorded byte-identical broken chrome-only renders across
two genuinely distinct processes at load average 100-124. At package-assembly time `uptime`
read **228** — attempting capture would spend the sparing-capture budget on a condition
already proven broken, so per ISC-310's own branch-2 design: the deterministic kittest suite
plus precise written descriptions of every native-chrome change, presented honestly instead
of a fabricated "live" screenshot.

## What you are actually approving

**The design system, before it restyles anything.** The live screens are deliberately
PIXEL-IDENTICAL to what shipped at Phase I close — all 31 pre-existing kittest goldens pass
byte-for-byte, which is the proof the token refactor changed nothing visually yet. What's
new and reviewable:

1. **`gallery.png` / `gallery_light.png`** — THE design system made visible. Every color
   role (25, both appearances, hex), a live contrast audit (all floors green), type scale,
   both spacing scales, radii/elevation, motion constants, every component state. This
   gallery is what ISC-311 will roll onto every screen. **This is the main thing to react
   to.** In the live app: Cmd+Shift+G.
2. **`screen_overview_dark.png` / `screen_overview_light.png` / `capability_page_insights.png`**
   — the CURRENT screens, unchanged, as the baseline the rollout starts from.

## The native-chrome changes (written descriptions — the branch-2 substitute for live capture)

- **Focus rings:** keyboard focus on nav rows now paints macOS's actual two-layer look — a
  soft 3px accent halo floating 1.5px OUTSIDE the row's edge at 35% opacity, plus the
  existing crisp 1px accent line inside. Before: only the 1px line, which read as a border
  change rather than focus. Visible in the gallery golden's focus demo (bottom section,
  right rect vs left).
- **Titlebar drag band:** the melted titlebar (top 34pt, full width) is now a real titlebar
  behaviorally — press-and-drag moves the window, double-click zooms (maximize toggle).
  Registered beneath every real control, so buttons/rows always win the press; only bare
  band drags. Before: hiding the titlebar had silently removed both affordances entirely.
- **Menu bar (ADE Status tray):** the three action rows now carry native Command key
  equivalents, visible at each row's trailing edge exactly like every native menu —
  Cmd-R Refresh Now, Cmd-O Open Control Center, Cmd-Q Quit ADE Status.
- **Vibrancy:** toggling System Settings → Accessibility → Reduce Transparency now takes
  effect IMMEDIATELY in a running app, both directions — the native glass material hides
  and the sidebar paints its opaque fill (and back). Before: the setting was only read at
  launch. One honest limit, stated in code too: an app LAUNCHED with Reduce Transparency on
  starts Opaque and cannot upgrade to glass live (no effect view was ever attached) — that
  direction needs a relaunch.

## The question

Does this design system — the token vocabulary the gallery shows, plus the chrome behaviors
described above — look right to roll out across every screen (ISC-311: Overview first, then
Capability detail, Projects, Activity, modals, nav chrome, each its own gated commit)?
Or does it need another pass first?

Per ISC-310: revise-and-resubmit is bounded at 2 cycles; a genuine owner verdict (quoted, not
paraphrased) gets logged to `## Decisions` before ISC-311 begins. Nothing proceeds until then.
