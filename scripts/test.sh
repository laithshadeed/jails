#!/usr/bin/env bash
# **Run every test executable in the workspace concurrently, plus doctests.**
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"

cd "${ROOT_DIR}"

logs="${ROOT_DIR}/target/jails-test-logs/gate"
rm -rf "$logs"
mkdir -p "$logs"

# Number of concurrent test binaries to execute
cores=$(nproc 2>/dev/null || echo 4)
concurrency=${TEST_CONCURRENCY:-$(( cores >= 2 ? cores * 3 : 2 ))}
[ "$concurrency" -ge 1 ] || concurrency=1

# Cap individual binary internal thread pool so child processes don't oversubscribe
export RUST_TEST_THREADS="${RUST_TEST_THREADS:-2}"

# Extract executables and their crate manifest directories
mapfile -t artifacts < <(
  cargo test --workspace --bins --tests --no-run --message-format=json 2>/dev/null | python3 -c '
import json, sys, os

PRIORITIES = {
    "cli": 100,
    "crash": 90,
    "jails_support": 85,
    "product_loop": 80,
    "agreement": 75,
    "golden": 70,
    "architecture": 60,
    "jails_drive": 50,
    "jails_model": 40,
    "jails_project": 30,
    "jails_compiler": 20,
}

items = []
for line in sys.stdin:
    try:
        msg = json.loads(line)
    except ValueError:
        continue
    if msg.get("reason") == "compiler-artifact" and msg.get("executable") and msg.get("profile", {}).get("test"):
        exe = msg["executable"]
        manifest = msg.get("manifest_path", "")
        manifest_dir = os.path.dirname(manifest) if manifest else ""
        items.append((exe, manifest_dir))

def sort_key(item):
    base = os.path.basename(item[0]).split("-")[0]
    return -PRIORITIES.get(base, 0)

items.sort(key=sort_key)
for exe, manifest_dir in items:
    print(f"{exe}\t{manifest_dir}")
'
)

if [ "${#artifacts[@]}" -lt 10 ]; then
  echo "test: only ${#artifacts[@]} test executables found; compilation may have failed" >&2
  exit 2
fi

names=()
log_files=()
exit_files=()

# Run test binaries concurrently with bounded concurrency
for item in "${artifacts[@]}"; do
  exe="${item%%	*}"
  manifest_dir="${item##*	}"
  base_name=$(basename "$exe")
  display_name=$(echo "$base_name" | sed 's/-[0-9a-f]*$//')
  names+=("$display_name")
  log_file="$logs/$display_name.log"
  exit_file="$logs/$display_name.exit"
  log_files+=("$log_file")
  exit_files+=("$exit_file")

  while [ "$(jobs -rp | wc -l)" -ge "$concurrency" ]; do
    wait -n 2>/dev/null || sleep 0.02
  done

  (
    export CARGO_MANIFEST_DIR="${manifest_dir:-$ROOT_DIR}"
    code=0
    "$exe" "$@" > "$log_file" 2>&1 || code=$?
    echo "$code" > "$exit_file"
  ) &
done

# Run doctests boundedly
names+=(doctests)
doc_log="$logs/doctests.log"
doc_exit="$logs/doctests.exit"
log_files+=("$doc_log")
exit_files+=("$doc_exit")
while [ "$(jobs -rp | wc -l)" -ge "$concurrency" ]; do
  wait -n 2>/dev/null || sleep 0.02
done
(
  code=0
  cargo test --workspace --doc -- "$@" > "$doc_log" 2>&1 || code=$?
  echo "$code" > "$doc_exit"
) &

# Wait for all workers to finish
wait

status=0
for i in "${!names[@]}"; do
  name=${names[$i]}
  log_file=${log_files[$i]}
  exit_file=${exit_files[$i]}
  code=$(cat "$exit_file" 2>/dev/null || echo 1)
  summary=$(grep -E '^test result' "$log_file" 2>/dev/null | tail -1 || true)
  if [ "$code" -eq 0 ] && [ -n "$summary" ] && grep -qE '^test result: ok' "$log_file"; then
    printf 'test: %-32s ok   %s\n' "$name" "${summary#test result: }"
  else
    status=1
    printf 'test: %-32s FAILED (exit %s)\n' "$name" "$code"
  fi
done

if [ "$status" -ne 0 ]; then
  echo
  echo "=========================================="
  echo "TEST FAILURES:"
  echo "=========================================="
  for i in "${!names[@]}"; do
    name=${names[$i]}
    log_file=${log_files[$i]}
    exit_file=${exit_files[$i]}
    code=$(cat "$exit_file" 2>/dev/null || echo 1)
    if [ "$code" -ne 0 ] || ! grep -qE '^test result: ok' "$log_file" 2>/dev/null; then
      echo "---- $name ($log_file) ----"
      if grep -q '^failures:' "$log_file"; then
        sed -n '/^failures:/,$p' "$log_file" | head -100
      else
        tail -40 "$log_file"
      fi
      echo "-----------------------------------------"
    fi
  done
fi

exit $status
