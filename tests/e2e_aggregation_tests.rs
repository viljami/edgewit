use axum_test::TestServer;
use edgewit::api::{AppState, app_router};
use edgewit::indexer::{IndexerActor, setup_index};
use edgewit::wal::WalAppender;
use metrics_exporter_prometheus::PrometheusBuilder;
use serde_json::json;

use std::sync::Arc;
use tempfile::TempDir;
use tokio::sync::mpsc;
use tokio::time::{Duration, sleep};

fn get_rfc3339(event_idx: u64, total_events: u64) -> String {
    let total_secs = 181 * 24 * 3600; // 181 days in 2023 (Jan-Jun)
    let offset = (event_idx * total_secs) / total_events;

    let mut remain = offset;
    let sec = remain % 60;
    remain /= 60;
    let min = remain % 60;
    remain /= 60;
    let hour = remain % 24;
    remain /= 24;
    let day_index = remain; // 0 to 180

    let mut d = day_index;
    let months = [31, 28, 31, 30, 31, 30];
    let mut month = 1;
    for &days in &months {
        if d < days {
            break;
        }
        d -= days;
        month += 1;
    }
    let day = d + 1;
    format!(
        "2023-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        month, day, hour, min, sec
    )
}

#[tokio::test]
async fn test_complex_aggregation_search() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter("edgewit=debug")
        .try_init();

    // Commit every second to make test fast
    unsafe { std::env::set_var("EDGEWIT_COMMIT_INTERVAL_SECS", "1") };

    let temp_dir = TempDir::new().unwrap();
    let data_dir = temp_dir.path().to_path_buf();

    // 1. Setup Tantivy Index
    let index = setup_index(&data_dir).expect("Failed to setup Tantivy index");
    let writer = index
        .writer(30_000_000)
        .expect("Failed to create IndexWriter");

    // 2. Initialize Channels
    let (wal_tx, wal_rx) = mpsc::channel(10000);
    let (idx_tx, idx_rx) = mpsc::channel(10000);

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
    let prometheus_handle = PrometheusBuilder::new().build_recorder().handle();

    let state = AppState {
        wal_sender: wal_tx,
        index_reader: index_reader.clone(),
        prometheus_handle,
        registry: edgewit::registry::IndexRegistry::new(),
        data_dir: std::path::PathBuf::from("/tmp"),
    };

    let server = Arc::new(TestServer::new(app_router(state)));

    // 6. Ingest 10000 items rapidly
    let total_events = 10000;

    // Use a LocalSet so we can use spawn_local and avoid Send bounds on the axum TestServer future
    let local = tokio::task::LocalSet::new();
    let server_clone = server.clone();
    local
        .run_until(async move {
            let mut handles = Vec::new();
            for i in 0..total_events {
                let s = server_clone.clone();
                handles.push(tokio::task::spawn_local(async move {
                    let timestamp = get_rfc3339(i, total_events);
                    let amount = 10.0 + (i % 10) as f64; // average is 14.5 exactly
                    let body = json!({
                        "timestamp": timestamp,
                        "amount": amount,
                        "type": if i % 2 == 0 { "even" } else { "odd" }
                    });
                    s.post("/e2e-aggs/_doc").json(&body).await;
                }));
            }
            for h in handles {
                h.await.unwrap();
            }
        })
        .await;

    // 7. Wait for WAL to flush and Indexer to commit
    let mut actual_hits = 0;
    let server_arc = server.clone(); // Re-clone for polling
    for _ in 0..20 {
        sleep(Duration::from_millis(500)).await;

        let search_resp = server_arc.get("/e2e-aggs/_search").await;

        let search_json = search_resp.json::<serde_json::Value>();
        actual_hits = search_json["hits"]["total"]["value"].as_u64().unwrap_or(0);

        if actual_hits == total_events {
            break;
        }
    }

    assert_eq!(
        actual_hits, total_events,
        "Did not index all documents in time!"
    );

    // 8. Test Complex Aggregations
    let aggs_query = json!({
        "size": 0,
        "aggs": {
            "total_sum": { "sum": { "field": "_source.amount" } },
            "avg_amount": { "avg": { "field": "_source.amount" } },
            "sales_per_month": {
                "date_histogram": {
                    "field": "_source.timestamp",
                    "fixed_interval": "30d"
                }
            },
            "sales_per_week": {
                "date_histogram": {
                    "field": "_source.timestamp",
                    "fixed_interval": "7d"
                }
            },
            "sales_per_day": {
                "date_histogram": {
                    "field": "_source.timestamp",
                    "fixed_interval": "1d"
                }
            },
            "sales_per_hour": {
                "date_histogram": {
                    "field": "_source.timestamp",
                    "fixed_interval": "1h"
                }
            }
        }
    });

    let search_resp = server_arc.post("/e2e-aggs/_search").json(&aggs_query).await;

    search_resp.assert_status_ok();
    let res_json = search_resp.json::<serde_json::Value>();

    let aggs = &res_json["aggregations"];

    // Validate Metrics Aggregations
    let total_sum = aggs["total_sum"]["value"].as_f64().unwrap();
    let avg_amount = aggs["avg_amount"]["value"].as_f64().unwrap();

    assert_eq!(total_sum, 145000.0, "Total sum should be exactly 145000.0");
    assert_eq!(avg_amount, 14.5, "Average amount should be exactly 14.5");

    // Validate Histogram Aggregations
    let buckets_month = aggs["sales_per_month"]["buckets"].as_array().unwrap().len();
    let buckets_week = aggs["sales_per_week"]["buckets"].as_array().unwrap().len();
    let buckets_day = aggs["sales_per_day"]["buckets"].as_array().unwrap().len();
    let buckets_hour = aggs["sales_per_hour"]["buckets"].as_array().unwrap().len();

    // 181 days total.
    // 30d buckets: 181/30 = 6 + 1 remainder. Note: depending on alignment with epoch, it might be 7 or 8 buckets. Let's assert it's > 0 to be safe, or print it.
    assert!(
        buckets_month > 5 && buckets_month < 10,
        "Should have roughly 7 monthly buckets, got {}",
        buckets_month
    );
    assert!(
        buckets_week > 24 && buckets_week < 30,
        "Should have roughly 26 weekly buckets, got {}",
        buckets_week
    );
    assert!(
        buckets_day >= 181 && buckets_day <= 183,
        "Should have roughly 181 daily buckets, got {}",
        buckets_day
    );
    // 181 days * 24 hours = 4344
    assert!(
        buckets_hour > 4000 && buckets_hour < 4500,
        "Should have roughly 4344 hourly buckets, got {}",
        buckets_hour
    );
}
