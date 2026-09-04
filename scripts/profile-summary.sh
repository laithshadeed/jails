#!/usr/bin/env bash
# **What the suite's toolchain subprocesses cost, printed on every verification run.**
set -uo pipefail
shopt -s nullglob
files=(target/jails-test-profile/*.tsv)
((${#files[@]})) || { echo "subprocess profile: nothing recorded"; exit 0; }
awk -F'\t' '
  $1=="span_ms"  { if ($2>span) span=$2 }
  $1=="queue_ms" { queue+=$2 }
  $1=="tool"     { runs[$2]+=$3; ms[$2]+=$4; total+=$4 }
  END {
    if (total==0 || span==0) { print "subprocess profile: nothing recorded"; exit }
    printf "subprocess cost:"
    for (t in ms) printf " %s %.1fs over %d;", t, ms[t]/1000, runs[t]
    printf "\n%.1fs of subprocess work in a %.1fs span (mean concurrency %.2f), %.1fs queued for a permit\n",
      total/1000, span/1000, total/span, queue/1000
  }' "${files[@]}"
