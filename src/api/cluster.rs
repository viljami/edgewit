use crate::api::AppState;
use axum::extract::State;
use axum::response::Json;
use serde::Serialize;
use serde_json::{Value, json};
use utoipa::ToSchema;

#[derive(Serialize, ToSchema)]
pub struct HealthResponse {
    pub cluster_name: String,
    pub status: String,
    pub timed_out: bool,
    pub number_of_nodes: u32,
    pub active_primary_shards: u32,
    pub active_shards: u32,
}

#[derive(Serialize, ToSchema)]
pub struct StatsResponse {
    pub _shards: ShardsInfo,
    pub _all: IndicesStats,
}

#[derive(Serialize, ToSchema, Debug)]
pub struct ShardsInfo {
    pub total: u32,
    pub successful: u32,
    pub skipped: u32,
    pub failed: u32,
}

#[derive(Serialize, ToSchema)]
pub struct IndicesStats {
    pub primaries: IndexStats,
    pub total: IndexStats,
}

#[derive(Serialize, ToSchema)]
pub struct IndexStats {
    pub docs: DocsStats,
    pub store: StoreStats,
}

#[derive(Serialize, ToSchema)]
pub struct DocsStats {
    pub count: u64,
    pub deleted: u64,
}

#[derive(Serialize, ToSchema)]
pub struct StoreStats {
    pub size_in_bytes: u64,
}

/// Handler for the root endpoint, emulating the default OpenSearch response
#[utoipa::path(
    get,
    path = "/",
    responses(
        (status = 200, description = "OpenSearch compatible greeting", body = Object)
    )
)]
pub async fn root_handler() -> Json<Value> {
    Json(json!({
        "name": "edgewit-node-1",
        "cluster_name": "edgewit",
        "version": {
            "distribution": "opensearch",
            "number": "2.11.0",
            "build_hash": "unknown",
            "build_date": "2023-10-01T00:00:00Z",
            "build_snapshot": false,
            "lucene_version": "9.7.0",
            "minimum_wire_compatibility_version": "7.10.0",
            "minimum_index_compatibility_version": "7.0.0",
            "build_type": "binary"
        },
        "tagline": "The OpenSearch Project: https://opensearch.org/"
    }))
}

/// Handler for the cluster health endpoint (/_health or /_cluster/health)
#[utoipa::path(
    get,
    path = "/_health",
    responses(
        (status = 200, description = "Cluster health status", body = HealthResponse)
    )
)]
pub async fn health_handler() -> Json<HealthResponse> {
    Json(HealthResponse {
        cluster_name: "edgewit".to_string(),
        status: "green".to_string(), // Initial placeholder: always green
        timed_out: false,
        number_of_nodes: 1,
        active_primary_shards: 0,
        active_shards: 0,
    })
}

/// Handler for the statistics endpoint (/_stats)
#[utoipa::path(
    get,
    path = "/_stats",
    responses(
        (status = 200, description = "Cluster and index statistics", body = StatsResponse)
    )
)]
pub async fn stats_handler(State(state): State<AppState>) -> Json<StatsResponse> {
    let searcher = state.index_reader.searcher();
    let num_docs = searcher.num_docs();
    let num_segments = searcher.segment_readers().len() as u32;

    metrics::gauge!("edgewit_index_docs_total").set(num_docs as f64);
    metrics::gauge!("edgewit_index_segments_total").set(f64::from(num_segments));

    Json(StatsResponse {
        _shards: ShardsInfo {
            total: 0,
            successful: 0,
            skipped: 0,
            failed: 0,
        },
        _all: IndicesStats {
            primaries: IndexStats {
                docs: DocsStats {
                    count: num_docs,
                    deleted: 0,
                },
                store: StoreStats { size_in_bytes: 0 },
            },
            total: IndexStats {
                docs: DocsStats {
                    count: num_docs,
                    deleted: 0,
                },
                store: StoreStats { size_in_bytes: 0 },
            },
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::{AppState, app_router};
    use axum_test::TestServer;
    use insta::assert_json_snapshot;
    use rstest::rstest;
    use tokio::sync::mpsc;

    fn setup_test_server() -> TestServer {
        let (tx, _rx) = mpsc::channel(100);
        let index = tantivy::Index::create_in_ram(crate::indexer::build_schema());
        let state = AppState {
            wal_sender: tx,
            index_reader: index.reader().unwrap(),
            prometheus_handle: metrics_exporter_prometheus::PrometheusBuilder::new()
                .build_recorder()
                .handle(),
        };
        let app = app_router(state);
        TestServer::new(app)
    }

    #[rstest]
    #[tokio::test]
    async fn test_root_handler() {
        let server = setup_test_server();
        let response = server.get("/").await;
        response.assert_status_ok();

        // Using insta for snapshot testing
        assert_json_snapshot!(response.json::<Value>(), @r###"
        {
          "cluster_name": "edgewit",
          "name": "edgewit-node-1",
          "tagline": "The OpenSearch Project: https://opensearch.org/",
          "version": {
            "build_date": "2023-10-01T00:00:00Z",
            "build_hash": "unknown",
            "build_snapshot": false,
            "build_type": "binary",
            "distribution": "opensearch",
            "lucene_version": "9.7.0",
            "minimum_index_compatibility_version": "7.0.0",
            "minimum_wire_compatibility_version": "7.10.0",
            "number": "2.11.0"
          }
        }
        "###);
    }

    #[rstest]
    #[tokio::test]
    async fn test_health_handler() {
        let server = setup_test_server();
        let response = server.get("/_health").await;
        response.assert_status_ok();

        assert_json_snapshot!(response.json::<Value>(), @r###"
        {
          "active_primary_shards": 0,
          "active_shards": 0,
          "cluster_name": "edgewit",
          "number_of_nodes": 1,
          "status": "green",
          "timed_out": false
        }
        "###);
    }
}
