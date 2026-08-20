# Jails refactor plan

Status: design-only review. This document proposes a refactor; it does not imply that the current CLI, generated output, or project-file formatting should change.

Review snapshot: 2026-08-20. The `src/` and `tests/` trees were reviewed from source and against codebase-memory generation `2026-08-20T12:39:48Z`. The graph reported no recorded parse gaps in the cited files, but that is a best-effort signal. `src/add.rs` changed after indexing, so its cited ranges were read directly and its graph counts are only orientation.

## Executive diagnosis

Jails does not mainly suffer from too little abstraction. It has several good local abstractions, but each stops at a different boundary:

- `ProjectContext` understands Maven workspaces, yet almost every command uses the smaller global-CWD-based `find_project_root()`.
- `add::Plan`, `spring::SpringSlice`, `generate::Artifact`, and `rename::Edit` all describe changes, but each command applies them differently.
- `generate::Field` is simultaneously a parsed CLI field, a rendered Java field, and an input to SQL/JDBC generation.
- `run_inherited` centralizes one subprocess case, while stdin, capture, tee, PATH lookup, debug output, and status handling are reimplemented elsewhere.
- Model/query code often prints directly, manually builds JSON, or signals “already reported” with `Err(String::new())`.

This produces two recurring failure shapes:

1. A concern is fixed in one path but not its sibling. Examples are `debug` execution, `generate` versus `destroy`, and configured layer names versus `stats`.
2. A command validates and writes in alternating steps, so a later failure can leave an internally inconsistent project.

The refactor should therefore establish a few explicit seams before physically splitting the large files:

```text
CLI syntax
   |
typed request + invocation policies
   |
project snapshot -> feature planner -> ChangeSet -> preflight -> apply
                                   \-> preview

subprocess request -> CommandSpec -> one synchronous executor

query -> structured report -> human/JSON renderer
```

Do not start by moving thousands of template lines. First make dependency direction and side effects honest; then the file split becomes mostly mechanical.

## Keep these existing choices

The refactor is not a rewrite. These are useful foundations:

- Keep `Capability`, `ArtifactKind`, `Runtime`, `Optionality`, `Flavor`, and the diagnostic `Status` as closed enums. Exhaustive matching fits this product better than a plugin registry.
- Keep the targeted, comment-preserving POM and Compose edits. `src/pom.rs:1-12` documents why a formatting-destructive XML round trip is unacceptable.
- Keep pure text analysis/rendering where it is already pure, including most of `java.rs`, `pom.rs`, `sql.rs`, and the `collect_*` inspection functions.
- Keep `add::Plan` as evidence that plan-before-apply works. It is the seed of the common mutation boundary, not something to discard.
- Keep real generated-project compilation tests and the descriptive behavior-test names. They have found product failures that isolated unit tests would miss.
- Keep the intentionally small dependency set. Add a crate only for a concrete boundary such as RAII temp directories or stable JSON, not because a “modern” stack is expected to have it.

## P0 behavior contradictions to pin before moving code

These should receive focused regression tests and intentional decisions before a broad refactor. They are not merely style issues.

**Status: all seven are fixed** (2026-08-20), each verified against the source
first rather than taken from this table on trust, and each with a regression
test that fails if it comes back. The first six landed as one reviewable
commit separate from any mechanical move, as this document asks; the hidden-
writes row was fixed later, after an earlier revision of this table wrongly
recorded it as done. What was decided:

| Row | Resolution |
|---|---|
| `--debug` skipped execution | Debug prints and then runs, everywhere. `migrate::psql` and the Kafka stdin path no longer return early. |
| `--pretend` not global in effect | `generate cases`/`migration` and `new-cli` preview for real; `new` **refuses** it, because its project is whatever start.spring.io returns and a preview that downloads the zip is not a preview. Refused, never ignored. |
| Multi-capability partial success | `add::preflight` plans every requested capability before any is applied. Not a transaction, and does not claim to be — it removes the failure jails can see coming. |
| Generate/destroy not inverses | `unsplice_registration` undoes command registration; the round-trip test asserts the dispatcher is byte-identical. `destroy strategy` derives implementations from disk rather than a second path list. |
| `Migrate { check }` ignored | `--check=false` is now an error naming the only mode, rather than a flag that reads as a toggle and is not one. |
| Toolchain policy contradicts itself | `TARGET_RELEASE` is 27; its doc had argued for 25 above it. The doc now states the decision, names the three files that must agree, and records the cost — which turned out to be the row below. |
| A requested write can hide more writes | Both halves done. `package-info.java` was written as a side effect of writing a class, so `--pretend` named two files and the run wrote three; it is planned as an artifact now, so preview and apply consume one list. And `write_new_file` takes an explicit root instead of discovering one from process CWD — which was a live bug, not only an architectural smell: a `new-cli` project's own base package never got a `package-info.java`, because the lookup found the *surrounding* project or none at all. |

Also found while fixing the above, and worth recording beside them: 11 of 104
integration tests were reporting green without running, because the acceptance
tier self-skips on a pre-GA JDK. `JAILS_REQUIRE_TOOLCHAIN=1` makes each a
failure naming what was missing.

Two further items that needed no ADR have also landed:

- **§6, the duplicated layer list.** This document predicted the drift and it
  had already happened: `stats` reported against jails' default package names,
  so a project renaming a layer in `jails.toml` had those files counted as
  "Other", and `cli`/`messaging` were never counted at all. `config` now owns
  the list, its headings and the project's renames; the validation list is
  derived from the same constant.
- **§5, `main` returning `ExitCode`.** `process::exit` skipped destructors on
  the current stack, which works against exactly the staged-file and
  scratch-resource cleanup the later phases depend on. `migrate` already
  creates a scratch database it owns.

**The ADRs are decided** (2026-08-20), so nothing below is blocked on them:

1. **Java target: 27**, recorded on `TARGET_RELEASE` with the three files that
   must agree and the cost of a pre-GA release spelled out.
2. **MSRV: current stable.** jails is installed from source by its author; an
   MSRV lane would gate work nothing consumes.
3. **`.jails/state.toml`: no.** Committed *intent* is worth having and now
   exists (`[project] capabilities`); committed *provenance* is not. A digest
   manifest conflicts on every generate, goes stale on any legitimate edit,
   and adds a second source of truth that can disagree with disk. The
   alternative this document names for a "no" is what shipped: `remove`
   re-renders and names any generated file that no longer matches, so it
   cannot delete hand-finished work silently.
4. **`--pretend`: global, or refused in as many words.** Done.
5. **Windows: supported best-effort**, which the `cfg!(windows)` branches
   already implied. It immediately paid: `run.rs` and `project.rs` disagreed
   about whether mvnd is `mvnd` or `mvnd.cmd`, so `about` named a command
   `test` would not run.
6. **The library façade is an internal testing surface**, kept minimal.

Phase 1 has begun where it was load-bearing rather than as a sweep: the file
writer no longer rediscovers the project. The remaining `find_project_root()`
callers are command entry points, which is where this document says discovery
belongs; what mattered was that a *writer* had one.

The larger phases (the `ChangeSet` rollback journal and `Move`, `CommandSpec`,
`lib.rs`, the typed field model) remain design-only. Their user-visible
guarantees have been retrofitted onto the existing shape instead -- one plan,
previewed and preflighted before any write, no hidden writes, no partial
apply on a validation failure -- which is where the value of that IR actually
lands.

| Behavior | Current evidence | Required contract |
|---|---|---|
| `--debug` sometimes becomes “do not execute” | `run::run_inherited` prints and executes (`src/run.rs:33-45`), but piped `psql` and Kafka stdin paths print and return success (`src/migrate.rs:181-184`, `src/kafka.rs:233-239`). `jails --debug migrate --check` can therefore claim migrations applied without running the SQL. | Debug affects observability only. Preview affects execution. They must be orthogonal. |
| Global `--pretend` is not global in effect | `main` does not pass it to `new` or `new-cli` (`src/main.rs:364-375`). `generate cases` and `generate migration` return before the later preview branch (`src/generate.rs:1038-1047`, `1408-1430`). | Every project-writing command either plans and previews without effects or rejects preview explicitly; never silently ignore it. |
| Multi-capability operations can partially succeed | `main` applies capabilities sequentially with `try_for_each` (`src/main.rs:389-414`). | Plan all requested capabilities together, merge shared edits, preflight once, then apply once. |
| Generate and destroy are not true inverses | Command generation edits a dispatcher after creating files (`src/generate.rs:1432-1439`, `3952-4007`); command destruction deletes two files but does not unsplice registration (`src/generate.rs:1692-1695`). Scaffold destruction also maintains a second static path list that omits conditional fixture/migration artifacts (`src/generate.rs:1503-1529`, `1663-1677`). | Derive creation and removal from one ownership record/plan. Never maintain independent path lists. |
| A requested write can hide more writes | `write_new_file` may create `package-info.java` (`src/generate.rs:849-923`). It discovers the project from process CWD even when `new_cli` is writing a different new root (`src/new.rs:269-276`). | Every intended write must be visible in the `ChangeSet`, rooted in an explicit destination. |
| A CLI option has no semantic value | `Migrate { check }` is declared at `src/main.rs:256-263`, then ignored at dispatch (`src/main.rs:435`). | Remove it while there is one mode, or replace it with a real typed mode. |
| Toolchain policy contradicts itself | `src/pom.rs:21-30` argues for Java 25, `TARGET_RELEASE` is `27` on line 31, `new::initializr_java` caps bootstrap at 26 (`src/new.rs:201-207`), and `mise.toml` pins JDK 26. | Decide default target, minimum generated-code target, Initializr bootstrap ceiling, and test JDK once in a `ToolchainPolicy`. |

Bug fixes should be separate, reviewable commits from mechanical module moves. Characterization tests should preserve intended current output, not encode a known defect as the new contract.

## 1. Establish one explicit project boundary

`ProjectContext::discover_from` already knows the active module, workspace, inherited Java/Spring facts, module list, and workspace Maven command (`src/project.rs:31-66`). It is effectively only used by `about`; `generate::find_project_root`, a nearest-POM lookup tied to `std::env::current_dir`, has 27 direct graph callers (`src/generate.rs:603-613`). Maven selection is also duplicated in `run.rs:21-31` and `project.rs:259-275`.

Make `ProjectContext` the identity boundary for commands operating on an existing project:

```rust
struct ProjectContext {
    workspace_root: PathBuf,
    module_root: PathBuf,
    maven: MavenCommand,
    kind: ProjectKind,
    java_release: Option<JavaRelease>,
    layout: ProjectLayout,
}
```

The exact fields can differ. The important contracts are:

- Capture the invocation directory once at the CLI boundary and call `discover_from(&start_dir)`.
- Preserve both module and workspace roots. POM/source generation normally targets the module; wrapper/tool resolution may come from the workspace. Make that choice visible rather than assuming “root” means both.
- Give commands that create a project an explicit destination. Do not manufacture an optional/fake `ProjectContext` for `new`.
- Load mutable input into a per-command `ProjectSnapshot`: POM text, Compose text, config, and a sorted Java source index. A planner reads the snapshot; it does not reread arbitrary files while planning.
- Pass roots/snapshots to helpers. No renderer, parser, or file writer may call `current_dir()` or rediscover the project.

Acceptance criteria:

- `find_project_root()` has no production callers and is removed after a temporary compatibility phase.
- `CWD_LOCK` (`src/main.rs:45-50`) and unit-test `set_current_dir` blocks disappear.
- Nested Maven modules consistently use the intended module POM and workspace wrapper.
- `new` and `new-cli` can be tested with an explicit destination without changing process CWD.

Avoid turning this into a giant service-locator `AppContext`. Invocation policy, project identity, process execution, and output writers may be passed together at the application boundary, but domain functions should receive only what they use.

## 2. Use one mutation IR: `ChangeSet`

Today the mutation vocabulary is split across `add::Plan` (`src/add.rs:97-131`), `SpringSlice`, `generate::Artifact`, `rename::Edit`, and direct `fs::write` calls. The common abstraction should be an execution IR, not a trait/plugin framework:

```rust
struct ChangeSet {
    changes: Vec<Change>,
    after_commit: Vec<Effect>,
}

enum Change {
    Create {
        path: ProjectPath,
        contents: Vec<u8>,
        ownership: Ownership,
    },
    Update {
        path: ProjectPath,
        expected: FileFingerprint,
        contents: Vec<u8>,
        ownership: Ownership,
    },
    Delete {
        path: ProjectPath,
        expected: FileFingerprint,
        ownership: Ownership,
    },
    Move {
        from: ProjectPath,
        to: ProjectPath,
        expected: FileFingerprint,
        contents: Vec<u8>,
    },
}

enum Effect {
    RunFormatter,
    StartCompose(Vec<ServiceName>),
    StopCompose(Vec<ServiceName>),
    GitInit,
}
```

This is illustrative, not a demand for these exact names. `ProjectPath` is justified only if it proves that planned paths are relative to and contained by the selected project root. `FileFingerprint` can initially include the full before-content; do not use `DefaultHasher` as a persistent digest.

Feature planners may retain private domain plans and lower them into `ChangeSet`. The shared layer owns only project mutations and their application semantics.

The pipeline must be:

1. Build all changes, including secondary edits such as package-info, POM, properties, Compose, Failsafe, and dispatcher registration.
2. Normalize and merge changes. Detect two capabilities proposing incompatible edits to the same path.
3. Preflight every path, collision, expected-before value, ownership rule, and root-containment rule before the first write.
4. Render `--pretend`/`--dry-run` directly from this exact plan.
5. For apply, stage replacement files beside their destinations so rename stays on the same filesystem. Keep an undo journal for already-applied steps.
6. Run external effects only after filesystem changes commit.

Do not claim true multi-file atomicity: ordinary filesystems cannot provide it across an arbitrary project tree. The useful guarantee is “no writes after a validation failure,” plus best-effort rollback and a precise recovery report after an I/O failure.

### Ownership must be explicit

Jails edits files people also edit. The existing audit already found hand-written properties inside a Jails-owned marker block (`audit-2026-08-20-rewards.md:476-494`). A prompt is helpful, but path existence alone is not ownership: `remove` deletes every planned existing file (`src/add.rs:427-432`, `604-609`), and `destroy` does the same (`src/generate.rs:1768-1802`).

Use explicit policies:

- `CreateOnly`: fail if the path exists.
- `ManagedBlock`: edit/remove only the identified block or semantic POM/Compose entry and report unowned additions.
- `GeneratedFile`: remove only if current content matches the recorded generated content; drift requires `--force`.
- `UserFilePatch`: apply only when the before-snapshot still matches, so a concurrent/manual edit is not overwritten.

Strong recommendation: record managed generated files and blocks in a committed `.jails/state.toml`, using relative paths, generator/capability identity, schema version, and a stable content digest. This solves version drift that `legacy_deps` currently handles ad hoc. It is a product decision, however: if persistent state is rejected, generate/remove must at minimum compare current content with the exact expected content and refuse destructive drift. Re-rendering with the current Jails version is not sufficient evidence of what an older version created.

Acceptance criteria:

- Every writer supports preview through the same plan used by apply.
- `add A B` preflights both before applying either.
- No helper that sounds like one file write performs a hidden second write.
- Generate/destroy and add/remove share ownership data instead of duplicating path lists.
- External Docker, Maven, Git, and formatter effects never run before the project change commit.
- Removing a hand-edited generated file/block refuses or clearly requires `--force`; it never deletes silently.

## 3. Separate field meaning from Java, SQL, JDBC, and fixtures

`generate::Field` stores a rendered Java type and imports, `owned` and `collection` booleans, optionality, and SQL-only constraints (`src/generate.rs:64-78`). `sql::column` then peels `Optional<T>` and matches Java spelling strings (`src/sql.rs:72-117`). `sql::Column` mixes DDL facts with Java/JDBC expressions (`src/sql.rs:25-45`). This makes Java rendering the accidental source of truth for every target.

Parse the CLI syntax into a target-independent model:

```rust
struct FieldSpec {
    name: JavaIdent,
    ty: TypeRef,
    presence: Presence,
    validation: Validation,
    schema: SchemaConstraints,
}

enum TypeRef {
    String,
    I32,
    I64,
    F64,
    Boolean,
    Decimal,
    Uuid,
    Instant,
    LocalDate,
    LocalDateTime,
    Duration,
    Uri,
    Path,
    User(JavaTypeName),
    List(Box<TypeRef>),
    Map(Box<TypeRef>, Box<TypeRef>),
}

enum Presence {
    Required,
    Optional,
}

enum Validation {
    None,
    NonBlank,
}
```

`NonBlank` is validation, not a third presence state. SQL constraints remain a separate closed value. Render through explicit projections such as `JavaField::from(&FieldSpec)`, `SchemaColumn::try_from(&FieldSpec)`, `JdbcBinding::try_from(&FieldSpec)`, and `FixtureValue::try_from(&FieldSpec)`.

Also replace the overloaded `fields: Vec<String>` application API with a typed request enum. The same vector currently means record fields, enum constants, sealed variants, a case-file path, or other kind-specific arguments (`src/generate.rs:1025-1401`). Clap syntax can remain compatible while conversion at the boundary produces variants such as `GenerationRequest::Record`, `::Enum`, `::Cases`, and `::Migration`.

Acceptance criteria:

- A field token is parsed and validated once, with an error containing its token/argument position.
- SQL/JDBC/fixture code never reparses a Java type string.
- Invalid combinations are unrepresentable or rejected during request conversion.
- Java/SQL output remains byte-identical except for separately approved product fixes.

## 4. Centralize synchronous process execution

Process construction/execution is spread through `run`, `compose`, `new`, `doctor`, `why`, `kafka`, `migrate`, and `console`. PATH lookup exists independently in `run.rs:8-31`, `compose.rs:481-498`, and `project.rs:259-275`.

Use one concrete executor and a data description:

```rust
enum OutputMode {
    Inherit,
    Capture,
    Tee,
}

struct CommandSpec {
    program: OsString,
    args: Vec<OsString>,
    cwd: Option<PathBuf>,
    env: Vec<(OsString, OsString)>,
    stdin: Option<Vec<u8>>,
    output: OutputMode,
}

enum Diagnostics {
    Normal,
    Debug,
}

enum ApplyMode {
    Apply,
    Preview,
}
```

Required behavior:

- `Diagnostics::Debug` prints the command and then executes it when `ApplyMode::Apply` is active.
- Preview prints the intended command/effect and does not execute it.
- Preserve native argument boundaries with `OsString`/`OsStr`; do not join lossy strings into an ambiguous shell-looking line. Forwarded child arguments should not be forced through UTF-8.
- Centralize wrapper/mvnd/mvn and Docker Compose resolution.
- Handle inherited, captured, tee, and stdin-fed processes consistently, including spawn and non-zero-status errors.
- Debug rendering must redact secrets and should not start printing environment values. `PGPASSWORD` is currently placed in a child environment.
- Bound captured logs where a child can be long-running. Production remains synchronous; adding Tokio to wait on child processes would add complexity without a demonstrated benefit.

Start with a concrete type. A narrow trait is justified later only if a real executor and a fake executor both need to satisfy the same consumer. Do not create a trait per tool or command. Pure planner tests can often assert `CommandSpec` values without any process mock.

Temporary external resources also need owners. A scratch database/container should have an explicit fallible `close()` plus best-effort `Drop` fallback; cleanup that can fail should not be hidden only in `Drop`.

## 5. Separate errors, outcomes, and rendering

The crate-wide `Result<T, String>` (`src/main.rs:27`) flattens I/O and subprocess sources. Empty strings mean “failure already printed” in doctor, watched run, and migrate (`src/doctor.rs:105-110`, `src/run.rs:128-130`, `src/migrate.rs:108-118`). Model types such as `ProjectContext` print themselves and build JSON manually (`src/project.rs:68-151`).

Use three different concepts:

- `AppError`: an unexpected failure with structured variants, source errors, operation, path/program, and relevant input. `thiserror` is reasonable if it removes real boilerplate; a hand-written small enum is also fine.
- `CommandOutcome`: successful computation with an intentional success/failure exit status and a structured report. A failing `doctor` report is an outcome, not an empty error.
- Renderers: human and JSON presentation at the CLI edge, writing through supplied `Write` handles rather than arbitrary `println!` calls deep in the model.

`main` should return `std::process::ExitCode`. `process::exit` at `src/main.rs:466-475` skips destructors on the current stack, which works against staged-file and temporary-resource cleanup.

Use `serde`/`serde_json` for the documented machine-readable contracts (`about`, routes, beans) once their schemas are captured in compatibility tests. Do not add a general logging framework unless Jails develops long-running/concurrent behavior that needs structured spans.

Acceptance criteria:

- No `Result<T, String>` aliases and no empty-string sentinel errors.
- Lower layers neither print an error and return it nor manually choose the process exit code.
- One CLI boundary renders an error once and maps outcomes/errors to `ExitCode`.
- JSON tests deserialize and assert schema/fields instead of matching fragments.

## 6. Give project documents semantic owners

Preserve the current targeted text strategy, but put all queries and edits for one format behind one document type:

- `PomDocument`: dependencies, plugins, modules, project kind, release, Spring version/parent facts, and comment-preserving edits. Remove the second XML scanner in `project.rs:213-331` over time.
- `ComposeDocument`: services, owned service blocks, and scoped Postgres connection facts. Runtime start/stop belongs in `compose/runtime`, not the document.
- `ProjectLayout`: a closed `Layer` enum with `ALL`, config key, default subpackage, and display order. `config.rs:33-55` and `inspect.rs:642-654` currently maintain different lists; renamed configured layers can be counted as “Other.”
- `JavaSourceTree`: a sorted, once-per-command view reused by routes, beans, stats, notes, Kafka topic lookup, doctor checks, and main/base-package discovery.

These are deliberately limited semantic readers, not full parsers. Do not grow this work into a full Maven model, full Java compiler frontend, or formatting-destructive YAML/XML serialization.

The hand-parsed `jails.toml` subset is not itself a priority defect: `src/config.rs:18-27` states the trade-off. Keep it strict and clearly custom while the grammar stays tiny. Move to typed `serde` + `toml` only if configuration meaningfully expands; then reject unknown/duplicate keys rather than silently widening the language.

## 7. Target module shape

Source modules should use the modern `file.rs` plus `file/child.rs` layout and avoid `mod.rs`. This is a destination, not the first commit:

```text
src/
  main.rs                       # parse, call library, render, ExitCode
  lib.rs                        # narrow testable application façade
  cli.rs                        # clap schema and CLI -> typed request conversion
  app.rs                        # command dispatch only
  error.rs
  ui.rs

  project.rs
  project/context.rs
  project/layout.rs
  project/source_tree.rs

  change.rs
  change/apply.rs
  change/ownership.rs
  process.rs

  pom.rs
  pom/document.rs
  compose.rs
  compose/document.rs
  compose/runtime.rs
  compose/postgres.rs

  field.rs
  generate.rs
  generate/request.rs
  generate/scaffold.rs
  generate/domain.rs
  generate/web.rs
  generate/cli.rs
  generate/migration.rs
  generate/render.rs

  capability.rs
  capability/database.rs
  capability/messaging.rs
  capability/spring.rs
  capability/tooling.rs

  inspect.rs
  inspect/routes.rs
  inspect/beans.rs
  inspect/stats.rs
  inspect/notes.rs

  doctor.rs
  doctor/checks.rs
  why.rs
  why/rules.rs

  java.rs
  java/lexer.rs
  java/model.rs

templates/java/                   # optional for only the longest stable bodies
```

Dependency direction:

```text
cli -> app -> project-aware feature planners -> field/document models
                                            \-> ChangeSet -> apply/process adapters

renderers -> typed requests + field/project models
renderers -X-> CWD, filesystem writes, stdout, subprocesses
```

Keep small render functions in Rust. Move a long Java body to a checked-in asset only when it materially improves reviewability. Do not require a general template engine before splitting the current files.

## 8. Test architecture

The current suite has valuable end-to-end coverage: 100 `#[test]` functions in the 3,561-line `tests/cli.rs`, all eventually invoking `CARGO_BIN_EXE_jails`. Its infrastructure and dependency tiers are mixed, though:

- `tests/common/mod.rs` contains command setup, fake tools, real-host probing, and the main Spring fixture.
- `temp_dir` has 109 callers but returns a bare `PathBuf`, so successful test directories leak.
- `write_fake_maven` has 27 callers and is also used for Docker, Java, psql, sqlite3, and jshell. Its `$*` log loses argument boundaries, while `read_log` turns every read failure into an empty log (`tests/common/mod.rs:38-68`).
- 21 tests probe Maven, ten probe the target JDK, and one probes Docker, then return early if unavailable. The harness reports those as passing, so a green run does not say which acceptance behavior executed.
- Fake tools are POSIX shell scripts and `set_executable` only exists under `cfg(unix)` (`tests/common/mod.rs:42-64`), while production includes Windows branches.

Use a layered test stack:

| Boundary | Test style |
|---|---|
| Parsers, semantic field types, POM/Compose edits, renderers | unit/table/property tests beside the module |
| Feature planning and merged changes | pure `ChangeSet` tests |
| Change application and ownership conflict behavior | component tests in an RAII temporary workspace |
| Command construction | exact `CommandSpec` tests, preserving argv/env boundaries |
| CLI parsing, aliases, dispatch, exit mapping | direct library/application tests |
| Binary wiring and a user journey per command family | small black-box CLI suite |
| Generated Java compilation | explicit Maven/JDK acceptance target |
| Docker/Testcontainers runtime | explicit service acceptance target |

Recommended hybrid layout:

```text
tests/
  cli.rs                         # default hermetic integration target
  cli/offline.rs
  cli/filesystem.rs
  cli/process_contracts.rs

  acceptance_maven.rs            # separately selected/provisioned
  acceptance_services.rs         # Docker lane

  support/mod.rs                  # integration-test convention exception
  support/workspace.rs            # tempfile::TempDir owner
  support/fake_toolchain.rs
  support/toolchain.rs
  support/fixtures/plain.rs
  support/fixtures/spring.rs
  support/fixtures/inspectable.rs
```

`tests/support/mod.rs` is the intentional exception to the source `mod.rs` rule: Rust treats a top-level file in `tests/` as a separate integration crate, while a subdirectory module avoids a useless zero-test target. Default behavior tests stay in submodules of one target to avoid excessive independent linking; Maven and Docker are separate targets because CI must select them independently.

Specific changes:

- Add `TestWorkspace` backed by `tempfile::TempDir`; provide an explicit preserve-on-failure/debug escape hatch.
- Sanitize the default test environment: isolated home/config, stable locale/color, and a constructed PATH. For real Maven tests, probe the Java Maven actually uses, not merely `javac` on PATH.
- Prefer direct `CommandSpec` assertions. Where black-box fake executables remain, record lossless argv and distinguish “not invoked” from “log could not be read.”
- Mark locally optional real-tool tests with a reasoned `#[ignore = "requires Maven/JDK"]` or `#[ignore = "requires Docker"]`. Dedicated CI runs the target/ignored set and fails if prerequisites are missing; it must not silently return.
- Add timeouts and bounded concurrency to Maven/Docker lanes.
- Parse JSON in assertions. Use focused golden/snapshot tests for stable generated artifacts and user-visible reports, with semantic assertions alongside them so snapshots are not the only oracle.
- Add property tests only where a law exists: field parsing, POM/Compose edit idempotence, and plan/apply/remove round trips. Do not turn every example test into `proptest`.
- Decide whether Windows is supported. If yes, add a native Windows command-contract lane; if no, gate/document Unix-only integration fixtures instead of accidentally failing to compile.

Before moving tests, record `cargo test -- --list` and prove the inventory is unchanged. Delete redundant black-box substring tests only after an equivalent lower-level contract test exists.

## 9. Rust 2026 baseline and Cargo policy

Rust 1.97.1 is the current stable release in this review, and the repository already uses edition 2024. Edition 2024 implies Cargo resolver 3 for this non-virtual package, so adding `resolver = "3"` is unnecessary. Sources: [official Rust releases](https://blog.rust-lang.org/releases/), [Edition 2024 resolver behavior](https://doc.rust-lang.org/stable/edition-guide/rust-2024/cargo-resolver.html).

Separate three concepts that are currently conflated:

1. Edition: keep `edition = "2024"`.
2. Development/formatting toolchain: pin 1.97.1 in one place (`rust-toolchain.toml` or mise; choose one Rust authority).
3. MSRV: set Cargo's `rust-version` to the oldest version actually supported and test it. The apparent source syntax floor is 1.88 because edition-2024 let chains stabilized there. A sensible starting policy is `rust-version = "1.88"` plus an MSRV CI check; if the product intentionally supports only current stable, declare 1.97 instead. Do not copy “1.97” into `rust-version` merely because `rust.md` calls it the coding baseline. Sources: [Cargo `rust-version`](https://doc.rust-lang.org/stable/cargo/reference/rust-version.html), [Rust 1.88 let chains](https://blog.rust-lang.org/2025/06/26/Rust-1.88.0/).

At review time, `cargo check --all-targets` succeeded but emitted duplicate-test-attribute warnings in `src/sql.rs`; formatting check had broad differences; strict Clippy reported 27 diagnostics. Baseline these deliberately before making them gates. Do not mix a repository-wide format rewrite with architecture commits.

Target CI after the baseline is clean:

```text
cargo fmt --all -- --check
cargo check --locked --all-targets
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo test --locked
cargo +1.88 check --locked --all-targets     # if 1.88 is the chosen MSRV
```

Add focused `[lints.rust]` and `[lints.clippy]` policy to `Cargo.toml`. Do not enable the entire Clippy `restriction` group; Clippy explicitly warns that restriction lints can contradict each other. If the whole `pedantic` group is desired, baseline it in a dedicated change with narrow, reasoned allows. Source: [Clippy usage and lint-group guidance](https://doc.rust-lang.org/stable/clippy/usage.html).

Keep the tracked `Cargo.lock` and use `--locked` in CI/reproducible installation. Prefer a few justified dependencies:

- `tempfile` as a dev dependency for owned workspaces.
- `serde`/`serde_json` when machine-readable report schemas move out of manual string assembly.
- `thiserror` only if the structured error enum's manual implementation becomes noise.
- A stable digest crate only if the ownership manifest uses hashes.

Do not add Tokio, a Cargo workspace, feature matrices, a build script, a virtual filesystem trait, or a plugin system without a separate demonstrated need.

The thin `main.rs`/`lib.rs` split is justified here because it creates a real testable application boundary, not because the file count is large. Rust's own testing guidance recommends putting binary logic in a library so integration tests can exercise it directly: [Rust test organization](https://doc.rust-lang.org/book/ch11-03-test-organization.html#integration-tests-for-binary-crates). Returning `ExitCode` also permits normal destruction instead of `process::exit`: [`ExitCode`](https://doc.rust-lang.org/std/process/struct.ExitCode.html), [`process::exit` cleanup behavior](https://doc.rust-lang.org/std/process/fn.exit.html).

## 10. Migration sequence

### Phase 0 — Freeze intended behavior and policies

- Record the current test inventory and representative generated outputs.
- Add regression coverage for every P0 contradiction above.
- Decide Java target policy, preview scope, supported platforms, ownership persistence, and MSRV.
- Baseline formatting and chosen lints in separate mechanical changes.

Exit: every known semantic contradiction has an explicit intended contract and test; generated-output goldens exist for the high-volume renderers.

### Phase 1 — Thin binary, typed requests/outcomes, explicit project context

- Add `lib.rs`, keep modules private, and expose only a narrow application façade.
- Move Clap definitions/conversion to `cli.rs`; make `main() -> ExitCode` parse, call, render, and return.
- Introduce `AppError`, `CommandOutcome`, `Diagnostics`, `ApplyMode`, `ProjectContext`, `ProjectLayout`, and typed command request structs.
- Migrate commands one at a time from implicit CWD to explicit context.

Exit: no domain/helper code reads process CWD; `CWD_LOCK` is gone; lower layers return values rather than choosing exit behavior.

### Phase 2 — Process and tool resolution

- Introduce `CommandSpec` and the synchronous executor.
- Move wrapper/Maven/Docker/PATH resolution into one owner.
- Port inherited, stdin, capture, and tee paths without behavior changes, then fix the debug inconsistency in a separate commit.
- Convert opaque forwarded arguments/classpaths to `OsString`.

Exit: subprocess execution and debug rendering have one implementation; debug-executes and preview-does-not tests cover every output/stdin mode.

### Phase 3 — `ChangeSet`, preview, and ownership

- Start with `generate`/`destroy`, using the existing `Artifact` shape as an adapter.
- Move dispatcher, package-info, dependency, plugin, migration, and fixture work into the same plan.
- Migrate `rename`, then combined `add`/`remove`, then `new`/`new-cli` using a staging directory in the destination parent.
- Implement the chosen managed-file/block ownership policy and conflict reporting.

Exit: every project writer is plan -> preflight -> preview/apply; no side effect is hidden; inverse operations use recorded/shared ownership data; external effects are post-commit.

### Phase 4 — Typed field/generation model

- Add `GenerationRequest`, `FieldSpec`, `TypeRef`, presence/validation/schema types, and structured parse errors.
- Adapt Java, SQL, JDBC, and fixture renderers behind compatibility projections.
- Remove Java-string re-parsing only after golden equivalence is proven.

Exit: renderer dependency direction is one-way from semantic model; current generated output remains stable.

### Phase 5 — Semantic documents and source snapshot

- Introduce `PomDocument`, `ComposeDocument`, `Layer`, and `JavaSourceTree`.
- Move inspection/doctor queries and duplicate Maven/Compose readers to those owners.
- Use structured report values and stable JSON rendering.

Exit: one comment-preserving reader/editor owns each project format; configured layers are honored everywhere; each command walks Java sources once.

### Phase 6 — Physical split and test/CI lanes

- Split `add.rs`, `generate.rs`, and `spring.rs` along the now-proven planner/model/renderer boundaries.
- Move the default tests into hermetic submodules and real-tool tests into explicit acceptance targets.
- Replace redundant black-box checks only after lower-level coverage lands.

Exit: default `cargo test --locked` is hermetic and reports no silent skips; Maven and Docker lanes are separately selectable, provisioned, timed out, and mandatory in CI where enabled.

## Decisions that need an explicit ADR before implementation

1. Is the Java default 25 or 27, and what does Jails promise before a JDK/Initializr release is generally available?
2. Is MSRV 1.88 with current-stable development tooling, or is Jails intentionally latest-stable-only?
3. Is `.jails/state.toml` acceptable as committed project metadata? If not, what evidence authorizes destructive remove/destroy?
4. Does global `--pretend` cover `new`, migration creation, case generation, formatter execution, Git initialization, and service start/stop exactly as the README promises?
5. Is Windows a supported platform or only production-compilation best effort?
6. Is the new library façade an internal testing surface or a supported external API? Keep it minimal until that is decided.

## Non-goals

- No rewrite, CLI syntax redesign, or broad generated-Java redesign.
- No dynamic plugin architecture for closed command/capability sets.
- No async runtime for synchronous subprocess work.
- No trait per command, renderer, repository, filesystem, or tool.
- No formatting-destructive XML/YAML round trip.
- No full Java parser.
- No generic `utils.rs`; shared behavior needs a domain owner.
- No general template language as a prerequisite.
- No behavior changes hidden inside file moves or formatting commits.
- No promise of filesystem-wide atomicity.
- No destructive removal of a file/block whose ownership or unchanged state cannot be proven.

## Definition of success

The refactor is complete when:

- Invocation policy, project identity, planning, application, process execution, and rendering are separate and have one owner each.
- Every write is visible in a plan; preview and apply consume the same plan.
- `debug` never changes whether an apply-mode command executes.
- Multi-capability commands preflight and apply as one unit.
- Generate/destroy and add/remove cannot drift through duplicated path knowledge.
- Java, SQL, JDBC, and fixture rendering consume one typed field model.
- There is one Maven/tool resolver and no process-global CWD dependency.
- Errors preserve sources; reports are structured; `main` returns `ExitCode`.
- Default tests are hermetic, temporary resources are owned, and optional acceptance coverage is visibly skipped or explicitly run—never silently passed.
- Comment/format preservation and user-edit ownership remain first-class product guarantees.
