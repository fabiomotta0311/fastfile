//! `ff-index`
//!
//! Fonte do índice: leitura da MFT, USN Journal, fallback por varredura
//! e estrutura de dados do índice.

#[cfg(windows)]
pub mod windows {
    //! Componentes específicos para Windows / NTFS.
}

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
