//! Estrutura principal do índice (Struct-of-Arrays com arenas de nomes).

use std::collections::HashMap;
use std::io::{self, Read, Write};

use crate::entry::{Entry, FLAG_DELETED, NO_PARENT};
use crate::source::{IndexChange, IndexError};

const SNAPSHOT_MAGIC: &[u8; 8] = b"FFSNAP01";

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
    pub frn_map: HashMap<u64, u32>,
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
            frn_map: HashMap::with_capacity(entries_cap),
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
        let max_hops = self.entries.len();

        while current != NO_PARENT {
            if chain.len() > max_hops {
                return Err(IndexError::CycleDetected(id));
            }
            chain.push(current);
            let entry = self.entry(current)?;
            current = entry.parent;
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
    pub fn save_snapshot<W: Write>(&self, writer: &mut W) -> io::Result<()> {
        let mut hasher = 0xcbf29ce484222325u64;
        let mut update_hash = |data: &[u8]| {
            for &byte in data {
                hasher ^= byte as u64;
                hasher = hasher.wrapping_mul(0x100000001b3u64);
            }
        };

        writer.write_all(SNAPSHOT_MAGIC)?;
        update_hash(SNAPSHOT_MAGIC);

        let vol_bytes = self.volume.as_bytes();
        let vol_len = vol_bytes.len() as u16;
        writer.write_all(&vol_len.to_le_bytes())?;
        update_hash(&vol_len.to_le_bytes());
        writer.write_all(vol_bytes)?;
        update_hash(vol_bytes);

        let journal_id_bytes = self.journal_id.to_le_bytes();
        writer.write_all(&journal_id_bytes)?;
        update_hash(&journal_id_bytes);

        let last_usn_bytes = self.last_usn.to_le_bytes();
        writer.write_all(&last_usn_bytes)?;
        update_hash(&last_usn_bytes);

        let entries_count = self.entries.len() as u64;
        writer.write_all(&entries_count.to_le_bytes())?;
        update_hash(&entries_count.to_le_bytes());

        let names_len = self.names.len() as u64;
        writer.write_all(&names_len.to_le_bytes())?;
        update_hash(&names_len.to_le_bytes());

        let names_lc_len = self.names_lc.len() as u64;
        writer.write_all(&names_lc_len.to_le_bytes())?;
        update_hash(&names_lc_len.to_le_bytes());

        let frn_count = self.frn_map.len() as u64;
        writer.write_all(&frn_count.to_le_bytes())?;
        update_hash(&frn_count.to_le_bytes());

        for entry in &self.entries {
            let mut buf = [0u8; 32];
            buf[0..4].copy_from_slice(&entry.parent.to_le_bytes());
            buf[4..8].copy_from_slice(&entry.name_off.to_le_bytes());
            buf[8..16].copy_from_slice(&entry.size.to_le_bytes());
            buf[16..24].copy_from_slice(&entry.mtime.to_le_bytes());
            buf[24..26].copy_from_slice(&entry.name_len.to_le_bytes());
            buf[26..28].copy_from_slice(&entry.flags.to_le_bytes());
            buf[28..32].copy_from_slice(&entry._reserved.to_le_bytes());
            writer.write_all(&buf)?;
            update_hash(&buf);
        }

        writer.write_all(&self.names)?;
        update_hash(&self.names);

        writer.write_all(&self.names_lc)?;
        update_hash(&self.names_lc);

        for (&frn, &id) in &self.frn_map {
            let mut frn_buf = [0u8; 12];
            frn_buf[0..8].copy_from_slice(&frn.to_le_bytes());
            frn_buf[8..12].copy_from_slice(&id.to_le_bytes());
            writer.write_all(&frn_buf)?;
            update_hash(&frn_buf);
        }

        writer.write_all(&hasher.to_le_bytes())?;
        Ok(())
    }

    /// Carrega um snapshot binário do índice, validando integridade e versão (§3, Issue #12).
    pub fn load_snapshot<R: Read>(reader: &mut R) -> io::Result<Self> {
        let mut hasher = 0xcbf29ce484222325u64;
        let mut update_hash = |data: &[u8]| {
            for &byte in data {
                hasher ^= byte as u64;
                hasher = hasher.wrapping_mul(0x100000001b3u64);
            }
        };

        let mut magic = [0u8; 8];
        reader.read_exact(&mut magic)?;
        if &magic != SNAPSHOT_MAGIC {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "formato de snapshot inválido ou cabeçalho incorreto",
            ));
        }
        update_hash(&magic);

        let mut vol_len_buf = [0u8; 2];
        reader.read_exact(&mut vol_len_buf)?;
        update_hash(&vol_len_buf);
        let vol_len = u16::from_le_bytes(vol_len_buf) as usize;

        let vol_bytes = read_exact_vec(reader, vol_len)?;
        update_hash(&vol_bytes);
        let volume = String::from_utf8(vol_bytes)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;

        let mut u64_buf = [0u8; 8];

        reader.read_exact(&mut u64_buf)?;
        update_hash(&u64_buf);
        let journal_id = u64::from_le_bytes(u64_buf);

        reader.read_exact(&mut u64_buf)?;
        update_hash(&u64_buf);
        let last_usn = i64::from_le_bytes(u64_buf);

        reader.read_exact(&mut u64_buf)?;
        update_hash(&u64_buf);
        let entries_count = u64::from_le_bytes(u64_buf) as usize;

        reader.read_exact(&mut u64_buf)?;
        update_hash(&u64_buf);
        let names_len = u64::from_le_bytes(u64_buf) as usize;

        reader.read_exact(&mut u64_buf)?;
        update_hash(&u64_buf);
        let names_lc_len = u64::from_le_bytes(u64_buf) as usize;

        reader.read_exact(&mut u64_buf)?;
        update_hash(&u64_buf);
        let frn_count = u64::from_le_bytes(u64_buf) as usize;

        // As contagens vêm do arquivo e ainda não foram validadas pelo checksum: nunca pré-alocar
        // além de `PREALLOC_CAP`; a memória só cresce conforme os dados realmente existem.
        let mut entries = Vec::with_capacity(entries_count.min(PREALLOC_CAP));
        let mut entry_buf = [0u8; 32];
        for _ in 0..entries_count {
            reader.read_exact(&mut entry_buf)?;
            update_hash(&entry_buf);
            let parent = u32::from_le_bytes(entry_buf[0..4].try_into().unwrap());
            let name_off = u32::from_le_bytes(entry_buf[4..8].try_into().unwrap());
            let size = u64::from_le_bytes(entry_buf[8..16].try_into().unwrap());
            let mtime = i64::from_le_bytes(entry_buf[16..24].try_into().unwrap());
            let name_len = u16::from_le_bytes(entry_buf[24..26].try_into().unwrap());
            let flags = u16::from_le_bytes(entry_buf[26..28].try_into().unwrap());
            let _reserved = u32::from_le_bytes(entry_buf[28..32].try_into().unwrap());
            entries.push(Entry {
                parent,
                name_off,
                size,
                mtime,
                name_len,
                flags,
                _reserved,
            });
        }

        let names = read_exact_vec(reader, names_len)?;
        update_hash(&names);

        let names_lc = read_exact_vec(reader, names_lc_len)?;
        update_hash(&names_lc);

        let mut frn_map = HashMap::with_capacity(frn_count.min(PREALLOC_CAP));
        let mut frn_buf = [0u8; 12];
        for _ in 0..frn_count {
            reader.read_exact(&mut frn_buf)?;
            update_hash(&frn_buf);
            let frn = u64::from_le_bytes(frn_buf[0..8].try_into().unwrap());
            let id = u32::from_le_bytes(frn_buf[8..12].try_into().unwrap());
            frn_map.insert(frn, id);
        }

        let mut checksum_buf = [0u8; 8];
        reader.read_exact(&mut checksum_buf)?;
        let expected_checksum = u64::from_le_bytes(checksum_buf);
        if hasher != expected_checksum {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
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
        let file = std::fs::File::open(path)?;
        let mut reader = io::BufReader::new(file);
        Self::load_snapshot(&mut reader)
    }
}

/// Limite de pré-alocação ao carregar snapshots (contagens do arquivo não são confiáveis).
const PREALLOC_CAP: usize = 1 << 16;

/// Lê exatamente `len` bytes sem alocar `len` antecipadamente (o tamanho vem de dados não confiáveis).
fn read_exact_vec<R: Read>(reader: &mut R, len: usize) -> io::Result<Vec<u8>> {
    let mut buf = Vec::new();
    reader.by_ref().take(len as u64).read_to_end(&mut buf)?;
    if buf.len() != len {
        return Err(io::Error::new(
            io::ErrorKind::UnexpectedEof,
            "snapshot truncado",
        ));
    }
    Ok(buf)
}

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
