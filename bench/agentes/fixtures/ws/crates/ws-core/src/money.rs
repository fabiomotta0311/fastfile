/// Converte "12.5", "12.50" ou "-3" em centavos.
pub fn parse_cents(s: &str) -> Option<i64> {
    let s = s.trim();
    let (neg, body) = match s.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, s),
    };
    let (int, frac) = body.split_once('.').unwrap_or((body, ""));
    if int.is_empty() || frac.len() > 2 {
        return None;
    }
    if !int.chars().chain(frac.chars()).all(|c| c.is_ascii_digit()) {
        return None;
    }
    let frac_cents: i64 = match frac.len() {
        0 => 0,
        1 => frac.parse::<i64>().ok()? * 10,
        _ => frac.parse::<i64>().ok()?,
    };
    let total = int.parse::<i64>().ok()?.checked_mul(100)?.checked_add(frac_cents)?;
    Some(if neg { -total } else { total })
}

/// Formata centavos como "12.50"; valores negativos levam o sinal "-".
pub fn format_cents(c: i64) -> String {
    let sign = if c < 0 { "-" } else { "" };
    let abs = c.abs();
    format!("{}{}.{:02}", sign, abs / 100, abs % 100)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ida_e_volta() {
        assert_eq!(parse_cents("12.50"), Some(1250));
        assert_eq!(format_cents(1250), "12.50");
    }
}
