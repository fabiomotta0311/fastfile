fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() > 1 && (args[1] == "--ui" || args[1] == "-ui" || args[1] == "gui") {
        let current_exe = std::env::current_exe().unwrap_or_default();
        let ui_exe = current_exe.with_file_name("fastfile.exe");
        let ui_target = if ui_exe.exists() {
            ui_exe
        } else {
            current_exe.with_file_name("ff-ui.exe")
        };
        if ui_target.exists() {
            let _ = std::process::Command::new(ui_target).spawn();
            return;
        }
    }
    ff_cli::main();
}
