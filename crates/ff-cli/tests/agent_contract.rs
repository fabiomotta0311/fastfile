use ff_cli::{
    run, Cli, Commands, DiffArgs, DuArgs, FindArgs, GrepArgs, IndexArgs, RecentArgs, StatArgs,
};

#[test]
fn test_contract_du_json_output() {
    let cli = Cli {
        command: Commands::Du(DuArgs {
            target: ".".into(),
            top: 5,
            depth: Some(1),
            budget: None,
            relative: true,
            all: false,
            include_secrets: false,
            json: true,
            compact: false,
        }),
    };

    assert!(run(&cli).is_ok());
}

#[test]
fn test_contract_find_json_with_filters() {
    let cli = Cli {
        command: Commands::Find(FindArgs {
            pattern: Some("*.rs".into()),
            name: None,
            target: ".".into(),
            regex: false,
            fuzzy: false,
            ext: Some("rs".into()),
            size: Some(">10B".into()),
            file_type: Some("file".into()),
            since: None,
            limit: 10,
            cursor: None,
            budget: None,
            relative: true,
            all: false,
            include_secrets: false,
            json: true,
            compact: false,
            synthetic: None,
        }),
    };

    assert!(run(&cli).is_ok());
}

#[test]
fn test_contract_stat_json() {
    let cli = Cli {
        command: Commands::Stat(StatArgs {
            target: "Cargo.toml".into(),
            include_secrets: false,
            json: true,
            compact: false,
        }),
    };

    assert!(run(&cli).is_ok());
}

#[test]
fn test_contract_recent_json() {
    let cli = Cli {
        command: Commands::Recent(RecentArgs {
            target: ".".into(),
            since: "24h".into(),
            limit: 10,
            cursor: None,
            budget: None,
            relative: true,
            all: false,
            include_secrets: false,
            json: true,
            compact: false,
        }),
    };

    assert!(run(&cli).is_ok());
}

#[test]
fn test_contract_budget_truncation() {
    let mut items = vec![
        serde_json::json!({"path": "file1.txt", "size": 100}),
        serde_json::json!({"path": "file2.txt", "size": 200}),
        serde_json::json!({"path": "file3.txt", "size": 300}),
    ];

    // Budget de 10 tokens (~40 caracteres) só cabe 1 item
    let (truncated, count) = ff_cli::util::apply_budget(&mut items, Some(10));
    assert!(truncated);
    assert_eq!(count, Some(1));
    assert_eq!(items.len(), 1);
}

#[test]
fn test_contract_secret_filtering() {
    let secret_file = ".env";
    assert!(ff_cli::secrets::is_secret_file(secret_file));

    let normal_file = "Cargo.toml";
    assert!(!ff_cli::secrets::is_secret_file(normal_file));
}

#[test]
fn test_contract_grep_json() {
    let cli = Cli {
        command: Commands::Grep(GrepArgs {
            pattern: "workspace".into(),
            path: "Cargo.toml".into(),
            ext: None,
            file_type: None,
            mode: "content".into(),
            case_sensitive: false,
            limit: 10,
            budget: None,
            relative: true,
            all: false,
            include_secrets: false,
            no_index: false,
            build_index: false,
            json: true,
            compact: false,
        }),
    };

    assert!(run(&cli).is_ok());
}

#[test]
fn test_contract_diff_json() {
    let cli = Cli {
        command: Commands::Diff(DiffArgs {
            file_a: "Cargo.toml".into(),
            file_b: "Cargo.toml".into(),
            context: 3,
            budget: None,
            include_secrets: false,
            json: true,
            compact: false,
        }),
    };

    assert!(run(&cli).is_ok());
}

#[test]
fn test_contract_mcp_tools() {
    use ff_cli::mcp::{execute_mcp_tool, get_tool_definitions, handle_request, JsonRpcRequest};
    use serde_json::json;

    // Test tool definitions count
    let tools = get_tool_definitions();
    assert_eq!(tools.len(), 10);

    // Test initialize request
    let init_req = JsonRpcRequest {
        jsonrpc: "2.0".to_string(),
        id: Some(json!(1)),
        method: "initialize".to_string(),
        params: None,
    };
    let init_resp = handle_request(&init_req).expect("initialize should respond");
    assert_eq!(init_resp.jsonrpc, "2.0");
    assert_eq!(init_resp.id, Some(json!(1)));
    let server_info = init_resp.result.unwrap();
    assert_eq!(server_info["serverInfo"]["name"], "fastfile");

    // Test tools/call for disk_usage
    let du_res = execute_mcp_tool("disk_usage", &json!({ "target": ".", "top": 3 })).unwrap();
    assert_eq!(du_res["schema"], "ff/1");

    // Test tools/call for find_files
    let find_res = execute_mcp_tool(
        "find_files",
        &json!({ "pattern": "Cargo.toml", "target": "." }),
    )
    .unwrap();
    assert_eq!(find_res["schema"], "ff/1");

    // Test tools/call for file_stat
    let stat_res = execute_mcp_tool("file_stat", &json!({ "target": "Cargo.toml" })).unwrap();
    assert_eq!(stat_res["schema"], "ff/1");

    // Test tools/call for recent_changes
    let recent_res =
        execute_mcp_tool("recent_changes", &json!({ "target": ".", "limit": 5 })).unwrap();
    assert_eq!(recent_res["schema"], "ff/1");

    // Test tools/call for search_content
    let grep_res = execute_mcp_tool(
        "search_content",
        &json!({ "pattern": "workspace", "path": "Cargo.toml" }),
    )
    .unwrap();
    assert_eq!(grep_res["schema"], "ff/1");

    // Test tools/call for diff_files
    let diff_res = execute_mcp_tool(
        "diff_files",
        &json!({ "file_a": "Cargo.toml", "file_b": "Cargo.toml" }),
    )
    .unwrap();
    assert_eq!(diff_res["schema"], "ff/1");

    // Test tools/call for batch
    let batch_res = execute_mcp_tool(
        "batch",
        &json!({
            "queries": [
                { "type": "stat", "target": "Cargo.toml" },
                { "type": "find", "pattern": "*.toml", "target": "." },
                { "type": "sym", "name": "run_mcp_server" }
            ]
        }),
    )
    .unwrap();
    assert_eq!(batch_res["schema"], "ff/1");

    // Test tools/call for find_symbol
    let sym_res = execute_mcp_tool(
        "find_symbol",
        &json!({ "name": "run_mcp_server", "limit": 5 }),
    )
    .unwrap();
    assert_eq!(sym_res["schema"], "ff/1");

    // Test tools/call for repo_map
    let map_res = execute_mcp_tool(
        "repo_map",
        &json!({ "path": ".", "depth": 2, "budget": 1500 }),
    )
    .unwrap();
    assert_eq!(map_res["schema"], "ff/1");

    // Test tools/call for find_duplicates
    let dupes_res =
        execute_mcp_tool("find_duplicates", &json!({ "path": ".", "limit": 5 })).unwrap();
    assert_eq!(dupes_res["schema"], "ff/1");
}

#[test]
fn test_contract_sym_json() {
    let cli = Cli {
        command: Commands::Sym(ff_cli::SymArgs {
            name: "run_mcp_server".into(),
            path: ".".into(),
            kind: None,
            lang: Some("rust".into()),
            limit: 10,
            cursor: None,
            budget: None,
            relative: true,
            all: false,
            include_secrets: false,
            json: true,
            compact: false,
        }),
    };

    assert!(run(&cli).is_ok());
}

#[test]
fn test_contract_map_json() {
    let cli = Cli {
        command: Commands::Map(ff_cli::MapArgs {
            path: ".".into(),
            depth: 2,
            budget: Some(1500),
            all: false,
            include_secrets: false,
            json: true,
            compact: false,
        }),
    };

    assert!(run(&cli).is_ok());
}

#[test]
fn test_contract_find_fuzzy_json() {
    let cli = Cli {
        command: Commands::Find(FindArgs {
            pattern: Some("archmd".into()),
            name: None,
            target: ".".into(),
            regex: false,
            fuzzy: true,
            ext: None,
            size: None,
            file_type: None,
            since: None,
            limit: 5,
            cursor: None,
            budget: None,
            relative: true,
            all: false,
            include_secrets: false,
            json: true,
            compact: false,
            synthetic: None,
        }),
    };

    assert!(run(&cli).is_ok());
}

#[test]
fn test_contract_dupes_json() {
    let cli = Cli {
        command: Commands::Dupes(ff_cli::DupesArgs {
            path: ".".into(),
            min_size: "1B".into(),
            limit: 5,
            budget: None,
            relative: true,
            all: false,
            include_secrets: false,
            json: true,
            compact: false,
        }),
    };

    assert!(run(&cli).is_ok());
}

#[test]
fn test_contract_trigram_index_and_grep_json() {
    let tmp = std::env::temp_dir().join(format!("ff_contract_tri_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(tmp.join("subdir")).unwrap();

    std::fs::write(
        tmp.join("subdir/target.txt"),
        "IndexedTargetSymbol found here\n",
    )
    .unwrap();
    std::fs::write(
        tmp.join("subdir/other.txt"),
        "Another file without the target\n",
    )
    .unwrap();

    // 1. fastfile index <dir> --trigram --json
    let index_cli = Cli {
        command: Commands::Index(IndexArgs {
            target: tmp.to_string_lossy().to_string(),
            route: None,
            respect_ignore: true,
            trigram: true,
            max_file_size_mb: None,
            max_total_mb: None,
            json: true,
            compact: false,
        }),
    };
    assert!(run(&index_cli).is_ok());

    // 2. fastfile grep "IndexedTargetSymbol" <dir> --json
    let grep_cli = Cli {
        command: Commands::Grep(GrepArgs {
            pattern: "IndexedTargetSymbol".into(),
            path: tmp.to_string_lossy().to_string(),
            ext: None,
            file_type: None,
            mode: "content".into(),
            case_sensitive: false,
            limit: 10,
            budget: None,
            relative: true,
            all: false,
            include_secrets: false,
            no_index: false,
            build_index: false,
            json: true,
            compact: false,
        }),
    };
    assert!(run(&grep_cli).is_ok());

    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn test_contract_budget_one_envelope_behavior() {
    let cli = Cli {
        command: Commands::Find(FindArgs {
            pattern: Some("*.rs".into()),
            name: None,
            target: ".".into(),
            regex: false,
            fuzzy: false,
            ext: Some("rs".into()),
            size: None,
            file_type: None,
            since: None,
            limit: 50,
            cursor: None,
            budget: Some(1), // 1 token = 4 bytes
            relative: true,
            all: false,
            include_secrets: false,
            json: true,
            compact: true,
            synthetic: None,
        }),
    };

    assert!(run(&cli).is_ok());
}

#[test]
fn test_contract_compact_mode() {
    let cli = Cli {
        command: Commands::Find(FindArgs {
            pattern: Some("Cargo.toml".into()),
            name: None,
            target: ".".into(),
            regex: false,
            fuzzy: false,
            ext: None,
            size: None,
            file_type: None,
            since: None,
            limit: 5,
            cursor: None,
            budget: None,
            relative: true,
            all: false,
            include_secrets: false,
            json: false,
            compact: true, // --compact implies JSON
            synthetic: None,
        }),
    };

    assert!(run(&cli).is_ok());
}

#[test]
fn test_contract_file_result_item_name_fallback() {
    let item_with_name = ff_cli::schema::FileResultItem {
        path: "crates/ff-cli/src/lib.rs".into(),
        name: Some("lib.rs".into()),
        is_dir: false,
        size: 100,
        mtime: 1700000000,
        mtime_iso: None,
    };
    assert_eq!(item_with_name.name_or_basename(), "lib.rs");

    let item_without_name = ff_cli::schema::FileResultItem {
        path: "crates/ff-cli/src/lib.rs".into(),
        name: None,
        is_dir: false,
        size: 100,
        mtime: 1700000000,
        mtime_iso: None,
    };
    assert_eq!(item_without_name.name_or_basename(), "lib.rs");
}
