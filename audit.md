<!--
audit.md — one review of `new-world-compiler-jdl-v1` against
`simplify-sol.md` and `jdl-sol.md`, dated 2026-08-29.

**Read at `5cec56b`, re-verified at `cd0d45f`.** The test-suite series that
landed between the two (`58df59f`..`cd0d45f`) changed no file under
`jails-model`, `jails-compiler`, `jails-workspace` or `jails-contracts`, so
every A1-A4 finding is unaltered; the six A2 defects were nonetheless
re-reproduced against a binary built at `cd0d45f` rather than carried over.
A5.5 and A5.6 are rewritten for what that series closed.

**This is a measurement, not a plan.** Every claim below was produced by
running the binary or reading the tree at that commit, and each carries the
command or the `file:line` that produced it. Where a number is quoted it was
counted, not estimated. Nothing here is a proposal; `plan.md` is where work
goes.

**A finding that is closed is deleted from this file**, in the commit that
closes it — the delete-don't-mark convention `plan.md`, `bugs.md`,
`missing.md`, `modern.md` and `research.md` share. `git log -p -- audit.md`
is the record.

Findings are `A<section>.<item>` and the numbers are stable and never reused,
so a source comment may cite `audit.md A2.4` after the entry is gone and
still resolve through git.
-->

# audit.md — the canonical cutover, measured

## What was audited

`new-world-compiler-jdl-v1`: fifteen commits of canonical-compiler work on
top of `61413d7`, plus the test-suite series that landed during this review. The authorities are
`simplify-sol.md` (compiler architecture, the five contracts, gates G0–G5,
the deletion map, the fitness rules) and `jdl-sol.md` (JDL v1: grammar,
conventions §9.7, coverage §17, compiler layers §20, conformance §21,
acceptance checklist §24). `CLAUDE.md` supplies the traps this project has
already paid for once.

The "Implementation checkpoint" paragraphs in both documents are the
implementer's own status. They were treated as claims to check, not as facts.

**Environment note, because it changes what a test run means here.**
`git merge-file --diff-algorithm=` needs git ≥ 2.47; this machine has 2.43,
so the three-way merge exits 129 and the tests that exercise it fail for that
reason alone -- 58 of them when counted at `5cec56b`.
The mise toolchain (JDK 26) is not installed either. Those failures are the
machine, not the branch; every defect recorded below was reproduced by
running the binary in a way that does not depend on either.

---

## A0 — the answer

**The legacy path cannot be deleted.** The canonical architecture is real,
correctly layered, and delivers the single hardest thing the design asked
for — but it covers roughly two thirds of the product surface, its safety
proof is written almost entirely against a JDL dialect `jdl-sol.md` §22
supersedes, it has no byte-level regression net of its own, and it currently
emits a `pom.xml` Maven refuses to read.

Three things are genuinely done and should not be relitigated:

- **Source is no longer a database.** `jails-java` is not a dependency of any
  canonical crate, and nothing on the canonical path reparses generated Java
  or SQL. This was the deepest problem in the original audit.
- **Requirements come from the model, not from bytes.** Every
  `contains("…")` in `jails-compiler` is inside `#[cfg(test)]`; there is no
  canonical counterpart to `route/support.rs`'s scan of emitted Java.
- **Preview and apply cannot plan twice.** `model_generate::finish_generation`
  (`src/model_generate.rs:167`) does one capture, one compile, one
  materialize, then either reports the bundle or executes *that* bundle.

---

## A1 — coverage

### A1.1 Twenty of thirty-nine generators, twenty-one of twenty-five capabilities

`src/canonical_support.rs` is the authority and is honest code: an exhaustive
match that stops compiling when a clap variant is added. Its own test pins
20/39 and 21/25.

Still legacy: `migration`, `handler`, `command`, `cli`, `cases`, `client`,
`fetcher`, `job`, `http-workflow`, `association`, `http-sink`,
`idempotency`, `auth`, `webhook`, `search`, `durable-job`, `socket`,
`presence`, `seed`; and `format`, `ci`, `docker`, `k8s`.

### A1.2 Fifteen of twenty-three component kinds accept a declaration and emit nothing

`ComponentKind::ALL` has 23 entries (`crates/jails-model/src/component.rs:71`).
The compiler emits from `UnitKind`, which has 8
(`crates/jails-model/src/unit.rs:83`): `Class`, `Interface`, `Service`,
`Test`, `IntegrationTest`, `Sealed`, `Strategy`, `Controller`.

The other fifteen link, plan, apply and report success while producing no
file and no diagnostic:

```
jdl 1
app Comp { pkg com.example.comp  java 26  platform plain  build maven  storage none }
component class Widget
component client Audit { route POST "/v1/audit" }
component job Nightly
component cli Admin
component idempotency Payment
```
```
$ jails model check
model valid: .jails/model.jdl (7 nodes, 0 entities, 0 operations)
$ jails sync
synchronized sha256:09ee5ed…: 3 operations, 4 files written, 0 files deleted
$ find .jails/generated -type f
main/java/com/example/comp/Widget.java
test/java/com/example/comp/WidgetTest.java
```

`jdl-sol.md` §18.2 requires "every generated output has exactly one owner" and
§20.2 requires an exhaustiveness test that fails "when a registered role has
no emitter". Neither exists. A silent no-op on a declaration the author wrote
is worse than a refusal.

### A1.3 `jails app apply` still enters the legacy engine on a canonical project

`src/main.rs:68` dispatches `Command::App` unconditionally; `src/app.rs`
contains no `model_command::owns()` guard. Every other mutating command is
gated. Reproduced on a project with `.jails/model.jdl` and a two-line
`.jails/app.toml`:

```
$ jails app apply --pretend
plan ea6026fe… apply
  create  jails.toml
  replace pom.xml
  create  src/main/java/com/example/q2/adapters/Json.java
  ledger  create
```

Two editable authorities writing one project, capability code outside the
managed tree, and a legacy ledger created beside a compiler lock. `CLAUDE.md`
("Never permit both editable sources"; "Unsupported canonical mutations must
refuse rather than silently invoking the legacy engine") and
`simplify-sol.md` ("direct CLI and declarative apply are no longer competing
planners") both forbid this.

### A1.4 There is no supported route into JDL v1

- `jails new` / `new-cli` write no model at all. `jdl-sol.md` §17.3 says both
  "materialize the selected `app` axes in JDL".
- `jails model import` emits the **pre-v1 draft**, not `jdl 1`:
  ```
  application Boot @id(project_boot)
  package com.example.boot
  java 26
  dialect postgresql
  ```
  No `jdl 1` header, no `platform`, no `build` — the two axes §22 says the
  importer "MUST inspect the selected module once and materialize".
- `jails model upgrade --to 1`, the bridge §22 mandates, **does not exist**
  (absent from all 108 subcommands in `jails commands --json`).

So `jdl 1` is reachable only by hand-authoring the file.

### A1.5 Five of eleven legacy ownership kinds have no canonical home

`jails_protocol::vocabulary::resource::ResourceKey` is the legacy ownership
vocabulary.

| `ResourceKey` | canonical home |
|---|---|
| `WholeFile` | `RenderedTree.files` / `ReaderFacetKind::ManagedFile` |
| `MavenDependency` | `DocumentIntent::ReconcileDependencies` |
| `BuildFeature` | `DocumentIntent::ReconcileBuildFeatures` |
| `ComposeService` | `ReaderFacetKind::ComposeService` |
| `Property` | `DocumentIntent::ReconcileProperties` |
| migration history | `PlannedOperation::AppendMigration` |
| `MarkedBlock { path, marker }` | **none** — only compose is modelled |
| `CommandRegistration { dispatcher, command }` | **none** |
| `SpringTestImport { path, class }` | **none** |
| `MavenMainClass(ProjectPath)` | **none** |
| `Query(QueryId)` | **none** |

The five missing ones are exactly the surgical edits into reader-owned files —
the hardest category. Concretely, canonical `storage postgres` emits no
`TestcontainersConfig` and no `@Import` splice, so the trap `CLAUDE.md`
records at length ("`add db` on Spring must wire tests, or `mvn verify` goes
red on a test nobody wrote") has no canonical answer.

### A1.6 Lifecycle commands with no canonical backend

`undo`, `history`, `show`, `adopt`, `modernize`, `fmt`, and the `app.toml`
manifest. All but `app` refuse cleanly (`src/main.rs:93,361,451,469,474,654`),
which is the right interim behaviour; they are listed here as scope, not as
defects.

---

## A2 — correctness defects, each reproduced against the binary

### A2.1 The canonical path writes a `pom.xml` Maven cannot read

`storage postgres` on a plain-Maven project splices four dependencies with no
`<version>` and no Spring Boot parent to manage them:

```
$ jails sync && mvn -o validate
[ERROR] 'dependencies.dependency.version' for org.flywaydb:flyway-core:jar is missing.
[ERROR] 'dependencies.dependency.version' for org.flywaydb:flyway-database-postgresql:jar is missing.
[ERROR] 'dependencies.dependency.version' for org.postgresql:postgresql:jar is missing.
[ERROR] 'dependencies.dependency.version' for org.springframework.boot:spring-boot-starter-jdbc:jar is missing.
[ERROR] The build could not read 1 project
```

This is the trap `CLAUDE.md` states verbatim: versionless is correct under
`spring-boot-starter-parent` and **fatal** without one, `validate` included.
The cause is A3.7: these are hand-rolled `BuildDependency { version: None }`
values inside `Compiler::compile` (`crates/jails-compiler/src/lib.rs:391-410`)
that bypass `DependencySpec`, which already carries `spring_managed_version`
for exactly this.

### A2.1b Two marked blocks declare the same Maven plugin

`ensure_maven_source_root` (`crates/jails-workspace/src/documents.rs:506`)
gives every source set its own `<!-- jails:generated-source-root:<set> -->`
block, and each block contains a complete `<plugin>` declaration of
`org.codehaus.mojo:build-helper-maven-plugin`. A project with a generated main
*and* test root therefore declares that plugin twice inside one `<plugins>`,
and Maven reports it:

```
[WARNING] 'build.plugins.plugin.(groupId:artifactId)' must be unique but found
duplicate declaration of plugin org.codehaus.mojo:build-helper-maven-plugin
```

`jdl-sol.md` §9.7 asks for the opposite shape -- "Maven uses distinct
`add-source` and `add-test-source` executions", one plugin, two executions.
Restructuring it changes the bytes of an accepted reader facet, so it needs
its own change with a migration path rather than a clause in A2.1.

An earlier draft of this entry blamed a clash with the plugin `jails new-cli`
writes. That was wrong and is corrected here: `new-cli` writes none, `sync`
writes one per source set, and a second `sync` adds nothing.

### A2.2 Field source order is discarded, and record component order is ABI

`Entity.fields` is a `BTreeMap<FieldId, Field>`
(`crates/jails-model/src/model.rs`). Source order `zulu, id, alpha` produces:

```java
public record Task(
    String alpha,
    UUID id,
    String zulu
) {
```

`jdl-sol.md` §7.3 lists entity fields first among the orders that "MUST be
retained", "because Java record component order is ABI".

### A2.3 `order by [createdAt desc, id]` emits `order by created_at, id`

The direction is parsed and linked into `QuerySemantics.order: Vec<Ordering>`;
the emitter reads the flat `Query.order_by: Vec<FieldId>`
(`crates/jails-compiler/src/emit_operation.rs:34`). Generated adapter:

```java
sql.append(" order by created_at, id");
```

A query declared newest-first returns oldest-first. There is no test: `grep
"order by \["` over `tests/` returns nothing.

### A2.4 A command's `emit` publishes nothing, and only the first `emit` survives

`crates/jails-model/src/jdl/v1/parser/operation.rs:145` keeps
`transition.emits.first()` as the flat `yields: Option<OperationId>`, and the
emitters read `yields` (`emit_operation/transition.rs:94`). §12.4 makes
`emit Event` repeatable.

For commands it is worse. A `command Create(title, status) { emit TaskCreated
emit TaskAudited }` emits both payload records and a `JdbcCreateCommand` that
references neither:

```java
public Task execute(CreateCommand.Input input) {
    JdbcClient.StatementSpec statement = jdbc.sql("insert into task …");
    …
    return statement.query(Task.class).single();
}
```

### A2.5 `select` is read as the update list, so `jdl-sol.md` §4 does not link

`crates/jails-model/src/jdl/v1/parser/operation.rs:141` sets
`sets = all parameters` when `update` is omitted, without subtracting the
selector or the `@version` field. §12.4 says "all non-selector, non-version
entity parameters".

```
$ jails sync   # transition Close(id) { select [id] }
jails: canonical transition `close` attempts to rewrite primary key `id`
```

The same rule breaks the spec's own §4 complete example, which §21 designates
an executable conformance fixture:

```
$ jails model check   # jdl-sol.md §4, verbatim
[model-managed-field-target] $.operations.complete.semantics:
    field `version` is compiler-managed and cannot be set or updated
[model-ejection-target] $.ejections.task_repo_fake.target:
    ejection target `Task.repo.fake` is neither a generated artifact nor a
    semantic implementation boundary
```

Nothing extracts the documentation examples, which §21 requires ("MUST be
extracted in CI rather than copied into disconnected test strings").

### A2.6 Tables are not pluralized

`crates/jails-model/src/naming.rs` has no pluralizer. Canonical emits
`create table task` and `V001__create_task.sql`; legacy emits `tasks` and
`V001__create_tasks.sql` (`tests/cli/reports.rs:414`,
`tests/cli/effects.rs:208`). §9.7 specifies the irregular map, the invariant
list and the `fe→ves` / `y→ies` rules in full. Importing a legacy project
silently changes its table name.

---

## A3 — the abstraction

### A3.1 `Provenance` discards every typed ID at the boundary that needs them

```rust
pub struct Provenance {
    pub artifact_id: String,
    pub ejection_id: Option<String>,
    pub ejectable: bool,
    pub semantic_ids: BTreeSet<String>,
    pub compiler_pass: String,
}
```

The model has `EntityId`, `OperationId`, `ComponentId`, `EjectionId`. Merge
identity and ownership transfer — the two things stable IDs exist for — are
decided by string comparison. There are also three spellings of one boundary:
`ejection_id`, `ejectable`, and `ejection_target()` falling back to
`artifact_id`. `simplify-sol.md` specifies exactly two IDs.

### A3.2 `SemanticPlan` is five counters, and the review surface regressed

```rust
pub struct SemanticPlan {
    pub model_nodes: usize,
    pub managed_files: usize,
    pub migrations: usize,
    pub reader_document_intents: usize,
    pub effects: usize,
}
```

This is what the reader reviews and what `PlanDigest` commits to. The
consequence is visible at the CLI: `--diff` is advertised as "unified
current-to-prepared file diffs" and on a canonical project prints Rust
`Debug`:

```
$ jails --diff g field Task note:string? --pretend
plan sha256:a0a6d3ea…: 4 operations, 8 managed files
  PublishMergedTree { root: ProjectPath(".jails/generated"), before: Some(ContentDigest("sha256:68a8…")), … }
```

Legacy `--pretend` names the operation and the path (`create
src/…/Json.java`, `replace pom.xml`). The new plan is less reviewable than
the one it replaces.

### A3.3 Illegal states are representable in `WorkspaceSnapshot`

`accepted_model`, `accepted_projection` and `accepted_compiler` are three
independent `Option`s, while `crates/jails-workspace/src/capture.rs:34`
already has an `AcceptedCompilerState { model, projection, compiler }` that
it splats apart on the way out.

### A3.4 The general file API the design forbids is in the contract

`WorkspaceSnapshot.files: BTreeMap<ProjectPath, CapturedFile>`, and the
compiler reads it (`crates/jails-compiler/src/lib.rs:653`, `compose_path`).
`simplify-sol.md`: "It exposes facts and an overlay, never a root path or
general-purpose file API." Only capture's discipline about *what* it captures
holds the line.

### A3.5 `ExternalTypeIndex` is dead, so `TypeSemantics` is not kept

Never populated in `capture.rs`, never read by the compiler.
`TypeRef::External(String)` is therefore a bare name with no semantics — a
project-owned type is exactly as untyped as it was in the legacy generator.

### A3.6 `DocumentIntent` mixes four altitudes

`ReconcileDependencies` and `ReconcileBuildFeatures` are correctly
build-neutral. Against them:

- `EnsureMavenSourceRoot` / `EnsureGradleSourceRoot` put a build-tool choice
  inside the pure layer.
- `ReconcileProperties { previous, desired }` carries a precomputed diff
  inside a desired-state intent.
- `EjectFile` and `AdoptJava` carry bytes, and `AdoptJava.base` is documented
  as "exact bytes last rendered by the legacy generator" — the legacy engine
  written into the destination vocabulary.

The deletion map's row (`Change` + `DesiredChange` + `SemanticEdit` → one
`PatchSet`) is half done: there is one enum, and it is not one altitude.

### A3.7 The one good emitter abstraction is not applied uniformly

`Pack` (`crates/jails-compiler/src/emit_capability.rs:106`) is the best-designed
thing in the new code — a declarative table of `JavaFile` / `DependencySpec` /
`ResourceFile` / `PropertySpec` with a closed `BootCondition`, sharing
`templates/*.java` with the legacy renderers so the two cannot drift. It
covers capabilities only. Entity facets, operations and components are
bespoke `format!` code, and `storage postgres` builds its dependencies inline
in `Compiler::compile` — which is where A2.1 comes from.

`Pack` also carries `PackageOverride { suffix, project_subpackage }`, a
per-file placement knob of the kind §17.2 retires and §9.7 forbids.

### A3.8 `Component` is the `Option` soup §7.2 forbids

```rust
pub struct Component {
    pub kind: ComponentKind,
    pub parameters: Vec<ComponentParameter>,
    pub on: Option<ComponentReference>,
    pub yields: Option<ComponentReference>,
    pub route: Option<OperationRoute>,
    pub bindings: Vec<ParameterBinding>,
    pub variants: Vec<ComponentVariant>,
    pub source: Option<String>,
}
```

§7.2: "`Operation` and `Component` MUST be tagged enums with kind-specific
payloads. They MUST NOT be … structs where most fields are `Option`."
`Operation` got this right. `Component` did not, so nothing in the type stops
a `component class` carrying a `route`, and the validity matrix lives
separately in `crates/jails-model/src/linker/component/registry.rs` — a
23-row hand-maintained kind-by-member table, which is `refuse_misplaced`
reborn under the deletion map's own row. In fairness it is a much better
version: a typed `Rule` with a `Presence` enum, exhaustive over the kind
enum, in one place.

`SourceUnit` has the same shape plus raw `Option<String>` references (`on`,
`yields`) sitting unlinked inside the *linked* model.

### A3.9 Two representations of every operation, inside the new crate

`Query { filters, order_by, limit, route }` beside `QuerySemantics {
parameters, joins, order, limit, route }`; `Transition { fields, sets, yields }`
beside `TransitionSemantics { select, update, emits }`. The JDL v1 parser
populates both; the emitters read the flat one. This single decision produced
A2.3, A2.4 and A2.5 — and A2.5 is why the spec's own example does not link.
It is "one intent copied through too many representations" re-created at
smaller scale.

### A3.10 `RenderedTree` is both the in-memory type and the on-disk format

`RenderedFile.bytes: Vec<u8>` under `#[derive(Serialize)]` becomes a JSON
array of integers. 6.2 KB of generated Java produces a 99,892-byte
`.jails/compiler.lock.json`, pretty-printed, rewritten whole on every
mutation. The legacy store separated these deliberately —
`jails-protocol`'s `envelope.rs` owns the format and hex-encodes the payload.

### A3.11 There is no `OutputConvention` registry, and the layout contradicts §9.7

Emitters concatenate packages inline in 15+ places
(`format!("{}.adapters.jdbc", model.project.base_package)` and friends across
`emit_java.rs`, `emit_dto.rs`, `emit_factory.rs`, `emit_operation.rs`,
`emit_capability.rs`). §20.2 requires one registry and says emitters "MUST
NOT concatenate a package, prefix, suffix, filename, or test marker
themselves".

Observed against §9.7's closed layer table:

| §9.7 | emitted |
|---|---|
| repository port in `app` | `repository` |
| primary SQL adapter in `adapters` | `adapters.jdbc` |
| fake adapter in `adapters` | `adapters.memory` |
| query types in `app`/`adapters`/`web` | `application.queries` |
| HTTP in `web` | `ports.http` |
| — | `application` (`ExecutionContext`) |

`application`, `ports` and `repository` are not §9.7 layers. §3.1 rule 4
makes conventions part of `jdl 1` and forbids a compiler changing one
silently, so every row above is a future breaking move for any project
generated on this branch. The `ETest.java` row of §9.7 is also unmet: no
companion test is emitted for an entity record.

### A3.12 `AppModel` is missing three fields the spec puts in the digest

Against §7.2: no `language_version`, no `convention_version`, no separate
`enums` map (enums are entities carrying `enum_constants`), and no `derived:
BTreeMap<DerivedRoleKey, DerivedValue>`. `model explain` does not exist.
§18.4 and §20.3 require the derived records and §7.2 makes them part of the
accepted-model and plan digest, so "convention must not mean hidden
behaviour" is currently unmet in both directions: the values are neither
inspectable nor digested.

### A3.13 Two diagnostic code systems, and no spans below the parser

94 `JDL0001`–`JDL1002` codes live in `crates/jails-model/src/jdl/v1/`; 140
kebab `model-*` codes live in the linker, which is §18.2's passes 2–9. Below
that, `jails-compiler` and `jails-workspace` return `Result<_, String>` in 78
places and `CompileError` is a newtype over `String` — a third vocabulary
with no codes at all.

`Diagnostic { code, path, message, fix }` also has no severity, file, span,
related spans or notes. §18.3's contract is met by the parser only; a linker
diagnostic points at `$.operations.complete.semantics`, not at a line.

`JDL1001`/`JDL1002` are outside the ranges §18.3 declares.

### A3.14 No typed artifact IR

`simplify-sol.md` Pass 4 specifies `JavaFile` / `JavaDecl` / `JavaExpr` /
`SqlExpr`. None exist. The canonical emitters assemble Java and SQL with
`format!` exactly as the legacy generators do (67 sites in `emit_unit.rs`, 53
each in `emit_sql.rs` and `emit_java.rs`). This is the largest unbuilt piece
of the design and the reason for A4.2.

---

## A4 — simplicity, measured

Production lines, `#[cfg(test)]` stripped and blank lines excluded.

| | lines | units covered |
|---|---:|---:|
| legacy transaction kernel (`prepare` + `commit` + protocol `intent`/`durable`/`observe`) | 18,789 | — |
| replaced by `jails-workspace` + `jails-contracts` | **3,763** | — |
| legacy generation (`generate` + `spec` + `java` + `project` + `engine`) | 41,328 | 64 |
| replaced by `jails-model` + `jails-compiler` + root `model_*` frontends | **25,389** | 41 |

### A4.1 The transaction-kernel simplification is real, and it is the big one

Roughly 5×. Object store, custom codec, GC, journal, receipts and
roll-forward are replaced by capture → merge → exact plan → lock-last
publication. This is `simplify-sol.md`'s largest claim and it is delivered.

### A4.2 The generation simplification has not happened

646 production lines per generator-or-capability on the legacy side; 619 on
the canonical side. Flat. The cause is A3.14: moving string assembly into a
new crate does not make it cheaper. The one place a real IR exists — `Pack` —
is also the one place legacy and canonical share templates and cannot drift.

### A4.3 Representation count is about the same; the win is authority, not arity

Old chain: `Intent`, `Recipe`, `Recorded`, `Declared`, `Asked`,
`CanonicalMutationRequest`, `DesiredChange`, `SemanticEdit`, `Change`,
`PreparedChange`, `PreparedKind`, ledger rows, journal records, receipts,
effects, `Outcome` — sixteen. New chain: CST, `source::*`, `ModelPatch`,
`AppModel`, `SourceUnit`, `WorkspaceSnapshot`, `PlanDraft`, `RenderedTree`,
`DocumentIntent`, `TreeManifest`, `Plan`/`PlannedOperation`, `PlanBundle`,
`Execution` — thirteen.

The improvement is not arity. It is that the new ones are a lowering
pipeline, each derived from the one above it, where several old ones were
parallel authorities arguing about one fact. Claim "one authority", not
"fewer representations" — and note that A3.9 shows the old habit reproducing
itself inside the new crates.

### A4.4 The tree is currently larger than before the rewrite began

Three model front ends are live and editable: `.jails/model.toml`
(`source.rs`, 581 lines), the pre-v1 JDL draft (`jdl.rs` + `jdl/declaration.rs`
+ `jdl/operation.rs` + `jdl/render.rs` + `syntax_edit.rs`, 1,912), and
`jdl 1` (`jdl/v1/`, 4,955). Above them sit 6,551 lines of frontend adapters
in the root binary carrying 25 `is_v1_source` branch sites; `model_capability.rs`
alone has 8 v1 branches and 16 jdl branches, because every mutating command
is written three times. Expected mid-cutover, but no simplicity claim can be
banked until two of the three front ends are gone.

---

## A5 — the proof

### A5.1 Zero golden coverage for canonical output

All 61 directories under `tests/golden/` snapshot legacy `src/main/java`
trees; `grep -rl "jails/generated" tests/golden` returns nothing. The
canonical `.jails/generated` tree — the product of the new architecture — has
no byte snapshot anywhere, so nothing fails when a canonical emitter changes
bytes.

### A5.2 No golden for any canonical persisted format

`tests/protocol-golden/` holds five fixtures, all legacy. There is none for
`jails.compiler-lock.v2`, `jails.plan.v1` or `jails.plan-bundle.v1`. These are
`#[derive(Serialize)]` over `AppModel`, so **adding a field to any model
struct silently changes the persisted format**. There is already a v1→v2 lock
bump and no test that a v1 lock still decodes. `simplify-sol.md`'s fitness
rule ("every persisted union tag and field number is generated and
golden-tested") and G0's "old fixtures decode and new canonical encodings are
golden" are unmet on the canonical side.

### A5.3 The G1 differential corpus does not exercise JDL v1

`tests/differential.rs:17` defines `EMPTY_JDL` as the pre-v1 draft, and all
32 scenarios use it. Across the whole tree, `jdl 1` appears in 10 blocks in
one file (`tests/cli/model.rs`); the pre-v1 draft appears in 47 and
`.jails/model.toml` in 44. Even `jails-compiler`'s own unit tests author
their models in TOML (`crates/jails-compiler/src/lib.rs:694`).

The G1 gate is currently protecting the front end that `jdl-sol.md` §22 says
to delete.

### A5.4 The frozen oracle is not frozen

`scripts/verify-rewrite-g1-canary.sh:5` defaults `JAILS_LEGACY_REVISION` to
`git rev-parse HEAD`. The comparison is still meaningful — the legacy engine
is live at HEAD — but it is not the frozen pre-cutover binary
`simplify-sol.md` asks for, and it drifts with the branch.

### A5.5 G4 is closed for the kernel being deleted, and empty for the one replacing it

`855e438` closed G4 properly: `every_failpoint_converges_after_a_child_dies_there`
runs every entry in `fault::POINTS` in a child that `abort()`s inside the
trip, then opens what the crash left — including a lock whose owner is gone —
and asserts convergence twice, requiring `SIGABRT` rather than merely an
unsuccessful exit so a panicking child cannot satisfy it. `plan.md` P13.6
records it as closed and that is accurate.

All 22 of those failpoints are in `jails-commit`, the legacy kernel.
`jails-workspace::execute` has none, and there is no canonical equivalent of
the suite. So the property `simplify-sol.md` trades rollback away for — "a
crashed command may leave a temporarily mixed but individually valid tree;
the next identical generation repairs it deterministically" — is now
rigorously proved for the code that is being deleted and asserted only in
prose for the code that replaces it. G4's *method* transfers; its coverage
does not.

### A5.6 The one gate is green, and still not reproducible

At `5cec56b` the clippy step of `mise run verify-rewrite` failed on rustc
1.94 — `nonminimal_bool` at `crates/jails-java/src/java.rs:686`, since
`is_none_or` stabilised and `-D clippy::all` is workspace policy. `588561b`
fixed it, and `mise.toml` now splits a separately callable `lint` task so the
pre-commit hook and the gate cannot drift on flags. Both good.

The finding that survives is the one underneath: there is still no
`rust-toolchain.toml`, `mise.toml`'s `[tools]` still pins Java, Maven, mvnd
and Gradle but not rustc, and `.github/workflows/verify-rewrite.yml:45` still
uses `dtolnay/rust-toolchain@stable`. The single gate the cutover plan rests
on can go red tomorrow from a clippy release touching code nobody edited —
which is exactly what it just did.

### A5.7 `git merge-file --diff-algorithm=` requires git ≥ 2.47

`crates/jails-workspace/src/merge.rs:42` and legacy
`crates/jails-prepare/src/merge.rs:108` both pass it. On git 2.43 — Ubuntu
24.04 LTS, Debian 12, RHEL 9 — it exits 129 and **every regeneration touching
an already-generated file fails**, which is the D1 loop the product is built
around. It fails safe (a hard `Err`, no writes), nothing preflights the
version, and `doctor` does not check it.

---

## A6 — craft

### A6.1 The comment density collapsed

| crate | code | comment lines | % |
|---|---:|---:|---:|
| `jails-generate` | 23,871 | 4,773 | 19 |
| `jails-project` | 13,172 | 2,483 | 18 |
| `jails-prepare` | 10,266 | 1,509 | 14 |
| `jails-model` | 13,925 | 199 | **1** |
| `jails-compiler` | 8,328 | 85 | **1** |
| `jails-workspace` | 3,779 | 37 | **0** |

`crates/jails-workspace/src/materialize.rs` — the single boundary where a
semantic patch becomes filesystem bytes — has no module doc comment at all.

This is not style. Written-down reasoning is the mechanism this project uses
to stop a decision being silently reversed: 262 source comments cite
`plan.md` by section, `CLAUDE.md` records each trap with the failure that
paid for it, and closed items are recoverable only through `git log -p`. A
one-line "this is the compatibility projection; the rich value in `semantics`
is not read yet" would have flagged A2.3 and A2.4 at review time.

### A6.2 Three functions carry what §20.1 specifies as nine layers

```
508  crates/jails-compiler/src/lib.rs:68          Compiler::compile
404  crates/jails-model/src/model_apply.rs:9      AppModel::apply
353  crates/jails-model/src/linker.rs:26          Linker::link
283  crates/jails-model/src/jdl/v1/parser/declaration.rs:4  parse_app
260  crates/jails-compiler/src/emit_operation/transition.rs:11  lower
```

`compile()` reaches 32 columns of indent and holds 12 inline `return Err`
refusals. There is no separate validator layer; validation is split between
`Linker::link` and `Compiler::compile`, so §18.2's ordered passes exist as
prose. `tests/architecture/`'s "largest module" gate does not catch this
because it measures modules, not functions.

### A6.3 What is good, and should be kept

- No bare `.unwrap()` in production across all three canonical crates: 85
  `.expect("…")` with real messages and 25 `unreachable!`. Five of the latter
  carry no message; several encode cross-crate invariants, so relaxing a
  linker rule turns a diagnostic into a panic.
- Eight `#[allow]` escape hatches in the whole of the new code, every one
  `too_many_arguments`.
- `RenderedTree::insert` makes two invariants structural: managed output
  cannot escape the managed root, and "two compiler units emit `X`" is an
  error rather than last-writer-wins.
- `ReaderFacetKind` — storing only the marked slice as merge BASE, so
  `compose.yaml` stays reader-owned around it — is better than the design
  asked for, and generalised from Redis to Kafka and Mail, which is the test
  of an abstraction.
- `src/canonical_support.rs` is coverage as code: an exhaustive match that
  stops compiling when a clap variant is added, with the reason written above
  it.

### A6.4 Three constants for one persisted format

`COMPILER_LOCK_SCHEMA` (`materialize.rs:10`) and `COMPILER_LOCK_SCHEMA_V1` /
`_V2` (`capture.rs:15-16`), in two modules. The decode fails closed, which is
right; nothing records what changed between the versions.

### A6.5 `jails-workspace` declares `jails-compiler` as a runtime dependency

It is used only under `#[cfg(test)]` (`materialize.rs:673`), and
`simplify-sol.md` states the two must never import each other.

---

## A7 — suggested order

1. **A1.3** — guard `app apply`. One line; it is an active data-integrity
   hole.
2. **A3.9** — collapse the flat/`*Semantics` duplication in `jails-model`.
   This closes A2.3, A2.4 and A2.5 together, and makes §4 link.
3. **A2.1b** — give the Maven source roots one plugin block with several
   executions, and plan the migration for the accepted facet bytes.
4. **A2.2** — preserve field source order.
5. **A3.11 / A3.12** — build the convention registry, the `derived` records
   and `model explain` *before* more emitters land. Every emitter added now
   hard-codes a placement §9.7 will later have to move.
6. **A1.4** — write `model upgrade --to 1`, then port `tests/differential.rs`
   and `tests/cli/model.rs` onto `jdl 1` (**A5.3**). Until that lands the G1
   gate protects the wrong front end.
7. **A5.1 / A5.2** — golden the canonical tree and the three canonical
   persisted formats, and add the v1-lock decode test.
8. **A1.2** — emitters or refusals for the fifteen silent component kinds,
   plus the §20.2 role↔emitter exhaustiveness test that would have caught
   them.
9. **A5.4 / A5.6** — pin `JAILS_LEGACY_REVISION` to a real pre-cutover SHA
   and pin the Rust toolchain.
10. **A5.5** — port G4's child-process method to `jails-workspace::execute`.
    The suite `855e438` wrote is the template; what it needs is failpoints on
    the canonical publication sequence and the convergence assertion stated
    against the compiler lock rather than the journal.
11. **A6.1** — write the module docs while the reasons are still recoverable.

`A3.14` (typed artifact IR) is the largest remaining piece of the design and
is not on this list because it is a phase, not a fix.
