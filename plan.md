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
| `bugs.md` | 0 | B46–B51 all closed; the file holds the convention and the coverage note |
| `missing.md` | 7 | adoption (`adopt resource`, `architecture baseline`), `modernize` re-planning, and four generator shapes |
| `modern.md` | 8 | the generated Java assessed against `java.md`/`backend.md` |
| `research.md` | 9 | the remaining product direction |

Phases P0–P5, P7 and P11 are closed and deleted. What remains is P6 (generated
prose and the real defects behind it), P8 (one scope decision), P9
(`research.md`) and P10, which is down to its adoption half.

**P10.7 is closed.** The mission is implemented on the untouched checkout with
jails commands only, and separately from an empty directory; both are verified
by a real build, and the greenfield one by an actual two-way conversation over
HTTP against a running application. The five things that were not expressible
are `--set` (pin a component the endpoint decides), `--via` on a use case
(resolve a foreign key on the way in), `--if-match optional` plus `--set` on a
transition (a partial update an ordinary browser page can reach), `--path` on a
scaffold (a collection route that is a fixed contract) and `--bind` (a request
parameter whose name is neither the component's nor its snake_case). The
command log is `minicom/minicom-org/spring-commands.sh`.

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

- [ ] **P6.6** Delete `modern.md`. Blocked on **eight** entries, each one a
      defect that survives a perfect field spec and each re-confirmed on
      2026-08-27 against a project built from nothing but `jails new --offline`
      plus nine commands (`modern.md`'s header has the exact list). They need
      converting into real items before the file can go:

      - **§4.3** no index serves any query the application runs. `g query --on
        X userId:uuid` reads a table whose only index is the primary key. jails
        can see the shape and could say so the way `free-text-closed-set` does.
      - **§5.4** a `boolean` domain component becomes a `Boolean` on the wire,
        both ways, with `@NotNull` compensating for the boxing.
      - **§6.3** `AppMetrics`, `CorsConfig` and `MetricsConfig` land in the
        root package: a capability's files decide nothing, so nothing places
        them, while every *kind* goes through `generate::layout`.
      - **§6.4** `MessageService` is four one-line forwards to the port. The
        port earns its interface (there is a real in-memory second
        implementation); the service between the controller and it does not.
      - **§6.5** two API styles in one service — REST for the scaffold,
        RPC-over-POST for the generated operations, including a `POST` to
        read — chosen by which command wrote the route rather than by a
        decision. Compounded by P10.7's last bullet: `g scaffold` refuses
        `--path`, so a project cannot be made consistent without hand-editing.
      - **§7** two read-side defects: the service-layer criteria record bound
        directly as `@RequestBody` in a project whose own generated Javadoc
        argues the wire type must not be the domain type, and a silent
        `MAX_RESULTS = 100` with no cursor, no total and nothing in the
        response saying the list was truncated.
      - **§8** `g event`'s listener logs an id and drops the event, under a
        Javadoc saying it hands the event to the application. There is nowhere
        to hand it: no port is generated. A project consuming a topic discards
        every message on it and logs that it received them.
      - **§9** the generated tests mostly test the framework: a service test
        that can only fail if Mockito breaks, an association IT that asks
        `pg_constraint` whether PostgreSQL recorded the FK the migration
        declared, every fixture value `"sample"`, and no concurrency test for
        the CAS the `version` column exists for.

      Two entries closed on re-check and are deleted from `modern.md`: **§6.1**
      (the service takes the port interface now, not a concrete `Jdbc*` class)
      and **§13.6** (its shape half shipped as `g client --method/--on/
      --returns`). Everything narrative, the hand-built reference slice, and
      every entry the file itself labels an *input* problem are deleted too —
      `git log -p -- modern.md` is the record.

---

## P8 — the primitives the real projects needed

- [ ] **P8.11** Delete `missing.md`. No longer blocked on M18 alone: the file
      is four entries now, and three are adoption work rather than a scope
      line -- `adopt resource`, `architecture baseline` (P10.8) and
      `modernize` re-planning what the ledger records. M18 itself is still a
      *decision* rather than work:
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

- [ ] **P10.8** **`jails architecture baseline`.** The *warning* half shipped:
      `g scaffold` into a project that already reaches `java.sql` outside
      `adapters` names the files and says the suite will fail on them. What is
      still four manual steps in a file jails wrote is accepting them --
      set both `freeze.store.default.allowStoreCreation` and
      `allowStoreUpdate` true, run the suite once, set both back, commit
      `.jails/architecture-baseline`. Doing it by hand takes a minute and is
      the last thing between a legacy checkout and a green `jails check`,
      which is the wrong place to hand the reader a four-step recipe.

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
