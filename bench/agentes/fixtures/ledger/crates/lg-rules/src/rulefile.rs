//! Arquivo de regras do usuário (`regras.txt`).
//!
//! Uma regra por linha: `PADRAO => categoria`, opcionalmente prefixada por prioridade
//! entre colchetes: `[10] *IFOOD* => alimentacao:delivery`. Linhas vazias e `#` são
//! ignoradas.

use lg_core::{Error, Result};

use crate::engine::Rule;

pub fn parse_rules(text: &str) -> Result<Vec<Rule>> {
    let mut out = Vec::new();
    for (i, raw) in text.lines().enumerate() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let err = |m: &str| Error::Parse { line: i + 1, message: m.to_string() };
        let (priority, rest) = match line.strip_prefix('[') {
            Some(r) => {
                let (n, rest) = r.split_once(']').ok_or_else(|| err("prioridade sem ']'"))?;
                let p: i32 = n.trim().parse().map_err(|_| err("prioridade inválida"))?;
                (p, rest.trim_start())
            }
            None => (0, line),
        };
        let (pat, cat) = rest.split_once("=>").ok_or_else(|| err("esperado 'PADRAO => categoria'"))?;
        let (pat, cat) = (pat.trim(), cat.trim());
        if pat.is_empty() || cat.is_empty() {
            return Err(err("padrão e categoria são obrigatórios"));
        }
        if cat.contains(char::is_whitespace) {
            return Err(err("categoria não pode ter espaços"));
        }
        out.push(Rule::new(pat, &cat.to_lowercase()).with_priority(priority));
    }
    Ok(out)
}

/// Inverso de [`parse_rules`], para `ledger rules export`.
pub fn format_rules(rules: &[Rule]) -> String {
    let mut s = String::new();
    for r in rules {
        if r.priority != 0 {
            s.push_str(&format!("[{}] ", r.priority));
        }
        s.push_str(r.pattern.source());
        s.push_str(" => ");
        s.push_str(&r.category);
        s.push('\n');
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_lines() {
        let rules = parse_rules("# minhas regras\n[10] *IFOOD* => Alimentacao:Delivery\nNETFLIX => assinaturas\n\n").unwrap();
        assert_eq!(rules.len(), 2);
        assert_eq!(rules[0].priority, 10);
        assert_eq!(rules[0].category, "alimentacao:delivery");
        assert_eq!(rules[1].priority, 0);
    }

    #[test]
    fn roundtrip() {
        let text = "[-2] POSTO* => transporte\nSPOTIFY => assinaturas\n";
        assert_eq!(format_rules(&parse_rules(text).unwrap()), text);
    }

    #[test]
    fn errors_carry_line_numbers() {
        let e = parse_rules("A => b\n[x] C => d\n").unwrap_err();
        assert!(matches!(e, Error::Parse { line: 2, .. }));
        assert!(parse_rules("SEM SETA\n").is_err());
        assert!(parse_rules("A => duas palavras\n").is_err());
        assert!(parse_rules(" => b\n").is_err());
    }
}
