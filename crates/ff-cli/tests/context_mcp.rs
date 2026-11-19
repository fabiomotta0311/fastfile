//! Contrato da exposição `repair_context` (CLI `context` e MCP `tools/call`), opt-in experimental.

use serde_json::{json, Value};
use std::{
    fs,
    io::Write,
    path::PathBuf,
    process::{Command, Stdio},
    sync::atomic::{AtomicUsize, Ordering},
};

struct Fixture(PathBuf);

impl Fixture {
    fn new() -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let dir = std::env::temp_dir().join(format!(
            "ff-context-mcp-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&dir).unwrap();
        #[cfg(not(windows))]
        let dir = dir.canonicalize().unwrap();
        let f = Self(dir);
        f.write("repo/src/math.rs", "pub fn total() -> usize {\n    42\n}\n");
        f
    }

    fn write(&self, path: &str, text: &str) {
        let path = self.0.join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }

    /// Muitos testes que citam `total`, em faixas separadas, para exercitar o orçamento.
    fn many_ranges(&self) {
        let mut text = String::new();
        for n in 0..12 {
            text.push_str(&format!(
                "#[test]\nfn check_{n}() {{\n    assert_eq!(total(), 42);\n}}\n{}",
                "\n".repeat(8)
            ));
        }
        self.write("repo/tests/many.rs", &text);
    }

    fn cli(&self, args: &[&str]) -> (bool, Value, String) {
        let out = Command::new(env!("CARGO_BIN_EXE_ff"))
            .current_dir(&self.0)
            .args(["context", "--root", "repo"])
            .args(args)
            .output()
            .unwrap();
        let both = format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        let json = serde_json::from_slice(&out.stdout).unwrap_or(Value::Null);
        (out.status.success(), json, both)
    }

    fn run(&self, args: &[&str]) -> Value {
        let (ok, value, text) = self.cli(args);
        assert!(ok, "{text}");
        value
    }

    /// Fala JSON-RPC com `ff mcp` (opt-in ligado por padrão) e devolve a resposta de cada id.
    fn mcp(&self, experimental: bool, calls: &[(&str, Value)]) -> Vec<Value> {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_ff"));
        cmd.current_dir(&self.0)
            .arg("mcp")
            .env_remove("FASTFILE_EXPERIMENTAL")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped());
        if experimental {
            cmd.env("FASTFILE_EXPERIMENTAL", "context");
        }
        let mut child = cmd.spawn().unwrap();
        let mut stdin = child.stdin.take().unwrap();
        for (id, (method, params)) in calls.iter().enumerate() {
            let req = json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params});
            writeln!(stdin, "{req}").unwrap();
        }
        drop(stdin);
        let out = child.wait_with_output().unwrap();
        String::from_utf8(out.stdout)
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect()
    }

    fn mcp_context(&self, arguments: Value) -> (bool, Value) {
        let responses = self.mcp(
            true,
            &[(
                "tools/call",
                json!({"name": "repair_context", "arguments": arguments}),
            )],
        );
        let result = &responses[0]["result"];
        let text = result["content"][0]["text"].as_str().unwrap();
        (
            result["isError"] == false,
            serde_json::from_str(text).unwrap(),
        )
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn warnings(report: &Value) -> Vec<&str> {
    report["warnings"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(Value::as_str)
        .collect()
}

#[test]
fn cli_and_mcp_return_the_same_contract() {
    let f = Fixture::new();
    f.many_ranges();
    for (cli, mcp) in [
        (
            vec!["--symbol", "total", "--budget", "700"],
            json!({"root": "repo", "symbol": "total", "budget": 700}),
        ),
        (
            vec!["--location", "repo/src/math.rs:2"],
            json!({"root": "repo", "location": "repo/src/math.rs:2"}),
        ),
        (
            vec!["--diagnostic", "repo/src/math.rs:2|mensagem"],
            json!({"root": "repo", "diagnostic": "repo/src/math.rs:2|mensagem"}),
        ),
    ] {
        let direct = f.run(&cli);
        let (ok, via_mcp) = f.mcp_context(mcp);
        assert!(ok, "{via_mcp}");
        assert_eq!(direct, via_mcp);
        assert_eq!(direct["schema"], "ff-context/1");
        for key in [
            "root",
            "anchor",
            "coverage",
            "budget",
            "truncated",
            "omitted_count",
            "warnings",
            "empty_means_absent",
        ] {
            assert!(direct.get(key).is_some(), "falta {key}: {direct}");
        }
    }
}

#[test]
fn catalog_is_unchanged_unless_the_experiment_is_enabled() {
    let f = Fixture::new();
    let names = |experimental| -> Vec<String> {
        f.mcp(experimental, &[("tools/list", json!({}))])[0]["result"]["tools"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["name"].as_str().unwrap().to_owned())
            .collect()
    };
    let off = names(false);
    let on = names(true);
    assert!(!off.contains(&"repair_context".to_owned()));
    assert_eq!(on.len(), off.len() + 1);
    assert_eq!(
        on.iter()
            .filter(|n| *n != "repair_context")
            .collect::<Vec<_>>(),
        off.iter().collect::<Vec<_>>()
    );
    let responses = f.mcp(
        false,
        &[(
            "tools/call",
            json!({"name": "repair_context", "arguments": {"root": "repo", "symbol": "total"}}),
        )],
    );
    assert_eq!(responses[0]["result"]["isError"], true);
    let text = responses[0]["result"]["content"][0]["text"]
        .as_str()
        .unwrap();
    assert!(text.contains("UNKNOWN_TOOL"), "{text}");
    let tool = &f.mcp(true, &[("tools/list", json!({}))])[0]["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["name"] == "repair_context")
        .unwrap()
        .clone();
    assert_eq!(tool["annotations"]["readOnlyHint"], true);
    assert_eq!(tool["inputSchema"]["additionalProperties"], false);
}

#[test]
fn no_unrestricted_mode_and_errors_never_echo_input() {
    let f = Fixture::new();
    for extra in ["include_secrets", "all", "hidden", "follow_links"] {
        let mut args = json!({"root": "repo", "symbol": "total"});
        args[extra] = json!(true);
        let responses = f.mcp(
            true,
            &[(
                "tools/call",
                json!({"name": "repair_context", "arguments": args}),
            )],
        );
        let text = responses[0]["result"]["content"][0]["text"]
            .as_str()
            .unwrap();
        assert_eq!(responses[0]["result"]["isError"], true, "{extra}");
        assert!(text.contains("INVALID_ARGUMENT"), "{text}");
        assert!(!text.contains(extra), "{text}");
    }
    let secret = "password=fixtureErrorNeverEcho";
    let (ok, _, both) = f.cli(&["--symbol", secret]);
    assert!(!ok);
    assert!(!both.contains("fixtureErrorNeverEcho"), "{both}");
    let (ok, report) = f.mcp_context(json!({"root": "repo", "symbol": secret}));
    assert!(!ok);
    assert!(!report.to_string().contains("fixtureErrorNeverEcho"));
    let (ok, report) = f.mcp_context(json!({"root": "repo", "budget": 0, "symbol": "total"}));
    assert!(!ok, "{report}");
}

#[test]
fn diagnostic_secret_is_never_echoed_by_cli_or_mcp() {
    let f = Fixture::new();
    f.write(
        "repo/src/math.rs",
        "pub fn total() -> usize {\n    let password = \"fixtureFilePasswordNeverEcho\";\n    42\n}\n",
    );
    let diagnostic = "repo/src/math.rs:2|token=fixtureDiagnosticNeverEcho";
    let direct = f.run(&["--diagnostic", diagnostic]);
    let (ok, via_mcp) = f.mcp_context(json!({"root": "repo", "diagnostic": diagnostic}));
    assert!(ok);
    for report in [direct, via_mcp] {
        let text = report.to_string();
        assert!(!text.contains("fixtureDiagnosticNeverEcho"));
        assert!(!text.contains("fixtureFilePasswordNeverEcho"));
        assert!(text.contains("redacted:assignment"));
    }
    // Um segredo no próprio caminho da localização nunca volta na resposta nem no erro.
    let (_, report, both) = f.cli(&[
        "--location",
        "repo/ghp_fixturePathNeverEcho123456789012345678.rs:1",
    ]);
    assert!(!both.contains("fixturePathNeverEcho"), "{both} {report}");
}

#[test]
fn minimum_and_tight_budgets_stay_partial_and_inside_the_limit() {
    let f = Fixture::new();
    f.many_ranges();
    let full = f.run(&["--symbol", "total", "--budget", "50000"]);
    let all = full["snippets"].as_array().unwrap().len();
    assert!(all >= 10 && full["truncated"] == false);
    assert_eq!(full["empty_means_absent"], true);

    let tiny = f.run(&["--symbol", "total", "--budget", "1"]);
    assert_eq!(tiny["truncated"], true);
    assert_eq!(tiny["coverage"], "partial");
    assert_eq!(tiny["empty_means_absent"], false);
    assert!(warnings(&tiny).contains(&"budget_too_small"));
    assert!(tiny.get("snippets").is_none());

    let mut seen_truncation = false;
    for budget in (1..900).step_by(23) {
        let r = f.run(&["--symbol", "total", "--budget", &budget.to_string()]);
        let bytes = r.to_string().len();
        if r["warnings"] != json!(["budget_too_small"]) {
            assert!(bytes <= budget * 4, "budget {budget}: {bytes} bytes");
        }
        if r["truncated"] == true {
            seen_truncation = true;
            assert_eq!(r["coverage"], "partial", "budget {budget}");
            assert_eq!(r["empty_means_absent"], false);
            if r["warnings"] == json!(["budget_too_small"]) {
                continue;
            }
            assert_eq!(
                r["omitted_count"].as_u64().unwrap() as usize
                    + r["snippets"].as_array().map_or(0, Vec::len),
                all,
                "budget {budget}"
            );
        }
        // A definição observada é preservada antes de qualquer teste ou menção candidata.
        if let Some(snippets) = r["snippets"].as_array().filter(|s| !s.is_empty()) {
            assert!(snippets
                .iter()
                .flat_map(|s| s["evidence"].as_array().unwrap())
                .any(|e| e["status"] == "observed"));
        }
    }
    assert!(seen_truncation);

    let mid = f.run(&["--symbol", "total", "--budget", "600"]);
    let first = &mid["omitted"][0];
    assert_eq!(first["status"], "candidate");
    assert!(first["read"].as_str().unwrap().contains("repo"), "{first}");
}

#[test]
fn expired_deadline_is_partial_never_empty() {
    let f = Fixture::new();
    let direct = f.run(&["--symbol", "total", "--deadline-ms", "0"]);
    let (ok, via_mcp) = f.mcp_context(json!({"root": "repo", "symbol": "total", "timeout_ms": 0}));
    assert!(ok);
    for report in [direct, via_mcp] {
        assert!(warnings(&report).contains(&"deadline_exceeded"));
        assert_eq!(report["coverage"], "partial");
        assert_eq!(report["empty_means_absent"], false);
        assert_eq!(report["symbol_status"], "partial");
    }
}

#[test]
fn links_outside_the_root_are_not_followed_and_make_scope_partial() {
    let f = Fixture::new();
    f.write("outer/other.rs", "fn total() {} // NEVER_EXPOSE\n");
    let link = f.0.join("repo/link");
    let outer = f.0.join("outer");
    #[cfg(unix)]
    std::os::unix::fs::symlink(&outer, &link).unwrap();
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let output = Command::new("cmd.exe")
            .args(["/d", "/v:off", "/c"])
            .env("FF_CONTEXT_LINK", &link)
            .env("FF_CONTEXT_TARGET", &outer)
            .raw_arg(r#"mklink /J "%FF_CONTEXT_LINK%" "%FF_CONTEXT_TARGET%""#)
            .output()
            .unwrap();
        assert!(output.status.success(), "{output:?}");
    }
    let direct = f.run(&["--symbol", "total"]);
    let (ok, via_mcp) = f.mcp_context(json!({"root": "repo", "symbol": "total"}));
    assert!(ok);
    for report in [&direct, &via_mcp] {
        assert_eq!(report["definition_count"], 1);
        assert!(!report.to_string().contains("NEVER_EXPOSE"));
        assert!(warnings(report).contains(&"link_skipped"));
        assert_eq!(report["coverage"], "partial");
        assert_eq!(report["empty_means_absent"], false);
    }
    let through = f.run(&["--location", "repo/link/other.rs:1"]);
    assert_eq!(through["anchor"]["status"], "unverified");
    assert_eq!(through["anchor"]["reason"], "not_in_scope");
    #[cfg(windows)]
    fs::remove_dir(&link).unwrap();
}

#[test]
fn unreadable_file_makes_the_scope_partial() {
    let f = Fixture::new();
    f.write("repo/src/locked.rs", "fn locked() {}\n");
    let locked = f.0.join("repo/src/locked.rs");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&locked, fs::Permissions::from_mode(0o000)).unwrap();
        if fs::File::open(&locked).is_ok() {
            return; // executando como root: o acesso não é negado
        }
    }
    // Sem compartilhamento, outra abertura falha com violação de acesso.
    #[cfg(windows)]
    let _guard = {
        use std::os::windows::fs::OpenOptionsExt;
        fs::OpenOptions::new()
            .read(true)
            .share_mode(0)
            .open(&locked)
            .unwrap()
    };
    let direct = f.run(&["--symbol", "locked"]);
    let (ok, via_mcp) = f.mcp_context(json!({"root": "repo", "symbol": "locked"}));
    assert!(ok);
    for report in [&direct, &via_mcp] {
        assert!(warnings(report).contains(&"unreadable_source"), "{report}");
        assert_eq!(report["coverage"], "partial");
        assert_eq!(report["symbol_status"], "partial");
        assert_eq!(report["files_skipped"], 1);
        assert_eq!(report["empty_means_absent"], false);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&locked, fs::Permissions::from_mode(0o644)).unwrap();
    }
}

#[test]
fn unverified_anchor_never_proves_absence() {
    let f = Fixture::new();
    for (location, reason) in [
        ("repo/src/missing.rs:1", "not_in_scope"),
        ("repo/src/math.rs:900", "line_out_of_range"),
    ] {
        let report = f.run(&["--location", location]);
        assert_eq!(report["anchor"]["status"], "unverified");
        assert_eq!(report["anchor"]["reason"], reason);
        assert_eq!(report["empty_means_absent"], false);
    }
}
