//! Robustez do protocolo IPC: requisições malformadas ou hostis nunca derrubam o handler.
//! Determinístico (xorshift) para rodar no CI normal; exploração longa fica em `fuzz/`.

use std::sync::{Arc, RwLock};

use ff_index::{FileIndex, FLAG_DIRECTORY, FLAG_FILE, NO_PARENT};
use ff_indexer::{IpcRequest, RequestHandler};

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

fn handler() -> RequestHandler {
    let mut idx = FileIndex::new();
    let root = if cfg!(windows) { "C:" } else { "/volume" };
    idx.volume = root.to_string();
    let r = idx
        .add_entry(0, NO_PARENT, root, FLAG_DIRECTORY, 0, 0)
        .unwrap();
    let d = idx.add_entry(1, r, "src", FLAG_DIRECTORY, 0, 0).unwrap();
    for i in 0..50u64 {
        idx.add_entry(10 + i, d, &format!("file{i}.rs"), FLAG_FILE, i * 10, 5)
            .unwrap();
    }
    RequestHandler::new(Arc::new(RwLock::new(idx)))
}

const SEEDS: &[&str] = &[
    r#"{"type":"Ping"}"#,
    r#"{"type":"Status"}"#,
    r#"{"type":"Find","payload":{"pattern":"file","limit":10,"cursor":0,"all":true}}"#,
    r#"{"type":"Find","payload":{"pattern":"f.*e","is_regex":true,"limit":5,"min_size":10,"max_size":400}}"#,
    r#"{"type":"Find","payload":{"pattern":"flie","fuzzy":true,"extension":"rs","file_type":"file"}}"#,
    r#"{"type":"Du","payload":{"target":"C:\\","depth":2,"top":5}}"#,
    r#"{"type":"Stat","payload":{"path":"C:\src"}}"#,
];

#[test]
fn hostile_numeric_and_pattern_values_do_not_panic_or_exhaust_memory() {
    let h = handler();
    let big_regex = format!("{}a{}", "(".repeat(5000), ")".repeat(5000));
    let requests = vec![
        r#"{"type":"Find","payload":{"pattern":"f","limit":18446744073709551615}}"#.to_string(),
        r#"{"type":"Find","payload":{"pattern":"f","limit":10,"cursor":18446744073709551615}}"#
            .to_string(),
        r#"{"type":"Find","payload":{"pattern":"f","min_size":18446744073709551615,"max_size":0}}"#
            .to_string(),
        r#"{"type":"Find","payload":{"pattern":"f","min_mtime":-9223372036854775808}}"#.to_string(),
        r#"{"type":"Du","payload":{"target":"C:\\","depth":4294967295,"top":18446744073709551615}}"#
            .to_string(),
        format!(
            r#"{{"type":"Find","payload":{{"pattern":{},"is_regex":true,"limit":5}}}}"#,
            serde_json::to_string(&big_regex).unwrap()
        ),
        format!(
            r#"{{"type":"Find","payload":{{"pattern":{},"limit":5}}}}"#,
            serde_json::to_string(&"x".repeat(1 << 20)).unwrap()
        ),
        r#"{"type":"Find","payload":{"pattern":"(?i)(a+)+$","is_regex":true,"limit":5}}"#
            .to_string(),
        r#"{"type":"Stat","payload":{"path":"\u0000\ud800"}}"#.to_string(),
    ];
    for r in requests {
        if let Ok(req) = serde_json::from_str::<IpcRequest>(&r) {
            let _ = h.handle(req);
        }
    }
}

#[test]
fn mutated_requests_never_panic() {
    let h = handler();
    let mut rng = Rng(0xDEAD_BEEF_CAFE_F00D);
    for _ in 0..6000 {
        let seed = SEEDS[rng.below(SEEDS.len())].as_bytes();
        let mut b = seed.to_vec();
        for _ in 0..1 + rng.below(6) {
            match rng.below(4) {
                0 => {
                    let i = rng.below(b.len());
                    b[i] = rng.next() as u8;
                }
                1 => {
                    let i = rng.below(b.len());
                    b.remove(i);
                }
                2 => {
                    let i = rng.below(b.len() + 1);
                    b.insert(i, b"{}[]\":,-0123456789e"[rng.below(19)]);
                }
                _ => {
                    let i = rng.below(b.len());
                    b[i] ^= 1 << rng.below(8);
                }
            }
        }
        if let Ok(req) = serde_json::from_slice::<IpcRequest>(&b) {
            let _ = h.handle(req);
        }
    }
}

#[test]
fn random_bytes_never_panic_in_deserialization() {
    let mut rng = Rng(1);
    for _ in 0..3000 {
        let n = rng.below(200);
        let b: Vec<u8> = (0..n).map(|_| rng.next() as u8).collect();
        let _ = serde_json::from_slice::<IpcRequest>(&b);
    }
    // Aninhamento profundo: serde_json limita a recursão (128) e retorna erro.
    let deep = format!("{}{}", "[".repeat(100_000), "]".repeat(100_000));
    assert!(serde_json::from_str::<IpcRequest>(&deep).is_err());
}
