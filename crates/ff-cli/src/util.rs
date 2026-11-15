//! Funções utilitárias de formatação, conversão de tamanhos e durações.

use std::path::Path;
use std::time::Duration;

/// Analisa especificações de tamanho como ">1GB", "<=10MB", ">=500KB", "<1M", "100B".
pub fn parse_size_filter(spec: &str) -> Result<(Option<u64>, Option<u64>), String> {
    let s = spec.trim();
    if s.is_empty() {
        return Err("especificação de tamanho vazia".into());
    }

    let (op, num_part) = if let Some(stripped) = s.strip_prefix(">=") {
        (">=", stripped)
    } else if let Some(stripped) = s.strip_prefix("<=") {
        ("<=", stripped)
    } else if let Some(stripped) = s.strip_prefix('>') {
        (">", stripped)
    } else if let Some(stripped) = s.strip_prefix('<') {
        ("<", stripped)
    } else if let Some(stripped) = s.strip_prefix('=') {
        ("=", stripped)
    } else {
        ("=", s)
    };

    let bytes = parse_bytes_value(num_part.trim())?;

    match op {
        ">" => Ok((Some(bytes.saturating_add(1)), None)),
        ">=" => Ok((Some(bytes), None)),
        "<" => Ok((None, Some(bytes.saturating_sub(1)))),
        "<=" => Ok((None, Some(bytes))),
        "=" => Ok((Some(bytes), Some(bytes))),
        _ => Err(format!("operador desconhecido: '{op}'")),
    }
}

/// Converte strings como "10MB", "1GB", "500K", "1024" para quantidade de bytes.
pub fn parse_bytes_value(s: &str) -> Result<u64, String> {
    let s = s.trim().to_uppercase();
    if s.is_empty() {
        return Err("valor de bytes vazio".into());
    }

    let (num_str, multiplier) = if s.ends_with("TIB") || s.ends_with("TB") {
        (
            s.trim_end_matches("TIB").trim_end_matches("TB"),
            1024u64 * 1024 * 1024 * 1024,
        )
    } else if s.ends_with('T') {
        (&s[..s.len() - 1], 1024u64 * 1024 * 1024 * 1024)
    } else if s.ends_with("GIB") || s.ends_with("GB") {
        (
            s.trim_end_matches("GIB").trim_end_matches("GB"),
            1024u64 * 1024 * 1024,
        )
    } else if s.ends_with('G') {
        (&s[..s.len() - 1], 1024u64 * 1024 * 1024)
    } else if s.ends_with("MIB") || s.ends_with("MB") {
        (
            s.trim_end_matches("MIB").trim_end_matches("MB"),
            1024u64 * 1024,
        )
    } else if s.ends_with('M') {
        (&s[..s.len() - 1], 1024u64 * 1024)
    } else if s.ends_with("KIB") || s.ends_with("KB") {
        (s.trim_end_matches("KIB").trim_end_matches("KB"), 1024u64)
    } else if s.ends_with('K') {
        (&s[..s.len() - 1], 1024u64)
    } else if s.ends_with('B') {
        (&s[..s.len() - 1], 1u64)
    } else {
        (s.as_str(), 1u64)
    };

    let num: f64 = num_str
        .trim()
        .parse()
        .map_err(|_| format!("número inválido '{num_str}'"))?;

    if num < 0.0 {
        return Err("tamanho não pode ser negativo".into());
    }

    Ok((num * multiplier as f64) as u64)
}

/// Analisa strings de duração como "30s", "15m", "1h", "2d", "1w".
pub fn parse_duration(spec: &str) -> Result<Duration, String> {
    let s = spec.trim().to_lowercase();
    if s.is_empty() {
        return Err("duração vazia".into());
    }

    let (num_str, unit_secs) = if let Some(n) = s.strip_suffix("ms") {
        let millis: u64 = n
            .trim()
            .parse()
            .map_err(|_| format!("número inválido '{n}'"))?;
        return Ok(Duration::from_millis(millis));
    } else if let Some(n) = s.strip_suffix('s') {
        (n, 1u64)
    } else if let Some(n) = s.strip_suffix('m') {
        (n, 60u64)
    } else if let Some(n) = s.strip_suffix('h') {
        (n, 3600u64)
    } else if let Some(n) = s.strip_suffix('d') {
        (n, 86400u64)
    } else if let Some(n) = s.strip_suffix('w') {
        (n, 7 * 86400u64)
    } else {
        (s.as_str(), 1u64)
    };

    let num: u64 = num_str
        .trim()
        .parse()
        .map_err(|_| format!("número de duração inválido '{num_str}'"))?;

    Ok(Duration::from_secs(num.saturating_mul(unit_secs)))
}

/// Formata bytes em formato legível humano (B, KB, MB, GB, TB).
#[must_use]
pub fn format_bytes(bytes: u64) -> String {
    const KIB: u64 = 1024;
    const MIB: u64 = 1024 * KIB;
    const GIB: u64 = 1024 * MIB;
    const TIB: u64 = 1024 * GIB;

    if bytes >= TIB {
        format!("{:.2} TB", bytes as f64 / TIB as f64)
    } else if bytes >= GIB {
        format!("{:.2} GB", bytes as f64 / GIB as f64)
    } else if bytes >= MIB {
        format!("{:.2} MB", bytes as f64 / MIB as f64)
    } else if bytes >= KIB {
        format!("{:.1} KB", bytes as f64 / KIB as f64)
    } else {
        format!("{bytes} B")
    }
}

/// Formata timestamp Unix em string ISO 8601 / RFC 3339 simplificada.
#[must_use]
pub fn format_unix_timestamp(secs: i64) -> String {
    if secs <= 0 {
        return "1970-01-01T00:00:00Z".to_string();
    }
    // Conversão de calendário gregoriano sem dependências externas
    let s = secs as u64;
    let sec = s % 60;
    let min = (s / 60) % 60;
    let hour = (s / 3600) % 24;
    let days_since_epoch = s / 86400;

    let (year, month, day) = days_to_ymd(days_since_epoch);

    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{min:02}:{sec:02}Z")
}

fn days_to_ymd(days: u64) -> (u32, u32, u32) {
    let mut d = days;
    let mut year = 1970;

    loop {
        let leap = is_leap_year(year);
        let days_in_year = if leap { 366 } else { 365 };
        if d < days_in_year {
            break;
        }
        d -= days_in_year;
        year += 1;
    }

    let leap = is_leap_year(year);
    let month_days = [
        31,
        if leap { 29 } else { 28 },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];

    let mut month = 1;
    for &md in &month_days {
        if d < md {
            break;
        }
        d -= md;
        month += 1;
    }

    (year, month, (d + 1) as u32)
}

fn is_leap_year(y: u32) -> bool {
    (y.is_multiple_of(4) && !y.is_multiple_of(100)) || y.is_multiple_of(400)
}

/// Converte um caminho para forma relativa à raiz indicada, se aplicável.
#[must_use]
pub fn make_relative_path(path: &str, base: &str) -> String {
    let p = Path::new(path);
    let b = Path::new(base);

    if let Ok(rel) = p.strip_prefix(b) {
        let s = rel.to_string_lossy().to_string();
        if s.is_empty() {
            ".".to_string()
        } else {
            s
        }
    } else if let Ok(canonical_base) = std::fs::canonicalize(b) {
        if let Ok(rel) = p.strip_prefix(&canonical_base) {
            let s = rel.to_string_lossy().to_string();
            if s.is_empty() {
                ".".to_string()
            } else {
                s
            }
        } else {
            path.to_string()
        }
    } else {
        path.to_string()
    }
}

/// Aplica o limite aproximado de tokens (`budget`), truncando a lista de itens serializados.
///
/// Estimativa: ~4 caracteres por token.
pub fn apply_budget<T: serde::Serialize>(
    items: &mut Vec<T>,
    budget: Option<usize>,
) -> (bool, Option<usize>) {
    let Some(max_tokens) = budget else {
        return (false, None);
    };

    let max_chars = max_tokens.saturating_mul(4);
    let mut current_chars = 0;
    let mut kept_count = 0;

    for item in items.iter() {
        if let Ok(json) = serde_json::to_string(item) {
            let len = json.len() + 2; // vírgula e espaçamento
            if current_chars + len > max_chars && kept_count > 0 {
                break;
            }
            current_chars += len;
            kept_count += 1;
        }
    }

    if kept_count < items.len() {
        items.truncate(kept_count);
        (true, Some(kept_count))
    } else {
        (false, None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_size_filter() {
        assert_eq!(
            parse_size_filter(">1GB").unwrap(),
            (Some(1024 * 1024 * 1024 + 1), None)
        );
        assert_eq!(
            parse_size_filter(">=500KB").unwrap(),
            (Some(500 * 1024), None)
        );
        assert_eq!(
            parse_size_filter("<10MB").unwrap(),
            (None, Some(10 * 1024 * 1024 - 1))
        );
        assert_eq!(parse_size_filter("<=20B").unwrap(), (None, Some(20)));
        assert_eq!(parse_size_filter("=100").unwrap(), (Some(100), Some(100)));
        assert_eq!(parse_size_filter("100B").unwrap(), (Some(100), Some(100)));
    }

    #[test]
    fn test_parse_duration() {
        assert_eq!(parse_duration("30s").unwrap(), Duration::from_secs(30));
        assert_eq!(parse_duration("15m").unwrap(), Duration::from_secs(900));
        assert_eq!(parse_duration("1h").unwrap(), Duration::from_secs(3600));
        assert_eq!(parse_duration("2d").unwrap(), Duration::from_secs(172800));
        assert_eq!(parse_duration("1w").unwrap(), Duration::from_secs(604800));
    }

    #[test]
    fn test_format_bytes() {
        assert_eq!(format_bytes(500), "500 B");
        assert_eq!(format_bytes(1024), "1.0 KB");
        assert_eq!(format_bytes(1024 * 1024), "1.00 MB");
        assert_eq!(format_bytes(1024 * 1024 * 1024), "1.00 GB");
    }

    #[test]
    fn test_format_unix_timestamp() {
        assert_eq!(format_unix_timestamp(0), "1970-01-01T00:00:00Z");
        assert_eq!(format_unix_timestamp(1700000000), "2023-11-14T22:13:20Z");
    }

    #[test]
    fn test_relative_path() {
        let rel = make_relative_path("C:/Users/test/file.rs", "C:/Users/test");
        assert_eq!(rel, "file.rs");
    }
}
