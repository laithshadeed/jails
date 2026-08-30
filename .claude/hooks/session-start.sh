#!/bin/bash
#
# Make a Claude Code on the web session able to run this repository's one gate.
#
# `mise run verify-rewrite` is the only answer to "is this green" (CLAUDE.md),
# and the base web image cannot run it. Three things are missing, and each one
# fails in a way that looks like a product defect:
#
#   * the toolchain `mise.toml` pins. The image ships JDK 21; generated
#     projects declare `--release 26`, so `javac` refuses outright and about
#     fifty tier-3 tests go red with `release version 26 not supported`.
#   * a JDK that trusts the sandbox's TLS proxy. The environment supplies one
#     through `JAVA_TOOL_OPTIONS`, and the suite *replaces* that variable with
#     its own GC flags (`REAL_JAVA_TOOL_OPTIONS` in `tests/common/mod.rs`) --
#     so every Maven download dies on `PKIX path building failed`. The import
#     below puts the CA where nothing can drop it.
#   * a running container engine, for Testcontainers, the compose capabilities
#     and the generated OCI image gate.
#
# **Without them the suite does not merely fail, it lies.** A tier-3 test that
# cannot find its toolchain calls `common::skip()` and is counted as passing,
# which is why `JAILS_REQUIRE_TOOLCHAIN=1` exists and why this hook is worth
# its startup cost: measure with the toolchain or measure nothing.
#
# Everything here is idempotent and confined to the machine -- no repository
# file is written -- so re-running it on `resume` or `clear` is free.
set -euo pipefail

[ "${CLAUDE_CODE_REMOTE:-}" = "true" ] || exit 0

log() { echo "session-start: $*" >&2; }
project="${CLAUDE_PROJECT_DIR:-$(cd "$(dirname "$0")/../.." && pwd)}"
env_file="${CLAUDE_ENV_FILE:-/dev/null}"
export PATH="$HOME/.local/bin:$PATH"

# --- the toolchain `mise.toml` pins -----------------------------------------
#
# mise rather than a hand-rolled download because the repository already
# depends on it by name: `.claude/settings.json`'s Stop hook and
# `.githooks/pre-push` both run `mise run verify-rewrite` and nothing else.
# A session without mise has been running that hook into a "command not found"
# it reports as a gate failure.
if ! command -v mise >/dev/null 2>&1; then
  log "installing mise"
  curl -fsSL https://mise.run | sh >/dev/null
fi
export PATH="$HOME/.local/share/mise/shims:$PATH"
mise trust --quiet "$project/mise.toml" >/dev/null 2>&1 || true
log "installing the pinned toolchain"
(cd "$project" && mise install)
# JDK 21 is not the repository default and is not in `mise.toml`. One test
# needs it: `unheld_gradle_example_manifest_builds_on_its_pinned_toolchain`
# pins Gradle 8.5 on JDK 21, and locates both through `mise which` precisely so
# it can run inside the ordinary suite rather than in a second `cargo test`.
mise install java@21

# --- a JDK that trusts the sandbox's TLS proxy ------------------------------
#
# **The whole bundle, diffed by fingerprint -- not the two-certificate file
# next to it.** `/root/.ccr/agent-proxy-ca.crt` carries two CAs and the
# sandbox intercepts with six: an egress-gateway CA and a TLS-inspection CA
# are in `ca-bundle.crt` only. Trusting the two looked right and failed under
# load, because which CA signs a connection varies -- a cold `mvn` succeeded by
# hand and the same artifact failed inside a parallel suite, which reads
# exactly like flaky infrastructure and is not.
#
# So: every certificate in the bundle the JDK does not already trust. The
# fingerprint diff is what keeps that cheap (one `keytool -list`, then openssl
# per certificate) and what keeps it honest -- it names no issuer, so a CA the
# sandbox adds tomorrow is picked up without editing this file.
ca_bundle="${SSL_CERT_FILE:-/root/.ccr/ca-bundle.crt}"
if [ -f "$ca_bundle" ] && command -v openssl >/dev/null 2>&1; then
  split_dir="$(mktemp -d)"
  csplit -z -s -f "$split_dir/ca-" -b "%03d.pem" "$ca_bundle" '/BEGIN CERTIFICATE/' '{*}'
  for java_home in "$HOME"/.local/share/mise/installs/java/*/; do
    keytool="$java_home/bin/keytool"
    [ -x "$keytool" ] || continue
    known=$("$keytool" -list -cacerts -storepass changeit 2>/dev/null \
      | grep -oE '[0-9A-F]{2}(:[0-9A-F]{2}){31}' | tr -d ':' | tr 'A-F' 'a-f')
    added=0
    for pem in "$split_dir"/ca-*.pem; do
      fingerprint=$(openssl x509 -in "$pem" -noout -fingerprint -sha256 2>/dev/null \
        | sed 's/.*=//' | tr -d ':' | tr 'A-F' 'a-f')
      [ -n "$fingerprint" ] || continue
      case "$known" in *"$fingerprint"*) continue ;; esac
      "$keytool" -importcert -noprompt -trustcacerts -cacerts -storepass changeit \
        -alias "ccr-$fingerprint" -file "$pem" >/dev/null 2>&1 && added=$((added + 1))
    done
    [ "$added" -gt 0 ] && log "trusted $added CA(s) in $(basename "$java_home")"
  done
  rm -rf "$split_dir"
fi

# --- a container engine ------------------------------------------------------
#
# Best-effort: a session with no daemon is still worth starting, and every test
# that needs one already reports itself skipped.
if ! docker info >/dev/null 2>&1 && command -v dockerd >/dev/null 2>&1; then
  log "starting dockerd"
  (dockerd >/tmp/dockerd.log 2>&1 &)
  for _ in $(seq 1 30); do
    docker info >/dev/null 2>&1 && break
    sleep 1
  done
  docker info >/dev/null 2>&1 || log "dockerd did not come up; see /tmp/dockerd.log"
fi

# --- the base image the generated Dockerfile builds in -----------------------
#
# `templates/add/dockerfile_build_maven` runs `mvn package` *inside* a
# container, which meets the same intercepting proxy and trusts it no more than
# the host JDK did. The image gate builds with `--pull=false` (a deliberate
# choice -- see `verified_app_images`), so a locally retagged copy of the base
# image is what its `FROM` resolves to.
#
# The environment already ships the bundle as a Java truststore, and the build
# stage runs `mvn -DskipTests`, so `MAVEN_OPTS` reaches the only JVM involved.
# No `keytool` loop is needed here, and none should be added: this is a builder
# stage that never enters the published image.
#
# The tag is read out of the template and `TARGET_RELEASE` rather than written
# here, because a second copy of it would drift the moment either moves.
truststore=/root/.ccr/java-truststore.p12
if [ -f "$truststore" ] && docker info >/dev/null 2>&1; then
  release=$(sed -n 's/.*TARGET_RELEASE: &str = "\([0-9]*\)".*/\1/p' \
    "$project/crates/jails-project/src/pom.rs" | head -1)
  base=$(sed -n 's/^FROM \([^ ]*\) AS build/\1/p' \
    "$project/templates/add/dockerfile_build_maven" | head -1)
  base="${base//\{\{RELEASE\}\}/$release}"
  if [ -n "$release" ] && [ -n "$base" ] && \
     ! docker image inspect "jails-ca-trusted:$release" >/dev/null 2>&1; then
    log "trusting the proxy CA in $base"
    build_dir="$(mktemp -d)"
    cp "$truststore" "$build_dir/ccr-truststore.p12"
    cat > "$build_dir/Dockerfile" <<DOCKERFILE
FROM $base
COPY ccr-truststore.p12 /etc/ccr-truststore.p12
ENV MAVEN_OPTS="-Djavax.net.ssl.trustStore=/etc/ccr-truststore.p12 -Djavax.net.ssl.trustStorePassword=changeit -Djavax.net.ssl.trustStoreType=PKCS12"
DOCKERFILE
    # Tagged as the base image so the generated `FROM` finds it, and again
    # under a name of our own so the check above can tell "already done" from
    # "somebody pulled the real one".
    docker build --tag "$base" --tag "jails-ca-trusted:$release" "$build_dir" >/dev/null 2>&1 \
      || log "could not pre-trust $base; the OCI image gate will fail"
    rm -rf "$build_dir"
  fi
fi

# --- what the session inherits ----------------------------------------------
{
  echo "export PATH=\"\$HOME/.local/bin:\$HOME/.local/share/mise/shims:\$PATH\""
  java_home="$(cd "$project" && mise where java 2>/dev/null || true)"
  # Maven picks its JDK from JAVA_HOME, not from the `javac` on PATH, and the
  # two disagreeing is exactly how a suite comes to test a release nobody
  # asked for.
  [ -n "$java_home" ] && echo "export JAVA_HOME=\"$java_home\""
  # `git merge-file --diff-algorithm` reached git only in 2.44 and this image
  # ships 2.43. jails probes for it, but the gate pins the empty value so one
  # answer to "is this green" cannot depend on the distribution underneath it.
  echo "export JAILS_GIT_DIFF_ALGORITHM="
} >> "$env_file"

log "ready"
