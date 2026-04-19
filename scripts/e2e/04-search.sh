#!/usr/bin/env bash
# =============================================================================
# E2E Group 04 — Search
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
if ! declare -f http_get >/dev/null 2>&1 || ! declare -f http_post >/dev/null 2>&1 || ! declare -f assert_status >/dev/null 2>&1; then
  echo "helpers not loaded: skipping 04-search.sh" >&2
  return 0
fi

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
resp=$(http_get "${MAIN_URL}/indexes/e2e-logs/_search?q=_dynamic:hello")
assert_status   "GET /_search?q=_dynamic:hello  →  200"       "200" "$resp"
assert_json_eq  "/_search?q=_dynamic:hello: exactly 1 hit"    ".hits.total.value" "1" "$resp"
assert_json_eq  "/_search?q=_dynamic:hello: correct message"  ".hits.hits[0]._source.message" "hello container world" "$resp"

# Level filter
resp=$(http_get "${MAIN_URL}/indexes/e2e-logs/_search?q=_dynamic:WARN")
assert_status   "GET /_search?q=_dynamic:WARN  →  200"          "200" "$resp"
assert_json_gte "/_search?q=_dynamic:WARN: ≥ 2 hits"            ".hits.total.value" 2 "$resp"

# POST – match_all with size limit
resp=$(http_post "${MAIN_URL}/indexes/e2e-logs/_search" "application/json" '{"query":{"match_all":{}},"size":5}')
assert_status        "POST /_search match_all size=5  →  200" "200" "$resp"
assert_json_array_len "POST /_search size=5: 5 hits returned" ".hits.hits" 5 "$resp"

# POST – match query
resp=$(http_post "${MAIN_URL}/indexes/e2e-logs/_search" "application/json" '{"query":{"match":{"_dynamic":"bulk"}},"size":10}')
assert_status   "POST /_search match:bulk  →  200"           "200" "$resp"
assert_json_gte "POST /_search match:bulk: ≥ 4 hits"         ".hits.total.value" 4 "$resp"

# POST – query_string DSL
resp=$(http_post "${MAIN_URL}/indexes/e2e-logs/_search" "application/json" '{"query":{"query_string":{"query":"_dynamic:shutdown"}},"size":10}')
assert_status   "POST /_search query_string:shutdown  →  200" "200" "$resp"
assert_json_eq  "POST /_search query_string: exactly 1 hit"   ".hits.total.value" "1" "$resp"

# POST – bool/must
resp=$(http_post "${MAIN_URL}/indexes/e2e-logs/_search" "application/json" '{"query":{"bool":{"must":[{"match":{"_dynamic":"INFO"}}]}},"size":10}')
assert_status   "POST /_search bool/must:INFO  →  200"        "200" "$resp"
assert_json_gte "POST /_search bool/must:INFO: ≥ 2 hits"      ".hits.total.value" 2 "$resp"

# Pagination: from=0&size=2
resp=$(http_post "${MAIN_URL}/indexes/e2e-logs/_search" "application/json" '{"query":{"match_all":{}},"size":2,"from":0}')
assert_status        "POST /_search from=0 size=2  →  200"    "200" "$resp"
assert_json_array_len "POST /_search pagination: 2 items"     ".hits.hits" 2 "$resp"
assert_json_gte       "POST /_search pagination: total ≥ 7"   ".hits.total.value" 7 "$resp"

# Sensor filter — documents from pi-01
resp=$(http_get "${MAIN_URL}/indexes/e2e-logs/_search?q=_dynamic:pi-01")
assert_status   "GET /_search?q=_dynamic:pi-01  →  200"         "200" "$resp"
assert_json_gte "/_search?q=_dynamic:pi-01: ≥ 2 hits"           ".hits.total.value" 2 "$resp"

# Completed group
return 0
