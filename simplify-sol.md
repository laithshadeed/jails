# Simplifying `jails`: make the hidden compiler explicit

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

There is one product decision that no refactor can evade:

- If generated files remain freely editable while Jails must later update,
  rename and delete them without relying on Git, most ownership, merge, object
  store and recovery machinery is intrinsic.
- If generated files are disposable until explicitly **ejected**, the managed
  tree becomes a pure compiler output. Most of that machinery disappears.
- If Jails becomes a Rails-style one-shot generator, even more disappears, but
  so do declarative apply, safe destroy, model-driven evolution and undo.

My recommended destination is the second model: a **managed application
compiler with an explicit eject escape hatch**—conditional on declarative
apply, safe evolution and later destroy being product requirements worth
keeping. If those are not essential, the honest one-shot generator is simpler
and should win. Keep the current CLI and `app.toml` as front ends initially. Do
not invent JDL until the model and IR have proved themselves.

## The new vision, in one page

**Jails becomes an application compiler, not a file-aware generator.** This
vision assumes Jails is meant to keep a declared application evolving. If its
job ends after scaffolding, stop here and build the one-shot option instead.

The source of truth is one versioned application model. The current CLI stays
pleasant, but every mutating command is only syntax sugar for a `ModelPatch`.
The compiler resolves that model once and produces one immutable plan. Preview,
confirmation, export and apply all use that exact plan. Reproducible source is
written to a wholly managed generated tree. Reader code lives outside that
tree. When a reader must take over generated code, `eject` transfers ownership
explicitly and permanently.

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
          -> ONE Plan {
                 next model,
                 managed tree,
                 append-only migrations,
                 rare reader-file patches,
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
- managed output needs no per-file three-way ownership merge;
- preview and commit cannot accidentally plan twice;
- wire tags/codecs/command metadata are generated from one domain schema;
- most transaction storage protects only model, migrations and rare external
  patches, not every reproducible generated byte;
- run/test/db/editor/contract utilities stop expanding the compiler kernel.

### The irreducible kernel

The replacement has four concepts:

1. `AppModel`: what the application means.
2. `Compiler`: pure model/project-facts to typed artifacts and evolution.
3. `Plan`: the exact, reviewable state transition.
4. `Executor`: lock, recheck, stage, apply and recover that plan.

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

Do not compare new SQLite bytes with old journal-directory bytes. Compare a
canonical `StateView`/`ReceiptView` describing their semantics. Intentional
changes require a checked-in expectation or migration rule; an agent may not
silently refresh every golden.

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
- **G2 — behavior journeys:** all 98 command paths map to at least one
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
through HEAD `a229087e9113c9e1daf11383a5c6feace6d05e43`. While this document was
being written, a concurrent uncommitted `--bind` change added one protocol file
and touched the engine, generator and root CLI. That complete Rust delta was
read separately. The final inventory below is the live filesystem after that
delta, not an inference from the module graph:

| Scope | Rust files | Raw lines | Main concern |
|---|---:|---:|---|
| `crates/jails-engine` + root `src` | 66 | 18,973 | orchestration and CLI |
| `jails-generate` + `jails-java` | 64 | 27,952 | lowering and rendering |
| `jails-protocol` + `jails-project` + `jails-spec` + `jails-state` | 87 | 39,221 | domain, wire values and project state |
| `jails-commit` + `jails-prepare` + `jails-drive` + `jails-report` + `jails-support` + `jails-testkit` | 83 | 36,511 | planning, transactions and tools |
| **Total** | **300** | **122,657** | **96,567 nonblank code lines** |

The totals include colocated tests and should not be read as 96,567 lines of
production logic. They are useful for scale and coverage, not as a productivity
metric.

Every baseline Rust file was inventoried and assigned to one of four audits;
the new file and every modified Rust hunk that appeared afterward were then
read directly. Findings are grouped by responsibility below instead of
repeated as a 300-row filename dump. The codebase graph was used first for
structure, call paths and hotspots. Its project matched the committed HEAD,
but exact coverage checks still reported many files as metadata-changed or
absent from the recorded generation. A clean graph coverage result means no
*recorded* gap, not proof that an exhaustive query is complete; the filesystem
inventory and source reads are the authority for the exhaustive statements
here. The final coverage call for the newly appeared delta was attempted twice,
but the graph transport had closed; those paths are therefore qualified by
direct source inspection rather than fresh graph coverage.

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
reader-owned. Safe later mutation requires retained identity, owners,
preimages, current images, merge policy and human confirmation.

The current ledger, object blobs, three-way merge, force policy, receipts and
undo are different answers to that one ambiguity. Deleting them without
changing the ownership contract would make Jails smaller by making it unsafe.

The way out is not a cleverer merge. It is a state transition users can
understand:

```text
managed facet --jails eject--> reader-owned source
```

Before ejection, Jails may replace the file or entire generated tree. After
ejection, it never rewrites or destroys that source. A model change may report
that an ejected implementation is stale, but the compiler does not pretend to
own it.

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

This is the most important place not to confuse deletion with simplification.
Arbitrary files in an existing directory cannot all be atomically replaced as
one operation. A crash-safe multi-file mutation needs a lock, staged bytes,
preconditions, a durable record of intent and idempotent roll-forward. Moving
the staging directory beside the project or using SQLite does not repeal that
filesystem fact.

What can be simplified is the number of meanings carried through the kernel.
Preparation currently has parallel resource, operation, ledger and effect
representations. Commit stores its own journal and receipt forms, object
images, preconditions and recovery state. This creates enough surface for the
normal and recovery paths to drift. The concrete defects and the tests they
need are collected under “Transaction defects the rewrite must cover” below.
The architectural point is that mirrored paths no longer obviously mirror
each other.

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

### B. Managed compiler with ejection — recommended

The application model is source. Managed Java, tests, HTTP collections and
reproducible configuration are compiler outputs. Readers extend generated
ports from ordinary source and explicitly eject a generated facet when they
need to own its implementation.

Schema migrations are not reproducible output: they are append-only history
events produced by a semantic model diff. Rare patches to reader-owned build
or configuration files are also explicit plan operations, not ordinary
renderer output.

This keeps declarative apply, preview, safe evolution and deterministic
generation while removing the ambiguity that drives most merge and ownership
code. It is a product change, especially for users accustomed to editing a
generated class in place. That change must be prototyped with real projects
before the legacy engine is deleted.

### C. Keep editable managed files and refactor in place

Typed IR, one model and one plan would still improve the system substantially.
However, retaining “edit this output and let Jails later merge/delete it” also
retains ownership records, content blobs, three-way merge, conflict policy and
multi-file crash recovery. Expect a cleaner implementation, not a small one.

Choose this only if in-place editing plus later management is a defining
product requirement. It is internally coherent; it simply has an irreducible
cost.

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
blob. SQLite may retain the base digest, exact plan bytes and historical model
images for recovery/audit, but those are observations—not another editable
source. A CLI `ModelPatch` includes the resulting model-file bytes in the plan.
Manual edits simply change the next captured digest.

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
    ReplaceManifest(AppModel),
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

Typed IR need not model every token of Java on day one. Start with types,
imports, annotations, declarations, statements and expressions that current
generators compose dynamically. A checked whole-file template remains fine
for a stable leaf. The test is whether semantic code is passed around as a
string and later inspected or mutated.

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

### Pass 6: emit and form one immutable plan

```rust
struct CompileOutput {
    next_model_file: BlobId,
    generated: GeneratedTree<BlobId>,
    migrations: Vec<AppendOnlyArtifact>,
    owned_patches: Vec<OwnedPatch>,
    follow_up_effects: Vec<Effect>,
    diagnostics: Vec<Diagnostic>,
}

struct Plan {
    id: PlanId,
    compiler: CompilerVersion,
    base: SnapshotPreconditions,
    input: CanonicalModelPatch,
    output: CompileOutput,
    digest: PlanDigest,
}
```

Human review, JSON output and portable serialization are projections of this
one `Plan`. Applying it first rechecks `base`; it never reparses argv or calls
the route that originally compiled it. A stale plan is rejected. A caller may
explicitly request a recompile, but that produces a new digest and a new thing
to review.

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
Gradle each need one stable integration that adds those roots. The exact path
is a prototype question; the ownership property is not.

Choose one primary build contract rather than leaving regeneration ambiguous:
**commit the managed generated tree**. Maven/Gradle receive a one-time source
root, but no Jails plugin is required in every IDE/build. CI runs
`jails model check --frozen` and fails when committed output is not the exact
projection of the model and pinned compiler version. Generated diffs remain
reviewable, while ownership is still absolute because the whole subtree is
managed. An optional build plugin can be explored later; ignored build-time
output is not the default vision.

Reader code lives only in ordinary `src/main/java` and `src/test/java`. It
depends on generated ports/types or supplies implementations through explicit
extension points. Compiler output never performs identifier surgery in that
tree as part of normal operation.

### Ejection

Ejection is allowed only at a declared **implementation boundary**, not an
arbitrary facet or file. A boundary has a generated ABI—ports, DTOs and
signatures that stay managed—and a replaceable implementation artifact. One
file cannot be half managed by two boundaries.

`jails eject <implementation-id>` should:

1. materialize the selected generated source into the reader-owned tree;
2. mark that implementation boundary `external` in the model and record the
   symbols/signatures it provides;
3. remove the managed artifact from the next generation;
4. preserve the managed ABI needed by other facets;
5. fail model linking—not merely warn—when a later change makes the external
   implementation's declared ABI incompatible.

Ejection is irreversible by default because silently reclaiming ownership is
dangerous. A separate `jails adopt` may prove that the external bytes match a
generated artifact and bring it back under management.

Some current generators produce deliberately empty stubs intended for
immediate editing. Under the new model those should either be born ejected or
be replaced with generated interfaces plus reader-owned implementation
templates. Generating a managed class whose purpose is to be edited recreates
the paradox.

### Irreproducible outputs

Not everything belongs in the generated tree:

- Flyway migrations are append-only historical artifacts;
- a user build file may require a one-time source-root integration patch;
- secrets and machine-specific settings are never model output;
- external effects such as starting a service are not files.

The plan type makes these categories explicit. The executor can retain a
small journal for migration allocation and rare reader-file patches without
journaling thousands of disposable generated files.

## A smaller transaction kernel

SQLite is a good implementation option once the semantic collapse has
happened, not a substitute for it. A `.jails/state.sqlite` database could
replace the custom metadata codecs, object directory, GC bookkeeping and
several ledger/receipt files with a few normalized tables:

```text
model_observation(plan_id, before_digest, after_digest)
plan(id, payload, digest, status)
operation(plan_id, sequence, kind, path, before_digest, after_digest, status)
blob(digest, bytes)
effect(plan_id, sequence, kind, idempotency_key, status, detail)
```

The algorithm remains conservative:

1. acquire one project lock;
2. roll forward an unfinished plan;
3. validate the plan's snapshot preconditions;
4. persist the exact plan and every after-image blob with status `Prepared` in
   one `synchronous=FULL` SQLite transaction;
5. stage each blob beside its target, sync the file and its staging directory;
6. for each ordered operation, inspect the target: a before-digest means apply
   the exact after-image; an after-digest means the earlier activation already
   succeeded; anything else is a conflict;
7. after rename/create/delete, sync the target and parent directory, then mark
   the operation applied;
8. once every operation is observably at its after-state, mark the plan and
   model-file observation committed;
9. release the lock;
10. run retriable follow-up effects and record their outcomes.

SQLite makes metadata atomic and queryable. It does **not** remove the
filesystem journal or its crash window; it implements that journal with one
state machine. If a crash lands after filesystem activation but before the DB
update, recovery sees the after-digest and advances the operation. Recovery
always replays the reviewed content-addressed blobs. It never recompiles and
quietly substitutes new bytes. Reproducibility makes later repair/GC simple,
not crash recovery nondeterministic.

If the model file is committed and the generated tree is reproducible, most
history belongs in Git. Keep Jails history only for execution/recovery and
evolution evidence. Undo becomes “apply the inverse model patch and compile,”
except for database migrations, whose inverse is a new forward migration.

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

`jails-protocol` is already a hand-written schema compiler. The baseline source
audit counted 188 `impl Codec` blocks, 197 encode functions, 197 decode
functions and 49 tag/from-tag pairs; the concurrent `--bind` delta immediately
added two more codecs. Request variants are repeated across subject types,
numeric tags, validation dispatch, codecs, maintenance classification, JSON
views and tests.

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
jails eject          transfer one facet to reader ownership
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
  semantic IDs, AppModel, ModelPatch, ProjectFacts, artifact/plan DTOs,
  diagnostics and generated wire DTOs; no filesystem or compiler behavior

jails-compiler
  link/validate, facets, type algebra, dependency graph, schema evolution,
  Java/SQL/HTTP emitters; depends only on jails-model

jails-workspace
  capture/adoption, document backends, Plan executor, SQLite state and
  recovery; depends only on jails-model

jails-cli
  composition root, generated command catalog, front ends and reporting;
  depends on jails-model + jails-compiler + jails-workspace

jails-tools-*
  optional run/test/db/log/editor/contract processes behind a versioned seam

jails-testkit
  shared test-only primitives
```

The dependency arrows are deliberately acyclic:

```text
jails-cli ──> jails-compiler ──> jails-model
    └──────> jails-workspace ──> jails-model
```

The CLI asks `jails-workspace` for one captured `ProjectFacts`, passes those
facts plus `AppModel` to `jails-compiler`, and hands the returned plan to the
workspace executor. Compiler and workspace never import one another. Optional
tools are subprocess clients and do not import the mutation protocol.

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

Use event sourcing only if historical audit and replay are product
requirements. A current semantic snapshot plus a bounded transaction/effect
journal is simpler for most projects.

## What will not simplify Jails by itself

### A new JDL

The CLI field syntax, generator vocabulary and two application-manifest paths
are already domain-specific languages. A prettier grammar does not remove any
semantic duplication. It adds a parser, spans, formatter, migration,
completion and documentation.

Define `AppModel` and `ModelPatch` first. If TOML then proves too clumsy, add a
grammar as one more frontend and compile it to the same model. That keeps the
language replaceable and testable.

### Minijinja or another template engine

Loops and conditionals would shorten some render functions. They would also
move structural Java decisions into untyped text. They do not unify type
semantics, names, dependencies, lifecycle or schema evolution. Use a richer
template engine only behind typed artifact IR, and count template complexity
as code rather than declaring it deleted.

### A three-crate rewrite

Crate count affects navigation and compile boundaries. It does not eliminate
`Recipe`, `Declared`, `DesiredChange`, `SemanticEdit`, prepared identities,
journals or receipts. Consolidating can help a strangler migration, but the
goal is one representation per concept and a narrow dependency graph.

### “Render in a temp directory and atomically swap it”

This works for a wholly managed new destination and is why
`src/new/publish.rs` is good. It does not atomically replace arbitrary files in
an existing nonempty project across platforms. It becomes useful for the new
disposable managed subtree, not as a replacement for journaling reader-file
patches.

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
5. freeze `PlanV1` as ordered operations over content-addressed blobs;
6. freeze the SQLite/filesystem replay state machine;
7. decide which CLI tool suites remain in the core product;
8. build and pin `jails-legacy` plus the differential E2E corpus.

### Parallel implementation lanes

Once those types are checked in, assign disjoint file ownership and run all
lanes concurrently:

| Lane | Delivers | Does not wait for |
|---|---|---|
| Model/linker | `AppModel`, explicit IDs, `ModelPatch`, reference graph, legacy-state importer, typed evolution programs | renderers or executor |
| Compiler kernel | facet IR, `TypeSemantics`, node-specific name projections, requirement graph, schema projection, `Plan` builder | CLI or SQLite implementation |
| Emitters | Java/SQL/HTTP/build emitters, split by independent facet families; existing templates reused where byte-stable | other emitter families |
| Workspace | one captured `ProjectView`, Maven/Gradle/properties/compose document backends, managed-root integration | command frontends |
| Executor | SQLite journal, exact blob replay, fsync/rename state machine, effects, `StateView`/`ReceiptView`, legacy receipt reader | generator internals |
| Frontends | command catalog, Clap, TOML/Serde model, direct CLI-to-patch adapters, plan/report encodings | emitter implementation details |
| Mechanical generation | wire codec derives, command metadata macro, local semantic registries | application compiler passes |
| Product split | external run/test/db/log/editor/contract commands and one versioned subprocess seam | compiler cutover |
| E2E firewall | twin-tree legacy/new runner, semantic normalizers, strict toolchain lane, child-process crash matrix, real-project corpus | implementation lanes |

Each lane compiles against the frozen interfaces and lands only when its G0–G5
gates pass. A quick representative spike (`record`, `query`, `add db`) may test
that the interfaces are sufficient; it is not a slow production migration.
Once validated, agents port all remaining facet families in parallel.

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
| journal/receipt/object directory protocols | SQLite plan/event state plus tiny replay | bespoke storage/GC/rewrite machinery |
| `main.rs` dispatch + command-path + pre-Clap parsing | generated command catalog | duplicated command oracles |
| `new` generation paths | compiler to `GeneratedTree`; keep `Publication` executor | manual preview path lists and nested engine state |
| drive/report/tool suites | optional command processes or explicit core modules | facade coupling and duplicated runners/caches |

## Architecture fitness rules

Add tests for properties that express the new shape, not file-size thresholds:

- after capture, no compiler module can access `std::fs`, a project root or a
  process runner;
- identical `AppModel + ProjectFacts + compiler version` yields an identical
  plan digest;
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

## Final recommendation

The crazy idea that fits the evidence is not “write a new language.” It is:

> Treat Jails as a compiler whose source is one application model, whose
> generated tree is disposable until ejected, whose irreversible output is an
> explicit evolution plan, and whose executor applies exactly the plan the
> reader reviewed.

Build the application compiler plus separate wire-codec and command-catalog
generators. Keep the current syntax as compatibility front ends. Use typed
facet and artifact IR rather than strings, generated source as output rather
than state, explicit stable IDs rather than name inference, and SQLite as a
simplifier for the remaining durable kernel.

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
