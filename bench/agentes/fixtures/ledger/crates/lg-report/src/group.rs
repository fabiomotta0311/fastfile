//! Períodos de relatório: mês civil, trimestre e exercício fiscal.
//!
//! O exercício fiscal começa em `start_month` (1 = janeiro, o padrão). Com início em
//! abril, o exercício "2024" vai de 2024-04-01 a 2025-03-31: o ano do exercício é o ano
//! civil em que ele começa. O trimestre fiscal 1 começa no mês de início.

use std::collections::BTreeMap;
use std::fmt;

use lg_core::{Date, Txn};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FiscalCalendar {
    start_month: u8,
}

impl Default for FiscalCalendar {
    fn default() -> Self {
        FiscalCalendar { start_month: 1 }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Period {
    Month { year: i32, month: u8 },
    Quarter { fiscal_year: i32, quarter: u8 },
    Year { fiscal_year: i32 },
}

impl fmt::Display for Period {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Period::Month { year, month } => write!(f, "{year:04}-{month:02}"),
            Period::Quarter { fiscal_year, quarter } => write!(f, "FY{fiscal_year}-T{quarter}"),
            Period::Year { fiscal_year } => write!(f, "FY{fiscal_year}"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Granularity {
    Month,
    Quarter,
    Year,
}

impl Granularity {
    pub fn parse(s: &str) -> Option<Granularity> {
        match s.trim().to_ascii_lowercase().as_str() {
            "mes" | "mês" | "month" | "mensal" => Some(Granularity::Month),
            "trimestre" | "quarter" | "trimestral" => Some(Granularity::Quarter),
            "ano" | "year" | "anual" | "exercicio" => Some(Granularity::Year),
            _ => None,
        }
    }
}

impl FiscalCalendar {
    pub fn new(start_month: u8) -> Option<FiscalCalendar> {
        (1..=12).contains(&start_month).then_some(FiscalCalendar { start_month })
    }

    pub fn start_month(&self) -> u8 {
        self.start_month
    }

    /// Ano do exercício que contém `d`.
    pub fn fiscal_year(&self, d: Date) -> i32 {
        if d.month() >= self.start_month {
            d.year()
        } else {
            d.year() - 1
        }
    }

    /// Mês dentro do exercício: 0 no mês de início, 11 no último.
    fn fiscal_month_index(&self, d: Date) -> u8 {
        (d.month() + 12 - self.start_month) % 12
    }

    pub fn period(&self, d: Date, g: Granularity) -> Period {
        match g {
            Granularity::Month => Period::Month { year: d.year(), month: d.month() },
            Granularity::Quarter => Period::Quarter {
                fiscal_year: self.fiscal_year(d),
                quarter: self.fiscal_month_index(d) / 3 + 1,
            },
            Granularity::Year => Period::Year { fiscal_year: self.fiscal_year(d) },
        }
    }

    /// Primeiro e último dia do exercício.
    pub fn bounds(&self, fiscal_year: i32) -> (Date, Date) {
        let start = Date::ymd(fiscal_year, self.start_month, 1);
        let next = if self.start_month == 1 {
            Date::ymd(fiscal_year + 1, 1, 1)
        } else {
            Date::ymd(fiscal_year + 1, self.start_month, 1)
        };
        (start, next.add_days(-1))
    }

    pub fn group<'a>(&self, txns: &'a [Txn], g: Granularity) -> BTreeMap<Period, Vec<&'a Txn>> {
        let mut out: BTreeMap<Period, Vec<&Txn>> = BTreeMap::new();
        for t in txns {
            out.entry(self.period(t.date, g)).or_default().push(t);
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn calendar_year_by_default() {
        let c = FiscalCalendar::default();
        assert_eq!(c.fiscal_year(Date::ymd(2024, 6, 30)), 2024);
        assert_eq!(c.period(Date::ymd(2024, 5, 9), Granularity::Quarter).to_string(), "FY2024-T2");
        assert_eq!(c.bounds(2024), (Date::ymd(2024, 1, 1), Date::ymd(2024, 12, 31)));
    }

    #[test]
    fn april_start_months_before_april() {
        let c = FiscalCalendar::new(4).unwrap();
        assert_eq!(c.fiscal_year(Date::ymd(2025, 3, 31)), 2024);
        assert_eq!(c.fiscal_year(Date::ymd(2024, 12, 1)), 2024);
        assert_eq!(c.period(Date::ymd(2025, 1, 15), Granularity::Quarter).to_string(), "FY2024-T4");
        assert_eq!(c.bounds(2024), (Date::ymd(2024, 4, 1), Date::ymd(2025, 3, 31)));
    }

    #[test]
    fn invalid_start_month() {
        assert!(FiscalCalendar::new(0).is_none());
        assert!(FiscalCalendar::new(13).is_none());
    }

    #[test]
    fn granularity_names() {
        assert_eq!(Granularity::parse("Trimestre"), Some(Granularity::Quarter));
        assert_eq!(Granularity::parse("mensal"), Some(Granularity::Month));
        assert_eq!(Granularity::parse("x"), None);
    }

    #[test]
    fn month_periods_sort_chronologically() {
        let a = Period::Month { year: 2023, month: 12 };
        let b = Period::Month { year: 2024, month: 1 };
        assert!(a < b);
        assert_eq!(a.to_string(), "2023-12");
    }
}
