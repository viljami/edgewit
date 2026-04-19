#!/usr/bin/env bash
# =============================================================================
# E2E Group 09 — Persistence across restart
# =============================================================================
# This script is intended to be sourced by the main test runner. It assumes
# the shared helpers from `scripts/e2e/00-helpers.sh` are already sourced and
# that the following variables are available in the environment:
#   - PERSIST_URL
#   - PERSIST_CONTAINER
#   - PERSIST_VOLUME
#   - PERSIST_PORT
#   - IMAGE
#   - COMMIT_WAIT (optional)
#
# The script must not call `exit` — use `pass` / `fail` helpers to report.
# =============================================================================

# Bail quietly if helpers aren't present so the runner can continue with other groups.
if ! declare -f http_put >/dev/null 2>&1 || ! declare -f http_post >/dev/null 2>&1 || ! declare -f http_get >/dev/null 2>&1; then
  echo "helpers not loaded: skipping 09-persist.sh" >&2
  return 0
fi

section "Persistence across restart (port ${PERSIST_PORT})"

# Ensure any previous container/volume are cleaned up so test is deterministic
docker stop "${PERSIST_CONTAINER}" 2>/dev/null || true
docker rm   "${PERSIST_CONTAINER}" 2>/dev/null || true
docker volume rm "${PERSIST_VOLUME}" 2>/dev/null || true
docker volume create "${PERSIST_VOLUME}" > /dev/null || true

# Start container with persistent volume mounted
docker run -d \
  --name "${PERSIST_CONTAINER}" \
  -p "${PERSIST_PORT}:9200" \
  -v "${PERSIST_VOLUME}:/data" \
  -e RUST_LOG=info \
  -e EDGEWIT_COMMIT_INTERVAL_SECS=1 \
  "${IMAGE}" >/dev/null

# Wait until the container is reachable. Prefer runner-provided wait_for_ready if present.
if declare -f wait_for_ready >/dev/null 2>&1; then
  wait_for_ready "${PERSIST_URL}" "${PERSIST_CONTAINER}"
else
  # Fallback: poll /_health
  i=0
  READY_TO="${READY_TIMEOUT:-60}"
  while [[ $i -lt $READY_TO ]]; do
    resp=$(_curl --max-time 2 "${PERSIST_URL}/_health")
    code=$(status_of "$resp")
    if [[ "$code" != "000" ]]; then
      pass "Persist container ready (HTTP ${code})"
      break
    fi
    printf "."
    sleep 1
    i=$((i+1))
  done
  if [[ $i -ge $READY_TO ]]; then
    fail "Persist container did not become ready" "HTTP != 000" "timed out after ${READY_TO}s"
    docker logs "${PERSIST_CONTAINER}" 2>&1 | tail -n 200 || true
    return 0
  fi
fi

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
echo "  ⏳ Waiting ${COMMIT_WAIT:-3}s for indexer to commit before restart…"
sleep "${COMMIT_WAIT:-3}"

resp=$(http_get "${PERSIST_URL}/indexes/persist-test/_search")
assert_status   "Persist: search before restart  →  200"   "200" "$resp"
assert_json_gte "Persist: 2 docs visible before restart"   ".hits.total.value" 2 "$resp"

# Stop and remove the container, then restart it with the same volume
echo ""
echo "  Restarting container with the same data volume…"
docker stop "${PERSIST_CONTAINER}" 2>/dev/null || true
docker rm   "${PERSIST_CONTAINER}" 2>/dev/null || true

docker run -d \
  --name "${PERSIST_CONTAINER}" \
  -p "${PERSIST_PORT}:9200" \
  -v "${PERSIST_VOLUME}:/data" \
  -e RUST_LOG=info \
  -e EDGEWIT_COMMIT_INTERVAL_SECS=1 \
  "${IMAGE}" >/dev/null

# Wait again for readiness
if declare -f wait_for_ready >/dev/null 2>&1; then
  wait_for_ready "${PERSIST_URL}" "${PERSIST_CONTAINER}"
else
  i=0
  READY_TO="${READY_TIMEOUT:-60}"
  while [[ $i -lt $READY_TO ]]; do
    resp=$(_curl --max-time 2 "${PERSIST_URL}/_health")
    code=$(status_of "$resp")
    if [[ "$code" != "000" ]]; then
      pass "Persist container ready after restart (HTTP ${code})"
      break
    fi
    printf "."
    sleep 1
    i=$((i+1))
  done
  if [[ $i -ge $READY_TO ]]; then
    fail "Persist container did not become ready after restart" "HTTP != 000" "timed out after ${READY_TO}s"
    docker logs "${PERSIST_CONTAINER}" 2>&1 | tail -n 200 || true
    return 0
  fi
fi

# Verify documents survived restart
resp=$(http_get "${PERSIST_URL}/indexes/persist-test/_search")
assert_status   "Persist: search after restart  →  200"    "200" "$resp"
assert_json_gte "Persist: 2 docs survive restart"          ".hits.total.value" 2 "$resp"
assert_json_eq  "Persist: index still registered"          ".hits.total.relation" "eq" "$resp"

# Ingest more data after restart to confirm WAL is writable
resp=$(http_post "${PERSIST_URL}/persist-test/_doc" "application/json" \
  '{"timestamp":"2024-06-02T08:00:00Z","message":"post-restart doc","level":"INFO"}')
assert_status "Persist: ingest after restart  →  201" "201" "$resp"

echo ""
echo "  ⏳ Waiting ${COMMIT_WAIT:-3}s for post-restart commit…"
sleep "${COMMIT_WAIT:-3}"

resp=$(http_get "${PERSIST_URL}/indexes/persist-test/_search")
assert_json_gte "Persist: 3 docs after post-restart ingest" ".hits.total.value" 3 "$resp"

# Cleanup persistence resources
docker stop "${PERSIST_CONTAINER}" 2>/dev/null || true
docker rm   "${PERSIST_CONTAINER}" 2>/dev/null || true
docker volume rm "${PERSIST_VOLUME}" 2>/dev/null || true

# Completed group
return 0
