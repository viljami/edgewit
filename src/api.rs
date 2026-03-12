use axum::{
    Router,
    extract::{Path, State},
    http::StatusCode,
    response::Json,
    routing::{get, post},
};
use bytes::Bytes;
use serde::Serialize;
use serde_json::{Value, json};
use tokio::sync::oneshot;
use utoipa::{OpenApi, ToSchema};

use crate::wal::{IngestEvent, WalRequest};

#[derive(Clone)]
pub struct AppState {
    pub wal_sender: tokio::sync::mpsc::Sender<WalRequest>,
}

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

/// Handler for ingesting a single document
#[utoipa::path(
    post,
    path = "/{index}/_doc",
    responses(
        (status = 201, description = "Document ingested successfully", body = Object),
        (status = 500, description = "Failed to write to WAL")
    ),
    params(
        ("index" = String, Path, description = "Index name to ingest into")
    )
)]
pub async fn ingest_doc_handler(
    State(state): State<AppState>,
    Path(index): Path<String>,
    body: Bytes,
) -> Result<(StatusCode, Json<Value>), (StatusCode, String)> {
    let (tx, rx) = oneshot::channel();

    let req = WalRequest {
        event: IngestEvent {
            index: index.clone(),
            payload: body.to_vec(),
        },
        responder: tx,
    };

    if state.wal_sender.send(req).await.is_err() {
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            "WAL channel closed".to_string(),
        ));
    }

    match rx.await {
        Ok(Ok(_)) => Ok((
            StatusCode::CREATED,
            Json(json!({
                "_index": index,
                "result": "created",
                "_shards": {
                    "total": 1,
                    "successful": 1,
                    "failed": 0
                }
            })),
        )),
        Ok(Err(e)) => Err((StatusCode::INTERNAL_SERVER_ERROR, e)),
        Err(_) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            "WAL responder dropped".to_string(),
        )),
    }
}

/// Handler for OpenSearch compatible bulk ingestion
#[utoipa::path(
    post,
    path = "/_bulk",
    responses(
        (status = 200, description = "Bulk documents ingested successfully", body = Object),
        (status = 500, description = "Failed to write to WAL")
    )
)]
pub async fn bulk_handler(
    State(state): State<AppState>,
    body: Bytes,
) -> Result<(StatusCode, Json<Value>), (StatusCode, String)> {
    // For M1, we accept the raw bulk body, assume a generic index like "default",
    // and write the entire payload. In M2 we'll parse the NDJSON properly.
    let index = "default".to_string();

    let (tx, rx) = oneshot::channel();

    let req = WalRequest {
        event: IngestEvent {
            index: index.clone(),
            payload: body.to_vec(),
        },
        responder: tx,
    };

    if state.wal_sender.send(req).await.is_err() {
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            "WAL channel closed".to_string(),
        ));
    }

    match rx.await {
        Ok(Ok(_)) => {
            Ok((
                StatusCode::OK,
                Json(json!({
                    "took": 1,
                    "errors": false,
                    "items": [] // Simplified for M1
                })),
            ))
        }
        Ok(Err(e)) => Err((StatusCode::INTERNAL_SERVER_ERROR, e)),
        Err(_) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            "WAL responder dropped".to_string(),
        )),
    }
}

// Generate the OpenAPI schema from the handlers and structs
#[derive(OpenApi)]
#[openapi(
    paths(
        root_handler,
        health_handler,
        stats_handler,
        ingest_doc_handler,
        bulk_handler
    ),
    components(schemas(
        HealthResponse,
        StatsResponse,
        ShardsInfo,
        IndicesStats,
        IndexStats,
        DocsStats,
        StoreStats
    )),
    info(
        title = "Edgewit API",
        description = "Lightweight, Rust-based search and analytics engine for edge environments.",
        version = "0.1.0"
    )
)]
pub struct ApiDoc;

pub fn app_router(state: AppState) -> Router {
    Router::new()
        .route("/", get(root_handler))
        .route("/_health", get(health_handler))
        .route("/_cluster/health", get(health_handler)) // OpenSearch alias
        .route("/_stats", get(stats_handler))
        .route("/_bulk", post(bulk_handler))
        .route("/:index/_doc", post(ingest_doc_handler))
        .with_state(state)
}
