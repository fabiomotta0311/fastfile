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
use windows_sys::Win32::Storage::FileSystem::{ReadFile, SetFilePointerEx, FILE_BEGIN};
use windows_sys::Win32::System::IO::DeviceIoControl;

use crate::entry::NO_PARENT;
use crate::index::FileIndex;
use crate::ntfs::{
    plan_mft_reads, read_mft_parallel_stats, resolve_mft_layout, VolumeReader, FRN_BASE_MASK,
};

/// Número do registro da raiz do volume na `$MFT`.
const NTFS_ROOT_RECORD: u64 = 5;
use crate::source::{IndexError, IndexSource};
use crate::windows::usn::UsnSource;
use crate::windows::volume::{open_volume, open_volume_unbuffered};

/// Control code Win32 para obter parâmetros de geometria e $MFT do volume NTFS.
pub const FSCTL_GET_NTFS_VOLUME_DATA: u32 = 0x00090064;

/// Assinatura mágica de um registro MFT válido ("FILE").
pub use crate::ntfs::{apply_mft_fixups, MFT_RECORD_MAGIC};

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

/// Fonte do índice baseada na leitura bruta da tabela $MFT (Rota 2).
pub struct MftSource {
    pub volume: String,
    pub elapsed_ms: f64,
    pub fallback_used: bool,
    /// Por que a leitura bruta da `$MFT` foi abandonada em favor do USN (quando `fallback_used`).
    pub fallback_reason: Option<String>,
    pub journal_id: u64,
    pub last_usn: i64,
}

/// Lê até `buf.len()` bytes do volume a partir de `offset` (bytes). Retorna quantos bytes foram lidos.
fn read_at(handle: HANDLE, offset: i64, buf: &mut [u8]) -> io::Result<usize> {
    let mut new_pos = 0i64;
    // SAFETY: `handle` é um handle de volume válido do chamador e `new_pos` é um `i64` local.
    if unsafe { SetFilePointerEx(handle, offset, &mut new_pos, FILE_BEGIN) } == 0 {
        return Err(io::Error::last_os_error());
    }
    let mut bytes_read = 0u32;
    // SAFETY: `buf` é um slice vivo com o tamanho informado (limitado a `u32`); `bytes_read` é local.
    let ok = unsafe {
        ReadFile(
            handle,
            buf.as_mut_ptr(),
            buf.len().min(u32::MAX as usize) as u32,
            &mut bytes_read,
            std::ptr::null_mut(),
        )
    };
    if ok == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(bytes_read as usize)
}

/// Handle de volume de uso exclusivo de um leitor; fecha ao sair do escopo.
struct VolumeHandle {
    handle: HANDLE,
    /// Alinhamento exigido pelas leituras (4096 em I/O sem cache; 1 com cache).
    align: usize,
}

impl VolumeReader for VolumeHandle {
    fn read_at(&mut self, offset: u64, buf: &mut [u8]) -> io::Result<usize> {
        read_at(self.handle, offset as i64, buf)
    }

    fn alignment(&self) -> usize {
        self.align
    }
}

impl Drop for VolumeHandle {
    fn drop(&mut self) {
        // SAFETY: o handle foi aberto por `open_volume`, pertence a esta struct e é fechado uma única vez.
        unsafe { CloseHandle(self.handle) };
    }
}

/// Registros por bloco de leitura (padrão: blocos de 4 MiB). `FASTFILE_MFT_CHUNK_MIB` (1 a 64) ajusta.
fn mft_chunk_records(record_size: usize) -> u64 {
    let mib = std::env::var("FASTFILE_MFT_CHUNK_MIB")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .filter(|n| (1..=64).contains(n))
        .unwrap_or(4);
    ((mib << 20) / record_size.max(1) as u64).max(1)
}

/// Quantidade de leitores paralelos da `$MFT` (padrão: até 4, limitado pelo número de CPUs lógicas).
///
/// Em NVMe real, 4, 8 e 16 leitores deram o mesmo tempo (≈9 s para 6 GB de `$MFT`): mais leitores só
/// aumentam a disputa (e o risco em discos mais lentos).
fn mft_reader_count() -> usize {
    std::env::var("FASTFILE_MFT_READERS")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .filter(|n| (1..=64).contains(n))
        .unwrap_or_else(|| {
            std::thread::available_parallelism()
                .map(|n| n.get())
                .unwrap_or(4)
                .clamp(2, 4)
        })
}

impl MftSource {
    pub fn new(volume: impl Into<String>) -> Self {
        Self {
            volume: volume.into(),
            elapsed_ms: 0.0,
            fallback_used: false,
            fallback_reason: None,
            journal_id: 0,
            last_usn: 0,
        }
    }

    /// Rota de contingência: enumera a MFT inteira pelo USN (sem tamanhos de arquivo).
    fn fallback_to_usn(
        &mut self,
        index: &mut FileIndex,
        start: Instant,
        reason: impl Into<String>,
    ) {
        self.fallback_used = true;
        let reason = reason.into();
        if std::env::var_os("FASTFILE_MFT_TRACE").is_some() {
            eprintln!("[mft] leitura bruta da $MFT abandonada, usando USN: {reason}");
        }
        self.fallback_reason = Some(reason);
        let mut usn = UsnSource::new(&self.volume);
        let _ = usn.populate(index);
        self.elapsed_ms = (start.elapsed().as_secs_f64() * 1000.0 * 100.0).round() / 100.0;
    }

    /// Obtém os parâmetros do volume NTFS.
    ///
    /// # Safety
    /// O ponteiro `handle` deve ser um identificador de arquivo Win32 aberto e válido.
    pub unsafe fn query_ntfs_volume_data(handle: HANDLE) -> io::Result<NtfsVolumeDataBuffer> {
        let mut vol_data = NtfsVolumeDataBuffer::default();
        let mut bytes_returned = 0u32;

        // SAFETY: os buffers de entrada e saída são locais, vivem além da chamada e têm os tamanhos informados.
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
            Err(e) => {
                // Se a leitura direta da partição não for permitida, recorre ao USN (Rota 1) ou Walk
                self.fallback_to_usn(
                    index,
                    start,
                    format!("não foi possível abrir o volume: {e}"),
                );
                return Ok(());
            }
        };

        // SAFETY: `handle` foi aberto por `open_volume` e permanece válido durante a chamada.
        if let Ok(jd) = unsafe { UsnSource::query_journal(handle) } {
            self.journal_id = jd.usn_journal_id;
            self.last_usn = jd.next_usn;
        }

        // SAFETY: `handle` foi aberto por `open_volume` e permanece válido durante a chamada.
        let vol_data = match unsafe { Self::query_ntfs_volume_data(handle) } {
            Ok(vd) => vd,
            Err(e) => {
                // SAFETY: o handle é válido, pertence a este escopo e é fechado uma única vez, sem uso posterior.
                unsafe { CloseHandle(handle) };
                self.fallback_to_usn(
                    index,
                    start,
                    format!("FSCTL_GET_NTFS_VOLUME_DATA falhou: {e}"),
                );
                return Ok(());
            }
        };

        let record_size = if vol_data.bytes_per_file_record_segment > 0 {
            vol_data.bytes_per_file_record_segment as usize
        } else {
            1024
        };

        let bytes_per_cluster = u64::from(vol_data.bytes_per_cluster);
        let mft_start_offset = vol_data.mft_start_lcn as i64 * vol_data.bytes_per_cluster as i64;

        // O registro 0 ($MFT) descreve onde a tabela inteira está (run list do seu $DATA). Ler só a
        // partir do primeiro cluster deixaria de fora todos os outros extents de uma MFT fragmentada.
        let layout = {
            let mut rec0 = vec![0u8; (bytes_per_cluster as usize).max(record_size)];
            match read_at(handle, mft_start_offset, &mut rec0) {
                Ok(n) if n >= record_size => resolve_mft_layout(
                    &mut rec0[..record_size],
                    record_size,
                    bytes_per_cluster,
                    |off, buf| read_at(handle, off as i64, buf),
                )
                .map_err(|e| format!("layout da $MFT recusado: {e:?}")),
                Ok(n) => Err(format!("leitura do registro 0 curta ({n} bytes)")),
                Err(e) => Err(format!("leitura do registro 0 falhou: {e}")),
            }
        };
        let layout = match layout {
            Ok(l) => l,
            Err(reason) => {
                // Layout ilegível ou incompleto: o USN enumera a MFT inteira (sem tamanhos).
                // SAFETY: o handle é válido, pertence a este escopo e é fechado uma única vez, sem uso posterior.
                unsafe { CloseHandle(handle) };
                self.fallback_to_usn(index, start, reason);
                return Ok(());
            }
        };
        // Vários leitores em paralelo (cada um com o seu handle): um SSD NVMe só atinge a vazão total
        // com fila de I/O > 1. `FASTFILE_MFT_READERS=1` força leitura sequencial (útil em HDD).
        let Some(plan) = plan_mft_reads(
            &layout,
            record_size as u64,
            bytes_per_cluster,
            mft_chunk_records(record_size),
        ) else {
            // SAFETY: o handle é válido, pertence a este escopo e é fechado uma única vez, sem uso posterior.
            unsafe { CloseHandle(handle) };
            self.fallback_to_usn(index, start, "plano de leitura da $MFT inválido");
            return Ok(());
        };
        // O handle principal não é mais necessário: cada leitor abre o seu.
        // SAFETY: o handle é válido, pertence a este escopo e é fechado uma única vez, sem uso posterior.
        unsafe { CloseHandle(handle) };

        let total_records = layout.data_size / record_size as u64;
        if std::env::var_os("FASTFILE_MFT_TRACE").is_some() {
            eprintln!(
                "[mft] layout ok: {} extents, {} registros ({:.0} MiB)",
                layout.runs.len(),
                total_records,
                layout.data_size as f64 / (1024.0 * 1024.0)
            );
        }
        let root_name = self
            .volume
            .trim_start_matches(r"\\.\")
            .trim_end_matches(['\\', '/'])
            .to_string();
        let mut pending_parents: Vec<(u32, u64)> =
            Vec::with_capacity(total_records.min(8_000_000) as usize);
        let volume = self.volume.clone();
        let readers = mft_reader_count();
        let trace = std::env::var_os("FASTFILE_MFT_TRACE").is_some();

        // I/O sem cache do sistema (padrão): o cache fatia e copia leituras grandes e sequenciais, e
        // limitava a vazão a ~240 MB/s num NVMe. `FASTFILE_MFT_BUFFERED=1` volta ao I/O com cache.
        let mut unbuffered = std::env::var("FASTFILE_MFT_BUFFERED").map_or(true, |v| v != "1");
        let complete = loop {
            let read_start = Instant::now();
            let (ok, stats) = read_mft_parallel_stats(
                &plan,
                record_size,
                bytes_per_cluster as usize,
                readers,
                || {
                    if unbuffered {
                        open_volume_unbuffered(&volume).map(|handle| VolumeHandle {
                            handle,
                            align: 4096,
                        })
                    } else {
                        open_volume(&volume, false).map(|handle| VolumeHandle { handle, align: 1 })
                    }
                },
                |batch| {
                    for p in batch {
                        // O registro 5 é a raiz e se chama "." no NTFS: usa o nome do volume ("C:"), como
                        // as demais origens, para que os caminhos comecem em "C:\...".
                        let name = if p.frn & FRN_BASE_MASK == NTFS_ROOT_RECORD {
                            root_name.as_str()
                        } else {
                            p.record.name.as_str()
                        };
                        if let Ok(id) = index.add_entry(
                            p.frn,
                            NO_PARENT,
                            name,
                            p.record.flags,
                            p.record.size,
                            p.record.unix_mtime,
                        ) {
                            pending_parents.push((id, p.record.parent_frn));
                        }
                    }
                },
            );
            if trace {
                let secs = read_start.elapsed().as_secs_f64();
                let mib = layout.data_size as f64 / (1024.0 * 1024.0);
                eprintln!(
                    "[mft] modo={} leitores={} blocos={} dados={:.0} MiB leitura+parse={:.2}s ({:.0} MiB/s) ok={}",
                    if unbuffered { "sem-cache" } else { "com-cache" },
                    readers,
                    plan.len(),
                    mib,
                    secs,
                    mib / secs.max(1e-9),
                    ok
                );
                let s = |n: u64| n as f64 / 1e9;
                eprintln!(
                    "[mft] tempos (somas entre threads): leitores: io={:.2}s parse={:.2}s bloqueados={:.2}s | consumidor: indice={:.2}s espera={:.2}s",
                    s(stats.read_nanos),
                    s(stats.parse_nanos),
                    s(stats.blocked_send_nanos),
                    s(stats.sink_nanos),
                    s(stats.recv_wait_nanos)
                );
            }
            if ok || !unbuffered {
                break ok;
            }
            // O I/O sem cache pode não ser aceito (driver, alinhamento): tenta de novo com cache
            // antes de abandonar a MFT.
            *index = FileIndex::new();
            pending_parents.clear();
            unbuffered = false;
        };

        if !complete {
            // Leitura incompleta: não deixar um índice parcial silencioso. Recomeça pelo USN.
            *index = FileIndex::new();
            self.fallback_to_usn(index, start, "leitura de algum bloco da $MFT falhou");
            return Ok(());
        }

        // Resolução dos nós pais na hierarquia
        for (entry_id, parent_frn) in pending_parents {
            // A raiz do NTFS (registro 5) tem a si mesma como pai: deixa-a sem pai, senão todo caminho
            // que sobe até ela entra em ciclo.
            if let Some(parent_id) = index.id_by_frn(parent_frn).filter(|&p| p != entry_id) {
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
