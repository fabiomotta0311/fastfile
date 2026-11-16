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

    // Diff lado a lado (ff-diff)
    pub sbs_diff: Option<SideBySideDiff>,
    pub diff_current_jump: usize,

    // Atalho global de teclado (Win32 RegisterHotKey / Issue #17)
    pub hotkey_listener: crate::hotkey::GlobalHotkeyListener,
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

            sbs_diff: None,
            diff_current_jump: 0,

            hotkey_listener: crate::hotkey::GlobalHotkeyListener::new(),
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

    /// Atualiza o cálculo do treemap para a pasta atual.
    pub fn refresh_treemap(&mut self, bounds: TreemapRect) {
        if let Some(ref idx) = self.local_index {
            let layout =
                ff_size::treemap::build_directory_treemap(idx, self.treemap_dir_id, bounds);
            self.treemap_layout = Some(layout);
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
                .on_hover_text("Compara dois arquivos selecionados lado a lado (estilo WinMerge)")
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
                }
            });
        });
    }

    fn render_status_bar(&self, ui: &mut Ui) {
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
        if self.preview_panel_open && self.preview_target_path.is_some() {
            // Layout dividido com pré-visualização à direita
            let total_width = ui.available_width();
            let list_width = (total_width * 0.58).max(300.0);

            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.set_width(list_width);
                    self.render_virtualized_table(ui);
                });

                ui.separator();

                ui.vertical(|ui| {
                    self.render_preview_panel(ui);
                });
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
            None => return,
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

        // Barra de busca dentro do arquivo com navegação
        ui.horizontal(|ui| {
            ui.label("Filtrar no arquivo:");
            let resp = ui.add_sized(
                [180.0, 22.0],
                egui::TextEdit::singleline(&mut self.preview_search_query).hint_text("Buscar..."),
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

        match self.preview_result {
            Some(PreviewResult::Text(ref text_preview)) => {
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
                ui.label(
                    egui::RichText::new(format!(
                        "Arquivo Binário — Tamanho: {} (Hex Dump)",
                        format_size_bytes(bin_preview.size)
                    ))
                    .color(COLOR_WARNING),
                );

                ui.add_space(8.0);

                ScrollArea::vertical().show(ui, |ui| {
                    for line in &bin_preview.hex_dump {
                        ui.label(
                            egui::RichText::new(line)
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
                    egui::RichText::new("Selecione um arquivo para pré-visualizar")
                        .color(COLOR_TEXT_MUTED),
                );
            }
        }
    }

    fn render_treemap_tab(&mut self, ui: &mut Ui) {
        let available_size = ui.available_size();
        let treemap_rect = TreemapRect::new(0.0, 0.0, available_size.x, available_size.y - 40.0);

        ui.horizontal(|ui| {
            if ui.button("⬆️ Subir Pasta").clicked() && !self.treemap_history.is_empty() {
                self.treemap_dir_id = self.treemap_history.pop().unwrap_or(0);
                self.refresh_treemap(treemap_rect);
            }

            if let Some(ref layout) = self.treemap_layout {
                ui.label(
                    egui::RichText::new(format!(
                        "Pasta atual: {} ({})",
                        layout.root_name,
                        format_size_bytes(layout.total_size)
                    ))
                    .strong(),
                );
            }

            if ui.button("🔄 Recalcular").clicked() {
                self.refresh_treemap(treemap_rect);
            }
        });

        ui.separator();

        if self.treemap_layout.is_none() {
            self.refresh_treemap(treemap_rect);
        }

        let (response, painter) = ui.allocate_painter(
            Vec2::new(available_size.x, available_size.y - 45.0),
            Sense::click(),
        );

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

                // Escreve o nome nos retângulos que tiverem espaço suficiente
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

        ui.horizontal(|ui| {
            ui.heading(
                egui::RichText::new("⚖️ Diff Lado a Lado (WinMerge)")
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

        // Cabeçalho dos dois arquivos
        let half_width = (ui.available_width() - 20.0) / 2.0;
        ui.horizontal(|ui| {
            ui.add_sized(
                [half_width, 20.0],
                egui::Label::new(
                    egui::RichText::new(format!("⬅️ {}", diff.file_a))
                        .color(COLOR_TEXT_PRIMARY)
                        .strong(),
                )
                .truncate(),
            );

            ui.add_sized(
                [half_width, 20.0],
                egui::Label::new(
                    egui::RichText::new(format!("➡️ {}", diff.file_b))
                        .color(COLOR_TEXT_PRIMARY)
                        .strong(),
                )
                .truncate(),
            );
        });

        ui.separator();

        // Linhas de diff lado a lado alinhadas
        ScrollArea::vertical()
            .auto_shrink([false, false])
            .show_rows(ui, 20.0, diff.rows.len(), |ui, row_range| {
                for idx in row_range {
                    let row = &diff.rows[idx];

                    ui.horizontal(|ui| {
                        // Painel Esquerdo (Arquivo A)
                        ui.allocate_ui(egui::vec2(half_width, 20.0), |ui| {
                            if let Some(ref left) = row.left {
                                let bg = match row.kind {
                                    ff_diff::ChangeKind::Delete => {
                                        Color32::from_rgba_premultiplied(100, 20, 20, 70)
                                    }
                                    ff_diff::ChangeKind::Modified => {
                                        Color32::from_rgba_premultiplied(80, 40, 20, 70)
                                    }
                                    _ => Color32::TRANSPARENT,
                                };

                                Frame::new().fill(bg).show(ui, |ui| {
                                    ui.horizontal(|ui| {
                                        ui.label(
                                            egui::RichText::new(format!("{:4}", left.lineno))
                                                .color(COLOR_TEXT_MUTED)
                                                .monospace(),
                                        );

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
                                    });
                                });
                            }
                        });

                        ui.separator();

                        // Painel Direito (Arquivo B)
                        ui.allocate_ui(egui::vec2(half_width, 20.0), |ui| {
                            if let Some(ref right) = row.right {
                                let bg = match row.kind {
                                    ff_diff::ChangeKind::Insert => {
                                        Color32::from_rgba_premultiplied(20, 100, 20, 70)
                                    }
                                    ff_diff::ChangeKind::Modified => {
                                        Color32::from_rgba_premultiplied(20, 70, 40, 70)
                                    }
                                    _ => Color32::TRANSPARENT,
                                };

                                Frame::new().fill(bg).show(ui, |ui| {
                                    ui.horizontal(|ui| {
                                        ui.label(
                                            egui::RichText::new(format!("{:4}", right.lineno))
                                                .color(COLOR_TEXT_MUTED)
                                                .monospace(),
                                        );

                                        for span in &right.spans {
                                            let color = match span.kind {
                                                ff_diff::ChangeKind::Insert => COLOR_SUCCESS,
                                                _ => COLOR_TEXT_PRIMARY,
                                            };
                                            ui.label(
                                                egui::RichText::new(&span.text)
                                                    .color(color)
                                                    .monospace(),
                                            );
                                        }
                                    });
                                });
                            }
                        });
                    });
                }
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
