#!/usr/bin/env bash
# Every container image the suite starts, read out of the tree.
#
# **Derived, never declared.** A second list of pinned tags is a list that
# drifts -- and the failure is silent, because warming the wrong tag looks
# exactly like warming the right one. The suite's own sources are the
# authority: `tests/common` names what the harness starts, and the compiler's
# capability packs name what a generated `compose.yaml` declares.
#
# Used by `.github/workflows/verify-rewrite.yml` to pull these while the Rust
# build is still running, so the first test that wants a container does not
# wait for a registry.
set -euo pipefail

cd "$(dirname "$0")/.."

images=$(grep -rhoE '"(postgres|redis|apache/kafka|axllent/mailpit|ghcr\.io/[a-z0-9./-]+):[A-Za-z0-9._-]+"' \
    tests crates templates | tr -d '"' | sort -u)

# A scanner that has lost the code reports exactly the same clean result as one
# that read it all, so it says how many it found and refuses an empty answer.
count=$(printf '%s\n' "$images" | grep -c . || true)
if [ "$count" -lt 4 ]; then
    echo "container-images: found only $count image(s); the scan has lost the tree" >&2
    exit 1
fi

printf '%s\n' "$images"
