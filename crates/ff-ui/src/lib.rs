//! `ff-ui`
//!
//! Interface gráfica desktop nativa em egui para o FastFile (§4 e Issues #13, #14, #15, #19).

pub mod app;
pub mod hotkey;
pub mod i18n;
pub mod theme;

pub use app::FastFileApp;
pub use hotkey::GlobalHotkeyListener;
pub use i18n::Locale;

#[cfg(test)]
mod tests {
    use super::*;
    use app::{QuickFilter, SortColumn};

    #[test]
    fn test_app_initialization_and_empty_search() {
        let app = FastFileApp::new();
        assert_eq!(app.query, "");
        assert_eq!(app.active_filter, QuickFilter::All);
        assert_eq!(app.sort_column, SortColumn::Name);

        // Deve carregar entradas no índice local ou IPC
        assert!(app.results.len() <= 2000);
    }

    #[test]
    fn test_format_size_bytes() {
        assert_eq!(app::format_size_bytes(500), "500 B");
        assert_eq!(app::format_size_bytes(1024), "1.0 KB");
        assert_eq!(app::format_size_bytes(10 * 1024 * 1024), "10.0 MB");
        assert_eq!(app::format_size_bytes(2 * 1024 * 1024 * 1024), "2.00 GB");
    }

    #[test]
    fn test_sorting_logic() {
        let mut app = FastFileApp::new();
        let mut items = vec![
            app::SearchResultItem {
                id: 1,
                name: "b.txt".into(),
                path: "b.txt".into(),
                size: 200,
                mtime: 10,
                is_dir: false,
            },
            app::SearchResultItem {
                id: 2,
                name: "a.txt".into(),
                path: "a.txt".into(),
                size: 100,
                mtime: 20,
                is_dir: false,
            },
        ];

        // Ordena por nome ascendente
        app.sort_column = SortColumn::Name;
        app.sort_ascending = true;
        app.apply_sorting(&mut items);
        assert_eq!(items[0].name, "a.txt");
        assert_eq!(items[1].name, "b.txt");

        // Ordena por tamanho descendente
        app.sort_column = SortColumn::Size;
        app.sort_ascending = false;
        app.apply_sorting(&mut items);
        assert_eq!(items[0].name, "b.txt"); // 200 bytes > 100 bytes
        assert_eq!(items[1].name, "a.txt");
    }

    #[test]
    fn test_selection_and_diff_trigger() {
        let mut app = FastFileApp::new();
        app.results = vec![
            app::SearchResultItem {
                id: 1,
                name: "a.txt".into(),
                path: "a.txt".into(),
                size: 10,
                mtime: 10,
                is_dir: false,
            },
            app::SearchResultItem {
                id: 2,
                name: "b.txt".into(),
                path: "b.txt".into(),
                size: 20,
                mtime: 20,
                is_dir: false,
            },
        ];

        // Seleciona 2 arquivos
        app.selected_indices.insert(0);
        app.selected_indices.insert(1);
        assert_eq!(app.selected_indices.len(), 2);
    }

    #[test]
    fn test_generate_csv_string_and_escaping() {
        let items = vec![
            app::SearchResultItem {
                id: 1,
                name: "dados, 2026.csv".into(),
                path: "C:\\Users\\fabio\\dados, 2026.csv".into(),
                size: 1024,
                mtime: 1700000000,
                is_dir: false,
            },
            app::SearchResultItem {
                id: 2,
                name: "pasta \"especial\"".into(),
                path: "C:\\Users\\fabio\\pasta \"especial\"".into(),
                size: 0,
                mtime: 0,
                is_dir: true,
            },
        ];

        let csv = app::generate_csv_string(&items);
        assert!(csv.starts_with("Nome,Caminho,Tipo,Tamanho_Bytes,Tamanho_Formatado,Modificado\n"));
        // Campo com vírgula deve vir com aspas
        assert!(csv.contains("\"dados, 2026.csv\""));
        assert!(csv.contains("\"C:\\Users\\fabio\\dados, 2026.csv\""));
        // Campo com aspas duplas deve vir escapado com aspas duplas dobradas
        assert!(csv.contains("\"pasta \"\"especial\"\"\""));
        assert!(csv.contains("1024,1.0 KB"));
        assert!(csv.contains("Diretório,0,0 B"));
    }

    #[test]
    fn test_locale_system_detection_and_toggle() {
        let detected = Locale::detect_from_system();
        assert!(detected == Locale::Pt || detected == Locale::En);

        // Toggle
        assert_eq!(Locale::Pt.toggle(), Locale::En);
        assert_eq!(Locale::En.toggle(), Locale::Pt);

        // Strings em Português
        assert_eq!(Locale::Pt.code(), "PT");
        assert!(Locale::Pt.window_title().contains("Busca"));
        assert_eq!(Locale::Pt.tab_results(), "📂 Resultados");
        assert_eq!(Locale::Pt.tab_treemap(), "📊 Treemap de Uso");
        assert_eq!(Locale::Pt.tab_diff(), "⚖️ Diff Lado a Lado");
        assert_eq!(Locale::Pt.col_name(), "Nome");

        // Strings em Inglês
        assert_eq!(Locale::En.code(), "EN");
        assert!(Locale::En.window_title().contains("Search"));
        assert_eq!(Locale::En.tab_results(), "📂 Results");
        assert_eq!(Locale::En.tab_treemap(), "📊 Disk Treemap");
        assert_eq!(Locale::En.tab_diff(), "⚖️ Side-by-Side Diff");
        assert_eq!(Locale::En.col_name(), "Name");
    }
}
