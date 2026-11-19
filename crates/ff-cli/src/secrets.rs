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

/// Regras de valores com cara de segredo dentro do conteúdo (contrato `SECRETS-CONTENT-001`).
/// Cada regra tem um nome curto e o grupo de captura a substituir (0 = casamento inteiro).
fn content_rules() -> &'static [(&'static str, regex::Regex, usize)] {
    static RULES: std::sync::OnceLock<Vec<(&'static str, regex::Regex, usize)>> =
        std::sync::OnceLock::new();
    RULES.get_or_init(|| {
        let rule = |name: &'static str, re: &str, group: usize| {
            (name, regex::Regex::new(re).expect("regra de segredo válida"), group)
        };
        vec![
            rule("private-key", r"-----BEGIN [A-Z0-9 ]*PRIVATE KEY( BLOCK)?-----", 0),
            rule(
                "github-token",
                r"\b(?:gh[pousr]_[A-Za-z0-9]{36,}|github_pat_[A-Za-z0-9_]{22,})",
                0,
            ),
            rule(
                "aws-access-key",
                r"\b(?:AKIA|ASIA|AGPA|AIDA|AROA)[0-9A-Z]{16}\b",
                0,
            ),
            rule("slack-token", r"\bxox[abprs]-[A-Za-z0-9-]{10,}", 0),
            rule("api-key", r"\bsk-(?:ant-|proj-)?[A-Za-z0-9_-]{20,}", 0),
            rule("google-api-key", r"\bAIza[0-9A-Za-z_-]{35}", 0),
            rule(
                "stripe-key",
                r"\b(?:sk|rk|pk)_(?:live|test)_[0-9A-Za-z]{16,}",
                0,
            ),
            rule(
                "jwt",
                r"\beyJ[A-Za-z0-9_-]{8,}\.eyJ[A-Za-z0-9_-]{8,}\.[A-Za-z0-9_-]{8,}",
                0,
            ),
            rule(
                "connection-string",
                r"(?i)\b[a-z][a-z0-9+.-]*://[^\s:/@]+:([^\s@/]{3,})@",
                1,
            ),
            rule(
                "assignment",
                r#"(?i)\b(?:password|passwd|pwd|secret|api[_-]?key|access[_-]?key|access[_-]?token|auth[_-]?token|client[_-]?secret|private[_-]?key|token)\b["']?\s*[:=]\s*["']?([A-Za-z0-9_\-+/=!@#$%^&*~]{8,})"#,
                1,
            ),
        ]
    })
}

/// Mascara trechos com cara de token, chave ou senha em `text`.
/// Devolve o texto mascarado e quantos trechos foram substituídos.
#[must_use]
pub fn mask_secrets(text: &str) -> (String, usize) {
    let mut out = text.to_string();
    let mut count = 0usize;
    for (name, re, group) in content_rules() {
        if !re.is_match(&out) {
            continue;
        }
        let replacement = format!("«redacted:{name}»");
        let mut result = String::with_capacity(out.len());
        let mut last = 0;
        for caps in re.captures_iter(&out) {
            let Some(m) = caps.get(*group) else { continue };
            if m.as_str().starts_with('«') {
                continue;
            }
            result.push_str(&out[last..m.start()]);
            result.push_str(&replacement);
            last = m.end();
            count += 1;
        }
        result.push_str(&out[last..]);
        out = result;
    }
    (out, count)
}

/// Mascara as linhas devolvidas por uma busca em conteúdo. Devolve o total de trechos mascarados.
pub fn mask_grep_report(report: &mut ff_content::GrepReport) -> usize {
    let mut total = 0;
    if let Some(ref mut matches) = report.matches {
        for m in matches.iter_mut() {
            let (masked, n) = mask_secrets(&m.line_text);
            if n > 0 {
                m.line_text = masked;
                total += n;
            }
        }
    }
    total
}

/// Mascara as linhas de um diff estruturado. Devolve o total de trechos mascarados.
pub fn mask_diff_report(report: &mut ff_diff::DiffReport) -> usize {
    let mut total = 0;
    for hunk in &mut report.hunks {
        for line in &mut hunk.lines {
            let (masked, n) = mask_secrets(&line.text);
            if n > 0 {
                line.text = masked;
                // Os trechos intra-linha poderiam remontar o valor; descartamos o realce.
                line.spans.clear();
                total += n;
            }
        }
    }
    total
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

    #[test]
    fn content_masking_hides_token_values() {
        let cases = [
            ("token = \"ghp_abcdefghijklmnopqrstuvwxyz0123456789\"", "ghp_abcdef"),
            ("aws = \"AKIAIOSFODNN7EXAMPLE\"", "AKIAIOSFODNN7EXAMPLE"),
            ("password: hunter2secret", "hunter2secret"),
            ("API_KEY=sk-ant-REDACTEDREDACTEDREDACTED12", "REDACTEDREDACTED"),
            ("url = postgres://admin:s3cr3tP4ss@db:5432/app", "s3cr3tP4ss"),
            ("-----BEGIN RSA PRIVATE KEY-----", "BEGIN RSA PRIVATE KEY"),
            (
                "auth eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxMjM0NTY3ODkwIn0.dozjgNryP4J3jVmNHl0w5N_XgL0n3I9PlFUP0THsR8U",
                "eyJhbGciOiJIUzI1NiJ9",
            ),
        ];
        for (input, secret) in cases {
            let (out, n) = mask_secrets(input);
            assert!(n >= 1, "não mascarou: {input}");
            assert!(!out.contains(secret), "vazou em: {out}");
            assert!(out.contains("«redacted:"));
        }
    }

    #[test]
    fn content_masking_keeps_ordinary_code() {
        for input in [
            "let password = args.password;",
            "fn check_password(pwd: &str) -> bool {",
            "token: Option<String>,",
            "secret_hidden += 1;",
            "pub fn mask_secrets(text: &str) -> (String, usize) {",
        ] {
            let (out, n) = mask_secrets(input);
            assert_eq!(n, 0, "falso positivo: {input} -> {out}");
            assert_eq!(out, input);
        }
    }
}
