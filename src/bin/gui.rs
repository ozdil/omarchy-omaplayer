use eframe::egui;
use omaplayer::gui::OmaPlayerApp;

fn main() -> eframe::Result<()> {
    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1040.0, 680.0])
            .with_min_inner_size([800.0, 520.0])
            .with_title("OmaPlayer • Müzik İstasyonu"),
        ..Default::default()
    };

    eframe::run_native(
        "OmaPlayer",
        native_options,
        Box::new(|cc| Ok(Box::new(OmaPlayerApp::new(cc)))),
    )
}
