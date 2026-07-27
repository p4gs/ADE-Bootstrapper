//! ADE Control Center — native macOS GUI for the ADE Bootstrapper.
//! Presentation layer only: all domain logic lives in the TypeScript core,
//! consumed through the local `ade gui` API (see ade-gui-core).
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod data;
mod theme;

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("ADE Control Center")
            .with_inner_size([1100.0, 740.0])
            .with_min_inner_size([860.0, 560.0])
            .with_app_id("com.ade-bootstrapper.control-center"),
        ..Default::default()
    };
    eframe::run_native(
        "ADE Control Center",
        options,
        Box::new(|cc| Ok(Box::new(app::ControlCenterApp::new(cc)))),
    )
}
