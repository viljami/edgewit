#!/usr/bin/env bash
# =============================================================================
# E2E Group 02 — Index management
# =============================================================================
# This script is intended to be sourced by the main test runner. It assumes
# the shared helpers from `scripts/e2e/00-helpers.sh` are already sourced and
# that the following variables are available in the environment:
#   - MAIN_URL
#   - MAIN_CONTAINER (optional; used by diagnostics)
#
# The script must not call `exit` — use `pass` / `fail` helpers to report.
# =============================================================================

# Bail quietly if helpers aren't present so the runner can continue with other groups.
if ! declare -f http_put >/dev/null 2>&1 || ! declare -f assert_status >/dev/null 2>&1; then
  echo "helpers not loaded: skipping 02-indexes.sh" >&2
  return 0
fi

section "Index management"

# List indexes (should be empty initially)
resp=$(http_get "${MAIN_URL}/indexes")
assert_status "GET /indexes (empty)  →  200" "200" "$resp"

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

# Create index
resp=$(http_put "${MAIN_URL}/indexes/e2e-logs" "$INDEX_DEF")
assert_status "PUT /indexes/e2e-logs  →  200" "200" "$resp"

# Retrieve the index and check fields
resp=$(http_get "${MAIN_URL}/indexes/e2e-logs")
assert_status "GET /indexes/e2e-logs  →  200" "200" "$resp"
assert_json_eq "GET /indexes/e2e-logs: name field" ".name" "e2e-logs" "$resp"
assert_json_eq "GET /indexes/e2e-logs: mode = dynamic" ".mode" "dynamic" "$resp"
assert_json_eq "GET /indexes/e2e-logs: compression = zstd" ".compression" "zstd" "$resp"
assert_json_eq "GET /indexes/e2e-logs: timestamp_field" ".timestamp_field" "timestamp" "$resp"

# List indexes should include the new index
resp=$(http_get "${MAIN_URL}/indexes")
assert_status "GET /indexes (populated)  →  200" "200" "$resp"
assert_json_array_contains_value "GET /indexes: e2e-logs listed" ".[].name" "e2e-logs" "$resp"

# Idempotent upsert — same payload a second time must succeed
resp=$(http_put "${MAIN_URL}/indexes/e2e-logs" "$INDEX_DEF")
assert_status "PUT /indexes/e2e-logs (idempotent upsert)  →  200" "200" "$resp"

# Unknown index → 404
resp=$(http_get "${MAIN_URL}/indexes/does-not-exist")
assert_status "GET /indexes/does-not-exist  →  404" "404" "$resp"

# Completed group
return 0
