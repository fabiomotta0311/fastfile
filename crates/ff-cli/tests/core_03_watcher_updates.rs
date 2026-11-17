//! Testes de regressão e validação para CORE-03:
//! Atualizar corretamente tamanhos e recuperar o watcher.
//!
//! Critérios de aceite:
//! - Criar, aumentar, reduzir, renomear e excluir arquivos atualiza consultas e agregações.
//! - Falhas transitórias não encerram silenciosamente a atualização.
//! - Perda de continuidade provoca reconciliação ou reconstrução.
//! - Tratamento seguro de arquivos removidos entre notificação e leitura de metadados.
//! - Exposição de estado do watcher (WatcherStatus) via Status IPC.
//! - Testes exercitam arquivos sintéticos reais em disco, além das estruturas internas.

use ff_index::{FileIndex, IndexChange, NO_PARENT};
use ff_indexer::{IpcRequest, IpcResponse, RequestHandler, WatcherStatus};
use ff_size::SizeAggregator;
use std::fs::File;
use std::io::Write;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};

struct TestDirGuard {
    path: PathBuf,
}

impl TestDirGuard {
    fn new(prefix: &str) -> Self {
        let unique = format!(
            "fastfile_test_{prefix}_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        );
        let path = std::env::temp_dir().join(unique);
        std::fs::create_dir_all(&path).expect("criar pasta temporária");
        Self { path }
    }
}

impl Drop for TestDirGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

#[test]
fn test_core_03_real_synthetic_files_size_updates_and_aggregation() {
    let guard = TestDirGuard::new("size_agg");
    let base_path = &guard.path;

    // 1. Criação de arquivo sintético real no disco
    let file_path = base_path.join("data_test.bin");
    {
        let mut f = File::create(&file_path).expect("criar arquivo real");
        f.write_all(&vec![0xAA; 256]).expect("escrever 256 bytes");
        f.flush().expect("flush");
    }

    let initial_len = std::fs::metadata(&file_path).unwrap().len();
    assert_eq!(initial_len, 256);

    // Configura índice com o diretório e o arquivo
    let mut index = FileIndex::new();
    index.volume = "C:".to_string();

    let root_id = index
        .add_entry(100, NO_PARENT, "root", ff_index::FLAG_DIRECTORY, 0, 1000)
        .unwrap();

    let file_frn = 200u64;
    let _ = index
        .add_entry(file_frn, root_id, "data_test.bin", 0, initial_len, 1000)
        .unwrap();

    // Validação da agregação inicial
    let summary = SizeAggregator::aggregate(&index, Some(root_id), None, 10);
    assert_eq!(
        summary.total_size, 256,
        "Agregação inicial deve refletir 256 bytes"
    );

    // 2. Aumentar tamanho do arquivo real no disco (256 -> 4096 bytes)
    {
        let mut f = std::fs::OpenOptions::new()
            .append(true)
            .open(&file_path)
            .expect("abrir para append");
        f.write_all(&vec![0xBB; 3840])
            .expect("escrever mais 3840 bytes");
        f.flush().expect("flush");
    }
    let extended_len = std::fs::metadata(&file_path).unwrap().len();
    assert_eq!(extended_len, 4096);

    // Emite IndexChange::Modify com o novo tamanho real
    let change_extend = IndexChange::Modify {
        frn: file_frn,
        flags: 0,
        size: extended_len,
        mtime: 1100,
    };
    index
        .apply_change(&change_extend)
        .expect("aplicar Modify (aumento)");

    // Verifica se o índice e o agregador atualizaram
    let entry_id = index.id_by_frn(file_frn).expect("id por frn");
    assert_eq!(
        index.entry(entry_id).unwrap().size,
        4096,
        "Tamanho da entrada no índice deve ser 4096"
    );
    let agg_extended = SizeAggregator::aggregate(&index, Some(root_id), None, 10);
    assert_eq!(
        agg_extended.total_size, 4096,
        "Agregação deve refletir aumento para 4096 bytes"
    );

    // 3. Reduzir tamanho do arquivo real no disco (truncar para 64 bytes)
    {
        let f = File::create(&file_path).expect("recriar/truncar arquivo");
        f.set_len(64).expect("truncar para 64 bytes");
    }
    let truncated_len = std::fs::metadata(&file_path).unwrap().len();
    assert_eq!(truncated_len, 64);

    let change_truncate = IndexChange::Modify {
        frn: file_frn,
        flags: 0,
        size: truncated_len,
        mtime: 1200,
    };
    index
        .apply_change(&change_truncate)
        .expect("aplicar Modify (redução)");

    assert_eq!(index.entry(entry_id).unwrap().size, 64);
    let agg_truncated = SizeAggregator::aggregate(&index, Some(root_id), None, 10);
    assert_eq!(
        agg_truncated.total_size, 64,
        "Agregação deve refletir redução para 64 bytes"
    );

    // 4. Renomear o arquivo
    let change_rename = IndexChange::Rename {
        frn: file_frn,
        new_parent_frn: 100,
        new_name: "data_renamed.bin".to_string(),
    };
    index.apply_change(&change_rename).expect("aplicar Rename");
    assert_eq!(index.name(entry_id), Ok("data_renamed.bin"));

    // 5. Excluir o arquivo
    let _ = std::fs::remove_file(&file_path);
    let change_delete = IndexChange::Delete { frn: file_frn };
    index.apply_change(&change_delete).expect("aplicar Delete");

    assert!(index.id_by_frn(file_frn).is_none());
    let agg_deleted = SizeAggregator::aggregate(&index, Some(root_id), None, 10);
    assert_eq!(
        agg_deleted.total_size, 0,
        "Agregação deve zerar após exclusão"
    );
}

#[cfg(windows)]
#[test]
fn test_core_03_resolve_file_size_handles_deleted_before_stat() {
    let guard = TestDirGuard::new("usn_race");
    let base_path = &guard.path;
    let temp_file = base_path.join("ephemeral.tmp");

    let mut index = FileIndex::new();
    index.volume = base_path.to_string_lossy().to_string();

    let root_frn = 1000u64;
    let _ = index
        .add_entry(
            root_frn,
            NO_PARENT,
            &base_path.to_string_lossy(),
            ff_index::FLAG_DIRECTORY,
            0,
            0,
        )
        .unwrap();

    let file_frn = 2000u64;
    // Cria arquivo e verifica que resolve_file_size obtém o tamanho real
    {
        let mut f = File::create(&temp_file).unwrap();
        f.write_all(b"hello temporary file").unwrap();
    }
    let res_size = ff_index::windows::UsnWatcher::resolve_file_size(
        &index,
        file_frn,
        root_frn,
        "ephemeral.tmp",
    );
    assert_eq!(
        res_size.unwrap(),
        Some(20),
        "Deve resolver 20 bytes para arquivo existente"
    );

    // Agora remove o arquivo simulando corrida (excluído antes da leitura de metadados)
    std::fs::remove_file(&temp_file).unwrap();

    let res_size_deleted = ff_index::windows::UsnWatcher::resolve_file_size(
        &index,
        file_frn,
        root_frn,
        "ephemeral.tmp",
    );
    assert_eq!(
        res_size_deleted.unwrap(),
        None,
        "Deve retornar Ok(None) indicando remoção segura sem falhar o watcher"
    );
}

#[test]
fn test_core_03_watcher_status_reporting_in_ipc_status() {
    let index = FileIndex::new();
    let index_lock = Arc::new(RwLock::new(index));

    let watcher_status = Arc::new(RwLock::new(WatcherStatus::default()));
    let handler = RequestHandler::new(Arc::clone(&index_lock))
        .with_watcher_status(Arc::clone(&watcher_status));

    // Inicialmente default (inativo)
    let resp = handler.handle(IpcRequest::Status);
    if let IpcResponse::Ok(json_val) = resp {
        assert_eq!(json_val["watcher"]["active"], false);
        assert_eq!(json_val["watcher"]["state"], "stopped");
        assert_eq!(json_val["watcher"]["changes_applied"], 0);
        assert_eq!(json_val["watcher"]["reconciliations"], 0);
    } else {
        panic!("Esperado IpcResponse::Ok");
    }

    // Simula transição para saudável com alterações aplicadas
    {
        let mut st = watcher_status.write().unwrap();
        st.active = true;
        st.state = "healthy".to_string();
        st.changes_applied = 42;
        st.last_sync_unix = Some(1700000000);
        st.reconciliations = 0;
        st.last_error = None;
    }

    let resp2 = handler.handle(IpcRequest::Status);
    if let IpcResponse::Ok(json_val) = resp2 {
        assert_eq!(json_val["watcher"]["active"], true);
        assert_eq!(json_val["watcher"]["state"], "healthy");
        assert_eq!(json_val["watcher"]["changes_applied"], 42);
        assert_eq!(json_val["watcher"]["last_sync_unix"], 1700000000);
        assert!(json_val["watcher"]["last_error"].is_null());
    } else {
        panic!("Esperado IpcResponse::Ok");
    }

    // Simula degradação por falha transitória (sem encerrar o processo)
    {
        let mut st = watcher_status.write().unwrap();
        st.state = "degraded".to_string();
        st.last_error = Some("Erro transitório de leitura".to_string());
    }

    let resp3 = handler.handle(IpcRequest::Status);
    if let IpcResponse::Ok(json_val) = resp3 {
        assert_eq!(json_val["watcher"]["state"], "degraded");
        assert_eq!(
            json_val["watcher"]["last_error"],
            "Erro transitório de leitura"
        );
    } else {
        panic!("Esperado IpcResponse::Ok");
    }

    // Simula perda de continuidade do journal -> reconciliação / reconstrução
    {
        let mut st = watcher_status.write().unwrap();
        st.state = "reconciling".to_string();
        st.reconciliations += 1;
        st.last_error = Some("perda de continuidade do journal: journal entry deleted".to_string());
    }

    let resp4 = handler.handle(IpcRequest::Status);
    if let IpcResponse::Ok(json_val) = resp4 {
        assert_eq!(json_val["watcher"]["state"], "reconciling");
        assert_eq!(json_val["watcher"]["reconciliations"], 1);
    } else {
        panic!("Esperado IpcResponse::Ok");
    }
}

#[test]
fn test_core_03_reconciliation_swaps_index_seamlessly() {
    let mut initial_index = FileIndex::new();
    initial_index.volume = "C:".to_string();
    let r0 = initial_index
        .add_entry(0, NO_PARENT, "C:", ff_index::FLAG_DIRECTORY, 0, 0)
        .unwrap();
    initial_index
        .add_entry(1, r0, "stale.txt", 0, 50, 1000)
        .unwrap();

    let shared_index = Arc::new(RwLock::new(initial_index));
    let watcher_status = Arc::new(RwLock::new(WatcherStatus::default()));
    let handler = RequestHandler::new(Arc::clone(&shared_index))
        .with_watcher_status(Arc::clone(&watcher_status));

    // Busca antes da reconciliação encontra stale.txt
    let req1 = IpcRequest::Find {
        pattern: "stale.txt".to_string(),
        is_regex: false,
        fuzzy: false,
        limit: 10,
        cursor: None,
        target: None,
        extension: None,
        file_type: None,
        min_size: None,
        max_size: None,
        min_mtime: None,
        all: false,
        include_secrets: false,
    };
    if let IpcResponse::Ok(json_val) = handler.handle(req1) {
        let matches = json_val["matches"].as_array().expect("matches array");
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0]["name"], "stale.txt");
    } else {
        panic!("Esperado IpcResponse::Ok");
    }

    // Simula reconciliação reconstruindo o índice completo
    {
        let mut new_index = FileIndex::new();
        new_index.volume = "C:".to_string();
        let new_root = new_index
            .add_entry(0, NO_PARENT, "C:", ff_index::FLAG_DIRECTORY, 0, 0)
            .unwrap();
        new_index
            .add_entry(2, new_root, "fresh_rebuilt.txt", 0, 1024, 2000)
            .unwrap();

        // Faz o swap atômico no lock compartilhado e atualiza status
        let mut lock = shared_index.write().unwrap();
        *lock = new_index;

        let mut st = watcher_status.write().unwrap();
        st.state = "healthy".to_string();
        st.reconciliations += 1;
    }

    // Consultas seguintes no handler já usam o índice reconciliado
    let req2 = IpcRequest::Find {
        pattern: "fresh_rebuilt.txt".to_string(),
        is_regex: false,
        fuzzy: false,
        limit: 10,
        cursor: None,
        target: None,
        extension: None,
        file_type: None,
        min_size: None,
        max_size: None,
        min_mtime: None,
        all: false,
        include_secrets: false,
    };
    if let IpcResponse::Ok(json_val) = handler.handle(req2) {
        let matches = json_val["matches"].as_array().expect("matches array");
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0]["name"], "fresh_rebuilt.txt");
        assert_eq!(matches[0]["size"], 1024);
    } else {
        panic!("Esperado IpcResponse::Ok");
    }

    // stale.txt não existe mais
    let req3 = IpcRequest::Find {
        pattern: "stale.txt".to_string(),
        is_regex: false,
        fuzzy: false,
        limit: 10,
        cursor: None,
        target: None,
        extension: None,
        file_type: None,
        min_size: None,
        max_size: None,
        min_mtime: None,
        all: false,
        include_secrets: false,
    };
    if let IpcResponse::Ok(json_val) = handler.handle(req3) {
        let matches = json_val["matches"].as_array().expect("matches array");
        assert_eq!(matches.len(), 0);
    } else {
        panic!("Esperado IpcResponse::Ok");
    }
}
