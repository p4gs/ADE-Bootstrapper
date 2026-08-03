# UI design reference — how the Control Center gets looked at

## The problem this file exists to solve

Every visual defect this app has shipped reached the screen because nobody saw
the frame first. Not the owner, not the agent writing the code. Layout was
changed by reasoning about `egui::Layout` and hoping — and hope produced a
17-row list with ragged row heights, a chip that swallowed a third of the width,
and `Uninstall` painted the loudest colour on the page, six times over.

The single highest-leverage practice in agent-assisted UI work is a visual
feedback loop: render, look, critique, iterate. The web ecosystem gets this free
through headless browsers. This app is Rust + egui, so it needs its own.

## The loop: `egui_kittest`

`egui_kittest` (dev-dependency of `ade-control-center`, version-locked to egui
0.35) renders the real UI headlessly to PNG inside `cargo test`. No window, no
window server, no focus stealing, no Interceptor.

```
cargo test -p ade-control-center                 # compare against baselines
UPDATE_SNAPSHOTS=1 cargo test -p ade-control-center   # re-record baselines
```

Baselines live in `crates/ade-control-center/tests/snapshots/` and are
committed. They are the reference — a diff there is a design change, and should
be looked at like any other diff.

**Two rules learned the hard way:**

1. **Prime a frame before painting.** `Context::set_fonts` takes effect on the
   *following* frame, so painting text in a named font family on frame one
   panics inside epaint. `row.rs`'s `harness()` helper burns frame one on font
   registration and requests a repaint.
2. **Snapshot states side by side, never one at a time.** Misalignment between
   rows is invisible in isolation and obvious in a stack. `rows_all_states`
   deliberately renders healthy / update-available / missing / manual / broken /
   plugin / disabled / busy in one image.

`egui_kittest` is built on AccessKit, so the same harness queries the
accessibility labels the Interceptor drive uses. A snapshot test and an AX
invariance test are the same test.

## Why the row is a pure function

`row::capability_row` takes a `RowState` and returns an `Option<RowEvent>`. It
renders and reports; it never touches the engine. That is what makes it
renderable in a test with no background threads and no window — the purity is
not architectural taste, it is what buys the feedback loop.

## `egui-shadcn` — read, not adopted (decision open)

<https://github.com/oetiker/egui-shadcn> is the only egui-aware Claude Code
design plugin that exists. It ports shadcn-v4 (new-york / OKLCH) to egui:
theme + tokens, themed components (Button, Input, Label, Card, Tabs, Switch,
Checkbox, Select, Separator, Badge), and a **flexbox-substitute layout layer**.
Runtime deps are only `egui` + `egui_extras`.

**Why it is worth reading:** it independently arrived at the same kittest
self-correction loop adopted here, which is corroboration that the loop is the
right primitive. And its layout layer targets a real gap — egui has no flexbox,
which is exactly why the action cluster's left edge is still ragged while its
right edge is pinned.

**Why it is not adopted:** 0 stars, 30 commits, validated against one reference
screen. Vendoring a component module into this app is a supply-chain decision,
and this repo holds a 95/95 coverage gate and a `cargo deny` policy for reasons
that apply to its own dependencies too. Adopting shadcn's visual language would
also pull the Control Center further from macOS and toward the web, which is the
opposite of the stated direction.

**Revisit if:** the layout layer proves it can hold a fixed grid across
17 heterogeneous rows, or the project gains real adoption.

## The ceiling, stated plainly

No plugin makes egui look like a macOS 26 app. egui's own documentation says it
"doesn't know or care on what OS it is running" — no SF Pro, no SF Symbols, no
HIG control metrics, no real focus rings. Every macOS-native design skill in the
ecosystem (twostraws/SwiftUI-Agent-Skill at 4.4k stars, AvdLee's at 3.3k,
`claude-code-apple-skills`, `apple-skills`) assumes SwiftUI or AppKit and none
of it applies here.

The `frontend-design` plugin (Anthropic, official, now installed at user scope)
is still worth having: roughly 90% of its SKILL.md is framework-agnostic design
judgement — palette, type scale, structural hierarchy, and an explicit list of
the three looks AI defaults to. Take that half; ignore the CSS specificity
advice.

Porting the Control Center to SwiftUI is the only route to a genuinely native
look. `ade-core` would stay Rust and the tray is already AppKit. That decision
is open and belongs to the owner.
