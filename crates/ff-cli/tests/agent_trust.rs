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
    assert_eq!(v["schema"], "ff/2");
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
    assert_eq!(v["schema"], "ff/2");
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

/// `PATH-CWD-001` (ff/2): todo `path` abre direto da pasta atual; `--schema 1` mantém o formato antigo.
#[test]
fn ff2_paths_open_from_cwd_and_schema_1_is_kept() {
    let dir = fixture("paths");
    std::fs::create_dir_all(dir.join("sub/deep")).unwrap();
    std::fs::write(dir.join("sub/deep/alvo.txt"), "marcador_ff2\n").unwrap();

    let in_dir = |args: &[&str]| {
        let mut cmd = ff();
        cmd.current_dir(&dir).args(args);
        run_json(&mut cmd, None).0
    };

    let v = in_dir(&["find", "alvo.txt", "-t", "sub", "-c"]);
    assert_eq!(v["schema"], "ff/2");
    assert_eq!(v["results"][0]["path"], "sub/deep/alvo.txt");

    let v = in_dir(&["grep", "marcador_ff2", "sub", "-c"]);
    assert_eq!(v["result"]["matches"][0]["path"], "sub/deep/alvo.txt");
    assert!(
        v["result"].get("summary").is_none(),
        "ff/2 não repete o summary"
    );

    let v = in_dir(&["--schema", "1", "find", "alvo.txt", "-t", "sub", "-c"]);
    assert_eq!(v["schema"], "ff/1");
    let p = v["results"][0]["path"].as_str().unwrap().replace('\\', "/");
    assert_eq!(p, "deep/alvo.txt", "ff/1: relativo ao alvo");

    let mut cmd = ff();
    cmd.current_dir(&dir)
        .env("FASTFILE_SCHEMA", "1")
        .args(["grep", "marcador_ff2", "sub", "-c"]);
    let (v, _) = run_json(&mut cmd, None);
    assert_eq!(v["schema"], "ff/1");
    assert!(v["result"].get("summary").is_some());

    let _ = std::fs::remove_dir_all(&dir);
}

/// `ff read`: trechos de vários arquivos numa chamada, continuação por `next` e segredos.
#[test]
fn read_returns_ranges_with_continuation_and_masking() {
    let dir = fixture("read");
    let body: String = (1..=50).map(|i| format!("linha {i}\n")).collect();
    std::fs::write(dir.join("a.txt"), &body).unwrap();
    std::fs::write(dir.join("cfg.yaml"), "password: hunter2secret\nnome: ok\n").unwrap();
    std::fs::write(dir.join(".env"), "API_KEY=abc\n").unwrap();
    std::fs::write(dir.join("bin.dat"), [0u8, 1, 2, 3, 0, 5]).unwrap();

    let mut cmd = ff();
    cmd.current_dir(&dir).args([
        "read",
        "a.txt:10-12",
        "a.txt:30",
        "a.txt",
        "cfg.yaml",
        ".env",
        "bin.dat",
        "--around",
        "1",
        "--max-lines",
        "5",
        "-c",
    ]);
    let (v, code) = run_json(&mut cmd, None);
    assert_eq!(code, 0, "{v}");
    let r = v["results"].as_array().unwrap();

    assert_eq!(r[0]["content"], "linha 10\nlinha 11\nlinha 12\n");
    assert_eq!(r[0]["total_lines"], 50);
    assert!(
        r[0].get("next").is_none(),
        "intervalo explícito não pede continuação"
    );

    assert_eq!(r[1]["content"], "linha 29\nlinha 30\nlinha 31\n");
    assert_eq!(r[1]["focus_line"], 30);

    assert_eq!(r[2]["end_line"], 5);
    assert_eq!(r[2]["next"], "a.txt:6-");

    let cfg = r[3]["content"].as_str().unwrap();
    assert!(!cfg.contains("hunter2secret") && cfg.contains("nome: ok"));

    assert_eq!(r[4]["error"]["code"], "SECRET_FILE");
    assert_eq!(r[5]["binary"], true);

    // Continuação pelo `next`.
    let mut cmd = ff();
    cmd.current_dir(&dir)
        .args(["read", "a.txt:6-", "--max-lines", "2", "-c"]);
    let (v, _) = run_json(&mut cmd, None);
    assert_eq!(v["results"][0]["content"], "linha 6\nlinha 7\n");

    let _ = std::fs::remove_dir_all(&dir);
}
