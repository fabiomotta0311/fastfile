//! Gravação de snapshot: atômica, sem resíduos e privada.

use ff_index::entry::{FLAG_DIRECTORY, FLAG_FILE, NO_PARENT};
use ff_index::FileIndex;

fn index_with(n: usize) -> FileIndex {
    let mut idx = FileIndex::new();
    idx.volume = "C:".into();
    let root = idx
        .add_entry(1, NO_PARENT, "C:", FLAG_DIRECTORY, 0, 0)
        .unwrap();
    for i in 0..n {
        idx.add_entry(
            10 + i as u64,
            root,
            &format!("f{i}.txt"),
            FLAG_FILE,
            i as u64,
            1,
        )
        .unwrap();
    }
    idx
}

fn temp_dir(tag: &str) -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!("ff-snapsave-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

#[test]
fn save_replaces_existing_file_and_leaves_no_temp() {
    let dir = temp_dir("replace");
    let path = dir.join("idx.ffidx");
    index_with(3).save_snapshot_file(&path).unwrap();
    index_with(50).save_snapshot_file(&path).unwrap();

    let loaded = FileIndex::load_snapshot_file(&path).unwrap();
    assert_eq!(loaded.len(), 51);
    let names: Vec<_> = std::fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(
        names,
        vec!["idx.ffidx"],
        "sobrou arquivo temporário: {names:?}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn failed_save_keeps_previous_snapshot_intact() {
    let dir = temp_dir("fail");
    let path = dir.join("idx.ffidx");
    index_with(5).save_snapshot_file(&path).unwrap();
    let before = std::fs::read(&path).unwrap();

    // Destino inexistente: a gravação falha e não pode tocar no snapshot antigo.
    let missing = dir.join("nao-existe").join("idx.ffidx");
    assert!(index_with(9).save_snapshot_file(&missing).is_err());
    assert_eq!(std::fs::read(&path).unwrap(), before);
    assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1);
    let _ = std::fs::remove_dir_all(&dir);
}

#[cfg(unix)]
#[test]
fn snapshot_file_is_user_only() {
    use std::os::unix::fs::PermissionsExt;
    let dir = temp_dir("mode");
    let path = dir.join("idx.ffidx");
    index_with(2).save_snapshot_file(&path).unwrap();
    assert_eq!(
        std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        0o600
    );
    let _ = std::fs::remove_dir_all(&dir);
}
