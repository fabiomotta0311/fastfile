//! Convenções numéricas por localidade (subconjunto do CLDR usado pelo razão).

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Locale {
    pub tag: &'static str,
    /// Separador decimal.
    pub decimal: char,
    /// Separador de milhar; `None` não agrupa.
    pub group: Option<char>,
    /// Símbolo antes do número ("R$ 10,00") ou depois ("10,00 €").
    pub symbol_first: bool,
    /// Espaço entre símbolo e número.
    pub symbol_space: bool,
    /// Sinal negativo entre parênteses, como em relatórios contábeis.
    pub parens_for_negative: bool,
}

const fn loc(tag: &'static str, decimal: char, group: Option<char>, symbol_first: bool, symbol_space: bool) -> Locale {
    Locale { tag, decimal, group, symbol_first, symbol_space, parens_for_negative: false }
}

pub static LOCALES: &[Locale] = &[
    loc("en-US", '.', Some(','), true, false),
    loc("en-GB", '.', Some(','), true, false),
    loc("pt-BR", ',', Some('.'), true, true),
    loc("pt-PT", ',', Some(' '), false, true),
    loc("es-ES", ',', Some('.'), false, true),
    loc("es-AR", ',', Some('.'), true, true),
    loc("es-MX", '.', Some(','), true, false),
    loc("de-DE", ',', Some('.'), false, true),
    loc("de-CH", '.', Some('\''), true, true),
    loc("fr-FR", ',', Some(' '), false, true),
    loc("it-IT", ',', Some('.'), false, true),
    loc("ja-JP", '.', Some(','), true, false),
    loc("hi-IN", '.', Some(','), true, false),
    loc("C", '.', None, false, true),
];

impl Locale {
    /// Procura pela etiqueta BCP 47; aceita `_` no lugar de `-` e ignora maiúsculas.
    pub fn by_tag(tag: &str) -> Option<Locale> {
        let t = tag.trim().replace('_', "-");
        LOCALES.iter().copied().find(|l| l.tag.eq_ignore_ascii_case(&t))
    }

    /// Como `by_tag`, mas cai na língua (`pt-AO` -> primeira `pt-*`) e por fim em `C`.
    pub fn resolve(tag: &str) -> Locale {
        if let Some(l) = Locale::by_tag(tag) {
            return l;
        }
        let lang = tag.trim().split(['-', '_']).next().unwrap_or("").to_ascii_lowercase();
        LOCALES
            .iter()
            .copied()
            .find(|l| l.tag.split('-').next() == Some(lang.as_str()))
            .unwrap_or_else(Locale::c)
    }

    pub fn c() -> Locale {
        *LOCALES.last().expect("tabela não vazia")
    }

    pub fn with_parens(mut self) -> Locale {
        self.parens_for_negative = true;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lookup_variants() {
        assert_eq!(Locale::by_tag("en_us").unwrap().tag, "en-US");
        assert_eq!(Locale::by_tag("DE-de").unwrap().decimal, ',');
        assert!(Locale::by_tag("xx-YY").is_none());
    }

    #[test]
    fn resolve_falls_back() {
        assert_eq!(Locale::resolve("de-AT").tag, "de-DE");
        assert_eq!(Locale::resolve("zz").tag, "C");
        assert_eq!(Locale::resolve("fr-FR").tag, "fr-FR");
    }

    #[test]
    fn tags_are_unique() {
        let mut seen = std::collections::HashSet::new();
        for l in LOCALES {
            assert!(seen.insert(l.tag));
            assert_ne!(Some(l.decimal), l.group, "{}", l.tag);
        }
    }
}
