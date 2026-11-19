#![no_main]
use std::sync::{Arc, OnceLock, RwLock};

use ff_index::{FileIndex, FLAG_DIRECTORY, FLAG_FILE, NO_PARENT};
use ff_indexer::{IpcRequest, RequestHandler};
use libfuzzer_sys::fuzz_target;

fn handler() -> &'static RequestHandler {
    static H: OnceLock<RequestHandler> = OnceLock::new();
    H.get_or_init(|| {
        let mut idx = FileIndex::new();
        let root = if cfg!(windows) { "C:" } else { "/volume" };
        idx.volume = root.to_string();
        let r = idx
            .add_entry(0, NO_PARENT, root, FLAG_DIRECTORY, 0, 0)
            .unwrap();
        for i in 0..32u64 {
            idx.add_entry(10 + i, r, &format!("file{i}.rs"), FLAG_FILE, i, 1)
                .unwrap();
        }
        RequestHandler::new(Arc::new(RwLock::new(idx)))
    })
}

fuzz_target!(|data: &[u8]| {
    if let Ok(req) = serde_json::from_slice::<IpcRequest>(data) {
        let _ = handler().handle(req);
    }
});
