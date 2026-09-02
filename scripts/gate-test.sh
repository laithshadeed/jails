#!/usr/bin/env bash
# Every test executable in the workspace at once, plus the doctests.
#
# `cargo test --workspace` runs the twenty-odd test binaries one after another,
# and the real-toolchain binary alone spans forty seconds while the others add
# thirty more in a queue. Nothing in them needs the queue: the budgets a
# concurrent run has to respect -- the JVM permits, the toolbox fixture claims
# -- are `flock`s under `target/`, shared however the suite is launched, so
# the binaries are started together and each writes its own log. Arguments
# are handed to every binary, so a filter works as it does under cargo.
#
# Run it through `scripts/bounded.sh`, like everything as heavy.
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
  echo "gate-test: only ${#executables[@]} test executables found; the build is not there to run" >&2
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
    printf 'gate-test: %-28s ok   %s\n' "$name" "${summary#test result: }"
  else
    status=1
    printf 'gate-test: %-28s FAILED (exit %s)\n' "$name" "$code"
  fi
done
if [ "$status" -ne 0 ]; then
  for i in "${!pids[@]}"; do
    name=${names[$i]}
    if ! grep -qE '^test result: ok' "$logs/$name.log" || grep -qE '^test result: FAILED' "$logs/$name.log"; then
      echo
      echo "==== $name ($logs/$name.log)"
      # The failures section, or the tail when the binary died before one.
      if grep -q '^failures:' "$logs/$name.log"; then
        sed -n '/^failures:/,$p' "$logs/$name.log" | head -300
      else
        tail -60 "$logs/$name.log"
      fi
    fi
  done
fi
exit $status
