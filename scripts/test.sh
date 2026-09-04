#!/usr/bin/env bash
# **Run every test executable in the workspace concurrently, plus doctests.**
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
exec python3 "${SCRIPT_DIR}/test_runner.py" "$@"
