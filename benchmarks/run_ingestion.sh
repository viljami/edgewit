#!/bin/bash
set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DATA_DIR="$SCRIPT_DIR/data"
BULK_JSON_FILE="$DATA_DIR/http_logs_bulk.ndjson"
CHUNK_FILE="$DATA_DIR/http_logs_chunk.ndjson"

EDGEWIT_URL="http://localhost:9200/_bulk"
OPENSEARCH_URL="http://localhost:9201/_bulk"

echo "==> Ingestion Benchmark (Phase 2) <=="

# Ensure wrk is installed
if ! command -v wrk &> /dev/null; then
    echo "Error: 'wrk' is not installed."
    echo "Install it via: brew install wrk"
    exit 1
fi

# Ensure dataset exists
if [ ! -f "$BULK_JSON_FILE" ]; then
    echo "Dataset not found. Please run download_dataset.sh first."
    exit 1
fi

# Create a representative chunk (e.g., 5000 documents = 10000 lines because of the index metadata line)
# This will be used as the payload to hammer the servers
DOCS_PER_REQUEST=5000
LINES_PER_REQUEST=$((DOCS_PER_REQUEST * 2))

if [ ! -f "$CHUNK_FILE" ]; then
    echo "Creating a $DOCS_PER_REQUEST document chunk for load testing..."
    head -n "$LINES_PER_REQUEST" "$BULK_JSON_FILE" > "$CHUNK_FILE"
fi

CHUNK_SIZE=$(du -h "$CHUNK_FILE" | cut -f1)
echo "Payload chunk size: $CHUNK_SIZE ($DOCS_PER_REQUEST docs/request)"
echo "------------------------------------------------"

# Create a Lua script for wrk to send POST requests with the chunk file body
WRK_SCRIPT="$DATA_DIR/post.lua"
cat <<EOF > "$WRK_SCRIPT"
wrk.method = "POST"
wrk.headers["Content-Type"] = "application/x-ndjson"
local f = io.open("$CHUNK_FILE", "rb")
if f then
  wrk.body = f:read("*all")
  f:close()
end
EOF

# Function to run benchmark
run_benchmark() {
    local name=$1
    local url=$2
    local concurrency=$3
    local duration=$4

    echo "Running benchmark against $name ($url)..."
    echo "Concurrency: $concurrency, Duration: $duration"

    # Check if target is up
    if ! curl -s "$url" > /dev/null; then
        echo "Warning: $name might not be running at $url"
        echo "Make sure you started docker-compose.benchmark.yml"
        echo ""
    fi

    # Run wrk
    # -t: Threads
    # -c: Connections
    # -d: Duration
    # -s: Script

    wrk -t 2 -c "$concurrency" -d "$duration" -s "$WRK_SCRIPT" "$url"

    echo "------------------------------------------------"
}

# Run the tests
# We use a moderate concurrency (e.g., 10 concurrent clients) for 30 seconds
CONCURRENCY=10
DURATION="30s"

echo "Starting Edgewit Benchmark..."
run_benchmark "Edgewit" "$EDGEWIT_URL" "$CONCURRENCY" "$DURATION"

echo "Starting OpenSearch Benchmark..."
run_benchmark "OpenSearch" "$OPENSEARCH_URL" "$CONCURRENCY" "$DURATION"

echo "Benchmark complete."
echo "Compare the 'Requests/sec' to calculate Documents/sec."
echo "Formula: (Requests/sec) * $DOCS_PER_REQUEST = Ingestion Throughput (Docs/sec)"
