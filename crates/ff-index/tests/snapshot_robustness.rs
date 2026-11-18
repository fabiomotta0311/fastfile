//! Robustez do carregamento de snapshot com entrada corrompida ou hostil (nunca pânico, nunca OOM).
//!
//! Gerador pseudoaleatório determinístico (xorshift): o teste é reprodutível e roda no CI normal.
//! Para exploração longa guiada por cobertura, ver `fuzz/` (cargo-fuzz).

use ff_index::entry::{FLAG_DIRECTORY, FLAG_FILE, NO_PARENT};
use ff_index::FileIndex;

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n.max(1) as u64) as usize
    }
}

fn fnv(data: &[u8]) -> u64 {
    let mut h = 0xcbf29ce484222325u64;
    for &b in data {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

fn valid_snapshot() -> Vec<u8> {
    let mut index = FileIndex::new();
    let root = index
        .add_entry(1, NO_PARENT, "C:", FLAG_DIRECTORY, 0, 0)
        .unwrap();
    let dir = index
        .add_entry(2, root, "Users", FLAG_DIRECTORY, 0, 1)
        .unwrap();
    for i in 0..20u64 {
        index
            .add_entry(10 + i, dir, &format!("arquivo-{i}.txt"), FLAG_FILE, i, 2)
            .unwrap();
    }
    let mut out = Vec::new();
    index.save_snapshot(&mut out).unwrap();
    out
}

/// Carrega e, se aceito, exercita todas as entradas: nada pode entrar em pânico.
fn load_and_walk(bytes: &[u8]) {
    if let Ok(index) = FileIndex::load_snapshot(&mut &bytes[..]) {
        for id in 0..index.len().min(10_000) as u32 {
            let _ = index.name(id);
            let _ = index.name_lc(id);
            let _ = index.build_path(id);
        }
    }
}

#[test]
fn valid_snapshot_roundtrips() {
    let bytes = valid_snapshot();
    let index = FileIndex::load_snapshot(&mut &bytes[..]).unwrap();
    assert_eq!(index.len(), 22);
}

#[test]
fn absurd_counts_fail_fast_without_allocating() {
    let mut h = Vec::new();
    h.extend_from_slice(b"FFSNAP01");
    h.extend_from_slice(&0u16.to_le_bytes()); // volume vazio
    h.extend_from_slice(&0u64.to_le_bytes()); // journal_id
    h.extend_from_slice(&0i64.to_le_bytes()); // last_usn
    for _ in 0..4 {
        h.extend_from_slice(&u64::MAX.to_le_bytes()); // entries, names, names_lc, frn
    }
    assert!(FileIndex::load_snapshot(&mut &h[..]).is_err());

    // Só a arena de nomes absurda, com o resto plausível.
    let mut h2 = h.clone();
    let off = 8 + 2 + 8 + 8;
    h2[off..off + 8].copy_from_slice(&0u64.to_le_bytes());
    h2[off + 16..off + 24].copy_from_slice(&0u64.to_le_bytes());
    h2[off + 24..off + 32].copy_from_slice(&0u64.to_le_bytes());
    assert!(FileIndex::load_snapshot(&mut &h2[..]).is_err());
}

#[test]
fn mutated_snapshots_never_panic() {
    let base = valid_snapshot();
    let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
    for round in 0..4000 {
        let mut b = base.clone();
        match rng.below(4) {
            0 => {
                for _ in 0..1 + rng.below(8) {
                    let i = rng.below(b.len());
                    b[i] = rng.next() as u8;
                }
            }
            1 => b.truncate(rng.below(b.len())),
            2 => b.extend((0..rng.below(64)).map(|_| rng.next() as u8)),
            _ => {
                let i = rng.below(b.len());
                b[i] ^= 1 << rng.below(8);
            }
        }
        // Metade das rodadas: recalcula o checksum para que a mutação passe da validação de integridade
        // e alcance os consumidores (offsets de nomes, pais, ciclos).
        if round % 2 == 0 && b.len() > 8 {
            let n = b.len() - 8;
            let sum = fnv(&b[..n]);
            b[n..].copy_from_slice(&sum.to_le_bytes());
        }
        load_and_walk(&b);
    }
}

#[test]
fn random_garbage_never_panics() {
    let mut rng = Rng(42);
    for _ in 0..2000 {
        let len = rng.below(256);
        let mut b: Vec<u8> = (0..len).map(|_| rng.next() as u8).collect();
        if rng.below(2) == 0 && b.len() >= 8 {
            b[..8].copy_from_slice(b"FFSNAP01");
        }
        load_and_walk(&b);
    }
}
