use std::process::ExitCode;

use lg_cli::args::parse_args;
use lg_cli::commands::run;
use lg_cli::Settings;
use lg_parse::Config;

fn read(path: &str) -> Result<String, String> {
    std::fs::read_to_string(path).map_err(|e| format!("{path}: {e}"))
}

fn main_inner() -> Result<String, String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let inv = parse_args(&args).map_err(|e| e.to_string())?;
    let config_path = inv.flags.config.clone().unwrap_or_else(|| "ledger.conf".to_string());
    let config = match read(&config_path) {
        Ok(text) => Config::parse(&text).map_err(|e| e.to_string())?,
        Err(_) if inv.flags.config.is_none() => Config::default(),
        Err(e) => return Err(e),
    };
    let settings = Settings::resolve(&config, &inv.flags).map_err(|e| e.to_string())?;
    let rules_text = settings.rules_file.as_deref().map(read).transpose()?;
    let texts: Vec<(String, String)> = inv
        .inputs
        .iter()
        .map(|p| read(p).map(|t| (p.clone(), t)))
        .collect::<Result<_, _>>()?;
    let inputs: Vec<(&str, &str)> = texts.iter().map(|(n, t)| (n.as_str(), t.as_str())).collect();
    run(&inv.command, &settings, &inputs, rules_text.as_deref()).map_err(|e| e.to_string())
}

fn main() -> ExitCode {
    match main_inner() {
        Ok(out) => {
            print!("{out}");
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("ledger: {e}");
            ExitCode::from(2)
        }
    }
}
