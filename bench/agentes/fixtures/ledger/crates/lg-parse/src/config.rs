//! Arquivo de configuração `ledger.conf` (estilo INI).
//!
//! ```text
//! # comentário
//! [report]
//! currency = BRL
//! locale = pt-BR
//! fiscal_start = 4
//! ```
//!
//! As chaves ficam disponíveis como `secao.chave`. Chaves fora de seção ficam sem prefixo.
//! Uma chave repetida vale pela última ocorrência.

use std::collections::BTreeMap;

use lg_core::{Error, Result};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Config {
    values: BTreeMap<String, String>,
}

impl Config {
    pub fn parse(text: &str) -> Result<Config> {
        let mut values = BTreeMap::new();
        let mut section = String::new();
        for (i, raw) in text.lines().enumerate() {
            let line = raw.trim();
            if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
                continue;
            }
            if let Some(rest) = line.strip_prefix('[') {
                let name = rest
                    .strip_suffix(']')
                    .ok_or_else(|| Error::Config(format!("linha {}: seção sem ']'", i + 1)))?;
                section = name.trim().to_lowercase();
                continue;
            }
            let (k, v) = line
                .split_once('=')
                .ok_or_else(|| Error::Config(format!("linha {}: esperado chave = valor", i + 1)))?;
            let key = k.trim().to_lowercase();
            if key.is_empty() {
                return Err(Error::Config(format!("linha {}: chave vazia", i + 1)));
            }
            let value = strip_inline_comment(v.trim());
            let full = if section.is_empty() { key } else { format!("{section}.{key}") };
            values.insert(full, value.to_string());
        }
        Ok(Config { values })
    }

    pub fn get(&self, key: &str) -> Option<&str> {
        self.values.get(key).map(|s| s.as_str())
    }

    pub fn get_or<'a>(&'a self, key: &str, default: &'a str) -> &'a str {
        self.get(key).unwrap_or(default)
    }

    pub fn get_u8(&self, key: &str) -> Result<Option<u8>> {
        match self.get(key) {
            None => Ok(None),
            Some(v) => v
                .parse()
                .map(Some)
                .map_err(|_| Error::Config(format!("{key}: número inválido '{v}'"))),
        }
    }

    pub fn get_bool(&self, key: &str) -> Result<Option<bool>> {
        match self.get(key).map(|v| v.to_lowercase()) {
            None => Ok(None),
            Some(v) if ["1", "true", "sim", "yes"].contains(&v.as_str()) => Ok(Some(true)),
            Some(v) if ["0", "false", "nao", "não", "no"].contains(&v.as_str()) => Ok(Some(false)),
            Some(v) => Err(Error::Config(format!("{key}: booleano inválido '{v}'"))),
        }
    }

    pub fn set(&mut self, key: &str, value: &str) {
        self.values.insert(key.to_lowercase(), value.to_string());
    }

    pub fn keys(&self) -> impl Iterator<Item = &str> {
        self.values.keys().map(|k| k.as_str())
    }
}

/// Remove `# comentário` no fim do valor, respeitando valores entre aspas.
fn strip_inline_comment(v: &str) -> &str {
    if let Some(inner) = v.strip_prefix('"').and_then(|r| r.split_once('"')) {
        return inner.0;
    }
    match v.find(" #") {
        Some(i) => v[..i].trim_end(),
        None => v,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sections_and_keys() {
        let c = Config::parse("top = 1\n[Report]\nCurrency = BRL # padrão\nlocale=pt-BR\n\n[rules]\nfile = \"regras #1.txt\"\n").unwrap();
        assert_eq!(c.get("top"), Some("1"));
        assert_eq!(c.get("report.currency"), Some("BRL"));
        assert_eq!(c.get("report.locale"), Some("pt-BR"));
        assert_eq!(c.get("rules.file"), Some("regras #1.txt"));
        assert_eq!(c.get("report.missing"), None);
    }

    #[test]
    fn last_value_wins() {
        let c = Config::parse("[a]\nx=1\nx=2\n").unwrap();
        assert_eq!(c.get("a.x"), Some("2"));
    }

    #[test]
    fn typed_getters() {
        let c = Config::parse("[r]\nn=4\nb=sim\nbad=x\n").unwrap();
        assert_eq!(c.get_u8("r.n").unwrap(), Some(4));
        assert_eq!(c.get_bool("r.b").unwrap(), Some(true));
        assert!(c.get_bool("r.bad").is_err());
        assert!(c.get_u8("r.bad").is_err());
        assert_eq!(c.get_u8("r.none").unwrap(), None);
    }

    #[test]
    fn errors() {
        assert!(Config::parse("[sem fim\n").is_err());
        assert!(Config::parse("so texto\n").is_err());
        assert!(Config::parse(" = 3\n").is_err());
    }
}
