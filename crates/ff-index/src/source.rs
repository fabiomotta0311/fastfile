//! Traits e tipos para fontes do índice e atualizações ao vivo.

use std::fmt;

use crate::index::FileIndex;

/// Erros decorrentes de manipulação ou consulta do índice.
#[derive(Debug, PartialEq, Eq, Clone)]
pub enum IndexError {
    /// O identificador de entrada (ID) é inválido ou fora dos limites.
    InvalidId(u32),
    /// O identificador NTFS (FRN) não foi encontrado no mapa.
    FrnNotFound(u64),
    /// O nó pai informado não existe.
    ParentNotFound(u32),
    /// Ciclo detectado ao reconstruir caminho.
    CycleDetected(u32),
    /// Operação inválida.
    Other(String),
}

impl fmt::Display for IndexError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidId(id) => write!(f, "entrada com ID {id} não encontrada no índice"),
            Self::FrnNotFound(frn) => write!(f, "FRN {frn} não encontrado no índice"),
            Self::ParentNotFound(p) => write!(f, "nó pai {p} não encontrado no índice"),
            Self::CycleDetected(id) => {
                write!(f, "ciclo detectado na hierarquia a partir do ID {id}")
            }
            Self::Other(msg) => write!(f, "{msg}"),
        }
    }
}

impl std::error::Error for IndexError {}

/// Mudança individual no sistema de arquivos para atualização ao vivo do índice (ex.: USN Journal).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IndexChange {
    /// Inserção de uma nova entrada.
    Insert {
        frn: u64,
        parent_frn: u64,
        name: String,
        flags: u16,
        size: u64,
        mtime: i64,
    },
    /// Remoção de uma entrada existente.
    Delete { frn: u64 },
    /// Renomeação ou movimentação de pasta de uma entrada.
    Rename {
        frn: u64,
        new_parent_frn: u64,
        new_name: String,
    },
    /// Atualização de metadados (tamanho, data, flags).
    Modify {
        frn: u64,
        size: u64,
        mtime: i64,
        flags: u16,
    },
}

/// Abstração da fonte de dados para o índice (MFT, varredura ou sintética).
pub trait IndexSource {
    type Error: std::error::Error + Send + Sync + 'static;

    /// Popula o `FileIndex` a partir desta fonte.
    fn populate(&mut self, index: &mut FileIndex) -> Result<(), Self::Error>;
}
