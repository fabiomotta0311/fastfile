//! Leitura de extratos bancários e da configuração.

pub mod config;
pub mod csv;
pub mod statement;

pub use config::Config;
pub use statement::{parse_statement, StatementOptions};
