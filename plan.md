<!--
plan.md — the working checklist: what is still open, and nothing else.

**Item identifiers are `P<phase>.<item>`, deliberately.** Roughly 208 source
comments cite the *old* `plan.md` by section number (`plan.md §R6`,
`plan.md §19.2`) and resolve through
`git log --diff-filter=D -- plan.md` then `git show <commit>^:plan.md`.
`P3.1` can never be confused with `§R6`, so both citation styles keep working.

**A closed item is deleted from this file**, in the same commit that deletes
the entry it closes from `bugs.md`, `missing.md`, `modern.md` or
`research.md` — the delete-don't-mark convention all five files share.
`git log -p -- plan.md` is the record.

**Item and phase numbers are stable and never reused**, which is what makes the
deletions safe: roughly thirty source comments cite a *closed* item by number
(`plan.md P3.1`, `plan.md P8.8`), and `git log -p -- plan.md` still resolves
each to a subject. A phase with no open items disappears entirely rather than
being renumbered, so P0–P5 and P7 are gone and P6 is one item long.
-->

# plan.md — closing bugs.md, missing.md, modern.md, research.md

## Context

Four documents describe what jails does not do yet:

| document | open | subject |
|---|---:|---|
| `bugs.md` | 6 | defects reproduced from an empty directory, B46–B51 |
| `missing.md` | 1 | M18, the one deliberate scope line, recorded as a decision |
| `modern.md` | 10 | the generated Java assessed against `java.md`/`backend.md` |
| `research.md` | 9 | the remaining product direction |

Phases P0–P5 and P7 are closed and deleted. What remains is P6 (generated
prose and the real defects behind it), P8 (one scope decision), P9
(`research.md`), P10 (the wire contract of one untouched take-home) and P11
(the defects the last dogfooding pass found).

**Every *defect* below is reproducible from a clean `jails new`**, and states
the command that produced it. That is the bar, because goldens compare bytes
and never run the code: the oracle that finds this class is a real build. The
remaining items are capability work, and each names the exit condition that
retires it.

---

## Working discipline (every item)

- One item ≈ one commit. `cargo build --workspace && cargo test --workspace &&
  cargo install --path .` green, `cargo fmt --all`, `cargo clippy --workspace
  --all-targets` clean, then push to `main`.
- **Delete the closed entry from its source document in the same commit**, and
  delete the item from here in the same commit.
- A new refusal or kind gets its `SCENARIOS` row (`tests/common/scenarios.rs`);
  there is no fourth list.
- Verified against `deps/`, never from memory, for anything version-shaped.

---

## P6 — the prose, and the real bugs behind it

- [ ] **P6.6** Delete `modern.md`. Every remaining entry is closed by here.
      *Blocked on ten entries no item covers.* Every §-numbered entry below
      is jails-side and reproducible; none is an input problem the file itself
      labels as such. They need converting into real items before the file can
      go:

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
      §1, §2, §3.2, §4.6, §4.7, §5.1, §10, §12, §13.1, §13.10 and §13.11 are
      either narrative, the hand-built reference, or input problems the file
      itself labels as such — they are the record of *why*, and they go when
      the ten above do. §13.6 is closed: its shape half shipped with `g
      client --method/--on/--returns`.

---

## P8 — the primitives the real projects needed

- [ ] **P8.11** Delete `missing.md`. Blocked on **M18** alone, and M18 is a
      *decision* rather than work: jails generates a REST surface and no
      operator surface, which is the one thing every Django port gets free and
      every jails port does not. Either build the back-office generator or
      record the scope line somewhere permanent — `README.md`'s "Not yet" is
      the place — and delete the file. Do not delete it while the only record
      of the decision is the file being deleted.

---

## P9 — research.md's remaining sections

In `research.md` §9's own order.

- [ ] **P9.1** §4.6 — the repository contract test. One contract interface
      executed once against the fake and once against `JdbcOrderRepository`, so
      semantic drift becomes a failing test; today the two adapters can
      disagree indefinitely. Then factory named states and sequences; the
      seeds half already shipped as `g seed`.
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
      `jails fmt` and `jails console`. **No longer blocked.** The prerequisite
      was read as "install a `gradle` binary" and none is on PATH, but two
      checkouts ship their own wrapper and run it:
      `minicom/minicom-15-01-2026/spring` (Boot 4.1 after `jails modernize`)
      and `minicom/old/mc-01-06-2026/spring` (Gradle 8.5 / Boot 2.7.18 / JDK
      21 -- Gradle 8.5 refuses JDK 26 with `Unsupported class file major
      version 70`, so `JAVA_HOME` has to point at 21). Two Gradle generations
      is what the exit gate actually wanted: claims observed, not inferred.
- [ ] **P9.7** §2.4c semantic readiness, §2.4b service identity labels,
      §2.4a test-dependency hints, §2.3 the shared source index — **each behind
      a dated measurement**, per §2.3's own note that the latency win was
      claimed and never measured. Record a baseline for `routes`/`beans` on the
      largest proof app first.
- [ ] **P9.8** §2.7 — the Ecto-style SQL sandbox stays deliberately deferred.
      If the experiment is run, record the negative result rather than deleting
      the section.
- [ ] **P9.9** Delete `research.md`. Blocked on P9.1–P9.8 and P9.10; §2.7
      (P9.8) is a deliberate deferral rather than work, so it needs the same
      treatment M18 does — a permanent home for the decision before the file
      that records it goes.
- [ ] **P9.10** `jails schema diff` requires `.jails/app.toml`, so it does not
      run on the shape `jails new` produces. Carried out of P2.8, which closed
      the `migrate lint` half: that command wanted the manifest for the dialect
      alone and the project's driver is the same authority
      `Project::sql_dialect` uses everywhere else. `schema diff` is real work —
      its `declared` authority *is* the manifest's entity list, and the
      equivalent over the ledger's recorded specs does not exist yet.
      Reproduce: `jails new --offline d && cd d && jails schema diff --from
      declared --to migrations`.

---

## P10 — the wire contract, driven by one untouched take-home

`minicom-15-01-2026` is the checkout jails has to be able to finish: a Spring
Boot backend with four endpoints, and two hand-written frontends that already
call **nine**. The frontends are the specification and they are not negotiable
— they ship with the brief, and a backend that answers a different shape is a
backend that does not work.

`jails modernize` closed the version half, and the wire half is closed for
paths, form binding, enum wire values and optional filters. The nine endpoints,
verbatim from `customer.js` and `admin.js`:

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

- [ ] **P10.7** Implement the mission on the checkout itself, with jails
      commands only, and record the command log. The mission is two-way
      communication: a customer replies, and the admin sees the reply.
      *In progress.* The generators the mission needs work on the checkout --
      three scaffolds, four closed sets, an association, a path-variable query
      and two form-bound use cases -- and every defect in `bugs.md` was found
      by running its build. Five things are not expressible by any jails
      command, each blocking a named endpoint. All five re-confirmed against
      HEAD on 2026-08-27:

      - **a use case cannot pin a component to a constant.** `POST
        /admin_api/messages` must write `sender_type = ADMIN` and
        `/customer_api/messages` must write `CUSTOMER`; today both take it
        from the caller, so either endpoint can forge the other's messages.
      - **a use case cannot resolve a foreign key on a write.** `POST
        /customer_api/messages` carries `email`, not `user_id`. `g query
        --via` does this on the read side and there is no write equivalent.
      - **a use case returns its target, and two endpoints must return
        something else.** `POST /customer_api/ping` returns the *unread
        messages* for the email it was given; `POST /customer_api/read`
        returns nothing and mutates a flag.
      - **there is no generator for a partial update.** The three `PATCH
        /admin_api/conversations/{userId}/{field}` endpoints are one shape
        repeated: set one column on the row a path variable selects.
      - **a resource's route is not settable.** `g scaffold User` serves
        `/users`; the frontend calls `/admin_api/users`. `--path` is refused
        by name here -- *"`--path` applies to a controller, a use case or a
        query"* -- which is the honest answer and not yet the useful one.
- [ ] **P10.8** **`g scaffold` writes an ArchUnit fitness function that fails
      on the project it was generated into.** `RAW_JDBC_STAYS_IN_ADAPTERS` went
      red on `minicom-15-01-2026` because the reader's own
      `UsersController`/`MessagesController` hold a `JdbcTemplate` -- code
      jails did not write and was not asked about. A generated test that fails
      over pre-existing code turns "try jails on this project" into "jails
      broke my build", which is the adoption story in one line. Options are a
      scope limited to packages jails owns rows for, or writing the rule only
      into a project that starts clean; measure which before choosing.

---

## P11 — the defects the last dogfooding pass found

Six reports, all reproduced from an empty directory against `jails 0.1.0` built
from HEAD. Full transcripts are in `bugs.md`; the item is the fix.

- [ ] **P11.1** **B46** — the second `destroy --storage drop` on a re-created
      resource refuses with jails' own internal-bug message and writes nothing,
      so a resource that has been dropped once can never be dropped again. The
      drop planner walks the whole sealed lineage while the read set declares
      only the current head, and the guard fires on the first command with more
      than one create to walk. Declare the superseded creates in the read set;
      the refusal itself is correct behaviour for an undeclared read.
- [ ] **P11.2** **B47** — `doctor` reports `25 checks, all clear` over a
      `.jails/ledger.toml` no mutating command can read. `compat` already
      classifies the store as absent / current / unreadable and `doctor` never
      asks. Add the check, and make `resource status` name the cause instead of
      answering `state: ambiguous`. This is the two-oracles-disagreeing shape,
      in its worst form: the command you run when something is wrong is the one
      that says nothing is.
- [ ] **P11.3** **B48** — a `--path` query with a path variable generates a
      controller test that POSTs to a GET-only route, with `{userId}` never
      expanded, carrying a JSON body for a `@PathVariable`. It fails at the URI
      with `IllegalArgumentException: Not enough variable values available to
      expand`. The controller renderer already worked out which criteria come
      from the URL; the test renderer branches on the criteria record and does
      not know. Pass the same resolved value to both.
- [ ] **P11.4** **B49** — `--method` is accepted and silently ignored by
      `g query`: `--method post` emits `@GetMapping`. Deriving the verb from
      whether every filter is a path variable is right; accepting a flag that
      contradicts the derivation and saying nothing is not. Refuse by name, the
      way `--path` on `g scaffold` already does.
- [ ] **P11.5** **B50** — `g record String value:string` emits
      `public record String(String value)`, whose component is typed as the
      record rather than as text, because a package member outranks the
      implicit `java.lang` import. Both it and its generated test compile, so
      the tier that answers the question this tool exists for is green over it.
      `Name` already refuses Java reserved words; every reserved word is
      lowercase and the name is capitalised before the check, so the check
      never fires. `java.lang`'s type names are a closed list and belong in the
      same place.
- [ ] **P11.6** **B51** — `jails explain query` still says "Required scalar
      equality filters only" and optional filters ship. `explain` is a
      hand-written table by design, and `every_kind_has_an_explanation` checks
      only that a kind *has* a row. Nothing checks that the row is still true —
      the same oracle-drift shape
      `every_command_a_message_tells_the_reader_to_run_is_one_that_exists`
      exists to catch on the other side.

---

## Verification

Per commit, the workflow `CLAUDE.md` mandates — `--workspace` is not optional,
since `cargo test` at the root reported 390 passing where the tree has 418:

```
cargo fmt --all && cargo clippy --workspace --all-targets \
  && cargo build --workspace && cargo test --workspace && cargo install --path .
```

The full suite is **148 s wall, 1501 tests, 38 binaries** on this machine;
`tests/cli` is 84 s of it, dominated by real `mvn`/`javac`.

Per phase, the tier that answers the question the tool exists for:

```
JAILS_REQUIRE_TOOLCHAIN=1 cargo test --workspace
cargo test --test architecture -- --nocapture --test-threads=1
```

The first turns every graceful skip into a failure naming what was missing —
necessary before believing a green run covered the generated-code path.

**End to end, per item, in a disposable project under the scratch directory**,
which is how every entry in these four documents was found in the first place:
`jails new --offline`, the item's commands, `mvn -o test-compile` or
`mvn -o test` wherever a claim needs a compiler, `jails doctor`, and
`jails migrate --check` against a real PostgreSQL wherever a claim is about the
schema. No jails source, test or doc file is modified while reproducing.

**The regression corpus is already on disk.** `~/code/minicom-jails/` holds the
rebuilt projects; re-running a recorded command log (`jails history` per
project) is the strongest available check that the naming and identity spine
did not regress. `minicom/` holds the untouched originals, and two of them ship
a Gradle wrapper — the only Gradle jails can be observed against, since no
`gradle` binary is on PATH.
