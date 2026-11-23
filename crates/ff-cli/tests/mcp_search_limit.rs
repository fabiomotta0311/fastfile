//! `search_content` no MCP: limite padrão menor que o do CLI, corte estável e `has_more` (#120).

use ff_cli::mcp::{execute_mcp_tool, SEARCH_CONTENT_DEFAULT_LIMIT, SERVER_INSTRUCTIONS};
use serde_json::{json, Value};

fn sandbox(nome: &str, linhas: usize) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("ff-mcp-limit-{nome}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    for arq in ["a.txt", "b.txt"] {
        let texto: String = (0..linhas).map(|i| format!("alvo {i}\n")).collect();
        std::fs::write(dir.join(arq), texto).unwrap();
    }
    dir
}

fn buscar(dir: &std::path::Path, extra: Value) -> Value {
    let mut args = json!({ "pattern": "alvo", "path": dir.to_string_lossy(), "use_index": false });
    for (k, v) in extra.as_object().unwrap() {
        args[k] = v.clone();
    }
    execute_mcp_tool("search_content", &args).unwrap()
}

fn posicoes(res: &Value) -> Vec<(String, u64)> {
    res["result"]["matches"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| {
            (
                m["path"].as_str().unwrap().to_string(),
                m["line_number"].as_u64().unwrap(),
            )
        })
        .collect()
}

#[test]
fn padrao_corta_no_limite_em_ordem_e_avisa() {
    let dir = sandbox("padrao", 40);
    let res = buscar(&dir, json!({}));
    let pos = posicoes(&res);
    assert_eq!(pos.len(), SEARCH_CONTENT_DEFAULT_LIMIT);
    let mut ordenado = pos.clone();
    ordenado.sort();
    assert_eq!(pos, ordenado, "resultados em ordem de caminho e linha");
    assert_eq!(res["has_more"], true);
    assert_eq!(
        res["truncated"], false,
        "truncated continua sendo só o budget"
    );
    let avisos = res["warnings"].to_string();
    assert!(avisos.contains("mode=files"), "{avisos}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn sem_corte_nao_marca_has_more() {
    let dir = sandbox("completo", 5);
    let res = buscar(&dir, json!({ "limit": 100 }));
    assert_eq!(posicoes(&res).len(), 10);
    assert_eq!(res["has_more"], false);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn limite_explicito_nunca_e_ultrapassado() {
    let dir = sandbox("explicito", 40);
    for limite in [1usize, 7, 41] {
        let res = buscar(&dir, json!({ "limit": limite }));
        assert_eq!(posicoes(&res).len(), limite, "limit={limite}");
        assert_eq!(res["has_more"], true, "limit={limite}");
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn instrucoes_ensinam_a_carregar_e_quando_trocar_o_shell() {
    for trecho in [
        "mcp__fastfile__search_content",
        "instead of shelling out to grep",
        "has_more=true",
        "at most 30",
    ] {
        assert!(SERVER_INSTRUCTIONS.contains(trecho), "falta: {trecho}");
    }
    assert!(SERVER_INSTRUCTIONS.contains(&format!("at most {SEARCH_CONTENT_DEFAULT_LIMIT}")));
}
