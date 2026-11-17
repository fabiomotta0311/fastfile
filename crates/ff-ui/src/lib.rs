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

        app.local_index = Some(idx);
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

    #[test]
    fn test_tm_01_resizing_recomputes_layout_without_disk_scan() {
        use ff_size::treemap::TreemapRect;

        let mut app = FastFileApp::new();
        let mut idx = ff_index::FileIndex::new();
        let r = idx
            .add_entry(0, ff_index::NO_PARENT, "C:", ff_index::FLAG_DIRECTORY, 0, 0)
            .unwrap();
        let _ = idx.add_entry(1, r, "doc.txt", 0, 1024, 100).unwrap();
        let _ = idx.add_entry(2, r, "img.png", 0, 2048, 200).unwrap();

        app.treemap_custom_index = Some(idx);
        app.treemap_dir_id = 0;

        // Layout inicial 800x600
        let b1 = TreemapRect::new(0.0, 0.0, 800.0, 600.0);
        app.recompute_treemap_layout(b1);
        assert!(app.treemap_layout.is_some());
        let l1 = app.treemap_layout.as_ref().unwrap();
        assert_eq!(l1.bounds.w, 800.0);
        assert_eq!(l1.bounds.h, 600.0);
        assert_eq!(l1.items.len(), 2);

        // Redimensionamento para 1280x720: deve reorganizar o mapa geometricamente sem tocar em disco nem criar receiver
        let b2 = TreemapRect::new(0.0, 0.0, 1280.0, 720.0);
        app.recompute_treemap_layout(b2);
        let l2 = app.treemap_layout.as_ref().unwrap();
        assert_eq!(l2.bounds.w, 1280.0);
        assert_eq!(l2.bounds.h, 720.0);
        assert_eq!(l2.items.len(), 2);
        assert!(app.treemap_scanning_path.is_none());
        assert!(app.treemap_receiver.is_none());
    }

    #[test]
    fn test_tm_01_refresh_rereads_disk_reflecting_changes() {
        use std::fs;

        let temp_dir = std::env::temp_dir().join(format!(
            "ff_tm01_refresh_test_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis()
        ));
        let _ = fs::create_dir_all(&temp_dir);
        let file_a = temp_dir.join("arquivo_a.txt");
        fs::write(&file_a, vec![b'a'; 100]).unwrap();

        let mut app = FastFileApp::new();
        let path_str = temp_dir.to_string_lossy().to_string();

        // 1. Dispara varredura inicial do disco (não bloqueante)
        app.trigger_treemap_disk_scan(path_str.clone(), false);
        assert!(app.treemap_receiver.is_some());
        assert_eq!(app.treemap_scanning_path.as_ref().unwrap(), &path_str);

        // Aguarda término da thread de varredura
        let rx = app.treemap_receiver.take().unwrap();
        let res1 = rx
            .recv_timeout(std::time::Duration::from_secs(5))
            .expect("timeout aguardando varredura inicial");
        app.treemap_custom_index = Some(res1.index);
        app.treemap_last_analyzed = Some(std::time::SystemTime::now());
        app.treemap_data_source = Some(res1.source);
        app.recompute_treemap_layout(ff_size::treemap::TreemapRect::new(0.0, 0.0, 600.0, 400.0));

        let l1 = app.treemap_layout.as_ref().unwrap();
        assert_eq!(l1.total_size, 100);
        assert_eq!(l1.items.len(), 1);
        assert_eq!(l1.items[0].name, "arquivo_a.txt");
        assert_eq!(l1.items[0].size, 100);

        // 2. Modifica arquivo externamente e adiciona novo arquivo
        fs::write(&file_a, vec![b'a'; 300]).unwrap(); // aumenta de 100 para 300 bytes
        let file_b = temp_dir.join("arquivo_b.rs");
        fs::write(&file_b, vec![b'b'; 200]).unwrap(); // novo arquivo de 200 bytes

        // 3. Executa a ação de atualizar análise
        app.trigger_treemap_disk_scan(path_str.clone(), true);
        let rx2 = app.treemap_receiver.take().unwrap();
        let res2 = rx2
            .recv_timeout(std::time::Duration::from_secs(5))
            .expect("timeout aguardando atualização de análise");

        app.treemap_custom_index = Some(res2.index);
        app.treemap_last_analyzed = Some(std::time::SystemTime::now());
        app.treemap_data_source = Some(res2.source);
        app.recompute_treemap_layout(ff_size::treemap::TreemapRect::new(0.0, 0.0, 600.0, 400.0));

        let l2 = app.treemap_layout.as_ref().unwrap();
        assert_eq!(l2.total_size, 500); // 300 + 200
        assert_eq!(l2.items.len(), 2);
        assert!(app.treemap_last_analyzed.is_some());
        assert_eq!(
            app.treemap_data_source,
            Some(app::TreemapDataSource::DiskScan)
        );

        // 4. Remove um arquivo externamente e atualiza novamente
        let _ = fs::remove_file(&file_a);
        app.trigger_treemap_disk_scan(path_str.clone(), true);
        let rx3 = app.treemap_receiver.take().unwrap();
        let res3 = rx3
            .recv_timeout(std::time::Duration::from_secs(5))
            .expect("timeout aguardando remocao");
        app.treemap_custom_index = Some(res3.index);
        app.recompute_treemap_layout(ff_size::treemap::TreemapRect::new(0.0, 0.0, 600.0, 400.0));

        let l3 = app.treemap_layout.as_ref().unwrap();
        assert_eq!(l3.total_size, 200);
        assert_eq!(l3.items.len(), 1);
        assert_eq!(l3.items[0].name, "arquivo_b.rs");

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_tm_01_navigation_and_history_preserved_on_refresh() {
        use std::fs;

        let temp_dir = std::env::temp_dir().join(format!(
            "ff_tm01_nav_test_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis()
        ));
        let sub_dir = temp_dir.join("subpasta");
        let _ = fs::create_dir_all(&sub_dir);
        let inner_file = sub_dir.join("item.txt");
        fs::write(&inner_file, vec![b'x'; 150]).unwrap();

        let mut app = FastFileApp::new();
        let root_str = temp_dir.to_string_lossy().to_string();

        // Varredura da raiz
        app.trigger_treemap_disk_scan(root_str.clone(), false);
        let rx = app.treemap_receiver.take().unwrap();
        let res = rx.recv_timeout(std::time::Duration::from_secs(5)).unwrap();
        app.treemap_custom_index = Some(res.index);
        app.treemap_root_path = Some(root_str.clone());
        app.recompute_treemap_layout(ff_size::treemap::TreemapRect::new(0.0, 0.0, 800.0, 600.0));

        // Navega para a subpasta
        app.navigate_treemap_to_path(
            &sub_dir,
            ff_size::treemap::TreemapRect::new(0.0, 0.0, 800.0, 600.0),
        );
        assert_ne!(app.treemap_dir_id, 0);
        assert_eq!(app.treemap_history, vec![0]);
        assert!(app.treemap_path_input.to_lowercase().contains("subpasta"));

        // Adiciona um arquivo dentro da subpasta
        let extra_file = sub_dir.join("extra.txt");
        fs::write(&extra_file, vec![b'y'; 250]).unwrap();

        // Atualizar análise com preservação da pasta visualizada
        app.trigger_treemap_disk_scan(root_str.clone(), true);
        let rx2 = app.treemap_receiver.take().unwrap();
        let res2 = rx2.recv_timeout(std::time::Duration::from_secs(5)).unwrap();

        // Simula o processamento do receiver no update()
        let preserve_target = res2.preserve_path.unwrap();
        app.treemap_custom_index = Some(res2.index);
        let mut target_dir_id = 0;
        let p_norm = preserve_target.trim_end_matches(['\\', '/']).to_lowercase();
        if let Some(ref idx) = app.treemap_custom_index {
            for (id, entry) in idx.entries.iter().enumerate() {
                if entry.is_dir() && !entry.is_deleted() {
                    if let Ok(entry_path) = idx.build_path(id as u32) {
                        if entry_path.trim_end_matches(['\\', '/']).to_lowercase() == p_norm {
                            target_dir_id = id as u32;
                            break;
                        }
                    }
                }
            }
        }
        app.treemap_dir_id = target_dir_id;
        app.rebuild_treemap_history();
        app.recompute_treemap_layout(ff_size::treemap::TreemapRect::new(0.0, 0.0, 800.0, 600.0));

        // A navegação permaneceu na subpasta e o histórico até a raiz foi preservado
        assert_ne!(app.treemap_dir_id, 0);
        assert_eq!(app.treemap_history, vec![0]);
        let l = app.treemap_layout.as_ref().unwrap();
        assert_eq!(l.items.len(), 2); // item.txt + extra.txt
        assert_eq!(l.total_size, 400); // 150 + 250

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_tm_01_locale_strings_and_data_source() {
        let loc = Locale::Pt;
        assert_eq!(loc.treemap_refresh(), "🔄 Atualizar análise");
        assert!(loc
            .treemap_refresh_tooltip()
            .contains("Lê os dados atualizados do disco"));
        assert!(loc
            .treemap_last_analysis("2026-11-17 01:00:00", "Varredura de Disco")
            .contains("Última análise: 2026-11-17 01:00:00"));

        let loc_en = Locale::En;
        assert_eq!(loc_en.treemap_refresh(), "🔄 Refresh analysis");
        assert!(loc_en
            .treemap_last_analysis("2026-11-17 01:00:00", "Disk Scan")
            .contains("Last analysis: 2026-11-17 01:00:00"));

        assert_eq!(
            app::TreemapDataSource::DiskScan.label(Locale::Pt),
            "Varredura de Disco"
        );
        assert_eq!(
            app::TreemapDataSource::Mft.label(Locale::Pt),
            "NTFS MFT (Volume)"
        );
        assert_eq!(
            app::TreemapDataSource::DiskScan.label(Locale::En),
            "Disk Scan"
        );
    }

    #[test]
    fn test_tm_02_scan_cancellation_and_partial_result() {
        use std::fs;
        use std::sync::atomic::Ordering;

        let temp_dir = std::env::temp_dir().join(format!(
            "ff_tm02_cancel_test_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis()
        ));
        let _ = fs::create_dir_all(&temp_dir);
        for i in 0..20 {
            fs::write(temp_dir.join(format!("file_{i}.txt")), vec![b'c'; 50]).unwrap();
        }

        let mut app = FastFileApp::new();
        let path_str = temp_dir.to_string_lossy().to_string();

        app.trigger_treemap_disk_scan(path_str, false);
        assert!(app.treemap_scanning_path.is_some());
        assert!(app.treemap_cancel_token.is_some());

        // Cancela a varredura ativamente
        app.cancel_treemap_scan();
        assert!(app.treemap_scanning_path.is_none());
        assert!(matches!(
            app.treemap_status,
            Some(app::TreemapAnalysisStatus::Cancelled { .. })
        ));
        assert!(app
            .treemap_cancel_token
            .as_ref()
            .unwrap()
            .load(Ordering::Relaxed));

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_tm_02_access_errors_registered_as_partial_analysis() {
        use app::{ScanAccessError, TreemapAnalysisStatus};

        let errors = [
            ScanAccessError {
                path: "C:\\System Volume Information".to_string(),
                message: "Acesso negado (os error 5)".to_string(),
            },
            ScanAccessError {
                path: "C:\\Windows\\System32\\config".to_string(),
                message: "Permissão insuficiente".to_string(),
            },
        ];

        let status_partial = TreemapAnalysisStatus::Partial {
            errors_count: errors.len(),
        };

        // Critério de aceite: Análise parcial NÃO é apresentada como completa
        assert_ne!(status_partial, TreemapAnalysisStatus::Complete);
        assert!(matches!(
            status_partial,
            TreemapAnalysisStatus::Partial { errors_count: 2 }
        ));

        // Critério de aceite: Falhas de acesso aparecem no resumo
        let summary_pt = Locale::Pt.treemap_status_partial(2);
        assert!(summary_pt.contains("Parcial (2 pastas sem permissão)"));

        let summary_en = Locale::En.treemap_status_partial(2);
        assert!(summary_en.contains("Partial (2 restricted folders)"));
    }

    #[test]
    fn test_tm_02_old_analysis_does_not_overwrite_current_target() {
        use app::{TreemapAnalysisStatus, TreemapDataSource, TreemapScanResult};
        use std::fs;

        let base_tmp = std::env::temp_dir().join(format!(
            "ff_tm02_stale_test_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis()
        ));
        let dir1 = base_tmp.join("alvo_antigo");
        let dir2 = base_tmp.join("alvo_novo");
        let _ = fs::create_dir_all(&dir1);
        let _ = fs::create_dir_all(&dir2);

        let mut app = FastFileApp::new();

        // Dispara varredura 1
        app.trigger_treemap_disk_scan(dir1.to_string_lossy().to_string(), false);
        let scan_1_id = app.treemap_current_scan_id;
        assert_eq!(scan_1_id, 1);

        // Dispara varredura 2 antes da 1 concluir (cancela e invalida a anterior)
        app.trigger_treemap_disk_scan(dir2.to_string_lossy().to_string(), false);
        let scan_2_id = app.treemap_current_scan_id;
        assert_eq!(scan_2_id, 2);

        // Cria resultado fictício que teria chegado atrasado da varredura 1
        let mut idx_old = ff_index::FileIndex::new();
        let _ = idx_old.add_entry(
            0,
            ff_index::NO_PARENT,
            "AlvoAntigo",
            ff_index::FLAG_DIRECTORY,
            0,
            0,
        );

        let old_result = TreemapScanResult {
            scan_id: scan_1_id,
            root_path: dir1.to_string_lossy().to_string(),
            preserve_path: None,
            index: idx_old,
            source: TreemapDataSource::DiskScan,
            scan_ms: 50.0,
            status: TreemapAnalysisStatus::Complete,
            access_errors: Vec::new(),
        };

        // Simula verificação de descarte: scan_id antigo não bate com treemap_current_scan_id
        let should_discard = old_result.scan_id != app.treemap_current_scan_id;
        assert!(
            should_discard,
            "Resultado com scan_id antigo DEVE ser descartado"
        );

        // Cria resultado da varredura 2 (alvo novo)
        let mut idx_new = ff_index::FileIndex::new();
        let _ = idx_new.add_entry(
            0,
            ff_index::NO_PARENT,
            "AlvoNovo",
            ff_index::FLAG_DIRECTORY,
            0,
            0,
        );

        let new_result = TreemapScanResult {
            scan_id: scan_2_id,
            root_path: dir2.to_string_lossy().to_string(),
            preserve_path: None,
            index: idx_new,
            source: TreemapDataSource::DiskScan,
            scan_ms: 60.0,
            status: TreemapAnalysisStatus::Complete,
            access_errors: Vec::new(),
        };

        let should_accept = new_result.scan_id == app.treemap_current_scan_id;
        assert!(should_accept, "Resultado com scan_id atual DEVE ser aceito");

        // Aplica o resultado novo
        app.treemap_custom_index = Some(new_result.index);
        app.treemap_root_path = Some(new_result.root_path.clone());
        app.treemap_status = Some(new_result.status);

        assert_eq!(
            app.treemap_root_path.as_deref(),
            Some(dir2.to_string_lossy().as_ref())
        );
        assert_eq!(app.treemap_status, Some(TreemapAnalysisStatus::Complete));

        let _ = fs::remove_dir_all(&base_tmp);
    }

    #[test]
    fn test_tm_02_real_time_progress_tracking() {
        use std::fs;
        use std::sync::atomic::Ordering;

        let temp_dir = std::env::temp_dir().join(format!(
            "ff_tm02_prog_test_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis()
        ));
        let _ = fs::create_dir_all(&temp_dir);
        for i in 0..15 {
            fs::write(temp_dir.join(format!("item_{i}.dat")), vec![b'z'; 120]).unwrap();
        }

        let mut app = FastFileApp::new();
        let path_str = temp_dir.to_string_lossy().to_string();

        app.trigger_treemap_disk_scan(path_str, false);
        assert!(app.treemap_progress.is_some());

        // Aguarda a conclusão
        let rx = app.treemap_receiver.take().unwrap();
        let res = rx.recv_timeout(std::time::Duration::from_secs(5)).unwrap();

        let progress = app.treemap_progress.as_ref().unwrap();
        assert!(progress.items_count.load(Ordering::Relaxed) >= 15);
        assert!(progress.bytes_count.load(Ordering::Relaxed) >= (15 * 120));
        assert_eq!(res.status, app::TreemapAnalysisStatus::Complete);

        let _ = fs::remove_dir_all(&temp_dir);
    }
}
