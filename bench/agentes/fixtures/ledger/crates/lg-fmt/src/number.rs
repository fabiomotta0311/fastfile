//! Números e valores monetários por localidade.

use lg_core::Money;

use crate::locale::Locale;

/// Insere o separador de milhar a cada três dígitos, da direita para a esquerda.
fn group_digits(digits: &str, sep: Option<char>) -> String {
    let Some(sep) = sep else {
        return digits.to_string();
    };
    let len = digits.len();
    let mut out = String::with_capacity(len + len / 3);
    for (i, ch) in digits.chars().enumerate() {
        if i > 0 && (len - i) % 3 == 0 {
            out.push(sep);
        }
        out.push(ch);
    }
    out
}

/// Formata `minor` unidades menores com `minor_units` casas: `format_minor(-123456, 2, en-US)`
/// = "-1,234.56". Sem símbolo de moeda.
pub fn format_minor(minor: i64, minor_units: u8, locale: &Locale) -> String {
    let negative = minor < 0;
    let abs = minor.unsigned_abs();
    let scale = 10_u64.pow(minor_units as u32);
    let int_part = group_digits(&(abs / scale).to_string(), locale.group);
    let body = if minor_units == 0 {
        int_part
    } else {
        format!("{int_part}{}{:0width$}", locale.decimal, abs % scale, width = minor_units as usize)
    };
    match (negative, locale.parens_for_negative) {
        (false, _) => body,
        (true, false) => format!("-{body}"),
        (true, true) => format!("({body})"),
    }
}

/// Valor com símbolo, na posição da localidade: "R$ 1.234,56", "1.234,56 €", "-$12.00".
pub fn format_amount(money: Money, locale: &Locale) -> String {
    let cur = money.currency();
    let number = format_minor(money.minor().abs(), cur.minor_units, locale);
    let symbol = cur.display_symbol();
    let space = if locale.symbol_space || symbol.len() == 3 && symbol == cur.code { " " } else { "" };
    let unsigned = if locale.symbol_first {
        format!("{symbol}{space}{number}")
    } else {
        format!("{number}{space}{symbol}")
    };
    match (money.is_negative(), locale.parens_for_negative) {
        (false, _) => unsigned,
        (true, false) => format!("-{unsigned}"),
        (true, true) => format!("({unsigned})"),
    }
}

/// Percentual com uma casa: 0.125 -> "12,5%" em pt-BR. Entrada em pontos-base (1/10000).
pub fn format_basis_points(bp: i64, locale: &Locale) -> String {
    let s = format_minor(bp, 2, &Locale { group: None, ..*locale });
    let trimmed = s.trim_end_matches('0').trim_end_matches(locale.decimal);
    format!("{trimmed}%")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn l(tag: &str) -> Locale {
        Locale::by_tag(tag).unwrap()
    }

    #[test]
    fn grouping_and_decimals() {
        assert_eq!(format_minor(123456, 2, &l("en-US")), "1,234.56");
        assert_eq!(format_minor(123456, 2, &l("pt-BR")), "1.234,56");
        assert_eq!(format_minor(99999, 2, &l("pt-BR")), "999,99");
        assert_eq!(format_minor(1234567, 2, &l("C")), "12345.67");
        assert_eq!(format_minor(-5, 2, &l("en-US")), "-0.05");
        assert_eq!(format_minor(1234, 3, &l("de-DE")), "1,234");
    }

    #[test]
    fn amounts_with_symbol() {
        assert_eq!(format_amount(Money::of(123456, "BRL"), &l("pt-BR")), "R$ 1.234,56");
        assert_eq!(format_amount(Money::of(-1200, "USD"), &l("en-US")), "-$12.00");
        assert_eq!(format_amount(Money::of(123456, "EUR"), &l("de-DE")), "1.234,56 €");
        assert_eq!(format_amount(Money::of(1050, "CHF"), &l("en-US")), "CHF 10.50");
    }

    #[test]
    fn parentheses_for_negatives() {
        let p = l("en-US").with_parens();
        assert_eq!(format_amount(Money::of(-1200, "USD"), &p), "($12.00)");
        assert_eq!(format_minor(-1200, 2, &p), "(12.00)");
    }

    #[test]
    fn basis_points() {
        assert_eq!(format_basis_points(1250, &l("pt-BR")), "12,5%");
        assert_eq!(format_basis_points(1800, &l("en-US")), "18%");
    }
}
