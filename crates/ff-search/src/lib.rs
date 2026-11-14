//! `ff-search`
//!
//! Consulta por nome sobre o índice (substring, curingas, regex,
//! filtros de tamanho, data e extensão) com varredura paralela via Rayon e memchr.

pub mod bench;
pub mod engine;
pub mod query;

pub use bench::{benchmark_search, BenchmarkStats};
pub use engine::{SearchEngine, SearchMatch, SearchResult};
pub use query::{Filters, Pattern, Query};

#[must_use]
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version() {
        assert_eq!(version(), "0.1.0");
    }
}
