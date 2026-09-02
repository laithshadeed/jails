<!--
Workstream D. `docs/00-contracts.md` carries the contracts and the ownership
rules; nothing here repeats them.

**A closed item is deleted from this file**, in the commit that closes it.
Item numbers are stable and never reused.
-->

# 40 — The gates, the suite, and the CI budget

**Read `docs/00-contracts.md` first.**

## What you own

`.github/workflows/**`, `.githooks/**`, `mise.toml`, `scripts/**`,
`tests/common/**`, `tests/architecture/**`, `tests/corpus/**`. You own **how
anything is known to be green**, and the wall clock it costs.

## What you do not touch

You do not fix product defects. When a gate you sharpen turns something red,
the fix belongs to whoever owns the path; hand it over with the reproduction.
The exception is a defect *in the harness*: a fixture that lies, a skip that
reports as a pass, an oracle comparing a binary with itself.

## The specification sections this work answers to

§21 conformance test suite.

## How you know you are green

```
mise run verify-rewrite
JAILS_TEST_PROFILE=1 cargo test --test cli -- --nocapture   # the subprocess profile
cargo test --test architecture -- --nocapture --test-threads=1   # the board
```

**Every measurement needs `JAILS_TOOLCHAIN=1` and a container engine.**
Without them the container-dependent tests fail in milliseconds and the run
measures the suite minus a third of its work.

---

# The gates

`mise run verify-rewrite` is the single answer to "is this green".
`.githooks/pre-push` and `.github/workflows/verify-rewrite.yml` invoke it and
nothing else. It runs `cargo fmt --check`, `cargo clippy -D warnings`,
`RUSTDOCFLAGS='-D warnings' cargo doc`, and `cargo test --workspace` under
`JAILS_GIT_DIFF_ALGORITHM= JAILS_TOOLCHAIN=1`.

Both pins are load-bearing. Without the second, a tier-3 test that cannot find
its toolchain skips and counts as a pass. Without the first the gate is not
deterministic: `git merge-file`'s histogram and myers algorithms can resolve an
ambiguous three-way merge differently, and the merged bytes go into the managed
tree. The empty value pins git's own default, which every git supports.

## G0–G5

- **G0 — mandatory execution and protocol.** Scenario names and golden
  directories match exactly; every protocol fixture is read by something;
  every advertised failpoint is named outside the registry.
- **G1 — the product loop.** `tests/product_loop.rs`, 38 scenarios plus the
  corpus, each run against the binary.
- **G2 — behavior journeys.** Every advertised command path maps to a test,
  held in two places: `every_advertised_command_path_has_a_journey`
  (`tests/cli/developer_tools.rs`) walks `jails commands --json` and requires
  each path in a test's argv, and `every_inventoried_command_path_is_invoked_by_a_test`
  (`tests/architecture/rules.rs`) holds `docs/feature-inventory.tsv`'s half.
  Coverage may not fall, and an exemption no longer needed must come off. It
  proves reachability, not behaviour: a refusal counts, and the match is
  textual.
- **G3 — exact real toolchain.** `tests/common/scenarios.rs` is the
  machine-readable kind/capability map, held by
  `every_kind_and_capability_has_a_golden_scenario` against the binary's own
  help. `format` is the one documented exemption in `COVERED_ELSEWHERE`.
- **G4 — crash/recovery.** `failpoints!` is one declaration: it emits
  `POINTS`, the list a crash test enumerates, and one constant per point, the
  only thing `trip` accepts. A point nobody trips is an unused constant and
  `-D dead-code` fails the build. `crates/jails-workspace/tests/crash.rs`
  declares nine points over the publication sequence and asserts convergence,
  in-process and in an aborting child.
- **G5 — real-project corpus.** `tests/corpus/` holds five checked-in
  project trees jails did not write, with `policy.tsv` accounting for every
  one. It grows by dropping a directory in. `core` is not a synonym for
  `domain`: it means the domain model in one codebase and shared framework
  glue in the next, and `spring-renamed-layers` pins the refusal.

---

# The CI budget

**The whole `verify` job must finish inside five minutes** -- set-up,
checkout, cargo restore, the gate, and the post steps -- with no test removed
and no second job added, because the bill is per minute and a parallel job is
billed twice.

## The arithmetic

The runner is four cores and the test phase is subprocess-bound, so its wall
clock is total subprocess work over four: **four seconds of work removed buys
one second of wall.** `scripts/subprocess-summary.sh` prints the total on
every gate run; when mean concurrency equals the core count the run is packed
and only removing work can help. Read the current numbers off the run in front
of you; the noise floor between byte-identical runs is about ±40s, so verify a
change by the step it targets and only then ask whether the total moved, over
several runs.

Where the work is: a JVM booting a Spring context inside a generated project's
`mvn test`. Not Maven's own start, not container starts, not the product
binary, whose median invocation is tens of milliseconds.

## Closed off, with the reason

Do not re-propose these:

- **Scheduling.** Ordering, thread counts, a larger permit budget and a
  cleverer work-stealer are worth nothing on a packed four-core runner.
- **`CARGO_INCREMENTAL=0`** shrinks the cache and makes the gate slower by
  removing cross-run compilation reuse; GitHub bills wall clock, not bytes.
- **Caching workspace `target/` artifacts between commits** puts an upload on
  the critical path of every run for a saving that is only the crates a commit
  did not touch.
- **Pruning superseded `target/debug/deps` by any rule short of
  `cargo-sweep`** costs more recompilation than transfer. Trimming superseded
  incremental sessions is the half that pays and is done.
- **`mvnd`** fails under concurrent invocation (`StaleAddressException`),
  which is the only way this suite runs Maven.
- **`-DforkCount=0`** trades the isolation surefire exists for.
- **AppCDS / class-data sharing** over Maven's JVM or the Spring context: the
  archive cannot match a per-project classpath.
- **Spring lazy initialization** breaks `contextLoads`.
- **JUnit class-level parallelism** manufactures contention: generated tests
  share a database, ports and fixture files.
- **Batching Maven runs.** The cheap runs are plain-Java fixtures that never
  pay a Spring boot, and the expensive ones are doing real work; merging saves
  a floor they never paid or costs the tier's per-test isolation. The merges
  that exist are kept because merging is the stronger check.
- **A generated-project cache across CI runs.** The binary is not
  reproducible under the dev profile, and a byte-exact tree comparison buys
  nothing because what a run costs is the JVM, not javac.

## What is left, if five minutes is still the target

- **A larger runner.** The only lever that reaches 300s; it buys time rather
  than money.
- **One Maven run per group of tests**, at the cost of the tier's isolation.
- **Fewer JVMs inside `jails` itself.** A classpath cache for `jails runner`
  has a clear shape: its invocations resolve the same project's classpath from
  scratch.

## Open items

**P13.7 The suite is `tests/cli` and nothing else.** The other binaries finish
inside it, so only `cli` has a critical path and a budget. Profile it with
`JAILS_TEST_PROFILE=1 -- --nocapture`; the per-subprocess lines go to stderr.
**Exit:** the job fits its budget, or the budget is restated with the
measurement that makes it unreachable. It is currently the second.

**P13.9 A full tmpfs still reports itself as a product bug from one place.**
`jails_support::scratch::reserve` leads with *"failed to create a scratch
directory"*, a sentence about jails, on a storage error. The harness half
names the disk, counts the fixtures holding it and carries a `fix:` line.
Reproduction: fill `/tmp`, run `cargo test -p jails`.

**P13.10 `cached_toolchain_dir_with_salt` takes no lock**, so two gates
running at once race on `target/jails-e2e-cache`: one walks `remove_dir_all`
while the other creates files underneath it, and `.jails-generated-ready` is
written before the directory is filled, so the second process reuses a
half-built toolbox. **Exit:** the fixture takes the same `flock` the Maven
budget uses, and the ready marker is written last.
