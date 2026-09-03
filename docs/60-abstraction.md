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
   still functions, and which side an emitter falls on is decided rather
   than pending.** `Recipe<N>` (`jails-compiler::recipe`) is files,
   dependencies, properties, compose services, build features and a
   placement rule as one `static`, over a `Node` -- a capability, a
   component, an operation, an entity, a stored entity or a source unit --
   and `recipe::render` is the one loop. The 22 capability packs, twelve
   component kinds, the entity's one-file facets, the operation ports, the
   three storage adapters, six of the eight source-unit kinds, the Kafka
   slice of an event and a command's outbox are rows, and what a template
   cannot say is a named fragment renderer (`emit_java/fragment.rs`,
   `operation.rs`, `storage.rs`, `unit.rs`). **A `Fragment::Rendered` is one
   named function of `(&AppModel, &Node)`, independent of every other
   fragment, and a recipe's `files` is a static list of one file each.** So
   an emitter is a row when its structural blocks are independent per-node
   answers, and it stays a function on one of three gates: its blocks are
   several readings of one pass over the fields (`dto`'s hoist, `seed`'s
   sampling, the record's companion test, an operation adapter's ladder); a
   block needs a fact the signature does not carry, in practice the captured
   Boot major (`dto`'s validation package, every MockMvc dialect); or the
   files it writes are not one per row (`strategy`, one per variant;
   `emit_architecture`, one per model). `emit.rs` holds the two tables and
   each remaining module's own doc names its gate. The widening of
   `Fragment::Rendered` to carry project facts was weighed for `dto` and
   declined: a captured fact can already ride on the *node*, which is how
   `Stored` carries the owner and the bean, and `dto` fails the hoist gate
   as well -- so the change would make 25 fragments' signatures wider and
   still leave `dto` a function.
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

## What stays exactly as it is

The executor and its crash proof; the three-way merge and the lock's
BASE/OURS/THEIRS rule; `PlanBundle`, `PlannedOperation` (six kinds is right),
`ProjectPath`, `ContentDigest`; `DerivedValue` and `jails model explain`;
`BuiltinSemantics` as the one type table; the marked block; the templates as
`.java` files. None of these is where the shapes multiplied.

## What a plan step is measured against

Every numbered `S60.n` item is closed, so this file is now the five nouns and
the four verbs and nothing pending. A plan step that lands a deletion without
moving toward one of these is
still worth landing; a step that adds a *new* shape -- a fourth vocabulary, a
`_with_x` variant, a second project reader -- is not, whatever it deletes.
