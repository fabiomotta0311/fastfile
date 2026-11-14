use clap::{Args, Parser, Subcommand};

/// Código de saída retornado quando um subcomando invocado ainda não foi implementado (stub).
pub const EXIT_NOT_IMPLEMENTED: i32 = 1;

/// Erros de execução do CLI.
#[derive(Debug, PartialEq, Eq)]
pub enum CliError {
    /// O subcomando ainda não está implementado.
    NotImplemented(&'static str),
    /// Erro de execução ou parâmetro inválido.
    Other(String),
}

impl CliError {
    /// Retorna o código de saída do processo para o erro.
    #[must_use]
    pub fn exit_code(&self) -> i32 {
        match self {
            Self::NotImplemented(_) => EXIT_NOT_IMPLEMENTED,
            Self::Other(_) => 1,
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
            || std::path::Path::new(val).exists();

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
}

#[derive(Args, Debug, PartialEq, Eq)]
pub struct FindArgs {
    /// Padrão de busca (substring, curingas como *.toml, etc.)
    pub pattern: String,

    /// Tratar padrão como expressão regular
    #[arg(short, long)]
    pub regex: bool,

    /// Filtrar por extensão de arquivo (ex.: rs, toml)
    #[arg(short, long)]
    pub ext: Option<String>,

    /// Limite máximo de resultados a exibir
    #[arg(short, long, default_value_t = 100)]
    pub limit: usize,

    /// Quantidade de entradas sintéticas para buscar (padrão: 100000)
    #[arg(short, long)]
    pub synthetic: Option<usize>,
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
    if let Err(err) = run(&cli) {
        std::process::exit(err.exit_code());
    }
}

pub fn run(cli: &Cli) -> Result<(), CliError> {
    match &cli.command {
        Commands::Index(args) => {
            eprintln!(
                "ff index: subcomando não implementado (alvo: {})",
                args.target
            );
            Err(CliError::NotImplemented("index"))
        }
        Commands::Find(args) => {
            let count = args.synthetic.unwrap_or(100_000);
            let mut index = ff_index::FileIndex::new();
            let mut source = ff_index::SyntheticSource::new(count);
            let _ = ff_index::IndexSource::populate(&mut source, &mut index);

            let query = match ff_search::Query::parse(&args.pattern, args.regex) {
                Ok(q) => {
                    let filters = ff_search::Filters {
                        ext: args.ext.clone(),
                        ..Default::default()
                    };
                    q.with_filters(filters)
                }
                Err(err) => {
                    eprintln!("ff find: erro no padrão de busca: {err}");
                    return Err(CliError::Other(err.to_string()));
                }
            };

            let result = ff_search::SearchEngine::search(&index, &query);
            println!(
                "Encontrados {} resultados em {:.2?} ({} entradas varridas):",
                result.matches.len(),
                result.elapsed,
                result.total_scanned
            );

            for m in result.matches.iter().take(args.limit) {
                if let Ok(path) = index.build_path(m.id) {
                    println!("{path}");
                }
            }

            if result.matches.len() > args.limit {
                println!(
                    "... e mais {} resultados omitidos (use --limit para ver mais)",
                    result.matches.len() - args.limit
                );
            }

            Ok(())
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_index() {
        let cli = Cli::try_parse_from(["ff", "index", "D:"]).unwrap();
        assert_eq!(
            cli.command,
            Commands::Index(IndexArgs {
                target: "D:".into()
            })
        );
    }

    #[test]
    fn test_index_without_target_fails() {
        let res = Cli::try_parse_from(["ff", "index"]);
        assert!(res.is_err());
    }

    #[test]
    fn test_parse_find() {
        let cli =
            Cli::try_parse_from(["ff", "find", "*.rs", "--ext", "rs", "--limit", "50"]).unwrap();
        assert_eq!(
            cli.command,
            Commands::Find(FindArgs {
                pattern: "*.rs".into(),
                regex: false,
                ext: Some("rs".into()),
                limit: 50,
                synthetic: None,
            })
        );
    }

    #[test]
    fn test_find_without_pattern_fails() {
        let res = Cli::try_parse_from(["ff", "find"]);
        assert!(res.is_err());
    }

    #[test]
    fn test_find_execution_succeeds() {
        let cli = Cli::try_parse_from(["ff", "find", "file_1", "--synthetic", "1000"]).unwrap();
        let res = run(&cli);
        assert!(res.is_ok());
    }

    #[test]
    fn test_parse_watch() {
        let cli = Cli::try_parse_from(["ff", "watch", "C:"]).unwrap();
        assert_eq!(
            cli.command,
            Commands::Watch(WatchArgs {
                target: "C:".into()
            })
        );
    }

    #[test]
    fn test_watch_without_target_fails() {
        let res = Cli::try_parse_from(["ff", "watch"]);
        assert!(res.is_err());
    }

    #[test]
    fn test_bench_without_target_fails() {
        let res = Cli::try_parse_from(["ff", "bench"]);
        assert!(res.is_err());
    }

    #[test]
    fn test_bench_with_target_and_patterns() {
        let cli = Cli::try_parse_from(["ff", "bench", "C:", "a", "b"]).unwrap();
        assert_eq!(
            cli.command,
            Commands::Bench(BenchArgs {
                target: "C:".into(),
                patterns: vec!["a".into(), "b".into()]
            })
        );
    }

    #[test]
    fn test_bench_invalid_target_fails() {
        let res = Cli::try_parse_from(["ff", "bench", "pat1", "pat2"]);
        assert!(res.is_err());
    }

    #[test]
    fn test_parse_fastfile_binary_name() {
        let cli = Cli::try_parse_from(["fastfile", "find", "main.rs"]).unwrap();
        assert_eq!(
            cli.command,
            Commands::Find(FindArgs {
                pattern: "main.rs".into(),
                regex: false,
                ext: None,
                limit: 100,
                synthetic: None,
            })
        );
    }

    #[test]
    fn test_stubs_return_not_implemented() {
        let cli = Cli::try_parse_from(["ff", "index", "C:"]).unwrap();
        assert_eq!(run(&cli), Err(CliError::NotImplemented("index")));

        let cli = Cli::try_parse_from(["ff", "watch", "C:"]).unwrap();
        assert_eq!(run(&cli), Err(CliError::NotImplemented("watch")));

        let cli = Cli::try_parse_from(["ff", "bench", "C:", "q1"]).unwrap();
        assert_eq!(run(&cli), Err(CliError::NotImplemented("bench")));
    }

    #[test]
    fn test_not_implemented_exit_code() {
        let err = CliError::NotImplemented("watch");
        assert_eq!(err.exit_code(), EXIT_NOT_IMPLEMENTED);
        assert_ne!(EXIT_NOT_IMPLEMENTED, 0);
    }
}
