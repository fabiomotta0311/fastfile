#![cfg(windows)]
use std::time::Duration;
use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
use windows_sys::Win32::Storage::FileSystem::{FILE_FLAG_FIRST_PIPE_INSTANCE, PIPE_ACCESS_DUPLEX};
use windows_sys::Win32::System::Pipes::{
    ConnectNamedPipe, CreateNamedPipeW, PIPE_READMODE_BYTE, PIPE_TYPE_BYTE, PIPE_WAIT,
};

#[test]
fn startup_returns_while_daemon_accepts_without_replying() {
    let endpoint = ff_indexer::default_endpoint_name();
    let name: Vec<u16> = endpoint.encode_utf16().chain(Some(0)).collect();
    // SAFETY: o nome do pipe é UTF-16 terminado em NUL e vive durante a chamada.
    let handle = unsafe {
        CreateNamedPipeW(
            name.as_ptr(),
            PIPE_ACCESS_DUPLEX | FILE_FLAG_FIRST_PIPE_INSTANCE,
            PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT,
            1,
            4096,
            4096,
            0,
            std::ptr::null(),
        )
    };
    assert_ne!(
        handle, INVALID_HANDLE_VALUE,
        "test requires no real daemon running"
    );
    // Pass as integer because raw handles are not Send.
    let raw = handle as usize;
    // SAFETY: `raw` é o handle do pipe criado acima; só esta thread o usa e o fecha uma vez.
    std::thread::spawn(move || unsafe {
        let h = raw as *mut core::ffi::c_void;
        ConnectNamedPipe(h, std::ptr::null_mut());
        std::thread::sleep(Duration::from_secs(3));
        CloseHandle(h);
    });
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _app = ff_ui::app::FastFileApp::new();
        tx.send(()).unwrap();
    });
    assert!(
        rx.recv_timeout(Duration::from_millis(500)).is_ok(),
        "constructor waited on daemon or local indexing"
    );
}
