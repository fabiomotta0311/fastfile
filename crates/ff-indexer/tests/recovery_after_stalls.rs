#![cfg(windows)]
use ff_indexer::{IpcClient, IpcRequest};
use std::io::Write;
use std::os::windows::io::FromRawHandle;
use std::time::Duration;
use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
use windows_sys::Win32::Storage::FileSystem::PIPE_ACCESS_DUPLEX;
use windows_sys::Win32::System::Pipes::{
    ConnectNamedPipe, CreateNamedPipeW, PIPE_READMODE_BYTE, PIPE_TYPE_BYTE,
    PIPE_UNLIMITED_INSTANCES, PIPE_WAIT,
};

#[test]
fn healthy_server_works_after_four_unresponsive_connections() {
    let stalled = format!(r"\\.\pipe\ff-stall-recovery-{}", std::process::id());
    let healthy = format!(r"\\.\pipe\ff-healthy-recovery-{}", std::process::id());
    let (ready_tx, ready_rx) = std::sync::mpsc::channel();
    let (release_tx, release_rx) = std::sync::mpsc::channel();
    let name: Vec<u16> = stalled.encode_utf16().chain(Some(0)).collect();
    let server = std::thread::spawn(move || unsafe {
        let mut handles = Vec::new();
        for _ in 0..4 {
            let h = CreateNamedPipeW(
                name.as_ptr(),
                PIPE_ACCESS_DUPLEX,
                PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT,
                PIPE_UNLIMITED_INSTANCES,
                4096,
                4096,
                0,
                std::ptr::null(),
            );
            assert_ne!(h, INVALID_HANDLE_VALUE);
            handles.push(h);
        }
        ready_tx.send(()).unwrap();
        for &h in &handles {
            ConnectNamedPipe(h, std::ptr::null_mut());
        }
        release_rx.recv_timeout(Duration::from_secs(10)).unwrap();
        for h in handles {
            CloseHandle(h);
        }
    });
    ready_rx.recv_timeout(Duration::from_secs(3)).unwrap();
    let mut workers = Vec::new();
    for _ in 0..4 {
        let endpoint = stalled.clone();
        workers.push(std::thread::spawn(move || {
            IpcClient::new(endpoint).send_timeout(&IpcRequest::Ping, Duration::from_millis(300))
        }));
    }
    for worker in workers {
        assert_eq!(
            worker.join().unwrap().unwrap_err().kind(),
            std::io::ErrorKind::TimedOut
        );
    }
    let (ready_tx, ready_rx) = std::sync::mpsc::channel();
    let name: Vec<u16> = healthy.encode_utf16().chain(Some(0)).collect();
    let good = std::thread::spawn(move || unsafe {
        let h = CreateNamedPipeW(
            name.as_ptr(),
            PIPE_ACCESS_DUPLEX,
            PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT,
            1,
            4096,
            4096,
            0,
            std::ptr::null(),
        );
        assert_ne!(h, INVALID_HANDLE_VALUE);
        ready_tx.send(()).unwrap();
        ConnectNamedPipe(h, std::ptr::null_mut());
        let mut file = std::fs::File::from_raw_handle(h as _);
        let mut reader = std::io::BufReader::new(&mut file);
        let mut request = String::new();
        std::io::BufRead::read_line(&mut reader, &mut request).unwrap();
        writeln!(file, "{{\"status\":\"Ok\",\"data\":{{\"pong\":true}}}}").unwrap();
    });
    ready_rx.recv_timeout(Duration::from_secs(3)).unwrap();
    let result = IpcClient::new(healthy).send_timeout(&IpcRequest::Ping, Duration::from_secs(2));
    // Always unblock both servers before asserting so a failing regression terminates.
    release_tx.send(()).unwrap();
    server.join().unwrap();
    if result.is_err() {
        /* connect so the healthy test server can exit */
        // The endpoint is deliberately left untouched: the process can exit with its detached thread.
        drop(good);
    } else {
        good.join().unwrap();
    }
    assert!(
        result.is_ok(),
        "healthy endpoint must recover after timeouts: {result:?}"
    );
}
