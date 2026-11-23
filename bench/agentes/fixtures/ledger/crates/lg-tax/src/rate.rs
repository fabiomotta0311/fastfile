//! Alíquota em partes por milhão.

use std::fmt;

use lg_core::{Error, Result};

pub const PPM: i64 = 1_000_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Rate {
    ppm: i64,
}

impl Rate {
    pub const ZERO: Rate = Rate { ppm: 0 };

    pub const fn from_ppm(ppm: i64) -> Rate {
        Rate { ppm }
    }

    pub fn ppm(&self) -> i64 {
        self.ppm
    }

    /// "12.5", "12,5%", "0.65" -> partes por milhão. Até 4 casas decimais.
    pub fn parse_percent(s: &str) -> Result<Rate> {
        let t = s.trim().trim_end_matches('%').trim().replace(',', ".");
        let err = || Error::InvalidAmount(format!("alíquota '{s}'"));
        let (int_part, frac_part) = t.split_once('.').unwrap_or((&t, ""));
        if int_part.is_empty() && frac_part.is_empty() || frac_part.len() > 4 {
            return Err(err());
        }
        if !int_part.bytes().all(|b| b.is_ascii_digit()) || !frac_part.bytes().all(|b| b.is_ascii_digit()) {
            return Err(err());
        }
        let int_v: i64 = if int_part.is_empty() { 0 } else { int_part.parse().map_err(|_| err())? };
        let mut frac = frac_part.to_string();
        while frac.len() < 4 {
            frac.push('0');
        }
        let frac_v: i64 = frac.parse().map_err(|_| err())?;
        let ppm = int_v.checked_mul(10_000).and_then(|v| v.checked_add(frac_v)).ok_or_else(err)?;
        if ppm > 1_000_000 {
            return Err(err());
        }
        Ok(Rate { ppm })
    }

    pub fn plus(self, other: Rate) -> Rate {
        Rate { ppm: self.ppm + other.ppm }
    }
}

impl fmt::Display for Rate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let whole = self.ppm / 10_000;
        let frac = self.ppm % 10_000;
        if frac == 0 {
            write!(f, "{whole}%")
        } else {
            let s = format!("{frac:04}");
            write!(f, "{whole}.{}%", s.trim_end_matches('0'))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_and_display() {
        assert_eq!(Rate::parse_percent("12.5").unwrap().ppm(), 125_000);
        assert_eq!(Rate::parse_percent("0,65%").unwrap().ppm(), 6_500);
        assert_eq!(Rate::parse_percent("18").unwrap().ppm(), 180_000);
        assert_eq!(Rate::from_ppm(125_000).to_string(), "12.5%");
        assert_eq!(Rate::from_ppm(6_500).to_string(), "0.65%");
        assert_eq!(Rate::from_ppm(180_000).to_string(), "18%");
    }

    #[test]
    fn parse_rejects() {
        for s in ["", "abc", "1.23456", "101", "-1", "1.2.3"] {
            assert!(Rate::parse_percent(s).is_err(), "{s}");
        }
    }
}
