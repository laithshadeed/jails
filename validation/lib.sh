#!/usr/bin/env bash
#
# Shared harness for the workout validation scripts.
#
# Each `NN-<slug>.sh` sources this, runs a sequence of jails commands, and
# asserts on the Java that came out. The scripts are a SPEC: they encode the
# jails features the stacks gym needs, so a script failing means the feature
# is not built yet, not that you did something wrong. See README.md.
#
# Sourced, never run directly.

set -euo pipefail

readonly REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
readonly GYM="$REPO/stacks"

KEEP=false
[[ "${1:-}" == "--keep" ]] && KEEP=true

failures=0
step=0

# ---- reporting -------------------------------------------------------------

bold()    { printf '\033[1m%s\033[0m\n' "$*"; }
section() { printf '\n\033[1m[%s]\033[0m %s\n' "$1" "${2:-}"; }
pass()    { printf '  \033[32mok\033[0m    %s\n' "$*"; }
fail()    { printf '  \033[31mFAIL\033[0m  %s\n' "$*"; failures=$((failures + 1)); }
skip()    { printf '  \033[33mskip\033[0m  %s\n' "$*"; }

# Runs a jails command, echoing it first.
#
# Never aborts on failure: a missing generator should surface as every
# dependent assertion failing, so one run tells you the whole story instead of
# only the first thing that broke.
run() {
  step=$((step + 1))
  printf '\n\033[1m[%d]\033[0m %s\n' "$step" "$*"
  "$@" || fail "command exited non-zero: $*"
  return 0
}

# ---- assertions ------------------------------------------------------------

# has <file> <extended-regex> <description>
has() {
  local file="$1" pattern="$2" what="$3"
  if [[ ! -f "$file" ]]; then
    fail "$what -- ${file#"$PROJECT/"} does not exist"
  elif grep -Eq -- "$pattern" "$file"; then
    pass "$what"
  else
    fail "$what -- no /$pattern/ in ${file#"$PROJECT/"}"
  fi
}

# lacks <file> <extended-regex> <description>
# For rules whose whole point is an absence -- e.g. a blank check that must
# NOT be generated.
lacks() {
  local file="$1" pattern="$2" what="$3"
  if [[ ! -f "$file" ]]; then
    fail "$what -- ${file#"$PROJECT/"} does not exist"
  elif grep -Eq -- "$pattern" "$file"; then
    fail "$what -- unexpected /$pattern/ in ${file#"$PROJECT/"}"
  else
    pass "$what"
  fi
}

# exists <path> <description>
exists() {
  if [[ -e "$1" ]]; then pass "$2"; else fail "$2 -- ${1#"$PROJECT/"} missing"; fi
}

# rejects <description> -- asserts the given jails command FAILS. Used for
# input jails should refuse rather than silently accept.
rejects() {
  local what="$1"; shift
  if "$@" >/dev/null 2>&1; then
    fail "$what -- command unexpectedly succeeded: $*"
  else
    pass "$what"
  fi
}

# ---- project setup ---------------------------------------------------------

# start <project-name> -- fresh temp dir, `jails new-cli`, cd into it.
# Sets $PROJECT, $SRC, $TEST and the conventional package dirs.
start() {
  local name="$1"

  command -v jails >/dev/null || { echo "jails not on PATH -- cargo install --path ." >&2; exit 1; }
  command -v mvn   >/dev/null || { echo "mvn not on PATH" >&2; exit 1; }

  WORKDIR="$(mktemp -d "${TMPDIR:-/tmp}/jails-$name-XXXXXX")"
  PROJECT="$WORKDIR/$name"
  trap _cleanup EXIT

  bold "jails $(jails --version 2>/dev/null | awk '{print $2}')  ->  $WORKDIR"
  cd "$WORKDIR"

  # Plain Maven CLI project: hand-written pom, App.java dispatcher, JUnit +
  # AssertJ. No Spring -- the gym bans it outright.
  run jails new-cli "$name"
  cd "$PROJECT"

  local pkg="src/main/java/com/example/$name"
  SRC="$PROJECT/$pkg"
  TEST="$PROJECT/src/test/java/com/example/$name"
  DOMAIN="$SRC/domain"
  APP="$SRC/app"
  ADAPTERS="$SRC/adapters"
  CLI="$SRC/cli"
  API="$SRC/api"
  readonly PROJECT SRC TEST DOMAIN APP ADAPTERS CLI API
}

_cleanup() {
  if $KEEP; then
    echo; bold "kept: $PROJECT"
  else
    rm -rf "$WORKDIR"
  fi
}

# fixtures <slug> -- copy a gym fixture directory onto the test classpath and
# assert a few landed. Fixtures are shared with the TypeScript flavor and are
# the real spec for each workout's edge cases.
fixtures() {
  local slug="$1"; shift
  local dir="$GYM/fixtures/$slug"

  section fixtures "$slug on the test classpath"
  if [[ ! -d "$dir" ]]; then
    fail "missing gym fixtures: $dir"
    return 0
  fi
  cp -r "$dir" src/test/resources/fixtures/
  for f in "$@"; do
    exists "src/test/resources/fixtures/$slug/$f" "$f"
  done
}

# ---- the real bar ----------------------------------------------------------

# build -- compile and run the generated suite.
#
# Calls mvn directly rather than `jails test`: jails prefers mvnd, and if
# mvnd's daemon runs an older JDK than the release jails targets, the forked
# test JVM cannot load the classes it just compiled. That would be measuring
# the toolchain, not the generated code.
build() {
  section build "mvn test"
  if mvn -q test >"$WORKDIR/mvn.log" 2>&1; then
    pass "generated project compiles and passes"
  else
    fail "mvn test failed -- see $WORKDIR/mvn.log"
    $KEEP || tail -30 "$WORKDIR/mvn.log"
  fi
}

# verdict <workout label> -- final summary and exit code.
verdict() {
  echo
  if (( failures == 0 )); then
    bold "PASS -- $1 scaffolds clean"
  else
    bold "FAIL -- $failures check(s) failed for $1"
  fi
  exit $(( failures > 0 ))
}
