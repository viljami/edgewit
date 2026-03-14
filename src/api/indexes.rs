use axum::http::StatusCode;
use axum::{
    Json,
    extract::{Path, State},
    response::IntoResponse,
};
use serde_json::json;
use std::env;

use crate::api::AppState;
use crate::registry::RegistryError;
use crate::schema::definition::IndexDefinition;

fn is_management_enabled() -> bool {
    env::var("EDGEWIT_API_INDEX_MANAGEMENT_ENABLED")
        .unwrap_or_else(|_| "true".to_string())
        .to_lowercase()
        == "true"
}

#[utoipa::path(
    put,
    path = "/indexes/{index}",
    request_body = IndexDefinition,
    responses(
        (status = 200, description = "Index created/updated successfully"),
        (status = 400, description = "Invalid schema"),
        (status = 403, description = "API management disabled")
    )
)]
pub async fn create_index_handler(
    State(state): State<AppState>,
    Path(index): Path<String>,
    Json(mut payload): Json<IndexDefinition>,
) -> impl IntoResponse {
    if !is_management_enabled() {
        return (
            StatusCode::FORBIDDEN,
            Json(json!({"error": "Index management via API is disabled"})),
        );
    }

    // Ensure the payload name matches the URL path
    if payload.name != index {
        payload.name = index.clone();
    }

    // Validate and register/upsert in memory
    match state.registry.upsert(payload.clone()) {
        Ok(_) => {
            // Persist to disk
            let indexes_dir = state.data_dir.join("indexes");
            if let Err(e) = std::fs::create_dir_all(&indexes_dir) {
                tracing::error!("Failed to create indexes directory: {}", e);
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({"error": "Failed to create directory on disk"})),
                );
            }

            let file_path = indexes_dir.join(format!("{}.index.yaml", index));
            match serde_yaml::to_string(&payload) {
                Ok(yaml_str) => {
                    if let Err(e) = std::fs::write(&file_path, yaml_str) {
                        tracing::error!("Failed to write index definition to disk: {}", e);
                        return (
                            StatusCode::INTERNAL_SERVER_ERROR,
                            Json(json!({"error": "Failed to persist index definition"})),
                        );
                    }
                }
                Err(e) => {
                    tracing::error!("Failed to serialize index definition: {}", e);
                    return (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(json!({"error": "Failed to serialize definition"})),
                    );
                }
            }

            (
                StatusCode::OK,
                Json(json!({"message": "Index updated successfully"})),
            )
        }
        Err(RegistryError::ValidationError(e)) => (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": format!("Validation failed: {}", e)})),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": format!("Internal error: {}", e)})),
        ),
    }
}

#[utoipa::path(
    get,
    path = "/indexes/{index}",
    responses(
        (status = 200, description = "Index definition returned", body = IndexDefinition),
        (status = 404, description = "Index not found")
    )
)]
pub async fn get_index_handler(
    State(state): State<AppState>,
    Path(index): Path<String>,
) -> impl IntoResponse {
    match state.registry.get(&index) {
        Some(def) => (StatusCode::OK, Json(json!(def))),
        None => (
            StatusCode::NOT_FOUND,
            Json(json!({"error": "Index not found"})),
        ),
    }
}

#[utoipa::path(
    get,
    path = "/indexes",
    responses(
        (status = 200, description = "List of all index definitions", body = Vec<IndexDefinition>)
    )
)]
pub async fn list_indexes_handler(State(state): State<AppState>) -> impl IntoResponse {
    let indexes = state.registry.list();
    (StatusCode::OK, Json(json!(indexes)))
}

#[utoipa::path(
    delete,
    path = "/indexes/{index}",
    responses(
        (status = 200, description = "Index deleted successfully"),
        (status = 403, description = "API management disabled"),
        (status = 404, description = "Index not found")
    )
)]
pub async fn delete_index_handler(
    State(state): State<AppState>,
    Path(index): Path<String>,
) -> impl IntoResponse {
    if !is_management_enabled() {
        return (
            StatusCode::FORBIDDEN,
            Json(json!({"error": "Index management via API is disabled"})),
        );
    }

    match state.registry.remove(&index) {
        Ok(_) => {
            let indexes_dir = state.data_dir.join("indexes");
            let file_path = indexes_dir.join(format!("{}.index.yaml", index));

            // Try to delete the yaml file
            if file_path.exists()
                && let Err(e) = std::fs::remove_file(&file_path) {
                    tracing::error!("Failed to delete index definition file: {}", e);
                }

            // Wipe data
            let index_data_dir = indexes_dir.join(&index);
            if index_data_dir.exists()
                && let Err(e) = std::fs::remove_dir_all(&index_data_dir) {
                    tracing::error!("Failed to delete index data directory: {}", e);
                }

            (
                StatusCode::OK,
                Json(json!({"message": "Index deleted successfully"})),
            )
        }
        Err(RegistryError::NotFound(_)) => (
            StatusCode::NOT_FOUND,
            Json(json!({"error": "Index not found"})),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": format!("Internal error: {}", e)})),
        ),
    }
}
