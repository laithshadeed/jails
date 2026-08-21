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
- `jails generate|g scaffold <Name> [field:type ...]` — a REST resource that
  **runs**, not a set of stubs: immutable record, repository port, a derived
  raw-JDBC adapter, an in-memory adapter (so the app starts before there is a
  database), request/response DTOs with validation from the field spec, a
  service, a controller with the four operations and the status codes the
  situations mean (201 with `Location`, 204, 404 rather than an empty 200),
  and tests for each. `g scaffold Note id:uuid title:string!` then `jails run`
  gives you `POST /notes` → 201 and `GET /notes/nope` → 404 with nothing to
  write. **Exactly one adapter is a bean**, and which one depends on whether
  the project has a database yet: with `spring-boot-starter-jdbc` present the
  JDBC adapter is a `@Repository` over `JdbcClient` with named parameters,
  and the in-memory one becomes a plain fake for tests; without it there is no
  `JdbcClient` type to compile against at all, so the adapter is plain JDBC
  over a caller-owned `Connection` and the in-memory one carries
  `@Repository` so the app still starts. Annotating both would make two beans
  qualify for one injection point, which is the ambiguity `jails beans`
  reports.
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
  and a `TestcontainersConfig` for tests. That last one declares the
  container as a `@Bean` with `@ServiceConnection` — Spring Boot's current
  idiom, and the one its own docs prefer over `@Testcontainers`/`@Container`
  static fields, because Spring caches a context past the container's
  JUnit-managed lifetime. `add db` splices `@Import(TestcontainersConfig.class)`
  into the `@SpringBootTest` classes already in the project, because Docker
  Compose is skipped in tests and without a DataSource Spring cannot pick a
  driver — so adding the capability and walking away would break the
  `contextLoads` test that came with the project. It is an `@Import` rather
  than a global `spring.factories` registration (which jails used to write)
  so that pure slices and `@WebMvcTest`s do not each start a PostgreSQL they
  never query. JDBC would also
  CGLIB-proxy every `@Repository`, which breaks `final` classes, so `add db`
  sets `spring.persistence.exceptiontranslation.enabled=false` (this
  capability is raw SQL, not JPA). `jails add` starts postgres immediately when Docker is
  on PATH (`--no-start` skips that). `jails start` / `jails stop` start and
  stop the compose services on their own; `jails run` starts whatever is in
  `compose.yaml` either way. This
  capability is raw SQL only: no persistence framework or generated schema.
- `jails add|a kafka` — a Kafka client (`spring-boot-starter-kafka` or
  `kafka-clients`), a KRaft broker in `compose.yaml`, and on Spring the
  poison-message path every project writes on its second day: a `KafkaConfig`
  with a `DefaultErrorHandler`, a `DeadLetterPublishingRecoverer` that names
  `<topic>.DLT` explicitly (the recoverer's own default is `-dlt`, so a
  project that declares `.DLT` and ships a consumer for it finds it empty),
  and a retryable/fatal classification — a record that will not deserialize
  fails identically on every attempt, and retrying it blocks the partition
  with consumer lag as the only symptom. The properties include
  `ErrorHandlingDeserializer` (without which that bad record never reaches
  the error handler at all), `group.protocol=consumer` (KIP-848 is the broker
  default since Kafka 4.0 but the *client* default is still `classic`),
  `acks=all` and `enable.idempotence`. Test dependencies come too —
  `testcontainers-kafka` and `awaitility`, without which no test can touch a
  broker. Stacks with `add db` in one file; `remove kafka` takes only the
  broker back out.
- `jails add|a <csv|sqlite|json|testkit|fake|http|format> [--name <Base>] [--dry-run]` — grows an
  existing project by a whole capability: the dependency (spliced into
  `pom.xml`, comments and formatting preserved), the code that uses it, and
  a passing test. Idempotent, so re-running reports what is already there.
  `csv` gives a record-based reader over Commons CSV; `sqlite` gives a
  `Database` record plus a migration runner over plain JDBC (no ORM); `json`
  gives a shared Jackson 3 (`tools.jackson`) `JsonMapper` wrapper and a tree
  API for input whose shape you can't trust. Jackson 3 has `java.time` built
  in, so this is **one** artifact rather than two -- and adding the 2.x
  `com.fasterxml` pair to a Boot 4 project (whose web starter already brings
  Jackson 3) put two Jackson majors on one classpath, which does not conflict,
  does not warn, and leaves half the code on a mapper nobody configured.
  `jails doctor` reports that case.
`remove` names every generated file that has changed since jails wrote it,
before deleting it — at the confirmation prompt, in `--dry-run`, and under
`--force`, which is the path that would otherwise be silent. "It exists" is
not ownership: a `CsvReader` you spent an afternoon on looks exactly like the
stub jails generated. It does not refuse — `remove` is the documented inverse
of `add` — but it will not delete your work without saying so, the same line
it already draws for hand-written properties inside a jails-owned block.

- `jails remove|rm <capability>... [--force]` — the inverse of `add`: unsplices
  the same dependencies, deletes the same files, removes compose services, and
  stops their containers. Confirms unless `--force`.
- `jails start [db|kafka]...` — `docker compose up -d` for the named services,
  or everything in `compose.yaml` when invoked with no arguments.
- `jails stop [db|kafka]...` — stop those containers (`db` is the postgres
  service). Does not delete `compose.yaml`.
- `jails add|a api` — the error-handling slice every Spring service writes by
  hand: a `@RestControllerAdvice` extending Spring's own
  `ResponseEntityExceptionHandler`, so framework exceptions keep their
  statuses, plus a sealed `ApiException` (`NotFound`/`Conflict`/`Rejected`)
  the advice switches over with no `default` branch — adding a variant stops
  the build until its status is decided. Responses are RFC 9457
  `application/problem+json`, and bean-validation failures report each bad
  field in a `fields` extension member instead of a bare 400. Adds
  `spring-boot-starter-validation`.
- `jails add|a actuator` — health, info and metrics, exposed by name rather
  than with `*` (which publishes heap dumps and the resolved environment). The
  generated test pins both halves: health is up, `env` and `heapdump` are not.
- `jails add|a cache` — `@EnableCaching` plus Caffeine and a **bounded** spec.
  The test counts invocations, because a cache that is silently off looks
  exactly like a cache that is on.
- `jails generate|g dto <Name>` — `<Name>Request` and `<Name>Response` records
  for a domain type, with the mapping both ways and a round-trip test. Reads
  the record already on disk when no field spec is given. Constraints come
  from the field spec (`@NotNull`, `@NotBlank`; never on a primitive, which
  cannot be null), and an `Optional<T>` component becomes a plain nullable
  field on the wire. Splices `spring-boot-starter-validation` if absent.
- `jails generate|g client <Name>` — a declarative HTTP client: an
  `@HttpExchange` interface, an `@ImportHttpServices` registration, and a test
  that drives it against a real socket on an ephemeral port. No base URL in
  the code — the group's URL comes from
  `spring.http.serviceclient.<group>.base-url`. Splices
  `spring-boot-starter-restclient`, without which the proxies are built but no
  base URL is ever applied (the failure reads "URI with undefined scheme" and
  says nothing about a missing dependency).
- `jails generate|g job <Name>` — a `@Scheduled` component whose interval is a
  property, not a constant, and which catches its own failures: an exception
  escaping a scheduled method cancels every future run, silently.
- `jails add|a redis` — a `KeyValueStore` wrapper, a compose service, and an
  `IT` against a real container. Every write takes a lifetime:
  `opsForValue().set(k, v)` with no expiry stores a key forever, so the TTL is
  a required argument with a configured default rather than something you can
  forget. `@ServiceConnection(name = "redis")` names the factory explicitly —
  leaving Boot to infer it from the image fails at runtime with `No
  ConnectionDetails found for source`, which reads like a missing dependency
  rather than a naming problem.
- `jails add|a observability` (alias `metrics`) — a Prometheus scrape endpoint,
  plus the two conventions every project rediscovers. Meter names are declared
  once in `AppMetrics` rather than as a string literal per call site, because
  those drift (`orders.created`, `order_created`) until a dashboard quietly
  stops matching; and a generated `MeterRegistryCustomizer` tags every meter
  with the application name, because two services reporting to one Prometheus
  otherwise publish the same series and their values are summed — graphs that
  are wrong rather than missing. A property cannot do that job:
  `management.observations.key-values.*` tags observations, and a `Counter`
  registered straight on the registry is not one. The exposure list is unioned
  with whatever is already set, so `add actuator` and `add observability` in
  either order both leave `prometheus` exposed. The generated test scrapes the
  live endpoint rather than the registry, since a missing registry is not an
  error — it is a 404 nobody notices for days.
- `jails add|a toxiproxy` (alias `faults`) — network failure you can switch on.
  A Toxiproxy container goes in front of a dependency and `Faults` gives the
  test three verbs: `cut()` refuses connections, `latency()` slows them, and
  `blackhole()` accepts the connection and then says nothing. The third is the
  one worth having — stopping a container proves only that a dead dependency
  fails, while the outage that actually pages you is a socket that stays open
  and never answers, which every "is the port open" check calls healthy and
  which hangs the calling thread forever unless a read timeout is set. Point
  the application at `fault.host()`/`fault.port()`, not at the dependency's own
  address, or the traffic misses the proxy and the test passes for no reason.
  `heal()` undoes toxics *and* re-enables a cut proxy, so one test cannot leave
  the next one failing against something it never touched. The generated test
  proxies Toxiproxy's own control API — no second image, and a failure there
  can only be the proxy.
- `jails add|a security` — an explicit `SecurityFilterChain` instead of the
  default one. Adding the starter alone secures every endpoint and prints a
  generated password at startup, which is safe and opaque — and the usual
  reaction is a blanket `permitAll()` nobody revisits. The generated chain is
  default-deny, permits only `/actuator/health/**`, and is stateless with CSRF
  off (safe *only* together: CSRF protects ambient credentials, and a chain
  that issues no session cookie has none). The test asserts both directions,
  because a test that authenticated requests succeed passes just as happily
  against `permitAll()` on everything.
- `jails generate|g event <Name>` — a Kafka slice: the payload record, a
  publisher keyed by event id (ordering is per partition; a null key
  round-robins), a listener that deliberately does not catch (swallowing
  commits an offset for a message never processed), and an `IT` that publishes
  through a real broker via Testcontainers and waits on a latch. Field
  declarations use the same typed model as the other generators (for example,
  `id:uuid crawlRunId:uuid url:uri occurredAt:instant`); a typed event requires
  a non-optional `id`, and the generated publisher key, samples, and assertions
  derive from those fields. With no fields, the legacy `String id, Instant
  occurredAt` contract remains available. `jails add
  kafka` on Spring now also writes the properties that make this work at all —
  `auto-offset-reset=earliest` (a new consumer group otherwise starts at the
  end of the topic and sees nothing published before it joined), the
  `JacksonJson*` serializers (the older `Json*` pair is deprecated for removal
  since Spring Kafka 4.0), and the deserializer's trusted-packages list.
- `jails stats` — files, lines and code per layer, plus the test-to-code
  ratio. Comment lines are excluded, or generated Javadoc would triple every
  count.
- `jails notes [tag]` — `TODO`/`FIXME`/`HACK`/`XXX` in comments. String
  literals are excluded, so jails' own `"TODO: map a row"` exception messages
  do not bury the real ones.
- `jails doctor` — everything that has to be true before the app starts,
  checked in one pass: the JDK on PATH against the release `pom.xml` targets,
  Maven, Docker (via `docker info`, which also works when `docker` is podman's
  CLI shim), each compose service, a real TCP connection to postgres, Flyway
  migrations, the test-classpath Testcontainers initializer `add db` installs,
  `DOCKER_HOST` for Testcontainers, both Jackson artifacts, the HTTP port, and
  every constructor dependency that no bean supplies. It also catches three
  things that fail silently rather than loudly: a compose provider that is
  podman-compose while `spring-boot-docker-compose` is on the classpath (the
  app dies during startup before any of its own code runs), two Jackson majors
  declared at once (they do not conflict, so half the code ends up on a mapper
  nobody configured), and an `@Repository` still on an in-memory adapter in a
  project that has a `DataSource` — which starts perfectly and serves every
  request out of a map that empties on restart. Reads only — it never
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
- `jails migrate [--no-start]` — apply every migration to a scratch database,
  in Flyway's order, and report the first one that fails with its file and
  line. The database is created and dropped around the run, so your data is
  untouched, and it is the same server and version the migrations will really
  run against. Deliberately **not** a `doctor` check: doctor is read-only by
  contract so it stays safe mid-debug, and it can only answer whether anything
  *will* run the migrations — this answers whether they work. Exits non-zero on
  failure.
- `jails kafka <topics|describe|send|poison|tail|dlt|lag|reset> [--no-start]`
  — the broker counterpart to `jails db`. Everything runs inside the compose
  broker container, so there is nothing to install: the Kafka CLI tools ship
  in the image. The topic defaults to the one the source declares (a `TOPIC`
  constant, read textually so it answers on a project that does not compile)
  and the group to `spring.kafka.consumer.group-id`. `send` publishes one JSON
  record with a key — ordering is per partition and a null key round-robins;
  `poison` publishes an unparseable one so you can watch it reach the DLT
  rather than stall the partition; `dlt` tails the dead-letter topic with
  headers, which is where the failure reason is; `lag` is the one number that
  says whether a consumer is keeping up.
- `jails console|c [--no-build] [-- <args>...]` — `jshell` with the project's
  compiled classes and Maven runtime classpath. This is not a Spring-booted
  REPL (Java has no `rails console`); it is a JDK shell that can see your
  types. `--no-build` skips `mvn compile`.
- `jails destroy|d <type> <Name> [--force]` — deletes exactly what the
  matching `generate` call would have created.
- `jails test [name]` — uses `./mvnw` when present. A bare `Money` becomes
  `MoneyTest`; a name ending in `IT` runs through Failsafe and `verify`.
  Any command that writes an `*IT` also splices the Failsafe plugin, because
  it is *not* part of the Spring Boot parent's default build — without it
  `mvn verify` completes, reports success, and runs none of them.
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

## Declarative applications: `jails app`

`.jails/app.toml` composes the same generic capabilities and generators the
CLI exposes. It is a reproducible command sequence, not a domain-specific
plugin or a second programming language:

```toml
schema = 1
capabilities = ["db", "api", "actuator", "security"]

[[generate]]
kind = "enum"
name = "TaskStatus"
fields = ["PENDING", "RUNNING", "DONE"]

[[generate]]
kind = "scaffold"
name = "Task"
fields = ["id:uuid@pk", "status:TaskStatus@index", "createdAt:instant"]
indexes = ["status, created_at desc"]
```

- `jails app plan [--manifest <path>]` validates the manifest and shows its
  capability and generation intents without writing.
- `jails app apply [--manifest <path>] [--no-start]` installs capabilities in
  declaration order, then applies generation intents in declaration order.
- global `--pretend` turns `app apply` into the same read-only plan.

Progress is recorded after each successful intent in
`.jails/app-state-v1`, so retrying after a later failure does not collide with
completed generation. Capabilities remain independently idempotent through
`jails.toml`. Changing an already-applied intent does not rewrite user code:
it is a new intent, and the normal generator collision check stops for an
explicit evolution step.

The manifest is intentionally a closed schema: `schema`, `capabilities`, and
`[[generate]]` entries with `kind`, `name`, `fields`, `indexes`, `package`,
`strategy_on`, and `strategy_yields`. Unknown keys fail instead of being
silently ignored. See [`examples/DOGFOOD.md`](examples/DOGFOOD.md) for the
complete crawler and support-inbox flows and the friction ledger driving the
next generic improvements.

This first slice records resumable intent; it does not yet provide the
planned universal atomic `ChangeSet`, safe field evolution, or production
profile verification. Those limitations are explicit rather than hidden
behind a “production-ready” label.

## `jails.toml`

`--package` is a per-call override. For a project whose layout differs from
jails' defaults throughout, put the per-project one at the project root next
to `pom.xml`:

```toml
[layout]
service   = "application"
adapters  = "persistence"
web       = "api"
```

The keys are the layer names in the table above (`domain`, `app`, `service`,
`web`, `cli`, `adapters`, `api`, `testkit`, `clients`, `jobs`, `messaging`)
and the values are package paths relative to the base package — dotted
(`infra.jdbc`) for a nested one, empty for the base package itself. A key that
is not a layer name is an error rather than a no-op: a `jails.toml` saying
`adapter = "persistence"` that silently kept writing to `adapters` would be
worse than no file at all. `--package` still wins for a single call.

### `[project]` — what the project is made of

The other half of the file is the list of capabilities the project has:

```toml
[project]
capabilities = ["db", "kafka", "json", "testkit", "format"]
```

You do not maintain this. `jails add` records every capability it applies and
`jails remove` takes it back out, so the file is a true description of the
project rather than one somebody has to remember to update — which matters,
because `jails sync` acts on it.

- `jails sync [--dry-run] [--no-start]` — apply every declared capability that
  is not there yet. A fresh clone becomes the project it claims to be in one
  command, instead of whoever set it up recalling which `add` calls they ran.
  It is also how a project takes a newer jails' output: every capability is
  idempotent and reports what is already there, so a sync over a correct
  project changes nothing and says so. With `--dry-run` it answers "what is
  this project missing?" without writing.

A capability name that jails does not know is an error listing the real ones,
for the same reason a misspelled layer is: `postgress` sitting in the file
would look declared and never sync. The names are the labels `add` uses, not
its aliases — `db`, not `postgres` — so one capability cannot be listed twice
under two spellings.

Both tables are closed sets. This renames layers and declares capabilities and
nothing else — no template overrides, no per-kind paths, no plugin hooks.

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
- `jails generate|g strategy <Name> <Variant...> --on <Type> [--yields <Type>]`
  (alias `rule`) — the open set, and the counterpart to `sealed`: one port
  interface, a bean per implementation, and Spring collecting them into a
  `List<Name>` the caller iterates without knowing what is in it.

  ```
  jails g strategy RewardRule Coffee LargeTransaction --on Transaction --yields Reward
  ```

  writes `RewardRule` (`Optional<Reward> apply(Transaction)`),
  `CoffeeRewardRule` and `LargeTransactionRewardRule` — each `@Component`, each
  with a `@Disabled` test naming what to prove. A variant that already carries
  the interface's name keeps it rather than doubling it. Without `--yields` the
  strategy is a predicate returning `boolean`; with it, an implementation
  declines by returning `Optional.empty()`, which is what lets every
  implementation see every input.

  It earns a generator because the failure is silent: the interface, the
  implementations, the annotation on each and the `List<Name>` constructor
  parameter are four things that have to agree, and an implementation missing
  its `@Component` is simply not in the list — it never runs, and nothing
  reports a problem. `--on`/`--yields` types that aren't in the project yet are
  named at generation time rather than left for the next `mvn`.

  `jails destroy strategy <Name>` reads the implementations back off disk
  rather than rebuilding a variant list it was never given, so it also removes
  ones added by hand — left behind implementing a deleted interface, they stop
  the project compiling.
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

**An `@marker` picks the table constraint.** These change the generated SQL
and nothing about the Java type — a record cannot say that this UUID and that
string are together the primary key.

| marker | in the migration |
| --- | --- |
| `@pk` | part of the primary key; several make it composite, in declaration order |
| `@unique` | `unique` on the column |
| `@index` | its own single-column index |
| `@positive` | `check (col > 0)` — numeric columns only |
| `@nonnegative` | `check (col >= 0)` — numeric columns only |

Repeatable and order-independent (`amount:long@positive@index`), and they
combine with the optionality suffix either way round. An unknown marker is an
error listing the real ones, because a typo that parsed as "no constraint"
would produce a schema quietly missing the primary key you thought you asked
for. Falling back: with no `@pk`, a column named `id` is still the key, and
failing that there is none — jails will not invent a surrogate.

For the index a per-column marker cannot spell — composite, or ordered — pass
`--index` (repeatable) to `g scaffold`:

```
jails g scaffold Reward transactionId:uuid@pk ruleId:string@pk \
    customerId:uuid amount:long@positive createdAt:instant \
    --index 'customer_id, created_at desc'
```

Column names in `--index` are checked against the table before anything is
written; a typo there would otherwise surface at `flyway migrate` on whichever
machine ran it first.

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

Every package jails writes a class into gets a null-marked
`package-info.java` the first time, so `@NullMarked` covers the whole tree
rather than the one package somebody remembered. It is written only when
`org.jspecify:jspecify` is actually a dependency — `new` and `new-cli` add it
— because annotating a package that cannot resolve `@NullMarked` would hand
you a compile error for a file you did not ask for.

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
