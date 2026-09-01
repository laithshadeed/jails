<!--
One of six. `docs/00-contracts.md` is the one every reader starts from; it
carries the contracts, the identifier map and the ownership table that keep
these six from contradicting each other.

**A closed item is deleted from the file that holds it**, in the commit that
closes it -- never marked done. `git log -p -- docs/` is the record.

**Item and section numbers are stable and never reused.** A section with no
open items disappears rather than being renumbered.

Status prose is dated where it is a measurement. Everything else is written in
the present tense as a rule: a note narrating what a module used to be gives a
reader nothing to act on and goes stale on its own.
-->

# 40 — The gates, the suite, and the CI budget

**Read `docs/00-contracts.md` first.** It carries the five contracts, the
deletion map, the identifier map and the ownership table; nothing here repeats
them, and work that contradicts them is wrong however well it reads.

## What you own

`.github/workflows/**`, `.githooks/**`, `mise.toml`, `scripts/**`,
`tests/common/**`, `tests/architecture/**`, `tests/corpus/**`.

You own **how anything is known to be green**, and the wall clock it costs.

## What you do not touch

You do not fix product defects. When a gate you sharpen turns something
red, the fix belongs to whoever owns the path -- hand it over with the
reproduction rather than patching across the boundary.

The one exception is a defect *in the harness*: a fixture that lies, a skip
that reports as a pass, an oracle comparing a binary with itself. Those are
yours, and P13.11 below is the largest of them.

Three things are shared with the other three workstreams and have resolution
rules in `docs/00-contracts.md`: `tests/golden/**`, `tests/architecture/board.rs`
and `LAYERS`. Append to `tests/common/scenarios.rs`; move nothing in it.

## The specification sections this work answers to

§21 conformance test suite. The rest of this workstream answers to the
gates below rather than to the specification.

## How you know you are green

```
mise run verify-rewrite
JAILS_TEST_PROFILE=1 cargo test --test cli -- --nocapture   # the subprocess profile
cargo test --test architecture -- --nocapture --test-threads=1   # the board
```

**Every measurement needs `JAILS_REQUIRE_TOOLCHAIN=1` and a container engine.**
Without them the container-dependent tests fail in milliseconds and the run
measures the suite minus a third of its work -- which is how `-DforkCount=0`
came to look like a 51s win locally while moving CI from 298s to 298s.

---

# The gates

`mise run verify-rewrite` is the single answer to "is this green".
`.githooks/pre-push` and `.github/workflows/verify-rewrite.yml` invoke it and
nothing else, so hook, CI and this file cannot disagree about what passing
means. It runs `cargo fmt --check`, `cargo clippy -D warnings`,
`RUSTDOCFLAGS='-D warnings' cargo doc`, and then every test binary
concurrently under `JAILS_GIT_DIFF_ALGORITHM= JAILS_REQUIRE_TOOLCHAIN=1`.

Both environment pins are load-bearing. Without the second, a tier-3 test that
cannot find its toolchain skips and counts as a pass. Without the first the
gate is not deterministic: `git merge-file` grew `--diff-algorithm` after 2.43,
histogram and myers can resolve an ambiguous three-way merge differently, and
the merged bytes go into the managed tree and the accepted projection -- so a
gate whose merges depend on the distribution underneath it is two answers
wearing one name. The empty value pins git's own default, which every git ever
shipped supports.

## G0–G5 — where each gate stands

- **G0 — mandatory execution and protocol.** Closed. Scenario names and golden
  directories match exactly; every protocol fixture is read by something; every
  advertised failpoint is named outside the registry.

- **G1 — differential CLI.** `tests/product_loop.rs`, 38 scenarios plus the
  corpus. **Read *What "both implementations" currently means* below before
  trusting this one.**

- **G2 — behavior journeys.** Every advertised command path maps to a test.
  Held in two places: `every_advertised_command_path_has_a_journey`
  (`tests/cli/developer_tools.rs`) walks `jails commands --json` and requires
  each path to appear in a test's argv, and
  `every_inventoried_command_path_is_invoked_by_a_test`
  (`tests/architecture/rules.rs`) holds the inventory half. The gate fails in
  both directions: coverage may not fall, and an exemption that is no longer
  needed must come off.

  **What it proves is reachability, not behaviour.** A refusal counts, and the
  match is textual -- a test that merely mentions `"model", "plan"` in an argv
  satisfies it. That is the right bar for "no command is completely untested"
  and the wrong one to read as coverage.

- **G3 — exact real toolchain.** `tests/common/scenarios.rs` is the
  machine-readable kind/capability map, held by
  `every_kind_and_capability_has_a_golden_scenario` against the binary's own
  help. `format` is the one documented exemption, listed in `COVERED_ELSEWHERE`
  with the test that does cover it, and that test's existence is asserted.

- **G4 — crash/recovery.** Closed. `failpoints!` is one declaration: it emits
  `POINTS`, the list a crash test enumerates, and one constant per point, which
  is the only thing `trip` accepts. A point nobody trips is an unused constant
  and `-D dead-code` fails the build; a point tripped but unadvertised cannot
  be written. `crates/jails-workspace/tests/crash.rs` declares nine points over
  the canonical publication sequence and asserts convergence rather than
  roll-forward, because there is no journal.

  **The aborting half earned its cost immediately.** The in-process matrix was
  green and the child-abort matrix was not: an injected `Err` unwinds, so the
  staged temporary's guard removes it, and a crash between staging and rename
  looked survivable. An `abort()` leaves the temporary on disk, where
  `verify_preconditions` reads it as an unmanaged file inside the managed tree
  and refuses -- permanently. `execute::sweep_staged` is the fix, and the prefix
  is `.jails-staged-` rather than `tempfile`'s `.tmp` so the only thing in a
  project that looks like a reader's file and is not says whose it is.

- **G5 — real-project corpus.** `tests/corpus/` holds five checked-in project
  trees jails did not write, with `policy.tsv` accounting for every one. The
  corpus is bytes rather than a Rust table on purpose: it grows by dropping a
  directory in, and a corpus only a Rust programmer can extend is not one. It
  found a real defect on its second entry -- `jails adopt` read only the first
  package segment, so a class in `infra/jdbc` was adopted as
  `adapters = "infra"`, a directory holding no Java at all.

  One finding is a refusal rather than a defect: `core` is not a synonym for
  `domain` and should not become one. It means the domain model in one codebase
  and shared framework glue in the next, so it fails the synonym table's bar on
  *unambiguous* rather than on *common*. `spring-renamed-layers` pins the
  refusal so nobody adds it on a guess.

## What "both implementations" currently means

**Only one test compares two binaries, and only under the canary.**
`subjects_with_fixture` and `adopted_subjects` each return a single canonical
subject; the legacy half was deleted rather than each case rewritten, and the
array shape was kept so a case that stops holding still says which subject it
was about. `every_corpus_project_is_treated_the_same` is the one that takes a
second binary, from `JAILS_LEGACY_BIN`.

Nothing sets that variable except `scripts/verify-rewrite-g1-canary.sh`, and
**CI does not run the canary** -- the workflow runs `verify-rewrite` and
nothing else. So on every run that actually happens, G1's differential half is
a canonical-only regression suite.

Two things now stop that from being invisible. The corpus test's legacy subject
is *absent* rather than a second copy of the binary under test, and a
`JAILS_LEGACY_BIN` equal to that binary is refused -- previously it fell back
to `CARGO_BIN_EXE_jails`, so an ordinary run compared the binary with itself
and every assertion passed meaning nothing. And
`every_test_target_a_script_names_exists` fails when a script names a cargo
test target that does not exist, which is how the canary came to be running
`--test differential` months after that harness was renamed.

**Restoring a real legacy subject across the 38 scenarios is open work**, and
it is not mechanical: the legacy binary predates JDL v1, seeds a ledger rather
than `.jails/model.jdl`, and writes its Java to `src/main/java` rather than
below the managed root, so a subject needs its own seed and its own record
path. Until that is done, the honest description of G1 is "the canonical
implementation does not regress", and the cutover claim it was written to
support -- that the replacement behaves like the thing it replaces -- rests on
the corpus test under the canary alone.


---

# The CI budget

**The whole `verify` job must finish inside five minutes** -- set-up, checkout,
cargo restore, the gate, and the post steps -- with no test removed and no
second job added, because the bill is per minute and a parallel job is billed
twice.

## The runner is four cores, and that sets everything else

`run-tests` prints its width, and on CI it says **`30 binaries, 4 at a time`**.
Every number below follows from that: the test phase is subprocess-bound, so
its wall clock is total subprocess work over four, and **four seconds of work
removed buys one second of wall**.

## Where the five minutes currently go

Two runs, because the two shapes cost very different amounts. `33322968191`
changed no Rust at all against a warm cache, so its compile is zero and the
save is skipped -- that is the floor. `33506161538` changed sources, which is
the ordinary commit:

| step | docs-only | code |
|---|---|---|
| set up, checkout, rustup cache and install | 11 | 12 |
| **restore cargo** | **42** | **78** |
| restore `~/.m2` and `~/.gradle` | 4 | 6 |
| mise, JDK 21, toolchain banner | 14 | 17 |
| **`mise run verify-rewrite`** | **217** | **382** |
| trim | — | 1 |
| **save cargo** | **21** | **53** |
| post steps | 2 | 1 |
| **job** | **314** | **550** |

**Quote the second column.** A gate exists for commits that change code, and
the first column is what it costs to change a comment.

Inside that 382s gate, `run-tests` reported **325.2s of test phase carrying
1102.7s of subprocess work at mean concurrency 3.39**, with 41.1s spent
queueing for a permit. So compilation, `fmt`, `clippy` and `doc` together are
about 57s, and everything else is the suite.

## Whether five minutes is reachable, and the arithmetic that answers it

A perfect four-core packing of 1102.7s is **276s**, and the observed 325.2s is
85% of that -- so scheduling is worth at most ~49s and there is no queue worth
draining. Add the 35s of set-up and toolchain steps that no change removes and
the floor is **311s before a single byte is compiled or transferred**.

**So the job cannot reach 300s at the current amount of work, whatever is done
to the cache or the schedule.** Getting there means removing roughly 500s of
subprocess work -- 46% of it -- with no test removed. The levers, priced
honestly at 4:1:

| lever | work removed | wall |
|---|---:|---:|
| collapse 36 Maven runs toward 10 (the per-run floor, ~24 times) | ~170s | ~42s |
| keep the container images off the critical path | ~70s | ~18s |
| everything scheduling can still give | — | ~49s |
| **together** | | **~109s** |

That is a **~440s job**, not a 300s one. The honest options past it are a
structural change to the real-toolchain tier -- one Maven run per group of
tests rather than per test, which trades that tier's isolation -- or a larger
runner, which halves the wall and doubles the per-minute rate, so it buys time
rather than money.

**Do not read the earlier "perfect packing, concurrency 4.00, ordering is
worth zero" measurement as still current.** It was true of run
`33413442610`; `33506161538` measured 3.39 with 41.1s queued over the same
total work, so the suite's packing moves with its shape and has to be re-read
from the run in front of you.

## The permit cap is not the constraint, measured three ways

`JAILS_TEST_MAX_TOOLCHAIN_PROCESSES` at 8, 12 and 16 over the whole suite on
a sixteen-core developer box: **114.2s, 112.0s and 113.9s**, with queueing
falling 40.1s -> 0.0s -> 0.0s and mean concurrency reaching **7.08 and staying
there**. Zero queueing and a flat wall means the suite does not *have* more
than about seven things to run at once, so raising the cap cannot help and
lowering it below 8 is the only setting that could hurt.

**One run proves nothing about this job.** Two runs that compiled *nothing* --
both documentation-only commits against a warm cache -- measured the gate at
217s and 261s, restore at 42s and 52s, and the job at 316s and 343s. The noise
floor is about ±40s, or 13%, for byte-identical work. Verify a change by the
*step it targets* (`Save cargo` going 21s -> `skipped` is unambiguous) and only
then ask whether the total moved, over several runs.

## What is already closed off, with the number that closed it

Do not re-propose these. Each was measured and each cost an afternoon:

- **The runner is at a perfect four-core packing.** `run-tests` reported
  1106.2s of subprocess work in a 276.4s wall at mean concurrency **4.00**,
  with 33.6s of permit waiting across 217 subprocesses. Ordering, thread
  counts, a larger permit budget and a cleverer work-stealer are worth
  **zero**. What follows is arithmetic: **four seconds of subprocess work
  removed buys one second of wall.**
- **`CARGO_INCREMENTAL=0`** shrinks the cache from 3.7 GB to 978 MB and the
  save from 45s to 14s, and made the gate 90-120s *slower* by removing the
  cross-run compilation reuse. Reverted. GitHub bills wall clock, not work.
- **Caching workspace `target/` artifacts between commits**: the entry is 1-2 GB
  compressed and would have to be written every run, putting 30-60s of upload
  on the critical path against a saving that is only the crates a commit did
  not touch.
- **Pruning superseded artifacts out of the cargo cache**: 87% of
  `target/debug/deps` is superseded, and every keep-rule measured costs more
  recompilation (24s, 126s, 150s) than the transfer it saves. Trimming
  superseded *incremental sessions* is the half that pays and is already done:
  ~24s a run once settled.
- **`mvnd`**: 2.45s a run faster and **three failures out of four** when four
  builds start concurrently, which is the only way this suite runs Maven.
- **`-DforkCount=0`**: 0.55s a run, and it trades away the isolation surefire
  exists to provide.
- **lld**: already the default linker since Rust 1.90. Passing it again changes
  nothing.
- **The generated-project cache cannot survive a CI run.** The stamp is not the
  obstacle: the binary is not reproducible (dev-profile codegen units are not
  deterministically ordered), and a byte-exact tree comparison was implemented,
  measured at 147s/149s without reuse against 150s/147s with it, and deleted.

## The one lever with the size to matter

**Fewer Maven runs.** Maven is 693.1s of the 1106.2s across 36 runs, averaging
19.3s, and 267s of that is spent before any test does anything:

| | s | share |
|---|---|---|
| Maven start (`validate`) | 1.54 | 24% |
| javac, main and test | 1.45 | 22% |
| surefire fork | ~1.1 | 17% |
| Spring context boot | 2.54 | 38% |
| **total floor, per run** | **6.52** | |

**And the second test class in a run is free**: the same project built with
one, two, four and eight `@SpringBootTest` classes, one Maven invocation each,
measured 6.56s, 6.48s, 6.44s and 6.49s. Spring caches a context per
configuration inside the JVM, so once the floor is paid the rest costs nothing
measurable. That is the entire case for batching.

Collapsing 36 runs toward a dozen saves the floor about twenty-four times --
roughly **156s of work, which is 39s of wall**. Against a 314s job that is the
difference between missing five minutes and making it, and it is the only
candidate of that size. `cached_toolchain_dir_with_salt` is the pattern
already: `spring-core-toolbox`, `spring-services-toolbox`, `spring-db-toolbox`
and `proof-apps` are expensive because they do real work rather than paying the
floor over and over.

**Two warnings, both earned.** Merging nine capability packs into two shared
projects measured **471.8s -> 478.0s warm-to-warm: nothing**, because that
experiment was run on a saturated four-core developer box with no idle to fill;
the merges are worth keeping because merging is the *stronger* check -- it is
what caught `mail` and `actuator` contradicting each other -- not because they
were faster. And a partly cold run reports 730.2s against a warm 471.8s, so
**compare warm against warm or the number will tell you whatever you hoped**.

**Exit:** three consecutive `verify` runs under 300s, each with the same test
count as the run before the change, and the per-step breakdown recorded here.

## Open items

**P13.11 G1 has no legacy subject.** *What "both implementations"
currently means* above measures this. The 38 product-loop
scenarios each run one canonical subject; only the corpus test takes a second
binary, and only under `scripts/verify-rewrite-g1-canary.sh`, which CI does not
run. Restoring a real legacy subject is not mechanical: the frozen binary
predates JDL v1, seeds a ledger rather than `.jails/model.jdl`, and writes Java
to `src/main/java` rather than below the managed root, so `Subject` needs a
per-subject seed and record path.

**Exit:** the canary runs in CI on a schedule, or the differential claim is
withdrawn from this file and the harness renamed to what it is.


**P13.7 The suite is ~110s of `tests/cli` because it compiles 36 Java
projects**, and the remaining lever is Maven's JVM startup. Profile with
`JAILS_TEST_PROFILE=1` (it needs `-- --nocapture`).


**P13.9 A full tmpfs reports itself as a product bug**, and the one-hour
fixture sweep does not bound a burst. Two `new-cli` unit tests failed with
`PoisonError` and *"failed to create a scratch directory"*, which reads as a
jails defect and is a disk.


**P13.10 The Maven budget is shared across processes and the fixture it
protects is not.** `cached_toolchain_dir_with_salt` shares one persistent
fixture per label under `target/jails-e2e-cache` and takes no lock, so two gate
runs at once corrupt each other and produce `capabilities::` failures that read
exactly like capability regressions. **Run one gate at a time** until this is
fixed.

---

## The environment a measurement was taken in

Tier-3 tests shell out to Maven and compile against `pom::TARGET_RELEASE`
(currently 26). A machine whose JDK is older fails every one of them with
`release version 26 not supported`. Those failures are the machine, not the
tree -- but note the trap they sit next to: **a skipped tier-3 test is reported
as passing**, which is why every skip goes through `common::skip()` and
`JAILS_REQUIRE_TOOLCHAIN=1` turns each into a failure naming what was missing.
The gate sets it.

