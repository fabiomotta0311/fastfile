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

pub mod extractors;
pub use extractors::{
    extract_docx_text, extract_pdf_text, extract_text_from_file, has_document_extractor,
};

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

fn default_column_one() -> u64 {
    1
}

/// Correspondência individual de conteúdo encontrada.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GrepMatch {
    pub path: String,
    pub line_number: u64,
    #[serde(default = "default_column_one")]
    pub column_number: u64,
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
    /// Arquivos descartados pelo índice de trigramas sem leitura (representados e sem candidato).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub files_skipped_by_index: Option<usize>,
    /// Arquivos lidos por estarem fora do índice (novos, alterados, acima dos tetos, documentos).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub files_outside_index: Option<usize>,
    /// Quando `true`, o sumário não é serializado dentro do relatório (o chamador já o publica
    /// no envelope, como no esquema `ff/2`).
    #[serde(skip)]
    pub omit: bool,
}

impl GrepSummary {
    fn is_omitted(&self) -> bool {
        self.omit
    }
}

/// Relatório consolidado retornado para o chamador.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GrepReport {
    #[serde(skip_serializing_if = "GrepSummary::is_omitted")]
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
    /// Trata o padrão como texto literal (`rg -F`).
    pub fixed_strings: bool,
    /// Só casa palavras inteiras (`rg -w`).
    pub word: bool,
    /// Inclui arquivos e pastas ocultos, mantendo as regras de `.gitignore` (`rg --hidden`).
    pub hidden: bool,
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
            fixed_strings: false,
            word: false,
            hidden: false,
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
        "document" | "doc" => Some(&["docx", "pdf"]),
        _ => None,
    }
}

/// Executa a busca em documentos enriquecidos (DOCX/PDF) extraindo o texto limpo linha por linha.
fn search_document_file_into(
    matcher: &grep_regex::RegexMatcher,
    path: &std::path::Path,
    path_str: &str,
    mode: GrepMode,
    limit: usize,
) -> (Vec<GrepMatch>, usize) {
    use grep_matcher::Matcher;
    let Ok(Some(text)) = extractors::extract_text_from_file(path) else {
        return (Vec::new(), 0);
    };

    let mut matches = Vec::new();
    let mut count = 0;

    for (line_idx, line) in text.lines().enumerate() {
        let line_bytes = line.as_bytes();
        if let Ok(Some(mat)) = matcher.find(line_bytes) {
            count += 1;
            if mode == GrepMode::Content {
                matches.push(GrepMatch {
                    path: path_str.to_string(),
                    line_number: (line_idx + 1) as u64,
                    column_number: mat.start() as u64 + 1,
                    line_text: line.to_string(),
                });
            }
            if mode == GrepMode::Files {
                return (matches, 1);
            }
            if limit > 0 && count >= limit {
                break;
            }
        }
    }

    (matches, count)
}

/// Decisão do índice de trigramas para um arquivo encontrado na varredura.
enum IndexDecision {
    /// O índice representa o arquivo e ele não contém os trigramas da consulta.
    Skip,
    /// O índice representa o arquivo e ele pode casar: precisa ser lido.
    Candidate,
    /// O índice não representa este arquivo como está em disco: precisa ser lido.
    NotCovered,
}

/// Filtro derivado do índice de trigramas persistido para a raiz da busca.
struct IndexFilter {
    covered: std::collections::HashMap<String, (u64, u64)>,
    candidates: std::collections::HashSet<String>,
}

impl IndexFilter {
    fn decide(&self, root: &std::path::Path, entry: &ignore::DirEntry) -> IndexDecision {
        let path = entry.path();
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        if extractors::has_document_extractor(ext) {
            return IndexDecision::NotCovered;
        }
        let Ok(rel) = path.strip_prefix(root) else {
            return IndexDecision::NotCovered;
        };
        let rel = rel.to_string_lossy().replace('\\', "/");
        let Some(&(mtime, size)) = self.covered.get(&rel) else {
            return IndexDecision::NotCovered;
        };
        let Ok(meta) = entry.metadata() else {
            return IndexDecision::NotCovered;
        };
        let disk_mtime = meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map_or(0, |d| d.as_secs());
        if disk_mtime != mtime || meta.len() != size {
            return IndexDecision::NotCovered;
        }
        if self.candidates.contains(&rel) {
            IndexDecision::Candidate
        } else {
            IndexDecision::Skip
        }
    }
}

/// Carrega o índice de trigramas da raiz, se existir e for compatível com a consulta.
fn load_index_filter(options: &GrepOptions) -> Option<IndexFilter> {
    let index_file = options
        .index_path
        .clone()
        .unwrap_or_else(|| trigram::default_index_path(&options.target));
    if !index_file.exists() {
        return None;
    }
    let query_pattern = if options.fixed_strings {
        regex_syntax::escape(&options.pattern)
    } else {
        options.pattern.clone()
    };
    let query_trigrams = trigram::extract_query_trigrams(&query_pattern)?;
    let idx = trigram::TrigramIndex::load_from_file(&options.target, &index_file).ok()?;
    // Um índice construído sem as regras de ignore não corresponde à varredura padrão.
    if !idx.config.respect_ignore {
        return None;
    }
    let candidates = idx
        .candidate_relative_paths(&query_trigrams)
        .into_iter()
        .map(str::to_string)
        .collect();
    let covered = idx
        .covered_files()
        .into_iter()
        .map(|(k, v)| (k.to_string(), v))
        .collect();
    Some(IndexFilter {
        covered,
        candidates,
    })
}

/// Executa a busca em conteúdo com as opções fornecidas.
pub fn grep(options: &GrepOptions) -> Result<GrepReport, String> {
    let start = Instant::now();

    let matcher = RegexMatcherBuilder::new()
        .case_insensitive(options.case_insensitive)
        .fixed_strings(options.fixed_strings)
        .word(options.word)
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
    let skipped_by_index = AtomicUsize::new(0);
    let outside_index = AtomicUsize::new(0);

    let collected_matches = Mutex::new(Vec::new());
    let collected_files = Mutex::new(Vec::new());
    let collected_counts = Mutex::new(Vec::new());

    if target_meta.is_file() {
        // Arquivo único
        files_searched.fetch_add(1, Ordering::Relaxed);
        let path_str = options.target.to_string_lossy().to_string();
        let ext = options
            .target
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("");

        let (matches, match_count) = if extractors::has_document_extractor(ext) {
            search_document_file_into(
                &matcher,
                &options.target,
                &path_str,
                options.mode,
                options.limit,
            )
        } else {
            let mut searcher = SearcherBuilder::new()
                .binary_detection(BinaryDetection::quit(0x00))
                .line_number(true)
                .build();

            let mut file_sink = GrepSink {
                matcher: &matcher,
                path_str: path_str.clone(),
                mode: options.mode,
                matches: Vec::new(),
                match_count: 0,
                limit: options.limit,
            };

            let _ = searcher.search_path(&matcher, &options.target, &mut file_sink);
            (file_sink.matches, file_sink.match_count)
        };

        if match_count > 0 {
            total_matches.fetch_add(match_count, Ordering::Relaxed);
            match options.mode {
                GrepMode::Content => {
                    collected_matches.lock().unwrap().extend(matches);
                }
                GrepMode::Files => {
                    collected_files.lock().unwrap().push(path_str);
                }
                GrepMode::Count => {
                    collected_counts.lock().unwrap().push(FileMatchCount {
                        path: path_str,
                        count: match_count,
                    });
                }
            }
        }
    } else {
        // Índice de trigramas (contrato `TRIGRAM-COVERAGE-001`): o índice só pode *descartar*
        // arquivos que ele representa fielmente (mesmo mtime e tamanho, com trigramas). Arquivos
        // novos, alterados, acima dos tetos do índice ou de documento (PDF/DOCX) são sempre lidos,
        // então o resultado é idêntico ao da busca sem índice.
        let index_filter = if options.use_index && !options.all {
            load_index_filter(options)
        } else {
            None
        };
        if index_filter.is_some() {
            is_indexed.store(true, Ordering::Relaxed);
        }
        if let Some(ref f) = index_filter {
            candidates_count.store(f.candidates.len(), Ordering::Relaxed);
        }

        {
            // Diretório com varredura paralela normal (fallback ou sem índice)
            let mut builder = WalkBuilder::new(&options.target);
            builder.follow_links(false);

            if !options.all {
                builder
                    .hidden(!options.hidden)
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
                let index_filter = &index_filter;
                let skipped_by_index = &skipped_by_index;
                let outside_index = &outside_index;

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

                    if let Some(filter) = index_filter {
                        match filter.decide(&options.target, &entry) {
                            IndexDecision::Skip => {
                                skipped_by_index.fetch_add(1, Ordering::Relaxed);
                                return ignore::WalkState::Continue;
                            }
                            IndexDecision::Candidate => {}
                            IndexDecision::NotCovered => {
                                outside_index.fetch_add(1, Ordering::Relaxed);
                            }
                        }
                    }

                    files_searched.fetch_add(1, Ordering::Relaxed);

                    let path_str = path.to_string_lossy().to_string();
                    let current_total = total_matches.load(Ordering::Relaxed);
                    if current_total >= options.limit {
                        return ignore::WalkState::Quit;
                    }

                    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
                    let (matches, match_count) = if extractors::has_document_extractor(ext) {
                        search_document_file_into(
                            matcher,
                            path,
                            &path_str,
                            options.mode,
                            options.limit.saturating_sub(current_total),
                        )
                    } else {
                        let mut file_sink = GrepSink {
                            matcher,
                            path_str: path_str.clone(),
                            mode: options.mode,
                            matches: Vec::new(),
                            match_count: 0,
                            limit: options.limit.saturating_sub(current_total),
                        };

                        let _ = searcher.search_path(matcher, path, &mut file_sink);
                        (file_sink.matches, file_sink.match_count)
                    };

                    if match_count > 0 {
                        total_matches.fetch_add(match_count, Ordering::Relaxed);
                        match options.mode {
                            GrepMode::Content => {
                                let mut lock = collected_matches.lock().unwrap();
                                lock.extend(matches);
                            }
                            GrepMode::Files => {
                                let mut lock = collected_files.lock().unwrap();
                                lock.push(path_str);
                            }
                            GrepMode::Count => {
                                let mut lock = collected_counts.lock().unwrap();
                                lock.push(FileMatchCount {
                                    path: path_str,
                                    count: match_count,
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
            files_skipped_by_index: is_indexed_val
                .then(|| skipped_by_index.load(Ordering::Relaxed)),
            files_outside_index: is_indexed_val.then(|| outside_index.load(Ordering::Relaxed)),
            omit: false,
        },
        matches: final_matches,
        files: final_files,
        counts: final_counts,
    })
}

/// Extensões comuns de arquivos binários conhecidos que devem ser pulados na busca de texto puro.
/// Note que documentos enriquecidos como .docx e .pdf não estão nesta lista,
/// pois possuem extrator de texto dedicado (extractors::has_document_extractor).
const BINARY_EXTENSIONS: &[&str] = &[
    "exe", "dll", "so", "dylib", "obj", "bin", "o", "a", "lib", "png", "jpg", "jpeg", "gif", "ico",
    "bmp", "webp", "svgz", "zip", "tar", "gz", "bz2", "xz", "7z", "rar", "xlsx", "pptx", "iso",
    "img", "vmdk", "vhdx", "class", "pyc", "pyd", "wasm",
];

fn is_binary_extension(ext: &str) -> bool {
    let ext_lc = ext.to_lowercase();
    BINARY_EXTENSIONS.contains(&ext_lc.as_str())
}

/// Executa busca em conteúdo pré-filtrando candidatos através do `FileIndex` (§3, Issue #9).
pub fn grep_with_index(
    options: &GrepOptions,
    index: &ff_index::FileIndex,
) -> Result<GrepReport, String> {
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

    let target_canon = options
        .target
        .canonicalize()
        .unwrap_or_else(|_| options.target.clone());
    let target_prefix = target_canon.to_string_lossy().to_string();
    let norm_target = target_prefix.trim_end_matches(['\\', '/']);
    let norm_target_trimmed = norm_target.strip_prefix(r"\\?\").unwrap_or(norm_target);

    // 1. Filtrar candidatos diretamente do índice em memória
    let mut candidates: Vec<PathBuf> = Vec::new();
    for id in 0..index.entries.len() {
        let entry = &index.entries[id];
        if entry.is_deleted() || entry.is_dir() {
            continue;
        }

        // Pular arquivos vazios ou gigantescos (> 100 MB)
        if entry.size == 0 || entry.size > 100 * 1024 * 1024 {
            continue;
        }

        let name = match index.name(id as u32) {
            Ok(n) => n,
            Err(_) => continue,
        };

        let ext = std::path::Path::new(name)
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("");

        // Pular binários conhecidos
        if is_binary_extension(ext) {
            continue;
        }

        // Filtro por extensão permitida
        if let Some(ref allowed) = allowed_exts {
            if !allowed.iter().any(|a| a.eq_ignore_ascii_case(ext)) {
                continue;
            }
        }

        let path_str = match index.build_path(id as u32) {
            Ok(p) => p,
            Err(_) => continue,
        };

        // Filtro por pasta/target
        if !norm_target_trimmed.is_empty() && norm_target_trimmed != "." {
            let path_clean = path_str.strip_prefix(r"\\?\").unwrap_or(&path_str);
            let path_clean_lc = path_clean.to_lowercase();
            let norm_target_lc = norm_target_trimmed.to_lowercase();

            if !path_clean_lc.eq(&norm_target_lc)
                && !path_clean_lc.starts_with(&format!("{norm_target_lc}\\"))
                && !path_clean_lc.starts_with(&format!("{norm_target_lc}/"))
            {
                continue;
            }
        }

        // Filtro de pastas ignoradas se !options.all
        if !options.all
            && (path_str.contains("\\target\\")
                || path_str.contains("/target/")
                || path_str.contains("\\node_modules\\")
                || path_str.contains("/node_modules/")
                || path_str.contains("\\.git\\")
                || path_str.contains("/.git/"))
        {
            continue;
        }

        candidates.push(PathBuf::from(path_str));
    }

    let candidates_count = candidates.len();
    let files_searched = AtomicUsize::new(0);
    let total_matches = AtomicUsize::new(0);

    let collected_matches = Mutex::new(Vec::new());
    let collected_files = Mutex::new(Vec::new());
    let collected_counts = Mutex::new(Vec::new());

    // 2. Busca paralela com rayon apenas nos candidatos pré-filtrados
    candidates.par_iter().for_each(|path| {
        let current_total = total_matches.load(Ordering::Relaxed);
        if options.limit > 0 && current_total >= options.limit {
            return;
        }

        files_searched.fetch_add(1, Ordering::Relaxed);
        let path_str = path.to_string_lossy().to_string();
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");

        let (matches, match_count) = if extractors::has_document_extractor(ext) {
            search_document_file_into(
                &matcher,
                path,
                &path_str,
                options.mode,
                options.limit.saturating_sub(current_total),
            )
        } else {
            let mut searcher = SearcherBuilder::new()
                .binary_detection(BinaryDetection::quit(0x00))
                .line_number(true)
                .build();

            let mut file_sink = GrepSink {
                matcher: &matcher,
                path_str: path_str.clone(),
                mode: options.mode,
                matches: Vec::new(),
                match_count: 0,
                limit: options.limit.saturating_sub(current_total),
            };

            let _ = searcher.search_path(&matcher, path, &mut file_sink);
            (file_sink.matches, file_sink.match_count)
        };

        if match_count > 0 {
            total_matches.fetch_add(match_count, Ordering::Relaxed);
            match options.mode {
                GrepMode::Content => {
                    let mut lock = collected_matches.lock().unwrap();
                    lock.extend(matches);
                }
                GrepMode::Files => {
                    let mut lock = collected_files.lock().unwrap();
                    lock.push(path_str);
                }
                GrepMode::Count => {
                    let mut lock = collected_counts.lock().unwrap();
                    lock.push(FileMatchCount {
                        path: path_str,
                        count: match_count,
                    });
                }
            }
        }
    });

    let elapsed = start.elapsed();
    let total_matches_val = total_matches.load(Ordering::Relaxed);

    let mut final_matches = if options.mode == GrepMode::Content {
        Some(collected_matches.into_inner().unwrap())
    } else {
        None
    };
    if let Some(ref mut ms) = final_matches {
        ms.sort_by(|a, b| {
            a.path
                .cmp(&b.path)
                .then_with(|| a.line_number.cmp(&b.line_number))
        });
        if options.limit > 0 && ms.len() > options.limit {
            ms.truncate(options.limit);
        }
    }

    let mut final_files = if options.mode == GrepMode::Files {
        Some(collected_files.into_inner().unwrap())
    } else {
        None
    };
    if let Some(ref mut fs) = final_files {
        fs.sort();
        if options.limit > 0 && fs.len() > options.limit {
            fs.truncate(options.limit);
        }
    }

    let mut final_counts = if options.mode == GrepMode::Count {
        Some(collected_counts.into_inner().unwrap())
    } else {
        None
    };
    if let Some(ref mut cs) = final_counts {
        cs.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.path.cmp(&b.path)));
        if options.limit > 0 && cs.len() > options.limit {
            cs.truncate(options.limit);
        }
    }

    let files_matched_val = match options.mode {
        GrepMode::Content => final_matches
            .as_ref()
            .map(|m| {
                let mut unique = std::collections::HashSet::new();
                for item in m {
                    unique.insert(&item.path);
                }
                unique.len()
            })
            .unwrap_or(0),
        GrepMode::Files => final_files.as_ref().map(|f| f.len()).unwrap_or(0),
        GrepMode::Count => final_counts.as_ref().map(|c| c.len()).unwrap_or(0),
    };

    Ok(GrepReport {
        summary: GrepSummary {
            pattern: options.pattern.clone(),
            target: options.target.to_string_lossy().to_string(),
            total_matches: total_matches_val,
            files_searched: files_searched.load(Ordering::Relaxed),
            files_matched: files_matched_val,
            elapsed_ms: (elapsed.as_secs_f64() * 1000.0 * 100.0).round() / 100.0,
            indexed: true,
            candidates_count: Some(candidates_count),
            files_skipped_by_index: None,
            files_outside_index: None,
            omit: false,
        },
        matches: final_matches,
        files: final_files,
        counts: final_counts,
    })
}

struct GrepSink<'a> {
    matcher: &'a grep_regex::RegexMatcher,
    path_str: String,
    mode: GrepMode,
    matches: Vec<GrepMatch>,
    match_count: usize,
    limit: usize,
}

impl<'a> Sink for GrepSink<'a> {
    type Error = io::Error;

    fn matched(&mut self, _searcher: &Searcher, mat: &SinkMatch<'_>) -> Result<bool, Self::Error> {
        self.match_count += 1;

        if self.mode == GrepMode::Content {
            let line_number = mat.line_number().unwrap_or(0);
            let line_bytes = mat.bytes();
            let line_text = String::from_utf8_lossy(line_bytes)
                .trim_end_matches(['\r', '\n'])
                .to_string();

            use grep_matcher::Matcher;
            let column_number = self
                .matcher
                .find(line_bytes)
                .ok()
                .flatten()
                .map(|m| m.start() as u64 + 1)
                .unwrap_or(1);

            self.matches.push(GrepMatch {
                path: self.path_str.clone(),
                line_number,
                column_number,
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
            ..GrepOptions::default()
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
            ..GrepOptions::default()
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
        let idx_path = tmp.with_extension("idx");
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
            ..GrepOptions::default()
        };

        let report = grep(&opts).unwrap();
        assert!(report.summary.indexed);
        assert_eq!(report.summary.candidates_count, Some(2));
        assert_eq!(report.summary.files_searched, 2); // Leu apenas os 2 arquivos candidatos!
        assert_eq!(report.summary.total_matches, 2);

        let _ = fs::remove_dir_all(&tmp);
        let _ = fs::remove_file(tmp.with_extension("idx"));
    }

    fn sorted_files(opts: &GrepOptions) -> Vec<String> {
        let mut files = grep(opts).unwrap().files.unwrap_or_default();
        files.sort();
        files
    }

    /// `TRIGRAM-COVERAGE-001`: com índice desatualizado ou truncado pelos tetos, o resultado é
    /// idêntico ao da busca sem índice.
    #[test]
    fn indexed_grep_matches_unindexed_when_index_is_stale_or_capped() {
        let tmp = std::env::temp_dir().join(format!("ff_grep_tri_cov_{}", std::process::id()));
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(tmp.join("src")).unwrap();
        for i in 0..6 {
            fs::write(
                tmp.join(format!("src/f{i}.txt")),
                format!("linha {i} com NeedleWord aqui\n"),
            )
            .unwrap();
        }
        fs::write(tmp.join("src/other.txt"), "nada a ver\n").unwrap();

        // Índice limitado a 2 arquivos: os demais ficam fora dele.
        let config = TrigramConfig {
            max_files: 2,
            ..TrigramConfig::default()
        };
        let mut idx = TrigramIndex::new(&tmp, config);
        idx.build_or_update().unwrap();
        let idx_path = tmp.with_extension("idx");
        idx.save_to_file(&idx_path).unwrap();

        // Depois do índice: um arquivo novo e um arquivo alterado (tamanho muda).
        fs::write(tmp.join("src/new.txt"), "NeedleWord recém-criado\n").unwrap();
        fs::write(tmp.join("src/other.txt"), "agora tem NeedleWord também\n").unwrap();

        for pattern in ["NeedleWord", "needleword", "Needle(Word)+", "N.edleWord"] {
            let base = GrepOptions {
                pattern: pattern.into(),
                target: tmp.clone(),
                mode: GrepMode::Files,
                limit: 1000,
                use_index: false,
                index_path: Some(idx_path.clone()),
                ..GrepOptions::default()
            };
            let indexed = GrepOptions {
                use_index: true,
                ..base.clone()
            };
            let expected = sorted_files(&base);
            assert_eq!(expected.len(), 8, "padrão {pattern}");
            assert_eq!(sorted_files(&indexed), expected, "padrão {pattern}");
        }

        let report = grep(&GrepOptions {
            pattern: "NeedleWord".into(),
            target: tmp.clone(),
            mode: GrepMode::Files,
            limit: 1000,
            index_path: Some(idx_path.clone()),
            ..GrepOptions::default()
        })
        .unwrap();
        assert!(report.summary.indexed);
        assert!(report.summary.files_outside_index.unwrap() >= 6);

        let _ = fs::remove_dir_all(&tmp);
        let _ = fs::remove_file(idx_path);
    }

    /// `TRIGRAM-QUERY-001`: partes opcionais, classes e escapes não viram trigramas obrigatórios.
    #[test]
    fn query_trigrams_ignore_optional_parts() {
        use crate::trigram::bytes_to_trigram;
        let has = |tris: &[u32], s: &[u8]| tris.contains(&bytes_to_trigram(s));

        let tris = extract_query_trigrams("colou?r").unwrap();
        assert!(has(&tris, b"olo"));
        assert!(!has(&tris, b"lou") && !has(&tris, b"our") && !has(&tris, b"lor"));
        assert!(extract_query_trigrams("foo|bar").is_none());
        assert!(extract_query_trigrams("[abc]de").is_none());

        let tris = extract_query_trigrams(r"\dabc").unwrap();
        assert!(has(&tris, b"abc"));
        assert!(!has(&tris, b"dab"));

        let tris = extract_query_trigrams("abcd?").unwrap();
        assert!(has(&tris, b"abc"));
        assert!(!has(&tris, b"bcd"));

        // Não ASCII interrompe a sequência (o índice só normaliza caixa ASCII).
        let tris = extract_query_trigrams("açãoxyz").unwrap();
        assert!(has(&tris, b"oxy") && has(&tris, b"xyz"));
        assert_eq!(tris.len(), 2);
    }

    #[test]
    fn test_grep_with_file_index_prefilter() {
        use ff_index::IndexSource;
        let tmp = std::env::temp_dir().join(format!("ff_test_grep_idx_{}", std::process::id()));
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(tmp.join("sub")).unwrap();

        let f1 = tmp.join("sub/hello.rs");
        let f2 = tmp.join("sub/ignored.bin");
        let f3 = tmp.join("sub/other.rs");

        fs::write(&f1, "fn main() {\n    let foo = \"target_word\";\n}\n").unwrap();
        fs::write(&f2, b"\x00\x01\x02target_word\x03\x04").unwrap();
        fs::write(&f3, "fn other() { let x = 42; }\n").unwrap();

        let mut index = ff_index::FileIndex::new();
        let mut source = ff_index::WalkSource::new(&tmp).with_respect_ignore(false);
        source.populate(&mut index).unwrap();

        let opts = GrepOptions {
            pattern: "target_word".into(),
            target: tmp.clone(),
            mode: GrepMode::Content,
            case_insensitive: true,
            ext: Some("rs".into()),
            file_type: None,
            limit: 10,
            all: false,
            use_index: true,
            index_path: None,
            ..GrepOptions::default()
        };

        let report = grep_with_index(&opts, &index).unwrap();
        assert!(report.summary.indexed);
        assert_eq!(report.summary.total_matches, 1);
        let matches = report.matches.unwrap();
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].line_number, 2);
        // "    let foo = \"target_word\";" -> coluna começa em 16 (1-indexed)
        assert_eq!(matches[0].column_number, 16);

        let _ = fs::remove_dir_all(&tmp);
    }
}
