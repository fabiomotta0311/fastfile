/// Colapsa qualquer sequência de espaços em branco em um único espaço.
pub fn normalize(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub fn count_words(s: &str) -> usize {
    s.split_whitespace().count()
}

/// Mantém no máximo `max_chars` caracteres (não bytes).
pub fn truncate(s: &str, max_chars: usize) -> String {
    s.chars().take(max_chars).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn conta_palavras_simples() {
        assert_eq!(count_words("a b c"), 3);
        assert_eq!(truncate("abcdef", 3), "abc");
        assert_eq!(normalize("a b"), "a b");
    }
}
