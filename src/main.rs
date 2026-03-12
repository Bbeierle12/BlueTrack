mod app;
mod backend;
mod model;
mod settings;
mod store;
mod ui;

use app::BluetoothApp;
use eframe::egui;

fn main() -> eframe::Result<()> {
    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1480.0, 920.0])
            .with_min_inner_size([1200.0, 720.0])
            .with_title("Bluetooth Mapper"),
        ..Default::default()
    };

    eframe::run_native(
        "Bluetooth Mapper",
        native_options,
        Box::new(|cc| Ok(Box::new(BluetoothApp::new(cc)))),
    )
}
