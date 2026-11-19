//! Contas hierárquicas separadas por dois-pontos: `ativo:banco:corrente`.

use std::fmt;

use crate::error::{Error, Result};

pub const SEPARATOR: char = ':';
pub const ROOTS: [&str; 5] = ["ativo", "passivo", "receita", "despesa", "patrimonio"];

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct AccountId(String);

impl AccountId {
    /// Normaliza para minúsculas e valida a raiz e os segmentos.
    pub fn parse(s: &str) -> Result<AccountId> {
        let norm = s.trim().to_lowercase();
        if norm.is_empty() {
            return Err(Error::InvalidAccount(s.to_string()));
        }
        let mut segments = norm.split(SEPARATOR);
        let root = segments.next().unwrap_or_default();
        if !ROOTS.contains(&root) {
            return Err(Error::InvalidAccount(format!("{s} (raiz deve ser uma de {})", ROOTS.join(", "))));
        }
        for seg in norm.split(SEPARATOR) {
            if seg.is_empty() || seg.chars().any(|c| c.is_whitespace()) {
                return Err(Error::InvalidAccount(s.to_string()));
            }
        }
        Ok(AccountId(norm))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn segments(&self) -> impl Iterator<Item = &str> {
        self.0.split(SEPARATOR)
    }

    pub fn depth(&self) -> usize {
        self.segments().count()
    }

    pub fn root(&self) -> &str {
        self.segments().next().unwrap_or_default()
    }

    pub fn parent(&self) -> Option<AccountId> {
        self.0.rfind(SEPARATOR).map(|i| AccountId(self.0[..i].to_string()))
    }

    /// Prefixo truncado em `depth` segmentos (a própria conta se já for mais rasa).
    pub fn truncate(&self, depth: usize) -> AccountId {
        if depth == 0 || self.depth() <= depth {
            return self.clone();
        }
        AccountId(self.segments().take(depth).collect::<Vec<_>>().join(":"))
    }

    /// `ativo:banco` é ancestral de `ativo:banco:corrente`, mas não de `ativo:bancos`.
    pub fn is_ancestor_of(&self, other: &AccountId) -> bool {
        other.0.len() > self.0.len()
            && other.0.starts_with(&self.0)
            && other.0[self.0.len()..].starts_with(SEPARATOR)
    }

    pub fn is_self_or_descendant_of(&self, other: &AccountId) -> bool {
        self == other || other.is_ancestor_of(self)
    }

    /// Todos os ancestrais, do mais raso para o mais profundo, sem incluir a própria conta.
    pub fn ancestors(&self) -> Vec<AccountId> {
        let mut out = Vec::new();
        let mut cur = self.parent();
        while let Some(p) = cur {
            cur = p.parent();
            out.push(p);
        }
        out.reverse();
        out
    }
}

impl fmt::Display for AccountId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn a(s: &str) -> AccountId {
        AccountId::parse(s).unwrap()
    }

    #[test]
    fn parse_normalizes_and_validates() {
        assert_eq!(a("Ativo:Banco:Corrente").as_str(), "ativo:banco:corrente");
        assert!(AccountId::parse("caixa:loja").is_err());
        assert!(AccountId::parse("ativo::x").is_err());
        assert!(AccountId::parse("ativo:conta corrente").is_err());
        assert!(AccountId::parse("").is_err());
    }

    #[test]
    fn hierarchy() {
        let c = a("ativo:banco:corrente");
        assert_eq!(c.depth(), 3);
        assert_eq!(c.root(), "ativo");
        assert_eq!(c.parent(), Some(a("ativo:banco")));
        assert_eq!(a("ativo").parent(), None);
        assert_eq!(c.truncate(2), a("ativo:banco"));
        assert_eq!(c.truncate(5), c);
        assert_eq!(c.ancestors(), vec![a("ativo"), a("ativo:banco")]);
    }

    #[test]
    fn ancestry_respects_segments() {
        assert!(a("ativo:banco").is_ancestor_of(&a("ativo:banco:corrente")));
        assert!(!a("ativo:banco").is_ancestor_of(&a("ativo:bancos")));
        assert!(!a("ativo:banco").is_ancestor_of(&a("ativo:banco")));
        assert!(a("ativo:banco").is_self_or_descendant_of(&a("ativo")));
    }
}
