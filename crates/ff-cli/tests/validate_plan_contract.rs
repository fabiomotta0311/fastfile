//! Contrato do plano de validação experimental (`ff/validate-plan/experimental-2`).
//!
//! Cobre as três entradas do CLI, a paridade CLI/MCP, o truncamento por orçamento e os fallbacks
//! conservadores: nada de seleção estreita com cobertura falsamente completa, e nenhuma sugestão
//! executada pelo FastFile.

use serde_json::{json, Value};
use std::fs;
use std::io::{Read, Write};
use std::path::PathBuf;
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};

struct Repo(PathBuf);
impl Repo {
    fn new() -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let dir = std::env::temp_dir().join(format!(
            "ff-plan-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&dir).unwrap();
        let repo = Self(dir);
        repo.git(&["init", "-q"]);
        repo.git(&["config", "user.name", "FastFile Fixture"]);
        repo.git(&["config", "user.email", "fixture@example.invalid"]);
        repo.git(&["config", "core.autocrlf", "false"]);
        repo
    }
    fn git(&self, args: &[&str]) {
        let out = Command::new("git")
            .current_dir(&self.0)
            .args(args)
            .output()
            .unwrap();
        assert!(out.status.success(), "{args:?}: {out:?}");
    }
    fn write(&self, path: &str, content: &str) {
        let path = self.0.join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, content).unwrap();
    }
    fn commit(&self) {
        self.git(&["add", "."]);
        self.git(&["commit", "-qm", "fixture"]);
    }
    fn run(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_ff"))
            .current_dir(&self.0)
            .arg("validate-plan")
            .args(args)
            .output()
            .unwrap()
    }
    /// Texto exato do plano no CLI (bytes que a superfície CLI entrega).
    fn plan_text(&self, args: &[&str]) -> String {
        let mut options = vec!["--experimental"];
        options.extend_from_slice(args);
        let out = self.run(&options);
        assert!(out.status.success(), "{out:?}");
        String::from_utf8(out.stdout)
            .unwrap()
            .trim_end()
            .to_string()
    }
    fn plan(&self, args: &[&str]) -> Value {
        let plan: Value = serde_json::from_str(&self.plan_text(args)).unwrap();
        assert_no_execution(&plan);
        plan
    }
    /// Sessão MCP real sobre stdio, com o servidor rodando na raiz do fixture.
    fn mcp_session(&self, requests: &[Value]) -> Vec<Value> {
        let mut child = Command::new(env!("CARGO_BIN_EXE_ff"))
            .current_dir(&self.0)
            .arg("mcp")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let mut stdin = child.stdin.take().unwrap();
        for request in requests {
            writeln!(stdin, "{request}").unwrap();
        }
        stdin.flush().unwrap();
        drop(stdin);
        let mut text = String::new();
        child
            .stdout
            .take()
            .unwrap()
            .read_to_string(&mut text)
            .unwrap();
        assert!(child.wait().unwrap().success());
        text.lines()
            .filter(|line| !line.trim().is_empty())
            .map(|line| serde_json::from_str(line).unwrap())
            .collect()
    }
    /// Texto exato do plano devolvido pela ferramenta MCP.
    fn mcp_text(&self, arguments: Value) -> String {
        let responses = self.mcp_session(&[json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "tools/call",
            "params": { "name": "validate_plan", "arguments": arguments }
        })]);
        let result = &responses[0]["result"];
        assert_eq!(result["isError"], false, "{result}");
        result["content"][0]["text"].as_str().unwrap().to_string()
    }
    fn mcp_call(&self, arguments: Value) -> Value {
        let plan: Value =
            serde_json::from_str(&self.mcp_text(arguments)).expect("plano MCP é JSON");
        assert_no_execution(&plan);
        plan
    }
    fn mcp_error(&self, arguments: Value) -> Value {
        let responses = self.mcp_session(&[json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "tools/call",
            "params": { "name": "validate_plan", "arguments": arguments }
        })]);
        let result = &responses[0]["result"];
        assert_eq!(result["isError"], true, "{result}");
        serde_json::from_str(result["content"][0]["text"].as_str().unwrap()).unwrap()
    }
}
impl Drop for Repo {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// O FastFile sugere; quem executa é o harness, e a evidência de validação só existe depois disso.
fn assert_no_execution(plan: &Value) {
    assert_eq!(plan["executed"], false);
    assert_eq!(plan["evidence_status"], "not_executed");
    let text = plan.to_string();
    assert!(
        !text.contains("executed_at") && !text.contains("passed") && !text.contains("failed"),
        "plano não pode carregar resultado de execução: {plan}"
    );
}

fn reasons(plan: &Value) -> Vec<String> {
    plan["broadening_reasons"]
        .as_array()
        .unwrap()
        .iter()
        .map(|reason| reason.as_str().unwrap().to_string())
        .collect()
}

/// A suíte ampla que acompanha a primeira checagem quando a cobertura está completa.
fn broader(plan: &Value) -> Value {
    plan["additional"]
        .as_array()
        .unwrap()
        .iter()
        .find(|suggestion| suggestion["scope"] == "workspace")
        .expect("sugestão de workspace")
        .clone()
}

fn sample_crate(repo: &Repo) {
    repo.write(
        "Cargo.toml",
        "[package]\nname = 'sample'\nversion = '0.1.0'\n",
    );
    repo.write("src/lib.rs", "pub fn answer() -> u8 {\n    1\n}\n");
    repo.write(
        "tests/check.rs",
        "#[test]\nfn checks_answer() { assert_eq!(sample::answer(), 3); }\n",
    );
}

#[test]
fn separates_layers_and_finds_test_in_another_module() {
    let repo = Repo::new();
    sample_crate(&repo);
    repo.write(".gitignore", "ignored.rs\n");
    repo.commit();
    assert!(!repo.run(&[]).status.success());
    repo.write("src/lib.rs", "pub fn answer() -> u8 {\n    2\n}\n");
    repo.git(&["add", "src/lib.rs"]);
    repo.write("src/lib.rs", "pub fn answer() -> u8 {\n    3\n}\n");
    repo.write("new.rs", "fn local_file() {}\n");
    repo.write("ignored.rs", "fn ignored() {}\n");
    repo.write(".env", "SESSION=sensitive_marker\n");
    let plan = repo.plan(&["--path", "src"]);
    let changes = plan["changes"].as_array().unwrap();
    assert_eq!(changes.len(), 2, "{plan}");
    assert_eq!(changes[0]["layer"], "staged");
    assert_eq!(changes[1]["layer"], "unstaged");
    assert_eq!(changes[1]["symbols"][0], "answer");
    assert_eq!(plan["coverage"]["complete"], true);
    assert_eq!(plan["truncated"], false);
    assert_eq!(plan["changes_omitted"], 0);
    assert!(reasons(&plan).is_empty(), "{plan}");
    assert_eq!(plan["first_check"]["scope"], "first_test");
    assert_eq!(
        plan["first_check"]["argv"],
        json!([
            "cargo",
            "test",
            "-p",
            "sample",
            "--test",
            "check",
            "checks_answer",
            "--locked"
        ])
    );
    assert_eq!(plan["first_check"]["evidence"][0]["path"], "tests/check.rs");
    assert_eq!(plan["first_check"]["evidence"][0]["line"], 2);
    assert_eq!(
        plan["first_check"]["evidence"][0]["source"],
        "test_symbol_reference_heuristic"
    );
    assert_eq!(plan["additional"][0]["scope"], "package");
    assert_eq!(plan["additional"][1]["scope"], "workspace");
    let full = repo.plan(&["--base", "HEAD"]);
    assert_eq!(full["changes"].as_array().unwrap().len(), 3, "{full}");
    assert_eq!(full["changes"][2]["layer"], "untracked");
    assert!(!full.to_string().contains("sensitive_marker"));
    assert!(!full.to_string().contains(".env"));
    assert!(!full.to_string().contains("ignored.rs"));
}

#[test]
fn rename_removal_dependents_and_config_fallback() {
    let repo = Repo::new();
    repo.write("Cargo.toml", "[workspace]\nmembers = ['core', 'app']\n");
    repo.write(
        "core/Cargo.toml",
        "[package]\nname = 'sample-core'\nversion = '0.1.0'\n",
    );
    repo.write("core/src/lib.rs", "mod removed;\n");
    repo.write("core/src/removed.rs", "fn gone() {}\n");
    repo.write("core/src/old.rs", "fn moved() {}\n");
    repo.write("app/Cargo.toml", "[package]\nname = 'sample-app'\nversion = '0.1.0'\n[dependencies]\nalias = { path = '../core', package = 'sample-core' }\n");
    repo.write("app/src/lib.rs", "pub fn demo() {}\n");
    repo.write(".github/workflows/ci.yml", "jobs:\n  checks:\n    steps:\n      - run: cargo test --workspace --locked\n      - run: echo never-run-this\n");
    repo.commit();
    repo.git(&["mv", "core/src/old.rs", "core/src/new.rs"]);
    repo.git(&["rm", "core/src/removed.rs"]);
    let plan = repo.plan(&[]);
    let changes = plan["changes"].as_array().unwrap();
    assert!(
        changes.iter().any(|c| c["status"] == "renamed"
            && c["old_path"] == "core/src/old.rs"
            && c["path"] == "core/src/new.rs"),
        "{plan}"
    );
    assert!(changes
        .iter()
        .any(|c| c["status"] == "deleted" && c["path"] == "core/src/removed.rs"));
    assert_eq!(
        plan["impacted_packages"],
        json!(["sample-app", "sample-core"])
    );
    assert_eq!(plan["coverage"]["complete"], true);
    assert!(reasons(&plan).contains(&"removed_or_renamed_path".to_string()));
    assert_eq!(
        plan["first_check"]["argv"],
        json!(["cargo", "test", "--workspace", "--locked"])
    );
    assert_eq!(
        plan["first_check"]["evidence"][0]["path"],
        ".github/workflows/ci.yml"
    );
    assert_eq!(
        plan["first_check"]["evidence"][0]["source"],
        "known_ci_test_suite"
    );
    assert_eq!(plan["additional"], json!([]));
    assert!(!plan.to_string().contains("never-run-this"));
    repo.write(".cargo/config.toml", "[build]\nincremental = false\n");
    let fallback = repo.plan(&[]);
    assert_eq!(
        fallback["first_check"]["argv"],
        json!(["cargo", "test", "--workspace", "--locked"])
    );
    assert!(reasons(&fallback).contains(&"configuration_or_non_rust_change".to_string()));
    assert!(!fallback.to_string().contains("never-run-this"));
}

#[test]
fn explicit_base_keeps_committed_and_worktree_changes_distinct() {
    let repo = Repo::new();
    sample_crate(&repo);
    repo.commit();
    repo.git(&["tag", "validation-base"]);
    repo.write("src/lib.rs", "pub fn committed() {}\n");
    repo.commit();
    repo.write("src/lib.rs", "pub fn working() {}\n");
    let plan = repo.plan(&["--base", "validation-base"]);
    assert_eq!(plan["coverage"]["base_to_index"], "base_to_index");
    assert_eq!(plan["changes"][0]["layer"], "base_to_index");
    assert_eq!(plan["changes"][1]["layer"], "unstaged");
    let default = repo.plan(&[]);
    assert_ne!(plan["base_commit"], default["base_commit"]);
    assert_eq!(default["changes"].as_array().unwrap().len(), 1);
    assert!(!repo
        .run(&["--experimental", "--path", "../outside"])
        .status
        .success());
    assert!(!repo
        .run(&["--experimental", "--base", "HEAD~1"])
        .status
        .success());
    assert!(!repo.run(&["--path", "src"]).status.success());
}

#[test]
fn configured_git_helpers_are_not_called_and_the_index_is_unchanged() {
    let repo = Repo::new();
    sample_crate(&repo);
    repo.write(".gitattributes", "*.rs diff=fixture filter=fixture\n");
    repo.commit();
    for key in [
        "core.fsmonitor",
        "diff.fixture.command",
        "diff.fixture.textconv",
        "filter.fixture.clean",
    ] {
        repo.git(&["config", key, "echo unexpected > planner-helper-marker"]);
    }
    repo.write("src/lib.rs", "pub fn changed() {}\n");
    let index = repo.0.join(".git/index");
    let before = fs::read(&index).unwrap();
    let plan = repo.plan(&[]);
    assert!(
        plan["changes"]
            .as_array()
            .unwrap()
            .iter()
            .any(|change| change["layer"] == "unstaged"
                && change["symbols"]
                    .as_array()
                    .unwrap()
                    .contains(&Value::from("changed"))),
        "{plan}"
    );
    assert_eq!(fs::read(index).unwrap(), before);
    assert!(!repo.0.join("planner-helper-marker").exists());
}

#[test]
fn oversized_worktree_file_is_not_reported_as_clean() {
    let repo = Repo::new();
    sample_crate(&repo);
    repo.commit();
    repo.write("src/lib.rs", &"x".repeat(130 * 1024));
    let plan = repo.plan(&[]);
    assert_eq!(plan["coverage"]["complete"], false);
    assert_eq!(reasons(&plan), vec!["partial_diff".to_string()]);
    assert_eq!(plan["first_check"]["scope"], "workspace");
    assert_eq!(plan["additional"], json!([]));
}

#[test]
fn mcp_returns_the_same_plan_as_the_cli_and_executes_nothing() {
    let repo = Repo::new();
    sample_crate(&repo);
    repo.commit();
    repo.write("src/lib.rs", "pub fn answer() -> u8 {\n    3\n}\n");
    let cli = repo.plan(&[]);
    let mcp = repo.mcp_call(json!({ "experimental": true }));
    // A ordem das chaves muda (o MCP serializa a partir de `serde_json::Value`), o conteúdo não.
    assert_eq!(cli, mcp, "CLI e MCP precisam entregar o mesmo plano");
    assert_eq!(mcp["schema"], "ff/validate-plan/experimental-2");
    assert_eq!(mcp["first_check"]["scope"], "first_test");
    assert_eq!(mcp["truncated"], false);
    // Sem orçamento explícito também há paridade.
    assert_eq!(
        cli,
        repo.mcp_call(json!({ "experimental": true, "path": "." }))
    );
    let base_cli = repo.plan(&["--base", "HEAD"]);
    assert_eq!(
        base_cli,
        repo.mcp_call(json!({ "experimental": true, "base": "HEAD" }))
    );
}

#[test]
fn mcp_advertises_the_experimental_tool_as_read_only() {
    let repo = Repo::new();
    sample_crate(&repo);
    repo.write("src/lib.rs", "pub fn answer() -> u8 {\n    2\n}\n");
    let responses = repo.mcp_session(&[
        json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {} }),
        json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/list" }),
    ]);
    let tools = responses[1]["result"]["tools"].as_array().unwrap();
    let tool = tools
        .iter()
        .find(|tool| tool["name"] == "validate_plan")
        .expect("validate_plan precisa estar em tools/list");
    assert_eq!(tool["annotations"]["readOnlyHint"], true);
    assert_eq!(tool["annotations"]["destructiveHint"], false);
    assert_eq!(tool["annotations"]["openWorldHint"], false);
    assert_eq!(tool["inputSchema"]["required"], json!(["experimental"]));
    assert!(tool["description"]
        .as_str()
        .unwrap()
        .contains("Experimental"));
}

#[test]
fn truncated_budget_broadens_the_check_instead_of_narrowing_it() {
    let repo = Repo::new();
    sample_crate(&repo);
    repo.commit();
    repo.write("src/lib.rs", "pub fn answer() -> u8 {\n    3\n}\n");
    let full = repo.mcp_call(json!({ "experimental": true }));
    assert_eq!(full["first_check"]["scope"], "first_test");
    assert_eq!(full["changes"].as_array().unwrap().len(), 1);

    // Um orçamento que não cabe o plano completo, mas ainda cabe o plano amplo.
    let budget = serde_json::to_string(&full).unwrap().len() / 4 - 1;
    let truncated = repo.mcp_call(json!({ "experimental": true, "budget": budget }));
    assert_eq!(truncated["truncated"], true, "{truncated}");
    assert_eq!(truncated["coverage"]["complete"], false);
    assert!(reasons(&truncated).contains(&"budget_truncated".to_string()));
    assert_eq!(truncated["changes"], json!([]));
    assert_eq!(truncated["changes_omitted"], 1);
    assert_eq!(truncated["additional"], json!([]));
    // A suíte ampla continua; uma primeira checagem estreita não pode sobrar do truncamento.
    assert_eq!(truncated["first_check"]["scope"], "workspace");
    assert_eq!(
        truncated["first_check"]["argv"],
        json!(["cargo", "test", "--workspace", "--locked"])
    );
    assert!(serde_json::to_string(&truncated).unwrap().len() <= budget * 4);

    // Orçamento menor que o envelope mínimo: nenhuma recomendação, e nunca uma estreita.
    let minimal = repo.mcp_call(json!({ "experimental": true, "budget": 1 }));
    assert_eq!(minimal["first_check"], Value::Null, "{minimal}");
    assert_eq!(minimal["truncated"], true);
    assert_eq!(minimal["coverage"]["complete"], false);
    assert!(reasons(&minimal).contains(&"budget_truncated".to_string()));
}

#[test]
fn mcp_rejects_missing_opt_in_and_scope_outside_the_checkout() {
    let repo = Repo::new();
    sample_crate(&repo);
    repo.write("sub/keep.rs", "fn nested() {}\n");
    repo.commit();
    repo.write("src/lib.rs", "pub fn answer() -> u8 {\n    3\n}\n");

    let no_opt_in = repo.mcp_error(json!({}));
    assert_eq!(no_opt_in["error"]["code"], "INVALID_ARGUMENT");
    let false_opt_in = repo.mcp_error(json!({ "experimental": false }));
    assert_eq!(false_opt_in["error"]["code"], "INVALID_ARGUMENT");

    let escaping = repo.mcp_error(json!({ "experimental": true, "path": "../outside" }));
    assert_eq!(escaping["error"]["code"], "INVALID_ARGUMENT");
    let absolute = repo.mcp_error(json!({
        "experimental": true,
        "path": repo.0.join("src").to_string_lossy()
    }));
    assert_eq!(absolute["error"]["code"], "INVALID_ARGUMENT");

    // Raiz que não é a raiz do working tree Git e diretório sem repositório.
    let nested = repo.mcp_error(json!({
        "experimental": true,
        "root": repo.0.join("sub").to_string_lossy()
    }));
    assert_eq!(nested["error"]["code"], "TOOL_ERROR", "{nested}");
    let outside = std::env::temp_dir().join(format!("ff-plan-outside-{}", std::process::id()));
    fs::create_dir_all(&outside).unwrap();
    let missing = repo.mcp_error(json!({
        "experimental": true,
        "root": outside.to_string_lossy()
    }));
    assert_eq!(missing["error"]["code"], "TOOL_ERROR", "{missing}");
    let _ = fs::remove_dir_all(&outside);

    // Mudanças em outro checkout não entram no plano desta raiz.
    let other = Repo::new();
    sample_crate(&other);
    other.commit();
    other.write("src/lib.rs", "pub fn outside_repo_symbol() {}\n");
    let plan = repo.mcp_call(json!({ "experimental": true }));
    assert!(!plan.to_string().contains("outside_repo_symbol"), "{plan}");
    assert_eq!(plan["changes"].as_array().unwrap().len(), 1);
}

#[test]
fn rename_to_an_excluded_path_is_not_a_complete_plan() {
    let repo = Repo::new();
    sample_crate(&repo);
    repo.commit();
    repo.git(&["mv", "src/lib.rs", ".env"]);
    let plan = repo.mcp_call(json!({ "experimental": true }));
    assert_eq!(plan["coverage"]["complete"], false, "{plan}");
    assert!(reasons(&plan).contains(&"excluded_path_change".to_string()));
    assert!(!plan.to_string().contains(".env"), "{plan}");
    assert_eq!(plan["first_check"]["scope"], "workspace");
    assert_eq!(plan["additional"], json!([]));
}

#[test]
fn generated_code_falls_back_to_the_workspace_suite() {
    let repo = Repo::new();
    sample_crate(&repo);
    repo.write("src/generated.rs", "pub fn tabulated() {}\n");
    repo.commit();
    repo.write(
        "src/generated.rs",
        "// @generated by build.rs; do not edit\npub fn tabulated() {}\npub fn appended() {}\n",
    );
    let plan = repo.mcp_call(json!({ "experimental": true }));
    assert!(
        plan["changes"]
            .as_array()
            .unwrap()
            .iter()
            .any(|change| change["generated"] == true),
        "{plan}"
    );
    assert!(reasons(&plan).contains(&"generated_code_change".to_string()));
    assert_eq!(plan["first_check"]["scope"], "workspace");
    assert_eq!(plan["additional"], json!([]));
}

#[test]
fn matrix_workflow_is_evidence_and_never_a_command_source() {
    let repo = Repo::new();
    sample_crate(&repo);
    repo.write(
        ".github/workflows/ci.yml",
        "name: ci\njobs:\n  checks:\n    strategy:\n      matrix:\n        flags: ['--lib', '--all-targets']\n    steps:\n      - run: cargo test ${{ matrix.flags }}\n",
    );
    repo.commit();
    repo.write("src/lib.rs", "pub fn answer() -> u8 {\n    4\n}\n");
    let plan = repo.plan(&[]);
    let text = plan.to_string();
    assert!(!text.contains("${{"), "{plan}");
    assert!(!text.contains("--all-targets"), "{plan}");
    assert_eq!(plan["first_check"]["scope"], "first_test");
    assert_eq!(
        broader(&plan)["argv"],
        json!(["cargo", "test", "--workspace", "--locked"])
    );
    assert_eq!(broader(&plan)["evidence"][0]["source"], "manifest");
    assert_eq!(broader(&plan)["evidence"][0]["path"], "Cargo.toml");

    // O workflow literal é aceito apenas como evidência da suíte conhecida, nunca como argv.
    repo.write(
        ".github/workflows/ci.yml",
        "name: ci\njobs:\n  checks:\n    steps:\n      - run: cargo test --workspace --locked\n",
    );
    repo.commit();
    repo.write("src/lib.rs", "pub fn answer() -> u8 {\n    5\n}\n");
    let literal = repo.plan(&[]);
    assert_eq!(
        broader(&literal)["argv"],
        json!(["cargo", "test", "--workspace", "--locked"])
    );
    assert_eq!(
        broader(&literal)["evidence"][0]["source"],
        "known_ci_test_suite"
    );
    assert_eq!(
        broader(&literal)["evidence"][0]["path"],
        ".github/workflows/ci.yml"
    );
    assert_eq!(broader(&literal)["evidence"][0]["line"], 5);
}

#[test]
fn macro_body_change_has_no_resolved_symbol_and_broadens() {
    let repo = Repo::new();
    repo.write(
        "Cargo.toml",
        "[package]\nname = 'sample'\nversion = '0.1.0'\n",
    );
    repo.write(
        "src/lib.rs",
        "macro_rules! doubled {\n    ($value:expr) => {\n        $value * 2\n    };\n}\n",
    );
    repo.write(
        "tests/check.rs",
        "#[test]\nfn checks_doubled() { assert_eq!(sample::doubled!(2), 4); }\n",
    );
    repo.commit();
    repo.write(
        "src/lib.rs",
        "macro_rules! doubled {\n    ($value:expr) => {\n        $value * 3\n    };\n}\n",
    );
    let plan = repo.mcp_call(json!({ "experimental": true }));
    // `macro_rules!` não é declaração reconhecida: sem símbolo resolvido nada de primeira
    // checagem estreita, mesmo com um teste cujo nome cita a macro.
    assert_eq!(plan["changes"][0]["symbols"], json!([]), "{plan}");
    assert!(
        reasons(&plan).contains(&"unmapped_symbols".to_string()),
        "{plan}"
    );
    assert_eq!(plan["first_check"]["scope"], "workspace");
    assert_eq!(plan["additional"], json!([]));
    assert_eq!(plan["coverage"]["complete"], true);
}

#[test]
fn unrecognized_configuration_broadens_to_the_workspace_suite() {
    let repo = Repo::new();
    repo.write(
        "Cargo.toml",
        "[workspace]\nmembers = ['app']\n[workspace.dependencies]\nmissing = { path = 'missing' }\n",
    );
    repo.write(
        "app/Cargo.toml",
        "[package]\nname = 'sample-app'\nversion = '0.1.0'\n[dependencies]\nmissing = { workspace = true }\n",
    );
    repo.write("app/src/lib.rs", "pub fn demo() {}\n");
    repo.commit();
    repo.write("app/src/lib.rs", "pub fn demo() -> u8 { 1 }\n");
    let plan = repo.mcp_call(json!({ "experimental": true }));
    assert!(
        reasons(&plan).contains(&"unresolved_path_dependency".to_string()),
        "{plan}"
    );
    assert_eq!(plan["first_check"]["scope"], "workspace");
    assert_eq!(plan["additional"], json!([]));
    assert_eq!(
        plan["first_check"]["evidence"][0]["source"], "manifest",
        "sem a suíte conhecida a evidência é o manifesto, não um comando extraído"
    );
}
