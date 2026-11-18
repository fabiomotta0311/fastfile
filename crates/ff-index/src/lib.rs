//! `ff-index`
//!
//! Fonte do índice: leitura da MFT, USN Journal, fallback por varredura
//! e estrutura de dados do índice (Struct-of-Arrays com arenas de nomes).

pub mod entry;
pub mod index;
pub mod linux;
pub mod ntfs;
pub mod path;
pub mod source;
pub mod synthetic;
pub mod walk;

#[cfg(windows)]
pub mod windows;

pub use entry::{
    Entry, FLAG_ARCHIVE, FLAG_DELETED, FLAG_DIRECTORY, FLAG_FILE, FLAG_HIDDEN, FLAG_READONLY,
    FLAG_SYSTEM, NO_PARENT,
};
pub use index::FileIndex;
pub use linux::{LinuxError, LinuxLiveSource, LinuxWatcherKind};
pub use path::{
    clean_path, clean_unc_prefix, has_ignored_component, is_ignored_build_dir,
    is_in_directory_tree, is_secret_file, is_target_in_volume,
};
pub use source::{IndexChange, IndexError, IndexSource};
pub use synthetic::SyntheticSource;
pub use walk::{WalkError, WalkSource};

#[cfg(windows)]
pub use windows::{MftSource, UsnSource, UsnWatcher};

#[must_use]
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version() {
        assert_eq!(version(), env!("CARGO_PKG_VERSION"));
    }
}
