pub mod cluster;
pub mod ingest;

use axum::{
    Router,
    routing::{get, post},
};
use utoipa::OpenApi;

pub use cluster::*;
pub use ingest::*;

use crate::wal::WalRequest;

#[derive(Clone)]
pub struct AppState {
    pub wal_sender: tokio::sync::mpsc::Sender<WalRequest>,
}

// Generate the OpenAPI schema from the handlers and structs
#[derive(OpenApi)]
#[openapi(
    paths(
        cluster::root_handler,
        cluster::health_handler,
        cluster::stats_handler,
        ingest::ingest_doc_handler,
        ingest::bulk_handler
    ),
    components(schemas(
        cluster::HealthResponse,
        cluster::StatsResponse,
        cluster::ShardsInfo,
        cluster::IndicesStats,
        cluster::IndexStats,
        cluster::DocsStats,
        cluster::StoreStats
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
        .route("/", get(cluster::root_handler))
        .route("/_health", get(cluster::health_handler))
        .route("/_cluster/health", get(cluster::health_handler)) // OpenSearch alias
        .route("/_stats", get(cluster::stats_handler))
        .route("/_bulk", post(ingest::bulk_handler))
        .route("/:index/_doc", post(ingest::ingest_doc_handler))
        .with_state(state)
}
