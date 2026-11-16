//! `ff-indexer`
//!
//! Daemon de indexação em segundo plano com IPC tipada e segura (§1, §3 e #11).
//! Mantém o índice residente em memória e serve consultas para o cliente `ff` sem exigir elevação na UI.

use std::io::{self, BufRead, BufReader, Write};
use std::sync::{Arc, RwLock};
use std::time::Instant;

use ff_index::FileIndex;
use ff_search::Query;
use ff_size::SizeAggregator;
use serde::{Deserialize, Serialize};

pub mod service;

pub const IPC_SCHEMA_VERSION: &str = "ff/ipc/1";

/// Retorna o identificador padrão do endpoint IPC para o usuário atual.
#[must_use]
pub fn default_endpoint_name() -> String {
    let user = std::env::var("USERNAME")
        .or_else(|_| std::env::var("USER"))
        .unwrap_or_else(|_| "default".to_string());

    #[cfg(windows)]
    {
        format!(r"\\.\pipe\fastfile-{user}")
    }
    #[cfg(not(windows))]
    {
        let runtime = std::env::var("XDG_RUNTIME_DIR").unwrap_or_else(|_| "/tmp".to_string());
        format!("{runtime}/fastfile-{user}.sock")
    }
}

/// Requisições suportadas pelo protocolo tipado do indexador.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", content = "payload")]
pub enum IpcRequest {
    /// Teste de conectividade e latência.
    Ping,
    /// Informações sobre o estado do índice residente.
    Status,
    /// Busca por nome/padrão no índice.
    Find {
        pattern: String,
        is_regex: bool,
        fuzzy: bool,
        limit: usize,
    },
    /// Agregação de tamanho por pasta (disco).
    Du {
        target: String,
        depth: Option<u32>,
        top: usize,
    },
    /// Metadados de um arquivo ou diretório.
    Stat { path: String },
}

/// Respostas estruturadas retornadas pelo indexador.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "status", content = "data")]
pub enum IpcResponse {
    Ok(serde_json::Value),
    Error(String),
}

/// Manipulador de requisições sobre o índice compartilhado.
pub struct RequestHandler {
    index: Arc<RwLock<FileIndex>>,
    start_time: Instant,
}

impl RequestHandler {
    #[must_use]
    pub fn new(index: Arc<RwLock<FileIndex>>) -> Self {
        Self {
            index,
            start_time: Instant::now(),
        }
    }

    /// Processa uma requisição e produz a resposta correspondente.
    pub fn handle(&self, req: IpcRequest) -> IpcResponse {
        match req {
            IpcRequest::Ping => IpcResponse::Ok(serde_json::json!({
                "pong": true,
                "uptime_secs": self.start_time.elapsed().as_secs(),
            })),
            IpcRequest::Status => {
                let idx = self.index.read().unwrap();
                let mem = idx.memory_usage();
                IpcResponse::Ok(serde_json::json!({
                    "schema": IPC_SCHEMA_VERSION,
                    "volume": idx.volume,
                    "entries": idx.len(),
                    "memory_bytes": mem,
                    "memory_mb": (mem as f64 / (1024.0 * 1024.0) * 100.0).round() / 100.0,
                    "journal_id": idx.journal_id,
                    "last_usn": idx.last_usn,
                    "uptime_secs": self.start_time.elapsed().as_secs(),
                }))
            }
            IpcRequest::Find {
                pattern,
                is_regex,
                fuzzy,
                limit,
            } => {
                let query = match Query::parse_with_options(&pattern, is_regex, fuzzy) {
                    Ok(q) => q,
                    Err(e) => return IpcResponse::Error(format!("consulta inválida: {e}")),
                };

                let idx = self.index.read().unwrap();
                let result = ff_search::SearchEngine::search(&idx, &query);

                let mut matches = Vec::new();
                for mat in result.matches.iter().take(limit) {
                    if let Ok(path) = idx.build_path(mat.id) {
                        let entry = &idx.entries[mat.id as usize];
                        matches.push(serde_json::json!({
                            "id": mat.id,
                            "path": path,
                            "size": entry.size,
                            "is_dir": entry.is_dir(),
                            "score": mat.score,
                        }));
                    }
                }

                IpcResponse::Ok(serde_json::json!({
                    "total_matched": result.matches.len(),
                    "total_scanned": result.total_scanned,
                    "elapsed_ms": (result.elapsed.as_secs_f64() * 1000.0 * 100.0).round() / 100.0,
                    "matches": matches,
                }))
            }
            IpcRequest::Du { target, depth, top } => {
                let idx = self.index.read().unwrap();
                let root_id = if target == "." || target == idx.volume || target.is_empty() {
                    None
                } else {
                    idx.entries
                        .iter()
                        .position(|e| {
                            if !e.is_dir() || e.is_deleted() {
                                return false;
                            }
                            idx.build_path(e.parent)
                                .map(|p| p == target)
                                .unwrap_or(false)
                        })
                        .map(|id| id as u32)
                };

                let summary = SizeAggregator::aggregate(&idx, root_id, depth, top);
                let items: Vec<serde_json::Value> = summary
                    .items
                    .into_iter()
                    .map(|it| {
                        let path = idx.build_path(it.id).unwrap_or_default();
                        serde_json::json!({
                            "path": path,
                            "size": it.total_size,
                            "files": it.file_count,
                            "dirs": it.dir_count,
                            "depth": it.depth,
                        })
                    })
                    .collect();

                IpcResponse::Ok(serde_json::json!({
                    "target": target,
                    "total_size": summary.total_size,
                    "total_files": summary.total_files,
                    "total_dirs": summary.total_dirs,
                    "items": items,
                }))
            }
            IpcRequest::Stat { path } => {
                let idx = self.index.read().unwrap();
                let found = idx.entries.iter().enumerate().find(|(id, e)| {
                    if e.is_deleted() {
                        return false;
                    }
                    idx.build_path(*id as u32)
                        .map(|p| p.eq_ignore_ascii_case(&path))
                        .unwrap_or(false)
                });

                if let Some((id, entry)) = found {
                    IpcResponse::Ok(serde_json::json!({
                        "id": id,
                        "path": path,
                        "size": entry.size,
                        "is_dir": entry.is_dir(),
                        "is_file": entry.is_file(),
                        "mtime": entry.mtime,
                        "flags": entry.flags,
                    }))
                } else {
                    IpcResponse::Error(format!("caminho '{path}' não encontrado no índice"))
                }
            }
        }
    }
}

/// Cliente IPC para comunicação com o indexador residente.
pub struct IpcClient {
    endpoint: String,
}

impl IpcClient {
    #[must_use]
    pub fn new(endpoint: impl Into<String>) -> Self {
        Self {
            endpoint: endpoint.into(),
        }
    }

    /// Cria um cliente com o endpoint padrão do usuário.
    #[must_use]
    pub fn default_client() -> Self {
        Self::new(default_endpoint_name())
    }

    /// Envia uma requisição ao indexador e aguarda a resposta estruturada.
    pub fn send(&self, request: &IpcRequest) -> io::Result<IpcResponse> {
        let req_json = serde_json::to_string(request)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?;

        #[cfg(windows)]
        {
            use std::fs::OpenOptions;

            let mut file = OpenOptions::new()
                .read(true)
                .write(true)
                .open(&self.endpoint)?;

            writeln!(file, "{req_json}")?;
            file.flush()?;

            let mut reader = BufReader::new(file);
            let mut line = String::new();
            reader.read_line(&mut line)?;

            serde_json::from_str(&line).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
        }

        #[cfg(not(windows))]
        {
            use std::os::unix::net::UnixStream;
            let mut stream = UnixStream::connect(&self.endpoint)?;
            writeln!(stream, "{req_json}")?;
            stream.flush()?;

            let mut reader = BufReader::new(stream);
            let mut line = String::new();
            reader.read_line(&mut line)?;

            serde_json::from_str(&line).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ipc_handler_ping_and_status() {
        let index = Arc::new(RwLock::new(FileIndex::new()));
        let handler = RequestHandler::new(index);

        let ping_res = handler.handle(IpcRequest::Ping);
        match ping_res {
            IpcResponse::Ok(val) => {
                assert_eq!(val["pong"], true);
            }
            IpcResponse::Error(e) => panic!("esperava Ok: {e}"),
        }

        let status_res = handler.handle(IpcRequest::Status);
        match status_res {
            IpcResponse::Ok(val) => {
                assert_eq!(val["schema"], IPC_SCHEMA_VERSION);
                assert_eq!(val["entries"], 0);
            }
            IpcResponse::Error(e) => panic!("esperava Ok: {e}"),
        }
    }
}
