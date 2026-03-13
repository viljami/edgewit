use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::Json,
};
use bytes::Bytes;
use serde_json::{Value, json};
use tokio::sync::oneshot;

use crate::api::AppState;
use crate::wal::{IngestEvent, WalRequest};

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
    metrics::counter!("edgewit_ingest_requests_total").increment(1);
    metrics::counter!("edgewit_ingest_bytes_total").increment(body.len() as u64);

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
    metrics::counter!("edgewit_ingest_requests_total").increment(1);
    metrics::counter!("edgewit_ingest_bytes_total").increment(body.len() as u64);

    let payload_str = std::str::from_utf8(&body)
        .map_err(|_| (StatusCode::BAD_REQUEST, "Invalid UTF-8".to_string()))?;
    let mut lines = payload_str.lines().filter(|l| !l.trim().is_empty());

    let mut receivers = Vec::new();
    let mut indices = Vec::new();

    while let Some(meta_line) = lines.next() {
        let meta: serde_json::Value = serde_json::from_str(meta_line).unwrap_or_default();
        let index = meta
            .get("index")
            .and_then(|i| i.get("_index"))
            .and_then(|i| i.as_str())
            .unwrap_or("default")
            .to_string();

        let doc_line = match lines.next() {
            Some(l) => l,
            None => break,
        };

        let (tx, rx) = oneshot::channel();

        let req = WalRequest {
            event: IngestEvent {
                index: index.clone(),
                payload: doc_line.as_bytes().to_vec(),
            },
            responder: tx,
        };

        if state.wal_sender.send(req).await.is_err() {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                "WAL channel closed".to_string(),
            ));
        }

        receivers.push(rx);
        indices.push(index);
    }

    let mut items = Vec::new();
    let mut has_errors = false;

    for (i, rx) in receivers.into_iter().enumerate() {
        let status = match rx.await {
            Ok(Ok(_)) => 201,
            _ => {
                has_errors = true;
                500
            }
        };
        items.push(json!({"index": {"_index": indices[i], "status": status}}));
    }

    Ok((
        StatusCode::OK,
        Json(json!({
            "took": 1,
            "errors": has_errors,
            "items": items
        })),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::app_router;
    use axum_test::TestServer;
    use insta::assert_json_snapshot;
    use rstest::rstest;
    use tokio::sync::mpsc;

    fn setup_test_server() -> (TestServer, mpsc::Receiver<WalRequest>) {
        let (tx, rx) = mpsc::channel(100);
        let index = tantivy::Index::create_in_ram(crate::indexer::build_schema());
        let state = AppState {
            wal_sender: tx,
            index_reader: index.reader().unwrap(),
            prometheus_handle: metrics_exporter_prometheus::PrometheusBuilder::new()
                .build_recorder()
                .handle(),
        };
        let app = app_router(state);
        let server = TestServer::new(app).unwrap();
        (server, rx)
    }

    #[rstest]
    #[tokio::test]
    async fn test_ingest_doc_success() {
        let (server, mut wal_rx) = setup_test_server();

        // Spawn a background task to simulate the WAL responding with success
        tokio::spawn(async move {
            if let Some(req) = wal_rx.recv().await {
                // Assert the index is correct
                assert_eq!(req.event.index, "test-index");
                let _ = req.responder.send(Ok(()));
            }
        });

        let response = server
            .post("/test-index/_doc")
            .json(&json!({"message": "test event"}))
            .await;

        response.assert_status(StatusCode::CREATED);

        assert_json_snapshot!(response.json::<serde_json::Value>(), @r###"
        {
          "_index": "test-index",
          "_shards": {
            "failed": 0,
            "successful": 1,
            "total": 1
          },
          "result": "created"
        }
        "###);
    }

    #[rstest]
    #[tokio::test]
    async fn test_bulk_ingest_success() {
        let (server, mut wal_rx) = setup_test_server();

        tokio::spawn(async move {
            if let Some(req) = wal_rx.recv().await {
                let _ = req.responder.send(Ok(()));
            }
        });

        let response = server
            .post("/_bulk")
            .text("{\"index\": {\"_index\": \"test\"}}\n{\"message\": \"hello\"}\n")
            .await;

        response.assert_status(StatusCode::OK);

        assert_json_snapshot!(response.json::<serde_json::Value>(), @r###"
        {
          "errors": false,
          "items": [
            {
              "index": {
                "_index": "test",
                "status": 201
              }
            }
          ],
          "took": 1
        }
        "###);
    }

    #[rstest]
    #[tokio::test]
    async fn test_ingest_doc_wal_failure() {
        let (server, mut wal_rx) = setup_test_server();

        tokio::spawn(async move {
            if let Some(req) = wal_rx.recv().await {
                // Simulate a WAL disk write error
                let _ = req.responder.send(Err("Disk full".to_string()));
            }
        });

        let response = server
            .post("/test-index/_doc")
            .json(&json!({"message": "test event"}))
            .await;

        response.assert_status(StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(response.text(), "Disk full");
    }
}
