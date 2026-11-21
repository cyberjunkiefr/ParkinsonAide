#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod audio;
mod config;
mod phrases;
mod stt;

fn main() -> eframe::Result<()> {
    let mut viewport = egui::ViewportBuilder::default()
        .with_title("ParkinsonAide")
        .with_inner_size([1100.0, 900.0])
        .with_maximized(true)
        .with_min_inner_size([820.0, 640.0]);
    if let Ok(img) = image::load_from_memory(include_bytes!("../icons/record.png")) {
        let img = img.to_rgba8();
        let (w, h) = img.dimensions();
        viewport = viewport.with_icon(egui::IconData { rgba: img.into_raw(), width: w, height: h });
    }
    eframe::run_native(
        "ParkinsonAide",
        eframe::NativeOptions { viewport, ..Default::default() },
        Box::new(|cc| Ok(Box::new(app::ParkinsonApp::new(cc)))),
    )
}
