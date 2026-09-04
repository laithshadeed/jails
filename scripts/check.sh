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

run_stage "fmt" "cargo fmt --check" cargo fmt --check
run_stage "clippy" "cargo clippy with -D warnings" cargo clippy --workspace --all-targets -- -D warnings
run_stage "doc" "cargo doc with -D warnings" env RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps
run_stage "test" "run all workspace test suites" "${SCRIPT_DIR}/test.sh"

TOTAL_ELAPSED=$((SECONDS - TOTAL_START))

echo "=========================================="
if [[ ${#FAILED_STAGES[@]} -eq 0 ]]; then
  printf "${GREEN}${BOLD}ALL STAGES PASSED${NC} in %ss!\n" "${TOTAL_ELAPSED}"
  exit 0
else
  printf "${RED}${BOLD}FAILED STAGES:${NC} %s in %ss\n" "${FAILED_STAGES[*]}" "${TOTAL_ELAPSED}"
  exit 1
fi
