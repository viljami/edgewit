use axum_test::TestServer;
use edgewit::api::{AppState, app_router};
use edgewit::index_manager::IndexManager;
use edgewit::registry::IndexRegistry;
use metrics_exporter_prometheus::PrometheusBuilder;

use tempfile::TempDir;
use tokio::sync::mpsc;

#[tokio::test]
async fn test_api_authentication() {
    // Set the expected API key BEFORE initializing the app router to ensure the
    // OnceLock in auth.rs picks it up. Use unsafe to modify the env var.
    unsafe {
        std::env::set_var("EDGEWIT_API_KEY", "test-secret-token");
    }

    let temp_dir = TempDir::new().unwrap();
    let data_dir = temp_dir.path().to_path_buf();
    let registry = IndexRegistry::new();
    let index_manager = IndexManager::new(data_dir.clone(), registry.clone(), 20);

    let (wal_tx, _wal_rx) = mpsc::channel(100);

    // Provide a basic app state (no indexer/wal background threads needed just to test auth middleware)
    let prometheus_handle = PrometheusBuilder::new().build_recorder().handle();
    let state = AppState {
        wal_sender: wal_tx,
        index_manager,
        prometheus_handle,
        registry,
        data_dir,
    };

    let app = app_router(state);
    let server = TestServer::new(app);

    // 1. Unauthenticated request should fail
    let unauth_resp = server.get("/_health").await;
    unauth_resp.assert_status(axum::http::StatusCode::UNAUTHORIZED);

    // 2. Incorrect token should fail
    let bad_auth_resp = server
        .get("/_health")
        .add_header(axum::http::header::AUTHORIZATION, "Bearer wrong-token")
        .await;
    bad_auth_resp.assert_status(axum::http::StatusCode::UNAUTHORIZED);

    // 3. Correct token should succeed
    let auth_resp = server
        .get("/_health")
        .add_header(
            axum::http::header::AUTHORIZATION,
            "Bearer test-secret-token",
        )
        .await;
    auth_resp.assert_status(axum::http::StatusCode::OK);
}
