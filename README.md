# jails

A `rails`-CLI-inspired tool for Spring Boot / plain Maven projects. Steals
exactly one idea from Rails: `generate scaffold` produces a whole working,
tested vertical slice in one command.

## Build

```
cargo build --release
```

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
- `jails add|a <csv|sqlite|json> [--name <Base>] [--dry-run]` — grows an
  existing project by a whole capability: the dependency (spliced into
  `pom.xml`, comments and formatting preserved), the code that uses it, and
  a passing test. Idempotent, so re-running reports what is already there.
  `csv` gives a record-based reader over Commons CSV; `sqlite` gives a
  `Database` record plus a migration runner over plain JDBC (no ORM); `json`
  gives a shared Jackson `ObjectMapper` wrapper.
- `jails destroy|d <type> <Name> [--force]` — deletes exactly what the
  matching `generate` call would have created.
- `jails test [filter]` — `mvn test` (or `mvnd` if present), `filter` maps
  to `-Dtest=filter`.
- `jails build` — `mvn package`.
- `jails run [--no-build] [--watch]` — finds the file with `static void
  main` under `src/main/java` (or uses `spring-boot:run` for Spring
  projects), compiles and runs it. `--no-build` skips straight to running
  whatever's already in `target/`. `--watch` (Spring Boot + devtools only)
  recompiles on every source change and lets devtools restart the
  already-running app — no manual restarts.
- `jails completion <bash|zsh|fish|elvish|powershell>` — shell completion.

Field types: `string`, `text` (`@Lob`), `int`/`integer`, `long`, `boolean`,
`date`, `datetime`, `double`.

## Not yet

Deferred out of v1 on purpose — see `prompt.md`:

- `jails console` — no clean Java equivalent to an app-booted REPL.
- `jails routes` — needs real annotation scanning, v2 once v1 is proven.
- Gradle support — Maven only for now.
- Any kind of plugin system.
