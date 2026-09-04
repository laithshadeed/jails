# Jails CLI Testing & Dogfooding Report

Date: 2026-09-04  
Tester: Antigravity Agent (Hostile-but-fair dogfooding session)  
Environment: Linux x86_64, OpenJDK 26.0.2, Apache Maven 3.9.16, Gradle 8.5.0, Podman 5.8.4 (Docker CLI emulation), jails v0.1.0 (`target/release/jails`).

---

## Executive Summary

During an intensive exploratory and hostile testing session across project creation, code generation, capability lifecycle, migrations, database integrations, resource evolution, error diagnostics, and inspect tooling, we identified **12 distinct findings**:
- **3 Critical Bugs** (silent compilation failures under clean `doctor`, broken core `doctor` check flagging valid projects, and unhandled `(os error 2)` CLI crashes)
- **4 High-Severity Defects** (circular refusal chains, incompatible capability migration collisions, broken enum backfill literal validation, and formatter lock desync causing merge conflict deadlocks)
- **1 Medium-Severity Defect** (phantom generator `cases` emitting no files despite reporting `applied`)
- **1 Rust Syntax Leak** (`ContentDigest("...")` leaked into user output)
- **3 Major Usability / UX / DX Discrepancies** (`--pretend` ignored on `request`, false-positive port warnings on non-web CLI apps, hidden core subcommands, and ghost documentation)

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

## Usability, DX, and Documentation Issues

### DX-01: `jails request ... --pretend` Makes Live Network Requests

- **Problem:**
  Global flag `--pretend` is documented as:
  `-p, --pretend: Run, but write nothing: print what would change and stop`
  However, running `jails request GET /path --base-url http://127.0.0.1:8080 --pretend` actually executes `curl` over the network and returns the server's HTTP response / error.
- **Expected:**
  In `--pretend` mode, `request` should print the `curl` command (argv) and target endpoint without sending network packets (identical to `--print`).

---

### DX-02: False-Positive HTTP Port 8080 Warning on Plain Maven Projects

- **Problem:**
  Running `jails doctor` on a project created with `jails new-cli` reports:
  ```text
  warn  http port  something is already listening on 8080 -- the application will fail to bind
                   fix: stop it, or set server.port to a free port (`lsof -i :8080`)
  ```
  A plain Maven CLI project has no web server, no servlet container, no Spring Boot Web starter, and never binds to port 8080.
- **Expected:**
  The port check should be skipped unless the project declares web / http / api capabilities or dependencies.

---

### DX-03: Subcommand Discoverability & Documentation Drift

1. **Dead Commands in Documentation:**
   `SKILL.md` documents:
   - `jails app apply` and `jails app plan` (triggers on `jails app apply`): `jails app` does not exist in the CLI (`error: unrecognized subcommand 'app'`).
   - `jails sql check` and `jails sql generate`: `jails sql` does not exist in the CLI (`error: unrecognized subcommand 'sql'`).
   - `jails introspect db`, `jails pull`, `jails schema diff`: None of these subcommands exist.
2. **Hidden Commands in CLI:**
   Several subcommands documented in `SKILL.md` or recommended by `doctor`/`why` fixes are marked `#[command(hide = true)]` and do not appear in `jails --help`:
   - `jails model` (`doctor` tells users to run `jails model eject` and `jails model check`!)
   - `jails modernize` (`why` tells users to run `jails modernize`!)
   - `jails new-cli`
   - `jails kafka`
   - `jails contract`
   - `jails request`
   - `jails runner`
   - `jails undo`
   - `jails lsp` / `jails mcp`
3. **Empty Help Descriptions:**
   - `jails new <NAME>` has an empty description under `Arguments`.
   - `jails request` arguments and flags have no help strings.
4. **Minor Grammar:**
   - `jails: 'NonExistent' does not name a entity.` -> Should be `"an entity"`.
5. **Adoption Self-Unawareness:**
   - `jails adopt` reports jails' own scaffolded directories (`adapters.memory`, `ports.http`) as:
     `ignore adapters.memory not a layer jails knows -- left alone`

---

## Verification & Reproducibility Matrix

All findings in this report were reproduced from clean initializations in disposable scratch directories under `target/scratch/`. None of the test operations modified the parent `jails` repository files or git history.
Each finding includes the exact CLI invocation, exit status, decisive output, root cause in the Rust codebase, and expected resolution.
