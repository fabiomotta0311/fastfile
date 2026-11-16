//! Binário executável `ff-ui` / `fastfile-ui` (§4 e Issue #13).

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use eframe::NativeOptions;
use ff_ui::FastFileApp;

fn main() -> eframe::Result<()> {
    let native_options = NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("FastFile — Busca, Prévia, Diff e Uso de Disco")
            .with_inner_size([1180.0, 760.0])
            .with_min_inner_size([800.0, 500.0]),
        ..Default::default()
    };

    eframe::run_native(
        "FastFile",
        native_options,
        Box::new(|_cc| Ok(Box::new(FastFileApp::new()))),
    )
}
