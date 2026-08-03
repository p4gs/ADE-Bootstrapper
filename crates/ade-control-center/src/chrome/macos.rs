//! The `unsafe` half of the glass sidebar — every AppKit call in the app
//! lives in this file (Phase E). The decisions were made in
//! `ade_core::gui::chrome`; this file only executes them, and any failure
//! along the way degrades to `Opaque` rather than erroring.
//!
//! The recipe is the Phase 0 spike, re-derived (the spike code itself is
//! lost; its conclusions were preserved as build notes):
//! - the native effect view is added to the window's content-view hierarchy
//!   with `layer.zPosition = -1.0`, which empirically draws it BEHIND the
//!   egui GL surface;
//! - a `hitTest: -> nil` override on the host view passes every click
//!   through to egui — the material is scenery, never a control;
//! - `NSGlassEffectView` renders fine with an empty `contentView`;
//! - the strip tracks window height via autoresizing (height sizable, right
//!   margin flexible), so resize needs no observer.

use ade_core::gui::chrome::{select_tier, sidebar_strip, ChromeTier};
use objc2::rc::Retained;
use objc2::runtime::AnyClass;
use objc2::{define_class, msg_send, MainThreadMarker, MainThreadOnly};
use objc2_app_kit::{
    NSAutoresizingMaskOptions, NSGlassEffectView, NSView, NSVisualEffectBlendingMode,
    NSVisualEffectMaterial, NSVisualEffectState, NSVisualEffectView, NSWorkspace,
};
use objc2_foundation::{NSPoint, NSRect, NSSize};
use raw_window_handle::{HasWindowHandle, RawWindowHandle};

define_class!(
    // SAFETY:
    // - NSView has no subclassing requirements beyond main-thread-only,
    //   which `#[thread_kind = MainThreadOnly]` enforces.
    // - `ChromeHostView` does not implement `Drop`.
    #[unsafe(super = NSView)]
    #[thread_kind = MainThreadOnly]
    #[name = "AdeChromeHostView"]
    struct ChromeHostView;

    impl ChromeHostView {
        /// The pass-through override: returning nil here excludes this view
        /// AND its effect-view subtree from hit testing, so every click in
        /// the sidebar strip reaches egui underneath.
        #[unsafe(method_id(hitTest:))]
        fn hit_test(&self, _point: NSPoint) -> Option<Retained<NSView>> {
            None
        }
    }
);

/// The live handle to the attached material (ISC-309, vibrancy surface):
/// tier selection used to be launch-time-only, so toggling Reduce
/// Transparency in System Settings mid-run either kept glass against the
/// user's stated accessibility preference or missed it entirely until
/// relaunch. The handle lets the app respond while running.
pub(crate) struct ChromeHandle {
    host: Retained<ChromeHostView>,
    /// The last observed Reduce Transparency value, so the per-frame poll
    /// reports only CHANGES.
    reduce_transparency: bool,
}

impl ChromeHandle {
    /// Poll the accessibility setting (a cheap AppKit getter — this runs on
    /// the main thread every frame, which is exactly where AppKit wants it).
    /// On a change: hide or reveal the native material and report the new
    /// "sidebar should be transparent" fact for the egui side. `None` means
    /// no change. Turning Reduce Transparency OFF restores the material
    /// live too — the attach survives hidden, so both directions work
    /// without relaunch.
    pub(crate) fn poll_reduce_transparency(&mut self) -> Option<bool> {
        let reduce = NSWorkspace::sharedWorkspace().accessibilityDisplayShouldReduceTransparency();
        if reduce == self.reduce_transparency {
            return None;
        }
        self.reduce_transparency = reduce;
        self.host.setHidden(reduce);
        Some(!reduce)
    }
}

/// Query the runtime + accessibility facts, decide the tier (pure, in core),
/// and attach the native material behind the sidebar strip. Returns the tier
/// actually achieved, plus the live handle when a material was attached.
pub(crate) fn attach(cc: &eframe::CreationContext<'_>) -> (ChromeTier, Option<ChromeHandle>) {
    let Some(mtm) = MainThreadMarker::new() else {
        return (ChromeTier::Opaque, None);
    };
    // Class EXISTENCE is the capability probe (never OS-version parsing).
    let has_glass = AnyClass::get(c"NSGlassEffectView").is_some();
    let has_vibrancy = AnyClass::get(c"NSVisualEffectView").is_some();
    let reduce_transparency =
        NSWorkspace::sharedWorkspace().accessibilityDisplayShouldReduceTransparency();
    let tier = select_tier(has_glass, has_vibrancy, reduce_transparency);
    if tier == ChromeTier::Opaque {
        return (ChromeTier::Opaque, None);
    }
    match attach_effect_view(cc, mtm, tier) {
        Some(host) => (
            tier,
            Some(ChromeHandle {
                host,
                reduce_transparency,
            }),
        ),
        // No handle / no window / no layer: the painted fill stays.
        None => (ChromeTier::Opaque, None),
    }
}

fn attach_effect_view(
    cc: &eframe::CreationContext<'_>,
    mtm: MainThreadMarker,
    tier: ChromeTier,
) -> Option<Retained<ChromeHostView>> {
    let handle = cc.window_handle().ok()?;
    let RawWindowHandle::AppKit(appkit) = handle.as_raw() else {
        return None;
    };
    // SAFETY: eframe hands out the live winit NSView for this window, and
    // `MainThreadMarker::new()` succeeded, so dereferencing it as a
    // main-thread-only NSView upholds objc2's contract. The reference does
    // not outlive this call.
    let content_view: &NSView = unsafe { appkit.ns_view.cast::<NSView>().as_ref() };

    let bounds = content_view.bounds();
    let strip = sidebar_strip(bounds.size.width, bounds.size.height);
    let frame = NSRect::new(
        NSPoint::new(strip.x, strip.y),
        NSSize::new(strip.width, strip.height),
    );

    // The host: a pass-through view pinned to the leading edge at full
    // height. Autoresizing (flexible right margin + sizable height) keeps it
    // tracking the window with no resize observer.
    // SAFETY: plain NSView designated initializer on a freshly allocated
    // instance of our own subclass.
    let host: Retained<ChromeHostView> =
        unsafe { msg_send![ChromeHostView::alloc(mtm), initWithFrame: frame] };
    host.setAutoresizingMask(
        NSAutoresizingMaskOptions::ViewHeightSizable | NSAutoresizingMaskOptions::ViewMaxXMargin,
    );
    host.setWantsLayer(true);

    let inner = NSRect::new(
        NSPoint::new(0.0, 0.0),
        NSSize::new(strip.width, strip.height),
    );
    let fill =
        NSAutoresizingMaskOptions::ViewWidthSizable | NSAutoresizingMaskOptions::ViewHeightSizable;
    match tier {
        ChromeTier::Glass => {
            // Guarded by the AnyClass existence probe above: this typed
            // binding resolves the class at runtime by name.
            let glass = NSGlassEffectView::initWithFrame(NSGlassEffectView::alloc(mtm), inner);
            // Spike fact: the glass renders with an EMPTY contentView; the
            // sidebar's actual content stays egui-drawn on top.
            glass.setAutoresizingMask(fill);
            host.addSubview(&glass);
        }
        ChromeTier::Vibrancy => {
            let effect = NSVisualEffectView::initWithFrame(NSVisualEffectView::alloc(mtm), inner);
            effect.setMaterial(NSVisualEffectMaterial::Sidebar);
            effect.setBlendingMode(NSVisualEffectBlendingMode::BehindWindow);
            effect.setState(NSVisualEffectState::FollowsWindowActiveState);
            effect.setAutoresizingMask(fill);
            host.addSubview(&effect);
        }
        ChromeTier::Opaque => return None,
    }

    content_view.addSubview(&host);
    // The spike's key ordering fact: a negative zPosition draws the native
    // view BEHIND the egui GL surface. Without a layer there is nothing to
    // order, so that path reports failure and stays opaque.
    let layer = host.layer()?;
    layer.setZPosition(-1.0);
    Some(host)
}
