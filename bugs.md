# bugs.md — dogfooding the edit/change-your-mind loop

Initial session: 2026-08-25. Binary: `jails 0.1.0` built from this checkout.

**Recheck: 2026-08-26 #3 (HEAD `3a023c0`).** Rebuilt and reinstalled `jails 0.1.0`
from current HEAD (previous pass was `0c369dd`; 4 commits since, all in the
command-result/JSON and prepared-report area). Direct reproductions in disposable
`/tmp/jails-dogfood-run3/*` projects, with real PostgreSQL via `jails migrate
--check` and real `mvn -o test-compile` wherever a claim needed a compiler.

- **Verified still broken, reproduced verbatim:** B2/B2a, B3, B5, B12, B14, B17,
  B18, B19, B20, B22, B25a, B33, B34, B35, B36, B37, B38.
- **Amended, subject survived but its face changed:**
  - **B1:** the one-way door has *opened* -- `g scaffold Book` after
    `destroy --storage drop` now exits 0 -- but it writes **no create-table
    migration**, so the regenerated JDBC adapter queries a table the migration
    history dropped. `migrate --check` and `doctor` are both green over it. The
    crash became a silent incoherence; severity unchanged.
- **Corrected in place (report still valid, detail was stale):**
  - **B38:** `A` is no longer the only one-letter name that trips it. `I`
    pluralises to `is`, also reserved, also `syntax error at or near "is"`.
    Swept `Status Datum Person Index News Order OrderService ThingRepository
    FooController BarTest PackageInfo Application Config Main A I O` -- only `A`
    and `I` fail.
  - **B19:** two more internal citations found, both still shipping.
- **New:** **B39** (critical) `g field` silently breaks every companion generated
  against the entity, and `doctor` stays green over a project that will not
  compile -- found by the section-20 audit and bisected to five commands;
  **B40** (high) `jails rename` panics on any non-ASCII byte in any scanned
  `.java` file, including jails' own `g idempotency` output; **B41** (high) a
  closed refusal loop between `migrate --check`, the migration seal and
  `resource repair`; **B42** (medium) `--output json` writes zero bytes on every
  failure; **B43** (medium) `jails add format` invalidates jails' own recorded
  output and `doctor` then reports it as the developer's edits; **B44** (low)
  `jails explain`'s help text is the completion command's.
- **Not retested:** **B10** -- port 5432 is still held by an unrelated project's
  container (`my-minicom-postgres-1`) and restarting it would disrupt that work.
- **Skipped / untestable:** live Gradle execution (still no `gradle` binary).
- **No jails source, test, build or doc file was modified in this pass.** Every
  reproduction ran in a disposable `/tmp/jails-dogfood-run3/*` project;
  `bugs.md` and the untracked `dogfood.md` run log are the only files written.
  (`git status` also shows `src/cli.rs`, `src/main.rs` and
  `crates/jails-engine/src/route/artifact.rs` modified, timestamped *after* this
  pass's last write and by no command this pass ran -- they are another session's
  concurrent edits, as in the previous pass, and are not this pass's.)

---

**Recheck: 2026-08-26 #2 (HEAD `0c369dd`).** Rebuilt and reinstalled `jails 0.1.0`
from current HEAD (the previous pass in this file was `33abf9e`; 14 commits since).
Direct reproductions in disposable `/tmp/dg*` projects, with real PostgreSQL via
`jails migrate --check` and real `mvn -o test-compile` wherever the claim needed a
compiler.

- **Removed, proved fixed:**
  - **B8:** `jails new` leaves no `.jails-new.lock` behind in the parent directory.
  - **B9:** `migrate --check` runs clean with a foreign container already bound to
    `:5432` -- it now uses its own ephemeral-port scratch container and prints no
    compose error.
  - **B11 / B11a:** `g record X` then `g scaffold X` no longer splits the entity.
    `g field` afterwards updates all ten surfaces, and the field list survives
    `destroy record X` -- no orphaned columns, all surfaces agree.
  - **B13:** a hand-deleted generated file is now reported
    (`FAIL managed Order  recorded output ... is missing`) with a working fix,
    `jails resource repair <Entity> --strategy roll-forward`, which restores it.
  - **B21:** `destroy enum Status` refuses while `scaffold Post` references it.
  - **B23:** `remove db --force` refuses while a scaffold depends on it; every
    capability with no dependants (`api`, `kafka`, `redis`, `security`,
    `observability`, `format`) still round-trips add/remove cleanly.
  - **B25:** `--package` artifacts are recorded now: `g field <E> --package <p>`,
    `destroy` and `resource status` all reach them. (The residue is folded into
    B25a; `g field` *without* the flag still says the entity is not recorded.)
  - **B27:** a single-line Gradle `dependencies { ... }` block is spliced into
    correctly and idempotently -- entries land inside the braces, no duplicates.
  - **B28:** `destroy scaffold Parent` refuses while an association points at it
    (but see **B37** -- the refusal it offers is a dead end).
  - **B31:** `id`/`Id` and `userId`/`user_id` are refused at parse time, naming
    both Java names and the colliding column.
  - **B32:** `hashCode`, `toString` and `equals` are refused as record components.
- **Verified still broken:** B1, B2/B2a, B3, B5 (new, cleaner reproduction), B12,
  B14 (now partial), B17 (now partial), B18, B19 (partial -- new leaks), B20, B22,
  B25a.
- **Corrected in place (report still valid, detail was stale):**
  - **B20:** the previous pass claimed that editing `.jails/app.toml` "permanently
    disables the imperative path". Reverting the manifest by hand changes nothing:
    the refusal is unconditional, because a direct field command always differs
    from the manifest by the field being added. Verdict and severity unchanged.
  - **B14:** its bullet list asked five questions; `doctor` now answers two of
    them, so those two were moved out of the open list and into the amendment.
  - **B5:** the original reproduction (the end state of B11, plus a hand-deleted
    file) no longer reproduces -- `doctor` catches both halves now. Replaced with a
    reproduction that still holds: a project broken only by jails' own output.
  - **B3 / B12:** rewritten around the `jails resource field` surface that now
    exists, which the previous pass predates.
- **New:** **B33** (critical) every mutating `resource field` subcommand fails on a
  storage-backed entity; **B34** (critical) `resource field` on a plain record
  writes `alter table` migrations for a table that never existed; **B35** (high)
  Java keyword entity names generate an uncompilable project; **B36** (high) an
  invalid entity name panics instead of refusing; **B37** (high) an entity in an
  association can never be destroyed -- both halves refuse and point at each other;
  **B38** (medium) entity `A` produces table `as`, which PostgreSQL will not create.
- **Not retested:** **B10** (`jails run` cold-start race) -- port 5432 is held by an
  unrelated project's container and restarting it would have disrupted that work.
  A commit named "Wait for database readiness before launch" landed in this range,
  so the report is likely stale; it is kept until it is actually executed.
- **Skipped / untestable:** live Gradle execution (no `gradle` binary; `gradlew`
  would need the network). B27's fix was verified by reading `build.gradle`.
- **No jails source, test, build or doc file was modified in this pass.** Every
  reproduction ran in a disposable `/tmp/dg*` project against
  `~/.cargo/bin/jails` built from `0c369dd`; `bugs.md` is the only file in this
  repository this pass wrote to. (`git status` also shows Rust sources modified
  from 11:59 onward -- those are another session's edits, made after this pass's
  11:55 build, and are not this pass's.)

---

## B1 — A destroyed entity can never be recreated. The project is wedged forever.

**Severity: critical.** This is a one-way door with no exit and no warning.

`destroy` writes a `drop table` migration but the *create* migration keeps its
original version number, and the seal then refuses to touch it. Regeneration
wants to write `V001__create_books.sql` again, hits the append-only seal, and
dies. The name is burned for the life of the repository.

```
jails new bookclub --package com.example.bookclub
cd bookclub && jails add db
jails g scaffold Book id:uuid@pk title:string! author:string isbn:string@unique pages:int
jails destroy scaffold Book --storage drop --confirm-table books --force   # succeeds
jails g scaffold Book id:uuid@pk title:string                               # dead forever
```
```
jails: migration-edited-after-seal: `src/main/resources/db/migration/V001__create_books.sql`
       is published append-only schema history and cannot be replaced or deleted.
       fix: keep its recorded bytes and append the next migration for the desired schema change.
```

Confirmed it is the create-migration name and not something about `Book`: a
fresh `Member` in the same project correctly got `V003__create_members.sql` (V002 was the drop).
Because the migration name derives from the table name, which derives from the
entity name, the collision is *stable* — retrying, renaming fields, or changing
the field list all hit the same wall.

Notes that make it worse:

- `destroy --storage drop --confirm-table books` is the **sanctioned, documented
  path**. It prints `applied`, exits 0, and reports no problem. Nothing at
  destroy time says "you will not be able to recreate this."
- The suggested fix is not actionable. There is no command that appends the next
  create-table migration under a fresh version, and the seal blocks doing it by
  hand under the recorded name.
- The only escape found was deleting the project and starting over.

**Expected:** re-creating a destroyed entity should emit
`V00N__create_books.sql` at the *next* free version, exactly as it does for a
never-before-seen entity.

---

**Recheck `3a023c0`: the refusal is gone; the project is silently wrong instead.**
The recreate now exits 0 and prints its `create` lines, but appends no
create-table migration -- so the schema history ends at the `drop`, while the
regenerated Java queries the table.

```
jails new one --package com.one --offline && cd one && jails add db
jails g scaffold Book id:uuid@pk title:string
jails destroy scaffold Book --storage drop --confirm-table books --force
jails g scaffold Book id:uuid@pk title:string          # exit 0
```
```
$ ls src/main/resources/db/migration/
V001__create_books.sql   V002__drop_books.sql          # and nothing else

$ grep -o 'from books\|into books' src/main/java/com/one/adapters/JdbcBookRepository.java
from books
from books
into books

$ jails migrate --check
  ok    V001__create_books.sql
  ok    V002__drop_books.sql
  2 migration(s) applied cleanly to a scratch database.
$ jails doctor
25 checks, all clear.
$ mvn -o test-compile        # exit 0
```

Three oracles call this project healthy and one does not: `jails resource status
Book` reports `state: drop-pending`, which is the only place the truth appears.
Every query against `books` fails at runtime.

This is strictly worse than the old refusal, which at least stopped. The
severity stays critical and the expectation is unchanged -- the recreate must
append `V003__create_books.sql`.

---

## B2 — `rename` cannot finish a storage-backed entity, and even a successful record rename corrupts the ledger

**Severity: critical.** `jails rename --help` still promises "Rename a type and every reference to it (files, companions, call sites)". Two current outcomes, both dead ends.

**Scaffold with a create-table migration — rename aborts, files unchanged.**

```
jails g scaffold Member id:uuid@pk name:string! email:string@unique
jails rename Member Reader --force
```
```
10 file(s), 39 occurrence(s), 2 file rename(s).
jails: `src/main/resources/db/migration/V001__create_members.sql` was not captured, so planning may not read it.
       fix: declare it in the read set. Reaching past the snapshot would decide on a fact nothing recorded, and the commit-time staleness check would have nothing to compare.
```

Exit 1. Every `Member*` file is still on disk. `g field Reader` / `destroy scaffold Reader` say the name is not recorded; `g field Member` still works. The silent companion-corruption path from the previous pass is no longer reachable — rename no longer applies — but a storage-backed entity still cannot be renamed. The error is an internal planning leak (B19): "declare it in the read set" is not something a user can do. The "10 file(s)..." counts print as if work happened; it did not.

The previous B2b/B2c (destroy after rename dropping storage policy and stranding tests) were not re-hit because the rename never committed.

**Plain `g record` — files rename, ledger bases do not. B2a still holds.**

```
jails g record Member name:string!
jails rename Member Reader --force     # exit 0; Member.java/Test -> Reader.java/Test
jails g field Reader nickname:string?
```
```
jails: `src/main/java/com/names/domain/Reader.java` is jails' own output and its bytes differ from what
       this would write, but the store has not recorded the bytes jails wrote -- so it cannot tell your edits from a regeneration.
       fix: destroy and regenerate, or keep the file. It was written before this jails recorded output bases.
```

The message is still factually wrong: this binary wrote `Reader.java` seconds earlier. The offered fix is still B1 if the type ever grew a table.

**Recheck `0c369dd`: unchanged, and the new lifecycle commands do not help.**
`rename Member Reader --force` on a scaffold still exits 1 with the same
`was not captured, so planning may not read it` / `declare it in the read set`
leak, files untouched. On a plain `g record`, `rename Note Memo --force` still
succeeds and still poisons the base, so `g field Memo extra:string?` refuses with
`it was written before this jails recorded output bases` seconds after this binary
wrote it. `jails resource status Memo` corroborates the corruption in its own
vocabulary (`state: ambiguous`, `lifecycle-not-recorded: the entity predates
lifecycle adoption`) -- for a file created two commands earlier. And the repair
route contradicts itself:

```
$ jails resource repair Memo --strategy roll-forward
jails: no resource lifecycle matches `Memo`.
       fix: run `jails resource status Memo` to inspect recorded identities
$ jails resource status Memo        # exit 0, reports the resource
```

The fix line names a command that succeeds, from a command that says the thing
does not exist.

---

## B3 — No way to rename, retype, or drop a field. At all.

**Severity: high.** This is the single most common edit and there is no command
for it. Combined with B1 there is no workaround either.

```
$ jails g field Loan dueOn:instant          # dueOn is currently :date
jails: `scaffold Loan` already has a `dueOn` component.
       fix: choose another name. Removing or changing a component is a data
       migration, and jails does not write one it cannot check against the rows.

$ jails destroy field Loan dueOn
error: unexpected argument 'dueOn' found

$ jails g scaffold Book title:string! author:string isbn:string@unique pageCount:int
jails: migration-edited-after-seal: ... V001__create_books.sql ...
```

Three doors, three refusals. The escape hatch is `destroy scaffold` + regenerate
— which B1 makes permanently fatal. So **`pages` → `pageCount` is not
expressible in jails**, on any path.

The caution is defensible on its own terms (a type change *is* a data
migration). But "choose another name" is not a fix for a rename, and refusing
every path including the safe ones is what makes the tool feel hostile to
ordinary editing. Dropping a nullable column with no data, or renaming a column
via `alter table ... rename column`, are both mechanically safe and both
refused.

**Suggested minimum:** `jails g field <Entity> <old> --rename <new>` emitting
`alter table ... rename column`, and `jails destroy field <Entity> <name>`
gated the same way `--storage drop --confirm-table` is.

---

**Recheck `0c369dd`: partially addressed, and only for entities with no table.**
A field-evolution surface now exists -- `jails resource field add|rename|type|
nullability|drop` -- and on a plain `g record` it works: `resource field rename Tag
label name --column single-cutover` and `resource field drop Tag name
--confirm-column name` both commit. On a **scaffold** -- the case this report is
about, and the only case with a column to migrate -- every mutating subcommand
fails outright (**B33**), and on a record it writes migrations against a table
that does not exist (**B34**). `jails g field <E> <old> --rename <new>` and
`jails destroy field <E> <f>` are still `error: unexpected argument`, and neither
refusal mentions `jails resource field`, so the new surface is unreachable from
the commands a reader would try first.

---

## B5 — `doctor` reports "25 checks, all clear" on a project that does not compile

**Severity: medium.** *Amended `0c369dd`: the original reproduction is fixed, a
cleaner one is not.* `doctor` gained a real project check -- `managed <Entity>`
reports a recorded output that is missing (`FAIL`) or edited since the last commit
(`warn`), and it caught both a hand-deleted `InMemoryOrderRepository.java` and a
hand-deleted `V003__create_items.sql`. That is the B13/B14 gap closing.

What it still cannot see is a project broken entirely by jails' *own* output:

```
jails new kw --package com.kw --offline && cd kw && jails add db
jails g scaffold enum id:uuid@pk x:int      # exit 0 -- see B35
jails doctor                                 # 25 checks, all clear
mvn -o test-compile                          # 25 files fail to compile
```

Every file on disk is byte-identical to what jails wrote, so the drift check is
correct and silent; the project still does not build. The same hole shows up after
B18: `jails resource repair Order --strategy roll-forward` over a half-applied
transaction re-records the torn state as the new base and `doctor` returns to
`25 checks, all clear` while the JDBC insert names a column no migration creates.

**Expected:** a check that a recorded entity's field list matches the columns its
migrations created -- the one question that separates "the bytes are the ones I
wrote" from "the project is coherent".

---

## B10 — `jails run` starts Spring Boot before PostgreSQL is ready for TCP connections (startup race)

**Severity: medium (intermittent on cold start).** Not retested on HEAD `33abf9e`
— Postgres was already accepting connections on `:5432`, and restarting it would
have disrupted other projects. The previous reproduction stands.

`jails run` invokes `docker compose up -d` for postgres, but immediately hands off to
`mvn spring-boot:run` without waiting for the database server inside the container
to accept connections.

On a cold start or when the postgres container is still initializing, Spring Boot
boots up, HikariCP / Flyway tries to connect, and the socket connection fails:

```
Caused by: org.flywaydb.core.internal.exception.FlywaySqlUnableToConnectException: Unable to obtain connection from database: The connection attempt failed.
SQL State : 08001
Caused by: java.net.SocketException: Connection reset
```

Spring cascades this failure up as an `UnsatisfiedDependencyException` on
whatever bean injects `JdbcClient` (e.g. `flywayInitializer` -> `jdbcClient` ->
`jdbcMessageRepository`), and `jails run` reports:

```
jails: the application failed to start, even though .../mvnw reported success.
(spring-boot-devtools runs main on its own thread and swallows the exception.)
A bean could not be constructed because one of its dependencies could not be
```

Running `jails run` a few seconds later (once the database is warm and accepting
connections) succeeds immediately without changes.

**Expected:** `jails run` should probe database readiness (e.g. TCP socket check or
`pg_isready` with a short timeout, matching what `jails doctor` checks) before launching
the Spring Boot process.

---

## B12 — a one-character typo in a field name is permanent, in all six surfaces

**Severity: high.** The single most common mistake there is, and there is no undo.

```
jails g field Customer phoen:string?      # typo
jails destroy field Customer phoen        # error: unexpected argument 'phoen'
jails g field Customer phone:string?      # so now you have both
```

Result — every surface now carries both spellings, forever:

```
record   : UUID id, String email, String name, Optional<String> phoen, Optional<String> phone
request  : @NotNull UUID id, ..., String phoen, String phone
response : UUID id, ..., String phoen, String phone
sql      : add column phoen text; | add column phone text;
insert   : (id, email, name, phoen, phone)
http     : id email name phoen phone
```

`destroy field` does not exist (B3), and `destroy scaffold` + regenerate is the
one-way door (B1). The typo ships.

A `rename column` is mechanically safe on a column no code reads yet, and jails
knows it just wrote it. This is the case that most deserves an escape hatch.

---

**Recheck `0c369dd`: still reproduced verbatim** on a clean scaffold --
`g field Order phoen:string?` lands in ten surfaces, `destroy field` is still
`error: unexpected argument 'phoen' found`, and `g field ... --rename phone` is
still `error: unexpected argument '--rename'`. The `jails resource field rename`
route that would fix this refuses on every scaffold (**B33**), so the typo still
ships.

---

## B14 — `doctor` and `sync` have no notion of entity drift at all

**Severity: medium.** This is the umbrella over B12 and B18 (B11 and B13, its
original members, are fixed).

Everything in this file that ends in a broken or silently-wrong project is
detectable from two things jails already has in hand: the ledger, and the files
on disk. None of it is checked. Concretely, `doctor` never asks:

- does a recorded entity's field list match the columns its migrations created?
  (the one that would catch B18's tear)
- is there a `create table` in the migrations with no live entity claiming it?
- do the record, the request DTO, the JDBC insert and the fixture agree on the
  component list?

`doctor` today answers "is your *environment* healthy" — Maven, JDK, Docker,
ports, containers. That is 25 of 25 checks. After an edit goes wrong, the
question the reader actually has is "is my *project* still coherent", and
nothing answers it. `capability_drift_checks` already does exactly this shape of
work for capabilities; entities have no equivalent.

**Recheck `0c369dd`: half of this is fixed.** `doctor` now has a `managed
<Entity>` check answering the two most mechanical of these questions -- a ledger-recorded file
missing from disk, and one whose bytes changed since the last commit -- and
`jails resource repair <Entity> --strategy roll-forward` restores the missing one.
`jails sync` is unchanged: it still prints `applied ... ledger replace` over a
missing file and restores nothing, so the command whose name promises this is
still not the command that does it.

Still unasked, and still the ones that matter after an edit goes wrong:

- does a recorded entity's field list match the columns its migrations created?
  (this is what leaves B18 green)
- is there a `create table` in the migrations with no live entity claiming it?
  (B22 produces exactly this)
- do the record, the request DTO, the JDBC insert and the fixture agree?

And the new check has a blind spot of its own: a migration written by
`jails resource field` is not recorded as managed output, so deleting
`V004__rename_label_to_name.sql` by hand leaves `doctor` at `all clear`, while
deleting the create migration beside it is caught (**B34**).

---

## Design question, not a bug — the POST body requires the client to invent the id

`CustomerRequest` renders the `@pk` component as `@NotNull UUID id`, and
`customer.http` posts a hardcoded
`"id": "00000000-0000-0000-0000-000000000001"`. Nothing generates an identity
server-side. That is a defensible choice (it makes creates idempotent), but it
is unstated, and posting the sample body twice will violate the primary key.
Worth a line in `explain scaffold` either way.

---

## B17 — a git merge conflict in the ledger is misdiagnosed, and the only fix offered bricks the project

**Severity: critical.** `.jails/ledger.toml` is a checked-in, hex-encoded single
line. Two developers each generating something is a guaranteed conflict, and it
is unresolvable by hand.

All three realistic corruptions are *detected* — good — but all three get the
same wrong diagnosis and the same destructive fix:

```
# truncated (crash / partial write)
jails: ... cannot be read by this jails: ledger does not end with a newline
       fix: it was written by a different version. Delete `.jails/` to start its
             history over, or use the jails that wrote it.

# git conflict markers
jails: ... expected a line `schema = …`, found `<<<<<<< HEAD`
       fix: it was written by a different version. Upgrade to, or use, the jails
             version that wrote it; this version will not treat unknown state as empty.

# empty file
jails: ... ledger does not end with a newline
       fix: it was written by a different version. ...
```

None of these was written by a different version. A file with `<<<<<<< HEAD` in
it has one obvious cause and one obvious fix, and neither is mentioned.

### Following the advice bricks the project

`Delete .jails/ to start its history over` is offered as the primary fix. It is
not recoverable:

```
$ rm -rf .jails
$ jails g field Booking extra:int
jails: no `Booking` is recorded in this project.
$ jails destroy scaffold Booking
jails: no `scaffold Booking` is recorded in this project.
$ jails g scaffold Booking id:uuid@pk from:date to:date guest:string nights:int
jails: `src/main/resources/db/migration/V001__create_bookings.sql` already exists
       and jails did not write it.
```

Every entity in the project becomes permanently uneditable and undestroyable,
and re-recording is blocked by the migration that is already on disk. (Only
`jails.toml` survives, so capabilities still work — `doctor` still reports
`ok capability db`, which makes the project look healthy.)

**Expected:** name the actual cause when it is knowable (conflict markers are
unmistakable), and never make "delete the store" the headline fix for a
recoverable file. A `jails ledger repair`/`--reconstruct` that re-records
entities from the files on disk is the missing command; failing that, the advice
should be "restore `.jails/ledger.toml` from git" — which is correct, safe, and
was not mentioned.

---

**Recheck `0c369dd`: the destructive half is fixed, the diagnosis is not.**
`Delete .jails/ to start its history over` is gone from every one of the three
corruptions -- so following the advice no longer bricks the project, which was the
critical part of this report. What is left is medium severity: all three still
report `it was written by a different version`, none of them was, and none offers
a way back.

```
# truncated / empty
jails: .../ledger.toml cannot be read by this jails: ledger does not end with a newline
       fix: it was written by a different version. Upgrade to, or use, the jails
            version that wrote it; this version will not treat unknown state as empty.
# git conflict markers
jails: ... ledger has 13 line(s); schema 2 is exactly five, in a fixed order
       fix: it was written by a different version. ...
```

A file containing `<<<<<<< HEAD` has one cause and one fix, and
`git checkout -- .jails/ledger.toml` -- correct, safe, and the thing the reader
should do -- is still not mentioned.

---

## B18 — a failed write tears the transaction in half, then jails blames the developer for its own writes

**Severity: critical.** Triggered by anything that makes one path unwritable
mid-transaction — a root-owned directory, a full disk, NFS, an IDE lock.

```
$ chmod 555 src/main/resources/db/migration
$ jails g field Booking zzz:string?
jails: RecoveryBlocked(Unreadable { error_kind: "could not publish
       .../V010__add_zzz_to_bookings.sql: Permission denied (os error 13)" })
```

The command failed. The project did not roll back:

```
record  : ... int nights, Optional<String> tag, Optional<String> zzz   <-- has zzz
request : has zzz
insert  : insert into bookings (id, from, to, guest, nights, tag, zzz) <-- has zzz
migrations for zzz: 0                                                  <-- NONE
```

The Java now reads and writes a column that no migration will ever create. Every
insert fails at runtime with `column "zzz" does not exist`. This is the
DB/code divergence again (B11a), reached this time by a transient filesystem
error rather than a mistake.

### And the next command accuses you of editing

Having half-written five files, jails now reads its own output as the
developer's hand edits:

```
$ jails g field Booking q:int
jails: 5 file(s) have places where your edit and the generator's change overlap:
         requests/booking.http (1 place(s))
         src/main/java/com/hotel/adapters/JdbcBookingRepository.java (4 place(s))
         src/main/java/com/hotel/domain/Booking.java (1 place(s))
         src/main/java/com/hotel/web/BookingRequest.java (2 place(s))
         src/main/java/com/hotel/web/BookingResponse.java (2 place(s))
       fix: committing marker bytes with a resumable pending conflict is plan.md
            §R5.4, which is not wired to this route yet. Move your version aside,
            or destroy and regenerate.
```

Nobody edited those files. The entity is now permanently in conflict with
itself, and the only offered escape is `destroy and regenerate` — B1, the
one-way door.

**Expected:** the write phase is already transactional (there is a journal and a
`RecoveryBlocked` state, so the machinery exists). A publish that cannot complete
must roll back or roll forward, never stop half-applied — and it must not record
its own partial output as a new base that the next run diffs against.

---

**Recheck `0c369dd`: the wording improved, the tear did not.** The error is prose
now rather than `RecoveryBlocked(Unreadable { ... })`:

```
$ chmod 555 src/main/resources/db/migration
$ jails g field Order zzz:string?
jails: a file could not be read (could not publish .../V005__add_zzz_to_orders.sql:
       Permission denied (os error 13)).
       fix: make it readable and run the command again.
```

The project is still torn exactly as before: `Order.java`, `OrderRequest.java`,
`OrderResponse.java`, `order.http` and `JdbcOrderRepository.java` all carry `zzz`,
`insert into orders (id, total, note, note2, phoen, zzz)`, and **no migration
creates the column**. The next command still reports the five files as the
developer's edits (the `plan.md §R5.4` citation is gone, so B19 is fixed on this
path, but the accusation stands).

The new part is worse. `doctor` now flags the five files as
`changed since the last jails commit`, and its own advertised repair adopts them:

```
$ jails resource repair Order --strategy roll-forward
applied 64e55a92...   ledger replace
$ jails doctor
25 checks, all clear.
```

The tear is now the recorded truth. `doctor` is green, the build is green, and
every insert will fail at runtime with `column "zzz" does not exist`.

---

## B19 — internal Rust `Debug` output and a deleted design document leak into user-facing errors

**Severity: low individually, corrosive in aggregate.** Three of the failures
above reported themselves like this:

```
jails: StaleInput("`src/main/java/com/hotel/domain` does not hold what it held
       when this plan was made.
       fix: something was added or removed
       after the plan read it; replan.")

jails: RecoveryBlocked(Unreadable { error_kind: "could not publish ...:
       Permission denied (os error 13)" })
```

Both are `{:?}` on an internal enum: the variant name, the quotes, the struct
braces, and a literal `
` where a newline was meant. Every other jails error in
this file is well-written prose with a `fix:` line — these ones bypassed it.

Worse, B18's fix line ships an internal citation to the reader:

> `fix: committing marker bytes with a resumable pending conflict is plan.md
> §R5.4, which is not wired to this route yet.`

`plan.md` was deleted from the repository (per `CLAUDE.md`, it resolves only
through `git show`). A user-facing error should not cite a document the user
cannot open, and should not tell them a feature is unimplemented as though that
were an action they can take.

---

**Recheck `0c369dd`: partially fixed.** The two `{:?}` leaks quoted above are gone
-- the permission failure is prose (see B18) and `plan.md §R5.4` no longer ships to
the reader. Two internal leaks remain, both on paths a reader reaches on their
first attempt:

```
$ jails resource field rename Customer phoen phone --column single-cutover
jails: resource Intent(IntentId { recipe: Scaffold, name: Name("Customer"),
       package: Package("com.svc") }) no longer has the expected source path.

$ jails rename Member Reader --force
jails: `.../V001__create_members.sql` was not captured, so planning may not read it.
       fix: declare it in the read set.
```

The first is `{:?}` on `IntentId`; the second asks the reader to do something only
the implementation can do.

**Recheck `3a023c0`: both leaks above still ship, and two more were found.**
`resource field rename` on a scaffold still prints `{:?}` on `IntentId` (B33) and
`rename` still says `declare it in the read set` (B2). New this pass, both in
text a reader meets before any failure:

- `jails g --help` documents `--output` with `§R3.4 makes a command's result a
  *value* ...` -- a citation to a document that is not in the repository, shipped
  in `--help` rather than in an error.
- `jails explain --help` prints the *completion* command's description (**B44**).

---

## B20 — on a manifest project, adding a field to an existing entity is impossible by *either* path

**Severity: critical.** `.jails/app.toml` is the declarative "declare what you
want and let reconciliation work out the difference" path — the one designed for
changing your mind. It cannot express a changed mind about a field.

Project created from a manifest (`jails new crm --app app.toml`):

```toml
[[generate]]
kind = "scaffold"
name = "Lead"
fields = ["id:uuid@pk", "email:string@unique", "score:int"]
```

Add one field to that list and re-apply:

```
$ jails app apply
jails: migration-edited-after-seal: `.../V001__create_leads.sql` is published
       append-only schema history and cannot be replaced or deleted.
```

So use the imperative path instead:

```
$ jails g field Lead note:string?
jails: `scaffold Lead` is wanted differently by a direct command and the app
       manifest.
       fix: make the declarations agree — applying one would be undone by the
            other's next run.
       a direct command wants: id:uuid@pk email:string@unique score:int note:string?
       the app manifest wants: id:uuid@pk email:string@unique score:int
```

The on-disk `.jails/app.toml` *already listed* `note:string?`. "the app manifest
wants" is the last *successfully applied* snapshot, not the file just edited.
`g field` with the same field the file now declares still refuses.

**Each path refuses and points at the other.** `app apply` says use a migration;
`g field` says make the declarations agree — and the only way to make them agree
is the edit `app apply` just rejected. There is no third path.

A field edit on one entity also poisons the rest of `app apply`: deleting an
unrelated `Deal` block in the same file still dies on `V001__create_leads.sql`.

It is worse than a refusal, because the manifest edit persists. Once
`app.toml` says `note:string?` and the ledger does not, *every* subsequent direct
command on `Lead` fails the "wanted differently" check. Editing a text file —
with no jails command run at all — permanently disables the imperative path for
that entity. Reverting the manifest by hand is the only way out, and nothing says
so.

### What the manifest *can* do

Scoped precisely:

| operation | result |
|---|---|
| re-apply an unchanged manifest | works, idempotent |
| add a whole new `[[generate]]` entity | works |
| delete a whole `[[generate]]` entity | works — destroys its files (but see B22) |
| **change an existing entity's `fields`** | **fatal, both paths** |

So the manifest is append-and-delete-only at entity granularity. For the
operation people actually perform daily — "this record needs one more column" —
the declarative path is not merely awkward, it is closed.

---

**Recheck `0c369dd`: still reproduced in full, with one detail corrected.** All
three doors are still shut -- `app apply` after adding one field to a `[[generate]]`
`fields` list dies on `migration-edited-after-seal`, and both `jails g field Lead
note:string?` **and** the new `jails resource field add Lead note:string?` refuse
with `wanted differently by a direct command and the app manifest`.

*Correction to the previous pass:* the claim that editing `app.toml` "permanently
disables the imperative path" overstated it. Reverting the manifest by hand
changes nothing, because the refusal is unconditional: on a manifest-owned entity a
direct field command always differs from the manifest by exactly the field being
added, so `a direct command wants: ... note:string?` versus `the app manifest
wants: ...` is the permanent state, not a stale snapshot. There is no edit to
`app.toml` that makes the direct path legal, and no `app apply` that accepts the
edit. The verdict is unchanged and the severity is unchanged.

---

## B22 — imperative and declarative removal disagree about data loss

**Severity: high.** The same intent, expressed two ways, gets two different
levels of care.

```
$ jails destroy scaffold Deal
jails: storage-policy-required: `Deal` is backed by table `deals`.
       fix: preserve it with `--storage preserve`, or plan data loss with
            `--storage drop --confirm-table deals`.
```

Delete the same entity's four lines from `app.toml` and re-apply:

```
$ jails app apply
  delete src/test/java/com/crm/domain/DealTest.java
  delete src/test/java/com/crm/service/DealServiceTest.java
  ... (all Java deleted)
  ledger replace
```

No storage policy. No confirmation. No `drop table` migration —
`V002__create_deals.sql` is left in place, so the table survives with no code
that knows about it, and nothing reports the orphan (B14).

The manifest has no syntax for expressing storage intent, so the ceremony the
imperative path insists on cannot even be written down in the declarative one.

*(One accidental upside: because the create migration is never removed, re-adding
`Deal` to the manifest works. The declarative path escapes B1 precisely by doing
less than it should.)*

---

**Recheck `0c369dd`: still reproduced.** Deleting the `Deal` block from
`.jails/app.toml` and re-applying deletes every Java file with no storage policy,
no confirmation and no `drop table` migration; `V002__create_deals.sql` stays and
the table survives with nothing claiming it. The imperative path still insists on
`--storage drop --confirm-table deals` for the same intent.

---

## B25a — `--package` is relative, and a fully-qualified value is silently doubled

`--package` is interpreted relative to the base package. Passing the value that
appears at the top of every generated file — the fully-qualified one — is the
obvious mistake, and nothing catches it:

```
$ jails g scaffold Invoice ... --package com.ops.billing
  create src/main/java/com/ops/com/ops/billing/InvoiceService.java
$ head -1 .../InvoiceService.java
package com.ops.com.ops.billing;
```

It compiles, so nothing ever reports it. Combined with B25 the files are also
unreachable, so the only fix is `rm -rf` by hand.

**Recheck `0c369dd`: the doubling is unchanged.**
`jails g scaffold Invoice id:uuid@pk amount:decimal --package com.svc.billing`
still writes `src/main/java/com/svc/com/svc/billing/InvoiceService.java`. The
files *are* recorded now (B25 is fixed), so they can be evolved and destroyed --
under the doubled package, which makes the mistake durable rather than merely
present.

The other residue from B25: an entity generated with `--package billing` is only
reachable when the flag is repeated. `jails g field Refund memo:string?` answers
`no Refund is recorded in this project` and suggests scaffolding it again, while
`jails resource status Refund` reports `Refund@com.svc.billing, state: consistent`
in the same directory. One of those two is wrong, and the one that is wrong is the
one telling the reader to re-create an entity that exists.

**Expected:** reject (or normalise) a `--package` value that already starts with
the project's base package, and resolve an entity by name when exactly one
package holds it.

---

---

## B33 — every mutating `resource field` subcommand fails on a storage-backed entity

**Severity: critical.** `jails resource field` is the answer to B3 and B12 — rename,
retype, change nullability, drop — and on a scaffold, which is the only kind that
has a column to migrate, none of it works.

```
jails new svc --package com.svc --offline && cd svc && jails add db
jails g scaffold Customer id:uuid@pk email:string phoen:string?
jails resource field rename Customer phoen phone --column single-cutover
```
```
jails: resource Intent(IntentId { recipe: Scaffold, name: Name("Customer"),
       package: Package("com.svc") }) no longer has the expected source path.
       fix: rerun the command against the path reported by `resource status`
```

Exit 1, nothing written. Identical failure for `resource field drop Customer phoen
--confirm-column phoen`, `resource field type Item qty --to long --strategy safe`,
and `resource field nullability Customer email --nullable`. `resource field add`
is the one subcommand that works. On a plain `g record` the same rename and drop
succeed (and then produce B34), so the failure is specific to entities that have a
table — the case the feature exists for.

The advice is a dead end in two ways. `resource status` takes a selector, not a
path, and it reports the resource as healthy:

```
$ jails resource status Customer
resource: Customer@com.svc
state: consistent
declaration: present
generated: present
migration-history: present
table: customers
```

There is no path in that output to rerun against, and the command that says the
source path is wrong is contradicted by the command it tells you to consult. The
message body is also `{:?}` on `IntentId` (B19).

**Expected:** on a scaffold whose `resource status` is `consistent`, a rename/drop/
retype should plan its migration and commit — and if some precondition is genuinely
missing, name it in the reader's vocabulary rather than printing an internal id.

---

## B34 — `resource field` on a plain record writes `alter table` for a table that never existed

**Severity: critical.** Silently unappliable migration history, from a command that
exits 0.

```
jails new svc --package com.svc --offline && cd svc && jails add db
jails g record Tag id:uuid@pk label:string?        # a record: no create-table migration
jails resource field rename Tag label name --column single-cutover   # exit 0
jails resource field drop   Tag name  --confirm-column name          # exit 0
```

```sql
-- V004__rename_label_to_name.sql
alter table tags rename column label to name;
-- V008__drop_name.sql
alter table tags drop column name;
```

There is no `tags` table anywhere in the project — `g record` creates none. Verified
against real PostgreSQL:

```
$ jails migrate --check
  ok    V001__create_customers.sql
  ok    V002__add_note_to_customers.sql
  ok    V003__create_items.sql
  FAIL  V004__rename_label_to_name.sql
  psql:<stdin>:3: ERROR:  relation "tags" does not exist
```

Every migration after it is now unreachable, and the whole history is unappliable
in any environment.

Two things make it worse than a bad file:

- **`doctor` cannot see it.** These migrations are not recorded as managed output,
  so hand-deleting `V004__rename_label_to_name.sql` leaves `25 checks, all clear`,
  while hand-deleting the neighbouring `V003__create_items.sql` — written by
  `g scaffold` — is correctly reported as `FAIL managed Item recorded output ... is
  missing`. The command's own migration is outside the check that exists.
- The drop migration also names no table in its filename (`V008__drop_name.sql`),
  so a repository with two entities gets colliding, uninformative version names.

**Expected:** refuse a physical-column operation on an entity with no table,
naming the entity's kind — a `record` has no columns to migrate.

---

## B35 — a Java keyword as an entity name is accepted and the project stops compiling

**Severity: high.** Nothing refuses it, and every surface is generated.

```
jails new kw --package com.kw --offline && cd kw && jails add db
jails g scaffold class id:uuid@pk x:int      # exit 0, ~15 files + V001__create_classes.sql
mvn -o test-compile
```
```
[ERROR] .../service/ClassService.java:[33,30] <identifier> expected
[ERROR] .../web/ClassResponse.java:[21,43] <identifier> expected
[ERROR] .../app/ClassRepository.java:[24,26] <identifier> expected
```

The *type* name is fine (`Class`); what breaks is the derived lowercase
**variable** name, which is the keyword itself (`Class class`). Tested across
keywords in one project — `class`, `enum`, `int`, `new`, `null`, `static`, `void`
all generate and all fail `javac` (25 files across 6 entities). `record` and `var`
are restricted identifiers and compile fine, correctly.

The recovery is B1: each accepted name has already sealed a `create table`
migration, so destroying and regenerating under a corrected name is the one-way
door.

**Expected:** the same parse-time refusal that field names already get
(`field name 'hashCode' conflicts with ...`), applied to the entity name, checking
the *derived identifier* rather than only the capitalised type.

---

## B36 — an invalid entity name panics instead of refusing

**Severity: high.** Exit 101 and a Rust panic message, where every neighbouring
mistake gets a `jails:` line and a `fix:`.

```
$ jails g scaffold 'Bad!Name' id:uuid@pk x:int
thread 'main' (2553082) panicked at crates/jails-generate/src/sql.rs:378:10:
generated names are validated before SQL projection: Told("name `Bad!Name` contains
`!`, which is not valid in a Java identifier")
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace
$ echo $?
101
```

Same panic, same line, for `9Lives`, `Foo-Bar`, `Foo Bar` and `Ünïcode`. Nothing is
written, so the damage is confined to the message — but the message tells the
reader the tool is broken rather than that their input is, and the assertion text
says the input was *supposed* to have been validated earlier, which is where the
check belongs. By contrast `jails g scaffold Good id:uuid@pk x:nosuchtype` exits 1
with a clean list of valid types.

**Expected:** exit 1 with the panic's own sentence rendered as a `jails:` refusal —
the wording is already correct, it is simply arriving through `unwrap`.

---

## B37 — an entity in an association can never be destroyed: both halves refuse and point at each other

**Severity: high.** Found while confirming B28's fix. The refusal B28 asked for
exists; the way out of it does not.

```
jails g scaffold Parent id:uuid@pk name:string
jails g scaffold Child  id:uuid@pk parentId:uuid title:string
jails g association ChildParent --on Child --yields Parent parentId=id

$ jails destroy scaffold Parent --storage drop --confirm-table parents --force
jails: removing `scaffold Parent` would leave `association ChildParent` pointing at nothing.
       fix: remove the dependant first, or keep a declaration that owns `scaffold Parent`.

$ jails destroy association ChildParent --force
jails: migrations, associations, and field changes are forward-only; create a new
       migration instead of destroying one
```

Exit 1 both times. "Remove the dependant first" names an operation the tool
refuses on principle, and the second refusal offers no command at all — "create a
new migration" is not something `jails` will do for an association, and doing it
by hand does not retire the ledger row that the first refusal is reading.

The same two commands in the other order fail the same way. The entity is
permanently undestroyable, which also means the association is permanently
unremovable — an ordinary modelling reversal ("these two should not be linked
after all") has no expression.

**Expected:** one of the two has to open. Either `destroy association` retires the
row and appends a `drop constraint` migration, or the parent's refusal names a
concrete command (`--cascade`, or the association destroy it would accept) rather
than an operation that does not exist.

---

## B38 — a one-letter entity name produces table `as`, which PostgreSQL will not create

**Severity: medium.** Reserved *field* names are refused (B16, fixed); the table
name derived from the entity name is not checked at all.

```
jails g scaffold A id:uuid@pk n:string        # exit 0
```
```
$ jails migrate --check
  FAIL  V001__create_as.sql
  psql:<stdin>:8: ERROR:  syntax error at or near "as"
  LINE 1: create table as (
```

`A` pluralises to `as`, which is a PostgreSQL reserved word. Swept the obvious
neighbours in one project — `Table`, `Group`, `Select`, `Where`, `User` all
pluralise clear of the reserved list (`tables`, `groups`, `selects`, `wheres`,
`users`) and apply cleanly — so `A` is currently the only name found that trips it,
but it is a name the tool advertises no restriction on. The Java compiles, so
nothing reports the problem until a migration is run, and the create migration is
sealed by then (B1).

**Expected:** the parse-time check that already refuses reserved *column* names,
applied to the derived table name.

**Recheck `3a023c0`: still broken, and `A` is not the only one.** `I` pluralises
to `is`, equally reserved:

```
$ jails g scaffold I id:uuid@pk v:int     # exit 0 -> V001__create_is.sql
$ jails migrate --check
  FAIL  V001__create_is.sql
  psql:<stdin>:8: ERROR:  syntax error at or near "is"
```

Swept seventeen names in one project (`Status Datum Person Index News Order
OrderService ThingRepository FooController BarTest PackageInfo Application
Config Main A I O`); all generate, and only `A` (`as`) and `I` (`is`) fail to
apply. `O` -> `os` is fine. See also **B41**: the failure message's own advice
walks the reader into a closed loop.

---

## B39 — `g field` silently breaks every companion generated against the entity, and `doctor` stays green

**Severity: critical.** A green `doctor` over a project that does not compile —
the worst outcome in the tool. Found by the section-20 audit (fifty operations,
then one hard look) and bisected to five commands.

`g field` updates the scaffold's own ten surfaces correctly. It does not touch
the `query`, `transition` or `usecase` companions that construct the same record,
does not mention them, and nothing afterwards reports them.

```
jails new qg --package com.qg --offline && cd qg && jails add db
jails g scaffold Order id:uuid@pk total:decimal
jails g query FindOrders --on Order total:decimal
mvn -o test-compile                                    # exit 0 -- coherent
jails g field Order version:int --default-literal 0    # exit 0, silent
```

The operation list `g field` prints names zero companions. Then:

```
$ jails doctor
25 checks, all clear.

$ mvn -o test-compile
[ERROR] .../adapters/JdbcFindOrdersQuery.java:[51,16] constructor Order in record
        com.qg.domain.Order cannot be applied to given types;
```

**All three companion kinds are affected**, confirmed in one project by
generating them together and adding one nullable field:

| companion generated against `Order` | file broken by `g field Order memo:string?` |
|---|---|
| `g query FindOrders --on Order` | `JdbcFindOrdersQuery.java:[52]` |
| `g transition ShipOrder --on Order` | `JdbcShipOrderTransition.java:[58]` |
| `g usecase PlaceOrder --on Order` | `DefaultPlaceOrderUseCase.java:[24]` |

`doctor` reports `25 checks, all clear` in that state too, because every file on
disk is byte-identical to what jails wrote — the drift check is correct and
silent, exactly as **B5** describes, and this is the sharpest instance of it.
It is also the concrete form of the question **B14** says nobody asks: whether the
surfaces that name a record still agree with it.

Every intermediate command reported success. Only a compiler found it.

**Impact:** the ordinary "this entity needs one more column" edit — the single
most common change there is — silently breaks the build of any project that has
generated a query, transition or use case, and every jails oracle says the
project is healthy.

**Expected:** `g field` must either regenerate the companions that construct the
record (they are recorded output, so they are findable) or refuse and name them.
Failing both, `doctor` must compare a recorded entity's component list against
the companions that construct it.

---

## B40 — `jails rename` panics on any non-ASCII byte in any scanned `.java` file, including jails' own output

**Severity: high.** Exit 101, and the command is permanently unusable in the
project afterwards. `jails g idempotency` writes an em dash into its Javadoc, so
jails disables its own `rename` with its own output.

```
jails new p2 --package com.p2 --offline && cd p2 && jails add db
jails g idempotency Claim          # writes `—` into the generated Javadoc
jails g record Note body:string
jails rename Note Memo --force
```
```
thread 'main' (3287264) panicked at crates/jails-java/src/identifier.rs:234:14:
start byte index 537 is not a char boundary; it is inside '—' (bytes 536..539 of string)
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace
$ echo $?
101
```

**Control:** the identical project without the `g idempotency` step renames
cleanly — `2 file(s), 4 occurrence(s), 2 file rename(s).`, exit 0. The `record`
being renamed contains no non-ASCII byte itself; one such byte *anywhere in the
scanned tree* is enough.

A hand-written comment does it just as well, which is the case that will actually
be hit:

```
$ printf 'package com.p3;\n// café — note\nclass Junk {}\n' > src/main/java/com/p3/Junk.java
$ jails rename Memo Note2 --force
thread 'main' panicked at crates/jails-java/src/identifier.rs:234:14:
start byte index 23 is not a char boundary; it is inside 'é' (bytes 22..24 of string)
```

**Scoped:** only `rename` is affected. In the same project `routes`, `beans`,
`lint`, `stats`, `notes`, `doctor` and `g field` all exit 0, so this is byte
slicing in `identifier.rs` rather than anything shared by the source readers.
Nothing is written, so the damage is confined to the message and to `rename`
being unavailable — but it is unavailable *for every type in the project, for
good*, on any real codebase with an accented word in a comment.

**Impact:** the rename command is dead in any project containing one non-ASCII
byte, and jails' own generator puts one there.

**Expected:** slice on character boundaries. Exit 1 if there is genuinely
something to refuse; there is not — renaming is unaffected by a comment.

---

## B41 — a closed refusal loop: `migrate --check` says edit the migration, the seal says restore it, `resource repair` restores the broken one

**Severity: high.** Refusal chains that close a loop are the highest-value class
in this file, and this is a complete one, reachable from an ordinary mistake
(**B38**: entity `A` produces the reserved table name `as`).

```
jails new letter --package com.lt --offline && cd letter && jails add db
jails g scaffold A id:uuid@pk n:string        # exit 0
```

**Step 1 — `migrate --check` fails and tells you to edit the file.**

```
$ jails migrate --check
  FAIL  V001__create_as.sql
  psql:<stdin>:8: ERROR:  syntax error at or near "as"
fix: edit V001__create_as.sql and re-run `jails migrate --check`. Migrations are
     forward-only, so fix the file rather than adding one that undoes it --
     nothing has run anywhere yet.
```

**Step 2 — doing exactly that fixes the SQL and breaks every generator.**

```
$ sed -i 's/create table as (/create table "as" (/' src/main/resources/db/migration/V001__create_as.sql
$ jails migrate --check
  ok    V001__create_as.sql
  1 migration(s) applied cleanly to a scratch database.

$ jails g field A extra:string?                                    # exit 1
jails: migration-edited-after-seal: `src/main/resources/db/migration/V001__create_as.sql`
       differs from its sealed identity or bytes.
       fix: restore the exact recorded migration and append a later version
```

The second `fix:` is the exact inverse of the first. "Append a later version" does
not help: what is wrong is the `create table` statement itself, and the seal is on
that file.

**Step 3 — the repair command completes the circle.**

```
$ jails resource repair A --strategy roll-forward                  # exit 0
applied 44bb89dc...
  replace src/main/resources/db/migration/V001__create_as.sql
  ledger  replace

$ jails migrate --check
  FAIL  V001__create_as.sql
  psql:<stdin>:8: ERROR:  syntax error at or near "as"
```

`resource repair` silently overwrote the reader's correction with the broken
recorded bytes and returned the project to step 1. Nothing warned that the repair
would discard a hand edit, and nothing named it as a discard afterwards.

**Impact:** the documented recovery for a failing migration is a cycle with no
exit, and the repair verb destroys the user's fix without saying so.

**Expected:** `migrate --check`'s advice must distinguish a migration jails owns
and has sealed from one it does not. For a sealed one, editing is precisely what
the next command forbids, so the advice must not be "edit it" — and
`resource repair` must not silently overwrite a file whose bytes the reader
changed.

---

## B42 — `--output json` writes nothing at all on every failure

**Severity: medium.** The flag documents itself as *"One projection, two
encodings ... the same status, operation list, ledger line and effects"*. On the
success path that holds. On every failure path the JSON encoding produces **zero
bytes on stdout** and the refusal goes to stderr as prose only.

```
jails new orc --package com.orc --offline && cd orc && jails add db
jails g scaffold Order id:uuid@pk total:decimal
```
```
$ jails g record Note body:string --pretend --output json | head -c 60
{"schema":"jails.command-result.v2","command":{"path":["generate"    <- valid

$ jails destroy scaffold Order --pretend --output json 2>/dev/null | wc -c
0
$ jails destroy scaffold Order --pretend --output json 2>&1 >/dev/null
jails: storage-policy-required: `Order` is backed by table `orders`.
       fix: preserve it with `jails destroy scaffold Order --storage preserve`, ...
```

Not specific to that refusal — stdout is empty for all of them, and `json-v1` is
the same:

| failing run (`--output json`) | stdout bytes |
|---|---|
| `destroy scaffold Order --pretend` (storage policy) | 0 |
| `g field Nope x:int` (unknown entity) | 0 |
| `g record Z q:nosuchtype` (bad field type) | 0 |
| `destroy scaffold Order --pretend --output json-v1` | 0 |

The success document carries `"status"` and `"exit_code"` fields, so failures are
plainly meant to be representable in it; nothing emits one.

**Impact:** every consumer of the machine encoding — the editor protocol, CI, and
`jails.nvim`, which already reads `jails commands --json` — gets a parse error or
an empty document on exactly the runs it most needs to report, and has to fall
back to scraping English prose off stderr.

**Expected:** a failing run emits the same v2 document with its status and
non-zero `exit_code`, carrying the refusal and its `fix:` as fields.

---

## B43 — `jails add format` invalidates jails' own recorded output, and `doctor` then reports it as the developer's edits

**Severity: medium.** Three commands, all documented, all exit 0, and the project
ends up accusing the reader of edits they did not make.

```
jails new f1 --package com.f1 --offline && cd f1
jails add db
jails add api
jails doctor          # 25 checks: 0 failing, 1 warning(s)   <- the pre-existing one
jails add format      # exit 0
jails doctor
```
```
warn  capability api      recorded output `src/main/java/com/f1/api/ApiExceptionHandler.java` changed since the last jails commit
warn  capability db       recorded output `src/test/java/com/f1/TestcontainersConfig.java` changed since the last jails commit
warn  capability api      recorded output `src/test/java/com/f1/api/ApiExceptionHandlerTest.java` changed since the last jails commit
29 checks: 0 failing, 4 warning(s).
```

`add format` runs `spotless:apply` over the whole project (this is deliberate and
documented — formatter wrapping cannot be predicted from a template), reformats
three files jails itself wrote and recorded, and does not re-record the new bytes.

`jails sync` does repair it — `already formatted -- nothing to change.` /
`ledger replace`, after which `doctor` returns to its one pre-existing warning.
But the warnings carry no `fix:` line and never name `sync`, so the reader has
three unexplained accusations and no offered way out.

This also feeds **B18**: `resource repair --strategy roll-forward` adopts
whatever is on disk as the new base, so the state that ought to be re-recorded
and the state that ought to be rejected are indistinguishable to the repair verb.

**Impact:** the ordinary `add db; add api; add format` opening sequence leaves a
project that reports drift it does not have, on files the developer never opened.

**Expected:** `add format` re-records the bytes it rewrites in the same
transaction — it knows which files it touched and it already owns them.

---

## B44 — `jails explain --help` prints the completion command's description, and it reaches `commands --json`

**Severity: low.** Two adjacent clap doc comments have run together: `explain`
carries `completion`'s text in front of its own, and `completion` is left with
none.

```
$ jails explain --help
Print a shell-completion script: source <(jails completion bash) Explain what a
generator kind is for, and the trap it invites

$ jails completion --help
Usage: jails completion [OPTIONS] <SHELL>       <- no description at all
```

`jails --help` shows the same run-on text on the `explain` row, and it is not
confined to help output — it is what the machine surface serves:

```
$ jails commands --json | ...
explain    :: 'Print a shell-completion script: source <(jails completion bash) Explain what a generator kind is for, and the trap it invites'
completion :: None
```

`jails.nvim` builds its menus from exactly this payload, so the wrong sentence is
what an editor shows for `explain`.

**Impact:** cosmetic, but it is the first thing a reader sees about two commands,
and it ships to every consumer of `commands --json`.

**Expected:** `explain` describes `explain`; `completion` has a description.

---

## What worked well

Worth recording so the fixes don't regress it:

**Confirmed again on `3a023c0`** (section 1, 13 and 14 probes, all in fresh
projects):

- **The baseline stayed boring.** A ten-type scaffold (`uuid@pk`, required and
  optional text, `@unique`, int, decimal, date, instant, boolean) generated,
  applied to real PostgreSQL, compiled, and served four routes. `bytes` is
  refused cleanly with a fix and writes nothing.
- **`--pretend` matched the real run exactly**, operation for operation,
  including the `replace pom.xml` and `create .jails/architecture.toml` lines an
  earlier reading of this had wrongly recorded as missing.
- **Repeated commands are honest no-ops.** An identical `g scaffold` second run
  prints `nothing to do`; so does `app apply` over an unchanged manifest, three
  times running, with the file tree byte-identical afterwards.
- **Capability order is repaired exactly as `CLAUDE.md` claims.** `add api`
  before `add db` produces a different `ApiExceptionHandler` (no
  `DuplicateKeyException` arm) — `doctor` reports `1 failing`, `jails sync`
  restores it to byte-identical with the other order, and a second `sync` says
  `nothing to do`. This is the model the rest of the recovery surface should
  follow, and the only documented repair in this file that passed every part of
  the section-14 test.
- **Round trip is clean.** `generate` then `destroy` across `record`, `command`,
  `cli`, `controller`, `service`, `event`, `client` and `job`, each in its own
  fresh project, left nothing behind but empty layer directories.
- **"Not found" is consistent across oracles.** `resource repair`, `g field`,
  `destroy --pretend`, `resource field add` and `src` all exit 1 on a name that
  does not exist, with the same claim in different words.
- **`doctor` caught every hand edit in the fifty-operation project** — an
  appended comment, a touched class declaration and a deleted adapter, correctly
  split into two warnings and one FAIL, and `resource repair` restored the
  deleted file. What it missed was **B39**, which no file-level check can see.
- **The incomplete-invocation refusals are a genuine contract, revealed in
  order.** `g transition ShipOrder --on Order` asks for `id`, then for a numeric
  `version`, then for a field to update, each naming the next requirement; `g
  event` and `g usecase` do the same. Followed literally, all of them terminate
  in a successful command rather than a contradiction.


- `@pk`, `@unique` and the association FK all produced correct, idiomatic SQL.
- `g field` on a *clean* entity is genuinely good: it updated the record,
  request/response DTOs, JDBC adapter, three tests, the `.http` file and the
  JSON fixture, and appended a correctly-numbered migration.
- The `@unique` backfill refusal is the model for how the rest of this should
  feel — it explains the hazard *and* gives the three-step way through:
  `required unique text field 'barcode' has no safe automatic backfill. fix: add
  it as nullable first, backfill distinct values, then add not-null.`
- `destroy` on a never-renamed entity was exact: 20 files, a drop-table
  migration, nothing stranded.
- `migrate --check` against real Postgres validated all 8 migrations.
- **Hand-edits to generated files survive.** Adding code to `CustomerService`
  and then running `jails g field Customer nickname:string?` left the edit
  untouched. This is the property the whole workflow rests on and it holds.
- `record` -> `scaffold` *upgrade* writes the full slice correctly (it is only
  the ledger row it fails to replace).
- **Concurrency is safe.** Four `jails g record` commands run in parallel in one
  project: one applied, three refused with a stale-plan error, nothing was
  half-written and the ledger stayed readable. Optimistic concurrency is
  working.
- **`--pretend` is honest.** Its operation list diffed byte-identical against the
  real run of the same `g scaffold`, and re-running an identical command reports
  `nothing to do` rather than duplicating anything.
- **Capability add/remove round-trips cleanly** for `api`, `kafka`, `redis`,
  `security`, `observability` and `format` when nothing depends on them -- the new
  dependant check (B21/B23) does not over-refuse.
- **Ledger corruption is always detected, never silently accepted** — truncated,
  emptied and conflict-marked ledgers were all caught (the diagnosis and fix are
  wrong, B17, but the detection is right, which is the hard part).
- **Java reserved words are refused** as field names, by name, at parse time.
  SQL reserved words now are too (was B16), as are record-forbidden Object method
  names (`hashCode`, `toString`, `equals` -- was B32) and two Java names folding to
  one SQL column (`id`/`Id`, `userId`/`user_id` -- was B31). Field-name validation
  is now the best-finished part of the tool; the entity name gets none of it
  (B35, B36, B38).
- **Duplicate field names are refused** (`field 'name' is declared twice`), and
  `userName` + `username` correctly produce distinct `user_name` / `username`
  columns rather than colliding.
- A scaffold without `@pk`, or with two, is now refused before any file is written.
- `jails new` rejects whitespace in the project name before creating a directory.
- Generated `.http` files include item GET and DELETE.
- `jails explain association|transition|query|durable-job` ends with a working example.
- Person / Category / NewsItem pluralise to `people` / `categories` / `news_items`,
  and the HTTP routes match.
- Failed parse of `@pk`, unknown types, and SQL keywords writes nothing.
- Hyphenated (`my-app`), acronym (`AWS`), and one-letter (`x`) project names all create.
- The scaffold's `@pk` path, `--index` validation and the FK migration are all
  correct and idiomatic.
- **Enums round-trip correctly.** `g enum Status` then `status:Status` on a
  scaffold produced a `text` column, `.param("status", post.status().name())` on
  write and `Status.valueOf(rows.getString("status"))` on read.
- **`--timestamps` is fully wired**, not just a column pair: `created_at` and
  `updated_at` are `not null` in the DDL *and* populated by `Instant.now()` in
  `ArticleRequest.toDomain()`, so an insert cannot fail on them.
- **`--index "author, title"` produced a correct composite index** on the right
  table.
- **`app apply` is idempotent**, and adding or deleting a whole `[[generate]]`
  entity both work correctly.
- **`--pretend` is faithful.** Captured the operation list from
  `g usecase --pretend`, then ran it for real: byte-identical, 6 operations both
  times.
- **The composite kinds destroy cleanly.** `durable-job`, `query`, `transition`,
  two `usecase`s, `client` and `fetcher` generated together, then destroyed one
  by one: 39 files removed, nothing stranded, `mvn test-compile` still 0 errors.
  This is `tests/agreement.rs` earning its keep.
- **`new-cli` authoring is clean.** `g record`, `g command` and `g field` in a
  plain-Java project with no framework: compiles, dispatcher registration works.
- **Gradle works for the ordinary case.** Project detection, `g scaffold`,
  `g field`, `destroy` (13 files, no leftovers) and an idempotent dependency
  splice into a conventional multi-line `dependencies { }` block.
- **The composite kinds' preconditions are real and well-judged**, even where
  the messages arrive one at a time: `query` refusing an unfiltered read,
  `transition` requiring a `version`, and `usecase` refusing to invent a value
  it cannot infer are all correct calls.
- **Unowned migrations do not disrupt subsequent generators.** Adding a manual
  `V002__manual.sql` allows subsequent generators to increment versioning safely to `V003`.
- **Spaces in parent directories work cleanly.** Creating a project inside a path
  with spaces (`/tmp/jails-parent space-xxx/validapp`) compiles, adds capabilities,
  and scaffolds cleanly.
- **Transaction history commands (`jails history`, `jails show <id>`) are fully functional**,
  providing verifiable audit logs and before/after file operation images.

## The shape of it

*Rewritten at `0c369dd`; the previous version cited reports that are now removed.*

**The surfaces agree now, and that is a real change.** The whole B11/B11a/B13/B31
group is gone: `g field` updates all ten projections together, a second ledger row
for one name no longer splits an entity, colliding column names are refused, and
`doctor` grew a `managed <Entity>` check plus a working
`jails resource repair --strategy roll-forward`. So does the dependency group —
B21, B23 and B28 all refuse now, in the shape `add` already used.

What is left divides in three.

**1. The migration seal still has no "next free version" escape.** B1, B3, B12 and
B20 are one gap: a create migration cannot be superseded, so re-creating a
destroyed entity, changing a field on a manifest entity, and undoing a typo all
dead-end at the same refusal. `jails resource field` was clearly built to be the
way through it, which makes B33 the most valuable thing in this file: on every
entity that actually has a table, that command does not run at all.

**2. The lifecycle layer contradicts itself.** Three different commands now report
on the same entity and disagree: `resource status Customer` says `consistent`
while `resource field rename` says its source path is wrong (B33); `resource status
Memo` reports a resource that `resource repair Memo` says does not exist (B2);
`resource status Refund` finds an entity that `g field Refund` says is not
recorded (B25a); and B37's two refusals each name the other command as the fix.
Every one of these is a pair of commands reading the same store and answering
differently — cheap to detect, and corrosive out of proportion to the code behind
it, because the reader cannot tell which answer to believe.

**3. Validation and failure paths are still much less finished than the happy
path.** An entity name is checked for nothing: keywords generate an uncompilable
project (B35), invalid characters panic (B36), and `A` seals an unappliable
migration (B38) — while *field* names are checked carefully in three separate ways.
A publish that cannot complete still stops half-applied (B18), and the repair
command now records the tear as the new base, so the DB/code divergence this file
has chased since the first pass survives a green build *and* a green `doctor`
again — reached this time through the repair itself.

That end state is still the single most important thing to fix, and the check that
would catch every road to it is still missing: **does a recorded entity's field
list match the columns its migrations created?** `doctor` can now answer "are the
bytes the ones jails wrote"; it cannot answer that one, and B18, B34 and B5's new
reproduction all slip through the difference.

## Coverage

What was exercised, so the gaps are visible: `new`, `new-cli`, `new --app`, `new --gradle`, and hand-written Gradle/Maven projects; `add`/`remove`/`sync` for `db`; `scaffold`, `record`,
`enum`, `field`, `association`, `usecase`, `query`, `transition`, `durable-job`,
`client`, `fetcher`, `dto`, `command`; `destroy` for every one of those;
`rename`; `app plan`/`app apply`; `history`, `show`; `doctor`, `migrate --check`, `why`, `explain`,
`commands`; `--pretend`, `--diff`, `--ast`, `--output json`, `--package`, `--timestamps`, `--index`, `--storage`.
Real PostgreSQL and podman throughout, so every SQL claim was executed rather
than read.

Recheck `33abf9e` additionally covered: hyphenated / acronym / one-letter `jails new`, `new-cli` + `g command` dispatcher registration, Person/Category/NewsItem plurals (`people`/`categories`/`news-items`), `--pretend` matching apply, failed-parse writes nothing, `timestamp` alias, `g webhook`/`g idempotency`/`g query` incomplete invocations, `jails schema diff` (requires an app.toml; not usable on an imperative project), and `mvn test-compile` of `hashCode`/`toString` records.

Recheck `0c369dd` additionally covered: `jails resource status|repair|field
add|rename|type|nullability|drop`; `--pretend` output diffed against the real run
(identical); a repeated identical `g scaffold` (`nothing to do`); `add`/`remove`
round-trips for `api`, `kafka`, `redis`, `security`, `observability`, `format` and
the `loadtest` precondition refusal; `--package` before, after and repeated, empty
`--package`, `--index 'n desc'`, `--` termination; Java keyword and invalid-character
entity names compiled with `mvn -o test-compile`; reserved-word table names
executed against real PostgreSQL; a single-line Gradle `dependencies` block; and a
git-conflicted / truncated / empty ledger. `jails run` cold start (B10) remains the
one report not executed.

Not covered: the remaining generator kinds (`sealed`, `strategy`, `value`,
`repo`, `job`, `event`, `cli`, `handler`, `http-workflow`, `auth`, `migration`,
`cases`); `http-sink` past the first `--on` refusal; capabilities other than
`db` (`api`, `kafka`, `security`, `cache`, `observability`, `csv`, `json`,
`docker`, `ci`, `format`, and the rest); `jails adopt` on a foreign project;
`testd`/`--affected`; `jails run` cold start (B10); and running a generated
application end to end against a live database. The Gradle build was never
executed — Gradle is not installed here, so B27's consequence is inferred from
the file contents rather than observed.
