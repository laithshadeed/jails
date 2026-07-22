# jails — a `rails`-CLI-inspired tool for Spring Boot / plain Maven projects

## Non-goals (read this first)

Not a framework. Not a port of Rails. Not "clone every Rails command."
We're stealing exactly one idea: **`rails generate scaffold Post title:string`
produces a whole working, tested vertical slice in one command instead of
one file at a time.** Everything below exists to deliver that one idea for
Spring Boot, as simply as possible.

No plugin architecture, no hook chains, no ORM abstraction, no template
engine dependency, no third-party generator namespace lookup (all things
real Rails has and we deliberately skip). One binary, one hardcoded
generator per artifact type, plain Rust string-building for templates
(same style already proven out in `springgen.nvim`'s Lua templates — see
`/home/laith/code/my-dotfiles/home/.config/nvim/lua/springgen/init.lua`
for the exact shape to translate).

If a feature needs a config file, a plugin system, or "just one more
abstraction layer" to implement — cut it, don't build it.

## What Rails actually gives you (researched from the real source at
`/home/laith/code/jails/rails`, distilled, not ported)

- `rails new` — whole app skeleton in one command.
- `rails generate scaffold Post title:string body:text` — cascades into:
  model + migration + controller (full CRUD) + views + routes entry + tests,
  all from one line, field types parsed straight off the CLI args.
- `rails generate model/controller/migration/...` — same generators,
  invoked standalone for just one piece.
- `rails destroy` — exact inverse of generate, deletes what it made.
- `rails test` — runs the test suite, single-test/file targeting supported.
- `rails routes` — prints the full URL table.
- Migration naming (`add_x_to_y`, `create_x`) is semantically parsed to
  auto-write the right migration body from trailing `field:type` args.

Every one of these is a UX pattern to imitate for Spring/Maven, not a
literal feature to reimplement identically — Java has no migrations-via-DSL
convention, no ORM-agnostic model layer, etc. Translate the *shape*, not
the mechanism.

## v1 scope (this is the whole thing — do not add more)

### Project creation
- `jails new <name> [--deps web,data-jpa] [--java 26]`
  Wraps start.spring.io's `starter.zip` API (same approach already proven
  in the dotfiles' `spring-init` bash function — port that exact logic).
  Maven only, Java 26 default. No Gradle option in v1.
- `jails new-cli <name>`
  Plain Maven CLI project — **write the files directly** (`pom.xml`,
  `App.java` with a `main`, `AppTest.java` with one passing JUnit 5 test).
  Do NOT shell out to `mvn archetype:generate` — it's slow, needs network,
  and without exact archetype coordinates falls into an interactive
  catalog picker (we hit this ourselves this session). Hand-write the
  three files from a template; keep the pom minimal (JUnit 5 dependency,
  maven-compiler-plugin pinned to the project's Java version, nothing else).

### Scaffolding — the actual point of this tool
- `jails generate|g scaffold <Name> [field:type ...]`
  The cascade command. Produces, in one shot, into the right
  `src/main/java/<base-package>/` (inferred from `*Application.java`,
  exact same logic as `springgen.nvim`'s `base_package()`):
  - `<Name>.java` — `@Entity`, one field per `field:type` arg (see type
    table below), `@Id @GeneratedValue` id field, plain getters/setters
    (no Lombok dependency assumption — keep the generated code
    dependency-free unless the project already has Lombok, in which case
    prefer `@Data`).
  - `<Name>Repository.java` — `extends JpaRepository<Name, Long>`.
  - `<Name>Service.java` — `@Service`, CRUD methods (`findAll`, `findById`,
    `save`, `deleteById`) thinly wrapping the repository.
  - `<Name>Controller.java` — `@RestController`, full CRUD REST endpoints
    (`GET /names`, `GET /names/{id}`, `POST /names`, `PUT /names/{id}`,
    `DELETE /names/{id}`), calling the service.
  - `<Name>ControllerTest.java` under `src/test/java/...` — one passing
    test per endpoint at minimum (can be thin — `@SpringBootTest` +
    MockMvc, or even simpler if that keeps things simpler).
- `jails generate|g <controller|service|repository|entity|test> <Name> [field:type ...]`
  Same four artifact generators standalone (this already exists as
  `:SpringNew` in `springgen.nvim` — port/adapt that Lua logic to Rust,
  don't redesign it). `entity` gets the new field:type parsing that
  `:SpringNew entity` doesn't have yet.
- `jails destroy|d <type> <Name>`
  Deletes exactly the file(s) the matching generate call would have
  created. No "replay in reverse" machinery — just compute the same
  path(s) and `rm` them, with a y/n prompt or `--force`.

**Field type table (extend only if a real need shows up — start minimal):**
`string`→`String`, `text`→`String` + `@Lob`, `int`/`integer`→`Integer`,
`long`→`Long`, `boolean`→`Boolean`, `date`→`java.time.LocalDate`,
`datetime`→`java.time.LocalDateTime`, `double`→`Double`.

### Running things
- `jails test [filter]` — wraps `mvn test` (or `mvnd` if present —
  reuse the dotfiles' now-fixed mvnd, same `.mvn/jvm.config` auto-heal
  trick isn't jails' job, just shell out to whichever binary is on PATH).
  `filter` maps to `-Dtest=filter`.
- `jails build` — `mvn package`.
- `jails run` — find the file with `static void main` under
  `src/main/java` (or use the project's already-known Application class
  for Spring projects), compile, run.

### Explicitly deferred (do not build in v1; note in README as "not yet")
`console` (no clean Java equivalent to an app-booted REPL without real
work), `routes` (real value, but needs actual annotation scanning —
v2 once v1 is proven), Gradle support, migrations/Liquibase, anything
resembling a plugin system.

## Implementation notes

- Rust, 2026 edition, `clap` (derive API) for the CLI, minimal deps
  otherwise — no template-engine crate, plain `format!`/string-building
  (matches the existing Lua template style 1:1, just translate it).
- Single binary crate, flat module structure: `main.rs`, `new.rs`,
  `generate.rs`, `run.rs` — resist splitting further until something
  actually demands it.
- Reuse logic, don't reinvent it: `spring-init` (bash) for the
  start.spring.io call, `springgen.nvim` (Lua) for the four base
  generators and base-package inference — these are the reference
  implementations to port, not just "similar" prior art.
- No tests-for-the-generator-of-generators meta-complexity — test the
  four or five things that actually matter (does `new-cli` produce a
  project that passes `mvn test`? does `generate scaffold` produce a
  project that compiles?).

## First concrete step

Nothing has been written yet. Start with `cargo init`, `new-cli` (it's
self-contained, no network dependency, and directly answers "simple
Maven CLI + unit tests" from earlier in this session), then `new`, then
the four standalone generators, then `scaffold` last (it's the other
four composed).
