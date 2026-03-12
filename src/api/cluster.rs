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

#[derive(Serialize, ToSchema)]
pub struct ShardsInfo {
    pub total: u32,
    pub successful: u32,
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
            "distribution": "edgewit",
            "number": "0.1.0",
            "build_type": "binary"
        },
        "tagline": "You Know, for Edge"
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
pub async fn stats_handler() -> Json<StatsResponse> {
    Json(StatsResponse {
        _shards: ShardsInfo {
            total: 0,
            successful: 0,
            failed: 0,
        },
        _all: IndicesStats {
            primaries: IndexStats {
                docs: DocsStats {
                    count: 0,
                    deleted: 0,
                },
                store: StoreStats { size_in_bytes: 0 },
            },
            total: IndexStats {
                docs: DocsStats {
                    count: 0,
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
        let state = AppState { wal_sender: tx };
        let app = app_router(state);
        TestServer::new(app).unwrap()
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
          "tagline": "You Know, for Edge",
          "version": {
            "build_type": "binary",
            "distribution": "edgewit",
            "number": "0.1.0"
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
