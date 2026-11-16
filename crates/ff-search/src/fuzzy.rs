//! Algoritmo de correspondência aproximada (fuzzy search) de alta performance (§3.5 e #27).

/// Avalia a similaridade entre uma consulta `query` e um nome alvo `target`.
/// Retorna `Some(score)` se a correspondência atender ao limiar mínimo de similaridade.
///
/// Scores:
/// - Correspondência exata: 10.000+
/// - Prefixo exato: 8.000+
/// - Substring exata: 6.000+
/// - Subsequência completa: 3.000 a 5.900
/// - Correspondência aproximada / typo-tolerante: 1.000 a 2.900
#[must_use]
pub fn fuzzy_score(query: &str, target: &str) -> Option<u32> {
    if query.is_empty() || target.is_empty() {
        return None;
    }

    let q_bytes = query.as_bytes();
    let t_bytes = target.as_bytes();

    let q_len = q_bytes.len();
    let t_len = t_bytes.len();

    // Rejeição rápida: alvo muito curto para conter consulta aproximada
    if t_len < q_len / 2 {
        return None;
    }

    // 1. Correspondência exata
    if q_bytes == t_bytes {
        return Some(10_000);
    }

    // 2. Prefixo exato
    if target.starts_with(query) {
        let coverage = (q_len * 1_000) / t_len;
        return Some(8_000 + coverage as u32);
    }

    // 3. Substring exata
    if target.contains(query) {
        let coverage = (q_len * 1_000) / t_len;
        return Some(6_000 + coverage as u32);
    }

    // 4. Verificação de subsequência completa
    if let Some(subseq_score) = score_subsequence(q_bytes, t_bytes) {
        return Some(subseq_score);
    }

    // 5. Correspondência aproximada / tolerante a erros e abreviações (ex.: archmd -> ARQUITETURA.md)
    score_approximate(q_bytes, t_bytes)
}

/// Pontua uma correspondência onde todos os caracteres da query aparecem na ordem no target.
fn score_subsequence(q: &[u8], t: &[u8]) -> Option<u32> {
    let mut q_idx = 0;
    let mut t_idx = 0;

    let mut consecutive = 0u32;
    let mut boundary_matches = 0u32;
    let mut total_distance = 0u32;
    let mut last_t_match = 0usize;

    while q_idx < q.len() && t_idx < t.len() {
        if q[q_idx] == t[t_idx] {
            if q_idx > 0 {
                let dist = (t_idx - last_t_match) as u32;
                total_distance += dist;
                if dist == 1 {
                    consecutive += 1;
                }
            }

            // Bônus de limite de palavra (após '.', '_', '-', '/', ou início)
            if t_idx == 0 || is_word_boundary(t[t_idx - 1]) {
                boundary_matches += 1;
            }

            last_t_match = t_idx;
            q_idx += 1;
        }
        t_idx += 1;
    }

    // Se todos os caracteres da query foram encontrados em sequência
    if q_idx == q.len() {
        let base = 3_000u32;
        let consecutive_bonus = consecutive * 60;
        let boundary_bonus = boundary_matches * 100;
        let distance_penalty = total_distance.min(1_000) * 2;

        let score = base + consecutive_bonus + boundary_bonus;
        Some(score.saturating_sub(distance_penalty).max(1_000))
    } else {
        None
    }
}

/// Pontua alinhamento aproximado (LCS / Smith-Waterman simplificado para nomes de arquivo).
fn score_approximate(q: &[u8], t: &[u8]) -> Option<u32> {
    let q_len = q.len();
    let t_len = t.len();

    // Se query for muito longa para DP em stack, limita a 64 bytes
    if q_len > 64 || t_len > 256 {
        return None;
    }

    // Longest Common Subsequence usando buffer estático em stack (sem alocação na heap)
    let mut dp = [0u16; 65 * 257];
    let cols = t_len + 1;

    for i in 1..=q_len {
        let q_char = q[i - 1];
        let row_curr = i * cols;
        let row_prev = (i - 1) * cols;

        for j in 1..=t_len {
            if q_char == t[j - 1] {
                dp[row_curr + j] = dp[row_prev + (j - 1)] + 1;
            } else {
                let top = dp[row_prev + j];
                let left = dp[row_curr + (j - 1)];
                dp[row_curr + j] = if top > left { top } else { left };
            }
        }
    }

    let lcs = dp[q_len * cols + t_len] as usize;

    // Critério mínimo para correspondência aproximada:
    // Pelo menos 60% dos caracteres da query preservados em ordem
    let min_ratio = if q_len <= 4 { 0.75 } else { 0.60 };
    let ratio = (lcs as f64) / (q_len as f64);
    if ratio < min_ratio || lcs < 3 {
        return None;
    }

    let mut score = (ratio * 2_000.0) as u32;

    // Bônus se primeiro caractere coincide
    if q[0] == t[0] {
        score += 300;
    }

    // Bônus se última extensão coincide
    if let (Some(q_ext), Some(t_ext)) = (
        q.split(|&b| b == b'.').next_back(),
        t.split(|&b| b == b'.').next_back(),
    ) {
        if !q_ext.is_empty() && q_ext == t_ext {
            score += 400;
        }
    }

    Some(score)
}

#[inline]
fn is_word_boundary(b: u8) -> bool {
    b == b'.' || b == b'_' || b == b'-' || b == b'/' || b == b'\\'
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_exact_and_prefix() {
        assert_eq!(fuzzy_score("cargo.toml", "cargo.toml"), Some(10_000));
        assert!(fuzzy_score("cargo", "cargo.toml").unwrap() >= 8_000);
        assert!(fuzzy_score("toml", "cargo.toml").unwrap() >= 6_000);
    }

    #[test]
    fn test_subsequence_match() {
        let s = fuzzy_score("ctoml", "cargo.toml");
        assert!(s.is_some());
        assert!(s.unwrap() >= 3_000);
    }

    #[test]
    fn test_archmd_matches_arquitetura_md() {
        // Caso específico citado na Issue #27
        let score_arch = fuzzy_score("archmd", "arquitetura.md");
        assert!(
            score_arch.is_some(),
            "archmd deve corresponder a arquitetura.md"
        );

        let score_irrelevant = fuzzy_score("archmd", "cargo.toml");
        assert!(
            score_arch.unwrap() > score_irrelevant.unwrap_or(0),
            "arquitetura.md deve ter pontuação superior a arquivos irrelevantes"
        );
    }

    #[test]
    fn test_no_match_for_completely_different() {
        assert_eq!(fuzzy_score("xyz123", "cargo.toml"), None);
    }
}
