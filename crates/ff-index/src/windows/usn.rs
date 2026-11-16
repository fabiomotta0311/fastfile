//! Leitura e acompanhamento do NTFS pelo USN Journal (Rota 1 e Checkpoints A, C, E).

#![cfg(windows)]

use std::fmt;
use std::io;
use std::mem::size_of;
use std::os::raw::c_void;
use std::time::Instant;

use windows_sys::Win32::Foundation::{CloseHandle, ERROR_HANDLE_EOF, HANDLE, INVALID_HANDLE_VALUE};
use windows_sys::Win32::Storage::FileSystem::{
    FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_HIDDEN, FILE_ATTRIBUTE_READONLY, FILE_ATTRIBUTE_SYSTEM,
};
use windows_sys::Win32::System::IO::DeviceIoControl;

use crate::entry::{FLAG_DIRECTORY, FLAG_FILE, FLAG_HIDDEN, FLAG_READONLY, FLAG_SYSTEM, NO_PARENT};
use crate::index::FileIndex;
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

/// Máscara para extrair o File Reference Number de 48 bits descartando o sequence number de 16 bits.
pub const FRN_BASE_MASK: u64 = 0x0000_FFFF_FFFF_FFFF;

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

        let journal_data = match unsafe { Self::query_journal(handle) } {
            Ok(jd) => jd,
            Err(_) => {
                unsafe { CloseHandle(handle) };
                return Ok(false);
            }
        };

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

        let mut watcher = match UsnWatcher::new(volume, index.journal_id, index.last_usn) {
            Ok(w) => w,
            Err(_) => return Ok(false),
        };

        let _ = watcher.poll_changes(index)?;
        index.last_usn = watcher.last_usn;

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

        let journal_res = unsafe { Self::query_journal(handle) };
        let journal_data = match journal_res {
            Ok(jd) => jd,
            Err(err) => {
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
                unsafe { CloseHandle(handle) };
                return Err(UsnError::Io(err));
            }

            if bytes_returned < 8 {
                break;
            }

            // Os primeiros 8 bytes contêm o próximo FileReferenceNumber a ser consultado
            let next_frn = u64::from_le_bytes(buffer[0..8].try_into().unwrap());
            enum_data.start_file_reference_number = next_frn;

            let mut offset = 8usize;
            let total_bytes = bytes_returned as usize;

            while offset + size_of::<UsnRecordV2>() <= total_bytes {
                let record = unsafe { &*(buffer[offset..].as_ptr() as *const UsnRecordV2) };

                if record.record_length == 0 {
                    break;
                }

                let record_len = record.record_length as usize;
                if offset + record_len > total_bytes {
                    break;
                }

                let frn = record.file_reference_number & FRN_BASE_MASK;
                let parent_frn = record.parent_file_reference_number & FRN_BASE_MASK;

                // Extração do nome em UTF-16
                let name_off = offset + record.file_name_offset as usize;
                let name_len_bytes = record.file_name_length as usize;

                if name_off + name_len_bytes <= offset + record_len {
                    let u16_slice = unsafe {
                        std::slice::from_raw_parts(
                            buffer[name_off..].as_ptr() as *const u16,
                            name_len_bytes / 2,
                        )
                    };
                    let name = String::from_utf16_lossy(u16_slice);

                    let is_dir = (record.file_attributes & FILE_ATTRIBUTE_DIRECTORY) != 0;
                    let mut flags = if is_dir { FLAG_DIRECTORY } else { FLAG_FILE };
                    if (record.file_attributes & FILE_ATTRIBUTE_READONLY) != 0 {
                        flags |= FLAG_READONLY;
                    }
                    if (record.file_attributes & FILE_ATTRIBUTE_HIDDEN) != 0 {
                        flags |= FLAG_HIDDEN;
                    }
                    if (record.file_attributes & FILE_ATTRIBUTE_SYSTEM) != 0 {
                        flags |= FLAG_SYSTEM;
                    }

                    // No ENUM_USN_DATA, o timestamp de modificação vem no record (FILETIME 100ns desde 1601)
                    // Convertemos para timestamp Unix (segundos desde 1970)
                    let unix_secs = (record.time_stamp - 116_444_736_000_000_000) / 10_000_000;

                    // Adiciona entrada com pai temporário NO_PARENT e registra para resolução posterior
                    if let Ok(entry_id) =
                        index.add_entry(frn, NO_PARENT, &name, flags, 0, unix_secs)
                    {
                        pending_parents.push((entry_id, parent_frn));
                    }
                }

                offset += record_len;
            }
        }

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

        let mut offset = 8usize;
        let total_bytes = bytes_returned as usize;
        let mut applied_changes = Vec::new();

        while offset + size_of::<UsnRecordV2>() <= total_bytes {
            let record = unsafe { &*(buffer[offset..].as_ptr() as *const UsnRecordV2) };

            if record.record_length == 0 {
                break;
            }

            let record_len = record.record_length as usize;
            if offset + record_len > total_bytes {
                break;
            }

            let frn = record.file_reference_number & FRN_BASE_MASK;
            let parent_frn = record.parent_file_reference_number & FRN_BASE_MASK;
            let reason = record.reason;

            let name_off = offset + record.file_name_offset as usize;
            let name_len_bytes = record.file_name_length as usize;

            let name = if name_off + name_len_bytes <= offset + record_len {
                let u16_slice = unsafe {
                    std::slice::from_raw_parts(
                        buffer[name_off..].as_ptr() as *const u16,
                        name_len_bytes / 2,
                    )
                };
                String::from_utf16_lossy(u16_slice)
            } else {
                String::new()
            };

            if (reason & USN_REASON_FILE_CREATE) != 0 {
                let is_dir = (record.file_attributes & FILE_ATTRIBUTE_DIRECTORY) != 0;
                let mut flags = if is_dir { FLAG_DIRECTORY } else { FLAG_FILE };
                if (record.file_attributes & FILE_ATTRIBUTE_READONLY) != 0 {
                    flags |= FLAG_READONLY;
                }
                let unix_secs = (record.time_stamp - 116_444_736_000_000_000) / 10_000_000;

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
                let unix_secs = (record.time_stamp - 116_444_736_000_000_000) / 10_000_000;
                let is_dir = (record.file_attributes & FILE_ATTRIBUTE_DIRECTORY) != 0;
                let mut flags = if is_dir { FLAG_DIRECTORY } else { FLAG_FILE };
                if (record.file_attributes & FILE_ATTRIBUTE_READONLY) != 0 {
                    flags |= FLAG_READONLY;
                }

                let change = IndexChange::Modify {
                    frn,
                    size: 0,
                    mtime: unix_secs,
                    flags,
                };
                let _ = index.apply_change(&change);
                applied_changes.push(change);
            }

            offset += record_len;
        }

        Ok(applied_changes)
    }
}

impl Drop for UsnWatcher {
    fn drop(&mut self) {
        if self.handle != INVALID_HANDLE_VALUE && !self.handle.is_null() {
            unsafe { CloseHandle(self.handle) };
        }
    }
}
