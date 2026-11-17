//! Testes de regressão e validação para CORE-04:
//! Evitar bloqueio da interface por IPC (Busca não-bloqueante, timeout, descarte por geração, fallback e reconexão).
//!
//! Critérios de aceite:
//! - Executar consultas fora da thread gráfica.
//! - Adicionar timeout, reconexão e estados de erro.
//! - Descartar respostas de pesquisas antigas quando houver uma consulta mais recente (generation counter).
//! - Definir fallback quando o daemon ficar indisponível.
//! - A interface permanece utilizável com daemon lento ou indisponível.
//! - Erro aparece de forma clara, em vez de parecer uma busca sem resultados.

use ff_index::{FileIndex, NO_PARENT};
use ff_indexer::{IpcClient, IpcRequest};
use ff_ui::app::{AsyncSearchResponse, FastFileApp, SearchResultItem};
use std::time::Duration;

#[test]
fn test_core_04_stale_response_discarded_by_generation() {
    let mut app = FastFileApp::new();
    app.local_index = None;
    app.is_ipc_connected = false;

    // Dispara a primeira busca: gera ID 1
    app.search_generation = 1;
    app.is_searching = true;

    // Usuário digita mais rápido e dispara a segunda busca: gera ID 2
    app.search_generation = 2;

    // Cria um canal sintético para injetar respostas simulando threads assíncronas concorrentes
    let (tx, rx) = std::sync::mpsc::channel();
    app.search_result_rx = Some(rx);

    // Resposta atrasada da busca antiga (geração 1)
    let stale_resp = AsyncSearchResponse {
        generation: 1,
        items: vec![SearchResultItem {
            id: 10,
            name: "arquivo_antigo_stale.txt".to_string(),
            path: "C:\\arquivo_antigo_stale.txt".to_string(),
            size: 100,
            mtime: 1000,
            is_dir: false,
        }],
        total_scanned: 100,
        latency_ms: 45.0,
        error: None,
        fallback_used: false,
        ipc_connected: true,
    };

    // Resposta recente da nova busca (geração 2)
    let fresh_resp = AsyncSearchResponse {
        generation: 2,
        items: vec![SearchResultItem {
            id: 20,
            name: "arquivo_recente_fresh.txt".to_string(),
            path: "C:\\arquivo_recente_fresh.txt".to_string(),
            size: 200,
            mtime: 2000,
            is_dir: false,
        }],
        total_scanned: 200,
        latency_ms: 12.0,
        error: None,
        fallback_used: false,
        ipc_connected: true,
    };

    // Envia primeiro a resposta antiga e depois a recente
    tx.send(stale_resp).unwrap();
    tx.send(fresh_resp).unwrap();

    // Simula o processamento do frame no loop da UI
    if let Some(ref rx) = app.search_result_rx {
        while let Ok(resp) = rx.try_recv() {
            if resp.generation == app.search_generation {
                app.is_searching = false;
                app.search_error = resp.error;
                let mut items = resp.items;
                app.apply_sorting(&mut items);
                app.results = items;
                app.total_indexed_files = resp.total_scanned;
                app.search_latency_ms = resp.latency_ms;
            }
        }
    }

    // Valida que apenas o resultado recente (geração 2) foi aplicado e o antigo foi descartado
    assert_eq!(
        app.results.len(),
        1,
        "Apenas uma resposta deve ser aplicada"
    );
    assert_eq!(
        app.results[0].name, "arquivo_recente_fresh.txt",
        "Resultado antigo (stale) deve ser descartado e substituído pelo mais recente"
    );
    assert_eq!(app.total_indexed_files, 200);
    assert!(!app.is_searching);
}

#[test]
fn test_core_04_ipc_client_send_timeout_when_daemon_unresponsive() {
    // Cria um cliente apontando para um endpoint inexistente ou que não responde
    let dummy_endpoint = if cfg!(windows) {
        r"\\.\pipe\fastfile-fake-timeout-pipe-test-core-04"
    } else {
        "/tmp/fastfile-fake-timeout-sock-test-core-04.sock"
    };

    let client = IpcClient::new(dummy_endpoint);

    let start = std::time::Instant::now();
    // Timeout estrito de 200ms
    let res = client.send_timeout(&IpcRequest::Ping, Duration::from_millis(200));

    let elapsed = start.elapsed();
    assert!(
        res.is_err(),
        "Endpoint inexistente ou sem resposta deve retornar erro"
    );
    // Verifica que o tempo respeitou o limite sem travar indefinidamente
    assert!(
        elapsed < Duration::from_millis(1500),
        "A chamada deve retornar rapidamente por erro ou timeout, sem travar a interface"
    );
}

#[test]
fn test_core_04_fallback_to_local_index_when_daemon_fails() {
    let mut app = FastFileApp::new();

    // Cria índice local com dados sintéticos
    let mut idx = FileIndex::new();
    let r = idx
        .add_entry(0, NO_PARENT, "C:", ff_index::FLAG_DIRECTORY, 0, 0)
        .unwrap();
    idx.add_entry(1, r, "arquivo_local_1.txt", 0, 512, 100)
        .unwrap();
    idx.add_entry(2, r, "arquivo_local_2.rs", 0, 1024, 200)
        .unwrap();

    app.local_index = Some(idx);
    app.is_ipc_connected = true; // Simula que acreditava estar conectado

    // Configura endpoint inexistente para forçar erro de comunicação IPC
    app.ipc_client = Some(IpcClient::new(
        "\\\\.\\pipe\\fastfile-unreachable-daemon-test-core-04",
    ));

    app.query = "arquivo_local_1".to_string();
    app.execute_search();

    // 1. Deve alternar is_ipc_connected para false após a falha
    assert!(
        !app.is_ipc_connected,
        "is_ipc_connected deve ser atualizado para false quando o daemon falhar"
    );

    // 2. Deve apresentar mensagem de erro/aviso clara de fallback
    assert!(
        app.search_error.is_some(),
        "search_error deve registrar o erro do daemon com o aviso de fallback"
    );
    let err_str = app.search_error.as_ref().unwrap();
    assert!(
        err_str.contains("Daemon indexador indisponível")
            || err_str.contains("Tempo limite esgotado"),
        "Mensagem de erro deve ser descritiva: {err_str}"
    );
    assert!(
        err_str.contains("Exibindo resultados do índice local"),
        "Mensagem deve informar claramente o fallback para o índice local: {err_str}"
    );

    // 3. Os resultados da busca local devem estar presentes (não vazio)
    assert_eq!(
        app.results.len(),
        1,
        "Fallback para índice local deve retornar o arquivo pesquisado"
    );
    assert_eq!(app.results[0].name, "arquivo_local_1.txt");
}

#[test]
fn test_core_04_error_state_explicit_when_no_local_index() {
    let mut app = FastFileApp::new();
    app.local_index = None;
    app.is_ipc_connected = true;
    app.ipc_client = Some(IpcClient::new(
        "\\\\.\\pipe\\fastfile-unreachable-daemon-test-no-local",
    ));

    app.query = "teste".to_string();
    app.execute_search();

    // Quando o daemon falha e não há índice local, a falha DEVE ser explícita
    assert!(!app.is_ipc_connected);
    assert!(
        app.search_error.is_some(),
        "Erro deve ser explícito em vez de parecer uma busca com 0 resultados"
    );
    let err_str = app.search_error.as_ref().unwrap();
    assert!(
        err_str.contains("Daemon indexador indisponível")
            || err_str.contains("Tempo limite esgotado"),
        "Erro deve explicar a indisponibilidade do daemon: {err_str}"
    );
    assert_eq!(
        app.results.len(),
        0,
        "Sem índice local, a lista de resultados fica vazia mas acompanhada de search_error explícito"
    );
}

#[test]
fn test_core_04_async_trigger_search_and_reconnection_flow() {
    let mut app = FastFileApp::new();
    let mut idx = FileIndex::new();
    let r = idx
        .add_entry(0, NO_PARENT, "C:", ff_index::FLAG_DIRECTORY, 0, 0)
        .unwrap();
    idx.add_entry(1, r, "async_target.txt", 0, 1024, 100)
        .unwrap();

    app.local_index = Some(idx);
    app.is_ipc_connected = false;

    // Dispara busca assíncrona fora da thread gráfica
    app.query = "async_target".to_string();
    let gen_before = app.search_generation;
    app.trigger_search();

    // Verifica incremento de geração e estado de busca
    assert_eq!(app.search_generation, gen_before + 1);
    assert!(app.is_searching, "is_searching deve ser ativado");
    assert!(
        app.search_result_rx.is_some(),
        "Canal receptor de busca deve estar configurado"
    );

    // Aguarda o término da thread de busca assíncrona com timeout
    let rx = app.search_result_rx.take().unwrap();
    let resp = rx
        .recv_timeout(Duration::from_secs(3))
        .expect("A busca assíncrona deve responder em menos de 3s");

    assert_eq!(resp.generation, app.search_generation);
    assert_eq!(resp.items.len(), 1);
    assert_eq!(resp.items[0].name, "async_target.txt");

    // Testa disparo da reconexão em background
    app.trigger_reconnect_ipc();
    assert!(
        app.reconnect_receiver.is_some(),
        "Canal de reconexão deve ser iniciado"
    );
}
