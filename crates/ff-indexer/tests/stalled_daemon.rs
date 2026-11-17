#![cfg(windows)]
use ff_indexer::{IpcClient, IpcRequest};
use std::io::{BufRead, BufReader, Read};
use std::os::windows::io::FromRawHandle;
use std::time::Duration;
use windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE;
use windows_sys::Win32::Storage::FileSystem::PIPE_ACCESS_DUPLEX;
use windows_sys::Win32::System::Pipes::{
    ConnectNamedPipe, CreateNamedPipeW, PIPE_READMODE_BYTE, PIPE_TYPE_BYTE,
    PIPE_UNLIMITED_INSTANCES, PIPE_WAIT,
};

#[test]
fn stalled_daemon_cannot_accumulate_unbounded_ipc_workers() {
    let endpoint = format!(r"\\.\pipe\fastfile-stalled-{}", std::process::id());
    let name: Vec<u16> = endpoint.encode_utf16().chain(Some(0)).collect();
    let (ready_tx, ready_rx) = std::sync::mpsc::channel();
    let server = std::thread::spawn(move || unsafe {
        for _ in 0..12 {
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
            ready_tx.send(()).unwrap();
            ConnectNamedPipe(h, std::ptr::null_mut());
            let mut file = std::fs::File::from_raw_handle(h as _);
            let mut reader = BufReader::new(&mut file);
            let mut request = String::new();
            reader.read_line(&mut request).unwrap();
            assert!(!request.is_empty());
            match reader.read(&mut [0]) {
                Ok(0) => (),
                Err(e) if e.kind() == std::io::ErrorKind::BrokenPipe => (),
                other => panic!("client did not close: {other:?}"),
            }
        }
    });
    let client = IpcClient::new(endpoint);
    for _ in 0..12 {
        ready_rx.recv_timeout(Duration::from_secs(3)).unwrap();
        assert_eq!(
            client
                .send_timeout(&IpcRequest::Ping, Duration::from_millis(50))
                .unwrap_err()
                .kind(),
            std::io::ErrorKind::TimedOut
        );
    }
    server.join().unwrap();
}
