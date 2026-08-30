# Simplifying `jails`: make the hidden compiler explicit

---

## Where this stands (2026-08-30)

**The compiler is built, every advertised generator runs on it, and every gate
this document set is green. The legacy path is still on disk because deleting
it is the cutover -- one deliberate commit, on a machine whose toolchain can
run the suite that would have to prove it.**

| | |
|---|---|
| generators on the canonical path | **39 of 39**, held by `canonical_support::registry_classifies_every_advertised_word` |
| capabilities | **25 of 25** |
| component kinds with a backend | **23 of 23** -- `every_component_kind_is_emitted_or_refused` has no refusal left to reach |
| architecture fitness rules | all thirteen held: nine by a test, four by a type or by Cargo. One added that the list did not have. See *Where each fitness rule stands* |
| merge gates | **all six green**, and G0 reliably so -- two causes of a one-in-three false red are fixed. See *What a run of G0 actually does* |
| deletion map | **not started.** Its first step is the `new` generation paths row, measured -- see *What `new` has to become first* |

**Why the deletion has not happened.** The *Integration and one coordinated
cutover* section makes it step 5 through 7, after "all gates pass". Both things
this document listed as blocking it are closed:

- **G1 survives the deletion, and this document said how.** Step 6 keeps "the
  frozen old binary fixture needed by compatibility tests", and
  `scripts/verify-rewrite-g1-canary.sh` is it: the legacy side is built from a
  frozen git revision, and the script *refuses* when that revision resolves to
  HEAD, because comparing the binary under test with itself passes every
  assertion and means nothing. So the differential gate keeps working after
  the legacy crates are gone. This was recorded here as a blocker on
  2026-08-30 and it was wrong; checking beat asserting.
- **G5 has its corpus.** Five checked-in projects jails did not write, covering
  all three of `adopt`'s rules and both build systems, each run through both
  binaries with `policy.tsv` accounting for every one. Two of the five found
  something the moment they were written -- an `adopt` defect, and a synonym
  nobody should add.

What is left is not a *condition*; it is the cutover itself, which is a change
of a different kind from everything above it. Nineteen crates become four plus
two leaves, `LAYERS` loses two thirds of its rows, and every test that drives
the legacy path either moves to the canonical one or goes. That is one
deliberate commit on a machine with the full toolchain, not a step to slip into
a session that was doing something else.

The one remaining red mark is **the machine this was written from, not the
branch**: JDK 21 against `TARGET_RELEASE` 26, so 21 tier-3 tests cannot compile
what they generate. It is recorded in `CLAUDE.md`.

**And "this machine" is the sentence to be careful with, because there is more
than one.** Written from the container, that paragraph read as a property of
the work; it is a property of the box. Measured on the developer machine on
2026-08-30: **git 2.55.0** (the `merge-file` floor is 2.44), **JDK 26.0.2**
matching `TARGET_RELEASE`, and a working container engine. The whole gate runs
there, tier 3 included, and came back green -- 33 binaries, 139s warm, 259s
cold.

So the toolchain condition the cutover was waiting on is met; it was never
waiting on the branch. What is left is the size of the change, not a
prerequisite.

There used to be a second red mark, and it was **not** the machine: jails
passed `--diff-algorithm` to `git merge-file`, a flag git 2.43 does not have,
so 58 tests died on a usage error. It is probed now, with
`JAILS_GIT_DIFF_ALGORITHM` to pin one answer across machines, and closing it
un-hid 29 tests that had never run here -- six of which were failing for real.
A gate that cannot run is worth less than no gate, because it reports the same
green.

One thing went ahead of it. G0 is what step 5 means by "all gates pass", and
measured over seven runs it failed two in six, from two causes -- a fixture
corpus that filled `/tmp`, and a lock settle window bounded by the wrong
quantity. Both are fixed and both are in *What a run of G0 actually does*.
Deleting most of the workspace behind a gate that is green two times in three
is how a real regression gets re-run until it passes.

What is left is the cutover, and its first step is `new` -- not a deletion.
Seeding `.jails/model.jdl` from `new` was tried and refused by two canonical
adapters, because `new` hand-writes the pom and properties the model expects to
own. See *What `new` has to become first*.

---

## Maintainer decisions (2026-08-27)

**Added by the maintainer. These are decided requirements, not analysis. Where
they conflict with a recommendation below, they win, and the affected section
says so.**

### D1 — The iterative edit loop is the product. CONFIRMED.

Already reflected in "The product choice": option **B, merge-managed compiler
with implementation-boundary ejection**, is chosen and option C is rejected. Recording the
acceptance test so it cannot be traded away later by an implementation
convenience:

```
jails g record Task title:string!
# hand-add a method to the generated Task.java
jails g field Task done:boolean      -> my method survives, component added
# edit a validation message jails itself wrote
jails g field Task priority:int      -> my wording survives, component added
# hand-edit the exact line jails rewrites
jails g field Task dueAt:instant     -> clean refusal, nothing written
```

All four behaviours exist in the current binary and were verified against it.
No phase of the rewrite may regress them. "Eject first, then edit" is not an
acceptable substitute: ejection is for handing an artifact over permanently,
not for making an ordinary edit.

Two consequences already recorded above and worth keeping visible: **BASE is
the one exact accepted compiler projection, not a history/object/journal
system**, and **ejection is scoped by an implementation-boundary ID, not an
entity ID** — so ejecting one implementation never drags its ports and record
out with it. File artifact IDs remain unique merge identities. A separate
ejection ID may intentionally group several files, such as a controller and
its test, into the one implementation boundary a reader takes over. The
accepted model alone cannot reproduce BASE across an emitter upgrade; the
compiler lock therefore carries that single irreducible projection and its
digest.

### D2 — JDL is a required deliverable. This overrides "A new JDL" below.

The maintainer wants a real authoring grammar. The section
"What will not simplify Jails by itself → A new JDL" is **overridden as to the
conclusion**; its reasoning about *sequencing* is kept.

The normative language contract is [JDL v1 — implementation
specification](jdl-sol.md). That document owns vocabulary, grammar, static
semantics, conventions, CLI-to-source mappings, evolution rules, diagnostics,
and conformance examples. This document owns the compiler architecture and
implementation sequence. If an illustrative JDL spelling here conflicts with
`jdl-sol.md`, `jdl-sol.md` wins.

What is decided:

- **JDL is the human authoring syntax and the file git tracks.** One authoring
  format, not two. A machine-readable projection (`jails model show --json`)
  is an export, never a second editable source — two editable sources is the
  exact disease this rewrite exists to cure.
- **JDL is a front end, not the model.** It parses to the same `AppModel` as
  the CLI does. `AppModel` and `ModelPatch` are still defined first; the
  grammar targets a settled semantics rather than guessing at one.
- **The CLI stays.** `jails g scaffold Task ...` edits the JDL file, the way
  `cargo add` edits `Cargo.toml`. It is not a competing authority.

Sequencing, unchanged from the reasoning below: model and patch types first,
TOML compatibility front end to get the compiler working, then the JDL front
end once the semantics stop moving. This is implementation order, not a choice
between durable source formats. The canonical source shape and complete
executable example are specified by the [JDL v1 decision](jdl-sol.md#1-decision)
and [complete example](jdl-sol.md#4-complete-example).

**Stable-ID decision: identity is inline in JDL.** Nodes use unobtrusive
`@id(...)` annotations, minted by CLI mutations when identity must be
materialized. Preserve-table and other rename behavior follows the
[stable-identity rules](jdl-sol.md#8-stable-identity); this document does not
define a second rename annotation.

A sidecar was rejected because it would create a second synchronization
surface beside the one reviewed authoring file. Inline identity also makes a
rename reviewable in an ordinary diff. The linker derives deterministic IDs
when they are absent; a CLI rename materializes the old effective ID before
changing the name. Silent identity changes are not acceptable.

### D3 — Ergonomics are a requirement, not a preference.

The concise, nested authoring shape is *why* D2 is wanted. Two specific rules:

- **Keep the compact leaf syntax.** Canonical JDL keeps a field on one line,
  for example `title: string @notBlank @index`; the exact closed attribute
  vocabulary belongs to [the field
  specification](jdl-sol.md#94-field-attributes). `FieldSpec::parse` may remain
  a CLI or temporary TOML compatibility adapter, but it must lower to canonical
  JDL rather than define a second source grammar.
- **Nesting belongs to the thing it describes.** A transition's selected
  fields, guards, state changes, and emitted events read as one construct, not
  sibling keys in a flat table. This is the case TOML genuinely loses, and it
  is the strongest technical argument for D2.

Measure it: the current expanded form costs roughly 25 lines for a five-field
entity where today's CLI takes one line. That regression is the thing to fix,
by either route.

---

## Executive verdict

The architecture is not nonsense. It is a serious attempt to make an unusually
ambitious promise safe: generate into an existing project, let the reader edit
the result, remember who owns every fragment, preview later changes, undo them,
recover after a crash, and keep several declarations of the application in
agreement.

The problem is that this promise has made **generated source both an output and
a database**. Jails emits Java and SQL, reparses those files to recover facts it
previously knew, combines those facts with a manifest, a ledger, migrations,
configuration, receipts and live database evidence, and then tries to decide
which authority wins. Every new generator multiplies that work across CLI
validation, naming, rendering, dependency inference, ownership, evolution,
destroy, recovery and reporting.

Jails is already a compiler. Its current intermediate representation is a bag
of optional recipe fields plus whatever strings happen to have been rendered.
The clean solution is not primarily another crate split, a larger template
engine, or a new syntax. It is to give the existing compiler:

1. one versioned semantic application model;
2. typed declarations and typed compiler IR;
3. explicit resolve, validate, lower, diff and emit passes;
4. one immutable `Plan` that preview and apply share;
5. a hard boundary between managed output and reader-owned source;
6. a much smaller transaction kernel that protects only irreproducible state.

The product decision is now explicit: **generate, hand-edit, generate again is
the product**, including edits inside records and every other generated file.
That requires a three-way merge, but it does not require the legacy object
store, codec, journal, roll-forward protocol, or entity-granular ownership.
The compiler lock supplies the exact accepted projection as BASE, workspace
capture supplies OURS, and the next model renders THEIRS. A clean merge becomes
an exact plan after-image; an overlap refuses before any write.

The recommended destination is therefore a **merge-managed application
compiler with implementation-boundary-scoped ejection**. Ejection is for transferring a whole
replaceable implementation boundary, not for making an ordinary hand edit.
Records and ports remain managed ABI; adapter implementations can be ejected
independently. Keep the familiar CLI as a front end. JDL is now the chosen
human source syntax and lowers into the already-proved model and IR; it is not
a parallel semantic system.

The implementation checkpoint now proves the risky part rather than merely
describing it. Canonical record and enum ABIs are merge-managed, a Spring enum
converter is a separate managed artifact, and both TOML compatibility input
and `.jails/model.jdl` pass the exact generate/edit/generate sequence: disjoint
methods and wording survive while overlaps refuse before any write. Unique
artifact IDs scope merge history; implementation-boundary IDs scope ejection.
JDL currently supports records, value-object record
profiles, scaffolds, fields, enums, plain classes, interfaces, services,
sealed types with ordered variants and exhaustive companion tests, standalone
tests, integration tests, companion tests, entity-derived test factories,
repository ABI facets, typed open-set strategies, typed HTTP controllers,
compiler-owned `fake`/`db`/`api` backends, declarative
`csv`/`json`/`http`/`testkit`/`sqlite`/`h2`/`actuator`/`cache`/`cors`/`observability`/`security`/`sse`/`redis`/`kafka`/`mail`/`toxiproxy`/`coverage`/`loadtest` projections, merge-managed generated resources, and reader-document facets,
plus preserve-table entity rename and nested
command/query/transition/event declarations. Familiar generators edit the JDL
losslessly and mint inline IDs.

The compiler now has one generic whole-project-file facet for outputs outside
`.jails/generated`. It stores the generated file itself as BASE and uses the
same three-way merge/refusal kernel as Java. Loadtest is the first consumer:
all six files remain in `load-tests/`, typed model routes replace source
rescanning, disjoint edits survive, and overlap or edited removal refuses
before any write.

The normative `jdl 1` implementation has also started as an independent,
version-gated frontend rather than another string-to-TOML adapter. Its lexer
and CST retain every byte and declaration span, the parser calls the typed
linker directly, local CST replacements preserve unrelated source, and the
first formatter layer is idempotent. The executable core currently covers app,
cap, dep, prop, enum, entity/projection/field/constraint, and eject forms. The
remaining operation/relation/component registries and richer linked nodes are
still mandatory before the complete example or cutover can be called done.

The mandatory G0 gate is executable as `mise run verify-rewrite` and is green:
format, strict workspace clippy, build, the full Rust suite, 410 ordinary CLI
E2Es, and the separately pinned Gradle/JDK real-project test all passed. This is
the safety floor, not a cutover claim. Since that checkpoint, JDL has gained
semantic string-length bounds, canonically written as `@length(1..200)`,
dependencies, settings, indexes, artifact ejections, destroy/retire/revive,
field evolution, and direct one-way import. Bounds lower to both Java
validation and SQL checks;
they are not parser-only metadata. Defaults still need an explicit semantic
decision (constructor default, storage default, or operation-input default)
before the grammar accepts them. Remaining work is primarily generator and
capability backend parity, richer transition semantics and multi-release schema
campaigns, followed by G1--G5 and legacy deletion.
Ordinary new projects therefore stay on the compatibility engine; partial
parity is not called a cutover.

The first G1 canary is executable as `mise run verify-rewrite-g1-canary`. It
builds `JAILS_LEGACY_REVISION` (the branch point against `main` by default) in
an isolated temporary tree and runs the exact record generate/edit/generate
loop through that frozen binary and the canonical JDL path. It compares the
reader-visible safety contract: disjoint method and message edits survive,
the next field appears, an overlapping generated-line edit refuses, and the
refusal changes no byte or executable bit. A second canary proves that an
identical generation rerun changes no reader-visible state and that destroy
removes the artifact on both sides. Private object/receipt bytes are normalized
out of the rerun comparison, while the stronger all-byte comparison remains on
the refusal path. A third canary covers identical operation reruns and
operation destroy, plus value-object rerun and destroy. A fifth scenario
hand-edits every Java artifact emitted by a scaffold and proves a later field
evolution preserves every edit while updating the record. A sixth does the
same for enum artifacts, proves an identical rerun is safe, and checks clean
explicit destroy. A seventh migrates `class`, `interface`, `service`, `test`,
and `integration-test` onto one typed source-unit node, hand-edits every
emitted main/test Java file, and proves later generation preserves all edits
against the frozen legacy binary. The integration-test declaration also
lowers to one build-tool-neutral feature: Maven renders/removes an exact
Failsafe block and Gradle renders/removes separate unit/integration task
wiring. Editing either the Java line or the marked build block that the
compiler must change refuses the entire plan before a write. Maven uses
distinct `add-source` and `add-test-source` executions; Gradle uses distinct
`main` and `test` source-set blocks. Stable artifact identity also
carries reader edits across a package/path move. An eighth scenario evolves a
sealed type's ordered variants as one semantic unit while separately
merge-managing its ABI and exhaustive test. Disjoint edits in both files
survive, generated switch/variant overlap refuses every write, identical reruns
are byte-stable, the generated project compiles, and destroy removes both
artifacts. A ninth scenario exposes an architectural bug rather than
normalizing it: the frozen legacy engine records a factory as an independent
recipe, so later `g field` replays that recipe with field arguments and refuses
atomically. The canonical compiler models factory as an entity facet, evolves
its record and testkit builder together, preserves a reader-added method, and
keeps identical factory reruns byte-stable. Its artifact ID is independently
ejectable, so the factory can become reader-owned while the record ABI keeps
evolving under the compiler. A tenth scenario preserves the legacy repository
loop while replacing its independent recipe with an entity facet: reader edits
to the repository port survive field evolution and identical reruns, and
destroy removes the port without removing the record ABI. The repository port
is non-ejectable ABI; fake and database implementations keep their own
capability-scoped ejection IDs. A new DTO scenario moves the three-file wire
contract onto an entity facet: request, response, and contract test each keep
an independent merge artifact ID; all three preserve reader edits across
field evolution; an overlapping component edit refuses the whole plan before
writes; real Maven compiles and runs the result; and destroy removes only the
DTO projections. The wire contracts remain managed ABI rather than ejectable
implementations. The legacy/canonical canary separately proves identical DTO
reruns preserve edits and clean destroy is symmetric without treating the
legacy recipe journal as architecture. An eleventh scenario evolves a strategy's
ordered variants and preserves edits across its port, evaluator,
implementations, and tests. It proves identical reruns and symmetric destroy
against the frozen legacy binary; the port ABI is non-ejectable while every
implementation boundary has its own artifact ID. A twelfth scenario covers the
typed controller unit: both controller and test preserve reader edits through
later generation and identical reruns, and destroy removes both against the
frozen legacy binary. Canonical controller evolution additionally proves typed
method/path/body changes, atomic overlap refusal, real Maven compilation, and
one shared ejection boundary spanning the two independently merge-managed file
artifacts. The all-scenario G1 gate is now green with 31 tests against the
frozen pre-cutover binary. The additional scenarios cover CSV/JSON data packs,
HTTP/Fake test packs, Testkit's five Java files plus fixture resource, and
SQLite's three-file Java implementation plus append-only migration history.
They prove edit-preserving regeneration and symmetric implementation removal;
SQLite removal deliberately retains the reader-edited migration because schema
history is neither a replaceable implementation facet nor an ejection target.
The H2 scenario adds a Spring-specific pack with a merge-managed database test,
Boot-version-aware dependencies, and main/test property sets; unrelated reader
properties survive both regeneration and capability removal.
The Actuator scenario adds the same iterative loop around its endpoint contract
test and key-scoped management properties. Its canonical-only E2E then ejects
the Java test without surrendering dependency or property ownership. Cache
extends the same proof to a two-file implementation boundary and bounded
Caffeine configuration. CORS adds a second two-file boundary whose test source,
annotation import, and test starter are selected from the captured Spring Boot
major. Both legacy and canonical routes preserve edits to both Java files and
unrelated properties through later generation, then remove only CORS-owned
state; the canonical-only E2E transfers both files byte-for-byte and runs the
Boot 4 preflight test with real Maven after ejection.
Observability extends that proof to four independently merge-managed Java
files under one ejection boundary, a second Boot-version-sensitive import, two
Spring-managed dependencies, and 24 bounded properties. Its differential
scenario preserves all four edits and unrelated property state; its canonical
E2E transfers the exact live bytes and proves the Prometheus scrape with real
Maven after ejection.
Security adds five independently merge-managed files, a data-driven Boot 3
floor, Boot-versioned test imports, and four dependencies under one ejection
boundary. Its differential scenario caught a real legacy shared-ownership bug:
removing Security after CORS drops the Boot 4 web MVC test starter. The
canonical compiler does not copy that defect; dependency reconciliation keeps
CORS buildable, and a canonical real-Maven E2E proves the stacked project after
Security's exact-byte Java ejection.
SSE adds the first declarative pack whose files intentionally span packages:
the hub, scheduling switch and concurrency test live beside the application,
while the stream controller remains in the owned web layer. Four stable
artifact IDs share `cap_sse`; regeneration preserves edits in every file,
removal takes back only the SSE property and implementation, and ejection
transfers exact live bytes while the Web dependency and scheduler pool remain
managed. The canonical real-Maven E2E runs all four generated concurrency
tests after that transfer.
Redis extends the same compiler projection beyond files without claiming an
entire reader document. The accepted lock stores the exact marked Redis service
block as a stable reader facet; `compose.yaml` remains reader-owned around it.
Generate/edit/generate therefore preserves hand edits both inside the service
and in unrelated services, while an emitter change that touches the same line
refuses through the ordinary `git merge-file` path before publication. Its two
Java artifacts, three properties, three dependencies, Failsafe feature and
non-persistent Compose service all come from one declarative pack. The
differential scenario runs this loop through legacy and canonical binaries,
and the canonical E2E compiles the resulting integration test with real Maven.
Kafka validates that reader facets are a general boundary rather than a Redis
special case. One declarative pack projects four merge-managed Java artifacts,
the marked broker block, six Spring dependencies and the complete
serializer/deserializer, consumer-group and producer-durability property set.
Spring and plain Maven select their dependency/source projections from the
same pack: plain Maven receives only the pinned client plus Compose service.
The differential loop preserves edits in every Java file, inside the broker
block, and outside it; the canonical real-Maven E2E compiles and runs the
poison-message policy test after regeneration.
Mail adds a third reader-facet consumer and proves that the abstraction handles
a service whose model identity (`mail`) differs from its Compose name
(`mailpit`). Its sender and container-backed delivery proof are separate merge
artifacts under `cap_mail`; Failsafe, three explicit mail settings,
Boot-sensitive test dependencies and the marked service are derived from the
same declaration. The differential loop preserves both Java edits, an edit
inside Mailpit and unrelated YAML, while the canonical Maven `verify` E2E
compiles and executes the integration-test boundary.
Toxiproxy proves the same compiler pack shape does not require Spring or a
reader-document facet. Its two generated testkit files are independent merge
artifacts under the single `cap_toxiproxy` implementation boundary, with exact
test-scoped dependencies. The 290-line inline Rust string emitter was deleted;
legacy and canonical routes now render the same Java templates. Differential
coverage edits both files, generates again, and removes only Toxiproxy; the
canonical E2E additionally runs `FaultsTest` through real Maven and Docker.
Coverage establishes a separate zero-source pack shape instead of pretending
every capability emits Java. The model lowers it to `BuildFeature::Coverage`;
the workspace owns one marked JaCoCo block per Maven or Gradle dialect, stacks
it independently with integration-test wiring, preserves all reader bytes
outside the block, and refuses edits inside it before publication. Differential
coverage proves add/generate/remove parity with the frozen binary; a canonical
real-Maven `verify` E2E produces `target/site/jacoco/jacoco.xml` and enforces the
declared threshold.
Canonical-only E2Es additionally prove package moves, atomic overlap refusal,
whole-boundary ejection, frozen-model convergence, and real Java compilation.
The first differential run also caught and
removed a real ABI drift: required `int`, `long`, `double`, and `boolean`
fields now remain Java primitives in canonical records and ports, while only
nullable fields use `Optional` of their boxed type.

The representative database-operation slice is now executable rather than a
paper spike. Canonical `add db` compiles commands, queries, and transitions to
separate `JdbcClient` adapters. Commands generate omitted UUID keys, bind
modeled inputs, map omitted optional values to SQL null, and refuse an omitted
required value. Queries use required and presence-sensitive optional filters,
semantic ordering, and a default ceiling of 100. Transitions update by primary
key, use non-set inputs as guards, and publish a modeled domain event inside the
transaction.

The query CLI E2E starts from JDL, generates a scaffold/query/database
capability, hand-edits the adapter, evolves the record, proves the edit
survives, proves an identical sync is byte-stable, forces an overlapping SQL
line and verifies zero writes, retries cleanly, ejects only
`art_cap_db_<operation>_query`, evolves again, and verifies the query ABI stays
managed. A companion write-operation E2E hand-edits command and transition
adapters, evolves both, proves one overlap aborts the model and every generated
file, retries, ejects the command without moving the transition, evolves the
still-managed transition, ejects that second boundary independently, and then
proves later field evolution leaves both reader-owned files untouched while
both ports remain managed. Both scenarios run frozen-model and real Maven
compilation gates when the host supports Java 26.

## The new vision, in one page

**Jails becomes an application compiler, not a file-aware generator.** This
vision assumes Jails is meant to keep a declared application evolving. If its
job ends after scaffolding, stop here and build the one-shot option instead.

The source of truth is one versioned application model. The current CLI stays
pleasant, but every mutating command is only syntax sugar for a `ModelPatch`.
The compiler pipeline resolves that model once and materializes one immutable
plan. Preview, confirmation, export and apply all use that exact plan.
Reproducible source is written to a merge-managed generated tree. Readers may
edit any generated file. Later compilation preserves disjoint edits and
refuses overlapping edits atomically. When a reader must take over an entire
replaceable implementation, `eject` transfers only that implementation
boundary explicitly and permanently while its ABI stays managed.

```text
                       CURRENT

CLI args / app.toml / Java / SQL / ledger / migrations / live DB
          -> Recipe -> strings -> Change -> Desire -> edits
          -> prepared variants -> journal -> receipt -> report
          -> later reparse the strings to rediscover the application

                         NEW

CLI / model file / schema import
          -> ModelPatch
          -> AppModel + captured ProjectFacts
          -> typed compiler passes
          -> PlanDraft
          -> workspace materializer
          -> ONE Plan {
                 next model,
                 semantic summary,
                 ordered content-addressed operations,
                 retriable effects,
                 exact preconditions
             }
          -> preview or execute that same Plan
```

### What the user experiences

The familiar command still works:

```text
jails g scaffold Note id:uuid@pk title:string!
```

But it no longer launches a special scaffold pipeline. It adds or updates a
logical entity in the model:

```toml
[entities.note]
id = "ent_01JNOTE"
java_name = "Note"
facets = ["record", "repository", "service", "http"]

[entities.note.fields.id]
id = "fld_01JNOTEID"
type = "uuid"
primary_key = true

[entities.note.fields.title]
id = "fld_01JNOTETITLE"
type = "string"
non_blank = true
```

`scaffold` is a profile over four ordinary facets. The compiler derives Java,
SQL, HTTP contracts, dependencies and tests from those typed facets. A later
field change edits the model, computes a semantic schema diff, invalidates the
actual dependent nodes, and emits one reviewed migration. It does not reparse
the record, reconstruct a `Recipe`, run the create generator and throw away
the wrong migration.

Generated code lives in a managed source root. Custom application logic lives
in reader-owned source and implements generated ports. If the reader really
wants to own a generated controller implementation:

```text
jails eject implementation.entity.note.http-controller
```

Jails copies that implementation to reader source, marks it external in the
model and never overwrites or destroys it again. Its generated port/DTO ABI
stays managed; an incompatible later model change fails linking until the
external declaration is updated. Ownership is no longer guessed from bytes.

### What disappears

This vision deletes categories, not merely files:

- generated Java/SQL is no longer application state;
- direct CLI and declarative apply are no longer competing planners;
- `Intent -> Recipe -> IntentSpec -> canonical strings -> Recipe` disappears;
- per-kind flag rejection becomes typed declaration parsing;
- dependency inference no longer scans generated bytes;
- field evolution no longer reruns whole generators;
- rename updates logical model edges instead of text plus cloned stale specs;
- kind-specific ownership merge code collapses into one artifact-aware
  three-way merge;
- preview and commit cannot accidentally plan twice;
- wire tags/codecs and command metadata come from separate small,
  single-purpose declarations;
- most transaction storage protects only model, migrations and rare external
  patches, not every reproducible generated byte;
- run/test/db/editor/contract utilities stop expanding the compiler kernel.

### The irreducible kernel

The replacement has five contracts:

1. `AppModel`: what the application means.
2. `WorkspaceSnapshot`: every external fact captured once.
3. `Compiler`: pure snapshot/patch to typed artifacts and a `PlanDraft`.
4. `Plan`: the materialized, exact, reviewable state transition.
5. `Executor`: lock, recheck, stage, apply and recover that plan.

Everything else is a frontend, backend, document adapter or optional tool.
That is the new architecture against which every proposed module earns its
place.

## E2E safety is the rewrite firewall

AI makes replacing 100,000 lines fast; it does not make an unrecorded behavior
obvious. The old binary must become an executable oracle before its internals
are deleted. The rewrite is allowed to move quickly because every agent's work
is checked end-to-end, not because the system is refactored on faith.

The repository already has a strong base:

- 29 Rust integration-test source files under `tests/`, plus the separate
  `jails-commit` crash target, with more than 400 authored test entry points;
- 61 registered generation/capability scenarios and 62 golden directories;
- byte-for-byte generated-tree snapshots through the real `jails` binary;
- a help-derived gate that gives every one of the 39 generator kinds a golden
  scenario and covers 24 of 25 capabilities (`format` is explicitly exempt);
- generate/destroy agreement checks;
- portable-plan, app, history, effects and engine tests;
- real Maven/JDK generated-project compilation and test-report assertions;
- crash/failpoint tests and five protocol fixtures.

That is a useful regression suite, but the test count is not the same as E2E
coverage. The source audit found concrete holes:

| Surface | What is proved now | Rewrite risk still open |
|---|---|---|
| Golden CLI | one real-binary invocation and complete output-tree bytes for each registered scenario | executor state is excluded; `scaffold-plain` is an orphan golden directory; one scenario per kind does not cover option interactions |
| Real Java builds | strong Maven/Gradle fixtures, with several suites pinning XML report/test counts | the shared strict toolbox exercises only about 32 of 39 kinds; several tests generate project A but compile toolbox B; some Spring suites check only Maven exit status |
| CLI/engine | broad command parsing and route coverage | hundreds of CLI tests and dozens of engine tests include component/in-process checks, so they must not be advertised collectively as black-box E2E |
| Crash/recovery | enumerated injected `Err`/unwind sweeps | `before-directory` and `after-file-rename` are advertised but have no matching trip; `after-root-sync` is tripped but unadvertised; no child-process death matrix proves durable recovery |
| Protocol | three useful referenced compatibility fixtures | `testd-request.hex` and `testd-reply.hex` have no source reference, so their presence proves nothing |
| Automation | a local pre-push hook runs `cargo build` and `cargo test` | no repository CI configuration was found; the hook omits `--workspace` crate tests and does not set `JAILS_REQUIRE_TOOLCHAIN=1`, so toolchain checks may self-skip |

The tests named “every generator and capability together” currently run the
same explicit toolbox subset, not every advertised feature. Goldens can also
preserve an existing bug or be mass-regenerated to bless one. Those are the
first coverage defects to fix; they are not reasons to slow the rewrite down.

### Differential E2E harness

Before replacing a path, build and freeze the current binary as
`jails-legacy`. Run legacy and new implementations in twin copies of the same
fixture and compare:

```text
Scenario {
  initial tree,
  environment/tool fakes,
  command sequence,
  expected exits,
  machine stdout/stderr,
  canonical plan,
  final project tree,
  durable model/receipt view,
  rerun/idempotency result,
  generated-project build/test result
}
```

For each scenario, compare:

1. exit status and versioned machine output;
2. semantic plan operations after normalizing only declared nondeterminism;
3. every reader-visible project byte and mode;
4. model/resource ownership and append-only migrations;
5. second-run idempotency;
6. preview versus apply digest;
7. inverse/destroy behavior where supported;
8. generated Maven/Gradle compile and executed test counts;
9. recovery after every real failpoint.

Do not compare new compiler-lock bytes with old journal-directory bytes.
Translate the legacy ledger/receipt into a canonical compatibility view, then
compare its surviving semantics with the new model, accepted projection and
reader-visible tree. Intentional changes require a checked-in expectation or
migration rule; an agent may not silently refresh every golden. Output
comparison is exact by default. A finite `CompatibilityMap` may declare
specific destination changes such as moving a class into the managed root, but
it maps individual artifacts and contracts; it may not exclude a whole
generated directory from comparison.

### Required E2E gates

These are merge gates, not a timeline:

- **G0 — mandatory execution and protocol:** create one `verify-rewrite`
  command that runs format/clippy, `cargo build --workspace` and
  `JAILS_REQUIRE_TOOLCHAIN=1 cargo test --workspace`; pre-push and CI invoke
  only that command. Scenario names and golden directories match exactly;
  every protocol fixture has a source reference; old fixtures decode and new
  canonical encodings are golden.
- **G1 — differential CLI:** all registered scenarios run through both
  binaries and compare exit, plan, output tree, rerun and destroy semantics.
- **G2 — behavior journeys:** all 100 live command paths map to at least one
  checked-in journey. Cover success/refusal, CLI/manifest equivalence, reader
  edits and conflicts, lifecycle operations, plan portability, old-state
  import and pairwise option interactions.
- **G3 — exact real toolchain:** maintain a machine-readable map from every
  generator kind and capability to a build fixture. Build the exact generated
  tree under test—not a neighboring toolbox—and pin Surefire, Failsafe and
  Gradle report counts plus unexpected skips.
- **G4 — crash/recovery:** each advertised fault is asserted to fire in a
  child process that dies without unwinding; restart reaches exactly pre-plan
  or post-plan state, a second restart is idempotent, and effects are
  preserved. Generate the registry and trip sites from one declaration.
- **G5 — real-project corpus:** promote the proof manifests and `validation/`
  workouts, then add sanitized adopted and reader-edited Spring/plain projects.
  Each runs legacy and new plan/apply/build plus semantic comparison and rerun.

Independent AI agents can rewrite model, compiler, emitters, command schema
and executor in parallel. No workstream merges unless its relevant old-vs-new
gate is green. Once all gates are green, cut over once and delete the legacy
path.

## Audit basis

The four full-scope audits began against HEAD `f0e66829...` and were refreshed
through the live filesystem based on HEAD `143c55ed...`. While this document
was being written, the `--bind` feature added one protocol file and touched the
engine, generator and root CLI. A later uncommitted `architecture baseline`
command added `crates/jails-drive/src/baseline.rs` and modified six more Rust
files under `crates/` and `src/`. Both complete deltas were read separately.
The final inventory below includes them and is not an inference from the module
graph:

| Scope | Rust files | Raw lines | Main concern |
|---|---:|---:|---|
| `crates/jails-engine` + root `src` | 66 | 19,001 | orchestration and CLI |
| `jails-generate` + `jails-java` | 64 | 27,950 | lowering and rendering |
| `jails-protocol` + `jails-project` + `jails-spec` + `jails-state` | 87 | 39,221 | domain, wire values and project state |
| `jails-commit` + `jails-prepare` + `jails-drive` + `jails-report` + `jails-support` + `jails-testkit` | 84 | 36,696 | planning, transactions and tools |
| **Total** | **301** | **122,868** | **96,689 nonblank code lines** |

The totals include colocated tests and should not be read as 96,689 lines of
production logic. They are useful for scale and coverage, not as a productivity
metric.

Every baseline Rust file was inventoried and assigned to one of four audits;
each new file and every modified Rust hunk that appeared afterward were then
read directly. Findings are grouped by responsibility below instead of
repeated as a 301-row filename dump. The codebase graph was used first for
structure, call paths and hotspots. Its project matched an early committed
HEAD, but the indexed generation was behind the final `--bind` delta and exact
coverage checks also reported many earlier files as metadata-changed or absent.
A clean graph coverage result means no *recorded* gap, not proof that an
exhaustive query is complete; the filesystem inventory and source reads are
the authority for the exhaustive statements here. Fresh coverage calls for the
live deltas were unavailable after the graph transport closed; those paths are
therefore qualified by direct source inspection rather than a final indexed
generation.

The measured graph also supports the qualitative picture. Among the heaviest
cross-crate boundaries were protocol-to-support, generate-to-project,
generate-to-protocol, drive-to-support, prepare-to-protocol and
engine-to-nearly-everything. `jails-protocol` is therefore not a small bottom
layer in practice, and `jails-engine` is an integration layer rather than a
domain core.

## What the program actually does today

The intended transaction flow is coherent when viewed alone:

```text
argv / app.toml / imported plan
        |
        v
CLI structs and hand-written dispatch
        |
        v
Intent + Recipe + canonical request syntax
        |
        v
engine route: capture project and machine state
        |
        v
generator: Java/SQL/config artifact strings
        |
        v
Desire -> reconcile -> merge -> PreparedChange
        |
        v
Prepared transition + ledger + objects + effects
        |
        v
journal -> activate files -> recover/roll forward
        |
        v
receipt -> Outcome -> command envelope -> report
```

The difficulty is that semantic decisions occur at almost every box. A field
name can be parsed at the CLI, projected to Java, projected again to SQL,
recovered from a generated record, inferred from a migration, stored in a
ledger record, copied into a portable plan and reconstructed during recovery.
There is no single value one can point at and say: “this is the application.”

The generator audit makes the accidental compiler especially visible:

- `Recipe` is a global cross-kind bag of optional values
  (`crates/jails-generate/src/generate.rs:428`).
- `refuse_misplaced` separately maintains the kind-by-flag validity matrix
  (`generate/recipes/flags.rs:17`).
- `artifacts_for` destructures and revalidates it in a 700-plus-line match
  (`generate/recipes.rs:32`).
- `plan_recipe` then adds dependencies, properties, registrations, package
  files, CLI support and DDL redirection after rendering
  (`generate.rs:266`).
- `write_new_file` performs still more semantic-looking normalization at the
  write boundary (`generate/write.rs:55`).

The graph found 140 direct callees from `artifacts_for` and 41 from
`plan_recipe`. That is not merely a “large function” problem. It is evidence
that validation, semantic lowering, rendering and requirement collection have
not been separated.

## The four sources of complexity

### 1. Product breadth

Jails is not one scaffolder. The root CLI also contains project creation,
history and undo, schema comparison and pull, SQL contracts, editor protocol,
HTTP contract comparison, requests, runners, consoles and logs. `jails-drive`
adds process launch, testing, daemon management, affected tests, migration,
Kafka, benchmarking and linting. `jails-report` adds a substantial diagnostic
suite.

Many of these are useful. They are nevertheless separate products sharing one
binary. No compiler architecture can make fifty-plus distinct Spring and Java
opinions cost the same as five. Before measuring a refactor by LOC, decide
which of these opinions belong in the core product and which are optional tool
packs.

### 2. The editable-output paradox

The README promises that the reader may edit generated files, while `remove`
and `destroy` still know what they may take back. This creates an unavoidable
information problem. Once a generated `CsvReader` has been edited, its bytes
do not reveal whether it is still generated, partly generated, or entirely
reader-owned. Safe later mutation requires stable artifact identity, a
reproducible base, the live bytes, the next render and one conflict policy.

The current ledger, object blobs, force policy, receipts and undo are several
overlapping answers to that ambiguity. The semantic model plus one exact
accepted projection makes the preimage unambiguous, so they can be deleted
while retaining one small three-way merge and exact-plan preconditions. That
single projection is necessary across compiler upgrades; historical objects
are not.

The ordinary workflow is a merge users already understand:

```text
accepted render (BASE) + hand-edited file (OURS) + next render (THEIRS)
    -> clean exact after-image | atomic conflict refusal
```

Permanent ownership transfer remains an explicit state transition:

```text
managed implementation --jails eject--> reader-owned implementation
```

Before ejection, Jails merge-manages the artifact. After ejection, it never
rewrites or destroys that implementation source. Generated ports and DTOs
remain the managed ABI; an incompatible model change fails linking until the
external implementation declaration is updated.

### 3. No canonical semantic world

Generator-owned source currently acts as persistent state:

- records, enums and sealed types are rediscovered from Java in
  `generate/domain.rs`;
- scaffold fields are read back from generated files in
  `generate/scaffold.rs`;
- workflow targets are reconstructed from records in
  `spring/workflow.rs`;
- uniqueness is inferred by scanning historical SQL in `spring/schema.rs`;
- the engine converts between Java member and SQL column spellings in
  `route/request.rs:150` and `route/request.rs:190`;
- field evolution decompiles a type, reconstructs a recipe and reruns the
  generator (`route/field/evolution.rs:103`).

Source import is a valid adapter for adopting an existing project. It is a
poor internal bus between two compiler passes. Facts Jails originated should
remain typed facts until emission.

The lack of a world model also explains hard-coded invalidation. For example,
`route/field/companion.rs:131` lists five artifact kinds known to read a
target's components. A real reference graph would derive that relationship
from typed edges such as `query.on`, `query.via`, `command.yields` and
`association.parent`.

### 4. One intent is copied through too many representations

There are legitimate boundaries between user syntax, semantics, a plan and a
durable receipt. The current code has more than those four. A mutation may be
represented as CLI structs, `Intent`, `Recipe`, `Declared`, `Asked`,
`CanonicalMutationRequest`, `CanonicalRequestSyntaxV1`, `Desire`, resource
claims, prepared identities, prepared changes, ledger rows, file operations,
journal records, receipts, `Outcome` variants and two command-envelope
versions.

`crates/jails-engine/src/route.rs:206` contains `Recorded`, an owned copy of
recipe fields used to reconstruct a borrowed `Recipe`. Its own commentary
records why: several manual unpacking sites omitted a newly added option.
`route/request.rs:71` performs another optional-field projection. These are
not domain types; they are adapters between duplicate representations.

A concurrent change during this audit provided a useful controlled example.
Adding one `--bind component=parameter` concept touched **20 Rust files** across
root CLI/manifest, engine `Intent`/`Recorded`/request projection, generator
`Recipe` and flag validation, endpoint/rendering, protocol declarations,
identity and durable compatibility. It also required a new wire type and a
ledger codec bump. The new `Endpoint::bindings` is a good local owner for the
rendering rule, but the breadth of propagation is the architectural signal: a
single semantic field must still be copied through every phase and storage
shape. In the proposed design it would enter one `OperationDecl`, resolve to
one endpoint IR field, and have generated wire/front-end projections.

A second live change added `jails architecture baseline`. Its core behavior is
small, but the command required a new Clap variant and subcommand, another
`main.rs` match arm, a public drive module, widened build-runner visibility,
generator remediation text and architecture-classification entries. This is
the command-side version of the same signal: adding one verb crosses several
manual catalogs and ownership boundaries. In the proposed design its command
metadata comes from the catalog and its implementation is one optional tool
handler; neither the compiler nor mutation protocol learns the verb.

The transaction layers have the same pattern around
`PreparedKind`, `OperationSemantics`, `PreparedIdentity`, `PreparedChange`,
ledger records, effects, journals and receipts. Each type may be locally
well-motivated, yet recovery and normal execution must manually preserve the
same meaning across all of them.

## Findings by subsystem

### Root CLI and `jails-engine`

The root `src/main.rs` is a large hand-written match from Clap variants to
facade and route calls. `src/cli.rs` is itself more than a thousand lines and
is supplemented by command-specific modules. Command meaning is then restated
in canonical command-path aliases, request fingerprints, JSON refusal
envelopes, editor completion and dispatch.

There is already a portable-plan seam, which is valuable. But the ordinary
confirmation path in `src/dispatch.rs:122` computes a pretend outcome for the
prompt and, unless `--plan-out` was requested, invokes the route again for the
commit (`src/dispatch.rs:175-218`). The comment promises the same computation;
the implementation normally repeats the computation. The API should make the
promise literal: `compile -> Plan`, prompt over that exact value, then
`execute(Plan)` after rechecking its preconditions.

`src/app/manifest.rs` hand-parses a narrow TOML subset and manually maintains
`KNOWN_GENERATE_KEYS`. Its header says this avoids dependencies, but `toml`,
Serde and Serde JSON are already workspace dependencies. A closed
`#[serde(deny_unknown_fields)]` model can retain fail-closed behavior while
deleting a custom comment/string/array parser and its second schema table.

`src/new/*` implements a separate `Publication` transaction. Its sibling
scratch directory and final rename are sound for creating an absent directory,
but new-project generation should still use the same compiler output type as
an existing project. “Publish a complete generated tree into an absent
destination” is an executor backend, not a separate generation architecture.

Within the engine, `route/artifact.rs` mixes parsing, policy, planning,
provenance, storage evolution, read-set widening and commit. `destroy` alone
contains feature-specific lifecycle and storage rules. `route/maintenance/*`
is effectively a Java/SQL refactoring compiler embedded in orchestration.
`route/support.rs:49` infers dependencies by scanning emitted Java bytes for
AssertJ, Failsafe, Web MVC, JDBC and Docker-related evidence. Requirements
should be typed outputs of lowering, not semantic analysis performed after
code generation.

The engine also records incomplete provenance. `route/provenance.rs` uses no
template identity and an empty relevant-input set because generators read the
project directly. A compiler that receives a captured `World`/`ProjectFacts`
value can report its inputs structurally and can compute a real dependency
graph.

`Asked` is another incomplete command model. It holds a canonical semantic
request beside a manually reconstructed syntax map, but routes decide which
options make it into that map. For `app apply`, the request records little more
than `no_start`, the syntax does not identify the manifest, and
`manifest_source` remains empty (`route/request.rs:623-770`). Two different
manifests can therefore share an invocation fingerprint even though their
prepared bytes differ. Parsing should produce one `ParsedInvocation` with
semantic command, explicit syntax and source input identities; all hashes and
reports derive from it.

Evolution and rename reveal the same missing model. Several field/lifecycle
paths rerun a full recipe and then delete the create-migration it generated,
before adding a different forward migration. Companion invalidation is a
five-kind table. Drop-field rederives a SQL column with `snake_case` instead of
using the stored physical column, which is wrong after a preserve-column
rename (`route/field.rs:428-437`). The rename path textually rewrites generated
Java and moves entity IDs while cloning referenced specs unchanged
(`route/maintenance/rename/source.rs:7-32`); dependent `on`, `yields` or field
references can remain semantically stale even when source bytes show the new
name. Stable logical IDs plus model/schema diffs remove all three failure
classes.

Finally, some orchestration bypasses its own transaction story. `app apply`
may commit its aggregate plan and then format as a second mutation; the
formatting failure is swallowed (`route/app.rs:96-115` and
`route/capability.rs:107-136`). Formatting must either be a pure compiler pass
or a recorded retriable effect. It should not be an invisible second
transaction.

### `jails-generate` and `jails-java`

The generator contains good local abstractions, but they are islands rather
than one pipeline. `named_query.rs` accepts a verified query contract;
`spring/query/shape.rs`, transition structs and workflow resolution are small
typed IRs. The proposed compiler should promote this pattern across every
recipe.

The largest repeated costs are:

- cross-product branching over kind, flag, project shape and backend;
- independent type/name/sample tables;
- rendering structural Java and SQL as strings;
- reparsing generated source;
- inferring support requirements from emitted paths or bytes;
- parallel implementations of the same semantic operation.

Repository generation is a concrete example. JdbcClient and plain-JDBC paths
independently derive columns, keys, casts, insertion, mapping, ordering and
tests in `generate/repository.rs`. Both should consume one `RepositoryIR` and
differ only in backend lowering.

Type behavior is similarly spread across SQL mapping, Java samples, JSON
samples, named-query JDBC types and boxing, DTO validation, durable-job
samples and pin literals. One test in `generate.rs` exists specifically to
keep duplicate JSON sample tables synchronized. That invariant should be a
single `TypeSemantics` value.

The template layer is strict about placeholder sets, which is good, but it is
text substitution. Renderers therefore assemble imports, annotations,
expressions, fragments and conditional members as strings. Some structural
variants mutate already-rendered source with `.replace`. A template engine
with `if` and `for` can improve ergonomics, but it will not create a semantic
model. Typed Java/SQL builders should decide structure; templates should only
format stable leaves or whole, simple skeletons.

`jails-java` now contains several partial scanners: one for Java declarations
and annotations, one for identifier-aware rename, one for text blocks/import
tidying, plus anchor-based annotation and dispatch edits. Do not write a full
Java compiler in Rust. Use one lossless token/CST facade for adopted or
reader-owned Java, and keep normal managed generation independent of parsing.
If a JVM implementation is chosen later, use the Java ecosystem's syntax
model instead of maintaining these scanners.

### `jails-protocol`, `jails-spec`, `jails-project` and `jails-state`

These crates are intended to hold validated values and the resolved project,
but several different kinds of stability are mixed together:

- semantic concepts such as recipes, fields, resources and effects;
- versioned wire representations used by plans, ledgers and receipts;
- project discovery and build/layout facts;
- decoders for Jails-owned files;
- projections reconstructed from those files;
- live or adopted Java/SQL evidence.

That mix makes “protocol” depend on Java, spec and support, and makes most
higher layers depend on protocol. It is a wide vocabulary crate rather than a
small stable boundary. Re-export facades in each crate make moves convenient,
but they also allow module code to see a much broader lower-layer surface than
its real responsibility requires.

The right separation is by *reason for change*, not by whether a struct looks
like data:

- semantic model types change when the Jails language changes;
- wire DTOs change only when an on-disk/API version changes;
- project facts change when Maven, Gradle, Java or Spring detection changes;
- state adapters change when storage changes.

Wire DTOs should decode and immediately convert to semantic types at the
boundary. They should not become the model passed throughout the compiler.
Likewise, project discovery should produce a closed `ProjectFacts` snapshot;
compiler passes should not retain a `Project` that lets them read arbitrary
files later.

The current code has useful building blocks to keep: validated names and
identifiers, closed compatibility classifiers, field syntax, build-layout
facts, SQL parsing and conservative refusal. The simplification is to give
each one a single role and to stop using the ledger projection as a substitute
for an application model.

### `jails-prepare` and `jails-commit`

The legacy kernel tries to provide rollback/roll-forward for a very broad set
of arbitrary mutations and external effects. That contract forced a lock,
staged objects, a durable intent journal, receipts and mirrored recovery paths.
The rewrite should not reproduce that machinery under new names.

Narrow the canonical mutation contract instead: all reproducible source is
merged before planning; every file write is exact and idempotent; append-only
migrations have deterministic paths; external effects are outside the source
commit; and the compiler lock is published last. A crash can leave a mixed but
valid intermediate checkout which the same semantic command deterministically
converges. This removes the need for a durable roll-forward protocol while
retaining precondition checks and refusal on unexpected bytes.

The current preparation layer's parallel resource, operation, ledger and
effect representations, and commit's journal/receipt/object/recovery forms,
remain valuable evidence for differential tests. They are not destination
types. The concrete defects below explain why the old path should be frozen
and deleted after parity rather than adapted into the new compiler.

The long-term transaction kernel should accept one canonical `Plan`, persist
that same value once, and execute a small sequence of idempotent operations.
It should know nothing about recipes, Java, Spring, resources or human report
wording. External effects such as starting services or running a formatter
should normally be retriable follow-ups after the project mutation, not file
transaction states.

### `jails-drive`, `jails-report`, `jails-support` and `jails-testkit`

`jails-support` is mostly appropriately low-level, but broad facade re-exports
make helpers appear as an implicit shared framework. Keep only genuinely
domain-free filesystem, process, codec and lock primitives. A helper used by
one subsystem should live with that subsystem.

`jails-drive` and `jails-report` contain useful, substantial applications.
Their complexity is mostly orthogonal to compiling an application model. A
schema inspector, test daemon, process launcher, Kafka helper and diagnostic
engine need not share the compiler's mutation vocabulary or release cadence.
They can remain in the workspace while becoming separate command packs or
binaries behind a small JSON/subprocess protocol.

This is not a call to build a general in-process plugin lifecycle. The simplest
extension seam is Unix-like: `jails foo` may delegate to a `jails-foo`
executable that receives a versioned request and project-facts file. Crashes,
dependencies and state remain isolated. Commands that are central to model
compilation stay built in; tools that merely inspect or launch can move out.

`jails-testkit` is already tiny and should remain tiny.

## The product choice

There are four coherent architectures. The current hybrid is the expensive
space between them.

### A. Honest one-shot generator

Jails writes files once; after that they are the reader's files. Preview is a
diff, and Git is the normal undo mechanism. There is no later managed destroy,
rename, sync, declarative apply, ownership merge, object store or receipt
history.

This is the smallest and most Rails-like product. It is attractive if Jails'
primary value is “start me with excellent Java,” not “keep my application
model reconciled.” It gives up many features the current code has spent most
of its complexity making safe.

### B. Merge-managed compiler with implementation-boundary ejection — chosen

The application model is semantic source. Managed Java, tests, HTTP
collections and reproducible configuration are compiler projections, but
their live files are intentionally editable. Every regeneration performs a
three-way merge from accepted-model render, live bytes, and next-model render.
Disjoint edits survive; overlaps refuse before a plan exists. Readers eject a
declared implementation boundary only when they want Jails to stop generating
that artifact entirely.

Schema migrations are not reproducible output: they are append-only history
events produced by a semantic model diff. Rare patches to reader-owned build
or configuration files are also explicit plan operations, not ordinary
renderer output.

This keeps declarative apply, preview, safe evolution, deterministic generation
and the iterative edit loop. The merge kernel stays small: read the one
accepted projection from the compiler lock, shell out to the proven three-way
merge, and freeze clean merged bytes into the exact plan. Unique artifact IDs,
rather than entity IDs or file paths, pair projections across rename. Separate
implementation-boundary IDs scope ejection across one or more artifacts.

### C. Disposable managed tree with ejection

This makes generated output replaceable wholesale and requires an explicit
ejection before any edit. It removes the merge but breaks the defining
workflow: adding one method to `Task.java` would force ownership transfer before
the next `jails g field`. It is coherent, but it is not this product.

### D. JVM build-time compiler

A more radical destination is a Java compiler core packaged as a Maven/Gradle
plugin or annotation processor. It could read the same application model,
generate source during the build and use a mature Java syntax/emit model. A
small native launcher could retain the fast CLI experience.

This moves Jails into its target ecosystem and makes generated-source roots a
natural build concept. It also adds JVM startup, plugin-version compatibility,
two build-tool adapters and a harder adoption story. SQL migration allocation
still cannot safely happen as an incidental annotation-processing side effect.
Treat this as a prototype or alternative backend, not the core vision.

## Recommended semantic model

### Explicit stable identities, mutable labels and projections

Names alone make rename ambiguous. A TOML table key is also not immutable: a
manual edit from `[entities.order]` to `[entities.purchase]` otherwise looks
like delete plus create. Give every model node a visible, generated stable ID.
The table key is a convenient label; identity survives its rename.

```toml
schema = 2

[project]
base_package = "com.example.shop"
platform = "spring"

[entities.order]
id = "ent_01JORDER7K3"
java_name = "PurchaseOrder"
table = "orders"
facets = ["record", "repository", "service", "http"]

[entities.order.fields.id]
id = "fld_01JORDERID"
java_name = "id"
column = "id"
type = "uuid"
primary_key = true

[entities.order.fields.customer]
id = "fld_01JCUSTOMER"
java_name = "customerId"
column = "customer_id"
type = "uuid"

[operations.find_recent_orders]
id = "op_01JRECENT"
kind = "query"
java_name = "RecentOrders"
on = "order"
order_by = ["created_at desc", "id"]
limit = 100
```

The IDs are stable semantic identities. Labels such as `order` are resolved to
those IDs when the model links. Changing `java_name` is a Java rename; changing
`table` or `column` is an explicit storage rename. A CLI rename preserves the
ID and updates label references atomically. A manual label rename that leaves
an unresolved reference fails linking; it is never inferred as delete/create.
This replaces the current repeated round-trip between Java member, SQL column,
URL, property and package spellings.

The model file(s) are the **only desired-state authority**. A root file may
import closed-schema fragments, so “one model” means one linker rather than one
blob. The compiler lock retains only the accepted model and exact accepted
projection needed to reproduce the next merge BASE. Git supplies longer-lived
audit history. A CLI `ModelPatch` includes the resulting model-file bytes in
the plan. Manual edits simply change the next captured digest.

### Typed declarations, not an optional-field soup

The CLI and manifest should lower immediately to variants for which irrelevant
states cannot exist:

```rust
enum Declaration {
    Entity(EntityDecl),
    Operation(OperationDecl),
    Capability(CapabilityDecl),
}

enum OperationDecl {
    Create(CreateDecl),
    Query(QueryDecl),
    Transition(TransitionDecl),
    Event(EventDecl),
    Worker(WorkerDecl),
    Workflow(WorkflowDecl),
}

struct QueryDecl {
    id: OperationId,
    target: EntityId,
    filters: Vec<FilterDecl>,
    via: Option<EntityId>,
    order: Vec<Ordering>,
    limit: Limit,
    endpoint: Endpoint,
}
```

A query cannot accidentally carry a durable-job-only option because the type
has nowhere to store it. The negative kind-by-flag table disappears.

### One semantic world

All declarations link into a canonical graph:

```rust
struct AppModel {
    schema: ModelSchema,
    project: ProjectIntent,
    entities: BTreeMap<EntityId, Entity>,
    operations: BTreeMap<OperationId, Operation>,
    capabilities: BTreeMap<CapabilityId, CapabilityIntent>,
}

struct Entity {
    id: EntityId,
    names: EntityNames,
    fields: Vec<Field>,
    constraints: Vec<Constraint>,
    facets: BTreeSet<Facet>,
}

struct EntityNames {
    java_type: JavaTypeName,
    sql_table: SqlIdent,
    route_segment: RouteSegment,
    config_prefix: ConfigPrefix,
}

struct FieldNames {
    java_member: JavaMemberName,
    sql_column: SqlIdent,
}

struct OperationNames {
    java_type: JavaTypeName,
    route_segment: RouteSegment,
}
```

Each node owns only the projections that make sense for that node; this must
not become another universal `NameSet` full of `Option`s. Name derivation is a
pure projection from the semantic node, while explicit model values override
that projection.

`scaffold` becomes a profile that adds a known set of facets to one entity. It
does not need its own monolithic implementation. `destroy scaffold` removes
those model facets; dependency analysis determines the output delta.

References such as `on`, `via`, `yields` and associations are graph edges. A
field change invalidates every compiler node that actually references the
field or entity. No recipe-kind invalidation table is needed.

### One type algebra

Every builtin field type should have one semantic definition:

```rust
struct TypeSemantics {
    java: JavaType,
    sql: DialectMap<SqlType>,
    jdbc: JdbcCodec,
    wire: WireCodec,
    validation: ConstraintPolicy,
    samples: SampleSet,
    key_policy: KeyPolicy,
}
```

Java imports, primitive boxing, SQL type, JDBC read/write, wire conversion,
validation, and Java/JSON/SQL samples all derive from this table. An external
project-owned type enters the model as a typed unresolved/resolved symbol with
explicit capabilities; it does not trigger scattered “maybe disable the test”
branches.

If the compiler cannot synthesize a required sample, it should emit a
diagnostic tied to the declaration. A user may provide a sample or choose to
omit a proof explicitly. Silently emitting `@Disabled` is a weak substitute
for missing type semantics.

## The compiler pipeline

The important rule is that only the capture/adoption boundary reads the
workspace. Every later pass is pure over explicit input values.

### Pass 0: capture

Capture one immutable input:

```rust
struct WorkspaceSnapshot {
    model: Versioned<AppModel>,
    project: ProjectFacts,
    external_types: ExternalTypeIndex,
    migration_history: MigrationHistory,
    owned_patches: OwnedPatchState,
    preconditions: SnapshotPreconditions,
}
```

`ProjectFacts` contains the exact Maven/Gradle, Java, Spring, package, layout
and configuration facts compiler passes are allowed to observe. An adoption
adapter may parse existing Java and SQL into `ExternalTypeIndex` and an import
proposal. Normal generation never reparses its own output.

This makes read sets and provenance real. If a pass asks the snapshot for an
entity, type or build capability, that query can be recorded. A renderer can
no longer reach through `Project` and silently read another file.

### Pass 1: parse front ends to `ModelPatch`

Current CLI syntax and `app.toml` both produce the same typed patch:

```rust
enum ModelPatch {
    AddEntity(EntityDecl),
    AddFacet(EntityId, FacetDecl),
    AddOperation(OperationDecl),
    ChangeField(EntityId, FieldId, FieldPatch),
    RenameProjection(NodeId, Projection, String),
    Remove(NodeId, RemovalPolicy),
    SetCapability(CapabilityId, CapabilityPatch),
    ReplaceModel(AppModel),
}
```

An imperative command is not a competing authority. It edits the same model
that declarative apply replaces. If no model exists, the first command creates
one by importing the current legacy ledger/project once.

The closed option schema can be data-driven. A `RecipeSpec` may define command
name, aliases, arguments, help, applicability and the typed lowering function.
That one catalog can build Clap, manifest schema, completion, documentation and
canonical request metadata. It should not encode arbitrary semantic code in
YAML; lowering remains typed Rust.

### Pass 2: link, resolve and validate

Resolve logical IDs, fields, packages, routes, capabilities and project-owned
symbols. Validate constraints once. Produce diagnostics with model spans. At
the end of this pass the compiler has a valid `World`; later emitters do not
perform user-facing semantic validation again.

This is where defaults live. The audit found one current example in which the
artifact path constructs a default `PUT` endpoint while `Recipe::http_method`
defaults to `GET` (`generate/recipes.rs:59` versus `generate.rs:478`). A
resolved operation must contain one method, so downstream disagreement is
impossible.

### Pass 3: normalize facets and derive the dependency graph

Expand profiles such as `scaffold` into primitive facets, derive names once,
and build a graph of what reads what. A compact primitive vocabulary could be:

- entity/value type;
- repository/storage;
- command/create operation;
- query;
- transition;
- endpoint/contract;
- event/worker/workflow;
- capability.

The existing named recipes remain friendly syntax and migration-compatible
identities. Internally, their output is a composition of these primitives.
This lets scaffold, use-case, query, transition, durable work and messaging
share lowering instead of each assembling a vertical slice from strings.

### Pass 4: lower to typed artifact IR

```rust
enum Unit {
    Java(JavaFile),
    Sql(MigrationUnit),
    Http(HttpCollection),
    Property(PropertyClaim),
    Dependency(DependencyClaim),
    Registration(RegistrationClaim),
}

struct JavaFile {
    identity: ArtifactId,
    package: Package,
    imports: BTreeSet<Import>,
    declarations: Vec<JavaDecl>,
    requirements: BTreeSet<Requirement>,
    provenance: Provenance,
}

enum JavaExpr {
    Variable(VarId),
    Field(Box<JavaExpr>, FieldId),
    Call(FunctionId, Vec<JavaExpr>),
    Construct(JavaType, Vec<JavaExpr>),
    Literal(Value),
    Convert(CodecId, Box<JavaExpr>),
}

enum SqlExpr {
    Column(ColumnId),
    Parameter(FieldId),
    Literal(SqlValue),
    Compare(Comparison, Box<SqlExpr>, Box<SqlExpr>),
    Boolean(BooleanOp, Vec<SqlExpr>),
    Cast(Box<SqlExpr>, SqlType),
}
```

Expressions do not embed a caller's variable name, so changing `value` to
`candidate` never requires string replacement. Imports are sets, not
pre-rendered blocks. Requirements attach to semantic units; the compiler does
not scan the bytes for `assertThat`, `@WebMvcTest` or JDBC names.

Typed IR need not model every token of Java. Model the types, imports,
annotations, declarations, statements and expressions that generators compose
dynamically. A checked whole-file template remains fine for a stable leaf. The
test is whether semantic code is passed around as a string and later inspected
or mutated.

### Pass 5: derive schema and evolution

Project the resolved world into `SchemaModel` and compare it with the last
accepted semantic schema by stable logical ID:

```rust
enum SchemaChange {
    CreateTable(Table),
    RenameTable { id: EntityId, from: SqlIdent, to: SqlIdent },
    AddColumn { table: EntityId, field: Field },
    RenameColumn { field: FieldId, from: SqlIdent, to: SqlIdent },
    AlterColumn { field: FieldId, change: TypeChange, policy: ChangePolicy },
    AddConstraint(Constraint),
    DropConstraint(ConstraintId),
    DropColumn { field: FieldId, policy: DropPolicy },
    DropTable { entity: EntityId, policy: DropPolicy },
}
```

Classify changes rather than pretending all diffs are equally automatable:

- safe and automatic: create table, add nullable/defaulted column, add index,
  some validated widenings;
- requires explicit policy: rename, non-null backfill, representation change,
  online cutover;
- destructive/refused by default: drop data, unsafe narrowing, ambiguous
  identity.

Stable IDs make differences unambiguous; they do not make multi-deployment
changes automatic. Expand/backfill/dual-read/cutover/contract must be explicit,
persisted `Evolution` programs with named steps, deployment preconditions and
proofs. Initially automate only conservative single-step DDL. Port the current
campaign machinery into typed evolution programs before deleting it; never
infer a rolling migration from two schema snapshots.

The existing rename and field-evolution routes contain valuable policy that
can move into this pass. Their source scanning, recipe reconstruction and
whole-generator replay can then be deleted.

Declared schema, migration history and a live database should not be symmetric
authorities. Give each a role:

- the model states desired semantics;
- migrations are append-only history of accepted transitions;
- the live database is observed evidence and drift detection;
- `pull` proposes a model patch; it never silently changes authority.

### Pass 6: emit a draft, then materialize one exact plan

```rust
struct PlanDraft {
    next_model: Versioned<AppModel>,
    generated: RenderedTree,
    migrations: Vec<RenderedMigration>,
    reader_document_intents: Vec<DocumentIntent>,
    follow_up_effects: Vec<Effect>,
    summary: SemanticPlan,
    diagnostics: Vec<Diagnostic>,
}

struct Plan {
    id: PlanId,
    compiler: CompilerVersion,
    base: SnapshotPreconditions,
    input: CanonicalModelPatch,
    summary: SemanticPlan,
    operations: Vec<PlannedOperation>,
    follow_up_effects: Vec<Effect>,
    digest: PlanDigest,
}

enum PlannedOperation {
    ReplaceModelFile { before: Option<FileImageRef>, after: FileImageRef },
    PublishMergedTree { root: ProjectPath, before: Option<TreeId>, after: TreeId },
    AppendMigration { path: ProjectPath, after: FileImageRef },
    PatchReaderFile { path: ProjectPath, before: FileImageRef, after: FileImageRef },
    ReplaceCompilerLock { before: Option<FileImageRef>, after: FileImageRef },
}

struct TreeEntry {
    kind: FileKind,
    mode: FileMode,
    blob: BlobId,
}
```

The pure compiler returns `PlanDraft`. The workspace materializer first
reconciles every artifact as `BASE = accepted projection`, `OURS = live file`,
`THEIRS = next compiler projection`. Only clean merge results enter the tree
manifest. It then applies document intents to the captured
`WorkspaceSnapshot`, renders the canonical model file, hashes every exact
after-image, and returns a `PlanBundle { plan, trees, blobs }`. That is the
only boundary at which a semantic patch becomes filesystem bytes:

```text
WorkspaceSnapshot + CanonicalModelPatch
    -> compiler PlanDraft
    -> workspace reconcile BASE/OURS/THEIRS
    -> workspace materializer PlanBundle
    -> preview/export OR executor
```

Human review, JSON output and portable serialization are projections of the
one exact `Plan`. `PublishMergedTree` is deliberately not
`ReplaceGeneratedTree`: its after-tree already contains every surviving reader
edit. Applying it first rechecks the complete captured base; it never reparses
argv, reruns a document backend, recompiles, or performs a merge. A stale plan
is rejected before any write. A caller may explicitly request a recompile, but
that produces a new digest and a new thing to review. `TreeEntry` makes file
kind and mode part of identity, so the E2E comparison and executor share the
same definition of a tree. `PlanDigest` commits to the semantic summary,
preconditions, ordered operations, every referenced tree/blob digest and the
effect identities; a portable `PlanBundle` carries those exact referenced
objects. The compiler-lock operation is always last because that lock is the
commit marker and the exact BASE for the next merge.

`Outcome` can become an execution record over a plan rather than a sum of
planned, committed, recovered and effect-retry variants with many projection
methods:

```rust
struct Execution {
    plan: PlanIdentity,
    core: CoreStatus,
    recovery: Vec<RecoveryEvent>,
    effects: Vec<EffectStatus>,
    timing: Timing,
}
```

## Managed output and ejection

This boundary determines whether the architectural simplification is real.

### Managed tree

Put reproducible compiler output under an unmistakable source root, for
example:

```text
.jails/generated/main/java/
.jails/generated/test/java/
.jails/generated/main/resources/
```

or a conventional visible `generated-src/jails/...` equivalent. Maven and
Gradle each receive stable, source-set-aware integration for those roots.
Maven uses `add-source` for main and `add-test-source` for tests; Gradle patches
the corresponding `main` and `test` source sets. The ownership property, not a
build-tool shortcut, is the invariant.

Choose one primary build contract rather than leaving regeneration ambiguous:
**commit the merge-managed generated tree**. Maven/Gradle receive a one-time
source root, but no Jails plugin is required in every IDE/build. CI runs
`jails model check --frozen`; generated and hand-written deltas remain
reviewable together.

Reader code may live in ordinary source or as edits inside the generated tree.
Jails does not infer semantics from those edits: it carries them as OURS in a
three-way merge. It performs no identifier surgery in ordinary source as part
of normal generation.

### Ejection

Ejection is allowed only at a declared **implementation boundary**, not an
entity and not an arbitrary source span. Merge identity and ownership identity
are separate:

- every emitted file has a unique `artifact_id`, which pairs its old and new
  compiler projections for three-way merge;
- one or more cohesive implementation files may share an `ejection_id`, which
  is the ownership boundary transferred by `jails eject`;
- generated ABI records and ports have no ejectable boundary and therefore
  remain managed.

A controller and its companion test, for example, have different artifact IDs
but share one HTTP-adapter ejection ID. Editing either file does not eject
anything; both remain merge-managed. Explicitly ejecting the HTTP adapter
transfers both together while the request/response records and service port
stay generated. One file cannot be half managed by two boundaries.

`jails eject <implementation-id>` should:

1. materialize every artifact in the selected boundary into the reader-owned
   tree;
2. mark that implementation boundary `external` in the model and record the
   symbols/signatures it provides;
3. remove the managed artifact from the next generation;
4. preserve the managed ABI needed by other facets;
5. fail model linking—not merely warn—when a later change makes the external
   implementation's declared ABI incompatible.

Ejection is irreversible by default because silently reclaiming ownership is
dangerous. A separate `jails adopt` may prove that the external bytes match a
generated artifact and bring it back under management.

Current generators may produce stubs intended for immediate editing. Those
edits remain merge-managed like edits to records. Ejection is optional and
boundary-scoped when the reader wants permanent ownership of a replaceable
implementation.

### Irreproducible outputs

Not everything belongs in the generated tree:

- Flyway migrations are append-only historical artifacts;
- a user build file may require a one-time source-root integration patch;
- secrets and machine-specific settings are never model output;
- external effects such as starting a service are not files.

The plan type makes these categories explicit. The merge-managed output is one
`PublishMergedTree` operation backed by the complete, already-reconciled tree
manifest. Migrations, the model file and rare reader-file patches remain
individual exact-image operations because their histories are irreproducible.

## A smaller transaction kernel

Do not replace the legacy journal with SQLite. Delete the object store, custom
codec, GC, receipt graph, WAL phases and roll-forward engine. The canonical
path needs only one project mutex, exact preconditions, atomic single-file
writes and one compiler lock containing the accepted model plus exact accepted
compiler projection.

The algorithm is deliberately convergent rather than rollback-oriented:

1. capture every input once under a project lock;
2. render the next compiler projection;
3. reconcile every file as BASE/OURS/THEIRS and refuse the entire command if
   any merge conflicts;
4. freeze the clean merged tree, migrations, reader patches, model update and
   their complete preconditions into one exact plan;
5. recheck all preconditions before the first write;
6. publish each exact after-image with a temporary sibling plus atomic rename;
7. write `.jails/compiler.lock.json` **last** as the acceptance marker and the
   BASE for the next generation;
8. verify the complete after-state, release the lock, then run explicitly
   idempotent follow-up effects.

Every file operation accepts both its captured before-image and its exact
after-image. Therefore a process death during publication does not require a
transaction log: before the final compiler-lock write, the old accepted
projection remains BASE and rerunning the same semantic command converges the
partially published tree to the same after-state. After the final lock write,
all reproducible operations have already landed. An unexpected third image is
a conflict, never a cue to guess or roll forward.

This trades invisible rollback for an explicit property users can understand:
a crashed command may leave a temporarily mixed but individually valid tree;
the next identical generation repairs it deterministically, and no later
command can accept it as a new baseline prematurely. Most history belongs in
Git. Database migrations remain forward-only evidence; their inverse is a new
forward migration, not filesystem rollback.

### Transaction defects the rewrite must cover

Do not build the new architecture on unverified transaction assumptions. The
transaction audit identified these paths for immediate tests or temporary
disablement:

1. External and machine preconditions are represented but not enforced in
   `jails-commit/src/execute.rs:646-738`.
2. Ledger observation performs independent content/metadata reads and may not
   describe one coherent snapshot (`store.rs:45-95`).
3. Normal execution promotes durable objects before committing the ledger,
   while recovery's roll-forward does not visibly perform the same promotion
   (`execute.rs:267-274`; `recover.rs:150-170`).
4. Recovery drops post-commit effects even though receipt validation requires
   exact agreement (`recover.rs:185-195`; `journal.rs:611-628`).
5. Advertised crash failpoints and actual trip sites do not match, and the
   crash loop does not assert that its requested point fired
   (`fault.rs:79-101`; `tests/crash.rs:139-177`).
6. Finalise and Abort default `ledger_after` to absence outside the Apply
   branch (`jails-prepare/src/pipeline.rs:779-839`).
7. Abort restoration can construct empty or missing replacement objects
   (`pipeline.rs:417-485`).
8. `Tree::join`/`inside` are lexical and need explicit `..` and symlink escape
   tests (`jails-support/src/apply/mod.rs:722-793`).
9. create/replace file contracts need race-safe create-new/existence semantics
   (`apply/mod.rs:95-119`).
10. activation currently treats broad metadata failures like absence
    (`jails-commit/src/activate.rs:40-69`).

These are high-value, bounded fixes regardless of which long-term design is
chosen. Where Finalise, Abort, conflict or generic tool paths have no real
caller, deleting or quarantining them is safer than preserving an aspirational
state machine in the trusted core.

## Write one compiler and several tiny mechanical generators

### 1. The application compiler

This is the pipeline described above: user declarations become a linked world,
typed facets, artifacts, schema evolution and one plan. It is business logic
and should remain readable, handwritten Rust around strong types.

### 2. Separate mechanical generators

`jails-protocol` is already a hand-written schema compiler. It contains
hundreds of codec implementations and paired encode/decode methods; the
`--bind` delta immediately added two more codecs. Request variants are repeated
across subject types, numeric tags, validation dispatch, codecs, maintenance
classification, JSON views and tests.

A small `wire_schema!`/derive can describe each versioned wire family:

```text
union Request @wire(version = 11) {
  1: Generate(GenerateRequest),
  2: Destroy(DestroyRequest),
  ...
}

record FieldSpec {
  1: id FieldId,
  2: type FieldType,
  3: optionality Optionality,
  4: constraints [Constraint],
}
```

Generate:

- stable numeric tags and canonical codecs;
- conversion skeletons between wire versions and semantic types;
- golden compatibility tests;
- exhaustive dispatch over variants.

Keep handwritten:

- business validation;
- reference resolution;
- reconciliation policy;
- compiler passes;
- migration safety decisions;
- document transformations.

Use a different `commands!` catalog for parse shape, aliases, canonical IDs,
help and completion. Keep semantic registries such as field-type behavior and
`LayerDef { id, default_package, heading }` as small typed tables whose
projections are derived locally. Wire layout, CLI syntax and semantic model do
not share one god-schema or version cadence.

Start with declarative macros or tiny checked-in generators. Do not design a
beautiful general-purpose metalanguage. Generated code should be inspectable,
deterministic and snapshot-tested. Old wire bytes remain golden fixtures, and
version migration is explicit rather than “bump one global codec number and
refuse the old world.”

### What should be dynamic

Dynamic data is useful for:

- recipe names, aliases, option metadata and help;
- simple capability bundles;
- custom logical-type metadata;
- external plugin request/response payloads;
- renderer/template selection.

It is a poor fit for core invariants such as safe rename, schema cutover,
ownership reconciliation and Java/SQL lowering. Encoding those in YAML or a
general rules language merely replaces Rust with an interpreter that has
weaker diagnostics. Compile declarative metadata into strong types; use a
typed Rust escape hatch for semantics that do not fit.

## Project and document adapters

### One true snapshot

The domain audit found three overlapping project views: legacy `Project`,
protocol `ProjectSnapshot`/`ProjectedProject`, and `ProjectContext`. The legacy
model calls itself immutable but methods such as `projected_text`,
`projected_sources` and `projected_names_in` still read the filesystem
(`jails-project/src/model/mod.rs:819-936`). The protocol snapshot correctly
refuses undeclared reads (`observe/snapshot.rs:529-607`).

Make a snapshot-backed `ProjectView` the only planner input. It exposes facts
and an overlay, never a root path or general-purpose file API. This is both
simpler and more correct: deterministic compilation, read-set validation and
cache keys all become consequences of the type boundary.

### One patch algebra

The project layer currently has legacy `Change`, protocol `DesiredChange`,
`SemanticEdit`, projected edits, diffs and prepared file operations. Replace
the middle of that chain with one ordered `PatchSet`:

```rust
enum DocumentPatch {
    PutManaged { artifact: ArtifactId, path: ProjectPath, bytes: BlobId },
    RemoveManaged { artifact: ArtifactId, path: ProjectPath },
    Maven(MavenPatch),
    GradleFragment(GradlePatch),
    Properties(PropertiesPatch),
    Compose(ComposePatch),
    JavaExternal(JavaPatch),
    AppendMigration(MigrationArtifact),
}
```

Each document backend parses once into facts and stable spans, validates a
typed patch, applies it and can derive an inverse where inversion is honest.
Retire and apply should not have mirrored match forests.

### Stop partially parsing arbitrary build languages where possible

`jails-project/src/gradle.rs` is roughly 1,500 lines and is effectively a
partial Groovy parser. A simpler contract is a one-time, reviewed
`apply from`/plugin anchor and a Jails-owned generated Gradle fragment. The
compiler then replaces its own fragment rather than surgically understanding
every Gradle program.

Maven does not have an identical include seam. At minimum, consolidate the two
custom XML scanners into one span-preserving Maven document backend. A more
radical build-extension prototype may own dependencies and generated source
roots, but it must be evaluated against Maven lifecycle/version behavior
before it becomes the default.

Use a truthful sum type such as `BuildModel::Maven` and
`BuildModel::Gradle`; do not store Gradle text in a field named `pom` or let a
Maven-flavored `Flavor` stand for the entire build world.

### Fix semantic naming collisions

Two protocol types named `RoutePath` currently describe different grammars:
an application route prefix and an endpoint pattern. Name them `RoutePrefix`
and `EndpointPattern`, backed by shared route tokens where appropriate. Similar
aliases that weaken `FieldId` to generic `Name` should be replaced with the
strong logical/physical identity used by evolution.

### Fail closed through one state adapter

`jails-state::compat` deliberately distinguishes absent, current and
unreadable state. `jails-project::generated_files` converts broad read/parse
failure to absence. Durable reads should all go through the former. “Could not
read” must never become “nothing exists” on a path that controls ownership or
recovery.

## Command architecture

There are currently at least three command oracles:

- Clap definitions in `src/cli.rs` and its submodules;
- canonical alias/path reconstruction in `src/cli/command_path.rs`;
- the pre-Clap plan parser in `src/plan_command.rs`;
- plus the semantic match in `src/main.rs`.

The manual path table already omits live nested commands including resource
index, migration lint, database console and test daemon. One command schema
should generate/own:

```text
parse shape
canonical command ID
aliases
help and examples
machine request schema
editor completion metadata
mutation/read-only classification
handler identity
fingerprint projection
```

Make plan import an ordinary command such as `jails plan apply FILE`. The plan
already carries its authenticated original request identity; inert argv should
not be reparsed before Clap to reconstruct it.

A smaller core CLI could converge on:

```text
jails init/new       compile a model into an empty tree
jails model check    parse, link and typecheck
jails plan           compile model/current workspace to an exact Plan
jails apply          execute an exact Plan
jails diff           show semantic and filesystem projections
jails eject          transfer one implementation boundary to reader ownership
jails history/show   render typed transaction views
jails schema import  turn live schema evidence into a ModelPatch
```

`g scaffold`, `g query`, `add db` and other friendly commands remain aliases
for typed model patches. They are not separate planners.

The editor surface should either become a real long-lived language server over
the compiler model, or become much smaller. The current command rescans and
hashes the project per request, walks a simplified Clap model, and offers
diagnostics too shallow to justify a separate pseudo-protocol. A model schema
can generate completion and diagnostics; a server can cache the captured
world.

The contract checker should likewise emit a real OpenAPI/contract IR from
facets before claiming comparisons over request, response and security
semantics. Today its emitted model is thinner than the comparison logic. Until
that gap closes, delegating comparison or removing unsupported scopes is
simpler and more truthful.

## Target dependency shape

Do not begin by choosing an aesthetically pleasing crate count. First delete
duplicate concepts; then let ownership form the boundaries. A plausible end
state is:

```text
jails-model
  semantic IDs, AppModel, ModelPatch and type semantics only

jails-contracts
  WorkspaceSnapshot/ProjectFacts, diagnostics, PlanDraft, DocumentIntent,
  exact Plan/TreeManifest/Blob DTOs; depends on jails-model

jails-wire
  generated versioned durable/wire DTOs and explicit adapters; depends on
  jails-model or jails-contracts only at the encoded boundary

jails-compiler
  link/validate, facets, type algebra, dependency graph, schema evolution,
  Java/SQL/HTTP emitters; depends on jails-model + jails-contracts

jails-workspace
  capture/adoption, document materialization, three-way merge, exact Plan
  executor and compiler-lock acceptance; depends on jails-model +
  jails-contracts

jails-cli
  composition root, generated command catalog, front ends and reporting;
  depends on model/contracts/wire + compiler + workspace

jails-tools-*
  optional run/test/db/log/editor/contract processes behind a versioned seam

jails-testkit
  shared test-only primitives
```

The dependency arrows are deliberately acyclic:

```text
                jails-model
                    ↑
             jails-contracts
               ↑           ↑
       jails-compiler   jails-workspace
               ↑           ↑
               └─ jails-cli ┘

jails-wire ──> jails-model / jails-contracts
jails-cli  ──> jails-wire
```

The CLI asks `jails-workspace` for one captured `WorkspaceSnapshot`, passes it
plus the canonical patch to `jails-compiler`, then asks the workspace to
materialize the returned draft into an exact `PlanBundle`. Preview and execute
consume that bundle. Compiler and workspace never import one another. Wire
types are not re-exported as the semantic model, and optional tools are
subprocess clients that do not import the mutation protocol.

These are ownership boundaries, not a demand for seven Cargo packages. Model,
contracts and wire can begin as private modules in one kernel crate as long as
they have one-way dependencies and no broad facade re-export.

The current thirteen crates may temporarily collapse during the rewrite because
moving a concept is easier inside one crate. That is not itself a success
metric. Five crates containing the same fifteen representations would be the
same architecture with fewer `Cargo.toml` files.

## Radical ideas worth prototyping

### A private Git object store

Instead of custom blobs, trees, diffs, merge bases, receipts and garbage
collection, Jails could keep a private bare repository under `.jails` and store
managed generations as commits. Git already supplies content-addressed blobs,
trees, history, diff, merge, integrity checking and GC.

This is much more attractive if editable managed files remain a requirement.
It does not solve arbitrary dirty/untracked workspace files, executable and
symlink policy, filesystem activation, database migrations or external
effects. Requiring the reader's worktree to be clean would simplify further
but would be a major UX change. Prototype it as a storage backend, not as an
assumption baked into the semantic model.

### Database-first authority

For database-heavy projects, a PostgreSQL catalog plus an append-only intended
migration stream could be the canonical schema input. Entity and repository
facets would project from it. This deletes some parallel migration/source
inference and makes live truth central.

The price is offline generation, portability and greenfield usability. A
safer compromise is a schema-import frontend that creates a `ModelPatch`,
after which the application model remains authoritative.

### External recipe/backend plugins

If extensibility becomes a real requirement, prefer a versioned subprocess or
Wasm boundary:

```text
ResolvedWorld + requested facet + ProjectFacts
    -> plugin
    -> typed ArtifactUnits + Requirements + Diagnostics
```

The core validates paths, identities and requirements before accepting output.
Do not expose transaction lifecycle hooks. A plugin compiles an artifact; it
does not participate in locking, recovery or ledger mutation.

This should follow a stable IR. Adding plugins while `Recipe`, `Change` and
prepared state are still in flux would make every accidental representation a
public compatibility promise.

### Full event sourcing

An append-only stream of `PlanCreated`, `OperationApplied`, `ModelCommitted`,
`EffectAttempted` and `EffectCompleted` can make history and recovery elegant.
SQLite tables plus an event column are a practical version. A custom event log
would recreate codec, migration, compaction and corruption problems.

Use event sourcing only if historical audit and replay become separate product
requirements. It is not part of this architecture: the current semantic model,
accepted projection lock and Git history are enough for the compiler workflow.

## What will not simplify Jails by itself

### A new JDL — SUPERSEDED by maintainer decision D2

*The conclusion below is overridden; the sequencing argument is kept and is now
D2's implementation order. See "Maintainer decisions" at the top and the
normative [JDL v1 implementation specification](jdl-sol.md).*

The original analysis: the CLI field syntax, generator vocabulary and two
application-manifest paths are already domain-specific languages, so a prettier
grammar removes no semantic duplication while adding a parser, spans,
formatter, migration, completion and documentation.

What survives that reasoning is the *order*, not the refusal. Define `AppModel`
and `ModelPatch` first and get the compiler working against a TOML front end.
Then add the grammar as one more front end compiling to the same model, which
keeps the language replaceable and testable.

What changes: the grammar is a required deliverable rather than a contingency,
because authoring ergonomics — concise leaves and constructs that nest with the
thing they describe — are a product requirement (D3), not a preference. TOML
loses that case on nested operations specifically, and no amount of key
flattening recovers it.

### Minijinja or another template engine

Loops and conditionals would shorten some render functions. They would also
move structural Java decisions into untyped text. They do not unify type
semantics, names, dependencies, lifecycle or schema evolution. Use a richer
template engine only behind typed artifact IR, and count template complexity
as code rather than declaring it deleted.

### A three-crate rewrite

Crate count affects navigation and compile boundaries. It does not eliminate
`Recipe`, `Declared`, `DesiredChange`, `SemanticEdit`, prepared identities,
journals or receipts. Temporary consolidation can make the rewrite easier, but
the goal is one representation per concept and a narrow dependency graph.

### “Render in a temp directory and atomically swap it”

This works for a wholly managed new destination and is why
`src/new/publish.rs` is good. It does not decide how hand edits merge with a
new projection, nor does it atomically replace arbitrary reader files in an
existing nonempty project across platforms. It is a publication technique,
not a replacement for artifact reconciliation or exact reader-file
preconditions.

### A fully dynamic runtime schema

Maps of strings and rule expressions make adding variants look cheap while
moving failures from the compiler to runtime. They are appropriate at plugin
boundaries, not for core ownership, schema and recovery invariants. Generate
strong Rust from a declarative schema instead.

### A full Java compiler in Rust

Jails needs deterministic emission and bounded, lossless edits to adopted Java.
It does not need overload resolution, bytecode generation or the Java type
system. One shared token/CST adapter—or a JVM backend—is enough. The existing
multiple partial scanners should converge, not expand into a language
implementation.

### Deleting the WAL because generated output is reproducible

Only the managed tree is reproducible. Model revisions, migrations and rare
reader-owned patches are not. Shrink the recovery domain first, then shrink the
journal. Do not weaken crash safety while the current broad mutation contract
still exists.

### LOC is not the limiting variable

AI can replace this volume quickly. The hard part is deciding which behavior
deserves to survive and proving the replacement preserves it. Do not make LOC
or elapsed time an architecture constraint. Freeze the contracts, shard the
rewrite aggressively, and let the differential suite decide whether the new
implementation is done.

## AI-native rewrite DAG

Do not port recipe by recipe or dual-write two transaction stores. Make the
few serial decisions once, freeze the old binary as the E2E oracle, fan the
implementation out across agents, integrate, cut over and delete.

### Serial root decisions

These contracts unblock every workstream and cannot be delegated to competing
interpretations:

1. choose one-shot generation or managed compiler/ejection;
2. declare model files the sole desired-state authority;
3. choose explicit stable IDs and the exact ejectable implementation ABI;
4. commit the managed generated tree and require `model check --frozen`;
5. freeze `PlanV1` as ordered exact-image operations;
6. freeze lock-last publication and deterministic crash convergence;
7. decide which CLI tool suites remain in the core product;
8. build and pin `jails-legacy` plus the differential E2E corpus.

### Parallel implementation lanes

Once those types are checked in, assign disjoint file ownership and run all
lanes concurrently:

| Lane | Delivers | Does not wait for |
|---|---|---|
| Model/linker | `AppModel`, explicit IDs, `ModelPatch`, reference graph, legacy-state importer, typed evolution programs | renderers or executor |
| Compiler kernel | facet IR, `TypeSemantics`, node-specific name projections, requirement graph, schema projection, `PlanDraft` builder | CLI or workspace implementation |
| Emitters | Java/SQL/HTTP/build emitters, split by independent facet families; existing templates reused where byte-stable | other emitter families |
| Workspace | one captured `WorkspaceSnapshot`, Maven/Gradle/properties/compose document backends, exact-plan materializer, managed-root integration | command frontends |
| Executor | exact preconditions, atomic after-image publication, lock-last acceptance, legacy receipt reader | generator internals |
| Frontends | command catalog, Clap, TOML/Serde model, direct CLI-to-patch adapters, plan/report encodings | emitter implementation details |
| Mechanical generation | wire codec derives, command metadata macro, local semantic registries | application compiler passes |
| Product split | external run/test/db/log/editor/contract commands and one versioned subprocess seam | compiler cutover |
| E2E firewall | twin-tree legacy/new runner, semantic normalizers, strict toolchain lane, child-process crash matrix, real-project corpus | implementation lanes |

Each lane compiles against the frozen interfaces and lands only when its G0–G5
gates pass. A representative spike (`record`, `query`, `add db`) tests that the
interfaces are sufficient; it is not an incremental production path. Once
validated, agents port all remaining facet families in parallel.

### Integration and one coordinated cutover

1. Link all lanes into a separate `jails-next` binary.
2. Run every old/new differential scenario, strict generated-project build,
   protocol fixture, evolution case and crash point.
3. Run one-way import over copies of real schema-1 projects and compare plans
   and builds.
4. Make intentional behavior changes explicit in semantic expectations; never
   mass-refresh goldens to force green.
5. Switch the `jails` entry point and on-disk schema once all gates pass.
6. Keep only a read-only legacy state/receipt importer and the frozen old
   binary fixture needed by compatibility tests.
7. Delete the old engine, protocol translations, render paths and transaction
   store in the same cutover change.

There is no shadow production engine or long dual-write period in this plan.
Parallel AI throughput handles the volume; the E2E firewall handles the risk.

## What `new` has to become first (2026-08-30, measured)

**The cutover's first step is not deleting anything. It is `new`.**

`model_command::owns` is the whole canonical switch -- a project is canonical
if `.jails/model.jdl` exists and legacy otherwise -- and `jails new` seeds no
model, so every project jails creates is legacy and every project that is
canonical got there by a `model.jdl` written by hand, which in practice means
the tests and `model import`. Seeding one from `new` looks like the small
commit that flips the default. It was tried, and it is not:

- **`new-cli` + `add fake` refuses.** `could not materialize exact plan: Maven
  already declares 'org.assertj:assertj-core' outside
  `<!-- jails:dependencies -->``. `new-cli` hand-writes a pom carrying AssertJ
  and JUnit; the canonical dependency adapter reconciles the complete set
  inside its marked block and refuses to adopt a coordinate declared outside
  it.
- **`new` (Spring) + `add db` refuses.** `reader-owned properties already
  declare `server.shutdown``. `write_default_properties` hand-writes six keys,
  two of which -- `server.shutdown` and
  `spring.lifecycle.timeout-per-shutdown-phase` -- the `db` capability also
  declares. Even `jails set server.shutdown=graceful` refuses on a project
  jails created seconds earlier.

Neither adapter is wrong. Refusing a reader-owned collision is what stops jails
silently taking over a line somebody else wrote, and the legacy property splice
claiming such a key idempotently is exactly the looseness the canonical
contract exists to remove. What is wrong is the premise: **`new` hand-writes
build content that the canonical model expects to be the authority on.** A
seeded model turns every one of those bytes into a reader-owned collision with
its own project.

So this is the deletion map's `new` generation paths row -- *compiler
`PlanDraft` plus workspace materializer* -- and it is a precondition of the
cutover rather than a consequence of it. Two things it needs that are not
there yet: the pom and `application.properties` a new project starts with have
to be compiler output rather than `new`'s own text, and the canonical sync has
to accept an explicit root. `model_command::sync` reads the process CWD, which
for a project being created is its parent -- the same edge `plan.md` §R6.5 hit
with `--app`, and the reason every route already takes a resolved `Project`
instead of calling `discover`.

**What did hold, and is worth knowing before that work starts:** a plain
project whose `.jails/model.jdl` is seeded *and* whose pom collisions are
avoided runs the whole canonical loop. `g record`, `g field`, and `add csv`,
`add json` and `add testkit` all applied model patches, wrote to
`.jails/generated`, created no legacy ledger, and `mvn clean verify` passed on
the result. The compiler is ready for this; `new` is what is not.

## What "39 of 39" does not say (2026-08-30, measured)

**A generator being on the canonical path means it has a backend, not that its
backend does what the legacy one did.** `canonical_support::registry_classifies_
every_advertised_word` counts backends, and it is right to; it is not a parity
gate and nothing else is one either. Probing found two gaps that every existing
test agreed was fine:

- **`g strategy` refused on a plain project.** `refuse.rs` grouped `Strategy`
  with `Service` and `Controller` and rejected all three without Spring, while
  `CLAUDE.md` records that "plain-Maven projects get the same layout with no
  annotation" and the legacy generator has always emitted one there. Fixed.
- **`g record`, `g value` and `g enum` emitted no companion test.** The legacy
  path writes `<Name>Test.java` beside every one of them, `@Disabled` and
  naming the component when a sample cannot be built -- `CLAUDE.md` is explicit
  that "emitting nothing would silently drop coverage". Fixed:
  `emit_companion_test` writes all three shapes, on JUnit's own assertions
  rather than AssertJ, because a canonical project is not guaranteed to declare
  AssertJ and a generator that drags in a dependency for a file the reader did
  not ask for is the plumbing this tool exists to remove.

**Both were invisible to the differential gate, and the reason is worth
keeping.** G1 compares what it was told to compare -- for the iterative record
scenario, the record's own source file -- so an artifact the canonical side
never writes is not a difference it can see. A differential suite proves the
files it names agree; it says nothing about a file only one side produces. The
same blind spot hid `g strategy`, whose plain-project shape no scenario runs.

So the cutover needs a parity gate that is not a spot check: for each advertised
generator, on each fixture shape, the *set* of artifacts both engines write
should be compared before their bytes are. That is a cheaper thing to build
than the deletion it protects.

## Concrete deletion map

| Current area | Destination | Eventual deletion/simplification |
|---|---|---|
| `jails-spec::Field` plus protocol `FieldSpec` | one model type registry and renderer views | old field derivation/translation tables |
| repeated `Layer`, route and name tables | small typed registries with derived projections | synchronized enum/label/package tables |
| `Recipe` + `refuse_misplaced` + giant artifact match | typed declarations, recipe metadata and compiler passes | optional-field bag and negative flag matrix |
| `Recorded`, `Declared`, `Asked` | `ModelPatch`, canonical request identity and `Plan` | manual recipe reconstruction and syntax copies |
| generated Java/SQL reparsing | adoption importer plus `AppModel` | source-as-database normal paths |
| `Change` + `DesiredChange` + `SemanticEdit` + projected edit forests | one semantic `PatchSet` | mirrored apply/retire and translation layers |
| desired/observed/applied/pending record variants | canonical records plus `StateDelta` | phase-specific row duplication |
| hand codecs/tags/JSON serializers | generated wire/report schemas | repetitive encode/decode/match code |
| project `Project`/`ProjectContext`/snapshot overlap | snapshot-backed `ProjectView` | post-capture arbitrary disk reads |
| Gradle partial parser | managed fragment/plugin anchor | most arbitrary Groovy rewriting |
| duplicate Maven XML scanners | one document backend | second scanners and field-name lies |
| per-byte dependency inference | typed IR requirements | `with_test_support`-style scans |
| route-level rename/field evolution replay | stable IDs + semantic schema diff | whole-generator reruns and hard-coded companions |
| prepare identity/semantics/prepared representations | one canonical `Plan` | agreement methods and vector cloning |
| journal/receipt/object directory protocols | lock-last convergent exact-file executor plus compiler projection lock | bespoke storage, codec, GC, WAL, receipt and roll-forward machinery |
| `main.rs` dispatch + command-path + pre-Clap parsing | generated command catalog | duplicated command oracles |
| `new` generation paths | compiler `PlanDraft` plus workspace materializer; keep useful `Publication` mechanics | manual preview path lists and nested engine state |
| drive/report/tool suites | optional command processes or explicit core modules | facade coupling and duplicated runners/caches |

## Architecture fitness rules

Add tests for properties that express the new shape, not file-size thresholds:

- after capture, no compiler module can access `std::fs`, a project root or a
  process runner;
- identical `WorkspaceSnapshot + CanonicalModelPatch + CompilerVersion` yields
  an identical plan digest;
- preview, plan export, confirmation and apply reference the same digest;
- every command and alias resolves through one generated catalog;
- every builtin type has one semantics row;
- every artifact requirement comes from IR, never a content/path scan;
- managed output is written only below the managed root;
- reader-owned source is changed only by an explicit typed patch/eject/adopt
  operation;
- every persisted union tag and field number is generated and golden-tested;
- every advertised failpoint fires in at least one test;
- every active transaction state has one tested recovery transition;
- the planner's read set is complete by construction, not inferred after the
  fact;
- optional tool crates cannot import mutation executor internals.

Avoid gates that assert only a maximum file length or a minimum scanner count.
Those improve navigation but can be satisfied while every duplicated concept
survives.

### What a run of G0 actually does (2026-08-30)

**G0's whole claim is one answer to "is this green", and measured over seven
runs on the developer machine it gave two answers.** Two runs in six failed,
from two unrelated causes, neither in the product. A gate that is green two
times in three is worse than a slow one, because the third answer is
indistinguishable from a real regression and teaches the reader to re-run
rather than to look. Both causes are fixed below.

Timing first, since the cutover needs a machine that can afford to prove it.
Sixteen logical CPUs, 30 GB, full toolchain:

| phase | cold | warm |
|---|---|---|
| `fmt --check` + `clippy --workspace --all-targets` | 83s | ~5s |
| test-harness compilation | ~100s | ~6s |
| test execution, 33 binaries, 16 at a time | 130-155s | 130-155s |
| **whole gate** | **259s** | **139-148s** |

1156s of CPU against 139s of wall, so ~8.3x parallel. `cli` is the entire
critical path at 130-136s; the other 32 binaries finish inside its shadow
(`engine` 23s, `architecture_allowances` 20s, `differential` 15s).

The two causes:

- **The fixture corpus filled `/tmp`, and the age rule could not see it.**
  Every fixture is `keep()`d so a failure can be inspected, and the sweep only
  collected what was over an hour old. One run leaves ~1,900 directories and
  1.4 GB; six fit inside that hour. The seventh died with **580 `No space left
  on device` panics, every one in a test that was working**. Fixed by bounding
  the corpus by count as well as age, with a floor that keeps the age rule's
  promise that a concurrent run's fixtures are never touched. The policy is a
  pure function now, because a `Once` over the real temporary directory is not
  something a test can drive.
- **A fifty-millisecond settle window that was not long enough.** Five
  `tests/engine.rs` tests failed at once, each unable to re-acquire a lock it
  had already released, each reporting *its own* previous command as the
  holder -- `read_best_effort` reads a file that is deliberately never deleted,
  so it names the last writer rather than the current one, and that read as a
  process blocking itself.

  `lock.rs` already documented the cause: a `fork` copies the descriptor table,
  so a child holds every lock the parent had open until `exec` drops them. What
  was wrong was the bound. The window is not the length of a `fork`/`exec`; it
  is how long the child takes to *reach* `exec`, which on a loaded machine is a
  scheduling question. Measured against a genuine `fork` with a child that
  delays before `exec`, the re-acquire delay tracks it one-for-one: 2.2 ms at
  no delay, 12.6 ms at 10 ms, 53.4 ms at 50 ms, 203.2 ms at 200 ms. One `fork`
  captures every lock held at that instant, which is why five tests failed
  together rather than one. `SETTLE` is 500 ms now, with that table in the
  comment.

  **Two earlier measurements looked like refutations and were not**, which is
  worth recording because both were the same mistake: polling 1500 ms past the
  deadline freed zero locks, and a direct reproduction showed 0.0 ms over forty
  trials. The first ran over two runs that never hit the flake, so it measured
  the deliberate contention tests instead. The second used
  `subprocess.Popen`, which glibc implements with `vfork` -- the parent is
  suspended until `exec`, so there is no window to observe. A negative result
  from an instrument that cannot see the effect is not evidence.

**This matters to the cutover specifically.** Step 5 of *Integration and one
coordinated cutover* is "switch the entry point once all gates pass", and the
change it gates deletes most of the workspace. A one-in-three false red is
exactly the condition under which a real regression gets re-run until it goes
away.

### Where the gates stand (2026-08-30)

- **G0** — `mise run verify-rewrite`; `.githooks/pre-push` and
  `.github/workflows/verify-rewrite.yml` invoke it and nothing else, so hook,
  CI and `CLAUDE.md` cannot disagree about what passing means. Its
  `cargo build --workspace` was removed with a measurement: it built nothing
  the suite did not build anyway, and was a barrier between two halves of one
  compile graph.
- **G1** — `tests/differential.rs`, 44 scenarios across both implementations,
  plus five checked-in foreign projects under `tests/corpus/` run through both
  binaries. Green here, which it was not until `--diff-algorithm` stopped being
  passed to `git merge-file` unconditionally.
- **G2** — the inventory half was already held
  (`feature_inventory_covers_the_live_clap_tree_exactly_once`); the journey
  half was not, and now is:
  `every_inventoried_command_path_is_invoked_by_a_test` maps 99 of 109 live
  command paths to a test. The ten that are not are `kafka *` and
  `test daemon *`, named with their reason -- each drives a broker inside a
  compose container or a resident JVM over a socket, so a journey means a
  tier-3 fixture nobody has written. The gate fails in *both* directions:
  coverage may not fall, and an exemption that is no longer needed must come
  off, because one left in place hides the next command that loses its
  journey.
- **G3** — `tests/common/scenarios.rs` is the map, held by
  `every_kind_and_capability_has_a_golden_scenario` against the binary's own
  help.
- **G4** — **done.** `failpoints!` is the one declaration: it emits `POINTS`,
  the list a crash test enumerates, and one constant per point, which is the
  only thing `trip` accepts. Both silent failures the hand-written pair had
  are now compile errors rather than a source-scanning test. A point nobody
  trips has an unused constant, and `-D dead-code` fails the build -- that is
  a fault which could never fire, proving a recovery path nothing exercises. A
  point tripped but unadvertised cannot be written at all, because every
  constant is in `POINTS` by construction.
  `a_capability_install_converges_from_every_failpoint` is still the sweep,
  and one property stays a test because a macro cannot state it: the wire
  names must be distinct, since two points sharing a string would fire
  together and prove a recovery path with the wrong fault.

  **And G4 now covers the executor that is staying**, not only the kernel
  being deleted. `crates/jails-workspace/tests/crash.rs` declares nine points
  over the canonical publication sequence and asserts a *different* property,
  because there is no journal to roll forward: re-running the same bundle
  after a death at any instant reaches byte-for-byte the tree a clean run
  reaches, and a further run writes and deletes nothing. The last clause is
  the half a "the second run fixes it" claim usually skips -- an executor that
  rewrote everything every time would satisfy convergence forever and still be
  unable to tell a reader whether anything had changed.

  **The aborting half earned its cost immediately.** The in-process matrix was
  green; the child-abort matrix was not. An injected `Err` unwinds, so the
  staged `NamedTempFile`'s guard removes it, and a crash between staging and
  rename looked survivable. An `abort()` leaves the temporary on disk, where
  `verify_preconditions` reads it as *an unmanaged file appeared inside the
  managed tree* -- and refuses, permanently, because nothing removed it and
  every later plan refused the same way. A project wedged by jails' own
  temporary file is the exact opposite of the sentence this executor trades
  rollback away for. `execute::sweep_staged` is the fix, and the prefix is
  `.jails-staged-` rather than `tempfile`'s `.tmp` so that the only thing in a
  project which looks like a reader's file and is not says whose it is.
- **G5** — the proof manifests are promoted: `examples/proof-policy.tsv` is
  enforced by `tests/cli/examples.rs`, and
  `example_manifest_policy_covers_every_checked_in_manifest` stops one being
  added without a tier. The `validation/` workouts were *not* promoted and had
  rotted: their state table was two weeks stale and the scripts had run
  against no gate. Measured on 2026-08-30, **eight of ten now have zero real
  failures**, against a table claiming 9/6/13/18/23/10/11/25/25.

  Four of the failures were the *workouts* being stale rather than jails
  missing anything -- each was a refusal jails has grown since they were
  written: `from`, `to`, `offset` and `limit` are PostgreSQL reserved words; a
  record called `Override` shadows `java.lang.Override` inside its own package
  and compiles while meaning the wrong thing; and one workout ran `g repo` on
  a name it had never declared. That inverts the README's own premise, which
  now says so.

  The nine that remain are one product question, not nine bugs: `g repo` emits
  a single `Jdbc<Name>Repository` whatever the dialect, and these expect the
  dialect in the name. Left failing rather than renamed away.

  **The sanitized corpus now exists and has five entries**: `tests/corpus/`
  holds checked-in project trees jails did not write, with `policy.tsv`
  accounting for every one, and
  `every_corpus_project_is_treated_the_same_by_both_implementations` runs each
  through both binaries. Between them they cover all three of `adopt`'s rules
  and both build systems: a layer nested two deep, a project with no layers at
  all, two known synonyms beside one unknown name, two candidates for one layer
  (neither written, both named, and the unambiguous sibling still recorded),
  and a Kotlin-DSL Gradle build with no `pom.xml` at all.

  **Widening the corpus widened the harness first.** A row carried one
  expectation, so an entry could state a quarter of what it was checked in to
  prove and the rest lived in the prose column, where nothing reads it. A row
  is a `;`-separated list now, and `reports:` asserts both halves of "reported,
  never guessed at" -- named in the output *and* absent from `[layout]` --
  because naming a directory and then recording it anyway reads as diligence
  and behaves as a coin toss.

  One finding, and it is a refusal rather than a defect: `core` is not a
  synonym for `domain`, and should not become one. It is a name real projects
  use constantly and it means the domain model in one codebase and shared
  framework glue in the next, so it fails the table's own bar on the second
  half rather than the first -- common is not the test, unambiguous is.
  `spring-renamed-layers` pins the refusal so nobody adds it on a guess.

  The point of bytes over the existing Rust
  fixture is that it grows without a Rust change -- a corpus only a Rust
  programmer can extend is not a corpus.

  **It found a real defect on its second entry**, which is the argument for
  having one. `jails adopt` read only the *first* package segment, so a class
  in `infra/jdbc` was adopted as `adapters = "infra"` -- the grandparent,
  which holds no Java at all. Every later command would have been pointed at
  an empty tree and nothing would have said so, because `Config::layers()`
  honours a nested layout perfectly well and had no way to know that one was
  invented. It contradicted adopt's own comment, three lines above the bug,
  saying the walk exists to stop exactly that. Fixed: the whole
  package-relative directory is recorded, the synonym table reads its leaf,
  and an unrecognised leaf is reported by name rather than guessed at.

  Still open: promoting the workouts to a *gate* needs a JDK matching
  `TARGET_RELEASE` and the `stacks/fixtures/` checkout -- every workout's two
  environmental failures are those two things -- and the corpus wants more
  entries, which is now somebody dropping a directory in rather than a change
  to this suite.

**G1 and the engine suite are green here now.** They used to show 8 failures,
all `git merge-file` exit 129 -- git 2.43 against the `--diff-algorithm` flag
jails passed unconditionally, which is a usage error rather than a merge
outcome. `jails_support::git` asks the machine instead, and
`JAILS_GIT_DIFF_ALGORITHM` pins one answer for a team whose machines differ.
What is still red here is the JDK, and any measurement taken here is a
measurement of the other tiers.

### Where each fitness rule stands (2026-08-30)

Two were enforced by a *type* rather than a test, which is better and is why
they had none; three were genuinely unheld and now are.

| rule | held by |
|---|---|
| compiler pure after capture | `rules::canonical_compiler_is_pure_after_capture` |
| identical snapshot/patch/version ⇒ identical digest | **added:** `materialize::the_same_snapshot_patch_and_compiler_produce_the_same_plan_digest`, plus the two negatives -- a different compiler version and a different patch are different plans |
| preview, export, confirmation and apply name one digest | **added:** `preview_export_and_apply_all_name_one_plan_digest` |
| one generated command catalog | `commands.rs` walks the live `clap::Command`; `every_command_a_message_tells_the_reader_to_run_is_one_that_exists` checks messages against it |
| every builtin has one semantics row | `BuiltinSemantics` is one exhaustive match, and the ladder's *largest table of per-builtin knowledge outside its row* keeps a second one from growing -- it is what sent `alternate` and `json` onto that row this week |
| requirements from IR, never a content scan | structural: the canonical crates cannot reach the filesystem, and no production line inspects rendered bytes to decide a requirement |
| managed output only below the managed root | structural: `RenderedTree::insert` refuses a path outside its root, so it cannot be violated |
| reader-owned source only via typed patch/eject/adopt | `rules::canonical_workspace_has_one_mutation_owner`, plus `PatchReaderFile`'s captured before-image |
| persisted tags and field numbers golden-tested | `every_protocol_fixture_is_read_by_something`, `tests/protocol-golden` -- including the canonical side: `compiler-lock-v2.json` for the accepted state and `plan-bundle-v1.json` for `jails.plan.v1`/`jails.plan-bundle.v1`, the document a reviewer confirms and the executor applies |
| every advertised failpoint fires in a test | the compiler, not a test: `failpoints!` emits both the registry and the constants, so an untripped point is unused (`-D dead-code`) and an unadvertised trip site cannot be written. `engine::a_capability_install_converges_from_every_failpoint` sweeps the legacy kernel and `jails-workspace/tests/crash.rs` the canonical executor, each asserting its own point actually tripped |
| every transaction state has a recovery transition | `recover.rs`'s own tests plus that sweep. **This one is not audited to the letter**: the coverage is by failpoint, and "every active transaction state" is a stronger claim than "every armed fault converges" |
| planner read set complete by construction | `WorkspaceSnapshot` is the read set, and the purity rule above is what makes it complete |
| tool crates cannot reach executor internals | Cargo, plus `no_module_depends_on_a_layer_above_its_own` for module edges |

**One rule this list did not have, added because it cost three separate
hand-written assertions in one afternoon:** no generated Java may carry an
unsubstituted `{{placeholder}}`. `{{name}}` is the placeholder syntax
*because* no `{{` appears in any Java jails writes, so one that survives is
always a key the renderer was not given -- a file that compiles nowhere, which
the golden bytes then record as if it were intended. It is checked over the
whole golden corpus at once (`no_generated_java_carries_an_unsubstituted_placeholder`),
with a floor on the files examined, since a scanner that has lost the corpus
reports exactly what a clean one does. Java only: a generated `.http` file
uses `{{baseUrl}}`, a workflow writes `${{ github.ref }}`, and a compose
healthcheck reads `{{.Config.User}}` -- all three the file format's own syntax
rather than jails'.

## Final recommendation

The crazy idea that fits the evidence is not “write a new language.” It is:

> Treat Jails as a compiler whose source is one application model, whose
> generated tree is merge-managed by stable artifact identity, whose
> irreversible output is an explicit evolution plan, and whose executor
> applies exactly the plan the reader reviewed.

Build the application compiler plus separate wire-codec and command-catalog
generators, with [JDL v1](jdl-sol.md) as its single durable authoring language.
Keep the current syntax as compatibility front ends. Use typed
facet and artifact IR rather than strings, generated source as a projection
rather than semantic state, explicit stable IDs rather than name inference,
and a small lock-last, convergent exact-plan executor instead of the legacy
object store, codec, journal, receipts and roll-forward state machine.

The largest code deletion will not come from shorter render functions. It will
come from making these questions disappear:

- Which of six representations is authoritative?
- What facts can be recovered from the Java we emitted?
- Which recipe kinds happen to depend on this field?
- Is this edited file still ours?
- Did preview and apply run the same computation?
- Does recovery reproduce every side condition of normal commit?

One model, one graph, one plan and one explicit ownership transfer answer all
six.
