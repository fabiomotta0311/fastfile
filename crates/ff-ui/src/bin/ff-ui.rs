//! Alias `ff-ui` para retrocompatibilidade com atalhos e scripts existentes.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let initial_locale = ff_ui::Locale::detect_from_system();
    let mut viewport = egui::ViewportBuilder::default()
        .with_title(initial_locale.window_title())
        .with_inner_size([1180.0, 760.0])
        .with_min_inner_size([800.0, 500.0]);

    if let Some(icon) = ff_ui::load_app_icon() {
        viewport = viewport.with_icon(icon);
    }

    let native_options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };

    eframe::run_native(
        "FastFile",
        native_options,
        Box::new(|_cc| Ok(Box::new(ff_ui::FastFileApp::new()))),
    )
    .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;

    Ok(())
}
