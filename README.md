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

- `jails new <name> [--deps web,jdbc] [--java 27] [--no-git] [--no-devtools]`
  — new Spring Boot project via start.spring.io. `git init` + `.gitignore`
  and `spring-boot-devtools` (needed for `run --watch`) are on by default.
- `jails new-cli <name> [--release 27] [--no-git]` — new plain Maven CLI
  project (hand-written `pom.xml`, `App.java`, `AppTest.java`), no network
  required. `App.java` is a working command dispatcher, not a Hello World
  stub, so `generate command` has something to register into from the start.
- `jails generate|g scaffold <Name> [field:type ...]` — immutable record,
  repository port, raw-JDBC adapter, service/controller stubs, and tests.
- `jails generate|g record <Name> [field:type ...]` — immutable data carrier
  with compact-constructor validation and a companion test. No persistence
  annotations are emitted.
- `jails generate|g repo <Name>` — repository port, `Jdbc<Name>Repository`
  adapter, and a disabled real-database `IT`. SQL is emitted as editable text
  blocks and `map`/`bind` remain explicit TODOs. `repository` is an alias.
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
- `jails add|a db` — PostgreSQL JDBC, Flyway, PostgreSQL Testcontainers, and
  the migration directory. Spring projects also receive the JDBC starter.
  This capability is raw SQL only: no persistence framework or generated schema.
- `jails add|a <csv|sqlite|json|testkit|fake|http|format> [--name <Base>] [--dry-run]` — grows an
  existing project by a whole capability: the dependency (spliced into
  `pom.xml`, comments and formatting preserved), the code that uses it, and
  a passing test. Idempotent, so re-running reports what is already there.
  `csv` gives a record-based reader over Commons CSV; `sqlite` gives a
  `Database` record plus a migration runner over plain JDBC (no ORM); `json`
  gives a shared Jackson `ObjectMapper` wrapper, with `java.time` support
  wired in and a tree API for input whose shape you can't trust.
- `jails destroy|d <type> <Name> [--force]` — deletes exactly what the
  matching `generate` call would have created.
- `jails test [name]` — uses `./mvnw` when present. A bare `Money` becomes
  `MoneyTest`; a name ending in `IT` runs through Failsafe and `verify`.
- `jails build` — `mvn package`.
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
  compile + tests (`mvn verify`). Both need `jails add format`.
- `jails completion <bash|zsh|fish|elvish|powershell>` — shell completion.

`generate`, `destroy` and `add` all take `--package <sub>` to override where
the code lands; `--package ''` writes straight into the base package.

Every command takes `--debug`, which prints the `mvnw`/`mvn`/`mvnd`/`java`/`git`/`curl`
command lines jails shells out to instead of running them silently.

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
| `add http`, `handler` | `api` |
| `add testkit`/`fake` | `testkit` (test tree) |

`scaffold` spans these packages without introducing persistence annotations.
Everything jails writes is emitted in the
import order palantir-java-format wants, so `add format` leaves a project that
passes `jails check` immediately.

## Neovim

`jails.nvim/` in this repo is a thin wrapper around the binary: add it to your
runtimepath and use `:Jails <subcommand> ...`. It completes subcommands and
artifact kinds, opens whatever `generate` created, confirms `destroy` in the
editor, and streams `test`/`build`/`run` into a reused terminal panel. It
shells out to the real `jails` on PATH and deliberately reimplements none of
its logic.

## Not yet

Deferred out of v1 on purpose — this is meant to stay a small tool:

- `jails console` — no clean Java equivalent to an app-booted REPL.
- `jails routes` — needs real annotation scanning, v2 once v1 is proven.
- Gradle support — Maven only for now.
- Any kind of plugin system.
