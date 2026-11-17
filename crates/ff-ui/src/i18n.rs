//! Módulo de Internacionalização (i18n) e Localização (Locale) para o FastFile UI.
//!
//! Suporta detecção automática do idioma do Sistema Operacional (Windows e Linux)
//! e alternância manual instantânea na interface (Português e Inglês).

use serde::{Deserialize, Serialize};

/// Idiomas suportados pela interface gráfica.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum Locale {
    /// Português (Brasil / Portugal)
    #[default]
    Pt,
    /// Inglês (Universal / EUA)
    En,
}

impl Locale {
    /// Detecta o idioma padrão da interface a partir do sistema operacional.
    ///
    /// No Windows: Consulta `GetUserDefaultUILanguage()` via Win32. Se for português (0x16), retorna `Locale::Pt`, senão `Locale::En`.
    /// No Linux / Unix: Analisa as variáveis de ambiente `LC_ALL`, `LC_MESSAGES` ou `LANG`.
    pub fn detect_from_system() -> Self {
        #[cfg(windows)]
        {
            extern "system" {
                fn GetUserDefaultUILanguage() -> u16;
            }
            // Chama a API nativa da kernel32
            let lang_id = unsafe { GetUserDefaultUILanguage() };
            // Os 10 bits de menor ordem indicam a língua primária (0x16 = LANG_PORTUGUESE)
            let primary_lang = lang_id & 0x03FF;
            if primary_lang == 0x16 {
                Locale::Pt
            } else {
                Locale::En
            }
        }

        #[cfg(not(windows))]
        {
            for var in &["LC_ALL", "LC_MESSAGES", "LANG"] {
                if let Ok(val) = std::env::var(var) {
                    let lower = val.to_lowercase();
                    if lower.starts_with("pt") {
                        return Locale::Pt;
                    }
                }
            }
            Locale::En
        }
    }

    /// Rótulo curto para o seletor na UI (ex: "PT" ou "EN").
    pub fn code(&self) -> &'static str {
        match self {
            Locale::Pt => "PT",
            Locale::En => "EN",
        }
    }

    /// Rótulo legível do idioma.
    pub fn display_name(&self) -> &'static str {
        match self {
            Locale::Pt => "Português",
            Locale::En => "English",
        }
    }

    /// Alterna entre os idiomas disponíveis.
    pub fn toggle(&self) -> Self {
        match self {
            Locale::Pt => Locale::En,
            Locale::En => Locale::Pt,
        }
    }

    // --- Títulos e Cabeçalhos ---

    pub fn window_title(&self) -> &'static str {
        match self {
            Locale::Pt => "FastFile — Busca, Prévia, Diff e Uso de Disco",
            Locale::En => "FastFile — Search, Preview, Diff & Disk Treemap",
        }
    }

    // --- Abas Principais ---

    pub fn tab_results(&self) -> &'static str {
        match self {
            Locale::Pt => "📂 Resultados",
            Locale::En => "📂 Results",
        }
    }

    pub fn tab_treemap(&self) -> &'static str {
        match self {
            Locale::Pt => "📊 Treemap de Uso",
            Locale::En => "📊 Disk Treemap",
        }
    }

    pub fn tab_diff(&self) -> &'static str {
        match self {
            Locale::Pt => "⚖️ Diff Lado a Lado",
            Locale::En => "⚖️ Side-by-Side Diff",
        }
    }

    // --- Filtros de Categoria ---

    pub fn filter_all(&self) -> &'static str {
        match self {
            Locale::Pt => "Todos",
            Locale::En => "All",
        }
    }

    pub fn filter_code(&self) -> &'static str {
        match self {
            Locale::Pt => "Código",
            Locale::En => "Code",
        }
    }

    pub fn filter_docs(&self) -> &'static str {
        match self {
            Locale::Pt => "Documentos",
            Locale::En => "Documents",
        }
    }

    pub fn filter_media(&self) -> &'static str {
        match self {
            Locale::Pt => "Mídia",
            Locale::En => "Media",
        }
    }

    pub fn filter_archives(&self) -> &'static str {
        match self {
            Locale::Pt => "Compactados",
            Locale::En => "Archives",
        }
    }

    pub fn filter_large(&self) -> &'static str {
        match self {
            Locale::Pt => "> 100 MB",
            Locale::En => "> 100 MB",
        }
    }

    // --- Botões e Ações Principais ---

    pub fn compare_button(&self, count: usize) -> String {
        match self {
            Locale::Pt => format!("Comparar ({count})"),
            Locale::En => format!("Compare ({count})"),
        }
    }

    pub fn show_preview(&self) -> &'static str {
        match self {
            Locale::Pt => "Mostrar Prévia",
            Locale::En => "Show Preview",
        }
    }

    pub fn hide_preview(&self) -> &'static str {
        match self {
            Locale::Pt => "Ocultar Prévia",
            Locale::En => "Hide Preview",
        }
    }

    pub fn download_csv(&self) -> &'static str {
        match self {
            Locale::Pt => "Baixar CSV",
            Locale::En => "Download CSV",
        }
    }

    pub fn back_to_search(&self) -> &'static str {
        match self {
            Locale::Pt => "Voltar à Busca",
            Locale::En => "Back to Search",
        }
    }

    pub fn search_placeholder(&self) -> &'static str {
        match self {
            Locale::Pt => "Buscar por nome, curingas (*.rs) ou regex (/pattern/)...",
            Locale::En => "Search by name, wildcards (*.rs) or regex (/pattern/)...",
        }
    }

    // --- Colunas da Tabela de Resultados ---

    pub fn col_name(&self) -> &'static str {
        match self {
            Locale::Pt => "Nome",
            Locale::En => "Name",
        }
    }

    pub fn col_size(&self) -> &'static str {
        match self {
            Locale::Pt => "Tamanho",
            Locale::En => "Size",
        }
    }

    pub fn col_path(&self) -> &'static str {
        match self {
            Locale::Pt => "Caminho",
            Locale::En => "Path",
        }
    }

    pub fn col_date(&self) -> &'static str {
        match self {
            Locale::Pt => "Modificado",
            Locale::En => "Modified",
        }
    }

    pub fn no_files_found(&self) -> &'static str {
        match self {
            Locale::Pt => "Nenhum arquivo encontrado",
            Locale::En => "No files found",
        }
    }

    // --- Painel de Pré-visualização ---

    pub fn preview_header(&self) -> &'static str {
        match self {
            Locale::Pt => "👁 Pré-visualização",
            Locale::En => "👁 Preview",
        }
    }

    pub fn preview_filter_hint(&self) -> &'static str {
        match self {
            Locale::Pt => "Filtrar no arquivo...",
            Locale::En => "Filter in file...",
        }
    }

    pub fn preview_matches(&self, count: usize) -> String {
        match self {
            Locale::Pt => format!("{count} ocorrência(s)"),
            Locale::En => format!("{count} match(es)"),
        }
    }

    pub fn preview_binary_title(&self, size_str: &str) -> String {
        match self {
            Locale::Pt => format!("Arquivo Binário — Tamanho: {size_str} (Hex Dump)"),
            Locale::En => format!("Binary File — Size: {size_str} (Hex Dump)"),
        }
    }

    pub fn preview_pdf_title(&self, size_str: &str) -> String {
        match self {
            Locale::Pt => format!("Texto extraído do documento PDF (Tamanho: {size_str})"),
            Locale::En => format!("Extracted text from PDF document (Size: {size_str})"),
        }
    }

    pub fn preview_docx_title(&self, size_str: &str) -> String {
        match self {
            Locale::Pt => format!("Texto extraído do documento DOCX (Tamanho: {size_str})"),
            Locale::En => format!("Extracted text from DOCX document (Size: {size_str})"),
        }
    }

    pub fn preview_loading(&self) -> &'static str {
        match self {
            Locale::Pt => "Carregando pré-visualização...",
            Locale::En => "Loading preview...",
        }
    }

    pub fn preview_failed(&self, err: &str) -> String {
        match self {
            Locale::Pt => format!("Falha ao carregar pré-visualização: {err}"),
            Locale::En => format!("Failed to load preview: {err}"),
        }
    }

    pub fn preview_truncated(&self) -> &'static str {
        match self {
            Locale::Pt => "[Conteúdo truncado para exibição rápida. Arquivo completo no disco]",
            Locale::En => "[Content truncated for fast display. Full file preserved on disk]",
        }
    }

    // --- Treemap de Disco ---

    pub fn treemap_choose_folder(&self) -> &'static str {
        match self {
            Locale::Pt => "📁 Escolher Pasta...",
            Locale::En => "📁 Choose Folder...",
        }
    }

    pub fn treemap_analyze(&self) -> &'static str {
        match self {
            Locale::Pt => "🔍 Analisar",
            Locale::En => "🔍 Analyze",
        }
    }

    pub fn treemap_analyze_tooltip(&self) -> &'static str {
        match self {
            Locale::Pt => "Calcula e renderiza o Treemap da pasta informada",
            Locale::En => "Calculates and renders Treemap for the specified directory",
        }
    }

    pub fn treemap_up_folder(&self) -> &'static str {
        match self {
            Locale::Pt => "⬆ Subir Pasta",
            Locale::En => "⬆ Up Folder",
        }
    }

    pub fn treemap_up_folder_tooltip(&self) -> &'static str {
        match self {
            Locale::Pt => "Volta um nível acima no diretório",
            Locale::En => "Go up one directory level",
        }
    }

    pub fn treemap_refresh(&self) -> &'static str {
        match self {
            Locale::Pt => "🔄 Recalcular",
            Locale::En => "🔄 Refresh",
        }
    }

    pub fn treemap_refresh_tooltip(&self) -> &'static str {
        match self {
            Locale::Pt => "Recalcula o treemap para a pasta atual",
            Locale::En => "Recalculates the treemap for the current folder",
        }
    }

    pub fn treemap_quick_links(&self) -> &'static str {
        match self {
            Locale::Pt => "Atalhos rápidos:",
            Locale::En => "Quick links:",
        }
    }

    pub fn treemap_shortcut_user(&self) -> &'static str {
        match self {
            Locale::Pt => "👤 Usuário",
            Locale::En => "👤 User",
        }
    }

    pub fn treemap_shortcut_docs(&self) -> &'static str {
        match self {
            Locale::Pt => "📁 Documentos",
            Locale::En => "📁 Documents",
        }
    }

    pub fn treemap_shortcut_downloads(&self) -> &'static str {
        match self {
            Locale::Pt => "⬇ Downloads",
            Locale::En => "⬇ Downloads",
        }
    }

    pub fn treemap_shortcut_desktop(&self) -> &'static str {
        match self {
            Locale::Pt => "🖥 Desktop",
            Locale::En => "🖥 Desktop",
        }
    }

    pub fn treemap_total(&self, size_str: &str) -> String {
        match self {
            Locale::Pt => format!("Total: {size_str}"),
            Locale::En => format!("Total: {size_str}"),
        }
    }

    pub fn treemap_scanning_title(&self) -> &'static str {
        match self {
            Locale::Pt => "Varrendo pasta em segundo plano...",
            Locale::En => "Scanning directory in background...",
        }
    }

    pub fn treemap_scanning_path(&self, path: &str) -> String {
        match self {
            Locale::Pt => format!("Calculando tamanhos e arquivos de: {path}"),
            Locale::En => format!("Calculating sizes and files for: {path}"),
        }
    }

    pub fn treemap_scanning_hint(&self) -> &'static str {
        match self {
            Locale::Pt => "Você pode consultar a aba 'Resultados' ou 'Diff' normalmente enquanto o cálculo é concluído.",
            Locale::En => "You can continue using the 'Results' or 'Diff' tabs normally while calculation finishes.",
        }
    }

    pub fn treemap_empty(&self) -> &'static str {
        match self {
            Locale::Pt => "Nenhum arquivo ou pasta encontrado para exibição no Treemap.",
            Locale::En => "No files or folders found for Treemap display.",
        }
    }

    // --- Diff Lado a Lado ---

    pub fn diff_header(&self) -> &'static str {
        match self {
            Locale::Pt => "⚖️ Diff Lado a Lado",
            Locale::En => "⚖️ Side-by-Side Diff",
        }
    }

    pub fn diff_additions(&self, n: usize) -> String {
        match self {
            Locale::Pt => format!("+{n} adições"),
            Locale::En => format!("+{n} additions"),
        }
    }

    pub fn diff_deletions(&self, n: usize) -> String {
        match self {
            Locale::Pt => format!("-{n} remoções"),
            Locale::En => format!("-{n} deletions"),
        }
    }

    pub fn diff_identical(&self) -> &'static str {
        match self {
            Locale::Pt => "Arquivos são idênticos em conteúdo",
            Locale::En => "Files are identical in content",
        }
    }

    pub fn diff_binary(&self) -> &'static str {
        match self {
            Locale::Pt => "Arquivos binários diferentes (não comparáveis em texto puro)",
            Locale::En => "Binary files differ (cannot be compared as plain text)",
        }
    }

    pub fn diff_error(&self) -> &'static str {
        match self {
            Locale::Pt => "Erro ao abrir arquivos para diff",
            Locale::En => "Error opening files for diff",
        }
    }

    // --- Barra de Status Inferior ---

    pub fn status_mft_scan(&self) -> &'static str {
        match self {
            Locale::Pt => "✓ Varredura MFT Concluída",
            Locale::En => "✓ MFT Scan Complete",
        }
    }

    pub fn status_local_scan(&self) -> &'static str {
        match self {
            Locale::Pt => "✓ Varredura Local Concluída",
            Locale::En => "✓ Local Scan Complete",
        }
    }

    pub fn status_results_count(&self, filtered: usize, total: usize) -> String {
        match self {
            Locale::Pt => format!("{filtered} resultados de {total} arquivos indexados"),
            Locale::En => format!("{filtered} results out of {total} indexed files"),
        }
    }

    pub fn status_no_files_to_export(&self) -> &'static str {
        match self {
            Locale::Pt => "Nenhum arquivo nos resultados para exportar",
            Locale::En => "No files in results to export",
        }
    }

    pub fn status_csv_saved(&self, path: &str) -> String {
        match self {
            Locale::Pt => format!("✓ Resultados exportados para: {path}"),
            Locale::En => format!("✓ Results exported to: {path}"),
        }
    }

    pub fn status_csv_error(&self, err: &str) -> String {
        match self {
            Locale::Pt => format!("Erro ao exportar CSV: {err}"),
            Locale::En => format!("Error exporting CSV: {err}"),
        }
    }

    pub fn status_treemap_ready(&self, path: &str) -> String {
        match self {
            Locale::Pt => format!("✓ Treemap de {path} calculado com sucesso!"),
            Locale::En => format!("✓ Treemap for {path} calculated successfully!"),
        }
    }

    pub fn status_treemap_error(&self, err: &str) -> String {
        match self {
            Locale::Pt => format!("Falha na varredura do Treemap: {err}"),
            Locale::En => format!("Treemap scan failed: {err}"),
        }
    }
}
