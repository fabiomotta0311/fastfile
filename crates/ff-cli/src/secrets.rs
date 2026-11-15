//! Proteção e mascaramento de segredos por padrão (§3.2).

/// Verifica se um arquivo ou diretório é considerado potencialmente sensível (segredo).
#[must_use]
pub fn is_secret_file(name: &str) -> bool {
    let lower = name.to_lowercase();

    // Arquivos de ambiente
    if lower == ".env" || lower.starts_with(".env.") {
        return true;
    }

    // Chaves SSH conhecidas
    if matches!(
        lower.as_str(),
        "id_rsa"
            | "id_rsa.pub"
            | "id_ed25519"
            | "id_ed25519.pub"
            | "id_dsa"
            | "id_ecdsa"
            | ".git-credentials"
    ) {
        return true;
    }

    // Extensões de certificados, chaves criptográficas e tokens
    if lower.ends_with(".pem")
        || lower.ends_with(".key")
        || lower.ends_with(".pkcs12")
        || lower.ends_with(".pfx")
        || lower.ends_with(".p12")
        || lower.ends_with(".jwt")
        || lower.ends_with(".har")
        || lower.ends_with(".kdbx")
    {
        return true;
    }

    // Arquivos de credenciais específicos
    if lower == "credentials.json"
        || lower == "token.json"
        || (lower.starts_with("service-account") && lower.ends_with(".json"))
    {
        return true;
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_secret_detection() {
        assert!(is_secret_file(".env"));
        assert!(is_secret_file(".env.local"));
        assert!(is_secret_file(".env.production.local"));
        assert!(is_secret_file("id_rsa"));
        assert!(is_secret_file("id_ed25519"));
        assert!(is_secret_file("server.key"));
        assert!(is_secret_file("cert.pem"));
        assert!(is_secret_file("session.jwt"));
        assert!(is_secret_file("network.har"));
        assert!(is_secret_file("credentials.json"));
        assert!(is_secret_file("passwords.kdbx"));

        assert!(!is_secret_file("main.rs"));
        assert!(!is_secret_file("Cargo.toml"));
        assert!(!is_secret_file("README.md"));
        assert!(!is_secret_file("app.env.ts"));
    }
}
