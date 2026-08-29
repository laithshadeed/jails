# bugs.md — open defects found by dogfooding jails

**One report is open: B57.** B46-B51 are closed and deleted; each was
re-reproduced from an empty directory against the binary built from HEAD rather
than assumed from the diff.

Binary: `jails 0.1.0`, built and installed from this checkout. Every report
below was reproduced from an empty directory with the commands as written, in a
disposable project under a scratch directory. **No jails source, test, build or
doc file is modified while reproducing.**

**A closed report is *deleted* from this file, not marked done.**
`git log -p -- bugs.md` is where a closed one and the run that closed it live.
Numbers are stable and never reused, so a `bugs.md B33` citation in the source
still resolves to a subject.

---

## B57 - re-running an installed capability bricks a project that has a compose service

Every mutating command dies afterwards, and `doctor`'s own fix is the one thing
that cannot work.

```sh
jails new --offline b57 --package com.example.b57 && cd b57
jails add db --no-start
jails add cors          # applied ... exit 0
jails add cors          # <- here
```

```text
jails: .../.jails/transactions/5f9b77.../objects/sha256/7d/dde2f4...:
       No such file or directory (os error 2)
```

**The first run reports success.** `applied 3bd9e4...`, the file list, `ledger
replace`, `effect compose reconcile (0 up, 0 stopped) (done)`, exit 0. Nothing
says a transaction was left unfinished.

**It is terminal, not transient.** The same missing object every time, so the
project is stuck: `jails add`, `jails sync` and `jails g record` all fail with
that line and nothing else. There is no `fix:` on it, and what it names is a
path inside `.jails/`, which is the one place `CLAUDE.md` says a reader should
never have to reason about.

**`doctor` sees it and prescribes the thing that fails.** It reports

```text
FAIL  transaction  transaction 113938... started and did not finish, so some of
                   jails' own output is newer than what jails has recorded
             fix: run the same command again -- it finishes the interrupted
                  transaction before doing anything new. Do not run
                  `jails resource repair`
```

Running the same command again is exactly the reproduction above. That is
B47's shape at its worst: the command you run when something is wrong is the
one that sends you in a circle.

**The trigger is the compose service, not the capability.** Bisected from an
empty directory:

| sequence | result |
|---|---|
| `add cors`, `add cors` | `nothing to do` |
| `add db --no-start`, `add db --no-start` | `nothing to do` |
| `add sqlite`, `add api`, `add api` | `nothing to do` |
| `add db --no-start`, `add cors`, `add cors` | **fails** |
| `add db --no-start`, `add api`, `add api` | **fails** |
| `add db`, `add api`, `add api` | **fails** |
| `add kafka --no-start`, `add api`, `add api` | **fails** |

`sqlite` is the control: it is a database capability that writes no compose
service, and it does not reproduce. So it is re-running an already-installed
capability *in a project that declares a compose service* -- the no-op path
writes no object, and something on the compose-reconcile effect still expects
one.

Worth saying because it is the reason this was not caught: the *first* install
of each capability is what every scenario and every proof application
exercises, and `jails sync` -- the command whose whole job is re-applying
recorded capabilities -- is the one that cannot run here.

---

## Never covered

Recorded so the gaps in this file are visible rather than implied.
`testd` and `--affected`; `test --engine warm`; `jails run` cold start;
`sql check --live`, `introspect`, `pull`, `contract check`, `editor`,
`request`, `runner`, `logs`, `console`.

**No longer in this list: a generated application run end to end against a
live database.** `minicom/minicom-org/spring` was started against a real
PostgreSQL and driven over HTTP -- sign-in, an admin message, a customer
reply the admin then reads, a mark-as-read, and a customer request asking for
`sender_type=ADMIN` that is still stored as `CUSTOMER`. It was started with
`mvn spring-boot:run` rather than `jails run`, so **`jails run` cold start is
still uncovered** and is the half that remains.

No `gradle` binary is on PATH, so a Gradle claim can only be exercised through
a checkout that ships its own wrapper. Three generations have now been
observed: `minicom/old/mc-01-06-2026/spring` (8.5 / Boot 2.7.18 / JDK 21),
`minicom/minicom-15-01-2026/spring`, and the same checkout after `jails
modernize` took it to Gradle 9.7 / Boot 4.1 / JDK 26, where `./gradlew build`
runs 60 unit and 23 integration tests green.

## B58 — `g event` emits Kafka code and neither supplies the dependency nor refuses

`jails g event <Name>` writes `<Name>Publisher.java` and `<Name>Listener.java`
importing `org.springframework.kafka.core`, `org.springframework.kafka.support`
and `org.springframework.kafka.annotation`, plus a `<Name>MessagingIT`. In a
project without `jails add kafka` none of that is on the classpath, so the
generate succeeds, prints its created files, and leaves a project that does not
compile.

That is the exact failure `CLAUDE.md` says a generator must not cause: *"A
generator that emits code must supply the dependency it needs ... Handing the
reader a compile error for a line they did not write is exactly the plumbing
this tool exists to remove."*

**Why nothing caught it.** The `event` golden scenario is `g event Transaction`
against a Spring fixture with no capability, and the golden suite compares
*bytes* — it never compiles. `simplify-sol.md`'s G3 is about exactly this gap,
and closing it surfaced this on the first real build.

**The fix is a refusal, not a splice.** `add kafka` supplies three dependencies
(the Boot starter, `testcontainers-kafka` for the generated IT, and
`micrometer-core`, which `spring-kafka` declares `optionalApi` so it is not
inherited) as well as the error handler and DLT routing. Splicing one of them
would trade a missing-import error for a missing-bean one. The pattern this
repository already uses is the right one: `usecase --yields` refuses without
`add json` and names it —

```
usecase X --yields Y needs the generic JSON capability for durable payloads.
       fix: run `jails add json` first.
```

`g event` should refuse the same way, keyed on the same projection check
(`project.projected_main_sources()`, not disk, so a well-ordered `app apply`
that installs the capability in the same transition is not refused).

**Reproduce:**

```
jails new demo && cd demo
jails g event Transaction
mvn -q -B test        # package org.springframework.kafka.core does not exist
```

## B59 — `use seed` has no emitter (the silent-factory half is fixed)

`ProjectionKind::Seed` parses, links, and passes validation. It then reaches
the emitter as `Facet::Factory`, because `compatibility_facet` maps
`Factory | Seed => Facet::Factory` and `Facet` is what `emit_java` dispatches
on. So a canonical model declaring

```
use repo for Note
use seed for Note
```

compiles, reports `synchronized ... 8 files written`, and produces
`testkit/NoteFactory.java` — a test fixture. The `db/seeds/note.json` and the
`@Profile("seed")` runner that `jails g seed` writes on the legacy path are
nowhere, and nothing says so.

**This is worse than the refusal beside it.** `jails g seed` on a canonical
project refuses and names what is missing; editing the JDL by hand — which is
what the old refusal told the reader to do — reaches this instead, and a wrong
artifact reported as success is the failure mode a missing one does not have.

**The prerequisite check is not the guard it looks like.** `seed` requires
`repo`, `storage postgres` and `platform spring`, and all three pass here.
Validation is checking that the projection is *allowed*, not that anything
renders it.

**Half fixed.** `Facet::Seed` exists now and the emitter refuses by name
instead of falling into the factory's arm, so the wrong artifact is gone and
`a_seed_projection_refuses_by_name_rather_than_emitting_a_factory` holds it.
`Facet` is the emitter's dispatch key and its match is exhaustive, which is
what makes the gap a compile error rather than a silent reuse --
`ProjectionKind::Search` already had its own facet for the same reason.

**What is left is the emitter**, and it is more than a rendering pass:

- The row file needs a **JSON** sample per builtin. `BuiltinSemantics.sample`
  is a *Java* expression; the legacy JSON table lives in `scaffold.rs`, keyed
  by Java type name. The canonical answer is another column on the one builtin
  row, which is what that table is for.
- The runner reads rows through the `json` capability's reader, so `seed` gains
  a fourth prerequisite the validator does not check today (it checks `repo`,
  `storage postgres` and `platform spring`).
- Three artifacts, not one: the row file, the `@Profile("seed")` runner, and
  the test that proves the shipped rows still bind to the record.

**Reproduce:**

```
# a canonical Spring project with `storage postgres`
printf 'use repo for Note\nuse seed for Note\n' >> .jails/model.jdl
jails sync
# was: "synchronized ... 8 files written" plus testkit/NoteFactory.java
# now: "the canonical compiler has no seed emitter yet: ... fix: ..."
```
