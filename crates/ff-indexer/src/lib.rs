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
        #[serde(default)]
        is_regex: bool,
        #[serde(default)]
        fuzzy: bool,
        #[serde(default)]
        limit: usize,
        #[serde(default)]
        target: Option<String>,
        #[serde(default)]
        extension: Option<String>,
        #[serde(default)]
        file_type: Option<String>,
        #[serde(default)]
        min_size: Option<u64>,
        #[serde(default)]
        max_size: Option<u64>,
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
                target,
                extension,
                file_type,
                min_size,
                max_size,
            } => {
                let query = match Query::parse_with_options(&pattern, is_regex, fuzzy) {
                    Ok(q) => q,
                    Err(e) => return IpcResponse::Error(format!("consulta inválida: {e}")),
                };

                let idx = self.index.read().unwrap();
                let result = ff_search::SearchEngine::search(&idx, &query);

                // Normaliza o target se especificado
                let norm_target = target
                    .as_deref()
                    .map(|t| t.trim_end_matches(['\\', '/']).to_lowercase());

                let mut matches = Vec::new();
                for mat in &result.matches {
                    let entry = match idx.entries.get(mat.id as usize) {
                        Some(e) if !e.is_deleted() => e,
                        _ => continue,
                    };

                    // Filtro de tamanho
                    if let Some(min_s) = min_size {
                        if entry.size < min_s {
                            continue;
                        }
                    }
                    if let Some(max_s) = max_size {
                        if entry.size > max_s {
                            continue;
                        }
                    }

                    // Filtro de tipo (file ou dir)
                    if let Some(ref ft) = file_type {
                        let ft_lc = ft.to_lowercase();
                        let mismatch_file = (ft_lc == "f" || ft_lc == "file") && !entry.is_file();
                        let mismatch_dir = (ft_lc == "d" || ft_lc == "dir" || ft_lc == "directory")
                            && !entry.is_dir();
                        if mismatch_file || mismatch_dir {
                            continue;
                        }
                    }

                    let path = match idx.build_path(mat.id) {
                        Ok(p) => p,
                        Err(_) => continue,
                    };

                    // Filtro de target (prefixo de caminho)
                    if let Some(ref nt) = norm_target {
                        let p_lc = path.to_lowercase();
                        if !p_lc.starts_with(nt) {
                            continue;
                        }
                    }

                    // Filtro de extensão
                    if let Some(ref ext) = extension {
                        let ext_clean = ext.trim_start_matches('.').to_lowercase();
                        let actual_ext = std::path::Path::new(&path)
                            .extension()
                            .and_then(|e| e.to_str())
                            .unwrap_or("")
                            .to_lowercase();
                        if actual_ext != ext_clean {
                            continue;
                        }
                    }

                    let name = std::path::Path::new(&path)
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("")
                        .to_string();

                    matches.push(serde_json::json!({
                        "id": mat.id,
                        "path": path,
                        "name": name,
                        "size": entry.size,
                        "mtime": entry.mtime,
                        "is_dir": entry.is_dir(),
                        "score": mat.score,
                    }));

                    if limit > 0 && matches.len() >= limit {
                        break;
                    }
                }

                IpcResponse::Ok(serde_json::json!({
                    "total_matched": matches.len(),
                    "total_scanned": result.total_scanned,
                    "elapsed_ms": (result.elapsed.as_secs_f64() * 1000.0 * 100.0).round() / 100.0,
                    "matches": matches,
                }))
            }
            IpcRequest::Du { target, depth, top } => {
                let idx = self.index.read().unwrap();
                let clean_target = target.trim();
                let norm_target = clean_target.trim_end_matches(['\\', '/']);

                let is_root = norm_target == "."
                    || norm_target.is_empty()
                    || norm_target.eq_ignore_ascii_case(&idx.volume)
                    || norm_target
                        .eq_ignore_ascii_case(&format!("{}:", idx.volume.trim_end_matches(':')))
                    || norm_target.eq_ignore_ascii_case(&format!(
                        "{}:\\",
                        idx.volume.trim_end_matches([':', '\\'])
                    ));

                let root_id = if is_root {
                    None
                } else {
                    // Resolve o ID da PRÓPRIA pasta alvo no índice
                    let found = idx.entries.iter().enumerate().find(|(id, e)| {
                        if !e.is_dir() || e.is_deleted() {
                            return false;
                        }
                        if let Ok(entry_path) = idx.build_path(*id as u32) {
                            let entry_norm = entry_path.trim_end_matches(['\\', '/']);
                            entry_norm.eq_ignore_ascii_case(norm_target)
                        } else {
                            false
                        }
                    });

                    match found {
                        Some((id, _)) => Some(id as u32),
                        None => {
                            return IpcResponse::Error(format!(
                                "alvo '{target}' não encontrado no índice coberto pelo daemon (volume '{}')",
                                idx.volume
                            ));
                        }
                    }
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

    /// Envia uma requisição ao indexador com limite de tempo estrito (CORE-04).
    pub fn send_timeout(
        &self,
        request: &IpcRequest,
        timeout: std::time::Duration,
    ) -> io::Result<IpcResponse> {
        let (tx, rx) = std::sync::mpsc::channel();
        let endpoint = self.endpoint.clone();
        let req = request.clone();

        std::thread::spawn(move || {
            let client = IpcClient::new(endpoint);
            let res = client.send(&req);
            let _ = tx.send(res);
        });

        match rx.recv_timeout(timeout) {
            Ok(res) => res,
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "tempo limite esgotado ao aguardar resposta do indexador residente",
            )),
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => Err(io::Error::new(
                io::ErrorKind::ConnectionAborted,
                "canal de comunicação IPC encerrado inesperadamente",
            )),
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

    #[test]
    fn test_ipc_handler_find_filters_and_du_target() {
        let mut idx = FileIndex::new();
        let root = if cfg!(windows) { "C:" } else { "/" };
        let projects = if cfg!(windows) {
            "C:\\Projects"
        } else {
            "/Projects"
        };
        let fastfile = if cfg!(windows) {
            "C:\\Projects\\fastfile"
        } else {
            "/Projects/fastfile"
        };
        let missing = if cfg!(windows) {
            "C:\\NaoExiste"
        } else {
            "/NaoExiste"
        };
        idx.volume = root.to_string();
        // Cria estrutura de pastas e arquivos:
        // C:\ (dir raiz, id 0)
        //   Projects\ (dir, id 1, parent 0)
        //     fastfile\ (dir, id 2, parent 1)
        //       main.rs (file, id 3, parent 2, size 1000)
        //       doc.md  (file, id 4, parent 2, size 500)
        //   Windows\ (dir, id 5, parent 0)
        //     notepad.exe (file, id 6, parent 5, size 2000)
        let d_root = idx
            .add_entry(0, ff_index::NO_PARENT, root, ff_index::FLAG_DIRECTORY, 0, 0)
            .unwrap();
        let d_proj = idx
            .add_entry(1, d_root, "Projects", ff_index::FLAG_DIRECTORY, 0, 0)
            .unwrap();
        let d_ff = idx
            .add_entry(2, d_proj, "fastfile", ff_index::FLAG_DIRECTORY, 0, 0)
            .unwrap();
        idx.add_entry(3, d_ff, "main.rs", 0, 1000, 100).unwrap();
        idx.add_entry(4, d_ff, "doc.md", 0, 500, 200).unwrap();
        let d_win = idx
            .add_entry(5, d_root, "Windows", ff_index::FLAG_DIRECTORY, 0, 0)
            .unwrap();
        idx.add_entry(6, d_win, "notepad.exe", 0, 2000, 300)
            .unwrap();

        let handler = RequestHandler::new(Arc::new(RwLock::new(idx)));

        // 1. Find geral
        let res = handler.handle(IpcRequest::Find {
            pattern: "*.rs".to_string(),
            is_regex: false,
            fuzzy: false,
            limit: 10,
            target: None,
            extension: None,
            file_type: None,
            min_size: None,
            max_size: None,
        });
        match res {
            IpcResponse::Ok(val) => {
                assert_eq!(val["total_matched"], 1);
                assert_eq!(val["matches"][0]["name"], "main.rs");
            }
            IpcResponse::Error(e) => panic!("esperava Ok: {e}"),
        }

        // 2. Find com filtro de target restrito a Projects
        let res_target = handler.handle(IpcRequest::Find {
            pattern: "*".to_string(),
            is_regex: false,
            fuzzy: false,
            limit: 10,
            target: Some(projects.to_string()),
            extension: None,
            file_type: None,
            min_size: None,
            max_size: None,
        });
        match res_target {
            IpcResponse::Ok(val) => {
                let matches = val["matches"].as_array().unwrap();
                assert!(!matches.is_empty());
                for m in matches {
                    let path = m["path"].as_str().unwrap();
                    assert!(path.to_lowercase().starts_with(&projects.to_lowercase()));
                }
            }
            IpcResponse::Error(e) => panic!("esperava Ok: {e}"),
        }

        // 3. Du sobre pasta válida (resolve o próprio diretório e não o filho)
        let du_res = handler.handle(IpcRequest::Du {
            target: fastfile.to_string(),
            depth: Some(1),
            top: 10,
        });
        match du_res {
            IpcResponse::Ok(val) => {
                assert_eq!(val["total_files"], 2);
                assert_eq!(val["total_size"], 1500);
            }
            IpcResponse::Error(e) => panic!("esperava Ok: {e}"),
        }

        // 4. Du sobre pasta inexistente retorna erro estruturado em vez de agregar raiz
        let du_err = handler.handle(IpcRequest::Du {
            target: missing.to_string(),
            depth: Some(1),
            top: 10,
        });
        match du_err {
            IpcResponse::Error(e) => {
                assert!(e.contains("não encontrado"));
            }
            IpcResponse::Ok(_) => panic!("deveria ter retornado Erro"),
        }
    }
}
