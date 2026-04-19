#!/usr/bin/env bash
# =============================================================================
# E2E Group 07 — Index deletion
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
if ! declare -f http_delete >/dev/null 2>&1 || ! declare -f assert_status >/dev/null 2>&1; then
  echo "helpers not loaded: skipping 07-delete.sh" >&2
  return 0
fi

section "Index deletion"

# Delete the e2e-logs index
resp=$(http_delete "${MAIN_URL}/indexes/e2e-logs")
assert_status "DELETE /indexes/e2e-logs  →  200" "200" "$resp"

# Confirm index is gone
resp=$(http_get "${MAIN_URL}/indexes/e2e-logs")
assert_status "GET /indexes/e2e-logs (after delete)  →  404" "404" "$resp"

# Ensure the indexes listing no longer contains the deleted index
resp=$(http_get "${MAIN_URL}/indexes")
assert_json_array_not_contains_value "GET /indexes: e2e-logs no longer listed" ".[].name" "e2e-logs" "$resp"

# Deleting a non-existent index should return 404
resp=$(http_delete "${MAIN_URL}/indexes/e2e-logs")
assert_status "DELETE /indexes/e2e-logs (again)  →  404" "404" "$resp"

# Completed group
return 0
