//! Parsers NTFS puros (sem Win32): buffers de USN e registros da MFT.
//!
//! A leitura bruta do volume é específica do Windows, mas *interpretar* os bytes não precisa ser: estes
//! parsers recebem `&[u8]`, nunca entram em pânico com entrada corrompida ou hostil (toda leitura é
//! limitada) e compilam/testam em Linux (CI headless) e sob fuzz.

use crate::entry::{FLAG_COMPRESSED, FLAG_OFFLINE, FLAG_REPARSE, FLAG_SPARSE};
use crate::entry::{FLAG_DIRECTORY, FLAG_FILE, FLAG_HIDDEN, FLAG_READONLY, FLAG_SYSTEM};

/// Máscara para extrair o File Reference Number de 48 bits, descartando o sequence number de 16 bits.
pub const FRN_BASE_MASK: u64 = 0x0000_FFFF_FFFF_FFFF;

/// Assinatura de um registro de arquivo da MFT.
pub const MFT_RECORD_MAGIC: &[u8; 4] = b"FILE";

/// Tamanho fixo do cabeçalho de um `USN_RECORD_V2` (sem o nome).
pub const USN_RECORD_V2_HEADER_LEN: usize = 60;

/// Diferença entre 1601-01-01 (FILETIME) e 1970-01-01 em intervalos de 100 ns.
const FILETIME_UNIX_EPOCH_DIFF: i64 = 116_444_736_000_000_000;

const ATTR_READONLY: u32 = 0x0001;
const ATTR_HIDDEN: u32 = 0x0002;
const ATTR_SYSTEM: u32 = 0x0004;
const ATTR_DIRECTORY: u32 = 0x0010;

/// Converte um FILETIME (100 ns desde 1601) em segundos Unix, sem estourar com valores extremos.
#[must_use]
pub fn filetime_to_unix_secs(filetime: i64) -> i64 {
    filetime.saturating_sub(FILETIME_UNIX_EPOCH_DIFF) / 10_000_000
}

/// Decodifica UTF-16 little-endian de bytes brutos (sem exigir alinhamento a `u16`).
#[must_use]
pub fn utf16le_lossy(bytes: &[u8]) -> String {
    let (pairs, _) = bytes.as_chunks::<2>();
    let units: Vec<u16> = pairs.iter().map(|c| u16::from_le_bytes(*c)).collect();
    String::from_utf16_lossy(&units)
}

fn rd_u16(b: &[u8], o: usize) -> Option<u16> {
    Some(u16::from_le_bytes(
        *b.get(o..o.checked_add(2)?)?.first_chunk()?,
    ))
}

fn rd_u32(b: &[u8], o: usize) -> Option<u32> {
    Some(u32::from_le_bytes(
        *b.get(o..o.checked_add(4)?)?.first_chunk()?,
    ))
}

fn rd_u64(b: &[u8], o: usize) -> Option<u64> {
    Some(u64::from_le_bytes(
        *b.get(o..o.checked_add(8)?)?.first_chunk()?,
    ))
}

fn rd_i64(b: &[u8], o: usize) -> Option<i64> {
    rd_u64(b, o).map(|v| v as i64)
}

/// Flags do índice a partir dos atributos de arquivo do Windows.
#[must_use]
pub fn attrs_to_flags(attrs: u32) -> u16 {
    let mut flags = if attrs & ATTR_DIRECTORY != 0 {
        FLAG_DIRECTORY
    } else {
        FLAG_FILE
    };
    if attrs & ATTR_READONLY != 0 {
        flags |= FLAG_READONLY;
    }
    if attrs & ATTR_HIDDEN != 0 {
        flags |= FLAG_HIDDEN;
    }
    if attrs & ATTR_SYSTEM != 0 {
        flags |= FLAG_SYSTEM;
    }
    flags
}

/// Registro USN V2 já interpretado.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UsnRecord {
    pub frn: u64,
    pub parent_frn: u64,
    pub reason: u32,
    pub file_attributes: u32,
    /// Segundos Unix da última modificação.
    pub unix_secs: i64,
    pub is_dir: bool,
    pub flags: u16,
    /// `None` se os limites do nome dentro do registro forem inválidos.
    pub name: Option<String>,
}

/// Interpreta um buffer devolvido por `FSCTL_ENUM_USN_DATA` / `FSCTL_READ_USN_JOURNAL`.
///
/// `buf` contém exatamente os bytes retornados: 8 bytes de cabeçalho (próximo FRN/USN, lidos pelo
/// chamador) seguidos de registros V2 consecutivos. Para ao primeiro registro inconsistente; registros
/// de outra versão são ignorados.
#[must_use]
pub fn parse_usn_records(buf: &[u8]) -> Vec<UsnRecord> {
    let mut out = Vec::new();
    let total = buf.len();
    let mut offset = 8usize;

    while offset
        .checked_add(USN_RECORD_V2_HEADER_LEN)
        .is_some_and(|end| end <= total)
    {
        let Some(record_length) = rd_u32(buf, offset).map(|v| v as usize) else {
            break;
        };
        if record_length < USN_RECORD_V2_HEADER_LEN || record_length > total - offset {
            break;
        }
        let rec = &buf[offset..offset + record_length];

        if rd_u16(rec, 4) == Some(2) {
            // Os `unwrap_or` abaixo são inalcançáveis (o cabeçalho de 60 bytes foi verificado).
            let file_attributes = rd_u32(rec, 52).unwrap_or(0);
            let name_len = rd_u16(rec, 56).unwrap_or(0) as usize;
            let name_off = rd_u16(rec, 58).unwrap_or(0) as usize;
            let name = (name_off >= USN_RECORD_V2_HEADER_LEN
                && name_off + name_len <= record_length)
                .then(|| utf16le_lossy(&rec[name_off..name_off + name_len]));

            out.push(UsnRecord {
                frn: rd_u64(rec, 8).unwrap_or(0) & FRN_BASE_MASK,
                parent_frn: rd_u64(rec, 16).unwrap_or(0) & FRN_BASE_MASK,
                reason: rd_u32(rec, 40).unwrap_or(0),
                file_attributes,
                unix_secs: filetime_to_unix_secs(rd_i64(rec, 32).unwrap_or(0)),
                is_dir: file_attributes & ATTR_DIRECTORY != 0,
                flags: attrs_to_flags(file_attributes),
                name,
            });
        }
        offset += record_length;
    }
    out
}

/// Aplica o array de fixup em um buffer de registro MFT (geralmente 1024 bytes / 2 setores).
pub fn apply_mft_fixups(record: &mut [u8], record_size: usize) -> bool {
    if record.len() < 48 || record_size < 1024 {
        return false;
    }
    if &record[0..4] != MFT_RECORD_MAGIC {
        return false;
    }

    let (Some(us_offset), Some(us_count)) = (rd_u16(record, 4), rd_u16(record, 6)) else {
        return false;
    };
    let (us_offset, us_count) = (us_offset as usize, us_count as usize);

    if us_count == 0 || us_offset + us_count * 2 > record.len() {
        return false;
    }

    let Some(signature) = rd_u16(record, us_offset) else {
        return false;
    };
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
        if rd_u16(record, sector_end) != Some(signature) {
            return false;
        }
        let fixup_entry_off = us_offset + (i + 1) * 2;
        let (Some(&b0), Some(&b1)) = (record.get(fixup_entry_off), record.get(fixup_entry_off + 1))
        else {
            return false;
        };
        record[sector_end] = b0;
        record[sector_end + 1] = b1;
    }
    true
}

/// Registro de arquivo da MFT em uso, com o nome preferido ($FILE_NAME) e o tamanho ($DATA).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MftRecord {
    pub is_dir: bool,
    pub name: String,
    pub parent_frn: u64,
    pub size: u64,
    pub unix_mtime: i64,
    pub file_attrs: u32,
    pub flags: u16,
}

/// Interpreta um registro bruto da MFT (aplica o fixup, que modifica `rec`).
///
/// Retorna `None` para registros sem assinatura, com fixup inválido, fora de uso ou sem nome.
#[must_use]
pub fn parse_mft_record(rec: &mut [u8], record_size: usize) -> Option<MftRecord> {
    if rec.len() < record_size || !apply_mft_fixups(rec, record_size) {
        return None;
    }
    let rec: &[u8] = rec;

    let header_flags = rd_u16(rec, 22)?;
    if header_flags & 0x0001 == 0 {
        return None; // fora de uso
    }
    let is_dir = header_flags & 0x0002 != 0;

    let mut attr_off = rd_u16(rec, 20)? as usize;
    let bytes_in_use = rd_u32(rec, 24)? as usize;
    let limit = bytes_in_use.min(record_size).min(rec.len());

    let mut file_name: Option<String> = None;
    let mut parent_frn = 0u64;
    let mut size = 0u64;
    let mut unix_mtime = 0i64;
    let mut file_attrs = 0u32;

    while attr_off.checked_add(8).is_some_and(|end| end <= limit) {
        let Some(attr_type) = rd_u32(rec, attr_off) else {
            break;
        };
        if attr_type == 0xFFFF_FFFF {
            break;
        }
        let Some(attr_len) = rd_u32(rec, attr_off + 4).map(|v| v as usize) else {
            break;
        };
        if attr_len == 0 || attr_len > limit - attr_off {
            break;
        }
        let attr_end = attr_off + attr_len;
        let Some(&non_resident) = rec.get(attr_off + 8) else {
            break;
        };
        let non_resident = non_resident != 0;

        if attr_type == 0x30 && !non_resident && attr_len >= 22 {
            // $FILE_NAME residente
            let (Some(val_len), Some(val_off)) = (
                rd_u32(rec, attr_off + 16).map(|v| v as usize),
                rd_u16(rec, attr_off + 20).map(|v| v as usize),
            ) else {
                break;
            };
            let fn_start = attr_off + val_off;

            if val_len >= 66 && fn_start.checked_add(val_len).is_some_and(|e| e <= attr_end) {
                let name_len = rec.get(fn_start + 64).copied().unwrap_or(0) as usize;
                let namespace = rec.get(fn_start + 65).copied().unwrap_or(0);

                // Preferência por nomes Win32 ou Win32/DOS sobre apenas DOS 8.3 (namespace 2).
                if (namespace != 2 || file_name.is_none())
                    && fn_start + 66 + name_len * 2 <= attr_end
                {
                    if let (Some(p), Some(m), Some(s), Some(a)) = (
                        rd_u64(rec, fn_start),
                        rd_i64(rec, fn_start + 16),
                        rd_u64(rec, fn_start + 48),
                        rd_u32(rec, fn_start + 56),
                    ) {
                        file_name = Some(utf16le_lossy(
                            &rec[fn_start + 66..fn_start + 66 + name_len * 2],
                        ));
                        parent_frn = p & FRN_BASE_MASK;
                        size = s;
                        file_attrs = a;
                        unix_mtime = filetime_to_unix_secs(m);
                    }
                }
            }
        } else if attr_type == 0x80 {
            // $DATA: tamanho real dos dados
            if non_resident && attr_len >= 56 {
                if let Some(real) = rd_u64(rec, attr_off + 48) {
                    size = real;
                }
            } else if !non_resident && attr_len >= 24 {
                if let Some(len) = rd_u32(rec, attr_off + 16) {
                    size = u64::from(len);
                }
            }
        }

        attr_off = attr_end;
    }

    let name = file_name?;
    let mut flags = attrs_to_flags(file_attrs & !ATTR_DIRECTORY);
    // O tipo vem do cabeçalho do registro, não dos atributos.
    flags =
        (flags & !(FLAG_DIRECTORY | FLAG_FILE)) | if is_dir { FLAG_DIRECTORY } else { FLAG_FILE };
    if file_attrs & 0x0200 != 0 {
        flags |= FLAG_SPARSE;
    }
    if file_attrs & 0x0800 != 0 {
        flags |= FLAG_COMPRESSED;
    }
    if file_attrs & 0x0400 != 0 {
        flags |= FLAG_REPARSE;
    }
    if file_attrs & 0x1000 != 0 || file_attrs & 0x0044_0000 != 0 {
        flags |= FLAG_OFFLINE;
    }

    Some(MftRecord {
        is_dir,
        name,
        parent_frn,
        size,
        unix_mtime,
        file_attrs,
        flags,
    })
}

/// Um extent contíguo (em clusters) do `$DATA` da `$MFT`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DataRun {
    /// Primeiro cluster lógico (LCN) do extent no volume.
    pub lcn: u64,
    /// Quantidade de clusters do extent.
    pub clusters: u64,
}

/// Onde a `$MFT` realmente está no volume: seus extents e o tamanho real dos dados.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MftLayout {
    pub runs: Vec<DataRun>,
    /// Tamanho real do `$DATA` da `$MFT` em bytes (`registros = data_size / record_size`).
    pub data_size: u64,
}

/// Por que o layout da `$MFT` não pôde ser determinado (para diagnóstico).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LayoutError {
    /// Registro 0 curto, sem assinatura `FILE` ou com fixup inválido.
    BadRecord,
    /// O registro 0 não tem `$DATA` sem nome.
    NoData,
    /// `$DATA` residente (impossível numa MFT real).
    Resident,
    /// O `$DATA` do registro 0 começa em um VCN diferente de 0.
    Extension { start_vcn: u64 },
    /// Run list malformado (campo de tamanho inválido, run esparso, LCN negativo, overflow).
    BadRunList,
    /// Mais extents do que o limite defensivo.
    TooManyRuns,
    /// Os extents conhecidos não cobrem o `$DATA` inteiro (resto em registros de extensão).
    Incomplete {
        runs: usize,
        covered_bytes: u64,
        data_size: u64,
        has_attribute_list: bool,
    },
    /// `$ATTRIBUTE_LIST` não residente grande demais para ser lido com segurança.
    UnsupportedAttributeList,
    /// Falha ao ler ou interpretar o `$ATTRIBUTE_LIST` não residente.
    BadAttributeList,
    /// Registro de extensão ilegível, fora da região já mapeada ou inconsistente.
    BadExtension { record: u64 },
}

/// Limite defensivo de extents (a `$MFT` mais fragmentada observada tem dezenas).
const MAX_MFT_RUNS: usize = 4096;

/// Limite do `$ATTRIBUTE_LIST` não residente lido do volume (defesa contra tamanho absurdo).
const MAX_ATTR_LIST_BYTES: u64 = 16 * 1024 * 1024;

/// Atributo localizado dentro de um registro (cabeçalho comum).
struct Attr {
    ty: u32,
    off: usize,
    end: usize,
    non_resident: bool,
    name_len: u8,
}

/// Percorre os atributos de um registro (já com fixup aplicado), com todos os limites verificados.
fn scan_attrs(rec: &[u8], record_size: usize) -> Result<Vec<Attr>, LayoutError> {
    use LayoutError::BadRecord;
    let mut out = Vec::new();
    let mut off = rd_u16(rec, 20).ok_or(BadRecord)? as usize;
    let limit = (rd_u32(rec, 24).ok_or(BadRecord)? as usize)
        .min(record_size)
        .min(rec.len());

    while off.checked_add(8).is_some_and(|end| end <= limit) {
        let ty = rd_u32(rec, off).ok_or(BadRecord)?;
        if ty == 0xFFFF_FFFF {
            break;
        }
        let len = rd_u32(rec, off + 4).ok_or(BadRecord)? as usize;
        if len == 0 || len > limit - off {
            return Err(BadRecord);
        }
        out.push(Attr {
            ty,
            off,
            end: off + len,
            non_resident: *rec.get(off + 8).ok_or(BadRecord)? != 0,
            name_len: *rec.get(off + 9).ok_or(BadRecord)?,
        });
        off += len;
    }
    Ok(out)
}

enum AttrList {
    None,
    Resident(Vec<u8>),
    NonResident { runs: Vec<DataRun>, size: u64 },
}

struct Rec0Info {
    runs: Vec<DataRun>,
    data_size: u64,
    attr_list: AttrList,
}

fn covered_clusters(runs: &[DataRun]) -> u64 {
    runs.iter().fold(0u64, |a, r| a.saturating_add(r.clusters))
}

/// Interpreta o registro 0 (fixup já aplicado): extents do `$DATA` e localização do `$ATTRIBUTE_LIST`.
fn scan_rec0(rec: &[u8], record_size: usize) -> Result<Rec0Info, LayoutError> {
    use LayoutError::{BadRecord, Extension, NoData, Resident};
    let mut attr_list = AttrList::None;
    let mut data: Option<(Vec<DataRun>, u64)> = None;

    for a in scan_attrs(rec, record_size)? {
        if a.ty == 0x20 {
            attr_list = if a.non_resident {
                let runlist_off = rd_u16(rec, a.off + 32).ok_or(BadRecord)? as usize;
                let size = rd_u64(rec, a.off + 48).ok_or(BadRecord)?;
                AttrList::NonResident {
                    runs: decode_runs(rec, a.off + runlist_off, a.end)?,
                    size,
                }
            } else {
                let len = rd_u32(rec, a.off + 16).ok_or(BadRecord)? as usize;
                let voff = rd_u16(rec, a.off + 20).ok_or(BadRecord)? as usize;
                let start = a.off.checked_add(voff).ok_or(BadRecord)?;
                let end = start.checked_add(len).ok_or(BadRecord)?;
                if end > a.end {
                    return Err(BadRecord);
                }
                AttrList::Resident(rec[start..end].to_vec())
            };
        } else if a.ty == 0x80 && a.name_len == 0 {
            if !a.non_resident || a.end - a.off < 64 {
                return Err(Resident);
            }
            let start_vcn = rd_u64(rec, a.off + 16).ok_or(BadRecord)?;
            if start_vcn != 0 {
                return Err(Extension { start_vcn });
            }
            let runlist_off = rd_u16(rec, a.off + 32).ok_or(BadRecord)? as usize;
            let data_size = rd_u64(rec, a.off + 48).ok_or(BadRecord)?;
            data = Some((decode_runs(rec, a.off + runlist_off, a.end)?, data_size));
        }
    }
    let (runs, data_size) = data.ok_or(NoData)?;
    Ok(Rec0Info {
        runs,
        data_size,
        attr_list,
    })
}

/// Lê o *run list* do `$DATA` do registro 0 (`$MFT`) para descobrir os extents da tabela.
///
/// Só vê os extents que cabem no próprio registro 0. Para uma `$MFT` muito fragmentada o restante está em
/// registros de extensão (via `$ATTRIBUTE_LIST`): use [`resolve_mft_layout`]. Retorna `None` se o layout
/// não for confiável ou estiver incompleto.
#[must_use]
pub fn parse_mft_layout(
    rec0: &mut [u8],
    record_size: usize,
    bytes_per_cluster: u64,
) -> Option<MftLayout> {
    parse_mft_layout_detailed(rec0, record_size, bytes_per_cluster).ok()
}

/// Como [`parse_mft_layout`], mas informa o motivo da recusa.
pub fn parse_mft_layout_detailed(
    rec0: &mut [u8],
    record_size: usize,
    bytes_per_cluster: u64,
) -> Result<MftLayout, LayoutError> {
    if bytes_per_cluster == 0 || rec0.len() < record_size || !apply_mft_fixups(rec0, record_size) {
        return Err(LayoutError::BadRecord);
    }
    let info = scan_rec0(rec0, record_size)?;
    let covered_bytes = covered_clusters(&info.runs).saturating_mul(bytes_per_cluster);
    if info.runs.is_empty() || covered_bytes < info.data_size {
        return Err(LayoutError::Incomplete {
            runs: info.runs.len(),
            covered_bytes,
            data_size: info.data_size,
            has_attribute_list: !matches!(info.attr_list, AttrList::None),
        });
    }
    Ok(MftLayout {
        runs: info.runs,
        data_size: info.data_size,
    })
}

/// Decodifica um run list (cada atributo começa no LCN absoluto 0). Não valida a cobertura.
fn decode_runs(rec: &[u8], mut pos: usize, end: usize) -> Result<Vec<DataRun>, LayoutError> {
    use LayoutError::{BadRunList, TooManyRuns};
    let mut runs = Vec::new();
    let mut lcn: i64 = 0;

    loop {
        let header = *rec.get(pos).filter(|_| pos < end).ok_or(BadRunList)?;
        if header == 0 {
            break;
        }
        let len_size = (header & 0x0F) as usize;
        let off_size = (header >> 4) as usize;
        // Run esparso (offset de tamanho 0) não ocorre no `$DATA` da `$MFT`: tratar como inválido.
        if len_size == 0 || len_size > 8 || off_size == 0 || off_size > 8 {
            return Err(BadRunList);
        }
        let body = pos + 1;
        if body
            .checked_add(len_size + off_size)
            .is_none_or(|e| e > end)
        {
            return Err(BadRunList);
        }

        let clusters = rec[body..body + len_size]
            .iter()
            .rev()
            .fold(0u64, |acc, &b| (acc << 8) | u64::from(b));
        let off_bytes = &rec[body + len_size..body + len_size + off_size];
        let mut delta = off_bytes
            .iter()
            .rev()
            .fold(0u64, |acc, &b| (acc << 8) | u64::from(b));
        if off_bytes[off_size - 1] & 0x80 != 0 && off_size < 8 {
            delta |= u64::MAX << (off_size * 8); // extensão de sinal
        }

        lcn = lcn.checked_add(delta as i64).ok_or(BadRunList)?;
        if clusters == 0 || lcn < 0 {
            return Err(BadRunList);
        }
        if runs.len() >= MAX_MFT_RUNS {
            return Err(TooManyRuns);
        }
        runs.push(DataRun {
            lcn: lcn as u64,
            clusters,
        });
        pos = body + len_size + off_size;
    }
    Ok(runs)
}

/// Entrada do `$ATTRIBUTE_LIST` relevante para o `$DATA` sem nome.
struct ListEntry {
    start_vcn: u64,
    mft_record: u64,
}

fn parse_attribute_list(list: &[u8]) -> Vec<ListEntry> {
    let mut out = Vec::new();
    let mut pos = 0usize;
    while pos.checked_add(26).is_some_and(|e| e <= list.len()) {
        let (Some(ty), Some(rec_len)) = (rd_u32(list, pos), rd_u16(list, pos + 4)) else {
            break;
        };
        let rec_len = rec_len as usize;
        if rec_len < 26 || pos + rec_len > list.len() {
            break;
        }
        let name_len = list[pos + 6];
        if ty == 0x80 && name_len == 0 {
            if let (Some(vcn), Some(fref)) = (rd_u64(list, pos + 8), rd_u64(list, pos + 16)) {
                out.push(ListEntry {
                    start_vcn: vcn,
                    mft_record: fref & FRN_BASE_MASK,
                });
            }
        }
        pos += rec_len;
    }
    out
}

/// Traduz um deslocamento lógico dentro da `$MFT` para o deslocamento físico no volume, informando
/// quantos bytes restam contíguos naquele extent.
fn map_logical_offset(
    runs: &[DataRun],
    bytes_per_cluster: u64,
    logical: u64,
) -> Option<(u64, u64)> {
    let mut base = 0u64;
    for run in runs {
        let len = run.clusters.checked_mul(bytes_per_cluster)?;
        if logical < base.checked_add(len)? {
            let within = logical - base;
            return Some((
                run.lcn
                    .checked_mul(bytes_per_cluster)?
                    .checked_add(within)?,
                len - within,
            ));
        }
        base = base.checked_add(len)?;
    }
    None
}

/// Lê o conteúdo de um `$ATTRIBUTE_LIST` não residente a partir dos seus próprios extents.
fn read_nonresident_list(
    list_runs: &[DataRun],
    size: u64,
    bytes_per_cluster: u64,
    read: &mut impl FnMut(u64, &mut [u8]) -> std::io::Result<usize>,
) -> Result<Vec<u8>, LayoutError> {
    use LayoutError::{BadAttributeList, UnsupportedAttributeList};
    if size == 0 || size > MAX_ATTR_LIST_BYTES {
        return Err(UnsupportedAttributeList);
    }
    // Leituras em blocos múltiplos do cluster (alinhamento exigido pelo volume), no máximo ~1 MiB.
    let max_chunk = ((1u64 << 20) / bytes_per_cluster).max(1) * bytes_per_cluster;
    let mut data: Vec<u8> = Vec::with_capacity(size as usize);

    for run in list_runs {
        let mut done = 0u64;
        let run_bytes = run
            .clusters
            .checked_mul(bytes_per_cluster)
            .ok_or(BadAttributeList)?;
        let base = run
            .lcn
            .checked_mul(bytes_per_cluster)
            .ok_or(BadAttributeList)?;
        while done < run_bytes && (data.len() as u64) < size {
            let missing = (size - data.len() as u64).next_multiple_of(bytes_per_cluster);
            let chunk = (run_bytes - done).min(max_chunk).min(missing);
            let mut buf = vec![0u8; chunk as usize];
            match read(base + done, &mut buf) {
                Ok(n) if n as u64 >= chunk => {}
                _ => return Err(BadAttributeList),
            }
            data.extend_from_slice(&buf);
            done += chunk;
        }
    }
    if (data.len() as u64) < size {
        return Err(BadAttributeList);
    }
    data.truncate(size as usize);
    Ok(data)
}

/// Descobre **todos** os extents da `$MFT`, seguindo o `$ATTRIBUTE_LIST` quando o registro 0 não os
/// comporta (MFT muito fragmentada: o resto do run list vive em registros de extensão).
///
/// `read(offset, buf)` lê bytes do volume, com `offset` e `buf.len()` múltiplos do cluster. Os registros
/// de extensão do próprio `$MFT` ficam na região já mapeada pelos extents do registro 0, então são
/// localizados traduzindo `número × tamanho_do_registro` pelos extents conhecidos.
pub fn resolve_mft_layout(
    rec0: &mut [u8],
    record_size: usize,
    bytes_per_cluster: u64,
    mut read: impl FnMut(u64, &mut [u8]) -> std::io::Result<usize>,
) -> Result<MftLayout, LayoutError> {
    use LayoutError::{BadExtension, BadRecord, Incomplete, TooManyRuns};
    if bytes_per_cluster == 0 || rec0.len() < record_size || !apply_mft_fixups(rec0, record_size) {
        return Err(BadRecord);
    }
    let info = scan_rec0(rec0, record_size)?;
    let data_size = info.data_size;
    let has_list = !matches!(info.attr_list, AttrList::None);
    let incomplete = |runs: &[DataRun]| Incomplete {
        runs: runs.len(),
        covered_bytes: covered_clusters(runs).saturating_mul(bytes_per_cluster),
        data_size,
        has_attribute_list: has_list,
    };

    let mut runs = info.runs;
    if covered_clusters(&runs).saturating_mul(bytes_per_cluster) >= data_size && !runs.is_empty() {
        return Ok(MftLayout { runs, data_size });
    }

    let list = match info.attr_list {
        AttrList::Resident(v) => v,
        AttrList::NonResident {
            runs: list_runs,
            size,
        } => read_nonresident_list(&list_runs, size, bytes_per_cluster, &mut read)?,
        AttrList::None => return Err(incomplete(&runs)),
    };
    let mut extensions: Vec<(u64, u64)> = parse_attribute_list(&list)
        .into_iter()
        .filter(|e| e.start_vcn != 0)
        .map(|e| (e.start_vcn, e.mft_record))
        .collect();
    extensions.sort_unstable();
    extensions.dedup();

    for (start_vcn, rec_no) in extensions {
        let covered = covered_clusters(&runs);
        if start_vcn < covered {
            continue; // já coberto
        }
        if start_vcn > covered {
            return Err(incomplete(&runs)); // buraco no run list
        }

        // Onde está o registro de extensão? Dentro da região já mapeada da própria $MFT.
        let logical = rec_no
            .checked_mul(record_size as u64)
            .ok_or(BadExtension { record: rec_no })?;
        let (disk, contiguous) = map_logical_offset(&runs, bytes_per_cluster, logical)
            .ok_or(BadExtension { record: rec_no })?;
        if contiguous < record_size as u64 {
            return Err(BadExtension { record: rec_no });
        }
        let base = disk - disk % bytes_per_cluster;
        let lead = (disk - base) as usize;
        let read_len = (lead + record_size).next_multiple_of(bytes_per_cluster as usize);
        let mut buf = vec![0u8; read_len];
        match read(base, &mut buf) {
            Ok(n) if n >= lead + record_size => {}
            _ => return Err(BadExtension { record: rec_no }),
        }
        let ext = &mut buf[lead..lead + record_size];
        if !apply_mft_fixups(ext, record_size) {
            return Err(BadExtension { record: rec_no });
        }
        let ext: &[u8] = ext;

        let attr = scan_attrs(ext, record_size)
            .map_err(|_| BadExtension { record: rec_no })?
            .into_iter()
            .find(|a| {
                a.ty == 0x80
                    && a.name_len == 0
                    && a.non_resident
                    && rd_u64(ext, a.off + 16) == Some(start_vcn)
            })
            .ok_or(BadExtension { record: rec_no })?;
        let runlist_off = rd_u16(ext, attr.off + 32).ok_or(BadExtension { record: rec_no })?;
        let more = decode_runs(ext, attr.off + runlist_off as usize, attr.end)
            .map_err(|_| BadExtension { record: rec_no })?;
        if runs.len() + more.len() > MAX_MFT_RUNS {
            return Err(TooManyRuns);
        }
        runs.extend(more);
    }

    if covered_clusters(&runs).saturating_mul(bytes_per_cluster) < data_size {
        return Err(incomplete(&runs));
    }
    Ok(MftLayout { runs, data_size })
}

fn gcd(a: u64, b: u64) -> u64 {
    if b == 0 {
        a
    } else {
        gcd(b, a % b)
    }
}

/// Um bloco de leitura da `$MFT`: onde ler e a que registros ele corresponde.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReadChunk {
    /// Deslocamento em bytes no volume.
    pub offset: u64,
    /// Quantos registros (inteiros) interpretar neste bloco.
    pub records: u64,
    /// Número do primeiro registro do bloco (FRN base), contínuo ao longo de todos os extents.
    pub first_record: u64,
}

/// Divide a leitura da `$MFT` em blocos de até `chunk_records` registros (arredondado para baixo a um
/// múltiplo do cluster), percorrendo todos os extents.
///
/// Os números de registro são contínuos entre extents (o registro `n` é o `n`-ésimo da tabela lógica,
/// não do disco). Retorna `None` em caso de overflow aritmético ou parâmetros inválidos.
#[must_use]
pub fn plan_mft_reads(
    layout: &MftLayout,
    record_size: u64,
    bytes_per_cluster: u64,
    chunk_records: u64,
) -> Option<Vec<ReadChunk>> {
    if record_size == 0 || bytes_per_cluster == 0 || chunk_records == 0 {
        return None;
    }
    // Todo bloco (exceto o último de uma leitura) deve ter tamanho múltiplo do cluster, para que o
    // seguinte comece alinhado: I/O sem cache exige deslocamentos alinhados ao setor.
    let granule = bytes_per_cluster / gcd(record_size, bytes_per_cluster);
    let chunk_records = (chunk_records / granule).max(1) * granule;
    let total_records = layout.data_size / record_size;
    let mut chunks = Vec::new();
    let mut record = 0u64;

    for run in &layout.runs {
        if record >= total_records {
            break;
        }
        let run_bytes = run.clusters.checked_mul(bytes_per_cluster)?;
        let mut offset = run.lcn.checked_mul(bytes_per_cluster)?;
        let mut remaining = (run_bytes / record_size).min(total_records - record);

        while remaining > 0 {
            let n = remaining.min(chunk_records);
            chunks.push(ReadChunk {
                offset,
                records: n,
                first_record: record,
            });
            offset = offset.checked_add(n.checked_mul(record_size)?)?;
            record += n;
            remaining -= n;
        }
    }
    // Se os extents não cobrem todos os registros declarados, o plano está incompleto.
    (record == total_records).then_some(chunks)
}

/// Leitura posicional de um volume (ou de qualquer fonte de bytes endereçável).
pub trait VolumeReader {
    /// Lê até `buf.len()` bytes a partir de `offset`; retorna quantos foram lidos.
    fn read_at(&mut self, offset: u64, buf: &mut [u8]) -> std::io::Result<usize>;

    /// Alinhamento exigido (em bytes) para o endereço do buffer, o deslocamento e o tamanho de cada
    /// leitura. `1` = sem exigência (I/O com cache). I/O sem cache no Windows exige o tamanho do setor.
    fn alignment(&self) -> usize {
        1
    }
}

/// Registro da MFT interpretado, com o seu número (FRN base).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedRecord {
    pub frn: u64,
    pub record: MftRecord,
}

/// Lê e interpreta a `$MFT` com vários leitores em paralelo (fila de I/O > 1).
///
/// Uma leitura síncrona por vez mal usa um SSD NVMe (centenas de MB/s contra GB/s). Aqui cada leitor
/// abre a sua própria fonte via `open` (cada handle tem o seu ponteiro de arquivo), pega o próximo bloco
/// do `plan` e interpreta os registros na própria thread. O `sink` recebe os blocos na ordem de chegada
/// (a ordem entre blocos não importa: os pais são resolvidos depois, por FRN) e roda na thread chamadora.
///
/// A memória é limitada por canal com capacidade fixa. Retorna `true` somente se **todos** os blocos
/// foram lidos; qualquer falha de abertura ou leitura devolve `false` e o chamador não deve confiar no
/// que o `sink` recebeu.
pub fn read_mft_parallel<R, O>(
    plan: &[ReadChunk],
    record_size: usize,
    bytes_per_cluster: usize,
    readers: usize,
    open: O,
    sink: impl FnMut(Vec<ParsedRecord>),
) -> bool
where
    R: VolumeReader,
    O: Fn() -> std::io::Result<R> + Sync,
{
    read_mft_parallel_stats(plan, record_size, bytes_per_cluster, readers, open, sink).0
}

/// Tempos acumulados da leitura paralela, para diagnosticar onde o tempo vai (somas entre threads).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ReadStats {
    /// Tempo dos leitores dentro de `read_at` (I/O), somado entre os leitores.
    pub read_nanos: u64,
    /// Tempo dos leitores interpretando registros, somado entre os leitores.
    pub parse_nanos: u64,
    /// Tempo dos leitores bloqueados esperando o consumidor (canal cheio), somado.
    pub blocked_send_nanos: u64,
    /// Tempo do consumidor dentro do `sink` (inserção no índice).
    pub sink_nanos: u64,
    /// Tempo do consumidor esperando dados dos leitores.
    pub recv_wait_nanos: u64,
}

/// Como [`read_mft_parallel`], mas devolve também os tempos por etapa.
pub fn read_mft_parallel_stats<R, O>(
    plan: &[ReadChunk],
    record_size: usize,
    bytes_per_cluster: usize,
    readers: usize,
    open: O,
    mut sink: impl FnMut(Vec<ParsedRecord>),
) -> (bool, ReadStats)
where
    R: VolumeReader,
    O: Fn() -> std::io::Result<R> + Sync,
{
    use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
    use std::sync::mpsc::sync_channel;
    use std::time::Instant;

    if record_size == 0 || bytes_per_cluster == 0 {
        return (false, ReadStats::default());
    }
    let readers = readers.clamp(1, plan.len().max(1));
    let max_records = plan.iter().map(|c| c.records).max().unwrap_or(0) as usize;
    let buf_len = (max_records * record_size).next_multiple_of(bytes_per_cluster);

    let next = AtomicUsize::new(0);
    let failed = AtomicBool::new(false);
    let (read_ns, parse_ns, blocked_ns) = (AtomicU64::new(0), AtomicU64::new(0), AtomicU64::new(0));
    let (tx, rx) = sync_channel::<Vec<ParsedRecord>>(readers * 2);
    let mut received = 0usize;
    let (mut sink_ns, mut recv_ns) = (0u64, 0u64);

    std::thread::scope(|scope| {
        for _ in 0..readers {
            let tx = tx.clone();
            let (next, failed, open) = (&next, &failed, &open);
            let (read_ns, parse_ns, blocked_ns) = (&read_ns, &parse_ns, &blocked_ns);
            scope.spawn(move || {
                let Ok(mut reader) = open() else {
                    failed.store(true, Ordering::Relaxed);
                    return;
                };
                // Buffer alinhado em endereço (necessário para I/O sem cache), sem `unsafe`:
                // sobra-alocação e fatia a partir do primeiro endereço alinhado.
                let align = reader.alignment().max(1);
                let mut storage = vec![0u8; buf_len + align];
                let start = storage.as_ptr().align_offset(align);
                let buf = &mut storage[start..start + buf_len];
                loop {
                    if failed.load(Ordering::Relaxed) {
                        return;
                    }
                    let i = next.fetch_add(1, Ordering::Relaxed);
                    let Some(chunk) = plan.get(i) else { return };
                    let n = chunk.records as usize;
                    let want = (n * record_size).next_multiple_of(bytes_per_cluster);
                    let t_read = Instant::now();
                    let result = reader.read_at(chunk.offset, &mut buf[..want]);
                    read_ns.fetch_add(t_read.elapsed().as_nanos() as u64, Ordering::Relaxed);
                    match result {
                        Ok(got) if got >= n * record_size => {}
                        _ => {
                            failed.store(true, Ordering::Relaxed);
                            return;
                        }
                    }
                    let t_parse = Instant::now();
                    let mut parsed = Vec::with_capacity(n);
                    for r in 0..n {
                        let rec = &mut buf[r * record_size..(r + 1) * record_size];
                        if let Some(record) = parse_mft_record(rec, record_size) {
                            parsed.push(ParsedRecord {
                                frn: (chunk.first_record + r as u64) & FRN_BASE_MASK,
                                record,
                            });
                        }
                    }
                    parse_ns.fetch_add(t_parse.elapsed().as_nanos() as u64, Ordering::Relaxed);
                    let t_send = Instant::now();
                    let sent = tx.send(parsed);
                    blocked_ns.fetch_add(t_send.elapsed().as_nanos() as u64, Ordering::Relaxed);
                    if sent.is_err() {
                        return;
                    }
                }
            });
        }
        drop(tx);
        loop {
            let t_recv = Instant::now();
            let Ok(batch) = rx.recv() else { break };
            recv_ns += t_recv.elapsed().as_nanos() as u64;
            received += 1;
            let t_sink = Instant::now();
            sink(batch);
            sink_ns += t_sink.elapsed().as_nanos() as u64;
        }
    });

    let stats = ReadStats {
        read_nanos: read_ns.load(Ordering::Relaxed),
        parse_nanos: parse_ns.load(Ordering::Relaxed),
        blocked_send_nanos: blocked_ns.load(Ordering::Relaxed),
        sink_nanos: sink_ns,
        recv_wait_nanos: recv_ns,
    };
    (
        !failed.load(Ordering::Relaxed) && received == plan.len(),
        stats,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn usn_record(
        frn: u64,
        parent: u64,
        reason: u32,
        attrs: u32,
        name: &str,
        major: u16,
    ) -> Vec<u8> {
        let name16: Vec<u8> = name.encode_utf16().flat_map(u16::to_le_bytes).collect();
        let len = (USN_RECORD_V2_HEADER_LEN + name16.len()).next_multiple_of(8);
        let mut r = vec![0u8; len];
        r[0..4].copy_from_slice(&(len as u32).to_le_bytes());
        r[4..6].copy_from_slice(&major.to_le_bytes());
        r[8..16].copy_from_slice(&frn.to_le_bytes());
        r[16..24].copy_from_slice(&parent.to_le_bytes());
        let ft: i64 = FILETIME_UNIX_EPOCH_DIFF + 1_700_000_000 * 10_000_000;
        r[32..40].copy_from_slice(&ft.to_le_bytes());
        r[40..44].copy_from_slice(&reason.to_le_bytes());
        r[52..56].copy_from_slice(&attrs.to_le_bytes());
        r[56..58].copy_from_slice(&(name16.len() as u16).to_le_bytes());
        r[58..60].copy_from_slice(&(USN_RECORD_V2_HEADER_LEN as u16).to_le_bytes());
        r[60..60 + name16.len()].copy_from_slice(&name16);
        r
    }

    fn usn_buffer(records: &[Vec<u8>]) -> Vec<u8> {
        let mut b = vec![0u8; 8];
        for r in records {
            b.extend_from_slice(r);
        }
        b
    }

    #[test]
    fn parses_usn_records() {
        let buf = usn_buffer(&[
            usn_record(0x0003_0000_0000_0042, 5, 0x100, 0x10, "docs", 2),
            usn_record(43, 0x42, 0x200, 0x01, "leia-me.txt", 2),
        ]);
        let recs = parse_usn_records(&buf);
        assert_eq!(recs.len(), 2);
        assert_eq!(recs[0].frn, 0x42, "sequence number deve ser descartado");
        assert_eq!(recs[0].parent_frn, 5);
        assert!(recs[0].is_dir);
        assert_eq!(recs[0].name.as_deref(), Some("docs"));
        assert_eq!(recs[0].unix_secs, 1_700_000_000);
        assert_eq!(recs[1].name.as_deref(), Some("leia-me.txt"));
        assert_eq!(recs[1].flags & FLAG_READONLY, FLAG_READONLY);
        assert_eq!(recs[1].reason, 0x200);
    }

    #[test]
    fn skips_other_versions_and_stops_on_corruption() {
        let mut buf = usn_buffer(&[
            usn_record(1, 0, 0, 0, "v3", 3),
            usn_record(2, 0, 0, 0, "ok", 2),
        ]);
        assert_eq!(parse_usn_records(&buf).len(), 1);

        // record_length maior que o buffer: para sem pânico.
        buf.extend_from_slice(&usn_record(3, 0, 0, 0, "cortado", 2)[..40]);
        assert_eq!(parse_usn_records(&buf).len(), 1);

        // record_length = 0 e < cabeçalho: não entra em laço infinito.
        let mut zero = usn_buffer(&[usn_record(4, 0, 0, 0, "x", 2)]);
        zero[8..12].copy_from_slice(&0u32.to_le_bytes());
        assert!(parse_usn_records(&zero).is_empty());
        zero[8..12].copy_from_slice(&10u32.to_le_bytes());
        assert!(parse_usn_records(&zero).is_empty());
    }

    #[test]
    fn invalid_name_bounds_yield_no_name() {
        let mut buf = usn_buffer(&[usn_record(1, 0, 0, 0, "abc", 2)]);
        buf[8 + 56..8 + 58].copy_from_slice(&0xFFFFu16.to_le_bytes()); // nome além do registro
        assert_eq!(parse_usn_records(&buf)[0].name, None);
        buf[8 + 56..8 + 58].copy_from_slice(&6u16.to_le_bytes());
        buf[8 + 58..8 + 60].copy_from_slice(&4u16.to_le_bytes()); // offset dentro do cabeçalho
        assert_eq!(parse_usn_records(&buf)[0].name, None);
    }

    #[test]
    fn extreme_timestamps_do_not_overflow() {
        assert_eq!(filetime_to_unix_secs(i64::MIN), i64::MIN / 10_000_000);
        assert_eq!(
            filetime_to_unix_secs(i64::MAX),
            (i64::MAX - FILETIME_UNIX_EPOCH_DIFF) / 10_000_000
        );
    }

    /// Monta um registro MFT de 1024 bytes com um $FILE_NAME residente e um $DATA residente.
    pub(crate) fn mft_record(name: &str, in_use: bool, is_dir: bool, attrs: u32) -> Vec<u8> {
        let mut r = vec![0u8; 1024];
        r[0..4].copy_from_slice(MFT_RECORD_MAGIC);
        r[4..6].copy_from_slice(&48u16.to_le_bytes()); // offset do update sequence
        r[6..8].copy_from_slice(&3u16.to_le_bytes()); // assinatura + 2 setores
        r[48..50].copy_from_slice(&0x0001u16.to_le_bytes()); // assinatura
        r[510..512].copy_from_slice(&0x0001u16.to_le_bytes());
        r[1022..1024].copy_from_slice(&0x0001u16.to_le_bytes());
        r[50..52].copy_from_slice(&[0xAA, 0xBB]); // bytes originais do setor 1
        r[52..54].copy_from_slice(&[0xCC, 0xDD]); // bytes originais do setor 2
        r[20..22].copy_from_slice(&56u16.to_le_bytes());
        let flags = u16::from(in_use) | if is_dir { 2 } else { 0 };
        r[22..24].copy_from_slice(&flags.to_le_bytes());

        let name16: Vec<u8> = name.encode_utf16().flat_map(u16::to_le_bytes).collect();
        let val_len = 66 + name16.len();
        let attr_len = (24 + val_len).next_multiple_of(8);
        let a = 56;
        r[a..a + 4].copy_from_slice(&0x30u32.to_le_bytes());
        r[a + 4..a + 8].copy_from_slice(&(attr_len as u32).to_le_bytes());
        r[a + 16..a + 20].copy_from_slice(&(val_len as u32).to_le_bytes());
        r[a + 20..a + 22].copy_from_slice(&24u16.to_le_bytes());
        let v = a + 24;
        r[v..v + 8].copy_from_slice(&0x0005_0000_0000_0007u64.to_le_bytes());
        let ft: i64 = FILETIME_UNIX_EPOCH_DIFF + 1_600_000_000 * 10_000_000;
        r[v + 16..v + 24].copy_from_slice(&ft.to_le_bytes());
        r[v + 48..v + 56].copy_from_slice(&1234u64.to_le_bytes());
        r[v + 56..v + 60].copy_from_slice(&attrs.to_le_bytes());
        r[v + 64] = (name16.len() / 2) as u8;
        r[v + 65] = 1; // Win32
        r[v + 66..v + 66 + name16.len()].copy_from_slice(&name16);

        let d = a + attr_len; // $DATA residente com 99 bytes
        r[d..d + 4].copy_from_slice(&0x80u32.to_le_bytes());
        r[d + 4..d + 8].copy_from_slice(&32u32.to_le_bytes());
        r[d + 16..d + 20].copy_from_slice(&99u32.to_le_bytes());
        r[d + 32..d + 36].copy_from_slice(&0xFFFF_FFFFu32.to_le_bytes());
        r[24..28].copy_from_slice(&((d + 40) as u32).to_le_bytes()); // bytes em uso
        r
    }

    #[test]
    fn parses_mft_record_and_applies_fixups() {
        let mut r = mft_record("relatório.pdf", true, false, 0x0002);
        let m = parse_mft_record(&mut r, 1024).unwrap();
        assert_eq!(m.name, "relatório.pdf");
        assert_eq!(m.parent_frn, 7);
        assert_eq!(m.size, 99, "o $DATA sobrescreve o tamanho do $FILE_NAME");
        assert_eq!(m.unix_mtime, 1_600_000_000);
        assert!(!m.is_dir);
        assert_eq!(m.flags & FLAG_HIDDEN, FLAG_HIDDEN);
        assert_eq!(&r[510..512], &[0xAA, 0xBB], "fixup do setor 1");
        assert_eq!(&r[1022..1024], &[0xCC, 0xDD], "fixup do setor 2");

        let mut d = mft_record("pasta", true, true, 0x10);
        assert!(parse_mft_record(&mut d, 1024).unwrap().is_dir);
    }

    #[test]
    fn rejects_unused_unsigned_and_bad_fixup_records() {
        assert!(parse_mft_record(&mut mft_record("x", false, false, 0), 1024).is_none());
        let mut bad_magic = mft_record("x", true, false, 0);
        bad_magic[0] = b'B';
        assert!(parse_mft_record(&mut bad_magic, 1024).is_none());
        let mut bad_fix = mft_record("x", true, false, 0);
        bad_fix[510] = 0x77;
        assert!(parse_mft_record(&mut bad_fix, 1024).is_none());
        assert!(parse_mft_record(&mut [0u8; 10], 1024).is_none());
    }

    #[test]
    fn attribute_at_end_of_record_does_not_index_out_of_bounds() {
        // Regressão: `attr_off + 8 == record_size` indexava `rec[attr_off + 8]` fora do registro.
        let mut r = mft_record("x", true, false, 0);
        r[20..22].copy_from_slice(&1016u16.to_le_bytes());
        r[24..28].copy_from_slice(&1024u32.to_le_bytes());
        assert!(parse_mft_record(&mut r, 1024).is_none());

        // Atributo $FILE_NAME curto no fim do registro.
        let mut r = mft_record("x", true, false, 0);
        r[20..22].copy_from_slice(&1008u16.to_le_bytes());
        r[1008..1012].copy_from_slice(&0x30u32.to_le_bytes());
        r[1012..1016].copy_from_slice(&8u32.to_le_bytes());
        r[24..28].copy_from_slice(&1024u32.to_le_bytes());
        assert!(parse_mft_record(&mut r, 1024).is_none());
    }

    /// Codifica um run list (`(clusters, delta_lcn)`), terminado em zero.
    fn encode_runs(runs: &[(u64, i64)]) -> Vec<u8> {
        let mut list = Vec::new();
        for &(clusters, delta) in runs {
            let len_bytes = clusters.to_le_bytes();
            let ln = (8 - clusters.leading_zeros() as usize / 8).max(1);
            let off_bytes = delta.to_le_bytes();
            // Menor número de bytes que preserva o sinal.
            let mut on = 8;
            while on > 1 {
                let hi = off_bytes[on - 1];
                let next = off_bytes[on - 2];
                if (hi == 0x00 && next & 0x80 == 0) || (hi == 0xFF && next & 0x80 != 0) {
                    on -= 1;
                } else {
                    break;
                }
            }
            list.push(((on as u8) << 4) | ln as u8);
            list.extend_from_slice(&len_bytes[..ln]);
            list.extend_from_slice(&off_bytes[..on]);
        }
        list.push(0);
        list
    }

    /// Registro de 1024 bytes com (opcionalmente) um `$ATTRIBUTE_LIST` residente e um `$DATA` não
    /// residente sem nome com o run list dado, começando em `start_vcn`.
    fn mft_data_record(
        runs: &[(u64, i64)],
        data_size: u64,
        start_vcn: u64,
        attr_list: &[u8],
    ) -> Vec<u8> {
        let mut r = mft_record("$MFT", true, false, 0);
        // Descarta os atributos do helper (preservando os bytes de fixup em 510..512).
        for b in &mut r[56..510] {
            *b = 0;
        }
        let mut a = 56usize;
        if !attr_list.is_empty() {
            let len = (24 + attr_list.len()).next_multiple_of(8);
            r[a..a + 4].copy_from_slice(&0x20u32.to_le_bytes());
            r[a + 4..a + 8].copy_from_slice(&(len as u32).to_le_bytes());
            r[a + 16..a + 20].copy_from_slice(&(attr_list.len() as u32).to_le_bytes());
            r[a + 20..a + 22].copy_from_slice(&24u16.to_le_bytes());
            r[a + 24..a + 24 + attr_list.len()].copy_from_slice(attr_list);
            a += len;
        }
        let list = encode_runs(runs);
        let attr_len = (64 + list.len()).next_multiple_of(8);
        r[a..a + 4].copy_from_slice(&0x80u32.to_le_bytes());
        r[a + 4..a + 8].copy_from_slice(&(attr_len as u32).to_le_bytes());
        r[a + 8] = 1; // não residente
        r[a + 16..a + 24].copy_from_slice(&start_vcn.to_le_bytes());
        r[a + 32..a + 34].copy_from_slice(&64u16.to_le_bytes()); // offset do run list
        r[a + 40..a + 48].copy_from_slice(&data_size.to_le_bytes());
        r[a + 48..a + 56].copy_from_slice(&data_size.to_le_bytes());
        r[a + 56..a + 64].copy_from_slice(&data_size.to_le_bytes());
        r[a + 64..a + 64 + list.len()].copy_from_slice(&list);
        let end = a + attr_len;
        r[end..end + 4].copy_from_slice(&0xFFFF_FFFFu32.to_le_bytes());
        r[24..28].copy_from_slice(&((end + 8) as u32).to_le_bytes());
        r
    }

    /// Como `mft_data_record`, mas com `$ATTRIBUTE_LIST` **não residente** (run list `list_runs`, `list_size` bytes).
    fn mft_data_record_nonres_list(
        runs: &[(u64, i64)],
        data_size: u64,
        list_runs: &[(u64, i64)],
        list_size: u64,
    ) -> Vec<u8> {
        let mut r = mft_data_record(runs, data_size, 0, &[]);
        // Reconstrói os atributos: $ATTRIBUTE_LIST não residente primeiro, $DATA depois.
        let data_attr_off = 56usize;
        let data_len =
            u32::from_le_bytes(r[data_attr_off + 4..data_attr_off + 8].try_into().unwrap())
                as usize;
        let data_attr = r[data_attr_off..data_attr_off + data_len].to_vec();
        let lr = encode_runs(list_runs);
        let list_len = (64 + lr.len()).next_multiple_of(8);
        for b in &mut r[56..510] {
            *b = 0;
        }
        let a = 56usize;
        r[a..a + 4].copy_from_slice(&0x20u32.to_le_bytes());
        r[a + 4..a + 8].copy_from_slice(&(list_len as u32).to_le_bytes());
        r[a + 8] = 1; // não residente
        r[a + 32..a + 34].copy_from_slice(&64u16.to_le_bytes());
        r[a + 40..a + 48].copy_from_slice(&list_size.to_le_bytes());
        r[a + 48..a + 56].copy_from_slice(&list_size.to_le_bytes());
        r[a + 56..a + 64].copy_from_slice(&list_size.to_le_bytes());
        r[a + 64..a + 64 + lr.len()].copy_from_slice(&lr);
        let d = a + list_len;
        r[d..d + data_attr.len()].copy_from_slice(&data_attr);
        let end = d + data_attr.len();
        r[end..end + 4].copy_from_slice(&0xFFFF_FFFFu32.to_le_bytes());
        r[24..28].copy_from_slice(&((end + 8) as u32).to_le_bytes());
        r
    }

    /// Registro 0 de 1024 bytes com `$DATA` não residente da `$MFT`, com os runs dados.
    fn mft0_with_runs(runs: &[(u64, i64)], data_size: u64) -> Vec<u8> {
        mft_data_record(runs, data_size, 0, &[])
    }

    /// Entrada de `$ATTRIBUTE_LIST` para um `$DATA` sem nome.
    fn list_entry(start_vcn: u64, mft_record: u64) -> Vec<u8> {
        let mut e = vec![0u8; 32];
        e[0..4].copy_from_slice(&0x80u32.to_le_bytes());
        e[4..6].copy_from_slice(&32u16.to_le_bytes());
        e[8..16].copy_from_slice(&start_vcn.to_le_bytes());
        e[16..24].copy_from_slice(&(mft_record | (7 << 48)).to_le_bytes()); // com sequence number
        e
    }

    #[test]
    fn parses_fragmented_mft_layout() {
        // 3 extents: LCN 0xC0000 (96 clusters), depois +0x1000 (64), depois -0x2000 (32, retrocede).
        let mut r = mft0_with_runs(&[(96, 0xC0000), (64, 0x1000), (32, -0x2000)], 192 * 4096);
        let layout = parse_mft_layout(&mut r, 1024, 4096).unwrap();
        assert_eq!(
            layout.runs,
            vec![
                DataRun {
                    lcn: 0xC0000,
                    clusters: 96
                },
                DataRun {
                    lcn: 0xC1000,
                    clusters: 64
                },
                DataRun {
                    lcn: 0xBF000,
                    clusters: 32
                },
            ]
        );
        assert_eq!(layout.data_size, 192 * 4096);
    }

    #[test]
    fn rejects_incomplete_or_unsupported_layouts() {
        // Extents cobrem menos que `data_size`: faltam runs (extensão em $ATTRIBUTE_LIST).
        let mut short = mft0_with_runs(&[(10, 0x1000)], 100 * 4096);
        assert!(parse_mft_layout(&mut short, 1024, 4096).is_none());
        assert_eq!(
            parse_mft_layout_detailed(&mut mft0_with_runs(&[(10, 0x1000)], 100 * 4096), 1024, 4096),
            Err(LayoutError::Incomplete {
                runs: 1,
                covered_bytes: 10 * 4096,
                data_size: 100 * 4096,
                has_attribute_list: false,
            })
        );
        // Sem `$DATA`.
        assert!(parse_mft_layout(&mut mft_record("x", true, false, 0), 1024, 4096).is_none());
        // Cluster inválido / registro curto.
        let ok = mft0_with_runs(&[(8, 0x1000)], 8 * 4096);
        assert!(parse_mft_layout(&mut ok.clone(), 1024, 0).is_none());
        assert!(parse_mft_layout(&mut ok.clone()[..100], 1024, 4096).is_none());
        // Run com LCN negativo.
        let mut neg = mft0_with_runs(&[(8, -5)], 8 * 4096);
        assert!(parse_mft_layout(&mut neg, 1024, 4096).is_none());
    }

    /// Volume de 64 clusters (4096 B). A `$MFT` tem 32 clusters (128 registros): os 16 primeiros no LCN 10
    /// (registro 0 em 10, extensão nº 5 dentro desse trecho), e os outros 16 num extent no LCN 40, descrito
    /// só pelo registro de extensão.
    fn volume_with_extension() -> (Vec<u8>, Vec<u8>) {
        let bpc = 4096usize;
        let data_size = 32 * 4096u64;
        let attr_list = [list_entry(0, 0), list_entry(16, 5)].concat();
        let rec0 = mft_data_record(&[(16, 10)], data_size, 0, &attr_list);
        let ext = mft_data_record(&[(16, 40)], data_size, 16, &[]);
        let mut vol = vec![0u8; 64 * bpc];
        // O registro 5 fica no logical 5*1024 = dentro do cluster 1 do primeiro extent (LCN 11).
        let at = 10 * bpc + 5 * 1024;
        vol[at..at + 1024].copy_from_slice(&ext);
        (rec0, vol)
    }

    #[test]
    fn resolves_layout_through_attribute_list_extension() {
        let (mut rec0, vol) = volume_with_extension();
        // Sem seguir o `$ATTRIBUTE_LIST`, o layout é recusado como incompleto.
        assert!(matches!(
            parse_mft_layout_detailed(&mut rec0.clone(), 1024, 4096),
            Err(LayoutError::Incomplete {
                has_attribute_list: true,
                ..
            })
        ));
        let layout = resolve_mft_layout(&mut rec0, 1024, 4096, |off, buf| {
            let o = off as usize;
            buf.copy_from_slice(&vol[o..o + buf.len()]);
            Ok(buf.len())
        })
        .unwrap();
        assert_eq!(
            layout.runs,
            vec![
                DataRun {
                    lcn: 10,
                    clusters: 16
                },
                DataRun {
                    lcn: 40,
                    clusters: 16
                },
            ]
        );
        assert_eq!(layout.data_size, 32 * 4096);
        // O plano resultante cobre todos os 128 registros.
        let plan = plan_mft_reads(&layout, 1024, 4096, 64).unwrap();
        assert_eq!(plan.iter().map(|c| c.records).sum::<u64>(), 128);
    }

    #[test]
    fn extension_failures_are_reported_not_ignored() {
        // Leitura do registro de extensão falha.
        let (mut rec0, _) = volume_with_extension();
        let err = resolve_mft_layout(&mut rec0, 1024, 4096, |_, _| {
            Err(std::io::Error::other("sem acesso"))
        });
        assert_eq!(err, Err(LayoutError::BadExtension { record: 5 }));

        // Registro de extensão fora da região já mapeada (nº 999 ≫ 16 clusters × 4 registros).
        let attr_list = [list_entry(0, 0), list_entry(16, 999)].concat();
        let mut far = mft_data_record(&[(16, 10)], 32 * 4096, 0, &attr_list);
        let err = resolve_mft_layout(&mut far, 1024, 4096, |_, b| Ok(b.len()));
        assert_eq!(err, Err(LayoutError::BadExtension { record: 999 }));

        // Buraco no run list (extensão começa no VCN 20, mas só 16 clusters estão cobertos).
        let attr_list = [list_entry(0, 0), list_entry(20, 5)].concat();
        let mut gap = mft_data_record(&[(16, 10)], 32 * 4096, 0, &attr_list);
        assert!(matches!(
            resolve_mft_layout(&mut gap, 1024, 4096, |_, b| Ok(b.len())),
            Err(LayoutError::Incomplete { .. })
        ));

        // Sem `$ATTRIBUTE_LIST` e incompleto: erro de cobertura, não de extensão.
        let mut none = mft0_with_runs(&[(8, 10)], 32 * 4096);
        assert!(matches!(
            resolve_mft_layout(&mut none, 1024, 4096, |_, b| Ok(b.len())),
            Err(LayoutError::Incomplete {
                has_attribute_list: false,
                ..
            })
        ));
    }

    #[test]
    fn resolves_layout_with_nonresident_attribute_list() {
        let (_, mut vol) = volume_with_extension();
        let data_size = 32 * 4096u64;
        // A lista vai para o cluster 50 do volume, descrita por um run próprio do registro 0.
        let list = [list_entry(0, 0), list_entry(16, 5)].concat();
        vol[50 * 4096..50 * 4096 + list.len()].copy_from_slice(&list);
        let mut rec0 =
            mft_data_record_nonres_list(&[(16, 10)], data_size, &[(1, 50)], list.len() as u64);
        let layout = resolve_mft_layout(&mut rec0, 1024, 4096, |off, buf| {
            let o = off as usize;
            buf.copy_from_slice(&vol[o..o + buf.len()]);
            Ok(buf.len())
        })
        .unwrap();
        assert_eq!(layout.runs.len(), 2);
        assert_eq!(
            layout.runs[1],
            DataRun {
                lcn: 40,
                clusters: 16
            }
        );
    }

    #[test]
    fn nonresident_attribute_list_failures_are_reported() {
        let data_size = 32 * 4096u64;
        // Tamanho absurdo: recusado sem tentar ler.
        let mut huge = mft_data_record_nonres_list(&[(16, 10)], data_size, &[(1, 50)], u64::MAX);
        assert_eq!(
            resolve_mft_layout(&mut huge, 1024, 4096, |_, _| panic!("não deve ler")),
            Err(LayoutError::UnsupportedAttributeList)
        );
        // Leitura falha.
        let mut rec0 = mft_data_record_nonres_list(&[(16, 10)], data_size, &[(1, 50)], 64);
        assert_eq!(
            resolve_mft_layout(&mut rec0, 1024, 4096, |_, _| Err(std::io::Error::other(
                "x"
            ))),
            Err(LayoutError::BadAttributeList)
        );
        // Tamanho declarado maior que os clusters alocados à lista.
        let mut short = mft_data_record_nonres_list(&[(16, 10)], data_size, &[(1, 50)], 3 * 4096);
        assert_eq!(
            resolve_mft_layout(&mut short, 1024, 4096, |_, b| Ok(b.len())),
            Err(LayoutError::BadAttributeList)
        );
    }

    #[test]
    fn complete_layout_does_not_need_extension_reads() {
        let mut rec0 = mft0_with_runs(&[(8, 10), (8, 100)], 16 * 4096);
        let layout = resolve_mft_layout(&mut rec0, 1024, 4096, |_, _| {
            panic!("não deve ler o volume quando o registro 0 basta")
        })
        .unwrap();
        assert_eq!(layout.runs.len(), 2);
    }

    #[test]
    fn mutated_attribute_lists_never_panic() {
        let (rec0, vol) = volume_with_extension();
        let mut rng = Rng(0xA77_1157);
        for _ in 0..20_000 {
            let mut b = mutate(&mut rng, &rec0);
            b.resize(1024, 0);
            let _ = resolve_mft_layout(&mut b, 1024, 4096, |off, buf| {
                let o = (off as usize).min(vol.len());
                let n = buf.len().min(vol.len() - o);
                buf[..n].copy_from_slice(&vol[o..o + n]);
                Ok(n)
            });
        }
    }

    #[test]
    fn mutated_mft_layouts_never_panic() {
        let base = mft0_with_runs(&[(96, 0xC0000), (64, 0x1000), (32, -0x2000)], 192 * 4096);
        let mut rng = Rng(0x00FE_ED00);
        for _ in 0..20_000 {
            let mut b = mutate(&mut rng, &base);
            b.resize(1024, 0);
            let _ = parse_mft_layout(&mut b, 1024, 4096);
        }
    }

    #[test]
    fn read_plan_numbers_records_continuously_across_extents() {
        // 3 extents de 8, 4 e 2 clusters de 4096 B = 32 + 16 + 8 = 56 registros de 1024 B.
        let layout = MftLayout {
            runs: vec![
                DataRun {
                    lcn: 100,
                    clusters: 8,
                },
                DataRun {
                    lcn: 500,
                    clusters: 4,
                },
                DataRun {
                    lcn: 50,
                    clusters: 2,
                },
            ],
            data_size: 56 * 1024,
        };
        let plan = plan_mft_reads(&layout, 1024, 4096, 20).unwrap();
        assert_eq!(
            plan,
            vec![
                ReadChunk {
                    offset: 100 * 4096,
                    records: 20,
                    first_record: 0
                },
                ReadChunk {
                    offset: 100 * 4096 + 20 * 1024,
                    records: 12,
                    first_record: 20
                },
                ReadChunk {
                    offset: 500 * 4096,
                    records: 16,
                    first_record: 32
                },
                ReadChunk {
                    offset: 50 * 4096,
                    records: 8,
                    first_record: 48
                },
            ]
        );
        assert_eq!(plan.iter().map(|c| c.records).sum::<u64>(), 56);
    }

    #[test]
    fn read_plan_truncates_to_data_size_and_rejects_gaps() {
        // O último extent é maior que o necessário: lê só até `data_size`.
        let layout = MftLayout {
            runs: vec![DataRun {
                lcn: 10,
                clusters: 100,
            }],
            data_size: 10 * 1024,
        };
        let plan = plan_mft_reads(&layout, 1024, 4096, 1024).unwrap();
        assert_eq!(
            plan,
            vec![ReadChunk {
                offset: 10 * 4096,
                records: 10,
                first_record: 0
            }]
        );

        // Extents curtos demais para `data_size`: plano incompleto.
        let short = MftLayout {
            runs: vec![DataRun {
                lcn: 10,
                clusters: 1,
            }],
            data_size: 100 * 1024,
        };
        assert!(plan_mft_reads(&short, 1024, 4096, 1024).is_none());
        // Overflow e parâmetros inválidos.
        let huge = MftLayout {
            runs: vec![DataRun {
                lcn: u64::MAX,
                clusters: 2,
            }],
            data_size: 1024,
        };
        assert!(plan_mft_reads(&huge, 1024, 4096, 1024).is_none());
        assert!(plan_mft_reads(&layout, 0, 4096, 1024).is_none());
        assert!(plan_mft_reads(&layout, 1024, 4096, 0).is_none());
    }

    /// Volume em memória: bytes endereçáveis, com falha opcional a partir de um offset.
    struct FakeVolume<'a> {
        data: &'a [u8],
        fail_at: Option<u64>,
        align: usize,
    }
    impl VolumeReader for FakeVolume<'_> {
        fn alignment(&self) -> usize {
            self.align
        }
        fn read_at(&mut self, offset: u64, buf: &mut [u8]) -> std::io::Result<usize> {
            // Como I/O sem cache: endereço, deslocamento e tamanho precisam ser múltiplos do alinhamento.
            assert_eq!(buf.as_ptr() as usize % self.align, 0, "buffer desalinhado");
            assert_eq!(offset as usize % self.align, 0, "offset desalinhado");
            assert_eq!(buf.len() % self.align, 0, "tamanho desalinhado");
            if self.fail_at.is_some_and(|f| offset >= f) {
                return Err(std::io::Error::other("falha simulada"));
            }
            let start = offset as usize;
            if start >= self.data.len() {
                return Ok(0);
            }
            let n = buf.len().min(self.data.len() - start);
            buf[..n].copy_from_slice(&self.data[start..start + n]);
            Ok(n)
        }
    }

    /// Volume com a MFT fragmentada em 3 extents (invertidos no disco) e 40 registros nomeados.
    fn fragmented_volume() -> (Vec<u8>, MftLayout) {
        let cluster = 4096usize;
        let mut vol = vec![0u8; 64 * cluster];
        // extents (em clusters): A em 20..24 (16 registros), B em 4..8 (16), C em 40..42 (8)
        let extents = [(20usize, 4u64), (4, 4), (40, 2)];
        let mut record = 0usize;
        for &(lcn, clusters) in &extents {
            for k in 0..(clusters as usize * cluster / 1024) {
                let rec = mft_record(&format!("registro-{record}.txt"), true, false, 0);
                let off = lcn * cluster + k * 1024;
                vol[off..off + 1024].copy_from_slice(&rec);
                record += 1;
            }
        }
        let layout = MftLayout {
            runs: extents
                .iter()
                .map(|&(lcn, clusters)| DataRun {
                    lcn: lcn as u64,
                    clusters,
                })
                .collect(),
            data_size: 40 * 1024,
        };
        (vol, layout)
    }

    #[test]
    fn parallel_read_parses_every_record_exactly_once() {
        let (vol, layout) = fragmented_volume();
        for readers in [1usize, 3, 8] {
            for chunk_records in [4u64, 7, 1024] {
                let plan = plan_mft_reads(&layout, 1024, 4096, chunk_records).unwrap();
                let mut seen = std::collections::BTreeMap::new();
                let ok = read_mft_parallel(
                    &plan,
                    1024,
                    4096,
                    readers,
                    || {
                        Ok(FakeVolume {
                            data: &vol,
                            fail_at: None,
                            align: 4096,
                        })
                    },
                    |batch| {
                        for p in batch {
                            assert!(seen.insert(p.frn, p.record.name).is_none(), "FRN duplicado");
                        }
                    },
                );
                assert!(ok, "readers={readers} chunk={chunk_records}");
                assert_eq!(seen.len(), 40);
                for (frn, name) in &seen {
                    assert_eq!(
                        name,
                        &format!("registro-{frn}.txt"),
                        "FRN contínuo entre extents"
                    );
                }
            }
        }
    }

    #[test]
    fn parallel_read_reports_failure_instead_of_partial_success() {
        let (vol, layout) = fragmented_volume();
        let plan = plan_mft_reads(&layout, 1024, 4096, 4).unwrap();
        // Falha ao ler o segundo extent (offset >= cluster 4).
        let ok = read_mft_parallel(
            &plan,
            1024,
            4096,
            4,
            || {
                Ok(FakeVolume {
                    data: &vol,
                    fail_at: Some(4 * 4096),
                    align: 4096,
                })
            },
            |_| {},
        );
        assert!(!ok);
        // Falha ao abrir a fonte.
        let ok = read_mft_parallel::<FakeVolume, _>(
            &plan,
            1024,
            4096,
            2,
            || Err(std::io::Error::other("sem acesso")),
            |_| {},
        );
        assert!(!ok);
        // Leitura curta (EOF antes do esperado) também é falha.
        let short = &vol[..20 * 4096 + 100];
        let ok = read_mft_parallel(
            &plan,
            1024,
            4096,
            2,
            || {
                Ok(FakeVolume {
                    data: short,
                    fail_at: None,
                    align: 4096,
                })
            },
            |_| {},
        );
        assert!(!ok);
    }

    struct Rng(u64);
    impl Rng {
        fn next(&mut self) -> u64 {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            self.0
        }
        fn below(&mut self, n: usize) -> usize {
            (self.next() % n.max(1) as u64) as usize
        }
    }

    fn mutate(rng: &mut Rng, base: &[u8]) -> Vec<u8> {
        let mut b = base.to_vec();
        for _ in 0..1 + rng.below(12) {
            match rng.below(4) {
                0 => {
                    let i = rng.below(b.len());
                    b[i] = rng.next() as u8;
                }
                1 => {
                    let i = rng.below(b.len());
                    b[i] ^= 1 << rng.below(8);
                }
                2 => b.truncate(1 + rng.below(b.len())),
                _ => {
                    let i = rng.below(b.len());
                    let j = rng.below(b.len());
                    b[i..].iter_mut().take(j % 8).for_each(|x| *x = 0xFF);
                }
            }
        }
        b
    }

    #[test]
    fn mutated_usn_buffers_never_panic() {
        let base = usn_buffer(&[
            usn_record(1, 0, 0x100, 0x10, "a", 2),
            usn_record(2, 1, 0x200, 0x20, "bbbb.txt", 2),
        ]);
        let mut rng = Rng(0xABCDEF);
        for _ in 0..20_000 {
            let _ = parse_usn_records(&mutate(&mut rng, &base));
        }
        for _ in 0..5_000 {
            let n = rng.below(300);
            let junk: Vec<u8> = (0..n).map(|_| rng.next() as u8).collect();
            let _ = parse_usn_records(&junk);
        }
    }

    #[test]
    fn mutated_mft_records_never_panic() {
        let base = mft_record("arquivo-longo-de-teste.txt", true, false, 0x20);
        let mut rng = Rng(0x1234_5678);
        for _ in 0..20_000 {
            let mut b = mutate(&mut rng, &base);
            // Mantém o tamanho do registro: o chamador sempre fornece `record_size` bytes.
            b.resize(1024, 0);
            let _ = parse_mft_record(&mut b, 1024);
        }
        for _ in 0..5_000 {
            let mut b: Vec<u8> = (0..1024).map(|_| rng.next() as u8).collect();
            b[0..4].copy_from_slice(MFT_RECORD_MAGIC);
            let _ = parse_mft_record(&mut b, 1024);
        }
    }
}
