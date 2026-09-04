#!/usr/bin/env bash
# Compatibility wrapper: delegates to scripts/run-bounded.sh
exec "$(dirname "$0")/run-bounded.sh" "$@"
