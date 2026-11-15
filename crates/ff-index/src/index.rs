//! Estrutura principal do índice (Struct-of-Arrays com arenas de nomes).

use std::collections::HashMap;

use crate::entry::{Entry, FLAG_DELETED, NO_PARENT};
use crate::source::{IndexChange, IndexError};

/// Índice de arquivos em memória otimizado para varredura e busca instantânea (§2.2).
#[derive(Debug, Default)]
pub struct FileIndex {
    /// Entradas do índice (ID = índice no vetor).
    pub entries: Vec<Entry>,
    /// Arena contínua com todos os nomes de arquivo em UTF-8.
    pub names: Vec<u8>,
    /// Arena paralela com todos os nomes já em minúsculas (UTF-8) para busca case-insensitive.
    pub names_lc: Vec<u8>,
    /// Mapeamento de NTFS File Reference Number (FRN) para o ID da entrada.
    pub frn_map: HashMap<u64, u32>,
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
        }
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
        self.entries.push(Entry {
            parent,
            name_off,
            size,
            mtime,
            name_len,
            flags,
            _reserved: 0,
        });

        if frn != 0 {
            self.frn_map.insert(frn, id);
        }

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
                    #[cfg(windows)]
                    buf.push('\\');
                    #[cfg(not(windows))]
                    buf.push('/');
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
}
