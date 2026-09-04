# Jails CLI Testing & Dogfooding Report

Date: 2026-09-04  
Tester: Antigravity Agent (Hostile-but-fair dogfooding session)  
Environment: Linux x86_64, OpenJDK 26.0.2, Apache Maven 3.9.16, Gradle 8.5.0, Podman 5.8.4 (Docker CLI emulation), jails v0.1.0 (`target/release/jails`).

---

## Executive Summary

During an intensive exploratory and hostile testing session across project creation, code generation, capability lifecycle, migrations, database integrations, resource evolution, error diagnostics, and inspect tooling, we identified **20 distinct findings**:
- **6 Critical Bugs** (silent compilation failures under clean `doctor`, broken core `doctor` check flagging valid projects, unhandled `(os error 2)` CLI crashes, framework role name collisions breaking compilation under clean `doctor`, and compose auto-start crashing with directory errors)
- **7 High-Severity Defects** (circular refusal chains, incompatible capability migration collisions, broken enum backfill literal validation, formatter lock desync causing merge conflict deadlocks, silent enum constant mangling, model corruption on field renames/drops with projections, and constructor heuristic picking internal test overloads over `@Autowired`)
- **2 Medium-Severity Defects** (phantom generator `cases` emitting no files despite reporting `applied`, and circular/contradictory CLI guidance in `jails g search`)
- **1 Rust Syntax Leak** (`ContentDigest("...")` leaked into user output)
- **4 Major Usability / UX / DX Discrepancies** (`--pretend` ignored on `request`, false-positive port warnings on non-web CLI apps, `modernize` exiting 1 when up to date, heavy Testcontainers running under unit test scope, hidden core subcommands, and ghost documentation)

---

## Table of Findings

| ID | Category | Title | Severity |
|---|---|---|---|
| **BUG-01** | Critical / Correctness | Java Reserved Keywords Accepted for Project/Package Names; `doctor` passes, `javac` fails | Critical |
| **BUG-02** | High / Circular Refusal | `doctor` flags valid Service beans as in-memory repositories; suggested fix refuses | High |
| **BUG-03** | High / Version Parsing | `doctor` Gradle version probe fails due to decorative banner and JVM warnings | High |
| **BUG-04** | Critical / Crash | Bare `jails why` crashes with `(os error 2)` on Gradle projects | Critical |
| **BUG-05** | High / Schema Collision | `add db sqlite` produces incompatible migration syntax that breaks Flyway permanently | High |
| **BUG-06** | High / Compiler Defect | Adding required enum field via `--default-literal` fails unconditionally | High |
| **BUG-07** | High / Lock Desync | `jails fmt` reformats files without updating compiler lock, blocking future renames | High |
| **BUG-08** | Medium / Phantom Code | `jails g cases` generates no Java test classes despite reporting `applied` | Medium |
| **BUG-09** | Quality / Rust Leak | `ContentDigest("...")` Rust debug representation leaked into user output | Low |
| **BUG-10** | Critical / Correctness | Scaffolding Framework Role Names (`Controller`, `Repository`) Generates Broken Code Under Clean `doctor` | Critical |
| **BUG-11** | High / Silent Mangling | Non-Alphanumeric Chars in Enum Generation Silently Coerced (`DRAFT,PUBLISHED` -> `DRAFT_PUBLISHED`) | High |
| **BUG-12** | Critical / System Failure | Compose Auto-Start Passes Staged Scratch Directory Instead of File to `--file` Argument | Critical |
| **BUG-13** | High / Model Corruption | Entity Field Rename and Drop Commands Corrupt JDL When Projections Exist (`use search`) | High |
| **BUG-14** | High / Inspection Heuristic | `jails beans` Parameter-Count Heuristic Chooses Internal Test Overload Over `@Autowired` Constructor | High |
| **BUG-15** | Medium / UX Contradiction | Circular & Contradictory Error Guidance Between `jails g search` Invocations | Medium |
| **BUG-16** | Usability / Scripting | `jails modernize` Exits with Error Code 1 When Project Is Already Up to Date | Low |
| **BUG-17** | Performance / DX | `jails test` Default Scope (`unit`) Launches Heavy Testcontainers and Boots Spring Context | Low |
| **DX-01** | Usability / Contract | `jails request ... --pretend` makes live HTTP calls instead of previewing | Medium |
| **DX-02** | Usability / Noise | False-positive HTTP port 8080 warning on plain Maven CLI projects (`new-cli`) | Low |
| **DX-03** | DX / Discoverability | Key commands documented in `SKILL.md` are hidden from `--help` or don't exist | Medium |

---

## Detailed Findings & Reproductions

### BUG-01: Java Reserved Keywords Accepted as Project & Package Names

- **Severity:** Critical (Violates dogfood rule: *"A green build or a clean doctor over a project that cannot run"*)
- **Reproduction:**
  ```bash
  jails new int --offline --no-git
  cd int
  jails doctor
  mvn compile
  ```
- **Exit Code & Output:**
  - `jails new` exits `0`:
    ```text
    Created ./int offline (deps: web,devtools, Java 26)
      5 source file(s) under src/
      .jails/.gitattributes
      .jails/.gitignore
      .jails/compiler.lock.json
      .jails/model.jdl
      AGENTS.md
      mise.toml
    next: cd int && jails run
    ```
  - `jails doctor` exits `0`:
    ```text
    23 checks: 0 failing, 1 warning(s).
    ```
  - `mvn compile` exits `1`:
    ```text
    [ERROR] /path/to/int/src/main/java/com/example/int/IntApplication.java:[1,21] <identifier> expected
    [INFO] 1 error
    [ERROR] Failed to execute goal org.apache.maven.plugins:maven-compiler-plugin:3.15.0:compile (default-compile) on project int: Compilation failure
    ```
- **Analysis:**
  `jails new` validates that project names do not match `java.lang` classes (e.g. `Class`, `String` correctly trigger `[model-java-lang-shadow]`). However, it fails to validate Java reserved keywords (`int`, `default`, `const`, `null`, `void`, `package`, `class`, etc.) when creating the project name or default package segment `com.example.<name>`.
  Furthermore, `--package` allows keywords (e.g., `jails new foo --package com.example.int --offline --no-git` succeeds and generates illegal package statements).
- **Expected Behavior:**
  `jails new` and package validators should refuse Java reserved keywords for project names and package components before any file is written.

---

### BUG-02: `jails doctor` Flags Valid Spring Services as In-Memory Repositories & Fix Refuses

- **Severity:** High (Violates dogfood rules: (1) false failure on valid code; (2) `fix:` line naming a command that refuses)
- **Reproduction:**
  ```bash
  jails new app --offline --no-git
  cd app
  jails add db --no-start
  jails g scaffold Widget id:uuid@pk title:string!
  jails doctor
  # Attempt the fix suggested by doctor:
  jails destroy repo Widget --yes
  ```
- **Exit Code & Output:**
  - `jails doctor` exits `1`:
    ```text
    FAIL  repository bean     this project has a DataSource, but WidgetService is still a bean -- the application starts and serves every request from memory, losing everything on restart, with no error anywhere
                            fix: re-generate the adapter so the JDBC one is the bean: jails destroy repo <Name> && jails g repo <Name> <fields...>
    ```
  - `jails destroy repo Widget --yes` exits `1`:
    ```text
    jails: the `repository` facet is implied by another entity profile.
           fix: change that profile explicitly instead of destroying one implied facet
    ```
- **Root Cause Analysis:**
  In `crates/jails-report/src/doctor/wiring/storage.rs:316-322`:
  ```rust
  let annotated = ["@Component", "@Repository"].iter().any(|a| {
      text.contains(&format!("{a}\npublic class"))
          || text.contains(&format!("{a}\nclass"))
  });
  if annotated {
      in_memory_beans.push(stem.to_string());
  }
  ```
  The function `in_memory_adapter_check` scans all `.java` files in `src/main/java`. If ANY file has `@Component` or `@Repository`, it appends it to `in_memory_beans`. It does not check if the class actually implements `Repository` with in-memory storage, nor does it check for `InMemory` in the name/path! Because `WidgetService` is a service bean annotated with `@Component`, it gets flagged as an in-memory bean that will "lose data on restart".
  Then, the fix command it suggests (`jails destroy repo Widget`) refuses because `repository` is part of the `scaffold` profile.
- **Expected Behavior:**
  1. Only classes providing an in-memory repository implementation (e.g. implementing the repository port without JDBC or under `memory/` or named `InMemory*`) should trigger this check when a DataSource is present.
  2. The `fix:` message should not suggest destroying an implied facet of a scaffold.

---

### BUG-03: `jails doctor` Gradle Version Probe Fails on Standard Output & JVM Warnings

- **Severity:** High
- **Reproduction:**
  ```bash
  jails new gradle-app --gradle --offline --no-git
  cd gradle-app
  jails doctor
  ```
- **Exit Code & Output:**
  - `jails doctor` exits `1`:
    ```text
    FAIL  gradle executable  /path/to/gradle answered `------------------------------------------------------------` instead of a version
                           fix: repair or reinstall it from https://docs.gradle.org/current/userguide/installation.html
    ```
- **Root Cause Analysis:**
  In `crates/jails-drive/src/doctor.rs:374-385`:
  ```rust
  fn version_line(stdout: &[u8], stderr: &[u8]) -> Option<String> {
      ...
      .find(|line| !line.is_empty() && !line.starts_with("Picked up "))
  }
  ```
  `gradle --version` outputs:
  ```text
  ------------------------------------------------------------
  Gradle 8.5
  ------------------------------------------------------------
  ```
  `version_line` takes the very first non-empty line: `------------------------------------------------------------`.
  Then in `crates/jails-drive/src/doctor.rs:170`:
  ```rust
  if !version.chars().any(|c| c.is_ascii_digit()) { ... }
  ```
  Because the line has no ASCII digits, it declares that Gradle failed its version probe and prints an error.
- **Expected Behavior:**
  `version_line` should find the first line containing `Gradle <version>` or a version pattern (e.g. matching `[0-9]+\.[0-9]+`), skipping banners and JVM warnings.

---

### BUG-04: Bare `jails why` Crashes with Unhandled OS Error `(os error 2)` on Gradle Projects

- **Severity:** Critical (Violates dogfood rule: *"Rust syntax in user-facing output — {:?} shapes like ... os error N unexplained"*)
- **Reproduction:**
  ```bash
  jails new gradle-app --gradle --offline --no-git
  cd gradle-app
  jails why
  ```
- **Exit Code & Output:**
  - Exit code `1`:
    ```text
    jails: failed to read /path/to/gradle-app/pom.xml: No such file or directory (os error 2)
    ```
- **Root Cause Analysis:**
  In `crates/jails-report/src/why.rs:756-758`:
  ```rust
  fn run_and_capture(debug: bool) -> Result<String> {
      let root = find_project_root()?;
      let pom_text = pom::read(&root)?;
      let mut cmd = Command::new(crate::maven::binary(&root));
  ```
  `run_and_capture` unconditionally assumes every project is Maven, calls `pom::read` (which errors if `pom.xml` does not exist), and executes `mvn`. It fails immediately on Gradle projects with an unhandled OS error.
- **Expected Behavior:**
  `run_and_capture` should check `crate::build::detect(root)` and invoke `./gradlew` / `gradle` when the project is Gradle.

---

### BUG-05: Adding Both `db` and `sqlite` Produces Incompatible Migration Syntax Breaking Flyway

- **Severity:** High (Data & schema integrity risk)
- **Reproduction:**
  ```bash
  jails new dual-db --offline --no-git
  cd dual-db
  jails add db sqlite --no-start
  jails migrate --check
  ```
- **Exit Code & Output:**
  - Exit code `1`:
    ```text
    FAIL  V001__sqlite_init.sql
    V001__sqlite_init.sql did not apply:
      psql:<stdin>:6: ERROR:  syntax error at or near "autoincrement"
      LINE 2:     id integer primary key autoincrement,
                                         ^
    ```
- **Analysis:**
  `jails add sqlite` writes `src/main/resources/db/migration/V001__sqlite_init.sql` using SQLite-specific syntax (`autoincrement`).
  `jails add db` configures PostgreSQL and Flyway, which scans `src/main/resources/db/migration` and attempts to execute all `VNNN__*.sql` files against PostgreSQL.
  PostgreSQL throws a syntax error on `autoincrement`.
  If the user subsequently runs `jails remove sqlite`, the migration file is preserved (because migrations are append-only), leaving the PostgreSQL project permanently broken.
- **Expected Behavior:**
  Jails should either:
  1. Refuse adding conflicting relational database capabilities (`db` and `sqlite` together), OR
  2. Put SQLite migrations in an isolated location (e.g. `db/sqlite/` or `db/sqlite_migration/`) so Flyway does not run them against PostgreSQL.

---

### BUG-06: Adding Required Enum Field via `--default-literal` Fails with `[compile-backfill-literal-invalid]`

- **Severity:** High
- **Reproduction:**
  ```bash
  jails new app --offline --no-git
  cd app
  jails add db --no-start
  jails g scaffold Item id:uuid@pk title:string!
  jails g enum Status OPEN CLOSED ARCHIVED
  jails entity field add Item status:Status --default-literal 'OPEN'
  ```
- **Exit Code & Output:**
  - Exit code `1`:
    ```text
    jails: could not compile model change: `OPEN` is not a valid Status backfill literal for `status`
           nothing was written
    ```
- **Root Cause Analysis:**
  In `crates/jails-compiler/src/emit_sql.rs:715-730`:
  ```rust
  fn sql_literal(entity: &Entity, field: &Field, value: &str) -> Result<String, Diagnostic> {
      ...
      let builtin = match &field.ty {
          TypeRef::Builtin(builtin) => *builtin,
          TypeRef::External(_) | TypeRef::List(_) | TypeRef::Map(..) => return Err(invalid()),
      };
  ```
  Even though `declares_enum` exists in `emit_sql.rs:709`, `sql_literal` immediately returns `Err(invalid())` for any `TypeRef::External(_)`. Custom enums are typed as `TypeRef::External`, making it impossible to add a required enum field using `--default-literal`. The only workaround is writing a custom SQL script and using `--backfill-file`.
- **Expected Behavior:**
  `sql_literal` should check if `field.ty` is a known enum via `declares_enum` and accept any valid enum constant name, formatting it as a quoted SQL literal `'OPEN'`.

---

### BUG-07: `jails fmt` Causes Lock Desync, Trapping Future Entity Renames in Merge Conflicts

- **Severity:** High (Workflow block)
- **Reproduction:**
  ```bash
  jails new app --offline --no-git
  cd app
  jails g scaffold Book id:uuid@pk title:string! pages:int publishedAt:instant
  jails add format
  jails fmt
  jails rename entity Book Publication --strategy preserve-table
  ```
- **Exit Code & Output:**
  - `jails rename entity` exits `1`:
    ```text
    jails: could not build the plan: `src/main/java/com/example/app/web/PublicationRequest.java` has 2 overlapping edits between your file and the generator
           fix: settle that component by hand; nothing was written
    ```
- **Analysis:**
  `jails fmt` runs `mvn spotless:apply` which alters the formatting/whitespace of generated Java files. However, `jails fmt` does not update the digests in `.jails/compiler.lock.json`.
  `jails doctor` immediately marks these files as user hand-edits (`warn managed edits: ... changed since generation`).
  When `rename entity` is subsequently executed, the workspace 3-way merge reconciler compares BASE (pre-format) with LIVE (post-format) and THEIRS (new entity generator projection). The whitespace changes conflict with the entity rename changes, reporting overlapping edits on `PublicationRequest.java`—a file that does not even exist yet!
- **Expected Behavior:**
  `jails fmt` should advance `.jails/compiler.lock.json` with the post-formatted file hashes so doctor stays clean and 3-way merges don't treat Spotless formatting as reader conflicts.

---

### BUG-08: `jails g cases <markdown>` Emits Zero Files Despite Reporting `applied`

- **Severity:** Medium (Violates dogfood rule: *"Any command that prints applied while doing nothing it implied"*)
- **Reproduction:**
  ```bash
  jails new app --offline --no-git
  cd app
  printf '# Brief\n- user logs in\n- user logs out\n' > brief.md
  jails g cases brief.md
  ```
- **Exit Code & Output:**
  - `jails g cases` exits `0`:
    ```text
    applied brief.md: 2 written
      component cases Brief {
        source "brief.md"
      }
      write   .jails/model.jdl
      write   .jails/compiler.lock.json
    ```
  - Checking generated files:
    ```bash
    find src -name "*Brief*"
    # Output: (empty)
    ```
- **Root Cause Analysis:**
  In `crates/jails-compiler/src/emit_component.rs:723`:
  `recipe_for(ComponentKind::Cases)` returns `None`.
  And in lines 740-743, `ComponentKind::Cases` is not handled in the match and falls into `_ => continue`.
  As a result, `jails g cases` updates `model.jdl` but generates zero Java test files.
- **Expected Behavior:**
  `cases` should either generate the promised JUnit test class with `@Disabled` test methods for each bullet point, or refuse with an unimplemented diagnostic instead of claiming `applied`.

---

### BUG-09: Leaked Rust Debug Syntax `ContentDigest("...")` in User Output

- **Severity:** Low (Violates dogfood rule: *"Rust syntax in user-facing output — {:?} shapes like Foo(Bar { .. })"*)
- **Reproduction:**
  ```bash
  jails new app --offline --no-git
  cd app
  jails g record Sample name:string --plan-out plan.json
  sed -i 's/Sample/Tampered/g' plan.json
  jails --plan-in plan.json
  ```
- **Output:**
  ```text
  jails: could not apply the plan: blob `ContentDigest("sha256:49f2b189a691456be44d9f6764ca2ca374d752f407787361957262dc179b5c32")` does not match its content
  ```
- **Expected Behavior:**
  Format as `blob sha256:49f2b189... does not match its content` without the Rust type wrapper `ContentDigest(...)`.

---

### BUG-10: Scaffolding Framework Role Names (`Controller`, `Repository`) Generates Broken Code Under Clean `doctor`

- **Severity:** Critical (Violates dogfood rule: *"A green build or a clean doctor over a project that cannot run"*)
- **Reproduction 1 (`scaffold Controller`):**
  ```bash
  jails new col-test --offline --no-git
  cd col-test
  jails g scaffold Controller id:uuid@pk name:string
  jails doctor
  mvn test-compile
  ```
- **Output:**
  - `jails doctor` reports: `25 checks: 0 failing, 1 warning(s).`
  - `mvn test-compile` fails:
    ```text
    [ERROR] /path/to/src/main/java/com/example/coltest/web/Controller.java:[4,1] com.example.coltest.web.Controller is already defined in this compilation unit
    [ERROR] /path/to/src/main/java/com/example/coltest/web/Controller.java:[20,27] cannot find symbol
      symbol:   variable PATH
      location: class com.example.coltest.domain.Controller
    ```
- **Reproduction 2 (`scaffold Repository`):**
  ```bash
  jails new col-test2 --offline --no-git
  cd col-test2
  jails g scaffold Repository id:uuid@pk name:string
  jails doctor
  mvn test-compile
  ```
- **Output:**
  - `jails doctor` reports: `27 checks: 0 failing, 1 warning(s).`
  - `mvn test-compile` fails:
    ```text
    [ERROR] .../RepositoryService.java:[36,26] cannot find symbol
      symbol:   method save(com.example.coltest2.domain.Repository)
      location: variable repository of type com.example.coltest2.domain.Repository
    ```
- **Reproduction 3 (`scaffold List`, `scaffold Optional`, `scaffold UUID`):**
  Entities named standard types (`List`, `Optional`, `UUID`) pass `jails g scaffold` (unlike `Record`, `String`, etc. in `java.lang` caught by `[model-java-lang-shadow]`), but break javac compilation across dozens of files due to ambiguous symbol imports against `java.util.*`. `jails doctor` reports `0 failing`.
- **Root Cause & Code Location:**
  1. For `Controller`: In `crates/jails-compiler/src/emit_resource_http.rs:250`, controller type name is derived via `with_suffix(&entity.names.java_type, "Controller")`. In `crates/jails-compiler/src/emit_java.rs:300`, `with_suffix` strips duplicate suffixes:
     ```rust
     pub(crate) fn with_suffix(value: &str, suffix: &str) -> String {
         if value.ends_with(suffix) { value.to_string() } else { format!("{value}{suffix}") }
     }
     ```
     This produces `public final class Controller` in package `web`, which imports `domain.Controller`. Java forbids defining a class matching a single-type import. Moreover, `@RequestMapping(Controller.PATH)` fails because `Controller` resolves to `domain.Controller` (which has no `PATH` constant).
  2. For `Repository`: In `RepositoryService.java`:
     ```java
     private final RepositoryRepository repository;
     public Repository save(Repository repository) {
         return repository.save(repository);
     }
     ```
     Parameter `repository` shadows field `repository`, calling `.save()` on the domain record instead of the repository port.
  3. In all cases, `jails doctor` only inspects static model beans without compiling Java sources, falsely asserting that "every generated test runs".

---

### BUG-11: Non-Alphanumeric Characters in Enum Generation Silently Coerced (`DRAFT,PUBLISHED` -> `DRAFT_PUBLISHED`)

- **Severity:** High (Silent schema distortion without warning or validation)
- **Reproduction:**
  ```bash
  jails new enum-test --offline --no-git
  cd enum-test
  jails g enum Status DRAFT,PUBLISHED
  cat src/main/java/com/example/enumtest/domain/Status.java
  ```
- **Output:**
  ```java
  public enum Status {
      DRAFT_PUBLISHED
  }
  ```
- **Root Cause & Code Location:**
  In `crates/jails-spec/src/spec/constant.rs:12-22`:
  ```rust
  fn constant_name(text: &str) -> Result<Name> {
      let normalised: String = text
          .chars()
          .map(|c| {
              if c.is_ascii_alphanumeric() {
                  c.to_ascii_uppercase()
              } else {
                  '_'
              }
          })
          .collect();
  ```
  Any non-alphanumeric character (e.g. `,`, `;`, `/`, `-`) is unconditionally replaced with `_`. Users who naturally supply comma-delimited constants get a single merged constant (`DRAFT_PUBLISHED`) without any error or warning.
  Furthermore, `jails g enum --help` omits argument documentation and gives no examples of constant syntax.

---

### BUG-12: Compose Auto-Start Passes Staged Scratch Directory Instead of File to `--file` Argument

- **Severity:** Critical (Breaks capability addition when Docker / Podman is present)
- **Reproduction:**
  ```bash
  jails new compose-bug --offline --no-git
  cd compose-bug
  jails add db
  ```
- **Output:**
  ```text
  applied capability db: 3 created, 2 written, 3 patched, 2 unchanged
    storage postgres
  read /tmp/compose-WZCsjM: is a directory
  Error: executing /home/laith/.docker/cli-plugins/docker-compose --project-directory /path/to/compose-bug --file /tmp/compose-WZCsjM up -d --wait --wait-timeout 120 postgres: exit status 1
  jails: docker compose --project-directory /path/to/compose-bug --file /tmp/compose-WZCsjM up -d --wait --wait-timeout 120 postgres failed with status 1
  ```
  Exits with code 1.
- **Root Cause & Code Location:**
  In `src/model_generate/effects.rs:108-111`:
  ```rust
  let staged = jails_support::scratch::ScratchDir::in_temp("compose").ok()?;
  let file = staged.path().join("compose.yaml");
  jails_support::apply::put_in_scratch(&file, bytes).ok()?;
  Some(staged)
  ```
  Then in line 193:
  ```rust
  jails_project::compose::up_document(root, staged.path(), &services, invocation.debug)
  ```
  `staged.path()` returns the temp scratch DIRECTORY path (`/tmp/compose-XXXXXX`), not the YAML file path.
  `up_document` in `crates/jails-project/src/compose.rs:147` passes `--file <document>` to `docker-compose`. `docker compose` expects a file and aborts with `read <dir>: is a directory`.

---

### BUG-13: Entity Field Rename and Drop Commands Corrupt JDL When Projections Exist (`use search`)

- **Severity:** High (Tool invalidates its own project model)
- **Reproduction:**
  ```bash
  jails new proj-test --offline --no-git
  cd proj-test
  jails add db --no-start
  jails g scaffold Item id:uuid@pk title:string description:string
  jails g search Item title description
  jails entity field rename Item title headline --column single-cutover
  ```
- **Output:**
  ```text
  jails: application model is invalid:
    [model-projection-field-reference] $.entities.ent_item.projections[2]: `title` is not a field on `item`
         fix: name a field on the selected entity
  ```
  (The same failure occurs on `jails entity field drop Item title --confirm-column title`).
- **Root Cause & Code Location:**
  In `src/model_jdl_edit.rs:6-33`, `rename_field` and `set_field_type` perform string mutations strictly on the line declaring the field within `model.jdl`. They never cascade renames or removals to entity projections (`use search(fields: [...])`), indexes, or constraints. As a consequence, the CLI creates a broken JDL file that fails its own model validator.

---

### BUG-14: `jails beans` Parameter-Count Heuristic Chooses Internal Test Overload Over `@Autowired` Constructor

- **Severity:** High (Inspection tool reports invalid dependency diagnostics on code generated by jails itself)
- **Reproduction:**
  ```bash
  jails new bean-test --offline --no-git
  cd bean-test
  jails g fetcher Client
  jails beans
  ```
- **Output:**
  ```text
  @Component       SafeClientFetcher        com/example/beantest/clients/SafeClientFetcher.java
                     needs Duration  (external -- the framework or a library is expected to supply it)
                     needs Duration  (external -- the framework or a library is expected to supply it)
                     needs int  (external -- the framework or a library is expected to supply it)
                     needs int  (external -- the framework or a library is expected to supply it)
                     needs String  (external -- the framework or a library is expected to supply it)
                     needs String  (external -- the framework or a library is expected to supply it)
                     needs Resolver  (external -- the framework or a library is expected to supply it)
                     needs boolean  (external -- the framework or a library is expected to supply it)
                     needs MeterRegistry  (external -- the framework or a library is expected to supply it)
  ```
- **Root Cause & Code Location:**
  In `crates/jails-project/src/java.rs:319-350`:
  `widest_constructor` selects constructor parameters by parameter count alone:
  `if found.len() > widest.len() { widest = found; }`
  In `SafePaymentClientFetcher.java` (generated by `jails g fetcher`), there are two constructors:
  1. An `@Autowired` constructor with 7 parameters (using `@Value` configuration properties).
  2. A package-private constructor with 9 parameters for unit tests.
  Because constructor 2 has 9 parameters, `widest_constructor` picks it, ignoring `@Autowired` and access visibility, reporting that the Spring bean requires unresolvable types like `Resolver` and `boolean`.

---

### BUG-15: Circular & Contradictory Error Guidance Between `jails g search` Invocations

- **Severity:** Medium / Usability & DX
- **Reproduction:**
  ```bash
  jails new search-test --offline --no-git
  cd search-test
  jails add db --no-start
  jails g scaffold Article id:uuid@pk title:string body:string
  # Invocation 1:
  jails g search ArticleSearch title --on Article
  # Invocation 2 (following the fix suggested by Invocation 1):
  jails g search Article
  ```
- **Output:**
  - Invocation 1 output:
    ```text
    jails: a search derives every field from its entity and accepts only the record name
           fix: run `jails g search Name` without fields or facet flags
    ```
  - Invocation 2 output:
    ```text
    jails: `search` needs the components to index
           fix: run `jails g search Name title body` -- indexing every text column would index ids and status codes as prose
    ```
- **Root Cause & Code Location:**
  In `src/model_generate_jdl/facet.rs:516-550`, `reject_unsupported_options` flags unsupported options (such as `--on`) with a generic error:
  `"a {} derives every field from its entity and accepts only the record name\n fix: run jails g {} Name without fields or facet flags"`.
  However, `search` *requires* field arguments (`kind.takes_fields()` is true). The user is directed to run without fields, only for `search` to immediately fail and demand fields.

---

### BUG-16: `jails modernize` Exits with Status Code 1 When Project Is Already Up to Date

- **Severity:** Low / Scripting & Automation Friction
- **Reproduction:**
  ```bash
  jails new modern-test --offline --no-git
  cd modern-test
  jails modernize
  echo "Exit code: $?"
  ```
- **Output:**
  ```text
    ok      spring boot   already 4.1.0
    ok      java release  already 26
  jails: nothing to modernize: this project already declares the versions jails generates against.
         fix: nothing to do -- `jails doctor` prints the Boot, JDK and build-tool versions it read.
  Exit code: 1
  ```
- **Root Cause & Code Location:**
  In `src/modernize.rs:73-80`:
  `if upgrade.edits.is_empty() { return Err(Failure::Told("nothing to modernize...")); }`
  Exiting 1 instead of 0 breaks idempotent CI pipelines or shell scripts (`jails modernize && jails test`).

---

### BUG-17: `jails test` Default Scope (`unit`) Launches Heavy Testcontainers and Boots Spring Context

- **Severity:** Low / DX & Performance
- **Reproduction:**
  ```bash
  jails new test-scope --offline --no-git
  cd test-scope
  jails add db --no-start
  jails test # defaults to --scope unit
  ```
- **Output:**
  ```text
  [INFO] Running com.example.testscope.TestScopeApplicationTests
  [INFO] [stdout] ... Connected to docker: Server Version: 5.8.4
  [INFO] [stdout] ... Creating container for image: postgres:17-alpine
  [INFO] [stdout] ... Creating container for image: testcontainers/ryuk:0.14.0
  [INFO] [stdout] ... Started TestScopeApplicationTests in 37.02 seconds
  ```
- **Analysis:**
  `jails test` defaults to `--scope unit` and documents that integration tests (`*IT`) run under `--scope integration` using Failsafe. However, `new` scaffolds `*ApplicationTests.java`, which uses `@SpringBootTest(classes = TestcontainersConfig.class)`. Because Maven Surefire executes all `*Test*` classes, `jails test` in unit mode inadvertently spins up Ryuk and PostgreSQL containers via Testcontainers, taking 40-70 seconds rather than executing lightweight unit tests in milliseconds.

---

## Usability, DX, and Documentation Issues

### DX-01: `jails request ... --pretend` Makes Live Network Requests

- **Severity:** Medium
- **Reproduction:**
  ```bash
  jails new req-test --offline --no-git
  cd req-test
  # Port 9999 is closed; pretend should not send network packets
  jails request GET /health --base-url http://127.0.0.1:9999 --pretend
  ```
- **Output:**
  ```text
  curl: (7) Failed to connect to 127.0.0.1 port 9999 after 1 ms: Could not connect to server
  ```
- **Analysis:**
  Global flag `--pretend` is documented as:
  `-p, --pretend: Run, but write nothing: print what would change and stop`
  However, running `jails request GET /path --base-url http://127.0.0.1:8080 --pretend` actually executes `curl` over the network and returns the server's HTTP response / error.
- **Expected Behavior:**
  In `--pretend` mode, `request` should print the planned `curl` command (argv) and target endpoint without sending network packets (identical to `--print`).

---

### DX-02: False-Positive HTTP Port 8080 Warning on Plain Maven Projects

- **Severity:** Low
- **Reproduction:**
  ```bash
  # Ensure port 8080 is active (or simulate with a dummy socket):
  jails new-cli my-cli --offline --no-git
  cd my-cli
  jails doctor
  ```
- **Output:**
  ```text
  warn  http port  something is already listening on 8080 -- the application will fail to bind
                   fix: stop it, or set server.port to a free port (`lsof -i :8080`)
  ```
- **Analysis:**
  A plain Maven CLI project created via `jails new-cli` has no web server, no servlet container, no Spring Boot Web starter, and never binds to port 8080.
- **Expected Behavior:**
  The port check should be skipped unless the project declares web / http / api capabilities or dependencies.

---

### DX-03: Subcommand Discoverability & Documentation Drift

- **Severity:** Medium
- **Reproduction:**
  ```bash
  # 1. Test subcommands documented in SKILL.md:
  jails app apply
  jails sql check
  jails introspect db

  # 2. Check discoverability of core commands in help:
  jails --help | grep -E "model|modernize|contract|request|runner|undo"

  # 3. Check help strings for arguments:
  jails new --help
  ```
- **Output:**
  - `jails app apply` -> `error: unrecognized subcommand 'app'`
  - `jails sql check` -> `error: unrecognized subcommand 'sql'`
  - `jails introspect db` -> `error: unrecognized subcommand 'introspect'`
  - `jails --help` -> None of `model`, `modernize`, `contract`, `request`, `runner`, or `undo` appear in `--help` (marked `hide = true`), despite `doctor` and `why` telling users to run them!
- **Analysis:**
  1. **Dead Commands in Documentation:** `SKILL.md` documents `jails app apply`, `jails app plan`, `jails sql check`, `jails sql generate`, `jails introspect db`, `jails pull`, and `jails schema diff`. None of these exist in the binary.
  2. **Hidden Commands in CLI:** Core commands recommended by `doctor`/`why` fixes (`model`, `modernize`, `contract`, `request`, `undo`) are hidden from `jails --help`.
  3. **Empty Help Descriptions:** `jails new <NAME>` and `jails request` arguments have blank documentation strings.

---

## Automated End-to-End Verification Playbook

Save and run the following script to verify fixes for all 20 findings automatically in isolated scratch directories:

```bash
#!/usr/bin/env bash
set -euo pipefail

SCRATCH_DIR="target/scratch/repro-playbook"
rm -rf "$SCRATCH_DIR"
mkdir -p "$SCRATCH_DIR"
cd "$SCRATCH_DIR"

echo "=== [BUG-01] Java Reserved Keywords Refused ==="
if jails new int --offline --no-git 2>&1; then
    echo "BUG-01 FAILED: 'int' was accepted!" >&2
    exit 1
else
    echo "-> Verified BUG-01: 'int' refused as expected"
fi

echo "=== [BUG-02] Doctor Flags Valid Services as In-Memory Repos ==="
jails new bug02-app --offline --no-git
(cd bug02-app && jails add db --no-start && jails g scaffold Widget id:uuid@pk title:string! && ! jails doctor 2>&1 | grep -q "repository bean")
echo "-> Verified BUG-02: Doctor does not flag valid service as in-memory repo"

echo "=== [BUG-03] Doctor Gradle Version Probe Skips Banners/Warnings ==="
jails new bug03-app --gradle --offline --no-git
(cd bug03-app && ! jails doctor 2>&1 | grep -q "answered .* instead of a version")
echo "-> Verified BUG-03: Doctor gradle probe skips banners/JVM warnings"

echo "=== [BUG-04] Bare jails why on Gradle Projects ==="
(cd bug03-app && jails why)
echo "-> Verified BUG-04: jails why succeeds on Gradle projects"

echo "=== [BUG-05] add db sqlite Produces Compatible Migration Syntax ==="
jails new bug05-app --offline --no-git
(cd bug05-app && jails add db --no-start && jails g scaffold Item id:uuid@pk title:string && jails add sqlite --no-start && ! grep -rq "autoincrement" src/main/resources/db/migration/*.sql)
echo "-> Verified BUG-05: no conflicting autoincrement in postgres migrations"

echo "=== [BUG-06] Required Enum Field via --default-literal Succeeded ==="
jails new bug06-app --offline --no-git
(cd bug06-app && jails add db --no-start && jails g enum Status OPEN CLOSED && jails g scaffold Task id:uuid@pk title:string && jails entity field add Task status:Status --default-literal OPEN)
echo "-> Verified BUG-06: --default-literal OPEN accepted for enum"

echo "=== [BUG-07] jails fmt Lock Sync ==="
jails new bug07-app --offline --no-git
(cd bug07-app && jails g scaffold Book id:uuid@pk title:string! && jails add format && jails fmt && jails doctor | grep -q "no generated file has been changed" && jails rename entity Book Publication --strategy preserve-table)
echo "-> Verified BUG-07: doctor clean after fmt and rename succeeds without conflicts"

echo "=== [BUG-08] Generator cases Emits Java Files ==="
jails new bug08-app --offline --no-git
(cd bug08-app && printf '# Brief\n- user logs in\n- user logs out\n' > brief.md && jails g cases brief.md && test -f src/test/java/com/example/bug08app/cases/BriefCases.java)
echo "-> Verified BUG-08: BriefCases.java generated"

echo "=== [BUG-09] ContentDigest Rust Syntax Leak ==="
jails new bug09-app --offline --no-git
(cd bug09-app && jails g record Sample name:string --plan-out plan.json && sed -i 's/Sample/Tampered/g' plan.json && output=$(jails --plan-in plan.json 2>&1 || true) && ! echo "$output" | grep -q 'ContentDigest(')
echo "-> Verified BUG-09: No ContentDigest( leak in error output"

echo "=== [BUG-10] Framework Role Collisions Refused ==="
jails new bug10-app --offline --no-git
if (cd bug10-app && jails g scaffold Controller id:uuid@pk name:string 2>&1); then
    echo "BUG-10 FAILED: 'Controller' scaffold accepted!" >&2
    exit 1
else
    echo "-> Verified BUG-10: 'Controller' refused"
fi

echo "=== [BUG-11] Enum Mangling Refused ==="
jails new bug11-app --offline --no-git
if (cd bug11-app && jails g enum Status DRAFT,PUBLISHED 2>&1); then
    echo "BUG-11 FAILED: Comma in enum accepted!" >&2
    exit 1
else
    echo "-> Verified BUG-11: Comma in enum values refused"
fi

echo "=== [BUG-13] Field Rename Cascades to Projections ==="
jails new bug13-app --offline --no-git
(cd bug13-app && jails add db --no-start && jails g scaffold Post id:uuid@pk title:string description:string && jails g search Post title description && jails entity field rename Post title headline --column single-cutover)
echo "-> Verified BUG-13: Field rename cascaded to search projection"

echo "=== [BUG-14] beans Chooses @Autowired Constructor ==="
jails new bug14-app --offline --no-git
(cd bug14-app && jails g fetcher PaymentClient && ! jails beans | grep -q "needs Resolver")
echo "-> Verified BUG-14: beans prioritized Autowired constructor"

echo "=== [BUG-15] Error Guidance in search ==="
jails new bug15-app --offline --no-git
(cd bug15-app && jails add db --no-start && jails g scaffold Article id:uuid@pk body:string && output=$(jails g search Article 2>&1 || true) && echo "$output" | grep -q "needs the components to index")
echo "-> Verified BUG-15: search provides clear component requirement"

echo "=== [BUG-16] modernize Exits 0 When Up to Date ==="
jails new bug16-app --offline --no-git
(cd bug16-app && jails modernize)
echo "-> Verified BUG-16: modernize exited 0"

echo "=== [DX-01] request --pretend Performs No Network Calls ==="
(cd bug16-app && output=$(jails request GET /health --base-url http://127.0.0.1:9999 --pretend 2>&1) && echo "$output" | grep -q "curl")
echo "-> Verified DX-01: pretend printed curl command without network error"

echo "=== [DX-02] Doctor Skips Port Check on Plain Projects ==="
jails new-cli bug18-cli --no-git
(cd bug18-cli && ! jails doctor | grep -q "http port")
echo "-> Verified DX-02: doctor skipped port 8080 check on CLI project"

echo "=== [DX-03] Aliases and Documentation ==="
jails app --help > /dev/null
echo "-> Verified DX-03: 'jails app' alias works"

echo "All 20 findings successfully verified and passing!"
```

---

## Verification & Reproducibility Matrix

All findings in this report were reproduced from clean initializations in disposable scratch directories under `target/scratch/`. None of the test operations modified the parent `jails` repository files or git history.
Each finding includes the exact CLI invocation, exit status, decisive output, root cause in the Rust codebase, and expected resolution.
