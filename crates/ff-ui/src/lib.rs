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

/// Ícone oficial do aplicativo embutido em PNG.
pub const APP_ICON_PNG: &[u8] = include_bytes!("../icon.png");

/// Carrega o ícone oficial da janela para o `egui::ViewportBuilder`.
#[must_use]
pub fn load_app_icon() -> Option<egui::IconData> {
    let img = image::load_from_memory_with_format(APP_ICON_PNG, image::ImageFormat::Png)
        .ok()?
        .to_rgba8();
    let (width, height) = img.dimensions();
    Some(egui::IconData {
        rgba: img.into_raw(),
        width,
        height,
    })
}

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
    fn test_resizable_columns_and_search_target_path() {
        let mut app = FastFileApp::new();
        assert!(!app.search_target_path.is_empty());
        assert_eq!(app.preview_split_ratio, 0.55);
        assert_eq!(app.col_width_name, 240.0);
        assert_eq!(app.col_width_size, 90.0);

        // Altera largura das colunas e split ratio
        app.col_width_name = 320.0;
        app.col_width_size = 110.0;
        app.preview_split_ratio = 0.65;

        assert_eq!(app.col_width_name, 320.0);
        assert_eq!(app.col_width_size, 110.0);
        assert_eq!(app.preview_split_ratio, 0.65);
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
        assert!(matches!(
            detected,
            Locale::Pt | Locale::En | Locale::Es | Locale::Fr | Locale::It | Locale::Ru
        ));

        // Toggle cíclico pelos 6 idiomas
        assert_eq!(Locale::Pt.toggle(), Locale::En);
        assert_eq!(Locale::En.toggle(), Locale::Es);
        assert_eq!(Locale::Es.toggle(), Locale::Fr);
        assert_eq!(Locale::Fr.toggle(), Locale::It);
        assert_eq!(Locale::It.toggle(), Locale::Ru);
        assert_eq!(Locale::Ru.toggle(), Locale::Pt);

        // Strings em Português
        assert_eq!(Locale::Pt.code(), "PT");
        assert!(Locale::Pt.window_title().contains("Busca"));
        assert_eq!(Locale::Pt.tab_results(), "📂 Resultados");
        assert_eq!(Locale::Pt.tab_treemap(), "📊 Treemap de Uso");
        assert_eq!(Locale::Pt.col_name(), "Nome");

        // Strings em Inglês
        assert_eq!(Locale::En.code(), "EN");
        assert!(Locale::En.window_title().contains("Search"));
        assert_eq!(Locale::En.tab_results(), "📂 Results");
        assert_eq!(Locale::En.tab_treemap(), "📊 Disk Treemap");
        assert_eq!(Locale::En.col_name(), "Name");

        // Strings em Espanhol
        assert_eq!(Locale::Es.code(), "ES");
        assert!(Locale::Es.window_title().contains("Búsqueda"));
        assert_eq!(Locale::Es.tab_results(), "📂 Resultados");
        assert_eq!(Locale::Es.col_name(), "Nombre");

        // Strings em Francês
        assert_eq!(Locale::Fr.code(), "FR");
        assert!(Locale::Fr.window_title().contains("Recherche"));
        assert_eq!(Locale::Fr.tab_results(), "📂 Résultats");
        assert_eq!(Locale::Fr.col_name(), "Nom");

        // Strings em Italiano
        assert_eq!(Locale::It.code(), "IT");
        assert!(Locale::It.window_title().contains("Ricerca"));
        assert_eq!(Locale::It.tab_results(), "📂 Risultati");
        assert_eq!(Locale::It.col_name(), "Nome");

        // Strings em Russo (Cirílico completo em UTF-8)
        assert_eq!(Locale::Ru.code(), "RU");
        assert!(Locale::Ru.window_title().contains("Поиск"));
        assert_eq!(Locale::Ru.tab_results(), "📂 Результаты");
        assert_eq!(Locale::Ru.tab_treemap(), "📊 Дерево Диска");
        assert_eq!(Locale::Ru.tab_diff(), "⚖️ Сравнение Diff");
        assert_eq!(Locale::Ru.col_name(), "Имя");
        assert_eq!(Locale::Ru.col_size(), "Размер");
        assert_eq!(Locale::Ru.col_path(), "Путь");
    }

    #[test]
    fn test_load_app_icon() {
        let icon = load_app_icon();
        assert!(icon.is_some());
        let icon_data = icon.unwrap();
        assert_eq!(icon_data.width, 256);
        assert_eq!(icon_data.height, 256);
        assert_eq!(icon_data.rgba.len(), (256 * 256 * 4) as usize);
    }

    #[test]
    fn test_execute_search_with_local_index_and_filters() {
        let mut app = FastFileApp::new();
        // Garante que o índice local possui dados
        let mut idx = ff_index::FileIndex::new();
        let r = idx
            .add_entry(0, ff_index::NO_PARENT, "C:", ff_index::FLAG_DIRECTORY, 0, 0)
            .unwrap();
        let f1 = idx.add_entry(1, r, "projeto.rs", 0, 1024, 100).unwrap();
        let f2 = idx.add_entry(2, r, "manual.pdf", 0, 2048, 200).unwrap();
        let f3 = idx.add_entry(3, r, "foto.png", 0, 4096, 300).unwrap();
        let _ = (f1, f2, f3);

        app.local_index = Some(std::sync::Arc::new(idx));
        app.is_ipc_connected = false;

        // Busca vazia traz tudo
        app.query.clear();
        app.execute_search();
        assert_eq!(app.results.len(), 4); // C:, projeto.rs, manual.pdf, foto.png

        // Busca por código
        app.active_filter = app::QuickFilter::Code;
        app.execute_search();
        assert_eq!(app.results.len(), 1);
        assert_eq!(app.results[0].name, "projeto.rs");

        // Busca por docs
        app.active_filter = app::QuickFilter::Docs;
        app.execute_search();
        assert_eq!(app.results.len(), 1);
        assert_eq!(app.results[0].name, "manual.pdf");
    }
}
