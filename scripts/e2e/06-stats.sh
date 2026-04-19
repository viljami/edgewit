#!/usr/bin/env bash
# =============================================================================
# E2E Group 06 — Stats & catalog reflect ingested data
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
if ! declare -f http_get >/dev/null 2>&1 || ! declare -f assert_status >/dev/null 2>&1; then
  echo "helpers not loaded: skipping 06-stats.sh" >&2
  return 0
fi

section "Stats & catalog after ingest"

# GET /_stats — check overall documents count
resp=$(http_get "${MAIN_URL}/_stats")
assert_status   "GET /_stats (with data)  →  200"        "200" "$resp"
assert_json_gte "/_stats: total docs ≥ 17 (7 logs + 10 aggs)" "._all.primaries.docs.count" 17 "$resp"

# GET /_cat/indexes — list indexes and document counts
resp=$(http_get "${MAIN_URL}/_cat/indexes")
assert_status   "GET /_cat/indexes (with data)  →  200"  "200" "$resp"

# At least two entries: e2e-logs and e2e-aggs
CAT_LEN=$(body_of "$resp" | jq 'length' 2>/dev/null || echo 0)
# sanitize CAT_LEN to a non-empty integer
CAT_LEN="${CAT_LEN:-0}"
CAT_LEN="${CAT_LEN//[^0-9]/}"
CAT_LEN=${CAT_LEN:-0}
if [ "$CAT_LEN" -ge 2 ]; then
  pass "_cat/indexes: ≥ 2 indexes listed (got ${CAT_LEN})"
else
  fail "_cat/indexes: expected ≥ 2 indexes" "≥ 2" "$CAT_LEN"
fi

# Verify docs.count in the catalog is non-zero for e2e-logs
E2E_LOGS_COUNT=$(body_of "$resp" | jq -r '.[] | select(.index=="e2e-logs") | .["docs.count"]' 2>/dev/null || echo "0")
# sanitize E2E_LOGS_COUNT to an integer (strip non-digits)
E2E_LOGS_COUNT="${E2E_LOGS_COUNT:-0}"
E2E_LOGS_COUNT="${E2E_LOGS_COUNT//[^0-9]/}"
E2E_LOGS_COUNT=${E2E_LOGS_COUNT:-0}
if [ "$E2E_LOGS_COUNT" -ge 7 ]; then
  pass "_cat/indexes e2e-logs: docs.count ≥ 7 (got ${E2E_LOGS_COUNT})"
else
  fail "_cat/indexes e2e-logs: docs.count ≥ 7" "≥ 7" "$E2E_LOGS_COUNT"
fi

# Completed group
return 0
