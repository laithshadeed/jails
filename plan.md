# plan.md — what is left to build

Trimmed 2026-08-22 against `ae63145`, and re-audited against the working tree
later the same day. Everything that shipped has been cut: this file is now only
pending work, the standing rules that constrain it, and the evidence each item
is real. **What the code already is, and the traps in it, live in `CLAUDE.md`;
the user-facing surface is `README.md`.**

**The re-audit closed five sequence items** — the genericity test (§4.6), `why`
on every Maven failure and `why --json` (§10.1, §15.2), the §5.2 observability
and actuator defaults with their three `doctor` checks, the §5.3 datasource
defaults, and the `new`-owned production properties of §5.3/§5.7. All five are
in the working tree and **not yet committed** at the time of writing; verify
with `git status` before trusting a line below that says "shipped".
**§20 reviews that change** — six repairs, a vocabulary test that is leakier
than it looks, and two tests that got weaker.

**Section numbers are stable on purpose.** `CLAUDE.md`, `examples/DOGFOOD.md`
and `validation/README.md` cite them. Sections whose content is entirely
finished were deleted rather than renumbered, so §1, §3 and §8 are gaps, not
omissions.

## 0. The goal, and how to know it is met

### 0.1 The goal

**Build real applications using nothing but `jails` commands, with zero
hand-written Java or SQL — and make each rebuild faster, cheaper and easier
than the last.**

The applications are not the product. **jails is the product.** They exist to
make the tool's gaps observable, because a generic tool cannot be proved
generic by its own test suite.

Four apps exist and all four pass their gates —
`examples/{web-crawler,support-inbox,payments-gateway,ledger-cli}/.jails/app.toml`.
Each was chosen so no two share a floor: **A** web crawler (outbound I/O,
bounded traversal, termination), **B** support inbox (tenancy, ordering,
durable delivery), **C** payments gateway (money, idempotency, o11y,
throughput — the app §5 answers to), **D** ledger CLI (**no Spring at all** —
the control that catches Spring-shaped assumptions in the generic machinery).

**jails is never done with the loop.** The next application, in a shape none
of these four covers, is the next falsifier.

### 0.2 The one constraint

**jails stays a generic tool.** A crawler, a support inbox, a payments gateway
and a ledger CLI are four lists of the same generic intents.

> **The moment a proof app needs the word `crawl`, `conversation`,
> `workspace`, `payment`, `merchant`, `settlement`, `ledger` or `inbox`
> inside `src/` or `templates/`, the abstraction has failed. The fix is a new
> generic primitive, never a branch.**

§4.6 is the enforceable form: six questions every core change must pass, plus
a repository test that greps for showcase vocabulary. **That test now exists**
— `tests/genericity.rs::core_generation_stays_free_of_showcase_vocabulary`,
alongside the older canary
`app_manifest_plan_is_domain_blind_and_writes_nothing`.

### 0.3 The loop

```
1. Write / update the app's .jails/app.toml
2. Run the runbook (§18) — record EVERY command, verbatim
3. Did it need a hand-edit to Java or SQL?
       yes -> that is the finding. Do NOT fix the app.
               Write a friction-ledger row, fix jails generically, go to 2.
       no  -> continue
4. Does it pass its acceptance contract (examples/ACCEPTANCE.md)?
       no  -> same as above
       yes -> continue
5. Score it (§0.4). Did faster/cheaper/easier improve?
       no  -> pick the worst number, fix that, go to 2
       yes -> the app is done; move to the next one
```

**Rule 3 is the one that matters, and it is the easiest to violate under time
pressure.** Never hand-fix a proof application to make it pass. A manual edit
is not a fix — it is evidence, and it belongs in `examples/DOGFOOD.md`'s
friction ledger as a row naming the generic jails improvement it implies.

### 0.4 The scorecard — what "faster, cheaper, easier" means

Record these per app, per run, in `examples/DOGFOOD.md`. Every one is a
number, so "it feels better" is never the answer.

| Axis | Metric | Now | Target |
|---|---|---|---|
| **Easier** | **hand-written Java or SQL lines** | **0** | **0**, always — any other value is a defect |
| **Easier** | manual interventions during the gate | friction-ledger row count | trending to zero |
| **Easier** | commands from empty directory to passing gate | 4 (`new`, `mkdir`, `cp`, `app apply`) | **1** — see §18's closing question |
| **Cheaper** | manifest lines per app | 65 (A), 263 (B) | falls as generators absorb repetition |
| **Cheaper** | generated lines per manifest line | ~18× for one scaffold | rises |
| **Faster** | full gate wall time | **293 s** | container reuse is not the lever it looked like (§7); §10.2 is |
| **Faster** | edit → test result | 3,810 ms | ~110 ms (§10.2), measured not estimated |
| **Confidence** | acceptance clauses open | 4, all lifecycle (§4.2) | 0 |

**Hand-written lines must stay at zero.** It is currently zero across all four
manifests — the strongest single fact in this repository. If it ever goes
positive, stop feature work.

### 0.5 Done

**Each app is done** when it is generated from its manifest with zero hand
edits, passes its contract in `examples/ACCEPTANCE.md`, and has a
friction-ledger entry for anything that was awkward.

**jails is done with this round** when the four lifecycle clauses in §4.2 are
closed — atomic apply, drift repair, offline creation, hosted CI — because
those are what stop the loop from being cheap to run again.

---

## 2. The two budgets

### 2.1 Latency (measured)

| What | Time |
|---|---:|
| `mvn -o -q compile`, nothing changed | 2.27 s |
| `mvn -o -q test -Dtest=X`, warm | **3.81 s** (corpus median 2.57 s, p90 25.2 s) |
| `javac` one file with `-J-XX:+AutoCreateSharedArchive` | **0.25 s** (vs 1.45 s) |
| JUnit `Launcher`, precompiled, fresh JVM | 1.65 s |
| **same test, 2nd run in the same JVM** | **9–13 ms** |
| **one file recompiled via `JavaCompiler` API, warm** | **74–166 ms** |
| a domain record test itself | 5 ms |
| `postgres:17-alpine` via Testcontainers | 7.0–8.8 s + 0.45 s Ryuk |

**Edit one test, recompile, rerun is ~110 ms of real work; Maven charges
3,810 ms.** Quarkus' bar: 1,470 → 295 ms from *selection* alone, and it nags
above 4 s.

**The gate is 293 s and most of it is containers.** Reuse looked like the
lever and is not (§7); §10.2 and §19.3 are where the number actually moves.

### 2.2 Authorship — where the remaining asymmetry is

One `g scaffold` writes **1,180 lines in 39 ms**. Adding **one field** to it
is 6 files, ~17 edit sites, plus a hand-written migration — and there is still
no `g field` (`grep -c 'ArtifactKind::Field'` → 0).

**jails can create almost anything and change almost nothing.** The
support-inbox manifest is 263 lines and 40-odd intents; jails can create all
of them and evolve none of them.

| Change shape | Today | After §9 |
|---|---|---|
| Add a field to a resource | 6 files, ~17 sites, + a migration | 1 command |
| Model first (`g record`, then scaffold) | blocked; retype every field | `g scaffold <Name>` |
| `created_at`/`updated_at` | typed per table; `updated_at` never updates | `--timestamps` |
| Test data for a new test | `new` a 6-component record; +1 breaks 40 call sites | `g factory` |
| Change a field in `.jails/app.toml` | **fails on a path collision** (§9.7) | re-applied |

The right generator metric is **authored lines and decisions remaining after
generation**, not generated line count. Today that is **0** across all four
proof apps. §4.5 makes keeping it at zero a gate.

---

## 4. The proof applications

### 4.1 Why a proof has to be an application

jails' own test suite cannot prove jails is generic. Golden files pin bytes,
tier 3 compiles what someone wrote a test for, and neither answers the only
question that matters: **can a real product be built out of these primitives
with no hand-written Java, and without a domain word appearing in core?**

The harness exists — do not rebuild it: `examples/*/.jails/app.toml`,
`examples/ACCEPTANCE.md`, `examples/DOGFOOD.md`, and five tests in
`tests/cli.rs` from `app_manifest_plan_is_domain_blind_and_writes_nothing`
through `app_manifests_pass_the_full_generated_verification_gate`.

### 4.2 What is still open, and it is no longer capability work

All four apps pass. Every remaining clause in `ACCEPTANCE.md` is a **tool
lifecycle gap**, not a missing generator:

| Open clause | Closed by |
|---|---|
| Atomic whole-manifest `ChangeSet` | §11 |
| Provenance / drift repair | §11, whose primitive is §9.1 |
| Offline project creation | §11 — the asset already exists as `write_spring_fixture` |
| Execution of the generated hosted CI workflows | External — keep hosted CI a required check |

### 4.3 App C — the payments gateway

Shipped. The manifest is `examples/payments-gateway/.jails/app.toml`; its
acceptance contract is in `examples/ACCEPTANCE.md`. **It is the app §5 answers
to** — every row there is something the real payments system does and a
jails-generated app still does not.

What C exposed and is still open: **no CORS** (§13.1); **no idempotency
receipt primitive** — a `@unique` column gives one-row-per-key but not the
*retained result* semantics (§13.3); **no `--timestamps`**, so `createdAt` is
hand-declared five times (§9.5).

**It must not add a payments concept to core.** Money is `amount:long` plus a
`Currency` enum; idempotency is a unique key plus a receipt row; settlement is
a `durable-job`. If any of that needs a new noun in `src/`, that is the
finding — and see §9.8 on why there is no `money` field type.

### 4.4 App D — the control

Shipped. `examples/ledger-cli/.jails/app.toml`, plain Maven, no Spring, no
web, no database. It exercises `value`, `sealed`, `strategy`, `cli`, `command`
and `record` — kinds A/B/C never touch.

**The rule that keeps D a real control: adding to it must not add a line to
`src/`.** If it does, that line is the finding.

### 4.5 The authorship ledger — the number that proves the thesis

Record per app, per gate run:

| Metric | Why |
|---|---|
| Manifest lines | the input |
| Generated Java + SQL lines | the output |
| **Hand-written Java or SQL** | **must be 0.** Non-zero is a friction-ledger row, not a footnote |
| Manual interventions during the gate | should trend to zero |
| Commands from empty directory to passing gate | `new` → `app apply` → `check` |
| Gate wall time | 293 s today; also a latency budget |

"Smarter generators mean you move faster" is measurable exactly as
**hand-written lines per feature trending to zero while the feature set
grows**.

### 4.6 The genericity gate

Before a line lands in `src/`, all six must hold:

1. Can it be named without mentioning a showcase domain?
2. Is it useful to at least three materially different applications?
3. Is it a Spring/build/application concern rather than business behaviour?
4. Can a project decline it without weakening unrelated capabilities?
5. Does it lower through the same intent, capability and write path?
6. Does the generated application remain operable **without jails installed**?

Two mechanical guards, **both now in place**:

- `tests/genericity.rs` greps `src/` and `templates/` for showcase vocabulary
  (`crawl`, `spider`, `conversation`, `workspace`, `inbox`, `payment`,
  `merchant`, `settlement`, `ledger`, `reconcile`, `robots`) and fails on a hit
  outside a comment. **Do not trust it further than §20.2 says you can**: it
  matches whole lowercase-delimited words only, so `workspace_root`,
  `PaymentService` and `crawler` all pass, and the word list bans `workspace`
  while permitting `tenant`. It has exactly one allow-list entry, with its reason
  stated in the table: `http_workflow_java.java` may contain `robots`, because
  RFC 9309 is a web standard rather than a domain noun. **Keep the allow-list
  at one entry with a reason each** — the reason is the point, and a second
  unreasoned entry is how this test stops meaning anything.
  Landing it was not free, which is the evidence it was worth writing: it
  forced `workspace` out of `project.rs` (renamed to `reactor`, taking
  `about --json` to `schema_version: 2`) and `crawl`/`robots` vocabulary out of
  four Spring templates.
- **`app plan` must stay domain-blind and write nothing** — already pinned;
  keep that test first in the file, it is the canary.

---

## 5. Production defaults, from a real system

Read out of `/home/laith/code/projects/payments-gateway-service` (22 modules,
332,397 lines of Java, Boot 4 / Java 26, Prometheus + Tempo + Grafana, Hikari,
Kafka, k6, Helm). **The point of this section is that every row is
domain-blind**: each one is a Spring/ops concern any serious service needs, so
each has a generic home in jails and none introduces a payments concept.

**§5.2 and §5.3 are now implemented; §5.4–§5.6 are not.** The same grep that
returned nothing when this section was written now finds
`management.server.port` in `spring.rs`, `add/database.rs` and `doctor.rs`, and
`pool-name` / `pg_is_in_recovery` in `add/database.rs` — while `enforcer` and
`jacoco` are still absent everywhere.

The pattern to copy is not the config — it is that **each setting exists
because of a specific silent failure**, and the real system documents which.

### 5.1 What "batteries included" should mean

Rails' actual promise is not that it writes more code; it is that the defaults
are the ones an expert would have chosen, and you never have to know why. So
each row below becomes one of three things:

- a **generated default** in the capability that owns it,
- a **`doctor` check** when the failure is a misconfiguration jails cannot own,
- a **`why` rule** when the failure surfaces as a runtime symptom.

Never a fourth thing: an option the user has to discover.

### 5.2 Observability — `add observability` / `add actuator` — **shipped**

Every row of the original table is generated, with the reasoning carried as a
comment beside the property rather than here — which is where it belongs, since
the person who needs it is reading `application.properties`, not `plan.md`.
`actuator_slice` owns the management port, `/management` base path, the health
cache TTL, the narrow exposure list, the explicit `liveness`/`readiness` groups
and `info.app.*` from `@project.*@`. `observability_slice` owns the SLO buckets
with `percentiles-histogram=false`, the per-metric percentiles and expected-value
bounds, `tracing.propagation.type=w3c`, sampling, the three baggage field lists
(including `local-fields`, so an internal id is not propagated to a third party)
and the `/dev/stdout` access log with the `management.server.tomcat.accesslog.prefix`
override that stops Tomcat trying to create `/dev/management_stdout`. Both
capabilities write the same management values, so their application order does
not matter.

The three `doctor` checks landed with them, in `doctor::management_checks`:
management port isolation, a dangerous-endpoint scan of the exposure list
(`*`, `env`, `configprops`, `heapdump`), and a liveness group that must be
`ping` alone. All three are `Warn` with a `fix:` line, because the application
runs happily with all three mistakes — which is exactly what makes them
`doctor`'s business.

**What is left of §5.2, and it is small:**

- **Selective `management.metrics.enable.<name>=false`.** The real system
  disables six Resilience4j circuit-breaker series by name. jails cannot copy
  that list — the meters only exist if the project has those libraries — so the
  generic form is: when a capability installs a library with known-noisy
  meters, that capability disables them, the same way it owns its own
  properties block. Nothing needs it until a capability has noisy meters.
- **A pod-identity common tag.** `MetricsConfig` tags `application` from
  `spring.application.name`. The real system also tags `pod.name: ${POD_NAME}`,
  which is what lets you tell two replicas apart. It belongs with `add k8s`
  (§5.6), because the environment variable only exists if something sets it.

### 5.3 Data access — `add db` — **shipped**

`add db`'s properties block now carries `pool-name=primary`,
`maximum-pool-size=20`, `connection-timeout=1000`, `max-lifetime=60000`,
`initialization-fail-timeout=1`, an explicit
`transaction-isolation=TRANSACTION_READ_COMMITTED`, and the standout trick —
`connection-init-sql=SELECT 1/(1-pg_is_in_recovery()::int)`, which makes a write
pool that has landed on a read replica refuse to start instead of failing on the
first `INSERT` in production. `server.shutdown=graceful` and
`spring.lifecycle.timeout-per-shutdown-phase=30s` are there too.

The `new`-owned rows of §5.3 and §5.7 shipped in the same pass:
`write_default_properties` writes `server.max-http-request-header-size=16KB`,
`spring.threads.virtual.enabled=false`, `spring.mvc.problemdetails.enabled=true`,
`server.shutdown=graceful` and the shutdown-phase timeout, each with the comment
saying which silent failure it prevents. It only adds a key that is absent, so
it never argues with a project that has already decided.

**What is left of §5.3:** separate pools per role, sized independently, with the
inverse `pg_is_in_recovery` guard on the read pool. That is `--module`
territory — a second pool has no meaning until there is a second datasource to
point it at — and it stays documented rather than generated.

### 5.4 Build and quality gates — `new` and `add format`

The real root POM carries `maven-enforcer-plugin` with **`requireJavaVersion`
and `requireMavenVersion`**, `jacoco-maven-plugin`, `maven-checkstyle-plugin`,
`editorconfig-maven-plugin`, `flatten-maven-plugin`, `maven-dependency-plugin`,
and both `surefire` and `failsafe`.

Generic homes: **`new` writes the enforcer rules** — jails already knows
`TARGET_RELEASE`, so this is free, and it converts jails' most common `doctor`
FAIL into a build-time error with a fix line. **`add coverage`** owns Jacoco
with a stated threshold. **`add format`** already owns the formatter; add
`editorconfig` alongside it. `flatten-maven-plugin` matters only for
multi-module and belongs with `--module`.

### 5.5 Load and capacity — `add loadtest`

The real system ships `load-tests/` with **k6** (`load-test.js`, `api.js`,
`payload-builder.js`, `token-cache.js`, a `Makefile`, a `README`). Not JMeter,
not Gatling — a JS file and a binary.

Generic form: **`add loadtest` writes a k6 script derived from the generated
routes** (`inspect.rs` already computes the route table) with bodies from
`sample_value` — the fourth reuse of that machinery after fixtures, factories
and `.http` files — plus a `Makefile` target and a `README` paragraph. Then
**`jails bench --load` records p50/p95/p99 into `.jails/benchmarks/`**, and App
C's contract asserts a p99 budget. A tool whose pitch is speed should prove its
own numbers.

Note this deliberately replaces a `g load` written as a Java `main` with
HdrHistogram, which had an unresolved invocation problem — `jails run` finds
"the file with `static void main`" and a second one creates ambiguity. k6 has
no such problem because it is not Java.

### 5.6 Deployment — `add k8s`

`add docker` already generates a non-root multi-stage image running as
`10001:10001`, verified in the gate. What the real system adds and jails does
not: a **Helm chart** whose probes point at the **management port** by name
(`port: o11y`) with `failureThreshold: 5/3, periodSeconds: 10,
timeoutSeconds: 3`, and a `prometheus-rule.yaml` whose burn-rate alerts depend
on the SLO buckets in §5.2.

`add k8s` is reasonable *after* §5.2 exists, because the probes and the alert
rules are only correct if the management port and the buckets are. Sequence it
last, and keep it to one deployment, one service, one configmap and probes —
not a chart framework.

### 5.7 One honest counterweight — the default shipped, the checks did not

**`spring.threads.virtual.enabled: false`.** A Boot 4 payments system on Java 26
explicitly *disables* virtual threads and runs a bounded pool (`threads: 100`).

Do not read that as "virtual threads are wrong". Read it as: **a production
system with real throughput requirements made the opposite call, so jails must
not force it.** `new` now writes the property explicitly with a comment saying
the concurrency bound moves to every downstream dependency — which is the whole
point: the setting is a decision the reader can see and reverse, not a default
they inherit blind.

**Still to build: the two `doctor` traps**, neither of which is an opinion about
virtual threads:

- A virtual-threads application whose only work is `@Scheduled` **exits 0
  immediately** unless `spring.main.keep-alive=true`. Nothing logs an error; the
  process simply ends, and on Kubernetes it looks like a crash loop with no
  crash.
- Pinning is observable via the JFR `jdk.VirtualThreadPinned` event, on by
  default at 20 ms — **not** via `-Djdk.tracePinnedThreads`, which no longer
  exists on JDK ≥ 24, so a `doctor` note recommending it would be wrong on
  every JDK jails targets.

### 5.8 The meta-lesson

The payments system's `AGENTS.md` is 166 lines and is the highest-signal file
in a 332,397-line repository. It encodes conventions an agent would otherwise
violate: package layout, `CREATE INDEX CONCURRENTLY`, partition-by-date, reuse
`java.util.Currency`, the exact `./mvnw test -pl <module> -am -Dtest=<X>
-Dsurefire.failIfNoSpecifiedTests=false` invocation.

That is §15.1's argument, with evidence: **`jails new` should write an
`AGENTS.md`**, and its content should be *rendered from* the same tables
`jails lint` and `jails commands --json` use, so it cannot drift into a lie.
The payments file already carries jails' own `failIfNoSpecifiedTests` fix as
documented tribal knowledge — exactly the kind of thing a generator should be
handing you instead.

---

## 6. Maintainability — options, ranked

`src/spring.rs` is **6,586 lines** and still holds **31** whole Java files as
inline `format!` strings opening `r#"package {pkg};`, against 36 `include_str!`
templates — so the migration is past halfway and stalled, not unstarted. Every
brace in the 31 is doubled — the exact tax `src/template.rs` exists to remove —
and none of it is Java any editor or compiler can check. The file is 1.6×
`generate.rs` (4,035) and 4.4× `add.rs` (1,485), which **inverts** CLAUDE.md's
stated reason for the `add.rs`/`spring.rs` split; fix that rationale in the same
change as §6.5.

But file size is the symptom, not the disease.

### 6.1 The disease, measured

**"What files does kind X produce?" is answered in several places, and only
some of them are checked against each other.**

| Copy | Where | Status |
|---|---|---|
| 1. The generator | 14 `*_files` functions in `spring.rs`, all returning `Vec<(PathBuf, String, &'static str)>` | the source of truth |
| 2. **The destroy path list** | `generate::KIND_FILES` — one table of (tree, layer, placement, filename), plus `NO_FILE_TABLE` for the four kinds with no path list | **still a transcription**, just a shorter one |
| 3. The golden scenario | `tests/common/scenarios.rs` `SCENARIOS` | complete, and a test keeps it so |
| 4. The editor lists | four Lua tables in `jails.nvim` | pinned by `tests/editor.rs` |
| 5. The README table | prose | unchecked |

Copies 1 and 2 are *checked* against each other for every kind
(`tests/agreement.rs`, in both directions, with `ALLOWED_LEFTOVER` carrying a
reason per deliberate keep) — but **checked is not the same as single**. The
transcription is still there to drift; the test only says so after the fact.

So the maintainability question is not "how do we write less Rust". It is:
**how does "what kind X produces" stop having five definitions?**

### 6.2 The options

Ranked by value ÷ effort. B–E need no new file format at all.

**B. Delete the destroy path table; derive it from the generator.** *~1 day.*

`destroy` calls the same `*_files` function `generate` does and takes the path
out of each returned tuple. Copy 2 stops existing.

- **Buys**: kills a documented class of bug outright. No new format, no new
  concept, ~200 lines deleted.
- **Costs**: some `*_files` read records off disk (`fields_from_record`) to
  decide what to emit, and at destroy time the record may be the thing being
  deleted. Two mitigations, both precedented: make rendering **lazy** so paths
  are computed without bodies and without the `--on`/`--yields`/fields that
  `destroy` is never given, and keep the `g strategy` pattern where destroy
  deliberately reads disk to find implementations added by hand.
- **Note**: right mechanism for `--pretend`, where nothing has been written
  yet. For `destroy` *after a jails upgrade*, prefer the **recorded** file list
  of §11.2 — a recomputed path gives you today's answer for yesterday's file.
- **This needs D as its mechanism**; see below.

**C. Finish the template migration.** *Ongoing, incremental.*

Every `r#"package {pkg};` block becomes `templates/spring/*.java`. Already the
house pattern, and further along than it was: `add/` is effectively done
(17 `include_str!` against 2 inline blocks, both in `add.rs` itself), and
`spring.rs` is at 36 against 31.

- **Buys**: `spring.rs` from ~6,586 to roughly 2,500 lines *of decisions*; Java
  an editor highlights and a human can review as Java; no doubled braces.
- **Costs**: none beyond the work. Each extraction is independently reviewable
  and golden-testable.
- **Do it as you touch each generator, never as a big-bang refactor.**

**D. A typed artifact builder.** *~2 days, Rust only.*

Replace the `Vec<(PathBuf, String, &'static str)>` convention with a builder
each generator declares into:

```rust
Artifacts::new(&root)
    .main(layout::SERVICE, "{name}Command.java",        tpl::USECASE_COMMAND)
    .main(layout::SERVICE, "Default{name}UseCase.java", tpl::USECASE_IMPL)
    .test(layout::WEB,     "{name}ControllerTest.java", tpl::USECASE_CTRL_TEST)
```

- **Buys**: path and template declared together and once; `destroy` reads
  `.paths()`; each generator's *shape* becomes readable at a glance instead of
  buried in 400 lines of string building. Makes B fall out for free.
- **Costs**: 14 call sites to convert; still Rust, so no external tool can read
  it.
- **B and D are really one move** — do D as the mechanism, B as the result.

**E. Data-ise the type table.** *~1 day.*

`builtin_by_java_name` (`generate/field.rs`) and `builtin_mapping` (`sql.rs`)
are a pure lookup table — java type ↔ SQL type ↔ read expression ↔ write
expression ↔ sample value — expressed as Rust `match` arms in two files that
must stay in step.

```toml
# types/instant.toml
java = "java.time.Instant"
sql  = "timestamptz"
read  = "rs.getTimestamp(\"{col}\").toInstant()"
write = "Timestamp.from({recv})"
sample = "Instant.parse(\"2026-01-01T00:00:00Z\")"
imports = ["java.time.Instant", "java.sql.Timestamp"]
```

- **Buys**: adding a field type becomes one file instead of edits in two
  modules; the round-trip property (`reverse(forward(t)) == t`) becomes a table
  test; `jails inspect db` (reverse mapping) gets its table for free.
- **Costs**: small; the `write` expression must keep baking in the receiver
  (`Timestamp.from(x.at())`, not `x.Timestamp.from(at())`) — a documented trap.
- **Genuinely declarative, genuinely low-risk.** This is data, not logic.

**F. One descriptor file per kind.** *~1 week.*

```toml
# kinds/usecase.toml
name     = "usecase"
aliases  = ["uc"]
summary  = "A create workflow with a transaction boundary, route and tests"
requires = { spring = true, capabilities = ["db"] }
args     = { on = "required:Resource", yields = "optional:Event", fields = "spec" }

[[artifact]]
template = "spring/usecase_command.java"
path     = "{service}/{Name}Command.java"

[golden]                                   # REQUIRED KEY
fixture = "spring"
steps   = [["g","scaffold","Note","id:uuid@pk","title:string!"],
           ["g","usecase","CreateNote","id:uuid","title:string!","--on","Note"]]
```

Copies 1–5 collapse to one file, consumed by: the `ArtifactKind` enum and clap
aliases (generated in `build.rs`), `--help` text, `destroy`'s paths, the golden
scenario table, `jails commands --json` (which then *deletes* the Lua lists
rather than pinning them), the README table, and `AGENTS.md` in generated
projects (§5.8).

- **Buys**: the structural fix. And one property nothing else here has:
  **`[golden]` is a required key, so it becomes impossible to add a kind
  without a snapshot test** — converting a recurring discipline failure into a
  compile error.
- **Costs**: a week; a `build.rs`; a second place to look when reading a
  generator; and a standing risk of drifting into a template language.
- **Scope rule**: descriptors hold **data** — names, aliases, preconditions,
  template→path pairs, golden steps. Never logic. The test: *could this be
  wrong in a way only a human reading the generated Java would notice?* If yes,
  it is logic and it stays in Rust. `usecase`'s compatibility check and
  inference engine are logic.
- **Do F after B–E**, because they change what a descriptor needs to hold.

**Where F comes from, so nobody re-derives it.** Angular schematics
(`ideas/angular/packages/core/schematics/collection.json`) and Nx
(`ideas/nx/packages/js/generators.json`) independently converged on exactly
this object — `factory`, `schema`, `aliases`, `description` — which is the
strongest available signal that F is a stable design rather than a guess.
Angular's meta-schema also names three fields worth copying that are easy to
miss: **`extends`** (specialise a generator without forking it — a better
answer to "flexibility" than a plugin hook), **`hidden`** (kinds that exist for
composition should not clutter `--help`), and **`private`** (callable from
another generator but not from the CLI — exactly `outbox`'s status).
OpenRewrite's `rewrite.yml` makes **`preconditions`** first-class data, which
is what `requires = { spring = true, capabilities = ["db"] }` is.

**Considered and rejected: path metadata in the template header**
(`// jails:path {service}/{{name}}Command.java`). Ordering and conditionality
are not expressible, and roughly a third of jails' artifacts are conditional
(`--yields` adds the outbox; `repository_wiring` changes the adapter shape).
You would end up with the header for simple kinds and Rust for the rest — six
copies instead of five.

### 6.3 The rule that separates a good option from a bad one

A DSL earns its place when you need to **analyze** the programs, not merely run
them. jails genuinely needs that: `destroy` needs the inverse, `--pretend`
needs the plan, `app apply` needs drift detection. But look at *what* it needs
to analyze: **paths and ownership, never content.**

> **Model the output, not the process.** Declare the artifact set; keep the
> decisions in Rust.

That rule is what separates B/D/F (worth doing) from a generator DSL
(rejected — see §16).

Two supporting observations worth not re-deriving:

- **Rails' generator is a script of actions, not a template.** jails already
  has that vocabulary scattered across `pom::add_dependency`,
  `register_command`, `install_test_container_import` and the `@Import` merger
  — the independent argument for `src/codemod.rs` (§11).
- **OpenAPI generator picked Mustache *because* it has no conditionals**,
  independently arriving at `template.rs`'s rule.
- **Dart `build_runner` is the model jails deliberately rejects.**
  `build_extensions` + `build_to: source` is a *derivation* model where output
  is a function of a source file and is regenerated every build. jails takes
  the other branch: **you own and edit the generated code.** The moment
  generated code is "not yours", `g field`, `edited_files` and
  print-never-clobber all stop making sense.

### 6.4 Recommended path

1. **B + D** (~2 days) — the artifact builder, and derive the destroy paths.
   This is the one that removes a real bug class.
2. **C** (ongoing) — templates out of `spring.rs` as you touch each generator,
   plus §6.5's file split.
3. **E** (~1 day) — the type table as data. Independent of everything else.
4. **F** (~1 week) — descriptors, once B–E have settled what they must hold.

### 6.5 The file split, whichever option you take

`spring.rs` is one file for two unrelated reasons: Spring *capabilities* and
Spring *generators*. Mirror the convention `add/` and `generate/` already use:

```
src/spring.rs            -> require_spring, exposure_include, shared helpers
src/spring/capability/   -> api.rs actuator.rs cache.rs security.rs observability.rs
src/spring/workflow/     -> usecase.rs query.rs transition.rs
src/spring/durable/      -> job.rs outbox.rs sink.rs
src/spring/http/         -> client.rs fetcher.rs workflow.rs
src/spring/schema.rs     -> association.rs
```

and **fix CLAUDE.md's rationale in the same change**, so the next split is made
on a true premise.

### 6.6 Extension: four tiers, and where a plugin system is and is not safe

README defers "any kind of plugin system". That deferral is right about *one*
form of plugin and wrong as a blanket answer, because "I want flexibility"
decomposes into four wants with wildly different costs.

**What a plugin system buys** is third-party extension without a core release,
plus per-team customisation. For a tool whose maintainer and user are the same
person, the first is worth approximately nothing. What it **costs** is
permanent: a public API surface that must stay stable forever, version skew
(JHipster's `--blueprints` is the standing cautionary tale — the override
surface is every sub-generator, so every core change can break one), untested
combinations, and the loss of the property that every generated file is
golden-tested.

So do not ask "should jails have plugins". Ask which tier is wanted:

| Tier | The want | Mechanism | Status |
|---|---|---|---|
| 1 | "put generated code somewhere else" | `jails.toml [layout]` | **exists**; §12's `jails adopt` extends it |
| 2 | "change what the generated code *looks like*" | `--template-dir` / `.jails/templates/` override, resolved before `include_str!` defaults | **worth doing** |
| 3 | **"add a new generator"** | **data-only kind**: a descriptor plus templates dropped in `.jails/kinds/` or `~/.config/jails/kinds/` | falls out of option F |
| 4 | "add a generator that makes decisions" | Rust, and a release | unchanged |

**Tier 2 — template overrides.** OpenAPI generator's `-t/--template-dir` is the
precedent (`modules/openapi-generator-cli/.../cmd/Generate.java:88`, wired at
`:504`) and it is the flexibility people actually reach for: not a new
generator, just *this* class shaped differently. Resolution order becomes
`.jails/templates/<name>.java` → `~/.config/jails/templates/<name>.java` → the
`include_str!` default. Cheap, independent of everything else. The honest cost:
**an overridden template is not golden-tested**, so a project that overrides
one has opted out of the guarantee for that file. Mitigate by having `doctor`
report every active override by name — the same honesty rule as `remove`'s
`unowned_properties`.

**Tier 3 — data-only kinds, a plugin system in the only form that does not
break jails' guarantees.** A kind expressible as metadata, a list of
`template → path` pairs and a `[golden]` block, with **no conditionals**, needs
no Rust. Dropping such a directory in gives a user-defined generator with no
arbitrary code execution, no API to keep stable, and — because the descriptor
carries its own golden steps — output that is still snapshot-tested. `destroy`
works on it for free, because the path list is the same data.

**The line is precise and checkable: data is extensible, logic is not.** A kind
that needs an `if` is logic and belongs in core. Two guards, both cheap:
**refuse conditionals outright** in the descriptor schema (an unknown key is an
error, the same closed-set rule `jails.toml` and the field markers already
use), and **make the boundary visible** — `jails commands --json` and `doctor`
report which kinds are core and which are data-only, so nobody discovers by
accident that half their generators are unversioned local files.

**What stays refused, at every tier**: lifecycle hooks, arbitrary shell,
downloadable packs, and anything that executes code at plan or apply time.
cookiecutter is the concrete evidence — `cookiecutter/hooks.py:95` runs
`pre_gen_project` / `post_gen_project` through `subprocess.Popen(...,
shell=run_thru_shell)`, annotated `# nosec` in its own source. That is
arbitrary shell execution from a downloaded template. The refusal is not
squeamishness; it is a citation.

---

## 7. Corrections ledger — designs that rest on facts that are not true

Read before implementing anything that sounds like one of these.

| The claim | What is actually true |
|---|---|
| **AOT cache pays on every devtools restart, `mvn test` fork and `jails run`** | The cache **refuses any classpath containing a directory**, and `target/classes` is one. All three named loops are out. A devtools restart is also a new classloader in the same JVM, so there is no process start to save. AOT is real — 6.6 s → 2.96 s on a **jar** classpath — and belongs to `jails build` / `add docker` |
| **`-XX:+AllowEnhancedClassRedefinition` covers ~90% of edits** | Not an OpenJDK flag; it is JetBrains Runtime / DCEVM. JBR tops out at JDK 25 and `TARGET_RELEASE` is now `"25"`, so the path is reachable — but still not a default: stock JVMTI is method bodies only, and jails' domain layer is records and sealed types, so **every domain edit is a restart on a stock JVM** |
| **JDWP `RedefineClasses` is command set 2** | Command set **1**; set 2 is `ReferenceType`. A working client is ~400 lines, not 150. Use jdt.ls's HCR or `jdb redefine` first |
| **`SseEmitter`'s never-time-out value is `Long.MAX_VALUE`** | Spring's own reactive path uses **`-1L`**; Spring's default is `null` and the 30 s is Tomcat's `Connector.asyncTimeout` |
| **Intercom webhooks are `X-Hub-Signature-256` / SHA-256** | Intercom signs `X-Hub-Signature` with HMAC-**SHA-1**, `sha1=` prefix, keyed by `client_secret`. A verifier built to the wrong spec **rejects every real delivery** |
| **A crawler needs jsoup and crawler-commons** | `http_workflow_java.java` parses with the JDK's `HTMLEditorKit` and fetches `/robots.txt` as a frontier entry. **Zero new dependencies.** Do not add an `add crawl` capability, do not clone jsoup |
| **Relations should be inferred from an `author:User` component** | `g association` does it **explicitly**, with both records read, types checked across the boundary, composite keys free, identifier length checked, no `ON DELETE` invented. Explicit beat inferred. What survives is narrower — §9.2 |
| **A `notify` crate would be the second dependency** | Third: `clap` and `clap_complete` are both declared. Polling is still right |
| **`rails test --only-failures`** | RSpec's, not Rails'. Rails prints a copy-pasteable `bin/rails test path:LINE` — copy that instead |
| **Boot 4 sets `spring.threads.virtual.enabled=true`** | Default is **`false`** — and see §5.7, where a production system sets it to `false` deliberately |
| **`-XX:TieredStopAtLevel=1` / `spring.jmx.enabled=false` are speed tips** | `spring-boot:run` already passes the first; JMX is already off. For STS4 live hover you want JMX back **on** |
| **Mint JWTs with Nimbus directly** | A level too low. Spring Security 7 ships `NimbusJwtEncoder.withSecretKey` (`@since 7.0`). The silent failure: **a JWT with no `exp` passes the default decoder**, and the default chain checks no issuer and no audience |
| **`withReuse(true)` is "safe unconditionally" and the largest lever on the 293 s gate** | **False, and it was tried.** The reuse key is `sha1` of the serialised `CreateContainerCmd` (`GenericContainer.hash`), and **nothing in it identifies the project** — so every jails project on `postgres:17` reuses the *same* database. Both number their migrations from `V001`, so Flyway refuses to start: *"Migration checksum mismatch for migration version 001."* The gate went red on the support inbox inheriting the crawler's schema history. A per-project label would fix the hash, but nothing deterministic and portable is unique per project. **So the generated config does not ask for reuse**; `jails setup` writes the machine flag, `doctor` counts what reuse leaves running, and `TestcontainersConfig`'s Javadoc states the one-line change and its cost |

---

## 9. Tier 1 — the authorship engine

### 9.1 `g field` — the highest-value generator jails does not have

```
jails g field Payment settledAt:instant?
```

Reads the record with `fields_from_record`, refuses a duplicate component,
appends in declaration order, then rewrites **only the derived files that still
match what jails would have written**, printing snippets for the rest:

```
updated  domain/Payment.java
updated  web/PaymentRequest.java
created  db/migration/V021__add_settled_at_to_payments.sql
skipped  adapters/JdbcPaymentRepository.java -- you have edited this file
         add to the select list:  settled_at
         bind:                    ps.setObject(9, …)
```

**The refusal is the design.** The ownership oracle is `edited_files`
(`src/add/database.rs`) — nine lines that re-render the template and diff the
bytes. It over-reports after a jails upgrade; over-reporting prints a snippet
you paste, over-writing destroys work. **Print, never clobber.**

Migration from `sql.rs`, forward-only. A `not null` column on a populated table
needs a default, so the generated SQL carries one and says so. **`--remove` is
not in v1** — dropping a column is a data decision.

### 9.2 The narrow relation gap that survives `g association`

`g scaffold Post id:uuid@pk author:User` still emits `author text not null` plus
a Javadoc reading *"Not persisted, because jails has no mapping for the type"*
(`src/generate/repository.rs`). **The app compiles, starts, returns 201, and
the author is gone.**

The fix is small because `association` proved the machinery: when a component's
type is a record in this project with exactly one `@pk`, **refuse the
scaffold** and name the two commands that do the job. A refusal that teaches
beats both a silent `text` column and a second inference path.

### 9.4 One rule for where fields come from

Thirteen call sites read a record off disk and disagree about failure (ten of
them in `spring.rs`). `g dto` and `g repo` are still the two extremes and only
one has moved: `dto` errors with a fix line (`src/generate.rs:1825`), while
**`g repo` still uses `unwrap_or_default()`** (`:1904`) and silently yields a
TODO-shaped adapter.
`usecase`, `query`, `transition`, `durable-job`, `association` and `outbox`
each raise their own wording. **And `g scaffold` does not read the record at
all** — `scaffold_artifacts` only calls `parse_fields`.

So model-first is blocked on the kind spanning the most files, while eight
newer kinds *require* it. State the rule once: **spec if given, else the record
on disk, else an error naming the record and the fix.**

### 9.5 `--timestamps`

Absent (`grep -rn timestamps src/ templates/` finds only prose). **Half of it
exists in the wrong half of the tool**: `usecase` already infers timestamps.
What is missing is the DDL and adapter side, where the lie lives — `updated_at`
is a column nothing updates. All four proof manifests hand-declare
`createdAt:instant`.

### 9.6 `g factory`, `requests/*.http`, and refusals

**`g factory Payment`** — defaults from `sample_value`; a component jails
cannot sample starts `null` and `build()` **throws naming it**, never a guessed
default. **`requests/payment.http`** as a `g scaffold` side artifact.

**Refusals are ergonomics.** `jails: …/fixtures/payments.json already exists`
is the message for the most common mistaken command in the tool. It should name
the cause and the next command. `doctor` is held to this standard by a test
asserting every `FAIL` carries `fix:`; **generators are not — add the same
test.**

### 9.7 The manifest is the ergonomic unit, and editing a field breaks it

`app.rs`: `plan` runs `add::preflight` and writes nothing; `apply` installs
capabilities, runs each pending intent, **writes state after every one** (so an
interrupt resumes), then **reconciles every capability a second time**.

The gap is the state key — `kind|name|package|fields|indexes|on|yields`.
**Change a `fields` line and you change the key**, so the old intent stays in
state and the edited one arrives *pending*; `apply` calls `generate`, which
finds the files and refuses. **It fails, with §9.6's useless message.** At 263
manifest lines that is not theoretical. `g field` is the primitive; §11.1 is the
durable fix.

### 9.8 What not to build here

No ORM, no lazy loading, no `g field --remove` in v1, no inflector overrides,
no rewriting an applied migration, no provenance ledger as a *prerequisite*
(§11), no `makemigrations` autodetect. **And no `money` field type** — App C
uses `amountMinor:long` plus a currency string, which is what the real payments
system does; a `money` type would be a domain concept in core and would fail
§4.6 question 1.

---

## 10. Tier 2 — the latency engine

### 10.1 What is left of the free wins

**One line, and it is the last of them: `mise.toml` from `new`.** Everything
else in this subsection has shipped.

`why` now runs on **every** Maven failure, not just watched runs:
`run::run_inherited` tees the output, `is_maven_program` recognises the six
`mvn`/`mvnw`/`mvnd` spellings, and `report_maven_failure` pipes the tail through
`why::report` — so all 20 rules apply to `build`, `test`, `check` and `mvn`, and
an unrecognised failure says so and points at `jails doctor` instead of leaving
a raw log. `jails test`'s flags (`Class#method`, `path:line`, `--failed`,
`--fail-fast`, `--slowest`, the rerun line on failure), the devtools poll
defaults from `new`, and `jails setup`'s reuse flag were already done.

### 10.2 `jails test --fast`

**Step 1, console launcher.** Splice `junit-platform-console` test-scoped with
**no version** (Boot's parent imports `junit-bom`), then `java @cp.args
org.junit.platform.console.ConsoleLauncher execute --select-method …
--details=testfeed --fail-if-no-tests`. `cwd` must be the module root.
Estimated 0.35–0.6 s vs 2.57 s — **unverified; measure first** (§19.1).

**Step 2, `jails testd`** — one resident JVM holding
`ToolProvider.getSystemJavaCompiler()` and the `"junit"` provider over a unix
socket. Compile in-process (74–166 ms warm), run via
`LauncherFactory.openSession()` (9–13 ms warm), fresh `URLClassLoader` per run.
**That classloader cost is the one unmeasured piece** (§19.2).

**Step 3, `--affected`** — a reverse-dependency index from `.class` constant
pools: ~120 lines (skip entries by tag width — `CONSTANT_Long`/`Double` take
**two** slots — keep `Utf8` and `Class`, scan `Utf8` for `L<pkg>/<Class>;`).
Blunt rules for Spring; **unknown ⇒ run**; exclude `*IT` by default.

**The correctness price:** compiling only the changed file is unsound — a
removed method leaves a stale caller. Which is why **`jails check` stays `mvn
clean verify`** and every fast path falls back to it loudly.

### 10.3 `jails dev`

Watcher (150–250 ms poll, 400 ms quiet, plus Quarkus' extra 200 ms when a file
is size 0); compile with `javac -J-XX:+AutoCreateSharedArchive` (**0.25 s vs
1.45 s**); **classify before acting** — method body → swap; record component,
`sealed`, annotation, new class, field or signature → **restart, printing the
JVMTI reason by name**; `pom.xml` → full restart. **jails' domain layer is
records, so every edit there is a restart** — say so or it looks broken. Swap
via jdt.ls's java-debug bundle (free, with frame popping) before `jdb`, before
a Rust JDWP client. Write `target/classes/.jails-reload` only after a
successful compile and point `spring.devtools.restart.trigger-file` at it. Pipe
through `why::FATAL_MARKERS`. Quarkus' key map. `--timings` on everything.

**Check first (§19.5):** with m2e setting the output folder to `target/classes`
and devtools watching classpath directories, `:w` → jdt.ls writes the class →
devtools restarts in ~1.4 s, with no Maven. If that holds here, `jails run
--hot` is a README paragraph and a doctor check rather than a supervisor.

### 10.4 `jails run --tc`, `runner`, `boot`

`mvn spring-boot:test-run` with a generated `TestApplication` doing
`SpringApplication.from(App::main).with(TestcontainersConfig.class).run(args)`
and `@RestartScope` on the container bean gives a dev run backed by the
Testcontainers config `add db` **already generates**, with no compose — which
also routes around podman-compose. **`jails runner -e`** boots the context and
`getBean`s for the non-interactive case. **`jails boot`** =
`-Dspring.context.exit=onRefresh`: a startup smoke test with no port, and the
AOT training-run switch. This replaces "boot a context inside jshell", which
dies on the DataSource.

---

## 11. Tier 3 — lifecycle: the four clauses `ACCEPTANCE.md` still names

Sequence, rather than a 3–5 week `ChangeSet` up front:

1. **`g field` first** (§9.1) — drift reconciliation for a changed manifest
   line *is* that command.
2. **`.jails/files` + `.jails/version`** (§11.2) — the path set.
3. **Regenerate and 3-way merge** (§11.1) — the content merge.
4. **Then** one atomic plan, if it still earns its keep.

Keep: paths normalised and confined; all conflicts detected before the first
write; `--pretend` and apply rendering the **same** object; expected hashes,
not string matching; a second identical apply a no-op. **A sequence of per-file
renames is not an atomic transaction** — promise deterministic preflight plus
crash recovery, and say so.

**Structural note:** `write_new_file` is *not* the single choke point it looks
like — `src/add.rs` writes an existing path directly with `fs::write`,
bypassing the collision check. A ledger hung off `write_new_file` alone has a
hole exactly where a capability updates a file it previously wrote.

**`src/codemod.rs`** — collect the splice primitives (`pom::add_dependency`,
compose blocks, property blocks, `register_command`,
`install_test_container_import`, the `@Import` merger, the `jails.toml`
one-liner) under named operations. Same extraction as `process.rs`; pays on
every capability and is a prerequisite for §6.2 option F.

**`new --offline`** closes the third clause in a day: vendor
`write_spring_fixture` via `include_str!`, explicit flag, and when
start.spring.io fails the error *suggests* it rather than silently falling
back.

### 11.1 Drift repair: regenerate and 3-way merge, not an ownership ledger

`ideas/copier` solves the same problem and solves it better, and jails is
unusually well placed to copy it.

**Copier's `_apply_update`** (`ideas/copier/copier/_main.py:1377`):

1. Regenerate the **old** template version into a temp dir, using the **stored
   answers** (`subproject.last_answers`) and the **stored commit**.
2. Regenerate the **new** version into a second temp dir.
3. Diff old-generated against new-generated.
4. `git apply` that diff to the user's real project, and where it conflicts,
   **`git merge-file`** — a 3-way merge leaving conflict markers
   (`_main.py:1610-1642`).

**The insight: you never need per-file ownership hashes.** You need the stored
inputs and the ability to re-run the generator. The diff between old-output and
new-output *is* exactly what jails changed; git decides how it lands on top of
the user's edits, which is a problem git is far better at than any hash
comparison.

**Why this fits jails better than it fits copier.** The hard part for copier is
step 1 — it must check out an old template commit. **jails' most important case
does not need that at all.** For the §9.7 failure — a `fields` line edited in
`.jails/app.toml` — the *generator* is unchanged; only the *intent* differs:

```
regenerate intent with the OLD fields   -> temp A        (jails is deterministic)
regenerate intent with the NEW fields   -> temp B
git diff --no-index A B                 -> patch
git apply --3way <patch>                -> the project
```

No version pinning, no ledger, no ownership model. `.jails/app-state-v1`
already stores the old intent — that *is* `last_answers`. And `--pretend`
becomes "print the patch", which is strictly more informative than the list of
paths it prints today.

**What it costs, stated honestly:**

- **It needs a git repo.** `jails new` runs `git init` by default and
  `--no-git` exists, so the fallback is: no repo → fall back to today's
  behaviour and say so.
- **It leaves conflict markers** in a `.java` file when the user has edited the
  same lines. That is correct, is what every developer already knows how to
  resolve, and is strictly better than `g field`'s "print the snippet and
  refuse" — which is the right call only while this does not exist.
- **The jails-upgrade case is genuinely harder, and Nx has the better answer.**
  When the *template* changed rather than the intent, step 1 would need the old
  jails binary to regenerate old output. Do not go there.
  `ideas/nx/packages/js/migrations.json` shows the alternative: a `generators`
  map whose every entry carries a **`version`**, a `description` and a
  `factory`, so upgrading collects every migration newer than the project's
  recorded version and runs them in order — transforming the existing project
  forward instead of reconstructing its past. That is the same shape as Flyway,
  which jails already uses for SQL. **Scope v1 to intent edits** — the case
  broken today — and add versioned migrations when the first template change
  actually needs one.

The `edited_files` oracle (`src/add/database.rs`) stays for the capability
path, where there is no stored intent to re-run.

### 11.2 The path set: record what was written, do not recompute it

`ideas/openapi-generator` solves the other half, and its javadoc states the
purpose exactly (`DefaultGenerator.java:2000`):

> *"Generates a file at `.openapi-generator/FILES` to track the files created
> by the user's latest run. This is ideal for CI and regeneration of code
> without stale/unused files from older generations."*

The implementation (`:2005-2050`) is ~40 lines: take the list of files the run
produced, relativise each against the output dir, **normalise separators to `/`
so Windows and Linux agree**, sort case-sensitively, write one per line.
Alongside it goes `.openapi-generator/VERSION`.

**jails should write `.jails/files` and `.jails/version` the same way:**

- **Better than recomputing paths from the generator** (§6.2 B). B is still
  right for `--pretend`, where nothing has been written yet. But for `destroy`
  *after a jails upgrade*, recomputation gives you today's paths for
  yesterday's files — and silently strands anything whose path changed. A
  recorded list cannot drift, because it is not derived.
- **It answers "what did this intent write?" directly**, which is the question
  `destroy` and drift repair both ask.
- **It closes the stale-file case `examples/DOGFOOD.md` names** — *"does not
  yet notice a generated file deleted afterward"*. Regenerate, diff the new
  file list against the recorded one, and act on the difference: files no
  longer produced are stale, files missing from disk were deleted by hand.
- **`VERSION` is exactly the pin §11.1's upgrade case needs.**

Two details worth copying rather than rediscovering: sort and separator
normalisation are what make the file diffable and stable across machines, and
`FILES` deliberately excludes its own metadata entry so regeneration does not
churn it.

**§11.2 gives you the path set; §11.1 gives you the content merge.** Neither
needs an ownership model.

---

## 12. Tier 4 — reach: the codebase you did not create

In `ideas/minicom-public/spring`, **zero of ~30 commands work** — the gate is
`generate::find_project_root`, 11 lines looking for `pom.xml` and nothing else,
with ~30 call sites and three further copies of the rule.

Dropping a **one-line stub `pom.xml`** into a copy makes `routes`, `beans`,
`stats`, `notes`, `rename --dry-run`, `destroy --pretend`, `doctor` and
`g record` all work against Gradle sources. `inspect.rs` and `rename.rs`
contain **zero** occurrences of `pom`.

```rust
pub(crate) enum Build { Maven, Foreign(&'static str), Bare }
pub(crate) fn project_build(root: &Path) -> Build   // new; find_project_root's signature unchanged
```

Nearest wins. Then three guards: `pom::read` says which build tool it found;
eight Maven-inherent commands get `require_maven`; and **`doctor` reports the
real build tool** — not optional, because a confident wrong report is worse
than a refusal. **Frame it in README**: *jails never reads, writes, parses or
invokes `build.gradle`.* That is strictly less than Gradle support.

Caveats: the stub-pom trick **changes the Java jails emits**
(`repository_wiring` returns `PlainJdbc`, `jspecify_available` false), so
degraded mode must *say* which shape it chose; **`add` still will not work** and
should not be exempted; **multi-module Gradle** puts `build.gradle` in `app/`
with `settings.gradle` above.

**`jails adopt`** writes a `[layout]` table, not new machinery — verified:
`[layout] web = "controllers"` made `stats` report `Web 2` (was `Other 4`) with
no code change. Map subpackages onto `LAYERS_IN_ORDER` through a closed synonym
table; a directory matching nothing is **reported, not guessed**. It must
**never** write `[project] capabilities`.

---

## 13. Tier 5 — the capabilities still missing

### 13.1 `add cors` — still the actual blocker

`grep -rni cors src/ templates/ README.md` returns **nothing**, and
`security_config_java.java` has `anyRequest().authenticated()` and never calls
`.cors(...)`. **A jails app plus `add security` cannot serve a browser
widget.** The naive fix is wrong in a way that bites later:
`applyPermitDefaultValues()` permits only GET, HEAD and POST and no credentials
— the classic "works until mark-as-read becomes a PUT". Name the methods, put
origins in a marked properties block, and **wire `.cors(...)` into the
generated chain in the same change.** Two doctor checks fall out:
`@EnableWebMvc` with the webmvc starter (switches off auto-configuration), and
`addMapping("/**")` with no `allowedOrigins`.

### 13.2 `add sse` — the four details every SSE design gets wrong

`-1L` (or `0L`), not `Long.MAX_VALUE`. **`onCompletion` alone suffices** for
removal — but it runs on a *container* thread concurrently with the
broadcaster, so the registry must be `ConcurrentHashMap<K, Set<SseEmitter>>`
with `newKeySet()`. **`spring.task.scheduling.pool.size` defaults to 1**, so a
15 s heartbeat blocking on one dead client stalls every other scheduled job.
**`Last-Event-ID` is not implemented by Spring** — emitting `id()` without a
`@RequestHeader` replay path advertises resumability you do not have. One
Framework-7-only fact that makes "SSE + virtual threads" real: Framework 7
replaced `synchronized` with a `ReentrantLock` throughout `ResponseBodyEmitter`
to avoid pinning.

### 13.3 The rest, each waiting for an acceptance clause

| Slice | The silent failure it prevents | Proves |
|---|---|---|
| **`g idempotency`** (a receipt keyed by scope + key + canonical request hash) | A `@unique` column gives one-row-per-key but **not the retained result**; a retry gets a 409 instead of the original response. App C is the first app that needs it | C |
| **`add coverage`** (Jacoco + a stated threshold) | §5.4 | C |
| **`add loadtest`** (k6 from the route table) | §5.5 | C |
| **`add k8s`** (probes on the management port, burn-rate rules) | §5.6 — only correct after §5.2 | C |
| **`g auth`** | Boot 4 auto-configures **no `JwtEncoder`**, and **a JWT with no `exp` passes the default decoder** | B, C |
| **`g webhook`** (inbound; `http-sink` is outbound) | Signature over **raw bytes**, `MessageDigest.isEqual`, Stripe's 300 s tolerance | B |
| **`add mail`** | Boot 4's `-test` twin convention; the IT reads mail back over POP3 as Boot's own test does | B |
| **`g search`** | A `generated always as (…) stored` `tsvector` — *generated*, because a trigger someone forgets on UPDATE is the silent failure | B |
| **`add flags`, `add shedlock`, `add storage`, `add arch`, `add nullcheck`** | Build the first time a project needs one. `add shedlock`: two instances fire the 02:00 job, customers get two emails, nothing logs an error | — |

### 13.4 The UI decision

An agent inbox needs HTML; a take-home that renders nothing is half a
submission. `g page` with **JTE** + htmx + `add sse`: JTE templates are
compiled, type-checked Java, so a renamed record component breaks the build
rather than the request — jails' bar — and its dev mode runs its own watcher,
so edits are sub-second with no dependence on devtools LiveReload (deprecated
in Boot 4.1 with no replacement).

---

## 14. Tier 6 — the editor

**Projectionist, not a Lua reimplementation.** `tpope/vim-projectionist`
accepts projections **in memory** via `projectionist#append(root, …)` — nothing
written into the repo, which dissolves the objection. On detect, run
`jails about --json` once per root and build the table from `layout` +
`base_package`. **11 layers means a generated slice crosses more directories
than it used to.**

**An enriched `about --json` is the prerequisite**: add `layout` (through
`Config::layers()`, i.e. *renamed* values), `base_package`, `capabilities`,
`java_root`/`test_root`, pinned to `LAYERS_IN_ORDER` by a test. Do not call it
"v2" any more — `schema_version` was already bumped to `2` by the
`workspace` → `reactor` rename the genericity test forced (§4.6), so the next
payload change is `3`, and the number now says nothing about which fields are
present. Normalise the version key while you are there: there are **three**
spellings now — `about` uses `schema_version`, `routes`/`beans` use `version`,
and `why --json` uses `version` at the envelope root.
**Add `line` to `Route`/`Bean`** — today `line` exists on `Note` and nothing
else, so `routes --json` is a list; with a line it is a quickfix and a picker.

**`gf` into JDK and project source — six lines, no plugin.** Neovim's
`ftplugin/java.vim` already sets `includeexpr`, `suffixesadd`, `include`,
`define` and honours `g:ftplugin_java_source_path`; only `'path'` is missing.

**jdt.ls settings and bundles.** Add `updateBuildConfiguration = 'automatic'`
(the default `'interactive'` is why **every `jails add` leaves red squiggles
until prompted**), `autobuild.enabled`, `downloadSources`, `-Xmx2G`. Then
**java-debug** and **vscode-java-test**, which give
`jdtls.dap.test_nearest_method()` — one test, no Maven, no Surefire — and hot
code replace with frame popping. Pair with `jails run --debug` and
`spring.devtools.restart.enabled=false` for that run.

**`:compiler jails`** — `$VIMRUNTIME/compiler/maven.vim` already carries javac
errors, non-parseable POM and the Surefire multi-line pattern. Copy the
`errorformat` verbatim (the `current_compiler` guard makes `runtime!` bite).

**Pickers** via `fzf-lua` (this config has no telescope) over `routes --json` /
`beans --json` — sub-50 ms on a project that does not compile, which jdt.ls
cannot do. **`jails src <Type>`** resolves a project type, else a type under
`deps/`.

**Keymap collisions**: `<leader>j{t,c,r,b,g}` vs `<leader>J{t,c,r,b,g}` — a
shift slip turns "extract constant" into `mvn clean verify`. Split
semantically. **`javac_lint`** recompiles the whole tree on every save, runs
bare `javac` with **no `--release`**, and re-runs `dependency:build-classpath`
on every pom change; fix all three and keep its output out of `target/classes`.
**Two bugs**: `setqflist({}, 'r', …)` **replaces** the list jdtls just built
(should be `' '`), and `vim.fn.termopen` is deprecated.

Note the `<leader>J...` keymaps live in a *third* repo
(`~/code/my-dotfiles/home/.config/nvim/init.lua`), which this project's git
history does not track.

---

## 15. Tier 7 — the agent as second user

### 15.1 `AGENTS.md`, with evidence

§5.8: a 166-line `AGENTS.md` is the highest-signal file in a 332K-line
repository. **`jails new` should write one**, and its banned-API list must be
*rendered from* the same table `jails lint` matches against, so it cannot drift
into a lie — a hand-written one is a `validation/README.md` waiting to happen.
Content: use `jails test <Name>`, not `mvn test`; `jails check` is the gate and
*why*; `jails doctor` before debugging the environment; records, no Lombok, no
ORM; the layer table; the field-spec grammar.

### 15.2 The rest

**`jails lint`** — a closed rule table over the stale-API families jails
already knows (`@MockBean`, `javax.validation`, Jackson 2 alongside 3,
`spring-boot-starter-web`, `@Entity`, Lombok, preview features), plus **`double`
in money code** from App C's contract. Sub-second, exit 1, `file:line`.

**`--json` everywhere.** Four commands have it now: `about`, `routes`, `beans`
and — the one this section called highest value — **`why`**, which emits
`{"version":1,"recognized":bool,"diagnoses":[{headline,because,fixes[]}]}` and
so makes the explanation available as quickfix text. `doctor --json`,
`test --json`, `stats` and `notes` are an afternoon each and unstarted.
**`jails commands --json`** then *deletes* the Lua lists rather than pinning
them (§6.2 F).

**`jails explain <kind>`** exposes the rationale the Javadoc carries, so an
agent stops "fixing" `@Repository` onto the second adapter. **Promote
`g cases`** — it turns a markdown brief's acceptance bullets into a test class,
and README mentions it once.

**No MCP server** — worse than the CLI an agent already shells to. **No LLM
inside jails** — deterministic generation is the product.

---

## 16. Anti-goals

| Temptation | Why not |
|---|---|
| Plugin **lifecycle hooks**, arbitrary shell, downloadable packs, a **generator DSL with conditionals**, codegen from an external schema language (note §6.6 Tier 3 is *not* this) | §6.3 and §6.6. `.jails/app.toml`'s closed schema and §6.2 F's descriptors are data, not logic |
| A template language with conditionals and loops | It would shrink Rust and grow something worse: logic no test can reach directly and no compiler can check. Substitution only; anything structural stays in Rust and arrives pre-rendered |
| Gradle *support* | Distinct from Gradle-directory *tolerance*, which is §12 |
| ORM, lazy loading, an Active Record clone | `g association` is a constraint, not a mapping layer |
| A `money` field type, a `payment` kind, a `spider` kind | §4.6 |
| A `jails-support` runtime jar | ActiveSupport lock-in; capabilities write classes *into* the project |
| Lombok; preview features; `StructuredTaskScope` | Editor tax; still preview |
| **`add crawl`, jsoup, crawler-commons** | Superseded — `http-workflow` does it with zero dependencies |
| Wrapping crawler4j / webmagic / Nutch / StormCrawler | Dead, platforms, or a second runtime |
| A Rust `jails lsp` | ~1.2–1.8k lines, a JSON parser, a second server fighting jdt.ls, parser pressure on `java.rs` |
| The AOT cache in `dev`/`test`; CRaC; JBR as a default | §7 |
| Maven 4, the build cache extension, `useIncrementalCompilation=false` | Not GA / restores a deleted `target/` / stale-dependent `NoSuchMethodError` |
| Making `jails check` incremental | The leftover-`.class` bug is real |
| Forcing virtual threads on | §5.7 — a production system chose the opposite |
| Treating a skipped test as coverage | `JAILS_REQUIRE_TOOLCHAIN=1` exists for this |

---

## 17. The sequence

**Proves**: **A** crawler, **B** inbox, **C** payments gateway, **D** ledger
CLI, **—** infrastructure.

**Closed in the 2026-08-22 re-audit**, and removed from the table rather than
struck through: the showcase-vocabulary grep test (§4.6), `why` on every Maven
failure and `why --json` (§10.1, §15.2), the §5.2 observability and actuator
defaults with their three `doctor` checks, and the §5.3 datasource defaults plus
the `new`-owned production properties (§5.3, §5.7). That is four of the seven
items that used to sit above `g field`, so **the authorship debt is now the top
of the list rather than the middle of it** — which is the right shape, because
it is the debt paid on every model change.

**Ahead of everything below: §20.1.** Six defects in the change that closed
items from this table, five of them one-liners, one of them (`jails build`,
`test` and `check` losing their colour) a regression against a trap `CLAUDE.md`
already documents. They are not in the table because they are repairs, not
features — but they come first.

| # | Item | § | Effort | Proves |
|---|---|---|---|---|
| 1 | **§6.2 B + D** — the artifact builder, then derive `destroy`'s paths from it. `KIND_FILES` is a shorter transcription, not a derivation; deriving it needs lazily-rendered artifacts so a path can be computed without a body | §6.2 | 1 day | A B C D |
| 2 | Editor config: jdt.ls settings + bundles + HCR, `'path'`, `:compiler jails`, keymap split | §14 | S, no Rust | — |
| 3 | **`g scaffold` reads the record**; one field-source rule; refusal messages carry `fix:` and a test enforces it | §9.4, §9.6 | S each | A B C |
| 4 | **`g field`** | §9.1 | M | A B C |
| 5 | **`.jails/files` + `.jails/version`**, then **regenerate + 3-way merge** | §11.1–11.2 | M | A B C |
| 6 | `--timestamps`, `g factory`, `requests/*.http` | §9.5, §9.6 | M total | A B C D |
| 7 | `g idempotency` | §13.3 | M | C |
| 8 | Scaffold **refuses** an unmapped project-typed component | §9.2 | S | B C |
| 9 | §5.4 enforcer rules in `new`; `add coverage`; `add loadtest` | §5.4–5.5 | M | C |
| 10 | Enriched `about --json` + line numbers; projectionist; pickers; `jails src` | §14 | M | — |
| 11 | **§6.2 C + §6.5** — templates out of `spring.rs` (31 inline blocks left), split the file; **§6.2 E** — type table as data | §6 | M, ongoing | — |
| 12 | `jails test --fast` + `jails bench` | §10.2 | M | D first |
| 13 | `add cors` | §13.1 | S | B C |
| 14 | **§6.6 Tier 2** — template overrides (`.jails/templates/`) + `doctor` reports active overrides | §6.6 | S | — |
| 15 | `new --offline` + `app init`; `mise.toml` from `new` | §11, §10.1 | S–M | A B C D |
| 16 | **§6.2 F** — one descriptor per kind; delete the Lua lists; `[golden]` becomes a required key | §6 | L | — |
| 17 | §12 marker widening + `jails adopt` | §12 | M | — |
| 18 | `jails testd` + `--affected` | §10.2 | L | — |
| 19 | `jails dev` v1 | §10.3 | L | — |
| 20 | `add sse`; `g auth`, `g webhook`, `add mail`, `g search`; `add k8s`; the two §5.7 virtual-thread `doctor` traps | §13, §5.7 | M each | B C |
| 21 | `AGENTS.md` + `jails lint` + `--json` on `doctor`/`test`/`stats`/`notes` | §15 | M | — |
| 22 | Atomic whole-manifest `ChangeSet`; `codemod.rs` | §11 | L | A B C |

Item 1 removes the last unchecked duplication of "what does kind X produce".
Items 3–8 are the authorship debt, paid on **every model change**, and they are
now the front of the queue. Item 9 is what is left of "batteries included" after
§5.2 and §5.3 landed. Item 18 is the biggest latency number and is correctly
late.

**The stopping rule:** when a proof app's acceptance clause is closed, stop
working on that capability. `ACCEPTANCE.md` says the gate may report
`generated`, `configured`, `user-owned` or `not selected` and **must never call
an unproved property guaranteed or production ready.**

---

## 18. Runbook

Every change, no exceptions:

```bash
cd ~/code/jails
cargo build && cargo test && cargo install --path .
```

Before believing a green run covered the generated-code path:

```bash
JAILS_REQUIRE_TOOLCHAIN=1 cargo test
```

Re-running the loop for one app (C shown; the others differ only in the
manifest and in `new` vs `new-cli`):

```bash
export JAILS_BIN="$PWD/target/debug/jails"
cd /tmp && "$JAILS_BIN" new payments-gateway --deps web,validation
mkdir -p payments-gateway/.jails
cp ~/code/jails/examples/payments-gateway/.jails/app.toml payments-gateway/.jails/
cd payments-gateway
"$JAILS_BIN" app plan                 # must write nothing; read the intent list
"$JAILS_BIN" app apply --no-start
"$JAILS_BIN" routes && "$JAILS_BIN" beans && "$JAILS_BIN" stats
"$JAILS_BIN" doctor && "$JAILS_BIN" migrate --check
"$JAILS_BIN" check                    # mvn clean verify
git -C . diff --stat                  # MUST be empty of hand edits
```

All four as regression, in-tree:

```bash
cargo test app_manifests_compile_without_manual_source_edits -- --nocapture
cargo test app_manifests_pass_the_full_generated_verification_gate -- --nocapture
```

**What to record for each app** (§4.5): manifest lines, generated lines,
**hand-written lines (must be 0)**, manual interventions, command count, wall
time. Then add one row per friction item to `DOGFOOD.md`, in its existing
shape: *Application | Step | Manual intervention or weak output | Generic jails
improvement.*

**Then ask the question this whole exercise exists to answer:** which two
commands above should have been one? Today the answer is already visible —
`new` + `mkdir` + `cp` + `app apply` is four steps that should be
`jails new <name> --app <manifest>`. That is item 15.

---

## 19. Measure before promising

1. Console-launcher wall time here (est. 0.35–0.6 s) and the resident-JVM band
   (est. 50–150 ms).
2. The cost of a fresh `URLClassLoader` per `testd` run.
3. How many distinct Spring contexts the proof apps build (`missCount` under
   `org.springframework.test.context.cache=DEBUG`). At 293 s and 123 tests
   **this is the highest-value measurement in the list.**
4. `postgres:17` with reuse under podman, and whether `withReuse(true)`
   disturbs `@ServiceConnection`.
5. Where jdt.ls writes `.class` files here — **§10.3's "the loop already
   exists" finding pivots on it.**
6. p99 for App C under the §5.5 k6 profile, before any performance claim.
7. ~~Whether `CSVFormat.Builder.build()` still exists at commons-csv 1.14.1.~~
   **Answered.** `add csv` pins 1.14.1 and `csv_reader_java.java` calls
   `.get()`; the unit test in `add.rs` binds the pinned version and the
   generated call together, and the real-toolchain tier compiles the result.
8. Confirm the tier-3 skips are actually gone now `TARGET_RELEASE` is `"25"`
   (`JAILS_REQUIRE_TOOLCHAIN=1 cargo test`), and that `doctor`'s daily false
   FAIL is gone with them. **Still open** — and it is the cheapest unanswered
   item in this list, so do it before believing any green run below.

Note the payments gateway targets **Java 26**, not 25. That is a data point,
not a reason to move: 25 is LTS and everything in this plan is available at 25.

---

## 20. Review of the 2026-08-22 in-flight change

Read against the **uncommitted** working tree while it was still being written,
so check each line against the code before acting on it. Nothing here is a
finding about jails' design; it is a defect list for one change.

**The headline, because it is the question this review was asked to answer: no
non-generic logic was introduced.** Every new code path — `OutputMode::Tee`,
`report_maven_failure`, `java::masked`/`without_literals`,
`doctor::management_checks`, `why --json`, the Hikari and actuator property
blocks — is keyed on Spring, Maven or ops facts, and passes §4.6 questions 1–5
without argument. What the change moved is **vocabulary**, and in three places
it moved it badly.

### 20.1 Defects with a known fix

| # | Defect | Where | Fix |
|---|---|---|---|
| 1 | **Colour is gone from every Maven command.** `run_inherited` now pipes for any Maven program so `why` can read the tail, but `forced_color` is still called only from the `run`/`watch` sites — its own doc comment still says "`run_watched` always pipes", which is no longer the only thing that pipes. `build`, `test`, `clean`, `fmt`, `check` and `mvn` hand Maven a pipe with `-Dstyle.color` at `auto`, jansi sees no tty, output goes monochrome. This is the trap `CLAUDE.md` documents for `run`, reintroduced at the other six call sites | `src/run.rs`, `run_inherited` / `forced_color` | one line: call `forced_color` when `is_maven` |
| 2 | **The metric label and the enum disagree.** `counter(claim.kind() == Kind.POLICY ? "robots" : "page")` — the arm was renamed, the label was not | `templates/spring/http_workflow_java.java:266` | pick one; see §20.2 on which |
| 3 | **`read_and_tee`'s tail is O(n) per chunk.** `captured.drain(..excess)` memmoves the whole 4 MB buffer for every 8 KB chunk once the cap is hit — a 100 MB `mvn -X` log does tens of GB of memcpy | `src/process.rs`, `read_and_tee` | `VecDeque`, or keep a chunk list and concatenate at the end |
| 4 | **`let Ok(read) = reader.read(..) else { break }` swallows `ErrorKind::Interrupted`**, which would silently truncate a build's output on a spurious EINTR | `src/process.rs`, `read_and_tee` | retry on `Interrupted` |
| 5 | **`doctor` now has two property readers with opposite semantics.** `property_value` takes the **last** match; `port_check` takes the **first**. `.properties` is last-wins, so `port_check` is the wrong one | `src/doctor.rs` | route `port_check` through `property_value` |
| 6 | **`doctor` never checks the port it now generates.** jails writes `management.server.port=8081` and `why` has a rule for "Port 8081 was already in use", but `port_check` still probes only `server.port`. `doctor` exists to catch what `why` explains afterwards | `src/doctor.rs`, `port_check` | probe both |

### 20.2 The vocabulary test is a leaky sieve, and it broke a contract anyway

`tests/genericity.rs::word_offsets` lowercases the whole text **before**
matching, then demands a non-alphanumeric, non-`_` boundary on both sides. So it
catches a bare `workspace` and misses `workspace_root`, `PaymentService`,
`crawler`, `payments`, `inboxes`, `tenantId` — that is, it misses exactly the
camelCase and snake_case compounds that domain vocabulary actually arrives as,
which is most of the Java in `templates/`.

The consequence is on disk right now: `src/project.rs` still has
`workspace_root`, `roots_to_workspace` and `maven_command(workspace_root)`,
untouched, while `"workspace"` in the JSON output *was* renamed to `"reactor"`,
taking `schema_version` 1 → 2. **The test forced a breaking change to the public
contract and left the identifiers behind it.** The struct field is `reactor` and
its parameters are `workspace_root`; that is less consistent than before the
change. `reactor` is the right Maven word and nothing consumes the JSON yet
(jails.nvim does not read it), so the break is cheap — but §14 wanted that
version bump for the enriched payload and it is now spent.

Two smaller defects in the same file:

- The `assert!(!reason.trim().is_empty(), "allow-list reasons are
  load-bearing")` sits **inside** the `.any()` closure, so it runs only when a
  forbidden word is found, and `.any()` short-circuits past the rest. Hoist it
  to a standalone loop before the scan, or a second test.
- Nothing asserts an allow-list entry is still **used**. Rename
  `http_workflow_java.java` and the entry becomes a silent hole. `ALLOWED`
  should be checked for staleness the way a reason is checked for emptiness.

**And the word list is the wrong list.** `payment`, `merchant`, `spider` and
`inbox` are domain nouns. `workspace`, `reconcile` and `conversation` are
ordinary engineering words a showcase app happens to use — `reconcile` is what
`app apply` literally does. Banning them by grep drives renames that cost
clarity, and the list's two blessings are backwards:

- **`workspaceId` → `tenantId`** in `scope_authorizer_test_java.java`. The
  mechanism is untouched — `ScopeAuthorizer.require(authentication, claimName,
  expected)` takes the claim name as a parameter, and the literal is a test
  fixture, not a code path. But `CLAUDE.md` names `tenant` as the word the
  `@scope` design exists to avoid, and `tenant` is **not** on `FORBIDDEN` while
  `workspace` is. The lint mandated the swap and then blessed the result.
- **`Kind.ROBOTS` → `Kind.POLICY`.** The *direction* is right and worth keeping:
  `POLICY` names the generic role in the frontier state machine and
  `ROBOTS_PATH = "/robots.txt"` keeps the concrete standard at the edge, which
  is better layering than `ROBOTS` throughout. The defect is that it is
  **half-applied** (§20.1 item 2), and the reason it is half-applied is
  structural: the allow-list is scoped **per file**, so the template may say
  `robots` and the SQL that `spring.rs` generates for the same concept may not.
  Scope the allow-list per **concept**, or accept the word in both places.

**Do not respond to any of this by growing `FORBIDDEN`.** The fix is a
compound-aware match (split camelCase and `_` before comparing) and a word list
pruned to actual domain nouns. A longer list matched by a leakier rule is how
this test becomes a rename generator.

### 20.3 Two tests got weaker, and the §5.2 default ships unproven

- `security_test_java.java` moved from `@SpringBootTest` +
  `@AutoConfigureMockMvc` to `@WebMvcTest(controllers = SecurityConfigTest.class)`
  + `@Import(SecurityConfig.class)`, and its assertion went from
  `hasStatusOk()` on `/actuator/health` to `hasStatus(404)` on
  `/management/health`. It no longer proves the chain **permits** health, only
  that a request reaches MVC — and passing the test class itself as
  `controllers =` reads as a typo. The move was forced (with the management port
  split off, MockMvc genuinely cannot reach the endpoint), but the option that
  keeps the proof is `webEnvironment = RANDOM_PORT` against 8081.
- `actuator_test_java.java` and `prometheus_scrape_test_java.java` carry
  `@SpringBootTest(properties = "management.server.port=")` — they **switch off
  the headline §5.2 default in order to test around it**. So nothing proves
  Spring binds a second connector; `doctor::management_checks` reads back the
  same string jails wrote, which is tautological verification. One real
  two-connector test would close it, and it is the only thing standing between
  §5.2 and being genuinely proved rather than merely generated.

### 20.4 Two Hikari values §5.1's own rule does not justify

§5.1 says every generated default exists because of a specific silent failure.
Eight of `add db`'s ten new properties clear that bar — `pg_is_in_recovery` in
particular is the best line in the change. Two do not:

- **`max-lifetime=60000`.** Sixty seconds against Hikari's thirty-minute
  default: twenty connections recycled every minute, each one re-running the new
  `connection-init-sql` round trip, forever. Which silent failure does that
  prevent in a jails app? The real system presumably has a failover or proxy
  window that makes it right there; copying the number without the reason is the
  cargo-culting §5.1 was written against.
- **`connection-timeout=1000`.** Defensible, and tight enough to be worth
  watching: one second to obtain a connection is fine on localhost and marginal
  against a Testcontainers postgres under podman on a loaded machine. If the
  real-toolchain tier starts flaking, this is the first thing to raise.

(`initialization-fail-timeout=1` reads alarming and is not: it bounds the
fail-fast *window*, so a single successful attempt against a live database
passes. Leave it.)

### 20.5 What is right, so nobody reverts it

`why` on every Maven failure is the correct design and better than §10.1 asked
for — `is_maven_program` covers all six `mvn`/`mvnw`/`mvnd` spellings, and an
unrecognised failure says so and points at `doctor` instead of dumping a raw
log. The tee threading model itself is right: two threads, flush per chunk,
bounded tail. `java::masked` (memcpy plus blanked ranges, rather than filling a
buffer with spaces byte by byte) is a real win, and `without_literals` is a
genuine new capability that stops `notes` being fooled by `"TODO"` inside a
string. `enclosing_test_method` runs `java::blanked()` first, so a `@Test` in
Javadoc cannot promote the method below it — the trap `CLAUDE.md` warns about,
avoided. `is_test_annotation` matching the annotation's **last** segment is a
real bug found and fixed: jails' own generated ITs carry fully-qualified
annotations, and prefix matching missed every one. And the §5.2/§5.3 properties
carry their reason as a comment beside the value, which is what let §5.2 and
§5.3 be cut from this file rather than merely marked done.
