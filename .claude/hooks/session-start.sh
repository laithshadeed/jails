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
# which is why `JAILS_TOOLCHAIN=1` turns that skip into a failure and why this
# hook is worth its startup cost: measure with the toolchain or measure
# nothing.
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

# --- the same toolchain outside the repository ------------------------------
#
# **A mise shim resolves its version from the *current directory*, and the
# tests do not run in this one.** `mise.toml` pins the toolchain for
# `/home/user/jails`; a tier-3 test generates a project into a scratch
# directory and runs `mvn`, `mvnd` and `java` *there*, where there is no
# `mise.toml` and the shim falls back to the global config -- empty, in a
# session whose hook installed everything per-project. `CLAUDE.md` records
# what that costs, and the three failures differ enough that none of them
# names the cause:
#
#   * `mvnd` exits with `mise ERROR No version is set for shim: mvnd`.
#   * `mvn` *passes*, on whatever system Maven is on PATH -- 3.9.11 here
#     against the pinned 3.9.16 -- so the suite silently tests the wrong one.
#   * `java` is the dangerous one: `jails testd` compiles its daemon with the
#     single-file source launcher, so a wrong JDK is an
#     `UnsupportedClassVersionError` inside a process whose output nobody
#     reads, surfacing as four `tooling::` tests failing with an empty report.
#
# The versions are read back out of the project rather than repeated here, so
# this cannot drift from `mise.toml` the way a second copy of a pinned version
# always does. `mise use -g` rewrites the global config in place and is safe to
# repeat.
log "pinning the same toolchain globally, for the scratch directories"
(cd "$project" && mise ls --current --json) | python3 -c '
import json, sys
for tool, entries in json.load(sys.stdin).items():
    for entry in entries:
        if entry.get("active"):
            print(tool + "@" + entry["requested_version"])
            break
' | while read -r pin; do
  mise use -g "$pin" >/dev/null 2>&1 || log "could not pin $pin globally"
done
# The Gradle example needs JDK 21 and finds it through `mise which java@21`,
# which is a lookup rather than a default -- so 21 is deliberately *not* global.
# Pinning it here would hand every scratch directory a JDK that cannot compile
# `--release 26`, which is the failure this whole section exists to remove.

# --- the Gradle example's dependencies, fetched once --------------------------
#
# **Maven Central answers 429 through the sandbox proxy, and Gradle is the one
# consumer with a cold cache.** `~/.m2` arrives warm enough that the Maven tier
# passes; `~/.gradle` does not, so
# `unheld_gradle_example_manifest_builds_on_its_pinned_toolchain` spends its
# only attempt resolving `spring-boot-gradle-plugin` and
# `spring-boot-dependencies` from Central and fails on
# `Received status code 429 from server: Too Many Requests`. That reads as a
# broken build and is a rate limit.
#
# Resolving them here moves that fetch into the hook, whose container state is
# cached, so the suite finds them locally and the next session does not fetch
# them at all. **Best-effort by construction**: a failure here logs and the
# session continues, because a warm cache is an optimisation and the test can
# still reach the network itself.
#
# The Boot version is read out of the example's own README rather than written
# down again -- this repository's rule is that a pinned version has one owner,
# and a second copy in a provisioning script is exactly the copy nobody updates.
boot_version="$(grep -o -- '--boot [0-9][^ ]*' \
  "$project/examples/minicom-spring/README.md" 2>/dev/null | head -1 | awk '{print $2}')"
if [ -n "$boot_version" ] && [ ! -d "$HOME/.gradle/caches/modules-2/files-2.1/org.springframework.boot" ]; then
  log "warming the Gradle cache for Boot $boot_version"
  warm_dir="$(mktemp -d)"
  printf "rootProject.name = 'warm'\n" > "$warm_dir/settings.gradle"
  cat > "$warm_dir/build.gradle" <<GRADLE
buildscript {
  repositories { mavenCentral() }
  dependencies { classpath "org.springframework.boot:spring-boot-gradle-plugin:$boot_version" }
}
repositories { mavenCentral() }
configurations { warm }
dependencies { warm platform("org.springframework.boot:spring-boot-dependencies:$boot_version") }
tasks.register('warmUp') { doLast { configurations.warm.resolve() } }
GRADLE
  # The example's own toolchain: Gradle 8.5 on JDK 21, which is what resolves
  # the 2.7.x line. Running it under the repository default would warm the
  # cache for a Gradle the test never uses.
  (cd "$warm_dir" && mise x java@21 gradle@8.5 -- gradle --no-daemon --quiet warmUp) \
    >/dev/null 2>&1 || log "could not warm the Gradle cache; the example test will fetch it itself"
  rm -rf "$warm_dir"
fi

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
#
# **The stale pid file is why this is not just `dockerd &`.** A daemon that
# died leaves `/var/run/docker.pid` behind, and dockerd refuses to start over
# it -- "process with PID N is still running", about a process that is not.
# The session then looks like one that never had a container engine, which is
# a quieter and more misleading failure than the one that actually happened.
if ! docker info >/dev/null 2>&1 && command -v dockerd >/dev/null 2>&1; then
  if [ -f /var/run/docker.pid ] && ! kill -0 "$(cat /var/run/docker.pid)" 2>/dev/null; then
    log "removing a pid file left by a dead dockerd"
    rm -f /var/run/docker.pid
  fi
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
# the host JDK did.
#
# **A retagged local image does not reach it, and cannot be made to.** The
# generated Dockerfile opens with `# syntax=docker/dockerfile:1`, and that
# external frontend resolves every `FROM` against the registry -- so whatever
# `maven:...` points to locally is simply not consulted. Measured on this
# machine: the identical build reports 154 imported CA certificates in its base
# with the directive removed and **zero** with it present, and no arrangement
# of tags, `--pull=false`, `docker rmi`, `buildx prune` or a daemon restart
# moves that number. So the image is published under a name of its own and the
# gate is told to substitute it, through `JAILS_OCI_BASE_IMAGES` and
# `--build-context`, which is the one mechanism the frontend does honour.
#
# **Import the bundle into the image's own `cacerts`; never point the JVM at
# `java-truststore.p12`.** The environment builds that store for the host and
# it is missing exactly the certificates that matter -- 152 of the bundle's
# 154, and the two it leaves out are the `CCR agent-proxy interception CA`,
# which is what actually signs the connection. Setting
# `-Djavax.net.ssl.trustStore` at it does not merely fail to help: it
# *replaces* the JDK's own store, so it is strictly worse than doing nothing.
# An earlier version of this hook set it, which is how a fixable image became
# a permanently broken one. `mvn -B -ntp validate` inside the container
# reproduces both answers in about four seconds if this is ever in doubt.
#
# **The image is keyed on the bundle's content, because the CA rotates.** Its
# common name carries a month (`... (production) 2026-08`) and `/root/.ccr` is
# regenerated per session, while the container image store survives into the
# next one. A guard that asked "does the trusted image exist" answered yes
# about an image built against a CA that no longer signs anything.
#
# **Two CAs sign here, not one, which is why the whole bundle goes in.** The
# host's own traffic is signed by the `CCR agent-proxy interception CA`; a
# BuildKit `RUN` is signed by `sandbox-egress-gateway-production Egress Gateway
# CA` instead. Both are in `ca-bundle.crt` and only one is in any subset of
# it.
#
# The tag is read out of the template and `TARGET_RELEASE` rather than written
# here, because a second copy of it would drift the moment either moves.
ca_bundle="${SSL_CERT_FILE:-/root/.ccr/ca-bundle.crt}"
if [ -f "$ca_bundle" ] && docker info >/dev/null 2>&1; then
  release=$(sed -n 's/.*TARGET_RELEASE: &str = "\([0-9]*\)".*/\1/p' \
    "$project/crates/jails-project/src/pom.rs" | head -1)
  base=$(sed -n 's/^FROM \([^ ]*\) AS build/\1/p' \
    "$project/templates/add/dockerfile_build_maven" | head -1)
  base="${base//\{\{RELEASE\}\}/$release}"
  stamp=$(sha256sum "$ca_bundle" | cut -c1-12)
  trusted="jails-ca-trusted:$release-$stamp"
  if [ -n "$release" ] && [ -n "$base" ] && \
     ! docker image inspect "$trusted" >/dev/null 2>&1; then
    log "trusting the proxy CA for $base"
    build_dir="$(mktemp -d)"
    cp "$ca_bundle" "$build_dir/ccr-ca-bundle.crt"
    cat > "$build_dir/Dockerfile" <<DOCKERFILE
FROM $base
COPY ccr-ca-bundle.crt /usr/local/share/ca-certificates/ccr-ca-bundle.crt
# Split and import one at a time: keytool takes a single certificate per
# -importcert, and a bundle handed to it whole becomes one entry that nothing
# validates a chain against.
RUN set -eu; \\
    csplit -z -s -f /tmp/ccr- -b '%03d.pem' \\
      /usr/local/share/ca-certificates/ccr-ca-bundle.crt '/BEGIN CERTIFICATE/' '{*}'; \\
    for pem in /tmp/ccr-*.pem; do \\
      keytool -importcert -noprompt -trustcacerts \\
        -keystore "\$JAVA_HOME/lib/security/cacerts" -storepass changeit \\
        -alias "ccr-\$(basename "\$pem" .pem)" -file "\$pem" >/dev/null 2>&1 || true; \\
    done; \\
    rm -f /tmp/ccr-*.pem; \\
    if command -v update-ca-certificates >/dev/null 2>&1; then \\
      update-ca-certificates >/dev/null 2>&1 || true; \\
    fi
# Explicitly empty, not absent: an inherited -Djavax.net.ssl.trustStore would
# override the cacerts imported above with a store that does not carry the
# interception CA.
ENV MAVEN_OPTS=""
DOCKERFILE
    if docker build --tag "$trusted" "$build_dir" >/dev/null 2>&1; then
      log "trusted $(grep -c 'BEGIN CERTIFICATE' "$ca_bundle") CA(s) for $base"
    else
      log "could not pre-trust $base; the OCI image gate will fail"
    fi
    rm -rf "$build_dir"
  fi
  # Asked of the daemon rather than tracked through the branches above, so
  # "already built", "built now" and "failed" reach the export as one answer.
  docker image inspect "$trusted" >/dev/null 2>&1 || trusted=
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
  # What the OCI image gate should build a generated `FROM` against. Empty on
  # any machine where the trusted image could not be built, so the gate falls
  # back to building exactly what jails wrote and fails honestly rather than
  # against a substitution that is not there.
  [ -n "${trusted:-}" ] && [ -n "${base:-}" ] \
    && echo "export JAILS_OCI_BASE_IMAGES=\"$base=$trusted\""
} >> "$env_file"

log "ready"
