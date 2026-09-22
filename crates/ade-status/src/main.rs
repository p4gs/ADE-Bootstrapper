//! ADE Status — menu-bar helper.
//!
//! The helper is a macOS `NSStatusItem` app (see `macos.rs`). The workspace
//! still has to build from source on Linux and Windows, so this entry point
//! compiles everywhere: on macOS it runs the AppKit app; elsewhere it says
//! plainly that there is no tray for this platform yet and exits 0, so a
//! `cargo build --workspace` on a Linux or Windows box never fails here.

#[cfg(target_os = "macos")]
mod macos;

#[cfg(target_os = "macos")]
fn main() {
    macos::main();
}

#[cfg(not(target_os = "macos"))]
fn main() {
    println!("ade-status: the menu-bar helper is macOS-only; run `ade-control-center` directly on this platform.");
}
