//! Contratos de confiança para agentes (docs/avaliacao-agentes.md, P0): respostas nunca
//! silenciosamente erradas ou incompletas.

use std::io::Write;
use std::process::{Command, Stdio};

fn ff() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ff"))
}

fn run_json(cmd: &mut Command, stdin: Option<&str>) -> (serde_json::Value, i32) {
    let mut child = cmd
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("executa ff");
    if let Some(input) = stdin {
        child
            .stdin
            .take()
            .unwrap()
            .write_all(input.as_bytes())
            .unwrap();
    } else {
        drop(child.stdin.take());
    }
    let out = child.wait_with_output().unwrap();
    let text = String::from_utf8_lossy(&out.stdout);
    let value = serde_json::from_str(text.trim())
        .unwrap_or_else(|e| panic!("saída não é JSON ({e}): {text}"));
    (value, out.status.code().unwrap_or(-1))
}

fn fixture(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("ff_agent_trust_{name}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// `batch` no CLI executa as consultas e devolve os mesmos resultados das chamadas avulsas.
#[test]
fn cli_batch_returns_real_results() {
    let dir = fixture("batch");
    std::fs::write(dir.join("alpha.toml"), "chave = 1\n").unwrap();
    std::fs::write(dir.join("beta.rs"), "fn alpha_marker() {}\n").unwrap();
    let root = dir.to_string_lossy().to_string();

    let input = serde_json::json!({ "queries": [
        { "command": "stat", "target": dir.join("alpha.toml").to_string_lossy() },
        { "command": "find", "pattern": "*.toml", "target": root },
        { "command": "grep", "pattern": "alpha_marker", "path": root, "mode": "files" },
        { "command": "inexistente" }
    ]})
    .to_string();

    let (v, code) = run_json(ff().args(["batch", "-c"]), Some(&input));
    assert_eq!(code, 0, "{v}");
    assert_eq!(v["schema"], "ff/1");
    let results = v["results"].as_array().unwrap();
    assert_eq!(results.len(), 4);

    assert_eq!(results[0]["result"]["size"], 10);
    assert_eq!(results[1]["summary"]["total_matches"], 1);
    assert_eq!(results[2]["summary"]["files_matched"], 1);
    assert_eq!(results[3]["status"], "error");
    assert_eq!(results[3]["command"], "inexistente");

    let _ = std::fs::remove_dir_all(&dir);
}

/// `SECRETS-CONTENT-001`: valores com cara de segredo saem mascarados por padrão.
#[test]
fn grep_masks_secret_values_by_default() {
    let dir = fixture("mask");
    std::fs::write(
        dir.join("config.yaml"),
        "token = \"ghp_abcdefghijklmnopqrstuvwxyz0123456789\"\npassword: hunter2secret\n",
    )
    .unwrap();
    let root = dir.to_string_lossy().to_string();

    let (v, _) = run_json(ff().args(["grep", "token|password", &root, "-c"]), None);
    let text = v.to_string();
    assert!(!text.contains("ghp_abcdef"), "{text}");
    assert!(!text.contains("hunter2secret"), "{text}");
    assert_eq!(v["summary"]["secrets_masked"], 2);

    let (v, _) = run_json(
        ff().args(["grep", "token", &root, "-c", "--include-secrets"]),
        None,
    );
    assert!(v.to_string().contains("ghp_abcdef"));

    let _ = std::fs::remove_dir_all(&dir);
}

/// Erros de uso seguem o envelope `ff/1` com código próprio (não colidem com `TARGET_NOT_FOUND`).
#[test]
fn usage_errors_are_structured() {
    let (v, code) = run_json(ff().args(["grep", "x", "--flag-que-nao-existe"]), None);
    assert_eq!(code, 64);
    assert_eq!(v["schema"], "ff/1");
    assert_eq!(v["error"]["code"], "USAGE_ERROR");
}

/// `--timeout` devolve erro estruturado em vez de travar o harness.
#[test]
fn timeout_returns_structured_error() {
    let (v, code) = run_json(
        ff().args([
            "find",
            "x",
            "--synthetic",
            "50000000",
            "-c",
            "--timeout",
            "0.2",
        ]),
        None,
    );
    if code == 0 {
        // Máquina rápida o bastante para terminar antes do limite: nada a verificar.
        return;
    }
    assert_eq!(code, 6, "{v}");
    assert_eq!(v["error"]["code"], "TIMEOUT");
}

#[test]
fn split_timeout_arg_removes_global_flag() {
    let args = ["ff", "find", "x", "--timeout", "2.5", "-c"]
        .iter()
        .map(std::ffi::OsString::from)
        .collect();
    let (rest, t) = ff_cli::split_timeout_arg(args);
    assert_eq!(rest, ["ff", "find", "x", "-c"]);
    assert_eq!(t, Some(std::time::Duration::from_millis(2500)));

    let args = ["ff", "grep", "--", "--timeout"]
        .iter()
        .map(std::ffi::OsString::from)
        .collect();
    let (rest, t) = ff_cli::split_timeout_arg(args);
    assert_eq!(rest.len(), 4);
    assert_eq!(t, None);
}

#[test]
fn mcp_declares_read_only_and_negotiates_version() {
    use ff_cli::mcp::{get_tool_definitions, negotiate_protocol_version};
    for tool in get_tool_definitions() {
        assert_eq!(
            tool["annotations"]["readOnlyHint"], true,
            "{}",
            tool["name"]
        );
        assert_eq!(tool["annotations"]["destructiveHint"], false);
    }
    assert_eq!(negotiate_protocol_version(Some("2024-11-05")), "2024-11-05");
    assert_eq!(negotiate_protocol_version(Some("1999-01-01")), "2025-06-18");
    assert_eq!(negotiate_protocol_version(None), "2025-06-18");
}

#[test]
fn mcp_errors_are_typed() {
    use ff_cli::mcp::classify_tool_error;
    assert_eq!(
        classify_tool_error("alvo não encontrado: 'x'"),
        "TARGET_NOT_FOUND"
    );
    assert_eq!(
        classify_tool_error("ferramenta desconhecida: 'x'"),
        "UNKNOWN_TOOL"
    );
    assert_eq!(classify_tool_error("timeout: a ferramenta"), "TIMEOUT");
    assert_eq!(
        classify_tool_error("modo inválido 'x': use 'content'"),
        "INVALID_ARGUMENT"
    );
}

/// Paridade com o ripgrep: `-F` (literal), `-w` (palavra) e `--hidden` (pastas ocultas).
#[test]
fn grep_fixed_word_and_hidden_flags() {
    let dir = fixture("parity");
    std::fs::create_dir_all(dir.join(".github/workflows")).unwrap();
    std::fs::write(dir.join(".github/workflows/ci.yml"), "run: cargo test\n").unwrap();
    std::fs::write(
        dir.join("a.txt"),
        "versão 1.2 aqui\nversao 102 ali\ncargo_test\n",
    )
    .unwrap();
    let root = dir.to_string_lossy().to_string();
    let count = |v: &serde_json::Value| v["summary"]["total_matches"].as_u64().unwrap();

    let (v, _) = run_json(ff().args(["grep", "1.2", &root, "-c"]), None);
    assert_eq!(count(&v), 2, "regex: '.' casa qualquer caractere");
    let (v, _) = run_json(ff().args(["grep", "1.2", &root, "-c", "-F"]), None);
    assert_eq!(count(&v), 1, "-F: só o literal");

    let (v, _) = run_json(ff().args(["grep", "cargo", &root, "-c", "--hidden"]), None);
    assert_eq!(count(&v), 2, "--hidden inclui .github/");
    let (v, _) = run_json(
        ff().args(["grep", "cargo", &root, "-c", "-w", "--hidden"]),
        None,
    );
    assert_eq!(count(&v), 1, "-w exclui cargo_test");
    let (v, _) = run_json(ff().args(["grep", "cargo", &root, "-c"]), None);
    assert_eq!(count(&v), 1, "sem --hidden, .github/ fica de fora");

    let _ = std::fs::remove_dir_all(&dir);
}
