# Test-suite performance investigation

## Goal

Keep running the complete test suite and preserve its assertions while removing
duplicated tool startup, accidental service dependencies, and avoidable CPU and
memory contention.

This document records the August 2026 profiling session. The baseline was a
warm working tree with Maven dependencies already present locally.

## Reproducing the measurements

### Revisions and machine

The original baseline is commit
`1bf7c42612e636b65064c7c2dd43a6975aab33f9`; the completed implementation is
`de75b76c30f22ec978b55dab7d598ebdec549ce4`. Use a separate Git worktree when
comparing them so checkout does not disturb local changes:

```sh
git worktree add /tmp/jails-baseline 1bf7c42612e636b65064c7c2dd43a6975aab33f9
git worktree add /tmp/jails-optimized de75b76c30f22ec978b55dab7d598ebdec549ce4
```

The measurements came from this host:

- Linux 7.1.3-201.fc44.x86_64;
- AMD Ryzen 7 PRO 6850U, 8 cores / 16 hardware threads;
- 30 GiB RAM and 8 GiB zram swap;
- Rust 1.97.1 and Cargo 1.97.1;
- Maven 3.9.16 and OpenJDK 26.0.2;
- Podman 5.8.4 through its Docker-compatible CLI.

Record the comparison host before each run. These commands need no root
access:

```sh
date -Is
git rev-parse HEAD
uname -srmo
lscpu | grep -E 'Model name|^CPU\(s\):|Thread|Core|Socket'
free -h
uptime
rustc --version
cargo --version
mvn --version
podman --version
```

Do not compare a quiet run with one performed while another Cargo, Maven, Java,
Podman, or image-build job is active. Record `uptime`, `free -h`, and the first
five `vmstat` samples even if the machine appears idle. In particular, a full
swap allocation is not proof of active swapping; the `si` and `so` columns in
`vmstat` determine whether pages are moving during the test.

### Warm-up and cache definitions

All reported baseline numbers are dependency-warm: the Cargo registry, Maven
local repository, container images, and Rust build artifacts were already
present. They are not first-clone or first-download measurements. Do not delete
`~/.cargo`, `~/.m2`, or the container image store before reproducing them.

The optimized checkout also has a generated-project cache at
`target/jails-e2e-cache`. "Cold cache" means that directory is absent at the
start of a run. "Warm cache" means one identical test run has completed at the
same commit, leaving generated sources and Maven `target` directories there.
Tests and Maven are still executed on a warm run; only unchanged generation
and compilation products are reused.

Prime a warm optimized measurement once, then measure the second invocation:

```sh
env -u JAVA_TOOL_OPTIONS -u MAVEN_OPTS cargo test --test cli
env -u JAVA_TOOL_OPTIONS -u MAVEN_OPTS cargo test --test cli
```

To obtain a cold-cache sample without deleting an existing cache, move it to a
unique temporary path, run the test once, then move the new cache aside and
restore the original. Do not use the same temporary name if it already exists:

```sh
mv target/jails-e2e-cache /tmp/jails-e2e-cache.saved
env -u JAVA_TOOL_OPTIONS -u MAVEN_OPTS cargo test --test cli
mv target/jails-e2e-cache /tmp/jails-e2e-cache.cold-result
mv /tmp/jails-e2e-cache.saved target/jails-e2e-cache
```

If the cache does not exist yet, omit the first and last `mv`. Changing or
rebuilding the `jails` executable invalidates the generated-project cache, so
always perform the warm-up after switching revisions.

### Timing commands

GNU `time` supplied wall time, aggregate user/system CPU, context switches,
page faults, and maximum RSS. Redirect test output so terminal rendering is
not part of the result. Run from the selected worktree root:

```sh
/usr/bin/time -v -o /tmp/jails-full.time \
  env -u JAVA_TOOL_OPTIONS -u MAVEN_OPTS cargo test \
  > /tmp/jails-full.log 2>&1

/usr/bin/time -v -o /tmp/jails-cli.time \
  env -u JAVA_TOOL_OPTIONS -u MAVEN_OPTS cargo test --test cli \
  > /tmp/jails-cli.log 2>&1

/usr/bin/time -v -o /tmp/jails-app.time \
  env -u JAVA_TOOL_OPTIONS -u MAVEN_OPTS cargo test --test cli \
  app_manifests_pass_the_full_generated_verification_gate -- --exact \
  > /tmp/jails-app.log 2>&1
```

The 159.36-second baseline is `/tmp/jails-full.time`; the 156.89-second CLI
row is the duration reported by libtest in `/tmp/jails-full.log`. The 38.54
and 27.69-second optimized results are GNU `time` elapsed wall values from the
second, cache-warm CLI and application commands respectively. Extract the
comparable fields with:

```sh
grep -E 'Elapsed|User time|System time|Maximum resident|context switches|Major' \
  /tmp/jails-full.time /tmp/jails-cli.time /tmp/jails-app.time
grep -E 'test result:|Finished' /tmp/jails-full.log /tmp/jails-cli.log \
  /tmp/jails-app.log
```

Reproduce the isolated rows in the representative-profile table with the same
GNU `time` wrapper and one exact libtest filter at a time:

```sh
for test in \
  add_kafka_and_generate_event_compile_against_real_spring \
  add_json_on_a_spring_project_defers_to_the_parents_version_and_compiles \
  add_observability_serves_a_prometheus_scrape \
  add_redis_wires_a_ttl_enforcing_store_and_a_compose_service \
  add_db_installs_postgres_flyway_and_testcontainers_without_an_orm \
  ledger_cli_manifest_builds_without_spring
do
  /usr/bin/time -v -o "/tmp/${test}.time" \
    env -u JAVA_TOOL_OPTIONS -u MAVEN_OPTS cargo test --test cli \
    "$test" -- --exact > "/tmp/${test}.log" 2>&1
done
```

The first four names correspond to the Kafka, JSON, observability, and Redis
rows; the fifth is the database regression and the sixth initializes the
shared plain toolbox. On the baseline revision the database command is expected
to fail after doing its measured work, which is why that table records both
17.25 seconds and `failed`. The failure itself is part of the diagnosis, not a
passing benchmark result.

To recover libtest's apparent in-suite durations, add `--report-time` to the
full CLI run:

```sh
env -u JAVA_TOOL_OPTIONS -u MAVEN_OPTS cargo test --test cli -- --report-time \
  > /tmp/jails-cli-per-test.log 2>&1
```

The historical 147.94- and 83.29-second rows were working-tree checkpoints
during this investigation, not commits. Their implementation boundaries are
described below, but they cannot be reconstructed byte-for-byte from Git and
should be treated as diagnostic waypoints. The two committed endpoints above
are the reproducible before/after comparison.

Wall time varies with host load, thermal state, filesystem cache, and container
storage. The values in this document are single observed trials, not statistical
estimates. For a new benchmark, run three measured warm trials, retain all
three logs, and report minimum/median/maximum; use the minimum only when
comparing with the "best complete warm result" below.

### CPU, scheduling, memory, and I/O profiles

Run the samplers in separate terminals while executing one of the timing
commands above:

```sh
vmstat -w 1 > /tmp/jails-vmstat.log
iostat -xz 1 > /tmp/jails-iostat.log
pidstat -durwt 1 > /tmp/jails-pidstat.log
```

Stop each sampler with Ctrl-C immediately after the test. `vmstat` supplies
run-queue, swap-in/out, CPU wait, and context-switch samples. `iostat` supplies
device utilisation and latency. `pidstat` identifies the Maven, Java, Rust,
and container processes responsible for CPU, I/O, memory pressure, and
scheduler waits.

For the hardware-counter comparison, run the same command under `perf stat`.
For example:

```sh
perf stat -d -o /tmp/jails-app.perf -- \
  env -u JAVA_TOOL_OPTIONS -u MAVEN_OPTS cargo test --test cli \
  app_manifests_pass_the_full_generated_verification_gate -- --exact
```

Use `perf stat -d` for every side of a comparison; a normal timed run and a
`perf` run are not directly comparable because counter collection adds some
overhead. The task-clock, context-switch, CPU-migration, cycle, instruction,
IPC, and cache-miss fields support the CPU/scheduling conclusions below. The
`vmstat` `si`/`so` fields support the swap conclusion, and `iostat` `%util` and
`await` support the disk-I/O conclusion.

The controlled four-test contention profile used one libtest process and four
test threads, so the harness's process permit was shared exactly as it is in a
complete run. The following Bash snippet selects only those four tests for
profiling. Its skip arguments are measurement instrumentation only; no result
from this subset is used as proof that the complete suite passes.

```bash
keep='^(add_kafka_and_generate_event_compile_against_real_spring|add_json_on_a_spring_project_defers_to_the_parents_version_and_compiles|add_observability_serves_a_prometheus_scrape|add_redis_wires_a_ttl_enforcing_store_and_a_compose_service)$'
mapfile -t all_tests < <(
  cargo test --test cli -- --list | sed -n 's/: test$//p'
)
args=(--test-threads=4 --report-time)
for test in "${all_tests[@]}"; do
  if [[ ! "$test" =~ $keep ]]; then
    args+=(--skip "$test")
  fi
done
perf stat -d -o /tmp/jails-four-way.perf -- \
  env -u JAVA_TOOL_OPTIONS -u MAVEN_OPTS cargo test --test cli -- \
  "${args[@]}"
```

Finally, verify that the measured run did not silently omit tests. The logs
must end in passing Cargo/libtest summaries, and the generated Surefire XML is
also checked by the Rust harness for exact test totals with zero failures,
errors, and skipped tests. Search the benchmark command and diff for selectors
or skip mechanisms:

```sh
grep -E 'test result:' /tmp/jails-full.log
git diff 1bf7c42612e636b65064c7c2dd43a6975aab33f9..HEAD -- \
  | grep -E '@Disabled|#\[ignore\]|DskipTests|Dtest='
```

The second command should print nothing newly added by this optimization.

## Baseline

| Work | Wall time |
| --- | ---: |
| `cargo test` | 159.36 s |
| `cargo test --no-run` | 0.08 s |
| 401 Rust unit tests | 0.03 s |
| 160 tests in `tests/cli.rs` | 156.89 s |
| Other test binaries | about 2.3 s |

Rust compilation and the Rust unit tests are not the bottleneck. Almost all
elapsed time belongs to CLI integration tests which create Java projects and
launch Maven, javac, Surefire/Failsafe, Spring, and sometimes Testcontainers.

The warm full run consumed 1,493 CPU-seconds of user time and 157 CPU-seconds
of system time. It averaged more than ten cores for 159 seconds.

## Representative profiles

The apparent time is the duration reported while the tests ran as part of the
contended suite. The isolated time is the same test run alone through the
compiled integration-test binary.

| Test | Apparent suite time | Isolated time | Isolated maximum RSS | Main intrinsic work |
| --- | ---: | ---: | ---: | --- |
| Kafka | 65.89 s | 6.84 s | 537 MiB | Maven/JVM and Spring startup |
| JSON | 32.51 s | 6.12 s | 383 MiB | Maven/JVM and Spring startup |
| Observability | 30.13 s | 7.76 s | 471 MiB | two Spring contexts |
| Redis | 28.37 s | 6.74 s | 390 MiB | Maven/JVM and Spring startup |
| Database | 44.94 s | 17.25 s, failed | 521 MiB | PostgreSQL container startup |
| Shared plain toolbox | shared by callers | 11.43 s | 806 MiB | generation plus one Maven gate |

The non-database Maven lifecycles themselves took about 4.7--6.7 seconds.
Spring context creation accounted for roughly 1--2 seconds within those
lifecycles.

## Where the time goes

### CPU and scheduling are the general bottleneck

The test harness permits at most four external toolchain process trees. This is
necessary because a Maven process starts javac and test JVMs, and those JVMs
create their own thread pools. Libtest can have many tests waiting for one of
the four permits, and its per-test timer includes that wait.

A controlled four-test run (Kafka, JSON, observability, and Redis) completed in
14.47 seconds. Each test took 12.6--14.5 seconds rather than its isolated
6.1--7.8 seconds. During the four-way run:

- task-clock time represented about 11.6 busy CPUs;
- involuntary context switches rose to 92,464;
- instructions per cycle fell from about 1.1 in isolation to 0.7;
- L1 data-cache misses increased;
- disk utilisation stayed low.

This is CPU, cache, and scheduler contention. For example, Kafka's reported
65.89 seconds is about seven seconds of intrinsic work plus permit queueing and
competition with other JVM process trees.

The limit of four is already an intentional mitigation. Earlier unbounded runs
allowed roughly sixteen Maven/JVM trees and turned seven-second builds into
40--75 second builds. It should be benchmarked after larger changes rather than
changed by intuition.

### Repeated process and framework startup

The full suite launched at least 28 Java compilation phases and 23 Spring
application contexts. Maven daemon use is deliberately disabled for real
generated-project tests because concurrent mvnd runs have previously been
intermittent. Consequently, every Maven call pays for a fresh Maven JVM, model
construction, plugin discovery, compilation setup, and a test JVM.

Several tests already deduplicate work with `OnceLock`. This makes the suite
faster, but makes libtest durations misleading: every caller waiting for the
same initialization can be reported as slow even though only one caller did
the work.

### Memory and swap

An isolated Java integration test used roughly 380--537 MiB; the larger plain
toolbox reached 806 MiB. GNU `time` reports the largest process, not the sum of
all concurrently live Maven, JVM, Rust, and container processes. Four process
trees can therefore consume several GiB even when no single process looks
extreme.

The controlled profiles recorded no process swaps and almost no major page
faults. Memory was not the isolated critical path, but concurrent JVM and
container churn can evict the machine's cold pages into zram. A full zram
device does not by itself mean active thrashing: Linux normally leaves cold
pages there while RAM is available and swap-in/swap-out activity is quiet.

### Disk I/O

Ordinary generated-project tests left NVMe mostly idle. PostgreSQL container
startup was the exception: it drove the device to roughly 93--96% utilisation
for several seconds, with elevated write latency. Container storage and
service readiness are real costs for the database and full application gates.

### Network and services

No measured Maven run downloaded an artifact, so the external network was not
on the critical path.

Local service configuration was a problem. Generated production properties
point PostgreSQL and Kafka at Compose's fixed `localhost:5432` and
`localhost:9092`. Database tests import a test configuration whose
`@ServiceConnection` supplies a dynamically mapped PostgreSQL endpoint.
Messaging integration tests similarly create a Kafka container.

However, every unrelated full Spring context also discovered the generated
Kafka listeners and started consumers against `localhost:9092`. The full app
gate emitted continuous reconnect warnings. With a developer broker on that
port the dependency is masked; without it, contexts perform useless local
network work and shutdown coordination.

An isolated database regression also exposed an ordering bug in the test: it
created an additional `@SpringBootTest` after running `jails add db`, too late
for the capability's documented import reconciliation. It then depended on a
host PostgreSQL at port 5432. The application-manifest path already performs a
second capability reconciliation after generation; the regression fixture
must exercise that same documented ordering instead of depending on external
state.

## Implementation plan

The rule is to deduplicate infrastructure work, not assertions or generated
tests.

### Phase 1: deterministic, quiet service wiring

- Disable Kafka listener auto-startup for the Rust harness's ordinary Maven
  verification contexts.
- Explicitly enable listener startup inside the real generated messaging IT,
  where `@ServiceConnection` supplies a Kafka container.
- Correct the database regression fixture so every `@SpringBootTest` exists
  when `add db` performs its import reconciliation.
- Keep all real messaging and database integration tests enabled.

This removes localhost reconnect loops without pretending a real broker test
passed: the messaging IT is still run by Failsafe and explicitly opts in.

### Phase 2: reduce Maven/JVM launches

- Batch compatible generated projects through a temporary Maven reactor, or
  introduce a reliable single-flight Maven-daemon runner.
- Keep focused Rust assertions per capability, but route identical generated
  trees to one content-addressed verification result within the same
  `cargo test` process.
- Put compatible Spring tests in the same Maven/JVM invocation so Spring's
  context cache can work.

The largest expected speedup comes from reducing the number of 5--7 second
Maven lifecycles, not from making Rust tests less parallel.

### Phase 3: share container lifecycles safely

- Start one PostgreSQL and one Kafka service per suite-level verification
  process.
- Isolate PostgreSQL tests with a unique database/schema and isolate Kafka
  tests with unique topics and consumer groups.
- Retain dedicated tests for the generated Testcontainers configuration, so
  sharing the harness service does not remove coverage of generated wiring.
- Do not enable Testcontainers cross-run reuse globally: its reuse key does
  not identify the project, retained state can leak between runs, and Ryuk
  deliberately does not reap reusable containers.

### Phase 4: benchmark runtime policy

- Benchmark toolchain limits 2, 3, and 4 after the number of Maven launches is
  reduced.
- Benchmark short-lived JVM settings such as `-XX:+UseSerialGC` and
  `-XX:TieredStopAtLevel=1`; keep them only if the complete suite improves.
- Record permit queue time separately from subprocess execution time so future
  reports distinguish a slow operation from a long wait.

## Success criteria

- `cargo test` still runs every Rust, generated JUnit, Surefire, Failsafe, and
  container integration test that it runs today.
- No test requires developer services on ports 5432, 6379, or 9092.
- No unrelated Spring context repeatedly attempts to connect to Kafka.
- The full suite passes from a clean container state.
- Total wall time, peak aggregate memory, involuntary context switches, and
  container starts are recorded for each implementation phase.
- A reasonable first target is 40--70 seconds on the profiled machine; it is a
  target to validate, not a promised result.

## First implementation result

Phase 1 now uses an explicit `KafkaTestcontainersConfig` generated by the
Kafka capability. Messaging ITs and transactional-outbox ITs import it; its
static broker survives across their Spring contexts inside one Failsafe JVM
and Ryuk still cleans it up at JVM exit. Ordinary Spring contexts run with
Kafka listener auto-startup disabled by the Rust harness, while the real
messaging IT explicitly enables it again.

The isolated database regression was corrected to create its cross-package
`@SpringBootTest` before `add db` performs its documented reconciliation. It
now passes with two Spring contexts against the dynamically mapped PostgreSQL
container and no host database.

Measured results from a clean container state:

| Verification | Before | After |
| --- | ---: | ---: |
| Database regression | failed after 17.25 s | passed in 17.22 s |
| Three complete generated-app Maven verifies | failed/stopped after 137--142 s | passed in 72.08 s |
| Complete `tests/cli.rs` integration binary | 156.89 s | 147.94 s, 160/160 passed |

The failed generated-app run identified the hidden dependency precisely: the
messaging IT's consumer used its Kafka container, but transactional-outbox
producers in separate Spring contexts still used `localhost:9092` and waited
for Kafka's 60-second metadata timeout. Importing the shared, dynamic broker
configuration into both test families removed that timeout without disabling
either family.

The first complete-suite verification then exposed one more contention leak.
Maven, javac, Surefire, Failsafe, and Testcontainers already shared a
four-process budget, but the application-image test bypassed it and launched
three additional Docker builds. Under full-suite load one `support-inbox`
build failed; the identical test passed alone in 84.03 seconds. Docker pulls
and builds now use the same process gate. This preserves all three image builds
and their non-root-user assertions while preventing seven heavyweight process
trees from competing at once. A subsequent complete `cargo test` passed, with
the CLI integration binary completing in 147.94 seconds.

## Completed implementation and current limit

The remaining planned work was implemented without deleting, disabling,
ignoring, or selecting out tests:

- Short-lived Maven JVMs use Serial GC and stop tiered compilation at level 1.
  This reduced a representative small Maven verification from 9.89 seconds to
  4.54 seconds.
- Compatible generator cases share three verified Spring toolboxes. Each Rust
  test still generates and inspects its own exact fixture, while identical
  Java compilation and JUnit execution are consolidated into compatible Maven
  projects. Exact Surefire XML counts, failures, errors, and skipped counts are
  asserted by the harness.
- The three generated applications share one suite PostgreSQL service and one
  suite Kafka service. Applications receive isolated databases, while their
  existing unique topics and groups isolate Kafka state. Generated
  Testcontainers wiring remains covered separately.
- Unit contexts for the applications use H2; their real database and messaging
  integration tests continue to run through Failsafe against PostgreSQL and
  Kafka. Unit Maven work overlaps service startup and the three OCI image
  builds.
- Generated Maven workspaces and their `target` directories persist below
  `target/jails-e2e-cache`. The cache key includes the `jails` executable, so a
  changed product regenerates fixtures. Every `cargo test` invocation still
  launches Maven and reruns every associated JUnit test; only unchanged source
  generation and Maven recompilation products are reused.
- All Maven, Docker, javac, Surefire, and Failsafe work shares a six-process
  toolchain budget. Four processes underused the machine; eight produced heavy
  contention and was substantially slower.

Measured on the same machine:

| Configuration | Complete CLI integration binary |
| --- | ---: |
| Original baseline | 156.89 s |
| Deterministic Kafka and shared process gate | 147.94 s |
| Short-lived JVM settings | 83.29 s |
| Consolidated fixtures and shared services, warm cache | 38.54 s |

The isolated generated-application gate is 27.69 seconds warm, down from
52.26 seconds. Its measured process totals were 99.93 user seconds and 21.82
system seconds with a peak RSS of 787,496 KiB.

The requested sub-30-second threshold is therefore reached for that previously
dominant application gate, but not yet for the complete unfiltered suite. The
best complete warm result is 38.54 seconds. The host had 8 GiB of swap fully
allocated and varying load during these measurements; `vmstat` showed roughly
10--11% I/O wait in busy runs. This explains some run-to-run variance, but the
remaining eight-to-nine seconds should not be claimed as solved by machine
noise. Further improvement needs another reduction in Maven/JVM startup work
or a safe long-lived build daemon, while preserving the exact test inventory.

Experiments deliberately not retained:

- A three-module Maven reactor took 56.70 seconds and raised peak RSS to about
  1.30 GiB.
- Parallel Failsafe classes took 47.79 seconds because the integration tests
  compete for the same host resources.
- Combining Surefire and Failsafe into one Maven verification took 45.56
  seconds.
- A four-process budget took 62.50 seconds, while eight processes drove host
  load above 24 and took 88.53 seconds.
- `mvnd` was not reliable in this environment because of a stale daemon socket.

Those changes were reverted; they are recorded here to prevent repeating
profiling paths that made the complete suite slower.
