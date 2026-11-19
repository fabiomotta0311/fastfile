//! Categorização de transações.
//!
//! Uma [`Rule`] associa um [`Pattern`] sobre a descrição a uma categoria. O [`Engine`]
//! escolhe a regra vencedora quando várias casam; ver a ordem em `engine.rs` e em
//! `docs/regras.md`.

pub mod builtin;
pub mod engine;
pub mod pattern;
pub mod rulefile;

pub use engine::{Engine, Rule};
pub use pattern::Pattern;
