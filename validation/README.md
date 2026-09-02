# Workout validation scripts

Ten scripts, one per stacks workout. Each runs a sequence of `jails` commands
against a throwaway project and asserts on the Java that comes out.

**These are a spec, not a test suite.** A failing script means jails does not
have a feature yet, *or* that the script asks for something jails has decided
is wrong. Read the refusal before changing jails.

```bash
./validation/01-normalise.sh          # one workout
./validation/01-normalise.sh --keep   # leave the project in /tmp to poke at
for f in validation/[0-9]*.sh; do "$f"; done   # all ten
```

Every script is self-contained: fresh `jails new-cli`, fresh temp dir, cleaned
up on exit. `lib.sh` holds the shared harness (`run`, `has`, `lacks`,
`exists`, `rejects`, `fixtures`, `build`, `verdict`).

Two failures are environmental on any machine without the full toolchain and
say nothing about jails: `mvn test` needs a JDK matching `pom::TARGET_RELEASE`,
and `fixtures` reads `stacks/fixtures/`, an untracked sibling checkout like
`deps/`. Run on a box with both before reading a failure as a missing feature.

The one known real failure is naming: workouts 05 and 10 expect
`Sqlite<Name>Repository` where `g repo` names its adapter
`Jdbc<Name>Repository`.
