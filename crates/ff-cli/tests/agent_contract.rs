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
