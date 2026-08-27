<!--
plan.md — the working checklist. Recreated 2026-08-26 after the original was
deleted and folded into `pending.md` (itself since deleted in `2f8003b`).

**Item identifiers are `P<phase>.<item>`, deliberately.** Roughly 208 source
comments cite the *old* `plan.md` by section number (`plan.md §R6`,
`plan.md §19.2`) and resolve through
`git log --diff-filter=D -- plan.md` then `git show <commit>^:plan.md`.
`P3.1` can never be confused with `§R6`, so both citation styles keep working.

A checked box is a shipped commit. A closed entry is *deleted* from its source
document — `bugs.md`, `missing.md`, `modern.md`, `research.md` — in that same
commit, per the convention all four use.
-->

# plan.md — closing bugs.md, missing.md, modern.md, research.md

## Context

Four open documents describe what jails does not do yet, and they overlap far
more than their separate filenames suggest. `bugs.md` has two live reports;
`missing.md` has eleven entries from rebuilding six real minicom projects with
nothing but jails commands; `modern.md` assesses the resulting Java against
`java.md`/`backend.md` and separates *"jails rendered a bad field spec
faithfully"* from *"jails does this whatever you type"*; `research.md` is the
remaining product direction.

Read together, most of the entries are symptoms of **seven** causes. Fixing the
causes closes far more than fixing the entries would, and `modern.md` §13.11
already did the hard half of that analysis — its right-hand column is the list
of defects that survive a perfect field spec, and every one of them is
reproducible from a clean `jails new`.

The intended outcome: every entry in `bugs.md`, `missing.md` and `modern.md`
closed, and `research.md`'s remaining sections delivered — shipped to `main` in
small green commits rather than one large branch.

**On the filename.** Roughly 208 source comments cite a deleted `plan.md` by
section number and resolve through `git show <commit>^:plan.md`. This file uses
**`P<phase>.<item>`** identifiers so a new `plan.md P3.1` can never be confused
with a historical `plan.md §R6`. The header of the committed file will say so.

---

## The seven causes

| | Cause | Symptoms it accounts for |
|---|---|---|
| **A** | One concept, three renderings, none of them recorded | modern §11.1, §3.1, §3.3, §13.5; research §3.10; half of §11.4 |
| **B** | Nobody owns identity assignment | missing M3, M6; modern §7, §8.2, §8.3, §11.2, §13.3; bugs.md's POST note |
| **C** | The closed set is closed in Java and open in SQL | modern §4.5, §11.3, §13.4 |
| **D** | A generator ships something broken and the suite cannot see it | missing M1, M2; modern §2, §11.6, §13.2, §13.6, §13.7, §13.8; research §0.2 |
| **E** | Generated prose asserts what the code does not do | modern §8.1, §8.2, §11.5, §13.9 |
| **F** | Evolution regenerates the schema, not what was derived from it | missing M1b; modern §8.3, §11.4 |
| **G** | Primitives every one of the six real projects needed | missing M4, M5, M6, M8, M9, M10 |

`modern.md` §13.1 proves A and B are worth the churn: the same tool given one
better character produced the good corpus, so **the input that produces a table
with no primary key should be refused.**

## Working discipline (every checked box)

- One item ≈ one commit. `cargo build --workspace && cargo test --workspace &&
  cargo install --path .` green, `cargo fmt --all`, `cargo clippy --workspace
  --all-targets` clean, then push to `main`.
- **Delete the closed entry from its source document in the same commit** —
  `bugs.md`, `missing.md`, `modern.md` and `research.md` all use the
  delete-don't-mark convention, and `git log -p -- <file>` is the record.
- Tick the box here in the same commit. This file is the todo list.
- A new refusal or kind gets its `SCENARIOS` row (`tests/common/scenarios.rs`);
  there is no fourth list.
- Verified against `deps/`, never from memory, for anything version-shaped.

---

## P0 — land what is already written (uncommitted work in the tree)

`crates/jails-report/src/schema_lineage.rs` (new, 275 lines),
`managed_drift.rs`, `jails-commit/src/{execute,store}.rs`,
`jails-engine/src/route/lifecycle.rs`. Manually verified already: a torn write
is rolled forward by the next unrelated command, `doctor` names the pending
transaction instead of green-lighting it, `resource repair` refuses while one
is open, and a deleted or edited sealed migration is reported.

- [x] **P0.1** Fix the three gates. `("jails-report", "schema_lineage", 7)` is
      already added to `LAYERS` in `tests/architecture/rules.rs`. Remaining:
      the `doctor` module-lines ratchet (1479 vs ceiling 1477 — raise once with
      the reason recorded beside it in `tests/architecture/board.rs`), and
      `core_generation_stays_free_of_showcase_vocabulary` (add the new files to
      the `ledger` allow-list in `tests/genericity.rs` with a reason).
- [x] **P0.2** Regression tests: an interrupted transaction recovered on the
      next command; the seal + lineage checks. Writing the first one surfaced
      the real shape of B18: recovery inside `execute::commit` can only tell
      its caller the plan is stale, and **twelve routes call `commit_set`
      directly** with no replan loop to catch that — so a torn write answered
      "run the command again" forever. `route::finish_interrupted` now runs
      once in `dispatch::mutate_confirmed`, before any route reads the store,
      and a refusal after it still says what recovery finished.
- [x] **P0.3** Deleted **B18** and **B5/B14** from `bugs.md` — every numbered
      report in that file is now closed — and research.md §0.1, §0.3 and
      roadmap items 1, 2 and 4 (§0.3's manifest routing landed with B20/B22).

## P1 — the controls, as a block (cause D)

Chosen deliberately over pairing each with its fix: land all four so the whole
surfaced defect list is visible before any generator changes. Each is expected
to go **red on landing**; P1.5 records what they name.

- [x] **P1.1** A combined-kind tier-3 scenario. One project on the **Boot 4
      default** flavour with `scaffold` + `enum` + `strategy` + `usecase` +
      `association` + the default capability set, compiled and run with real
      `mvn test`. The existing scenarios exercise one kind on one flavour,
      which is exactly why missing M1 and modern §2 are invisible. Extend
      the shared Spring **core toolbox** (`tests/cli/main.rs`), which already
      generates many kinds into one project and runs real `mvn test` over it —
      it was missing `strategy` and `enum` beside its `scaffold`, which is
      exactly the pair M1 needs. It also had a second hole: the toolbox cache
      is keyed on the product binary, so adding a step to that list reused the
      previous tree and never ran it. Salted with the harness text now.
- [x] **P1.2** The default-variant execution gate (modern §11.6's
      generalisation). `add cors` *is* run through real `mvn test` — against
      `write_spring2_fixture`, a Boot 2 project, so the Boot 4 branch every
      real project gets has never been compiled. Assert that **whichever
      variant a version-sniffing template renders for the current default is
      the one executed**. Same failure mode as `JAILS_REQUIRE_TOOLCHAIN`, one
      level up. Landed as
      `every_version_sniffed_rendering_names_where_its_default_branch_runs` in
      `tests/architecture/rules.rs`: a scanner finds every production file that
      branches on the framework version, and each must name the test that runs
      the branch it takes on the current default — checked to exist. It found
      `add cors` and `add h2` uncompiled on Boot 4, and `add h2` writing a
      `java.sql` test outside `adapters`, which is a red build against the
      ArchUnit rule `g scaffold` installs.
- [x] **P1.3** The `@Disabled` honesty gate (modern §13.8). A generated
      `@Disabled` test reports green exactly as a skipped tier-3 test does.
      Either the command's summary names what it could not assert, or no test
      file is written. `minicom-2025-12-13` has five of nine disabled,
      including both controller tests, and reports green. Answered on both
      surfaces: a `test-disabled` warning derived from the *bytes* in the one
      report projection every command goes through (so a generator added
      tomorrow gets it without knowing it exists), and a `doctor` warning over
      recorded test output, which keeps answering after the summary scrolls
      away. A warning and never a failure — the file is exactly what jails
      meant to write, and the work it names is the reader's.
- [x] **P1.4** The `fix:`-command conformance test (research §0.2). Extract
      every `fix:` command the scenario suite emits and assert it does not
      immediately refuse. Landed statically, which is stronger than running
      them: every backticked `jails …` in a production message is checked
      against `jails commands --json` — the parser's own walk, not a second
      list. It found the frozen-conflict message telling readers to run
      `jails continue`, a verb that has never existed, and it found
      `jails commands` itself stopping at depth one, so `remove fast-test`,
      `resource field add`, `app apply` and `db console` were absent from the
      surface it claims to describe (and from `jails.nvim`'s completion).
- [x] **P1.5** research.md §0.2 deleted. The controls surfaced, and this
      session closed: `g strategy` vs the ArchUnit rule (M1); `--package`
      emitting an unimportable signature (M1a); `destroy` reporting a
      `--package`-placed resource as never generated (M1b); `add cors` and
      `add h2` never compiled on the default Boot version; `add h2` writing a
      `java.sql` test outside `adapters`; `add cors` going red the moment its
      origin is configured; generated `@Disabled` tests reported as green;
      `jails continue` named and never built; `jails commands` describing one
      level of a nested surface.

## P2 — fix what P1 surfaced (cause D)

- [x] **P2.1** `add cors` — one command, red build. It writes
      `app.cors.allowed-origins=http://127.0.0.1:8008,…` and a test asserting
      `https://example.invalid` is allowed. The reproduction is narrower than
      the report: a *fresh* `add cors` is self-consistent, and goes red the
      moment the origin is replaced — which `.invalid` exists to demand. So
      the test reads `app.cors.allowed-origins` out of the context instead of
      restating the value baked in at generation time. One source was the
      right instinct and the wrong source.
- [x] **P2.2** `g strategy` vs jails' own ArchUnit rule (missing M1 / modern
      §13.2). `g scaffold` writes `DOMAIN_HAS_NO_FRAMEWORK_DEPENDENCIES`;
      `g strategy` writes `@Component` implementations into `domain..`, and the
      `@Component` is load-bearing. Two first-party generators disagreeing
      about the domain boundary, and the disagreement is a red build. Resolve
      by placing the beans in `service`/`adapters` and keeping `domain`
      framework-free. `crates/jails-generate/src/generate/recipes.rs:584` is
      the kind's arm.
- [x] **P2.3** M1a — `--package` never imports the `--on`/`--yields` types, so
      the workaround does not compile either. Route the strategy renderer
      through the same `import_of` helper the scaffold uses for cross-package
      imports; it already returns an empty string when the packages match.
- [x] **P2.4** `add db` has no Spring Boot floor (missing M2). Four spliced
      coordinates do not exist on Boot 2.7 (`spring-boot-flyway`,
      `flyway-database-postgresql`, `spring-boot-testcontainers`,
      `spring-boot-docker-compose`), and `MANAGED` is chosen on whether Boot
      manages dependencies rather than on the Boot **version** —
      `crates/jails-generate/src/add/database.rs`. Refusing everything below
      Boot 4 would degrade politely where working is possible, which
      `CLAUDE.md`'s Gradle note argues against, so `add db` picks the module
      set the project's version *has*: three boundaries, each checked in
      `deps/spring-boot` — testcontainers and docker-compose at 3.1,
      `flyway-database-postgresql` managed at 3.3, `spring-boot-flyway` only at
      4.0. Below 3.1 it refuses by name. Needed a Boot `(major, minor)` reader;
      the major alone chooses an import and cannot choose a module set.
- [x] **P2.5** `g client` writes a remote call with no timeout, no base URL and
      no defined failure mode (modern §13.6). `backend.md` §1 admits no
      exceptions here. Write `spring.http.client.connect-timeout` /
      `read-timeout` and a `…base-url` alongside, from the plan, the way
      `ensure_failsafe` and `ensure_assertj` are written from the write path.
      Prefix and both keys checked in `deps/spring-boot` v4.0.0
      (`HttpClientProperties extends HttpClientSettingsProperties`). The base
      URL is `https://example.invalid` on `add cors`'s reasoning: RFC 2606
      reserves it, and the alternative failure is a first call dying on
      `URI with undefined scheme`, which says nothing about a missing setting.
      The *shape* half of §13.6 stays open as missing.md M7.
- [x] **P2.6** `g migration` writes a file whose whole content is
      `-- Forward-only migration. Write explicit SQL below.`; Flyway applies it
      and records the checksum, so the history asserts an index that does not
      exist (modern §13.7). Refuse to write an empty one, or write it in a form
      Flyway will not apply until it has content.
- [x] **P2.7** `add api` installs a sealed `ApiException`, an exhaustive
      handler and 40 lines of Javadoc, and **nothing throws it in 0 of 7
      projects** (modern §6.1, §11.7, §13.10) — while the one operation with
      real failure modes hand-rolls `ResponseStatusException`. Wired: a
      generated transition raises `ApiException.NotFound`/`.Conflict` when the
      project declares the type, read through the projection so `add api` and
      `g transition` in one manifest apply see each other. The other branch is
      not a tidiness fallback — without `add api` the class does not exist and
      nothing else compiles, the same rule `repository_wiring` follows for
      `JdbcClient`. Both branches are compiled and run.
- [x] **P2.8** `jails migrate lint` and `jails schema diff` require
      `.jails/app.toml`, so neither runs on the shape `jails new` produces.
      `migrate lint` is closed: it wanted the manifest for the dialect alone,
      and the driver the project declares is the same authority
      `Project::sql_dialect` uses everywhere else. `schema diff` still needs
      one, and that half is real work — its *declared* authority is the
      manifest's entity list, and the equivalent over the ledger's recorded
      specs does not exist yet. Carried into P9.

## P3 — the naming and binding spine (cause A)

Full convergence, as chosen. Expect large golden churn; regenerate in the same
commit as the change that causes it.

- [x] **P3.1** `FieldName` in `jails-protocol` owning both renderings:
      `java()` → lowerCamelCase, `column()` → snake_case. `user_id:uuid` and
      `userId:uuid` converge on Java `userId` + SQL `user_id`; a spec name that
      cannot produce a Java identifier by convention is the only error case.
      `sql::snake_case` already exists
      (`crates/jails-generate/src/sql.rs:376`) and is the SQL half; the Java
      half is currently the raw spec string. Templates lose access to the raw
      name. Closes modern §3.1, §3.3, §11.1. Landed with the column as the
      **normal form** and the Java name derived from it, which is what makes
      `user_id`, `userId` and `user_ID` one field rather than three. That in
      turn made the mapping's reversibility load-bearing, so a word not
      starting with a letter (`_id`, `a_1b`) is the refusal — without it
      `a_1b` and `a1b` reach a record as two components of one name. The
      column-collision refusal and the duplicate-name refusal collapsed into
      one branch, since two distinct `FieldName`s can no longer share a
      column.
- [x] **P3.2** Recorded `ColumnBinding` — a `(EntityId, field name) → column
      name` pair per managed entity, written at create time and consulted by
      every SQL projection instead of re-deriving. `TableBinding` already does
      this at the entity level. Unblocks research §3.10 `--column preserve` (a
      ledger edit with no migration; `single-cutover` becomes that edit plus a
      forward `alter table … rename column`) and gives P0's coherence check a
      binding to compare *through*. Delete research.md §3.10 and roadmap item 5.
      Landed as the pair inside `FieldName` rather than a table beside it: the
      binding is per `(EntityId, field)` because it rides in that entity's
      recorded `FieldSpec`, which is one place rather than two.
      `@column(<sql_name>)` is the canonical spelling, emitted **only** when
      convention can no longer produce the column — without it every re-plan
      through `evolve_existing`'s canonical field tokens would derive the
      column straight back. `Field` carries the column now and `sql::column`
      reads it, and the one `snake_case` moved down to `jails-java` so
      `jails-spec` and `jails-protocol` share it rather than owning a copy
      each. The ledger payload codec is `-3`; `-1` and `-2` are refused by
      name, since a payload carrying only the Java half has no second value
      to promote.
- [x] **P3.3** `findById` is typed on the primary key, not on `String` (modern
      §13.5 — 11 of 12 generated ports, and in `my-minicom` two ports over two
      tables in one app disagree). Thread the `@pk` field's Java type through
      `crates/jails-generate/src/generate/repository.rs` and the ~10 templates
      under `templates/spring/` that hardcode `findById(String)` /
      `String.valueOf(x.id())`. Landed as `KeyType`, derived once per resource
      and handed down, because the port, both adapters, the in-memory fake,
      the service, the controller and five dependent templates all have to
      agree and none of them can see the others. Three things fell out of it.
      The type carries **two sample values**, since a generated test has to
      say "this one is there and that one is not" and a key type with no way
      to write one down produces a test that does not compile — so a key stays
      untyped unless a pair exists, which is also what keeps `Duration`,
      `URI`, `Path`, `boolean` and `double` out (none is a sane URL path
      segment). The in-memory adapter now keys on the **repository's** key
      component rather than on `id`, so it and the JDBC `where` clause stop
      disagreeing. And `cast(:id as uuid)` / `setString` are gone wherever the
      parameter is already the column's own type — they existed only to undo
      the text port.
- [x] **P3.4** Names that carry no meaning (modern §3.2): the
      `Message_userAssociation` underscore, `Default…UseCase`, `…QueryPort`.
      `backend.md` §8 bans `Helper`/`Manager`/`Util` for the same reason.
      Narrow, template-only. The underscore is closed at the *identity*
      boundary rather than in the association template — `recorded_name`
      capitalises the first letter and stops, so any kind whose name becomes a
      Java type had it — and refused rather than normalised, because
      `Message_user` could mean `MessageUser` or `MessageBelongsToUser` and
      only the reader knows which. `{X}QueryPort`/`{X}Query` became
      `{X}Query`/`{X}Criteria`, so `Jdbc{X}Query implements {X}Query` reads
      the way `Jdbc{X}Repository implements {X}Repository` already does.
      `Default{X}UseCase` became `Storing{X}UseCase` — what it does, against
      `Outbox{X}UseCase`, which stores *and* stages. **modern.md §6.4 stays
      open**: with no `--yields` the port still has exactly one
      implementation, which `backend.md` §8 bans outright, and collapsing the
      two is a structural change rather than a rename.

## P4 — who assigns identity (cause B)

- [x] **P4.1** `g scaffold` **refuses without a primary key**. `rails g
      scaffold` gives you one whether you ask or not because it is not a
      preference; jails made it opt-in and a project that did not opt in got
      two tables with no primary key, a `findById` that throws if two rows
      match, and a compare-and-swap presented as atomic over a multi-row update
      (modern §4.1, §11.2). `create_table` already emits `primary key (id)`
      when an `id` column exists — the gap is the refusal. **Already shipped**
      when this phase was reached: `scaffold::require_single_primary_key`
      refuses both an implicit identity and a composite one before writing,
      and `scaffold_refuses_an_implicit_or_composite_identity_before_writing`
      pins both. That closes §4.1's third bullet too — with a declared `@pk`,
      `migrations_declare_unique_key` finds the primary key and stops adding
      `create unique index … on users (id)` just to give a foreign key a
      target. Only the documents needed changing.
- [x] **P4.2** An assignment policy on the pk field —
      `ClientSupplied | ServerGenerated | DatabaseGenerated` — consumed by
      `create_table`, the use case, the request DTO and the in-memory fake.
      `long@pk`/`int@pk` get `generated always as identity` and
      `insert … returning`, and the use case stops naming the id. Today
      `crates/jails-generate/src/spring/workflow.rs:280` hands `0L` to every
      create over an integer key, in every project, so the primary create path
      works exactly once and the generated test asserts
      `assertThat(created.id()).isNotNull()` on a primitive `long`
      (missing M3, modern §13.3).

      Landed as `sql::Assignment` plus `sql::generated_key`, derived from the
      key's *type* rather than configured: an application can write a UUID and
      cannot write a unique integer without asking the database, and a
      database can assign an integer and has no business inventing a UUID.
      Four consequences worth recording.

      **The port's `save` returns the stored row.** `void save` cannot carry a
      key the database assigned, and two shapes of one port would be worse
      than one — so it returns for every key type, and the Javadoc says which
      of the two facts applies.

      **`getGeneratedKeys`, not `returning`.** `insert … returning` is one
      round trip and PostgreSQL-only; H2's parser has no such clause and
      `Project::sql_dialect` treats H2 as a supported target. Both adapters
      use JDBC's generated-key retrieval and rebuild the record around it,
      which costs nothing because every other component is already in hand.

      **`g usecase` refuses to name a database-assigned key.** Without that
      the component is accepted, rendered into the record, and dropped by an
      insert that omits the identity column — a create that reads as
      honouring the caller's id and silently does not.
      `examples/minicom/.jails/app.toml` declared exactly that and now does
      not.

      **Every generated test that wrote a key down had to stop.** The
      repository round trip, the query IT, the transition IT and the
      association probe all inserted a literal key; a `generated always as
      identity` column refuses one outright, and the sequence does not roll
      back with the transaction, so a literal that passes once fails on the
      next run. They all read the saved row now. The one M3 asked for is
      new: `twoCreatesAreTwoRows`, emitted only where the use case assigns
      the key, because a command that carries it is `ClientSupplied` and two
      identical commands are then one row on purpose.
- [x] **P4.3** The request DTO stops asking the client for server state
      (modern §7). `POST /messages` currently requires the primary key, the
      timestamp, the read flag *and* the optimistic-lock version — the exact
      defect the generated test's own Javadoc describes and then commits.
      Closes bugs.md's "POST body invents the id" note. Three components are
      withheld now — the audit pair, an assigned primary key, and a required
      numeric `version`, recognised by the same rule `g transition` uses to
      find it. Two exceptions are deliberate and each was a real generated
      project: a **`@scope`** component is proved against the caller's own
      token, so it is exactly what the caller must send even when it is the
      key (`support-inbox` declares `id:uuid@pk@scope`); and a key **not
      named `id`** is a natural one the caller chose, which is the convention
      `usecase_default` has always used for the `String` case, stated once in
      `sql::key_assignment` now. Identity moved out of the web layer at the
      same time: `{X}Service.create` mints it, and `toDomain` writes a
      documented placeholder nothing reads. `g dto` passes no assigned key at
      all — it owns no table. modern §7's other three entries are read-side
      and stay open.
- [x] **P4.4** `uuidv7()` where the database supports it, not
      `gen_random_uuid()` (modern §4.2 — `backend.md` §5 names random UUID keys
      specifically), and `order by` a real ordering column rather than a random
      id (§4.4). Landed the other way round on the first half: the database
      does not assign the key here — the *application* does — so the fix is a
      generated `TimeOrderedUuid`, version 7 per RFC 9562, and every mint goes
      through it. That is the `g auth` / `add sse` shape: the JDK has no v7
      factory, `UUID.randomUUID()` is wrong in a way nothing reports, and the
      generated test is what keeps it fixed because a generator that went back
      to version 4 would look identical at every call site. Making the *column*
      default to `uuidv7()` was rejected: it needs PostgreSQL 18, has no H2
      equivalent, and would turn every UUID key into
      `Assignment::DatabaseGenerated`, which refuses the client-supplied
      `id:uuid` that is the idempotent-create idiom `examples/web-crawler`
      uses. `sql::ordering` closes §4.4: newest first by whichever timestamp
      the table has, with the key as the *tiebreak* rather than the sort, so
      two rows written in the same instant do not swap between two identical
      requests. `gen_random_uuid()` survives in exactly one place — the
      one-shot backfill `add_column` writes and then drops — where locality is
      not a property a single `update` has.
- [x] **P4.5** The version travels as an `ETag` / `If-Match` / `412` rather
      than as a bespoke JSON field (modern §7, §10.5), and expected outcomes
      are a sealed return type rather than two exception classes (§5.3, §10.3):
      a caller that forgets a `catch` finds out in production, one that forgets
      a `switch` arm does not compile. Both halves landed together because
      they are one rewrite of `g transition`. The port returns a sealed
      `Result` with `Applied`, `StaleVersion(current)` and `NotFound(id)` —
      `StaleVersion` carries the stored row, which is what lets the 412 serve
      its `ETag`, so the adapter's `select exists(…)` became a `select` of the
      row. `version` left the command record and became a second parameter,
      because an expected version is a precondition rather than data; it
      arrives as `If-Match` (weak prefix and quotes accepted, since that is
      what a client echoes back) and returns as `ETag`. §5.3's third point
      closed with it: "in the authorized scope" is now printed only where a
      `@scope` field exists, and the outcome records carry values instead of
      one string shared by every 404. P2.7's `ApiException` wiring survives —
      the switch *arms* raise into it where the class exists — and
      `ResponseStatusException` survives for exactly one thing, a malformed
      `If-Match`, which is a 400 rather than any of `ApiException`'s
      variants.

## P5 — the closed set, in SQL (cause C)

- [x] **P5.1** An enum column emits `check (col in (…))`. Zero `check (`
      appears in 20 migrations across all seven projects, and this is **not** an
      input problem: the user declared `g enum`, jails generated the Java enum
      and the column, jails holds the constant list, and still wrote a column
      that accepts `'banana'` (modern §13.4). `backend.md` §5 makes it the
      highest-value line in the file. The constraint is **named and
      table-level** rather than an inline column check, because P5.2 has to be
      able to replace it and PostgreSQL's automatic name is an implementation
      detail. `add_column` emits it too: when a field was declared is not a
      fact about the domain. An enum jails cannot read gets no check at all —
      a guessed list would reject a value the Java enum accepts, at
      `flyway migrate`, on whichever machine runs it first. The association
      probe's row had to stop using `'association-probe'` for such a column,
      which is the constraint doing exactly its job on the first run.
- [x] **P5.2** `g enum` adding a constant generates the
      `alter table … drop constraint … add constraint …` migration in the same
      step — the follow-on question P5.1 creates, answered rather than avoided,
      through the append-only sealing machinery `resource field` already uses.
      Which tables is read off the *source*, the same way `destroy strategy`
      reads its implementations — a record written by hand against a generated
      table is still a column with this constraint on it — and gated on a
      `create_<table>` migration existing, without which a plain `g record`
      would get an `alter table` that is unappliable everywhere and reported
      nowhere. **A removal is refused rather than migrated**: a stored row may
      still hold the dropped constant, jails cannot ask the database from
      here, and the `add constraint` would otherwise fail at
      `flyway migrate` about a command that reported success. Re-declaring the
      same set writes nothing, so a re-run stays idempotent.
- [x] **P5.3** A `String` field with a small closed set is challenged
      (modern §11.3). `direction:String!` produced an unconstrained column, an
      unconstrained record, and a `"sample"` fixture; jails already has
      `g enum` and its own example manifest uses one here. A **warning and
      never a refusal**: jails cannot know a `String` has a closed set, only
      the reader can, and what a tool can do is notice the shape and name the
      command. Detected on the emitted migration's bytes, in the one
      projection every command goes through, so a kind added tomorrow is
      covered. The name list is deliberately short and matched on the whole
      trailing word — a longer one warns about ordinary text, a substring
      match warns about `statuses_note`. It also closed the hole P1.3 left:
      `AppliedReceipt` carries the warnings now, because one that appears on
      `--pretend` and vanishes on the real run is one nobody sees.
- [x] **P5.4** The schema's remaining non-negotiables (modern §4.7):
      case-insensitive unique index on an email-shaped `@unique`, a
      `check (length(btrim(x)) > 0)` where the Java constructor rejects blank,
      and either an explicit reason for `deferrable initially deferred` on
      every generated FK or a different default (§13.10). Took the *explain*
      branch on the foreign key, and for a measured reason rather than
      taste: switching to an immediate check turned three generated
      integration tests red at once, because a `@Transactional` test that
      inserts a child and rolls back never reaches the commit where a
      deferred violation surfaces. That is worth knowing — those tests were
      green over rows PostgreSQL would never have accepted — but it is a
      question about what a generated child test should seed, not about the
      constraint, so the constraint keeps the default and states both what
      the deferral buys and what it costs. `on delete no action` is stated
      the same way, including that `restrict` is never deferred and so gives
      up the other line. Only §4.7's `--timestamps` bullet survives, and it
      is an input problem.

## P6 — the prose, and the real bugs behind it (cause E)

- [x] **P6.1** The Kafka partition key is unique per record and the Javadoc
      claims it gives "ordering per entity" — the exact behaviour it prevents
      (modern §8.1). Key on the entity id. `backend.md` §4: *"The partition key
      is the design decision."*
      *Done:* `g event <Name> --on <Entity>` keys the publisher on the
      payload's `<entity>Id` component — the same convention `usecase
      --yields`, `association` and `durable-job` already read — and refuses
      when that component is missing or optional. No `--on` means the key
      stays the event id and the Javadoc now *says* there is no per-entity
      order, naming the flag that would give one, rather than claiming one the
      code never had. jails does not pick a component by looking for a name
      ending in `Id`: an event carrying both `userId` and `accountId` has two
      defensible answers and only the caller knows which. The
      `outbox-http-sink` scenario carries `--on Message` so the ordered branch
      is in the goldens.
- [x] **P6.2** The event id **is** the message id, and the outbox stages
      `on conflict (id) do nothing`, so a second event about the same entity is
      silently discarded (modern §8.2).
      *Done:* the event's own `id` is now **minted** by the outbox
      (`TimeOrderedUuid.next()`), never mapped from the command or the target.
      `<target>Id` is how an event refers to the resource; `id` is how it
      refers to itself. Two generated tests were passing on the coincidence:
      `{Usecase}OutboxIT` looked its row up by `result.id()`, and
      `{Name}MessagingIT` asserted on whichever record reached its probe
      first — which, with a fresh consumer group and
      `auto-offset-reset=earliest`, is whatever a neighbouring test left on the
      topic. Both now read the value they mean: the IT selects the staged row's
      id back out of the outbox table, and the probe waits for the event whose
      id it published. The second of those went red on the real broker the
      moment the ids stopped agreeing, which is the whole argument for the
      end-to-end tier.
- [x] **P6.3** The outbox relay ceiling is one event per second (`claim()` is
      `limit 1` on a `fixedDelay=PT1S` worker), there is no jitter on the
      backoff, and a multi-sink partial failure re-sends a Kafka publish that
      succeeded (modern §8.4).
      *Done:* three fixes in the generated relay. `claim(batchSize)` leases a
      batch and `runOnce()` drains until a short batch says the runnable set is
      exhausted, so the ceiling is `batch-size` per tick
      (`outbox.<usecase>.batch-size`, default 100) rather than one; it
      terminates because every claimed row either succeeds or has its next
      attempt pushed forward. The retry interval keeps `2^attempts` capped at
      five minutes but scales it by a random factor between a half and one, so
      every row staged in one incident no longer retries at the same instant.
      And a `delivered text[]` column records which sinks accepted the event
      before the next sink is tried, so a retry skips them — without it a Kafka
      publish that succeeded is re-sent on every attempt a slower HTTP sink
      fails, and at-least-once quietly becomes at-least-`max_attempts`.
      `aSinkThatAlreadyAcceptedIsNotSentTheEventAgain` is generated per outbox
      and runs against real PostgreSQL (Failsafe count 70 → 72).
- [x] **P6.4** Delete or repair the claims that are false: "keyed on the
      `email` component", "ordering per entity", "scoped matches cannot mutate
      another tenant's row" (there is no scope in the SQL), "this type has no
      `id` component" (it has one). 27% of production Java is comment and a
      wrong explanation is believed. Add a ceiling on template comment density
      to `tests/architecture/board.rs`. The load-bearing ones — the
      `@ServiceConnection` explanation, the Failsafe note, the
      `DeadLetterPublishingRecoverer` default — stay (modern §11.5, §12).
      *Done:* three of the four claims had already been repaired at their
      source by earlier items — "ordering per entity" by P6.1, the scope
      wording by P4.5's `scope_clause`, and the in-memory adapter's "no `id`
      component" by P3.3's `key_component`. The fourth is repaired here: the
      repository Javadoc printed the *column* name, so a key called
      `customerId` was announced as `customer_id` — an accessor the reader
      goes looking for and does not find. `the_key_javadoc_names_the_component
      _not_the_column` pins both halves: the prose names the component, the
      SQL still names the column. The new gate measures the percentage of
      non-blank lines in `templates/**.java` that are comment; it is 21 and
      the ceiling holds it there rather than driving it down, because what
      remains is the load-bearing prose §12 asks to keep.
- [x] **P6.5** Two generators, two answers, one arguing against the other: the
      scaffold path sets both audit columns to one `Instant` and explains why,
      the use-case path calls the clock twice (modern §13.9). And a `usecase`
      defaults an enum **positionally** — `IssueStatus.values()[0]` — so
      reordering a `g enum` silently changes every generated create
      (missing.md, "two smaller things").
      *Done:* the use case hoists one `Instant now` whenever it fills in more
      than one timestamp, and the explanation is a single `dto::AUDIT_PREAMBLE`
      both paths read — one text and one rule, since two generators disagreeing
      about a decision is worse than either answer alone. The enum default and
      the enum *sample* both name the constant now
      (`ConversationStatus.OPEN`, `Currency.GBP`), falling back to
      `values()[0]` only where the constants cannot be read at all.
      `one_create_reads_the_clock_once_for_every_timestamp_it_fills_in` pins
      the first; the proof apps show it end to end.
- [ ] **P6.6** Delete `modern.md`. Every remaining entry is closed by here.
      *Not yet, and the premise was wrong.* P6.1–P6.5 and P7.1 closed §8.1,
      §8.2, §8.4, §11.1 and §11.2, and this pass also deleted §4.5 (the
      free-text closed set is a `free-text-closed-set` warning now, P5.3) and
      §5.2 (`findById` takes the key's own type, P3.3). What still stands is
      ten jails-side entries, none of them covered by an item above:

      - **§4.3** no index serves any query the application runs. jails could
        say so the way it says `free-text-closed-set` — a `query --on X` whose
        filter columns have no index is a shape it can see.
      - **§5.4** boxed primitives on the wire (`Boolean`, `Long` in a response
        describing a `boolean` and a `long`), then `@NotNull` compensating.
      - **§6.1** the service layer takes a concrete `Jdbc*` class *and* a
        concrete sibling implementation, under Javadoc saying it depends on
        interfaces.
      - **§6.3** `AppMetrics`, `CorsConfig`, `MetricsConfig` land in the root
        package because nothing decides where they go.
      - **§6.4** interfaces with one implementation, and `MessageService`
        forwarding four calls. P3.4 left this open deliberately.
      - **§6.5** two API styles in one service — REST for the scaffold,
        RPC-over-POST for the generated operations, including a `POST` to read.
      - **§7** three read-side defects: a command/query record bound directly
        as `@RequestBody`, a query named *unread* that takes `isRead` as a
        parameter, and a silent `MAX_RESULTS`.
      - **§8** the generated listener is a `TODO` that logs an id and drops the
        event.
      - **§9** the generated tests mostly test the framework: a service test
        that can only fail if Mockito breaks, an association IT that asserts
        Postgres recorded the FK the migration declared, every fixture value
        `"sample"`, and no concurrency test for the CAS the `version` column
        exists for.
      - **§13.6**'s surviving half is `missing.md` M7, tracked as **P8.8**.

      §1, §2, §3.2, §4.6, §4.7, §5.1, §10, §12, §13.1, §13.10 and §13.11 are
      either narrative, the hand-built reference, or input problems the file
      itself labels as such — they are the record of *why*, and they go when
      the ten above do.

## P7 — evolution keeps derived code true (cause F)

- [x] **P7.1** A generated file whose stated premise has become false is
      re-planned or reported, never left with a comment contradicting the code
      beside it (modern §11.4). `g field id` wrote `V004` and left
      `InMemoryUserRepository.findById` returning `Optional.empty()` with a
      TODO saying the type has no id — `findById` always empty, `save` keying
      on a colliding counter, `deleteById` removing a `UUID` from a
      `Map<String, …>` (modern §8.3). Extend the companion re-plan that landed
      for `g field` in `e3c7041`.
      *Done:* three things. The dependent set is no longer a three-kind
      allowlist matched on `--on`: it is every recipe that reads the target's
      component list off disk — `query`, `transition`, `usecase`,
      `association`, `durable-job` — matched on `--on` **or** `--yields`, which
      is how an association names its parent and a `durable-job` its resource.
      A companion is re-planned from its own argument *shape*, so an
      association's `child=parent` mappings are no longer read as an empty
      field list — that refused the whole unrelated `g field` with "association
      needs at least one `childField=parentField` mapping". And a regenerated
      companion no longer re-emits the migration its first generation already
      applied.
      The `InMemoryUserRepository` state itself is unreachable now — `g
      scaffold` requires exactly one `@pk`, and with a single key the fake and
      the JDBC adapter key on the same component (P3.3). What was left was the
      branch that produced it: three methods quietly doing the wrong thing
      under a comment explaining why, beside a JDBC adapter that failed
      explicitly on the same input. It throws now.
- [x] **P7.2** `--package` is not a one-way door (missing M1b). The report was
      that placement is unrecorded; it is not — `--package` is part of an
      entity's *identity*, deliberately, which is what makes slices possible.
      The defect was the refusal: a lookup miss reported the resource as never
      generated, seconds after the generate that printed `ledger replace`, so
      `jails history` + `jails undo` was the only way back to a state the error
      said was already there. The refusal names the recorded package now.

## P8 — the primitives the six real projects needed (cause G)

All three of `missing.md`'s named primitives, in full, plus the smaller entries.

- [x] **P8.1** M5 — `--via <Association>` on `g query`, letting one filter name
      a column on the parent. `g association` already reads both records and
      type-checks the field mapping across the boundary, which is exactly what
      a join needs and is used today only to emit a foreign key. Covers all
      four real endpoints in the table without inventing a query language.
      *Done, with one deliberate departure: `--via` names the parent **type**,
      not the association.* An association records its mapping only in the
      migration it wrote, and re-reading generated SQL to recover a decision is
      the guessing `build.rs` refuses to do with a build file. The join column
      is derived from the two records instead — `<parent>Id` when the child
      declares it, otherwise the single component of the parent key's type
      whose name ends in `Id`; two candidates is a refusal naming both. A
      joined select qualifies every column, including the target's own, and the
      generated IT saves the parent first and reads the child's foreign key and
      every parent-side filter off it, so the row it stores actually matches.
      `IntentSpec` gained `via`, which bumped the ledger payload codec to
      `jails-ledger-payload-4`. `minicom`'s manifest now carries
      `UnreadForEmail email:string! isRead:boolean --on Message --via User` —
      the Django original's whole customer-facing surface, and the endpoint
      M5 was written about — and it passes `jails check` against real
      PostgreSQL.
- [x] **P8.2** M5's smaller half — `--order-by` and `--limit` on `g query`.
      *Done:* `--order-by 'sentAt desc, id'` names components of `--on` (or the
      columns they map to — each spelling resolves to exactly one column, and
      refusing one of two unambiguous names would be arbitrary), with `asc`
      /`desc` and nothing else after a name, the same closed grammar `--index`
      uses so nothing arbitrary is recorded as trusted SQL. `--limit` replaces
      the built-in ceiling of 100, and `--limit 0` is refused since it can only
      ever return nothing. Shape-validated in `jails-protocol` and resolved
      against the target's components in the generator, the split `on`/`yields`
      already have — the layer that builds a spec holds the query's *filters*
      and never reads the target. `IntentSpec` gained `order_by` and `limit`,
      so the payload codec is `jails-ledger-payload-5`. `minicom`'s
      `UnreadForEmail` carries `order_by = "timeStamp desc"` and `limit = 20`,
      which is the Django original's `[:20]` on `-created_at`, and it passes
      `jails check`. `query.rs` crossed the largest-module ceiling on the way,
      so its planning half is `spring/query/shape.rs` now.
- [x] **P8.3** M6 — get-or-create by natural key: `--on-conflict <field>` on
      `g usecase`. The statement is one `g explain idempotency` already
      describes verbatim (`insert … on conflict (…) do nothing returning`);
      what is missing is a verb that applies it to a scaffold's own unique key.
      The single most repeated hand-written line across the six projects.
      *Done:* `--on-conflict <component>` replaces `Storing{X}UseCase` with
      `Ensuring{X}UseCase`, a `JdbcClient` adapter implementing the same port —
      the shape `g transition` already uses, because an operation whose
      atomicity lives in SQL is written where the SQL is. A port with a
      `save(T)` cannot express the clause, and read-then-insert reopens the
      window the single statement exists to close.
      Two things the real database taught. **The conflict target is not always
      the column**: P5.4's `@unique` email is indexed on `lower(email)`, and
      `on conflict (email)` finds no index — PostgreSQL refuses the whole
      statement. It is derived through `sql::case_insensitive`, the same
      function the DDL uses, so the two cannot disagree. And **jails cannot
      check that the column is unique**: a record read off disk carries no
      constraints, re-reading its own migration is the guessing `build.rs`
      refuses, and taking the caller's word verifies nothing — so the generated
      IT checks it against a real database, where it is a fact. `--on-conflict`
      with `--yields` is refused: the outbox delegates to the class this
      replaces. `minicom` carries `EnsureUser email:string! --on User
      --on-conflict email` — the first line of the Django ping handler — and it
      passes `jails check`.
- [x] **P8.4** M4a — a `WebSocketHandler`-shaped kind: the handler, its
      `WebSocketConfigurer` registration, and a test. Same shape as
      `g handler`. `add sse` covers the server→client half of read receipts and
      presence and none of the client→server half.
      *Done:* `g socket <Name>` (aliases `websocket`, `ws`) writes the handler,
      the registration at `/ws/<name>`, the test, and splices
      `spring-boot-starter-websocket`. Three things it decides rather than
      copies, each verified in `deps/spring-framework`: every session is
      wrapped in `ConcurrentWebSocketSessionDecorator`, because a
      `WebSocketSession` is not safe for concurrent sends and a broadcast is
      exactly that shape — the failure is `IllegalStateException: …
      [TEXT_PARTIAL_WRITING]`, load-dependent and never reproducible at the
      desk; a session that throws `IOException` is evicted, since letting it
      out stops the broadcast and swallowing it keeps the corpse; and the
      handshake stays same-origin, with the registration naming the line to
      change rather than changing it. The Spring toolbox generates one and runs
      `mvn test` over it, so the starter splice is checked by the compiler
      rather than asserted.
- [x] **P8.5** M4b (`missing.md` renumbered this from M4's second half) — the
      presence primitive. The Django original tracks admin
      presence in a module-level dict with a comment saying it only works
      because there is one process: the author knew it was wrong and shipped it
      anyway. An in-memory presence map is silently correct on one node and
      silently wrong on two, with no error either way — the same class of
      "the default is wrong in a way nothing reports" that `g auth` and
      `add sse` exist for, so the generated **test** is what keeps the fix in
      place.
      *Done:* `g presence <Name>` writes a port, a PostgreSQL adapter keyed by
      `(scope, member, node)`, the migration, the `@EnableScheduling` the sweep
      needs, and the IT that is the whole argument — two adapters are two
      nodes, one joins and the other is asked, which a module-level dict fails
      and a shared table passes. Two decisions beside it: a row per *node*,
      because a member connected twice is present until both claims are gone,
      and a `seen_at` window rather than a leave-only protocol, because a
      process that dies never sends `leave` and presence built on explicit
      departure is permanently wrong after the first crash. Domain-blind like
      `g idempotency`: scope and member are strings the caller picks.
      `minicom` carries `g presence Admin` and passes `jails check`
      (Failsafe 8 → 11 tests).
- [x] **P8.6** M9 — an index on an existing table: `resource index`, or
      `--index` on `g field`. `g field` can already add a *column* to a live
      table with a data plan, which is the harder problem; an index has no data
      plan to argue about, and `sql::validate_index` already parses
      `'created_at desc'` into column plus ordering.
      *Done:* `jails resource index add <Entity> '<columns>'` — one forward
      migration, the columns checked against the table before anything is
      written, and the index recorded on the entity so a re-plan reproduces it
      and a second attempt at the same one is refused. It is named in the same
      `{table}_idx{n}` series a `create table` uses, from one `declared_index`
      both call, so two commands cannot give two indexes one name.
      *It also uncovered a live two-spellings bug.* `IndexSpec` records
      **fields**, deliberately, so every path handing a recorded index to a
      generator has to render it back to columns — and one did not: `app
      apply` passed the manifest's own column tokens while a re-plan passed
      `IndexSpec::canonical()`, which is camelCase. The create migration is
      one-shot so it never surfaced, until this command re-planned a scaffold
      and `validate_index` reported "no column 'customerId' in this table" over
      a table that has it. `request::as_column_names` is the one renderer now,
      and it reads the field's own recorded binding rather than `snake_case`,
      because a `@column(...)` override is exactly where the two differ.
- [x] **P8.7** M8 — `--path` on `g controller`, `g usecase`, `g query`. Derived
      paths are a virtue greenfield and unusable when the URLs are a fixed
      external contract. The derivability argument does not block it: `destroy`
      finds files by what the ledger recorded, so a recorded `--path` is no
      harder to undo than `--package` is meant to be.
      *Done:* `RoutePath` is a validated protocol value rather than a
      passthrough — it is text jails writes into an annotation, so a leading
      `/` is required, `..` is refused and the charset is Spring's own path
      grammar. The derived paths are unchanged where nothing names one, which
      the goldens confirm byte-for-byte. `minicom` answers `/customer_api/ping`
      and `/customer_api/read` now, which is what `foo-website/foo.js`
      hardcodes, and it passes `jails check`.
      *Noted on the way:* `IntentSpec` has taken five optional refinements in
      this phase and is now large enough that clippy asks for the enum around
      it to be boxed. It is deliberately not boxed — `Intent` is the variant a
      ledger is almost entirely made of, so the indirection would pessimise
      every row to save bytes on the few that are not intents — and the real
      answer when it costs something is to group the refinements rather than
      box the enum.
- [x] **P8.8** M7 — `g client` takes `--method`/`--on`/`--returns` (and the
      P8.7 path), so it generates the call the project makes rather than a REST
      collection to delete. The `HttpClientsConfig` / restclient splice it
      already writes alongside is the valuable half and stays.
      *Done:* naming any of the three switches the interface from the CRUD
      collection to the one call described — `@PostExchange("/v1/chat/
      completions") ChatReply call(@RequestBody ChatRequest request)`. `--path`
      alone still just renames the collection's base path, which is a
      different and coherent thing to want. The generated test is whole and
      `@Disabled` with a `sample()` that throws, the same rule `g controller`
      follows: jails has no type model, so a body it invented would be a test
      of its guess. `--method` is now `Optional` for `client` in the recipe
      metadata, so the arity table says what the CLI does.
      *And M13, which this surfaced.* Adding a second client to the Spring
      toolbox turned the first one's test red exactly as M13 describes:
      `@ImportHttpServices` carries one group name and the shared
      `HttpClientsConfig` was rewritten with the newer client's. Each client
      gets its own `<Name>ClientConfig` listing itself by type now — additive
      by construction — and two clients compile and pass together under real
      Maven.
- [x] **P8.9** M10 — a seed path: `db/seeds/*.json` plus a plain Java
      `SeedRunner` going through repository **ports**, never JDBC. Production
      execution behind an explicit profile or flag. Its absence is what pushed
      a database write into a `GET` handler in `mc-01-06-2026`.
      `g seed <Resource>` writes the file with one row built from the record's
      own components, and a `@Profile("seed")` `ApplicationRunner` that loads
      it through `<Resource>Repository`. Into an empty table only: an edited
      seed row cannot be told from a change made in the database, so
      reseeding is left to whoever knows which it was. The companion test is
      what makes the file live — nothing else reads it until somebody starts
      under the profile, so a renamed component would otherwise surface as a
      start-up that dies on one machine. Proved by the minicom manifest under
      real `mvn clean verify` (surefire 19 -> 20 reports, 55 -> 56 tests).
- [x] **P8.10** M11 and the two smaller entries — `g transition --unguarded`
      (or an `explain transition` line naming the escape hatch), and
      `g strategy` generating the evaluator its port's Javadoc describes, with
      ordering, since `FallbackBotRule` must run last or it swallows every
      message and nothing in the generated code says so.
      The escape hatch, not the flag: an unguarded transition is a lost update
      nothing reports, which is the one thing this kind exists to prevent, so
      `explain transition` names `g usecase` plus a `save` through the port as
      the place that decision belongs — and the refusal that sent readers
      looking now prints the two-command sequence itself
      (`jails g field <Target> version:long --default-literal 0`).
      `g strategy` writes `<Name>Evaluator` beside the beans and `@Order`s each
      implementation; the field it holds them in goes through
      `sql::table_name`, since gluing an `s` on produced `eligibilitys`. Both
      compile under real Maven in the Spring toolbox. The third entry — a
      usecase defaulting an enum positionally — closed with P6.5 and its text
      went with the rest.
- [ ] **P8.11** Delete `missing.md`.

## P9 — research.md's remaining sections

In `research.md` §9's own order, minus what the phases above already delivered
(items 1, 2, 4 and 5 close in P0 and P3.2).

- [ ] **P9.1** §4.6 — the repository contract test. One contract interface
      executed once against the fake and once against `JdbcOrderRepository`, so
      semantic drift becomes a failing test; today the two adapters can
      disagree indefinitely. Then factory named states and sequences, and the
      seeds already done in P8.9.
- [ ] **P9.2** §3.3 — frozen conflicts, `continue` and `abort`. The marker
      bytes are produced and dropped today; `PendingIdentity`,
      `ResolutionIdentity` and `RestoreIdentity` exist in
      `jails-protocol::durable::conflict` with no route and no verb. Build the
      five-step durable state machine, not another flag.
- [ ] **P9.3** §4.2 — slices. `SliceSpecV1` and `SliceName` exist and nothing
      reaches them, while `rename resource` *requires* a `<slice>.<name>`
      selector. A project with no slices must keep working unchanged, with the
      unqualified spelling resolving to one implicit slice.
- [ ] **P9.4** §4.1 — the extended field grammar, then §6.1's
      `generate scaffold` surface. Note the §4.1 ordering dependency: decide
      whether `@audit` is a spelling of the existing `--timestamps` /
      `AuditPolicy` or a distinct one **before** shipping it.
- [ ] **P9.5** §4.7 — policy and contract matrices, closed form only: no
      expression string, no SpEL passthrough, the same rule that keeps
      `@check(...)` out of the field spec.
- [ ] **P9.6** §5.1 — Gradle behavioural parity for the warm test engine,
      `jails fmt` and `jails console`. **Blocked**: no `gradle` binary on this
      machine, so every Gradle claim in the repository is currently inferred
      from file contents. Installing one is the prerequisite for the row.
- [ ] **P9.7** §2.4c semantic readiness, §2.4b service identity labels,
      §2.4a test-dependency hints, §2.3 the shared source index — **each behind
      a dated measurement**, per §2.3's own note that the latency win was
      claimed and never measured. Record a baseline for `routes`/`beans` on the
      largest proof app first.
- [ ] **P9.8** §2.7 — the Ecto-style SQL sandbox stays deliberately deferred.
      If the experiment is run, record the negative result rather than deleting
      the section.
- [ ] **P9.9** Delete `research.md`.

## P10 — the wire contract, driven by one untouched take-home

`minicom-15-01-2026` is the checkout jails has to be able to finish: a Spring
Boot 2.7 backend with four endpoints, and two hand-written frontends that
already call **nine**. The frontends are the specification, and they are not
negotiable -- they ship with the brief, and a backend that answers a different
shape is a backend that does not work.

`jails modernize` closed the version half (`git log` has it). What is left is
the wire half, and every entry below was measured by generating into a copy of
that project and reading what came out. Four of them are `missing.md` entries
that this one project needs all at once, which is the strongest argument yet
that they are one subject rather than five.

The nine endpoints, verbatim from `customer.js` and `admin.js`:

| method | path | body |
|---|---|---|
| POST | `/customer_api/ping` | form `email` |
| POST | `/customer_api/messages` | form `email, content, category, priority` |
| POST | `/customer_api/read` | form `email, message_id` |
| GET | `/admin_api/users` | — |
| GET | `/admin_api/messages/{userId}` | — |
| POST | `/admin_api/messages` | form `user_id, content, email` |
| PATCH | `/admin_api/conversations/{userId}/status` | JSON `{status}` |
| PATCH | `/admin_api/conversations/{userId}/category` | JSON `{category}` |
| PATCH | `/admin_api/conversations/{userId}/priority` | JSON `{priority}` |

- [x] **P10.1** **A project whose schema is `schema.sql` gets no DDL, and
      nothing says so.** `jails g scaffold Conversation …` into that checkout
      wrote a repository, an adapter and an `IT` against a `conversations`
      table that does not exist, and printed no warning: the migration is
      conditional on `src/main/resources/db/migration` already existing, which
      is `add db`'s directory. This is the silent wrong answer the whole
      project is organised against, so it goes first. Spring's
      `spring.sql.init` reads `schema.sql`, jails already knows the dialect
      from the driver, and `codemod`'s marked block is exactly the shape for
      appending to a file the reader owns.
      Done: `codemod::Marked` learned a comment token (`--` for SQL, chosen by
      the path in `Marked::for_path`, so the splice and the unsplice cannot
      disagree), `scaffold::schema_block` renders the DDL through the same two
      calls the migration arm makes, and a project with neither destination is
      reported by name with both fixes. Verified on the checkout: H2 2.4.240
      accepts the block and the whole Spring context starts over it.
- [ ] **P10.8** **`g scaffold` writes an ArchUnit fitness function that fails
      on the project it was generated into.** `RAW_JDBC_STAYS_IN_ADAPTERS` went
      red on `minicom-15-01-2026` because the reader's own
      `UsersController`/`MessagesController` hold a `JdbcTemplate` -- code
      jails did not write and was not asked about. A generated test that fails
      over pre-existing code turns "try jails on this project" into "jails
      broke my build", which is the adoption story in one line. Options are a
      scope limited to packages jails owns rows for, or writing the rule only
      into a project that starts clean; measure which before choosing.
- [x] **P10.2** **M15 — every generated endpoint binds JSON, and the clients
      post forms.** `$.post` sends `application/x-www-form-urlencoded`; the
      generated controller is `@Valid @RequestBody`, so six of the nine
      endpoints reject every real request with a 415. Five of the six carry a
      body jails already models as a request record.
      Done: `--consumes json|form` on `controller`, `usecase`, `query` and
      `transition`, recorded on the intent (payload codec 7 -> 8) so a re-plan
      reproduces it. `form` renders `@Valid @ModelAttribute`, which Spring
      binds from request parameters through the record's canonical
      constructor. Proved against a running server rather than asserted:
      `curl -X POST -d "userId=7&status=open"` at a generated endpoint returns
      201 with `Location: /conversations/1` and the row in H2.
- [x] **P10.3** **The JSON key case is jails', not the client's.** The pages
      read `message.sender_type`, `message.created_at` and `user.id`; a
      generated response record emits `senderType` and `createdAt`. jails has
      no way to say a project's wire format is snake_case.
      Done, and the JSON half needed no new feature:
      `jails set spring.jackson.property-naming-strategy=SNAKE_CASE` already
      owns that key, and `Project::wire_naming()` reads the wire off the
      property that decides it rather than asking to be told twice -- the same
      rule `sql_dialect` follows about the driver. What was missing is the
      binder: Spring's data binder has no naming strategy, so a form-bound
      record now carries `@BindParam("user_id")` on exactly the components
      whose two spellings differ.
      Two things had to be found first, both by running the thing.
      `@EnableWebMvc` disables Boot's MVC auto-configuration, so *every*
      `spring.jackson.*` property was silently ignored on this project --
      `cors_checks` already had that warning and matched only the Boot 4
      starter name, so it reported nothing on a project written before the
      rename. Fixed, and `jails doctor` says it now.
      End to end on the checkout: `curl -d "user_id=42&status=open"` returns
      `{"id":1,"user_id":42,"status":"open"}`, which is the shape the two
      pages read.
- [ ] **P10.4** **M14 — an enum constant has no wire value.** The vocabularies
      here are `open`/`in_progress`/`resolved`/`closed` (lowercase),
      `Account`/`Billing`/`Product`/`Technical`/`Other` (TitleCase) and
      `-`/`!`/`!!`, which are not identifiers at all. `g enum` uppercases, so
      none of the three can be expressed.
- [ ] **P10.5** **M16 — the admin list filters are optional and independent.**
      Status, category and priority, any subset, and `g query` takes required
      scalars only.
- [ ] **P10.6** A path with a variable in it: `/admin_api/messages/{userId}`
      returns `{messages, conversation}`. Check what `--path` accepts today
      before deciding whether this is a gap or a doc line.
- [ ] **P10.7** Implement the mission on the checkout itself, with jails
      commands only, and record the command log. The mission is two-way
      communication: a customer replies, and the admin sees the reply.

---

## Verification

Per commit, the workflow `CLAUDE.md` mandates — `--workspace` is not optional,
since `cargo test` at the root reported 390 passing where the tree has 418:

```
cargo fmt --all && cargo clippy --workspace --all-targets \
  && cargo build --workspace && cargo test --workspace && cargo install --path .
```

Per phase, the tier that answers the question the tool exists for:

```
JAILS_REQUIRE_TOOLCHAIN=1 cargo test --workspace
cargo test --test architecture -- --nocapture --test-threads=1
```

The second turns every graceful skip into a failure naming what was missing —
necessary before believing a green run covered the generated-code path.

**End to end, per phase, in a disposable project under the scratch directory**,
which is how every entry in these four documents was found in the first place:
`jails new`, the phase's commands, `mvn -o test-compile` or `mvn -o test`
wherever a claim needs a compiler, `jails doctor`, and `jails migrate --check`
against a real PostgreSQL wherever a claim is about the schema. No jails
source, test or doc file is modified while reproducing.

**The regression corpus is already on disk.** `~/code/minicom-jails/` holds the
six rebuilt projects and `my-minicom/` the seventh; five of six are green and
two are red for reasons P2.1 and P2.2 fix. Re-running the recorded command log
(`jails history` per project) after P3 and P4 is the strongest available check
that the naming and identity spine did not regress anything, and the fastest
way to see modern.md §13.1's delta close.
