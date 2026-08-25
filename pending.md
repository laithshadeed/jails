# What is pending

**This file is only what is not done.** What the code already *is* belongs in
`CLAUDE.md`; what it *does* belongs in `README.md`; why a closed decision went
the way it did belongs in git.

It replaces six files -- `plan.md`, `abstract.md`, `playground.md`,
`missing.md`, `refactor.md`, `test.md` -- and roughly 200 comments across the
code still cite them by section number. Those citations resolve through git:

```sh
git log --diff-filter=D -- plan.md    # finds the commit that removed it
git show <commit>^:plan.md            # prints it
```

(`refactor.md` is the one exception: the copy folded in here had been
regenerated untracked, so `git show` reaches an older tracked version.)

**Closed items are deleted from this file rather than marked done**, and the
same two commands find them: `git log -p -- pending.md`. A short index of what
closed and when is at the end, so a `pending.md §N` citation in the code can
still be resolved to a subject. Section numbers are **stable** — a closed
section's number is never reused.

**Every number below was measured on 2026-08-25** against `main`, and each item
says how. A claim with no measurement is an opinion and is labelled one.

---

## 1. Open defects in what jails generates

### 1.2 Four kinds refuse on a Spring Boot 2 project

`jails new --gradle --boot 2.7.18` makes pre-Boot-3 projects reachable, and
`add cors`, `g enum`, `g scaffold` and `g usecase` work there —
`what_jails_generates_for_boot_2_compiles_and_what_cannot_refuses_by_name`
generates them into a real Boot 2.7.18 project and runs `mvn test`.

Four still refuse, through `spring::require_jakarta_spring`, and the reason is
in the **main** source set where no test variant helps:

| kind | needs | Spring version |
|---|---|---|
| `add api` | `ProblemDetail` | Framework 6 |
| `add security` | `requestMatchers` | Security 6 |
| `g query`, `g transition` | `JdbcClient` | Framework 6.1 |

Each needs a Boot 2 form of the code it writes, and for `add api` that means
inventing the error envelope RFC 9457 replaced — which is the per-project
invention that capability exists to remove. **This is a deliberate limit, not a
gap nobody got to**, so the bar for changing it is a reason the refusal is
costing somebody something.

**One residual imprecision, stated rather than hidden.** `JdbcClient` is
Framework 6.1, which is Boot **3.2**, and `boot_major` reads a major and nothing
finer. The floor is drawn at 3, so a Boot 3.0 or 3.1 project still gets a
compile error naming `JdbcClient`. Drawn there rather than at 4 because 4 would
refuse the Boot 3.2+ projects this works on today.

### 1.4 Generated business behaviour is unwritten, by design

The ledger match rules and the Kafka listeners in every generated application
contain the application-specific reaction nobody has written, so the ledger does
not reconcile and a received event drives nothing. That is the honest boundary
of a scaffolding tool. **The open question is whether the declarative manifest
can be extended far enough to generate those decisions, or whether they are
properly the reader's code.** Opinion, not measurement — and §2.2 and §2.3 are
the two experiments most likely to settle it, because a ranking rule set and
four framework ports of one domain are both cases where the answer is
falsifiable rather than arguable.

---

## 2. The portfolio: what jails has to be able to build

The `examples/` applications are not demos. They are the acceptance criterion —
the only evidence that the generic machinery is generic, because a crawler, a
support inbox and a payments gateway are three lists of the same intents and
none of them gets a command, branch, enum or template in core. Every gap in §1
was found by building one.

Where the portfolio stands today:

| application | what it clones | manifest | proved by the suite |
|---|---|---|---|
| payments gateway | a payments gateway | `examples/payments-gateway/` | yes — `SPRING_APP_MANIFESTS` |
| support inbox | Intercom | `examples/support-inbox/` | yes |
| web crawler | Google | `examples/web-crawler/` | yes |
| ledger CLI | stacks.ai | `examples/ledger-cli/` | yes — `ledger_cli_manifest_builds_without_spring`, the one non-Spring proof |
| minicom | Intercom, ported from the Rails and Django originals | `examples/minicom/` | **no** |
| minicom-spring | the Gradle interview scaffold | `examples/minicom-spring/` | **no** — verified by hand on 2026-08-25, nothing holds it |
| Gradient Lattes | `gradient.md` | — | not started |
| Throxy persona ranker | `throxy/` | — | not started |

`gradient.md` and `throxy/` are **local-only inputs and are gitignored**, so a
clone of this public repository will not have them. Three reasons, the first
sufficient on its own: they are other companies' take-home material and not
ours to publish; `throxy/` is its own upstream repo, which is the gitlink
accident `/deps/` is in `.gitignore` to prevent; and `throxy/data/leads.csv`
carries real people's names, job titles and employer domains. **The generated
proof application is jails' own output and is committable — the brief it was
built from is not.** Anything landing in `examples/` has to stand on its own
without quoting either.

Two things fall out of that table before any new work. **Two Intercom-shaped
manifests exist and only one is proved** — `support-inbox` is in
`SPRING_APP_MANIFESTS` and `examples/minicom/` is not, so the second can drift
against a generator change with nothing failing. And `examples/minicom-spring/`
is the same shape: it is the proof that `jails new --gradle` works and it is
held by nothing.

### 2.1 Gradient Lattes — `gradient.md`

Spring Boot 4.1, Java 26. An ordering API for autonomous baristas over two bean
suppliers: a cheap roastery with limited stock and an expensive chain with
plenty. Part 1 hides the supplier choice from the caller behind a 30-second
deadline; part 2 makes two stores share a supply that runs out at lunch.

**The suppliers are part of the solution, not a hosted service.** The brief was
rewritten so nothing reaches the public internet: no external URLs, no
credentials, no `Authorization` header. jails writes the supplier service too,
and its funky behaviour is the thing being reproduced — 429 with a `Retry-After`
set from *when stock actually replenishes*, 200 with `{"success": "true"}` for
most orders, and 200 with a garbage body for ~5% of them **with the beans still
consumed**, which is what makes the rotten case expensive.

What it will exercise, and where it is likely to find gaps:

- **One client, two configurations.** `g client` writes one `@HttpExchange`
  interface; the roastery and the chain differ in stock and price, not protocol,
  so they are one port constructed twice. That interface is also the seam a test
  substitutes a fake at, so no test needs a socket — which is §9's rule about
  developer services, arriving from the application side for once.
- **Retry that reads the signal.** Honouring `Retry-After` rather than backing
  off on a constant. jails has no retry capability; `resilience4j` is in
  `deps.tsv` and nothing generates against it. This is the first real candidate.
- **A deadline, not a timeout.** "Apologise and offer instant coffee" at 30
  seconds is a budget spanning several supplier calls, which is a different
  thing from a per-call timeout and jails expresses neither.
- **Fair share between two stores.** A quota or allocator over a contended
  resource. `g idempotency` is the nearest primitive and is not it.
- **Seedable randomness and configurable stock**, so a 429, a rotten delivery
  and part 2's both-suppliers-empty case are forced on demand rather than waited
  for. `add testkit` gives deterministic clocks and ids; a seeded generator is
  the missing half.

### 2.2 Throxy persona ranker — `throxy/`

Spring Boot 4.1, Java 26. A Next.js scaffold (`src/app/api/rank/route.ts`) that
loads ~200 leads from `data/leads.csv`, ranks them against
`data/persona-spec.md`, and returns the best relevant contacts per company.
`GET /api/leads` lists, `POST /api/rank` ranks. Relevance filtering is part of
the ranking: an HR contact at a target company is a lead you should *not* email
about a sales platform.

**Two jobs, and the second is the interesting one.** Re-implement it in Spring
Boot — and do the whole homework **without any external service**, which means
without the Vercel AI SDK and without an OpenAI or Anthropic key. The original
brief expects an LLM to do the ranking; doing it locally forces the scoring to
be explicit, deterministic and testable, which is the only version jails can
generate and the only version a test can assert on.

Note the shape this shares with 2.1: **an interview brief pointing at an
external service, re-done with that service replaced by something local.** That
is not a coincidence, it is what makes both of them admissible as proof
applications at all — §9's success criteria forbid a test that needs a developer
service, and a proof app that cannot be proved is a demo.

Likely exercise: `add csv` for the lead load, `g record`/`g value` for the lead
and the persona spec, `g scaffold` or `g query` for the two routes, and a
scoring strategy — `g strategy` is the open-set primitive, one bean per rule,
which is exactly the shape "disqualification criteria plus weighted signals"
wants. If the persona spec's rules can be expressed as a manifest, that is
evidence for §1.4's open question; if they cannot, that is evidence against it,
and either answer is worth having.

### 2.3 All of minicom, with jails only

`minicom/minicom-public/` is a whole prototype Intercom: a Rails server, a
Django server, a Node server, a Spring server, and two static sites (`foo` on
`127.0.0.1:8008`, `bar` on `8009`) that talk to them. `examples/minicom/`
already ports the *domain* — users, messages, a read flag, a direction enum —
and `examples/minicom-spring/` reproduces the Gradle scaffold. Neither is the
whole thing.

The target is the rest: every server in that repository re-expressed as jails
manifests, and nothing hand-written. It is the largest of the three and the one
that most directly tests the claim in §1.4 — four framework ports of one domain
is the strongest available evidence about where the generic manifest stops.

Start by proving what already exists: put `examples/minicom/` into
`SPRING_APP_MANIFESTS` and `examples/minicom-spring/` behind a Gradle equivalent
of it, so the two manifests that exist stop drifting silently. That is a small
change and it is the prerequisite for the rest.

### 2.4 The cost, which has to be decided before the first one lands

**`SPRING_APP_MANIFESTS` currently holds three applications and they dominate
the suite.** §9 measures the tail: three concurrent Failsafe runs against the
shared PostgreSQL and Kafka, starting at ~21.5 s and alone determining when the
CLI binary ends. Adding three or four more proof applications to that list
multiplies the thing that is already the bottleneck, on a suite that is
59.60 s today and has a stated target of 30.

So decide the relationship first. The options, none of them free:

- **All of them in `SPRING_APP_MANIFESTS`.** Honest and slow. Only viable after
  §9's Failsafe tail is shortened, which makes this blocked on that work rather
  than merely expensive.
- **A tier.** Proof applications that run on every `cargo test`, and a larger
  set behind an env var that CI runs and a laptop does not. The risk is the one
  §9's success criteria name: a test that does not run by default is a test
  nobody notices breaking, and this repository already has the
  `JAILS_REQUIRE_TOOLCHAIN=1` precedent for turning a silent skip into a
  failure — the same trick would have to apply here.
- **Generate-and-typecheck by default, full Maven gate on a subset.** Cheapest,
  and it gives up exactly the property the proof applications exist for: that
  the generated project *runs*.

**Do not add the first new application before choosing.** Three of them arriving
one at a time, each adding ten seconds, is how the suite gets to two minutes
with nobody having decided that it should.

---

---

## 3. Gradle and Maven parity

**Maven stays the default.** `jails new` with no `--gradle` creates a Maven
project and should go on doing so, and `jails new --gradle` is done —
`examples/minicom-spring/` is the manifest, verified end to end against real
Gradle 8.5 and JDK 21. `build.gradle.kts` and a root holding only
`settings.gradle` stay `Foreign` on purpose.

Still Maven-only:

| what | why it is not portable yet |
|---|---|
| `jails fmt` | The *transactional* half. `route::format` runs the formatter in a sandbox laid out from the projection, so the reformat is a reviewed diff committed in the same transaction — and it drives that with Maven. Gradle in a throwaway tree needs its wrapper, its caches and a writable `build/`, which is a different bargain. It refuses by name and points at `./gradlew spotlessApply`, which the project is already configured for |
| `testd`, `test --fast`, `test --affected`, `jails console` | All need a *resolved classpath*, which jails gets from `dependency:build-classpath`. Gradle has no equivalent without adding a task to the build — and adding one to a file the reader owns, for a convenience, is a different bargain from splicing a dependency they asked for |

---

## 6. The abstractions worth introducing

### 6.6 Where the other traits belong

Zero traits is a legitimate Rust style. It stops being legitimate where the same
shape repeats with no way to name it.

- **`Renderer`.** Every generator is a free function reached through a 36-arm
  match, each taking a different tuple. `spring::Slice` fixed exactly this for
  the Spring kinds and worked — no function in `spring.rs` takes more than five
  parameters and a ratchet holds it there. The same treatment has not reached
  `generate/recipes.rs`. §6.2's request object made it possible.
- **`ToolRunner`.** Real Maven is mocked by shadowing `PATH` with a shell
  script, and `real_path_without_mvnd()` exists to rebuild `PATH` around `mvn`'s
  launcher shelling out to coreutils. That is a lot of machinery to avoid one
  trait behind `process::CommandSpec`. **Genuinely optional** — the PATH
  approach tests the real argv construction, which a fake would not. Weigh it,
  do not assume it.

---

## 8. Files and tests

### 8.1 The largest module is `doctor/wiring.rs`

658 production lines, and the ratchet is at exactly that. It is a list of
independent checks, which is the one shape where length is not complexity — but
it is the largest module in the workspace now, and **the honest answer to the
next rise there is the split, not another ceiling**. That is what happened to
`projection.rs` when §3's build-feature key pushed it over: its two per-key arm
lists became `projection/edit.rs` and the row fell 662 → 649.

---

## 9. Test-suite performance

**Not achieved: plain, unfiltered `cargo test` under 30 seconds.**

Measured 2026-08-25, after a binary change invalidated the generated-project
cache:

```sh
/usr/bin/time -v -o /tmp/jails-full.time \
  env -u JAVA_TOOL_OPTIONS -u MAVEN_OPTS cargo test > /tmp/jails-full.log 2>&1
```

59.60 s wall, 292.91 user CPU-seconds, 57.63 system, 648,816 KiB peak RSS,
173,537 involuntary context switches. The CLI binary alone was 38.74 s with
177/177 passing. The best recorded warm CLI figure is 38.54 s, so the CLI half
is at its known floor and the extra ~21 s is regeneration plus the other nine
binaries.

**The bottleneck is CPU and scheduling, not disk, network or swap.** A warm CLI
run accumulates ~255 CPU-seconds in ~34 wall seconds with ~219k involuntary
context switches, only 16 major faults, and almost no permit-queue time at a
limit of eight. The tail is three concurrent real Failsafe runs against the
shared PostgreSQL and Kafka services; once they start they alone determine the
end of the binary.

**The next optimization must shorten or safely overlap that Failsafe/Maven tail
without deleting, disabling, ignoring or filtering any acceptance test.** The
remaining candidates are another reduction in Maven/JVM startup work, or a safe
long-lived build daemon.

### Reproducing a measurement

Dependency-warm throughout: Cargo registry, Maven local repository, container
images and Rust artifacts already present. Do not delete `~/.cargo`, `~/.m2` or
the image store. `target/jails-e2e-cache` holds generated projects and their
Maven `target` directories; **its key includes the `jails` executable**, so
always warm up after switching revisions.

```sh
# a warm number is the *second* invocation
cargo test --test cli && cargo test --test cli

# per-subprocess timings: start_ms, run_start_ms, end_ms, queue_ms, run_ms
JAILS_TEST_PROFILE=1 cargo test --test cli -- --nocapture 2>&1 \
  | rg JAILS_TEST_PROFILE

# the toolchain permit limit is 6 unless overridden; record it with any result
JAILS_TEST_MAX_TOOLCHAIN_PROCESSES=8 JAILS_TEST_PROFILE=1 cargo test --test cli
```

`queue_ms` is time waiting for a toolchain permit and `run_ms` is time inside
the child — libtest's own per-test duration includes the wait and is therefore
misleading. Stable libtest rejects `--report-time`; it is nightly-only.

Run three warm trials and report min/median/max. Never compare a quiet run with
one taken while another Cargo, Maven, Java or Podman job is active. A full zram
allocation is **not** proof of swapping — `vmstat`'s `si`/`so` columns decide
that.

Verify a measured run did not silently omit tests:

```sh
rg 'test result:' /tmp/jails-full.log
git diff <base>..HEAD -- | rg '@Disabled|#\[ignore\]|DskipTests|Dtest='
```

### Experiments deliberately not retained

Recorded so nobody re-walks a path that made the suite slower:

| tried | result |
|---|---|
| three-module Maven reactor | 56.70 s, peak RSS ~1.30 GiB |
| parallel Failsafe classes | 47.79 s — the ITs compete for the same host resources |
| Surefire and Failsafe in one Maven verification | 45.56 s |
| four-process toolchain budget | 62.50 s |
| eight-process budget | 88.53 s, host load above 24 |
| `mvnd` | unreliable here — stale daemon socket |
| Testcontainers cross-run reuse | **rejected on correctness**: the reuse key does not identify the project, retained state leaks between runs, and Ryuk deliberately does not reap reusable containers |

Two retained findings worth not rediscovering: short-lived Maven JVMs use Serial
GC and `-XX:TieredStopAtLevel=1` (a representative verification went 9.89 s →
4.54 s), and Podman's default `--pull=missing` cost 66.25 s on a fully cached
build against 1.19 s with `--pull=never` — the harness pre-pulls every `FROM`
image, so its builds use `--pull=never`.

### Success criteria, unchanged

- `cargo test` still runs every Rust, generated JUnit, Surefire, Failsafe and
  container integration test it runs today.
- No test requires developer services on ports 5432, 6379 or 9092.
- No unrelated Spring context repeatedly attempts to connect to Kafka.
- The suite passes from a clean container state.
- Wall time, peak aggregate memory, involuntary context switches and container
  starts are recorded for each phase.

---

---

## 10. Documentation and the gates that shape it

### 10.2 Load-bearing citations of deleted files

Roughly 154 comments cite `plan.md §N` and 48 cite `abstract.md §N`; six name
`missing.md` or `refactor.md`. Those citations are the best record of *why*, and
they resolve through the two git commands at the top of this file — but a
citation needing two git commands to follow is one nobody follows, and the code
is organised around section numbers in documents that are not present.

Promote the ones a **rule still depends on** into short decision records and
re-point those citations:

```text
  docs/decisions/001-one-writer.md
  docs/decisions/002-transaction-protocol.md
  docs/decisions/003-machine-state-compatibility.md
  docs/decisions/004-hermetic-processes.md
  docs/decisions/005-closed-schemas.md
```

Leave the rest citing `plan.md §N`. A *historical* citation is fine; a
load-bearing one is not.

---

## 11. Not started, and open by design

- **Conflicted merges cannot be resumed.** When a regeneration and a reader's
  edit genuinely overlap, the three-way merge produces conflict markers. The
  specification commits those with a frozen record that the next invocation
  continues or aborts. The bytes are produced and validated and
  `jails-protocol`’s `durable/conflict.rs` has the abort's both-images machinery; the
  frozen record, the refusal while it stands, and the continue/abort commands do
  not exist (`jails --help` has no `continue` or `abort`). jails refuses
  instead, naming the hunk count. **It lands as one piece or not at all** — a
  project that can enter a conflicted state and not leave it is worse than one
  that refuses the merge. Building the enter side alone was tried and backed
  out.

- **Unmeasured:** the k6 load profile `add loadtest` writes has never been run,
  so the p99 claim is unmeasured and says so. Spring context-cache misses across
  the example applications have never been counted.

- **Anti-goals**, unchanged: domain-specific generators, executable plugin
  hooks, a conditional template language, an ORM or a runtime support jar,
  silent Gradle support, an embedded model server, incremental `check`, or
  treating a skipped test as coverage.

---

## Sequencing

The list this file opened with is spent: steps 1 through 7 — honest gates, dead
code, the `Codec` trait, one request and one field model, one table per kind,
one transaction protocol, and the crate and file splits — are all done, and §4's
executor gate reads **0** against a target of 0.

What is left does not sequence into a line, because the three groups barely
touch:

1. **§2, the portfolio.** The largest item by far, and §2.4's tier decision
   comes first — it decides whether the three applications wait for §9. The
   cheapest first move is putting `examples/minicom/` into `SPRING_APP_MANIFESTS`
   and `examples/minicom-spring/` behind a Gradle equivalent: they exist, they
   are held by nothing, and they answer the cost question with real numbers.
2. **§9, test-suite performance.** What makes §2 affordable. Independent of
   everything else here.
3. **The small ones**, in any order: §6.6's `Renderer`, §10.2's decision
   records, §3's two Maven-only commands, §11's conflict resume.

§1.2 and §1.4 are not work items so much as stated limits; they change when
somebody finds them costly.

---

## Closed, so a `pending.md §N` citation still resolves

Deleted from this file on **2026-08-25**; `git log -p -- pending.md` has each
one in full, with the measurement it was closed on.

| § | subject |
|---|---|
| 1.1 | a `@unique` violation answered 500; `add api` renders a `DuplicateKeyException` arm, `doctor` reports the drift, `jails sync` repairs it |
| 1.3 | two lists of the same field types, drifted five apart |
| 4 | the R6.4 executor gate read green over unfinished work; it measures what its rung claims and reads 0 |
| 5 | `jails new` was not a second transaction protocol — publication by rename, said in a type |
| 6.1 | one `Codec` trait; 126 types, eight monomorphisations deleted |
| 6.2 | one validated request; `ResolvedIntent` deleted, `too_many_arguments` denied |
| 6.3 | one field-spec parser; merging the two found two live divergences |
| 6.4 | one table per recipe; four of the "seven" were never tables |
| 6.5 | the empty-string sentinel, replaced by `Failure::Reported` |
| 7.1–7.7 | the crate boundaries, and every mutation through the executor |
| 8.2 | `tests/cli.rs` at 8,142 lines, split into six subjects of one binary |
| 8.3 | 901 lines of tests colocated out of `generate.rs`; `jails-engine`'s first |
| 8.4 | `playground/`, untracked: 663 files of output whose manifests are proved |
| 10.1 | `CLAUDE.md` described a seven-crate repository that had thirteen |
| 10.3 | a test was choosing production names (`invoke` for `dispatch`) |
| 11 (CI) | hosted CI, dropped — not wanted |
