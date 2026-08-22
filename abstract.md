# abstract.md — the abstractions, judged against the design canon

`plan.md` says what to build next. `CLAUDE.md` says what the code is and what
traps are in it. This file says what the code *should have been*, and is allowed
to disagree with both.

It is not a task list. §7 is a ladder, but the point of the document is §2–§5:
a diagnosis in a vocabulary older than this repo, so the next person can apply
the same test without rediscovering it.

---

## 0. The one sentence

jails is **a function from an intent to a change in a project, plus that
function's inverse and its verifier.**

```
plan   : (Project, Intent) -> Change      -- pure
apply  : (Project, Change) -> Report
revert : (Project, Change) -> Report
verify : (Project, Change) -> [Finding]
```

Everything the tool does is one of those four. `g scaffold` is plan+apply.
`destroy` is plan+revert. `--pretend` is plan+describe. `add` is plan+apply.
`remove` is plan+revert. `sync` is plan+apply over a recorded set. `doctor` is
verify. `app apply` is plan+apply over a list.

**None of those four exists as a function in `src/`.** There are three apply
paths, two hand-written reverts, and a verifier that shares no code with
anything it verifies. Everything below is evidence for that sentence, a
vocabulary for why it happened, and a target.

---

## 1. The canon, and which half of it applies to Rust

The useful core of object-oriented design is not classes. It is three ideas that
predate and outlive them:

**1. Information hiding — Parnas, *On the Criteria To Be Used in Decomposing
Systems into Modules* (CACM, 1972).** A module is not a step in the process; it
is a **design decision hidden from everyone else**. Parnas's KWIC example gives
two decompositions of one program: by processing step (input → circular shift →
alphabetize → output) and by secret (how lines are stored, how shifts are
represented). The flowchart decomposition looks natural and is wrong, because a
change to one decision touches every module.

**This is the single most important idea for jails, and jails is split both
ways at once.** §3 shows that the modules named after a *secret* (`pom.rs`,
`compose.rs`, `java.rs`, `process.rs`, `template.rs`, `config.rs`) are the
healthy ones, and the modules named after a *step* (`generate.rs`, `add.rs`,
`doctor.rs`) are the mess. The repo already proved Parnas right and did not
notice.

**2. Responsibility-driven design — Wirfs-Brock, *Object Design* (2002).** Ask
what each part *is responsible for*, and give it one **role stereotype**:
information holder, structurer, service provider, controller, coordinator,
interfacer. A part with three stereotypes is three parts.

**3. Encapsulate what varies — GoF (1994), restated by Martin as OCP and by
Cockburn as ports & adapters.** Find the axis of change and put a value on it.
jails' axis of change is *the set of recipes*; that axis currently runs straight
through eight files.

Supporting instruments, each used by name below:

- **Cohesion / coupling scales — Yourdon & Constantine, *Structured Design*
  (1979).** Cohesion, worst to best: coincidental, logical, temporal,
  procedural, communicational, sequential, functional.
- **Connascence — Page-Jones, *What Every Programmer Should Know About OOD*
  (1995).** A graded scale of coupling (name < type < meaning < position <
  algorithm < value < identity) with three rules: minimise total connascence,
  minimise connascence that **crosses** an encapsulation boundary, maximise
  connascence **within** one. It is the only vocabulary here that ranks two
  couplings against each other, which is what a refactor queue needs.
- **Smells and their named cures — Fowler & Beck, *Refactoring*.** Long
  Parameter List, Data Clumps, Primitive Obsession, Repeated Switches, Shotgun
  Surgery, Divergent Change, Feature Envy. Cures: Introduce Parameter Object,
  Preserve Whole Object, Extract Class, Replace Conditional with Polymorphism.
- **Deep vs shallow modules, information leakage, temporal decomposition —
  Ousterhout, *A Philosophy of Software Design* (2018).** A good module has a
  small interface over a large implementation. **Temporal decomposition** —
  organising code by the order in which operations happen — is his named
  anti-pattern, and it is `generate.rs`.
- **Command with undo, and Interpreter — GoF.** An operation reified as an
  object that can `execute` and `unexecute`. This is exactly `Change`, and the
  reason `destroy` is hand-written is that jails has the operation as *code*
  rather than as an *object*.
- **The counterweight — Sandi Metz, *The Wrong Abstraction* (2016).**
  "Duplication is far cheaper than the wrong abstraction." Taken seriously in
  §8; it is the strongest argument against this document and it has a
  falsifiable answer.

**The half that does not translate.** Rust is not Smalltalk and this is a CLI,
not a framework. Explicitly rejected: inheritance hierarchies and Template
Method (use enums and functions); Visitor (Rust's `match` *is* double dispatch);
Abstract Factory and DI containers (a `Recipe` enum is enough); getter/setter
objects and mutable object graphs (values and `&`); a `ChangeApplier` class
(a `Change` value plus a free `apply` is the same thing without the ceremony).

The Rust-native expressions of the ideas that do translate:

| OO idea | Rust form |
|---|---|
| Encapsulate what varies | enum + exhaustive `match`, checked by the compiler |
| Replace Type Code with Polymorphism | data-carrying enum variants, or a table of values |
| Command with undo | a `Change` value + `apply`/`revert` |
| Introduce Parameter Object | a struct; the compiler then finds every call site |
| Make illegal states unrepresentable (Minsky) | `Ref { name, expect: Referent }` instead of `Option<String>` |
| Parse, don't validate (King) | `plan()` returns `Change` or an error — never a half-applied project |
| Information hiding | one `mod` per secret, `pub(crate)` as the boundary |

---

## 2. The test, applied

*If a concept is real, adding one instance of it is one edit.*

| Concept | Edits to add one | Smell |
|---|---|---|
| A `why` rule | **1** — a row in `RULES` | — |
| A field type | 2 (`generate/field.rs`, `sql.rs`) | Divergent Change |
| A capability | ~4 (enum, `label()`, submodule, `build_plan` arm) | Shotgun Surgery |
| A `Plan` field | **3, and `doctor` still cannot see it** | Shotgun Surgery |
| An artifact kind | ~8 (enum, generate arm, `KIND_FILES` row, dep `match`, destroy cases, scenario, Lua lists, README) | Shotgun Surgery + Repeated Switches |

`why.rs` is the only clean one, and it is the only place the concept is **a value
in a table** rather than **an arm in a `match`**. That is not a coincidence; it
is Replace-Conditional-with-Polymorphism already done, once, by accident of
`why` having been written last.

---

## 3. `src/` is a mess — the audit, and the one line that explains it

35 modules, 33,079 lines. Judged by cohesion (Yourdon–Constantine) and role
count (Wirfs-Brock). Line counts exclude `mod tests`.

### 3.1 The healthy modules — each hides exactly one secret

| Module | Prod LOC | Secret it hides | Cohesion | Roles |
|---|---|---|---|---|
| `process.rs` | 312 | how a tool is found and run, and that secrets are never printed | functional | interfacer |
| `template.rs` | 119 | how `{{key}}` becomes bytes | functional | service provider |
| `java.rs` | 604 | `blanked()` — how to scan Java without a parser | functional | service provider |
| `pom.rs` | 678 | how `pom.xml` is spliced without disturbing comments | functional | interfacer |
| `compose.rs` | 599 | the same for `compose.yaml` | functional | interfacer |
| `config.rs` | 424 | the same for `jails.toml`; owner of `LAYERS_IN_ORDER` | functional | information holder |
| `why.rs` | 744 | log signature → cause, as data | functional | information holder |
| `sql.rs` | 757 | the field → SQL/JDBC projection | functional | service provider |

**Every one of these is a Parnas module**, and every one is fine. They are also
the modules nobody complains about. This is the repo's own controlled
experiment, and it already returned a result.

### 3.2 The unhealthy modules — each is named after a step, not a secret

| Module | Prod LOC | Cohesion | Roles it plays | Verdict |
|---|---|---|---|---|
| `spring.rs` | 6,621 | **logical** — everything sharing the `require_spring` precondition | information holder (dep constants), service provider (39 inline Java bodies), interfacer (`pom::read` mid-render), coordinator (14 `*_files`) | worst module in the repo |
| `generate.rs` | 3,013 | **temporal** — parse → dispatch → write → side effects | Ousterhout's named anti-pattern, verbatim |
| `add.rs` | 940 | **procedural** — the apply sequence | holds the one good type (`Plan`) and no interpreter for it |
| `doctor.rs` | 1,365 | logical | Feature Envy on `add`'s knowledge (§4.2) |
| `project.rs` | 292 | **coincidental** — the worst rung | see below |

**`project.rs` is the clean specimen.** It contains `ProjectContext` — reactor,
module, java release, spring_boot, maven command — which is *precisely* the value
§5 says the codebase needs. It is referenced **nowhere outside its own file**;
`generate.rs`, `add.rs`, `spring.rs` and `doctor.rs` contain zero mentions. The
same file also holds `json_string`, a JSON escaper, used by `why.rs`,
`inspect.rs` and `add/tooling.rs` because there is no `json.rs`.

One module, two unrelated secrets, and the useful one unused. That is
coincidental cohesion, and it happened because the module was named for a
**noun in the domain** rather than for **a decision it hides**.

### 3.3 The line that explains the whole mess

> **The modules that hide a file format or a tool are excellent. The modules
> that own a command are a mess.**

`generate.rs`, `add.rs`, `doctor.rs`, `run.rs`, `new.rs` are decomposed by
processing step — Parnas's flowchart criterion. `pom.rs`, `compose.rs`,
`java.rs`, `process.rs` are decomposed by secret. The first group has produced
every duplication in §4; the second group has produced none.

"`src` is a mess" is therefore not a size problem and not a tidiness problem.
It is **one wrong decomposition criterion applied to five files.**

---

## 4. Five abstractions that went the wrong way

### 4.1 `Vec<Artifact>` is a Command object with the undo amputated

`add` found the right idea; `generate` never got it. `add::Plan` is a value —
deps, plugins, files, compose, properties, legacy_deps, test-import — computed
before anything is written. That is why `preflight` can exist and why `remove`
can be an inverse at all.

`generate` returns `Vec<Artifact>`: *files only*. Everything else a generator
needs was bolted onto the tail of the write path as statements:

```
generate.rs:2230   if let Some(dep) = match kind { Dto|Scaffold => …, Client => …, Fetcher => … }
generate.rs:2269   if matches!(kind, Command) { register_command(…) }
generate.rs:2279   match kind { Dto|Scaffold => ensure_dependency(…), Event => … }
generate.rs:1012   ensure_failsafe(root, &artifacts)
generate.rs:1049   ensure_assertj(root, …)
```

Those are `Plan.deps`, `Plan.plugins` and a missing `Plan.edits`, written
longhand, in another module, with no inverse. **A Command object that cannot
`unexecute` forces you to write the undo separately** — which is exactly
`KIND_FILES`, a second transcription of the file list.

And there are **four** shapes for "a file to write":

| Shape | Where |
|---|---|
| `Artifact { kind: &'static str, path, contents }` | `generate.rs:1120` |
| `NewFile { path, contents }` | `add.rs:133` |
| `SpringSlice.files: Vec<(PathBuf, String)>` | `spring.rs:21` |
| `Vec<(PathBuf, String, &'static str)>` | 14 `*_files` fns in `spring.rs` |

Two of them live in the *same file*. `spring.rs` holds capability slices and
generator outputs side by side in different shapes — the strongest available
evidence that they are one concept nobody named.

*Smell:* Divergent Change + Repeated Switches. *Cure:* Extract Class (`Change`)
+ Command-with-undo.

### 4.2 `Plan` is data with no interpreter — and `doctor` is Feature Envy

`Plan` has 8 fields. Each is handled three times: `add`'s apply, `add`'s
`if dry_run` branch, `remove`'s inverse. Two independent implementations of one
traversal plus a mirror that must be kept exact by hand.

`doctor.rs` contains **zero** references to `Plan` or `build_plan`. It is 1,365
lines re-deriving, by reading the project back off disk, the facts
`add/database.rs`, `add/messaging.rs` and `add/data.rs` already own: which
dependency, which property, which test wiring, which compose service.

That is Feature Envy at module scale: `doctor` wants `add`'s knowledge and, being
unable to hold it, re-implements it. And it means the drift `tests/agreement.rs`
catches between `generate` and `destroy` has an exact sibling between `add` and
`doctor` that **nothing catches**, because there is no shared value to compare.

Two symptoms that read as separate bugs and are one:

- `--pretend` and apply are different code, so `--pretend` has been wrong
  before (`package-info.java`: two files named, three written —
  `generate.rs:2189-2192`).
- `dry_run || pretend` appears at **5** call sites in `main.rs`. Two names for
  one boolean, OR'd at dispatch, because the global flag and the per-command
  flag reach two different implementations. *Connascence of meaning*, crossing
  a module boundary — Page-Jones's rule 2 violated in the smallest possible way.

### 4.3 Primitive Obsession and a Data Clump, at scale

**188 function signatures take `root: &Path`.** From that one primitive, the
same facts are rediscovered:

| Fact | Call sites |
|---|---|
| `find_project_root()` | 31 |
| `pom::read()` | 21 |
| `pom::has_dependency()` | 19 |
| `pom::flavor()` | 15 |
| `fields_from_record()` | 15 |
| `Config::load()` | 14 |
| `base_package()` | 12 |
| `mockmvc_autoconfigure_import()` | 7 |
| `webmvc_test_import()` | 5 |

There is no `Project` value in use — there is a directory and 188 functions that
each re-derive what they need. (`ProjectContext` exists and is unused; §3.2.)

The layer packages are the acute case: because `Layers` is not a value, the
package names travel one at a time, and **16 functions in `spring.rs` take 8–11
parameters**:

```rust
usecase_files(root, security, service, web, domain, app, adapters, name, target, fields)
//            ^^^^  ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^  one value, spelled six times
durable_job_files(… 11 params)   outbox_files(… 11 params)
transition_files(… 10 params)    query_files(… 10 params)
```

This is a textbook **Data Clump** (the same six values always travel together)
producing **Long Parameter List** and **connascence of position** at degree 10 —
the highest-cost coupling in Page-Jones's static ranking, replicated 16 times.
The cure has a name and has had one since 1999: **Introduce Parameter Object**.

The second-order damage is worse than the ugliness. Because a generator holds
`root`, it does I/O *while rendering* — `pom::read` inside `usecase_files`,
`json_sample(root, …)` inside a template renderer. **Rendering is not pure.**
That is Ousterhout's *information leakage*: the decision "how do I learn a fact
about this project" is smeared across every renderer instead of hidden behind
one. And it is precisely why artifacts cannot be lazy, why a path cannot be
computed without a body, and why `KIND_FILES` had to be typed by hand.

plan.md §6.2 B lists lazy rendering as a *cost* of deriving destroy paths. It is
not a cost; it is the *cause* of the knot, and it is one Introduce-Parameter-
Object away.

### 4.4 `--on` / `--yields`: a type code that never became a type

From `main.rs:180`:

> For `strategy`, the type each implementation examines. For `usecase`, the
> existing scaffolded resource the operation creates; for `query`, the
> scaffolded resource it reads; for `durable-job`, the existing generated use
> case it invokes. For `command`, the dispatcher to register it in.

One `Option<String>`, five referents, no type. This is Primitive Obsession in
its original sense — a domain concept represented by a built-in — and the domain
concept is **a typed reference from one artifact to another**: the dependency
edge between intents. That edge would let `usecase --on Note` fail usefully when
`Note` was never generated, let `destroy Note` warn that a use case points at it,
and let `app apply` order intents by dependency instead of by file order.

The naming damage is already public. Because the flag was invented for `strategy`
and reused, the key in `.jails/app.toml` — a **user-facing manifest format** — is
spelled `strategy_on` on a `usecase`:

```toml
[[generate]]
kind = "usecase"
name = "QueueCrawl"
strategy_on = "CrawlRun"     # not a strategy
```

An implementation detail of the first case became schema. In DDD terms the
*ubiquitous language* broke: the word in the file no longer names the thing.
That is what "the abstraction went in the wrong direction" looks like from
outside the codebase.

### 4.5 An anemic ledger, split seven ways

| File | Owner | Says |
|---|---|---|
| `jails.toml` `[project] capabilities` | `config.rs` | what the project *has* |
| `.jails/app.toml` `capabilities` | `app.rs` | what it *should have* |
| `.jails/app.toml` `[[generate]]` | `app.rs` | intents wanted |
| `.jails/app-state-v1` | `app.rs` | intents applied, keyed by **full-argument hash** |
| `.jails/intents/*.files` | `generated_files.rs` | paths per intent, keyed by **kind+name+package** |
| `.jails/files` | `generated_files.rs` | union of paths |
| `.jails/models` | `generated_files.rs` | field specs per record |
| `.jails/version` | `generated_files.rs` | jails version |

Two capability lists. Two intent registries **keyed differently** — and that
difference *is* the §9.7 bug rather than a nearby cause. `app-state-v1` keys on
`kind|name|package|fields|indexes|on|yields`, so editing a `fields` line makes a
new key; `.jails/intents/` keys on kind+name+package, so it still points at the
old files. The edited intent arrives as pending, `generate` refuses on files that
exist, and the two ledgers now disagree about one artifact.

**Identity is (recipe, name, package). Arguments are content, not identity.**
That is Evans's entity-vs-value-object distinction and nothing more exotic. Get
that one line right and §9.7 becomes an *update to a known entity* — which is
exactly the input plan.md §11.1's regenerate-and-3-way-merge needs and currently
has to reconstruct.

---

## 5. Why it happened — the pattern to stop repeating

Every step was locally correct:

1. `generate` came first and only wrote files, so `Vec<Artifact>` was right.
2. `add` came later, needed more than files, and correctly invented `Plan` — but
   as a **second** mechanism, because retrofitting `generate` was out of scope.
3. Generators then grew needs `add` already had, and got them as
   tail-of-function special cases, because `Plan` lived in the other module and
   was shaped for capabilities.
4. `destroy` needed the file list, could not call the generator (impure, §4.3),
   and got a transcription. `tests/agreement.rs` was written to police the
   transcription.
5. `app.rs` sat on both and needed a third notion of applied-ness.
6. `doctor` needed to check what `add` installs, could not reach `Plan`, and
   re-encoded it.

The pattern, stated so it is recognisable next time:

> **When a requirement did not fit the abstraction, the abstraction was cloned
> and a test was added to keep the clones honest.**

Checked duplication beats unchecked duplication — plan.md §6.1 is right about
that. But it is a holding action, and this is the sixth round. Page-Jones's rule
2 is the sharp version: `tests/agreement.rs` exists to police **connascence of
value crossing a module boundary**, and the canonical fix for connascence that
crosses a boundary is to move it inside one — not to measure it.

---

## 6. The target

Seven types. Four already exist under other names; one (`ProjectContext`)
already exists and is unused.

```rust
// ─── model/ — pure. No I/O, no `root: &Path`, nothing that can fail on disk.

/// Resolved once at the top. Replaces 188 `root: &Path` and ~120 re-reads.
/// This is `project::ProjectContext` finally being used.
struct Project {
    root: PathBuf, base: String, flavor: Flavor, boot: Option<u32>,
    layers: Layers,                 // config renames already applied
    pom: String,
    installed: BTreeSet<Capability>,
    ledger: Ledger,
}
impl Project {
    fn main(&self, l: Layer, p: Option<&Package>) -> PathBuf;
    fn test(&self, l: Layer, p: Option<&Package>) -> PathBuf;
    fn record(&self, ty: &str) -> Option<&[Field]>;   // was fields_from_record(root, …)
}

/// Introduce Parameter Object, applied to the six-string data clump.
struct Layers(/* Layer -> package name */);

/// What the user asked for. Entity identity = (recipe, name, package).
struct Intent { recipe: Recipe, name: Name, package: Option<Package>, args: Args }
enum   Recipe { Kind(ArtifactKind), Capability(Capability) }
struct Args    { fields: Vec<Field>, indexes: Vec<Index>, refs: Refs, timestamps: bool }
struct Refs    { on: Option<Ref>, yields: Option<Ref> }
struct Ref     { name: String, expect: Referent }   // Resource|UseCase|Event|Dispatcher|Type

/// The Command object. One shape, replacing Artifact / NewFile / SpringSlice / tuple.
struct Change {
    files: Vec<Artifact>, deps: Vec<Dependency>, plugins: Vec<Plugin>,
    properties: Vec<PropertyBlock>, compose: Vec<ComposeService>,
    edits: Vec<Edit>, legacy: Vec<Dependency>,   // legacy = revert-only
}
struct Artifact { tree: Tree, layer: Layer, placement: Placement,
                  file: String, label: &'static str, body: Body }

/// The key move: a body is a recipe for bytes, not bytes. Makes `plan` pure.
enum Body { Template(&'static str, Bindings), Computed(fn(&Project,&Intent)->Result<String>) }

/// plan.md §11's `codemod.rs`, as data instead of scattered functions.
enum Edit {
    RegisterCommand { dispatcher: Ref, command: String },
    ImportTestConfig { class: String },
    UnionProperty { key: &'static str, values: Vec<&'static str> },  // exposure_include
}
```

Four functions, each written **once**:

```rust
fn plan(p: &Project, i: &Intent) -> Result<Change>;   // pure; every refusal lives here
fn apply(p: &Project, c: &Change) -> Result<Report>;
fn revert(p: &Project, c: &Change) -> Result<Report>;
fn verify(p: &Project, c: &Change) -> Vec<Finding>;   // doctor
fn describe(c: &Change) -> Report;                    // --pretend / --dry-run
```

### 6.1 Give `Change` an algebra

`Change` should be a **monoid**: an empty value, and an associative merge that
deduplicates deps and detects file collisions. Three things then stop being
features and become consequences:

- `add db kafka` = fold two Changes → one preflight, one pom write.
- `app apply` = fold N → **the atomic whole-manifest `ChangeSet` plan.md §22
  wants**, at position 22 of the queue, for free at position 3.
- Conflict detection = a pure function on the folded value, before any write.

This is the composability half of the Composite pattern, which is the half worth
keeping.

### 6.2 What falls out rather than being built

- **`KIND_FILES` deleted.** `destroy` = `plan(…).files.map(path)`; bodies are
  never rendered because `Body` is lazy. plan.md §6.2 B as a consequence, not a
  project.
- **`--pretend` cannot disagree with apply** — `describe` and `apply` consume
  one value. `dry_run` collapses into `pretend`; the 5 `dry_run || pretend`
  sites go.
- **`remove` stops being a hand-mirrored 200 lines.** It is `revert`.
- **`doctor` becomes derived:** for each ledger intent, `plan` it against
  today's project and report the delta. The ~20 hand-written `*_check` functions
  shrink to the ones that probe the **environment** (podman socket, JDK, Docker
  reachability) — which is doctor's real value and which no plan can derive.
- **Drift detection is that same delta**, so plan.md §11.1's regenerate-and-merge
  gets its "old output" for nothing.
- **`require_spring` becomes a precondition on `Recipe`**, checked by `plan`
  against `Project.flavor`. That precondition is the *only* reason `spring.rs`
  is one 6,621-line file (logical cohesion, §3.2) — turn it into data and the
  file dissolves along real seams.

### 6.3 One ledger

```toml
# .jails/ledger.toml — replaces app-state-v1, intents/*, files, models, version,
# and jails.toml's capability list.
version = "0.9.3"

[[applied]]
recipe = "scaffold"; name = "CrawlRun"; package = ""
fields = ["id:uuid@pk", "seedUrl:uri", "status:CrawlStatus@index"]
files  = ["src/main/java/…/CrawlRun.java", "…"]

[[applied]]
recipe = "capability"; name = "db"
files  = ["…"]
```

Identity `(recipe, name, package)`; `fields` is content; `files` is **recorded,
not recomputed** (plan.md §11.2 stays correct — recompute for `--pretend` where
nothing exists yet, read the record for `destroy` after an upgrade).

`jails.toml` stays the user's hand-editable layout and declared capabilities.
`.jails/ledger.toml` is jails' bookkeeping and is never hand-edited. That is a
real boundary, unlike today's seven-way split.

### 6.4 The layout: one module per secret

Parnas's criterion, applied deliberately instead of half-accidentally. jails
generates hexagonal architecture and does not have one; `spring.rs` mixes domain
knowledge (what a use case *is*), rendering (Java strings) and I/O
(`pom::read`) in one file.

```
src/model/     Project, Layers, Intent, Args, Refs, Change, Artifact, Field   — pure
src/recipes/   plan(): kinds/*, capabilities/* — one file per recipe          — pure
src/render/    templates + Bindings                                           — pure
src/apply/     the interpreter + codemod.rs (pom, compose, properties, java)  — the only I/O
src/inspect/   doctor, why, routes, beans, stats                              — read-only
src/cli/       clap dispatch                                                  — thin
src/json.rs    the escaper currently squatting in project.rs
```

Two rules that keep it honest, both checkable in review:

1. **One role stereotype per module.** `model` and `recipes` are information
   holders and service providers; `apply` is the only interfacer. A renderer
   that reads a file has taken a second role and is wrong.
2. **`plan` may not touch the filesystem.** Enforceable by the type: `plan`
   takes `&Project` and returns `Result<Change>`, and `Project` is the *only*
   window onto disk.

plan.md §6.5 proposes splitting `spring.rs` by subject (capability / workflow /
durable / http). That is a better file list and still the wrong axis: it keeps
rendering, decisions and I/O interleaved inside each new file. **Split by phase
first, subject second** — `recipes/kinds/usecase.rs` is then genuinely small,
because its Java lives in `templates/` and its I/O lives in `apply/`.

---

## 7. Ladder — each rung ships green, none is a big bang

The golden suite (38 scenarios, 457 files, `tests/agreement.rs` both ways) is
the oracle. **No rung may change a golden byte.** That suite is what makes this a
mechanical exercise instead of a gamble; it should be spent, not admired.

| # | Rung | Named refactoring | Removes | Cost |
|---|---|---|---|---|
| 1 | Adopt `Project` + `Layers`; thread instead of `root` | Introduce Parameter Object | 188 `root: &Path`; the 8–11-param clump; ~120 re-reads | 2–3 d, mechanical (`Project::root()` keeps old sites alive mid-move) |
| 2 | One `Change`; delete `Artifact`/`NewFile`/`SpringSlice`/tuple | Extract Class | 4 shapes → 1 | 1 d |
| 3 | One `apply`/`revert`/`describe`; `Change` monoid | Command with undo | `add`'s dry-run branch; `remove`'s longhand; `dry_run\|\|pretend`; **plan.md §22's ChangeSet** | 1–2 d |
| 4 | `Body` lazy; `plan` pure | Separate Query from Modifier | the reason `KIND_FILES` exists | 1 d |
| 5 | Derive `destroy` from `plan`; **delete `KIND_FILES`** | — | plan.md §6.1 copy 2 | 0.5 d, free after 4 |
| 6 | `Edit` + `apply/codemod.rs` | Replace Conditional with Polymorphism | splices across 5 modules; 29 production `fs::write` sites | 1 d |
| 7 | Typed `Refs`; `on`/`yields` in `app.toml`, `strategy_on` deprecated alias | Replace Type Code with Subclasses | §4.4 | 1 d |
| 8 | One `.jails/ledger.toml`; identity = (recipe,name,package) | Entity vs Value Object | 7 state files → 2; **§9.7 fixed structurally** | 1–2 d |
| 9 | `doctor` derives capability checks from `plan` | Move Method | ~600 lines of re-encoded facts; a whole unchecked drift class | 2 d |
| 10 | Templates out of `spring.rs` (39 inline bodies, 221 `{{`) | Extract Class | plan.md §6.2 C, now trivial | ongoing |
| 11 | Split `src/` by secret (§6.4); rescue `json_string` | Move Module | coincidental cohesion in `project.rs` | 1 d, last on purpose |

Rungs 1–5 are ~6 days and remove the documented bug class. Rung 9 is where the
compounding shows: it is cheap **only because** 1–5 happened. Rung 11 is last
because moving files before the types are right just relocates the mess.

**Where I disagree with plan.md's sequence.** The structural work sits at
positions 1, 11, 16 and 22 of a 22-item queue, and each is priced as if the
others had not happened — §6.2 F (descriptors) is a week partly because there is
no `Change` for a descriptor to describe; §22 (`codemod.rs`, atomic `ChangeSet`)
is L partly because there is no interpreter to target. Rungs 1–5 re-price 11, 16
and 22 downward.

They do **not** re-price the authorship debt (items 3–8, `g field`), which is
orthogonal and genuinely more urgent. **If only one track can run, run that
one** — it is the user-visible one, and this document is worth nothing next to a
tool that makes a model change cheap.

**On §6.2 F.** After rungs 1–8, one descriptor per kind stops being a new
architecture and becomes a serialisation of `Recipe` + `Change`. Do it then or
not at all — doing it first would freeze today's shape into a file format.

---

## 8. The strongest argument against this document

Sandi Metz: *"Duplication is far cheaper than the wrong abstraction."* This
codebase has been through several hands, and the natural failure mode of a
document like this one is to propose the sixth wrong abstraction with more
confidence than the five before it. Three answers, in descending strength:

1. **The unification is not a guess; it is already proven by tests.**
   `tests/agreement.rs` demonstrates that `generate` and `destroy` agree on
   every kind, in both directions, today. A test that passes over duplicated
   logic is evidence that the logic is genuinely one thing. This is the opposite
   of the speculative case Metz warns about — the abstraction is being *observed*
   and then named, not invented and then imposed.
2. **Every rung is byte-checkable and independently revertible.** The golden
   suite says whether a rung changed behaviour, immediately.
3. **Each rung has a falsifiable gate.** If it does not hit its number, revert
   it — the rung was wrong, and finding that out costs a day.

| Rung | Gate — revert if not met |
|---|---|
| 1 | `root: &Path` count 188 → under 40; no `spring.rs` fn over 5 params |
| 2 | exactly one struct in `src/` with a `contents`/`body` field |
| 3 | `add.rs` loses its `if dry_run` branch; `remove` under 60 lines; zero `dry_run \|\| pretend` |
| 4–5 | `KIND_FILES` and `NO_FILE_TABLE` deleted; `tests/agreement.rs` still green |
| 6 | zero `fs::write` outside `src/apply/` |
| 8 | `.jails/` holds 2 files; an edited `fields` line round-trips without a manual `destroy` |
| 9 | `doctor.rs` under 700 lines with capability checks still passing |

Metz's rule is about *speculative* abstraction. Where a gate is missed, her rule
wins and the rung goes back.

---

## 9. The two rules to carry forward

plan.md §6.3 already has the right rule for the output, and it is unchanged:

> **Model the output, not the process.**

This file adds the one that would have prevented all five failures in §4, and it
is Parnas's 1972 criterion with jails' own evidence attached:

> **Decompose by secret, not by step.** A module named for a command
> (`generate`, `add`, `doctor`) accretes every concern that command touches. A
> module named for a hidden decision (`pom`, `compose`, `process`, `java`) stays
> small for a decade. jails contains both experiments and they have already
> returned their result.

And the operational corollary, which is measurable and therefore enforceable in
review:

> **Adding one instance of a concept should be one edit.** When it is not, the
> concept is not modelled — it is spelled out. `why.rs` is one edit. Everything
> in §2's table that is not should become one, and the edit count is the number
> to watch on every change.
>
> When a requirement does not fit an abstraction, **widen the abstraction or
> delete it — do not clone it and write a test to keep the clones in step.** A
> test that polices duplication is a receipt for a decision not yet made.
> `tests/agreement.rs` is an excellent test, and its existence is the strongest
> single argument for deleting the thing it tests.
