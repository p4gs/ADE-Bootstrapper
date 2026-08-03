//! ADE Status — macOS menu-bar helper (Rust, no server: links ade-core).
//!
//! Drives AppKit DIRECTLY via `objc2` (replacing tray-icon + winit). The
//! ordering follows the proven PulseMenuBar.swift pattern: the
//! `NSStatusItem` is created inside `applicationDidFinishLaunching:` on an
//! `NSApplicationDelegate`, never before `run()`.
//!
//! Verification gotcha (macOS 26 "Tahoe"): third-party status items are
//! hosted as replica windows OWNED BY Control Center's process — a
//! CGWindowList probe filtered to THIS pid finds nothing even for a
//! working item (true for Swift reference apps too). To verify
//! materialization, diff layer-25 window IDs across launch/kill: this app
//! adds exactly one 32x33 replica per display and removes it on exit.
//!
//! A background thread runs core capability detection on a timeout budget
//! shorter than its poll interval (the menu bar can never hang — ISC-189),
//! reads running-job counts from `$ADE_HOME/jobs.json` (written by the
//! Control Center — ISC-171), and stores the result in a shared snapshot
//! with a generation counter. A main-thread `NSTimer` re-renders the
//! button + menu only when the generation moves; AppKit is never touched
//! off the main thread. Detection failure degrades the menu; it never
//! crashes (ISC-190).

use std::cell::{Cell, OnceCell};
use std::panic::AssertUnwindSafe;
use std::process::Command;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use ade_core::exec::{real_exec, real_which};
use ade_core::gui::inventory::{detect_capabilities, DetectOptions};
use ade_core::gui::jobs::{read_jobs_summary, read_last_finished};
use ade_core::gui::menubar::{build_menubar_payload, tray_title, MenubarPayload};
use ade_core::gui::state::{ade_home_from_env, load_gui_state};
use ade_core::types::{ExecFn, WhichFn};
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, ProtocolObject, Sel};
use objc2::{define_class, msg_send, sel, DefinedClass, MainThreadOnly};
use objc2_app_kit::{
    NSAccessibility, NSApplication, NSApplicationActivationPolicy, NSApplicationDelegate,
    NSBezierPath, NSCellImagePosition, NSColor, NSImage, NSLineCapStyle, NSLineJoinStyle, NSMenu,
    NSMenuItem, NSStatusBar, NSStatusItem, NSVariableStatusItemLength,
};
use objc2_foundation::{
    MainThreadMarker, NSNotification, NSObject, NSObjectProtocol, NSPoint, NSRect, NSSize,
    NSString, NSTimer,
};

const POLL_SECS: u64 = 10;
const TICK_SECS: f64 = 2.0;
const PROBE_TIMEOUT_SECS: u64 = 4;
const MAX_CAPABILITY_ROWS: usize = 20;
const CONTROL_CENTER_BUNDLE: &str = "com.ade-bootstrapper.control-center";

// ---------------------------------------------------------------------------
// Data layer (background thread) — unchanged detection pipeline.
// ---------------------------------------------------------------------------

/// One detection pass, with the live subprocess/PATH implementations.
fn poll_once() -> Result<MenubarPayload, String> {
    poll_once_with(real_exec(), real_which())
}

/// The detection pass, with `exec`/`which` injectable (ISC-190: this is what
/// lets a test simulate "detection itself fails" — a panic anywhere inside
/// `detect_capabilities`, which is exactly what a probe thread panicking
/// (`ade_core::gui::inventory::detect_capabilities`'s
/// `handle.join().expect("detect thread")`) looks like from here — WITHOUT
/// needing a real broken tool on the machine running the test).
///
/// `catch_unwind` is the tray's whole ISC-190 contract in one call: whatever
/// goes wrong inside detection becomes `Err(..)`, never a propagated panic —
/// so the poll thread's `loop` (`spawn_poll_loop`) survives to poll again
/// next cycle, and the main-thread render path always has a `Result` to
/// match on, never a crash. `AssertUnwindSafe` is required only because
/// `exec`/`which` are trait-object closures now passed in as parameters
/// (`Arc<dyn Fn + Send + Sync>` is not provably `RefUnwindSafe` — the
/// compiler cannot see inside a trait object to know it holds no interior
/// mutability); the closure itself does no mutation across the unwind
/// boundary, so asserting unwind-safety here is sound.
fn poll_once_with(exec: ExecFn, which: WhichFn) -> Result<MenubarPayload, String> {
    std::panic::catch_unwind(AssertUnwindSafe(|| {
        let home = ade_home_from_env();
        let state = load_gui_state(&home).state;
        let last_jobs = read_last_finished(&home);
        let latest = std::collections::BTreeMap::new();
        let capabilities = detect_capabilities(&DetectOptions {
            exec,
            which,
            disabled: &state.disabled,
            last_jobs: &last_jobs,
            latest_versions: &latest,
            probe_running: true,
            probe_timeout: Duration::from_secs(PROBE_TIMEOUT_SECS),
        });
        let running_jobs = read_jobs_summary(&home)
            .map(|(running, _)| running)
            .unwrap_or(0) as u32;
        build_menubar_payload(&capabilities, running_jobs)
    }))
    .map_err(|_| "detection panicked".to_string())
}

/// Latest poll result plus a generation counter. The background threads
/// write; the main-thread timer re-renders only when the generation moves.
#[derive(Default)]
struct Snapshot {
    generation: u64,
    latest: Option<Result<MenubarPayload, String>>,
}

type Shared = Arc<Mutex<Snapshot>>;

fn lock_snapshot(shared: &Shared) -> MutexGuard<'_, Snapshot> {
    // A poisoned snapshot mutex only means a writer panicked mid-store;
    // the data is a plain value, so keep serving it.
    shared
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn store_poll_result(shared: &Shared, result: Result<MenubarPayload, String>) {
    let mut guard = lock_snapshot(shared);
    guard.generation += 1;
    guard.latest = Some(result);
}

/// One immediate poll (menu "Refresh Now"). Never blocks the main thread.
fn spawn_one_shot_poll(shared: Shared) {
    std::thread::spawn(move || {
        let result = poll_once();
        store_poll_result(&shared, result);
    });
}

/// The standing 10s poll loop.
fn spawn_poll_loop(shared: Shared) {
    std::thread::spawn(move || loop {
        let result = poll_once();
        store_poll_result(&shared, result);
        std::thread::sleep(Duration::from_secs(POLL_SECS));
    });
}

fn open_control_center() {
    let by_bundle = Command::new("open")
        .args(["-b", CONTROL_CENTER_BUNDLE])
        .output();
    let opened = by_bundle
        .map(|output| output.status.success())
        .unwrap_or(false);
    if !opened {
        let _ = Command::new("open")
            .args(["-a", "ADE Control Center"])
            .output();
    }
}

// ---------------------------------------------------------------------------
// Presentation helpers (main thread).
// ---------------------------------------------------------------------------

/// The status button's canvas, in points.
const ICON_SIZE: f64 = 18.0;

/// Draw the 18x18 TEMPLATE status icon (Phase F): a rounded square holding a
/// check, all painted `NSBezierPath` geometry — no text glyph, so nothing can
/// land on the emoji rendering path. Template images are drawn in black and
/// tinted by the system, adapting to menu-bar appearance automatically. The
/// drawing-handler form re-runs the block at every backing scale, so the mark
/// stays crisp on Retina bars (the deprecated lockFocus path rasterizes
/// once). `None` means the caller falls back to a text-only title.
fn status_image() -> Option<Retained<NSImage>> {
    let handler = block2::RcBlock::new(|_rect: NSRect| -> objc2::runtime::Bool {
        NSColor::blackColor().setStroke();
        let outline = NSBezierPath::bezierPathWithRoundedRect_xRadius_yRadius(
            NSRect::new(NSPoint::new(2.5, 2.5), NSSize::new(13.0, 13.0)),
            3.5,
            3.5,
        );
        outline.setLineWidth(1.5);
        outline.stroke();
        let check = NSBezierPath::bezierPath();
        check.setLineWidth(1.6);
        check.setLineCapStyle(NSLineCapStyle::Round);
        check.setLineJoinStyle(NSLineJoinStyle::Round);
        // AppKit coordinates are y-up: the check dips, then rises.
        check.moveToPoint(NSPoint::new(5.4, 9.3));
        check.lineToPoint(NSPoint::new(7.9, 6.9));
        check.lineToPoint(NSPoint::new(12.2, 11.6));
        check.stroke();
        objc2::runtime::Bool::YES
    });
    let image = NSImage::imageWithSize_flipped_drawingHandler(
        NSSize::new(ICON_SIZE, ICON_SIZE),
        false,
        &handler,
    );
    image.setTemplate(true);
    Some(image)
}

fn counts_line(payload: &MenubarPayload) -> String {
    format!(
        "{} ok · {} warn · {} err · {} missing{}",
        payload.counts.ok,
        payload.counts.warnings,
        payload.counts.errors,
        payload.counts.missing,
        if payload.counts.running_jobs > 0 {
            format!(" · {} job(s) running", payload.counts.running_jobs)
        } else {
            String::new()
        }
    )
}

/// Append a disabled informational row.
fn add_info_item(menu: &NSMenu, mtm: MainThreadMarker, text: &str) {
    let item = NSMenuItem::new(mtm);
    item.setTitle(&NSString::from_str(text));
    item.setEnabled(false);
    menu.addItem(&item);
}

// ---------------------------------------------------------------------------
// The dropdown's content, decoupled from AppKit (ISC-190).
// ---------------------------------------------------------------------------

/// One row of the dropdown's content, independent of `NSMenu`/`NSMenuItem` —
/// this is what makes ISC-190's degraded-menu contract provable with a plain
/// `#[test]` instead of only a live AX walk.
#[derive(Debug, Clone, PartialEq, Eq)]
enum MenuEntry {
    /// A disabled informational row.
    Info(String),
    /// A horizontal separator.
    Separator,
    /// A clickable action row. There is no "disabled action" state in this
    /// menu — a degraded snapshot changes what is shown ABOVE these rows,
    /// never whether these three work.
    Action {
        title: &'static str,
        action: TrayAction,
        /// The Command-modified key equivalent shown at the row's trailing
        /// edge — native menus carry them (ISC-309, menu-bar surface);
        /// AppKit's default modifier for a lowercase equivalent is Command,
        /// so plain "r"/"o"/"q" render and fire as
        /// Cmd-R / Cmd-O / Cmd-Q while the menu is open.
        key: &'static str,
    },
}

/// The selector an action row invokes, kept as plain data so `menu_plan`
/// never touches `Sel`/AppKit — only `build_menu` maps it to the real
/// selector.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TrayAction {
    Refresh,
    OpenControlCenter,
    Quit,
}

/// The whole dropdown's content, computed from the latest poll result alone.
/// Pure: no AppKit, no I/O.
///
/// The three action rows are appended UNCONDITIONALLY, after the payload
/// section and regardless of whether `payload` is present — this IS the
/// entire ISC-190 contract: whatever detection did (succeeded, is still
/// running, or panicked), "Refresh Now" / "Open Control Center" / "Quit ADE
/// Status" keep working. `menu_always_carries_working_actions_even_when_...`
/// below pins exactly this.
fn menu_plan(payload: Option<&MenubarPayload>) -> Vec<MenuEntry> {
    let mut entries = Vec::new();
    match payload {
        Some(payload) => {
            entries.push(MenuEntry::Info(format!("ADE — {}", payload.label)));
            entries.push(MenuEntry::Info(counts_line(payload)));
            entries.push(MenuEntry::Separator);
            for item in payload.items.iter().take(MAX_CAPABILITY_ROWS) {
                // The glyph column is two characters wide ("ok" is the
                // longest), so labels align down the menu.
                entries.push(MenuEntry::Info(format!("{:<2} {}", item.glyph, item.label)));
            }
        }
        None => {
            entries.push(MenuEntry::Info("ADE — status unavailable".to_string()));
            entries.push(MenuEntry::Info(
                "Capability detection is not responding".to_string(),
            ));
        }
    }
    entries.push(MenuEntry::Separator);
    entries.push(MenuEntry::Action {
        title: "Refresh Now",
        action: TrayAction::Refresh,
        key: "r",
    });
    entries.push(MenuEntry::Action {
        title: "Open Control Center",
        action: TrayAction::OpenControlCenter,
        key: "o",
    });
    entries.push(MenuEntry::Separator);
    entries.push(MenuEntry::Action {
        title: "Quit ADE Status",
        action: TrayAction::Quit,
        key: "q",
    });
    entries
}

// ---------------------------------------------------------------------------
// App delegate — owns the NSStatusItem, renders snapshots on the main thread.
// ---------------------------------------------------------------------------

struct Ivars {
    shared: Shared,
    status_item: OnceCell<Retained<NSStatusItem>>,
    rendered_generation: Cell<Option<u64>>,
    /// The template status icon, drawn once. `None` inside means creation
    /// failed and every render falls back to the text-only title.
    icon: OnceCell<Option<Retained<NSImage>>>,
}

define_class!(
    // SAFETY:
    // - The superclass NSObject does not have any subclassing requirements.
    // - `Delegate` does not implement `Drop`.
    #[unsafe(super = NSObject)]
    #[thread_kind = MainThreadOnly]
    #[name = "AdeStatusDelegate"]
    #[ivars = Ivars]
    struct Delegate;

    // SAFETY: `NSObjectProtocol` has no safety requirements.
    unsafe impl NSObjectProtocol for Delegate {}

    // SAFETY: `NSApplicationDelegate` has no safety requirements.
    unsafe impl NSApplicationDelegate for Delegate {
        // SAFETY: The signature is correct.
        #[unsafe(method(applicationDidFinishLaunching:))]
        fn did_finish_launching(&self, _notification: &NSNotification) {
            // Proven ordering (PulseMenuBar.swift): create the status item
            // HERE, once AppKit has fully launched.
            let status_item =
                NSStatusBar::systemStatusBar().statusItemWithLength(NSVariableStatusItemLength);
            self.ivars()
                .status_item
                .set(status_item)
                .expect("applicationDidFinishLaunching runs once");
            self.render();

            let target: &AnyObject = self;
            // SAFETY: `tick:` is defined on this class below, and the
            // delegate outlives the timer (retained for the process
            // lifetime by `main` while `app.run()` never returns).
            unsafe {
                NSTimer::scheduledTimerWithTimeInterval_target_selector_userInfo_repeats(
                    TICK_SECS,
                    target,
                    sel!(tick:),
                    None,
                    true,
                );
            }
        }
    }

    impl Delegate {
        #[unsafe(method(tick:))]
        fn tick(&self, _timer: &NSTimer) {
            let generation = lock_snapshot(&self.ivars().shared).generation;
            if self.ivars().rendered_generation.get() == Some(generation) {
                return;
            }
            self.render();
        }

        #[unsafe(method(refreshNow:))]
        fn refresh_now(&self, _sender: Option<&AnyObject>) {
            spawn_one_shot_poll(Arc::clone(&self.ivars().shared));
        }

        #[unsafe(method(openControlCenter:))]
        fn open_control_center_action(&self, _sender: Option<&AnyObject>) {
            open_control_center();
        }

        #[unsafe(method(quitApp:))]
        fn quit_app(&self, _sender: Option<&AnyObject>) {
            NSApplication::sharedApplication(self.mtm()).terminate(None);
        }
    }
);

impl Delegate {
    fn new(mtm: MainThreadMarker, shared: Shared) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(Ivars {
            shared,
            status_item: OnceCell::new(),
            rendered_generation: Cell::new(None),
            icon: OnceCell::new(),
        });
        // SAFETY: The signature of `NSObject`'s `init` method is correct.
        unsafe { msg_send![super(this), init] }
    }

    /// Repaint the status button + dropdown from the latest snapshot.
    /// Main thread only (`&self` proves it: the class is MainThreadOnly).
    fn render(&self) {
        let mtm = self.mtm();
        let ivars = self.ivars();
        let (generation, latest) = {
            let guard = lock_snapshot(&ivars.shared);
            (guard.generation, guard.latest.clone())
        };
        ivars.rendered_generation.set(Some(generation));
        let payload = match &latest {
            Some(Ok(payload)) => Some(payload),
            _ => None,
        };

        let status_item = ivars
            .status_item
            .get()
            .expect("render only runs after applicationDidFinishLaunching");
        // ASCII status vocabulary (Phase F): healthy is a bare template icon,
        // needs-attention adds the count, broken adds "!", and a tray with no
        // payload at all states "?" — the AX label carries the full sentence.
        let (title, ax_label) = match payload {
            Some(payload) => (tray_title(payload), format!("ADE — {}", payload.label)),
            None => ("?".to_string(), "ADE — status unavailable".to_string()),
        };
        if let Some(button) = status_item.button(mtm) {
            match ivars.icon.get_or_init(status_image) {
                Some(image) => {
                    button.setImage(Some(image));
                    button.setImagePosition(if title.is_empty() {
                        NSCellImagePosition::ImageOnly
                    } else {
                        NSCellImagePosition::ImageLeft
                    });
                    button.setTitle(&NSString::from_str(&title));
                }
                None => {
                    // Text-only fallback: the app name keeps the item findable
                    // when there is no icon to carry that job.
                    let text = if title.is_empty() {
                        "ADE".to_string()
                    } else {
                        format!("ADE {title}")
                    };
                    button.setTitle(&NSString::from_str(&text));
                }
            }
            button.setAccessibilityLabel(Some(&NSString::from_str(&ax_label)));
        }
        // Replacing the whole menu is fine even while it is open.
        status_item.setMenu(Some(&self.build_menu(payload)));
    }

    fn build_menu(&self, payload: Option<&MenubarPayload>) -> Retained<NSMenu> {
        let mtm = self.mtm();
        let menu = NSMenu::new(mtm);
        // Info rows are plain disabled items; opt out of AppKit's automatic
        // enabling so `setEnabled` is authoritative.
        menu.setAutoenablesItems(false);
        for entry in menu_plan(payload) {
            match entry {
                MenuEntry::Info(text) => add_info_item(&menu, mtm, &text),
                MenuEntry::Separator => menu.addItem(&NSMenuItem::separatorItem(mtm)),
                MenuEntry::Action { title, action, key } => {
                    let sel = match action {
                        TrayAction::Refresh => sel!(refreshNow:),
                        TrayAction::OpenControlCenter => sel!(openControlCenter:),
                        TrayAction::Quit => sel!(quitApp:),
                    };
                    menu.addItem(&self.action_item(mtm, title, sel, key));
                }
            }
        }
        menu
    }

    fn action_item(
        &self,
        mtm: MainThreadMarker,
        title: &str,
        action: Sel,
        key: &str,
    ) -> Retained<NSMenuItem> {
        let item = NSMenuItem::new(mtm);
        item.setTitle(&NSString::from_str(title));
        item.setEnabled(true);
        // Native menus carry their key equivalents; a lowercase equivalent
        // defaults to the Command modifier, which is exactly the convention
        // (Cmd-Q quit, Cmd-R refresh, Cmd-O open).
        item.setKeyEquivalent(&NSString::from_str(key));
        let target: &AnyObject = self;
        // SAFETY: `action` is a selector defined on this class, and the
        // delegate (the unretained target) outlives every menu it builds —
        // it is the app delegate, kept alive in `main` for the process
        // lifetime.
        unsafe {
            item.setTarget(Some(target));
            item.setAction(Some(action));
        }
        item
    }
}

fn main() {
    let mtm = MainThreadMarker::new().expect("ade-status must run on the main thread");

    let shared: Shared = Arc::new(Mutex::new(Snapshot::default()));
    spawn_poll_loop(Arc::clone(&shared));

    let app = NSApplication::sharedApplication(mtm);
    app.setActivationPolicy(NSApplicationActivationPolicy::Accessory);

    let delegate = Delegate::new(mtm, shared);
    app.setDelegate(Some(ProtocolObject::from_ref(&*delegate)));
    app.run();
}

// ---------------------------------------------------------------------------
// ISC-190 — tray degradation: when detection itself fails, the menu still
// opens with a degraded notice and "Open Control Center"/"Quit" keep
// working; the tray never hangs and never crashes.
//
// Everything here is pure Rust (`menu_plan`, `poll_once_with`) — no AppKit,
// no `MainThreadMarker` needed, so these run as ordinary `#[test]`s under
// plain `cargo test`, not just as a live/manual check.
// ---------------------------------------------------------------------------
#[cfg(test)]
mod tests {
    use super::*;
    use ade_core::types::{ExecOpts, ExecResult};
    use std::time::Instant;

    fn sample_payload() -> MenubarPayload {
        poll_once_with(
            std::sync::Arc::new(|_argv: &[String], _opts: &ExecOpts| {
                ExecResult::failure(127, "not installed")
            }),
            std::sync::Arc::new(|_name: &str| None),
        )
        .expect("a fully-missing but non-panicking probe set still completes")
    }

    fn action_sequence(entries: &[MenuEntry]) -> Vec<TrayAction> {
        entries
            .iter()
            .filter_map(|entry| match entry {
                MenuEntry::Action { action, .. } => Some(*action),
                _ => None,
            })
            .collect()
    }

    /// The literal ISC-190 contract: when there is no payload at all (never
    /// polled, or the last poll failed), the menu still opens with a
    /// degraded notice AND its three actions are present and unconditional.
    #[test]
    fn menu_always_carries_working_actions_even_when_detection_failed() {
        let plan = menu_plan(None);
        assert!(
            plan.contains(&MenuEntry::Info("ADE — status unavailable".to_string())),
            "missing the degraded notice: {plan:?}"
        );
        assert!(
            plan.contains(&MenuEntry::Info(
                "Capability detection is not responding".to_string()
            )),
            "missing the degraded detail line: {plan:?}"
        );
        assert_eq!(
            action_sequence(&plan),
            vec![
                TrayAction::Refresh,
                TrayAction::OpenControlCenter,
                TrayAction::Quit,
            ],
            "Refresh Now / Open Control Center / Quit must all be present and in order even with no payload"
        );
    }

    /// The same three actions in the same order also survive a HEALTHY
    /// payload — a regression guard so nobody accidentally makes them
    /// conditional on `payload` while "fixing" the degraded case above.
    /// ISC-309 (menu-bar surface): the three action rows carry native
    /// Command key equivalents — Cmd-R refresh, Cmd-O open, Cmd-Q quit —
    /// in BOTH the healthy and the degraded plan, because the equivalents
    /// are part of the same unconditional action block ISC-190 pinned.
    #[test]
    fn action_rows_carry_native_key_equivalents() {
        let healthy = sample_payload();
        for plan in [menu_plan(None), menu_plan(Some(&healthy))] {
            let keys: Vec<&str> = plan
                .into_iter()
                .filter_map(|entry| match entry {
                    MenuEntry::Action { key, .. } => Some(key),
                    _ => None,
                })
                .collect();
            assert_eq!(keys, vec!["r", "o", "q"]);
        }
    }

    #[test]
    fn a_healthy_payload_carries_the_same_three_actions() {
        let payload = sample_payload();
        let plan = menu_plan(Some(&payload));
        assert_eq!(
            action_sequence(&plan),
            vec![
                TrayAction::Refresh,
                TrayAction::OpenControlCenter,
                TrayAction::Quit,
            ]
        );
    }

    /// Simulates detection itself failing via an injected failing (panicking)
    /// `which`: a `which` panic occurs directly inside one of
    /// `detect_capabilities`'s per-capability scoped threads (`which` is
    /// called synchronously inside `probe_one`, unlike `exec`, which is
    /// wrapped in `with_timeout`'s OWN spawned thread) — so it reproduces,
    /// byte-for-byte, the real failure path this app is built to survive:
    /// `ade_core::gui::inventory::detect_capabilities`'s
    /// `handle.join().expect("detect thread")` panicking on the CALLING
    /// thread, which is exactly the panic `poll_once`'s `catch_unwind` exists
    /// to catch. Proves the poll never propagates that panic — which is what
    /// lets `spawn_poll_loop`'s `loop` keep polling on the next cycle instead
    /// of silently dying, and what gives `render()` a `Result` to match on
    /// instead of crashing the process.
    #[test]
    fn poll_once_survives_a_panicking_probe_without_crashing() {
        let exec: ExecFn = std::sync::Arc::new(|_argv: &[String], _opts: &ExecOpts| {
            ExecResult::failure(127, "not installed")
        });
        let panicking_which: WhichFn =
            std::sync::Arc::new(|_name: &str| panic!("simulated dead detector"));
        let result = poll_once_with(exec, panicking_which);
        assert!(
            result.is_err(),
            "a panic inside detection must degrade to Err, never propagate"
        );
    }

    /// The "killall simulating a dead detector" case, automated: an `exec`
    /// that panics on every call simulates one specific tool's probe
    /// subprocess wrapper being dead. Unlike `which` above, `exec` calls are
    /// wrapped in `with_timeout`'s own spawned thread — when that thread
    /// panics, its `Sender` is dropped mid-unwind, so `recv_timeout` returns
    /// almost immediately (not after the full timeout) with the "probe timed
    /// out" fallback `ExecResult`, and `detect_capabilities` treats it as an
    /// ordinary failed probe for THAT ONE capability. The pass as a whole
    /// still completes and reports Ok — a single dead detector cannot take
    /// down the rest, and cannot hang the tray.
    #[test]
    fn poll_once_isolates_a_single_dead_probe_without_failing_the_whole_pass() {
        let panicking_exec: ExecFn = std::sync::Arc::new(|_argv: &[String], _opts: &ExecOpts| {
            panic!("simulated dead detector")
        });
        let which: WhichFn = std::sync::Arc::new(|_name: &str| None);
        let started = Instant::now();
        let result = poll_once_with(panicking_exec, which);
        assert!(
            result.is_ok(),
            "one dead probe subprocess must not fail the whole detection pass: {result:?}"
        );
        assert!(
            started.elapsed() < Duration::from_secs(2),
            "an immediately-panicking probe must resolve fast, not wait out the full probe timeout"
        );
    }

    /// The quantitative half of "never hangs": even when EVERY probe's
    /// subprocess wrapper hangs forever (never returns, never panics — a
    /// genuinely dead detector, not just a fast-failing one), the whole pass
    /// still completes, bounded by the per-probe timeout — not the number of
    /// capabilities, since `detect_capabilities` runs them in parallel. Real
    /// wall-clock, not a mock: this is `with_timeout`'s bound proven all the
    /// way through the tray's own `poll_once_with`, at the app's own
    /// `PROBE_TIMEOUT_SECS`.
    #[test]
    fn poll_once_is_bounded_even_when_every_probe_hangs_forever() {
        let hung_exec: ExecFn = std::sync::Arc::new(|_argv: &[String], _opts: &ExecOpts| {
            std::thread::sleep(Duration::from_secs(3600));
            unreachable!("the timeout must win this race")
        });
        let which: WhichFn = std::sync::Arc::new(|_name: &str| None);
        let started = Instant::now();
        let result = poll_once_with(hung_exec, which);
        assert!(
            result.is_ok(),
            "a bounded-timeout pass still completes: {result:?}"
        );
        assert!(
            started.elapsed() < Duration::from_secs(PROBE_TIMEOUT_SECS * 2),
            "poll_once_with took {:?}, longer than 2x the probe timeout — the tray would hang",
            started.elapsed()
        );
    }
}
