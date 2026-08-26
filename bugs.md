# bugs.md — dogfooding the edit/change-your-mind loop

Initial session: 2026-08-25. Binary: `jails 0.1.0` built from this checkout.

**Recheck: 2026-08-26 (Hostile-but-fair dogfooding pass).** Rebuilt `jails 0.1.0` from HEAD (`25cb1f6`) and executed the comprehensive 10-section test matrix across disposable test directories with real PostgreSQL, Podman, and Maven compiler verification.

- **Verified & Still Reproduced:**
  - **B1:** Recreating a destroyed entity is permanently wedged by migration seal collision.
  - **B2 (B2a, B2b, B2c):** `rename` renames domain type but not companion classes/tests and corrupts ledger tracking.
  - **B3 / B12:** Field rename, retype, and typo removal (`destroy field`) are rejected/unsupported on all paths.
  - **B4:** `g scaffold` without `@pk` silently selects the first column as identity.
  - **B5:** `jails doctor` reports 24–25 checks all clear on projects broken by stranded classes or compilation failures.
  - **B6:** Composite generator kinds (`association`, `transition`, `durable-job`, `query`) reveal requirements one refusal at a time without actionable syntax examples in `explain`.
  - **B7:** Advertised field types in `jails g --help` (`bigdecimal`, `zoneid`) mismatch parse rejection vocabulary (`decimal`, `zone-id`).
  - **B8:** `jails new` leaves `.jails-new.lock` in the parent directory.
  - **B9:** `jails migrate --check` outputs noisy compose container bind errors before reporting clean migration success.
  - **B10:** Cold startup race between Postgres socket readiness and Spring Boot in `jails run`.
  - **B11 / B11a:** `g record` then `g scaffold` creates duplicate ledger entries; subsequent `g field` updates only a subset of files and breaks compilation.
  - **B13 / B14:** Hand-deleted generated files are ignored by `doctor` and not restored by `jails sync`.
  - **B15:** Generated `.http` files only include collection `GET` and `POST`, omitting item `GET /{id}` and `DELETE /{id}`.
  - **B16:** SQL reserved words (`from`, `to`, `order`, `user`, etc.) are accepted as field names without escaping, causing syntax errors in DDL/DML.
  - **B17:** Ledger merge conflicts with git markers are misdiagnosed as written by another version and advise deleting `.jails/`.
  - **B18 / B19:** Mid-mutation failures (e.g. read-only migration directories) tear transactions in half, leak internal Rust `Debug` representations (`RecoveryBlocked(Unreadable { ... })`), and cite deleted design docs (`plan.md §R5.4`).
  - **B20:** Adding fields to existing entities in declarative `app.toml` manifests is blocked on both declarative and imperative paths.
  - **B21:** Destroying an enum referenced by another scaffold succeeds without dependency checks and leaves uncompilable code.
  - **B22:** Declarative entity deletion in `app.toml` bypasses storage policy confirmations and leaves orphaned tables.
  - **B23:** `remove db --force` unsplices Spring JDBC starter while leaving generated JDBC repository adapters, breaking compilation.
  - **B25 / B25a:** `--package` override artifacts are omitted from ledger tracking and fully-qualified package arguments are path-doubled.
  - **B26:** `g durable-job` constraints contradict each other when target use cases lack an explicit `id:uuid`.
  - **B27:** Single-line Gradle `dependencies { }` blocks cause dependency splices to land outside the block and duplicate repeatedly.
- **New Defects Discovered:**
  - **B28 (Critical):** `destroy scaffold` on a parent entity drops its table without checking incoming foreign key constraints, generating invalid drop migrations that fail `jails migrate --check` with dependency errors.
  - **B29 (High):** Scaffolding a composite primary key generates throwing stubs (`UnsupportedOperationException`) in the JDBC repository and 500 runtime HTTP endpoints while passing mocked unit tests.
  - **B30 (High):** `jails new` accepts whitespace in project names, generating an invalid `pom.xml` `artifactId` that Maven rejects on build.
- **Skipped / Untestable:**
  - Live Gradle build execution (Gradle binary is not installed on this host; syntax and file changes inspected directly).

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
jails g scaffold Book title:string! author:string isbn:string@unique pages:int
jails destroy scaffold Book --storage drop --confirm-table books --force   # succeeds
jails g scaffold Book title:string                                          # dead forever
```
```
jails: migration-edited-after-seal: `src/main/resources/db/migration/V001__create_books.sql`
       is published append-only schema history and cannot be replaced or deleted.
       fix: keep its recorded bytes and append the next migration for the desired schema change.
```

Confirmed it is the create-migration name and not something about `Book`: a
fresh `Member` in the same project correctly got `V004__create_members.sql`.
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

## B2 — `rename` renames the type but not its companions, and silently corrupts the ledger

**Severity: critical.** Ends in a project that does not compile, via commands
that all reported success.

`jails rename --help` promises "Rename a type and every reference to it (files,
companions, call sites)". Companions are not renamed.

```
jails g scaffold Member id:uuid@pk name:string! email:string@unique
jails rename Member Reader --force
```

Result — `Member.java` → `Reader.java`, and that is all. Left behind with their
old names but bodies now referring to `Reader`:

```
adapters/InMemoryMemberRepository.java   adapters/JdbcMemberRepository.java
app/MemberRepository.java                service/MemberService.java
web/MemberController.java                web/MemberRequest.java
web/MemberResponse.java                  adapters/JdbcMemberRepositoryIT.java
```

So the project now has `MemberService` whose job is `Reader`. It compiles, so
nothing tells you. Three follow-on failures:

**B2a — the entity becomes unaddressable by either name.**

```
$ jails g field Reader nickname:string?
jails: `.../domain/Reader.java` is jails' own output and its bytes differ from what
       this would write, but the store has not recorded the bytes jails wrote --
       so it cannot tell your edits from a regeneration.
       fix: destroy and regenerate, or keep the file.
       It was written before this jails recorded output bases.

$ jails g field Member nickname:string?
jails: no `Member` is recorded in this project.
```

The message is also factually wrong: `Reader.java` was written *by this binary,
seconds earlier, by `jails rename` itself*, not "before this jails recorded
output bases". The rename updated the ledger's name without updating the
recorded output bytes. And the offered fix — destroy and regenerate — is B1, the
one-way door.

**B2b — rename severs the entity from its table.** Before the rename,
`destroy scaffold Book` correctly refused without a storage policy. After the
rename, `destroy scaffold Reader` required **no** `--storage` flag and wrote
**no** drop-table migration. Table `members` and the FK
`loans_loan_member_fk` were left orphaned in the schema with no mention.

**B2c — rename drops files from the ledger, so destroy strands them and the
build breaks.** `rename` reported "10 file(s), 39 occurrence(s)" and never
touched `MemberServiceTest.java` or `MemberControllerTest.java`. `destroy
scaffold Reader` then didn't delete them either — they were no longer attributed
to the entity. Both survive, referencing deleted classes:

```
$ mvn -o test-compile
[ERROR] .../service/MemberServiceTest.java:[7,32] cannot find symbol
[ERROR] .../web/MemberControllerTest.java:[8,36] cannot find symbol
... COMPILE=1
```

This is exactly what `tests/agreement.rs` exists to prevent, and it does hold on
the un-renamed path (`destroy scaffold Book` deleted all 20 of its files
cleanly). The generate→destroy agreement is simply not preserved *across a
rename*.

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

## B4 — `g scaffold` with no `@pk` generates a REST resource keyed on its first column

**Severity: high.** Silent, and shaped like data loss.

`jails g scaffold Book title:string! author:string isbn:string@unique pages:int`
produced a table with **no primary key and no id column**:

```sql
create table books (
  title   text    not null,
  author  text    not null,
  isbn    text    not null unique,
  pages   integer not null
);
```

...and a repository that silently made `title` the identity:

```java
public Optional<Book> findById(String id) { ... where title = :id ... }
public boolean deleteById(String id)      { ... delete from books where title = :id ... }
```

So `DELETE /books/{id}` deletes **every book with that title**, and
`GET /books/{id}` returns an arbitrary one of them. `title` is not unique and
not declared unique. `isbn` *was* declared `@unique` and was not chosen.

The generated `BookController` even contains a comment noticing the problem —
"No `id` component, so there is no per-item URL to advertise in a Location
header" — and then serves `/{id}` routes anyway.

Adding `id:uuid@pk` produces the right thing, so the fix is known. The bug is
that omitting it is accepted silently rather than refused. A scaffold is a CRUD
resource by definition; one without an identity column should be an error
naming `@pk`, not a guess at the first field.

---

## B5 — `doctor` reports "18 checks, all clear" on a project that does not compile

**Severity: medium.** Run against the end state of B2 — orphaned `members`
table, orphaned FK, two stranded test files, `mvn test-compile` failing —
`jails doctor` reported every check green, including
`ok  beans  6 bean(s), every project-typed dependency resolvable`.

Doctor is read-only by contract and shouldn't compile anything, but it has the
ledger and the filesystem, and every one of these is answerable from those two:

- a recorded entity whose output bytes no longer match (B2a already detects this
  in `g field` — doctor doesn't ask)
- a `create table` in the migrations with no live entity claiming it
- an association whose parent resource has been destroyed
- ledger-recorded files that are missing, or entity-named files on disk that the
  ledger doesn't claim

Right now "all clear" means "your environment is fine", but it reads as "your
project is fine", which is where the reader actually needs help after an edit.

---

## B6 — the composite kinds reveal their requirements one refusal at a time

**Severity: low (friction), but it cost four attempts.**

```
$ jails g association Loan --on Member --yields Member
jails: association Loan needs at least one `childField=parentField` mapping
$ jails g association Loan --on Member memberId=id
jails: association Loan needs its parent resource.
       fix: pass `--yields <Parent>`.
$ jails g association LoanMember --on Loan --yields Member memberId=id
ok
```

`jails explain association` gives an excellent *rationale* and no syntax. `g
--help` is generic `generate` help with none of association's flags and no
example using `--on`/`--yields`. The non-obvious part — that NAME is the
association's own name, not the child entity — is nowhere. One worked example in
`explain` or in the first error would have covered it.

(Credit where due: once invoked correctly the FK migration was exactly right,
including declining to invent an `ON DELETE`.)

**This is a pattern, not one command.** `g durable-job` took *five* sequential
refusals to invoke — `--on`, then `--yields`, then `id:uuid`, then an exact field
list — and the fifth contradicted the third (B26). `g transition` took three:
`id`, then `version`, then "at least one field to update". Each message is
individually good; the problem is that none of them states the whole contract, so
a first-time invocation is a guessing game played one round trip at a time.

A `jails explain <kind>` that ended with one working invocation would fix all of
them at once, since the table already exists and already has an entry per kind.

---

## B7 — the advertised field-type list doesn't match the one in the error message

**Severity: low.**

`jails g --help` advertises: `bigdecimal`, `zoneid`, `datetime`.
The rejection message offers: `decimal`, `zone-id`, `datetime`.

Both spellings actually parse, so this is cosmetic — but the reader is reading
two different lists for one closed vocabulary and can't tell which is canonical.

Separately, `timestamp` is rejected. It is the most natural name for a
`timestamptz` column, and the column jails generates for `instant` is literally
`timestamptz`. Worth accepting as an alias.

---

## B8 — `jails new` leaves a `.jails-new.lock` in the parent directory on success

**Severity: low.**

```
$ jails new bookclub --package com.example.bookclub
Created ./bookclub (deps: web,devtools, Java 26)
$ ls
bookclub/  .jails-new.lock
```

22 bytes, in *my* directory, not the project's — so it lands in whatever the
reader's `~/code` is and is never cleaned up. Exit was 0; nothing failed.

---

## B9 — `jails migrate --check` prints a fatal-looking compose error, then succeeds

**Severity: low (noise).** With Postgres already running (`jails doctor` said
`ok service postgres running` moments earlier), `migrate --check` tried to start
it again:

```
Error response from daemon: rootlessport listen tcp 0.0.0.0:5432: bind: address already in use
Error: executing docker-compose up -d postgres: exit status 1
jails: docker compose up -d postgres exited with exit status: 1
  ok    V001__create_books.sql
  ...
8 migration(s) applied cleanly to a scratch database.
```

Three lines of `Error:` and a `jails:` prefix — the shape of a failure — above a
completely successful run. "Already up" isn't a failure and shouldn't be
reported as one.

---

## B10 — `jails run` starts Spring Boot before PostgreSQL is ready for TCP connections (startup race)

**Severity: medium (intermittent on cold start).**

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

## B11 — `g record X` then `g scaffold X` leaves two ledger rows; every later `g field` silently updates only half the entity

**Severity: critical.** Ends in the database and the Java disagreeing about the
schema, with a **green build** and a clean `doctor`.

The mistake is completely ordinary: you generate a `record`, realise you actually
wanted the whole CRUD slice, and re-run as `scaffold`. jails accepts the upgrade
and writes all 12 scaffold files — it looks like it worked.

```
jails g record   Order id:uuid@pk total:int      # oops, wanted a scaffold
jails g scaffold Order id:uuid@pk total:int      # accepted, writes the full slice
jails g field    Order note:string?              # <-- only touches 3 files
```

That third command reports:

```
  replace src/main/java/com/shop/domain/Order.java
  create  src/main/resources/db/migration/V006__add_note_to_orders.sql
  replace src/test/java/com/shop/domain/OrderTest.java
```

Three files. The same command on a clean scaffold (`Customer`) touches **ten** —
record, `Request`, `Response`, the JDBC adapter, three tests, the `.http` file
and the fixture. The `record` ledger row won, so the scaffold's half was never
updated. Measured drift immediately afterwards:

```
record Order   : UUID id, int total, Optional<String> note
OrderRequest   : @NotNull UUID id, @NotNull Integer total          <-- no note
jdbc insert    : (id, total)                                       <-- no note
sql columns    : id, total, add column note text                   <-- has note
http body      : id total                                          <-- no note
```

`mvn test-compile` fails: `constructor Order in record com.shop.domain.Order
cannot be applied to given types` in `JdbcOrderRepository` and `OrderRequest`.
Every subsequent `g field` widens the gap — `note2` behaved identically.

### B11a — the recovery silently reverts the Java and orphans the columns

There *is* a recovery, and it is entirely undiscoverable: `jails destroy record
Order` drops the stray row. It prints one line —

```
applied 27c032b689e2...
  ledger  replace
```

— and after it, `g field` correctly targets the scaffold again. But the scaffold
row's recorded field list was frozen at `(id, total)` and never learned about
`note` or `note2`. So the next regeneration rebuilds the Java from that stale
list. End state, verified:

```
record Order : UUID id, int total, Optional<String> note3    <-- note, note2 GONE
OrderRequest : @NotNull UUID id, @NotNull Integer total, String note3
jdbc insert  : (id, total, note3)
migrations   : V006 add column note   text;
               V007 add column note2  text;   <-- never dropped
               V008 add column note3  text;
mvn test-compile: 0 errors
```

**The build is green.** `jails doctor` is clean. And the database has two columns
— `note`, `note2` — that no Java code anywhere knows exist, with no migration
dropping them and nothing, anywhere, reporting it. A schema/code divergence that
survives a full green `mvn verify` is the worst failure mode in the tool.

Note also that the operation list is not a truthful account of the side effects:
`destroy record Order` reported only `ledger replace`, yet it changed which
declaration is authoritative for `Order` and therefore what the *next* command
writes to eight files.

**Expected:** re-running a different kind on an existing name should either
refuse (naming the existing row and the `destroy` that clears it) or *replace*
the row rather than adding a second one. In no case should two rows for one name
be reachable, and a regeneration must never drop a component that a shipped
migration has already added as a column.

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

## B13 — nothing detects or repairs a hand-deleted generated file

**Severity: medium.** Deleting a file you think you don't need is ordinary.

```
rm src/main/java/com/shop/adapters/InMemoryOrderRepository.java
jails doctor     # 18 checks, 0 failing — silent
jails sync       # "applied ... ledger replace" — restores nothing
jails g field Order note:string?   # proceeds as if nothing were missing
```

`sync` is the command whose whole job is "make the project match what is
recorded", and it reports `applied` while doing nothing about a
ledger-recorded file that is not on disk. Its output gives the reader no way to
tell "nothing needed doing" from "I did not look."

(The bean graph survives here — the JDBC adapter carries `@Component`, so
`doctor`'s `beans` check was correct to stay green. The loss is the in-memory
test fake.)

---

## B14 — `doctor` and `sync` have no notion of entity drift at all

**Severity: medium.** This is the umbrella over B11, B12 and B13.

Everything in this file that ends in a broken or silently-wrong project is
detectable from two things jails already has in hand: the ledger, and the files
on disk. None of it is checked. Concretely, `doctor` never asks:

- does any name have **two** ledger rows of different kinds? (B11)
- is a ledger-recorded file **missing** from disk? (B13)
- does a recorded entity's field list match the columns its migrations created?
  (B11a — this alone would have caught the two orphaned columns)
- is there a `create table` in the migrations with no live entity claiming it?
- do the record, the request DTO, the JDBC insert and the fixture agree on the
  component list?

`doctor` today answers "is your *environment* healthy" — Maven, JDK, Docker,
ports, containers. That is 18 of 18 checks. After an edit goes wrong, the
question the reader actually has is "is my *project* still coherent", and
nothing answers it. `capability_drift_checks` already does exactly this shape of
work for capabilities; entities have no equivalent.

---

## B15 — the generated `.http` file only exercises two of the four routes

**Severity: low.** `requests/customer.http` contains `POST /customers` and
`GET /customers`. The controller also serves `GET /customers/{id}` and
`DELETE /customers/{id}`, and neither appears. The file is the fastest way to
check a resource by hand, so the two routes keyed on the identity column — the
two B4 gets wrong — are the ones you cannot easily try.

---

## Design question, not a bug — the POST body requires the client to invent the id

`CustomerRequest` renders the `@pk` component as `@NotNull UUID id`, and
`customer.http` posts a hardcoded
`"id": "00000000-0000-0000-0000-000000000001"`. Nothing generates an identity
server-side. That is a defensible choice (it makes creates idempotent), but it
is unstated, and posting the sample body twice will violate the primary key.
Worth a line in `explain scaffold` either way.

---

## B16 — SQL reserved words are accepted as field names, producing invalid DDL *and* invalid DML

**Severity: critical.** jails already has a reserved-word guard. It checks Java
and not SQL.

```
$ jails g record X id:uuid@pk class:string
jails: name `class` is a Java reserved word          <-- correctly refused

$ jails g scaffold Booking id:uuid@pk from:date to:date guest:string
  ... 12 files written, no warning                    <-- accepted
```

The most natural way to model a date range is `from`/`to`. What it generates:

```sql
create table bookings (
  id     uuid not null,
  from   date not null,     -- syntax error
  to     date not null,     -- syntax error
  guest  text not null,
  constraint bookings_pk primary key (id)
);
```

`jails migrate --check` does catch the DDL:

```
FAIL  V001__create_bookings.sql
  psql:<stdin>:10: ERROR:  syntax error at or near "from"
```

**But nothing checks the DML, and that is the real damage.** The Java compiles
(`from` and `to` are legal Java identifiers), the application starts, and every
generated query is a syntax error:

```java
private static final String COLUMNS = "id, from, to, guest, nights";
    select id, from, to, guest, nights from bookings
    insert into bookings (id, from, to, guest, nights)
    values (:id, :from, :to, :guest, :nights)
```

Run verbatim against the project's own PostgreSQL:

```
$ psql -c 'select id, from, to, guest, nights from bookings'
ERROR:  syntax error at or near "from"
$ psql -c "insert into bookings (id, from, to, guest, nights) values (...)"
ERROR:  syntax error at or near "from"
```

So the reader hand-quotes the DDL (which is what `migrate --check`'s fix line
tells them to do, and it works), the migration goes green — and **every endpoint
on the resource 500s at runtime**, because the adapter is regenerated unquoted
on every subsequent `g field`.

### How big is the gap

Measured directly: for each word, does `jails g record` accept it as a field
name, and does PostgreSQL reject it as an unquoted column? **69 words are
accepted by jails and rejected by PostgreSQL**, including:

```
order  user   group  desc   end    check  limit  offset  references  primary
column table  select where  from   to     grant  union   all         any
when   then   having into   using  constraint    unique  array       asc
cast   collate  cross  distinct  except  foreign  full   ilike  in   inner
is     join   leading  left   like   natural  on   only  or   outer
right  similar  some   window  with   current_date  current_time  current_user
```

`order`, `user`, `group`, `desc`, `end`, `check`, `limit`, `left`, `right`,
`from`, `to`, `in`, `is`, `on`, `with` are all ordinary domain vocabulary.

**Expected:** the existing reserved-word check should have a SQL half, refusing
at parse time the way the Java half does — or `sql::Column` should quote
identifiers unconditionally, in both the DDL and the DML, from the one column
list it already shares.

*(Table names are safe by accident: pluralisation moves `Order` to `orders`,
`Value` to `values`, `Group` to `groups`, and PostgreSQL accepts all of those.
Verified — this is luck, not a guard.)*

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
jails: ... ledger has 13 line(s); schema 2 is exactly five, in a fixed order
       fix: it was written by a different version. ...

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
```

**Each path refuses and points at the other.** `app apply` says use a migration;
`g field` says make the declarations agree — and the only way to make them agree
is the edit `app apply` just rejected. There is no third path.

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

## B21 — `destroy` never checks whether anything still uses the type

**Severity: high.** Ordinary authoring: you generate an enum, wire it into a
record, later decide the enum was a bad idea.

```
jails g enum Status Draft Published Archived
jails g scaffold Post id:uuid@pk title:string status:Status   # Status is a component
jails destroy enum Status --force                              # deleted, no warning
```

```
  delete  src/main/java/com/blog/domain/Status.java
  delete  src/main/java/com/blog/domain/package-info.java
  delete  src/test/java/com/blog/domain/StatusTest.java
```

`mvn test-compile` → **14 errors**. jails had everything it needed to warn:
`Post`'s recorded field list literally contains `status:Status`, and it is in the
same ledger `destroy` just rewrote. No dependency check is performed.

**Expected:** refuse, naming the entities whose components reference the type —
the same shape as the `storage-policy-required` refusal, which does this well.

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

## B23 — `remove <capability>` yanks the dependency and leaves the code that needs it

**Severity: high.** `add` is explicit that a capability installing code without
its dependency is unacceptable — "a capability that installs the code and skips
the dependency is worse than one that refuses". `remove` has no symmetric check.

```
jails add db
jails g scaffold Article ...      # generates JdbcArticleRepository, uses JdbcClient
jails remove db --force           # succeeds
```

```
[ERROR] JdbcArticleRepository.java:[14,44] package org.springframework.jdbc.core.simple does not exist
[ERROR] JdbcArticleRepository.java:[45,19] cannot find symbol
... 26 errors
```

`remove db` unsplices `spring-boot-starter-jdbc` while every JDBC adapter it
generated stays on disk. `JdbcClient` lives in `spring-jdbc`, so the project
stops compiling immediately.

**Expected:** the same refusal `add` gives, inverted — name the generated files
that depend on this capability and require `--force` (or offer to destroy them),
rather than leaving a project that cannot build.

---

## B25 — `--package` writes the files but records nothing, orphaning the entity permanently

**Severity: critical.** `--package` is the documented placement override, and it
breaks the entire lifecycle of whatever it places.

```
$ jails g scaffold Refund id:uuid@pk amount:bigdecimal --package billing
  create src/main/java/com/ops/billing/Refund.java
  ... 11 files ...
  ledger replace                      <-- says it recorded something

$ jails g field Refund memo:string?
jails: no `Refund` is recorded in this project.

$ jails destroy scaffold Refund --force
jails: no `scaffold Refund` is recorded in this project.
```

Control, same command without `--package`: recorded correctly, `g field` works.
So every `--package` artifact is write-once — it cannot be evolved, cannot be
destroyed, and 11 files are stranded with no way to reach them through jails
again. The `ledger replace` line in the output is actively misleading.

### B25a — `--package` is relative, and a fully-qualified value is silently doubled

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

**Expected:** record the entity with its package, and reject (or normalise) a
`--package` value that already starts with the project's base package.

---

## B26 — `g durable-job` has two rules that contradict each other, making it unsatisfiable

**Severity: high.** Not a hard-to-reach edge: it is what you get by following
jails' own guidance one step earlier.

```
$ jails g usecase CloseTicket --on Ticket
jails: usecase CloseTicket cannot safely infer `subject` (String) for Ticket.
       fix: add `subject:<type>` to the usecase fields; ...
$ jails g usecase CloseTicket --on Ticket subject:string     # did as told
```

Now attach a durable job to it. Every possible field list is refused:

```
fields=[]                      -> needs a stable `id:uuid` field
fields=[subject:string]        -> needs a stable `id:uuid` field
fields=[id:uuid]               -> fields must exactly match CloseTicketCommand
                                  in declaration order. expected: subject:String
fields=[id:uuid subject:string]-> fields must exactly match CloseTicketCommand ...
fields=[subject:string id:uuid]-> fields must exactly match CloseTicketCommand ...
```

Rule A demands the fields exactly equal the command's (`subject:String`).
Rule B demands an `id:uuid` the command does not have. They cannot both hold, so
`g durable-job` can never be run against this use case.

**Root cause and the missing hint:** it works when the target use case itself
declares `id:uuid`:

```
$ jails g usecase OpenTicket --on Ticket id:uuid subject:string
$ jails g durable-job Nudge --on OpenTicket --yields Ticket id:uuid subject:string
  ... applied
```

So the constraint is really on `g usecase` — a use case without `id:uuid` can
never carry a durable job — and that is stated nowhere. The refusal that sent me
down this path (`add subject:<type> to the usecase fields`) listed exactly what
to add and omitted `id`, even though `Ticket` has `id:uuid@pk`.

**Expected:** the second refusal should name the real problem — "`CloseTicket`
has no `id:uuid`; regenerate the use case with one" — rather than demanding a
field list that the other rule forbids.

---

## B27 — a single-line Gradle `dependencies { }` block defeats the splice, which then duplicates on every run

**Severity: high** for Gradle projects, which is the case Gradle support was
added for.

With a conventional multi-line block, the splice is **correct and idempotent** —
two generates, no duplicates, both entries inside the braces. With the block
written on one line, jails does not find it, appends at the end of the file, and
loses idempotency:

```groovy
plugins { id 'java'; id 'org.springframework.boot' version '3.3.4' }
dependencies { implementation 'org.springframework.boot:spring-boot-starter-web' }
 testImplementation 'org.assertj:assertj-core'                      <-- outside the block
 implementation 'org.springframework.boot:spring-boot-starter-validation'
 testImplementation 'org.assertj:assertj-core'                      <-- duplicate
```

After a few ordinary commands: **`assertj-core` appears 4 times and
`spring-boot-starter-validation` twice**, all at the top level of the build
script, where `testImplementation` is not a callable method. `pom::add_dependency`
is idempotent; the Gradle path is not.

*(Gradle is not installed on this machine, so I could not execute the build. The
placement outside the `dependencies` block and the duplication are both verified
by reading the file; the resulting build failure is inferred, not observed.)*

The `// jails:integration-tests` marked block, by contrast, was written correctly
in both layouts.

**Expected:** the same treatment `pom.rs` gets — find the block whatever its
formatting, refuse rather than guess if it cannot be found (the bar `CLAUDE.md`
sets for `gradle.rs`: "answer exactly or refuse, never guess"), and stay
idempotent.

---

## B28 — `destroy scaffold` drops parent table without checking incoming foreign keys, emitting broken drop migrations

**Severity: critical.** Silently breaks migration execution for the entire repository.

When an association connects a child entity to a parent entity (`jails g association ChildParent --on Child --yields Parent parentId=id`), a foreign key constraint (`children_child_parent_fk`) is added to the database schema.

Destroying the parent entity with a storage drop policy succeeds without warning or error:

```bash
jails g scaffold Parent id:uuid@pk name:string
jails g scaffold Child id:uuid@pk parentId:uuid title:string
jails g association ChildParent --on Child --yields Parent parentId=id
jails destroy scaffold Parent --storage drop --confirm-table parents --force
```

Result: `destroy` reports success and writes `V004__drop_parents.sql`:

```sql
drop table parents;
```

When running `jails migrate --check` (or applying migrations in production):

```
FAIL  V004__drop_parents.sql

V004__drop_parents.sql did not apply:

  psql:<stdin>:1: ERROR:  cannot drop table parents because other objects depend on it
  DETAIL:  constraint children_child_parent_fk on table children depends on table parents
  HINT:  Use DROP ... CASCADE to drop the dependent objects too.
```

The project migration history is now broken and unappliable.

**Expected:** `destroy scaffold` should check for incoming foreign keys in the ledger, refusing to destroy a parent entity whose table is referenced by existing child associations (naming the dependent child entities and associations), or explicitly emit `drop table parents cascade;` / drop FK constraints first.

---

## B29 — Scaffolding a composite primary key produces throwing stubs (`UnsupportedOperationException`) and 500 runtime HTTP endpoints

**Severity: high.** Compiles cleanly, but endpoints crash with 500 at runtime and generated tests are disabled.

Declaring multiple `@pk` fields on a scaffold is accepted without error:

```bash
jails g scaffold OrgMember orgId:uuid@pk userId:uuid@pk role:string
```

What it generates:

1. Migration `V001__create_org_members.sql` correctly emits `primary key (org_id, user_id)`.
2. `OrgMemberRepository` interface generates single-string ID methods:
   ```java
   Optional<OrgMember> findById(String id);
   boolean deleteById(String id);
   ```
3. `JdbcOrgMemberRepository` implements them as throwing stubs:
   ```java
   @Override
   public Optional<OrgMember> findById(String id) {
       throw new UnsupportedOperationException("findById requires a composite-key repository port");
   }

   @Override
   public boolean deleteById(String id) {
       throw new UnsupportedOperationException("deleteById requires a composite-key repository port");
   }
   ```
4. `OrgMemberController` serves `GET /org-members/{id}` and `DELETE /org-members/{id}`. Calling either route crashes with HTTP 500 (`UnsupportedOperationException`).
5. `JdbcOrgMemberRepositoryIT` is generated as `@Disabled("todo: model the composite repository key in the port before enabling this round trip")`.

Because `OrgMemberControllerTest` uses Mockito mocks for `OrgMemberService`, all unit tests pass with `mvn test-compile` and `mvn test`, hiding the runtime 500 error.

**Expected:** If composite-key repository ports are not supported, `g scaffold` should reject multiple `@pk` declarations at parse time with an actionable error. If supported, it should generate composite repository ports (or record key classes) and working controller endpoints.

---

## B30 — `jails new` accepts whitespace in project names, generating an invalid `pom.xml` that Maven cannot compile

**Severity: high.** Creates a broken project on step 1.

Running `jails new` with a quoted name containing spaces:

```bash
jails new "my app with spaces" --package com.example.spaceapp --offline
```

`jails new` exits 0 and creates directory `./my app with spaces`. However, it writes the unescaped name directly into `pom.xml`:

```xml
<artifactId>my app with spaces</artifactId>
```

When building or compiling with Maven:

```
$ mvn -o test-compile
[ERROR] Some problems were encountered while processing the POMs:
[ERROR] 'artifactId' with value 'my app with spaces' does not match a valid id pattern. @ line 11, column 17
[ERROR] The build could not read 1 project -> [Help 1]
```

**Expected:** `jails new` should either validate the project name and reject whitespace with a clear error, or sanitize/normalize `artifactId` to valid kebab-case (`my-app-with-spaces`).

---

## What worked well

Worth recording so the fixes don't regress it:

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
  the ledger row it fails to replace, B11).
- **Concurrency is safe.** Four `jails g record` commands run in parallel in one
  project: one applied, three refused with a stale-plan error, nothing was
  half-written and the ledger stayed readable. Optimistic concurrency is
  working.
- **Ledger corruption is always detected, never silently accepted** — truncated,
  emptied and conflict-marked ledgers were all caught (the diagnosis and fix are
  wrong, B17, but the detection is right, which is the hard part).
- **Java reserved words are refused** as field names, by name, at parse time.
  This is exactly the guard B16 needs for SQL.
- **Duplicate field names are refused** (`field 'name' is declared twice`), and
  `userName` + `username` correctly produce distinct `user_name` / `username`
  columns rather than colliding.
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
  the messages arrive one at a time (B6): `query` refusing an unfiltered read,
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

B1, B2, B3 and B28 are one problem: **the ledger and the migration history disagree
about what identity and dependency mean.** The ledger keys an entity by name and lets `rename`
change it; the migration seal keys the schema by table name and treats it as
permanent. Any edit that crosses that line — rename, drop-and-recreate, retype, or dropping a parent table with FKs —
falls into the gap, and the gap has no floor. Fixing B1 alone (next free version
number for a re-created table) would restore destroy-and-regenerate as a working
escape hatch, which makes B3 survivable and B2 recoverable.

B11, B12, B13 and B29 add the second half: **nothing verifies that the surfaces still
agree.** jails writes multiple representations of one field and key structure — record, request
DTO, response DTO, JDBC insert, migration, fixture/`.http`, repository interface — and derives them
correctly on the happy path. But there is no check that they *still* match, so
the moment one command updates a subset (or generates throwing repository stubs for composite keys), the project keeps compiling and keeps
reporting healthy while the database and the code mean different things.

Two fixes cover most of this file:

1. **One row per name.** A second ledger row for an existing name is the root of
   B11, and refusing it (or replacing instead of appending) is a local change.
2. **A `doctor` entity-drift check**, built the way `capability_drift_checks`
   already is: re-derive each recorded entity's surfaces and report any that
   disagree, with `fix: jails sync`. That turns B11a, B12, B13 and B29 from silent
   into merely annoying — which, given B1 and B3 leave no way to undo, is the
   difference between a project you can finish and one you restart.

B16, B17, B18 and B30 are a third group: **input validation and failure paths are much less finished
than the happy path.** Each is one step from something jails already does well —
the reserved-word guard exists but only covers Java; ledger corruption is
detected but misdiagnosed; project creation accepts invalid artifact IDs; the write phase has a journal and a recovery state but
stops half-applied. And all four converge on the same end state as B11a: the
database and the Java disagreeing, with nothing reporting it.

That end state is worth naming as the single most important thing to fix. Multiple
different roads lead to it — a wrong-kind re-run (B11a), a reserved word (B16),
a permission error (B18), a composite PK stub (B29), and a hand-deleted file (B13) — and in every case the
build is green and `doctor` is clean. Whatever else changes, **something has to
be able to answer "do my migrations and my Java still agree?"**

The recovery story needs one thing that does not exist: a way to rebuild the
ledger from the files on disk. B1, B2a, B17 and B18 all currently dead-end at
`destroy and regenerate`, which B1 makes fatal. A `jails ledger repair` would
turn every one of them from unrecoverable into a bad afternoon.

The authoring round adds the sharpest version of the whole problem, B20: on a
manifest project **adding a field to an existing entity is impossible by either
path**, because `app apply` refuses on the migration seal and `g field` refuses
because the manifest disagrees. Each error tells you to use the other one. That
is the single most common authoring operation there is, and both doors are shut.

It also shows the same root cause reaching a fourth surface. B1, B3, B20 and the
`app apply` half of B20 are *all* the migration seal refusing to let a create
migration be superseded. The seal is right that published history is immutable;
what is missing is the next step — emitting a *new* migration at the next free
version for the delta. Everything downstream of that one gap is currently a dead
end.

And B21, B22, B23 and B28 are one more shared miss: **`destroy`/`remove` never ask who
else is using the thing.** An enum a record's component references, a table whose
create migration is still live, a capability whose dependency generated code
imports, or a parent table referenced by a child FK — in all cases jails holds the information needed to refuse and does
not consult it. `add` gets this right, and `storage-policy-required` is the model
refusal; they just are not applied on the way out.

## Coverage

What was exercised, so the gaps are visible: `new`, `new-cli`, `new --app`, `new --gradle`, and hand-written Gradle/Maven projects; `add`/`remove`/`sync` for `db`; `scaffold`, `record`,
`enum`, `field`, `association`, `usecase`, `query`, `transition`, `durable-job`,
`client`, `fetcher`, `dto`, `command`; `destroy` for every one of those;
`rename`; `app plan`/`app apply`; `history`, `show`; `doctor`, `migrate --check`, `why`, `explain`,
`commands`; `--pretend`, `--diff`, `--ast`, `--output json`, `--package`, `--timestamps`, `--index`, `--storage`.
Real PostgreSQL and podman throughout, so every SQL claim was executed rather
than read.

Not covered: the remaining generator kinds (`sealed`, `strategy`, `value`,
`repo`, `job`, `event`, `cli`, `handler`, `http-workflow`, `http-sink`,
`auth`, `idempotency`, `webhook`, `migration`, `cases`); capabilities other than
`db` (`api`, `kafka`, `security`, `cache`, `observability`, `csv`, `json`,
`docker`, `ci`, `format`, and the rest); `jails adopt` on a foreign project;
`testd`/`--affected`; and running a generated application end to end against a
live database. The Gradle build was never executed — Gradle is not installed
here, so B27's consequence is inferred from the file contents rather than
observed.
