#!/usr/bin/env bash
# =============================================================================
# Refactored Edgewit Container End-to-End Test Runner
# =============================================================================
# This runner orchestrates per-group e2e scripts under `scripts/e2e/`.
# It sources shared helpers (`00-helpers.sh`) and then sequentially runs
# per-group scripts (sourced into this process so PASS/FAIL counters persist).
#
# Expectations for per-group scripts:
# - They should assume helpers are already sourced.
# - They must use `pass` / `fail` helpers to report results (do not call exit).
# - Keep them idempotent and avoid `set -e` that would terminate the runner.
#
# Usage:
#   ./scripts/test-container.sh [--skip-build] [--keep-container] [--image NAME]
# =============================================================================

set -euo pipefail

# ---------------------------------------------------------------------------
# Argument parsing
# ---------------------------------------------------------------------------
SKIP_BUILD=false
KEEP_CONTAINER=false
IMAGE="edgewit:e2e-test"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --skip-build)      SKIP_BUILD=true ;;
    --keep-container)  KEEP_CONTAINER=true ;;
    --image)           shift; IMAGE="$1" ;;
    *) echo "Unknown option: $1" >&2; exit 1 ;;
  esac
  shift
done

# ---------------------------------------------------------------------------
# Environment / Defaults
# ---------------------------------------------------------------------------
REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
E2E_DIR="$(cd "$(dirname "$0")/e2e" && pwd)"

MAIN_CONTAINER="edgewit-e2e-main"
AUTH_CONTAINER="edgewit-e2e-auth"
PERSIST_CONTAINER="edgewit-e2e-persist"
PERSIST_VOLUME="edgewit-e2e-persist-vol"

MAIN_PORT=19200
AUTH_PORT=19201
PERSIST_PORT=19202

export MAIN_URL="http://localhost:${MAIN_PORT}"
export AUTH_URL="http://localhost:${AUTH_PORT}"
export PERSIST_URL="http://localhost:${PERSIST_PORT}"

READY_TIMEOUT=60   # seconds to wait for a container to become reachable
COMMIT_WAIT=3      # seconds to wait after ingest for the indexer to commit

# ---------------------------------------------------------------------------
# Source helpers
# ---------------------------------------------------------------------------
HELPERS="${E2E_DIR}/00-helpers.sh"
if [[ ! -f "${HELPERS}" ]]; then
  echo "Missing helpers file: ${HELPERS}" >&2
  echo "Please ensure scripts/e2e/00-helpers.sh exists." >&2
  exit 1
fi
# shellcheck source=/dev/null
source "${HELPERS}"

# Ensure helpers didn't change our strictness
set -euo pipefail

# ---------------------------------------------------------------------------
# Utility functions
# ---------------------------------------------------------------------------
_wait_for_ready() {
  # Wait until the container's HTTP server is reachable (any HTTP response counts).
  # $1 = base URL, $2 = container name (for logs)
  local url="$1" label="$2"
  printf "  Waiting for %-30s..." "$label"
  local i=0
  while [[ $i -lt $READY_TIMEOUT ]]; do
    # use curl with short timeout to test readiness
    local status
    status=$(curl -s -o /dev/null -w "%{http_code}" --max-time 2 "${url}/_health" 2>/dev/null || echo "000")
    if [[ "$status" != "000" ]]; then
      echo -e " ${GREEN}ready${NC} (${i}s, HTTP ${status})"
      return 0
    fi
    printf "."
    sleep 1
    i=$((i + 1))
  done
  echo -e " ${RED}timed out after ${READY_TIMEOUT}s!${NC}"
  echo "Last container logs (${label}):"
  docker logs "$label" 2>&1 | tail -n 200 || true
  return 1
}

_start_main_container() {
  _stop_rm() {
    docker stop "$1" 2>/dev/null || true
    docker rm   "$1" 2>/dev/null || true
  }

  _stop_rm "${MAIN_CONTAINER}"
  echo "Starting main container (${MAIN_CONTAINER}) from image ${IMAGE}..."
  docker run -d \
    --name "${MAIN_CONTAINER}" \
    -p "${MAIN_PORT}:9200" \
    -e RUST_LOG=info \
    -e EDGEWIT_COMMIT_INTERVAL_SECS=1 \
    "${IMAGE}" >/dev/null
}

_stop_all_and_cleanup() {
  echo ""
  if [[ "${KEEP_CONTAINER}" == true ]]; then
    echo -e "${YELLOW}⚠  --keep-container: leaving containers in place for inspection.${NC}"
    echo -e "   Main     : docker logs ${MAIN_CONTAINER}"
    echo -e "   Auth     : docker logs ${AUTH_CONTAINER}"
    echo -e "   Persist  : docker logs ${PERSIST_CONTAINER}"
    return
  fi

  echo "Cleaning up containers and volumes…"
  docker stop "${MAIN_CONTAINER}" 2>/dev/null || true
  docker rm   "${MAIN_CONTAINER}" 2>/dev/null || true

  docker stop "${AUTH_CONTAINER}" 2>/dev/null || true
  docker rm   "${AUTH_CONTAINER}" 2>/dev/null || true

  docker stop "${PERSIST_CONTAINER}" 2>/dev/null || true
  docker rm   "${PERSIST_CONTAINER}" 2>/dev/null || true

  docker volume rm "${PERSIST_VOLUME}" 2>/dev/null || true
}

# Ensure cleanup runs on exit unless we are keeping containers
trap _stop_all_and_cleanup EXIT

# ---------------------------------------------------------------------------
# Build image (unless skipped)
# ---------------------------------------------------------------------------
section "Build"

if [[ "${SKIP_BUILD}" == true ]]; then
  echo "  Skipping docker build (--skip-build)"
  if ! docker image inspect "${IMAGE}" &>/dev/null; then
    echo -e "  ${RED}Image '${IMAGE}' not found locally. Remove --skip-build or pull the image first.${NC}" >&2
    exit 1
  fi
  pass "Image '${IMAGE}' available"
else
  echo "  Building ${IMAGE} from ${REPO_ROOT}…"
  if docker build -t "${IMAGE}" "${REPO_ROOT}" --quiet; then
    pass "docker build → ${IMAGE}"
  else
    fail "docker build → ${IMAGE}"
    exit 1
  fi
fi

# ---------------------------------------------------------------------------
# Start main container (shared by many groups)
# ---------------------------------------------------------------------------
section "Container startup"

_start_main_container

if ! _wait_for_ready "${MAIN_URL}" "${MAIN_CONTAINER}"; then
  echo "Main container did not become ready. Aborting run."
  exit 1
fi

# ---------------------------------------------------------------------------
# Ordered per-group scripts to run (sourced so PASS/FAIL counters persist)
# ---------------------------------------------------------------------------
GROUP_SCRIPTS=(
  "${E2E_DIR}/01-cluster.sh"
  "${E2E_DIR}/02-indexes.sh"
  "${E2E_DIR}/03-ingest.sh"
  "${E2E_DIR}/04-search.sh"
  "${E2E_DIR}/05-aggs.sh"
  "${E2E_DIR}/06-stats.sh"
  "${E2E_DIR}/07-delete.sh"
  "${E2E_DIR}/08-auth.sh"
  "${E2E_DIR}/09-persist.sh"
)

section "Running test groups"

for grp in "${GROUP_SCRIPTS[@]}"; do
  if [[ ! -f "${grp}" ]]; then
    echo ""
    echo -e "${YELLOW}Notice:${NC} Group script not found: ${grp}"
    echo -e "  Skipping. Create the file to run this group, or run the monolithic tests."
    # continue allowing other groups to run if present
    continue
  fi

  echo ""
  echo "------------------------------------------------------------"
  echo " Sourcing test group: ${grp}"
  echo "------------------------------------------------------------"
  # shellcheck source=/dev/null
  source "${grp}"
done

# ---------------------------------------------------------------------------
# Final summary (helpers provide e2e_print_summary)
# ---------------------------------------------------------------------------
echo ""
if command -v e2e_print_summary >/dev/null 2>&1; then
  if ! e2e_print_summary; then
    exit 1
  fi
else
  # Fallback summary if helper not present for some reason
  echo -e "${BOLD}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
  echo -e "${BOLD} Test Summary${NC}"
  echo -e "${BOLD}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
  echo -e "  ${GREEN}Passed : ${PASS:-0}${NC}"
  if [[ ${FAIL:-0} -gt 0 ]]; then
    echo -e "  ${RED}Failed : ${FAIL}${NC}"
    echo ""
    echo -e "${RED}${BOLD}Failed tests:${NC}"
    for t in "${FAILED_TESTS[@]:-}"; do
      echo -e "  ${RED}•${NC} $t"
    done
    echo ""
    exit 1
  else
    echo -e "  ${GREEN}Failed : 0${NC}"
    echo ""
    echo -e "${GREEN}${BOLD}All ${PASS:-0} tests passed ✔${NC}"
  fi
fi

# Runner exits normally (trap will perform cleanup)
exit 0
