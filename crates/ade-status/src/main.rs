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
use std::process::Command;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use ade_core::exec::{real_exec, real_which};
use ade_core::gui::inventory::{detect_capabilities, DetectOptions};
use ade_core::gui::jobs::{read_jobs_summary, read_last_finished};
use ade_core::gui::menubar::{build_menubar_payload, AggregateStatus, MenubarPayload};
use ade_core::gui::state::{ade_home_from_env, load_gui_state};
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, ProtocolObject, Sel};
use objc2::{define_class, msg_send, sel, AnyThread, DefinedClass, MainThreadOnly};
use objc2_app_kit::{
    NSAccessibility, NSApplication, NSApplicationActivationPolicy, NSApplicationDelegate, NSColor,
    NSForegroundColorAttributeName, NSMenu, NSMenuItem, NSStatusBar, NSStatusItem,
    NSVariableStatusItemLength,
};
use objc2_foundation::{
    MainThreadMarker, NSAttributedString, NSAttributedStringKey, NSDictionary, NSNotification,
    NSObject, NSObjectProtocol, NSString, NSTimer,
};

const POLL_SECS: u64 = 10;
const TICK_SECS: f64 = 2.0;
const PROBE_TIMEOUT_SECS: u64 = 4;
const MAX_CAPABILITY_ROWS: usize = 20;
const CONTROL_CENTER_BUNDLE: &str = "com.ade-bootstrapper.control-center";

// ---------------------------------------------------------------------------
// Data layer (background thread) — unchanged detection pipeline.
// ---------------------------------------------------------------------------

fn poll_once() -> Result<MenubarPayload, String> {
    std::panic::catch_unwind(|| {
        let home = ade_home_from_env();
        let state = load_gui_state(&home).state;
        let last_jobs = read_last_finished(&home);
        let latest = std::collections::BTreeMap::new();
        let capabilities = detect_capabilities(&DetectOptions {
            exec: real_exec(),
            which: real_which(),
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
    })
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

/// Status → dot color: green ok, orange warn, red error; gray is the
/// degraded/unknown path (no payload), handled by the caller.
fn status_color(status: AggregateStatus) -> Retained<NSColor> {
    match status {
        AggregateStatus::Ok => NSColor::systemGreenColor(),
        AggregateStatus::Warn => NSColor::systemOrangeColor(),
        AggregateStatus::Error => NSColor::systemRedColor(),
    }
}

/// A "●" attributed with the given foreground color, for the status button.
fn colored_dot(color: &NSColor) -> Retained<NSAttributedString> {
    // SAFETY: reading a constant AppKit attribute-name key.
    let key: &NSAttributedStringKey = unsafe { NSForegroundColorAttributeName };
    let keys: [&NSAttributedStringKey; 1] = [key];
    let objects: [&AnyObject; 1] = [color];
    let attributes = NSDictionary::from_slices(&keys, &objects);
    // SAFETY: the dictionary maps NSAttributedStringKey → NSColor, the
    // documented value type for NSForegroundColorAttributeName.
    unsafe {
        NSAttributedString::initWithString_attributes(
            NSAttributedString::alloc(),
            &NSString::from_str("●"),
            Some(&attributes),
        )
    }
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
// App delegate — owns the NSStatusItem, renders snapshots on the main thread.
// ---------------------------------------------------------------------------

struct Ivars {
    shared: Shared,
    status_item: OnceCell<Retained<NSStatusItem>>,
    rendered_generation: Cell<Option<u64>>,
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
        let (color, ax_label) = match payload {
            Some(payload) => (
                status_color(payload.status),
                format!("ADE — {}", payload.label),
            ),
            None => (
                NSColor::systemGrayColor(),
                "ADE — status unavailable".to_string(),
            ),
        };
        if let Some(button) = status_item.button(mtm) {
            button.setAttributedTitle(&colored_dot(&color));
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
        match payload {
            Some(payload) => {
                add_info_item(&menu, mtm, &format!("ADE — {}", payload.label));
                add_info_item(&menu, mtm, &counts_line(payload));
                menu.addItem(&NSMenuItem::separatorItem(mtm));
                for item in payload.items.iter().take(MAX_CAPABILITY_ROWS) {
                    add_info_item(&menu, mtm, &format!("{}  {}", item.glyph, item.label));
                }
            }
            None => {
                add_info_item(&menu, mtm, "ADE — status unavailable");
                add_info_item(&menu, mtm, "Capability detection is not responding");
            }
        }
        menu.addItem(&NSMenuItem::separatorItem(mtm));
        menu.addItem(&self.action_item(mtm, "Refresh Now", sel!(refreshNow:)));
        menu.addItem(&self.action_item(mtm, "Open Control Center", sel!(openControlCenter:)));
        menu.addItem(&NSMenuItem::separatorItem(mtm));
        menu.addItem(&self.action_item(mtm, "Quit ADE Status", sel!(quitApp:)));
        menu
    }

    fn action_item(&self, mtm: MainThreadMarker, title: &str, action: Sel) -> Retained<NSMenuItem> {
        let item = NSMenuItem::new(mtm);
        item.setTitle(&NSString::from_str(title));
        item.setEnabled(true);
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
