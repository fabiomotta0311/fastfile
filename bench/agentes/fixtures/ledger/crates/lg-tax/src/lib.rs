//! Impostos sobre lançamentos.
//!
//! Alíquotas são guardadas em partes por milhão ([`Rate`]) para representar valores como
//! 0,65% sem ponto flutuante. O arredondamento de cada região vem da tabela em
//! `regions.rs`; o cálculo em si delega a `lg_core::rounding`.

pub mod calc;
pub mod rate;
pub mod regions;

pub use calc::{tax_for, Breakdown, Line, RoundingScope};
pub use rate::Rate;
pub use regions::{region, Region};
