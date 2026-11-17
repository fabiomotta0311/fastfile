#![cfg(windows)]
use ff_indexer::{IpcClient, IpcRequest};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use std::time::Duration;
use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
use windows_sys::Win32::Storage::FileSystem::PIPE_ACCESS_DUPLEX;
use windows_sys::Win32::System::Pipes::{
    ConnectNamedPipe, CreateNamedPipeW, PIPE_READMODE_BYTE, PIPE_TYPE_BYTE,
    PIPE_UNLIMITED_INSTANCES, PIPE_WAIT,
};

#[test]
fn stalled_daemon_cannot_accumulate_unbounded_ipc_workers() {
    let endpoint = format!(r"\\.\pipe\fastfile-stalled-{}", std::process::id());
    let name: Vec<u16> = endpoint.encode_utf16().chain(Some(0)).collect();
    let accepted = Arc::new(AtomicUsize::new(0));
    let server_count = accepted.clone();
    let (ready_tx, ready_rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || unsafe {
        let mut handles = Vec::new();
        for i in 0..16 {
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
            if i == 0 {
                ready_tx.send(()).unwrap();
            }
            ConnectNamedPipe(h, std::ptr::null_mut());
            server_count.fetch_add(1, Ordering::SeqCst);
            handles.push(h);
        }
        std::thread::sleep(Duration::from_secs(5));
        for h in handles {
            CloseHandle(h);
        }
    });
    ready_rx.recv_timeout(Duration::from_secs(2)).unwrap();
    let client = IpcClient::new(endpoint);
    for _ in 0..12 {
        assert!(client
            .send_timeout(&IpcRequest::Ping, Duration::from_millis(50))
            .is_err());
    }
    std::thread::sleep(Duration::from_millis(100));
    assert!(
        accepted.load(Ordering::SeqCst) <= 4,
        "stalled connections accumulated: {}",
        accepted.load(Ordering::SeqCst)
    );
}
