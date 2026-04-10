use bluetrack::app::BluetoothApp;
use eframe::egui;

fn main() -> eframe::Result<()> {
    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1520.0, 920.0])
            .with_min_inner_size([1200.0, 720.0])
            .with_title("BlueTrack"),
        ..Default::default()
    };

    eframe::run_native(
        "BlueTrack",
        native_options,
        Box::new(|cc| Ok(Box::new(BluetoothApp::new(cc)))),
    )
}
