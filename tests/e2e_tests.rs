use axum_test::TestServer;
use edgewit::api::{AppState, app_router};
use edgewit::index_manager::IndexManager;
use edgewit::indexer::IndexerActor;
use edgewit::registry::IndexRegistry;
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

    let registry = IndexRegistry::new();

    let mut fields = std::collections::HashMap::new();
    fields.insert(
        "message".to_string(),
        edgewit::schema::definition::FieldDefinition {
            field_type: edgewit::schema::definition::FieldType::Text,
            indexed: true,
            stored: false,
            fast: false,
            optional: true,
            default: None,
        },
    );

    fields.insert(
        "optional_tag".to_string(),
        edgewit::schema::definition::FieldDefinition {
            field_type: edgewit::schema::definition::FieldType::Text,
            indexed: true,
            stored: false,
            fast: false,
            optional: true,
            default: None,
        },
    );

    let def = edgewit::schema::definition::IndexDefinition {
        name: "e2e-index".to_string(),
        description: None,
        timestamp_field: "timestamp".to_string(),
        mode: edgewit::schema::definition::SchemaMode::Dynamic,
        partition: edgewit::schema::definition::PartitionStrategy::None,
        retention: None,
        compression: edgewit::schema::definition::CompressionOption::Zstd,
        fields,
        settings: std::collections::HashMap::new(),
    };
    registry.register(def).unwrap();

    let index_manager = IndexManager::new(data_dir.clone(), registry.clone(), 20);

    // 2. Initialize Channels
    let (wal_tx, wal_rx) = mpsc::channel(100);
    let (idx_tx, idx_rx) = mpsc::channel(100);

    // 3. Spawn Indexer Thread
    let indexer = IndexerActor::new(index_manager.clone(), registry.clone(), idx_rx, 15);
    tokio::spawn(async move {
        indexer.run().await;
    });

    // 4. Spawn WAL Thread
    let wal_appender = WalAppender::new(data_dir.clone(), wal_rx, idx_tx, 0);
    tokio::task::spawn_blocking(move || {
        wal_appender.run();
    });

    // 5. Setup AppState
    // In tests we should not use install_recorder if we run multiple tests. But here we have one main test.
    // Try building a stand-alone recorder for axum if possible. If not, install_recorder should be fine in a single test environment.
    // 5. Setup Router and Test Server
    let prometheus_handle = PrometheusBuilder::new().install_recorder().unwrap();
    let state = AppState {
        wal_sender: wal_tx,
        index_manager,
        prometheus_handle,
        registry,
        data_dir,
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

    let sparse_resp = server
        .post("/e2e-index/_doc")
        .json(&json!({"message": "third doc", "optional_tag": "special"}))
        .await;

    sparse_resp.assert_status(axum::http::StatusCode::CREATED);

    // 7. Wait for WAL to flush and Indexer to commit
    // In our implementation, IndexerActor processes elements and commits periodically.
    // The indexer commits every 5 seconds or 10,000 docs.
    // We wait 6 seconds to ensure the interval passes.
    sleep(Duration::from_secs(3)).await;

    // 8. Test Search
    let search_resp = server
        .get("/indexes/e2e-index/_search")
        .add_query_param("q", "message:hello")
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
        .get("/indexes/e2e-index/_search")
        .add_query_param("q", "message:second")
        .await;

    search_resp2.assert_status_ok();
    let search_json2 = search_resp2.json::<serde_json::Value>();
    assert_eq!(search_json2["hits"]["total"]["value"], 1);
    assert_eq!(search_json2["hits"]["hits"][0]["_source"]["level"], "WARN");

    let search_sparse_resp = server
        .get("/indexes/e2e-index/_search")
        .add_query_param("q", "optional_tag:special")
        .await;

    search_sparse_resp.assert_status_ok();
    let search_sparse_json = search_sparse_resp.json::<serde_json::Value>();
    assert_eq!(search_sparse_json["hits"]["total"]["value"], 1);
    assert_eq!(
        search_sparse_json["hits"]["hits"][0]["_source"]["message"],
        "third doc"
    );

    // 8.5 Test Wildcard Search
    let search_all_resp = server
        .get("/indexes/e2e-index/_search")
        .add_query_param("q", "*")
        .await;

    search_all_resp.assert_status_ok();
    let search_all_json = search_all_resp.json::<serde_json::Value>();
    assert_eq!(search_all_json["hits"]["total"]["value"], 3);

    let search_empty_resp = server.get("/indexes/e2e-index/_search").await;

    search_empty_resp.assert_status_ok();
    let search_empty_json = search_empty_resp.json::<serde_json::Value>();
    assert_eq!(search_empty_json["hits"]["total"]["value"], 3);

    // 8.6 Test Stats
    let stats_resp = server.get("/_stats").await;
    stats_resp.assert_status_ok();
    let stats_json = stats_resp.json::<serde_json::Value>();
    assert_eq!(stats_json["_all"]["primaries"]["docs"]["count"], 3);

    // 8.7 Test Cat Indices
    let cat_resp = server.get("/_cat/indexes").await;
    cat_resp.assert_status_ok();
    let cat_json = cat_resp.json::<serde_json::Value>();
    assert_eq!(cat_json[0]["index"], "e2e-index");
    assert_eq!(cat_json[0]["docs.count"], "3");

    // 9. Test Health
    let health_resp = server.get("/_health").await;
    health_resp.assert_status_ok();

    // 10. Test Metrics
    let metrics_resp = server.get("/metrics").await;
    metrics_resp.assert_status_ok();
    // Text output isn't populated unless we installed global recorder, but rendering works based on prometheus logic.
    // Note: since we used build_recorder() and not install_recorder(), global macros inside handlers won't register to THIS handle unless we installed it globally.
    // Since unit tests run concurrently, testing metrics text fully requires care, but we assert it returns 200 OK.

    // 11. Cleanup
    temp_dir.close().unwrap();
}
