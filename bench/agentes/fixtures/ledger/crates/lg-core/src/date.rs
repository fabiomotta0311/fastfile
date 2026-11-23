//! Data civil (calendário gregoriano proléptico), sem fuso nem hora.

use std::fmt;

use crate::error::{Error, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Date {
    year: i32,
    month: u8,
    day: u8,
}

pub fn is_leap_year(year: i32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

pub fn days_in_month(year: i32, month: u8) -> u8 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if is_leap_year(year) => 29,
        2 => 28,
        _ => 0,
    }
}

impl Date {
    pub fn new(year: i32, month: u8, day: u8) -> Result<Date> {
        if !(1..=9999).contains(&year) || !(1..=12).contains(&month) {
            return Err(Error::InvalidDate(format!("{year:04}-{month:02}-{day:02}")));
        }
        if day == 0 || day > days_in_month(year, month) {
            return Err(Error::InvalidDate(format!("{year:04}-{month:02}-{day:02}")));
        }
        Ok(Date { year, month, day })
    }

    pub fn ymd(year: i32, month: u8, day: u8) -> Date {
        Date::new(year, month, day).expect("data válida")
    }

    pub fn year(&self) -> i32 {
        self.year
    }

    pub fn month(&self) -> u8 {
        self.month
    }

    pub fn day(&self) -> u8 {
        self.day
    }

    /// "2024-03-31".
    pub fn parse_iso(s: &str) -> Result<Date> {
        let err = || Error::InvalidDate(s.to_string());
        let parts: Vec<&str> = s.trim().split('-').collect();
        if parts.len() != 3 || parts[0].len() != 4 || parts[1].len() != 2 || parts[2].len() != 2 {
            return Err(err());
        }
        let y = parts[0].parse().map_err(|_| err())?;
        let m = parts[1].parse().map_err(|_| err())?;
        let d = parts[2].parse().map_err(|_| err())?;
        Date::new(y, m, d)
    }

    /// "31/03/2024" (dia/mês/ano, como nos extratos brasileiros).
    pub fn parse_br(s: &str) -> Result<Date> {
        let err = || Error::InvalidDate(s.to_string());
        let parts: Vec<&str> = s.trim().split('/').collect();
        if parts.len() != 3 || parts[2].len() != 4 || parts[0].is_empty() || parts[1].is_empty() {
            return Err(err());
        }
        let d = parts[0].parse().map_err(|_| err())?;
        let m = parts[1].parse().map_err(|_| err())?;
        let y = parts[2].parse().map_err(|_| err())?;
        Date::new(y, m, d)
    }

    /// Aceita ISO ou o formato brasileiro.
    pub fn parse_any(s: &str) -> Result<Date> {
        if s.contains('/') {
            Date::parse_br(s)
        } else {
            Date::parse_iso(s)
        }
    }

    /// Dias desde 1970-01-01 (algoritmo de Howard Hinnant).
    pub fn to_days(&self) -> i64 {
        let y = self.year as i64 - if self.month <= 2 { 1 } else { 0 };
        let era = if y >= 0 { y } else { y - 399 } / 400;
        let yoe = y - era * 400;
        let m = self.month as i64;
        let doy = (153 * (m + if m > 2 { -3 } else { 9 }) + 2) / 5 + self.day as i64 - 1;
        let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
        era * 146097 + doe - 719468
    }

    pub fn from_days(days: i64) -> Date {
        let z = days + 719468;
        let era = if z >= 0 { z } else { z - 146096 } / 146097;
        let doe = z - era * 146097;
        let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
        let y = yoe + era * 400;
        let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
        let mp = (5 * doy + 2) / 153;
        let d = (doy - (153 * mp + 2) / 5 + 1) as u8;
        let m = if mp < 10 { mp + 3 } else { mp - 9 } as u8;
        let y = if m <= 2 { y + 1 } else { y } as i32;
        Date { year: y, month: m, day: d }
    }

    pub fn add_days(&self, n: i64) -> Date {
        Date::from_days(self.to_days() + n)
    }

    /// 0 = segunda ... 6 = domingo.
    pub fn weekday(&self) -> u8 {
        ((self.to_days() + 3).rem_euclid(7)) as u8
    }

    pub fn first_of_month(&self) -> Date {
        Date { day: 1, ..*self }
    }

    pub fn last_of_month(&self) -> Date {
        Date { day: days_in_month(self.year, self.month), ..*self }
    }

    pub fn month_key(&self) -> (i32, u8) {
        (self.year, self.month)
    }
}

impl fmt::Display for Date {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:04}-{:02}-{:02}", self.year, self.month, self.day)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn leap_years() {
        assert!(is_leap_year(2024));
        assert!(!is_leap_year(2023));
        assert!(!is_leap_year(1900));
        assert!(!is_leap_year(2100));
    }

    #[test]
    fn parse_formats() {
        assert_eq!(Date::parse_iso("2024-03-31").unwrap(), Date::ymd(2024, 3, 31));
        assert_eq!(Date::parse_br("31/03/2024").unwrap(), Date::ymd(2024, 3, 31));
        assert_eq!(Date::parse_br("1/4/2024").unwrap(), Date::ymd(2024, 4, 1));
        assert_eq!(Date::parse_any("2024-02-29").unwrap(), Date::ymd(2024, 2, 29));
    }

    #[test]
    fn parse_rejects_invalid() {
        for s in ["2023-02-29", "2024-13-01", "2024-04-31", "31-03-2024", "2024/03/31", "00/01/2024", "2024-3-1"] {
            assert!(Date::parse_any(s).is_err(), "{s}");
        }
    }

    #[test]
    fn days_roundtrip() {
        assert_eq!(Date::ymd(1970, 1, 1).to_days(), 0);
        assert_eq!(Date::ymd(2024, 3, 1).to_days() - Date::ymd(2024, 2, 28).to_days(), 2);
        for n in [-1000, 0, 1, 19_000, 20_000, 60_000] {
            assert_eq!(Date::from_days(n).to_days(), n);
        }
    }

    #[test]
    fn calendar_helpers() {
        let d = Date::ymd(2024, 2, 10);
        assert_eq!(d.last_of_month(), Date::ymd(2024, 2, 29));
        assert_eq!(d.first_of_month(), Date::ymd(2024, 2, 1));
        assert_eq!(Date::ymd(2024, 12, 31).add_days(1), Date::ymd(2025, 1, 1));
        assert_eq!(Date::ymd(2024, 3, 18).weekday(), 0);
        assert_eq!(d.to_string(), "2024-02-10");
    }
}
