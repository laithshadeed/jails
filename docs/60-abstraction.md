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
   one-file facets, the operation ports, the three storage adapters, the
   Kafka slice of an event and a command's outbox are rows, and what a
   template cannot say is a named fragment renderer
   (`emit_java/fragment.rs`, `emit_java/operation.rs`,
   `emit_java/storage.rs`). `emit.rs` holds two tables: four recipe walks
   and five function passes, and the five are what still builds Java from
   the model's *structure* with `format!` -- the multi-file facets, a
   query's SQL, a proof's request (657 sites). **An emitter is a row when
   its structural blocks are independent answers about one node**, because
   that is what a `Fragment::Rendered` -- one function of the model and the
   node -- can be; it is a function when its blocks are several outputs of
   one pass over the fields. S60.3 has the count and what is left.
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
selector, expected version and `Input` record; `emit_java/storage.rs`
renders a table's column list, an insert's column and value lists, its `on
conflict` clause and its bind chain, the primary key's Java type in the
boxed spelling a type argument needs, and the `@Component` the in-memory
adapter carries only when nothing else implements the port. The templates
around them are real `.java` files under `templates/spring/` (`entity_*`,
`operation_*`, `repository_*`, `search_jdbc`, `enum_converter`).

Rows today: the 22 capability packs (`Recipe<Capability>`), twelve
component kinds (`Recipe<Component>`), the entity's one-file facets --
record, enum, repository port, service, events port, search port -- plus
the test-data builder and the enum's Spring converter (three
`Recipe<Entity>`, one per compiler pass), the four operation ports and the
event's record (`Recipe<Operation>` in `emit_java/operation.rs`), the Kafka
slice of an `event` and a command's outbox (`Recipe<Operation>`), and the
three storage implementations -- the in-memory repository adapter, the JDBC
one and the JDBC search adapter (three `Recipe<Stored>` in
`emit_java/storage.rs`, again one per compiler pass). Of the 39 generator
kinds, 18 are wholly rows (the twelve components, `record`, `value`,
`factory`, `enum`, `event`, `search`); five are a row for the port and a
function for what implements it or proves it (`repo`, `usecase`, `query`,
`transition`, and `scaffold` through its `http` facet); the rest are
functions. `emit.rs` walks two tables -- `RECIPE_WALKS` (four) and
`FUNCTIONS` (five) -- and a unit test pins the lengths; `emit_java::emit`
stays in `FUNCTIONS` because it hosts the entity, port and storage recipe
walks *and* the functions below.

**What a node is, is what unblocked the adapters.** The two reasons the
storage adapters were not rows were both facts about *a stored entity*
rather than about an entity: which owner the adapter belongs to and which
one is the project's bean come from `emit::jdbc_on_classpath`, a fact of
the captured build, and a storage-scoped artifact id is
`art_<storage>_<entity>_<role>`, which the loop's `art_<node>_<role>` does
not spell. `Stored` carries both -- the owner the caller resolved, the id
prefix that owner implies, and the bean decision -- and the recipe shape
did not change at all: `Node::provenance` is the node's, so the three
provenances the emitters wrote out by hand are one match on which adapter
this is.

**The criterion an emitter is measured against, and it is the fragment's
signature.** A `Fragment::Rendered` is one named function of `(&AppModel,
&Node)`, independent of every other fragment on the recipe. So an emitter
is a row exactly when its structural blocks are *independent* answers about
one node, and it stays a function when either of two things is true: its
blocks are several outputs of one pass over the fields -- two fragments
recomputing one pass is the drift the fragment renderers exist to remove,
and it is worse than the `format!` it replaced because the duplication is
no longer visible in one function -- or a block needs a fact the signature
does not carry, which in practice is the captured Boot major.

**What remains, and why each is still a function.**

- `emit_dto` -- the `dto` facet's request, response and contract test.
  Fails both tests at once. The request's components carry validation
  annotations whose package a Boot major moved (`jakarta` vs `javax`),
  which is a *prefix* rather than a type and so is not an `Import::Moved`
  row either; and its `toDomain` argument list and the locals hoisted above
  it are two outputs of one hoisting pass, because `--timestamps` has to
  read the clock once for the whole row.
- `emit_resource_http` and `emit_seed` -- the other two multi-file facets.
- `emit_unit` -- `class`, `interface`, `service`, `controller`, `sealed`,
  `strategy`, `test`, `integration-test`.
- `emit_operation` -- the command, query and transition adapters, **and
  this is what the exit means by SQL lowering.** A command's insert column
  list, its bind chain, its answer type and the collaborator it holds are
  four outputs of one ladder over the target's fields -- a declared
  assignment, then a `@scope` field, then `updated`, then a minted `uuid7`,
  then any other default -- and the order of those arms is what decides
  where a value comes from. Splitting it into four fragments would have
  four functions walk that ladder and agree by luck.
- `emit_relation` -- one integration proof per relation: the catalogue
  query that says the constraint is there and which ordered pairs it holds,
  and the rejected insert that says the database enforces it. One file, no
  adapters; the earlier description here of "the join table and both sides'
  adapters" was stale.
- `emit_http` -- the controller of every routed operation and its HTTP
  proof. The proof drives a request through `emit_mockmvc` from a sample of
  the operation's input, which reaches across nodes; the controller's shape
  is conditional on the binding, the precondition and the status set at
  once.
- `emit_architecture` -- one file per *model*, not per node, which the
  recipe's node-per-row shape does not express.

Beside them, two component kinds stay functions inside the component
walk (`http_sink`, `durable_job`: each renders a sample argument list from
*another* node's fields), and three model-level shared files
(`SchedulingConfig`, `ApiError`, the architecture test) are one file per
model rather than per node. The repository contract and the two tests that
call it stay functions in `emit_java/repository.rs` for the `emit_http`
reason: each reaches across nodes, for the record arguments a proof
constructs and for the ancestor rows a foreign key demands before a child
can be stored.

The number to read is the `format!` count, **less the refusals**, because a
refusal message is prose and will always be built with `format!`:

```
all=$(grep -rho 'format!(' crates/jails-compiler/src | wc -l)
refusals=$(grep -rhoE 'Diagnostic::new\(|refuse::' crates/jails-compiler/src | wc -l)
echo $((all - refusals))
```

657 now: 857 `format!` less 200 refusal sites. The whole-crate figure was
834 when this file was first measured, 808 at the start of the `Recipe`
work, 762 before the fragment renderers and 723 after them; it then rose to
865 when A3.13 gave every compiler refusal a code and a `fix:` line of its
own, which is why the subtraction is now part of the measurement.

**And it is a weak number, which the storage adapters proved.** Moving
three whole Java classes out of Rust and into templates -- some 120 lines
of Java that had been living inside `format!` strings with every brace
doubled -- moved the count by two, because a class body is *one* site
however long it is. The count still only falls, so it is worth keeping as a
ratchet, but the thing it is a proxy for is Java assembled in Rust, and the
proxy is loose. What the row conversion actually bought is one shell, one
path rule, one provenance rule and one import rule for three more files.

**Exit:** every emitter whose structural blocks are independent per-node
answers is a row plus named fragments, and `FUNCTIONS` holds only the SQL
lowering (`emit_operation`), the proofs (`emit_http`, `emit_relation`, the
repository contract and its two tests) and the model-level files
(`emit_architecture`). Reaching it means the four in `emit_java` -- `dto`,
`http`, `seed` and the units -- and the first of those needs
`Fragment::Rendered` to carry the captured Boot major, which is a change to
the recipe shape and should be made for `dto` or not at all.

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
