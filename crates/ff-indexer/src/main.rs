//! Binário `ff-indexer`
//!
//! Processo de serviço elevado que mantém o índice residente na memória e serve consultas via IPC (§1, §3 e #11).

use std::path::PathBuf;
use std::sync::{Arc, RwLock};
use std::time::Instant;

use clap::Parser;
use ff_index::{FileIndex, IndexSource};
use ff_indexer::{default_endpoint_name, IpcRequest, IpcResponse, RequestHandler};

#[derive(Parser, Debug)]
#[command(
    name = "ff-indexer",
    author = "Fabio Motta",
    version,
    about = "Daemon de indexação de arquivos residente em memória com IPC segura (FastFile)"
)]
struct Args {
    /// Volume NTFS ou diretório alvo para manter indexado (padrão: "C:")
    #[arg(default_value = "C:")]
    target: String,

    /// Caminho do arquivo de snapshot para cold-start instantâneo
    #[arg(long)]
    snapshot: Option<PathBuf>,

    /// Executar apenas um ciclo de inicialização/retomada e sair
    #[arg(long)]
    once: bool,
}

fn main() {
    let args = Args::parse();
    let endpoint = default_endpoint_name();

    println!("Iniciando ff-indexer para o alvo '{}'...", args.target);
    println!("Endpoint IPC: {endpoint}");

    let index = Arc::new(RwLock::new(FileIndex::new()));
    let start = Instant::now();

    // 1. Tentar carregar a partir de snapshot prévio (§3, Issue #12)
    let mut loaded_from_snapshot = false;
    if let Some(ref snap_path) = args.snapshot {
        if snap_path.exists() {
            println!("Carregando snapshot de '{}'...", snap_path.display());
            match FileIndex::load_snapshot_file(snap_path) {
                Ok(loaded) => {
                    let mut lock = index.write().unwrap();
                    *lock = loaded;
                    loaded_from_snapshot = true;
                    println!(
                        "Snapshot carregado em {:.2?}: {} entradas.",
                        start.elapsed(),
                        lock.len()
                    );
                }
                Err(e) => {
                    eprintln!("Aviso: falha ao carregar snapshot ({e}). Reindexando...");
                }
            }
        }
    }

    // 2. Se carregou do snapshot, tenta retomada incremental pelo USN (USN-RESUME-001)
    #[allow(unused_mut)]
    let mut resumed = false;
    #[cfg(windows)]
    if loaded_from_snapshot {
        let mut lock = index.write().unwrap();
        match ff_index::windows::UsnSource::resume_from_snapshot(&args.target, &mut lock) {
            Ok(true) => {
                println!(
                    "Retomada incremental pelo USN concluída com sucesso! Total: {} entradas.",
                    lock.len()
                );
                resumed = true;
            }
            Ok(false) => {
                println!(
                    "USN Journal foi truncado ou alterado; executando reindexação completa..."
                );
            }
            Err(e) => {
                eprintln!("Erro ao retomar pelo USN ({e}); executando reindexação completa...");
            }
        }
    }

    // 3. Se não carregou ou retomada falhou, indexa completamente
    if !resumed && !loaded_from_snapshot {
        println!("Indexando volume '{}'...", args.target);
        let mut new_index = FileIndex::new();

        #[cfg(windows)]
        {
            let mut mft = ff_index::windows::MftSource::new(&args.target);
            if mft.populate(&mut new_index).is_err() {
                let mut usn = ff_index::windows::UsnSource::new(&args.target);
                if usn.populate(&mut new_index).is_err() {
                    let mut walk =
                        ff_index::WalkSource::new(&args.target).with_respect_ignore(false);
                    let _ = walk.populate(&mut new_index);
                }
            }
        }
        #[cfg(not(windows))]
        {
            let mut walk = ff_index::WalkSource::new(&args.target).with_respect_ignore(false);
            let _ = walk.populate(&mut new_index);
        }

        let mut lock = index.write().unwrap();
        *lock = new_index;
        println!(
            "Indexação completa concluída em {:.2?}: {} entradas.",
            start.elapsed(),
            lock.len()
        );

        // Salva snapshot atualizado se caminho configurado
        if let Some(ref snap_path) = args.snapshot {
            if let Err(e) = lock.save_snapshot_file(snap_path) {
                eprintln!(
                    "Aviso: não foi possível salvar snapshot em '{}': {e}",
                    snap_path.display()
                );
            } else {
                println!("Snapshot salvo em '{}'.", snap_path.display());
            }
        }
    }

    if args.once {
        println!("Modo --once: encerrando indexador.");
        return;
    }

    let handler = Arc::new(RequestHandler::new(Arc::clone(&index)));

    // 4. Servidor IPC
    #[cfg(windows)]
    run_windows_ipc_server(&endpoint, handler);

    #[cfg(not(windows))]
    run_unix_ipc_server(&endpoint, handler);
}

#[cfg(windows)]
fn run_windows_ipc_server(pipe_name: &str, handler: Arc<RequestHandler>) {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::Storage::FileSystem::{
        ReadFile, WriteFile, FILE_FLAG_FIRST_PIPE_INSTANCE, PIPE_ACCESS_DUPLEX,
    };
    use windows_sys::Win32::System::Pipes::{
        ConnectNamedPipe, CreateNamedPipeW, DisconnectNamedPipe, PIPE_READMODE_MESSAGE,
        PIPE_TYPE_MESSAGE, PIPE_WAIT,
    };

    let wide_name: Vec<u16> = OsStr::new(pipe_name)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();

    println!("Servidor IPC pronto. Aguardando conexões no Named Pipe...");

    loop {
        let pipe_handle = unsafe {
            CreateNamedPipeW(
                wide_name.as_ptr(),
                PIPE_ACCESS_DUPLEX | FILE_FLAG_FIRST_PIPE_INSTANCE,
                PIPE_TYPE_MESSAGE | PIPE_READMODE_MESSAGE | PIPE_WAIT,
                1,
                65536,
                65536,
                0,
                std::ptr::null_mut(),
            )
        };

        if pipe_handle == INVALID_HANDLE_VALUE {
            eprintln!("Erro ao criar Named Pipe; tentando novamente em 1s...");
            std::thread::sleep(std::time::Duration::from_secs(1));
            continue;
        }

        let connected = unsafe { ConnectNamedPipe(pipe_handle, std::ptr::null_mut()) };
        if connected != 0 || unsafe { windows_sys::Win32::Foundation::GetLastError() } == 535 {
            // ERROR_PIPE_CONNECTED = 535
            let mut buffer = vec![0u8; 65536];
            let mut bytes_read = 0u32;
            let read_ok = unsafe {
                ReadFile(
                    pipe_handle,
                    buffer.as_mut_ptr(),
                    buffer.len() as u32,
                    &mut bytes_read,
                    std::ptr::null_mut(),
                )
            };

            if read_ok != 0 && bytes_read > 0 {
                let req_slice = &buffer[..bytes_read as usize];
                let response = match serde_json::from_slice::<IpcRequest>(req_slice) {
                    Ok(req) => handler.handle(req),
                    Err(e) => IpcResponse::Error(format!("requisição inválida: {e}")),
                };

                if let Ok(mut resp_bytes) = serde_json::to_vec(&response) {
                    resp_bytes.push(b'\n');
                    let mut bytes_written = 0u32;
                    unsafe {
                        WriteFile(
                            pipe_handle,
                            resp_bytes.as_ptr(),
                            resp_bytes.len() as u32,
                            &mut bytes_written,
                            std::ptr::null_mut(),
                        );
                    }
                }
            }
        }

        unsafe {
            DisconnectNamedPipe(pipe_handle);
            CloseHandle(pipe_handle);
        }
    }
}

#[cfg(not(windows))]
fn run_unix_ipc_server(socket_path: &str, handler: Arc<RequestHandler>) {
    use std::io::{BufRead, BufReader, Write};
    use std::os::unix::fs::PermissionsExt;
    use std::os::unix::net::UnixListener;

    let _ = std::fs::remove_file(socket_path);
    let listener = match UnixListener::bind(socket_path) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("Erro ao associar Unix Socket em '{socket_path}': {e}");
            return;
        }
    };

    // Restringe permissões para 0o600 (apenas o próprio usuário pode conectar)
    let _ = std::fs::set_permissions(socket_path, std::fs::Permissions::from_mode(0o600));
    println!("Servidor IPC pronto. Aguardando conexões no Unix Domain Socket...");

    for mut stream in listener.incoming().flatten() {
        let mut reader = BufReader::new(stream.try_clone().unwrap());
        let mut line = String::new();
        if reader.read_line(&mut line).is_ok() && !line.is_empty() {
            let response = match serde_json::from_str::<IpcRequest>(&line) {
                Ok(req) => handler.handle(req),
                Err(e) => IpcResponse::Error(format!("requisição inválida: {e}")),
            };

            if let Ok(resp_json) = serde_json::to_string(&response) {
                let _ = writeln!(stream, "{resp_json}");
                let _ = stream.flush();
            }
        }
    }
}
