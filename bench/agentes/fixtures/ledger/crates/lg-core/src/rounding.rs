//! Divisão inteira com arredondamento explícito.
//!
//! Todo cálculo que produz fração de unidade menor (impostos, rateios, conversões)
//! termina aqui. O modo padrão do razão é [`RoundingMode::HalfEven`] (arredondamento
//! bancário), que não acumula viés em somas longas.

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RoundingMode {
    /// Empate vai para o par mais próximo: 0,125 -> 0,12 e 0,135 -> 0,14.
    #[default]
    HalfEven,
    /// Empate se afasta de zero: 0,125 -> 0,13 e -0,125 -> -0,13.
    HalfUp,
    /// Trunca em direção a zero.
    Down,
}

impl RoundingMode {
    pub fn parse(s: &str) -> Option<RoundingMode> {
        match s.trim().to_ascii_lowercase().as_str() {
            "half-even" | "half_even" | "bancario" | "bankers" => Some(RoundingMode::HalfEven),
            "half-up" | "half_up" | "comercial" => Some(RoundingMode::HalfUp),
            "down" | "truncar" | "truncate" => Some(RoundingMode::Down),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            RoundingMode::HalfEven => "half-even",
            RoundingMode::HalfUp => "half-up",
            RoundingMode::Down => "down",
        }
    }
}

/// `numer / denom` arredondado segundo `mode`. `denom` precisa ser positivo.
///
/// O sinal do resultado segue o de `numer`: o arredondamento é simétrico em torno de zero,
/// de modo que um estorno arredonda exatamente como a compra correspondente com sinal trocado.
pub fn div_round(numer: i128, denom: i128, mode: RoundingMode) -> i128 {
    assert!(denom > 0, "divisor deve ser positivo");
    let negative = numer < 0;
    let n = numer.unsigned_abs();
    let d = denom as u128;
    let q = n / d;
    let r = n % d;
    let magnitude = match mode {
        RoundingMode::Down => q,
        RoundingMode::HalfUp => {
            if r * 2 >= d {
                q + 1
            } else {
                q
            }
        }
        RoundingMode::HalfEven => round_half_even_magnitude(q, r, d),
    };
    if negative {
        -(magnitude as i128)
    } else {
        magnitude as i128
    }
}

fn round_half_even_magnitude(q: u128, r: u128, d: u128) -> u128 {
    let twice = r * 2;
    if twice < d {
        return q;
    }
    if twice > d {
        return q + 1;
    }
    // Empate exato: vai para o par.
    if q % 2 == 0 {
        q
    } else {
        q + 1
    }
}

/// Rateio de `total` em partes proporcionais a `weights`, sem perder unidades:
/// as sobras do arredondamento para baixo vão para as maiores frações restantes.
pub fn allocate(total: i64, weights: &[u64]) -> Vec<i64> {
    let sum: u128 = weights.iter().map(|&w| w as u128).sum();
    if weights.is_empty() || sum == 0 {
        return vec![0; weights.len()];
    }
    let negative = total < 0;
    let t = total.unsigned_abs() as u128;
    let mut parts: Vec<u128> = Vec::with_capacity(weights.len());
    let mut rems: Vec<(u128, usize)> = Vec::with_capacity(weights.len());
    for (i, &w) in weights.iter().enumerate() {
        let num = t * w as u128;
        parts.push(num / sum);
        rems.push((num % sum, i));
    }
    let assigned: u128 = parts.iter().sum();
    let mut left = t - assigned;
    rems.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    for (_, i) in rems {
        if left == 0 {
            break;
        }
        parts[i] += 1;
        left -= 1;
    }
    parts
        .into_iter()
        .map(|p| if negative { -(p as i64) } else { p as i64 })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn half_even_positive_ties() {
        assert_eq!(div_round(125, 10, RoundingMode::HalfEven), 12);
        assert_eq!(div_round(135, 10, RoundingMode::HalfEven), 14);
        assert_eq!(div_round(5, 10, RoundingMode::HalfEven), 0);
        assert_eq!(div_round(15, 10, RoundingMode::HalfEven), 2);
    }

    #[test]
    fn half_even_non_ties() {
        assert_eq!(div_round(124, 10, RoundingMode::HalfEven), 12);
        assert_eq!(div_round(126, 10, RoundingMode::HalfEven), 13);
        assert_eq!(div_round(-124, 10, RoundingMode::HalfEven), -12);
        assert_eq!(div_round(-126, 10, RoundingMode::HalfEven), -13);
    }

    #[test]
    fn half_up_and_down() {
        assert_eq!(div_round(125, 10, RoundingMode::HalfUp), 13);
        assert_eq!(div_round(-125, 10, RoundingMode::HalfUp), -13);
        assert_eq!(div_round(129, 10, RoundingMode::Down), 12);
        assert_eq!(div_round(-129, 10, RoundingMode::Down), -12);
    }

    #[test]
    fn parse_names() {
        assert_eq!(RoundingMode::parse("bancario"), Some(RoundingMode::HalfEven));
        assert_eq!(RoundingMode::parse("HALF-UP"), Some(RoundingMode::HalfUp));
        assert_eq!(RoundingMode::parse("x"), None);
    }

    #[test]
    fn allocate_keeps_total() {
        assert_eq!(allocate(100, &[1, 1, 1]), vec![34, 33, 33]);
        assert_eq!(allocate(-100, &[1, 1, 1]), vec![-34, -33, -33]);
        assert_eq!(allocate(10, &[0, 0]), vec![0, 0]);
        assert_eq!(allocate(7, &[2, 1]).iter().sum::<i64>(), 7);
    }
}
