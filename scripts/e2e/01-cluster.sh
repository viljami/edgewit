#!/usr/bin/env bash
# =============================================================================
# E2E Group 01 — Cluster endpoints
# =============================================================================
# This script is intended to be sourced by the main test runner. It assumes
# the shared helpers from `scripts/e2e/00-helpers.sh` are already sourced and
# that the following variables are available in the environment:
#   - MAIN_URL
#   - MAIN_CONTAINER (optional; used by diagnostics)
#
# The script must not call `exit` — use `pass` / `fail` helpers to report.
# =============================================================================

# When sourced, prefer to bail quietly if helpers aren't present so the
# runner can continue with other groups.
if ! declare -f http_get >/dev/null 2>&1 || ! declare -f assert_status >/dev/null 2>&1; then
  echo "helpers not loaded: skipping 01-cluster.sh" >&2
  return 0
fi

# Human-friendly section header (helpers provide this)
section "Cluster endpoints"

# GET /  — basic cluster info
resp=$(http_get "${MAIN_URL}/")
assert_status      "GET /  →  200"                        "200" "$resp"
assert_json_eq     "GET /: cluster_name = edgewit"        ".cluster_name"   "edgewit"  "$resp"
assert_json_eq     "GET /: name field present"            ".name"           "edgewit-node-1" "$resp"

# GET /version — should include version key
resp=$(http_get "${MAIN_URL}/version")
assert_status      "GET /version  →  200"                 "200" "$resp"
assert_body_contains "GET /version: version key present"  '"version"'        "$resp"

# GET /_health — cluster health
resp=$(http_get "${MAIN_URL}/_health")
assert_status      "GET /_health  →  200"                 "200" "$resp"
assert_json_eq     "/_health: cluster_name"               ".cluster_name"    "edgewit" "$resp"
assert_json_eq     "/_health: status = green"             ".status"          "green"   "$resp"
assert_json_eq     "/_health: timed_out = false"          ".timed_out"       "false"   "$resp"
assert_json_eq     "/_health: number_of_nodes = 1"        ".number_of_nodes" "1"       "$resp"

# alias endpoint
resp=$(http_get "${MAIN_URL}/_cluster/health")
assert_status      "GET /_cluster/health (alias)  →  200" "200" "$resp"
assert_json_eq     "/_cluster/health: status = green"     ".status"          "green"   "$resp"

# Stats (fresh cluster)
resp=$(http_get "${MAIN_URL}/_stats")
assert_status      "GET /_stats (fresh)  →  200"          "200" "$resp"
assert_json_eq     "/_stats: docs.count = 0 (fresh)"      "._all.primaries.docs.count" "0" "$resp"

# _cat/indexes should respond (format is JSON in this test suite)
resp=$(http_get "${MAIN_URL}/_cat/indexes")
assert_status      "GET /_cat/indexes (fresh)  →  200"    "200" "$resp"

# Metrics endpoint exposes prometheus text metrics (look for a known metric)
resp=$(http_get "${MAIN_URL}/metrics")
assert_status      "GET /metrics  →  200"                 "200" "$resp"
assert_body_contains "GET /metrics: prometheus text format" "edgewit_ingest_requests_total" "$resp"

# Completed group — do not exit; the runner will continue to the next group.
return 0
