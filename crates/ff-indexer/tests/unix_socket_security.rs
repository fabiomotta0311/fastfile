#![cfg(unix)]
use std::io::{BufRead, BufReader, Write};
use std::os::unix::fs::{FileTypeExt, PermissionsExt};
use std::os::unix::net::UnixStream;

use ff_indexer::unix_socket::bind_private;

fn fresh_dir(tag: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("fastfile-sock-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn socket_is_user_only_and_usable() {
    let dir = fresh_dir("mode");
    let path = dir.join("ff.sock");
    let listener = bind_private(&path).unwrap();

    let meta = std::fs::symlink_metadata(&path).unwrap();
    assert!(meta.file_type().is_socket());
    assert_eq!(meta.permissions().mode() & 0o777, 0o600);

    let server = std::thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        let mut line = String::new();
        BufReader::new(stream.try_clone().unwrap())
            .read_line(&mut line)
            .unwrap();
        let mut stream = stream;
        writeln!(stream, "pong:{}", line.trim()).unwrap();
    });
    let mut client = UnixStream::connect(&path).unwrap();
    writeln!(client, "ping").unwrap();
    let mut reply = String::new();
    BufReader::new(client).read_line(&mut reply).unwrap();
    assert_eq!(reply.trim(), "pong:ping");
    server.join().unwrap();
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn staging_dir_is_cleaned_and_stale_socket_replaced() {
    let dir = fresh_dir("stale");
    let path = dir.join("ff.sock");
    std::fs::write(&path, b"stale").unwrap();
    let _listener = bind_private(&path).unwrap();
    assert!(std::fs::symlink_metadata(&path)
        .unwrap()
        .file_type()
        .is_socket());
    let leftovers: Vec<_> = std::fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .collect();
    assert_eq!(
        leftovers.len(),
        1,
        "sobrou diretório de preparo: {leftovers:?}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn symlink_at_final_path_is_replaced_not_followed() {
    let dir = fresh_dir("link");
    let victim = dir.join("victim");
    std::fs::write(&victim, b"keep").unwrap();
    let path = dir.join("ff.sock");
    std::os::unix::fs::symlink(&victim, &path).unwrap();
    let _listener = bind_private(&path).unwrap();
    assert_eq!(std::fs::read(&victim).unwrap(), b"keep");
    assert!(std::fs::symlink_metadata(&path)
        .unwrap()
        .file_type()
        .is_socket());
    let _ = std::fs::remove_dir_all(&dir);
}
