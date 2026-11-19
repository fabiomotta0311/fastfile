//! Cálculo de impostos.
//!
//! Dois escopos de arredondamento convivem:
//! - [`RoundingScope::PerLine`]: arredonda o imposto de cada linha e soma (padrão das notas
//!   de serviço);
//! - [`RoundingScope::PerInvoice`]: soma as bases e arredonda uma vez (padrão do varejo).
//!
//! Estornos entram como linhas negativas e precisam produzir exatamente o imposto da
//! compra original com o sinal trocado.

use lg_core::{Error, Money, Result, RoundingMode};

use crate::rate::{Rate, PPM};
use crate::regions::Region;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RoundingScope {
    #[default]
    PerLine,
    PerInvoice,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Line {
    pub description: String,
    pub net: Money,
    /// Linhas de serviço usam a alíquota de serviço da região.
    pub service: bool,
}

impl Line {
    pub fn goods(description: &str, net: Money) -> Line {
        Line { description: description.to_string(), net, service: false }
    }

    pub fn service(description: &str, net: Money) -> Line {
        Line { description: description.to_string(), net, service: true }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Breakdown {
    pub net: Money,
    pub tax: Money,
    pub gross: Money,
    pub per_line: Vec<Money>,
}

/// Imposto de um valor à alíquota dada.
pub fn tax_for(net: Money, rate: Rate, mode: RoundingMode) -> Result<Money> {
    net.mul_ratio(rate.ppm(), PPM, mode)
}

fn rate_for(line: &Line, region: &Region) -> Rate {
    if line.service {
        region.service_rate()
    } else {
        region.goods_rate()
    }
}

pub fn breakdown(lines: &[Line], region: &Region, scope: RoundingScope) -> Result<Breakdown> {
    let first = lines.first().ok_or_else(|| Error::InvalidAmount("nota sem linhas".into()))?;
    let cur = first.net.currency();
    let mode = region.rounding;
    let mut per_line = Vec::with_capacity(lines.len());
    let mut net = Money::zero(cur);
    for l in lines {
        net = net.checked_add(&l.net)?;
        per_line.push(tax_for(l.net, rate_for(l, region), mode)?);
    }
    let tax = match scope {
        RoundingScope::PerLine => Money::sum(cur, &per_line)?,
        RoundingScope::PerInvoice => {
            // Soma exata em ppm e arredonda uma única vez.
            let mut exact: i128 = 0;
            for l in lines {
                exact += l.net.minor() as i128 * rate_for(l, region).ppm() as i128;
            }
            let v = lg_core::rounding::div_round(exact, PPM as i128, mode);
            Money::from_minor(i64::try_from(v).map_err(|_| Error::Overflow)?, cur)
        }
    };
    let gross = net.checked_add(&tax)?;
    Ok(Breakdown { net, tax, gross, per_line })
}

/// Valor líquido a partir do bruto (imposto "por dentro"): net = gross / (1 + rate).
pub fn net_from_gross(gross: Money, rate: Rate, mode: RoundingMode) -> Result<Money> {
    gross.mul_ratio(PPM, PPM + rate.ppm(), mode)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::regions::region;

    #[test]
    fn simple_tax() {
        let t = tax_for(Money::of(10000, "BRL"), Rate::from_ppm(180_000), RoundingMode::HalfEven).unwrap();
        assert_eq!(t.minor(), 1800);
    }

    #[test]
    fn positive_ties_follow_mode() {
        // 2,50 * 5% = 0,125
        let net = Money::of(250, "BRL");
        let r = Rate::from_ppm(50_000);
        assert_eq!(tax_for(net, r, RoundingMode::HalfEven).unwrap().minor(), 12);
        assert_eq!(tax_for(net, r, RoundingMode::HalfUp).unwrap().minor(), 13);
    }

    #[test]
    fn per_line_vs_per_invoice() {
        let sp = region("BR-SP").unwrap();
        let lines = vec![
            Line::goods("a", Money::of(333, "BRL")),
            Line::goods("b", Money::of(333, "BRL")),
            Line::goods("c", Money::of(334, "BRL")),
        ];
        let pl = breakdown(&lines, sp, RoundingScope::PerLine).unwrap();
        let pi = breakdown(&lines, sp, RoundingScope::PerInvoice).unwrap();
        assert_eq!(pl.net.minor(), 1000);
        assert_eq!(pi.tax.minor(), 180);
        assert_eq!(pl.per_line.len(), 3);
        assert_eq!(pl.gross.minor(), pl.net.minor() + pl.tax.minor());
    }

    #[test]
    fn service_lines_use_service_rate() {
        let sp = region("BR-SP").unwrap();
        let b = breakdown(&[Line::service("consultoria", Money::of(10000, "BRL"))], sp, RoundingScope::PerLine).unwrap();
        assert_eq!(b.tax.minor(), 500);
    }

    #[test]
    fn empty_invoice_is_error() {
        assert!(breakdown(&[], region("BR-SP").unwrap(), RoundingScope::PerLine).is_err());
    }

    #[test]
    fn gross_to_net() {
        let n = net_from_gross(Money::of(11800, "BRL"), Rate::from_ppm(180_000), RoundingMode::HalfEven).unwrap();
        assert_eq!(n.minor(), 10000);
    }
}
