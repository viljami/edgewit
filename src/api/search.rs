use axum::{
    Json,
    extract::{Path, Query, State},
    response::IntoResponse,
};
use serde::{Deserialize, Serialize};
use tantivy::{
    Document, IndexReader,
    aggregation::AggregationCollector,
    aggregation::agg_req::Aggregations,
    aggregation::agg_result::AggregationResults,
    collector::{Count, TopDocs},
    query::QueryParser,
};
use tracing::error;

use super::AppState;
use crate::api::ShardsInfo;

#[derive(Deserialize, Debug, Default)]
pub struct SearchQueryParams {
    pub q: Option<String>,
    pub from: Option<usize>,
    pub size: Option<usize>,
}

#[derive(Deserialize, Debug, Default)]
pub struct SearchRequestBody {
    pub query: Option<serde_json::Value>,
    pub from: Option<usize>,
    pub size: Option<usize>,
    pub sort: Option<serde_json::Value>,
    pub aggs: Option<Aggregations>,
}

#[derive(Serialize, Debug)]
pub struct SearchResponse {
    pub took: u64,
    pub timed_out: bool,
    pub _shards: ShardsInfo,
    pub hits: HitsInfo,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub aggregations: Option<AggregationResults>,
}

#[derive(Serialize, Debug)]
pub struct HitsInfo {
    pub total: TotalHits,
    pub max_score: Option<f32>,
    pub hits: Vec<Hit>,
}

#[derive(Serialize, Debug)]
pub struct TotalHits {
    pub value: usize,
    pub relation: String,
}

#[derive(Serialize, Debug)]
pub struct Hit {
    pub _index: String,
    pub _id: String,
    pub _score: Option<f32>,
    pub _source: serde_json::Value,
}

/// Helper to execute the actual Tantivy search
fn execute_search(
    reader: &IndexReader,
    query_str: &str,
    target_index: Option<&str>,
    from: usize,
    size: usize,
    aggs: Option<Aggregations>,
) -> Result<SearchResponse, String> {
    let _ = reader.reload();
    let searcher = reader.searcher();
    let schema = searcher.index().schema();

    // Default to searching the `_source` field, which we configured as TEXT in M2
    let source_field = schema.get_field("_source").unwrap();

    let query_parser = QueryParser::for_index(searcher.index(), vec![source_field]);

    // Construct the query. If a specific index is targeted, we should technically filter by it.
    // For simplicity in M3, we parse the user's query and if target_index is present, we wrap it in a boolean query.
    let final_query_str = if let Some(idx) = target_index {
        if query_str.trim().is_empty() || query_str == "*" {
            format!("_index:{}", idx)
        } else {
            format!("+(_index:{}) +({})", idx, query_str)
        }
    } else {
        if query_str.trim().is_empty() {
            "*".to_string()
        } else {
            query_str.to_string()
        }
    };

    let query = query_parser
        .parse_query(&final_query_str)
        .map_err(|e| format!("Invalid query: {}", e))?;

    let start = std::time::Instant::now();

    // We collect the total count and the top-K documents, and optionally aggregations
    let limit = if size == 0 { 1 } else { size };
    let (total_docs, top_docs, extracted_aggs) = if let Some(aggs_req) = aggs {
        let agg_collector = AggregationCollector::from_aggs(aggs_req, Default::default());
        let (total_docs, top_docs, aggs_res) = searcher
            .search(
                &query,
                &(
                    Count,
                    TopDocs::with_limit(limit).and_offset(from),
                    agg_collector,
                ),
            )
            .map_err(|e| format!("Search error: {}", e))?;
        (total_docs, top_docs, Some(aggs_res))
    } else {
        let (total_docs, top_docs) = searcher
            .search(
                &query,
                &(Count, TopDocs::with_limit(limit).and_offset(from)),
            )
            .map_err(|e| format!("Search error: {}", e))?;
        (total_docs, top_docs, None)
    };

    let took = start.elapsed().as_millis() as u64;

    let mut hits = Vec::new();
    let mut max_score = None;

    if size > 0 {
        for (score, doc_address) in top_docs {
            if max_score.is_none() {
                max_score = Some(score);
            }

            let retrieved_doc: tantivy::TantivyDocument = searcher
                .doc(doc_address)
                .map_err(|e| format!("Failed to retrieve doc: {}", e))?;

            let doc_json_str = retrieved_doc.to_json(&schema);
            let mut source_json = serde_json::Value::Null;
            let mut index_name = "unknown".to_string();

            if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&doc_json_str) {
                if let Some(arr) = parsed.get("_source").and_then(|v| v.as_array()) {
                    if !arr.is_empty() {
                        source_json = arr[0].clone();
                    }
                } else if let Some(val) = parsed.get("_source") {
                    source_json = val.clone();
                }

                if let Some(arr) = parsed.get("_index").and_then(|v| v.as_array()) {
                    if !arr.is_empty() {
                        if let Some(s) = arr[0].as_str() {
                            index_name = s.to_string();
                        }
                    }
                } else if let Some(val) = parsed.get("_index").and_then(|v| v.as_str()) {
                    index_name = val.to_string();
                }
            }

            hits.push(Hit {
                _index: index_name,
                _id: format!("{}-{}", doc_address.segment_ord, doc_address.doc_id), // Synthetic ID
                _score: Some(score),
                _source: source_json,
            });
        }
    }

    Ok(SearchResponse {
        took,
        timed_out: false,
        _shards: ShardsInfo {
            total: 1,
            successful: 1,
            skipped: 0,
            failed: 0,
        },
        hits: HitsInfo {
            total: TotalHits {
                value: total_docs,
                relation: "eq".to_string(),
            },
            max_score,
            hits,
        },
        aggregations: extracted_aggs,
    })
}

// Extract search query from either URL params or JSON body
fn extract_query_string(params: &SearchQueryParams, body: Option<&SearchRequestBody>) -> String {
    if let Some(q) = &params.q {
        return q.clone();
    }

    if let Some(b) = body {
        if let Some(q) = &b.query {
            // Very naive OpenSearch query DSL parsing for M3
            // If they passed {"query": {"query_string": {"query": "foo"}}}
            if let Some(qs) = q.get("query_string") {
                if let Some(query_str) = qs.get("query").and_then(|v| v.as_str()) {
                    return query_str.to_string();
                }
            }
            // If they passed {"query": {"match_all": {}}}
            if q.get("match_all").is_some() {
                return "*".to_string();
            }
        }
    }

    "*".to_string()
}

/// OpenSearch `/_search` endpoint (searches all indices)
#[utoipa::path(
    get,
    path = "/_search",
    tag = "search",
    responses(
        (status = 200, description = "Search results")
    )
)]
pub async fn global_search_handler(
    State(state): State<AppState>,
    Query(params): Query<SearchQueryParams>,
    body: Option<Json<SearchRequestBody>>,
) -> impl IntoResponse {
    let query_str = extract_query_string(&params, body.as_ref().map(|j| &j.0));
    let from = params
        .from
        .or_else(|| body.as_ref().and_then(|b| b.from))
        .unwrap_or(0);
    let size = params
        .size
        .or_else(|| body.as_ref().and_then(|b| b.size))
        .unwrap_or(10);

    let aggs = body.as_ref().and_then(|b| b.aggs.clone());
    let reader = state.index_reader.clone(); // Assumes index_reader is added to AppState

    match execute_search(&reader, &query_str, None, from, size, aggs) {
        Ok(resp) => axum::response::Json(resp).into_response(),
        Err(e) => {
            error!("Search failed: {}", e);
            (
                axum::http::StatusCode::BAD_REQUEST,
                format!("Search error: {}", e),
            )
                .into_response()
        }
    }
}

/// OpenSearch `/:index/_search` endpoint (searches a specific index)
#[utoipa::path(
    get,
    path = "/{index}/_search",
    tag = "search",
    responses(
        (status = 200, description = "Search results for index")
    )
)]
pub async fn index_search_handler(
    State(state): State<AppState>,
    Path(index): Path<String>,
    Query(params): Query<SearchQueryParams>,
    body: Option<Json<SearchRequestBody>>,
) -> impl IntoResponse {
    let query_str = extract_query_string(&params, body.as_ref().map(|j| &j.0));
    let from = params
        .from
        .or_else(|| body.as_ref().and_then(|b| b.from))
        .unwrap_or(0);
    let size = params
        .size
        .or_else(|| body.as_ref().and_then(|b| b.size))
        .unwrap_or(10);

    let aggs = body.as_ref().and_then(|b| b.aggs.clone());
    let reader = state.index_reader.clone();

    match execute_search(&reader, &query_str, Some(&index), from, size, aggs) {
        Ok(resp) => axum::response::Json(resp).into_response(),
        Err(e) => {
            error!("Search failed for index {}: {}", index, e);
            (
                axum::http::StatusCode::BAD_REQUEST,
                format!("Search error: {}", e),
            )
                .into_response()
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::api::{AppState, app_router};
    use crate::indexer::build_schema;
    use axum_test::TestServer;
    use tantivy::Index;
    use tokio::sync::mpsc;

    fn setup_test_server() -> TestServer {
        let (tx, _rx) = mpsc::channel(100);
        let schema = build_schema();
        let index = Index::create_in_ram(schema.clone());
        let mut writer = index.writer(15_000_000).unwrap();

        let doc_str = r#"{"_index": "test_idx", "_source": {"message": "hello edgewit"}}"#;
        let doc = tantivy::TantivyDocument::parse_json(&schema, doc_str).unwrap();
        writer.add_document(doc).unwrap();
        writer.commit().unwrap();

        let state = AppState {
            wal_sender: tx,
            index_reader: index.reader().unwrap(),
        };

        TestServer::new(app_router(state)).unwrap()
    }

    #[tokio::test]
    async fn test_global_search_endpoint() {
        let server = setup_test_server();

        let response = server
            .get("/_search")
            .add_query_param("q", "_source.message:hello")
            .await;

        response.assert_status_ok();

        let json = response.json::<serde_json::Value>();
        assert_eq!(json["hits"]["total"]["value"], 1);
        assert_eq!(
            json["hits"]["hits"][0]["_source"]["message"],
            "hello edgewit"
        );
        assert_eq!(json["hits"]["hits"][0]["_index"], "test_idx");
    }

    #[tokio::test]
    async fn test_index_search_endpoint() {
        let server = setup_test_server();

        // Search specifically in test_idx
        let response = server
            .get("/test_idx/_search")
            .add_query_param("q", "_source.message:hello")
            .await;

        response.assert_status_ok();
        let json = response.json::<serde_json::Value>();
        assert_eq!(json["hits"]["total"]["value"], 1);

        // Search in wrong index
        let response = server
            .get("/wrong_idx/_search")
            .add_query_param("q", "_source.message:hello")
            .await;

        response.assert_status_ok();
        let json = response.json::<serde_json::Value>();
        assert_eq!(json["hits"]["total"]["value"], 0);
    }

    #[tokio::test]
    async fn test_search_aggregations() {
        let server = setup_test_server();

        let req_body = serde_json::json!({
            "size": 0,
            "aggs": {
                "messages": {
                    "terms": {
                        "field": "_source.message"
                    }
                }
            }
        });

        let response = server.get("/_search").json(&req_body).await;

        response.assert_status_ok();

        let json = response.json::<serde_json::Value>();
        assert!(
            json.get("aggregations").is_some(),
            "Aggregations should be present in the response"
        );
    }
}
