//! Leitura direta e parsing da tabela MFT do NTFS (Rota 2 e Checkpoints A, D, E).
//!
//! Permite extrair nomes, hierarquia de diretórios e tamanhos reais diretamente
//! dos registros de 1024 bytes da `$MFT` sem depender de chamadas individuais de stat (§2.1).

#![cfg(windows)]

use std::fmt;
use std::io;
use std::mem::size_of;
use std::os::raw::c_void;
use std::time::Instant;

use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
use windows_sys::Win32::Storage::FileSystem::{
    ReadFile, SetFilePointerEx, FILE_ATTRIBUTE_HIDDEN, FILE_ATTRIBUTE_READONLY,
    FILE_ATTRIBUTE_SYSTEM, FILE_BEGIN,
};
use windows_sys::Win32::System::IO::DeviceIoControl;

use crate::entry::{FLAG_DIRECTORY, FLAG_FILE, FLAG_HIDDEN, FLAG_READONLY, FLAG_SYSTEM, NO_PARENT};
use crate::index::FileIndex;
use crate::source::{IndexError, IndexSource};
use crate::windows::usn::{UsnSource, FRN_BASE_MASK};
use crate::windows::volume::open_volume;

/// Control code Win32 para obter parâmetros de geometria e $MFT do volume NTFS.
pub const FSCTL_GET_NTFS_VOLUME_DATA: u32 = 0x00090064;

/// Assinatura mágica de um registro MFT válido ("FILE").
pub const MFT_RECORD_MAGIC: &[u8; 4] = b"FILE";

/// Estrutura de dados retornada por `FSCTL_GET_NTFS_VOLUME_DATA`.
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct NtfsVolumeDataBuffer {
    pub volume_serial_number: i64,
    pub number_sectors: i64,
    pub total_clusters: i64,
    pub free_clusters: i64,
    pub total_reserved: i64,
    pub bytes_per_sector: u32,
    pub bytes_per_cluster: u32,
    pub bytes_per_file_record_segment: u32,
    pub clusters_per_file_record_segment: u32,
    pub mft_valid_data_length: i64,
    pub mft_start_lcn: i64,
    pub mft2_start_lcn: i64,
    pub mft_zone_start: i64,
    pub mft_zone_end: i64,
}

/// Cabeçalho padrão de um registro MFT de 1024 bytes (`FILE_RECORD_SEGMENT_HEADER`).
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct FileRecordHeader {
    pub magic: [u8; 4],
    pub update_sequence_offset: u16,
    pub update_sequence_size: u16,
    pub lsn: u64,
    pub sequence_number: u16,
    pub hard_link_count: u16,
    pub first_attribute_offset: u16,
    pub flags: u16, // 0x0001 = Em uso, 0x0002 = Diretório
    pub bytes_in_use: u32,
    pub bytes_allocated: u32,
    pub base_file_record_segment: u64,
    pub next_attribute_instance: u16,
    pub segment_number_high: u16,
    pub base_file_record_segment_number: u32,
}

/// Erros originados durante a leitura da MFT bruta.
#[derive(Debug)]
pub enum MftError {
    Io(io::Error),
    Index(IndexError),
    InvalidMagic,
    CorruptedRecord,
}

impl fmt::Display for MftError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(e) => write!(f, "erro de E/S na leitura da MFT: {e}"),
            Self::Index(e) => write!(f, "erro ao inserir no índice: {e}"),
            Self::InvalidMagic => write!(f, "assinatura mágica inválida de registro MFT"),
            Self::CorruptedRecord => write!(f, "registro MFT corrompido ou fixup inválido"),
        }
    }
}

impl std::error::Error for MftError {}

impl From<io::Error> for MftError {
    fn from(e: io::Error) -> Self {
        Self::Io(e)
    }
}

impl From<IndexError> for MftError {
    fn from(e: IndexError) -> Self {
        Self::Index(e)
    }
}

/// Aplica o array de fixup em um buffer de registro MFT (geralmente 1024 bytes / 2 setores).
pub fn apply_mft_fixups(record: &mut [u8], record_size: usize) -> bool {
    if record.len() < size_of::<FileRecordHeader>() || record_size < 1024 {
        return false;
    }

    if &record[0..4] != MFT_RECORD_MAGIC {
        return false;
    }

    let us_offset = u16::from_le_bytes(record[4..6].try_into().unwrap()) as usize;
    let us_count = u16::from_le_bytes(record[6..8].try_into().unwrap()) as usize;

    if us_count == 0 || us_offset + us_count * 2 > record.len() {
        return false;
    }

    let signature = u16::from_le_bytes(record[us_offset..us_offset + 2].try_into().unwrap());
    let sector_size = 512usize;
    let sectors_count = record_size / sector_size;

    if us_count - 1 < sectors_count {
        return false;
    }

    for i in 0..sectors_count {
        let sector_end = (i + 1) * sector_size - 2;
        if sector_end + 2 > record.len() {
            return false;
        }

        let current_tag =
            u16::from_le_bytes(record[sector_end..sector_end + 2].try_into().unwrap());
        if current_tag != signature {
            return false;
        }

        let fixup_entry_off = us_offset + (i + 1) * 2;
        let original_bytes = [record[fixup_entry_off], record[fixup_entry_off + 1]];
        record[sector_end] = original_bytes[0];
        record[sector_end + 1] = original_bytes[1];
    }

    true
}

/// Fonte do índice baseada na leitura bruta da tabela $MFT (Rota 2).
pub struct MftSource {
    pub volume: String,
    pub elapsed_ms: f64,
    pub fallback_used: bool,
    pub journal_id: u64,
    pub last_usn: i64,
}

impl MftSource {
    pub fn new(volume: impl Into<String>) -> Self {
        Self {
            volume: volume.into(),
            elapsed_ms: 0.0,
            fallback_used: false,
            journal_id: 0,
            last_usn: 0,
        }
    }

    /// Obtém os parâmetros do volume NTFS.
    ///
    /// # Safety
    /// O ponteiro `handle` deve ser um identificador de arquivo Win32 aberto e válido.
    pub unsafe fn query_ntfs_volume_data(handle: HANDLE) -> io::Result<NtfsVolumeDataBuffer> {
        let mut vol_data = NtfsVolumeDataBuffer::default();
        let mut bytes_returned = 0u32;

        let success = unsafe {
            DeviceIoControl(
                handle,
                FSCTL_GET_NTFS_VOLUME_DATA,
                std::ptr::null(),
                0,
                &mut vol_data as *mut _ as *mut c_void,
                size_of::<NtfsVolumeDataBuffer>() as u32,
                &mut bytes_returned,
                std::ptr::null_mut(),
            )
        };

        if success != 0 {
            Ok(vol_data)
        } else {
            Err(io::Error::last_os_error())
        }
    }
}

impl IndexSource for MftSource {
    type Error = MftError;

    fn populate(&mut self, index: &mut FileIndex) -> Result<(), Self::Error> {
        let start = Instant::now();

        let handle_res = open_volume(&self.volume, false);
        let handle = match handle_res {
            Ok(h) => h,
            Err(_) => {
                // Se a leitura direta da partição não for permitida, recorre ao USN (Rota 1) ou Walk
                self.fallback_used = true;
                let mut usn = UsnSource::new(&self.volume);
                let _ = usn.populate(index);
                self.elapsed_ms = (start.elapsed().as_secs_f64() * 1000.0 * 100.0).round() / 100.0;
                return Ok(());
            }
        };

        if let Ok(jd) = unsafe { UsnSource::query_journal(handle) } {
            self.journal_id = jd.usn_journal_id;
            self.last_usn = jd.next_usn;
        }

        let vol_data = match unsafe { Self::query_ntfs_volume_data(handle) } {
            Ok(vd) => vd,
            Err(_) => {
                unsafe { CloseHandle(handle) };
                self.fallback_used = true;
                let mut usn = UsnSource::new(&self.volume);
                let _ = usn.populate(index);
                self.elapsed_ms = (start.elapsed().as_secs_f64() * 1000.0 * 100.0).round() / 100.0;
                return Ok(());
            }
        };

        let record_size = if vol_data.bytes_per_file_record_segment > 0 {
            vol_data.bytes_per_file_record_segment as usize
        } else {
            1024
        };

        // Calcula a posição física do início da $MFT (LCN * bytes_per_cluster)
        let mft_start_offset = vol_data.mft_start_lcn as i64 * vol_data.bytes_per_cluster as i64;

        let mut new_pos = 0i64;
        let seek_success =
            unsafe { SetFilePointerEx(handle, mft_start_offset, &mut new_pos, FILE_BEGIN) };

        if seek_success == 0 {
            unsafe { CloseHandle(handle) };
            self.fallback_used = true;
            let mut usn = UsnSource::new(&self.volume);
            let _ = usn.populate(index);
            self.elapsed_ms = (start.elapsed().as_secs_f64() * 1000.0 * 100.0).round() / 100.0;
            return Ok(());
        }

        // Lê bloco de registros da MFT (ex.: 256 KB = 256 registros de 1 KB)
        const CHUNK_RECORDS: usize = 256;
        let chunk_size = CHUNK_RECORDS * record_size;
        let mut buffer = vec![0u8; chunk_size];
        let mut record_number = 0u64;

        // Limite máximo de leitura para o primeiro lote de clusters contíguos da MFT
        let max_records_to_read = 500_000usize;
        let mut pending_parents: Vec<(u32, u64)> = Vec::with_capacity(50_000);

        while (record_number as usize) < max_records_to_read {
            let mut bytes_read = 0u32;
            let read_ok = unsafe {
                ReadFile(
                    handle,
                    buffer.as_mut_ptr(),
                    chunk_size as u32,
                    &mut bytes_read,
                    std::ptr::null_mut(),
                )
            };

            if read_ok == 0 || bytes_read == 0 {
                break;
            }

            let records_in_chunk = (bytes_read as usize) / record_size;
            if records_in_chunk == 0 {
                break;
            }

            for r_idx in 0..records_in_chunk {
                let rec_off = r_idx * record_size;
                let rec_slice = &mut buffer[rec_off..rec_off + record_size];

                if &rec_slice[0..4] != MFT_RECORD_MAGIC {
                    record_number += 1;
                    continue;
                }

                if !apply_mft_fixups(rec_slice, record_size) {
                    record_number += 1;
                    continue;
                }

                let flags = u16::from_le_bytes(rec_slice[22..24].try_into().unwrap());
                let in_use = (flags & 0x0001) != 0;
                let is_dir = (flags & 0x0002) != 0;

                if !in_use {
                    record_number += 1;
                    continue;
                }

                let first_attr_off =
                    u16::from_le_bytes(rec_slice[20..22].try_into().unwrap()) as usize;
                let mut attr_off = first_attr_off;
                let bytes_in_use =
                    u32::from_le_bytes(rec_slice[24..28].try_into().unwrap()) as usize;
                let limit = bytes_in_use.min(record_size);

                let mut file_name: Option<String> = None;
                let mut parent_frn: u64 = 0;
                let mut data_size: u64 = 0;
                let mut unix_mtime: i64 = 0;
                let mut file_attrs: u32 = 0;

                while attr_off + 8 <= limit {
                    let attr_type =
                        u32::from_le_bytes(rec_slice[attr_off..attr_off + 4].try_into().unwrap());
                    if attr_type == 0xFFFF_FFFF {
                        break;
                    }

                    let attr_len = u32::from_le_bytes(
                        rec_slice[attr_off + 4..attr_off + 8].try_into().unwrap(),
                    ) as usize;
                    if attr_len == 0 || attr_off + attr_len > limit {
                        break;
                    }

                    let non_resident = rec_slice[attr_off + 8] != 0;

                    if attr_type == 0x30 && !non_resident {
                        // Atributo $FILE_NAME (residente)
                        let val_off = u16::from_le_bytes(
                            rec_slice[attr_off + 20..attr_off + 22].try_into().unwrap(),
                        ) as usize;
                        let val_len = u32::from_le_bytes(
                            rec_slice[attr_off + 16..attr_off + 20].try_into().unwrap(),
                        ) as usize;
                        let fn_start = attr_off + val_off;

                        if fn_start + val_len <= attr_off + attr_len && val_len >= 66 {
                            let p_frn = u64::from_le_bytes(
                                rec_slice[fn_start..fn_start + 8].try_into().unwrap(),
                            ) & FRN_BASE_MASK;
                            let alt_mtime = i64::from_le_bytes(
                                rec_slice[fn_start + 16..fn_start + 24].try_into().unwrap(),
                            );
                            let r_size = u64::from_le_bytes(
                                rec_slice[fn_start + 48..fn_start + 56].try_into().unwrap(),
                            );
                            let f_attrs = u32::from_le_bytes(
                                rec_slice[fn_start + 56..fn_start + 60].try_into().unwrap(),
                            );
                            let name_len = rec_slice[fn_start + 64] as usize;
                            let namespace = rec_slice[fn_start + 65];

                            // Preferência por nomes Win32 ou Win32/DOS sobre apenas DOS 8.3
                            if (namespace != 2 || file_name.is_none())
                                && fn_start + 66 + name_len * 2 <= attr_off + attr_len
                            {
                                let u16_slice = unsafe {
                                    std::slice::from_raw_parts(
                                        rec_slice[fn_start + 66..].as_ptr() as *const u16,
                                        name_len,
                                    )
                                };
                                file_name = Some(String::from_utf16_lossy(u16_slice));
                                parent_frn = p_frn;
                                data_size = r_size;
                                file_attrs = f_attrs;
                                unix_mtime = (alt_mtime - 116_444_736_000_000_000) / 10_000_000;
                            }
                        }
                    } else if attr_type == 0x80 {
                        // Atributo $DATA (tamanho real dos dados)
                        if non_resident && attr_off + 56 <= attr_off + attr_len {
                            let real_size = u64::from_le_bytes(
                                rec_slice[attr_off + 48..attr_off + 56].try_into().unwrap(),
                            );
                            data_size = real_size;
                        } else if !non_resident && attr_off + 24 <= attr_off + attr_len {
                            let val_len = u32::from_le_bytes(
                                rec_slice[attr_off + 16..attr_off + 20].try_into().unwrap(),
                            ) as u64;
                            data_size = val_len;
                        }
                    }

                    attr_off += attr_len;
                }

                if let Some(name) = file_name {
                    let frn = record_number & FRN_BASE_MASK;
                    let mut flags = if is_dir { FLAG_DIRECTORY } else { FLAG_FILE };
                    if (file_attrs & FILE_ATTRIBUTE_READONLY) != 0 {
                        flags |= FLAG_READONLY;
                    }
                    if (file_attrs & FILE_ATTRIBUTE_HIDDEN) != 0 {
                        flags |= FLAG_HIDDEN;
                    }
                    if (file_attrs & FILE_ATTRIBUTE_SYSTEM) != 0 {
                        flags |= FLAG_SYSTEM;
                    }

                    if let Ok(id) =
                        index.add_entry(frn, NO_PARENT, &name, flags, data_size, unix_mtime)
                    {
                        pending_parents.push((id, parent_frn));
                    }
                }

                record_number += 1;
            }
        }

        unsafe { CloseHandle(handle) };

        // Resolução dos nós pais na hierarquia
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
