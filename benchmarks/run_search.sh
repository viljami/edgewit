#!/bin/bash
set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DATA_DIR="$SCRIPT_DIR/data"

EDGEWIT_URL="http://localhost:9200/http_logs/_search"
OPENSEARCH_URL="http://localhost:9201/http_logs/_search"

echo "==> Search & Aggregation Benchmark (Phase 3) <=="

# Ensure wrk is installed
if ! command -v wrk &> /dev/null; then
    echo "Error: 'wrk' is not installed."
    echo "Install it via: brew install wrk"
    exit 1
fi

# Concurrency and duration
CONCURRENCY=10
DURATION="30s"
THREADS=2

echo "Concurrency: $CONCURRENCY, Duration: $DURATION"
echo "------------------------------------------------"

# 1. Match All (GET)
run_get_benchmark() {
    local name=$1
    local url=$2
    local query_name=$3

    echo "Running [$query_name] against $name ($url)..."

    if ! curl -s "$url" > /dev/null; then
        echo "Warning: $name might not be running or index might not exist."
    fi

    wrk -t "$THREADS" -c "$CONCURRENCY" -d "$DURATION" "$url"
    echo "------------------------------------------------"
}

# 2. Aggregations (POST)
run_post_benchmark() {
    local name=$1
    local url=$2
    local query_name=$3
    local payload_file=$4

    echo "Running [$query_name] against $name ($url)..."

    # Create Lua script for wrk POST
    local lua_script="$DATA_DIR/search_post.lua"
    cat <<EOF > "$lua_script"
wrk.method = "POST"
wrk.headers["Content-Type"] = "application/json"
local f = io.open("$payload_file", "rb")
if f then
  wrk.body = f:read("*all")
  f:close()
end
EOF

    wrk -t "$THREADS" -c "$CONCURRENCY" -d "$DURATION" -s "$lua_script" "$url"
    echo "------------------------------------------------"
}

# --- Test 1: Match All ---
echo ">>> Test 1: Match All (Baseline Overhead) <<<"
run_get_benchmark "Edgewit" "$EDGEWIT_URL" "Match All"
run_get_benchmark "OpenSearch" "$OPENSEARCH_URL" "Match All"

# --- Test 2: Term Search ---
echo ">>> Test 2: Term Search (Full-text lookup for 'GET') <<<"
# Assuming 'GET' is a common HTTP method in the logs
TERM_EDGEWIT_URL="${EDGEWIT_URL}?q=GET"
TERM_OPENSEARCH_URL="${OPENSEARCH_URL}?q=GET"

run_get_benchmark "Edgewit" "$TERM_EDGEWIT_URL" "Term Search"
run_get_benchmark "OpenSearch" "$TERM_OPENSEARCH_URL" "Term Search"

# --- Test 3: Aggregations ---
echo ">>> Test 3: Aggregations (Terms agg on status codes) <<<"
AGG_PAYLOAD="$DATA_DIR/agg_payload.json"
cat <<EOF > "$AGG_PAYLOAD"
{
  "size": 0,
  "aggs": {
    "status_codes": {
      "terms": {
        "field": "response"
      }
    }
  }
}
EOF

run_post_benchmark "Edgewit" "$EDGEWIT_URL" "Terms Aggregation" "$AGG_PAYLOAD"
run_post_benchmark "OpenSearch" "$OPENSEARCH_URL" "Terms Aggregation" "$AGG_PAYLOAD"

echo "Benchmark complete."
