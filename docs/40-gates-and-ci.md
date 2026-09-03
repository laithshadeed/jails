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

**The whole `verify` job must finish inside ten minutes** -- set-up,
checkout, cargo restore, the gate, and the post steps -- with no test removed
and no second job added, because the bill is per minute and a parallel job is
billed twice.

Five was the number until 2026-09-02, and this is the measurement that
retired it: that day's run did 1003 s of subprocess work at mean concurrency
3.47 on four cores, which is a packed runner, so the test phase alone is
250 s at perfect packing; the compile from a warm cache took another 190 s;
the job took 9 m 14 s end to end. Five minutes on this runner would need
the work halved, and the levers that remain are listed at the end of this
file. The ten-minute figure is a ceiling, not a target: a change that adds a
minute of JVM work still has to say why.

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

## The developer machine, measured 2026-09-02

The same suite on 16 cores and 30 GB, through `scripts/bounded.sh` (12-core
quota, 15 GB cap, no swap): **53 s for the whole gate warm** -- `fmt`,
clippy, rustdoc, the test build, and every test including the real-toolchain
tier -- and about 150 s cold, when no proof has been recorded yet or every
generated byte changed. Before the day's changes the same tree took 205 s
unbounded and, run beside four `cargo` builds, took the machine into swap.
What moved it, in order of size:

- **Proofs are memoised on the bytes they prove.** A real-toolchain test
  proves that one exact generated tree compiles and passes under one
  toolchain; the same tree proved again is the same proof. `tests/support/mvn.rs`
  is `mvn` with a cache in front (the section below): warm, the 47 Maven runs
  of a full suite cost 8 s instead of 600 s.
- **The critical path was one test.** The shared plain toolbox ran sixteen
  `jails` commands in sequence and each ran `mvn spotless:apply` -- the
  `format` capability's follow-up effect -- so the test spent 162 of 165 s in
  JVMs the product started. The product now formats once per manifest replay
  and never after a no-op execution, and the toolbox is one manifest: 19 s.
- **Five Spring boots in one test.** The runner lifecycle test started and
  stopped five contexts in turn, 57 s, and none depended on another. They
  are five tests over one compiled fixture, each booting its own copy: 15 s.
- **Twenty-two test binaries ran one after another.** `cargo test` runs them
  in sequence; `scripts/gate-test.sh` starts them at once, since the budgets
  they share are `flock`s under `target/`, and the queue of small binaries
  that used to follow `tests/cli` is gone.
- **Libtest's alphabetical order.** With one thread per core the long tests
  under `model::` started at 115 s and set the tail; `RUST_TEST_THREADS` at
  twice the quota starts every one of them early and lets the permit pool,
  not the alphabet, decide.
- **The pools size themselves from the cgroup**, not the machine, so the
  kernel's answer and the harness's cannot disagree.
- **Three compilations that shared nothing ran one after another.** Clippy,
  rustdoc and the test build each compile the workspace into their own
  artifacts, and one `target/` holds one lock; `scripts/gate-build.sh` runs
  them concurrently in three target directories.

What is left warm is what no cache can replay: the three proof apps'
`docker build` and OCI checks (~15 s each, in parallel), `javac` and the
Spring contexts `jails run`, `runner` and `testd` start themselves (~10 s a
boot), and the compile phase (~15 s incremental). Cold, it is the JVM work
the cache is recording, and the levers below still apply to it.

## The proof cache

`tests/support/mvn.rs` is built as the cargo example named `mvn`, so a
`--debug` line the product prints still reads `.../mvn compile`. The harness
runs it in place of `mvn` and hands it to the product through `JAILS_MAVEN`,
so a JVM the product starts is memoised the same way as one the harness
starts. What it keys:

- the project tree, minus `target/`, `build/`, the VCS and IDE directories,
  and everything under `.jails/`, because Maven reads none of that and the
  model spells the scratch directory's name;
- the argv, with the project directory and any loopback port blanked --
  `-Dmdep.outputFile=<project>/target/…` and
  `jdbc:postgresql://127.0.0.1:<port>/…` are the same run in another
  directory against the same service on another port;
- the environment Maven, the JVM, Spring and Testcontainers read
  (`MAVEN_*`, `JAVA_*`, `JDK_*`, `SPRING_*`, `TESTCONTAINERS_*`, `DOCKER_*`),
  the identity of the `mvn` and `java` that would run (resolved path, size,
  modification time), and `JAILS_PROOF_CACHE_KEY`, which the harness sets to
  the images of the suite-scoped PostgreSQL and Kafka a proof app is run
  against.

What it records, for a successful run only: the exit status, stdout and
stderr, every file the run changed outside `target/` (Spotless formatting
sources), any output file the argv named by absolute path, and `target/`
whole, with the project directory replaced by a placeholder in every text
file so a surefire report replays with the right `user.dir`. A replay writes
all of it back, dated now, so `jails test --fast` over the replayed classes
sees a build newer than its sources.

**A hit can never turn a failing proof green**: only green runs are
recorded, for byte-identical inputs. Every proof is still proven for exactly
the bytes the test asserts; what is skipped is proving the same bytes twice.
A change that alters a project's generated bytes reruns that project's
proof and no other. Entries live under `target/jails-proof-cache/entries`,
`misses.log` says which key missed and why one can be diffed against
another's `key.txt`, entries unused for two weeks are swept, and
`JAILS_PROOF_CACHE_OFF=1` runs everything for real. CI restores the newest
cache and saves its own under the commit's key.

The one place a replay is a weaker statement than a run is a proof against
a live service the harness started -- the three proof apps. The key names
the images, not the containers, so a replay stands for "these bytes passed
against these images", which is what the cold run proved too.

## What five minutes would take

- **A larger runner.** The only lever that reaches 300s; it buys time rather
  than money.
- **One Maven run per group of tests**, at the cost of the tier's isolation.
- **Fewer JVMs inside `jails` itself.** A classpath cache for `jails runner`
  has a clear shape: its invocations resolve the same project's classpath from
  scratch.

## Open items

None. The last three (P13.7, P13.9, P13.10) closed on 2026-09-02: the
budget above is the restated one, `jails_support::scratch` names the disk
on a storage error, and the toolchain fixture takes a `flock` beside its
tree with the ready marker written last.
