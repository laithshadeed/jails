#!/bin/bash
# Run a tests/cli filter and print only compact failure reasons.
cd /home/laith/code/jails-merge
OUT=/tmp/claude-1000/-home-laith-code-jails-merge/bc0996dd-b789-4cb6-9d61-f1a2c112e549/scratchpad/run.txt
cargo test --test cli "$1" > "$OUT" 2>&1
grep -E '^test result' "$OUT"
python3 /tmp/claude-1000/-home-laith-code-jails-merge/bc0996dd-b789-4cb6-9d61-f1a2c112e549/scratchpad/why.py "$OUT" "" "${2:-3}"
