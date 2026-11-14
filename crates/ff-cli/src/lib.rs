use clap::{Args, Parser, Subcommand};

/// Código de saída retornado quando um subcomando invocado ainda não foi implementado (stub).
pub const EXIT_NOT_IMPLEMENTED: i32 = 1;

/// Erros de execução do CLI.
#[derive(Debug, PartialEq, Eq)]
pub enum CliError {
    /// O subcomando ainda não está implementado.
    NotImplemented(&'static str),
}

impl CliError {
    /// Retorna o código de saída do processo para o erro.
    #[must_use]
    pub fn exit_code(&self) -> i32 {
        match self {
            Self::NotImplemented(_) => EXIT_NOT_IMPLEMENTED,
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
    /// Padrão de busca (substring, curingas, etc.)
    pub pattern: String,
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
            eprintln!(
                "ff find: subcomando não implementado (padrão: {})",
                args.pattern
            );
            Err(CliError::NotImplemented("find"))
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
        let cli = Cli::try_parse_from(["ff", "find", "*.rs"]).unwrap();
        assert_eq!(
            cli.command,
            Commands::Find(FindArgs {
                pattern: "*.rs".into()
            })
        );
    }

    #[test]
    fn test_find_without_pattern_fails() {
        let res = Cli::try_parse_from(["ff", "find"]);
        assert!(res.is_err());
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
                pattern: "main.rs".into()
            })
        );
    }

    #[test]
    fn test_subcommands_return_not_implemented() {
        let cli = Cli::try_parse_from(["ff", "index", "C:"]).unwrap();
        assert_eq!(run(&cli), Err(CliError::NotImplemented("index")));

        let cli = Cli::try_parse_from(["ff", "find", "query"]).unwrap();
        assert_eq!(run(&cli), Err(CliError::NotImplemented("find")));

        let cli = Cli::try_parse_from(["ff", "watch", "C:"]).unwrap();
        assert_eq!(run(&cli), Err(CliError::NotImplemented("watch")));

        let cli = Cli::try_parse_from(["ff", "bench", "C:", "q1"]).unwrap();
        assert_eq!(run(&cli), Err(CliError::NotImplemented("bench")));
    }

    #[test]
    fn test_not_implemented_exit_code() {
        let err = CliError::NotImplemented("find");
        assert_eq!(err.exit_code(), EXIT_NOT_IMPLEMENTED);
        assert_ne!(EXIT_NOT_IMPLEMENTED, 0);
    }
}
