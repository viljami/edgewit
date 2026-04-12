use criterion::{Criterion, criterion_group, criterion_main};
use serde_json::json;
use std::collections::HashMap;
use std::time::Duration;
use tempfile::TempDir;
use tokio::sync::mpsc;

use axum_test::TestServer;

use edgewit::api::{AppState, app_router};
use edgewit::index_manager::IndexManager;
use edgewit::indexer::{IndexerActor, PurgeCommand};
use edgewit::registry::IndexRegistry;
use edgewit::schema::definition::{
    CompressionOption, IndexDefinition, PartitionStrategy, SchemaMode,
};
use edgewit::wal::WalAppender;
use metrics_exporter_prometheus::PrometheusBuilder;

/// Returns a minimal index definition suitable for benchmarking.
/// Uses `PartitionStrategy::None` and an empty fields map so no timestamp
/// field is required and validation always passes.
fn bench_index_def(name: &str) -> IndexDefinition {
    IndexDefinition {
        name: name.to_string(),
        description: None,
        timestamp_field: "timestamp".to_string(),
        mode: SchemaMode::Dynamic,
        partition: PartitionStrategy::None,
        retention: None,
        compression: CompressionOption::Zstd,
        fields: HashMap::new(),
        settings: HashMap::new(),
    }
}

async fn setup_app() -> (TestServer, TempDir) {
    let temp_dir = TempDir::new().unwrap();
    let data_dir = temp_dir.path().to_path_buf();

    let registry = IndexRegistry::new();

    // The indexer looks up the registry for every ingest event; indexes must be
    // pre-registered or every document will be silently dropped with an error log.
    registry.upsert(bench_index_def("bench-index")).unwrap();
    registry.upsert(bench_index_def("search-bench")).unwrap();

    let index_manager = IndexManager::new(data_dir.clone(), registry.clone(), 20);

    let (wal_tx, wal_rx) = mpsc::channel(10000);
    let (idx_tx, idx_rx) = mpsc::channel(10000);

    // IndexerActor now owns a purge channel as well; create a dummy receiver.
    let (_purge_tx, purge_rx) = mpsc::channel::<PurgeCommand>(32);

    let indexer = IndexerActor::new(
        index_manager.clone(),
        registry.clone(),
        idx_rx,
        30,
        purge_rx,
    );
    tokio::spawn(async move {
        indexer.run().await;
    });

    let wal_appender = WalAppender::new(data_dir.clone(), wal_rx, idx_tx, 0);
    tokio::task::spawn_blocking(move || {
        wal_appender.run();
    });

    let prometheus_handle = PrometheusBuilder::new().build_recorder().handle();

    let state = AppState {
        wal_sender: wal_tx,
        index_manager,
        prometheus_handle,
        registry,
        data_dir,
    };

    let server = TestServer::new(app_router(state));

    (server, temp_dir)
}

fn bench_ingest(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let (server, _dir) = rt.block_on(setup_app());

    let mut group = c.benchmark_group("ingest");

    // Generate a 1000-document bulk payload
    let mut bulk_payload = String::new();
    for i in 0..1000 {
        bulk_payload.push_str(&json!({ "index": { "_index": "bench-index" } }).to_string());
        bulk_payload.push('\n');
        bulk_payload.push_str(&json!({ "message": "bulk log entry", "value": i }).to_string());
        bulk_payload.push('\n');
    }
    let body_bytes = bytes::Bytes::from(bulk_payload);

    group.throughput(criterion::Throughput::Elements(1000));

    group.bench_function("bulk_1000_docs", |b| {
        b.to_async(&rt).iter(|| async {
            let resp = server
                .post("/_bulk")
                .add_header("content-type", "application/x-ndjson")
                .bytes(body_bytes.clone())
                .await;
            resp.assert_status_ok();
        })
    });

    group.finish();
}

fn bench_search(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let (server, _dir) = rt.block_on(setup_app());

    // Pre-populate 10k documents
    rt.block_on(async {
        let mut bulk_payload = String::new();
        for i in 0..10000 {
            let timestamp = format!("2023-01-{:02}T12:00:00Z", (i % 28) + 1);
            bulk_payload.push_str(&json!({ "index": { "_index": "search-bench" } }).to_string());
            bulk_payload.push('\n');
            bulk_payload.push_str(
                &json!({
                    "message": "hello world",
                    "amount": i as f64,
                    "timestamp": timestamp
                })
                .to_string(),
            );
            bulk_payload.push('\n');
        }
        let resp = server
            .post("/_bulk")
            .add_header("content-type", "application/x-ndjson")
            .bytes(bytes::Bytes::from(bulk_payload))
            .await;
        resp.assert_status_ok();

        // Wait for the indexer to commit the batch
        tokio::time::sleep(Duration::from_secs(6)).await;
    });

    let mut group = c.benchmark_group("search");

    let match_all_query = json!({
        "size": 10,
        "query": { "match_all": {} }
    });

    group.bench_function("match_all", |b| {
        b.to_async(&rt).iter(|| async {
            let resp = server
                .post("/indexes/search-bench/_search")
                .json(&match_all_query)
                .await;
            resp.assert_status_ok();
        })
    });

    // Aggregation fields are prefixed with `_source.` because all document
    // data is stored inside the JSON `_source` field by the indexer.
    let aggs_query = json!({
        "size": 0,
        "aggs": {
            "sum_amount": { "sum": { "field": "_source.amount" } },
            "daily_sales": {
                "date_histogram": {
                    "field": "_source.timestamp",
                    "fixed_interval": "1d"
                }
            }
        }
    });

    group.bench_function("aggregations", |b| {
        b.to_async(&rt).iter(|| async {
            let resp = server
                .post("/indexes/search-bench/_search")
                .json(&aggs_query)
                .await;
            resp.assert_status_ok();
        })
    });

    group.finish();
}

criterion_group! {
    name = benches;
    config = Criterion::default().sample_size(10);
    targets = bench_ingest, bench_search
}
criterion_main!(benches);
