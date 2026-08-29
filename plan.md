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
| `bugs.md` | 1 | B57, found while re-confirming the others |
| `missing.md` | 6 | `adopt resource`, `modernize` re-planning, and four generator shapes |
| `modern.md` | 8 | the generated Java assessed against `java.md`/`backend.md` |
| `research.md` | 9 | the remaining product direction |

Phases P0–P5, P7 and P11 are closed and deleted. What remains is P6 (generated
prose and the real defects behind it), P8 (one scope decision) and P9
(`research.md`). **P10 is closed**: P10.7's five blockers shipped and P10.8 is
`jails architecture baseline`.

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
        decision. Half closed: `g scaffold --path` is accepted now, so a
        project *can* be made consistent. What is left is that consistency is
        something the reader has to ask for on every command.
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
      `jails fmt` and `jails console`. **No longer blocked**, and there are
      three observable generations now rather than two: no `gradle` binary is
      on PATH, but `minicom/old/mc-01-06-2026/spring` runs its own wrapper at
      Gradle 8.5 / Boot 2.7.18 / JDK 21 (8.5 refuses JDK 26 with `Unsupported
      class file major version 70`, so `JAVA_HOME` has to point at 21),
      `minicom/minicom-15-01-2026/spring` is the same skeleton, and `jails
      modernize` takes that checkout to Gradle 9.7 / Boot 4.1 / JDK 26 where
      `./gradlew build` is green -- 60 unit tests and 23 integration tests
      against a real PostgreSQL, `integrationTest` included. The substrate the
      exit gate needs exists; the cross-engine comparison it asks for has not
      been run, and the warm engine still refuses on Gradle by name.
- [ ] **P9.7** §2.4c semantic readiness, §2.4b service identity labels,
      §2.4a test-dependency hints, §2.3 the shared source index — **each behind
      a dated measurement**, per §2.3's own note that the latency win was
      claimed and never measured. Record a baseline for `routes`/`beans` on the
      largest proof app first. §2.4c is now the *semantic* half only: `--wait
      --wait-timeout 120` and per-service healthchecks shipped, so what is left
      is `SELECT 1` rather than the engine's opinion about the container.
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

**Closed.** Kept as a heading only until the next phase renumbering, so a
`plan.md P10.7` citation still resolves.

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

---

## P12 — the defect found while re-confirming the closed ones

- [ ] **P12.1** **B57** — re-running an already-installed capability in a
      project that declares a compose service leaves an unfinished transaction,
      and every mutating command afterwards dies on an object that was never
      stored. It is terminal rather than transient, the refusal names a path
      inside `.jails/` and carries no `fix:`, and `doctor` prescribes running
      the same command again, which is the reproduction. `jails sync` -- whose
      whole job is re-applying recorded capabilities -- is among the commands
      that cannot run.

      Bisected in `bugs.md`: `add sqlite` is the control, so the trigger is the
      compose service rather than any one capability. Not caught because every
      scenario and every proof application exercises the *first* install.

      Two things to fix, and the second is the one that matters: the no-op
      re-apply must not leave a transaction expecting an object it never wrote,
      and `doctor`'s `fix:` must not name a command that reproduces the fault.

## P13 — the gates, and what they were not saying

- [ ] **P13.6** **G0 is closed; G1 has a harness and G2-G5 do not.**
      `simplify-sol.md`'s cutover gates. `mise run verify-rewrite` already
      existed as G0's single command; what was missing around it is now in
      place, and each hole it names was confirmed against the tree rather than
      taken from the audit:

      - **CI existed nowhere.** `.github/workflows/verify-rewrite.yml` runs
        that one command and nothing else, on a runner given every tool a
        skip would otherwise hide -- JDK 26 and `jshell`, Maven, mvnd, Gradle
        8.5 on JDK 21, git, and a container runtime. Unverified against a real
        runner; it has only been checked for structure.
      - **`.githooks/pre-push` ran its own `cargo build && cargo test`** -- no
        `--workspace`, so the root package alone, and no
        `JAILS_REQUIRE_TOOLCHAIN`, so a test that could not find its toolchain
        passed by skipping. It execs the one command now. (`.githooks/` is
        tracked, contrary to what `CLAUDE.md` said; that note is corrected in
        the same change.)
      - **`tests/golden/scaffold-plain` belonged to no scenario.** A snapshot
        nothing compares against is worse than a missing one: a missing golden
        fails the first time its scenario runs, an orphan looks like coverage
        and `git diff tests/golden` stays clean whatever happens to the bytes.
      - **`testd-request.hex` and `testd-reply.hex` were read by nothing.**
        They are v1 line-protocol captures, and `compatibility.rs` now decodes
        both and asserts the framing against `TESTD_PROTOCOL`, so changing that
        constant fails against real bytes rather than against another constant.
      - **`before-directory` and `after-file-rename` were advertised in
        `fault::POINTS` and tripped nowhere.** A crash test enumerating
        `POINTS` armed faults that could never fire and reported a pass. Both
        are placed now -- before the `create_dir`, and after each of the three
        ways a file is published, before its parent is synced.

      Three gates hold those: golden directories and scenarios match exactly,
      every protocol fixture is read by something, and every advertised
      failpoint is named outside the registry. Each was verified by injecting
      a violation.

      **G2 is closed too.** All 108 advertised command paths now have a
      journey, held by `every_advertised_command_path_has_a_journey`, which
      reads the same `jails commands --json` catalog that walks the parsing
      `clap::Command`. Ten had none: the eight `kafka` subcommands,
      `architecture baseline` and `setup`. Their journeys assert refusals,
      which is what G2 asks for and all that is reachable without a broker --
      and `setup` is given a scratch `HOME`, because it is the one command
      that writes outside any project and a journey against the real one would
      rewrite the developer's own file.

      Two things the gate needed that are worth keeping: it strips comments,
      because prose naming a command is not coverage; and a journey must name
      its path *literally* rather than assemble it from a loop variable, since
      a journey the gate cannot see reads exactly like a missing one.

      **G3 is closed.** All 39 generator kinds are now compiled by a real
      toolchain. 13 were not: `every_remaining_generator_kind_compiles_in_one_
      spring_project` closes 12 of them in **one project and one `mvn test`**
      (23s), because twelve fixtures would have been twelve Maven invocations
      against a suite already at 108s (P13.7) -- and what needs proving is that
      each kind's output compiles, not that it does so alone. The 13th,
      `socket`, was already covered and the gate could not see it.

      This matters because the golden suite checks *bytes*, not compilability:
      jails could emit Java that does not compile for any of those kinds and
      every existing test stayed green. **It was, for one of them.** The first
      real build found `bugs.md` B58 -- `g event` emits
      `org.springframework.kafka.*` and neither supplies the dependency nor
      refuses without it, so a plain `jails new` + `g event` leaves a project
      that does not compile. That is the defect this gate exists to find, found
      by it.

      **Getting the number right took four wrong answers**, each worth
      remembering. The matcher counts braces to find function bodies, and these
      files are full of Java fixtures -- so a `{` in a string literal is not a
      block, and counting raw made one body span its whole file and reported
      *every* kind as compiled. Blanking literals first is `java::blanked()`'s
      trick. Then the marker list omitted `real_maven_cmd`, which is what the
      toolbox *builders* call -- so a dozen kinds generated there were reported
      as never compiled. A coverage gate that under-reports sends people to
      write tests that already exist.

      **G4 is closed.** `every_failpoint_converges_after_a_child_dies_there`
      runs every entry in `fault::POINTS` in a child process that `abort()`s
      inside the trip, then opens what the crash left -- including a lock whose
      owner is gone -- and asserts the same convergence, twice.

      `Armed::aborting_at` is the new half. The existing suite arms an injected
      `Err`, and that is the easier case: an `Err` unwinds, so every guard
      between the trip and the test releases, the lock drops in order and
      `Drop` runs on the journal. A machine losing power does none of that.
      The child is asserted to die **by `SIGABRT`, not merely unsuccessfully**
      -- a panicking child exits 101 after unwinding, which is the state the
      in-process suite already covers, and accepting it would make this a
      slower copy of that test.

      **It was written off as blocked, and was not.** `crash.rs`'s header said
      the child suite "needs the CLI to route through this executor". The
      executor is reachable as a library -- which is how every test in that
      file already drives it -- so the child is just that binary with two
      environment variables set. The header is corrected in the same change.

      **G5's corpus exists, is enforced, and now includes a project jails did
      not write.** `examples/proof-policy.tsv` is the machine-readable map --
      six manifests with build tool, highest tier, cadence, gate name and
      prerequisites, checked by `tests/cli/examples.rs`, two at tier 2 with
      real containers.

      Every one of those is jails' own output, which was the gap: nothing
      proved the tool against a codebase it did not generate, and a generator
      can be perfectly correct about its own layout while being wrong about
      somebody else's. `an_adopted_reader_written_project_generates_compiles_
      and_keeps_its_own_bytes` closes it with a hand-written Maven project --
      foreign coordinates, foreign packages, classes with bodies -- and proves
      four things in order: adoption reads the foreign layout, generation lands
      beside the reader's code and a real compiler accepts both, the reader's
      files come back byte-identical, and a rerun settles to "nothing to do"
      without rewriting anything.

      That last one corrected an assumption rather than the product. The test
      first asserted a repeated `g record` would *refuse*; it reports "nothing
      to do" and exits 0, which is right -- identity is the entity, so
      re-declaring the same record is an update that changes nothing, and
      second-run idempotency is what `simplify-sol.md`'s differential list asks
      for anyway.

      **G5's differential half is now there too.** The fixture moved into
      `tests/common/` -- two copies of a project whose whole definition is
      "foreign to jails" would drift into two different foreignnesses -- and
      `an_adopted_project_is_treated_the_same_by_both_implementations` runs it
      through both subjects: adopt, generate into the reader's own package,
      rerun, and the reader's bytes unchanged at every step.

      The order is not the obvious one and is worth keeping: `adopt` refuses on
      a canonical project, and is right to, because adoption is how jails
      learns a layout it did not choose and that must happen before a model
      claims to know one. So the canonical subject adopts first and writes
      `.jails/model.jdl` after.

      **The canary's default made it compare the binary with itself.**
      `JAILS_LEGACY_REVISION` defaulted to `HEAD`, so `mise run
      verify-rewrite-g1-canary` built the binary under test a second time,
      passed all 38 assertions and meant nothing -- a check that had silently
      stopped checking, which is the exact shape this file keeps finding. The
      default is the branch point against `main` now, and a revision resolving
      to `HEAD` is refused rather than reported green. Against frozen
      `61413d7f` all 38 differential tests pass, and the binary is
      load-bearing: pointing `JAILS_LEGACY_BIN` at `/bin/true` fails the
      adopted test immediately.

      **The Spring flavour is there too, and it found `bugs.md` B59.** G5 asks
      for adopted *Spring/plain* projects, so `Adopted` is a flavour on one
      fixture rather than two fixtures -- the reader's classes, packages and
      directory names are the same foreignness either way, and what differs is
      what jails reads off the build file. That is the half that matters: every
      version fact a template renders against comes from the *reader's* pom.

      B59 was the divergence it surfaced, and it is fixed in the same change.
      Adoption recorded `adapters = "persistence"` identically on both sides,
      and then the canonical compiler named its packages with **28 hardcoded
      `format!("{}.adapters.jdbc", base_package)` sites**, none of which could
      apply a rename because none of them knew there was one. `jails adopt`
      would have printed its mapping, written its file, and changed nothing
      about where a canonical project's code went -- a configuration command
      that reports success and has no effect.

      The fix is one function. `ProjectIntent::package_for(suffix)` is the only
      place the compiler turns a layer into a package, and **only the head
      segment renames**: a reader who called their adapters `persistence` means
      `persistence.jdbc`, not that the JDBC adapter moved. A head with no
      rename key -- `repository`, `ports`, `application` are the compiler's own
      facet packages -- passes through unchanged, which is the honest answer
      rather than a guessed mapping onto a legacy layer.

      **The layout is a declaration, not an observation**, and that decided
      where it lives. `jails.toml` is jails' own manifest rather than a file
      the reader maintains, so `Layout` sits on `ProjectIntent` and reaches it
      from that manifest through capture -- the same compatibility-input shape
      `.jails/model.toml` has, until JDL declares a layout itself. Copying it
      onto the model in `compile` rather than threading it beside the model is
      deliberate: 48 signatures already carry `&AppModel`, and a second
      parameter through all of them is the sprawl `spring::Slice` exists to
      remove on the legacy side.

      Two gates hold it. `the_compilers_renameable_layers_are_the_engines_
      layers` pins `RENAMEABLE_LAYERS` against `Layer::ALL` -- they are one
      list in two crates that cannot see each other, since `jails-model` sits
      below `jails-spec` -- and
      `both_implementations_write_adapters_into_the_reader_s_own_package`
      proves the two reach the same package by different routes, the legacy
      scaffold emitting its own in-memory adapter and the canonical one
      arriving as the `fake` capability. Verified by falsification: making
      `Layout::segment` ignore its renames fails it immediately.

      **One thing G5's wording asks for that is still not there**: the
      differential half compares CLI behaviour and reader bytes, not a real
      build on both sides. The real build is proved on the current binary only,
      by `an_adopted_reader_written_project_generates_compiles_and_keeps_its_
      own_bytes`. Adding `mvn` to both differential subjects spends a 23s build
      to watch one compiler agree with itself, which is the worst ratio on this
      list -- it is worth doing when the legacy side is a frozen binary from
      before the emitters changed, and not before.

- [ ] **P13.8** **The canonical parity gap is 4 capabilities and 4 kinds --
      far smaller on the capability side than this file assumed, and deeper on
      the kind side than its own refusal message says.** `simplify-sol.md` says the
      remaining work before the gates is *"primarily generator and capability
      backend parity"*; nothing said how much. Measured 2026-08-29 by running
      every entry of `jails commands --json` against a fresh canonical project:

      - **Capabilities: 25 of 25 canonical, as of this entry.** The four that
        refused -- `format`, `ci`, `docker`, `k8s` -- were the ones writing
        *project* files rather than Java, which is why they were left: nothing
        about them is entity-shaped. `ci`, `docker` and `k8s` went through the
        reader-facet file protocol `loadtest` already used; `format` is a
        `BuildFeature` plus an `.editorconfig`. Each is proven byte-identical
        to the legacy output by a differential test, and the count is pinned in
        `registry_classifies_every_advertised_word`.

        Three things that fell out and are worth keeping. **Every one of those
        files now has one owner** under `templates/add/`, read by both engines
        -- two copies of a CI workflow drift on pinned action SHAs, which is
        the drift nobody sees until an advisory names a version still running.
        **`k8s`'s preconditions moved from the build file to the model**: the
        legacy engine greps the pom for the actuator and the registry, where
        the canonical one asks whether the capability is declared, which is
        stricter -- a hand-spliced starter satisfied the old check while
        leaving `sync` nothing to reconcile. And **canonical `format` refuses
        on Gradle by name**, because Spotless needs a `plugins { }` entry that
        is only legal as the script's first statement, and this backend appends
        marked blocks rather than guessing where the top of a reader's build
        file is.

        **The template extraction went wrong once and the checker agreed with
        it.** `format!` renders `{{` as `{`, and the PromQL in the burn-rate
        alerts is written `{{application="demo"}}`. The verifier applied the
        same wrong un-escaping to both sides and reported IDENTICAL for a file
        it had changed; the golden suite caught it. That is the argument for a
        second oracle that does not share the first one's reasoning, in one
        example.
      - **Kinds: `migration`, `association`, `search` and `seed` refuse** with
        *"its JDL syntax editor is not implemented yet -- edit
        `.jails/model.jdl` directly and run `jails sync`"*.

        **The refusal now says which of those it is**, per kind, because the
        generic one told the reader to edit `.jails/model.jdl` and run `jails
        sync` -- false advice for every one of the four, since the model would
        accept the declaration and no emitter would render anything. Deleting
        it left `require_toml_mutation` with no callers at all: that message
        existed solely for these kinds. The kind dispatch in
        `model_generate_jdl.rs` is exhaustive now with no `_` arm, so a kind
        added without deciding what a canonical project does with it is a
        compile error rather than a silent fall-through.

        **That refusal read like a frontend gap and is not one**, which took
        a second pass to establish -- the first entry here said "the model can
        express them; the CLI sugar cannot" and was wrong for three of the
        four. The model carries the *vocabulary*
        (`ProjectionKind::Search { fields }`, `ProjectionKind::Seed`,
        `AppModel.relations`) and the compiler does not read it.
        `Facet::Search` emits a port interface and nothing else -- no
        `tsvector` column, no GIN index, no JDBC adapter, which is three
        quarters of what legacy `g search` produces -- while `seed` and
        `relations` are read by no emitter at all: there is no `references`
        or `foreign key` anywhere in `jails-compiler`. So editing the JDL by
        hand and running `sync`, which is what the refusal tells the reader to
        do, would not produce the artifact either.

        There is also no `ModelPatch` variant that adds a projection.
        `factory`, `dto` and `repo` reach the model through `AddFacet`, which
        carries no fields, so a search projection cannot be expressed as a
        patch even once the emitter exists.

        **One of the four turned out to be worse than a refusal.** `use seed`
        parses, links and passes its prerequisite check, and then reached the
        emitter as `Facet::Factory` -- because `compatibility_facet` mapped
        `Factory | Seed` onto one facet and `Facet` is the dispatch key. So a
        model asking for seed data got `<Name>Factory.java` and a report of
        success. `bugs.md` B59 has it; the silent half is fixed with a distinct
        `Facet::Seed` whose missing arm is a compile error, and what remains is
        the emitter, which needs a JSON sample column on the builtin table, a
        fourth prerequisite (`json`, whose reader the runner uses), and three
        artifacts rather than one.

        **`association` is closed on the emitter side.** A declared `relation`
        block linked, `sync` reported success, and no foreign key was written
        -- `book.author_id` referenced nothing. `emit_sql` now renders one
        `alter table ... add constraint` per relation, **after every `create
        table`**: the entity pass walks a `BTreeMap` by stable id and nothing
        about that is a dependency order, so an inline table constraint would
        be a migration that fails on its first run whenever the child sorted
        first. Removal refuses, the policy indexes already have. What is left
        for `g association` is only the syntax editor, and the refusal says so
        now instead of claiming the emitter is missing.

        **One thing to decide there:** `ReferentialAction` has no `NoAction`,
        so an omitted `on update` compiles to `restrict`, where the legacy
        generator writes `no action`. They differ -- `restrict` is never
        deferred -- and the reader who omitted the clause chose neither.

        `migration` is the fourth and is deliberate rather than missing:
        `CLAUDE.md` records migrations as irreproducible operations that stay
        visible in the plan instead of being rendered.

      **`CLAUDE.md` was three capabilities behind and said so as an
      instruction** -- *"Canonical capability profiles currently include
      `fake`, `db`, and `api` ... every other `add` capability must currently
      refuse"* -- which would have had the next reader add a refusal to code
      that already works. Corrected in the same change, with the date and the
      method, because this number moves.

      **Getting the measurement right took four wrong answers.** Three were
      the probe and one was mine -- calling the kind gap a frontend gap on the
      strength of a refusal message, without checking whether anything
      downstream consumed the declaration. The refusal was accurate about
      itself and misleading about the cause, which is the failure mode a
      `fix:` line invites: it named a repair that does not repair.

      The three probe mistakes were all the same one:
      reading a refusal as a parity gap without reading *which* refusal. `g
      controller Sample id:uuid@pk` is rejected for its flags, not its route;
      the 14 kinds reporting *"requires `jdl 1`"* wanted the v1 document form
      (`jdl 1` / `app X { pkg ... }`), not the older `application X @id(...)`
      one the differential fixtures still write; and `h2` conflicts with a
      model declaring `storage postgres`. A probe that scores "did it fail"
      instead of "what did it say" reports a tool as far less finished than it
      is.

- [ ] **P13.7** **The suite is 108s of `tests/cli` because it compiles 36 Java
      projects, and the remaining lever is Maven's JVM startup.** Profiled with
      the harness's own `JAILS_TEST_PROFILE=1` (it needs `-- --nocapture`;
      cargo captures stderr otherwise):

      - **153 subprocesses, 547s of run time, 449s queued** behind the permit
        pool, against ~108s wall.
      - **`mvn`: 339s over 39 invocations.** The `jails` binary itself is 199s
        over 106. Docker is 9s.
      - **36 distinct project directories**, so there is almost nothing
        redundant to remove: three are built twice, everything else once. The
        cost *is* the coverage.

      Done: `DEFAULT_MAX_TOOLCHAIN_PROCESSES` was the constant 6 and is derived
      from the machine now, clamped to `[6, 12]`. Worth 113.2s -> 108.4s here.
      Past about eight concurrent builds this machine stops getting faster --
      these are JVMs that fork Surefire again underneath, so the limit is
      memory and disk, not cores.

      **The one large lever left is `mvnd`, and it needs an experiment rather
      than a patch.** Warm `mvnd` is 0.6s against `mvn`'s 2.2s on a trivial
      project -- the whole difference is JVM startup, which is a fixed ~1.6s on
      every one of those 39 invocations, so roughly 62s of the 339s. The
      real-toolchain tests deliberately avoid it: `real_path_without_mvnd()`
      strips it from PATH, because `CLAUDE.md` records the daemon as flaky
      under JDK 26 with a native-library extraction bug.

      That claim now needs re-testing rather than trusting: mvnd 1.0.6 ran
      **20/20 green** here under JDK 26. But 20 runs of a plain project is not
      evidence about the case the note describes -- concurrent daemons across a
      parallel suite, Spring projects, Testcontainers. The experiment is to run
      the real-toolchain tier on the mvnd path repeatedly and count failures;
      if it holds, `real_path_without_mvnd` and the `CLAUDE.md` note both go,
      and the suite loses about a fifth of its Maven time. **Do not flip it on
      a handful of green runs** -- that is what the note is there to prevent.

- [ ] **P13.2** **Five production files parse Maven XML; the document asks for
      one.** `jails-project/src/pom.rs` is the path being replaced,
      `jails-workspace/src/{capture,documents}.rs` and
      `documents/build_feature.rs` are replacing it, and
      `jails-protocol/src/vocabulary/coordinate.rs` reads a plugin block as a
      protocol value. Four of the five are the strangler migration, so the
      duplication is deliberate until the cutover -- and gate R3.8 exists so
      that a *sixth* answer cannot appear while it is going on, which is the
      failure a migration invites.

      Closing this is the cutover decision, not a refactor: it means deleting
      `pom.rs` once the new backend is trusted. `jails-project/src/junit.rs` is
      deliberately below the bar -- it matches one element to read one
      artifact's version, which is a lookup rather than an opinion about
      structure.

- [ ] **P13.4** **144 wire formats are still written by hand, and the seam is
      *not* exhausted -- the first sweep was wrong about why.** It concluded
      the remainder needed per-type work because it treated
      `encoder.count(..)` as a hard blocker. It is not one.
      `Encoder::seq` **is** a count followed by a loop of `encode`, `set` is
      that plus the `ordered` check, and `map` the same for key/value pairs.
      So a codec that frames its own collection is byte-identical to
      `Vec<T>`, `BTreeSet<T>` or `BTreeMap<K, V>` doing it -- the canonical
      ordering guarantee included, which is the part that looked like it had
      to stay hand-written.

      **29 codecs frame a collection by hand**, roughly half of them with the
      `ordered` check. Every one whose field is already a `Vec`, `BTreeSet` or
      `BTreeMap` converts with no wire change. `RendererStamp` is the worked
      example: eight fields, one of them a hand-framed `Vec<ObjectId>`, and it
      derived with all 62 golden ledgers byte-identical.

      **A regex filter has now found its own ceiling**, and what it rejects is
      worth keeping rather than repeating. Of the collection-framing codecs,
      three refuse mechanically for a reason no attribute could express --
      `RendererContextV1`, `PreparedChange` and `ToolIdentityFingerprint` call
      `self.validate()?` inside `encode`, so the codec is enforcing an
      invariant rather than describing a layout. `AppliedEntity` opens with a
      refusal on an empty set. `PreparedIdentityV1` writes a format constant.
      Those five are not candidates and should stay hand-written.

      The rest were rejected by the *filter's* limits, not the code's: an
      `Option<String>` written through a closure, an inner enum encoded by its
      `tag()`, and a field parser that mis-reads multi-line generic types. Each
      would need a real Rust parser to clear safely, and the cost of getting it
      wrong is a silent four-byte wire change. **Convert these by reading
      them, one at a time.**

      What is left after those is decoders that re-parse through a constructor
      so a recovered journal cannot carry what the CLI would reject, and
      encodings whose payload is a *derived* value rather than a field. A
      derive growing an attribute per case would be a worse restatement of the
      same code, which is why R3.5's target is withdrawn rather than zero.

      **The golden trees are not sufficient on their own.**
      `PreparedIdentityV1` passed all 62 of them and still changed the wire:
      its `encode` opens with a bare `encoder.u32(1)` that belongs to no
      field, so the derive dropped four bytes and only
      `prepared_bundle_matches_the_protocol_golden` in `jails-prepare` caught
      it. A candidate filter that reasons about `self.<field>` writes cannot
      see a literal one -- so a struct whose encode carries anything that is
      not a field is not a candidate, however well its fields line up.

      **Convert in small batches, and run `cargo test -p jails-prepare
      -p jails-commit -p jails-protocol` alongside `--test golden` after
      each.** A sweep over this file set destroyed six files earlier by mixing
      an absolute string index with a relative one; the converter in use now
      diffs top-level declarations before and after and refuses to write if
      anything but the intended `impl Codec` disappeared. That check is about
      *structure* and says nothing about bytes, which is what the two test
      sets above are for.

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
