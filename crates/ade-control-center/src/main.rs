//! ADE Control Center — native macOS GUI for the ADE Bootstrapper.
//! Presentation layer only: all domain logic lives in ade-core, in-process.
//! Visual changes are verified by snapshot tests — see docs/UI-DESIGN-REFERENCE.md.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod activity;
mod app;
mod capability_page;
mod chrome;
mod data;
mod gallery;
mod header;
mod icons;
mod modals;
mod nav;
mod overview;
mod projects;
mod row;
#[cfg(test)]
mod testkit;
mod theme;

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("ADE Control Center")
            .with_inner_size([1100.0, 740.0])
            .with_min_inner_size([860.0, 560.0])
            // The glass sidebar (Phase E) needs a non-opaque surface so the
            // native material behind the strip can show. Content stays fully
            // opaque: the panels paint their own fills, and on the Opaque
            // chrome tier the whole window is covered by them.
            .with_transparent(true)
            // The canonical macOS shape: the content (and the glass strip)
            // runs the full window height, the titlebar is a transparent
            // overlay, and the traffic lights float over the sidebar. Without
            // this the titlebar sat as an opaque white band above the glass.
            .with_fullsize_content_view(true)
            .with_titlebar_shown(false)
            .with_title_shown(false)
            .with_app_id("com.ade-bootstrapper.control-center"),
        ..Default::default()
    };
    eframe::run_native(
        "ADE Control Center",
        options,
        Box::new(|cc| Ok(Box::new(app::ControlCenterApp::new(cc)))),
    )
}
