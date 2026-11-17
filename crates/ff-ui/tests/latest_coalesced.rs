use ff_index::{FileIndex, NO_PARENT};
use ff_ui::app::{AsyncSearchResponse, FastFileApp, SearchResultItem};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};

#[test]
fn burst_of_folder_requests_keeps_active_receiver() {
    let mut app = FastFileApp::new();
    app.is_ipc_connected = false;
    let (tx, rx) = std::sync::mpsc::channel();
    app.search_receiver = Some(rx);
    app.search_scanning_path = Some("blocked".into());
    let dir = std::env::temp_dir().join(format!("ff53-burst-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("latest-marker.txt"), b"latest").unwrap();
    for i in 0..500 {
        app.trigger_search_path_scan(&format!("folder-{i}"));
    }
    let latest = dir.to_string_lossy().into_owned();
    app.trigger_search_path_scan(&latest);
    assert_eq!(app.search_target_path, latest);
    app.search_target_path = "unfinished-draft".into();
    assert!(
        tx.send(Err("obsolete error".into())).is_ok(),
        "active scan was abandoned"
    );
    let ctx = egui::Context::default();
    app.poll_path_scan(&ctx);
    assert_eq!(app.search_scanning_path.as_deref(), Some(latest.as_str()));
    assert_eq!(app.search_target_path, "unfinished-draft");
    assert!(!app
        .status_notification
        .as_ref()
        .is_some_and(|(s, _)| s.contains("obsolete error")));
    let deadline = Instant::now() + Duration::from_secs(10);
    while app.search_receiver.is_some() && Instant::now() < deadline {
        app.poll_path_scan(&ctx);
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(app.search_receiver.is_none(), "latest scan did not finish");
    assert_eq!(app.search_target_path, "unfinished-draft");
    assert!(app
        .status_notification
        .as_ref()
        .is_some_and(|(s, _)| s.contains(&latest)));
    assert!(app.local_index.as_ref().unwrap().entries.len() >= 2);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn editing_path_without_submitting_keeps_active_scan_result() {
    let mut app = FastFileApp::new();
    app.is_ipc_connected = false;
    app.search_receiver = None;
    app.trigger_search_path_scan("requested-folder");
    let (tx, rx) = std::sync::mpsc::channel();
    app.search_receiver = Some(rx);
    app.search_target_path = "unfinished-draft".into();
    let mut index = FileIndex::new();
    index
        .add_entry(0, NO_PARENT, "marker.txt", 0, 1, 0)
        .unwrap();
    tx.send(Ok(("requested-folder".into(), index, 1.0)))
        .unwrap();
    let ctx = egui::Context::default();
    app.poll_path_scan(&ctx);
    assert_eq!(app.search_target_path, "unfinished-draft");
    assert_eq!(app.local_index.as_ref().unwrap().entries.len(), 1);
    assert!(app
        .status_notification
        .as_ref()
        .is_some_and(|(s, _)| s.contains("requested-folder")));
    assert!(app.search_result_rx.is_some());
}

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
