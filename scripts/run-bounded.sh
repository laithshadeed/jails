#!/usr/bin/env bash
# **Run a command inside a cgroup with strict CPU, memory, and I/O throttling.**
#
# Hardware empathy:
# - In dedicated CI ($CI / $GITHUB_ACTIONS): allocate runner capacity cleanly.
# - On interactive local workstations: strictly throttle CPU (at most half cores, max 8),
#   throttle I/O (idle ionice -c 3), lower process priority (nice 19),
#   and cap memory with 0 SWAP so the desktop/editor never lags or freezes.
set -euo pipefail

if [ "$#" -eq 0 ]; then
    echo "usage: scripts/run-bounded.sh <command> [args...]" >&2
    exit 2
fi

total_kib=$(awk '/^MemTotal:/ {print $2}' /proc/meminfo 2>/dev/null || echo 0)
cores=$(nproc 2>/dev/null || echo 4)

if [ -n "${CI:-}" ] || [ -n "${GITHUB_ACTIONS:-}" ]; then
    # Dedicated CI runner: use machine cores and 85% RAM
    cpu=${GATE_CPU:-${CI_CPU:-${JAILS_GATE_CPU:-$cores}}}
    memory_mb=${GATE_MEMORY_MB:-${JAILS_GATE_MEMORY_MB:-$(( total_kib * 85 / 1024 / 100 ))}}
    threads=${RUST_TEST_THREADS:-$(( cpu * 2 ))}
    nice_level=0
    cpu_weight=100
    io_weight=100
    toolchain_procs=${JAILS_TEST_MAX_TOOLCHAIN_PROCESSES:-$cpu}
else
    # Interactive workstation: strict bounds so machine never lags or thrashes
    # Cap CPU to at most half the machine or 8 cores, whichever is smaller
    workstation_cpu=$(( cores > 8 ? 8 : (cores > 2 ? cores / 2 : 1) ))
    [ "$workstation_cpu" -ge 1 ] || workstation_cpu=1
    cpu=${GATE_CPU:-$workstation_cpu}
    # Cap memory to at most 8 GB or 1/3 of total RAM
    max_local_mb=$(( total_kib / 1024 / 3 ))
    [ "$max_local_mb" -le 8192 ] || max_local_mb=8192
    [ "$max_local_mb" -ge 2048 ] || max_local_mb=2048
    memory_mb=${GATE_MEMORY_MB:-$max_local_mb}
    threads=${RUST_TEST_THREADS:-$(( cpu > 4 ? 4 : cpu ))}
    nice_level=19
    cpu_weight=20
    io_weight=20
    toolchain_procs=${JAILS_TEST_MAX_TOOLCHAIN_PROCESSES:-2}
fi

[ "$cpu" -ge 1 ] || cpu=1
export RUST_TEST_THREADS="$threads"
export JAILS_TEST_MAX_TOOLCHAIN_PROCESSES="$toolchain_procs"

if [ -z "${RUSTFLAGS:-}" ]; then
    if command -v mold >/dev/null 2>&1; then
        export RUSTFLAGS="-C link-arg=-fuse-ld=mold"
    elif command -v ld.lld >/dev/null 2>&1 || command -v lld >/dev/null 2>&1; then
        export RUSTFLAGS="-C link-arg=-fuse-ld=lld"
    fi
fi

# Check systemd-run capability
if command -v systemd-run >/dev/null 2>&1 \
    && systemd-run --user --scope -q -p MemoryMax="${memory_mb}M" true >/dev/null 2>&1; then
    high_mb=$(( memory_mb - 500 ))
    [ "$high_mb" -gt 0 ] || high_mb=$(( memory_mb / 2 ))
    
    if ionice -c 3 true >/dev/null 2>&1; then
        exec systemd-run --user --scope -q --collect \
            -p MemoryMax="${memory_mb}M" \
            -p MemoryHigh="${high_mb}M" \
            -p MemorySwapMax=0 \
            -p OOMPolicy=continue \
            -p CPUQuota="$(( cpu * 100 ))%" \
            -p CPUWeight="${cpu_weight}" \
            -p IOWeight="${io_weight}" \
            -- nice -n "${nice_level}" ionice -c 3 "$@"
    else
        exec systemd-run --user --scope -q --collect \
            -p MemoryMax="${memory_mb}M" \
            -p MemoryHigh="${high_mb}M" \
            -p MemorySwapMax=0 \
            -p OOMPolicy=continue \
            -p CPUQuota="$(( cpu * 100 ))%" \
            -p CPUWeight="${cpu_weight}" \
            -p IOWeight="${io_weight}" \
            -- nice -n "${nice_level}" ionice -c 2 -n 7 "$@"
    fi
fi

echo "run-bounded: systemd-run is not available; running with nice + ionice" >&2
if ionice -c 3 true >/dev/null 2>&1; then
    exec nice -n "$nice_level" ionice -c 3 "$@"
else
    exec nice -n "$nice_level" ionice -c 2 -n 7 "$@"
fi
