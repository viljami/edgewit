use axum::{
    Json,
    extract::{Path, Query, State},
    response::IntoResponse,
};
use serde::{Deserialize, Serialize};
use tantivy::{
    Document,
    aggregation::{
        AggregationLimitsGuard, DistributedAggregationCollector, agg_req::Aggregations,
        agg_result::AggregationResults, intermediate_agg_result::IntermediateAggregationResults,
    },
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

#[derive(Deserialize, Debug, Default, utoipa::ToSchema)]
pub struct SearchRequestBody {
    pub query: Option<serde_json::Value>,
    pub from: Option<usize>,
    pub size: Option<usize>,
    pub sort: Option<serde_json::Value>,
    #[schema(value_type = Option<serde_json::Value>)]
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
    readers: &[tantivy::IndexReader],
    query_str: &str,
    _target_index: Option<&str>,
    from: usize,
    size: usize,
    aggs: Option<Aggregations>,
) -> Result<SearchResponse, String> {
    let start = std::time::Instant::now();
    let limit = if size == 0 { 1 } else { from + size };

    let mut global_total_docs = 0;
    let mut global_hits: Vec<(f32, usize, tantivy::DocAddress)> = Vec::new();
    let mut global_aggs: Option<IntermediateAggregationResults> = None;

    for (reader_idx, reader) in readers.iter().enumerate() {
        let _ = reader.reload();
        let searcher = reader.searcher();
        let schema = searcher.index().schema();

        let source_field = match schema.get_field("_source") {
            Ok(f) => f,
            Err(_) => continue,
        };

        let query_parser = QueryParser::for_index(searcher.index(), vec![source_field]);

        let final_query_str = if query_str.trim().is_empty() {
            "*".to_string()
        } else {
            query_str.to_string()
        };

        let query = query_parser
            .parse_query(&final_query_str)
            .map_err(|e| format!("Invalid query: {e}"))?;

        if let Some(aggs_req) = &aggs {
            let agg_collector = DistributedAggregationCollector::from_aggs(
                aggs_req.clone(),
                AggregationLimitsGuard::default(),
            );
            let (total_docs, top_docs, aggs_res) = searcher
                .search(&query, &(Count, TopDocs::with_limit(limit), agg_collector))
                .map_err(|e| format!("Search error: {e}"))?;
            global_total_docs += total_docs;
            for (score, doc_address) in top_docs {
                global_hits.push((score, reader_idx, doc_address));
            }
            if let Some(existing_aggs) = &mut global_aggs {
                let _ = existing_aggs.merge_fruits(aggs_res);
            } else {
                global_aggs = Some(aggs_res);
            }
        } else {
            let (total_docs, top_docs) = searcher
                .search(&query, &(Count, TopDocs::with_limit(limit)))
                .map_err(|e| format!("Search error: {e}"))?;
            global_total_docs += total_docs;
            for (score, doc_address) in top_docs {
                global_hits.push((score, reader_idx, doc_address));
            }
        }
    }

    global_hits.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
    let paginated_hits = global_hits
        .into_iter()
        .skip(from)
        .take(if size == 0 { 0 } else { size });

    let mut hits = Vec::new();
    let mut max_score = None;

    for (score, reader_idx, doc_address) in paginated_hits {
        if max_score.is_none() {
            max_score = Some(score);
        }

        let searcher = readers[reader_idx].searcher();
        let schema = searcher.index().schema();
        let retrieved_doc: tantivy::TantivyDocument = searcher
            .doc(doc_address)
            .map_err(|e| format!("Failed to retrieve doc: {e}"))?;

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

            if let Some(arr) = parsed.get("_index").and_then(|v| v.as_array())
                && !arr.is_empty()
                && let Some(s) = arr[0].as_str()
            {
                index_name = s.to_string();
            }
        }

        hits.push(Hit {
            _index: index_name,
            _id: "unknown".to_string(), // In a real system, track actual document IDs
            _score: Some(score),
            _source: source_json,
        });
    }

    let took = start.elapsed().as_millis() as u64;
    metrics::histogram!("edgewit_search_latency_seconds").record(start.elapsed().as_secs_f64());
    metrics::counter!("edgewit_search_requests_total").increment(1);

    Ok(SearchResponse {
        took,
        timed_out: false,
        _shards: ShardsInfo {
            total: readers.len() as u32,
            successful: readers.len() as u32,
            skipped: 0,
            failed: 0,
        },
        hits: HitsInfo {
            total: TotalHits {
                value: global_total_docs,
                relation: "eq".to_string(),
            },
            max_score,
            hits,
        },
        aggregations: match (global_aggs, aggs) {
            (Some(res), Some(req)) => res
                .into_final_result(req, AggregationLimitsGuard::default())
                .ok(),
            _ => None,
        },
    })
}

// Extract search query from either URL params or JSON body
fn extract_query_string(params: &SearchQueryParams, body: Option<&SearchRequestBody>) -> String {
    if let Some(q) = &params.q {
        return q.clone();
    }

    if let Some(b) = body
        && let Some(q) = &b.query
    {
        // Very naive OpenSearch query DSL parsing for M3
        // If they passed {"query": {"query_string": {"query": "foo"}}}
        if let Some(qs) = q.get("query_string")
            && let Some(query_str) = qs.get("query").and_then(|v| v.as_str())
        {
            return query_str.to_string();
        }
        // If they passed {"query": {"match_all": {}}}
        if q.get("match_all").is_some() {
            return "*".to_string();
        }

        // If they passed {"query": {"match": {"field": "value"}}}
        if let Some(m) = q.get("match")
            && let Some(obj) = m.as_object()
        {
            for (k, v) in obj {
                if let Some(s) = v.as_str() {
                    return format!("{k}:{s}");
                } else if let Some(obj2) = v.as_object()
                    && let Some(q_val) = obj2.get("query").and_then(|v| v.as_str())
                {
                    return format!("{k}:{q_val}");
                }
            }
        }

        // Basic fallback for other simple queries (e.g., bool -> must -> match)
        if let Some(bool_q) = q.get("bool")
            && let Some(must) = bool_q.get("must")
            && let Some(arr) = must.as_array()
        {
            let mut parts = Vec::new();
            for item in arr {
                if let Some(m) = item.get("match") {
                    if let Some(obj) = m.as_object() {
                        for (k, v) in obj {
                            if let Some(s) = v.as_str() {
                                parts.push(format!("{k}:{s}"));
                            } else if let Some(obj2) = v.as_object()
                                && let Some(q_val) = obj2.get("query").and_then(|v| v.as_str())
                            {
                                parts.push(format!("{k}:{q_val}"));
                            }
                        }
                    }
                } else if let Some(m) = item.get("match_phrase")
                    && let Some(obj) = m.as_object()
                {
                    for (k, v) in obj {
                        if let Some(s) = v.as_str() {
                            parts.push(format!("{k}:\"{s}\""));
                        }
                    }
                }
            }
            if !parts.is_empty() {
                return parts.join(" AND ");
            }
        }
    }

    "*".to_string()
}

/// OpenSearch `/_search` endpoint (searches all indices)

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
    let readers = state
        .index_manager
        .get_all_readers(&index)
        .unwrap_or_default();

    match execute_search(&readers, &query_str, Some(&index.clone()), from, size, aggs) {
        Ok(resp) => axum::response::Json(resp).into_response(),
        Err(e) => {
            error!("Search failed for index {}: {}", index, e);
            (
                axum::http::StatusCode::BAD_REQUEST,
                format!("Search error: {e}"),
            )
                .into_response()
        }
    }
}

#[cfg(test)]
#[cfg(test)]
mod tests {}
