//! Repositório de transações com índices por conta, categoria e mês.

use lg_core::{AccountId, Txn};

use crate::index::Index;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TxnId(pub usize);

#[derive(Debug, Default, Clone)]
pub struct Store {
    slots: Vec<Option<Txn>>,
    live: usize,
    by_account: Index<AccountId>,
    by_category: Index<String>,
    by_month: Index<(i32, u8)>,
}

/// Categoria usada no índice para transações sem categoria.
pub const UNCATEGORIZED: &str = "(sem categoria)";

fn category_key(t: &Txn) -> String {
    t.category.clone().unwrap_or_else(|| UNCATEGORIZED.to_string())
}

impl Store {
    pub fn new() -> Store {
        Store::default()
    }

    pub fn insert(&mut self, txn: Txn) -> TxnId {
        let id = self.slots.len();
        self.by_account.insert(txn.account.clone(), id);
        self.by_category.insert(category_key(&txn), id);
        self.by_month.insert(txn.date.month_key(), id);
        self.slots.push(Some(txn));
        self.live += 1;
        TxnId(id)
    }

    pub fn extend(&mut self, txns: impl IntoIterator<Item = Txn>) -> Vec<TxnId> {
        txns.into_iter().map(|t| self.insert(t)).collect()
    }

    pub fn get(&self, id: TxnId) -> Option<&Txn> {
        self.slots.get(id.0).and_then(|s| s.as_ref())
    }

    /// Remove a transação e a retira de todos os índices.
    pub fn remove(&mut self, id: TxnId) -> Option<Txn> {
        let txn = self.slots.get_mut(id.0)?.take()?;
        self.by_account.remove(&txn.account, id.0);
        self.by_category.remove(&category_key(&txn), id.0);
        self.by_month.remove(&txn.date.month_key(), id.0);
        self.live -= 1;
        Some(txn)
    }

    /// Troca a categoria mantendo o índice coerente.
    pub fn set_category(&mut self, id: TxnId, category: Option<String>) -> bool {
        let Some(Some(txn)) = self.slots.get_mut(id.0) else {
            return false;
        };
        let old = category_key(txn);
        txn.category = category;
        let new = category_key(txn);
        if old != new {
            self.by_category.remove(&old, id.0);
            self.by_category.insert(new, id.0);
        }
        true
    }

    pub fn len(&self) -> usize {
        self.live
    }

    pub fn is_empty(&self) -> bool {
        self.live == 0
    }

    pub fn iter(&self) -> impl Iterator<Item = (TxnId, &Txn)> {
        self.slots
            .iter()
            .enumerate()
            .filter_map(|(i, s)| s.as_ref().map(|t| (TxnId(i), t)))
    }

    /// Resolve um id vindo de um índice. Um id indexado sempre aponta para um slot ocupado;
    /// o contrário é corrupção de índice e deve aparecer cedo.
    pub(crate) fn indexed(&self, id: usize) -> &Txn {
        self.slots[id].as_ref().expect("índice aponta para transação removida")
    }

    pub(crate) fn account_index(&self) -> &Index<AccountId> {
        &self.by_account
    }

    pub(crate) fn category_index(&self) -> &Index<String> {
        &self.by_category
    }

    pub(crate) fn month_index(&self) -> &Index<(i32, u8)> {
        &self.by_month
    }

    pub fn categories(&self) -> Vec<&str> {
        self.by_category.keys().map(|k| k.as_str()).collect()
    }

    pub fn accounts(&self) -> Vec<&AccountId> {
        self.by_account.keys().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lg_core::{Date, Money};

    fn txn(day: u8, cat: Option<&str>) -> Txn {
        let t = Txn::new(Date::ymd(2024, 3, day), AccountId::parse("ativo:banco").unwrap(), "X", Money::of(100, "BRL"));
        match cat {
            Some(c) => t.with_category(c),
            None => t,
        }
    }

    #[test]
    fn insert_get_len() {
        let mut s = Store::new();
        let a = s.insert(txn(1, Some("mercado")));
        let b = s.insert(txn(2, None));
        assert_eq!(s.len(), 2);
        assert_eq!(s.get(a).unwrap().date.day(), 1);
        assert_eq!(s.categories(), vec!["(sem categoria)", "mercado"]);
        assert_eq!(s.iter().map(|(id, _)| id).collect::<Vec<_>>(), vec![a, b]);
    }

    #[test]
    fn remove_keeps_ids_stable() {
        let mut s = Store::new();
        let a = s.insert(txn(1, None));
        let b = s.insert(txn(2, None));
        assert!(s.remove(a).is_some());
        assert!(s.remove(a).is_none());
        assert_eq!(s.len(), 1);
        assert_eq!(s.get(b).unwrap().date.day(), 2);
        assert!(s.get(a).is_none());
    }

    #[test]
    fn set_category_reindexes() {
        let mut s = Store::new();
        let a = s.insert(txn(1, None));
        assert!(s.set_category(a, Some("lazer".into())));
        assert_eq!(s.categories(), vec!["lazer"]);
        assert!(!s.set_category(TxnId(99), None));
    }
}
