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

- [ ] **P0.1** Fix the three gates. `("jails-report", "schema_lineage", 7)` is
      already added to `LAYERS` in `tests/architecture/rules.rs`. Remaining:
      the `doctor` module-lines ratchet (1479 vs ceiling 1477 — raise once with
      the reason recorded beside it in `tests/architecture/board.rs`), and
      `core_generation_stays_free_of_showcase_vocabulary` (add the new files to
      the `ledger` allow-list in `tests/genericity.rs` with a reason).
- [ ] **P0.2** Regression tests: an interrupted transaction recovered on the
      next command; the seal + lineage checks.
- [ ] **P0.3** Delete **B18** and **B5/B14** from `bugs.md`; delete
      research.md §0.1 and roadmap items 1 and 2. Commit, push.

## P1 — the controls, as a block (cause D)

Chosen deliberately over pairing each with its fix: land all four so the whole
surfaced defect list is visible before any generator changes. Each is expected
to go **red on landing**; P1.5 records what they name.

- [ ] **P1.1** A combined-kind tier-3 scenario. One project on the **Boot 4
      default** flavour with `scaffold` + `enum` + `strategy` + `usecase` +
      `association` + the default capability set, compiled and run with real
      `mvn test`. The existing scenarios exercise one kind on one flavour,
      which is exactly why missing M1 and modern §2 are invisible. Extend
      `tests/common/scenarios.rs` and `tests/cli/generate.rs`.
- [ ] **P1.2** The default-variant execution gate (modern §11.6's
      generalisation). `add cors` *is* run through real `mvn test` — against
      `write_spring2_fixture`, a Boot 2 project, so the Boot 4 branch every
      real project gets has never been compiled. Assert that **whichever
      variant a version-sniffing template renders for the current default is
      the one executed**. Same failure mode as `JAILS_REQUIRE_TOOLCHAIN`, one
      level up.
- [ ] **P1.3** The `@Disabled` honesty gate (modern §13.8). A generated
      `@Disabled` test reports green exactly as a skipped tier-3 test does.
      Either the command's summary names what it could not assert, or no test
      file is written. `minicom-2025-12-13` has five of nine disabled,
      including both controller tests, and reports green.
- [ ] **P1.4** The `fix:`-command conformance test (research §0.2). Extract
      every `fix:` command the scenario suite emits and assert it does not
      immediately refuse. This is the control for the whole "oracles disagree"
      theme.
- [ ] **P1.5** Record the surfaced list at the top of P2 and delete research.md
      §0.2. Commit, push.

## P2 — fix what P1 surfaced (cause D)

- [ ] **P2.1** `add cors` — one command, red build. It writes
      `app.cors.allowed-origins=http://127.0.0.1:8008,…` and a test asserting
      `https://example.invalid` is allowed. The test must assert the origins
      the capability configured.
- [ ] **P2.2** `g strategy` vs jails' own ArchUnit rule (missing M1 / modern
      §13.2). `g scaffold` writes `DOMAIN_HAS_NO_FRAMEWORK_DEPENDENCIES`;
      `g strategy` writes `@Component` implementations into `domain..`, and the
      `@Component` is load-bearing. Two first-party generators disagreeing
      about the domain boundary, and the disagreement is a red build. Resolve
      by placing the beans in `service`/`adapters` and keeping `domain`
      framework-free. `crates/jails-generate/src/generate/recipes.rs:584` is
      the kind's arm.
- [ ] **P2.3** M1a — `--package` never imports the `--on`/`--yields` types, so
      the workaround does not compile either. Route the strategy renderer
      through the same `import_of` helper the scaffold uses for cross-package
      imports; it already returns an empty string when the packages match.
- [ ] **P2.4** `add db` has no Spring Boot floor (missing M2). Four spliced
      coordinates do not exist on Boot 2.7 (`spring-boot-flyway`,
      `flyway-database-postgresql`, `spring-boot-testcontainers`,
      `spring-boot-docker-compose`), and `MANAGED` is chosen on whether Boot
      manages dependencies rather than on the Boot **version** —
      `crates/jails-generate/src/add/database.rs:109`. Refuse by name, in the
      shape `require_jakarta_spring` already uses, naming the *module*.
- [ ] **P2.5** `g client` writes a remote call with no timeout, no base URL and
      no defined failure mode (modern §13.6). `backend.md` §1 admits no
      exceptions here. Write `spring.http.client.connect-timeout` /
      `read-timeout` and a commented `…base-url` alongside, the way
      `ensure_failsafe` and `ensure_assertj` are written from the write path.
- [ ] **P2.6** `g migration` writes a file whose whole content is
      `-- Forward-only migration. Write explicit SQL below.`; Flyway applies it
      and records the checksum, so the history asserts an index that does not
      exist (modern §13.7). Refuse to write an empty one, or write it in a form
      Flyway will not apply until it has content.
- [ ] **P2.7** `add api` installs a sealed `ApiException`, an exhaustive
      handler and 40 lines of Javadoc, and **nothing throws it in 0 of 7
      projects** (modern §6.1, §11.7, §13.10) — while the one operation with
      real failure modes hand-rolls `ResponseStatusException`. Wire the
      generated code into it, or say it did not.
- [ ] **P2.8** `jails migrate lint` and `jails schema diff` require
      `.jails/app.toml`, so neither runs on the shape `jails new` produces.
      Both questions are answerable from the migrations and the ledger alone
      (bugs.md, "not a bug" section).

## P3 — the naming and binding spine (cause A)

Full convergence, as chosen. Expect large golden churn; regenerate in the same
commit as the change that causes it.

- [ ] **P3.1** `FieldName` in `jails-protocol` owning both renderings:
      `java()` → lowerCamelCase, `column()` → snake_case. `user_id:uuid` and
      `userId:uuid` converge on Java `userId` + SQL `user_id`; a spec name that
      cannot produce a Java identifier by convention is the only error case.
      `sql::snake_case` already exists
      (`crates/jails-generate/src/sql.rs:376`) and is the SQL half; the Java
      half is currently the raw spec string. Templates lose access to the raw
      name. Closes modern §3.1, §3.3, §11.1.
- [ ] **P3.2** Recorded `ColumnBinding` — a `(EntityId, field name) → column
      name` pair per managed entity, written at create time and consulted by
      every SQL projection instead of re-deriving. `TableBinding` already does
      this at the entity level. Unblocks research §3.10 `--column preserve` (a
      ledger edit with no migration; `single-cutover` becomes that edit plus a
      forward `alter table … rename column`) and gives P0's coherence check a
      binding to compare *through*. Delete research.md §3.10 and roadmap item 5.
- [ ] **P3.3** `findById` is typed on the primary key, not on `String` (modern
      §13.5 — 11 of 12 generated ports, and in `my-minicom` two ports over two
      tables in one app disagree). Thread the `@pk` field's Java type through
      `crates/jails-generate/src/generate/repository.rs` and the ~10 templates
      under `templates/spring/` that hardcode `findById(String)` /
      `String.valueOf(x.id())`.
- [ ] **P3.4** Names that carry no meaning (modern §3.2): the
      `Message_userAssociation` underscore, `Default…UseCase`, `…QueryPort`.
      `backend.md` §8 bans `Helper`/`Manager`/`Util` for the same reason.
      Narrow, template-only.

## P4 — who assigns identity (cause B)

- [ ] **P4.1** `g scaffold` **refuses without a primary key**. `rails g
      scaffold` gives you one whether you ask or not because it is not a
      preference; jails made it opt-in and a project that did not opt in got
      two tables with no primary key, a `findById` that throws if two rows
      match, and a compare-and-swap presented as atomic over a multi-row update
      (modern §4.1, §11.2). `create_table` already emits `primary key (id)`
      when an `id` column exists — the gap is the refusal.
- [ ] **P4.2** An assignment policy on the pk field —
      `ClientSupplied | ServerGenerated | DatabaseGenerated` — consumed by
      `create_table`, the use case, the request DTO and the in-memory fake.
      `long@pk`/`int@pk` get `generated always as identity` and
      `insert … returning`, and the use case stops naming the id. Today
      `crates/jails-generate/src/spring/workflow.rs:280` hands `0L` to every
      create over an integer key, in every project, so the primary create path
      works exactly once and the generated test asserts
      `assertThat(created.id()).isNotNull()` on a primitive `long`
      (missing M3, modern §13.3).
- [ ] **P4.3** The request DTO stops asking the client for server state
      (modern §7). `POST /messages` currently requires the primary key, the
      timestamp, the read flag *and* the optimistic-lock version — the exact
      defect the generated test's own Javadoc describes and then commits.
      Closes bugs.md's "POST body invents the id" note.
- [ ] **P4.4** `uuidv7()` where the database supports it, not
      `gen_random_uuid()` (modern §4.2 — `backend.md` §5 names random UUID keys
      specifically), and `order by` a real ordering column rather than a random
      id (§4.4).
- [ ] **P4.5** The version travels as an `ETag` / `If-Match` / `412` rather
      than as a bespoke JSON field (modern §7, §10.5), and expected outcomes
      are a sealed return type rather than two exception classes (§5.3, §10.3):
      a caller that forgets a `catch` finds out in production, one that forgets
      a `switch` arm does not compile.

## P5 — the closed set, in SQL (cause C)

- [ ] **P5.1** An enum column emits `check (col in (…))`. Zero `check (`
      appears in 20 migrations across all seven projects, and this is **not** an
      input problem: the user declared `g enum`, jails generated the Java enum
      and the column, jails holds the constant list, and still wrote a column
      that accepts `'banana'` (modern §13.4). `backend.md` §5 makes it the
      highest-value line in the file.
- [ ] **P5.2** `g enum` adding a constant generates the
      `alter table … drop constraint … add constraint …` migration in the same
      step — the follow-on question P5.1 creates, answered rather than avoided,
      through the append-only sealing machinery `resource field` already uses.
- [ ] **P5.3** A `String` field with a small closed set is challenged
      (modern §11.3). `direction:String!` produced an unconstrained column, an
      unconstrained record, and a `"sample"` fixture; jails already has
      `g enum` and its own example manifest uses one here.
- [ ] **P5.4** The schema's remaining non-negotiables (modern §4.7):
      case-insensitive unique index on an email-shaped `@unique`, a
      `check (length(btrim(x)) > 0)` where the Java constructor rejects blank,
      and either an explicit reason for `deferrable initially deferred` on
      every generated FK or a different default (§13.10).

## P6 — the prose, and the real bugs behind it (cause E)

- [ ] **P6.1** The Kafka partition key is unique per record and the Javadoc
      claims it gives "ordering per entity" — the exact behaviour it prevents
      (modern §8.1). Key on the entity id. `backend.md` §4: *"The partition key
      is the design decision."*
- [ ] **P6.2** The event id **is** the message id, and the outbox stages
      `on conflict (id) do nothing`, so a second event about the same entity is
      silently discarded (modern §8.2).
- [ ] **P6.3** The outbox relay ceiling is one event per second (`claim()` is
      `limit 1` on a `fixedDelay=PT1S` worker), there is no jitter on the
      backoff, and a multi-sink partial failure re-sends a Kafka publish that
      succeeded (modern §8.4).
- [ ] **P6.4** Delete or repair the claims that are false: "keyed on the
      `email` component", "ordering per entity", "scoped matches cannot mutate
      another tenant's row" (there is no scope in the SQL), "this type has no
      `id` component" (it has one). 27% of production Java is comment and a
      wrong explanation is believed. Add a ceiling on template comment density
      to `tests/architecture/board.rs`. The load-bearing ones — the
      `@ServiceConnection` explanation, the Failsafe note, the
      `DeadLetterPublishingRecoverer` default — stay (modern §11.5, §12).
- [ ] **P6.5** Two generators, two answers, one arguing against the other: the
      scaffold path sets both audit columns to one `Instant` and explains why,
      the use-case path calls the clock twice (modern §13.9). And a `usecase`
      defaults an enum **positionally** — `IssueStatus.values()[0]` — so
      reordering a `g enum` silently changes every generated create
      (missing.md, "two smaller things").
- [ ] **P6.6** Delete `modern.md`. Every remaining entry is closed by here.

## P7 — evolution keeps derived code true (cause F)

- [ ] **P7.1** A generated file whose stated premise has become false is
      re-planned or reported, never left with a comment contradicting the code
      beside it (modern §11.4). `g field id` wrote `V004` and left
      `InMemoryUserRepository.findById` returning `Optional.empty()` with a
      TODO saying the type has no id — `findById` always empty, `save` keying
      on a colliding counter, `deleteById` removing a `UUID` from a
      `Map<String, …>` (modern §8.3). Extend the companion re-plan that landed
      for `g field` in `e3c7041`.
- [ ] **P7.2** Placement is recorded, so `--package` is not a one-way door
      (missing M1b). The strategy row is recorded without its placement, so
      `destroy` reconstructs default `domain..` paths, finds nothing, and
      reports the resource as absent seconds after the generate that recorded
      it — recoverable only through `jails history` + `jails undo`, which is
      not what the error message points at. Same family as B2's package fix.

## P8 — the primitives the six real projects needed (cause G)

All three of `missing.md`'s named primitives, in full, plus the smaller entries.

- [ ] **P8.1** M5 — `--via <Association>` on `g query`, letting one filter name
      a column on the parent. `g association` already reads both records and
      type-checks the field mapping across the boundary, which is exactly what
      a join needs and is used today only to emit a foreign key. Covers all
      four real endpoints in the table without inventing a query language.
- [ ] **P8.2** M5's smaller half — `--order-by` and `--limit` on `g query`.
- [ ] **P8.3** M6 — get-or-create by natural key: `--on-conflict <field>` on
      `g usecase`. The statement is one `g explain idempotency` already
      describes verbatim (`insert … on conflict (…) do nothing returning`);
      what is missing is a verb that applies it to a scaffold's own unique key.
      The single most repeated hand-written line across the six projects.
- [ ] **P8.4** M4a — a `WebSocketHandler`-shaped kind: the handler, its
      `WebSocketConfigurer` registration, and a test. Same shape as
      `g handler`. `add sse` covers the server→client half of read receipts and
      presence and none of the client→server half.
- [ ] **P8.5** M4b — the presence primitive. The Django original tracks admin
      presence in a module-level dict with a comment saying it only works
      because there is one process: the author knew it was wrong and shipped it
      anyway. An in-memory presence map is silently correct on one node and
      silently wrong on two, with no error either way — the same class of
      "the default is wrong in a way nothing reports" that `g auth` and
      `add sse` exist for, so the generated **test** is what keeps the fix in
      place.
- [ ] **P8.6** M9 — an index on an existing table: `resource index`, or
      `--index` on `g field`. `g field` can already add a *column* to a live
      table with a data plan, which is the harder problem; an index has no data
      plan to argue about, and `sql::validate_index` already parses
      `'created_at desc'` into column plus ordering.
- [ ] **P8.7** M8 — `--path` on `g controller`, `g usecase`, `g query`. Derived
      paths are a virtue greenfield and unusable when the URLs are a fixed
      external contract. The derivability argument does not block it: `destroy`
      finds files by what the ledger recorded, so a recorded `--path` is no
      harder to undo than `--package` is meant to be.
- [ ] **P8.8** M7 — `g client` takes `--method`/`--on`/`--returns` (and the
      P8.7 path), so it generates the call the project makes rather than a REST
      collection to delete. The `HttpClientsConfig` / restclient splice it
      already writes alongside is the valuable half and stays.
- [ ] **P8.9** M10 — a seed path: `db/seeds/*.json` plus a plain Java
      `SeedRunner` going through repository **ports**, never JDBC. Production
      execution behind an explicit profile or flag. Its absence is what pushed
      a database write into a `GET` handler in `mc-01-06-2026`.
- [ ] **P8.10** M11 and the two smaller entries — `g transition --unguarded`
      (or an `explain transition` line naming the escape hatch), and
      `g strategy` generating the evaluator its port's Javadoc describes, with
      ordering, since `FallbackBotRule` must run last or it swallows every
      message and nothing in the generated code says so.
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
