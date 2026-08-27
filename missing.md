# missing.md — what a real project needed and jails could not give

Found by pointing jails at `minicom/minicom-15-01-2026-*`, an untouched
take-home with four backends and two hand-written frontends. Every entry is a
line that could not be written with the CLI.

**A closed entry is deleted from this file, not marked done.**
`git log -p -- missing.md` is the record.

Closed and deleted, so a re-report can be recognised as one: `jails
architecture baseline` (the ArchUnit freeze store, granted through system
properties for one run so `archunit.properties` stays strict), `--set` (an
endpoint pins a component the caller must not choose), `--via` on a use case
(a write resolves its foreign key from a component of the parent), `--path` on
a scaffold (a collection whose URL is a fixed contract), `--if-match optional`
(a transition an ordinary browser page can reach, since `$.ajax` sends no
conditional header and Spring answers 400 before generated code runs),
`--bind` (a request parameter whose name is neither the component's nor its
snake_case -- the brief's own page reads `message.id` and posts `message_id`),
the `websocket`
capability (`jails g socket <Name>` is the slice, and `jails add websocket`
now says so), first-class `devtools` (one dependency and no code, so `jails
add dependency` is the verb -- both `run --watch` and `doctor` name it now),
H2's console and safe URL defaults (`jails db`, `jails db --web`, and two
`doctor` checks), WebSocket route discovery, `@Value`/SpEL parsing in
`jails beans`, a `GET` reading its filters from the query string
(`--consumes form` on `g query` now decides the verb as well as the binding,
so `GET /admin_api/conversations?status=open&category=Billing` is reachable),
and a `transition` whose key is a path variable (`--select userId --path
/admin_api/conversations/{userId}/status`, so `PATCH` at the URL an admin
frontend actually calls is one command). `jails fmt` on Gradle is a *deliberate* refusal, recorded in
`research.md` §5.1, not a gap.

---

## Code generation and architecture helpers

### A fallback handler for the exception nobody wrote a rule for

**Most of this shipped and the last arm did not.** `jails add api` writes
`api/ApiExceptionHandler.java`, a `@RestControllerAdvice` extending
`ResponseEntityExceptionHandler`, so the common web exceptions and validation
binding errors are RFC 9457 `ProblemDetail` already, and `DuplicateKeyException`
is a 409. That is the entry as it was originally written, minus one thing.

- What is not there is `@ExceptionHandler(Exception.class)`. An exception no
  rule names still reaches `BasicErrorController` and comes back as a generic
  500 with no `type`, no `detail` and nothing in the terminal a reader can
  match to the request that caused it -- which is the failure this entry was
  reported for, since a `@PathVariable` mismatch is exactly the kind of thing
  no rule names.
- It is deliberately the last arm to write rather than the first: a fallback
  that swallows everything is how a stack trace stops reaching the log at all,
  so it has to log before it answers, and what it answers must not leak the
  message of an exception nobody vetted.
- **Expected**: `add api` writes the fallback too -- log at `error` with the
  request path, answer a `ProblemDetail` carrying the status and nothing else.

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

- `--consumes json` and `--consumes form` each work, and `--bind` now names the
  request parameter a form field arrives under. What is still unexpressible is
  *one endpoint that accepts both*: `@RequestBody` and `@ModelAttribute` are
  different method parameters and a handler cannot carry both, so answering
  both content types means two handlers over one command, or a binder that
  reads whichever arrived.
- Real pages send form-urlencoded and real API clients send JSON, and the
  minicom frontends are the first kind, so the shipped half is the one the
  brief needs. This is what is left.
- **Expected**: `--consumes json,form` generating one route that binds either,
  with the generated proof posting both.

### In-Memory / Room-Based Presence Generators
- `jails g presence` generates PostgreSQL cluster-backed presence, but lightweight in-memory group/room chat presence (e.g. admin online tracking per customer email channel) is a common pattern that lacks a generator recipe.
- **Expected**: A `socket-presence` recipe for room-based presence and lifecycle events.

---
