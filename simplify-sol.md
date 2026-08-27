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
compiler with an explicit eject escape hatch**. Keep the current CLI and
`app.toml` as front ends initially. Do not invent JDL until the model and IR
have proved themselves.

## Audit basis

This audit used the live tree at HEAD
`f0e66829cb8d1ef236485bdf1b5df2bf886a93e5`, including the then-uncommitted
42-line addition in `crates/jails-report/src/doctor/wiring.rs`. The inventory
was taken from the filesystem, not inferred from the module graph:

| Scope | Rust files | Raw lines | Main concern |
|---|---:|---:|---|
| `crates/jails-engine` + root `src` | 66 | 18,933 | orchestration and CLI |
| `jails-generate` + `jails-java` | 64 | 27,880 | lowering and rendering |
| `jails-protocol` + `jails-project` + `jails-spec` + `jails-state` | 86 | 39,049 | domain, wire values and project state |
| `jails-commit` + `jails-prepare` + `jails-drive` + `jails-report` + `jails-support` + `jails-testkit` | 83 | 36,445 | planning, transactions and tools |
| **Total** | **299** | **122,307** | **96,317 nonblank code lines** |

The totals include colocated tests and should not be read as 96,317 lines of
production logic. They are useful for scale and coverage, not as a productivity
metric.

Every Rust file in those scopes was inventoried and assigned to one of four
audits. Findings are grouped by responsibility below instead of repeated as a
299-row filename dump. The codebase graph was used first for structure, call
paths and hotspots. Its current project matched the HEAD above, but exact
coverage checks still reported several files as metadata-changed or absent
from the recorded generation. Those files were read directly. A clean graph
coverage result means no *recorded* gap, not proof that an exhaustive query is
complete; the filesystem inventory and source reads are the authority for the
exhaustive statements here.

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

