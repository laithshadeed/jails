#!/usr/bin/env bash
#
# Does this push need the code checks? Evaluated for the pushed range.
#
#   EVENT_NAME=push BEFORE_SHA=<a> AFTER_SHA=<b> ./scripts/ci-scope.sh
#   ./scripts/ci-scope.sh --explain
#
# Outputs:
#   code=true|false
#   reason=<one line>

set -uo pipefail

ZERO_SHA='0000000000000000000000000000000000000000'
EVENT_NAME="${EVENT_NAME:-push}"
BEFORE_SHA="${BEFORE_SHA:-}"
AFTER_SHA="${AFTER_SHA:-HEAD}"

EXPLAIN=0
[[ "${1:-}" == "--explain" ]] && EXPLAIN=1

# Non-code paths that do not affect code compilation or verification
is_doc_path() {
  local p="$1"
  if [[ "$p" =~ ^[^/]+\.md$ ]] || \
     [[ "$p" == docs/* ]] || \
     [[ "$p" == notes/* ]] || \
     [[ "$p" == ideas/* ]]; then
    return 0
  fi
  return 1
}

CODE=true
REASON=""

emit() {
  printf 'code=%s\n' "$CODE"
  printf 'reason=%s\n' "${REASON//$'\n'/ }"
  (( EXPLAIN )) && printf 'ci-scope: code=%s — %s\n' "$CODE" "$REASON" >&2
  exit 0
}

if [[ "$EVENT_NAME" != "push" ]]; then
  REASON="$EVENT_NAME is not a push — code checks run on pull requests"
  emit
fi

if [[ -z "$BEFORE_SHA" || "$BEFORE_SHA" == "$ZERO_SHA" ]]; then
  REASON="no before-sha (new branch, or first push) — running all checks"
  emit
fi

if ! git cat-file -e "${BEFORE_SHA}^{commit}" 2>/dev/null; then
  REASON="before-sha ${BEFORE_SHA:0:7} is not in checkout (force push or shallow clone) — running all checks"
  emit
fi

if ! git cat-file -e "${AFTER_SHA}^{commit}" 2>/dev/null; then
  REASON="after-sha ${AFTER_SHA:0:7} is not in checkout — running all checks"
  emit
fi

TMP="$(mktemp -d)"
cleanup() { rm -rf "$TMP"; }
trap cleanup EXIT

PATHS_FILE="$TMP/paths"
git diff --name-only "$BEFORE_SHA" "$AFTER_SHA" >"$PATHS_FILE" 2>/dev/null || : >"$PATHS_FILE"

mapfile -t PATHS <"$PATHS_FILE"
FILTERED=()
for p in "${PATHS[@]}"; do [[ -n "$p" ]] && FILTERED+=("$p"); done
PATHS=("${FILTERED[@]}")

if (( ${#PATHS[@]} == 0 )); then
  REASON="no changed paths in pushed range — running all checks"
  emit
fi

code_paths=()
for p in "${PATHS[@]}"; do
  if ! is_doc_path "$p"; then
    code_paths+=("$p")
  fi
done

if (( ${#code_paths[@]} )); then
  REASON="${#code_paths[@]} of ${#PATHS[@]} changed path(s) affect code/build, e.g. ${code_paths[0]}"
  emit
fi

CODE=false
REASON="all ${#PATHS[@]} changed path(s) are non-code documentation/notes"
emit
