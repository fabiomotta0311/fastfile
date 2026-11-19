//! Valor monetário em unidades menores.

use std::cmp::Ordering;
use std::fmt;

use crate::currency::{self, Currency};
use crate::error::{Error, Result};
use crate::rounding::{div_round, RoundingMode};

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct Money {
    minor: i64,
    currency: &'static Currency,
}

impl Money {
    pub fn from_minor(minor: i64, currency: &'static Currency) -> Money {
        Money { minor, currency }
    }

    pub fn zero(currency: &'static Currency) -> Money {
        Money::from_minor(0, currency)
    }

    /// Atalho para testes e exemplos: `Money::of(1999, "BRL")` = R$ 19,99.
    pub fn of(minor: i64, code: &str) -> Money {
        Money::from_minor(minor, currency::require(code).expect("moeda conhecida"))
    }

    pub fn minor(&self) -> i64 {
        self.minor
    }

    pub fn currency(&self) -> &'static Currency {
        self.currency
    }

    pub fn is_zero(&self) -> bool {
        self.minor == 0
    }

    pub fn is_negative(&self) -> bool {
        self.minor < 0
    }

    pub fn abs(&self) -> Money {
        Money::from_minor(self.minor.abs(), self.currency)
    }

    pub fn neg(&self) -> Money {
        Money::from_minor(-self.minor, self.currency)
    }

    fn same(&self, other: &Money) -> Result<()> {
        if self.currency.code == other.currency.code {
            Ok(())
        } else {
            Err(Error::CurrencyMismatch { left: self.currency.code, right: other.currency.code })
        }
    }

    pub fn checked_add(&self, other: &Money) -> Result<Money> {
        self.same(other)?;
        let v = self.minor.checked_add(other.minor).ok_or(Error::Overflow)?;
        Ok(Money::from_minor(v, self.currency))
    }

    pub fn checked_sub(&self, other: &Money) -> Result<Money> {
        self.same(other)?;
        let v = self.minor.checked_sub(other.minor).ok_or(Error::Overflow)?;
        Ok(Money::from_minor(v, self.currency))
    }

    /// Multiplica por `numer / denom` com arredondamento explícito.
    pub fn mul_ratio(&self, numer: i64, denom: i64, mode: RoundingMode) -> Result<Money> {
        if denom <= 0 {
            return Err(Error::InvalidAmount(format!("divisor {denom}")));
        }
        let v = div_round(self.minor as i128 * numer as i128, denom as i128, mode);
        let v = i64::try_from(v).map_err(|_| Error::Overflow)?;
        Ok(Money::from_minor(v, self.currency))
    }

    /// Soma uma sequência; vazia dá zero na moeda indicada.
    pub fn sum<'a, I>(currency: &'static Currency, items: I) -> Result<Money>
    where
        I: IntoIterator<Item = &'a Money>,
    {
        let mut acc = Money::zero(currency);
        for m in items {
            acc = acc.checked_add(m)?;
        }
        Ok(acc)
    }

    /// Lê um valor decimal com ponto ou vírgula como separador decimal e sem separador de
    /// milhar: "1234.5", "-0,99", "+12". Casas além das da moeda são arredondadas com `mode`.
    pub fn parse(text: &str, currency: &'static Currency, mode: RoundingMode) -> Result<Money> {
        let t = text.trim();
        let invalid = || Error::InvalidAmount(text.to_string());
        if t.is_empty() {
            return Err(invalid());
        }
        let (negative, digits) = match t.as_bytes()[0] {
            b'-' => (true, &t[1..]),
            b'+' => (false, &t[1..]),
            _ => (false, t),
        };
        let (int_part, frac_part) = match digits.find(['.', ',']) {
            Some(i) => (&digits[..i], &digits[i + 1..]),
            None => (digits, ""),
        };
        if int_part.is_empty() && frac_part.is_empty() {
            return Err(invalid());
        }
        if !int_part.bytes().all(|b| b.is_ascii_digit()) || !frac_part.bytes().all(|b| b.is_ascii_digit()) {
            return Err(invalid());
        }
        let units = currency.minor_units as usize;
        let int_value: i128 = if int_part.is_empty() { 0 } else { int_part.parse().map_err(|_| invalid())? };
        let frac_digits = frac_part.len();
        let frac_value: i128 = if frac_part.is_empty() { 0 } else { frac_part.parse().map_err(|_| invalid())? };
        let pow = |n: usize| 10_i128.checked_pow(n as u32).ok_or(Error::Overflow);
        let whole = int_value.checked_mul(pow(frac_digits)?).ok_or(Error::Overflow)? + frac_value;
        let mut minor = if frac_digits <= units {
            whole.checked_mul(pow(units - frac_digits)?).ok_or(Error::Overflow)?
        } else {
            div_round(whole, pow(frac_digits - units)?, mode)
        };
        if negative {
            minor = -minor;
        }
        let minor = i64::try_from(minor).map_err(|_| Error::Overflow)?;
        Ok(Money::from_minor(minor, currency))
    }

    /// Representação canônica sem símbolo nem agrupamento: "-1234.50", "1234" (sem casas).
    pub fn to_plain(&self) -> String {
        let units = self.currency.minor_units as u32;
        let sign = if self.minor < 0 { "-" } else { "" };
        let abs = self.minor.unsigned_abs();
        if units == 0 {
            return format!("{sign}{abs}");
        }
        let scale = 10_u64.pow(units);
        format!("{sign}{}.{:0width$}", abs / scale, abs % scale, width = units as usize)
    }
}

impl PartialOrd for Money {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        if self.currency.code == other.currency.code {
            Some(self.minor.cmp(&other.minor))
        } else {
            None
        }
    }
}

impl fmt::Debug for Money {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} {}", self.to_plain(), self.currency.code)
    }
}

impl fmt::Display for Money {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} {}", self.currency.code, self.to_plain())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn brl() -> &'static Currency {
        currency::brl()
    }

    #[test]
    fn parse_basic() {
        let m = Money::parse("1234.5", brl(), RoundingMode::HalfEven).unwrap();
        assert_eq!(m.minor(), 123450);
        assert_eq!(Money::parse("-0,99", brl(), RoundingMode::HalfEven).unwrap().minor(), -99);
        assert_eq!(Money::parse("+12", brl(), RoundingMode::HalfEven).unwrap().minor(), 1200);
        assert_eq!(Money::parse(",5", brl(), RoundingMode::HalfEven).unwrap().minor(), 50);
    }

    #[test]
    fn parse_rounds_extra_digits() {
        assert_eq!(Money::parse("0.125", brl(), RoundingMode::HalfEven).unwrap().minor(), 12);
        assert_eq!(Money::parse("0.125", brl(), RoundingMode::HalfUp).unwrap().minor(), 13);
        assert_eq!(Money::parse("0.129", brl(), RoundingMode::Down).unwrap().minor(), 12);
    }

    #[test]
    fn parse_rejects_garbage() {
        for s in ["", "-", "1.2.3", "1 000", "abc", "1e3", "."] {
            assert!(Money::parse(s, brl(), RoundingMode::HalfEven).is_err(), "{s}");
        }
    }

    #[test]
    fn arithmetic_checks_currency() {
        let a = Money::of(100, "BRL");
        let b = Money::of(50, "USD");
        assert!(a.checked_add(&b).is_err());
        assert_eq!(a.checked_sub(&Money::of(30, "BRL")).unwrap().minor(), 70);
        assert_eq!(a.partial_cmp(&b), None);
    }

    #[test]
    fn mul_ratio_rounds() {
        let m = Money::of(1000, "BRL");
        assert_eq!(m.mul_ratio(1, 3, RoundingMode::HalfEven).unwrap().minor(), 333);
        assert_eq!(m.mul_ratio(2, 3, RoundingMode::Down).unwrap().minor(), 666);
        assert!(m.mul_ratio(1, 0, RoundingMode::HalfEven).is_err());
    }

    #[test]
    fn plain_text() {
        assert_eq!(Money::of(-123450, "BRL").to_plain(), "-1234.50");
        assert_eq!(Money::of(5, "BRL").to_plain(), "0.05");
        assert_eq!(Money::of(1234, "KWD").to_plain(), "1.234");
    }

    #[test]
    fn sum_empty_is_zero() {
        let items: Vec<Money> = vec![];
        assert!(Money::sum(brl(), &items).unwrap().is_zero());
        let items = vec![Money::of(1, "BRL"), Money::of(2, "BRL")];
        assert_eq!(Money::sum(brl(), &items).unwrap().minor(), 3);
    }
}
