# 1000x DX for `jails`: a cross-ecosystem research report

Written 2026-08-25 against `main` (`9e5f7e7`). Every number in it was measured
on this machine on that date, and every claim about jails' current behaviour
cites a file and line. Where a claim is an opinion it is labelled one — the
same rule `pending.md` holds itself to.

**Read `pending.md` first.** This report deliberately does not repeat anything
already in it. §2's proof-application portfolio, §9's test-suite tail and §11's
anti-goals are inputs here, not proposals.

---

## Section 0: What the research actually found (read this before Section 1)

Three findings reframe the brief, and the third is the one that matters.

### 0.1 jails is not slow. Measured.

The prompt's Pillar 1 asks how to "eliminate every delay". Measured on this
machine, in `/tmp/.../scratchpad/lat`:

| command | wall |
|---|---|
| `jails commands --json` | **41 ms** |
| `jails new-cli demo --no-git` | **54 ms** |
| `jails g scaffold Order total:long status:String --pretend` (17 files planned) | **58 ms** |
| `jails g scaffold ...` applied (17 files, ledger written, pom spliced) | **130 ms** |
| `mvn -q test` on that project, failing at compile | **2.55 s** |

A whole transactional 17-file plan — capture, desire, reconcile, project, diff,
render — costs 58 ms. **jails' own latency is already two orders of magnitude
below the cheapest thing it hands off to.** There is no 1000x left inside the
Rust. Optimising `jails-prepare` further would be optimising 4% of the smallest
number in the table.

The 1000x is in three other places:

1. **The Java-side loop jails opens and does not close.** jails writes SQL it
   never executes, tests it never runs, and (see 0.3) a project it never
   confirms compiles. Every one of those is a round trip the developer pays for
   later, at `mvn` speed or at `flyway migrate` speed or at production speed.
2. **The substrate Java is slow on.** `testd` already proves the shape: 464 ms
   for the first JUnit session in a JVM against 20 ms warm, so a resident JVM
   turns 0.62 s into 0.06–0.10 s (`CLAUDE.md`, `testd.rs`). That argument
   generalises to every other JVM question jails answers by forking.
3. **The direction jails cannot go.** jails is code-first only. There is no
   path from an existing database, an existing `.sql` file, or an existing
   class file back into the ledger. That closes the door on the majority of
   real Java projects, which have a schema before they have jails.

### 0.2 The ledger is the asset, and it is one variant short

`ResourceKey` (`crates/jails-protocol/src/vocabulary/resource.rs:154`) is a
closed vocabulary of exactly ten claim kinds:

```
WholeFile · MavenDependency · BuildFeature · ComposeService · Property
MarkedBlock · CommandRegistration · HumanConfigCapability
SpringTestImport · MavenMainClass
```

This is a genuinely unusual thing for a scaffolder to have. Rails, Phoenix,
Laravel and Loco all generate and forget; jails records *what it claimed and
why*, so `destroy` is exact, `sync` is a re-plan, and `doctor`'s
`capability_drift_checks` derives its checks from `add::plan_for` rather than
hand-writing them.

**What is missing from that list is the database schema.** A migration is a
`WholeFile`; the table it creates, the columns in it and the queries against it
are owned by nobody. That single absence is what blocks:

- schema diffing (there is nothing to diff against),
- `jails pull` (nothing to write the pulled catalog into),
- migration linting that knows what the column *was*,
- `destroy` proposing a down-migration,
- and the sqlc-style `.sql`-as-input flow entirely.

Almost everything in Sections 2–4 of this report reduces to one enabling
change: **`ResourceKey::SchemaObject` and `ResourceKey::Query`**. Ranked by
leverage per line of Rust, nothing else in the repository comes close.

### 0.3 Two live defects found while measuring

Neither is in `pending.md`. Both reproduce at `HEAD` in three commands.

**(a) A debug `eprintln!` ships in `jails g scaffold`.**
`crates/jails-generate/src/generate/scaffold.rs:60` reads

```rust
eprintln!("PROBE field={field:?} column={column:?}");
```

introduced in `efa3954`. Every scaffold prints one line of `Debug`-formatted
`Field`/`Column` per component to stderr. This is exactly the discipline
`process.rs` centralises ("debug prints and then runs — that property lives in
the executor now rather than at each site, which is where it was violated").

**(b) `jails g scaffold` in a `new-cli` project produces a project that does not
compile, silently.**

```sh
jails new-cli demo --no-git && cd demo
jails g scaffold Order total:long status:String
mvn -q test
# [ERROR] OrderController.java:[62,16] cannot find symbol: variable HttpStatus
# [ERROR] OrderController.java:[62,38] cannot find symbol: variable ResponseEntity
```

`scaffold_artifacts_from_fields` calls `spring::resource_controller_java`
unconditionally (`generate/scaffold.rs:287–289` at `HEAD`). Seventeen kinds
guard themselves with `require_spring_project` in
`generate/recipes.rs:145–356`; `scaffold`, `controller`, `service` and `dto` do
not. And `report_degraded_shape` (`generate.rs:61`) returns early unless
`project.build()` is `Build::Foreign`, so a Maven-without-Spring project gets
no warning either — it is neither refused nor degraded nor reported.

This is the exact failure Pillar 2 exists to prevent, in the tool's flagship
command, on the projects `CLAUDE.md` says `add` is "most useful for". It is
worth fixing before any idea in this report.

---

## Section 1: Executive DX vision and the top ten

### The vision, in one sentence

> **jails should be the only thing a Java developer has to be fast, because it
> is the only thing that holds the whole truth: the schema, the queries, the
> wiring, the containers and the tests are one reconciled state, and every
> command is a transaction over it.**

The competitive insight from the survey is that the ecosystems with the best DX
are the ones where *one artefact is the source of truth and everything else is
derived*: Prisma's schema, sqlc's SQL, Django's models, Ecto's changesets,
Encore's code-as-infrastructure. The ones with the worst DX are those where two
artefacts must be kept in step by a human — which is precisely the failure jails
already refuses in the small (one column list feeds the DDL, the select, the
insert, the bind and the row mapper) and has not yet refused in the large.

jails' version of "one source of truth" cannot be a schema DSL, because
`pending.md` §11 forbids the machinery that would need. It is **the ledger**.
The ledger is already a reconciled projection of a project. Extending it to
cover the schema and the queries turns it from a record of what jails wrote
into a model of what the project *is* — at which point schema-first,
code-first, verification, linting and reverse-engineering are all the same
operation viewed from different sides.

### The four laws this report holds itself to

Every proposal below was filtered through `pending.md` §11's anti-goals and
`CLAUDE.md`'s scope bar. Nothing here proposes:

- a runtime jar, an ORM, reflection or bytecode weaving,
- a domain-specific generator (a crawler, an inbox and a gateway stay three
  lists of the same generic intents),
- a conditional template language or executable plugin hooks — **data is
  extensible, logic is not**,
- silent Gradle support, or a check that half-understands a build file,
- an incremental `check`, or treating a skipped test as coverage.

Where a proposal comes close to one of those lines, it says so.

### The top ten

Ranked by (DX impact × confidence) ÷ implementation cost. The crate column is
where the work lands; the pillar column is which of the three it serves.

| # | Concept | Adapted from | Crate | Pillar | Cost |
|---|---|---|---|---|---|
| 1 | **`ResourceKey::SchemaObject`** — the schema becomes a claimable resource | Django `state.py`, Alembic comparators | `jails-protocol` | 2, 3 | M |
| 2 | **`jails verify`** — every generated query proved against the schema it will meet | sqlc `vet`/`verify`, SQLx offline | `jails-report` + new `jails-catalog` | 2 | M |
| 3 | **`jails migrate --lint`** — destructive / data-dependent / incompatible change detection | Atlas `sqlcheck` | `jails-report` | 2 | S |
| 4 | **`--diff`** — `--pretend` shows the bytes, not just the verbs | Rails/Loco generator output, `git diff` | `jails-prepare` | 2 | **S** |
| 5 | **`jails pull`** — a domain slice reverse-engineered from a live catalog | PostgREST `SchemaCache`, jOOQ, Ent, Supabase | new `jails-catalog` | 3 | M |
| 6 | **Query files as input** — `db/queries/*.sql` → typed `JdbcClient` methods | **sqlc** | `jails-generate` | 3 | L |
| 7 | **Dev services** — containers appear because config is absent | Quarkus DevServices | `jails-drive` | 1 | M |
| 8 | **The transactional test sandbox** — parallel integration tests, one database, zero truncation | **Ecto SQL Sandbox** | `jails-generate` (`add testkit`) | 1 | M |
| 9 | **`add archunit`** — the fitness gate jails can write and a human cannot | ArchUnit `Architectures` | `jails-generate` | 2 | **S** |
| 10 | **`jailsd`** — `testd` grows from "runs tests" to "answers JVM questions" | Encore daemon, Quarkus dev mode, Gradle daemon | `jails-drive` | 1 | L |

Two runners-up that did not make the ten but appear in the sections below:
**`jails contract`** (OpenAPI breaking-change gate, the sqlc-`verify` idea
applied to HTTP) and **`jails model`** (a TUI over `.jails/app.toml`, which is
already a JDL and does not need to become a language).

---

## Section 2: Pillar 1 — sub-second feedback loops

### 2.1 The honest baseline

Restating 0.1 because every proposal here is measured against it: jails plans a
17-file scaffold in **58 ms** and applies it in **130 ms**. The suite's own
number is **59.60 s** for `cargo test`, of which the CLI binary is 38.54 s warm,
and `pending.md` §9 names the tail exactly: *three concurrent real Failsafe runs
against the shared PostgreSQL and Kafka; once they start they alone determine
the end of the binary.*

So Pillar 1 has one target, not many: **the cost of a real integration test.**
Everything below attacks that, and the biggest single win (2.3) attacks it from
a direction `pending.md` §9 has not considered.

### 2.2 Dev services: the container appears because the config is absent

**The mechanism, from Quarkus.**
`DevServicesDatasourceProcessor.shouldStartBasedOnConfigHandler`
(`deps/quarkus/extensions/datasource/deployment/.../DevServicesDatasourceProcessor.java:361`)
is four lines long and is the whole idea:

```java
if (dataSourceBuildTimeConfig.devservices().enabled().isEmpty()) {
    for (DevServicesDatasourceConfigurationHandlerBuildItem i : configHandlers) {
        if (i.getCheckConfiguredFunction().test(dbName)) {
            // this database has explicit configuration
            // we don't start the devservices
            return false;
```

**Absence of configuration is the trigger.** Not a flag, not a profile — the
fact that nobody said where the database is. And the second half is
`ContainerLocator` (`.../devservices/common/ContainerLocator.java:21`), which
finds an already-running container *by label*, so a container survives across
processes and is reused rather than restarted.

**Why this is worth more to jails than to Quarkus.** Quarkus needs a build-time
extension model to do it. jails needs neither, because jails already:

- starts compose services before `run`/`watch` (`compose.rs`),
- knows which capabilities the project declares (`jails.toml` `capabilities`),
- knows the container engine's real shape on this machine (`doctor` probes with
  bare `docker info` and `docker ps --format '{{.Names}}'` precisely because
  this machine's `docker` is podman's shim), and
- has the `PostCommitEffect::ComposeReconcile` effect already in the protocol.

**The proposal.** `jails run` and `jails test`, when the project declares a
database and `spring.datasource.url` is **not** set (in `application.properties`,
in the environment, or on the command line), start a labelled container and
inject the URL into the child process's environment. **Nothing is written to the
project.** That last clause is what keeps it inside jails' scope bar: this is
not `add db` doing less work, it is `run` supplying what the process needs and
leaving no trace.

```
$ jails run
jails: no spring.datasource.url is set and this project declares `db`.
       starting postgres:16 as jails-devservice-demo-db (label jails.devservice=db)
       SPRING_DATASOURCE_URL=jdbc:postgresql://localhost:32771/demo  (injected, not written)
       reuse: this container is kept between runs; `jails stop --devservices` removes it.
```

Three rules, each load-bearing and each mirroring a rule jails already holds:

1. **Explicit configuration always wins and is never overridden.** Quarkus's
   rule. If the reader set a URL, the dev service does not start, and jails says
   why it did not.
2. **The container is labelled and located, not counted.** `ContainerLocator`'s
   rule. A second `jails run` finds the running one. This is the same discipline
   as `scratch::ScratchDir` — never claim something that already exists — read
   in the other direction.
3. **It says so, every time.** A container that appears silently is magic, and
   `CLAUDE.md`'s whole position on `add db` is that a capability which installs
   code and skips the dependency is worse than one that refuses.

**Where it lands.** `jails-drive` (it starts something, so it cannot be
`jails-report` — that crate sits below `jails-drive` structurally so a reporting
command that started something would not compile). The container decision is a
pure function of the project and belongs in `jails-prepare` as a new
`PostCommitEffect::DevServiceReconcile`, keeping the "decide in prepare, apply
in the executor" split intact.

**Expected impact.** Removes `jails add db && jails start && jails migrate` — 3
commands and one compose file — from the path to a first working query. Does not
touch the test tail; 2.3 does that.

### 2.3 The transactional test sandbox — the biggest available win, and the one nobody has tried

**The mechanism, from Ecto.** `Ecto.Adapters.SQL.Sandbox` gives each test its
own pooled connection, opens a transaction on it, and rolls it back at the end.
Because each concurrent test holds a *different* connection, tests see only
their own uncommitted data and can run in parallel against one database with no
truncation, no fixtures reload and no ordering constraints. Its `shared` mode
(`Sandbox.mode(Repo, {:shared, self()})`) exists for exactly the case where
another process — a server handling an HTTP request — must see the test's
transaction.

**Why Spring cannot do this today, verified in source.** Spring's
`@Transactional` test support rolls back by default, but it binds transaction
state to a `ThreadLocal`:

```java
// deps/spring-framework/spring-tx/.../TransactionSynchronizationManager.java:77
private static final ThreadLocal<Map<Object, Object>> resources =
        new NamedThreadLocal<>("Transactional resources");
```

and Spring's own reference documentation states the consequence:

> Spring's testing support binds transaction state to the current thread (via a
> `java.lang.ThreadLocal` variable) *before* the current test method is invoked.
> If a testing framework invokes the current test method in a new thread […] any
> actions performed within the current test method will *not* be invoked within
> the test-managed transaction.
> — `deps/spring-framework/framework-docs/.../testcontext-framework/tx.adoc:33`

That is why every `@SpringBootTest(webEnvironment = RANDOM_PORT)` integration
test in the Java world falls back to truncating tables, or to a fresh container,
or to `@DirtiesContext`. The request runs on Tomcat's thread; the test's
transaction is on JUnit's.

**The proposal: `add testkit` generates the shared-mode sandbox.** A
`DataSource` decorator that, for the duration of one test, hands *every* thread
the same physical connection — the one the test opened a transaction on — and
suppresses `close()` on it. That is Ecto's shared mode, and it is about 120
lines of ordinary Java with no reflection, no agent and no jar. Blueprint in
Section 7.6.

**What it buys, and where the number comes from.** `pending.md` §9's tail is
"three concurrent real Failsafe runs against the shared PostgreSQL and Kafka".
Today those runs are concurrent *processes* that must not see each other's rows,
which is why they need per-application isolation (§9's Phase 3 work). With a
sandbox, isolation is per-*test* rather than per-application, which means:

- integration tests within one application can run in parallel (`junit.jupiter.
  execution.parallel.enabled`), which today they cannot,
- no truncation between tests, and no `@DirtiesContext` — so the Spring context
  cache actually holds, which `pending.md` §11 notes has never been counted,
- and the per-application database isolation §9 built becomes an optimisation
  rather than a correctness requirement.

**This is a proposal, not a measurement.** I did not build it. But it attacks
the one thing §9 names as the determinant of the suite's end time, from a
direction §9's own candidate list ("another reduction in Maven/JVM startup work,
or a safe long-lived build daemon") does not contain — and it improves *every
generated project's* test suite, not just jails' own, which is the multiplier
that makes it a Pillar-1 item rather than a repo chore.

**The honest risk.** A test that deliberately asserts on committed state — an
outbox worker polling in another thread, a `REQUIRES_NEW` propagation — behaves
differently under a shared connection. The generated `SandboxExtension` must be
opt-in per test class (`@JailsSandbox`), never global, and the generated Javadoc
must name the trap. That is the same shape as `g strategy`'s "an implementation
missing `@Component` is simply not in the list": the failure is silent, so the
generator's job is to say so in the file.

### 2.4 `jailsd`: `testd` grows from "runs tests" to "answers JVM questions"

**The measured premise, from `CLAUDE.md`.** `testd` is 0.06–0.10 s against
`--fast`'s 0.62 s for one test method, and the reason is not the launcher: *the
first JUnit session in a JVM is 464 ms against 20 ms warm, and a cold `java`
pays it every run.*

That argument does not stop at JUnit. Every other JVM-shaped question jails
answers by forking pays a comparable constant:

| question | today | pays |
|---|---|---|
| does this compile? | `mvn -q compile` | JVM + Maven bootstrap + plugin resolution |
| what is the classpath? | `mvn dependency:build-classpath` | same, per call |
| is this SQL valid? | nothing (see Section 3) | — |
| what beans exist at runtime? | nothing; `beans` reads source | — |
| REPL | `jshell` + Maven classpath (`console.rs`) | same |

**The proposal.** Keep `testd`'s design intact — it is a Java program compiled
by `java`'s single-file source launcher, nothing enters the project, and it
deliberately does not compile — and add request types over the existing unix
socket: `classpath`, `verify-sql`, `beans-runtime`. The daemon already holds the
dependency classpath (and must go on holding *only* that: `CLAUDE.md`'s rule
that `target/classes` and `target/test-classes` are handed to JUnit as
`--class-path` so a child loader is built per run is what stops a daemon being
green over code that no longer exists).

**What must not change.** §10.2's finding that the daemon must not compile still
holds — the editor's language server already writes `target/classes` on save,
and compiling only the changed file is unsound. `jailsd` answers questions about
a classpath; it does not become a build tool.

**Cost.** L. This is the largest item in the report and should be sequenced
last, after 2.2 and 2.3 have been measured — because if the sandbox removes the
Failsafe tail, the daemon's remaining value is smaller than it looks today.

### 2.5 The content-addressed analysis cache, taken from sqlc

**The mechanism.** sqlc's `CachedAnalyzer`
(`deps/sqlc/internal/analyzer/analyzer.go:74`) treats analysing a query as an
*action* whose inputs are hashed:

```go
// Analyzing a query is an action whose inputs are the configuration, the
// schema migrations, and the query itself. (The sqlc binary is an
// implicit input of every action.)
action := store.NewAction("QueryAnalysis").AddInput("config", c.configBytes)
for _, m := range schema { action.AddInput("schema", []byte(m)) }
actionDigest := action.AddInput("query", []byte(q)).Digest()
```

and the result goes in a content-addressed store keyed by that digest. A second
run with the same schema and query never touches the database.

**Why this drops straight into jails.** `jails-commit`'s `store.rs` already *is*
a content-addressed object store, and `jails-protocol`'s
`observe/provenance.rs` already stamps "a changed input template or version
gives a different stamp" (`provenance.rs:399`). The digest inputs for a jails
query analysis are the same three: the jails binary's provenance stamp, the
ordered migration files, and the statement. Which means `jails verify`
(Section 3.1) can be **free on the second run**, and free in CI with a warm
cache, without inventing any caching machinery.

**One rule sqlc got right and jails must copy.** sqlc only caches for *managed*
databases (`if !c.db.Managed { return nil, true, nil }`) — because it cannot
prove an unmanaged database has not changed underneath it. jails' equivalent:
cache only when the schema came from the migration files jails owns, never when
it came from a live connection the reader pointed at.

### 2.6 Smaller latency items, with their sources

- **`jails testd --affected` already implements the best idea in this space**
  (reverse dependency index from constant pools). Nothing in the survey beats
  it. Bazel and Gradle both do more work for the same answer.
- **`git diff`-driven scope, not a marker.** `affected.rs`'s existing rule
  ("changed is what git reports rather than a marker jails writes, because a
  marker makes the same command select differently on two consecutive runs with
  no edit between") is a better version of what Air, Vite and `mix test.watch`
  all do. Keep it.
- **Air (`air-verse/air`) contributes nothing jails lacks** — `run --watch` +
  devtools already covers it, and `run_watched` scanning for
  `why::FATAL_MARKERS` is strictly better than Air's "restart and hope",
  because `mvn spring-boot:run` exits 0 over a dead application.

---

## Section 3: Pillar 2 — correctness, trust and zero puzzlement

jails is already the strongest tool in this survey on Pillar 2. `doctor`'s
`capability_drift_checks` re-planning rather than re-deriving, `why`'s rules
mined from real logs, `explain`'s hand-written table held by
`every_kind_has_an_explanation`, `agreement.rs` running every scenario forward
and back — none of Rails, Laravel, Phoenix, Loco or JHipster has an equivalent
of any of them.

So this section is short on general advice and long on the four specific holes
the survey exposed.

### 3.1 `jails verify` — jails generates SQL and never runs it

**The hole.** `sql.rs` derives the DDL, the select, the insert, the bind and the
row mapper from one `Column` list, which is what stops them drifting *from each
other*. Nothing stops them drifting from **the database**. A hand-edited
migration, a column dropped by a DBA, a `V007__.sql` written by a colleague, an
`ALTER` applied out of band — all of these leave a generated adapter that
compiles perfectly and fails at run time with "column does not exist".

jails' current answer is `jails migrate --check`, which applies every migration
to a scratch database and reports the first failure with psql's file and line.
That proves the *schema* is applyable. It proves nothing about the *queries*.

**The mechanism, from sqlc.** `sqlc vet` with the `sqlc/db-prepare` rule sends
every query to the database as a `PREPARE`, so the database itself is the
type-checker. `sqlc verify` goes further and checks *existing* queries against a
*proposed* schema change — the case sqlc needs a cloud service for, because it
has no record of what is deployed.

**jails does not need a cloud service, because git is the record.**

```
jails verify [--against <git-ref>] [--json]
```

1. Reserve a scratch database (`ScratchDir` has the exclusivity discipline
   already; `migrate.rs` has the create/drop-around-the-run pattern).
2. Apply every migration in numeric order (`migrate.rs` already sorts
   numerically, not lexically — `V10` before `V9` is a real trap it closed).
3. For every SQL statement jails generated and recorded, `PREPARE` it. A
   failure names the file, the statement, the entity that owns it, and the
   migration that most likely broke it.
4. With `--against <ref>`: apply only the migrations that exist at `<ref>`,
   then prepare the statements from the *working tree*. That answers "will the
   code I am about to ship survive the schema that is deployed?", which is the
   half sqlc charges for.

**What it needs.** `ResourceKey::Query { entity, name }` — because step 3 needs
a list of statements jails knows it owns, and today there is none. That is idea
#1 from Section 1 again.

**What it costs, after 2.5.** One scratch database on the first run; zero on
every subsequent run with an unchanged (binary, migrations, statement) triple.

### 3.2 `jails migrate --lint` — Atlas's catalogue, which jails has one entry of

**The hole.** `sql.rs:637–652` already refuses one dangerous migration by hand:

```
required unique text field `{}` has no safe automatic backfill.
       fix: add it as nullable first, backfill distinct values, then add
            not-null in a deliberate migration.
```

That is exactly right, and it is one item from a catalogue of about eight. Atlas
(`ariga/atlas`, `sql/sqlcheck/`) has the rest, organised as four analyzers.
Verbatim from its diagnostic strings:

| analyzer | file | what it catches |
|---|---|---|
| `destructive` | `destructive/destructive.go:81,111,123` | `Dropping table %q`; `Dropping non-virtual column %s` |
| `datadepend` | `datadepend/datadepend.go:152,160,200` | `Adding a unique index %q on table %q might fail in case %s duplicate entries`; `Modifying nullable column %q to non-nullable might fail in case it contains NULL values` |
| `condrop` | `condrop/condrop.go:71` | `Dropping foreign-key constraint %q` |
| `incompatible` | `incompatible/incompatible.go:58,69` | `Renaming table %q to %q`; `Renaming column %q to %q` |

**Why the split into four analyzers matters and is not cosmetic.** They are four
*different kinds of wrong*: destructive loses data unconditionally; data-depend
might fail depending on rows that exist; condrop loses a guarantee silently;
incompatible breaks deployed readers while the migration itself succeeds. A
single "dangerous migration" warning collapses four different fixes into one
shrug. jails' `why.rs` already holds the right shape for this — a table of
(signature, explanation, fix) with rules sharing a `group` so only the most
specific is reported.

**The proposal.** `jails migrate --lint` (or fold it into `doctor` — see the
caveat) runs the four analyzers over pending migrations. Each finding carries a
`fix:` line, which an integration test already asserts for every `doctor` FAIL.

**The caveat, and it decides the command name.** `doctor` is read-only *by
contract* and structurally — `jails-report` sits below `jails-drive` so a
reporting command that started something would not compile. A *static* lint over
migration text is read-only and could live in `doctor`. A lint that needs to
know whether duplicates actually exist needs a database and cannot. So: **the
static half goes in `doctor`, the data-dependent half goes in `migrate --lint`
beside `--check`, which already writes.** That split is not a compromise; it is
the crate boundary doing its job.

### 3.3 `--pretend` shows verbs, not bytes — the cheapest large win in the repo

**The hole, measured.** `crates/jails-prepare/src/report.rs:262`:

```rust
pub(crate) fn render(report: &Report) -> String {
    let mut out = format!("plan {} {}\n", report.transaction, kind_label(&report.kind));
    for operation in &report.operations {
        out.push_str(&format!("  {:<7} {}\n", operation.kind.verb(), operation.path));
    }
```

That is the whole of what `--pretend` shows. The output in Section 0.1 —
seventeen `create <path>` lines — tells a reader which files will appear and
nothing whatever about what will be in them. The prompt asks for "colorized
unified diffs and AST merge previews"; jails has neither.

**Why this is nearly free.** Every ingredient already exists:

- `pipeline/diff.rs:50` computes the operations from `base` (a
  `ProjectSnapshot`), `projection`, and `rendered` — **both sides of every diff
  are already in memory** when the report is built.
- The three-way case is already distinguished (`diff.rs:22`'s doc comment: *"a
  path this change owns goes through §R5.3's three-way rule instead, because
  'the generator changed it' and 'the reader changed it' are different facts"*).
- `Conflict { path, hunks }` (`diff.rs:43`) already counts hunks, so the merge
  preview's data is computed and then discarded.

**The proposal.**

```
jails g scaffold Order total:long --pretend --diff [--color=auto|always|never]
```

Three renderings, one per case, because the three cases are genuinely different
and collapsing them is what makes tools untrustworthy:

1. **create** — the new file, with a `+` gutter, elided to the first N lines
   with `… 42 more lines` unless `--diff=full`.
2. **replace of a file jails owns and the reader has not touched** — a unified
   diff, old against new.
3. **three-way** — the interesting one. Show *both* deltas: what the generator
   changed since the recorded base, and what the reader changed since the
   recorded base, side by side, then the merged result. That is the single piece
   of information a reader cannot reconstruct any other way, and jails is the
   only tool in this survey that has the recorded base needed to produce it.

**Sequencing note.** This is also the honest prerequisite for `pending.md` §11's
"conflicted merges cannot be resumed". A reader cannot be asked to resolve a
conflict they were never shown.

**Cost: S.** No new protocol, no new resource key, no new crate. It is a
rendering of values that are already computed.

### 3.4 `add archunit` — a fitness gate jails can write and a human cannot

**The mechanism.** ArchUnit's `Architectures.onionArchitecture()`
(`deps/archunit/archunit/src/main/java/com/tngtech/archunit/library/Architectures.java:764`),
used as in its own example
(`archunit-example/example-plain/.../OnionArchitectureTest.java:33`):

```java
onionArchitecture()
        .domainModels("..domain.model..")
        .domainServices("..domain.service..")
        .applicationServices("..application..")
        .adapter("cli", "..adapter.cli..")
        .adapter("persistence", "..adapter.persistence..")
        .adapter("rest", "..adapter.rest..")
        .check(classes);
```

**Why jails specifically should generate this.** Those package strings are the
problem with ArchUnit in practice: they are a second copy of the project's
layout, hand-maintained, and they go stale the moment someone renames a package
— at which point the rule matches nothing and passes, which is the worst
possible failure mode for a gate. This repository has a name for that exact
disease: *"a check that silently stops applying after a dependency bump is worse
than no check"* (`CLAUDE.md`, on Testcontainers 2.0 module renames).

jails is the only tool that can write this file correctly, because
`Config::layers()` applies the project's `jails.toml` renames and
`config::LAYERS_IN_ORDER` is the single owner of the eleven-layer list. An
`add archunit` capability renders the rule from `Config::layers()`, so a reader
who sets `adapters = "persistence"` gets a rule that still matches — and
`jails sync` re-plans it when they change it again.

**And it closes a real hole in what jails already promises.** `CLAUDE.md` claims
"Generated architectures follow Hexagonal / Explicit Ports & Adapters". Nothing
in a generated project enforces that. A reader who imports `JdbcOrderRepository`
from `OrderController` gets a green build today. Blueprint in Section 7.7.

**One rule, from jails' own precedent.** The generated rule must be
`@ArchTest`-annotated and run in the normal Surefire pass, not behind a profile.
`CLAUDE.md`'s Failsafe finding — *"jails generated integration tests for months
that never ran once […] which is worse than having no test because the green
build claims it passed"* — is the same trap wearing a different hat.

### 3.5 `jails contract` — the sqlc-`verify` idea applied to HTTP

**Observed:** `springdoc-openapi` and `swagger-core` are both in `deps/`, and
`grep -rl "springdoc\|openapi" crates/ templates/` returns exactly one file
(`crates/jails-java/src/template.rs`, and that is an unrelated word match).
Nothing generates against either.

**The mechanism, from utoipa / Huma / Fuego / Goa.** The unifying pattern across
all four is *the type signature is the contract*, and the OpenAPI document is
derived rather than written. Java cannot do the derivation at compile time
without an annotation processor (which is Micronaut's whole thesis), and jails
will not ship one.

**But jails does not need to derive it — jails knows it.** jails wrote the
controller, the request DTO, the response DTO, the validation annotations and
the error arms. It knows the resource path (`web::resource_path` delegating to
`sql::table_name`, so `/categories` not `/categorys`), it knows which methods a
scoped scaffold's controller answers (`scaffold.rs:401`: *"A scoped scaffold's
controller is create-only"*), and it knows the 409 arm exists only when the JDBC
starter is present.

So:

```
jails contract [--emit openapi.yaml] [--against <git-ref>] [--json]
```

`--emit` writes the document from the ledger — no runtime, no annotations, no
springdoc dependency. `--against` diffs it against the document at a git ref and
**fails on a breaking change**: a removed path, a removed response code, a
required request field that was not required before, a narrowed type. That is
`sqlc verify` for HTTP, and it is the piece that turns "jails generated a REST
API" into "jails will tell you before you break its clients".

**Honest limitation.** This describes what jails generated, not what the
application serves. A hand-written `@GetMapping` is invisible to it. The output
must say so — the same way `routes`/`beans` say they read source and therefore
miss anything decided at runtime. A contract document that silently omits half
the API is worse than none.

### 3.6 What the survey says jails should *not* do

Three ideas that look attractive and are wrong here, recorded so they are not
re-derived:

- **A runtime bean/route view.** Already an explicit "Not yet" in `README.md`,
  and the survey confirms the call: Spring's own Actuator does it, needs the app
  to start, and is useless in the case that matters (a context that fails to
  start). `beans` reading source is the better trade.
- **A CEL-style expression language for custom rules**, as in `sqlc vet`. It is
  elegant in sqlc and it is `pending.md` §11's "a conditional template language"
  and "executable plugin hooks" in one. jails' equivalent of extensibility is
  the closed vocabulary plus template overriding: data extensible, logic not.
- **`makemigrations`-style interactive questioning** (Django's
  `questioner.py`, 13 KB of "did you rename this column?" prompts). jails does
  not have to guess: the ledger holds the previous field list, so a rename is a
  *derivation*, not an interrogation. Loco's `guess_migration_type`
  (`deps/loco/loco-gen/src/infer.rs:72`) parses `add_note_to_users` into an
  operation and falls back to `MigrationType::Empty` when it cannot — a
  reasonable design for a tool with no ledger, and a straight violation of
  jails' "an unknown marker is an error, not a no-op" rule for a tool with one.

---

## Section 4: Pillar 3 — ultra-high-velocity authoring

jails already has 36 generator kinds, 25 capabilities and 41 subcommands, and a
declarative manifest that is a working JDL. Model factories exist (`g factory`,
built on the same `sample_value` the tests use) and two-row JSON fixtures ship
with every scaffold. So the honest gaps here are not "more generators" — they
are **the two directions jails cannot go**.

### 4.1 The missing direction, stated precisely

Every flow jails supports runs left to right:

```
CLI field spec  →  FieldSpec  →  Column  →  DDL + JDBC + Java + tests
```

`fields_from_record` reads Java back into `Field`, so there is a partial return
path from *code*. There is **no** return path from SQL: a repository-wide grep
for `information_schema`, `pg_catalog` and `DatabaseMetaData` returns exactly
one hit, a comment in `spring/h2.rs:159` about a URL-parsing quirk.

Two consequences, and the second is strategic:

1. A project that already has a database cannot adopt jails for its domain. It
   can adopt jails for capabilities (`jails adopt` handles the layout), but
   every entity has to be re-typed by hand into a field spec that must agree
   with a schema jails cannot read.
2. **That is most Java projects.** The population `CLAUDE.md` says `add` is
   most useful for — "a codebase jails did not create" — is exactly the
   population `generate` serves worst.

### 4.2 `jails pull` — a domain slice from a live catalog

**The mechanism, from PostgREST.** `SchemaCache.hs`
(`deps/postgrest/src/library/PostgREST/SchemaCache.hs`, 1,191 lines) issues a
handful of large catalog queries against `pg_class`, `pg_attribute`,
`pg_constraint`, `pg_depend` and the `information_schema._pg_*` helper
functions, and materialises one in-memory model of the schema from which the
entire REST surface is derived. jOOQ, Ent, SeaORM, Prisma's introspection engine
and Supabase all do the same thing with different output.

**The jails translation is unusually short, because the target model exists.**
jails does not need a schema model — it needs `Vec<FieldSpec>` and a table name,
because everything downstream (`sql::Column`, the record, the adapter, the DTOs,
the tests, the migration) is already a pure function of those.

```
jails pull [<table>...] [--url <jdbc-url>] [--as <Name>] [--pretend] [--all]
```

renders each table as *the field-spec tokens a human would have typed*, and then
runs the ordinary `g scaffold` path with them. The output of `--pretend` is
therefore the command you could have typed:

```
$ jails pull orders --pretend
jails: read `orders` from jdbc:postgresql://localhost:5432/shop

  jails g scaffold Order \
      id:uuid@pk \
      customerId:uuid \
      totalMinor:long@nonnegative \
      status:OrderStatus \
      placedAt:instant@index \
      cancelledAt:instant? \
      --index 'customer_id, placed_at desc'

  note: `status` is a Postgres enum with 3 labels. jails will also generate
        `jails g enum OrderStatus PENDING PAID CANCELLED`.
  note: `orders.notes` (type `jsonb`) has no jails field type and was skipped.
        fix: model it as an owned type and relate it with `jails g association`.
```

**Four rules, each derived from a mistake another tool in this survey makes.**

1. **Emit the command, not just the code.** Every other introspection tool
   (jOOQ, Prisma, SeaORM) emits generated code and leaves the reader unable to
   re-derive it. Emitting the *field spec* means the pull is reproducible, the
   ledger row is an ordinary entity, and `destroy` works — because the entity
   was created through the same path as a hand-typed one.
2. **An unmappable column is reported, never guessed.** `jsonb`, `tsvector`,
   arrays, domains, `hstore`. This is `@primary`-is-an-error applied to types.
   A silently dropped column produces a record that is missing a field nobody
   noticed, which is exactly the class of bug this repository refuses.
3. **The driver decides the dialect, not `jails.toml`.** `Project::sql_dialect`
   already holds this rule for writing; reading is the same rule.
4. **It writes a migration marked as already applied, or none at all.** Pulling
   an existing table must not generate `V001__create_orders.sql` that Flyway
   will then try to run. Either emit a Flyway baseline, or emit nothing and say
   so. Getting this wrong drops a production table on the next `migrate`.

**Where it lands.** A new crate, `jails-catalog`, between `jails-state` and
`jails-project` in the layer table: it reads a live database and classifies what
it finds, which is exactly `jails-state`'s job description applied to a
different substrate. It must not depend on `jails-generate`; the direction is
catalog → `FieldSpec` → the existing planner.

**The dependency question, and it is real.** jails' only third-party crates are
`clap` and `tempfile`. Reading Postgres means either a Rust driver (`tokio-
postgres` + `rustls`, a large tree) or shelling out to `psql` with a
`--csv`/`--tuples-only` query. **Shell out.** `console.rs` already runs `psql`,
`migrate.rs` already depends on it, `process.rs` already resolves tools on PATH
and never renders secrets (`ALWAYS_SECRET` exists precisely because
`console.rs` sets `PGPASSWORD`), and a `psql` that is absent is a refusal jails
can explain rather than a build-time cost every user pays. This also keeps
`jails pull` honest about `sqlite3`, where the query is `pragma table_info`.

### 4.3 Query files as input — the sqlc pattern, which fits jails better than it fits sqlc

**The mechanism.** sqlc reads `.sql` files whose statements carry a one-line
annotation:

```sql
-- name: FindOverdueInvoices :many
SELECT * FROM invoices WHERE due_at < $1 AND paid_at IS NULL ORDER BY due_at;
```

`ParseQueryNameAndType` (`deps/sqlc/internal/metadata/meta.go:60`) reads the
name and the command; the command is one of a closed set
(`meta.go:29–39`): `:one`, `:many`, `:exec`, `:execresult`, `:execrows`,
`:execlastid`, `:copyfrom`, `:batchexec`, `:batchmany`, `:batchone`. The query
is then analysed — either by sqlc's own Postgres parser or by preparing it
against a real database — and a typed struct plus a typed method fall out.

**Why this fits jails better than it fits sqlc.** sqlc has to *invent* the
philosophy that raw SQL is the source of truth. jails already holds it, as a
scope bar: no ORM, raw SQL, `JdbcClient`, in-memory fakes. The one thing jails
lacks is a way for the reader to write a query jails did not generate and still
get the type-safe half. Today that query is hand-written into the adapter, at
which point it is outside the ledger, invisible to `destroy`, and unverifiable.

**The proposal.**

```
src/main/resources/db/queries/invoice.sql     ← the reader writes this
```

```sql
-- name: findOverdue :many
select id, customer_id, total_minor, due_at
  from invoices
 where due_at < :cutoff and paid_at is null
 order by due_at;
```

```
jails g query-file src/main/resources/db/queries/invoice.sql
```

generates, into the adapters layer, one method per named query on a
`InvoiceQueries` port plus a `JdbcInvoiceQueries` adapter using `JdbcClient`
named parameters, plus a row record per distinct shape, plus an `*IT` that runs
each query against Testcontainers. Blueprint in Section 7.3.

**Three deliberate divergences from sqlc.**

1. **Named parameters, not `$1`.** `JdbcClient` supports `:name` natively and
   Java has no positional-parameter ergonomics worth defending. This also makes
   the generated method signature derivable without a parser: the parameter
   names are in the SQL.
2. **jails does not write a SQL parser.** sqlc has a full Postgres AST
   (`internal/engine/postgresql`), which is a multi-year project and is
   `pending.md` §11's territory. jails gets the column types by **preparing the
   statement against the scratch database it already builds in `migrate.rs`**
   and reading the result-set metadata — which is Section 3.1's machinery,
   reused. No parser, no dependency, and the types come from the database rather
   than from an approximation of it.
3. **`:one`/`:many`/`:exec` only.** sqlc's ten commands include batch and
   copy-from variants that need driver-specific support. Three is the closed set
   jails can honour; a fourth annotation is an error, not a no-op.

**What it needs.** `ResourceKey::Query { file, name }` — so a query removed from
the `.sql` file retires its generated method, which is `destroy`'s existing
semantics applied to a new resource. This is the second half of Section 0.2's
enabling change.

**Cost: L.** This is the largest authoring item and the highest ceiling. It is
also the one that makes jails useful to a team that has already written its SQL.

### 4.4 Schema-first ↔ code-first, reconciled — `jails schema diff`

With `ResourceKey::SchemaObject` in place, the two directions become one
operation. **Alembic organises this exactly right** and jails should copy the
organisation, not the code: `deps/alembic/alembic/autogenerate/compare/` is one
comparator module per concern —

```
tables.py · constraints.py · check_constraints.py · types.py
server_defaults.py · comments.py · schema.py
```

— each registered with `@comparators.dispatch_for(...)`, each emitting an `Op`,
and a separate `render.py` with `@renderers.dispatch_for(ops.CreateTableOp)`
turning `Op`s into migration source. The split between *deciding* and
*rendering* is the same split jails already enforces between `jails-prepare` and
the executor, and the same split `BuildFeature` enforces between "what the build
must do" and "the Maven block that does it".

jails' version, with an exhaustiveness guarantee Alembic cannot have:

```rust
enum SchemaOp {
    CreateTable { .. }, DropTable { .. },
    AddColumn { .. }, DropColumn { .. }, AlterColumnType { .. },
    SetNotNull { .. }, DropNotNull { .. },
    AddIndex { .. }, DropIndex { .. },
    AddConstraint { .. }, DropConstraint { .. },
}
```

matched exhaustively by the Postgres renderer *and* the SQLite renderer, so
**adding an operation is a compile error until both dialects render it** —
which is precisely the discipline `gradle.rs`'s four exhaustive `BuildFeature`
matches already establish ("adding a feature is a compile error until the Gradle
side exists").

Then:

```
jails schema diff                 # ledger vs. the migrations on disk
jails schema diff --live          # ledger vs. a real database
jails schema diff --emit          # write the migration that closes the gap
```

**And `g field` stops being a special case.** Today `route/field.rs:30` is a
one-shot whose migration is append-only, and removing or changing a component is
explicitly refused (`field.rs:38`: *"Removing or changing a component is a data
migration, and jails does not write one it cannot check against the rows that
are there"*). That refusal is correct today and stops being necessary once the
schema is a claimed resource and `--lint` (Section 3.2) can classify the change:
a `DropColumn` becomes a *destructive* diagnostic with a `fix:` line, not a
refusal — and a nullable-widening becomes an ordinary safe migration.

### 4.5 Seeds — the one Rails/Laravel/Phoenix idea jails genuinely lacks

`g factory` covers test-data builders; `fixtures/<table>.json` covers the
two-row test fixture. What none of them covers is **the developer's local
database after `jails start`** — Rails' `db/seeds.rb`, Laravel's seeders,
Phoenix's `priv/repo/seeds.exs`. Today a reader who wants twenty realistic rows
to click through writes them by hand.

```
jails g seed <Entity> [--rows 50] [--seed 42]
```

writes `src/main/java/.../testkit/<Entity>Seed.java` — a plain class with a
`main` that inserts N rows through the *existing repository port*, values from
the existing `sample_value` machinery, varied by a **seeded** PRNG so two runs
produce the same data. `jails seed [--reset]` runs it.

Two notes. It goes through the repository port rather than raw SQL so it cannot
drift from the schema. And the seeded PRNG is not decoration: `pending.md` §2.1
names *"seedable randomness and configurable stock, so a 429, a rotten delivery
and part 2's both-suppliers-empty case are forced on demand rather than waited
for"* as the missing half of `add testkit` — the same generator serves both.

### 4.6 A denser field DSL? No — and here is the measurement

The prompt proposes syntax like `status:enum{PENDING,PAID,CANCELLED}`. It is
tempting and it should be refused, for a reason this repository already
discovered the hard way.

`pending.md` §6.3 records that **two parsers of the field syntax was the
repository's most reliable drift generator** and cost two live divergences
before they were merged. Inline enum syntax adds a nested grammar — braces,
commas, and eventually `ref{Order.id}` and `check{> 0}` — to a parser whose
single-copy discipline was won at real cost. It also collides with a rule
already in place: `@check(...)` is refused because it would be "a string jails
passes through and cannot validate".

The existing two-command form is one keystroke longer and strictly better:

```
jails g enum OrderStatus PENDING PAID CANCELLED
jails g scaffold Order status:OrderStatus
```

because `OrderStatus` is then a real type the project owns, `is_enum` can read
it off disk to produce a sample, and `destroy` can find it.

**What *is* worth adding is the opposite of syntax: a TUI over the manifest.**
`.jails/app.toml` is already a JDL with a closed `[[generate]]` schema. JHipster
proves the multi-entity modelling value; JHipster's JDL also proves the cost of
inventing a language. `jails model` — a terminal editor over the manifest with
completion driven by `jails commands --json` (which the Neovim plugin already
consumes, and which cannot drift from the binary because it is walked out of the
same `clap::Command`) — gets the value without the grammar. It is a view over an
existing file format, which is the only kind of TUI that does not become a
second source of truth.

---

## Section 5: Cross-ecosystem pattern translation matrix

Every row was checked against a checkout in `deps/`. "Have it" means jails
already implements the pattern and the row is recorded so it is not re-proposed.

### 5.1 Patterns to adopt

| Source | Core DX innovation | How jails adapts it | Crate | Impact |
|---|---|---|---|---|
| **sqlc** (`internal/metadata/meta.go:60`) | `-- name: X :many` turns a `.sql` file into typed API | `g query-file`: `:one`/`:many`/`:exec`, `:name` params, types read by preparing against the scratch DB — no SQL parser | `jails-generate` + `jails-catalog` | Authoring ★★★ |
| **sqlc** (`internal/analyzer/analyzer.go:74`) | Analysis is a cached action keyed on (config, schema, query) | `jails verify` caches per (provenance stamp, migrations, statement) in `jails-commit`'s existing CAS | `jails-commit` | Latency ★★ |
| **sqlc** (`docs/howto/verify.md`) | Old queries checked against *new* schema | `jails verify --against <git-ref>`; git is the deployed record, so no cloud service | `jails-report` | Correctness ★★★ |
| **Atlas** (`sql/sqlcheck/{destructive,datadepend,condrop,incompatible}`) | Migration linting split by *kind of wrong* | `doctor` (static half) + `migrate --lint` (data-dependent half); jails has 1 of ~8 rules today at `sql.rs:637` | `jails-report`, `jails-drive` | Correctness ★★★ |
| **Quarkus** (`DevServicesDatasourceProcessor.java:361`) | *Absence* of config starts a container; labels make it reusable | `jails run`/`test` inject a URL for a declared-but-unconfigured service; nothing written to the project | `jails-drive` + new `PostCommitEffect` | Latency ★★★ |
| **Ecto** (`Ecto.Adapters.SQL.Sandbox`, shared mode) | Per-test transaction rolled back; shared connection for other threads | `add testkit` generates `SandboxDataSource` + `@JailsSandbox`; defeats Spring's `ThreadLocal` binding (`TransactionSynchronizationManager.java:77`) | `jails-generate` | Latency ★★★ |
| **PostgREST** (`SchemaCache.hs`) | The catalog *is* the API definition | `jails pull` renders catalog → field-spec tokens → the ordinary `g scaffold` path | new `jails-catalog` | Authoring ★★★ |
| **Alembic** (`autogenerate/compare/*.py`) | One comparator per concern; decide and render are separate | `SchemaOp` enum, exhaustively matched per dialect — adding an op is a compile error until both render it | `jails-protocol` | Correctness ★★★ |
| **ArchUnit** (`Architectures.java:764`) | Architecture as an ordinary unit test | `add archunit` renders the rule from `Config::layers()`, so a renamed layer cannot silently disable it | `jails-generate` | Correctness ★★ |
| **Django** (`migrations/state.py`) | A model *state* separate from both code and DB | `ResourceKey::SchemaObject` — jails' ledger becomes that state | `jails-protocol` | Correctness ★★★ |
| **Rails / Phoenix / Laravel** (`db/seeds`) | Realistic local data in one command | `g seed` + `jails seed`, through the repository port, seeded PRNG | `jails-generate` | Authoring ★★ |
| **utoipa / Huma / Goa** | The signature is the contract; docs are derived | `jails contract --emit` from the ledger; `--against <ref>` fails on breaking changes | `jails-report` | Correctness ★★ |
| **Encore** | Static declarations define infrastructure | The ledger already holds the declarations; `jails start` reconciles from it rather than from `compose.yaml` alone | `jails-drive` | Latency ★★ |
| **JHipster** (JDL) | One file models many entities and relations | `.jails/app.toml` already is one; add `jails model` (TUI), **not** a language | `jails` (root) | Authoring ★★ |
| **Loco** (`loco-gen/src/infer.rs:72`) | Migration intent inferred from its name | jails **derives** instead of guessing — the ledger holds the prior field list, so no `MigrationType::Empty` fallback is needed | `jails-generate` | Correctness ★ |

### 5.2 Patterns jails already has (do not re-propose)

| Source | Pattern | jails' equivalent, and where |
|---|---|---|
| Rails `g scaffold` | One command, a whole vertical slice | `g scaffold` — 17 files in 58 ms, measured |
| Laravel Artisan | Dense generator surface | 36 kinds, 25 capabilities, walked out of the same `clap::Command` (`commands.rs`) |
| Laravel factories | Test data builders with defaults | `g factory`, built on `sample_value` |
| Rails `rails c` / Tinker | REPL against the app | `jails console` (`jshell` + Maven classpath) |
| Rails `routes` | Route table | `jails routes` — reads source, so it works on an app that will not start |
| Phoenix `gen.context` | Explicit bounded contexts | The eleven-layer package model + `Slice::placed`/`owned` |
| Hanami slices | Explicit boundaries, DI | Ports-and-adapters generation + constructor injection throughout |
| Prisma / Ent | Schema as single source of truth | The ledger, which is stronger: it also owns properties, deps, compose services and build features |
| Testcontainers-Go | Ephemeral infra for tests | `add db` writes `@ServiceConnection` beans, deliberately not `@Container` static fields |
| Bazel / Gradle ABI tracking | Test selection from bytecode | `testd --affected` reading constant pools (`classfile.rs`) — including the `CONSTANT_Long` two-slot trap |
| Goose / Flyway | Ordered forward-only migrations | `migration_file`, numeric ordering (`V10` after `V9`), `migrate --check` on a scratch DB |
| SQLx `query!` | Compile-time SQL checking | **Not had.** This is Section 3.1. |
| Air / `mix test.watch` | Live reload | `run --watch`, plus `FATAL_MARKERS` scanning because `spring-boot:run` exits 0 over a dead app |

### 5.3 Patterns deliberately rejected

| Source | Pattern | Why not, in jails |
|---|---|---|
| sqlc `vet` | CEL expressions for user-defined rules | `pending.md` §11: executable hooks / conditional language. Data extensible, logic not |
| Django `makemigrations` | Interactive "did you rename?" questioning | The ledger knows. Guessing is the failure mode `@primary`-is-an-error exists to prevent |
| Prisma / Ent / JDL | A dedicated schema DSL | A second grammar and a second parser; §6.3 records what two parsers of one syntax cost |
| jOOQ | Generated fluent SQL DSL | A generated API surface the size of the schema, plus a runtime jar. Raw SQL is the scope bar |
| Micronaut | Compile-time DI via annotation processors | An annotation processor is a runtime/build dependency jails will not ship |
| PocketBase | Single binary containing the backend | jails generates a project; it is not the project |
| Hotwire / LiveView / HTMX / Livewire / templ / Ziex | Server-driven UI | jails generates APIs. A view layer is a different product |
| Spring Data REST | Repository automatically exposed as REST | Opaque; the whole point of the generated controller is that a reader can read it |
| Reflex / Wasp | Compile one language to another | Neither has a Java analogue that respects "generates pure, transparent, standard Java" |

---

## Section 6: Concrete CLI command specifications

Every command below follows jails' existing conventions: `--json` where the
answer is data, `--pretend` where it writes, an exit code that means something,
a `fix:` line on every failure, and a refusal in preference to a guess.

### 6.1 `jails verify`

```
jails verify [--against <git-ref>] [--json] [--no-cache]

  --against <ref>   apply only the migrations present at <ref>, then check the
                    statements in the working tree. Answers "will what I am
                    about to ship survive the schema that is deployed?"
  --json            machine-readable findings; keeps the exit code
  --no-cache        ignore the analysis cache and re-prepare everything
```

Exit 0 when every recorded statement prepares. Non-zero, via an empty `Err` so
`main` prints no redundant `jails: ` line, when one does not.

```
$ jails verify --against origin/main
jails: applying 7 migrations from origin/main to jails_verify_a41c9 … ok
jails: preparing 23 recorded statements … 22 ok, 1 failed

  FAIL  JdbcOrderRepository.findByStatus
        src/main/java/com/example/shop/adapters/JdbcOrderRepository.java:71
        select id, customer_id, total_minor, status, placed_at from orders
         where status = :status

        ERROR: column "status" does not exist
        LINE 1: ... total_minor, status, placed_at from orders where sta...
                                 ^

        `status` is added by V008__add_status_to_orders.sql, which is in your
        working tree and not in origin/main.

        fix: this is a forward-compatible deploy problem, not a code problem.
             ship V008 before, or with, this code — not after it.

23 statements, 1 failure. exit 1
```

Two things that specification does deliberately. It names the *migration* that
explains the failure, not just the error — `why.rs`'s discipline applied to SQL.
And it distinguishes "your code is wrong" from "your deploy order is wrong",
which is the distinction that makes the command worth running in CI.

### 6.2 `jails migrate --lint`

```
jails migrate --lint [--since <git-ref>] [--json] [--no-start]
```

```
$ jails migrate --lint --since origin/main
jails: 2 migrations are new since origin/main

  V008__add_status_to_orders.sql
    WARN  data-dependent  line 4
          adding a unique index on `orders (status)` will fail if duplicate
          values exist. 1,204 rows are present in the scratch database from
          your seeds; jails cannot know how many are in production.
          fix: create the index concurrently and verify it, or add the
               constraint in a separate deliberate migration.

  V009__drop_legacy_ref.sql
    FAIL  destructive     line 2
          dropping non-virtual column `orders.legacy_ref`.
          2 recorded statements read it: JdbcOrderRepository.findAll,
          OrderQueries.findOverdue.
          fix: retire the readers first — `jails verify --against HEAD` will
               go green once nothing selects it.

1 failure, 1 warning. exit 1
```

The `FAIL` on `V009` is the payoff of `ResourceKey::SchemaObject` +
`ResourceKey::Query` existing together: Atlas can say "you are dropping a
column", and only jails can say **which two of your own generated statements
will break**.

`--lint` needs a scratch database for the data-dependent half and therefore
lives on `migrate` (which writes) rather than on `doctor` (which is read-only by
contract and structurally cannot). The purely static half — destructive,
condrop, incompatible — is added to `doctor` as well, where it costs nothing.

### 6.3 `jails pull`

```
jails pull [<table>...] [--url <jdbc-url>] [--as <Name>] [--all]
           [--schema <name>] [--pretend] [--baseline]

  --url         defaults to the project's own spring.datasource.url
  --as <Name>   override the derived class name (`order_items` → `OrderItem`)
  --all         every table in the schema; refuses if any table is unmappable
                unless --pretend
  --baseline    also write a Flyway baseline so `migrate` will not try to
                re-create tables that are already there
```

`--pretend` output is in Section 4.2. Applied, it runs the ordinary generate
path, so the entity lands in the ledger as if it had been typed, and `destroy`
works on it.

### 6.4 `jails g query-file`

```
jails g query-file <path.sql> [--package <pkg>] [--pretend]
```

```
$ jails g query-file src/main/resources/db/queries/invoice.sql --pretend
plan 0f3a91 apply
  create  src/main/java/com/example/shop/app/InvoiceQueries.java
  create  src/main/java/com/example/shop/adapters/JdbcInvoiceQueries.java
  create  src/main/java/com/example/shop/domain/OverdueInvoice.java
  create  src/test/java/com/example/shop/adapters/JdbcInvoiceQueriesIT.java
  ledger  create

  3 queries: findOverdue :many, countUnpaid :one, markPaid :exec
  types read by preparing each statement against a scratch database built
  from the 8 migrations in db/migration.
```

Removing a query from the `.sql` file and re-running retires its method and its
row record, because each is a `ResourceKey::Query` claim — `destroy`'s existing
semantics, no new machinery.

### 6.5 `jails schema diff`

```
jails schema diff [--live] [--emit] [--json]

  (default)  the ledger's declared schema against the migrations on disk
  --live     the ledger's declared schema against a real database
  --emit     write the migration that closes the gap (refuses on a
             destructive op without --force, and says which op)
```

```
$ jails schema diff --live
jails: ledger declares 4 tables; the database has 5

  orders
    + column  cancelled_at timestamptz null    declared, not present
    ~ column  total_minor  bigint → numeric    present with a different type
  legacy_audit
    ? table                                    present, declared by nobody
                                               (jails will not touch it)

fix: `jails schema diff --emit` writes the two `orders` operations.
     `legacy_audit` is not jails' and is reported, never dropped.
```

The `?` row is the important one. Atlas and Alembic will both happily propose
dropping an undeclared table. jails must not — it is exactly the "a path nobody
owns" case that `pipeline/diff.rs` already handles correctly for files.

### 6.6 `jails --pretend --diff`

```
jails <any writing command> --pretend --diff[=full] [--color=auto|always|never]
```

```
$ jails g field Order note:String? --pretend --diff
plan 7b21c4 apply

  replace src/main/java/com/example/shop/domain/Order.java
    @@ -8,6 +8,7 @@
         Long id,
         String customerId,
         long totalMinor,
    +    Optional<String> note,
         Instant placedAt) {

  create  src/main/resources/db/migration/V009__add_note_to_orders.sql
    + -- Forward-only migration generated for a new record component.
    + alter table orders
    +   add column note text;

  merge   src/main/java/com/example/shop/service/OrderService.java
    generator changed, since the base jails recorded:
      @@ -22,3 +22,3 @@
      -  public Order create(String customerId, long totalMinor) {
      +  public Order create(String customerId, long totalMinor, Optional<String> note) {
    you changed, since the same base:
      @@ -31,0 +32,4 @@
      +    if (totalMinor > 100_000) {
      +      audit.large(customerId, totalMinor);
      +    }
    result: merges cleanly, 0 conflicting hunks.

nothing was written -- run the same command without --pretend to apply it.
```

That merge block is the single most valuable output in this report, and jails is
the only tool in the survey that can produce it, because it is the only one that
records the base it wrote.

### 6.7 `jails add archunit`

```
jails add archunit [--name <Base>] [--dry-run]
```

Writes one `ArchitectureTest.java` rendered from `Config::layers()`, plus the
`com.tngtech.archunit:archunit-junit5` test dependency at a pinned version (the
project has no BOM that manages it). Re-planned by `jails sync`, so renaming a
layer in `jails.toml` updates the rule.

### 6.8 `jails seed`

```
jails g seed <Entity> [--rows 50] [--seed 42]
jails seed [--reset] [--no-start]
```

`--reset` truncates only tables the ledger declares, never one it does not own.

### 6.9 `jails contract`

```
jails contract [--emit <path>] [--against <git-ref>] [--json]
```

Exit non-zero on a breaking change: a removed path, a removed response code, a
newly-required request field, a narrowed type. Prints, in the header of the
emitted document and in `--json`, that it describes what jails generated and not
what the application serves.

### 6.10 `jails model`

```
jails model [--manifest <path>]
```

A TUI over `.jails/app.toml`. Completion comes from `jails commands --json`, so
it cannot drift from the binary. Writing goes through `app apply`, so it is one
transition with the same reconciliation and the same `--pretend`. It is a view,
not a format.
