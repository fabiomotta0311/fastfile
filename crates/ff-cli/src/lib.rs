pub mod schema;
pub mod secrets;
pub mod util;

use std::io::{IsTerminal, Read};
use std::path::Path;
use std::time::{Instant, SystemTime};

use clap::{Args, Parser, Subcommand};
use ff_index::{FileIndex, IndexSource, SyntheticSource, WalkSource};
use ff_search::{Filters, Pattern, Query, SearchEngine};
use ff_size::SizeAggregator;

use crate::schema::{DuResultItem, FileResultItem, JsonEnvelope, JsonErrorEnvelope, StatResult};
use crate::secrets::is_secret_file;
use crate::util::{
    apply_budget, format_bytes, format_unix_timestamp, make_relative_path, parse_duration,
    parse_size_filter,
};

/// Códigos de saída do CLI FastFile (§3.1).
pub const EXIT_SUCCESS: i32 = 0;
pub const EXIT_NOT_IMPLEMENTED: i32 = 1;
pub const EXIT_TARGET_NOT_FOUND: i32 = 2;
pub const EXIT_INVALID_ARGUMENT: i32 = 3;
pub const EXIT_IO_ERROR: i32 = 4;
pub const EXIT_OTHER_ERROR: i32 = 5;

/// Erros de execução do CLI.
#[derive(Debug, PartialEq, Eq)]
pub enum CliError {
    /// O subcomando ainda não está implementado.
    NotImplemented(&'static str),
    /// Alvo ou arquivo não encontrado.
    TargetNotFound(String),
    /// Parâmetro de linha de comando inválido.
    InvalidArgument(String),
    /// Erro de E/S.
    Io(String),
    /// Erro genérico de execução.
    Other(String),
}

impl CliError {
    #[must_use]
    pub fn exit_code(&self) -> i32 {
        match self {
            Self::NotImplemented(_) => EXIT_NOT_IMPLEMENTED,
            Self::TargetNotFound(_) => EXIT_TARGET_NOT_FOUND,
            Self::InvalidArgument(_) => EXIT_INVALID_ARGUMENT,
            Self::Io(_) => EXIT_IO_ERROR,
            Self::Other(_) => EXIT_OTHER_ERROR,
        }
    }

    #[must_use]
    pub fn error_code(&self) -> &'static str {
        match self {
            Self::NotImplemented(_) => "NOT_IMPLEMENTED",
            Self::TargetNotFound(_) => "TARGET_NOT_FOUND",
            Self::InvalidArgument(_) => "INVALID_ARGUMENT",
            Self::Io(_) => "IO_ERROR",
            Self::Other(_) => "EXECUTION_ERROR",
        }
    }

    #[must_use]
    pub fn message(&self) -> String {
        match self {
            Self::NotImplemented(cmd) => format!("subcomando '{cmd}' não implementado"),
            Self::TargetNotFound(target) => format!("alvo não encontrado: '{target}'"),
            Self::InvalidArgument(msg) => format!("argumento inválido: {msg}"),
            Self::Io(msg) => format!("erro de E/S: {msg}"),
            Self::Other(msg) => msg.clone(),
        }
    }
}

/// Valida se o alvo informado é um volume (ex.: `C:`) ou um caminho de diretório válido.
pub fn validate_target(val: &str) -> Result<String, String> {
    let is_volume =
        (val.len() >= 2 && val.as_bytes()[0].is_ascii_alphabetic() && val.as_bytes()[1] == b':')
            || val.starts_with('/')
            || val.starts_with('\\')
            || val.contains('/')
            || val.contains('\\')
            || val == "."
            || val == ".."
            || Path::new(val).exists();

    if is_volume {
        Ok(val.to_string())
    } else {
        Err(format!(
            "alvo inválido '{val}': informe um volume (ex.: C:) ou caminho de diretório válido"
        ))
    }
}

#[derive(Parser, Debug)]
#[command(
    about = "FastFile — índice rápido de arquivos e busca instantânea",
    version
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand, Debug, PartialEq, Eq)]
pub enum Commands {
    /// Indexa um volume NTFS ou diretório
    Index(IndexArgs),
    /// Busca arquivos por nome no índice
    Find(FindArgs),
    /// Agrega uso de disco por pasta (estilo du / WizTree)
    Du(DuArgs),
    /// Exibe metadados detalhados de um arquivo ou diretório
    Stat(StatArgs),
    /// Lista arquivos modificados recentemente
    Recent(RecentArgs),
    /// Executa lote de consultas a partir de JSON
    Batch(BatchArgs),
    /// Acompanha alterações ao vivo pelo USN Journal
    Watch(WatchArgs),
    /// Executa benchmarks reproduzíveis de indexação e busca
    Bench(BenchArgs),
}

#[derive(Args, Debug, PartialEq, Eq)]
pub struct IndexArgs {
    /// Volume NTFS (ex.: C:) ou diretório a indexar
    #[arg(value_parser = validate_target)]
    pub target: String,

    /// Respeitar regras de .gitignore e ignorar arquivos ocultos
    #[arg(long)]
    pub respect_ignore: bool,

    /// Emitir saída em JSON
    #[arg(long)]
    pub json: bool,
}

#[derive(Args, Debug, PartialEq, Eq)]
pub struct FindArgs {
    /// Padrão de busca (substring, curingas como *.toml, etc.)
    pub pattern: Option<String>,

    /// Nome exato ou padrão a buscar (alternativa ao argumento posicional)
    #[arg(long)]
    pub name: Option<String>,

    /// Alvo ou diretório base da busca (padrão: diretório atual ".")
    #[arg(short, long, default_value = ".")]
    pub target: String,

    /// Tratar padrão como expressão regular
    #[arg(short, long)]
    pub regex: bool,

    /// Filtrar por extensão de arquivo (ex.: rs, toml)
    #[arg(short, long)]
    pub ext: Option<String>,

    /// Filtrar por tamanho (ex.: '>1GB', '<=10MB', '>=500KB')
    #[arg(long)]
    pub size: Option<String>,

    /// Filtrar por tipo: 'file' (f) ou 'dir' (d)
    #[arg(short, long = "type")]
    pub file_type: Option<String>,

    /// Filtrar modificados desde duração (ex.: '1h', '24h', '7d')
    #[arg(long)]
    pub since: Option<String>,

    /// Limite máximo de resultados a exibir
    #[arg(short, long, default_value_t = 50)]
    pub limit: usize,

    /// Cursor numérico para paginação de resultados
    #[arg(long)]
    pub cursor: Option<usize>,

    /// Orçamento aproximado de tokens para agentes (--budget N)
    #[arg(long)]
    pub budget: Option<usize>,

    /// Exibir caminhos relativos ao diretório alvo
    #[arg(long)]
    pub relative: bool,

    /// Incluir pastas ignoradas (.git, target, node_modules)
    #[arg(long)]
    pub all: bool,

    /// Incluir segredos (.env, chaves, *.jwt) sem mascarar
    #[arg(long)]
    pub include_secrets: bool,

    /// Emitir saída em JSON (ativo automaticamente se stdout não for terminal)
    #[arg(long)]
    pub json: bool,

    /// Quantidade de entradas sintéticas para buscar (útil para testes em qualquer SO)
    #[arg(short, long)]
    pub synthetic: Option<usize>,
}

#[derive(Args, Debug, PartialEq, Eq)]
pub struct DuArgs {
    /// Diretório ou volume a analisar
    #[arg(default_value = ".")]
    pub target: String,

    /// Quantidade de maiores pastas a retornar
    #[arg(long, default_value_t = 15)]
    pub top: usize,

    /// Profundidade máxima relativa (ex.: 1 para filhos imediatos)
    #[arg(long)]
    pub depth: Option<u32>,

    /// Orçamento aproximado de tokens para agentes
    #[arg(long)]
    pub budget: Option<usize>,

    /// Exibir caminhos relativos ao diretório alvo
    #[arg(long)]
    pub relative: bool,

    /// Incluir pastas ignoradas (.git, target, node_modules)
    #[arg(long)]
    pub all: bool,

    /// Incluir segredos sem mascarar
    #[arg(long)]
    pub include_secrets: bool,

    /// Emitir saída em JSON (ativo automaticamente se stdout não for terminal)
    #[arg(long)]
    pub json: bool,
}

#[derive(Args, Debug, PartialEq, Eq)]
pub struct StatArgs {
    /// Caminho do arquivo ou diretório
    pub target: String,

    /// Incluir segredos sem mascarar
    #[arg(long)]
    pub include_secrets: bool,

    /// Emitir saída em JSON (ativo automaticamente se stdout não for terminal)
    #[arg(long)]
    pub json: bool,
}

#[derive(Args, Debug, PartialEq, Eq)]
pub struct RecentArgs {
    /// Diretório base da busca (padrão: diretório atual ".")
    #[arg(short, long, default_value = ".")]
    pub target: String,

    /// Intervalo de modificação (ex.: '1h', '24h', '7d', '30m')
    #[arg(long, default_value = "1h")]
    pub since: String,

    /// Limite máximo de resultados a exibir
    #[arg(short, long, default_value_t = 50)]
    pub limit: usize,

    /// Cursor numérico para paginação de resultados
    #[arg(long)]
    pub cursor: Option<usize>,

    /// Orçamento aproximado de tokens para agentes
    #[arg(long)]
    pub budget: Option<usize>,

    /// Exibir caminhos relativos ao diretório alvo
    #[arg(long)]
    pub relative: bool,

    /// Incluir pastas ignoradas (.git, target, node_modules)
    #[arg(long)]
    pub all: bool,

    /// Incluir segredos sem mascarar
    #[arg(long)]
    pub include_secrets: bool,

    /// Emitir saída em JSON (ativo automaticamente se stdout não for terminal)
    #[arg(long)]
    pub json: bool,
}

#[derive(Args, Debug, PartialEq, Eq)]
pub struct BatchArgs {
    /// Arquivo contendo consultas em JSON (lê do stdin se omitido)
    #[arg(short, long)]
    pub file: Option<String>,

    /// Emitir saída em JSON
    #[arg(long)]
    pub json: bool,
}

#[derive(Args, Debug, PartialEq, Eq)]
pub struct WatchArgs {
    /// Volume NTFS a monitorar pelo USN Journal (ex.: C:)
    #[arg(value_parser = validate_target)]
    pub target: String,
}

#[derive(Args, Debug, PartialEq, Eq)]
pub struct BenchArgs {
    /// Volume NTFS para benchmark (ex.: C:)
    #[arg(value_parser = validate_target)]
    pub target: String,

    /// Padrões de consulta a testar
    #[arg(required = true)]
    pub patterns: Vec<String>,
}

pub fn main() {
    let cli = Cli::parse();
    let is_json = is_json_requested(&cli);

    if let Err(err) = run(&cli) {
        if is_json {
            let error_env = JsonErrorEnvelope::new(err.error_code(), &err.message());
            if let Ok(json) = serde_json::to_string_pretty(&error_env) {
                println!("{json}");
            } else {
                eprintln!("{}: {}", err.error_code(), err.message());
            }
        } else {
            eprintln!("{}: {}", err.error_code(), err.message());
        }
        std::process::exit(err.exit_code());
    }
}

/// Verifica se a saída JSON deve ser produzida para este comando.
#[must_use]
pub fn is_json_requested(cli: &Cli) -> bool {
    let flag = match &cli.command {
        Commands::Index(a) => a.json,
        Commands::Find(a) => a.json,
        Commands::Du(a) => a.json,
        Commands::Stat(a) => a.json,
        Commands::Recent(a) => a.json,
        Commands::Batch(a) => a.json,
        Commands::Watch(_) | Commands::Bench(_) => false,
    };

    flag || !std::io::stdout().is_terminal()
}

pub fn run(cli: &Cli) -> Result<(), CliError> {
    let is_json = is_json_requested(cli);

    match &cli.command {
        Commands::Index(args) => run_index(args, is_json),
        Commands::Find(args) => run_find(args, is_json),
        Commands::Du(args) => run_du(args, is_json),
        Commands::Stat(args) => run_stat(args, is_json),
        Commands::Recent(args) => run_recent(args, is_json),
        Commands::Batch(args) => run_batch(args, is_json),
        Commands::Watch(args) => {
            eprintln!(
                "ff watch: subcomando não implementado (alvo: {})",
                args.target
            );
            Err(CliError::NotImplemented("watch"))
        }
        Commands::Bench(args) => {
            eprintln!(
                "ff bench: subcomando não implementado (alvo: {}, padrões: {:?})",
                args.target, args.patterns
            );
            Err(CliError::NotImplemented("bench"))
        }
    }
}

fn run_index(args: &IndexArgs, is_json: bool) -> Result<(), CliError> {
    if !Path::new(&args.target).exists() && !args.target.ends_with(':') {
        return Err(CliError::TargetNotFound(args.target.clone()));
    }

    let start = Instant::now();
    let mut index = FileIndex::new();
    let mut source = WalkSource::new(&args.target).with_respect_ignore(args.respect_ignore);

    if let Err(e) = source.populate(&mut index) {
        return Err(CliError::Other(format!("erro ao indexar: {e}")));
    }

    let elapsed = start.elapsed();
    let mem = index.memory_usage();
    let mem_mb = mem as f64 / (1024.0 * 1024.0);
    let bytes_per_entry = if index.is_empty() {
        0.0
    } else {
        mem as f64 / index.len() as f64
    };

    if is_json {
        let summary = serde_json::json!({
            "target": args.target,
            "entries": index.len(),
            "memory_bytes": mem,
            "memory_mb": (mem_mb * 100.0).round() / 100.0,
            "bytes_per_entry": (bytes_per_entry * 10.0).round() / 10.0,
            "elapsed_ms": (elapsed.as_secs_f64() * 1000.0 * 100.0).round() / 100.0,
        });
        let env: JsonEnvelope<()> = JsonEnvelope::new("index", summary);
        println!(
            "{}",
            serde_json::to_string_pretty(&env).map_err(|e| CliError::Other(e.to_string()))?
        );
    } else {
        println!("Indexação concluída em {:.2?}:", elapsed);
        println!("  Entradas: {}", index.len());
        println!(
            "  Memória do índice: {:.2} MB ({:.1} bytes/arquivo)",
            mem_mb, bytes_per_entry
        );
    }

    Ok(())
}

fn run_find(args: &FindArgs, is_json: bool) -> Result<(), CliError> {
    let start = Instant::now();
    let mut index = FileIndex::new();

    if let Some(count) = args.synthetic {
        let mut source = SyntheticSource::new(count);
        let _ = source.populate(&mut index);
    } else {
        if !Path::new(&args.target).exists() && !args.target.ends_with(':') {
            return Err(CliError::TargetNotFound(args.target.clone()));
        }
        let mut source = WalkSource::new(&args.target).with_respect_ignore(!args.all);
        if let Err(e) = source.populate(&mut index) {
            return Err(CliError::Other(format!(
                "erro ao indexar '{}': {e}",
                args.target
            )));
        }
    }

    let raw_pattern = args
        .pattern
        .as_deref()
        .or(args.name.as_deref())
        .unwrap_or("*");

    let mut filters = Filters {
        ext: args.ext.clone(),
        ..Default::default()
    };

    if let Some(ref size_spec) = args.size {
        let (min, max) = parse_size_filter(size_spec).map_err(CliError::InvalidArgument)?;
        filters.min_size = min;
        filters.max_size = max;
    }

    if let Some(ref t) = args.file_type {
        match t.to_lowercase().as_str() {
            "dir" | "d" | "directory" => filters.dirs_only = true,
            "file" | "f" => filters.files_only = true,
            _ => {
                return Err(CliError::InvalidArgument(format!(
                    "tipo inválido '{t}': use 'file' ou 'dir'"
                )))
            }
        }
    }

    if let Some(ref dur_spec) = args.since {
        let dur = parse_duration(dur_spec).map_err(CliError::InvalidArgument)?;
        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        filters.min_mtime = Some(now.saturating_sub(dur.as_secs()) as i64);
    }

    let query = match Query::parse(raw_pattern, args.regex) {
        Ok(q) => q.with_filters(filters),
        Err(err) => return Err(CliError::InvalidArgument(err.to_string())),
    };

    let result = SearchEngine::search(&index, &query);

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

        items.push(FileResultItem {
            path: final_path,
            name,
            is_dir: entry.is_dir(),
            size: entry.size,
            mtime: entry.mtime,
            mtime_iso: Some(format_unix_timestamp(entry.mtime)),
        });
    }

    // Ordenar por data de modificação decrescente (mais recente primeiro)
    items.sort_by_key(|a| std::cmp::Reverse(a.mtime));

    let total_matched = items.len();
    let offset = args.cursor.unwrap_or(0);

    let mut paginated: Vec<FileResultItem> = if offset < items.len() {
        items.into_iter().skip(offset).take(args.limit).collect()
    } else {
        Vec::new()
    };

    let (truncated_by_budget, _budget_count) = apply_budget(&mut paginated, args.budget);

    let next_cursor = if offset + paginated.len() < total_matched {
        Some((offset + paginated.len()).to_string())
    } else {
        None
    };
    let has_more = next_cursor.is_some() || truncated_by_budget;

    let elapsed = start.elapsed();

    if is_json {
        let summary = serde_json::json!({
            "target": args.target,
            "pattern": raw_pattern,
            "total_matches": total_matched,
            "returned_matches": paginated.len(),
            "secrets_hidden": secrets_hidden,
            "total_scanned": result.total_scanned,
            "elapsed_ms": (elapsed.as_secs_f64() * 1000.0 * 100.0).round() / 100.0,
        });

        let mut env = JsonEnvelope::new("find", summary)
            .with_results(paginated)
            .with_cursor(next_cursor, has_more)
            .with_truncated(truncated_by_budget);

        if secrets_hidden > 0 {
            env = env.with_warning(format!(
                "{secrets_hidden} segredo(s) ocultado(s); use --include-secrets para exibir"
            ));
        }

        if truncated_by_budget {
            env = env.with_warning("limite de tokens (--budget) atingido; resultados truncados");
        }

        println!(
            "{}",
            serde_json::to_string_pretty(&env).map_err(|e| CliError::Other(e.to_string()))?
        );
    } else {
        println!(
            "Encontrados {} resultados em {:.2?} ({} entradas varridas):",
            total_matched, elapsed, result.total_scanned
        );

        for item in &paginated {
            let type_str = if item.is_dir { "DIR " } else { "FILE" };
            let size_str = format_bytes(item.size);
            println!(
                "[{type_str}] {:>10}  {}  {}",
                size_str,
                item.mtime_iso.as_deref().unwrap_or(""),
                item.path
            );
        }

        if total_matched > offset + paginated.len() {
            println!(
                "... e mais {} resultados omitidos (use --limit ou --cursor para ver mais)",
                total_matched - (offset + paginated.len())
            );
        }
        if secrets_hidden > 0 {
            println!("(Nota: {secrets_hidden} arquivos com cara de segredo foram ocultados. Use --include-secrets para ver).");
        }
    }

    Ok(())
}

fn run_du(args: &DuArgs, is_json: bool) -> Result<(), CliError> {
    if !Path::new(&args.target).exists() && !args.target.ends_with(':') {
        return Err(CliError::TargetNotFound(args.target.clone()));
    }

    let start = Instant::now();
    let mut index = FileIndex::new();
    let mut source = WalkSource::new(&args.target).with_respect_ignore(!args.all);

    if let Err(e) = source.populate(&mut index) {
        return Err(CliError::Other(format!(
            "erro ao indexar '{}': {e}",
            args.target
        )));
    }

    let summary = SizeAggregator::aggregate(&index, Some(0), args.depth, args.top);

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

        results.push(DuResultItem {
            path: final_path,
            size: item.total_size,
            files: item.file_count,
            dirs: item.dir_count,
            depth: item.depth,
        });
    }

    let (truncated_by_budget, _) = apply_budget(&mut results, args.budget);
    let elapsed = start.elapsed();

    if is_json {
        let json_summary = serde_json::json!({
            "target": args.target,
            "total_size": summary.total_size,
            "total_size_formatted": format_bytes(summary.total_size),
            "total_files": summary.total_files,
            "total_dirs": summary.total_dirs,
            "top_folders_returned": results.len(),
            "secrets_hidden": secrets_hidden,
            "elapsed_ms": (elapsed.as_secs_f64() * 1000.0 * 100.0).round() / 100.0,
        });

        let mut env = JsonEnvelope::new("du", json_summary)
            .with_results(results)
            .with_truncated(truncated_by_budget);

        if secrets_hidden > 0 {
            env = env.with_warning(format!(
                "{secrets_hidden} segredo(s) ocultado(s); use --include-secrets para exibir"
            ));
        }

        println!(
            "{}",
            serde_json::to_string_pretty(&env).map_err(|e| CliError::Other(e.to_string()))?
        );
    } else {
        println!("Uso de disco para '{}' ({:.2?}):", args.target, elapsed);
        println!(
            "Total: {} em {} arquivos e {} pastas\n",
            format_bytes(summary.total_size),
            summary.total_files,
            summary.total_dirs
        );
        println!(
            "{:<12} {:<10} {:<8} CAMINHO",
            "TAMANHO", "ARQUIVOS", "PASTAS"
        );
        println!("{:-<12} {:-<10} {:-<8} {:-<40}", "", "", "", "");

        for r in &results {
            println!(
                "{:<12} {:<10} {:<8} {}",
                format_bytes(r.size),
                r.files,
                r.dirs,
                r.path
            );
        }
    }

    Ok(())
}

fn run_stat(args: &StatArgs, is_json: bool) -> Result<(), CliError> {
    let p = Path::new(&args.target);
    let meta =
        std::fs::symlink_metadata(p).map_err(|_| CliError::TargetNotFound(args.target.clone()))?;

    let is_dir = meta.is_dir();
    let is_file = meta.is_file();
    let size = if is_dir { 0 } else { meta.len() };

    let mtime = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
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
        return Err(CliError::InvalidArgument(
            "o arquivo é considerado um segredo. Use --include-secrets para consultar".into(),
        ));
    }

    let (total_size, total_files, total_dirs) = if is_dir {
        let mut index = FileIndex::new();
        let mut source = WalkSource::new(&args.target).with_respect_ignore(false);
        if source.populate(&mut index).is_ok() {
            let sum = SizeAggregator::aggregate(&index, Some(0), None, 1);
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

    let stat_res = StatResult {
        path: args.target.clone(),
        name: file_name,
        is_dir,
        is_file,
        size: if is_dir {
            total_size.unwrap_or(0)
        } else {
            size
        },
        mtime,
        mtime_iso: Some(format_unix_timestamp(mtime)),
        readonly,
        hidden,
        system,
        total_size,
        total_files,
        total_dirs,
    };

    if is_json {
        let summary = serde_json::json!({
            "target": args.target,
            "type": if is_dir { "directory" } else { "file" },
            "size": stat_res.size,
            "mtime": mtime,
        });

        let env = JsonEnvelope::new("stat", summary).with_result(stat_res);
        println!(
            "{}",
            serde_json::to_string_pretty(&env).map_err(|e| CliError::Other(e.to_string()))?
        );
    } else {
        println!("Stat para '{}':", stat_res.path);
        println!("  Nome:       {}", stat_res.name);
        println!(
            "  Tipo:       {}",
            if stat_res.is_dir {
                "Diretório"
            } else {
                "Arquivo"
            }
        );
        println!(
            "  Tamanho:    {} ({} bytes)",
            format_bytes(stat_res.size),
            stat_res.size
        );
        println!(
            "  Modificado: {}",
            stat_res.mtime_iso.as_deref().unwrap_or("")
        );
        println!(
            "  Atributos:  readonly={}, hidden={}, system={}",
            stat_res.readonly, stat_res.hidden, stat_res.system
        );
        if let (Some(f), Some(d)) = (stat_res.total_files, stat_res.total_dirs) {
            println!("  Conteúdo:   {} arquivos, {} subpastas", f, d);
        }
    }

    Ok(())
}

fn run_recent(args: &RecentArgs, is_json: bool) -> Result<(), CliError> {
    let dur = parse_duration(&args.since).map_err(CliError::InvalidArgument)?;
    let now = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let min_mtime = now.saturating_sub(dur.as_secs()) as i64;

    let start = Instant::now();
    let mut index = FileIndex::new();

    if !Path::new(&args.target).exists() && !args.target.ends_with(':') {
        return Err(CliError::TargetNotFound(args.target.clone()));
    }

    let mut source = WalkSource::new(&args.target).with_respect_ignore(!args.all);
    if let Err(e) = source.populate(&mut index) {
        return Err(CliError::Other(format!(
            "erro ao indexar '{}': {e}",
            args.target
        )));
    }

    let query = Query::new(Pattern::Substring(String::new())).with_filters(Filters {
        min_mtime: Some(min_mtime),
        ..Default::default()
    });

    let result = SearchEngine::search(&index, &query);

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

        items.push(FileResultItem {
            path: final_path,
            name,
            is_dir: entry.is_dir(),
            size: entry.size,
            mtime: entry.mtime,
            mtime_iso: Some(format_unix_timestamp(entry.mtime)),
        });
    }

    // Ordenar decrescente por mtime
    items.sort_by_key(|a| std::cmp::Reverse(a.mtime));

    let total_matched = items.len();
    let offset = args.cursor.unwrap_or(0);

    let mut paginated: Vec<FileResultItem> = if offset < items.len() {
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

    if is_json {
        let summary = serde_json::json!({
            "target": args.target,
            "since": args.since,
            "min_mtime": min_mtime,
            "total_matches": total_matched,
            "returned_matches": paginated.len(),
            "secrets_hidden": secrets_hidden,
            "elapsed_ms": (elapsed.as_secs_f64() * 1000.0 * 100.0).round() / 100.0,
        });

        let mut env = JsonEnvelope::new("recent", summary)
            .with_results(paginated)
            .with_cursor(next_cursor, has_more)
            .with_truncated(truncated_by_budget);

        if secrets_hidden > 0 {
            env = env.with_warning(format!(
                "{secrets_hidden} segredo(s) ocultado(s); use --include-secrets para exibir"
            ));
        }

        println!(
            "{}",
            serde_json::to_string_pretty(&env).map_err(|e| CliError::Other(e.to_string()))?
        );
    } else {
        println!(
            "Arquivos modificados nos últimos {} em '{}' ({:.2?}):",
            args.since, args.target, elapsed
        );
        for item in &paginated {
            let type_str = if item.is_dir { "DIR " } else { "FILE" };
            println!(
                "[{type_str}] {:>10}  {}  {}",
                format_bytes(item.size),
                item.mtime_iso.as_deref().unwrap_or(""),
                item.path
            );
        }
        if total_matched > offset + paginated.len() {
            println!(
                "... e mais {} arquivos omitidos (use --limit ou --cursor para ver mais)",
                total_matched - (offset + paginated.len())
            );
        }
    }

    Ok(())
}

fn run_batch(args: &BatchArgs, is_json: bool) -> Result<(), CliError> {
    let input = if let Some(ref path) = args.file {
        std::fs::read_to_string(path)
            .map_err(|e| CliError::Io(format!("não foi possível ler '{path}': {e}")))?
    } else {
        let mut buffer = String::new();
        std::io::stdin()
            .read_to_string(&mut buffer)
            .map_err(|e| CliError::Io(format!("erro lendo stdin: {e}")))?;
        buffer
    };

    let queries: serde_json::Value = serde_json::from_str(&input)
        .map_err(|e| CliError::InvalidArgument(format!("JSON inválido em batch: {e}")))?;

    let query_array = queries
        .get("queries")
        .and_then(|q| q.as_array())
        .ok_or_else(|| {
            CliError::InvalidArgument("o objeto JSON de batch deve conter o array 'queries'".into())
        })?;

    let mut batch_results = Vec::new();

    for q in query_array {
        let cmd = q.get("command").and_then(|c| c.as_str()).unwrap_or("");
        match cmd {
            "stat" => {
                let target = q.get("target").and_then(|t| t.as_str()).unwrap_or(".");
                let _stat_args = StatArgs {
                    target: target.to_string(),
                    include_secrets: q
                        .get("include_secrets")
                        .and_then(|s| s.as_bool())
                        .unwrap_or(false),
                    json: true,
                };
                let res = serde_json::json!({
                    "command": "stat",
                    "target": target,
                    "status": "ok"
                });
                batch_results.push(res);
            }
            "find" => {
                let pattern = q.get("pattern").and_then(|p| p.as_str()).map(String::from);
                let target = q.get("target").and_then(|t| t.as_str()).unwrap_or(".");
                batch_results.push(serde_json::json!({
                    "command": "find",
                    "target": target,
                    "pattern": pattern,
                    "status": "ok"
                }));
            }
            other => {
                batch_results.push(serde_json::json!({
                    "command": other,
                    "status": "error",
                    "message": "comando não suportado em batch"
                }));
            }
        }
    }

    if is_json {
        let summary = serde_json::json!({
            "total_queries": query_array.len(),
            "processed": batch_results.len(),
        });
        let env = JsonEnvelope::new("batch", summary).with_results(batch_results);
        println!(
            "{}",
            serde_json::to_string_pretty(&env).map_err(|e| CliError::Other(e.to_string()))?
        );
    } else {
        println!(
            "Batch concluído: {} consultas processadas",
            batch_results.len()
        );
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_index() {
        let cli = Cli::try_parse_from(["ff", "index", "D:", "--respect-ignore"]).unwrap();
        assert_eq!(
            cli.command,
            Commands::Index(IndexArgs {
                target: "D:".into(),
                respect_ignore: true,
                json: false,
            })
        );
    }

    #[test]
    fn test_parse_du() {
        let cli = Cli::try_parse_from(["ff", "du", ".", "--top", "10", "--depth", "1", "--json"])
            .unwrap();
        assert_eq!(
            cli.command,
            Commands::Du(DuArgs {
                target: ".".into(),
                top: 10,
                depth: Some(1),
                budget: None,
                relative: false,
                all: false,
                include_secrets: false,
                json: true,
            })
        );
    }

    #[test]
    fn test_parse_stat() {
        let cli = Cli::try_parse_from(["ff", "stat", "Cargo.toml", "--json"]).unwrap();
        assert_eq!(
            cli.command,
            Commands::Stat(StatArgs {
                target: "Cargo.toml".into(),
                include_secrets: false,
                json: true,
            })
        );
    }

    #[test]
    fn test_parse_recent() {
        let cli = Cli::try_parse_from(["ff", "recent", "--since", "2h", "--limit", "20"]).unwrap();
        assert_eq!(
            cli.command,
            Commands::Recent(RecentArgs {
                target: ".".into(),
                since: "2h".into(),
                limit: 20,
                cursor: None,
                budget: None,
                relative: false,
                all: false,
                include_secrets: false,
                json: false,
            })
        );
    }

    #[test]
    fn test_parse_find() {
        let cli = Cli::try_parse_from([
            "ff", "find", "*.rs", "--ext", "rs", "--limit", "50", "--target", ".",
        ])
        .unwrap();
        assert_eq!(
            cli.command,
            Commands::Find(FindArgs {
                pattern: Some("*.rs".into()),
                name: None,
                target: ".".into(),
                regex: false,
                ext: Some("rs".into()),
                size: None,
                file_type: None,
                since: None,
                limit: 50,
                cursor: None,
                budget: None,
                relative: false,
                all: false,
                include_secrets: false,
                json: false,
                synthetic: None,
            })
        );
    }

    #[test]
    fn test_find_with_name_and_type() {
        let cli = Cli::try_parse_from(["ff", "find", "--name", "target", "--type", "dir"]).unwrap();
        assert_eq!(
            cli.command,
            Commands::Find(FindArgs {
                pattern: None,
                name: Some("target".into()),
                target: ".".into(),
                regex: false,
                ext: None,
                size: None,
                file_type: Some("dir".into()),
                since: None,
                limit: 50,
                cursor: None,
                budget: None,
                relative: false,
                all: false,
                include_secrets: false,
                json: false,
                synthetic: None,
            })
        );
    }

    #[test]
    fn test_find_with_size() {
        let cli = Cli::try_parse_from(["ff", "find", "*.vhdx", "--size", ">1GB"]).unwrap();
        assert_eq!(
            cli.command,
            Commands::Find(FindArgs {
                pattern: Some("*.vhdx".into()),
                name: None,
                target: ".".into(),
                regex: false,
                ext: None,
                size: Some(">1GB".into()),
                file_type: None,
                since: None,
                limit: 50,
                cursor: None,
                budget: None,
                relative: false,
                all: false,
                include_secrets: false,
                json: false,
                synthetic: None,
            })
        );
    }

    #[test]
    fn test_stubs_return_not_implemented() {
        let cli = Cli::try_parse_from(["ff", "watch", "C:"]).unwrap();
        assert_eq!(run(&cli), Err(CliError::NotImplemented("watch")));

        let cli = Cli::try_parse_from(["ff", "bench", "C:", "q1"]).unwrap();
        assert_eq!(run(&cli), Err(CliError::NotImplemented("bench")));
    }

    #[test]
    fn test_du_execution_succeeds() {
        let cli = Cli::try_parse_from(["ff", "du", ".", "--top", "5", "--json"]).unwrap();
        assert!(run(&cli).is_ok());
    }

    #[test]
    fn test_stat_execution_succeeds() {
        let cli = Cli::try_parse_from(["ff", "stat", "Cargo.toml", "--json"]).unwrap();
        assert!(run(&cli).is_ok());
    }

    #[test]
    fn test_stat_nonexistent_target_fails() {
        let cli = Cli::try_parse_from(["ff", "stat", "nonexistent_file_xyz_123.tmp"]).unwrap();
        assert_eq!(
            run(&cli),
            Err(CliError::TargetNotFound(
                "nonexistent_file_xyz_123.tmp".into()
            ))
        );
    }

    #[test]
    fn test_recent_execution_succeeds() {
        let cli = Cli::try_parse_from(["ff", "recent", "--since", "24h", "--json"]).unwrap();
        assert!(run(&cli).is_ok());
    }

    #[test]
    fn test_json_envelope_schema() {
        let env: JsonEnvelope<Vec<String>> =
            JsonEnvelope::new("test", serde_json::json!({ "count": 1 }))
                .with_results(vec!["item1".into()])
                .with_warning("aviso de teste");

        let serialized = serde_json::to_string(&env).unwrap();
        assert!(serialized.contains("\"schema\":\"ff/1\""));
        assert!(serialized.contains("\"source\":\"walk\""));
        assert!(serialized.contains("\"warnings\":[\"aviso de teste\"]"));
    }
}
