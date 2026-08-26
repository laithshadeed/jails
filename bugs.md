# bugs.md — dogfooding the edit/change-your-mind loop

Initial session: 2026-08-25. Binary: `jails 0.1.0` built from this checkout.

**A closed report is *deleted* from this file, not marked done.** `git log -p --
bugs.md` is where a closed one and the run that closed it live. Numbers are
stable and never reused, so a `bugs.md B33` citation still resolves to a
subject.

---

**Recheck: 2026-08-26 #4 (HEAD `e9ca5ca`, extended to `e3c7041`).** Rebuilt and
reinstalled `jails 0.1.0` from current HEAD (previous pass was `3a023c0`; 17
commits since, all in the lifecycle / field-evolution / Flyway-evidence area).
Direct reproductions in disposable projects under this session's scratch
directory, with real `mvn -o test-compile` wherever a claim needed a compiler.

Two further commits (`767b609`, `e3c7041`) landed from a concurrent session
while this pass was being written; the binary was rebuilt and **every surviving
report below was re-reproduced against `e3c7041`.** Two more closed as a result
and are noted in the list.

**Proved fixed and removed this pass — fifteen reports.** Each was reproduced
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

Closed by `e3c7041`, mid-pass:

- **B39** `g field` no longer refuses when generated companions exist — it
  regenerates them in the same transaction, and the project still compiles
  (`mvn -o test-compile`, 0 errors).
- **B43** `jails add format` re-records the bytes `spotless:apply` rewrites, so
  `doctor` goes from eight unexplained drift warnings to `26 checks, all clear`.

**Still broken, re-reproduced verbatim at `e3c7041`:** B5, B14, B18.

Closed after that pass, in the same session:

- **B45** `jails new --app` publishes the project and *then* reports a failed
  post-commit effect: exit 1, the effect named, and the project on disk. It
  used to throw the failure out of the publish-by-rename, discarding the whole
  scratch tree -- so a compose service that would not start left `ledger
  create` in the report and no directory at all.
- **B20** appending a component to a `[[generate]]` block keeps the sealed
  create migration and writes the *delta* as `alter table ... add column` --
  the same SQL and the same projection-aware version allocation `jails
  resource field add` produces. A required component (no backfill the manifest
  can carry) and any change that is not an append (a list diff cannot tell a
  rename from a drop-plus-add) are refused by name, pointing at the verbs that
  take the intent explicitly.
- **B22** deleting a table-backed `[[generate]]` block is refused with
  `storage-policy-required`, naming both retirement commands -- the same
  ceremony the imperative `destroy` insists on. The manifest still has no
  syntax for storage intent, so this does not invent one: it names the command
  that has it, after which the manifest and the store agree.
- **B41** the loop had one cause: `adopt_new_scaffolds` *skipped* an entity
  that already had a lifecycle, so regenerating a dropped scaffold left the
  state at `drop-pending` over a project whose create migration had just been
  appended. A re-declaration revives it instead, and all three oracles agree.
  The one surviving **B19** leak went with it -- "declare it in the read set"
  is an instruction to whoever is editing the route, and it now says it is a
  bug in jails, names the path, and offers the read-only commands that still
  work.
- **B2** `rename resource <Name> <New> --strategy preserve-table` takes a bare
  name -- demanding `<slice>.<current-name>` made the one path that carries the
  storage unreachable from every imperative project -- and keeps the resource's
  package, so `resource status` reports `consistent` and the next `g field`
  migrates `members` rather than a `readers` that was never created. The
  textual `jails rename` refuses a storage-backed resource by name and points
  at the strategy that fits.
- **B37** `destroy association` retires the row and appends `drop constraint`,
  which is the *next* migration rather than the un-running of one -- so the
  deadlock is gone in both directions and the whole lineage still applies
  cleanly to a scratch PostgreSQL.
- **B10** every generated compose service already declared a `healthcheck`;
  what was missing was `--wait`. `up -d --wait --wait-timeout 120` returns when
  PostgreSQL is *healthy* (`pg_isready`) rather than merely running, which is
  what `jails run` was racing.

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

Worth recording so the fixes don't regress it. **Re-confirmed on `e3c7041`**
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

*Rewritten at `e3c7041`. Fifteen reports were deleted this pass — the previous
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
each naming the other. Every one of these is a pair of commands reading the same
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

**This pass (`e9ca5ca` → `e3c7041`):** `new --offline`, `new --app`, `new-cli`; `add db`,
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
