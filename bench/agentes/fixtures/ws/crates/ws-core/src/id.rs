/// Normaliza um identificador: sem espaços nas pontas e em minúsculas.
pub fn normalize(s: &str) -> String {
    s.trim().to_lowercase()
}

#[cfg(test)]
mod tests {
    use super::normalize;

    #[test]
    fn remove_espacos_das_pontas() {
        assert_eq!(normalize("  abc  "), "abc");
    }
}
