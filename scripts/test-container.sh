#!/usr/bin/env bash
# =============================================================================
# Edgewit Container End-to-End Test Suite
# =============================================================================
# Tests the Edgewit binary running inside its Docker container by exercising
# the full HTTP API: cluster health, index management, ingest, search,
# aggregations, authentication, and data persistence across restarts.
#
# Requirements: docker, curl, jq
#
# Usage:
#   ./scripts/test-container.sh [options]
#
# Options:
#   --skip-build      Skip `docker build` and use an existing image
#   --keep-container  Leave containers running on exit (useful for debugging)
#   --image NAME      Docker image tag to use (default: edgewit:e2e-test)
# =============================================================================

set -euo pipefail

# ---------------------------------------------------------------------------
# Argument parsing
# ---------------------------------------------------------------------------
SKIP_BUILD=false
KEEP_CONTAINER=false
IMAGE="edgewit:e2e-test"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --skip-build)      SKIP_BUILD=true ;;
    --keep-container)  KEEP_CONTAINER=true ;;
    --image)           shift; IMAGE="$1" ;;
    *) echo "Unknown option: $1" >&2; exit 1 ;;
  esac
  shift
done

# ---------------------------------------------------------------------------
# Configuration
# ---------------------------------------------------------------------------
MAIN_CONTAINER="edgewit-e2e-main"
AUTH_CONTAINER="edgewit-e2e-auth"
PERSIST_CONTAINER="edgewit-e2e-persist"
PERSIST_VOLUME="edgewit-e2e-persist-vol"

MAIN_PORT=19200
AUTH_PORT=19201
PERSIST_PORT=19202

MAIN_URL="http://localhost:${MAIN_PORT}"
AUTH_URL="http://localhost:${AUTH_PORT}"
PERSIST_URL="http://localhost:${PERSIST_PORT}"

READY_TIMEOUT=60   # seconds to wait for a container to become reachable
COMMIT_WAIT=3      # seconds to wait after ingest for the indexer to commit

# ---------------------------------------------------------------------------
# ANSI colours
# ---------------------------------------------------------------------------
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
BOLD='\033[1m'
NC='\033[0m'

# ---------------------------------------------------------------------------
# Dependency checks
# ---------------------------------------------------------------------------
check_deps() {
  local missing=()
  for cmd in docker curl jq; do
    command -v "$cmd" &>/dev/null || missing+=("$cmd")
  done
  if [[ ${#missing[@]} -gt 0 ]]; then
    echo -e "${RED}Missing required tools: ${missing[*]}${NC}" >&2
    echo "Install them before running this script." >&2
    exit 1
  fi
}

# ---------------------------------------------------------------------------
# Test counters
# ---------------------------------------------------------------------------
PASS=0
FAIL=0
declare -a FAILED_TESTS=()

pass() {
  PASS=$((PASS + 1))
  echo -e "  ${GREEN}✔${NC} $1"
}

fail() {
  FAIL=$((FAIL + 1))
  FAILED_TESTS+=("$1")
  echo -e "  ${RED}✘${NC} $1"
  [[ -n "${2:-}" ]] && echo -e "    ${YELLOW}expected:${NC} $2"
  [[ -n "${3:-}" ]] && echo -e "    ${YELLOW}actual  :${NC} $3"
}

section() {
  echo ""
  echo -e "${BLUE}${BOLD}▶ $1${NC}"
}

# ---------------------------------------------------------------------------
# HTTP helpers
#
# Each helper writes:   <response-body>\n<http-status>
# If curl cannot connect it falls back to:   \n000
#
# We intentionally do NOT pass -f so that 4xx/5xx bodies are captured.
# ---------------------------------------------------------------------------
_curl() {
  curl -s --max-time 10 -w "\n%{http_code}" "$@" 2>/dev/null || printf "\n000"
}

http_get() {
  _curl "$1"
}

http_get_header() {
  # $1 = url, $2 = header value for Authorization
  _curl -H "Authorization: Bearer $2" "$1"
}

http_post() {
  # $1 = url, $2 = Content-Type, $3 = body
  _curl -X POST -H "Content-Type: $2" --data-binary "$3" "$1"
}

http_post_auth() {
  # $1 = url, $2 = Content-Type, $3 = body, $4 = token
  _curl -X POST -H "Content-Type: $2" -H "Authorization: Bearer $4" --data-binary "$3" "$1"
}

http_put() {
  # $1 = url, $2 = body
  _curl -X PUT -H "Content-Type: application/json" --data-binary "$2" "$1"
}

http_delete() {
  _curl -X DELETE "$1"
}

# Split a curl response string into body (everything except last line) and status (last line).
body_of()   { printf '%s' "$1" | sed '$d'; }
status_of() { printf '%s' "$1" | tail -n 1; }

# ---------------------------------------------------------------------------
# Assertion helpers
# ---------------------------------------------------------------------------
assert_status() {
  # $1=name  $2=expected_code  $3=curl_response
  local actual; actual=$(status_of "$3")
  if [[ "$actual" == "$2" ]]; then
    pass "$1 (HTTP $2)"
  else
    fail "$1" "HTTP $2" "HTTP $actual  ← $(body_of "$3" | head -c 200)"
  fi
}

assert_json_eq() {
  # $1=name  $2=jq_path  $3=expected_value  $4=curl_response
  local actual; actual=$(body_of "$4" | jq -r "$2" 2>/dev/null || echo "__jq_error__")
  if [[ "$actual" == "$3" ]]; then
    pass "$1"
  else
    fail "$1" "$2 == $3" "$2 == $actual"
  fi
}

assert_json_gte() {
  # $1=name  $2=jq_path  $3=minimum_int  $4=curl_response
  local actual; actual=$(body_of "$4" | jq -r "$2" 2>/dev/null || echo "0")
  # Use awk for numeric comparison so floats work too
  if awk "BEGIN{exit !($actual >= $3)}"; then
    pass "$1 ($2 = $actual ≥ $3)"
  else
    fail "$1" "$2 ≥ $3" "$2 = $actual"
  fi
}

assert_json_array_len() {
  # $1=name  $2=jq_path  $3=expected_len  $4=curl_response
  local actual; actual=$(body_of "$4" | jq -r "($2) | length" 2>/dev/null || echo "-1")
  if [[ "$actual" == "$3" ]]; then
    pass "$1 (length = $3)"
  else
    fail "$1" "$2 | length == $3" "length == $actual"
  fi
}

assert_body_contains() {
  # $1=name  $2=needle  $3=curl_response  (plain-text body, no jq)
  local body; body=$(body_of "$3")
  if echo "$body" | grep -qF "$2"; then
    pass "$1"
  else
    fail "$1" "body contains '$2'" "$(echo "$body" | head -c 200)…"
  fi
}

assert_json_array_contains_value() {
  # $1=name  $2=jq_path_to_array  $3=value  $4=curl_response
  local result; result=$(body_of "$4" | jq -r "$2" 2>/dev/null || echo "")
  if echo "$result" | grep -qF "$3"; then
    pass "$1"
  else
    fail "$1" "$2 contains '$3'" "got: $(echo "$result" | head -c 200)"
  fi
}

assert_json_array_not_contains_value() {
  # $1=name  $2=jq_path_to_array  $3=value  $4=curl_response
  local result; result=$(body_of "$4" | jq -r "$2" 2>/dev/null || echo "")
  if ! echo "$result" | grep -qF "$3"; then
    pass "$1"
  else
    fail "$1" "$2 does NOT contain '$3'" "found it: $(echo "$result" | head -c 200)"
  fi
}

# ---------------------------------------------------------------------------
# Container management
# ---------------------------------------------------------------------------
_stop_rm() {
  local name="$1"
  docker stop "$name" 2>/dev/null || true
  docker rm   "$name" 2>/dev/null || true
}

cleanup() {
  echo ""
  if [[ "$KEEP_CONTAINER" == true ]]; then
    echo -e "${YELLOW}⚠  --keep-container: leaving containers in place for inspection.${NC}"
    echo -e "   Main     : docker logs ${MAIN_CONTAINER}"
    echo -e "   Auth     : docker logs ${AUTH_CONTAINER}"
    echo -e "   Persist  : docker logs ${PERSIST_CONTAINER}"
    return
  fi
  echo "Cleaning up containers and volumes…"
  _stop_rm "$MAIN_CONTAINER"
  _stop_rm "$AUTH_CONTAINER"
  _stop_rm "$PERSIST_CONTAINER"
  docker volume rm "$PERSIST_VOLUME" 2>/dev/null || true
}

trap cleanup EXIT

# Wait until the container's HTTP server is reachable (any HTTP response counts,
# including 401 from an auth-protected container).
wait_for_ready() {
  local url="$1" label="$2"
  printf "  Waiting for %-30s" "$label…"
  local i=0
  while [[ $i -lt $READY_TIMEOUT ]]; do
    local status
    status=$(curl -s -o /dev/null -w "%{http_code}" --max-time 2 "${url}/_health" 2>/dev/null || echo "000")
    if [[ "$status" != "000" ]]; then
      echo -e " ${GREEN}ready${NC} (${i}s, HTTP ${status})"
      return 0
    fi
    printf "."
    sleep 1
    i=$((i + 1))
  done
  echo -e " ${RED}timed out after ${READY_TIMEOUT}s!${NC}"
  docker logs "$label" 2>&1 | tail -30 || true
  exit 1
}

start_main_container() {
  _stop_rm "$MAIN_CONTAINER"
  docker run -d \
    --name "$MAIN_CONTAINER" \
    -p "${MAIN_PORT}:9200" \
    -e RUST_LOG=info \
    -e EDGEWIT_COMMIT_INTERVAL_SECS=1 \
    "$IMAGE"
}

# =============================================================================
# Phase 0: Prerequisites
# =============================================================================
check_deps

# =============================================================================
# Phase 1: Build
# =============================================================================
section "Build"

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"

if [[ "$SKIP_BUILD" == true ]]; then
  echo "  Skipping docker build (--skip-build)"
  if ! docker image inspect "$IMAGE" &>/dev/null; then
    echo -e "  ${RED}Image '${IMAGE}' not found locally. Remove --skip-build or pull the image first.${NC}" >&2
    exit 1
  fi
  pass "Image '$IMAGE' available"
else
  echo "  Building ${IMAGE} from ${REPO_ROOT}…"
  if docker build -t "$IMAGE" "$REPO_ROOT" --quiet; then
    pass "docker build → $IMAGE"
  else
    fail "docker build → $IMAGE"
    exit 1
  fi
fi

# =============================================================================
# Phase 2: Start main container
# =============================================================================
section "Container startup"

start_main_container
wait_for_ready "$MAIN_URL" "$MAIN_CONTAINER"

# =============================================================================
# TEST GROUP 1: Cluster / info endpoints
# =============================================================================
section "Cluster endpoints"

resp=$(http_get "${MAIN_URL}/")
assert_status      "GET /  →  200"                        "200" "$resp"
assert_json_eq     "GET /: cluster_name = edgewit"        ".cluster_name"   "edgewit"  "$resp"
assert_json_eq     "GET /: name field present"            ".name"            "edgewit-node-1" "$resp"

resp=$(http_get "${MAIN_URL}/version")
assert_status      "GET /version  →  200"                 "200" "$resp"
assert_body_contains "GET /version: version key present"  '"version"'        "$resp"

resp=$(http_get "${MAIN_URL}/_health")
assert_status      "GET /_health  →  200"                 "200" "$resp"
assert_json_eq     "/_health: cluster_name"               ".cluster_name"    "edgewit" "$resp"
assert_json_eq     "/_health: status = green"             ".status"          "green"   "$resp"
assert_json_eq     "/_health: timed_out = false"          ".timed_out"       "false"   "$resp"
assert_json_eq     "/_health: number_of_nodes = 1"        ".number_of_nodes" "1"       "$resp"

resp=$(http_get "${MAIN_URL}/_cluster/health")
assert_status      "GET /_cluster/health (alias)  →  200" "200" "$resp"
assert_json_eq     "/_cluster/health: status = green"     ".status"          "green"   "$resp"

resp=$(http_get "${MAIN_URL}/_stats")
assert_status      "GET /_stats (fresh)  →  200"          "200" "$resp"
assert_json_eq     "/_stats: docs.count = 0 (fresh)"      "._all.primaries.docs.count" "0" "$resp"

resp=$(http_get "${MAIN_URL}/_cat/indexes")
assert_status      "GET /_cat/indexes (fresh)  →  200"    "200" "$resp"

resp=$(http_get "${MAIN_URL}/metrics")
assert_status      "GET /metrics  →  200"                 "200" "$resp"
assert_body_contains "GET /metrics: prometheus text format" "edgewit_ingest_requests_total" "$resp"

# =============================================================================
# TEST GROUP 2: Index management
# =============================================================================
section "Index management"

resp=$(http_get "${MAIN_URL}/indexes")
assert_status   "GET /indexes (empty)  →  200"  "200" "$resp"

# Build a minimal dynamic index definition
read -r -d '' INDEX_DEF <<'JSON' || true
{
  "name": "e2e-logs",
  "timestamp_field": "timestamp",
  "mode": "dynamic",
  "partition": "none",
  "compression": "zstd",
  "fields": {},
  "settings": {}
}
JSON

resp=$(http_put "${MAIN_URL}/indexes/e2e-logs" "$INDEX_DEF")
assert_status   "PUT /indexes/e2e-logs  →  200"              "200" "$resp"

resp=$(http_get "${MAIN_URL}/indexes/e2e-logs")
assert_status   "GET /indexes/e2e-logs  →  200"              "200" "$resp"
assert_json_eq  "GET /indexes/e2e-logs: name field"          ".name"            "e2e-logs"  "$resp"
assert_json_eq  "GET /indexes/e2e-logs: mode = dynamic"      ".mode"            "dynamic"   "$resp"
assert_json_eq  "GET /indexes/e2e-logs: compression = zstd"  ".compression"     "zstd"      "$resp"
assert_json_eq  "GET /indexes/e2e-logs: timestamp_field"     ".timestamp_field" "timestamp" "$resp"

resp=$(http_get "${MAIN_URL}/indexes")
assert_status   "GET /indexes (populated)  →  200"           "200" "$resp"
assert_json_array_contains_value "GET /indexes: e2e-logs listed" ".[].name" "e2e-logs" "$resp"

# Idempotent upsert — same payload a second time must succeed
resp=$(http_put "${MAIN_URL}/indexes/e2e-logs" "$INDEX_DEF")
assert_status   "PUT /indexes/e2e-logs (idempotent upsert)  →  200" "200" "$resp"

# Unknown index → 404
resp=$(http_get "${MAIN_URL}/indexes/does-not-exist")
assert_status   "GET /indexes/does-not-exist  →  404"        "404" "$resp"

# =============================================================================
# TEST GROUP 3: Document ingestion
# =============================================================================
section "Document ingestion"

resp=$(http_post "${MAIN_URL}/e2e-logs/_doc" "application/json" \
  '{"timestamp":"2024-06-01T10:00:00Z","message":"hello container world","level":"INFO","sensor":"pi-01"}')
assert_status   "POST /e2e-logs/_doc (doc 1)  →  201"      "201" "$resp"
assert_json_eq  "doc 1: _index = e2e-logs"                 "._index" "e2e-logs" "$resp"
assert_json_eq  "doc 1: result = created"                  ".result" "created"  "$resp"
assert_json_eq  "doc 1: _shards.successful = 1"            "._shards.successful" "1" "$resp"

resp=$(http_post "${MAIN_URL}/e2e-logs/_doc" "application/json" \
  '{"timestamp":"2024-06-01T10:00:01Z","message":"disk usage warning","level":"WARN","sensor":"pi-02"}')
assert_status   "POST /e2e-logs/_doc (doc 2)  →  201"      "201" "$resp"

resp=$(http_post "${MAIN_URL}/e2e-logs/_doc" "application/json" \
  '{"timestamp":"2024-06-01T10:00:02Z","message":"system shutdown requested","level":"ERROR","sensor":"pi-01"}')
assert_status   "POST /e2e-logs/_doc (doc 3)  →  201"      "201" "$resp"

# Bulk ingest – 4 more documents
BULK_BODY='{"index":{"_index":"e2e-logs"}}
{"timestamp":"2024-06-01T10:01:00Z","message":"bulk doc alpha","level":"INFO","sensor":"pi-03"}
{"index":{"_index":"e2e-logs"}}
{"timestamp":"2024-06-01T10:02:00Z","message":"bulk doc beta","level":"DEBUG","sensor":"pi-04"}
{"index":{"_index":"e2e-logs"}}
{"timestamp":"2024-06-01T10:03:00Z","message":"bulk doc gamma","level":"INFO","sensor":"pi-01"}
{"index":{"_index":"e2e-logs"}}
{"timestamp":"2024-06-01T10:04:00Z","message":"bulk doc delta","level":"WARN","sensor":"pi-02"}'

resp=$(http_post "${MAIN_URL}/_bulk" "application/x-ndjson" "$BULK_BODY")
assert_status        "POST /_bulk (4 docs)  →  200"          "200" "$resp"
assert_json_eq       "/_bulk: errors = false"                ".errors" "false" "$resp"
assert_json_array_len "/_bulk: 4 item entries in response"   ".items"  4       "$resp"

echo ""
echo "  ⏳ Waiting ${COMMIT_WAIT}s for the indexer to commit…"
sleep "$COMMIT_WAIT"

# =============================================================================
# TEST GROUP 4: Search
# =============================================================================
section "Search"

# Match-all with no parameters
resp=$(http_get "${MAIN_URL}/indexes/e2e-logs/_search")
assert_status   "GET /_search (no params)  →  200"           "200" "$resp"
assert_json_gte "/_search (no params): total ≥ 7"            ".hits.total.value" 7 "$resp"
assert_json_eq  "/_search: relation = eq"                    ".hits.total.relation" "eq" "$resp"
assert_json_eq  "/_search: timed_out = false"                ".timed_out" "false" "$resp"

# Wildcard q=*
resp=$(http_get "${MAIN_URL}/indexes/e2e-logs/_search?q=*")
assert_status   "GET /_search?q=*  →  200"                   "200" "$resp"
assert_json_gte "/_search?q=*: total ≥ 7"                    ".hits.total.value" 7 "$resp"

# Term search
resp=$(http_get "${MAIN_URL}/indexes/e2e-logs/_search?q=message:hello")
assert_status   "GET /_search?q=message:hello  →  200"       "200" "$resp"
assert_json_eq  "/_search?q=message:hello: exactly 1 hit"    ".hits.total.value" "1" "$resp"
assert_json_eq  "/_search?q=message:hello: correct message"  \
                ".hits.hits[0]._source.message" "hello container world" "$resp"

# Level filter
resp=$(http_get "${MAIN_URL}/indexes/e2e-logs/_search?q=level:WARN")
assert_status   "GET /_search?q=level:WARN  →  200"          "200" "$resp"
assert_json_gte "/_search?q=level:WARN: ≥ 2 hits"            ".hits.total.value" 2 "$resp"

# POST – match_all with size limit
resp=$(http_post "${MAIN_URL}/indexes/e2e-logs/_search" "application/json" \
  '{"query":{"match_all":{}},"size":5}')
assert_status        "POST /_search match_all size=5  →  200" "200" "$resp"
assert_json_array_len "POST /_search size=5: 5 hits returned" ".hits.hits" 5 "$resp"

# POST – match query
resp=$(http_post "${MAIN_URL}/indexes/e2e-logs/_search" "application/json" \
  '{"query":{"match":{"message":"bulk"}},"size":10}')
assert_status   "POST /_search match:bulk  →  200"           "200" "$resp"
assert_json_gte "POST /_search match:bulk: ≥ 4 hits"         ".hits.total.value" 4 "$resp"

# POST – query_string DSL
resp=$(http_post "${MAIN_URL}/indexes/e2e-logs/_search" "application/json" \
  '{"query":{"query_string":{"query":"message:shutdown"}},"size":10}')
assert_status   "POST /_search query_string:shutdown  →  200" "200" "$resp"
assert_json_eq  "POST /_search query_string: exactly 1 hit"   ".hits.total.value" "1" "$resp"

# POST – bool/must
resp=$(http_post "${MAIN_URL}/indexes/e2e-logs/_search" "application/json" \
  '{"query":{"bool":{"must":[{"match":{"level":"INFO"}}]}},"size":10}')
assert_status   "POST /_search bool/must:INFO  →  200"        "200" "$resp"
assert_json_gte "POST /_search bool/must:INFO: ≥ 2 hits"      ".hits.total.value" 2 "$resp"

# Pagination: from=0&size=2
resp=$(http_post "${MAIN_URL}/indexes/e2e-logs/_search" "application/json" \
  '{"query":{"match_all":{}},"size":2,"from":0}')
assert_status        "POST /_search from=0 size=2  →  200"    "200" "$resp"
assert_json_array_len "POST /_search pagination: 2 items"     ".hits.hits" 2 "$resp"
assert_json_gte       "POST /_search pagination: total ≥ 7"   ".hits.total.value" 7 "$resp"

# Sensor filter — documents from pi-01
resp=$(http_get "${MAIN_URL}/indexes/e2e-logs/_search?q=sensor:pi-01")
assert_status   "GET /_search?q=sensor:pi-01  →  200"         "200" "$resp"
assert_json_gte "/_search?q=sensor:pi-01: ≥ 2 hits"           ".hits.total.value" 2 "$resp"

# =============================================================================
# TEST GROUP 5: Aggregations
# =============================================================================
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
# Spread across 4 months to give date_histogram something to bucket.
AGG_BULK='{"index":{"_index":"e2e-aggs"}}
{"timestamp":"2024-01-05T12:00:00Z","amount":10.0,"category":"alpha"}
{"index":{"_index":"e2e-aggs"}}
{"timestamp":"2024-01-20T06:00:00Z","amount":11.0,"category":"beta"}
{"index":{"_index":"e2e-aggs"}}
{"timestamp":"2024-02-03T18:00:00Z","amount":12.0,"category":"alpha"}
{"index":{"_index":"e2e-aggs"}}
{"timestamp":"2024-02-14T09:00:00Z","amount":13.0,"category":"beta"}
{"index":{"_index":"e2e-aggs"}}
{"timestamp":"2024-03-01T00:00:00Z","amount":14.0,"category":"alpha"}
{"index":{"_index":"e2e-aggs"}}
{"timestamp":"2024-03-22T15:00:00Z","amount":15.0,"category":"beta"}
{"index":{"_index":"e2e-aggs"}}
{"timestamp":"2024-04-07T03:00:00Z","amount":16.0,"category":"alpha"}
{"index":{"_index":"e2e-aggs"}}
{"timestamp":"2024-04-18T21:00:00Z","amount":17.0,"category":"beta"}
{"index":{"_index":"e2e-aggs"}}
{"timestamp":"2024-05-09T11:00:00Z","amount":18.0,"category":"alpha"}
{"index":{"_index":"e2e-aggs"}}
{"timestamp":"2024-06-30T23:00:00Z","amount":19.0,"category":"beta"}'

resp=$(http_post "${MAIN_URL}/_bulk" "application/x-ndjson" "$AGG_BULK")
assert_status "POST /_bulk agg docs  →  200" "200" "$resp"

echo ""
echo "  ⏳ Waiting ${COMMIT_WAIT}s for the indexer to commit…"
sleep "$COMMIT_WAIT"

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

# --- Date histogram (monthly / 1d buckets sanity check) ---
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

# =============================================================================
# TEST GROUP 6: Stats & catalog reflect ingested data
# =============================================================================
section "Stats & catalog after ingest"

resp=$(http_get "${MAIN_URL}/_stats")
assert_status   "GET /_stats (with data)  →  200"        "200" "$resp"
assert_json_gte "/_stats: total docs ≥ 17 (7 logs + 10 aggs)" "._all.primaries.docs.count" 17 "$resp"

resp=$(http_get "${MAIN_URL}/_cat/indexes")
assert_status   "GET /_cat/indexes (with data)  →  200"  "200" "$resp"
# At least two entries: e2e-logs and e2e-aggs
CAT_LEN=$(body_of "$resp" | jq 'length' 2>/dev/null || echo 0)
if awk "BEGIN{exit !($CAT_LEN >= 2)}"; then
  pass "_cat/indexes: ≥ 2 indexes listed (got ${CAT_LEN})"
else
  fail "_cat/indexes: expected ≥ 2 indexes" "≥ 2" "$CAT_LEN"
fi

# Verify docs.count in the catalog is non-zero for e2e-logs
E2E_LOGS_COUNT=$(body_of "$resp" | jq -r '.[] | select(.index=="e2e-logs") | .["docs.count"]' 2>/dev/null || echo "0")
if awk "BEGIN{exit !($E2E_LOGS_COUNT >= 7)}"; then
  pass "_cat/indexes e2e-logs: docs.count ≥ 7 (got ${E2E_LOGS_COUNT})"
else
  fail "_cat/indexes e2e-logs: docs.count ≥ 7" "≥ 7" "$E2E_LOGS_COUNT"
fi

# =============================================================================
# TEST GROUP 7: Index deletion
# =============================================================================
section "Index deletion"

resp=$(http_delete "${MAIN_URL}/indexes/e2e-logs")
assert_status "DELETE /indexes/e2e-logs  →  200"                   "200" "$resp"

resp=$(http_get "${MAIN_URL}/indexes/e2e-logs")
assert_status "GET /indexes/e2e-logs (after delete)  →  404"       "404" "$resp"

resp=$(http_get "${MAIN_URL}/indexes")
assert_json_array_not_contains_value \
  "GET /indexes: e2e-logs no longer listed" ".[].name" "e2e-logs"  "$resp"

# Deleting a non-existent index → 404
resp=$(http_delete "${MAIN_URL}/indexes/e2e-logs")
assert_status "DELETE /indexes/e2e-logs (again)  →  404"            "404" "$resp"

# =============================================================================
# TEST GROUP 8: Authentication
# =============================================================================
section "Authentication (separate container on port ${AUTH_PORT})"

_stop_rm "$AUTH_CONTAINER"
docker run -d \
  --name "$AUTH_CONTAINER" \
  -p "${AUTH_PORT}:9200" \
  -e RUST_LOG=info \
  -e EDGEWIT_COMMIT_INTERVAL_SECS=1 \
  -e EDGEWIT_API_KEY=e2e-secret-token \
  "$IMAGE"
wait_for_ready "$AUTH_URL" "$AUTH_CONTAINER"

# No auth header → 401
resp=$(http_get "${AUTH_URL}/_health")
assert_status "No auth header  →  401"             "401" "$resp"

# Wrong token → 401
resp=$(http_get_header "${AUTH_URL}/_health" "definitely-wrong-token")
assert_status "Wrong token  →  401"                "401" "$resp"

# Malformed header (no "Bearer" prefix) → 401
resp=$(_curl -H "Authorization: e2e-secret-token" "${AUTH_URL}/_health")
assert_status "Malformed auth header  →  401"      "401" "$resp"

# Correct token on /_health → 200
resp=$(http_get_header "${AUTH_URL}/_health" "e2e-secret-token")
assert_status  "Correct token /_health  →  200"    "200" "$resp"
assert_json_eq "Correct token: status = green"     ".status" "green" "$resp"

# Ingest through the auth-protected container
resp=$(http_post_auth "${AUTH_URL}/auth-index/_doc" "application/json" \
  '{"timestamp":"2024-06-01T00:00:00Z","message":"authenticated ingest"}' \
  "e2e-secret-token")
assert_status "Authenticated POST /_doc  →  201"   "201" "$resp"

# Ingest without token → 401
resp=$(http_post "${AUTH_URL}/auth-index/_doc" "application/json" \
  '{"message":"should be rejected"}')
assert_status "Unauthenticated POST /_doc  →  401"  "401" "$resp"

# Metrics also protected
resp=$(http_get "${AUTH_URL}/metrics")
assert_status "GET /metrics without auth  →  401"   "401" "$resp"

resp=$(http_get_header "${AUTH_URL}/metrics" "e2e-secret-token")
assert_status "GET /metrics with auth  →  200"      "200" "$resp"
assert_body_contains "GET /metrics with auth: prometheus content" \
  "edgewit_ingest_requests_total" "$resp"

# Root endpoint also protected
resp=$(http_get "${AUTH_URL}/")
assert_status "GET / without auth  →  401"          "401" "$resp"

# =============================================================================
# TEST GROUP 9: Data persistence across container restarts
# =============================================================================
section "Persistence across restart (port ${PERSIST_PORT})"

_stop_rm "$PERSIST_CONTAINER"
docker volume rm "$PERSIST_VOLUME" 2>/dev/null || true
docker volume create "$PERSIST_VOLUME" > /dev/null

docker run -d \
  --name "$PERSIST_CONTAINER" \
  -p "${PERSIST_PORT}:9200" \
  -v "${PERSIST_VOLUME}:/data" \
  -e RUST_LOG=info \
  -e EDGEWIT_COMMIT_INTERVAL_SECS=1 \
  "$IMAGE"
wait_for_ready "$PERSIST_URL" "$PERSIST_CONTAINER"

# Create index and ingest two documents
read -r -d '' PERSIST_IDX_DEF <<'JSON' || true
{
  "name": "persist-test",
  "timestamp_field": "timestamp",
  "mode": "dynamic",
  "partition": "none",
  "compression": "zstd",
  "fields": {},
  "settings": {}
}
JSON

resp=$(http_put "${PERSIST_URL}/indexes/persist-test" "$PERSIST_IDX_DEF")
assert_status "Persist: PUT /indexes/persist-test  →  200" "200" "$resp"

resp=$(http_post "${PERSIST_URL}/persist-test/_doc" "application/json" \
  '{"timestamp":"2024-06-01T12:00:00Z","message":"persisted doc one","level":"INFO"}')
assert_status "Persist: ingest doc 1  →  201" "201" "$resp"

resp=$(http_post "${PERSIST_URL}/persist-test/_doc" "application/json" \
  '{"timestamp":"2024-06-01T12:00:01Z","message":"persisted doc two","level":"WARN"}')
assert_status "Persist: ingest doc 2  →  201" "201" "$resp"

echo ""
echo "  ⏳ Waiting ${COMMIT_WAIT}s for indexer to commit before restart…"
sleep "$COMMIT_WAIT"

resp=$(http_get "${PERSIST_URL}/indexes/persist-test/_search")
assert_status   "Persist: search before restart  →  200"   "200" "$resp"
assert_json_gte "Persist: 2 docs visible before restart"   ".hits.total.value" 2 "$resp"

# Stop and remove the container, then restart it with the same volume
echo ""
echo "  Restarting container with the same data volume…"
docker stop "$PERSIST_CONTAINER"
docker rm   "$PERSIST_CONTAINER"

docker run -d \
  --name "$PERSIST_CONTAINER" \
  -p "${PERSIST_PORT}:9200" \
  -v "${PERSIST_VOLUME}:/data" \
  -e RUST_LOG=info \
  "$IMAGE"
wait_for_ready "$PERSIST_URL" "$PERSIST_CONTAINER"

resp=$(http_get "${PERSIST_URL}/indexes/persist-test/_search")
assert_status   "Persist: search after restart  →  200"    "200" "$resp"
assert_json_gte "Persist: 2 docs survive restart"          ".hits.total.value" 2 "$resp"
assert_json_eq  "Persist: index still registered"          ".hits.total.relation" "eq" "$resp"

# Ingest more data after restart to confirm WAL is writable
resp=$(http_post "${PERSIST_URL}/persist-test/_doc" "application/json" \
  '{"timestamp":"2024-06-02T08:00:00Z","message":"post-restart doc","level":"INFO"}')
assert_status "Persist: ingest after restart  →  201" "201" "$resp"

echo ""
echo "  ⏳ Waiting ${COMMIT_WAIT}s for post-restart commit…"
sleep "$COMMIT_WAIT"

resp=$(http_get "${PERSIST_URL}/indexes/persist-test/_search")
assert_json_gte "Persist: 3 docs after post-restart ingest" ".hits.total.value" 3 "$resp"

# Cleanup persistence resources
_stop_rm "$PERSIST_CONTAINER"
docker volume rm "$PERSIST_VOLUME" 2>/dev/null || true

# =============================================================================
# Final summary
# =============================================================================
echo ""
echo -e "${BOLD}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
echo -e "${BOLD} Test Summary${NC}"
echo -e "${BOLD}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
echo -e "  ${GREEN}Passed : ${PASS}${NC}"

if [[ $FAIL -gt 0 ]]; then
  echo -e "  ${RED}Failed : ${FAIL}${NC}"
  echo ""
  echo -e "${RED}${BOLD}Failed tests:${NC}"
  for t in "${FAILED_TESTS[@]}"; do
    echo -e "  ${RED}•${NC} $t"
  done
  echo ""
  exit 1
else
  echo -e "  ${GREEN}Failed : 0${NC}"
  echo ""
  echo -e "${GREEN}${BOLD}All ${PASS} tests passed ✔${NC}"
fi
