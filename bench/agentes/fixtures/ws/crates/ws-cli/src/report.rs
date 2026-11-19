use ws_core::money::{format_cents, parse_cents};
use ws_text::words::normalize;

/// Soma valores monetários em texto e conta as linhas inválidas.
pub fn summarize(lines: &[&str]) -> String {
    let mut total = 0i64;
    let mut invalid = 0usize;
    for line in lines {
        match parse_cents(&normalize(line)) {
            Some(c) => total += c,
            None => invalid += 1,
        }
    }
    format!("total={} invalid={}", format_cents(total), invalid)
}
