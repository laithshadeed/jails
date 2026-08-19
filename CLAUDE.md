# jails

Rails-CLI-inspired scaffolding tool for Spring Boot / plain Maven projects.
`README.md` is the user-facing surface (command list, field types, what's
deliberately deferred) — treat it as the spec, and update it in the same
change as the code. The original `prompt.md` spec was deleted once the
commands it described all shipped; don't go looking for it.

The scope bar: this is a deliberately small v1. No `routes`, no Gradle, no
plugin system. Check `README.md`'s "Not yet" before adding a command that
isn't already there.

## Layout

- `src/main.rs` — clap derive CLI, dispatch only.
- `src/new.rs` — `new` (start.spring.io wrapper, real network) and `new-cli`
  (hand-written pom/App/AppTest, no network). Both also seed
  `src/test/resources/fixtures/.gitkeep`.
- `src/generate.rs` — all Java templates (`format!`, no template engine) +
  `generate`/`destroy`. `ArtifactKind` is a `clap::ValueEnum` — keep it that
  way, see gotcha below.
- `src/add.rs` — `add`/`remove <capability>` (csv/sqlite/json/db/kafka/…):
  grows or shrinks an existing project by a whole slice (dependency + code +
  test, and for `db`/`kafka` a compose service). `Capability` is a
  `clap::ValueEnum` for the same completion reason as `ArtifactKind`.
- `src/pom.rs` — flavor and release-level detection, plus a comment-preserving
  dependency/plugin splice and unsplice. `TARGET_RELEASE` lives here.
- `src/compose.rs` — the other user-owned file jails edits: `compose.yaml`.
  Marked service blocks so `add db` and `add kafka` stack, and `remove` can
  take one service out without touching the other. Also `start`/`stop`
  (`docker compose up/stop`) and the auto-start `run` shells out to.
- `src/run.rs` — `test`/`build`/`clean`/`run`, shells to `mvn`/`mvnd`. `run`/`watch`
  start compose services first when `compose.yaml` is present.
- `src/console.rs` — `db`/`dbconsole` (`psql` or `sqlite3`) and `console`/`c`
  (`jshell` + Maven classpath). Interactive; inherit stdio.
- `src/java.rs` — a deliberately small Java reader shared by `inspect`,
  `doctor` and `rename`: annotations and what they are attached to, a type's
  supertypes, a constructor's parameters. **Not a parser, and must not grow
  into one.** Its one trick is `blanked()`, which replaces comments and
  literals with spaces *of the same length*, so a scan cannot be fooled by
  `// @Service` while byte offsets still index the original source — which
  is why `annotations()` slices names and args out of `source`, not out of
  the blanked copy.
- `src/inspect.rs` — `routes` and `beans`. Reads source, never a running
  context: instant, and works on a project that does not start (the case
  that matters). The cost is anything decided at runtime, which the output
  states rather than hiding.
- `src/doctor.rs` — `doctor`. Read-only by contract: it must never start,
  stop or write anything, so it stays safe to run mid-debug. Every `FAIL`
  carries a `fix:` line (an integration test asserts this), and a failure
  exits non-zero via an *empty* `Err` so `main` prints no redundant
  `jails: ` line.
- `src/why.rs` — `why`. A table of (signature, explanation, fix) rules
  matched against a log. Rules sharing a `group` describe one failure
  through different messages and only the most specific is reported. Add
  rules only from failures that actually happened; a guessed cause costs
  more than no cause.
- `src/sql.rs` — the field spec -> SQL mapping: column name, Postgres type,
  and the two JDBC expressions. One column list feeds the DDL, the select,
  the insert, the bind and the row mapper, which is the whole point — a
  hand-written pair drifts (`amount` in the insert against `amount_minor` in
  the select compiles and fails at runtime). **The write expression bakes in
  the receiver** rather than letting callers prefix it: `Timestamp.from(x.at())`
  puts the receiver in the middle, and gluing it on the front yields
  `x.Timestamp.from(at())`, which reads fine and does not compile. Only the
  real-toolchain tier catches that, which is why
  `a_scaffold_with_database_types_compiles_including_its_derived_jdbc_adapter`
  exists.
- `src/rename.rs` — `rename`. Textual by design (see its module docs for
  when to prefer jdt.ls `grn`): whole identifiers only, string literals left
  alone and the skipped count reported.
- `tests/common/mod.rs` + `tests/cli.rs` — integration tests against the
  real compiled binary (`CARGO_BIN_EXE_jails`).
- `jails.nvim/` — tracked in this repo, but Lua, not Rust: a thin `:Jails`
  wrapper that shells out to the binary on PATH. It keeps its own hand-
  maintained `SUBCOMMANDS`/`KINDS`/`CAPABILITIES`/`OPTIONS` lists for
  `:Jails` completion, so a new subcommand, artifact kind, capability or
  flag has to be added there too or it silently won't complete. The
  `<leader>J...` keymaps that drive it live in a *third* repo
  (`~/code/my-dotfiles/home/.config/nvim/init.lua`), which this project's
  git history does not track.

Untracked siblings in this directory are **not** part of the project:
`rails/` and `start.spring.io/` are gitignored reference checkouts (separate
upstream repos, read-only research), and `demo/`/`stacks/` are scratch. Never
edit or document them as if they were jails'.

## Workflow (every change, no exceptions)

```
cargo build && cargo test && cargo install --path .
```
Tests must stay green before installing. A Stop hook runs this
automatically (see `.claude/settings.json`) — don't skip it manually even
though the hook exists, since the hook only fires on turn end, not mid-turn.

## Package layout

Generated code does **not** all land in the base package. `generate::layout`
maps each kind to the subpackage its layer conventionally owns (`domain`,
`repository`, `service`, `web`, `cli`, `adapters`, `api`, `testkit`), and
`--package` overrides it. Two consequences worth knowing before editing
templates:

- **`scaffold` now crosses package boundaries**, so `stub_repository`,
  `service_full`, `controller_full` and `controller_test` take an `extra`
  parameter holding the imports that costs. `import_of` returns an empty
  string when the two packages match, which is what keeps `--package ''`
  (everything flat) compiling.
- **`destroy` has to resolve the same subpackage `generate` used**, so both
  build their paths through the same `place()` closure. A kind added to one
  and not the other silently strands files.

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

## Field syntax: case is the rule

`parse_fields` reads `name:type[!?]`. **Lowercase = jails' table, capitalised =
a type the project owns**, passed through verbatim with no import (same
package). `builtin_by_java_name` is the exception that keeps `id:String`
working; without it a natural spelling would be read as an unknown project type
and silently disable the generated test.

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

- **Generated projects target Java 27** (`pom::TARGET_RELEASE`), which is
  not GA until 2026-09-15. mise's java registry carries *no* JDK 27 build
  of any vendor, so the EA build is symlinked in — see `mise.toml`. This
  shell does not run mise's activation hook, so `java` on a bare PATH is
  still 26; use `mise exec` or an explicit `JAVA_HOME` when something has
  to compile at release 27.
- **Tier-3 tests gate on `real_java_supports_target_release()`, not just
  on a JDK being present.** A JDK older than the target rejects
  `--release N` outright, so presence is not enough. Without the gate the
  suite goes red on any machine that hasn't installed the new JDK yet.
- **`base_package()` falls back to the shallowest .java file.** It used to
  require `*Application.java`, which only Spring projects have — `new-cli`
  projects have `App.java`, so `add` failed on exactly the projects it's
  most useful for.
- **`add json` needs two Jackson artifacts, not one.**
  `findAndRegisterModules()` only finds modules already on the classpath, and
  `jackson-databind` alone has no `java.time` support — so without
  `jackson-datatype-jsr310` every `LocalDate` (a type `generate`'s own field
  table emits) serialises as `{"year":…}` instead of an ISO string. Spring
  pulls it in transitively, so this only ever bit the plain-Maven flavor.
  Keep both artifacts pinned to the same `JACKSON_VERSION`; mixing versions
  across them is a documented `NoSuchMethodError`.
- **`jails check` is `mvn clean verify`.** Incremental `verify` leaves deleted
  tests in `target/`, and Surefire still runs the leftover `.class`. Don't
  "optimize" it back to bare verify.
- **`add db`'s test wiring is a container *bean* registered globally.** Two
  requirements pull in opposite directions and both have to be met. Boot's
  own docs want the container declared as a `@Bean` with
  `@ServiceConnection` (not a `@Testcontainers`/`@Container` static field:
  Spring caches the context past the container's JUnit-managed lifetime, and
  later tests then fail against a stopped container). But the documented way
  to use that `@TestConfiguration` is `@Import` on each test class, which is
  wrong here — JDBC auto-config demands a DataSource for *every*
  `@SpringBootTest` once the starter is present, including tests that never
  query. So `PostgresContainerConfig` is both: an `ApplicationContextInitializer`
  listed in test `META-INF/spring.factories`, whose only job is to register a
  nested `@TestConfiguration` holding the `@ServiceConnection` bean.
  `ServiceConnectionAutoConfiguration` finds it by type
  (`getBeanNamesForType(Container.class)`), so a programmatically registered
  bean definition is fine. Nothing calls `start()` —
  `spring-boot-testcontainers` registers
  `TestcontainersLifecycleApplicationContextInitializer` from its own
  `spring.factories`. That module is therefore a required dependency.
  `should_replace_postgres_test_config` checks for **both** markers, because
  each earlier generation had exactly one of them.
- **`add db` writes `spring.datasource.*` for the application itself**, read
  back out of `compose.yaml` rather than assumed. Spring's docker-compose
  module supplies these where it works and its connection details take
  precedence, so the properties are redundant there and load-bearing
  everywhere else — without them the app dies at startup on any machine
  whose compose provider Spring cannot drive.
- **`add db` on Spring registers a test-classpath ApplicationContextInitializer.**
  Docker Compose is skipped in tests (`spring.docker.compose.skip.in-tests=true`
  by default), so JDBC auto-config has no URL and fails with "Failed to
  determine a suitable driver class". `PostgresContainerConfig` implements
  `ApplicationContextInitializer` and is listed in test-only
  `META-INF/spring.factories`, so every `@SpringBootTest` sees a DataSource
  without an `@Import` on the test class. JDBC auto-config also registers
  persistence-exception translation, which CGLIB-proxies every `@Repository`
  and fails on `final` classes; `add db` disables it with
  `spring.persistence.exceptiontranslation.enabled=false` in main
  `application.properties` (raw SQL, no ORM). Do not "fix" this by setting
  `skip.in-tests=false` (that would share the compose database with tests)
  or by writing a `src/test/resources/application.properties` that shadows
  the main one.
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
  real usage (per spec), but the two real-compile tests in `tests/cli.rs`
  pin to plain `mvn` — see `real_path_without_mvnd()` in
  `tests/common/mod.rs`. Don't "fix" those tests back to the default PATH;
  they'll flake.
- **`mvn`'s own launcher script shells out to `uname`/`dirname`/`ls`/`expr`.**
  If you isolate PATH for a test (mocked mvn or real-mvn-only), you can
  strip specific binaries (e.g. `mvnd`) out of PATH, but you can't reduce
  PATH to *just* the tool directory — the real `mvn` script breaks with
  "command not found" for coreutils. Mocked fake-mvn scripts don't have
  this problem (they're a single `#!/bin/sh` line with no external calls).
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
- **All unit tests share one test binary** (this is a bin crate, not
  lib+bin), so `#[cfg(test)]` modules across `src/*.rs` run in the same
  process. Any test that calls `std::env::set_current_dir` MUST hold
  `crate::CWD_LOCK` (defined in `main.rs`) for the duration, or parallel
  tests race on the process-global cwd.
- **`cargo clippy` errors with E0514 (crate compiled by incompatible
  rustc)** in this environment — a toolchain/rustup mismatch between
  `cargo build`'s and clippy's driver, not a real code issue. Don't chase
  it; `cargo build`/`cargo test` are the real signal here.
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

The upstream checkouts under `deps/` (see `deps/deps.tsv`) are the reference,
not memory. Three things confirmed there and relied on by the templates:

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
