# jails

A `rails`-CLI-inspired tool for Spring Boot / plain Maven projects. Steals
exactly one idea from Rails: `generate scaffold` produces a whole working,
tested vertical slice in one command.

## Build

```
cargo build && cargo test && cargo install --path .
```

Installs to `~/.cargo/bin/jails`. Shell completion:
`source <(jails completion bash)`.

## Commands

- `jails new <name> [--deps web,data-jpa] [--java 27] [--no-git] [--no-devtools]`
  — new Spring Boot project via start.spring.io. `git init` + `.gitignore`
  and `spring-boot-devtools` (needed for `run --watch`) are on by default.
- `jails new-cli <name> [--no-git]` — new plain Maven CLI project
  (hand-written `pom.xml`, `App.java`, `AppTest.java`), no network required.
- `jails generate|g scaffold <Name> [field:type ...]` — entity + repository
  + service + controller + controller test, in one shot.
- `jails generate|g <controller|service|entity> <Name> [field:type ...]`
  — a single artifact plus its companion test (only `entity` takes
  `field:type` args). `jails generate|g repository <Name>` and
  `jails generate|g test <Name>` have no companion test of their own.
- `jails generate|g record <Name> [field:type ...]` — the plain-Java
  counterpart to `entity`: an immutable record whose compact constructor
  rejects nulls, no Spring or JPA involved, plus a companion test. Same
  `field:type` table as `entity`.
- `jails generate|g command <Name>` — a CLI subcommand for `new-cli`
  projects: `run(PrintStream, PrintStream, String...)` returning an exit
  code, with a `NAME` constant to dispatch on. Output streams are arguments
  and nothing calls `System.exit`, so the companion test drives the whole
  command in-process. The class Javadoc shows how to wire it into `main`;
  jails never edits your entry point.
- `jails add|a <csv|sqlite|json> [--name <Base>] [--dry-run]` — grows an
  existing project by a whole capability: the dependency (spliced into
  `pom.xml`, comments and formatting preserved), the code that uses it, and
  a passing test. Idempotent, so re-running reports what is already there.
  `csv` gives a record-based reader over Commons CSV; `sqlite` gives a
  `Database` record plus a migration runner over plain JDBC (no ORM); `json`
  gives a shared Jackson `ObjectMapper` wrapper, with `java.time` support
  wired in and a tree API for input whose shape you can't trust.
- `jails destroy|d <type> <Name> [--force]` — deletes exactly what the
  matching `generate` call would have created.
- `jails test [filter]` — `mvn test` (or `mvnd` if present), `filter` maps
  to `-Dtest=filter`.
- `jails build` — `mvn package`.
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

Every command takes `--debug`, which prints the `mvn`/`mvnd`/`java`/`git`/`curl`
command lines jails shells out to instead of running them silently.

Field types: `string`, `text` (`@Lob` on an entity, a plain `String`
everywhere else), `int`/`integer`, `long`, `boolean`, `date`, `datetime`,
`double`.

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
| `entity`, `record`, `value` | `domain` |
| `repository` | `repository` |
| `service` | `service` |
| `controller` | `web` |
| `command`, `cli` | `cli` |
| `add csv`/`json`/`sqlite` | `adapters` |
| `add http` | `api` |
| `add testkit`/`fake` | `testkit` (test tree) |

`scaffold` spans four of them in one command and adds the imports that
crossing those boundaries costs. Everything jails writes is emitted in the
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
