//! Leitura e acompanhamento do NTFS pelo USN Journal (Rota 1 e Checkpoints A, C, E).

#![cfg(windows)]

use std::fmt;
use std::io;
use std::mem::size_of;
use std::os::raw::c_void;
use std::time::Instant;

use windows_sys::Win32::Foundation::{CloseHandle, ERROR_HANDLE_EOF, HANDLE, INVALID_HANDLE_VALUE};
use windows_sys::Win32::System::IO::DeviceIoControl;

use crate::entry::NO_PARENT;
use crate::index::FileIndex;
use crate::ntfs::parse_usn_records;
use crate::source::{IndexChange, IndexError, IndexSource};
use crate::walk::WalkSource;
use crate::windows::volume::open_volume;

/// Control Codes de I/O para o USN Journal no Windows.
pub const FSCTL_ENUM_USN_DATA: u32 = 0x000900b3;
pub const FSCTL_READ_USN_JOURNAL: u32 = 0x000900bb;
pub const FSCTL_QUERY_USN_JOURNAL: u32 = 0x000900f4;

/// Razões de alteração no USN Journal.
pub const USN_REASON_DATA_OVERWRITE: u32 = 0x0000_0001;
pub const USN_REASON_DATA_EXTEND: u32 = 0x0000_0002;
pub const USN_REASON_DATA_TRUNCATION: u32 = 0x0000_0004;
pub const USN_REASON_FILE_CREATE: u32 = 0x0000_0100;
pub const USN_REASON_FILE_DELETE: u32 = 0x0000_0200;
pub const USN_REASON_RENAME_OLD_NAME: u32 = 0x0000_1000;
pub const USN_REASON_RENAME_NEW_NAME: u32 = 0x0000_2000;
pub const USN_REASON_BASIC_INFO_CHANGE: u32 = 0x0000_8000;

/// Estrutura para enumeração de USN (`MFT_ENUM_DATA_V0`).
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct MftEnumDataV0 {
    pub start_file_reference_number: u64,
    pub low_usn: i64,
    pub high_usn: i64,
}

/// Estrutura de dados do USN Journal retornado por `FSCTL_QUERY_USN_JOURNAL`.
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct UsnJournalDataV0 {
    pub usn_journal_id: u64,
    pub first_usn: i64,
    pub next_usn: i64,
    pub lowest_valid_usn: i64,
    pub max_usn: i64,
    pub maximum_size: u64,
    pub allocation_delta: u64,
}

/// Estrutura para leitura contínua de alterações (`READ_USN_JOURNAL_DATA_V0`).
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct ReadUsnJournalDataV0 {
    pub start_usn: i64,
    pub reason_mask: u32,
    pub return_only_on_close: u32,
    pub timeout: u64,
    pub bytes_to_wait_for: u64,
    pub usn_journal_id: u64,
}

/// Registro individual do USN versão 2.0 (`USN_RECORD_V2`).
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct UsnRecordV2 {
    pub record_length: u32,
    pub major_version: u16,
    pub minor_version: u16,
    pub file_reference_number: u64,
    pub parent_file_reference_number: u64,
    pub usn: i64,
    pub time_stamp: i64,
    pub reason: u32,
    pub source_info: u32,
    pub security_id: u32,
    pub file_attributes: u32,
    pub file_name_length: u16,
    pub file_name_offset: u16,
}

pub use crate::ntfs::FRN_BASE_MASK;

// O parser puro (`crate::ntfs`) lê estes campos por deslocamento fixo: confere o layout `repr(C)`.
// (`size_of` é 64 por padding de alinhamento; o cabeçalho real termina em 60.)
const _: () = {
    use std::mem::offset_of;
    assert!(offset_of!(UsnRecordV2, record_length) == 0);
    assert!(offset_of!(UsnRecordV2, major_version) == 4);
    assert!(offset_of!(UsnRecordV2, file_reference_number) == 8);
    assert!(offset_of!(UsnRecordV2, parent_file_reference_number) == 16);
    assert!(offset_of!(UsnRecordV2, time_stamp) == 32);
    assert!(offset_of!(UsnRecordV2, reason) == 40);
    assert!(offset_of!(UsnRecordV2, file_attributes) == 52);
    assert!(offset_of!(UsnRecordV2, file_name_length) == 56);
    assert!(offset_of!(UsnRecordV2, file_name_offset) == 58);
    assert!(offset_of!(UsnRecordV2, file_name_offset) + 2 == crate::ntfs::USN_RECORD_V2_HEADER_LEN);
};

/// Erros decorrentes de operações com o USN Journal.
#[derive(Debug)]
pub enum UsnError {
    Io(io::Error),
    Index(IndexError),
    ElevationRequired,
    NotNtfsVolume(String),
}

impl fmt::Display for UsnError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(e) => write!(f, "erro de E/S no USN Journal: {e}"),
            Self::Index(e) => write!(f, "erro ao atualizar índice a partir do USN: {e}"),
            Self::ElevationRequired => {
                write!(
                    f,
                    "acesso negado ao volume NTFS: privilégios de administrador necessários"
                )
            }
            Self::NotNtfsVolume(v) => {
                write!(f, "o volume '{v}' não é NTFS ou não possui USN Journal")
            }
        }
    }
}

impl std::error::Error for UsnError {}

impl From<io::Error> for UsnError {
    fn from(e: io::Error) -> Self {
        Self::Io(e)
    }
}

impl From<IndexError> for UsnError {
    fn from(e: IndexError) -> Self {
        Self::Index(e)
    }
}

/// Fonte de dados do índice a partir do `FSCTL_ENUM_USN_DATA` (Rota 1).
pub struct UsnSource {
    pub volume: String,
    pub last_usn: i64,
    pub journal_id: u64,
    pub elapsed_ms: f64,
    pub fallback_used: bool,
}

impl UsnSource {
    pub fn new(volume: impl Into<String>) -> Self {
        Self {
            volume: volume.into(),
            last_usn: 0,
            journal_id: 0,
            elapsed_ms: 0.0,
            fallback_used: false,
        }
    }

    /// Consulta os metadados do journal no volume.
    ///
    /// # Safety
    /// O ponteiro `handle` deve ser um identificador de arquivo Win32 aberto e válido.
    pub unsafe fn query_journal(handle: HANDLE) -> io::Result<UsnJournalDataV0> {
        let mut journal_data = UsnJournalDataV0::default();
        let mut bytes_returned = 0u32;

        // SAFETY: os buffers de entrada e saída são locais, vivem além da chamada e têm os tamanhos informados.
        let success = unsafe {
            DeviceIoControl(
                handle,
                FSCTL_QUERY_USN_JOURNAL,
                std::ptr::null(),
                0,
                &mut journal_data as *mut _ as *mut c_void,
                size_of::<UsnJournalDataV0>() as u32,
                &mut bytes_returned,
                std::ptr::null_mut(),
            )
        };

        if success != 0 {
            Ok(journal_data)
        } else {
            Err(io::Error::last_os_error())
        }
    }

    /// Tenta retomar o índice carregado a partir do último USN registrado no snapshot (§3, USN-RESUME-001).
    ///
    /// Retorna `Ok(true)` se o journal não foi truncado e os deltas foram aplicados com sucesso.
    /// Retorna `Ok(false)` se o journal foi truncado ou o ID divergiu (necessitando reindexação completa).
    pub fn resume_from_snapshot(volume: &str, index: &mut FileIndex) -> Result<bool, UsnError> {
        let handle = match open_volume(volume, false) {
            Ok(h) => h,
            Err(_) => return Ok(false),
        };

        // SAFETY: `handle` foi aberto por `open_volume` e permanece válido durante a chamada.
        let journal_data = match unsafe { Self::query_journal(handle) } {
            Ok(jd) => jd,
            Err(_) => {
                // SAFETY: o handle é válido, pertence a este escopo e é fechado uma única vez, sem uso posterior.
                unsafe { CloseHandle(handle) };
                return Ok(false);
            }
        };

        // SAFETY: o handle é válido, pertence a este escopo e é fechado uma única vez, sem uso posterior.
        unsafe { CloseHandle(handle) };

        // Verifica se o Journal ID coincide e se o último USN ainda é válido (não truncado)
        if index.journal_id != journal_data.usn_journal_id
            || index.last_usn < journal_data.lowest_valid_usn
            || index.last_usn > journal_data.next_usn
        {
            return Ok(false);
        }

        if index.last_usn == journal_data.next_usn {
            return Ok(true);
        }

        let trace = std::env::var_os("FASTFILE_MFT_TRACE").is_some();
        let t_watcher = Instant::now();
        let mut watcher = match UsnWatcher::new(volume, index.journal_id, index.last_usn) {
            Ok(w) => w,
            Err(_) => return Ok(false),
        };
        if trace {
            eprintln!("[usn] watcher aberto em {:.2?}", t_watcher.elapsed());
        }

        // Cada leitura traz no máximo 64 KiB: repete até alcançar o fim do diário (sem progresso, para).
        let target = journal_data.next_usn;
        let (mut polls, mut applied) = (0u32, 0usize);
        while watcher.last_usn < target {
            let before = watcher.last_usn;
            let t_poll = Instant::now();
            let changes = watcher.poll_changes(index)?;
            polls += 1;
            applied += changes.len();
            if trace {
                eprintln!(
                    "[usn] leitura {polls}: {} mudanças, USN {before} -> {}, {:.2?}",
                    changes.len(),
                    watcher.last_usn,
                    t_poll.elapsed()
                );
            }
            if watcher.last_usn <= before {
                break;
            }
        }
        index.last_usn = watcher.last_usn;
        if trace {
            eprintln!("[usn] retomada: {polls} leituras, {applied} mudanças aplicadas");
        }

        Ok(true)
    }
}

impl IndexSource for UsnSource {
    type Error = UsnError;

    fn populate(&mut self, index: &mut FileIndex) -> Result<(), Self::Error> {
        let start = Instant::now();

        let handle_res = open_volume(&self.volume, false);
        let handle = match handle_res {
            Ok(h) => h,
            Err(err) => {
                // Se falhar por falta de permissão ou volume não acessível diretamente, usa fallback
                self.fallback_used = true;
                let mut fallback = WalkSource::new(&self.volume);
                fallback.populate(index).map_err(|e| {
                    UsnError::Io(io::Error::other(format!(
                        "fallback falhou após erro ({err}): {e}"
                    )))
                })?;
                self.elapsed_ms = (start.elapsed().as_secs_f64() * 1000.0 * 100.0).round() / 100.0;
                return Ok(());
            }
        };

        // SAFETY: `handle` foi aberto por `open_volume` e permanece válido durante a chamada.
        let journal_res = unsafe { Self::query_journal(handle) };
        let journal_data = match journal_res {
            Ok(jd) => jd,
            Err(err) => {
                // SAFETY: o handle é válido, pertence a este escopo e é fechado uma única vez, sem uso posterior.
                unsafe { CloseHandle(handle) };
                self.fallback_used = true;
                let mut fallback = WalkSource::new(&self.volume);
                fallback.populate(index).map_err(|e| {
                    UsnError::Io(io::Error::other(format!(
                        "fallback falhou ao consultar USN ({err}): {e}"
                    )))
                })?;
                self.elapsed_ms = (start.elapsed().as_secs_f64() * 1000.0 * 100.0).round() / 100.0;
                return Ok(());
            }
        };

        self.journal_id = journal_data.usn_journal_id;
        self.last_usn = journal_data.next_usn;

        // Buffer de 512 KB para alta taxa de transferência na enumeração
        const BUFFER_SIZE: usize = 512 * 1024;
        let mut buffer = vec![0u8; BUFFER_SIZE];

        let mut enum_data = MftEnumDataV0 {
            start_file_reference_number: 0,
            low_usn: 0,
            high_usn: journal_data.next_usn,
        };

        // Estrutura temporária para mapear pai pendente de cada entrada
        // (id -> parent_frn)
        let mut pending_parents: Vec<(u32, u64)> = Vec::with_capacity(100_000);

        loop {
            let mut bytes_returned = 0u32;
            // SAFETY: os buffers de entrada e saída são locais, vivem além da chamada e têm os tamanhos informados.
            let success = unsafe {
                DeviceIoControl(
                    handle,
                    FSCTL_ENUM_USN_DATA,
                    &enum_data as *const _ as *const c_void,
                    size_of::<MftEnumDataV0>() as u32,
                    buffer.as_mut_ptr() as *mut c_void,
                    BUFFER_SIZE as u32,
                    &mut bytes_returned,
                    std::ptr::null_mut(),
                )
            };

            if success == 0 {
                let err = io::Error::last_os_error();
                if err.raw_os_error() == Some(ERROR_HANDLE_EOF as i32) {
                    break;
                }
                // SAFETY: o handle é válido, pertence a este escopo e é fechado uma única vez, sem uso posterior.
                unsafe { CloseHandle(handle) };
                return Err(UsnError::Io(err));
            }

            if bytes_returned < 8 {
                break;
            }

            // Os primeiros 8 bytes contêm o próximo FileReferenceNumber a ser consultado
            let next_frn = u64::from_le_bytes(buffer[0..8].try_into().unwrap());
            enum_data.start_file_reference_number = next_frn;

            let total_bytes = (bytes_returned as usize).min(buffer.len());
            for record in parse_usn_records(&buffer[..total_bytes]) {
                let Some(name) = record.name else { continue };
                // Adiciona entrada com pai temporário NO_PARENT e registra para resolução posterior
                if let Ok(entry_id) = index.add_entry(
                    record.frn,
                    NO_PARENT,
                    &name,
                    record.flags,
                    0,
                    record.unix_secs,
                ) {
                    pending_parents.push((entry_id, record.parent_frn));
                }
            }
        }

        // SAFETY: o handle é válido, pertence a este escopo e é fechado uma única vez, sem uso posterior.
        unsafe { CloseHandle(handle) };

        // Segunda passada: resolução dos nós pais na hierarquia
        for (entry_id, parent_frn) in pending_parents {
            if let Some(parent_id) = index.id_by_frn(parent_frn) {
                if let Some(entry) = index.entries.get_mut(entry_id as usize) {
                    entry.parent = parent_id;
                }
            }
        }

        self.elapsed_ms = (start.elapsed().as_secs_f64() * 1000.0 * 100.0).round() / 100.0;
        index.volume = self.volume.clone();
        index.journal_id = self.journal_id;
        index.last_usn = self.last_usn;
        Ok(())
    }
}

/// Monitor de alterações em tempo real via `FSCTL_READ_USN_JOURNAL` (Checkpoint C).
pub struct UsnWatcher {
    pub volume: String,
    pub handle: HANDLE,
    pub journal_id: u64,
    pub last_usn: i64,
}

impl UsnWatcher {
    /// Inicia um novo monitor a partir de um volume e USN inicial.
    pub fn new(volume: impl Into<String>, journal_id: u64, start_usn: i64) -> io::Result<Self> {
        let vol_str = volume.into();
        let handle = open_volume(&vol_str, false)?;
        Ok(Self {
            volume: vol_str,
            handle,
            journal_id,
            last_usn: start_usn,
        })
    }

    /// Resolve o tamanho em disco e trata remoção antes da leitura de metadados (CORE-03).
    ///
    /// Retorna:
    /// - `Ok(Some(size))`: arquivo existe e o tamanho foi obtido.
    /// - `Ok(None)`: arquivo foi removido entre a notificação e a leitura de metadados.
    /// - `Err(err)`: erro transitório de E/S.
    pub fn resolve_file_size(
        index: &FileIndex,
        frn: u64,
        parent_frn: u64,
        name: &str,
    ) -> Result<Option<u64>, io::Error> {
        let full_path = if let Some(parent_id) = index.id_by_frn(parent_frn) {
            if let Ok(parent_path) = index.build_path(parent_id) {
                Some(std::path::Path::new(&parent_path).join(name))
            } else {
                None
            }
        } else if let Some(entry_id) = index.id_by_frn(frn) {
            index
                .build_path(entry_id)
                .ok()
                .map(std::path::PathBuf::from)
        } else {
            None
        };

        if let Some(path) = full_path {
            match std::fs::symlink_metadata(&path) {
                Ok(meta) => Ok(Some(meta.len())),
                Err(err) if err.kind() == io::ErrorKind::NotFound => {
                    // Arquivo removido entre a notificação e a leitura de metadados (CORE-03)
                    Ok(None)
                }
                Err(err) => Err(err),
            }
        } else {
            Ok(Some(0))
        }
    }

    /// Lê os eventos mais recentes do USN Journal e os aplica diretamente ao `FileIndex`.
    pub fn poll_changes(&mut self, index: &mut FileIndex) -> Result<Vec<IndexChange>, UsnError> {
        let read_data = ReadUsnJournalDataV0 {
            start_usn: self.last_usn,
            reason_mask: 0xFFFF_FFFF,
            return_only_on_close: 0,
            timeout: 0,
            bytes_to_wait_for: 0,
            usn_journal_id: self.journal_id,
        };

        const BUFFER_SIZE: usize = 64 * 1024;
        let mut buffer = vec![0u8; BUFFER_SIZE];
        let mut bytes_returned = 0u32;

        // SAFETY: os buffers de entrada e saída são locais, vivem além da chamada e têm os tamanhos informados.
        let success = unsafe {
            DeviceIoControl(
                self.handle,
                FSCTL_READ_USN_JOURNAL,
                &read_data as *const _ as *const c_void,
                size_of::<ReadUsnJournalDataV0>() as u32,
                buffer.as_mut_ptr() as *mut c_void,
                BUFFER_SIZE as u32,
                &mut bytes_returned,
                std::ptr::null_mut(),
            )
        };

        if success == 0 {
            let err = io::Error::last_os_error();
            return Err(UsnError::Io(err));
        }

        if bytes_returned < 8 {
            return Ok(Vec::new());
        }

        // Primeiro campo retornado é o próximo USN
        let next_usn = i64::from_le_bytes(buffer[0..8].try_into().unwrap());
        self.last_usn = next_usn;

        let total_bytes = (bytes_returned as usize).min(buffer.len());
        let mut applied_changes = Vec::new();

        for record in parse_usn_records(&buffer[..total_bytes]) {
            let frn = record.frn;
            let parent_frn = record.parent_frn;
            let reason = record.reason;
            let is_dir = record.is_dir;
            let flags = record.flags;
            let unix_secs = record.unix_secs;
            let name = record.name.unwrap_or_default();

            if (reason & USN_REASON_FILE_CREATE) != 0 {
                if is_dir {
                    let change = IndexChange::Insert {
                        frn,
                        parent_frn,
                        name,
                        flags,
                        size: 0,
                        mtime: unix_secs,
                    };
                    let _ = index.apply_change(&change);
                    applied_changes.push(change);
                } else {
                    match Self::resolve_file_size(index, frn, parent_frn, &name) {
                        Ok(Some(size)) => {
                            let change = IndexChange::Insert {
                                frn,
                                parent_frn,
                                name,
                                flags,
                                size,
                                mtime: unix_secs,
                            };
                            let _ = index.apply_change(&change);
                            applied_changes.push(change);
                        }
                        Ok(None) => {
                            // Arquivo removido antes da leitura de metadados (CORE-03)
                            let _ = index.remove_by_frn(frn);
                        }
                        Err(_) => {
                            let change = IndexChange::Insert {
                                frn,
                                parent_frn,
                                name,
                                flags,
                                size: 0,
                                mtime: unix_secs,
                            };
                            let _ = index.apply_change(&change);
                            applied_changes.push(change);
                        }
                    }
                }
            } else if (reason & USN_REASON_FILE_DELETE) != 0 {
                let change = IndexChange::Delete { frn };
                let _ = index.apply_change(&change);
                applied_changes.push(change);
            } else if (reason & USN_REASON_RENAME_NEW_NAME) != 0 {
                let change = IndexChange::Rename {
                    frn,
                    new_parent_frn: parent_frn,
                    new_name: name,
                };
                let _ = index.apply_change(&change);
                applied_changes.push(change);
            } else if (reason
                & (USN_REASON_DATA_OVERWRITE
                    | USN_REASON_DATA_EXTEND
                    | USN_REASON_DATA_TRUNCATION
                    | USN_REASON_BASIC_INFO_CHANGE))
                != 0
            {
                if is_dir {
                    let change = IndexChange::Modify {
                        frn,
                        size: 0,
                        mtime: unix_secs,
                        flags,
                    };
                    let _ = index.apply_change(&change);
                    applied_changes.push(change);
                } else {
                    match Self::resolve_file_size(index, frn, parent_frn, &name) {
                        Ok(Some(size)) => {
                            let change = IndexChange::Modify {
                                frn,
                                size,
                                mtime: unix_secs,
                                flags,
                            };
                            let _ = index.apply_change(&change);
                            applied_changes.push(change);
                        }
                        Ok(None) => {
                            // Arquivo removido entre a notificação e a leitura de metadados (CORE-03)
                            let change = IndexChange::Delete { frn };
                            let _ = index.apply_change(&change);
                            applied_changes.push(change);
                        }
                        Err(_) => {
                            // Preserva o tamanho existente em vez de zerar (CORE-03)
                            let prev_size = index
                                .id_by_frn(frn)
                                .and_then(|id| index.entry(id).ok())
                                .map(|e| e.size)
                                .unwrap_or(0);
                            let change = IndexChange::Modify {
                                frn,
                                size: prev_size,
                                mtime: unix_secs,
                                flags,
                            };
                            let _ = index.apply_change(&change);
                            applied_changes.push(change);
                        }
                    }
                }
            }
        }

        Ok(applied_changes)
    }
}

impl Drop for UsnWatcher {
    fn drop(&mut self) {
        if self.handle != INVALID_HANDLE_VALUE && !self.handle.is_null() {
            // SAFETY: o handle é válido, pertence a este escopo e é fechado uma única vez, sem uso posterior.
            unsafe { CloseHandle(self.handle) };
        }
    }
}
