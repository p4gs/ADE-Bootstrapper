# ADE Control Center — researched UX redesign (v2)

## Context

The owner rejected the Control Center's appearance five times; each response was a
styling pass that improved a row and left the screen unchanged. Before any
implementation, the owner mandated deep research: current macOS UX practice,
four reference apps (Cork and WailBrew most relevant — both are Homebrew GUI
managers, the closest structural analogues to an 18-capability bootstrapper),
plus an analysis of how Zed achieves its aesthetic in Rust. Hard constraint:
**no emojis anywhere in the UX** — they read as AI-generated.

Three parallel research passes were run (Cork/WailBrew source trees, Davit +
OrbStack + macOS 26 practice, Zed's `crates/ui`/`crates/theme` token files).
This plan is the synthesis. It supersedes the v1 restructure plan but keeps its
still-valid codebase findings, phase mechanics, and the passed Phase 0 glass
spike.

**What has already landed** (keep, don't redo): the coverage-aware verdict in
`ade-core` · `ade remove` · the PATH fix (`envpath.rs`) that made detection tell
the truth · the `egui_kittest` snapshot loop · a pure `row.rs` with
`RowState`/`RowEvent` · the Phase 0 glass spike.

---

## Research findings that drive the design

### From Cork (SwiftUI, the "System Settings" language)
- **Dashboard-first landing.** Cork opens on a Start Page that answers, in
  order: how many updates → anything broken/adoptable → inventory counts →
  cache junk. The inventory itself is parked elsewhere. For a bounded
  18-capability catalog, "what do I have" is boring; **"what needs attention"
  is the product.**
- **The four-state status box.** Cork's outdated-packages box has a *designed
  view for every state*: checking / has-items / all-clear / check-failed. Most
  tools design only the has-items state. Steal outright.
- **Fully-qualified destructive labels**: "Uninstall <exact name>", never a bare
  "Uninstall". Two-tier destruction (Uninstall vs Purge) with the nuclear
  option opt-in.
- **Zero emoji in UI** — SF Symbols only, semantic Dynamic Type only, color
  almost entirely semantic (one blue tag dot, one teal "outdated" pill).
- Animated count changes make a static dashboard feel live after an install.

### From WailBrew (the structure to keep, the skin to avoid)
- **Grouped sidebar nav with live count badges** and inline keyboard shortcuts
  is the right skeleton for us (Cork's sidebar-as-inventory assumes hundreds of
  rows; we have 18).
- **Dependents-as-chips confirm dialog**: before uninstall it fetches what
  depends on the package and shows *what breaks, by name*. Best destructive-
  action detail in either app. Our analogue is better: the verdict already
  computes coverage impact ("Secret scanning would have no working provider").
- **Live-streamed command log** for every mutating operation — "every command
  is transparent" is trust-critical for a tool that installs things.
- **Data-freshness indicator** (spinner while checking, quiet clock when idle).
- **What to avoid, verbatim from its source**: emoji as load-bearing iconography
  (every nav item, every table row, toasts, even localized strings), a web font
  (Nunito 17px), 18px radii, uppercase bold table headers — "a web app dressed
  in translucency, not an AppKit citizen."

### From Davit + OrbStack + macOS practice
- **Healthy is silent.** The "rainbow status list" is a named AI-slop tell:
  if 16 of 18 rows are green, the attention budget is spent on "nothing is
  wrong". Color/symbol only for degraded, missing, broken. Never color alone —
  status = symbol + color + text (≥3:1 contrast).
- **Speed is the UI.** OrbStack's premium reputation is ~60% speed: reviewers
  quote cold-start ("Wait, did it just open?!") and Activity Monitor CPU%, not
  layout. Sub-200ms first paint from cached state, detection streaming in
  progressively, beats any visual treatment.
- **Secondary actions collapse into a per-row `⋯` menu** (Davit); badges only
  for facts with no other home.
- **Preview before bulk apply** (Davit Compose import, Pearcleaner): a
  deselectable checklist with per-item facts before "Install all missing".
- **macOS type metrics, not iOS**: body is 13pt (not 17), captions 10–11,
  section headers 13–15 semibold, large title 26. Using iOS sizes is itself a
  tell. Monospaced digits for anything numeric that updates.
- **Liquid Glass is navigation-layer only** (sidebar/toolbar), never content;
  never glass-on-glass; sidebar icons are monochrome now, not accent-tinted.
- **The AI-generated ban list** (written down, enforced): no emoji · no
  gradients · no per-row colored badges · no colored left-border strips · no
  icon-in-pale-chip squares · no indigo #4F46E5 · no iOS type sizes · one
  accent (system-adjacent blue) · one icon language at one stroke weight ·
  no motion without meaning.

### From Zed (the transferable design system — actual values from `crates/ui`)
- **A tiny scale, rigorously obeyed.** 4 text sizes (10/12/14/16), icon sizes =
  the same numbers, spacing 0/1/2/3/4/6/8/12/16/20/24/32/40/48 (most resolution
  below 8px), effectively one radius (4px). Beauty from the *absence* of
  intermediate values.
- **Two-layer color**: 12-step neutral ramps (+ alpha variants) → semantic
  roles (`surface_background`, `element_hover`, `text_muted`, `border_variant`).
  Component code never touches a hex.
- **Interaction states are alpha washes** (neutral-alpha steps 3/4/5), never
  new opaque colors — one token composites correctly over any surface.
- **Selection is background; focus is border** — and the border box is
  reserved with a transparent stroke when unfocused, so nothing shifts.
- **Content is ramp step 1, chrome is step 2** — chrome recedes one notch,
  never "raised". Ghost elements are transparent until hovered.
- **Shadows: pure black, alpha 0.03–0.12, multi-layer** (2 for elevated, 4 for
  modal incl. a zero-blur 1px edge line), and **stronger in dark mode** than
  light.
- **Hierarchy from color and size, not weight** (Zed ships weight 400
  everywhere; `text` step 12 vs `text_muted` step 10 does the work).
- **egui-specific fidelity lever**: round every stroked rect to the pixel grid
  (`Rect::round_to_pixels`) — without it 1px hairlines blur across two device
  pixels at fractional DPI. Keep radii small (4–10px) where egui's tessellation
  is indistinguishable from Zed's SDFs. Keep shadows in Zed's low-alpha range
  where egui's approximation error hides.

---

## The design

### Shape (unchanged from v1, now research-validated)

```
┌──────────────┬────────────────────────────────────┐
│ ADE          │  Everything is covered             │
│              │  18 capabilities · 9 areas · no    │
│ Overview   2 │  action needed                     │
│              │                                    │
│ CAPABILITIES │  NEEDS ATTENTION ──────────────    │
│  Secret Scan │  | Gitleaks missing   [Install]    │
│  Sandboxing 1│  | nono outdated      [Update]     │
│  Hooks       │                                    │
│  … 6 more    │  COVERAGE ─────────────────────    │
│ Projects     │  Secret Scanning  TruffleHog 3.96  │
│ Activity     │  Sandboxing       needs attention  │
│              │  …                                 │
│ Checked 2m ○ │                                    │
└──────────────┴────────────────────────────────────┘
  220pt glass        content · opaque · step 1
```

1. **Sidebar = task-framed nav with count badges** (WailBrew skeleton, no
   emoji, no icons — text labels only, which sidesteps the drawn-icon quality
   risk entirely). Overview · the ten capability groups · Projects · Activity.
   Badges are muted tabular numerals; they gain the warn color only when the
   group needs attention. A freshness line sits at the sidebar bottom (HIG:
   sync status belongs in the sidebar's bottom bar).
2. **Overview is the landing page** (Cork Start Page pattern):
   - Verdict headline as a sentence (20pt) + one-line detail — already computed
     by `verdict.rs`.
   - **Attention box with four designed states**: checking (indeterminate
     progress + last-known items dimmed) / has-items (ranked list, one primary
     action each) / all-clear (a designed quiet state: small check glyph,
     "Everything is covered", muted) / check-failed (error text + Retry).
   - **Coverage list**: ten rows, group name + provider fact ("TruffleHog
     3.96.0"). Healthy rows are *silent* — plain text, no color, no glyph.
     Only problem rows get the status glyph + tint.
   - Counts animate on change (Cork).
3. **One capability page at a time**: the group's `why` prose as page subtitle;
   tool rows beneath; one primary action per row + `⋯` menu (Reinstall,
   Uninstall, Reveal path, Copy version).
4. **Trust surfaces**:
   - Confirm modal (uninstall): title "Uninstall TruffleHog", body states the
     *coverage consequence* from the verdict ("Secret scanning will have no
     working provider"), destructive button repeats the full name. Escalation
     (e.g. remove config) is an in-dialog checkbox, not a separate menu item.
   - **Live command log**: job cards in Activity stream captured stdout/stderr
     into a monospace, ink-surface panel. The seam already exists —
     `gui/jobs.rs` captures full stdout/stderr per job with `append_log` and a
     `log_tail`; the UI just needs to render it.
   - **Bulk preview**: "Install all missing" opens a deselectable checklist
     (item, method, what it fixes) before running (Davit/Pearcleaner pattern).
5. **Speed contract**: first paint renders instantly from persisted last-known
   state (`gui/state.rs`); detection results stream in and rows update in
   place; the freshness indicator carries the "checking…" state. No blocking
   spinner screen, ever.

### Design tokens (macOS metrics × Zed structure) — all in `theme.rs`

| Token | Value | Source |
|---|---|---|
| Spacing scale | 0/1/2/3/4/6/8/12/16/20/24/32 | Zed (fine-grained low end) |
| Type scale | 11 caption · 13 body · 15 section · 20 page title | macOS metrics (13pt body, NOT 17) |
| Weights | regular + semibold only; hierarchy mostly from color | Zed principle, macOS headers |
| Numerals | monospaced digits for versions/counts (no row jitter) | macOS practice |
| Neutral ramp | 12 steps + alpha variants (sand-warm neutral), light + dark | Zed/Radix |
| Semantic roles | `background` `surface` `elevated_surface` `element_*` `ghost_*` `text` `text_muted` `text_disabled` `border` `border_variant` `border_focused` + 3 status | Zed `ThemeColors` |
| Plane rule | content = step 1 · sidebar/chrome = step 2 (glass-tinted) | Zed |
| Hover/active/selected | neutral **alpha washes** (steps 3/4/5), 80–120ms fade | Zed |
| Focus | accent 1px border, **box reserved via transparent stroke** when unfocused | Zed |
| Radius | 5 control · 10 grouped container · 12 popover | macOS inset lists, Zed-small |
| Shadows | elevated: 2 layers · modal: 4 layers incl. zero-blur 1px edge · black α 0.03–0.12 · **stronger in dark** | Zed elevation.rs |
| Hairlines | every stroked rect pixel-grid-rounded (`round_to_pixels`) | egui fidelity lever |
| Sidebar | 220pt · 28pt nav rows · 13pt labels · 11pt semibold muted section headers | HIG |
| Content rows | 44pt (title + subtitle), built on `egui::Sides` | HIG + v1 |
| Toggle | 31×18pt track | HIG |

Enforcement: no view code touches a raw `Color32` or a literal size — semantic
roles and the scale only. The semantic color type gets **no `Custom` variant**
(Zed's own doc-comment philosophy, enforced structurally).

### Iconography — the no-emoji system

egui cannot load SF Symbols (no system font file exists) and emoji are banned,
so the app's entire icon language is **painted geometry via epaint**, one
stroke weight, three sizes aligned to the type scale (10/13/16):

- status-attention: filled 7px circle (warn tint)
- status-broken: painted rounded-join triangle + exclamation stroke (NOT ⚠)
- status-ok: painted check stroke (used sparingly — healthy is silent; the
  check appears only in the all-clear attention box and the confirm of a
  completed job)
- chevron, ellipsis (three painted dots), external-arrow, magnifier — as needed

**Current violations to remove** (audited): `⚠` U+26A0 — which macOS renders as
an orange emoji triangle — in `ade-core/src/gui/menubar.rs:108` (tray title!)
and `verdict.rs` glyph strings; 25 text-glyph `●` dots doing status work with
baseline problems. Tray gets a drawn template `NSImage` (or plain text + count);
`verdict.rs::glyph()` CLI output switches to ASCII (`ok` / `!` / `x` / `-`).
Add a CI grep gate over `crates/` GUI code banning emoji-range codepoints so
they can't return.

### Typography

Spike SF Pro (`/System/Library/Fonts/SFNS.ttf`, weight axis via skrifa —
half-day, gates nothing else) → **fall back to Inter** (already embedded;
regular + semibold only; delete the never-rendered 632KB `Inter-Bold.otf`).
Versions/counts use the mono face (`SFNSMono.ttf` if the spike passes, else a
small embedded mono subset) — monospaced digits everywhere numbers update.

---

## Beyond the restyle — metrics, insights, and personas

The redesign above makes ADEB legible; this section makes it *useful beyond
install state*. Today the app answers "is it installed and healthy". The
expanded use cases answer three more questions, one per persona:

| Persona | Question ADEB answers | Surfaces |
|---|---|---|
| **P1 — the AI-assisted developer** (default lens) | "Am I protected, what did my guardrails actually *do*, and what are they costing me?" | Overview verdict · interventions summary · per-tool overhead stats |
| **P2 — the security / GRC engineer** | "Prove the control exists **and operates**." Control-design evidence (installed, configured, current) + operating-effectiveness evidence (it ran, it caught things, when) | Coverage matrix · evidence export · intervention history |
| **P3 — the new-machine bootstrapper** | "Get me from zero to covered, repeatably." | Guided first run · bulk-install preview · profile export/import |

No persona switcher in the UI — personas are lenses, served by layering:
Overview for P1's glance, capability pages + matrix + export for P2's depth,
first-run + bulk flows for P3. (Fleet/team management is explicitly out of
scope — ADEB stays a single-machine tool; the profile export is the escape
hatch for teams.)

### The metrics catalog — per capability group, tiered by data availability

Honesty rule: **a metric renders only when its data source is real.** No
placeholder zeros, no fabricated stats — an absent metric is absent. Tiers
stage the work:

- **Tier 1 — already have** (engine detection, `jobs.json`, project reports):
  version · latest version + update lag in days · install method · last
  install/update job outcome · project coverage count · detection freshness.
- **Tier 2 — cheap new probes** (one command or one stat call, no log
  ingestion): the per-group probes below.
- **Tier 3 — intervention ingestion** (parse tool output/reports retained from
  hook runs and scans): the "what did it catch" numbers. Per-tool adapters,
  shipped last, only where the tool already writes something parseable.

| Group | Tier-2 probes | Tier-3 interventions |
|---|---|---|
| Secret scanning | repos wired for commit-boundary scanning; last scan age per repo | secrets caught (TruffleHog/Gitleaks findings), by repo and week |
| Hook orchestration | projects with hooks installed; **hook-chain latency** (time a no-op run — "your commits pay 340ms") | hook failures blocked commits count |
| Dependency scanning | OSV database age; last audit age per project | advisories found / blocked versions |
| Sandboxing | config present; sandbox binary invoked recently (process/log mtime) | denials / blocked operations |
| Agent security rules | CodeGuard ruleset version vs upstream release; surfaces covered (Claude Code, Codex, Cursor…) | — (rules shape generation; no event stream) |
| Repo hygiene | repos with hardened git config (probe `git config`); policy drift count | force-push/history-rewrite blocks |
| Token efficiency | **`rtk gain`** — tokens saved, % reduction (the tool ships this stat today) | — (gain *is* the intervention metric) |
| Codebase wiki | indexed projects; index staleness (mtime vs HEAD); wiki size | — |
| Semantic search | indexed repos; index staleness; index size on disk | — |
| Coding harness | versions + update lag; recently-used detection (existing `process_names`) | — |

### The insights engine — metrics become sentences with one action

Same architecture as `verdict.rs` (pure function over facts, unit-testable,
shared by GUI/tray/CLI): `build_insights(facts) -> Vec<Insight>` where
`Insight { headline, evidence, action, rank }`. Every insight is a plain
sentence naming its evidence and carrying exactly one action:

- "2 of 5 projects have no commit hooks — secret scanning only covers wired
  repos." → *Wire them* (opens bulk preview scoped to those projects)
- "RTK saved 1.2M tokens this month (38% reduction)." → no action; value
  affirmation (the Cork animated-count moment)
- "OSV database is 21 days old — new advisories since then aren't checked." →
  *Refresh*
- "CocoIndex for `nthpartyfinder` is 14 days behind HEAD." → *Reindex*
- "nono is installed but nothing ran sandboxed this week." → adoption gap,
  links to the group's `why`
- "3 tools are more than 60 days behind their latest release." → *Update all*
  (via bulk preview)

Placement: insights render **below hard failures** in the Overview attention
box (broken > uncovered > insight — extends the existing `AttentionRank`), and
the relevant subset repeats on each capability page. Ranked, capped (top 5 on
Overview), dismissible per-insight with persistence in `gui/state.rs`.

### The coverage matrix — Projects becomes evidence

The Projects page becomes a **projects × capability-groups grid**: each cell
= wired / not wired / not applicable, from the existing per-project reports.
Cell click offers the wire action. Healthy cells silent (no green wall —
a small muted mark), gaps get the attention tint. This is P2's
control-coverage view and P1's "which repo am I naked in" view in one surface.

### Evidence export — the posture report

`ade export posture` (CLI) + "Export report" on Overview (GUI): a
markdown + JSON pair containing verdict, per-capability state (version,
method, health, config facts), the coverage matrix, intervention counts where
available, and timestamps — a point-in-time attestation of the development
environment's guardrails, suitable as GRC evidence. Deterministic ordering,
no identity/paths beyond `~`-relative (release-containment rules apply).

### First run — the guided path (Davit's lesson)

First launch must not be eighteen "Not installed" rows. It becomes: verdict →
"Set up your environment" → bulk-install preview (deselectable, methods shown)
→ live log while running → the all-clear state as the payoff. Same components
as the steady-state app — the guided path is the bulk flow with a welcome
sentence, not a separate wizard.

---

## What the codebase actually allows (from exploration — unchanged, still true)

- **`ade-control-center` is a binary-only crate** — every test lives in
  `#[cfg(test)] mod tests` inside `src/`, as `row.rs` already does.
- **`eframe::App` is `ui(&mut self, ui, _frame)`** — drops into
  `Harness::build_ui` with no adapter.
- **Coupling is light**: `activity_view`+`job_card` make zero engine calls; the
  tab bar mutates only `self.tab`; real coupling concentrates in `projects`
  (5 calls) and `header` (3).
- **`Shared` is `Default` but not `Clone`** — `app.rs:297-312` hand-copies 11
  fields; derive `Clone`, collapse to `guard.clone()`.
- **The current screen snapshot is not pixel-faithful** (heading drawn via a
  different code path than the app). Every extracted view gets a snapshot
  rendered through the same function the app calls.
- **Delete on the way through**: `ProjectReport::error` (unreachable),
  `FAMILY_BOLD`/`Inter-Bold.otf` (632KB embedded, never rendered), the direct
  `engine.shared.lock()` mutation at `app.rs:463`.

---

## Phases

Each phase ends with a **real-window capture** (verify the `*` in
`interceptor macos apps` before trusting any capture). Ship each before
starting the next. Nothing commits without the owner's Secretive tap.

**Phase 0 — precondition (~1h).** Derive `Clone` on `Shared`; move layout
constants into `theme.rs`; extract one `section_surface()`; delete the dead
code. Nothing visual changes.

**Phase A — the design system.** The token table above into `theme.rs`: ramp +
semantic roles rewiring `egui::Visuals`; alpha-wash interaction states; focus
ring with reserved box; pixel-grid hairline helper; shadow recipes; spacing/
type enums (no literals in view code — clippy `disallowed-methods` on
`Color32::from_rgb` in view modules where feasible, else a review gate).
Rebuild `row.rs` on `Sides` with the new tokens. SF Pro spike happens here.
*Visible win: rows stop looking arbitrary.*

**Phase B — the sidebar shell.** Text-only nav with count badges + freshness
footer; delete tab strip, group-by toggle, repeated headings. Split `app.rs`
cheapest-coupling-first, each view a free function returning `Option<Event>`
with a same-commit snapshot test: 1 `activity.rs` → 2 `nav.rs` → 3
`overview.rs` → 4 `capability_page.rs` → 5 `header.rs` → 6 `projects.rs`
(`project_input` needs `&mut String` — the one escape hatch).

**Phase C — Overview.** The Cork pattern: verdict sentence · four-state
attention box (checking/items/all-clear/failed — all four designed and
snapshot-tested) · silent-healthy coverage list · animated counts · freshness
indicator wired to real detection state.

**Phase D — capability page + trust surfaces.** Group `why` as subtitle; one
primary action + `⋯` popup (contents constructed unconditionally so the
AccessKit tree stays invariant); the coverage-consequence confirm modal; the
bulk-install preview checklist; job log tail in Activity (verify what output
the engine captures; add the seam if absent).

**Phase E — glass.** Phase 0 spike proved Tier 1. `chrome.rs` (pure tier
selection, tested in core) + `chrome/macos.rs` (all `unsafe`). Gate on class
existence. Sidebar only — never content, never glass-on-glass.

**Phase F — icon, motion, de-emoji sweep.** Painted-geometry icon set; tray
template image replacing `⚠`; `verdict.rs` ASCII glyphs; CI emoji-ban grep;
app icon → `sips`/`iconutil` → `.icns`; ad-hoc `codesign --sign -` so TCC
grants survive rebuilds. Motion: hover fade 100ms, action reveal 120ms, sheet
220ms, verdict change 260ms; navigation stays instant; no motion without
meaning.

**Phase G — metrics foundation.** Tier-1 facts surfaced (update lag, project
coverage, last job, freshness) as a stats strip on the capability page —
absent-when-unreal. Tier-2 probes in `ade-core` behind the existing `ExecFn`
seam (one module: `gui/stats.rs`, pure `assess` over probed facts, same
pattern as `inventory.rs`): `rtk gain` parse, OSV db age, index staleness,
hook-chain latency, CodeGuard surfaces, hardened-repo probe.

**Phase H — insights + matrix.** `gui/insights.rs` (`build_insights`, pure,
extends `AttentionRank`); Overview renders top-5 below hard failures with
dismiss persistence; capability pages show their subset; Projects page becomes
the coverage matrix with cell wire actions.

**Phase I — evidence + onboarding.** `ade export posture` (MD+JSON,
deterministic, `~`-relative paths only) + Overview export button; first-run
guided path assembled from the bulk-preview + live-log components; Tier-3
intervention adapters *only* where a tool already writes parseable output
(start: Gitleaks/TruffleHog report files from hook runs) — each adapter its
own small PR-sized slice.

**Cut** (unchanged): group-by toggle · Tools/Harnesses split · per-row
Uninstall/Reinstall buttons · per-row enable toggle (moves to capability page)
· the `(i)` tooltip (its prose becomes the page subtitle) · `v0.2.0` in the
header.

---

## Verification

**Standing gates every phase**: `cargo fmt --check` · `cargo clippy
--all-targets -D warnings` · `cargo test --workspace` ·
`scripts/coverage-check.sh` (95/95) · `cargo deny check` ·
`scripts/parity-check.sh`.

**The capture protocol** (exists because three verification failures happened
on this task — no image, wrong subject, wrong width):
1. Snapshot via `egui_kittest` at the window's real logical width (~900–1000pt).
2. Real window before any done-claim: `interceptor macos apps` → confirm the
   `*`, then `interceptor macos screenshot --mode display --save`.
3. Positive control on every absence.
4. AX-tree invariance resting vs hovered.
5. Machine truth over pixels for behaviour (`which`, `ade gui health`,
   `jobs.json`).

**New ISCs (from ISC-247)** cover: token scale used everywhere (no literal
sizes/colors in view code) · `Sides` rows hold alignment at 700/900/1400pt ·
sidebar selection persists across restart · `⋯` AX labels exist while closed ·
all four attention-box states render designed (snapshot each) · healthy
coverage rows contain zero status pigment (pixel assertion) · **zero
emoji-range codepoints in GUI crates (grep gate in CI)** · hairlines land on
the pixel grid at 1x and 2x · first paint from cached state with detection
still pending (kittest with a stalled engine stub) · confirm modal names the
capability and states the coverage consequence · monospaced digits in version
strings (no width change across `1.1.1`→`10.10.10`).

**Feature-phase ISCs (G–I)**: a metric never renders without a real data
source (kittest: stub with absent facts → stats strip omits, no zeros) ·
`rtk gain` output parses across its real format (fixture from the actual
binary) · every insight names evidence and carries ≤1 action (type-enforced) ·
insight dismissal persists across restart · matrix cells reflect the same
project reports the CLI prints (parity assertion) · posture export is
byte-deterministic for identical state and contains zero absolute home paths
(grep gate, same class as the release-containment rules) · first-run with
nothing installed reaches the bulk preview in one click · Tier-3 counts match
a hand-counted fixture report exactly — never estimated.

`.gitignore` gains `crates/ade-control-center/tests/snapshots/*.new.png` and
`*.diff.png`.

---

## Honest limits

- The defensible claim stays *"the navigation layer is a genuine system
  material; the content layer is egui"* — one real glass surface (sidebar),
  no per-control glass, no SF Symbols (painted geometry instead), no morphing.
- egui vs GPUI: no SDF-analytic curves or error-function shadows — mitigated by
  small radii, low-alpha shadows, and pixel-grid rounding, which keep the
  difference imperceptible at this design's values.
- Reduce Transparency / Increase Contrast adapt the native backdrop only;
  egui-drawn content re-derives by hand (poll `NSWorkspace` accessibility
  settings — follow-up, not in the first pass).
- The app icon will lack Tahoe's layered treatment (needs Icon Composer).

## Phase 0 glass spike — PASSED (2026-07-26, macOS 26.5.2) — still valid

Evidence in `scratchpad/evidence/spike-0{1,2,3}`; spike preserved as
`phase0-spike.patch` + `chrome-spike.rs`. Transparent surface PASS ·
`setZPosition(-1.0)` draws behind egui PASS · `hitTest -> nil` passes clicks
PASS · `NSGlassEffectView` renders with empty `contentView` PASS.
Build notes: `objc2-app-kit` needs `"objc2-quartz-core"` +
`"objc2-core-foundation"` features; `raw-window-handle = "0.6.2"` direct dep;
`objc2-quartz-core` has no `"objc2"` feature.

## Critical files

| File | Change |
|---|---|
| `crates/ade-control-center/src/theme.rs` | ramp + semantic roles + scales + shadows + SF Pro loader/Inter fallback |
| `crates/ade-control-center/src/row.rs` | rebuild on `Sides` with tokens |
| `crates/ade-control-center/src/app.rs` | shrink to shell/state/frame |
| `.../src/{nav,overview,capability_page,projects,activity,widgets,icons}.rs` | **new** — free functions + painted-geometry icons, snapshot-tested |
| `.../src/chrome/{mod,macos}.rs` | **new** — all `unsafe` AppKit |
| `crates/ade-core/src/gui/chrome.rs` | **new** — pure tier selection |
| `crates/ade-core/src/gui/verdict.rs` | glyph() → ASCII; keep computation; extend `AttentionRank` for insights |
| `crates/ade-core/src/gui/stats.rs` | **new** — Tier-1/2 metric probes, pure `assess` |
| `crates/ade-core/src/gui/insights.rs` | **new** — `build_insights`, pure, testable |
| `crates/ade-core/src/gui/posture.rs` | **new** — deterministic evidence export (MD+JSON) |
| `crates/ade-core/src/gui/menubar.rs` | de-emoji tray title |
| `crates/ade-status/src/main.rs` | template NSImage status item |
| `scripts/bundle-apps.sh` | icon, `NSRequiresAquaSystemAppearance=false`, ad-hoc sign |
| CI workflow | emoji-ban grep gate |
