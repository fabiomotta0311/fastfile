//! Armazenamento em memória.
//!
//! As transações ficam num vetor de slots (`Option<Txn>`); remover deixa o slot vazio
//! para que os identificadores ([`TxnId`]) continuem estáveis. Os índices por conta,
//! categoria e mês apontam para slots e precisam acompanhar inserções e remoções.

pub mod index;
pub mod query;
pub mod store;

pub use query::Query;
pub use store::{Store, TxnId};
