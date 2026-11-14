//! Definição de consultas, padrões de busca e filtros.

use regex::Regex;

/// Tipo de padrão de correspondência de nomes.
#[derive(Debug, Clone)]
pub enum Pattern {
    /// Substring exata em minúsculas (busca ultra-rápida via memchr/memmem).
    Substring(String),
    /// Expressão regular compilada (também usada para curingas glob).
    Regex(Regex),
}

impl Pattern {
    /// Cria um padrão inteligente:
    /// - Se `is_regex` for true, compila como regex case-insensitive.
    /// - Se contiver `*` ou `?`, compila como curinga (wildcard) case-insensitive.
    /// - Caso contrário, converte para minúsculas e usa `Substring`.
    pub fn parse(pattern: &str, is_regex: bool) -> Result<Self, regex::Error> {
        if is_regex {
            let re = regex::RegexBuilder::new(pattern)
                .case_insensitive(true)
                .build()?;
            return Ok(Pattern::Regex(re));
        }

        if pattern.contains('*') || pattern.contains('?') {
            let regex_str = wildcard_to_regex(pattern);
            let re = regex::RegexBuilder::new(&regex_str)
                .case_insensitive(true)
                .build()?;
            return Ok(Pattern::Regex(re));
        }

        Ok(Pattern::Substring(pattern.to_lowercase()))
    }
}

/// Converte um padrão com curingas (* e ?) em uma regex equivalente.
#[must_use]
pub fn wildcard_to_regex(pattern: &str) -> String {
    let mut regex_str = String::with_capacity(pattern.len() * 2 + 2);
    regex_str.push('^');
    for ch in pattern.chars() {
        match ch {
            '*' => regex_str.push_str(".*"),
            '?' => regex_str.push('.'),
            '.' | '+' | '(' | ')' | '[' | ']' | '{' | '}' | '^' | '$' | '|' | '\\' => {
                regex_str.push('\\');
                regex_str.push(ch);
            }
            _ => regex_str.push(ch),
        }
    }
    regex_str.push('$');
    regex_str
}

/// Filtros adicionais para restringir os resultados de busca.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Filters {
    /// Extensão de arquivo (ex.: "rs", "toml").
    pub ext: Option<String>,
    /// Tamanho mínimo em bytes.
    pub min_size: Option<u64>,
    /// Tamanho máximo em bytes.
    pub max_size: Option<u64>,
    /// Data de modificação mínima (Unix timestamp em segundos).
    pub min_mtime: Option<i64>,
    /// Data de modificação máxima (Unix timestamp em segundos).
    pub max_mtime: Option<i64>,
    /// ID do nó pai para busca restrita a uma pasta direta.
    pub parent_id: Option<u32>,
    /// Apenas diretórios.
    pub dirs_only: bool,
    /// Apenas arquivos normais.
    pub files_only: bool,
}

/// Especificação completa de uma consulta sobre o índice.
#[derive(Debug, Clone)]
pub struct Query {
    pub pattern: Pattern,
    pub filters: Filters,
}

impl Query {
    /// Cria uma nova consulta com o padrão informado e filtros padrão.
    pub fn new(pattern: Pattern) -> Self {
        Self {
            pattern,
            filters: Filters::default(),
        }
    }

    /// Cria uma consulta a partir de uma string de padrão e flag de regex opcional.
    pub fn parse(pattern: &str, is_regex: bool) -> Result<Self, regex::Error> {
        let pat = Pattern::parse(pattern, is_regex)?;
        Ok(Self::new(pat))
    }

    /// Anexa filtros à consulta.
    #[must_use]
    pub fn with_filters(mut self, filters: Filters) -> Self {
        self.filters = filters;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pattern_parse_substring() {
        let p = Pattern::parse("Main.RS", false).unwrap();
        match p {
            Pattern::Substring(s) => assert_eq!(s, "main.rs"),
            Pattern::Regex(_) => panic!("deveria ser Substring"),
        }
    }

    #[test]
    fn test_pattern_parse_wildcard() {
        let p = Pattern::parse("*.toml", false).unwrap();
        match p {
            Pattern::Regex(re) => {
                assert!(re.is_match("Cargo.toml"));
                assert!(re.is_match("config.TOML"));
                assert!(!re.is_match("toml.rs"));
            }
            Pattern::Substring(_) => panic!("deveria ser Regex para wildcard"),
        }
    }

    #[test]
    fn test_pattern_parse_regex() {
        let p = Pattern::parse(r"^lib\d+\.rs$", true).unwrap();
        match p {
            Pattern::Regex(re) => {
                assert!(re.is_match("lib123.rs"));
                assert!(!re.is_match("alib123.rs"));
            }
            Pattern::Substring(_) => panic!("deveria ser Regex"),
        }
    }
}
