//! Tipos centrais do razão.
//!
//! Tudo que representa valor monetário passa por [`Money`], que guarda unidades
//! menores (centavos, por exemplo) como inteiro. Nenhum cálculo usa ponto flutuante.

pub mod account;
pub mod currency;
mod currency_table;
pub mod date;
pub mod error;
pub mod money;
pub mod rounding;

pub use account::AccountId;
pub use currency::Currency;
pub use date::Date;
pub use error::{Error, Result};
pub use money::Money;
pub use rounding::RoundingMode;

/// Transação já normalizada, pronta para o armazenamento e os relatórios.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Txn {
    pub date: Date,
    pub account: AccountId,
    pub description: String,
    pub amount: Money,
    pub category: Option<String>,
}

impl Txn {
    pub fn new(date: Date, account: AccountId, description: impl Into<String>, amount: Money) -> Self {
        Txn { date, account, description: description.into(), amount, category: None }
    }

    pub fn with_category(mut self, category: impl Into<String>) -> Self {
        self.category = Some(category.into());
        self
    }
}
