#!/usr/bin/env bash
# Compatibility wrapper: delegates to scripts/test.sh
exec "$(dirname "$0")/test.sh" "$@"
