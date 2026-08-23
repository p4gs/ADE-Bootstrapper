---
type: Architecture Guide
title: Design system and visual verification
description: Why colour tokens live in the core rather than the app, contrast enforcement that returns the honest maximum, and a snapshot loop with two hard-won rules.
tags: [design-tokens, contrast, chrome, snapshots]
sources:
  - id: openwiki-source-f564c2199da8aff475b38135
    resource: repo://crates/ade-control-center/src/icons.rs
  - id: openwiki-source-17a84b293326a8829ef8c902
    resource: repo://crates/ade-control-center/src/testkit.rs
  - id: openwiki-source-046c84764ce5d802736a5c0d
    resource: repo://crates/ade-control-center/src/theme.rs
  - id: openwiki-source-0225a4b4e64b407c752e53b5
    resource: repo://crates/ade-core/src/gui/chrome.rs
  - id: openwiki-source-0b0b8383b69a4aab3d4eb3cd
    resource: repo://crates/ade-core/src/gui/tokens.rs
  - id: openwiki-source-7d6ec197ec0726777bdaf59a
    resource: repo://docs/UI-DESIGN-REFERENCE.md
  - id: openwiki-source-f5f66cd5930b37f7b967e230
    resource: repo://scripts/coverage-check.sh
  - id: openwiki-source-480cd6b9df05eb395fc288cc
    resource: repo://scripts/emoji-ban.sh
generated: {by: "claude-code", at: "2026-08-22T21:06:03.708Z"}
verified:
  - by: openwiki/0.3.3
    at: 2026-08-22T21:06:03.708Z
---

# Design system and visual verification

Rust-only; no TypeScript counterpart.

## Why the tokens live in the core

All colour data lives in the engine crate as plain byte structs, so the CLI never links a
rendering stack. But the reason the split is **load-bearing rather than taste** is the
coverage gate: both desktop crates are excluded from it by filename regex and the engine
crate is not. A colour decision moved into the app crate would leave the 95/95 gate
entirely; the same decision in the token module is measured.

The role vocabulary is a closed 34-variant enum whose resolution is total over role and
appearance, with **no custom-colour escape hatch** — deliberately, because a well-known
reference implementation carries one. View code cannot hit a missing token.

The neutral ramp is a deliberately **mixed identity**: one family in light, a different
one in dark, rather than one family with two variants. Surface planes diverge structurally
too, not merely in value — the light card is pure white on a tinted canvas, with controls
shifted a step darker in lockstep.

Contrast enforcement is a real standards implementation plus a binary-search repair that
blends toward whichever pole contrasts better, searches for the smallest hue-preserving
adjustment, and **returns the honest maximum when the target is unreachable** rather than
failing silently.

## One boundary, held by convention

The theme module is the single sanctioned conversion point from token to render colour,
and that boundary currently holds — a repository-wide search finds raw colour constructors
nowhere else in either desktop crate.

**But that gate is not in CI.** The Rust job runs formatting, lint, tests, dependency
policy, the glyph ban and coverage — and nothing else. The invariant is held today by
convention plus a pinned-baseline test, not by a build failure.

That pinned test is a hardcoded fixture with a stated authority per value, and it refuses
to derive anything from the token layer because doing so would make it circular. Its
contract is explicit: a failure without a new recorded decision is a regression, not a
re-baselining opportunity.

## Chrome selection

The material tier is selected by **class existence rather than operating-system version
parsing**, and a reduced-transparency preference overrides every tier before any class is
considered. The one unsafe platform block in the app crate degrades every failure path to
the opaque tier, keeping the painted fallback so nothing looks broken.

Two cross-crate constants are bound by test rather than by comment: the sidebar width
against the native strip width, and the window-control inset against the drag band.

The icon language is painted geometry with one stroke weight and three sizes locked to the
type scale. Emoji are banned by a CI gate that also bans the geometric status marks.

One composition bug is worth knowing because it shipped into a golden image and was caught
only by measuring pixels: render colours store premultiplied channels, so reading a
channel off a translucent tint and multiplying by alpha again double-applies it and washes
every status tint toward grey. The blend helper unmultiplies first.

## The snapshot loop, and its two rules

The design reference records both, and both are defensive rather than incidental. **Prime
a frame before painting**, because font registration takes effect on the *following*
frame and painting a named family on frame one panics inside the renderer. And **snapshot
states side by side, never one at a time**, because misalignment between rows is invisible
in isolation and obvious in a stack.

The harness runs a fixed step count rather than to quiescence, because a row with a job in
flight paints a spinner that requests a repaint every frame, so the interface never goes
quiet.

**36 baselines are committed but only 35 are produced by a test.** One has no
corresponding snapshot call anywhere in the tree — an orphan that nothing compares against
and no refresh run will update.

## Gaps stated rather than hidden

Dark-mode accent text is held to a lower contrast bar than light, and the comment says so:
a real, narrow gap left open on purpose rather than perturbing a reviewed baseline as a
side effect of an unrelated fix. One surface is excluded from the status-text contrast
assertions on purpose, justified by a claim about call sites that nothing mechanically
enforces. And the role table's runtime half checks only for duplicates and resolvability,
so a new variant breaks the build but is not forced into the gallery.

A launch-time opaque tier cannot gain a material mid-run, because the effect view was
never attached. That is documented, not hidden.

## Source map and tests

`crates/ade-core/src/gui/{tokens,chrome}.rs` and
`crates/ade-control-center/src/{theme,icons,gallery,testkit,chrome}`, plus
`docs/UI-DESIGN-REFERENCE.md` and the committed baselines. Focused tests:
`ensure_contrast_repairs_arbitrary_accents_on_real_surfaces`,
`ensure_contrast_returns_the_pole_when_the_target_is_unreachable`,
`pinned_baseline_values_are_unchanged`, `text_roles_hold_aa_contrast`,
`reduce_transparency_forces_opaque_over_everything`, and
`blend_over_composites_unmultiplied_channels`.
