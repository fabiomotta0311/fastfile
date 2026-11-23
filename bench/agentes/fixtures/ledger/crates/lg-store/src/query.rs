//! Consultas sobre o [`Store`].
//!
//! Todos os filtros são opcionais e combinados com E. O intervalo de datas é fechado nas
//! duas pontas: `from = 2024-03-01, to = 2024-03-31` traz o mês inteiro, inclusive o dia 31.
//! O índice usado como ponto de partida é escolhido pelo filtro mais seletivo disponível;
//! o resultado é o mesmo qualquer que seja o índice.

use lg_core::{AccountId, Date, Money, Txn};

use crate::store::{Store, TxnId};

#[derive(Debug, Clone, Default)]
pub struct Query {
    /// Conta ou qualquer descendente dela.
    pub account: Option<AccountId>,
    pub category: Option<String>,
    pub from: Option<Date>,
    pub to: Option<Date>,
    /// Trecho da descrição, sem diferenciar maiúsculas.
    pub text: Option<String>,
    /// Limites de valor, comparados em valor absoluto.
    pub min_abs: Option<Money>,
    pub max_abs: Option<Money>,
    pub limit: Option<usize>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Start {
    Category,
    Account,
    Months,
    Scan,
}

impl Query {
    pub fn new() -> Query {
        Query::default()
    }

    pub fn account(mut self, a: AccountId) -> Self {
        self.account = Some(a);
        self
    }

    pub fn category(mut self, c: &str) -> Self {
        self.category = Some(c.to_string());
        self
    }

    pub fn between(mut self, from: Date, to: Date) -> Self {
        self.from = Some(from);
        self.to = Some(to);
        self
    }

    pub fn text(mut self, t: &str) -> Self {
        self.text = Some(t.to_string());
        self
    }

    pub fn limit(mut self, n: usize) -> Self {
        self.limit = Some(n);
        self
    }

    fn start(&self) -> Start {
        if self.category.is_some() {
            Start::Category
        } else if self.account.is_some() {
            Start::Account
        } else if self.from.is_some() || self.to.is_some() {
            Start::Months
        } else {
            Start::Scan
        }
    }

    fn in_range(&self, d: Date) -> bool {
        if let Some(from) = self.from {
            if d < from {
                return false;
            }
        }
        if let Some(to) = self.to {
            if d > to {
                return false;
            }
        }
        true
    }

    fn matches(&self, t: &Txn) -> bool {
        if let Some(a) = &self.account {
            if !t.account.is_self_or_descendant_of(a) {
                return false;
            }
        }
        if let Some(c) = &self.category {
            if t.category.as_deref() != Some(c.as_str()) {
                return false;
            }
        }
        if !self.in_range(t.date) {
            return false;
        }
        if let Some(text) = &self.text {
            if !t.description.to_lowercase().contains(&text.to_lowercase()) {
                return false;
            }
        }
        let abs = t.amount.minor().abs();
        if let Some(m) = &self.min_abs {
            if abs < m.minor().abs() {
                return false;
            }
        }
        if let Some(m) = &self.max_abs {
            if abs > m.minor().abs() {
                return false;
            }
        }
        true
    }

    fn candidates(&self, store: &Store) -> Vec<usize> {
        match self.start() {
            Start::Category => store.category_index().get(self.category.as_ref().expect("categoria")).to_vec(),
            Start::Account => {
                let a = self.account.as_ref().expect("conta");
                let idx = store.account_index();
                idx.keys()
                    .filter(|k| k.is_self_or_descendant_of(a))
                    .flat_map(|k| idx.get(k).iter().copied())
                    .collect()
            }
            Start::Months => {
                let lo = self.from.map(|d| d.month_key()).unwrap_or((i32::MIN, 0));
                let hi = self.to.map(|d| d.month_key()).unwrap_or((i32::MAX, u8::MAX));
                store.month_index().range(lo..=hi)
            }
            Start::Scan => store.iter().map(|(id, _)| id.0).collect(),
        }
    }

    /// Resultado em ordem de data e, no mesmo dia, de inserção.
    pub fn run<'s>(&self, store: &'s Store) -> Vec<(TxnId, &'s Txn)> {
        let mut out: Vec<(TxnId, &Txn)> = self
            .candidates(store)
            .into_iter()
            .map(|id| (TxnId(id), store.indexed(id)))
            .filter(|(_, t)| self.matches(t))
            .collect();
        out.sort_by(|a, b| a.1.date.cmp(&b.1.date).then(a.0.cmp(&b.0)));
        if let Some(n) = self.limit {
            out.truncate(n);
        }
        out
    }

    pub fn total(&self, store: &Store) -> Option<Money> {
        let rows = self.run(store);
        let first = rows.first()?.1.amount.currency();
        let amounts: Vec<Money> = rows.iter().map(|(_, t)| t.amount).filter(|m| m.currency() == first).collect();
        Money::sum(first, &amounts).ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn acc(s: &str) -> AccountId {
        AccountId::parse(s).unwrap()
    }

    fn store() -> Store {
        let mut s = Store::new();
        let rows = [
            ("2024-02-28", "ativo:banco:corrente", "PADARIA", -800, Some("alimentacao")),
            ("2024-03-01", "ativo:banco:corrente", "MERCADO X", -15000, Some("mercado")),
            ("2024-03-15", "ativo:banco:poupanca", "RENDIMENTO", 1234, None),
            ("2024-03-20", "ativo:cartao", "MERCADO Y", -9900, Some("mercado")),
            ("2024-04-02", "ativo:banco:corrente", "SALARIO", 500000, Some("salario")),
        ];
        for (d, a, desc, v, c) in rows {
            let mut t = Txn::new(Date::parse_iso(d).unwrap(), acc(a), desc, Money::of(v, "BRL"));
            if let Some(c) = c {
                t = t.with_category(c);
            }
            s.insert(t);
        }
        s
    }

    fn descs(rows: &[(TxnId, &Txn)]) -> Vec<String> {
        rows.iter().map(|(_, t)| t.description.clone()).collect()
    }

    #[test]
    fn by_category() {
        let s = store();
        assert_eq!(descs(&Query::new().category("mercado").run(&s)), vec!["MERCADO X", "MERCADO Y"]);
    }

    #[test]
    fn by_account_includes_descendants() {
        let s = store();
        let r = Query::new().account(acc("ativo:banco")).run(&s);
        assert_eq!(descs(&r), vec!["PADARIA", "MERCADO X", "RENDIMENTO", "SALARIO"]);
    }

    #[test]
    fn date_range_starts_inclusive() {
        let s = store();
        let r = Query::new().between(Date::ymd(2024, 3, 1), Date::ymd(2024, 3, 25)).run(&s);
        assert_eq!(descs(&r), vec!["MERCADO X", "RENDIMENTO", "MERCADO Y"]);
    }

    #[test]
    fn text_amount_and_limit() {
        let s = store();
        assert_eq!(descs(&Query::new().text("mercado").limit(1).run(&s)), vec!["MERCADO X"]);
        let mut q = Query::new();
        q.min_abs = Some(Money::of(1000, "BRL"));
        q.max_abs = Some(Money::of(20000, "BRL"));
        assert_eq!(descs(&q.run(&s)), vec!["MERCADO X", "RENDIMENTO", "MERCADO Y"]);
    }

    #[test]
    fn total_of_matches() {
        let s = store();
        assert_eq!(Query::new().category("mercado").total(&s).unwrap().minor(), -24900);
        assert!(Query::new().category("nada").total(&s).is_none());
    }
}
