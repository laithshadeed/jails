#!/usr/bin/env bash
# **Run a command inside a cgroup that cannot take the machine down.**
#
# The suite is subprocess-bound: Maven JVMs, container starts, `jails` trees.
# Its own permit pools bound what *it* starts, but a pool decided at start-up
# cannot see the editor, the browser or a second `cargo` that arrive later,
# and one gate run has taken a 30 GB machine into swap and out. A cgroup is the
# kernel saying no at the boundary, whatever the pools believe.
#
# `systemd-run --user --scope` puts the command in a transient scope of the
# caller's session: same environment, same stdio, same exit status, and the
# limits apply to every descendant -- Maven, Surefire, Docker's client, all of
# it. Where systemd is not available (CI containers, macOS) the command runs
# as it is, and the harness budgets are the only bound.
#
# Sizes, overridable by environment:
#   JAILS_GATE_MEMORY_MB  MemoryMax for the scope, in MiB (default: half of MemTotal)
#   JAILS_GATE_CPU        CPUQuota in cores            (default: three quarters of nproc)
#   RUST_TEST_THREADS     libtest's threads; set here to twice the quota when unset,
#                         so the long tests all start early and queue on the pools
#
# `MemoryHigh` sits just under the cap so the kernel throttles and reclaims
# before it kills: a slow test is better than a dead JVM. The harness reads
# the scope's `memory.max` and `cpu.max` back (`tests/common/mod.rs`) and
# derives its JVM permits and thread budget from them, so the two cannot
# disagree about how big the machine is.
set -euo pipefail

if [ "$#" -eq 0 ]; then
    echo "usage: scripts/bounded.sh <command> [args...]" >&2
    exit 2
fi

total_kib=$(awk '/^MemTotal:/ {print $2}' /proc/meminfo 2>/dev/null || echo 0)
cores=$(nproc 2>/dev/null || echo 4)
memory_mb=${JAILS_GATE_MEMORY_MB:-$(( total_kib / 2048 ))}
cpu=${JAILS_GATE_CPU:-$(( cores * 3 / 4 ))}
[ "$cpu" -ge 1 ] || cpu=1
export RUST_TEST_THREADS="${RUST_TEST_THREADS:-$(( cpu * 2 ))}"

if command -v systemd-run >/dev/null 2>&1 \
    && systemd-run --user --scope -q -p MemoryMax="${memory_mb}M" true >/dev/null 2>&1; then
    # `MemoryHigh` is `MemoryMax` less one JVM's worth, so throttling starts
    # with room to finish the run that tipped it over.
    high_mb=$(( memory_mb - 700 ))
    [ "$high_mb" -gt 0 ] || high_mb=$(( memory_mb / 2 ))
    exec systemd-run --user --scope -q --collect \
        -p MemoryMax="${memory_mb}M" \
        -p MemoryHigh="${high_mb}M" \
        -p MemorySwapMax=0 \
        -p CPUQuota="$(( cpu * 100 ))%" \
        -- "$@"
fi

echo "bounded: systemd-run is not available here; running unbounded" >&2
exec "$@"
