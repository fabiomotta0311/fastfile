//! Padrões de descrição com curinga `*` (zero ou mais caracteres) e `?` (exatamente um).
//!
//! A comparação ignora maiúsculas/minúsculas ASCII e espaços repetidos: os bancos
//! costumam completar a descrição com espaços até uma largura fixa.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Token {
    Lit(char),
    Any,
    Star,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pattern {
    source: String,
    tokens: Vec<Token>,
}

/// Maiúsculas ASCII e espaços colapsados, sem espaços nas pontas.
pub fn normalize(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut last_space = true;
    for c in s.chars() {
        if c.is_whitespace() {
            if !last_space {
                out.push(' ');
                last_space = true;
            }
        } else {
            out.push(c.to_ascii_uppercase());
            last_space = false;
        }
    }
    if out.ends_with(' ') {
        out.pop();
    }
    out
}

impl Pattern {
    pub fn parse(source: &str) -> Pattern {
        let norm = normalize(source);
        let mut tokens = Vec::with_capacity(norm.len());
        for c in norm.chars() {
            let t = match c {
                '*' => Token::Star,
                '?' => Token::Any,
                c => Token::Lit(c),
            };
            // `**` equivale a `*`.
            if t == Token::Star && tokens.last() == Some(&Token::Star) {
                continue;
            }
            tokens.push(t);
        }
        Pattern { source: source.to_string(), tokens }
    }

    pub fn source(&self) -> &str {
        &self.source
    }

    /// Quantidade de caracteres literais: desempata regras de mesma prioridade.
    pub fn specificity(&self) -> usize {
        self.tokens.iter().filter(|t| matches!(t, Token::Lit(_))).count()
    }

    pub fn has_wildcards(&self) -> bool {
        self.tokens.iter().any(|t| !matches!(t, Token::Lit(_)))
    }

    pub fn matches(&self, text: &str) -> bool {
        let text: Vec<char> = normalize(text).chars().collect();
        let pat = &self.tokens;
        let (mut p, mut t) = (0usize, 0usize);
        let mut star: Option<(usize, usize)> = None;
        while t < text.len() {
            match pat.get(p) {
                Some(Token::Lit(c)) if *c == text[t] => {
                    p += 1;
                    t += 1;
                }
                Some(Token::Any) => {
                    p += 1;
                    t += 1;
                }
                Some(Token::Star) => {
                    star = Some((p, t));
                    p += 1;
                }
                _ => match star {
                    Some((sp, st)) => {
                        p = sp + 1;
                        t = st + 1;
                        star = Some((sp, st + 1));
                    }
                    None => return false,
                },
            }
        }
        while pat.get(p) == Some(&Token::Star) {
            p += 1;
        }
        p == pat.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn literal_is_exact_and_case_insensitive() {
        let p = Pattern::parse("Netflix");
        assert!(p.matches("NETFLIX"));
        assert!(p.matches("  netflix "));
        assert!(!p.matches("NETFLIX.COM"));
    }

    #[test]
    fn star_in_the_middle_and_end() {
        assert!(Pattern::parse("UBER*").matches("UBER TRIP 1234"));
        assert!(Pattern::parse("POSTO*SHELL").matches("POSTO 21 SHELL"));
        assert!(Pattern::parse("*FARMACIA*").matches("DROGARIA E FARMACIA SAO JOAO"));
        assert!(!Pattern::parse("POSTO*SHELL").matches("POSTO 21 IPIRANGA"));
    }

    #[test]
    fn question_mark_is_one_char() {
        assert!(Pattern::parse("LOJA ?").matches("LOJA 7"));
        assert!(!Pattern::parse("LOJA ?").matches("LOJA 77"));
    }

    #[test]
    fn spaces_are_collapsed() {
        assert!(Pattern::parse("PIX  RECEBIDO*").matches("PIX RECEBIDO   FULANO"));
        assert_eq!(normalize("  a   b  "), "A B");
    }

    #[test]
    fn specificity_counts_literals() {
        assert_eq!(Pattern::parse("MERCADO*").specificity(), 7);
        assert_eq!(Pattern::parse("**A?B*").specificity(), 2);
        assert!(Pattern::parse("A*").has_wildcards());
        assert!(!Pattern::parse("AB").has_wildcards());
    }
}
