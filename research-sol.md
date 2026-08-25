# Research Report: 1000x Developer Experience for `jails`

Date: 2026-08-25  
Status: engineering proposal grounded in the current `jails` worktree and upstream implementations

## Research basis and decision frame

This report treats “1000x” as a product direction, not a literal benchmark claim. The measurable objective is to collapse multi-minute, multi-tool workflows into one safe command; keep the common edit/test loop under one second; and make every mutation explainable, previewable, and recoverable.

The research combined:

- the current `jails` Rust workspace and CLI;
- local source checkouts in `deps/`, with implementation-level inspection of Rails generators, Laravel's generator base, Django's migration autodetector, Phoenix context generation, sqlc's compiler pipeline, Quarkus DevServices, Loco migration inference, Wasp's application specification, and PostgREST's schema cache;
- current primary documentation for the JVM, Spring, Quarkus, sqlc, SQLx, Ecto, Django, Rails, Alembic, Prisma, jOOQ, Testcontainers, ArchUnit, FastAPI, Wasp, and JHipster.

The most important baseline finding is that `jails` is already beyond the premise of a greenfield generator. It currently has:

- a field DSL and explicit layer model in `jails-spec`;
- vertical-slice generation, one-field-to-SQL/Java/test projections, fixtures, and raw `JdbcClient` adapters in `jails-generate`;
- pure preparation, three-way merging, one human/JSON result value, and dry-run parity in `jails-prepare`;
- project locking, guarded preimages, a content-addressed object store, write-ahead journals, and roll-forward recovery in `jails-commit`;
- canonical requests and provenance in `jails-protocol` and orchestration in `jails-engine`;
- Java source inspection and JVM constant-pool dependency extraction in `jails-java`;
- a resident `testd`, affected-test selection, build/run tools, database and Kafka consoles in `jails-drive`;
- `doctor`, `why`, `routes`, `beans`, `src`, and `explain` diagnostics in `jails-report`.

That changes the recommendation. The right architecture is not a second generator or a more magical runtime. It is to turn the existing canonical request → desired state → prepared bundle → durable commit pipeline into a **local Java workbench compiler**. One typed intent model should project to Java, SQL, tests, fixtures, contracts, documentation, and terminal explanations. Every projection must retain provenance and verification status.

One evidence limitation matters: the codebase-memory index generation was `2026-08-24T17:06:34Z`, while the worktree metadata had changed and the newly split `jails-drive`, `jails-report`, and `jails-state` paths were not tracked by that generation. The architectural findings above were therefore checked against the current source and crate READMEs rather than trusting stale graph locations.

---

## Section 1: Executive DX Vision and Top 10 Breakthrough Concepts

### The vision: a transparent local workbench, not a framework

The target experience is:

```text
short intent
    ↓ parse once
canonical, versioned model
    ↓ validate and project in memory
Java + SQL + tests + fixtures + contracts + architecture rules
    ↓ prepare once
one reviewable operation graph and diff
    ↓ either preview or durable commit
exactly the same result value
    ↓ warm local workbench
compile/test/run only what the change can affect
```

The governing rules are:

1. **Generated Java is the product.** There is no `jails.jar`, hidden object model, runtime proxy layer, or generated code that only `jails` can execute.
2. **One semantic input, many explicit projections.** A field, relation, route, or SQL query is parsed once and becomes a typed value. Java, DDL, JSON samples, fakes, and tests consume that value rather than reparsing strings or reverse-inferring one another.
3. **Fast paths must prove their eligibility.** If `testd` cannot prove that an affected-test set is safe, it widens to all tests. If SQL analysis is parse-only, output says `parse-only`, never `verified`.
4. **Preview and apply are the same computation.** `--pretend` stops before commit; it does not run a second planning path.
5. **Generated ownership is granular.** `jails` may update the nodes or marked regions it owns. Unowned code is preserved; ambiguous edits become conflicts with evidence.
6. **The terminal is the interface.** Interactive work is a TUI or prompt sequence that emits the same canonical manifest and prepared bundle as non-interactive commands.

### Top 10 concepts, ranked

| Rank | Breakthrough | Adapted mechanism | Why it is high leverage | Primary pillar | First owner |
|---:|---|---|---|---|---|
| 1 | **Canonical Slice Compiler** | Phoenix contexts, Wasp specs, JHipster JDL, FastAPI's one typed declaration | One intent emits a coherent vertical slice and prevents Java/SQL/test drift | Authoring + correctness | `jails-protocol`, `jails-spec`, `jails-generate` |
| 2 | **SQL Contract Compiler** | sqlc catalog/query split, SQLx live/offline checking, jOOQ reverse engineering | Makes raw SQL an asset: named queries become explicit Java records, binders, row mappers, ports, fakes, and contract tests | Correctness + authoring | `jails-spec`, `jails-generate`, `jails-drive` |
| 3 | **Warm Workbench Daemon** | `testd`, Spring DevTools' two-classloader restart, Air-style triggers | Keeps JVM, test discovery, schema catalog, classpath, and file fingerprints warm | Latency | `jails-drive`, `jails-java` |
| 4 | **Safe Affected Graph** | JVM constant pool, ArchUnit bytecode model, build dependency graphs | Transitive bytecode edges select the smallest safe test set; unknowns widen automatically | Latency + trust | `jails-java`, `jails-drive` |
| 5 | **Ambient Dev Services** | Quarkus DevServices and Spring Boot service connections | Missing local Postgres/Kafka/Redis is provisioned, labelled, reused, and explained without app runtime coupling | Latency + trust | `jails-project`, `jails-drive`, `jails-report` |
| 6 | **Three-Way Schema Reconciliation** | Django/Alembic state differencing, Prisma `db pull`, PostgREST catalog cache | Reconciles declared intent, generated baseline, and live DB; never silently chooses an authority | Correctness + authoring | `jails-spec`, `jails-project`, `jails-prepare` |
| 7 | **Evidence-Carrying Mutations** | existing prepared bundles/journals plus Rails `--pretend` ergonomics | Every operation names its owner, reason, precondition, risk, and exact inverse candidate | Trust | `jails-protocol`, `jails-prepare`, `jails-commit` |
| 8 | **Cause-Graph Diagnostics** | Spring FailureAnalyzers, compile-time DI metadata, current `why`/`doctor` | Turns “bean X failed” or “migration Y failed” into a shortest causal path and pasteable fix | Trust + latency | `jails-report`, `jails-project` |
| 9 | **Generated Test Economy** | Ecto SQL Sandbox, Laravel factories, Testcontainers service connections | Fast in-memory unit tests plus transaction-isolated real-DB contracts, generated from the same slice | Latency + correctness | `jails-generate`, `jails-testkit`, `jails-drive` |
| 10 | **Terminal Domain Studio** | Ace prompts, Wasp/JDL, Filament resource schemas | A keyboard-first multi-entity editor compiles to a reviewable manifest; it never owns a separate mutation engine | Authoring + trust | CLI, `jails-spec`, `jails-engine` |

### North-star service levels

Performance targets should be budgets with visible fallback reasons, not unconditional promises:

| Workflow | Warm target | Cold target | Correctness fallback |
|---|---:|---:|---|
| Parse a slice/manifest and produce a plan | p95 ≤ 50 ms | p95 ≤ 150 ms | Refuse malformed or ambiguous input |
| Render `--pretend` for a normal slice | p95 ≤ 100 ms | p95 ≤ 300 ms | Show which cache missed |
| `testd` run of one already-loaded unit test | p50 ≤ 25 ms, p95 ≤ 60 ms | JVM start reported separately | Restart daemon on classpath/test-engine drift |
| Affected test selection | ≤ 20 ms after compilation | ≤ 100 ms graph rebuild | Run all tests on an unknown edge or uncompiled source |
| Static SQL check from cached catalog | p95 ≤ 75 ms/query | p95 ≤ 500 ms/catalog | Mark parse-only or require live verification |
| Live SQL verification against a reused Postgres | p95 ≤ 250 ms/query | container startup separately timed | Refuse if server/catalog fingerprint differs |
| `doctor`, source-only routes/beans | p95 ≤ 150 ms | p95 ≤ 500 ms | State the static-analysis boundary |

Each command should emit timing spans under `--debug` or `--output json`: `discover`, `parse`, `observe`, `project`, `prepare`, `verify`, `commit`, and external process/container time. This makes latency work empirical.

---

## Section 2: Pillar 1 — Sub-Second Feedback Loops

### 2.1 Evolve `testd` into a warm workbench

The current daemon already removes the largest fixed cost: JVM startup. Its next version should keep five versioned caches per project root:

1. **Build model** — active module, Java release, source roots, classpath, compiler/test plugin configuration.
2. **File fingerprint table** — content hash and metadata for sources, resources, POM/Gradle files, migrations, and query files.
3. **Bytecode graph** — class → referenced types and reverse type → referrers, split into production and test owners.
4. **Test inventory** — JUnit unique IDs, source class, tags, last outcome, duration, and last bytecode digest.
5. **SQL catalog** — normalized schema fingerprint plus per-query analysis records.

The local IPC protocol should be explicit and versioned:

```text
HELLO protocol=2 project=<canonical-root-id> client=<jails-version>
SYNC changed=[path,digest,...] build_digest=<digest>
RUN selectors=[junit-unique-id...] mode=affected|failed|all epoch=<n>
RESULT epoch=<n> discovered=12 selected=3 passed=3 duration_ms=47 fallback=null
```

An epoch binds results to a source/build snapshot. A reply from epoch 41 cannot be shown as the result of epoch 42. POM, Gradle settings, JVM version, JUnit engine, or classpath changes invalidate the JVM process; ordinary bytecode changes invalidate only graph nodes and test outcomes.

The safe selection algorithm is:

```text
changed source
  → compiled owner classes (including nested classes)
  → reverse transitive CONSTANT_Class / descriptor references
  → test owners
```

This is grounded in the class-file format: the JVM constant pool contains symbolic class, field, method, interface, name/type, and descriptor information. The existing `referenced_types` parser is therefore the right cheap substrate. The JVM specification documents the one-based constant-pool table and its symbolic references ([JVM Specification §4](https://docs.oracle.com/javase/specs/jvms/se21/html/jvms-4.html)).

Constant-pool reachability is conservative but not complete. The selector must run **all tests**, with the reason printed, when any of these occurs:

- a changed Java source has no compiled owner class;
- annotation processors or generated sources changed;
- tests are selected through strings, resources, reflection, `ServiceLoader`, Spring configuration properties, SQL/migration files, or external contracts not represented by bytecode edges;
- class parsing sees an unknown class-file tag/version;
- the build/classpath fingerprint changed;
- a deletion or rename cannot be mapped to the previous class owner.

Add explicit dependency hints for the irreducible cases:

```toml
# jails.toml
[[test_dependency]]
input = "src/main/resources/db/migration/**"
tests = ["**/*RepositoryIT", "**/*MigrationIT"]

[[test_dependency]]
input = "src/main/resources/application*.properties"
tests = ["**/*ContextIT"]
```

The crucial UX is the explanation:

```text
$ jails testd --affected
selected 7/184 tests
  4  bytecode reverse dependencies of OrderService
  2  rule: db/migration/** → *RepositoryIT
  1  previous failure
fallback: none
47 ms test execution; 13 ms selection
```

### 2.2 Incremental compilation without pretending to be a compiler

`jails` should orchestrate the project's compiler rather than own Java semantics.

- If the IDE or language server has produced fresh `.class` files, use them.
- Otherwise run the narrowest supported build-tool compile goal and measure it.
- Cache the resolved wrapper, classpath, compiler release, and output roots by build-file digest.
- Watch directories with OS notifications, but always rescan hashes after overflow or wake-up; watcher events are hints, not truth.
- Coalesce changes for a short quiet window (for example 20–40 ms), then compile once.

For running applications, prefer Spring Boot's documented two-classloader restart model: stable dependencies stay in a base loader while application classes are replaced through a restart loader ([Spring Boot DevTools](https://docs.spring.io/spring-boot/reference/using/devtools.html)). Default `jails dev` behavior should be an explicit classloader restart, with output like `restart: structural change Order.class`; it should not claim live replacement.

JDK instrumentation can redefine classes, but support is optional and active frames continue running old bytecode; unsupported structural changes fail atomically ([Java 21 Instrumentation API](https://docs.oracle.com/en/java/javase/21/docs/api/java.instrument/java/lang/instrument/Instrumentation.html)). That makes arbitrary HotSwap a poor default for a zero-magic tool. If explored later, it should be opt-in, dev-process-only, limited to verified method-body-compatible changes, and always print `redefined` versus `restarted`. It must never become a generated production dependency.

### 2.3 Incremental source and AST indexes

`jails routes`, `beans`, field evolution, and merge previews should share a persistent source index keyed by `(canonical path, content digest, parser version)`. Store only facts derivable from source:

- package and top-level/nested types;
- record components and annotations;
- imports and constructor parameters;
- Spring stereotype, bean, and mapping annotations;
- generated ownership anchors and syntax spans.

On edit, parse the changed file and update its edges in one transaction. Consumers see an immutable index epoch. A parse failure preserves the last good facts only as `stale`; it may not silently present them as current. Terminal output must say:

```text
routes: 18 current, 2 unavailable
  src/main/java/.../LegacyController.java:87 parse failed near `}`
```

This extends the current lightweight `jails-java` approach. A full Java compiler front-end is unnecessary for owned edits, but a selector-based edit IR is necessary:

```rust
enum JavaEdit {
    AddImport { qualified: JavaType },
    AddAnnotation { target: TypeId, annotation: Annotation },
    AddRecordComponent { target: TypeId, component: FieldSpec },
    AddMember { target: TypeId, anchor: MemberAnchor, source: String },
}
```

The edit applies only if its target is unique and its preconditions match. Otherwise it produces an explainable conflict before commit.

### 2.4 Ambient Dev Services owned by the CLI

Quarkus' transferable insight is a decision rule: in dev/test mode, if an extension is present and an external connection is not configured, start and wire a local service. The service remains out of the production artifact ([Quarkus Dev Services overview](https://quarkus.io/guides/dev-services)). `jails` can adapt that without a runtime extension:

```text
jails dev/test/sql command
  → inspect declared capabilities and explicit config
  → if external endpoint exists: use it, start nothing
  → else find a matching labelled local container
  → else start one from an explicit, pinned service spec
  → pass connection details as process environment/system properties
  → print what was selected and why
```

Every service identity should hash image digest/tag, ports, environment, init scripts, and relevant migrations. Containers get labels such as:

```text
dev.jails.project=<root-id>
dev.jails.service=postgres
dev.jails.spec=<sha256>
dev.jails.managed=true
```

Commands:

```text
jails services up [postgres|kafka|redis|all]
jails services status [--output json]
jails services logs <name> [--follow]
jails services reset <name> --pretend
jails services down [--all-projects]
```

Rules that preserve trust:

- Explicit application configuration always wins; say `postgres: external (spring.datasource.url)`.
- Image versions are pinned in the capability spec and visible in the plan.
- Reuse is opt-in and local. Testcontainers calls reusable containers experimental and unsuitable for CI, so generated CI must use clean ephemeral instances ([Testcontainers reusable containers](https://java.testcontainers.org/features/reuse/)).
- Parallel-start independent containers; Testcontainers exposes parallel `deepStart` for this purpose ([advanced options](https://java.testcontainers.org/features/advanced_options/)).
- Readiness is semantic (`SELECT 1`, broker metadata), not “container is running.”
- CI defaults to isolated, disposable services; local development may reuse by exact spec hash.
- The CLI owns orchestration. Generated Java may use standard Spring Boot `@ServiceConnection` in integration tests, which maps container details into ordinary connection beans ([Spring Boot Testcontainers](https://docs.spring.io/spring-boot/reference/testing/testcontainers.html)).

### 2.5 Instant schema catalog and SQL contract verification

sqlc's implementation separates schema catalog construction, query parsing/rewriting, semantic analysis, and code generation. It caches catalog analysis and maps errors in rewritten SQL back to the developer's original source. Its documented flow is simply “write SQL → generate type-safe code,” with optional database-backed enhanced analysis and per-query caching ([sqlc generate](https://docs.sqlc.dev/en/latest/howto/generate.html)). SQLx adds a useful trust model: a query may be checked against a live `DATABASE_URL` or against checked-in offline metadata, and CI can verify that metadata is current ([SQLx `query!`](https://docs.rs/sqlx/latest/sqlx/macro.query.html)).

`jails` should expose three non-confusable verification levels:

| Level | Evidence | Allowed result label | Use |
|---|---|---|---|
| Parse | dialect parser accepts syntax and named-parameter rewrite | `parsed` | editor-speed feedback |
| Offline catalog | migrations/schema snapshot resolves tables, columns, parameters, outputs and nullability | `verified-offline` | disconnected development/CI |
| Live catalog | server prepares/describes query against exact schema fingerprint | `verified-live` | release gate and difficult PostgreSQL semantics |

Unknown analysis is not success. A query outside the offline analyzer's supported subset becomes `needs-live-verification`, never a guessed Java type.

Cache keys must include:

```text
dialect + server-major + normalized-schema-digest + query-digest
+ search_path + analyzer-version + type-override-digest
```

Store only non-secret metadata under `.jails/cache/sql/`; never persist connection passwords or full production URLs. A checked-in contract file belongs under `.jails/sql-contracts/` and contains query digest, schema digest, parameter/result columns, nullability, dialect, and verifier version.

Recommended SQL file convention:

```sql
-- name: FindPayableOrders :many
-- timeout: 250ms
SELECT id, user_id, total, status, created_at
FROM orders
WHERE status = :status
  AND total >= :minimum
ORDER BY created_at, id
LIMIT :limit;
```

Generated code uses an explicit `RowMapper` lambda, never `query(Order.class)`, because the latter is convenient reflection-based mapping. The contract compiler generates:

- `FindPayableOrdersParams` record;
- `FindPayableOrdersRow` record when the result is not exactly a domain record;
- a query port and `JdbcClient` implementation;
- deterministic bind order/name mapping;
- an in-memory fake whose filtering and stable ordering mirror the declared semantics where feasible;
- a live contract test that prepares/executes the query against Testcontainers;
- a frozen metadata check in CI.

### 2.6 Latency architecture and invalidation table

| Input changed | Reparse | Recompile | Restart JVM | Rebuild SQL catalog | Test scope |
|---|---|---|---|---|---|
| Java method body | one file | owning module incremental compile | no for tests; app restart-loader refresh | no | reverse bytecode dependents |
| Record shape / method signature | one file + dependents on compile error | owning module | yes for running app | maybe if query record is generated | reverse dependents, widen on failed compile |
| `pom.xml` / Gradle build | build model | build tool decides | yes | if JDBC/dialect/config changed | all |
| Migration | migration AST | generated contracts if schema changed | app restart if running | yes | migration + repository contracts |
| Named query SQL | query only | generated query sources | no until class changes compile | query entry only | query contract + known callers |
| `application.properties` | property file | usually no | app restart | if datasource/search path changed | context tests |
| Template override | template + placeholders | affected generated files after accepted plan | after compile | if SQL output changes | generated companion tests |

---

## Section 3: Pillar 2 — Correctness, Trust, and Zero Puzzlement

### 3.1 Make the prepared bundle the universal explanation

The current prepare/commit split is the strongest differentiator in `jails`; all new features should enter it rather than bypass it. Extend each operation with evidence:

```text
operation_id
path
kind: create | replace | delete | mkdir | effect
owner: capability/entity/intent
reason: human sentence + stable reason code
source_inputs: paths and content digests
precondition: absent | exact digest | owned-node digest
verification: parsed | compiled | verified-offline | verified-live | unverified
risk: additive | behavior-change | destructive | external-effect
inverse_candidate: operation id or unavailable reason
```

Human and JSON output must be projections of this same value. `--pretend` renders it and stops. Apply rechecks the captured preconditions under the project lock and commits the same plan; it never replans.

### 3.2 Dry-run and diff contract

Default `--pretend` output should be concise, with expansion flags:

```text
$ jails g scaffold Order ... --with-events --pretend
PLAN  16 operations, 0 conflicts, verified-live SQL

CREATE  src/main/java/com/acme/orders/domain/Order.java
        because entity Order owns its domain record
CREATE  src/main/resources/db/migration/V014__create_orders.sql
        because Order is persistent and Flyway is installed
EDIT    pom.xml  +1 dependency
        org.archunit:archunit-junit5 (test)

RISK    additive 15 · behavior-change 1 · destructive 0
NEXT    rerun without --pretend to apply transaction 01K...
```

Flags:

```text
--diff                 colorized unified file diffs
--ast                  semantic edit view (AddImport, AddRecordComponent, ...)
--why                  provenance/reason/input digests
--verify               run compile/SQL/architecture verification before accepting
--output json          stable machine-readable envelope
--no-color             deterministic logs
```

Color is redundant decoration; `+`, `-`, operation verbs, and risk labels must carry meaning without it. Large generated files default to a summary and require `--diff` to expand. Secrets in properties, environment, or connection strings are redacted before either renderer receives them.

### 3.3 AST-aware merge with honest conflict boundaries

AdonisJS demonstrates the transferable split between templates for new files and AST codemods for host-file edits; its development-only assembler keeps AST machinery out of production applications ([AdonisJS scaffolding and codemods](https://docs.adonisjs.com/guides/concepts/scaffolding)). For `jails`:

1. New owned files render from templates.
2. Owned semantic nodes edit through typed selectors and preconditions.
3. Regeneration performs base/current/desired three-way reconciliation.
4. Unowned overlaps produce a conflict in the plan; no write occurs.
5. Text merge is a final compatibility path, not proof of AST correctness.

Each conflict should show the selector and candidates:

```text
CONFLICT src/main/java/.../Order.java
  wanted: add component `status: OrderStatus` to record Order
  found:  two top-level records named Order after parse recovery
  kept:   current file unchanged
  fix:    make the target unique, then rerun; or use --package/--type
```

There should be no `--force` that discards unowned Java. A narrowly named `--accept-generated <operation-id>` may resolve a known conflict only after the resulting diff is shown and becomes part of a new prepared bundle.

### 3.4 Bidirectional consistency as a three-source reconciliation problem

Schema/code synchronization has three states, not two:

```text
D = declared intent (jails.toml / app manifest / SQL contracts)
G = last generated owned baseline (ledger + content objects)
L = live observations (current files and optional database catalog)
```

Reconciliation rules:

- `D = G, L = G`: no-op.
- `D ≠ G, L = G`: safe forward generation from a changed declaration.
- `D = G, L ≠ G`: user/live drift; preserve and report it.
- `D ≠ G, L ≠ G`: three-way merge if changes do not overlap; otherwise conflict.
- live DB-only object: import candidate, ignored object, or conflict—never silently delete.
- missing live DB object: destructive migration candidate requiring explicit acceptance.

Django's autodetector compares whole project states and orders operations because foreign keys and many-to-many changes interact. Alembic likewise produces candidate migrations from metadata/database differences and explicitly warns that candidates require review ([Django migrations](https://docs.djangoproject.com/en/6.0/topics/migrations/), [Alembic autogenerate](https://alembic.sqlalchemy.org/en/latest/autogenerate.html)). `jails schema diff` should adopt both ideas: global dependency ordering and mandatory review for rename/destructive ambiguity.

Rename detection is heuristic evidence, not fact:

```text
POSSIBLE RENAME orders.customer_id → orders.buyer_id (confidence 0.82)
  same type/nullability/FK target; old removed and new added
  choose: --accept-rename | --treat-as-drop-add
```

### 3.5 Diagnostics as causal graphs

Spring Boot itself uses `FailureAnalyzer` implementations to turn startup exceptions into a description and action, with the condition evaluation report as a deeper fallback ([Spring Boot startup failures](https://docs.spring.io/spring-boot/reference/features/spring-application.html)). `jails` can go further because it sees source, build files, capabilities, migrations, Compose, and prior generated ownership.

Normalize evidence into a graph:

```text
symptom → framework cause → missing/ambiguous declaration → project evidence → fixes
```

Examples:

```text
$ jails why --last
OrderController could not be created
└─ needs OrderService
   └─ needs OrderRepository
      ├─ JdbcOrderRepository exists but is not a bean
      │  evidence: no @Repository and no @Bean factory
      └─ InMemoryOrderRepository is test-scoped
fix:
  jails add db --pretend
  or annotate exactly one production adapter
```

```text
$ jails why migration V014
V014 failed at statement 3: foreign key target users(id)
└─ users.id is uuid
└─ orders.user_id is bigint
origin: Order.user:ref resolved from live catalog at schema digest 7a1...
fix: jails schema change Order user:uuid --pretend
```

Command family:

- `jails why [log-file|--last|bean <name>|migration <version>|query <name>] [--evidence] [--output json]`
- `jails doctor [--scope env|build|services|schema|wiring|ownership|all] [--fix --pretend]`
- `jails routes [--conflicts] [--openapi] [--output json]`
- `jails beans [--missing] [--cycles] [--path <bean>] [--output dot|json|human]`
- `jails explain <command|artifact|operation-id> [--why-generated]`

Static versus runtime evidence must be labelled. Source-only `routes` is fast and works on a broken app, but cannot know runtime-conditional mappings. A future `--runtime` mode may boot an isolated app and compare actual mappings; it must not replace the static default.

### 3.6 Architectural fitness gates

ArchUnit analyzes Java bytecode and exposes rules for packages, layers, slices, cycles, and onion/hexagonal architectures ([ArchUnit guide](https://www.archunit.org/userguide/html/000_Index.html)). A scaffold should generate one project-level `ArchitectureTest`, not one duplicate per entity.

Rules for the default layout:

- domain depends only on the JDK, JSpecify annotations, and domain packages;
- application ports may depend on domain, not Spring or adapters;
- web/JDBC/messaging adapters may depend inward;
- adapters do not depend on one another;
- slices under the configured slice package are cycle-free;
- only adapter/configuration packages use Spring stereotypes;
- raw JDBC appears only in JDBC adapters;
- controllers return response DTOs, not repository/JDBC types.

For adoption into an existing project, generate a baseline file that records existing violations and fail only on new ones. ArchUnit's `FreezingArchRule` validates this incremental adoption pattern, but `jails` should keep the baseline in an explicit, reviewable project file rather than hide it in machine-local state.

### 3.7 Reversibility after commit

Crash recovery and user undo are different:

- an **active** journal always rolls forward to a coherent commit;
- `jails undo <receipt>` creates a **new forward transaction** whose desired images are the prior preimages;
- undo rechecks that current files still match the original receipt's after-images;
- user changes after the commit cause a three-way merge or refusal;
- external effects (container starts, database migrations already applied) are listed separately and are never implied to be undone by restoring files.

Rails migrations demonstrate why reversibility must be explicit: known operations can be reversed, while arbitrary SQL needs declared up/down behavior ([Rails migrations](https://guides.rubyonrails.org/active_record_migrations.html)). For SQL migrations, generate `-- rollback:` only where safe and still require `jails migrate undo` to preview data-loss risk. Production defaults should be forward-fix migrations, not automatic rollback.

---

## Section 4: Pillar 3 — Ultra-High-Velocity Authoring

### 4.1 A backward-compatible slice DSL

Keep the existing compact field tokens and extend them through a typed grammar rather than ad hoc suffix checks:

```ebnf
field       = name, ":", type, { modifier } ;
type        = builtin | java_type | enum | ref | list | set | map ;
modifier    = "?" | "!" | "^" | annotation | default | check ;
enum        = "enum{", name, { ",", name }, "}" ;
ref         = "ref<", java_type, [ ".", name ], ">" ;
annotation  = "@", ("pk" | "scope" | "index" | "unique" | "audit") ;
default     = "=", literal ;
check       = "[", predicate, "]" ;
```

Proposed command:

```text
jails generate scaffold Order \
  id:uuid@pk \
  userId:ref<User.id>! \
  total:money![positive] \
  status:enum{PENDING,PAID,CANCELLED}=PENDING \
  createdAt:instant@audit \
  --index status,createdAt \
  --with-events OrderPaid \
  --with-audit
```

Parsing produces stable values such as `FieldSpec`, `RelationSpec`, `IndexSpec`, `EventSpec`, and `PolicySpec`. `money` should expand visibly to a configurable value object strategy—prefer `BigDecimal` plus currency or a generated `Money` record—rather than silently choosing cents or floating point.

The command must print normalization in debug/JSON output:

```text
userId:ref<User.id>! → column user_id uuid not null references users(id)
total:money![positive] → BigDecimal + numeric(19,4) + check(total > 0)
```

### 4.2 Contexts and slices, not CRUD bags

Phoenix's context generator makes the domain boundary a first argument and augments an existing context with conflict prompts ([`mix phx.gen.context`](https://phoenix.hexdocs.pm/1.7.7/Mix.Tasks.Phx.Gen.Context.html)). `jails` should allow:

```text
jails g scaffold Billing.Order ...
jails g scaffold Support.Order ...
```

The same noun may exist in different slices. Package layout, ports, migrations, and route prefixes derive from the slice. Cross-slice references require an explicit port or event; a generated ArchUnit rule enforces it. `jails.toml` records the mapping so singularization or package guessing is never the durable identity.

### 4.3 Schema-first: `pull`, `introspect`, and `schema diff`

Prisma's `db pull` reads a live schema into a model and preserves a defined subset of manual schema customizations on repeated introspection ([Prisma introspection](https://www.prisma.io/docs/orm/prisma-schema/introspection)). jOOQ demonstrates the Java value of reverse-engineering types and constraints from a real database so schema changes become compile failures ([jOOQ code generation](https://www.jooq.org/doc/latest/manual/code-generation/)). PostgREST shows which catalog facts matter for an instant API—tables, columns, primary/foreign keys, functions, and relationships—and why caching them matters ([PostgREST schema cache](https://postgrest.org/en/v10/schema_cache.html)).

`jails` should separate observation from mutation:

```text
jails introspect db [--schema public] [--table glob] [--output human|json|manifest]
jails pull [--schema public] [--table glob] [--into-slice Billing] --pretend
jails schema diff [--from declared|migrations|live] [--to ...]
jails schema check [--frozen]
```

`introspect` is read-only. `pull` creates or reconciles declarations and generated code through the normal prepared bundle. First pull establishes a baseline. Later pulls preserve aliases, ignored objects, naming overrides, and owned customization in `jails.toml`.

Catalog model minimum:

```text
Catalog
  schemas
    tables/views
      columns(type, nullability, default, identity/generated, comment)
      primary/unique/check constraints
      indexes (columns, expressions, predicate, method)
      foreign keys (columns, target, on update/delete, deferrability)
    enums/domains
    routines relevant to queries
```

Unsupported vendor constructs become explicit `Opaque` nodes carried through diffs. They are never dropped because the parser did not understand them.

### 4.4 SQL-first workflow

The complete workflow is:

```text
1. Write/edit migrations or pull a catalog.
2. Write named `.sql` queries.
3. `jails sql check` resolves parameters/results against a catalog.
4. `jails sql generate --pretend` shows Java/contract changes.
5. Apply through the transaction engine.
6. `jails testd --affected` runs generated contracts and callers.
7. CI runs `jails sql check --frozen --live` against a clean migrated DB.
```

sqlc's named query commands (`:one`, `:many`, `:exec`) are worth adapting because cardinality is a contract, not something inferable from arbitrary SQL. sqlc also expands `SELECT *` to explicit columns in generated code to prevent unexpected result drift ([sqlc selecting rows](https://docs.sqlc.dev/en/v1.23.0/howto/select.html)). Proposed annotations:

```sql
-- name: FindOrder :optional
-- domain: Order
-- timeout: 100ms

-- name: CreateOrder :one
-- transaction: required

-- name: CancelOrder :execrows
-- expect-rows: 1
```

The generator should reject `SELECT *` by default for stored contracts, require stable ordering for pageable/many queries, require a bound limit or explicit `--unbounded`, and flag writes without an expected row count.

### 4.5 Validation as an explicit boundary

Ecto changesets separate external casting/filtering, application validation, and database constraints ([Ecto Changeset](https://ecto.hexdocs.pm/Ecto.Changeset.html)). Generated Java should preserve that separation without introducing a changeset runtime:

- domain record compact constructor: intrinsic invariants that are true everywhere;
- web request record: transport parsing and Jakarta validation;
- application command/use case: authorization and state-transition rules;
- database migration: unique, foreign-key, and check constraints;
- exception advice: stable RFC 9457 problem responses.

This prevents the common generator error of putting every rule on a persistence entity or duplicating inconsistent checks across controller and repository.

### 4.6 Fakes, factories, seeds, and contracts

Laravel's factories provide reusable defaults, named states, sequences, and relationship composition; its test helpers reset state and can run seeders ([Laravel factories](https://laravel.com/framework/docs/10.x/eloquent-factories), [database testing](https://laravel.com/framework/docs/12.x/database-testing)). Ecto's SQL Sandbox uses explicit connection ownership and transaction rollback to enable concurrent PostgreSQL tests ([Ecto SQL Sandbox](https://ecto-sql.hexdocs.pm/Ecto.Adapters.SQL.Sandbox.html)). Adapt the principles, not their runtimes:

- generate `OrderFactory` in test sources with deterministic defaults, `.paid()`, `.cancelled()`, and `.withUserId(...)` states;
- generate a thread-safe `InMemoryOrderRepository` implementing the same port, with deterministic ordering and uniqueness behavior;
- generate JSON fixtures for readable stable examples and Java factories for combinatorial tests;
- generate `db/seeds/*.json` plus a plain Java `SeedRunner` that uses repository ports; production execution requires an explicit profile/flag;
- use `@Transactional` rollback for in-process integration tests and unique schema/database names for tests that spawn threads or commit independently;
- generate a repository contract interface executed once against the fake and once against `JdbcOrderRepository`, so semantic drift becomes a failing test.

The fake must document what it does not emulate: locking, isolation, vendor collation, constraint timing, SQL planner behavior. Those remain live-database contract tests.

### 4.7 Policy and contract generation

Add optional matrices to the manifest:

```toml
[[entity]]
name = "Order"
slice = "Billing"

[[entity.policy]]
action = "read"
roles = ["SUPPORT", "BILLING"]
scope = "userId == principal.userId || hasRole('SUPPORT')"

[[entity.event]]
name = "OrderPaid"
version = 1
fields = ["id", "userId", "total", "paidAt"]
```

Generate a sealed policy decision type, explicit authorizer port, table-driven unit tests, Spring adapter configuration, event record, JSON Schema/OpenAPI component, and producer/consumer contract tests. A policy matrix is high-risk: `--pretend` should summarize added/removed permissions separately from ordinary file edits.

### 4.8 Terminal-native studio

`jails studio` is a TUI front-end to `jails app`, not another data model.

```text
┌ Entities ──────────┐ ┌ Order ───────────────────────────────┐
│ User               │ │ id        uuid       pk required     │
│ Order              │ │ userId    → User.id  required        │
│ Invoice            │ │ total     money      > 0             │
└────────────────────┘ │ status    enum       PENDING          │
                       └────────────────────────────────────────┘
 F2 add field  F3 relation  F4 policy  F8 preview  F10 write manifest
```

Every keystroke edits an in-memory canonical manifest. F8 calls the same planner as `jails app plan`; F10 writes the manifest through the same transaction engine; applying the generated project changes is a separate confirmed action. The TUI supports `--record session.json` so interaction tests can replay deterministic events. Non-TTY invocation refuses or uses `--from manifest`, never hangs waiting for input.

---

## Section 5: Cross-Ecosystem Pattern Translation Matrix

Impact codes: **L** = feedback latency, **C** = correctness/trust, **A** = authoring velocity. “Adapt” means extract the mechanism into generated standard Java or CLI behavior; it does not mean adding the source framework as a runtime dependency.

| Source ecosystem / tool | Core DX innovation | `jails` adaptation for Java | Affected crate(s) | Impact |
|---|---|---|---|---|
| Ruby / [Rails](https://github.com/rails/rails) | Convention-led scaffold hooks; migrations; seeds; console | Composable slice recipes, global `--pretend`, explicit migration/seed artifacts, `jshell` classpath console | spec, generate, prepare, drive | L/C/A |
| Ruby / [Hanami](https://github.com/hanami/hanami) | Explicit slices and dependency boundaries | First-class slice identity, inward ports, cross-slice events, generated ArchUnit rules | protocol, spec, generate | C/A |
| Ruby / [Sinatra](https://github.com/sinatra/sinatra) | Minimal route declaration | Small route/controller generator with no new runtime abstraction | spec, generate | A |
| Ruby / [Roda](https://github.com/jeremyevans/roda) | Routing tree; branch-level middleware hoisting | Compile route/policy trees into ordinary Spring mappings and shared interceptor configuration | spec, generate, project | C/A |
| Ruby / [Kamal](https://github.com/basecamp/kamal) | Small, explicit zero-downtime deployment primitives | Generate inspectable container/deployment files and preflight commands; do not build a deploy runtime | generate, drive, report | C/A |
| Ruby / [Turbo](https://github.com/hotwired/turbo) | Server-driven partial UI updates | Optional generation of standard Spring MVC + HTMX/Turbo-compatible fragments; no `jails` browser runtime | generate | A |
| PHP / [Laravel](https://github.com/laravel/framework) | Dense Artisan generators, factories, seeders, Tinker | Command aliases, deterministic factories/states, seed plans, richer console completion | spec, generate, report, drive | L/A |
| PHP / [Filament](https://github.com/filamentphp/filament) | Resource schema drives forms/tables/actions | Use the slice model to generate admin API contracts and terminal resource summaries, not a web GUI | spec, generate, report | A |
| PHP / [Livewire](https://github.com/livewire/livewire) | Server state and automatic DOM diffs | Generate explicit SSE/HTMX endpoints where requested; reject implicit component runtime coupling | generate | A |
| Python / [Django](https://github.com/django/django) | Whole-project state diff and ordered migration operations | Schema graph differ, dependency ordering, rename candidates, migration preview/check | spec, project, prepare, generate | C/A |
| Python / [FastAPI](https://github.com/fastapi/fastapi) | One typed declaration drives validation and OpenAPI | `FieldSpec` drives domain/request validation, ProblemDetail, OpenAPI components, examples | spec, generate | C/A |
| Python / [Reflex](https://github.com/reflex-dev/reflex) | High-level declarations compile to another stack | Treat manifests as compiler inputs, but emit plain Java and standard assets with no `jails` runtime | protocol, spec, generate | A |
| Python / [Alembic](https://github.com/sqlalchemy/alembic) | Extensible metadata/database diff; candidate migrations | Typed `SchemaOp` IR, dialect comparators, mandatory review for destructive/ambiguous changes | spec, project, prepare | C/A |
| Elixir / [Phoenix](https://github.com/phoenixframework/phoenix) | Context generator makes API boundaries explicit | `Slice.Entity` syntax, augment-existing flow, scoped resource generation | spec, generate, engine | C/A |
| Elixir / [Phoenix LiveView](https://github.com/phoenixframework/phoenix_live_view) | Minimal state diffs over a persistent connection | Generate ordinary SSE/WebSocket ports/controllers and contracts, without a hidden state runtime | generate | A |
| Elixir / [Ecto](https://github.com/elixir-ecto/ecto) | Changesets and concurrent transactional SQL sandbox | Separate transport/domain/DB validation; transaction-isolated repository tests and explicit connection ownership | generate, testkit, drive | L/C |
| JS/TS / [AdonisJS](https://github.com/adonisjs/core) | Ace CLI, validated prompts, stubs plus AST codemods | Testable prompts, typed edit IR, ejectable templates with placeholder contracts | CLI, java, generate, prepare | C/A |
| JS/TS / [RedwoodJS](https://github.com/redwoodjs/redwood) | Full-stack vertical slice generation and Cells | Generate server-side slice plus OpenAPI/client contract boundary; adapt declarative loading/error states to tests | spec, generate | A |
| JS/TS / [CedarJS](https://github.com/cedarjs/cedar) | Continued Redwood-style generators and conventions | Track the maintained conventions independently; keep `jails` recipe/version compatibility explicit | spec, generate | C/A |
| JS/TS / [Wasp](https://github.com/wasp-lang/wasp) | Typed declarative application spec for routes/auth/jobs | Evolve `jails app` into the durable multi-slice authoring surface and TUI target | protocol, spec, engine, generate | C/A |
| JS/TS / [Prisma](https://github.com/prisma/prisma) | Schema DSL, generated client, migrations, repeated introspection | `pull`/`schema diff`, preserved naming overrides, checked-in SQL contracts; generate raw JDBC rather than a client runtime | spec, project, generate, prepare | C/A |
| JS/TS / [Next.js](https://github.com/vercel/next.js) | File conventions and co-located server actions | Optional file/package conventions and co-located HTTP request collections; avoid implicit runtime boundary magic | spec, generate | A |
| JS/TS / [HTMX](https://github.com/bigskysoftware/htmx) | Hypermedia actions declared in HTML attributes | Generate standard MVC fragment endpoints and contract examples when selected | generate | A |
| JS/TS / [tRPC](https://github.com/trpc/trpc) | End-to-end type inference with tiny authoring surface | Generate OpenAPI/JSON Schema and typed client fixtures from Java contracts; compile mismatch rather than share a runtime | spec, generate | C/A |
| Go / [sqlc](https://github.com/sqlc-dev/sqlc) | SQL is source; catalog analysis emits typed structs/methods | Named SQL contracts → Java records, explicit `JdbcClient` binders/mappers, fakes and checks | spec, generate, drive | L/C/A |
| Go / [Encore](https://github.com/encoredev/encore) | Static analysis derives infrastructure and tracing | Infer *candidates* from explicit annotations/capabilities, show plan, generate Compose/config/telemetry code | project, generate, report | C/A |
| Go / [Ent](https://github.com/ent/ent) | Schema-as-code graph traversal and migrations | Relation graph in canonical manifest; generate ports and explicit SQL joins, not an ORM runtime | protocol, spec, generate | C/A |
| Go / [Bun](https://github.com/uptrace/bun) | SQL-first builder with struct mapping | Keep SQL visible and generate explicit mapper lambdas; offer small query-building helpers only as source | generate | C/A |
| Go / [templ](https://github.com/a-h/templ) | Type-checked templates compile to functions | Validate owned templates at generation and compile generated Java views/components in verification gates | java, generate, drive | C/A |
| Go / [Huma](https://github.com/danielgtaylor/huma) | Type signatures drive validation and OpenAPI | One request/response spec generates Spring records, validation, examples, and OpenAPI | spec, generate | C/A |
| Go / [Fuego](https://github.com/go-fuego/fuego) | Generics/types drive web contracts | Same Huma/FastAPI transfer, constrained to generated standard Spring MVC | spec, generate | C/A |
| Go / [Goa](https://github.com/goadesign/goa) | Design-first API DSL emits servers, clients, docs | Add API intent to manifests and generate controllers, client contracts, docs and tests | protocol, spec, generate | C/A |
| Go / [PocketBase](https://github.com/pocketbase/pocketbase) | One binary supplies a complete local backend | Preserve `jails` single-binary installation and offer coherent capability bundles, while generated apps stay ordinary Java | CLI, generate, drive | L/A |
| Go / [Goravel](https://github.com/goravel/goravel) | Laravel-like structure and CLI in Go | Borrow command density, aliases, factories, jobs and migration naming—not facades/runtime service location | CLI, spec, generate | A |
| Go / [Air](https://github.com/air-verse/air) | Custom-trigger rebuild/restart loop | `jails dev --include/--exclude/--delay` with explicit restart reason and command | drive | L |
| Go / [goose](https://github.com/pressly/goose) | Embeddable, ordered SQL/function migrations | Ordered SQL migration inspection and checksum gates; keep Java migration functions optional and explicit | project, drive, report | C |
| Go / [Testcontainers-Go](https://github.com/testcontainers/testcontainers-go) | Ephemeral real infrastructure in tests | Generate equivalent Testcontainers Java configuration and service contracts | generate, testkit | C/A |
| Rust / [Loco](https://github.com/loco-rs/loco) | Rails-style scaffolds; migration intent inferred from names | Optional friendly migration-name parser that prints its normalized `SchemaOp` before generation | spec, generate | A |
| Rust / [Axum](https://github.com/tokio-rs/axum) | Typed extractors composed with Tower middleware | Generate explicit request types and standard Spring interceptors/filters from typed route policy | spec, generate | C/A |
| Rust / [SQLx](https://github.com/launchbadge/sqlx) | Live/offline compile-time SQL checking | Live and frozen catalog verification levels with CI staleness check | spec, drive, report | L/C |
| Rust / [SeaORM](https://github.com/SeaQL/sea-orm) | Entity generation from live schemas | `jails pull` catalog → domain/port/adapter generation without shipping ORM entities | project, spec, generate | A |
| Rust / [Diesel](https://github.com/diesel-rs/diesel) | Compile-time query/schema compatibility | Turn catalog mismatch into generator/CI errors and Java compile breakage through generated types | spec, generate, drive | C |
| Rust / [utoipa](https://github.com/juhaku/utoipa) | AST-derived OpenAPI | Static source index produces OpenAPI and compares it to generated contract baseline | java, report, generate | C/A |
| Rust / [Leptos](https://github.com/leptos-rs/leptos) | Fine-grained reactivity and full-stack contracts | Adapt dependency granularity to invalidation/test selection; optional standard SSE/HTML generation | java, drive, generate | L/A |
| Rust / [Dioxus](https://github.com/DioxusLabs/dioxus) | Hot RSX iteration | Adapt file-trigger and fine-grained rebuild UX, not its UI runtime | drive | L |
| Rust / [Shuttle](https://github.com/shuttle-hq/shuttle) | Infrastructure requested through code declarations | Convert source annotations to explicit proposed capabilities and previewed generated infrastructure | project, generate, prepare | C/A |
| Zig / [Jetzig](https://github.com/jetzig-framework/jetzig) | Build-time routes/templates and integrated primitives | Precompute route/template facts in CLI cache; emit simple Java | java, generate, drive | L/A |
| Zig / [http.zig](https://github.com/karlseguin/http.zig) | Minimal allocation-conscious request lifecycle | Generate bounded, explicit request handling and streaming; surface allocation/perf tests in optional profiles | generate, testkit | C |
| Zig / [Zap](https://github.com/zigzap/zap) | Thin high-performance HTTP wrapper | Keep generated controller adapters thin and benchmarkable | generate, drive | L/C |
| Zig / [Ziex](https://github.com/ziex-dev/ziex) | Embedded JSX compiled at build time | If views are added, compile checked templates to ordinary Java methods/resources | java, generate | C/A |
| Zig / [zzz](https://github.com/tardy-org/zzz) | io_uring-oriented async server | Research signal for generated load/streaming profiles only; no attempt to replace Spring I/O | drive, generate | L |
| JVM / [Spring Boot](https://github.com/spring-projects/spring-boot) | starters, auto-configuration, FailureAnalyzers, DevTools | Generate conventional Boot configuration; inspect conditions; orchestrate restart and service connections explicitly | project, generate, drive, report | L/C/A |
| JVM / [Spring Data REST](https://github.com/spring-projects/spring-data-rest) | Repository-to-REST exposure | Generate the equivalent controller/service/port code explicitly so API behavior remains visible | generate | A/C |
| JVM / [Quarkus](https://github.com/quarkusio/quarkus) | DevServices and build-time metadata | CLI-owned ambient services and cached source/build facts; no Quarkus runtime required | project, drive, report | L/C |
| JVM / [JHipster](https://github.com/jhipster/generator-jhipster) | JDL multi-entity/relationship authoring | Extend `jails app` manifest and terminal studio with typed relations, policies and events | protocol, spec, generate | A/C |
| JVM / [jOOQ](https://github.com/jOOQ/jOOQ) | DB reverse engineering and type-safe schema model | Borrow catalog breadth and compile-break-on-schema-change while emitting raw `JdbcClient`, not jOOQ runtime | project, spec, generate | C/A |
| JVM / [Micronaut](https://github.com/micronaut-projects/micronaut-core) | Compile-time DI/AOP metadata | Build a source/bytecode bean graph and generate explicit wiring checks; avoid runtime reflection metadata | java, project, report | L/C |
| JVM / [Ktor](https://github.com/ktorio/ktor) | Coroutine-native routing DSL | Adapt composable route-tree authoring to generated Spring mappings, preserving explicit middleware order | spec, generate | C/A |
| JVM / [ArchUnit](https://github.com/TNG/ArchUnit) | Architecture as executable bytecode rules | Generate one project fitness suite and incremental adoption baseline | generate, testkit, drive | C |
| BaaS / [Supabase](https://github.com/supabase/supabase) | Postgres catalog, auth/storage/realtime as coherent services | Offer explicit capability bundles and schema/policy introspection; never hide service ownership | project, generate, drive | A |
| BaaS / [PostgREST](https://github.com/PostgREST/postgrest) | Cached catalog turns tables/FKs/functions into API shape | Reuse its catalog categories for `pull`, relationship inference and invalidation | project, spec | L/C/A |
| BaaS / [Hasura](https://github.com/hasura/graphql-engine) | Instant GraphQL and relationship reflection | Generate optional GraphQL/OpenAPI adapters from catalog with reviewed auth/policy declarations | spec, generate | A/C |
| BaaS / [Directus](https://github.com/directus/directus) | Live schema introspection and admin resources | Terminal resource explorer and manifest generation; no web admin surface in `jails` | report, spec, generate | A |
| BaaS / [Appwrite](https://github.com/appwrite/appwrite) | Integrated auth/jobs/storage/functions bundle | Coherent, opt-in capability recipes with explicit Compose/config ownership | project, generate | A |
| BaaS / [Nhost](https://github.com/nhost/nhost) | Integrated Postgres/auth/GraphQL/storage | Same capability-bundle lesson; generated ports prevent vendor services entering the domain | project, generate | C/A |

### Patterns deliberately not transplanted

- ActiveRecord/JPA-style runtime entities and lazy loading: they conflict with records, ports, raw SQL, and transparent execution.
- runtime repository-to-API exposure: fast to start, but it hides authorization, DTO, error, and transaction behavior.
- server-held UI component runtimes: outside the CLI/TUI scope; only standard endpoint generation is transferable.
- infrastructure inferred and applied without a plan: static inference may propose capabilities, but the prepared diff remains the authority.
- generator plugins executing arbitrary third-party code: template/data extension is compatible with the trust model; an in-process plugin runtime is not.

