//! `ff-size`
//!
//! Agregação de tamanho por pasta (uso de disco) a partir do índice.
//! Permite relatórios rápidos estilo `du` ou WizTree com ordenação por tamanho e controle de profundidade.

use ff_index::entry::NO_PARENT;
use ff_index::FileIndex;
use serde::Serialize;

pub mod cleanup;
pub mod dupes;
pub mod simulation;
pub mod treemap;

pub use cleanup::{analyze_cleanup_opportunity, CleanupAdvice, CleanupCategory};
pub use dupes::{
    find_visual_duplicates, DupeScanProgress, VisualDupeFile, VisualDupeGroup, VisualDupeOptions,
};
pub use simulation::{RecoverySimulation, ReviewCandidateItem, ReviewSimulationSummary};

/// Métricas distintas de tamanho e projeção de recuperação em disco (TM-03).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct SizeMetrics {
    /// Tamanho nominal/lógico dos fluxos de dados em bytes.
    pub logical_size: u64,
    /// Espaço alocado em disco em bytes (ou `None` se desconhecido/não disponível).
    pub allocated_size: Option<u64>,
    /// Estimativa conservadora de espaço efetivamente liberável se excluído (ou `None` se desconhecido).
    pub reclaimable_size: Option<u64>,
}

/// Extrai métricas de tamanho para um arquivo individual a partir do índice e seus atributos (TM-03).
#[must_use]
pub fn file_metrics(index: &FileIndex, file_id: u32) -> SizeMetrics {
    if (file_id as usize) >= index.entries.len() {
        return SizeMetrics {
            logical_size: 0,
            allocated_size: None,
            reclaimable_size: None,
        };
    }

    let entry = &index.entries[file_id as usize];
    let logical_size = entry.size;

    let allocated_size = if entry.is_dir() {
        None
    } else if let Some(alloc) = index.allocated_size(file_id) {
        Some(alloc)
    } else if entry.is_offline() {
        Some(0)
    } else if entry.is_sparse() || entry.is_compressed() {
        None
    } else {
        Some(if logical_size == 0 {
            0
        } else {
            logical_size.div_ceil(4096) * 4096
        })
    };

    let reclaimable_size = if entry.is_offline() {
        // Placeholders na nuvem: não liberam espaço no disco local
        Some(0)
    } else if entry.is_hardlink() {
        // Hard links: deletar um link individual não libera clusters no disco se houver outros links
        Some(0)
    } else {
        allocated_size
    };

    SizeMetrics {
        logical_size,
        allocated_size,
        reclaimable_size,
    }
}

/// Uso de disco agregado para um diretório.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DirUsage {
    /// ID do diretório no índice.
    pub id: u32,
    /// Tamanho total acumulado (em bytes) de todos os arquivos contidos recursivamente.
    pub total_size: u64,
    /// Espaço alocado acumulado em disco (ou `None` se desconhecido).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub allocated_size: Option<u64>,
    /// Recuperação estimada acumulada (ou `None` se desconhecido).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reclaimable_size: Option<u64>,
    /// Quantidade de arquivos contidos recursivamente.
    pub file_count: u64,
    /// Quantidade de subdiretórios contidos recursivamente.
    pub dir_count: u64,
    /// Profundidade relativa ao diretório raiz consultado.
    pub depth: u32,
}

/// Uso de disco de um arquivo individual.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FileUsage {
    /// ID do arquivo no índice.
    pub id: u32,
    /// Tamanho em bytes.
    pub size: u64,
}

/// Sumário consolidado de uso de disco.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DiskUsageSummary {
    /// Tamanho total da árvore em bytes.
    pub total_size: u64,
    /// Espaço alocado total da árvore em bytes (ou `None` se desconhecido).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub total_allocated_size: Option<u64>,
    /// Recuperação estimada total da árvore em bytes (ou `None` se desconhecido).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub total_reclaimable_size: Option<u64>,
    /// Contagem total de arquivos.
    pub total_files: u64,
    /// Contagem total de diretórios.
    pub total_dirs: u64,
    /// Maiores diretórios ordenados por tamanho decrescente.
    pub items: Vec<DirUsage>,
    /// Maiores arquivos individuais ordenados por tamanho decrescente.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub top_files: Vec<FileUsage>,
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

        let tree = IncrementalSizeTree::build(index);

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
                    allocated_size: tree.dir_allocated_size(id as u32),
                    reclaimable_size: tree.dir_reclaimable_size(id as u32),
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

        // 4. Coletar maiores arquivos
        let mut top_files: Vec<FileUsage> = Vec::new();
        for (id, entry) in index.entries.iter().enumerate() {
            if entry.is_file() && !entry.is_deleted() {
                if let Some(target_root) = root_id {
                    let mut curr = entry.parent;
                    let mut belongs = false;
                    let mut hops = 0;
                    while curr != NO_PARENT && (curr as usize) < n && hops < 1024 {
                        if curr == target_root {
                            belongs = true;
                            break;
                        }
                        curr = index.entries[curr as usize].parent;
                        hops += 1;
                    }
                    if !belongs {
                        continue;
                    }
                }
                top_files.push(FileUsage {
                    id: id as u32,
                    size: entry.size,
                });
            }
        }
        top_files.sort_by_key(|f| std::cmp::Reverse(f.size));
        if top_files.len() > top_n {
            top_files.truncate(top_n);
        }

        let (total_allocated_size, total_reclaimable_size) = if let Some(target_root) = root_id {
            if (target_root as usize) < n {
                (
                    tree.dir_allocated_size(target_root),
                    tree.dir_reclaimable_size(target_root),
                )
            } else {
                (Some(0), Some(0))
            }
        } else {
            (
                Some(tree.dir_allocated_sizes.iter().filter_map(|&s| s).sum()),
                Some(tree.dir_reclaimable_sizes.iter().filter_map(|&s| s).sum()),
            )
        };

        DiskUsageSummary {
            total_size,
            total_allocated_size,
            total_reclaimable_size,
            total_files,
            total_dirs,
            items,
            top_files,
        }
    }
}

/// Árvore de tamanhos acumulados com suporte a atualização incremental O(depth) (§3, Issue #10).
#[derive(Debug, Clone, Default)]
pub struct IncrementalSizeTree {
    pub dir_sizes: Vec<u64>,
    pub dir_allocated_sizes: Vec<Option<u64>>,
    pub dir_reclaimable_sizes: Vec<Option<u64>>,
    pub dir_files: Vec<u64>,
    pub dir_subdirs: Vec<u64>,
}

impl IncrementalSizeTree {
    /// Constrói a árvore de tamanhos a partir do índice completo.
    #[must_use]
    pub fn build(index: &FileIndex) -> Self {
        use std::collections::HashMap;

        let n = index.entries.len();
        let mut dir_sizes = vec![0u64; n];
        let mut dir_allocated_sum = vec![0u64; n];
        let mut dir_allocated_unknown = vec![false; n];
        let mut dir_reclaimable_sum = vec![0u64; n];
        let mut dir_reclaimable_unknown = vec![false; n];
        let mut dir_files = vec![0u64; n];
        let mut dir_subdirs = vec![0u64; n];

        for (id, entry) in index.entries.iter().enumerate() {
            if entry.is_deleted() {
                continue;
            }

            if entry.is_file() {
                let m = file_metrics(index, id as u32);
                let mut curr = entry.parent;
                let mut hops = 0;

                while curr != NO_PARENT && (curr as usize) < n && hops < 1024 {
                    let idx = curr as usize;
                    dir_sizes[idx] += m.logical_size;
                    dir_files[idx] += 1;

                    if !entry.is_hardlink() {
                        if let Some(alloc) = m.allocated_size {
                            dir_allocated_sum[idx] += alloc;
                            dir_reclaimable_sum[idx] += alloc;
                        } else {
                            dir_allocated_unknown[idx] = true;
                            dir_reclaimable_unknown[idx] = true;
                        }
                    }

                    curr = index.entries[idx].parent;
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

        // Processa grupos de hard links para diretórios:
        for ids in index.hardlinks.values() {
            if ids.is_empty() {
                continue;
            }
            let first_id = ids[0];
            let m = file_metrics(index, first_id);
            let alloc_opt = m.allocated_size;

            let mut dir_counts: HashMap<u32, usize> = HashMap::new();
            for &id in ids {
                let mut curr = index.entries[id as usize].parent;
                let mut hops = 0;
                while curr != NO_PARENT && (curr as usize) < n && hops < 1024 {
                    *dir_counts.entry(curr).or_insert(0) += 1;
                    curr = index.entries[curr as usize].parent;
                    hops += 1;
                }
            }

            for (dir_id, count) in dir_counts {
                let idx = dir_id as usize;
                if let Some(alloc) = alloc_opt {
                    dir_allocated_sum[idx] += alloc;
                    if count == ids.len() {
                        dir_reclaimable_sum[idx] += alloc;
                    }
                } else {
                    dir_allocated_unknown[idx] = true;
                    if count == ids.len() {
                        dir_reclaimable_unknown[idx] = true;
                    }
                }
            }
        }

        let dir_allocated_sizes: Vec<Option<u64>> = dir_allocated_sum
            .into_iter()
            .zip(dir_allocated_unknown)
            .map(|(sum, unk)| if unk { None } else { Some(sum) })
            .collect();

        let dir_reclaimable_sizes: Vec<Option<u64>> = dir_reclaimable_sum
            .into_iter()
            .zip(dir_reclaimable_unknown)
            .map(|(sum, unk)| if unk { None } else { Some(sum) })
            .collect();

        Self {
            dir_sizes,
            dir_allocated_sizes,
            dir_reclaimable_sizes,
            dir_files,
            dir_subdirs,
        }
    }

    /// Atualiza incrementalmente o tamanho de um arquivo existente em O(depth).
    pub fn apply_size_change(
        &mut self,
        index: &FileIndex,
        entry_id: u32,
        old_size: u64,
        new_size: u64,
    ) {
        if (entry_id as usize) >= index.entries.len() {
            return;
        }

        let entry = &index.entries[entry_id as usize];
        let mut curr = entry.parent;
        let mut hops = 0;
        let n = self.dir_sizes.len();

        while curr != NO_PARENT && (curr as usize) < n && hops < 1024 {
            let idx = curr as usize;
            if new_size >= old_size {
                self.dir_sizes[idx] += new_size - old_size;
            } else {
                self.dir_sizes[idx] = self.dir_sizes[idx].saturating_sub(old_size - new_size);
            }
            curr = index.entries[idx].parent;
            hops += 1;
        }
    }

    /// Adiciona incrementalmente uma nova entrada inserida no índice.
    pub fn apply_insert(&mut self, index: &FileIndex, entry_id: u32) {
        let n = index.entries.len();
        if self.dir_sizes.len() < n {
            self.dir_sizes.resize(n, 0);
            self.dir_allocated_sizes.resize(n, None);
            self.dir_reclaimable_sizes.resize(n, None);
            self.dir_files.resize(n, 0);
            self.dir_subdirs.resize(n, 0);
        }

        let entry = &index.entries[entry_id as usize];
        if entry.is_deleted() {
            return;
        }

        let mut curr = entry.parent;
        let mut hops = 0;
        let is_file = entry.is_file();
        let size = entry.size;

        while curr != NO_PARENT && (curr as usize) < n && hops < 1024 {
            let idx = curr as usize;
            if is_file {
                self.dir_sizes[idx] += size;
                self.dir_files[idx] += 1;
            } else {
                self.dir_subdirs[idx] += 1;
            }
            curr = index.entries[idx].parent;
            hops += 1;
        }
    }

    /// Remove incrementalmente uma entrada deletada do índice.
    pub fn apply_delete(&mut self, index: &FileIndex, entry_id: u32) {
        if (entry_id as usize) >= index.entries.len() || (entry_id as usize) >= self.dir_sizes.len()
        {
            return;
        }

        let entry = &index.entries[entry_id as usize];
        let mut curr = entry.parent;
        let mut hops = 0;
        let is_file = entry.is_file();
        let size = entry.size;
        let n = self.dir_sizes.len();

        while curr != NO_PARENT && (curr as usize) < n && hops < 1024 {
            let idx = curr as usize;
            if is_file {
                self.dir_sizes[idx] = self.dir_sizes[idx].saturating_sub(size);
                self.dir_files[idx] = self.dir_files[idx].saturating_sub(1);
            } else {
                self.dir_subdirs[idx] = self.dir_subdirs[idx].saturating_sub(1);
            }
            curr = index.entries[idx].parent;
            hops += 1;
        }
    }

    /// Retorna o tamanho acumulado de um diretório específico em O(1).
    #[must_use]
    pub fn dir_size(&self, dir_id: u32) -> u64 {
        self.dir_sizes.get(dir_id as usize).copied().unwrap_or(0)
    }

    /// Retorna o tamanho nominal/lógico acumulado de um diretório.
    #[must_use]
    pub fn dir_logical_size(&self, dir_id: u32) -> u64 {
        self.dir_size(dir_id)
    }

    /// Retorna o espaço alocado em disco acumulado de um diretório (se disponível).
    #[must_use]
    pub fn dir_allocated_size(&self, dir_id: u32) -> Option<u64> {
        self.dir_allocated_sizes
            .get(dir_id as usize)
            .copied()
            .flatten()
    }

    /// Retorna a recuperação estimada acumulada de um diretório (se disponível).
    #[must_use]
    pub fn dir_reclaimable_size(&self, dir_id: u32) -> Option<u64> {
        self.dir_reclaimable_sizes
            .get(dir_id as usize)
            .copied()
            .flatten()
    }

    /// Retorna as métricas completas de tamanho e recuperação para o diretório.
    #[must_use]
    pub fn dir_metrics(&self, dir_id: u32) -> SizeMetrics {
        SizeMetrics {
            logical_size: self.dir_logical_size(dir_id),
            allocated_size: self.dir_allocated_size(dir_id),
            reclaimable_size: self.dir_reclaimable_size(dir_id),
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

    #[test]
    fn test_incremental_size_tree_updates() {
        let mut index = FileIndex::new();
        let root = index
            .add_entry(1, NO_PARENT, "C:", FLAG_DIRECTORY, 0, 0)
            .unwrap();
        let sub = index
            .add_entry(2, root, "sub", FLAG_DIRECTORY, 0, 0)
            .unwrap();
        let f1 = index
            .add_entry(3, sub, "f1.bin", FLAG_FILE, 100, 0)
            .unwrap();

        let mut tree = IncrementalSizeTree::build(&index);
        assert_eq!(tree.dir_size(root), 100);
        assert_eq!(tree.dir_size(sub), 100);

        // Altera tamanho de f1 de 100 para 250
        index.entry_mut(f1).unwrap().size = 250;
        tree.apply_size_change(&index, f1, 100, 250);
        assert_eq!(tree.dir_size(root), 250);
        assert_eq!(tree.dir_size(sub), 250);

        // Insere f2 com 50 bytes em sub
        let f2 = index.add_entry(4, sub, "f2.bin", FLAG_FILE, 50, 0).unwrap();
        tree.apply_insert(&index, f2);
        assert_eq!(tree.dir_size(root), 300);
        assert_eq!(tree.dir_size(sub), 300);

        // Deleta f1
        tree.apply_delete(&index, f1);
        assert_eq!(tree.dir_size(root), 50);
        assert_eq!(tree.dir_size(sub), 50);
    }

    #[test]
    fn test_top_files() {
        let mut index = FileIndex::new();
        let root = index
            .add_entry(1, NO_PARENT, "root", FLAG_DIRECTORY, 0, 0)
            .unwrap();
        let _f1 = index
            .add_entry(2, root, "small.txt", FLAG_FILE, 10, 0)
            .unwrap();
        let f2 = index
            .add_entry(3, root, "huge.bin", FLAG_FILE, 50_000, 0)
            .unwrap();

        let summary = SizeAggregator::aggregate(&index, Some(root), None, 5);
        assert_eq!(summary.top_files.len(), 2);
        assert_eq!(summary.top_files[0].id, f2);
        assert_eq!(summary.top_files[0].size, 50_000);
    }

    #[test]
    fn test_tm_03_size_metrics_distinction_and_reclamation() {
        use ff_index::entry::{FLAG_DIRECTORY, FLAG_FILE, FLAG_OFFLINE, FLAG_SPARSE};

        let mut index = FileIndex::new();
        let root = index
            .add_entry(1, NO_PARENT, "root", FLAG_DIRECTORY, 0, 0)
            .unwrap();
        let sub1 = index
            .add_entry(2, root, "sub1", FLAG_DIRECTORY, 0, 0)
            .unwrap();
        let sub2 = index
            .add_entry(3, root, "sub2", FLAG_DIRECTORY, 0, 0)
            .unwrap();

        // 1. Arquivo esparso: 10 MB lógico, mas apenas 8 KB alocado
        let sparse_file = index
            .add_entry(
                10,
                sub1,
                "sparse.vhd",
                FLAG_FILE | FLAG_SPARSE,
                10_000_000,
                0,
            )
            .unwrap();
        index.set_allocated_size(sparse_file, Some(8192));

        let m_sparse = file_metrics(&index, sparse_file);
        assert_eq!(m_sparse.logical_size, 10_000_000);
        assert_eq!(m_sparse.allocated_size, Some(8192));
        // Recuperação estimada DEVE ser o tamanho alocado (8 KB), nunca o lógico (10 MB)
        assert_eq!(m_sparse.reclaimable_size, Some(8192));

        // 2. Arquivo na nuvem (offline / placeholder desidratado): 50 MB lógico, 0 B em disco
        let cloud_file = index
            .add_entry(
                11,
                sub1,
                "onedrive.mp4",
                FLAG_FILE | FLAG_OFFLINE,
                50_000_000,
                0,
            )
            .unwrap();
        let m_cloud = file_metrics(&index, cloud_file);
        assert_eq!(m_cloud.logical_size, 50_000_000);
        assert_eq!(m_cloud.allocated_size, Some(0));
        assert_eq!(m_cloud.reclaimable_size, Some(0));

        // 3. Hard links: mesmo FRN 7777 em sub1 e sub2 (4 KB em disco)
        let hl1 = index
            .add_entry(7777, sub1, "link1.bin", FLAG_FILE, 4000, 0)
            .unwrap();
        let hl2 = index
            .add_entry(7777, sub2, "link2.bin", FLAG_FILE, 4000, 0)
            .unwrap();
        assert!(index.entries[hl1 as usize].is_hardlink());
        assert!(index.entries[hl2 as usize].is_hardlink());

        let m_hl1 = file_metrics(&index, hl1);
        let m_hl2 = file_metrics(&index, hl2);
        // Arquivo individual com múltiplos links não pode prometer liberação de espaço
        assert_eq!(m_hl1.reclaimable_size, Some(0));
        assert_eq!(m_hl2.reclaimable_size, Some(0));

        // 4. Arquivo com tamanho alocado desconhecido (esparso sem medição do SO)
        let unk_file = index
            .add_entry(20, sub1, "unknown.bin", FLAG_FILE | FLAG_SPARSE, 20_000, 0)
            .unwrap();
        let m_unk = file_metrics(&index, unk_file);
        assert_eq!(m_unk.logical_size, 20_000);
        // Valor desconhecido NÃO aparece como zero
        assert_eq!(m_unk.allocated_size, None);
        assert_eq!(m_unk.reclaimable_size, None);

        // 5. Agregação por diretório:
        let tree = IncrementalSizeTree::build(&index);
        // sub1 e sub2 sozinhos NÃO contêm todos os links de 7777, portanto a contribuição para recuperação deles é 0
        // No root, todos os links de 7777 estão contidos, então 7777 entra exatamente UMA vez (4096 bytes)
        assert_eq!(
            tree.dir_logical_size(sub1),
            10_000_000 + 50_000_000 + 4000 + 20_000
        );
        assert_eq!(tree.dir_logical_size(sub2), 4000);
        assert_eq!(
            tree.dir_logical_size(root),
            10_000_000 + 50_000_000 + 8000 + 20_000
        );

        // Em sub1 há arquivo com tamanho alocado desconhecido -> alocado de sub1 é None (não zero!)
        assert_eq!(tree.dir_allocated_size(sub1), None);
        assert_eq!(tree.dir_reclaimable_size(sub1), None);

        // sub2 só tem hl2 (4096 bytes alocados)
        assert_eq!(tree.dir_allocated_size(sub2), Some(4096));
        // Mas sua recuperação é 0 porque link1.bin está em sub1!
        assert_eq!(tree.dir_reclaimable_size(sub2), Some(0));
    }
}
