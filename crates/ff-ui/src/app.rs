//! Aplicação principal FastFile egui (§0.1, §4 e Issues #13, #14, #15, #19).

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, RwLock};
use std::time::Instant;

use egui::{
    Color32, CornerRadius, FontId, Frame, Margin, Rect, ScrollArea, Sense, Stroke, StrokeKind, Ui,
    Vec2,
};

use ff_diff::SideBySideDiff;
use ff_index::IndexSource;
use ff_indexer::{default_endpoint_name, IpcClient, IpcRequest, IpcResponse};
use ff_preview::{PreviewEngine, PreviewResult};
use ff_search::query::Query;
use ff_search::SearchEngine;
use ff_size::treemap::{TreemapItem, TreemapLayout, TreemapRect};

use crate::i18n::Locale;
use crate::theme::*;

/// Filtros rápidos por categoria de arquivo.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuickFilter {
    All,
    Code,
    Docs,
    Media,
    Archives,
    Large,
}

/// Coluna de ordenação na lista de resultados.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortColumn {
    Name,
    Path,
    Size,
    Date,
}

/// Aba ativa na interface principal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActiveTab {
    Search,
    Treemap,
    Diff,
}

/// Estado de conclusão da análise do Treemap (TM-02).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TreemapAnalysisStatus {
    /// Análise concluída com sucesso e sem erros de permissão.
    Complete,
    /// Análise parcial: concluída, mas com pastas sem permissão ou erros de acesso.
    Partial { errors_count: usize },
    /// Análise cancelada pelo usuário durante a varredura.
    Cancelled { items_scanned: usize },
    /// Falha na análise (caminho inexistente ou erro irrecuperável).
    Failed(String),
}

/// Registro de erro de acesso ou permissão durante a varredura (TM-02).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScanAccessError {
    pub path: String,
    pub message: String,
}

/// Progresso da varredura em tempo real para exibição na UI (TM-02).
#[derive(Debug, Default)]
pub struct TreemapScanProgress {
    pub items_count: AtomicUsize,
    pub bytes_count: AtomicU64,
    pub current_dir: RwLock<String>,
    pub access_errors_count: AtomicUsize,
}

/// Origem dos dados utilizados na análise do Treemap.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TreemapDataSource {
    DiskScan,
    Mft,
}

impl TreemapDataSource {
    #[must_use]
    pub fn label(&self, loc: Locale) -> &'static str {
        match self {
            Self::DiskScan => loc.treemap_source_disk(),
            Self::Mft => loc.treemap_source_mft(),
        }
    }
}

/// Resultado de uma varredura assíncrona do Treemap realizada em segundo plano (TM-01 / TM-02).
pub struct TreemapScanResult {
    pub scan_id: u64,
    pub root_path: String,
    pub preserve_path: Option<String>,
    pub index: ff_index::FileIndex,
    pub source: TreemapDataSource,
    pub scan_ms: f64,
    pub status: TreemapAnalysisStatus,
    pub access_errors: Vec<ScanAccessError>,
}

/// Linha de resultado da busca para renderização virtualizada.
#[derive(Debug, Clone)]
pub struct SearchResultItem {
    pub id: u32,
    pub name: String,
    pub path: String,
    pub size: u64,
    pub mtime: i64,
    pub is_dir: bool,
}

/// Estado global da aplicação FastFile egui.
pub struct FastFileApp {
    // Consulta e filtros
    pub query: String,
    pub active_filter: QuickFilter,
    pub active_tab: ActiveTab,

    // Conexão e índice
    pub ipc_client: Option<IpcClient>,
    pub local_index: Option<ff_index::FileIndex>,
    pub is_ipc_connected: bool,
    pub total_indexed_files: usize,
    pub search_latency_ms: f32,

    // Caminho da busca na tela de resultados
    pub search_target_path: String,
    pub search_scanning_path: Option<String>,
    pub search_receiver: Option<std::sync::mpsc::Receiver<(String, ff_index::FileIndex, f32)>>,

    // Redimensionamento de colunas e painel
    pub preview_split_ratio: f32,
    pub col_width_name: f32,
    pub col_width_size: f32,

    // Resultados da busca
    pub results: Vec<SearchResultItem>,
    pub sort_column: SortColumn,
    pub sort_ascending: bool,

    // Seleção (suporte a seleção múltipla com Ctrl e Shift)
    pub selected_indices: BTreeSet<usize>,
    pub last_selected_idx: Option<usize>,

    // Pré-visualização (ff-preview)
    pub preview_engine: PreviewEngine,
    pub preview_result: Option<PreviewResult>,
    pub preview_target_path: Option<String>,
    pub preview_search_query: String,
    pub preview_occurrence_idx: usize,
    pub preview_panel_open: bool,

    // Treemap de espaço (ff-size)
    pub treemap_layout: Option<TreemapLayout>,
    pub treemap_dir_id: u32,
    pub treemap_history: Vec<u32>,
    pub treemap_hovered: Option<TreemapItem>,
    pub treemap_path_input: String,
    pub treemap_root_path: Option<String>,
    pub treemap_custom_index: Option<ff_index::FileIndex>,
    pub treemap_scanning_path: Option<String>,
    pub treemap_last_analyzed: Option<std::time::SystemTime>,
    pub treemap_data_source: Option<TreemapDataSource>,
    pub treemap_status: Option<TreemapAnalysisStatus>,
    pub treemap_access_errors: Vec<ScanAccessError>,
    pub treemap_receiver: Option<std::sync::mpsc::Receiver<TreemapScanResult>>,
    pub treemap_progress: Option<Arc<TreemapScanProgress>>,
    pub treemap_cancel_token: Option<Arc<AtomicBool>>,
    pub treemap_current_scan_id: u64,

    // Diff lado a lado (ff-diff)
    pub sbs_diff: Option<SideBySideDiff>,
    pub diff_current_jump: usize,
    pub diff_scroll_to_row: Option<usize>,

    // Atalho global de teclado (Win32 RegisterHotKey / Issue #17)
    pub hotkey_listener: crate::hotkey::GlobalHotkeyListener,

    // Localização e idioma ativo da interface (pt-BR / en-US)
    pub locale: Locale,

    // Notificação temporária de status (ex: exportação CSV concluída)
    pub status_notification: Option<(String, std::time::Instant)>,
}

impl Default for FastFileApp {
    fn default() -> Self {
        Self::new()
    }
}

impl FastFileApp {
    #[must_use]
    pub fn new() -> Self {
        // Tenta conectar ao daemon indexador residente via IPC
        let endpoint = default_endpoint_name();
        let client = IpcClient::new(&endpoint);
        let is_ipc_connected = client.send(&IpcRequest::Ping).is_ok();
        let ipc_client = if is_ipc_connected { Some(client) } else { None };

        // Determina a pasta raiz inicial da busca
        let default_search_root = {
            let curr = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
            let curr_str = curr.to_string_lossy();
            let is_install_dir = curr_str.contains("Programs\\FastFile")
                || curr_str.contains("Programs/FastFile")
                || (curr.join("ff.exe").exists() && curr_str.ends_with("FastFile"));

            if is_install_dir {
                if let Ok(up) = std::env::var("USERPROFILE") {
                    up
                } else if let Ok(home) = std::env::var("HOME") {
                    home
                } else {
                    curr_str.to_string()
                }
            } else {
                curr_str.to_string()
            }
        };

        // Fallback: se IPC não estiver rodando, indexa diretório padrão do usuário ou atual
        let mut local_index = None;
        let mut total_indexed = 0;

        if is_ipc_connected {
            if let Some(ref c) = ipc_client {
                if let Ok(IpcResponse::Ok(val)) = c.send(&IpcRequest::Status) {
                    total_indexed = val["entries"].as_u64().unwrap_or(0) as usize;
                }
            }
        } else {
            let mut idx = ff_index::FileIndex::new();
            let mut source =
                ff_index::WalkSource::new(&default_search_root).with_respect_ignore(true);
            if source.populate(&mut idx).is_ok() {
                total_indexed = idx.entries.len();
                local_index = Some(idx);
            }
        }

        let mut app = Self {
            query: String::new(),
            active_filter: QuickFilter::All,
            active_tab: if std::env::var("FF_START_TAB")
                .map(|v| v.to_lowercase() == "treemap")
                .unwrap_or(false)
            {
                ActiveTab::Treemap
            } else {
                ActiveTab::Search
            },

            ipc_client,
            local_index,
            is_ipc_connected,
            total_indexed_files: total_indexed,
            search_latency_ms: 0.0,

            search_target_path: default_search_root,
            search_scanning_path: None,
            search_receiver: None,

            preview_split_ratio: 0.55,
            col_width_name: 240.0,
            col_width_size: 90.0,

            results: Vec::new(),
            sort_column: SortColumn::Name,
            sort_ascending: true,

            selected_indices: BTreeSet::new(),
            last_selected_idx: None,

            preview_engine: PreviewEngine::new(),
            preview_result: None,
            preview_target_path: None,
            preview_search_query: String::new(),
            preview_occurrence_idx: 0,
            preview_panel_open: true,

            treemap_layout: None,
            treemap_dir_id: 0,
            treemap_history: Vec::new(),
            treemap_hovered: None,
            treemap_path_input: std::env::current_dir()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(|_| "C:\\".to_string()),
            treemap_root_path: None,
            treemap_custom_index: None,
            treemap_scanning_path: None,
            treemap_last_analyzed: None,
            treemap_data_source: None,
            treemap_status: None,
            treemap_access_errors: Vec::new(),
            treemap_receiver: None,
            treemap_progress: None,
            treemap_cancel_token: None,
            treemap_current_scan_id: 0,

            sbs_diff: None,
            diff_current_jump: 0,
            diff_scroll_to_row: None,

            hotkey_listener: crate::hotkey::GlobalHotkeyListener::new(),
            locale: Locale::detect_from_system(),
            status_notification: None,
        };

        // Executa busca inicial
        app.execute_search();
        app
    }

    /// Executa a consulta de busca sobre o índice residente (via IPC) ou local (Ponto 1).
    pub fn execute_search(&mut self) {
        let start = Instant::now();

        let pattern = self.query.trim();
        let mut items = Vec::new();

        if self.is_ipc_connected {
            if let Some(ref client) = self.ipc_client {
                let req_pat = if pattern.is_empty() { "*" } else { pattern };
                let req_target = if self.search_target_path.trim().is_empty()
                    || self.search_target_path == "."
                {
                    None
                } else {
                    Some(self.search_target_path.clone())
                };
                let req = ff_indexer::IpcRequest::Find {
                    pattern: req_pat.to_string(),
                    is_regex: false,
                    fuzzy: false,
                    limit: 3000,
                    target: req_target,
                    extension: None,
                    file_type: None,
                    min_size: None,
                    max_size: None,
                };
                if let Ok(ff_indexer::IpcResponse::Ok(val)) = client.send(&req) {
                    if let Some(total) = val["total_scanned"].as_u64() {
                        self.total_indexed_files = total as usize;
                    }
                    if let Some(arr) = val["matches"].as_array() {
                        for item in arr {
                            let id = item["id"].as_u64().unwrap_or(0) as u32;
                            let path = item["path"].as_str().unwrap_or("").to_string();
                            let name = item["name"].as_str().unwrap_or("").to_string();
                            let size = item["size"].as_u64().unwrap_or(0);
                            let mtime = item["mtime"].as_i64().unwrap_or(0);
                            let is_dir = item["is_dir"].as_bool().unwrap_or(false);
                            items.push(SearchResultItem {
                                id,
                                name,
                                path,
                                size,
                                mtime,
                                is_dir,
                            });
                        }
                    }
                }
            }
        } else if let Some(ref idx) = self.local_index {
            self.total_indexed_files = idx.entries.len();

            if pattern.is_empty() {
                // Se a consulta estiver vazia, exibe as primeiras entradas não deletadas
                for id in 0..idx.entries.len() {
                    let entry = &idx.entries[id];
                    if entry.is_deleted() {
                        continue;
                    }
                    if let Ok(name) = idx.name(id as u32) {
                        let path = idx
                            .build_path(id as u32)
                            .unwrap_or_else(|_| name.to_string());
                        items.push(SearchResultItem {
                            id: id as u32,
                            name: name.to_string(),
                            path,
                            size: entry.size,
                            mtime: entry.mtime,
                            is_dir: entry.is_dir(),
                        });
                    }
                    if items.len() >= 2000 {
                        break;
                    }
                }
            } else if let Ok(search_query) = Query::parse(pattern, false) {
                let search_result = SearchEngine::search(idx, &search_query);
                for m in search_result.matches {
                    let entry = &idx.entries[m.id as usize];
                    if let Ok(name) = idx.name(m.id) {
                        let path = idx.build_path(m.id).unwrap_or_else(|_| name.to_string());
                        items.push(SearchResultItem {
                            id: m.id,
                            name: name.to_string(),
                            path,
                            size: entry.size,
                            mtime: entry.mtime,
                            is_dir: entry.is_dir(),
                        });
                    }
                }
            }
        }

        // Aplica filtro rápido de categoria
        items.retain(|item| match self.active_filter {
            QuickFilter::All => true,
            QuickFilter::Code => {
                let ext = Path::new(&item.name)
                    .extension()
                    .and_then(|e| e.to_str())
                    .unwrap_or("");
                matches!(
                    ext.to_lowercase().as_str(),
                    "rs" | "py" | "js" | "ts" | "go" | "c" | "cpp" | "h" | "cs" | "html" | "css"
                )
            }
            QuickFilter::Docs => {
                let ext = Path::new(&item.name)
                    .extension()
                    .and_then(|e| e.to_str())
                    .unwrap_or("");
                matches!(
                    ext.to_lowercase().as_str(),
                    "md" | "txt" | "pdf" | "docx" | "xlsx" | "pptx" | "json" | "toml" | "yaml"
                )
            }
            QuickFilter::Media => {
                let ext = Path::new(&item.name)
                    .extension()
                    .and_then(|e| e.to_str())
                    .unwrap_or("");
                matches!(
                    ext.to_lowercase().as_str(),
                    "png" | "jpg" | "jpeg" | "gif" | "webp" | "svg" | "mp4" | "mkv" | "mp3"
                )
            }
            QuickFilter::Archives => {
                let ext = Path::new(&item.name)
                    .extension()
                    .and_then(|e| e.to_str())
                    .unwrap_or("");
                matches!(
                    ext.to_lowercase().as_str(),
                    "zip" | "tar" | "gz" | "7z" | "rar" | "iso" | "bz2"
                )
            }
            QuickFilter::Large => item.size >= 100 * 1024 * 1024,
        });

        // Aplica ordenação
        self.apply_sorting(&mut items);

        self.results = items;
        self.search_latency_ms = start.elapsed().as_secs_f32() * 1000.0;
        self.selected_indices.clear();
        self.last_selected_idx = None;
    }

    /// Dispara a varredura e reindexação de uma nova pasta de busca (local ou IPC)
    pub fn trigger_search_path_scan(&mut self, target: &str) {
        let p_norm = target.trim().to_string();
        if p_norm.is_empty() {
            return;
        }

        self.search_target_path = p_norm.clone();

        if self.is_ipc_connected {
            self.execute_search();
            return;
        }

        self.search_scanning_path = Some(p_norm.clone());
        let (tx, rx) = std::sync::mpsc::channel();
        self.search_receiver = Some(rx);

        let target_path = p_norm;
        std::thread::spawn(move || {
            let start = Instant::now();
            let mut new_idx = ff_index::FileIndex::new();
            let mut walk = ff_index::WalkSource::new(&target_path).with_respect_ignore(true);
            let _ = walk.populate(&mut new_idx);
            let elapsed_ms = start.elapsed().as_secs_f32() * 1000.0;
            let _ = tx.send((target_path, new_idx, elapsed_ms));
        });
    }

    pub fn apply_sorting(&self, items: &mut [SearchResultItem]) {
        match self.sort_column {
            SortColumn::Name => {
                if self.sort_ascending {
                    items.sort_by_key(|a| a.name.to_lowercase());
                } else {
                    items.sort_by_key(|a| std::cmp::Reverse(a.name.to_lowercase()));
                }
            }
            SortColumn::Path => {
                if self.sort_ascending {
                    items.sort_by_key(|a| a.path.to_lowercase());
                } else {
                    items.sort_by_key(|a| std::cmp::Reverse(a.path.to_lowercase()));
                }
            }
            SortColumn::Size => {
                if self.sort_ascending {
                    items.sort_by_key(|i| i.size);
                } else {
                    items.sort_by_key(|i| std::cmp::Reverse(i.size));
                }
            }
            SortColumn::Date => {
                if self.sort_ascending {
                    items.sort_by_key(|i| i.mtime);
                } else {
                    items.sort_by_key(|i| std::cmp::Reverse(i.mtime));
                }
            }
        }
    }

    /// Carrega a prévia do arquivo selecionado utilizando `ff-preview`.
    pub fn load_preview(&mut self, path: &str) {
        self.preview_target_path = Some(path.to_string());
        let term = if self.preview_search_query.is_empty() {
            None
        } else {
            Some(self.preview_search_query.as_str())
        };

        match self.preview_engine.preview_file(path, term, 1000) {
            Ok(res) => self.preview_result = Some(res),
            Err(_) => self.preview_result = None,
        }
        self.preview_occurrence_idx = 0;
    }

    /// Abre o visualizador de diff comparando os 2 arquivos selecionados na tabela.
    pub fn open_diff_for_selected(&mut self) {
        let indices: Vec<usize> = self.selected_indices.iter().copied().collect();
        if indices.len() != 2 {
            return;
        }

        let p1 = &self.results[indices[0]].path;
        let p2 = &self.results[indices[1]].path;

        if let Ok(sbs) = ff_diff::diff_files_side_by_side(p1, p2) {
            self.sbs_diff = Some(sbs);
            self.diff_current_jump = 0;
            self.active_tab = ActiveTab::Diff;
        }
    }

    /// Exporta os resultados filtrados atuais para um arquivo CSV escolhido pelo usuário.
    pub fn export_results_to_csv(&mut self) {
        if self.results.is_empty() {
            return;
        }

        let default_name = format!(
            "fastfile_resultados_{}.csv",
            if self.query.trim().is_empty() {
                "todos".to_string()
            } else {
                self.query
                    .chars()
                    .filter(|c| c.is_alphanumeric() || *c == '_' || *c == '-')
                    .collect::<String>()
            }
        );

        if let Some(dest_path) = rfd::FileDialog::new()
            .set_file_name(&default_name)
            .add_filter("Planilha CSV (*.csv)", &["csv"])
            .save_file()
        {
            let csv = generate_csv_string(&self.results);
            match std::fs::write(&dest_path, csv.as_bytes()) {
                Ok(_) => {
                    self.status_notification = Some((
                        format!(
                            "✓ {} itens exportados com sucesso para: {}",
                            self.results.len(),
                            dest_path.file_name().unwrap_or_default().to_string_lossy()
                        ),
                        std::time::Instant::now(),
                    ));
                }
                Err(e) => {
                    self.status_notification = Some((
                        format!("❌ Erro ao exportar CSV: {e}"),
                        std::time::Instant::now(),
                    ));
                }
            }
        }
    }

    /// Recalcula unicamente o layout geométrico do Treemap para os limites visuais informados,
    /// sem realizar qualquer operação de E/S ou nova varredura de disco (TM-01).
    pub fn recompute_treemap_layout(&mut self, bounds: TreemapRect) {
        let index = self
            .treemap_custom_index
            .as_ref()
            .or(self.local_index.as_ref());

        if let Some(idx) = index {
            let layout =
                ff_size::treemap::build_directory_treemap(idx, self.treemap_dir_id, bounds);
            if let Ok(path) = idx.build_path(self.treemap_dir_id) {
                self.treemap_path_input = path;
            }
            self.treemap_layout = Some(layout);
        }
    }

    /// Atualiza o cálculo do treemap para a pasta atual (compatibilidade retroativa).
    pub fn refresh_treemap(&mut self, bounds: TreemapRect) {
        self.recompute_treemap_layout(bounds);
    }

    /// Reconstrói o histórico de navegação da pasta atual até a raiz do índice.
    pub fn rebuild_treemap_history(&mut self) {
        self.treemap_history.clear();
        let idx = match self
            .treemap_custom_index
            .as_ref()
            .or(self.local_index.as_ref())
        {
            Some(i) => i,
            None => return,
        };

        if self.treemap_dir_id as usize >= idx.entries.len() {
            self.treemap_dir_id = 0;
            return;
        }

        let mut curr = self.treemap_dir_id;
        let mut chain = Vec::new();
        while curr != 0 && (curr as usize) < idx.entries.len() {
            let parent = idx.entries[curr as usize].parent;
            if parent == ff_index::NO_PARENT || parent == curr {
                break;
            }
            chain.push(parent);
            curr = parent;
        }
        chain.reverse();
        self.treemap_history = chain;
    }

    /// Cancela a varredura assíncrona do Treemap em execução (TM-02).
    pub fn cancel_treemap_scan(&mut self) {
        if let Some(ref token) = self.treemap_cancel_token {
            token.store(true, Ordering::Relaxed);
        }
        let items_scanned = self
            .treemap_progress
            .as_ref()
            .map(|p| p.items_count.load(Ordering::Relaxed))
            .unwrap_or(0);
        self.treemap_scanning_path = None;
        self.treemap_status = Some(TreemapAnalysisStatus::Cancelled { items_scanned });
        self.status_notification = Some((
            "⏹ Varredura do Treemap cancelada pelo usuário".to_string(),
            std::time::Instant::now(),
        ));
    }

    /// Dispara uma varredura assíncrona do disco em segundo plano para reler a pasta informada,
    /// garantindo que a interface nunca seja bloqueada, exibindo progresso e permitindo cancelamento (TM-01 / TM-02).
    pub fn trigger_treemap_disk_scan(&mut self, root_path: String, preserve_current_view: bool) {
        let p_check = PathBuf::from(&root_path);
        if !p_check.exists() {
            self.treemap_status = Some(TreemapAnalysisStatus::Failed(format!(
                "Caminho não encontrado: {root_path}"
            )));
            self.status_notification = Some((
                format!("❌ Caminho não encontrado: {root_path}"),
                std::time::Instant::now(),
            ));
            return;
        }

        // Cancela ou invalida a análise anterior ao iniciar outra (TM-02)
        if let Some(ref old_token) = self.treemap_cancel_token {
            old_token.store(true, Ordering::Relaxed);
        }
        self.treemap_current_scan_id = self.treemap_current_scan_id.wrapping_add(1);
        let scan_id = self.treemap_current_scan_id;

        let cancel_token = Arc::new(AtomicBool::new(false));
        self.treemap_cancel_token = Some(cancel_token.clone());

        let progress = Arc::new(TreemapScanProgress::default());
        if let Ok(mut lock) = progress.current_dir.write() {
            *lock = root_path.clone();
        }
        self.treemap_progress = Some(progress.clone());

        let preserve_path = if preserve_current_view {
            Some(self.treemap_path_input.clone())
        } else {
            None
        };

        let (tx, rx) = std::sync::mpsc::channel();
        self.treemap_receiver = Some(rx);
        self.treemap_scanning_path = Some(root_path.clone());
        self.treemap_root_path = Some(root_path.clone());

        let target_path = root_path;
        std::thread::spawn(move || {
            let start = std::time::Instant::now();
            let mut new_idx = ff_index::FileIndex::new();
            #[cfg(windows)]
            {
                let clean = target_path.trim_end_matches(['\\', '/']);
                if clean.len() <= 2 && clean.ends_with(':') {
                    if let Some(letter) = clean.chars().next() {
                        let mut mft = ff_index::MftSource::new(letter);
                        if mft.populate(&mut new_idx).is_ok() && !new_idx.entries.is_empty() {
                            let scan_ms = start.elapsed().as_secs_f64() * 1000.0;
                            let _ = tx.send(TreemapScanResult {
                                scan_id,
                                root_path: target_path,
                                preserve_path,
                                index: new_idx,
                                source: TreemapDataSource::Mft,
                                scan_ms,
                                status: TreemapAnalysisStatus::Complete,
                                access_errors: Vec::new(),
                            });
                            return;
                        }
                    }
                }
            }

            let progress_cb = progress.clone();
            let mut walk = ff_index::WalkSource::new(&target_path)
                .with_respect_ignore(false)
                .with_cancel_token(cancel_token.clone())
                .with_progress(move |path, size, is_dir| {
                    progress_cb.items_count.fetch_add(1, Ordering::Relaxed);
                    progress_cb.bytes_count.fetch_add(size, Ordering::Relaxed);
                    if is_dir {
                        if let Ok(mut lock) = progress_cb.current_dir.write() {
                            *lock = path.to_string_lossy().to_string();
                        }
                    }
                });

            let pop_res = walk.populate(&mut new_idx);
            let scan_ms = start.elapsed().as_secs_f64() * 1000.0;
            let is_cancelled = cancel_token.load(Ordering::Relaxed);
            let raw_errors = walk.take_errors();
            let access_errors: Vec<ScanAccessError> = raw_errors
                .into_iter()
                .map(|(p, err)| ScanAccessError {
                    path: p.to_string_lossy().to_string(),
                    message: err,
                })
                .collect();

            let status = if is_cancelled {
                TreemapAnalysisStatus::Cancelled {
                    items_scanned: new_idx.entries.len(),
                }
            } else if let Err(e) = pop_res {
                TreemapAnalysisStatus::Failed(e.to_string())
            } else if !access_errors.is_empty() {
                TreemapAnalysisStatus::Partial {
                    errors_count: access_errors.len(),
                }
            } else {
                TreemapAnalysisStatus::Complete
            };

            let _ = tx.send(TreemapScanResult {
                scan_id,
                root_path: target_path,
                preserve_path,
                index: new_idx,
                source: TreemapDataSource::DiskScan,
                scan_ms,
                status,
                access_errors,
            });
        });
    }

    /// Navega o Treemap para uma pasta específica do sistema de arquivos.
    pub fn navigate_treemap_to_path<P: AsRef<Path>>(&mut self, path: P, bounds: TreemapRect) {
        let p = path.as_ref();
        let mut p_norm = p.to_string_lossy().to_string();

        // Normalização de letras de volume no Windows (ex.: "c" -> "C:\", "c:" -> "C:\")
        if p_norm.len() == 1 && p_norm.chars().next().unwrap().is_ascii_alphabetic() {
            p_norm = format!("{}:\\", p_norm.to_uppercase());
        } else if p_norm.len() == 2 && p_norm.ends_with(':') {
            p_norm = format!("{}\\", p_norm.to_uppercase());
        }

        let p_check = PathBuf::from(&p_norm);
        if !p_check.exists() {
            self.status_notification = Some((
                format!("❌ Caminho não encontrado: {p_norm}"),
                std::time::Instant::now(),
            ));
            return;
        }

        self.treemap_path_input = p_norm.clone();

        // 1. Tenta localizar a pasta dentro do índice residente ou customizado atual
        let mut found_id = None;
        let idx_ref = self
            .treemap_custom_index
            .as_ref()
            .or(self.local_index.as_ref());

        if let Some(idx) = idx_ref {
            // WalkSource indexes canonical paths (without the Windows verbatim prefix).
            let canonical = p_check.canonicalize().unwrap_or_else(|_| p_check.clone());
            let canonical_str = canonical.to_string_lossy();
            let comparable = canonical_str
                .strip_prefix(r"\\?\")
                .unwrap_or(&canonical_str);
            let norm_lc = comparable.trim_end_matches(['\\', '/']).to_lowercase();
            for (id, entry) in idx.entries.iter().enumerate() {
                if entry.is_dir() && !entry.is_deleted() {
                    if let Ok(entry_path) = idx.build_path(id as u32) {
                        if entry_path.trim_end_matches(['\\', '/']).to_lowercase() == norm_lc {
                            found_id = Some(id as u32);
                            break;
                        }
                    }
                }
            }
        }

        if let Some(id) = found_id {
            self.treemap_dir_id = id;
            self.rebuild_treemap_history();
            self.recompute_treemap_layout(bounds);
        } else {
            // Se não está contido no índice em memória, inicia varredura em segundo plano (background thread)!
            self.trigger_treemap_disk_scan(p_norm, false);
        }
    }
}

impl eframe::App for FastFileApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Verifica se o atalho global de teclado foi acionado para restaurar e focar a janela (Issue #17)
        if self.hotkey_listener.check_and_reset() {
            ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
            ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
            ctx.request_repaint();
        }

        apply_custom_theme(ctx);

        // Verifica se a varredura assíncrona do Treemap concluiu
        if let Some(ref rx) = self.treemap_receiver {
            match rx.try_recv() {
                Ok(scan_res) => {
                    // Resultado de uma análise antiga não substitui o alvo atual (TM-02)
                    if scan_res.scan_id != self.treemap_current_scan_id {
                        return;
                    }

                    self.treemap_receiver = None;
                    self.treemap_scanning_path = None;
                    self.treemap_cancel_token = None;
                    let root_path = scan_res.root_path;
                    let new_idx = scan_res.index;
                    let preserve_target = scan_res.preserve_path;
                    self.treemap_status = Some(scan_res.status.clone());
                    self.treemap_access_errors = scan_res.access_errors;

                    match &scan_res.status {
                        TreemapAnalysisStatus::Cancelled { items_scanned } => {
                            self.status_notification = Some((
                                format!(
                                    "⏹ Varredura de '{}' cancelada ({items_scanned} itens identificados)",
                                    root_path
                                ),
                                std::time::Instant::now(),
                            ));
                            if !new_idx.entries.is_empty() {
                                self.treemap_custom_index = Some(new_idx);
                                self.treemap_dir_id = 0;
                                self.treemap_history.clear();
                                let available_size = ctx.screen_rect().size();
                                let treemap_rect = TreemapRect::new(
                                    0.0,
                                    0.0,
                                    available_size.x,
                                    (available_size.y - 80.0).max(100.0),
                                );
                                self.recompute_treemap_layout(treemap_rect);
                            }
                        }
                        TreemapAnalysisStatus::Failed(err) => {
                            self.status_notification = Some((
                                format!("❌ Falha na análise de '{}': {}", root_path, err),
                                std::time::Instant::now(),
                            ));
                        }
                        TreemapAnalysisStatus::Complete | TreemapAnalysisStatus::Partial { .. } => {
                            if !new_idx.entries.is_empty() {
                                self.treemap_custom_index = Some(new_idx);
                                self.treemap_last_analyzed = Some(std::time::SystemTime::now());
                                self.treemap_data_source = Some(scan_res.source);

                                // Preserva a navegação da pasta atual quando possível (TM-01)
                                let mut target_dir_id = 0;
                                if let Some(ref p_target) = preserve_target {
                                    let p_norm =
                                        p_target.trim_end_matches(['\\', '/']).to_lowercase();
                                    if let Some(ref idx) = self.treemap_custom_index {
                                        for (id, entry) in idx.entries.iter().enumerate() {
                                            if entry.is_dir() && !entry.is_deleted() {
                                                if let Ok(entry_path) = idx.build_path(id as u32) {
                                                    if entry_path
                                                        .trim_end_matches(['\\', '/'])
                                                        .to_lowercase()
                                                        == p_norm
                                                    {
                                                        target_dir_id = id as u32;
                                                        break;
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }

                                self.treemap_dir_id = target_dir_id;
                                self.rebuild_treemap_history();

                                // Atualiza treemap_path_input para o caminho real da pasta ativa
                                if let Some(ref idx) = self.treemap_custom_index {
                                    if let Ok(path) = idx.build_path(self.treemap_dir_id) {
                                        self.treemap_path_input = path;
                                    }
                                }

                                let available_size = ctx.screen_rect().size();
                                let treemap_rect = TreemapRect::new(
                                    0.0,
                                    0.0,
                                    available_size.x,
                                    (available_size.y - 80.0).max(100.0),
                                );
                                self.recompute_treemap_layout(treemap_rect);

                                let formatted_time = if scan_res.scan_ms >= 1000.0 {
                                    format!("{:.2} s", scan_res.scan_ms / 1000.0)
                                } else {
                                    format!("{:.1} ms", scan_res.scan_ms)
                                };
                                let status_suffix = match scan_res.status {
                                    TreemapAnalysisStatus::Partial { errors_count } => {
                                        format!(" (Parcial: {errors_count} pastas sem permissão)")
                                    }
                                    _ => String::new(),
                                };
                                self.status_notification = Some((
                                    format!(
                                        "✓ Treemap de '{}' atualizado ({}){}!",
                                        root_path, formatted_time, status_suffix
                                    ),
                                    std::time::Instant::now(),
                                ));
                            } else {
                                self.status_notification = Some((
                                    format!("⚠️ Nenhum arquivo encontrado em {}", root_path),
                                    std::time::Instant::now(),
                                ));
                            }
                        }
                    }
                    ctx.request_repaint();
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {
                    // Mantém o loop repintando a cada 60ms para animar o spinner enquanto varre
                    ctx.request_repaint_after(std::time::Duration::from_millis(60));
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    self.treemap_receiver = None;
                    self.treemap_scanning_path = None;
                }
            }
        }

        // Verifica se a varredura assíncrona do caminho da busca concluiu
        if let Some(ref rx) = self.search_receiver {
            match rx.try_recv() {
                Ok((path, new_idx, scan_ms)) => {
                    self.search_receiver = None;
                    self.search_scanning_path = None;
                    self.total_indexed_files = new_idx.entries.len();
                    self.local_index = Some(new_idx);
                    self.search_latency_ms = scan_ms;
                    let formatted_time = if scan_ms >= 1000.0 {
                        format!("{:.2} s", scan_ms / 1000.0)
                    } else {
                        format!("{:.1} ms", scan_ms)
                    };
                    self.status_notification = Some((
                        format!("✓ Índice atualizado para '{}' ({})", path, formatted_time),
                        std::time::Instant::now(),
                    ));
                    self.execute_search();
                    ctx.request_repaint();
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {
                    ctx.request_repaint_after(std::time::Duration::from_millis(60));
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    self.search_receiver = None;
                    self.search_scanning_path = None;
                }
            }
        }

        // Barra Superior: Campo de busca, filtros rápidos, status e abas
        egui::TopBottomPanel::top("top_header_panel")
            .frame(
                Frame::new()
                    .fill(COLOR_PANEL)
                    .inner_margin(Margin::same(12)),
            )
            .show(ctx, |ui| {
                self.render_header(ui);
            });

        // Barra Inferior de Status
        egui::TopBottomPanel::bottom("status_bar_panel")
            .frame(Frame::new().fill(COLOR_BG).inner_margin(Margin::same(8)))
            .show(ctx, |ui| {
                self.render_status_bar(ui);
            });

        // Área Central baseada na Aba ativa
        egui::CentralPanel::default()
            .frame(Frame::new().fill(COLOR_BG).inner_margin(Margin::same(8)))
            .show(ctx, |ui| match self.active_tab {
                ActiveTab::Search => self.render_search_tab(ui),
                ActiveTab::Treemap => self.render_treemap_tab(ui),
                ActiveTab::Diff => self.render_diff_tab(ui),
            });
    }
}

impl FastFileApp {
    fn render_header(&mut self, ui: &mut Ui) {
        let loc = self.locale;
        ui.horizontal(|ui| {
            // Título do aplicativo com estilo moderno
            ui.heading(
                egui::RichText::new("⚡ FastFile")
                    .color(COLOR_ACCENT)
                    .strong(),
            );

            ui.add_space(16.0);

            // Campo de busca em tempo real com ícone e atalho
            ui.label(egui::RichText::new("🔍").size(15.0));
            let query_response = ui.add_sized(
                [ui.available_width() - 320.0, 28.0],
                egui::TextEdit::singleline(&mut self.query)
                    .hint_text(loc.search_placeholder())
                    .font(egui::TextStyle::Body),
            );

            if query_response.changed() {
                self.execute_search();
            }

            ui.add_space(8.0);

            // Botão "Comparar Selecionados (2)" quando 2 arquivos estão selecionados
            let compare_enabled = self.selected_indices.len() == 2;
            if ui
                .add_enabled(
                    compare_enabled,
                    egui::Button::new(
                        egui::RichText::new(loc.compare_button(self.selected_indices.len())).color(
                            if compare_enabled {
                                Color32::WHITE
                            } else {
                                COLOR_TEXT_MUTED
                            },
                        ),
                    ),
                )
                .on_hover_text(match loc {
                    Locale::Pt => "Compara dois arquivos selecionados lado a lado",
                    Locale::Es => "Compara dos archivos seleccionados lado a lado",
                    Locale::Fr => "Compare deux fichiers sélectionnés côte à côte",
                    Locale::It => "Confronta due file selezionati fianco a fianco",
                    Locale::Ru => "Сравнивает два выбранных файла бок о бок",
                    Locale::En => "Compares two selected files side-by-side",
                })
                .clicked()
            {
                self.open_diff_for_selected();
            }
        });

        ui.add_space(8.0);

        // Linha secundária: Filtros rápidos e Abas
        ui.horizontal(|ui| {
            // Seletor de abas
            ui.selectable_value(&mut self.active_tab, ActiveTab::Search, loc.tab_results());
            ui.selectable_value(&mut self.active_tab, ActiveTab::Treemap, loc.tab_treemap());
            if self.sbs_diff.is_some() {
                ui.selectable_value(&mut self.active_tab, ActiveTab::Diff, loc.tab_diff());
            }

            ui.separator();

            // Filtros rápidos
            let filter_clicked = ui
                .selectable_value(&mut self.active_filter, QuickFilter::All, loc.filter_all())
                .clicked()
                || ui
                    .selectable_value(
                        &mut self.active_filter,
                        QuickFilter::Code,
                        loc.filter_code(),
                    )
                    .clicked()
                || ui
                    .selectable_value(
                        &mut self.active_filter,
                        QuickFilter::Docs,
                        loc.filter_docs(),
                    )
                    .clicked()
                || ui
                    .selectable_value(
                        &mut self.active_filter,
                        QuickFilter::Media,
                        loc.filter_media(),
                    )
                    .clicked()
                || ui
                    .selectable_value(
                        &mut self.active_filter,
                        QuickFilter::Archives,
                        loc.filter_archives(),
                    )
                    .clicked()
                || ui
                    .selectable_value(
                        &mut self.active_filter,
                        QuickFilter::Large,
                        loc.filter_large(),
                    )
                    .clicked();

            if filter_clicked {
                self.execute_search();
            }

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .button(if self.preview_panel_open {
                        loc.hide_preview()
                    } else {
                        loc.show_preview()
                    })
                    .clicked()
                {
                    self.preview_panel_open = !self.preview_panel_open;
                    if self.preview_panel_open && self.preview_target_path.is_none() {
                        if let Some(&first_sel) = self.selected_indices.iter().next() {
                            if let Some(item) = self.results.get(first_sel) {
                                let path = item.path.clone();
                                self.load_preview(&path);
                            }
                        } else if let Some(first_item) = self.results.first() {
                            let path = first_item.path.clone();
                            self.load_preview(&path);
                        }
                    }
                }

                if self.active_tab == ActiveTab::Search {
                    let export_btn = egui::Button::new(format!("📥 {}", loc.download_csv()));
                    let has_results = !self.results.is_empty();
                    if ui
                        .add_enabled(has_results, export_btn)
                        .on_hover_text(if has_results {
                            let count = self.results.len();
                            match loc {
                                Locale::Pt => format!(
                                    "Exporta os {count} arquivos filtrados para uma planilha CSV"
                                ),
                                Locale::Es => {
                                    format!("Exporta los {count} archivos filtrados a una hoja CSV")
                                }
                                Locale::Fr => format!(
                                    "Exporte les {count} fichiers filtrés vers un tableur CSV"
                                ),
                                Locale::It => {
                                    format!("Esporta i {count} file filtrati in un foglio CSV")
                                }
                                Locale::Ru => format!(
                                    "Экспортирует {count} отфильтрованных файлов в таблицу CSV"
                                ),
                                Locale::En => {
                                    format!("Export {count} filtered files to a CSV spreadsheet")
                                }
                            }
                        } else {
                            loc.status_no_files_to_export().to_string()
                        })
                        .clicked()
                    {
                        self.export_results_to_csv();
                    }
                }
            });
        });
    }

    fn render_status_bar(&mut self, ui: &mut Ui) {
        let loc = self.locale;
        let formatted_time = if self.search_latency_ms >= 1000.0 {
            format!("{:.2} s", self.search_latency_ms / 1000.0)
        } else {
            format!("{:.1} ms", self.search_latency_ms)
        };

        ui.horizontal(|ui| {
            // Status do daemon indexador residente ou varredura local com o tempo decorrido entre parênteses
            if self.is_ipc_connected {
                ui.label(
                    egui::RichText::new(format!("{} ({})", loc.status_mft_scan(), formatted_time))
                        .color(COLOR_SUCCESS)
                        .size(11.0),
                );
            } else {
                ui.label(
                    egui::RichText::new(format!(
                        "{} ({})",
                        loc.status_local_scan(),
                        formatted_time
                    ))
                    .color(COLOR_WARNING)
                    .size(11.0),
                );
            }

            // Indicador de varredura assíncrona da busca
            if let Some(ref scan_path) = self.search_scanning_path {
                ui.separator();
                ui.label(
                    egui::RichText::new(format!("⏳ Indexando {scan_path}..."))
                        .color(COLOR_WARNING)
                        .strong()
                        .size(11.0),
                );
            }

            ui.separator();

            // Indicador de varredura assíncrona do Treemap
            if let Some(ref scan_path) = self.treemap_scanning_path {
                let txt = match loc {
                    Locale::Pt => format!("⏳ Analisando {scan_path} em segundo plano..."),
                    Locale::Es => format!("⏳ Analizando {scan_path} en segundo plano..."),
                    Locale::Fr => format!("⏳ Analyse de {scan_path} en arrière-plan..."),
                    Locale::It => format!("⏳ Analisi di {scan_path} in background..."),
                    Locale::Ru => format!("⏳ Анализ {scan_path} в фоновом режиме..."),
                    Locale::En => format!("⏳ Scanning {scan_path} in background..."),
                };
                ui.label(
                    egui::RichText::new(txt)
                        .color(COLOR_WARNING)
                        .strong()
                        .size(11.0),
                );
                ui.separator();
            }

            // Notificação temporária de status (ex: exportação CSV)
            let mut clear_notification = false;
            if let Some((ref msg, instant)) = self.status_notification {
                if instant.elapsed().as_secs() < 8 {
                    ui.label(
                        egui::RichText::new(msg)
                            .color(if msg.starts_with('✓') {
                                COLOR_SUCCESS
                            } else {
                                COLOR_WARNING
                            })
                            .strong()
                            .size(11.5),
                    );
                    ui.separator();
                } else {
                    clear_notification = true;
                }
            }
            if clear_notification {
                self.status_notification = None;
            }

            // Contagem de resultados e latência
            ui.label(
                egui::RichText::new(
                    loc.status_results_count(self.results.len(), self.total_indexed_files),
                )
                .color(COLOR_TEXT_MUTED)
                .size(11.0),
            );

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                // Seletor de Idioma / Locale Switcher
                let lang_btn =
                    ui.small_button(format!("🌐 {}", loc.code()))
                        .on_hover_text(format!(
                            "{} ({})\nClique para alternar / Click to switch",
                            loc.display_name(),
                            loc.code()
                        ));
                if lang_btn.clicked() {
                    self.locale = self.locale.toggle();
                    ui.ctx().send_viewport_cmd(egui::ViewportCommand::Title(
                        self.locale.window_title().to_string(),
                    ));
                }

                ui.separator();

                ui.label(
                    egui::RichText::new(format!("{:.1} ms", self.search_latency_ms))
                        .color(COLOR_ACCENT)
                        .size(11.0),
                );
            });
        });
    }

    fn render_search_tab(&mut self, ui: &mut Ui) {
        let loc = self.locale;

        // Barra de Pasta / Caminho Alvo da Busca
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("📂").size(14.0));
            ui.label(
                egui::RichText::new(loc.label_search_path())
                    .color(COLOR_TEXT_MUTED)
                    .size(12.0),
            );

            let path_input = ui.add_sized(
                [(ui.available_width() - 250.0).max(120.0), 24.0],
                egui::TextEdit::singleline(&mut self.search_target_path)
                    .hint_text("C:\\ ou pasta para pesquisar...")
                    .font(egui::TextStyle::Monospace),
            );
            if path_input.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                let p = self.search_target_path.clone();
                self.trigger_search_path_scan(&p);
            }

            if ui
                .button(format!("📁 {}", loc.btn_browse()))
                .on_hover_text("Escolher pasta no computador")
                .clicked()
            {
                if let Some(folder) = rfd::FileDialog::new().pick_folder() {
                    let p_str = folder.to_string_lossy().to_string();
                    self.trigger_search_path_scan(&p_str);
                }
            }

            if ui
                .small_button("🏠")
                .on_hover_text("Pasta do Usuário")
                .clicked()
            {
                if let Ok(up) = std::env::var("USERPROFILE").or_else(|_| std::env::var("HOME")) {
                    self.trigger_search_path_scan(&up);
                }
            }

            #[cfg(windows)]
            if ui
                .small_button("C:\\")
                .on_hover_text("Unidade C:\\")
                .clicked()
            {
                self.trigger_search_path_scan("C:\\");
            }

            if let Some(ref scanning) = self.search_scanning_path {
                ui.label(
                    egui::RichText::new(format!("⏳ Indexando {scanning}..."))
                        .color(COLOR_WARNING)
                        .strong()
                        .size(11.5),
                );
            }
        });

        ui.add_space(4.0);
        ui.separator();

        if self.preview_panel_open {
            // Layout dividido com pré-visualização à direita e separador redimensionável (Splitter arrastável)
            let available_size = ui.available_size();
            let height = available_size.y;
            let min_list_w = 260.0;
            let min_prev_w = 200.0;

            let mut list_width = self.preview_split_ratio * available_size.x;
            list_width = list_width.clamp(
                min_list_w,
                (available_size.x - min_prev_w - 12.0).max(min_list_w),
            );
            let preview_width = (available_size.x - list_width - 12.0).max(min_prev_w);

            ui.horizontal(|ui| {
                ui.allocate_ui_with_layout(
                    egui::vec2(list_width, height),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| {
                        self.render_virtualized_table(ui);
                    },
                );

                // Separador interativo arrastável (Splitter entre Lista e Prévia)
                let (sep_rect, sep_resp) =
                    ui.allocate_exact_size(egui::vec2(8.0, height), Sense::drag());

                if sep_resp.hovered() || sep_resp.dragged() {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
                }

                if sep_resp.dragged() {
                    let new_w = list_width + sep_resp.drag_delta().x;
                    self.preview_split_ratio = (new_w / available_size.x).clamp(0.20, 0.85);
                }

                let sep_color = if sep_resp.hovered() || sep_resp.dragged() {
                    COLOR_ACCENT
                } else {
                    COLOR_BORDER
                };
                ui.painter().rect_filled(
                    egui::Rect::from_center_size(sep_rect.center(), egui::vec2(2.0, height)),
                    CornerRadius::ZERO,
                    sep_color,
                );

                ui.allocate_ui_with_layout(
                    egui::vec2(preview_width, height),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| {
                        self.render_preview_panel(ui);
                    },
                );
            });
        } else {
            // Tabela ocupando a largura inteira
            self.render_virtualized_table(ui);
        }
    }

    fn render_virtualized_table(&mut self, ui: &mut Ui) {
        let loc = self.locale;
        // Cabeçalho clicável e redimensionável das colunas para ordenação e ajuste de largura
        ui.horizontal(|ui| {
            // Coluna Nome
            ui.allocate_ui_with_layout(
                egui::vec2(self.col_width_name, 22.0),
                egui::Layout::left_to_right(egui::Align::Center),
                |ui| {
                    let name_label = if self.sort_column == SortColumn::Name {
                        let arrow = if self.sort_ascending { " ▲" } else { " ▼" };
                        format!("{}{}", loc.col_name(), arrow)
                    } else {
                        loc.col_name().to_string()
                    };
                    if ui
                        .selectable_label(self.sort_column == SortColumn::Name, name_label)
                        .clicked()
                    {
                        if self.sort_column == SortColumn::Name {
                            self.sort_ascending = !self.sort_ascending;
                        } else {
                            self.sort_column = SortColumn::Name;
                            self.sort_ascending = true;
                        }
                        let mut res = self.results.clone();
                        self.apply_sorting(&mut res);
                        self.results = res;
                    }
                },
            );

            // Alça de redimensionamento entre Nome e Tamanho
            let (h_name_rect, h_name_resp) =
                ui.allocate_exact_size(egui::vec2(8.0, 22.0), Sense::drag());
            if h_name_resp.hovered() || h_name_resp.dragged() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
            }
            if h_name_resp.dragged() {
                self.col_width_name =
                    (self.col_width_name + h_name_resp.drag_delta().x).clamp(80.0, 700.0);
            }
            let h_name_color = if h_name_resp.hovered() || h_name_resp.dragged() {
                COLOR_ACCENT
            } else {
                COLOR_BORDER
            };
            ui.painter().line_segment(
                [h_name_rect.center_top(), h_name_rect.center_bottom()],
                Stroke::new(1.5_f32, h_name_color),
            );

            // Coluna Tamanho
            ui.allocate_ui_with_layout(
                egui::vec2(self.col_width_size, 22.0),
                egui::Layout::left_to_right(egui::Align::Center),
                |ui| {
                    let size_label = if self.sort_column == SortColumn::Size {
                        let arrow = if self.sort_ascending { " ▲" } else { " ▼" };
                        format!("{}{}", loc.col_size(), arrow)
                    } else {
                        loc.col_size().to_string()
                    };
                    if ui
                        .selectable_label(self.sort_column == SortColumn::Size, size_label)
                        .clicked()
                    {
                        if self.sort_column == SortColumn::Size {
                            self.sort_ascending = !self.sort_ascending;
                        } else {
                            self.sort_column = SortColumn::Size;
                            self.sort_ascending = false;
                        }
                        let mut res = self.results.clone();
                        self.apply_sorting(&mut res);
                        self.results = res;
                    }
                },
            );

            // Alça de redimensionamento entre Tamanho e Caminho
            let (h_size_rect, h_size_resp) =
                ui.allocate_exact_size(egui::vec2(8.0, 22.0), Sense::drag());
            if h_size_resp.hovered() || h_size_resp.dragged() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
            }
            if h_size_resp.dragged() {
                self.col_width_size =
                    (self.col_width_size + h_size_resp.drag_delta().x).clamp(50.0, 300.0);
            }
            let h_size_color = if h_size_resp.hovered() || h_size_resp.dragged() {
                COLOR_ACCENT
            } else {
                COLOR_BORDER
            };
            ui.painter().line_segment(
                [h_size_rect.center_top(), h_size_rect.center_bottom()],
                Stroke::new(1.5_f32, h_size_color),
            );

            // Coluna Caminho
            let path_label = if self.sort_column == SortColumn::Path {
                let arrow = if self.sort_ascending { " ▲" } else { " ▼" };
                format!("{}{}", loc.col_path(), arrow)
            } else {
                loc.col_path().to_string()
            };
            if ui
                .selectable_label(self.sort_column == SortColumn::Path, path_label)
                .clicked()
            {
                if self.sort_column == SortColumn::Path {
                    self.sort_ascending = !self.sort_ascending;
                } else {
                    self.sort_column = SortColumn::Path;
                    self.sort_ascending = true;
                }
                let mut res = self.results.clone();
                self.apply_sorting(&mut res);
                self.results = res;
            }
        });

        ui.separator();

        if self.results.is_empty() {
            ui.vertical_centered(|ui| {
                ui.add_space(60.0);
                ui.label(egui::RichText::new("🔍").size(32.0).color(COLOR_TEXT_MUTED));
                ui.add_space(8.0);
                ui.label(
                    egui::RichText::new(loc.no_files_found())
                        .color(COLOR_TEXT_MUTED)
                        .size(13.0),
                );
            });
            return;
        }

        // Lista Virtualizada (egui::ScrollArea com show_rows para suportar 1M+ linhas instantaneamente)
        let row_height = 24.0;
        let num_rows = self.results.len();

        let mut row_clicked = None;
        let ctrl_pressed = ui.input(|i| i.modifiers.ctrl || i.modifiers.command);
        let shift_pressed = ui.input(|i| i.modifiers.shift);

        ScrollArea::vertical()
            .auto_shrink([false, false])
            .show_rows(ui, row_height, num_rows, |ui, row_range| {
                for idx in row_range {
                    let item = &self.results[idx];
                    let is_selected = self.selected_indices.contains(&idx);

                    let bg_color = if is_selected {
                        COLOR_ACCENT.linear_multiply(0.25)
                    } else if idx % 2 == 1 {
                        COLOR_PANEL
                    } else {
                        COLOR_BG
                    };

                    let response = Frame::new()
                        .fill(bg_color)
                        .corner_radius(CornerRadius::same(4))
                        .inner_margin(Margin::symmetric(6, 3))
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                let icon = if item.is_dir { "📁" } else { "📄" };
                                ui.label(icon);

                                // Nome do arquivo (largura ajustável)
                                ui.add_sized(
                                    [self.col_width_name, 18.0],
                                    egui::Label::new(
                                        egui::RichText::new(&item.name)
                                            .color(if is_selected {
                                                Color32::WHITE
                                            } else {
                                                COLOR_TEXT_PRIMARY
                                            })
                                            .strong(),
                                    )
                                    .truncate(),
                                );

                                ui.add_space(8.0);

                                // Tamanho formatado (largura ajustável)
                                ui.add_sized(
                                    [self.col_width_size, 18.0],
                                    egui::Label::new(
                                        egui::RichText::new(format_size_bytes(item.size))
                                            .color(COLOR_TEXT_MUTED)
                                            .monospace(),
                                    ),
                                );

                                ui.add_space(8.0);

                                // Caminho completo (preenche restante)
                                ui.add(
                                    egui::Label::new(
                                        egui::RichText::new(&item.path)
                                            .color(COLOR_TEXT_MUTED)
                                            .size(11.5),
                                    )
                                    .truncate(),
                                );
                            });
                        })
                        .response
                        .interact(Sense::click());

                    if response.clicked() {
                        row_clicked = Some(idx);
                    }
                }
            });

        // Trata seleções com suporte a Ctrl (múltipla) e Shift (intervalo)
        if let Some(idx) = row_clicked {
            if ctrl_pressed {
                if self.selected_indices.contains(&idx) {
                    self.selected_indices.remove(&idx);
                } else {
                    self.selected_indices.insert(idx);
                }
                self.last_selected_idx = Some(idx);
            } else if shift_pressed && self.last_selected_idx.is_some() {
                let start = self.last_selected_idx.unwrap().min(idx);
                let end = self.last_selected_idx.unwrap().max(idx);
                self.selected_indices.clear();
                for i in start..=end {
                    self.selected_indices.insert(i);
                }
            } else {
                self.selected_indices.clear();
                self.selected_indices.insert(idx);
                self.last_selected_idx = Some(idx);
            }

            // Atualiza a prévia para o item selecionado
            let path = self.results[idx].path.clone();
            self.load_preview(&path);
        }
    }

    fn render_preview_panel(&mut self, ui: &mut Ui) {
        let loc = self.locale;
        let path = match self.preview_target_path {
            Some(ref p) => p.clone(),
            None => {
                ui.vertical_centered(|ui| {
                    ui.add_space(60.0);
                    ui.label(egui::RichText::new("👁️").size(36.0).color(COLOR_TEXT_MUTED));
                    ui.add_space(8.0);
                    ui.label(
                        egui::RichText::new(match loc {
                            Locale::Pt => "Selecione um arquivo da lista para pré-visualizar",
                            Locale::Es => "Seleccione un archivo de la lista para vista previa",
                            Locale::Fr => "Sélectionnez un fichier dans la liste pour un aperçu",
                            Locale::It => "Seleziona un file dall'elenco per l'anteprima",
                            Locale::Ru => "Выберите файл из списка для предпросмотра",
                            Locale::En => "Select a file from the list to preview",
                        })
                        .color(COLOR_TEXT_MUTED)
                        .size(13.0),
                    );
                });
                return;
            }
        };

        ui.horizontal(|ui| {
            ui.heading(
                egui::RichText::new(loc.preview_header())
                    .color(COLOR_TEXT_PRIMARY)
                    .size(15.0),
            );

            ui.add_space(8.0);
            ui.label(
                egui::RichText::new(
                    Path::new(&path)
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy(),
                )
                .color(COLOR_ACCENT)
                .monospace(),
            );
        });

        // Barra de busca dentro do arquivo com navegação (apenas para arquivos de texto)
        if matches!(self.preview_result, Some(PreviewResult::Text(_))) {
            ui.horizontal(|ui| {
                ui.label(loc.preview_filter_hint());
                let resp = ui.add_sized(
                    [180.0, 22.0],
                    egui::TextEdit::singleline(&mut self.preview_search_query).hint_text("..."),
                );

                if resp.changed() {
                    let p = path.clone();
                    self.load_preview(&p);
                }

                if let Some(PreviewResult::Text(ref text_res)) = self.preview_result {
                    if !text_res.occurrences.is_empty() {
                        ui.label(
                            egui::RichText::new(format!(
                                "{}/{} {}",
                                self.preview_occurrence_idx + 1,
                                text_res.occurrences.len(),
                                if loc == Locale::Pt {
                                    "ocorrências"
                                } else {
                                    "matches"
                                }
                            ))
                            .color(COLOR_WARNING)
                            .size(11.0),
                        );

                        if ui.button("◀").clicked() && self.preview_occurrence_idx > 0 {
                            self.preview_occurrence_idx -= 1;
                        }
                        if ui.button("▶").clicked()
                            && self.preview_occurrence_idx + 1 < text_res.occurrences.len()
                        {
                            self.preview_occurrence_idx += 1;
                        }
                    }
                }
            });
            ui.separator();
        }

        match self.preview_result {
            Some(PreviewResult::Text(ref text_preview)) => {
                let lines_label = match loc {
                    Locale::Pt => "linhas",
                    Locale::Es => "líneas",
                    Locale::Fr => "lignes",
                    Locale::It => "righe",
                    Locale::Ru => "строк",
                    Locale::En => "lines",
                };
                let type_label = match loc {
                    Locale::Pt | Locale::Es | Locale::It => "Tipo",
                    Locale::Fr | Locale::En => "Type",
                    Locale::Ru => "Тип",
                };
                let total_label = "Total";
                ui.label(
                    egui::RichText::new(format!(
                        "{type_label}: {} • {total_label}: {} {lines_label}",
                        text_preview.syntax_name, text_preview.total_lines
                    ))
                    .color(COLOR_TEXT_MUTED)
                    .size(11.0),
                );
                ui.add_space(4.0);

                ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show_rows(ui, 20.0, text_preview.lines.len(), |ui, row_range| {
                        for idx in row_range {
                            let line = &text_preview.lines[idx];
                            ui.horizontal(|ui| {
                                // Número da linha
                                ui.add_sized(
                                    [40.0, 18.0],
                                    egui::Label::new(
                                        egui::RichText::new(format!("{:4}", line.lineno))
                                            .color(COLOR_TEXT_MUTED)
                                            .monospace(),
                                    ),
                                );

                                // Spans com realce de sintaxe syntect
                                for span in &line.spans {
                                    let color = if span.is_match {
                                        Color32::from_rgb(255, 220, 0)
                                    } else {
                                        Color32::from_rgb(span.color.r, span.color.g, span.color.b)
                                    };

                                    let mut rich =
                                        egui::RichText::new(&span.text).color(color).monospace();
                                    if span.is_match {
                                        rich = rich.background_color(Color32::from_rgb(80, 70, 0));
                                    }
                                    ui.label(rich);
                                }
                            });
                        }
                    });
            }
            Some(PreviewResult::Binary(ref bin_preview)) => {
                let ext = Path::new(&path)
                    .extension()
                    .and_then(|e| e.to_str())
                    .map(|e| e.to_lowercase())
                    .unwrap_or_default();

                let size_str = format_size_bytes(bin_preview.size);
                let type_desc = match ext.as_str() {
                    "png" | "jpg" | "jpeg" | "gif" | "bmp" | "ico" | "webp" => {
                        let img_label = match loc {
                            Locale::Pt | Locale::Es => "Imagem",
                            Locale::Fr => "Image",
                            Locale::It => "Immagine",
                            Locale::Ru => "Изображение",
                            Locale::En => "Image",
                        };
                        format!(
                            "🖼️ {img_label} ({}) — {}: {size_str}",
                            ext.to_uppercase(),
                            loc.col_size()
                        )
                    }
                    "exe" | "dll" | "so" | "dylib" => {
                        let bin_label = match loc {
                            Locale::Pt => "Executável/Biblioteca",
                            Locale::Es => "Ejecutable/Biblioteca",
                            Locale::Fr => "Exécutable/Bibliothèque",
                            Locale::It => "Eseguibile/Libreria",
                            Locale::Ru => "Исполняемый файл/Библиотека",
                            Locale::En => "Executable/Binary",
                        };
                        format!(
                            "⚙️ {bin_label} ({}) — {}: {size_str}",
                            ext.to_uppercase(),
                            loc.col_size()
                        )
                    }
                    "zip" | "tar" | "gz" | "7z" | "rar" => {
                        let arch_label = match loc {
                            Locale::Pt => "Pacote Compactado",
                            Locale::Es => "Archivo Comprimido",
                            Locale::Fr => "Archive Compressée",
                            Locale::It => "Archivio Compresso",
                            Locale::Ru => "Сжатый Архив",
                            Locale::En => "Archive",
                        };
                        format!(
                            "📦 {arch_label} ({}) — {}: {size_str}",
                            ext.to_uppercase(),
                            loc.col_size()
                        )
                    }
                    _ => loc.preview_binary_title(&size_str),
                };

                ui.label(
                    egui::RichText::new(format!("{type_desc} (Hex Dump)"))
                        .color(COLOR_WARNING)
                        .strong(),
                );

                ui.add_space(4.0);

                let hex_lines = &bin_preview.hex_dump;
                ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show_rows(ui, 19.0, hex_lines.len(), |ui, row_range| {
                        for idx in row_range {
                            ui.label(
                                egui::RichText::new(&hex_lines[idx])
                                    .color(COLOR_TEXT_PRIMARY)
                                    .monospace()
                                    .size(11.5),
                            );
                        }
                    });
            }
            Some(PreviewResult::Empty) => {
                ui.label(
                    egui::RichText::new(if loc == Locale::Pt {
                        "Arquivo vazio"
                    } else {
                        "Empty file"
                    })
                    .color(COLOR_TEXT_MUTED),
                );
            }
            None => {
                ui.label(egui::RichText::new(loc.preview_loading()).color(COLOR_TEXT_MUTED));
            }
        }
    }

    fn render_treemap_tab(&mut self, ui: &mut Ui) {
        let loc = self.locale;
        let available_size = ui.available_size();
        let treemap_rect = TreemapRect::new(
            0.0,
            0.0,
            available_size.x,
            (available_size.y - 80.0).max(100.0),
        );

        // 1. Barra de Ações: Seleção de Pasta, Campo de Texto e Análise
        ui.horizontal(|ui| {
            if ui
                .button(loc.treemap_choose_folder())
                .on_hover_text(if loc == Locale::Pt {
                    "Abre diálogo nativo do sistema para selecionar qualquer pasta ou disco"
                } else {
                    "Opens native system dialog to choose any folder or drive"
                })
                .clicked()
            {
                if let Some(folder) = rfd::FileDialog::new().pick_folder() {
                    self.navigate_treemap_to_path(&folder, treemap_rect);
                }
            }

            ui.add_space(4.0);

            // Largura necessária para os 3 botões da direita (Analisar, Subir, Recalcular) mais margem segura
            let trailing_buttons_w = 420.0;
            let path_input_w = (ui.available_width() - trailing_buttons_w).max(120.0);

            let path_resp = ui.add_sized(
                [path_input_w, 22.0],
                egui::TextEdit::singleline(&mut self.treemap_path_input).hint_text(
                    if loc == Locale::Pt {
                        "Digite ou cole o caminho da pasta... Ex: C:\\ ou D:\\Projetos"
                    } else {
                        "Type or paste folder path... E.g. C:\\ or D:\\Projects"
                    },
                ),
            );

            if path_resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                let target = PathBuf::from(self.treemap_path_input.trim());
                self.navigate_treemap_to_path(&target, treemap_rect);
            }

            if ui
                .button(loc.treemap_analyze())
                .on_hover_text(loc.treemap_analyze_tooltip())
                .clicked()
            {
                let target = PathBuf::from(self.treemap_path_input.trim());
                self.navigate_treemap_to_path(&target, treemap_rect);
            }

            if ui
                .button(loc.treemap_up_folder())
                .on_hover_text(loc.treemap_up_folder_tooltip())
                .clicked()
            {
                if !self.treemap_history.is_empty() {
                    self.treemap_dir_id = self.treemap_history.pop().unwrap_or(0);
                    self.recompute_treemap_layout(treemap_rect);
                } else {
                    let cur = PathBuf::from(&self.treemap_path_input);
                    if let Some(parent) = cur.parent() {
                        self.navigate_treemap_to_path(parent, treemap_rect);
                    }
                }
            }

            if ui
                .add_enabled(
                    self.treemap_receiver.is_none(),
                    egui::Button::new(loc.treemap_refresh()),
                )
                .on_hover_text(loc.treemap_refresh_tooltip())
                .clicked()
            {
                let target = self
                    .treemap_root_path
                    .clone()
                    .unwrap_or_else(|| self.treemap_path_input.clone());
                self.trigger_treemap_disk_scan(target, true);
            }
        });

        // 2. Atalhos Rápidos para Drives e Pastas do Sistema
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new(loc.treemap_quick_links())
                    .color(COLOR_TEXT_MUTED)
                    .size(11.0),
            );

            if ui.small_button("C:\\").clicked() {
                self.navigate_treemap_to_path(Path::new("C:\\"), treemap_rect);
            }

            if Path::new("D:\\").exists() && ui.small_button("D:\\").clicked() {
                self.navigate_treemap_to_path(Path::new("D:\\"), treemap_rect);
            }

            if let Ok(user_dir) = std::env::var("USERPROFILE").or_else(|_| std::env::var("HOME")) {
                let user_path = PathBuf::from(&user_dir);
                if ui.small_button(loc.treemap_shortcut_user()).clicked() {
                    self.navigate_treemap_to_path(&user_path, treemap_rect);
                }

                let docs = user_path.join("Documents");
                if docs.exists() && ui.small_button(loc.treemap_shortcut_docs()).clicked() {
                    self.navigate_treemap_to_path(&docs, treemap_rect);
                }

                let dl = user_path.join("Downloads");
                if dl.exists() && ui.small_button(loc.treemap_shortcut_downloads()).clicked() {
                    self.navigate_treemap_to_path(&dl, treemap_rect);
                }

                let dt = user_path.join("Desktop");
                if dt.exists() && ui.small_button(loc.treemap_shortcut_desktop()).clicked() {
                    self.navigate_treemap_to_path(&dt, treemap_rect);
                }
            }
        });

        // 3. Barra de Metadados: Horário da última análise, Origem dos dados, Status e Total
        ui.horizontal(|ui| {
            if let Some(ref last_analyzed) = self.treemap_last_analyzed {
                let secs = last_analyzed
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_secs() as i64)
                    .unwrap_or(0);
                let time_str = format_unix_timestamp(secs);
                let source_str = self
                    .treemap_data_source
                    .map(|s| s.label(loc))
                    .unwrap_or_else(|| loc.treemap_source_disk());

                ui.label(
                    egui::RichText::new(loc.treemap_last_analysis(&time_str, source_str))
                        .color(COLOR_TEXT_MUTED)
                        .size(11.5),
                );

                if let Some(ref status) = self.treemap_status {
                    match status {
                        TreemapAnalysisStatus::Complete => {
                            ui.label(
                                egui::RichText::new(format!("• {}", loc.treemap_status_complete()))
                                    .color(COLOR_SUCCESS)
                                    .size(11.5)
                                    .strong(),
                            );
                        }
                        TreemapAnalysisStatus::Partial { errors_count } => {
                            let resp = ui.label(
                                egui::RichText::new(format!(
                                    "• ⚠️ {}",
                                    loc.treemap_status_partial(*errors_count)
                                ))
                                .color(COLOR_WARNING)
                                .size(11.5)
                                .strong(),
                            );
                            if !self.treemap_access_errors.is_empty() {
                                resp.on_hover_ui(|ui| {
                                    ui.label(
                                        egui::RichText::new(loc.treemap_access_errors_title())
                                            .strong(),
                                    );
                                    for err in self.treemap_access_errors.iter().take(8) {
                                        ui.label(format!("• {}: {}", err.path, err.message));
                                    }
                                    if self.treemap_access_errors.len() > 8 {
                                        ui.label(format!(
                                            "... e mais {} ocorrências",
                                            self.treemap_access_errors.len() - 8
                                        ));
                                    }
                                });
                            }
                        }
                        TreemapAnalysisStatus::Cancelled { items_scanned } => {
                            ui.label(
                                egui::RichText::new(format!(
                                    "• ⏹ {}",
                                    loc.treemap_status_cancelled(*items_scanned)
                                ))
                                .color(COLOR_TEXT_MUTED)
                                .size(11.5),
                            );
                        }
                        TreemapAnalysisStatus::Failed(err) => {
                            ui.label(
                                egui::RichText::new(format!(
                                    "• ❌ {}",
                                    loc.treemap_status_failed(err)
                                ))
                                .color(COLOR_DANGER)
                                .size(11.5),
                            );
                        }
                    }
                }
            } else if self.treemap_scanning_path.is_some() {
                ui.label(
                    egui::RichText::new("🔄 Atualizando dados do disco...")
                        .color(COLOR_ACCENT)
                        .size(11.5),
                );
            }

            if let Some(ref layout) = self.treemap_layout {
                let items_label = match loc {
                    Locale::Pt => "itens",
                    Locale::Es => "elementos",
                    Locale::Fr => "éléments",
                    Locale::It => "elementi",
                    Locale::Ru => "элементов",
                    Locale::En => "items",
                };
                let total_text = format!(
                    "Total: {} ({} {items_label})",
                    format_size_bytes(layout.total_size),
                    layout.items.len()
                );
                // Espaço flexível para empurrar à direita sem estourar e sem sequestrar altura
                let label_w = (total_text.len() as f32 * 8.5) + 32.0;
                let space = (ui.available_width() - label_w).max(8.0);
                ui.add_space(space);
                ui.label(egui::RichText::new(total_text).color(COLOR_ACCENT).strong());
            }
        });

        // Exibe tela amigável de carregamento com progresso em tempo real e cancelamento (TM-02)
        if let Some(scan_path) = self.treemap_scanning_path.clone() {
            let mut cancel_clicked = false;
            let progress_data = self.treemap_progress.as_ref().map(|p| {
                let items = p.items_count.load(Ordering::Relaxed);
                let bytes = p.bytes_count.load(Ordering::Relaxed);
                let curr_dir = p
                    .current_dir
                    .read()
                    .map(|d| d.clone())
                    .unwrap_or_else(|_| scan_path.clone());
                let errors = p.access_errors_count.load(Ordering::Relaxed);
                (items, bytes, curr_dir, errors)
            });

            ui.add_space(40.0);
            ui.centered_and_justified(|ui| {
                ui.vertical_centered(|ui| {
                    ui.add(egui::Spinner::new().size(40.0).color(COLOR_ACCENT));
                    ui.add_space(14.0);
                    ui.heading(
                        egui::RichText::new(loc.treemap_scanning_title())
                            .color(COLOR_ACCENT)
                            .size(17.0),
                    );
                    ui.add_space(8.0);

                    // Barra de progresso indeterminada (TM-02)
                    let pbar = egui::ProgressBar::new(0.0).animate(true);
                    ui.add_sized([380.0, 16.0], pbar);
                    ui.add_space(10.0);

                    // Informações de progresso em tempo real (itens, bytes, diretório)
                    if let Some((items, bytes, curr_dir, errors)) = progress_data {
                        ui.label(
                            egui::RichText::new(loc.treemap_scanning_dir_label(&curr_dir))
                                .color(COLOR_TEXT_PRIMARY)
                                .size(12.5),
                        );
                        ui.add_space(4.0);
                        ui.label(
                            egui::RichText::new(
                                loc.treemap_scanning_stats(items, &format_size_bytes(bytes)),
                            )
                            .color(COLOR_ACCENT)
                            .size(12.0)
                            .strong(),
                        );

                        if errors > 0 {
                            ui.add_space(4.0);
                            ui.label(
                                egui::RichText::new(format!(
                                    "⚠️ {errors} pastas com acesso negado até o momento"
                                ))
                                .color(COLOR_WARNING)
                                .size(11.5),
                            );
                        }
                    } else {
                        ui.label(
                            egui::RichText::new(loc.treemap_scanning_path(&scan_path))
                                .color(COLOR_TEXT_PRIMARY)
                                .size(13.0),
                        );
                    }

                    ui.add_space(16.0);

                    // Botão de Cancelar Varredura (TM-02)
                    if ui
                        .add(egui::Button::new(
                            egui::RichText::new(loc.treemap_cancel()).color(COLOR_DANGER),
                        ))
                        .clicked()
                    {
                        cancel_clicked = true;
                    }

                    ui.add_space(10.0);
                    ui.label(
                        egui::RichText::new(loc.treemap_scanning_hint())
                            .color(COLOR_TEXT_MUTED)
                            .size(11.5),
                    );
                });
            });

            if cancel_clicked {
                self.cancel_treemap_scan();
            }
            return;
        }

        // 3. Breadcrumbs / Migalhas de pão interativas
        let cur_path = PathBuf::from(&self.treemap_path_input);
        let ancestors: Vec<PathBuf> = cur_path.ancestors().map(|p| p.to_path_buf()).collect();
        if ancestors.len() > 1 {
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("📍").size(11.0));
                let mut clicked_ancestor = None;
                for anc in ancestors.iter().rev() {
                    let name = anc
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or_else(|| anc.to_str().unwrap_or("/"));
                    if ui.small_button(name).clicked() {
                        clicked_ancestor = Some(anc.clone());
                    }
                    ui.label(egui::RichText::new("›").color(COLOR_TEXT_MUTED));
                }
                if let Some(target) = clicked_ancestor {
                    self.navigate_treemap_to_path(&target, treemap_rect);
                }
            });
        }

        ui.separator();

        let painter_height = (ui.available_height() - 4.0).max(100.0);
        let painter_width = ui.available_width();
        let treemap_rect = TreemapRect::new(0.0, 0.0, painter_width, painter_height);

        if let Some(ref layout) = self.treemap_layout {
            // Se as dimensões da janela mudaram, reorganiza o mapa geometricamente sem tocar no disco (TM-01)
            if (layout.bounds.w - painter_width).abs() > 1.0
                || (layout.bounds.h - painter_height).abs() > 1.0
            {
                self.recompute_treemap_layout(treemap_rect);
            }
        } else if self.treemap_scanning_path.is_none() && self.treemap_receiver.is_none() {
            // Dispara varredura inicial se ainda não há dados nem varredura ativa
            let target = self.treemap_path_input.clone();
            self.trigger_treemap_disk_scan(target, false);
        }

        let (response, painter) =
            ui.allocate_painter(Vec2::new(painter_width, painter_height), Sense::click());

        let rect_origin = response.rect.min;
        let mut clicked_dir = None;

        if let Some(ref layout) = self.treemap_layout {
            let mouse_pos = ui.input(|i| i.pointer.hover_pos());

            for item in &layout.items {
                let item_rect = Rect::from_min_size(
                    rect_origin + egui::vec2(item.rect.x, item.rect.y),
                    egui::vec2(item.rect.w, item.rect.h),
                );

                let is_hovered = mouse_pos.is_some_and(|pos| item_rect.contains(pos));

                let fill = Color32::from_rgb(item.color.0, item.color.1, item.color.2);
                let border = if is_hovered {
                    Color32::WHITE
                } else {
                    COLOR_BORDER
                };

                painter.rect_filled(item_rect, CornerRadius::same(2), fill);
                painter.rect_stroke(
                    item_rect,
                    CornerRadius::same(2),
                    Stroke::new(1.0_f32, border),
                    StrokeKind::Inside,
                );

                // Escreve o nome e tamanho nos retângulos que tiverem espaço suficiente
                if item.rect.w >= 50.0 && item.rect.h >= 20.0 {
                    let label = format!("{}\n{}", item.name, format_size_bytes(item.size));
                    painter.text(
                        item_rect.center(),
                        egui::Align2::CENTER_CENTER,
                        label,
                        FontId::new(10.5, egui::FontFamily::Proportional),
                        Color32::WHITE,
                    );
                }

                if is_hovered {
                    egui::show_tooltip_at_pointer(
                        ui.ctx(),
                        ui.layer_id(),
                        response.id.with(item.id),
                        |ui: &mut egui::Ui| {
                            ui.label(egui::RichText::new(&item.name).strong());
                            ui.label(format!("Tamanho: {}", format_size_bytes(item.size)));
                            if item.is_dir {
                                ui.label(
                                    egui::RichText::new(if loc == Locale::Pt {
                                        "📁 Diretório (clique para abrir)"
                                    } else {
                                        "📁 Directory (click to open)"
                                    })
                                    .italics()
                                    .size(10.5),
                                );
                            } else {
                                ui.label(
                                    egui::RichText::new(if loc == Locale::Pt {
                                        "📄 Arquivo"
                                    } else {
                                        "📄 File"
                                    })
                                    .italics()
                                    .size(10.5),
                                );
                            }
                        },
                    );
                }

                if is_hovered && item.is_dir && response.clicked() {
                    clicked_dir = Some(item.id);
                }
            }
        }

        if let Some(dir_id) = clicked_dir {
            self.treemap_history.push(self.treemap_dir_id);
            self.treemap_dir_id = dir_id;
            self.recompute_treemap_layout(treemap_rect);
        }
    }

    fn render_diff_tab(&mut self, ui: &mut Ui) {
        let loc = self.locale;
        let diff = match self.sbs_diff {
            Some(ref d) => d,
            None => {
                ui.label(match loc {
                    Locale::Pt => "Nenhum diff ativo. Selecione 2 arquivos na busca com Ctrl+clique e clique em 'Comparar'.",
                    Locale::Es => "Ningún diff activo. Seleccione 2 archivos en la búsqueda con Ctrl+clic y presione 'Comparar'.",
                    Locale::Fr => "Aucun diff actif. Sélectionnez 2 fichiers avec Ctrl+clic et cliquez sur 'Comparer'.",
                    Locale::It => "Nessun diff attivo. Seleziona 2 file nella ricerca con Ctrl+clic e clicca su 'Confronta'.",
                    Locale::Ru => "Нет активного diff. Выберите 2 файла в поиске через Ctrl+клик и нажмите 'Сравнить'.",
                    Locale::En => "No active diff. Select 2 files in the search with Ctrl+click and click 'Compare'.",
                });
                return;
            }
        };

        // Barra superior do Diff
        ui.horizontal(|ui| {
            ui.heading(
                egui::RichText::new(loc.diff_header())
                    .color(COLOR_TEXT_PRIMARY)
                    .size(15.0),
            );

            ui.add_space(16.0);

            ui.label(
                egui::RichText::new(loc.diff_additions(diff.additions))
                    .color(COLOR_SUCCESS)
                    .strong(),
            );
            ui.label(
                egui::RichText::new(loc.diff_deletions(diff.deletions))
                    .color(COLOR_DANGER)
                    .strong(),
            );

            if diff.is_identical {
                ui.label(egui::RichText::new(loc.diff_identical()).color(COLOR_SUCCESS));
            }

            // Alinha botão 'Voltar à Busca' à direita sem usar with_layout para não sequestrar a altura vertical
            let back_text = loc.back_to_search();
            let btn_w = 140.0;
            let space = (ui.available_width() - btn_w).max(0.0);
            ui.add_space(space);
            if ui.button(back_text).clicked() {
                self.active_tab = ActiveTab::Search;
            }
        });

        ui.separator();

        let available_size = ui.available_size();
        let height = (available_size.y - 8.0).max(100.0);
        let total_w = available_size.x;
        let location_pane_width = 54.0;
        let content_w = (total_w - location_pane_width - 16.0).max(200.0);
        let half_w = (content_w - 6.0) / 2.0;

        ui.horizontal(|ui| {
            // ==========================================
            // 1. LOCATION PANE (Minimapa / Visão Geral)
            // ==========================================
            ui.allocate_ui_with_layout(
                egui::vec2(location_pane_width, height),
                egui::Layout::top_down(egui::Align::Min),
                |ui| {
                    ui.label(
                        egui::RichText::new("LOCATION")
                            .size(9.0)
                            .color(COLOR_TEXT_MUTED)
                            .strong(),
                    );

                    let pane_h = (height - 24.0).max(50.0);
                    let (pane_rect, pane_response) = ui.allocate_exact_size(
                        egui::vec2(location_pane_width, pane_h),
                        Sense::click(),
                    );

                    let painter = ui.painter();

                    // Fundo do Location Pane
                    painter.rect_filled(
                        pane_rect,
                        CornerRadius::same(3),
                        Color32::from_rgb(18, 20, 24),
                    );
                    painter.rect_stroke(
                        pane_rect,
                        CornerRadius::same(3),
                        Stroke::new(1.0_f32, Color32::from_rgb(40, 44, 54)),
                        StrokeKind::Inside,
                    );

                    let col_w = 18.0;
                    let col_a_rect = Rect::from_min_size(
                        egui::pos2(pane_rect.min.x + 5.0, pane_rect.min.y + 4.0),
                        egui::vec2(col_w, pane_rect.height() - 8.0),
                    );
                    let col_b_rect = Rect::from_min_size(
                        egui::pos2(pane_rect.min.x + 31.0, pane_rect.min.y + 4.0),
                        egui::vec2(col_w, pane_rect.height() - 8.0),
                    );

                    painter.rect_filled(
                        col_a_rect,
                        CornerRadius::same(2),
                        Color32::from_rgb(26, 28, 36),
                    );
                    painter.rect_filled(
                        col_b_rect,
                        CornerRadius::same(2),
                        Color32::from_rgb(26, 28, 36),
                    );

                    let total_rows = diff.rows.len();
                    if total_rows > 0 {
                        let bar_h = col_a_rect.height();
                        let step_h = (bar_h / total_rows as f32).max(1.5);

                        for (idx, row) in diff.rows.iter().enumerate() {
                            let y_rel = (idx as f32 / total_rows as f32) * bar_h;
                            let r_top = col_a_rect.min.y + y_rel;

                            match row.kind {
                                ff_diff::ChangeKind::Modified => {
                                    let rect_a = Rect::from_min_size(
                                        egui::pos2(col_a_rect.min.x, r_top),
                                        egui::vec2(col_w, step_h),
                                    );
                                    let rect_b = Rect::from_min_size(
                                        egui::pos2(col_b_rect.min.x, r_top),
                                        egui::vec2(col_w, step_h),
                                    );
                                    painter.rect_filled(
                                        rect_a,
                                        CornerRadius::ZERO,
                                        Color32::from_rgb(220, 160, 20),
                                    );
                                    painter.rect_filled(
                                        rect_b,
                                        CornerRadius::ZERO,
                                        Color32::from_rgb(220, 160, 20),
                                    );
                                }
                                ff_diff::ChangeKind::Delete => {
                                    let rect_a = Rect::from_min_size(
                                        egui::pos2(col_a_rect.min.x, r_top),
                                        egui::vec2(col_w, step_h),
                                    );
                                    let rect_b = Rect::from_min_size(
                                        egui::pos2(col_b_rect.min.x, r_top),
                                        egui::vec2(col_w, step_h),
                                    );
                                    painter.rect_filled(
                                        rect_a,
                                        CornerRadius::ZERO,
                                        Color32::from_rgb(220, 60, 60),
                                    );
                                    painter.rect_filled(
                                        rect_b,
                                        CornerRadius::ZERO,
                                        Color32::from_rgb(50, 54, 64),
                                    );
                                }
                                ff_diff::ChangeKind::Insert => {
                                    let rect_a = Rect::from_min_size(
                                        egui::pos2(col_a_rect.min.x, r_top),
                                        egui::vec2(col_w, step_h),
                                    );
                                    let rect_b = Rect::from_min_size(
                                        egui::pos2(col_b_rect.min.x, r_top),
                                        egui::vec2(col_w, step_h),
                                    );
                                    painter.rect_filled(
                                        rect_a,
                                        CornerRadius::ZERO,
                                        Color32::from_rgb(50, 54, 64),
                                    );
                                    painter.rect_filled(
                                        rect_b,
                                        CornerRadius::ZERO,
                                        Color32::from_rgb(40, 180, 80),
                                    );
                                }
                                _ => {}
                            }
                        }

                        // Clique no Location Pane para saltar diretamente para aquela região do diff
                        if pane_response.clicked() {
                            if let Some(pos) = pane_response.interact_pointer_pos() {
                                let rel_y = (pos.y - col_a_rect.min.y).clamp(0.0, bar_h);
                                let target_row = ((rel_y / bar_h) * total_rows as f32) as usize;
                                self.diff_scroll_to_row = Some(target_row);
                            }
                        }
                    }
                },
            );

            // Linha divisória entre Location Pane e o conteúdo
            ui.separator();

            // ==========================================
            // 2. ARQUIVOS LADO A LADO COM LINHA DIVISÓRIA
            // ==========================================
            ui.allocate_ui_with_layout(
                egui::vec2(content_w, height),
                egui::Layout::top_down(egui::Align::Min),
                |ui| {
                    // Cabeçalhos dos dois arquivos
                    ui.horizontal(|ui| {
                        ui.allocate_ui(egui::vec2(half_w, 22.0), |ui| {
                            ui.horizontal(|ui| {
                                ui.label(egui::RichText::new("📄").size(12.0));
                                ui.label(
                                    egui::RichText::new(&diff.file_a)
                                        .color(COLOR_TEXT_PRIMARY)
                                        .strong()
                                        .monospace(),
                                )
                                .on_hover_text(&diff.file_a);
                            });
                        });

                        // Separador central nítido de cabeçalho
                        let (div_rect, _) =
                            ui.allocate_exact_size(egui::vec2(4.0, 22.0), Sense::hover());
                        ui.painter().vline(
                            div_rect.center().x,
                            div_rect.y_range(),
                            Stroke::new(2.0_f32, Color32::from_rgb(70, 76, 95)),
                        );

                        ui.allocate_ui(egui::vec2(half_w, 22.0), |ui| {
                            ui.horizontal(|ui| {
                                ui.label(egui::RichText::new("📄").size(12.0));
                                ui.label(
                                    egui::RichText::new(&diff.file_b)
                                        .color(COLOR_TEXT_PRIMARY)
                                        .strong()
                                        .monospace(),
                                )
                                .on_hover_text(&diff.file_b);
                            });
                        });
                    });

                    ui.separator();

                    // Linhas do diff sincronizadas com ScrollArea
                    let mut scroll = ScrollArea::vertical().auto_shrink([false, false]);
                    if let Some(target) = self.diff_scroll_to_row.take() {
                        scroll = scroll.vertical_scroll_offset(target as f32 * 20.0);
                    }

                    scroll.show_rows(ui, 20.0, diff.rows.len(), |ui, row_range| {
                        for idx in row_range {
                            let row = &diff.rows[idx];

                            ui.horizontal(|ui| {
                                // --- PAINEL ESQUERDO (ARQUIVO A) ---
                                ui.allocate_ui(egui::vec2(half_w, 20.0), |ui| {
                                    let bg = match row.kind {
                                        ff_diff::ChangeKind::Delete => {
                                            Color32::from_rgba_premultiplied(100, 20, 20, 70)
                                        }
                                        ff_diff::ChangeKind::Modified => {
                                            Color32::from_rgba_premultiplied(80, 50, 15, 70)
                                        }
                                        ff_diff::ChangeKind::Insert => {
                                            Color32::from_rgb(26, 28, 34) // Bloco ausente
                                        }
                                        _ => Color32::TRANSPARENT,
                                    };

                                    Frame::new().fill(bg).show(ui, |ui| {
                                        ui.set_width(half_w);
                                        ui.horizontal(|ui| {
                                            // Calha do número da linha A
                                            let gutter_num = if let Some(ref left) = row.left {
                                                format!("{:4}", left.lineno)
                                            } else {
                                                " ---".to_string()
                                            };

                                            ui.allocate_ui(egui::vec2(44.0, 20.0), |ui| {
                                                Frame::new()
                                                    .fill(Color32::from_rgb(22, 24, 30))
                                                    .show(ui, |ui| {
                                                        ui.set_width(44.0);
                                                        ui.label(
                                                            egui::RichText::new(gutter_num)
                                                                .color(COLOR_TEXT_MUTED)
                                                                .monospace(),
                                                        );
                                                    });
                                            });

                                            // Linha divisória da calha
                                            let (gl_rect, _) = ui.allocate_exact_size(
                                                egui::vec2(1.0, 20.0),
                                                Sense::hover(),
                                            );
                                            ui.painter().vline(
                                                gl_rect.center().x,
                                                gl_rect.y_range(),
                                                Stroke::new(1.0_f32, Color32::from_rgb(45, 48, 60)),
                                            );

                                            // Conteúdo textual da linha A
                                            if let Some(ref left) = row.left {
                                                for span in &left.spans {
                                                    let color = match span.kind {
                                                        ff_diff::ChangeKind::Delete => COLOR_DANGER,
                                                        _ => COLOR_TEXT_PRIMARY,
                                                    };
                                                    ui.label(
                                                        egui::RichText::new(&span.text)
                                                            .color(color)
                                                            .monospace(),
                                                    );
                                                }
                                            } else {
                                                ui.label(egui::RichText::new(" ").monospace());
                                            }
                                        });
                                    });
                                });

                                // --- LINHA DIVISÓRIA CENTRAL ENTRE A E B ---
                                let (div_rect, _) =
                                    ui.allocate_exact_size(egui::vec2(4.0, 20.0), Sense::hover());
                                ui.painter().vline(
                                    div_rect.center().x,
                                    div_rect.y_range(),
                                    Stroke::new(2.0_f32, Color32::from_rgb(65, 72, 90)),
                                );

                                // --- PAINEL DIREITO (ARQUIVO B) ---
                                ui.allocate_ui(egui::vec2(half_w, 20.0), |ui| {
                                    let bg = match row.kind {
                                        ff_diff::ChangeKind::Insert => {
                                            Color32::from_rgba_premultiplied(20, 100, 30, 70)
                                        }
                                        ff_diff::ChangeKind::Modified => {
                                            Color32::from_rgba_premultiplied(80, 50, 15, 70)
                                        }
                                        ff_diff::ChangeKind::Delete => {
                                            Color32::from_rgb(26, 28, 34) // Bloco ausente
                                        }
                                        _ => Color32::TRANSPARENT,
                                    };

                                    Frame::new().fill(bg).show(ui, |ui| {
                                        ui.set_width(half_w);
                                        ui.horizontal(|ui| {
                                            // Calha do número da linha B
                                            let gutter_num = if let Some(ref right) = row.right {
                                                format!("{:4}", right.lineno)
                                            } else {
                                                " ---".to_string()
                                            };

                                            ui.allocate_ui(egui::vec2(44.0, 20.0), |ui| {
                                                Frame::new()
                                                    .fill(Color32::from_rgb(22, 24, 30))
                                                    .show(ui, |ui| {
                                                        ui.set_width(44.0);
                                                        ui.label(
                                                            egui::RichText::new(gutter_num)
                                                                .color(COLOR_TEXT_MUTED)
                                                                .monospace(),
                                                        );
                                                    });
                                            });

                                            // Linha divisória da calha
                                            let (gl_rect, _) = ui.allocate_exact_size(
                                                egui::vec2(1.0, 20.0),
                                                Sense::hover(),
                                            );
                                            ui.painter().vline(
                                                gl_rect.center().x,
                                                gl_rect.y_range(),
                                                Stroke::new(1.0_f32, Color32::from_rgb(45, 48, 60)),
                                            );

                                            // Conteúdo textual da linha B
                                            if let Some(ref right) = row.right {
                                                for span in &right.spans {
                                                    let color = match span.kind {
                                                        ff_diff::ChangeKind::Insert => {
                                                            COLOR_SUCCESS
                                                        }
                                                        _ => COLOR_TEXT_PRIMARY,
                                                    };
                                                    ui.label(
                                                        egui::RichText::new(&span.text)
                                                            .color(color)
                                                            .monospace(),
                                                    );
                                                }
                                            } else {
                                                ui.label(egui::RichText::new(" ").monospace());
                                            }
                                        });
                                    });
                                });
                            });
                        }
                    });
                },
            );
        });
    }
}

/// Formata contagem de bytes em formato legível B, KB, MB, GB.
pub fn format_size_bytes(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{bytes} B")
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else if bytes < 1024 * 1024 * 1024 {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    } else {
        format!("{:.2} GB", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
    }
}

/// Escapa um valor para uso seguro em arquivo CSV segundo a RFC 4180.
#[must_use]
pub fn escape_csv_field(field: &str) -> String {
    if field.contains(',') || field.contains('"') || field.contains('\n') || field.contains('\r') {
        format!("\"{}\"", field.replace('"', "\"\""))
    } else {
        field.to_string()
    }
}

/// Formata timestamp Unix em string de data legível (YYYY-MM-DD HH:MM:SS).
#[must_use]
pub fn format_unix_timestamp(secs: i64) -> String {
    if secs <= 0 {
        return String::new();
    }
    let s = secs as u64;
    let sec = s % 60;
    let min = (s / 60) % 60;
    let hour = (s / 3600) % 24;
    let days = s / 86400;

    let mut d = days;
    let mut year = 1970;
    loop {
        let leap = (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0);
        let days_in_year = if leap { 366 } else { 365 };
        if d < days_in_year {
            break;
        }
        d -= days_in_year;
        year += 1;
    }

    let leap = (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0);
    let month_days = [
        31,
        if leap { 29 } else { 28 },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];
    let mut month = 1;
    for &md in &month_days {
        if d < md {
            break;
        }
        d -= md;
        month += 1;
    }
    let day = d + 1;

    format!("{year:04}-{month:02}-{day:02} {hour:02}:{min:02}:{sec:02}")
}

/// Gera o conteúdo de texto CSV a partir de uma lista de itens de resultado da busca.
#[must_use]
pub fn generate_csv_string(items: &[SearchResultItem]) -> String {
    let mut csv = String::with_capacity(items.len() * 128 + 64);
    csv.push_str("Nome,Caminho,Tipo,Tamanho_Bytes,Tamanho_Formatado,Modificado\n");

    for item in items {
        let tipo = if item.is_dir { "Diretório" } else { "Arquivo" };
        let tamanho_fmt = format_size_bytes(item.size);
        let mtime_str = format_unix_timestamp(item.mtime);

        csv.push_str(&format!(
            "{},{},{},{},{},{}\n",
            escape_csv_field(&item.name),
            escape_csv_field(&item.path),
            escape_csv_field(tipo),
            item.size,
            escape_csv_field(&tamanho_fmt),
            escape_csv_field(&mtime_str),
        ));
    }

    csv
}
