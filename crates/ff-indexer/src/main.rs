//! Binário `ff-indexer`
//!
//! Processo de serviço elevado que mantém o índice residente na memória e serve consultas via IPC (§1, §3 e #11).

use std::path::PathBuf;
use std::sync::{Arc, RwLock};
use std::time::Instant;

use clap::{Parser, Subcommand};
use ff_index::{FileIndex, IndexSource};
use ff_indexer::service::{
    install_service, start_service, status_service, stop_service, uninstall_service,
};
use ff_indexer::{default_endpoint_name, IpcRequest, IpcResponse, RequestHandler};

#[derive(Parser, Debug)]
#[command(
    name = "ff-indexer",
    author = "Fabio Motta",
    version,
    about = "Daemon de indexação de arquivos residente em memória com IPC segura (FastFile)"
)]
struct Args {
    #[command(subcommand)]
    command: Option<IndexerCommand>,

    /// Volume NTFS ou diretório alvo para manter indexado (padrão: "C:")
    #[arg(default_value = "C:")]
    target: String,

    /// Caminho do arquivo de snapshot para cold-start instantâneo.
    /// Sem esta opção, o Windows usa `%LOCALAPPDATA%\FastFile\snapshots` automaticamente.
    #[arg(long)]
    snapshot: Option<PathBuf>,

    /// Não usa nem grava snapshot (reindexa o volume inteiro a cada início)
    #[arg(long)]
    no_snapshot: bool,

    /// Executar apenas um ciclo de inicialização/retomada e sair
    #[arg(long)]
    once: bool,

    /// Executa em segundo plano como Serviço do Windows
    #[arg(long)]
    service: bool,
}

#[derive(Subcommand, Debug, PartialEq, Eq)]
enum IndexerCommand {
    /// Gerencia o indexador como Serviço do Windows
    Service(ServiceArgs),
}

#[derive(clap::Args, Debug, PartialEq, Eq)]
struct ServiceArgs {
    #[command(subcommand)]
    action: ServiceAction,
}

#[derive(Subcommand, Debug, PartialEq, Eq)]
enum ServiceAction {
    /// Instala o serviço no Windows Service Control Manager (SCM)
    Install,
    /// Desinstala e remove o serviço do Windows
    Uninstall,
    /// Inicia o serviço do Windows
    Start,
    /// Para o serviço do Windows
    Stop,
    /// Exibe o status do serviço no SCM
    Status,
    /// Executa o daemon do serviço
    Run,
}

fn main() {
    let args = Args::parse();

    if let Some(IndexerCommand::Service(ref s_args)) = args.command {
        match s_args.action {
            ServiceAction::Install => match install_service() {
                Ok(msg) => println!("{msg}"),
                Err(e) => {
                    eprintln!("Erro: {e}");
                    std::process::exit(1);
                }
            },
            ServiceAction::Uninstall => match uninstall_service() {
                Ok(msg) => println!("{msg}"),
                Err(e) => {
                    eprintln!("Erro: {e}");
                    std::process::exit(1);
                }
            },
            ServiceAction::Start => match start_service() {
                Ok(msg) => println!("{msg}"),
                Err(e) => {
                    eprintln!("Erro: {e}");
                    std::process::exit(1);
                }
            },
            ServiceAction::Stop => match stop_service() {
                Ok(msg) => println!("{msg}"),
                Err(e) => {
                    eprintln!("Erro: {e}");
                    std::process::exit(1);
                }
            },
            ServiceAction::Status => match status_service() {
                Ok(msg) => println!("{msg}"),
                Err(e) => {
                    eprintln!("Erro: {e}");
                    std::process::exit(1);
                }
            },
            ServiceAction::Run => {
                run_daemon(&args);
            }
        }
        return;
    }

    run_daemon(&args);
}

fn run_daemon(args: &Args) {
    let endpoint = default_endpoint_name();

    println!("Iniciando ff-indexer para o alvo '{}'...", args.target);
    println!("Endpoint IPC: {endpoint}");

    // Snapshot em disco (cold start): carrega o índice salvo e retoma pelo USN em vez de reindexar tudo.
    let default_dir = if cfg!(windows) {
        ff_indexer::snapshot::snapshot_dir()
    } else {
        None
    };
    let snapshot_path = ff_indexer::snapshot::resolve_snapshot_path(
        args.snapshot.as_deref(),
        args.no_snapshot,
        args.service,
        &args.target,
        default_dir.as_deref(),
    );
    if let Some(path) = &snapshot_path {
        if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
            if let Err(e) = ff_indexer::snapshot::ensure_private_dir(parent) {
                eprintln!(
                    "Aviso: não foi possível preparar '{}': {e}",
                    parent.display()
                );
            }
        }
        println!("Snapshot: {}", path.display());
    } else {
        println!("Snapshot desativado: o volume será reindexado a cada início.");
    }

    let index = Arc::new(RwLock::new(FileIndex::new()));
    let start = Instant::now();

    // 1. Tentar carregar a partir de snapshot prévio (§3, Issue #12)
    #[cfg(windows)]
    let mut loaded_from_snapshot = false;
    if let Some(ref snap_path) = snapshot_path {
        if snap_path.exists() {
            println!("Carregando snapshot de '{}'...", snap_path.display());
            match FileIndex::load_snapshot_file(snap_path) {
                Ok(loaded) if !ff_indexer::snapshot::same_target(&loaded.volume, &args.target) => {
                    eprintln!(
                        "Aviso: o snapshot é do alvo '{}', não de '{}'; ignorando e reindexando.",
                        loaded.volume, args.target
                    );
                }
                Ok(loaded) if ff_indexer::snapshot::legacy_layout_reason(&loaded).is_some() => {
                    eprintln!(
                        "Aviso: snapshot de versão antiga ({}); ignorando e reindexando.",
                        ff_indexer::snapshot::legacy_layout_reason(&loaded).unwrap_or_default()
                    );
                }
                Ok(loaded) => {
                    let mut lock = index.write().unwrap();
                    *lock = loaded;
                    #[cfg(windows)]
                    {
                        loaded_from_snapshot = true;
                    }
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
        let resume_start = Instant::now();
        let before = (lock.len(), lock.last_usn);
        match ff_index::windows::UsnSource::resume_from_snapshot(&args.target, &mut lock) {
            Ok(true) => {
                println!(
                    "Retomada incremental pelo USN concluída com sucesso em {:.2?}! Total: {} entradas ({:+}); USN {} -> {}; {:.2?} desde o início.",
                    resume_start.elapsed(),
                    lock.len(),
                    lock.len() as i64 - before.0 as i64,
                    before.1,
                    lock.last_usn,
                    start.elapsed()
                );
                resumed = true;
                // Avança o ponto do USN no disco: sem isso o snapshot envelhece a cada execução até o diário dar
                // a volta e a retomada falhar. Só no --once, que sai antes do saver periódico; no modo daemon o
                // saver regrava em ~1 min, sem atrasar o início do serviço (a gravação leva segundos).
                if let Some(ref snap_path) = snapshot_path {
                    if args.once && (lock.len(), lock.last_usn) != before {
                        let t = Instant::now();
                        match lock.save_snapshot_file(snap_path) {
                            Ok(()) => println!("Snapshot atualizado em {:.2?}.", t.elapsed()),
                            Err(e) => {
                                eprintln!("Aviso: não foi possível atualizar o snapshot: {e}")
                            }
                        }
                    }
                }
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

    // 3. Se a retomada falhou ou se não carregou de snapshot, indexa completamente (Ponto 4)
    if !resumed {
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

        // O snapshot só é reaproveitável se souber de qual alvo veio (a varredura não preenche isto).
        if new_index.volume.is_empty() {
            new_index.volume = args.target.clone();
        }
        let mut lock = index.write().unwrap();
        *lock = new_index;
        println!(
            "Indexação completa concluída em {:.2?}: {} entradas.",
            start.elapsed(),
            lock.len()
        );

        // Salva snapshot atualizado se caminho configurado
        if let Some(ref snap_path) = snapshot_path {
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
        println!(
            "Modo --once: encerrando indexador ({:.2?} desde o início).",
            start.elapsed()
        );
        return;
    }

    let watcher_status = Arc::new(RwLock::new(ff_indexer::WatcherStatus::default()));
    let handler = Arc::new(
        RequestHandler::new(Arc::clone(&index)).with_watcher_status(Arc::clone(&watcher_status)),
    );

    // Inicia thread de background para manter o índice em memória sincronizado em tempo real (Ponto 3 e CORE-03)
    #[cfg(any(windows, target_os = "linux"))]
    let watch_index = Arc::clone(&index);
    #[cfg(any(windows, target_os = "linux"))]
    let target_vol = args.target.clone();

    #[cfg(windows)]
    {
        let thread_status = Arc::clone(&watcher_status);
        std::thread::Builder::new()
            .name("ff-usn-watcher".to_string())
            .spawn(move || {
                let vol_str = target_vol.clone();
                {
                    let mut st = thread_status.write().unwrap();
                    st.active = true;
                    st.state = "starting".to_string();
                }

                println!("Monitor USN ativo para manter índice vivo em tempo real...");

                let mut current_watcher: Option<ff_index::windows::UsnWatcher> = None;

                loop {
                    // 1. Assegura que o watcher está instanciado e com continuidade válida
                    if current_watcher.is_none() {
                        let (journal_id, last_usn) = {
                            let lock = watch_index.read().unwrap();
                            (lock.journal_id, lock.last_usn)
                        };

                        match ff_index::windows::UsnWatcher::new(&vol_str, journal_id, last_usn) {
                            Ok(w) => {
                                current_watcher = Some(w);
                                let mut st = thread_status.write().unwrap();
                                st.state = "healthy".to_string();
                                st.last_error = None;
                            }
                            Err(e) => {
                                {
                                    let mut st = thread_status.write().unwrap();
                                    st.state = "degraded".to_string();
                                    st.last_error = Some(format!("falha ao abrir USN watcher: {e}"));
                                }
                                eprintln!("Aviso: USN Watcher não disponível ({e}). Tentando novamente em 2s...");
                                std::thread::sleep(std::time::Duration::from_millis(2000));
                                continue;
                            }
                        }
                    }

                    // 2. Executa o polling de alterações
                    if let Some(ref mut watcher) = current_watcher {
                        std::thread::sleep(std::time::Duration::from_millis(250));
                        let mut lock = watch_index.write().unwrap();

                        match watcher.poll_changes(&mut lock) {
                            Ok(changes) => {
                                let now = std::time::SystemTime::now()
                                    .duration_since(std::time::UNIX_EPOCH)
                                    .unwrap_or_default()
                                    .as_secs() as i64;

                                if !changes.is_empty() {
                                    lock.last_usn = watcher.last_usn;
                                }

                                let mut st = thread_status.write().unwrap();
                                st.state = "healthy".to_string();
                                st.changes_applied = st.changes_applied.saturating_add(changes.len() as u64);
                                st.last_sync_unix = Some(now);
                                st.last_error = None;
                            }
                            Err(e) => {
                                let err_str = e.to_string();
                                eprintln!("Aviso no watcher USN ({err_str}); verificando continuidade...");

                                // Perda de continuidade ou troca de journal detectada (CORE-03):
                                let is_continuity_lost = match &e {
                                    ff_index::windows::UsnError::Io(io_err) => {
                                        let raw_code = io_err.raw_os_error().unwrap_or(0);
                                        raw_code == 1181 || raw_code == 87 || raw_code == 1179
                                    }
                                    _ => false,
                                };

                                if is_continuity_lost {
                                    {
                                        let mut st = thread_status.write().unwrap();
                                        st.state = "reconciling".to_string();
                                        st.reconciliations += 1;
                                        st.last_error = Some(format!("perda de continuidade do journal: {err_str}"));
                                    }
                                    println!("Perda de continuidade do USN detectada. Reconstruindo índice...");

                                    // Reconciliação / reconstrução do índice a partir do volume
                                    let mut new_index = ff_index::FileIndex::new();
                                    new_index.volume = vol_str.clone();

                                    let mut mft = ff_index::windows::MftSource::new(&vol_str);
                                    if mft.populate(&mut new_index).is_ok() {
                                        *lock = new_index;
                                        println!("Reconciliação MFT concluída: {} entradas.", lock.len());
                                    } else {
                                        let mut walk = ff_index::WalkSource::new(&vol_str);
                                        if walk.populate(&mut new_index).is_ok() {
                                            *lock = new_index;
                                            println!("Reconciliação Walk concluída: {} entradas.", lock.len());
                                        }
                                    }
                                } else {
                                    let mut st = thread_status.write().unwrap();
                                    st.state = "degraded".to_string();
                                    st.last_error = Some(err_str);
                                }

                                // Força descarte do watcher anterior para reconexão/retomada limpa
                                current_watcher = None;
                                std::thread::sleep(std::time::Duration::from_millis(1000));
                            }
                        }
                    }
                }
            })
            .expect("falha ao criar thread de USN watcher");
    }

    #[cfg(target_os = "linux")]
    {
        let thread_status = Arc::clone(&watcher_status);
        {
            let mut st = thread_status.write().unwrap();
            st.active = true;
            st.state = "healthy".to_string();
        }
        let stop_flag = Arc::new(std::sync::atomic::AtomicBool::new(false));
        if let Ok(watcher) = ff_index::linux::LinuxLiveSource::new(&target_vol) {
            watcher.spawn_watcher_thread(watch_index, stop_flag);
        }
    }

    // Mantém o snapshot atualizado: o próximo início retoma pelo USN a partir dele.
    #[cfg(windows)]
    if let Some(path) = snapshot_path.clone() {
        let saver_index = Arc::clone(&index);
        let already_saved = !resumed;
        let _ = std::thread::Builder::new()
            .name("ff-snapshot-saver".to_string())
            .spawn(move || snapshot_saver(saver_index, path, already_saved));
    }

    // 4. Servidor IPC
    #[cfg(windows)]
    run_windows_ipc_server(&endpoint, handler);

    #[cfg(not(windows))]
    run_unix_ipc_server(&endpoint, handler);
}

/// Regrava o snapshot periodicamente enquanto o índice muda (cold start rápido no próximo início).
///
/// `FASTFILE_SNAPSHOT_INTERVAL_SECS` ajusta o intervalo (mínimo 5 s; padrão 600 s). Só grava se o ponto do
/// USN ou o tamanho do índice mudou desde a última gravação. A gravação segura o bloqueio de leitura
/// (consultas continuam; o watcher espera) e é atômica, então uma queda nunca corrompe o snapshot anterior.
#[cfg(windows)]
fn snapshot_saver(index: Arc<RwLock<FileIndex>>, path: PathBuf, already_saved: bool) {
    use std::time::Duration;

    let interval = std::env::var("FASTFILE_SNAPSHOT_INTERVAL_SECS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .filter(|s| *s >= 5)
        .unwrap_or(600);
    let state = |i: &FileIndex| (i.last_usn, i.len());
    fn read(i: &RwLock<FileIndex>) -> std::sync::RwLockReadGuard<'_, FileIndex> {
        i.read().unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    let mut last_saved = already_saved.then(|| state(&read(&index)));
    // Após uma retomada, o snapshot em disco está defasado: regrava logo (1 min), não em 10.
    let mut wait = if last_saved.is_none() {
        interval.min(60)
    } else {
        interval
    };

    loop {
        std::thread::sleep(Duration::from_secs(wait));
        wait = interval;
        let guard = read(&index);
        let now = state(&guard);
        if last_saved == Some(now) {
            continue;
        }
        match guard.save_snapshot_file(&path) {
            Ok(()) => {
                last_saved = Some(now);
                println!("Snapshot atualizado: {} entradas.", now.1);
            }
            Err(e) => eprintln!("Aviso: não foi possível atualizar o snapshot: {e}"),
        }
    }
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
        PIPE_REJECT_REMOTE_CLIENTS, PIPE_TYPE_MESSAGE, PIPE_WAIT,
    };

    let wide_name: Vec<u16> = OsStr::new(pipe_name)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();

    // Falha fechada: sem DACL restrita, o pipe ficaria legível por outros usuários locais.
    let security = match ff_indexer::pipe_security::PipeSecurity::for_current_user() {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Erro ao montar a DACL do Named Pipe ({e}); IPC não iniciada.");
            return;
        }
    };

    println!("Servidor IPC pronto. Aguardando conexões no Named Pipe...");

    loop {
        // SAFETY: `wide_name` é UTF-16 terminada em NUL e `security` (DACL só do usuário + SYSTEM) vive
        // além da chamada.
        let pipe_handle = unsafe {
            CreateNamedPipeW(
                wide_name.as_ptr(),
                PIPE_ACCESS_DUPLEX | FILE_FLAG_FIRST_PIPE_INSTANCE,
                PIPE_TYPE_MESSAGE | PIPE_READMODE_MESSAGE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS,
                1,
                65536,
                65536,
                0,
                security.as_ptr(),
            )
        };

        if pipe_handle == INVALID_HANDLE_VALUE {
            eprintln!("Erro ao criar Named Pipe; tentando novamente em 1s...");
            std::thread::sleep(std::time::Duration::from_secs(1));
            continue;
        }

        // SAFETY: `pipe_handle` é um handle de pipe válido criado logo acima.
        let connected = unsafe { ConnectNamedPipe(pipe_handle, std::ptr::null_mut()) };
        // SAFETY: `GetLastError` não tem pré-condições.
        if connected != 0 || unsafe { windows_sys::Win32::Foundation::GetLastError() } == 535 {
            // ERROR_PIPE_CONNECTED = 535
            let mut buffer = vec![0u8; 65536];
            let mut bytes_read = 0u32;
            // SAFETY: `buffer` é um `Vec<u8>` vivo com o tamanho informado; `bytes_read` é local.
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
                    // SAFETY: `resp_bytes` vive além da chamada, com o tamanho informado; `bytes_written` é local.
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

        // SAFETY: `pipe_handle` é válido, criado nesta iteração, e fechado uma única vez aqui.
        unsafe {
            DisconnectNamedPipe(pipe_handle);
            CloseHandle(pipe_handle);
        }
    }
}

#[cfg(not(windows))]
fn run_unix_ipc_server(socket_path: &str, handler: Arc<RequestHandler>) {
    use std::io::{BufRead, BufReader, Write};

    // Socket criado já com 0o600, sem janela de permissões amplas (ver `unix_socket`).
    let listener = match ff_indexer::unix_socket::bind_private(std::path::Path::new(socket_path)) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("Erro ao associar Unix Socket em '{socket_path}': {e}");
            return;
        }
    };
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
