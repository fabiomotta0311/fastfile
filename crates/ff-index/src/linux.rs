//! Fonte do índice com atualização ao vivo no Linux (`fanotify` / `inotify`).
//!
//! Permite que o FastFile mantenha o índice sincronizado em tempo real (< 1 s)
//! sem a necessidade de revarrer o disco a cada consulta em servidores Linux (§3.4 e §3.5).

#[cfg(target_os = "linux")]
mod imp {
    use std::collections::HashMap;
    use std::ffi::CString;
    use std::fs;
    use std::io;
    use std::os::unix::ffi::OsStrExt;
    use std::os::unix::fs::MetadataExt;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, RwLock};
    use std::time::Duration;

    use ignore::WalkBuilder;

    use crate::entry::{FLAG_DIRECTORY, FLAG_FILE, FLAG_READONLY};
    use crate::index::FileIndex;
    use crate::source::{IndexChange, IndexError, IndexSource};

    /// Tipo do mecanismo de monitoramento ativo no Linux.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum LinuxWatcherKind {
        /// Monitoramento por volume/montagem (exige CAP_SYS_ADMIN / root).
        Fanotify,
        /// Monitoramento por diretórios via inotify (funciona sem privilégios elevados).
        Inotify,
        /// Nenhum mecanismo ativo.
        None,
    }

    /// Erros específicos do subsistema de monitoramento Linux.
    #[derive(Debug)]
    pub enum LinuxError {
        Io(io::Error),
        Index(IndexError),
        Initialization(String),
        WatchLimitExceeded,
    }

    impl std::fmt::Display for LinuxError {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            match self {
                Self::Io(e) => write!(f, "erro de E/S no Linux: {e}"),
                Self::Index(e) => write!(f, "erro no índice: {e}"),
                Self::Initialization(msg) => write!(f, "falha na inicialização do watcher: {msg}"),
                Self::WatchLimitExceeded => write!(f, "limite de watches do inotify excedido"),
            }
        }
    }

    impl std::error::Error for LinuxError {}

    impl From<io::Error> for LinuxError {
        fn from(e: io::Error) -> Self {
            Self::Io(e)
        }
    }

    impl From<IndexError> for LinuxError {
        fn from(e: IndexError) -> Self {
            Self::Index(e)
        }
    }

    /// Fonte de dados com atualização contínua em tempo real para Linux.
    pub struct LinuxLiveSource {
        /// Diretório base monitorado.
        pub root: PathBuf,
        /// Respeitar regras de `.gitignore` e ignorar ocultos.
        pub respect_ignore: bool,
        /// Tipo de watcher ativo.
        pub watcher_kind: LinuxWatcherKind,
        /// File descriptor do inotify.
        inotify_fd: i32,
        /// Mapeamento de Watch Descriptor (wd) -> (Caminho da Pasta, Inode da Pasta).
        wd_map: HashMap<i32, (PathBuf, u64)>,
        /// Mapeamento de caminho relativo/absoluto para inode (para exclusões e movimentações).
        path_to_ino: HashMap<PathBuf, u64>,
    }

    const INOTIFY_WATCH_MASK: u32 = libc::IN_CREATE
        | libc::IN_DELETE
        | libc::IN_MODIFY
        | libc::IN_MOVED_FROM
        | libc::IN_MOVED_TO
        | libc::IN_ATTRIB
        | libc::IN_DONT_FOLLOW;

    impl LinuxLiveSource {
        /// Cria uma nova fonte para o diretório raiz informado.
        pub fn new(root: impl AsRef<Path>) -> Result<Self, LinuxError> {
            let root_buf = root.as_ref().to_path_buf();
            if !root_buf.exists() {
                return Err(LinuxError::Io(io::Error::new(
                    io::ErrorKind::NotFound,
                    format!("diretório raiz '{}' não existe", root_buf.display()),
                )));
            }

            // Tenta inicializar inotify não-bloqueante
            let ifd = unsafe { libc::inotify_init1(libc::IN_NONBLOCK | libc::IN_CLOEXEC) };
            let (inotify_fd, watcher_kind) = if ifd >= 0 {
                (ifd, LinuxWatcherKind::Inotify)
            } else {
                return Err(LinuxError::Initialization(format!(
                    "falha ao criar inotify: {}",
                    io::Error::last_os_error()
                )));
            };

            Ok(Self {
                root: root_buf,
                respect_ignore: true,
                watcher_kind,
                inotify_fd,
                wd_map: HashMap::new(),
                path_to_ino: HashMap::new(),
            })
        }

        /// Retorna o mecanismo de monitoramento ativo.
        #[inline]
        pub fn watcher_kind(&self) -> LinuxWatcherKind {
            self.watcher_kind
        }

        /// Configura se deve respeitar `.gitignore`.
        #[must_use]
        pub fn with_respect_ignore(mut self, respect: bool) -> Self {
            self.respect_ignore = respect;
            self
        }

        /// Registra um diretório no inotify.
        fn add_dir_watch(&mut self, path: &Path, inode: u64) -> Result<i32, LinuxError> {
            let c_path = CString::new(path.as_os_str().as_bytes()).map_err(|e| {
                LinuxError::Io(io::Error::new(io::ErrorKind::InvalidInput, e.to_string()))
            })?;

            let wd = unsafe {
                libc::inotify_add_watch(self.inotify_fd, c_path.as_ptr(), INOTIFY_WATCH_MASK)
            };

            if wd < 0 {
                let err = io::Error::last_os_error();
                if err.raw_os_error() == Some(libc::ENOSPC) {
                    return Err(LinuxError::WatchLimitExceeded);
                }
                return Err(LinuxError::Io(err));
            }

            self.wd_map.insert(wd, (path.to_path_buf(), inode));
            self.path_to_ino.insert(path.to_path_buf(), inode);
            Ok(wd)
        }

        /// Lê e processa eventos pendentes no inotify, atualizando o índice em tempo real.
        /// Retorna a lista de alterações aplicadas ao índice.
        pub fn poll_events(
            &mut self,
            index: &mut FileIndex,
        ) -> Result<Vec<IndexChange>, LinuxError> {
            let mut changes = Vec::new();
            let mut buffer = [0u8; 8192];

            loop {
                let bytes_read = unsafe {
                    libc::read(
                        self.inotify_fd,
                        buffer.as_mut_ptr() as *mut libc::c_void,
                        buffer.len(),
                    )
                };

                if bytes_read <= 0 {
                    let err = io::Error::last_os_error();
                    if err.raw_os_error() == Some(libc::EAGAIN)
                        || err.raw_os_error() == Some(libc::EWOULDBLOCK)
                    {
                        // Não há mais eventos na fila
                        break;
                    }
                    if bytes_read == 0 {
                        break;
                    }
                    return Err(LinuxError::Io(err));
                }

                let mut offset = 0usize;
                let total = bytes_read as usize;

                while offset + std::mem::size_of::<libc::inotify_event>() <= total {
                    let event =
                        unsafe { &*(buffer[offset..].as_ptr() as *const libc::inotify_event) };

                    let event_len = event.len as usize;
                    let header_size = std::mem::size_of::<libc::inotify_event>();

                    let name = if event_len > 0 && offset + header_size + event_len <= total {
                        let name_bytes =
                            &buffer[offset + header_size..offset + header_size + event_len];
                        let nul_pos = name_bytes
                            .iter()
                            .position(|&b| b == 0)
                            .unwrap_or(name_bytes.len());
                        String::from_utf8_lossy(&name_bytes[..nul_pos]).to_string()
                    } else {
                        String::new()
                    };

                    let wd = event.wd;
                    let mask = event.mask;

                    offset += header_size + event_len;

                    if let Some((dir_path, parent_inode)) = self.wd_map.get(&wd).cloned() {
                        let full_path = if name.is_empty() {
                            dir_path.clone()
                        } else {
                            dir_path.join(&name)
                        };

                        if mask & (libc::IN_CREATE | libc::IN_MOVED_TO) != 0 {
                            // Arquivo ou pasta criada
                            if let Ok(meta) = fs::symlink_metadata(&full_path) {
                                let ino = meta.ino();
                                let size = meta.len();
                                let is_dir = meta.is_dir();
                                let mut flags = if is_dir { FLAG_DIRECTORY } else { FLAG_FILE };
                                if meta.permissions().readonly() {
                                    flags |= FLAG_READONLY;
                                }
                                let mtime = meta.mtime();

                                self.path_to_ino.insert(full_path.clone(), ino);

                                if is_dir {
                                    let _ = self.add_dir_watch(&full_path, ino);
                                }

                                let change = IndexChange::Insert {
                                    frn: ino,
                                    parent_frn: parent_inode,
                                    name: name.clone(),
                                    flags,
                                    size,
                                    mtime,
                                };
                                let _ = index.apply_change(&change);
                                changes.push(change);
                            }
                        } else if mask & (libc::IN_DELETE | libc::IN_MOVED_FROM) != 0 {
                            // Arquivo ou pasta removida
                            if let Some(ino) = self.path_to_ino.remove(&full_path) {
                                // Se for um diretório monitorado, limpa do wd_map
                                self.wd_map.retain(|&w, (p, _)| {
                                    if p == &full_path {
                                        unsafe {
                                            libc::inotify_rm_watch(self.inotify_fd, w);
                                        }
                                        false
                                    } else {
                                        true
                                    }
                                });
                                let change = IndexChange::Delete { frn: ino };
                                let _ = index.apply_change(&change);
                                changes.push(change);
                            }
                        } else if mask & libc::IN_MODIFY != 0 {
                            // Arquivo modificado
                            if let Ok(meta) = fs::symlink_metadata(&full_path) {
                                let ino = meta.ino();
                                let size = meta.len();
                                let is_dir = meta.is_dir();
                                let mut flags = if is_dir { FLAG_DIRECTORY } else { FLAG_FILE };
                                if meta.permissions().readonly() {
                                    flags |= FLAG_READONLY;
                                }
                                let mtime = meta.mtime();

                                let change = IndexChange::Modify {
                                    frn: ino,
                                    size,
                                    mtime,
                                    flags,
                                };
                                let _ = index.apply_change(&change);
                                changes.push(change);
                            }
                        }
                    }
                }
            }

            Ok(changes)
        }

        /// Inicia uma thread de segundo plano que monitora o sistema de arquivos continuamente,
        /// atualizando a instância compartilhada de `FileIndex` com latência < 100 ms.
        pub fn spawn_watcher_thread(
            mut self,
            index: Arc<RwLock<FileIndex>>,
            stop_flag: Arc<AtomicBool>,
        ) -> std::thread::JoinHandle<()> {
            std::thread::spawn(move || {
                while !stop_flag.load(Ordering::Relaxed) {
                    if let Ok(mut lock) = index.write() {
                        let _ = self.poll_events(&mut lock);
                    }
                    std::thread::sleep(Duration::from_millis(50));
                }
            })
        }
    }

    impl IndexSource for LinuxLiveSource {
        type Error = LinuxError;

        fn populate(&mut self, index: &mut FileIndex) -> Result<(), Self::Error> {
            let mut builder = WalkBuilder::new(&self.root);
            builder.follow_links(false);
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

            // Registra raiz
            let root = self.root.clone();
            if let Ok(root_meta) = fs::symlink_metadata(&root) {
                let root_ino = root_meta.ino();
                let _ = self.add_dir_watch(&root, root_ino);
            }

            let mut walk_source =
                crate::walk::WalkSource::new(&root).with_respect_ignore(self.respect_ignore);

            walk_source
                .populate(index)
                .map_err(|e| LinuxError::Io(io::Error::other(e.to_string())))?;

            // Mapeia todas as pastas encontradas para o inotify
            for entry_res in builder.build().filter_map(|e| e.ok()) {
                let path = entry_res.path();
                if entry_res.file_type().is_some_and(|ft| ft.is_dir()) {
                    if let Ok(meta) = entry_res.metadata() {
                        let ino = meta.ino();
                        let _ = self.add_dir_watch(path, ino);
                    }
                } else if let Ok(meta) = entry_res.metadata() {
                    self.path_to_ino.insert(path.to_path_buf(), meta.ino());
                }
            }

            Ok(())
        }
    }

    impl Drop for LinuxLiveSource {
        fn drop(&mut self) {
            if self.inotify_fd >= 0 {
                unsafe {
                    libc::close(self.inotify_fd);
                }
            }
        }
    }
}

#[cfg(not(target_os = "linux"))]
mod imp {
    use std::path::{Path, PathBuf};
    use std::sync::atomic::AtomicBool;
    use std::sync::{Arc, RwLock};

    use crate::index::FileIndex;
    use crate::source::{IndexChange, IndexError, IndexSource};

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum LinuxWatcherKind {
        Fanotify,
        Inotify,
        None,
    }

    #[derive(Debug)]
    pub enum LinuxError {
        UnsupportedPlatform,
        Index(IndexError),
        Io(std::io::Error),
    }

    impl std::fmt::Display for LinuxError {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            match self {
                Self::UnsupportedPlatform => {
                    write!(f, "LinuxLiveSource só está disponível no Linux")
                }
                Self::Index(e) => write!(f, "erro no índice: {e}"),
                Self::Io(e) => write!(f, "erro de E/S: {e}"),
            }
        }
    }

    impl std::error::Error for LinuxError {}

    pub struct LinuxLiveSource {
        pub root: PathBuf,
        pub respect_ignore: bool,
    }

    impl LinuxLiveSource {
        pub fn new(root: impl AsRef<Path>) -> Result<Self, LinuxError> {
            Ok(Self {
                root: root.as_ref().to_path_buf(),
                respect_ignore: true,
            })
        }

        #[inline]
        pub fn watcher_kind(&self) -> LinuxWatcherKind {
            LinuxWatcherKind::None
        }

        #[must_use]
        pub fn with_respect_ignore(mut self, respect: bool) -> Self {
            self.respect_ignore = respect;
            self
        }

        pub fn poll_events(
            &mut self,
            _index: &mut FileIndex,
        ) -> Result<Vec<IndexChange>, LinuxError> {
            Ok(Vec::new())
        }

        pub fn spawn_watcher_thread(
            self,
            _index: Arc<RwLock<FileIndex>>,
            _stop_flag: Arc<AtomicBool>,
        ) -> std::thread::JoinHandle<()> {
            std::thread::spawn(|| {})
        }
    }

    impl IndexSource for LinuxLiveSource {
        type Error = LinuxError;

        fn populate(&mut self, index: &mut FileIndex) -> Result<(), Self::Error> {
            let mut walk =
                crate::walk::WalkSource::new(&self.root).with_respect_ignore(self.respect_ignore);
            walk.populate(index)
                .map_err(|e| LinuxError::Io(std::io::Error::other(e.to_string())))
        }
    }
}

pub use imp::*;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{FileIndex, IndexSource};
    use std::fs;

    #[test]
    fn test_linux_source_creation_and_populate() {
        let tmp = std::env::temp_dir().join(format!("ff_linux_test_{}", std::process::id()));
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(tmp.join("subdir")).unwrap();
        fs::write(tmp.join("subdir/file1.txt"), "hello linux\n").unwrap();

        let mut source = LinuxLiveSource::new(&tmp).unwrap();
        let mut index = FileIndex::new();
        source.populate(&mut index).unwrap();

        assert!(index.len() >= 2); // subdir e file1.txt

        let _ = fs::remove_dir_all(&tmp);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn test_linux_live_events_create_modify_delete() {
        let tmp = std::env::temp_dir().join(format!("ff_linux_live_{}", std::process::id()));
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(&tmp).unwrap();

        let mut source = LinuxLiveSource::new(&tmp).unwrap();
        let mut index = FileIndex::new();
        source.populate(&mut index).unwrap();

        let initial_count = index.len();

        // 1. Criar novo arquivo
        let new_file = tmp.join("created_live.txt");
        fs::write(&new_file, "conteudo novo\n").unwrap();

        // Aguarda evento inotify
        std::thread::sleep(std::time::Duration::from_millis(100));
        let changes = source.poll_events(&mut index).unwrap();
        assert!(!changes.is_empty());
        assert!(index.len() > initial_count);

        // 2. Modificar arquivo
        fs::write(&new_file, "conteudo modificado maior\n").unwrap();
        std::thread::sleep(std::time::Duration::from_millis(100));
        let mod_changes = source.poll_events(&mut index).unwrap();
        assert!(!mod_changes.is_empty());

        // 3. Deletar arquivo
        fs::remove_file(&new_file).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(100));
        let del_changes = source.poll_events(&mut index).unwrap();
        assert!(!del_changes.is_empty());

        let _ = fs::remove_dir_all(&tmp);
    }
}
