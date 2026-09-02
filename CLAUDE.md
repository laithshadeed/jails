# jails

Rails-CLI-inspired scaffolding tool for Spring Boot and plain Maven or Gradle
projects, built as one semantic application compiler. `README.md` is the
user-facing surface -- the command list, the field types, what is deliberately
deferred -- and is the spec: update it in the same change as the code.
`ARCHITECTURE.md` is the map. **Every idea, roadmap item and open design
question lives under `docs/`** -- `docs/60-abstraction.md` is the shape the
code is converging on; this file describes what the code *is* and the traps
in it, and nothing here is a proposal.

The scope bar: no ORM, no jails runtime jar, no Lombok, no preview features in
generated Java, and no plugin system with lifecycle hooks. Check `README.md`'s
"Not yet" before adding a command that is not already there.

**Gradle is supported, and `gradle.rs` has one bar to clear: answer exactly or
refuse, never guess.** A tool that half-understands a build file and reports a
dependency the build does not have is the worst outcome available, worse than
refusing. The Gradle adapter appends one marked block and touches nothing else.

## The compiler

```text
.jails/model.jdl / CLI sugar
        -> edited JDL source (+ Evolution)
        -> AppModel + WorkspaceSnapshot
        -> pure Compiler
        -> PlanDraft
        -> exact content-addressed PlanBundle
        -> preview or the one Executor
```

The five contracts are authoritative and `docs/00-contracts.md` carries them:

- `AppModel` is desired-state authority. Stable IDs carry identity; Java, SQL,
  route and configuration names are projections.
- `WorkspaceSnapshot` captures every external fact once. Code below the
  compiler may observe the filesystem; the compiler may not.
- `Compiler` is pure. Equal snapshot, model, evolution and compiler version
  produce equal desired artifacts.
- `PlanBundle` is the exact reviewed transition. Preview, export, confirmation
  and apply refer to its digest; apply never replans.
- `jails-workspace::execute` is the only project writer. It locks, rechecks
  preconditions, publishes exact after-images, and converges on retry.

**"Converges on retry" is proved, not asserted, and the proof needs the
aborting half.** `crates/jails-workspace/tests/crash.rs` trips every point in
`fault::POINTS` twice -- once with an injected `Err`, once in a child process
that `abort()`s inside the trip -- and each row asserts its own point tripped.
An `Err` unwinds, so a staged temporary's guard removes it; a real crash leaves
it on disk, where `verify_preconditions` would read it as an unmanaged file
inside the managed tree and refuse permanently. `write_atomic` stages under
`.jails-staged-` so `sweep_staged` can recognise its own debris, and the sweep
runs under the lock.

**`.jails/model.jdl` is the one editable source.** `model_command::read_source`
is the funnel every mutation reads its model through; it refuses anything
else by name. `app plan|apply` reads a manifest and writes declarations into
the model, one way -- that is not a second editable source, because the model
is what every later command reads. `app init` writes the manifest and is the
one subcommand that refuses on a modelled project.

**Managed output is merge-managed below `.jails/generated`.** The accepted
model renders BASE, capture supplies OURS, the next model renders THEIRS. Clean
merges are frozen into the plan; conflicts refuse without writes; the lock
advances to THEIRS so hand edits remain deltas. Migrations, model revisions and
explicit reader-file patches are irreproducible and stay visible in the plan.
`model eject <artifact-id>` transfers one ejectable implementation into reader
source with a `Missing` before-image -- transfer is creation, never
reconciliation -- and excludes it from later managed trees. Records and ports
remain managed ABI. Ejection never infers ownership from edited bytes.

**Convention is recorded, not hidden: `jails model explain`.** Every name the
compiler derives is a `DerivedValue` in `AppModel.derived`, keyed by owner and
role with the `rule_id` that produced it, so a convention that moves cannot
move silently. It is recomputed from the model after every patch, never
accumulated, and `pinned` is decided by comparing with the convention rather
than by a flag carried from the source. Six of the twenty-three emitted
packages (`repository`, `application`, `ports` and their kin) sit under a head
JDL v1 §9.7 does not close, so a `jails.toml` layer rename does not reach them;
they are displayed as `convention.facet.*` rather than corrected, because
moving them would move files in every project generated so far.

**Rename and field evolution are source edits plus one `Evolution` step, not
lifecycle replay.** `rename resource --strategy preserve-table` keeps the
entity ID and SQL table, pairs BASE and THEIRS by artifact ID even when paths
move, and merges the old live file into the new path. `resource field
rename|type|nullability|drop` edit the field and carry exactly one typed
policy in the evolution: preserve-column rename
emits no migration; single-cutover changes the SQL projection explicitly; safe
type change accepts only proven PostgreSQL widenings; required nullability
captures the reader-owned backfill file as a precondition; drop requires the
accepted column and refuses while an operation references the field. Rolling
and expand/contract are campaigns and refuse.

**Destroy is subtraction.** Removing a declaration and compiling is the whole
of it; there is no reverse renderer and no file table. Removal refuses while an
operation edge still points at the declaration. A stored entity requires
`--storage preserve|drop`: preserve keeps an inactive node that exact-table
revival reuses; drop appends one forward migration. Indexes are stable entity
children and `resource index add|remove` appends one forward migration each.

**Capture reads the intended model.** `capture` takes the model the plan
will compile beside the one on disk and decides which reader trees to read
from the *intended* one, and `entry_point` takes `next_model`,
so the command that declares a thing sees what it needs: `add db` reads
`src/test/java` for the `@Import` splice, `g command` reads `src/main/java` to
register in `App.java`, `g cli` retargets `<mainClass>`. A test that runs two
commands and then reads the tree does not catch a frontend that forgot this --
the second command repairs the first one's omission -- so assert after each
command.

**The snapshot is the only project model.** `ProjectFacts` is the one answer
to "what is this project", and `jails_project::capture::observe` is the one
reader that produces it, for the snapshot the compiler reads and for the
`Project` every command above the compiler takes (`jails_project::project`):
a root, the facts, and the captured text of the build file for the questions
`gradle.rs` answers exactly or refuses. A fact a command needs that the
snapshot lacks is added to `ProjectFacts` by capture -- `main_class`,
`build_dependencies` (jails' own block included, because `doctor` asks what
is on the classpath and the compiler asks what the reader declared) and the
`reactor` walk `jails about` prints are the three added this way -- never
read again beside it. No module outside `capture` reads the pom, and the
board's Maven-scanner row holds the count at one.

**Every advertised generator and capability has a compiler backend: 39 of 39
and 25 of 25**, held by `canonical_support::registry_classifies_every_advertised_word`.
**The declarative ones are `Recipe` rows and `emit.rs` walks two tables.**
`crates/jails-compiler/src/recipe.rs` is the shape -- files, dependencies,
properties, compose services, build features, placement -- over a `Node`
(capability, component, operation), and `recipe::render` is the one loop;
`emit.rs` holds `RECIPE_WALKS` and `FUNCTIONS`, and the second is the list
of emitters that still build Java from the model's structure, which
`docs/60-abstraction.md` S60.3 counts. A new kind that is substitution over a
template is a row, not a module; a role appears in exactly one recipe and
`Import::Role` resolves it by lookup.
`scaffold` is one typed entity profile over four facets. `migration` is
deliberately not a declaration (JDL v1 §2.1, §12.6): it joins
`PlanDraft.migrations` as an ordinary `AppendMigration`. Three of the last
four generators needed no emitter, only syntax in front of a backend that
already existed; check for that shape before writing one.

**The controller's companion test drives MockMvc, not reflection.**
`emit_unit::controller_test` issues a real request through the dispatcher, with
`spring-boot-starter-webmvc-test` declared where Boot 4 split that slice out. A
route jails cannot drive is emitted whole and `@Disabled`, asserting status
only. **A weaker generated test is the failure mode to look for when a surface
moves to the compiler**: refusals are loud, a test that passes over a dead
application is not.

**`emit_mockmvc` is the one MockMvc dialect.** Which entry point a project has
(`MockMvcTester` on Boot 4, the classic `perform(...)` below it), the imports
each needs, whether the method declares `throws Exception`, and how a request
and the status it must answer with are spelled -- all of it is decided there,
and the three emitters that drive a route (`emit_http::proof`,
`emit_unit::controller_test`, `emit_resource_http`) consume it. `Status` is a
closed set because the two dialects spell one differently. Written three times
it drifted: the fluent "no `If-Match` applies unconditionally" case was emitted
into classic tests too, where `MockMvcTester` is not imported.

Facts the compiler decides once and no renderer re-derives: how a request
binds (a query is `@ModelAttribute`, a command is `@RequestBody`), what the
`Input` record declares, and what a value for a field looks like --
`emit_companion_test::builtin_sample` is the one reader of
`BuiltinSemantics::sample` and `emit_operation::proof::record_arguments` the
one entity sampler. Two renderers reaching one of those answers separately is
drift.

**Every project file has exactly one owner.** The CI workflow, Dockerfile,
chart and editor settings live under `templates/add/` and are substituted with
`str::replace`, never `template!`: GitHub writes `${{ github.ref }}` and
`docker image inspect` reads `{{.Config.User}}`, so a renderer treating `{{`
as a placeholder reads those files' own syntax as keys. `format!` renders `{{`
as `{`, which silently changes PromQL.

**Canonical `format` refuses on Gradle, by name.** Spotless needs an
`id 'com.diffplug.spotless'` entry inside `plugins { }`, legal only as the
first statement of the script, and the Gradle adapter's contract is that it
appends a marked block and touches nothing else.

`add dependency` / `remove dependency` and `set` / `unset` are not
capabilities: each is a stable model node (`(target, key)` for a setting), and
the compiler reconciles the complete set through the exact marked Maven/Gradle
dependency adapter or the properties adapter, which preserves unrelated bytes,
repairs only keys the previous model owned, refuses reader-owned collisions,
and guards creation with a captured missing-file precondition.

## Crates

A crate may only depend on one below it, and Cargo enforces that;
`no_module_depends_on_a_layer_above_its_own` in `tests/architecture/` enforces
the same rule for module-level edges, and **`LAYERS` in
`tests/architecture/rules.rs` is the authority on which crate a module belongs
to** -- this is the prose, and prose goes stale.

| crate | contract |
|---|---|
| `jails-model` | closed source schema, stable IDs, linking, semantic diagnostics, `AppModel`, `Evolution`, the removal guards, **and every closed vocabulary** -- `Layer`, `CapabilityKind`, `ArtifactKind`, `EndpointMethod`, `RequestFormat`, `Precondition`, `BuildSystem` and the compact field syntax |
| `jails-contracts` | portable `WorkspaceSnapshot`, `PlanDraft`, exact `Plan`, operations, trees and blobs |
| `jails-compiler` | pure semantic lowering; no filesystem, environment or subprocess access |
| `jails-workspace` | exact materialization, verification and the single executor, over what `jails-project` captured |
| `jails-codemod` | the marked block, the `@Import` splice, `blanked`; **no dependencies at all**, so both `jails-compiler` and `jails-project` can reach it |
| `jails-support` | write, run, hash and name: `apply` (the only module that writes), `process`, `hermetic`, `scratch`, `git`, `unified`, `lock`, `digest` (SHA-256, domain separation and hex, re-exported at the root), the validating newtypes, `Result` and `Failure` |
| `jails-spec` | where a project is and what builds it: `find_project_root`, `build`, plus `policy`, `coordinate`, `constant`, `suffix` and `release` (the three version pins a generated project carries). No vocabulary of its own and no clap |
| `jails-testkit` | `hold_cwd()`, taken as a `[dev-dependency]`; not `#[cfg(test)]`, because a dependent crate's tests cannot see one |
| `jails-project` | **the reader**: `capture` (the one place jails looks at a project; its `observe` half produces the `ProjectFacts` the snapshot and every command read), `documents` (the adapters over the reader's build file, properties and compose file, and `pom`, the one Maven reader), `merge` (the three-way merge), `project::Project` (a root plus its `ProjectFacts`), every reader-owned file jails edits: `config` (`jails.toml`), `compose`, `gradle`, `inspect`; and reading Java: `java` (the small reader), `classfile` (the constant-pool reader), `template` (rendering into it) |
| `jails-drive` | commands that **start something**: `run`, `test`, `testd`, `affected`, `migrate`, `kafka`, `console`, `bench`, `lint`; the one edge back down is `run` to `report::why` |
| `jails-report` | commands that **answer a question**: `doctor`, `why`, `explain`, `src`, `commands`; read-only because the crate sits below `jails-drive` |
| `jails` (root) | the binary: `main`, `cli`, `dispatch`, `new`, `app`, the `model_*` frontends, and `tests/` |

Five things to know before touching the workspace:

- **Each crate's `lib.rs` carries a facade block** re-exporting the lower
  crates, so module code says `crate::java` and `crate::Result` wherever it
  ships. Only that block knows which crate a module lives in, which makes
  moving one a one-line change. Keep it trimmed to what the crate references;
  a facade re-export keeps a module alive that nothing else calls.
- **`CARGO_MANIFEST_DIR` expands at the call site**, so `template!` cannot bake
  in its own root. `jails_project::template_at!` takes the root as an argument and
  each crate declares a one-line `template_here!` naming its own; `templates/`
  stays at the repository root.
- **The binary is the root package, not `crates/jails-cli`**, which keeps
  `tests/`, `tests/golden/` and `tests/fixtures/` where they are.
- **Every scanner walks `crates/*/src`, not `src/`**, and asserts a minimum
  file count, because a scanner that has lost the code reports the same clean
  result as one that read it all. Path-matching scanners accept any
  visibility.


## One owner per closed vocabulary, and it is `jails-model`

`Layer`, `CapabilityKind`, `ArtifactKind`, `EndpointMethod`,
`RequestFormat`, `Precondition` and `BuildSystem` are each defined once, in
`jails-model`, and **the CLI's `clap::ValueEnum`s are those same enums**
rather than copies with a `From` at the boundary. The crate carries a `cli`
feature (`cli = ["dep:clap"]`) that the binary and `jails-report` enable and
the compiler ladder leaves off, so `jails-model` still builds with no clap at
all. `jails-contracts` re-exports `BuildSystem`; `jails-project` re-exports
`layout` through its facade.

A member the CLI must not offer is `#[value(skip)]` with the reason beside
it, never a second list: `CapabilityKind::FastTest` is declared by `jails
test --fast`, and `Precondition::None` is a word JDL states and no flag
chooses. Where two closed sets genuinely differ, the difference is a
predicate on the one enum -- `CapabilityKind::addable` for what `jails add`
and `jails.toml` name, `declarable_in_source` for what a JDL `cap` may spell
(JDL v1 §12 makes `db` and `h2` selections of `app.storage`).

`label()` exists where a *file* stores the word: `CapabilityKind` has one
because `jails.toml`, `app.toml` and JDL all write it. `ArtifactKind`
deliberately has none -- nothing stores a generator kind, so clap's canonical
name is the only spelling and a hand-written table beside `value(name = ...)`
would be the second copy.

## Layout

- **The binary's front half is three files, split by the question each
  answers.** `src/cli.rs` is the clap definition -- *what can I type*.
  `src/main.rs` is the module list, the tree it hands to clap, and the match
  from a parsed command to a frontend -- *what does it do*. `src/dispatch.rs`
  turns a result into an exit status and one rendered report. `jails-codemod`
  has a `dispatch` too (the splice that registers a generated command in a
  project's own CLI); `module_of` in `tests/architecture/` identifies a module
  by `(crate, module)`, so the two do not measure each other.
- **The frontends are `src/model_*.rs`**, one per surface: `model_generate_jdl`
  (and its `component`, `operation`, `relation`, `facet`, `unit`, `index`,
  `edit`, `render` halves), `model_capability`, `model_resource`,
  `model_field_evolution`, `model_destroy`, `model_rename`, `model_index`,
  `model_migration`, `model_setting`, `model_eject`, `model_init`,
  `model_explain`, `model_doctor`, `model_status`. Each starts from
  `model_command::Current::load` (the one read of the invocation's model),
  runs its own exact-shape checks and the model's removal guards
  (`refuse_entity_removal` and kin, `jails_model::guard`), edits the JDL
  text, and hands a `PreparedMutation` -- the edited source plus an
  `Evolution` -- to `model_generate::finish_generation`. **The model is
  decided once there**: it is whatever the edited source links to, captured
  over, compiled beside the evolution, materialized and then either reported
  or executed as *that* bundle -- one computation for preview and apply. The
  plan's input bytes are the evolution serialised (`PlanInput`); the edit
  itself is in the plan as the model file's before- and after-image. A
  frontend that re-parses its edited source does so to *check* something
  (that a facet linked, that an index is gone), never to build a second
  description of the change. `src/model_command.rs` is the one owner
  of *which directory a model command is about*: `project_root` walks up to
  the nearest build file or model marker, nearest wins, and every model path
  stays project-relative because the same value becomes a `ProjectPath` in the
  exact plan. `--manifest` is the exception and is resolved absolute.
- **`src/new.rs`** -- `new` (start.spring.io, real network, or `--offline`),
  `new-cli` (hand-written pom, `App.java`, `AppTest.java`) and the Gradle
  project. All three seed the model, and the six default properties are `prop`
  declarations rather than reader-owned text, or a capability declaring the
  same key would collide with the project's own scaffolding. `new` stands in
  the *parent* of the project it creates, so its root rides on `Invocation`
  (`Invocation::for_new` pins it; `Invocation::root` is the one walk) and
  every model function takes the invocation or the root it resolved --
  `materialize_seed(root)` is the seed's entry, not a second walk.
- **`src/app.rs`** -- `jails app plan|apply`: a closed manifest at
  `.jails/app.toml` (`schema`, `capabilities`, `[[generate]]` rows of
  `kind`/`name`/`fields`/`timestamps`/`indexes`/`package`/`on`/`yields`, with
  `strategy_on`/`strategy_yields` as deprecated aliases; both spellings at once
  is an error). A `[[generate]]` row *is* a `GenerateArgs`, replayed row by row
  through the same frontends `jails g` and `jails add` use; every frontend is
  idempotent, so an interrupted replay is repaired by running it again.
  **Deliberately domain-blind**: a crawler, a support inbox and a payments
  gateway are three lists of the same generic intents, and none of them gets a
  command, branch, enum or template in core. A capability wires its own
  integration points (`DocumentIntent::ReconcileSpringTestImport` puts the
  container import in the tests that need it), so no second reconcile pass
  is needed.
- **`crates/jails-support/src/apply/`** -- **the only module that writes.**
  `fs::write` appears nowhere else and `tests/architecture/` fails when it
  does, and fails on a direct `apply::` call from anywhere that is not the
  write layer. Four verbs, distinguished by *what the caller believes is
  already there*: `create` (must not exist), `replace` (jails owns this file),
  `put` (the new content already accounts for whatever was there -- every
  splice into a reader-owned file lands here, with the byte-preserving merge
  done *before* the call by the module that owns the format), and
  `publish_tree` for the executor's staged after-images. `put_outside_project`,
  `ensure_directory_outside_project`, `put_in_scratch`, `remove_derived` and
  `ensure_derived_directory` are the named exemptions; the last two refuse a
  path outside `target/` or `build/`. `apply::Tree` is exempt as a type: a
  function taking one cannot reach a published project.
- **`crates/jails-support/src/process.rs`** -- `CommandSpec` and one
  synchronous executor, the one place a tool is resolved on `PATH`. Debug
  prints and then runs. Secrets are never rendered: `secret_env` marks one and
  `ALWAYS_SECRET` is a name-based backstop, because `console.rs` sets
  `PGPASSWORD`. Arguments stay `OsString` end to end.
  `jails_support::process::java_program()` resolves the JVM that runs compiled
  project code from `JAVA_HOME`, never `PATH`: Maven compiles under
  `JAVA_HOME`, so that is the release the `.class` files carry, and a machine
  with two JDKs makes `/usr/bin/java` fail with `UnsupportedClassVersionError`.
- **`crates/jails-support/src/scratch.rs`** -- `ScratchDir` is the only thing
  that creates a scratch directory, and
  `production_scratch_directories_are_exclusively_created` fails on an
  `env::temp_dir()` anywhere in production. Never claim a directory that
  already exists (`create_dir_all` treats "exists" as success); `Drop` removes
  only what `tempfile` returned; success paths call `close()` so a cleanup
  failure is reported. `keep()` hands the directory over for fixtures that
  outlive a test.
- **`crates/jails-support/src/git.rs`** -- probes whether this machine's
  `git merge-file` accepts `--diff-algorithm` by running it on three identical
  throwaway files: `git --version` is a string distributions decorate, and the
  question is which release added one flag. histogram and myers can resolve an
  ambiguous merge differently and the merged bytes go into the managed tree,
  so `JAILS_GIT_DIFF_ALGORITHM` pins it (a name, or empty for git's default);
  the gate pins the empty value and `doctor` reports which one this machine
  landed on. Both merges go through `git::merge_file_argv`, and a board row
  fails on a `--diff-algorithm` literal anywhere else.
- **`crates/jails-support/src/unified.rs`** -- the bounded diff. An LCS table
  is quadratic in *lines* while the guards around it are on *bytes*; 2 MB of
  source is thirty thousand lines whose square is seven gigabytes. It is
  bounded on the product.
- **`crates/jails-codemod/src/marked.rs`** -- the marked block, and only that:
  `# jails:<marker>` ... `# /jails:<marker>`, which is how jails edits a file
  the reader owns and what makes `remove` the exact inverse of `add`. The gate
  counts `file.literals`, not `file.production`: a marker only ever appears
  inside a string literal, so a gate reading blanked source reports zero.
  `Marked::indented` exists because a marker at column zero inside a YAML
  mapping is a parse error. A capability's `application.properties` settings
  are claimed one key at a time, not as a marked block, so `remove` retires
  exactly the keys the capability wrote and never the reader's; the comment
  jails writes above a key goes with it only while it is still byte-identical.
  `compose.yaml` and the shared test properties a durable job writes into are
  one *block* per owner, a different shape.
- **`crates/jails-codemod/src/annotate.rs`** -- the `@Import` splice
  (`splice_import`, `unsplice_import`, `is_spring_boot_test`), text in and text
  out, reading through `blanked()` so the `@SpringBootTest` in
  `TestcontainersConfig`'s own Javadoc example is not mistaken for one on a
  class.
- **`crates/jails-project/src/java.rs`** -- a deliberately small Java reader:
  annotations and what they attach to, a type's supertypes, a constructor's
  parameters. **Not a parser, and must not grow into one.** `blanked()`
  replaces comments and literals with spaces of the same length so a scan
  cannot be fooled by `// @Service` while byte offsets still index the original
  -- which is why `annotations()` slices out of `source`, not the blanked copy.
  `java::types_annotated_with` is the one walk of `src/test/java`.
- **`crates/jails-project/src/classfile.rs`** -- the smallest reader that answers
  "which types does this class name": constant pool only, `CONSTANT_Class`
  plus a descriptor scan of every `CONSTANT_Utf8`. `CONSTANT_Long` and
  `CONSTANT_Double` take two pool slots; a reader advancing by one lands on
  plausible tags and produces a wrong answer rather than an error.
- **`crates/jails-project/src/template.rs` + `templates/**.java`** -- the Java
  bodies, as Java files. **Templates are real `.java` files, never Rust
  `format!` strings**: Java is made of braces and `format!` owns that syntax.
  Placeholders are `{{name}}`; a missing or unused key is a panic. Substitution
  only, never a template engine: anything structural stays in Rust and is
  passed in rendered. A template under ~15 lines stays inline. `{{` *does*
  appear in generated `.http` files as the HTTP Client's own variable syntax;
  those are built with `format!` and escaped `{{{{`, so check `.java` alone if
  the renderer is ever revisited. Every template is written against `deps/`,
  not from memory: generated code targets APIs that move, and the failure is
  silent because it compiles against the version you had.
- **`crates/jails-spec/src/build.rs`** -- which build tool a directory uses,
  and nothing more. **Not the same question as `jails_model::BuildSystem`**,
  which is a module's `build` axis: `Build` answers what a directory looks
  like from outside, and its `Foreign(name)` -- a build file jails recognises
  by name and refuses to read -- is the half a two-member set cannot carry.
  The door is any recognised marker, nearest wins, and the
  Maven-inherent commands refuse themselves through `require_maven` with a
  refusal that can say what still works. jails never reads, writes, parses or
  invokes a foreign build file; recognising a filename is not understanding a
  build. Because the emitted Java is shaped by what the pom says, a missing
  pom silently changes the shape (plain JDBC instead of `JdbcClient`, no
  `package-info.java`), so the report says which shape it chose. `add`
  is not exempted: a capability that installs the code and skips the
  dependency is worse than one that refuses.
- **`crates/jails-project/src/config.rs`** -- `jails.toml`. Hand-parsed, one
  `[layout]` table of `key = "value"` pairs, and the keys are a **closed set**
  matching the eleven layers: an unknown one is an error, because a file
  saying `adapter = "persistence"` that silently kept writing to `adapters`
  would be worse than no file. `jails_model::layout::Layer` is the one owner of
  the layer list and `config::LAYERS_IN_ORDER` adds each layer's report
  heading to it; **anything reporting per layer goes through
  `Config::layers()`**, which applies the renames; layer matching is on whole
  path segments in sequence, so `webshop` is not `web` and a nested
  `adapters = "infra.jdbc"` still matches. `[project] capabilities` is what
  `jails sync` applies; it is maintained by `add` and `remove`, never by hand,
  and the names stored are `CapabilityKind::label()`, never clap aliases. Writing
  back is a one-line splice that leaves comments byte for byte alone.
- **`jails adopt`** (`src/adopt.rs`) -- a closed synonym table mapping
  directory names onto the layers, written as `[layout]` rows. An
  unrecognised directory is reported, not guessed; two candidates for one layer
  writes neither; it never touches `[project] capabilities`. `core` is
  deliberately not a synonym for `domain`.
- **`crates/jails-project/src/documents/pom.rs`** -- **the one reader of
  `pom.xml`, and the one thing that splices into it.** Capture asks it what
  the build declares, the dependency and build-feature adapters beside it ask
  where a block goes, and `new` and `modernize` ask it for the two edits they
  make before a model exists; `doctor`, `why` and `run` ask the resolved
  `Project`, whose facts came from the same reader. The board row *production
  files parsing Maven XML with their own scanner* is what keeps it the only
  one. `plugin_nest` is written once and read twice -- inserted, and compared
  against what is on disk to tell a reader's edit from jails' own writing. The
  three release pins (`TARGET_RELEASE`, `MIN_RELEASE`, `TARGET_BOOT`) are not
  read off anything and live in `jails_spec::release`. A build plugin is
  claimed by what it *does*, not by its coordinate --
  `BuildFeature::{IntegrationTests, Coverage, Formatting}` -- because
  `jacoco-maven-plugin` is not a name Gradle resolves; `gradle.rs`'s matches are
  exhaustive over the enum, so adding a feature is a compile error until the
  Gradle side exists.
- **`crates/jails-project/src/compose.rs`** -- `compose.yaml`: marked service
  blocks so `add db` and `add kafka` stack and `remove` takes one out; `start`
  and `stop`; the auto-start `run` shells out to.
- **`crates/jails-project/src/inspect.rs`** -- `routes`, `beans`, `stats`,
  `notes`. Reads source, never a running context: instant, and works on a
  project that does not start. Anything decided at runtime is stated rather
  than hidden.
- **`crates/jails-drive/src/run.rs`** -- `test`, `build`, `clean`, `check`,
  `run`, `watch`. `jails check` is `mvn clean verify`: incremental `verify`
  leaves deleted tests in `target/` and Surefire runs the leftover `.class`.
  `mvn spring-boot:run` exits 0 over a failed startup because devtools catches
  the exception on `restartedMain`, so `run::run_watched` pipes the output,
  scans it for `why::FATAL_MARKERS`, and passes `-Dstyle.color=always` and
  `spring.output.ansi.enabled=always` to keep colour. `jails run` resolves the
  POM's `<mainClass>` because a project with two dispatchers has two `main`s.
  `JAILS_MAVEN` names the Maven command; `maven::mvnd_can_start` probes mvnd
  up front because a read-only home kills it with an exit status
  indistinguishable from a failing build.
- **`crates/jails-drive/src/launcher.rs`** -- `jails test --fast`: JUnit's
  console launcher over already-compiled classes. The console artifact's
  version must equal the project's JUnit version (`junit-bom` constrains every
  artifact to one number from JUnit 6). `staleness()` must never read "no
  class files" as "nothing is stale". `--fast` is the no-mvnd path and the
  substrate for `testd`; do not describe it as faster than the default.
- **`crates/jails-drive/src/testing.rs` is the one test-execution
  vocabulary**: one `TestPlan`, one `TestReport`, and the selector, scope,
  engine, partition, case and outcome they are made of. It carries no
  encoding, because none of it leaves the process; the machine-readable
  spellings (`--output json`, the daemon's frames) come from its `name`
  methods, and `compile_owner` is derived from the engine rather than stored
  beside it. `run/filter.rs`, `launcher.rs` and `reports.rs` all split
  `Class#method` through `testing::split_selector`.
- **`crates/jails-drive/src/testd.rs` + `templates/testd/JailsTestDaemon.java`**
  -- a resident JVM over a unix socket. `testd/protocol.rs` is the frames --
  a four-byte big-endian length then `serde_json` -- and `testd/client.rs` the
  process lifecycle and the one place a daemon observation becomes a
  `TestReport`. **The daemon reports only what it observed** (which cases ran,
  their outcomes and durations, JUnit's output); which engine ran them, the
  scope and the selection are the coordinator's, so the Java side never
  restates a Rust field list. Both halves change together, and the protocol
  number in `protocol.rs` is what makes a client from another release restart
  a daemon rather than talk past it. **The classpath is split in two and
  must stay that way**: the daemon holds the dependencies and hands only
  `target/classes` and `target/test-classes` to JUnit as `--class-path`, so
  JUnit builds a child loader per run; put the outputs on the daemon's classpath
  and parent-first delegation serves the stale class forever. **It does not
  compile**: the editor's language server writes `target/classes` on save with
  the whole project's model, and compiling only the changed file is unsound.
  It is a Java program compiled by `java`'s single-file source launcher, not a
  jails jar. A run that produces no cases refuses rather than completes, with
  the head of JUnit's output and every `Caused by:` line in the refusal frame.
- **`crates/jails-drive/src/affected.rs`** -- `testd --affected`: a reverse
  dependency index over the constant pools in `target/`. **Unknown widens**: no
  git, a source with no compiled class, nothing compiled -- each returns
  `Everything` with the reason printed. "Changed" is what git reports, not a
  marker jails writes, because a marker makes the same command select
  differently on two consecutive runs with no edit between.
- **`crates/jails-drive/src/migrate.rs`** -- `jails migrate --check`: applies
  every migration to a scratch database and reports the first failure with
  psql's file and line. Ordering is numeric, not lexical. Not a `doctor` check,
  because doctor is read-only and this writes.
- **`crates/jails-drive/src/kafka.rs`** -- runs the image's own CLI tools
  inside the compose container. `BROKER` is `kafka:19092`, the inter-broker
  listener; `localhost:9092` works from inside the container only by accident.
  `topics_in()` locates a `TOPIC` constant with `blanked()` and reads the value
  from the original source.
- **`crates/jails-drive/src/console.rs`** -- `db`/`dbconsole` (`psql` or
  `sqlite3`) and `console` (`jshell` over the Maven classpath). Interactive;
  inherit stdio.
- **`crates/jails-drive/src/bench.rs`** -- runs the k6 script `add loadtest`
  wrote. It does not parse k6's output; k6's own thresholds decide.
- **`crates/jails-report/src/doctor/`** -- split by *who is being asked*:
  `environment.rs` asks the machine, `wiring.rs` asks the project whether a
  capability is wired up, and `doctor.rs` keeps the report and `--json`.
  Read-only by contract; `jails setup` is a different command and writes
  `~/.testcontainers.properties` through `apply::put_outside_project`. Every
  `FAIL` carries a `fix:` line, and a failure exits non-zero via an *empty*
  `Err` so `main` prints no redundant line. Drift in a modelled project is the
  model's question, answered by `jails sync`.
- **`crates/jails-report/src/why.rs`** -- a table of (signature, explanation,
  fix) rules matched against a log; rules sharing a `group` describe one
  failure and only the most specific is reported. Add rules only from failures
  that happened; a guessed cause costs more than no cause. The way to find
  them is to mine real logs for `Caused by:` lines, deduplicated and counted.
- **`crates/jails-report/src/explain.rs`** -- `jails explain <kind>`: a
  hand-written table, one entry per kind, with `every_kind_has_an_explanation`
  failing the build when a kind is added without one.
- **`crates/jails-report/src/commands.rs`** -- `jails commands [--json]`:
  every subcommand, kind, capability and flag, walked out of the same
  `clap::Command` that parses the arguments, to every depth. There is no
  second list. It is the oracle for
  `every_command_a_message_tells_the_reader_to_run_is_one_that_exists`, which
  scans every backticked `jails ...` in a production message.
- **`crates/jails-report/src/source.rs`** -- `jails src <Type>`: where a type
  is. Deliberately requires no build file, lists every match rather than
  picking, and reads the package off the `package` line rather than the path.
- **`examples/`** -- the proof applications: manifests built from the same
  generic intents, with `ACCEPTANCE.md` as the done/not-done contract and
  `proof-policy.tsv` naming the gate for each. **Never hand-edit a generated
  proof app to make it pass**; a manual edit is evidence for the next generic
  improvement.
- **`jails.nvim/`** -- Lua, not Rust: a thin `:Jails` wrapper that shells out
  to the binary and reads `jails commands --json` once per session for
  completion. It keeps no tables of its own; `tests/editor.rs` asserts that.
  Every failure path degrades to an empty menu, because a completer runs on
  every keystroke.
- **`deps/`** holds ~80 gitignored upstream checkouts (read-only research;
  `deps.tsv` is the manifest and `deps-update.sh` clones and fast-forwards
  them, blobless). `ideas/` holds reference projects. Never edit or document
  either as if it were jails'. `.gitignore` ignores `deps/` and only `deps/`;
  a checkout anywhere else is filed as a gitlink by `git add -A`.

## Workflow (every change, no exceptions)

```
cargo test --workspace                                  # the inner loop
mise run verify-rewrite && cargo install --path .       # before pushing
```

**There is one answer to "is this green", and it is `mise run
verify-rewrite`.** `.githooks/pre-push` and `.github/workflows/verify-rewrite.yml`
invoke it and nothing else, so hook, CI and this file cannot disagree. It runs
`cargo fmt --check`, then `cargo clippy --workspace --all-targets -D warnings`,
`RUSTDOCFLAGS='-D warnings' cargo doc --workspace --no-deps` (the comments
carry rustdoc links, and a link to a moved item is a warning nothing reads
otherwise) and the test build at once, and then `cargo test --workspace`
under `JAILS_TOOLCHAIN=1` and `JAILS_GIT_DIFF_ALGORITHM=`. `mise run lint` is its fast half and is what
`.githooks/pre-commit` runs; `git config core.hooksPath .githooks` wires both.
A Stop hook in `.claude/settings.json` runs the gate at turn end; run it
yourself before pushing anyway.

**`--workspace` is not optional**: `cargo test` at the workspace root tests
the root package only and says nothing about the rest.

**The gate runs inside a cgroup, and so must anything as heavy.**
`scripts/bounded.sh` puts a command in a transient user scope -- half the
memory, three quarters of the cores, no swap, `MemoryHigh` one JVM below the
cap so the kernel throttles before it kills -- and both `mise` tasks go
through it. The harness reads the scope back: `cpu.max` sizes the spawn
budget and the JVM permits, `memory.max` bounds the permits at 700 MB a JVM,
and the toolchain pool waits for a JVM's worth of headroom before taking a
slot. One unbounded run of the real-toolchain tier beside a few `cargo`
builds took a 30 GB machine into swap and down; never run the tier bare, and
run one heavy thing at a time (`JAILS_GATE_MEMORY_MB` and `JAILS_GATE_CPU`
shrink a second one). The compile phase is `scripts/gate-build.sh`: `fmt
--check`, then clippy, rustdoc and the test build concurrently in three
target directories (`target/lint`, `target/doc-check`, `target`), because
they share no artifacts and one `target/` serialises them. The test phase is
`scripts/gate-test.sh`: every test executable at once, plus the doctests,
each to its own log. Measured on 16 cores after this: the whole gate in
53 s warm and about 150 s cold, memory pressure under 1 %.

**`JAILS_TOOLCHAIN` is the one switch between the two commands.** Plain
`cargo test --workspace` is Rust only -- no JVM, no container, no build tool.
`JAILS_TOOLCHAIN=1` switches the real-toolchain tier on and turns anything it
cannot run into a failure naming what was missing; without it a tier-3 test
that cannot find `mvn`, a new enough `javac`, Gradle or a container runtime
does not run at all and counts as a pass. `real_mvn_available` and its
siblings answer `false` when the tier is off, so a probe never decides whether
the tier runs (`tests/common/toolchain.rs`).
`JAILS_TEST_MAX_TOOLCHAIN_PROCESSES` bounds the JVM count.

**Tier 3 needs three things on the machine**, and each missing one fails in a
way that looks like a product bug:

| missing | what it looks like |
|---|---|
| a JDK matching `TARGET_RELEASE` | `release version 26 not supported`, ~50 tests red |
| a running container engine | Testcontainers and the OCI image gate skip |
| `git` on PATH | `git merge-file` is the three-way merge |

Note `real_path_without_mvnd()` rebuilds PATH for the real-mvn tests, so which
JDK Maven uses is decided by `JAVA_HOME`, not by the `javac` the gate probed.

**A Claude Code on the web session provisions itself** through
`.claude/hooks/session-start.sh`: mise and the toolchain `mise.toml` pins, JDK
21 beside it for the pinned-Gradle example, a container engine, and the
sandbox's interception CAs. Three traps it exists for:

- **The CA half.** The sandbox intercepts TLS with six CAs; `ca-bundle.crt`
  carries all of them and `agent-proxy-ca.crt` two. Which CA signs a
  connection varies, so trusting two passes a hand-run `mvn` and fails inside
  a parallel suite. The hook imports every certificate in the bundle the JDK
  does not already trust, diffed by fingerprint. `java-truststore.p12` omits
  the interception CA and `-Djavax.net.ssl.trustStore` *replaces* the JDK's
  store, so pointing at it is worse than nothing. Inside a container it is a
  different CA again, and `# syntax=docker/dockerfile:1` resolves every `FROM`
  against the registry, so the hook publishes a trusted base image keyed on
  the bundle's hash and `JAILS_OCI_BASE_IMAGES` substitutes it.
- **A mise shim resolves the version from the current directory, and the
  tests do not run in this one.** A tier-3 test runs its toolchain in a scratch
  directory with no `mise.toml`, so `mvnd` errors, `mvn` silently uses the
  system Maven, and `java` is the wrong JDK -- which surfaces as `testd` tests
  failing with an empty report. `mise use -g java@<pinned> maven@<pinned>
  mvnd@<pinned>` is the repair; `mise ls` in `/tmp` shows the problem.
- **The proxy port rotates and `~/.m2/settings.xml` pins it.** A resumed
  session sends every Maven request to a dead socket, Maven caches each
  failure as a `.lastUpdated` marker, and it presents as ~25 product-shaped
  failures at `maven-resources-plugin ... Connection refused`. Repoint the
  file and `find ~/.m2/repository -name '*.lastUpdated' -delete`. Suspect this
  whenever the whole tier fails at once and the unit tiers are green.
- A dead `dockerd` leaves a pid file that blocks its own restart.

## How the suite stays fast

The rules, each measured rather than guessed:

- **A table-driven test is parallel over its table.** Libtest parallelises per
  `#[test]`, the wrong grain for one function driving sixty scenarios; tables
  go through `tests/common/parallel.rs`, a work-stealing scheduler over one
  process-wide permit gate. Write the cell as a function returning its
  findings, not a loop body pushing into a captured `Vec`, so the report stays
  in table order and `parallel::catching` keeps the cell's own assertion.
- **Scheduling is longest-first from what the last run observed.** Each run
  writes what it saw to `target/jails-test-costs/`; an unmeasured cell is
  scheduled first. It is a hint only: a missing or corrupt ledger changes the
  order and no result.
- **A scan of the workspace happens once.** `measure::sources()` in
  `tests/architecture/` is memoised behind a `OnceLock`; `genericity.rs` does
  the same; a third scanner goes through `parallel::map_by_cost`.
- **`[profile.dev] opt-level = 1`** with `debug = "line-tables-only"`, because
  the integration tests spawn `target/debug/jails` thousands of times. An
  incremental rebuild after an ordinary edit is unchanged.
- **`parallel::budget()` is four units per core** for the cheap `jails`
  spawns, which are `fork`/`exec`-bound. That is a different budget from
  `default_max_toolchain_processes`, which governs whole JVMs and is far
  smaller. Leave libtest's `--test-threads` at its default.
- **The Maven budget is a `flock`** under `target/`, shared however the suite
  is launched, because a second shell running `cargo test` while the first
  still is doubles the JVM count and an in-process `Mutex` cannot see it.
- **`cached_toolchain_dir_with_salt`** (`tests/common/mod.rs`) shares one
  persistent fixture per label under `target/jails-e2e-cache` and takes no
  lock, so **run one gate at a time**; if two runs overlapped,
  `rm -rf target/jails-e2e-cache` before believing the next result, because a
  half-built toolbox is stamped ready and reused, and the failures read like
  real `capabilities::` regressions.
- **A test that waits is worse than a test that works.** When a test is slow,
  ask what it is waiting for: a fake `docker` that starts no container leaves
  a readiness probe failing for its whole budget, and
  `common::listening_loopback_port()` is the fixture that stops lying. The way
  to find the next one is the occupancy timeline from `JAILS_TEST_PROFILE=1`
  (per-subprocess lines go to stderr under `-- --nocapture`): a slow test alone
  in the tail costs all of itself.
- **The critical path is `tests/cli`.** The other binaries finish inside it,
  so only `cli` has a budget. `scripts/subprocess-summary.sh` prints what the
  toolchain subprocesses cost on every gate run; read mean concurrency against
  the core count. On four cores the suite is packed, so four seconds of
  subprocess work removed buys one second of wall, and ordering, thread counts
  and permit budgets are worth nothing.
- **A proof is memoised on the bytes it proves.** `tests/support/mvn.rs` is
  `mvn` with a cache in front, built as the cargo example named `mvn`; the
  harness runs it in place of Maven and hands it to the product through
  `JAILS_MAVEN`. It keys the project tree (minus `target/` and `.jails/`
  outside `generated`), the argv with the directory and loopback ports
  blanked, the JVM's environment and the toolchain's identity, and replays a
  recorded green run whole: status, output, `target/` and every file the run
  changed. Only green runs are recorded, so a hit cannot pass a proof that
  would fail; a changed byte is a miss and runs Maven. Warm, the 47 Maven runs
  of a full suite cost 8 s. `target/jails-proof-cache/misses.log` names what
  missed, an entry's `key.txt` says why, and `JAILS_PROOF_CACHE_OFF=1` runs
  everything for real. `docs/40-gates-and-ci.md` has the whole contract.
- **A chain of boots in one test is five tests.** The runner lifecycle test
  started five Spring contexts in turn (57 s); they are five tests over one
  compiled fixture, each booting a copy (`copy_dir_all`, which copies
  `target/` last so the classes stay newer than the sources): 15 s.
- **Where the time goes is a JVM booting a Spring context**, not Maven's
  startup, not containers, not the product binary (median invocation ~70 ms)
  -- *unless the product spawns one*. The plain toolbox spent 162 of its
  165 s in `mvn spotless:apply`, one per mutation, because the `format`
  capability's follow-up effect ran after every row of a manifest replay;
  it runs once per replay now and never after an execution that wrote
  nothing, and the toolbox is one manifest (`tests/fixtures/plain-toolbox.app.toml`).
  The way to find the next one is `JAILS_TEST_PROFILE=1` with the per-fixture
  timeline: a `jails` invocation measured in seconds is a JVM it started.
  `docs/40-gates-and-ci.md` records the levers already measured and declined
  -- mvnd under concurrency, `-DforkCount=0`, class-data sharing, lazy
  initialization, JUnit class-level parallelism, batching Maven runs -- so they
  are not proposed again.
- **CI is the same suite on a four-core runner** and measurements here
  transfer to it. The cargo cache key carries the compiler, the lockfile and a
  source hash; the JVM repositories are cached under a static key written by a
  green run; superseded incremental sessions and `deps` artifacts are trimmed
  before the save. `docs/40-gates-and-ci.md` prices what remains.

## Package layout

Generated code does not all land in the base package. Each kind renders into
the subpackage its layer owns; the eleven layers are `domain`, `app`,
`service`, `web`, `api`, `messaging`, `cli`, `clients`, `jobs`, `adapters`,
`testkit`, listed once as `jails_model::layout::Layer` -- the crate that owns
every closed vocabulary. A `jails.toml` `[layout]` rename is applied through
`Config::layers()` and reaches the model as `Layout`; six emitted packages
sit under heads the convention does not close and are reported as
`convention.facet.*` by `jails model explain` rather than moved. `--package`
overrides placement for the kinds that accept it.

## Import order is normalised at write time, not in templates

**`emit_java::JavaUnit` is the one Java shell**: a package, a sorted
`BTreeSet` of imported names and the declarations, and `JavaUnit::render` is
the only function in the compiler that writes a `package` line, an import
block or the provenance header. Static imports first, a blank line, then the
rest sorted, which is what palantir-java-format produces, so `add format`
leaves a project that passes `jails check`. An import an emitter needs is a
name added to the set (`import_from` skips one already in this unit's own
package), never a rendered `import` statement substituted into a template
placeholder; `JavaUnit::from_source` lifts a template's own `package` and
import lines into the same value, so the template stays a real `.java` file
and one block is rendered. A capability's Java goes out through
`JavaUnit::source`, which is the same shell without the provenance header.
`emit::tidy_java` then collapses doubled blank lines, and nothing else.
Do not hand-order imports in templates. Formatter *wrapping*
cannot be predicted from a template, so `add format` runs `spotless:apply`
once, best-effort. `package-info.java` is written by `emit::package_infos`
for every emitted package, only when `org.jspecify:jspecify` is a
dependency.

## Table constraints are model nodes, and a closed set

`@pk`, `@unique`, `@index`, `@positive`, `@nonnegative` parse off the compact
field syntax into the entity's `EntityConstraint`s and `Index`es
(`jails_model::constraint`, `jails_model::Index`), and `emit_sql` reads them
into the DDL. **An unknown marker is an error, not a no-op.** **No arbitrary
SQL**: `@check(...)` would be a string jails passes through and cannot
validate. `--index` (repeatable, on `g scaffold`) carries what a per-column
marker cannot, and `created_at desc` is a column plus an ordering. `@scope`
is the exception and touches no SQL: it marks a request-boundary field
proved against a same-named JWT claim, and the compiler refuses a scoped
operation when `add security` has not declared a `ScopeAuthorizer` --
tenancy without the word "tenant" existing in core.

## Field syntax: case is the rule

`crates/jails-model/src/field_syntax.rs` reads `name:type[!?]`. **Lowercase
= jails' table, capitalised = a type the project owns**, passed through
verbatim with no import. `normalize_type` canonicalises the CLI's aliases
(`String`, `text`, `bool`) on the way in, and `jdl 1` refuses a bare alias by
name through `BuiltinType::from_alias`; `Currency` is deliberately not a
builtin, because an enum of the currencies a project deals in is an ordinary
thing to generate. **It lives beside the alias table it answers to**, in the
crate that owns every closed vocabulary, because a second parser of this
syntax is the repository's most reliable drift generator. Its refusals are
`String`s and the binary wraps them at the call site, which is how a `?`
through `jails_support::Failure` keeps the message byte for byte.

The suffix sets optionality: `!` non-blank, `?` nullable, bare non-null.
`?` emits an `Optional<T>` component, and the compact constructor normalises
a null one with `requireNonNullElse(x, Optional.empty())`
(`emit_java::record_validation`) -- a deliberate departure from "Optional as a
return type only", because a record component is both at once.
`BuiltinSemantics` in `jails_model::builtin` is the one row per type that
knows its Java type, SQL type and sample value; when a sample is impossible
the companion test is emitted whole and `@Disabled`, naming the component.

## Gotchas

- **Which directory a command is about is one walk.** `model_command::project_root`
  walks up to the nearest build file or model marker, nearest wins, from the
  process directory; `jails g record` typed in `src/main/java` reaches the
  project root. Only the *read* is anchored; every model path stays
  project-relative.
- **The scenario table is the one place a new kind gets registered.**
  `tests/common/scenarios.rs` holds `SCENARIOS`; `tests/golden.rs` snapshots
  the bytes, `tests/agreement.rs` checks `generate` and `destroy` agree, and
  `every_kind_and_capability_has_a_golden_scenario` reads the kinds and
  capabilities out of `--help` and fails when one has no scenario. `format` is
  the documented exemption in `COVERED_ELSEWHERE`. A file `destroy`
  deliberately keeps goes in `ALLOWED_LEFTOVER` with its reason.
- **Generated projects target Java 26** (`release::TARGET_RELEASE`); adopted
  projects keep their release, with 21 as the floor. Tier-3 tests gate on
  `real_java_supports_target_release()`, not on a JDK being present.
- **`base_package()` falls back to the shallowest `.java` file**, because
  `new-cli` projects have `App.java`, not `*Application.java`.
- **`add json` is Jackson 3 (`tools.jackson`), one artifact.** java.time is
  built in; adding `jackson-datatype-jsr310` drags in the 2.x line beside 3.x,
  nothing warns, and half the code lands on a mapper nobody configured.
  `doctor` reports two Jackson majors as a FAIL. `JsonMapper.builder().build()`,
  `JacksonException extends RuntimeException`, `WRITE_DATES_AS_TIMESTAMPS`
  under `cfg.DateTimeFeature`. **The annotations are the exception and stayed
  on the 2.x coordinates**: `emit_enum` importing
  `com.fasterxml.jackson.annotation.JsonValue` into a Jackson 3 project reads
  like the bug this bullet warns about and is not one --
  `deps/jackson-databind/pom.xml` says so in a comment beside its own
  `jackson-annotations` dependency. Check there before "fixing" it.
- **Boot 4 split the servlet test slice.** `@WebMvcTest` and
  `@AutoConfigureMockMvc` live in `spring-boot-webmvc-test`, so a generated
  test using either needs `spring-boot-starter-webmvc-test`, which the
  compiler declares as a `BuildDependency` from the units it emitted; the
  captured Boot version (`snapshot.project.spring_boot`) picks the package.
  **The test fixture must not supply what the tool is supposed to supply**:
  `SPRING_FIXTURE_POM` declares only what `jails new` writes.
- **A type whose package a Boot major moved is an `Import::Moved` row, not a
  template placeholder.** `recipe::MovedImport` holds both spellings
  side by side with the rule that an unreadable version resolves to the older
  package; `AUTOCONFIGURE_MOCKMVC`, `WEBMVC_TEST` and
  `METER_REGISTRY_CUSTOMIZER` are the three (in `emit_capability`), and a
  recipe's `JavaFile` row asks for one so the name joins the unit's import
  set. Boot 4 moved
  `@AutoConfigureMockMvc` and `@WebMvcTest` to
  `org.springframework.boot.webmvc.test.autoconfigure` with no shim, and moved
  `MeterRegistryCustomizer` out of `actuate.autoconfigure`. The validation
  package (`jakarta` vs `javax`) is the fourth version fact and stays a
  function, because it is a package *prefix* rather than a type.
- **A recipe's own facts are on its row.** `Recipe::substitutions` carries
  what only that capability's templates spell -- an image tag, a URL, a route
  segment -- because one shared substitution list applies `redis:7-alpine` to
  `mail`'s templates and is safe only while no two packs pick the same key.
  What a *node* spells -- a component's route, a label as a table or a
  property prefix, the event a command relays -- is a `Key` on the row,
  rendered once by the node's `Node::key`, and a template's own package is
  always `{{pkg}}`, so placement is on the row and never in the template.
- **`Map<long, Note>` is not a type.** `int` and `long` are the only builtins
  with a Java primitive, and a required one is spelled with it everywhere
  except a *type argument*, where it has to be boxed;
  `emit_java::boxed_java_type` is that spelling and
  `emit_java/repository.rs` is the only place in the compiler that needs it.
  Goldens cannot catch this -- they compare bytes -- so the tier-3 kind sweep
  keys a `g repo` on `long` beside the `uuid` one.
- **The Boot floor is in the generated *code*, not its tests.** `add api`
  writes `ProblemDetail`, `add security` writes `requestMatchers`, `g query`
  and `g transition` write a `JdbcClient` adapter, all in the main source set;
  `refuse::preflight` refuses below the floor, naming the type the compiler
  would have emitted.
- **Exactly one repository adapter carries `@Repository`.**
  `emit::jdbc_on_classpath` decides: with `spring-boot-starter-jdbc`
  present the `JdbcClient` adapter is the bean and the in-memory one is an
  unannotated fake; without it the adapter is plain `Connection` JDBC and the
  in-memory one is the bean. `JdbcClient` lives in `spring-jdbc`, so without
  the starter the type does not exist. Two beans is the ambiguity `jails beans`
  reports.
- **`add db`'s test wiring is an imported `@TestConfiguration`, and both
  halves are load-bearing.** The container is a `@Bean` with
  `@ServiceConnection` in `TestcontainersConfig` (not a `@Container` static
  field: Spring caches the context past the container's lifetime), and it is
  `@Import`ed rather than registered globally (an initializer in
  `spring.factories` makes every slice test start a PostgreSQL it never
  queries). Once the JDBC starter is present auto-config demands a DataSource
  for every `@SpringBootTest`, so `add db` splices
  `@Import(TestcontainersConfig.class)` into the ones already on disk. A
  leftover `spring.factories` must be deleted. `spring-boot-testcontainers` is
  required: it registers the lifecycle initializer. Docker Compose is skipped
  in tests, so without this `mvn verify` dies on "Failed to determine a
  suitable driver class"; do not fix that with `skip.in-tests=false` or a test
  `application.properties` that shadows the main one. `add db` also disables
  `spring.persistence.exceptiontranslation.enabled`, which CGLIB-proxies every
  `@Repository` and fails on `final` classes, and writes `spring.datasource.*`
  read back out of `compose.yaml`.
- **`spring-boot-docker-compose` cannot drive podman-compose**, so every such
  project needs `spring.docker.compose.enabled=false`; jails starts the
  services itself. **`docker` here may be podman's shim**: `doctor` probes
  with bare `docker info` and `docker ps --format '{{.Names}}'`, which behave
  identically on both; **Testcontainers and the `docker` CLI look at different
  sockets**, so `jails start` succeeding proves nothing about
  `@SpringBootTest`; `doctor` checks it and `why` explains it.
  **Testcontainers 2.0 renamed every module**, so `doctor` matches on the
  `org.testcontainers` groupId alone.
- **A capability's plan is a pure function of the project, so order matters
  and `sync` is the repair.** `add api` renders a `DuplicateKeyException` arm
  only when the JDBC starter is present. `jails add db api` re-resolves the
  project between the two; `add api` then `add db` is repaired by `jails sync`.
  **Two capabilities own `management.endpoints.web.exposure.include`**, so
  `emit_capability::spring` unions the value rather than letting the last
  pack win.
- **`add observability` generates a `MeterRegistryCustomizer`** rather than
  `management.metrics.tags.*`, because it is code the project owns and does
  not depend on which actuator modules are present.
- **`add kafka` cannot know a topic name and must not guess one.** The
  capability owns everything topic-agnostic; `g event` owns what needs a
  payload type. The dead-letter destination is named explicitly in the
  recoverer, because `DeadLetterPublishingRecoverer` defaults to `<topic>-dlt`.
- **`g strategy` is the open counterpart to `g sealed`.** The port is in
  `domain` and the beans in `service`, because the scaffold's ArchUnit rule
  forbids Spring inside `domain..` and the `@Component` is what puts an
  implementation in the injected list. An implementation missing `@Component`
  is simply not in the list, which the generated Javadoc says. `destroy
  strategy` finds implementations by reading `supertypes` in every main-source
  directory, so a hand-written one is not left implementing a deleted
  interface. `--package` is part of an entity's identity, so `destroy` needs
  the same one.
- **`g idempotency` is the retained-result primitive**: a `@unique` column
  gives one row per key; what it withholds is the *result*. Four outcomes
  (run, replay, refuse a reused key, tell an in-flight retry to come back), and
  the claim is one `insert ... on conflict do nothing returning` because
  select-then-insert reopens the race. Domain-blind by construction.
- **A name that already carries its kind's suffix must not get it twice.**
  `strip_redundant_suffix` runs in `generate` and `destroy`; `scaffold` is
  exempt because it spans Controller, Service and Repository at once
  (`jails_spec::spec::suffix`).
- **`record`/`command` are the plain-Java kinds.** `command` registers itself
  in the project's dispatcher, found by *shape* (`is_dispatcher`: the registry
  type plus the `return commands;` anchor), one line spliced idempotently, with
  the Javadoc instructions as the fallback when there is no dispatcher or more
  than one (`DocumentIntent::EnsureCommandRegistration`);
  `DocumentIntent::SetMavenMainClass` retargets `<mainClass>` only off a stub
  jails wrote with no command registered.
- **`naming::plural_snake_case` is the only pluraliser**, and every table
  and resource path derives from it. Irregulars are a short list matched on
  the last word plus a short uncountable list; no `jails.toml` override,
  because derivability is what lets `destroy` find what `generate` wrote.
- **Commons CSV's `Builder.build()` is `Builder.get()` from 1.13**; a unit
  test beside the `csv` pack holds the pinned version and the generated call
  together.
- **Don't use preview features in generated Java.** Anything preview needs
  `--enable-preview` in compile and surefire and breaks on the next JDK.
- **The `@MockBean` family does not exist in Boot 4**; `@MockitoBean` and
  `@MockitoSpyBean` from `org.springframework.test.context.bean.override.mockito`
  are the replacement. `MockMvcTester` is the current MockMvc entry point.
- **`BuiltinSemantics` rewrites exactly one SQL name for H2**, `timestamptz`
  to `timestamp with time zone`, because every other type is in H2's own type
  table verbatim. The dialect is the model's `storage` axis. One column list feeds the DDL, the
  select, the insert, the bind and the row mapper, and the write expression
  bakes in the receiver (`Timestamp.from(x.at())`) because gluing it on the
  front yields `x.Timestamp.from(at())`, which only the real-toolchain tier
  catches.
- **`git merge-file --diff-algorithm` is probed, never assumed**: on git 2.43
  it exits 129, a usage error, and every regeneration over an edited file
  fails. See `jails_support::git` above.
- **Each crate gets its own test binary**, so `#[cfg(test)]` modules within
  one crate share a process and one current directory. Any test that calls
  `set_current_dir` holds `jails_testkit::hold_cwd()` for the duration, taken
  through `hold_cwd()` and never `.lock().unwrap()`, because a poisoned mutex
  reports a `PoisonError` naming neither the panic nor its cause.
- **`#[cfg(test)]` in a library crate means "when *this* crate is under
  test".** A dependent crate's tests cannot see it; a test helper that has to
  cross a crate boundary is ordinary public API or should not exist.
- **The test fixtures are handed to real Maven, so they have to be valid
  poms** with `modelVersion` and `version`.
- **`mvn`'s own launcher shells out to `uname`, `dirname`, `ls` and `expr`.**
  A test that isolates PATH can strip `mvnd` out of it, not reduce it to the
  tool directory. Mocked fake-mvn scripts are a single `#!/bin/sh` line.
- **This machine's `mvnd` is flaky under JDK 26**, and under concurrent
  invocation generally (`StaleAddressException`); `run.rs` still prefers it
  for real use, and the real-compile tests pin plain `mvn` through
  `real_path_without_mvnd()`.
- **`jails run` must not swallow a failed startup**: see `run.rs` above.
- **clap `alias` is invisible to `clap_complete`**; use `visible_alias` for
  anything typed interactively. **Free-form `String` args don't tab-complete**;
  any closed value set is a `clap::ValueEnum`.
- **The crate is on edition `"2024"`**, deliberately. Install target is
  `~/.cargo/bin/jails` via `cargo install --path .`; bash completion is
  registered in `~/code/my-dotfiles/home/.bashrc.d/60-completions.sh`, a
  separate repository.
- **Tests never call start.spring.io**; `write_spring_fixture` in
  `tests/common/mod.rs` is a hand-written, version-pinned fixture.

## Generated code tracks Spring Boot 4 / Framework 7, verified against source

The upstream checkouts under `deps/` are the reference, not memory. Three
things relied on by the templates and confirmed there: `@MockBean` is gone;
`MockMvcTester` (`org.springframework.test.web.servlet.assertj`) is the
MockMvc entry point and `@AutoConfigureMockMvc` contributes one whenever
AssertJ is on the classpath; Testcontainers containers are Spring beans with
`@ServiceConnection`, never `@Container` static fields.

## `scaffold` produces a running resource, and that constrains it

The scaffold's controller, service and DTOs are real, so the generated
application has to *start*: an in-memory adapter is generated and carries
`@Repository` while the JDBC one does not, or the other way round when the
JDBC starter is absent (`emit::jdbc_on_classpath`).

## Anything that emits an `*IT` also declares the build feature

Surefire runs `*Test`; `*IT` is Failsafe's, and Failsafe is not in the Spring
Boot parent's default build, so a `verify` completes and executes none of
them. An integration-test unit lowers to `BuildFeature::IntegrationTests`,
carried to the build file by `DocumentIntent::ReconcileBuildFeatures`: Maven
gets Failsafe with both goals bound, Gradle gets separate tasks wired into
`check`. Removing the last such unit removes the marked block.

## A generator that emits code declares the dependency it needs

And the version it needs depends on the project's flavour: a `<dependency>`
with no `<version>` is correct under `spring-boot-starter-parent` and fatal
without one, so every dependency is a `BuildDependency` reconciled through
`DocumentIntent::ReconcileDependencies` with the version decided once from
the captured flavour. `g dto` declares `spring-boot-starter-validation`;
`g client` declares `spring-boot-starter-restclient`, without which the
proxies build, the project starts, and the first call fails with `URI with
undefined scheme`.

## Testing philosophy

Three tiers, don't blur them:

1. **Unit tests** (colocated `#[cfg(test)] mod tests`) -- pure functions and
   filesystem-only logic, no Maven, no subprocess.
2. **Mocked-mvn integration tests** -- a fake `mvn`/`mvnd` script that logs
   argv, for verifying command construction.
3. **Real-toolchain integration tests** -- invoke `mvn`/`javac` against a
   fixture project, gated on `JAILS_TOOLCHAIN=1`. This is the only tier that
   answers the question the tool exists for: does it produce a project that
   compiles and passes its tests. Don't let tier 2 masquerade as tier 3.

**A skipped tier-3 test is reported as passing**, which is why every skip goes
through `common::skip()` and `JAILS_TOOLCHAIN=1` turns each into a failure.
`tests/cli/` is one binary with subject submodules (`new`, `generate`,
`capabilities`, `app`, `tooling`, `reports`, `model`, `sql`, `examples`,
`developer_tools`), each reaching the shared fixtures through `use super::*`.
`model` is itself a directory, split the same way: `tests/cli/model/mod.rs`
holds the JDL fixtures and project builders every submodule shares, and one
file per subject sits beside it (`source`, `plan`, `generate`, `operations`,
`merge`, `eject`, `destroy`, `resource`, `capability`, `build`, `database`,
`reports`), so a test is addressed as `model::<subject>::<name>`.
`tests/architecture/` is the structural ladder as ratchets: each row is a
number measured over production Rust (comments, string literals and
`#[cfg(test)]` modules blanked first), failing when it rises above its ceiling
and when it falls below without the ceiling being lowered in the same change.
`cargo test --test architecture -- --nocapture --test-threads=1` prints the
board; raising a ceiling is allowed once per rise with the reason recorded
beside it. Gates name their file by path, not basename.

**The board's largest-module row names whichever module is largest**, so a
split cannot be satisfied by moving a monolith; run the board rather than
trusting a filename. A brace-matching splitter must blank string literals
first, because these tests are full of Java; `tests/architecture/measure.rs`
has the implementation the gates use.
