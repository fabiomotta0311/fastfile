//! Moedas suportadas. A tabela completa fica em `currency_table.rs` (gerada a partir da
//! lista ISO 4217; não editar à mão sem atualizar `docs/moedas.md`).

use crate::currency_table::CURRENCIES;
use crate::error::{Error, Result};

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct Currency {
    /// Código ISO 4217 em maiúsculas.
    pub code: &'static str,
    /// Casas decimais da unidade menor (2 para BRL, 0 para moedas sem centavos).
    pub minor_units: u8,
    /// Símbolo usado na apresentação. Vazio quando não há símbolo próprio.
    pub symbol: &'static str,
    pub name: &'static str,
}

impl Currency {
    /// Fator entre a unidade maior e a menor (100 para 2 casas).
    pub fn scale(&self) -> i64 {
        10_i64.pow(self.minor_units as u32)
    }

    /// Símbolo ou, na falta dele, o próprio código.
    pub fn display_symbol(&self) -> &'static str {
        if self.symbol.is_empty() {
            self.code
        } else {
            self.symbol
        }
    }
}

/// Procura por código, sem diferenciar maiúsculas de minúsculas.
pub fn lookup(code: &str) -> Option<&'static Currency> {
    let code = code.trim();
    CURRENCIES.iter().find(|c| c.code.eq_ignore_ascii_case(code))
}

pub fn require(code: &str) -> Result<&'static Currency> {
    lookup(code).ok_or_else(|| Error::UnknownCurrency(code.to_string()))
}

/// Todas as moedas, na ordem da tabela.
pub fn all() -> &'static [Currency] {
    CURRENCIES
}

pub fn brl() -> &'static Currency {
    lookup("BRL").expect("BRL na tabela")
}

pub fn usd() -> &'static Currency {
    lookup("USD").expect("USD na tabela")
}

pub fn eur() -> &'static Currency {
    lookup("EUR").expect("EUR na tabela")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lookup_is_case_insensitive() {
        assert_eq!(lookup("brl").unwrap().code, "BRL");
        assert_eq!(lookup(" usd ").unwrap().code, "USD");
        assert!(lookup("XXX1").is_none());
    }

    #[test]
    fn common_scales() {
        assert_eq!(brl().scale(), 100);
        assert_eq!(usd().scale(), 100);
        assert_eq!(lookup("KWD").unwrap().scale(), 1000);
    }

    #[test]
    fn codes_are_unique_and_uppercase() {
        let mut seen = std::collections::HashSet::new();
        for c in all() {
            assert!(seen.insert(c.code), "duplicada: {}", c.code);
            assert_eq!(c.code.len(), 3);
            assert!(c.code.chars().all(|ch| ch.is_ascii_uppercase()));
            assert!(c.minor_units <= 4);
        }
    }

    #[test]
    fn display_symbol_falls_back_to_code() {
        assert_eq!(brl().display_symbol(), "R$");
        assert_eq!(lookup("CHF").unwrap().display_symbol(), "CHF");
    }
}
