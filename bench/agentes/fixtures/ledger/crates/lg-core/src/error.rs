use std::fmt;

/// Erros de domínio do razão. Mensagens em português porque aparecem na CLI.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    InvalidDate(String),
    InvalidAmount(String),
    UnknownCurrency(String),
    CurrencyMismatch { left: &'static str, right: &'static str },
    InvalidAccount(String),
    Overflow,
    Parse { line: usize, message: String },
    Config(String),
}

pub type Result<T> = std::result::Result<T, Error>;

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::InvalidDate(s) => write!(f, "data inválida: {s}"),
            Error::InvalidAmount(s) => write!(f, "valor inválido: {s}"),
            Error::UnknownCurrency(s) => write!(f, "moeda desconhecida: {s}"),
            Error::CurrencyMismatch { left, right } => write!(f, "moedas diferentes: {left} e {right}"),
            Error::InvalidAccount(s) => write!(f, "conta inválida: {s}"),
            Error::Overflow => write!(f, "valor fora do intervalo suportado"),
            Error::Parse { line, message } => write!(f, "linha {line}: {message}"),
            Error::Config(s) => write!(f, "configuração: {s}"),
        }
    }
}

impl std::error::Error for Error {}
