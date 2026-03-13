#!/bin/bash
set -e

# Directory setup
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DATA_DIR="$SCRIPT_DIR/data"
mkdir -p "$DATA_DIR"

echo "==> Edgewit Benchmark Dataset Preparation <=="
echo "This script downloads the 'http_logs' dataset used by OpenSearch Rally."
echo "Target directory: $DATA_DIR"
echo "------------------------------------------------"

# URLs for Rally corpora (Elasticsearch/OpenSearch standard benchmarks)
RALLY_HTTP_LOGS_URL="http://benchmarks.elasticsearch.org.s3.amazonaws.com/corpora/http_logs/documents-181998.json.bz2"

ARCHIVE_FILE="$DATA_DIR/http_logs.json.bz2"
RAW_JSON_FILE="$DATA_DIR/http_logs.json"
BULK_JSON_FILE="$DATA_DIR/http_logs_bulk.ndjson"

# Step 1: Download
if [ ! -f "$RAW_JSON_FILE" ]; then
    if [ ! -f "$ARCHIVE_FILE" ]; then
        echo "Downloading http_logs dataset..."
        echo "Note: The compressed file is ~1.2GB. This might take a few minutes."
        curl -L -o "$ARCHIVE_FILE" "$RALLY_HTTP_LOGS_URL"
    else
        echo "Compressed archive $ARCHIVE_FILE already exists."
    fi

    # Step 2: Decompress
    echo "Decompressing dataset (uncompresses to ~30GB, so we will stream and extract a subset instead of fully decompressing)..."
    # We will use bzcat to stream the extraction so we don't need 30GB of local disk space just to test
else
    echo "Raw JSON dataset $RAW_JSON_FILE already exists."
fi

# Step 3: Format for /_bulk API
# Standard Elasticsearch /_bulk API requires an action metadata line before every document.
# E.g.:
# {"index": {"_index": "http_logs"}}
# {"@timestamp": "...", "clientip": "..."}

SUBSET_DOCS=100000
echo "Creating a $SUBSET_DOCS document subset formatted for /_bulk API..."

if [ -f "$RAW_JSON_FILE" ]; then
    # If the user fully uncompressed it previously
    head -n "$SUBSET_DOCS" "$RAW_JSON_FILE"
else
    # Stream directly from the bzip2 archive to save space
    bzcat "$ARCHIVE_FILE" | head -n "$SUBSET_DOCS"
fi | awk '
{
    print "{\"index\": {\"_index\": \"http_logs\"}}"
    print $0
}' > "$BULK_JSON_FILE"

echo "------------------------------------------------"
echo "Done! Benchmark dataset ready at:"
echo "$BULK_JSON_FILE"
echo "File size: $(du -h "$BULK_JSON_FILE" | cut -f1)"
echo "Total documents: $SUBSET_DOCS"
