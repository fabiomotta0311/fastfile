//! Testes de regressão e validação para CORE-01:
//! Uniformização da resolução de caminhos entre CLI e daemon.
//!
//! Critérios de aceite:
//! - A mesma consulta local e por IPC retorna os mesmos itens para alvos relativos e absolutos.
//! - Consultar C:\foo não inclui C:\foobar.
//! - Testes cobrem espaços, Unicode, separadores e diretórios inexistentes.
//! - Alvo fora da cobertura do daemon produz resposta explícita ou fallback documentado.

use ff_cli::{resolve_client_target, CliError};
use ff_index::{clean_path, is_in_directory_tree, FileIndex, NO_PARENT};
use ff_indexer::{IpcRequest, IpcResponse, RequestHandler};
use std::fs;
use std::path::Path;
use std::sync::{Arc, Mutex, RwLock};

static CWD_LOCK: Mutex<()> = Mutex::new(());

fn fixture_root() -> &'static str {
    if cfg!(windows) {
        "C:"
    } else {
        "/volume"
    }
}
fn fixture_path(parts: &[&str]) -> String {
    let root = if cfg!(windows) { "C:\\" } else { "/volume" };
    parts
        .iter()
        .fold(std::path::PathBuf::from(root), |p, part| p.join(part))
        .to_string_lossy()
        .into_owned()
}

#[test]
fn test_core_01_resolve_client_target_dot_and_relative() {
    let _cwd_guard = CWD_LOCK.lock().unwrap();
    let cwd = std::env::current_dir().unwrap();
    let cleaned_cwd = clean_path(&cwd);

    // "." resolve para o diretório atual do cliente
    let res_dot = resolve_client_target(".").unwrap();
    assert_eq!(res_dot, cleaned_cwd);

    // "" (vazio) também resolve para o diretório atual
    let res_empty = resolve_client_target("").unwrap();
    assert_eq!(res_empty, cleaned_cwd);

    // Caminho relativo existente (ex: "Cargo.toml")
    if Path::new("Cargo.toml").exists() {
        let res_rel = resolve_client_target("Cargo.toml").unwrap();
        assert_eq!(res_rel, cleaned_cwd.join("Cargo.toml"));
    }
}

#[test]
fn test_core_01_resolve_client_target_nonexistent() {
    let non_existent = "pasta_totalmente_inexistente_12345_xyz";
    let err = resolve_client_target(non_existent).unwrap_err();
    match err {
        CliError::TargetNotFound(target) => {
            assert_eq!(target, non_existent);
        }
        _ => panic!("esperava TargetNotFound, obteve: {:?}", err),
    }
}

#[test]
fn test_core_01_foo_does_not_include_foobar_on_ipc_and_local() {
    let mut idx = FileIndex::new();
    idx.volume = fixture_root().to_string();

    // Monta hierarquia sintética:
    // C:\
    //   foo\ (dir, id 1)
    //     app.rs (file, id 2)
    //   foobar\ (dir, id 3)
    //     fake.rs (file, id 4)
    let d_root = idx
        .add_entry(0, NO_PARENT, fixture_root(), ff_index::FLAG_DIRECTORY, 0, 0)
        .unwrap();
    let d_foo = idx
        .add_entry(1, d_root, "foo", ff_index::FLAG_DIRECTORY, 0, 0)
        .unwrap();
    idx.add_entry(2, d_foo, "app.rs", 0, 100, 10).unwrap();

    let d_foobar = idx
        .add_entry(3, d_root, "foobar", ff_index::FLAG_DIRECTORY, 0, 0)
        .unwrap();
    idx.add_entry(4, d_foobar, "fake.rs", 0, 200, 20).unwrap();

    let handler = RequestHandler::new(Arc::new(RwLock::new(idx)));

    // Consulta IPC buscando dentro de C:\foo
    let req = IpcRequest::Find {
        pattern: "*.rs".to_string(),
        is_regex: false,
        fuzzy: false,
        limit: 50,
        target: Some(fixture_path(&["foo"])),
        extension: None,
        file_type: None,
        min_size: None,
        max_size: None,
    };

    let resp = handler.handle(req);
    match resp {
        IpcResponse::Ok(val) => {
            let matches = val["matches"].as_array().unwrap();
            // Deve conter apenas app.rs dentro de C:\foo
            assert_eq!(
                matches.len(),
                1,
                "C:\\foo não pode conter itens de C:\\foobar"
            );
            assert_eq!(matches[0]["name"], "app.rs");
            let path_str = matches[0]["path"].as_str().unwrap();
            assert!(path_str.contains("foo"));
            assert!(!path_str.contains("foobar"));
        }
        IpcResponse::Error(e) => panic!("esperava Ok: {e}"),
    }
}

#[test]
fn test_core_01_spaces_unicode_and_separators_on_ipc() {
    let mut idx = FileIndex::new();
    idx.volume = fixture_root().to_string();

    // Monta hierarquia com espaços, acentuação e cirílico:
    // C:\
    //   Meus Documentos\ (id 1)
    //     Café Expresso\ (id 2)
    //       relatório 2026.pdf (id 3)
    //   Проекты\ (id 4)
    //     быстрый файл\ (id 5)
    //       код.rs (id 6)
    let d_root = idx
        .add_entry(0, NO_PARENT, fixture_root(), ff_index::FLAG_DIRECTORY, 0, 0)
        .unwrap();

    let d_docs = idx
        .add_entry(1, d_root, "Meus Documentos", ff_index::FLAG_DIRECTORY, 0, 0)
        .unwrap();
    let d_cafe = idx
        .add_entry(2, d_docs, "Café Expresso", ff_index::FLAG_DIRECTORY, 0, 0)
        .unwrap();
    idx.add_entry(3, d_cafe, "relatório 2026.pdf", 0, 5000, 100)
        .unwrap();

    let d_proj = idx
        .add_entry(4, d_root, "Проекты", ff_index::FLAG_DIRECTORY, 0, 0)
        .unwrap();
    let d_fast = idx
        .add_entry(5, d_proj, "быстрый файл", ff_index::FLAG_DIRECTORY, 0, 0)
        .unwrap();
    idx.add_entry(6, d_fast, "код.rs", 0, 300, 200).unwrap();

    let handler = RequestHandler::new(Arc::new(RwLock::new(idx)));

    // 1. Busca com separadores misturados (forward slash) e acentuação
    let req_cafe = IpcRequest::Find {
        pattern: "*.pdf".to_string(),
        is_regex: false,
        fuzzy: false,
        limit: 10,
        target: Some(fixture_path(&["Meus Documentos", "Café Expresso"])),
        extension: None,
        file_type: None,
        min_size: None,
        max_size: None,
    };
    match handler.handle(req_cafe) {
        IpcResponse::Ok(val) => {
            assert_eq!(val["total_matched"], 1);
            assert_eq!(val["matches"][0]["name"], "relatório 2026.pdf");
        }
        IpcResponse::Error(e) => panic!("esperava Ok: {e}"),
    }

    // 2. Busca com caracteres cirílicos filtrando por arquivo *.rs
    let req_cyr = IpcRequest::Find {
        pattern: "*.rs".to_string(),
        is_regex: false,
        fuzzy: false,
        limit: 10,
        target: Some(fixture_path(&["Проекты", "быстрый файл"])),
        extension: None,
        file_type: Some("file".to_string()),
        min_size: None,
        max_size: None,
    };
    match handler.handle(req_cyr) {
        IpcResponse::Ok(val) => {
            assert_eq!(val["total_matched"], 1);
            assert_eq!(val["matches"][0]["name"], "код.rs");
        }
        IpcResponse::Error(e) => panic!("esperava Ok: {e}"),
    }
}

#[test]
fn test_core_01_out_of_volume_explicit_error() {
    let mut idx = FileIndex::new();
    idx.volume = fixture_root().to_string();

    let handler = RequestHandler::new(Arc::new(RwLock::new(idx)));

    // Consulta Find apontando para unidade D: (fora da cobertura de C:)
    let req_find = IpcRequest::Find {
        pattern: "*".to_string(),
        is_regex: false,
        fuzzy: false,
        limit: 10,
        target: Some(r"D:\dados\projeto".to_string()),
        extension: None,
        file_type: None,
        min_size: None,
        max_size: None,
    };
    match handler.handle(req_find) {
        IpcResponse::Error(msg) => {
            assert!(
                msg.contains("fora da cobertura do daemon"),
                "esperava mensagem explícita de cobertura, obteve: {msg}"
            );
        }
        IpcResponse::Ok(_) => panic!("deveria ter retornado erro para volume fora de cobertura"),
    }

    // Consulta Du apontando para unidade D:
    let req_du = IpcRequest::Du {
        target: r"D:\dados".to_string(),
        depth: Some(2),
        top: 10,
    };
    match handler.handle(req_du) {
        IpcResponse::Error(msg) => {
            assert!(
                msg.contains("fora da cobertura do daemon"),
                "esperava mensagem explícita de cobertura, obteve: {msg}"
            );
        }
        IpcResponse::Ok(_) => panic!("deveria ter retornado erro de volume"),
    }
}

#[test]
fn test_core_01_relative_vs_absolute_target_consistency_filesystem() {
    let _cwd_guard = CWD_LOCK.lock().unwrap();
    let temp_dir = std::env::temp_dir().join(format!("ff_core01_test_{}", std::process::id()));
    let _ = fs::remove_dir_all(&temp_dir);
    fs::create_dir_all(temp_dir.join("sub_alpha")).unwrap();
    fs::create_dir_all(temp_dir.join("sub_alpha_fake")).unwrap();
    // current_dir resolves symlinks (e.g. /var -> /private/var on macOS).
    let temp_dir = fs::canonicalize(temp_dir).unwrap();

    let file_real = temp_dir.join("sub_alpha").join("real.txt");
    let file_fake = temp_dir.join("sub_alpha_fake").join("fake.txt");
    fs::write(&file_real, b"conteudo real").unwrap();
    fs::write(&file_fake, b"conteudo fake").unwrap();

    // 1. Testa resolve_client_target com caminho relativo a partir do temp_dir
    let current_dir_backup = std::env::current_dir().unwrap();
    std::env::set_current_dir(&temp_dir).unwrap();

    let resolved_rel = resolve_client_target("sub_alpha").unwrap();
    let resolved_abs =
        resolve_client_target(&temp_dir.join("sub_alpha").to_string_lossy()).unwrap();

    // O caminho resolvido a partir do relativo DEVE ser idêntico ao resolvido a partir do absoluto
    assert_eq!(resolved_rel, resolved_abs);

    // Validação de pertinência à árvore de diretórios
    assert!(is_in_directory_tree(&file_real, &resolved_rel));
    assert!(!is_in_directory_tree(&file_fake, &resolved_rel));

    // Restaura o diretório de trabalho original
    std::env::set_current_dir(current_dir_backup).unwrap();
    let _ = fs::remove_dir_all(&temp_dir);
}
