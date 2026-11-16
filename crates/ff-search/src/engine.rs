//! Motor de busca paralelo sobre a estrutura de dados do índice (§2.2).

use std::time::{Duration, Instant};

use ff_index::FileIndex;
use memchr::memmem::Finder;
use rayon::prelude::*;

use crate::query::{Pattern, Query};

/// Correspondência encontrada na busca.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SearchMatch {
    pub id: u32,
    pub score: u32,
}

/// Resultado consolidado da consulta com métricas de tempo.
#[derive(Debug, Clone)]
pub struct SearchResult {
    pub matches: Vec<SearchMatch>,
    pub total_scanned: usize,
    pub elapsed: Duration,
}

const CHUNK_SIZE: usize = 4096;

pub struct SearchEngine;

impl SearchEngine {
    /// Executa a busca sobre o índice em paralelo via Rayon.
    #[must_use]
    pub fn search(index: &FileIndex, query: &Query) -> SearchResult {
        let start = Instant::now();
        let total_scanned = index.entries.len();

        let ext_lower = query.filters.ext.as_ref().map(|e| e.to_lowercase());

        let matches: Vec<SearchMatch> = match &query.pattern {
            Pattern::Substring(needle) => {
                let finder = Finder::new(needle.as_bytes());
                let finder = &finder;
                index
                    .entries
                    .par_chunks(CHUNK_SIZE)
                    .enumerate()
                    .flat_map_iter(|(chunk_idx, chunk)| {
                        let base_id = (chunk_idx * CHUNK_SIZE) as u32;
                        let ext_ref = ext_lower.as_deref();
                        chunk.iter().enumerate().filter_map(move |(offset, entry)| {
                            let id = base_id + offset as u32;
                            if entry.is_deleted() {
                                return None;
                            }
                            if query.filters.files_only && entry.is_dir() {
                                return None;
                            }
                            if query.filters.dirs_only && entry.is_file() {
                                return None;
                            }
                            if let Some(min_size) = query.filters.min_size {
                                if entry.size < min_size {
                                    return None;
                                }
                            }
                            if let Some(max_size) = query.filters.max_size {
                                if entry.size > max_size {
                                    return None;
                                }
                            }
                            if let Some(min_mtime) = query.filters.min_mtime {
                                if entry.mtime < min_mtime {
                                    return None;
                                }
                            }
                            if let Some(max_mtime) = query.filters.max_mtime {
                                if entry.mtime > max_mtime {
                                    return None;
                                }
                            }
                            if let Some(parent_id) = query.filters.parent_id {
                                if entry.parent != parent_id {
                                    return None;
                                }
                            }

                            let start = entry.name_off as usize;
                            let end = start + entry.name_len as usize;
                            let name_lc = &index.names_lc[start..end];

                            finder.find(name_lc)?;

                            if let Some(ext) = ext_ref {
                                if !name_ends_with_ext(name_lc, ext.as_bytes()) {
                                    return None;
                                }
                            }

                            Some(SearchMatch { id, score: 0 })
                        })
                    })
                    .collect()
            }
            Pattern::Regex(re) => index
                .entries
                .par_chunks(CHUNK_SIZE)
                .enumerate()
                .flat_map_iter(|(chunk_idx, chunk)| {
                    let base_id = (chunk_idx * CHUNK_SIZE) as u32;
                    let ext_ref = ext_lower.as_deref();
                    chunk.iter().enumerate().filter_map(move |(offset, entry)| {
                        let id = base_id + offset as u32;
                        if entry.is_deleted() {
                            return None;
                        }
                        if query.filters.files_only && entry.is_dir() {
                            return None;
                        }
                        if query.filters.dirs_only && entry.is_file() {
                            return None;
                        }
                        if let Some(min_size) = query.filters.min_size {
                            if entry.size < min_size {
                                return None;
                            }
                        }
                        if let Some(max_size) = query.filters.max_size {
                            if entry.size > max_size {
                                return None;
                            }
                        }
                        if let Some(min_mtime) = query.filters.min_mtime {
                            if entry.mtime < min_mtime {
                                return None;
                            }
                        }
                        if let Some(max_mtime) = query.filters.max_mtime {
                            if entry.mtime > max_mtime {
                                return None;
                            }
                        }
                        if let Some(parent_id) = query.filters.parent_id {
                            if entry.parent != parent_id {
                                return None;
                            }
                        }

                        let start = entry.name_off as usize;
                        let end = start + entry.name_len as usize;
                        let name_str = std::str::from_utf8(&index.names[start..end]).ok()?;

                        if !re.is_match(name_str) {
                            return None;
                        }

                        if let Some(ext) = ext_ref {
                            let name_lc = &index.names_lc[start..end];
                            if !name_ends_with_ext(name_lc, ext.as_bytes()) {
                                return None;
                            }
                        }

                        Some(SearchMatch { id, score: 0 })
                    })
                })
                .collect(),
            Pattern::Fuzzy(needle) => {
                let needle = needle.as_str();
                let mut matches: Vec<SearchMatch> = index
                    .entries
                    .par_chunks(CHUNK_SIZE)
                    .enumerate()
                    .flat_map_iter(|(chunk_idx, chunk)| {
                        let base_id = (chunk_idx * CHUNK_SIZE) as u32;
                        let ext_ref = ext_lower.as_deref();
                        chunk.iter().enumerate().filter_map(move |(offset, entry)| {
                            let id = base_id + offset as u32;
                            if entry.is_deleted() {
                                return None;
                            }
                            if query.filters.files_only && entry.is_dir() {
                                return None;
                            }
                            if query.filters.dirs_only && entry.is_file() {
                                return None;
                            }
                            if let Some(min_size) = query.filters.min_size {
                                if entry.size < min_size {
                                    return None;
                                }
                            }
                            if let Some(max_size) = query.filters.max_size {
                                if entry.size > max_size {
                                    return None;
                                }
                            }
                            if let Some(min_mtime) = query.filters.min_mtime {
                                if entry.mtime < min_mtime {
                                    return None;
                                }
                            }
                            if let Some(max_mtime) = query.filters.max_mtime {
                                if entry.mtime > max_mtime {
                                    return None;
                                }
                            }
                            if let Some(parent_id) = query.filters.parent_id {
                                if entry.parent != parent_id {
                                    return None;
                                }
                            }

                            let start = entry.name_off as usize;
                            let end = start + entry.name_len as usize;
                            let name_lc = std::str::from_utf8(&index.names_lc[start..end]).ok()?;

                            if let Some(ext) = ext_ref {
                                if !name_ends_with_ext(name_lc.as_bytes(), ext.as_bytes()) {
                                    return None;
                                }
                            }

                            let score = crate::fuzzy::fuzzy_score(needle, name_lc)?;
                            Some(SearchMatch { id, score })
                        })
                    })
                    .collect();

                matches.sort_by_key(|m| std::cmp::Reverse(m.score));
                matches
            }
        };

        let elapsed = start.elapsed();
        SearchResult {
            matches,
            total_scanned,
            elapsed,
        }
    }
}

#[inline]
fn name_ends_with_ext(name: &[u8], ext: &[u8]) -> bool {
    if name.len() < ext.len() + 1 {
        return false;
    }
    let dot_pos = name.len() - ext.len() - 1;
    name[dot_pos] == b'.' && &name[dot_pos + 1..] == ext
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::query::Filters;
    use ff_index::{FLAG_DIRECTORY, FLAG_FILE, NO_PARENT};

    #[test]
    fn test_search_substring() {
        let mut index = FileIndex::new();
        let root = index
            .add_entry(1, NO_PARENT, "C:", FLAG_DIRECTORY, 0, 0)
            .unwrap();
        let f1 = index
            .add_entry(2, root, "Cargo.toml", FLAG_FILE, 100, 1000)
            .unwrap();
        let f2 = index
            .add_entry(3, root, "main.rs", FLAG_FILE, 500, 2000)
            .unwrap();
        let f3 = index
            .add_entry(4, root, "CARGO.lock", FLAG_FILE, 800, 3000)
            .unwrap();

        // Busca substring "cargo" (case-insensitive)
        let q = Query::parse("cargo", false).unwrap();
        let res = SearchEngine::search(&index, &q);
        assert_eq!(res.matches.len(), 2);
        assert!(res.matches.iter().any(|m| m.id == f1));
        assert!(res.matches.iter().any(|m| m.id == f3));
        assert!(!res.matches.iter().any(|m| m.id == f2));
    }

    #[test]
    fn test_search_with_filters() {
        let mut index = FileIndex::new();
        let root = index
            .add_entry(1, NO_PARENT, "C:", FLAG_DIRECTORY, 0, 0)
            .unwrap();
        let _f1 = index
            .add_entry(2, root, "small.txt", FLAG_FILE, 50, 1000)
            .unwrap();
        let f2 = index
            .add_entry(3, root, "big.txt", FLAG_FILE, 5000, 2000)
            .unwrap();
        let _f3 = index
            .add_entry(4, root, "big.rs", FLAG_FILE, 5000, 2000)
            .unwrap();

        // Filtro: tamanho >= 1000 e extensão "txt"
        let filters = Filters {
            ext: Some("txt".into()),
            min_size: Some(1000),
            ..Default::default()
        };
        let q = Query::parse("big", false).unwrap().with_filters(filters);
        let res = SearchEngine::search(&index, &q);

        assert_eq!(res.matches.len(), 1);
        assert_eq!(res.matches[0].id, f2);
    }

    #[test]
    fn test_search_wildcard() {
        let mut index = FileIndex::new();
        let root = index
            .add_entry(1, NO_PARENT, "C:", FLAG_DIRECTORY, 0, 0)
            .unwrap();
        let f1 = index
            .add_entry(2, root, "test_one.rs", FLAG_FILE, 10, 0)
            .unwrap();
        let f2 = index
            .add_entry(3, root, "test_two.rs", FLAG_FILE, 20, 0)
            .unwrap();
        let _f3 = index
            .add_entry(4, root, "other.rs", FLAG_FILE, 30, 0)
            .unwrap();

        let q = Query::parse("test_*.rs", false).unwrap();
        let res = SearchEngine::search(&index, &q);
        assert_eq!(res.matches.len(), 2);
        assert!(res.matches.iter().any(|m| m.id == f1));
        assert!(res.matches.iter().any(|m| m.id == f2));
    }

    #[test]
    fn test_search_fuzzy() {
        let mut index = FileIndex::new();
        let root = index
            .add_entry(1, NO_PARENT, "C:", FLAG_DIRECTORY, 0, 0)
            .unwrap();
        let f1 = index
            .add_entry(2, root, "ARQUITETURA.md", FLAG_FILE, 100, 1000)
            .unwrap();
        let f2 = index
            .add_entry(3, root, "Cargo.toml", FLAG_FILE, 200, 2000)
            .unwrap();
        let _f3 = index
            .add_entry(4, root, "README.md", FLAG_FILE, 300, 3000)
            .unwrap();

        // Busca aproximada "archmd" deve achar e colocar ARQUITETURA.md no topo
        let q = Query::parse_with_options("archmd", false, true).unwrap();
        let res = SearchEngine::search(&index, &q);
        assert!(
            !res.matches.is_empty(),
            "deve encontrar resultados para archmd"
        );
        assert_eq!(res.matches[0].id, f1, "ARQUITETURA.md deve estar no topo");
        assert!(
            res.matches.iter().all(|m| m.id != f2),
            "Cargo.toml não deve corresponder a archmd"
        );
    }
}
