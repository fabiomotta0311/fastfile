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
    ///
    /// Observação sobre concorrência: cursores são baseados em offset ordinal estável.
    /// Caso arquivos sejam adicionados/removidos durante a iteração em um índice dinâmico,
    /// a posição pode sofrer deslocamento pontual entre requisições sucessivas (CORE-02).
    Find {
        pattern: String,
        #[serde(default)]
        is_regex: bool,
        #[serde(default)]
        fuzzy: bool,
        #[serde(default)]
        limit: usize,
        #[serde(default)]
        cursor: Option<usize>,
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
        #[serde(default)]
        min_mtime: Option<i64>,
        #[serde(default)]
        all: bool,
        #[serde(default)]
        include_secrets: bool,
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
                cursor,
                target,
                extension,
                file_type,
                min_size,
                max_size,
                min_mtime,
                all,
                include_secrets,
            } => {
                if let Some(ref ft) = file_type {
                    if !matches!(
                        ft.to_lowercase().as_str(),
                        "file" | "f" | "dir" | "d" | "directory"
                    ) {
                        return IpcResponse::Error(format!(
                            "tipo inválido '{ft}': use 'file' ou 'dir'"
                        ));
                    }
                }
                let query = match Query::parse_with_options(&pattern, is_regex, fuzzy) {
                    Ok(q) => q,
                    Err(e) => return IpcResponse::Error(format!("consulta inválida: {e}")),
                };

                let idx = self.index.read().unwrap();

                // Normaliza o target se especificado e valida cobertura de volume (CORE-01)
                let target_path_buf = target
                    .as_ref()
                    .map(|t| ff_index::clean_path(std::path::Path::new(t)));
                if let Some(ref tp) = target_path_buf {
                    if !ff_index::is_target_in_volume(tp, &idx.volume) {
                        return IpcResponse::Error(format!(
                            "alvo '{}' está fora da cobertura do daemon (volume indexado: '{}')",
                            tp.display(),
                            idx.volume
                        ));
                    }
                }

                let result = ff_search::SearchEngine::search(&idx, &query);

                let mut secrets_hidden = 0usize;
                let mut filtered_candidates = Vec::new();

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

                    // Filtro de mtime / since (CORE-02)
                    if let Some(min_t) = min_mtime {
                        if entry.mtime < min_t {
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

                    // Filtro de target: validação de pertencimento à árvore de diretórios (CORE-01)
                    if let Some(ref tp) = target_path_buf {
                        if !ff_index::is_in_directory_tree(std::path::Path::new(&path), tp) {
                            continue;
                        }
                    }

                    // Filtro de diretórios ignorados (target, node_modules, .git) quando !all (CORE-02)
                    if !all && ff_index::has_ignored_component(std::path::Path::new(&path)) {
                        continue;
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

                    // Proteção de segredos por padrão (CORE-02)
                    if !include_secrets && ff_index::is_secret_file(&name) {
                        secrets_hidden += 1;
                        continue;
                    }

                    // Cálculo do rank (profundidade + data + score)
                    let depth = path.split(['/', '\\']).count() as u32;
                    let depth_penalty = depth.saturating_mul(25);
                    let mtime_bonus = ((entry.mtime.max(0) / 86400) % 100) as u32;
                    let rank =
                        mat.score.saturating_mul(100).saturating_sub(depth_penalty) + mtime_bonus;

                    filtered_candidates.push((
                        mat.id,
                        rank,
                        entry.mtime,
                        entry.size,
                        entry.is_dir(),
                        path,
                        name,
                    ));
                }

                // Ordenação ANTES da paginação (CORE-02)
                if fuzzy {
                    filtered_candidates
                        .sort_by_key(|(_, rank, _, _, _, _, _)| std::cmp::Reverse(*rank));
                } else {
                    filtered_candidates
                        .sort_by_key(|(_, _, mtime, _, _, _, _)| std::cmp::Reverse(*mtime));
                }

                let total_matched = filtered_candidates.len();
                let offset = cursor.unwrap_or(0);
                let paged_items: Vec<_> = if offset < total_matched {
                    filtered_candidates
                        .into_iter()
                        .skip(offset)
                        .take(limit)
                        .collect()
                } else {
                    Vec::new()
                };

                let returned_matches = paged_items.len();
                let next_cursor = if offset + returned_matches < total_matched {
                    Some(offset + returned_matches)
                } else {
                    None
                };

                let mut matches = Vec::with_capacity(returned_matches);
                for (id, rank, mtime, size, is_dir, path, name) in paged_items {
                    matches.push(serde_json::json!({
                        "id": id,
                        "path": path,
                        "name": name,
                        "size": size,
                        "mtime": mtime,
                        "is_dir": is_dir,
                        "score": rank,
                    }));
                }

                IpcResponse::Ok(serde_json::json!({
                    "total_matched": total_matched,
                    "returned_matches": returned_matches,
                    "cursor": offset,
                    "next_cursor": next_cursor,
                    "secrets_hidden": secrets_hidden,
                    "total_scanned": result.total_scanned,
                    "elapsed_ms": (result.elapsed.as_secs_f64() * 1000.0 * 100.0).round() / 100.0,
                    "matches": matches,
                }))
            }
            IpcRequest::Du { target, depth, top } => {
                let idx = self.index.read().unwrap();
                let target_path_buf = ff_index::clean_path(std::path::Path::new(target.trim()));

                if !ff_index::is_target_in_volume(&target_path_buf, &idx.volume) {
                    return IpcResponse::Error(format!(
                        "alvo '{target}' está fora da cobertura do daemon (volume indexado: '{}')",
                        idx.volume
                    ));
                }

                let target_str = target_path_buf.to_string_lossy();
                let norm_target = target_str.trim_end_matches(['\\', '/']);

                let is_root = norm_target.is_empty()
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
                    // Resolve o ID da PRÓPRIA pasta alvo no índice via comparação de componentes
                    let found = idx.entries.iter().enumerate().find(|(id, e)| {
                        if !e.is_dir() || e.is_deleted() {
                            return false;
                        }
                        if let Ok(entry_path) = idx.build_path(*id as u32) {
                            let ep = ff_index::clean_path(std::path::Path::new(&entry_path));
                            ff_index::is_in_directory_tree(&ep, &target_path_buf)
                                && ff_index::is_in_directory_tree(&target_path_buf, &ep)
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
        let root = if cfg!(windows) { "C:" } else { "/volume" };
        let path = |parts: &[&str]| {
            let base = if cfg!(windows) { "C:\\" } else { "/volume" };
            parts
                .iter()
                .fold(std::path::PathBuf::from(base), |p, part| p.join(part))
                .to_string_lossy()
                .into_owned()
        };
        let outside = if cfg!(windows) {
            "D:\\Projects"
        } else {
            "/other/Projects"
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
        // Pasta que tem o mesmo prefixo textual "Projects", mas é outro diretório:
        let d_proj_fake = idx
            .add_entry(7, d_root, "ProjectsFake", ff_index::FLAG_DIRECTORY, 0, 0)
            .unwrap();
        idx.add_entry(8, d_proj_fake, "fake.txt", 0, 100, 400)
            .unwrap();

        let handler = RequestHandler::new(Arc::new(RwLock::new(idx)));

        // 1. Find geral
        let res = handler.handle(IpcRequest::Find {
            pattern: "*.rs".to_string(),
            is_regex: false,
            fuzzy: false,
            limit: 10,
            cursor: None,
            target: None,
            extension: None,
            file_type: None,
            min_size: None,
            max_size: None,
            min_mtime: None,
            all: false,
            include_secrets: false,
        });
        match res {
            IpcResponse::Ok(val) => {
                assert_eq!(val["total_matched"], 1);
                assert_eq!(val["matches"][0]["name"], "main.rs");
            }
            IpcResponse::Error(e) => panic!("esperava Ok: {e}"),
        }

        // 2. Find com filtro de target restrito a Projects (CORE-01: NÃO deve incluir ProjectsFake)
        let res_target = handler.handle(IpcRequest::Find {
            pattern: "*".to_string(),
            is_regex: false,
            fuzzy: false,
            limit: 10,
            cursor: None,
            target: Some(path(&["Projects"])),
            extension: None,
            file_type: None,
            min_size: None,
            max_size: None,
            min_mtime: None,
            all: false,
            include_secrets: false,
        });
        match res_target {
            IpcResponse::Ok(val) => {
                let matches = val["matches"].as_array().unwrap();
                assert!(!matches.is_empty());
                for m in matches {
                    let path = m["path"].as_str().unwrap();
                    assert!(
                        !path.contains("ProjectsFake"),
                        "caminho '{path}' não deveria ter casado pois pertence a ProjectsFake"
                    );
                }
            }
            IpcResponse::Error(e) => panic!("esperava Ok: {e}"),
        }

        // 2b. Find com target fora do volume coberto (ex: D:\Projects) retorna erro explícito (CORE-01)
        let res_out_of_volume = handler.handle(IpcRequest::Find {
            pattern: "*".to_string(),
            is_regex: false,
            fuzzy: false,
            limit: 10,
            cursor: None,
            target: Some(outside.to_string()),
            extension: None,
            file_type: None,
            min_size: None,
            max_size: None,
            min_mtime: None,
            all: false,
            include_secrets: false,
        });
        match res_out_of_volume {
            IpcResponse::Error(err) => {
                assert!(err.contains("fora da cobertura do daemon"));
            }
            IpcResponse::Ok(_) => panic!("deveria ter retornado Erro de cobertura de volume"),
        }

        // 3. Du sobre pasta válida (resolve o próprio diretório e não o filho)
        let du_res = handler.handle(IpcRequest::Du {
            target: path(&["Projects", "fastfile"]),
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

        // 3b. Du com target fora do volume coberto retorna erro explícito
        let du_out_of_vol = handler.handle(IpcRequest::Du {
            target: outside.to_string(),
            depth: Some(1),
            top: 10,
        });
        match du_out_of_vol {
            IpcResponse::Error(err) => {
                assert!(err.contains("fora da cobertura do daemon"));
            }
            IpcResponse::Ok(_) => panic!("deveria ter retornado Erro de volume"),
        }

        // 4. Du sobre pasta inexistente retorna erro estruturado em vez de agregar raiz
        let du_err = handler.handle(IpcRequest::Du {
            target: path(&["NaoExiste"]),
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
