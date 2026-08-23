# plan.md — what is left to build

Re-audited 2026-08-22 evening against **`38c3dc6`**, three commits past the
`ae63145` this file was trimmed at. Everything that shipped has been cut: this
file is only pending work, the standing rules that constrain it, and the
evidence each item is real. **What the code already is, and the traps in it,
live in `CLAUDE.md`; the user-facing surface is `README.md`.**

**Most of §17 closed in one push.** `b8e9be1`, `fffab7e` and `38c3dc6` are 220
files and +8,126/-2,094, and they landed `g field`, `g factory`, `--timestamps`,
`requests/*.http`, `.jails/files`/`.jails/version`/`.jails/intents`, `add cors`,
`add coverage`, `add loadtest`, `add k8s`, `jails lint`, `new --offline`,
`app init`, `mise.toml` and `AGENTS.md` from `new`, the §5.4 enforcer rules, the
enriched `about --json`, line numbers on `Route`/`Bean`, and two editor files.
§17 is rewritten around what is left. §20 is the review of that push, now
almost entirely closed.

**A further round is in flight and this file does not describe it.** As of
18:22 the working tree has uncommitted edits to `src/generate.rs`, `src/add.rs`
and `src/main.rs`, new untracked `src/model/` and `patterns/` directories, and
**does not compile** (22 errors, `config`/`base_package` out of scope around
`generate.rs:1847-1922`) — a mid-refactor state, not a broken commit. Re-audit
before trusting any line here. `abstract.md` (657 lines) is now **tracked** — committed in `7e92586`
alongside `src/model/mod.rs`. It is the third document beside this one and
`CLAUDE.md`, it says what the code *should have been*, and §6/§21 defer to it
where they overlap.

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
| **Easier** | commands from empty directory to passing gate | **1** (`jails new-cli <name> --app <manifest>`) | **1** — reached |
| **Cheaper** | manifest lines per app | 65 (A), 263 (B) | falls as generators absorb repetition |
| **Cheaper** | generated lines per manifest line | ~18× for one scaffold | rises |
| **Faster** | full gate wall time | **293 s** | container reuse is not the lever it looked like (§7); §10.2 is |
| **Faster** | edit → test result | **60–100 ms** (`jails testd`, measured; 3,810 ms was `mvn`, 620 ms is `mvnd`/`--fast`) | reached — the remaining lever is `--affected` on large suites |
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

One `g scaffold` writes **1,180 lines in 39 ms**. Adding one field used to be
6 files, ~17 edit sites and a hand-written migration.

**That asymmetry is closed.** `g field`, `g factory`, `--timestamps` and
`requests/*.http` all shipped in `b8e9be1`..`38c3dc6`, and `g field` refuses
rather than clobbers: it rewrites the derived files that still match what jails
would have written and prints snippets for the rest. What survives is the
*manifest* case — editing a `fields` line in `.jails/app.toml` still changes the
state key, so the edited intent arrives as pending against files that exist
(§9.7, §11.1). The primitive is there; the reconciliation on top of it is not.

| Change shape | Before | Now |
|---|---|---|
| Add a field to a resource | 6 files, ~17 sites, + a migration | `g field` |
| Model first (`g record`, then scaffold) | blocked; retype every field | recorded models under `.jails/models/` feed `scaffold` |
| `created_at`/`updated_at` | typed per table; `updated_at` never updates | `--timestamps` |
| Test data for a new test | `new` a 6-component record; +1 breaks 40 call sites | `g factory` |
| Change a field in `.jails/app.toml` | **fails on a path collision** (§9.7) | regenerate + 3-way merge; conflict markers only where you edited the same lines |

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

| Open clause | State |
|---|---|
| Offline project creation | **Closed** — `new --offline`, vendored templates, explicit flag |
| Provenance / drift repair | **Closed** — the path set is recorded (§11.2) *and* the content merge landed (§11.1), verified on the §9.7 case |
| Atomic whole-manifest `ChangeSet` | Open — §11, and `abstract.md` rung 3 re-prices it downward |
| Execution of the generated hosted CI workflows | External — keep hosted CI a required check |

### 4.3 App C — the payments gateway

Shipped. The manifest is `examples/payments-gateway/.jails/app.toml`; its
acceptance contract is in `examples/ACCEPTANCE.md`. **It is the app §5 answers
to** — every row there is something the real payments system does and a
jails-generated app still does not.

What C exposed is now closed: CORS (§13.1), `--timestamps` (§9.5) and the
idempotency receipt (§13.3) all shipped. The receipt was the last of the three
and the most interesting, because the gap was easy to mistake for solved: a
`@unique` column already gives one row per key, and what it withholds is the
*retained result*.

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

### 5.4–5.6 Build gates, load, deployment — **shipped**

`new` writes the `maven-enforcer-plugin` rules (`ensure_enforcer`, beside
`write_mise` and `write_agents`), so jails' most common `doctor` FAIL is now a
build-time error with a fix line. `add coverage` owns Jacoco with a stated
threshold; `add loadtest` owns the k6 script; `add k8s` owns the deployment,
service, configmap and probes. All four have golden scenarios.

**What is left here is one measurement, not a feature**: §19.6, a p99 for App C
under the k6 profile, before any performance claim is made. `jails bench` is
§17 item 5.

### 5.7 One honest counterweight — the default shipped, the checks did not

**`spring.threads.virtual.enabled: false`.** A Boot 4 payments system on Java 26
explicitly *disables* virtual threads and runs a bounded pool (`threads: 100`).

Do not read that as "virtual threads are wrong". Read it as: **a production
system with real throughput requirements made the opposite call, so jails must
not force it.** `new` now writes the property explicitly with a comment saying
the concurrency bound moves to every downstream dependency — which is the whole
point: the setting is a decision the reader can see and reverse, not a default
they inherit blind.

The first of the two `doctor` traps shipped: `doctor` warns when virtual
threads are on and `spring.main.keep-alive` is not `true`, which is the case
where an application whose only work is `@Scheduled` **exits 0 immediately**,
logging nothing, and looks on Kubernetes like a crash loop with no crash.

**Still to build:** nothing here needs a check, but record the fact so nobody
writes the wrong one — pinning is observable via the JFR
`jdk.VirtualThreadPinned` event, on by default at 20 ms, and **not** via
`-Djdk.tracePinnedThreads`, which no longer exists on JDK ≥ 24.

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
| 2. ~~**The destroy path list**~~ | ~~`generate::KIND_FILES`~~ — **deleted.** `destroy` reads the record, and where there is none recomputes through `generate::artifacts_for`, the same function `generate` writes from | **gone**: one list, and `tests/agreement.rs` proves it in both directions |
| 3. The golden scenario | `tests/common/scenarios.rs` `SCENARIOS` | complete, and a test keeps it so |
| 4. ~~The editor lists~~ | ~~four Lua tables in `jails.nvim`~~ | **gone** — derived from `jails commands --json`, which is derived from clap. `tests/editor.rs` now asserts the tables have *not* come back |
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
  That half shipped, and `destroy` now reads the recorded list.
- **Correction, from `abstract.md` §4.3.** The bullet above prices lazy
  rendering as a *cost* of this option. It is not a cost, it is the **cause**:
  generators hold `root` and therefore do I/O *while rendering*
  (`pom::read` inside `usecase_files`), so a path cannot be computed without a
  body, so `KIND_FILES` had to be typed by hand. Introduce Parameter Object
  first and laziness stops being something to pay for.
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

### 6.4 Recommended path — **superseded by `abstract.md` §7**

The ordering that used to be here (B+D, then C, then E, then F) was written
before `abstract.md`, and `abstract.md` §7 is a better sequence for the same
work: eleven rungs, each byte-checked against the golden suite, each with a
falsifiable gate that says when to revert it. Its argument for going first at
`Project`/`Layers` rather than at the artifact builder is sound — B and D are
cheap *after* rung 1 and awkward before it, which is the same point §6.2 B's
correction above makes.

**Read `abstract.md` §7 as the sequence and this section as the option
catalogue.** Keep F last in both.

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
| 2 | "change what the generated code *looks like*" | `.jails/templates/` then `~/.config/jails/templates/`, resolved before `include_str!` defaults | **shipped** (`src/template.rs`) |
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
`unowned_properties`. **Shipped**, and with one addition the sketch did not
have: the override is checked against the built-in's placeholder set, because
the third possible behaviour — quietly falling back to the built-in when the
override does not fit — is the worst of the three. The reader's file is ignored
and the build is green.

There is no `--template-dir` flag. A flag would make the override set depend on
how the command was typed, which is exactly the property that makes a generated
tree unreproducible; a directory in the project is a fact about the project.

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

### 9.1 `g field` — **shipped**

`jails g field Payment settledAt:instant?` reads the record, refuses a
duplicate component, appends in declaration order, then rewrites only the
derived files that still match what jails would have written and prints
snippets for the rest. **The refusal is the design** — the ownership oracle
re-renders the template and diffs the bytes, so it over-reports after a jails
upgrade, and over-reporting prints a snippet you paste while over-writing
destroys work. Print, never clobber. `--remove` is deliberately not in v1:
dropping a column is a data decision.

What it does **not** yet close is the manifest case — see §11.1 and §17 item 1.

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
them in `spring.rs`).

**Mostly closed.** `g repo` no longer uses `unwrap_or_default()` — it goes
through `fields_from_spec_or_record` like `dto` and `scaffold`, so all three
share one rule and one refusal. `usecase`, `query` and `transition` now share
`spring::Target::read`, which states the rule once and carries the `fix:` line
they used to word individually (or omit). `g scaffold` does read the record, via
`fields_from_spec_or_record`.

**Done.** `durable-job`, `association` (both the child and the parent read) and
`outbox`'s target read all go through `Target::read` too, so every generator
that resolves a `--on`/`--yields` resource raises the same sentence and the same
`fix:` line. The one deliberate exception is `outbox`'s *event* read, which
resolves in the messaging layer rather than the domain and refuses for a
different reason ("generate the typed event first").

So model-first is blocked on the kind spanning the most files, while eight
newer kinds *require* it. State the rule once: **spec if given, else the record
on disk, else an error naming the record and the fix.**

### 9.5–9.6 `--timestamps`, `g factory`, `requests/*.http` — **shipped**

`--timestamps` is a flag on the field-taking generators and owns the DDL and
adapter side, where the lie used to live (`updated_at` as a column nothing
updates). `g factory` builds defaults from `sample_value`; a component jails
cannot sample starts `null` and `build()` throws naming it, never a guessed
default. `requests/<resource>.http` is a `g scaffold` side artifact.

**Refusals now carry `fix:` and a test enforces it** —
`field_driven_generators_refuse_an_absent_model_with_a_fix` in `tests/cli.rs`
holds generators to the standard `doctor` was already held to.

### 9.7 The manifest is the ergonomic unit, and editing a field breaks it

`app.rs`: `plan` runs `add::preflight` and writes nothing; `apply` installs
capabilities, runs each pending intent, **writes state after every one** (so an
interrupt resumes), then **reconciles every capability a second time**.

The gap **was** the state key — `kind|name|package|fields|indexes|on|yields`.
Change a `fields` line and you change the key, so the old intent stayed in state
and the edited one arrived *pending*; `apply` called `generate`, which found the
files and refused.

**Fixed by §11.1 rather than by changing the key.** The old intent in state is
precisely copier's `last_answers`, so `apply` regenerates *both* intents into
temp dirs, diffs them, and merges the patch onto the project. The state key is
still argument-derived and that is now harmless: it is what makes the old
intent recoverable. Verified end to end — an added component merges clean and
compiles; against a hand-edited file it leaves conflict markers and says so.

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

**Nothing is left in this subsection.** `mise.toml` from `new` shipped with
`AGENTS.md` and the enforcer rules in `38c3dc6`; an earlier revision of this
line said otherwise and was wrong.

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

**Step 2, `jails testd`** — **shipped, and simpler than this sketch.** One
resident JVM over a unix socket, `src/testd.rs` plus a single Java template
compiled by `java`'s source launcher at start-up. Measured: **0.06–0.10 s for
one test method against 0.62 s**, and 0.27 s against 0.96 s for 151 classes.

Two departures from what was planned here, both from measurements taken since:

* **No in-process compiler.** §19.5 measured that the editor's language server
  already writes `target/classes` on save, so the compile is being done by
  something holding the whole project's model rather than one changed file —
  and the correctness note below says compiling one file is unsound. The
  daemon runs what is on disk and refuses a stale class through the same gate
  `--fast` uses.
* **No hand-rolled session or classloader.** `ConsoleLauncher.run` with
  `--class-path` naming only the output directories makes JUnit build the
  child loader and close it per run, so freshness is JUnit's semantics rather
  than jails'. The consequence is a rule that has to be kept: the daemon's own
  classpath carries the *dependencies only*, because a copy of the outputs up
  there would be served parent-first and the daemon would be green over
  deleted code, silently and forever.
**That classloader cost is measured** (§19.2): 0.04 ms to construct, ~0.5 ms per test class to
populate, against the 0.6 s of cold `java` it removes. Not the obstacle.

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

**None of the above was built, and that is the finding.** §19.5's check was
taken and it holds here: jdt.ls writes `.class` files straight into
`target/classes`, `jails new` already installs devtools, and it already writes
`META-INF/spring-devtools.properties` cutting Boot's 1 s + 400 ms down to
200 ms + 50 ms. So `:w` → class written → restart, with no watcher, no `javac`
invocation, no JVMTI and no supervisor process. The whole of §10.3 above is
work that a language server and one dependency were already doing.

**Shipped in its place:** the README's *save-and-reload loop* section, and
`doctor::wiring::hot_reload_checks`. The checks are the part that needed
building, because every way this loop breaks is silent — no devtools at all,
`spring.devtools.restart.enabled=false`, or a
`spring.devtools.restart.trigger-file`, which is the cruel one: the recompiled
class **is** seen and then deliberately ignored, so the loop reads as dead
rather than as configured. The two limits stay stated rather than engineered
around: a record component, `sealed` hierarchy, annotation or signature change
is a restart and jails' domain layer is records, and `jails check` remains
`mvn clean verify` because an incremental compile cannot see that a deleted
method left a stale caller.

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

1. ~~**`g field` first** (§9.1)~~ — **shipped.**
2. ~~**`.jails/files` + `.jails/version`** (§11.2) — the path set.~~ **Shipped**,
   and since folded into one `.jails/ledger.toml` (`src/ledger.rs`): the path
   set, the field specs and the applied-intent registry were three layouts
   recording one entity, and two of them were keyed differently.
3. ~~**Regenerate and 3-way merge** (§11.1) — the content merge.~~ **Shipped.**
   `reconcile_intent` in `src/app.rs`: regenerate old and new from the stored
   intent, `git diff --no-index`, `git merge-file`. No version pinning, no
   ownership ledger — exactly copier's shape, and cheaper here because for the
   §9.7 case the *generator* is unchanged and only the intent differs.
4. **Then** one atomic plan, if it still earns its keep.

Keep: paths normalised and confined; all conflicts detected before the first
write; `--pretend` and apply rendering the **same** object; expected hashes,
not string matching; a second identical apply a no-op. **A sequence of per-file
renames is not an atomic transaction** — promise deterministic preflight plus
crash recovery, and say so.

**Structural note — fixed.** `write_new_file` was *not* the single choke point
it looked like: `src/add.rs` wrote an existing path directly with `fs::write`,
bypassing the collision check, so a ledger hung off `write_new_file` alone had
a hole exactly where a capability updates a file it previously wrote.
`src/apply/` is now that choke point, with `fs::write` banned everywhere else
by `tests/architecture.rs`, and the four verbs distinguish *what the caller
believes is already there* rather than leaving it to convention. Naming the
belief immediately paid: `create` surfaced a latent double-write of
`package-info.java` that the old silent overwrite had hidden.

**`src/codemod.rs` — shipped**, and narrower than proposed, which is the
finding. The primitives listed here do not share an implementation: a pom
splice, a dispatcher registration and an `@Import` merge have nothing in common
below the surface. What they *did* share was one format with **five owners** —
`compose.rs`, `add.rs`, `add/database.rs`, `add/test_wiring.rs` and `doctor.rs`
each built and parsed `# jails:<marker>` … `# /jails:<marker>` with their own
`format!`, which is `process.rs` before extraction and with the same
consequence waiting.

So `codemod.rs` owns the marked block and nothing else: render, present-in,
body-in, strip-from. `tests/architecture.rs` fails on a `# jails:` literal
outside it, so a sixth owner cannot appear quietly. There is deliberately no
`replace`, because nothing needs one — Metz's rule about speculative
abstraction applies to a vocabulary as much as to a type.

The rest of the list stays where it is. `pom::add_dependency` belongs with the
pom, `register_command` with the dispatcher it knows the shape of. Collecting
them by *what they are called from* rather than by what they know would be the
temporal decomposition `abstract.md` §3.2 names.

**`new --offline` shipped** — `templates/new/offline_{pom.xml,application,application_test}`
vendored via `include_str!`, behind an explicit flag, so a start.spring.io
failure suggests it rather than silently falling back. `app init` shipped
beside it. What is still missing is `jails new <name> --app <manifest>`, which
is what collapses the four-command runbook to one (§18, §17 item 10).

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

No version pinning, no ownership model. `.jails/ledger.toml` already stores the
old intent — that *is* `last_answers`. And `--pretend`
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

### 11.2 The path set — **shipped, and since folded into one ledger**

`.jails/ledger.toml`, written by `src/ledger.rs` and reached through
`src/generated_files.rs`'s five verbs, read by `destroy`. Sorted,
`/`-normalised separators, **one path per line** — the two details from
`openapi-generator`'s `DefaultGenerator` that make the file diffable and stable
across machines, and the reason `files = [...]` is rendered multi-line rather
than as one long array.

It began as four files (`files`, `version`, `intents/*`, `models/*`); those are
one `[[applied]]`/`[[model]]` table now, keyed on `(recipe, name, package)`.
See `abstract.md` §4.5 — the fold is what closed §9.7.

This is the half that says **what** was written. §11.1 is the half that says
what to do when it has changed underneath you, and it is still open.

---

## 12. Tier 4 — reach: the codebase you did not create — **shipped**

Both halves landed. `src/build.rs` is the marker widening (and the refusals);
`src/adopt.rs` is `jails adopt`. Two things the sketch below did not have:
`generate` states which shape a missing pom chose *and* names the dependencies
it could not splice, and `doctor` **stops** after naming the build tool rather
than running fifteen checks against a pom that is not there — the same failure
§8.9 names, in a new disguise. What follows is the original argument.

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

### 13.1 `add cors` — **shipped**

`cors_config_java.java` plus its test, with `.cors(...)` wired into the
generated chain in the same change, the methods named explicitly rather than
`applyPermitDefaultValues()` (which permits only GET/HEAD/POST and no
credentials — the classic "works until mark-as-read becomes a PUT"), and
origins in a marked properties block.

### 13.2 `add sse` — **shipped**, and there were five

All four below were confirmed in `deps/` before a line was written, and the
generated `EventHub` states each one where the code makes the choice.
`src/spring/sse.rs`.

**The fifth turned up only because the generated test was run against real
Maven**: `ResponseBodyEmitter.complete()` sets a flag and forwards to a handler
the container installs when it takes the emitter, so **outside a request the
completion callbacks never fire** and `onCompletion` removes nothing. That is
why the hub exposes `unsubscribe` as real API — genuinely needed by any caller
that learns the client has gone by another route — rather than leaning on
`onCompletion` alone. Two of the four tests failed on the first run for exactly
this, which is the case for generating tests that are behavioural rather than
structural.

The original four:

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
| ~~**`g idempotency`**~~ — **shipped** | A `@unique` column gives one-row-per-key but **not the retained result**; a retry got a 409 instead of the original response. Four outcomes now: first call runs it, a matching retry replays the stored response, the same key with a *different* request is refused, and a retry while the first attempt is in flight is told to retry rather than handed a null body | C |
| ~~**`g auth`**~~ — **shipped** | Both confirmed in `deps/`: zero occurrences of `JwtEncoder` in all of Boot, and `JwtTimestampValidator.allowEmptyExpiryClaim = true` by default. The generated config undoes the second in one line, and `a_token_with_no_expiry_is_refused` is what keeps it — verified by deleting the line, which fails that test and only that test | B, C |
| ~~**`g webhook`**~~ — **shipped** | All three, plus a fourth the sketch did not name: the timestamp is checked in **both** directions and is **inside** the signature. Rejecting only stale timestamps leaves a far-future one accepted -- the same replay window with its sign flipped -- and a timestamp outside the signed bytes is a header anyone in the middle can rewrite. Seven generated tests, all passing against real Maven. `http-sink`'s `webhook` alias became `outbound`, since the name means the receiving end far more often | B |
| ~~**`add mail`**~~ — **shipped** | Both, and the send-and-read-back path was run against a live Mailpit rather than only compiled — every failure this test exists to catch compiles. Two defaults made explicit that the sketch did not name: `spring.mail.host`, whose absence falls back to `localhost:25` and fails at the first send rather than at startup, and the From address as one configured value. There is no `@ServiceConnection` for mail in Boot 4 (no `MailConnectionDetails` exists), so the IT binds host and port with `@DynamicPropertySource` | B |
| ~~**`g search`**~~ — **shipped** | Exactly that, and verified against a live PostgreSQL rather than reasoned about: the migration applies, a search matches, `websearch_to_tsquery` does not error on `it's "a" -- fine`, and after an `UPDATE` that changes the body the row stops matching the old text with nothing having to remember to reindex. Three details the sketch did not name: `coalesce(x, '')` around every column (`\|\|` with NULL yields NULL and would blank the vector), the configuration named in the expression rather than left to a session setting, and `websearch_to_tsquery` over `to_tsquery`, which throws on a bare two-word phrase | B |
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

**The prerequisite shipped.** `about --json` is at `schema_version: 3` and
carries `base_package`, `java_root`, `test_root`, `layout` (through
`Config::layers()`, i.e. *renamed* values) and `capabilities`. `Route` and
`Bean` both carry `line`, so `routes --json` is a quickfix list rather than a
flat one. `compiler/jails.vim` and `after/ftplugin/java.lua` landed too.

What is left is the part that is not Rust: jdt.ls settings and bundles
(`updateBuildConfiguration = 'automatic'`, java-debug, vscode-java-test),
projectionist wired off `about --json`, `fzf-lua` pickers over
`routes`/`beans --json`, `jails src`, and the `<leader>j`/`<leader>J` keymap
split — most of which lives in the dotfiles repo, not here. One tidy-up that
does belong here: **three JSON version spellings** (`about`'s
`schema_version`, `routes`/`beans`' `version`, `why --json`'s envelope
`version`).

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

### 15.1 `AGENTS.md` — **shipped**

`jails new` writes one (`write_agents`, beside `write_mise` and
`ensure_enforcer`), and `src/lint.rs` shares the table it is rendered from, so
the banned-API list cannot drift into a lie the way a hand-written one would.

### 15.2 The rest

**`jails lint` shipped** — `src/lint.rs`, a closed rule table over the
stale-API families jails already knows, sharing its table with the generated
`AGENTS.md`.

**`--json` everywhere — done.** Nine commands have it: `about`, `routes`,
`beans`, `why`, `commands`, `doctor`, `stats`, `notes` and `test`.

Two details worth not re-deriving. `doctor --json` renders the *same*
`Vec<Check>` the human report prints rather than re-deriving it, so the two
cannot describe different runs — the same reason `--pretend` and apply have to
consume one value. And `test --json` reports `passed` from the **build's own
verdict**, not from "no failed cases": a build can fail before a single test
runs, and an empty failure list would then read as success. The `cases` array
is what distinguishes "all green" from "nothing ran".

**`jails commands --json` shipped, and it deleted the Lua lists rather than
pinning them** (§6.2 F's argument, applied early because it needed no
descriptor format): subcommands, generator kinds, capabilities and per-command
flags, all walked out of the same `clap::Command` that parses the arguments.
`jails.nvim` lost 160 lines of hand-maintained tables and reads the payload
once per session, degrading to an empty menu on any failure — an older binary,
`jails` off PATH, a malformed payload — because a completer that raises inside
a keystroke handler is worse than one that offers nothing. The derived output
already carries flags the hand-written table had missed, `--timestamps` among
them.

**`jails explain <kind>` shipped**, and the `@Repository` case is its first
entry: the Javadoc explains the asymmetry to whoever reads the file, which is
the wrong reader for someone deciding *whether* to generate, and for an agent
that sees one annotated adapter and "fixes" the other into an ambiguous bean.

It is a hand-written table, and that is a sixth copy of "what does kind X
mean" by §6.1's count — so it is held to `why.rs`'s shape, the one
`abstract.md` §2 singles out as the only clean concept here: a value in a
table, one edit per instance, with `every_kind_has_an_explanation` failing the
build when a kind is added without one. Two further tests refuse an empty body
and a body that merely restates the summary.

**Promote `g cases`** — it turns a markdown brief's acceptance bullets into a
test class, and README mentions it once.

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

**Closed since the morning trim** and removed rather than struck through:
`g field`, `g factory`, `--timestamps`, `requests/*.http`, the recorded path set
(`.jails/files`, `.jails/version`, `.jails/intents/*.files`,
`.jails/models/*.files`), `add cors`, `add coverage`, `add loadtest`, `add k8s`,
`jails lint`, `AGENTS.md` and `mise.toml` from `new`, `new --offline`,
`app init`, the §5.4 enforcer rules, the enriched `about --json`
(`schema_version: 3`, with `base_package`, `java_root`, `test_root`, `layout`
and `capabilities`), `line` on `Route`/`Bean`, the showcase-vocabulary test, the
§5.2/§5.3 defaults with their `doctor` checks, `why` on every Maven failure and
`why --json`, and the §5.7 `spring.main.keep-alive` trap.

**The authorship debt is paid.** That was the front of the queue this morning
and it is gone: adding a field is one command, a factory is one command, and an
edited derived file is reported rather than clobbered. What is left is
maintainability, latency, reach, and the capabilities no proof app has demanded
yet — which is a healthier list than the one it replaces.

| # | Item | § | Effort | Proves |
|---|---|---|---|---|
| ~~1~~ | ~~**Finish drift repair: regenerate + 3-way merge.**~~ — **done and verified end to end.** `reconcile_intent` regenerates the old intent and the new one into temp dirs, diffs them, and `git merge-file`s the patch onto the project. Editing a `fields` line in `.jails/app.toml` now merges cleanly and compiles; against a **hand-edited** generated file it leaves `<<<<<<<` markers and a `fix:` line rather than clobbering — which is §11.1's stated, correct outcome. No git repo is a refusal naming `git init` | §11.1 | M | A B C |
| ~~2~~ | ~~**§6.5 — split the file.**~~ **done, and extended.** `abstract.md` rung 11 went another round: `generate.rs` 1,813 → 933 production lines (`generate/write.rs`, `generate/scaffold.rs`, `generate/remove.rs`), `spring/workflow.rs` 1,374 → 615 (`transition.rs`, `query.rs`), `doctor.rs` split by *who is being asked* (`environment.rs` the machine, `wiring.rs` the project). Largest module 2,379 → 1,262. Originally: §6.2 C is done: all 39 inline `format!` Java blocks are `templates/spring/*.java`, goldens byte-identical, and `spring.rs` is 5,517 raw lines / ~3,880 of decisions. §21.1's parameter problem is **also done** for this file — no function takes over five parameters, because `spring::Slice` made placement a value. `src/spring/` now holds `workflow.rs`, `durable.rs`, `http.rs` and `schema.rs`; `spring.rs` is 6,624 → ~1,900 lines. What is left of §6 is **§6.2 E**, the type table as data | §6, §21.1 | S remaining | — |
| — | **§4.4 / `abstract.md` rung 7, schema half** — **done.** `on`/`yields` are the manifest keys, `strategy_on`/`strategy_yields` are deprecated aliases, both spellings under one reference is an error, and all four proof-app manifests are migrated (64 keys) with every `app_manifest*` test green | §4.4 | S | A B C D |
| ~~3~~ | ~~**§6.2 B + D** — the artifact builder~~ — **done.** `generate::artifacts_for` is the query (`abstract.md` rung 4) and `destroy` recomputes through it (rung 5), so `KIND_FILES` and `NO_FILE_TABLE` are gone: −1,017 lines from `generate.rs`, which took the largest module from 2,379 production lines to 1,813. The record still wins where there is one (§11.2 unchanged); recomputation is the answer for a project that predates `.jails/`, and the six kinds it cannot answer for are declared in `tests/agreement.rs` with the argument no generic shape can guess | §6.2 | 1 day | A B C D |
| ~~4~~ | ~~Scaffold **refuses** an unmapped project-typed component~~ — **done.** The teaching refusal (read the referenced record's stored `@pk`, name the two commands that do the job) was already written but shadowed by a generic refusal raised earlier in `scaffold_artifacts`; the generic one is gone and all four persistence refusals now name the offending component as `name:Type` and carry a `fix:` line | §9.2 | S | B C |
| ~~5a~~ | ~~`jails test --fast`~~ — **shipped, and measured before being claimed.** Console launcher over the compiled classes, with the JUnit version derived from the project's own (a guessed pin resolved fine and died with `NoSuchMethodError` — `junit-bom` constrains every artifact to one version, confirmed in `deps/junit-framework`). Falls back loudly whenever a source is newer than the classes, nothing is compiled, or the run needs Surefire's XML. **§19.1's measurement says it does not beat `mvnd`**, so it is documented as the no-mvnd path and as step 2's substrate, not as a win | §10.2 | M | D first |
| ~~5b~~ | ~~`jails bench`~~ **shipped**, and deliberately thin: it states the load profile, refuses without `add loadtest` or without k6, and runs k6. **It does not parse k6's output** — k6 prints p95 and p99 itself and its own thresholds already decide pass or fail, so a parser here would be a second answer to a settled question. The sharper reason is this repository's own rule: **k6 is not installed on this machine**, so a parser would be written against a format nobody had seen. §19.6 therefore remains **unmeasured**, and that is stated rather than implied away by the command existing | §5.4–5.6 | S | C |
| ~~6~~ | ~~`g idempotency` — the retained-result receipt.~~ **done.** Receipt record, store port, PostgreSQL adapter, guard, unit test and migration. The claim is one `insert … on conflict do nothing returning`, because select-then-insert leaves the race the mechanism exists to close. Five generated tests run and pass against real Maven | §13.3 | M | C |
| 7 | Editor: jdt.ls settings + bundles + HCR, pickers, projectionist, keymap split. **The two pieces that live here are done**: `jails src <Type>` (project sources, then `JAILS_SOURCE_PATH` or `deps/`; lists every match rather than picking, because three `Status.java` files is ordinary), and the JSON version spellings — nine emitters said `schema_version` or `version`, they all say `schema_version` now, with per-payload numbers, and `tests/architecture.rs` fails on a tenth spelling. The rest is `~/code/my-dotfiles`, which this repo's git history does not track | §14 | done here | — |
| — | **`abstract.md` rung 6 + §4.2's deriving half** — **done.** `src/apply/` is the only module that writes (`fs::write` banned elsewhere by `tests/architecture.rs`), and `doctor::capability_drift_checks` re-plans every recorded capability through `add::plan_for`, closing a drift class that had no test | §11, §4.2 | M | A B C D |
| ~~8~~ | ~~`--json` everywhere; `jails explain <kind>`; `jails commands --json`~~ — **done.** `commands --json` (which deleted 160 lines of Lua tables), `doctor --json` (the same `Vec<Check>` the human report prints), `stats --json`, `notes --json` (file/line/tag/text, a quickfix list), `test --json` (read from Surefire's XML, the same source `--failed` and `--slowest` use, so the three cannot disagree about what ran), and `jails explain <kind>` | §15.2 | S | — |
| — | **`abstract.md` rung 8 — one ledger** — **done.** `src/ledger.rs` and `.jails/ledger.toml` replace `app-state-v1`, `intents/*`, `models/*`, `files` and `version`. Identity is `(recipe, name, package)`, everything else content, so §9.7's edited-`fields` case is an update to a known entity rather than a new intent against existing files. `generate` and `app apply` write disjoint columns of one row through `ledger::entry_mut`. Goldens went from 98 bookkeeping files to 21, and `the_goldens_still_hold_the_properties_that_matter` fails if a third file or a subdirectory appears | §11.2 | M | A B C D |
| ~~9~~ | ~~**§6.6 Tier 2** — template overrides~~ — **done.** `.jails/templates/<name>` beats `~/.config/jails/templates/<name>` beats the `include_str!` default; all 107 template sites go through one `template!` macro so no generator has to opt in. `doctor` reports every active override by name with the reason (not golden-tested). An override is held to the built-in's **placeholder set** — a mismatch is an error naming the reader's file, not a panic naming jails' | §6.6 | S | — |
| ~~10~~ | ~~`jails new <name> --app <manifest>`~~ — **done.** `new` and `new-cli` both take `--app`, seeding `.jails/app.toml` and applying it against the project just created. Verified on App D: one command from an empty directory to `mvn clean verify` green. Needed `add::add_in`, `add::preflight_in` and `ResolvedIntent::apply_to` so nothing in the apply path reads the process CWD | §11 | S | A B C D |
| 11 | **§6.2 F** — one descriptor per kind. **Most of what it was going to buy has since been bought another way, and the headline property is already enforced**: `[golden]` was to make it "impossible to add a kind without a snapshot test", and `every_kind_and_capability_has_a_golden_scenario` does that today — verified by deleting the `search` scenario, which failed with *"1 thing(s) jails can generate have no golden scenario: kind `search`"*. `destroy`'s paths are derived from the generator now (rungs 4–5), which cannot drift from it at all, where a descriptor still could; the Lua lists are gone, deleted by `commands --json`. What is left unique to F is generating the `ArtifactKind` enum, clap aliases, `--help` and the README table from one file — real, but a `build.rs` and a week for the smaller half of the original case | §6 | L, and re-price it first | — |
| ~~12~~ | ~~§12 marker widening + `jails adopt`~~ — **done.** `src/build.rs` names the build tool without reading it; `find_project_root` takes any recognised marker, nearest wins; ten Maven-inherent commands refuse through `require_maven` naming what still works; `generate` states which shape a missing pom chose and which dependencies it could not splice; `doctor` leads with the real build tool instead of reporting on an absent pom. `jails adopt` writes `[layout]` from a closed synonym table, reports what it does not recognise, refuses to pick between two candidates, and never touches `[project] capabilities` | §12 | M | — |
| 13 | ~~`jails testd`~~ **shipped and measured**: 0.06-0.10 s for one test method against `--fast`'s and mvnd's 0.62 s, and 0.27 s against 0.96 s for a 151-class suite. §19.2 explains it -- the first JUnit session in a JVM is 464 ms against 20 ms warm, and a cold `java` pays that every run. It **does not compile**, which §10.2's design assumed it would: §19.5 measured that the editor's language server already writes `target/classes` on save, so the daemon runs what is there and refuses a stale class through the same gate `--fast` uses. Freshness comes from JUnit -- `--class-path` naming only the output directories, so a child loader is built and closed per run -- and the daemon's own classpath must therefore *exclude* them or parent-first delegation serves the stale class silently, which an integration test pins. **`--affected` is still open** and is now the cheaper half of this row | §10.2 | L | — |
| ~~14~~ | ~~`jails dev` v1.~~ **Answered by measurement and therefore not built.** §19.5 asked where jdt.ls writes `.class` files; it writes them into the project's own `target/classes` with no Maven run, and `jails new` has been installing devtools *and* tuning its poll interval all along. So both halves of the loop already shipped and the supervisor §10.3 describes would have been a third party to a conversation two programs were already having. What shipped instead is the README's save-and-reload section and `doctor`'s `reload` check — because the machinery existed and the **diagnosis** did not, and every way this loop breaks is silent | §10.3 | L→S | C |
| ~~15~~ | ~~`add sse`~~ (§13.2), ~~`g auth`~~, ~~`g webhook`~~, ~~`g search`~~ and ~~`add mail`~~ (§13.3) — **all shipped.** What is left in §13.3 is the "build the first time a project needs one" row: `add flags`, `add shedlock`, `add storage`, `add arch`, `add nullcheck`, none of which a proof app has demanded | §13 | done | B C |
| 16 | ~~`codemod.rs`~~ **shipped** — narrower than proposed and the narrowing is the finding (§11): the primitives share a *format*, not an implementation, and that format had five owners. `src/apply/` remains the single write path. What is left is the atomic whole-manifest `ChangeSet` on top, which is the part §11's own sequence puts last and calls "if it still earns its keep" | §11 | S remaining | A B C |

Item 1 is the only one with a broken user-visible case behind it, so it is
first. Items 2–3 are the maintainability debt, and item 2 got worse in this
push rather than better — that is the cost of shipping fifteen features without
touching the file they all live in. Item 13 is still the biggest latency number
and still correctly late.

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
commands above should have been one? The first answer shipped —
`new` + `mkdir` + `cp` + `app apply` is now
`jails new <name> --app <manifest>`, and the runbook above collapses to:

```bash
"$JAILS_BIN" new-cli ledger --app ~/code/jails/examples/ledger-cli/.jails/app.toml
cd ledger && "$JAILS_BIN" doctor && "$JAILS_BIN" check
```

Ask it again of what is left. `doctor` and `check` are the next pair, and they
are *not* obviously one command: `check` writes and takes minutes, `doctor` is
read-only and instant, so folding them would cost the property that makes
`doctor` safe to run mid-debug.

---

## 19. Measure before promising

1. ~~Console-launcher wall time here (est. 0.35–0.6 s)~~ — **measured, and the
   estimate was right about the launcher and wrong about what it beats.**

   One test method, `jails new-cli` project, three runs each:

   | path | wall clock |
   |---|---|
   | `mvn -q test -Dtest=NoteTest` | 2.14 / 2.19 / 2.31 s |
   | `jails test NoteTest` (mvnd, the default) | 0.62 / 0.62 / 0.59 s |
   | `jails test --fast NoteTest` (console launcher) | 0.60 / 0.59 / 0.61 s |

   The whole suite is the same story (0.62–0.72 s mvnd, 0.67–0.77 s `--fast`).

   **So `--fast` does not beat the default here, and the plan's 2.57 s baseline
   was `mvn`, which is not what jails runs.** `run.rs` prefers `mvnd`, and the
   daemon has already removed the JVM-start and dependency-resolution cost that
   the launcher was going to remove. What is left in both is ~0.6 s of *cold
   `java` process*, which is exactly the floor step 1 cannot go below.

   Two things follow, and both are the opposite of "ship it and claim a win":

   - `--fast` earns its place where mvnd does not run — and `CLAUDE.md` records
     that this machine's mvnd is flaky under JDK 26, which is a real case. It
     is 2.2 s → 0.6 s there, a 3.6× win. It is not a default and must not be
     described as faster than one.
   - **The latency item that matters is step 2, `jails testd`** (§17 item 13),
     because a resident JVM is the only thing that removes the 0.6 s floor.
     Step 1 is now its substrate — the selector translation, the cached test
     classpath and the staleness gate are all reusable — rather than a win in
     itself.

   The resident-JVM band (est. 50–150 ms) is still unmeasured.
2. ~~The cost of a fresh `URLClassLoader` per `testd` run.~~ **Measured. It is
   not the obstacle, and the loader itself is free.** A resident JVM, a
   `jails new-cli` project, 30 iterations, two sizes:

   | | 21 test classes | 151 test classes |
   |---|---|---|
   | A construct the `URLClassLoader` | 0.04 ms | 0.04 ms |
   | B load every test class through it | 2.1 ms | 12.8 ms |
   | C JUnit session, **fresh** loader | 20.3 ms | 82.6 ms |
   | S JUnit session, **shared** loader | 6.6 ms | 11.8 ms |
   | **C − S, the price of freshness** | **13.7 ms** | **70.9 ms** |

   All medians. The premium is roughly linear at **~0.5 ms per test class**,
   and it is class loading and verification — constructing the loader is
   0.04 ms at both sizes, so the thing the question named is not the thing
   that costs.

   **The arithmetic that decides item 13 therefore holds.** §19.1 measured the
   floor at ~0.6 s of cold `java`, and a fresh loader gives that back for
   2 % of it on a small project and 12 % on a 151-class one — while being
   exactly what makes a recompiled class visible, which is the correctness the
   daemon exists to preserve. `--affected` shrinks it further, because a run
   over a subset loads only that subset.

   **The larger finding is the first iteration**, which is not in the table
   because it is not a median: 464 ms at 21 classes and 758 ms at 151, against
   20 ms and 83 ms warm. That is engine and JIT warmup, an order of magnitude
   more than the classloader, and it is precisely what a resident JVM
   amortises and a cold `java` pays every single time. The case for `testd` is
   that number, not the launcher's.
3. How many distinct Spring contexts the proof apps build (`missCount` under
   `org.springframework.test.context.cache=DEBUG`). At 293 s and 123 tests
   **this is the highest-value measurement in the list.**
4. `postgres:17` with reuse under podman, and whether `withReuse(true)`
   disturbs `@ServiceConnection`.
5. ~~Where jdt.ls writes `.class` files here.~~ **Measured. It writes into
   the project's own `target/classes`, with no Maven run.** A `jails new-cli`
   project plus one `g record`, confirmed to have no `target/` at all, was
   opened headless in nvim and left until class files appeared:

   ```
   target/classes/com/example/probe/App.class
   target/classes/com/example/probe/domain/package-info.class
   target/test-classes/com/example/probe/domain/NoteTest.class
   ```

   m2e points Eclipse's output folder at Maven's, so **the language server that
   is already compiling the file for diagnostics is compiling it to the
   directory devtools is watching.** §10.3's premise holds, and the earlier
   partial answer — an empty jdt.ls workspace — was reading the right absence
   for the right reason.

   Note the first attempt waited on `target/classes` *existing* and quit before
   autobuild had written anything, which looked like a contradiction (the
   directory was there, `find -name '*.class'` was empty). Waiting on a
   `.class` rather than on the directory is what settled it — an ambiguous
   measurement reported as an answer would have been worse than none.
6. p99 for App C under the §5.5 k6 profile, before any performance claim.
   **`jails bench` now exists to take it** (§17 item 5b) and the k6 script has
   carried `p(95)<500, p(99)<1000` thresholds since `add loadtest` shipped —
   but **k6 is not installed here**, so the number is still unmade. It is one
   `mise use -g k6` away, and until then no performance claim may be made.
7. ~~Whether `CSVFormat.Builder.build()` still exists at commons-csv 1.14.1.~~
   **Answered.** `add csv` pins 1.14.1 and `csv_reader_java.java` calls
   `.get()`; the unit test in `add.rs` binds the pinned version and the
   generated call together, and the real-toolchain tier compiles the result.
8. ~~Confirm the tier-3 skips are actually gone now `TARGET_RELEASE` is
   `"25"`, and that `doctor`'s daily false FAIL is gone with them.~~
   **Answered, and the answer is yes on both.** `JAILS_REQUIRE_TOOLCHAIN=1
   cargo test` is fully green — 482 tests, **zero skips**, so nothing in the
   suite is reporting "passed" for a tier-3 path it never ran. `jails doctor`
   on a freshly generated project reports *"10 checks, all clear"* and exits 0;
   the `jdk` check reads *"java 26 on PATH, project targets 25"*, which is the
   case that used to FAIL when the target was an unreleased 27. Re-run the
   `JAILS_REQUIRE_TOOLCHAIN=1` form before believing any green run, since a
   plain `cargo test` still reports a skip as a pass by design.

Note the payments gateway targets **Java 26**, not 25. That is a data point,
not a reason to move: 25 is LTS and everything in this plan is available at 25.

---

## 20. Review of the 2026-08-22 push — closed, with two residues

Reviewed while `b8e9be1`..`38c3dc6` was being written; **every defect below was
fixed before those commits landed**, and several were fixed better than the
review proposed. Kept as a record of why the code looks the way it does, so
none of it is re-litigated — not as pending work.

**The finding that mattered most was the one that was negative: no non-generic
logic was introduced.** Every new code path is keyed on Spring, Maven or ops
facts. What the push moved was vocabulary, and the vocabulary fixes are §20.2.

### 20.1 The six defects, and what they became

| Defect | Resolution |
|---|---|
| Colour gone from every Maven command — `run_inherited` piped for `why` but `forced_color` was only called from the `run`/`watch` sites | `run_inherited` now calls `forced_color` under the same `is_maven` guard that selects `Tee` |
| `counter(Kind.POLICY ? "robots" : "page")` — arm renamed, label not | Label is `"policy"`; the enum and the metric agree |
| `read_and_tee`'s tail was O(n) per chunk (`Vec::drain`) | `VecDeque` with the cap as its capacity |
| `read_and_tee` swallowed `ErrorKind::Interrupted` | `Err(e) if e.kind() == Interrupted => continue` |
| `doctor` had two property readers disagreeing on first- vs last-wins | `port_check` routed through `property_value`; last-wins everywhere, which is what `.properties` means |
| `doctor` never probed the 8081 it generates | Probes both, from `property_value(.., "management.server.port")` |

### 20.2 The vocabulary test, rebuilt

Every objection was taken, and the result is a better test than the one
reviewed. Recorded because the failure modes are not obvious from the code:

- **Matching is tokenised, not substring-on-lowercase.** `word_offsets` splits
  on non-alphanumerics *and* camelCase boundaries, then compares tokens — so
  `PaymentService`, `workspace_root`, `crawler` and `payments` are all caught,
  which is exactly the class the first version missed while still forcing a
  rename.
- **The word list was pruned to actual domain nouns.** `conversation`,
  `workspace` and `reconcile` are gone: they are ordinary engineering words a
  showcase app happens to use, and `reconcile` is what `app apply` literally
  does. Eight remain. **Do not grow the list** — a longer list matched by a
  leakier rule is how this test becomes a rename generator.
- **The allow-list is per concept, not per file** — `AllowedConcept { word,
  files, reason }` — so `robots` is legal in both the template and the
  `spring.rs` that generates the matching SQL. That cross-file split was what
  produced the enum/label mismatch above.
- **Both guards on the allow-list are real now**: the reason-is-non-empty
  assertion was hoisted out of the short-circuiting `.any()` closure, and a
  stale allowance (a word no longer present in the file that claims it) fails
  the test.

**Two residues, both small, neither a defect the test can catch:**

- **`project.rs` is half-renamed.** The public field is `reactor` and the
  internals are still `workspace_root` / `roots_to_workspace`. It was a lint
  artifact; now that `workspace` is not forbidden it is a plain inconsistency,
  and the right fix is to finish the rename rather than revert it — `reactor`
  is Maven's own word. (The `schema_version` burn this caused is moot: it moved
  to `3` for the enriched payload, which is a legitimate reason.)
- **`scope_authorizer_test_java.java` uses `tenantId`.** It is a hardcoded test
  fixture, not a code path — `ScopeAuthorizer.require(auth, claimName, value)`
  takes the name as a parameter. But the swap from `workspaceId` was mandated
  by a ban that no longer exists, and `CLAUDE.md` names `tenant` as the word
  the `@scope` design exists to avoid. Pick a claim name that is neither.

### 20.3 The two weakened tests, and how they came back stronger

Both were fixed past the bar the review set:

- `security_test_java.java` gained a real `SecurityProbeController` in the same
  file, so `@WebMvcTest(controllers = SecurityProbeController.class)` is
  legitimate and `/management/health` returns **200** — proving `permitAll()`
  rather than treating a 404 as success — alongside 401 unauthenticated and 200
  authenticated on `/anything`.
- `actuator_test_java.java` moved to `webEnvironment = RANDOM_PORT` with
  `management.server.port=0`, `@LocalServerPort`, `@Value("${local.management.port}")`
  and a real `HttpClient` against **both** connectors. **§5.2's port isolation
  is now genuinely proved rather than read back out of the file jails wrote** —
  which was the review's actual complaint, and it is closed.

### 20.4 The two unjustified Hikari values

`max-lifetime=60000` was removed. `connection-timeout=1000` was kept, which the
review said was defensible — if the real-toolchain tier starts flaking against
a Testcontainers postgres under podman, raise this first.
`initialization-fail-timeout=1` stays and is correct: it bounds the fail-fast
*window*, so one successful attempt against a live database passes.

---

## 21. jails against the Rust Design Patterns book

`patterns/` at the repo root is a clone of **rust-unofficial/patterns**,
read-only research in the same category as `deps/` and `ideas/` — worth adding
to `CLAUDE.md`'s list of untracked siblings so nobody edits it.

Audited 2026-08-22. **jails follows the book closely.** Recorded so the clean
results are not re-derived:

| Book | jails |
|---|---|
| *Use borrowed types for arguments* — the most-cited idiom | **0** functions take `&String`, `&PathBuf` or `&Vec<T>`, across 33,433 lines |
| *Clone to satisfy the borrow checker* (anti-pattern) | 73 `.clone()` calls — one per 458 lines |
| *`#[deny(warnings)]`*, *Deref polymorphism* (anti-patterns) | both absent |
| *Builder* | `CommandSpec` is a textbook consuming builder (`mut self -> Self`) |
| *RAII guards* | `CWD_LOCK`, and all 16 sites bind `let _guard = …` rather than `let _ = …`, which would drop the guard immediately — the classic bug, avoided everywhere |
| *Prefer small crates* | two dependencies |
| *`mem::take`*, *Constructor*, *Default* | used, correctly, where they apply |
| *Privacy for extensibility*, `#[non_exhaustive]`, the FFI chapters, generics-as-type-classes | **not applicable** — a bin crate, no public API, no `unsafe` |

Non-test `unwrap()` count is **3**.

### 21.1 The one gap, and `abstract.md` diagnosed it better

The book's *Newtype* and *Type Consolidation into Wrappers* are the one family
jails does not use: **zero tuple-struct newtypes**, while 96 functions take
three or more bare `&str` and the worst take nine. Swap two and it compiles and
emits wrong Java.

**Do not plan against this section — plan against `abstract.md` §4.3**, which
found the same thing first and found the *cause*, which this audit did not: the
parameters are a **Data Clump** shed by `root: &Path`, because a generator
holding `root` does I/O while rendering, so the layer packages have to travel
one at a time. Newtypes are the symptom's cure; Introduce Parameter Object is
the disease's. Its rung 1 covers both.

### 21.2 The gate that is regressing, and the test that would stop it

`abstract.md` §8 prices rung 1 with a falsifiable gate — **`root: &Path` from
188 to under 40, and no `spring.rs` function over five parameters** — and says
to revert the rung if it misses. Nothing measures it, and by the same grep the
number is going the wrong way:

| | `ae63145` | `38c3dc6` | `7e92586` | worktree |
|---|---|---|---|---|
| `root: &Path` | 161 | 190 | 191 | **195** |

`7e92586` added `src/model/mod.rs` — `Project`, `Layers`, `Layer`, `Change`,
`Artifact`, 410 lines of exactly the right types — **beside** the primitive
rather than instead of it. `spring.rs` still has 38 functions over five
parameters. Mid-rung that is expected; unmeasured it is precisely the failure
`abstract.md` §5 names, *"the abstraction was cloned"*, with `model/mod.rs` as
the sixth clone.

**Make the gate a ratchet test.** `tests/genericity.rs` already proved the
shape in this repo: a number in a test, failing when it rises, is the thing that
actually moved the vocabulary problem after prose did not. A
`tests/architecture.rs` asserting `root: &Path` stays under a recorded ceiling
and no `spring.rs` function exceeds N parameters costs an afternoon, turns
`abstract.md`'s eleven gates into eleven ratchets, and makes rung 1 impossible
to half-finish. It is also the operational form of `abstract.md` §9's own rule —
*"the edit count is the number to watch on every change"* — which today is
watched by nobody.

**Done.** `tests/architecture.rs` exists and carries all eleven gates. It fails
in both directions — a rise above the ceiling, *and* a fall below it without the
new value being recorded — so an unmeasured improvement is a build failure
rather than a quiet gift to the next regression. `root: &Path` is **148** and
falling; `spring.rs` functions over five parameters is **0**. Read
`abstract.md` §8.1 before trusting the numbers in the table above: they were
measured by counting commas and adding one, which overstates every wrapped
signature by one, so the real starting count was 27 rather than 38.
