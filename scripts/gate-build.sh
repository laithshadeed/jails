#!/usr/bin/env bash
# **The gate's compile phase: three compilations that share nothing, at once.**
#
# `cargo clippy --all-targets`, `cargo doc` and `cargo test --no-run` each
# compile the workspace, and each keeps its own artifacts: `check` metadata
# for clippy, rustdoc's for doc, code for the tests. In one `target/` they
# cannot overlap -- cargo holds one lock on the build directory -- so a change
# to a bottom crate paid three full compilations in a row, ~85 s of a 185 s
# gate. In three target directories they run concurrently and the phase costs
# the longest of them, the test build.
#
# `cargo fmt --check` runs first and alone: it is a second, and its failure
# should be the first line, not one of three interleaved.
#
# The `lint` task in `mise.toml` uses the same `target/lint` for clippy, so the
# pre-commit hook and the gate share one clippy cache and never invalidate the
# test build's.
set -uo pipefail

cargo fmt --all --check || exit 1

logs=$(mktemp -d "${TMPDIR:-/tmp}/jails-gate-build.XXXXXX")
trap 'rm -rf "$logs"' EXIT

cargo clippy --workspace --all-targets --target-dir target/lint -- -D warnings \
    > "$logs/clippy" 2>&1 &
clippy=$!
RUSTDOCFLAGS='-D warnings' cargo doc --workspace --no-deps --target-dir target/doc-check \
    > "$logs/doc" 2>&1 &
doc=$!
cargo test --workspace --no-run > "$logs/test" 2>&1 &
tests=$!

status=0
for job in "clippy:$clippy" "doc:$doc" "test:$tests"; do
    name=${job%%:*}
    pid=${job##*:}
    if wait "$pid"; then
        echo "gate-build: $name ok"
    else
        echo "gate-build: $name FAILED"
        cat "$logs/$name"
        status=1
    fi
done
exit $status
