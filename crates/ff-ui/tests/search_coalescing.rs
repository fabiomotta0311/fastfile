use ff_ui::app::{AsyncSearchResponse, FastFileApp};

#[test]
fn rapid_searches_keep_the_inflight_worker_receiver() {
    let mut app = FastFileApp::new();
    app.is_ipc_connected = false;
    let (tx, rx) = std::sync::mpsc::channel();
    app.search_result_rx = Some(rx);
    for i in 0..100 {
        app.query = format!("query-{i}");
        app.trigger_search();
    }
    tx.send(AsyncSearchResponse {
        generation: 0,
        items: vec![],
        total_scanned: 0,
        latency_ms: 0.0,
        error: None,
        fallback_used: false,
        ipc_connected: false,
    })
    .expect("in-flight receiver was replaced");
    assert!(app.search_result_rx.as_ref().unwrap().try_recv().is_ok());
}
