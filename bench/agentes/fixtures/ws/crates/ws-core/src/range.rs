pub fn clamp(v: i64, lo: i64, hi: i64) -> i64 {
    if v < lo {
        lo
    } else if v > hi {
        hi
    } else {
        v
    }
}

/// Intervalos semiabertos `[início, fim)`: intervalos que só se tocam não se sobrepõem.
pub fn overlaps(a: (i64, i64), b: (i64, i64)) -> bool {
    a.0 < b.1 && b.0 < a.1
}

/// Índice circular; aceita `i` negativo.
pub fn wrap_index(i: i64, len: i64) -> i64 {
    i.rem_euclid(len)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn limita_nos_extremos() {
        assert_eq!(clamp(-5, 0, 10), 0);
        assert_eq!(clamp(50, 0, 10), 10);
    }
}
