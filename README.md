# jails

A small, opinionated scaffolding tool for Spring Boot and plain Maven
projects. Jails favors immutable Java types, explicit ports, visible SQL, and
short commands. It does not generate or depend on an ORM.

## Architecture & Internals

For the compiler design, the crate map and the contributor guide, see
[**`ARCHITECTURE.md`**](ARCHITECTURE.md).

## Build

```
cargo build --workspace && cargo test --workspace && cargo install --path .
```

Installs `jails` into cargo's install root (`~/.cargo/bin` unless
`CARGO_INSTALL_ROOT` says otherwise). Shell completion:
`source <(jails completion bash)`.

## The application compiler

A project has one human-authored desired-state model at `.jails/model.jdl`.
Every `jails g`, `jails add`, `jails entity`, `jails set` and `jails destroy`
is an edit to that model, followed by one compilation of the whole model into
an exact, content-addressed plan that is previewed byte for byte and then
executed. Generated Java is a merge-managed projection written beside your
own sources under `src/`, and `.jails/compiler.lock.json` is what says which
files are jails': on the next generation, disjoint hand edits survive and
overlapping edits refuse before anything is written.

`jails new`, `jails new-cli` and `jails new --app` all seed the model. A project
jails did not create reaches it through `jails model init`, and `jails adopt`
first records the project's layout in `jails.toml`.

**Editing that file by hand is the first path**, and `jails sync` compiles it:
the CLI is sugar that writes the same declarations. A generated model carries
no `@id` -- an identity the compiler derives from the name beside it says
nothing -- so what `jails g scaffold` writes is what you would have written,
and `jails model fmt` is the one command that changes its layout.

```jdl
jdl 1

app Notes {
  pkg com.example.notes
  java 26
  platform spring
  build maven
  storage postgres
}

entity Note {
  use scaffold

  id:    uuid @pk
  title: string @notBlank @length(1..200)

  command CreateNote(title) {
    route POST "/notes"
  }
}
```

```
$ $EDITOR .jails/model.jdl      # add `body: string?` to Note
$ jails sync                    # one plan, previewed, then applied
```

The compiler entry points:

```text
jails model check [--manifest .jails/model.jdl] [--frozen]
jails model plan  [--manifest .jails/model.jdl] [--plan-out plan.json]
jails model apply --plan-in plan.json
jails model eject <boundary>
jails model status
jails model relocate
jails model explain
jails model fmt [--check]
jails sync
```

Every diagnostic these print names the model node it is about
(`$.entities.note.fields.title.type`) and, where the document declared one,
the line it was written on: `at .jails/model.jdl:36:9` on its own line under
the message. One mistake is one diagnostic -- a field whose type is
misspelled suppresses the index, constraint, operation and marker
diagnostics that only fire because it did not link, so the list is the
mistakes rather than their consequences.

`plan` captures the workspace once and writes a content-addressed exact plan.
`apply` verifies and executes those reviewed bytes without recompiling; applying
the same bundle twice converges to zero writes, and a stale precondition fails
before publication. `check --frozen` is the CI assertion that committed managed
output matches the model and compiler version exactly. `sync` compiles the
current model and executes its plan directly. `explain` lists every name the
compiler derived rather than the author writing, with the rule that produced
it. `status` lists every file the accepted projection owns -- the lock's
list, since managed files sit beside yours under `src/` -- and whether each
is byte for byte what jails wrote, carries a hand edit the next generation
merges over, or is missing; every managed file also carries a header line
naming the artifact it was rendered from. `relocate` is the one-time move
for a project a release before this one generated under `.jails/generated`:
every managed file the lock names moves to its `src/` path with its hand
edits, the lock is rewritten, the marked source-root block leaves the build
file, and it refuses by name if any destination already exists.

**Ejection** is the escape hatch for one implementation boundary, named by a
readable path (`Note.repo.fake`, `Note.http.api`, `Audit.implementation`)
that the boundary registry resolves, or by the artifact id generated
provenance reports (art_ent_note_repository_memory). The files stay exactly
where they are, hand edits included: the plan records an `eject` declaration
and takes the boundary out of the accepted projection in the lock, and from
then on jails neither rewrites nor deletes that source. Records, ports and
operation interfaces stay managed ABI.

**Capabilities** are declarations in the model. Each is a pack: Java files with
their own merge identities, the dependencies and properties they need, and one
capability-scoped ejection boundary. Compose services, build features such as
Failsafe or JaCoCo, and whole project files such as a CI workflow are reader-
document facets: the lock records the generated slice, so hand edits inside it
merge and everything around it is byte-preserved.

**Operations** are compiler input. `command`, `query`, `transition` and
`event` declarations lower to typed managed Java ABI; with `db` declared they
lower to `JdbcClient` adapters, and with `api` to Spring controllers with a
companion test that drives a real request. Every implementation is ejectable
independently while its port stays managed.

**Evolution** is typed. `rename entity --strategy preserve-table` changes the
Java projection and keeps the entity ID, table and routes; `entity field
rename|type|nullability|drop` are stable-ID patches with exactly one typed
policy each; `entity index add|remove` and `--storage preserve|drop` append
exactly one forward migration. Rolling and expand/contract campaigns refuse.

**Destroy** is subtraction: it removes the declaration and recompiles. The
linker refuses while an operation still references the target; a stored entity
requires `--storage preserve|drop`; a hand-edited artifact that would disappear
refuses before writes.

## Commands

- `jails about [--json]` (alias: `jails info`) — describes the project: its
  name and root, the Java release, Spring Boot presence and the Maven
  wrapper or command, in five lines. A multi-module build gains the reactor,
  the active module and the modules it declares; a single-module project is
  called a project rather than a reactor with one module in it. It works
  from any directory below a module. `--output json` emits the versioned
  contract used by editor integrations and other tools.
`--output json` is available on every command and carries the same value the
screen does: the status, the file list with its verbs, the declaration a
mutation wrote, and the notes. The reviewed transition itself is what
`--plan-out` writes. The older per-command `--json` still parses for one
release and is no longer advertised. `doctor` and `test` keep their exit
codes under either spelling, so `jails doctor --output json && deploy`
behaves like `jails doctor && deploy`.

Every mutation prints the JDL it wrote above the files that JDL implies, so
`jails g record Money amount:long` shows the `entity Money { … }` declaration
it added to `.jails/model.jdl`. The CLI is sugar over that one editable
source: the next change can be made by hand in the file and applied with
`jails sync`.

- `jails explain <kind|capability>` — what a generator kind or a capability is
  for and the trap it invites. Both vocabularies answer to one argument, and
  no capability is spelled like a kind. `jails explain --flag <name>` does the same for a global flag:
  `--pretend`, `--output`, `--yes`, `--plan-out`, `--timing` and `--debug`
  carry one help line each, and the reason each exists lives here rather
  than on the first screen of `--help`. The generated Javadoc carries the same reasoning for whoever reads
  the file; this is for whoever is deciding whether to generate it.
  `jails g <kind> --help` prints that same entry followed by the flags that
  kind accepts, read off the tables the frontends refuse by, so it names
  only flags that kind can honour. The shared `jails g --help` is the union
  of every kind's vocabulary and stays available.
- `jails model jdl` — the language this binary accepts: the declaration
  families and the `@attributes` each takes, the field types and their
  aliases, the `use` projections, and the `cap` kinds a source may spell.
  Every row is walked out of the registries the parser refuses against, so
  it cannot describe a language the binary does not have.
- `jails commands [--json]` — every subcommand, generator kind, capability and
  flag jails accepts, derived from the same definition that parses the
  arguments, so it cannot drift from the binary. `--json` is what the Neovim
  plugin reads instead of keeping its own completion tables.
- `jails editor <handshake|complete|symbols|diagnostics> --output json` — the
  versioned, read-only protocol `jails.nvim` speaks, and any other editor
  adapter can. `handshake` negotiates the protocol version and reports the
  project root, its build and its release; `complete` finishes one
  already-tokenized argument at a byte offset, from the clap tree for the
  closed sets every project shares and from the model for the ones it does
  not -- the entities `--on`, `--via` and `--yields` can name, the components
  a field list can filter on, the types after a colon and the `@markers`
  after those. A project with no model, or one mid-edit that does not parse,
  offers nothing rather than a diagnostic; `symbols routes|beans|tests|types
  [query]` returns project symbols with stable identities (a route's is
  `route:<METHOD>:<path>:<handler>`, which `jails request` accepts as a
  target); `diagnostics --scope project|buffer [--file <path>]` returns
  structured diagnostics, each tagged with the evidence it rests on --
  including the language's own, from the same parse and link `model check`
  runs, with the code, the model path as `subject` and a zero-based range in
  `.jails/model.jdl`. It refuses without `--output json`, because an adapter
  must never parse human output.
- `jails new <name> [--deps web,jdbc] [--java 26] [--no-git] [--no-devtools]`
  — new Spring Boot project via start.spring.io. `git init` + `.gitignore`
  and `spring-boot-devtools` (needed for `run --watch`) are on by default.
  It creates `./<name>` and refuses to overwrite an existing directory. Java
  defaults to Java 26. Any supported Java 21+ release can be selected
  explicitly. When Initializr only accepts an earlier bootstrap release,
  Jails retargets the generated Maven project to the requested release.
- `jails new <name> --gradle [--boot 2.7.18] [--gradle-version 8.5]
  [--jar-name <name>] [--jar-version <version>]` — the same, as a Groovy
  Gradle build. jails writes every file itself here and never contacts
  start.spring.io, which is what lets `--boot` name a version Initializr no
  longer serves and what makes `--pretend` honest on this path. A 2.x
  `--boot` gets the `buildscript {}` build file, the only shape that applies
  the Boot 2 Gradle plugin; anything later gets `plugins {}` and Gradle's
  native bom support. The Gradle distribution defaults from the Boot version
  rather than to one number — Boot 4's plugin throws below Gradle 8.14, and
  Boot 2.7 does not run on 9.x. The four Gradle flags are **refused, not
  ignored**, without `--gradle`.

  `gradlew`, `gradlew.bat` and `gradle-wrapper.properties` are written from
  templates; `gradle-wrapper.jar` is a binary with no standalone published
  coordinate, so it is fetched from Gradle's own repository at the matching
  tag. If it cannot be had, **none** of the three is written and jails says
  so: `run` falls back to `gradle` on PATH when there is no `gradlew`, so no
  wrapper is a working project and a wrapper missing its jar is not.
- `jails new-cli <name> [--release 26] [--no-git]` — new plain Maven CLI
  project (hand-written `pom.xml`, `App.java`, `AppTest.java`), no network
  required. `App.java` is a working command dispatcher, not a Hello World
  stub, so `generate command` has something to register into from the start.
- `jails generate|g scaffold <Name> [field:type ...]` — a REST entity that
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
  It also writes `requests/<name>.http`, a collection an editor can send, and
  **the generated controller test sends the same body on every build** — one
  builder, two readers, because a collection describing a request the record
  refuses is a request nobody can make. The collection documents only what the
  controller answers: a resource with an `@scope` field is create-only, since
  every read has to carry the tenant and is therefore a `jails g query`.
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
- `jails generate|g controller <Name> [--method <get|post|put|patch|delete>]
  [--on <Type>] [--returns <Type>]` — one route, in the shape you say. The
  default is `GET` returning the entity name, which is a route that works and
  a test that runs. `--returns <Type>` makes it the response type and `--on
  <Type>` the `@RequestBody`, importing each from the domain layer.

  A verb with no body — `get`, `delete` — never gets a `@RequestBody`
  parameter: a body there is not forbidden by HTTP and is dropped somewhere
  between the caller and the handler, so the parameter would silently never
  bind.

  When either type is named, the handler is a `todo` that throws and **the
  test is emitted whole and `@Disabled`**, naming what to implement. Same rule
  as `sample_value`: jails has no type model, so it cannot build a
  `Verification` to return or one to post. A guessed body would not compile,
  and emitting no test would silently drop the coverage.
- `jails generate|g class <Name>` — a plain `public final class` and its
  companion test, both in the **base package** rather than a
  `domain`/`service` subpackage: "a class" says nothing about which layer owns
  it. No Spring and no fields — the kind to reach for when what you want
  is ordinary Java: an algorithm, a ring buffer, a parser. The generated test
  constructs the class, so it compiles the moment it is written and stops
  compiling the day you add a real constructor, which is the prompt to write
  the real assertion.
- `jails generate|g command <Name> [--on <Dispatcher>]` — a CLI subcommand for `new-cli`
  projects, registered in the project's dispatcher automatically: `run(PrintStream, PrintStream, String...)` returning an exit
  code, with a `NAME` constant to dispatch on. Output streams are arguments
  and nothing calls `System.exit`, so the companion test drives the whole
  command in-process. jails splices one line into the dispatcher's
  `commands()`. A project can easily have two — `new-cli` writes `App.java`
  and `g cli <Name>` writes another — so `--on <Dispatcher>` names the one
  this command belongs to; without it, an ambiguous project gets a note
  listing the candidates and the Javadoc's instructions as the fallback.
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
  than a global `spring.factories` registration, so that pure slices and
  `@WebMvcTest`s do not each start a PostgreSQL they never query. JDBC would also
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
- `jails add|a <csv|sqlite|json|testkit|fake|http|format> [--name <Base>] [--pretend]` — grows an
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
`remove` shows you every deletion before it makes one, and takes no for an
answer. "It exists" is not ownership: a `CsvReader` you spent an afternoon on
looks exactly like the stub jails generated. It does not refuse — `remove` is
the documented inverse of `add` — but it will not delete your work without
showing you the list first, and `--yes` is how you say yes in advance. That
holds for `--output json` too: an encoding with nobody to ask refuses rather
than proceeding.

**A capability's settings in `application.properties` are owned one key at a
time.** There are no `# jails:<capability>` markers around them: jails records
which keys it wrote, and `remove` takes back exactly those. Anything you added
beside them is yours and stays. The comment jails writes above a key it
introduces goes with that key — unless you have edited it, in which case it is
your prose and it stays too. (`compose.yaml` still uses marked blocks, because
there the unit is a whole service block rather than a setting.)

- `jails remove|rm <capability>... [--yes]` — the inverse of `add`: unsplices
  the same dependencies, deletes the same files, removes compose services, and
  stops their containers. Confirms unless `--yes`.
- `jails add fast-test` / `jails remove fast-test` — put JUnit's console
  launcher on the test classpath, or take it back off. It is an ordinary
  capability, because a command that reports test results must not edit your
  build file: `jails test --fast` used to install it as a side effect of how
  the tests were run, and it now names this instead.
- `jails start [db|kafka]...` — `docker compose up -d` for the named services,
  or everything in `compose.yaml` when invoked with no arguments.
- `jails stop [db|kafka]...` — stop those containers (`db` is the postgres
  service). Does not delete `compose.yaml`.
- `jails logs [services...] [--follow] [--since <when>] [--tail <n>]` — bounded
  logs from the compose services `compose.yaml` declares, defaulting to every
  one of them and to the last 200 lines. Bounded by default because the case
  this exists for is reading what a service said while it failed to start, and
  an unbounded dump of a container that has been up for a week buries it.
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
- `jails generate|g client <Name> [--method <verb>] [--on <Request>]
  [--returns <Response>] [--path <path>]` — a declarative HTTP client: an
  `@HttpExchange` interface, an `@ImportHttpServices` registration, and a test
  that drives it against a real socket on an ephemeral port. No base URL in
  the code — the group's URL comes from
  `spring.http.serviceclient.<group>.base-url`. Splices
  `spring-boot-starter-restclient`, without which the proxies are built but no
  base URL is ever applied (the failure reads "URI with undefined scheme" and
  says nothing about a missing dependency).

  **Name a verb, a body or a return type and you get that call**, not a REST
  collection: `--method post --on ChatRequest --returns ChatReply --path
  /v1/chat/completions` generates one `@PostExchange` method taking and
  returning those types. Naming none of the three keeps the collection shape.
  All three are applied or refused, never accepted and discarded: reporting
  success for work not done is the failure class jails is scrupulous about.

  **Each client gets its own registration.** `@ImportHttpServices` carries one
  group name, so a single shared config scanned by package meant a second
  client rewrote it and every earlier client lost its configuration —
  silently at generate time, and visibly only as the older client's own test
  calling `https://example.invalid`. One `<Name>ClientConfig` per client,
  listed by type, is additive by construction.
- `jails generate|g fetcher <Name>` — a bounded outbound byte-fetch port and
  Apache HttpClient adapter. Every HTTP redirect is revalidated, requests stay
  on the exact original host, HTTPS cannot downgrade, private/reserved DNS
  answers are rejected, and the connection is pinned to the addresses that
  passed validation to close the DNS-rebinding window. Timeouts, response
  size, redirect count, user agent, and allowed media types are properties;
  metrics and adversarial real-socket tests are generated with it. Ordinary
  calls reject non-2xx responses; protocol-aware callers may explicitly
  accept selected statuses (for example robots.txt 404/410) without bypassing
  any redirect, DNS, byte, media-type, or timeout check.
- `jails generate|g http-workflow <Name> --on <Fetcher>` (alias `hflow`) — a
  persistent, bounded HTML traversal composed over an existing generated
  fetcher. It generates a PostgreSQL frontier and run/page ledger, leases with
  expired-work reclaim, canonical exact-origin link discovery, robots policy,
  retry classification, hard page/depth limits, cancellation, status/page
  APIs, metrics, and a real integration test. The name and fetcher are generic;
  no crawler application type exists in Jails core.
- `jails generate|g job <Name>` — a `@Scheduled` component whose interval is a
  property, not a constant, and which catches its own failures: an exception
  escaping a scheduled method cancels every future run, silently.
- `jails generate|g durable-job <Name> <field:type...> --on <Usecase>
  --yields <Resource>` (alias `djob`) — PostgreSQL-backed work with atomic
  claim, expiring leases, `SKIP LOCKED`, bounded exponential retry, observable
  terminal failure, and payload idempotency. The payload must exactly match an
  existing generated command and carry the entity's stable UUID identity;
  replay after a crash between the business commit and queue acknowledgement
  observes that identity before repeating the effect.
- `jails generate|g http-sink <Name> --on <Usecase> --yields <Event>` (alias
  `webhook`) — adds a conditional HTTP destination to the typed transactional
  outbox already generated by that use case. The stable event id becomes the
  `Idempotency-Key`; redirects are disabled, only 2xx acknowledges delivery,
  bearer credentials stay in configuration, timeouts are finite, and provider
  rejection/loopback contracts are generated. Configure
  `outbox.<usecase-kebab>.http.<name-kebab>.url`; optional keys are
  `.bearer-token`, `.connect-timeout-ms`, and `.request-timeout-ms`. Every sink
  is at-least-once, but a retry no longer re-sends to the sinks that already
  accepted the event: each acceptance is recorded on the outbox row before the
  next sink is tried, so a failing HTTP delivery does not republish to Kafka on
  every attempt.
- `jails generate|g idempotency <Name>` (alias `idempotent`) — at-most-once
  execution with a **retained result**: a scoped receipt record, a store port,
  a PostgreSQL adapter, a guard and its tests, plus the migration. Needs
  `jails add db`.

  A `@unique` column on the key already gives one row per key. What it does not
  give is the retained result, so a retry finds the row, fails the insert, and
  is answered 409 Conflict — telling a caller that never saw the first response
  that the work happened while still withholding what happened. The guard has
  four outcomes instead: run it, replay the stored response to a matching
  retry, refuse the same key carrying a *different* request, and tell a retry
  that arrives while the first attempt is in flight to come back. The claim is
  a single `insert … on conflict do nothing returning`, because select-then-
  insert leaves the race the whole mechanism exists to close.
- `jails generate|g usecase <Name> <field:type...> --on <Resource>
  [--yields <Event>] [--on-conflict <component>]` (alias `uc`) — an executable create operation over an
  existing scaffold: a typed command, an application port, a transactional
  implementation that fills in what it can infer (ids, timestamps, status
  defaults, counters, flags, empty optionals) and refuses what it cannot, an
  HTTP adapter, and tests. With `--yields <Event>` it also generates a
  transactional outbox: the business row and the typed event commit together,
  a leased relay delivers to every configured sink, and PostgreSQL tests prove
  bounded retry and inspectable terminal failure. An event component named
  `<Resource>Id` is the identity of the row the use case just created; the
  event's own `id` is minted, so two events about one entity are two rows
  rather than one silently discarded as a duplicate.

  **`--on-conflict <component>` makes the create a get-or-create.** The
  generated implementation is `Ensuring<Name>UseCase`, a `JdbcClient` adapter
  rather than the repository-backed `Storing<Name>UseCase`: one
  `insert … on conflict (…) do nothing returning`, then a read of the row that
  was already there. That is deliberate — a port with a `save(T)` cannot
  express the clause, and read-then-insert reopens the window where two callers
  both see nothing and both proceed. The component must be one the command
  carries, or every call would invent a new key and nothing would ever
  conflict. Whether its column is actually unique is *not* checked at
  generation time — a record read off disk carries no constraints — so the
  generated `IT` checks it against a real database instead, where it is a fact
  rather than a claim. It cannot be combined with `--yields`, since the outbox
  delegates to the class this replaces. The relay drains in
  batches (`outbox.<usecase-kebab>.batch-size`, default 100) rather than
  moving one row per tick, and its retry interval carries jitter.
- `jails generate|g query <Name> <field:type...> --on <Resource> [--via
  <Parent>]` — a typed read: a query record, a port, a JDBC adapter and an HTTP
  adapter, with every declared field an equality filter.

  **`--via <Parent>` reads a second table**, so a filter may name a component
  the target does not own — `jails g query UnreadForEmail email:string!
  isRead:boolean --on Message --via User` filters messages by the *user's*
  email. The join column is derived from the two records: `<parent>Id` when the
  child declares it, otherwise the single component of the parent key's type
  whose name ends in `Id`; two candidates is a refusal naming both, never a
  choice. `--via` names the parent **type**, not an association — an
  association records its mapping only in the migration it wrote, and jails
  does not re-read generated SQL to recover a decision. A joined select
  qualifies every column, including the target's own.

  **`--path` names the route**, for `controller`, `usecase` and `query`. A
  derived path is a virtue greenfield — one shape, and every generated surface
  agrees about it — and unusable when the URLs are a fixed external contract,
  which is what porting a service or writing a server against an existing
  frontend means. `--path /customer_api/ping` is recorded on the entity, so
  `destroy` and a re-plan both know it; it is validated rather than passed
  through, because it is text jails writes into an annotation.

  **`--order-by` and `--limit` say out loud what the adapter would otherwise
  decide silently.**
  `--order-by 'sentAt desc, id'` names components of `--on` (or the columns
  they map to), each optionally `asc`/`desc` and nothing else — arbitrary SQL
  is refused here rather than recorded as trusted generated SQL, the same rule
  `--index` follows. Omit it and the order is newest first with the key as the
  tiebreak. `--limit` replaces the built-in ceiling of 100; `--limit 0` is
  refused, since it can only ever return nothing. Results have stable key ordering and a
  hard row ceiling; the adapter's SQL comes from the same column model as the
  table's DDL.
- `jails generate|g cases <path/to/file.md>` — one `@Test` per bullet in a
  markdown file, as a class-level `@Disabled` todo list the build can read.
  Every case throws rather than passing vacuously: delete one `@Disabled`,
  make that case pass, move to the next. Note the NAME here is the markdown
  path, not a class name.
- `jails generate|g transition <Name> <field:type...> --on <Resource>` — an
  atomic PostgreSQL compare-and-swap for state changes. `id`, fields marked
  `@scope`, and the required numeric `version` match the row; every remaining
  field is updated and the version increments in the same statement. Missing
  or cross-scope rows become 404, stale versions become 409, and generated
  real-database tests prove a stale retry cannot mutate twice.
- `jails generate|g association <Name> childField=parentField... --on <Child>
  --to <Parent>` (alias `fk`) — an explicit persisted relationship between
  existing scaffolds. It validates field/type/order compatibility, emits the
  target composite uniqueness and ordered PostgreSQL foreign key, defers the
  integrity check until commit so atomic units of work can write either order,
  and generates tests for exact schema shape plus impossible cross-boundary
  historical data.
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
- `jails g search <Name> <component>...` (alias `fts`) — PostgreSQL full-text
  search over a record that already exists. The `tsvector` is a **generated
  column**, not a trigger, and that is the whole kind: a trigger has one silent
  failure — somebody adds an UPDATE path that does not fire it, the row's text
  changes, the vector does not, and the row silently stops matching a search
  that should find it. `generated always as (…) stored` cannot drift,
  because PostgreSQL maintains it. Every column is wrapped in `coalesce(x, '')`
  (`||` with a NULL operand yields NULL, which would blank the whole vector),
  the text search configuration is named in the expression rather than left to
  a session setting, and the adapter uses `websearch_to_tsquery` — the syntax
  in which unformatted text is a valid query, where `to_tsquery` throws a
  syntax error on a bare two-word phrase. You name the components to index:
  indexing every text column would index ids and status codes as prose. Needs
  `jails add db`.
- `jails g webhook <Name>` (alias `hook`) — an inbound webhook endpoint you
  can believe. Three details, and each is a way this is normally got wrong.
  **The signature is over the raw bytes**: two JSON documents can mean the same
  thing and hash differently (key order, whitespace, `1.0` against `1`), so a
  verifier that binds the body and re-serialises to check rejects good
  deliveries — intermittently, depending on the sender's formatting. The
  controller takes `@RequestBody byte[]`, which reads like a shortcut and is the
  whole design. **The comparison is `MessageDigest.isEqual`**, because
  `Arrays.equals` returns at the first differing byte and how long a rejection
  takes then says how much of the signature was right. **The timestamp is
  checked in both directions and is inside the signature** — five minutes,
  Stripe's tolerance; rejecting only stale timestamps leaves a far-future one
  accepted, and leaving the timestamp out of the signed bytes makes it a header
  anyone in the middle can rewrite. The endpoint answers 200 before doing the
  work, because senders retry on anything else and time out in seconds. The
  outbound half is `g http-sink` (whose `webhook` alias is now `outbound`).
- `jails g auth <Name>` (alias `jwt`) — this service issuing its own tokens,
  and the default that has to be undone. **Spring Boot auto-configures no
  `JwtEncoder`** — there is not one occurrence of the type in the whole of
  Boot; the resource-server starter gives you a decoder for *someone else's*
  tokens and stops. And **a token with no `exp` passes the default decoder**:
  `JwtTimestampValidator` ships `allowEmptyExpiryClaim = true`, so every
  out-of-the-box configuration accepts a token that never expires, and nothing
  warns. One line closes it and one generated test keeps it closed — delete the
  line and no other test notices. The key is symmetric and read from
  configuration; two services that verify each other's tokens want a key pair
  and a published JWK set, never one shared secret. Needs `jails add security`.
- `jails add|a mail` (alias `smtp`) — sending, a Mailpit compose service, and
  an integration test that **reads the message back over POP3**. That last part
  is the point: a mail test that checks `send()` did not throw proves almost
  nothing, since a wrong From, a wrong recipient, an empty subject and a message
  the server silently drops all pass it. The shape is copied from Spring Boot's
  own `MailSenderAutoConfigurationIntegrationTests`. Two defaults are made
  explicit: `spring.mail.host` is set, because unset it falls back to
  `localhost:25` and a misconfigured deployment then fails at the first send
  rather than at startup; and the From address is one configured value, because
  a per-call-site literal drifts and the one that drifts is the one a receiving
  server rejects for failing SPF. There is no `@ServiceConnection` for mail in
  Boot 4, so the test binds host and port with `@DynamicPropertySource` — which
  is why it does not look like `add db`'s.
- `jails add|a sse` (alias `events`) — Server-Sent Events, and the five details
  this design gets wrong. The emitter timeout is `-1L`, not `Long.MAX_VALUE`:
  it reaches `AsyncContext.setTimeout`, where the Servlet spec reads zero or
  less as "no timeout" and `Long.MAX_VALUE` is a real one a container may
  reject. `onCompletion` alone covers the clean close, the timeout and the
  broken pipe — but it runs on a container thread while a broadcast is in
  flight, which is why the registry is a `ConcurrentHashMap` of `newKeySet()`
  and why subscribe and unsubscribe both go through one `compute` on the same
  bin lock. The heartbeat needs `spring.task.scheduling.pool.size` raised,
  because it defaults to **1** and one heartbeat blocking on a dead client
  stalls every other `@Scheduled` job in the application. No event `id()` is
  emitted, because Spring implements no `Last-Event-ID` replay and an id would
  advertise resumability that does not exist. And `unsubscribe` is public
  because `complete()` on an emitter no request is holding fires no callbacks
  at all — it forwards to a handler the container installs. Topic-agnostic, the
  same line `add kafka` draws.
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
  reaction is a blanket `permitAll()` nobody revisits. The generated local
  profile has explicit BCrypt development credentials; `prod` is a JWT
  resource server and cannot fall back to that user. Both chains are
  default-deny, permit only `/actuator/health/**`, and are stateless with CSRF
  off (safe *only* together: CSRF protects ambient credentials, and a chain
  that issues no session cookie has none). A generated `ScopeAuthorizer`
  enforces same-named JWT claims for fields marked `@scope` and reports
  mismatches as 404 to prevent cross-tenant enumeration.
- `jails add|a docker` (alias `image`) — a multi-stage OCI `Dockerfile`, narrow
  build context, Java release derived from the project POM, and an image CI
  check that asserts the numeric non-root runtime user.
- `jails add|a ci` — a least-privilege GitHub Actions `clean verify` gate with
  timeouts, concurrency cancellation, Maven caching, and immutable action
  commit pins.
- `jails add|a k8s` — a Helm chart under `deploy/chart` (`Chart.yaml`,
  `values.yaml`, deployment, service, configmap and a `PrometheusRule`). The
  management port is separate from the serving one, so liveness and readiness
  probes are reachable without exposing actuator to traffic, and the rule ships
  SLO burn-rate alerts over the metrics `add observability` registers. It
  **refuses by name** rather than guessing: it needs Spring, plus `actuator`,
  `observability` and `docker`, and says which one is missing and the command
  that installs it. A chart that deploys an image the project does not build is
  worse than no chart.
- `jails add|a h2` — an in-process database with the browser console wired up.
  Generated DDL switches dialect with it: the driver decides, and the one type
  name that differs is `timestamptz`, which H2 knows only inside its PostgreSQL
  wire-protocol server and rejects in a `create table`.
  File-backed for the application (inside the project, not `~`, so two
  checkouts never share one file) and **in-memory for the tests**, through
  `src/test/resources/config/application.properties`. Two details a hand-edited
  pom gets wrong: `H2ConsoleAutoConfiguration` lives in the separate
  `spring-boot-h2console` module in Boot 4, so without it
  `spring.h2.console.enabled=true` is a property with nothing listening to it;
  and a suite that inherits the file URL writes into the working tree and fails
  on H2's file lock the moment it runs while the application is up. The
  generated test asserts both the connection and the URL, because only the
  second can catch the overlay silently not being read.
- `jails add dependency <group>:<artifact> [--version <v>] [--scope
  compile|runtime|test]` — the escape hatch for a library jails has no
  capability for. It splices the dependency and does nothing else: no wiring,
  no test, no `jails.toml` entry, because jails does not know what the library
  is for. `jails remove dependency <group>:<artifact>` is the exact inverse.

  Omit `--version` when the project's parent or an imported BOM manages it.
  Maven refuses to read a pom whose dependency has no version and nothing
  manages it — every goal fails, `validate` included — so jails asks rather
  than guessing.

  The point of routing this through jails rather than an editor is that the
  splice is then *owned*: `remove` takes exactly it back out, and a later
  capability wanting the same artifact collides visibly.
- `jails set <key>=<value> [--tests]` / `jails unset <key> [--tests]` — one
  setting in `application.properties`, as an owned resource. Same reason: jails
  knows which keys it wrote, so `remove` and `sync` keep working and two owners
  of one key are a collision rather than a silent last-wins.

  In a canonical project, each `(main|test, key)` is a stable model node.
  Changing the value retains its identity; the compiler reconciles the whole
  target set while preserving comments and unrelated keys byte-for-byte. A
  reader-owned declaration of the same key is refused before any write.

  `--tests` writes `src/test/resources/config/application.properties` instead
  — that path and not the obvious one, because `classpath:/config/` outranks
  `classpath:/` **and is additive**, so one key there overrides one key here.
  `src/test/resources/application.properties` shadows the main file wholesale
  and silently unsets everything the tests did not restate. This is how a
  project gets a test-only datasource without the suite writing to whatever
  the application's own URL points at.
- `jails generate|g socket <Name>` (aliases `websocket`, `ws`) — the
  client→server half of a chat, which `add sse` does not cover: a
  `TextWebSocketHandler`, its `WebSocketConfigurer` registration at
  `/ws/<name>`, the `spring-boot-starter-websocket` dependency, and a test.
  Three decisions it makes and states: every session is wrapped in
  `ConcurrentWebSocketSessionDecorator`, because a `WebSocketSession` is not
  safe for concurrent sends and a broadcast is exactly that shape (the failure
  is `IllegalStateException: … [TEXT_PARTIAL_WRITING]`, load-dependent, and
  never reproducible at the desk); a session that throws `IOException` is
  evicted rather than retried forever or allowed to stop the broadcast; and the
  handshake stays same-origin, with the registration saying where to widen it
  rather than widening it, because that is a security decision.
- `jails generate|g presence <Name>` — who is connected, in PostgreSQL rather
  than in one process's memory. An in-memory presence map is silently correct
  on one node and silently wrong on two: it does not throw and it does not
  warn, it answers a question about the cluster from one process. A row per
  `(scope, member, node)`, because a member connected twice is present until
  both claims are gone, and a `seen_at` window rather than a leave-only
  protocol, because a process that dies never sends `leave`. Scope and member
  are strings the caller picks — jails does not know what is present in what.
  The generated `IT` is the point: two adapters are two nodes, one joins and
  the other is asked. Needs `jails add db`.
- `jails generate|g seed <Entity>` — development data for an entity that
  already exists: `src/main/resources/db/seeds/<table>.json` with one sample
  row built from the record's own components, and a `@Profile("seed")`
  `ApplicationRunner` that loads it **through the repository port**, never SQL
  — a seeder that inserts rows itself is the one dataset in a project the
  record's constructor never sees. It loads into an empty table only: an
  edited seed row cannot be told from a change somebody made in the database.
  The companion test binds the shipped file to the record on every build,
  because nothing else reads it until somebody starts under the profile. Needs
  `jails add db` and `jails add json`.
- `jails generate|g event <Name> [--on <Entity>]` — a Kafka slice: the payload
  record, a publisher, a listener that deliberately does not catch (swallowing
  commits an offset for a message never processed), and an `IT` that publishes
  through a real broker via Testcontainers and waits on a latch. Field
  declarations use the same typed model as the other generators (for example,
  `id:uuid crawlRunId:uuid url:uri occurredAt:instant`); a typed event requires
  a non-optional `id`, and the generated samples and assertions derive from
  those fields.

  **`--on <Entity>` is what makes the topic ordered.** Kafka guarantees order
  within a partition and nothing across partitions, so the key is the whole
  guarantee — and the event's own `id` is unique per record, which spreads
  every event about one entity across every partition. `--on Message` keys on
  the payload's `messageId` component instead (required, and refused if it is
  missing or optional), which is the same `<entity>Id` convention `usecase
  --yields`, `association` and `durable-job` already read. Without `--on` the
  key stays the event id and the generated Javadoc says so, rather than
  claiming an ordering the code does not have. With no fields, the `String id, Instant
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
- `jails setup` — the one machine-level step: permit Testcontainers to reuse
  containers between test runs by writing `testcontainers.reuse.enable=true`
  into `~/.testcontainers.properties`. It is the largest saving available to
  a suite that starts PostgreSQL, and it cannot live in the project: the flag
  is read from the home file or the environment, and a copy on the classpath
  does nothing. An existing setting, including an explicit `false`, is left
  alone; `jails doctor` reports which it is. `--pretend` prints what it would
  add.
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
- `jails contract emit [--format openapi|json-schema] [--out <path>]` and
  `jails contract check --against <file|git-rev> [--scope source|declared]`
  — a portable HTTP contract projected from the routes the source declares:
  an OpenAPI 3.1 document, or a JSON Schema enumerating `METHOD /path`,
  marked `source-observed` because it is read off the controllers and never
  off a running app. `check` compares the current projection with a
  committed document, or with the one at a git revision, and exits non-zero
  naming every route the baseline had that the source no longer does — the
  CI step for "did this change break a client". `--out` writes the document
  into the project and honours `--pretend`.
- `jails request <METHOD> <target> --base-url <origin> [--param k=v]
  [--query k=v] [--header k=v] [--header-env k=VAR] [--json <body>|--data
  <body>] [--timeout <s>] [--follow] [--print]` — resolve a route and hand
  the request to `curl`. The target is an origin-relative `/path`, a handler
  (`TicketController#show`), or the `route:<METHOD>:<path>:<handler>`
  identity `jails routes` prints; path parameters come from `--param`.
  Headers travel in a private curl config file rather than on argv, and
  `--header-env Authorization=TOKEN` reads the value out of the environment,
  so a secret never appears in `ps` or in `--debug` output. `--print` shows
  the exact, redacted `curl` line and runs nothing. `--profile` names an
  origin the manifest declares and refuses by name when none is.
- `jails beans [pattern] [--json]` — every `@Component`/`@Service`/`@Repository`/
  `@Controller`/`@Configuration` and every `@Bean` method, with each
  constructor dependency marked resolvable or not. A dependency naming a type
  this project declares but never registers is the static half of "required a
  bean of type … that could not be found", caught before the context starts.
- `jails rename <Old> <New> [--pretend] [--yes]` — rename a type, its
  `Test`/`Tests`/`IT` companions, and every reference. Textual, and honest
  about it: it matches whole identifiers only (`Reward` never matches inside
  `RewardHistory`) and leaves string literals alone, reporting how many
  mentions it skipped. Neovim's `grn` (jdt.ls) is scope-aware and better where
  it works — this is for when the language server is not attached or the
  project does not currently compile.
- `jails rename entity <Current> <New> --strategy
  preserve-table|single-cutover` — coordinate the declaration, the generated
  Java, the table binding, the migration history, and owned SQL literals in
  one reviewed plan. `preserve-table` is the safe shape: it changes the
  logical Java name while retaining the physical table and external route.
  `single-cutover` appends one forward PostgreSQL migration and refuses
  reader-owned SQL, opaque routines/views/triggers, or unowned storage-object
  names. `--strategy rolling` is refused by name: a rolling or
  expand/contract rename is a *campaign* of ordinary plans run as the readers
  are ready, and the tool will not own the waiting between them.
- `jails adopt entity <Name>` — register a type you wrote in the model, so
  `entity field`, `rename entity` and `destroy` work on it. Reads the
  record's components off `src/main/java`, maps each Java type through the
  field-type table or refuses by component, and marks the record yours with
  `eject <Name>.record @adopted`; your file is a plan input, never an output.
  See *A codebase jails did not create*.
- Every mutating command accepts `--pretend --plan-out <file>`. The named plan
  is atomically written mode 0600 outside the project transaction and contains
  the exact prepared bytes plus root, generation, protocol, toolchain,
  preimage, and content-digest bindings. Apply it from the same command path
  without repeating semantic arguments, for example
  `jails generate scaffold --plan-in /tmp/reviewed-plan.json`. Import verifies
  the plan before taking the project lock, never reparses the original intent,
  and then goes through the one executor every other command does.
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
  in the image. The topic defaults to the one the source declares — the
  `@KafkaListener(topics = …)` `jails g event` writes, or a `TOPIC` constant,
  both read textually so they answer on a project that does not compile. A
  `${key:default}` placeholder resolves against `application.properties` and
  falls back to the default; a placeholder with neither is refused rather than
  guessed at, because an invented topic is a `tail` that reads an empty one and
  reports that nothing arrived. The group defaults to
  `spring.kafka.consumer.group-id`. `send` publishes one JSON
  record with a key — ordering is per partition and a null key round-robins;
  `poison` publishes an unparseable one so you can watch it reach the DLT
  rather than stall the partition; `dlt` tails the dead-letter topic with
  headers, which is where the failure reason is; `lag` is the one number that
  says whether a consumer is keeping up.
- `jails console|c [--no-build] [-- <args>...]` — `jshell` with the project's
  compiled classes and Maven runtime classpath. This is not a Spring-booted
  REPL (Java has no `rails console`); it is a JDK shell that can see your
  types. `--no-build` skips `mvn compile`.
- `jails runner --file <script.jsh|-> [--profile <p>] [--main <Class>]
  [--web none|random|configured] [--compile] [--yes]` — `rails runner`: a
  project-relative JShell script, or stdin with `-`, run non-interactively
  inside a booted Spring context. jails writes a private startup script that
  boots the project's `main` (or `--main`) with the given profiles and web
  mode, appends the shutdown to the script, and runs `jshell` over the Maven
  runtime classpath; a failed snippet fails the command. A boot outside the
  `dev` and `test` profiles, or one that binds the configured port, prints a
  preflight first and needs a terminal's confirmation or `--yes`. Absolute
  paths and `..` are refused: the script is trusted code and lives in the
  project.
- `jails destroy|d <type> <Name> [--yes]` — deletes exactly what the
  matching `generate` call would have created.
- `jails test [filter] [--failed] [--fail-fast] [--slowest N]` — uses `./mvnw`
  when present. The filter takes four shapes: a bare `Money` becomes
  `MoneyTest`; a name ending in `IT` runs through Failsafe and `verify`;
  `Money#converts` runs one method (the suffix is applied to the class half
  only, and the Surefire/Failsafe choice is made on the class, so
  `PayoutIT#settles` is still Failsafe's); and
  `src/test/java/.../PayoutTest.java:42` resolves the `@Test` enclosing that
  line — JUnit has no file-and-line selector, so jails does it, which is what
  an editor keybinding needs. A nested class is addressed the way JUnit
  addresses it, `Outer$Nested#method`.
  **`--failed`** rereads `target/{surefire,failsafe}-reports` and reruns
  exactly what failed — a skipped test is not a failure, so it will not drag
  every `@Disabled` test back in. **`--fail-fast`** stops at the first failing
  class. **`--slowest N`** prints the slowest N from the same reports (Maven
  already timed them; a number jails measured would include its own startup).
  On a failure it prints the line that reruns just what broke.
  A filter matching nothing is "no tests ran", not a stack trace.

  **`--fast`** skips Maven entirely and runs the already-compiled classes
  through JUnit's console launcher. Measured here: 2.2 s → 0.6 s against plain
  `mvn`, and **no faster than the `mvnd` daemon jails already prefers**, whose
  0.6 s it merely matches. Use it when mvnd is not available or not working; do
  not expect it to beat the default. Whenever a source file is newer than the
  compiled classes — or nothing is compiled, or you asked for `--json`,
  `--slowest` or `--fail-fast`, which read Surefire's XML — it says why, names
  the source that is newer, and runs the full Maven path instead. Running stale
  classes silently would be green over code that no longer exists, which is the
  one outcome worse than being slow. `jails check` is always `mvn clean verify`
  and is not affected.

  The launcher itself is a dependency in your POM, and `jails add fast-test`
  is what installs it. `jails test` never writes: a run that would reach the
  launcher without it refuses and names that command, while a `--fast` run on
  a project with nothing compiled falls back to the build tool and needs no
  launcher at all. A dependency that appeared because of *how* somebody ran
  their tests is exactly what this avoids.
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
  already-running app — no manual restarts. In an editor with a Java language
  server you do not need it; see [the save-and-reload loop](#the-save-and-reload-loop).
- `jails testd [filter] [--stop] [--status]` — the same tests against a
  **resident JVM**, started on demand and reused. Measured on a scaffolded
  single-entity Spring project on sixteen cores, warm daemon: 0.06–0.10 s to
  run one `NoteTest` method against 0.62 s for `test --fast` or `mvnd`, and
  0.27 s against 0.96 s for the 151-class suite of the `minicom` example
  under `examples/`. The reason is not the launcher — it is
  that the *first* JUnit session in a JVM costs 464 ms where warm ones cost
  20 ms, and a cold `java` pays that every single run.

  It **runs what is compiled and never compiles**: a source newer than its
  class is refused, naming `jails test`. That is not a limitation so much as a
  division of labour — your editor's Java language server is already writing
  `target/classes` on every save (see [the save-and-reload
  loop](#the-save-and-reload-loop)), so between the two the loop is save, run,
  read, in about a tenth of a second.

  `--affected` runs only the tests reachable from what git reports changed in
  the working tree, using a reverse-dependency index built from the constant
  pools already sitting in `target/` — no configuration, nothing to keep in
  step. Reachability is transitive, so a change to a domain record selects the
  controller test three hops away. **Every unknown widens it**: no git, a
  source with no compiled class, nothing compiled yet, a change under a source
  root jails does not know — each prints the reason, names the path that caused
  it, and runs everything, because a selector that silently drops a test is a
  green build proving nothing. It cannot see reflection, a component scan or a
  resource file, which is one more reason `jails check` stays `mvn clean
  verify`.

  Needs the console launcher, which `jails test --fast` installs for you. The
  daemon exits after 30 minutes idle, restarts itself when `pom.xml` changes,
  and is per-project. `jails check` is still `mvn clean verify` — nothing fast
  is allowed to be the last word.
- `jails fmt` — reformat in place (Spotless); `jails check` — format check +
  compile + tests (`mvn clean verify`). Both need `jails add format`. The
  `clean` is load-bearing: Maven's incremental compile leaves deleted tests
  in `target/`, and Surefire will still run them.
- `jails lint` — a closed set of source checks for APIs and shortcuts that
  **compile** but conflict with what jails generates: `@MockBean` where Boot 4
  wants `@MockitoBean`, and its siblings. No compiler and no Maven, so it
  answers on a project that does not build. The same table is rendered into the
  generated `AGENTS.md`, which is what stops the machine check and the guidance
  given to a coding agent drifting apart.
- `jails completion <bash|zsh|fish|elvish|powershell>` — shell completion. The
  bash script carries a hook that asks `jails editor complete` what this
  project declares, so `jails g query Recent st<TAB>` completes `status:`
  from the entity; it falls through to the static script for every position
  the model has no answer for, and only `g`/`generate` reaches the binary.

`generate`, `destroy`, `add` and `remove` all take `--package <sub>` to override where
the code lands, which the model carries as `@package(name)` on the
declaration; `--package ''` writes straight into the base package, spelled
`@package("")`. It selects a place and never a name: every type, route, SQL
and test name is still the convention's.

## The save-and-reload loop

There is no `jails dev`, and there deliberately isn't going to be one: on a
machine with a Java language server the loop already exists, and jails ships
both halves of it.

**Measured, not assumed.** A fresh project with no `target/` at all, opened in
Neovim and left alone, ends up with `target/classes/**.class` and
`target/test-classes/**.class` written by jdt.ls — with no Maven run. Eclipse's
m2e connector points the output folder at Maven's own, so the language server
that is already compiling your file for diagnostics is compiling it *to the
directory the running application is watching*.

The other half is `spring-boot-devtools`, which `jails new` puts in by default
and which polls the classpath and restarts when a class changes. So:

```
:w  ->  jdt.ls writes target/classes/...  ->  devtools restarts
```

A `jails g` is a save too: managed sources are written into the same
`src/main/java` the language server is already watching, so there is no
second source root to tell an IDE about and no build-file block declaring one.

`jails new` also writes `src/main/resources/META-INF/spring-devtools.properties`
with a 200 ms poll and a 50 ms quiet period. Boot's defaults are 1 s and 400 ms,
which is up to **1.4 s of waiting after a save before the restart even begins**.
They are `defaults.` entries, so anything you set yourself still wins, and they
apply only when devtools is active locally — never in a packaged jar, never in
tests.

**Every way this breaks is silent**, which is why `jails doctor` has a `reload`
check rather than jails having a supervisor. It reports the three settings whose
wrong value costs nothing at startup and simply means saving a file does
nothing: no `spring-boot-devtools` at all, `spring.devtools.restart.enabled=false`,
and `spring.devtools.restart.trigger-file` — the last being the one that reads
as "hot reload is broken here", because a recompiled class *is* seen and then
deliberately ignored until that one file is touched.

Two things the loop does not survive, and no amount of tuning changes it:
changing a record component, a `sealed` hierarchy, an annotation or a method
signature is a **restart**, not a swap. jails' domain layer is records, so every
edit there is a restart — that is devtools working, not failing. And `jails
check` stays `mvn clean verify`: an incremental compile cannot see that a
deleted method left a stale caller.

## Declarative applications: `jails app`

`.jails/app.toml` composes the same generic capabilities and generators the
CLI exposes. It is a reproducible command sequence, not a domain-specific
plugin or a second programming language:

```toml
schema = 1
capabilities = ["db", "api", "actuator", "security", "docker", "ci"]

[[generate]]
kind = "enum"
name = "TaskStatus"
fields = ["PENDING", "RUNNING", "DONE"]

[[generate]]
kind = "scaffold"
name = "Task"
fields = ["id:uuid@pk", "status:TaskStatus@index", "createdAt:instant"]
indexes = ["status, created_at desc"]

[[generate]]
kind = "usecase"
name = "CreateTask"
fields = []
on = "Task"

[[generate]]
kind = "query"
name = "TasksByStatus"
fields = ["status:TaskStatus"]
on = "Task"
```

`usecase` generates a typed command, application port, transactional
implementation, POST adapter, and mock-free tests over an existing scaffold.
It only derives conservative values (identity, timestamp, status default,
empty optional/collection, zero counter, or false); if a required value cannot
be proven, generation stops and asks for that field. With
`strategy_yields = "SomeEvent"` (CLI: `--yields SomeEvent`), the event must
already be generated; Jails wraps the use case with a PostgreSQL transactional
outbox, an ordered sink port, Kafka adapter, leased bounded-retry relay, stable
event identity, and a real database/broker test. Optional `http-sink` intents
join that same delivery chain; the relay marks success only after every
configured sink acknowledges the event. `association` declares ordered
child-to-parent field mappings as persisted composite tenant/ownership
invariants. `http-workflow` composes a generated safe fetcher into durable,
bounded traversal without adding a domain-specific app command. `query`
generates a typed read port, visible named-parameter JDBC SQL, POST adapter, and
a real PostgreSQL test. Its first contract deliberately accepts only required
scalar equality filters, orders by a stable key, and caps the result window at
100 instead of guessing null, list, keyset, or sort semantics.
`transition` generates a scope-aware optimistic update: `id`, `@scope` fields,
and `version` match the row; remaining fields update and version increments in
one statement.

- `jails app plan [--manifest <path>]` validates the manifest and shows its
  capability and generation intents without writing.
- `jails app apply [--manifest <path>] [--no-start]` installs capabilities in
  declaration order, then applies generation intents in declaration order.
- global `--pretend` turns `app apply` into the same read-only plan.
- `jails new <name> --app <manifest>` and `jails new-cli <name> --app
  <manifest>` create the project, seed the manifest into `.jails/app.toml`, and
  apply it — one command from an empty directory to a project that passes
  `mvn clean verify`. The manifest path is read relative to where you are
  standing, not to the project being created.

`apply` replays the manifest row by row into the model through the same
frontends `jails g` and `jails add` use. Every frontend is idempotent, so an
interrupted apply is repaired by running it again. Changing an already-applied
row is an update to a known entity: an intent is identified by kind, name and
package, and its fields are content, so the regenerated result is three-way
merged over whatever you have edited by hand and a conflict refuses rather than
overwrites.

The manifest is intentionally a closed schema: `schema`, `capabilities`, and
`[[generate]]` entries with `kind`, `name`, `fields`, `timestamps`, `indexes`,
`package`, `on`, and `yields`. Unknown keys fail instead of being silently
ignored.

`on` and `yields` are the reference keys — the entity an intent acts on, and
what it produces. `strategy_on` and `strategy_yields` parse as deprecated
aliases; setting the same reference under both names is an error rather than a
coin toss. [`examples/ACCEPTANCE.md`](examples/ACCEPTANCE.md) is the executable
done/not-done boundary for the example applications.

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

- `jails sync [--pretend] [--no-start]` — apply every declared capability that
  is not there yet. A fresh clone becomes the project it claims to be in one
  command, instead of whoever set it up recalling which `add` calls they ran.
  It is also how a project takes a newer jails' output: every capability is
  idempotent and reports what is already there, so a sync over a correct
  project changes nothing and says so. With `--pretend` it answers "what is
  this project missing?" without writing.

A capability name that jails does not know is an error listing the real ones,
for the same reason a misspelled layer is: `postgress` sitting in the file
would look declared and never sync. The names are the labels `add` uses, not
its aliases — `db`, not `postgres` — so one capability cannot be listed twice
under two spellings. Declaring the same capability twice is an error too,
whichever of the two shapes below it is written in.

### `[[capability]]` — one that was given a name or a package

The array holds a capability nobody parameterised. `jails add csv --name Order`
is a different thing from `jails add csv --name Invoice` — two readers, two
classes, two sets of files — and a string array has nowhere to say which. Those
get a table each:

```toml
[[capability]]
kind = "csv"
name = "Order"

[[capability]]
kind = "actuator"
package = "ops"
```

`kind` is required; `name` and `package` are the same values `jails add` takes,
and which of them a capability accepts depends on what it is:

| | `--name` | `--package` |
|---|---|---|
| `csv`, `sqlite`, `json`, `http` | yes — two names are two capabilities | yes — part of which one it is |
| `api`, `actuator`, `cache`, `security`, `cors`, `sse`, `mail`, `redis`, `observability` | no — there is one per project | yes — it moves where the class is placed |
| everything else | no | no — the output is project-global |

A parameter a capability has no meaning for is refused rather than ignored, on
the command line and in this file alike, so the manifest cannot declare
something `jails add` would have turned down. You do not maintain these tables
either: `add` writes one when the capability it applied needed it, `remove`
takes the whole table back out, and both leave every other byte of the file
alone.

### `[[architecture.allow]]` — a reviewed exception to the fitness suite

`g scaffold` writes an ArchUnit suite that fails on a class reaching across a
layer boundary. Sometimes one is deliberate, and the way to say so is a table
per edge:

```toml
[[architecture.allow]]
from = "billing"
to = "shared"
packages = ["com.example.shop.domain.shared.money.."]
reason = "billing reads the shared money value objects"
expires = "2027-01-31"
```

All five keys are required. `packages` must name a bounded package inside the
`to` slice — a blanket `com.example.shop.domain..` is refused, because an
allowance that covers everything is the rule switched off under another name.
`expires` is what stops one outliving its reason, and an allowance nothing uses
fails the suite so a dependency that has since gone does not leave a permanent
hole.

**jails never acts on these; the generated test reads them.** They live in
`jails.toml` rather than under `.jails/` because they are about your code and
are read by your build: `rm -rf .jails` leaves a project that still runs the
same suite and reaches the same verdict. jails checks the shape when it reads
the file — an unknown key is an error naming the five, for the same reason a
misspelled layer is — and the suite reports everything else, where the refusal
can name the dependency it was about.

The frozen violations `jails architecture baseline` records live in
`src/test/resources/archunit/frozen`, an ordinary checked-in test resource.

All four tables are closed sets. This renames layers, declares capabilities and
records reviewed architecture exceptions, and nothing else — no template
overrides, no per-kind paths, no plugin hooks.

Every command takes `--debug`, which prints the `mvnw`/`mvn`/`mvnd`/`java`/`git`/`curl`
command lines jails shells out to instead of running them silently.

Which Maven runs is `./mvnw` when the project has one, then `mvnd` when it is
installed and can start, then `mvn`. **`JAILS_MAVEN` overrides all three** and
names the command to run — the escape hatch for a machine where the daemon is
present but unusable. jails also declines to pick `mvnd` when its registry
directory is not writable, because that failure happens *before* Maven runs and
looks like an ordinary non-zero exit at the call site. `jails doctor` reports
which one it chose and why.

Every command that writes also takes `--pretend` (`-p`): it runs every check
and prints what would change, then stops without touching the project. Global
on purpose — Rails puts it on every generator rather than on the few that
looked risky, and the value is never having to remember which commands
support it. One spelling per verb: `--dry-run` still parses for one release
and is advertised nowhere, and so do `--force` (now `--yes`), `g field` (now
`jails entity field add`), `model plan --bundle` (now `--plan-out`) and
`model apply --bundle` (now `--plan-in`).

- `jails generate|g handler <Name>` — an `HttpHandler` in `api/` for one
  entity: derives its path (`WorkItem` → `/work-items`), takes its service as
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
  `CoffeeRewardRule` and `LargeTransactionRewardRule` — each `@Component`,
  each `@Order`ed, each with a `@Disabled` test naming what to prove — plus
  `RewardRuleEvaluator`, the fold: it takes the whole `List<RewardRule>` as one
  constructor parameter and answers `first(...)` (the first rule that grants
  anything) and `all(...)` (everything granted, in order). The order is
  explicit because without it the injected list is whatever component scanning
  produced, so a rule that answers everything can silently come first and
  nothing after it is ever reached. A variant that already carries
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
`map<string,double>`). A required collection is defensively copied with
`List.copyOf`/`Map.copyOf` and rejects null, so the caller's own reference
cannot change what the record holds; an optional one is `Optional.empty()`
when it is absent. A map key is a `string` or an enum, an element is not
itself optional or a collection, and a *stored* entity refuses a collection
by name: a column type for one would be a codec, and the specification
forbids that silently becoming JSON. A **capitalised** one is a
type this project owns and is used verbatim, so the generators compose:

```
jails g enum Currency GBP EUR USD
jails g record SourceRef system:string externalId:string
jails g value CanonicalTransaction id:string! amountMinor:long \
    currency:Currency source:SourceRef note:string?
```

The Java spellings of the built-ins (`String`, `LocalDate`, …) still mean the
built-in, so `id:String` behaves like `id:string`.

**An `@marker` adds a constraint the Java type cannot carry.** Most change the
generated SQL. `@scope` instead changes generated HTTP boundaries.

| marker | in the migration |
| --- | --- |
| `@pk` | part of the primary key; several make it composite, in declaration order |
| `@unique` | `unique` on the column |
| `@index` | its own single-column index |
| `@scope` | require the request value to equal the authenticated JWT's same-named claim; scoped scaffolds omit unsafe broad list/get/delete routes |
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

**Afterwards, `jails entity index add`.** `--index` and `@index` are both
creation-time, so an index a table turns out to need later had no verb:

```
jails entity index add Message 'customer_id, created_at desc'
```

Adding writes one forward migration, checks the columns the same way, and
records the index on the entity so a re-plan reproduces it. Removing requires
the exact physical index name and writes a later `drop index`; the accepted
create migration is never rewritten:

```
jails entity index remove Message 'customer_id, created_at desc' \
  --confirm-index idx_message_index_ab12cd34ef56
```

A wrong confirmation, missing shape, or direct model deletion refuses before
any byte changes. The same index twice is refused rather than written twice.
An index is the easy half of what `entity field add` already does — a new
column has to argue about a data plan for a populated table and an index has
none.

The enum's sample is the first constant by *name*: `Currency.GBP`, not
`Currency.values()[0]`, so reordering the enum cannot silently change what the
sample stands for.

**A suffix picks the validation.** `name:string!` is required *and* non-blank;
`name:string?` becomes an `Optional<String>` component (pass `null` to mean
absent); bare `name:string` is required but may be blank. Hardcoding one policy
is what made every generated value type reject blank descriptions. `!` is a
text rule, so `when:date!` is an error rather than a no-op.

jails cannot invent a sample of a type you own, so a companion test that needs
one is generated in full and `@Disabled`, naming the component it needs. Two
cases escape that: an enum is filled in with its first constant, and a `?`
component with `Optional.empty()`.

## What a new project looks like

One source root per source set. Everything jails generates is written into
`src/main/java`, `src/test/java`, `src/main/resources`, `src/test/resources`
and `src/test/http` beside whatever you write there yourself, and the only
thing that says which files are jails' is the accepted projection in
`.jails/compiler.lock.json`. Nothing under `.jails/` holds Java, SQL or
resources. Open the project in any IDE and there is exactly the tree it
expects.

`.jails/` is the **input**: `.jails/model.jdl`, the lock that reproduces a
merge base, and `.jails/app.toml` if you keep a manifest. The two things in
there that are not input — `apply.lock`, a mutex, and `run/`, a daemon socket
beside two caches — are covered by a `.jails/.gitignore` jails writes itself,
so neither reaches a commit or a diff whatever your own `.gitignore` says.

**Commit the lock, and let git leave it alone.** jails writes a
`.jails/.gitattributes` marking `compiler.lock.json` as `-diff
merge=binary`: it is one exact copy of every managed file, so a diff of it
restates the whole project beside the change you actually made, and a
textual three-way merge of two branches' locks produces a file that
describes neither tree. When a merge conflicts on it, keep either side —
`git checkout --ours .jails/compiler.lock.json` — and run `jails sync`.
Either side's projection is a real ancestor of both trees, so the merge of
your managed files is still a three-way merge; what you lose by picking one
is nothing but the other side's record of what it had generated. Both files
live inside `.jails/`, because a `.gitattributes` at the repository root is
yours.

Delete `.jails` and the application is untouched: it builds, and `mvn test`
passes with the same tests, generated ones included. What you have lost is the
ability to regenerate and to merge, and the next `jails g` says exactly that
rather than quietly seeding a second model over code it no longer recognises
as its own — it finds the `// Generated by jails from art_…` header on your
sources and tells you to restore the model from git.

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
| `add db` / `add kafka` | `compose.yaml` (and `src/main/resources/db/migration` for `db`; Spring `add db` also writes `TestcontainersConfig` and `@Import`s it into the `@SpringBootTest` classes already on disk) |
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

It also brings the `.jdl` filetype with it: `.jails/model.jdl` gets syntax
highlighting for the JDL v1 grammar, and the buffer picks up the canonical
formatter's own settings (two-space indent, `//` comments, a 100-column
target), so an edit made by hand and one made by `jails model fmt` agree.
Buffer-local keys are `<leader>Jk` check, `<leader>Jf` fmt, `<leader>Jp` plan
and `<leader>Je` explain.

The colours are the smaller half of why the filetype exists. Copilot -- and
anything else keyed on filetype -- disables itself in a buffer that has none,
and Neovim ships no `.jdl` rule, so before this the model was the one file in
a jails project with no completion in it. `.jdl` is also JHipster's extension
for an unrelated language: the path jails owns is claimed outright, and any
other `.jdl` only when it opens with the `jdl <version>` header JDL v1 requires,
so a JHipster file falls through to whatever else claims it.

`tests/editor.rs` checks the highlighted vocabulary against the parser's own
string literals. A syntax file is a hand-written copy of a vocabulary the
compiler owns, and it drifts invisibly -- a misspelled keyword just renders in
the default colour and reads as an ordinary identifier.

- `jails src <Type> [--json]` — where a Java type's source is, fully qualified.
  Searches the project's own sources first, then whatever `JAILS_SOURCE_PATH`
  names (or `deps/` when it does not). Instant, and works on a project that does
  not compile — which is exactly when a language server can least help. It
  **lists every match rather than picking one**: a project with three
  `Status.java` files is ordinary, and silently choosing sends your editor to
  the wrong one.
- `jails bench [--vus N] [--duration 30s] [--export FILE]` — runs the k6 load
  test `jails add loadtest` wrote, stating the profile first so the number is
  reproducible. jails does not parse k6's output: k6 prints p95 and p99 itself
  and its own thresholds (`http_req_failed rate<0.01`,
  `http_req_duration p(95)<500, p(99)<1000`) decide pass or fail. Refuses
  without a load test, and without k6 on PATH.

## A codebase jails did not create

Most of jails never touches a build tool — `routes`, `beans`, `stats`,
`notes`, `why`, `explain`, `rename`, `doctor` and most of `generate` read
source and write source, and none of them needs Maven to answer. The door
looks for any build marker it recognises (`pom.xml`, `build.gradle`,
`build.gradle.kts`, `settings.gradle`, `build.xml`, `BUILD.bazel`), nearest
wins — keying it on `pom.xml` alone refuses all of them on a foreign
project.

**A Groovy `build.gradle` is read and spliced, not merely recognised.**
`add`, `generate`, `doctor`, `about`, `build`, `clean`, `check`, `test`, `run`
and `watch` all work on one, and `jails gradle` is the escape hatch `jails mvn`
is for Maven. `jails test --failed`, `--json` and `--slowest` work too: Gradle
writes the same JUnit XML Surefire does, in a different directory.

**The warm test engine, `--affected`, `testd`, `console` and `runner` work on
Gradle too, and they never guess at the build's layout.** What each needs is
a resolved classpath and the output directories, and those are the build's
own answers: the `// jails:dependencies` block jails owns in `build.gradle`
registers a `jailsClasspath` task that prints `configurations.runtimeClasspath`,
`testRuntimeClasspath` and every source set's output, and jails invokes it
through the wrapper and caches the answer under `.jails/run/` until any
Gradle input file changes. `jails test`, `--engine build` and `--engine warm`
discover the same tests and report the same counts on either build. A Gradle
project that has no such block yet — one jails did not create, before its
first canonical command — is refused by name with the way out: `jails test
--fast` declares the test launcher, which writes the block. Gradle's
up-to-date check is content-based, so a source that was touched without
changing keeps its class older than itself; the warm engine then reports the
outputs stale and hands the run to the build engine, which is the safe
direction.

`jails add format` and `jails fmt` refuse on Gradle, by name. Spotless is
applied with `id 'com.diffplug.spotless'` inside `plugins {}`, which Gradle
only accepts as the script's first statement, and jails' Gradle adapter only
ever appends a marked block. The appended alternative — `buildscript {}` plus
`apply plugin:` after `plugins {}` — was measured on Gradle 9.7 and fails
evaluation, so there is no shape that keeps the adapter's contract. Apply the
plugin yourself and run `./gradlew spotlessApply`; `check` then enforces it.

**Maven stays the default.** `jails new` with no `--gradle` creates a Maven
project and goes on doing so. `--gradle` is for the case the reading work was
built for in the first place: a Gradle service you have to work in, which you
now also have a way to stand up from nothing.

**Generated *tests* have a floor the generated main code does not.**
`MockMvcTester` (`org.springframework.test.web.servlet.assertj`) is Spring
Framework 6.2, which is Boot 3.4, and nine of jails' companion tests are
written against it. `g controller` and `add cors` carry a classic
`MockMvc` variant and pick it from the project's Boot major. The other seven —
`add api`, `add security`, `g scaffold`, `g usecase`, `g query`,
`g transition` — **refuse** on an older project rather than write a test that
cannot compile, and the refusal names the version and what still works.

Every reader of `build.gradle` has three answers, not two: yes, no, and *"this
file says something I do not understand"* — and the third refuses rather than
guessing. `build.gradle.kts` and a root holding only `settings.gradle` are
still foreign on purpose: recognising a filename is not understanding a build,
and a tool that half-understands one reports a dependency the build does not
have.

For a genuinely foreign build the cost is stated where you meet it — generated
code is shaped by what the pom says, so with no pom the repository adapter is
plain JDBC rather than a Spring `JdbcClient` bean and no `package-info.java` is
annotated. `generate` prints which shape it chose and names the dependencies
you will have to add yourself; `doctor` leads with the real build tool rather
than reporting on a pom that is not there.

### `jails adopt`

For a project that keeps its controllers in `controllers` and its repositories
in `persistence`. `adopt` reads the directories under your base package, maps
the ones it recognises onto jails' eleven layers, and writes a `[layout]` table
— which is all it does. Every command that reports or writes per layer already
reads that table.

A directory it does not recognise is **reported, not guessed**: a wrong
`[layout]` entry is worse than a missing one, because jails would then write
confidently into the wrong package. If two directories both look like the same
layer, neither is written and both are named — a `[layout]` table can only say
one thing.

It never writes `[project] capabilities`. That list is what `jails sync`
applies, and inferring it would have `sync` install things nobody asked for.

Run `jails adopt --pretend` first.

`jails model init` is the step after it: it writes the `app` block of
`.jails/model.jdl` from what the project already says about itself, and
nothing else.

### `jails adopt entity <Name>`

For a type you wrote before jails knew the project. Once the model exists,
`jails entity field add Message ...`, `jails rename entity Message ...`
and `jails destroy record Message` all answer *"no `Message` is declared"*
about a record that is plainly there — because the model is what those
commands read, and nothing put it in the model. `adopt entity` does:

```text
jails adopt entity Message
  adopt   Message  src/main/java/com/example/notes/domain/Message.java
  field   id:uuid
  field   title:string
  field   body:string?
  yours   Message.record  -- jails will not write `src/main/java/.../Message.java`
```

It finds the one `Message.java` under `src/main/java` (two is a refusal that
names both, the way `jails src` lists rather than picks), reads its record
components — or a class's constructor parameters — with the same small Java
reader `beans` uses, and maps each Java type onto a field type through the
one table in `Field syntax`, read backwards: `UUID` is `uuid` because `uuid`
renders to `UUID`. A component is required unless it is `Optional<T>`. A type
the table does not render to is refused **by component**, naming the type and
the types it knows; a capitalised type the project itself declares under
`src/main/java` — an enum of its own — passes through by name, exactly as
`jails g record Message priority:Priority` would. Nothing is guessed at.

What it writes is the `entity` declaration beside one line that says whose
the record is:

```jdl
entity Message {
  id:    uuid
  title: string
  body:  string?
}

eject Message.record @id(eject_ca48573a411aefbe) @adopted
```

`@adopted` is `eject` without the transfer: the compiler leaves the record
out of `.jails/generated`, writes nothing at your path, and takes your file
into the plan as an exact input — a precondition with its digest, never an
output — so the plan refuses if the file changes between preview and apply.
No companion test is rendered for it either: the one jails writes pins the
null rejection of the compact constructor jails renders, which a record jails
did not render need not have. A record outside the `domain` layer is pinned
with `@package` rather than moved. Your repository, service or controller
stay yours and unmodelled: jails does not guess which of its facets they are.

From then on the entity is one the `jails entity` commands know. `entity field
add` evolves the model — the field in your record is yours to add, and the
generated code that reads it compiles against your file. `rename entity`
follows you rather than leading: while your file still names `Message` it
refuses with `manual-edit-required` and the path, and once you have renamed
the type it moves the declaration and the `eject Message.record` line with
it. `destroy record` removes the declaration and its `@adopted` line together
and leaves your file where it is — and refuses, like any destroy, while an
operation still points at the entity.

Adopting a type twice is a no-op that says so. `--pretend` previews the plan;
`--output json` reports the execution.

### `jails architecture baseline`

`g scaffold` writes an ArchUnit fitness suite, and on a project written before
jails arrived it fails over the reader's own code. `baseline` records today's
violations so the rules fail only on **new** ones — the four manual steps
setting up ArchUnit's freeze store, as one command.

Nothing on disk is rewritten. The permission is granted for one run through
system properties, so `archunit.properties` stays strict in the repository and
a new violation still fails the build. A baseline that edited the rules would
be indistinguishable, six months later, from never having had them. The store
is `src/test/resources/archunit/frozen`: an ordinary test resource, committed
with the code it describes, and nothing under `.jails/`.

### A path that addresses its filters

```
jails g query TicketsFor userId:long --on Ticket --path '/admin_api/tickets/{userId}'
```

A `{name}` in the path must name a filter this query takes. It becomes a
`@PathVariable` and the endpoint becomes a **GET with no body** — the criteria
record is still what the port takes, so the port never learns that some of its
input came from a URL.

**All or none.** A variable naming no filter is refused (the value would go
nowhere), and so is a mix — the controller would have to build the criteria
from a partial body plus some path variables, and "which half came from where"
is a rule nobody would remember.

Before this, a template in `--path` was accepted as text: the controller
carried it in `@RequestMapping`, declared no `@PathVariable`, and Spring
matched the URL then looked for a request body nobody sent.

### Optional query filters

```
jails g query TicketsByStatus status:string! category:string? --on Ticket
```

A `?` filter means **absent is unfiltered**, which is one answer rather than a
guess. It generates `(cast(:category as text) is null or category = :category)`
— and the cast is load-bearing rather than tidiness: PostgreSQL rejects a bare
`$1 is null` with *"could not determine data type of parameter"*, because that
position gives the parameter no type to infer from. The second occurrence needs
none; the column supplies it.

A **collection** filter is still refused. `in (...)`, `= any(...)` and "every
one of them" are three different queries, and jails will not choose.

### An enum constant's wire value

```
jails g enum IssueStatus OPEN=open IN_PROGRESS=in_progress
jails g enum IssuePriority NONE=- HIGH=! URGENT=!!
```

A bare constant is called its own name and generates exactly what it always
did. `NAME=wire` says the two are different, which they usually are once a
client already exists — `open`, `Account`, `!!` are none of them Java
identifiers in any casing.

**The Java name and the wire value are two different things**, and treating
them as one fails quietly: an enum whose constant is `OPEN` serialises as
`"OPEN"`, the page reads `"open"`, and the badge is blank. So the enum carries
`@JsonValue`/`@JsonCreator`, the generated test round-trips every value, and an
unknown one throws listing what would have been accepted.

**The database stores the name, not the wire value**, and the `check`
constraint lists the names: a column is an internal contract with one reader,
and the wire value is the external one.

On a Spring project jails also writes `<Name>Converter`, a
`Converter<String, <Name>>` bean. `@JsonValue` covers a JSON body and nothing
else — a form field, a path variable and a query parameter all go through
Spring's conversion service, whose enum converter calls `valueOf` and knows
only the Java names. Without that bean a form carrying `status=open` is a 400
whose message is about binding rather than about the value.

### `--consumes json|form`

`$.post(url, {email})`, which is what a jQuery page and an HTML form send, is
`application/x-www-form-urlencoded`. A `@Valid @RequestBody` endpoint — a JSON
body — answers that **415**, with a message about a content type rather than
about the code, so the binding has to be stated rather than assumed.

```
jails g usecase Ping email:string! --on User --consumes form --path /customer_api/ping
```

emits `@Valid @ModelAttribute` instead, which Spring binds from request
parameters through the record's canonical constructor. Valid on `controller`,
`usecase`, `query` and `transition` — the four recipes that bind one request
body. `handler` writes a whole CRUD surface rather than one route, and
`webhook` reads the raw bytes *before* the signature is checked, which is the
bug that kind exists to avoid; both refuse the flag by name.

Recorded on the intent, so `jails sync` and `jails app apply` regenerate the
same shape, and changing it is an edit to a known entity rather than a new one.

**The names on the wire follow the project, not jails.** Set
`spring.jackson.property-naming-strategy=SNAKE_CASE` (`jails set` owns the key)
and a form-bound record's components carry `@BindParam("user_id")` for exactly
the components whose two spellings differ. That annotation is not decoration:
Spring's **data binder** has no naming strategy — the Jackson property
configures Jackson — so without it a project whose JSON is `user_id` still
binds a form field called `userId`, and the component silently arrives null.

If nothing seems to apply, check `jails doctor` for **MVC override**.
`@EnableWebMvc` in a Boot project switches off Boot's web auto-configuration,
so every `spring.jackson.*` property is ignored with nothing in the log to say
so.

### Where a scaffold's `create table` goes

Two places, and which one is decided by what the project already has:

- **`src/main/resources/db/migration/`** — a Flyway migration, when that
  directory exists. `jails add db` creates it.
- **`src/main/resources/schema.sql`** — a `-- jails:table-<name>` marked block
  appended to the script Spring initialises the datasource from
  (`spring.sql.init`), when there is no Flyway. `jails destroy` takes exactly
  that block back out and leaves the tables you wrote alone.

A project with **neither** is told so, by name, with both fixes. Saying
nothing leaves it with a repository, a JDBC adapter and an `IT` against a table
that does not exist.

### `jails modernize` (alias `upgrade`)

For a project still on the versions it was created with. It moves the build to
the Spring Boot and JDK jails generates against, as **one commit**, because the
edits are interdependent — a Gradle wrapper bumped without the toolchain block
fails evaluation, and a toolchain bumped without the wrapper fails on an
unsupported class file version.

On a Gradle project it changes five things, and every one of them is a real
`./gradlew build` that failed without it:

| what | why the failure does not say so |
|---|---|
| Boot plugin → 4.1.0 | — |
| Gradle wrapper → 9.7.0 | 8.5 does not run on JDK 26 at all |
| Java release → 26, as a `java { toolchain { … } }` block | Gradle 9 removed the project-level `sourceCompatibility`, and fails *evaluation* with "unknown property" before a task runs |
| `test { useJUnitPlatform() }` | reports "the test task did not discover any tests" rather than "your tests are JUnit 5" |
| `datetime` → `timestamp` in `schema.sql` | H2 2.x answers `Unknown data type: "DATETIME"`, four `Caused by` levels below a bean-creation error |

On a Maven project it moves the `spring-boot-starter-parent` version and
whichever release property the POM already states. A release the POM does not
state is left to its parent rather than decided here.

The SQL rewrite is gated on H2 actually being the project's driver, and only
touches `src/main/resources/schema.sql` and `data.sql` — the two files Spring
initialises a datasource from. Flyway migrations under `db/migration` are
applied-once history, and rewriting one that has already run changes a checksum
rather than a schema.

**What the upgrade breaks in code you own is reported, not rewritten.** A
Jackson 2 import is named with its file, and left alone: Boot 4 ships Jackson 3,
where the package moved *and* the API changed, so a mechanical rename would
produce something that still does not compile while looking migrated. Same for
`javax.*` packages that became `jakarta.*`.

Run `jails modernize --pretend` first.

On a project that has a model, `modernize` recompiles it afterwards, the way
`jails sync` does: the Boot version it moves decides what jails' own files
say — which `MockMvc` dialect a controller test drives, which package
`@AutoConfigureMockMvc` comes from, `javax` against `jakarta` — so the
generated tree follows the build file in the same command. `--pretend` says
so without writing either.

## Shaping the generated code

Drop a file at `.jails/templates/<name>` to replace the built-in template of
the same name for that project, or at `~/.config/jails/templates/<name>` to do
it for every project on the machine. The project's copy wins. The names are the
paths under jails' own `templates/` directory — `generate/command_test.java`,
`spring/idempotency_guard_java.java`, and so on.

The placeholders are the contract. An override has to use exactly the ones the
built-in uses, no more and no fewer; anything else is refused with a message
naming your file and the difference. Everything else about the file is yours.

The cost, stated plainly: **an overridden template is not covered by jails'
snapshot tests**, so you own that file's output. `jails doctor` lists every
override in effect for exactly that reason.

## Not yet

Deferred out of v1 on purpose — this is meant to stay a small tool:

- **Understanding a Gradle build.** The Gradle adapter appends one marked
  block to `build.gradle` or `build.gradle.kts` and touches nothing else, so
  the two DSLs differ by the syntax of that block rather than by a grammar
  jails has to understand; a project holding both build scripts refuses rather
  than picking one. The bar is answer exactly or refuse, never guess — which
  is why a classpath is *asked* of the build through the task in that block,
  and why `format` and `fmt` stay refused on Gradle: the plugin entry they
  need cannot be appended, and that was measured rather than assumed.
- A runtime bean/route view (booting the context and asking Spring itself).
  `routes` and `beans` read source instead, which is instant and works on a
  project that does not start — at the cost of anything decided at runtime.
- A plugin system with lifecycle hooks or third-party code. Overriding a
  *template* is supported (see below); running arbitrary code inside jails is
  not, and the difference is the point — data is extensible, logic is not.
- **Pagination.** A query declaring `limit` caps the rows and nothing more:
  the port gets a `DEFAULT_LIMIT`, the adapter gets a `limit` clause, and a
  caller handed exactly that many rows cannot tell a full page from a complete
  result. No cursor, no total, no truncation flag. Ask for a limit you can
  live with, or eject the adapter and page it yourself.
- **One API style, chosen per command rather than per project.** A scaffold
  serves REST over a resource path; a `command`, `query` or `transition`
  serves whatever its own `route` says, which is commonly `POST` — including
  for reads. `g scaffold --path` and an explicit `route` on each operation let
  a project be made consistent, but consistency is something you ask for every
  time rather than something the project settles once.
- **A layer for capability configuration.** A capability's own classes —
  `CorsConfig`, `MetricsConfig`, `AppMetrics` and the rest — are written into
  the base package, where a generated *kind* goes through the package
  convention instead. Placing them properly means a new package in a closed
  convention table, which moves those files in every project generated so far;
  until that is worth doing, they sit beside the application class.
- **Generated tests prove wiring, not domain behaviour.** They check that a
  route dispatches, that a record rejects what its constraints forbid, that a
  listener reaches its port, and that a migration's schema is the one the
  model declared. They do not check the rules of your domain, they seed every
  string field with `"sample"`, and there is no generated concurrency test for
  the CAS an `@version` column exists for. Treat them as a floor to build on.
- **`jails g action <Name> --on <Controller>`** — splicing a handler and its
  test into an existing controller. `g controller` always writes a new
  standalone file, so related routes end up in separate classes unless you
  move them by hand.
- **An operator or back-office surface.** jails generates the REST surface and
  nothing to administer it — no CRUD console, no admin views. This is the one
  thing a Django or Rails port expects for free and does not get here; it is a
  scope line rather than a plan.
- **An endpoint accepting both JSON and a form body.** `consumes json` and
  `consumes form` each work; one route accepting either is not expressible.
- **Lightweight in-process presence.** `g presence` generates the
  PostgreSQL-backed, cluster-safe version. A single-node in-memory variant
  would be a different recipe and does not exist.
- **Latency work behind a measurement.** An incremental source index for
  `routes`, `beans` and the editor protocol, additive test-dependency hints
  for `test --affected`, service identity labels and semantic readiness for
  `run` are each plausible and each waits for a number showing where the
  time goes; none starts without one.
- **An Ecto-style SQL sandbox.** Wrapping each generated test in a
  transaction that rolls back instead of starting a container per class is
  not a dependency of anything and not a default. If the experiment is run,
  the result is recorded here either way.
- **Slices as a declaration.** A slice that owns package layout, ports,
  migrations and a route prefix is not in this language version, and adding
  one is a version boundary rather than a keyword: a compiler that did not
  know the word would accept the declaration and quietly ignore every name
  it moves. `--package` is the vertical slice today, collapsing one
  entity's classes into a single package. `rename entity` still accepts
  a `Billing.Task` prefix and ignores it, which is harmless because two
  entities cannot project to one Java type; the prefix goes with the next
  deliberate breaking change rather than on its own.
