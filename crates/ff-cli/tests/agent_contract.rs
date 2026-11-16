use ff_cli::{run, Cli, Commands, DiffArgs, DuArgs, FindArgs, GrepArgs, RecentArgs, StatArgs};

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
            json: true,
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
    assert_eq!(tools.len(), 9);

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
        }),
    };

    assert!(run(&cli).is_ok());
}
