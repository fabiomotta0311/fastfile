//! Fluxo completo da CLI sobre o extrato de exemplo, sem processo filho.

use lg_cli::args::{parse_args, Command};
use lg_cli::commands::run;
use lg_cli::Settings;
use lg_parse::Config;

const EXTRATO: &str = include_str!("../../../exemplos/extrato-2024-03.csv");

fn settings() -> Settings {
    Settings::resolve(&Config::default(), &Default::default()).unwrap()
}

fn exec(cmd: &Command) -> String {
    run(cmd, &settings(), &[("extrato-2024-03.csv", EXTRATO)], None).unwrap()
}

#[test]
fn categories_report_totals() {
    let out = exec(&Command::Categories);
    let last = out.lines().last().unwrap();
    assert!(last.starts_with("TOTAL"), "{out}");
    assert!(last.contains("R$ 7.736,21"), "{last}");
    assert!(last.contains("-R$ 4.994,68"), "{last}");
    assert!(out.contains("(sem categoria)"), "transferência sem regra: {out}");
}

#[test]
fn balance_single_account() {
    let out = exec(&Command::Balance { depth: None });
    assert!(out.contains("R$ 2.741,53"), "{out}");
    assert_eq!(out.lines().count(), 4, "{out}");
}

#[test]
fn monthly_period_report() {
    let out = exec(&Command::Period { by: "mes".into(), tax_rate: Some("10".into()) });
    assert!(out.contains("2024-03"), "{out}");
    assert!(out.contains("24"), "{out}");
}

#[test]
fn query_by_category_first_half_of_month() {
    let cmd = Command::Query {
        category: Some("mercado".into()),
        from: Some("2024-03-01".into()),
        to: Some("2024-03-15".into()),
        text: None,
        limit: None,
    };
    let out = exec(&cmd);
    assert!(out.contains("SUPERMERCADO DIA 123"), "{out}");
    assert_eq!(out.lines().count(), 3, "{out}");
}

#[test]
fn argv_roundtrip() {
    let args: Vec<String> = ["--currency", "BRL", "categories", "a.csv"].iter().map(|s| s.to_string()).collect();
    let inv = parse_args(&args).unwrap();
    assert_eq!(inv.command, Command::Categories);
    assert_eq!(inv.inputs, vec!["a.csv"]);
}
