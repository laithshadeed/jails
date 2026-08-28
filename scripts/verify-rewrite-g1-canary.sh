#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
legacy_revision=${JAILS_LEGACY_REVISION:-$(git -C "$repo_root" rev-parse HEAD)}
scratch=$(mktemp -d "${TMPDIR:-/tmp}/jails-g1-canary.XXXXXX")

cleanup() {
  rm -rf -- "$scratch"
}
trap cleanup EXIT INT TERM

legacy_source="$scratch/source"
legacy_target="$scratch/target"
mkdir -p "$legacy_source"
git -C "$repo_root" archive --format=tar --output="$scratch/legacy.tar" "$legacy_revision"
tar -xf "$scratch/legacy.tar" -C "$legacy_source"

echo "[verify-rewrite-g1-canary] building legacy revision $legacy_revision"
CARGO_TARGET_DIR="$legacy_target" cargo build \
  --locked \
  --manifest-path "$legacy_source/Cargo.toml" \
  --bin jails

echo "[verify-rewrite-g1-canary] comparing legacy and canonical product loops"
JAILS_LEGACY_BIN="$legacy_target/debug/jails" \
  cargo test --test differential -- --nocapture
