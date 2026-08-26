---
name: jails
description: "Scaffolding, evolving and operating Spring Boot and plain Java projects with the jails CLI. Triggers on: jails, jails new, jails generate, jails g, jails add, jails resource, jails app apply, jails doctor, jails test, jails sql, spring boot scaffolding, ports and adapters, raw jdbc java, testcontainers spring, flyway migrations with jails, hex architecture java."
---

# jails

A Rails-inspired CLI for Java and Spring Boot (JDK 26 default, 21 floor;
Maven and Groovy Gradle). Ports and adapters, pure records, raw `JdbcClient`,
**no ORM**. Every mutation is one transaction: prepared in memory, previewable
byte-for-byte, committed through a write-ahead journal in `.jails/`.

State of this file: verified against `jails 0.1.0` at HEAD `e3c7041`
(2026-08-26) by running the binary, not by reading the docs. `bugs.md` is the
live defect ledger and `research.md` is what is not built yet — both were
rechecked at the same commit.

## The three rules that explain everything else

1. **Preview and apply are the same computation.** `--pretend` (alias
   `--dry-run`) stops before the commit; it does not run a second planner. Its
   operation list is byte-identical to the real run's. Add `--diff` for the
   bytes, `--ast` for the semantic edits.
2. **The ledger is the identity, the files are the projection.** `.jails/ledger.toml`
   records what jails declared. `destroy` stops declaring something and
   reconciliation works out what that means — there is no path table.
3. **Migrations are append-only and sealed at first publication.** Nothing
   replaces or deletes a published `VNNN__*.sql`. Every schema change is a new
   forward migration. There are no down migrations and there never will be.

## Command map

Everything below exists and runs today.

### Create

| | |
|---|---|
| `jails new <name> [--deps web,jdbc] [--java 26]` | Spring Boot via start.spring.io |
| `jails new <name> --offline` | same, from the vendored fixture, no network |
| `jails new <name> --gradle [--boot 2.7.18] [--gradle-version 8.5]` | Groovy Gradle, written locally |
| `jails new-cli <name> [--release 26]` | plain Maven CLI with a command dispatcher |
| `jails new <name> --app <manifest.toml>` | create **and** apply a manifest in one transaction |
| `jails adopt` | write a `[layout]` table matching where an existing project already keeps things |

### Generate — `jails g <kind> <Name> [fields...]`

Plain Java: `record` `class` `interface` `value` `enum` `sealed` `strategy`
`command` `cli` `handler` `factory` `test` `integration-test` `cases`
`migration`.

Spring: `scaffold` `controller` `service` `repo` `dto` `usecase` `query`
`transition` `event` `client` `fetcher` `job` `durable-job` `association`
`http-workflow` `http-sink` `idempotency` `auth` `webhook` `search`.

`jails explain <kind>` gives the rationale *and the trap* for each one — read it
before generating something unfamiliar; it is a hand-maintained table, not
generated filler.

Common flags: `--package <sub.package>` (relative to the base package),
`--timestamps`, `--index "col, col desc"` (repeatable), `--on <Resource>`,
`--yields <Event>`.

### Add capabilities — `jails add <capability>...`

`db` `sqlite` `h2` `kafka` `redis` `csv` `json` `mail` `http` `api` `actuator`
`cache` `security` `cors` `sse` `observability` `format` `coverage` `testkit`
`fake` `toxiproxy` `loadtest` `docker` `k8s` `ci`

`jails.toml`'s `[project] capabilities` is maintained by `add`/`remove`, not by
hand. `jails sync` re-plans every recorded capability and repairs drift.

### Evolve a resource — `jails resource ...`

This is the surface that replaced "destroy and regenerate". Use it.

```
jails resource status <Entity>                     # the lifecycle oracle
jails resource field add    <Entity> <name:type>  [--default-literal V | --backfill-file P]
jails resource field rename <Entity> <old> <new>   --column single-cutover
jails resource field type   <Entity> <field>       --to <type> --strategy safe|expand-contract
jails resource field nullability <Entity> <field>  --nullable | --required [--backfill-file P]
jails resource field drop   <Entity> <field>       --confirm-column <exact-column>
jails resource repair <Entity> --strategy roll-forward
jails resource revive <Entity> --table <preserved-table>
```

Each writes a new forward migration. The guards ask for exactly the evidence
they need and no more.

### Destroy

```
jails destroy <kind> <Name> [--force]
jails destroy scaffold <Name> --storage preserve
jails destroy scaffold <Name> --storage drop --confirm-table <exact-table>
```

A table-backed entity refuses without an explicit storage policy. `--storage
drop` writes `VNNN__drop_<table>.sql`; regenerating afterwards writes a fresh
create migration, so the cycle is complete and its history is readable.

### Verify and inspect

```
jails doctor [--json]        # environment + recorded-output drift; every FAIL carries a fix:
jails migrate --check        # apply every migration to a scratch database
jails migrate lint           # classify destructive statements  (needs .jails/app.toml)
jails routes | beans | stats | notes | lint | src <Type>
jails why [logfile]          # root-cause a failure from a table of real signatures
jails history | show <id> | undo <id>
```

`routes` and `beans` read source, never a running context — they work on a
project that will not start, and they say so with `evidence:` and `limitation:`
lines.

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

`name:type[!?][@constraint...]`

**Case is the rule.** Lowercase is a jails builtin; capitalised is a type the
project owns, passed through verbatim with no import.

Builtins: `string` `int` `long` `double` `decimal` `boolean` `uuid` `date`
`datetime` `instant` `duration` `zone-id` `uri` `path` `currency` `bytes`.

Suffix — optionality:
- (bare) non-null
- `!` non-null **and** non-blank
- `?` emits `Optional<T>` and normalises a null one in the compact constructor

Constraints, a closed set, parsed off the *type* so either order works:
`@pk` `@unique` `@index` `@positive` `@nonnegative` `@scope`.

All but `@scope` change SQL and nothing about the Java type. `@scope` marks a
request-boundary field proved against a same-named JWT claim — it is how tenancy
works without the word "tenant" existing in core, and it refuses unless
`add security` wrote a `ScopeAuthorizer`.

**An unknown marker is an error, not a no-op.** So is an unknown type, a
duplicate field name, a Java or SQL reserved word, two names folding to one
column, a scaffold with no `@pk` or two, and an entity name whose lower-camel
spelling is a Java keyword or whose plural table is a PostgreSQL keyword. Every
one of those refuses before a byte is written.

Composite or ordered indexes go on the command, not the field:
`--index "author, created_at desc"`.

## Two ways to drive it

**Imperative** — `jails g ...` and `jails add ...`, recorded in the ledger as you
go. This is the better-finished path today.

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

The manifest is deliberately domain-blind: a crawler, a support inbox and a
payments gateway are three lists of the same generic intents. `examples/` holds
the proof applications and `examples/proof-policy.tsv` declares the tier each is
held to.

## Traps worth knowing before you hit them

- **`jails check` is `mvn clean verify`, and the `clean` matters.** Incremental
  `verify` leaves deleted tests in `target/` and Surefire runs the leftovers.
- **Anything writing an `*IT` must configure Failsafe.** jails does this from
  the write path; if you hand-write one, `mvn verify` will pass without running
  it, which is worse than having no test.
- **Capability order changes output.** `add api` before `add db` renders an
  exception handler without the `DuplicateKeyException` arm. `doctor` reports it
  and `jails sync` repairs it byte-identically. `jails add db api` in one command
  is right, because the project is re-resolved between transitions.
- **`--package` is relative to the base package**, and `--package ''` means flat.
- **A name that already carries its kind's suffix does not get it twice** —
  `g service OrderService` is `OrderService`, not `OrderServiceService`.
  `scaffold` is exempt because it spans three suffixes at once.
- **The POST body wants the client to supply the id.** `@pk` renders as
  `@NotNull UUID id` and the generated `.http` posts a fixed UUID. Posting it
  twice violates the primary key.
- **`g field` on an entity with `g query` / `g usecase` companions regenerates
  those companions too.** That is one transaction, so review `--pretend` before
  running it on a slice you have hand-edited.
- **`jails add format` rewrites and re-records jails' own output** in the same
  transaction, so `doctor` stays clean. Do not run `spotless:apply` yourself and
  expect the same — the re-record is what keeps the ledger honest.
- **`migrate lint` and `schema diff` need `.jails/app.toml`** and so do not run
  on an imperative project.

## Known-broken paths — check `bugs.md` before trusting these

Verified broken at HEAD `e3c7041`:

- **`jails rename <Old> <New>` on a table-backed entity commits the Java half
  only.** No `alter table ... rename to`, no create migration for the new table
  name. Flyway then stops, and both `doctor` and `resource status` report health.
  Use `resource field rename` for fields; for an entity, prefer not renaming yet.
  (`bugs.md` B2.)
- **A write that fails mid-transaction leaves the project torn**, and `resource
  repair --strategy roll-forward` then adopts the tear as the recorded truth.
  If a command fails with a filesystem error, inspect the tree before running
  anything else. (`bugs.md` B18.)
- **`jails new --app` discards the whole project** if a post-commit compose
  effect fails — e.g. something else already holds `:5432`. Free the port, or
  run `new` and `app apply` as two steps. (`bugs.md` B45.)
- **On a manifest project a field cannot be added to an existing entity** —
  `app apply` hits the migration seal and the manifest has no way to append.
  (`bugs.md` B20.)
- **Deleting an entity from the manifest skips the storage ceremony** the
  imperative `destroy` insists on: every Java file goes, the create migration
  and the table stay, and nothing reports the orphan. (`bugs.md` B22.)
- **Neither half of an association can be destroyed** — each refusal names the
  other command, and `destroy association` refuses as forward-only.
  (`bugs.md` B37.)
- **`doctor` cannot tell whether an entity's fields match the columns its
  migrations created.** It answers "are these the bytes jails wrote". Until that
  check exists, a green `doctor` is not evidence that the project will start —
  run `jails migrate --check` and `jails check`.

## Working on jails itself

`cargo build --workspace && cargo test --workspace && cargo install --path .` —
`--workspace` is not optional; `cargo test` at the root tests the root package
only. `JAILS_REQUIRE_TOOLCHAIN=1 cargo test` turns every skipped real-toolchain
test into a failure naming what was missing; use it before believing a green run
covered the generated-code path. `CLAUDE.md` is the architecture and the trap
list; `README.md` is the user-facing spec and is updated in the same change as
the code.
