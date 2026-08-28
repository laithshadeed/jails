---
name: jails
description: "Scaffolding, evolving and operating Spring Boot and plain Java projects with the jails CLI. Triggers on: jails, jails new, jails generate, jails g, jails add, jails resource, jails app apply, jails doctor, jails test, jails sql, spring boot scaffolding, ports and adapters, raw jdbc java, testcontainers spring, flyway migrations with jails, hex architecture java."
---

# jails

A Rails-inspired CLI for Java and Spring Boot (JDK 26 default, 21 floor;
Maven and Groovy Gradle). Ports and adapters, pure records, raw `JdbcClient`,
**no ORM**. Mutations are prepared in memory and previewable byte-for-byte.
Canonical projects execute content-addressed exact plans; legacy projects use
the older write-ahead journal while the cutover is in progress.

State of this file: verified against `jails 0.1.0` at HEAD `d05a8af`
(2026-08-27) by running the binary. All numbered reports in `bugs.md` are closed;
active implementation phases are tracked in `plan.md`.

## The three rules that explain everything else

1. **Preview and apply are the same computation.** `--pretend` (alias
   `--dry-run`) stops before the commit; it does not run a second planner. Its
   operation list is byte-identical to the real run's. Add `--diff` for the
   bytes, `--ast` for the semantic edits.
2. **Desired state is the identity, files are projections.** In a canonical
   project, `.jails/model.jdl` and inline `@id(...)` stable IDs are
   authoritative. `.jails/model.toml` is a temporary compatibility input. In a
   legacy project, `.jails/ledger.toml` records declarations. Neither
   architecture uses generated paths as semantic identity.
3. **Migrations are append-only and sealed at first publication.** Nothing
   replaces or deletes a published `VNNN__*.sql`. Every schema change is a new
   forward migration. There are no down migrations and there never will be.

Canonical generated Java and marked reader-document facets are merge-managed:
the accepted compiler projection is BASE, live bytes are OURS, and the next
projection is THEIRS. Clean disjoint hand edits survive; overlap refuses before
any write. A reader facet records only the generated slice, not the surrounding
document. The canonical executor rechecks all captured preconditions before
publishing exact after-images. Legacy torn transactions are still recovered by
journal roll-forward.

## Canonical project mode

The presence of `.jails/model.jdl` or temporary `.jails/model.toml` opts a
project into the application compiler; the two sources are mutually exclusive.
Familiar supported mutations become typed `ModelPatch` inputs and must never
silently fall back to the legacy ledger/prepare/commit stack. JDL currently
covers record/scaffold/field/enum/factory/dto/repo/strategy/controller generation, standalone
class/interface/service/sealed/test/integration-test units, nested operation
generation, typed strategy and HTTP controller units, `fake`/`db`/`api` capability declarations, and preserve-table
entity rename.
`csv`, `json`, `http`, `testkit`, `sqlite`, `h2`, `actuator`, `cache`, `cors`, `observability`, `security`, `sse`, `redis`, `kafka`, `mail`, `toxiproxy`, `coverage`, and `loadtest` are compiler-owned declarative
packs too. Their Java files and Testkit fixture resource use the same three-way
merge, and one capability ID ejects the complete implementation boundary.
SQLite's initial SQL uses append-only migration history, so removing or
ejecting its Java boundary never deletes that migration.
H2's pack also reconciles Boot-version-sensitive dependencies and separate
main/test datasource properties while preserving unrelated reader keys.
Actuator's pack owns one merge-managed endpoint contract test, its Spring
starter, and narrow key-scoped management properties. Ejecting `cap_actuator`
moves only that test into reader source; dependency and property ownership stay
with the still-declared capability.
Cache's configuration and proof test are independently merge-managed files in
one ejectable Java boundary. Its Spring starter, Caffeine dependency, and
bounded cache properties remain managed after Java ejection.
CORS owns two merge-managed Java files in one ejectable boundary plus its
exact-origin property. Captured Boot `<4` selects classic `MockMvc`; Boot 4+
selects `MockMvcTester`, the moved annotation import, and the web MVC test
starter. Ejection transfers Java only, leaving the declared property managed.
Observability owns four merge-managed Java files under one ejection boundary,
the Actuator and Prometheus dependencies, and bounded metrics/tracing/access-log
properties. Captured Boot 4+ selects the moved `MeterRegistryCustomizer`
import. Java ejection keeps the dependency and property contract managed.
Security owns five merge-managed Java files under one ejection boundary and
enforces a Boot 3 floor for its main source. Boot 4 selects the moved
`WebMvcTest` import and starter. Security dependencies remain managed after
Java ejection, and shared reconciliation must retain CORS's test starter.
SSE owns four merge-managed Java files spanning the application root and web
package. `cap_sse` ejects the complete live implementation while its Web
dependency and scheduler-pool property remain compiler-managed.
Redis owns two merge-managed Java files, its dependencies/properties/Failsafe
feature, and one marked Compose service. Only the exact service block is stored
as merge BASE, so hand edits inside it and unrelated YAML survive later
generation; overlapping block edits refuse atomically.
Mail owns a merge-managed sender and integration proof, Boot-sensitive test
dependencies, Failsafe, three properties, and a marked Mailpit reader facet.
The same generate/edit/generate guarantees apply to both Java and the service
block; the rest of Compose stays reader-owned.
Toxiproxy owns two merge-managed testkit files under one implementation
boundary and exact test-scoped dependencies. Both engines render the same
shared templates; edits to either file survive later generation, while removal
does not touch an independent testkit or fake boundary.
Coverage is a pure typed build feature: its Maven/Gradle JaCoCo block is
lossless, independently removable, and refuses an edit inside the owned block
before any model or workspace write.
Loadtest owns six merge-managed project files outside `.jails/generated`.
Typed routes regenerate `api.js`; disjoint edits in any load-test file survive,
overlaps refuse atomically, and removing an edited file refuses instead of
deleting reader work.
Dependencies, settings, indexes, artifact ejections, import, destroy,
retire/revive, field evolution, and bounded strings now edit JDL directly.
Unsupported generator and capability backends still refuse before legacy
dispatch.

Canonical mode is explicit until the compiler covers every advertised
new-project follow-up. Ordinary Spring, offline Spring, Gradle, plain CLI, and
`new --app` creation stay on the compatibility engine; an explicit model or
`model import` opts a project into the compiler.

`model eject <implementation-boundary-id>` transfers one adapter implementation boundary to
reader ownership. Records and ports remain managed ABI. Ejection is implementation-boundary
scoped, not entity scoped; editing a record does not require ejection because
ordinary generated files participate in the three-way merge.

Canonical `integration-test` is also a semantic build feature. Maven receives
Failsafe with both `integration-test` and `verify`; Gradle receives separate
unit/integration tasks with `check` depending on the latter. The marked block
is lossless, refuses edits, and disappears when the last integration-test
unit is destroyed.

`jails sync` is canonical exact reconciliation. It compiles the current model
and executes that plan through `jails-workspace`; it must not create or consult
the legacy object, receipt, or journal state.

`jails test --fast` records a canonical `fast-test` capability and exact JUnit
console dependency. `jails remove fast-test` removes both; neither command
enters the legacy mutation engine.

`jails model import` currently accepts legacy ledgers containing record and enum
declarations. It three-way merges each recorded legacy render, live Java, and
canonical render while moving the result into the managed tree; an enum and its
Spring converter are separate artifacts in that transition. Unsupported
declarations, merge conflicts, and stale reviewed plans refuse before cutover;
the legacy ledger remains unchanged as evidence, and the imported source is
written directly as `.jails/model.jdl`.

Reader-owned SQL backfills are captured files and exact plan input. A change
between plan and apply makes the plan stale and produces no writes.

## Command map

Everything below exists and runs today.

### Create

| | |
|---|---|
| `jails new <name> [--deps web,jdbc] [--java 26]` | Spring Boot via start.spring.io |
| `jails new <name> --offline` | same, from the vendored fixture, no network |
| `jails new <name> --gradle [--boot 2.7.18] [--gradle-version 8.5]` | Groovy Gradle, written locally |
| `jails new-cli <name> [--release 26]` | plain Maven CLI with a command dispatcher |
| `jails new <name> --app <manifest.toml>` | create **and** apply a manifest in one transaction (publishes project before post-commit effects) |
| `jails adopt` | write a `[layout]` table matching where an existing project already keeps things |

### Generate — `jails g <kind> <Name> [fields...]`

Plain Java: `record` `class` `interface` `value` `enum` `sealed` `strategy`
`command` `cli` `handler` `factory` `test` `integration-test` `cases`
`migration`.

Spring: `scaffold` `controller` `service` `repo` `dto` `usecase` `query`
`transition` `event` `client` `fetcher` `job` `durable-job` `association`
`http-workflow` `http-sink` `idempotency` `auth` `webhook` `search`
`socket` `presence` `seed`.

`jails explain <kind>` gives the rationale *and the trap* for each one — read it
before generating something unfamiliar; it is a hand-maintained table, not
generated filler.

Common flags:
- `--package <sub.package>` (relative to the base package, `''` for flat)
- `--timestamps` (audit timestamps: `createdAt`, `updatedAt`)
- `--index "col, col desc"` (repeatable composite or ordered index)
- `--on <Resource>` (target resource / dispatcher / entity)
- `--yields <Event|Parent>` (yielded event or parent resource)
- `--path <path>` (`controller`, `usecase`, `query`, `client`: custom route; path variables like `/items/{itemId}` bind `@PathVariable` GET routes)
- `--consumes json|form` (`controller`, `usecase`, `query`, `transition`: `form` binds `@Valid @ModelAttribute` and `@BindParam`; on a `query` it also makes the route a **GET reading the query string**)

Key Spring generators:
- `scaffold <Name> <fields...>` — requires single `@pk`. Immutable record, repo port, `JdbcClient` adapter, in-memory fake, request/response DTOs, controller, `.http` collection, and tests. DDL is written to `db/migration` (or `schema.sql` via marked blocks if no migration dir exists).
- `usecase <Name> [fields...] --on <Resource> [--yields <Event>] [--on-conflict <component>]` — create operation. `--on-conflict <comp>` turns it into atomic get-or-create (`Ensuring<Name>UseCase` via `JdbcClient` `ON CONFLICT DO NOTHING RETURNING`). `--yields` adds outbox with batch drain, backoff jitter, and per-sink delivery tracking (`delivered text[]`).
- `query <Name> [fields...] --on <Resource> [--via <Parent>] [--order-by '<cols>'] [--limit <n>]` — typed read with equality filters. `?` suffix on a filter (e.g. `status:Status?`) renders `(cast(:status as type) is null or col = :status)`. `--via <Parent>` joins parent table. The verb is derived, never chosen: GET when every filter is a `--path` variable, **GET binding `@ModelAttribute` from the query string with `--consumes form`** (so `GET /admin_api/users?status=open&category=Billing` is reachable), POST otherwise.
- `transition <Name> [fields...] --on <Resource>` — atomic CAS update matching `id`, `@scope`, and required `version:long`. Returns sealed `Result` (`Applied`, `StaleVersion`, `NotFound`), maps `If-Match` / `ETag`, and raises `ApiException` when `add api` is present.
- `client <Name> [--method <verb>] [--on <Req>] [--returns <Resp>] [--path <path>]` — typed `@HttpExchange` client with timeouts and base URL. Generates independent `<Name>ClientConfig` per client.
- `event <Name> [fields...] [--on <Entity>]` — Kafka payload record and publisher. `--on <Entity>` keys partitions on `<entity>Id` for per-entity ordering; mints `TimeOrderedUuid` so outbox doesn't drop duplicates.
- `enum <Name> [NAME=wire...]` — enum with `@JsonValue`/`@JsonCreator` and Spring converters. Widening an enum automatically emits `alter table ... drop constraint ... add constraint` forward migrations.
- `strategy <Name> [variants...] [--on <Req>] [--yields <Resp>]` — strategy interface and `@Component` implementations placed in `service`/`adapters` (keeping `domain` dependency-free), plus ordered `<Name>Evaluator`.
- `socket <Name>` (aliases: `websocket`, `ws`) — `TextWebSocketHandler`, `/ws/<name>` registration, concurrency decorator, and test.
- `presence <Name>` — PostgreSQL cluster presence `(scope, member, node)`, heartbeats, sweep, and multi-node test.
- `seed <Resource>` — development data in `db/seeds/<table>.json` loaded through repository port with `@Profile("seed")` `ApplicationRunner`.

### Add capabilities — `jails add <capability>...`

`db` `sqlite` `h2` `kafka` `redis` `csv` `json` `mail` `http` `api` `actuator`
`cache` `security` `cors` `sse` `observability` `format` `coverage` `testkit`
`fake` `toxiproxy` `loadtest` `docker` `k8s` `ci`

Escape hatches and configuration:
- `jails add dependency <group>:<artifact> [--version <v>] [--scope compile|runtime|test]`
- `jails remove dependency <group>:<artifact>`
- `jails set <key>=<value> [--tests]` / `jails unset <key> [--tests]`
- `jails remove fast-test`

`jails.toml`'s `[project] capabilities` is maintained by `add`/`remove`, not by
hand. `jails sync` re-plans every recorded capability and repairs drift.

### Evolve a resource — `jails resource ...`

This is the surface that replaced "destroy and regenerate". Use it.

```
jails resource status <Entity>                     # the lifecycle oracle
jails resource field add    <Entity> <name:type>  [--default-literal V | --backfill-file P]
jails resource field rename <Entity> <old> <new>   --column preserve|single-cutover|rolling
jails resource field type   <Entity> <field>       --to <type> --strategy safe|expand-contract
jails resource field nullability <Entity> <field>  --nullable | --required [--backfill-file P]
jails resource field drop   <Entity> <field>       --confirm-column <exact-column>
jails resource index add    <Entity> '<columns>'   # appends forward create migration
jails resource index remove <Entity> '<columns>' --confirm-index <exact-sql-name>
jails resource repair <Entity> --strategy roll-forward
jails resource revive <Entity> --table <preserved-table>
jails rename resource <Name> <New> --strategy preserve-table|single-cutover|rolling
```

Storage changes write new forward migrations; a preserve-column Java rename
does not. Safe type changes accept only proven widenings, required fields need
typed literal or reader-owned SQL backfill, and drop needs the exact accepted
column. Rolling rename and expand/contract are multi-release campaigns and
currently refuse on the canonical path. The guards ask for exactly the
evidence they need and no more. In canonical projects the compiler re-renders
all affected stable-ID projections and three-way merges live generated files;
legacy projects still re-plan the recorded companions.

Canonical `add api` compiles routed command/query/transition ports into Spring
HTTP controllers. These controller artifacts can be independently ejected;
ejection transfers the captured live file (including hand edits) to
`src/main/java`, while the operation interface remains managed. Canonical
`fake` and `db` use the same artifact-scoped implementation rule. Canonical
`db` commands, queries, and transitions compile to separate `JdbcClient`
adapters. Commands generate omitted UUID keys and refuse unmodeled required
values; queries use required and presence-sensitive optional filters, semantic
ordering, and a default limit of 100; transitions update by primary key, use
non-set inputs as guards, and publish modeled events transactionally. Ejecting
one operation adapter leaves its managed ABI and every other entity facet or
operation implementation in place.

### Destroy

```
jails destroy <kind> <Name> [--force]
jails destroy scaffold <Name> --storage preserve
jails destroy scaffold <Name> --storage drop --confirm-table <exact-table>
jails destroy association <Name>                   # retires FK and appends drop constraint
```

A table-backed entity refuses without an explicit storage policy. `--storage
preserve` removes generated projections but keeps the inactive model node and
table; revive requires that exact preserved table name. `--storage drop` writes
`VNNN__drop_<table>.sql`; regenerating afterwards writes a fresh create
migration, so the cycle is complete and its history is readable. Retired
entities refuse field/index evolution until revived. `resource index add`
records a stable ordered index node as well as writing its one forward create
migration. `resource index remove` requires the exact accepted SQL name,
removes that node, and appends a forward drop migration without rewriting
sealed history.

### Verify and inspect

```
jails doctor [--json]        # environment, drift, sealed migration bytes, and lineage column replay
jails migrate --check        # apply every migration to a scratch database
jails migrate lint           # classify destructive statements (works from declared driver)
jails routes | beans | stats | notes | lint | src <Type>
jails why [logfile]          # root-cause a failure from a table of real signatures
jails history | show <id> | undo <id>
```

`routes` and `beans` read source, never a running context — they work on a
project that will not start, and they say so with `evidence:` and `limitation:`
lines. `routes` lists WebSocket endpoints too, as `WS <path> <Handler>`, read
from a `WebSocketConfigurer`'s `registry.addHandler(...)`; a path assembled at
run time is still outside what it can see.

`doctor` verifies recorded output drift, checks digest-sealed migration bytes,
performs a bounded replay of migration history to verify entity fields match
columns, and warns on `@Disabled` test files. It also **fails** on machine
state it cannot read (`.jails/ledger.toml`), on a Gradle wrapper whose pinned
distribution cannot launch on the JDK on PATH, and on an H2 URL combining
`AUTO_SERVER=TRUE` with `DB_CLOSE_ON_EXIT=FALSE`.

### Test and run

```
jails test [<Name|method>...] [--scope unit|integration|all] [--engine auto|build|warm]
           [--compile auto|ide|build|none] [--watch] [--affected] [--failed]
           [--tag T] [--fail-fast] [--slowest N] [--explain-selection]
jails run  [--launcher auto|classpath|build-tool|jar] [--compile ...]
           [--services existing|start|none] [--profile P] [--watch]
jails check                  # mvn clean verify — the truth
jails build | clean | fmt | mvn ... | gradle ...
jails start | stop           # the only CLI-owned compose lifecycle
```

`--engine warm` and `--affected` are **Maven only**; Gradle delegates to the
build engine and says so. `jails fmt` refuses on Gradle by name (use
`./gradlew spotlessApply`; `add format` has already configured it).

`run --watch` needs `spring-boot-devtools` to restart on recompile, and says so
with the command that adds it to *this* project:
`jails add dependency org.springframework.boot:spring-boot-devtools --scope runtime`.

### SQL, schema and contracts

```
jails sql check [--offline|--live --datasource NAME] [--frozen]
jails sql generate
jails introspect db | jails pull | jails schema diff
jails contract emit | jails contract check --against <file>
```

Evidence is never overstated: results are labelled `parsed`,
`verified-offline`, `verified-live` or `executed`, and an unsupported query
becomes a blocking diagnostic rather than a guess.

### Tools and integration

```
jails request GET /orders/1     # route-aware curl, argv shown, secrets redacted
jails db                        # a client for whichever database this project has
jails db --web                  # H2's own browser console (not Spring's /h2-console)
jails db <file.sqlite>          # SQLite
jails db console                # real PostgreSQL client against a declared datasource
jails console                   # JShell with the Spring context booted
jails runner script.jsh         # noninteractive snippet over the project classpath
jails logs <service>            # bounded read-only compose logs
jails kafka ...                 # topics and messages, using the broker image's own tools
jails editor ...                # versioned read-only protocol for editor adapters
jails commands --json           # the single source jails.nvim builds its menus from
```

### Global flags on every command

`--pretend` `--output human|json|json-v1` `--diff` `--ast` `--debug`
`--plan-out <file>` `--plan-in <file>`

`--plan-out`/`--plan-in` export and re-apply an authenticated prepared
transaction without replanning. The JSON envelope carries `status`,
`exit_code`, `report`, `receipt`, `error` and `timings`, on success **and** on
failure.

## Field DSL

`name:type[!?][@constraint...]` (and `NAME=wire` for `g enum`)

**Case is the rule.** Lowercase is a jails builtin; capitalised is a type the
project owns, passed through verbatim with no import.

Builtins: `string` `int` `long` `double` `decimal` `boolean` `uuid` `date`
`datetime` `instant` `duration` `zone-id` `uri` `path` `currency` `bytes`.

Suffix — optionality:
- (bare) non-null
- `!` non-null **and** non-blank (emits constructor trim/blank check and SQL `check (length(btrim(col)) > 0)`)
- `?` emits `Optional<T>` in records/DTOs; in `query` filters emits `(cast(:x as type) is null or col = :x)`

Constraints, parsed off the *type* so either order works:
`@pk` `@unique` `@index` `@positive` `@nonnegative` `@scope`.

Identity and Key Assignment:
- `g scaffold` **requires** a single `@pk`.
- Key assignment is derived from the key type:
  - `uuid@pk`: server-assigned via RFC 9562 UUIDv7 (`TimeOrderedUuid`).
  - `int@pk` / `long@pk`: database-assigned (`generated always as identity`), retrieved via JDBC `getGeneratedKeys`.
- Repository ports return the saved row on `save(T)`.
- `findById` is strongly typed on the primary key's Java type (`UUID`, `Long`), not `String`.
- Create request DTOs withhold server-assigned state (primary key, audit timestamps, optimistic lock version) from POST bodies.
- `@unique` on email-named columns automatically generates case-insensitive unique index `lower(email)`.
- Enum fields emit SQL `check (col in ('VAL1', 'VAL2'))`.
- Naming convergence: snake_case column is normal form; Java lowerCamelCase is derived. Recorded `@column(...)` bindings preserve custom column names.
- Free-text `String` fields with enum-like names trigger `free-text-closed-set` warnings.

Composite or ordered indexes go on the command, not the field:
`--index "author, created_at desc"`.

## Two ways to drive it

**Imperative** — `jails g ...` and `jails add ...`, recorded in the ledger as you
go.

**Declarative** — `.jails/app.toml`, applied with `jails app plan` / `jails app
apply` as **one transaction** over the whole manifest. Idempotent; an
interrupted apply resumes from the journal.

```toml
schema = 1
capabilities = ["db", "api"]

[[generate]]
kind = "scaffold"
name = "Ticket"
fields = ["id:uuid@pk", "subject:string!", "openedAt:instant"]
timestamps = true
```

The declarative engine handles lifecycle deltas:
- Appending a field to `[[generate]]` automatically writes a forward `alter table ... add column` migration.
- Removing a table-backed entity from the manifest refuses with `storage-policy-required` until an explicit retirement command is run.
- Re-declaring a previously dropped scaffold revives it.

## Traps worth knowing before you hit them

- **`jails check` is `mvn clean verify`, and the `clean` matters.** Incremental
  `verify` leaves deleted tests in `target/` and Surefire runs the leftovers.
- **Anything writing an `*IT` must configure Failsafe.** jails does this from
  the write path; if you hand-write one, `mvn verify` will pass without running
  it, which is worse than having no test.
- **ArchUnit fitness rules enforce the architecture.** `g scaffold` installs
  `DOMAIN_HAS_NO_FRAMEWORK_DEPENDENCIES` and `RAW_JDBC_STAYS_IN_ADAPTERS`.
  Keep domain records clean of Spring annotations (`@Component`, `@Repository`).
- **Capability order changes output.** `add api` before `add db` renders an
  exception handler without the `DuplicateKeyException` arm. `doctor` reports it
  and `jails sync` repairs it byte-identically. `jails add db api` in one command
  is right, because the project is re-resolved between transitions.
- **`--package` is relative to the base package**, and `--package ''` means flat.
- **A name that already carries its kind's suffix does not get it twice** —
  `g service OrderService` is `OrderService`, not `OrderServiceService`.
  `scaffold` is exempt because it spans three suffixes at once.
- **`g field` / `resource field` regenerates companion slices.** `query`, `transition`,
  `usecase`, `association`, and `durable-job` companions are re-planned in the same
  transaction, so review `--pretend` before running it on a slice you have hand-edited.
- **`jails add format` rewrites and re-records jails' own output** in the same
  transaction, so `doctor` stays clean. Do not run `spotless:apply` yourself and
  expect the same — the re-record is what keeps the ledger honest.
- **`schema diff` requires `.jails/app.toml`**; `migrate lint` runs on both
  imperative and manifest projects using the project's declared SQL driver.
- **A `transition` selects by `id` unless `--select <field>` names another**,
  and that key may come from the URL: `--select userId --path
  '/admin_api/conversations/{userId}/status'` drops the selector from the
  command record and binds it as a `@PathVariable`. The port shape is the same
  either way -- `execute(key, command, expectedVersion)` -- so the adapter and
  the controller cannot disagree about where the key was. A variable naming
  anything other than the selector is refused, and so is a second one.
- **A flag a recipe derives is refused, not ignored.** `--method` applies to
  `controller`, `client` and `transition` (PUT by default, PATCH the other
  correct spelling for an idempotent update); `--path` to `controller`,
  `usecase`, `query` and `transition`: a `query`, `usecase` or `transition` derives
  its verb from the request (GET when every filter comes from `--path`, POST
  when it carries a body), so passing one there is refused by name. Same for
  `--path` on `g scaffold`, `--via` outside `query`, `--consumes` outside the
  four recipes that bind a body.
- **An entity may not be named after a `java.lang` type.** `String`, `Class`,
  `Record` and the other 105 are refused where they would be *declared*:
  a package member outranks the implicit import, so `record String(String x)`
  types its own component as itself — and compiles. References are untouched,
  so `body:String` is still a string field.
- **`jails add <word>` names what to run instead** when the word is not a
  capability but jails has an answer — `websocket` points at `g socket`,
  `devtools` at `add dependency`, `flyway` at `add db`. A word with no answer
  still gets clap's list of what does exist.
- **Gradle pins a distribution, and a distribution cannot run on any JDK.**
  `jails new --gradle --boot 2.x` refuses a Java release the Gradle it also
  picks cannot launch on; `doctor` fails on an adopted wrapper in the same
  state; `jails why` recognises `Unsupported class file major version`, which
  names neither Gradle nor Java. Boot 2.7 needs Gradle 8.5 under JDK 21.
- **The generated ArchUnit suite is strict on a new project and will fail on
  an adopted one.** `g scaffold` says so up front when the project already has
  files outside `adapters` using `java.sql`, and names the bootstrap: set
  **both** `freeze.store.default.allowStoreCreation` and `...allowStoreUpdate`
  to `true` in `src/test/resources/archunit.properties`, run once, set both
  back, and commit `.jails/architecture-baseline`. Creation alone writes an
  empty index and every rule still fails.
- **`add h2` writes `AUTO_SERVER=TRUE`, and never `DB_CLOSE_ON_EXIT=FALSE`.**
  The first is what lets `jails db` attach while the application runs; H2
  refuses the pair outright and the application dies at startup reporting
  `Feature not supported`, which names neither property.

## Working on jails itself

`cargo build --workspace && cargo test --workspace && cargo install --path .` —
`--workspace` is not optional; `cargo test` at the root tests the root package
only. `JAILS_REQUIRE_TOOLCHAIN=1 cargo test` turns every skipped real-toolchain
test into a failure naming what was missing; use it before believing a green run
covered the generated-code path. `CLAUDE.md` is the architecture and the trap
list; `README.md` is the user-facing spec and is updated in the same change as
the code.
