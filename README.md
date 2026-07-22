# jails

A `rails`-CLI-inspired tool for Spring Boot / plain Maven projects. Steals
exactly one idea from Rails: `generate scaffold` produces a whole working,
tested vertical slice in one command.

## Build

```
cargo build --release
```

## Commands

- `jails new <name> [--deps web,data-jpa] [--java 26]` — new Spring Boot
  project via start.spring.io.
- `jails new-cli <name>` — new plain Maven CLI project (hand-written
  `pom.xml`, `App.java`, `AppTest.java`), no network required.
- `jails generate|g scaffold <Name> [field:type ...]` — entity + repository
  + service + controller + controller test, in one shot.
- `jails generate|g <controller|service|repository|entity|test> <Name> [field:type ...]`
  — a single artifact. Only `entity` takes `field:type` args.
- `jails destroy|d <type> <Name> [--force]` — deletes exactly what the
  matching `generate` call would have created.
- `jails test [filter]` — `mvn test` (or `mvnd` if present), `filter` maps
  to `-Dtest=filter`.
- `jails build` — `mvn package`.
- `jails run` — finds the file with `static void main` under
  `src/main/java` (or uses `spring-boot:run` for Spring projects), compiles
  and runs it.

Field types: `string`, `text` (`@Lob`), `int`/`integer`, `long`, `boolean`,
`date`, `datetime`, `double`.

## Not yet

Deferred out of v1 on purpose — see `prompt.md`:

- `jails console` — no clean Java equivalent to an app-booted REPL.
- `jails routes` — needs real annotation scanning, v2 once v1 is proven.
- Gradle support — Maven only for now.
- Migrations / Liquibase.
- Any kind of plugin system.
