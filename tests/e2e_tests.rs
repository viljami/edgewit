use axum_test::TestServer;
use edgewit::api::{AppState, app_router};
use edgewit::indexer::{IndexerActor, setup_index};
use edgewit::wal::WalAppender;
use metrics_exporter_prometheus::PrometheusBuilder;
use serde_json::json;

use tempfile::TempDir;
use tokio::sync::mpsc;
use tokio::time::{Duration, sleep};

#[tokio::test]
async fn test_full_ingest_and_search_flow() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter("edgewit=debug")
        .try_init();
    unsafe { std::env::set_var("EDGEWIT_COMMIT_INTERVAL_SECS", "1") };
    let temp_dir = TempDir::new().unwrap();
    let data_dir = temp_dir.path().to_path_buf();

    // 1. Setup Tantivy Index
    let index = setup_index(&data_dir).expect("Failed to setup Tantivy index");
    let writer = index
        .writer(15_000_000)
        .expect("Failed to create IndexWriter");

    // 2. Initialize Channels
    let (wal_tx, wal_rx) = mpsc::channel(100);
    let (idx_tx, idx_rx) = mpsc::channel(100);

    // 3. Spawn Indexer Thread
    let indexer = IndexerActor::new(writer, index.schema(), idx_rx);
    tokio::spawn(async move {
        indexer.run().await;
    });

    // 4. Spawn WAL Thread
    let wal_appender = WalAppender::new(data_dir.clone(), wal_rx, idx_tx, 0);
    tokio::task::spawn_blocking(move || {
        wal_appender.run();
    });

    // 5. Setup AppState
    let index_reader = index.reader().unwrap();
    // In tests we should not use install_recorder if we run multiple tests. But here we have one main test.
    // Try building a stand-alone recorder for axum if possible. If not, install_recorder should be fine in a single test environment.
    let prometheus_handle = PrometheusBuilder::new().build_recorder().handle();

    let state = AppState {
        wal_sender: wal_tx,
        index_reader: index_reader.clone(),
        prometheus_handle,
        registry: edgewit::registry::IndexRegistry::new(),
        data_dir: std::path::PathBuf::from("/tmp"),
    };

    let server = TestServer::new(app_router(state));

    // 6. Test Ingest
    let ingest_resp = server
        .post("/e2e-index/_doc")
        .json(&json!({
            "message": "hello e2e world",
            "level": "INFO",
            "user_id": 42
        }))
        .await;

    ingest_resp.assert_status(axum::http::StatusCode::CREATED);

    let bulk_resp = server
        .post("/e2e-index/_doc")
        .json(&json!({"message": "second doc", "level": "WARN"}))
        .await;

    bulk_resp.assert_status(axum::http::StatusCode::CREATED);

    // 7. Wait for WAL to flush and Indexer to commit
    // In our implementation, IndexerActor processes elements and commits periodically.
    // The indexer commits every 5 seconds or 10,000 docs.
    // We wait 6 seconds to ensure the interval passes.
    sleep(Duration::from_secs(3)).await;

    // 8. Test Search
    let search_resp = server
        .get("/_search")
        .add_query_param("q", "_source.message:hello")
        .await;

    search_resp.assert_status_ok();
    let search_json = search_resp.json::<serde_json::Value>();
    println!(
        "Search result: {}",
        serde_json::to_string_pretty(&search_json).unwrap()
    );
    assert_eq!(search_json["hits"]["total"]["value"], 1);
    assert_eq!(
        search_json["hits"]["hits"][0]["_source"]["message"],
        "hello e2e world"
    );

    let search_resp2 = server
        .get("/e2e-index/_search")
        .add_query_param("q", "_source.message:second")
        .await;

    search_resp2.assert_status_ok();
    let search_json2 = search_resp2.json::<serde_json::Value>();
    assert_eq!(search_json2["hits"]["total"]["value"], 1);
    assert_eq!(search_json2["hits"]["hits"][0]["_source"]["level"], "WARN");

    // 9. Test Health
    let health_resp = server.get("/_health").await;
    health_resp.assert_status_ok();

    // 10. Test Metrics
    let metrics_resp = server.get("/metrics").await;
    metrics_resp.assert_status_ok();
    // Text output isn't populated unless we installed global recorder, but rendering works based on prometheus logic.
    // Note: since we used build_recorder() and not install_recorder(), global macros inside handlers won't register to THIS handle unless we installed it globally.
    // Since unit tests run concurrently, testing metrics text fully requires care, but we assert it returns 200 OK.
}
