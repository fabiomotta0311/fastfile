//! Estrutura principal do índice (Struct-of-Arrays com arenas de nomes).

use std::collections::HashMap;
use std::hash::{BuildHasherDefault, Hasher};
use std::io::{self, Read, Write};

use crate::entry::{Entry, FLAG_DELETED, NO_PARENT};
use crate::source::{IndexChange, IndexError};

/// Formato atual do snapshot (checksum por palavras de 8 bytes, SNAPSHOT-CHECKSUM-002).
const SNAPSHOT_MAGIC: &[u8; 8] = b"FFSNAP02";
/// Formato anterior (checksum byte a byte); ainda aceito na carga, nunca gravado.
const SNAPSHOT_MAGIC_V1: &[u8; 8] = b"FFSNAP01";

/// Valor sentinela indicando que o tamanho alocado não está disponível / desconhecido.
pub const ALLOCATED_SIZE_UNKNOWN: u64 = u64::MAX;

/// Índice de arquivos em memória otimizado para varredura e busca instantânea (§2.2).
#[derive(Debug, Default, Clone)]
pub struct FileIndex {
    /// Entradas do índice (ID = índice no vetor).
    pub entries: Vec<Entry>,
    /// Arena contínua com todos os nomes de arquivo em UTF-8.
    pub names: Vec<u8>,
    /// Arena paralela com todos os nomes já em minúsculas (UTF-8) para busca case-insensitive.
    pub names_lc: Vec<u8>,
    /// Mapeamento de NTFS File Reference Number (FRN) para o ID da entrada.
    pub frn_map: FrnMap,
    /// Mapeamento de grupos de hard links compartilhando o mesmo FRN/i-node.
    pub hardlinks: HashMap<u64, Vec<u32>>,
    /// Espaço alocado em disco correspondente a cada entrada (Struct-of-Arrays).
    pub allocated_sizes: Vec<u64>,
    /// Volume associado a este índice (ex.: "C:", ".").
    pub volume: String,
    /// ID do USN Journal no momento da indexação.
    pub journal_id: u64,
    /// Último USN processado e registrado no índice.
    pub last_usn: i64,
}

impl FileIndex {
    /// Cria um novo índice vazio.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Cria um índice pré-alocando capacidade para entradas e arenas de nomes.
    #[must_use]
    pub fn with_capacity(entries_cap: usize, names_cap: usize) -> Self {
        Self {
            entries: Vec::with_capacity(entries_cap),
            names: Vec::with_capacity(names_cap),
            names_lc: Vec::with_capacity(names_cap),
            frn_map: FrnMap::with_capacity_and_hasher(entries_cap, Default::default()),
            hardlinks: HashMap::new(),
            allocated_sizes: Vec::with_capacity(entries_cap),
            volume: String::new(),
            journal_id: 0,
            last_usn: 0,
        }
    }

    /// Retorna o espaço alocado em disco para a entrada (se disponível).
    #[inline]
    #[must_use]
    pub fn allocated_size(&self, id: u32) -> Option<u64> {
        self.allocated_sizes
            .get(id as usize)
            .copied()
            .filter(|&sz| sz != ALLOCATED_SIZE_UNKNOWN)
    }

    /// Define o espaço alocado em disco para a entrada.
    pub fn set_allocated_size(&mut self, id: u32, size: Option<u64>) {
        let idx = id as usize;
        if self.allocated_sizes.len() <= idx {
            self.allocated_sizes.resize(idx + 1, ALLOCATED_SIZE_UNKNOWN);
        }
        self.allocated_sizes[idx] = size.unwrap_or(ALLOCATED_SIZE_UNKNOWN);
    }

    /// Quantidade de entradas totais no índice (incluindo marcadas como deletadas).
    #[inline]
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Retorna verdadeiro se o índice não contém entradas.
    #[inline]
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Retorna o nome original da entrada com o ID especificado.
    pub fn name(&self, id: u32) -> Result<&str, IndexError> {
        let entry = self.entry(id)?;
        let start = entry.name_off as usize;
        let end = start + entry.name_len as usize;
        if end > self.names.len() {
            return Err(IndexError::Other(format!(
                "offset de nome inválido para ID {id}: [{start}..{end}] em arena de tamanho {}",
                self.names.len()
            )));
        }
        std::str::from_utf8(&self.names[start..end])
            .map_err(|e| IndexError::Other(format!("nome UTF-8 inválido para ID {id}: {e}")))
    }

    /// Retorna o nome em minúsculas da entrada com o ID especificado.
    pub fn name_lc(&self, id: u32) -> Result<&str, IndexError> {
        let entry = self.entry(id)?;
        let start = entry.name_off as usize;
        let end = start + entry.name_len as usize;
        if end > self.names_lc.len() {
            return Err(IndexError::Other(format!(
                "offset de nome_lc inválido para ID {id}: [{start}..{end}] em arena de tamanho {}",
                self.names_lc.len()
            )));
        }
        std::str::from_utf8(&self.names_lc[start..end])
            .map_err(|e| IndexError::Other(format!("nome_lc UTF-8 inválido para ID {id}: {e}")))
    }

    /// Retorna uma referência imutável à entrada correspondente ao ID.
    #[inline]
    pub fn entry(&self, id: u32) -> Result<&Entry, IndexError> {
        self.entries
            .get(id as usize)
            .ok_or(IndexError::InvalidId(id))
    }

    /// Retorna uma referência mutável à entrada correspondente ao ID.
    #[inline]
    pub fn entry_mut(&mut self, id: u32) -> Result<&mut Entry, IndexError> {
        self.entries
            .get_mut(id as usize)
            .ok_or(IndexError::InvalidId(id))
    }

    /// Consulta o ID correspondente a um número de referência NTFS (FRN).
    #[inline]
    #[must_use]
    pub fn id_by_frn(&self, frn: u64) -> Option<u32> {
        self.frn_map.get(&frn).copied()
    }

    /// Insere uma nova entrada no índice.
    pub fn add_entry(
        &mut self,
        frn: u64,
        parent: u32,
        name: &str,
        flags: u16,
        size: u64,
        mtime: i64,
    ) -> Result<u32, IndexError> {
        if parent != NO_PARENT && (parent as usize) >= self.entries.len() {
            return Err(IndexError::ParentNotFound(parent));
        }

        let name_off = self.names.len() as u32;
        let name_len = name.len() as u16;

        self.names.extend_from_slice(name.as_bytes());

        // Escreve versão em minúsculas na arena paralela
        let name_lower = name.to_lowercase();
        self.names_lc.extend_from_slice(name_lower.as_bytes());

        let id = self.entries.len() as u32;
        let mut final_flags = flags;

        if frn != 0 {
            if let Some(&existing_id) = self.frn_map.get(&frn) {
                // Hard link detectado: duas entradas compartilham o mesmo FRN/i-node
                self.entries[existing_id as usize].flags |= crate::entry::FLAG_HARDLINK;
                final_flags |= crate::entry::FLAG_HARDLINK;

                let group = self.hardlinks.entry(frn).or_default();
                if !group.contains(&existing_id) {
                    group.push(existing_id);
                }
                if !group.contains(&id) {
                    group.push(id);
                }
            }
            self.frn_map.insert(frn, id);
        }

        self.entries.push(Entry {
            parent,
            name_off,
            size,
            mtime,
            name_len,
            flags: final_flags,
            _reserved: 0,
        });

        Ok(id)
    }

    /// Remove (soft-delete) uma entrada do índice a partir do seu FRN.
    pub fn remove_by_frn(&mut self, frn: u64) -> Result<u32, IndexError> {
        let id = self
            .frn_map
            .remove(&frn)
            .ok_or(IndexError::FrnNotFound(frn))?;
        let entry = self.entry_mut(id)?;
        entry.flags |= FLAG_DELETED;
        Ok(id)
    }

    /// Renomeia ou move uma entrada existente pelo seu FRN.
    pub fn rename_by_frn(
        &mut self,
        frn: u64,
        new_parent: u32,
        new_name: &str,
    ) -> Result<u32, IndexError> {
        let id = self
            .frn_map
            .get(&frn)
            .copied()
            .ok_or(IndexError::FrnNotFound(frn))?;
        if new_parent != NO_PARENT && (new_parent as usize) >= self.entries.len() {
            return Err(IndexError::ParentNotFound(new_parent));
        }

        let name_off = self.names.len() as u32;
        let name_len = new_name.len() as u16;

        self.names.extend_from_slice(new_name.as_bytes());
        let name_lower = new_name.to_lowercase();
        self.names_lc.extend_from_slice(name_lower.as_bytes());

        let entry = self.entry_mut(id)?;
        entry.parent = new_parent;
        entry.name_off = name_off;
        entry.name_len = name_len;

        Ok(id)
    }

    /// Atualiza tamanho, data de modificação e flags de uma entrada pelo FRN.
    pub fn update_by_frn(
        &mut self,
        frn: u64,
        size: u64,
        mtime: i64,
        flags: u16,
    ) -> Result<u32, IndexError> {
        let id = self
            .frn_map
            .get(&frn)
            .copied()
            .ok_or(IndexError::FrnNotFound(frn))?;
        let entry = self.entry_mut(id)?;
        entry.size = size;
        entry.mtime = mtime;
        entry.flags = flags;
        Ok(id)
    }

    /// Aplica uma mutação estruturada (`IndexChange`) no índice (ex.: USN Journal).
    pub fn apply_change(&mut self, change: &IndexChange) -> Result<(), IndexError> {
        match change {
            IndexChange::Insert {
                frn,
                parent_frn,
                name,
                flags,
                size,
                mtime,
            } => {
                let parent_id = if *parent_frn == 0 {
                    NO_PARENT
                } else {
                    self.id_by_frn(*parent_frn)
                        .ok_or(IndexError::FrnNotFound(*parent_frn))?
                };
                self.add_entry(*frn, parent_id, name, *flags, *size, *mtime)?;
            }
            IndexChange::Delete { frn } => {
                self.remove_by_frn(*frn)?;
            }
            IndexChange::Rename {
                frn,
                new_parent_frn,
                new_name,
            } => {
                let new_parent_id = if *new_parent_frn == 0 {
                    NO_PARENT
                } else {
                    self.id_by_frn(*new_parent_frn)
                        .ok_or(IndexError::FrnNotFound(*new_parent_frn))?
                };
                self.rename_by_frn(*frn, new_parent_id, new_name)?;
            }
            IndexChange::Modify {
                frn,
                size,
                mtime,
                flags,
            } => {
                self.update_by_frn(*frn, *size, *mtime, *flags)?;
            }
        }
        Ok(())
    }

    /// Reconstrói o caminho completo subindo pelos pais sob demanda (§2.2).
    pub fn build_path(&self, id: u32) -> Result<String, IndexError> {
        let mut buf = String::new();
        self.build_path_buf(id, &mut buf)?;
        Ok(buf)
    }

    /// Reconstrói o caminho completo gravando em um buffer reutilizável.
    pub fn build_path_buf(&self, id: u32, buf: &mut String) -> Result<(), IndexError> {
        buf.clear();
        let mut current = id;
        let mut chain = Vec::with_capacity(16);
        // Caminhos reais têm no máximo ~16 mil componentes (limite de 32 767 caracteres do Windows): um limite
        // fixo faz um ciclo falhar rápido, em vez de percorrer o índice inteiro (milhões de saltos).
        let max_hops = self.entries.len().min(MAX_PATH_HOPS);

        while current != NO_PARENT {
            if chain.len() > max_hops {
                return Err(IndexError::CycleDetected(id));
            }
            chain.push(current);
            let entry = self.entry(current)?;
            // Auto-pai (a raiz do NTFS aponta para si mesma, inclusive em snapshots antigos) marca o fim.
            current = if entry.parent == current {
                NO_PARENT
            } else {
                entry.parent
            };
        }

        if chain.is_empty() {
            return Err(IndexError::InvalidId(id));
        }

        // Reconstrói da raiz para a folha
        chain.reverse();
        for (i, &node_id) in chain.iter().enumerate() {
            let name = self.name(node_id)?;
            if i > 0 {
                let prev_name = self.name(chain[i - 1])?;
                // Se o anterior não terminar com separador nem for letra de unidade com ':' (ex: C:), adiciona separador
                if !prev_name.ends_with('\\') && !prev_name.ends_with('/') {
                    let is_unix = buf.contains('/') || (cfg!(not(windows)) && !buf.contains(':'));
                    let sep = if is_unix { '/' } else { '\\' };
                    buf.push(sep);
                }
            }
            buf.push_str(name);
        }

        Ok(())
    }

    /// Estima o uso total de memória (em bytes) das estruturas de dados alocadas.
    #[must_use]
    pub fn memory_usage(&self) -> usize {
        let entries_mem = self.entries.capacity() * std::mem::size_of::<Entry>();
        let names_mem = self.names.capacity();
        let names_lc_mem = self.names_lc.capacity();
        // HashMap tem overhead por bucket além de (K, V)
        let hashmap_mem =
            self.frn_map.capacity() * (std::mem::size_of::<u64>() + std::mem::size_of::<u32>() + 8);
        entries_mem + names_mem + names_lc_mem + hashmap_mem
    }

    /// Salva o estado completo do índice em formato binário versionado com checksum (§3, Issue #12).
    ///
    /// Grava sempre o formato atual (`FFSNAP02`, checksum por palavras de 8 bytes).
    pub fn save_snapshot<W: Write>(&self, writer: &mut W) -> io::Result<()> {
        let mut out = HashingWriter {
            writer,
            hash: SnapshotHash::V2(WordFnv::new()),
        };

        out.put(SNAPSHOT_MAGIC)?;
        let vol_bytes = self.volume.as_bytes();
        out.put(&(vol_bytes.len() as u16).to_le_bytes())?;
        out.put(vol_bytes)?;
        out.put(&self.journal_id.to_le_bytes())?;
        out.put(&self.last_usn.to_le_bytes())?;
        out.put(&(self.entries.len() as u64).to_le_bytes())?;
        out.put(&(self.names.len() as u64).to_le_bytes())?;
        out.put(&(self.names_lc.len() as u64).to_le_bytes())?;
        out.put(&(self.frn_map.len() as u64).to_le_bytes())?;

        // Registros de tamanho fixo vão em lotes: uma chamada de escrita e de hash por lote.
        let mut batch = Vec::with_capacity(SNAPSHOT_BATCH);
        for entry in &self.entries {
            batch.extend_from_slice(&entry.parent.to_le_bytes());
            batch.extend_from_slice(&entry.name_off.to_le_bytes());
            batch.extend_from_slice(&entry.size.to_le_bytes());
            batch.extend_from_slice(&entry.mtime.to_le_bytes());
            batch.extend_from_slice(&entry.name_len.to_le_bytes());
            batch.extend_from_slice(&entry.flags.to_le_bytes());
            batch.extend_from_slice(&entry._reserved.to_le_bytes());
            if batch.len() + ENTRY_BYTES > SNAPSHOT_BATCH {
                out.put(&batch)?;
                batch.clear();
            }
        }
        out.put(&batch)?;
        batch.clear();

        out.put(&self.names)?;
        out.put(&self.names_lc)?;

        for (&frn, &id) in &self.frn_map {
            batch.extend_from_slice(&frn.to_le_bytes());
            batch.extend_from_slice(&id.to_le_bytes());
            if batch.len() + FRN_BYTES > SNAPSHOT_BATCH {
                out.put(&batch)?;
                batch.clear();
            }
        }
        out.put(&batch)?;

        let checksum = out.hash.finish();
        out.writer.write_all(&checksum.to_le_bytes())
    }

    /// Carrega um snapshot binário do índice, validando integridade e versão (§3, Issue #12).
    ///
    /// Aceita o formato atual (`FFSNAP02`) e o anterior (`FFSNAP01`, checksum byte a byte).
    pub fn load_snapshot<R: Read>(reader: &mut R) -> io::Result<Self> {
        Self::load_snapshot_from(reader, None)
    }

    /// Carga com o tamanho total opcional (`total_len`): quando conhecido, os tamanhos do cabeçalho
    /// precisam bater exatamente com ele antes de alocar, e cada vetor é alocado de uma vez.
    fn load_snapshot_from<R: Read>(reader: &mut R, total_len: Option<u64>) -> io::Result<Self> {
        let invalid = |msg: &'static str| io::Error::new(io::ErrorKind::InvalidData, msg);

        let mut magic = [0u8; 8];
        reader.read_exact(&mut magic)?;
        let mut hash = if &magic == SNAPSHOT_MAGIC {
            SnapshotHash::V2(WordFnv::new())
        } else if &magic == SNAPSHOT_MAGIC_V1 {
            SnapshotHash::V1(FNV_OFFSET)
        } else {
            return Err(invalid(
                "formato de snapshot inválido ou cabeçalho incorreto",
            ));
        };
        hash.update(&magic);
        let mut input = HashingReader { reader, hash };

        let mut u16_buf = [0u8; 2];
        input.get(&mut u16_buf)?;
        let vol_len = u16::from_le_bytes(u16_buf) as usize;
        let mut vol_bytes = vec![0u8; vol_len];
        input.get(&mut vol_bytes)?;
        let volume = String::from_utf8(vol_bytes)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;

        let journal_id = input.u64()?;
        let last_usn = input.u64()? as i64;
        let entries_count = input.len_field()?;
        let names_len = input.len_field()?;
        let names_lc_len = input.len_field()?;
        let frn_count = input.len_field()?;

        // As contagens vêm do arquivo e ainda não foram validadas pelo checksum. Com o tamanho total
        // conhecido, elas precisam bater exatamente com ele; sem ele, a pré-alocação fica limitada a
        // `PREALLOC_CAP` e a memória só cresce conforme os dados realmente chegam.
        let body = entries_count
            .checked_mul(ENTRY_BYTES)
            .and_then(|n| n.checked_add(names_len))
            .and_then(|n| n.checked_add(names_lc_len))
            .and_then(|n| frn_count.checked_mul(FRN_BYTES)?.checked_add(n))
            .ok_or_else(|| invalid("snapshot com tamanhos inconsistentes"))?;
        let verified = match total_len {
            Some(total) => {
                let header = 8 + 2 + vol_len as u64 + 6 * 8;
                if header + body as u64 + 8 != total {
                    return Err(invalid("snapshot truncado ou com tamanhos inconsistentes"));
                }
                true
            }
            None => false,
        };
        let cap = |n: usize| if verified { n } else { n.min(PREALLOC_CAP) };

        let mut batch = vec![0u8; SNAPSHOT_BATCH];
        let mut entries = Vec::with_capacity(cap(entries_count));
        let mut left = entries_count;
        while left > 0 {
            let n = left.min(SNAPSHOT_BATCH / ENTRY_BYTES);
            let chunk = &mut batch[..n * ENTRY_BYTES];
            input.get(chunk)?;
            let (records, _) = chunk.as_chunks::<ENTRY_BYTES>();
            entries.extend(records.iter().map(|b| {
                let (head, tail) = b.split_at(16);
                Entry {
                    parent: u32::from_le_bytes([head[0], head[1], head[2], head[3]]),
                    name_off: u32::from_le_bytes([head[4], head[5], head[6], head[7]]),
                    size: u64::from_le_bytes(head[8..16].try_into().unwrap_or_default()),
                    mtime: i64::from_le_bytes(tail[0..8].try_into().unwrap_or_default()),
                    name_len: u16::from_le_bytes([tail[8], tail[9]]),
                    flags: u16::from_le_bytes([tail[10], tail[11]]),
                    _reserved: u32::from_le_bytes([tail[12], tail[13], tail[14], tail[15]]),
                }
            }));
            left -= n;
        }

        let names = input.vec(names_len, cap(names_len), &mut batch)?;
        let names_lc = input.vec(names_lc_len, cap(names_lc_len), &mut batch)?;

        let mut frn_map = FrnMap::with_capacity_and_hasher(cap(frn_count), Default::default());
        let mut left = frn_count;
        while left > 0 {
            let n = left.min(SNAPSHOT_BATCH / FRN_BYTES);
            let chunk = &mut batch[..n * FRN_BYTES];
            input.get(chunk)?;
            let (pairs, _) = chunk.as_chunks::<FRN_BYTES>();
            for p in pairs {
                let (frn, id) = p.split_at(8);
                frn_map.insert(
                    u64::from_le_bytes(frn.try_into().unwrap_or_default()),
                    u32::from_le_bytes(id.try_into().unwrap_or_default()),
                );
            }
            left -= n;
        }

        let mut checksum_buf = [0u8; 8];
        input.reader.read_exact(&mut checksum_buf)?;
        if input.hash.finish() != u64::from_le_bytes(checksum_buf) {
            return Err(invalid(
                "corrupção de dados: checksum do snapshot não confere",
            ));
        }

        Ok(Self {
            entries,
            names,
            names_lc,
            frn_map,
            hardlinks: HashMap::new(),
            allocated_sizes: Vec::new(),
            volume,
            journal_id,
            last_usn,
        })
    }

    /// Salva o snapshot em um arquivo em disco, de forma **atômica** e privada.
    ///
    /// Grava em um arquivo temporário vizinho, sincroniza e só então renomeia sobre o destino: uma queda no
    /// meio da gravação nunca deixa um snapshot truncado no lugar do anterior. O arquivo é criado com
    /// permissão `0o600` em Unix (o índice de nomes é dado sensível).
    pub fn save_snapshot_file<P: AsRef<std::path::Path>>(&self, path: P) -> io::Result<()> {
        let path = path.as_ref();
        let file_name = path.file_name().ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidInput, "caminho de snapshot inválido")
        })?;
        let mut tmp_name = file_name.to_os_string();
        tmp_name.push(format!(".tmp-{}", std::process::id()));
        let tmp = path.with_file_name(tmp_name);

        let result = (|| {
            let mut options = std::fs::OpenOptions::new();
            options.write(true).create(true).truncate(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.mode(0o600);
            }
            let file = options.open(&tmp)?;
            let mut writer = io::BufWriter::with_capacity(1 << 20, file);
            self.save_snapshot(&mut writer)?;
            writer.flush()?;
            let file = writer
                .into_inner()
                .map_err(io::IntoInnerError::into_error)?;
            file.sync_all()?;
            drop(file); // fecha antes do rename (obrigatório no Windows)
            std::fs::rename(&tmp, path)
        })();
        if result.is_err() {
            let _ = std::fs::remove_file(&tmp);
        }
        result
    }

    /// Carrega o snapshot a partir de um arquivo em disco.
    pub fn load_snapshot_file<P: AsRef<std::path::Path>>(path: P) -> io::Result<Self> {
        let mut file = std::fs::File::open(path)?;
        let total_len = file.metadata()?.len();
        Self::load_snapshot_from(&mut file, Some(total_len))
    }
}

/// Limite de pré-alocação ao carregar snapshots de tamanho desconhecido (contagens não confiáveis).
const PREALLOC_CAP: usize = 1 << 16;

/// Máximo de componentes ao reconstruir um caminho (acima disso, trata como ciclo).
const MAX_PATH_HOPS: usize = 32_768;

/// Bytes de uma entrada no snapshot.
const ENTRY_BYTES: usize = 32;
/// Bytes de um par FRN → ID no snapshot.
const FRN_BYTES: usize = 12;

/// Tamanho do lote de E/S do snapshot (múltiplo de 32 e de 12).
const SNAPSHOT_BATCH: usize = 96 * 2048;

const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

/// FNV-1a sobre palavras de 8 bytes (little-endian); os bytes que sobram no fim entram um a um.
///
/// SNAPSHOT-CHECKSUM-002: ~8× mais rápido que o FNV byte a byte do `FFSNAP01` (0,39 s → 0,05 s num
/// snapshot de 365 MB, medido em 2026-11-18). O resultado independe de como os dados são fatiados.
#[derive(Clone, Copy)]
struct WordFnv {
    hash: u64,
    tail: [u8; 8],
    tail_len: usize,
}

impl WordFnv {
    fn new() -> Self {
        Self {
            hash: FNV_OFFSET,
            tail: [0; 8],
            tail_len: 0,
        }
    }

    fn word(&mut self, w: [u8; 8]) {
        self.hash ^= u64::from_le_bytes(w);
        self.hash = self.hash.wrapping_mul(FNV_PRIME);
    }

    fn update(&mut self, mut data: &[u8]) {
        if self.tail_len > 0 {
            let take = (8 - self.tail_len).min(data.len());
            self.tail[self.tail_len..self.tail_len + take].copy_from_slice(&data[..take]);
            self.tail_len += take;
            data = &data[take..];
            if self.tail_len < 8 {
                return;
            }
            let w = self.tail;
            self.word(w);
            self.tail_len = 0;
        }
        let (words, rest) = data.as_chunks::<8>();
        for w in words {
            self.word(*w);
        }
        self.tail[..rest.len()].copy_from_slice(rest);
        self.tail_len = rest.len();
    }

    fn finish(&self) -> u64 {
        fnv_bytes(self.hash, &self.tail[..self.tail_len])
    }
}

/// FNV-1a byte a byte (checksum do `FFSNAP01`).
fn fnv_bytes(mut hash: u64, data: &[u8]) -> u64 {
    for &byte in data {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    hash
}

/// Checksum de uma versão do formato de snapshot.
enum SnapshotHash {
    V1(u64),
    V2(WordFnv),
}

impl SnapshotHash {
    fn update(&mut self, data: &[u8]) {
        match self {
            Self::V1(h) => *h = fnv_bytes(*h, data),
            Self::V2(h) => h.update(data),
        }
    }

    fn finish(&self) -> u64 {
        match self {
            Self::V1(h) => *h,
            Self::V2(h) => h.finish(),
        }
    }
}

/// Escreve e acumula o checksum do que foi escrito.
struct HashingWriter<'a, W: Write> {
    writer: &'a mut W,
    hash: SnapshotHash,
}

impl<W: Write> HashingWriter<'_, W> {
    fn put(&mut self, data: &[u8]) -> io::Result<()> {
        self.writer.write_all(data)?;
        self.hash.update(data);
        Ok(())
    }
}

/// Lê e acumula o checksum do que foi lido.
struct HashingReader<'a, R: Read> {
    reader: &'a mut R,
    hash: SnapshotHash,
}

impl<R: Read> HashingReader<'_, R> {
    fn get(&mut self, buf: &mut [u8]) -> io::Result<()> {
        self.reader.read_exact(buf)?;
        self.hash.update(buf);
        Ok(())
    }

    fn u64(&mut self) -> io::Result<u64> {
        let mut buf = [0u8; 8];
        self.get(&mut buf)?;
        Ok(u64::from_le_bytes(buf))
    }

    /// Campo de tamanho: um `u64` que precisa caber em `usize`.
    fn len_field(&mut self) -> io::Result<usize> {
        usize::try_from(self.u64()?)
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "snapshot com tamanho absurdo"))
    }

    /// Lê `len` bytes num vetor que começa com capacidade `cap`, em lotes do tamanho de `batch`.
    fn vec(&mut self, len: usize, cap: usize, batch: &mut [u8]) -> io::Result<Vec<u8>> {
        let mut out = Vec::with_capacity(cap);
        let mut left = len;
        while left > 0 {
            let n = left.min(batch.len());
            self.get(&mut batch[..n])?;
            out.extend_from_slice(&batch[..n]);
            left -= n;
        }
        Ok(out)
    }
}

/// Hasher dos FRNs: multiplicação dobrada de 64×64 → 128 bits.
///
/// As chaves são números de arquivo do próprio volume (não vêm de rede nem de usuário remoto), então a
/// proteção contra HashDoS do SipHash padrão não é necessária; ela custava ~40% a mais para montar o mapa
/// de 4 milhões de FRNs (0,36 s → 0,26 s, medido em 2026-11-18).
#[derive(Default, Clone, Copy)]
pub struct FrnHasher(u64);

impl Hasher for FrnHasher {
    fn finish(&self) -> u64 {
        self.0
    }

    fn write(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.write_u64(u64::from(b));
        }
    }

    fn write_u64(&mut self, n: u64) {
        let p = u128::from(self.0 ^ n) * 0x9E37_79B9_7F4A_7C15;
        self.0 = (p as u64) ^ ((p >> 64) as u64);
    }
}

/// Mapa FRN → ID de entrada.
pub type FrnMap = HashMap<u64, u32, BuildHasherDefault<FrnHasher>>;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entry::{FLAG_DIRECTORY, FLAG_FILE};

    #[test]
    fn test_index_insert_and_path() {
        let mut index = FileIndex::new();
        // Raiz "C:"
        let root = index
            .add_entry(100, NO_PARENT, "C:", FLAG_DIRECTORY, 0, 1000)
            .unwrap();
        assert_eq!(root, 0);

        // Pasta "Windows"
        let windows = index
            .add_entry(101, root, "Windows", FLAG_DIRECTORY, 0, 1001)
            .unwrap();
        assert_eq!(windows, 1);

        // Subpasta "System32"
        let sys32 = index
            .add_entry(102, windows, "System32", FLAG_DIRECTORY, 0, 1002)
            .unwrap();

        // Arquivo "cmd.exe"
        let cmd = index
            .add_entry(103, sys32, "cmd.exe", FLAG_FILE, 300_000, 1003)
            .unwrap();

        assert_eq!(index.name(cmd).unwrap(), "cmd.exe");
        assert_eq!(index.name_lc(cmd).unwrap(), "cmd.exe");
        assert_eq!(
            index.build_path(cmd).unwrap(),
            "C:\\Windows\\System32\\cmd.exe"
        );
    }

    #[test]
    fn test_index_rename_and_move() {
        let mut index = FileIndex::new();
        let root = index
            .add_entry(10, NO_PARENT, "C:", FLAG_DIRECTORY, 0, 0)
            .unwrap();
        let folder1 = index
            .add_entry(11, root, "Folder1", FLAG_DIRECTORY, 0, 0)
            .unwrap();
        let folder2 = index
            .add_entry(12, root, "Folder2", FLAG_DIRECTORY, 0, 0)
            .unwrap();
        let file = index
            .add_entry(13, folder1, "file.txt", FLAG_FILE, 100, 0)
            .unwrap();

        assert_eq!(index.build_path(file).unwrap(), "C:\\Folder1\\file.txt");

        // Move para Folder2 e renomeia para doc.txt
        index.rename_by_frn(13, folder2, "doc.txt").unwrap();
        assert_eq!(index.name(file).unwrap(), "doc.txt");
        assert_eq!(index.name_lc(file).unwrap(), "doc.txt");
        assert_eq!(index.build_path(file).unwrap(), "C:\\Folder2\\doc.txt");
    }

    #[test]
    fn test_index_delete() {
        let mut index = FileIndex::new();
        let root = index
            .add_entry(1, NO_PARENT, "C:", FLAG_DIRECTORY, 0, 0)
            .unwrap();
        let file = index
            .add_entry(2, root, "test.txt", FLAG_FILE, 50, 0)
            .unwrap();

        assert!(!index.entry(file).unwrap().is_deleted());
        index.remove_by_frn(2).unwrap();
        assert!(index.entry(file).unwrap().is_deleted());
        assert!(index.id_by_frn(2).is_none());
    }

    #[test]
    fn test_index_apply_change() {
        let mut index = FileIndex::new();
        let root_change = IndexChange::Insert {
            frn: 1,
            parent_frn: 0,
            name: "C:".into(),
            flags: FLAG_DIRECTORY,
            size: 0,
            mtime: 10,
        };
        index.apply_change(&root_change).unwrap();

        let file_change = IndexChange::Insert {
            frn: 2,
            parent_frn: 1,
            name: "hello.rs".into(),
            flags: FLAG_FILE,
            size: 200,
            mtime: 20,
        };
        index.apply_change(&file_change).unwrap();

        let id = index.id_by_frn(2).unwrap();
        assert_eq!(index.build_path(id).unwrap(), "C:\\hello.rs");

        let mod_change = IndexChange::Modify {
            frn: 2,
            size: 500,
            mtime: 30,
            flags: FLAG_FILE,
        };
        index.apply_change(&mod_change).unwrap();
        assert_eq!(index.entry(id).unwrap().size, 500);

        let del_change = IndexChange::Delete { frn: 2 };
        index.apply_change(&del_change).unwrap();
        assert!(index.entry(id).unwrap().is_deleted());
    }

    #[test]
    fn test_cycle_detection() {
        let mut index = FileIndex::new();
        let a = index
            .add_entry(1, NO_PARENT, "a", FLAG_DIRECTORY, 0, 0)
            .unwrap();
        let b = index.add_entry(2, a, "b", FLAG_DIRECTORY, 0, 0).unwrap();
        // Força ciclo: a aponta para b
        index.entry_mut(a).unwrap().parent = b;

        assert_eq!(index.build_path(b), Err(IndexError::CycleDetected(b)));
    }

    #[test]
    fn word_fnv_does_not_depend_on_how_data_is_split() {
        let data: Vec<u8> = (0..1000u32).map(|i| (i * 31 % 251) as u8).collect();
        let mut whole = WordFnv::new();
        whole.update(&data);
        for split in [1usize, 3, 7, 8, 9, 13, 64, 999] {
            let mut parts = WordFnv::new();
            for chunk in data.chunks(split) {
                parts.update(chunk);
            }
            assert_eq!(parts.finish(), whole.finish(), "fatias de {split}");
        }
    }

    #[test]
    fn snapshot_file_rejects_size_mismatch_before_allocating() {
        let mut index = FileIndex::new();
        index
            .add_entry(1, NO_PARENT, "C:", FLAG_DIRECTORY, 0, 0)
            .unwrap();
        let mut bytes = Vec::new();
        index.save_snapshot(&mut bytes).unwrap();
        let total = bytes.len() as u64;
        assert!(FileIndex::load_snapshot_from(&mut &bytes[..], Some(total)).is_ok());
        assert!(FileIndex::load_snapshot_from(&mut &bytes[..], Some(total + 1)).is_err());
        assert!(FileIndex::load_snapshot_from(&mut &bytes[..], Some(total - 1)).is_err());
    }

    #[test]
    fn test_snapshot_roundtrip_and_checksum_verification() {
        let mut index = FileIndex::new();
        index.volume = "C:".to_string();
        index.journal_id = 998877;
        index.last_usn = 12345678;

        let root = index
            .add_entry(1, NO_PARENT, "C:", FLAG_DIRECTORY, 0, 100)
            .unwrap();
        let f1 = index
            .add_entry(2, root, "arquivo.txt", FLAG_FILE, 1024, 200)
            .unwrap();
        let d1 = index
            .add_entry(3, root, "src", FLAG_DIRECTORY, 0, 300)
            .unwrap();
        let f2 = index
            .add_entry(4, d1, "main.rs", FLAG_FILE, 2048, 400)
            .unwrap();

        let mut buffer = Vec::new();
        index.save_snapshot(&mut buffer).unwrap();
        assert!(!buffer.is_empty());

        // Carrega do snapshot
        let mut reader = io::Cursor::new(&buffer);
        let loaded = FileIndex::load_snapshot(&mut reader).unwrap();

        assert_eq!(loaded.volume, "C:");
        assert_eq!(loaded.journal_id, 998877);
        assert_eq!(loaded.last_usn, 12345678);
        assert_eq!(loaded.len(), index.len());
        assert_eq!(loaded.build_path(f1).unwrap(), "C:\\arquivo.txt");
        assert_eq!(loaded.build_path(f2).unwrap(), "C:\\src\\main.rs");
        assert_eq!(loaded.entry(f1).unwrap().size, 1024);

        // Teste de corrupção: altera um byte de payload
        let mut corrupted = buffer.clone();
        let len = corrupted.len();
        corrupted[len - 15] ^= 0xFF;
        let mut corrupted_reader = io::Cursor::new(&corrupted);
        assert!(FileIndex::load_snapshot(&mut corrupted_reader).is_err());
    }
}
