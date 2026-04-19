#!/usr/bin/env bash
# =============================================================================
# E2E Group 03 — Document ingestion
# =============================================================================
# This script is intended to be sourced by the main test runner. It assumes
# the shared helpers from `scripts/e2e/00-helpers.sh` are already sourced and
# that the following variables are available in the environment:
#   - MAIN_URL
#   - MAIN_CONTAINER (optional; used by diagnostics)
#   - COMMIT_WAIT
#
# The script must not call `exit` — use `pass` / `fail` helpers to report.
# =============================================================================

# Bail quietly if helpers aren't present so the runner can continue with other groups.
if ! declare -f http_post >/dev/null 2>&1 || ! declare -f assert_status >/dev/null 2>&1; then
  echo "helpers not loaded: skipping 03-ingest.sh" >&2
  return 0
fi

section "Document ingestion"

# Doc 1
resp=$(http_post "${MAIN_URL}/e2e-logs/_doc" "application/json" \
  '{"timestamp":"2024-06-01T10:00:00Z","message":"hello container world","level":"INFO","sensor":"pi-01"}')
assert_status   "POST /e2e-logs/_doc (doc 1)  →  201"      "201" "$resp"
# Validate response fields for doc 1 (if present)
assert_json_eq  "doc 1: _index = e2e-logs"                 "._index" "e2e-logs" "$resp"
assert_json_eq  "doc 1: result = created"                  ".result" "created"  "$resp"
assert_json_eq  "doc 1: _shards.successful = 1"            "._shards.successful" "1" "$resp"

# Doc 2
resp=$(http_post "${MAIN_URL}/e2e-logs/_doc" "application/json" \
  '{"timestamp":"2024-06-01T10:00:01Z","message":"disk usage warning","level":"WARN","sensor":"pi-02"}')
assert_status   "POST /e2e-logs/_doc (doc 2)  →  201"      "201" "$resp"

# Doc 3
resp=$(http_post "${MAIN_URL}/e2e-logs/_doc" "application/json" \
  '{"timestamp":"2024-06-01T10:00:02Z","message":"system shutdown requested","level":"ERROR","sensor":"pi-01"}')
assert_status   "POST /e2e-logs/_doc (doc 3)  →  201"      "201" "$resp"

# Bulk ingest – 4 more documents (NDJSON)
BULK_BODY=$'{"index":{"_index":"e2e-logs"}}\n{"timestamp":"2024-06-01T10:01:00Z","message":"bulk doc alpha","level":"INFO","sensor":"pi-03"}\n{"index":{"_index":"e2e-logs"}}\n{"timestamp":"2024-06-01T10:02:00Z","message":"bulk doc beta","level":"DEBUG","sensor":"pi-04"}\n{"index":{"_index":"e2e-logs"}}\n{"timestamp":"2024-06-01T10:03:00Z","message":"bulk doc gamma","level":"INFO","sensor":"pi-01"}\n{"index":{"_index":"e2e-logs"}}\n{"timestamp":"2024-06-01T10:04:00Z","message":"bulk doc delta","level":"WARN","sensor":"pi-02"}'

resp=$(http_post "${MAIN_URL}/_bulk" "application/x-ndjson" "$BULK_BODY")
assert_status        "POST /_bulk (4 docs)  →  200"          "200" "$resp"
assert_json_eq       "/_bulk: errors = false"                ".errors" "false" "$resp"
assert_json_array_len "/_bulk: 4 item entries in response"   ".items"  4       "$resp"

echo ""
echo "  ⏳ Waiting ${COMMIT_WAIT:-3}s for the indexer to commit…"
sleep "${COMMIT_WAIT:-3}"

# Completed group
return 0
