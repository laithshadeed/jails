#!/usr/bin/env bash
# **Run every test executable in the workspace concurrently, plus doctests.**
set -u
cd "$(dirname "$0")/.."
logs=target/jails-test-logs/gate
rm -rf "$logs"
mkdir -p "$logs"

mapfile -t executables < <(
  cargo test --workspace --no-run --message-format=json 2>/dev/null | python3 -c '
import json, sys
for line in sys.stdin:
    try:
        message = json.loads(line)
    except ValueError:
        continue
    if message.get("reason") == "compiler-artifact" and message.get("executable") \
            and message.get("profile", {}).get("test"):
        print(message["executable"])
'
)
if [ "${#executables[@]}" -lt 10 ]; then
  echo "test-workspace: only ${#executables[@]} test executables found; the build is not there to run" >&2
  exit 2
fi

names=()
pids=()
for exe in "${executables[@]}"; do
  name=$(basename "$exe" | sed 's/-[0-9a-f]*$//')
  names+=("$name")
  "$exe" "$@" > "$logs/$name.log" 2>&1 &
  pids+=($!)
done
names+=(doctests)
cargo test --workspace --doc -- "$@" > "$logs/doctests.log" 2>&1 &
pids+=($!)

status=0
for i in "${!pids[@]}"; do
  wait "${pids[$i]}"
  code=$?
  name=${names[$i]}
  summary=$(grep -E '^test result' "$logs/$name.log" | tail -1)
  if [ "$code" -eq 0 ]; then
    printf 'test-workspace: %-28s ok   %s\n' "$name" "${summary#test result: }"
  else
    status=1
    printf 'test-workspace: %-28s FAILED (exit %s)\n' "$name" "$code"
  fi
done
if [ "$status" -ne 0 ]; then
  for i in "${!pids[@]}"; do
    name=${names[$i]}
    if ! grep -qE '^test result: ok' "$logs/$name.log" || grep -qE '^test result: FAILED' "$logs/$name.log"; then
      echo
      echo "==== $name ($logs/$name.log)"
      if grep -q '^failures:' "$logs/$name.log"; then
        sed -n '/^failures:/,$p' "$logs/$name.log" | head -300
      else
        tail -60 "$logs/$name.log"
      fi
    fi
  done
fi
exit $status
