#!/usr/bin/env bash
# **Build production-ready binaries for jails.**
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"
DIST_DIR="${ROOT_DIR}/dist"

MODE="release"
INSTALL=0
INSTALL_DIR="${INSTALL_DIR:-$HOME/.local/bin}"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --debug)
      MODE="debug"
      shift
      ;;
    --release)
      MODE="release"
      shift
      ;;
    --install)
      INSTALL=1
      shift
      ;;
    --install-dir)
      INSTALL_DIR="$2"
      INSTALL=1
      shift 2
      ;;
    -h|--help)
      echo "usage: $0 [--release|--debug] [--install] [--install-dir <path>]"
      exit 0
      ;;
    *)
      echo "unknown option: $1" >&2
      exit 2
      ;;
  esac
done

cd "${ROOT_DIR}"

echo "Building production-ready 'jails' binary (${MODE} profile)..."

# Use mold or lld if available
if command -v mold >/dev/null 2>&1; then
  export RUSTFLAGS="-C link-arg=-fuse-ld=mold ${RUSTFLAGS:-}"
elif command -v ld.lld >/dev/null 2>&1 || command -v lld >/dev/null 2>&1; then
  export RUSTFLAGS="-C link-arg=-fuse-ld=lld ${RUSTFLAGS:-}"
fi

if [[ "$MODE" == "release" ]]; then
  cargo build --release --locked --bin jails
  SRC_BIN="${ROOT_DIR}/target/release/jails"
else
  cargo build --locked --bin jails
  SRC_BIN="${ROOT_DIR}/target/debug/jails"
fi

mkdir -p "${DIST_DIR}"
DEST_BIN="${DIST_DIR}/jails"
cp -f "${SRC_BIN}" "${DEST_BIN}"
chmod +x "${DEST_BIN}"

# Binary metrics
size_human=$(du -h "${DEST_BIN}" | awk '{print $1}')
sha=$(sha256sum "${DEST_BIN}" | awk '{print $1}')
version=$("${DEST_BIN}" --version 2>/dev/null || echo "jails 0.1.0")

echo "=========================================="
echo "PRODUCTION BINARY READY: ${DEST_BIN}"
echo "  Version: ${version}"
echo "  Size:    ${size_human}"
echo "  SHA256:  ${sha}"
echo "=========================================="

if [[ "$INSTALL" -eq 1 ]]; then
  mkdir -p "${INSTALL_DIR}"
  cp -f "${DEST_BIN}" "${INSTALL_DIR}/jails"
  chmod +x "${INSTALL_DIR}/jails"
  echo "Installed to: ${INSTALL_DIR}/jails"
fi
