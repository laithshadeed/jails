# jails

Rails-CLI-inspired scaffolding tool for Spring Boot / plain Maven projects.
`README.md` is the user-facing surface (command list, field types, what's
deliberately deferred) — treat it as the spec, and update it in the same
change as the code. The original `prompt.md` spec was deleted once the
commands it described all shipped; don't go looking for it.

The scope bar: no ORM, no jails runtime jar, no Lombok, no preview features in
generated Java, and no plugin system with lifecycle hooks. Check `README.md`'s
"Not yet" before adding a command that isn't already there.

**"No Gradle" was on that list and was deliberately removed on 2026-08-24.**
The target that reversed it is `minicom-public/spring`, a Gradle + Spring Boot
project that has to be worked in daily: `add`, `check`, `test`, `build` and
`run` all refused there, and `generate` wrote code with a note listing the
dependencies the reader had to splice by hand. Degrading politely is worth less
than working, when the project is the one you are actually in. The old rule's
*reason* survives as the bar `gradle.rs` has to clear -- answer exactly or
refuse, never guess -- because a tool that half-understands a build file
reporting a dependency the build does not have is still the worst outcome
available.

**Every idea, roadmap item and open design question lives in `pending.md`.**
This file describes what the code *is* and the traps in it; `pending.md`
describes what is not done and why. Do not add proposals here.

**`pending.md` holds only what is open.** A closed item is *deleted* from it
rather than marked done, so `git log -p -- pending.md` is where a closed one and
the measurement it closed on live. Section numbers are stable and never reused,
and the file ends with a one-line index of the closed ones so a `pending.md §N`
citation in the code still resolves to a subject.

**Six design documents were deleted and folded into `pending.md`:** `plan.md`,
`abstract.md`, `playground.md`, and -- on 2026-08-24 and 2026-08-25 --
`missing.md` (what one real migration needed and did not get, all eight entries
now closed), `refactor.md` (the 26-item structural audit) and `test.md` (the
test-suite performance investigation). Roughly 208 comments across the code
still cite them by section number -- `plan.md §R6`, `abstract.md §3.2`,
`missing.md §3` -- and those citations are still the best record of *why* a
decision was made. They resolve through git:
`git log --diff-filter=D -- plan.md` finds the commit that removed the file,
and `git show <commit>^:plan.md` prints it.

**One exception:** the `refactor.md` on disk when it was folded in had been
recreated untracked, so `git show` reaches an *older* tracked version rather
than that one. `pending.md`'s header says so. `pending.md` carries forward only
what is still outstanding, re-measured rather than transcribed.

## Workspace

Thirteen crates, lowest first. A crate may only depend on one below it, and
Cargo enforces that; `no_module_depends_on_a_layer_above_its_own` in
`tests/architecture/` enforces the same rule for module-level edges the
compiler cannot see, and assigns every module its crate. **That table
(`LAYERS`, `tests/architecture/rules.rs`) is the authority on which crate a
module belongs to** — this one is the prose, and prose is what goes stale.

| crate | what belongs in it |
|---|---|
| `jails-support` | **write, run, encode.** Nothing here knows what a Java project is — `codemod` moved to `jails-project` and `CWD_LOCK` to `jails-testkit` when that rule was applied honestly, and `runner` is `hermetic`, named for the contract that separates it from `process`. `Result`, `Failure` and `debug_cmd` live here. |
| `jails-testkit` | one `CWD_LOCK`, taken as a `[dev-dependency]`. Test infrastructure that cannot be `#[cfg(test)]`, because a dependent crate's tests cannot see one. |
| `jails-java` | reading Java (`java`, `classfile`) and rendering templates into it (`template`). |
| `jails-spec` | where a project is and how it is laid out (`build`, `spec::paths`, `spec::layout`), what a field spec means (`spec::field`), and the closed CLI vocabularies (`spec::kind`). |
| `jails-state` | **jails' own machine state, read and classified**: `compat` (absent / current / unreadable, never a fourth answer that quietly repairs something) and `listing` (what a directory holds). Below the Java project on purpose — `jails-commit` needs both and neither is about Java. |
| `jails-protocol` | **the validated values every closed jails format is built from** — `Recipe`, `Name`, `Package`, `FieldSpec`, `EntityId`, `ResourceKey`, and the plan/transition/effect vocabulary above them. One constructor per type, and every wire decoder calls it, so a value rejected at the CLI cannot arrive through a recovered journal instead. 23 flat modules; §7.4 of `pending.md` groups them. |
| `jails-project` | one resolved `model::Project`, plus every file jails writes *about* a project — the reader's (`config`, `compose`, `pom`, `gradle`) and the read-only view of jails' own (`compat`, `projection`, `ledger`). |
| `jails-generate` | everything that decides what Java to write: `generate`, `spring`, `add`, `sql`. Its planning half (`plan_for`, `artifacts_for`) is what the engine calls and is pure. |
| `jails-prepare` | **turning semantic desire into an exact executable transition**: `desire`, `reconcile`, `pipeline`, `merge`, `sandbox`, `report`. Plan-only — nothing here creates `.jails/` or commits anything. Everything a commit needs to *decide* is decided here, so the executor applies a value rather than re-deriving one. |
| `jails-commit` | **making a prepared transition durable, and recovering one**: `store`, `journal`, `execute`, `activate`, `recover`, `gc`. Crash recovery rolls a fully persisted, validated journal *forward*; preimages exist for a guarded explicit abort and for audit, not as the crash policy. That is what keeps this crate small — there is one direction to finish in. |
| `jails-report` | commands that **answer a question**: `doctor`, `why`, `explain`, `source`, `commands`. Read-only by contract, and the contract is structural — this crate sits *below* `jails-drive`, so a reporting command that started something would not compile. |
| `jails-drive` | commands that **start something**: `run`, `testd`, `launcher`, `affected`, `migrate`, `kafka`, `console`, `bench`, `lint`, `reports`. The one edge back down is `run` → `report::why`, because `mvn spring-boot:run` exits 0 over a failed startup. |
| `jails-engine` | **one request, as one transition.** `route` and its submodules are where a parsed command becomes a capture, a desire, a preparation and a commit. Above the executor because it drives it; below the CLI because it is not about arguments. |
| `jails` (root) | the binary: `main`, `new`, `app`, `invoke`, and `tests/`. |

`jails-engine` and `jails-drive` sit at the same level and do not reference
each other; so do `jails-generate` and `jails-prepare`. The layering is a DAG,
not a line, and `LAYERS` records it as one number per module because a
same-level edge is allowed.

**This existed because `src/` was one twelve-module cycle** — `add`, `compose`,
`config`, `generate`, `inspect`, `launcher`, `model`, `project`, `run`,
`spring`, `sql`, `why` — so no boundary could be drawn anywhere in it. The
cause was not tangled logic: every back-edge was a single symbol, because
everything below the generators reached up into `generate.rs` for `Field`,
`layout` and `find_project_root`. `jails-spec` is those symbols at their own
layer.

Four things to know before touching it:

- **Each crate's `lib.rs` carries a facade block** re-exporting the lower
  crates, so module code keeps saying `crate::java` and `crate::Result`
  wherever it ships. Only that block knows which crate a module lives in,
  which is what makes moving one a one-line change instead of a sweep through
  forty files. Keep it trimmed to what the crate actually references.
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

- `src/main.rs` — clap derive CLI, dispatch only.
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
  A template used to be a Rust `format!` string, which meant **every brace
  doubled** (`class {name}Controller {{`, and `{{@code public}}` in Javadoc)
  because `format!` owns that syntax and Java is made of braces. The
  templates are real `.java` files now, pulled in with `include_str!` so they
  are still compile-time constants with no runtime file access and no new
  dependency. Placeholders are `{{name}}`, chosen by **checking**: no `{{`
  appears anywhere in the 162 golden files, so it cannot collide, while
  `${name}` would (spring.rs generates `@Value("${...}")`). A missing or
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
  list is derived from it rather than written out again. `inspect.rs` used to
  keep its own copy, which is exactly the drift `refactor.md` §6 predicted --
  see `pending.md`'s header for how that citation resolves. It
  reported against jails' *default* package names, so a project with
  `adapters = "persistence"` had its adapters counted as "Other", and `cli`
  and `messaging` -- missing from the copy -- were never counted at all.
  Anything reporting per layer must go through `Config::layers()`, which
  applies the project's renames. Layer matching is on whole path segments in
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
  `find_project_root` used to look for `pom.xml` alone, which refused ~30
  commands on a foreign project when only about ten of them need Maven at all
  (`inspect.rs` and `rename.rs` contain zero occurrences of `pom`). The door
  is any recognised marker now, nearest wins, and the Maven-inherent commands
  refuse themselves through `require_maven` — a refusal that can say what
  still works. **jails never reads, writes, parses or invokes a foreign build
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
- `crates/jails-support/src/codemod.rs` — **the marked block, and only that**: `# jails:<marker>`
  … `# /jails:<marker>`, which is how jails edits a file the reader owns and
  what makes `remove` the exact inverse of `add`. It had five owners
  (`compose.rs`, `add.rs`, `add/database.rs`, `add/test_wiring.rs`,
  `doctor.rs`) each with its own `format!`; `tests/architecture/` now fails
  on a `# jails:` literal outside this module, so a sixth cannot appear
  quietly. It no longer wraps a capability's `application.properties` settings
  — see the per-key rule in the gotchas below. `Marked::indented` exists because a marker at column zero inside a
  YAML mapping is a parse error rather than a misplaced comment. There is no
  `replace` — nothing needs one, and `remove` then `add` is the path `sync`
  takes.
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
  Before it, `add` knew what a capability installs and `doctor` could not ask,
  so a project whose generated file had been deleted reported nothing. The
  hand-written checks stay: they cover projects with **no** recorded capability
  list, where there is nothing to derive from, and they carry failure modes no
  plan can express (two Jackson majors, podman's socket). Every `FAIL`
  carries a `fix:` line (an integration test asserts this), and a failure
  exits non-zero via an *empty* `Err` so `main` prints no redundant
  `jails: ` line.
- `crates/jails-drive/src/why.rs` — `why`. A table of (signature, explanation, fix) rules
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

  **It is no longer the biggest file here.** `plan.md` §6.5's split landed:
  `crates/jails-generate/src/spring/workflow.rs` (usecase + its outbox half, transition, query),
  `crates/jails-generate/src/spring/durable.rs` (job, durable-job), `crates/jails-generate/src/spring/http.rs` (client,
  fetcher, http-workflow, http-sink) and `crates/jails-generate/src/spring/schema.rs` (association,
  idempotency). `spring.rs` itself is down from 6,624 to ~1,900 lines and holds
  the shared precondition, the shared helpers used by more than one kind, and
  the capability slices.

  Two things the split needed and the next one will too: a child module reaches
  its parent's **private** items through `use super::*;`, but the parent needs
  `pub(crate)` on anything it borrows back — `scheduling_config_java` and
  `durable_alternate_sample` are the two the outbox shares with `durable`. And
  `include_str!` is relative to the *file*, so every template path in a moved
  block gains a `../`.

  The largest module is now `crates/jails-generate/src/generate.rs`, which `abstract.md` §3.2 calls
  Ousterhout's named anti-pattern verbatim — parse → dispatch → write → side
  effects. `tests/architecture/` has a gate on the largest module precisely
  so a split cannot be satisfied by *moving* a monolith.

  **Placement is a value, not six strings: `spring::Slice`.** Every generator
  and renderer here takes a `Slice` — a resolved `model::Project` plus the
  `--package` override — and asks it for `placed(Layer::X)` (this slice's own
  classes, honouring `--package`) or `owned(Layer::X)` (where an *existing*
  resource lives, ignoring it). That distinction is load-bearing and used to
  be restated at every call site as `place(layout::WEB)` versus
  `subpackage(&base, config.layer(layout::DOMAIN))`. Sixteen functions took
  eight to twelve positional parameters because of it; **no function in this
  file now takes more than five**, and `tests/architecture/` fails if one
  does. `Target`, `Defaults`, `Emission`, `Update` and `Projection` are the
  other parameter objects that fell out — each one a group of values that is
  computed together and consumed together.

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
  create the project, seed `.jails/app.toml`, apply it. One command from an
  empty directory to a project that passes `mvn clean verify`. Making it work
  meant removing every `Project::discover()` from the apply path — every route
  takes an explicit `Run` carrying a resolved `Project` — because `discover`
  reads the **process CWD**, which is the parent directory, not the project
  just created.
- `src/app.rs` — `jails app plan|apply`: a declarative manifest at
  `.jails/app.toml` (`schema`, `capabilities`, and a closed `[[generate]]`
  schema of `kind`/`name`/`fields`/`timestamps`/`indexes`/`package`/`on`/
  `yields`, with `strategy_on`/`strategy_yields` kept as deprecated aliases
  because they shipped in a user-facing file format — setting one reference
  under both spellings is an error, not a last-one-wins). **Deliberately domain-blind** — the module docs say it,
  and it is load-bearing: a crawler, a support inbox and a payments gateway
  are three lists of the same generic intents, and none of them gets a
  command, branch, enum or template in core. `apply` is **one transition** over
  the whole manifest: capabilities and intents are declared together and
  reconciliation works out the difference, so an interrupted apply resumes from
  the journal rather than from a half-written registry.

  It used to reconcile every capability a *second* time, because a generator
  can create an integration point an already-installed capability needs — the
  case being `add db` wiring a `@SpringBootTest` that a later row writes. That
  is fixed where it belongs instead: the capability writing the test puts the
  container import in itself (`route::support::with_test_support`), so there is
  nothing for a second pass to catch and the formatter runs once.
- `.jails/ledger.toml` — the **one** file jails keeps its own bookkeeping in,
  and it is the transaction store's, not a module's. `jails-protocol`'s
  `envelope.rs` owns the file format (magic, schema number, checksum, and a
  hex-encoded canonical payload); `jails-commit`'s `store.rs` reads and writes
  it; `crates/jails-project/src/compat.rs` is the **read-only** classifier
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
- `crates/jails-drive/src/source.rs` — `jails src <Type>`: where a type's source is. The one
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
- `crates/jails-drive/src/rename.rs` — `rename`. Textual by design (see its module docs for
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
  Eleven gates, each a number measured over *production* Rust (comments,
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
- `crates/jails-drive/src/explain.rs` — `jails explain <kind>`: why each artifact is shaped the
  way it is, and the trap it invites. **A hand-written table, deliberately** —
  a rationale is prose with nowhere to derive it from — so it is held to
  `why.rs`'s shape: a value in a table, one edit per kind, with
  `every_kind_has_an_explanation` failing the build when a kind is added
  without one. That is what stops it becoming the editor lists.
- `crates/jails-drive/src/commands.rs` — `jails commands [--json]`: every subcommand, generator
  kind, capability and flag, walked out of the same `clap::Command` that parses
  the arguments and the same `ValueEnum`s that validate them. **There is no
  second list**, which is the point — adding a kind is one edit and this output
  follows.
- `jails.nvim/` — tracked in this repo, but Lua, not Rust: a thin `:Jails`
  wrapper that shells out to the binary on PATH. **It no longer keeps its own
  completion tables**: 160 lines of `SUBCOMMANDS`/`KINDS`/`CAPABILITIES`/
  `OPTIONS` were deleted in favour of reading `jails commands --json` once per
  session. They had drifted eight kinds and three capabilities behind the CLI,
  and `tests/editor.rs` pinning them only caught it after the fact; that test
  now asserts the tables have *not* come back. Every failure path — an older
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
cargo build --workspace && cargo test --workspace && cargo install --path .
```
**`--workspace` is not optional.** `cargo test` at a workspace root tests the
root package only: it reported 390 passing where the tree has 418, and nothing
said the other 28 had not run. Tests must stay green before installing. A Stop
hook runs this automatically (see `.claude/settings.json`) — don't skip it
manually even though the hook exists, since the hook only fires on turn end,
not mid-turn.

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
- **Generated projects target Java 25** (`pom::TARGET_RELEASE`), the current
  LTS. This was 27 and was changed deliberately: 27 is non-LTS, was not GA
  until 2026-09-15, had no vendor build in mise's registry (so an EA build had
  to be symlinked in), had no `eclipse-temurin:27-jre` image for `add docker`,
  and made `doctor` FAIL by default on any shell without mise's activation
  hook. Everything jails generates is available at 25. `java` on a bare PATH
  here is 26, which accepts `--release 25`, so the tier-3 tests no longer skip
  for that reason.
- **Tier-3 tests gate on `real_java_supports_target_release()`, not just
  on a JDK being present.** A JDK older than the target rejects
  `--release N` outright, so presence is not enough. Without the gate the
  suite goes red on any machine that hasn't installed the new JDK yet.
- **`base_package()` falls back to the shallowest .java file.** It used to
  require `*Application.java`, which only Spring projects have — `new-cli`
  projects have `App.java`, so `add` failed on exactly the projects it's
  most useful for.
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
- **`JAILS_MAVEN` names the Maven command, and mvnd is probed before it is
  chosen.** mvnd writes a registry under the Maven user home *before* Maven
  runs, so a read-only home kills it with a non-zero exit indistinguishable
  from a failing build at the call site -- a retry there would re-run a
  genuinely broken build. `maven::mvnd_can_start` answers it up front instead.
- **`java::types_annotated_with` is the one walk of `src/test/java`.** There
  were three, two matching a raw substring -- which reads the
  `@SpringBootTest` inside `TestcontainersConfig`'s own Javadoc example as a
  declaration. That is how `doctor` came to name the wrong container config and
  then report every other test as missing an import of it.
- **The `@Import` splice lives in `jails-java`, not in `add`.** Two engines
  perform it now -- `add/test_wiring.rs` and the V2 projection -- and a second
  copy of a surgical edit to a file the reader owns is a copy that drifts.
  `jails_java::annotate` is text in and text out: `splice_import`,
  `unsplice_import`, and `is_spring_boot_test`, which reads through
  `java::blanked()` so the `@SpringBootTest` in `TestcontainersConfig`'s own
  Javadoc example is not mistaken for one on a class.
- **`add db`'s test wiring is an imported `@TestConfiguration`, and both
  halves are load-bearing.** The container is declared as a `@Bean` with
  `@ServiceConnection` in `TestcontainersConfig` (not a
  `@Testcontainers`/`@Container` static field: Spring caches the context past
  the container's JUnit-managed lifetime, and later tests then fail against a
  stopped container). It is `@Import`ed rather than registered globally --
  jails used to list an `ApplicationContextInitializer` in test
  `META-INF/spring.factories`, which gave every `@SpringBootTest` a DataSource
  for free *and* made every pure slice and `@WebMvcTest` start a PostgreSQL it
  never queried.
  The reason the global version existed is still real: once the JDBC starter
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
- **A property cannot tag meters registered directly on the registry.**
  `management.metrics.tags.*` was removed in Boot 3, and its replacement
  `management.observations.key-values.*` tags *observations* — a plain
  `Counter` is not one, so half the meters go untagged and nothing complains.
  `add observability` generates a `MeterRegistryCustomizer` calling
  `config().commonTags(...)`, which covers both. Boot 4 also moved that
  interface out of `actuate.autoconfigure` with no shim, so the import is
  version-sniffed like `@AutoConfigureMockMvc` is.
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
  implementations by reading `java::type_info(...).supertypes` under the
  domain package. That is deliberately *better* than a stored list: an
  implementation added by hand after the generate call is still one of this
  strategy's classes, and leaving it behind implementing a deleted interface
  stops the project compiling.
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
  that calls `std::env::set_current_dir` MUST hold `CWD_LOCK` for the duration,
  or parallel tests race on it. The lock lives in `jails-support` and is
  deliberately **not** `#[cfg(test)]`: the crates that need it are not the
  crate that defines it, and one instance per dependent test binary is exactly
  the scope it has to cover.
- **`#[cfg(test)]` in a library crate means "when *this* crate is under
  test".** A dependent crate's tests cannot see it. That killed
  `parse_fields_for_test`, a `#[cfg(test)]` helper `sql.rs` and `spring.rs`
  called from their own test modules back when one binary held everything. If
  a test helper has to cross a crate boundary, it is ordinary public API — or
  it should not exist, which was the answer there.
- **`cargo clippy` works here now**, and a `.githooks/pre-commit` runs
  `cargo fmt --all --check` and `cargo clippy --workspace --all-targets` before every
  commit, so
  a lint is a *blocked commit* rather than a warning you can ignore. It used to
  fail with E0514 (crate compiled by incompatible rustc) on a toolchain
  mismatch, which is why this file said to skip it; that is no longer true, and
  running it before staging saves a rejected commit. `cargo clippy --fix
  --allow-dirty --allow-staged --all-targets` handles the mechanical ones.
  Note `.githooks/` is untracked, so it is a property of this checkout rather
  than of the project.
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
  into `Optional<...>`. `fields_from_record` used to store `Optional<String>`
  there instead, so a template that worked for `parse_fields` input produced
  uncompilable code for a record read off disk. Two representations of one
  thing is how that happens.

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
Admin` refused over files it had not written. In `app/reconcile.rs` the same
collision would have merged a regenerated intent against somebody else's base.

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

**A skipped tier-3 test is reported as passing.** When `TARGET_RELEASE` was
27 — an unreleased JDK — `javac` on a bare PATH rejected it and **11 of the
104 integration tests did nothing** while the suite said green. The move to 25
should have removed that cause; confirm it rather than assume it. Every skip
goes through `common::skip()`, and `JAILS_REQUIRE_TOOLCHAIN=1 cargo test`
turns each one into a failure naming what was missing. Use it before believing
a green run covered the generated-code path:

```
JAILS_REQUIRE_TOOLCHAIN=1 cargo test
```

Note `real_path_without_mvnd()` rebuilds PATH for the real-mvn tests, so
which JDK Maven actually uses is decided by `JAVA_HOME`, not by the `javac`
the gate probed — the two can disagree, and the gate is the optimistic one.
