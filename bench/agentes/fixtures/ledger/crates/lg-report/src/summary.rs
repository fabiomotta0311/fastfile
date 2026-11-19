//! Totais por categoria e por conta (com consolidação nos ancestrais).

use std::collections::BTreeMap;

use lg_core::{AccountId, Currency, Money, Result, Txn};

pub const UNCATEGORIZED: &str = "(sem categoria)";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CategoryTotals {
    pub currency: &'static Currency,
    pub income: BTreeMap<String, Money>,
    pub expense: BTreeMap<String, Money>,
}

impl CategoryTotals {
    /// Separa receitas (valores positivos) de despesas (negativos) por categoria.
    /// Transações em outra moeda são ignoradas e contadas em `skipped`.
    pub fn compute(txns: &[&Txn], currency: &'static Currency) -> Result<(CategoryTotals, usize)> {
        let mut income: BTreeMap<String, Money> = BTreeMap::new();
        let mut expense: BTreeMap<String, Money> = BTreeMap::new();
        let mut skipped = 0;
        for t in txns {
            if t.amount.currency() != currency {
                skipped += 1;
                continue;
            }
            let cat = t.category.clone().unwrap_or_else(|| UNCATEGORIZED.to_string());
            let bucket = if t.amount.is_negative() { &mut expense } else { &mut income };
            let cur = bucket.entry(cat).or_insert_with(|| Money::zero(currency));
            *cur = cur.checked_add(&t.amount)?;
        }
        Ok((CategoryTotals { currency, income, expense }, skipped))
    }

    pub fn total_income(&self) -> Result<Money> {
        Money::sum(self.currency, self.income.values())
    }

    pub fn total_expense(&self) -> Result<Money> {
        Money::sum(self.currency, self.expense.values())
    }

    pub fn net(&self) -> Result<Money> {
        self.total_income()?.checked_add(&self.total_expense()?)
    }

    /// Despesas da maior para a menor (em valor absoluto); empate por nome.
    pub fn top_expenses(&self, n: usize) -> Vec<(&str, Money)> {
        let mut v: Vec<(&str, Money)> = self.expense.iter().map(|(k, m)| (k.as_str(), *m)).collect();
        v.sort_by(|a, b| a.1.minor().cmp(&b.1.minor()).then(a.0.cmp(b.0)));
        v.truncate(n);
        v
    }
}

/// Saldo por conta, consolidado: cada conta soma o próprio saldo e o de todas as
/// descendentes. Contas ancestrais sem lançamento direto também aparecem.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Summary {
    pub currency: &'static Currency,
    pub balances: BTreeMap<AccountId, Money>,
}

impl Summary {
    pub fn compute(txns: &[&Txn], currency: &'static Currency, max_depth: Option<usize>) -> Result<Summary> {
        let mut balances: BTreeMap<AccountId, Money> = BTreeMap::new();
        for t in txns {
            if t.amount.currency() != currency {
                continue;
            }
            let leaf = match max_depth {
                Some(d) => t.account.truncate(d),
                None => t.account.clone(),
            };
            let mut chain = leaf.ancestors();
            chain.push(leaf);
            for a in chain {
                let cur = balances.entry(a).or_insert_with(|| Money::zero(currency));
                *cur = cur.checked_add(&t.amount)?;
            }
        }
        Ok(Summary { currency, balances })
    }

    pub fn balance(&self, account: &AccountId) -> Money {
        self.balances.get(account).copied().unwrap_or_else(|| Money::zero(self.currency))
    }

    /// Linhas para exibição: (indentação, nome do último segmento, saldo).
    pub fn rows(&self) -> Vec<(usize, String, Money)> {
        self.balances
            .iter()
            .map(|(a, m)| (a.depth() - 1, a.segments().last().unwrap_or_default().to_string(), *m))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lg_core::{currency, Date};

    fn t(acc: &str, v: i64, cat: Option<&str>) -> Txn {
        let x = Txn::new(Date::ymd(2024, 5, 1), AccountId::parse(acc).unwrap(), "x", Money::of(v, "BRL"));
        match cat {
            Some(c) => x.with_category(c),
            None => x,
        }
    }

    #[test]
    fn category_totals_split_income_and_expense() {
        let txns = [
            t("ativo:banco", -1000, Some("mercado")),
            t("ativo:banco", -500, Some("mercado")),
            t("ativo:banco", 300000, Some("salario")),
            t("ativo:banco", -200, None),
            Txn::new(Date::ymd(2024, 5, 1), AccountId::parse("ativo:banco").unwrap(), "y", Money::of(-1, "USD")),
        ];
        let refs: Vec<&Txn> = txns.iter().collect();
        let (c, skipped) = CategoryTotals::compute(&refs, currency::brl()).unwrap();
        assert_eq!(skipped, 1);
        assert_eq!(c.expense["mercado"].minor(), -1500);
        assert_eq!(c.expense[UNCATEGORIZED].minor(), -200);
        assert_eq!(c.net().unwrap().minor(), 298300);
        assert_eq!(c.top_expenses(1), vec![("mercado", Money::of(-1500, "BRL"))]);
    }

    #[test]
    fn summary_rolls_up() {
        let txns = [t("ativo:banco:corrente", 1000, None), t("ativo:banco:poupanca", 500, None), t("ativo:carteira", 20, None)];
        let refs: Vec<&Txn> = txns.iter().collect();
        let s = Summary::compute(&refs, currency::brl(), None).unwrap();
        assert_eq!(s.balance(&AccountId::parse("ativo").unwrap()).minor(), 1520);
        assert_eq!(s.balance(&AccountId::parse("ativo:banco").unwrap()).minor(), 1500);
        assert_eq!(s.rows()[0], (0, "ativo".to_string(), Money::of(1520, "BRL")));
        let shallow = Summary::compute(&refs, currency::brl(), Some(2)).unwrap();
        assert!(!shallow.balances.contains_key(&AccountId::parse("ativo:banco:corrente").unwrap()));
    }
}
