//! ADE Control Center — Dioxus contender.
//!
//! Renders the same `ade_core::gui::views` models as the egui crate, so the
//! bake-off compares toolkits and nothing else. The renderer is chosen at
//! build time (`--features webview` or `--features native`, mutually
//! exclusive); the component tree is identical under both.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod console;
mod icons;
mod inspector;
mod provider_list;
mod sidebar;
mod styles;

#[cfg(all(feature = "webview", feature = "native"))]
compile_error!(
    "`webview` and `native` are mutually exclusive renderers: build with exactly one \
     (`--features webview` or `--no-default-features --features native`)."
);

#[cfg(not(any(feature = "webview", feature = "native")))]
compile_error!("one renderer feature is required: `webview` or `native`.");

/// Window metrics, shared by both renderers so the two screenshots are the
/// same size — and the same as the egui twin's `ViewportBuilder`.
const TITLE: &str = "ADE Control Center";
const DEFAULT_SIZE: (f64, f64) = (1100.0, 740.0);
const MIN_SIZE: (f64, f64) = (860.0, 560.0);

/// wry over the system WebView. `Config::with_window` takes tao's
/// `WindowBuilder`, which `dioxus::desktop` re-exports.
#[cfg(all(feature = "webview", not(feature = "native")))]
fn main() {
    use dioxus::desktop::{Config, LogicalSize, WindowBuilder};

    let window = WindowBuilder::new()
        .with_title(TITLE)
        .with_inner_size(LogicalSize::new(DEFAULT_SIZE.0, DEFAULT_SIZE.1))
        .with_min_inner_size(LogicalSize::new(MIN_SIZE.0, MIN_SIZE.1));
    dioxus::LaunchBuilder::desktop()
        .with_cfg(Config::new().with_window(window))
        .launch(app::App);
}

/// Blitz, with no webview at all. `dioxus_native::Config` takes winit's
/// `WindowAttributes` rather than a builder, and `launch_cfg` is the native
/// renderer's own entry point — `LaunchBuilder::new()` would route here too,
/// but its `deprecated` attribute does not list `native` among the renderer
/// features, so going through it emits a spurious warning.
#[cfg(all(feature = "native", not(feature = "webview")))]
fn main() {
    use dioxus::native::{launch_cfg, Config, LogicalSize, WindowAttributes};

    let attributes = WindowAttributes::default()
        .with_title(TITLE)
        .with_inner_size(LogicalSize::new(DEFAULT_SIZE.0, DEFAULT_SIZE.1))
        .with_min_inner_size(LogicalSize::new(MIN_SIZE.0, MIN_SIZE.1));
    let config = Config::new().with_window_attributes(attributes);
    launch_cfg(app::App, Vec::new(), vec![Box::new(config)]);
}
