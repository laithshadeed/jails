# Research Report: 1000x Developer Experience for `jails`

**Author:** Grok (Cursor), systems / DX research  
**Sources:** `prompt.md`, the `jails` workspace (README, crates, templates), and upstream checkouts under `deps/` (118 directories as of 2026-08-25).  
**Date:** 2026-08-25  
**Status:** Research synthesis. Proposals only; nothing in this file is an implementation plan that has been accepted into `pending.md`.

---

## How this research was done

`prompt.md` asked for a cross-ecosystem survey translated into `jails`-shaped commands, generated Java, and crate owners. The method was:

1. Read the current `jails` surface (`README.md`, `ArtifactKind` / `Capability` in `crates/jails-spec/src/spec/kind.rs`, `testd` / `affected` / `classfile`, `sql.rs`, `doctor` / `why`, the prepare/commit pipeline).
2. Inspect implementations in `deps/` rather than recalling marketing pages. The highest-signal files are cited inline.
3. Translate a mechanism only when it survives the constraints in `prompt.md` and `README.md`: **no `jails.jar`**, **no ORM**, **CLI / future TUI only**, **no plugin lifecycle hooks**, **no `jails dev` supervisor**, **migrations remain forward-only**, **`jails check` stays `mvn clean verify`**.

A "1000x" claim that ignores what already ships is fiction. The useful multiplier is **remaining latency × remaining puzzlement × remaining authoring ceremony**, measured against today's loop (save → jdt.ls → `testd` / devtools → `why` / `doctor`).

### Catalogue coverage

Present and inspected under `deps/`: Rails, Laravel, Django, Alembic, Phoenix, Ecto, Adonis, Wasp, Prisma, sqlc, Encore, Ent, Bun, templ, Huma, Fuego, PocketBase, Goravel, goose, Loco, Axum, SQLx, utoipa, Leptos, Dioxus, Jetzig, http.zig, Zap, Ziex, Spring Boot, Quarkus, JHipster, ArchUnit, PostgREST, Atlas, Hasura (`graphql-engine`), Directus, Supabase.

Listed in `prompt.md` and **not** cloned (patterns covered by siblings already on disk): FastAPI / Huma, Micronaut / Quarkus build-time, HTMX / templ, Next.js / Wasp, tRPC / field-spec as SSoT, Diesel / SQLx, Hanami / jails layers, Kamal / `add docker`+`add k8s`, Air / Boot DevTools, Spring Data REST (rejected: magic REST from repositories).

---

## Table of contents

1. [Section 1: Executive DX vision and top 10 breakthrough concepts](#section-1-executive-dx-vision--top-10-breakthrough-concepts)
2. [Section 2: Pillar 1 — sub-second feedback loops](#section-2-deep-dive-into-pillar-1--sub-second-feedback-loops)
3. [Section 3: Pillar 2 — correctness, trust, and zero puzzlement](#section-3-deep-dive-into-pillar-2--correctness-trust--zero-puzzlement)
4. [Section 4: Pillar 3 — ultra-high velocity authoring](#section-4-deep-dive-into-pillar-3--ultra-high-velocity-authoring)
5. [Section 5: Cross-ecosystem pattern translation matrix](#section-5-cross-ecosystem-pattern-translation-matrix)
6. [Section 6: Concrete CLI command specifications](#section-6-concrete-cli-command-specifications)
7. [Section 7: Generated Java code blueprints](#section-7-generated-java-code-blueprints)
8. [Section 8: Implementation roadmap and crate-by-crate plan](#section-8-implementation-roadmap--crate-by-crate-architecture-plan)

---

## Section 1: Executive DX Vision & Top 10 Breakthrough Concepts

### What `jails` already is

`jails` is not a green-field Rails clone. It already implements the expensive half of several famous DX tricks:

| Famous trick | Already in jails | Evidence |
|---|---|---|
| Rails `g scaffold` that you can hit with HTTP | `g scaffold` writes a running resource (record, port, JDBC + in-memory adapters, DTOs, service, controller, migration, fixture, tests, `.http`) | `README.md`; `crates/jails-generate/src/generate/scaffold.rs` |
| Phoenix bounded contexts / explicit layers | Eleven layers owned by `config::LAYERS_IN_ORDER`; `--package` override | `CLAUDE.md`; `jails.toml` `[layout]` |
| sqlc "one column list" | `sql::columns` feeds DDL, select, insert, bind, and row mapper together | `crates/jails-generate/src/sql.rs` |
| Resident test JVM | `jails testd`: 0.06–0.10 s vs ~0.62 s `--fast`; **does not compile** | `crates/jails-drive/src/testd.rs`; `templates/testd/JailsTestDaemon.java` |
| Reverse-dep test selection | `testd --affected` from constant pools | `crates/jails-java/src/classfile.rs`; `crates/jails-drive/src/affected.rs` |
| Atomic, journaled mutations | prepare → commit; `--pretend`; crash roll-forward | `jails-prepare`, `jails-commit` |
| Failure translation | `why` (grouped rules from real logs), `doctor` (every FAIL has `fix:`) | `crates/jails-report/src/why.rs`, `doctor.rs` |
| Declarative app | `.jails/app.toml` + `jails app apply` as one transition | `src/app.rs` |
| DevServices-ish local infra | `add db`/`kafka`/`redis` write compose; `run`/`start` bring it up; Testcontainers as `@Bean` + `@ServiceConnection` | `add/database.rs`; README save-and-reload loop |
| Schema evolution without destroy/regenerate | `g field` refreshes derived files, reports hand-edits | `generate/scaffold.rs` |

The remaining pain is therefore not "Java cannot feel like Rails". It is three specific holes:

1. **The SQL jails emits is derived from a field spec, but the SQL the reader writes by hand is untyped.** `g query` / `g repo` cannot see a `.sql` file. sqlc's whole product is that inversion.
2. **Nothing asks Postgres whether a query is still true after a migration.** `jails migrate --check` applies Flyway to a scratch DB (syntax + apply). It does not `PREPARE` application SQL against the resulting catalog the way SQLx `query!` and `sqlc vet` / `sqlc/db-prepare` do.
3. **The fast loop is still a command you remember to type.** jdt.ls already writes `target/classes` on save; `testd` already runs in tens of milliseconds; there is no watcher that connects the two without inventing `jails dev`.

A 1000x leap is the product of closing those holes while keeping every mutation explicit.

```
DX ≈ (time-to-green after an edit)⁻¹
   × (chance the tool told the truth) 
   × (production-grade bytes produced per keystroke)
```

Today's time-to-green for a unit test that does not need a container is already ~100 ms if you remember `testd --affected`. The remaining orders of magnitude live in (a) forgetting that command, (b) waiting on a container or a Maven reactor, (c) discovering at runtime that a column was renamed, (d) writing a custom query by hand.

### Top 10 breakthrough concepts

These are ordered by expected impact under the constraints, not by novelty.

#### 1. SQL-first query files (`jails sql`) — sqlc, inverted

**Source:** `deps/sqlc`. `Compiler.parseCatalog` reads schema/migration SQL, strips rollback/psql meta (`internal/migrations`), parses queries whose first line is `-- name: GetAuthor :one`, then `internal/codegen/golang` emits structs and methods. Named params are `sqlc.arg(name)` (`docs/howto/named_parameters.md`).

**Why it feels fast:** the query the DBA would write *is* the source of truth. Types fall out. There is no ORM between the SQL and the call site.

**jails translation:** a new kind `sql` (or `query-file`) that reads `src/main/resources/sql/<name>.sql`, builds a catalog from `db/migration/*.sql` the same way sqlc builds one from schema files, and emits:

- a Java record for `:one` / `:many` result columns
- a port method + `JdbcClient` adapter using **named** parameters (already the house style in `jdbc_repository` / `jdbc_query_java.java`)
- an in-memory fake that implements the same port
- a companion test

This is the inverse of today's `sql.rs` (field spec → SQL). Both directions must share one catalog type so they cannot disagree about `timestamptz` vs `timestamp with time zone`.

**Crate:** catalog + SQL AST in `jails-java` (read-only analysis) or a focused module beside `sql.rs`; emission in `jails-generate`; identity in `jails-protocol` (`ArtifactKind`).

**Reject:** shipping sqlc as a subprocess that emits Go, or a Java annotation processor (that is a compile-time runtime in the project).

#### 2. Offline SQL verifier (`jails sql check`) — SQLx `query!` without macros

**Source:** `deps/sqlx/README.md`: `query!` connects at compile time, `PREPARE`s the statement, caches descriptors; offline mode writes `.sqlx/` so CI has no database (`sqlx-cli/README.md`). `deps/sqlc/docs/howto/vet.md`: CEL rules plus `sqlc/db-prepare`. `docs/howto/verify.md`: check *new schema* against *old queries* (expand/contract).

**Why it feels correct:** a renamed column is a compile error, not a 500 at 3am.

**jails translation:** two modes, both CLI, nothing in the generated JAR:

1. **Catalog mode (default, no daemon):** parse migrations into an in-memory catalog; type-check every `db.sql("""...""")` string jails itself wrote *and* every file under `src/main/resources/sql/`. Fast, works offline, cannot see `SELECT` that uses a Postgres function the catalog does not know.
2. **Prepare mode (`--live`):** `PREPARE` against the compose Postgres `jails db` already knows how to reach. Cache the results under `.jails/sql-cache/` (ledger-adjacent, not a project dependency). CI replays the cache when `SQL_OFFLINE=1`.

`jails migrate --check` stays the apply-to-scratch path. `sql check` is the *query* path. Doctor remains read-only: it can *report* a stale cache, not apply migrations.

**Crate:** `jails-drive` (live Postgres, like `migrate.rs`); `jails-report` for the doctor row; cache format in `jails-protocol`.

#### 3. `testd --watch` on `target/classes`, not a new supervisor

**Source:** Gradle daemon / JUnit console reuse (already measured in `testd.rs`); Quarkus continuous testing; Air (not cloned — DevTools already covers process restart).

**Why it feels fast:** the 464 ms cold JUnit tax is paid once; the editor already compiled.

**jails translation:** `jails testd --watch` inotify/fanotify-watches `target/classes` and `target/test-classes`, then runs `--affected` (constant-pool reverse graph). It **must not compile**. It **must not** be named `jails dev` — `README.md` is explicit that the save-and-reload loop is jdt.ls + DevTools, and a supervisor would lie when that loop is silently broken (the `reload` doctor check exists for that).

Optional: default `testd` without args in a TTY to `--watch --affected` *after* printing the staleness rule once.

**Crate:** `jails-drive` (`testd.rs` + `affected.rs`).

#### 4. Desired-state schema diff (`jails schema diff` / `jails g migration --from-model`)

**Source:** Django `MigrationAutodetector` (`deps/django/django/db/migrations/autodetector.py`) diffs model state vs migration graph. Alembic `compare_metadata` (`deps/alembic/alembic/autogenerate/api.py`) diffs SQLAlchemy `MetaData` vs `Inspector`. Atlas (`deps/atlas/README.md`) declarative desired-state vs live DB, plus versioned SQL. Loco `infer::guess_migration_type(name)` (`deps/loco/loco-gen/src/migration.rs`) infers `AddColumns` from `add_columns_to_users`.

**Why it feels fast:** you stop writing `V00N__add_foo.sql` by hand after `g field` already knew the column.

**jails translation:** the desired state is the union of field specs on recorded scaffolds (ledger), not a Hibernate model. The actual state is either (a) the Flyway files on disk, parsed, or (b) `information_schema` of the running compose DB. Diff emits the next `VNNN__….sql` as a **preview** under `--pretend`, then the same create-only write path.

Keep migrations **forward-only** (README). Do not generate `down`. Loco's reversible `remove_column` in `down` is the wrong import.

`g field` already refreshes Java. This command is the missing SQL half when the reader added a column to the record by hand, or when they pulled schema from prod.

**Crate:** catalog parse in the same place as (1); `jails-generate` `migration.rs`; `jails-drive` for live inspect.

#### 5. Live catalog pull (`jails pull`) — PostgREST's `pg_class` walk, generate-only

**Source:** `deps/postgrest/src/library/PostgREST/SchemaCache.hs` — one startup query against catalogs for tables, columns, FKs, routines. Hasura/Directus/Supabase are the same idea with more product around it.

**Why it feels fast:** an existing database becomes a jails project in one command.

**jails translation:** `jails pull [--schema public] [--table notes]` reads `pg_catalog` via `psql` (already how `db` / `migrate --check` talk to Postgres). Emits `g scaffold` / `g record` + `g association` recipes as `--pretend` first. Never starts a PostgREST sidecar. Never exposes REST from the catalog at runtime (that would be Spring Data REST, which is magic and out of scope).

**Crate:** `jails-drive` (connection) + `jails-generate` (artifacts) + `jails-engine` (one transition).

#### 6. Interactive `--pretend` TUI — not a web GUI

**Source:** Rails generator conflict prompts (`phx.gen.context` `prompt_for_code_injection` in `deps/phoenix/lib/mix/tasks/phx.gen.context.ex`); git's interactive add; sqlc's "show me the Go".

**Why it feels trustworthy:** you see the unified diff *and* the AST-level three-way merge (`jails-prepare/src/pipeline/diff.rs` already computes operations and conflict hunks) before anything hits the journal.

**jails translation:** when stdout is a TTY and `--pretend` is off, `generate`/`add`/`app apply` can `--interactive` (or default for multi-file scaffolds): pager of per-file diffs, keys to skip/apply/abort. Conflicted merges (`pending.md` §11: continue/abort not built) belong here as the human protocol for frozen conflict records.

**Crate:** `jails-engine` routing + a small TUI in the binary crate. No new web port.

#### 7. `why` as a growing, evidence-gated table — plus query-shaped Flyway

**Source:** existing `RULES` in `crates/jails-report/src/why.rs` (docker-env, missing-bean, flyway-checksum, datasource, …). The file's own law: add rules only from failures that happened.

**Gap:** Spring `FailureAnalyzer` messages, circular `@Configuration`, Flyway *validate* vs checksum, `sqlc verify`-style "this migration breaks this query". Those are still logs.

**jails translation:** keep the table. Add groups only with a captured log in `tests/`. New group `sql-prepare` from (2). New group `devtools-silent` tying to the existing doctor `reload` check so `jails run --watch` failures that look like "nothing happened" get a sentence.

**Crate:** `jails-report` only.

#### 8. Generated ArchUnit fitness tests from `LAYERS_IN_ORDER`

**Source:** `deps/archunit` (layered architecture rules as unit tests). Spring Modulith (`deps/spring-modulith`) is the Spring-native cousin; jails already encodes layers in config, so ArchUnit is the portable assertion.

**Why it feels correct:** a controller that `new JdbcNoteRepository()` is a compile-green, architecture-red test in Surefire — the same "unknown widens" honesty as `--affected`.

**jails translation:** `add arch` (or emit from `g scaffold` once) writes one test class: domain must not import `web`/`adapters` JDBC types; `web` must not import `JdbcClient`; only one `@Repository` per port (the wiring rule already documented). Uses ArchUnit on the test classpath — a **test** dependency, not a runtime jar, so it does not violate the scope bar.

**Crate:** `jails-generate` capability plan; `jails-spec` `Capability::Arch`.

#### 9. Named-command migrations + richer field DSL — Loco / Rails attributes, without ActiveRecord

**Source:** Rails scaffold USAGE (`title body:text tracking_id:integer:uniq`); Loco `guess_migration_type`; Phoenix `phx.gen.context Accounts User users name:string`.

**jails already has:** `name:type[!?]` plus `@pk` `@unique` `@index` `@positive` `@scope`; `g field`; `g association`; `enum{…}` is the prompt's example and is **not** in the field parser today as inline enum literals.

**jails translation:**

- Inline enums: `status:enum{PENDING,PAID,CANCELLED}` → `g enum` + column `text`/`varchar` + `valueOf` (the one owned type `sql.rs` already maps).
- `g migration add_title_to_notes title:string!` infers `ALTER TABLE notes ADD COLUMN …` (Loco) while still writing Flyway SQL, not SeaORM Rust.
- `--with-events` / `--with-audit` on scaffold as **opt-in extra recipes** in one transition (existing `g event`, timestamps), not new runtime interceptors.

**Crate:** `jails-protocol` field parser; `jails-generate`.

#### 10. Seeds, factories, and an Ecto-style *test* sandbox — without a jails runtime

**Source:** Laravel factories/seeders; Rails `db/seeds`; Ecto SQL sandbox (not in the `ecto` checkout — it lives in `ecto_sql`; the *idea* is checkout-per-test transactions so concurrent `async: true` tests share one DB). `g factory` already exists (`generate/domain.rs` `factory_java`).

**Gap:** no first-class seeder; ITs still start Testcontainers; `testd` cannot cheaply run a JDBC IT against compose with isolation.

**jails translation:**

- `g seed Notes` writes a Java class that uses the repository port (so in-memory tests stay in-memory) plus a `main` for `jails run -- SeedNotes`.
- Document `spring.datasource` + Flyway against compose for local ITs; optional `jails testd --db` that exports `SPRING_DATASOURCE_URL` to the already-running compose Postgres and wraps each IT in `BEGIN … ROLLBACK` via a **generated** `TestTx` utility in the project (plain JDBC, no jails class).
- Do not implement Postgres snapshot isolation in Rust.

**Crate:** `jails-generate` + `jails-drive` for `--db` env injection only.

### Explicitly rejected (constraint or honesty)

| Idea | Why not |
|---|---|
| `jails dev` process supervisor | README: the loop is jdt.ls + DevTools; doctor `reload` exists because silent breakages are the failure mode a supervisor would hide |
| `jails rund` / Spring hot-swap of records | Domain is records; signature changes are restarts. Selling "50 ms handler reload" would be DevTools lying |
| Web "studio" / desktop GUI | Prompt: CLI and future TUI only |
| `jails.jar` SQL interceptor | Scope bar |
| Spring Data REST / PostgREST-in-process | Runtime magic from the catalog |
| Hibernate, Liquibase-as-ORM, Ent-style graph runtime | No ORM |
| Plugin hooks | README Not yet |
| Reversible `down` migrations | Forward-only by design |
| sqlc Cloud `verify` | Network product; local catalog + `--live PREPARE` is the honest subset |
| Encore Cloud / Shuttle annotations | Infra-from-code that provisions AWS is a different product; `jails.toml` capabilities are the local analogue |

---

## Section 2: Deep Dive into Pillar 1 — Sub-Second Feedback Loops

### 2.1 The loop that already exists

```
:w  →  jdt.ls writes target/classes  →  DevTools restart (app)
                                  ↘  jails testd (tests, ~20–100 ms warm)
```

Measured facts from `testd.rs` / README:

- First JUnit session in a JVM: **464 ms**; warm: **~20 ms**.
- `testd` vs `--fast`: **0.06–0.10 s** vs **~0.62 s** for one method.
- Daemon classpath **must not** include `target/classes` (parent-first stale hits). Documented in `JailsTestDaemon.java`.
- Daemon **must not compile** (`testd.rs` module docs; plan.md §19.5 via comments).
- `--affected` widens on every unknown (`affected.rs`): no git, no class file, unreadable pool → `Everything(reason)`.

Pillar 1 work is **connecting and tightening this**, not replacing Maven as the source of truth (`jails check` remains `mvn clean verify` because incremental `target/` lies about deleted tests).

### 2.2 `testd --watch`

Mechanism: watch the **output** directories, not `src/`. The language server is the compiler. On each quiet period (reuse DevTools' 50 ms quiet / 200 ms poll numbers from `spring-devtools.properties` so the two halves feel like one product):

1. `launcher::staleness` — refuse and print `fix: jails test` if source is newer than class (LSP lagged).
2. `affected::select` — constant-pool graph.
3. Unix socket `RUN` to the existing daemon.

Constant-pool cost: `classfile.rs` is already "smallest reader that answers which types this class names", including Utf8 descriptor scan and the `CONSTANT_Long`/`Double` two-slot trap. Watching can **incrementally** re-absorb only changed `.class` files instead of walking all of `target/` each time — a `jails-java` API `referenced_types` already returns `BTreeSet<String>`. Keep "unknown widens".

Do not index the Maven classpath. Edges to `java.util.List` are dropped on purpose (`affected.rs`).

### 2.3 Incremental graph, not incremental javac

Encore and Micronaut pay for build-time processing in the **framework**. jails must not. The analogue of Encore's parser (`deps/encore/v2`, `parser/`) is:

- **Source:** `java.rs` `blanked()` for annotations/routes (`jails routes` / `beans`).
- **Bytecode:** `classfile.rs` for test selection.

A third index — SQL strings inside `db.sql("""…""")` — is what concept (2) needs. Extracting those strings is a `blanked()` walk of `src/main/java`, not a compiler plugin. Cache hashes of (file, query text) → catalog fingerprint in `.jails/sql-cache/`.

### 2.4 DevServices without putting Testcontainers in the hot path

Quarkus `DevServicesDatasourceProcessor` (`deps/quarkus/extensions/datasource/deployment/.../DevServicesDatasourceProcessor.java`) starts a container **if** launch mode is dev/test **and** no JDBC URL is set, via `DevServicesDatasourceProvider` (Postgres implementation uses Testcontainers `PostgreSQLContainer`).

jails already chose a more explicit split:

- **App process:** compose + `spring.datasource.*` from `compose.yaml` + `spring-boot-docker-compose` (with `spring.docker.compose.skip.in-tests=true`).
- **Tests:** `TestcontainersConfig` `@Bean` `@ServiceConnection`, `@Import`ed only into `@SpringBootTest`.

That split is load-bearing (README: global `spring.factories` started Postgres for every `@WebMvcTest`). Do not collapse it into Quarkus-style "missing URL ⇒ start container" inside generated Java.

Pillar 1 improvement is **CLI-side**:

- `jails testd --watch` never starts Docker.
- `jails testd --db` (concept 10) uses compose, which `jails start` already owns.
- `doctor` already checks the engine, the socket Testcontainers will see, and compose provider (podman vs Docker Compose v2). Keep starting infra in `run`/`start`/`add`, never in generated production code.

Compose DevServices in Quarkus (`ComposeDevServicesProcessor`) is closer to what jails has. Treat Quarkus as validation of the design, not a rewrite.

### 2.5 Fast SQL

sqlc's `parseCatalog` is the right architecture for jails:

1. Glob migration files.
2. Strip Flyway/psql noise (`RemoveRollbackStatements`, `RemovePsqlMetaCommands` in `deps/sqlc/internal/migrations`).
3. Parse into a catalog.
4. Resolve each query against that catalog (`query_catalog.go`, `output_columns.go`, `resolve.go`).

Postgres `PREPARE` (SQLx) is the **fallback** for functions, casts, and `search_path` the in-process parser will get wrong. `migrate --check` already creates a scratch database on the **same server** as compose so extensions match (`migrate.rs` docs). Reuse that for `--live` analysis: apply migrations to scratch, `PREPARE` each query, drop scratch. Do not use `postgres:latest` in CI if the app is 16 + pgvector.

Latency budget:

| Step | Target | Notes |
|---|---|---|
| Parse all `V*.sql` in a typical project | < 20 ms | sqlc is Go; a Rust parser (e.g. wrapping `pg_query`) must stay in this band or we shell to sqlc **as a hermetic binary**, not a library in generated apps |
| Catalog-check 50 queries | < 50 ms | |
| Live `PREPARE` of 50 queries on warm local PG | < 200 ms | round-trip bound |
| testd --affected after class change | < 30 ms select + 20–80 ms JUnit | already in this range |

If a Rust pg parser is too wrong too often, **shelling to `sqlc compile` against generated `sqlc.yaml` pointing at `db/migration` + `src/main/resources/sql`** is allowed: sqlc is a **developer tool**, like `psql` and `mvn`, not a runtime. Generated Java still uses `JdbcClient` only.

### 2.6 Hot reload: tell the truth

DevTools restarts on classpath change. Record/component/sealed/annotation changes **are** restarts. `doctor` `reload` already names: missing DevTools, `restart.enabled=false`, `trigger-file`. Pillar 1 is not "50 ms Spring handler swap". It is "tests in 80 ms and an honest restart of the app".

Jetzig/Zig compile-time routes and templ's compile-to-Go functions are the wrong import for Spring MVC. They are the right import for **`jails routes` remaining a source scan** (instant, works if the app will not start).

---

## Section 3: Deep Dive into Pillar 2 — Correctness, Trust & Zero Puzzlement

### 3.1 Mutations the reader can replay

The prepare pipeline (`jails-prepare/src/pipeline/diff.rs`) already distinguishes: unowned path vs owned path vs three-way merge vs conflict hunks. `--pretend` runs checks and prints the plan without the commit executor.

Puzzlement remaining:

- Multi-file scaffolds dump a file list; they do not show a **colorized unified diff** per path in the terminal (unless the user runs `diff` themselves).
- Conflicts: `pending.md` §11 — markers can be produced conceptually, but **continue/abort commands do not exist**; jails refuses instead. That is the honest v1. The TUI in concept (6) is how continue/abort should land **as one piece**.

Proposal: `--diff` (always available) prints unified diffs for every `FileOp`. `--interactive` pages them. JSON `--pretend` already exists on reporting commands; generate should grow a `--json` plan for editors (Neovim already uses `jails commands --json`).

### 3.2 Diagnostics: `why`, `doctor`, `routes`, `beans`, `explain`

**`why`:** signature ∩ group ∩ most-specific. Law: unrecognised → say so. Do not infer. Expand only with captured logs (existing test `every_root_cause_seen_in_real_logs_is_recognised` pattern).

Highest-value new signatures from the sqlc/SQLx survey:

- Flyway checksum mismatch (group exists: `flyway-checksum`) — keep.
- `column … does not exist` / `ERROR: column` from JDBC — map to `jails sql check` / `g field`.
- Ambiguous column after `ADD COLUMN` (the exact `sqlc verify` anecdote in `docs/howto/verify.md`) — `sql-prepare` group.

**`doctor`:** environment vs wiring vs `capability_drift_checks` (re-plans `add::plan_for`). New checks:

- SQL cache fingerprint ≠ current migrations.
- ArchUnit test missing if capability recorded.
- `testd` socket stale (daemon dead but pid file — if any).

**`routes` / `beans`:** stay source-based. README Not yet: no booted context. Encore's live MCP (`go_llm_instructions.txt`) is the opposite choice (runtime introspection). For jails, source is the feature (works when the context will not start). Optionally later: `jails beans --runtime` as an explicit, refused-by-default experiment — not in v1 of this research.

**`explain <kind>`:** hand-written table; `every_kind_has_an_explanation`. New kinds (`sql`, `seed`, `arch`) need a row in the same change.

### 3.3 AST-aware merge vs sqlc regenerate

sqlc regenerates whole files and tells you not to edit `*.sql.go`. jails **owns** generated Java but **three-way merges** reader edits (`diff.rs`). That is the harder, correct product.

SQL-first files should be **reader-owned SQL** + **jails-owned Java adapters**. Same ownership as templates vs `pom.xml` marked blocks. If the reader edits `JdbcListNotesQuery.java` by hand, `jails sql` refuses or merges; it must not silently clobber. The ledger entity is the `.sql` file; Java is derived.

### 3.4 Compile-time-verifiable SQL

Without annotation processors:

```
db/migration/*.sql  ──► Catalog
src/main/resources/sql/*.sql  ──► Queries ──► check vs Catalog
generated Jdbc*  ──► extract SQL literals ──► same check
```

`sqlc vet` CEL rules that matter to copy as **hardcoded** Rust checks (not a CEL interpreter in v1):

- `:exec` with a `RETURNING` (mismatched command).
- Query not using an index — **skip** (needs live `EXPLAIN`; too magical / unstable).
- `SELECT *` in generated code — jails already expands `COLUMNS` on purpose; **allow** in generated adapters, **warn** in hand-written `.sql` files.

SQLx offline directory ↔ `.jails/sql-cache/*.json` with schema version in the envelope (`jails-protocol` already owns magic/schema/checksum for the ledger).

### 3.5 Architectural fitness

Generate `ArchitectureTest` (name TBD) with ArchUnit:

- Packages from `Config::layers()` (honour `jails.toml` renames — the inspect.rs lesson).
- `..adapters..` JDBC implementations may depend on `JdbcClient`; `..web..` may not.
- No `java.sql` in `domain`.
- Slice tests: `g usecase` ports stay free of Spring web types.

This is the ArchUnit import of what `tests/architecture/` already does for **jails' own Rust**. Dogfooding the idea.

Spring Modulith application modules are heavier than we need; skip unless a proof app is a modulith.

### 3.6 Reversibility

`destroy` is ledger-driven (no `KIND_FILES`). SQL-first entities destroy derived Java and **leave** the `.sql` if the reader created it? No: if the entity owns both, destroy both, with `--pretend`. Migrations still cannot be destroyed.

`remove arch` unsplices the test dependency and deletes the generated test.

---

## Section 4: Deep Dive into Pillar 3 — Ultra-High Velocity Authoring

### 4.1 Vertical slice DSL

Today:

```
jails g scaffold Note id:uuid@pk title:string! body:text? --index "title"
```

Target (still one clap parse, still `FieldSpec::parse`):

```
jails g scaffold Order \
  id:uuid@pk \
  user:ref \
  total:money \
  status:enum{PENDING,PAID,CANCELLED} \
  --with-events \
  --with-audit
```

Meaning, all existing kinds, one transition (`dispatch::one_transition_each` already re-resolves the project between capabilities; the same must happen between recipes in one `generate` line if `--with-events` needs the record on disk):

| Token | Existing / new |
|---|---|
| `user:ref` | owned type / FK; `g association` if `--on` known |
| `total:money` | existing field type vocabulary (`BUILTIN_FIELD_TYPES`) |
| `status:enum{…}` | **new** inline enum → `g enum Status` + field `status:Status` |
| `--with-events` | `g event` payload + publisher stubs |
| `--with-audit` | `created_at`/`updated_at` timestamps (scaffold already has a timestamps story) |

Phoenix `phx.gen.context Accounts User users …` maps to **package / slice**, which jails already has as `--package` and layers. Do not add a second "context" noun.

Wasp's `app({ … })` maps to **`.jails/app.toml`**, which already exists. Do not add `.wasp`. Grow `[[generate]]` with `sql = "src/main/resources/sql/list_notes.sql"` rows.

JHipster JDL is a third language. jails already rejected a conditional template language. JDL entities ≈ field spec + associations; import could be `jails app apply` from a converted TOML, not a JDL runtime.

### 4.2 SQL-first workflow (the new authoring path)

File `src/main/resources/sql/list_notes_by_title.sql`:

```sql
-- name: ListNotesByTitle :many
-- port: NoteRepository          -- optional: extend existing port instead of new
SELECT id, title, body, created_at
FROM notes
WHERE title = sqlc.arg(title)
ORDER BY created_at DESC
LIMIT sqlc.arg(max_results);
```

`jails g sql src/main/resources/sql/list_notes_by_title.sql` (or `jails sql generate`) emits Section 7's query port + JDBC + fake + test.

`:one` / `:many` / `:exec` / `:execrows` copy sqlc commands (`internal/metadata`). `:batchexec` can wait.

Named parameters: accept both `sqlc.arg(title)` and `:title` (JdbcClient house style). Normalize to named `JdbcClient` `.param("title", …)`.

### 4.3 Schema-first pull

```
jails pull --table notes --pretend
```

Reads PostgREST-equivalent catalog (tables, columns, nullability, PK, unique, FK). Maps Postgres types through the **inverse** of `sql.rs` `column_type`. Unmapped types (`ltree`, `jsonb` until chosen) → refuse that column with a name, same as unmapped Java types today.

FK rows become `g association` mappings `childField=parentField`.

### 4.4 Code-first remaining the default

Field spec → Java + SQL stays the happy path for green-field. SQL-first is for the queries `g query` equality filters cannot express (joins, `tsquery`, window functions). `g query` stays; it is the 80% path. `jails sql` is the 20% that currently becomes a handwritten `JdbcClient` with no tests.

### 4.5 Seeds and factories

`g factory Note` exists. Add:

- `jails g seed demo` — `DemoSeed` calling ports, plus `requests/` or fixtures.
- Fixture directory already seeded by `new`. Two-row fixtures stay the rule (ordering bugs).

Laravel + Filament admin UIs are out of scope (not a Java admin generator).

### 4.6 Contract tests

Huma (`deps/huma/README.md`): types → OpenAPI 3.1 + RFC 9457. jails `add api` already does problem+json. Next: `jails openapi` emitting OpenAPI from `jails routes` + DTO records (source scan). Client `g client` already exists (`@HttpExchange`). Pact-style contracts can wait; OpenAPI is the portable artifact.

tRPC's end-to-end types need a shared TS client — out of scope unless a later `openapi-generator` splice (already in `deps/openapi-generator`).

### 4.7 TUI modeler

A ratatui wizard for "three entities + two associations" that writes an `app.toml` fragment and `--pretend`s `app apply`. This is concept (6) aimed at authoring rather than diffs. Same binary, no GUI.

---

## Section 5: Cross-Ecosystem Pattern Translation Matrix

| Source | Core DX innovation | How jails adapts it | Crate | Latency | Correctness | Authoring |
|---|---|---|---|---|---|---|
| Rails `g scaffold` + destroy | Whole resource + inverse | Already: `g scaffold` / ledger `destroy` | generate, commit | — | high | high |
| Rails attribute DSL (`uniq`, `digest`) | Dense field tokens | Extend `FieldSpec`; no `has_secure_password` magic | protocol, generate | — | med | high |
| Rails schema dumper | DB → Ruby schema | `jails pull` → field specs + SQL, not AR | drive, generate | med | high | high |
| Hanami slices | Explicit bounded contexts | Already: layers + `--package` | spec, project | — | high | med |
| Laravel Artisan + factories | One CLI for make/seed | `g factory` exists; `g seed` new | generate | — | med | high |
| Laravel Tinker | REPL with app boot | `jails console` (jshell + Maven CP) exists | drive | med | med | med |
| Django `makemigrations` | Autodetect model vs graph | Diff ledger field specs vs parsed Flyway | generate, java | med | high | high |
| Alembic `compare_metadata` | Inspector vs metadata | Live `information_schema` vs desired catalog | drive | low | high | med |
| FastAPI type hints (via Huma) | Types drive HTTP + OpenAPI | Field spec + records already; add `openapi` emit | generate, report | — | high | high |
| Phoenix `phx.gen.context` | Context API boundary | Already ports; optional package prefix | generate | — | high | high |
| Ecto changesets | Cast/validate ≠ persist | Already: request DTO `@Valid` vs domain record compact ctor | generate | — | high | med |
| Ecto SQL sandbox | Concurrent DB tests | Generated `TestTx` + compose; no runtime lib | generate, drive | high | high | med |
| Adonis Ace | Typed generators | clap `ValueEnum` already | spec | — | high | med |
| Wasp `.wasp` | Declarative app graph | `.jails/app.toml` — extend, don't replace | engine, protocol | — | high | high |
| Prisma schema + contract-first Next | Schema as hashed contract | Ledger entity + sql-cache hash | protocol, commit | — | high | med |
| Prisma migrate | Desired schema → SQL | Atlas-like diff of catalogs | generate | med | high | high |
| sqlc | SQL → typed code | `jails sql` + JdbcClient | java, generate | high | high | high |
| sqlc `vet` / `verify` | Lint + expand/contract | `sql check`; local only | drive, report | med | high | — |
| SQLx `query!` + offline | PREPARE + cache | `--live` + `.jails/sql-cache` | drive, protocol | med | high | — |
| Encore infra-from-code | Static decls → local infra | Capabilities + compose; no cloud | generate, drive | high | med | med |
| Encore tracing | Local traces | Optional `add observability` (exists); not a new runtime | generate | — | med | — |
| Ent schema-as-code | Graph queries | **Reject** as runtime; pull associations only | — | — | — | — |
| goose | SQL or Go migrations | Flyway SQL only; `g migration` | generate, drive | — | high | med |
| Loco name-inferred migrations | `add_columns_to_users` | `g migration add_title_to_notes title:string` | generate | — | med | high |
| Axum extractors | Type-safe request parts | Spring MVC + validated DTOs (exists) | generate | — | med | med |
| utoipa / Huma | Types → OpenAPI | `jails openapi` from routes + DTOs | report, generate | — | high | med |
| templ / Ziex | Compile-time HTML | Out of scope (Java API tool) | — | — | — | — |
| Shuttle annotations | Infra via attributes | **Reject** (runtime/platform) | — | — | — | — |
| Jetzig build-time routes | Routes known before run | `jails routes` source scan (exists) | project | high | med | — |
| Spring Boot autoconfig | Classpath magic | **Reject** as a generator strategy; starters stay explicit `add` | generate | — | high | — |
| Spring Data REST | Repo → HTTP | **Reject** | — | — | — | — |
| Quarkus DevServices | Missing URL → container | Keep compose + explicit Testcontainers beans | drive | high | high | med |
| Quarkus build-time DI | No runtime scan | Stay on Spring; don't switch | — | — | — | — |
| JHipster JDL | Entity language | Import to app.toml only | engine | — | med | high |
| Micronaut AOT DI | Compile-time injection | Same as Quarkus — don't switch | — | — | — | — |
| Ktor routing DSL | Nested routes | `jails routes` already lists; no Ktor emit | — | — | — | — |
| ArchUnit | Architecture as tests | `add arch` generated tests | generate | — | high | — |
| PostgREST schema cache | Catalog → API | Catalog → **generate**, not serve | drive, generate | med | high | high |
| Atlas declarative | Desired vs actual DB | `schema diff` | drive, generate | med | high | high |
| Hasura / Directus | Instant API on catalog | **Reject** as runtime; same pull as PostgREST | — | — | — | — |
| PocketBase | Single binary BaaS | **Reject** (different product); CLI density is the lesson | — | — | — | — |
| Kamal | Bare-metal deploy | `add docker` / `add k8s` exist | generate | — | med | med |
| HTMX / LiveView / Livewire / Turbo | Server-driven UI | Out of scope for this backend generator | — | — | — | — |
| Next.js / tRPC | Full-stack types | OpenAPI + existing `g client` | generate | — | med | med |
| Air | Go live reload | DevTools (exists) | — | high | — | — |
| Testcontainers-Go | Ephemeral infra in tests | Already Testcontainers-Java beans | generate | med | high | — |
| Goravel | Laravel-in-Go | Artisan density; ignore facades | — | — | — | med |

---

## Section 6: Concrete CLI Command Specifications

### 6.1 `jails testd --watch`

```
jails testd --watch [--affected] [filter]
```

**Behavior:** start daemon if needed (existing). Watch `target/classes` and `target/test-classes`. Debounce ~50 ms. On fire: staleness gate, then `--affected` unless a filter was given.

**Sample:**

```
testd: watching /path/target/{classes,test-classes}
testd: Note.class → com.example.demo.web.NoteControllerTest
..
testd: 2 tests, 0.08 s
```

**Refuse:** source newer than class (`testd not taken: … fix: jails test`).

### 6.2 `jails sql` / `jails generate sql`

```
jails g sql <file.sql|--all> [--package <sub>] [--pretend] [--diff]
jails sql check [--live] [--offline]
jails sql cache
```

**`--all`:** every `src/main/resources/sql/**/*.sql` plus, in check mode, SQL literals extracted from types the ledger marked as generated JDBC.

**Check sample (catalog mode):**

```
sql check: 12 queries, catalog from 4 migrations
FAIL  src/main/resources/sql/list_notes_by_title.sql
      column notes.titl does not exist
      hint: notes.title (did you mean?)
fix:  edit the query, or jails g field Note …
```

**Live sample:**

```
sql check --live: PREPARE on postgres://localhost:5432/demo_scratch
OK    12 queries
wrote .jails/sql-cache/v1/<hash>.json
```

**`--pretend` generate sample:**

```
would create  src/main/java/…/ListNotesByTitleQuery.java
would create  src/main/java/…/JdbcListNotesByTitleQuery.java
would create  src/main/java/…/InMemoryListNotesByTitleQuery.java
would create  src/test/java/…/ListNotesByTitleQueryTest.java
```

`--diff` shows the Java body (Section 7).

### 6.3 `jails pull`

```
jails pull [--schema public] [--table <name>]... [--pretend] [--diff]
```

Uses `compose.yaml` / `spring.datasource` the same way `jails db` does. No table argument → list tables and refuse to dump the world without `--all`.

```
pull: notes (id uuid pk, title text not null, body text)
would run: jails g scaffold Note id:uuid@pk title:string! body:string?
```

### 6.4 `jails schema diff` / `jails g migration --from-model`

```
jails schema diff [--live] [--pretend]
jails g migration --from-model [--name add_notes_title]
```

```
schema diff (desired = ledger scaffolds, actual = db/migration)
  notes: add column title text not null
would create db/migration/V004__add_notes_title.sql
```

`--live` diffs against `information_schema` instead of files (drift vs production).

### 6.5 `jails generate scaffold` extensions

```
jails g scaffold Order \
  id:uuid@pk userId:uuid@index total:long@nonnegative \
  status:enum{PENDING,PAID,CANCELLED} \
  --with-events --pretend
```

Still one journaled transition. `--with-events` appends `g event OrderPlaced` artifacts.

### 6.6 `jails g seed` and `jails testd --db`

```
jails g seed Demo
jails testd --db --affected   # exports JDBC URL to compose; ITs opt in via generated TestTx
```

`--db` **refuses** if compose is down: `fix: jails start`. Never starts containers from testd (keeps testd < 100 ms for unit tests).

### 6.7 `jails add arch`

```
jails add arch
```

Splices ArchUnit test dependency (versioned like AssertJ), writes `ArchitectureTest.java`, `ensure` on verify via Surefire (not Failsafe). `doctor` drift if the test file is deleted.

### 6.8 `jails openapi`

```
jails openapi [--out target/openapi.json]
```

Source-derived, like `routes`. No running app.

### 6.9 Interactive apply

```
jails g scaffold Note title:string! --interactive
```

Keys: `a` apply all, `s` skip file, `q` abort (no journal). `--pretend` is still the non-interactive dry run.

### 6.10 `why` / `doctor` additions (no new verbs)

`jails why` gains `sql-prepare` and `devtools-silent` groups. `jails doctor` gains SQL cache + ArchUnit drift rows. Every FAIL still prints `fix:`.

---

## Section 7: Generated Java Code Blueprints

These match current house style: no Lombok, JSpecify where the project has it, `JdbcClient` named parameters, `Objects.requireNonNull`, compact-constructor validation, exactly one persistence bean, RFC 9457 when `add api` is present. They are **targets** for new generators; several are already close to `record_java` / `jdbc_repository` / `in_memory_repository_java.java` / `resource_controller_java.java`.

### 7.1 Domain record

```java
package com.example.demo.domain;

import java.util.Objects;
import java.util.UUID;

/**
 * An immutable Order value.
 *
 * <p>The compact constructor rejects what the field spec said to reject, so
 * any instance that exists is a valid one and callers downstream do not
 * have to re-check.
 */
public record Order(UUID id, UUID userId, long total, OrderStatus status) {

    public Order {
        Objects.requireNonNull(id, "id");
        Objects.requireNonNull(userId, "userId");
        Objects.requireNonNull(status, "status");
        if (total < 0) {
            throw new IllegalArgumentException("total must be nonnegative");
        }
    }
}
```

Inline `status:enum{PENDING,PAID,CANCELLED}` also emits `OrderStatus` (existing `g enum` shape).

### 7.2 Type-safe JDBC repository (`JdbcClient`, zero reflection)

Already the shape in `repository.rs` `Jdbc{name}Repository`. Kept here as the canonical blueprint SQL-first adapters must match:

```java
package com.example.demo.adapters;

import com.example.demo.domain.Order;
import com.example.demo.domain.OrderStatus;
import java.sql.ResultSet;
import java.sql.SQLException;
import java.util.List;
import java.util.Objects;
import java.util.Optional;
import java.util.UUID;
import org.springframework.jdbc.core.simple.JdbcClient;
import org.springframework.stereotype.Repository;

/**
 * {@link OrderRepository} over {@link JdbcClient}. No ORM: the queries are
 * visible, and the only abstraction is a named parameter.
 *
 * <p>Parameters are named rather than positional on purpose. A {@code ?} list
 * is a silent-swap bug waiting for a schema change.
 */
@Repository
public final class JdbcOrderRepository implements OrderRepository {

    private static final String COLUMNS =
            """
            id, user_id, total, status
            """;

    private final JdbcClient db;

    public JdbcOrderRepository(JdbcClient db) {
        this.db = Objects.requireNonNull(db, "db is required");
    }

    @Override
    public Optional<Order> findById(UUID id) {
        Objects.requireNonNull(id, "id");
        return db.sql("""
                        select %s
                        from orders
                        where id = :id
                        """.formatted(COLUMNS))
                .param("id", id)
                .query(JdbcOrderRepository::map)
                .optional();
    }

    @Override
    public List<Order> findAll() {
        return db.sql("""
                        select %s
                        from orders
                        order by id
                        """.formatted(COLUMNS))
                .query(JdbcOrderRepository::map)
                .list();
    }

    @Override
    public void save(Order order) {
        Objects.requireNonNull(order, "order is required");
        db.sql("""
                        insert into orders (id, user_id, total, status)
                        values (:id, :user_id, :total, :status)
                        """)
                .param("id", order.id())
                .param("user_id", order.userId())
                .param("total", order.total())
                .param("status", order.status().name())
                .update();
    }

    @Override
    public boolean deleteById(UUID id) {
        Objects.requireNonNull(id, "id");
        return db.sql("delete from orders where id = :id")
                        .param("id", id)
                        .update()
                > 0;
    }

    private static Order map(ResultSet rows, int rowNumber) throws SQLException {
        return new Order(
                rows.getObject("id", UUID.class),
                rows.getObject("user_id", UUID.class),
                rows.getLong("total"),
                OrderStatus.valueOf(rows.getString("status")));
    }
}
```

### 7.3 SQL-first query (new)

From `-- name: ListOrdersByStatus :many` + `WHERE status = sqlc.arg(status)`:

```java
package com.example.demo.app;

import com.example.demo.domain.Order;
import com.example.demo.domain.OrderStatus;
import java.util.List;

public interface ListOrdersByStatusQuery {

    List<Order> execute(OrderStatus status, int maxResults);
}
```

```java
package com.example.demo.adapters;

import com.example.demo.app.ListOrdersByStatusQuery;
import com.example.demo.domain.Order;
import com.example.demo.domain.OrderStatus;
import java.sql.ResultSet;
import java.sql.SQLException;
import java.util.List;
import java.util.Objects;
import java.util.UUID;
import org.springframework.jdbc.core.simple.JdbcClient;
import org.springframework.stereotype.Component;

/** Visible SQL from src/main/resources/sql/list_orders_by_status.sql. */
@Component
public final class JdbcListOrdersByStatusQuery implements ListOrdersByStatusQuery {

    private static final int MAX_RESULTS = 100;

    private final JdbcClient db;

    public JdbcListOrdersByStatusQuery(JdbcClient db) {
        this.db = Objects.requireNonNull(db, "db is required");
    }

    @Override
    public List<Order> execute(OrderStatus status, int maxResults) {
        Objects.requireNonNull(status, "status");
        int limit = Math.min(Math.max(maxResults, 1), MAX_RESULTS);
        return db.sql("""
                        select id, user_id, total, status
                        from orders
                        where status = :status
                        order by id
                        limit :max_results
                        """)
                .param("status", status.name())
                .param("max_results", limit)
                .query(JdbcListOrdersByStatusQuery::map)
                .list();
    }

    private static Order map(ResultSet rows, int rowNumber) throws SQLException {
        return new Order(
                rows.getObject("id", UUID.class),
                rows.getObject("user_id", UUID.class),
                rows.getLong("total"),
                OrderStatus.valueOf(rows.getString("status")));
    }
}
```

### 7.4 In-memory fake

Matches `templates/spring/in_memory_repository_java.java`: `ConcurrentHashMap`, no `@Repository` when JDBC is the bean.

```java
package com.example.demo.adapters;

import com.example.demo.app.ListOrdersByStatusQuery;
import com.example.demo.domain.Order;
import com.example.demo.domain.OrderStatus;
import java.util.List;
import java.util.Objects;

public final class InMemoryListOrdersByStatusQuery implements ListOrdersByStatusQuery {

    private final OrderRepository orders;

    public InMemoryListOrdersByStatusQuery(OrderRepository orders) {
        this.orders = Objects.requireNonNull(orders, "orders");
    }

    @Override
    public List<Order> execute(OrderStatus status, int maxResults) {
        Objects.requireNonNull(status, "status");
        return orders.findAll().stream()
                .filter(order -> order.status() == status)
                .limit(Math.max(maxResults, 0))
                .toList();
    }
}
```

### 7.5 REST controller (existing shape)

See `templates/spring/resource_controller_java.java`: `@Valid` body, 201+Location, 404 vs empty 200, 204 vs 404 on delete. Scoped resources stay create-only on the collection controller (`g query` for tenant reads). No change required for Pillar 3 except optional OpenAPI annotations **only if** they stay in Javadoc / SpringDoc and `add` splices `springdoc` — do not force a new runtime model.

### 7.6 Flyway migration

```sql
-- db/migration/V004__create_orders.sql
create table orders (
    id uuid not null primary key,
    user_id uuid not null,
    total bigint not null,
    status text not null,
    constraint orders_total_nonnegative check (total >= 0)
);

create index orders_user_id_idx on orders (user_id);
```

`schema diff` would emit `alter table … add column` files with the next numeric version (`migrate.rs`: numeric order, not lexical — `V10` before `V9` as strings is the bug).

### 7.7 Slice integration test with Testcontainers

Keep Boot's rule: container as `@Bean` `@ServiceConnection`, `@Import(TestcontainersConfig.class)` on the `@SpringBootTest`. Failsafe `*IT`. AssertJ.

```java
package com.example.demo.adapters;

import static org.assertj.core.api.Assertions.assertThat;

import com.example.demo.domain.Order;
import com.example.demo.domain.OrderStatus;
import com.example.demo.test.TestcontainersConfig;
import java.util.UUID;
import org.junit.jupiter.api.Test;
import org.springframework.beans.factory.annotation.Autowired;
import org.springframework.boot.test.context.SpringBootTest;
import org.springframework.context.annotation.Import;

@SpringBootTest
@Import(TestcontainersConfig.class)
class JdbcOrderRepositoryIT {

    @Autowired
    private OrderRepository orders;

    @Test
    void roundTrip() {
        var order = new Order(UUID.randomUUID(), UUID.randomUUID(), 199L, OrderStatus.PENDING);
        orders.save(order);
        assertThat(orders.findById(order.id())).contains(order);
    }
}
```

### 7.8 ArchUnit rule (new)

```java
package com.example.demo;

import static com.tngtech.archunit.lang.syntax.ArchRuleDefinition.noClasses;

import com.tngtech.archunit.core.importer.ImportOption;
import com.tngtech.archunit.junit.AnalyzeClasses;
import com.tngtech.archunit.junit.ArchTest;
import com.tngtech.archunit.lang.ArchRule;

@AnalyzeClasses(packages = "com.example.demo", importOptions = ImportOption.DoNotIncludeTests.class)
class ArchitectureTest {

    @ArchTest
    static final ArchRule domain_does_not_depend_on_jdbc =
            noClasses()
                    .that()
                    .resideInAPackage("..domain..")
                    .should()
                    .dependOnClassesThat()
                    .resideInAPackage("org.springframework.jdbc..");

    @ArchTest
    static final ArchRule web_does_not_depend_on_jdbc_client =
            noClasses()
                    .that()
                    .resideInAPackage("..web..")
                    .should()
                    .dependOnClassesThat()
                    .haveSimpleName("JdbcClient");
}
```

Package names must come from `Config::layers()` at generation time so a renamed `adapters = "persistence"` still matches (the inspect.rs drift lesson).

---

## Section 8: Implementation Roadmap & Crate-by-Crate Architecture Plan

Priority is **proof on this machine**, then API surface, then TUI sugar. Each step must leave `cargo test --workspace` green and add a golden scenario (`tests/common/scenarios.rs`) for any new kind/capability.

### Phase 0 — Do not start until these are decided

1. **SQL parser strategy:** (A) hermetic `sqlc` binary as a `CommandSpec` (like `psql`), (B) Rust `pg_query` / equivalent, (C) catalog-only using live `PREPARE` and no offline parser. Recommendation: **A for query analysis** (sqlc already understands `-- name:` and migrations), **Rust for extracting `db.sql("""`) strings** via `java::blanked()`. sqlc stays off the generated classpath.
2. **Proof app:** one `examples/` manifest that uses a join query `g query` cannot express, so `jails sql` is forced to be real (`pending.md` §2.4 cost decision still open — do not add a fourth proof app lightly).
3. **Continue/abort conflicts** (`pending.md` §11) should not be half-built underneath the TUI.

### Phase 1 — Pillar 1 (days, mostly `jails-drive`)

| Step | Work | Crate |
|---|---|---|
| 1.1 | `testd --watch` + debounce + `--affected` default in watch mode | `jails-drive` |
| 1.2 | Incremental `classfile` absorb of changed `.class` only | `jails-java`, `jails-drive` |
| 1.3 | Document watch in README save-and-reload section; doctor if inotify hits `src/` instead of `target/` | README, `jails-report` |

Success: save in Neovim → jdt.ls → testd watch prints the controller test in < 150 ms on a warm daemon, without typing a command.

### Phase 2 — SQL catalog (the 1000x authoring + correctness)

| Step | Work | Crate |
|---|---|---|
| 2.1 | `ArtifactKind::Sql` + `explain` row + golden scenario | `jails-spec`, `jails-report`, `tests/` |
| 2.2 | Catalog from `db/migration` (sqlc subprocess or shared parser) | `jails-java` or `jails-generate/src/sql/` |
| 2.3 | Emit port + JdbcClient + in-memory + test from one `.sql` file | `jails-generate` |
| 2.4 | `sql check` catalog mode on generated + hand SQL | `jails-drive` |
| 2.5 | `--live` PREPARE on migrate-style scratch DB; cache in `.jails/sql-cache` | `jails-drive`, `jails-protocol`, `jails-commit` (store files) |
| 2.6 | `why` group `sql-prepare`; `doctor` stale cache | `jails-report` |
| 2.7 | `sqlc verify` analogue: check **current** queries against **proposed** migration file under `--pretend` | `jails-drive` |

Ledger: the `.sql` file is the entity; Java adapters are derived outputs (same as scaffold's JDBC adapter vs record).

### Phase 3 — Schema diff and pull

| Step | Work | Crate |
|---|---|---|
| 3.1 | Inverse of `sql.rs` type table (Postgres → field spec); refuse unknown | `jails-generate` `sql.rs` |
| 3.2 | `schema diff` desired (ledger) vs actual (files) | `jails-generate`, `jails-engine` |
| 3.3 | `--live` via `psql` / information_schema (PostgREST queries as a template) | `jails-drive` |
| 3.4 | `jails pull --table` → scaffold recipes `--pretend` | `jails-engine` |
| 3.5 | Loco-style `g migration add_x_to_y a:string` | `jails-generate` `migration.rs` |

### Phase 4 — Authoring density

| Step | Work | Crate |
|---|---|---|
| 4.1 | Inline `enum{A,B}` in `FieldSpec` | `jails-protocol` |
| 4.2 | `--with-events` / `--with-audit` as extra recipes in one generate transition | `jails-engine`, `jails-generate` |
| 4.3 | `g seed`; factory already exists | `jails-generate` |
| 4.4 | `add arch` + layer names from `Config::layers()` | `jails-generate`, `jails-spec` |
| 4.5 | `jails openapi` | `jails-report` / `jails-project` inspect |
| 4.6 | `app.toml` `sql =` rows | `src/app.rs`, protocol |

### Phase 5 — Trust UX

| Step | Work | Crate |
|---|---|---|
| 5.1 | `--diff` unified diffs on generate/add | `jails-prepare` report |
| 5.2 | `--interactive` TTY pager | binary / engine |
| 5.3 | Conflict continue/abort **complete protocol** or nothing | `jails-commit`, `pending.md` §11 |

### Phase 6 — Test DB isolation (optional, after proof)

Generated `TestTx`, `testd --db`, compose URL only. No Ecto sandbox ported to a jar.

### Crate ownership summary

```
jails-protocol   ArtifactKind::Sql, sql-cache envelope, FieldSpec enum{}
jails-java       blanked() SQL-literal extraction; optional catalog types; classfile watch helper
jails-spec       Capability::Arch; kind clap values
jails-generate   sql file → Java; schema diff SQL; seed; arch test; inline enum
jails-prepare    diffs for new outputs; pretend report
jails-commit     sql-cache as store artifacts if they must survive crash; else put_outside? No: cache is project-local derived — prefer target/ or .jails/ via existing apply verbs
jails-drive      testd --watch, sql check --live, pull, testd --db env
jails-report     why groups, doctor rows, openapi, explain
jails-engine     pull/sql/scaffold --with-* as one transition; interactive
jails (binary)   TUI, clap flags
```

`jails-commit` should not grow a SQL engine. `jails-support` stays write/run/encode.

### What not to sequence in

- Kotlin Gradle DSL (README Not yet)
- Runtime bean dump
- Plugin system
- ORM
- `jails dev`
- Frontend / HTMX / LiveView
- Encore Cloud / sqlc Cloud
- Reversible Flyway

### Success criteria (research, not a gate until implemented)

1. A join query lives in a `.sql` file; `jails g sql` produces Java that `mvn test` passes; renaming a column in `V00N` makes `jails sql check` fail before `testd` is green on stale SQL.
2. `testd --watch` is the default local test loop; `jails check` remains the merge gate.
3. `jails pull --table` on the compose DB of a proof app round-trips types `sql.rs` knows, and refuses the rest by name.
4. `add arch` fails a deliberately wrong import in a unit test without starting Spring.
5. No generated `import com.jails.…`. No new process named `jails dev`.

---

## Appendix A — Highest-signal files read

| Path | Why |
|---|---|
| `crates/jails-drive/src/testd.rs` | Daemon contract: no compile, split classpath, 464 ms vs 20 ms |
| `templates/testd/JailsTestDaemon.java` | Child loader freshness |
| `crates/jails-java/src/classfile.rs` | Constant-pool edges; Long/Double slots |
| `crates/jails-drive/src/affected.rs` | Unknown widens |
| `crates/jails-generate/src/sql.rs` | One column list for DDL/bind/map |
| `crates/jails-generate/src/generate/repository.rs` | Named `JdbcClient` adapter |
| `crates/jails-drive/src/migrate.rs` | Scratch DB, numeric versions, not a doctor check |
| `crates/jails-report/src/why.rs` | Evidence-gated rules |
| `crates/jails-prepare/src/pipeline/diff.rs` | Three-way merge vs clobber |
| `deps/sqlc/internal/compiler/compile.go` | parseCatalog + migration strip |
| `deps/sqlc/docs/howto/{generate,vet,verify,named_parameters}.md` | Product semantics |
| `deps/sqlx/README.md` | PREPARE at compile time; offline cache |
| `deps/quarkus/.../DevServicesDatasourceProcessor.java` | Missing URL → container (do not copy into Java) |
| `deps/postgrest/.../SchemaCache.hs` | pg_catalog as API source |
| `deps/django/.../autodetector.py` | Model state vs migrations |
| `deps/alembic/alembic/autogenerate/api.py` | Inspector vs metadata |
| `deps/atlas/README.md` | Declarative vs versioned SQL |
| `deps/loco/loco-gen/src/migration.rs` | Infer migration from name |
| `deps/phoenix/lib/mix/tasks/phx.gen.context.ex` | Context boundary + injection prompt |
| `deps/rails/.../scaffold/USAGE` | Attribute DSL + destroy inverse |
| `deps/prisma/ARCHITECTURE.md` | Contract-first / hashed schema |
| `deps/encore/README.md` | Infra decls; local real Postgres |
| `deps/huma/README.md` | Types → OpenAPI + RFC 9457 |

---

## Appendix B — Mapping to the three pillars (checklist)

| Concept | P1 latency | P2 correctness | P3 authoring |
|---|---|---|---|
| testd --watch | primary | staleness honesty | — |
| sqlc-style files | check in ms | typed SQL | primary |
| sql check --live | slower, optional | primary | — |
| schema diff / pull | — | drift visible | primary |
| pretend TUI | — | primary | secondary |
| why / doctor growth | — | primary | — |
| ArchUnit | Surefire ms | primary | — |
| inline enum / --with-* | — | — | primary |
| seeds / testd --db | IT path | isolation | primary |

The 1000x number is not a benchmark. It is a reminder that **waiting, guessing, and retyping JDBC** are the three taxes `jails` was built to remove — and that two of them are already mostly paid. The unpaid tax is **SQL the reader wrote**, and the unpaid convenience is **not having to type `testd --affected` after every save**.
