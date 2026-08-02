//! Window-chrome tier selection (Phase E) — the pure half of the glass
//! sidebar. Everything here is a decision over plain facts; the `unsafe`
//! AppKit half lives in the Control Center crate and only *executes* what
//! this module decides.
//!
//! The tier is chosen by CLASS EXISTENCE, never by OS-version parsing: the
//! question is "can this process instantiate NSGlassEffectView", and the
//! runtime answers it directly. Reduce Transparency is an accessibility
//! setting and overrides everything — an owner who asked for opacity gets
//! opacity, whatever the OS offers.

/// The three chrome tiers, best first. `Opaque` is also the answer for
/// non-macOS builds, tests, and any attach failure — the app must look
/// designed (current step-2 fill), not broken, on that path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChromeTier {
    /// macOS 26+ `NSGlassEffectView` behind the sidebar strip.
    Glass,
    /// `NSVisualEffectView` (material Sidebar) — the long-supported fallback.
    Vibrancy,
    /// No native material; the sidebar paints its own opaque fill.
    Opaque,
}

/// The AppKit class each tier gates on. The strings are the contract with
/// `chrome/macos.rs`: existence of the NAME is the capability probe.
pub const GLASS_CLASS: &str = "NSGlassEffectView";
pub const VIBRANCY_CLASS: &str = "NSVisualEffectView";

/// Decide the tier. Reduce Transparency forces `Opaque` no matter what
/// classes exist; otherwise the best available material wins.
pub fn select_tier(
    has_glass_class: bool,
    has_vibrancy_class: bool,
    reduce_transparency: bool,
) -> ChromeTier {
    if reduce_transparency {
        return ChromeTier::Opaque;
    }
    if has_glass_class {
        ChromeTier::Glass
    } else if has_vibrancy_class {
        ChromeTier::Vibrancy
    } else {
        ChromeTier::Opaque
    }
}

impl ChromeTier {
    /// The class the attach step instantiates for this tier; `None` means
    /// nothing is attached at all.
    pub fn effect_class(self) -> Option<&'static str> {
        match self {
            ChromeTier::Glass => Some(GLASS_CLASS),
            ChromeTier::Vibrancy => Some(VIBRANCY_CLASS),
            ChromeTier::Opaque => None,
        }
    }

    /// Whether the egui sidebar should paint a transparent fill so the
    /// native material shows through.
    pub fn wants_transparent_sidebar(self) -> bool {
        self != ChromeTier::Opaque
    }
}

/// The sidebar strip's width in points — must match the egui panel width
/// (the Control Center asserts the two constants agree in a test).
pub const SIDEBAR_WIDTH_PT: f64 = 220.0;

/// The native effect view's frame inside the window's content view, in
/// points. Origin-agnostic: the strip spans the FULL height, so it is the
/// same rect under flipped and unflipped coordinate systems.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StripFrame {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

/// The geometry contract for the 220pt sidebar strip: pinned to the leading
/// edge, full height, width clamped to the window (a window narrower than
/// the sidebar gets the whole window, never a negative rect).
pub fn sidebar_strip(window_width: f64, window_height: f64) -> StripFrame {
    StripFrame {
        x: 0.0,
        y: 0.0,
        width: SIDEBAR_WIDTH_PT.min(window_width.max(0.0)),
        height: window_height.max(0.0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reduce_transparency_forces_opaque_over_everything() {
        for (glass, vibrancy) in [(true, true), (true, false), (false, true), (false, false)] {
            assert_eq!(
                select_tier(glass, vibrancy, true),
                ChromeTier::Opaque,
                "reduce-transparency must win (glass={glass}, vibrancy={vibrancy})"
            );
        }
    }

    #[test]
    fn the_best_available_material_wins_when_transparency_is_allowed() {
        assert_eq!(select_tier(true, true, false), ChromeTier::Glass);
        assert_eq!(select_tier(true, false, false), ChromeTier::Glass);
        assert_eq!(select_tier(false, true, false), ChromeTier::Vibrancy);
        assert_eq!(select_tier(false, false, false), ChromeTier::Opaque);
    }

    #[test]
    fn each_tier_names_the_class_it_instantiates() {
        assert_eq!(ChromeTier::Glass.effect_class(), Some("NSGlassEffectView"));
        assert_eq!(
            ChromeTier::Vibrancy.effect_class(),
            Some("NSVisualEffectView")
        );
        assert_eq!(ChromeTier::Opaque.effect_class(), None);
        // The constants ARE the probe strings — pinned so a rename breaks
        // here, not silently at runtime.
        assert_eq!(GLASS_CLASS, "NSGlassEffectView");
        assert_eq!(VIBRANCY_CLASS, "NSVisualEffectView");
    }

    #[test]
    fn only_the_opaque_tier_keeps_the_painted_sidebar_fill() {
        assert!(ChromeTier::Glass.wants_transparent_sidebar());
        assert!(ChromeTier::Vibrancy.wants_transparent_sidebar());
        assert!(!ChromeTier::Opaque.wants_transparent_sidebar());
    }

    #[test]
    fn the_strip_pins_the_leading_edge_at_full_height() {
        let strip = sidebar_strip(1100.0, 740.0);
        assert_eq!(
            strip,
            StripFrame {
                x: 0.0,
                y: 0.0,
                width: 220.0,
                height: 740.0,
            }
        );
    }

    #[test]
    fn the_strip_clamps_to_a_narrow_window_and_never_goes_negative() {
        assert_eq!(sidebar_strip(180.0, 600.0).width, 180.0);
        assert_eq!(sidebar_strip(0.0, 600.0).width, 0.0);
        let degenerate = sidebar_strip(-5.0, -5.0);
        assert_eq!(degenerate.width, 0.0);
        assert_eq!(degenerate.height, 0.0);
    }
}
