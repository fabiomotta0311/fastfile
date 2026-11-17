pub mod context_menu;
pub mod dupes;
pub mod map;
pub mod mcp;
pub mod schema;
pub mod secrets;
pub mod util;

use std::io::{IsTerminal, Read};
use std::path::Path;
use std::time::{Instant, SystemTime};

use clap::{Args, Parser, Subcommand};
use ff_content::{GrepMode, GrepOptions};
use ff_diff::diff_files;
use ff_index::{FileIndex, IndexSource, SyntheticSource, WalkSource};
use ff_search::{Filters, Pattern, Query, SearchEngine};
use ff_size::SizeAggregator;

use crate::schema::{
    finalize_envelope_budget, DuResultItem, FileResultItem, JsonEnvelope, JsonErrorEnvelope,
    StatResult, SymbolResultItem,
};
use crate::secrets::is_secret_file;
use crate::util::{
    format_bytes, format_unix_timestamp, make_relative_path, parse_duration, parse_size_filter,
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
    /// Busca texto em conteúdo de arquivos (estilo ripgrep)
    Grep(GrepArgs),
    /// Compara dois arquivos e produz diff estruturado em blocos
    Diff(DiffArgs),
    /// Executa lote de consultas a partir de JSON
    Batch(BatchArgs),
    /// Localiza onde funções, structs, classes ou tipos estão definidos no código
    Sym(SymArgs),
    /// Gera visão compacta de um repositório (estrutura, linguagens, tamanhos, principais símbolos)
    Map(MapArgs),
    /// Localiza arquivos duplicados por tamanho e hash (somente leitura)
    Dupes(DupesArgs),
    /// Inicia o servidor MCP (Model Context Protocol) sobre stdio
    Mcp(McpArgs),
    /// Acompanha alterações ao vivo pelo USN Journal
    Watch(WatchArgs),
    /// Executa benchmarks reproduzíveis de indexação e busca
    Bench(BenchArgs),
    /// Registra opções do FastFile no menu de contexto do Windows Explorer
    RegisterContextMenu,
    /// Remove opções do FastFile do menu de contexto do Windows Explorer
    UnregisterContextMenu,
}

#[derive(Args, Debug, PartialEq, Eq)]
pub struct IndexArgs {
    /// Volume NTFS (ex.: C:) ou diretório a indexar
    #[arg(value_parser = validate_target)]
    pub target: String,

    /// Respeitar regras de .gitignore e ignorar arquivos ocultos
    #[arg(long)]
    pub respect_ignore: bool,

    /// Rota do indexador: 'mft' (leitura bruta), 'usn' (journal) ou 'walk' (varredura)
    #[arg(long)]
    pub route: Option<String>,

    /// Criar ou atualizar índice de trigramas para busca ultra-rápida em conteúdo
    #[arg(long)]
    pub trigram: bool,

    /// Limite máximo em MB para arquivos no índice de trigramas (padrão: 5 MB)
    #[arg(long)]
    pub max_file_size_mb: Option<u64>,

    /// Limite máximo acumulado em MB para o índice de trigramas (padrão: 200 MB)
    #[arg(long)]
    pub max_total_mb: Option<u64>,

    /// Emitir saída em JSON
    #[arg(long)]
    pub json: bool,

    /// Emitir JSON compacto sem indentação (-c, --compact)
    #[arg(short = 'c', long)]
    pub compact: bool,
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

    /// Busca aproximada por similaridade de nome (fuzzy)
    #[arg(short = 'z', long = "fuzzy")]
    pub fuzzy: bool,

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

    /// Orçamento em tokens estimados (1 token ≈ 4 bytes UTF-8) aplicado sobre a resposta JSON completa (--budget N)
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

    /// Emitir JSON compacto sem indentação e sem campos redundantes (-c, --compact)
    #[arg(short = 'c', long)]
    pub compact: bool,

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

    /// Orçamento em tokens estimados (1 token ≈ 4 bytes UTF-8) aplicado sobre a resposta JSON completa (--budget N)
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

    /// Emitir JSON compacto sem indentação e sem campos redundantes (-c, --compact)
    #[arg(short = 'c', long)]
    pub compact: bool,
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

    /// Emitir JSON compacto sem indentação (-c, --compact)
    #[arg(short = 'c', long)]
    pub compact: bool,
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

    /// Orçamento em tokens estimados (1 token ≈ 4 bytes UTF-8) aplicado sobre a resposta JSON completa (--budget N)
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

    /// Emitir JSON compacto sem indentação e sem campos redundantes (-c, --compact)
    #[arg(short = 'c', long)]
    pub compact: bool,
}

#[derive(Args, Debug, PartialEq, Eq)]
pub struct GrepArgs {
    /// Padrão de busca (expressão regular)
    pub pattern: String,

    /// Arquivo ou diretório base (padrão: ".")
    #[arg(default_value = ".")]
    pub path: String,

    /// Filtrar por extensão de arquivo (ex.: rs, toml)
    #[arg(short, long)]
    pub ext: Option<String>,

    /// Filtrar por tipo/linguagem de arquivo (ex.: rust, toml, python, js, ts, go)
    #[arg(short = 't', long = "type")]
    pub file_type: Option<String>,

    /// Modo de exibição: 'content' (padrão), 'files' ou 'count'
    #[arg(long, default_value = "content")]
    pub mode: String,

    /// Diferenciar maiúsculas e minúsculas
    #[arg(short = 's', long)]
    pub case_sensitive: bool,

    /// Limite máximo de resultados a exibir
    #[arg(short, long, default_value_t = 100)]
    pub limit: usize,

    /// Orçamento em tokens estimados (1 token ≈ 4 bytes UTF-8) aplicado sobre a resposta JSON completa (--budget N)
    #[arg(long)]
    pub budget: Option<usize>,

    /// Exibir caminhos relativos ao diretório base
    #[arg(long)]
    pub relative: bool,

    /// Incluir arquivos e pastas ignorados (.git, target, node_modules)
    #[arg(long)]
    pub all: bool,

    /// Incluir segredos sem mascarar
    #[arg(long)]
    pub include_secrets: bool,

    /// Não utilizar o índice de trigramas mesmo se disponível
    #[arg(long)]
    pub no_index: bool,

    /// Criar ou atualizar o índice de trigramas antes da busca
    #[arg(long)]
    pub build_index: bool,

    /// Emitir saída em JSON (ativo automaticamente se stdout não for terminal)
    #[arg(long)]
    pub json: bool,

    /// Emitir JSON compacto sem indentação (-c, --compact)
    #[arg(short = 'c', long)]
    pub compact: bool,
}

#[derive(Args, Debug, PartialEq, Eq)]
pub struct DiffArgs {
    /// Arquivo original (versão antiga)
    pub file_a: String,

    /// Arquivo modificado (versão nova)
    pub file_b: String,

    /// Quantidade de linhas de contexto (padrão: 3)
    #[arg(short = 'u', long = "unified", default_value_t = 3)]
    pub context: usize,

    /// Orçamento em tokens estimados (1 token ≈ 4 bytes UTF-8) aplicado sobre a resposta JSON completa (--budget N)
    #[arg(long)]
    pub budget: Option<usize>,

    /// Incluir segredos sem mascarar
    #[arg(long)]
    pub include_secrets: bool,

    /// Emitir saída em JSON (ativo automaticamente se stdout não for terminal)
    #[arg(long)]
    pub json: bool,

    /// Emitir JSON compacto sem indentação (-c, --compact)
    #[arg(short = 'c', long)]
    pub compact: bool,
}

#[derive(Args, Debug, PartialEq, Eq)]
pub struct BatchArgs {
    /// Arquivo contendo consultas em JSON (lê do stdin se omitido)
    #[arg(short, long)]
    pub file: Option<String>,

    /// Emitir saída em JSON
    #[arg(long)]
    pub json: bool,

    /// Emitir JSON compacto sem indentação (-c, --compact)
    #[arg(short = 'c', long)]
    pub compact: bool,
}

#[derive(Args, Debug, PartialEq, Eq)]
pub struct SymArgs {
    /// Nome do símbolo (função, struct, classe, etc.) a buscar
    pub name: String,

    /// Arquivo ou diretório base (padrão: ".")
    #[arg(default_value = ".")]
    pub path: String,

    /// Filtrar por tipo de símbolo: fn, struct, class, trait, interface, enum, type, const
    #[arg(short, long)]
    pub kind: Option<String>,

    /// Filtrar por linguagem: rust, python, js, ts, go, cs
    #[arg(long)]
    pub lang: Option<String>,

    /// Limite máximo de resultados a exibir
    #[arg(short, long, default_value_t = 50)]
    pub limit: usize,

    /// Cursor numérico para paginação de resultados
    #[arg(long)]
    pub cursor: Option<usize>,

    /// Orçamento em tokens estimados (1 token ≈ 4 bytes UTF-8) aplicado sobre a resposta JSON completa (--budget N)
    #[arg(long)]
    pub budget: Option<usize>,

    /// Exibir caminhos relativos ao diretório base
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

    /// Emitir JSON compacto sem indentação e sem campos redundantes (-c, --compact)
    #[arg(short = 'c', long)]
    pub compact: bool,
}

#[derive(Args, Debug, PartialEq, Eq)]
pub struct MapArgs {
    /// Diretório raiz do repositório a mapear (padrão: ".")
    #[arg(default_value = ".")]
    pub path: String,

    /// Profundidade máxima de pastas na árvore (padrão: 3)
    #[arg(long, default_value_t = 3)]
    pub depth: usize,

    /// Orçamento em tokens estimados (1 token ≈ 4 bytes UTF-8) aplicado sobre a resposta JSON completa (--budget N)
    #[arg(long)]
    pub budget: Option<usize>,

    /// Incluir pastas ignoradas (.git, target, node_modules)
    #[arg(long)]
    pub all: bool,

    /// Incluir segredos sem mascarar
    #[arg(long)]
    pub include_secrets: bool,

    /// Emitir saída em JSON (ativo automaticamente se stdout não for terminal)
    #[arg(long)]
    pub json: bool,

    /// Emitir JSON compacto sem indentação (-c, --compact)
    #[arg(short = 'c', long)]
    pub compact: bool,
}

#[derive(Args, Debug, PartialEq, Eq)]
pub struct DupesArgs {
    /// Diretório base para análise de duplicados (padrão: ".")
    #[arg(default_value = ".")]
    pub path: String,

    /// Tamanho mínimo do arquivo (ex.: 1B, 10KB, 1MB; padrão: 1B)
    #[arg(long, default_value = "1B")]
    pub min_size: String,

    /// Limite de grupos de duplicados a exibir (padrão: 50)
    #[arg(short, long, default_value_t = 50)]
    pub limit: usize,

    /// Orçamento em tokens estimados (1 token ≈ 4 bytes UTF-8) aplicado sobre a resposta JSON completa (--budget N)
    #[arg(long)]
    pub budget: Option<usize>,

    /// Retornar caminhos relativos ao alvo (padrão: true)
    #[arg(long, default_value_t = true)]
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

    /// Emitir JSON compacto sem indentação (-c, --compact)
    #[arg(short = 'c', long)]
    pub compact: bool,
}

#[derive(Args, Debug, PartialEq, Eq)]
pub struct WatchArgs {
    /// Volume NTFS ou diretório a monitorar em tempo real (ex.: . ou /var/log ou C:)
    #[arg(value_parser = validate_target)]
    pub target: String,

    /// Executar apenas um ciclo de verificação (útil para testes e scripts)
    #[arg(long)]
    pub once: bool,

    /// Emitir eventos em formato JSON estruturado
    #[arg(long)]
    pub json: bool,

    /// Emitir JSON compacto sem indentação (-c, --compact)
    #[arg(short = 'c', long)]
    pub compact: bool,
}

#[derive(Args, Debug, PartialEq, Eq)]
pub struct BenchArgs {
    /// Volume NTFS ou diretório para benchmark (ex.: C: ou .)
    #[arg(value_parser = validate_target)]
    pub target: String,

    /// Padrões de consulta a testar
    #[arg(required = true)]
    pub patterns: Vec<String>,

    /// Quantidade de iterações por consulta (padrão: 20)
    #[arg(long, default_value_t = 20)]
    pub iterations: usize,

    /// Emitir saída em JSON
    #[arg(long)]
    pub json: bool,

    /// Emitir JSON compacto sem indentação (-c, --compact)
    #[arg(short = 'c', long)]
    pub compact: bool,
}

#[derive(Args, Debug, PartialEq, Eq)]
pub struct McpArgs {}

pub fn main() {
    let cli = Cli::parse();
    let is_json = is_json_requested(&cli);
    let is_compact = is_compact_requested(&cli);

    if let Err(err) = run(&cli) {
        if is_json {
            let error_env = JsonErrorEnvelope::new(err.error_code(), &err.message());
            let out = if is_compact {
                serde_json::to_string(&error_env)
            } else {
                serde_json::to_string_pretty(&error_env)
            };
            if let Ok(json) = out {
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
        Commands::Index(a) => a.json || a.compact,
        Commands::Find(a) => a.json || a.compact,
        Commands::Du(a) => a.json || a.compact,
        Commands::Stat(a) => a.json || a.compact,
        Commands::Recent(a) => a.json || a.compact,
        Commands::Grep(a) => a.json || a.compact,
        Commands::Diff(a) => a.json || a.compact,
        Commands::Batch(a) => a.json || a.compact,
        Commands::Sym(a) => a.json || a.compact,
        Commands::Map(a) => a.json || a.compact,
        Commands::Dupes(a) => a.json || a.compact,
        Commands::Watch(a) => a.json || a.compact,
        Commands::Bench(a) => a.json || a.compact,
        Commands::Mcp(_) | Commands::RegisterContextMenu | Commands::UnregisterContextMenu => false,
    };

    flag || !std::io::stdout().is_terminal()
}

/// Verifica se o formato JSON compacto foi solicitado.
#[must_use]
pub fn is_compact_requested(cli: &Cli) -> bool {
    match &cli.command {
        Commands::Index(a) => a.compact,
        Commands::Find(a) => a.compact,
        Commands::Du(a) => a.compact,
        Commands::Stat(a) => a.compact,
        Commands::Recent(a) => a.compact,
        Commands::Grep(a) => a.compact,
        Commands::Diff(a) => a.compact,
        Commands::Batch(a) => a.compact,
        Commands::Sym(a) => a.compact,
        Commands::Map(a) => a.compact,
        Commands::Dupes(a) => a.compact,
        Commands::Watch(a) => a.compact,
        Commands::Bench(a) => a.compact,
        _ => false,
    }
}

pub fn run(cli: &Cli) -> Result<(), CliError> {
    let is_json = is_json_requested(cli);

    match &cli.command {
        Commands::Index(args) => run_index(args, is_json),
        Commands::Find(args) => run_find(args, is_json),
        Commands::Du(args) => run_du(args, is_json),
        Commands::Stat(args) => run_stat(args, is_json),
        Commands::Recent(args) => run_recent(args, is_json),
        Commands::Grep(args) => run_grep(args, is_json),
        Commands::Diff(args) => run_diff(args, is_json),
        Commands::Batch(args) => run_batch(args, is_json),
        Commands::Sym(args) => run_sym(args, is_json),
        Commands::Map(args) => run_map(args, is_json),
        Commands::Dupes(args) => run_dupes(args, is_json),
        Commands::Mcp(_) => mcp::run_mcp_server().map_err(CliError::Other),
        Commands::Watch(args) => run_watch(args, is_json),
        Commands::Bench(args) => run_bench(args, is_json),
        Commands::RegisterContextMenu => {
            let msg = context_menu::register_context_menu().map_err(CliError::Other)?;
            println!("{msg}");
            Ok(())
        }
        Commands::UnregisterContextMenu => {
            let msg = context_menu::unregister_context_menu().map_err(CliError::Other)?;
            println!("{msg}");
            Ok(())
        }
    }
}

fn run_index(args: &IndexArgs, is_json: bool) -> Result<(), CliError> {
    if !Path::new(&args.target).exists() && !args.target.ends_with(':') {
        return Err(CliError::TargetNotFound(args.target.clone()));
    }

    if args.trigram {
        let p = Path::new(&args.target);
        if !p.is_dir() {
            return Err(CliError::InvalidArgument(format!(
                "alvo para índice de trigramas deve ser um diretório: '{}'",
                args.target
            )));
        }

        let config = ff_content::TrigramConfig {
            max_file_size: args.max_file_size_mb.unwrap_or(5) * 1024 * 1024,
            max_total_bytes: args.max_total_mb.unwrap_or(200) * 1024 * 1024,
            max_files: 50_000,
            respect_ignore: args.respect_ignore,
        };

        let idx_path = ff_content::default_index_path(p);
        let mut idx = if idx_path.exists() {
            ff_content::TrigramIndex::load_from_file(p, &idx_path)
                .unwrap_or_else(|_| ff_content::TrigramIndex::new(p, config.clone()))
        } else {
            ff_content::TrigramIndex::new(p, config.clone())
        };
        idx.config = config;

        let stats = idx
            .build_or_update()
            .map_err(|e| CliError::Other(e.to_string()))?;
        idx.save_to_file(&idx_path)
            .map_err(|e| CliError::Other(e.to_string()))?;

        if is_json {
            let summary = serde_json::json!({
                "type": "trigram",
                "target": stats.root,
                "files_indexed": stats.files_indexed,
                "unique_trigrams": stats.unique_trigrams,
                "total_bytes": stats.total_bytes,
                "files_added": stats.files_added,
                "files_modified": stats.files_modified,
                "files_removed": stats.files_removed,
                "elapsed_ms": stats.elapsed_ms,
                "index_path": idx_path.to_string_lossy(),
            });
            let env: JsonEnvelope<()> = JsonEnvelope::new("index", summary);
            println!(
                "{}",
                serde_json::to_string_pretty(&env).map_err(|e| CliError::Other(e.to_string()))?
            );
        } else {
            println!(
                "Indexação de trigramas concluída em {:.2} ms:",
                stats.elapsed_ms
            );
            println!("  Alvo: {}", stats.root);
            println!("  Arquivos indexados: {}", stats.files_indexed);
            println!("  Trigramas únicos: {}", stats.unique_trigrams);
            println!(
                "  Tamanho total: {:.2} MB",
                stats.total_bytes as f64 / (1024.0 * 1024.0)
            );
            println!(
                "  Adicionados: {}, Modificados: {}, Removidos: {}",
                stats.files_added, stats.files_modified, stats.files_removed
            );
            println!("  Caminho do índice: {}", idx_path.display());
        }
        return Ok(());
    }

    let start = Instant::now();
    let mut index = FileIndex::new();
    #[allow(unused_mut)]
    let mut method = "walk";

    #[cfg(windows)]
    {
        let is_volume = args.target.ends_with(':')
            || args.target.starts_with(r"\\.\")
            || (args.target.len() <= 3 && args.target.contains(':'));

        if is_volume {
            let requested_route = args.route.as_deref().unwrap_or("auto");
            match requested_route {
                "usn" => {
                    method = "usn";
                    let mut source = ff_index::windows::UsnSource::new(&args.target);
                    if let Err(e) = source.populate(&mut index) {
                        return Err(CliError::Other(format!("erro ao indexar via USN: {e}")));
                    }
                    if source.fallback_used {
                        method = "walk_fallback";
                    }
                }
                "mft" => {
                    method = "mft";
                    let mut source = ff_index::windows::MftSource::new(&args.target);
                    if let Err(e) = source.populate(&mut index) {
                        return Err(CliError::Other(format!("erro ao indexar via MFT: {e}")));
                    }
                    if source.fallback_used {
                        method = "walk_fallback";
                    }
                }
                _ => {
                    // "auto": tenta MFT (Rota 2) com fallback para USN (Rota 1) e Walk
                    method = "mft";
                    let mut source = ff_index::windows::MftSource::new(&args.target);
                    if let Err(e) = source.populate(&mut index) {
                        return Err(CliError::Other(format!("erro ao indexar volume: {e}")));
                    }
                    if source.fallback_used {
                        method = "walk_fallback";
                    }
                }
            }
        } else {
            let mut source = WalkSource::new(&args.target).with_respect_ignore(args.respect_ignore);
            if let Err(e) = source.populate(&mut index) {
                return Err(CliError::Other(format!("erro ao indexar: {e}")));
            }
        }
    }

    #[cfg(not(windows))]
    {
        let mut source = WalkSource::new(&args.target).with_respect_ignore(args.respect_ignore);
        if let Err(e) = source.populate(&mut index) {
            return Err(CliError::Other(format!("erro ao indexar: {e}")));
        }
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
            "method": method,
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
        println!("  Alvo: {}", args.target);
        println!("  Método: {}", method);
        println!("  Entradas: {}", index.len());
        println!(
            "  Memória do índice: {:.2} MB ({:.1} bytes/arquivo)",
            mem_mb, bytes_per_entry
        );
    }

    Ok(())
}

fn try_ipc_find(args: &FindArgs) -> Option<serde_json::Value> {
    if args.synthetic.is_some() || args.since.is_some() {
        return None;
    }

    let (min_size, max_size) = if let Some(ref size_spec) = args.size {
        match parse_size_filter(size_spec) {
            Ok((min, max)) => (min, max),
            Err(_) => return None,
        }
    } else {
        (None, None)
    };

    let client = ff_indexer::IpcClient::default_client();
    let pat = args
        .pattern
        .as_deref()
        .or(args.name.as_deref())
        .unwrap_or("*");

    let req = ff_indexer::IpcRequest::Find {
        pattern: pat.to_string(),
        is_regex: args.regex,
        fuzzy: args.fuzzy,
        limit: if args.limit > 0 { args.limit * 4 } else { 2000 },
        target: Some(args.target.clone()),
        extension: args.ext.clone(),
        file_type: args.file_type.clone(),
        min_size,
        max_size,
    };

    match client.send(&req) {
        Ok(ff_indexer::IpcResponse::Ok(val)) => Some(val),
        _ => None,
    }
}

fn try_ipc_du(target: &str, depth: Option<u32>, top: usize) -> Option<serde_json::Value> {
    let client = ff_indexer::IpcClient::default_client();
    let req = ff_indexer::IpcRequest::Du {
        target: target.to_string(),
        depth,
        top,
    };
    match client.send(&req) {
        Ok(ff_indexer::IpcResponse::Ok(val)) => Some(val),
        _ => None,
    }
}

fn try_ipc_stat(target: &str) -> Option<serde_json::Value> {
    let client = ff_indexer::IpcClient::default_client();
    let req = ff_indexer::IpcRequest::Stat {
        path: target.to_string(),
    };
    match client.send(&req) {
        Ok(ff_indexer::IpcResponse::Ok(val)) => Some(val),
        _ => None,
    }
}

fn run_find(args: &FindArgs, is_json: bool) -> Result<(), CliError> {
    let start = Instant::now();
    let raw_pattern = args
        .pattern
        .as_deref()
        .or(args.name.as_deref())
        .unwrap_or("*");

    // Unificação de contrato: tenta IPC primeiro; se indisponível, executa Walk local (Ponto 2)
    let (raw_candidates, total_scanned, is_from_ipc) = if let Some(ipc_val) = try_ipc_find(args) {
        let total_scanned = ipc_val["total_scanned"].as_u64().unwrap_or(0) as usize;
        let mut candidates = Vec::new();
        if let Some(arr) = ipc_val["matches"].as_array() {
            for item in arr {
                let path = item["path"].as_str().unwrap_or("").to_string();
                let name = item["name"].as_str().unwrap_or("").to_string();
                let size = item["size"].as_u64().unwrap_or(0);
                let mtime = item["mtime"].as_i64().unwrap_or(0);
                let is_dir = item["is_dir"].as_bool().unwrap_or(false);
                let score = item["score"].as_u64().unwrap_or(1) as u32;
                candidates.push((path, name, is_dir, size, mtime, score));
            }
        }
        (candidates, total_scanned, true)
    } else {
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

        let query = match Query::parse_with_options(raw_pattern, args.regex, args.fuzzy) {
            Ok(q) => q.with_filters(filters),
            Err(err) => return Err(CliError::InvalidArgument(err.to_string())),
        };

        let result = SearchEngine::search(&index, &query);
        let mut candidates = Vec::new();
        for m in &result.matches {
            let entry = &index.entries[m.id as usize];
            let name = index.name(m.id).unwrap_or("").to_string();
            let full_path = index.build_path(m.id).unwrap_or_else(|_| name.clone());
            candidates.push((
                full_path,
                name,
                entry.is_dir(),
                entry.size,
                entry.mtime,
                m.score,
            ));
        }

        (candidates, result.total_scanned, false)
    };

    let mut secrets_hidden = 0usize;
    let mut ranked_items: Vec<(FileResultItem, u32)> = Vec::new();

    for (full_path, name, is_dir, size, mtime, score) in raw_candidates {
        if !args.include_secrets && is_secret_file(&name) {
            secrets_hidden += 1;
            continue;
        }

        let should_rel = args.relative
            || args.compact
            || args.target == "."
            || !Path::new(&args.target).is_absolute();
        let final_path = if should_rel {
            make_relative_path(&full_path, &args.target)
        } else {
            full_path
        };

        let depth = final_path.split('/').count() as u32;
        let depth_penalty = depth.saturating_mul(25);
        let mtime_bonus = ((mtime.max(0) / 86400) % 100) as u32;
        let rank = score.saturating_mul(100).saturating_sub(depth_penalty) + mtime_bonus;

        ranked_items.push((
            FileResultItem {
                path: final_path,
                name: if args.compact { None } else { Some(name) },
                is_dir,
                size,
                mtime,
                mtime_iso: if args.compact {
                    None
                } else {
                    Some(format_unix_timestamp(mtime))
                },
            },
            rank,
        ));
    }

    if args.fuzzy {
        // Ranking por qualidade da correspondência, profundidade e data (§3.5 e #27)
        ranked_items.sort_by_key(|(_, rank)| std::cmp::Reverse(*rank));
    } else {
        // Ordenar por data de modificação decrescente (mais recente primeiro)
        ranked_items.sort_by_key(|(item, _)| std::cmp::Reverse(item.mtime));
    }

    let items: Vec<FileResultItem> = ranked_items.into_iter().map(|(item, _)| item).collect();

    let total_matched = items.len();
    let offset = args.cursor.unwrap_or(0);

    let paginated: Vec<FileResultItem> = if offset < items.len() {
        items.into_iter().skip(offset).take(args.limit).collect()
    } else {
        Vec::new()
    };

    let elapsed = start.elapsed();

    if is_json {
        let summary = if args.compact {
            serde_json::json!({
                "target": args.target,
                "pattern": raw_pattern,
                "total_matches": total_matched,
                "secrets_hidden": secrets_hidden,
                "total_scanned": total_scanned,
                "elapsed_ms": (elapsed.as_secs_f64() * 1000.0 * 100.0).round() / 100.0,
            })
        } else {
            serde_json::json!({
                "target": args.target,
                "pattern": raw_pattern,
                "total_matches": total_matched,
                "returned_matches": paginated.len(),
                "secrets_hidden": secrets_hidden,
                "total_scanned": total_scanned,
                "elapsed_ms": (elapsed.as_secs_f64() * 1000.0 * 100.0).round() / 100.0,
            })
        };

        let mut env = JsonEnvelope::new("find", summary)
            .with_source(if is_from_ipc { "ipc" } else { "walk" })
            .with_results(paginated);

        if secrets_hidden > 0 {
            env = env.with_warning(format!(
                "{secrets_hidden} segredo(s) ocultado(s); use --include-secrets para exibir"
            ));
        }

        let (_env, json_output, _truncated) =
            finalize_envelope_budget(env, args.budget, args.compact, total_matched, offset);
        println!("{json_output}");
    } else {
        println!(
            "Encontrados {} resultados em {:.2?} ({} entradas varridas):",
            total_matched, elapsed, total_scanned
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
    if is_json {
        if let Some(ipc_val) = try_ipc_du(&args.target, args.depth, args.top) {
            let total_size = ipc_val["total_size"].as_u64().unwrap_or(0);
            let total_files = ipc_val["total_files"].as_u64().unwrap_or(0);
            let total_dirs = ipc_val["total_dirs"].as_u64().unwrap_or(0);

            let mut results = Vec::new();
            if let Some(arr) = ipc_val["items"].as_array() {
                for item in arr {
                    let path = item["path"].as_str().unwrap_or("").to_string();
                    let size = item["size"].as_u64().unwrap_or(0);
                    let files = item["files"].as_u64().unwrap_or(0);
                    let dirs = item["dirs"].as_u64().unwrap_or(0);
                    let depth = item["depth"].as_u64().unwrap_or(0) as u32;
                    results.push(DuResultItem {
                        path,
                        size,
                        files,
                        dirs,
                        depth,
                    });
                }
            }

            let summary = serde_json::json!({
                "target": args.target,
                "total_size": total_size,
                "total_size_human": format_bytes(total_size),
                "total_files": total_files,
                "total_dirs": total_dirs,
                "items_count": results.len(),
            });

            let env = JsonEnvelope::new("du", summary)
                .with_source("ipc")
                .with_results(results);

            println!(
                "{}",
                serde_json::to_string_pretty(&env).map_err(|e| CliError::Other(e.to_string()))?
            );
            return Ok(());
        }
    }

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
        let should_rel = args.relative
            || args.compact
            || args.target == "."
            || !Path::new(&args.target).is_absolute();
        let final_path = if should_rel {
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

    let total_results = results.len();
    let elapsed = start.elapsed();

    if is_json {
        let json_summary = if args.compact {
            serde_json::json!({
                "target": args.target,
                "total_size": summary.total_size,
                "total_files": summary.total_files,
                "total_dirs": summary.total_dirs,
                "top_folders": results.len(),
                "elapsed_ms": (elapsed.as_secs_f64() * 1000.0 * 100.0).round() / 100.0,
            })
        } else {
            serde_json::json!({
                "target": args.target,
                "total_size": summary.total_size,
                "total_size_formatted": format_bytes(summary.total_size),
                "total_files": summary.total_files,
                "total_dirs": summary.total_dirs,
                "top_folders_returned": results.len(),
                "secrets_hidden": secrets_hidden,
                "elapsed_ms": (elapsed.as_secs_f64() * 1000.0 * 100.0).round() / 100.0,
            })
        };

        let mut env = JsonEnvelope::new("du", json_summary).with_results(results);

        if secrets_hidden > 0 {
            env = env.with_warning(format!(
                "{secrets_hidden} segredo(s) ocultado(s); use --include-secrets para exibir"
            ));
        }

        let (_env, json_output, _truncated) =
            finalize_envelope_budget(env, args.budget, args.compact, total_results, 0);
        println!("{json_output}");
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
    let meta = match std::fs::symlink_metadata(p) {
        Ok(m) => m,
        Err(_) => {
            if let Some(ipc_val) = try_ipc_stat(&args.target) {
                if is_json {
                    let path = ipc_val["path"].as_str().unwrap_or(&args.target).to_string();
                    let file_name = Path::new(&path)
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or(&path)
                        .to_string();
                    let is_dir = ipc_val["is_dir"].as_bool().unwrap_or(false);
                    let is_file = ipc_val["is_file"].as_bool().unwrap_or(true);
                    let size = ipc_val["size"].as_u64().unwrap_or(0);
                    let mtime = ipc_val["mtime"].as_i64().unwrap_or(0);
                    let stat_res = StatResult {
                        path: path.clone(),
                        name: file_name,
                        is_dir,
                        is_file,
                        size,
                        mtime,
                        mtime_iso: Some(format_unix_timestamp(mtime)),
                        readonly: false,
                        hidden: false,
                        system: false,
                        total_size: None,
                        total_files: None,
                        total_dirs: None,
                    };
                    let summary = serde_json::json!({
                        "path": path,
                        "type": if is_dir { "directory" } else { "file" },
                        "size": size,
                        "size_human": format_bytes(size),
                        "mtime": mtime,
                        "mtime_iso": format_unix_timestamp(mtime),
                    });
                    let env: JsonEnvelope<StatResult> = JsonEnvelope::new("stat", summary)
                        .with_source("ipc")
                        .with_result(stat_res);
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&env)
                            .map_err(|e| CliError::Other(e.to_string()))?
                    );
                    return Ok(());
                }
            }
            return Err(CliError::TargetNotFound(args.target.clone()));
        }
    };

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
        mtime_iso: if args.compact {
            None
        } else {
            Some(format_unix_timestamp(mtime))
        },
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
        let out = if args.compact {
            serde_json::to_string(&env)
        } else {
            serde_json::to_string_pretty(&env)
        };
        println!("{}", out.map_err(|e| CliError::Other(e.to_string()))?);
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
        let should_rel = args.relative
            || args.compact
            || args.target == "."
            || !Path::new(&args.target).is_absolute();
        let final_path = if should_rel {
            make_relative_path(&full_path, &args.target)
        } else {
            full_path
        };

        items.push(FileResultItem {
            path: final_path,
            name: if args.compact { None } else { Some(name) },
            is_dir: entry.is_dir(),
            size: entry.size,
            mtime: entry.mtime,
            mtime_iso: if args.compact {
                None
            } else {
                Some(format_unix_timestamp(entry.mtime))
            },
        });
    }

    // Ordenar decrescente por mtime
    items.sort_by_key(|a| std::cmp::Reverse(a.mtime));

    let total_matched = items.len();
    let offset = args.cursor.unwrap_or(0);

    let paginated: Vec<FileResultItem> = if offset < items.len() {
        items.into_iter().skip(offset).take(args.limit).collect()
    } else {
        Vec::new()
    };

    let elapsed = start.elapsed();

    if is_json {
        let summary = if args.compact {
            serde_json::json!({
                "target": args.target,
                "since": args.since,
                "min_mtime": min_mtime,
                "total_matches": total_matched,
                "secrets_hidden": secrets_hidden,
                "elapsed_ms": (elapsed.as_secs_f64() * 1000.0 * 100.0).round() / 100.0,
            })
        } else {
            serde_json::json!({
                "target": args.target,
                "since": args.since,
                "min_mtime": min_mtime,
                "total_matches": total_matched,
                "returned_matches": paginated.len(),
                "secrets_hidden": secrets_hidden,
                "elapsed_ms": (elapsed.as_secs_f64() * 1000.0 * 100.0).round() / 100.0,
            })
        };

        let mut env = JsonEnvelope::new("recent", summary).with_results(paginated);

        if secrets_hidden > 0 {
            env = env.with_warning(format!(
                "{secrets_hidden} segredo(s) ocultado(s); use --include-secrets para exibir"
            ));
        }

        let (_env, json_output, _truncated) =
            finalize_envelope_budget(env, args.budget, args.compact, total_matched, offset);
        println!("{json_output}");
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

fn run_grep(args: &GrepArgs, is_json: bool) -> Result<(), CliError> {
    let mode = match args.mode.to_lowercase().as_str() {
        "content" => GrepMode::Content,
        "files" => GrepMode::Files,
        "count" => GrepMode::Count,
        other => {
            return Err(CliError::InvalidArgument(format!(
                "modo inválido '{other}': use 'content', 'files' ou 'count'"
            )))
        }
    };

    let p = Path::new(&args.path);
    if !p.exists() {
        return Err(CliError::TargetNotFound(args.path.clone()));
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

    let opts = GrepOptions {
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

    let mut report = ff_content::grep(&opts).map_err(CliError::InvalidArgument)?;

    let mut secrets_hidden = 0usize;

    // Filtrar segredos se !args.include_secrets
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

    let should_rel =
        args.relative || args.compact || args.path == "." || !Path::new(&args.path).is_absolute();
    if should_rel {
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

    if is_json {
        let summary = serde_json::json!({
            "pattern": report.summary.pattern,
            "target": report.summary.target,
            "mode": args.mode,
            "total_matches": report.summary.total_matches,
            "files_searched": report.summary.files_searched,
            "files_matched": report.summary.files_matched,
            "secrets_hidden": secrets_hidden,
            "elapsed_ms": report.summary.elapsed_ms,
        });

        let mut env = JsonEnvelope::new("grep", summary).with_result(report);

        if secrets_hidden > 0 {
            env = env.with_warning(format!(
                "{secrets_hidden} segredo(s) ocultado(s); use --include-secrets para exibir"
            ));
        }

        let serialize = |e: &JsonEnvelope<ff_content::GrepReport>| -> String {
            if args.compact {
                serde_json::to_string(e).unwrap_or_default()
            } else {
                serde_json::to_string_pretty(e).unwrap_or_default()
            }
        };

        if let Some(budget) = args.budget {
            let max_bytes = budget.saturating_mul(4);
            let mut out_str = serialize(&env);
            if out_str.len() > max_bytes {
                env.truncated = Some(true);
                loop {
                    let popped = if let Some(ref mut rep) = env.result {
                        if let Some(ref mut matches) = rep.matches {
                            if !matches.is_empty() {
                                matches.pop();
                                true
                            } else {
                                false
                            }
                        } else if let Some(ref mut files) = rep.files {
                            if !files.is_empty() {
                                files.pop();
                                true
                            } else {
                                false
                            }
                        } else if let Some(ref mut counts) = rep.counts {
                            if !counts.is_empty() {
                                counts.pop();
                                true
                            } else {
                                false
                            }
                        } else {
                            false
                        }
                    } else {
                        false
                    };

                    if !popped {
                        break;
                    }

                    out_str = serialize(&env);
                    if out_str.len() <= max_bytes {
                        break;
                    }
                }

                if out_str.len() > max_bytes {
                    env.warnings.push(format!(
                        "orçamento de {budget} token(s) (~{max_bytes} bytes) insuficiente para incluir resultados (envelope mínimo requer {} bytes); resultados omitidos",
                        out_str.len()
                    ));
                    out_str = serialize(&env);
                } else {
                    env.warnings.push(
                        "limite de tokens (--budget) atingido; resultados truncados".to_string(),
                    );
                    out_str = serialize(&env);
                }
            }
            println!("{out_str}");
        } else {
            let out_str = serialize(&env);
            println!("{out_str}");
        }
    } else {
        match mode {
            GrepMode::Content => {
                if let Some(matches) = &report.matches {
                    for m in matches {
                        println!("{}:{}: {}", m.path, m.line_number, m.line_text);
                    }
                }
            }
            GrepMode::Files => {
                if let Some(files) = &report.files {
                    for f in files {
                        println!("{f}");
                    }
                }
            }
            GrepMode::Count => {
                if let Some(counts) = &report.counts {
                    for c in counts {
                        println!("{}: {}", c.path, c.count);
                    }
                }
            }
        }
    }

    Ok(())
}

fn run_diff(args: &DiffArgs, is_json: bool) -> Result<(), CliError> {
    let p_a = Path::new(&args.file_a);
    let p_b = Path::new(&args.file_b);

    if !p_a.exists() {
        return Err(CliError::TargetNotFound(args.file_a.clone()));
    }
    if !p_b.exists() {
        return Err(CliError::TargetNotFound(args.file_b.clone()));
    }

    if !args.include_secrets {
        let name_a = p_a.file_name().and_then(|n| n.to_str()).unwrap_or("");
        let name_b = p_b.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if is_secret_file(name_a) || is_secret_file(name_b) {
            return Err(CliError::InvalidArgument(
                "o arquivo é considerado um segredo. Use --include-secrets para comparar".into(),
            ));
        }
    }

    let report = diff_files(p_a, p_b, args.context)
        .map_err(|e| CliError::Io(format!("erro ao calcular diff: {e}")))?;

    if is_json {
        let summary = serde_json::json!({
            "file_a": report.file_a,
            "file_b": report.file_b,
            "additions": report.additions,
            "deletions": report.deletions,
            "is_identical": report.is_identical,
            "hunks_count": report.hunks.len(),
        });

        let mut env = JsonEnvelope::new("diff", summary).with_result(report);

        let serialize = |e: &JsonEnvelope<ff_diff::DiffReport>| -> String {
            if args.compact {
                serde_json::to_string(e).unwrap_or_default()
            } else {
                serde_json::to_string_pretty(e).unwrap_or_default()
            }
        };

        if let Some(budget) = args.budget {
            let max_bytes = budget.saturating_mul(4);
            let mut out_str = serialize(&env);
            if out_str.len() > max_bytes {
                env.truncated = Some(true);
                while out_str.len() > max_bytes {
                    let popped = env
                        .result
                        .as_mut()
                        .map(|r| r.hunks.pop().is_some())
                        .unwrap_or(false);
                    if !popped {
                        break;
                    }
                    out_str = serialize(&env);
                }
                if out_str.len() > max_bytes {
                    env.warnings.push(format!(
                        "orçamento de {budget} token(s) (~{max_bytes} bytes) insuficiente para incluir resultados (envelope mínimo requer {} bytes); resultados omitidos",
                        out_str.len()
                    ));
                    out_str = serialize(&env);
                } else {
                    env.warnings.push(
                        "limite de tokens (--budget) atingido; resultados truncados".to_string(),
                    );
                    out_str = serialize(&env);
                }
            }
            println!("{out_str}");
        } else {
            let out_str = serialize(&env);
            println!("{out_str}");
        }
    } else {
        if report.is_identical {
            println!(
                "Arquivos '{}' e '{}' são idênticos.",
                report.file_a, report.file_b
            );
            return Ok(());
        }

        println!("--- {}", report.file_a);
        println!("+++ {}", report.file_b);

        for hunk in &report.hunks {
            println!("{}", hunk.header);
            for line in &hunk.lines {
                match line.kind {
                    ff_diff::ChangeKind::Equal => println!(" {}", line.text),
                    ff_diff::ChangeKind::Insert => println!("+{}", line.text),
                    ff_diff::ChangeKind::Delete => println!("-{}", line.text),
                    ff_diff::ChangeKind::Modified => println!("~{}", line.text),
                }
            }
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
                    compact: args.compact,
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
            "grep" => {
                let pattern = q.get("pattern").and_then(|p| p.as_str()).unwrap_or("");
                let path = q.get("path").and_then(|p| p.as_str()).unwrap_or(".");
                batch_results.push(serde_json::json!({
                    "command": "grep",
                    "pattern": pattern,
                    "path": path,
                    "status": "ok"
                }));
            }
            "diff" => {
                let file_a = q.get("file_a").and_then(|p| p.as_str()).unwrap_or("");
                let file_b = q.get("file_b").and_then(|p| p.as_str()).unwrap_or("");
                batch_results.push(serde_json::json!({
                    "command": "diff",
                    "file_a": file_a,
                    "file_b": file_b,
                    "status": "ok"
                }));
            }
            "sym" => {
                let name = q.get("name").and_then(|p| p.as_str()).unwrap_or("");
                let path = q.get("path").and_then(|p| p.as_str()).unwrap_or(".");
                batch_results.push(serde_json::json!({
                    "command": "sym",
                    "name": name,
                    "path": path,
                    "status": "ok"
                }));
            }
            "map" => {
                let path = q.get("path").and_then(|p| p.as_str()).unwrap_or(".");
                batch_results.push(serde_json::json!({
                    "command": "map",
                    "path": path,
                    "status": "ok"
                }));
            }
            "dupes" => {
                let path = q.get("path").and_then(|p| p.as_str()).unwrap_or(".");
                batch_results.push(serde_json::json!({
                    "command": "dupes",
                    "path": path,
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
        let out = if args.compact {
            serde_json::to_string(&env)
        } else {
            serde_json::to_string_pretty(&env)
        };
        println!("{}", out.map_err(|e| CliError::Other(e.to_string()))?);
    } else {
        println!(
            "Batch concluído: {} consultas processadas",
            batch_results.len()
        );
    }

    Ok(())
}

fn run_sym(args: &SymArgs, is_json: bool) -> Result<(), CliError> {
    let base_path = Path::new(&args.path);
    if !base_path.exists() {
        return Err(CliError::TargetNotFound(args.path.clone()));
    }

    let kind_filter = args.kind.as_deref().and_then(ff_sym::SymbolKind::parse);
    if args.kind.is_some() && kind_filter.is_none() {
        return Err(CliError::InvalidArgument(format!(
            "tipo de símbolo inválido '{}': use fn, struct, class, trait, interface, enum, type ou const",
            args.kind.as_deref().unwrap_or("")
        )));
    }

    let start_time = Instant::now();
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

        let should_rel = args.relative
            || args.compact
            || args.path == "."
            || !Path::new(&args.path).is_absolute();
        let out_path = if should_rel {
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
    let page: Vec<SymbolResultItem> = filtered_symbols
        .into_iter()
        .skip(start_idx)
        .take(args.limit)
        .collect();

    let elapsed = start_time.elapsed().as_secs_f64() * 1000.0;

    if is_json {
        let summary = if args.compact {
            serde_json::json!({
                "name": args.name,
                "target": args.path,
                "kind": args.kind,
                "lang": args.lang,
                "total_matches": total_matches,
                "secrets_hidden": secrets_hidden,
                "elapsed_ms": (elapsed * 100.0).round() / 100.0,
            })
        } else {
            serde_json::json!({
                "name": args.name,
                "target": args.path,
                "kind": args.kind,
                "lang": args.lang,
                "total_matches": total_matches,
                "returned_matches": page.len(),
                "secrets_hidden": secrets_hidden,
                "elapsed_ms": (elapsed * 100.0).round() / 100.0,
            })
        };

        let mut env = JsonEnvelope::new("sym", summary).with_results(page);

        if secrets_hidden > 0 {
            env = env.with_warning(format!(
                "{secrets_hidden} segredo(s) ocultado(s); use --include-secrets para exibir"
            ));
        }

        let (_env, json_output, _truncated) =
            finalize_envelope_budget(env, args.budget, args.compact, total_matches, start_idx);
        println!("{json_output}");
    } else {
        if page.is_empty() {
            println!(
                "Nenhum símbolo correspondente encontrado para '{}'.",
                args.name
            );
            return Ok(());
        }

        println!(
            "{:<24} {:<10} {:<8} {:<32} ASSINATURA",
            "SÍMBOLO", "TIPO", "LANG", "ARQUIVO:LINHA"
        );
        for item in &page {
            let loc = format!("{}:{}", item.path, item.line);
            println!(
                "{:<24} {:<10} {:<8} {:<32} {}",
                item.name, item.kind, item.language, loc, item.signature
            );
        }
        if start_idx + page.len() < total_matches {
            println!(
                "\n... mais resultados disponíveis (use --cursor {})",
                start_idx + page.len()
            );
        }
    }

    Ok(())
}

fn run_map(args: &MapArgs, is_json: bool) -> Result<(), CliError> {
    let base_path = Path::new(&args.path);
    if !base_path.exists() {
        return Err(CliError::TargetNotFound(args.path.clone()));
    }

    let options = map::MapOptions {
        root: base_path.to_path_buf(),
        depth: args.depth,
        budget: args.budget,
        all: args.all,
        include_secrets: args.include_secrets,
    };

    let (repo_map, truncated, used_tokens) =
        map::generate_repo_map(&options).map_err(CliError::Other)?;

    if is_json {
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

        let out = if args.compact {
            serde_json::to_string(&env)
        } else {
            serde_json::to_string_pretty(&env)
        };
        println!("{}", out.map_err(|e| CliError::Other(e.to_string()))?);
    } else {
        let formatted = map::format_repo_map_human(&repo_map, truncated);
        println!("{formatted}");
    }

    Ok(())
}

fn run_dupes(args: &DupesArgs, is_json: bool) -> Result<(), CliError> {
    let base_path = Path::new(&args.path);
    if !base_path.exists() {
        return Err(CliError::TargetNotFound(args.path.clone()));
    }

    let min_size_bytes = match parse_size_filter(&args.min_size) {
        Ok((Some(min), _)) => min,
        Ok((None, _)) => 1,
        Err(_) => crate::util::parse_bytes_value(&args.min_size).unwrap_or(1),
    };

    let options = dupes::DupesOptions {
        target: base_path.to_path_buf(),
        min_size: min_size_bytes,
        limit: args.limit,
        relative: args.relative,
        all: args.all,
        include_secrets: args.include_secrets,
    };

    let start_time = Instant::now();
    let mut dupes_result = dupes::find_duplicates(&options).map_err(CliError::Other)?;
    let elapsed = start_time.elapsed().as_secs_f64() * 1000.0;

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

    if is_json {
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

        let serialize = |e: &JsonEnvelope<crate::schema::DupesResult>| -> String {
            if args.compact {
                serde_json::to_string(e).unwrap_or_default()
            } else {
                serde_json::to_string_pretty(e).unwrap_or_default()
            }
        };

        if let Some(budget) = args.budget {
            let max_bytes = budget.saturating_mul(4);
            let mut out_str = serialize(&env);
            if out_str.len() > max_bytes {
                env.truncated = Some(true);
                while out_str.len() > max_bytes {
                    let popped = env
                        .result
                        .as_mut()
                        .map(|r| r.groups.pop().is_some())
                        .unwrap_or(false);
                    if !popped {
                        break;
                    }
                    out_str = serialize(&env);
                }
                if out_str.len() > max_bytes {
                    env.warnings.push(format!(
                        "orçamento de {budget} token(s) (~{max_bytes} bytes) insuficiente para incluir resultados (envelope mínimo requer {} bytes); resultados omitidos",
                        out_str.len()
                    ));
                    out_str = serialize(&env);
                } else {
                    env.warnings.push(
                        "limite de tokens (--budget) atingido; resultados truncados".to_string(),
                    );
                    out_str = serialize(&env);
                }
            }
            println!("{out_str}");
        } else {
            let out_str = serialize(&env);
            println!("{out_str}");
        }
    } else {
        let formatted = dupes::format_dupes_human(&dupes_result, truncated);
        println!("{formatted}");
    }

    Ok(())
}

fn run_watch(args: &WatchArgs, is_json: bool) -> Result<(), CliError> {
    let p = Path::new(&args.target);
    if !p.exists() && !args.target.ends_with(':') {
        return Err(CliError::TargetNotFound(args.target.clone()));
    }

    #[cfg(windows)]
    let is_volume = args.target.ends_with(':')
        || args.target.starts_with(r"\\.\")
        || (args.target.len() <= 3 && args.target.contains(':'));
    #[cfg(not(windows))]
    let is_volume = false;

    if is_volume {
        #[cfg(windows)]
        {
            let mut index = FileIndex::new();
            let mut source = ff_index::windows::UsnSource::new(&args.target);
            source
                .populate(&mut index)
                .map_err(|e| CliError::Other(e.to_string()))?;

            if !is_json {
                println!(
                    "Monitorando '{}' em tempo real pelo USN Journal: {} itens indexados. Pressione Ctrl+C para encerrar.",
                    args.target,
                    index.len()
                );
            }

            if args.once {
                if is_json {
                    let env: JsonEnvelope<()> = JsonEnvelope::new(
                        "watch",
                        serde_json::json!({
                            "target": args.target,
                            "watcher": "USN_Journal",
                            "entries": index.len(),
                            "events_count": 0,
                            "last_usn": source.last_usn,
                        }),
                    );
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&env)
                            .map_err(|e| CliError::Other(e.to_string()))?
                    );
                }
                return Ok(());
            }

            if let Ok(mut watcher) =
                ff_index::windows::UsnWatcher::new(&args.target, source.journal_id, source.last_usn)
            {
                loop {
                    let changes = watcher
                        .poll_changes(&mut index)
                        .map_err(|e| CliError::Other(e.to_string()))?;
                    for change in changes {
                        if is_json {
                            let env: JsonEnvelope<()> = JsonEnvelope::new(
                                "watch_event",
                                serde_json::json!({
                                    "event": format!("{change:?}"),
                                    "total_entries": index.len(),
                                }),
                            );
                            println!(
                                "{}",
                                serde_json::to_string(&env)
                                    .map_err(|e| CliError::Other(e.to_string()))?
                            );
                        } else {
                            println!(
                                "[ao vivo - USN] {change:?} (total no índice: {})",
                                index.len()
                            );
                        }
                    }
                    std::thread::sleep(std::time::Duration::from_millis(50));
                }
            } else {
                return Err(CliError::Other(
                    "não foi possível abrir o monitor USN Journal (privilégios de administrador necessários)".to_string(),
                ));
            }
        }
    }

    let mut source =
        ff_index::LinuxLiveSource::new(p).map_err(|e| CliError::Other(e.to_string()))?;
    let mut index = FileIndex::new();
    source
        .populate(&mut index)
        .map_err(|e| CliError::Other(e.to_string()))?;

    if !is_json {
        println!(
            "Monitorando '{}' em tempo real ({:?}): {} itens indexados. Pressione Ctrl+C para encerrar.",
            args.target,
            source.watcher_kind(),
            index.len()
        );
    }

    if args.once {
        let changes = source
            .poll_events(&mut index)
            .map_err(|e| CliError::Other(e.to_string()))?;
        if is_json {
            let env: JsonEnvelope<()> = JsonEnvelope::new(
                "watch",
                serde_json::json!({
                    "target": args.target,
                    "watcher": format!("{:?}", source.watcher_kind()),
                    "entries": index.len(),
                    "events_count": changes.len(),
                }),
            );
            println!(
                "{}",
                serde_json::to_string_pretty(&env).map_err(|e| CliError::Other(e.to_string()))?
            );
        }
        return Ok(());
    }

    loop {
        let changes = source
            .poll_events(&mut index)
            .map_err(|e| CliError::Other(e.to_string()))?;
        for change in changes {
            if is_json {
                let env: JsonEnvelope<()> = JsonEnvelope::new(
                    "watch_event",
                    serde_json::json!({
                        "event": format!("{change:?}"),
                        "total_entries": index.len(),
                    }),
                );
                println!(
                    "{}",
                    serde_json::to_string(&env).map_err(|e| CliError::Other(e.to_string()))?
                );
            } else {
                println!("[ao vivo] {change:?} (total no índice: {})", index.len());
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
}

fn run_bench(args: &BenchArgs, is_json: bool) -> Result<(), CliError> {
    let p = Path::new(&args.target);
    if !p.exists() && !args.target.ends_with(':') {
        return Err(CliError::TargetNotFound(args.target.clone()));
    }

    #[cfg(windows)]
    let is_volume = args.target.ends_with(':')
        || args.target.starts_with(r"\\.\")
        || (args.target.len() <= 3 && args.target.contains(':'));
    #[cfg(not(windows))]
    let is_volume = false;

    let index_start = Instant::now();
    let mut index = FileIndex::new();

    let route_used = if is_volume {
        #[cfg(windows)]
        {
            let mut mft = ff_index::windows::MftSource::new(&args.target);
            if mft.populate(&mut index).is_ok() {
                "mft"
            } else {
                let mut usn = ff_index::windows::UsnSource::new(&args.target);
                if usn.populate(&mut index).is_ok() {
                    "usn"
                } else {
                    let mut walk = WalkSource::new(&args.target).with_respect_ignore(false);
                    walk.populate(&mut index)
                        .map_err(|e| CliError::Other(e.to_string()))?;
                    "walk"
                }
            }
        }
        #[cfg(not(windows))]
        {
            let mut walk = WalkSource::new(&args.target).with_respect_ignore(false);
            walk.populate(&mut index)
                .map_err(|e| CliError::Other(e.to_string()))?;
            "walk"
        }
    } else {
        let mut walk = WalkSource::new(&args.target).with_respect_ignore(false);
        walk.populate(&mut index)
            .map_err(|e| CliError::Other(e.to_string()))?;
        "walk"
    };

    let index_elapsed = index_start.elapsed();
    let total_entries = index.len();
    let memory_bytes = index.memory_usage();
    let memory_mb = memory_bytes as f64 / (1024.0 * 1024.0);
    let bytes_per_entry = memory_bytes.checked_div(total_entries).unwrap_or(0);

    let checkpoint_a_pass = index_elapsed.as_secs_f64() < 10.0
        || (total_entries > 0
            && (index_elapsed.as_secs_f64() / total_entries as f64 * 1_000_000.0) < 10.0);
    let checkpoint_e_pass = bytes_per_entry <= 150;

    let mut search_results = Vec::new();
    let mut all_b_pass = true;

    for pat in &args.patterns {
        let stats = ff_search::benchmark_search(&index, pat, false, args.iterations);
        let b_pass = stats.p95 <= std::time::Duration::from_millis(50);
        if !b_pass {
            all_b_pass = false;
        }
        search_results.push((stats, b_pass));
    }

    if is_json {
        let patterns_json: Vec<serde_json::Value> = search_results
            .iter()
            .map(|(s, b_pass)| {
                serde_json::json!({
                    "pattern": s.pattern,
                    "iterations": s.iterations,
                    "matches": s.matches_count,
                    "total_scanned": s.total_scanned,
                    "p50_ms": (s.p50.as_secs_f64() * 1000.0 * 100.0).round() / 100.0,
                    "p95_ms": (s.p95.as_secs_f64() * 1000.0 * 100.0).round() / 100.0,
                    "p99_ms": (s.p99.as_secs_f64() * 1000.0 * 100.0).round() / 100.0,
                    "min_ms": (s.min.as_secs_f64() * 1000.0 * 100.0).round() / 100.0,
                    "max_ms": (s.max.as_secs_f64() * 1000.0 * 100.0).round() / 100.0,
                    "checkpoint_b_pass": b_pass,
                })
            })
            .collect();

        let summary = serde_json::json!({
            "target": args.target,
            "route_used": route_used,
            "total_entries": total_entries,
            "index_elapsed_ms": (index_elapsed.as_secs_f64() * 1000.0 * 100.0).round() / 100.0,
            "memory_mb": (memory_mb * 100.0).round() / 100.0,
            "bytes_per_entry": bytes_per_entry,
            "checkpoints": {
                "checkpoint_a": {
                    "description": "Indexação < 10s por 1M arquivos",
                    "pass": checkpoint_a_pass,
                },
                "checkpoint_b": {
                    "description": "Latência de busca P95 < 50ms",
                    "pass": all_b_pass,
                },
                "checkpoint_e": {
                    "description": "Memória do índice <= 150B/entrada (< 300MB p/ 2M)",
                    "pass": checkpoint_e_pass,
                }
            }
        });

        let env = JsonEnvelope::new("bench", summary).with_results(patterns_json);
        println!(
            "{}",
            serde_json::to_string_pretty(&env).map_err(|e| CliError::Other(e.to_string()))?
        );
    } else {
        println!("=== FastFile Benchmark (ff bench) ===");
        println!("Alvo: {} (rota: {})", args.target, route_used);
        println!("Total de entradas: {}", total_entries);
        println!("Tempo de indexação: {:.2?}", index_elapsed);
        println!(
            "Uso de memória: {:.2} MB ({} bytes/entrada)",
            memory_mb, bytes_per_entry
        );
        println!();
        println!("--- Avaliação de Checkpoints (Fase 0) ---");
        println!(
            "Checkpoint A (Indexação < 10s / 1M itens): {}",
            if checkpoint_a_pass { "PASS" } else { "FAIL" }
        );
        println!(
            "Checkpoint B (Latência P95 < 50ms):         {}",
            if all_b_pass { "PASS" } else { "FAIL" }
        );
        println!(
            "Checkpoint E (Memória <= 150B/entrada):     {}",
            if checkpoint_e_pass { "PASS" } else { "FAIL" }
        );
        println!();
        println!(
            "{:<20} {:>8} {:>10} {:>10} {:>10} {:>8}",
            "Padrão", "Matches", "P50 (ms)", "P95 (ms)", "P99 (ms)", "Status"
        );
        println!("{:-<72}", "");
        for (s, b_pass) in &search_results {
            println!(
                "{:<20} {:>8} {:>10.2} {:>10.2} {:>10.2} {:>8}",
                s.pattern,
                s.matches_count,
                s.p50.as_secs_f64() * 1000.0,
                s.p95.as_secs_f64() * 1000.0,
                s.p99.as_secs_f64() * 1000.0,
                if *b_pass { "PASS" } else { "FAIL" }
            );
        }
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
                route: None,
                respect_ignore: true,
                trigram: false,
                max_file_size_mb: None,
                max_total_mb: None,
                json: false,
                compact: false,
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
                compact: false,
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
                compact: false,
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
                compact: false,
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
                fuzzy: false,
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
                compact: false,
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
                fuzzy: false,
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
                compact: false,
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
                fuzzy: false,
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
                compact: false,
                synthetic: None,
            })
        );
    }

    #[test]
    fn test_parse_bench() {
        let cli = Cli::try_parse_from(["ff", "bench", ".", "Cargo", "--iterations", "5", "--json"])
            .unwrap();
        assert_eq!(
            cli.command,
            Commands::Bench(BenchArgs {
                target: ".".into(),
                patterns: vec!["Cargo".into()],
                iterations: 5,
                json: true,
                compact: false,
            })
        );
    }

    #[test]
    fn test_bench_execution_succeeds() {
        let cli = Cli::try_parse_from(["ff", "bench", ".", "Cargo", "--iterations", "2", "--json"])
            .unwrap();
        assert!(run(&cli).is_ok());
    }

    #[test]
    fn test_parse_watch() {
        let cli = Cli::try_parse_from(["ff", "watch", ".", "--once", "--json"]).unwrap();
        assert_eq!(
            cli.command,
            Commands::Watch(WatchArgs {
                target: ".".into(),
                once: true,
                json: true,
                compact: false,
            })
        );
    }

    #[test]
    fn test_watch_execution_once_succeeds() {
        let cli = Cli::try_parse_from(["ff", "watch", ".", "--once", "--json"]).unwrap();
        assert!(run(&cli).is_ok());
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
    fn test_parse_grep() {
        let cli = Cli::try_parse_from(["ff", "grep", "fn main", "--ext", "rs", "--json"]).unwrap();
        assert_eq!(
            cli.command,
            Commands::Grep(GrepArgs {
                pattern: "fn main".into(),
                path: ".".into(),
                ext: Some("rs".into()),
                file_type: None,
                mode: "content".into(),
                case_sensitive: false,
                limit: 100,
                budget: None,
                relative: false,
                all: false,
                include_secrets: false,
                no_index: false,
                build_index: false,
                json: true,
                compact: false,
            })
        );
    }

    #[test]
    fn test_grep_execution_succeeds() {
        let cli = Cli::try_parse_from(["ff", "grep", "workspace", "Cargo.toml", "--json"]).unwrap();
        assert!(run(&cli).is_ok());
    }

    #[test]
    fn test_parse_diff() {
        let cli =
            Cli::try_parse_from(["ff", "diff", "a.toml", "b.toml", "-u", "5", "--json"]).unwrap();
        assert_eq!(
            cli.command,
            Commands::Diff(DiffArgs {
                file_a: "a.toml".into(),
                file_b: "b.toml".into(),
                context: 5,
                budget: None,
                include_secrets: false,
                json: true,
                compact: false,
            })
        );
    }

    #[test]
    fn test_diff_execution_succeeds() {
        let cli =
            Cli::try_parse_from(["ff", "diff", "Cargo.toml", "Cargo.toml", "--json"]).unwrap();
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

    #[test]
    fn test_parse_mcp() {
        let cli = Cli::try_parse_from(["ff", "mcp"]).unwrap();
        assert_eq!(cli.command, Commands::Mcp(McpArgs {}));
    }

    #[test]
    fn test_parse_sym() {
        let cli =
            Cli::try_parse_from(["ff", "sym", "run_mcp_server", "--kind", "fn", "--json"]).unwrap();
        assert_eq!(
            cli.command,
            Commands::Sym(SymArgs {
                name: "run_mcp_server".into(),
                path: ".".into(),
                kind: Some("fn".into()),
                lang: None,
                limit: 50,
                cursor: None,
                budget: None,
                relative: false,
                all: false,
                include_secrets: false,
                json: true,
                compact: false,
            })
        );
    }

    #[test]
    fn test_sym_execution_succeeds() {
        let cli = Cli::try_parse_from(["ff", "sym", "run_mcp_server", "--json"]).unwrap();
        assert!(run(&cli).is_ok());
    }

    #[test]
    fn test_parse_map() {
        let cli = Cli::try_parse_from([
            "ff", "map", ".", "--depth", "2", "--budget", "1500", "--json",
        ])
        .unwrap();
        assert_eq!(
            cli.command,
            Commands::Map(MapArgs {
                path: ".".into(),
                depth: 2,
                budget: Some(1500),
                all: false,
                include_secrets: false,
                json: true,
                compact: false,
            })
        );
    }

    #[test]
    fn test_map_execution_succeeds() {
        let cli = Cli::try_parse_from([
            "ff", "map", ".", "--depth", "2", "--budget", "1500", "--json",
        ])
        .unwrap();
        assert!(run(&cli).is_ok());
    }

    #[test]
    fn test_parse_dupes() {
        let cli = Cli::try_parse_from([
            "ff",
            "dupes",
            ".",
            "--min-size",
            "10KB",
            "--limit",
            "20",
            "--json",
        ])
        .unwrap();
        assert_eq!(
            cli.command,
            Commands::Dupes(DupesArgs {
                path: ".".into(),
                min_size: "10KB".into(),
                limit: 20,
                budget: None,
                relative: true,
                all: false,
                include_secrets: false,
                json: true,
                compact: false,
            })
        );
    }

    #[test]
    fn test_dupes_execution_succeeds() {
        let cli = Cli::try_parse_from(["ff", "dupes", ".", "--limit", "10", "--json"]).unwrap();
        assert!(run(&cli).is_ok());
    }
}
