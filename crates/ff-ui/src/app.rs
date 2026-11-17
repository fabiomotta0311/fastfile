//! Aplicação principal FastFile egui (§0.1, §4 e Issues #13, #14, #15, #19).

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::time::Instant;

use egui::{
    Color32, CornerRadius, FontId, Frame, Margin, Rect, ScrollArea, Sense, Stroke, StrokeKind, Ui,
    Vec2,
};

use ff_diff::SideBySideDiff;
use ff_index::IndexSource;
use ff_indexer::{default_endpoint_name, IpcClient, IpcRequest};
use ff_preview::{PreviewEngine, PreviewResult};
use ff_search::query::Query;
use ff_search::SearchEngine;
use ff_size::treemap::{TreemapItem, TreemapLayout, TreemapRect};

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
    pub treemap_custom_index: Option<ff_index::FileIndex>,
    pub treemap_scanning_path: Option<String>,
    pub treemap_receiver: Option<std::sync::mpsc::Receiver<(String, ff_index::FileIndex)>>,

    // Diff lado a lado (ff-diff)
    pub sbs_diff: Option<SideBySideDiff>,
    pub diff_current_jump: usize,
    pub diff_scroll_to_row: Option<usize>,

    // Atalho global de teclado (Win32 RegisterHotKey / Issue #17)
    pub hotkey_listener: crate::hotkey::GlobalHotkeyListener,

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

        // Fallback: se IPC não estiver rodando, indexa diretório atual ou raiz
        let mut local_index = None;
        let mut total_indexed = 0;

        if !is_ipc_connected {
            let mut idx = ff_index::FileIndex::new();
            let root = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
            let mut source = ff_index::WalkSource::new(&root).with_respect_ignore(true);
            if source.populate(&mut idx).is_ok() {
                total_indexed = idx.entries.len();
                local_index = Some(idx);
            }
        }

        let mut app = Self {
            query: String::new(),
            active_filter: QuickFilter::All,
            active_tab: ActiveTab::Search,

            ipc_client,
            local_index,
            is_ipc_connected,
            total_indexed_files: total_indexed,
            search_latency_ms: 0.0,

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
            treemap_custom_index: None,
            treemap_scanning_path: None,
            treemap_receiver: None,

            sbs_diff: None,
            diff_current_jump: 0,
            diff_scroll_to_row: None,

            hotkey_listener: crate::hotkey::GlobalHotkeyListener::new(),
            status_notification: None,
        };

        // Executa busca inicial
        app.execute_search();
        app
    }

    /// Executa a consulta de busca sobre o índice residente ou local.
    pub fn execute_search(&mut self) {
        let start = Instant::now();

        let pattern = self.query.trim();
        let mut items = Vec::new();

        if let Some(ref idx) = self.local_index {
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

    /// Atualiza o cálculo do treemap para a pasta atual.
    pub fn refresh_treemap(&mut self, bounds: TreemapRect) {
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

        // 1. Tenta localizar a pasta dentro do índice residente atual
        let mut found_id = None;
        if let Some(ref idx) = self.local_index {
            let norm_lc = p_norm.trim_end_matches(['\\', '/']).to_lowercase();
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
            self.treemap_custom_index = None;
            self.treemap_dir_id = id;
            self.treemap_history.clear();
            self.treemap_scanning_path = None;
            self.treemap_receiver = None;
            self.refresh_treemap(bounds);
        } else {
            // Inicia varredura em segundo plano (background thread) para NÃO travar a UI!
            let (tx, rx) = std::sync::mpsc::channel();
            self.treemap_receiver = Some(rx);
            self.treemap_scanning_path = Some(p_norm.clone());
            self.treemap_layout = None; // Limpa o layout anterior para exibir a tela de carregamento

            let target_path = p_norm;
            std::thread::spawn(move || {
                let mut new_idx = ff_index::FileIndex::new();

                #[cfg(windows)]
                {
                    let clean = target_path.trim_end_matches(['\\', '/']);
                    if clean.len() <= 2 && clean.ends_with(':') {
                        if let Some(letter) = clean.chars().next() {
                            let mut mft = ff_index::MftSource::new(letter);
                            if mft.populate(&mut new_idx).is_ok() && !new_idx.entries.is_empty() {
                                let _ = tx.send((target_path, new_idx));
                                return;
                            }
                        }
                    }
                }

                let mut walk = ff_index::WalkSource::new(&target_path).with_respect_ignore(false);
                let _ = walk.populate(&mut new_idx);
                let _ = tx.send((target_path, new_idx));
            });
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
                Ok((path, new_idx)) => {
                    self.treemap_receiver = None;
                    self.treemap_scanning_path = None;
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
                        self.refresh_treemap(treemap_rect);
                        self.status_notification = Some((
                            format!("✓ Treemap de {} calculado com sucesso!", path),
                            std::time::Instant::now(),
                        ));
                    } else {
                        self.status_notification = Some((
                            format!("⚠️ Nenhum arquivo encontrado em {}", path),
                            std::time::Instant::now(),
                        ));
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
                    .hint_text(
                        "Digite para buscar instantaneamente por nome ou filtros... (Ctrl+F)",
                    )
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
                        egui::RichText::new(format!("Comparar ({})", self.selected_indices.len()))
                            .color(if compare_enabled {
                                Color32::WHITE
                            } else {
                                COLOR_TEXT_MUTED
                            }),
                    ),
                )
                .on_hover_text("Compara dois arquivos selecionados lado a lado")
                .clicked()
            {
                self.open_diff_for_selected();
            }
        });

        ui.add_space(8.0);

        // Linha secundária: Filtros rápidos e Abas
        ui.horizontal(|ui| {
            // Seletor de abas
            ui.selectable_value(&mut self.active_tab, ActiveTab::Search, "📂 Resultados");
            ui.selectable_value(
                &mut self.active_tab,
                ActiveTab::Treemap,
                "📊 Treemap de Uso",
            );
            if self.sbs_diff.is_some() {
                ui.selectable_value(&mut self.active_tab, ActiveTab::Diff, "⚖️ Diff Lado a Lado");
            }

            ui.separator();

            // Filtros rápidos
            let filter_clicked = ui
                .selectable_value(&mut self.active_filter, QuickFilter::All, "Todos")
                .clicked()
                || ui
                    .selectable_value(&mut self.active_filter, QuickFilter::Code, "Código")
                    .clicked()
                || ui
                    .selectable_value(&mut self.active_filter, QuickFilter::Docs, "Documentos")
                    .clicked()
                || ui
                    .selectable_value(&mut self.active_filter, QuickFilter::Media, "Mídia")
                    .clicked()
                || ui
                    .selectable_value(
                        &mut self.active_filter,
                        QuickFilter::Archives,
                        "Compactados",
                    )
                    .clicked()
                || ui
                    .selectable_value(&mut self.active_filter, QuickFilter::Large, "> 100 MB")
                    .clicked();

            if filter_clicked {
                self.execute_search();
            }

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .button(if self.preview_panel_open {
                        "Ocultar Prévia"
                    } else {
                        "Mostrar Prévia"
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
                    let export_btn = egui::Button::new("📥 Baixar CSV");
                    let has_results = !self.results.is_empty();
                    if ui
                        .add_enabled(has_results, export_btn)
                        .on_hover_text(if has_results {
                            format!(
                                "Exporta os {} arquivos filtrados para uma planilha CSV",
                                self.results.len()
                            )
                        } else {
                            "Nenhum arquivo nos resultados para exportar".to_string()
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
        ui.horizontal(|ui| {
            // Status do daemon indexador residente
            if self.is_ipc_connected {
                ui.label(
                    egui::RichText::new("● IPC Indexador Ativo")
                        .color(COLOR_SUCCESS)
                        .size(11.0),
                );
            } else {
                ui.label(
                    egui::RichText::new("● Varredura Local")
                        .color(COLOR_WARNING)
                        .size(11.0),
                );
            }

            ui.separator();

            // Indicador de varredura assíncrona do Treemap
            if let Some(ref scan_path) = self.treemap_scanning_path {
                ui.label(
                    egui::RichText::new(format!("⏳ Analisando {scan_path} em segundo plano..."))
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
                egui::RichText::new(format!(
                    "{} resultados de {} arquivos indexados",
                    self.results.len(),
                    self.total_indexed_files
                ))
                .color(COLOR_TEXT_MUTED)
                .size(11.0),
            );

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(
                    egui::RichText::new(format!("{:.1} ms", self.search_latency_ms))
                        .color(COLOR_ACCENT)
                        .size(11.0),
                );
            });
        });
    }

    fn render_search_tab(&mut self, ui: &mut Ui) {
        if self.preview_panel_open {
            // Layout dividido com pré-visualização à direita
            let available_size = ui.available_size();
            let height = available_size.y;
            let list_width = (available_size.x * 0.56).max(300.0);
            let preview_width = (available_size.x - list_width - 16.0).max(200.0);

            ui.horizontal(|ui| {
                ui.allocate_ui_with_layout(
                    egui::vec2(list_width, height),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| {
                        self.render_virtualized_table(ui);
                    },
                );

                ui.separator();

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
        // Cabeçalho clicável das colunas para ordenação
        ui.horizontal(|ui| {
            if ui
                .selectable_label(self.sort_column == SortColumn::Name, "Nome")
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

            ui.add_space(200.0);

            if ui
                .selectable_label(self.sort_column == SortColumn::Size, "Tamanho")
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

            ui.add_space(60.0);

            if ui
                .selectable_label(self.sort_column == SortColumn::Path, "Caminho")
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

                                // Nome do arquivo
                                ui.add_sized(
                                    [220.0, 18.0],
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

                                // Tamanho formatado
                                ui.add_sized(
                                    [80.0, 18.0],
                                    egui::Label::new(
                                        egui::RichText::new(format_size_bytes(item.size))
                                            .color(COLOR_TEXT_MUTED)
                                            .monospace(),
                                    ),
                                );

                                // Caminho completo
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
        let path = match self.preview_target_path {
            Some(ref p) => p.clone(),
            None => {
                ui.vertical_centered(|ui| {
                    ui.add_space(60.0);
                    ui.label(egui::RichText::new("👁️").size(36.0).color(COLOR_TEXT_MUTED));
                    ui.add_space(8.0);
                    ui.label(
                        egui::RichText::new("Selecione um arquivo da lista para pré-visualizar")
                            .color(COLOR_TEXT_MUTED)
                            .size(13.0),
                    );
                });
                return;
            }
        };

        ui.horizontal(|ui| {
            ui.heading(
                egui::RichText::new("👁️ Pré-visualização")
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
                ui.label("Filtrar no arquivo:");
                let resp = ui.add_sized(
                    [180.0, 22.0],
                    egui::TextEdit::singleline(&mut self.preview_search_query)
                        .hint_text("Buscar..."),
                );

                if resp.changed() {
                    let p = path.clone();
                    self.load_preview(&p);
                }

                if let Some(PreviewResult::Text(ref text_res)) = self.preview_result {
                    if !text_res.occurrences.is_empty() {
                        ui.label(
                            egui::RichText::new(format!(
                                "{}/{} ocorrências",
                                self.preview_occurrence_idx + 1,
                                text_res.occurrences.len()
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
                ui.label(
                    egui::RichText::new(format!(
                        "Tipo: {} • Total: {} linhas",
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

                let type_desc = match ext.as_str() {
                    "png" | "jpg" | "jpeg" | "gif" | "bmp" | "ico" | "webp" => {
                        format!(
                            "🖼️ Imagem ({}) — Tamanho: {}",
                            ext.to_uppercase(),
                            format_size_bytes(bin_preview.size)
                        )
                    }
                    "exe" | "dll" | "so" | "dylib" => {
                        format!(
                            "⚙️ Executável/Biblioteca ({}) — Tamanho: {}",
                            ext.to_uppercase(),
                            format_size_bytes(bin_preview.size)
                        )
                    }
                    "zip" | "tar" | "gz" | "7z" | "rar" => {
                        format!(
                            "📦 Pacote Compactado ({}) — Tamanho: {}",
                            ext.to_uppercase(),
                            format_size_bytes(bin_preview.size)
                        )
                    }
                    _ => {
                        format!(
                            "📦 Arquivo Binário — Tamanho: {}",
                            format_size_bytes(bin_preview.size)
                        )
                    }
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
                ui.label(egui::RichText::new("Arquivo vazio").color(COLOR_TEXT_MUTED));
            }
            None => {
                ui.label(
                    egui::RichText::new("Carregando pré-visualização...").color(COLOR_TEXT_MUTED),
                );
            }
        }
    }

    fn render_treemap_tab(&mut self, ui: &mut Ui) {
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
                .button("📁 Escolher Pasta...")
                .on_hover_text(
                    "Abre diálogo nativo do sistema para selecionar qualquer pasta ou disco",
                )
                .clicked()
            {
                if let Some(folder) = rfd::FileDialog::new().pick_folder() {
                    self.navigate_treemap_to_path(&folder, treemap_rect);
                }
            }

            ui.add_space(4.0);

            // Reserva espaço exato para os 3 botões da direita (Analisar, Subir, Recalcular) mais margem segura
            let trailing_buttons_w = 340.0;
            let path_input_w = (ui.available_width() - trailing_buttons_w).max(120.0);

            let path_resp = ui.add_sized(
                [path_input_w, 22.0],
                egui::TextEdit::singleline(&mut self.treemap_path_input)
                    .hint_text("Digite ou cole o caminho da pasta... Ex: C:\\ ou D:\\Projetos"),
            );

            if path_resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                let target = PathBuf::from(self.treemap_path_input.trim());
                self.navigate_treemap_to_path(&target, treemap_rect);
            }

            if ui
                .button("🔍 Analisar")
                .on_hover_text("Calcula e renderiza o Treemap da pasta informada")
                .clicked()
            {
                let target = PathBuf::from(self.treemap_path_input.trim());
                self.navigate_treemap_to_path(&target, treemap_rect);
            }

            if ui
                .button("⬆️ Subir Pasta")
                .on_hover_text("Retorna para a pasta ancestral anterior")
                .clicked()
            {
                if !self.treemap_history.is_empty() {
                    self.treemap_dir_id = self.treemap_history.pop().unwrap_or(0);
                    self.refresh_treemap(treemap_rect);
                } else {
                    let cur = PathBuf::from(&self.treemap_path_input);
                    if let Some(parent) = cur.parent() {
                        self.navigate_treemap_to_path(parent, treemap_rect);
                    }
                }
            }

            if ui
                .button("🔄 Recalcular")
                .on_hover_text("Recalcula o treemap para a pasta atual")
                .clicked()
            {
                self.refresh_treemap(treemap_rect);
            }
        });

        // 2. Atalhos Rápidos para Drives e Pastas do Sistema
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new("Atalhos rápidos:")
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
                if ui.small_button("👤 Usuário").clicked() {
                    self.navigate_treemap_to_path(&user_path, treemap_rect);
                }

                let docs = user_path.join("Documents");
                if docs.exists() && ui.small_button("📁 Documentos").clicked() {
                    self.navigate_treemap_to_path(&docs, treemap_rect);
                }

                let dl = user_path.join("Downloads");
                if dl.exists() && ui.small_button("⬇️ Downloads").clicked() {
                    self.navigate_treemap_to_path(&dl, treemap_rect);
                }

                let dt = user_path.join("Desktop");
                if dt.exists() && ui.small_button("💻 Desktop").clicked() {
                    self.navigate_treemap_to_path(&dt, treemap_rect);
                }
            }

            if let Some(ref layout) = self.treemap_layout {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.add_space(10.0);
                    ui.label(
                        egui::RichText::new(format!(
                            "Total: {} ({} itens)",
                            format_size_bytes(layout.total_size),
                            layout.items.len()
                        ))
                        .color(COLOR_ACCENT)
                        .strong(),
                    );
                });
            }
        });

        // Exibe tela amigável de carregamento caso a varredura assíncrona esteja ativa
        if let Some(ref scan_path) = self.treemap_scanning_path {
            ui.add_space(50.0);
            ui.centered_and_justified(|ui| {
                ui.vertical_centered(|ui| {
                    ui.add(egui::Spinner::new().size(44.0).color(COLOR_ACCENT));
                    ui.add_space(16.0);
                    ui.heading(
                        egui::RichText::new("Analisando Uso de Disco em Segundo Plano...")
                            .color(COLOR_ACCENT)
                            .size(17.0),
                    );
                    ui.add_space(8.0);
                    ui.label(
                        egui::RichText::new(format!(
                            "Varrendo arquivos e pastas em \"{}\"... A aplicação continua 100% responsiva.",
                            scan_path
                        ))
                        .color(COLOR_TEXT_PRIMARY)
                        .size(13.0),
                    );
                    ui.add_space(4.0);
                    ui.label(
                        egui::RichText::new(
                            "Você pode consultar a aba 'Resultados' ou 'Diff' normalmente enquanto o cálculo é concluído.",
                        )
                        .color(COLOR_TEXT_MUTED)
                        .size(11.5),
                    );
                });
            });
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

        if self.treemap_layout.is_none() {
            self.refresh_treemap(treemap_rect);
        }

        let painter_height = (available_size.y - 85.0).max(100.0);
        let (response, painter) =
            ui.allocate_painter(Vec2::new(available_size.x, painter_height), Sense::click());

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

                if is_hovered && item.is_dir && response.clicked() {
                    clicked_dir = Some(item.id);
                }
            }
        }

        if let Some(dir_id) = clicked_dir {
            self.treemap_history.push(self.treemap_dir_id);
            self.treemap_dir_id = dir_id;
            self.refresh_treemap(treemap_rect);
        }
    }

    fn render_diff_tab(&mut self, ui: &mut Ui) {
        let diff = match self.sbs_diff {
            Some(ref d) => d,
            None => {
                ui.label("Nenhum diff ativo. Selecione 2 arquivos na busca com Ctrl+clique e clique em 'Comparar'.");
                return;
            }
        };

        // Barra superior do Diff
        ui.horizontal(|ui| {
            ui.heading(
                egui::RichText::new("⚖️ Diff Lado a Lado")
                    .color(COLOR_TEXT_PRIMARY)
                    .size(15.0),
            );

            ui.add_space(16.0);

            ui.label(
                egui::RichText::new(format!("+{} adições", diff.additions))
                    .color(COLOR_SUCCESS)
                    .strong(),
            );
            ui.label(
                egui::RichText::new(format!("-{} remoções", diff.deletions))
                    .color(COLOR_DANGER)
                    .strong(),
            );

            if diff.is_identical {
                ui.label(egui::RichText::new("Os arquivos são idênticos.").color(COLOR_SUCCESS));
            }

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("Voltar à Busca").clicked() {
                    self.active_tab = ActiveTab::Search;
                }
            });
        });

        ui.separator();

        let available_size = ui.available_size();
        let height = (available_size.y - 10.0).max(100.0);
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
