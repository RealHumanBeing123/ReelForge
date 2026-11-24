#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod capture;
mod interchange;
mod model;
mod particles;
mod playback;
mod preview;
mod probe;
mod record;
mod farm;
mod raw;
mod render;
mod scopes;
mod sync;
mod tracker;
mod util;

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title("ReelForge")
            .with_inner_size([1440.0, 860.0])
            .with_min_inner_size([960.0, 600.0])
            .with_drag_and_drop(true),
        ..Default::default()
    };
    eframe::run_native(
        "ReelForge",
        options,
        Box::new(|cc| Ok(Box::new(app::ReelForge::new(cc)))),
    )
}
