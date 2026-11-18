//! Robustez do servidor MCP: chamadas JSON-RPC com argumentos hostis nunca causam pânico.
//! Determinístico (xorshift) para rodar no CI normal; exploração longa fica em `fuzz/`.

use ff_cli::mcp::{handle_request, JsonRpcRequest};
use serde_json::{json, Value};

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n.max(1) as u64) as usize
    }
}

const TOOLS: &[&str] = &[
    "disk_usage",
    "find_files",
    "file_stat",
    "recent_changes",
    "search_content",
    "diff_files",
    "batch",
    "find_symbol",
    "repo_map",
    "find_duplicates",
    "inexistente",
];
const KEYS: &[&str] = &[
    "target",
    "path",
    "pattern",
    "name",
    "regex",
    "fuzzy",
    "ext",
    "size",
    "limit",
    "top",
    "depth",
    "budget",
    "cursor",
    "all",
    "include_secrets",
    "relative",
    "a",
    "b",
    "file_a",
    "file_b",
    "since",
    "query",
    "symbol",
    "mode",
    "ignore_case",
    "context",
    "queries",
    "max_depth",
];

fn random_value(rng: &mut Rng, dir: &str, depth: u32) -> Value {
    match rng.below(if depth > 2 { 8 } else { 10 }) {
        0 => Value::Null,
        1 => json!(rng.below(2) == 0),
        2 => json!(rng.next() as i64),
        3 => json!(u64::MAX),
        4 => json!(-1),
        5 => json!(1.5e300),
        6 => json!(dir),
        7 => {
            let n = rng.below(24);
            json!((0..n)
                .map(|_| char::from_u32(rng.below(0x2FF) as u32).unwrap_or('?'))
                .collect::<String>())
        }
        8 => Value::Array(
            (0..rng.below(4))
                .map(|_| random_value(rng, dir, depth + 1))
                .collect(),
        ),
        _ => random_object(rng, dir, depth + 1),
    }
}

fn random_object(rng: &mut Rng, dir: &str, depth: u32) -> Value {
    let mut m = serde_json::Map::new();
    for _ in 0..rng.below(6) {
        m.insert(
            KEYS[rng.below(KEYS.len())].to_string(),
            random_value(rng, dir, depth),
        );
    }
    Value::Object(m)
}

/// Diretório pequeno e descartável, também usado como diretório de trabalho: o padrão "." das
/// ferramentas nunca varre o repositório ou o disco real.
fn sandbox() -> String {
    static DIR: std::sync::OnceLock<std::path::PathBuf> = std::sync::OnceLock::new();
    let dir = DIR.get_or_init(|| {
        let dir = std::env::temp_dir().join(format!("ff-mcp-rob-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("a.rs"), "fn main() {}\n").unwrap();
        std::fs::write(dir.join("b.rs"), "fn main() { let x = 1; }\n").unwrap();
        std::env::set_current_dir(&dir).unwrap();
        dir
    });
    dir.to_string_lossy().into_owned()
}

fn call(method: &str, params: Value) {
    let req = JsonRpcRequest {
        jsonrpc: "2.0".into(),
        id: Some(json!(1)),
        method: method.into(),
        params: Some(params),
    };
    let _ = handle_request(&req);
}

#[test]
fn tool_calls_with_hostile_arguments_never_panic() {
    let d = sandbox();

    let mut rng = Rng(0xFA57_F11E);
    for _ in 0..800 {
        let tool = TOOLS[rng.below(TOOLS.len())];
        let mut args = random_object(&mut rng, &d, 0);
        if rng.below(3) != 0 {
            if let Value::Object(m) = &mut args {
                m.insert("target".into(), json!(d));
            }
        }
        call("tools/call", json!({ "name": tool, "arguments": args }));
    }
}

#[test]
fn malformed_envelopes_never_panic() {
    sandbox();
    let mut rng = Rng(77);
    let methods = [
        "initialize",
        "tools/list",
        "tools/call",
        "ping",
        "notifications/initialized",
        "resources/list",
        "",
        "\u{0}",
    ];
    for _ in 0..2000 {
        let m = methods[rng.below(methods.len())];
        call(m, random_value(&mut rng, ".", 0));
    }
    // `params` ausente e `id` de tipos estranhos.
    for id in [
        json!(null),
        json!("x"),
        json!(1.5),
        json!([1]),
        json!({"a":1}),
    ] {
        let req = JsonRpcRequest {
            jsonrpc: "2.0".into(),
            id: Some(id),
            method: "tools/call".into(),
            params: None,
        };
        let _ = handle_request(&req);
    }
}
