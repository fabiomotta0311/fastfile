//! Representação compacta de uma entrada do sistema de arquivos no índice.

pub const NO_PARENT: u32 = u32::MAX;

pub const FLAG_FILE: u16 = 0;
pub const FLAG_DIRECTORY: u16 = 1 << 0;
pub const FLAG_HIDDEN: u16 = 1 << 1;
pub const FLAG_SYSTEM: u16 = 1 << 2;
pub const FLAG_READONLY: u16 = 1 << 3;
pub const FLAG_ARCHIVE: u16 = 1 << 4;
pub const FLAG_DELETED: u16 = 1 << 15;

/// Entrada do índice (Struct-of-Arrays com arena de nomes, §2.2).
///
/// Tamanho fixo de 32 bytes para máxima densidade de cache.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(C)]
pub struct Entry {
    /// Índice do nó pai no vetor de entradas (`NO_PARENT` para a raiz).
    pub parent: u32,
    /// Offset do nome (em bytes) na arena `names`.
    pub name_off: u32,
    /// Tamanho do arquivo em bytes (0 para diretórios).
    pub size: u64,
    /// Data de modificação em segundos desde Unix Epoch.
    pub mtime: i64,
    /// Comprimento do nome em bytes (UTF-8).
    pub name_len: u16,
    /// Atributos e flags (diretório, oculto, sistema, etc.).
    pub flags: u16,
    /// Padding para alinhamento de 8 bytes e 32 bytes no total.
    pub _reserved: u32,
}

impl Entry {
    #[inline]
    #[must_use]
    pub fn is_dir(&self) -> bool {
        (self.flags & FLAG_DIRECTORY) != 0
    }

    #[inline]
    #[must_use]
    pub fn is_file(&self) -> bool {
        !self.is_dir()
    }

    #[inline]
    #[must_use]
    pub fn is_deleted(&self) -> bool {
        (self.flags & FLAG_DELETED) != 0
    }

    #[inline]
    #[must_use]
    pub fn is_hidden(&self) -> bool {
        (self.flags & FLAG_HIDDEN) != 0
    }

    #[inline]
    #[must_use]
    pub fn is_system(&self) -> bool {
        (self.flags & FLAG_SYSTEM) != 0
    }

    #[inline]
    #[must_use]
    pub fn is_readonly(&self) -> bool {
        (self.flags & FLAG_READONLY) != 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_entry_size() {
        assert_eq!(std::mem::size_of::<Entry>(), 32);
    }

    #[test]
    fn test_entry_flags() {
        let entry = Entry {
            parent: NO_PARENT,
            name_off: 0,
            size: 1024,
            mtime: 0,
            name_len: 4,
            flags: FLAG_DIRECTORY | FLAG_HIDDEN,
            _reserved: 0,
        };
        assert!(entry.is_dir());
        assert!(!entry.is_file());
        assert!(entry.is_hidden());
        assert!(!entry.is_deleted());
    }
}
