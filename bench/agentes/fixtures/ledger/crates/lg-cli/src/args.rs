//! Argumentos da linha de comando (sem dependências externas).
//!
//! ```text
//! ledger [--config ARQ] [--locale TAG] [--currency COD] [--fiscal-start N]
//!        [--rounding MODO] [--account CONTA] <comando> [opções] <extratos...>
//!
//! comandos:
//!   categories              receitas e despesas por categoria
//!   balance [--depth N]     saldos por conta
//!   period [--by mes|trimestre|ano] [--tax-rate PCT]
//!   query [--category C] [--from D] [--to D] [--text T] [--limit N]
//!   rules                   exporta as regras efetivas
//! ```

use lg_core::{Error, Result};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Flags {
    pub config: Option<String>,
    pub locale: Option<String>,
    pub currency: Option<String>,
    pub fiscal_start: Option<u8>,
    pub rounding: Option<String>,
    pub account: Option<String>,
    pub rules_file: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    Categories,
    Balance { depth: Option<usize> },
    Period { by: String, tax_rate: Option<String> },
    Query { category: Option<String>, from: Option<String>, to: Option<String>, text: Option<String>, limit: Option<usize> },
    Rules,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Invocation {
    pub flags: Flags,
    pub command: Command,
    pub inputs: Vec<String>,
}

fn need(args: &[String], i: usize, name: &str) -> Result<String> {
    args.get(i + 1).cloned().ok_or_else(|| Error::Config(format!("{name} exige um valor")))
}

fn number<T: std::str::FromStr>(v: &str, name: &str) -> Result<T> {
    v.parse().map_err(|_| Error::Config(format!("{name}: número inválido '{v}'")))
}

pub fn parse_args(args: &[String]) -> Result<Invocation> {
    let mut flags = Flags::default();
    let mut i = 0;
    // Opções globais até o nome do comando.
    while i < args.len() && args[i].starts_with("--") {
        let a = args[i].as_str();
        match a {
            "--config" => flags.config = Some(need(args, i, a)?),
            "--locale" => flags.locale = Some(need(args, i, a)?),
            "--currency" => flags.currency = Some(need(args, i, a)?),
            "--fiscal-start" => flags.fiscal_start = Some(number(&need(args, i, a)?, a)?),
            "--rounding" => flags.rounding = Some(need(args, i, a)?),
            "--account" => flags.account = Some(need(args, i, a)?),
            "--rules" => flags.rules_file = Some(need(args, i, a)?),
            _ => return Err(Error::Config(format!("opção desconhecida: {a}"))),
        }
        i += 2;
    }
    let name = args.get(i).ok_or_else(|| Error::Config("comando ausente".into()))?.clone();
    i += 1;
    let mut opts: Vec<(String, String)> = Vec::new();
    let mut inputs = Vec::new();
    while i < args.len() {
        if args[i].starts_with("--") {
            opts.push((args[i].clone(), need(args, i, &args[i])?));
            i += 2;
        } else {
            inputs.push(args[i].clone());
            i += 1;
        }
    }
    let take = |k: &str| opts.iter().rev().find(|(n, _)| n == k).map(|(_, v)| v.clone());
    let allowed: &[&str] = match name.as_str() {
        "categories" | "rules" => &[],
        "balance" => &["--depth"],
        "period" => &["--by", "--tax-rate"],
        "query" => &["--category", "--from", "--to", "--text", "--limit"],
        _ => return Err(Error::Config(format!("comando desconhecido: {name}"))),
    };
    if let Some((bad, _)) = opts.iter().find(|(n, _)| !allowed.contains(&n.as_str())) {
        return Err(Error::Config(format!("{name}: opção desconhecida {bad}")));
    }
    let command = match name.as_str() {
        "categories" => Command::Categories,
        "rules" => Command::Rules,
        "balance" => Command::Balance { depth: take("--depth").map(|v| number(&v, "--depth")).transpose()? },
        "period" => Command::Period { by: take("--by").unwrap_or_else(|| "mes".into()), tax_rate: take("--tax-rate") },
        "query" => Command::Query {
            category: take("--category"),
            from: take("--from"),
            to: take("--to"),
            text: take("--text"),
            limit: take("--limit").map(|v| number(&v, "--limit")).transpose()?,
        },
        _ => unreachable!(),
    };
    Ok(Invocation { flags, command, inputs })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(s: &str) -> Vec<String> {
        s.split_whitespace().map(String::from).collect()
    }

    #[test]
    fn global_flags_then_command() {
        let inv = parse_args(&v("--locale en-US --fiscal-start 4 period --by ano a.csv b.csv")).unwrap();
        assert_eq!(inv.flags.locale.as_deref(), Some("en-US"));
        assert_eq!(inv.flags.fiscal_start, Some(4));
        assert_eq!(inv.command, Command::Period { by: "ano".into(), tax_rate: None });
        assert_eq!(inv.inputs, vec!["a.csv", "b.csv"]);
    }

    #[test]
    fn query_options() {
        let inv = parse_args(&v("query --category mercado --from 2024-03-01 --limit 5 x.csv")).unwrap();
        match inv.command {
            Command::Query { category, from, to, limit, .. } => {
                assert_eq!(category.as_deref(), Some("mercado"));
                assert_eq!(from.as_deref(), Some("2024-03-01"));
                assert_eq!(to, None);
                assert_eq!(limit, Some(5));
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn errors() {
        assert!(parse_args(&v("")).is_err());
        assert!(parse_args(&v("voar")).is_err());
        assert!(parse_args(&v("--locale")).is_err());
        assert!(parse_args(&v("--nada x categories")).is_err());
        assert!(parse_args(&v("balance --by ano")).is_err());
        assert!(parse_args(&v("balance --depth x")).is_err());
    }
}
