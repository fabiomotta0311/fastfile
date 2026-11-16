//! Binário executável `fastfile` / `ff-ui` (§4 e Issue #13).
//!
//! Quando invocado sem argumentos (ex: Win + R, clique duplo ou atalho no menu iniciar),
//! abre a interface gráfica nativa egui sem janela de console.
//!
//! Quando invocado com argumentos no terminal (ex: `fastfile find ...`, `fastfile grep ...`),
//! conecta ao console do processo pai e executa a CLI unificada.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use eframe::NativeOptions;
use ff_ui::FastFileApp;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();

    // Se foram passados subcomandos de linha de comando (ex: `fastfile find ...`, `fastfile grep ...`, `fastfile mcp`):
    if args.len() > 1 && args[1] != "--ui" && args[1] != "-ui" && args[1] != "gui" {
        #[cfg(windows)]
        unsafe {
            use windows_sys::Win32::System::Console::{AttachConsole, ATTACH_PARENT_PROCESS};
            AttachConsole(ATTACH_PARENT_PROCESS);
        }

        let current = std::env::current_exe().unwrap_or_default();
        let cli_exe = current.with_file_name("ff.exe");
        if cli_exe.exists() {
            let status = std::process::Command::new(cli_exe)
                .args(&args[1..])
                .status();
            match status {
                Ok(s) => std::process::exit(s.code().unwrap_or(0)),
                Err(_) => {
                    ff_cli::main();
                    return Ok(());
                }
            }
        } else {
            ff_cli::main();
            return Ok(());
        }
    }

    // Sem argumentos (ou com flag explícita --ui): abre a interface gráfica egui
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
    .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;

    Ok(())
}
