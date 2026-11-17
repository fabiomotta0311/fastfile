#![cfg(unix)]
use ff_indexer::{IpcClient, IpcRequest, IpcResponse};
use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::net::UnixListener;
use std::time::Duration;

#[test]
fn healthy_server_recovers_after_four_stalls() {
    let dir = std::env::temp_dir().join(format!("fastfile-ipc-recovery-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let stalled_path = dir.join("stalled.sock");
    let healthy_path = dir.join("healthy.sock");
    let stalled = UnixListener::bind(&stalled_path).unwrap();
    let healthy = UnixListener::bind(&healthy_path).unwrap();
    let server = std::thread::spawn(move || {
        for _ in 0..4 {
            let (stream, _) = stalled.accept().unwrap();
            let mut reader = BufReader::new(stream);
            let mut request = String::new();
            reader.read_line(&mut request).unwrap();
            assert!(!request.is_empty());
            let mut byte = [0];
            assert_eq!(
                reader.read(&mut byte).unwrap(),
                0,
                "timed-out client must close"
            );
        }
    });
    let client = IpcClient::new(stalled_path.to_string_lossy());
    for _ in 0..4 {
        assert_eq!(
            client
                .send_timeout(&IpcRequest::Ping, Duration::from_millis(100))
                .unwrap_err()
                .kind(),
            std::io::ErrorKind::TimedOut
        );
    }
    server.join().unwrap();
    let good = std::thread::spawn(move || {
        let (mut stream, _) = healthy.accept().unwrap();
        let mut request = String::new();
        BufReader::new(&mut stream).read_line(&mut request).unwrap();
        writeln!(stream, "{{\"status\":\"Ok\",\"data\":{{\"pong\":true}}}}").unwrap();
    });
    let response = IpcClient::new(healthy_path.to_string_lossy())
        .send_timeout(&IpcRequest::Ping, Duration::from_secs(2))
        .unwrap();
    assert!(matches!(response, IpcResponse::Ok(value) if value["pong"] == true));
    good.join().unwrap();
    std::fs::remove_dir_all(dir).unwrap();
}
