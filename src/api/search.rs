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

/// Executes a Tantivy search across the provided readers.
///
/// Accepts a slice so that it remains unit-testable with in-RAM indexes.
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

    let mut global_total_docs: usize = 0;
    let mut global_hits: Vec<(f32, usize, tantivy::DocAddress)> = Vec::new();
    let mut global_aggs: Option<IntermediateAggregationResults> = None;

    for (reader_idx, reader) in readers.iter().enumerate() {
        if let Err(e) = reader.reload() {
            error!("Failed to reload reader: {e}");
        }
        let searcher = reader.searcher();
        let schema = searcher.index().schema();

        let source_field = match schema.get_field("_source") {
            Ok(f) => f,
            Err(_) => continue,
        };

        let query_parser = QueryParser::for_index(searcher.index(), vec![source_field]);
        let query: Box<dyn tantivy::query::Query> =
            if query_str.trim().is_empty() || query_str.trim() == "*" {
                Box::new(tantivy::query::AllQuery)
            } else {
                query_parser
                    .parse_query(query_str)
                    .map_err(|e| format!("Invalid query: {e}"))?
            };

        if let Some(aggs_req) = &aggs {
            let collector = DistributedAggregationCollector::from_aggs(
                aggs_req.clone(),
                AggregationLimitsGuard::default(),
            );
            let (total, top, aggs_res) = searcher
                .search(&query, &(Count, TopDocs::with_limit(limit), collector))
                .map_err(|e| format!("Search error: {e}"))?;
            global_total_docs += total;
            for (score, addr) in top {
                global_hits.push((score, reader_idx, addr));
            }
            if let Some(existing) = &mut global_aggs {
                let _ = existing.merge_fruits(aggs_res);
            } else {
                global_aggs = Some(aggs_res);
            }
        } else {
            let (total, top) = searcher
                .search(&query, &(Count, TopDocs::with_limit(limit)))
                .map_err(|e| format!("Search error: {e}"))?;
            global_total_docs += total;
            for (score, addr) in top {
                global_hits.push((score, reader_idx, addr));
            }
        }
    }

    global_hits.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
    let paginated = global_hits
        .into_iter()
        .skip(from)
        .take(if size == 0 { 0 } else { size });

    let mut hits = Vec::new();
    let mut max_score: Option<f32> = None;

    for (score, reader_idx, doc_address) in paginated {
        max_score.get_or_insert(score);

        let searcher = readers[reader_idx].searcher();
        let schema = searcher.index().schema();
        let retrieved_doc: tantivy::TantivyDocument = searcher
            .doc(doc_address)
            .map_err(|e| format!("Failed to retrieve doc: {e}"))?;

        let doc_json = retrieved_doc.to_json(&schema);
        let mut source_json = serde_json::Value::Null;
        let mut index_name = "unknown".to_string();

        if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&doc_json) {
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
            _id: "unknown".to_string(),
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

/// Translates an OpenSearch-style query DSL subset into a Tantivy query string.
fn extract_query_string(params: &SearchQueryParams, body: Option<&SearchRequestBody>) -> String {
    if let Some(q) = &params.q {
        return q.clone();
    }

    if let Some(b) = body
        && let Some(q) = &b.query
    {
        if let Some(qs) = q.get("query_string")
            && let Some(s) = qs.get("query").and_then(|v| v.as_str())
        {
            return s.to_string();
        }
        if q.get("match_all").is_some() {
            return "*".to_string();
        }
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
        if let Some(bool_q) = q.get("bool")
            && let Some(must) = bool_q.get("must")
            && let Some(arr) = must.as_array()
        {
            let mut parts = Vec::new();
            for item in arr {
                if let Some(m) = item.get("match")
                    && let Some(obj) = m.as_object()
                {
                    for (k, v) in obj {
                        if let Some(s) = v.as_str() {
                            parts.push(format!("{k}:{s}"));
                        } else if let Some(obj2) = v.as_object()
                            && let Some(q_val) = obj2.get("query").and_then(|v| v.as_str())
                        {
                            parts.push(format!("{k}:{q_val}"));
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

    // Single reader per logical index
    let readers = state
        .index_manager
        .get_reader(&index)
        .map(|r| vec![r])
        .unwrap_or_default();

    match execute_search(&readers, &query_str, Some(&index), from, size, aggs) {
        Ok(resp) => axum::response::Json(resp).into_response(),
        Err(e) => {
            error!("Search failed for '{}': {e}", index);
            (
                axum::http::StatusCode::BAD_REQUEST,
                format!("Search error: {e}"),
            )
                .into_response()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use tantivy::{Index, doc, schema::*};

    #[test]
    fn test_execute_search_wildcard() {
        let mut schema_builder = Schema::builder();
        let source_field = schema_builder.add_json_field("_source", STORED | TEXT);
        let schema = schema_builder.build();
        let index = Index::create_in_ram(schema);

        let mut writer = index.writer(15_000_000).unwrap();
        writer
            .add_document(doc!(source_field => json!({"message": "hello"})))
            .unwrap();
        writer
            .add_document(doc!(source_field => json!({"message": "world"})))
            .unwrap();
        writer.commit().unwrap();

        let reader = index.reader().unwrap();

        let res = execute_search(&[reader.clone()], "*", None, 0, 10, None).unwrap();
        assert_eq!(res.hits.total.value, 2);

        let res_empty = execute_search(&[reader.clone()], " ", None, 0, 10, None).unwrap();
        assert_eq!(res_empty.hits.total.value, 2);
    }

    #[test]
    fn test_execute_search_missing_source_field() {
        let mut schema_builder = Schema::builder();
        let text_field = schema_builder.add_text_field("text", TEXT | STORED);
        let schema = schema_builder.build();
        let index = Index::create_in_ram(schema);

        let mut writer = index.writer(15_000_000).unwrap();
        writer.add_document(doc!(text_field => "hello")).unwrap();
        writer.commit().unwrap();

        let reader = index.reader().unwrap();

        // Should return 0 hits gracefully when _source field is absent
        let res = execute_search(&[reader], "*", None, 0, 10, None).unwrap();
        assert_eq!(res.hits.total.value, 0);
        assert!(res.hits.hits.is_empty());
    }
}
