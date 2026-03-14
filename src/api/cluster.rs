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

#[derive(Serialize, ToSchema)]
pub struct CatIndex {
    pub health: String,
    pub status: String,
    pub index: String,
    pub uuid: String,
    pub pri: String,
    pub rep: String,
    #[serde(rename = "docs.count")]
    pub docs_count: String,
    #[serde(rename = "docs.deleted")]
    pub docs_deleted: String,
    #[serde(rename = "store.size")]
    pub store_size: String,
    #[serde(rename = "pri.store.size")]
    pub pri_store_size: String,
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
pub async fn stats_handler(State(_state): State<AppState>) -> Json<StatsResponse> {
    // TODO: Calculate from index manager per partition
    let num_docs = 0;
    let num_segments = 0;

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

/// Handler for the cat indexes endpoint (/_cat/indexes)
#[utoipa::path(
    get,
    path = "/_cat/indexes",
    responses(
        (status = 200, description = "List of indexes with stats", body = Vec<CatIndex>)
    )
)]
pub async fn cat_indexes_handler(State(state): State<AppState>) -> Json<Vec<CatIndex>> {
    // TODO: Calculate from index manager per partition
    let num_docs = 0;

    let mut indices = Vec::new();
    let registered = state.registry.list();

    if registered.is_empty() {
        indices.push(CatIndex {
            health: "green".to_string(),
            status: "open".to_string(),
            index: "edgewit".to_string(),
            uuid: "unknown".to_string(),
            pri: "1".to_string(),
            rep: "0".to_string(),
            docs_count: num_docs.to_string(),
            docs_deleted: "0".to_string(),
            store_size: "0b".to_string(),
            pri_store_size: "0b".to_string(),
        });
    } else {
        for def in registered {
            indices.push(CatIndex {
                health: "green".to_string(),
                status: "open".to_string(),
                index: def.name,
                uuid: "unknown".to_string(),
                pri: "1".to_string(),
                rep: "0".to_string(),
                docs_count: num_docs.to_string(), // Approximation since we use a monolithic index for now
                docs_deleted: "0".to_string(),
                store_size: "0b".to_string(),
                pri_store_size: "0b".to_string(),
            });
        }
    }

    Json(indices)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::{AppState, app_router};
    use crate::registry::IndexRegistry;
    use axum_test::TestServer;
    use insta::assert_json_snapshot;
    use rstest::rstest;
    use std::path::PathBuf;
    use tokio::sync::mpsc;

    fn setup_test_server() -> TestServer {
        let (tx, _rx) = mpsc::channel(100);
        let state = AppState {
            wal_sender: tx,
            index_manager: crate::index_manager::IndexManager::new(
                PathBuf::from("/tmp"),
                IndexRegistry::new(),
                20,
            ),
            prometheus_handle: metrics_exporter_prometheus::PrometheusBuilder::new()
                .build_recorder()
                .handle(),
            registry: IndexRegistry::new(),
            data_dir: PathBuf::from("."),
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
