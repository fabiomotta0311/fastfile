//! Testes de regressão e validação para CORE-02:
//! Paginação contínua, totais e filtros do find por IPC vs Local.
//!
//! Critérios de aceite:
//! - Percorrer todas as páginas retorna todos os resultados autorizados, sem repetição ou omissão em um índice estável.
//! - Totais não representam apenas o lote truncado.
//! - Testes comparam execução local e IPC.
//! - Uniformização de --all, caminhos relativos, proteção de segredos e orçamento entre os backends.

use ff_index::{FileIndex, NO_PARENT};
use ff_indexer::{IpcRequest, IpcResponse, RequestHandler};
use std::collections::HashSet;
use std::sync::{Arc, RwLock};

#[test]
fn real_walk_and_ipc_filter_hidden_and_gitignored_before_pagination() {
    use ff_index::IndexSource;
    let dir = std::env::temp_dir().join(format!("ff-core02-privacy-{}", std::process::id()));
    std::fs::create_dir_all(dir.join(".git")).unwrap();
    std::fs::create_dir_all(dir.join(".private")).unwrap();
    for (name, content) in [
        (".gitignore", "ignored.rs\n"),
        ("visible.rs", "ok"),
        ("ignored.rs", "no"),
        (".hidden.rs", "no"),
        (".private/nested.rs", "no"),
    ] {
        std::fs::write(dir.join(name), content).unwrap();
    }
    let dir = dir.canonicalize().unwrap();
    let target = dir
        .to_str()
        .unwrap()
        .strip_prefix(r"\\?\")
        .unwrap_or(dir.to_str().unwrap());
    let mut idx = FileIndex::new();
    ff_index::WalkSource::new(target)
        .with_respect_ignore(false)
        .populate(&mut idx)
        .unwrap();
    idx.volume = if cfg!(windows) {
        target[..2].to_string()
    } else {
        target.to_string()
    };
    let handler = RequestHandler::new(Arc::new(RwLock::new(idx)));
    for all in [false, true] {
        let mut req = boundary_request(1, None, Some("file"));
        if let IpcRequest::Find {
            target: t,
            all: a,
            include_secrets,
            ..
        } = &mut req
        {
            *t = Some(target.to_string());
            *a = all;
            *include_secrets = true;
        }
        let IpcResponse::Ok(ipc) = handler.handle(req) else {
            panic!("IPC failed")
        };
        let mut cmd = std::process::Command::new(env!("CARGO_BIN_EXE_ff"));
        cmd.args([
            "find",
            "*.rs",
            "--target",
            target,
            "--type",
            "file",
            "--limit",
            "1",
            "--include-secrets",
            "--json",
        ]);
        if all {
            cmd.arg("--all");
        }
        let output = cmd.output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let local: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(local["source"], "walk");
        assert_eq!(local["summary"]["total_matches"], if all { 4 } else { 1 });
        if all {
            assert_eq!(local["summary"]["total_matches"], ipc["total_matched"]);
            assert_eq!(local["results"][0]["path"], ipc["matches"][0]["path"]);
        } else {
            // The CLI must not use this incomplete IPC filtering for its default mode.
            assert_eq!(ipc["total_matched"], 4);
            assert!(local["results"][0]["path"]
                .as_str()
                .unwrap()
                .ends_with("visible.rs"));
        }
    }
    std::fs::remove_dir_all(dir).unwrap();
}

fn boundary_request(limit: usize, cursor: Option<usize>, file_type: Option<&str>) -> IpcRequest {
    IpcRequest::Find {
        pattern: "*.rs".into(),
        is_regex: false,
        fuzzy: false,
        limit,
        cursor,
        target: Some(fixture_path(&[])),
        extension: None,
        file_type: file_type.map(str::to_string),
        min_size: None,
        max_size: None,
        min_mtime: None,
        all: true,
        include_secrets: false,
    }
}

fn boundary_fixture() -> RequestHandler {
    let mut idx = FileIndex::new();
    idx.volume = fixture_root().into();
    let root = idx
        .add_entry(0, NO_PARENT, fixture_root(), ff_index::FLAG_DIRECTORY, 0, 0)
        .unwrap();
    idx.add_entry(1, root, "a.rs", 0, 1, 1).unwrap();
    idx.add_entry(2, root, "b.rs", 0, 1, 2).unwrap();
    RequestHandler::new(Arc::new(RwLock::new(idx)))
}

#[test]
fn zero_limit_matches_local_pagination() {
    let handler = boundary_fixture();
    for cursor in [None, Some(1), Some(9)] {
        let IpcResponse::Ok(value) = handler.handle(boundary_request(0, cursor, None)) else {
            panic!("expected success")
        };
        assert_eq!(value["total_matched"], 2);
        assert_eq!(value["returned_matches"], 0);
        assert_eq!(value["matches"].as_array().unwrap().len(), 0);
        let expected = if cursor.unwrap_or(0) < 2 {
            Some(cursor.unwrap_or(0))
        } else {
            None
        };
        assert_eq!(value["next_cursor"].as_u64(), expected.map(|n| n as u64));
    }
}

#[test]
fn invalid_type_rejected_even_without_matches() {
    let handler = boundary_fixture();
    for invalid in ["bogus", "", " files "] {
        assert!(
            matches!(
                handler.handle(boundary_request(10, None, Some(invalid))),
                IpcResponse::Error(_)
            ),
            "type {invalid:?} must be rejected"
        );
    }
}

#[test]
fn local_cli_and_ipc_agree_on_zero_limit_and_invalid_type() {
    let dir = std::env::temp_dir().join(format!("ff-core02-boundary-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("a.rs"), "a").unwrap();
    std::fs::write(dir.join("b.rs"), "b").unwrap();
    let target = dir.to_str().unwrap();
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_ff"))
        .args(["find", "*.rs", "--target", target, "--limit", "0", "--json"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let local: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(local["source"], "walk");
    let IpcResponse::Ok(ipc) = boundary_fixture().handle(boundary_request(0, None, None)) else {
        panic!("expected success")
    };
    assert_eq!(local["summary"]["total_matches"], ipc["total_matched"]);
    assert_eq!(
        local["results"].as_array().unwrap().len(),
        ipc["returned_matches"].as_u64().unwrap() as usize
    );
    let invalid = std::process::Command::new(env!("CARGO_BIN_EXE_ff"))
        .args([
            "find", "*.rs", "--target", target, "--type", "bogus", "--json",
        ])
        .output()
        .unwrap();
    assert!(!invalid.status.success());
    assert!(matches!(
        boundary_fixture().handle(boundary_request(10, None, Some("bogus"))),
        IpcResponse::Error(_)
    ));
    std::fs::remove_dir_all(dir).unwrap();
}

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
fn test_core_02_pagination_and_totals_no_duplication_or_omission() {
    let mut idx = FileIndex::new();
    idx.volume = fixture_root().to_string();

    let d_root = idx
        .add_entry(0, NO_PARENT, fixture_root(), ff_index::FLAG_DIRECTORY, 0, 0)
        .unwrap();
    let d_src = idx
        .add_entry(1, d_root, "src", ff_index::FLAG_DIRECTORY, 0, 0)
        .unwrap();

    // Timestamps iguais exercitam o desempate entre requisições independentes.
    let total_files = 100usize;
    for i in (0..total_files).rev() {
        let name = format!("item_{i:02}.rs");
        idx.add_entry((10 + i) as u64, d_src, &name, 0, 100, 1000)
            .unwrap();
    }

    let handler = RequestHandler::new(Arc::new(RwLock::new(idx)));

    let page_size = 7usize;
    let mut current_cursor = Some(0usize);
    let mut collected_names = Vec::new();
    let mut page_count = 0usize;

    while let Some(offset) = current_cursor {
        page_count += 1;
        let req = IpcRequest::Find {
            pattern: "*.rs".to_string(),
            is_regex: false,
            fuzzy: false,
            limit: page_size,
            cursor: Some(offset),
            target: Some(fixture_path(&["src"])),
            extension: None,
            file_type: None,
            min_size: None,
            max_size: None,
            min_mtime: None,
            all: false,
            include_secrets: false,
        };

        match handler.handle(req) {
            IpcResponse::Ok(val) => {
                let total_matched = val["total_matched"].as_u64().unwrap() as usize;
                let returned_matches = val["returned_matches"].as_u64().unwrap() as usize;
                let cursor_reported = val["cursor"].as_u64().unwrap() as usize;

                // CRUCIAL: O total_matched NUNCA deve ser apenas o lote truncado (7)! Deve ser 25!
                assert_eq!(
                    total_matched, total_files,
                    "total_matched deve representar a contagem global, não o lote"
                );
                assert_eq!(cursor_reported, offset);

                let matches = val["matches"].as_array().unwrap();
                assert_eq!(matches.len(), returned_matches);

                for m in matches {
                    let name = m["name"].as_str().unwrap().to_string();
                    collected_names.push(name);
                }

                current_cursor = val["next_cursor"].as_u64().map(|c| c as usize);
            }
            IpcResponse::Error(e) => panic!("esperava Ok: {e}"),
        }
    }

    assert_eq!(page_count, 15);
    assert_eq!(collected_names.len(), total_files);

    // Sem repetição nem omissão
    let mut sorted_names = collected_names.clone();
    sorted_names.sort();
    assert_eq!(
        collected_names, sorted_names,
        "empates devem ordenar por caminho"
    );
    let unique_names: HashSet<_> = collected_names.iter().collect();
    assert_eq!(
        unique_names.len(),
        total_files,
        "nenhum item pode ser duplicado ou omitido ao percorrer todas as páginas"
    );
}

#[test]
fn test_core_02_all_flag_and_ignored_directories() {
    let mut idx = FileIndex::new();
    idx.volume = fixture_root().to_string();

    let d_root = idx
        .add_entry(0, NO_PARENT, fixture_root(), ff_index::FLAG_DIRECTORY, 0, 0)
        .unwrap();
    let d_proj = idx
        .add_entry(1, d_root, "project", ff_index::FLAG_DIRECTORY, 0, 0)
        .unwrap();

    // Arquivo normal
    idx.add_entry(2, d_proj, "lib.rs", 0, 100, 100).unwrap();

    // Arquivo em pasta target (junk/build)
    let d_target = idx
        .add_entry(3, d_proj, "target", ff_index::FLAG_DIRECTORY, 0, 0)
        .unwrap();
    idx.add_entry(4, d_target, "build_artifact.rs", 0, 500, 200)
        .unwrap();

    // Arquivo em pasta node_modules
    let d_node = idx
        .add_entry(5, d_proj, "node_modules", ff_index::FLAG_DIRECTORY, 0, 0)
        .unwrap();
    idx.add_entry(6, d_node, "package.rs", 0, 300, 300).unwrap();

    let handler = RequestHandler::new(Arc::new(RwLock::new(idx)));

    let mut req_target = boundary_request(10, None, None);
    if let IpcRequest::Find {
        ref mut target,
        ref mut all,
        ..
    } = req_target
    {
        *target = Some(fixture_path(&["project", "target"]));
        *all = false;
    }
    let IpcResponse::Ok(target_result) = handler.handle(req_target) else {
        panic!("expected success")
    };
    assert_eq!(target_result["total_matched"], 1);
    assert_eq!(target_result["matches"][0]["name"], "build_artifact.rs");

    // 1. Busca sem `--all` (all = false): pastas ignoradas devem ser omitidas
    let req_clean = IpcRequest::Find {
        pattern: "*.rs".to_string(),
        is_regex: false,
        fuzzy: false,
        limit: 10,
        cursor: None,
        target: Some(fixture_path(&["project"])),
        extension: None,
        file_type: None,
        min_size: None,
        max_size: None,
        min_mtime: None,
        all: false,
        include_secrets: false,
    };
    match handler.handle(req_clean) {
        IpcResponse::Ok(val) => {
            assert_eq!(val["total_matched"], 1);
            let matches = val["matches"].as_array().unwrap();
            assert_eq!(matches.len(), 1);
            assert_eq!(matches[0]["name"], "lib.rs");
        }
        IpcResponse::Error(e) => panic!("esperava Ok: {e}"),
    }

    // 2. Busca com `--all` (all = true): deve incluir arquivos sob target e node_modules
    let req_all = IpcRequest::Find {
        pattern: "*.rs".to_string(),
        is_regex: false,
        fuzzy: false,
        limit: 10,
        cursor: None,
        target: Some(fixture_path(&["project"])),
        extension: None,
        file_type: None,
        min_size: None,
        max_size: None,
        min_mtime: None,
        all: true,
        include_secrets: false,
    };
    match handler.handle(req_all) {
        IpcResponse::Ok(val) => {
            assert_eq!(val["total_matched"], 3);
            let matches = val["matches"].as_array().unwrap();
            assert_eq!(matches.len(), 3);
        }
        IpcResponse::Error(e) => panic!("esperava Ok: {e}"),
    }
}

#[test]
fn test_core_02_secrets_protection_and_reporting() {
    let mut idx = FileIndex::new();
    idx.volume = fixture_root().to_string();

    let d_root = idx
        .add_entry(0, NO_PARENT, fixture_root(), ff_index::FLAG_DIRECTORY, 0, 0)
        .unwrap();

    // Arquivos normais
    idx.add_entry(1, d_root, "main.rs", 0, 100, 100).unwrap();
    idx.add_entry(2, d_root, "config.json", 0, 200, 200)
        .unwrap();

    // Arquivos sensíveis
    idx.add_entry(3, d_root, ".env", 0, 50, 300).unwrap();
    idx.add_entry(4, d_root, "id_rsa", 0, 1500, 400).unwrap();
    idx.add_entry(5, d_root, "server.key", 0, 800, 500).unwrap();

    let handler = RequestHandler::new(Arc::new(RwLock::new(idx)));

    // 1. Busca sem `--include-secrets`: segredos devem ser filtrados e reportados em secrets_hidden
    let req_protected = IpcRequest::Find {
        pattern: "*".to_string(),
        is_regex: false,
        fuzzy: false,
        limit: 10,
        cursor: None,
        target: Some(fixture_path(&[])),
        extension: None,
        file_type: Some("file".to_string()),
        min_size: None,
        max_size: None,
        min_mtime: None,
        all: true,
        include_secrets: false,
    };
    match handler.handle(req_protected) {
        IpcResponse::Ok(val) => {
            assert_eq!(val["total_matched"], 2);
            assert_eq!(val["secrets_hidden"], 3);
            let matches = val["matches"].as_array().unwrap();
            let names: Vec<_> = matches
                .iter()
                .map(|m| m["name"].as_str().unwrap())
                .collect();
            assert!(names.contains(&"main.rs"));
            assert!(names.contains(&"config.json"));
            assert!(!names.contains(&".env"));
            assert!(!names.contains(&"id_rsa"));
            assert!(!names.contains(&"server.key"));
        }
        IpcResponse::Error(e) => panic!("esperava Ok: {e}"),
    }

    // 2. Busca com `--include-secrets`: segredos devem ser incluídos normalmente
    let req_unprotected = IpcRequest::Find {
        pattern: "*".to_string(),
        is_regex: false,
        fuzzy: false,
        limit: 10,
        cursor: None,
        target: Some(fixture_path(&[])),
        extension: None,
        file_type: Some("file".to_string()),
        min_size: None,
        max_size: None,
        min_mtime: None,
        all: true,
        include_secrets: true,
    };
    match handler.handle(req_unprotected) {
        IpcResponse::Ok(val) => {
            assert_eq!(val["total_matched"], 5);
            assert_eq!(val["secrets_hidden"], 0);
            let matches = val["matches"].as_array().unwrap();
            assert_eq!(matches.len(), 5);
        }
        IpcResponse::Error(e) => panic!("esperava Ok: {e}"),
    }
}

#[test]
fn test_core_02_compare_local_and_ipc_execution() {
    let mut idx = FileIndex::new();
    idx.volume = fixture_root().to_string();

    let d_root = idx
        .add_entry(0, NO_PARENT, fixture_root(), ff_index::FLAG_DIRECTORY, 0, 0)
        .unwrap();
    let d_crates = idx
        .add_entry(1, d_root, "crates", ff_index::FLAG_DIRECTORY, 0, 0)
        .unwrap();

    // 10 arquivos normais
    for i in 0..10 {
        let name = format!("modulo_{i}.rs");
        idx.add_entry(
            (10 + i) as u64,
            d_crates,
            &name,
            0,
            1000 + (i as u64 * 10),
            (2000 + i) as i64,
        )
        .unwrap();
    }

    // 1 arquivo de segredo
    idx.add_entry(99, d_crates, ".env.production", 0, 50, 2050)
        .unwrap();

    // 1 arquivo dentro de target (pasta de build)
    let d_tgt = idx
        .add_entry(100, d_crates, "target", ff_index::FLAG_DIRECTORY, 0, 0)
        .unwrap();
    idx.add_entry(101, d_tgt, "build.rs", 0, 80, 2060).unwrap();

    let handler = RequestHandler::new(Arc::new(RwLock::new(idx.clone())));

    // Consulta por IPC com paginação limit 5, all = false, include_secrets = false
    let req_ipc = IpcRequest::Find {
        pattern: "*.rs".to_string(),
        is_regex: false,
        fuzzy: false,
        limit: 5,
        cursor: Some(0),
        target: Some(fixture_path(&["crates"])),
        extension: None,
        file_type: Some("file".to_string()),
        min_size: None,
        max_size: None,
        min_mtime: None,
        all: false,
        include_secrets: false,
    };

    let ipc_resp = handler.handle(req_ipc);
    let (ipc_total, ipc_page_names) = match ipc_resp {
        IpcResponse::Ok(val) => {
            let total = val["total_matched"].as_u64().unwrap() as usize;
            let matches = val["matches"].as_array().unwrap();
            let names: Vec<String> = matches
                .iter()
                .map(|m| m["name"].as_str().unwrap().to_string())
                .collect();
            (total, names)
        }
        IpcResponse::Error(e) => panic!("esperava Ok: {e}"),
    };

    // Consulta Local executada diretamente
    let query = ff_search::Query::parse_with_options("*.rs", false, false).unwrap();
    let search_res = ff_search::SearchEngine::search(&idx, &query);

    let mut local_filtered = Vec::new();
    for m in &search_res.matches {
        let entry = &idx.entries[m.id as usize];
        if entry.is_deleted() || !entry.is_file() {
            continue;
        }
        let full_path = idx.build_path(m.id).unwrap();
        let name = idx.name(m.id).unwrap();

        if ff_index::has_ignored_component(std::path::Path::new(&full_path)) {
            continue;
        }
        if ff_index::is_secret_file(name) {
            continue;
        }
        local_filtered.push((name.to_string(), entry.mtime));
    }

    // Ordenação idêntica (mtime decrescente)
    local_filtered.sort_by_key(|(_, mtime)| std::cmp::Reverse(*mtime));

    let local_total = local_filtered.len();
    let local_page_names: Vec<String> =
        local_filtered.into_iter().take(5).map(|(n, _)| n).collect();

    // Verificação estrita de paridade Local vs IPC (CORE-02)
    assert_eq!(
        ipc_total, local_total,
        "totais de matches devem ser rigorosamente idênticos entre IPC e Local"
    );
    assert_eq!(
        ipc_page_names, local_page_names,
        "itens retornados e ordenação na página devem ser rigorosamente idênticos"
    );
}
