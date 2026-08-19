# jails

A small, opinionated scaffolding tool for Spring Boot and plain Maven
projects. Jails favors immutable Java types, explicit ports, visible SQL, and
short commands. It does not generate or depend on an ORM.

## Build

```
cargo build && cargo test && cargo install --path .
```

Installs to `~/.cargo/bin/jails`. Shell completion:
`source <(jails completion bash)`.

## Commands

- `jails about [--json]` (alias: `jails info`) — describes the current Maven
  context: the top-level reactor, active module, Java release, Spring Boot
  presence, Maven wrapper/command, and recursively declared modules. It works
  from any directory below a module. `--json` emits the versioned contract
  used by editor integrations and other tools.
- `jails new <name> [--deps web,jdbc] [--java 27] [--no-git] [--no-devtools]`
  — new Spring Boot project via start.spring.io. `git init` + `.gitignore`
  and `spring-boot-devtools` (needed for `run --watch`) are on by default.
  It creates `./<name>` and refuses to overwrite an existing directory. Java
  defaults to 27; while Initializr only accepts an earlier bootstrap release,
  Jails retargets the generated Maven project to the requested release.
- `jails new-cli <name> [--release 27] [--no-git]` — new plain Maven CLI
  project (hand-written `pom.xml`, `App.java`, `AppTest.java`), no network
  required. `App.java` is a working command dispatcher, not a Hello World
  stub, so `generate command` has something to register into from the start.
- `jails generate|g scaffold <Name> [field:type ...]` — immutable record,
  repository port, raw-JDBC adapter, service/controller stubs, and tests.
  When the project has a `db/migration` directory (i.e. `jails add db` has
  run), it also writes the `create table` for the same field spec — the DDL,
  the insert and the row mapper all come from one column list, which is what
  keeps them from drifting. When `src/test/resources/fixtures` exists (every
  `new`/`new-cli` project seeds it), it writes a two-row fixture keyed by the
  same column names, which `add testkit`'s `Fixtures` loader reads. Two rows,
  not one: a single row cannot catch an ordering bug or a `findAll` that
  returns only the first result.
- `jails generate|g record <Name> [field:type ...]` — immutable data carrier
  with compact-constructor validation and a companion test. No persistence
  annotations are emitted.
- `jails generate|g repo <Name> [field:type ...]` — repository port,
  `Jdbc<Name>Repository` adapter, and a disabled real-database `IT`.
  `repository` is an alias. **The adapter is derived, not stubbed**: given a
  field spec — or, with none, the record already on disk — jails writes the
  select list, the insert, the bind and the row mapper from one column list,
  so they cannot disagree about a name or a type. Types it cannot map (a
  project class that is not an enum, a collection) are named in the class
  Javadoc rather than guessed at, and a type it knows nothing about at all
  still falls back to the old `map`/`bind` TODOs.
- `jails generate|g migration <description>` (short: `g mig`) — creates the
  next `VNNN__description.sql` under `db/migration`. Migrations are
  forward-only and cannot be destroyed.
- `jails generate|g interface <Name>` — a plain Java interface.
- `jails generate|g integration-test <Name>` (short: `g it`) — a disabled
  `<Name>IT` skeleton for a real boundary test.
- `jails generate|g <controller|service|class|value|enum|sealed|test> ...`
  — the remaining small Java artifacts and their useful companion tests.
- `jails generate|g class <Name>` — a plain `public final class` and its
  companion test, both in the **base package** rather than a
  `domain`/`service` subpackage: "a class" says nothing about which layer owns
  it. No Spring and no fields — the kind to reach for when what you want
  is ordinary Java: an algorithm, a ring buffer, a parser. The generated test
  constructs the class, so it compiles the moment it is written and stops
  compiling the day you add a real constructor, which is the prompt to write
  the real assertion.
- `jails generate|g command <Name>` — a CLI subcommand for `new-cli`
  projects, registered in the project's dispatcher automatically: `run(PrintStream, PrintStream, String...)` returning an exit
  code, with a `NAME` constant to dispatch on. Output streams are arguments
  and nothing calls `System.exit`, so the companion test drives the whole
  command in-process. jails splices one line into the dispatcher's
  `commands()`; if the project has no dispatcher (or more than one) it says
  so and leaves the Javadoc's instructions as the fallback.
- `jails generate|g cli <Name>` — a second dispatcher, for projects that
  want one separate from `App.java`. `new-cli` already gives you one.
- `jails add|a db` — PostgreSQL JDBC, Flyway, PostgreSQL Testcontainers, a
  `compose.yaml` service, and the migration directory. Spring projects also
  receive the JDBC starter, `spring-boot-docker-compose` so the database
  starts with the app, `spring.datasource.*` properties read out of
  `compose.yaml` so the application can reach the database on any machine,
  and a `PostgresContainerConfig` for tests. That last one declares the
  container as a `@Bean` with `@ServiceConnection` — Spring Boot's current
  idiom, and the one its own docs prefer over `@Testcontainers`/`@Container`
  static fields, because Spring caches a context past the container's
  JUnit-managed lifetime. It registers itself from
  `src/test/resources/META-INF/spring.factories`, so every `@SpringBootTest`
  gets a DataSource without an `@Import` on the test class — Docker Compose
  is skipped in tests, and without a DataSource Spring cannot pick a driver. JDBC would also
  CGLIB-proxy every `@Repository`, which breaks `final` classes, so `add db`
  sets `spring.persistence.exceptiontranslation.enabled=false` (this
  capability is raw SQL, not JPA). `jails add` starts postgres immediately when Docker is
  on PATH (`--no-start` skips that). `jails start` / `jails stop` start and
  stop the compose services on their own; `jails run` starts whatever is in
  `compose.yaml` either way. This
  capability is raw SQL only: no persistence framework or generated schema.
- `jails add|a kafka` — a Kafka client (`spring-boot-starter-kafka` or
  `kafka-clients`) and a KRaft broker in `compose.yaml`. Stacks with `add db`
  in one file; `remove kafka` takes only the broker back out.
- `jails add|a <csv|sqlite|json|testkit|fake|http|format> [--name <Base>] [--dry-run]` — grows an
  existing project by a whole capability: the dependency (spliced into
  `pom.xml`, comments and formatting preserved), the code that uses it, and
  a passing test. Idempotent, so re-running reports what is already there.
  `csv` gives a record-based reader over Commons CSV; `sqlite` gives a
  `Database` record plus a migration runner over plain JDBC (no ORM); `json`
  gives a shared Jackson `ObjectMapper` wrapper, with `java.time` support
  wired in and a tree API for input whose shape you can't trust.
- `jails remove|rm <capability>... [--force]` — the inverse of `add`: unsplices
  the same dependencies, deletes the same files, removes compose services, and
  stops their containers. Confirms unless `--force`.
- `jails start [db|kafka]...` — `docker compose up -d` for the named services,
  or everything in `compose.yaml` when invoked with no arguments.
- `jails stop [db|kafka]...` — stop those containers (`db` is the postgres
  service). Does not delete `compose.yaml`.
- `jails doctor` — everything that has to be true before the app starts,
  checked in one pass: the JDK on PATH against the release `pom.xml` targets,
  Maven, Docker (via `docker info`, which also works when `docker` is podman's
  CLI shim), each compose service, a real TCP connection to postgres, Flyway
  migrations, the test-classpath Testcontainers initializer `add db` installs,
  `DOCKER_HOST` for Testcontainers, both Jackson artifacts, the HTTP port, and
  every constructor dependency that no bean supplies. Reads only — it never
  starts, stops or writes anything, so it is safe mid-debug. Each failing line
  carries the command that fixes it, and a failure exits non-zero so
  `jails doctor && jails run` works.
- `jails why [log]` — translate a failure into what it actually means. Reads a
  log file, or stdin (`jails test 2>&1 | jails why`), or with neither it starts
  the app and reads what it prints. Every rule was written against a failure
  that really happened: "Could not find a valid Docker environment" (Testcontainers
  does not read podman's socket), "Failed to determine a suitable driver class",
  "required a bean of type", port clashes, Flyway checksum mismatches, JDK/release
  mismatches, `NoSuchMethodError` version skew. An unrecognised failure is
  reported as unrecognised rather than guessed at.
- `jails routes [--json]` — every HTTP route the source declares: Spring's
  `@GetMapping`/`@PostMapping`/… with the type-level `@RequestMapping` prefix
  applied, plus `generate handler`'s `HttpHandler` types and their `PATH`
  constant. Read from source, so it answers on a project that does not start.
- `jails beans [pattern] [--json]` — every `@Component`/`@Service`/`@Repository`/
  `@Controller`/`@Configuration` and every `@Bean` method, with each
  constructor dependency marked resolvable or not. A dependency naming a type
  this project declares but never registers is the static half of "required a
  bean of type … that could not be found", caught before the context starts.
- `jails rename <Old> <New> [--dry-run] [--force]` — rename a type, its
  `Test`/`Tests`/`IT` companions, and every reference. Textual, and honest
  about it: it matches whole identifiers only (`Reward` never matches inside
  `RewardHistory`) and leaves string literals alone, reporting how many
  mentions it skipped. Neovim's `grn` (jdt.ls) is scope-aware and better where
  it works — this is for when the language server is not attached or the
  project does not currently compile.
- `jails db|dbconsole [file] [--no-start] [-- <args>...]` — `rails dbconsole`:
  `psql` against the compose postgres that `add db` started (credentials from
  `compose.yaml`). Starts postgres first unless `--no-start`. Pass a SQLite
  file to open it with `sqlite3` instead. Extra args after `--` go to the
  client: `jails db -- -c 'select 1'`.
- `jails console|c [--no-build] [-- <args>...]` — `jshell` with the project's
  compiled classes and Maven runtime classpath. This is not a Spring-booted
  REPL (Java has no `rails console`); it is a JDK shell that can see your
  types. `--no-build` skips `mvn compile`.
- `jails destroy|d <type> <Name> [--force]` — deletes exactly what the
  matching `generate` call would have created.
- `jails test [name]` — uses `./mvnw` when present. A bare `Money` becomes
  `MoneyTest`; a name ending in `IT` runs through Failsafe and `verify`.
- `jails build` — `mvn package`.
- `jails clean` — `mvn clean`. Wipes `target/` so leftover classes from deleted sources cannot linger; `jails check` does this automatically.
- `jails mvn -- <args...>` — escape hatch for Maven options Jails should not
  duplicate; it still prefers the project wrapper.
- `jails run [--no-build] [--watch] [-- <args>...]` — finds the file with
  `static void main` under `src/main/java` (or uses `spring-boot:run` for
  Spring projects), compiles and runs it. Everything after `--` is forwarded
  to the program: `jails run -- normalise input.json`. When the project has a
  `generate cli` dispatcher, that wins over a leftover `App.java`, so argv
  actually reaches something that routes it. `--no-build` skips straight to
  running whatever's already in `target/`. `--watch` (Spring Boot + devtools
  only) recompiles on every source change and lets devtools restart the
  already-running app — no manual restarts.
- `jails fmt` — reformat in place (Spotless); `jails check` — format check +
  compile + tests (`mvn clean verify`). Both need `jails add format`. The
  `clean` is load-bearing: Maven's incremental compile leaves deleted tests
  in `target/`, and Surefire will still run them.
- `jails completion <bash|zsh|fish|elvish|powershell>` — shell completion.

`generate`, `destroy`, `add` and `remove` all take `--package <sub>` to override where
the code lands; `--package ''` writes straight into the base package.

Every command takes `--debug`, which prints the `mvnw`/`mvn`/`mvnd`/`java`/`git`/`curl`
command lines jails shells out to instead of running them silently.

Every command that writes also takes `--pretend` (`-p`): it runs every check
and prints what would change, then stops without touching the project. Global
on purpose — Rails puts it on every generator rather than on the few that
looked risky, and the value is never having to remember which commands
support it. `add`, `remove` and `rename` spell the same thing `--dry-run`.

- `jails generate|g handler <Name>` — an `HttpHandler` in `api/` for one
  resource: derives its path (`WorkItem` → `/work-items`), takes its service as
  a constructor dependency so the same code path serves CLI and HTTP, and maps
  outcomes to 400 / 404 / 422 through a shared `ApiError` envelope (generated
  if absent). The companion test drives it over a real loopback socket on an
  ephemeral port.
- `jails generate|g sealed <Name> <Variant...>` — a sealed interface with a
  `permits` clause and one record per variant, plus a test whose `switch` has
  no `default`, so adding a variant breaks the build. The closed set an enum
  can't model, because each case carries its own data.
- `jails generate|g enum <Name> <CONSTANT...>` — a plain enum plus its test.
  Also the one type jails can build a sample of, which is why an enum-typed
  component keeps its companion test working.

## Field syntax

`name:type`, with two modifiers:

**Case picks the table.** A lowercase type is one of jails' own — `string`,
`text`, `int`/`integer`, `long`, `boolean`, `date`, `datetime`, `instant`,
`uuid`, `currency`, `decimal`, `bytes`, `duration`, `zone-id`, `uri`, `path`,
`double`, plus `list<T>` and `map<K,V>` whose elements resolve the same way (`list<Match>`,
`map<string,double>`). A collection component is defensively copied and
defaults to empty rather than null, so no consumer has to guard a bucket. A **capitalised** one is a
type this project owns and is used verbatim, so the generators compose:

```
jails g enum Currency GBP EUR USD
jails g record SourceRef system:string externalId:string
jails g value CanonicalTransaction id:string! amountMinor:long \
    currency:Currency source:SourceRef note:string?
```

The Java spellings of the built-ins (`String`, `LocalDate`, …) still mean the
built-in, so `id:String` behaves like `id:string`.

**A suffix picks the validation.** `name:string!` is required *and* non-blank;
`name:string?` becomes an `Optional<String>` component (pass `null` to mean
absent); bare `name:string` is required but may be blank. Hardcoding one policy
is what made every generated value type reject blank descriptions. `!` is a
text rule, so `when:date!` is an error rather than a no-op.

jails cannot invent a sample of a type you own, so a companion test that needs
one is generated in full and `@Disabled`, naming the component it needs. Two
cases escape that: an enum is filled in with `Currency.values()[0]`, and a `?`
component with `Optional.empty()`.

## What a new project looks like

Both `new` and `new-cli` lay down the standard Maven tree plus an empty
`src/test/resources/fixtures/` (with a `.gitkeep`, since git won't track an
empty directory) — the conventional home for sample CSV/JSON/SQL files that
tests read off the classpath, which is exactly what `add testkit`'s `Fixtures`
helper and the `add csv|json|sqlite` capabilities want.

Generated code goes into the subpackage its layer conventionally owns, not
into one flat pile beside `App.java`:

| Kind | Package |
| --- | --- |
| `record`, `value` | `domain` |
| `service` | `service` |
| `controller` | `web` |
| `command`, `cli` | `cli` |
| `repo` (port) | `app` |
| `repo` (adapter) | `adapters` |
| `migration` | `src/main/resources/db/migration` |
| `add csv`/`json`/`sqlite` | `adapters` |
| `add db` / `add kafka` | `compose.yaml` (and `src/main/resources/db/migration` for `db`; Spring `add db` also writes `PostgresContainerConfig` and test-classpath `META-INF/spring.factories`) |
| `add http`, `handler` | `api` |
| `add testkit`/`fake` | `testkit` (test tree) |

`scaffold` spans these packages without introducing persistence annotations.
Everything jails writes is emitted in the
import order palantir-java-format wants, so `add format` leaves a project that
passes `jails check` immediately.

## Neovim

`jails.nvim/` in this repo is a thin wrapper around the binary: add it to your
runtimepath and use `:Jails <subcommand> ...`. It completes subcommands and
artifact kinds, capabilities, command options, and existing test class names.
Commands run from the nearest `pom.xml`, so they still work when Neovim's
global working directory is elsewhere. Generated files are added to the
quickfix list and the first is opened; `destroy` is confirmed in the editor;
and long-running commands share a reusable terminal panel. Configure it after
adding the runtime path:

```lua
require('jails').setup({ terminal_height = 12 })
```

The plugin shells out to the real `jails` on PATH and deliberately
reimplements none of its project-generation logic.

## Not yet

Deferred out of v1 on purpose — this is meant to stay a small tool:

- Gradle support — Maven only for now.
- A runtime bean/route view (booting the context and asking Spring itself).
  `routes` and `beans` read source instead, which is instant and works on a
  project that does not start — at the cost of anything decided at runtime.
- Any kind of plugin system.
