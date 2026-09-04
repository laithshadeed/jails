#!/usr/bin/env bash
# Compatibility wrapper: delegates to scripts/check.sh
exec "$(dirname "$0")/check.sh" "$@"
