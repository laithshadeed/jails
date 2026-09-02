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
| `jails-model` | 139 | 43 are `source.rs`, the unlinked wire shape the parser builds and the linker consumes -- a deliberate second copy, held by nothing else |
| root binary | 63 | 18 are clap argument types; the rest are per-command request bags |
| `jails-contracts` | 44 | |
| `jails-drive` | 38 | two test-execution vocabularies (`testing::*V1`, `testd::*V2`) |
| `jails-project` | 31 | a second project model (`model::{Project, Layer, Layers, Artifact, Change}`) beside the snapshot's `ProjectFacts` |
| `jails-spec` | 14 | six closed vocabularies that also exist in `jails-model` |
| `jails-compiler` | 10 | |
| `jails-workspace` | 4 | |

Six specific shapes, each measured:

1. **One mutation is decided twice.** Every frontend edits the JDL text
   (`next_source`) and builds a `ModelPatch` (34 variants) for the same
   change; `PreparedMutation` carries both and the plan records the patch
   serialised. The compiler then applies the patch to the model it parsed
   from the *old* source while the plan replaces the source file with the
   *new* text -- two routes to the same next model, and a re-parse of the
   edited source at 24 sites to pull the linked declaration out for the
   patch.
2. **Closed vocabularies exist in two or three crates each.** `Layer` is
   defined in `jails-model`, `jails-spec` and `jails-project`. `Capability`,
   `Dialect`, `HttpMethod`, `WireFormat`, `ArtifactKind` are `clap::ValueEnum`s
   in `jails-spec`; the model spells the same sets as `CAPS`, `storage`,
   `EndpointMethod`, `RequestFormat`, `UnitKind`/`ComponentKind`/`ProjectionKind`.
   `Build` is in `jails-spec` and `BuildSystem` in `jails-contracts`. Every
   pair has a translation and a test that they
   agree, and `the_compilers_renameable_layers_are_the_engines_layers` exists
   only because there are two.
3. **Generators are code, capabilities are data.** A capability is a `Pack`:
   files, dependencies, properties, compose services, build features and a
   placement rule, as one `static`. A generator kind is a `lower_and_emit`
   function -- nine of them, plus six `lower`s -- each walking the model and
   assembling Java with `format!` (834 sites). The two shapes emit into the
   same `RenderedTree`.
4. **The compiler re-derives its external facts.** `emit::Observed` is a
   hand-picked subset of the snapshot, rebuilt in `Compiler::compile`; every
   emitter that needs one more fact widens it.
5. **The tool crates keep a second project model.** `jails-project::Project`
   and `ProjectContext` answer "what is at this path" for `drive` and
   `report`, reading the disk again, while the snapshot's `ProjectFacts`
   answers the same question for the compiler.
6. **Entry points come in families.** `capture`, `capture_planned` and
   `capture_import` remain of a family of four; `materialize` and
   `finish_generation` are one function each now, taking the model update
   and the reader paths as arguments. The binary's `_at` family
   (`compile_at`, `load_model_at`, `resolve_manifest_at`, `sync_at`,
   `read_source_at`, `owns_at`, `replay_at` and kin, nine functions) is the
   same observation: each is one caller's exception promoted to API.

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
`--plan-in` starting at `execute`. That composition is `finish_generation`
today; the change is what it takes.

### S60.1 — `Edit` replaces `ModelPatch` and the text splices

An `Edit` is a syntactic operation on the CST, from a closed set small enough
to list: append a declaration, remove a declaration, set or clear an
attribute or property, add or remove a member, rename an identifier, replace
one member. Every frontend becomes a function from its arguments to an
`Edit`, and `edit` is the one place JDL text is rewritten -- byte-preserving
outside the touched span, which `jdl/v1/edit.rs` already does.

The model is then whatever the edited source parses and links to. The plan
records the source before-image and after-image (`ReplaceModelFile` already
does), which is the only "patch" the executor needs. `ModelPatch`,
`AppModel::apply`, `model_apply.rs` and the `Batch` plumbing go;
`CanonicalModelPatch` records the `Edit` and the `Evolution` instead of the
patch. `Batch` is a `Vec<Edit>`.

What does not fit in the source is **`Evolution`**: a one-shot instruction
about how to get from the accepted model to the next one, which is not
desired state and must not be written into the file. The list is short and
closed -- rename-column policy (`preserve` / `single-cutover`), type-change
strategy (`safe`), storage retirement (`preserve` / `drop`) and its
confirmation, index removal's confirmation -- and it is one enum passed to
`compile` beside the model, where `emit_sql::derive` reads it. Seven
`ModelPatch` variants exist only to carry these; they become `Evolution`'s
variants and nothing else changes.

**Exit:** `ModelPatch` is gone; `Compiler::compile` takes a `&AppModel` and
an `Evolution`; `PreparedMutation` carries the edited source and the
evolution and nothing else about the change.

### S60.2 — one owner per closed vocabulary

Every closed set lives in `jails-model` and nowhere else: layers, capability
kinds, artifact kinds, dialects, HTTP methods, wire formats, build systems,
platforms. The CLI's `clap::ValueEnum`s are those enums, not copies: the
model crate gains a `cli` feature that derives `ValueEnum` on them, or
`jails-spec` becomes a generated table read *from* `jails-model`'s `ALL`
lists. Either way there is one list and a `label()` per member.

`jails-spec` then holds two things the model does not: where a project is
(`find_project_root`, `build`) and the compact field syntax's parser -- and
the parser's output is a model `Field`, so it moves next to
`BuiltinType::from_alias` (S53.4, S54.5). What remains of `jails-spec` is
small enough to question.

**Exit:** one definition of each set; the tests that check two copies agree
are deleted with the second copy.

### S60.3 — `Recipe` generalises `Pack` to every kind

`Pack` is the right shape and it is used for 25 of the 25 capabilities. The
39 generator kinds are functions because each was written when its kind was
added. A `Recipe` is a `Pack` that can also name the *model node* it renders
from and the *roles* it emits:

```text
Recipe {
    node:   which model node kind this renders (entity facet, unit, component, operation, capability)
    files:  [(role, template, placement, ejectable, test?)]
    deps:   [DependencySpec]         props: [PropertySpec]
    build:  [BuildFeature]           compose: [ComposeService]
    when:   BootCondition
}
```

Rendering is one loop: for each node, look up its recipe, render each file
through the one `JavaUnit` builder (S55.2) with the node's typed values as
the template's keys. What a template needs that is *structural* -- a column
list, a parameter list, a switch over variants -- is a small closed set of
fragment renderers, named in the recipe rather than written inline. The
emitters that cannot be a recipe (SQL lowering, the proof tests, the
dispatcher splice) stay functions, and the count of those is the number to
watch: the target is under ten.

This is also A3.15's registry: a role appears in exactly one recipe, so
`Entity.repo.fake` resolves by looking it up, and an emitter asking for an
unregistered role fails to compile.

**Exit:** `emit.rs` walks one table; `lower_and_emit` is gone; the
`format!` count is the fragment renderers' and the SQL lowering's alone.

### S60.4 — the snapshot is the only project model

`emit::Observed` goes: the compiler reads `snapshot.project` directly, and a
fact it needs that the snapshot lacks is added to `ProjectFacts` by capture.
`jails-project::model::{Project, Slice, Layer, Layers, Artifact, Change}` and
`ProjectContext` go: `jails-project` becomes the *reader* that produces
`ProjectFacts` and captured files (today's `capture/observe.rs` and the
document adapters), and `jails-drive`/`jails-report` take a snapshot -- or a
`Project` that is nothing but a root plus a lazily captured snapshot. A
command that starts a JVM has no reason to parse `pom.xml` its own way.

**Exit:** one `struct` answers "what is this project"; no module outside
capture reads the pom, and the board's Maven-scanner row reads one.

### S60.5 — one entry point per verb

`capture(root, model, reader_paths)`; `materialize(snapshot, desired)`;
`mutate(invocation, edit, evolution)`. Variants become arguments with
defaults, not functions. `materialize` and `finish_generation` are there;
`capture_planned` and `capture_import` differ from `capture` by one argument
each (the intended model, the model-absent precondition) and become one.
The `_at` family in the binary is the same observation: `Invocation` already
carries the root, and `Current::load` reads it, so a frontend never needs a
root-taking twin.

**Exit:** the `pub fn` count in `jails-workspace` is four; the binary has no
`_at` function.

### S60.6 — one test-execution vocabulary

`jails-drive::testing` (`TestExecutionPlanV1`, `TestReportV1`, ...) and
`testd::v2` describe one thing -- select tests, run them through an engine,
report cases -- as two versioned protocols. One `TestPlan` and one `TestReport`,
with the daemon's wire framing as an encoding of them rather than a second
model.

### S60.7 — managed output lives in `src/`, and the lock says what is managed

JDL v1 §9.7 places every layer under `src/main/java` or `src/test/java`. The
code places all of it under `.jails/generated/{main,test}/{java,resources}`
and `.jails/generated/requests`, then edits the reader's build file so the
build can find it: a `build-helper-maven-plugin` block on Maven, a source-set
block per root on Gradle (`documents/source_root.rs`,
`DocumentIntent::EnsureMavenSourceRoots`). That is the code diverging from the
language, and it costs the product its own premise. `docs/00-contracts.md`
§1.2 rejected the disposable tree because "a tree nobody may edit is a tree
nobody trusts", and D1 says the reader edits generated files and the merge
keeps the edits. A dotfolder is hidden by IntelliJ, VS Code, `ls` and
ripgrep by default, and its name says *do not edit*. So the design asks for
edits in a place built to discourage them, splits one Java package across two
source roots, and makes every reader of source learn a second tree
(`inspect/roots.rs`, the storage wiring check in `doctor`, `jails src`).

The path does exactly one job in the code: it answers "is this file jails'"
with a prefix test (`MANAGED_ROOT` in the compiler, `ejectable.rs`, the
source-root splice, the managed walk in `capture`). The lock,
`.jails/compiler.lock.json`, already holds the accepted projection as a
`RenderedTree` keyed by path with the BASE bytes of every managed file, so the
answer is available without the prefix: **a file is managed if the accepted
projection names its path.** Nothing in the merge depends on where the file
is. BASE is the accepted render, OURS is the captured file, THEIRS is the next
render, at whatever path.

What changes, in order of dependency:

1. **`RenderedTree.root` goes.** The projection is a set of project paths; the
   compiler stops checking `baseline.root != root` and `ejectable.rs` stops
   translating `.jails/generated/main/java/…` into `src/main/java/…`, because
   the emitted path *is* the reader path. Every emitter's `*_ROOT` constant
   collapses onto the §9.7 table, which then has one owner (S60.2's rule).
2. **Capture reads the lock first and walks the paths it names**, plus the
   reader trees `ReaderTrees` already selects. Today's wholesale walk of the
   managed root becomes a walk of the accepted projection's paths, and the
   only new observation is a reader file at a path the next render wants
   that the lock does not own: a collision, refused with the file named, which
   is the `create` verb's contract and the eject collision rule stated once
   more. `verify_preconditions`' "unmanaged file inside the managed tree"
   refusal becomes this check; `sweep_staged` sweeps the parents of the
   bundle's paths, which it already does for reader files.
3. **Ejection is a lock edit, not a move.** `eject <boundary>` removes the
   boundary's artifacts from the accepted projection and records the `eject`
   declaration; the files stay where they are. The `Missing` before-image
   rule and "transfer is creation" go, because there is no transfer.
4. **`check --frozen` compares the lock's paths**, not a directory. A managed
   file the reader deleted is a difference; a reader file beside a managed one
   is not.
5. **Delete** `documents/source_root.rs`, `EnsureMavenSourceRoots`, the Gradle
   source-set block, the second root in `inspect/roots.rs` and `doctor`, and
   the `ProjectPath::parse(".jails/generated/…")` block in the compiler.
   `requests/*.http` moves beside the tests, under `src/test/http`.
6. **Each managed file carries one header line naming its artifact ID**
   (`// jails: art_…`, part of BASE, so an edit to it is an ordinary edit).
   `ls .jails/generated` was the way to see what jails owns; `jails model
   status` listing the lock is the replacement, and the header is the answer
   from inside the file.

What it does not change: the three-way merge, the lock's BASE/OURS/THEIRS
rule, the executor and its crash proof, `PlanBundle`. A project generated
before this needs one migration, `jails model relocate`: move
`.jails/generated/<set>/<kind>/…` to `src/<set>/<kind>/…`, rewrite the paths
in the lock, remove the marked source-root block from the build file, and
refuse if any destination exists. `tests/golden/**` is regenerated and the
diff is read.

**Exit:** no path under `.jails/` holds Java, SQL or resources; the string
`.jails/generated` appears in `jails model relocate` and nowhere else; the
board's Maven-scanner row counts one fewer marked block; a generated project
opened in an IDE shows one source root per source set.

## What stays exactly as it is

The executor and its crash proof; the three-way merge and the lock's
BASE/OURS/THEIRS rule; `PlanBundle`, `PlannedOperation` (six kinds is right),
`ProjectPath`, `ContentDigest`; `DerivedValue` and `jails model explain`;
`BuiltinSemantics` as the one type table; the marked block; the templates as
`.java` files. None of these is where the shapes multiplied.

## Where each plan meets this

| item | plan | step |
|---|---|---|
| S60.1 `Edit` and `Evolution` | 52, 54 | S54.2 supplies `Edit` and `Evolution`; S52.1 makes each frontend a function to one |
| S60.2 one vocabulary | 51, 53, 54 | S51.2 moves survivors; S53.4 and S54.5 move the field parser; the `Layer` triple is S53.1's first deletion |
| S60.3 `Recipe` | 55 | S55.2 (the shell) and S55.5 (packs as data) are its first two rungs |
| S60.4 the snapshot | 53 | S53.2, S53.3 |
| S60.5 one entry point | 53 | S53.7 |
| S60.6 one test vocabulary | 53 | S53.5 |
| S60.7 managed output in `src/` | none yet | after S60.2 (one owner for the §9.7 table) and S60.4 (capture is the one reader); needs `jails model relocate` |

A plan step that lands a deletion without moving toward one of these is
still worth landing; a step that adds a *new* shape -- a fourth vocabulary, a
`_with_x` variant, a second project reader -- is not, whatever it deletes.
