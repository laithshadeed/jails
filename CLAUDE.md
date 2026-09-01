# jails

Rails-CLI-inspired scaffolding tool for Spring Boot / plain Maven projects.
`README.md` is the user-facing surface (command list, field types, what's
deliberately deferred) — treat it as the spec, and update it in the same
change as the code. The original `prompt.md` spec was deleted once the
commands it described all shipped; don't go looking for it.

The scope bar: no ORM, no jails runtime jar, no Lombok, no preview features in
generated Java, and no plugin system with lifecycle hooks. Check `README.md`'s
"Not yet" before adding a command that isn't already there.

**Gradle is supported, and `gradle.rs` has one bar to clear: answer exactly
or refuse, never guess.** A tool that half-understands a build file and reports
a dependency the build does not have is the worst outcome available -- worse
than refusing, and worse than not supporting Gradle at all. Degrading politely
is worth less than working when the project is the one you are actually in,
which is why `add`, `check`, `test`, `build` and `run` all work on a Gradle
project rather than declining.

**Every idea, roadmap item and open design question lives under `docs/`.**
This file describes what the code *is* and the traps in it; `docs/` is the
design, the measured state, and the working checklist of what is not done and
why. Do not add proposals here.

**Twelve design documents became one on 2026-09-01, and that one became six.**
`jdl-sol.md`, `jdl.md`, `simplify-sol.md`, `simplify-gemini.md`,
`simplify-opus.md`, `simplify-glm.md`, `plan.md`, `audit.md`, `bugs.md`,
`missing.md`, `modern.md` and `research.md` were one system described twelve
times, and they had begun to disagree with each other and with the tree -- two
of them named a differential harness under a filename it had not had for days,
and a third described a gate under a function name that no longer existed.
Twelve documents is twelve places to update and eleven that will not be.

**The six are not a second attempt at organising prose.** `new.md` was correct
and was one queue: four agents working at once had to read all of it and then
edit the same file. The six split the *work* rather than the subject, so each
names the paths it owns and the paths it must not touch:

| file | workstream | owns |
|---|---|---|
| `docs/00-contracts.md` | — read first by everyone | the contracts, the deletion map, the identifier map, the ownership table |
| `docs/01-jdl-v1.md` | — normative reference | JDL v1, section numbering unchanged |
| `docs/10-language.md` | A | `crates/jails-model`, the JDL front ends |
| `docs/20-generated-java.md` | B | `crates/jails-compiler`, `templates/` |
| `docs/30-cutover.md` | C | `crates/jails-workspace`, `jails-project`, the legacy crates |
| `docs/40-gates-and-ci.md` | D | `.github/`, `mise.toml`, `scripts/`, `tests/common`, `tests/architecture` |

**Every identifier survived both merges**, deliberately: `P<phase>.<item>`,
`A<section>.<item>`, `B<n>`, `M<n>`, `research.md §N`, `modern.md §N` and
`jdl-sol.md §N` all still resolve, because the identifier travelled rather than
being renumbered. Read `<file>.md <id>` as "the entry with that id", and
`docs/00-contracts.md` carries the table saying which of the six holds it.
`docs/01-jdl-v1.md` is the JDL specification with its section numbering
unchanged, which is what the `jdl-sol.md §N` citations index --
`the_specification_complete_example_links_except_its_one_recorded_gap` extracts
its §4 and links it, so that file cannot rot into prose.

**Only P8.11 was split**, into `P8.11a` (adoption, in `30-cutover.md`) and
`P8.11b` (generators, in `20-generated-java.md`), because its four bullets
belonged to two different agents. Nothing else was renumbered.

Every citation count in this section is
`grep -rIoh --include='*.rs' '<doc>\.md' crates/*/src src tests | wc -l`, and
each drifts with every commit that adds a comment. **Re-measure before quoting
one**; they are recorded to show the order of magnitude that makes leaving the
citations in place cheaper than stripping them, not as facts to cite.

**A closed item is *deleted*, never marked done**, in the commit that closes
it. `git log -p -- docs/` is where a closed item and the measurement that
closed it live. Item and section numbers are stable and never reused, which is
what makes the deletions safe.

**Seventeen design documents are gone from disk and still cited** -- the
twelve above plus five older ones -- and every one resolves the same way:
`pending.md` (the checklist until `2f8003ba`),
`abstract.md` (the §7 ladder `tests/architecture/` implements), `refactor.md`,
`playground.md` and `test.md`. Those citations are still the best record of
*why* a decision was made, which is why they were left in place rather than
stripped when the files went.

```
git log --diff-filter=D -- pending.md     # the commit that removed it
git show <commit>^:pending.md             # its last content
```

**`refactor.md` is the one that does not resolve cleanly.** `git show` reaches
an older tracked version than the one folded in, because the copy on disk at
that point was untracked. There is no way to recover it.

## Canonical compiler cutover

The destination architecture is one semantic application compiler. It is
already present beside the legacy path and is the direction of travel:

```text
.jails/model.jdl / CLI sugar
        -> ModelPatch
        -> AppModel + WorkspaceSnapshot
        -> pure Compiler
        -> PlanDraft
        -> exact content-addressed PlanBundle
        -> preview or the one Executor
```

The five contracts are authoritative:

- `AppModel` is desired-state authority. Stable IDs carry identity; Java, SQL,
  route, and configuration names are projections.
- `WorkspaceSnapshot` captures every external fact once. Code below the
  compiler may observe the filesystem; the compiler may not.
- `Compiler` is pure. Equal snapshot, patch, and compiler version must produce
  equal desired artifacts.
- `PlanBundle` is the exact reviewed transition. Preview, export, confirmation,
  and apply must refer to its digest; apply never replans.
- `jails-workspace::execute` is the only canonical project writer. It locks,
  rechecks preconditions, publishes exact after-images, and converges on retry.

**"Converges on retry" is proved, not asserted, and the proof needs the
aborting half.** `crates/jails-workspace/tests/crash.rs` runs every point in
`fault::POINTS` twice -- once with an injected `Err`, once in a child process
that `abort()`s inside the trip -- and each row asserts its own point actually
tripped, so a matrix that stopped reaching one reports a failure rather than a
pass. The unwinding half was green while the aborting half was not, and the
difference is the whole reason to pay for a child process: an `Err` unwinds, so
the staged `NamedTempFile`'s guard removes it, while a real crash leaves it on
disk -- where `verify_preconditions` reads it as *an unmanaged file appeared
inside the managed tree* and refuses **permanently**, since nothing removed it
and every later plan refused the same way. `write_atomic` stages under
`.jails-staged-` rather than `tempfile`'s `.tmp` so `sweep_staged` can
recognise its own debris, and the sweep runs under the lock, where nothing
matching can belong to a live run.

**`new-cli` and `new --app` are canonical; ordinary `new` is not, and that is
the cutover's first step rather than a switch** -- see `src/new.rs` below for
the measurement blocking it. Do not make offline Spring or Gradle canonical by
default until every advertised follow-up workflow has a compiler backend:
default-on partial coverage breaks working capability commands.

`.jails/model.jdl` is the intended authoring boundary; `.jails/model.toml`
remains a temporary compatibility input for existing canonical projects.
**Never permit both editable sources** -- and read that as it is written,
because one of the things it sounds like it forbids is allowed. `app
plan|apply` *reads* a legacy authority and writes declarations into the model,
one way, once. That is not a second editable source, because the model is what
every later command reads. What is forbidden is a second thing the reader
*edits*, which is why `app init` -- the subcommand that writes a manifest --
is the one that still refuses on a canonical project.

**And the rule is being broken today, by a project on `.jails/model.toml`.**
Reproduced 2026-09-01: `model check` accepts one, `jails g record` applies a
patch and writes files, and `jails model upgrade` refuses it by name -- so it
is fully editable with no route to `jdl 1`. It has no route because the
command that was the route is gone: `jails model import` no longer exists, and
`jails model --help` lists `init check upgrade fmt plan apply explain eject`.
`model init` replaced it for a *foreign* project and writes the app block
only. `docs/10-language.md` A4.4 is the item.

**The renderer a one-shot carry-across needs now exists.**
`jails_model::render_jdl_v1` (`crates/jails-model/src/jdl/emit/`) takes a
linked `AppModel` and writes JDL v1. It refuses twice, and the second is what
makes it safe to point at somebody's project: every construct it cannot state
refuses by name, and then it parses and links what it just wrote and compares
that against the model it was given, because a renderer that silently drops a
field is how a one-shot migration corrupts a project. Proven over all 61
models in `tests/golden` and over §4's complete example.

What it does not have yet is a command, and the missing piece is one plan
operation rather than more rendering: the upgrade must write
`.jails/model.jdl` **and** retire `.jails/model.toml` in the same exact plan,
or the project ends with two sources again. `PlannedOperation` is
`ReplaceModelFile`, `PublishMergedTree`, `AppendMigration`, `ReplaceStateFile`,
`PatchReaderFile` and `RemoveReaderFile` -- none retires a model file -- and
`materialize_with_model` takes exactly one `ModelFileUpdate`.

Reproducible output belongs below `.jails/generated` and is merge-managed. The
accepted model renders BASE, capture supplies OURS, and the next model renders
THEIRS. Clean merges are frozen into the plan; conflicts refuse without writes.
The lock advances to THEIRS so hand edits remain deltas. Migrations, model revisions, and explicit
reader-file patches are irreproducible operations and must remain visible in
the plan rather than being smuggled into rendering. `model eject <artifact-id>`
transfers one ejectable adapter implementation into reader source, records the
transfer, and excludes that artifact from later managed trees. Records and
ports remain managed ABI. Capture must include every prospective reader
destination, collision must refuse, and ejection never infers ownership from
edited bytes or silently reclaims it.

Canonical `rename resource ... --strategy preserve-table` is a projection
patch, not lifecycle replay. Keep the entity stable ID and SQL table unchanged,
pair BASE/THEIRS by artifact ID even when paths move, and merge the old live
file into the new path. A destination collision or overlapping edit must refuse
before any model, lock, migration, build, or generated-tree write. Do not route
single-cutover or rolling rename into the legacy engine for a canonical project.

Canonical `resource field rename|type|nullability|drop` must also stay on the
model/compiler/workspace path. `ReplaceField` preserves stable field ID and
label and carries exactly one typed policy. Preserve-column rename changes only
the Java projection and emits no migration; single-cutover changes the SQL
projection explicitly. Safe type change accepts only the compiler's proven
PostgreSQL widenings. Required nullability captures the reader-owned backfill
file as a precondition and embeds those exact bytes before `set not null`.
Drop requires the accepted SQL column and must refuse while an operation
references the field. Every successful change still renders THEIRS and
three-way merges live Java as OURS. Rolling and expand/contract are campaigns,
not excuses to dispatch a canonical project to the legacy engine.

**Convention is recorded, not hidden: `jails model explain`.** Every name the
compiler derives rather than the author writing it -- the package, the Java
type, the SQL table and column, the HTTP route -- is a `DerivedValue` in
`AppModel.derived`, keyed by owner and role and carrying the `rule_id` that
produced it. Being *in* the model is the point rather than a convenience: it
puts the records in the accepted-model and plan digest, so a convention that
moves cannot move silently, which is `jdl-sol.md` §7.2 and §18.4 as one
mechanism. Two rules keep it honest -- it is recomputed from the model after
linking, after every patch and after the layout arrives, never accumulated;
and `pinned` is decided by comparing with the convention rather than by a flag
carried from the source, because a flag would make `derived` stop being a
function of the model.

**It is also where the §9.7 divergence lives.** Six of the twenty-three emitted
packages sit under a head §9.7 does not close -- `repository`, `application`,
`ports` -- and a `Head::Facet` is renamed by nothing, so a `jails.toml` that
renames `adapters` does not reach them. Their rule reads `convention.facet.*`
where a layer's reads `convention.layer.*`. Reconciling the six would move
files in every project generated so far, and §3.1 rule 4 forbids a compiler
changing a convention silently, so they are displayed rather than corrected.

Canonical crates, lowest first:

| crate | contract |
|---|---|
| `jails-model` | closed source schema, stable IDs, linking, semantic diagnostics, `AppModel` and `ModelPatch` |
| `jails-contracts` | portable `WorkspaceSnapshot`, `PlanDraft`, exact `Plan`, operations, trees and blobs |
| `jails-compiler` | pure semantic lowering to a desired artifact tree; no filesystem, environment or subprocess access |
| `jails-workspace` | capture, exact materialization, verification and the single canonical executor |

The workspace boundary currently has lossless, marked adapters for adding
`.jails/generated/main/java` to Maven and Groovy/Kotlin Gradle projects. Those
edits are exact `PatchReaderFile` operations with captured before-images;
arbitrary build-language mutation is deliberately not implied. Canonical
`set`/`unset` are stable setting-node patches lowered to complete main/test
property sets. Their adapter preserves unrelated bytes, repairs only keys the
previous model owned, refuses reader-owned collisions, and guards creation with
a captured missing-file precondition. Compose and migration adapters still
belong to the cutover. Ejection uses the same reader-file operation, but its
before-image must be `Missing`: transfer is creation of a new reader-owned
source, never reconciliation with an existing one.

During cutover, `.jails/model.jdl` or temporary `.jails/model.toml` opts a
project into the canonical path.

**Every advertised generator is routed through it: 39 of 39**, held by
`canonical_support::registry_classifies_every_advertised_word`, so a kind
added without a backend fails there rather than at the cutover. `scaffold` is
one typed entity profile over four facets, not a copied planner.

**Three of the last four needed no emitter at all**, which is the pattern to
check before writing one: `search` and `association` already had a complete
compiler backend and only wanted the syntax editor in front of it, so their
refusals told the reader to hand-edit `.jails/model.jdl` -- true, and useless.
**`migration` is the one that is deliberately not a declaration**, per
`jdl-sol.md` §2.1 ("Flyway/ordered migration files | no | immutable,
append-only history") and §12.6 ("authors never name managed migrations in
JDL"). It joins `PlanDraft.migrations` beside the derived ones, so it is an
ordinary `AppendMigration` in the reviewed plan rather than a side effect.

Unsupported canonical mutations must still refuse rather than silently
invoking the legacy engine. Delete this qualification only when all advertised
mutations, capabilities, schema evolution and reader-file patches use the
canonical contracts and the legacy planner/state/executor have been removed --
**the generators are through; the legacy crates are still there.**

`jails sync` in a canonical project compiles the current model and executes its
exact plan directly. Never route canonical sync through `jails-engine`; it is
the ordinary convergence command and must not create `.jails/objects`,
receipts, or a legacy journal.

Canonical `test --fast` owns its launcher through the `fast-test` model
capability. Installation/removal reconciles the build through the exact
document backend; never call the legacy fast-test precondition for a canonical
project.

**The controller's companion test drives MockMvc, not reflection.**
`emit_unit::controller_test` issues a real request through the dispatcher, in
the `MockMvcTester` shape on Boot 4 and the classic `perform(...)` shape below
it -- sniffed on the Boot major, like `@AutoConfigureMockMvc`'s package, with
`spring-boot-starter-webmvc-test` declared where Boot 4 split that slice out of
`spring-boot-starter-test`. It replaced a test that read the route back off the
handler's annotation, which holds whenever the annotation is present: when the
application cannot start, when two controllers claim the path, and when the
method is never dispatched. **That is the failure mode to look for when a
surface moves to the compiler** -- the canonical backend's refusals and wrong
answers are loud, and a weaker generated test is not. A route jails cannot
drive (a declared return type, or a request body it cannot construct) is still
emitted whole and `@Disabled`, asserting status only, because guessing a value
would not compile and emitting nothing would drop the coverage silently.

Linked `command`, `query`, `transition` and `event` operations already emit
typed managed Java ABI. This proves operations are compiler nodes rather than
dead manifest metadata. Familiar `g usecase|query|transition|event` frontends
already append those typed declarations with exact field-shape checks; Spring
HTTP adapters are emitted by the canonical `api` capability. They delegate to
the managed operation ports and are ejectable independently; business
implementations are not guessed from incomplete model semantics.

Canonical `destroy record|scaffold|usecase|query|transition|event` is model
subtraction or explicit stored-entity retirement followed by ordinary
compilation. Never add a canonical reverse renderer or file table. Removal
refuses while an operation edge still points at the declaration. A stored
entity requires `--storage preserve|drop`: preserve keeps an inactive semantic
node and emits no SQL, exact-table revival reuses it, and confirmed drop
appends one forward migration. Inactive nodes cannot evolve or receive new
operation edges.

Indexes are stable entity children. `resource index add` resolves model field
identity, records ordered columns, and appends one forward migration. Do not
reconstruct indexes from generated SQL or add a second index ledger; index
removal remains an explicit unsupported policy until its forward migration
contract is implemented.

**Canonical capability coverage is 25 of 25.** `fake`, `db` and `api` were the
first three -- in-memory repositories, JDBC repositories with schema
migrations, Spring operation controllers -- and most of the rest followed by
ordinary whole-model compilation. The last four, closed 2026-08-29, were the
ones that write *project* files rather than Java: `ci`, `docker` and `k8s` go
through the reader-facet file protocol `loadtest` already used, and `format` is
a `BuildFeature` plus an `.editorconfig`.

`canonical_support::registry_classifies_every_advertised_word` holds that
number, so a capability added without a backend fails there rather than at the
cutover. Should another arrive without one, it must refuse before legacy
dispatch; never let a canonical project silently create a legacy ledger.

**Every one of those project files has exactly one owner.** The workflow,
Dockerfile, chart and editor settings live under `templates/add/` and *both*
engines `include_str!` them -- two copies drift on pinned action SHAs and base
image tags, and neither drift is visible where anyone looks. They are
substituted with `str::replace`, never `template!`: GitHub writes
`${{ github.ref }}` and `docker image inspect` reads `{{.Config.User}}`, so a
renderer treating `{{...}}` as a placeholder reads those files' own syntax as
keys. **`format!` renders `{{` as `{`**, which is how an extraction of the
PromQL in the alert rules silently changed them -- the golden suite caught it,
a hand-written checker sharing the same wrong assumption did not.

**Canonical `format` refuses on Gradle, by name.** Spotless needs an
`id 'com.diffplug.spotless'` entry inside `plugins { }`, which is legal only as
the first statement of the script, and the canonical Gradle backend's whole
contract is that it appends a marked block and touches nothing else. Guessing
where the top of somebody's build file is produces a script that no longer
evaluates. Ejecting one of these
implementation artifacts moves the captured live bytes, including hand edits,
to reader source; it never ejects the managed ABI. `add dependency` / `remove dependency` are the exception because they
are not capabilities: each is a stable model node, and the compiler reconciles
the full set through the exact marked Maven/Gradle dependency adapter.
`set` / `unset` follow the same rule for settings: `(target, key)` has stable
identity and the exact properties adapter reconciles the complete target set;
never route a canonical project through the legacy property ledger.

## Legacy workspace during cutover

Thirteen legacy crates coexist with the four canonical crates above, plus two
leaf crates that belong to neither ladder: `jails-codec-derive` (the
`#[derive(Codec)]` proc macro) and `jails-codemod` (the marked block, with no
dependencies at all). Nineteen in total. A crate
may only depend on one below it, and Cargo enforces that;
`no_module_depends_on_a_layer_above_its_own` in
`tests/architecture/` enforces the same rule for module-level edges the
compiler cannot see, and assigns every module its crate. **That table
(`LAYERS`, `tests/architecture/rules.rs`) is the authority on which crate a
module belongs to** — this one is the prose, and prose is what goes stale.

| crate | what belongs in it |
|---|---|
| `jails-support` | **write, run, encode, and name.** Nothing here knows what a Java project is — `codemod` moved to `jails-project` and the working-directory lock to `jails-testkit` when that rule was applied honestly, and `runner` is `hermetic`, named for the contract that separates it from `process`. `Result`, `Failure` and `debug_cmd` live here, and so do `identity` and `identifier` — see the note below the table. |
| `jails-testkit` | one `hold_cwd()`, taken as a `[dev-dependency]`. Test infrastructure that cannot be `#[cfg(test)]`, because a dependent crate's tests cannot see one. |
| `jails-java` | reading Java (`java`, `classfile`) and rendering templates into it (`template`). |
| `jails-spec` | where a project is and how it is laid out (`build`, `spec::paths`, `spec::layout`), what a field spec means (`spec::field`), and the closed CLI vocabularies (`spec::kind`). |
| `jails-state` | **jails' own machine state, read and classified**: `compat` (absent / current / unreadable, never a fourth answer that quietly repairs something) and `listing` (what a directory holds). Below the Java project on purpose — `jails-commit` needs both and neither is about Java. |
| `jails-protocol` | **the plan/transition/effect vocabulary** — `Recipe`, `FieldSpec`, `EntityId`, `ResourceKey` and the intent, durable and observation values above them. One constructor per type, and every wire decoder calls it, so a value rejected at the CLI cannot arrive through a recovered journal instead. 43 modules under five heads (`vocabulary`, `intent`, `durable`, `observe`, `compatibility`); §7.4 of `pending.md` groups them. The validating newtypes are one crate lower, in `jails-support` — see below. |
| `jails-project` | one resolved `model::Project`, plus every file jails writes *about* a project — the reader's (`config`, `compose`, `pom`, `gradle`) and the read-only `projection` of jails' own. `compat` is `jails-state`'s, one row up; this said both. |
| `jails-generate` | everything that decides what Java to write: `generate`, `spring`, `add`, `sql`. Its planning half (`plan_for`, `artifacts_for`) is what the engine calls and is pure. |
| `jails-prepare` | **turning semantic desire into an exact executable transition**: `desire`, `reconcile`, `pipeline`, `merge`, `sandbox`, `report`. Plan-only — nothing here creates `.jails/` or commits anything. Everything a commit needs to *decide* is decided here, so the executor applies a value rather than re-deriving one. |
| `jails-commit` | **making a prepared transition durable, and recovering one**: `store`, `journal`, `execute`, `activate`, `recover`, `gc`. Crash recovery rolls a fully persisted, validated journal *forward*; preimages exist for a guarded explicit abort and for audit, not as the crash policy. That is what keeps this crate small — there is one direction to finish in. |
| `jails-report` | commands that **answer a question**: `doctor`, `why`, `explain`, `source`, `commands`. Read-only by contract, and the contract is structural — this crate sits *below* `jails-drive`, so a reporting command that started something would not compile. |
| `jails-drive` | commands that **start something**: `run`, `testd`, `launcher`, `affected`, `migrate`, `kafka`, `console`, `bench`, `lint`, `reports`, and `testing` — the vocabulary of `jails test`, whose only consumers are here and in the binary. The one edge back down is `run` → `report::why`, because `mvn spring-boot:run` exits 0 over a failed startup. |
| `jails-engine` | **one request, as one transition.** `route` and its submodules are where a parsed command becomes a capture, a desire, a preparation and a commit. Above the executor because it drives it; below the CLI because it is not about arguments. |
| `jails` (root) | the binary: `main`, `new`, `app`, `invoke`, and `tests/`. |

`jails-engine` and `jails-drive` sit at the same level and do not reference
each other; so do `jails-generate` and `jails-prepare`. The layering is a DAG,
not a line, and `LAYERS` records it as one number per module because a
same-level edge is allowed.

**`jails-spec` exists to keep the ladder acyclic.** Without it everything
below the generators reaches up into `generate.rs` for `Field`, `layout` and
`find_project_root`, and those single back-edges are enough to make the whole
of `src/` one cycle with no boundary drawable anywhere in it. `jails-spec` is
those symbols at their own layer, which is why a new shared symbol belongs
there rather than beside its first caller.

**Vocabulary a surviving crate needs does not live in a crate that dies.**
`jails-drive` and `jails-report` are not legacy -- they are the commands that
outlive the cutover -- so anything they need must sit outside the nine crates
the strangler deletes. That is why `identity` (the validating newtypes:
`ObjectId`, `Name`, `Package`, `JavaType`, `ProjectPath`, `SqlName`) and
`identifier` are in `jails-support`, and `testing` is in `jails-drive`, rather
than beside the legacy engine they happen to be used by.

`jails-drive` holds 15 references into those nine crates and `jails-report` 50,
and `jails-report`'s are reports *about* the legacy ledger -- a different
problem, not a misplaced type. The canonical four depend on nothing legacy, so
those two crates and the binary are the whole blocker.

Five things to know before touching it:

- **Each crate's `lib.rs` carries a facade block** re-exporting the lower
  crates, so module code keeps saying `crate::java` and `crate::Result`
  wherever it ships. Only that block knows which crate a module lives in,
  which is what makes moving one a one-line change instead of a sweep through
  forty files. Keep it trimmed to what the crate actually references. **A move
  leaves a re-export behind**, which is why the 115 files that say
  `jails_protocol::identity::Name` still say it after `identity` changed
  crates.
- **`jails-support` names itself** with `extern crate self as jails_support`.
  `#[derive(Codec)]` writes absolute paths into every impl it generates, and
  those do not resolve inside the crate that now hosts them.
- **`CARGO_MANIFEST_DIR` expands at the call site**, so `template!` cannot bake
  in its own root: it would resolve to whichever crate invoked it.
  `jails_java::template_at!` takes the root as an argument and each crate
  declares a one-line `template_here!` wrapper naming its own. `templates/`
  stays at the repository root; `jails-generate` and `testd` reach it through
  `concat!(env!("CARGO_MANIFEST_DIR"), "/../../templates/")`.
- **The binary is the root package, not `crates/jails-cli`.** That keeps
  `tests/`, `tests/golden/` and `tests/fixtures/` where they are, so the 24
  golden trees were not rewritten for a move that changes no bytes.
- **Every scanner has to walk `crates/*/src`, not `src/`.**
  `tests/architecture/` and `tests/genericity.rs` both do, and both assert a
  minimum file count, because a scanner that has lost the code reports exactly
  the same clean result as one that read it all. Path-matching scanners must
  also accept any visibility: the split turned `pub(crate) struct` into `pub
  struct`, and a gate matching only the first spelling silently read zero.

## Layout

- **The binary's front half is three files, split by the question each one
  answers.** `src/cli.rs` is the clap definition — read when somebody asks
  *what can I type*. `src/main.rs` is the module list, the tree it hands to
  clap, and `main`'s translation of a `Failure` into an exit status.
  `src/dispatch.rs` is the match from a parsed command to a transition — read
  when somebody asks *what does it do*. `jails-java` has a `dispatch` too --
  the splice that registers a generated command in a project's own CLI -- and
  the two coexist because `module_of` identifies a module by `(crate, module)`.
  A gate that matched on **basename** alone would measure each against the
  other's rules.
- **The canonical frontends are `src/model_*.rs`**, one per surface —
  `model_generate`, `model_resource`, `model_capability`, `model_destroy`,
  `model_rename`, `model_index`, `model_migration`, `model_setting`,
  `model_eject`, `model_import`, `model_upgrade`, `model_init`, `model_explain`,
  `model_doctor`, `model_status`, and the JDL editors behind
  `model_jdl_edit`/`model_generate_jdl`. `src/model_command.rs` is the
  canonical/legacy switch and the one owner of *which directory a model command
  is about* — see the first gotcha below.
- `src/new.rs` — `new` (start.spring.io wrapper, real network) and `new-cli`
  (hand-written pom/App/AppTest, no network). Both also seed
  `src/test/resources/fixtures/.gitkeep`.
- `crates/jails-generate/src/generate.rs` — `generate`/`destroy` dispatch, `scaffold_artifacts`,
  the write path and the project helpers. `ArtifactKind` is a
  `clap::ValueEnum` — keep it that way, see gotcha below. The per-kind
  generators live in submodules beside it:
  - the field spec moved down to `crates/jails-spec/src/spec/field.rs`;
    `sql.rs` is the SQL/JDBC projection of the same spec.
  - `generate/domain.rs` — `record`, `value`, `enum`, `sealed`, `strategy`.
  - `generate/web.rs` — the `controller`/`service` stubs and `handler`.
  - `generate/repository.rs` — `repo`: the port and the JDBC adapters.
  - `generate/cli.rs` — `command`, `cli`, and dispatcher registration.
  - `generate/migration.rs` — `migration` and `cases`, the two kinds whose
    NAME is not a Java class.
  - `generate/write.rs` — **how a generated file reaches disk in the shape
    jails guarantees**, and the reason it is a module rather than thirteen
    loose helpers: every rule in it is one a template would otherwise have to
    remember. Import normalisation, `package-info.java` planning,
    `ensure_failsafe`, `ensure_assertj`, `tidy_blank_lines`.
  - `generate/scaffold.rs` — `scaffold` and its evolution step `g field`.

  **The tests are colocated now**, and the extraction that failed twice
  succeeded on the third attempt for one reason worth keeping: **a
  brace-matching splitter must blank string literals first.** These tests are
  full of Java, so ending an item at the next line that is exactly `}` cuts one
  mid-literal. Blank comments and string literals — `r#"…"#` included — to
  spaces of the *same length*, count braces in the blanked copy, then slice the
  original. That is `java::blanked()`'s trick applied to Rust, and
  `tests/architecture/measure.rs` has the implementation the gates use.

  901 lines moved out: 16 tests to `generate/domain.rs`, 10 to `generate/web.rs`,
  5 to `generate/repository.rs`, 4 to `generate/cli.rs`, and 13 to
  `crates/jails-spec/src/spec/field.rs` — those last were testing the field spec
  through this file's re-export, so they belong to the crate that owns it. The
  nine that stayed are the nine about `generate.rs` itself.
- `crates/jails-generate/src/add.rs` — `add`/`remove`/`sync`/`preflight`: the orchestration that
  grows or shrinks an existing project by a whole slice (dependency + code +
  test, and for `db`/`kafka` a compose service). `Capability` is a
  `clap::ValueEnum` for the same completion reason as `ArtifactKind`. The
  per-capability plans live beside it:
  - `add/database.rs` — `db` and `sqlite`.
  - `add/messaging.rs` — `kafka`.
  - `add/data.rs` — `csv` and `json`.
  - `add/testing.rs` — `testkit`, `fake`, `toxiproxy`.
  - `add/tooling.rs` — `http` and `format`.

  `ci` and `docker` are the two capabilities whose plans live in `add.rs`
  itself rather than a submodule.

  The Spring-only capabilities live in `crates/jails-generate/src/spring.rs` instead — a different
  cut: they share one precondition (`require_spring`), not one subject. See
  that entry below, and note it has outgrown its original rationale.
- `templates/**.java` + `crates/jails-java/src/template.rs` — the Java bodies, as Java files.
  **Templates are real `.java` files, never Rust `format!` strings.** Java is
  made of braces and `format!` owns that syntax, so a `format!` template
  doubles every one of them (`class {name}Controller {{`, and `{{@code
  public}}` in Javadoc). They are pulled in with `include_str!`, so they are
  still compile-time constants with no runtime file access and no new
  dependency. Placeholders are `{{name}}`, which is safe because no `{{`
  appears in any `.java` jails writes, while `${name}` would collide
  (`spring.rs` generates `@Value("${...}")`). **Check `.java` specifically if
  this is ever revisited**, not the whole golden corpus: `{{` *does* appear in
  generated `.http` files, where `{{baseUrl}}` is the HTTP Client format's own
  variable syntax. Those are built with a Rust `format!` (escaped `{{{{`)
  rather than rendered through `template!`, so the two syntaxes never meet. A
  missing or
  unused key is a panic, not silent text in a generated class. It is
  substitution only — **not** a template engine: anything structural (Spring's
  `@Component` versus its absence, a body repeated per field) stays in Rust
  and is passed in rendered. A template under ~15 lines stays inline, since a
  file for four lines is indirection with nothing to show for it.
- `crates/jails-support/src/apply/` — **the only module that writes.** `fs::write` appears nowhere
  else, and `tests/architecture/` fails when it does. It also fails on a direct
  `apply::` call from anywhere that is not the write layer: the gate is at
  **zero**, so every mutation goes through the executor. Five verbs are exempt
  and say so in their names — `put_outside_project` and
  `ensure_directory_outside_project` (the machine, not a project),
  `put_in_scratch`, and `remove_derived`/`ensure_derived_directory`, which
  **refuse** a path outside `target/` or `build/` so the exemption is checked
  rather than promised. `apply::Tree` is exempt as a type: a function taking one
  cannot reach a published project. Four verbs, and the
  distinction between them is *what the caller believes is already there*:
  `create` (must not exist — the refusal `g scaffold` and `g record` are built
  on), `replace` (jails owns this file and is rewriting its own output),
  `put` (the new content already accounts for whatever was there — every
  splice into `pom.xml`, `compose.yaml`, `application.properties` and
  `jails.toml` lands here, with the byte-preserving merge done *before* the
  call by the module that owns the format), and `put_outside_project`
  (deliberately long: `jails setup` writes `~/.testcontainers.properties` and
  nothing that edits a project should reach it by accident). Plus `atomically`
  for `.jails/` bookkeeping and `put_bytes` for the 3-way merge result.

  This exists because `write_new_file` *looked* like the single choke point and
  was not: `add.rs` wrote capability files with a bare `fs::write` straight
  past the collision check, which is the hole plan.md §11 predicted a ledger
  would have. Giving `create` real meaning immediately surfaced a latent
  double-write — `ensure_package_info` writes `package-info.java`, and when the
  artifact being written *is* `package-info.java` the caller then wrote it
  again. Harmless while the second write was a silent overwrite; an "already
  exists" refusal the moment the write path started refusing to clobber.
- `crates/jails-support/src/process.rs` — `CommandSpec` + one synchronous executor, and the one
  place a tool is resolved on PATH. Extracted because two copies of "which
  tool is this" had already drifted: `run.rs` vs `project.rs` on whether mvnd
  is `mvnd` or `mvnd.cmd`, and `compose.rs` vs `doctor.rs` on whether Docker
  Compose is `docker compose` or the standalone `docker-compose`. **Debug
  prints and then runs** — that property lives in the executor now rather than
  at each site, which is where it was violated. **Secrets are never rendered**:
  `secret_env` marks one explicitly, and `ALWAYS_SECRET` is a name-based
  backstop, because `console.rs` sets `PGPASSWORD` on a plain `Command` that
  reaches debug rendering through `run_inherited` — a rule every call site has
  to remember is a rule that decays into printing a password. Arguments stay
  `OsString` end to end so a forwarded argument containing a space survives.
- **The layer list has one owner: `config::LAYERS_IN_ORDER`.** It carries each
  layer's package name *and* the heading `stats` prints, and the validation
  list is derived from it rather than written out again. **Anything reporting
  per layer must go through `Config::layers()`**, which applies the project's
  renames. A second copy of the list reports against jails' *default* package
  names, so a project with `adapters = "persistence"` has its adapters counted
  as "Other", and any layer missing from the copy is never counted at all. Layer matching is on whole path segments in
  sequence, so `webshop` is not the `web` layer and a nested
  `adapters = "infra.jdbc"` still matches.
- **`jails.toml` is a manifest, and its truth is maintained by `add`, not by
  the user.** `[project] capabilities` is what `jails sync` applies. `add`
  records every capability it applies — *including* on the "already set up"
  path, since a capability installed before the manifest existed is still part
  of the project — and `remove` takes it back out, because left listed the next
  `sync` would restore what was just removed. This is the whole design: a
  manifest somebody has to remember to update is a manifest that is wrong, and
  a wrong one is worse than none because `sync` acts on it. The names stored
  are `Capability::label()`, never clap aliases, or one capability could be
  listed twice under two spellings. Writing back is a targeted one-line splice
  that leaves comments and `[layout]` byte-for-byte alone — this is a file
  people edit, same rule as `pom.rs`.
- `crates/jails-spec/src/build.rs` — **which build tool a directory uses, and nothing more.**
  **The door is any recognised marker, nearest wins**, and the Maven-inherent
  commands refuse themselves through `require_maven` — a refusal that can say
  what still works. Keying the door on `pom.xml` alone refuses about thirty
  commands on a foreign project when only about ten of them need Maven at all
  (`inspect.rs` and `rename.rs` contain zero occurrences of `pom`). **jails never reads, writes, parses or invokes a foreign build
  file**; recognising a filename is not understanding a build. Because the
  templates are shaped by what the pom says, a missing pom silently changes
  the Java jails emits (`repository_wiring` → `PlainJdbc`,
  `jspecify_available` → false), so `generate::report_degraded_shape` says
  which shape it chose and names the dependencies it could not splice. `add`
  is **not** exempted: a capability that installs the code and skips the
  dependency is worse than one that refuses.
- `crates/jails-engine/src/route/maintenance.rs` — `jails adopt`: a closed
  synonym table mapping directory names onto `LAYERS_IN_ORDER`, committed as
  `SemanticEdit::HumanConfigLayout` rows in the `[layout]` table.
  **Configuration, not machinery** — everything downstream already reads
  `Config::layers()`. Three rules, each load-bearing: an unrecognised directory
  is reported not guessed; two candidates for one layer writes neither; and it
  never touches `[project] capabilities`, because that is the list `sync` acts
  on.
- `crates/jails-project/src/config.rs` — `jails.toml`, the per-project layout override. Hand-parsed
  (jails' only dependency is clap), understands one `[layout]` table of
  `key = "value"` pairs, and the keys are a **closed set** matching
  `generate::layout` — an unknown one is an error, because a `jails.toml`
  saying `adapter = "persistence"` that silently kept writing to `adapters`
  would be worse than no file. Read through the `place` closure in
  `generate`/`destroy`/`add`, so a renamed layer is renamed everywhere.
- `crates/jails-drive/src/kafka.rs` — `jails kafka`: the broker counterpart to `jails db`. Runs
  the image's own CLI tools inside the compose container, so nothing is
  installed. Note `BROKER` is `kafka:19092`, the *inter-broker* listener —
  `localhost:9092` is the host-side advertised one and works from inside the
  container only by accident of the port mapping. `topics_in()` reads a
  `TOPIC` constant out of source, using `java::blanked()` to locate the
  declaration and the **original** string to read the value, since `blanked`
  replaces the quotes too.
- `crates/jails-drive/src/migrate.rs` — `jails migrate --check`: applies every migration to a
  scratch database (`create database` / `drop database` around the run) and
  reports the first failure with psql's file and line. **Not a `doctor`
  check** — doctor is read-only by contract and this writes. Ordering is
  numeric, not lexical: `V10` sorts before `V9` as a string, which would apply
  migrations in an order nobody has tested.
- `crates/jails-project/src/pom.rs` — flavor and release-level detection, plus a comment-preserving
  dependency/plugin splice and unsplice. `TARGET_RELEASE` lives here.
- **A build plugin is claimed by what it *does*, not by its coordinate.**
  `ResourceKey::BuildFeature(BuildFeature)` — `IntegrationTests`, `Coverage`,
  `Formatting` — because `jacoco-maven-plugin` is not a name Gradle resolves,
  and keying by it filed a Gradle project's claim under a plugin it does not
  have. The Maven XML block is one rendering and `gradle.rs`'s block is the
  other. `gradle.rs`'s four matches are exhaustive over the enum, so **adding a
  feature is a compile error until the Gradle side exists** — which is what
  replaced the run-time refusal for an unrecognised plugin.
- `crates/jails-codemod/src/marked.rs` — **the marked block, and only that**:
  `# jails:<marker>` … `# /jails:<marker>`, which is how jails edits a file the
  reader owns and what makes `remove` the exact inverse of `add`. It had five
  owners (`compose.rs`, `add.rs`, `add/database.rs`, the test wiring,
  `doctor.rs`) each with its own `format!`; `tests/architecture/` fails on a
  `# jails:` literal outside this crate, so a sixth cannot appear quietly.

  **It is its own crate with no dependencies at all**, and that is what makes
  it reachable from both ladders: neither `jails-compiler` nor
  `jails-workspace` depends on `jails-project`, so a splice living there forces
  those crates to write their own.

  **The gate counts `file.literals`, not `file.production`.** Blanked source
  has every string literal replaced by spaces, and a `# jails:` marker only
  ever appears inside one — so a gate reading blanked source reports zero
  whatever the code says.

  It does not wrap a capability's `application.properties` settings; see the
  per-key rule in the gotchas below. `Marked::indented` exists because a marker
  at column zero inside a YAML mapping is a parse error rather than a misplaced
  comment. There is no `replace` — nothing needs one, and `remove` then `add`
  is the path `sync` takes.
- `crates/jails-project/src/compose.rs` — the other user-owned file jails edits: `compose.yaml`.
  Marked service blocks so `add db` and `add kafka` stack, and `remove` can
  take one service out without touching the other. Also `start`/`stop`
  (`docker compose up/stop`) and the auto-start `run` shells out to.
- `crates/jails-drive/src/launcher.rs` — `jails test --fast`: JUnit's console launcher over the
  already-compiled classes, no Maven. Three things worth knowing before
  touching it. **The console artifact's version must equal the project's own
  JUnit version** — a mismatch resolves fine and dies at run time with
  `NoSuchMethodError` wrapped in "versions not properly aligned"; `junit-bom`
  constrains every artifact to one number from JUnit 6 (confirmed in
  `deps/junit-framework/junit-bom`), while JUnit 5 paired jupiter `5.y.z` with
  platform `1.y.z`. **`staleness()` must never read "no class files" as
  "nothing is stale"** — that would run an empty classpath and report success.
  And **`--fast` does not beat `mvnd`**: `plan.md` §19.1 has the numbers, it is
  the no-mvnd path and the substrate for `jails testd`, and it must not be
  described as faster than the default.
- `crates/jails-drive/src/testd.rs` + `templates/testd/JailsTestDaemon.java` — `jails testd`: a
  resident JVM over a unix socket. **0.06-0.10 s against `--fast`'s 0.62 s**
  for one test method, measured; §19.2 says why, and it is not the launcher --
  the first JUnit session in a JVM is 464 ms against 20 ms warm, and a cold
  `java` pays it every run. Three things to know before touching it.
  **The classpath is split in two and must stay that way**: the daemon holds
  the *dependencies* on its own classpath and hands only `target/classes` and
  `target/test-classes` to JUnit as `--class-path`, so JUnit builds a child
  loader per run. Put the outputs on the daemon's classpath as well and
  parent-first delegation serves the stale class forever -- a daemon that looks
  perfect and is green over code that no longer exists.
  **It does not compile, deliberately.** §10.2's design had it hold a
  `JavaCompiler`; §19.5 measured that the editor's language server already
  writes `target/classes` on save, so the compile is being done by something
  with the whole project's model rather than one changed file -- and §10.2
  itself records that compiling only the changed file is unsound.
  And it is **a Java program, not a jails jar**: the daemon is a template
  compiled by `java`'s single-file source launcher at start-up, and nothing
  about it enters the project.
- `crates/jails-drive/src/affected.rs` + `crates/jails-java/src/classfile.rs` — `jails testd --affected`: a reverse
  dependency index built from the constant pools already in `target/`.
  `classfile.rs` is **the smallest reader that can answer "which types does
  this class name"** and must not grow into a class-file parser (same rule as
  `java.rs`): constant pool only, `CONSTANT_Class` plus a descriptor scan of
  every `CONSTANT_Utf8`, because a type named only in a signature is still one
  whose change breaks the class. **`CONSTANT_Long` and `CONSTANT_Double` take
  two pool slots**, and a reader advancing by one lands on tags that are
  usually valid — so it produces a plausible wrong answer rather than an error.
  Verified against 2,957 real class files under `deps/spring-boot`, not only
  synthetic pools. `affected.rs`'s rule is **unknown widens**: no git, a source
  with no compiled class, nothing compiled — each returns `Everything` with the
  reason printed. "Changed" is what git reports rather than a marker jails
  writes, because a marker makes the same command select differently on two
  consecutive runs with no edit between, and after a red run with nothing
  changed it would select nothing and report green.
- `crates/jails-drive/src/run.rs` — `test`/`build`/`clean`/`run`, shells to `mvn`/`mvnd`. `run`/`watch`
  start compose services first when `compose.yaml` is present.
- `crates/jails-drive/src/console.rs` — `db`/`dbconsole` (`psql` or `sqlite3`) and `console`/`c`
  (`jshell` + Maven classpath). Interactive; inherit stdio.
- `crates/jails-java/src/java.rs` — a deliberately small Java reader shared by `inspect`,
  `doctor` and `rename`: annotations and what they are attached to, a type's
  supertypes, a constructor's parameters. **Not a parser, and must not grow
  into one.** Its one trick is `blanked()`, which replaces comments and
  literals with spaces *of the same length*, so a scan cannot be fooled by
  `// @Service` while byte offsets still index the original source — which
  is why `annotations()` slices names and args out of `source`, not out of
  the blanked copy.
- `crates/jails-project/src/inspect.rs` — `routes` and `beans`. Reads source, never a running
  context: instant, and works on a project that does not start (the case
  that matters). The cost is anything decided at runtime, which the output
  states rather than hiding.
- `crates/jails-drive/src/doctor.rs` + `crates/jails-report/src/doctor/` — `doctor`. Split by **who is being asked**:
  `doctor/environment.rs` asks the machine (is Maven there, which JDK will run,
  is the container engine up and is it the one Testcontainers will find),
  `doctor/wiring.rs` asks the project whether a capability is actually wired up,
  and `doctor.rs` keeps the report, `--json`, and `capability_drift_checks` —
  the half that is *derived* from `add::plan_for` rather than hand-written.
  Read-only by contract: it must never start,
  stop or write anything, so it stays safe to run mid-debug. (`jails setup` is
  a different command and does write, to `~/.testcontainers.properties`, which
  is why it goes through `apply::put_outside_project`.)
  **`capability_drift_checks` re-plans rather than re-derives**: for every
  capability `jails.toml` records it calls `add::plan_for` — planning is pure,
  no writes, no subprocesses — and reports any dependency, file, property or
  compose service the plan wants and the project lacks, with `fix: jails sync`.
  Deriving it is what lets `doctor` notice a generated file somebody deleted;
  hand-written checks cannot, because only `add` knows what a capability
  installs. The hand-written ones stay for the two things derivation cannot
  cover: projects with **no** recorded capability list, where there is nothing
  to derive from, and failure modes no plan can express (two Jackson majors,
  podman's socket). Every `FAIL`
  carries a `fix:` line (an integration test asserts this), and a failure
  exits non-zero via an *empty* `Err` so `main` prints no redundant
  `jails: ` line.
- `crates/jails-report/src/why.rs` — `why`. A table of (signature, explanation, fix) rules
  matched against a log. Rules sharing a `group` describe one failure
  through different messages and only the most specific is reported. Add
  rules only from failures that actually happened; a guessed cause costs
  more than no cause. The way to find them is to mine the real logs:
  `grep -rhoa -E "Caused by: [a-zA-Z0-9_.]+(Exception|Error): .{0,80}"` over
  `~/.codex/sessions`, deduplicated and counted. Doing that once took
  coverage of this machine's distinct root causes from 2/6 to 6/6, and
  `every_root_cause_seen_in_real_logs_is_recognised` pins each with the
  count that justified it. Two of the four additions were variants of
  failures already covered — Testcontainers caches its environment probe, so
  the *retry* message ("Previous attempts to find a Docker environment
  failed") appears more often than the original.
- `crates/jails-generate/src/sql.rs` — the field spec -> SQL mapping: column name, column type,
  and the two JDBC expressions. **The dialect is chosen by the driver, not by
  `jails.toml`** (`Project::sql_dialect`): a manifest records what was asked
  for, a driver is a fact about the database the schema will meet. Postgres
  wins when both are present, because that is what `add db` migrates with
  Flyway. `Dialect::column_type` rewrites exactly **one** name -- `timestamptz`
  -> `timestamp with time zone` -- and that is the finding, not an oversight:
  every other type jails emits is in H2's own type table verbatim
  (`deps/h2database/.../value/DataType.java`), while `timestamptz` exists in H2
  only inside its PostgreSQL wire-protocol server and fails to parse in a
  `create table`. Confirmed against a real H2 2.4.240 both ways. One column list feeds the DDL, the select,
  the insert, the bind and the row mapper, which is the whole point — a
  hand-written pair drifts (`amount` in the insert against `amount_minor` in
  the select compiles and fails at runtime). **The write expression bakes in
  the receiver** rather than letting callers prefix it: `Timestamp.from(x.at())`
  puts the receiver in the middle, and gluing it on the front yields
  `x.Timestamp.from(at())`, which reads fine and does not compile. Only the
  real-toolchain tier catches that, which is why
  `a_scaffold_with_database_types_compiles_including_its_derived_jdbc_adapter`
  exists.
- `crates/jails-generate/src/spring.rs` — the Spring-only capabilities (`api`, `actuator`, `cache`,
  `security`, `redis`, `observability`) **and most of the generator kinds**:
  `client`, `job`, `dto`, `event`, plus everything added since —
  `fetcher`, `usecase` (and its `outbox` half), `query`, `transition`,
  `durable-job`, `association`, `http-workflow`, `http-sink`. They are here
  because they share one precondition — a Spring Boot parent, checked once in
  `require_spring`.

  **Every Java body is now a template file.** The 39 remaining inline
  `format!` strings with doubled braces were extracted to
  `templates/spring/*.java` in one mechanical pass, verified byte-for-byte by
  the golden suite: the file went 6,624 → 5,517 lines and 4,596 lines became
  real Java an editor can check. `tests/architecture/` holds that at zero.

  **The kinds live in submodules; `spring.rs` holds only what they share.**
  `spring/workflow.rs` (usecase and its outbox half), `spring/durable.rs` (job,
  durable-job), `spring/http.rs` (client, fetcher, http-workflow, http-sink),
  `spring/schema.rs` (association, idempotency), and `spring/transition/` and
  `spring/query/` each in their own directory. `spring.rs` is 1,118 lines --
  558 of them production, which is what the board counts -- and holds the
  shared precondition, the helpers more than one kind uses, and the capability
  slices.

  **`transition` and `query` split again by secret.**
  `spring/query/proof.rs` and `spring/transition/proof.rs` hold what jails
  writes to *prove* the recipe, separately from the route renderer. The fact a
  generated test turns on -- where the request's values come from -- is one the
  route renderer has already resolved, and a test renderer that resolves it a
  second time is free to disagree. `bugs.md` B48 is that drift.

  Two things the split needed and the next one will too: a child module reaches
  its parent's **private** items through `use super::*;`, but the parent needs
  `pub(crate)` on anything it borrows back — `scheduling_config_java` and
  `durable_alternate_sample` are the two the outbox shares with `durable`. And
  `include_str!` is relative to the *file*, so every template path in a moved
  block gains a `../`.

  **The board's *largest module* row names whichever module is largest**, so a
  split cannot be satisfied by *moving* a monolith. It sits at 688 production
  lines on `crates/jails-model/src/jdl/v1/parser.rs`. Run the board rather than
  trusting the filename in this paragraph. The shape it exists to keep out is
  `abstract.md` §3.2's: parse → dispatch → write → side effects in one file.

  **Placement is a value, not six strings: `spring::Slice`.** Every generator
  and renderer here takes a `Slice` — a resolved `model::Project` plus the
  `--package` override — and asks it for `placed(Layer::X)` (this slice's own
  classes, honouring `--package`) or `owned(Layer::X)` (where an *existing*
  resource lives, ignoring it). That distinction is load-bearing, and stating
  it at each call site as `place(layout::WEB)` versus `subpackage(&base,
  config.layer(layout::DOMAIN))` is what pushes these functions to eight and
  twelve positional parameters. **No function in this file takes more than
  five**, and `tests/architecture/` fails if one does. `Target`, `Defaults`,
  `Emission`, `Update` and `Projection` are the other parameter objects — each
  a group of values computed together and consumed together.

  `crates/jails-generate/src/spring/auth.rs` (`g auth`) and `crates/jails-generate/src/spring/sse.rs` (`add sse`) are the
  two most recent, and both exist because a default is wrong in a way nothing
  reports: `JwtTimestampValidator` accepts a token with no `exp`, and
  `spring.task.scheduling.pool.size` is 1. In both cases the generated *test*
  is the thing that keeps the fix in place, because removing the fix changes no
  behaviour any other test can observe.

  **Every template was written against `deps/`, not from memory.** The
  generated code targets APIs that moved recently, and the failure mode is
  silent: it compiles against the version you had.
- `src/new.rs` also owns **`--app <manifest>`** on both `new` and `new-cli`:
  create the project, seed the model, apply it. One command from an empty
  directory to a project that passes `mvn clean verify`. Making it work meant
  removing every `Project::discover()` from the apply path — every route takes
  an explicit `Run` carrying a resolved `Project` — because `discover` reads
  the **process CWD**, which is the parent directory, not the project just
  created.

  **`new --app` is canonical**: a model, no ledger, generation under
  `.jails/generated`. The root rides on `Invocation` rather than being threaded
  as a parameter, and that was the shape worth finding — `jails new` stands in
  the *parent* of the project it is creating, so `model_command::root` resolves
  the wrong one, while an explicit root would have reached roughly nine
  frontend entry points on the one ladder rung that exists to discourage a fact
  re-derived from a primitive. `Invocation` is that resolved value and was
  already threaded everywhere. The `_at` family — `compile_at`, `load_model_at`,
  `resolve_manifest_at`, `sync_at`, `materialize_seed` — is a **containment
  boundary** that stops the walk from the process directory, not a pattern to
  extend downward.

  **Plain `jails new` is still legacy, and that is the cutover's first step.**
  It is one line to seed the model and it is not a switch: flipping it reaches
  a canonical project that applies and compiles, and what stops it is a
  measurement — how much of the legacy engine's generated test suite the
  compiler reproduces for the same manifest. **The legacy side is pinned**, at
  `reports: 21, tests: 57` in `tests/cli/examples.rs`; the canonical side is
  not pinned anywhere and has to be measured by compiling the minicom manifest.
  **Re-measure it; do not quote a number from this file.** It moves whenever
  the compiler gains coverage.

  The canonical `api` capability does emit a companion test per controller,
  issuing a real request. Two facts that renderer must not work out for itself
  are decided once — how the request binds (a query is `@ModelAttribute`, a
  command is `@RequestBody`) and what the `Input` record declares. Two
  renderers reaching that answer separately is `bugs.md` B48.

  `new` must also seed its six default properties as `prop` declarations rather
  than reader-owned text, or a capability declaring the same key collides with
  the project's own scaffolding.
- `src/app.rs` — `jails app plan|apply`: a declarative manifest at
  `.jails/app.toml` (`schema`, `capabilities`, and a closed `[[generate]]`
  schema of `kind`/`name`/`fields`/`timestamps`/`indexes`/`package`/`on`/
  `yields`, with `strategy_on`/`strategy_yields` kept as deprecated aliases
  because they shipped in a user-facing file format — setting one reference
  under both spellings is an error, not a last-one-wins). **Deliberately domain-blind** — the module docs say it,
  and it is load-bearing: a crawler, a support inbox and a payments gateway
  are three lists of the same generic intents, and none of them gets a
  command, branch, enum or template in core.

  **There are two backends, and the project decides which runs.** On a legacy
  project `apply` is **one transition** over the whole manifest: capabilities
  and intents are declared together and reconciliation works out the
  difference, so an interrupted apply resumes from the journal rather than from
  a half-written registry. On a canonical one it **replays** the manifest row
  by row into the model, through the same frontends `jails g` and `jails add`
  use — a `[[generate]]` row *is* a `GenerateArgs`, byte for byte the value
  `jails g` parses. Row by row costs atomicity (a manifest that fails on row
  nine leaves rows one to eight applied) and buys convergence: every frontend
  is idempotent, so an interrupted replay is repaired by running it again,
  where the legacy path needed a journal a canonical project does not have.
  `app init` is the one subcommand that still refuses, because it writes the
  manifest rather than reading it.

  **A capability wires its own integration points, so no second reconcile
  pass is needed.** A generator can create something an already-installed
  capability needs — `add db` wiring a `@SpringBootTest` that a later row
  writes — and the answer is that the capability writing the test puts the
  container import in itself (`route::support::with_test_support`). Reconciling
  every capability a second time to catch it instead costs a duplicate
  formatter run and leaves the ownership in the wrong place.
- `.jails/ledger.toml` — the **one** file jails keeps its own bookkeeping in,
  and it is the transaction store's, not a module's. `jails-protocol`'s
  `envelope.rs` owns the file format (magic, schema number, checksum, and a
  hex-encoded canonical payload); `jails-commit`'s `store.rs` reads and writes
  it; `crates/jails-state/src/compat.rs` is the **read-only** classifier
  every command goes through — absent, current, or unreadable, and never a
  fourth answer that quietly repairs something.

  It replaced five files (`app-state-v1`, `intents/*`, `models/*`, `files`,
  `version`), two of which were intent registries keyed differently — which is
  what made an edited `fields` line arrive as a *new* intent against files that
  already existed. **Identity is the `EntityId`; everything else is content**,
  so an edit is an update to a known entity and `app apply` three-way merges
  it.

  **A store this binary cannot decode is an error, never an absence.** Treating
  it as empty would silently offer to regenerate a project's whole contents.
  There is no second format and no translation: jails is not released, so a
  ledger this binary did not write was written by a different jails, and the
  honest instruction is to say which file rather than guess at an older schema.

- `examples/` — the proof applications, and the reason the generic machinery
  can be trusted. `examples/web-crawler/` and `examples/support-inbox/` are
  manifests built from the same generic intents; `ACCEPTANCE.md` is the
  done/not-done contract per app; `DOGFOOD.md` is the command log, the
  twenty-one-defect ledger, and the friction ledger. **Never hand-edit a
  generated proof app to make it pass** — a manual edit is evidence for the
  next generic improvement and belongs in the friction ledger.
- `crates/jails-report/src/source.rs` — `jails src <Type>`: where a type's source is. The one
  command that deliberately does **not** require a build file — "where is this
  type" is a question about a directory, and the case it exists for (jumping
  into a library checkout) is often asked from a repo that is not a Maven
  project. It **lists every match rather than picking**, because a project with
  three `Status.java` files is ordinary and choosing silently sends an editor
  to the wrong one. The package is read off the `package` line, not derived
  from the path, since a checkout's layout does not always match its packages.
- `crates/jails-drive/src/bench.rs` — `jails bench`: runs the k6 script `add loadtest` wrote,
  after stating the load profile. **It does not parse k6's output** — k6 prints
  p95/p99 and its own thresholds decide pass/fail, and k6 is not installed on
  this machine, so a parser would be written against a format nobody has seen.
  `plan.md` §19.6's p99 is still unmeasured and says so.
- `crates/jails-engine/src/route/maintenance/rename.rs` — `rename`. Textual by design (see its module docs for
  when to prefer jdt.ls `grn`): whole identifiers only, string literals left
  alone and the skipped count reported.
- `tests/common/mod.rs` + `tests/cli/` — integration tests against the
  real compiled binary (`CARGO_BIN_EXE_jails`). **One binary, six subjects**:
  `main.rs` holds the shared fixtures and `new`, `generate`, `capabilities`,
  `app`, `tooling` and `reports` are ordinary submodules of it, each reaching
  the fixtures through `use super::*`. It was one 8,142-line file; the split is
  by *subject* rather than by tier, because which tier a test is in is already
  visible in whether it calls `common::skip`. A `tests/<name>.rs` is a crate
  root, so a split has to move it to `tests/<name>/main.rs` (cargo finds the
  same target) and reach the shared helpers through
  `#[path = "../common/mod.rs"] mod common;`.
- `tests/architecture/` — **the `abstract.md` §7 ladder, as ratchets.**
  Twenty-five gates, each a number measured over *production* Rust (comments,
  string literals and `#[cfg(test)]` modules are blanked first, the same trick
  `java.rs` uses). It fails when a number **rises above** its recorded ceiling
  *and* when one **falls below** it without the ceiling being lowered in the
  same change — so an improvement that is not written down is a failure, which
  is what makes progress stick. `cargo test --test architecture -- --nocapture
  --test-threads=1` prints the board. Raising a ceiling is allowed exactly
  once per rise and only with the reason recorded beside it in the file.
  This exists because `abstract.md` §8.1 measured `root: &Path` rising 21%
  across four commits with nothing to say so; prose did not move it.
  Four modules, one binary: `board.rs` is the ceilings, `rules.rs` the
  architecture properties and the `LAYERS` table, `measure.rs` the Rust
  blanking parser and every counting function (with its own unit tests
  colocated), `main.rs` only the crate docs. Gates name their file by **path**
  (`SPRING_RS`, `CODEMOD_RS`, `DOCTOR_RS`, `SCRATCH_RS`), not by basename —
  creating `src/new/spring.rs` once dragged two of `jails-generate/src/spring.rs`'s
  rows red for a file neither is about.
- **`g idempotency` is the retained-result primitive**, and the distinction it
  turns on is easy to lose: a `@unique` column already gives one row per key.
  What it withholds is the *result*, so a retry finds the row, fails the insert
  and gets a 409 — telling a caller that never saw the first response that the
  work happened, while still withholding what happened. The guard has four
  outcomes (run / replay / refuse a reused key / tell an in-flight retry to come
  back), and the claim is one `insert ... on conflict do nothing returning`
  because select-then-insert reopens the race. Domain-blind by construction:
  scope is a string the caller picks, the request is bytes the caller
  canonicalises, and the stored result is opaque.
- `crates/jails-report/src/explain.rs` — `jails explain <kind>`: why each artifact is shaped the
  way it is, and the trap it invites. **A hand-written table, deliberately** —
  a rationale is prose with nowhere to derive it from — so it is held to
  `why.rs`'s shape: a value in a table, one edit per kind, with
  `every_kind_has_an_explanation` failing the build when a kind is added
  without one. That is what stops it becoming the editor lists.
- `crates/jails-report/src/commands.rs` — `jails commands [--json]`: every subcommand, generator
  kind, capability and flag, walked out of the same `clap::Command` that parses
  the arguments and the same `ValueEnum`s that validate them. **There is no
  second list**, which is the point — adding a kind is one edit and this output
  follows. **It walks to every depth**, naming a nested command by the path you
  type (`remove fast-test`, `resource field add`, `app apply`, `db console`);
  stopping at depth one made it claim a surface it did not describe, which is
  the same defect `jails.nvim`'s deleted tables had. `help` is skipped: it is
  clap's, on every command at every depth.

  It is also the oracle for
  `every_command_a_message_tells_the_reader_to_run_is_one_that_exists`
  (`tests/cli/developer_tools.rs`), which scans every backticked `jails …` in a
  production message and checks the subcommand, the kind and the capability
  against it. research.md §0.2's theme is *oracles that disagree*, and the
  commonest form is a `fix:` line naming a command that was renamed elsewhere
  or never existed.
- `jails.nvim/` — tracked in this repo, but Lua, not Rust: a thin `:Jails`
  wrapper that shells out to the binary on PATH. **It keeps no completion
  tables of its own**: it reads `jails commands --json` once per session, and
  a hand-maintained `SUBCOMMANDS`/`KINDS`/`CAPABILITIES`/`OPTIONS` list drifts
  behind the CLI with nothing to stop it. `tests/editor.rs`
  asserts no such tables exist. Every failure path — an older
  binary, `jails` off PATH, a malformed payload — degrades to an empty menu
  rather than raising, because a completer runs on every keystroke. The
  `<leader>J...` keymaps that drive it live in a *third* repo
  (`~/code/my-dotfiles/home/.config/nvim/init.lua`), which this project's
  git history does not track.

Untracked siblings in this directory are **not** part of the project:
`deps/` holds ~80 gitignored upstream checkouts (each its own repo, read-only
research). **`.gitignore` ignores `deps/`, and only `deps/`** -- which is
worth knowing because a checkout that lands anywhere else is invisible to that
rule and `git add -A` files it as a gitlink. `deps-update.sh` did exactly that
once: it began life as `deps/update.sh` and cloned beside itself, so when it
moved to the repository root it cloned all 81 repositories *there*, 15 GB of
duplicates, and a commit went out carrying 81 pointers to repositories nobody
can clone. The script names its two paths separately now (`MANIFEST` beside
itself, `CHECKOUTS=deps`), and `./deps-update.sh --list` is the cheap way to
confirm it still reads the right tree -- it reported all 81 "missing" while
they sat in `deps/`. `ideas/` holds reference projects — crawler implementations, the
minicom stubs — cloned for study. Never edit or document either as if it were
jails'. `deps.tsv` and `deps-update.sh` at the repo root *are* tracked.

## Workflow (every change, no exceptions)

```
cargo test --workspace                                  # the inner loop
mise run verify-rewrite && cargo install --path .       # before pushing
```

**There are two commands and one switch between them, and the switch is
`JAILS_TOOLCHAIN`.** Plain `cargo test --workspace` is Rust only -- no JVM, no
container, no build tool -- and is **38.3s / 2005 tests inside a 2 GB cap**
here, on a machine already at load 18. `mise run verify-rewrite` sets
`JAILS_TOOLCHAIN=1`, which switches the real-toolchain tier on and turns
anything it then cannot run into a failure naming what was missing -- **165.6s
/ 2005 tests / 925 MB peak** here, with `JAILS_TEST_MAX_TOOLCHAIN_PROCESSES=4`
holding the JVM count down on a desktop that was already busy.

**That default is inverted from what it used to be, and inverting it is what
fixed three separate complaints at once.** The tier used to run whenever the
machine *happened* to have the tools, each probed off `PATH`, which meant:

| | before | after |
|---|---|---|
| `cargo test --workspace`, toolchain installed | 345.9s | **38.3s** |
| Maven subprocesses in that run | 859.3s over 36 | **none** |
| peak resident | ~7 GB | **638 MB** |
| same command on a machine without the tools | silently ran a third less | identical |

The memory figure is the one that mattered: a full run on a 30 GB desktop with
a browser open was OOM-killed by the kernel, and the kill left a PostgreSQL and
a Kafka container running for four hours afterwards. **Every one of those three
symptoms was the same decision** -- an expensive tier that opted itself in
based on what it found on `PATH`.

**Only part of that 7 GB was the tier, though, and the rest is the more useful
finding.** With the tier off the suite still peaked at 6.99 GB, and bisecting
reached one test, then one subprocess: `resource field add ... --diff` at
**6.8 GB in a single invocation**. Both diff implementations allocate an LCS
table quadratic in *lines* while every guard around them is on *bytes*, and
2 MB of source -- inside the 2 MB limit -- is about thirty thousand lines, whose
square at eight bytes a cell is seven gigabytes. They are bounded on the
product now (`jails-support::unified` for the canonical ladder,
`jails-prepare::review` for the legacy one). That single fix took the suite from
6.99 GB to 638 MB and from 59.6s to 38.3s.

Two things follow that are easy to undo by accident:

- **A probe must never be the thing that decides whether the tier runs.**
  `real_mvn_available` and its three siblings all answer `false` when the tier
  is off, so there is one switch rather than four independent PATH questions.
  `tests/common/toolchain.rs` owns it.
- **One variable, not two.** `JAILS_REQUIRE_TOOLCHAIN` existed only to notice
  that the old default was wrong; opting in *is* the requirement now, so it is
  gone and there is no combination of two flags left to get wrong.

**A Claude Code on the web session provisions itself.**
`.claude/hooks/session-start.sh` runs before the session starts and installs
what the gate needs: mise and the toolchain `mise.toml` pins, JDK 21 beside it
for the one pinned-Gradle example test, a container engine, and the sandbox's
interception CAs. It is remote-only (`CLAUDE_CODE_REMOTE`) and touches nothing
in the repository, so a laptop is unaffected.

**The CA half is the part that will be got wrong again.** The sandbox
intercepts TLS with *six* CAs; `/root/.ccr/agent-proxy-ca.crt` carries two of
them and `ca-bundle.crt` carries all of them. Trusting the two passes a
hand-run `mvn` and then fails inside a parallel suite, because which CA signs a
connection varies -- so it reads as flaky infrastructure and is not. The hook
imports every certificate in the bundle the JDK does not already trust,
diffed by SHA-256 fingerprint, naming no issuer. It cannot merely point the
JDK at the bundle: `real_maven_cmd` *replaces* `JAVA_TOOL_OPTIONS` with its
own GC flags, which is where the environment had put the truststore.

**Inside a container it is a different CA again, and a retagged image cannot
carry it.** Three findings, each of which cost a wrong fix first:

- **`/root/.ccr/java-truststore.p12` is not the bundle.** It holds 152 of the
  bundle's 154 certificates and the two it omits are the
  `CCR agent-proxy interception CA` -- the one actually signing. Pointing a
  build at it with `-Djavax.net.ssl.trustStore` *replaces* the JDK's own store,
  so it is strictly worse than doing nothing. An earlier hook did exactly that
  and turned a fixable image into a permanently broken one.
- **A BuildKit `RUN` is signed by a different CA than the host is** --
  `sandbox-egress-gateway-production Egress Gateway CA`, not the interception
  CA. Both are in `ca-bundle.crt`; neither subset of it has both. Import the
  whole bundle into the image's own `cacerts`.
- **`# syntax=docker/dockerfile:1` makes the local image store irrelevant.**
  That external frontend resolves every `FROM` against the registry, so no
  arrangement of tags reaches it -- measured as 154 imported CA certificates
  with the directive removed and **zero** with it present, unchanged by
  `--pull=false`, `docker rmi`, `buildx prune` or a daemon restart. The one
  mechanism it honours is `--build-context <name>=docker-image://<image>`,
  which is why `JAILS_OCI_BASE_IMAGES` exists: the hook publishes a trusted
  base under a name of its own and `verified_app_images` substitutes it. Empty
  everywhere else, so the gate builds exactly what jails wrote.

The trusted image is keyed on a hash of the bundle, because the CA rotates --
its common name carries a month and `/root/.ccr` is regenerated per session,
while the image store survives into the next one. A guard that asked "does the
trusted image exist" answered yes about an image built against a CA that no
longer signs anything.

**A dead `dockerd` leaves a pid file that blocks its own restart** ("process
with PID N is still running", about a process that is not), and the session
then looks like one that never had a container engine.

**The proxy port rotates, and `~/.m2/settings.xml` pins it.** A resumed
session gets a new port, so a `settings.xml` written by an earlier one sends
every Maven request to a socket nobody is listening on -- and the harness
*replaces* `JAVA_TOOL_OPTIONS` with its own GC flags, dropping the environment's
proxy sysprops, so that file is the only thing pointing Maven anywhere. It
presents as **~25 product-shaped test failures** at
`maven-resources-plugin ... Connection refused`, while a hand-run `mvn` from
the shell passes, because the shell still has the live port in its environment.
Maven then caches each failure as a `.lastUpdated` marker and honours it rather
than retrying, so repointing the file is not enough on its own:

```
find ~/.m2/repository -name '*.lastUpdated' -delete
```

A warm-looking local repository can therefore be poisoned rather than warm.
Suspect this before suspecting the diff whenever the whole real-toolchain tier
fails at once and the unit tiers are green.

**A mise shim resolves the version from the current directory, and the tests
do not run in this one.** `mise.toml` pins java, maven and mvnd for the
repository; a tier-3 test runs its toolchain inside a scratch directory, where
there is no `mise.toml` and the shim falls back to whatever the global config
names -- nothing, in a session whose hook installed the tools per-project. The
three failures differ and none of them names the cause:

| tool | what it looks like |
|---|---|
| `mvnd` | `mise ERROR No version is set for shim: mvnd`, and the command exits 1 |
| `maven` | *passes*, on whatever system Maven is on PATH -- 3.9.11 here against the pinned 3.9.16 |
| `java` | JDK 21, so `jails testd` dies loading a class compiled `--release 26` and prints **nothing** |

The `java` row is the dangerous one, and it is why the fix is a global rather
than a wrapper: the daemon is compiled by `java`'s single-file source launcher,
so a wrong JDK is a `UnsupportedClassVersionError` inside a process whose output
nobody reads, surfacing as four `tooling::` daemon tests failing with an empty
report. `mise use -g java@<pinned> maven@<pinned> mvnd@<pinned>` is the repair,
and `mise ls` in `/tmp` rather than in the repository is how to see the problem
at all.

Before it existed a web session ran two of the three tiers and said so
nowhere: `javac` rejected `--release 26`, ~50 tier-3 tests went red, and the
Stop hook ran `mise run verify-rewrite` into a command that was not installed.

**There is one answer to "is this green", and this is it.** `verify-rewrite`
is `simplify-sol.md`'s G0 gate, and `.githooks/pre-push` and
`.github/workflows/verify-rewrite.yml` invoke it and nothing else, so the hook,
CI and this file cannot drift apart about what passing means. `mise run lint`
is its fast half -- `fmt --check` plus `clippy --workspace --all-targets -D
warnings` -- and is what `.githooks/pre-commit` runs, for the same reason.

**`RUSTDOCFLAGS='-D warnings' cargo doc --workspace --no-deps` is in the gate
too, and the reason is this file's own subject.** The comments here are dense
with `` [`Type`] `` and `` [`module`] `` links, and rustdoc resolves every one
-- so a module that changes crates leaves a link naming an item that no longer
exists, reported at `warning` level, which nothing read. Twenty-five had
accumulated, several pointing at modules that had moved. It is `--no-deps` and
warm, ~28s on a ~217s gate, and it sits in `verify-rewrite` rather than in
`lint` on purpose: nearly tripling what `.githooks/pre-commit` waits for is a
worse trade than catching a stale link one push later.

Two properties it has that a hand-typed `cargo test` does not, both of which
this project has been bitten by:

- **`--workspace` is not optional.** `cargo test` at a workspace root tests the
  root package only: it reported 390 passing where the tree had 418, and
  nothing said the other 28 had not run.
- **`JAILS_TOOLCHAIN=1` switches the real-toolchain tier on**, and switching
  it on is what makes a missing tool a failure rather than a skip. Without it a
  tier-3 test that cannot find `mvn`, a new enough `javac`, Gradle or a
  container runtime does not run at all. It found
  `unheld_gradle_example_manifest_builds_on_its_pinned_toolchain`, which needs
  Gradle 8.5 on JDK 21 against a default of 26 and had never run here.

Tests must stay green before installing. A Stop hook runs this automatically
(see `.claude/settings.json`) — don't skip it manually even though the hook
exists, since the hook only fires on turn end, not mid-turn. It ran its own
`cargo build && cargo test && cargo install` on a **120-second timeout**, which
is shorter than `tests/cli` alone: it was being killed mid-suite every time, so
its verdict meant nothing. It runs the gate now, with a timeout that fits.

## How the suite stays fast, and the rules that keep it that way

**Read the environment note first, because it is the whole reason the numbers
below are split in two.** The tier-3 tests need a JDK that can compile
`TARGET_RELEASE` and a Docker daemon. On a machine missing either they skip or
fail fast, the suite still says something, and **every measurement taken there
is a measurement of the other two tiers**. Measure with
`JAILS_TOOLCHAIN=1` or measure nothing.

Measured on a four-core machine with the full toolchain present, so every
tier actually ran (`JAILS_TOOLCHAIN=1`). **The baseline is this
branch's own parent**, not some older revision -- the numbers below are what
*this* change is worth on top of everything already in the tree:

| | before | after |
|---|---|---|
| `cargo test --workspace` | 472.4s | **295.4s** |
| `mise run test` (concurrent binaries) | -- | **281.7s** |
| total CPU | 921.0s | 703.3s |
| every binary with Maven off PATH, concurrently | 64.0s | **22.4s** |
| CPU for that same set | 178.6s | **30.2s** |

**2.9x on everything that does not shell out to Maven -- and nearly 6x less
CPU for it -- against 1.6x overall.** The gap between those two is the whole
story of what is left, and
[the section below](#the-remaining-cost-is-maven-and-it-is-at-the-machines-floor)
is the measurement of why.

**Re-measure against the current parent, never against a remembered number.**
An earlier draft of this section quoted 289.1s -> 207.8s, taken before the
real-toolchain generator sweep landed. Both figures were true of the tree they
were measured on and neither described this one: the suite grew by roughly
180 seconds of Maven in between, so the honest ratio moved even though nothing
about the change did.

The rules that got it there, each one a rule rather than a one-off tidy-up:

**1. A table-driven test is parallel over its table.** Libtest parallelises
per `#[test]`, which is the wrong grain here: `agreement.rs` is two test
functions driving sixty-one independent scenarios and `golden.rs` is one. Each
cell is its own temporary directory and its own `jails` processes, so the
table goes through `tests/common/parallel.rs` -- a work-stealing scheduler over
one **process-wide** permit gate, so several concurrent tables cannot between
them oversubscribe the machine. `agreement` went 18.1s -> 2.3s, `golden`
9.2s -> 1.0s, `desired` 8.1s -> 3.1s, and `architecture_allowances` -- four
independent ArchUnit policies that shared one directory each rewrote, and so
had to run in sequence -- 14.1s -> 12.1s here and by the ratio of its four
Maven runs on a machine with cores to spare. **Write the cell as a function
returning its findings**, not as a loop body that pushes into a captured
`Vec`: the report then stays in table order however the cells ran, and
`parallel::catching` keeps a failing cell's own assertion message instead of
`a scoped thread panicked`.

**2. Scheduling is measured, not guessed.** Cells differ by orders of
magnitude and a work-stealing run's makespan is set by whatever starts last,
so the schedule is longest-processing-time first. Nothing declares a weight:
each run writes what it observed to `target/jails-test-costs/` and the next
run orders by it, with an unmeasured cell scheduled *first* because a new row
is more likely to be expensive than not. It is a hint and only a hint -- a
missing, stale or corrupt ledger changes the order and no result, which is why
it lives under `target/` and every read and write failure is ignored.
The per-binary ledger the deleted runner kept went with it; this one, over
scenarios inside a binary, is unaffected.

**3. A scan of the workspace happens once.** `tests/architecture/` has
twenty-five gates over the same 457 files and 6.2 MB, and it re-walked, re-read
and re-blanked all of it for each -- eleven full passes, 9.4s, no I/O worth
the name and no subprocess at all. `measure::sources()` is memoised behind a
`OnceLock` and its per-file blanking runs on the scheduler above: **0.35s**.
`genericity.rs` is the second scanner of the same tree and got the same
treatment: 3.6s -> 0.17s. If a third appears, it goes through
`parallel::map_by_cost` keyed on file size, largest first.

**4. `[profile.dev] opt-level = 1`, and it is about the *product* binary.**
The integration tests spawn `target/debug/jails` some thousands of times, and
an unoptimised build pays for that on every one; the byte-at-a-time blanking
parsers pay for it too. Level 1 with `debug = "line-tables-only"` keeps panic
locations, cost `jails-workspace`'s unit tests 6.3s -> 0.1s and the `cli`
binary 89.6s -> 48s with Maven absent. It buys that with about a hundred
seconds on a **cold** full build; an incremental rebuild after an ordinary
edit is unchanged at around four seconds, which is the number the inner loop
actually pays.

**5. Libtest's one thread per core is wrong for a process-spawn-bound
suite.** `cli` is 89.6s at four threads and 55.8s at sixteen with total CPU
unchanged, because these units spend their time in `fork`/`exec` and page
faults rather than on a core. `parallel::budget()` is four units per core for
that reason.

**That is `parallel::budget()`, not libtest's `--test-threads`, and raising
the latter is not the same lever.** The reading above invites it, so it was
measured over a warm `tests/cli` with the full toolchain: **147s at four
threads, 136s at eight, 140s at sixteen**. Eight is worth 7% and sixteen is
worse than eight, which puts the whole range inside this suite's noise -- and
The deleted runner recorded what oversubscription cost the last time it was
tried, a generated `http-sink` test whose localhost timeout went marginal on a
starved box. Leave it at the harness default.

That is a *different* budget from `default_max_toolchain_processes`, and the
two must not be confused: this one governs cheap `jails` spawns, that one
governs whole JVMs and is far smaller -- Surefire forks again underneath each
Maven, so its limit is memory and disk rather than cores, and it is measured
and clamped separately. Anything here that starts a build tool belongs under
*that* budget, not this one.

**6. `cargo test` runs the test binaries one after another, and that is now
the answer rather than the problem.** It was the problem for a while: the sum
of per-target times was within four seconds of the whole run's wall clock, so
essentially nothing overlapped, and `scripts/run-tests.py` ran all of them at
once, longest first, as the gate's runner.

**It was deleted on 2026-09-01, measured out rather than argued out**, and both
halves of the measurement matter:

| | `cargo test --workspace` | `scripts/run-tests.py` |
|---|---|---|
| toolchain tier on, 8 GB cap | **275.7s, 1991 passed** | killed |
| toolchain tier on, 10 GB cap | -- | **SIGKILL at 179s** |
| output when killed | streamed, up to the kill | empty (it buffered) |

- **The win had evaporated.** Concurrency across binaries is worth something
  only while no single binary dominates, and `cli` had grown to dominate
  completely: 42.7s of a 68.0s sum with the tier off, and essentially the whole
  run with it on. The other 29 overlap into its shadow either way.
- **The cost had not.** Sixteen binaries at once, each free to start JVMs, with
  a budget that counts Maven *processes* and never their footprint. It was
  OOM-killed by the kernel 179s into a run capped at 10 GB, having printed only
  `30 binaries, 16 at a time` -- and on a developer's desktop it took the whole
  session down and left a PostgreSQL and a Kafka running for four hours.

So the gate and the inner loop are both plain `cargo test --workspace` now,
which also removes the python3 dependency, the cost ledger for whole binaries,
and the `LD_LIBRARY_PATH` fix-up the runner needed because a proc-macro crate's
test harness links `libstd` dynamically.

**The Maven budget stays a `flock`, and the reason changed rather than
disappeared.** A `Mutex` and a `Condvar` are the whole machine's budget only
while one binary runs at a time, which is once again true of the runner -- but
not of the *machine*, where a second shell running `cargo test` while the first
still is doubles the JVM count with nothing to notice. One lock file per permit
under `target/`, shared however the suite is launched, is true in both cases and
costs nothing in the sequential one.

**Read the historical numbers below with the runner in mind.** Everything in
the rest of this section that quotes a `run-tests:` summary line was measured
under the concurrent runner and describes a scheduling regime that no longer
exists. The conclusions about *where the cost is* -- Maven, JVM starts, Spring
contexts -- are unaffected, because those are properties of the work rather
than of how it was launched. Re-measure any wall clock before quoting it.

### The remaining cost is Maven, and it is at the machine's floor

`cli` is the critical path and the real-toolchain tier is nearly all of its
cost: with Maven off PATH the whole suite's binaries finish concurrently in
22.4s against 295.4s with it. Five measurements bound what is left, and each
one closes off a plausible idea. **All five were taken on four cores** -- read
the concurrency one with that in mind, because it is the one that does not
generalise:

- **One `mvn test` on a cold generated Spring project is 7.1s wall and 9.3s
  CPU**, split 1.9s Maven start, 1.7s javac, **5.7s surefire fork and Spring
  context**. Sixty-one percent of it is a JVM booting a Spring context, which
  is the thing the tier exists to check.
- **The JVM flags are already right.** Dropping the harness's
  `-XX:+UseSerialGC -XX:TieredStopAtLevel=1` takes that run from 9.3s of CPU
  to **21.8s**. Adding `-XX:-UsePerfData`, pinned heap sizes, or
  `-XX:CICompilerCount=2` on top moves nothing. Do not go looking again
  without a measurement.
- **Concurrency is not the constraint *on four cores*.** Raising the Maven
  permit cap from six to ten changed 163.5s to 162.3s; removing it entirely
  (64) reached 158.0s while *raising* total CPU. That box is saturated rather
  than queued, even though the profile shows 848s of toolchain wall time and
  856s of it spent waiting for a permit.

  **This does not generalise, and the same repository has the counter-example
  written down.** `default_max_toolchain_processes` records `tests/cli` at
  113.2s with six permits and 106.3s with twelve, measured on sixteen cores.
  Four cores cannot distinguish "the cap is right" from "the machine is full",
  because at four cores every cap above six is the same cap. A permit
  experiment run here says nothing about a machine with cores to spare, which
  is exactly why that number is derived from the machine and not written down
  as a constant.
- **It is not I/O either.** Moving every scratch tree to a tmpfs took `sys`
  from 56.6s to 50.6s and wall from 155.2s to 160.9s. The page cache was
  already absorbing it.
- **An AppCDS archive over the Maven JVM** takes a no-op `mvn validate` from
  1.21s to 1.05s -- 13%, against several concurrent JVMs sharing one archive
  file. Not worth the corruption risk, and it cannot help the surefire fork at
  all, whose classpath contains a per-test temporary path and so can never
  match a shared archive.

So the tier is ~700s of JVM CPU and the floor on four cores is ~175s. **The
one lever left is the number of Maven runs**, which means generating several
tests' artifacts into one project and verifying them with one Maven run --
sharing one JVM start, one dependency resolution, and, because Spring caches a
context per configuration within a JVM, one context boot across many test
classes. It is worth roughly 3x on that subset and it is **not done**, because
it trades the tier's per-test isolation for speed and that is a call for
whoever owns the suite, not a performance change to slip in. If it is
attempted: do it on a machine whose JDK matches `TARGET_RELEASE`, since this
tier cannot be exercised at all on an older one and fails there with `release
version N not supported` -- nothing like the failure a wrong batching produces.

### What one Maven run costs, and why the marginal test in it is free

Profiled with the suite's own `JAILS_TEST_PROFILE` over `tests/cli`: **179
subprocesses, 781s of run time in a 216s wall** -- 41 Maven runs at 542s, 132
`jails` invocations at 223s, 6 docker at 16s.

**The box is saturated and the schedule is nearly optimal, which is what makes
the rest of this section the only lever.** Mean concurrency 4.4 on four cores,
*zero* seconds with nothing running, and 4.2s total ever spent waiting for a
Maven permit. Perfect four-core parallelism would be 195s against the measured
216s, so **every scheduling idea put together is worth at most 21s**: ordering,
more threads, a larger permit budget, a cleverer stealer. Do not spend time
there. The only thing that makes this faster is less work.

One `mvn test` on a generated Spring project, timed by goal against a warm
local repository:

| | s | share |
|---|---|---|
| Maven start (`validate`) | 1.54 | 24% |
| javac, main and test | 1.45 | 22% |
| surefire fork | ~1.1 | 17% |
| Spring context boot | 2.54 | 38% |
| **total** | **6.52** | |

41 runs x 6.52s is **267s, half of all Maven time, spent before any test does
anything**. Sixteen of those runs finish under 8s and are almost nothing but
that floor; nine runs of 20-45s carry 212s of genuine work.

**That distribution is out of date, and re-profiling before acting on it is
the difference between a 9s saving and a 100s one.** Re-measured 2026-08-30
with the same `JAILS_TEST_PROFILE`: 37 Maven runs, **730.2s**, and only
**seven runs under 9s totalling 39.3s**. The cheap-floor tail this paragraph
sends you after has essentially gone; batching all of it would buy about nine
seconds of wall. The cost had moved to a family the paragraph does not
mention -- **13 `canonical_*_pack_*` tests at an average 20.4s, 265.5s
together** -- each writing a Spring fixture, enabling one capability, and
appending a whole `mvn test` to prove the result compiles. Grouping by family
rather than by duration is what makes that visible:

| family | total | n | avg |
|---|---|---|---|
| capability-pack | 265.5s | 13 | 20.4s |
| canonical loops and others | 188.6s | 18 | 10.5s |
| proof-app | 163.0s | 3 | 54.3s |
| toolbox (already batched) | 113.1s | 3 | 37.7s |

**Those totals are from a partly cold run, and the trap they set is the one
this section was written to warn about.** The same suite profiled warm is
471.8s over 33 runs, so "730.2s" is mostly `jails-e2e-cache` being rebuilt,
not work batching can remove. Merging nine capability packs into two shared
projects was measured warm-to-warm at **471.8s -> 478.0s: nothing, inside the
noise** of a suite whose individual Maven runs are 20-55s on a contended box.
Compare warm against warm or the number will tell you whatever you hoped.

**And batching cannot help here anyway, which the concurrency profile says
outright.** Over a warm `tests/cli`: mean concurrency **3.25** on four cores,
peak 6, **0.0s** ever spent queueing for a Maven permit, 1.9s with nothing
running at all, against a perfect four-core packing of 169s for an observed
208.7s. The suite is 81% utilised and within a quarter of optimal. There is no
queue to drain and no idle to fill, so collapsing runs into shared projects
trades away per-test isolation for a saving the machine has no room to give.
The merges that exist are worth keeping because merging is the *stronger*
check -- it is what caught `mail` and `actuator` contradicting each other --
not because they made the suite faster.

**And the second test class in a run is free.** The same project built with
one, two, four and eight `@SpringBootTest` classes, one Maven invocation each:
6.56s, 6.48s, 6.44s, 6.49s. Spring caches a context per configuration inside
the JVM, so once the 6.52s is paid the rest cost nothing measurable. That is
the entire case for batching, and it is why the estimate is ~215s of Maven work
-- 41 runs collapsed toward 8 -- rather than a proportional saving.

`cached_toolchain_dir_with_salt` is that pattern already: `spring-core-toolbox`,
`spring-services-toolbox`, `spring-db-toolbox` and `proof-apps` *are* the
expensive runs, and they are expensive because they are doing real work rather
than paying the floor over and over.

Three smaller levers, measured on the same fixture, recorded so they are not
re-proposed as though they were big:

- **`-o` once the repository is warm: 0.47s a run.** Roughly 19s of work, but
  it fails hard rather than falling back if anything is genuinely missing.
- **`-DforkCount=0`: 0.55s a run.** It runs the tests inside the Maven JVM,
  which trades the isolation surefire exists to provide for about 23s of work.
- **`mvnd` is measured and refused, and the reason is sharper than the old
  note.** Per run it is exactly the win it promises: a full `test` on the
  Spring fixture goes 6.33s -> 3.88s, and the 2.45s saved beats the 1.54s
  Maven start alone because the daemon's javac is JIT-warm too -- about 96s of
  work across the suite's 39 runs. Run *sequentially* it is also stable: 24
  green out of 24 on JDK 26 here, including three forced cold daemon starts.

  It fails the moment it is used the way this suite uses Maven. Four builds
  started concurrently from four directories: plain `mvn` 16.2s and four
  successes; `mvnd` 5.0s and **three failures out of four**, with

      DaemonException$StaleAddressException: Could not receive a message from
      the daemon. No message received within 3000ms, daemon may have crashed.

  The whole suite with `mvnd` came in at 338.7s against 264.3s with `mvn`.
  So the old note -- "this machine's mvnd daemon is flaky under JDK 26" -- was
  right and was understating it: the trigger is concurrent invocation, not the
  machine, and concurrency is the one thing this suite will not give up. A
  per-run saving that costs the run's parallelism is not a saving.

### The CI job is the same suite on a smaller machine, and it paid twice

`.github/workflows/verify-rewrite.yml` runs `mise run verify-rewrite` on a
four-core GitHub runner. **Measurements taken here transfer to it**, which is
what makes it worth tuning from this machine at all: cold compilation is 214.0s
here against 207.6s there for the same two phases, and `cli` runs 257.9s there.
Confirm that before believing a CI experiment done locally, because the runner
is the same size only by coincidence.

One job, read off the log of run `33261806301` (9m50s wall, 589.8s):

| phase | s |
|---|---|
| checkout, toolchains, cache restores | 18.3 |
| `cargo fmt --all --check` | 4.4 |
| `cargo clippy --workspace --all-targets` | 18.4 |
| `cargo build --workspace` | 82.9 |
| test-harness compilation | 124.7 |
| test execution (`cli` 257.9s; the other 32 binaries 79.8s, in sequence) | 314.4 |
| the pinned Gradle example, on its own JDK | 26.6 |

**Compilation was 38% of it, with the dependency cache hitting.** That is not
the cache failing: `cargo-Linux-<Cargo.lock hash>` restores 381 MB and the
third-party crates are all in it. Every one of the twenty *workspace* crates is
genuinely cold, because the commit changed them.

Two of those rows were paying rather than measuring, and both are fixed.
Measured on the runner against `33265322341`, which ran 1856 tests to the
baseline's 1855 -- close enough that the phases compare directly:

| phase | before | after |
|---|---|---|
| `cargo fmt` | 4.4 | 2.2 |
| `cargo clippy --workspace --all-targets` | 18.4 | 14.7 |
| `cargo build --workspace` | 82.9 | *removed* |
| test-harness compilation | 124.7 | 179.4 |
| **all compilation** | **226.0** | **194.0** |
| test execution | 314.4 | 322.6 |
| the pinned Gradle example | 26.6 | 32.1 |
| **the gate step** | **571.4** | **550.9** |

- **`cargo build --workspace` built nothing the suite did not build anyway.**
  `mise.toml` has the reasoning; it is a barrier between two halves of one
  compile graph. Predicted from this machine at 214.0s -> 177.7s and measured
  on the runner at 226.0s -> 194.0s, which is the check that a compile
  experiment run here can be believed about there. The gate step moved less
  than compilation did because test execution and the Gradle step each drifted
  up by a few seconds, which is inside this job's run-to-run variance -- the
  same gate has come in at 571.4s and 654.2s on work that differed by nothing
  that matters.
- **Nothing cached `~/.m2`.** The cargo cache covers Rust and `mise-action`
  covers the tool binaries; the Maven local repository is on neither, and is
  not on the runner image. So every run re-resolved the whole Spring Boot,
  Testcontainers, Flyway, ArchUnit and spotless tree from Central. A cold local
  repository costs **21.8s for the 44 MB the suite's smallest Spring fixture
  needs**, measured on that fixture; the repository a full run fills is 296 MB.
  `33265322341` wrote 275 MB under `jvm-deps-Linux-v1`. Measured against the
  run that restored it, on `cli` -- the 431-test binary that is the critical
  path, reporting the same 431 passed both times: **294.4s into the test phase
  cold against 248.2s warm**. Read that against the noise: the same pair moved
  harness compilation 179.4s -> 160.0s on *identical* Rust, so a single run's
  difference below about 11% says nothing.

**Two more things the cache was doing wrong, both silent, both worth the shape
rather than the number.**

`rust-toolchain.toml` pinned rustc while the cargo key named only the OS and
`Cargo.lock`. A cargo artifact is valid only for the rustc that built it, so
every artifact in that entry became garbage the moment the pin named a
different compiler -- and the failure hid perfectly: the key still **hit**, so
the step reported success and restored 381 MB, cargo discarded all of it, and
`actions/cache` then skipped its save *because* a primary-key hit is exactly
when it does. Nothing could repair it. Measured on `33267456288`: 74
third-party crates compiled where the run before the pin compiled none, clippy
14.7s -> 31.3s, harness compilation 179.4s -> 240.5s. `rustc -V` is in the key
now.

And the entry was **written once and then frozen**, for the same
skip-on-hit reason: the key changed only with `Cargo.lock`, so whichever run
first saw that lockfile filled it and no later run could update it.
Dependencies were fine -- they move when the lock does -- but all twenty
workspace crates recompiled every run against sources that had moved on. The
key carries `github.sha` now so every run saves, and the restore-keys walk back
to the newest entry for this compiler and lockfile.

A **cancelled** run still skips the cargo save, and that is correct rather than
a gap: with a sha-keyed entry, a run that died two minutes in would write a
nearly empty cache that then becomes the *newest* match for the next run's
restore-keys. Better to keep the newest entry a complete one.

Measured end to end, two consecutive green runs after all of it -- 398s and
402s against a 571.4s baseline, on a job whose run-to-run spread had been
571.4s to 654.2s:

| phase | before | after |
|---|---|---|
| fmt + clippy | 22.8 | 17.0 |
| compilation | 207.6 | ~60 |
| test execution | 314.4 | 293.1 |
| the pinned Gradle example | 26.6 | ~28 |
| **the gate step** | **571.4** | **~400** |

  The save is a guarded `actions/cache/save` rather than the automatic post
  step, and `33265079310` is why: it was cancelled mid-gate, and the automatic
  step would have frozen a nearly empty repository under a key that never
  changes -- a permanent miss wearing a hit's clothes. The guard skipped it.

Two levers were measured and declined. They are recorded because both are
obvious enough to be proposed again:

- **lld is already the linker.** Rust 1.90 made `rust-lld` the default for
  `x86_64-unknown-linux-gnu`, and `rustc --print link-args` shows
  `-fuse-ld=lld` in the default arguments. Adding it to `RUSTFLAGS` passes the
  flag twice and changes nothing. Anything further here means mold, and mold
  has to be installed on both the runner and the developer's machine to keep
  one answer to "is this green".
- **Caching workspace `target/` artifacts between commits.** `target/debug/deps`
  is 5.9 GB, so the entry is one to two gigabytes compressed and -- unlike the
  dependency cache, which is written once and read for nothing -- it would have
  to be *written* on every run to be worth anything, putting 30-60s of
  compression and upload on the critical path. Against that, the saving is only
  the crates a commit did not touch, and this workspace is a deep chain where a
  mid-stack edit rebuilds most of what sits above it. The half that is stable
  is the dependency half, and that is already cached.


**The suite is `tests/cli`, and every measurement taken on a machine without
Docker is a measurement of something else.** From the gate's own summary on
run `33334340369`:

```
run-tests: 33 binaries, 4 at a time
run-tests: 298.2s wall for 33 binaries
run-tests: slowest cli 298.1s, engine 75.2s, architecture_allowances 45.6s,
           differential 32.4s, agreement 24.9s
```

**298.1s of a 298.2s test phase is one binary.** The other thirty-two finish
inside it and are free; nothing that speeds them up can show. Only `cli` has a
critical path, so only `cli` has a budget.

Take a local `tests/cli` number as evidence about CI only with
`JAILS_TOOLCHAIN=1` **and** a container engine running. With both,
local and CI agree closely -- 245s against 298s, and 474 of 476 tests passing
-- so the suite *can* be profiled off CI. Without a container runtime the
container-dependent tests fail in milliseconds and the run measures the suite
minus a third of its work, which is how `-DforkCount=0` came to look like a
51s win locally while moving CI from 298s to 298s.

**Profiled with both present, the shape is not what the sections above
assume.** Over one warm `tests/cli`: **507.1s of Maven across 34 runs, 285.2s
across 170 `jails` invocations, and 28.9s of `docker`** -- 821s of work
against a 245s span, so 84% of a perfect four-core packing.

Three things follow, and each contradicts a plausible guess:

- **Containers are 3% of the cost.** Starting PostgreSQL and Kafka is not
  where the time goes, so sharing or pooling them buys almost nothing.
- **The product binary is fast.** Median `jails` invocation is **73.5 ms**;
  the 1.7s mean is twenty invocations that are themselves JVM work
  (`jails check` is `mvn clean verify`, `jails test`, `app apply`), and they
  carry 92% of that bucket. Optimising the CLI itself would move nothing.
- **So ~770s of the 821s is a JVM**, and the only lever that matters is
  running fewer or cheaper ones.

The 236.9s behind the three proof applications is **not** the duplicate build
it looks like. `verified_app_fixtures` calls `verified_app_unit_fixtures`
deliberately, so Surefire runs while the containers are still starting, and
then executes only `failsafe:integration-test failsafe:verify`. Two lifecycle
phases, overlapped on purpose -- roughly 80s of unit tests and 153s of
integration tests against live services. Merging them removes the overlap
rather than the work.

**What the gate actually costs, measured step by step on run `33322968191`**
-- a commit that changed no Rust at all, against a warm cache, so the compile
is zero and what is left is the floor:

| step | s |
|---|---|
| set up, checkout, rustup cache, `rustup toolchain install` | 11 |
| **restore cargo** | **42** |
| restore `~/.m2` and `~/.gradle` | 4 |
| mise, JDK 21, toolchain banner | 14 |
| **`mise run verify-rewrite`** | **217** |
| **save cargo** | **21** |
| post steps | 2 |
| **job** | **314** |

So the irreducible part of a 314s job is a 217s gate and 97s of overhead, of
which 63s is moving the cargo entry on and off the runner. A commit that does
change code adds its compile to the 217s and nothing else.

**One run proves nothing about this job, and that is the trap most of this
section's history was written by.** Two runs that compiled *nothing* -- both
documentation-only commits against a warm cache -- measured the gate at 217s
and 261s, restore at 42s and 52s, and the whole job at 316s and 343s. So the
noise floor on a GitHub runner is about ±40s, or 13%, for byte-identical work.
Any change smaller than that is invisible in a single pair of runs, and a
threshold set without knowing this ("if the next run is over 316s, revert")
will reverse a correct change about half the time. Verify a CI change by the
*step it targets* -- `Save cargo` going 21s -> `skipped` is unambiguous --
and only then ask whether the total moved, over several runs.

**Read the gate's 217s against the concurrency numbers below before trying to
cut it**: `tests/cli` is 81% utilised on four cores with zero permit queueing,
so it is within a quarter of a perfect packing and there is no slack there to
reclaim.

**Pruning superseded artifacts out of the cargo cache does not pay, and the
number that makes it look like it should is real.** `cargo` never garbage-
collects `target/`: each CI run restores the previous run's artifacts and adds
its own, so a workspace crate accumulates one `.rlib` per historical build
hash while only the newest is linked. Measured here: `target/debug/deps` at
**9.68 GB across 2610 files, of which 8.44 GB -- 87% -- was superseded**, and
deleting it took `target/debug` from 13 GB to 4.6 GB. Against 63s a run spent
moving the entry (42s restore, 21s save), that looks like an easy ~48s.

It is not, because cargo rebuilds whatever it cannot find, and finding the
live set exactly is harder than it looks. Three rules were measured:

| keep rule | rebuild cost |
|---|---|
| newest per (stem, extension) by mtime | 24s |
| exactly the `filenames` cargo reports | **150s** |
| every file sharing a reported artifact's stem | **126s** |

The exact-filenames rule is the trap: `--message-format=json` names a fresh
dependency's `.rlib` and not the `.rmeta` beside it that pipelined compilation
reads, so cargo went off rebuilding `tempfile` and `serde_json`. Matching by
stem fixes that specific case and still misses others -- proc-macro and
build-script units among the likely candidates. Every rule costs more
recompilation than the transfer it saves.

Doing this properly means `cargo-sweep`, which reads `.fingerprint` rather
than guessing, and paying to install and cache it. Until someone does, the
stale 87% rides along, and that is the cheaper of the two.

**The other half of that entry does pay, and it is the cheap half.**
`target/debug/incremental` is the largest single thing in the cache, and
cargo's own housekeeping does not reach most of it: a unit's incremental
directory is named after its fingerprint and gets a fresh `s-*` session on
every build, and a directory whose fingerprint has moved on is never removed.
Measured here after a day of builds: **5.6 GB**, with two live
`jails_generate-*` and two live `jails_drive-*` directories differing only in
their hash. Keeping the newest session in each directory took a freshly built
tree from **1427 MB to 713 MB** with no rebuild at all, and the workflow's
trim step does it now.

The reason to keep the newest session rather than the whole directory is the
same measurement that explains why `CARGO_INCREMENTAL=0` was reverted below.
A one-line edit to `jails-generate`, timed over `cargo test --workspace
--no-run`:

| incremental state | rebuild |
|---|---|
| as the previous build left it | **4.1s** |
| `target/debug/incremental` deleted outright | **65.0s** |
| after the trim | **3.3s** |

So the state is worth about a minute of compilation a run and the superseded
half of it is worth nothing. Do not reach for the whole directory.

**On the runner it is four times that, and the size it removes is not what it
is worth.** The trim's own line from run `33383805799`, the first CI run to
carry it:

```
trim: dropped 189 superseded incremental sessions, 8149 MB -> 4081 MB
```

Twice the local figure, because CI's entry accumulates orphans from every run
that ever restored it and nothing had ever collected them. **The compressed
entry barely moved**: 2647 MB uploaded on that run against 2643 MB on the
next, so zstd was already collapsing near-duplicate sessions and what they
cost was disk rather than transfer. Read the 8149 -> 4081 as the saving and
you will over-claim it fourfold.

What it is actually worth is the transfer, measured across three consecutive
runs -- and it takes two of them to arrive, because the first trimmed save
still carries the orphans that run created:

| | baseline | first trimmed save | second | third |
|---|---|---|---|---|
| restore | 46s | 58s | 46s | **32s** |
| save | 45s | 37s | 34s | 35s |
| **transfer** | **91s** | 95s | 80s | **67s** |
| the gate itself | 323s | 372s | 298s | 355s |

**~24s a run, once settled.** The trim step itself is 1.3s.

**And the gate column is why no single run can show that.** Those four
numbers are near-identical work -- 298s, 323s, 355s, 372s -- so the job total
moves by more than the saving in both directions, and a conclusion drawn from
one pair of runs will say whatever that pair happened to do. This is the ±40s
noise floor recorded above, measured again from the other side.

**`CARGO_INCREMENTAL=0` on CI is a smaller cache and a slower gate, and the
gate is what is billed.** The cargo entry really is mostly incremental state --
4.4 GB of a 13 GB `target/debug` -- and turning it off took the upload from
3.7 GB to 978 MB and the save step from 45s to 14s, which is 30s of a job that
spends 95s moving that file around. Every one of those numbers is real and
none of them mattered, because the same change removed the cross-run
compilation reuse the entry existed to provide, and a CI run recompiles by
definition. Measured on this branch: **479s before, then 559s, 568s and 603s
over three consecutive runs after**, against a run-to-run spread that had been
about 40s. It is a regression of roughly 90-120s and it was reverted.

The lesson is the one the numbers above are easy to misread: **GitHub bills
wall clock, not work.** The suite's Maven *work* was cut from 730.2s to 478.0s
in the same period and the job got slower anyway -- `tests/cli` measured 212s
before the second batch of merges and 218s after. Reducing total CPU only
reduces wall while the machine is actually saturated, and a measurement of
work is not a measurement of the bill. Take the wall clock of the whole job
from the runs themselves before believing any of it.

**The generated-project cache cannot be made to survive a CI run, and the
reason is worth recording so nobody spends the afternoon on it twice.**
`cached_toolchain_dir` reuses a persistent generated tree for as long as the
`jails` binary that produced it is unchanged, stamped with that binary's length
and mtime. On CI the mtime is always new -- every run rebuilds -- so the stamp
never matches, and the ~276s of Maven work behind `proof-apps` and the Spring
toolboxes is paid again every run. Keying the stamp on a *hash of the
executable's contents* looks like the obvious fix, and it was tried: with the
tree deleted a proof-app test costs 112s, and run again against an unchanged
binary it costs 24s, so the mechanism does work.

It buys nothing on CI, because **the binary is not reproducible**. Touch one
source, relink, and the bytes differ -- measured at `18b980f6cf76e061` against
`49f6bd7ac5cf75e3` for identical sources -- since the dev profile splits
codegen across many units and their output is not deterministically ordered. A
fresh CI compile therefore yields a different binary every run, so a content
hash misses exactly as often as an mtime does. Making it deterministic means
`codegen-units = 1`, which costs far more compile time than the cache could
return.

**And the stamp was never the real obstacle, which is the correction that
matters here.** A stamp is only a guess at "would this binary produce the same
tree"; the tree itself answers it exactly. That was implemented: park the
superseded directory instead of deleting it, regenerate, compare the two
source trees byte for byte, and move the old `target/` back only when they
match. It needed one exclusion found by measurement rather than reasoning --
`.jails/lock` holds the pid of the run that took it, and a `journal.bin` under
`.jails/receipts` differs every time, so jails' transaction store had to be
left out while `.jails/generated` stayed in -- and after that all four
persistent fixtures reported `reused=true` on a relinked binary.

**It was then measured and deleted, because it is worth nothing.** `tests/cli`
came in at 147s and 149s with no reuse and 150s and 147s with all four
fixtures reusing. The javac output it preserves is about a second of a 30s
Maven run: what those runs cost is a JVM starting, a Spring context booting
and the tests executing, and reuse cannot skip any of it. The 67s figure that
made the fixture cache look valuable was a **cold `~/.m2`**, not a cold
`target/` -- the same trap as every other cold-versus-warm number in this
file, and CI caches `~/.m2` already.

So the stamp stays on the mtime, and the workflow keeps deleting
`target/jails-e2e-cache` before saving: an entry that can never be hit is
upload, download and storage for nothing. The cache is worth having *within* a
machine's working session, which is what it was written for.

**The runner is at a perfect four-core packing, and no scheduling change can
improve it.** This was the open question -- `tests/cli` measured 147s here and
296s there, and nothing local explained it -- and the always-on subprocess
summary answered it on its first CI run, `33413442610`:

```
run-tests: 276.4s wall for 33 binaries
run-tests: subprocess cost mvn 693.1s over 36, jails 275.6s over 171, docker 137.5s over 10
run-tests: 1106.2s of subprocess work in 276.4s (mean concurrency 4.00), 33.6s queued for a permit
```

**1106.2s of work at concurrency 4.00 on four cores.** A perfect packing is
276.6s; the observed wall is 276.4s. There is no idle to fill and no queue to
drain -- 33.6s of permit waiting across 217 subprocesses is a rounding error --
so ordering, thread counts, a larger permit budget and a cleverer work-stealer
are all worth **zero** there. Do not spend time on any of them.

**And the premise behind the gap was wrong.** The same summary on this machine
the same afternoon: 1253.6s of work in 408.5s, concurrency **3.07**, with
**234.1s** queued. The runner does *less* work, packs it *better*, and waits
*less*; it is the developer machine that is slower and contended. The 147s
that started this came from a session whose container was in better shape than
the one measuring 408s, so the comparison was between two local environments
rather than between a local one and CI. Take both halves from the same summary
or the gap is an artefact.

What follows from a perfect packing is arithmetic: wall clock is work over
four, so **four seconds of subprocess work removed buys one second of wall**.
Maven is 693.1s of the 1106.2s across 36 runs, averaging 19.3s, which makes
`plan.md`'s batching item the only lever with the size to matter -- and prices
it honestly. Collapsing 36 runs toward a dozen saves the per-run floor about
twenty-four times, roughly 156s of work, which is **39s of wall**. Worth
having, and nowhere near a minute and a half.

**Run one gate at a time.** `cached_toolchain_dir_with_salt`
(`tests/common/mod.rs`) shares one persistent fixture per label under
`target/jails-e2e-cache` and takes no lock:

```rust
if root.exists() { fs::remove_dir_all(&root).unwrap(); }
fs::create_dir_all(&root).unwrap();
```

Two processes racing there fail in two ways. One walks `remove_dir_all` while
the other creates files underneath it, and the remove dies `DirectoryNotEmpty`
(errno 39) on that `unwrap`. Separately, `.jails-generated-ready` is written
*before* the directory is filled, so a second process reads it, returns "reuse
this", and runs against a half-built toolbox -- `add kafka failed in the
services Spring toolbox`.

The Maven budget is a `flock` precisely so the suite can be launched however
you like; this fixture is the shared state that is not, so the budget is
process-safe and what it guards is not. `plan.md` P13.10 has the fix.

If two runs have overlapped, `rm -rf target/jails-e2e-cache` before believing
the next result -- a half-built toolbox is stamped ready and will be reused.
The failures read exactly like real `capabilities::` regressions.

**A test that waits is worse than a test that works.** The single most
expensive test in the suite was `run_starts_compose_services_only_when_
explicitly_requested`, at **30.0s** -- the entire wall clock of the `tooling`
module. It asked a narrow question (does compose go up before Spring?) with a
fake `docker` that starts no container, so `jails run`'s readiness probe spent
its whole 120 x 250ms budget failing to reach a PostgreSQL that was never
going to exist. Shortening the production budget would have been the wrong
fix; the fixture stops lying instead. `common::listening_loopback_port()`
holds a real socket open and the compose file declares its port, so the probe
finds what the fake `docker` claims it started: **0.02s**, and a better model
of the case, not a weaker one. It was also *flaky* before, failing under
full-suite load and passing alone. When a test is slow, ask what it is waiting
for before asking how to make the waiting faster.

**It happened again, and the second instance is the one that shows how to
find them.** `a_timed_warm_run_cancels_the_request_and_recycles_the_daemon`
proves that a one-second budget cancels a request still in flight, so its
fixture's `SlowTest` sleeps thirty seconds -- and its warm-up ran
`jails test --fast` with no selector and sat through every one of them. In the
subprocess profile that showed up as **33.7s at 201.6s of a 238.1s span, with
occupancy 1.0**: the whole binary was one test waiting, on an otherwise idle
four-core box, so those thirty seconds were thirty seconds of the suite's wall
clock. Compiling `SlowTest` and naming a trivial `PingTest` as the warm-up's
selector leaves the timed run exactly the margin it needs -- it passes
`--compile none`, so the class still has to be there -- and costs **5.3s**.

The way to find the next one is the occupancy timeline rather than the
per-test totals: bucket every profiled subprocess by the second it was
running, and look for a stretch at 1.0 near the end. A slow test inside a
saturated stretch costs a fraction of itself; a slow test alone in the tail
costs all of it.

## Package layout

Generated code does **not** all land in the base package. `generate::layout`
maps each kind to the subpackage its layer conventionally owns. There are
**eleven** layers, and `config::LAYERS_IN_ORDER` is their single owner:
`domain`, `app`, `service`, `web`, `api`, `messaging`, `cli`, `clients`,
`jobs`, `adapters`, `testkit`. `--package` overrides the placement. Two consequences worth knowing before editing
templates:

- **`scaffold` now crosses package boundaries**, so `stub_repository`,
  `service_full`, `controller_full` and `controller_test` take an `extra`
  parameter holding the imports that costs. `import_of` returns an empty
  string when the two packages match, which is what keeps `--package ''`
  (everything flat) compiling.
- **`destroy` acts on what the store recorded, and nothing else.** It stops
  declaring the entity and reconciliation works out what that means: a file
  only that entity owned becomes an absence, a dependency another entity still
  claims stays. There is no path table and no recomputation — `KIND_FILES`, 672
  lines of hand-written `(tree, layer, placement, filename)` rows, is deleted,
  and so is the `recomputed_paths` trick that replaced it. **Adding a kind
  therefore needs no destroy arm at all.**

  A kind with no recorded row prints why, naming the generate command that
  would record it — not a bare "nothing to destroy" over files that are right
  there. `tests/agreement.rs` runs every scenario forward and back and fails
  both ways: a path `destroy` names that nothing generated, and a file
  `generate` wrote that `destroy` would strand. A file that is *deliberately*
  kept — a migration, a fixture, a shared `SchedulingConfig` — goes in
  `ALLOWED_LEFTOVER` **with its reason**.

  The one sweep that goes beyond the record is `destroy strategy`: a strategy
  is an interface plus a bean per implementation, and an implementation written
  by hand afterwards is still one of its classes. Leaving it behind
  implementing a deleted interface stops the project compiling, so it is
  declared as an absence with `force` — the flag that means "the bytes are not
  jails'" — and the deletion prompt is the human ask that authorises it.

## Import order is normalised at write time, not in templates

`write_new_file` runs every `.java` file through `normalize_imports`: static
imports first, blank line, then the rest sorted -- which is what
palantir-java-format produces, so `add format` leaves a project that passes
`jails check` with no manual `jails fmt`. Don't hand-order imports in
templates; it decays the moment someone adds a template and nobody notices
until spotless fails on a freshly generated project.

Formatter *wrapping* is a different matter and cannot be predicted from a
template, so `add format` runs `spotless:apply` once (best-effort -- a machine
without Maven just gets a note).

## Table constraints live in the field spec, and are a closed set

`@pk`, `@unique`, `@index`, `@positive`, `@nonnegative` parse off the *type*
(before the `!`/`?` suffix, so either order works) into `Field::constraints`,
ride through `sql::Column`, and are read only by `create_table`. They change
SQL and nothing about the Java type.

**`@scope` is the exception and does not touch SQL at all.** It marks a
request-boundary field that must be proved against a same-named JWT claim, and
`spring::require_scope_authorizer` refuses any scoped operation when
`add security` has not written a `ScopeAuthorizer`. It is how tenancy works
without the word "tenant" existing in core.

Two rules that are the whole point:

- **An unknown marker is an error, not a no-op.** `@primary` silently meaning
  "no constraint" would produce a schema quietly missing the primary key
  someone believed they had asked for -- which is the failure this feature
  exists to remove, reintroduced.
- **No arbitrary SQL.** `@check(...)` taking a predicate would be a string
  jails passes through and cannot validate. `@positive` is one jails can
  confirm it is emitting against a numeric column, and it rejects the spec
  otherwise. The two exotic constraints a project actually needs are cheaper
  to write by hand than a passthrough that fails at `flyway migrate`.

`--index` (repeatable, on `g scaffold`) carries what a per-column marker
cannot: composite or ordered. Its column names are validated against the table
first -- `sql::validate_index` splits on whitespace so `created_at desc` is
read as a column plus ordering rather than as a column called
`"created_at desc"`.

A record read off disk carries **no** constraints (`fields_from_record`
defaults them): the Java type cannot say what the column is, and inferring a
primary key from a component called `id` would put one in a schema nobody
asked for.

## Field syntax: case is the rule

`parse_fields` reads `name:type[!?]`. **Lowercase = jails' table, capitalised =
a type the project owns**, passed through verbatim with no import (same
package). `builtin_by_java_name` is the exception that keeps `id:String`
working; without it a natural spelling would be read as an unknown project type
and silently disable the generated test.

**There is one parser, and it lives in `jails-protocol`.** `parse_fields` maps
each token through `FieldSpec::parse(..)?.projected()`; `derive_field` stays in
`jails-spec` because it is derivation, not parsing. Two parsers of this syntax
was the repository's most reliable drift generator and it cost two live
divergences before they were merged (`pending.md` §6.3): `amount:Currency` meant
`java.util.Currency` to one and a project enum to the other, and
`g field X ref:SomeOwnedType` did not work at all, because the projection
renders an owned type fully qualified and `resolve_type` matched case on the
whole token. **`builtin_by_java_name` is the authority on which Java spellings
are builtins** -- `Currency` is deliberately not one, because an enum of the
currencies a project deals in is an ordinary thing to generate.

The suffix sets `Optionality`: `!` non-blank, `?` unchecked/nullable, bare
non-null. `needs_null_check`/`needs_blank_check` are the only two places that
decide, so `record` and `value` cannot drift apart.

**`sample_value` returns `Option`** because jails has no type model: it cannot
know a `SourceRef` constructor. An enum it *can* handle -- `is_enum` reads the
file -- which is why `generate enum` earns its place twice. When a sample is
impossible the companion test is emitted whole and `@Disabled`, naming the
component; emitting a guess would produce a test that does not compile, and
emitting nothing would silently drop coverage.

**`?` emits an `Optional<T>` component**, and the compact constructor
normalises a null one with `requireNonNullElse(x, Optional.empty())` -- a null
`Optional` being the one thing worse than a null value. This is a deliberate
departure from `java.md`'s "Optional as a return type only, never a field": a
record component is both at once, and the alternative (a nullable component
plus a differently *named* Optional-returning method, since an accessor cannot
be overridden to change its return type) is worse on every axis. `?` also
rescues the sample problem -- `Optional.empty()` is a valid sample of a type
jails knows nothing about.

## Gotchas hit so far

- **Which directory a canonical command is about is one walk, and both halves
  have to use it.** `model_command::owns` -- the canonical/legacy switch --
  tested `.jails/model.jdl` against the *process* directory while the legacy
  engine walked up to the nearest build file. They agreed only at the project
  root, so `jails g record` typed in `src/main/java` dispatched to the legacy
  engine: Java landed in the reader's own tree instead of `.jails/generated`,
  and a `.jails/ledger.toml` appeared in a project that must never have one.
  `model_command::project_root` is now the same walk plus the two model
  markers, nearest wins. Its other half is `model_command::read_source`: every
  model path stays project-relative, because the same value becomes a
  `ProjectPath` in the exact plan and `ProjectPath` refuses an absolute one, so
  only the *read* is anchored. `--manifest` is the exception and is resolved
  absolute, since the reader typed it in their own directory. Anchoring the
  default instead put an absolute path into every report -- `model check` said
  `model valid: /tmp/.../.jails/model.toml`.
- **Capture reads the pre-patch model; the compiler emits from the patched
  one, and three defects came out of that one gap.** `capture` decided which
  reader trees to read from the model on disk, so the command that *declares*
  a thing never saw what it needed: `add db` did not read `src/test/java`, so
  the `@Import(TestcontainersConfig.class)` splice had nothing to splice into
  and `mvn verify` failed on the `contextLoads` test nobody wrote; `g command`
  did not read `src/main/java`, so nothing registered in `App.java`; and
  `emit_component::entry_point` read `snapshot.model.model`, so `g cli` did not
  retarget `<mainClass>`. `capture_planned` takes the *intended* model for the
  tree decision, and `entry_point` takes `next_model`. **A test that runs two
  commands and then reads the tree does not catch any of this** -- the second
  command repairs the first one's omission from the model it left behind, so
  assert after each command.

- **The scenario table is the one place a new kind gets registered, and a
  test enforces it.** `tests/common/scenarios.rs` holds `SCENARIOS` — every
  artifact kind and every capability in the smallest invocation that
  exercises it — and three targets read it: `tests/golden.rs` snapshots the
  bytes, `tests/agreement.rs` checks `generate` and `destroy` agree, and
  `every_kind_and_capability_has_a_golden_scenario` reads the kinds and
  capabilities out of `jails generate --help` / `jails add --help` and
  **fails when one has no scenario**. This is not decoration: twelve kinds
  and capabilities had zero coverage before it existed, eight kinds having
  been added without the golden count moving off 162 files / 25 scenarios.
  So **add the `Scenario`, do not add a fourth list** — which kinds a
  scenario covers is derived from its steps, not declared beside them.
  `format` is the single documented exemption (it shells out to
  `spotless:apply`, so its output depends on the toolchain rather than on
  jails); it is listed in `COVERED_ELSEWHERE` with the test that does cover
  it, and that test's existence is asserted.
- **`destroy` and `generate` are checked against each other for every kind.**
  `tests/agreement.rs` runs each scenario, attributes the created files to the
  command that wrote them, then runs `destroy --pretend` and compares. Both
  directions fail: a path `destroy` names that nothing generated, and a file
  `generate` wrote that `destroy` would strand. It found the `usecase`
  outbox sink port and its Kafka implementation being stranded on the first
  run. A file that is *deliberately* kept — a migration, a fixture, a shared
  `SchedulingConfig` — goes in `ALLOWED_LEFTOVER` **with its reason**.
- **Generated projects target Java 26** (`pom::TARGET_RELEASE`), the current
  GA product default. Adopted Maven and Gradle projects keep their configured
  release, with 21 as the supported compatibility floor; generation must not
  rewrite an adopted release merely because the default advanced. The checked
  mise toolchain is 26 so strict generated-project tests exercise the same
  release new projects declare.
- **Tier-3 tests gate on `real_java_supports_target_release()`, not just
  on a JDK being present.** A JDK older than the target rejects
  `--release N` outright, so presence is not enough. Without the gate the
  suite goes red on any machine that hasn't installed the new JDK yet.
- **`base_package()` falls back to the shallowest .java file.** Requiring
  `*Application.java` only works on Spring projects; `new-cli` projects have
  `App.java`, and `add` would fail on exactly the projects it is most useful
  for.
- **`add json` is Jackson 3 (`tools.jackson`), and that is one artifact, not
  two.** java.time is built into core databind in 3.x, so
  `jackson-datatype-jsr310` is not merely unnecessary -- adding it drags in the
  2.x line beside the 3.x one that Boot 4's web starter already provides. Two
  Jackson majors do not conflict (the packages differ), nothing warns, and
  half the code ends up on a mapper nobody configured. `doctor` reports that
  case as a FAIL. Other 3.x differences the templates depend on, all verified
  in `deps/jackson-databind`: `JsonMapper.builder().build()`,
  `JacksonException extends RuntimeException` (so no `throws
  JsonProcessingException`), and `WRITE_DATES_AS_TIMESTAMPS` moved to
  `cfg.DateTimeFeature` where it already defaults to `false`.
- **`jails check` is `mvn clean verify`.** Incremental `verify` leaves deleted
  tests in `target/`, and Surefire still runs the leftover `.class`. Don't
  "optimize" it back to bare verify.
- **Boot 4 split the servlet test slice, and `spring-boot-starter-test` does
  not bring it in.** `@WebMvcTest` and `@AutoConfigureMockMvc` live in
  `spring-boot-webmvc-test`, so a generated test that uses either needs
  `spring-boot-starter-webmvc-test` declared -- verified in
  `deps/spring-boot/starter/spring-boot-starter-test/build.gradle`. The
  dependency is supplied from the **write path** (`generate::ensure_webmvc_test`
  and `writes_a_webmvc_test`), keyed off the emitted bytes, for the same reason
  AssertJ and Failsafe are. `pom::webmvc_test_import_for` picks the package the
  project's Boot version has, so a Boot 3 project renders the legacy one and
  gets no dependency it does not need.

  **The test fixture must not supply what the tool is supposed to supply.**
  `SPRING_FIXTURE_POM` declared that module while `jails new` did not, and for
  months every real-toolchain test compiled against a POM the tool never
  produces -- hiding a release blocker where `mvn verify` stopped on the test
  jails itself wrote, so no Spring test in any generated project ran.
- **`jails run` resolves the POM's `<mainClass>`, and `g cli` moves it.** A
  project with two dispatchers has two `main` methods, and searching source
  picks whichever the walk reaches first -- which is how a jar and `jails run`
  came to start a different class from each other. The POM is Maven's own
  record of the entry point, so it is the one jails reads.
  `cli::adopt_as_entry_point` retargets it, but **only off a stub jails wrote
  with no command registered in it**: once `App` dispatches something it is the
  project's real CLI, and moving the jar out from under it would break what the
  reader built.
- **Anything running compiled project code takes `JAVA_HOME`'s JVM, never
  PATH's.** `jails_support::process::java_program()` is the one resolver, and
  `testd`, `jails run --no-build` and the plain-Java `run` all go through it.
  Maven compiles under `JAVA_HOME`, so that is the release the `.class` files
  carry; the first `java` on PATH is a different question. A machine with one
  JDK cannot tell them apart, and this one has two -- 26 from `mise.toml` and
  21 beside it for the pinned-Gradle example -- so `/usr/bin/java` loads
  nothing a modern build produced:

  ```
  UnsupportedClassVersionError: class file version 70.0, this version of the
  Java Runtime only recognizes class file versions up to 65.0
  ```

  Probing which `java` a *reader* would get is the other question, and that one
  belongs on PATH.
- **A `testd` run that produces no cases refuses; it does not complete.** The
  completed frame carries the daemon's console output on its **first case**, so
  a run with zero cases has nowhere to put a diagnostic: the coordinator gets
  `passed: false`, `cases: []`, and prints nothing at all while exiting 1. The
  refusal frame has fields of its own, and `summarize` fills them with the head
  of JUnit's output and every `Caused by:` line -- the two places it says why
  it found nothing.
- **`JAILS_MAVEN` names the Maven command, and mvnd is probed before it is
  chosen.** mvnd writes a registry under the Maven user home *before* Maven
  runs, so a read-only home kills it with a non-zero exit indistinguishable
  from a failing build at the call site -- a retry there would re-run a
  genuinely broken build. `maven::mvnd_can_start` answers it up front instead.
- **`java::types_annotated_with` is the one walk of `src/test/java`.** A walk
  that matches a raw substring reads the `@SpringBootTest` inside
  `TestcontainersConfig`'s own Javadoc example as a declaration, which makes
  `doctor` name the wrong container config and then report every other test as
  missing an import of it.
- **The `@Import` splice lives in `jails-codemod`, not in `add`.** Two engines
  perform it -- `jails-engine/src/route/support.rs` and the canonical
  projection -- and a second copy of a surgical edit to a file the reader owns
  is a copy that drifts.
  `jails_java::annotate` is text in and text out: `splice_import`,
  `unsplice_import`, and `is_spring_boot_test`, which reads through
  `java::blanked()` so the `@SpringBootTest` in `TestcontainersConfig`'s own
  Javadoc example is not mistaken for one on a class.
- **`add db`'s test wiring is an imported `@TestConfiguration`, and both
  halves are load-bearing.** The container is declared as a `@Bean` with
  `@ServiceConnection` in `TestcontainersConfig` (not a
  `@Testcontainers`/`@Container` static field: Spring caches the context past
  the container's JUnit-managed lifetime, and later tests then fail against a
  stopped container). It is `@Import`ed rather than registered globally: an
  `ApplicationContextInitializer` in test `META-INF/spring.factories` gives
  every `@SpringBootTest` a DataSource for free *and* makes every pure slice
  and `@WebMvcTest` start a PostgreSQL it never queries.
  The pressure toward the global version is real, though: once the JDBC starter
  is present, auto-config demands a DataSource for **every** `@SpringBootTest`,
  including the `contextLoads` test that shipped with the project. So `add db`
  splices `@Import(TestcontainersConfig.class)` into the `@SpringBootTest`
  classes already on disk (`install_test_container_import`), including ones in
  other packages, which need the import statement too. **Deleting a leftover
  `spring.factories` is not optional** -- left behind it keeps registering the
  old initializer, a second container starts for every test, and the migration
  looks like it did not work. Nothing calls `start()`:
  `spring-boot-testcontainers` registers
  `TestcontainersLifecycleApplicationContextInitializer` from its own
  `spring.factories`, so that module is a required dependency.
- **`add db` writes `spring.datasource.*` for the application itself**, read
  back out of `compose.yaml` rather than assumed. Spring's docker-compose
  module supplies these where it works and its connection details take
  precedence, so the properties are redundant there and load-bearing
  everywhere else — without them the app dies at startup on any machine
  whose compose provider Spring cannot drive.
- **`add db` on Spring must wire tests, or `mvn verify` goes red on a test
  nobody wrote.** Docker Compose is skipped in tests
  (`spring.docker.compose.skip.in-tests=true` by default), so JDBC auto-config
  has no URL and fails with "Failed to determine a suitable driver class". The
  `@Import` splice above is what prevents it. JDBC auto-config also registers
  persistence-exception translation, which CGLIB-proxies every `@Repository`
  and fails on `final` classes; `add db` disables it with
  `spring.persistence.exceptiontranslation.enabled=false` in main
  `application.properties` (raw SQL, no ORM). Do not "fix" any of this by
  setting `skip.in-tests=false` (that would share the compose database with
  tests) or by writing a `src/test/resources/application.properties` that
  shadows the main one.
- **`record`/`command` are the plain-Java kinds.** They work in `new-cli`
  projects without framework dependencies. A record occupies two paths
  (`<Name>.java` + `<Name>Test.java`), and `generate` refuses to overwrite
  either one.
  `command` **does** now register itself in the project's dispatcher, which
  `new-cli` provides as `App.java` (a Hello World stub would leave
  `generate command` -- the obvious next step -- with nothing to wire into).
  Dispatchers are found by *shape*, not filename: `is_dispatcher()` checks for
  the registry type and the `return commands;` anchor, so both `App.java` and
  `generate cli`'s `<Name>Cli.java` are found. The old rule ("only pom.rs edits a file the user owns") was a
  proxy for the real one -- *an edit must be surgical and leave every other
  byte alone* -- and hand-pasting a dispatch line after every `generate` was
  exactly the plumbing this tool exists to remove. `register_command` splices
  one line above `return commands;`, idempotently, and falls back to the
  Javadoc instructions when there is no dispatcher or more than one.
- **The test fixtures are handed to real Maven now, so they have to be valid
  poms.** `write_plain_fixture`'s pom had no `modelVersion` and no `version`
  for as long as nothing ran Maven against it; the moment
  `ledger_cli_manifest_builds_without_spring` did, every goal failed with
  `'modelVersion' is missing`. There was also a *second*
  `write_plain_fixture` in `tests/cli/` shadowing the shared one, whose
  doc comment said "Still never handed to Maven" — deleted.
- **`sql::table_name` is the only pluraliser, and `web::resource_path`
  delegates to it.** A second one does not stay in step: the framework-free
  handler served `/categorys` over a table called `categories`, while the
  Spring scaffold's controller (which did go through `table_name`) used the
  right path for the same resource. Irregulars are a deliberately short list
  matched on the last word, plus a short uncountable list; there is no
  `jails.toml` override, because derivability is what lets `destroy` find
  what `generate` wrote.
- **Commons CSV renamed `Builder.build()` to `Builder.get()` in 1.13.**
  The pinned version and the generated call have to move together; a unit
  test in `add.rs` asserts they do, because the mismatch only surfaces as
  a compile error in the real-toolchain tier.
- **Don't use preview features in generated Java.** Structured concurrency
  is on its seventh preview and primitive patterns their fifth as of JDK
  27 — anything preview needs `--enable-preview` wired into both compile
  and surefire and breaks on the next JDK. String templates (`STR."..."`)
  were withdrawn and do not exist at all.
- **`mvn spring-boot:run` exits 0 on a failed startup.** spring-boot-devtools
  runs `main` on its own `restartedMain` thread and catches the exception
  there, so Maven prints BUILD SUCCESS over a dead application — `jails run`
  reported success for an app that never came up. `run::run_watched` pipes
  the output, scans it for `why::FATAL_MARKERS`, and explains the failure
  inline. Piping costs the child its terminal, so the Spring path also passes
  `-Dstyle.color=always` and `spring.output.ansi.enabled=always`; drop those
  and `jails run` goes monochrome.
- **`spring-boot-docker-compose` cannot drive podman-compose.** It shells out
  with Docker Compose v2 syntax (`--ansi never`, `config --format=json`);
  podman-compose spells the first `--no-ansi` and has no `--format` at all,
  so it exits 2 and the app dies during startup. `jails add db` adds that
  dependency on Spring, so on this machine every such project needs
  `spring.docker.compose.enabled=false` — jails already starts the services
  itself in `run`/`start`, so nothing is lost. `why` has the rule.
- **`docker` here is podman's CLI shim.** `docker info --format
  '{{.ServerVersion}}'` exits 125 against podman's differently-shaped info
  report, and `podman-compose` rejects `compose ps --services --status`.
  `doctor` therefore probes with bare `docker info` and `docker ps --format
  '{{.Names}}'`, which behave identically on both engines. Don't "improve"
  either back to the Docker-specific spelling.
- **Testcontainers and the `docker` CLI look at different sockets.** The
  shim talks to podman's rootless socket; Testcontainers reads `DOCKER_HOST`
  or `/var/run/docker.sock` and finds neither, so `jails start` succeeding
  proves nothing about whether `@SpringBootTest` can start a container —
  it fails with "Could not find a valid Docker environment" (47 occurrences
  of the sibling DataSource failure and 8 of this one across one day of
  real sessions). `doctor` checks it; `why` explains it.
- **Testcontainers 2.0 renamed every module** (`postgresql` ->
  `testcontainers-postgresql`). `doctor` matches on the `org.testcontainers`
  groupId alone for that reason — a check that silently stops applying after
  a dependency bump is worse than no check.
- **A capability's plan is a pure function of the project, so order matters and
  `sync` is the repair.** `add api` renders a `DuplicateKeyException` → 409 arm
  only when the JDBC starter is present, because the exception is Spring's from
  `spring-tx`; an unconditional arm would hand an `api`-without-`db` project a
  compile error for a file it did not write. `jails add db api` is right because
  `dispatch::one_transition_each` **re-resolves the project between
  transitions** — it did not, and `api` planned against a pom that did not yet
  have the starter `db` had spliced two lines away. `add api` then `add db`
  cannot be right, and is not meant to be: `doctor` reports it and `jails sync`
  re-plans every recorded capability. Anything else whose rendering depends on
  what an earlier capability installed inherits both halves.
- **Two capabilities own `management.endpoints.web.exposure.include`.**
  `actuator` and `observability` each install their properties as their own
  marked block, and `.properties` is last-wins — so `add observability` then
  `add actuator` would leave `prometheus` unexposed and the scrape 404ing with
  nothing in the logs. `spring::exposure_include` reads the current value and
  unions, which makes the order stop mattering. A new capability touching that
  key must go through it too.
- **`add observability` generates a `MeterRegistryCustomizer` calling
  `config().commonTags(...)`, and Boot 4 moved that interface out of
  `actuate.autoconfigure` with no shim** — so the import is version-sniffed
  like `@AutoConfigureMockMvc` is. That is the part worth remembering.

  **It is not the only thing that would work, and the reason matters if
  somebody proposes replacing it.** `management.metrics.tags.*` does still tag
  every meter: `MetricsProperties.tags` lives in
  `spring-boot-micrometer-metrics` ("Common tags that are applied to every
  meter") and `PropertiesMeterFilter` turns it into
  `MeterFilter.commonTags(...)` on the registry, hand-registered `Counter`s
  included. (`management.observations.key-values.*` is a *different* knob, for
  observations.) The customizer is preferred anyway: it is code the project
  owns, it survives a property file being rewritten, and it does not depend on
  which actuator modules happen to be present.
- **Exactly one repository adapter may carry `@Repository`.** Two make two
  beans qualify for one injection point and the scaffold compiles but cannot
  start — the ambiguity `jails beans` exists to report.
  `generate::repository_wiring` decides: with `spring-boot-starter-jdbc`
  present the `JdbcClient` adapter is the bean and the in-memory one is an
  unannotated fake; without it the adapter is plain `Connection` JDBC (not a
  bean) and the in-memory one is. **`JdbcClient` is not a fallback choice** —
  it lives in `spring-jdbc`, so without the starter the type does not exist
  and the adapter would not compile. The first version of this change emitted
  `JdbcClient` for every Spring project and broke `g scaffold` on any project
  that had not run `add db`; only the real-toolchain tier caught it.
- **`g strategy` is the open counterpart to `g sealed`, and its `destroy`
  reads disk rather than a path list.** A strategy is a port interface plus a
  bean per implementation, which Spring collects into a `List<Port>`. Its
  failure mode is the quiet kind: an implementation missing `@Component` is
  simply not in the list, so it never runs and nothing reports a problem —
  which is why the generated Javadoc says so and why `--on`/`--yields` types
  that are not in the project are named at generation time. `destroy strategy`
  cannot be given the variant list (destroy takes no fields), so it finds
  implementations by reading `java::type_info(...).supertypes` in **every
  main-source directory the recorded rows name** -- not the port's own. That
  is deliberately *better* than a stored list: an implementation added by hand
  after the generate call is still one of this strategy's classes, and leaving
  it behind implementing a deleted interface stops the project compiling.

  **The port is in `domain` and the beans are in `service`**, and the split is
  load-bearing rather than taste: `g scaffold` writes an ArchUnit rule
  forbidding `org.springframework..` inside `domain..`, and the `@Component`
  on each implementation is the thing that puts it in the injected list. Two
  first-party generators disagreeing about where the domain boundary is means a
  red build on a clean generate. The port needs no framework, so it belongs in
  `domain`; the beans carry `@Component`, so they belong in `service`.
  Plain-Maven projects get the same layout with no annotation, because one
  placement is easier to explain than one that depends on the build file.
  `--on` and `--yields` reach the implementations through `import_of`, which is
  what makes `--package` compile: without it the signature names types it never
  imports.

  **`--package` is part of an entity's identity, so `destroy` needs the same
  one.** Two resources of a name in two packages are two resources, which is
  what makes slices possible; what was missing is that a lookup miss reported
  the resource as never generated, seconds after the generate that recorded
  it. The refusal names the recorded package now.
- **A name that already carries its kind's suffix must not get it twice.**
  `strip_redundant_suffix` runs in `generate` **and** `destroy` — applied to
  one and not the other, `destroy` rebuilds different paths and strands the
  files it claims to have deleted. `scaffold` is exempt: it spans Controller,
  Service and Repository at once, so stripping any one corrupts the others.
  It lives in `jails-protocol`'s `recipe.rs` with `recorded_name` and the
  suffix table, because these are **identity** rules — `recorded_name` decides
  the name a ledger row carries — and `jails-engine` was reaching down into the
  generators for one. `generate.rs` re-exports all three.
- **`package-info.java` is written from `write_new_file`, not per-kind**, for
  the same reason import normalisation is — a rule twenty templates must
  remember is a rule that decays. It is conditional on `org.jspecify:jspecify`
  actually being a dependency: annotating a package that cannot resolve
  `@NullMarked` hands the reader a compile error for a file they did not ask
  for.
- **`add kafka` cannot know a topic name, and must not guess one.** The
  capability owns everything topic-agnostic (the `DefaultErrorHandler`, the
  DLT routing, `ErrorHandlingDeserializer`); `g event` owns what needs a
  payload type (`NewTopic` beans, `spring.json.value.default.type`). The
  dead-letter destination is named **explicitly** in the recoverer:
  `DeadLetterPublishingRecoverer` defaults to `<topic>-dlt` and the source
  partition number, so a project declaring `<topic>.DLT` finds it empty with
  only a WARN to say so.
- **A capability's properties are claimed one key at a time, not as a marked
  block.** V1 wrapped them in `# jails:<capability>` … `# /jails:<capability>`
  and `remove` deleted the block wholesale — which took the reader's tuning
  with it, so `unowned_properties` existed to diff the block against what jails
  would write and name the lines it had not written before deleting them. A
  real project had ~20 hand-written Kafka properties inside jails' own markers.
  Under the transaction protocol each setting is a `ResourceKey::Property`
  owned by the capability that wrote it, so `remove` retires exactly those keys
  and never sees the reader's. **`application.properties` therefore has no
  `# jails:` markers in it any more.** The comment jails writes above a key it
  introduces is removed with that key, and only when it is still byte-identical
  to what jails wrote — an edited comment is the reader's prose.

  Marked blocks are still how jails edits `compose.yaml` and the shared
  `src/test/resources/config/application.properties` a durable job writes into:
  those are one *block* per owner in a file with several, which is a different
  shape from one setting per key.
- **Both JDL dialects state field order, and `FieldPlacement` has to agree
  with what re-parsing the source yields.** A record's positional constructor
  is ABI, and one column list feeds the DDL, the select, the insert and the row
  mapper -- so a lost order is a silently wrong argument list, not a formatting
  difference. v1 walks a CST and always knew the order; the pre-v1 draft
  reaches the linker by rendering intermediate TOML, whose tables are
  unordered, so it sorted by label until `render.rs` started carrying
  `field_order`. **`.jails/model.toml` deliberately still sorts**, because it
  is the temporary compatibility input and teaching a format on the deletion
  list to state an order is adding surface to something being removed. The
  trap is that these are *two* decisions that must match: `g field` places the
  new field in the patched model by `FieldPlacement`, and if that disagrees
  with the frontend, the very next command re-renders a different record.
  `patch.rs` records the heuristic this replaced -- "already sorted by label"
  was read as "states no order", which is true until an entity is declared
  alphabetically.
- **clap `alias` vs `visible_alias`**: hidden `alias` is invisible to
  `clap_complete`'s bash generator — `jails g <TAB>` fell back to top-level
  subcommand names instead of `generate`'s completions. Always use
  `visible_alias` for anything meant to be typed interactively.
- **Free-form `String` args don't tab-complete.** Any arg with a closed
  value set (like `generate`'s `kind`) must be a `clap::ValueEnum`, not a
  `String` matched by hand — that's the only way `clap_complete` can emit a
  static completion list.
- **This machine's `mvnd` daemon is flaky under JDK 26** (native-library
  extraction bug, unrelated to jails). `run.rs` still prefers `mvnd` for
  real usage (per spec), but the two real-compile tests under `tests/cli/`
  pin to plain `mvn` — see `real_path_without_mvnd()` in
  `tests/common/mod.rs`. Don't "fix" those tests back to the default PATH;
  they'll flake.
- **`mvn`'s own launcher script shells out to `uname`/`dirname`/`ls`/`expr`.**
  If you isolate PATH for a test (mocked mvn or real-mvn-only), you can
  strip specific binaries (e.g. `mvnd`) out of PATH, but you can't reduce
  PATH to *just* the tool directory — the real `mvn` script breaks with
  "command not found" for coreutils. Mocked fake-mvn scripts don't have
  this problem (they're a single `#!/bin/sh` line with no external calls).
- **A version fact is read off the project, never assumed — and there are now
  three.** `mockmvc_autoconfigure_import` and `webmvc_test_import` were the
  first; `spring::validation_package` (`jakarta` vs `javax`, which Boot crossed
  at 3.0) and `spring::mockmvc_template` (the classic `MockMvc` form for a
  project whose Framework predates 6.2) are the others. All live in `spring.rs`
  because they are questions about *this project* that more than one template
  asks.
- **The Boot floor is in the generated *code*, not its tests.** `pending.md`
  §1.2 read it the other way round and a real Boot 2.7.18 compile disproved it:
  `add api` writes `ProblemDetail` (Framework 6), `add security` writes
  `requestMatchers` (Security 6), and `g query`/`g transition` write a
  `JdbcClient` adapter (Framework 6.1) — all in the main source set, where no
  test variant helps. Those four refuse through
  `spring::require_jakarta_spring`, which names the **type** rather than a
  version, because that is what the compiler would have said. `add cors`,
  `g enum`, `g scaffold` and `g usecase` work there and
  `what_jails_generates_for_boot_2_compiles_and_what_cannot_refuses_by_name`
  compiles and runs them to prove it.
- **Spring Boot 4.x moved `@AutoConfigureMockMvc`** from
  `org.springframework.boot.test.autoconfigure.web.servlet` to
  `org.springframework.boot.webmvc.test.autoconfigure`, no back-compat
  shim. `generate.rs::mockmvc_autoconfigure_import()` sniffs the parent POM
  version and picks the right one — don't hardcode the import again.
- **Tests never call start.spring.io.** `generate_scaffold_produces_a_
  project_that_compiles_and_passes_tests` and friends use a hand-written,
  version-pinned fixture (`write_spring_fixture` in `tests/common/mod.rs`)
  instead. Keep it that way — don't reintroduce a network dependency into
  the test suite.
- **Each crate gets its own test binary**, so `#[cfg(test)]` modules within
  one crate share a process and one process-global current directory. Any test
  that calls `std::env::set_current_dir` MUST hold `jails_testkit::hold_cwd()`
  for the duration, or parallel tests race on it. The lock lives in
  `jails-testkit` and is deliberately **not** `#[cfg(test)]`: the crates that
  need it are not the crate that defines it, and one instance per dependent
  test binary is exactly the scope it has to cover. **Take it through
  `hold_cwd()` and never with `.lock().unwrap()`**: a holder that panicked
  poisons the mutex, and the next test to ask for it then fails with a
  `PoisonError` naming neither the panic nor its cause. That is how a full
  `/tmp` came to be reported as two unrelated `new-cli` failures.
- **`#[cfg(test)]` in a library crate means "when *this* crate is under
  test".** A dependent crate's tests cannot see it. That killed
  `parse_fields_for_test`, a `#[cfg(test)]` helper `sql.rs` and `spring.rs`
  called from their own test modules back when one binary held everything. If
  a test helper has to cross a crate boundary, it is ordinary public API — or
  it should not exist, which was the answer there.
- **A flag the reader's distribution might not have is asked about, never
  assumed.** `git merge-file --diff-algorithm=histogram` was passed
  unconditionally, and it reached that command after 2.43 -- so on Ubuntu 24.04
  LTS, Debian 12 and RHEL 9 it exits **129**, a usage error rather than a merge
  outcome, and every regeneration over a file the reader had edited failed.
  Nothing preflighted it and `doctor` did not check it. It also killed 58 tests
  here, which is the worse half: a gate that cannot run reports the same green
  as one that passed, and 29 tests and six real defects were hiding under it.

  `jails_support::git` now **probes** -- it runs `git merge-file` on three
  identical throwaway files and reads the exit status, because `git --version`
  is a string distributions decorate (`2.39.3 (Apple Git-146)`) and the
  question is which release added one flag to one command. Same reason
  `maven::mvnd_can_start` exists.

  **The fallback has a cost, and the cost is why there is an override.**
  histogram and myers can resolve an ambiguous merge differently, so two
  machines can turn one input into two managed trees -- recorded in each
  project's accepted projection. `JAILS_GIT_DIFF_ALGORITHM` pins it for
  everyone: a name to pin that algorithm, or empty to pin git's own default,
  which every git supports. `jails doctor` reports which one this machine
  landed on, because otherwise "why does my colleague's tree differ" has no
  answer anywhere in the product.

  **The gate pins it to the empty value**, in `mise.toml` and therefore in the
  pre-push hook and CI, which invoke nothing else. G0 asks for one answer to
  "is this green", and a gate whose merges depend on the distribution
  underneath it is two answers wearing one name. The cost is that the gate
  never exercises histogram; which hunks git picks is git's business, and the
  halves that are jails' -- the probe matching a direct invocation, and the
  flag landing between the caller's flags and the operands -- are unit-tested
  on whatever git the machine has.

  Both merges go through `git::merge_file_argv`. They live in ladders that
  cannot see each other -- `jails-workspace` canonical, `jails-prepare` legacy
  -- so neither can reuse the other's call, and a board row fails the build on
  a `--diff-algorithm` literal outside `jails-support::git`.
- **`.githooks/pre-commit` runs `cargo fmt --all --check` and `cargo clippy
  --workspace --all-targets` before every commit**, so a lint is a *blocked
  commit* rather than a warning you can ignore. Running it before staging saves
  a rejected commit. `cargo clippy --fix
  --allow-dirty --allow-staged --all-targets` handles the mechanical ones.
  `.githooks/` is tracked, so both hooks are the project's rather than this
  checkout's -- `core.hooksPath` has to point at it, which `git config
  core.hooksPath .githooks` does. **`pre-push` runs `mise run verify-rewrite`
  and nothing else**, per `simplify-sol.md`'s G0: one answer to "is this
  green", so the hook and CI cannot drift. A hook running its own `cargo build
  && cargo test` gets neither `--workspace` (so the root package alone) nor
  `JAILS_TOOLCHAIN` (so the real-toolchain tier does not run at all).
- **The crate is on edition `"2024"`, deliberately** — edition 2026 doesn't
  exist yet, whatever the version number suggests. Leave it alone.
- Install target is `~/.cargo/bin/jails` via `cargo install --path .`
  (already on PATH) — not a symlink into `~/.local/bin` or `~/bin`, which
  is how some other tools in `~/code/my-dotfiles` are wired. Don't
  "helpfully" switch install methods without asking; it was a deliberate
  choice among options.
- Bash completion is registered in
  `~/code/my-dotfiles/home/.bashrc.d/60-completions.sh`, guarded the same
  way as `gym`: `command -v jails &>/dev/null && source <(jails completion
  bash)`. That's a separate repo — changes there aren't tracked by this
  project's git history.

## Generated code tracks Spring Boot 4 / Framework 7, verified against source

The upstream checkouts under `deps/` are the reference, not memory.
`deps.tsv` at the repo root is the manifest (dir -> `owner/repo`) and
`deps-update.sh`, also at the root,
clones what's missing and fast-forwards the rest — both tracked here, while
`/deps/*/` is gitignored because each checkout is its own upstream repo. The
manifest covers every third-party library the payments-gateway-service poms
pull in, not just the ones jails' own templates target, so "check the source"
is answerable for the whole stack. New clones are blobless
(`--filter=blob:none`) since the 13 original full clones already cost 6.3 GB.

One gotcha the script now carries a comment about: **bash declares every name
in a `local` statement before running any of its assignments**, so
`local dir=$1 repo=$2 url="...${repo}..."` expands `$repo` as the blanked
local, not as `$2`. That built `github-personal:.git` and GitHub answered
`remote error: is not a valid repository name` for every repo at once — which
reads exactly like an account-wide ssh rate limit, and is not one. The `url=`
assignment has to be its own statement.

Three things confirmed in those checkouts and relied on by the templates:

- **`@MockBean`/`@SpyBean` no longer exist** in Boot 4 — there is no
  `MockBean.java` in the tree at all. The replacement is `@MockitoBean` /
  `@MockitoSpyBean` from
  `org.springframework.test.context.bean.override.mockito` (it lives in
  spring-framework's `spring-test`, not in Boot). jails never generated
  `@MockBean`, so nothing broke; don't introduce it.
- **`MockMvcTester`** (`org.springframework.test.web.servlet.assertj`) is the
  current MockMvc entry point, and `@AutoConfigureMockMvc` contributes one
  whenever AssertJ is on the classpath. `controller_stub_test` generates
  against it: one fluent chain rather than two families of static imports,
  and no `throws Exception` on the test method.
- **Testcontainers containers should be Spring beans**, not
  `@Testcontainers`/`@Container` static fields — Boot's own reference docs
  warn that Spring caches the context beyond the container's JUnit-managed
  lifetime, so later tests fail on a stopped container. `@ServiceConnection`
  (`org.springframework.boot.testcontainers.service.connection`) is how the
  connection details reach auto-configuration.

## `scaffold` produces a running resource, and that constrains it

The scaffold's controller/service/DTOs are real, not stubs, which means the
generated application has to *start*. Two consequences that are easy to undo
by accident:

- **An in-memory adapter is generated and carries `@Repository`; the JDBC one
  does not.** The JDBC adapter takes a `Connection` the caller owns, so it
  cannot be a bean. Without the in-memory one the context fails with "no
  qualifying bean of type ...Repository" — a scaffold that compiles and
  cannot run. Annotating both would make two beans qualify for one injection
  point, which is the ambiguity `jails beans` reports.
- **`Field.java_type` always holds the *inner* type**, with `Optionality`
  carrying the rest; `component_type` is the only place that wraps it back
  into `Optional<...>`. Storing `Optional<String>` in `java_type` instead --
  which is the tempting thing for `fields_from_record` to do -- gives one type
  two representations, and a template that works for `parse_fields` input then
  emits uncompilable code for a record read off disk.

## Anything that writes an `*IT` must also configure Failsafe

Surefire runs `*Test`; `*IT` is Failsafe's, and Failsafe is **not** in the
Spring Boot parent's default build. jails generated integration tests for
months that never ran once — `mvn verify` completed, reported success, and
executed none of them, which is worse than having no test because the green
build claims it passed. `generate::ensure_failsafe` is called from the write
path (not per-kind) so a new generator cannot forget, and `add.rs` does the
same for capability plans. Both goals are bound: `integration-test` runs
them, `verify` is what makes a failure fail the build.

## A generator that emits code must supply the dependency it needs

**And the version it needs depends on the project's flavour.** A
`<dependency>` with no `<version>` is correct under
`spring-boot-starter-parent`, which manages it, and *fatal* without one:
Maven refuses to read the pom at all and every goal fails, `validate`
included. So anything spliced goes through a flavour-aware chooser --
`spring::validation_dependency` (the Boot starter, or pinned
`jakarta.validation:jakarta.validation-api`, which is the artifact the
generated annotations actually come from), `spring::failsafe_plugin`,
`pom::assertj`. A versionless plugin is the quiet half: Maven only *warns*
and resolves whatever the running Maven defaults to.

**Every generated test is written against AssertJ, so `ensure_assertj` runs
from the write path** — `generate` and `add` both — for the same reason
`ensure_failsafe` does. `jails new`/`new-cli` put AssertJ in the pom, which is
exactly why this went unnoticed: the projects that need it are the ones jails
did **not** create, which is the case §12 is about.


`g dto` splices `spring-boot-starter-validation`; `g client` splices
`spring-boot-starter-restclient`. Handing the reader a compile error for a
line they did not write is exactly the plumbing this tool exists to remove.
`pom::add_dependency` is idempotent, so `ensure_dependency` is safe to call
on every run.

The restclient one is the non-obvious case and cost real time to find:
`@ImportHttpServices` builds the client proxies without it (that part is
Framework, not Boot), so the project compiles and starts — and the first call
fails with `URI with undefined scheme`, a message that says nothing about a
missing module. `spring-boot-starter-webmvc` does not bring it in; serving
HTTP and calling it are separate concerns.

## Scratch directories are reserved, never named

`jails_support::scratch::ScratchDir` is the only thing that creates one, and
`production_scratch_directories_are_exclusively_created` in
`tests/architecture/` fails on an `env::temp_dir()` anywhere in production.

The pattern it replaces was `env::temp_dir().join(pid + timestamp)` followed by
`create_dir_all`, which is not exclusive in **either** half: two callers can
read the same nanosecond, and `create_dir_all`'s whole contract is that an
existing directory counts as success. Both halves failed together about once in
five full-workspace runs — one test was handed another's tree and `jails g cli
Admin` refused over files it had not written. In `jails-prepare`'s `reconcile`
the same collision would have merged a regenerated intent against somebody
else's base.

Three rules, each load-bearing:

- **Never claim a directory that already exists.** `reserve` asks the OS for a
  fresh one. Reaching for `create_dir_all` to "make sure" the scratch root is
  there reintroduces the bug.
- **`Drop` removes only what `tempfile` returned**, never a hand-assembled
  path, so a guard cannot delete a directory it did not create.
- **Cleanup failure is reported on the explicit path.** `Drop` cannot return an
  error, so success paths call `close()`; a panic still cleans up silently,
  which is the right trade when the process is already failing.

`keep()` hands the directory over for recovery storage — and for test fixtures,
which outlive the test on purpose so a failure can be inspected.

`tempfile` is a normal dependency of `jails-support`, not a dev one, because
reconciliation needs the guard in production. It is the only third-party crate
here besides clap.

## Testing philosophy

Three tiers, don't blur them:
1. **Unit tests** (colocated `#[cfg(test)] mod tests` per file) — pure
   functions and filesystem-only logic, no Maven, no subprocess.
2. **Mocked-mvn integration tests** — a fake `mvn`/`mvnd` shell script that
   just logs argv, for verifying `run.rs`'s command construction (which
   binary, which flags) without needing real Maven.
3. **Real-toolchain integration tests** — actually invoke `mvn`/`javac`
   against a fixture project, gated on `mvn`/`java` being on PATH (skip
   gracefully, don't fail, if absent). This is the only tier that answers
   the question the whole tool exists for — "does it produce a project that
   actually compiles and passes tests?" Don't let tier 2 masquerade as
   tier 3.

**Tier 3 needs three things on the machine, and two of them are not in
`mise.toml`.** A JDK that can compile `TARGET_RELEASE` is pinned there; the
other two are not, and each fails in a way that looks like a product bug:

| missing | what it looks like |
|---|---|
| JDK matching `TARGET_RELEASE` | `release version 26 not supported`, ~50 tests red |
| a running Docker daemon | Testcontainers and the OCI image gate skip |
| a `git` on PATH | `git merge-file` is the three-way merge. Any version works: which diff algorithm it gets is probed, and `JAILS_GIT_DIFF_ALGORITHM` pins one |

None of them is optional for a measurement. A run without them exercises the
first two tiers only, and any timing taken from it describes those two tiers
however confidently it is written down.

**A skipped tier-3 test is reported as passing.** When `TARGET_RELEASE` was
27 — an unreleased JDK — `javac` on a bare PATH rejected it and **11 of the
104 integration tests did nothing** while the suite said green. The move to 25
should have removed that cause; confirm it rather than assume it. Every skip
goes through `common::skip()`, and `JAILS_TOOLCHAIN=1 cargo test` runs the
tier for real, turning anything it cannot run into a failure naming what was
missing. Use it before believing a green run covered the generated-code path:

```
JAILS_TOOLCHAIN=1 cargo test --workspace
```

Note `real_path_without_mvnd()` rebuilds PATH for the real-mvn tests, so
which JDK Maven actually uses is decided by `JAVA_HOME`, not by the `javac`
the gate probed — the two can disagree, and the gate is the optimistic one.
