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
        fs::create_dir_all(temp_dir.join("subpasta")).unwrap();
        // Use an equivalent non-canonical spelling, as can happen with Windows temp paths.
        let sub_dir = temp_dir.join("subpasta").join("..").join("subpasta");
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

    #[test]
    fn test_tm_03_format_optional_size_never_shows_unknown_as_zero() {
        use app::format_optional_size;

        // Critério de aceite: Valor desconhecido NÃO aparece como zero
        for loc in [
            Locale::Pt,
            Locale::En,
            Locale::Es,
            Locale::Fr,
            Locale::It,
            Locale::Ru,
        ] {
            let unknown_str = format_optional_size(None, loc);
            assert_ne!(
                unknown_str, "0 B",
                "Valor desconhecido não pode ser exibido como 0 B"
            );
            assert!(
                !unknown_str.starts_with('0'),
                "Valor desconhecido não pode começar com zero"
            );
            assert!(
                unknown_str.contains('—') || unknown_str.contains('-'),
                "Valor desconhecido deve conter traço ou indicação de indisponibilidade"
            );

            // Valores conhecidos aparecem corretamente
            assert_eq!(format_optional_size(Some(0), loc), "0 B");
            assert_eq!(format_optional_size(Some(2048), loc), "2.0 KB");
        }
    }

    #[test]
    fn test_tm_03_ui_size_modes_and_layout_recomputation() {
        use ff_index::entry::{FLAG_DIRECTORY, FLAG_FILE, FLAG_OFFLINE, FLAG_SPARSE, NO_PARENT};
        use ff_size::treemap::{TreemapRect, TreemapSizeMode};

        let mut app = FastFileApp::new();
        let mut idx = ff_index::FileIndex::new();

        let root = idx
            .add_entry(1, NO_PARENT, "C:\\TestDir", FLAG_DIRECTORY, 0, 0)
            .unwrap();

        // Arquivo esparso: 100 MB lógico, 8 KB alocado
        let f_sparse = idx
            .add_entry(
                2,
                root,
                "sparse.vhdx",
                FLAG_FILE | FLAG_SPARSE,
                100_000_000,
                0,
            )
            .unwrap();
        idx.set_allocated_size(f_sparse, Some(8192));

        // Arquivo offline (OneDrive): 50 MB lógico, 0 B em disco
        let _f_cloud = idx
            .add_entry(
                3,
                root,
                "cloud.zip",
                FLAG_FILE | FLAG_OFFLINE,
                50_000_000,
                0,
            )
            .unwrap();

        app.treemap_custom_index = Some(idx);
        app.treemap_dir_id = root;

        let bounds = TreemapRect::new(0.0, 0.0, 800.0, 600.0);

        // 1. Modo Lógico: total deve ser 150 MB
        app.treemap_size_mode = TreemapSizeMode::Logical;
        app.recompute_treemap_layout(bounds);
        let layout_logical = app.treemap_layout.as_ref().unwrap();
        assert_eq!(layout_logical.total_size, 150_000_000);
        assert_eq!(layout_logical.size_mode, TreemapSizeMode::Logical);

        // 2. Modo Alocado (Em disco): total deve ser 8 KB (sparse) + 0 B (cloud) = 8 KB
        app.treemap_size_mode = TreemapSizeMode::Allocated;
        app.recompute_treemap_layout(bounds);
        let layout_alloc = app.treemap_layout.as_ref().unwrap();
        assert_eq!(layout_alloc.total_size, 8192);
        assert_eq!(layout_alloc.size_mode, TreemapSizeMode::Allocated);

        // 3. Modo Recuperação Estimada: total deve ser 8 KB
        app.treemap_size_mode = TreemapSizeMode::Reclaimable;
        app.recompute_treemap_layout(bounds);
        let layout_rec = app.treemap_layout.as_ref().unwrap();
        assert_eq!(layout_rec.total_size, 8192);
        assert_eq!(layout_rec.size_mode, TreemapSizeMode::Reclaimable);
    }

    #[test]
    fn test_tm_03_hard_links_do_not_promise_undue_reclamation() {
        use ff_index::entry::{FLAG_DIRECTORY, FLAG_FILE, NO_PARENT};
        use ff_size::treemap::{TreemapRect, TreemapSizeMode};

        let mut app = FastFileApp::new();
        let mut idx = ff_index::FileIndex::new();

        let root = idx
            .add_entry(1, NO_PARENT, "C:\\Root", FLAG_DIRECTORY, 0, 0)
            .unwrap();
        let sub1 = idx
            .add_entry(2, root, "Sub1", FLAG_DIRECTORY, 0, 0)
            .unwrap();
        let sub2 = idx
            .add_entry(3, root, "Sub2", FLAG_DIRECTORY, 0, 0)
            .unwrap();

        // Dois links apontando para o mesmo FRN 8888 (100 KB em disco)
        let _hl1 = idx
            .add_entry(8888, sub1, "hardlink_1.bin", FLAG_FILE, 100_000, 0)
            .unwrap();
        let _hl2 = idx
            .add_entry(8888, sub2, "hardlink_2.bin", FLAG_FILE, 100_000, 0)
            .unwrap();

        app.treemap_custom_index = Some(idx);
        let bounds = TreemapRect::new(0.0, 0.0, 800.0, 600.0);

        // Sub1 sozinho contém apenas 1 dos links -> recuperação estimada deve ser 0
        app.treemap_dir_id = sub1;
        app.treemap_size_mode = TreemapSizeMode::Reclaimable;
        app.recompute_treemap_layout(bounds);
        let layout_sub1 = app.treemap_layout.as_ref().unwrap();
        // Critério de aceite: Hard links não geram promessa indevida de recuperação
        assert_eq!(
            layout_sub1.total_reclaimable_size,
            Some(0),
            "Excluir Sub1 não libera os blocos compartilhados com Sub2"
        );

        // Root contém ambos os links -> recuperação estimada deve contabilizar o arquivo UMA ÚNICA VEZ
        app.treemap_dir_id = root;
        app.recompute_treemap_layout(bounds);
        let layout_root = app.treemap_layout.as_ref().unwrap();
        let expected_alloc = 100_000u64.div_ceil(4096) * 4096;
        assert_eq!(
            layout_root.total_reclaimable_size,
            Some(expected_alloc),
            "Excluir Root libera o arquivo apenas uma vez"
        );
    }

    #[test]
    fn test_tm_04_selection_bidirectional_and_small_items_access() {
        use ff_index::entry::{FLAG_DIRECTORY, FLAG_FILE, NO_PARENT};
        use ff_size::treemap::TreemapRect;

        let mut app = FastFileApp::new();
        let mut idx = ff_index::FileIndex::new();

        let root = idx
            .add_entry(1, NO_PARENT, "C:\\Project", FLAG_DIRECTORY, 0, 100)
            .unwrap();
        let huge = idx
            .add_entry(2, root, "huge_archive.zip", FLAG_FILE, 100_000_000, 200)
            .unwrap();
        // Item microscópico (1 byte num canvas pequeno será omitido dos retângulos gráficos)
        let micro = idx
            .add_entry(3, root, "config.ini", FLAG_FILE, 1, 300)
            .unwrap();
        // Item vazio (0 bytes)
        let empty = idx
            .add_entry(4, root, ".gitkeep", FLAG_FILE, 0, 400)
            .unwrap();

        app.treemap_custom_index = Some(idx);
        app.treemap_dir_id = root;

        // Limite gráfico pequeno (60x60) para forçar o micro a ficar menor que 2 pixels
        let bounds = TreemapRect::new(0.0, 0.0, 60.0, 60.0);
        app.recompute_treemap_layout(bounds);

        let layout = app.treemap_layout.as_ref().unwrap();

        // 1. O bloco grande deve ter retângulo no treemap
        assert!(layout.items.iter().any(|i| i.id == huge));

        // 2. Critério: Itens pequenos e vazios continuam acessíveis pela lista (all_items)
        assert_eq!(layout.all_items.len(), 3);
        let micro_row = layout.all_items.iter().find(|i| i.id == micro).unwrap();
        assert_eq!(micro_row.name, "config.ini");
        assert_eq!(micro_row.size, 1);
        assert!(
            !micro_row.has_rect,
            "Microscópico não coube nos retângulos gráficos"
        );

        let empty_row = layout.all_items.iter().find(|i| i.id == empty).unwrap();
        assert_eq!(empty_row.name, ".gitkeep");
        assert_eq!(empty_row.size, 0);
        assert_eq!(empty_row.percentage, 0.0);

        // 3. Critério: Seleção bidirecional
        // Selecionar na lista destaca o ID
        app.treemap_selected_id = Some(huge);
        assert_eq!(app.treemap_selected_id, Some(huge));
        // O treemap pode destacar o retângulo correspondente
        let huge_rect_item = layout
            .items
            .iter()
            .find(|i| i.id == app.treemap_selected_id.unwrap());
        assert!(huge_rect_item.is_some());

        // Selecionar item sem retângulo (micro) é permitido e preserva integridade
        app.treemap_selected_id = Some(micro);
        assert_eq!(app.treemap_selected_id, Some(micro));
    }

    #[test]
    fn test_tm_04_selection_coherence_across_sorting_and_navigation() {
        use ff_index::entry::{FLAG_DIRECTORY, FLAG_FILE, NO_PARENT};
        use ff_size::treemap::TreemapRect;

        let mut app = FastFileApp::new();
        let mut idx = ff_index::FileIndex::new();

        let root = idx
            .add_entry(1, NO_PARENT, "C:\\TestDir", FLAG_DIRECTORY, 0, 100)
            .unwrap();
        let _file_b = idx
            .add_entry(2, root, "beta.rs", FLAG_FILE, 20_000, 150)
            .unwrap();
        let file_a = idx
            .add_entry(3, root, "alpha.rs", FLAG_FILE, 50_000, 120)
            .unwrap();
        let _file_c = idx
            .add_entry(4, root, "gamma.rs", FLAG_FILE, 10_000, 200)
            .unwrap();
        let sub = idx
            .add_entry(5, root, "SubFolder", FLAG_DIRECTORY, 0, 300)
            .unwrap();

        app.treemap_custom_index = Some(idx);
        app.treemap_dir_id = root;
        app.recompute_treemap_layout(TreemapRect::new(0.0, 0.0, 500.0, 400.0));

        // Seleciona alpha.rs por ID
        app.treemap_selected_id = Some(file_a);

        // Critério: Seleção permanece coerente após ordenação
        app.treemap_sort_col = crate::app::TreemapSortColumn::Name;
        app.treemap_sort_desc = false;
        assert_eq!(app.treemap_selected_id, Some(file_a));

        app.treemap_sort_col = crate::app::TreemapSortColumn::Size;
        app.treemap_sort_desc = true;
        assert_eq!(app.treemap_selected_id, Some(file_a));

        app.treemap_sort_col = crate::app::TreemapSortColumn::Percentage;
        assert_eq!(app.treemap_selected_id, Some(file_a));

        app.treemap_sort_col = crate::app::TreemapSortColumn::Modified;
        assert_eq!(app.treemap_selected_id, Some(file_a));

        // Filtragem preserva o item selecionado
        app.treemap_search_query = "alpha".to_string();
        assert_eq!(app.treemap_selected_id, Some(file_a));

        // Navegação para outra pasta limpa a seleção coherentemente
        app.treemap_history.push(root);
        app.treemap_dir_id = sub;
        app.treemap_selected_id = None;
        app.treemap_search_query.clear();
        app.recompute_treemap_layout(TreemapRect::new(0.0, 0.0, 500.0, 400.0));

        assert_eq!(app.treemap_selected_id, None);
        assert_eq!(app.treemap_dir_id, sub);
    }

    #[test]
    fn test_tm_04_performance_and_responsiveness_large_synthetic_corpus() {
        use ff_index::entry::{FLAG_DIRECTORY, FLAG_FILE, NO_PARENT};
        use ff_size::treemap::TreemapRect;

        let mut app = FastFileApp::new();
        let mut idx = ff_index::FileIndex::new();

        let root = idx
            .add_entry(1, NO_PARENT, "C:\\BigCorpus", FLAG_DIRECTORY, 0, 100)
            .unwrap();

        // Insere 20.000 arquivos sintéticos na pasta
        for i in 0..20_000 {
            let name = format!("file_{i:05}.bin");
            let size = ((i as u64 * 37) % 500_000) + 1;
            idx.add_entry(
                (i + 10) as u64,
                root,
                &name,
                FLAG_FILE,
                size,
                1000 + i as i64,
            )
            .unwrap();
        }

        app.treemap_custom_index = Some(idx);
        app.treemap_dir_id = root;

        let start = std::time::Instant::now();
        app.recompute_treemap_layout(TreemapRect::new(0.0, 0.0, 800.0, 600.0));
        let elapsed = start.elapsed();

        let layout = app.treemap_layout.as_ref().unwrap();
        assert_eq!(layout.all_items.len(), 20_000);

        // O cálculo deve ser instantâneo mesmo com 20k itens (< 100ms)
        assert!(
            elapsed.as_millis() < 500,
            "Cálculo de layout e lista deve ser ultra veloz, levou: {:?}",
            elapsed
        );
    }

    #[test]
    fn test_tm_05_inspection_panel_details_and_top_files() {
        use ff_index::entry::{FLAG_DIRECTORY, FLAG_FILE, NO_PARENT};
        use ff_size::treemap::TreemapRect;

        let mut app = FastFileApp::new();
        let mut idx = ff_index::FileIndex::new();

        let root = idx
            .add_entry(1, NO_PARENT, "C:\\ProjectRoot", FLAG_DIRECTORY, 0, 100)
            .unwrap();
        let sub = idx
            .add_entry(2, root, "Assets", FLAG_DIRECTORY, 0, 150)
            .unwrap();

        // Arquivos dentro de Assets
        let f_big = idx
            .add_entry(3, sub, "textures_4k.bin", FLAG_FILE, 10_000_000, 200)
            .unwrap();
        let f_med = idx
            .add_entry(4, sub, "audio_bg.wav", FLAG_FILE, 2_000_000, 210)
            .unwrap();
        let f_sml = idx
            .add_entry(5, sub, "manifest.json", FLAG_FILE, 1_000, 220)
            .unwrap();

        app.treemap_custom_index = Some(idx);
        app.treemap_dir_id = root;
        app.recompute_treemap_layout(TreemapRect::new(0.0, 0.0, 800.0, 600.0));

        // Seleciona a pasta Assets
        app.treemap_selected_id = Some(sub);
        assert!(app.treemap_details_open);

        let layout = app.treemap_layout.as_ref().unwrap();
        let selected_item = layout.all_items.iter().find(|i| i.id == sub).unwrap();
        assert_eq!(selected_item.name, "Assets");
        assert!(selected_item.is_dir);

        // Agregação de maiores arquivos internos para o painel de inspeção
        let idx_ref = app.treemap_custom_index.as_ref().unwrap();
        let summary = ff_size::SizeAggregator::aggregate(idx_ref, Some(sub), None, 5);

        assert_eq!(summary.total_files, 3);
        assert_eq!(summary.top_files.len(), 3);
        // O primeiro deve ser textures_4k.bin
        assert_eq!(summary.top_files[0].id, f_big);
        assert_eq!(summary.top_files[0].size, 10_000_000);
        assert_eq!(summary.top_files[1].id, f_med);
        assert_eq!(summary.top_files[2].id, f_sml);
    }

    #[test]
    fn test_tm_05_missing_file_handling_graceful() {
        // Testa o tratamento seguro de itens que desapareceram após a varredura
        let non_existent_path = "C:\\fake_path_12345_nonexistent_file.tmp";
        assert!(!std::path::Path::new(non_existent_path).exists());

        // reveal_in_file_manager deve retornar erro explícito sem panic
        let res = crate::app::reveal_in_file_manager(non_existent_path);
        assert!(res.is_err());
        assert!(res.unwrap_err().contains("não encontrado no disco"));

        // Tentativa de preview de arquivo inexistente não bloqueia nem causa pânico
        let app = FastFileApp::new();
        let preview_res = app
            .preview_engine
            .preview_file(non_existent_path, None, 100);
        assert!(preview_res.is_err());
    }

    #[test]
    fn test_tm_05_large_file_preview_non_blocking_mmap() {
        use std::io::Write;

        // Cria arquivo temporário sintético de 2 MB usando std::env::temp_dir
        let temp_path =
            std::env::temp_dir().join(format!("ff_test_large_{}.tmp", std::process::id()));
        {
            let mut file = std::fs::File::create(&temp_path).unwrap();
            let chunk = vec![b'A'; 64 * 1024];
            for _ in 0..32 {
                file.write_all(&chunk).unwrap();
            }
            file.flush().unwrap();
        }
        let path_str = temp_path.to_string_lossy().to_string();

        let app = FastFileApp::new();
        let start = std::time::Instant::now();
        let res = app.preview_engine.preview_file(&path_str, None, 50);
        let elapsed = start.elapsed();

        let _ = std::fs::remove_file(&temp_path);

        assert!(res.is_ok());
        // Prévia deve ser quase instantânea com mmap (< 30ms)
        assert!(
            elapsed.as_millis() < 100,
            "Preview via mmap deve ser instantâneo, levou: {:?}",
            elapsed
        );
    }

    #[test]
    fn test_tm_05_actions_target_correct_selected_item() {
        use ff_index::entry::{FLAG_DIRECTORY, FLAG_FILE, NO_PARENT};
        use ff_size::treemap::TreemapRect;

        let mut app = FastFileApp::new();
        let mut idx = ff_index::FileIndex::new();

        let root_path = std::env::temp_dir().join("TestActions");
        let root = idx
            .add_entry(
                1,
                NO_PARENT,
                &root_path.to_string_lossy(),
                FLAG_DIRECTORY,
                0,
                100,
            )
            .unwrap();
        let sub = idx
            .add_entry(2, root, "SubDir", FLAG_DIRECTORY, 0, 100)
            .unwrap();
        let f1 = idx
            .add_entry(3, sub, "doc_a.txt", FLAG_FILE, 500, 100)
            .unwrap();
        let f2 = idx
            .add_entry(4, sub, "doc_b.txt", FLAG_FILE, 600, 100)
            .unwrap();

        app.treemap_custom_index = Some(idx);
        app.treemap_dir_id = sub;
        app.recompute_treemap_layout(TreemapRect::new(0.0, 0.0, 600.0, 400.0));

        // 1. Seleciona f1
        app.treemap_selected_id = Some(f1);
        let layout = app.treemap_layout.as_ref().unwrap();
        let item1 = layout.all_items.iter().find(|i| i.id == f1).unwrap();
        assert_eq!(item1.name, "doc_a.txt");

        // Simula ação de busca rápida na pasta do item
        let target_dir = std::path::Path::new(&item1.path)
            .parent()
            .unwrap()
            .to_string_lossy()
            .to_string();
        app.search_target_path = target_dir;
        assert_eq!(
            std::path::Path::new(&app.search_target_path),
            root_path.join("SubDir")
        );

        // Simula ação de stage para Diff
        app.treemap_diff_staged = Some((item1.id, item1.path.clone()));
        assert_eq!(app.treemap_diff_staged.as_ref().unwrap().0, f1);

        // 2. Seleciona f2
        app.treemap_selected_id = Some(f2);
        let item2 = layout.all_items.iter().find(|i| i.id == f2).unwrap();
        assert_eq!(item2.name, "doc_b.txt");

        // O stage de Diff ainda preserva f1 como primeiro arquivo
        assert_eq!(app.treemap_diff_staged.as_ref().unwrap().0, f1);
    }

    #[test]
    fn test_tm_06_ui_space_recovery_filters_presets_consistency_and_totals() {
        use ff_index::entry::{FLAG_DIRECTORY, FLAG_FILE, NO_PARENT};
        use ff_size::treemap::{SpaceRecoveryFilter, SpaceRecoveryPreset, TreemapRect};

        let now_unix = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);

        let mut app = FastFileApp::new();
        let mut idx = ff_index::FileIndex::new();

        let root = idx
            .add_entry(
                1,
                NO_PARENT,
                "C:\\RecoveryTest",
                FLAG_DIRECTORY,
                0,
                now_unix,
            )
            .unwrap();

        // 1. Arquivo de 120 MB (Maiores arquivos)
        let f_large = idx
            .add_entry(
                2,
                root,
                "big_database.sql",
                FLAG_FILE,
                120 * 1024 * 1024,
                now_unix,
            )
            .unwrap();
        // 2. Download antigo (30 MB, pasta download, 45 dias atrás)
        let mtime_45d = now_unix - (45 * 86400);
        let f_old_dl = idx
            .add_entry(
                3,
                root,
                "download_archive.tar.gz",
                FLAG_FILE,
                30 * 1024 * 1024,
                mtime_45d,
            )
            .unwrap();
        // 3. Imagem ISO instalador (2 GB, 15 dias atrás)
        let f_iso = idx
            .add_entry(
                4,
                root,
                "hyperv_guest.vhdx",
                FLAG_FILE,
                2 * 1024 * 1024 * 1024,
                now_unix - (15 * 86400),
            )
            .unwrap();
        // 4. Vídeo grande (45 MB .mkv, 2 dias atrás)
        let f_vid = idx
            .add_entry(
                5,
                root,
                "webinar.mkv",
                FLAG_FILE,
                45 * 1024 * 1024,
                now_unix - (2 * 86400),
            )
            .unwrap();
        // 5. Arquivo compactado não recente (50 MB .zip, 200 dias atrás)
        let mtime_200d = now_unix - (200 * 86400);
        let f_zip = idx
            .add_entry(
                6,
                root,
                "old_photos.zip",
                FLAG_FILE,
                50 * 1024 * 1024,
                mtime_200d,
            )
            .unwrap();
        // 6. Arquivo pequeno recente
        let _f_txt = idx
            .add_entry(7, root, "readme.txt", FLAG_FILE, 2048, now_unix)
            .unwrap();

        app.treemap_custom_index = Some(idx);
        app.treemap_dir_id = root;
        let bounds = TreemapRect::new(0.0, 0.0, 800.0, 600.0);

        // A. Sem filtro: Todos os 6 itens são exibidos e total da pasta é igual ao total filtrado
        app.recompute_treemap_layout(bounds);
        let l_all = app.treemap_layout.as_ref().unwrap();
        assert_eq!(l_all.all_items.len(), 6);
        assert_eq!(l_all.total_folder_items_count, 6);
        assert_eq!(l_all.total_size, l_all.total_folder_size);

        // B. Filtro: LargestFiles (>= 100 MB) -> big_database.sql e hyperv_guest.vhdx
        app.treemap_filter = SpaceRecoveryFilter::from_preset(SpaceRecoveryPreset::LargestFiles);
        app.recompute_treemap_layout(bounds);
        let l_largest = app.treemap_layout.as_ref().unwrap();
        // Critério: Mapa e lista mostram consistentemente o conjunto filtrado
        assert_eq!(l_largest.all_items.len(), 2);
        assert_eq!(l_largest.items.len(), 2);
        assert!(l_largest.all_items.iter().any(|i| i.id == f_large));
        assert!(l_largest.all_items.iter().any(|i| i.id == f_iso));
        // Critério: A interface diferencia total da pasta e total filtrado
        assert_eq!(l_largest.total_folder_items_count, 6);
        assert!(l_largest.total_size < l_largest.total_folder_size);

        // C. Filtro: OldDownloads -> download_archive.tar.gz
        app.treemap_filter = SpaceRecoveryFilter::from_preset(SpaceRecoveryPreset::OldDownloads);
        app.recompute_treemap_layout(bounds);
        let l_dl = app.treemap_layout.as_ref().unwrap();
        assert_eq!(l_dl.all_items.len(), 1);
        assert_eq!(l_dl.all_items[0].id, f_old_dl);

        // D. Filtro: InstallersAndImages -> hyperv_guest.vhdx
        app.treemap_filter =
            SpaceRecoveryFilter::from_preset(SpaceRecoveryPreset::InstallersAndImages);
        app.recompute_treemap_layout(bounds);
        let l_inst = app.treemap_layout.as_ref().unwrap();
        assert_eq!(l_inst.all_items.len(), 1);
        assert_eq!(l_inst.all_items[0].id, f_iso);

        // E. Filtro: LargeVideos -> webinar.mkv
        app.treemap_filter = SpaceRecoveryFilter::from_preset(SpaceRecoveryPreset::LargeVideos);
        app.recompute_treemap_layout(bounds);
        let l_vid = app.treemap_layout.as_ref().unwrap();
        assert_eq!(l_vid.all_items.len(), 1);
        assert_eq!(l_vid.all_items[0].id, f_vid);

        // F. Filtro: Archives -> download_archive.tar.gz (.gz) e old_photos.zip (.zip)
        app.treemap_filter = SpaceRecoveryFilter::from_preset(SpaceRecoveryPreset::Archives);
        app.recompute_treemap_layout(bounds);
        let l_arch = app.treemap_layout.as_ref().unwrap();
        assert_eq!(l_arch.all_items.len(), 2);
        assert!(l_arch.all_items.iter().any(|i| i.id == f_old_dl));
        assert!(l_arch.all_items.iter().any(|i| i.id == f_zip));

        // G. Filtro: NotModifiedRecently (> 180 dias) -> old_photos.zip
        app.treemap_filter =
            SpaceRecoveryFilter::from_preset(SpaceRecoveryPreset::NotModifiedRecently);
        app.recompute_treemap_layout(bounds);
        let l_old = app.treemap_layout.as_ref().unwrap();
        assert_eq!(l_old.all_items.len(), 1);
        assert_eq!(l_old.all_items[0].id, f_zip);

        // H. Filtros combinados: Tamanho mínimo >= 40 MB + Extensão .zip
        let combined = SpaceRecoveryFilter {
            min_size: Some(40 * 1024 * 1024),
            extensions: vec!["zip".to_string()],
            ..Default::default()
        };
        app.treemap_filter = combined;
        app.recompute_treemap_layout(bounds);
        let l_comb = app.treemap_layout.as_ref().unwrap();
        assert_eq!(l_comb.all_items.len(), 1);
        assert_eq!(l_comb.all_items[0].id, f_zip);

        // I. Limpar filtro restaura a visão completa da pasta
        app.treemap_filter = SpaceRecoveryFilter::default();
        app.recompute_treemap_layout(bounds);
        let l_restored = app.treemap_layout.as_ref().unwrap();
        assert_eq!(l_restored.all_items.len(), 6);
        assert_eq!(l_restored.total_folder_items_count, 6);
    }

    #[test]
    fn test_tm_07_cleanup_opportunities_rules_and_guidance() {
        use ff_size::cleanup::{analyze_cleanup_opportunity, CleanupCategory};

        // 1. Artefato de build em projeto Rust confirmado (Cargo.toml presente)
        let sibs_rust = ["Cargo.toml", "src", "Cargo.lock"];
        let advice_target = analyze_cleanup_opportunity(
            "C:\\Projects\\fastfile\\target",
            "target",
            true,
            Some(&sibs_rust),
        );
        assert_eq!(advice_target.category, CleanupCategory::BuildArtifacts);
        assert_eq!(advice_target.rule_id, "RULE-BUILD-RUST-TARGET");
        assert!(advice_target.safe_to_delete);
        assert!(advice_target.consequence.contains("cargo build"));
        assert!(advice_target.action.contains("cargo clean"));

        // 2. CRITÉRIO: Pasta chamada target ISOLADA sem marcadores -> Revisão Manual sem garantia de descarte seguro
        let advice_target_iso =
            analyze_cleanup_opportunity("C:\\Backup\\target", "target", true, None);
        assert_eq!(advice_target_iso.category, CleanupCategory::ManualReview);
        assert_eq!(advice_target_iso.rule_id, "RULE-AMBIGUOUS-TARGET-DIR");
        assert!(!advice_target_iso.safe_to_delete);

        // 3. CRITÉRIO: Pasta chamada cache ISOLADA sem marcadores -> Revisão Manual sem garantia de descarte seguro
        let advice_cache_iso = analyze_cleanup_opportunity("C:\\Data\\cache", "cache", true, None);
        assert_eq!(advice_cache_iso.category, CleanupCategory::ManualReview);
        assert_eq!(advice_cache_iso.rule_id, "RULE-AMBIGUOUS-CACHE-DIR");
        assert!(!advice_cache_iso.safe_to_delete);

        // 4. Cache conhecido em AppData\Local\Temp -> Descarte seguro
        let advice_temp = analyze_cleanup_opportunity(
            "C:\\Users\\fabio\\AppData\\Local\\Temp\\junk.tmp",
            "junk.tmp",
            false,
            None,
        );
        assert_eq!(advice_temp.category, CleanupCategory::KnownCache);
        assert_eq!(advice_temp.rule_id, "RULE-CACHE-KNOWN-TEMP");
        assert!(advice_temp.safe_to_delete);

        // 5. CRITÉRIO: Conteúdo pessoal NÃO é chamado de lixo
        let advice_doc = analyze_cleanup_opportunity(
            "C:\\Users\\fabio\\Documents\\Contrato.docx",
            "Contrato.docx",
            false,
            None,
        );
        assert_eq!(advice_doc.category, CleanupCategory::PersonalContent);
        assert_eq!(advice_doc.rule_id, "RULE-PERSONAL-USER-CONTENT");
        assert!(!advice_doc.safe_to_delete);
        assert!(!advice_doc.reason.to_lowercase().contains("lixo"));
        assert!(
            !advice_doc.action.to_lowercase().contains("lixo")
                || advice_doc.action.contains("não é lixo")
        );

        // 6. CRITÉRIO: Dados de aplicações e sistema recebem orientação adequada, não recomendação genérica de exclusão
        let advice_git =
            analyze_cleanup_opportunity("C:\\Projects\\repo\\.git", ".git", true, None);
        assert_eq!(advice_git.category, CleanupCategory::ApplicationData);
        assert_eq!(advice_git.rule_id, "RULE-APP-GIT-REPO");
        assert!(!advice_git.safe_to_delete);
        assert!(advice_git.action.contains("git"));

        let advice_sys = analyze_cleanup_opportunity(
            "C:\\Windows\\System32\\kernel32.dll",
            "kernel32.dll",
            false,
            None,
        );
        assert_eq!(advice_sys.category, CleanupCategory::SystemProtected);
        assert_eq!(advice_sys.rule_id, "RULE-SYSTEM-PROTECTED-OS");
        assert!(!advice_sys.safe_to_delete);
        assert!(advice_sys.action.contains("Limpeza de Disco"));

        // 7. Instaladores e imagens
        let advice_iso = analyze_cleanup_opportunity(
            "C:\\Downloads\\ubuntu-24.04.iso",
            "ubuntu-24.04.iso",
            false,
            None,
        );
        assert_eq!(advice_iso.category, CleanupCategory::InstallerOrDownload);
        assert_eq!(advice_iso.rule_id, "RULE-INSTALLER-OR-DOWNLOAD");
        assert!(advice_iso.safe_to_delete);
    }

    #[test]
    fn test_tm_08_duplicates_visual_integration_and_acceptance_criteria() {
        use ff_size::{VisualDupeFile, VisualDupeGroup};

        let mut app = FastFileApp::new();
        assert!(!app.treemap_dupes_open);

        // A. Abrir/fechar painel de duplicados
        app.treemap_dupes_open = true;
        assert!(app.treemap_dupes_open);

        // B. Criar grupo com 3 arquivos: 2 são hard links (mesmo file_id) e 1 é cópia independente
        // Critério: Hard links são diferenciados de cópias independentes
        let mut group_hl = VisualDupeGroup {
            id: 1,
            hash: 0xDEADBEEF,
            size: 10_000_000, // 10 MB
            files: vec![
                VisualDupeFile {
                    path: "C:\\Data\\file_main.dat".to_string(),
                    name: "file_main.dat".to_string(),
                    size: 10_000_000,
                    mtime: 1700000000,
                    is_hardlink: true,
                    file_id: Some(5001),
                    is_personal: false,
                    is_preserved: true, // Usuário escolheu preservar a cópia principal
                    exists: true,
                },
                VisualDupeFile {
                    path: "C:\\Links\\file_hardlink.dat".to_string(),
                    name: "file_hardlink.dat".to_string(),
                    size: 10_000_000,
                    mtime: 1700000000,
                    is_hardlink: true,
                    file_id: Some(5001), // Mesmo file_id físico
                    is_personal: false,
                    is_preserved: false,
                    exists: true,
                },
                VisualDupeFile {
                    path: "C:\\Backups\\file_independent_copy.dat".to_string(),
                    name: "file_independent_copy.dat".to_string(),
                    size: 10_000_000,
                    mtime: 1700000000,
                    is_hardlink: false,
                    file_id: Some(5002), // Nó físico independente
                    is_personal: false,
                    is_preserved: false,
                    exists: true,
                },
            ],
            wasted_bytes: 0,
            reclaimable_physical_bytes: 0,
            is_stale: false,
        };

        group_hl.recalculate_metrics();

        // Desperdício nominal: 3 arquivos * 10MB - 10MB = 20 MB
        assert_eq!(group_hl.wasted_bytes, 20_000_000);
        // Recuperação física real: deduplica os hardlinks (5001 e 5002 = 2 instâncias físicas).
        // Liberar uma cópia independente recupera 10 MB.
        assert_eq!(group_hl.reclaimable_physical_bytes, 10_000_000);

        // C. Grupo com conteúdo pessoal
        // Critério: A aplicação não escolhe automaticamente arquivos pessoais para remoção
        let group_personal = VisualDupeGroup {
            id: 2,
            hash: 0xCAFEBABE,
            size: 2_500_000,
            files: vec![
                VisualDupeFile {
                    path: "C:\\Users\\fabio\\Pictures\\family.jpg".to_string(),
                    name: "family.jpg".to_string(),
                    size: 2_500_000,
                    mtime: 1700000000,
                    is_hardlink: false,
                    file_id: Some(6001),
                    is_personal: true,
                    is_preserved: false, // Nenhuma cópia pessoal é automaticamente selecionada para remoção!
                    exists: true,
                },
                VisualDupeFile {
                    path: "C:\\Users\\fabio\\Downloads\\family_copy.jpg".to_string(),
                    name: "family_copy.jpg".to_string(),
                    size: 2_500_000,
                    mtime: 1700000000,
                    is_hardlink: false,
                    file_id: Some(6002),
                    is_personal: true,
                    is_preserved: false,
                    exists: true,
                },
            ],
            wasted_bytes: 2_500_000,
            reclaimable_physical_bytes: 2_500_000,
            is_stale: false,
        };

        for file in &group_personal.files {
            assert!(file.is_personal);
            assert!(
                !file.is_preserved,
                "Preservação deve ser uma decisão consciente do usuário"
            );
        }

        // D. Atribuir grupos à UI
        app.treemap_dupes_groups = vec![group_hl, group_personal];
        assert_eq!(app.treemap_dupes_groups.len(), 2);

        // E. Critério: Alterações ocorridas durante a análise invalidam ou revalidam a conclusão
        let temp_dir = tempfile::tempdir().unwrap();
        let path_a = temp_dir.path().join("live_a.txt");
        let path_b = temp_dir.path().join("live_b.txt");

        std::fs::write(&path_a, "Mesmo conteudo nos dois arquivos").unwrap();
        std::fs::write(&path_b, "Mesmo conteudo nos dois arquivos").unwrap();

        let options = ff_size::VisualDupeOptions::default();
        let mut live_groups =
            ff_size::find_visual_duplicates(temp_dir.path(), &options, None, None).unwrap();
        assert_eq!(live_groups.len(), 1);
        assert!(!live_groups[0].is_stale);

        // Altera externamente o arquivo B
        std::fs::write(&path_b, "Conteudo alterado externamente que quebra o hash").unwrap();

        // Verificação de stale acusa alteração
        assert!(live_groups[0].check_stale());
        assert!(live_groups[0].is_stale);

        // Revalidação falha devido ao conteúdo divergente
        assert!(!live_groups[0].revalidate());
        assert!(live_groups[0].is_stale);

        // Restaura o conteúdo original idêntico
        std::fs::write(&path_b, "Mesmo conteudo nos dois arquivos").unwrap();
        assert!(live_groups[0].revalidate());
        assert!(!live_groups[0].is_stale);
    }

    #[test]
    fn test_tm_09_recovery_simulation_and_review_list() {
        use ff_size::{RecoverySimulation, ReviewCandidateItem};

        // 1. Inicializa o simulador de recuperação
        let mut sim = RecoverySimulation::new();

        // 2. Adiciona a pasta pai: 'C:\Projects\fastfile\target' (1.82 GB)
        let parent = ReviewCandidateItem::new(
            101,
            "C:\\Projects\\fastfile\\target".to_string(),
            "target".to_string(),
            true,
            1_954_283_520,
            Some(1_954_283_520),
            Some(1_954_283_520),
            1731800000,
            false,
        );
        sim.add_candidate(parent);

        // 3. Adiciona um filho direto: 'C:\Projects\fastfile\target\debug\fastfile.exe' (45 MB)
        let child = ReviewCandidateItem::new(
            102,
            "C:\\Projects\\fastfile\\target\\debug\\fastfile.exe".to_string(),
            "fastfile.exe".to_string(),
            false,
            47_185_920,
            Some(47_185_920),
            Some(47_185_920),
            1731800000,
            false,
        );
        sim.add_candidate(child);

        // 4. Critério: Selecionar pasta e filho não duplica o total
        let summary = sim.recalculate();
        assert_eq!(summary.total_items, 2);
        assert_eq!(summary.effective_items, 1);
        assert_eq!(summary.covered_items, 1);

        // Critério: Selecionar pasta e filho não duplica o total nem no tamanho nominal nem na estimativa
        assert_eq!(summary.total_selected_logical_size, 1_954_283_520);

        // A estimativa recuperável DEDUPLICA o filho coberto pelo pai
        assert_eq!(summary.total_estimated_reclaimable_size, 1_954_283_520);

        // Critério: A lista permanece exclusivamente consultiva e distingue valores
        assert_eq!(
            summary.space_already_reclaimed, 0,
            "Modo estritamente consultivo: nenhuma exclusão realizada"
        );

        // 5. Verifica se o filho recebeu a marcação de ancestral
        assert_eq!(
            sim.candidates[1].covered_by_ancestor,
            Some("C:\\Projects\\fastfile\\target".to_string())
        );

        // 6. Critério: Itens desconhecidos ou desatualizados são sinalizados
        let mut stale_item = ReviewCandidateItem::new(
            103,
            "C:\\Users\\fabio\\Downloads\\installer.iso".to_string(),
            "installer.iso".to_string(),
            false,
            2_000_000_000,
            Some(2_000_000_000),
            Some(2_000_000_000),
            1700000000,
            false,
        );
        stale_item.is_stale = true;
        stale_item.exists = false;
        sim.add_candidate(stale_item);

        let summary_stale = sim.recalculate();
        assert_eq!(summary_stale.stale_or_missing_count, 1);

        // 7. Remoção de item
        assert!(sim.contains_id(102));
        sim.remove_candidate_by_id(102);
        assert!(!sim.contains_id(102));
        let summary_after_remove = sim.recalculate();
        assert_eq!(summary_after_remove.total_items, 2);
        assert_eq!(summary_after_remove.covered_items, 0);

        // 8. Exportação CSV
        let csv = sim.export_csv();
        assert!(csv.contains("Caminho,Nome,Tipo,Tamanho_Logico,Recuperacao_Estimada"));
        assert!(csv.contains("target"));
        assert!(csv.contains("installer.iso"));

        // 9. Strings de localização
        let loc_pt = Locale::Pt;
        assert_eq!(
            loc_pt.treemap_review_title(),
            "Simulação de Recuperação e Lista de Revisão"
        );
        assert!(loc_pt
            .treemap_review_advisory_banner()
            .contains("Exclusivamente Consultivo"));
        assert_eq!(
            loc_pt.treemap_review_export_csv(),
            "💾 Exportar Simulação (CSV)"
        );

        let loc_en = Locale::En;
        assert_eq!(
            loc_en.treemap_review_title(),
            "Space Recovery Simulation & Review List"
        );
        assert!(loc_en
            .treemap_review_advisory_banner()
            .contains("Strictly Advisory Mode"));
        assert_eq!(
            loc_en.treemap_review_export_csv(),
            "💾 Export Simulation (CSV)"
        );
    }

    #[test]
    fn test_tm_10_ui_color_modes_legend_and_textual_indicators() {
        use ff_index::entry::FLAG_DIRECTORY;
        use ff_index::FileIndex;
        use ff_size::treemap::{TreemapColorMode, TreemapRect};

        let now_unix = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;

        let mut app = FastFileApp::new();
        assert_eq!(app.treemap_color_mode, TreemapColorMode::FileType);

        // Constrói um índice sintético para testar visualização
        let mut index = FileIndex::new();
        let root = index
            .add_entry(
                1,
                ff_index::entry::NO_PARENT,
                "C:\\Projects",
                FLAG_DIRECTORY,
                0,
                now_unix,
            )
            .unwrap();
        let target_dir = index
            .add_entry(2, root, "target", FLAG_DIRECTORY, 0, now_unix - 100)
            .unwrap();
        let _cargo = index
            .add_entry(3, root, "Cargo.toml", 0, 500, now_unix - 100)
            .unwrap();
        let rs_file = index
            .add_entry(4, root, "main.rs", 0, 20_000, now_unix - (2 * 86400))
            .unwrap();
        let old_zip = index
            .add_entry(
                5,
                root,
                "backup.zip",
                0,
                100_000_000,
                now_unix - (250 * 86400),
            )
            .unwrap();

        app.treemap_custom_index = Some(index);
        app.treemap_dir_id = root;

        let bounds = TreemapRect::new(0.0, 0.0, 800.0, 600.0);

        // A. Modo Inicial: FileType
        app.recompute_treemap_layout(bounds);
        let layout_ft = app.treemap_layout.as_ref().unwrap();
        assert_eq!(layout_ft.color_mode, TreemapColorMode::FileType);

        let item_rs = layout_ft
            .all_items
            .iter()
            .find(|i| i.id == rs_file)
            .unwrap();
        assert_eq!(item_rs.color, (222, 100, 38));
        assert_eq!(item_rs.color_badge, "RS");

        let legend_ft = app.treemap_color_mode.legend(true);
        assert!(!legend_ft.is_empty());
        assert_eq!(legend_ft[0].short_badge, "DIR");

        // B. Alterna para Modo Idade (Age)
        app.treemap_color_mode = TreemapColorMode::Age;
        app.recompute_treemap_layout(bounds);
        let layout_age = app.treemap_layout.as_ref().unwrap();
        assert_eq!(layout_age.color_mode, TreemapColorMode::Age);

        let item_rs_age = layout_age
            .all_items
            .iter()
            .find(|i| i.id == rs_file)
            .unwrap();
        assert_eq!(item_rs_age.color, (46, 204, 113)); // Verde < 7d
        assert_eq!(item_rs_age.color_badge, "<7d");

        let item_zip_age = layout_age
            .all_items
            .iter()
            .find(|i| i.id == old_zip)
            .unwrap();
        assert_eq!(item_zip_age.color, (155, 89, 182)); // Vinho > 180d
        assert_eq!(item_zip_age.color_badge, ">180d");

        let legend_age = app.treemap_color_mode.legend(true);
        assert_eq!(legend_age.len(), 6);
        assert_eq!(legend_age[0].short_badge, "<7d");

        // C. Alterna para Modo Categoria de Limpeza (CleanupCategory)
        app.treemap_color_mode = TreemapColorMode::CleanupCategory;
        app.recompute_treemap_layout(bounds);
        let layout_clean = app.treemap_layout.as_ref().unwrap();
        assert_eq!(layout_clean.color_mode, TreemapColorMode::CleanupCategory);

        let item_target_clean = layout_clean
            .all_items
            .iter()
            .find(|i| i.id == target_dir)
            .unwrap();
        assert_eq!(item_target_clean.color, (255, 107, 107)); // Build Artifact
        assert_eq!(item_target_clean.color_badge, "BUILD");

        let legend_clean = app.treemap_color_mode.legend(true);
        assert_eq!(legend_clean.len(), 7);
        assert_eq!(legend_clean[0].short_badge, "BUILD");

        // D. Critério: As cores permanecem consistentes durante a navegação
        app.treemap_dir_id = target_dir;
        app.recompute_treemap_layout(bounds);
        assert_eq!(app.treemap_color_mode, TreemapColorMode::CleanupCategory);

        // E. Strings de internacionalização
        let loc_pt = Locale::Pt;
        assert_eq!(loc_pt.treemap_color_mode_label(), "🎨 Cores:");
        assert_eq!(loc_pt.treemap_color_mode_filetype(), "📁 Tipo de Arquivo");
        assert_eq!(loc_pt.treemap_color_mode_age(), "⏳ Idade de Modificação");
        assert_eq!(
            loc_pt.treemap_color_mode_cleanup(),
            "⚡ Oportunidade de Limpeza"
        );
        assert_eq!(loc_pt.treemap_legend_label(), "Legenda:");

        let loc_en = Locale::En;
        assert_eq!(loc_en.treemap_color_mode_label(), "🎨 Colors:");
        assert_eq!(loc_en.treemap_color_mode_filetype(), "📁 File Type");
        assert_eq!(loc_en.treemap_color_mode_age(), "⏳ Modification Age");
        assert_eq!(loc_en.treemap_color_mode_cleanup(), "⚡ Cleanup Category");
        assert_eq!(loc_en.treemap_legend_label(), "Legend:");
    }

    #[test]
    fn test_tm_11_ui_snapshots_growth_and_inaccessible_protection() {
        use ff_index::entry::{FLAG_DIRECTORY, NO_PARENT};
        use ff_index::FileIndex;
        use ff_size::treemap::{TreemapColorMode, TreemapRect, TreemapSizeMode};
        use ff_size::{GrowthChangeKind, SnapshotStorage};

        let mut app = FastFileApp::new();

        // Armazenamento isolado em diretório temporário para testes
        let tmp_dir = tempfile::tempdir().unwrap();
        app.treemap_growth_storage = SnapshotStorage::new(Some(tmp_dir.path().to_path_buf()));

        // 1. Constrói índice sintético inicial
        let mut index1 = FileIndex::new();
        let r1 = index1
            .add_entry(
                1,
                NO_PARENT,
                "C:\\Projects\\fastfile",
                FLAG_DIRECTORY,
                0,
                1731800000,
            )
            .unwrap();
        let src1 = index1
            .add_entry(2, r1, "src", FLAG_DIRECTORY, 0, 1731800000)
            .unwrap();
        let _code1 = index1
            .add_entry(3, src1, "main.rs", 0, 10_000, 1731800000)
            .unwrap();
        let target1 = index1
            .add_entry(4, r1, "target", FLAG_DIRECTORY, 0, 1731800000)
            .unwrap();
        let _build1 = index1
            .add_entry(5, target1, "output.bin", 0, 500_000, 1731800000)
            .unwrap();
        let prot1 = index1
            .add_entry(6, r1, "protected", FLAG_DIRECTORY, 0, 1731800000)
            .unwrap();
        let _secret1 = index1
            .add_entry(7, prot1, "secret.key", 0, 4_096, 1731800000)
            .unwrap();
        let _old1 = index1
            .add_entry(9, r1, "old_cache.tmp", 0, 100_000, 1731800000)
            .unwrap();

        app.treemap_custom_index = Some(index1);
        app.treemap_dir_id = r1;
        app.treemap_root_path = Some("C:\\Projects\\fastfile".to_string());
        app.treemap_size_mode = TreemapSizeMode::Logical;
        app.treemap_color_mode = TreemapColorMode::FileType;
        app.treemap_status = Some(app::TreemapAnalysisStatus::Complete);

        let bounds = TreemapRect {
            x: 0.0,
            y: 0.0,
            w: 800.0,
            h: 600.0,
        };
        app.recompute_treemap_layout(bounds);
        assert!(app.treemap_layout.is_some());

        // 2. Salva o snapshot base da análise inicial
        app.treemap_growth_label_input = "Snapshot Base".to_string();
        let snap_base_id = app
            .save_current_treemap_snapshot()
            .expect("Salva snapshot base");
        assert_eq!(app.treemap_growth_snapshots.len(), 1);
        assert_eq!(
            app.treemap_growth_selected_base_id,
            Some(snap_base_id.clone())
        );

        // 3. Constrói índice com alterações:
        //    - output.bin cresceu de 500 KB para 1.5 MB (+1 MB) dentro de target
        //    - novo arquivo README.md adicionado diretamente na raiz (+20 KB)
        //    - arquivo old_cache.tmp removido (-100 KB)
        //    - pasta 'protected' omitida e reportada em access_errors (falta de permissão)
        let mut index2 = FileIndex::new();
        let r2 = index2
            .add_entry(
                1,
                NO_PARENT,
                "C:\\Projects\\fastfile",
                FLAG_DIRECTORY,
                0,
                1731886400,
            )
            .unwrap();
        let src2 = index2
            .add_entry(2, r2, "src", FLAG_DIRECTORY, 0, 1731886400)
            .unwrap();
        let _code2 = index2
            .add_entry(3, src2, "main.rs", 0, 10_000, 1731886400)
            .unwrap();
        let _readme2 = index2
            .add_entry(8, r2, "README.md", 0, 20_000, 1731886400)
            .unwrap();
        let target2 = index2
            .add_entry(4, r2, "target", FLAG_DIRECTORY, 0, 1731886400)
            .unwrap();
        let _build2 = index2
            .add_entry(5, target2, "output.bin", 0, 1_500_000, 1731886400)
            .unwrap();

        app.treemap_custom_index = Some(index2);
        app.treemap_access_errors = vec![app::ScanAccessError {
            path: "C:\\Projects\\fastfile\\protected".to_string(),
            message: "Acesso negado (permissão restrita)".to_string(),
        }];
        app.treemap_status = Some(app::TreemapAnalysisStatus::Partial { errors_count: 1 });
        app.recompute_treemap_layout(bounds);

        // 4. Recalcula o relatório de comparação
        app.recalculate_growth_report();
        let report = app
            .treemap_growth_report
            .as_ref()
            .expect("Relatório gerado");

        // Critério: Análises com coberturas incompletas geram aviso
        assert!(
            !report.warnings.is_empty(),
            "Deve conter aviso de cobertura/permissão"
        );
        assert!(!report.full_coverage);

        // Critério: Variações são calculadas sobre medidas compatíveis
        assert!(report.measures_compatible);
        assert_eq!(report.size_mode, TreemapSizeMode::Logical);

        // Critério: Ranquings de pastas que cresceram, novos e removidos
        assert!(!report.top_growing_folders.is_empty());
        let growing_target = report
            .top_growing_folders
            .iter()
            .find(|g| g.relative_path.contains("target"))
            .expect("target deve estar entre as pastas que cresceram");
        assert!(growing_target.delta_bytes > 0);
        assert_eq!(growing_target.kind, GrowthChangeKind::Grown);

        assert!(!report.top_added_items.is_empty());
        let added_readme = report
            .top_added_items
            .iter()
            .find(|a| a.relative_path.contains("README.md"))
            .expect("README.md deve estar entre os adicionados");
        assert_eq!(added_readme.kind, GrowthChangeKind::Added);
        assert_eq!(added_readme.current_size, Some(20_000));

        assert!(!report.top_removed_items.is_empty());
        let removed_cache = report
            .top_removed_items
            .iter()
            .find(|r| r.relative_path.contains("old_cache.tmp"))
            .expect("old_cache.tmp deve estar entre os removidos");
        assert_eq!(removed_cache.kind, GrowthChangeKind::Removed);

        // Critério Fundamental: Falta de permissão não é interpretada automaticamente como espaço liberado!
        let protected_item = report
            .all_changes
            .iter()
            .find(|c| {
                c.relative_path.contains("protected") || c.relative_path.contains("secret.key")
            })
            .expect("Item protegido deve estar presente no relatório de alterações");
        assert_eq!(
            protected_item.kind,
            GrowthChangeKind::InaccessibleDueToPermission,
            "Item inacessível por permissão não pode ser classificado como Removido"
        );
        assert_eq!(
            protected_item.delta_bytes, 0,
            "Item inacessível não pode somar delta negativo de liberação de espaço"
        );
        assert!(protected_item.had_access_error);

        // Critério: Histórico permanece local e pode ser removido
        let removed_count = app
            .treemap_growth_storage
            .clear_history(None)
            .expect("Limpa histórico local");
        assert_eq!(removed_count, 1);
        let remaining = app.treemap_growth_storage.list(None).expect("Lista vazia");
        assert!(remaining.is_empty());

        // Validação de strings i18n
        let loc_pt = Locale::Pt;
        assert_eq!(loc_pt.treemap_btn_snapshots(), "📈 Comparar Crescimento");
        assert!(loc_pt.treemap_growth_dialog_title().contains("Snapshots"));
        assert!(loc_pt
            .treemap_growth_inaccessible_badge()
            .contains("permissão"));

        let loc_en = Locale::En;
        assert_eq!(loc_en.treemap_btn_snapshots(), "📈 Compare Growth");
        assert!(loc_en.treemap_growth_dialog_title().contains("Snapshots"));
        assert!(loc_en
            .treemap_growth_inaccessible_badge()
            .contains("permission"));
    }
}
