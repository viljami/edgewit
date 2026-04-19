#!/usr/bin/env bash
# =============================================================================
# E2E test helpers
# =============================================================================
# Shared helpers for the end-to-end test suite. Designed to be `source`d by
# individual test-group scripts and the main runner.
#
# - Provides HTTP wrappers that return "<body>\n<http-status>"
# - Lightweight retry logic for transient 5xx/connection failures
# - Assertion helpers that update PASS/FAIL counters
# - Diagnostic helpers (dump container logs)
#
# Usage:
#   source ./scripts/e2e/00-helpers.sh
# =============================================================================

set -euo pipefail

# Prevent double-source
if [[ "${__E2E_HELPERS_LOADED:-}" == "1" ]]; then
  return 0
fi
readonly __E2E_HELPERS_LOADED=1

# Configurable timeout (seconds) for curl
E2E_CURL_TIMEOUT="${E2E_CURL_TIMEOUT:-15}"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
BOLD='\033[1m'
NC='\033[0m'

# Counters — allow runner to override; otherwise initialize
PASS=${PASS:-0}
FAIL=${FAIL:-0}
if [[ -z "${FAILED_TESTS+x}" ]]; then
  declare -a FAILED_TESTS=()
fi

# ---- Basic output helpers ---------------------------------------------------
pass() {
  PASS=$((PASS + 1))
  echo -e "  ${GREEN}✔${NC} $1"
}

fail() {
  FAIL=$((FAIL + 1))
  FAILED_TESTS+=("$1")
  echo -e "  ${RED}✘${NC} $1"
  [[ -n "${2:-}" ]] && echo -e "    ${YELLOW}expected:${NC} $2"
  [[ -n "${3:-}" ]] && echo -e "    ${YELLOW}actual  :${NC} $3"
}

section() {
  echo ""
  echo -e "${BLUE}${BOLD}▶ $1${NC}"
}

# ---- Dependency checks -----------------------------------------------------
check_deps() {
  local missing=()
  for cmd in docker curl jq; do
    command -v "$cmd" &>/dev/null || missing+=("$cmd")
  done
  if [[ ${#missing[@]} -gt 0 ]]; then
    echo -e "${RED}Missing required tools: ${missing[*]}${NC}" >&2
    echo "Install them before running this script." >&2
    exit 1
  fi
}

# ---- Diagnostics ------------------------------------------------------------
dump_container_logs() {
  # Usage: dump_container_logs <container-name> [lines]
  local name="${1:-}" lines="${2:-200}"
  if [[ -z "$name" ]]; then
    echo "dump_container_logs: no container name provided" >&2
    return 1
  fi
  echo "----- docker logs ${name} (last ${lines} lines) -----"
  docker logs "$name" 2>&1 | tail -n "$lines" || true
  echo "----------------------------------------------------"
}

# ---- HTTP helpers ----------------------------------------------------------
# All functions return "<body>\n<http-status>".
# If curl cannot connect we return "\n000".

_curl() {
  # generic wrapper
  curl -s --max-time "${E2E_CURL_TIMEOUT}" -w "\n%{http_code}" "$@" 2>/dev/null || printf "\n000"
}

http_get() {
  _curl "$1"
}

http_get_header() {
  # $1 = url, $2 = token
  _curl -H "Authorization: Bearer $2" "$1"
}

# Lightweight retry wrapper for idempotent retriable requests.
# Accepts a command (as an array) to execute and performs simple retry on
# connection failures (status 000) or server messages that look like WAL issues.
_retry_request_inner() {
  # Accept a command string to execute (avoids `local -n` which isn't portable
  # to older Bash versions such as the macOS default).
  # Usage: _retry_request_inner "<command-string>" [tries]
  local cmd_str="$1"
  local tries=${2:-3}
  local i=1
  local resp code body
  while true; do
    # Execute the provided command string in a subshell so the caller can
    # include complex curl invocations.
    resp=$(bash -lc "$cmd_str")
    code=$(printf '%s' "$resp" | tail -n1)
    body=$(printf '%s' "$resp" | sed '$d')
    # Success (2xx) or client error (4xx) -> return to caller
    if [[ "$code" != "500" && "$code" != "000" ]]; then
      printf '%s' "$resp"
      return 0
    fi
    # If transient (connection failure) or WAL-like message, retry a few times
    if [[ "$code" == "000" ]] || printf '%s' "$body" | grep -qi "WAL channel closed"; then
      if (( i >= tries )); then
        printf '%s' "$resp"
        return 0
      fi
      sleep $((i))   # simple backoff: 1,2,... seconds
      i=$((i + 1))
      continue
    fi
    # Other 5xx -> return immediately to let tests show details
    printf '%s' "$resp"
    return 0
  done
}

# POST with retry logic for transient failures
http_post() {
  # $1 = url, $2 = Content-Type, $3 = body
  local url="$1" ct="$2" body="$3"
  # Write body to a temp file to avoid quoting issues (portable on macOS).
  local tmp
  tmp=$(mktemp /tmp/e2e-body.XXXXXX) || tmp="/tmp/e2e-body.$$"
  printf '%s' "$body" > "$tmp"
  # Build a command string that references the temp file. The retry helper will
  # eval this string in a subshell; the tmp path is embedded literally so it
  # remains valid across retries.
  local cmd_str="curl -s --max-time ${E2E_CURL_TIMEOUT} -w '\n%{http_code}' -X POST -H \"Content-Type: ${ct}\" --data-binary @\"${tmp}\" \"${url}\" 2>/dev/null || printf '\\n000'"
  _retry_request_inner "$cmd_str" 3
  rm -f "$tmp"
}

http_post_auth() {
  # $1 = url, $2 = Content-Type, $3 = body, $4 = token
  local url="$1" ct="$2" body="$3" token="$4"
  # Use a temp file for the request body to avoid complex quoting issues.
  local tmp
  tmp=$(mktemp /tmp/e2e-body.XXXXXX) || tmp="/tmp/e2e-body.$$"
  printf '%s' "$body" > "$tmp"
  local cmd_str="curl -s --max-time ${E2E_CURL_TIMEOUT} -w '\n%{http_code}' -X POST -H \"Content-Type: ${ct}\" -H \"Authorization: Bearer ${token}\" --data-binary @\"${tmp}\" \"${url}\" 2>/dev/null || printf '\\n000'"
  _retry_request_inner "$cmd_str" 3
  rm -f "$tmp"
}

http_put() {
  # $1 = url, $2 = body
  local url="$1" body="$2"
  # Write the body to a temp file to avoid embedding large or complex JSON in
  # the command string (which can cause quoting issues on some shells).
  local tmp
  tmp=$(mktemp /tmp/e2e-body.XXXXXX) || tmp="/tmp/e2e-body.$$"
  printf '%s' "$body" > "$tmp"
  local cmd_str="curl -s --max-time ${E2E_CURL_TIMEOUT} -w '\n%{http_code}' -X PUT -H \"Content-Type: application/json\" --data-binary @\"${tmp}\" \"${url}\" 2>/dev/null || printf '\\n000'"
  _retry_request_inner "$cmd_str" 3
  rm -f "$tmp"
}

http_delete() {
  _curl -X DELETE "$1"
}

# Helpers to split curl response into body and status
body_of()   { printf '%s' "$1" | sed '$d'; }
status_of() { printf '%s' "$1" | tail -n 1; }

# ---- Assertion helpers -----------------------------------------------------
# These update PASS/FAIL counters and print helpful diagnostics where useful.

assert_status() {
  # $1=name  $2=expected_code  $3=curl_response
  local actual; actual=$(status_of "$3")
  if [[ "$actual" == "$2" ]]; then
    pass "$1 (HTTP $2)"
    return 0
  fi

  local body_preview; body_preview=$(body_of "$3" | head -c 200)
  fail "$1" "HTTP $2" "HTTP $actual  ← ${body_preview}"

  # Dump container logs automatically for connection/5xx failures to assist debugging.
  if [[ "$actual" == "000" || "$actual" =~ ^5 ]]; then
    echo ""
    echo "===== Diagnostics: container logs (last 200 lines) ====="
    # Caller/runner should set MAIN_CONTAINER/AUTH_CONTAINER/PERSIST_CONTAINER variables.
    if [[ -n "${MAIN_CONTAINER:-}" ]]; then
      docker logs "${MAIN_CONTAINER}" 2>&1 | tail -n 200 || true
    fi
    if [[ -n "${AUTH_CONTAINER:-}" ]]; then
      docker logs "${AUTH_CONTAINER}" 2>&1 | tail -n 200 || true
    fi
    if [[ -n "${PERSIST_CONTAINER:-}" ]]; then
      docker logs "${PERSIST_CONTAINER}" 2>&1 | tail -n 200 || true
    fi
    echo "======================================================="
  fi
}

assert_json_eq() {
  # $1=name  $2=jq_path  $3=expected_value  $4=curl_response
  local actual; actual=$(body_of "$4" | jq -r "$2" 2>/dev/null || echo "__jq_error__")
  if [[ "$actual" == "$3" ]]; then
    pass "$1"
  else
    local body_preview; body_preview=$(body_of "$4" | head -c 200)
    fail "$1" "$2 == $3" "$2 == $actual (body: ${body_preview})"
  fi
}

assert_json_gte() {
  # $1=name  $2=jq_path  $3=minimum_int  $4=curl_response
  local actual; actual=$(body_of "$4" | jq -r "$2" 2>/dev/null || echo "0")
  # Normalize empty/null to 0 and perform a numeric comparison robustly
  if awk -v a="$actual" -v b="$3" 'BEGIN{ if (a=="" || a=="null") a=0; exit !((a+0) >= (b+0)) }'; then
    pass "$1 ($2 = $actual ≥ $3)"
  else
    fail "$1" "$2 ≥ $3" "$2 = $actual"
  fi
}

assert_json_array_len() {
  # $1=name  $2=jq_path  $3=expected_len  $4=curl_response
  local actual; actual=$(body_of "$4" | jq -r "($2) | length" 2>/dev/null || echo "-1")
  if [[ "$actual" == "$3" ]]; then
    pass "$1 (length = $3)"
  else
    fail "$1" "$2 | length == $3" "length == $actual"
  fi
}

assert_body_contains() {
  # $1=name  $2=needle  $3=curl_response  (plain-text body)
  local body; body=$(body_of "$3")
  if echo "$body" | grep -qF "$2"; then
    pass "$1"
  else
    fail "$1" "body contains '$2'" "$(echo "$body" | head -c 200)…"
  fi
}

assert_json_array_contains_value() {
  # $1=name  $2=jq_path_to_array  $3=value  $4=curl_response
  local result; result=$(body_of "$4" | jq -r "$2" 2>/dev/null || echo "")
  if echo "$result" | grep -qF "$3"; then
    pass "$1"
  else
    fail "$1" "$2 contains '$3'" "got: $(echo "$result" | head -c 200)"
  fi
}

assert_json_array_not_contains_value() {
  # $1=name  $2=jq_path_to_array  $3=value  $4=curl_response
  local result; result=$(body_of "$4" | jq -r "$2" 2>/dev/null || echo "")
  if ! echo "$result" | grep -qF "$3"; then
    pass "$1"
  else
    fail "$1" "$2 does NOT contain '$3'" "found it: $(echo "$result" | head -c 200)"
  fi
}

# ---- Utilities --------------------------------------------------------------
# print summary; runner may choose to call its own summary instead.
e2e_print_summary() {
  echo ""
  echo -e "${BOLD}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
  echo -e "${BOLD} Test Summary${NC}"
  echo -e "${BOLD}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
  echo -e "  ${GREEN}Passed : ${PASS}${NC}"
  if [[ $FAIL -gt 0 ]]; then
    echo -e "  ${RED}Failed : ${FAIL}${NC}"
    echo ""
    echo -e "${RED}${BOLD}Failed tests:${NC}"
    for t in "${FAILED_TESTS[@]}"; do
      echo -e "  ${RED}•${NC} $t"
    done
    echo ""
    return 1
  else
    echo -e "  ${GREEN}Failed : 0${NC}"
    echo ""
    echo -e "${GREEN}${BOLD}All ${PASS} tests passed ✔${NC}"
    return 0
  fi
}

# End of helpers
