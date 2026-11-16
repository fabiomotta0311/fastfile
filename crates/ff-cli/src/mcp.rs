//! Servidor MCP (Model Context Protocol) sobre stdio (§3.1, §3.3 e #21).

use std::io::{BufRead, Write};
use std::path::Path;

use ff_index::IndexSource;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::schema::{JsonEnvelope, SymbolResultItem, DEFAULT_SOURCE, SCHEMA_VERSION};
use crate::secrets::is_secret_file;
use crate::util::{
    apply_budget, format_bytes, format_unix_timestamp, make_relative_path, parse_duration,
    parse_size_filter,
};
use crate::{DuArgs, DupesArgs, FindArgs, GrepArgs, MapArgs, RecentArgs, StatArgs, SymArgs};

/// Estrutura de requisição JSON-RPC 2.0.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcRequest {
    pub jsonrpc: String,
    pub id: Option<serde_json::Value>,
    pub method: String,
    #[serde(default)]
    pub params: Option<serde_json::Value>,
}

/// Estrutura de resposta JSON-RPC 2.0.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: String,
    pub id: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
}

/// Estrutura de erro JSON-RPC 2.0.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcError {
    pub code: i64,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

impl JsonRpcResponse {
    pub fn success(id: Option<serde_json::Value>, result: serde_json::Value) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id,
            result: Some(result),
            error: None,
        }
    }

    pub fn error(id: Option<serde_json::Value>, code: i64, message: impl Into<String>) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id,
            result: None,
            error: Some(JsonRpcError {
                code,
                message: message.into(),
                data: None,
            }),
        }
    }
}

/// Executa o loop principal do servidor MCP sobre stdio.
pub fn run_mcp_server() -> Result<(), String> {
    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    let mut stdin_lock = stdin.lock();
    let mut stdout_lock = stdout.lock();

    let mut line = String::new();

    while stdin_lock.read_line(&mut line).map_err(|e| e.to_string())? > 0 {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            line.clear();
            continue;
        }

        // Lidar opcionalmente com cabeçalhos HTTP/LSP "Content-Length: <len>\r\n\r\n"
        let json_str = if let Some(stripped) = trimmed.strip_prefix("Content-Length:") {
            if let Some(pos) = stripped.find("\r\n\r\n") {
                &stripped[pos + 4..]
            } else if let Some(pos) = stripped.find("\n\n") {
                &stripped[pos + 2..]
            } else {
                // Linha de cabeçalho intermediária; lê próxima linha
                line.clear();
                continue;
            }
        } else {
            trimmed
        };

        if let Ok(request) = serde_json::from_str::<JsonRpcRequest>(json_str) {
            let response = handle_request(&request);
            if let Some(resp) = response {
                let serialized = serde_json::to_string(&resp).map_err(|e| e.to_string())?;
                writeln!(stdout_lock, "{serialized}").map_err(|e| e.to_string())?;
                stdout_lock.flush().map_err(|e| e.to_string())?;
            }
        } else {
            let resp = JsonRpcResponse::error(None, -32700, "Parse error");
            let serialized = serde_json::to_string(&resp).map_err(|e| e.to_string())?;
            writeln!(stdout_lock, "{serialized}").map_err(|e| e.to_string())?;
            stdout_lock.flush().map_err(|e| e.to_string())?;
        }

        line.clear();
    }

    Ok(())
}

/// Trata uma requisição JSON-RPC do protocolo MCP.
pub fn handle_request(req: &JsonRpcRequest) -> Option<JsonRpcResponse> {
    match req.method.as_str() {
        "initialize" => {
            let result = json!({
                "protocolVersion": "2024-11-05",
                "capabilities": {
                    "tools": {}
                },
                "serverInfo": {
                    "name": "fastfile",
                    "version": env!("CARGO_PKG_VERSION")
                }
            });
            Some(JsonRpcResponse::success(req.id.clone(), result))
        }
        "notifications/initialized" => {
            // Notificação de cliente: nenhuma resposta necessária
            None
        }
        "ping" => Some(JsonRpcResponse::success(req.id.clone(), json!({}))),
        "tools/list" => {
            let tools = get_tool_definitions();
            Some(JsonRpcResponse::success(
                req.id.clone(),
                json!({ "tools": tools }),
            ))
        }
        "tools/call" => {
            let Some(params) = &req.params else {
                return Some(JsonRpcResponse::error(
                    req.id.clone(),
                    -32602,
                    "Invalid params",
                ));
            };

            let tool_name = params.get("name").and_then(|n| n.as_str()).unwrap_or("");
            let arguments = params.get("arguments").cloned().unwrap_or(json!({}));

            let call_result = execute_mcp_tool(tool_name, &arguments);

            let (text_content, is_error) = match call_result {
                Ok(val) => (
                    serde_json::to_string_pretty(&val).unwrap_or_default(),
                    false,
                ),
                Err(err) => (
                    json!({
                        "schema": SCHEMA_VERSION,
                        "error": {
                            "code": "TOOL_ERROR",
                            "message": err
                        }
                    })
                    .to_string(),
                    true,
                ),
            };

            let result = json!({
                "content": [
                    {
                        "type": "text",
                        "text": text_content
                    }
                ],
                "isError": is_error
            });

            Some(JsonRpcResponse::success(req.id.clone(), result))
        }
        _ => Some(JsonRpcResponse::error(
            req.id.clone(),
            -32601,
            format!("Method not found: {}", req.method),
        )),
    }
}

/// Retorna as definições e esquemas JSON das ferramentas expostas.
#[must_use]
pub fn get_tool_definitions() -> Vec<serde_json::Value> {
    vec![
        json!({
            "name": "disk_usage",
            "description": "Agrega uso de disco por pasta a partir do índice (estilo du / WizTree). Retorna totais e pastas ordenadas por tamanho decrescente.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "target": { "type": "string", "description": "Diretório base a analisar (padrão: '.')" },
                    "top": { "type": "integer", "description": "Quantidade de maiores pastas a retornar (padrão: 15)" },
                    "depth": { "type": "integer", "description": "Profundidade máxima relativa (ex.: 1 para filhos imediatos)" },
                    "relative": { "type": "boolean", "description": "Retornar caminhos relativos ao alvo (padrão: true)" },
                    "budget": { "type": "integer", "description": "Orçamento aproximado em tokens para truncamento seguro" },
                    "all": { "type": "boolean", "description": "Incluir pastas ignoradas (.git, target, node_modules)" },
                    "include_secrets": { "type": "boolean", "description": "Incluir arquivos e pastas sensíveis sem mascarar" }
                }
            }
        }),
        json!({
            "name": "find_files",
            "description": "Busca arquivos por nome no índice do FastFile com suporte a curingas (*.rs), filtros de tamanho (>1GB), tipo (file/dir) e data.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "pattern": { "type": "string", "description": "Padrão de busca (substring ou curingas como *.rs)" },
                    "name": { "type": "string", "description": "Nome exato ou padrão alternativo" },
                    "target": { "type": "string", "description": "Diretório base da busca (padrão: '.')" },
                    "regex": { "type": "boolean", "description": "Interpretar padrão como expressão regular" },
                    "fuzzy": { "type": "boolean", "description": "Busca aproximada por similaridade de nome (fuzzy)" },
                    "ext": { "type": "string", "description": "Filtrar por extensão de arquivo (ex.: rs, toml)" },
                    "size": { "type": "string", "description": "Filtro de tamanho (ex.: '>1GB', '<10MB', '>=500KB')" },
                    "type": { "type": "string", "description": "Tipo de entrada: 'file' (f) ou 'dir' (d)" },
                    "since": { "type": "string", "description": "Modificado desde duração (ex.: '1h', '24h', '7d')" },
                    "limit": { "type": "integer", "description": "Limite máximo de resultados (padrão: 50)" },
                    "cursor": { "type": "integer", "description": "Cursor numérico para paginação" },
                    "budget": { "type": "integer", "description": "Orçamento aproximado em tokens" },
                    "relative": { "type": "boolean", "description": "Retornar caminhos relativos ao alvo (padrão: true)" },
                    "all": { "type": "boolean", "description": "Incluir pastas ignoradas (.git, target, node_modules)" },
                    "include_secrets": { "type": "boolean", "description": "Incluir arquivos sensíveis sem mascarar" }
                }
            }
        }),
        json!({
            "name": "file_stat",
            "description": "Exibe metadados completos de um arquivo ou pasta: tamanho, datas, atributos e contagem recursiva.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "target": { "type": "string", "description": "Caminho do arquivo ou diretório" },
                    "include_secrets": { "type": "boolean", "description": "Permitir consulta a arquivos sensíveis" }
                },
                "required": ["target"]
            }
        }),
        json!({
            "name": "recent_changes",
            "description": "Lista arquivos modificados recentemente no diretório especificado, ordenados por data decrescente.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "target": { "type": "string", "description": "Diretório base (padrão: '.')" },
                    "since": { "type": "string", "description": "Intervalo de modificação (padrão: '1h', ex.: '30m', '24h', '7d')" },
                    "limit": { "type": "integer", "description": "Limite de resultados (padrão: 50)" },
                    "cursor": { "type": "integer", "description": "Cursor de paginação" },
                    "budget": { "type": "integer", "description": "Orçamento aproximado em tokens" },
                    "relative": { "type": "boolean", "description": "Retornar caminhos relativos ao alvo (padrão: true)" },
                    "all": { "type": "boolean", "description": "Incluir arquivos e pastas ignorados" },
                    "include_secrets": { "type": "boolean", "description": "Incluir segredos sem mascarar" }
                }
            }
        }),
        json!({
            "name": "search_content",
            "description": "Busca texto ou expressões regulares dentro de arquivos usando o motor do ripgrep, pulando arquivos binários.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "pattern": { "type": "string", "description": "Expressão regular a buscar no conteúdo" },
                    "path": { "type": "string", "description": "Arquivo ou diretório base (padrão: '.')" },
                    "ext": { "type": "string", "description": "Filtrar por extensão de arquivo (ex.: rs, toml)" },
                    "type": { "type": "string", "description": "Filtrar por linguagem (ex.: rust, toml, python, js, ts, go)" },
                    "mode": { "type": "string", "description": "Modo de saída: 'content' (padrão), 'files' ou 'count'" },
                    "case_sensitive": { "type": "boolean", "description": "Diferenciar maiúsculas e minúsculas" },
                    "limit": { "type": "integer", "description": "Limite máximo de resultados (padrão: 100)" },
                    "budget": { "type": "integer", "description": "Orçamento aproximado em tokens" },
                    "relative": { "type": "boolean", "description": "Retornar caminhos relativos (padrão: true)" },
                    "all": { "type": "boolean", "description": "Incluir arquivos ignorados (.git, target, etc.)" },
                    "include_secrets": { "type": "boolean", "description": "Permitir busca em arquivos sensíveis" },
                    "use_index": { "type": "boolean", "description": "Usar índice de trigramas se disponível (padrão: true)" },
                    "build_index": { "type": "boolean", "description": "Criar ou atualizar o índice de trigramas antes da busca (padrão: false)" }
                },
                "required": ["pattern"]
            }
        }),
        json!({
            "name": "diff_files",
            "description": "Calcula diferenças estruturadas entre dois arquivos, divididas em blocos (hunks) e linhas unificadas.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "file_a": { "type": "string", "description": "Arquivo original (versão antiga)" },
                    "file_b": { "type": "string", "description": "Arquivo modificado (versão nova)" },
                    "context": { "type": "integer", "description": "Linhas de contexto unificado (padrão: 3)" },
                    "budget": { "type": "integer", "description": "Orçamento aproximado em tokens" },
                    "include_secrets": { "type": "boolean", "description": "Permitir comparação de arquivos sensíveis" }
                },
                "required": ["file_a", "file_b"]
            }
        }),
        json!({
            "name": "batch",
            "description": "Executa lote de múltiplas consultas (find, stat, grep, diff, sym) em uma única requisição.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "queries": {
                        "type": "array",
                        "description": "Lista de consultas a executar",
                        "items": { "type": "object" }
                    }
                },
                "required": ["queries"]
            }
        }),
        json!({
            "name": "find_symbol",
            "description": "Localiza onde uma função, struct, classe, trait, interface ou tipo está definida no código-fonte via Tree-Sitter.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "name": { "type": "string", "description": "Nome do identificador ou símbolo a buscar (ex.: AuthenticatedExecutor)" },
                    "path": { "type": "string", "description": "Arquivo ou diretório base (padrão: '.')" },
                    "kind": { "type": "string", "description": "Filtrar por tipo (fn, struct, class, trait, interface, enum, type, const)" },
                    "lang": { "type": "string", "description": "Filtrar por linguagem (rust, python, ts, js, go, cs)" },
                    "limit": { "type": "integer", "description": "Limite máximo de resultados (padrão: 50)" },
                    "cursor": { "type": "integer", "description": "Cursor numérico de paginação" },
                    "budget": { "type": "integer", "description": "Orçamento aproximado em tokens" },
                    "relative": { "type": "boolean", "description": "Retornar caminhos relativos ao alvo (padrão: true)" },
                    "all": { "type": "boolean", "description": "Incluir pastas de build e arquivos ignorados" }
                },
                "required": ["name"]
            }
        }),
        json!({
            "name": "repo_map",
            "description": "Gera uma visão compacta e estruturada de um repositório: estrutura de diretórios, linguagens, contagens, tamanhos e principais símbolos de código por arquivo.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Diretório base do repositório (padrão: '.')" },
                    "depth": { "type": "integer", "description": "Profundidade máxima da árvore de diretórios (padrão: 3)" },
                    "budget": { "type": "integer", "description": "Orçamento aproximado em tokens para compactação (ex.: 1500)" },
                    "all": { "type": "boolean", "description": "Incluir pastas de build e arquivos ignorados" },
                    "include_secrets": { "type": "boolean", "description": "Incluir segredos sem mascarar" }
                }
            }
        }),
        json!({
            "name": "find_duplicates",
            "description": "Localiza arquivos duplicados no disco agrupando por tamanho e comparando hashes (prefixo + completo) sem alterar nenhum arquivo.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Diretório base a analisar (padrão: '.')" },
                    "min_size": { "type": "string", "description": "Tamanho mínimo de arquivo (ex.: '1B', '10KB', '1MB')" },
                    "limit": { "type": "integer", "description": "Limite máximo de grupos de duplicados (padrão: 50)" },
                    "budget": { "type": "integer", "description": "Orçamento aproximado em tokens para agentes" },
                    "relative": { "type": "boolean", "description": "Retornar caminhos relativos ao alvo (padrão: true)" },
                    "all": { "type": "boolean", "description": "Incluir pastas de build e arquivos ignorados" }
                }
            }
        }),
    ]
}

/// Executa a ferramenta MCP e retorna o JSON estruturado `ff/1`.
pub fn execute_mcp_tool(name: &str, args: &serde_json::Value) -> Result<serde_json::Value, String> {
    match name {
        "disk_usage" => {
            let du_args = DuArgs {
                target: args
                    .get("target")
                    .and_then(|v| v.as_str())
                    .unwrap_or(".")
                    .to_string(),
                top: args.get("top").and_then(|v| v.as_u64()).unwrap_or(15) as usize,
                depth: args.get("depth").and_then(|v| v.as_u64()).map(|d| d as u32),
                budget: args
                    .get("budget")
                    .and_then(|v| v.as_u64())
                    .map(|b| b as usize),
                relative: args
                    .get("relative")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(true),
                all: args.get("all").and_then(|v| v.as_bool()).unwrap_or(false),
                include_secrets: args
                    .get("include_secrets")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false),
                json: true,
            };
            execute_du_json(&du_args)
        }
        "find_files" => {
            let find_args = FindArgs {
                pattern: args
                    .get("pattern")
                    .and_then(|v| v.as_str())
                    .map(String::from),
                name: args.get("name").and_then(|v| v.as_str()).map(String::from),
                target: args
                    .get("target")
                    .and_then(|v| v.as_str())
                    .unwrap_or(".")
                    .to_string(),
                regex: args.get("regex").and_then(|v| v.as_bool()).unwrap_or(false),
                fuzzy: args.get("fuzzy").and_then(|v| v.as_bool()).unwrap_or(false),
                ext: args.get("ext").and_then(|v| v.as_str()).map(String::from),
                size: args.get("size").and_then(|v| v.as_str()).map(String::from),
                file_type: args.get("type").and_then(|v| v.as_str()).map(String::from),
                since: args.get("since").and_then(|v| v.as_str()).map(String::from),
                limit: args.get("limit").and_then(|v| v.as_u64()).unwrap_or(50) as usize,
                cursor: args
                    .get("cursor")
                    .and_then(|v| v.as_u64())
                    .map(|c| c as usize),
                budget: args
                    .get("budget")
                    .and_then(|v| v.as_u64())
                    .map(|b| b as usize),
                relative: args
                    .get("relative")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(true),
                all: args.get("all").and_then(|v| v.as_bool()).unwrap_or(false),
                include_secrets: args
                    .get("include_secrets")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false),
                json: true,
                synthetic: None,
            };
            execute_find_json(&find_args)
        }
        "file_stat" => {
            let target = args
                .get("target")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "o campo 'target' é obrigatório".to_string())?;
            let stat_args = StatArgs {
                target: target.to_string(),
                include_secrets: args
                    .get("include_secrets")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false),
                json: true,
            };
            execute_stat_json(&stat_args)
        }
        "recent_changes" => {
            let recent_args = RecentArgs {
                target: args
                    .get("target")
                    .and_then(|v| v.as_str())
                    .unwrap_or(".")
                    .to_string(),
                since: args
                    .get("since")
                    .and_then(|v| v.as_str())
                    .unwrap_or("1h")
                    .to_string(),
                limit: args.get("limit").and_then(|v| v.as_u64()).unwrap_or(50) as usize,
                cursor: args
                    .get("cursor")
                    .and_then(|v| v.as_u64())
                    .map(|c| c as usize),
                budget: args
                    .get("budget")
                    .and_then(|v| v.as_u64())
                    .map(|b| b as usize),
                relative: args
                    .get("relative")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(true),
                all: args.get("all").and_then(|v| v.as_bool()).unwrap_or(false),
                include_secrets: args
                    .get("include_secrets")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false),
                json: true,
            };
            execute_recent_json(&recent_args)
        }
        "search_content" => {
            let pattern = args
                .get("pattern")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "o campo 'pattern' é obrigatório".to_string())?;
            let grep_args = GrepArgs {
                pattern: pattern.to_string(),
                path: args
                    .get("path")
                    .and_then(|v| v.as_str())
                    .unwrap_or(".")
                    .to_string(),
                ext: args.get("ext").and_then(|v| v.as_str()).map(String::from),
                file_type: args.get("type").and_then(|v| v.as_str()).map(String::from),
                mode: args
                    .get("mode")
                    .and_then(|v| v.as_str())
                    .unwrap_or("content")
                    .to_string(),
                case_sensitive: args
                    .get("case_sensitive")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false),
                limit: args.get("limit").and_then(|v| v.as_u64()).unwrap_or(100) as usize,
                budget: args
                    .get("budget")
                    .and_then(|v| v.as_u64())
                    .map(|b| b as usize),
                relative: args
                    .get("relative")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(true),
                all: args.get("all").and_then(|v| v.as_bool()).unwrap_or(false),
                include_secrets: args
                    .get("include_secrets")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false),
                no_index: args
                    .get("use_index")
                    .and_then(|v| v.as_bool())
                    .map(|u| !u)
                    .unwrap_or(false),
                build_index: args
                    .get("build_index")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false),
                json: true,
            };
            execute_grep_json(&grep_args)
        }
        "diff_files" => {
            let file_a = args
                .get("file_a")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "o campo 'file_a' é obrigatório".to_string())?;
            let file_b = args
                .get("file_b")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "o campo 'file_b' é obrigatório".to_string())?;
            let diff_args = crate::DiffArgs {
                file_a: file_a.to_string(),
                file_b: file_b.to_string(),
                context: args.get("context").and_then(|v| v.as_u64()).unwrap_or(3) as usize,
                budget: args
                    .get("budget")
                    .and_then(|v| v.as_u64())
                    .map(|b| b as usize),
                include_secrets: args
                    .get("include_secrets")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false),
                json: true,
            };
            execute_diff_json(&diff_args)
        }
        "batch" => {
            let queries = args
                .get("queries")
                .and_then(|q| q.as_array())
                .ok_or_else(|| "o campo 'queries' deve ser um array".to_string())?;
            execute_batch_json(queries)
        }
        "find_symbol" => {
            let name = args
                .get("name")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "o campo 'name' é obrigatório".to_string())?;
            let sym_args = SymArgs {
                name: name.to_string(),
                path: args
                    .get("path")
                    .and_then(|v| v.as_str())
                    .unwrap_or(".")
                    .to_string(),
                kind: args.get("kind").and_then(|v| v.as_str()).map(String::from),
                lang: args.get("lang").and_then(|v| v.as_str()).map(String::from),
                limit: args.get("limit").and_then(|v| v.as_u64()).unwrap_or(50) as usize,
                cursor: args
                    .get("cursor")
                    .and_then(|v| v.as_u64())
                    .map(|c| c as usize),
                budget: args
                    .get("budget")
                    .and_then(|v| v.as_u64())
                    .map(|b| b as usize),
                relative: args
                    .get("relative")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(true),
                all: args.get("all").and_then(|v| v.as_bool()).unwrap_or(false),
                include_secrets: false,
                json: true,
            };
            execute_sym_json(&sym_args)
        }
        "repo_map" => {
            let map_args = MapArgs {
                path: args
                    .get("path")
                    .and_then(|v| v.as_str())
                    .unwrap_or(".")
                    .to_string(),
                depth: args.get("depth").and_then(|v| v.as_u64()).unwrap_or(3) as usize,
                budget: args
                    .get("budget")
                    .and_then(|v| v.as_u64())
                    .map(|b| b as usize),
                all: args.get("all").and_then(|v| v.as_bool()).unwrap_or(false),
                include_secrets: args
                    .get("include_secrets")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false),
                json: true,
            };
            execute_map_json(&map_args)
        }
        "find_duplicates" => {
            let dupes_args = DupesArgs {
                path: args
                    .get("path")
                    .and_then(|v| v.as_str())
                    .unwrap_or(".")
                    .to_string(),
                min_size: args
                    .get("min_size")
                    .and_then(|v| v.as_str())
                    .unwrap_or("1B")
                    .to_string(),
                limit: args.get("limit").and_then(|v| v.as_u64()).unwrap_or(50) as usize,
                budget: args
                    .get("budget")
                    .and_then(|v| v.as_u64())
                    .map(|b| b as usize),
                relative: args
                    .get("relative")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(true),
                all: args.get("all").and_then(|v| v.as_bool()).unwrap_or(false),
                include_secrets: false,
                json: true,
            };
            execute_dupes_json(&dupes_args)
        }
        unknown => Err(format!("ferramenta desconhecida: '{unknown}'")),
    }
}

fn execute_du_json(args: &DuArgs) -> Result<serde_json::Value, String> {
    if !Path::new(&args.target).exists() && !args.target.ends_with(':') {
        return Err(format!("alvo não encontrado: '{}'", args.target));
    }

    let start = std::time::Instant::now();
    let mut index = ff_index::FileIndex::new();
    let mut source = ff_index::WalkSource::new(&args.target).with_respect_ignore(!args.all);

    source
        .populate(&mut index)
        .map_err(|e| format!("erro ao indexar '{}': {e}", args.target))?;

    let summary = ff_size::SizeAggregator::aggregate(&index, Some(0), args.depth, args.top);

    let mut secrets_hidden = 0usize;
    let mut results = Vec::new();

    for item in summary.items {
        let name = index.name(item.id).unwrap_or("").to_string();
        if !args.include_secrets && is_secret_file(&name) {
            secrets_hidden += 1;
            continue;
        }

        let full_path = index.build_path(item.id).unwrap_or(name);
        let final_path = if args.relative {
            make_relative_path(&full_path, &args.target)
        } else {
            full_path
        };

        results.push(crate::schema::DuResultItem {
            path: final_path,
            size: item.total_size,
            files: item.file_count,
            dirs: item.dir_count,
            depth: item.depth,
        });
    }

    let (truncated_by_budget, _) = apply_budget(&mut results, args.budget);
    let elapsed = start.elapsed();

    let mut warnings = Vec::new();
    if secrets_hidden > 0 {
        warnings.push(format!(
            "{secrets_hidden} segredo(s) ocultado(s); use include_secrets=true para exibir"
        ));
    }
    if truncated_by_budget {
        warnings.push("limite de tokens (budget) atingido; resultados truncados".to_string());
    }

    Ok(json!({
        "schema": SCHEMA_VERSION,
        "source": DEFAULT_SOURCE,
        "command": "du",
        "warnings": warnings,
        "summary": {
            "target": args.target,
            "total_size": summary.total_size,
            "total_size_formatted": format_bytes(summary.total_size),
            "total_files": summary.total_files,
            "total_dirs": summary.total_dirs,
            "top_folders_returned": results.len(),
            "secrets_hidden": secrets_hidden,
            "elapsed_ms": (elapsed.as_secs_f64() * 1000.0 * 100.0).round() / 100.0,
        },
        "results": results,
        "truncated": truncated_by_budget
    }))
}

fn execute_find_json(args: &FindArgs) -> Result<serde_json::Value, String> {
    let start = std::time::Instant::now();
    let mut index = ff_index::FileIndex::new();

    if !Path::new(&args.target).exists() && !args.target.ends_with(':') {
        return Err(format!("alvo não encontrado: '{}'", args.target));
    }

    let mut source = ff_index::WalkSource::new(&args.target).with_respect_ignore(!args.all);
    source
        .populate(&mut index)
        .map_err(|e| format!("erro ao indexar '{}': {e}", args.target))?;

    let raw_pattern = args
        .pattern
        .as_deref()
        .or(args.name.as_deref())
        .unwrap_or("*");

    let mut filters = ff_search::Filters {
        ext: args.ext.clone(),
        ..Default::default()
    };

    if let Some(ref size_spec) = args.size {
        let (min, max) = parse_size_filter(size_spec)?;
        filters.min_size = min;
        filters.max_size = max;
    }

    if let Some(ref t) = args.file_type {
        match t.to_lowercase().as_str() {
            "dir" | "d" | "directory" => filters.dirs_only = true,
            "file" | "f" => filters.files_only = true,
            _ => return Err(format!("tipo inválido '{t}': use 'file' ou 'dir'")),
        }
    }

    if let Some(ref dur_spec) = args.since {
        let dur = parse_duration(dur_spec)?;
        let now = std::time::SystemTime::now()
            .duration_since(std::time::SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        filters.min_mtime = Some(now.saturating_sub(dur.as_secs()) as i64);
    }

    let query = match ff_search::Query::parse_with_options(raw_pattern, args.regex, args.fuzzy) {
        Ok(q) => q.with_filters(filters),
        Err(err) => return Err(err.to_string()),
    };

    let result = ff_search::SearchEngine::search(&index, &query);

    let mut secrets_hidden = 0usize;
    let mut ranked_items: Vec<(crate::schema::FileResultItem, u32)> = Vec::new();

    for m in &result.matches {
        let entry = &index.entries[m.id as usize];
        let name = index.name(m.id).unwrap_or("").to_string();

        if !args.include_secrets && is_secret_file(&name) {
            secrets_hidden += 1;
            continue;
        }

        let full_path = index.build_path(m.id).unwrap_or_else(|_| name.clone());
        let final_path = if args.relative {
            make_relative_path(&full_path, &args.target)
        } else {
            full_path
        };

        let depth = final_path.split('/').count() as u32;
        let depth_penalty = depth.saturating_mul(25);
        let mtime_bonus = ((entry.mtime.max(0) / 86400) % 100) as u32;
        let rank = m.score.saturating_mul(100).saturating_sub(depth_penalty) + mtime_bonus;

        ranked_items.push((
            crate::schema::FileResultItem {
                path: final_path,
                name,
                is_dir: entry.is_dir(),
                size: entry.size,
                mtime: entry.mtime,
                mtime_iso: Some(format_unix_timestamp(entry.mtime)),
            },
            rank,
        ));
    }

    if args.fuzzy {
        ranked_items.sort_by_key(|(_, rank)| std::cmp::Reverse(*rank));
    } else {
        ranked_items.sort_by_key(|(item, _)| std::cmp::Reverse(item.mtime));
    }

    let items: Vec<crate::schema::FileResultItem> =
        ranked_items.into_iter().map(|(item, _)| item).collect();

    let total_matched = items.len();
    let offset = args.cursor.unwrap_or(0);

    let mut paginated: Vec<crate::schema::FileResultItem> = if offset < items.len() {
        items.into_iter().skip(offset).take(args.limit).collect()
    } else {
        Vec::new()
    };

    let (truncated_by_budget, _) = apply_budget(&mut paginated, args.budget);

    let next_cursor = if offset + paginated.len() < total_matched {
        Some((offset + paginated.len()).to_string())
    } else {
        None
    };
    let has_more = next_cursor.is_some() || truncated_by_budget;
    let elapsed = start.elapsed();

    let mut warnings = Vec::new();
    if secrets_hidden > 0 {
        warnings.push(format!(
            "{secrets_hidden} segredo(s) ocultado(s); use include_secrets=true para exibir"
        ));
    }
    if truncated_by_budget {
        warnings.push("limite de tokens (budget) atingido; resultados truncados".to_string());
    }

    Ok(json!({
        "schema": SCHEMA_VERSION,
        "source": DEFAULT_SOURCE,
        "command": "find",
        "warnings": warnings,
        "summary": {
            "target": args.target,
            "pattern": raw_pattern,
            "total_matches": total_matched,
            "returned_matches": paginated.len(),
            "secrets_hidden": secrets_hidden,
            "total_scanned": result.total_scanned,
            "elapsed_ms": (elapsed.as_secs_f64() * 1000.0 * 100.0).round() / 100.0,
        },
        "results": paginated,
        "cursor": next_cursor,
        "has_more": has_more,
        "truncated": truncated_by_budget
    }))
}

fn execute_stat_json(args: &StatArgs) -> Result<serde_json::Value, String> {
    let p = Path::new(&args.target);
    let meta = std::fs::symlink_metadata(p)
        .map_err(|_| format!("alvo não encontrado: '{}'", args.target))?;

    let is_dir = meta.is_dir();
    let is_file = meta.is_file();
    let size = if is_dir { 0 } else { meta.len() };

    let mtime = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::SystemTime::UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_secs() as i64);

    let readonly = meta.permissions().readonly();

    #[cfg(windows)]
    let (hidden, system) = {
        use std::os::windows::fs::MetadataExt;
        let attrs = meta.file_attributes();
        ((attrs & 0x2) != 0, (attrs & 0x4) != 0)
    };

    #[cfg(not(windows))]
    let (hidden, system) = {
        let fname = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
        (fname.starts_with('.'), false)
    };

    let file_name = p
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(&args.target)
        .to_string();

    if !args.include_secrets && is_secret_file(&file_name) {
        return Err(
            "o arquivo é considerado um segredo. Use include_secrets=true para consultar".into(),
        );
    }

    let (total_size, total_files, total_dirs) = if is_dir {
        let mut index = ff_index::FileIndex::new();
        let mut source = ff_index::WalkSource::new(&args.target).with_respect_ignore(false);
        if source.populate(&mut index).is_ok() {
            let sum = ff_size::SizeAggregator::aggregate(&index, Some(0), None, 1);
            (
                Some(sum.total_size),
                Some(sum.total_files),
                Some(sum.total_dirs),
            )
        } else {
            (None, None, None)
        }
    } else {
        (None, None, None)
    };

    Ok(json!({
        "schema": SCHEMA_VERSION,
        "source": DEFAULT_SOURCE,
        "command": "stat",
        "summary": {
            "target": args.target,
            "type": if is_dir { "directory" } else { "file" },
            "size": if is_dir { total_size.unwrap_or(0) } else { size },
            "mtime": mtime,
        },
        "result": {
            "path": args.target,
            "name": file_name,
            "is_dir": is_dir,
            "is_file": is_file,
            "size": if is_dir { total_size.unwrap_or(0) } else { size },
            "mtime": mtime,
            "mtime_iso": format_unix_timestamp(mtime),
            "readonly": readonly,
            "hidden": hidden,
            "system": system,
            "total_size": total_size,
            "total_files": total_files,
            "total_dirs": total_dirs,
        }
    }))
}

fn execute_recent_json(args: &RecentArgs) -> Result<serde_json::Value, String> {
    let dur = parse_duration(&args.since)?;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let min_mtime = now.saturating_sub(dur.as_secs()) as i64;

    let start = std::time::Instant::now();
    let mut index = ff_index::FileIndex::new();

    if !Path::new(&args.target).exists() && !args.target.ends_with(':') {
        return Err(format!("alvo não encontrado: '{}'", args.target));
    }

    let mut source = ff_index::WalkSource::new(&args.target).with_respect_ignore(!args.all);
    source
        .populate(&mut index)
        .map_err(|e| format!("erro ao indexar '{}': {e}", args.target))?;

    let query = ff_search::Query::new(ff_search::Pattern::Substring(String::new())).with_filters(
        ff_search::Filters {
            min_mtime: Some(min_mtime),
            ..Default::default()
        },
    );

    let result = ff_search::SearchEngine::search(&index, &query);

    let mut secrets_hidden = 0usize;
    let mut items = Vec::new();

    for m in &result.matches {
        let entry = &index.entries[m.id as usize];
        let name = index.name(m.id).unwrap_or("").to_string();

        if !args.include_secrets && is_secret_file(&name) {
            secrets_hidden += 1;
            continue;
        }

        let full_path = index.build_path(m.id).unwrap_or_else(|_| name.clone());
        let final_path = if args.relative {
            make_relative_path(&full_path, &args.target)
        } else {
            full_path
        };

        items.push(crate::schema::FileResultItem {
            path: final_path,
            name,
            is_dir: entry.is_dir(),
            size: entry.size,
            mtime: entry.mtime,
            mtime_iso: Some(format_unix_timestamp(entry.mtime)),
        });
    }

    items.sort_by_key(|a| std::cmp::Reverse(a.mtime));

    let total_matched = items.len();
    let offset = args.cursor.unwrap_or(0);

    let mut paginated: Vec<crate::schema::FileResultItem> = if offset < items.len() {
        items.into_iter().skip(offset).take(args.limit).collect()
    } else {
        Vec::new()
    };

    let (truncated_by_budget, _) = apply_budget(&mut paginated, args.budget);

    let next_cursor = if offset + paginated.len() < total_matched {
        Some((offset + paginated.len()).to_string())
    } else {
        None
    };
    let has_more = next_cursor.is_some() || truncated_by_budget;
    let elapsed = start.elapsed();

    let mut warnings = Vec::new();
    if secrets_hidden > 0 {
        warnings.push(format!(
            "{secrets_hidden} segredo(s) ocultado(s); use include_secrets=true para exibir"
        ));
    }
    if truncated_by_budget {
        warnings.push("limite de tokens (budget) atingido; resultados truncados".to_string());
    }

    Ok(json!({
        "schema": SCHEMA_VERSION,
        "source": DEFAULT_SOURCE,
        "command": "recent",
        "warnings": warnings,
        "summary": {
            "target": args.target,
            "since": args.since,
            "min_mtime": min_mtime,
            "total_matches": total_matched,
            "returned_matches": paginated.len(),
            "secrets_hidden": secrets_hidden,
            "elapsed_ms": (elapsed.as_secs_f64() * 1000.0 * 100.0).round() / 100.0,
        },
        "results": paginated,
        "cursor": next_cursor,
        "has_more": has_more,
        "truncated": truncated_by_budget
    }))
}

fn execute_grep_json(args: &GrepArgs) -> Result<serde_json::Value, String> {
    let mode = match args.mode.to_lowercase().as_str() {
        "content" => ff_content::GrepMode::Content,
        "files" => ff_content::GrepMode::Files,
        "count" => ff_content::GrepMode::Count,
        other => {
            return Err(format!(
                "modo inválido '{other}': use 'content', 'files' ou 'count'"
            ))
        }
    };

    let p = Path::new(&args.path);
    if !p.exists() {
        return Err(format!("alvo não encontrado: '{}'", args.path));
    }

    if args.build_index && p.is_dir() {
        let config = ff_content::TrigramConfig::default();
        let idx_path = ff_content::default_index_path(p);
        let mut idx = if idx_path.exists() {
            ff_content::TrigramIndex::load_from_file(p, &idx_path)
                .unwrap_or_else(|_| ff_content::TrigramIndex::new(p, config.clone()))
        } else {
            ff_content::TrigramIndex::new(p, config.clone())
        };
        let _ = idx.build_or_update();
        let _ = idx.save_to_file(&idx_path);
    }

    let opts = ff_content::GrepOptions {
        pattern: args.pattern.clone(),
        target: p.to_path_buf(),
        mode,
        case_insensitive: !args.case_sensitive,
        ext: args.ext.clone(),
        file_type: args.file_type.clone(),
        limit: args.limit,
        all: args.all,
        use_index: !args.no_index,
        index_path: None,
    };

    let mut report = ff_content::grep(&opts)?;
    let mut secrets_hidden = 0usize;

    if !args.include_secrets {
        if let Some(ref mut matches) = report.matches {
            matches.retain(|m| {
                let fname = Path::new(&m.path)
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("");
                if is_secret_file(fname) {
                    secrets_hidden += 1;
                    false
                } else {
                    true
                }
            });
        }
        if let Some(ref mut files) = report.files {
            files.retain(|f| {
                let fname = Path::new(f)
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("");
                if is_secret_file(fname) {
                    secrets_hidden += 1;
                    false
                } else {
                    true
                }
            });
        }
        if let Some(ref mut counts) = report.counts {
            counts.retain(|c| {
                let fname = Path::new(&c.path)
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("");
                if is_secret_file(fname) {
                    secrets_hidden += 1;
                    false
                } else {
                    true
                }
            });
        }
    }

    if args.relative {
        let base = &args.path;
        if let Some(ref mut matches) = report.matches {
            for m in matches.iter_mut() {
                m.path = make_relative_path(&m.path, base);
            }
        }
        if let Some(ref mut files) = report.files {
            for f in files.iter_mut() {
                *f = make_relative_path(f, base);
            }
        }
        if let Some(ref mut counts) = report.counts {
            for c in counts.iter_mut() {
                c.path = make_relative_path(&c.path, base);
            }
        }
    }

    let truncated_by_budget = if let Some(ref mut matches) = report.matches {
        let (t, _) = apply_budget(matches, args.budget);
        t
    } else if let Some(ref mut files) = report.files {
        let (t, _) = apply_budget(files, args.budget);
        t
    } else if let Some(ref mut counts) = report.counts {
        let (t, _) = apply_budget(counts, args.budget);
        t
    } else {
        false
    };

    let mut warnings = Vec::new();
    if secrets_hidden > 0 {
        warnings.push(format!(
            "{secrets_hidden} segredo(s) ocultado(s); use include_secrets=true para exibir"
        ));
    }
    if truncated_by_budget {
        warnings.push("limite de tokens (budget) atingido; resultados truncados".to_string());
    }

    Ok(json!({
        "schema": SCHEMA_VERSION,
        "source": DEFAULT_SOURCE,
        "command": "grep",
        "warnings": warnings,
        "summary": {
            "pattern": report.summary.pattern,
            "target": report.summary.target,
            "mode": args.mode,
            "total_matches": report.summary.total_matches,
            "files_searched": report.summary.files_searched,
            "files_matched": report.summary.files_matched,
            "secrets_hidden": secrets_hidden,
            "elapsed_ms": report.summary.elapsed_ms,
        },
        "result": report,
        "truncated": truncated_by_budget
    }))
}

fn execute_diff_json(args: &crate::DiffArgs) -> Result<serde_json::Value, String> {
    let p_a = Path::new(&args.file_a);
    let p_b = Path::new(&args.file_b);

    if !p_a.exists() {
        return Err(format!("alvo não encontrado: '{}'", args.file_a));
    }
    if !p_b.exists() {
        return Err(format!("alvo não encontrado: '{}'", args.file_b));
    }

    if !args.include_secrets {
        let name_a = p_a.file_name().and_then(|n| n.to_str()).unwrap_or("");
        let name_b = p_b.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if is_secret_file(name_a) || is_secret_file(name_b) {
            return Err(
                "o arquivo é considerado um segredo. Use include_secrets=true para comparar".into(),
            );
        }
    }

    let mut report = ff_diff::diff_files(p_a, p_b, args.context)
        .map_err(|e| format!("erro ao calcular diff: {e}"))?;

    let (truncated_by_budget, _) = apply_budget(&mut report.hunks, args.budget);

    Ok(json!({
        "schema": SCHEMA_VERSION,
        "source": DEFAULT_SOURCE,
        "command": "diff",
        "summary": {
            "file_a": report.file_a,
            "file_b": report.file_b,
            "additions": report.additions,
            "deletions": report.deletions,
            "is_identical": report.is_identical,
            "hunks_count": report.hunks.len(),
        },
        "result": report,
        "truncated": truncated_by_budget
    }))
}

fn execute_batch_json(queries: &[serde_json::Value]) -> Result<serde_json::Value, String> {
    let mut batch_results = Vec::new();

    for q in queries {
        let cmd = q.get("command").and_then(|c| c.as_str()).unwrap_or("");
        let res = match cmd {
            "stat" => {
                let target = q.get("target").and_then(|t| t.as_str()).unwrap_or(".");
                let stat_args = StatArgs {
                    target: target.to_string(),
                    include_secrets: q
                        .get("include_secrets")
                        .and_then(|s| s.as_bool())
                        .unwrap_or(false),
                    json: true,
                };
                execute_stat_json(&stat_args)
            }
            "find" => {
                let find_args = FindArgs {
                    pattern: q.get("pattern").and_then(|p| p.as_str()).map(String::from),
                    name: None,
                    target: q
                        .get("target")
                        .and_then(|t| t.as_str())
                        .unwrap_or(".")
                        .to_string(),
                    regex: false,
                    fuzzy: q.get("fuzzy").and_then(|f| f.as_bool()).unwrap_or(false),
                    ext: q.get("ext").and_then(|e| e.as_str()).map(String::from),
                    size: q.get("size").and_then(|s| s.as_str()).map(String::from),
                    file_type: q.get("type").and_then(|t| t.as_str()).map(String::from),
                    since: q.get("since").and_then(|s| s.as_str()).map(String::from),
                    limit: q.get("limit").and_then(|l| l.as_u64()).unwrap_or(20) as usize,
                    cursor: None,
                    budget: None,
                    relative: true,
                    all: false,
                    include_secrets: false,
                    json: true,
                    synthetic: None,
                };
                execute_find_json(&find_args)
            }
            "du" => {
                let du_args = DuArgs {
                    target: q
                        .get("target")
                        .and_then(|t| t.as_str())
                        .unwrap_or(".")
                        .to_string(),
                    top: q.get("top").and_then(|t| t.as_u64()).unwrap_or(10) as usize,
                    depth: q.get("depth").and_then(|d| d.as_u64()).map(|d| d as u32),
                    budget: None,
                    relative: true,
                    all: false,
                    include_secrets: false,
                    json: true,
                };
                execute_du_json(&du_args)
            }
            "grep" => {
                let pattern = q.get("pattern").and_then(|p| p.as_str()).unwrap_or("");
                let grep_args = GrepArgs {
                    pattern: pattern.to_string(),
                    path: q
                        .get("path")
                        .and_then(|p| p.as_str())
                        .unwrap_or(".")
                        .to_string(),
                    ext: q.get("ext").and_then(|e| e.as_str()).map(String::from),
                    file_type: q.get("type").and_then(|t| t.as_str()).map(String::from),
                    mode: "content".into(),
                    case_sensitive: false,
                    limit: q.get("limit").and_then(|l| l.as_u64()).unwrap_or(20) as usize,
                    budget: None,
                    relative: true,
                    all: false,
                    include_secrets: false,
                    no_index: false,
                    build_index: false,
                    json: true,
                };
                execute_grep_json(&grep_args)
            }
            "sym" | "find_symbol" => {
                let name = q.get("name").and_then(|p| p.as_str()).unwrap_or("");
                let sym_args = SymArgs {
                    name: name.to_string(),
                    path: q
                        .get("path")
                        .and_then(|p| p.as_str())
                        .unwrap_or(".")
                        .to_string(),
                    kind: q.get("kind").and_then(|k| k.as_str()).map(String::from),
                    lang: q.get("lang").and_then(|l| l.as_str()).map(String::from),
                    limit: q.get("limit").and_then(|l| l.as_u64()).unwrap_or(20) as usize,
                    cursor: None,
                    budget: None,
                    relative: true,
                    all: false,
                    include_secrets: false,
                    json: true,
                };
                execute_sym_json(&sym_args)
            }
            other => Err(format!("comando desconhecido em batch: '{other}'")),
        };

        match res {
            Ok(v) => batch_results.push(v),
            Err(e) => batch_results.push(json!({
                "error": e,
                "status": "error"
            })),
        }
    }

    Ok(json!({
        "schema": SCHEMA_VERSION,
        "source": DEFAULT_SOURCE,
        "command": "batch",
        "summary": {
            "total_queries": queries.len(),
            "processed": batch_results.len()
        },
        "results": batch_results
    }))
}

fn execute_sym_json(args: &SymArgs) -> Result<serde_json::Value, String> {
    let base_path = Path::new(&args.path);
    if !base_path.exists() {
        return Err(format!("alvo não encontrado: '{}'", args.path));
    }

    let kind_filter = args.kind.as_deref().and_then(ff_sym::SymbolKind::parse);
    if args.kind.is_some() && kind_filter.is_none() {
        return Err(format!(
            "tipo de símbolo inválido '{}': use fn, struct, class, trait, interface, enum, type ou const",
            args.kind.as_deref().unwrap_or("")
        ));
    }

    let start_time = std::time::Instant::now();
    let engine = ff_sym::SymbolEngine::new();
    let options = ff_sym::SymOptions {
        name: Some(args.name.clone()),
        path: base_path.to_path_buf(),
        kind: kind_filter,
        lang: args.lang.clone(),
        limit: args.limit.saturating_mul(4).max(100),
        all: args.all,
    };

    let all_symbols = engine.search(&options);
    let total_matches = all_symbols.len();

    let mut filtered_symbols: Vec<SymbolResultItem> = Vec::new();
    let mut secrets_hidden = 0usize;

    for s in all_symbols {
        let p = Path::new(&s.path);
        let fname = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if !args.include_secrets && is_secret_file(fname) {
            secrets_hidden += 1;
            continue;
        }

        let out_path = if args.relative {
            make_relative_path(&s.path, &args.path)
        } else {
            s.path.clone()
        };

        filtered_symbols.push(SymbolResultItem {
            name: s.name,
            kind: s.kind.as_str().to_string(),
            path: out_path,
            line: s.line,
            column: s.column,
            signature: s.signature,
            language: s.language,
        });
    }

    let start_idx = args.cursor.unwrap_or(0);
    let mut page: Vec<SymbolResultItem> = filtered_symbols
        .into_iter()
        .skip(start_idx)
        .take(args.limit)
        .collect();

    let returned_before_budget = page.len();
    let (truncated_by_budget, _) = apply_budget(&mut page, args.budget);

    let next_cursor = if start_idx + returned_before_budget < total_matches {
        Some((start_idx + returned_before_budget).to_string())
    } else {
        None
    };
    let has_more = next_cursor.is_some();
    let elapsed = start_time.elapsed().as_secs_f64() * 1000.0;

    let mut warnings = Vec::new();
    if secrets_hidden > 0 {
        warnings.push(format!(
            "{secrets_hidden} segredo(s) ocultado(s); use include_secrets=true para exibir"
        ));
    }
    if truncated_by_budget {
        warnings.push("limite de tokens (budget) atingido; resultados truncados".to_string());
    }

    Ok(json!({
        "schema": SCHEMA_VERSION,
        "source": DEFAULT_SOURCE,
        "command": "sym",
        "warnings": warnings,
        "summary": {
            "name": args.name,
            "target": args.path,
            "kind": args.kind,
            "lang": args.lang,
            "total_matches": total_matches,
            "returned_matches": page.len(),
            "secrets_hidden": secrets_hidden,
            "elapsed_ms": (elapsed * 100.0).round() / 100.0,
        },
        "results": page,
        "cursor": next_cursor,
        "has_more": if has_more { Some(true) } else { None },
        "truncated": if truncated_by_budget { Some(true) } else { None }
    }))
}

fn execute_map_json(args: &MapArgs) -> Result<serde_json::Value, String> {
    let base_path = Path::new(&args.path);
    if !base_path.exists() {
        return Err(format!("diretório não encontrado: '{}'", args.path));
    }

    let options = crate::map::MapOptions {
        root: base_path.to_path_buf(),
        depth: args.depth,
        budget: args.budget,
        all: args.all,
        include_secrets: args.include_secrets,
    };

    let (repo_map, truncated, used_tokens) = crate::map::generate_repo_map(&options)?;

    let summary = serde_json::json!({
        "root": repo_map.root,
        "total_files": repo_map.total_files,
        "total_dirs": repo_map.total_dirs,
        "total_bytes": repo_map.total_bytes,
        "languages_count": repo_map.languages.len(),
        "modules_count": repo_map.modules.len(),
        "budget": args.budget,
        "budget_used": used_tokens,
        "truncated": truncated,
    });

    let mut env = JsonEnvelope::new("map", summary).with_result(repo_map);
    if truncated {
        env = env.with_truncated(true);
    }

    serde_json::to_value(&env).map_err(|e| e.to_string())
}

fn execute_dupes_json(args: &DupesArgs) -> Result<serde_json::Value, String> {
    let base_path = Path::new(&args.path);
    if !base_path.exists() {
        return Err(format!("diretório não encontrado: '{}'", args.path));
    }

    let min_size_bytes = match parse_size_filter(&args.min_size) {
        Ok((Some(min), _)) => min,
        Ok((None, _)) => 1,
        Err(_) => crate::util::parse_bytes_value(&args.min_size).unwrap_or(1),
    };

    let options = crate::dupes::DupesOptions {
        target: base_path.to_path_buf(),
        min_size: min_size_bytes,
        limit: args.limit,
        relative: args.relative,
        all: args.all,
        include_secrets: args.include_secrets,
    };

    let start = std::time::Instant::now();
    let mut dupes_result = crate::dupes::find_duplicates(&options)?;
    let elapsed = start.elapsed().as_secs_f64() * 1000.0;

    let total_groups_found = dupes_result.groups.len();
    dupes_result.groups.truncate(args.limit);

    let mut truncated = total_groups_found > args.limit;
    if let Some(budget) = args.budget {
        while !dupes_result.groups.is_empty() {
            let used = match serde_json::to_string(&dupes_result) {
                Ok(s) => s.len().div_ceil(4),
                Err(_) => 0,
            };
            if used <= budget {
                break;
            }
            dupes_result.groups.pop();
            truncated = true;
        }
    }

    let summary = serde_json::json!({
        "target": dupes_result.target,
        "total_scanned_files": dupes_result.total_scanned_files,
        "candidate_files": dupes_result.candidate_files,
        "duplicate_groups": dupes_result.duplicate_groups,
        "duplicate_files": dupes_result.duplicate_files,
        "wasted_bytes": dupes_result.wasted_bytes,
        "wasted_bytes_human": format_bytes(dupes_result.wasted_bytes),
        "elapsed_ms": (elapsed * 100.0).round() / 100.0,
        "budget": args.budget,
        "truncated": truncated,
    });

    let mut env = JsonEnvelope::new("dupes", summary).with_result(dupes_result);
    if truncated {
        env = env.with_truncated(true);
    }

    serde_json::to_value(&env).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mcp_initialize() {
        let req = JsonRpcRequest {
            jsonrpc: "2.0".into(),
            id: Some(json!(1)),
            method: "initialize".into(),
            params: Some(json!({})),
        };

        let resp = handle_request(&req).unwrap();
        assert_eq!(resp.id, Some(json!(1)));
        assert!(resp.result.is_some());

        let res = resp.result.unwrap();
        assert_eq!(res["serverInfo"]["name"], "fastfile");
    }

    #[test]
    fn test_mcp_tools_list() {
        let req = JsonRpcRequest {
            jsonrpc: "2.0".into(),
            id: Some(json!(2)),
            method: "tools/list".into(),
            params: Some(json!({})),
        };

        let resp = handle_request(&req).unwrap();
        let res = resp.result.unwrap();
        let tools = res["tools"].as_array().unwrap();

        assert!(tools.iter().any(|t| t["name"] == "find_files"));
        assert!(tools.iter().any(|t| t["name"] == "disk_usage"));
        assert!(tools.iter().any(|t| t["name"] == "file_stat"));
        assert!(tools.iter().any(|t| t["name"] == "recent_changes"));
        assert!(tools.iter().any(|t| t["name"] == "search_content"));
        assert!(tools.iter().any(|t| t["name"] == "diff_files"));
        assert!(tools.iter().any(|t| t["name"] == "batch"));
        assert!(tools.iter().any(|t| t["name"] == "find_symbol"));
        assert!(tools.iter().any(|t| t["name"] == "repo_map"));
        assert!(tools.iter().any(|t| t["name"] == "find_duplicates"));
    }

    #[test]
    fn test_mcp_tools_call_find_symbol() {
        let req = JsonRpcRequest {
            jsonrpc: "2.0".into(),
            id: Some(json!(3)),
            method: "tools/call".into(),
            params: Some(json!({
                "name": "find_symbol",
                "arguments": {
                    "name": "run_mcp_server",
                    "limit": 5
                }
            })),
        };

        let resp = handle_request(&req).unwrap();
        let res = resp.result.unwrap();
        assert_eq!(res["isError"], false);
        let content_text = res["content"][0]["text"].as_str().unwrap();
        assert!(content_text.contains("\"schema\": \"ff/1\""));
    }

    #[test]
    fn test_mcp_tools_call_find() {
        let req = JsonRpcRequest {
            jsonrpc: "2.0".into(),
            id: Some(json!(3)),
            method: "tools/call".into(),
            params: Some(json!({
                "name": "find_files",
                "arguments": {
                    "pattern": "Cargo.toml",
                    "limit": 5
                }
            })),
        };

        let resp = handle_request(&req).unwrap();
        let res = resp.result.unwrap();
        assert_eq!(res["isError"], false);
        let content_text = res["content"][0]["text"].as_str().unwrap();
        assert!(content_text.contains("\"schema\": \"ff/1\""));
    }

    #[test]
    fn test_mcp_tools_call_repo_map() {
        let req = JsonRpcRequest {
            jsonrpc: "2.0".into(),
            id: Some(json!(5)),
            method: "tools/call".into(),
            params: Some(json!({
                "name": "repo_map",
                "arguments": {
                    "path": ".",
                    "depth": 2,
                    "budget": 1500
                }
            })),
        };

        let resp = handle_request(&req).unwrap();
        let res = resp.result.unwrap();
        assert_eq!(res["isError"], false);
        let content_text = res["content"][0]["text"].as_str().unwrap();
        assert!(content_text.contains("\"schema\": \"ff/1\""));
        assert!(content_text.contains("\"command\": \"map\""));
    }

    #[test]
    fn test_mcp_tools_call_find_duplicates() {
        let req = JsonRpcRequest {
            jsonrpc: "2.0".into(),
            id: Some(json!(6)),
            method: "tools/call".into(),
            params: Some(json!({
                "name": "find_duplicates",
                "arguments": {
                    "path": ".",
                    "limit": 5
                }
            })),
        };

        let resp = handle_request(&req).unwrap();
        let res = resp.result.unwrap();
        assert_eq!(res["isError"], false);
        let content_text = res["content"][0]["text"].as_str().unwrap();
        assert!(content_text.contains("\"schema\": \"ff/1\""));
        assert!(content_text.contains("\"command\": \"dupes\""));
    }

    #[test]
    fn test_mcp_ping() {
        let req = JsonRpcRequest {
            jsonrpc: "2.0".into(),
            id: Some(json!(4)),
            method: "ping".into(),
            params: None,
        };

        let resp = handle_request(&req).unwrap();
        assert!(resp.result.is_some());
    }
}
