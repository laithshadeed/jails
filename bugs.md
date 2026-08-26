# bugs.md — dogfooding the edit/change-your-mind loop

Initial session: 2026-08-25. Binary: `jails 0.1.0` built from this checkout.

**A closed report is *deleted* from this file, not marked done.** `git log -p --
bugs.md` is where a closed one and the run that closed it live. Numbers are
stable and never reused, so a `bugs.md B33` citation still resolves to a
subject.

---

**Recheck: 2026-08-26 #4 (HEAD `e9ca5ca`).** Rebuilt and reinstalled `jails
0.1.0` from current HEAD (previous pass was `3a023c0`; 17 commits since, all in
the lifecycle / field-evolution / Flyway-evidence area). Direct reproductions in
disposable projects under this session's scratch directory, with real `mvn -o
test-compile` wherever a claim needed a compiler.

**Proved fixed and removed this pass — thirteen reports.** Each was reproduced
verbatim from the text that was deleted, in a fresh project, and now behaves
correctly:

- **B1** the one-way door is closed. `destroy scaffold Book --storage drop
  --confirm-table books --force` then `g scaffold Book …` writes
  `V002__drop_books.sql` **and** `V003__create_books.sql`; the regenerated
  adapter queries a table the migration history creates.
- **B3 / B33 / B34** field evolution works on the case it is for. `resource
  field add|rename|type|drop` all run on a storage-backed scaffold and append a
  correctly numbered forward migration; on a plain `g record` the same commands
  no longer write `alter table` for a table that never existed.
- **B12** a typo is no longer permanent: `resource field rename Loan borower
  borrower --column single-cutover` commits, with
  `V002__rename_borower_to_borrower.sql`.
- **B17** ledger corruption is diagnosed without bricking the project.
- **B19** on its original three paths — no `plan.md §N` citation and no Rust
  `Debug` rendering reaches a user-facing string. *(One leak survives and is
  folded into B41 below; the report is deleted because its subject is now that
  one line, not a class of failures.)*
- **B25a** `--package com.example.app.domain` lands at
  `com/example/app/domain`, not doubled.
- **B35 / B36 / B38** entity names are validated as carefully as field names
  were. `class`, `Foo-Bar`, `A` and `I` are each refused by name with a fix
  line, before any file is written.
- **B40** `jails rename` no longer panics on a non-ASCII byte.
- **B42** `--output json` emits a full envelope on failure, carrying `status`,
  `exit_code`, `error` and `timings`.
- **B44** `jails explain --help` describes `explain`, and `completion` has a
  description.

**Still broken, reproduced verbatim this pass:** B2 (new face), B5, B14, B18,
B20, B22, B37, B41 (new face), B43. **Downgraded:** B39. **Not retested:** B10.

**New:** **B45** (high) `jails new --app` silently discards the entire project
when a post-commit effect fails.

**No jails source, test, build or doc file was modified while reproducing.**

---

## B18 — a failed write tears the transaction in half, and the repair verb then adopts the tear

**Severity: critical. This is the most important report in the file.** Triggered
by anything that makes one path unwritable mid-transaction — a root-owned
directory, a full disk, NFS, an IDE lock.

```
jails new b9 --offline --no-git && cd b9 && jails add db
jails g scaffold Order id:uuid@pk total:decimal
chmod 555 src/main/resources/db/migration
jails g field Order zzz:string?
```
```
jails: a file could not be read (could not publish .../V002__add_zzz_to_orders.sql:
       Permission denied (os error 13)).
       fix: make it readable and run the command again.
```

The command failed. The project did not roll back:

```
Order.java                 : 2 occurrences of zzz
JdbcOrderRepository.java   : 5 occurrences of zzz
migrations creating zzz    : 0
```

Then the advertised repair makes the tear the recorded truth:

```
$ jails doctor
30 checks: 0 failing, 5 warning(s)          # the five torn files, as "changed"

$ jails resource repair Order --strategy roll-forward
applied acb420e0…   ledger replace

$ jails doctor
25 checks, all clear.

$ grep -o 'insert into orders ([^)]*)' .../JdbcOrderRepository.java
insert into orders (id, total, zzz)
```

`doctor` is green, `mvn verify` is green, and every insert fails at runtime with
`column "zzz" does not exist`.

**Expected:** the write phase is already transactional — there is a journal and a
blocked-recovery state, so the machinery exists. A publish that cannot complete
must roll back or roll forward, never stop half-applied. And `resource repair`
must distinguish *bytes jails wrote and lost track of* from *bytes jails wrote
and should never have written*; today both are "adopt what is on disk".

---

## B2 — a rename leaves the schema lineage pointing at the old table, and both oracles stay green

**Severity: critical.** *The rename now commits, which is new; what it does not
do is carry the storage with it.*

```
jails new b1 --offline --no-git && cd b1 && jails add db
jails g scaffold Member id:uuid@pk name:string! email:string@unique
jails rename Member Reader --force        # exit 0, files and companions renamed
jails g field Reader nickname:string?     # exit 0
```

Afterwards:

```
migrations : V001__create_members.sql           <- creates table `members`
             V002__add_nickname_to_readers.sql  <- alters table `readers`
adapter    : select … from readers / insert into readers
status     : resource status Reader -> state: consistent
doctor     : 25 checks, all clear.
```

There is no `alter table members rename to readers`, and no create migration for
`readers`. Flyway will stop on V002 with `relation "readers" does not exist`. The
project cannot start, and the two commands whose job is to say so both report
health.

The ledger half of the old report survives in a new form: immediately after the
rename, `jails resource status Member` reports the *old* identity as
`state: source-diverged, declaration: absent` while `resource status Reader`
reports `consistent`. Two identities, one entity.

`jails rename resource <from> <to> --strategy preserve-table` — the command
built for this — is unreachable from an ordinary project: it wants a
`<slice>.<current-name>` selector (`fix: use <slice>.<current-name>, for example
Billing.Task`) and no imperative project has slices, so every spelling tried
returns `no managed resource matches`.

**Expected:** either the legacy rename refuses on a storage-backed entity, or it
plans the storage move. Committing the Java half alone produces exactly the
DB/code divergence this file exists to chase, and it is the only path that
produces it with no error at all.

---

## B41 — a closed refusal loop: doctor names repair, repair names revive, revive leaks an internal planning term

**Severity: high.** Three commands, three different answers about one entity, and
no exit. Reproduced on an entity that is entirely present on disk:

```
$ jails doctor
warn  managed Book   recorded output `…/V003__create_books.sql` changed since the last jails commit
                     fix: jails resource repair Book --strategy roll-forward

$ jails resource repair Book --strategy roll-forward
jails: resource `Book` is retired, so repair cannot recreate its projections.
       fix: use `jails resource revive Book --table <recorded-table>` first.

$ jails resource revive Book --table books
jails: `src/main/resources/db/migration/V003__create_books.sql` was not captured,
       so planning may not read it.
       fix: declare it in the read set. Reaching past the snapshot would decide on
       a fact nothing recorded, and the commit-time staleness check would have
       nothing to compare.
```

Three things are wrong at once. `Book` is not retired — it was re-created two
commands earlier and `V003` creates its table (`resource status Book` reports
`state: drop-pending` for it, which is the underlying error). "Declare it in the
read set" is not something a user can do; it is the last surviving instance of
**B19**, and it is on the recovery path, which is the worst place for it. And the
first `fix:` line names a command that refuses.

**Expected:** the three oracles agree, or the disagreement is the error message.
Any `fix:` line naming another jails command should be reachable — a cheap
integration test could run every `fix:` command this file's scenarios produce and
assert it does not immediately refuse.

---

## B5 / B14 — nothing checks that a recorded entity's fields match the columns its migrations created

**Severity: medium as a report, critical as the enabling gap.** *B5 and B14 are
merged: they were always one question.*

`doctor` answers **"are these the bytes jails wrote"** — a `managed <Entity>`
check reports a recorded output that is missing (`FAIL`) or edited since the last
commit (`warn`), and it catches a hand-deleted adapter and a hand-deleted create
migration. That is real and it works.

It does not answer **"is this project coherent"**. Three questions are still
unasked, and each one is the exact hole a report above escapes through:

- does a recorded entity's field list match the columns its migrations created?
  (**B18** and **B2** both end green because of this one)
- is there a `create table` in the migrations with no live entity claiming it?
  (**B22** produces exactly this)
- do the record, the request DTO, the JDBC insert and the fixture agree on the
  component list?

`capability_drift_checks` already does this shape of work for capabilities by
re-planning; entities have no equivalent.

The existing check also has a verified blind spot: **a migration written by
`jails resource field` is not recorded as managed output.**

```
$ rm src/main/resources/db/migration/V002__rename_borower_to_borrower.sql
$ jails doctor
34 checks: 0 failing, 8 warning(s)       # not one of them is the deleted file

$ rm src/main/resources/db/migration/V001__create_loans.sql
$ jails doctor
… 2 failing                              # the create migration is caught
```

`jails sync` is also still not the command its name promises: it prints
`applied … ledger replace` over a missing file and restores nothing.

---

## B20 — on a manifest project, adding a field to an existing entity is impossible

**Severity: high.** The declarative path has no field-evolution route at all.

```
.jails/app.toml:
  [[generate]]
  kind = "scaffold"
  name = "Deal"
  fields = ["id:uuid@pk", "amount:decimal"]
```

`jails app apply` — fine. Now add one field to that list and re-apply:

```
$ jails app apply
jails: migration-edited-after-seal: `src/main/resources/db/migration/V001__create_deals.sql`
       is published append-only schema history and cannot be replaced or deleted.
       fix: keep its recorded bytes and append the next migration for the desired
       schema change.
```

The seal is right. The fix is unusable: **the manifest has no syntax for
appending a migration**, and `jails resource field add` operates on an imperative
identity, so the manifest and the entity immediately disagree about the field
list on the next `app apply`. The one command whose entire purpose is "declare
the shape you want and let reconciliation work out the difference" cannot express
the most common shape change there is.

**Expected:** `app apply` routes an added/changed field in a `[[generate]]` block
to the same canonical field-evolution request `jails resource field add` builds,
and appends the forward migration itself.

---

## B22 — imperative and declarative removal disagree about data loss

**Severity: high.** The same intent, expressed two ways, gets two different levels
of care.

```
$ jails destroy scaffold Deal
jails: storage-policy-required: `Deal` is backed by table `deals`.
       fix: preserve it with `--storage preserve`, or plan data loss with
            `--storage drop --confirm-table deals`.
```

Delete the same entity's block from `.jails/app.toml` and re-apply:

```
$ jails app apply
  … 19 deletions …
  ledger  replace
```

No storage policy. No confirmation. No `drop table` migration —
`V001__create_deals.sql` is left in place, so the table survives with no code that
knows about it, and **B5/B14** means nothing reports the orphan.

The manifest has no syntax for expressing storage intent, so the ceremony the
imperative path insists on cannot even be written down in the declarative one.

---

## B37 — an entity in an association can never be destroyed, and the fix line names a command that refuses

**Severity: high.** A hard deadlock, reproduced in three commands.

```
jails g scaffold Author id:uuid@pk name:string!
jails g scaffold Book id:uuid@pk title:string! authorId:uuid
jails g association BookAuthor authorId=id --on Book --yields Author
```
```
$ jails destroy scaffold Book --storage drop --confirm-table books --force
jails: removing `scaffold Book` would leave `association BookAuthor` pointing at nothing.
       fix: remove the dependant first, or keep a declaration that owns `scaffold Book`.

$ jails destroy association BookAuthor --force
jails: migrations, associations, and field changes are forward-only; create a new
       migration instead of destroying one
```

`destroy scaffold Author` gives the identical first refusal. So neither half of an
association can be removed, the offered fix is refused by the command it names,
and "create a new migration instead" is not an escape from a dependency check.

**Expected:** either `destroy association` supports a forward retirement (drop the
constraint in a new migration, retire the row), or the dependant check names that
path instead of one that cannot run.

---

## B45 — `jails new --app` discards the entire project when a post-commit effect fails

**Severity: high.** *New this pass.* The flagship one-command path — CLAUDE.md
calls it "one command from an empty directory to a project that passes `mvn clean
verify`" — leaves nothing on disk when a compose service cannot start.

```
$ jails new b5app --offline --no-git --app b5/app.toml
  … 28 `create` lines …
  ledger  create
  effect  compose reconcile (1 up, 0 stopped) (failed)
$ echo $?
1
$ ls -d b5app
ls: cannot access 'b5app': No such file or directory
```

The report says the transaction committed (`ledger create`). There is no `jails:`
line, no message saying the project was not created, and no directory. The same
manifest with no `db` capability creates and keeps the project (exit 0), and
`jails add db` in an *existing* project survives the identical compose failure and
keeps its files — so the discard is specific to the `new --app` publish path.

The trigger here was an unrelated container already holding `:5432`, which is an
ordinary state on a developer machine.

**Expected:** a failed post-commit effect is reported against a project that
exists. The effect is explicitly post-*commit*; it must not be able to unmake the
commit.

---

## B43 — `jails add format` invalidates jails' own recorded output, and `doctor` reports it as the developer's edits

**Severity: medium.** Documented commands, all exit 0, and the project ends up
accusing the reader of edits they did not make.

```
jails new b2 --offline --no-git && cd b2 && jails add db
jails g scaffold Loan id:uuid@pk borower:string!
jails add format
jails doctor
```
```
warn  managed Loan   recorded output `…/adapters/JdbcLoanRepository.java` changed since the last jails commit
                     fix: jails resource repair Loan --strategy roll-forward
warn  managed Loan   recorded output `…/web/LoanController.java` changed since the last jails commit
… 8 warnings total
```

`add format` runs `spotless:apply` over the whole project (deliberate and
documented — formatter wrapping cannot be predicted from a template), reformats
files jails itself wrote and recorded, and does not re-record the new bytes.

The offered fix is now actively wrong: `resource repair --strategy roll-forward`
restores the *unformatted* recorded bytes, undoing the formatting the reader just
asked for. `jails sync` is what repairs it, and no warning names `sync`.

This feeds **B18**: the state that ought to be re-recorded and the state that
ought to be rejected are indistinguishable to the repair verb.

**Expected:** `add format` re-records the bytes it rewrites in the same
transaction — it knows which files it touched and it already owns them.

---

## B39 — `g field` refuses when companions exist, and the refusal does not name the way through

**Severity: low.** *Downgraded from critical: the silent corruption is fixed.*

```
jails g scaffold Order id:uuid@pk total:decimal version:long
jails g query OrdersByTotal total:decimal --on Order
jails g usecase PlaceOrder total:decimal --on Order
```
```
$ jails g field Order note:string?
jails: evolving fields on `Order` would leave generated companions stale:
       query OrdersByTotal, usecase PlaceOrder
       fix: keep the current field list, or regenerate those companions after the
       resource shape is stable.
```

The refusal is correct and the project is safe. But the fix is circular as
written — the companions cannot be regenerated *before* the field exists. The
working path is `destroy query OrdersByTotal` / `destroy usecase PlaceOrder`,
then `g field`, then regenerate; all four steps work. Nothing says so.

---

## B10 — `jails run` starts Spring Boot before PostgreSQL is ready for TCP connections

**Severity: medium (intermittent on cold start).** **Not retested** on any recent
pass — `:5432` is held by an unrelated project's container and restarting it
would disrupt that work.

The cause is unchanged and visible in the source: `compose.rs` builds
`["up", "-d"]` with no `--wait`, and there is no healthcheck or semantic
readiness probe anywhere on the start path. `docker compose up -d` returns when
the container is *running*, which is before PostgreSQL accepts connections.

---

## Design question, not a bug — the POST body requires the client to invent the id

`CustomerRequest` renders the `@pk` component as `@NotNull UUID id`, and
`customer.http` posts a hardcoded
`"id": "00000000-0000-0000-0000-000000000001"`. Nothing generates an identity
server-side. That is a defensible choice (it makes creates idempotent), but it is
unstated, and posting the sample body twice will violate the primary key. Worth a
line in `explain scaffold` either way.

---

## Not a bug, but it makes two commands unusable where they are most wanted

`jails migrate lint` and `jails schema diff` both require `.jails/app.toml`:

```
$ jails migrate lint
jails: failed to read application manifest …/.jails/app.toml: No such file or
       directory (os error 2)
```

An imperative project — the shape every reproduction in this file uses, and the
shape `jails new` produces — has no manifest, so neither command runs there. Both
questions ("is this migration destructive", "do my three schema authorities
agree") are answerable from the migrations and the ledger alone.

---

## What worked well

Worth recording so the fixes don't regress it. **Re-confirmed on `e9ca5ca`**
unless noted.

- **Entity naming is now as carefully validated as field naming.** `class`,
  `Foo-Bar`, `A` and `I` are each refused by name, before any write, with a fix.
  This was three separate reports and it is the cleanest thing fixed this pass.
- **The destroy/recreate cycle works and is honest about the schema.**
  `destroy --storage drop --confirm-table` writes `V002__drop_books.sql`;
  regenerating writes `V003__create_books.sql`. Both directions leave a readable
  history.
- **Field evolution earns its surface.** Add, rename (single-cutover), retype,
  nullability and drop all commit against a real table with correctly numbered
  forward migrations, and the guarded ones ask for exactly the evidence they
  need: `--confirm-column`, `--default-literal`, `--backfill-file`, and the
  `@unique` backfill refusal that explains the three-step way through.
- **`--pretend` is faithful.** Its operation list diffs byte-identical against the
  real run, and an identical second command prints `nothing to do`.
- **Hand-edits to generated files survive.** Adding code to a service and then
  running `g field` leaves the edit untouched. This is the property the whole
  workflow rests on and it holds.
- **`--package` is fixed and stayed fixed** across relative, fully-qualified,
  repeated and empty spellings.
- **`--output json` now has a failure envelope**, carrying `status`, `exit_code`,
  `error` and `timings`.
- **Reports state their own limits.** `jails routes` ends with
  `evidence: static-inference` and a `limitation:` line naming what it did not
  evaluate.
- **Capability order is repaired exactly as `CLAUDE.md` claims.** `add api`
  before `add db` produces a different `ApiExceptionHandler`; `doctor` reports it
  and `jails sync` restores it byte-identical to the other order.
- **Concurrency is safe.** Four parallel `g record` calls: one applied, three
  refused with a stale-plan error, nothing half-written.
- **Round trip is clean** across `record`, `command`, `cli`, `controller`,
  `service`, `event`, `client`, `job`, and the composite kinds.
- **`@pk`, `@unique`, `--index`, `--timestamps` and the association FK all
  produce correct, idiomatic SQL**, and enums round-trip through a `text` column
  with `valueOf` on read.
- **The incomplete-invocation refusals are a genuine contract, revealed in
  order.** `g transition` asks for `id`, then a numeric `version`, then a field to
  update, each naming the next requirement, and terminates in a successful
  command.

## The shape of it

*Rewritten at `e9ca5ca`. Thirteen reports were deleted this pass — the previous
version's first theme is gone entirely.*

**The migration seal has its escape hatch now.** B1, B3, B12 and B33 were one
gap — a create migration could not be superseded, so re-creating a destroyed
entity, changing a field and undoing a typo all dead-ended at the same refusal.
`jails resource field` and forward drop/create migrations closed all four. That
was the largest single theme in this file and it is closed.

What is left divides in two.

**1. One question is unasked, and everything else that ends badly ends there.**
*Does a recorded entity's field list match the columns its migrations created?*
`doctor` can answer "are these the bytes jails wrote" and cannot answer this.
**B18** reaches a green `doctor` over a project whose every insert fails; **B2**
reaches the same state through a rename that exits 0; **B22** leaves an orphan
table nothing claims. Three different roads, one missing check at the end of all
of them. It is also the check that would make the repair verb safe, because
"adopt what is on disk" is only wrong when what is on disk is incoherent.

**2. The recovery surface still disagrees with itself.** **B41** is the clean
case: `doctor` names `resource repair`, `repair` says the resource is retired and
names `revive`, `revive` answers with an internal planning term, and the entity in
question is fully present on disk. **B37** is the same shape with two commands
each naming the other. **B43** has `doctor` offering a fix that undoes what the
reader just asked for. Every one of these is a pair of commands reading the same
store and answering differently — cheap to detect, and corrosive out of
proportion to the code behind it, because the reader cannot tell which answer to
believe.

**And the declarative path is a tier behind the imperative one.** **B20** (a
field cannot be added to a manifest entity) and **B22** (a manifest deletion skips
the storage ceremony) are the same gap seen from both ends: `app apply` has no
route to the field-evolution and storage-policy machinery the CLI grew. The
manifest is the surface the proof applications and `new --app` are built on, so it
is not a side path.

## Coverage

What was exercised, so the gaps are visible.

**This pass (`e9ca5ca`):** `new --offline`, `new --app`, `new-cli`; `add db`,
`add format`, `sync`, `remove`; `g scaffold`, `record`, `field`, `query`,
`usecase`, `association`, `enum`; `destroy` for scaffold / query / usecase /
association, with `--storage preserve|drop` and `--confirm-table`;
`resource status|repair|revive`; `resource field add|rename|type|drop` with
`--column`, `--confirm-column`, `--default-literal`; `rename` (legacy) and
`rename resource`; `app plan` / `app apply` including entity add, field add and
entity deletion; `doctor`, `routes`, `migrate lint`, `explain`, `commands`;
`--pretend`, `--output json`, `--package`; entity-name validation swept across
Java keywords, invalid characters and one-letter names; a torn transaction via
`chmod 555`; `mvn -o test-compile` wherever a claim needed a compiler.

**Carried from earlier passes, not re-run:** Gradle project detection and
generation; real-PostgreSQL `migrate --check`; the remaining generator kinds
(`sealed`, `strategy`, `value`, `repo`, `job`, `event`, `cli`, `handler`,
`http-workflow`, `auth`, `migration`, `cases`, `transition`, `durable-job`,
`client`, `fetcher`, `dto`, `idempotency`); capabilities other than `db` and
`format`; ledger corruption (truncated / emptied / conflict-marked).

**Never covered:** `testd` and `--affected`; `test --engine warm`; `jails run`
cold start (**B10**); `sql check --live`, `introspect`, `pull`, `contract check`,
`editor`, `request`, `runner`, `logs`, `console`; a generated application run end
to end against a live database; any Gradle *build* (no `gradle` binary on this
machine).
