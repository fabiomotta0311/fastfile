use clap::{Args, Parser, Subcommand};

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
    #[arg(default_value = "C:")]
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
    #[arg(default_value = "C:")]
    pub target: String,
}

#[derive(Args, Debug, PartialEq, Eq)]
pub struct BenchArgs {
    /// Volume NTFS para benchmark (ex.: C:)
    #[arg(default_value = "C:")]
    pub target: String,

    /// Padrões de consulta a testar
    pub patterns: Vec<String>,
}

pub fn main() {
    let cli = Cli::parse();
    run(cli);
}

pub fn run(cli: Cli) {
    match cli.command {
        Commands::Index(args) => {
            println!("ff index stub: alvo={}", args.target);
        }
        Commands::Find(args) => {
            println!("ff find stub: padrão={}", args.pattern);
        }
        Commands::Watch(args) => {
            println!("ff watch stub: alvo={}", args.target);
        }
        Commands::Bench(args) => {
            println!(
                "ff bench stub: alvo={}, padrões={:?}",
                args.target, args.patterns
            );
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
    fn test_parse_bench() {
        let cli = Cli::try_parse_from(["ff", "bench", "C:", "pat1", "pat2"]).unwrap();
        assert_eq!(
            cli.command,
            Commands::Bench(BenchArgs {
                target: "C:".into(),
                patterns: vec!["pat1".into(), "pat2".into()]
            })
        );
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
}
