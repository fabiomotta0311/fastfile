//! `ff-content`
//!
//! Busca multithread em conteúdo de arquivos utilizando o motor do ripgrep
//! (`grep-searcher`, `grep-regex`, `ignore`), com suporte a ignorar binários (§3.1, §3.2 e #22).

use std::fs;
use std::io;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Mutex;
use std::time::Instant;

use grep_regex::RegexMatcherBuilder;
use grep_searcher::{BinaryDetection, Searcher, SearcherBuilder, Sink, SinkMatch};
use ignore::WalkBuilder;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};

pub mod trigram;
pub use trigram::{
    default_cache_dir, default_index_path, extract_query_trigrams, extract_trigrams, TrigramConfig,
    TrigramIndex, TrigramIndexStats,
};

/// Modo de exibição e agregação dos resultados de busca.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GrepMode {
    /// Exibe os trechos e números de linha coincidentes.
    Content,
    /// Exibe apenas a lista de arquivos que contêm correspondências.
    Files,
    /// Exibe a contagem de correspondências por arquivo.
    Count,
}

/// Correspondência individual de conteúdo encontrada.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GrepMatch {
    pub path: String,
    pub line_number: u64,
    pub line_text: String,
}

/// Contagem de correspondências por arquivo.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FileMatchCount {
    pub path: String,
    pub count: usize,
}

/// Sumário da busca em conteúdo.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GrepSummary {
    pub pattern: String,
    pub target: String,
    pub total_matches: usize,
    pub files_searched: usize,
    pub files_matched: usize,
    pub elapsed_ms: f64,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub indexed: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub candidates_count: Option<usize>,
}

/// Relatório consolidado retornado para o chamador.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GrepReport {
    pub summary: GrepSummary,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub matches: Option<Vec<GrepMatch>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub files: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub counts: Option<Vec<FileMatchCount>>,
}

/// Opções de configuração para busca em conteúdo.
#[derive(Debug, Clone)]
pub struct GrepOptions {
    pub pattern: String,
    pub target: PathBuf,
    pub mode: GrepMode,
    pub case_insensitive: bool,
    pub ext: Option<String>,
    pub file_type: Option<String>,
    pub limit: usize,
    pub all: bool,
    pub use_index: bool,
    pub index_path: Option<PathBuf>,
}

impl Default for GrepOptions {
    fn default() -> Self {
        Self {
            pattern: String::new(),
            target: PathBuf::from("."),
            mode: GrepMode::Content,
            case_insensitive: true,
            ext: None,
            file_type: None,
            limit: 100,
            all: false,
            use_index: true,
            index_path: None,
        }
    }
}

/// Mapeamento de linguagem para extensões conhecidas.
fn language_to_extensions(lang: &str) -> Option<&'static [&'static str]> {
    match lang.to_lowercase().as_str() {
        "rust" | "rs" => Some(&["rs"]),
        "toml" => Some(&["toml"]),
        "json" => Some(&["json"]),
        "python" | "py" => Some(&["py", "pyw"]),
        "javascript" | "js" => Some(&["js", "mjs", "cjs"]),
        "typescript" | "ts" => Some(&["ts", "tsx"]),
        "c" => Some(&["c", "h"]),
        "cpp" | "c++" => Some(&["cpp", "cc", "cxx", "hpp", "hh"]),
        "go" => Some(&["go"]),
        "markdown" | "md" => Some(&["md", "markdown"]),
        _ => None,
    }
}

/// Executa a busca em conteúdo com as opções fornecidas.
pub fn grep(options: &GrepOptions) -> Result<GrepReport, String> {
    let start = Instant::now();

    let matcher = RegexMatcherBuilder::new()
        .case_insensitive(options.case_insensitive)
        .build(&options.pattern)
        .map_err(|e| format!("expressão regular inválida: {e}"))?;

    let allowed_exts: Option<Vec<String>> = if let Some(ref ext) = options.ext {
        Some(vec![ext.to_lowercase()])
    } else if let Some(ref lang) = options.file_type {
        language_to_extensions(lang).map(|exts| exts.iter().map(|e| e.to_string()).collect())
    } else {
        None
    };

    let target_meta = fs::symlink_metadata(&options.target).map_err(|e| {
        format!(
            "não foi possível acessar '{}': {e}",
            options.target.display()
        )
    })?;

    let files_searched = AtomicUsize::new(0);
    let total_matches = AtomicUsize::new(0);
    let is_indexed = AtomicBool::new(false);
    let candidates_count = AtomicUsize::new(0);

    let collected_matches = Mutex::new(Vec::new());
    let collected_files = Mutex::new(Vec::new());
    let collected_counts = Mutex::new(Vec::new());

    if target_meta.is_file() {
        // Arquivo único
        files_searched.fetch_add(1, Ordering::Relaxed);
        let mut searcher = SearcherBuilder::new()
            .binary_detection(BinaryDetection::quit(0x00))
            .line_number(true)
            .build();

        let mut file_sink = GrepSink {
            path_str: options.target.to_string_lossy().to_string(),
            mode: options.mode,
            matches: Vec::new(),
            match_count: 0,
            limit: options.limit,
        };

        let _ = searcher.search_path(&matcher, &options.target, &mut file_sink);

        if file_sink.match_count > 0 {
            total_matches.fetch_add(file_sink.match_count, Ordering::Relaxed);
            match options.mode {
                GrepMode::Content => {
                    collected_matches.lock().unwrap().extend(file_sink.matches);
                }
                GrepMode::Files => {
                    collected_files.lock().unwrap().push(file_sink.path_str);
                }
                GrepMode::Count => {
                    collected_counts.lock().unwrap().push(FileMatchCount {
                        path: file_sink.path_str,
                        count: file_sink.match_count,
                    });
                }
            }
        }
    } else {
        // Tenta filtrar candidatos via índice de trigramas se habilitado
        let trigram_candidates: Option<Vec<PathBuf>> = if options.use_index {
            let index_file = options
                .index_path
                .clone()
                .unwrap_or_else(|| trigram::default_index_path(&options.target));
            if index_file.exists() {
                if let Ok(idx) = trigram::TrigramIndex::load_from_file(&options.target, &index_file)
                {
                    if let Some(query_trigrams) = trigram::extract_query_trigrams(&options.pattern)
                    {
                        let mut cands = idx.find_candidates(&query_trigrams);
                        if let Some(ref exts) = allowed_exts {
                            cands.retain(|p| {
                                p.extension()
                                    .and_then(|e| e.to_str())
                                    .map(|e| e.to_lowercase())
                                    .is_some_and(|e| exts.contains(&e))
                            });
                        }
                        is_indexed.store(true, Ordering::Relaxed);
                        candidates_count.store(cands.len(), Ordering::Relaxed);
                        Some(cands)
                    } else {
                        None
                    }
                } else {
                    None
                }
            } else {
                None
            }
        } else {
            None
        };

        if let Some(candidates) = trigram_candidates {
            // Busca apenas nos arquivos candidatos pré-filtrados pelo índice de trigramas
            candidates.par_iter().for_each(|path| {
                let current_total = total_matches.load(Ordering::Relaxed);
                if current_total >= options.limit {
                    return;
                }

                files_searched.fetch_add(1, Ordering::Relaxed);

                let path_str = path.to_string_lossy().to_string();
                let mut searcher = SearcherBuilder::new()
                    .binary_detection(BinaryDetection::quit(0x00))
                    .line_number(true)
                    .build();

                let mut file_sink = GrepSink {
                    path_str: path_str.clone(),
                    mode: options.mode,
                    matches: Vec::new(),
                    match_count: 0,
                    limit: options.limit.saturating_sub(current_total),
                };

                let _ = searcher.search_path(&matcher, path, &mut file_sink);

                if file_sink.match_count > 0 {
                    total_matches.fetch_add(file_sink.match_count, Ordering::Relaxed);
                    match options.mode {
                        GrepMode::Content => {
                            let mut lock = collected_matches.lock().unwrap();
                            lock.extend(file_sink.matches);
                        }
                        GrepMode::Files => {
                            let mut lock = collected_files.lock().unwrap();
                            lock.push(path_str);
                        }
                        GrepMode::Count => {
                            let mut lock = collected_counts.lock().unwrap();
                            lock.push(FileMatchCount {
                                path: path_str,
                                count: file_sink.match_count,
                            });
                        }
                    }
                }
            });
        } else {
            // Diretório com varredura paralela normal (fallback ou sem índice)
            let mut builder = WalkBuilder::new(&options.target);
            builder.follow_links(false);

            if !options.all {
                builder
                    .hidden(true)
                    .ignore(true)
                    .git_ignore(true)
                    .git_exclude(true);
            } else {
                builder
                    .hidden(false)
                    .ignore(false)
                    .git_ignore(false)
                    .git_exclude(false);
            }

            let walker = builder.build_parallel();

            walker.run(|| {
                let matcher = &matcher;
                let allowed_exts = &allowed_exts;
                let files_searched = &files_searched;
                let total_matches = &total_matches;
                let collected_matches = &collected_matches;
                let collected_files = &collected_files;
                let collected_counts = &collected_counts;

                let mut searcher = SearcherBuilder::new()
                    .binary_detection(BinaryDetection::quit(0x00))
                    .line_number(true)
                    .build();

                Box::new(move |entry_res| {
                    let Ok(entry) = entry_res else {
                        return ignore::WalkState::Continue;
                    };

                    let path = entry.path();
                    if !entry.file_type().is_some_and(|ft| ft.is_file()) {
                        return ignore::WalkState::Continue;
                    }

                    if let Some(ref exts) = allowed_exts {
                        let ext = path
                            .extension()
                            .and_then(|e| e.to_str())
                            .map(|e| e.to_lowercase());
                        if !ext.is_some_and(|e| exts.contains(&e)) {
                            return ignore::WalkState::Continue;
                        }
                    }

                    files_searched.fetch_add(1, Ordering::Relaxed);

                    let path_str = path.to_string_lossy().to_string();
                    let current_total = total_matches.load(Ordering::Relaxed);
                    if current_total >= options.limit {
                        return ignore::WalkState::Quit;
                    }

                    let mut file_sink = GrepSink {
                        path_str: path_str.clone(),
                        mode: options.mode,
                        matches: Vec::new(),
                        match_count: 0,
                        limit: options.limit.saturating_sub(current_total),
                    };

                    let _ = searcher.search_path(matcher, path, &mut file_sink);

                    if file_sink.match_count > 0 {
                        total_matches.fetch_add(file_sink.match_count, Ordering::Relaxed);
                        match options.mode {
                            GrepMode::Content => {
                                let mut lock = collected_matches.lock().unwrap();
                                lock.extend(file_sink.matches);
                            }
                            GrepMode::Files => {
                                let mut lock = collected_files.lock().unwrap();
                                lock.push(path_str);
                            }
                            GrepMode::Count => {
                                let mut lock = collected_counts.lock().unwrap();
                                lock.push(FileMatchCount {
                                    path: path_str,
                                    count: file_sink.match_count,
                                });
                            }
                        }
                    }

                    ignore::WalkState::Continue
                })
            });
        }
    }

    let elapsed = start.elapsed();
    let total_matches_val = total_matches.load(Ordering::Relaxed);
    let files_searched_val = files_searched.load(Ordering::Relaxed);

    let (final_matches, final_files, final_counts, files_matched_val) = match options.mode {
        GrepMode::Content => {
            let mut list = collected_matches.into_inner().unwrap();
            list.sort_by(|a, b| {
                a.path
                    .cmp(&b.path)
                    .then_with(|| a.line_number.cmp(&b.line_number))
            });
            let count = list
                .iter()
                .map(|m| &m.path)
                .collect::<std::collections::HashSet<_>>()
                .len();
            (Some(list), None, None, count)
        }
        GrepMode::Files => {
            let mut list = collected_files.into_inner().unwrap();
            list.sort();
            let count = list.len();
            (None, Some(list), None, count)
        }
        GrepMode::Count => {
            let mut list = collected_counts.into_inner().unwrap();
            list.sort_by(|a, b| a.path.cmp(&b.path));
            let count = list.len();
            (None, None, Some(list), count)
        }
    };

    let is_indexed_val = is_indexed.load(Ordering::Relaxed);
    let candidates_count_val = if is_indexed_val {
        Some(candidates_count.load(Ordering::Relaxed))
    } else {
        None
    };

    Ok(GrepReport {
        summary: GrepSummary {
            pattern: options.pattern.clone(),
            target: options.target.to_string_lossy().to_string(),
            total_matches: total_matches_val,
            files_searched: files_searched_val,
            files_matched: files_matched_val,
            elapsed_ms: (elapsed.as_secs_f64() * 1000.0 * 100.0).round() / 100.0,
            indexed: is_indexed_val,
            candidates_count: candidates_count_val,
        },
        matches: final_matches,
        files: final_files,
        counts: final_counts,
    })
}

struct GrepSink {
    path_str: String,
    mode: GrepMode,
    matches: Vec<GrepMatch>,
    match_count: usize,
    limit: usize,
}

impl Sink for GrepSink {
    type Error = io::Error;

    fn matched(&mut self, _searcher: &Searcher, mat: &SinkMatch<'_>) -> Result<bool, Self::Error> {
        self.match_count += 1;

        if self.mode == GrepMode::Content {
            let line_number = mat.line_number().unwrap_or(0);
            let line_text = String::from_utf8_lossy(mat.bytes())
                .trim_end_matches(['\r', '\n'])
                .to_string();

            self.matches.push(GrepMatch {
                path: self.path_str.clone(),
                line_number,
                line_text,
            });
        }

        if self.mode == GrepMode::Files {
            // Em modo Files, uma correspondência já é suficiente
            return Ok(false);
        }

        if self.match_count >= self.limit {
            return Ok(false);
        }

        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_grep_single_file() {
        let opts = GrepOptions {
            pattern: "fastfile".into(),
            target: PathBuf::from("Cargo.toml"),
            mode: GrepMode::Content,
            case_insensitive: true,
            ext: None,
            file_type: None,
            limit: 10,
            all: false,
            use_index: false,
            index_path: None,
        };

        let report = grep(&opts).unwrap();
        assert!(report.summary.total_matches > 0);
        assert!(report.matches.is_some());
    }

    #[test]
    fn test_grep_files_mode() {
        let opts = GrepOptions {
            pattern: "workspace".into(),
            target: PathBuf::from("Cargo.toml"),
            mode: GrepMode::Files,
            case_insensitive: true,
            ext: None,
            file_type: None,
            limit: 10,
            all: false,
            use_index: false,
            index_path: None,
        };

        let report = grep(&opts).unwrap();
        assert!(report.files.is_some());
        assert_eq!(report.files.unwrap().len(), 1);
    }

    #[test]
    fn test_grep_with_trigram_index() {
        let tmp = std::env::temp_dir().join(format!("ff_grep_tri_test_{}", std::process::id()));
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(tmp.join("subdir")).unwrap();

        fs::write(
            tmp.join("subdir/match1.txt"),
            "SpecialSearchTarget in this file\n",
        )
        .unwrap();
        fs::write(
            tmp.join("subdir/match2.txt"),
            "Another file with SpecialSearchTarget line\n",
        )
        .unwrap();
        for i in 0..10 {
            fs::write(
                tmp.join(format!("subdir/unrelated_{i}.txt")),
                format!("Irrelevant file number {i}\n"),
            )
            .unwrap();
        }

        // Cria índice de trigramas
        let mut idx = TrigramIndex::new(&tmp, TrigramConfig::default());
        let _ = idx.build_or_update().unwrap();
        let idx_path = tmp.join("index.idx");
        idx.save_to_file(&idx_path).unwrap();

        // Grep com índice de trigramas ativado
        let opts = GrepOptions {
            pattern: "SpecialSearchTarget".into(),
            target: tmp.clone(),
            mode: GrepMode::Content,
            case_insensitive: true,
            ext: None,
            file_type: None,
            limit: 10,
            all: false,
            use_index: true,
            index_path: Some(idx_path),
        };

        let report = grep(&opts).unwrap();
        assert!(report.summary.indexed);
        assert_eq!(report.summary.candidates_count, Some(2));
        assert_eq!(report.summary.files_searched, 2); // Leu apenas os 2 arquivos candidatos!
        assert_eq!(report.summary.total_matches, 2);

        let _ = fs::remove_dir_all(&tmp);
    }
}
