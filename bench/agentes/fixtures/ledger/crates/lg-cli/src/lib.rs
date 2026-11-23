//! Biblioteca da CLI `ledger`: argumentos, configuração efetiva e comandos.
//!
//! `main.rs` só lê arquivos e repassa para [`commands::run`]; todo o resto é testável
//! sem processo filho.

pub mod args;
pub mod commands;
pub mod settings;

pub use args::{parse_args, Command, Flags};
pub use settings::Settings;
