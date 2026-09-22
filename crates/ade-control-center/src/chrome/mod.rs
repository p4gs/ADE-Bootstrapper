//! Native window chrome — the one real glass surface this app has.
//!
//! The DEFENSIBLE claim (and the whole claim): the navigation layer is a
//! genuine system material; the content layer is egui. Glass is sidebar-only,
//! never content, never glass-on-glass.
//!
//! Split exactly as the plan requires: tier selection and geometry are pure
//! and live in `ade_core::gui::chrome` (covered by core's 95/95 gate); every
//! `unsafe` AppKit call is confined to `macos.rs`. This file is only the
//! platform seam.

#[cfg(target_os = "macos")]
mod macos;

pub(crate) use ade_core::gui::chrome::ChromeTier;

/// The attached chrome: the tier achieved at launch plus (on macOS, when a
/// material actually attached) the live handle that responds to Reduce
/// Transparency changes while running (ISC-309, vibrancy surface).
pub(crate) struct ChromeAttachment {
    pub tier: ChromeTier,
    #[cfg(target_os = "macos")]
    handle: Option<macos::ChromeHandle>,
}

impl ChromeAttachment {
    /// Per-frame: `Some(new_transparent)` only when the user toggled Reduce
    /// Transparency since the last frame — the egui side flips its sidebar
    /// fill to match, both directions, no relaunch. On the Opaque tier (and
    /// off macOS) there is no material to toggle and this is always `None`;
    /// a launch-time Opaque cannot upgrade live because the effect view was
    /// never attached — documented, not hidden.
    pub(crate) fn poll_reduce_transparency(&mut self) -> Option<bool> {
        #[cfg(target_os = "macos")]
        {
            self.handle
                .as_mut()
                .and_then(|handle| handle.poll_reduce_transparency())
        }
        #[cfg(not(target_os = "macos"))]
        {
            None
        }
    }
}

/// Attach the native effect view behind the sidebar strip and report the
/// tier actually achieved. Any failure — no window handle, no usable class,
/// Reduce Transparency on, non-macOS build — lands on `Opaque`, where the
/// sidebar keeps its painted step-2 fill and nothing looks broken.
#[cfg(target_os = "macos")]
pub(crate) fn attach_sidebar_chrome(cc: &eframe::CreationContext<'_>) -> ChromeAttachment {
    let (tier, handle) = macos::attach(cc);
    ChromeAttachment { tier, handle }
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn attach_sidebar_chrome(_cc: &eframe::CreationContext<'_>) -> ChromeAttachment {
    ChromeAttachment {
        tier: ChromeTier::Opaque,
    }
}
