# missing.md — what a real project needed and jails could not give

Found by pointing jails at `minicom/minicom-15-01-2026-*`, an untouched
take-home with four backends and two hand-written frontends. Every entry is a
line that could not be written with the CLI.

**A closed entry is deleted from this file, not marked done.**
`git log -p -- missing.md` is the record.

Closed and deleted, so a re-report can be recognised as one: the `websocket`
capability (`jails g socket <Name>` is the slice, and `jails add websocket`
now says so), first-class `devtools` (one dependency and no code, so `jails
add dependency` is the verb -- both `run --watch` and `doctor` name it now),
H2's console and safe URL defaults (`jails db`, `jails db --web`, and two
`doctor` checks), WebSocket route discovery, and `@Value`/SpEL parsing in
`jails beans`. `jails fmt` on Gradle is a *deliberate* refusal, recorded in
`research.md` §5.1, not a gap.

---

## Code generation and architecture helpers

### Global Exception Handler & Error Scaffold (`jails g advice` / `jails add errors`)
- Spring Boot default error handling routes uncaught exceptions to `BasicErrorController`, producing generic 500 JSON without clear controller exception details or readable terminal stack traces (e.g. `@PathVariable` mismatches or missing URI parameters).
- Developers frequently need a structured `@RestControllerAdvice` that outputs clear debug logs and returns structured error payloads (e.g. RFC 9457 `ProblemDetail` or custom JSON with status, error, and message).
- **Expected**: `jails g advice <Name>` or `jails add error-handler` to scaffold a `@RestControllerAdvice` class with `@ExceptionHandler` methods for common web exceptions, validation binding errors, and fallback uncaught exceptions.

### Extending Existing Controllers (`jails g action` / `jails g route --on <Controller>`)
- `jails g controller <Name>` always creates a new standalone controller file. In traditional Spring projects where related routes live together in one controller (e.g. `MessagesController.java`), there is no CLI command to append an `@GetMapping` or `@PostMapping` handler method into an existing controller class.
- **Expected**: `jails g action <Name> --on <Controller>` (or `jails g method <Name> --on <Controller>`) to safely splice a new handler method and its corresponding MockMvc test into an existing controller.

### Adopting Pre-Existing Models (`jails adopt resource <Name>`)
- `jails resource field add <Entity>` refuses on a hand-written type: *"no `Message` is recorded in this project ... adding a component to something the store never recorded would mean guessing what its other components were declared as, and a declaration is not readable from the Java it produced."* The reasoning is right, and the consequence is that **every jails verb that evolves a resource is unreachable on a project jails did not create** -- which is the adoption story in one line.
- **Expected**: `jails adopt resource <Name>` registers an existing type into the store so `resource field`, `destroy` and `rename resource` work on it.

**What the design has to survive, measured on `minicom-15-01-2026-claude/spring`:**

- **The Java is not a record.** `Message.java` is a POJO -- `private long id, user_id;` on one line, `Boolean message_read`, getters below. `Project::record_in` reads a record's components and nothing else, and `java.rs` is deliberately "not a parser, and must not grow into one", so reading an arbitrary legacy class is not a small extension of what exists.
- **So the table is the better authority, not the Java.** Columns already map to and from `spec::Field` in both directions (`sql::Column`), and jails already observes schemas (`introspect db`, `pull`, `schema diff`). `jails adopt resource Message --table messages` can be *exact* where reading the class would be a guess.
- **Adoption must not claim jails wrote the schema.** This project's DDL is in `schema.sql`, not a Flyway lineage jails sealed. An adopted resource is storage-backed by a table jails did not create, so `--storage drop` has to stay refused for it -- the recorded lineage is the authority for what jails may retire, and adoption must not forge one.
- **Whatever cannot be read is confirmed, not defaulted.** A column says `varchar(64)` and not `@unique`; a record read off disk already carries no constraints for exactly this reason. The precedent to follow is `destroy --storage drop --confirm-table`: ask for the evidence rather than invent it.

### A route whose key is in the URL (`transition` path binding)

Measured on `minicom-15-01-2026-org`, implementing the feature list with jails
commands only. Every PATCH the frontend sends is
`/admin_api/conversations/{userId}/{field}`.

- `g transition` takes `--path` and `--method patch` now, and `--select
  <field>` lets it update a row keyed by `user_id` rather than `id`. What it
  still cannot do is take that key **from the URL**: the command record carries
  the selector, so a path variable is refused rather than mounted and ignored.
- **Expected**: with `--path /x/{userId}/status --select userId`, generate a
  command record *without* the selector and a port that takes it beside the
  command -- the URL identifies the row, the body carries the change.
- **Why it is refused rather than half-built**: mounting the variable without
  binding it is exactly `bugs.md` B48, which cost three generated tests failing
  with `Not enough variable values available to expand`. The two shapes of one
  port are the drift risk, so this needs deciding rather than defaulting.

### A `GET` with query-string filters (`query`)

- `g query` emits GET when every filter is a path variable and POST otherwise,
  so it cannot answer `GET /admin_api/users?status=open&category=Billing` --
  which is what a browser sends and what the minicom admin UI already calls.
- `--consumes form` binds `@ModelAttribute`, which Spring *does* fill from
  query parameters on a GET, so the pieces exist and only the verb decision is
  in the way.
- **Expected**: `--consumes form` plus a filter set that is all optional should
  give a GET binding from the query string.

### `modernize` does not re-plan jails' own output

- `jails modernize` moves the Boot version, and the Boot version decides what
  jails' *own* generated files should say -- `javax.validation` vs
  `jakarta.validation`, which `@AutoConfigureMockMvc` import, whether
  `spring-boot-starter-webmvc-test` is needed. After modernizing, files jails
  had written minutes earlier were reported as "rename these imports yourself".
- Regenerating fixes them, so the repair exists and nothing triggers it.
  `jails sync` re-plans recorded *capabilities*; there is no equivalent for
  recorded resources.
- **Expected**: `modernize` re-plans what the ledger records, the way `sync`
  does, and reports only the files the reader owns.

### Dual-Format `consumes = [json, form]` Request Support

- Current generators support `--consumes json` or `--consumes form`, but real-world web applications (like Minicom with jQuery `$.post` and API clients) frequently require endpoints that accept both form-urlencoded and JSON payloads without returning HTTP 415.
- **Expected**: Generator support for hybrid request binders or unified payload parsing.

### In-Memory / Room-Based Presence Generators
- `jails g presence` generates PostgreSQL cluster-backed presence, but lightweight in-memory group/room chat presence (e.g. admin online tracking per customer email channel) is a common pattern that lacks a generator recipe.
- **Expected**: A `socket-presence` recipe for room-based presence and lifecycle events.

---
