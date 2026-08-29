#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)

# The default used to be HEAD, which built the binary under test a second time
# and compared it with itself. Every assertion passed and none of them meant
# anything -- the exact shape of a check that has silently stopped checking.
# The branch point is the nearest revision that is genuinely a different
# implementation, so it is what "no revision given" now means.
default_revision=$(git -C "$repo_root" merge-base HEAD main 2>/dev/null || true)
legacy_revision=${JAILS_LEGACY_REVISION:-${default_revision:-}}
if [ -z "$legacy_revision" ]; then
  echo "[verify-rewrite-g1-canary] no branch point against main; name one with JAILS_LEGACY_REVISION=<rev>" >&2
  exit 1
fi

# A frozen revision that resolves to HEAD is the same vacuity arriving by
# another route -- an explicit `JAILS_LEGACY_REVISION=HEAD`, or a branch whose
# point is its own tip. Refuse rather than report green.
if [ "$(git -C "$repo_root" rev-parse "$legacy_revision^{commit}")" = "$(git -C "$repo_root" rev-parse 'HEAD^{commit}')" ]; then
  echo "[verify-rewrite-g1-canary] $legacy_revision is HEAD: that compares the binary under test with itself" >&2
  echo "                           name an earlier revision with JAILS_LEGACY_REVISION=<rev>" >&2
  exit 1
fi
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
