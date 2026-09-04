#!/usr/bin/env bash
# **The workspace compile phase: concurrent fmt check, clippy, doc check, and test compilation.**
#
# `cargo clippy --all-targets`, `cargo doc` and `cargo test --no-run` each
# compile the workspace concurrently into their own target directories to avoid
# cargo build lock serialization.
#
# In addition to the test binaries, `target/debug/examples/mvn` is compiled so the
# proof cache wrapper (`tests/support/mvn.rs`) is always available to memoise
# real-toolchain Maven executions.
set -uo pipefail

cargo fmt --all --check || exit 1

logs=$(mktemp -d "${TMPDIR:-/tmp}/jails-build.XXXXXX")
trap 'rm -rf "$logs"' EXIT

cargo clippy --workspace --all-targets --target-dir target/lint -- -D warnings \
    > "$logs/clippy" 2>&1 &
clippy=$!
RUSTDOCFLAGS='-D warnings' cargo doc --workspace --no-deps --target-dir target/doc-check \
    > "$logs/doc" 2>&1 &
doc=$!
(cargo test --workspace --no-run && cargo build --example mvn) > "$logs/test" 2>&1 &
tests=$!

status=0
for job in "clippy:$clippy" "doc:$doc" "test:$tests"; do
    name=${job%%:*}
    pid=${job##*:}
    if wait "$pid"; then
        echo "build-workspace: $name ok"
    else
        echo "build-workspace: $name FAILED"
        cat "$logs/$name"
        status=1
    fi
done
exit $status
