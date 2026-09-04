#!/usr/bin/env bash
# **Warm container images in the background during early pipeline setup.**
exec "$(dirname "$0")/container-images.sh" "$@"
