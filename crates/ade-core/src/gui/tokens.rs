//! Design tokens — the single source of truth for color, spacing, and shape.
//!
//! Phase J (ISA ISC-304..307). Three layers, all pure data with zero GUI
//! dependencies so the `ade` CLI never links a rendering stack and the logic
//! is coverage-counted in this crate:
//!
//! - **Color**: a semantic [`ColorRole`] enum (Zed's `crates/ui` pattern —
//!   view code asks for *meaning*, never a hex) resolved against an
//!   [`Appearance`] by [`resolve`]. The neutral data is a deliberate
//!   MIXED-RAMP identity (ISC-310 cycle-1 revision, owner-directed — the
//!   pure-gray dark read "bland and uninspired"): dark is Radix `slateDark`
//!   (the cool, blue-tinted family Zed's One / Warp / Linear / Raycast all
//!   live in), light stays Radix `sand` (warm paper). Both fetched verbatim
//!   from radix-ui/colors (MIT). Status/accent solids unchanged.
//! - **Contrast**: WCAG relative-luminance math and an auto-contrast repair
//!   function ([`ensure_contrast`]) in the spirit of Warp's
//!   `foreground_color_with_minimum_contrast` (itself modeled on Chromium's
//!   color_utils) — REIMPLEMENTED from the published algorithm description,
//!   no Warp code copied (warpui_core is MIT but drags 25+ internal crates;
//!   ISA Decisions 2026-08-02).
//! - **Metrics**: `Spacing`/`CornerRadii` with `Density`/`Roundness` presets.
//!   Shape (field names, step semantics, preset enums) follows
//!   `cosmic-theme` (pop-os/libcosmic, MPL-2.0) — a CLEAN-ROOM
//!   reimplementation from its documented public values, taken under the
//!   ISC-305 fallback because this repo's `deny.toml` is permissive-only by
//!   stated policy ("Copyleft is absent from the tree") and vendoring MPL
//!   files or the crate would either violate that policy or make the license
//!   gate vacuous. Standard-density and Round values are cosmic-theme's own
//!   published numbers; Compact/Spacious/SlightlyRound/Square variants anchor
//!   to cosmic's documented example values with ADEB-chosen intermediates,
//!   stated as such rather than passed off as cosmic's.
//!
//! `theme.rs` in `ade-control-center` is the ONLY sanctioned resolution layer
//! from these tokens to `egui::Color32` — the ISC-304 grep gate holds every
//! other file to zero raw color constructors.

/// Light or dark appearance. The token layer is appearance-keyed so the
/// resolver, not view code, owns the "which variant" decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Appearance {
    Light,
    Dark,
}

/// A straight (non-premultiplied) sRGBA color, `[r, g, b, a]`.
///
/// Pure data — the GUI crates convert to their renderer's type at the
/// resolution layer. Alpha 255 is opaque.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rgba(pub [u8; 4]);

impl Rgba {
    pub const fn r(self) -> u8 {
        self.0[0]
    }
    pub const fn g(self) -> u8 {
        self.0[1]
    }
    pub const fn b(self) -> u8 {
        self.0[2]
    }
    pub const fn a(self) -> u8 {
        self.0[3]
    }
}

/// Opaque color from a `0xRRGGBB` literal — mirrors the `c()` helper the
/// ramp data was originally authored with in `theme.rs`, so the values below
/// are byte-comparable against Phase I's source.
const fn rgb(rgb: u32) -> Rgba {
    Rgba([(rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8, 255])
}

/// Translucent color from a `0xRRGGBBAA` literal — mirrors `ca()`.
const fn rgba(rgba: u32) -> Rgba {
    Rgba([
        (rgba >> 24) as u8,
        (rgba >> 16) as u8,
        (rgba >> 8) as u8,
        rgba as u8,
    ])
}

// ───────────────────────── the neutral ramp ─────────────────────────
// Radix "sand" (MIT, radix-ui/colors), light + dark, solid + alpha variants.
// Step semantics (Radix's own): 1-2 app backgrounds · 3-5 element rest/hover/
// active · 6-8 borders · 9-10 solids · 11-12 text. Values identical to the
// ones `theme.rs` carried at Phase I close (d5b1ba8) — moved, not changed;
// `theme.rs`'s pinned-palette test is the cross-crate proof.

const SAND_LIGHT_STEPS: [Rgba; 12] = [
    rgb(0xfdfdfc),
    rgb(0xf9f9f8),
    rgb(0xf1f0ef),
    rgb(0xe9e8e6),
    rgb(0xe2e1de),
    rgb(0xdad9d6),
    rgb(0xcfceca),
    rgb(0xbcbbb5),
    rgb(0x8d8d86),
    rgb(0x82827c),
    rgb(0x63635e),
    rgb(0x21201c),
];

const SAND_LIGHT_ALPHA: [Rgba; 12] = [
    rgba(0x55550003),
    rgba(0x25250007),
    rgba(0x20100010),
    rgba(0x1f150019),
    rgba(0x1f180021),
    rgba(0x19130029),
    rgba(0x19140035),
    rgba(0x1915014a),
    rgba(0x0f0f0079),
    rgba(0x0c0c0083),
    rgba(0x080800a1),
    rgba(0x060500e3),
];

/// Radix `slateDark` — the cycle-1 dark neutral (see module docs). Fetched
/// verbatim from radix-ui/colors `src/dark.ts` on 2026-08-02.
const SLATE_DARK_STEPS: [Rgba; 12] = [
    rgb(0x111113),
    rgb(0x18191b),
    rgb(0x212225),
    rgb(0x272a2d),
    rgb(0x2e3135),
    rgb(0x363a3f),
    rgb(0x43484e),
    rgb(0x5a6169),
    rgb(0x696e77),
    rgb(0x777b84),
    rgb(0xb0b4ba),
    rgb(0xedeef0),
];

/// Radix `slateDarkA`, same source and fetch.
const SLATE_DARK_ALPHA: [Rgba; 12] = [
    rgba(0x00000000),
    rgba(0xd8f4f609),
    rgba(0xddeaf814),
    rgba(0xd3edf81d),
    rgba(0xd9edfe25),
    rgba(0xd6ebfd30),
    rgba(0xd9edff40),
    rgba(0xd9edff5d),
    rgba(0xdfebfd6d),
    rgba(0xe5edfd7b),
    rgba(0xf1f7feb5),
    rgba(0xfcfdffef),
];

/// Solid neutral-ramp step, 1-based (Radix's own numbering — every doc that
/// describes the ramps counts from 1, so the API does too). Which FAMILY the
/// appearance resolves to is the mixed-ramp identity: sand by day, slate by
/// night.
pub const fn neutral_step(appearance: Appearance, n: usize) -> Rgba {
    match appearance {
        Appearance::Light => SAND_LIGHT_STEPS[n - 1],
        Appearance::Dark => SLATE_DARK_STEPS[n - 1],
    }
}

/// Alpha-variant neutral step, 1-based. The same ramp as translucent
/// overlays — what makes interaction washes composite correctly over any
/// surface.
pub const fn neutral_alpha_step(appearance: Appearance, n: usize) -> Rgba {
    match appearance {
        Appearance::Light => SAND_LIGHT_ALPHA[n - 1],
        Appearance::Dark => SLATE_DARK_ALPHA[n - 1],
    }
}

// ───────────────────────── status/accent solids ─────────────────────────
// Dark values are the Phase I shipped literals (visually reviewed, the
// baseline for every existing snapshot). Light values are each hue's AA-safe
// Radix-derived step, chosen and documented in the Polish phase.

const ACCENT_DARK: Rgba = Rgba([76, 125, 255, 255]);
const OK_DARK: Rgba = Rgba([47, 191, 118, 255]);
const WARN_DARK: Rgba = Rgba([226, 156, 60, 255]);
const ERR_DARK: Rgba = Rgba([224, 92, 92, 255]);
const INFO_DARK: Rgba = Rgba([110, 163, 224, 255]);

const ACCENT_LIGHT: Rgba = rgb(0x3e63dd);
const OK_LIGHT: Rgba = rgb(0x1d5f42);
const WARN_LIGHT: Rgba = rgb(0x7d4c11);
const ERR_LIGHT: Rgba = rgb(0x99212a);
const INFO_LIGHT: Rgba = rgb(0x0f5399);

/// Semantic color roles — the app's complete color vocabulary. View code
/// (through the resolution layer's `Palette`) speaks these names; the values
/// live here and nowhere else.
///
/// The names ARE the design decisions: adding a variant is a design-system
/// change, not a convenience. There is deliberately no `Custom(Rgba)` escape
/// hatch — Zed's own enum carries one and its doc comment begs people not to
/// use it; omitting it entirely is the stronger version of that plea.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorRole {
    /// Content plane — ramp step 1. The extreme end; everything else recedes.
    Bg,
    /// Chrome/surface plane — step 2, one notch back. Sections, headers.
    Panel,
    /// Sunken content (log panels) — step 1 again: content, not chrome.
    Inset,
    /// Element at rest (buttons, inputs) — solid step 3.
    Widget,
    /// Element hovered — solid step 4.
    WidgetHover,
    /// Element pressed — solid step 5.
    WidgetActive,
    /// Border on non-interactive containers — step 6.
    Line,
    /// Hairline between rows — alpha step 4.
    Hairline,
    /// Ghost-element hover wash — alpha step 3.
    RowHover,
    /// Ghost-element active wash — alpha step 4.
    GhostActive,
    /// Selected wash (nav rows, list selection) — alpha step 5.
    Selected,
    /// High-contrast text — step 12. The "Default" role in Zed's vocabulary.
    Default,
    /// Secondary text — step 11 (the lowest step that holds AA at 11px).
    Muted,
    /// Tertiary/metadata text — also step 11; the third level of hierarchy
    /// comes from size and case, not a third grey.
    Faint,
    /// Disabled text — step 9. Exempt from AA by role; never used for
    /// information.
    Disabled,
    /// Primary-action fill, used under `OnAccent` text.
    Accent,
    /// Text painted on an `Accent` fill — white in both appearances.
    OnAccent,
    /// Healthy/success role.
    Success,
    /// Attention role.
    Warning,
    /// Error role.
    Error,
    /// Informational / "this is clickable" role.
    Info,
    /// Toggle-switch knob — white in both appearances.
    Knob,
    /// Toggle-switch track in the off position — the one appearance-varying
    /// gray that was hardcoded in widget code before Phase J.
    ToggleTrackOff,
    /// Modal backdrop scrim — translucent black, both appearances.
    Scrim,
    /// Explicit no-fill (fully transparent). A named role so "paint nothing"
    /// is a stated decision, not a raw constant in view code.
    NoFill,
    /// The 1px "lit from above" inner top edge on elevated cards — the
    /// machined-depth cue native dark surfaces carry (cycle-1). Transparent
    /// in light, where shadows carry the depth.
    EdgeHighlight,
    /// Selected navigation wash — accent-tinted, the treatment every
    /// polished sidebar uses (cycle-1; the neutral `Selected` wash remains
    /// for non-navigation selection).
    SelectedAccent,
    /// Status-tinted surface washes (cycle-1): a card carrying a live
    /// status reads as that status, not as gray. Alpha washes designed to
    /// composite over `Panel`.
    TintOk,
    TintWarn,
    TintErr,
    TintInfo,
    /// White lift composited over ANY saturated button fill on hover —
    /// hue-preserving, unlike lerping toward the neutral widget ramp
    /// (which visibly grayed accent buttons on approach; ISC-310 standing
    /// loop, 2026-08-03). Appearance-constant.
    FillHoverLift,
    /// Black shade composited over a saturated fill on press. Constant.
    FillPressShade,
    /// The 1px "gel" top light inside a filled button — the Big Sur cue.
    /// White in both appearances (it sits on a saturated fill, not the
    /// neutral surface, so unlike EdgeHighlight it never goes transparent).
    ButtonTopLight,
}

/// Resolve a role against an appearance. Total — every role has a value in
/// both appearances, so view code can never hit a missing token.
pub const fn resolve(role: ColorRole, appearance: Appearance) -> Rgba {
    let dark = matches!(appearance, Appearance::Dark);
    match role {
        // Surface planes diverge by appearance (ISC-310 cycle-3, the owner's
        // standing "keep iterating" directive): dark keeps cards one step
        // above the canvas (step 1 canvas / step 2 card); light runs the
        // macOS grouped-settings idiom instead — PURE WHITE cards on a
        // visibly gray sand canvas (step 3), which is Radix Themes' own
        // `--color-panel-solid: white` pattern. The old light step-1/step-2
        // pairing put a 6-value delta between card and canvas: cards read
        // as flat regions, not surfaces. Light controls shift one step
        // darker in lockstep so a rest button stays visible on the new
        // canvas.
        ColorRole::Bg => {
            if dark {
                neutral_step(appearance, 1)
            } else {
                neutral_step(appearance, 3)
            }
        }
        ColorRole::Panel => {
            if dark {
                neutral_step(appearance, 2)
            } else {
                Rgba([255, 255, 255, 255])
            }
        }
        ColorRole::Inset => {
            if dark {
                neutral_step(appearance, 1)
            } else {
                neutral_step(appearance, 2)
            }
        }
        ColorRole::Widget => {
            if dark {
                neutral_step(appearance, 3)
            } else {
                neutral_step(appearance, 4)
            }
        }
        ColorRole::WidgetHover => {
            if dark {
                neutral_step(appearance, 4)
            } else {
                neutral_step(appearance, 5)
            }
        }
        ColorRole::WidgetActive => {
            if dark {
                neutral_step(appearance, 5)
            } else {
                neutral_step(appearance, 6)
            }
        }
        ColorRole::Line => neutral_step(appearance, 6),
        ColorRole::Hairline => neutral_alpha_step(appearance, 4),
        ColorRole::RowHover => neutral_alpha_step(appearance, 3),
        ColorRole::GhostActive => neutral_alpha_step(appearance, 4),
        ColorRole::Selected => neutral_alpha_step(appearance, 5),
        ColorRole::Default => neutral_step(appearance, 12),
        ColorRole::Muted => neutral_step(appearance, 11),
        ColorRole::Faint => neutral_step(appearance, 11),
        ColorRole::Disabled => neutral_step(appearance, 9),
        ColorRole::Accent => {
            if dark {
                ACCENT_DARK
            } else {
                ACCENT_LIGHT
            }
        }
        ColorRole::OnAccent => Rgba([255, 255, 255, 255]),
        ColorRole::Success => {
            if dark {
                OK_DARK
            } else {
                OK_LIGHT
            }
        }
        ColorRole::Warning => {
            if dark {
                WARN_DARK
            } else {
                WARN_LIGHT
            }
        }
        ColorRole::Error => {
            if dark {
                ERR_DARK
            } else {
                ERR_LIGHT
            }
        }
        ColorRole::Info => {
            if dark {
                INFO_DARK
            } else {
                INFO_LIGHT
            }
        }
        ColorRole::Knob => Rgba([255, 255, 255, 255]),
        ColorRole::ToggleTrackOff => {
            if dark {
                Rgba([70, 70, 70, 255])
            } else {
                Rgba([190, 190, 190, 255])
            }
        }
        ColorRole::Scrim => Rgba([0, 0, 0, 100]),
        ColorRole::NoFill => Rgba([0, 0, 0, 0]),
        ColorRole::EdgeHighlight => {
            if dark {
                Rgba([255, 255, 255, 14])
            } else {
                Rgba([255, 255, 255, 0])
            }
        }
        ColorRole::SelectedAccent => {
            if dark {
                Rgba([76, 125, 255, 46])
            } else {
                Rgba([62, 99, 221, 36])
            }
        }
        ColorRole::TintOk => {
            if dark {
                Rgba([47, 191, 118, 20])
            } else {
                Rgba([29, 95, 66, 20])
            }
        }
        ColorRole::TintWarn => {
            if dark {
                Rgba([226, 156, 60, 20])
            } else {
                Rgba([125, 76, 17, 22])
            }
        }
        ColorRole::TintErr => {
            if dark {
                Rgba([224, 92, 92, 20])
            } else {
                Rgba([153, 33, 42, 20])
            }
        }
        ColorRole::TintInfo => {
            if dark {
                Rgba([110, 163, 224, 20])
            } else {
                Rgba([15, 83, 153, 20])
            }
        }
        ColorRole::FillHoverLift => Rgba([255, 255, 255, 26]),
        ColorRole::FillPressShade => Rgba([0, 0, 0, 36]),
        ColorRole::ButtonTopLight => Rgba([255, 255, 255, 46]),
    }
}

// ───────────────────────── WCAG contrast ─────────────────────────

/// WCAG 2.x relative luminance of a color, alpha ignored (the caller decides
/// what an alpha color composites over before asking about its luminance).
pub fn relative_luminance(color: Rgba) -> f64 {
    fn channel(byte: u8) -> f64 {
        let c = byte as f64 / 255.0;
        if c <= 0.04045 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    }
    0.2126 * channel(color.r()) + 0.7152 * channel(color.g()) + 0.0722 * channel(color.b())
}

/// WCAG contrast ratio between two colors, 1.0..=21.0.
pub fn contrast_ratio(a: Rgba, b: Rgba) -> f64 {
    let (la, lb) = (relative_luminance(a), relative_luminance(b));
    let (hi, lo) = if la > lb { (la, lb) } else { (lb, la) };
    (hi + 0.05) / (lo + 0.05)
}

/// Gamma-space per-channel blend, `t` in 0..=1 — the same blend family the
/// GUI's own `lerp_to_gamma` idiom uses, so repaired colors look like the
/// app's other interpolations rather than a different color science.
fn blend(from: Rgba, to: Rgba, t: f64) -> Rgba {
    let ch = |a: u8, b: u8| -> u8 { (a as f64 + (b as f64 - a as f64) * t).round() as u8 };
    Rgba([
        ch(from.r(), to.r()),
        ch(from.g(), to.g()),
        ch(from.b(), to.b()),
        from.a(),
    ])
}

/// Auto-contrast repair: return `fg` adjusted until it holds at least
/// `target` contrast against `bg`, blending toward whichever pole (black or
/// white) contrasts better with `bg` — Warp's pattern, itself modeled on
/// Chromium's color-shifting approach; reimplemented from the description.
///
/// The blend amount is found by binary search for the SMALLEST adjustment
/// that clears the target, so a repaired accent keeps as much of its own hue
/// as the target allows. If even the pole itself cannot reach the target
/// (mid-gray backgrounds cap the achievable ratio), the pole is returned —
/// the honest maximum, never a silent failure.
pub fn ensure_contrast(fg: Rgba, bg: Rgba, target: f64) -> Rgba {
    if contrast_ratio(fg, bg) >= target {
        return fg;
    }
    let white = Rgba([255, 255, 255, fg.a()]);
    let black = Rgba([0, 0, 0, fg.a()]);
    let pole = if contrast_ratio(white, bg) >= contrast_ratio(black, bg) {
        white
    } else {
        black
    };
    if contrast_ratio(pole, bg) < target {
        return pole;
    }
    let (mut lo, mut hi) = (0.0_f64, 1.0_f64);
    // 24 halvings puts the interval well below one 8-bit channel step.
    for _ in 0..24 {
        let mid = (lo + hi) / 2.0;
        if contrast_ratio(blend(fg, pole, mid), bg) >= target {
            hi = mid;
        } else {
            lo = mid;
        }
    }
    let repaired = blend(fg, pole, hi);
    // Rounding to u8 can land a hair under the target; nudge to the pole side
    // one step at a time until the *returned bytes* actually clear it.
    let mut t = hi;
    let mut out = repaired;
    while contrast_ratio(out, bg) < target && t < 1.0 {
        t = (t + 1.0 / 255.0).min(1.0);
        out = blend(fg, pole, t);
    }
    out
}

// ───────────────────────── spacing + shape metrics ─────────────────────────
// Shape follows cosmic-theme (pop-os/libcosmic, MPL-2.0): named steps,
// preset enums, From conversions. Clean-room reimplementation — see the
// module docs for the licensing rationale. Standard/Round values are
// cosmic-theme's published numbers; the other presets anchor to cosmic's
// documented examples (Compact m=16 xxxl=64; Spacious m=32 xxxl=160;
// SlightlyRound "most steps 8, xs 2"; Square "everything 2") with
// ADEB-chosen intermediates.

/// UI density — one user-facing knob that rescales the whole spacing system
/// together, never per-component.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Density {
    Compact,
    #[default]
    Standard,
    Spacious,
}

/// The spacing scale, named steps in px.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Spacing {
    pub space_none: u16,
    pub space_xxxs: u16,
    pub space_xxs: u16,
    pub space_xs: u16,
    pub space_s: u16,
    pub space_m: u16,
    pub space_l: u16,
    pub space_xl: u16,
    pub space_xxl: u16,
    pub space_xxxl: u16,
}

impl Spacing {
    pub const fn for_density(density: Density) -> Spacing {
        match density {
            Density::Compact => Spacing {
                space_none: 0,
                space_xxxs: 2,
                space_xxs: 4,
                space_xs: 8,
                space_s: 12,
                space_m: 16,
                space_l: 24,
                space_xl: 36,
                space_xxl: 48,
                space_xxxl: 64,
            },
            Density::Standard => Spacing {
                space_none: 0,
                space_xxxs: 4,
                space_xxs: 8,
                space_xs: 12,
                space_s: 16,
                space_m: 24,
                space_l: 32,
                space_xl: 48,
                space_xxl: 64,
                space_xxxl: 128,
            },
            Density::Spacious => Spacing {
                space_none: 0,
                space_xxxs: 6,
                space_xxs: 12,
                space_xs: 16,
                space_s: 24,
                space_m: 32,
                space_l: 40,
                space_xl: 56,
                space_xxl: 80,
                space_xxxl: 160,
            },
        }
    }
}

impl From<Density> for Spacing {
    fn from(density: Density) -> Spacing {
        Spacing::for_density(density)
    }
}

/// Corner roundness — three presets collapsing the full radius struct to one
/// user-facing choice, cosmic-theme's pattern.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Roundness {
    #[default]
    Round,
    SlightlyRound,
    Square,
}

/// Corner radii, per-corner `[f32; 4]` per step (cosmic-theme's shape — a
/// step can round corners asymmetrically even though every preset here is
/// symmetric).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CornerRadii {
    pub radius_0: [f32; 4],
    pub radius_xs: [f32; 4],
    pub radius_s: [f32; 4],
    pub radius_m: [f32; 4],
    pub radius_l: [f32; 4],
    pub radius_xl: [f32; 4],
}

const fn corners(r: f32) -> [f32; 4] {
    [r, r, r, r]
}

impl CornerRadii {
    pub const fn for_roundness(roundness: Roundness) -> CornerRadii {
        match roundness {
            Roundness::Round => CornerRadii {
                radius_0: corners(0.0),
                radius_xs: corners(4.0),
                radius_s: corners(8.0),
                radius_m: corners(16.0),
                radius_l: corners(32.0),
                radius_xl: corners(160.0),
            },
            Roundness::SlightlyRound => CornerRadii {
                radius_0: corners(0.0),
                radius_xs: corners(2.0),
                radius_s: corners(8.0),
                radius_m: corners(8.0),
                radius_l: corners(8.0),
                radius_xl: corners(8.0),
            },
            Roundness::Square => CornerRadii {
                radius_0: corners(0.0),
                radius_xs: corners(2.0),
                radius_s: corners(2.0),
                radius_m: corners(2.0),
                radius_l: corners(2.0),
                radius_xl: corners(2.0),
            },
        }
    }
}

impl From<Roundness> for CornerRadii {
    fn from(roundness: Roundness) -> CornerRadii {
        CornerRadii::for_roundness(roundness)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn as_array(s: Spacing) -> [u16; 10] {
        [
            s.space_none,
            s.space_xxxs,
            s.space_xxs,
            s.space_xs,
            s.space_s,
            s.space_m,
            s.space_l,
            s.space_xl,
            s.space_xxl,
            s.space_xxxl,
        ]
    }

    /// Every role resolves in both appearances and no appearance-varying role
    /// accidentally collapses to one value — the enum is total and the
    /// appearance key is load-bearing.
    #[test]
    fn every_role_resolves_and_appearance_matters_where_documented() {
        let varying = [
            ColorRole::Bg,
            ColorRole::Panel,
            ColorRole::Widget,
            ColorRole::WidgetHover,
            ColorRole::WidgetActive,
            ColorRole::Line,
            ColorRole::Default,
            ColorRole::Muted,
            ColorRole::Disabled,
            ColorRole::Accent,
            ColorRole::Success,
            ColorRole::Warning,
            ColorRole::Error,
            ColorRole::Info,
            ColorRole::ToggleTrackOff,
        ];
        for role in varying {
            assert_ne!(
                resolve(role, Appearance::Light),
                resolve(role, Appearance::Dark),
                "{role:?} must differ between appearances"
            );
        }
        let also_varying = [
            ColorRole::EdgeHighlight,
            ColorRole::SelectedAccent,
            ColorRole::TintOk,
            ColorRole::TintWarn,
            ColorRole::TintErr,
            ColorRole::TintInfo,
        ];
        for role in also_varying {
            assert_ne!(
                resolve(role, Appearance::Light),
                resolve(role, Appearance::Dark),
                "{role:?} must differ between appearances"
            );
        }
        let constant = [
            ColorRole::OnAccent,
            ColorRole::Knob,
            ColorRole::Scrim,
            ColorRole::NoFill,
            ColorRole::FillHoverLift,
            ColorRole::FillPressShade,
            ColorRole::ButtonTopLight,
        ];
        for role in constant {
            assert_eq!(
                resolve(role, Appearance::Light),
                resolve(role, Appearance::Dark),
                "{role:?} is documented appearance-constant"
            );
        }
    }

    /// The ramp data matches its sources byte-for-byte, spot-anchored at the
    /// ends and a middle step of each variant: light = Radix sand (unchanged
    /// since Phase I d5b1ba8), dark = Radix slateDark (the ISC-310 cycle-1
    /// owner-directed revision — ISA Decision 2026-08-02T19:45Z). The full
    /// cross-crate proof lives in theme.rs's pinned-palette test.
    #[test]
    fn ramp_values_match_their_sources() {
        assert_eq!(
            neutral_step(Appearance::Light, 1),
            Rgba([0xfd, 0xfd, 0xfc, 255])
        );
        assert_eq!(
            neutral_step(Appearance::Light, 12),
            Rgba([0x21, 0x20, 0x1c, 255])
        );
        assert_eq!(
            neutral_step(Appearance::Dark, 1),
            Rgba([0x11, 0x11, 0x13, 255])
        );
        assert_eq!(
            neutral_step(Appearance::Dark, 12),
            Rgba([0xed, 0xee, 0xf0, 255])
        );
        assert_eq!(
            neutral_alpha_step(Appearance::Dark, 4),
            Rgba([0xd3, 0xed, 0xf8, 0x1d])
        );
        assert_eq!(
            neutral_alpha_step(Appearance::Light, 4),
            Rgba([0x1f, 0x15, 0x00, 0x19])
        );
        assert_eq!(
            resolve(ColorRole::Accent, Appearance::Dark),
            Rgba([76, 125, 255, 255])
        );
        assert_eq!(
            resolve(ColorRole::Accent, Appearance::Light),
            Rgba([0x3e, 0x63, 0xdd, 255])
        );
    }

    /// Black-on-white is the 21:1 anchor every WCAG implementation must hit;
    /// a color against itself is 1:1.
    #[test]
    fn contrast_ratio_hits_the_wcag_anchors() {
        let white = Rgba([255, 255, 255, 255]);
        let black = Rgba([0, 0, 0, 255]);
        let ratio = contrast_ratio(black, white);
        assert!(
            (ratio - 21.0).abs() < 1e-9,
            "black/white must be 21:1, got {ratio}"
        );
        assert!((contrast_ratio(white, white) - 1.0).abs() < 1e-9);
        // Symmetry: order of arguments cannot matter.
        let a = resolve(ColorRole::Accent, Appearance::Dark);
        let b = resolve(ColorRole::Bg, Appearance::Dark);
        assert_eq!(contrast_ratio(a, b), contrast_ratio(b, a));
    }

    /// ISC-306's own bar, verbatim: at least 5 arbitrary accent colors
    /// spanning light and dark hues, repaired to >=4.5:1 text contrast on
    /// both the bg and panel surfaces, in both appearances.
    #[test]
    fn ensure_contrast_repairs_arbitrary_accents_on_real_surfaces() {
        let accents = [
            Rgba([255, 235, 59, 255]), // bright yellow — hopeless on light bg
            Rgba([32, 33, 36, 255]),   // near-black — hopeless on dark bg
            Rgba([233, 30, 99, 255]),  // saturated pink
            Rgba([0, 150, 136, 255]),  // teal
            Rgba([124, 77, 255, 255]), // violet
            Rgba([121, 85, 72, 255]),  // mid brown — low contrast everywhere
        ];
        for appearance in [Appearance::Light, Appearance::Dark] {
            for surface_role in [ColorRole::Bg, ColorRole::Panel] {
                let surface = resolve(surface_role, appearance);
                for accent in accents {
                    let repaired = ensure_contrast(accent, surface, 4.5);
                    let ratio = contrast_ratio(repaired, surface);
                    assert!(
                        ratio >= 4.5,
                        "{accent:?} on {surface_role:?}/{appearance:?} repaired to only {ratio:.3}:1"
                    );
                }
            }
        }
    }

    /// A color already clearing the target comes back unchanged — repair
    /// never perturbs what was fine.
    #[test]
    fn ensure_contrast_is_identity_when_already_sufficient() {
        let text = resolve(ColorRole::Default, Appearance::Dark);
        let bg = resolve(ColorRole::Bg, Appearance::Dark);
        assert_eq!(ensure_contrast(text, bg, 4.5), text);
    }

    /// When the target is unreachable even at the pole (a mid-gray surface
    /// caps the achievable ratio), the pole comes back — the honest maximum,
    /// not a silent under-target color.
    #[test]
    fn ensure_contrast_returns_the_pole_when_the_target_is_unreachable() {
        let mid_gray = Rgba([128, 128, 128, 255]);
        let out = ensure_contrast(Rgba([120, 120, 120, 255]), mid_gray, 21.0);
        assert!(
            out == Rgba([255, 255, 255, 255]) || out == Rgba([0, 0, 0, 255]),
            "unreachable target must land on a pole, got {out:?}"
        );
        // And the pole it picked is the better of the two.
        let white = contrast_ratio(Rgba([255, 255, 255, 255]), mid_gray);
        let black = contrast_ratio(Rgba([0, 0, 0, 255]), mid_gray);
        let got = contrast_ratio(out, mid_gray);
        assert_eq!(got, white.max(black));
    }

    /// Repair keeps the foreground's alpha — it fixes hue/luminance, not
    /// translucency, which is the caller's decision.
    #[test]
    fn ensure_contrast_preserves_alpha() {
        let translucent = Rgba([255, 235, 59, 128]);
        let bg = resolve(ColorRole::Bg, Appearance::Light);
        assert_eq!(ensure_contrast(translucent, bg, 4.5).a(), 128);
    }

    /// The Standard scale is cosmic-theme's published one, and every preset
    /// is strictly non-decreasing across its named steps — a scale with an
    /// inversion is a typo, not a design.
    #[test]
    fn spacing_presets_hold_the_documented_anchors_and_stay_sorted() {
        let standard = Spacing::for_density(Density::Standard);
        assert_eq!(
            as_array(standard),
            [0, 4, 8, 12, 16, 24, 32, 48, 64, 128],
            "Standard must be cosmic-theme's published scale"
        );
        let compact = Spacing::for_density(Density::Compact);
        assert_eq!((compact.space_m, compact.space_xxxl), (16, 64));
        let spacious = Spacing::for_density(Density::Spacious);
        assert_eq!((spacious.space_m, spacious.space_xxxl), (32, 160));
        for density in [Density::Compact, Density::Standard, Density::Spacious] {
            let arr = as_array(Spacing::for_density(density));
            assert!(
                arr.windows(2).all(|w| w[0] <= w[1]),
                "{density:?} scale has an inversion: {arr:?}"
            );
        }
        // Compact tightens and Spacious loosens relative to Standard, step by
        // step — the presets are one system at three densities, not three
        // unrelated scales.
        let (c, s, sp) = (as_array(compact), as_array(standard), as_array(spacious));
        for i in 0..c.len() {
            assert!(
                c[i] <= s[i] && s[i] <= sp[i],
                "step {i} breaks the density order"
            );
        }
        assert_eq!(Spacing::from(Density::Standard), standard);
    }

    /// Round is cosmic-theme's published radius scale; the other presets
    /// honor their documented character (SlightlyRound caps at 8, Square at
    /// 2, radius_0 is always 0 — its name is its contract).
    #[test]
    fn corner_presets_hold_their_documented_character() {
        let round = CornerRadii::for_roundness(Roundness::Round);
        assert_eq!(round.radius_0, [0.0; 4]);
        assert_eq!(round.radius_xs, [4.0; 4]);
        assert_eq!(round.radius_s, [8.0; 4]);
        assert_eq!(round.radius_m, [16.0; 4]);
        assert_eq!(round.radius_l, [32.0; 4]);
        assert_eq!(round.radius_xl, [160.0; 4]);
        let slight = CornerRadii::for_roundness(Roundness::SlightlyRound);
        for radii in [
            slight.radius_s,
            slight.radius_m,
            slight.radius_l,
            slight.radius_xl,
        ] {
            assert_eq!(radii, [8.0; 4]);
        }
        assert_eq!(slight.radius_xs, [2.0; 4]);
        assert_eq!(slight.radius_0, [0.0; 4]);
        let square = CornerRadii::for_roundness(Roundness::Square);
        for radii in [
            square.radius_xs,
            square.radius_s,
            square.radius_m,
            square.radius_l,
            square.radius_xl,
        ] {
            assert_eq!(radii, [2.0; 4]);
        }
        assert_eq!(square.radius_0, [0.0; 4]);
        assert_eq!(CornerRadii::from(Roundness::Round), round);
    }

    /// Rgba accessors are field-order-correct — a swapped channel here would
    /// silently tint the entire app.
    #[test]
    fn rgba_accessors_map_to_their_channels() {
        let color = Rgba([1, 2, 3, 4]);
        assert_eq!((color.r(), color.g(), color.b(), color.a()), (1, 2, 3, 4));
        assert_eq!(rgb(0x0a0b0c), Rgba([0x0a, 0x0b, 0x0c, 255]));
        assert_eq!(rgba(0x0a0b0c0d), Rgba([0x0a, 0x0b, 0x0c, 0x0d]));
    }
}
