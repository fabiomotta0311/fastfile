//! Reconstrução de caminhos: raiz do NTFS (auto-pai) e ciclos reais.

use ff_index::entry::{FLAG_DIRECTORY, FLAG_FILE, NO_PARENT};
use ff_index::{FileIndex, IndexError};
use std::time::{Duration, Instant};

#[test]
fn self_parent_root_ends_the_path_instead_of_cycling() {
    // Registro 5 do NTFS: a raiz aponta para si mesma.
    let mut idx = FileIndex::new();
    let root = idx
        .add_entry(5, NO_PARENT, "C:", FLAG_DIRECTORY, 0, 0)
        .unwrap();
    idx.entries[root as usize].parent = root;
    let dir = idx
        .add_entry(10, root, "Users", FLAG_DIRECTORY, 0, 0)
        .unwrap();
    let file = idx.add_entry(11, dir, "a.txt", FLAG_FILE, 1, 0).unwrap();
    assert_eq!(idx.build_path(file).unwrap(), r"C:\Users\a.txt");
    assert_eq!(idx.build_path(root).unwrap(), "C:");
}

#[test]
fn real_cycle_fails_fast_even_in_a_huge_index() {
    let mut idx = FileIndex::new();
    let a = idx
        .add_entry(1, NO_PARENT, "a", FLAG_DIRECTORY, 0, 0)
        .unwrap();
    let b = idx.add_entry(2, a, "b", FLAG_DIRECTORY, 0, 0).unwrap();
    idx.entries[a as usize].parent = b; // a -> b -> a
    for i in 0..200_000u64 {
        idx.add_entry(100 + i, NO_PARENT, "x", FLAG_FILE, 0, 0)
            .unwrap();
    }
    let t = Instant::now();
    for _ in 0..200 {
        assert!(matches!(
            idx.build_path(b),
            Err(IndexError::CycleDetected(_))
        ));
    }
    assert!(
        t.elapsed() < Duration::from_secs(2),
        "ciclo demorou {:?}",
        t.elapsed()
    );
}
