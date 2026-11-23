//! Execução dos comandos sobre extratos já lidos.

use lg_core::{Date, Error, Result, Txn};
use lg_parse::{parse_statement, StatementOptions};
use lg_report::group::Granularity;
use lg_report::{render, CategoryTotals, Summary};
use lg_rules::builtin::builtin_rules;
use lg_rules::rulefile::{format_rules, parse_rules};
use lg_rules::Engine;
use lg_store::{Query, Store};
use lg_tax::Rate;

use crate::args::Command;
use crate::settings::Settings;

/// Extrato de entrada já lido do disco: (nome para mensagens, conteúdo).
pub type Input<'a> = (&'a str, &'a str);

pub fn build_engine(settings: &Settings, rules_text: Option<&str>) -> Result<Engine> {
    let mut engine = Engine::default();
    // Regras do usuário vêm antes: em empate total, a definida primeiro vence.
    if let Some(text) = rules_text {
        engine.extend(parse_rules(text)?);
    }
    if settings.use_builtin_rules {
        engine.extend(builtin_rules());
    }
    Ok(engine)
}

/// Lê, categoriza e armazena todos os extratos.
pub fn load(settings: &Settings, inputs: &[Input<'_>], engine: &Engine) -> Result<Store> {
    let mut store = Store::new();
    for (name, text) in inputs {
        let mut opts = StatementOptions::new(settings.account.clone());
        opts.default_currency = settings.currency;
        opts.rounding = settings.rounding;
        opts.negate = settings.negate_import;
        let mut txns = parse_statement(text, &opts).map_err(|e| Error::Config(format!("{name}: {e}")))?;
        engine.apply(&mut txns);
        store.extend(txns);
    }
    Ok(store)
}

fn all(store: &Store) -> Vec<&Txn> {
    store.iter().map(|(_, t)| t).collect()
}

pub fn run(command: &Command, settings: &Settings, inputs: &[Input<'_>], rules_text: Option<&str>) -> Result<String> {
    let engine = build_engine(settings, rules_text)?;
    if let Command::Rules = command {
        return Ok(format_rules(engine.rules()));
    }
    if inputs.is_empty() {
        return Err(Error::Config("informe ao menos um extrato".into()));
    }
    let store = load(settings, inputs, &engine)?;
    let locale = &settings.locale;
    match command {
        Command::Rules => unreachable!(),
        Command::Categories => {
            let (totals, skipped) = CategoryTotals::compute(&all(&store), settings.currency)?;
            let mut out = render::category_report(&totals, locale)?;
            if skipped > 0 {
                out.push_str(&format!("\n{skipped} lançamento(s) em outra moeda ignorado(s)\n"));
            }
            Ok(out)
        }
        Command::Balance { depth } => {
            let s = Summary::compute(&all(&store), settings.currency, *depth)?;
            Ok(render::balance_report(&s, locale))
        }
        Command::Period { by, tax_rate } => {
            let g = Granularity::parse(by).ok_or_else(|| Error::Config(format!("período desconhecido: {by}")))?;
            let rate = match tax_rate {
                Some(r) => Rate::parse_percent(r)?,
                None => Rate::ZERO,
            };
            let txns: Vec<Txn> = all(&store).into_iter().cloned().collect();
            let groups = settings.calendar.group(&txns, g);
            render::period_report(&groups, rate, locale)
        }
        Command::Query { category, from, to, text, limit } => {
            let mut q = Query::new();
            q.category = category.clone();
            q.from = from.as_deref().map(Date::parse_any).transpose()?;
            q.to = to.as_deref().map(Date::parse_any).transpose()?;
            q.text = text.clone();
            q.limit = *limit;
            let rows = q.run(&store);
            let txns: Vec<&Txn> = rows.iter().map(|(_, t)| *t).collect();
            Ok(render::transactions_table(&txns, locale))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const EXTRATO: &str = "data;descricao;valor\n2024-03-05;PADARIA BOM PAO;-12,50\n2024-03-06;PAGTO SALARIO EMPRESA;5000,00\n2024-03-07;NETFLIX.COM;-39,90\n";

    #[test]
    fn categories_uses_builtin_rules() {
        let out = run(&Command::Categories, &Settings::default(), &[("x.csv", EXTRATO)], None).unwrap();
        assert!(out.contains("alimentacao:padaria"), "{out}");
        assert!(out.contains("receita:salario"), "{out}");
        assert!(out.contains("assinaturas:video"), "{out}");
    }

    #[test]
    fn user_rules_come_first() {
        let rules = "NETFLIX* => lazer:streaming\n";
        let out = run(&Command::Categories, &Settings::default(), &[("x.csv", EXTRATO)], Some(rules)).unwrap();
        assert!(out.contains("lazer:streaming"), "{out}");
        assert!(!out.contains("assinaturas:video"), "{out}");
    }

    #[test]
    fn query_by_text() {
        let cmd = Command::Query { category: None, from: None, to: None, text: Some("padaria".into()), limit: None };
        let out = run(&cmd, &Settings::default(), &[("x.csv", EXTRATO)], None).unwrap();
        assert_eq!(out.lines().count(), 3, "{out}");
    }

    #[test]
    fn needs_input() {
        assert!(run(&Command::Categories, &Settings::default(), &[], None).is_err());
        assert!(run(&Command::Rules, &Settings::default(), &[], None).unwrap().contains("PADARIA*"));
    }
}
