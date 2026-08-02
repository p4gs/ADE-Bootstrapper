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

/// Attach the native effect view behind the sidebar strip and report the
/// tier actually achieved. Any failure — no window handle, no usable class,
/// Reduce Transparency on, non-macOS build — lands on `Opaque`, where the
/// sidebar keeps its painted step-2 fill and nothing looks broken.
#[cfg(target_os = "macos")]
pub(crate) fn attach_sidebar_chrome(cc: &eframe::CreationContext<'_>) -> ChromeTier {
    macos::attach(cc)
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn attach_sidebar_chrome(_cc: &eframe::CreationContext<'_>) -> ChromeTier {
    ChromeTier::Opaque
}
