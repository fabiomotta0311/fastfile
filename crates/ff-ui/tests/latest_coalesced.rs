use ff_index::{FileIndex, NO_PARENT};
use ff_ui::app::{AsyncSearchResponse, FastFileApp, SearchResultItem};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};

#[test]
fn latest_coalesced_query_runs_after_stale_response() {
    let mut app = FastFileApp::new();
    let mut index = FileIndex::new();
    let root = index
        .add_entry(0, NO_PARENT, "root", ff_index::FLAG_DIRECTORY, 0, 0)
        .unwrap();
    index
        .add_entry(1, root, "latest_unique.txt", 0, 1, 0)
        .unwrap();
    index
        .add_entry(2, root, "intermediate.txt", 0, 1, 0)
        .unwrap();
    app.local_index = Some(Arc::new(index));
    app.is_ipc_connected = false;
    let (tx, rx) = std::sync::mpsc::channel();
    app.search_result_rx = Some(rx);
    app.query = "intermediate".into();
    app.trigger_search();
    app.query = "latest_unique".into();
    app.trigger_search();
    tx.send(AsyncSearchResponse {
        generation: 0,
        items: vec![SearchResultItem {
            id: 10,
            name: "stale".into(),
            path: "stale".into(),
            size: 0,
            mtime: 0,
            is_dir: false,
        }],
        total_scanned: 0,
        latency_ms: 0.0,
        error: None,
        fallback_used: false,
        ipc_connected: false,
    })
    .unwrap();
    let ctx = egui::Context::default();
    app.poll_search_response(&ctx);
    assert!(!app.results.iter().any(|item| item.name == "stale"));
    let deadline = Instant::now() + Duration::from_secs(5);
    while app.is_searching && Instant::now() < deadline {
        app.poll_search_response(&ctx);
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(!app.is_searching, "latest search did not complete");
    assert_eq!(
        app.results
            .iter()
            .map(|item| item.name.as_str())
            .collect::<Vec<_>>(),
        vec!["latest_unique.txt"]
    );
}
