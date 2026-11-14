//! `ff-size`
//!
//! Agregação de tamanho por pasta (uso de disco) a partir do índice.
//! Permite relatórios rápidos estilo `du` ou WizTree com ordenação por tamanho e controle de profundidade.

use ff_index::entry::NO_PARENT;
use ff_index::FileIndex;
use serde::Serialize;

/// Uso de disco agregado para um diretório.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DirUsage {
    /// ID do diretório no índice.
    pub id: u32,
    /// Tamanho total acumulado (em bytes) de todos os arquivos contidos recursivamente.
    pub total_size: u64,
    /// Quantidade de arquivos contidos recursivamente.
    pub file_count: u64,
    /// Quantidade de subdiretórios contidos recursivamente.
    pub dir_count: u64,
    /// Profundidade relativa ao diretório raiz consultado.
    pub depth: u32,
}

/// Sumário consolidado de uso de disco.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DiskUsageSummary {
    /// Tamanho total da árvore em bytes.
    pub total_size: u64,
    /// Contagem total de arquivos.
    pub total_files: u64,
    /// Contagem total de diretórios.
    pub total_dirs: u64,
    /// Maiores diretórios ordenados por tamanho decrescente.
    pub items: Vec<DirUsage>,
}

/// Agregador de tamanhos de diretórios a partir do `FileIndex`.
pub struct SizeAggregator;

impl SizeAggregator {
    /// Calcula a agregação de uso de disco para o índice completo ou uma subárvore.
    ///
    /// - `root_id`: ID da raiz a consultar (se `None`, assume todas as raízes).
    /// - `max_depth`: Limite de profundidade relativa (ex.: `Some(1)` para filhos imediatos).
    /// - `top_n`: Quantidade máxima de itens a retornar (ordenados por tamanho decrescente).
    #[must_use]
    pub fn aggregate(
        index: &FileIndex,
        root_id: Option<u32>,
        max_depth: Option<u32>,
        top_n: usize,
    ) -> DiskUsageSummary {
        let n = index.entries.len();
        let mut dir_sizes = vec![0u64; n];
        let mut dir_files = vec![0u64; n];
        let mut dir_subdirs = vec![0u64; n];

        // 1. Acumular tamanho subindo pelos ponteiros parent
        for entry in index.entries.iter() {
            if entry.is_deleted() {
                continue;
            }

            if entry.is_file() {
                let size = entry.size;
                let mut curr = entry.parent;
                let mut hops = 0;
                while curr != NO_PARENT && (curr as usize) < n && hops < 1024 {
                    dir_sizes[curr as usize] += size;
                    dir_files[curr as usize] += 1;
                    curr = index.entries[curr as usize].parent;
                    hops += 1;
                }
            } else if entry.is_dir() {
                let mut curr = entry.parent;
                let mut hops = 0;
                while curr != NO_PARENT && (curr as usize) < n && hops < 1024 {
                    dir_subdirs[curr as usize] += 1;
                    curr = index.entries[curr as usize].parent;
                    hops += 1;
                }
            }
        }

        // 2. Determinar profundidade relativa e pertencimento à raiz
        let mut depths: Vec<Option<u32>> = vec![None; n];

        if let Some(target_root) = root_id {
            if (target_root as usize) < n && index.entries[target_root as usize].is_dir() {
                depths[target_root as usize] = Some(0);
            }
        } else {
            // Sem root especificado, entradas sem parent estão em depth 0
            for (id, entry) in index.entries.iter().enumerate() {
                if entry.parent == NO_PARENT && entry.is_dir() {
                    depths[id] = Some(0);
                }
            }
        }

        // Propagar profundidades para baixo
        for (id, d_slot) in depths.iter_mut().enumerate() {
            let entry = &index.entries[id];
            if !entry.is_dir() || entry.is_deleted() {
                continue;
            }
            if d_slot.is_none() {
                // Tenta resolver calculando a distância até o target_root ou NO_PARENT
                let mut d = 0;
                let mut curr = id as u32;
                let mut reached_root = false;
                let mut hops = 0;

                while curr != NO_PARENT && (curr as usize) < n && hops < 1024 {
                    if let Some(target) = root_id {
                        if curr == target {
                            reached_root = true;
                            break;
                        }
                    } else if index.entries[curr as usize].parent == NO_PARENT {
                        reached_root = true;
                        break;
                    }

                    curr = index.entries[curr as usize].parent;
                    d += 1;
                    hops += 1;
                }

                if reached_root {
                    *d_slot = Some(d);
                }
            }
        }

        let mut items = Vec::new();

        let (total_size, total_files, total_dirs) = if let Some(target_root) = root_id {
            if (target_root as usize) < n {
                (
                    dir_sizes[target_root as usize],
                    dir_files[target_root as usize],
                    dir_subdirs[target_root as usize],
                )
            } else {
                (0, 0, 0)
            }
        } else {
            let mut s = 0;
            let mut f = 0;
            let mut d = 0;
            for id in 0..n {
                let entry = &index.entries[id];
                if entry.parent == NO_PARENT {
                    if entry.is_dir() {
                        s += dir_sizes[id];
                        f += dir_files[id];
                        d += 1 + dir_subdirs[id];
                    } else if entry.is_file() {
                        s += entry.size;
                        f += 1;
                    }
                }
            }
            (s, f, d)
        };

        for id in 0..n {
            let entry = &index.entries[id];
            if !entry.is_dir() || entry.is_deleted() {
                continue;
            }

            if let Some(depth) = depths[id] {
                if let Some(max_d) = max_depth {
                    if depth > max_d {
                        continue;
                    }
                }

                items.push(DirUsage {
                    id: id as u32,
                    total_size: dir_sizes[id],
                    file_count: dir_files[id],
                    dir_count: dir_subdirs[id],
                    depth,
                });
            }
        }

        // Ordenar por tamanho decrescente
        items.sort_by_key(|a| std::cmp::Reverse(a.total_size));

        if items.len() > top_n {
            items.truncate(top_n);
        }

        DiskUsageSummary {
            total_size,
            total_files,
            total_dirs,
            items,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ff_index::entry::{FLAG_DIRECTORY, FLAG_FILE};

    #[test]
    fn test_empty_index_aggregation() {
        let index = FileIndex::new();
        let summary = SizeAggregator::aggregate(&index, None, None, 10);
        assert_eq!(summary.total_size, 0);
        assert_eq!(summary.total_files, 0);
        assert_eq!(summary.total_dirs, 0);
        assert!(summary.items.is_empty());
    }

    #[test]
    fn test_tree_aggregation() {
        let mut index = FileIndex::new();

        // root (dir)
        let root = index
            .add_entry(10, NO_PARENT, "root", FLAG_DIRECTORY, 0, 0)
            .unwrap();

        // root/dir_a (dir)
        let dir_a = index
            .add_entry(20, root, "dir_a", FLAG_DIRECTORY, 0, 0)
            .unwrap();

        // root/dir_b (dir)
        let dir_b = index
            .add_entry(30, root, "dir_b", FLAG_DIRECTORY, 0, 0)
            .unwrap();

        // root/dir_a/file1.txt (100 bytes)
        let _f1 = index
            .add_entry(40, dir_a, "file1.txt", FLAG_FILE, 100, 0)
            .unwrap();

        // root/dir_a/file2.txt (200 bytes)
        let _f2 = index
            .add_entry(50, dir_a, "file2.txt", FLAG_FILE, 200, 0)
            .unwrap();

        // root/dir_b/file3.bin (500 bytes)
        let _f3 = index
            .add_entry(60, dir_b, "file3.bin", FLAG_FILE, 500, 0)
            .unwrap();

        let summary = SizeAggregator::aggregate(&index, Some(root), None, 10);

        assert_eq!(summary.total_size, 800);
        assert_eq!(summary.total_files, 3);
        assert_eq!(summary.total_dirs, 2);

        // root deve ter 800, dir_b 500, dir_a 300
        assert_eq!(summary.items.len(), 3);
        assert_eq!(summary.items[0].id, root);
        assert_eq!(summary.items[0].total_size, 800);
        assert_eq!(summary.items[1].id, dir_b);
        assert_eq!(summary.items[1].total_size, 500);
        assert_eq!(summary.items[2].id, dir_a);
        assert_eq!(summary.items[2].total_size, 300);
    }

    #[test]
    fn test_depth_filter() {
        let mut index = FileIndex::new();

        let root = index
            .add_entry(10, NO_PARENT, "root", FLAG_DIRECTORY, 0, 0)
            .unwrap();
        let sub = index
            .add_entry(20, root, "sub", FLAG_DIRECTORY, 0, 0)
            .unwrap();
        let deep = index
            .add_entry(30, sub, "deep", FLAG_DIRECTORY, 0, 0)
            .unwrap();
        let _file = index
            .add_entry(40, deep, "file.txt", FLAG_FILE, 1000, 0)
            .unwrap();

        // Apenas profundidade <= 1 (root e sub)
        let summary = SizeAggregator::aggregate(&index, Some(root), Some(1), 10);
        assert_eq!(summary.items.len(), 2);
        assert_eq!(summary.items[0].id, root);
        assert_eq!(summary.items[1].id, sub);
        assert!(!summary.items.iter().any(|item| item.id == deep));
    }
}
