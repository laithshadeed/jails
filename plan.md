# plan.md — one plan, from six documents and the source

Written 2026-08-21/22 against the tree at `94107ef`, merging `ideas-opus.md`
(461 lines), `ideas-grok.md` (1,001), `ideas-kimi.md` (915), `ideas-sol.md`
(2,578), `ideas-fable.md` (1,269) and `ideas-opus2.md` (1,517) — 7,741 lines
in total — plus the 22,593 lines of Rust in `src/`, the 5,020 lines of tests,
and the upstream checkouts under `deps/`.

The six documents disagree in about a dozen load-bearing places. This file
**resolves** those disagreements rather than listing them, and every claim
about the current tree in §4 was re-checked with a grep or a read *today* —
several things the earlier documents call defects have since been fixed, and
several they call opportunities have since shipped.

## 0. How to read this

There are **two budgets**, not one.

**Latency** is how long you wait. It is what you pay hundreds of times a day
and it is what five of the six documents optimise.

**Authorship** is how much you type and decide. It is what you pay every time
the *shape* of the code changes, and when the generator is missing you do not
wait — you spend an afternoon on mechanical edits across six files.

Per occurrence, authorship is the larger number and it has been the
under-counted one. A test that reruns in 11 ms is worth little on a model you
cannot change without editing six files by hand. So this plan puts the
generator surface (§5) ahead of the test daemon (§6), and puts nine live
defects (§4) ahead of both, because a tool that silently writes a broken
project costs more than either budget can measure.

**And nothing in it is believed until an application proves it.** §2 is the
mechanism: three real projects — a web crawler, an Intercom-shaped support
inbox, and a deliberately unlike plain-Maven CLI — each built from a
declarative manifest with **zero hand-written Java**, each with an executable
acceptance contract, each acting as a falsifier. Two of the three already
exist and have already found and fixed twenty-one generic defects. Every item
in §5–§11 should be traceable to a gap one of those apps exposed, and §13's
sequence carries a column saying which.

Everything here obeys the constraints that make jails small enough to trust:
no plugin system, no Gradle, no ORM, no jails runtime jar, no Lombok, no
preview features in generated Java, templates stay real `.java` files, doctor
stays read-only, tests stay three-tier. `src/app.rs:1-6` adds one more that
post-dates most of the source documents: **core is domain-blind** — a crawler
and a support inbox are two lists of the same generic intents, and neither
gets a command, branch, enum or template in core. §2.7 turns that from a
doctrine into a gate; §9 respects it. Three of the six source documents do
not, and are superseded where they conflict.

**A note on the baseline.** The working tree has moved substantially since
five of the six documents were written, and this file is written against the
tree, not against them. `usecase`, `query`, `fetcher` and `durable-job` are
shipped `ArtifactKind`s; `ci` and `docker` are shipped `Capability`s; `@scope`
is a shipped field marker; and `TARGET_RELEASE` is now `"25"`. Several things
the source documents propose as future work are done, and several things they
rule out are consequently unblocked. Where this file and they disagree, check
the tree.

---

## 1. The two budgets, measured

### 1.1 Latency

Measured on this machine (`ideas-opus2.md` §1, `ideas-fable.md` §2), warm
`~/.m2`, `mvn -o`, on `tests/golden/scaffold-spring`:

| What | Time | Note |
|---|---:|---|
| JVM boot (`java -version`) | 0.03 s | the language is not the problem |
| `java Hi.java` (source launcher) | 0.6–0.9 s | |
| `mvn -o -q compile`, nothing changed | 2.27 s | pure no-op overhead |
| `mvn -o -q test -Dtest=X`, warm | **3.81 s** | median across the corpus: 2.57 s, p90 25.2 s |
| `javac` all 13 files, cold | 3.19 s | of which ~1.2 s is JVM start |
| `javac` one file, `-J-XX:+AutoCreateSharedArchive` | **0.25 s** | measured, vs 1.45 s without |
| JUnit `Launcher`, precompiled, fresh JVM | 1.65 s | |
| **same test, 2nd run in the same JVM** | **9–13 ms** | |
| **one file recompiled via `JavaCompiler` API, warm** | **74–166 ms** | |
| a domain record test itself | 5 ms | `domain.MccTest`, 3 tests |
| a `@WebMvcTest` slice | 2.2 s | context start |
| a Testcontainers `postgres:17-alpine` | 7.0–8.8 s | plus 0.45 s of Ryuk |
| save → app restarted (`run --watch`) | ≥2.15 s of pure waiting | before any work happens |

Two rows carry the story: **edit one test, recompile it, rerun it is ~110 ms
of real work, and Maven charges 3,810 ms for it.** The overhead is not the
JVM, not javac, not JUnit. It is Maven's project model resolution, plugin
descriptor loading and a forked JVM per invocation.

Reference points: Quarkus goes 1,470 ms → 295 ms on a one-line change from
*test selection* alone, and nags when a reload exceeds 4 s. That is the bar.

### 1.2 Authorship

Measured (`ideas-opus2.md` §5.1) on `jails new-cli inbox` + `add sqlite json`
+ one scaffold:

| | |
|---|---:|
| Commands typed | 3 + 1 |
| Lines of Java in the project afterwards | **1,180** |
| Lines *you* wrote | 0 |
| Time for the scaffold command | **0.039 s** |

That is jails at its best, and it is already won. Now the second row, which
nobody had counted. **Add one field to that scaffold:**

| File | Sites | What changes |
|---|---:|---|
| `domain/Message.java` | 5 | component, `requireNonNull`, trim, blank check, Javadoc |
| `adapters/JdbcMessageRepository.java` | 5 | select list, insert columns, placeholders, bind, row mapper |
| `web/MessageRequest.java` | 2 | component + mapping |
| `web/MessageResponse.java` | 2 | component + mapping |
| `db/migration/V002__create_messages.sql` | 1 | and you cannot edit it — forward-only, so a new file by hand |
| `test/resources/fixtures/messages.json` | 2 | one per fixture row |
| **6 files** | **~17** | **plus a migration written from scratch** |

And there is no command for it. Re-running the scaffold with the extra field
refuses on the fixture file.

**One command creates 1,180 lines; changing one field is six files by hand.**
That asymmetry is the whole ergonomic story, and it is why `g scaffold Note`
is the first afternoon and the rest of the week is not.

### 1.3 The authorship budget, as a formula

```
authorship cost = Σ over change-shapes ( files touched × edit sites × frequency )
```

Ranked by that product, on a real project:

| Change shape | Today | After §5 | Frequency |
|---|---|---|---|
| Add a field to a resource | 6 files, ~17 sites, + a migration | 1 command | weekly, often daily |
| Store a relation (`author:User`) | **not stored at all** — column, bind, mapper and FK by hand | 1 field spec | per model |
| Fix `/categorys` → `/categories` | 5 edits: controller, DDL, migration, DTO, fixture rename | 0 | per badly-pluralised noun |
| Model first (`g record`, then scaffold it) | blocked; retype every field | `g scaffold <Name>` | per model |
| `created_at` / `updated_at` | typed per table, and `updated_at` never updates | `--timestamps` | per table |
| Test data for a new test | `new` a six-component record; +1 component breaks 40 call sites | `g factory` | 5–20/day |
| Change a field in `.jails/app.toml` | **no effect** — the intent is skipped | re-applied | per manifest edit |
| Commands available in a repo you didn't create | **0 of ~30** | 8 demonstrated | per foreign codebase |

The right quality metric for a generator is therefore **authored lines and
decisions remaining after generation**, not generated line count
(`ideas-sol.md`). If you must immediately repair imports, align DTO fields,
add a migration, invent fixtures, wire a bean or write the first meaningful
test, the generator is incomplete.

### 1.4 The honest multiple

There is no 1000× on anything. What is actually available:

- **~35×** on the edit→test cycle (3,810 ms → ~110 ms), measured;
- **~345×** on the rerun alone (3,810 ms → 11 ms), measured;
- **2.4×** on JVM start for a packaged app with an AOT cache, measured;
- **~8 s per test run** from Testcontainers reuse, upstream-verified;
- **6 files → 1 command** on the single most common model change, measured;
- **0 → 8 commands** in a repository jails did not create, run;
- and a category change in *not being stuck*, which is where the hours
  actually vanish and which nothing measures.

Do not multiply unrelated ratios into a fake headline. The compounding claim
that survives scrutiny is: 5–10× on the three loops you run hundreds of times
a day, 3–5× on standing up a vertical, and the removal of a class of silent
failure whose cost is unbounded.

---

## 2. The proof projects — three apps that keep jails honest

### 2.1 Why the proof has to be an application, not a test

jails' own test suite cannot prove jails is generic. Golden files pin bytes,
tier 3 compiles what someone wrote a test for, and neither answers the only
question that matters: **can a real product be built out of these primitives
with no hand-written Java, and without a single domain word appearing in
core?**

That question is falsifiable, and the falsifier is an application. The rule:

> **The moment a proof app needs the word `crawl`, `conversation`,
> `workspace`, `robots` or `inbox` inside `src/`, the abstraction has failed.
> The fix is a new generic primitive, never a branch.**

`src/app.rs:1-6` already states the doctrine. The proof projects are what
enforce it, because a doctrine nothing tests is a comment.

**This harness already exists and is working.** Do not rebuild it:

- `examples/web-crawler/.jails/app.toml` — 60 lines, 10 generic intents.
- `examples/support-inbox/.jails/app.toml` — 90 lines, 15 generic intents.
- `examples/ACCEPTANCE.md` — the done/not-done boundary, per app.
- `examples/DOGFOOD.md` — the command log, the defect ledger, the friction
  ledger.
- `tests/cli.rs`: `app_manifest_plan_is_domain_blind_and_writes_nothing`,
  `app_manifest_builds_the_crawler_skeleton_and_is_resumable`,
  `app_manifest_builds_the_support_inbox_from_the_same_generic_intents`,
  `app_manifests_compile_without_manual_source_edits`,
  `app_manifests_pass_the_full_generated_verification_gate`.

The rest of this plan should be read as **work that closes a gap one of these
apps has already exposed.** §13's sequence carries a column saying which.

### 2.2 What the harness has already proved

The tree has moved a long way past what four of the six source documents
assumed. Verified against the working tree today:

| | |
|---|---|
| `ArtifactKind` | **26** variants — including `usecase`, `query`, `fetcher`, `durable-job`, which three documents propose as future work |
| `Capability` | **18** — including `ci` and `docker`, which two documents propose as future work |
| `pom::TARGET_RELEASE` | **`"25"`** — the LTS decision two documents argued for **has been made**; see §14 for what that unblocks |
| `@scope` | A generic field marker that compares a request field against a same-named JWT claim, and suppresses scaffold routes that cannot prove the boundary. Tenancy without the word "tenant" in core |
| Proved by `mvn verify` | Idempotent re-apply, real PostgreSQL migrations and typed queries, generated create use-cases through MVC, authenticated observability, typed Kafka round trips, PostgreSQL-leased durable work with replay/lease-reclaim/bounded-retry/terminal-visibility tests |

**Two live caveats about the tree, both true right now.** First,
`src/generate.rs` and `src/spring.rs` are modified in the working tree and
**the crate does not currently compile** — `generate.rs:872` calls
`crate::spring::transition_files`, which `spring.rs` does not yet define. That
is somebody's in-flight work, not a defect to fix here; it does mean any gate
run has to wait for it, and it means the enum inventories above are a snapshot
of a moving target. Second, that in-flight work is a **`transition` kind**
(`generate.rs:855-872`) whose refusal message reads
`jails g transition {name} id:uuid workspaceId:uuid@scope status:Status
version:long --on Conversation` — which is precisely sequence item #13, the
optimistic-transition gap App B's `version` column exposes. The method is
already working: the acceptance clause came first, the generic kind followed.

`examples/DOGFOOD.md`'s defect table is the evidence that this works as a
method: **twenty-one defects found by building the two apps, every one fixed
generically.** A sample, because the shape is the point — none of these fixes
mentions a crawler or an inbox:

- multiword resources generated `crawlrun` while other generators emitted
  `crawlRun` → **one lower-camel naming rule across all generators**;
- `Optional<Instant>` was passed to `Timestamp.from(...)` before unwrapping →
  **optional JDBC writes map the contained value before `orElse(null)`**;
- database wiring only covered tests that existed when `db` was added →
  **`app apply` reconciles every capability after all generation intents**;
- `generate event` ignored declared fields → **event contracts, constructors,
  examples and broker tests derive from the shared typed field model**;
- reconciliation added a second `@Import`, which is not repeatable →
  **the generic Java splicer merges configuration classes into one `@Import`
  and removes only its own member on unsplice**;
- generated transactional use-cases were `final`, so Spring could not build a
  CGLIB proxy → **transactional generated components are proxyable, pinned by
  a regression test**.

That is exactly the loop the plan should run on. **Never hand-fix the
examples.** A manual edit inside a proof app is not a fix — it is evidence for
the next generic improvement, and it belongs in the friction ledger.

### 2.3 App A — the web crawler

**What it is for.** It is the app whose hard parts are *outbound I/O and
termination*: a bounded, polite, resumable traversal of a page graph. Nothing
about it is CRUD, which is why it is the better falsifier of the two.

**What it must do, from a manifest and no hand-written Java** (`ACCEPTANCE.md`):
accept a seed URL, durably resume after restart, fetch an exact-host finite
page graph, store each canonical URL once, and report status and pages through
generated APIs — with adversarial tests covering robots policy, redirects
leaving scope, private/reserved-address SSRF including DNS rebinding,
response size/type/time limits, cycles, duplicate links, retryable versus
terminal failure, cancellation, and hard page/depth bounds.

**What is already generated:** `CrawlStatus`, two scaffolds, two create
use-cases, two typed queries, a typed `PageDiscovered` Kafka event whose
publisher keys on the event id, a `durable-job` dispatcher — and the
`fetcher` kind, which is the interesting one. `PageFetcher` is a **generic
safe outbound fetch boundary**: exact-host redirect policy, HTTPS-downgrade
prevention, reserved-address rejection, DNS pinning after validation, byte and
media-type and time and redirect bounds, failure classification, metrics, and
adversarial real-socket tests. It knows no seed URL and no robots file, which
is what makes it a capability-shaped thing rather than a crawler feature.

**What is open, and which plan item closes it:**

| Open (ACCEPTANCE.md) | Closed by |
|---|---|
| Composing the fetch boundary into finite HTML traversal | The traversal `usecase` intent, plus `add crawl` for `Urls`/`Robots`/`Politeness`/`Frontier` — §9.4 |
| robots.txt and cancellation tests | `add crawl` (`crawlercommons`, 5xx ⇒ disallow-all) — §9.4 |
| Building the OCI image in the local gate; running the hosted CI files | A local OCI build assertion in the gate; keep hosted CI a required check |
| Adding a field to `CrawledPage` costs six files by hand | **`g field`** — §5.1 |
| `crawl_run_id` is a bare `uuid`, not a foreign key | **Relations** — §5.2 |

**The genericity check for this app:** `add crawl` is inside the line because
it knows no seed URL, exactly as `add kafka` knows no topic. A `spider`
*kind* is outside it, and three of the six source documents propose one. They
are superseded.

### 2.4 App B — the Intercom-shaped support inbox

**What it is for.** It is the app whose hard parts are *tenancy, ordering and
durable delivery*: many actors writing to shared aggregates under an isolation
boundary, with effects that must survive a crash.

**One correction it is built on.** `ideas/minicom-public` is **not** a product
spec — its entire success condition is an alert reading `Yay! Everything
works`, there is no messaging code in the repository, and there is no
conversations table. Two of the source documents designed generators against
a product that does not exist. This app is designed from Intercom's own
documented shape instead, which is why the manifest has `Workspace`,
`Contact`, `Conversation` and `Message` with a `version` column and a
`@scope`d `workspaceId` — none of which minicom has.

**What it must do:** create workspaces, contacts, conversations and messages;
list them through stable tenant-scoped queries; durably stage outbound
delivery — with executable tests proving cross-workspace reads and writes are
denied, duplicate idempotency keys do not duplicate effects, stale optimistic
versions fail without mutation, message creation and delivery staging are
atomic, retries keep a stable delivery ID, and terminal delivery failure is
inspectable.

**What is already generated:** four scaffolds, two enums, four create
use-cases, three tenant-key-shaped queries, a typed `MessageReceived` event, an
`OutboundDelivery` durable job, and a production JWT resource-server chain
where `@scope` fields are compared against same-named claims. Scaffold routes
that cannot prove scope are **not emitted** — a generic refusal, not an inbox
rule.

**What is open, and which plan item closes it:**

| Open (ACCEPTANCE.md) | Closed by |
|---|---|
| Tenant enforcement against every persisted *association* (not just the request boundary) | `@scope` propagated through relations — needs **relations** first, §5.2 |
| Optimistic transitions — `version` is a column nothing checks | A generic `--expect <field>` on `usecase`, generating the compare-and-set and the 409 |
| Transactional outbox / provider delivery | **`add queue`**'s reaper and stable delivery ID — §9.3 |
| Realtime: an agent inbox that updates without a refresh | **`add sse`** — §9.2, with the four details both source documents get wrong |
| A browser widget can't call it at all | **`add cors`** — §9.1. `grep -rni cors src/ templates/` still returns nothing |
| `conversationId` is a bare `uuid` | **Relations** — §5.2 |
| `/messages` route names are fine, but `/status`, `/analysis`-shaped nouns are not | **The inflector** — §5.3 |

**The honest counterweight, worth keeping in front of you.**
`ideas/minicom-rails` is the "Rails is super productive" exhibit, and in ~90
lines it shipped three bugs a compiler would have caught: a route to a
controller action that does not exist, `find_by` called on undefined local
variables, and a `_send_message(direction)` that overwrites its own parameter
so every admin reply is stored as outbound. It also did not do realtime — the
widget polls and the inbox does `window.location = window.location`.

**So the jails pitch is not "as fast as Rails".** It is: *as fast to the first
endpoint, and those three bugs cannot compile.* That is a claim jails can make
and Rails cannot, and this app is where it is demonstrated.

### 2.5 App C — the control, and why two apps are not enough

Two proof apps can share an accidental abstraction. Three deliberately unlike
ones cannot. And App A and App B currently share a very large floor: **both
are Spring, both select `db api actuator observability security json testkit
kafka docker ci`, both are web services.** Any Spring-shaped assumption that
leaked into the generic machinery is invisible to both.

So add a third, whose only job is to falsify:

**A plain-Maven CLI with no Spring at all.** Concretely: a CSV → double-entry
ledger reconciler. It reads two statement files, normalises them, matches
entries, and reports the unmatched ones with reasons.

```toml
# examples/ledger-cli/.jails/app.toml  (proposed)
schema = 1
capabilities = ["csv", "json", "sqlite", "testkit", "format"]

[[generate]]
kind = "value"
name  = "Money"
fields = ["amount:long", "currency:Currency"]

[[generate]]
kind = "enum"
name  = "MatchOutcome"
fields = ["MATCHED", "AMOUNT_DIFFERS", "DATE_DIFFERS", "UNMATCHED"]

[[generate]]
kind = "sealed"
name  = "LedgerError"
fields = ["MalformedRow", "UnknownCurrency", "DuplicateReference"]

[[generate]]
kind = "record"
name  = "Entry"
fields = ["reference:string!", "postedAt:date", "amount:Money", "memo:string?"]

[[generate]]
kind = "strategy"
name  = "MatchRule"
fields = ["ExactReference", "AmountAndDate", "FuzzyMemo"]
strategy_on = "Entry"
strategy_yields = "MatchOutcome"

[[generate]]
kind = "cli"
name  = "Ledger"

[[generate]]
kind = "command"
name  = "Reconcile"
```

Why this one:

- **It removes every shared assumption.** No Spring, no web, no Kafka, no
  security, no tenancy, no outbound I/O. If `app apply` only works when a
  Spring parent POM is present, this app says so on the first run.
- **It walks straight into a known live defect.** `g scaffold`/`g dto` splice
  a versionless `spring-boot-starter-validation` with no flavor check, so a
  `new-cli` project gets a `pom.xml` Maven refuses to parse (§4.1). A
  plain-Maven proof app makes that defect a **failing gate** rather than
  something you find by hand on a Tuesday.
- **It exercises the kinds the other two never touch** — `value`, `sealed`,
  `strategy`, `cli`, `command`, `record` — including `register_command`'s
  dispatcher splice and `g strategy`'s read-disk `destroy`.
- **It is cheap.** Seven intents, no containers, no broker; the gate is
  `mvn -o verify` in seconds, not the 196 s the durable-work gate takes.

**Its acceptance contract**, in the same shape as the other two: from the
manifest and no hand-written Java, produce a CLI that reads two CSVs, reports
matched and unmatched entries with a reason per row, exits non-zero when
unmatched entries exist, and round-trips its report as JSON — with tests
covering a malformed row, an unknown currency, a duplicate reference, an
amount that differs by one minor unit, and an empty file.

**And one rule that makes it a real control:** *adding App C must not add a
line to `src/`.* If it does, that line is the finding, and the fix is generic
— exactly as the twenty-one defects in `DOGFOOD.md` were.

### 2.6 The authorship ledger — the number that proves the thesis

§1.2 argues that generators buy authorship time. The proof apps are where that
stops being an argument. **Record these per app, per gate run:**

| Metric | Why |
|---|---|
| Manifest lines | 60 (crawler), 90 (inbox), ~35 (ledger) — the input |
| Generated Java + SQL lines | the output. One `g scaffold` alone is 1,180 |
| **Hand-written Java or SQL lines** | **must be 0.** Any non-zero value is a friction-ledger row, not a footnote |
| Manual interventions during the gate | the friction ledger's row count, which should trend to zero |
| Commands to go from empty directory to passing gate | `new` → `app apply` → `check`. If it grows, something regressed |
| Gate wall time | 196 s today for durable work; it is also a latency budget |

The one that carries the whole thesis is the third. "Smarter generators mean
you move faster" is measurable exactly as **hand-written lines per feature
trending to zero while the feature set grows** — and today it is genuinely 0
for both apps, which is the strongest single fact in this document.

### 2.7 The genericity gate, applied to every core change

Before a line lands in `src/`, it must pass all six (adapted from
`ideas-sol.md`):

1. Can it be named without mentioning a showcase domain?
2. Is it useful to at least three materially different applications? — which
   is now a question you can *answer*, because there are three.
3. Does it represent a Spring/build/application concern rather than business
   behaviour?
4. Can a project decline it without weakening unrelated capabilities?
5. Does it lower through the same intent, capability and write path?
6. Does the generated application remain understandable and operable **without
   jails installed**?

Fail 1–3 and it belongs in a manifest, not in core. Fail 4–6 and the design is
too coupled.

Two mechanical guards worth adding so this is enforced rather than believed:

- **A repository test that greps `src/` and `templates/` for the showcase
  vocabulary** (`crawl`, `spider`, `robots`, `conversation`, `workspace`,
  `inbox`, `ledger`, `reconcile`) and fails on a hit outside a comment. Cheap,
  and it is the only thing that stops the doctrine eroding one convenient
  branch at a time.
- **`app plan` must stay domain-blind and write nothing** — already pinned by
  `app_manifest_plan_is_domain_blind_and_writes_nothing`. Keep that test first
  in the file; it is the canary.

### 2.8 How the harness runs

From `examples/DOGFOOD.md`, unchanged:

```bash
cargo build && export JAILS_BIN="$PWD/target/debug/jails"

"$JAILS_BIN" new web-crawler --deps web,validation
mkdir -p web-crawler/.jails
cp examples/web-crawler/.jails/app.toml web-crawler/.jails/app.toml
cd web-crawler
"$JAILS_BIN" app plan
"$JAILS_BIN" app apply --no-start
"$JAILS_BIN" routes && "$JAILS_BIN" beans && "$JAILS_BIN" doctor && "$JAILS_BIN" check
```

Three friction items in that flow are themselves plan work, and they are the
first three rows of the friction ledger:

- `jails new` needs Initializr and a network → **`new --offline`**, whose
  asset already exists as `write_spring_fixture` in `tests/common/mod.rs`;
- the manifest is copied by hand → **`jails app init --manifest <path>`** or
  `new --app <path>`;
- `app apply` describes intents but does not lower them through one atomic
  plan → §7, and it is the trigger that makes provenance worth building.

The automated tier uses the offline fixture instead of Initializr, which is
why the gate runs without a network. Keep it that way.

---

## 3. Corrections ledger — do not build these

The most expensive thing in a plan is a design resting on a fact that is not
true. Each row below was checked against source; two of them would have eaten
a week each.

| Claim, and where it came from | What the source says |
|---|---|
| **AOT cache pays on every devtools restart, `mvn test` fork and `jails run`** (`ideas-opus.md` A2) | The AOT cache **refuses any classpath containing a directory**, and `target/classes` is a directory (`deps/jdk/.../aotClassLocation.cpp:687-708`; reproduced: `Error: non-empty directory 'target/classes'`). All three named loops are out. Worse, a devtools restart is a new classloader **in the same JVM** (`RestartClassLoader`), so there is no process start for a cache to save. AOT is real — 6.6 s → 2.96 s on a **jar** classpath — and belongs to `jails build` / `add docker`, not the dev loop. |
| **`-XX:+AllowEnhancedClassRedefinition` covers ~90% of real edits** (`ideas-opus.md` A1) | The flag does not exist in OpenJDK (`grep -rl` over `deps/jdk` returns nothing) — it is JetBrains Runtime / DCEVM. The source documents then ruled it out because **JBR tops out at JDK 25** while `TARGET_RELEASE` was 27. **`TARGET_RELEASE` is now `"25"`, so that objection is gone** and the JBR path is reachable for the first time — see §14. It is still not the default: stock JVMTI is method bodies only (`jvmti.xml:8136-8140`), so plan for it and treat enhanced redefinition as an opt-in `doctor`-detected bonus. Either way, since jails' domain layer is records and sealed types, **every domain edit is a restart on a stock JVM** — say so, or `jails dev` looks broken. |
| **JDWP `RedefineClasses` is command set 2, command 18** (`ideas-opus.md` A1) | Command set **1** (`VirtualMachine`); set 2 is `ReferenceType`. You also need `IDSizes` and `ClassesBySignature` first and ID widths are dynamic — a working client is ~400 lines, not 150 (`deps/jdk/.../jdwp.spec`). Use jdt.ls's existing HCR (free) or `jdb redefine` before writing one. |
| **`SseEmitter`'s never-time-out value is `Long.MAX_VALUE`** (folk answer behind both SSE designs) | Spring's own reactive path uses **`-1L`**, and Spring's default is `null` — the 30 s is Tomcat's `Connector.asyncTimeout`. Write `new SseEmitter(0L)` or `-1L` and `spring.mvc.async.request-timeout=-1`, verified end to end into `AbstractProcessor`. |
| **Intercom webhooks are `X-Hub-Signature-256` / HMAC-SHA-256** (`ideas-grok.md` §8.1) | Intercom signs `X-Hub-Signature` with HMAC-**SHA-1**, `sha1=` prefix, keyed by `client_secret`. A verifier built to that spec **rejects every real delivery**. The closed set is three: `hmac_sha1_hex`, `hmac_sha256_hex`, `stripe_v1` (with a mandatory 300 s tolerance check). |
| **minicom is "users → conversations → messages"** (`ideas-opus.md` B1, `ideas-grok.md` §8) | `ideas/minicom-public/README.md` says the entire success condition is an alert reading `Yay! Everything works`. There is **no messaging code in the repository at all** and **no conversations table** — `users(id,email,created_at,updated_at)` and `messages(id,user_id,content,message_read,…)`, flat, with a direction char. Both documents designed against a product that does not exist. The Intercom slices are still worth building, but from Intercom's own docs, not from this stub. |
| **`jails run --watch` already pipes through `why`** (`ideas-opus.md`) | It does not, and still does not — see §4.4. |
| **A `notify` crate would be the second dependency** (`ideas-opus.md` A1) | Third: `clap` and `clap_complete` are both declared. Polling is still right (devtools and Quarkus both poll), so this changes nothing except the sentence. |
| **`rails test --only-failures`** (`ideas-grok.md`) | Not a Rails feature; it is RSpec's. Rails prints a copy-pasteable `bin/rails test path:LINE` instead — which is the better thing to copy. |
| **Boot 4 sets `spring.threads.virtual.enabled=true`** (`ideas-grok.md` §8.3) | Default is **`false`**. And a virtual-threads app whose only work is `@Scheduled` **exits 0 immediately** unless `spring.main.keep-alive=true`. That is a `why` rule. |
| **`-XX:TieredStopAtLevel=1` and `spring.jmx.enabled=false` are speed tips** | `spring-boot:run` already passes the first (`optimizedLaunch=true`) and JMX is already off. Obsolete advice — and for STS4 live hover you actually want JMX back **on**. |
| **Mint JWTs with Nimbus directly** (`ideas-grok.md` §8.2) | A level too low. Spring Security 7 ships `NimbusJwtEncoder.withSecretKey` (`@since 7.0`, so every tutorial predates it) and Nimbus arrives transitively. The silent failure that earns the generator its place: **a JWT with no `exp` claim passes the default decoder** (`JwtTimestampValidator.allowEmptyExpiryClaim = true`), and the default chain checks no issuer and no audience. |
| **`add http` can be extended into a fetcher** (`ideas-opus.md` B3) | `add http` is an HTTP **server** on `com.sun.net.httpserver`. A crawler capability is a different thing; name it `add crawl`. |
| **robots.txt via `re2j` or a hand-rolled parser** | Take `crawlercommons.robots` — RFC 9309's longest-octet match, Allow-wins-tie and group-combining are exactly where hand parsers go wrong, and the 5xx⇒disallow-all rule is the one everybody gets backwards. |
| **CLAUDE.md: the manifest is `deps/deps.tsv` and `deps/update.sh`** | They are `deps.tsv` and `deps-update.sh` at the repo root. Confirmed by `ls`. This one matters because CLAUDE.md is the first thing every agent reads, and the wrong path propagated into roughly six citations before anyone checked. |

Two more facts worth carrying, both **confirmed** rather than corrected:
`@MockBean` is gone in Boot 4 (`@MockitoBean` in `spring-test`); `MockMvcTester`
is `@since 6.2`, i.e. Boot ≥ 3.4 — which is why §4.3 exists.

---

## 4. Tier 0 — the live defects, verified today

Every row was re-checked against `94107ef` while writing this file. One defect
the source documents report (`doctor` reading `java` on PATH instead of
`JAVA_HOME`) **is already fixed** — `doctor.rs:871-875` now prefers
`JAVA_HOME`. The rest are live.

| # | Defect | Evidence | Fix | Effort |
|---|---|---|---|---|
| 4.1 | **`g scaffold` and `g dto` write a `pom.xml` Maven cannot read.** `VALIDATION_STARTER` has `version: None`, correct under `spring-boot-starter-parent` and fatal without one. No flavor check: `require_spring_project` guards only `client`, `job`, `event`. | `src/generate.rs:1029,1059`; `src/spring.rs:29` — confirmed present today. `mvn -o test` on a `new-cli` project: `'dependencies.dependency.version' … is missing`. | Have `ensure_dependency` consult `pom::flavor` and splice a pinned version with no parent BOM — which is what every non-Spring capability in `add` already does. `maven-failsafe-plugin` is spliced versionless too; Maven only warns, so it is a trap rather than a break. | 30 min |
| 4.2 | **The golden suite ratifies that broken pom.** | `tests/golden/scaffold-plain/pom.xml:14-17` — read today, still versionless. | Regenerate and **read the diff**. That diff is the bug report. | 10 min |
| 4.3 | **`g controller` / `g scaffold` emit Java that cannot compile on Boot 3.0–3.3.** `controller_stub_test.java` imports `MockMvcTester` unconditionally (`@since 6.2` = Boot ≥ 3.4), while `spring_boot_major` resolves only the major and gates just the `@AutoConfigureMockMvc` package move. On Boot 3.2 jails emits the legacy package **and** the 6.2-only type. | `src/generate.rs:187-206`; `git -C deps/spring-boot show v3.3.0:gradle.properties` → Framework 6.1.8. | Resolve a **framework** version, not a Boot major; below 6.2 **refuse with a `fix:` line** rather than growing a second template family. Nine API families would have to fork to support Boot 2.7 output — that is the trap, quantified. Add `--no-test` as the escape hatch. | 2 h |
| 4.4 | **`jails test 'Class#method'` silently runs the wrong thing and exits 0.** The suffix is appended to the whole filter, then the Failsafe routing check runs against the **already-mangled** string. | `src/run.rs:162-180`, read today: `format!("{f}Test")` then `if test_name.ends_with("IT")`. `MoneyTest#roundsDown` → `-Dtest=MoneyTest#roundsDownTest`; `CheckoutIT#happy` → `…happyTest`, no longer ends in `IT`, routed to Surefire. | Split on `#` first, suffix only the class part, **and** move the Failsafe decision before the mangle. Fixing one without the other leaves the `IT` case broken. Also pass `-Dsurefire.failIfNoSpecifiedTests=false` — mandatory the moment any selection feature exists. | 20 min |
| 4.5 | **`jails run --watch` cannot report a failed startup** — the documented fix has one caller and it is the *non-watch* branch. `mvn spring-boot:run` exits 0 over a dead app because devtools catches on `restartedMain`. | `src/run.rs:83` (`run_watched`), sole caller `:372`; `watch()` at `:264` uses a bare `.spawn()` with inherited stdio. | Route `watch()` through `run_watched`. | 1 h |
| 4.6 | **The watcher only stats `.java`**, so editing `application.properties` or dropping a migration triggers nothing; it compares max-mtime, so it cannot name the changed file, misses deletions, and misses `git checkout` / `stash pop` (older mtimes). | `src/run.rs:325-346`. | Replace with a `HashMap<PathBuf, SystemTime>` over `src/main/java`, `src/test/java`, `src/main/resources/**`, `db/migration`, `pom.xml`, `compose.yaml`, `jails.toml`; compare with `!=`; report added/changed/**deleted**. Still polling, no crate. | 2 h |
| 4.7 | **Nothing asks whether the generated project builds.** Tier 1 tests pure functions, tier 2 tests argv, tier 3 compiles only the combinations someone wrote a test for (`new-cli` + `g scaffold` is not one), and the golden tier compares bytes to bytes — it cannot tell a correct pom from an unparseable one. | The structural cause of 4.1 and 4.3. | A tier-3 matrix over `{new-cli, new-spring}` × `{scaffold, dto, repo, handler, command, event, client, job}` × `{none, add db, add json}` running **`mvn -o validate`** (~2 s a cell, under a minute total). Gate it like the other tier-3 tests and add it to `JAILS_REQUIRE_TOOLCHAIN`. Two cells fail today. | 2–3 h |
| 4.8 | **`doctor` reports health over a pom Maven refuses to parse** (`ok maven`, `ok beans`, `9 checks all clear`), because `pom::read` falls back to `unwrap_or_default`. | `src/doctor.rs:117`. | At minimum make an unparseable pom a loud FAIL. Optionally `mvn -o -q validate` — it writes nothing so it stays inside doctor's read-only contract, at the cost of a subprocess. | 1 h |
| 4.9 | **Drift.** `g cases` is implemented (`ArtifactKind::Cases`, `src/generate/migration.rs`) and **absent from README** — `grep -c cases README.md` returns 1, and it is about sample values. `jails.nvim`'s lists are missing `toxiproxy` and `app` (checked today), plus aliases, `repository`/`mig`/`it`, `--check` on `migrate`, and `--debug` everywhere. `validation/README.md`'s status table blocks nine workouts on features that have shipped. | greps run today. | Fix each, then **a Rust test pinning the four Lua tables to `Capability::label()`, `ArtifactKind` and the clap tree.** That test is what stops the class of bug rather than an instance — every idea in every document adds an enum variant. | 1 h |

Two `jails.nvim` bugs worth folding in while there: `setqflist({}, 'r', …)`
**replaces** the error list jdtls just built (should be `' '`), and
`vim.fn.termopen` is deprecated since 0.11.

**This tier is roughly one focused day and it is strictly ahead of every
feature in every document**, because features land on top of it.

---

## 5. Tier 1 — the authorship engine

This is the section the latency framing was crowding out. Everything in it is
**domain-blind** and therefore inside the line `src/app.rs:1-6` draws: growing
a record, resolving a foreign key, pluralising a noun and reading a field spec
are not crawler features or inbox features.

**What is already done here, so this list stays honest.** The generator
surface has grown a great deal: `usecase` (a create workflow with a
transaction boundary, an MVC route, a port, an implementation and focused
mock-free tests), `query` (typed parameters and results with a JDBC adapter
and a real-database test), `durable-job` (PostgreSQL-leased work with replay,
lease reclaim, bounded retry and terminal-failure visibility), `fetcher` (a
safe outbound boundary), plus the `ci` and `docker` capabilities. All of them
derive from the one shared field model, which is why the `DOGFOOD.md` defect
table reads the way it does — most of those bugs were *two* outputs of one
model disagreeing, and each fix collapsed them onto one source of truth.

Everything below is what that model still cannot do, and each item names the
proof app that exposed it.

### 5.1 `g field` — the single highest-value generator jails does not have

```
jails g field Message priority:int
jails g field Message archivedAt:instant?
```

Reads the record with `fields_from_record` (exists, `generate/domain.rs:230`),
refuses a duplicate component, appends in declaration order, then rewrites
**only the derived files that still match what jails would have written**:

```
updated  domain/Message.java
updated  web/MessageRequest.java
updated  web/MessageResponse.java
created  db/migration/V007__add_priority_to_messages.sql
skipped  adapters/JdbcMessageRepository.java -- you have edited this file
         add to the select list:  priority
         add to the insert:       priority
         bind:                    ps.setInt(6, m.priority())
         map:                     rs.getInt("priority")
```

**That refusal is the design, not a limitation.** The ownership oracle is
`edited_files` (`src/add/database.rs:371-379`) — nine lines that re-render the
current template and diff the bytes. It cannot distinguish "you edited this"
from "jails changed its template", so on any jails upgrade it over-reports.
Over-reporting prints a snippet you paste; over-writing silently destroys
work. **Print, never clobber.**

`ideas-grok.md` §6 assumed a jails-owned marker already exists in generated
Java "same as capability property blocks". It does not — property blocks carry
`# jails:<label>` comments; generated `.java` carries nothing. `edited_files`
is the whole oracle available today, and it is enough to ship.

The migration comes from `sql.rs`, which already owns the column projection:
`alter table messages add column priority integer not null`, forward-only,
never an edit to `V002`. A `not null` column on a populated table needs a
default, so the generated SQL carries one and says so in a comment.

**`--remove` is deliberately not in v1.** Dropping a column is a migration you
write by hand because of the data. Adding is the 95% case.

Test: a golden scenario that scaffolds, runs `g field`, and snapshots the
record, both DTOs, the adapter, the new migration and the fixture; plus a unit
test that a hand-edited adapter is skipped rather than rewritten.

### 5.2 Relations — the only defect here whose symptom is lost data

Measured. `User.java` is on disk with `id:uuid@pk`. Then:

```
jails g scaffold Post id:uuid@pk author:User title:'string!'
```

produces `author text not null` in the migration and an adapter Javadoc
saying "Not persisted, because jails has no mapping for the type: author."
**The scaffold compiles, the app starts, `POST /posts` returns 201, and the
author is gone.** jails is honest about it in two comments, which is
consistent with its philosophy and is not enough — a comment in a generated
file is exactly what nobody reads.

The information needed is right there, and jails already reads records off
disk for `g repo` and `g dto`.

**The rule, closed and small:** if the referenced type is a record in this
project with exactly one `@pk` component — or failing that a component named
`id` — persist `<name>_id` with that component's SQL type and emit
`references <pluralised type> (<pk column>)` in the create table. The Java
component stays `User`; the *adapter* stores the id and the service loads it.
No lazy loading, no proxies, no `@ManyToOne`. This is `belongs_to` without
ActiveRecord.

Anything jails cannot resolve this way — no pk, a collection, a type it has
never seen — keeps today's behaviour exactly: named in the Javadoc, named in
the migration comment, not guessed at. **Do not invent `on delete` behaviour;**
omit it and document that the database default applies.

An explicit `@fk(users)` marker is the validatable escape hatch (the table is
in `db/migration`, so jails can check it), and it fits the existing closed
marker set.

### 5.3 The pluraliser is `+ "s"`, and it is visible in the URL

Confirmed today at `src/sql.rs:288-295`: append `s` unless it already ends in
`s`. Measured by scaffolding six nouns:

| Type | route / table / fixture | should be |
|---|---|---|
| `Category` | `/categorys` | categories |
| `Company` | `/companys` | companies |
| `Box` | `/boxs` | boxes |
| `Analysis` | `/analysis` | analyses |
| `Status` | `/status` | statuses |
| `Person` | `/persons` | people (or persons — defensible) |

`/categorys` is what you notice in the first minute of a demo, and fixing it
by hand is the same six-file spread as §5.1 for a typo you did not make.

**The good news is one owner.** Route path, table name and fixture filename
all derive from `table_name`. One closed inflector fixes all three: `…y` after
a consonant → `ies`; `…s|x|z|ch|sh` → `es`; `…f|fe` → `ves`; a short irregular
table (`person/people`, `analysis/analyses`, `status/statuses`,
`child/children`); an uncountable list (`data`, `info`). ~60 lines and a table
test.

Keep it closed and **keep it out of `jails.toml`**. A per-project override
means the table name is no longer derivable from the type name, and
derivability is the whole reason `destroy` can find what `generate` wrote.

The current Javadoc defends the naive rule on the grounds that "an irregular
plural is a judgment call, and a wrong guess in a migration is expensive to
undo". That is right about irregulars and wrong about the regular rules —
`categorys` is not a judgment call, it is a misspelling. Ship the regular
rules plus a short irregular table, and keep refusing to guess beyond it.

### 5.4 One rule for where fields come from

`g repo Draft` with no field spec reads `Draft.java` and derives everything
(`generate.rs:710`). So does `g dto` (`:789`). `g scaffold Draft` does not —
it refuses because `Draft.java` exists. So the natural workflow, model the
type first then generate the machinery around it, is blocked on the one kind
that spans the most files.

**State the rule once and implement it once:** a kind's fields come from the
spec if given, else from the record on disk if there is one, else it is an
error. Today that rule is implemented three times and differs each time.

### 5.5 `--timestamps`

Verified absent (`jails g --help` lists `--package`, `--index`, `--on`,
`--yields` only). Every table anyone writes carries `created_at` and
`updated_at`; both minicom tables do, in all four language stubs. So you type
`createdAt:instant updatedAt:instant` on every scaffold — and then the
`updatedAt` half is a lie because nothing updates it.

`--timestamps` adds both columns, puts `created_at` in the insert with the
clock the project already has (`add testkit` generates one), and — the part
that makes it a flag rather than two more keystrokes — **writes the
`updated_at` assignment into the adapter's update path**, so the column means
what its name says. Default on for `scaffold` and for `g repo` when it writes
a migration; off for bare `record`/`value`, because a domain type is not a
row. `--no-timestamps` declines. Golden files change wholesale; that is the
correct price.

### 5.6 `g factory` — the missing half of test speed

After generation, the biggest remaining Java tax is test data setup. Every
hand-written test `new`s a six-component record, and the day a component is
added, forty call sites break. FactoryBot is half of why Rails testing feels
fast, and jails already owns the hard part (`sample_value`).

```
jails g factory Note   ->  testkit/NoteFactory.valid().title("x").build()
```

Rules inherit the field-spec semantics exactly: enums get `values()[0]`, `?`
components get `Optional.empty()`, collections default empty, `!` fields get
non-blank samples. A component jails cannot sample starts `null` and
`build()` **throws naming the component** — the factory analogue of the
`@Disabled`-with-a-name rule. Never a guessed default: a silently-wrong
default in a factory poisons every test that uses it, which is strictly worse
than forty broken constructors.

Reads the record off disk, so it works on hand-written records too.

### 5.7 `requests/*.http`

The third everyday loop after test and dev is "fire the request", and today
that is a hand-typed curl lost in scrollback. `g scaffold` gains a side
artifact: `requests/note.http` — `@host = http://localhost:8080`, one block
per operation, bodies built from `sample_value` (the machinery that already
fills fixture rows and factory defaults — third reuse). The format is
tool-agnostic (IntelliJ, VS Code REST Client, `kulala.nvim`) and the files are
useful without any plugin. Once `g field` exists it updates the body the same
way it updates the fixture. Half a day.

### 5.8 Refusals are ergonomics

```
$ jails g scaffold Message ... priority:int
jails: .../src/test/resources/fixtures/messages.json already exists
```

Technically true, practically useless, and it is the message you get for the
single most common mistaken command in the tool. It should be:

```
jails: Message is already scaffolded (6 files).
       jails cannot grow a scaffold in place.
  fix: jails g field Message priority:int
```

`doctor` is already held to this standard — an integration test asserts every
`FAIL` carries a `fix:` line. Generators are held to no such standard, and
they are the commands people actually run. **Add the same test for generator
refusals.** One hour, and every wrong command starts teaching the right one.

### 5.9 The manifest is the new ergonomic unit, and it cannot be edited

`jails app` shipped (`src/app.rs`, 539 lines; `.jails/app.toml`,
`.jails/app-state-v1`, `plan`/`apply`, closed `[[generate]]` schema of
`kind`/`name`/`fields`/`indexes`/`package`/`strategy_on`/`strategy_yields`).
That answers `ideas-opus.md` B2's `new --template` and `ideas-grok.md` §8.5's
"a README section, not a command" — both are superseded; build on the
manifest.

But the state file records completed intent keys and **skips** them, so
editing a `fields` line and re-running `app apply` does nothing at all.
`examples/DOGFOOD.md` already flags this in its own friction ledger.

That is §5.1 one level up. **`g field` is the primitive the manifest needs to
become editable**, because "reconcile drift for a changed `fields` line" *is*
"add the field that is in the manifest and not in the record". Build the
command first, then let `app apply` call it — the other order writes the
reconciliation logic twice.

Two more manifest notes:

- Inside a manifest **there is no shell-quoting problem at all**, which is a
  real argument for making the manifest rather than the prompt the place you
  write field specs. At a prompt, `list<Match>` is a redirect and `!` is
  history expansion, so a realistic scaffold needs quotes on most of its
  arguments. Resolve it by accepting both spellings — `matched:list:Match` and
  `content:string.req` quote-free at a prompt, `list<T>` and `!` in a manifest
  — and documenting which is which. `validation/README.md` has recorded this
  tax for months with nothing done; make the decision either way.
- `parse_fields` should reject the eight names javac refuses as record
  components (`clone finalize getClass hashCode notify notifyAll toString
  wait`) with a sentence, instead of emitting a file that does not compile.

### 5.10 What not to build here

- **No ORM, no relation traversal, no lazy loading.** §5.2 is a column and a
  foreign key. `post.author()` returns a `User` the service loaded.
- **No `g field --remove`** in v1.
- **No inflector overrides in `jails.toml`.**
- **No `g field` that rewrites an old migration.** Forward-only stays.
- **No provenance ledger as a prerequisite** — see §7.
- **No domain-specific field types.** `email:string` is a string; if it needs
  a check it needs a constraint, and constraints are already a closed set.
- **No Django-style `makemigrations` autodetect.** It requires models to own
  the schema, which is ORM thinking through the back door. `g field` is the
  explicit, reviewable half. A read-only `jails schema --diff` that replays
  jails' own DDL subset and compares it to the records on disk is the honest
  version, and it is a `doctor` check (`migrate --check` cannot be one — it
  writes).

---

## 6. Tier 2 — the latency engine

Ordered by measured value, with §3's corrections already applied.

### 6.1 Free wins, hours not days

**Testcontainers reuse — ~8 s per run, the biggest single number here.**
`grep -rn withReuse templates/ src/` returns nothing today. Add
`.withReuse(true)` to `TestcontainersConfig`'s `@Bean` (`@UnstableAPI`; say so
in the Javadoc). Safe unconditionally: without the machine flag it is a no-op
plus a warning naming the file, and Boot's lifecycle processor already refuses
to destroy a reused container. The flag **must** live in
`~/.testcontainers.properties` or `TESTCONTAINERS_REUSE_ENABLE` — a classpath
`testcontainers.properties` does nothing. `doctor` reports it (reading `$HOME`
is a read); a one-off `jails setup` writes it, because doctor never writes.
Two consequences to encode: reused containers are **never registered with
Ryuk**, so they accumulate — doctor should count containers labelled
`org.testcontainers.hash` and print the cleanup line — and **the database
keeps its state between runs**, so a test assuming an empty table fails on the
second run; say that in the generated Javadoc. Confirm the interaction with
`@ServiceConnection` before writing it into the template (two lines to try).

**`META-INF/spring-devtools.properties` — ~1.2 s off every restart.** Every
`defaults.<key>` in that file applies **only when devtools is present**, with
zero effect on the packaged jar and no profile to remember. Boot's own modules
use it. `jails new` should write one:

```properties
defaults.spring.devtools.restart.poll-interval=200ms
defaults.spring.devtools.restart.quiet-period=50ms
defaults.spring.docker.compose.enabled=false   # this machine's podman-compose problem
```

The defaults are 1 s and 400 ms.

**`jails test` flags.** `-o -q -ntp`, `-Dsurefire.failIfNoSpecifiedTests=false`
(mandatory alongside §4.4), `Class#method`, `--fail-fast`
(`-Dsurefire.skipAfterFailureCount=1`), `--failed` (parse `<failure>`/`<error>`
out of `target/surefire-reports/TEST-*.xml`, ~30 lines, no XML library),
`--slowest`, and **the Rails move: print the rerun line on failure**
(`jails test path:LINE`). `--retry N` exists but is **off by default** — a
green build over a flake is the Failsafe failure mode again.

**`jails test <file>:<line>`.** Find the enclosing `@Test` with
`java::blanked()` + `java::annotations()` and emit `Class#method`. Jupiter
never resolves a `FileSelector`, so `--select-file …?line=N` on the console
launcher parses and silently runs nothing — jails must do the resolution.
Nested classes are `Outer$Nested#method`; the Neovim ftplugin currently uses
the filename, which is wrong for `@Nested`.

**`why` on every Maven failure, not just watched runs.** `run.rs` pipes
through `why::FATAL_MARKERS` only in `run_watched`; `test`, `build`, `check`
and `fmt` print raw Maven and stop. Non-zero exit → run the tail through
`why::explain` and print the top rule's explanation and `fix:` line after the
raw output (`--plain` opts out). Reuse the colour flags `run_watched` already
passes, since piping costs the child its TTY. This multiplies the value of
every rule already in the table and every future mined one.

**`mise.toml` from `new`**, pinning `java` and `maven` to what the pom
targets, so doctor's most common FAIL has a copy-pasteable fix.

### 6.2 `jails test --fast` — get Maven out of the loop

The category change is not "run Maven on a watch". Both `ideas-opus.md` A4.1
and `ideas-grok.md` §4 re-invoke Maven per change and therefore cap at ~2.5–3.8
s: they remove the keystroke, not the cost.

**Step 1, the console launcher.** Splice
`org.junit.platform:junit-platform-console` in test scope with **no version**
— Boot's parent imports `junit-bom`, so it tracks the project's Jupiter. Then:

```
java @target/jails/cp.args org.junit.platform.console.ConsoleLauncher execute \
  --select-method com.x.MoneyTest#roundsHalfUp --details=testfeed \
  --fail-if-no-tests --reports-dir target/jails/reports
```

`cwd` must be the module root (Surefire's `basedir`; Flyway `filesystem:`
locations and `src/test/resources` relative paths depend on it). `testfeed`
streams one line per test, which is the `--watch` and quickfix format. Spring's
test support is pure Jupiter extensions plus `spring.factories` — nothing
reads a Surefire API. Estimated 0.35–0.6 s vs 2.57 s; **unverified on this
machine and the first thing to measure.** `neotest-java` ships exactly this
path, which is independent evidence it works.

Classpath from `mvn -q -o dependency:build-classpath
-Dmdep.outputFile=.jails/cp.txt`, cached against `pom.xml`'s hash and
invalidated by every `add`/`remove`. (`jails c` re-runs this on every start
even with `--no-build`; use the same cache.)

**Step 2, `jails testd`.** One resident JVM holding
`ToolProvider.getSystemJavaCompiler()` and the launcher's `"junit"` tool
provider, over a unix socket at `~/.jails/testd/<project-hash>.sock`. Compile
the changed file in-process (**74–166 ms warm, measured**), run through
`LauncherFactory.openSession()` (**9–13 ms warm, measured**), isolate each run
behind a fresh `URLClassLoader` so redefinition limits never apply — you throw
the classloader away rather than hot-swapping into it. Die on `pom.xml`
change, on `testd stop`, and after an idle timeout.

**The fresh-classloader cost is the one unmeasured piece** of the 110 ms
figure (measured with a shared loader). Expect tens of ms, not seconds, but
treat it as unverified until benchmarked. A test that calls `System.exit`
kills the daemon; jails' own `command` template already avoids that.

**Step 3, `--affected`** — Quarkus' actual killer feature, which none of the
source documents except `ideas-fable.md` has. A reverse-dependency index built
from `.class` constant pools in `target/classes` and `target/test-classes`:
~120 lines of Rust (magic, pool count, skip entries by tag width —
`CONSTANT_Long`/`Double` take **two** slots — keep `Utf8` and `Class`, and
scan `Utf8` for `L<pkg>/<Class>;` so descriptor-only and annotation references
count). Sound for plain-Java tests. Blunt rules for Spring: any change to a
`@Component`/`@Service`/`@Repository`/`@Configuration` class, any new file
under the base package, any resource or migration change re-runs every
context-starting test. **Unknown ⇒ run.** Exclude `*IT` from the watch loop by
default; `--it` opts in. Print the count skipped and explain each selection;
`--since <ref>` takes the change set from `git diff --name-only`.

**The correctness price, stated plainly:** compiling only the changed file is
`useIncrementalCompilation=false`'s unsoundness — a removed method leaves a
stale caller and you get `NoSuchMethodError`. The index closes the common
case; `static final` constants javac inlines and annotation-processor output
stay unsound. **Which is why `jails check` stays `mvn clean verify`**, every
fast path falls back to it loudly, and the README says so in the same
paragraph. Do not make `check` incremental; the leftover-`.class` bug is real.

**`jails bench`** prints the ladder for *this* project on *this* machine
(Maven lifecycle / `surefire:test` / launcher / daemon / ±container reuse),
into `.jails/benchmarks/`, no telemetry. A tool whose pitch is speed should
prove its own numbers, and the README should promise no number it has not
printed.

### 6.3 `jails dev`

One process that starts the services a project's capabilities imply, starts
the app, watches the right files, compiles only what changed with a warm
`javac`, swaps or restarts — **and says which and why** — applies new
migrations, and prints one timed line per action.

1. **Watcher**: §4.6's replacement, 150–250 ms poll, 400 ms quiet period, plus
   Quarkus' extra 200 ms sleep when a file is size 0 (caught mid-write). No
   crate and **no inotify path at all** — a second code path that only runs on
   some machines only breaks on some machines.
2. **Compile**: `javac -J-XX:+AutoCreateSharedArchive
   -J-XX:SharedArchiveFile=.jails/javac.jsa --release N -cp <cached>:target/classes
   -d target/classes <files>` — **0.25 s instead of 1.45 s, measured**, versus
   `mvn compile` at seconds. Fall back to `mvn compile` loudly if javac is
   missing or the archive misbehaves.
3. **Classify before acting**, because stock HotSpot refuses everything except
   method bodies: a method-body change in a loaded class → **swap**; a record
   component, `sealed … permits`, an annotation, a new class, a field or a
   signature → **restart, printing the JVMTI reason by name**; `pom.xml` →
   full restart and classpath re-resolution. **jails' domain layer is records
   and sealed types, so every edit there is a restart** — a `jails dev` that
   promises "hot reload" without saying this looks broken.
4. **How to swap**, in order of cost: (a) leave it to the editor — jdt.ls's
   java-debug bundle already does `redefineClasses` **with frame popping**,
   zero jails code; (b) drive `jdb -attach` + `redefine`, which ships with
   every JDK; (c) a Rust JDWP client, a day, structured errors. Sequence c
   after a and b work. **Trap**: devtools' restart classloader can make one
   class name resolve to two `ReferenceType`s and JDI refuses — a run is
   *either* devtools-restart *or* JDWP-redefine. `new-cli` projects have no
   devtools, which is what makes jails-owned swap worth having at all.
5. Write `target/classes/.jails-reload` only after a **successful** compile and
   point `spring.devtools.restart.trigger-file` at it, so devtools never
   restarts into a half-written directory.
6. **Migrations**: a new file under `db/migration` is applied to the dev
   database immediately (`migrate.rs` has the psql path).
7. **Output**: pipe through `why::FATAL_MARKERS`; keep the last fatal match
   for `jails why --last` (the supervisor outlives the crashed app); print the
   routes table once at boot (`inspect.rs` computes it).
8. **Keys**, Quarkus' map: `r` re-run tests, `f` failing only, `m` re-apply
   migrations, `s` force restart, `q` quit. `stty raw -echo` through
   `process.rs`, no crate.
9. **`--timings` on everything.**

One free discovery worth checking before building any of this: **the
save→restart loop may already exist with no Maven in it.** jdt.ls imports
Maven through m2e, which sets the Eclipse output folder to `target/classes`;
devtools watches classpath directories; Spring's docs say saving in Eclipse
triggers a restart. So with `java.autobuild.enabled` and an app started by
`jails run`, `:w` → jdt.ls writes the class → devtools restarts in ~1.4 s,
with no `mvn compile` and no jails poll. If that holds here, `jails run --hot`
is a README paragraph and a doctor check rather than a supervisor. **Verify
where jdt.ls writes `.class` files in this setup first** — the whole thing
pivots on it.

### 6.4 `jails run --tc` — the honest "console with beans"

`mvn spring-boot:test-run` is a real goal since Boot 3.1. Paired with a
generated `src/test/.../TestApplication.java` doing
`SpringApplication.from(App::main).with(TestcontainersConfig.class).run(args)`
— and `@RestartScope` on the container bean so it survives devtools restarts —
it gives a dev run backed by the Testcontainers config **`add db` already
generates**, with no compose file at all. On this machine that also routes
around `spring-boot-docker-compose` being unable to drive podman-compose,
rather than working around it.

This replaces the "boot a Spring context inside jshell" idea in
`ideas-opus.md` A3. That design dies on the DataSource, tries to drive
podman-compose, and is slower than `jails run`. The honest pair is:

- **`jails console` keeps its truthful banner** — jshell plus this module's
  classpath, not a Spring context — and gains a generated `startup.jsh`
  (`import module java.base;`, the base package, AssertJ). ~15 lines.
- **`jails runner -e '<expr>' | <file> | -`** and `g script <Name>` boot the
  context and `getBean` for the non-interactive case, which is most of what
  `rails runner` is used for and 80% of "console with beans" for a tenth of
  the work.
- **`jails boot`** = `-Dspring.context.exit=onRefresh`: boots past singleton
  creation and `afterPropertiesSet`, before `Lifecycle` start and the HTTP
  port, then exits. A startup smoke test with no port and no compose — and the
  AOT training-run switch, so one mechanism serves two features.

### 6.5 Where AOT actually belongs

`jails build --extract` (`java -Djarmode=tools -jar app.jar extract`, ~15.7%
off start and the prerequisite for the cache) and `add aot`, which trains on
the **extracted** jar with `-Dspring.context.exit=onRefresh`, uses
`-XX:AOTMode=auto` (never `required` — a stale cache should degrade, not
refuse to boot), gitignores the cache, and has `doctor` flag one older than
any jar. Deploy-time and short-lived workers only. Note a JVM agent and an AOT
cache are mutually exclusive.

### 6.6 Explicitly out of the dev loop

CRaC (no CRaC JDK installed; vendor-only; the privilege story under rootless
podman is untested; **the checkpoint is a memory image containing every secret
the JVM saw**). JBR/DCEVM/HotswapAgent/JRebel (§3). The AOT cache (§3). Maven 4
(4.0.0-rc-6, not GA, and nothing in it moves the inner loop). The Maven build
cache extension (it would restore a `target/` that `clean` just deleted).

---

## 7. Tier 3 — safe mutation, and exactly how much of it you need

`ideas-sol.md` Bet 2 wants a universal `ChangeSet` engine, a provenance lock,
a journaled crash-recoverable apply and `jails status` — 3–5 weeks — **before**
field evolution. `ideas-opus2.md` §5.10 says `edited_files` plus "print the
snippet, never clobber" is correct today and does not block on it.

**Resolution: ship §5 on `edited_files`; build provenance when `app apply`
needs drift reconciliation, not before.** The reasoning:

- The failure `edited_files` cannot prevent is *over-reporting* — it prints a
  snippet you paste. The failure a bad ledger causes is *under-reporting* — it
  overwrites work. Over-reporting is the safe direction, so the cheap oracle
  is not merely acceptable, it fails the right way.
- The one thing a hash-based ledger buys that `edited_files` cannot is
  recognising an untouched file generated by an **older jails** — real, but
  paid once per upgrade, and paid in extra printed snippets rather than lost
  code.
- Provenance becomes load-bearing the moment `app apply` must reconcile a
  changed manifest line against a file on disk (§5.9), which is the natural
  trigger to build it.

**One structural note for whoever does build it:** `write_new_file` is *not*
the single choke point it looks like. `src/add.rs:325-333` writes an existing
path directly with `fs::write` after `normalize_imports`, bypassing the
collision check and `package-info` planning. A ledger hung off `write_new_file`
alone would have a hole exactly where a capability updates a file it
previously wrote.

When it is built, keep these properties and drop the rest: paths normalised,
relative and confined to the project; all conflicts detected before the first
persistent write; `--pretend` and apply rendering the **same** object; expected
hashes rather than best-effort string matching; a second identical apply is a
no-op. **A sequence of per-file renames is not an atomic multi-file
transaction** — promise deterministic preflight plus crash recovery, and say
so, rather than implying a rollback that did not happen.

Also worth extracting on the way, as a refactor rather than a feature:
**`src/codemod.rs`**, collecting the six splice primitives jails already has
(`pom::add_dependency`/`add_plugin`, compose marked blocks, property blocks +
`exposure_include`, `register_command`, `install_test_container_import`, the
`jails.toml` one-liner) under named operations. Same extraction as
`process.rs`, for the same reason, and it pays on every capability.

---

## 8. Tier 4 — reach: the codebase you did not create

Run, in `ideas/minicom-public/spring` and `ideas/monzo-crawler2/app`:

```
$ jails about
jails: no pom.xml found in this or any parent directory
```

**Zero of ~30 commands work.** The gate is one function —
`generate::find_project_root`, 11 lines at `src/generate.rs:86-96`, confirmed
today, walking up looking for `pom.xml` and nothing else — with ~30 call sites
and three further copies of the rule (`project::nearest_pom`,
`project::workspace_root`, and `root_markers = { 'pom.xml' }` in the Lua).

The decisive experiment: dropping a **one-line stub `pom.xml`** into a copy of
the Gradle Intercom stub makes `routes`, `beans`, `stats`, `notes`, `rename
--dry-run`, `destroy --pretend`, `doctor` and `g record`/`g controller` all
work correctly against the Gradle sources. `jails routes` printed
`POST /bar BarController#verify`. `src/inspect.rs` and `src/rename.rs` contain
**zero** occurrences of the string `pom` — their entire Maven dependency is
the root-finding call.

**The change:** widen the marker list in that one function and return *why* it
matched.

```rust
pub(crate) enum Build { Maven, Foreign(&'static str), Bare }
pub(crate) fn find_project_root() -> Result<PathBuf>   // signature unchanged
pub(crate) fn project_build(root: &Path) -> Build      // new
```

Checked per directory while walking up so the nearest wins: `pom.xml` →
`Maven`; `build.gradle{,.kts}` / `settings.gradle{,.kts}` → `Foreign`;
`jails.toml` → `Bare`. A `pom.xml` beside a `build.gradle` still wins.

Then three guards so the degraded mode is honest rather than lying:

- `pom::read` is the funnel for every command needing pom *content*; on ENOENT
  it consults `project_build` and says "this project is built by Gradle
  (build.gradle); jails only edits pom.xml".
- The eight Maven-inherent commands (`test build clean fmt check mvn run
  console`) get a one-line `require_maven` guard.
- **`doctor` reports the real build tool.** Not optional: with a stub pom,
  doctor today prints `9 checks, all clear` over a Gradle Boot 2.7 project —
  and a confident wrong report is worse than a refusal.

**Frame it correctly in README**: *jails never reads, writes, parses or invokes
`build.gradle`. It stops treating `pom.xml` as the only thing that marks a
project root.* That is strictly less than Gradle support, and the "no Gradle"
constraint has been applied to a case it does not cover — at the cost of
leaving 100% of the tool unreachable in the one codebase you are handed on
interview day.

Three caveats the experiment surfaced:

1. **The stub-pom trick changes the Java jails emits.** Without a readable
   pom, `repository_wiring` returns `PlainJdbc` and `jspecify_available`
   returns false, so no `package-info.java` and a different adapter shape. A
   degraded mode must *say* which shape it chose.
2. **`add` still will not work** — `require_java_release` hard-errors without
   `<maven.compiler.release>`. It should not be exempted; `add`'s whole job is
   a pom edit.
3. **Multi-module Gradle breaks the Maven-shaped assumption**: in
   `monzo-crawler2`, `build.gradle` is in `app/` while `settings.gradle` is a
   directory above. Rooting at the nearest marker is right for `generate` and
   wrong for `workspace_root`. Pick per command and write the test.

### `jails adopt`

Once the root resolves, `g record` in the Intercom stub lands in
`com.intercom.spring.domain` — a package that project does not have. Its real
packages are `models` and `controllers`.

**The placement engine for this already exists**, verified end to end: writing
`[layout] web = "controllers"` / `domain = "models"` into the stub made `jails
stats` report `Web 2` (previously `Other 4`) and put generated files in the
right directories, with no code change.

So `jails adopt` writes no new machinery — it writes that file. Resolve the
base package (`base_package` already falls back to the shallowest `.java`),
enumerate the immediate subpackage directories, map them onto
`config::LAYERS_IN_ORDER` through a small **closed** synonym table
(`model|models|entity|entities|domain → domain`,
`controller|controllers|web|rest → web`, …). A directory matching nothing is
**reported, not guessed**; a layer with two candidates is reported and left
unmapped, because choosing between `model/` and `domain/` is exactly the
silent-wrong-placement failure the command exists to prevent.

It must **never** write `[project] capabilities` — that table means "what
`add` installed and `sync` should restore", and in a foreign project jails
installed nothing.

---

## 9. Tier 5 — the capability set, inside the domain-blind line

`src/app.rs:1-6` rules out a `spider` kind, an `inbox` kind and anything else
named after a showcase. Three of the six documents propose exactly those. What
survives that doctrine, and why:

- **A capability is inside the line when it knows no domain noun.** `add
  crawl` (jsoup + `Urls` + `Robots` + `Fetcher`) is as domain-blind as `add
  csv` — it knows no seed URL, exactly as `add kafka` knows no topic. `add
  cors`, `add sse`, `add queue`, `add mail`, `add auth`, `add storage` are all
  the same shape.
- **Traversal, assignment and extraction arrive as generic intents** —
  `usecase`, `query`, `event`, `durable-job`, all four of which now **exist**;
  the friction ledger's commitment has been kept.
- **The bug list is the acceptance criteria** regardless of which command
  emits the code.

Two of the capabilities the source documents propose have already shipped:
**`ci`** (pinned, least-privilege workflows) and **`docker`** (a non-root
multi-stage image deriving Java from the POM). Both proof apps select them.
So the list below is what `examples/ACCEPTANCE.md`'s "still open" paragraph
names, and nothing else — ordered by what is blocking a proof app today, not
by interest:

### 9.1 `add cors` — the actual blocker, and nobody mentioned it

`grep -rni cors src/ templates/ README.md` returns **nothing** — verified
today. And `templates/spring/security_config_java.java` has
`anyRequest().authenticated()` and never calls `.cors(...)`, so `add security`
leaves no CORS configuration and the preflight is handled by the security
chain. **A jails-generated Spring app plus `add security` cannot serve a
browser widget.** The minicom exercise is inherently cross-origin — foo on
:8008, bar on :8009, both POSTing to :3000, three distinct origins.

The naive fix is wrong in a way that bites later:
`CorsConfiguration.applyPermitDefaultValues()` — what a bare
`addMapping("/**")` gets you — permits only GET, HEAD and POST and does not
allow credentials. That is the classic "works until mark-as-read becomes a
PUT" failure. `add cors` must name the methods explicitly, put the origins in
a marked properties block, and **wire `.cors(...)` into the generated security
chain in the same change.**

Two doctor checks fall out, both seen in the corpus: `@EnableWebMvc` in a
project with the Boot webmvc starter (switches off Boot's auto-configuration;
static resources stop being served), and `addMapping("/**")` with no
`allowedOrigins`.

### 9.2 `add sse` — with the four details both SSE designs get wrong

`SseEmitter` is alive and undeprecated in Framework 7; both documents pick it
and both are right about that.

- **The never-time-out value is `-1L`** (or `0L`), not `Long.MAX_VALUE` — §3.
- **`onCompletion` alone suffices for removal** ("called when an async request
  completed for **any** reason including timeout and network error"). One
  document prescribes three callbacks, the other prescribes "complete on
  IOException" which `send()`'s javadoc explicitly calls unnecessary. The
  requirement both **miss** is the real one: `onCompletion` runs on a
  *container* thread, concurrently with whatever thread is broadcasting, so
  the registry must be `ConcurrentHashMap<K, Set<SseEmitter>>` with
  `newKeySet()` — not a synchronized list.
- **`spring.task.scheduling.pool.size` defaults to 1.** A
  `@Scheduled(fixedRate = 15000)` heartbeat blocking on one dead client stalls
  **every other scheduled job in the application**, and nothing logs it. Raise
  the pool or set `spring.threads.virtual.enabled=true`, and say which in the
  Javadoc.
- **`Last-Event-ID` is not implemented by Spring** — zero matches across
  spring-web and spring-webmvc. Spring gives you `SseEventBuilder.id()` on the
  way out and reads nothing on the way back in. A hub that emits `id()`
  without a `@RequestHeader("Last-Event-ID")` replay path is advertising
  resumability it does not have. The replay itself is visible SQL:
  `select … where topic = ? and id > ? order by id`.

One Framework-7-only fact that makes "SSE + virtual threads" a real
recommendation rather than a slogan: Framework 7 replaced `synchronized` with
an explicit `ReentrantLock` throughout `ResponseBodyEmitter` specifically to
avoid virtual-thread pinning. On 6.2 the same hub pins the carrier thread on
every `send()`.

Test over a real port with `RestTestClient` (`MockMvc` has no connector), and
a `CountDownLatch`, never a sleep.

### 9.3 `add queue` — the backbone both verticals silently assume

Rails 8 moved the job queue into the database because requiring Redis to send
an email later was the wrong shape for small apps. jails has `add redis` and
no queue; `g job` is a *timer*. Mailer delivery, webhook re-delivery, crawl
frontier persistence and digest jobs each re-derive the same five mistakes.

One `jobs` table (jsonb payload, `state in (ready,running,done,dead)`,
`run_at`, `attempts`, `max_attempts`, `locked_at/by`, `idempotency_key`), with:

- a **partial index on `state='ready'`** so the claim stays O(1) as `done`
  rows accumulate, and a unique partial index on the idempotency key;
- **enqueue in the caller's transaction** — the adapter takes the caller's
  `Connection`, exactly as the scaffold's does. This is the property a Redis
  queue cannot give, and it is the reason the DB queue is the right default;
- **claim** with `select … for update skip locked limit ?` inside a CTE,
  `order by run_at, id`, and `attempts` incremented **at claim** — so a
  SIGKILLed worker has burned its attempt and an OOM-killing job cannot loop
  forever;
- **a reaper** every minute for `running` rows with a stale `locked_at`.
  Without it a `kill -9` leaves a job `running` forever and nothing logs it.
  **This, not `SKIP LOCKED`, is what is usually missing.**
- full-jitter backoff capped at an hour, `dead` at `max_attempts`, never
  dropped, never swallowed;
- a worker with the semaphore acquired **before** claiming, virtual threads,
  `LISTEN` on a dedicated non-pooled connection with polling as the durable
  fallback, and `server.shutdown=graceful`.

`g worker <Name> --queue <q>` writes a handler; a generated `JobWiringTest`
fails the context when two handlers claim one queue — the "missing
`@Component` means the list is silently short" failure `g strategy` already
teaches. `jails queue list|failed|retry` shells through the `console.rs` psql
path, mirroring `jails kafka`. Name `db-scheduler` in the Javadoc as the
graduation path.

Tests (Testcontainers + Awaitility): two workers × 100 jobs = exactly 100
runs; a poison handler retries then goes `dead` with `last_error`; `dead` is
never claimed; shutdown completes in-flight work.

**Sequence this before `g mailer` and before any durable crawl frontier.** A
mailer that sends synchronously in the request is the tutorial version; a
queue that retries is the product version; the difference is one capability.

### 9.4 The rest, with the trap each one exists to remove

| Slice | The silent failure it prevents |
|---|---|
| **`g auth`** (Spring Security 7 `NimbusJwtEncoder.withSecretKey`, HS256, secret from a property) | **Boot 4 auto-configures no `JwtEncoder` at all** and configures a `JwtDecoder` only from `jwk-set-uri`/`issuer-uri`, so 100% of symmetric mint/verify wiring is hand-written. And **a JWT with no `exp` passes the default decoder** — call `setAllowEmptyExpiryClaim(false)` and add issuer/audience validators explicitly. Use `spring-boot-starter-security-oauth2-resource-server`; the un-prefixed one is deprecated in Boot 4. Test `alg: "none"`. |
| **`g webhook --strategy hmac_sha1_hex\|hmac_sha256_hex\|stripe_v1`** | Signature over **raw bytes** (`@RequestBody byte[]`), never re-serialised JSON; `MessageDigest.isEqual`; Stripe's 300 s tolerance mandatory; 401/202. Closed strategy set — a passthrough algorithm name is a string jails cannot validate. Test a body with a non-ASCII character. |
| **`add mail`** | `spring-boot-starter-mail` + its `-test` twin (Boot 4 convention: splice `X-test` with every `X`), Mailpit in compose, the IT reading mail back over **POP3** as Boot's own test does, and `Mailer.send` enqueuing through `add queue` when present. No `@ServiceConnection` factory exists for SMTP — use a `DynamicPropertyRegistrar` and say why. |
| **`g search <Entity> --on body,subject`** | A `generated always as (…) stored` `tsvector` column plus a GIN index — a *generated* column, because a trigger someone forgets to fire on UPDATE is the silent failure. `websearch_to_tsquery` (does not throw on user input), `ts_rank_cd`, keyset pagination. Validate `--on` against the record's components before writing anything, same rule as `--index`. 30 lines of visible SQL; no search service. |
| **`add crawl`** | `Urls` (canonicalisation; the dedup key *is* the value), `Robots` via `crawlercommons` (4xx ⇒ allow all, **5xx or unreachable ⇒ disallow all** — the rule everyone gets backwards), `Politeness` (per host: `Semaphore(1)` + next-allowed instant; **acquire the permit first, then sleep the gap** — the Go reference does it backwards and collapses its own concurrency), `Fetcher` (redirects re-scoped per hop, `Retry-After` on 429/503, retry only 5xx/`IOException`, body cap, `HttpClient` is `AutoCloseable`), `Frontier` (`newKeySet().add` **is** the dedup — no separate `contains()`; termination is `enqueued == completed`, **never** `queue.isEmpty()`), `Crawler` (virtual threads + a semaphore acquired on the submitting thread, `completed` in `finally`), and a `FakeSite` on `com.sun.net.httpserver` for tests. Clone jsoup and crawler-commons into `deps.tsv` **before** writing a line of template — CLAUDE.md's rule. Note `deps-update.sh` reads the manifest with `IFS=$'\t' read`, so append rows with `printf`, not `echo`. |
| **`g timeline <Name> --parts …`** | The generator neither vertical document has and the Intercom data model needs: an append-only part table, a `Projection.fold(parts)` **switch with no `default`** so adding a part type stops the build until its effect on state is decided, and the denormalised summary row updated **in the same transaction**. Prevents the inbox saying "open" while the timeline says closed. A plain scaffold would generate `update … set assignee = ?` and lose the audit trail. |
| **`add flags`**, **`add shedlock`**, **`add storage`**, **`add arch`**, **`add nullcheck`**, **`add ci`**, **`add docker`** | Each is specified in the source documents and each is real; build them the first time a project needs one, not before. `add shedlock` earns its place on the classic silent failure — two instances both fire the 02:00 job, customers get two emails, nothing logs an error. |

The five crawler bugs, read out of the three reference solutions in `ideas/`,
are the acceptance criteria for whatever shape wins: check-then-act dedup; the
raw URL as the visited key; fused fetch+parse (the parser cannot be tested
without a network); a latch counted down on one path only; **relative links
silently dropped** because `Jsoup.parse` was called with no base URI while the
tests used only absolute hrefs; and zero robots.txt implementations across all
three. Three assertions carry the design: exactly one *request* per canonical
URL (`verify(1, getRequestedFor(...))`, stronger than "visited once"),
`assertTimeoutPreemptively` (pins the in-flight counter and fails the moment
anyone moves the decrement out of the `finally`), and zero requests to a
robots-disallowed path.

### 9.5 The UI decision

An agent inbox needs HTML, and today a jails project can only be an API — a
take-home that renders nothing is half a submission. The recommendation is
`g page <Name>` with **JTE** + htmx + `add sse`: JTE templates are compiled,
type-checked Java, so a renamed record component breaks the build rather than
the request — which is jails' bar — and its development mode runs its own
watcher, so template edits are sub-second with no context restart and no
dependence on devtools LiveReload (deprecated in Boot 4.1 with no
replacement). One layout, one fragment convention, no asset pipeline. The
widget stays hand-written static JS; jails does not own frontends.

---

## 10. Tier 6 — the editor

Most of this is configuration in `~/code/my-dotfiles`, not Rust, and it is the
cheapest tier in the plan.

**Navigation via projectionist, not a Lua reimplementation.**
`tpope/vim-projectionist` fires `User ProjectionistDetect` and accepts
projections **in memory** via `projectionist#append(root, projections)` —
nothing written into the repo, which dissolves the "don't leak editor config
into the Java project" objection. On detect, run `jails about --json` once per
root, build the table from `layout` + `base_package`, append. You get
`:A`/`:AS`/`:AV`/`:AT` with a **list of alternates, first readable wins**
(controller → its test, else its service; JDBC adapter → its IT, else the
in-memory one, else the port), `:Econtroller` `:Eservice` `:Erepo`
`:Emigration` `:Etest` `:Efixture` with completion, `path` (so `gf` works),
`make: jails`, `dispatch: jails test`, `console: jails console`, `start: jails
run`. Several hundred lines of Lua not written and not tested.

**`about --json` v2** is the prerequisite: add `layout` (through
`Config::layers()`, i.e. *renamed* values — the drift `inspect.rs` already
suffered once), `base_package`, `capabilities`, `java_root`/`test_root`, with
a test pinning the keys to `config::LAYERS_IN_ORDER`. Normalise the version
key while there: `about` uses `schema_version`, `routes`/`beans` use
`version`. Add `line` to `Route`/`Bean` (`java.rs`'s `blank_range` already
preserves newlines for exactly this) — without a line, `routes --json` is a
list; with one it is a quickfix and a picker source.

**`gf` into JDK and project source — six lines, no plugin.** Neovim's own
`ftplugin/java.vim` already sets `includeexpr`, `suffixesadd`, `include` and
`define`, and honours `g:ftplugin_java_source_path` (a `.zip` gets a
`JavaFileTypeZipFile()` `includeexpr`). Only `'path'` is missing:

```lua
vim.opt_local.path:prepend({ root..'/src/main/java', root..'/src/test/java' })
vim.g.ftplugin_java_source_path = (vim.env.JAVA_HOME or '~/.local/share/jdk/jdk-27')..'/lib/src.zip'
```

That is "`gf` while jdt.ls is cold" by configuration instead of code, plus
`[i`, `:ilist`, `:dsearch` for free.

**jdt.ls settings and bundles.** `ftplugin/java.lua` sets no `init_options`
and no `settings.java.configuration`. Add `updateBuildConfiguration =
'automatic'` (default is `'interactive'`, which is why **every `jails add`
leaves red squiggles until prompted**), `autobuild.enabled = true` (§6.3
depends on it), `downloadSources`, `-Xmx2G`. Then the two bundles —
**java-debug** and **vscode-java-test** — which give
`jdtls.dap.test_nearest_method()` (one test method, no Maven, no Surefire, on
the already-compiled workspace) and **hot code replace with frame popping**.
Pair with `jails run --debug` appending
`-agentlib:jdwp=…,address=127.0.0.1:5005` and setting
`spring.devtools.restart.enabled=false` for that run, since a devtools restart
kills the JDWP session.

**`:compiler jails`.** `$VIMRUNTIME/compiler/maven.vim` already carries javac
errors with and without columns, non-parseable POM, and the Surefire
multi-line `<<< FAILURE!` … `at Foo.bar(Foo.java:42)` pattern.
`jails.nvim/compiler/jails.vim` is ~15 lines: `makeprg`, that `errorformat`
copied verbatim (the `current_compiler` guard makes `runtime!` bite), plus
`%-Gcreate\ %f`. `why`'s explanation goes in the quickfix **title** and a
`vim.notify`, not as entry text.

**Pickers**: `fzf-lua` (this config has no telescope) over `routes --json` /
`beans --json`, jumping to `source:line`. Sub-50 ms on a project that does not
compile — jdt.ls cannot do that.

**`jails src <Type>`**: resolve a project type, else a type under `deps/`
(filename stem, then `^public (class|interface|record|enum) X\b`), print
`file:line`. `deps/` is a real checkout with `git log`; `gd` through jdt.ls is
a source-jar download of possibly another version. Have `why`'s `fix:` lines
cite it, which is how `deps/` stops being a jails-developer trick and becomes
a jails-user one. Doctor may WARN when `deps/` is absent **only** when
`JAILS_DEPS` is set, so a machine that never cloned deps is not nagged.

**Keymap collisions**: `<leader>j{t,c,r,b,g}` (ftplugin) versus
`<leader>J{t,c,r,b,g}` (jails) — a shift-key slip turns "extract constant"
into `mvn clean verify`. Make the split semantic: `<leader>j` = this buffer /
language server, `<leader>J` = the project / jails. Point `jt`/`jf`/`jm` at
`jails test` (one Maven resolver — `about --json` already emits
`maven_command` and nothing reads it) or better at `test_nearest_method()`.

**`javac_lint` on `BufWritePost`** has three problems: it recompiles the
**whole** `src/main/java` on every save; it runs bare `javac` with **no
`--release`**, i.e. JDK 26 semantics against a release-27 project; and it
re-runs `dependency:build-classpath` on every pom mtime change, so the first
save after any `jails add` pays a second Maven run on top of jdt.ls's
re-import. Pass `--release`, resolve `javac` from `JAVA_HOME`, put the autocmd
behind `vim.g.jails_javac_on_save`, and keep its output **out of**
`target/classes` — that is what stops it triggering devtools.

---

## 11. Tier 7 — the agent as second user

`java.md`, `spring.md` and `backend.md` are ~70 KB of hand-written personas
whose whole purpose is stopping a model from writing Boot 2 / JPA / Lombok /
`@MockBean` code, and `CLAUDE.md` is 40 KB of the same for this repo. **A
generated project inherits none of it.**

1. **`jails new` writes `AGENTS.md` into the project** — and the banned-API
   list in it is *rendered from* the same table `jails lint` matches against,
   so it cannot drift into a lie. That is the whole trick: a hand-written
   AGENTS.md is a `validation/README.md` waiting to happen. Content: the
   project is jails-managed; use `jails test <Name>`, not `mvn test`; `jails
   check` is the gate and *why*; `jails doctor` before debugging the
   environment; records, no Lombok, no ORM; the layer table and a pointer to
   the field-spec grammar. Claude Code, Cursor and codex all pick it up with
   zero wiring.
2. **`jails lint`** — a closed rule table over the stale-API families jails
   already knows about (`@MockBean`, `javax.validation`, Jackson 2 alongside
   3, `spring-boot-starter-web`, `@Entity`, Lombok, preview features).
   Sub-second, exit 1, `file:line`. It turns a six-minute compile-read-fix
   loop into a check an agent can run before handing back. `doctor` already
   does the Jackson-majors version of this, so the rule shape is proven.
3. **`--json` everywhere.** Only `about`, `routes` and `beans` have it today.
   `doctor --json`, `why --json`, `test --json` (one object per `testfeed`
   event), `stats`, `notes` are each an afternoon and each removes a parsing
   step from both the editor and the agent. **`why --json` is the highest
   value** — it makes the *explanation* available as quickfix text, and
   `why.rs` already stores exactly `{signature, explanation, fix}`.
   `jails commands --json` (a walk of the clap tree) then deletes the
   hand-copied Lua lists rather than pinning them.
4. **`jails explain <kind>`** exposes the design rationale the Javadoc and
   CLAUDE.md carry, so an agent stops "fixing" `@Repository` onto the second
   adapter.
5. **Promote `g cases`.** It turns a markdown brief's acceptance bullets into
   a test class. It is the spec-first workflow both you and an agent want, it
   is implemented, and §4.9 says nobody can find it.

Two things to say no to. **An MCP server is worse than the CLI an agent
already shells to** — it adds a process, a schema to keep in sync, and a
failure mode in headless runs, for no capability the CLI lacks. (If it is ever
wanted: ~600–800 lines, zero crates, and it needs a JSON *parser* jails does
not have.) And **no LLM inside jails**: deterministic generation is the
product, and the moment a generator asks a model for a selector you cannot
golden-file it and you cannot destroy it.

---

## 12. Anti-goals — the union, with reasons

| Temptation | Why not |
|---|---|
| A plugin system, `jails recipe intercom`, a template DSL, pack catalogs | README "Not yet". `.jails/app.toml`'s closed `[[generate]]` schema is the answer, and it is already close to the line — a closed schema of existing kinds is defensible, an ordered list of arbitrary user intents a compiler expands is the thing being deferred |
| Gradle *support* | Distinct from Gradle-directory *tolerance*, which is §8 and is worth having |
| ORM, JPA, `JpaRepository`, an Active Record clone, lazy loading | SQL stays derived and visible; §5.2 is a column and a foreign key |
| Django-style `makemigrations` autodetect | Requires models to own the schema — ORM thinking through the back door |
| A `jails-support` runtime jar | ActiveSupport lock-in; capabilities write classes *into* the project |
| Lombok | Editor tax on modern JDKs; records exist |
| Preview features, string templates, `StructuredTaskScope`, `LazyConstant` | All still preview at 27; string templates were withdrawn. Virtual threads and `ScopedValue` are final and are the concurrency story |
| Wrapping crawler4j / webmagic / Nutch / StormCrawler / spider / crawl4ai | Dead, or platforms, or a second runtime. Generate the types |
| `jails crawl <url>` as a subcommand | jails scaffolds crawlers; it is not one |
| A `spider` / `inbox` / `conversation` kind, enum or template in core | `src/app.rs:1-6` |
| A Rust `jails lsp` | ~1.2–1.8k lines, a JSON parser, a second server fighting jdt.ls over `.java` buffers, and the pressure that turns `java.rs` into the parser CLAUDE.md forbids |
| Migrating to `nvim-java`; neotest; snippets generated from `templates/*.java` | Each discards a correct existing fix, or duplicates `jails g class` with something worse |
| The AOT cache in `dev`/`test`; CRaC; JBR/DCEVM/HotswapAgent/JRebel | §3 |
| Maven 4, the build cache extension, `useIncrementalCompilation=false` in generated poms | Not GA / restores a `target/` `clean` just deleted / trades ~200 ms for stale-dependent `NoSuchMethodError` |
| Making `jails check` incremental | The leftover-`.class` bug is real. The fast path is `jails test`, loudly documented |
| `TESTCONTAINERS_RYUK_DISABLED` by default | Buys 0.45 s, loses all crash cleanup |
| Booting a Spring context inside jshell | Dies on the DataSource, tries to drive podman-compose, slower than `jails run`. `jails runner` and `--tc` are the honest designs |
| Web UIs (Telescope / Horizon / Dev UI analogues) | `why`, `jails queue` and structured events are the terminal-honest half |
| Redis required for the queue; Elasticsearch for v1 search | §9.3, §9.4 |
| Auto-fallback to a vendored `new` on network failure | Explicit `--offline`; silent mode switches are how "works on my machine" happens |
| Treating a skipped test as coverage | 11 of 104 integration tests do nothing on this machine and the suite still says green — `JAILS_REQUIRE_TOOLCHAIN=1` exists for this |
| Any generator whose failure mode is loud | A loud failure is one the compiler already reports |

---

## 13. The sequence

Effort is in jails-shaped days: golden scenario, README, nvim list, enum entry,
no plugin. **S** < 1 day, **M** 1–3 days, **L** > 3 days.

The **Proves** column is the point: it names the proof app whose acceptance
contract the item closes, so nothing on this list is here because it sounded
good. **A** = web crawler, **B** = support inbox, **C** = the ledger CLI
control, **—** = infrastructure for the plan itself.

| # | Item | § | Effort | Proves | Why here |
|---|---|---|---|---|---|
| 0 | **Stand up App C** (`examples/ledger-cli/.jails/app.toml` + its acceptance contract + a gate cell) | §2.5 | S | C | Costs an afternoon and immediately makes 4.1 a failing gate instead of folklore. It is also the only thing that can falsify "generic" along the Spring axis |
| 1 | Tier 0: all nine defects + the `mvn validate` matrix + the Lua-pinning test | §4 | 1 day | A B **C** | Features land on top of it. Two of them make jails write a broken project, and C hits one on its first run |
| 2 | Editor config: jdt.ls settings + bundles + HCR, `'path'`/`src.zip`, `:compiler jails`, keymap split, `javac_lint --release`/opt-in | §10 | S, no Rust | — | Test-at-cursor, debugging, `gf` and quickfix — today, with no Rust written |
| 3 | Testcontainers reuse + doctor + `jails setup`; `spring-devtools.properties`; `mise.toml` from `new` | §6.1 | S | A B | −8 s per container test. The full gate takes 196 s today; most of it is containers |
| 4 | `jails test` flags: `Class#method`, `path:line`, `--failed`, `--fail-fast`, `--slowest`, rerun snippet, `failIfNoSpecifiedTests=false` | §6.1 | S–M | — | Every test run, including every gate run |
| 5 | `why` on every Maven failure; `why --json`; doctor/why additions | §6.1, §11 | S | A B C | Multiplies every rule already in the table; the gate's failures become readable |
| 6 | **The inflector**, **scaffold reads the record off disk**, **refusal messages with `fix:`** | §5.3, §5.4, §5.8 | S each | A B | Half-day changes, all three visible in the first minute of a demo |
| 7 | **`g field`**, then **relations** | §5.1, §5.2 | M each | A B | The two that move jails from "makes a project" to "grows a project". `g field` first — §5.9 means `app apply` needs it before the manifest can become editable. Relations unblock B's "tenant enforcement against every persisted association", which is its largest open clause |
| 8 | `--timestamps`, `g factory`, `requests/*.http` | §5.5–§5.7 | M total | A B C | Ride the same generator surface; do them together. Both manifests hand-declare `createdAt`/`discoveredAt` today |
| 9 | `about --json` v2 + line numbers; projectionist; fzf-lua pickers; `jails src` | §10 | M | — | `:A`/`:E*` and routes/beans jump — 50–200 keystroke-saves a day |
| 10 | `jails test --fast` (console launcher + classpath cache) + `jails bench` | §6.2 | M | C first | 2.5 s → ~0.5 s, and the first thing that proves its own number. Measure on C, which has no containers to hide behind |
| 11 | `add cors`, then `add sse` | §9.1, §9.2 | S, M | B | `cors` is small and is what currently makes a jails app unusable from a browser widget — B's whole point |
| 12 | **Already in flight** — a generic `transition` kind (`generate.rs:855-872`, `spring::transition_files` not yet written, so the tree does not compile). Finish it | §2.4 | M | B | B's `version` column exists and nothing checks it. One generic kind closes a named acceptance clause — and it is the method working as designed |
| 13 | `add queue` (transactional outbox, reaper, stable delivery ID) | §9.3 | L | B, then A | B's "transactional outbox / provider delivery" clause, and later A's durable frontier. Before mailer and webhook retry |
| 14 | `add crawl` (+ `deps.tsv` rows, cloned first) and the traversal `usecase` | §9.4 | L | A | A's largest open clause: composing the shipped `fetcher` into finite traversal, plus robots and cancellation. After #4 and #10 |
| 15 | §8 marker widening + `jails adopt` | §8 | M | — | Changes what the tool *is*: from "makes new projects" to "you can bring it to a codebase" |
| 16 | `jails testd` + `--affected` | §6.2 | L | — | The biggest single number (35×/345×) and deliberately not first — it has the one unmeasured piece, and an 11 ms test loop is worth little on a model you cannot change |
| 17 | `jails dev` v1: watcher, `javac`+CDS, trigger file, `why` piping, migrations, keys, timings | §6.3 | L | — | After #10/#16 prove the primitives |
| 18 | Provenance / `ChangeSet`, `jails status`, `codemod.rs` | §7 | L | A B | Exactly when `app apply` must reconcile a changed manifest line — friction-ledger rows 3 and 4 |
| 19 | `g auth`, `g webhook`, `add mail`, `g search`, `g timeline` | §9.4 | M each | B | In that order; each waits for a real acceptance clause |
| 20 | `AGENTS.md` + `jails lint` + `--json` everywhere + `g cases` in README | §11 | M | — | Every agent session |
| 21 | `new --offline`, `app init`, local OCI build in the gate, hosted CI as a required check | §2.8 | S–M | A B C | The three friction-ledger rows in the harness flow, plus two open delivery clauses |
| 22 | `add aot`, `jails ship`, `jails upgrade`, `add arch`, `add nullcheck`, `--module`, `g load`, `jails recap` | §6.5, §9.4 | S–M each | — | As they come up |

Items 0–6 are roughly a week of mostly small changes and they change the feel
of every subsequent hour. Items 7–8 are the ones you pay for on **every single
model change**. Item 16 is the biggest number and is correctly late.

**The stopping rule:** when a proof app's acceptance clause is closed, stop
working on that capability. `ACCEPTANCE.md` says the gate may report
`generated`, `configured`, `user-owned` or `not selected` and **must never
call an unproved property guaranteed or production ready** — that sentence is
what keeps this list from growing forever.

---

## 14. Measure before promising

These are the numbers a feature in this plan rests on that have **not** been
measured on this machine. `jails bench` exists to answer the first four.

1. Console-launcher wall time here (estimated 0.35–0.6 s) and the resident-JVM
   band (estimated 50–150 ms).
2. The cost of a fresh `URLClassLoader` per `testd` run — the one unmeasured
   piece of the 110 ms figure.
3. How many distinct Spring contexts a jails scaffold's suite actually builds
   (`missCount` under `org.springframework.test.context.cache=DEBUG`). 1, 2 or
   3 decides whether context accounting is worth anything.
4. `postgres:17` with reuse under podman: confirm ~0 s on the second run, and
   confirm `withReuse(true)` does not disturb the `@ServiceConnection` wiring.
5. **Does a `@SpringBootTest` with Testcontainers succeed on this machine
   today?** `DOCKER_HOST=unix:///run/user/1000/podman/podman.sock` is exported
   and `podman.socket` is active, which contradicts the documented gotcha in
   CLAUDE.md. Re-verify before designing around it:
   `JAILS_REQUIRE_TOOLCHAIN=1 JAVA_HOME=~/.local/share/jdk/jdk-27 cargo test`.
6. Where jdt.ls writes `.class` files in this setup — `target/classes` via m2e
   is the expectation, and devtools prints the classpath at startup. **The
   whole §6.3 "the loop already exists" finding pivots on it.**
7. Whether `CSVFormat.Builder.build()` still exists at commons-csv 1.14.1 —
   CLAUDE.md says it was *renamed* in 1.13; the javadoc says *deprecated*. Add
   the checkout to `deps.tsv` and settle it.
8. The newest `gg.jte:jte-spring-boot-starter-4` and the released coordinates
   for jsoup and the split WireMock artifacts.

### The decision that was already made, and what it unblocks

Three of the six source documents argued that `pom::TARGET_RELEASE = 27` was
wrong: JDK 27 is non-LTS, was not GA until 2026-09-15, had no vendor build in
mise's registry, had no `eclipse-temurin:27-jre` image, made `doctor` FAIL by
default on any shell without the mise hook, and silently skipped **11 of 104
integration tests** while the suite still reported green.

**It has been changed.** `src/pom.rs:32` now reads `TARGET_RELEASE = "25"`,
and `tests/golden/scaffold-plain/pom.xml` pins `<maven.compiler.release>25`.
`examples/ACCEPTANCE.md` records the gate passing on "Java 25 LTS
compilation". Nothing further is needed here — but four consequences follow
that the plan should collect rather than rediscover:

1. **The tier-3 skips should be gone.** JDK 26 on a bare PATH accepts
   `--release 25`. Run `JAILS_REQUIRE_TOOLCHAIN=1 cargo test` and confirm the
   11 tests now execute; if any still skip, the gate is measuring something
   else and that is worth knowing.
2. **`doctor`'s daily false FAIL should be gone**, since `java` on PATH is 26
   and the project targets 25. That removes the "health check that cries
   wolf" problem independently of the `JAVA_HOME` fix.
3. **`add docker` no longer needs a `jlink` stage.** `eclipse-temurin:25-jre`
   exists, so the runtime stage is the plain `-jre` base — simpler than the
   design two documents wrote around the EA problem.
4. **The JetBrains Runtime path is reachable for the first time.** JBR's
   ceiling is 25 and the target is now 25, so `-XX:+AllowEnhancedClassRedefinition`
   is a real option for `jails dev` (§6.3) rather than a dead end. It stays
   opt-in and `doctor`-detected — a second JVM is a big ask for a default —
   but it moves from "blocked" to "an experiment worth an afternoon", and it
   is the only thing that would make a record-component edit hot-swappable.

**What still needs recording** is the reason, next to the pin, in CLAUDE.md —
which currently documents the 27 rationale and the mise symlink, and is now
wrong. That is one of the drift items in §4.9, and it matters more than the
others because CLAUDE.md is the first thing every agent on this repo reads.

---

## 15. Provenance

What this file is made of, and how much to trust each part.

- **`ideas-opus.md`** — the loop-latency framing, `jails dev`, the sub-second
  test list, `g sse`/`g page`/`add auth`, `new --template`, `jails scratch`,
  `jails docs`, `jails upgrade`. Its two headline mechanisms (the AOT cache in
  the dev loop, enhanced class redefinition) are dead; see §3. Everything else
  survives.
- **`ideas-grok.md`** — vim-rails projections, `jails src`, `g field` + alter
  migrations, `add html` + `g spider`, `g webhook`/`g auth`/`add sse`/`g
  mailer`, the Lua-list pinning test, and the sharpest anti-goal table. Its
  webhook algorithm and its "console with beans is a trap" call are corrected
  and confirmed respectively.
- **`ideas-kimi.md`** — the synthesis discipline and K1–K21: `recap`, `.env`,
  `inspect db`, `g factory`, `--timestamps`, `add queue`, `g search`, `add
  flags`, `add shedlock`, `g extractor`, `g load`, `add ci`, `add docker`,
  `requests/*.http`, `new --offline`, `migrate --status`, `db --seed`, CRaC,
  `AGENTS.md`, `why` on every failure, `--module`. `add queue` is the single
  most valuable thing it added.
- **`ideas-sol.md`** — the `ChangeSet`/provenance/journal design, the CLI
  schema and event protocol, the production contract, package-by-feature, the
  genericity release gate (six questions every core change must pass), the
  toolchain resolver, and the crawler safety corpus. The largest and the most
  ambitious; its sequencing (provenance before evolution) is the one place
  this plan overrules it, in §7.
- **`ideas-fable.md`** — twelve research passes with `file:line` citations. The
  correction table in §3 is largely its work, as is the jdt.ls/HCR path, the
  `--affected` constant-pool index, `spring-devtools.properties`, the Rails
  migration grammar, the queue reaper, the JWT `exp` finding, the SSE
  `ReentrantLock` finding, `crawlercommons`, projectionist, and `run --tc`.
- **`ideas-opus2.md`** — the only document that ran the tool. Every number in
  §1 and every defect in §4 traces to it. It is also where the authorship
  budget (§1.2–§1.3) comes from, which is the axis this plan is organised
  around.

- **`examples/`** — `ACCEPTANCE.md` (the done/not-done boundary, §2.3–§2.4),
  `DOGFOOD.md` (the twenty-one-defect ledger and the friction ledger), and the
  two manifests. This is not a source document, it is the *harness*, and §2
  makes it the plan's driver rather than an appendix.

**Re-verified against the working tree while writing this file** (still live
unless noted): §4.1 (`generate.rs:1029,1059`, `spring.rs:29` — versionless,
no flavor check), §4.2 (`tests/golden/scaffold-plain/pom.xml:16` — still
versionless, though the pom now pins release **25**), §4.4
(`run.rs`, `pub fn test` — the mangle-then-route bug reads exactly as
described), §4.5 (`run.rs:83` `run_watched`, sole caller `:372`; `watch()` at
`:264`), §4.6 (`run.rs:325`), §4.9 (`grep -c cases README.md` → 1;
`toxiproxy`/`app` absent from `jails.nvim/lua/jails/init.lua`), §5.1 (no
`ArtifactKind::Field`), §5.3 (`sql.rs:331-338`, naive `+ "s"`, and its unit
test still asserts only `rewards`/`work_items`/`news`), §5.4
(`generate.rs:710,789` — `repo` and `dto` read the record, `scaffold` does
not), §5.5 (no `--timestamps` anywhere in `src/`), §8 (`generate.rs:86-96`,
`pom.xml` only), §9.1 (**zero** `cors` matches in `src/`, `templates/`,
`README.md`), §6.1 (**zero** `withReuse` matches).

**Corrected while writing this file** — the earlier draft of this plan got
these wrong by truncating an enum extraction, and they matter because four of
the six source documents propose as future work things that already exist:
`ArtifactKind` has **26** variants, not 22 — including `Fetcher`,
`DurableJob`, `Usecase` and `Query`. `Capability` has **18**, not 16 —
including `Ci` and `Docker`. `@scope` is a shipped field constraint
(`generate/field.rs:349,360`). And `pom::TARGET_RELEASE` is **`"25"`**, not
27, which resolves a decision three source documents argued about and
unblocks four things — see §14.

**One defect the source documents report is already fixed**: `doctor` now
resolves `JAVA_HOME` before PATH (`doctor.rs:871-875`).

**Not verified here, and therefore not load-bearing**: every latency figure
attributed to `ideas-opus2.md` §1 and `ideas-fable.md` §2 (they were measured
in those sessions, not this one); the 196 s gate duration and the "both
manifests pass `mvn verify`" claims, which come from `DOGFOOD.md` rather than
from a run in this session; the upstream `deps/` line numbers, which drift —
prefer the stable anchors (function names, enum variants, the grep itself);
and everything in §14.

This file will drift the same way `validation/README.md` did. When it
contradicts the working tree, the working tree wins.
