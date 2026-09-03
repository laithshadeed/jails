<!--
The target abstraction. `docs/50-simplify.md` says what the five plans delete;
this file says what shape they are deleting *towards*, so five agents removing
code in parallel converge on one system rather than five tidier versions of
the old one.

**A closed item is deleted from this file**, in the commit that closes it.
Item numbers `S60.n` are stable and never reused.
-->

# 60 — The abstraction: five nouns, four verbs

**Read `docs/00-contracts.md` first.** The five contracts stand. What this
file changes is the number of *shapes* between them: the code carries several
representations of each contract and a translation layer between every pair,
and that is where the mess is.

## What the tree looks like today

Measured 2026-09-02 over the crates that survive the pass
(`grep -rhoE '^\s*pub(\(crate\))? (struct|enum|trait) \w+' <crate>/src | wc -l`):

| crate | public types | of which |
|---|---:|---|
| `jails-model` | 143 | 43 are `source.rs`, the unlinked wire shape the parser builds and the linker consumes -- a deliberate second copy, held by nothing else; the closed vocabularies every other crate reads are here |
| root binary | 59 | 15 are clap argument types; the rest are per-command request bags |
| `jails-contracts` | 43 | |
| `jails-drive` | 39 | |
| `jails-project` | 28 | a second project model (`model::{Project, Layers, Artifact, Change}`) beside the snapshot's `ProjectFacts` |
| `jails-compiler` | 11 | |
| `jails-spec` | 7 | where a project is and what builds it; no vocabulary at all |
| `jails-workspace` | 5 | |

Five specific shapes, each measured:

1. **Every closed vocabulary has one owner, and it is `jails-model`.**
   `Layer`, `CapabilityKind`, `ArtifactKind`, `EndpointMethod`,
   `RequestFormat`, `Precondition` and `BuildSystem` are defined once, and
   the CLI's `clap::ValueEnum`s are those same enums under the model crate's
   `cli` feature rather than copies with a `From` at the boundary. One set
   is deliberately not folded: `jails_spec::build::Build` answers what a
   *directory* looks like from outside -- including a build file jails
   recognises by name and refuses to read -- which is a different question
   from a module's `build` axis, and its `Foreign(name)` reaches a refusal.
2. **Every declarative renderer is a `Recipe`; the structural ones are
   still functions.** `Recipe<N>` (`jails-compiler::recipe`) is files,
   dependencies, properties, compose services, build features and a
   placement rule as one `static`, over a `Node` -- a capability, a
   component, an operation or an entity -- and `recipe::render` is the one
   loop. The 22 capability packs, twelve component kinds, the entity's
   one-file facets, the operation ports, the Kafka slice of an event and a
   command's outbox are rows, and what a template cannot say is a named
   fragment renderer (`emit_java/fragment.rs`, `emit_java/operation.rs`).
   `emit.rs` holds two tables: four recipe walks and five function passes,
   and the five are what still builds Java from the model's *structure*
   with `format!` -- the repository adapters, a query's SQL, a proof's
   request (723 sites). S60.3 has the count and what is left.
3. **The compiler reads the snapshot.** Every pass takes
   `&WorkspaceSnapshot`; the two predicates the compiler decides over it
   (`emit::jdbc_on_classpath`, `emit::jspecify_on_classpath`) are named
   functions of `snapshot.project`, not fields copied out of it.
4. **The tool crates keep a second project model.** Closed: `Project` is a
   root plus the `ProjectFacts` capture observes, `jails-project` is the
   reader that produces them for the snapshot and for `drive` and `report`
   alike, and a fact a command needs that the snapshot lacks is added to
   `ProjectFacts` by capture rather than read again beside it.
5. **Entry points come in families.** Closed: `capture`, `materialize` and
   `finish_generation` are one function each, taking the intended model,
   the model update and the reader paths as arguments; the binary's `_at`
   family is gone.

## The target

Five nouns. Each is one type, owned by one crate, with no second spelling.

| noun | type | crate | what it is |
|---|---|---|---|
| **Source** | `Document` (the CST) | `jails-model` | the JDL text; the only thing a command edits |
| **Model** | `AppModel` | `jails-model` | what the source means, linked; owns every closed vocabulary |
| **Snapshot** | `WorkspaceSnapshot` | `jails-contracts` | every external fact, captured once; `ProjectFacts` is the only project model |
| **Desired** | `PlanDraft` | `jails-contracts` | the managed tree, reader-file intents and migrations the model implies |
| **Plan** | `PlanBundle` | `jails-contracts` | the exact, content-addressed transition |

Four verbs. Each is one function.

```text
edit    : Source × Edit                 -> Source        (jails-model)
compile : Snapshot × Model × Evolution  -> Desired       (jails-compiler)
plan    : Snapshot × Desired            -> Plan          (jails-workspace)
execute : Plan                          -> ()            (jails-workspace)
```

and one composition in the binary:

```text
mutate(root, edit, evolution) = execute(plan(snapshot, compile(snapshot, link(edit(source)), evolution)))
```

with `--pretend` stopping after `plan`, `--plan-out` writing it, and
`--plan-in` starting at `execute`. That composition is `finish_generation`,
and it takes exactly that: the edited source and an `Evolution`. `edit` is
the closed set of byte-preserving functions in `jdl/v1/edit.rs` (append or
remove a declaration, set a property or attribute, add, replace or remove a
member, rename an identifier) -- a set of functions rather than an enum,
because nothing consumes the edit as a value: the plan records the source
before- and after-image, and the evolution is the only input the compiler
needs beside the model. `Evolution` is one closed enum in `jails-model`,
passed to `compile` and read by `emit_sql::derive`.

### S60.3 — `Recipe` generalises `Pack` to every kind

**Landed:** `Recipe<N>` in `crates/jails-compiler/src/recipe.rs` is the
shape, `Node` is what a capability, a component, an operation and an entity
each implement (its id and name, the closed `Key` vocabulary its templates
may spell, the provenance its files carry, and the package one of its
layers is -- an entity with a pinned `pkg` answers that for every row of
its slice), and `recipe::render` is the one loop: for each node, look its
recipe up, substitute the node's keys and the recipe's fragments into each
row's template, and write it through the one `JavaUnit` shell. A row is

```text
JavaFile { role, template, before_boot, imports, only_when, source_set, placement, ejectable, class, template_class }
```

with `Naming` (`Fixed`, `Suffix`, `Wrap`, `By`) for the class, `Placement`
for the layer, `Import::{Own, Role, From, Keyed, Moved, ContainerSupport}`
for what the template cannot say, and `Fragment::{WhenCapability, WhenBoot,
Rendered}` for what is structural. A fragment is rendered once per node and
only when some selected file spells its key, so a primary key's type is
asked of an entity with a port and never of an enum; a rendered fragment
carries the imports its text relies on, and they join only the files that
spell its key; a fragment may spell `{{class}}`, because fragments are
substituted before the file's keys. `Need` refuses before rendering. A
row's `role` is a `jails_model::boundary` entry, which is what makes
`eject Task.repo.fake` resolve to the id the row emits.

**The named fragment renderers exist**, and they are the closed set this
item asked for: `emit_java/fragment.rs` renders a record's components and
its compact constructor (through the same `record_declarations` and
`record_constructor` every operation `Input` goes through), an enum's
constants and the members its wire values need, a primary key's Java type,
and the four lists a test-data builder is made of; `emit_java/operation.rs`
renders a port's `ROUTE` constant, answer type, execution context, row
selector, expected version and `Input` record. The templates around them
are real `.java` files under `templates/spring/` (`entity_*`,
`operation_*`, `enum_converter`).

Rows today: the 22 capability packs (`Recipe<Capability>`), twelve
component kinds (`Recipe<Component>`), the entity's one-file facets --
record, enum, repository port, service, events port, search port -- plus
the test-data builder and the enum's Spring converter (three
`Recipe<Entity>`, one per compiler pass), the four operation ports and the
event's record (`Recipe<Operation>` in `emit_java/operation.rs`), the Kafka
slice of an `event` and a command's outbox (`Recipe<Operation>`). Of the 39
generator kinds, 17 are wholly rows (the twelve components, `record`,
`value`, `factory`, `enum`, `event`); six are a row for the port and a
function for what implements it (`repo`, `search`, `usecase`, `query`,
`transition`, and `scaffold` through its `http` facet); the rest are
functions. `emit.rs` walks two tables -- `RECIPE_WALKS` (four) and
`FUNCTIONS` (five) -- and a unit test pins the lengths; `emit_java::emit`
stays in `FUNCTIONS` because it hosts the entity and port recipe walks
*and* the functions below.

**What remains, and why each is still a function.**

- `emit_java` -- the multi-file facets (`dto`, `http`, `seed`) and the
  repository and search adapters. The adapters choose their owner
  (`cap_db`, or the scaffold's default when JDBC is only observed) and
  which one is the bean from `emit::jdbc_on_classpath`, a fact of the
  captured build a recipe row cannot read, and their artifact ids are
  keyed on the storage capability rather than the entity, which the loop's
  `art_<node>_<role>` does not spell. Their bodies -- a column list, a bind
  list, the `on conflict` clause -- are the next fragments to name.
- `emit_operation` -- command, query and transition adapters: a JDBC
  adapter whose SQL and bind list are lowered from the operation's
  parameters.
- `emit_relation` -- association: the join table and both sides' adapters.
- `emit_http` -- the HTTP proof of every routed operation, which drives a
  request through `emit_mockmvc` from a sample of the operation's input. It
  reaches across nodes for the sample and stays a function.
- `emit_architecture` -- the one ArchUnit test, a model-level file.

Beside them, two component kinds stay functions inside the component
walk (`http_sink`, `durable_job`: each renders a sample argument list from
*another* node's fields), the unit kinds (`class`, `interface`, `service`,
`controller`, `sealed`, `strategy`, `test`, `integration-test`) are
`emit_unit`, and three model-level shared files (`SchedulingConfig`,
`ApiError`, the architecture test) are one file per model rather than per
node, which the recipe's node-per-row shape does not express.

The number to read is the `format!` count:

```
grep -rc 'format!(' crates/jails-compiler/src | awk -F: '{s+=$2} END {print s}'
```

723 now (834 when this file was first measured, 808 at the start of the
`Recipe` work, 762 before the fragment renderers). It falls as an adapter's
body becomes a template plus named fragments, and the next rung is the
repository adapters' column list and bind list.

**Exit:** every facet and operation emitter is a row plus named fragments;
`FUNCTIONS` holds only SQL lowering and the proofs; the `format!` count is
the fragment renderers' and the SQL lowering's alone.

## What stays exactly as it is

The executor and its crash proof; the three-way merge and the lock's
BASE/OURS/THEIRS rule; `PlanBundle`, `PlannedOperation` (six kinds is right),
`ProjectPath`, `ContentDigest`; `DerivedValue` and `jails model explain`;
`BuiltinSemantics` as the one type table; the marked block; the templates as
`.java` files. None of these is where the shapes multiplied.

## Where each plan meets this

| item | plan | step |
|---|---|---|
| S60.3 `Recipe` | 55 | S55.2 (the shell) and S55.5 (packs as data) are its first two rungs |

A plan step that lands a deletion without moving toward one of these is
still worth landing; a step that adds a *new* shape -- a fourth vocabulary, a
`_with_x` variant, a second project reader -- is not, whatever it deletes.
