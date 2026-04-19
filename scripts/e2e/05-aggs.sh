#!/usr/bin/env bash
# =============================================================================
# E2E Group 05 — Aggregations
# =============================================================================
# This script is intended to be sourced by the main test runner. It assumes
# the shared helpers from `scripts/e2e/00-helpers.sh` are already sourced and
# that the following variables are available in the environment:
#   - MAIN_URL
#   - COMMIT_WAIT (optional; used to wait for indexer commits)
#
# The script must not call `exit` — use `pass` / `fail` helpers to report.
# =============================================================================

# Bail quietly if helpers aren't present so the runner can continue with other groups.
if ! declare -f http_put >/dev/null 2>&1 || ! declare -f http_post >/dev/null 2>&1 || ! declare -f assert_status >/dev/null 2>&1; then
  echo "helpers not loaded: skipping 05-aggs.sh" >&2
  return 0
fi

section "Aggregations"

# Create a dedicated index with explicit fast numeric + datetime fields
read -r -d '' AGG_INDEX_DEF <<'JSON' || true
{
  "name": "e2e-aggs",
  "timestamp_field": "timestamp",
  "mode": "dynamic",
  "partition": "none",
  "compression": "zstd",
  "fields": {
    "amount": {
      "type": "float",
      "indexed": true,
      "fast": true,
      "stored": false,
      "optional": true
    },
    "timestamp": {
      "type": "datetime",
      "indexed": true,
      "fast": true,
      "stored": false,
      "optional": false
    }
  },
  "settings": {}
}
JSON

resp=$(http_put "${MAIN_URL}/indexes/e2e-aggs" "$AGG_INDEX_DEF")
assert_status "PUT /indexes/e2e-aggs  →  200" "200" "$resp"

# Ingest 10 docs: amounts 10..19  →  sum=145, avg=14.5
# Spread across months to give date_histogram buckets.
AGG_BULK=$'{"index":{"_index":"e2e-aggs"}}\n{"timestamp":"2024-01-05T12:00:00Z","amount":10.0,"category":"alpha"}\n{"index":{"_index":"e2e-aggs"}}\n{"timestamp":"2024-01-20T06:00:00Z","amount":11.0,"category":"beta"}\n{"index":{"_index":"e2e-aggs"}}\n{"timestamp":"2024-02-03T18:00:00Z","amount":12.0,"category":"alpha"}\n{"index":{"_index":"e2e-aggs"}}\n{"timestamp":"2024-02-14T09:00:00Z","amount":13.0,"category":"beta"}\n{"index":{"_index":"e2e-aggs"}}\n{"timestamp":"2024-03-01T00:00:00Z","amount":14.0,"category":"alpha"}\n{"index":{"_index":"e2e-aggs"}}\n{"timestamp":"2024-03-22T15:00:00Z","amount":15.0,"category":"beta"}\n{"index":{"_index":"e2e-aggs"}}\n{"timestamp":"2024-04-07T03:00:00Z","amount":16.0,"category":"alpha"}\n{"index":{"_index":"e2e-aggs"}}\n{"timestamp":"2024-04-18T21:00:00Z","amount":17.0,"category":"beta"}\n{"index":{"_index":"e2e-aggs"}}\n{"timestamp":"2024-05-09T11:00:00Z","amount":18.0,"category":"alpha"}\n{"index":{"_index":"e2e-aggs"}}\n{"timestamp":"2024-06-30T23:00:00Z","amount":19.0,"category":"beta"}'

resp=$(http_post "${MAIN_URL}/_bulk" "application/x-ndjson" "$AGG_BULK")
assert_status "POST /_bulk agg docs  →  200" "200" "$resp"

echo ""
echo "  ⏳ Waiting ${COMMIT_WAIT:-3}s for the indexer to commit…"
sleep "${COMMIT_WAIT:-3}"

# Verify all 10 documents are indexed
resp=$(http_get "${MAIN_URL}/indexes/e2e-aggs/_search")
assert_json_eq "e2e-aggs: 10 docs indexed" ".hits.total.value" "10" "$resp"

# --- Sum + Avg ---
resp=$(http_post "${MAIN_URL}/indexes/e2e-aggs/_search" "application/json" '{
  "size": 0,
  "aggs": {
    "total_sum": { "sum": { "field": "amount" } },
    "avg_amount": { "avg": { "field": "amount" } }
  }
}')
assert_status   "POST /_search sum+avg aggs  →  200"  "200" "$resp"
assert_json_eq  "aggs sum: total_sum = 145"           ".aggregations.total_sum.value" "145" "$resp"
assert_json_eq  "aggs avg: avg_amount = 14.5"         ".aggregations.avg_amount.value" "14.5" "$resp"

# --- Date histogram (30-day buckets across ~6 months) ---
resp=$(http_post "${MAIN_URL}/indexes/e2e-aggs/_search" "application/json" '{
  "size": 0,
  "aggs": {
    "by_30d": { "date_histogram": { "field": "timestamp", "fixed_interval": "30d" } }
  }
}')
assert_status "POST /_search date_histogram (30d)  →  200" "200" "$resp"
BUCKET_COUNT=$(body_of "$resp" | jq '.aggregations.by_30d.buckets | length' 2>/dev/null || echo 0)
if awk "BEGIN{exit !($BUCKET_COUNT >= 1)}"; then
  pass "date_histogram (30d): ≥ 1 bucket returned (got ${BUCKET_COUNT})"
else
  fail "date_histogram (30d): expected ≥ 1 bucket" "≥ 1" "$BUCKET_COUNT"
fi

# --- Date histogram (1d) sanity check ---
resp=$(http_post "${MAIN_URL}/indexes/e2e-aggs/_search" "application/json" '{
  "size": 0,
  "aggs": {
    "by_day": { "date_histogram": { "field": "timestamp", "fixed_interval": "1d" } }
  }
}')
assert_status "POST /_search date_histogram (1d)  →  200" "200" "$resp"
DAY_BUCKETS=$(body_of "$resp" | jq '.aggregations.by_day.buckets | length' 2>/dev/null || echo 0)
if awk "BEGIN{exit !($DAY_BUCKETS >= 10)}"; then
  pass "date_histogram (1d): ≥ 10 daily buckets (got ${DAY_BUCKETS})"
else
  fail "date_histogram (1d): expected ≥ 10 daily buckets" "≥ 10" "$DAY_BUCKETS"
fi

# Completed group
return 0
