//! Fonte de varredura paralela do sistema de arquivos (fallback sem admin / não-NTFS, §2.4).

use std::collections::HashMap;
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::SystemTime;

use ignore::{WalkBuilder, WalkState};

use crate::entry::{FLAG_DIRECTORY, FLAG_FILE, FLAG_READONLY, NO_PARENT};
#[cfg(windows)]
use crate::entry::{FLAG_HIDDEN, FLAG_SYSTEM};
use crate::index::FileIndex;
use crate::source::{IndexError, IndexSource};

/// Erros originados durante a varredura do sistema de arquivos.
#[derive(Debug)]
pub enum WalkError {
    Io(std::io::Error),
    Index(IndexError),
    RootNotFound(PathBuf),
}

impl fmt::Display for WalkError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(e) => write!(f, "erro de E/S na varredura: {e}"),
            Self::Index(e) => write!(f, "erro ao registrar no índice: {e}"),
            Self::RootNotFound(p) => write!(f, "diretório raiz não encontrado: {}", p.display()),
        }
    }
}

impl std::error::Error for WalkError {}

impl From<std::io::Error> for WalkError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}

impl From<IndexError> for WalkError {
    fn from(e: IndexError) -> Self {
        Self::Index(e)
    }
}

/// Item coletado durante a varredura concorrente.
struct ScannedItem {
    path: PathBuf,
    name: String,
    parent_path: Option<PathBuf>,
    is_dir: bool,
    size: u64,
    allocated_size: Option<u64>,
    mtime: i64,
    flags: u16,
    frn: u64,
}

/// Callback de progresso chamado durante a varredura (`path`, `size`, `is_dir`).
pub type ProgressCallback = Arc<dyn Fn(&Path, u64, bool) + Send + Sync>;

/// Fonte de índice que percorre o disco usando varredura paralela multithread (fallback).
pub struct WalkSource {
    root_path: PathBuf,
    respect_ignore: bool,
    max_depth: Option<usize>,
    cancel_token: Option<Arc<AtomicBool>>,
    progress_fn: Option<ProgressCallback>,
    errors: Arc<Mutex<Vec<(PathBuf, String)>>>,
}

impl WalkSource {
    /// Cria uma nova fonte de varredura a partir do diretório raiz especificado.
    pub fn new<P: AsRef<Path>>(root_path: P) -> Self {
        Self {
            root_path: root_path.as_ref().to_path_buf(),
            respect_ignore: false,
            max_depth: None,
            cancel_token: None,
            progress_fn: None,
            errors: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Configura se deve respeitar regras de `.gitignore` e arquivos ocultos (útil para repositórios).
    #[must_use]
    pub fn with_respect_ignore(mut self, respect: bool) -> Self {
        self.respect_ignore = respect;
        self
    }

    /// Limita a profundidade da varredura.
    #[must_use]
    pub fn with_max_depth(mut self, depth: usize) -> Self {
        self.max_depth = Some(depth);
        self
    }

    /// Configura um token atômico para interrupção/cancelamento da varredura.
    #[must_use]
    pub fn with_cancel_token(mut self, token: Arc<AtomicBool>) -> Self {
        self.cancel_token = Some(token);
        self
    }

    /// Configura uma função de callback para atualização contínua de progresso (`path`, `size`, `is_dir`).
    #[must_use]
    pub fn with_progress<F>(mut self, cb: F) -> Self
    where
        F: Fn(&Path, u64, bool) + Send + Sync + 'static,
    {
        self.progress_fn = Some(Arc::new(cb));
        self
    }

    /// Retorna os erros de permissão ou leitura capturados durante a varredura.
    #[must_use]
    pub fn take_errors(&self) -> Vec<(PathBuf, String)> {
        self.errors
            .lock()
            .map(|mut e| std::mem::take(&mut *e))
            .unwrap_or_default()
    }

    /// Informa se a varredura foi cancelada pelo token.
    #[must_use]
    pub fn is_cancelled(&self) -> bool {
        self.cancel_token
            .as_ref()
            .map(|t| t.load(Ordering::Relaxed))
            .unwrap_or(false)
    }
}

#[cfg(windows)]
fn query_compressed_file_size(path: &Path) -> Option<u64> {
    use std::os::windows::ffi::OsStrExt;
    let wide: Vec<u16> = path
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let mut high: u32 = 0;
    unsafe {
        let low = windows_sys::Win32::Storage::FileSystem::GetCompressedFileSizeW(
            wide.as_ptr(),
            &mut high,
        );
        if low == 0xFFFF_FFFF && windows_sys::Win32::Foundation::GetLastError() != 0 {
            None
        } else {
            Some(((high as u64) << 32) | (low as u64))
        }
    }
}

#[cfg(unix)]
fn extract_frn(meta: &std::fs::Metadata, _fallback: u64) -> u64 {
    use std::os::unix::fs::MetadataExt;
    meta.ino()
}

#[cfg(not(unix))]
fn extract_frn(_meta: &std::fs::Metadata, fallback: u64) -> u64 {
    fallback
}

fn extract_flags_and_meta(
    meta: &std::fs::Metadata,
    #[allow(unused_variables)] path: &Path,
) -> (bool, u64, Option<u64>, i64, u16) {
    let is_dir = meta.is_dir();
    let size = if is_dir { 0 } else { meta.len() };

    let mtime = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_secs() as i64);

    let mut flags = if is_dir { FLAG_DIRECTORY } else { FLAG_FILE };
    if meta.permissions().readonly() {
        flags |= FLAG_READONLY;
    }

    let mut allocated_size = if is_dir {
        None
    } else {
        Some(if size == 0 {
            0
        } else {
            size.div_ceil(4096) * 4096
        })
    };

    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        let attrs = meta.file_attributes();
        if (attrs & 0x2) != 0 {
            flags |= FLAG_HIDDEN;
        }
        if (attrs & 0x4) != 0 {
            flags |= FLAG_SYSTEM;
        }
        if (attrs & 0x200) != 0 {
            flags |= crate::entry::FLAG_SPARSE;
        }
        if (attrs & 0x800) != 0 {
            flags |= crate::entry::FLAG_COMPRESSED;
        }
        if (attrs & 0x400) != 0 {
            flags |= crate::entry::FLAG_REPARSE;
        }
        if (attrs & 0x1000) != 0 || (attrs & 0x440000) != 0 {
            flags |= crate::entry::FLAG_OFFLINE;
        }

        if !is_dir {
            if (flags & crate::entry::FLAG_OFFLINE) != 0 {
                // Arquivo offline / na nuvem: não ocupa espaço em disco local
                allocated_size = Some(0);
            } else if (flags & (crate::entry::FLAG_SPARSE | crate::entry::FLAG_COMPRESSED)) != 0 {
                // Arquivo esparso ou comprimido: mede tamanho alocado real
                if let Some(comp_sz) = query_compressed_file_size(path) {
                    allocated_size = Some(comp_sz);
                }
            }
        }
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if meta.nlink() > 1 {
            flags |= crate::entry::FLAG_HARDLINK;
        }
        if !is_dir {
            // Em Unix, blocks() informa quantidade de blocos de 512 bytes alocados
            allocated_size = Some(meta.blocks() * 512);
        }
    }

    (is_dir, size, allocated_size, mtime, flags)
}

impl IndexSource for WalkSource {
    type Error = WalkError;

    fn populate(&mut self, index: &mut FileIndex) -> Result<(), Self::Error> {
        let canonical_root = self
            .root_path
            .canonicalize()
            .map_err(|_| WalkError::RootNotFound(self.root_path.clone()))?;

        let root_meta = std::fs::symlink_metadata(&canonical_root)?;
        let (root_is_dir, root_size, root_allocated, root_mtime, root_flags) =
            extract_flags_and_meta(&root_meta, &canonical_root);
        let root_frn = extract_frn(&root_meta, 1);

        let root_str = canonical_root.to_string_lossy();
        let root_name = root_str.strip_prefix(r"\\?\").unwrap_or(&root_str);

        let root_id = index.add_entry(
            root_frn,
            NO_PARENT,
            root_name,
            root_flags | if root_is_dir { FLAG_DIRECTORY } else { 0 },
            root_size,
            root_mtime,
        )?;
        index.set_allocated_size(root_id, root_allocated);

        let mut path_to_id: HashMap<PathBuf, u32> = HashMap::with_capacity(4096);
        path_to_id.insert(canonical_root.clone(), root_id);

        let (tx, rx) = mpsc::channel();
        let fallback_counter = AtomicU64::new(10_000);
        let fallback_ref = &fallback_counter;

        let mut builder = WalkBuilder::new(&canonical_root);
        builder.follow_links(false).same_file_system(false);

        if self.respect_ignore {
            builder
                .hidden(true)
                .ignore(true)
                .git_ignore(true)
                .git_exclude(true);
        } else {
            builder
                .hidden(false)
                .ignore(false)
                .git_ignore(false)
                .git_exclude(false);
        }

        if let Some(depth) = self.max_depth {
            builder.max_depth(Some(depth));
        }

        let walker = builder.build_parallel();
        let cancel_token = self.cancel_token.clone();
        let progress_fn = self.progress_fn.clone();
        let errors = self.errors.clone();

        walker.run(|| {
            let tx = tx.clone();
            let root = canonical_root.clone();
            let cancel_token = cancel_token.clone();
            let progress_fn = progress_fn.clone();
            let errors = errors.clone();

            Box::new(move |entry_result| {
                if let Some(ref token) = cancel_token {
                    if token.load(Ordering::Relaxed) {
                        return WalkState::Quit;
                    }
                }

                let entry = match entry_result {
                    Ok(e) => e,
                    Err(err) => {
                        // Registra erros e pastas sem permissão (TM-02)
                        let path = match &err {
                            ignore::Error::WithPath { path, .. } => path.clone(),
                            ignore::Error::WithDepth { err: inner, .. }
                            | ignore::Error::WithLineNumber { err: inner, .. } => {
                                if let ignore::Error::WithPath { path, .. } = &**inner {
                                    path.clone()
                                } else {
                                    PathBuf::from(".")
                                }
                            }
                            _ => PathBuf::from("."),
                        };
                        if let Ok(mut lock) = errors.lock() {
                            lock.push((path, err.to_string()));
                        }
                        return WalkState::Continue;
                    }
                };

                let path = entry.path();
                if path == root {
                    return WalkState::Continue;
                }

                let meta = match entry.metadata() {
                    Ok(m) => m,
                    Err(err) => {
                        if let Ok(mut lock) = errors.lock() {
                            lock.push((path.to_path_buf(), err.to_string()));
                        }
                        return WalkState::Continue;
                    }
                };

                let name = entry.file_name().to_string_lossy().into_owned();
                let parent_path = path.parent().map(Path::to_path_buf);
                let (is_dir, size, allocated_size, mtime, flags) =
                    extract_flags_and_meta(&meta, path);
                let fallback = fallback_ref.fetch_add(1, Ordering::Relaxed);
                let frn = extract_frn(&meta, fallback);

                if let Some(ref cb) = progress_fn {
                    cb(path, size, is_dir);
                }

                let item = ScannedItem {
                    path: path.to_path_buf(),
                    name,
                    parent_path,
                    is_dir,
                    size,
                    allocated_size,
                    mtime,
                    flags,
                    frn,
                };

                let _ = tx.send(item);
                WalkState::Continue
            })
        });

        drop(tx);

        let mut items = Vec::with_capacity(4096);
        for item in rx {
            items.push(item);
        }

        // Ordena por profundidade de caminho (número de componentes) para garantir
        // que pais sejam sempre processados antes dos filhos
        items.sort_by_key(|item| item.path.components().count());

        for item in items {
            if let Some(ref token) = self.cancel_token {
                if token.load(Ordering::Relaxed) {
                    break;
                }
            }

            let parent_id = if let Some(ref p) = item.parent_path {
                path_to_id.get(p).copied().unwrap_or(root_id)
            } else {
                root_id
            };

            let id = index.add_entry(
                item.frn, parent_id, &item.name, item.flags, item.size, item.mtime,
            )?;
            index.set_allocated_size(id, item.allocated_size);

            if item.is_dir {
                path_to_id.insert(item.path, id);
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_walk_source_on_current_repo() {
        let mut index = FileIndex::new();
        let mut source = WalkSource::new(".").with_respect_ignore(true);
        source.populate(&mut index).unwrap();

        assert!(!index.is_empty());
        // Deve encontrar Cargo.toml
        let mut found_cargo = false;
        for id in 0..index.len() as u32 {
            if index.name(id).unwrap_or("") == "Cargo.toml" {
                found_cargo = true;
                let path = index.build_path(id).unwrap();
                assert!(path.ends_with("Cargo.toml"));
                break;
            }
        }
        assert!(found_cargo, "Cargo.toml deveria estar no índice varrido");
    }
}
