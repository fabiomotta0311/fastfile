//! Extrato bancário em CSV -> transações normalizadas.
//!
//! O cabeçalho é obrigatório e as colunas são localizadas pelo nome (sem diferenciar
//! maiúsculas, acentos comuns aceitos): `data`, `descricao`/`histórico`, `valor` e,
//! opcionalmente, `moeda` e `categoria`. Linhas vazias e linhas que começam com `#` são
//! ignoradas.

use lg_core::{currency, AccountId, Currency, Date, Error, Money, Result, RoundingMode, Txn};

use crate::csv::{detect_separator, split_record};

#[derive(Debug, Clone)]
pub struct StatementOptions {
    pub account: AccountId,
    /// Moeda usada quando o extrato não tem coluna `moeda`.
    pub default_currency: &'static Currency,
    /// Separador; `None` detecta pelo cabeçalho.
    pub separator: Option<char>,
    pub rounding: RoundingMode,
    /// Inverte o sinal (extratos de cartão trazem compras como positivas).
    pub negate: bool,
}

impl StatementOptions {
    pub fn new(account: AccountId) -> Self {
        StatementOptions {
            account,
            default_currency: currency::brl(),
            separator: None,
            rounding: RoundingMode::HalfEven,
            negate: false,
        }
    }
}

#[derive(Debug, Default)]
struct Columns {
    date: Option<usize>,
    description: Option<usize>,
    amount: Option<usize>,
    currency: Option<usize>,
    category: Option<usize>,
}

fn fold(name: &str) -> String {
    name.trim()
        .to_lowercase()
        .chars()
        .map(|c| match c {
            'á' | 'à' | 'â' | 'ã' => 'a',
            'é' | 'ê' => 'e',
            'í' => 'i',
            'ó' | 'ô' | 'õ' => 'o',
            'ú' => 'u',
            'ç' => 'c',
            c => c,
        })
        .collect()
}

fn map_columns(header: &[String]) -> Result<Columns> {
    let mut cols = Columns::default();
    for (i, h) in header.iter().enumerate() {
        match fold(h).as_str() {
            "data" | "date" | "data lancamento" => cols.date = Some(i),
            "descricao" | "historico" | "description" | "memo" => cols.description = Some(i),
            "valor" | "amount" | "valor (r$)" => cols.amount = Some(i),
            "moeda" | "currency" => cols.currency = Some(i),
            "categoria" | "category" => cols.category = Some(i),
            _ => {}
        }
    }
    let missing: Vec<&str> = [("data", cols.date), ("descricao", cols.description), ("valor", cols.amount)]
        .iter()
        .filter(|(_, c)| c.is_none())
        .map(|(n, _)| *n)
        .collect();
    if !missing.is_empty() {
        return Err(Error::Parse { line: 1, message: format!("colunas ausentes: {}", missing.join(", ")) });
    }
    Ok(cols)
}

fn field<'a>(row: &'a [String], idx: Option<usize>, line: usize, name: &str) -> Result<&'a str> {
    let i = idx.expect("coluna obrigatória mapeada");
    row.get(i)
        .map(|s| s.as_str())
        .ok_or_else(|| Error::Parse { line, message: format!("campo {name} ausente") })
}

/// Converte "1.234,56" ou "1,234.56" para o formato aceito por [`Money::parse`].
pub fn normalize_amount(raw: &str) -> String {
    let t: String = raw.trim().chars().filter(|c| !c.is_whitespace()).collect();
    let t = t.trim_start_matches("R$").to_string();
    let last_dot = t.rfind('.');
    let last_comma = t.rfind(',');
    match (last_dot, last_comma) {
        (Some(d), Some(c)) if c > d => t.replace('.', "").replace(',', "."),
        (Some(_), Some(_)) => t.replace(',', ""),
        (None, Some(c)) if t.matches(',').count() == 1 && t.len() - c - 1 != 3 => t.replace(',', "."),
        (None, Some(_)) => t.replace(',', ""),
        _ => t,
    }
}

pub fn parse_statement(text: &str, opts: &StatementOptions) -> Result<Vec<Txn>> {
    let mut lines = text
        .lines()
        .enumerate()
        .map(|(i, l)| (i + 1, l.trim_end_matches('\r')))
        .filter(|(_, l)| !l.trim().is_empty() && !l.trim_start().starts_with('#'));
    let (_, header_line) = lines.next().ok_or(Error::Parse { line: 1, message: "extrato vazio".into() })?;
    let header_line = header_line.trim_start_matches('\u{feff}');
    let sep = opts.separator.unwrap_or_else(|| detect_separator(header_line));
    let header = split_record(header_line, sep, 1)?;
    let cols = map_columns(&header)?;
    let mut out = Vec::new();
    for (line, raw) in lines {
        let row = split_record(raw, sep, line)?;
        let date_s = field(&row, cols.date, line, "data")?;
        let date = Date::parse_any(date_s).map_err(|e| Error::Parse { line, message: e.to_string() })?;
        let desc = field(&row, cols.description, line, "descricao")?.trim().to_string();
        let cur = match cols.currency.and_then(|i| row.get(i)).map(|s| s.trim()).filter(|s| !s.is_empty()) {
            Some(code) => currency::require(code).map_err(|e| Error::Parse { line, message: e.to_string() })?,
            None => opts.default_currency,
        };
        let amount_s = normalize_amount(field(&row, cols.amount, line, "valor")?);
        let mut amount = Money::parse(&amount_s, cur, opts.rounding).map_err(|e| Error::Parse { line, message: e.to_string() })?;
        if opts.negate {
            amount = amount.neg();
        }
        let mut txn = Txn::new(date, opts.account.clone(), desc, amount);
        if let Some(cat) = cols.category.and_then(|i| row.get(i)).map(|s| s.trim()).filter(|s| !s.is_empty()) {
            txn = txn.with_category(cat);
        }
        out.push(txn);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn opts() -> StatementOptions {
        StatementOptions::new(AccountId::parse("ativo:banco:corrente").unwrap())
    }

    #[test]
    fn normalizes_amounts() {
        assert_eq!(normalize_amount("1.234,56"), "1234.56");
        assert_eq!(normalize_amount("1,234.56"), "1234.56");
        assert_eq!(normalize_amount("-12,5"), "-12.5");
        assert_eq!(normalize_amount("R$ 10,00"), "10.00");
        assert_eq!(normalize_amount("1,234"), "1234");
        assert_eq!(normalize_amount("99.90"), "99.90");
    }

    #[test]
    fn parses_basic_statement() {
        let text = "Data;Histórico;Valor\n05/03/2024;PADARIA BOM PAO;-12,50\n# comentário\n\n2024-03-06;SALARIO;5.000,00\n";
        let txns = parse_statement(text, &opts()).unwrap();
        assert_eq!(txns.len(), 2);
        assert_eq!(txns[0].date, Date::ymd(2024, 3, 5));
        assert_eq!(txns[0].amount.minor(), -1250);
        assert_eq!(txns[1].description, "SALARIO");
        assert_eq!(txns[1].amount.minor(), 500000);
    }

    #[test]
    fn optional_columns() {
        let text = "date,description,amount,currency,category\n2024-01-02,Coffee,\"3,50\",USD,lazer\n2024-01-03,Tea,2.00,,\n";
        let txns = parse_statement(text, &opts()).unwrap();
        assert_eq!(txns[0].amount.currency().code, "USD");
        assert_eq!(txns[0].amount.minor(), 350);
        assert_eq!(txns[0].category.as_deref(), Some("lazer"));
        assert_eq!(txns[1].amount.currency().code, "BRL");
        assert_eq!(txns[1].category, None);
    }

    #[test]
    fn negate_and_errors() {
        let mut o = opts();
        o.negate = true;
        let txns = parse_statement("data;descricao;valor\n2024-01-02;X;10\n", &o).unwrap();
        assert_eq!(txns[0].amount.minor(), -1000);
        assert!(parse_statement("data;valor\n2024-01-02;10\n", &o).is_err());
        let e = parse_statement("data;descricao;valor\n2024-02-30;X;10\n", &o).unwrap_err();
        assert!(matches!(e, Error::Parse { line: 2, .. }));
        assert!(parse_statement("", &o).is_err());
    }
}
