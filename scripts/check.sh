#!/usr/bin/env bash
#
# Unified mechanical quality gate for jails:
#   1. format check (cargo fmt --check)
#   2. clippy (cargo clippy --workspace --all-targets -- -D warnings)
#   3. documentation check (RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps)
#   4. workspace test suite (concurrent executable runner + doctests)
#
# Hardware empathy: executes bounded under scripts/run-bounded.sh
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"
cd "${ROOT_DIR}"

if [[ -z "${BOUNDED_SCOPE:-}" ]]; then
  exec "${SCRIPT_DIR}/run-bounded.sh" "$0" "$@"
fi

if [[ -t 1 ]]; then
  GREEN='\033[0;32m'
  RED='\033[0;31m'
  YELLOW='\033[0;33m'
  CYAN='\033[0;36m'
  BOLD='\033[1m'
  NC='\033[0m'
else
  GREEN=''
  RED=''
  YELLOW=''
  CYAN=''
  BOLD=''
  NC=''
fi

LOG_DIR="${ROOT_DIR}/target/check-logs"
rm -rf "${LOG_DIR}"
mkdir -p "${LOG_DIR}"

TOTAL_START=${SECONDS}
FAILED_STAGES=()

run_stage() {
  local name="$1"
  local desc="$2"
  shift 2

  printf "${CYAN}==>${NC} ${BOLD}%s${NC} (%s)...\n" "${name}" "${desc}"
  local start=${SECONDS}
  local log_file="${LOG_DIR}/${name}.log"

  if "$@" > "${log_file}" 2>&1; then
    local elapsed=$((SECONDS - start))
    printf "  ${GREEN}✓${NC} %s passed in %ss\n" "${name}" "${elapsed}"
  else
    local elapsed=$((SECONDS - start))
    printf "  ${RED}✗${NC} %s FAILED in %ss\n" "${name}" "${elapsed}"
    FAILED_STAGES+=("${name}")
    echo "--- ${name} error log (tail 40 lines) ---" >&2
    tail -n 40 "${log_file}" >&2
    echo "-----------------------------------------" >&2
  fi
}

echo "Starting jails quality gate..."

# Phase 1: Lint & Format (fmt runs concurrently with clippy)
echo "==> fmt & clippy..."
cargo fmt --check > "${LOG_DIR}/fmt.log" 2>&1 & pid_fmt=$!
cargo clippy --workspace --all-targets -- -D warnings > "${LOG_DIR}/clippy.log" 2>&1 & pid_clippy=$!

wait $pid_fmt && fmt_ok=1 || fmt_ok=0
wait $pid_clippy && clippy_ok=1 || clippy_ok=0

if [ "$fmt_ok" -eq 1 ]; then
  printf "  ${GREEN}✓${NC} fmt passed\n"
else
  printf "  ${RED}✗${NC} fmt FAILED\n"
  FAILED_STAGES+=("fmt")
  echo "--- fmt error log (tail 40 lines) ---" >&2
  tail -n 40 "${LOG_DIR}/fmt.log" >&2
  echo "-----------------------------------------" >&2
fi

if [ "$clippy_ok" -eq 1 ]; then
  printf "  ${GREEN}✓${NC} clippy passed\n"
else
  printf "  ${RED}✗${NC} clippy FAILED\n"
  FAILED_STAGES+=("clippy")
  echo "--- clippy error log (tail 40 lines) ---" >&2
  tail -n 40 "${LOG_DIR}/clippy.log" >&2
  echo "-----------------------------------------" >&2
fi

if [[ ${#FAILED_STAGES[@]} -gt 0 ]]; then
  TOTAL_ELAPSED=$((SECONDS - TOTAL_START))
  echo "=========================================="
  printf "${RED}${BOLD}FAILED STAGES:${NC} %s in %ss\n" "${FAILED_STAGES[*]}" "${TOTAL_ELAPSED}"
  exit 1
fi

# Phase 2: Test & Doc (cargo doc runs concurrently with test suite)
echo "==> test & doc..."
(env RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps > "${LOG_DIR}/doc.log" 2>&1) & pid_doc=$!
("${SCRIPT_DIR}/test.sh" > "${LOG_DIR}/test.log" 2>&1) & pid_test=$!

wait $pid_doc && doc_ok=1 || doc_ok=0
wait $pid_test && test_ok=1 || test_ok=0

if [ "$doc_ok" -eq 1 ]; then
  printf "  ${GREEN}✓${NC} doc passed\n"
else
  printf "  ${RED}✗${NC} doc FAILED\n"
  FAILED_STAGES+=("doc")
  echo "--- doc error log (tail 40 lines) ---" >&2
  tail -n 40 "${LOG_DIR}/doc.log" >&2
  echo "-----------------------------------------" >&2
fi

if [ "$test_ok" -eq 1 ]; then
  printf "  ${GREEN}✓${NC} test passed\n"
else
  printf "  ${RED}✗${NC} test FAILED\n"
  FAILED_STAGES+=("test")
  echo "--- test error log (tail 40 lines) ---" >&2
  tail -n 40 "${LOG_DIR}/test.log" >&2
  echo "-----------------------------------------" >&2
fi

TOTAL_ELAPSED=$((SECONDS - TOTAL_START))

echo "=========================================="
if [[ ${#FAILED_STAGES[@]} -eq 0 ]]; then
  printf "${GREEN}${BOLD}ALL STAGES PASSED${NC} in %ss!\n" "${TOTAL_ELAPSED}"
  exit 0
else
  printf "${RED}${BOLD}FAILED STAGES:${NC} %s in %ss\n" "${FAILED_STAGES[*]}" "${TOTAL_ELAPSED}"
  exit 1
fi
