#!/usr/bin/env bash
# =============================================================================
# E2E Group 08 — Authentication
# =============================================================================
# This script is intended to be sourced by the main test runner. It assumes
# the shared helpers from `scripts/e2e/00-helpers.sh` are already sourced and
# that the following variables are available in the environment:
#   - AUTH_URL
#   - AUTH_CONTAINER
#   - AUTH_PORT
#   - IMAGE
#   - READY_TIMEOUT (optional)
#   - COMMIT_WAIT (optional)
#
# The script must not call `exit` — use `pass` / `fail` helpers to report.
# =============================================================================

# Bail quietly if helpers aren't present so the runner can continue with other groups.
if ! declare -f http_get >/dev/null 2>&1 || ! declare -f http_post >/dev/null 2>&1 || ! declare -f assert_status >/dev/null 2>&1; then
  echo "helpers not loaded: skipping 08-auth.sh" >&2
  return 0
fi

section "Authentication (separate container on port ${AUTH_PORT})"

# Ensure any old auth container is stopped/removed
docker stop "${AUTH_CONTAINER}" 2>/dev/null || true
docker rm   "${AUTH_CONTAINER}" 2>/dev/null || true

# Start auth-protected instance
docker run -d \
  --name "${AUTH_CONTAINER}" \
  -p "${AUTH_PORT}:9200" \
  -e RUST_LOG=info \
  -e EDGEWIT_COMMIT_INTERVAL_SECS=1 \
  -e EDGEWIT_API_KEY=e2e-secret-token \
  "${IMAGE}" >/dev/null

# Wait until the auth container is reachable
READY_TO="${READY_TIMEOUT:-60}"
i=0
while [[ $i -lt $READY_TO ]]; do
  resp=$(http_get "${AUTH_URL}/_health")
  code=$(status_of "$resp")
  if [[ "$code" != "000" ]]; then
    pass "Waiting for ${AUTH_CONTAINER} ready (HTTP ${code})"
    break
  fi
  printf "."
  sleep 1
  i=$((i + 1))
done
if [[ $i -ge $READY_TO ]]; then
  fail "Auth container did not become ready" "HTTP != 000" "timed out after ${READY_TO}s"
  echo "Last ${AUTH_CONTAINER} logs:"
  docker logs "${AUTH_CONTAINER}" 2>&1 | tail -n 200 || true
  # Do not exit runner; return so the runner can report summary
  return 0
fi

# No auth header → 401
resp=$(http_get "${AUTH_URL}/_health")
assert_status "No auth header  →  401" "401" "$resp"

# Wrong token → 401
resp=$(http_get_header "${AUTH_URL}/_health" "definitely-wrong-token")
assert_status "Wrong token  →  401" "401" "$resp"

# Malformed header (no "Bearer" prefix) → 401
# use the lower-level _curl (helpers provide it) to craft malformed header
resp=$(_curl -H "Authorization: e2e-secret-token" "${AUTH_URL}/_health")
assert_status "Malformed auth header  →  401" "401" "$resp"

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

# Leave auth container running for inspection (cleanup will handle removal)
return 0
