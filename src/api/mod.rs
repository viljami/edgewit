pub mod auth;
pub mod cluster;
pub mod indexes;
pub mod search;

use axum::{
    Router,
    routing::{get, post},
};
use utoipa::OpenApi;

pub use cluster::*;
pub use search::*;

use crate::ingestion::routes as ingest;
pub use crate::ingestion::routes::*;

use axum::extract::State;
use axum::response::IntoResponse;

#[utoipa::path(
    get,
    path = "/metrics",
    responses(
        (status = 200, description = "Prometheus compatible metrics", body = String)
    )
)]
pub async fn metrics_handler(State(state): State<AppState>) -> impl IntoResponse {
    state.prometheus_handle.render()
}

use crate::index_manager::IndexManager;
use crate::registry::IndexRegistry;
use std::path::PathBuf;
use tokio::sync::mpsc;

#[derive(Clone)]
pub struct AppState {
    pub wal_sender: mpsc::Sender<crate::wal::WalRequest>,
    pub index_manager: IndexManager,
    pub prometheus_handle: metrics_exporter_prometheus::PrometheusHandle,
    pub registry: IndexRegistry,
    pub data_dir: PathBuf,
}

// Generate the OpenAPI schema from the handlers and structs
#[derive(OpenApi)]
#[openapi(
    paths(
        cluster::root_handler,
        cluster::health_handler,
        cluster::stats_handler,
        ingest::ingest_doc_handler,
        ingest::bulk_handler,
        search::global_search_handler,
        search::index_search_handler,
        metrics_handler,
        cluster::cat_indexes_handler,
        indexes::create_index_handler,
        indexes::get_index_handler,
        indexes::delete_index_handler,
        indexes::list_indexes_handler
    ),
    components(schemas(
        cluster::HealthResponse,
        cluster::StatsResponse,
        cluster::ShardsInfo,
        cluster::IndicesStats,
        cluster::IndexStats,
        cluster::DocsStats,
        cluster::StoreStats,
        cluster::CatIndex,
        search::SearchRequestBody,
        crate::schema::definition::IndexDefinition,
        crate::schema::definition::FieldDefinition,
        crate::schema::definition::SchemaMode,
        crate::schema::definition::PartitionStrategy,
        crate::schema::definition::CompressionOption,
        crate::schema::definition::FieldType
    )),
    info(
        title = "Edgewit API",
        description = "Lightweight, Rust-based search and analytics engine for edge environments. Implements a focused subset of the OpenSearch API.",
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
        .route("/_cat/indexes", get(cluster::cat_indexes_handler))
        .route("/metrics", get(metrics_handler))
        .route("/_bulk", post(ingest::bulk_handler))
        .route("/{index}/_doc", post(ingest::ingest_doc_handler))
        .route("/indexes", get(indexes::list_indexes_handler))
        .route(
            "/indexes/{index}",
            get(indexes::get_index_handler)
                .put(indexes::create_index_handler)
                .delete(indexes::delete_index_handler),
        )
        .route(
            "/_search",
            get(search::global_search_handler).post(search::global_search_handler),
        )
        .route(
            "/{index}/_search",
            get(search::index_search_handler).post(search::index_search_handler),
        )
        .route_layer(axum::middleware::from_fn(auth::auth_middleware))
        .with_state(state)
}
