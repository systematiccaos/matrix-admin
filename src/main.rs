mod app;
mod config;
mod matrix;
mod worker;

use app::MatrixAdminApp;

fn main() -> eframe::Result<()> {
    let native_options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default().with_inner_size([760.0, 640.0]),
        ..Default::default()
    };

    eframe::run_native(
        "Matrix Admin",
        native_options,
        Box::new(|_cc| Ok(Box::new(MatrixAdminApp::new()))),
    )
}
