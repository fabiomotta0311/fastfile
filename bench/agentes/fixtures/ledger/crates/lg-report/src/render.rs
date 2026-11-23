//! Renderização dos relatórios em texto.

use std::collections::BTreeMap;

use lg_core::{Money, Result, Txn};
use lg_fmt::{format_amount, Align, Locale, Table};
use lg_tax::{tax_for, Rate};

use crate::group::Period;
use crate::summary::{CategoryTotals, Summary};

/// Lista de lançamentos: data, descrição, categoria e valor (alinhado à direita).
pub fn transactions_table(txns: &[&Txn], locale: &Locale) -> String {
    let mut t = Table::new(&["data", "descricao", "categoria", "valor"]).align(3, Align::Right);
    t.max_width = Some(40);
    for x in txns {
        let cat = x.category.as_deref().unwrap_or("-");
        t.push_row(&[x.date.to_string(), x.description.clone(), cat.to_string(), format_amount(x.amount, locale)]);
    }
    t.render()
}

/// Receitas, despesas e saldo por categoria.
pub fn category_report(c: &CategoryTotals, locale: &Locale) -> Result<String> {
    let mut t = Table::new(&["categoria", "receitas", "despesas"]).align(1, Align::Right).align(2, Align::Right);
    let mut cats: Vec<&String> = c.income.keys().chain(c.expense.keys()).collect();
    cats.sort();
    cats.dedup();
    let zero = Money::zero(c.currency);
    for cat in cats {
        let inc = c.income.get(cat).copied().unwrap_or(zero);
        let exp = c.expense.get(cat).copied().unwrap_or(zero);
        t.push_row(&[cat.clone(), format_amount(inc, locale), format_amount(exp, locale)]);
    }
    t.push_row(&[
        "TOTAL".to_string(),
        format_amount(c.total_income()?, locale),
        format_amount(c.total_expense()?, locale),
    ]);
    Ok(t.render())
}

/// Saldos por conta, indentados pela hierarquia.
pub fn balance_report(s: &Summary, locale: &Locale) -> String {
    let mut t = Table::new(&["conta", "saldo"]).align(1, Align::Right);
    for (indent, name, m) in s.rows() {
        t.push_row(&[format!("{}{}", "  ".repeat(indent), name), format_amount(m, locale)]);
    }
    t.render()
}

/// Total por período, com o imposto estimado sobre as despesas à alíquota dada.
pub fn period_report(groups: &BTreeMap<Period, Vec<&Txn>>, rate: Rate, locale: &Locale) -> Result<String> {
    let mut t = Table::new(&["periodo", "lancamentos", "total", "imposto"])
        .align(1, Align::Right)
        .align(2, Align::Right)
        .align(3, Align::Right);
    for (p, txns) in groups {
        let Some(first) = txns.first() else { continue };
        let cur = first.amount.currency();
        let same: Vec<Money> = txns.iter().map(|x| x.amount).filter(|m| m.currency() == cur).collect();
        let total = Money::sum(cur, &same)?;
        let mut tax = Money::zero(cur);
        for m in same.iter().filter(|m| m.is_negative()) {
            tax = tax.checked_add(&tax_for(*m, rate, lg_core::RoundingMode::HalfEven)?)?;
        }
        t.push_row(&[p.to_string(), txns.len().to_string(), format_amount(total, locale), format_amount(tax, locale)]);
    }
    Ok(t.render())
}

#[cfg(test)]
mod tests {
    use super::*;
    use lg_core::{currency, AccountId, Date};

    fn txn(d: Date, desc: &str, v: i64, cat: Option<&str>) -> Txn {
        let t = Txn::new(d, AccountId::parse("ativo:banco").unwrap(), desc, Money::of(v, "BRL"));
        match cat {
            Some(c) => t.with_category(c),
            None => t,
        }
    }

    #[test]
    fn transactions_table_ascii() {
        let a = txn(Date::ymd(2024, 3, 5), "PADARIA", -1250, Some("alimentacao"));
        let b = txn(Date::ymd(2024, 3, 6), "SALARIO", 500000, None);
        let out = transactions_table(&[&a, &b], &Locale::by_tag("pt-BR").unwrap());
        let lines: Vec<&str> = out.lines().collect();
        assert_eq!(lines[0], "data        descricao  categoria          valor");
        assert_eq!(lines[2], "2024-03-05  PADARIA    alimentacao    -R$ 12,50");
        assert_eq!(lines[3], "2024-03-06  SALARIO    -            R$ 5.000,00");
    }

    #[test]
    fn category_report_has_total() {
        let a = txn(Date::ymd(2024, 3, 5), "X", -1250, Some("mercado"));
        let b = txn(Date::ymd(2024, 3, 6), "Y", 10000, Some("salario"));
        let (c, _) = CategoryTotals::compute(&[&a, &b], currency::brl()).unwrap();
        let out = category_report(&c, &Locale::by_tag("en-US").unwrap()).unwrap();
        assert!(out.lines().last().unwrap().starts_with("TOTAL"));
        assert!(out.contains("-R$12.50"));
    }

    #[test]
    fn period_report_estimates_tax_on_expenses() {
        let a = txn(Date::ymd(2024, 3, 5), "X", -10000, None);
        let b = txn(Date::ymd(2024, 3, 6), "Y", 50000, None);
        let mut g = BTreeMap::new();
        g.insert(Period::Month { year: 2024, month: 3 }, vec![&a, &b]);
        let out = period_report(&g, Rate::from_ppm(100_000), &Locale::by_tag("pt-BR").unwrap()).unwrap();
        assert!(out.contains("2024-03"));
        assert!(out.contains("-R$ 10,00"), "{out}");
    }
}
