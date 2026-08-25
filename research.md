# Research Report: 1000x Developer Experience for `jails`

Date: 2026-08-25
Status: engineering proposal grounded in the current `jails` worktree and upstream implementations

## Research basis and decision frame

This report treats “1000x” as a product direction, not a literal benchmark claim. The measurable objective is to collapse multi-minute, multi-tool workflows into one safe command; keep the common edit/test loop under one second; and make every mutation explainable, previewable, and recoverable.

The research combined:

- the current `jails` Rust workspace and CLI;
- the current `jails.nvim` adapter and a JDK-26-aware Neovim/jdtls configuration used as a downstream acceptance fixture;
- local source checkouts in `deps/`, with implementation-level inspection of Rails generators, Laravel's generator base, Django's migration autodetector, Phoenix context generation, sqlc's compiler pipeline, Quarkus DevServices, Loco migration inference, Wasp's application specification, and PostgREST's schema cache;
- current primary documentation for the JVM, Spring, Quarkus, sqlc, SQLx, Ecto, Django, Rails, Alembic, Prisma, jOOQ, Testcontainers, ArchUnit, FastAPI, Wasp, and JHipster.

The most important baseline finding is that `jails` is already beyond the premise of a greenfield generator. It currently has:

- a field DSL and explicit layer model in `jails-spec`;
- vertical-slice generation, one-field-to-SQL/Java/test projections, fixtures, and raw `JdbcClient` adapters in `jails-generate`;
- pure preparation, three-way merging, one human/JSON result value, and dry-run parity in `jails-prepare`;
- project locking, guarded preimages, a content-addressed object store, write-ahead journals, and crash-recovery roll-forward durability in `jails-commit`;
- canonical requests and provenance in `jails-protocol` and orchestration in `jails-engine`;
- Java source inspection and JVM constant-pool dependency extraction in `jails-java`;
- a resident `testd`, affected-test selection, build/run tools, database and Kafka consoles in `jails-drive`;
- `doctor`, `why`, `routes`, `beans`, `src`, and `explain` diagnostics in `jails-report`;
- a thin Neovim adapter exists, but it still parses human output for created files, duplicates part of CLI completion, synchronously adapts bespoke route/bean JSON in downstream pickers, and relies on separate compiler, jdtls, and save-time javac diagnostic paths.

That changes the recommendation. The right architecture is not a second generator or a more magical runtime. It is to turn the existing canonical request → desired state → prepared bundle → durable commit pipeline into a **local Java workbench compiler**. One typed intent model should project to Java, SQL, tests, fixtures, contracts, documentation, and terminal explanations. Every projection must retain provenance and verification status.

The main codebase-memory index was refreshed in full at generation `2026-08-25T16:52:03Z`; task-directed coverage checks reported no recorded gaps in the implementation paths used here. The `task-service` acceptance worktree had newer local Java edits than its index, so those edits, its migration, manifest/ledger, Git diff, and actual CLI dry-run output were inspected directly. Graph coverage remains a best-effort locator rather than proof of completeness.

### How disagreements were reconciled

This document is a synthesis, not a concatenation. It is governed by:

1. the research brief's product constraints: code generation rather than a shipped runtime, terminal-only interaction, modern Java, explicit ports/adapters and raw JDBC, and safe transactional mutations;
2. facts verified in the current source;
3. measurements with a recorded date, revision, command, and environment;
4. implementation-level findings from local upstream checkouts and current primary documentation;
5. engineering judgment, with unmeasured proposals labelled accordingly.

Historical product notes contribute measurements, defects, portfolio ideas, and unresolved questions, but they are not treated as product policy. Numbers are not silently treated as fresh: they were measured on 2026-08-25, chiefly against `main` or revision `9e5f7e7`, while this synthesis was produced at repository `HEAD` `6e3631a` with an active worktree.

## How coding agents should use this report

Start with Section 7: it is the normative RFC and overrides ambiguous product prose. Select one dependency-ready DX work package from Section 7.13, use Section 9.3 to preserve crate ownership, then read only the relevant pillar and Java blueprint for rationale and output shape. Before editing, confirm current symbols and protocol versions because the repository is evolving faster than this document's line references.

An implementation handoff should report the work-package ID, changed public contracts, protocol/schema migration effect, conformance test IDs, real-route/generated-project evidence, and measured latency when performance is claimed. A green parser unit test alone is never completion evidence.

---

## Section 0: Current Reality, Measured Baseline, and Open Constraints

### 0.1 The multiplier is outside the Rust planning path

The most useful measurement in the research is that `jails` itself is already fast. The following figures came from the dated measurement report and are preserved with their scope; they were not re-run during consolidation:

| Operation | Recorded wall time | Interpretation |
|---|---:|---|
| `jails commands --json` | 41 ms | CLI discovery is not the bottleneck |
| `jails new-cli demo --no-git` | 54 ms | project materialization is already interactive |
| 17-file scaffold with `--pretend` | 58 ms | capture, projection, reconciliation, and reporting are already sub-100 ms |
| same scaffold applied | 130 ms | durable mutation overhead is small |
| `mvn -q test` on that generated project | 2.55 s, compile failure | the Java/build verification loop dominates—and exposed a defect |

The full Rust/generated-project test suite was separately measured at 59.60 seconds wall, 292.91 user CPU-seconds, 57.63 system CPU-seconds, about 634 MiB peak RSS, and 173,537 involuntary context switches. The warm CLI test binary floor was about 38.54 seconds. Three concurrent real Failsafe runs against shared PostgreSQL and Kafka services determine the tail. This points work away from shaving milliseconds from `jails-prepare` and toward Java verification, real-database tests, JVM startup reuse, and safe authoring directions that currently do not exist.

The product-level “1000x” therefore means:

- eliminate a later Maven/Flyway/runtime round trip by proving generated output now;
- retain the warm JVM and select the smallest safe tests automatically;
- accept existing schemas and hand-written SQL as inputs instead of making users retype them;
- collapse multi-command setup into one explicit, observable CLI action;
- preserve the existing mutation safety while showing the actual bytes and semantic reasons.

### 0.2 Three immediate defects take priority over new features

The measurement work found two defects that remain visible in the current source and should precede the roadmap:

1. `crates/jails-generate/src/generate/scaffold.rs` still emits an unconditional `eprintln!("PROBE …")` for every scaffold field. Delete it and add an architecture/test gate against stray `println!`, `eprintln!`, and `dbg!` outside deliberate reporting modules.
2. A scaffold generated into a plain Maven `new-cli` project writes Spring MVC imports without ensuring Spring is present. A golden snapshot pins the bytes, but no tier-3 compile gate proves that shape. The preferred fix is a framework-free handler/controller projection for plain projects; the acceptable smaller fix is an explicit refusal. Merely warning while emitting uncompilable Java is not acceptable.
3. Directory reporting currently overstates work. `parents(...)` derives every ancestor of each created file and emits `DirectoryOp::Create` without distinguishing a directory that already exists; the report maps every such operation to human `mkdir`/JSON `create_directory`. Directory creation may remain an idempotent executor prerequisite, but it is not a user-visible change unless observation proved the path absent. Capture each candidate parent as missing/directory/non-directory, emit `mkdir` only for missing, refuse a file/symlink collision, and hold preview/receipt parity with nested-existing-directory fixtures.

This is a concrete Pillar-2 lesson: byte snapshots prove deterministic output, not compilability. Every flagship generator shape needs a real `javac`/Maven or Gradle gate in addition to golden output.

### 0.3 Current compatibility and implementation facts

- The canonical new-project target is JDK 26 in `jails-project::pom::TARGET_RELEASE` for Maven and Gradle generation. Java 21 remains the compatibility floor for adopted projects, whose configured release is preserved unless the user explicitly requests an upgrade.
- Spring Boot 2.7 generation exists but four shapes deliberately refuse or are imprecise: `add api`, `add security`, `g query`, and `g transition`. `JdbcClient` requires Spring Framework 6.1 / Boot 3.2, while current detection deliberately sees only a Boot major: refusing all Boot 3 would reject supported 3.2+ projects, but accepting the whole major leaves 3.0/3.1 with a compiler diagnostic. Feature-specific version capability detection may replace this trade only with minor-version fixtures for both sides of the boundary.
- Maven remains the default and the complete path. Gradle project creation works, but transactional formatting, `testd`, `test --fast`, `test --affected`, and `jails console` remain Maven-only because they need a resolved classpath or a hermetic wrapper arrangement. Silent or partial Gradle claims remain forbidden.
- The three test entry points do not currently share one execution policy. `jails test` runs Maven/Gradle; Maven `test --fast` starts a fresh `java` ConsoleLauncher only when classes are current and falls back for JSON, slowest, fail-fast, stale output, or an unresolved filter; Gradle refuses the flag. `testd` is Maven-only, starts a resident JVM, refuses stale classes instead of compiling, and has a separate selector/status command surface. This explains why the dependable habit collapses back to plain `jails test`.
- Application startup also repeats avoidable work. Current `run`/`run --watch` calls Compose startup first; Maven uses `spring-boot:run`; Maven watch polls at 750 ms and launches `mvn compile` after a source fingerprint change; Gradle uses continuous `bootRun`. The redesigned path must reuse explicit running services, the build daemon, current IDE output, and one runtime-classpath cache while retaining Spring DevTools as the restart owner.
- The current console is bare classpath JShell, not a booted Spring environment. The current rename walks Java sources, replaces identifier tokens, moves matching files, and deliberately leaves literals; it has no manifest/query/catalog/storage transition. Sections 7.16 and 7.17 replace those partial contracts rather than layering another command beside them.
- A concrete `Task` scaffold exposes a more severe lifecycle split. `jails --pretend --output json destroy scaffold Task --force` proposes deleting the original `V001__create_tasks.sql` with no database effect, so an already-created table survives while its schema history disappears from source. Re-running `generate scaffold Task` proposes replacing that same V001 from the current record and prints the stray field `PROBE` lines. The inspected worktree currently has Java and V001 edits for `assignee_id` and `version`; if another environment applied the earlier V001, replacing the file cannot add those columns and may also trigger Flyway checksum validation. The CLI has an additive field command but no complete alter/drop/repair/revive lifecycle, so the user can be left between ledger, Java, migration history, and live schema states. Section 7.19 makes migration history append-only and defines a way out of every such state.
- Conflicted three-way merges are currently refused. A durable frozen conflict plus `continue`/`abort` does not yet exist.
- Generated migrations are forward-only and file transaction recovery is roll-forward. This RFC preserves both properties: it does not add generated down migrations or database undo.
- `jails check` currently delegates to the clean build-tool truth. Faster verification can be added as a distinct evidence level without pretending it is identical to a clean build.

These facts define the migration path from today's implementation. A proposal that changes one must name the existing failure mode, supply new evidence that closes it, and ship a migration path; a flag name or safety adjective is not evidence.

The roadmap is delta work, not a request to rebuild what already exists. The current tree already has one whole-manifest `app` transition, a prepared-bundle/commit pipeline with pretend parity, command-derived JSON vocabulary, a resident `testd`, bytecode affected-test selection with widening, generated JDBC plus in-memory projections, compose-backed `start`/`stop`, and read-only `doctor`/`why` reports. New work SHALL extend those paths and tests rather than introduce parallel models, command inventories, service lifecycles, or mutation engines.

### 0.4 The acceptance portfolio is part of the architecture

Generic machinery is proved by unrelated applications, not by one happy-path fixture. The current portfolio state is:

| Application | Purpose | Automated proof state |
|---|---|---|
| payments gateway | transactional/payment-shaped slice | held by `SPRING_APP_MANIFESTS` |
| support inbox | Intercom-shaped application | held |
| web crawler | crawler/search-shaped application | held |
| ledger CLI | non-Spring proof | held by its dedicated test |
| minicom | second Intercom-shaped port | manifest exists, not held |
| minicom-spring | Gradle interview scaffold | hand-verified, not held |
| private acceptance brief A | complex business rules outside CRUD | not started; local material is not publishable |
| private acceptance brief B | deterministic ingestion and ranking behavior | not started; local material is not publishable |

Before adding more proof applications, choose the cost model: all run by default, a strict CI tier that fails when skipped, or generate/typecheck for some and full Maven/Failsafe for a representative subset. Adding applications one at a time without this decision grows the already dominant suite tail accidentally.

The first inexpensive repair is to put the existing minicom manifests under automated Maven/Gradle proof. Private briefs remain local inputs only; only anonymized, independently written fixtures may be committed.

### 0.5 Reconciled idea families

Where the evidence and proposals disagreed, the synthesis prefers the smallest mechanism that closes a measured workflow while preserving current ownership boundaries:

| Idea family | Consolidated direction | Safety/validation condition |
|---|---|---|
| save/test/run loop | Make `jails test` the sole dependable test front door: it partitions one requested test universe across build-daemon and warm engines, and `--watch` owns the resident daemon. Make `jails run` reuse a cached runtime classpath and exactly one compiler source while Spring DevTools owns reload | `--fast` never changes which tests count; warm ineligibility delegates visibly; no test path supervises the app, and run starts services only with explicit `--services start` |
| SQL parser vs live database | Use three evidence levels: fast parse, offline migration catalog, and live `PREPARE`/describe. Build or adopt only a bounded dialect parser; use the database as final authority and cache by complete inputs | Unsupported SQL becomes `needs-live-verification`, never guessed success |
| CLI DSL and manifests | Keep compact CLI fields shell-safe and bounded; put composite relationships, policies, and advanced database constructs in the existing structured app manifest | One constructor path, source spans, no arbitrary predicate/SpEL language, and copy-paste tests through Bash, Zsh, Fish, and PowerShell |
| Receipt undo | Permit forward file undo only for receipts that contain no migration or external-effect ambiguity; otherwise generate a corrective-plan explanation and refuse | Crash recovery stays roll-forward; migration files and applied schemas are never rolled back or removed by undo |
| SQL sandbox | Retain only as a deferred generated-test experiment, not a roadmap dependency or default | Must fail explicitly for commit, `REQUIRES_NEW`, after-commit events, concurrent statements, and unsupported thread handoff; isolated schema remains the baseline |
| Development services | Reuse committed Compose through existing `start`/`stop` and generated Spring Testcontainers beans. Live SQL requires an explicit datasource or an already running declared service | No absence-triggered provisioning, second service command tree, dynamic-port shadow configuration, or Docker startup from `testd`/read-only commands |
| Catalog architecture | Add observed `SchemaObjectId` values and query ownership, but keep the migration file as durable schema authority | Opaque PostgreSQL objects are preserved and block unsafe diffs; no live I/O in protocol/spec/generate |
| Application authoring | Extend the existing app manifest only after a proof fixture demonstrates a concrete expression gap; defer a separate domain-model TUI | Equivalent existing manifest/CLI input produces one plan; no parallel manifest language |
| ArchUnit | Generate one project-level, syncable architecture suite from configured layers with explicit, reasoned shared-kernel/edge allowances | It runs in the default unit-test phase; seeded violations and allowance expiry/cleanup are tested |
| Neovim/editor integration | Add one versioned editor protocol over existing command-result/event contracts; keep `jails.nvim` an asynchronous adapter for completion, symbols, diagnostics, plans, receipts, and test-watch status | No human-output scraping, Lua command grammar, UI-thread waits, debugger duplication, app supervision, or diagnostic-namespace clearing |
| Developer tool gateway | Add route-aware `curl`, a `pgcli`-first database console, and a Spring-booted JDK 26 REPL over the real project classpath | Explicit invocation may boot the app or connect to it, but never starts infrastructure; TTY, signals, credentials, profiles, and child exit status remain transparent |
| Resource lifecycle | Separate deletable code projections from append-only schema history; evolve fields through new forward migrations and retain retired entity/storage identity | Plain destroy refuses ambiguous storage intent; repair/revive has a deterministic path from edited/deleted migrations, preserved tables, and partial generated state |
| Resource rename | Treat a rename as one stable resource identity crossing generated Java, manifest edges, SQL contracts, migrations, tests, and editor symbols | External API names remain stable by default; physical table preservation is explicit, while a physical rename is a new forward migration and refuses unresolved hand-written or opaque dependencies |

### 0.6 Additional high-value problem statements

These are useful opportunities and evidence sources, not constraints on the research roadmap:

- prove `examples/minicom/` and `examples/minicom-spring/` automatically;
- reduce the real Failsafe/Maven suite tail without disabling or filtering tests;
- use anonymized private acceptance cases to settle the boundary between generated structure and hand-written behavior without publishing their source material;
- complete Maven/Gradle parity with explicit wrapper, classpath, and transaction semantics;
- split `doctor/wiring.rs` rather than raising its file-size ceiling again;
- evaluate a `Renderer` abstraction for repeated generator shape and a `ToolRunner` only if it improves testing without hiding real argv behavior;
- promote load-bearing historical plan citations into short decision records for one-writer, transaction protocol, machine-state compatibility, hermetic processes, and closed schemas;
- design frozen conflict `continue`/`abort` as a complete durable state machine;
- measure the generated k6 profile and Spring context-cache misses rather than repeating unmeasured performance claims.
- remove the repeated classpath/credential/endpoint ceremony around `curl`, `pgcli`, JShell, and one-shot application scripts without wrapping those tools in a proprietary runtime.

The hard exclusions are: no shipped `jails` runtime/framework, no web or desktop UI, no heavy ORM/JPA substitution, no opaque runtime magic, and no unsafe or unexplained mutation path. New CLI/TUI/compiler ideas remain in scope when they satisfy those constraints.

---

## Section 1: Executive DX Vision and Top 12 Breakthrough Concepts

### The vision: a transparent local workbench, not a framework

The target experience is:

```text
short intent
    ↓ parse once
canonical, versioned model
    ↓ validate and project in memory
Java + SQL + tests + fixtures + contracts + architecture rules
    ↓ prepare once
one reviewable operation graph and diff
    ↓ either preview or durable commit
exactly the same result value
    ↓ warm local workbench
compile/test/run only what the change can affect
```

The governing rules are:

1. **Generated Java is the product.** There is no `jails.jar`, hidden object model, runtime proxy layer, or generated code that only `jails` can execute.
2. **One semantic input, many explicit projections.** A field, relation, route, or SQL query is parsed once and becomes a typed value. Java, DDL, JSON samples, fakes, and tests consume that value rather than reparsing strings or reverse-inferring one another.
3. **Fast paths must prove their eligibility.** If `testd` cannot prove that an affected-test set is safe, it widens to all tests. If SQL analysis is parse-only, output says `parse-only`, never `verified`.
4. **Preview and apply are the same computation.** `--pretend` stops before commit; it does not run a second planning path.
5. **Generated ownership is granular.** `jails` may update the nodes or marked regions it owns. Unowned code is preserved; ambiguous edits become conflicts with evidence.
6. **The terminal is the interface.** Interactive work is a TUI or prompt sequence that emits the same canonical manifest and prepared bundle as non-interactive commands.

### Top 12 concepts, ranked

| Rank | Breakthrough | Adapted mechanism | Why it is high leverage | Primary pillar | First owner |
|---:|---|---|---|---|---|
| 1 | **SQL Contract Compiler** | sqlc catalog/query split, SQLx live/offline checking, jOOQ reverse engineering | Makes reader-owned SQL an asset: named queries become explicit Java records, binders, mappers, ports, fakes, and contract tests | Correctness + authoring | `jails-spec`, `jails-generate`, `jails-drive` |
| 2 | **Evidence-Carrying Prepared Diffs** | existing prepared bundles/journals plus Rails `--pretend` ergonomics | Shows exact bytes, ownership, reasons, risks, and verification before the existing commit path | Trust + authoring | `jails-prepare`, `jails-report` |
| 3 | **Safe Resource Lifecycle and Coordinated Rename** | append-only migration lineage, stable manifest identity, and forward schema evolution | Makes add/change/destroy/repair/rename converge Java, ledger, migrations, and storage instead of trapping a project between them | Correctness + authoring | `jails-spec`, `jails-project`, `jails-prepare` |
| 4 | **Unified Fast Test and Run Loop** | current `test`, `testd`, build daemons, runtime classpath, IDE output, Spring DevTools | Keeps `jails test` correct while selecting the fastest eligible engine, and removes repeated build-tool/service startup from `jails run` | Latency + trust | `jails-project`, `jails-java`, `jails-drive` |
| 5 | **Maven/Gradle Behavioral Parity** | wrapper/build-model abstraction | Prevents every later feature from being Maven-only or silently partial | Reach + trust | `jails-project`, `jails-drive` |
| 6 | **Existing App-Manifest Extension** | Phoenix contexts, Wasp specs, JHipster JDL | Extends one shipped whole-app transition only where proof apps expose a real modeling gap | Authoring + correctness | `jails-protocol`, `jails-spec`, `jails-generate` |
| 7 | **Bounded Schema Observation** | Django/Alembic state comparison, Prisma pull, PostgREST catalog cache | Compares declared, migration-derived, and live facts while preserving unsupported PostgreSQL objects as opaque blockers | Correctness | `jails-project`, `jails-prepare` |
| 8 | **Evidence-Bounded Diagnostics** | Spring FailureAnalyzers plus current `why`/`doctor` | Combines source facts and captured runtime failures without simulating the Spring container | Trust + latency | `jails-report`, `jails-project` |
| 9 | **Generated Test Economy** | Laravel factories and Spring Testcontainers service connections | Fast fakes plus ordinary real-database contracts, generated from the same typed values | Latency + correctness | `jails-generate`, `jails-testkit` |
| 10 | **Application Tool Gateway** | Rails console/dbconsole/runner ergonomics over JShell, `pgcli`, and `curl` | Turns the project model into one-command access to a booted Spring context, database, and running HTTP app without replacing the underlying tools | Latency + authoring | `jails-project`, `jails-drive` |
| 11 | **Versioned Editor Bridge** | LSP-style ranges plus existing command/event results | Gives Neovim structured completion, symbols, diagnostics, plans, receipts, and test-watch status | Authoring + trust | CLI, `jails.nvim` |
| 12 | **Adoptable Architecture Fitness** | ArchUnit bytecode rules and baselines | Makes configured boundaries executable while allowing explicit shared kernels and justified edges | Correctness | `jails-generate` |

### Two enabling primitives beneath the twelve

The cross-ecosystem ideas become one product only if queries can be owned and schema facts can be named without pretending that a column is independently reversible. Add one durable query resource and one non-owning observed identity:

```rust
ResourceKey::Query { file: ProjectPath, name: QueryName }
SchemaObjectId { dialect, namespace, kind, name, parent }
```

`Query` gives each named statement an owner, verification record, generated projections, and retirement behavior. `SchemaObjectId` identifies an observation or diff node but is not a ledger ownership claim: the Flyway file remains the durable schema authority, and file destroy/undo cannot manufacture a database inverse from the ID.

The second primitive is an evidence vocabulary shared by every fast path:

```text
parsed < verified-offline < verified-live < executed-test
```

Output may report a stronger level only when it carries the relevant input digests and verifier version. `hypothesis` is a separate blocking diagnostic state, never a success grade. This prevents a cache hit, heuristic rename, source-only bean scan, or parse-only SQL result from being presented as fact.

### North-star service levels

Performance targets should be budgets with visible fallback reasons, not unconditional promises:

| Workflow | Warm target | Cold target | Correctness fallback |
|---|---:|---:|---|
| Parse a slice/manifest and produce a plan | p95 ≤ 50 ms | p95 ≤ 150 ms | Refuse malformed or ambiguous input |
| Render `--pretend` for a normal slice | p95 ≤ 100 ms | p95 ≤ 300 ms | Show which cache missed |
| `testd` run of one already-loaded eligible unit test | initial gate p50 ≤ 100 ms, p95 ≤ 150 ms; ratchet only from a dated baseline | JVM start reported separately | Restart daemon on classpath/test-engine drift; delegate ineligible tests |
| Affected test selection | ≤ 20 ms after compilation | ≤ 100 ms graph rebuild | Run all tests on an unknown edge or uncompiled source |
| Static SQL check from cached catalog | p95 ≤ 75 ms/query | p95 ≤ 500 ms/catalog | Mark parse-only or require live verification |
| Live SQL verification against an already running Postgres | p95 ≤ 250 ms/query after connection/catalog setup | connection and any user-run service startup separately timed | Refuse if no explicit datasource or server/catalog fingerprint differs |
| `doctor`, source-only routes/beans | p95 ≤ 150 ms | p95 ≤ 500 ms | State the static-analysis boundary |
| Route-aware `jails request` dispatch overhead | p95 ≤ 25 ms before `curl` | p95 ≤ 100 ms discovery | Refuse ambiguous route; preserve raw curl failure/body |
| `jails db console` dispatch overhead | p95 ≤ 50 ms before `pgcli` | p95 ≤ 150 ms discovery | Refuse missing client or unreachable explicit datasource |
| `jails console` launch overhead | p95 ≤ 250 ms beyond build-model/classpath and Spring boot | classpath and Spring boot timed separately | Refuse stale outputs, unsupported release, or ambiguous main class |
| Coordinated resource-rename plan | p95 ≤ 250 ms after project facts are warm | p95 ≤ 1 s source/SQL rescan | Refuse unresolved hand-written, ambiguous, opaque, or deployment-unsafe edges |

Each command should emit timing spans under `--debug` or `--output json`: `discover`, `parse`, `observe`, `project`, `prepare`, `verify`, `commit`, and external process/container time. This makes latency work empirical.

---

## Section 2: Pillar 1 — Sub-Second Feedback Loops

### 2.1 Make `jails test` the one correct, fast front door

The present commands expose implementation choices to the user. `jails test` is dependable but takes the build path; Maven `test --fast` launches a fresh JVM rather than the daemon and falls back for several reporting flags; Gradle refuses it; `testd` is Maven-only and refuses stale classes without arranging compilation. The target removes that decision from normal use:

```text
jails test [<test-or-method>...]
  [--scope unit|integration|all]
  [--watch] [--affected] [--failed] [--tag <tag>]...
  [--engine auto|build|warm]
  [--compile auto|ide|build|none]
  [--fail-fast] [--slowest[=<n>]] [--output human|json]

jails test daemon status|restart|stop
```

The default scope remains `unit` for compatibility with Maven/Gradle `test`; an explicit `*IT` selector routes to integration, and `--scope all` runs both. `engine=auto` is the default and preserves the requested test universe: eligible tests may run warm while ineligible tests delegate to the build tool, then one `TestReportV1` combines both partitions. `engine=build` forces the ordinary wrapper. `engine=warm` is diagnostic/strict and refuses any ineligible partition rather than silently omitting it. Existing `--fast` is a one-release compatibility alias for `--engine auto`; because auto is the default, it prints the chosen engines and reasons but never means “run fewer tests.” Existing `jails testd` forms map visibly to `jails test --engine warm --compile none` or `jails test daemon ...` for one release.

Compilation policy is independent. `auto` consumes fresh IDE output when its epoch and classpath match, otherwise uses the narrow build-tool compile/test-classes task through mvnd or the Gradle daemon. `ide` waits for the configured jdtls output and refuses after a bounded timeout. `build` always asks the wrapper. `none` requires current output. A stale class is never run as current, but the default now repairs staleness instead of sending the user to a different command.

The complete test path should keep five versioned caches per project root:

1. **Build model** — active module, Java release, source roots, classpath, compiler/test plugin configuration.
2. **File fingerprint table** — content hash and metadata for sources, resources, POM/Gradle files, migrations, and query files.
3. **Bytecode graph** — class → referenced types and reverse type → referrers, split into production and test owners.
4. **Test inventory** — JUnit unique IDs, source class, tags, last outcome, duration, and last bytecode digest.
5. **SQL catalog** — normalized schema fingerprint plus per-query analysis records.

Keep ownership explicit. The Java daemon owns the warm JUnit launcher, a replaceable test classloader, and test inventory. The Rust-side test coordinator owns output watching, build fingerprints, bytecode/source graphs, selection, epochs, compilation policy, result aggregation, and process recycling. Build-tool compilation is still Maven/Gradle execution, not a Java compiler inside `jails`. The coordinator does not own application execution, service startup, SQL analysis, or debugging.

The local IPC protocol should be explicit and versioned:

```text
HELLO protocol=2 project=<canonical-root-id> client=<jails-version>
SYNC changed=[path,digest,...] build_digest=<digest>
RUN selectors=[junit-unique-id...] mode=affected|failed|all epoch=<n>
RESULT epoch=<n> discovered=12 selected=3 passed=3 duration_ms=47 fallback=null
```

An epoch binds results to a source/build snapshot. A reply from epoch 41 cannot be shown as the result of epoch 42. POM, Gradle settings, JVM version, JUnit engine, or classpath changes invalidate the JVM process; ordinary bytecode changes invalidate only graph nodes and test outcomes.

The safe selection algorithm is:

```text
changed source
  → compiled owner classes (including nested classes)
  → reverse transitive CONSTANT_Class / descriptor references
  → test owners
```

This is grounded in the class-file format: the JVM constant pool contains symbolic class, field, method, interface, name/type, and descriptor information. The existing `referenced_types` parser is therefore the right cheap substrate. The JVM specification documents the one-based constant-pool table and its symbolic references ([JVM Specification §4](https://docs.oracle.com/javase/specs/jvms/se21/html/jvms-4.html)).

Constant-pool reachability is conservative but not complete. Interface injection, Spring configuration, AOP pointcuts, profiles/conditions, reflection, resources, and context-cache keys create edges absent from direct bytecode references. The selector must run **all eligible warm tests** and delegate integration/context tests to the ordinary build-tool path, with the reason printed, when any of these occurs:

- a changed Java source has no compiled owner class;
- annotation processors or generated sources changed;
- tests are selected through strings, resources, reflection, `ServiceLoader`, Spring configuration properties, SQL/migration files, or external contracts not represented by bytecode edges;
- class parsing sees an unknown class-file tag/version;
- the build/classpath fingerprint changed;
- a deletion or rename cannot be mapped to the previous class owner.

Additive dependency hints may cover irreducible project-specific cases:

```toml
# .jails/app.toml
[[test_dependency]]
input = "src/main/resources/db/migration/**"
tests = ["**/*RepositoryIT", "**/*MigrationIT"]

[[test_dependency]]
input = "src/main/resources/application*.properties"
tests = ["**/*ContextIT"]
```

Hints can only add tests to the computed set; they can never remove a test, suppress widening, or make an incomplete graph current. Rename/delete validation must report stale patterns. The crucial UX is the explanation:

```text
$ jails test --affected
selected 7/184 tests
  4  bytecode reverse dependencies of OrderService
  2  rule: db/migration/** → *RepositoryIT
  1  previous failure
fallback: none
47 ms test execution; 13 ms selection
```

`jails test --watch` watches compiled output roots rather than assuming a source edit is compiled. On each quiet window it:

1. detects changed `.class` files and updates only their graph edges;
2. compares source/output freshness and waits briefly for the IDE compiler;
3. resolves stale output according to the declared compile policy and reports the compiler owner;
4. selects affected tests, widening on every unknown;
5. sends the epoch-bound run to the warm JVM.

Warm eligibility defaults to tests with no Spring context, container, integration-test, fork, or known global-state requirement. Ineligible tests run through the ordinary Maven/Gradle selector. Unlike the current console-launcher path, the daemon produces the same normalized report fields required by JSON, slowest, failed, and fail-fast; engine-specific raw XML is an input, not the public contract. A randomized oracle compares warm batches in different orders with isolated runs. The daemon restores changed system properties where possible and recycles after 50 test-classloader generations, 128 MiB of metaspace growth, any leaked non-daemon thread, or an engine/classpath change. These defaults are configurable but may only become less conservative with measured oracle evidence.

### 2.2 Incremental compilation and fast application start without pretending to be a compiler

`jails` should orchestrate the project's compiler rather than own Java semantics.

- If the IDE or language server has produced fresh `.class` files, use them.
- Otherwise run the narrowest supported build-tool compile goal and measure it.
- Cache the resolved wrapper, classpath, compiler release, and output roots by build-file digest.
- Watch directories with OS notifications, but always rescan hashes after overflow or wake-up; watcher events are hints, not truth.
- Coalesce changes for a short quiet window (for example 20–40 ms), then compile once.

Application startup shares the runtime-classpath provider later used by console:

```text
jails run [--watch]
  [--launcher auto|classpath|build-tool|jar]
  [--compile auto|ide|build|none]
  [--services existing|start|none]
  [--profile <name>]... [--] <application-argv>...
```

`launcher=auto` is the default. With current output and a matching cached runtime-classpath fingerprint it launches the main class directly with the selected JDK, main/resources outputs, and resolved dependencies. Otherwise it runs the narrow compile/classes task through mvnd or the Gradle daemon, resolves once, then launches directly. `build-tool` preserves `spring-boot:run`/`bootRun` for diagnosis; `jar` requires a current packaged artifact. Existing `--no-build` becomes a one-release alias for `--compile none --launcher auto`, with freshness validation instead of “whatever happens to be in target.” Application argv remains tokenized; it is never space-joined into a plugin property in the direct path.

Service policy is visible. `existing` is the default and only checks declared endpoints; `start` explicitly invokes the existing `jails start` lifecycle before boot; `none` performs no service checks. This replaces the current unconditional Compose `up` inside `run`/watch and makes its latency a user choice. Startup reports separate `service-check`, `compile`, `classpath`, `jvm-launch`, `spring-started`, and `application-ready` spans. Ready means a configured HTTP/TCP probe succeeded or a captured ordinary Spring readiness signal was observed; a merely live PID is `started`, not `ready`.

`run --watch` has one compiler owner. `compile=ide` watches output directories and lets jdtls update them. `compile=build` watches source/resource inputs and uses the build daemon's incremental compilation. `auto` selects IDE only when the handshake proves its output root/epoch, otherwise build. It does not combine jdtls, a 750 ms source poll, and a second Maven/Gradle continuous compiler. Spring DevTools remains the application restart owner: it watches changed classpath directories and uses its restart classloader, which is typically faster than a cold start ([Spring Boot DevTools automatic restart](https://docs.spring.io/spring-boot/reference/using/devtools.html#using.devtools.restart)). jdtls/DAP continues to own debug HotSwap.

JDK 26 makes Spring Boot's AOT cache a relevant cold-start experiment, but not a default. Spring documents the AOT cache as the successor to CDS on Java 25+ ([Spring Boot class-data sharing and AOT cache](https://docs.spring.io/spring-boot/reference/packaging/class-data-sharing.html)). A later `jails run cache prepare` spike may create a project-local ignored cache only from a packaged artifact, training profile, exact JDK, classpath, JVM args, and application digest; `run` uses it only on an exact fingerprint match. The cache does not compose with source-level DevTools restart and is rejected unless benchmarks beat the direct-classpath cold path materially.

Application reload therefore stays on `run --watch` plus Spring DevTools. `jails` may diagnose missing/stale class output and captured startup failure, but it does not attach an instrumentation agent, create another restart classloader, or claim ownership of debug reload. Structural Java changes are ordinary DevTools/process restarts, not a new HotSwap feature.

### 2.3 Incremental source and AST indexes

Incremental AST updates should be one shared facility, not separate caches inside commands. `jails routes`, `beans`, field evolution, and merge previews should share a persistent source index keyed by `(canonical path, content digest, parser version)`. Store only facts derivable from source:

- package and top-level/nested types;
- record components and annotations;
- imports and constructor parameters;
- Spring stereotype, bean, and mapping annotations;
- generated ownership anchors and syntax spans.

On edit, parse the changed file and update its edges in one transaction. Consumers see an immutable index epoch. A parse failure preserves the last good facts only as `stale`; it may not silently present them as current. Terminal output must say:

```text
routes: 18 current, 2 unavailable
  src/main/java/.../LegacyController.java:87 parse failed near `}`
```

This extends the current lightweight `jails-java` approach for read-only facts. A semantic edit vocabulary is useful as a **report view**, not as a new mutation engine:

```rust
enum JavaEditReport {
    AddImport { qualified: JavaType },
    AddAnnotation { target: TypeId, annotation: Annotation },
    AddRecordComponent { target: TypeId, component: FieldSpec },
    AddMember { target: TypeId, anchor: MemberAnchor, source: String },
}
```

These values describe what an existing owned-region/text splice changed after preparation. Applying changes remains the current ownership-aware byte preparation and three-way merge. An unowned or ambiguous Java shape conflicts; `jails-java` MUST NOT grow classpath symbol resolution, whole-file AST rewriting, comment reprinting, or import disambiguation. A future external OpenRewrite adapter, if justified, runs as an explicit external verifier/recipe and still returns bytes through the same prepare path.

### 2.4 Explicit development-service interoperability

Quarkus demonstrates the value of eliminating service ceremony, but absence-triggered provisioning would create a second lifecycle beside the project's committed Compose and generated Spring Testcontainers configuration ([Quarkus Dev Services overview](https://quarkus.io/guides/dev-services)). `jails` therefore adopts discovery/explanation, not implicit provisioning:

```text
run/sql command
  → inspect explicit datasource/environment
  → otherwise inspect the committed Compose service and its configured host port
  → verify that the selected Docker/Podman endpoint is reachable by the consumer
  → use the already-running service or refuse with `jails start` as the fix
  → print what was selected and why
```

For diagnostics and live-SQL cache identity, hash the committed image/tag, ports, non-secret environment names, init scripts, and relevant migrations. Existing managed resources may use labels such as:

```text
dev.jails.project=<root-id>
dev.jails.service=postgres
dev.jails.spec=<sha256>
dev.jails.managed=true
```

Rules that preserve trust:

- Explicit application configuration always wins; say `postgres: external (spring.datasource.url)`.
- Image versions are pinned in the capability spec and visible in the plan.
- `jails start`/`stop` remain the only CLI-owned Compose lifecycle. A missing service is a refusal, never a side effect of `testd`, `doctor`, editor completion, or `sql check`.
- Generated integration tests use Spring-managed `@Bean` plus `@ServiceConnection`, not CLI leases or static JUnit `@Container` fields.
- Readiness is semantic (`SELECT 1`, broker metadata), not “container is running.”
- Docker, Podman, WSL, Colima, and similar environments are capability-tested by consumer: a container visible to a CLI is not assumed visible to Testcontainers.
- Live SQL accepts an explicit datasource or the verified running Compose endpoint. It never invents a dynamic port or shadow application configuration.

### 2.5 Instant schema catalog and SQL contract verification

sqlc's implementation separates schema catalog construction, query parsing/rewriting, semantic analysis, and code generation. It caches catalog analysis and maps errors in rewritten SQL back to the developer's original source. Its documented flow is simply “write SQL → generate type-safe code,” with optional database-backed enhanced analysis and per-query caching ([sqlc generate](https://docs.sqlc.dev/en/latest/howto/generate.html)). SQLx adds a useful trust model: a query may be checked against a live `DATABASE_URL` or against checked-in offline metadata, and CI can verify that metadata is current ([SQLx `query!`](https://docs.rs/sqlx/latest/sqlx/macro.query.html)).

`jails` should expose three non-confusable verification levels:

| Level | Evidence | Allowed result label | Use |
|---|---|---|---|
| Parse | dialect parser accepts syntax and named-parameter rewrite | `parsed` | editor-speed feedback |
| Offline catalog | migrations/schema snapshot resolves tables, columns, parameters, outputs and nullability | `verified-offline` | disconnected development/CI |
| Live catalog | server prepares/describes query against exact schema fingerprint | `verified-live` | release gate and difficult PostgreSQL semantics |

Unknown analysis is not success. A query outside the offline analyzer's supported subset becomes `needs-live-verification`, never a guessed Java type.

Cache keys must include:

```text
dialect + server-major + normalized-schema-digest + query-digest
+ search_path + analyzer-version + type-override-digest
```

Store only non-secret metadata under `.jails/cache/sql/`; never persist connection passwords or full production URLs. A checked-in contract file belongs under `.jails/sql-contracts/` and contains query digest, schema digest, parameter/result columns, nullability, dialect, and verifier version.

Recommended SQL file convention:

```sql
-- name: FindPayableOrders :many
-- timeout: 250ms
SELECT id, account_id, total, status, created_at
FROM orders
WHERE status = :status
  AND total >= :minimum
ORDER BY created_at, id
LIMIT :limit;
```

Generated code uses an explicit `RowMapper` lambda, never `query(Order.class)`, because the latter is convenient reflection-based mapping. The contract compiler generates:

- `FindPayableOrdersParams` record;
- `FindPayableOrdersRow` record when the result is not exactly a domain record;
- a query port and `JdbcClient` implementation;
- deterministic bind order/name mapping;
- an in-memory fake whose filtering and stable ordering mirror the declared semantics where feasible;
- a live contract test that prepares/executes the query against Testcontainers;
- a frozen metadata check in CI.

### 2.6 Latency architecture and invalidation table

| Input changed | Reparse | Recompile | Restart JVM | Rebuild SQL catalog | Test scope |
|---|---|---|---|---|---|
| Java method body | one file | owning module incremental compile | no for tests; Spring DevTools observes output | no | reverse bytecode dependents |
| Record shape / method signature | one file + dependents on compile error | owning module | yes for running app | maybe if query record is generated | reverse dependents, widen on failed compile |
| `pom.xml` / Gradle build | build model | build tool decides | yes | if JDBC/dialect/config changed | all |
| Migration | migration AST | generated contracts if schema changed | app restart if running | yes | migration + repository contracts |
| Named query SQL | query only | generated query sources | no until class changes compile | query entry only | query contract + known callers |
| `application.properties` | property file | usually no | app restart | if datasource/search path changed | context tests |
| Template override | template + placeholders | affected generated files after accepted plan | after compile | if SQL output changes | generated companion tests |

### 2.7 Experimental Ecto-style SQL sandbox

Ecto's SQL Sandbox makes a real database cheap by checking out a connection per test, opening a transaction, and rolling it back; shared mode lets collaborating processes use the test owner's connection ([Ecto SQL Sandbox](https://hexdocs.pm/ecto_sql/Ecto.Adapters.SQL.Sandbox.html)). Spring's ordinary test transaction is bound to the current thread, so a request handled on another thread, an outbox poller, or a `REQUIRES_NEW` operation can escape it ([Spring test-managed transactions](https://docs.spring.io/spring-framework/reference/testing/testcontext-framework/tx.html)).

That makes a generated sandbox one of the highest-upside and highest-risk ideas in the combined research. Prototype four explicit isolation modes:

| Mode | Best for | Guarantee | Known limitation |
|---|---|---|---|
| in-memory fake | domain/use-case unit tests | deterministic port behavior | no database semantics |
| ordinary `@Transactional` | same-thread repository tests | rollback on the test thread | work on other threads escapes |
| generated shared `SandboxDataSource` | HTTP tests whose collaborating threads can be bound to one checkout | real SQL and rollback without truncation | incompatible with real commit/independent transactions unless explicitly handled |
| isolated schema/database | outbox, jobs, concurrent transactions, commit behavior | strongest realistic isolation | slower setup/cleanup |

The sandbox candidate consists only of generated test code: a JUnit extension, connection lease, `DataSource` decorator, and opt-in annotation. It is never a `jails` runtime dependency and never the global default. A proof must include HTTP thread handoff, connection-pool exhaustion, nested and `REQUIRES_NEW` transactions, virtual threads, outbox polling, timeout cleanup, and parallel failure cases.

Measure it against the recorded Failsafe tail using the same application and warm-run procedure. Promote it only if it materially lowers wall time without weakening a test's semantics; otherwise retain per-schema/Testcontainers isolation and record the negative result.

---

## Section 3: Pillar 2 — Correctness, Trust, and Zero Puzzlement

### 3.1 Make the prepared bundle the universal explanation

The current prepare/commit split is the strongest differentiator in `jails`; all new features should enter it rather than bypass it. Extend each operation with evidence:

```text
operation_id
path
kind: create | replace | delete | mkdir | effect
owner: capability/entity/intent
reason: human sentence + stable reason code
source_inputs: paths and content digests
precondition: absent | exact digest | owned-node digest
verification: parsed | compiled | verified-offline | verified-live | unverified
risk: additive | behavior-change | destructive | external-effect
inverse_candidate: operation id or unavailable reason
```

Human and JSON output must be projections of this same value. `--pretend` renders it and stops. Apply rechecks the captured preconditions under the project lock and commits the same plan; it never replans.

### 3.2 Dry-run and diff contract

Default `--pretend` output should be concise, with expansion flags:

```text
$ jails g scaffold Order ... --with-events --pretend
PLAN  16 operations, 0 conflicts, verified-live SQL

CREATE  src/main/java/com/acme/orders/domain/Order.java
        because entity Order owns its domain record
CREATE  src/main/resources/db/migration/V014__create_orders.sql
        because Order is persistent and Flyway is installed
EDIT    pom.xml  +1 dependency
        com.tngtech.archunit:archunit-junit5 (test)

RISK    additive 15 · behavior-change 1 · destructive 0
NEXT    rerun without --pretend to apply transaction 01K...
```

Flags:

```text
--diff                 colorized unified file diffs
--ast                  semantic edit view (AddImport, AddRecordComponent, ...)
--why                  provenance/reason/input digests
--verify               run compile/SQL/architecture verification before accepting
--output json          stable machine-readable envelope
--no-color             deterministic logs
```

Color is redundant decoration; `+`, `-`, operation verbs, and risk labels must carry meaning without it. Large generated files default to a summary and require `--diff` to expand. Secrets in properties, environment, or connection strings are redacted before either renderer receives them.

### 3.3 AST-aware merge with honest conflict boundaries

AdonisJS demonstrates the transferable split between templates for new files and AST codemods for host-file edits; its development-only assembler keeps AST machinery out of production applications ([AdonisJS scaffolding and codemods](https://docs.adonisjs.com/guides/concepts/scaffolding)). For `jails`:

1. New owned files render from templates.
2. Owned semantic nodes edit through typed selectors and preconditions.
3. Regeneration performs base/current/desired three-way reconciliation.
4. Unowned overlaps produce a conflict in the plan; no write occurs.
5. Text merge is a final compatibility path, not proof of AST correctness.

Each conflict should show the selector and candidates:

```text
CONFLICT src/main/java/.../Order.java
  wanted: add component `status: OrderStatus` to record Order
  found:  two top-level records named Order after parse recovery
  kept:   current file unchanged
  fix:    make the target unique, then rerun; or use --package/--type
```

There should be no generic `--force` that discards unowned Java. Existing narrowly scoped destructive authorization, such as removing a hand-written strategy implementation whose generated interface is being destroyed, remains valid only for that named operation and after its exact diff is confirmed. New conflicts use a narrowly named `--accept-generated <operation-id>` only after the resulting diff is shown and becomes part of a new prepared bundle.

### 3.4 Bidirectional consistency as a three-source reconciliation problem

Schema/code synchronization has three states, not two:

```text
D = declared intent (.jails/app.toml / SQL contracts)
G = last generated owned baseline (ledger + content objects)
L = live observations (current files and optional database catalog)
```

Reconciliation rules:

- `D = G, L = G`: no-op.
- `D ≠ G, L = G`: safe forward generation from a changed declaration.
- `D = G, L ≠ G`: user/live drift; preserve and report it.
- `D ≠ G, L ≠ G`: three-way merge if changes do not overlap; otherwise conflict.
- live DB-only object: import candidate, ignored object, or conflict—never silently delete.
- missing live DB object: destructive migration candidate requiring explicit acceptance.
- unsupported objects—partial/expression indexes, generated columns, extension types, triggers, routines, RLS policies, and unparsed constraints—remain opaque observations with their defining digest; they are never normalized away, regenerated, renamed, or dropped.

Django's autodetector compares whole project states and orders operations because foreign keys and many-to-many changes interact. Alembic likewise produces candidate migrations from metadata/database differences and explicitly warns that candidates require review ([Django migrations](https://docs.djangoproject.com/en/6.0/topics/migrations/), [Alembic autogenerate](https://alembic.sqlalchemy.org/en/latest/autogenerate.html)). `jails schema diff` should adopt both ideas: global dependency ordering and mandatory review for rename/destructive ambiguity. If an opaque object depends on, contains, or may be affected by a candidate operation, the plan blocks with `unsupported-schema-object`; preserving its text is not proof that the operation is safe.

Rename detection is heuristic evidence, not fact:

```text
POSSIBLE RENAME orders.customer_id → orders.buyer_id (confidence 0.82)
  same type/nullability/FK target; old removed and new added
  choose: --accept-rename | --treat-as-drop-add
```

### 3.5 Migration linting by kind of risk

Atlas's SQL analyzers provide a valuable taxonomy: a migration can be destructive, data-dependent, constraint-dropping, or deployment-incompatible. Those categories require different remedies and should not collapse into one “dangerous migration” warning ([Atlas SQL checks](https://github.com/ariga/atlas/tree/master/sql/sqlcheck)).

| Risk class | Example | What `jails` adds |
|---|---|---|
| destructive | drop table/column | owned-object provenance, affected queries, row-count evidence when live |
| data-dependent | unique index with duplicates; nullable → not-null with nulls | static warning plus optional live/fixture probe |
| constraint loss | drop FK/check/unique | policies/contracts that relied on the guarantee |
| deployment incompatible | rename/type narrowing | old/new query compatibility and expand/contract deployment advice |

Command shape:

```text
jails migrate lint [--since <git-ref>] [--offline|--live] [--output human|json]
```

The offline pass parses ordered migrations and relates `SchemaOp`s to `ResourceKey::Query` claims. The live pass may inspect data in an explicit disposable or development database, but it never asserts facts about production rows it has not seen. A dropped column diagnostic should name the generated/registered queries that still read it and recommend an expand/contract order.

### 3.6 Diagnostics as causal graphs

Spring Boot itself uses `FailureAnalyzer` implementations to turn startup exceptions into a description and action, with the condition evaluation report as a deeper fallback ([Spring Boot startup failures](https://docs.spring.io/spring-boot/reference/features/spring-application.html)). `jails` can go further because it sees source, build files, capabilities, migrations, Compose, and prior generated ownership.

Normalize bounded evidence—including source declarations and captured startup failures—into a graph:

```text
symptom → framework cause → missing/ambiguous declaration → project evidence → fixes
```

Examples:

```text
$ jails why --last
OrderController could not be created
└─ needs OrderService
   └─ needs OrderRepository
      ├─ JdbcOrderRepository exists but is not a bean
      │  hypothesis: no source-visible @Repository or @Bean factory
      │  limitation: profiles, conditions, post-processors and programmatic beans not evaluated
      └─ InMemoryOrderRepository is test-scoped
fix:
  jails add db --pretend
  or annotate exactly one production adapter
```

```text
$ jails why migration V014
V014 failed at statement 3: foreign key target users(id)
└─ users.id is uuid
└─ orders.user_id is bigint
origin: Order.user:ref resolved from live catalog at schema digest 7a1...
fix: jails schema change Order user:uuid --pretend
```

Command family:

- `jails why [log-file|--last|bean <name>|migration <version>|query <name>] [--evidence] [--output json]`
- `jails doctor [--scope env|build|schema|wiring|ownership|all] [--output json]`
- `jails routes [--conflicts] [--openapi] [--output json]`
- `jails beans [--missing] [--cycles] [--path <bean>] [--output dot|json|human]`
- `jails explain <command|artifact|operation-id> [--why-generated]`

Static versus runtime evidence must be labelled. Source-only `routes` is fast and works on a broken app, but cannot know runtime-conditional mappings. `jails` does not simulate Spring's `BeanFactory`, conditions, profiles, SpEL, post-processors, proxies, or context caching. Runtime conclusions may only come from application output the user already ran/captured or an explicitly invoked ordinary project test; `doctor`/`why` never boot an app or install a diagnostic runtime. Typed fixes remain inert canonical-request data for an explicit later preview action—diagnostic commands themselves are read-only.

### 3.7 Architectural fitness gates

ArchUnit analyzes Java bytecode and exposes rules for packages, layers, slices, cycles, and onion/hexagonal architectures ([ArchUnit guide](https://www.archunit.org/userguide/html/000_Index.html)). A scaffold should generate one project-level `ArchitectureTest`, not one duplicate per entity.

Rules for the default layout:

- domain depends only on the JDK, JSpecify annotations, and domain packages;
- application ports may depend on domain, not Spring or adapters;
- web/JDBC/messaging adapters may depend inward;
- adapters do not depend on one another;
- slices under the configured slice package are cycle-free;
- only adapter/configuration packages use Spring stereotypes;
- raw JDBC appears only in JDBC adapters;
- controllers return response DTOs, not repository/JDBC types.

For adoption into an existing project, generate a baseline file that records existing violations and fail only on new ones. ArchUnit's `FreezingArchRule` validates this incremental adoption pattern, but `jails` should keep the baseline in an explicit, reviewable project file rather than hide it in machine-local state.

Legitimate shared kernels and cross-slice ports are explicit configuration, never blanket package exclusions:

```toml
[[architecture.allow]]
from = "billing"
to = "shared"
packages = ["com.acme.shared.money.."]
reason = "Money is the reviewed shared-kernel value"
```

Each allowance needs `from`, `to`, a bounded package pattern, and a non-empty reason. The generated project-level test verifies the allowance is used; an unused allowance fails so stale waivers are removed. Wildcards that match the base package or every slice are refused.

### 3.8 Reversibility after commit

Crash recovery and user undo are different:

- an **active** journal always rolls forward to a coherent commit;
- `jails undo <receipt>` creates a **new forward transaction** whose desired images are the prior preimages;
- undo rechecks that current files still match the original receipt's after-images;
- user changes after the commit cause a three-way merge or refusal;
- a receipt that includes migration history or an external effect is not file-undoable as a unit because the tool cannot prove whether another process applied or observed it;
- refusal emits a forward corrective-plan outline and leaves every file and database untouched.

Generated migrations contain no executable or commented down migration. Database evolution is forward-fix only. A receipt records `undo_eligible = false` plus a stable reason for migration/external-effect transactions; it does not carry a database inverse candidate. This prevents a friendly file command from producing code/schema split brain or local data loss.

### 3.9 Generated HTTP contracts and compatibility checks

FastAPI, Huma, Fuego, Goa, and utoipa all exploit one typed declaration to drive validation and OpenAPI. `jails` already knows the controller intent, request/response records, constraints, routes, status codes, and problem types it generates, so it can emit a contract without a production annotation processor or documentation runtime.

```text
jails contract emit [--format openapi|json-schema] [--out <path>]
jails contract check --against <git-ref|file> [--output human|json]
```

`check` fails on removed paths/operations/responses, newly required inputs, narrowed types, removed enum values, and stricter auth policy unless an explicit compatibility rule permits the change. It distinguishes three scopes:

- `declared`: contract projected from canonical generator intent;
- `source-observed`: declared plus routes/types the static Java index can prove;
- `runtime-observed`: optional isolated application observation.

The emitted document states its scope. It must never imply that a generated-only contract includes arbitrary hand-written or runtime-conditional endpoints.

---

## Section 4: Pillar 3 — Ultra-High-Velocity Authoring

### 4.1 A backward-compatible slice DSL

Keep the existing compact field tokens and extend them through a typed grammar rather than ad hoc suffix checks:

The CLI form is deliberately shell-safe. Unquoted tokens contain no braces, angle brackets, brackets, parentheses, pipes, spaces, glob characters, or arbitrary SQL:

```ebnf
field       = name, ":", cli-type, [ optionality ], { annotation }, [ default ] ;
cli-type    = builtin | java-type | enum-type | reference ;
optionality = "?" | "!" ;
enum-type   = "enum.", enum-value, { ".", enum-value } ;
reference   = "ref.", entity, ".", field-name
            | "ref.", slice, ".", entity, ".", field-name ;
annotation  = "@", ("pk" | "scope" | "index" | "unique" | "audit" | "positive" | "nonnegative") ;
default     = "=", shell-safe-literal ;
```

`shell-safe-literal` is limited to letters, digits, `_`, `.`, `:`, `+`, and `-`. Values outside that alphabet, collections, composite keys/relations, join tables, database expressions, policies, and custom types use structured manifest fields. The conformance suite passes every documented CLI example through Bash, Zsh, Fish, PowerShell, and direct argv construction and asserts identical tokens.

Proposed command:

```text
jails generate scaffold Order \
  id:uuid@pk \
  accountId:uuid \
  total:decimal@positive \
  status:enum.PENDING.PAID.CANCELLED=PENDING \
  createdAt:instant@audit \
  --index status,createdAt \
  --with-events OrderPaid \
  --with-audit
```

Parsing produces stable values such as `FieldSpec`, `RelationSpec`, `IndexSpec`, and `EventSpec`. Decimal amount plus currency is modeled explicitly or as a separately generated `Money` value; `money` is not a magic field type and never silently chooses scale, currency, cents, or floating point.

The command must print normalization in debug/JSON output:

```text
accountId:uuid → column account_id uuid not null
total:decimal@positive → BigDecimal + configured numeric scale + check(total > 0)
```

### 4.2 Contexts and slices, not CRUD bags

Phoenix's context generator makes the domain boundary a first argument and augments an existing context with conflict prompts ([`mix phx.gen.context`](https://phoenix.hexdocs.pm/Mix.Tasks.Phx.Gen.Context.html)). `jails` should allow:

```text
jails g scaffold Billing.Order ...
jails g scaffold Support.Order ...
```

The same noun may exist in different slices. Package layout, ports, migrations, and route prefixes derive from the slice. Cross-slice references require an explicit port or event; a generated ArchUnit rule enforces it. The existing app manifest records the mapping so singularization or package guessing is never the durable identity.

### 4.3 Schema-first: `pull`, `introspect`, and `schema diff`

Prisma's `db pull` reads a live schema into a model and preserves a defined subset of manual schema customizations on repeated introspection ([Prisma introspection](https://www.prisma.io/docs/orm/prisma-schema/introspection)). jOOQ demonstrates the Java value of reverse-engineering types and constraints from a real database so schema changes become compile failures ([jOOQ code generation](https://www.jooq.org/doc/latest/manual/code-generation/)). PostgREST shows which catalog facts matter for an instant API—tables, columns, primary/foreign keys, functions, and relationships—and why caching them matters ([PostgREST schema cache](https://docs.postgrest.org/en/v16/references/schema_cache.html)).

`jails` should separate observation from mutation:

```text
jails introspect db [--schema public] [--table glob] [--output human|json|manifest]
jails pull [--schema public] [--table glob] [--into-slice Billing] --pretend
jails schema diff [--from declared|migrations|live] [--to ...]
jails schema check [--frozen]
```

`introspect` is read-only. `pull` creates or reconciles declarations and generated code through the normal prepared bundle. First pull establishes a baseline. Later pulls preserve aliases, ignored objects, naming overrides, and owned customization in `.jails/app.toml`.

Catalog model minimum:

```text
Catalog
  schemas
    tables/views
      columns(type, nullability, default, identity/generated, comment)
      primary/unique/check constraints
      indexes (columns, expressions, predicate, method)
      foreign keys (columns, target, on update/delete, deferrability)
    enums/domains
    routines relevant to queries
```

Unsupported vendor constructs become explicit `Opaque` nodes carried through diffs. They are never dropped because the parser did not understand them.

### 4.4 SQL-first workflow

The complete workflow is:

```text
1. Write/edit migrations or pull a catalog.
2. Write named `.sql` queries.
3. `jails sql check` resolves parameters/results against a catalog.
4. `jails sql generate --pretend` shows Java/contract changes.
5. Apply through the transaction engine.
6. `jails testd --affected` runs generated contracts and callers.
7. CI runs `jails sql check --frozen --live` against a clean migrated DB.
```

sqlc's named query commands (`:one`, `:many`, `:exec`) are worth adapting because cardinality is a contract, not something inferable from arbitrary SQL. sqlc also expands `SELECT *` to explicit columns in generated code to prevent unexpected result drift ([sqlc selecting rows](https://docs.sqlc.dev/en/stable/howto/select.html)). Proposed annotations:

```sql
-- name: FindOrder :optional
-- domain: Order
-- timeout: 100ms

-- name: CreateOrder :one
-- transaction: required

-- name: CancelOrder :execrows
-- expect-rows: 1
```

The generator should reject `SELECT *` by default for stored contracts, require stable ordering for pageable/many queries, require a bound limit or explicit `--unbounded`, and flag writes without an expected row count.

Static named SQL is the default, not the whole query story. Optional search criteria use a closed structured declaration, never raw Java string concatenation or an exponential set of hand-written files:

```toml
[[query]]
name = "SearchOrders"
slice = "Billing"
from = "orders"
select = ["id", "account_id", "total", "status", "created_at"]

[[query.filter]]
parameter = "status"
column = "status"
operator = "eq"
optional = true

[[query.filter]]
parameter = "minimumTotal"
column = "total"
operator = "gte"
optional = true

[[query.sort]]
name = "created"
columns = ["created_at", "id"]
```

V1 criteria operators are a closed set (`eq`, `neq`, `lt`, `lte`, `gt`, `gte`, `prefix`, `in`) validated against catalog types. Sorts are named, declared column lists; request data can never become an identifier or SQL fragment. The generated adapter appends only generator-owned constant fragments, binds every value, and enforces a bounded limit. Offline verification checks the base and every fragment against one catalog; live contract tests exercise the empty set, each individual filter, all filters, every sort, and null/empty-list boundaries. A query needing joins, CTEs, vendor operators, correlated subqueries, or arbitrary reporting shape stays reader-owned named SQL. An opt-in project may use jOOQ directly, but `jails` does not add or wrap a production query-builder runtime.

Explicit mappers are generated boilerplate, not hand-maintained ceremony. Flat results use a generated `RowMapper`. A declared one-to-many projection must name ordered parent/child keys and generates a `ResultSetExtractor` fold with immutable final records; absent grouping metadata is refused rather than guessed. `jails` does not promise general object-graph hydration.

### 4.5 Validation as an explicit boundary

Ecto changesets separate external casting/filtering, application validation, and database constraints ([Ecto Changeset](https://ecto.hexdocs.pm/Ecto.Changeset.html)). Generated Java should preserve that separation without introducing a changeset runtime:

- domain record compact constructor: intrinsic invariants that are true everywhere;
- web request record: transport parsing and Jakarta validation;
- application command/use case: authorization and state-transition rules;
- application mutation service: one visible `@Transactional` boundary around the repository changes and outbox write; repositories and domain records do not open transactions themselves;
- database migration: unique, foreign-key, and check constraints;
- exception advice: stable RFC 9457 problem responses.

This prevents the common generator error of putting every rule on a persistence entity or duplicating inconsistent checks across controller and repository.

### 4.6 Fakes, factories, seeds, and contracts

Laravel's factories provide reusable defaults, named states, sequences, and relationship composition; its test helpers reset state and can run seeders ([Laravel factories](https://laravel.com/framework/docs/13.x/eloquent-factories), [database testing](https://laravel.com/framework/docs/13.x/database-testing)). Ecto's SQL Sandbox uses explicit connection ownership and transaction rollback to enable concurrent PostgreSQL tests ([Ecto SQL Sandbox](https://ecto-sql.hexdocs.pm/Ecto.Adapters.SQL.Sandbox.html)). Adapt the principles, not their runtimes:

- generate `OrderFactory` in test sources with deterministic defaults, `.paid()`, `.cancelled()`, and `.withUserId(...)` states;
- generate a thread-safe `InMemoryOrderRepository` implementing the same port, with deterministic ordering and uniqueness behavior;
- generate JSON fixtures for readable stable examples and Java factories for combinatorial tests;
- generate seed data in `db/seeds/*.json` plus a plain Java `SeedRunner` that uses repository ports; production execution requires an explicit profile/flag;
- use `@Transactional` rollback for in-process integration tests and unique schema/database names for tests that spawn threads or commit independently;
- generate a repository contract interface executed once against the fake and once against `JdbcOrderRepository`, so semantic drift becomes a failing test.

The fake must document what it does not emulate: locking, isolation, vendor collation, constraint timing, SQL planner behavior. Those remain live-database contract tests.

### 4.7 Policy and contract generation

Add optional matrices to the manifest:

```toml
[[entity]]
name = "Order"
slice = "Billing"

[[entity.policy]]
action = "read"
allow_roles = ["SUPPORT", "BILLING"]
owner_field = "userId"
principal_claim = "userId"

[[entity.event]]
name = "OrderPaid"
version = 1
fields = ["id", "userId", "total", "paidAt"]
```

This closed form means “permit when the principal has an allowed role or when `owner_field` equals `principal_claim`.” V1 has no expression string, SpEL passthrough, function call, negation, or user-defined evaluator. Generate a sealed policy decision type, explicit authorizer port, table-driven unit tests, Spring adapter configuration, event record, JSON Schema/OpenAPI component, and producer/consumer contract tests. Unsupported policy logic remains ordinary hand-written code behind the authorizer port. A policy matrix is high-risk: `--pretend` should summarize added/removed permissions separately from ordinary file edits.

### 4.8 JDK 26 default with a Java 21 compatibility floor

Modern syntax should make generated decisions easier to read, not become a compatibility trick:

- records are the default for domain values, commands, query parameters/results, events, request/response DTOs, and problem extensions;
- sealed interfaces model genuinely closed outcomes—such as `PolicyDecision.Permit` / `Deny` or `TransitionResult.Applied` / `Conflict`—and Java 21's finalized [pattern matching for `switch`](https://openjdk.org/jeps/441) turns a newly generated outcome into a compile error at every unhandled adapter;
- pattern matching for `switch` belongs at explicit translation boundaries (domain outcome → HTTP response/event), not in reflection-like registries that discover behavior implicitly;
- JSpecify `@NullMarked` is generated at package boundaries; nullable database/transport facts use `@Nullable`, while domain optionality remains a deliberate type/invariant decision;
- [virtual threads](https://openjdk.org/jeps/444) are a project-owned runtime choice, not a generator default. Diagnostics may explain their interaction with blocking JDBC and the connection pool, but generation does not install a scheduler or silently change execution policy;
- ordinary executors, `JdbcClient`, Spring MVC, and JDK types remain visible. `jails` must not generate a scheduler, continuation wrapper, or concurrency runtime of its own.

For example, a generated sealed application result can be translated exhaustively:

```java
return switch (service.cancel(id)) {
    case CancelResult.Cancelled(var order) -> ResponseEntity.ok(OrderResponse.from(order));
    case CancelResult.NotFound(var missingId) -> throw new OrderNotFoundException(missingId);
    case CancelResult.AlreadyPaid(var order) -> ResponseEntity
            .status(HttpStatus.CONFLICT)
            .body(OrderResponse.from(order));
};
```

JDK 26 is a six-month feature release, not an LTS release. Choosing it here is a deliberate new-project default, not a claim that an adopted project's deployment policy should change. The release policy is exact:

1. `jails new` and `jails new-cli` default Maven and Gradle compiler settings to `--release 26`; generated acceptance fixtures use the same default.
2. An explicit `--java` or `--release` value of 21 or newer overrides that default. Releases below 21 are refused.
3. An adopted project retains the release observed from its build model. No add/generate/check command silently upgrades it.
4. The planner verifies every selected Java feature against that observed release. An unsupported feature produces a diagnostic naming the construct, required release, configured release, and a typed, separately previewable upgrade request when one is available.
5. The default path requires a host compiler that supports release 26. `doctor` reports the selected executable and release, and offers a platform-appropriate typed fix such as configuring `java@26`; strict CI never skips a compiler gate because the host JDK is old.
6. Preview language features are never generated by default. A future preview mode would require an explicit project setting, matching compile/test/run flags, and its own compatibility tests.

The coordinated default change includes `jails-project::pom::TARGET_RELEASE`, CLI help/defaults, Maven and Gradle templates, generated-project gates, toolchain metadata, examples, and fixtures. A partial update is a release blocker because it creates projects that report one release and compile with another.

### 4.9 Manifest-first editor workflow

Complex application authoring starts as ordinary TOML/JSON in the existing app manifest. The editor protocol supplies schema/capability negotiation, diagnostics, symbols, preview, and exact-plan apply; Neovim can provide forms or pickers without introducing another persisted model. A separate domain-model TUI is deferred until measured editor use proves that text editing plus structured diagnostics is the bottleneck. If it is ever built, deterministic manifest replay and planner equivalence remain mandatory gates.

### 4.10 Rails-grade access to the running application and its tools

Rails makes the project CLI the shortest path into the application: its console loads the full application environment, `dbconsole` selects the configured database client, and `runner` executes one-off code in that environment ([Rails command-line guide](https://guides.rubyonrails.org/command_line.html#interacting-with-a-rails-application)). `jails` should provide that cohesion while retaining standard Java and native tools:

```text
jails request GET /api/orders --query status=PENDING
jails request POST route:POST:/api/orders:com.acme.billing.OrderController#create \
  --json @request.json --header-env Authorization=DEV_AUTHORIZATION
jails db console [--database primary] [--client pgcli|psql]
jails console [--profile dev] [--main com.acme.Application]
jails runner --file scripts/reprice-orders.jsh [--profile dev]
jails logs [service] [--follow] [--since 10m]
```

`jails request` resolves a declared route or literal path and launches the installed `curl`; it does not implement HTTP. The default curl policy is `--silent --show-error --fail-with-body`, so an HTTP error retains its useful response body while returning failure ([curl manual](https://curl.se/docs/manpage.html#--fail-with-body)). Route parameters, query pairs, JSON/file/stdin bodies, environment-backed headers, timeouts, and an exact `--print` mode remove quoting and endpoint lookup ceremony. It never starts the application.

`jails db console` resolves one explicit datasource and launches `pgcli` by default. `pgcli` already supplies PostgreSQL-aware completion and highlighting and accepts the standard `PGHOST`, `PGPORT`, `PGUSER`, `PGPASSWORD`, and `PGDATABASE` environment variables ([pgcli](https://www.pgcli.com/)). Credentials therefore stay in the child environment rather than a URI or argument visible in process listings. Missing `pgcli` is a typed `tool-unavailable` error with install guidance; fallback to `psql` is explicit through `--client psql`, never silent.

`jails console` is an application-aware REPL, not bare JShell. It resolves the Maven or Gradle runtime classpath, requires current compiled output, then launches the selected JDK's JShell with a private startup script. That script boots the selected Spring Boot main class with the explicit profile in the JShell execution JVM and defines `ctx`, `bean(Class)`, `bean(String)`, `beans()`, `env()`, and `tx(Supplier<T>)`. JShell is the JDK-standard interactive evaluator and supports startup scripts directly ([JDK 26 JShell guide](https://docs.oracle.com/en/java/javase/26/jshell/), [startup scripts](https://docs.oracle.com/en/java/javase/26/jshell/scripts.html)). There is no generated production dependency or proprietary REPL.

Console boot is an explicit side-effecting action: application initializers and beans may connect to configured external systems. The CLI prints the main class, project release, active profiles, web mode, and redacted datasource source before confirmation when the profile is not `dev` or `test`. It does not start Compose or Testcontainers. V1 deliberately has no `--sandbox`: a root JDBC transaction cannot truthfully roll back `REQUIRES_NEW`, after-commit events, other threads, remote calls, or multiple datasources. `tx(...)` is an explicit per-expression helper using the application's `PlatformTransactionManager`, not a session-wide safety claim.

`jails runner` uses the same boot and helper contract but evaluates a project-relative `.jsh` file non-interactively and exits. Inline source is omitted from v1 to avoid shell-history leakage and quoting ambiguity; stdin is accepted only with `--file -`. `jails logs` is a read-only projection of the committed Compose service declaration and delegates to the selected Compose implementation; it cannot create, restart, reset, or remove a service.

All of these commands are transparent process adapters: they inherit the terminal when interactive, forward signals, preserve the child exit code, show a redacted exact invocation under `--debug`, and do not capture an unbounded session transcript. `doctor` probes tool presence and versions but never installs them. Section 7.16 fixes their APIs and execution rules.

### 4.11 Coordinated resource rename

A resource name participates in more than a Java filename. It may determine the domain type, request/response records, repository port, adapter, factory, architecture rules, route label, table binding, migrations, named-query ownership, SQL text, contract snapshots, test selectors, and editor symbol IDs. A correct rename operates on that dependency set through one stable entity identity.

```text
jails rename resource Billing.Task WorkItem --strategy preserve-table
jails rename resource Billing.Task WorkItem --strategy single-cutover
jails rename resource Billing.Task WorkItem --strategy rolling
jails rename storage Billing.WorkItem --complete <campaign-id> --old-version-retired
```

The strategies make storage intent explicit:

- `preserve-table` renames the logical/generated Java resource while persisting `table = "tasks"` as an intentional physical-name override. The old table is not left as an unexplained convention mismatch.
- `single-cutover` additionally creates a new forward Flyway migration that renames `tasks` to the normalized target (for example `work_items`) and updates verified SQL/contracts in the prepared after-state. It is marked deployment-incompatible because old and new application versions cannot safely overlap.
- `rolling` performs the safe code stage first with the old table explicitly bound and records a versioned rename campaign in the committed manifest. After the old application version is retired, `rename storage --complete` creates the forward physical rename migration and clears the campaign. It does not guess that a compatibility view is writable for every query or `ON CONFLICT` form.

External contracts are a separate axis. Routes, JSON property names, event names, and event field names remain stable by default. Renaming them requires an explicit contract option, produces compatibility evidence, and follows the project's breaking-change policy. Applied migrations are never edited, and no reverse migration is generated.

Owned generated files/regions can be moved and regenerated transactionally. Reader-owned named SQL and hand-written Java are not rewritten by a home-grown AST or text substitution. The planner reports exact symbol/query references as `manual-edit-required`; the user edits those sources, then reruns the rename so compilation and SQL verification can validate them against the prepared after-state. Unknown reflection/string references widen discovery, and opaque database dependencies block the physical rename. Section 7.17 defines the durable identity, request, stage, and refusal contracts.

### 4.12 Resource evolution, destruction, and recovery

A scaffold is not one disposable file bundle once its migration may have run. It has at least three lifetimes:

1. generated application projections can be retired or regenerated;
2. the logical `EntityId`, table binding, and retirement state must remain addressable;
3. versioned migration history is append-only and remains in source even after the resource is retired.

Consequently, a committed migration is sealed at the first successful receipt. Later generation may read it as schema history but cannot replace or delete it. Adding or changing a field creates the next forward migration; destroying a scaffold retains earlier migrations and requires an explicit storage policy. A physical drop is a new migration, never deletion of the create migration.

The intended commands are discoverable and recoverable:

```text
jails resource status Task [--datasource primary]
jails resource field add Task assigneeId:uuid?
jails resource field rename Task assigneeId ownerId --column preserve
jails resource field type Task priority --to long --strategy safe
jails resource field nullability Task description --required \
  --backfill-file db/backfills/task_description.sql
jails resource field drop Task legacyCode --confirm-column legacy_code

jails destroy scaffold Task --storage preserve
jails destroy scaffold Task --storage drop --confirm-table tasks
jails destroy scaffold Task --storage drop --confirm-table tasks \
  --migrate --datasource primary
jails resource revive Task --table tasks
jails resource repair Task --strategy roll-forward [--datasource primary]
```

Plain `destroy scaffold` refuses when the resource has storage because silently preserving and silently dropping are both surprising. `--storage preserve` retires generated code while recording the existing table as intentionally preserved. `--storage drop` retires code and appends a forward drop migration; it reports that the table is only `drop-planned` until the normal migration command applies it. Adding `--migrate --datasource primary` is the explicit “drop it from this configured database now” form: it uses the post-commit effect machinery, lists every pending migration before confirmation, and retains a retryable receipt if the external effect fails.

`resource revive` reactivates the same stable identity and adopts its preserved table without emitting another `CREATE TABLE`. It refuses an unverified or colliding table. `resource repair --strategy roll-forward` compares sealed migration bytes, current source, ledger/receipts, the migration-derived catalog, and—when explicitly supplied—Flyway history plus live catalog. Its normal repair restores an edited/deleted sealed migration from the object store, represents the desired delta in a new migration, and regenerates only owned projections. It never calls Flyway repair, changes a recorded database checksum, deletes live data, or chooses whether current Java or live schema is authoritative without showing that choice. Section 7.19 specifies the state machine and refusal cases.

---

## Section 5: Cross-Ecosystem Pattern Translation Matrix

Impact codes: **L** = feedback latency, **C** = correctness/trust, **A** = authoring velocity. This is an evidence catalogue, not the roadmap: a row is implementable only when Section 7 assigns it a work package and conformance gate. “Adapt” means extract the narrow mechanism into generated standard Java or CLI behavior; it does not mean adding the source framework as a runtime dependency. Frontend, deployment, BaaS, and infrastructure rows are comparison evidence and are rejected for the core unless a later RFC changes the hard exclusions.

| Source ecosystem / tool | Core DX innovation | `jails` adaptation for Java | Affected crate(s) | Impact |
|---|---|---|---|---|
| Ruby / [Rails](https://github.com/rails/rails) | Convention-led scaffold hooks; migrations; seeds; console | Composable slice recipes, global `--pretend`, explicit migration/seed artifacts, `jshell` classpath console | spec, generate, prepare, drive | L/C/A |
| Ruby / [Hanami](https://github.com/hanami/hanami) | Explicit slices and dependency boundaries | First-class slice identity, inward ports, cross-slice events, generated ArchUnit rules | protocol, spec, generate | C/A |
| Ruby / [Sinatra](https://github.com/sinatra/sinatra) | Minimal route declaration | Small route/controller generator with no new runtime abstraction | spec, generate | A |
| Ruby / [Roda](https://github.com/jeremyevans/roda) | Routing tree; branch-level middleware hoisting | Compile route/policy trees into ordinary Spring mappings and shared interceptor configuration | spec, generate, project | C/A |
| Ruby / [Kamal](https://github.com/basecamp/kamal) | Small, explicit zero-downtime deployment primitives | Generate inspectable container/deployment files and preflight commands; do not build a deploy runtime | generate, drive, report | C/A |
| Ruby / [Turbo](https://github.com/hotwired/turbo) | Server-driven partial UI updates | Optional generation of standard Spring MVC + HTMX/Turbo-compatible fragments; no `jails` browser runtime | generate | A |
| PHP / [Laravel](https://github.com/laravel/framework) | Dense Artisan generators, factories, seeders, Tinker | Command aliases, deterministic factories/states, seed plans, richer console completion | spec, generate, report, drive | L/A |
| PHP / [Filament](https://github.com/filamentphp/filament) | Resource schema drives forms/tables/actions | Use the slice model to generate admin API contracts and terminal resource summaries, not a web GUI | spec, generate, report | A |
| PHP / [Livewire](https://github.com/livewire/livewire) | Server state and automatic DOM diffs | Generate explicit SSE/HTMX endpoints where requested; reject implicit component runtime coupling | generate | A |
| Python / [Django](https://github.com/django/django) | Whole-project state diff and ordered migration operations | Schema graph differ, dependency ordering, rename candidates, migration preview/check | spec, project, prepare, generate | C/A |
| Python / [FastAPI](https://github.com/fastapi/fastapi) | One typed declaration drives validation and OpenAPI | `FieldSpec` drives domain/request validation, ProblemDetail, OpenAPI components, examples | spec, generate | C/A |
| Python / [Reflex](https://github.com/reflex-dev/reflex) | High-level declarations compile to another stack | Treat manifests as compiler inputs, but emit plain Java and standard assets with no `jails` runtime | protocol, spec, generate | A |
| Python / [Alembic](https://github.com/sqlalchemy/alembic) | Extensible metadata/database diff; candidate migrations | Typed `SchemaOp` IR, dialect comparators, mandatory review for destructive/ambiguous changes | spec, project, prepare | C/A |
| Database tooling / [Atlas](https://github.com/ariga/atlas) | Migration linting separates destructive, data-dependent, constraint, and incompatibility risks | `migrate lint` relates typed schema operations to owned queries and optional live row evidence | protocol, project, drive, report | C/A |
| Elixir / [Phoenix](https://github.com/phoenixframework/phoenix) | Context generator makes API boundaries explicit | `Slice.Entity` syntax, augment-existing flow, scoped resource generation | spec, generate, engine | C/A |
| Elixir / [Phoenix LiveView](https://github.com/phoenixframework/phoenix_live_view) | Minimal state diffs over a persistent connection | Generate ordinary SSE/WebSocket ports/controllers and contracts, without a hidden state runtime | generate | A |
| Elixir / [Ecto](https://github.com/elixir-ecto/ecto) | Changesets and concurrent transactional SQL sandbox | Separate transport/domain/DB validation; transaction-isolated repository tests and explicit connection ownership | generate, testkit, drive | L/C |
| JS/TS / [AdonisJS](https://github.com/adonisjs/core) | Ace CLI, validated prompts, stubs plus AST codemods | Testable prompts, typed edit IR, ejectable templates with placeholder contracts | CLI, java, generate, prepare | C/A |
| JS/TS / [RedwoodJS](https://github.com/redwoodjs/redwood) | Full-stack vertical slice generation and Cells | Generate server-side slice plus OpenAPI/client contract boundary; adapt declarative loading/error states to tests | spec, generate | A |
| JS/TS / [CedarJS](https://github.com/cedarjs/cedar) | Continued Redwood-style generators and conventions | Track the maintained conventions independently; keep `jails` recipe/version compatibility explicit | spec, generate | C/A |
| JS/TS / [Wasp](https://github.com/wasp-lang/wasp) | Typed declarative application spec for routes/auth/jobs | Evolve `jails app` into the durable multi-slice authoring surface and TUI target | protocol, spec, engine, generate | C/A |
| JS/TS / [Prisma](https://github.com/prisma/prisma) | Schema DSL, generated client, migrations, repeated introspection | `pull`/`schema diff`, preserved naming overrides, checked-in SQL contracts; generate raw JDBC rather than a client runtime | spec, project, generate, prepare | C/A |
| JS/TS / [Next.js](https://github.com/vercel/next.js) | File conventions and co-located server actions | Optional file/package conventions and co-located HTTP request collections; avoid implicit runtime boundary magic | spec, generate | A |
| JS/TS / [HTMX](https://github.com/bigskysoftware/htmx) | Hypermedia actions declared in HTML attributes | Generate standard MVC fragment endpoints and contract examples when selected | generate | A |
| JS/TS / [tRPC](https://github.com/trpc/trpc) | End-to-end type inference with tiny authoring surface | Generate OpenAPI/JSON Schema and typed client fixtures from Java contracts; compile mismatch rather than share a runtime | spec, generate | C/A |
| Go / [sqlc](https://github.com/sqlc-dev/sqlc) | SQL is source; catalog analysis emits typed structs/methods | Named SQL contracts → Java records, explicit `JdbcClient` binders/mappers, fakes and checks | spec, generate, drive | L/C/A |
| Go / [Encore](https://github.com/encoredev/encore) | Static analysis derives infrastructure and tracing | Infer *candidates* from explicit annotations/capabilities, show plan, generate Compose/config/telemetry code | project, generate, report | C/A |
| Go / [Ent](https://github.com/ent/ent) | Schema-as-code graph traversal and migrations | Relation graph in canonical manifest; generate ports and explicit SQL joins, not an ORM runtime | protocol, spec, generate | C/A |
| Go / [Bun](https://github.com/uptrace/bun) | SQL-first builder with struct mapping | Keep SQL visible and generate explicit mapper lambdas; offer small query-building helpers only as source | generate | C/A |
| Go / [templ](https://github.com/a-h/templ) | Type-checked templates compile to functions | Validate owned templates at generation and compile generated Java views/components in verification gates | java, generate, drive | C/A |
| Go / [Huma](https://github.com/danielgtaylor/huma) | Type signatures drive validation and OpenAPI | One request/response spec generates Spring records, validation, examples, and OpenAPI | spec, generate | C/A |
| Go / [Fuego](https://github.com/go-fuego/fuego) | Generics/types drive web contracts | Same Huma/FastAPI transfer, constrained to generated standard Spring MVC | spec, generate | C/A |
| Go / [Goa](https://github.com/goadesign/goa) | Design-first API DSL emits servers, clients, docs | Add API intent to manifests and generate controllers, client contracts, docs and tests | protocol, spec, generate | C/A |
| Go / [PocketBase](https://github.com/pocketbase/pocketbase) | One binary supplies a complete local backend | Preserve `jails` single-binary installation and offer coherent capability bundles, while generated apps stay ordinary Java | CLI, generate, drive | L/A |
| Go / [Goravel](https://github.com/goravel/goravel) | Laravel-like structure and CLI in Go | Borrow command density, aliases, factories, jobs and migration naming—not facades/runtime service location | CLI, spec, generate | A |
| Go / [Air](https://github.com/air-verse/air) | Custom-trigger rebuild/restart loop | Apply its debounce/overflow lessons only to `testd --watch`; application restart remains `run --watch`/DevTools | drive | L |
| Go / [goose](https://github.com/pressly/goose) | Embeddable, ordered SQL/function migrations | Ordered SQL migration inspection and checksum gates; keep Java migration functions optional and explicit | project, drive, report | C |
| Go / [Testcontainers-Go](https://github.com/testcontainers/testcontainers-go) | Ephemeral real infrastructure in tests | Generate equivalent Testcontainers Java configuration and service contracts | generate, testkit | C/A |
| Rust / [Loco](https://github.com/loco-rs/loco) | Rails-style scaffolds; migration intent inferred from names | Optional friendly migration-name parser that prints its normalized `SchemaOp` before generation | spec, generate | A |
| Rust / [Axum](https://github.com/tokio-rs/axum) | Typed extractors composed with Tower middleware | Generate explicit request types and standard Spring interceptors/filters from typed route policy | spec, generate | C/A |
| Rust / [SQLx](https://github.com/launchbadge/sqlx) | Live/offline compile-time SQL checking | Live and frozen catalog verification levels with CI staleness check | spec, drive, report | L/C |
| Rust / [SeaORM](https://github.com/SeaQL/sea-orm) | Entity generation from live schemas | `jails pull` catalog → domain/port/adapter generation without shipping ORM entities | project, spec, generate | A |
| Rust / [Diesel](https://github.com/diesel-rs/diesel) | Compile-time query/schema compatibility | Turn catalog mismatch into generator/CI errors and Java compile breakage through generated types | spec, generate, drive | C |
| Rust / [utoipa](https://github.com/juhaku/utoipa) | AST-derived OpenAPI | Static source index produces OpenAPI and compares it to generated contract baseline | java, report, generate | C/A |
| Rust / [Leptos](https://github.com/leptos-rs/leptos) | Fine-grained reactivity and full-stack contracts | Adapt dependency granularity to invalidation/test selection; optional standard SSE/HTML generation | java, drive, generate | L/A |
| Rust / [Dioxus](https://github.com/DioxusLabs/dioxus) | Hot RSX iteration | Adapt file-trigger and fine-grained rebuild UX, not its UI runtime | drive | L |
| Rust / [Shuttle](https://github.com/shuttle-hq/shuttle) | Infrastructure requested through code declarations | Convert source annotations to explicit proposed capabilities and previewed generated infrastructure | project, generate, prepare | C/A |
| Zig / [Jetzig](https://github.com/jetzig-framework/jetzig) | Build-time routes/templates and integrated primitives | Precompute route/template facts in CLI cache; emit simple Java | java, generate, drive | L/A |
| Zig / [http.zig](https://github.com/karlseguin/http.zig) | Minimal allocation-conscious request lifecycle | Generate bounded, explicit request handling and streaming; surface allocation/perf tests in optional profiles | generate, testkit | C |
| Zig / [Zap](https://github.com/zigzap/zap) | Thin high-performance HTTP wrapper | Keep generated controller adapters thin and benchmarkable | generate, drive | L/C |
| Zig / [Ziex](https://github.com/ziex-dev/ziex) | Embedded JSX compiled at build time | If views are added, compile checked templates to ordinary Java methods/resources | java, generate | C/A |
| Zig / [zzz](https://github.com/tardy-org/zzz) | io_uring-oriented async server | Research signal for generated load/streaming profiles only; no attempt to replace Spring I/O | drive, generate | L |
| JVM / [Spring Boot](https://github.com/spring-projects/spring-boot) | starters, auto-configuration, FailureAnalyzers, DevTools | Generate conventional Boot configuration; inspect conditions; let DevTools own restart and keep service selection explicit | project, generate, drive, report | L/C/A |
| JVM / [Spring Data REST](https://github.com/spring-projects/spring-data-rest) | Repository-to-REST exposure | Generate the equivalent controller/service/port code explicitly so API behavior remains visible | generate | A/C |
| JVM / [Quarkus](https://github.com/quarkusio/quarkus) | DevServices and build-time metadata | Reuse its explicit selection/explanation UX for already declared services; do not import absence-triggered provisioning | project, report | L/C |
| JVM / [JHipster](https://github.com/jhipster/generator-jhipster) | JDL multi-entity/relationship authoring | Extend the existing `jails app` manifest only for proved relation/event gaps | protocol, spec, generate | A/C |
| JVM / [jOOQ](https://github.com/jOOQ/jOOQ) | DB reverse engineering and type-safe schema model | Borrow catalog breadth and compile-break-on-schema-change while emitting raw `JdbcClient`, not jOOQ runtime | project, spec, generate | C/A |
| JVM / [Micronaut](https://github.com/micronaut-projects/micronaut-core) | Compile-time DI/AOP metadata | Build a source/bytecode bean graph and generate explicit wiring checks; avoid runtime reflection metadata | java, project, report | L/C |
| JVM / [Ktor](https://github.com/ktorio/ktor) | Coroutine-native routing DSL | Adapt composable route-tree authoring to generated Spring mappings, preserving explicit middleware order | spec, generate | C/A |
| JVM / [ArchUnit](https://github.com/TNG/ArchUnit) | Architecture as executable bytecode rules | Generate one project fitness suite and incremental adoption baseline | generate, testkit, drive | C |
| BaaS / [Supabase](https://github.com/supabase/supabase) | Postgres catalog, auth/storage/realtime as coherent services | Offer explicit capability bundles and schema/policy introspection; never hide service ownership | project, generate, drive | A |
| BaaS / [PostgREST](https://github.com/PostgREST/postgrest) | Cached catalog turns tables/FKs/functions into API shape | Reuse its catalog categories for `pull`, relationship inference and invalidation | project, spec | L/C/A |
| BaaS / [Hasura](https://github.com/hasura/graphql-engine) | Instant GraphQL and relationship reflection | Generate optional GraphQL/OpenAPI adapters from catalog with reviewed auth/policy declarations | spec, generate | A/C |
| BaaS / [Directus](https://github.com/directus/directus) | Live schema introspection and admin resources | Terminal resource explorer and manifest generation; no web admin surface in `jails` | report, spec, generate | A |
| BaaS / [Appwrite](https://github.com/appwrite/appwrite) | Integrated auth/jobs/storage/functions bundle | Coherent, opt-in capability recipes with explicit Compose/config ownership | project, generate | A |
| BaaS / [Nhost](https://github.com/nhost/nhost) | Integrated Postgres/auth/GraphQL/storage | Same capability-bundle lesson; generated ports prevent vendor services entering the domain | project, generate | C/A |

### Patterns deliberately not transplanted

- ActiveRecord/JPA-style runtime entities and lazy loading: they conflict with records, ports, raw SQL, and transparent execution.
- runtime repository-to-API exposure: fast to start, but it hides authorization, DTO, error, and transaction behavior.
- server-held UI component runtimes: outside the CLI/TUI scope; only standard endpoint generation is transferable.
- infrastructure inferred and applied without a plan: static inference may propose capabilities, but the prepared diff remains the authority.
- generator plugins executing arbitrary third-party code: template/data extension is compatible with the trust model; an in-process plugin runtime is not.

---

## Section 6: Concrete CLI Command Specifications

### 6.0 Common command contract

Every mutating command implements one global reporting contract. Human and JSON output exist today; json-v1 is retained only for the v2 compatibility window defined in Section 7.7:

```text
--pretend, --dry-run     prepare and report; do not commit or run external effects
--output human|json|json-v1
                         human or current JSON; json-v1 is a temporary compatibility encoding
--debug                  exact subprocesses, cache decisions, timing spans
```

Add these consistent review flags:

```text
--diff                   expand file-level unified diffs
--ast                    expand the semantic report derived from prepared byte edits
--verify none|fast|full  validation gate before acceptance (default: fast)
--yes                    accept a conflict-free plan non-interactively
--plan-out <file>        write a redacted, content-digested prepared plan
--plan-in <file>         apply only that plan after rechecking all preconditions
```

`--plan-in` never reparses command arguments. A plan is bound to the canonical project root, observed generation, input digests, tool/protocol version, and template digests. `--plan-out` refuses when a required secret cannot be represented as an environment-variable reference.

Exit status convention:

| Code | Meaning |
|---:|---|
| 0 | success, including a truthful no-op |
| 1 | refusal, invalid input, stale input, blocked recovery, verification failure, or external effect failure; inspect the stable error code |
| 2 | prepared merge conflict; inspect the conflict report |

### 6.1 Enhanced `generate scaffold`

```text
jails generate scaffold <Slice.Entity|Entity> <field>...
  [--package <java.package>]
  [--route <path>]
  [--index <field[,field...]>]...
  [--unique <field[,field...]>]...
  [--with-events <Event[,Event...]>]
  [--with-audit]
```

Example:

```text
$ jails g scaffold Billing.Order \
    id:uuid@pk accountId:uuid total:decimal@positive \
    status:enum.PENDING.PAID.CANCELLED=PENDING createdAt:instant@audit \
    --index status,createdAt --with-events OrderPaid --with-audit \
    --pretend --diff

PLAN  Billing.Order  transaction 01K4...
VERIFY fields 5/5 · relations 0/0 · SQL verified-offline · Java compile pending

CREATE  .../billing/domain/Order.java
CREATE  .../billing/application/OrderRepository.java
CREATE  .../billing/adapter/jdbc/JdbcOrderRepository.java
CREATE  .../billing/adapter/memory/InMemoryOrderRepository.java
CREATE  .../billing/service/OrderService.java
CREATE  .../billing/web/OrderRequest.java
CREATE  .../billing/web/OrderResponse.java
CREATE  .../billing/web/OrderController.java
CREATE  .../billing/domain/OrderPaid.java
CREATE  .../db/migration/V014__create_orders.sql
CREATE  ... 9 tests/contracts/fixtures/request examples
EDIT    pom.xml  +ArchUnit test dependency
EDIT    .jails/app.toml  +entity Billing.Order

RISK    additive 20 · behavior-change 1 · destructive 0
NO WRITE (--pretend)
```

The operation list depends on installed capabilities. If Flyway is absent, output says `SKIP migration: no database migration capability`; it does not create dead SQL. A reference such as `accountId:ref.Account.id` must resolve to one stored key or planning fails with candidates and a fix. Policy matrices, composite relations, custom database types, and other complex shapes are manifest-only.

Generated layout:

```text
src/main/java/com/acme/billing/
├── domain/
│   ├── Order.java
│   ├── OrderStatus.java
│   └── OrderPaid.java
├── application/
│   ├── OrderRepository.java
│   └── AuthorizeOrder.java
├── service/OrderService.java
├── adapter/
│   ├── jdbc/JdbcOrderRepository.java
│   └── memory/InMemoryOrderRepository.java
└── web/
    ├── OrderRequest.java
    ├── OrderResponse.java
    └── OrderController.java
src/main/java/com/acme/web/ApiProblemAdvice.java  # augment one project-level advice
src/main/resources/db/migration/V014__create_orders.sql
src/test/java/com/acme/billing/
├── domain/OrderTest.java
├── service/OrderServiceTest.java
├── adapter/OrderRepositoryContract.java
├── adapter/jdbc/JdbcOrderRepositoryIT.java
├── adapter/memory/InMemoryOrderRepositoryTest.java
└── web/OrderControllerTest.java
src/test/java/com/acme/ArchitectureTest.java     # one project-level suite
src/test/resources/fixtures/orders.json
requests/orders.http
```

### 6.2 SQL contracts: `sql check`, `sql generate`, and `sql diff`

```text
jails sql init [--dialect postgres|mysql|sqlite]
jails sql check [<file|query-name>] [--offline|--live] [--frozen]
  [--against <git-ref|contract-dir>] [--no-cache]
jails sql generate [<file|query-name>] [--into-slice <Slice>] [--pretend]
jails sql diff [--generated] [--contracts]
jails sql explain <query-name> [--plan] [--analyze] [--params <json>]
```

Semantics:

- `check --offline` applies ordered migrations to the cached static catalog and resolves queries.
- `check --live` uses an explicit, already reachable datasource, migrates only when that invocation explicitly owns a scratch database, then prepares/describes every query. It never provisions a service.
- `--frozen` requires checked-in contract digests to match inputs and refuses to update them.
- `--against` checks working-tree queries against the schema/contracts at a deployment reference, exposing expand/contract ordering failures before release.
- `--no-cache` repeats the analysis and reports the new evidence digest.
- `generate` writes Java only from verified offline or live metadata. Parse-only results remain diagnostics and never produce TODO/refusal stubs that masquerade as compilable contracts.
- `explain --analyze` executes SQL and therefore requires explicit parameter values and confirmation outside a disposable database.

Example:

```text
$ jails sql check --live
catalog  postgres 17 · 14 migrations · digest 7a1c9d2
✓ FindOrder          :optional  1 param   5 columns   18 ms
✓ FindPayableOrders  :many      3 params  5 columns   21 ms
✗ CancelOrder        :execrows
  db/queries/orders.sql:31:19 column "version" does not exist
  nearest: orders.revision
  contract unchanged; generated Java unchanged
```

Generated SQL layout:

```text
src/main/resources/db/queries/orders.sql
src/main/java/com/acme/billing/application/query/
├── FindPayableOrders.java                # port
├── FindPayableOrdersParams.java
└── FindPayableOrdersRow.java
src/main/java/com/acme/billing/adapter/jdbc/JdbcFindPayableOrders.java
src/test/java/com/acme/billing/adapter/query/FindPayableOrdersContract.java
.jails/sql-contracts/billing/find-payable-orders.json
```

The JSON contract includes no data or credentials:

```json
{
  "schema": "jails.sql-contract.v1",
  "id": {"slice":"Billing","name":"FindPayableOrders"},
  "dialect": "postgresql",
  "query_digest": "sha256:f41b...",
  "catalog_digest": "sha256:7a1c...",
  "cardinality": "many",
  "parameters": [
    {"name":"status","sql_type":"text","java_type":"OrderStatus","nullable":false},
    {"name":"minimum","sql_type":"numeric","java_type":"BigDecimal","nullable":false},
    {"name":"limit","sql_type":"int4","java_type":"int","nullable":false}
  ],
  "columns": [
    {"name":"id","sql_type":"uuid","java_type":"UUID","nullable":false}
  ],
  "evidence": {
    "level": "verified-live",
    "input_digest": "sha256:f41b...",
    "catalog_digest": "sha256:7a1c...",
    "toolchain_digest": "sha256:58db...",
    "details_digest": "sha256:c318..."
  }
}
```

### 6.3 Database observation and import

```text
jails introspect db
  [--url <env:NAME|jdbc-url>] [--schema <name>] [--table <glob>]...
  [--include views,enums,domains,routines,indexes,policies]
  [--output human|json|manifest]

jails pull
  [--url <env:NAME|jdbc-url>] [--schema <name>] [--table <glob>]...
  [--into-slice <Slice>] [--naming preserve|java]
  [--baseline] [--ignore <glob>]...
```

`--url env:DEV_DATABASE_URL` is preferred because it does not put a secret in shell history or process listings. Human output redacts user, host where configured, and password. `introspect` never writes. `pull` always supports `--pretend` and defaults to a conflict-free additive plan; destructive reconciliation requires a separate `schema diff` acceptance.

Example import:

```text
$ jails pull --schema billing --table 'order*' --into-slice Billing --pretend
OBSERVED  3 tables · 17 columns · 4 FKs · 6 indexes · 1 enum
MAP
  billing.orders       → Billing.Order
  billing.order_lines  → Billing.OrderLine
  billing.order_status → Billing.OrderStatus
WARN
  orders.metadata jsonb → JsonNode (requires jackson; explicit override recorded)
  order_totals view → read-only query adapter
CONFLICT
  none
PLAN  31 creates · 2 edits · 0 deletes
NO WRITE (--pretend)
```

### 6.4 Schema evolution and migration generation

```text
jails schema diff
  [--from declared|migrations|live:<env>]
  [--to declared|migrations|live:<env>]
  [--accept-rename <old>=<new>]...
  [--treat-as-drop-add <old>=<new>]...
  [--output human|json]

jails schema migration <Name>
  [<field-change>...]
  [--from-diff <plan-id>]
  [--pretend]

jails migrate check [--clean] [--dialect postgres]
jails migrate lint [--since <git-ref>] [--offline|--live]
```

Friendly names may seed an operation, following Loco's useful inference (`AddStatusToOrders`, `CreateOrderLines`), but the normalized operation prints before generation. Loco's current generator explicitly pattern-matches migration names into operation types ([Loco generators](https://loco.rs/docs/reference/generators/)). Fields/flags override inferred words; an unknown name refuses unless `--empty` is explicit. Generated SQL is forward-only and contains no down/rollback body.

Destructive example:

```text
$ jails schema diff --from live:DEV_DATABASE_URL --to declared
DESTRUCTIVE
  DROP COLUMN orders.legacy_code  text nullable
    live rows with non-null value: 18,402
    declaration: absent
POSSIBLE RENAME
  orders.customer_id → buyer_id  confidence 0.82
REFUSED
  choose --accept-rename orders.customer_id=buyer_id or
         --treat-as-drop-add orders.customer_id=buyer_id
```

### 6.5 Fast application run with explicit ownership

```text
jails run [--watch]
  [--launcher auto|classpath|build-tool|jar]
  [--compile auto|ide|build|none]
  [--services existing|start|none]
  [--profile <name>]... [--] <application-argv>...
jails start
jails stop
```

Example:

```text
$ jails run --watch
services  existing: postgres healthy in 18 ms
compile   ide: jdtls output epoch 92 is current
classpath cache hit  sha256:...
launch    JDK 26 · com.acme.Application · direct classpath
started   611 ms
ready     http://127.0.0.1:8080/actuator/health in 742 ms
watch     target/classes, target/resources (Spring DevTools owns restart)
```

`services=existing` is the default and never starts Compose; `--services start` is the explicit one-command convenience. The direct-classpath launcher avoids repeated Maven/Gradle plugin startup while using the same resolved runtime inputs as console. `run --watch` continues to use ordinary Spring DevTools/process behavior. The test path never starts `run`, Docker, Compose, or a debugger. The Neovim adapter may display run and test statuses, but it does not merge their ownership.

### 6.6 Unified `jails test` specification

```text
jails test [<test-or-method>...]
  [--scope unit|integration|all]
  [--watch] [--affected] [--failed] [--tag <tag>]...
  [--engine auto|build|warm]
  [--compile auto|ide|build|none]
  [--until-fail] [--repeat <n>]
  [--timeout <duration>]
  [--db off|schema]
  [--explain-selection]

jails test daemon status|stop|restart
```

Selector precedence is intersection except where nonsensical:

```text
explicit names ∩ tags ∩ affected
+ previous failures when --failed is present
```

An empty affected set is successful only when there were no relevant changes and the bytecode/source epoch is current. Otherwise it widens. `--until-fail` never reuses application/test static state silently: the daemon resets its test classloader or forks according to the configured isolation boundary and prints it.

`engine=auto` preserves the requested universe and partitions it between warm and build-tool engines. `--fast` remains a temporary alias for this default and never changes scope. `engine=warm` is strict and refuses ineligible tests; `engine=build` is the diagnostic baseline. The daemon is started on demand by the test coordinator, so normal users do not invoke it directly.

`--watch` observes compiled outputs and resolves stale sources through the chosen compilation policy rather than executing old bytecode. Database/Spring integration tests are ineligible for the default warm daemon and use the ordinary build-tool/Testcontainers path; `schema` is the conservative real-database isolation selector. Human and JSON output carry one ordered report with per-test engine, selection reason, compile owner, duration, outcome, and fallback reason.

### 6.7 Diagnostics and explanations

```text
jails doctor [--scope <scope>] [--output human|json]
jails why [<log>] [--last] [--evidence]
jails why bean <type> [--path-to <consumer>]
jails why migration <version>
jails why query <name>
jails routes [--conflicts] [--openapi]
jails beans [--missing] [--cycles]
jails explain <artifact|capability|operation-id>
```

`doctor` and `why` are read-only. A machine result may contain a typed canonical-request fix, but executing it is a separate explicit preview through the normal mutation command or editor action.

### 6.8 Receipts and undo

```text
jails history [--limit <n>] [--output human|json]
jails show <transaction-id> [--diff] [--why]
jails undo <transaction-id> [--pretend] [--merge]
jails recover [--status|--continue]
```

Example:

```text
$ jails undo 01K4... --pretend
RESTORE  pom.xml                          exact after-image matches
DELETE   .../OrderController.java         exact after-image matches
MERGE    .../Order.java                   user edited after transaction
REFUSED  receipt contains migration V014; application cannot be disproved
  fix: prepare a forward corrective migration and matching code change
NO WRITE (--pretend)
```

### 6.9 HTTP contracts and release verification

```text
jails contract emit [--format openapi|json-schema] [--out <path>]
jails contract check --against <git-ref|file> [--scope declared|source]
```

`contract check` implements the compatibility rules from Section 3.9. It does not replace `jails check`: release verification remains the clean build-tool truth. CI composes explicit commands so evidence cannot be mistaken for a clean build:

Example:

```text
jails doctor --output json
jails sql check --frozen --live
jails contract check --against origin/main --scope declared
jails check
```

### 6.10 Existing application manifest

```text
jails app plan [<manifest>] [--diff]
jails app apply [<manifest>] [--yes]
jails app export [--from ledger|live] [--output toml|json]
```

These commands extend the shipped whole-manifest transition; they do not introduce a second application model. Complex authoring remains text/editor-first, and `app plan` is the only route from a manifest to project changes.

### 6.11 Editor protocol

Editors consume a stable CLI protocol; they do not scrape human output or duplicate command semantics:

```text
jails editor handshake [--path <project-path>] [--output json]
jails editor complete --arg-index <n> --byte-offset <n>
  [--path <project-path>] [--output json] -- <argv>...
jails editor symbols routes|beans|queries|tests|types
  [--query <text>] [--path <project-path>] [--output json]
jails editor diagnostics --scope buffer --file <project-relative-path>
  [--path <project-path>] [--evidence parsed|offline|live] [--output json]
jails editor diagnostics --scope project [--path <project-path>]
  [--evidence parsed|offline|live] [--output json]
```

`argv` excludes the executable. `arg-index` is zero-based; it may equal the argument count to complete a new empty token. `byte-offset` is a zero-based UTF-8 byte offset inside that argument and MUST fall on a code-point boundary. Editors pass tokens directly after `--`; neither side reparses a shell command line. The exact reports, lifecycle, and Neovim mapping are specified in Section 7.15.

### 6.12 Application tool gateway

```text
jails request <METHOD> <route-id|route-name|/path>
  [--profile <name>|--base-url <origin>]
  [--param name=value]... [--query name=value]...
  [--header name=value]... [--header-env name=ENV_NAME]...
  [--json @file|@-] [--data @file|@-] [--timeout <duration>]
  [--follow] [--print]

jails db console [--database <name>] [--profile <name>]
  [--client pgcli|psql] [--single-connection]

jails console [--profile <name>]... [--main <qualified-type>]
  [--web none|random|configured] [--compile]

jails runner --file <project-relative.jsh|->
  [--profile <name>]... [--main <qualified-type>]
  [--web none|random|configured] [--compile]

jails logs [<declared-service>]... [--follow]
  [--since <duration>] [--tail <count>]
```

These are transparent adapters for curl, pgcli/psql, JDK JShell, and the existing Compose implementation. They preserve the terminal, signals, byte streams, and child status; secrets are child-environment/stdin data, not argv. `request` never starts an app, database console never starts a database, console/runner never start infrastructure, and logs never changes service state. Section 7.16 is normative.

### 6.13 Coordinated resource rename

```text
jails rename resource <Slice>.<Current> <New>
  --strategy preserve-table|single-cutover|rolling
  [--table <target-table>]
  [--api preserve|rename] [--route <target-route>]

jails rename storage <Slice>.<Current>
  --complete <campaign-id> --old-version-retired
```

Rename follows one stable entity ID through the existing manifest, ledger-owned Java, generated tests, query ownership, contracts, and editor symbols. It never edits an applied migration. Preserving a table records an explicit binding; changing it creates a new forward migration. Reader-owned SQL, hand-written Java, opaque database objects, and breaking external names are blockers or separate explicit work, never guessed rewrites. Section 7.17 is normative.

### 6.14 Resource lifecycle and repair

```text
jails resource status <Slice.Entity|Entity>
  [--datasource <name>] [--output human|json]

jails resource field add <Entity> <field-spec>
  [--default-literal <typed-value>|--backfill-file <project-path>]
jails resource field rename <Entity> <current> <new>
  [--column preserve|single-cutover|rolling]
jails resource field type <Entity> <field> --to <type>
  --strategy safe|expand-contract [--conversion-file <project-path>]
jails resource field nullability <Entity> <field> --nullable|--required
  [--backfill-file <project-path>]
jails resource field drop <Entity> <field> --confirm-column <sql-name>

jails destroy scaffold <Entity> --storage preserve|drop
  [--confirm-table <sql-name>] [--migrate --datasource <name>]
jails resource revive <Entity> --table <sql-name>
jails resource repair <Entity> --strategy roll-forward
  [--datasource <name>]
```

`generate field <Entity> <field-spec>` remains a compatibility alias for `resource field add` and must use the same canonical request. There is no generic “edit” string: rename, type, nullability, and drop have different risk/precondition contracts. `--backfill-file` and `--conversion-file` are project-relative reader-owned SQL files; inline SQL is intentionally absent. A required field on a table that may contain rows needs a typed constant default or a backfill plan before Java switches to the required type.

All field changes append the next migration and update Java/tests/contracts in one prepared project transaction. They never replace an existing migration. `destroy scaffold` refuses without `--storage` when a table binding exists. Drop requires the exact table confirmation; `--force` does not imply data loss. `--migrate` is optional, requires an explicit datasource, and runs only after the project transaction commits, through the existing retryable effect protocol. Human and JSON results distinguish `code-retired`, `storage-preserved`, `drop-planned`, and `drop-applied`.

`resource status` is read-only and always works for active, retired, missing, or partially damaged resources. `repair --strategy roll-forward` is the only automatic repair in v1: it restores sealed migration bytes when possible, emits new migrations for semantic deltas, and reconciles owned projections. Ambiguous authority, an applied checksum matching neither known image, missing receipt objects, opaque SQL, and a live/catalog contradiction all refuse with exact next actions. Section 7.19 is normative.

---

## Section 7: Implementation RFC — Agent-Executable Contracts

### 7.0 RFC metadata, language, and decision status

| Field | Value |
|---|---|
| Identifier | JDX-001 |
| Status | Proposed implementation contract |
| Audience | maintainers and coding agents implementing the roadmap |
| Scope | CLI, compiler IR, SQL/schema evidence, transaction integration, test-watch IPC, diagnostics, editor adapters, and compatibility |
| Compatibility baseline | current canonical request, prepare/report, receipt, journal, and roll-forward recovery contracts |
| First dialect/runtime | PostgreSQL, JDK 26 default, Java 21 adopted-project floor, Spring JDBC; plain Java remains a supported projection where a feature does not require Spring |

The words MUST, MUST NOT, REQUIRED, SHALL, SHALL NOT, SHOULD, SHOULD NOT, and MAY are normative. A code example in this section is an interface contract unless it is labelled illustrative. Earlier sections explain product intent; when wording is ambiguous, this section controls implementation.

The feature classes are:

| Class | Features | Shipping rule |
|---|---|---|
| Core | extension of the existing app/slice model, SQL contracts, query ownership, evidence, prepared diffs, affected tests, explicit datasource resolution, contract checks | may ship once its conformance tests pass |
| Experimental | generated-test SQL transaction sandbox | opt-in generated test code only, explicit experimental diagnostic, measured promotion gate |
| Deferred | application supervision, JVM redefinition, implicit service provisioning, domain TUI/web GUI, general ORM, general SQL optimizer, generated production runtime | outside this RFC |

An implementation conforms only when:

1. every mutation follows parse → observe → plan → prepare → verify → commit → post-commit effects;
2. preview and apply use the same desired change set and prepared operation projection;
3. no preview starts a service, changes a database, changes managed project state, or runs a post-commit effect; an explicit plan-out may write only its named plan artifact;
4. generated applications compile and run without the jails executable or a jails runtime dependency;
5. machine output uses a declared schema and stable diagnostic codes;
6. caches can be deleted without losing authoritative project state;
7. uncertainty widens verification or refuses; it never silently weakens a check.

### 7.1 One execution protocol

New commands MUST extend the existing transaction pipeline. They MUST NOT write directly from a Clap arm, generator, observer, daemon, TUI, or doctor fix.

~~~text
CLI syntax
  → canonical request
  → typed feature input
  → read-only observations
  → pure desired change set
  → prepared bundle and one report
  → verification against the prepared after-state
  → guarded commit under the project lock
  → journalled post-commit effects
  → command envelope
~~~

The target engine boundary is:

~~~rust
pub struct MutationRequest<I> {
    pub syntax: CanonicalRequestSyntaxV1,
    pub input: I,
    pub mode: MutationMode,
    pub verification: VerificationPolicy,
}

pub enum MutationMode {
    Preview,
    Apply,
}

pub enum VerificationPolicy {
    None,
    Fast,
    Full,
}

pub struct PlannedMutation {
    pub subject: PlannedSubject,
    pub desired: DesiredChangeSet,
    pub verification: VerificationPlan,
}

pub trait MutationFeature {
    type Input;
    type Observation;

    fn observe(
        &self,
        root: &CanonicalRoot,
        input: &Self::Input,
    ) -> Result<Self::Observation, DiagnosticSet>;

    fn plan(
        &self,
        input: &Self::Input,
        observed: &Self::Observation,
    ) -> Result<PlannedMutation, DiagnosticSet>;
}
~~~

This trait is an engine adapter seam, not permission for the planner to perform I/O. Observe may read the project or an explicitly selected external source. Plan MUST be deterministic and pure for equal typed input and equal observations. Existing prepare and commit APIs remain the only path from DesiredChangeSet to files.

The engine SHALL enforce these barriers:

| Barrier | Required check | Failure behavior |
|---|---|---|
| after parse | canonical request validates and fingerprints | invalid-request, no observation |
| after observe | every observation has provenance and a content fingerprint | input-unreadable or input-invalid |
| after plan | DesiredChangeSet validates ownership and attribution | plan-refused |
| after prepare | operation/preimage/resource invariants validate | prepare-refused |
| after verify | all required gates passed against the prepared after-state digest | verification-failed |
| before commit | root, generation, and preimages still match under lock | stale-input; replan from the start |
| after commit | receipt matches the committed transaction | internal-invariant or recovery-blocked |

Costly observation and verification SHOULD happen outside the project lock. Commit MUST reacquire the lock and recheck every captured precondition; it MUST NOT replan under the lock. A stale plan returns a stale result and leaves the project unchanged.

The prepared after-state digest is:

~~~text
SHA-256(
  "JAILS-PREPARED-AFTER-1" ||
  canonical-root ||
  observed-generation ||
  ordered(path, before-object, after-object, mode, owners) ||
  ledger-before ||
  ledger-after ||
  ordered(post-commit-effect)
)
~~~

Length-prefixed canonical codec bytes SHALL be used for each component; string concatenation is notation only. Verification records and plan files bind to this digest.

Prepared operations describe actual observable changes, not idempotent executor calls. Parent-directory planning uses an `lstat`-style, no-follow observation for every ancestor of a file creation:

~~~rust
pub enum DirectoryFact {
    Missing,
    Directory,
    NonDirectory { kind: FilesystemKind },
}

pub struct DirectoryPrecondition {
    pub path: ProjectPath,
    pub expected: DirectoryFact,
}
~~~

`Missing` produces one ordered `DirectoryOp::Create`; `Directory` produces no public operation but remains a commit precondition that must still resolve beneath the canonical root as a real directory; `NonDirectory` refuses during preparation. Commit may call an idempotent `create_dir_all` internally to satisfy actual create operations, but reports `mkdir`/`create_directory` only for `Missing`. Existing directories are omitted from human and JSON operation arrays. If a future `--verbose` mode exposes observations, it labels them `already-directory` outside the operation list and therefore outside mutation counts. Preview, applied receipt, operation identity, and JSON/human output all use the same filtered operation sequence.

### 7.2 Canonical application and slice model

The existing app manifest, compact CLI fields, and database pull MUST construct the same values. `ApplicationSpecV1` is a versioned extension of that manifest, not a second source of application truth. Downstream generators MUST accept typed values and MUST NOT parse field strings.

Target public model in jails-spec:

~~~rust
pub struct ApplicationSpecV1 {
    pub name: AppName,
    pub base_package: JavaPackage,
    pub java_release: JavaRelease,
    pub dialect: SqlDialect,
    pub slices: BTreeMap<SliceName, SliceSpecV1>,
}

pub struct JavaRelease(u16);

pub struct SliceSpecV1 {
    pub package: Option<JavaPackage>,
    pub route_prefix: Option<RoutePath>,
    pub entities: BTreeMap<EntityName, EntitySpecV1>,
    pub queries: BTreeMap<QueryName, QuerySpecV1>,
    pub events: BTreeMap<EventName, EventSpecV1>,
    pub policies: BTreeMap<PolicyName, PolicySpecV1>,
}

pub struct EntitySpecV1 {
    pub id: EntityId,
    pub lifecycle: DeclaredEntityLifecycle,
    pub table: TableBinding,
    pub fields: Vec<FieldSpec>,
    pub indexes: Vec<IndexSpec>,
    pub audit: AuditPolicy,
}

pub struct EntityId(ObjectId);

pub enum DeclaredEntityLifecycle {
    Active,
    RetiredPreservingStorage,
    RetiredDropPlanned { migration: ProjectPath },
}

pub enum TableBinding {
    Conventional(SqlName),
    Explicit(SqlName),
    PendingRename {
        current: SqlName,
        target: SqlName,
        campaign: RenameCampaignId,
    },
}

pub struct RelationSpecV1 {
    pub source: FieldPath,
    pub target: FieldPath,
    pub on_delete: ReferentialAction,
}

pub enum AuditPolicy {
    None,
    Created,
    CreatedAndUpdated,
}
~~~

Maps are used where order is not semantic. Field order and composite-index column order are semantic and remain vectors. Constructors SHALL validate names, duplicates, cross-slice references, key types, route conflicts, default literals, and constraint/type compatibility before values enter the protocol.

`EntityId` is the identity across logical and physical renames; display names and table bindings are attributes. A legacy manifest without IDs normalizes each entity to `SHA-256("JAILS-ENTITY-ID-1" || application-identity || original-slice || original-entity-name)` and the next successful app mutation persists the result. Once persisted, an ID is never recomputed from a renamed path. Two IDs or two live names may not alias one entity.

JavaRelease accepts integers greater than or equal to 21. New-project construction supplies 26 when the user omits a release. Adopted-project planning replaces no build setting: it records the release observed from the Maven or Gradle model and reports a manifest/build mismatch as a diagnostic. A requested release change is a separate canonical mutation, never a side effect of generating a slice.

The compact CLI field grammar is deliberately shell-safe and exactly:

~~~ebnf
field       = name, ":", cli-type, [ optionality ], { annotation }, [ default ] ;
optionality = "?" | "!" ;
cli-type    = builtin | java-type | enum-type | reference ;
enum-type   = "enum.", enum-value, { ".", enum-value } ;
reference   = "ref.", entity, ".", field-name
            | "ref.", slice, ".", entity, ".", field-name ;
annotation  = "@pk" | "@scope" | "@index" | "@unique" | "@audit"
            | "@positive" | "@nonnegative" ;
default     = "=", shell-safe-literal ;
~~~

`shell-safe-literal` contains only ASCII letters, digits, `_`, `.`, `:`, `+`, and `-`. The grammar contains no braces, angle brackets, brackets, parentheses, pipes, whitespace, glob characters, or raw SQL. No suffix means required. Question mark means nullable and projects to `Optional` for reference types. Exclamation mark preserves its existing meaning: non-null and non-blank, and is valid only for text. At most one optionality suffix and one default are allowed. Unknown annotations are refused with the complete valid set. Collections, composite relations, join tables, custom database types, policy expressions, and defaults outside the literal alphabet are manifest-only structured values. Bash, Zsh, Fish, PowerShell, and direct-argv conformance tests MUST observe identical argument tokens for every documented CLI example.

Reference resolution rules are:

1. `ref.Slice.Entity.field` resolves exactly.
2. `ref.Entity.field` resolves only when the entity name is unique across slices.
3. Compact CLI references always name a field; a structured manifest relation MAY target an entity only when it has one primary-key field.
4. A composite primary key is manifest-only and requires every target field explicitly.
5. The source SQL type must be assignment-compatible with the target key type.
6. Ambiguity is a diagnostic with sorted candidates; no naming guess is persisted.

Manifest decoding SHALL support TOML and JSON with schema identifier jails.app.v1. The canonical TOML keys and nesting are:

~~~toml
schema = "jails.app.v1"

[application]
name = "Orders"
base_package = "com.acme"
java_release = 26
dialect = "postgresql"

[slices.Billing]
package = "com.acme.billing"
route_prefix = "/billing"

[slices.Billing.entities.Order]
id = "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
table = "orders"
audit = "created-and-updated"
fields = [
  "id:uuid@pk",
  "accountId:uuid",
  "total:decimal@positive",
  "status:enum.PENDING.PAID.CANCELLED=PENDING",
  "createdAt:instant@audit",
]

[[slices.Billing.entities.Order.indexes]]
name = "orders_status_created_at_idx"
fields = ["status", "createdAt"]
unique = false
~~~

The JSON form uses the same snake_case keys and object/array nesting. Simple entity field strings pass through `FieldSpec::parse` exactly once and are stored as typed `FieldSpec` values. Structured TOML/JSON objects represent collection and composite-relation shapes without embedding another string language. Unknown fields are errors. A newer schema is refused with an upgrade instruction. A decoder may accept documented legacy aliases, but it MUST normalize them before constructing `CanonicalRequestSyntaxV1` so equivalent CLI and manifest input have the same semantic digest.

Source spans are parse metadata:

~~~rust
pub struct Spanned<T> {
    pub value: T,
    pub source: SourceId,
    pub span: ByteSpan,
}
~~~

Spans MUST NOT participate in semantic identity. Diagnostics retain them; protocol and generation values do not.

### 7.3 Resource identity and evidence

Resource variants MUST be appended to the current wire tag registry; existing tags MUST NOT move:

~~~rust
pub enum ResourceKey {
    // existing variants and tags 0 through 9 remain unchanged
    Query(QueryId), // tag 10
}

pub struct SchemaObjectId {
    pub dialect: SqlDialect,
    pub namespace: SqlNamespace,
    pub kind: SchemaObjectKind,
    pub name: SqlName,
    pub parent: Option<QualifiedSqlName>,
}

pub enum SchemaObjectKind {
    Schema,
    Table,
    Column,
    PrimaryKey,
    ForeignKey,
    Unique,
    Index,
    Check,
    Enum,
    Domain,
    View,
    Routine,
    Policy,
}

pub struct QueryId {
    pub slice: SliceName,
    pub name: QueryName,
}
~~~

`SchemaObjectId` is an observation/diff identity, not a `ResourceKey`, ledger owner, or promise that a database object can be reversed. The migration file remains the durable schema authority. Schema object identity uses declared database names, not Java names or migration filenames. A column parent is its qualified table name; a constraint or index parent is its qualified table. Query identity uses slice plus query name and survives moving the SQL file.

Evidence is a closed ladder:

~~~rust
pub enum EvidenceLevel {
    Parsed,
    VerifiedOffline,
    VerifiedLive,
    Executed,
}

pub struct EvidenceRecord {
    pub subject: EvidenceSubject,
    pub level: EvidenceLevel,
    pub input_digest: ObjectId,
    pub catalog_digest: Option<ObjectId>,
    pub toolchain_digest: ObjectId,
    pub details_digest: ObjectId,
}
~~~

EvidenceLevel ordering is meaningful only for records with equal subject, input digest, catalog digest, and relevant toolchain. A static inference that has not reached `Parsed` is a diagnostic hypothesis, not successful evidence and not an `EvidenceRecord`. VerifiedLive from a different schema does not satisfy VerifiedOffline for the current schema. Executed means a contract fixture executed successfully; it does not assert production performance or data equivalence.

Evidence identity excludes wall-clock time, elapsed duration, host path, port, credentials, and container ID. Those belong in an observational result, never in a deterministic plan or cache key.

### 7.4 SQL contract compiler

#### 7.4.1 Source format

Each managed statement begins with a directive block:

~~~sql
-- jails:name FindPayableOrders
-- jails:cardinality many
-- jails:param status text
-- jails:param minimum numeric
-- jails:param limit int4
SELECT id, account_id, total, status, created_at
FROM orders
WHERE status = :status
  AND total >= :minimum
ORDER BY created_at, id
LIMIT :limit;
~~~

Directive grammar:

~~~ebnf
name        = "-- jails:name ", upper-ident ;
cardinality = "-- jails:cardinality ",
              ( "one" | "optional" | "many" | "exec" | "execrows" ) ;
parameter   = "-- jails:param ", lower-ident, " ", sql-type, [ " nullable" ] ;
statement   = name, newline, cardinality, newline, { parameter, newline }, sql ;
~~~

Rules:

- one directive block owns exactly one terminated SQL statement;
- query names are unique project-wide after Slice qualification;
- every named parameter in SQL has one declaration, and every declaration is used;
- repeated uses of a parameter share one value;
- colon inside a PostgreSQL cast, quoted string, quoted identifier, comment, or dollar-quoted body is not a parameter;
- positional parameters and named parameters may not be mixed;
- files under the configured query roots may contain ordinary comments, but no executable unnamed statement;
- cardinality one requires exactly one row, optional allows zero or one, many allows any count, exec returns no row contract, and execrows returns an affected-row count.

The parser SHALL retain original-to-normalized byte-span mappings so live database errors point to the reader-owned SQL.

Parameter contracts preserve directive order; result columns preserve database description order. The query digest hashes the UTF-8 statement, excluding jails directives, after converting CRLF/CR to LF and ensuring exactly one terminal newline. It does not reformat SQL or remove ordinary comments, so a reader-owned textual change remains observable. Catalog digests hash canonical codec bytes with SchemaObjectId ordering.

#### 7.4.2 Public IR and adapters

~~~rust
pub struct QuerySource {
    pub id: QueryId,
    pub path: ProjectPath,
    pub statement_span: ByteSpan,
    pub sql: String,
    pub cardinality: Cardinality,
    pub declared_parameters: Vec<DeclaredParameter>,
}

pub struct QueryContractV1 {
    pub id: QueryId,
    pub dialect: SqlDialect,
    pub query_digest: ObjectId,
    pub catalog_digest: ObjectId,
    pub cardinality: Cardinality,
    pub parameters: Vec<ParameterContract>,
    pub columns: Vec<ColumnContract>,
    pub evidence: EvidenceRecord,
}

pub trait DialectAdapter {
    fn parse_query(&self, source: &QuerySource)
        -> Result<ParsedQuery, DiagnosticSet>;

    fn apply_migration(
        &self,
        catalog: &CatalogSnapshot,
        migration: &MigrationSource,
    ) -> Result<CatalogSnapshot, DiagnosticSet>;

    fn describe_live(
        &self,
        connection: &RedactedConnection,
        query: &ParsedQuery,
    ) -> Result<LiveDescription, DiagnosticSet>;
}
~~~

CatalogSnapshot and QueryContractV1 are protocol values and implement the canonical codec. Database clients, filesystem paths outside ProjectPath, and credentials are runtime values and MUST NOT enter either type.

The compiler stages are fixed:

1. discover ordered migrations and query sources;
2. parse and normalize directives and SQL;
3. build or load a catalog snapshot;
4. resolve relations, parameters, result columns, nullability, and cardinality;
5. optionally prepare/describe against a scratch or explicitly selected live database;
6. emit a QueryContractV1;
7. project Java from that contract;
8. prepare generated files and checked-in contract changes through the normal transaction pipeline.

Java generation MUST refuse when a required Java mapping is unknown. It MUST NOT silently use Object. An explicit project mapping may resolve a vendor type and becomes part of the toolchain digest.

The cache key is:

~~~text
SHA-256(
  "JAILS-SQL-CHECK-1" ||
  dialect ||
  normalized-query ||
  declared-parameters ||
  ordered-migration-digests ||
  catalog-digest ||
  type-mapping-digest ||
  analyzer-version
)
~~~

Cache hits are valid only when every component matches. No-cache recomputes but MUST produce the same evidence identity. Frozen mode compares the recomputed contract with the checked-in contract and never rewrites it.

Generated names are fixed by the blueprint in Section 8.8:

| Contract | Generated Java |
|---|---|
| QueryId Billing.FindPayableOrders | application/query/FindPayableOrders.java |
| parameters present | nested Params record |
| cardinality one | Row execute(Params) |
| cardinality optional | Optional<Row> execute(Params) |
| cardinality many | List<Row> execute(Params) |
| cardinality exec | void execute(Params) |
| cardinality execrows | int execute(Params) |
| JDBC adapter | adapter/jdbc/JdbcFindPayableOrders.java |

Zero-parameter queries omit Params and expose execute(). RowMapper code is explicit and non-reflective. The SQL remains reader-owned; generated Java and contract files remain ledger-owned.

### 7.5 Schema snapshots, diffs, and migration plans

The three authorities are independent values:

~~~rust
pub struct ReconciliationInput {
    pub declared: Option<SchemaSnapshot>,
    pub migrations: Option<SchemaSnapshot>,
    pub live: Option<SchemaSnapshot>,
}

pub struct SchemaSnapshot {
    pub dialect: SqlDialect,
    pub objects: BTreeMap<SchemaObjectId, SchemaObject>,
    pub digest: ObjectId,
    pub provenance: SchemaProvenance,
}

pub enum SchemaProvenance {
    Declared,
    Migrations { files: Vec<ProjectPath> },
    Live { server_major: u16, database_fingerprint: ObjectId },
}
~~~

Diffing produces typed operations:

~~~rust
pub enum SchemaOp {
    Create(SchemaObject),
    Alter { before: SchemaObject, after: SchemaObject },
    Rename { before: SchemaObjectId, after: SchemaObjectId },
    Drop(SchemaObject),
}

pub enum MigrationRisk {
    Additive,
    DataDependent,
    ConstraintLoss,
    Destructive,
    DeploymentIncompatible,
    Opaque,
}

pub struct PlannedSchemaOp {
    pub operation: SchemaOp,
    pub dependencies: BTreeSet<SchemaObjectId>,
    pub risks: BTreeSet<MigrationRisk>,
}
~~~

The differ SHALL match exact stable identity first. Similar names may produce rename candidates, never Rename operations. Only an explicit accept-rename choice creates Rename and records that choice in the canonical request.

Ordering is a deterministic topological sort with SchemaObjectId as the tie-breaker. Required dependencies include:

- schema before contained objects;
- enum/domain before columns using it;
- table before its columns, keys, indexes, checks, and policies;
- referenced table/key before foreign key creation;
- foreign key/index/constraint removal before dropping its column or table;
- view/routine removal before dropping referenced objects when dependency facts exist.

A dependency cycle is prepare-refused with the cycle path. Opaque vendor objects are retained and classified `Opaque`. If an intended operation might alter, replace, or invalidate an opaque object, planning fails with `unsupported-schema-object`, identifies the object and provenance, and emits no migration. Similar-looking SQL is never treated as proof of reversibility.

Schema operations are forward plans only. Generated migrations contain no executable or commented down body, `PlannedSchemaOp` carries no inverse, and receipt-driven file undo refuses every transaction that contains a migration or an unresolved external effect. Recovery guidance produces a new forward corrective plan.

### 7.6 CLI grammar, streams, and exit semantics

The root options remain:

~~~rust
pub struct GlobalArgs {
    pub debug: bool,
    pub pretend: bool,
    pub output: Output,
}

pub enum Output {
    Human,
    Json,
    JsonV1,
}

pub struct ReviewArgs {
    pub diff: bool,
    pub ast: bool,
    pub verify: VerificationPolicy,
    pub yes: bool,
    pub plan_in: Option<PathBuf>,
    pub plan_out: Option<PathBuf>,
}
~~~

ReviewArgs is flattened only into mutation commands. The global pretend flag is syntactically accepted for a read-only command and has no additional effect; JSON output reports read_only: true. The CLI MUST NOT print a warning merely because a globally valid safety flag is redundant.

Existing per-command `--json` flags become visible compatibility aliases for `--output json` and normalize to the same canonical request. Supplying both with different values is invalid-request. Mutation verification defaults to fast. There is no umbrella `jails verify` command: CI and developers compose the evidence-producing commands explicitly, while `jails check` retains its current meaning as the clean build truth.

Argument constraints:

| Command/flags | Constraint |
|---|---|
| plan-in | conflicts with all semantic positionals/options except output, debug, yes, and verification; verification may equal or strengthen the plan minimum, never weaken it |
| plan-out | allowed with preview or apply; after successful preparation, atomically writes only the named mode-0600 plan artifact and never adds it to the prepared project operation set |
| diff and ast | may be combined; diff is bytes, ast is semantic edit summary |
| offline and live | mutually exclusive; neither means highest available evidence without starting a service |
| frozen | requires offline or live evidence and a checked-in contract |
| against | accepts a Git object or existing contract directory; ambiguous input is invalid-request |
| explain --analyze | requires explicit params and a disposable datasource or yes confirmation |
| until-fail and repeat | mutually exclusive |
| test explicit selectors plus affected/tags | intersection |
| failed | unions previous failures after the intersection; duplicates removed |

Stdout is reserved for the selected result encoding and generated content explicitly requested with an out flag. Debug logs, subprocess echo, progress animation, and daemon diagnostics go to stderr. JSON mode emits no ANSI escapes and no prose on stdout.

Exit codes preserve the implemented CommandStatus projection:

| Exit | Statuses |
|---:|---|
| 0 | succeeded, preview, no-op, applied, finalised, aborted, effect-retried, and effect-superseded |
| 1 | refused, stale, recovery-blocked, effect-failed, verification failure, failed tests, unavailable service, or invalid input |
| 2 | conflicted |

The stable error code, not a new process exit number, distinguishes failures. Add these codes to the next command-result schema registry:

| Code | Meaning |
|---|---|
| sql-parse | SQL could not be parsed |
| sql-unverified | required evidence level was not reached |
| schema-drift | compared authorities differ outside accepted operations |
| migration-risk | configured risk policy refused the plan |
| migration-sealed | a requested operation would change append-only migration history |
| migration-edited-after-seal | a current migration differs from its proved sealed image |
| storage-policy-required | table-backed destroy omitted preserve/drop intent or exact destructive confirmation |
| resource-inconsistent | declaration, generated projections, migration history, or optional live schema cannot be reconciled automatically |
| resource-not-revivable | the retired resource has a committed drop plan or proved applied drop |
| data-plan-required | a field evolution needs explicit backfill/conversion evidence |
| storage-dependency-blocked | a requested table/column operation has unresolved or opaque dependents |
| contract-breaking | compatibility check found a breaking change |
| verification-failed | one or more named verification steps failed |
| service-unavailable | an explicitly required service could not become healthy |
| protocol-mismatch | client and local daemon have no common protocol |
| watch-overflow | watcher rescan could not prove a current epoch |

Invalid arguments discovered by Clap may use Clap's standard exit code before a CommandEnvelope exists. Once dispatch constructs a canonical request, every failure uses the envelope mapping above.

### 7.7 Machine-readable result and event protocols

Existing jails.command-result.v1 output SHALL be frozen by golden tests. It MUST NOT gain fields or enum variants under the same schema identifier. The new features introduce jails.command-result.v2. During one compatibility release, output json-v1 remains available for existing report kinds; requesting it for a v2-only report returns protocol-mismatch.

V2 top-level shape is exact:

~~~json
{
  "schema": "jails.command-result.v2",
  "command": {
    "path": ["sql", "check"],
    "fingerprint": "sha256:...",
    "read_only": true
  },
  "status": "refused",
  "exit_code": 1,
  "project_commit": "none",
  "recovery": [],
  "report": {
    "kind": "sql-check",
    "schema": "jails.sql-check.v1",
    "data": {}
  },
  "receipt": null,
  "error": {
    "code": "sql-unverified",
    "message": "live evidence was required",
    "diagnostics": []
  },
  "timings": []
}
~~~

The corresponding Rust result model is:

~~~rust
pub struct CommandEnvelopeV2 {
    pub command: CommandIdentity,
    pub status: CommandStatusV2,
    pub project_commit: ProjectCommitDisposition,
    pub recovery: Vec<RecoveryOutcome>,
    pub report: Option<CommandReportV2>,
    pub receipt: Option<AppliedReceipt>,
    pub error: Option<ErrorReportV2>,
    pub timings: Vec<TimingSpan>,
}

pub enum CommandStatusV2 {
    Succeeded,
    Preview,
    NoOp,
    Applied,
    Conflicted,
    Finalised,
    Aborted,
    EffectRetried,
    EffectSuperseded,
    Refused,
    Stale,
    RecoveryBlocked,
    EffectFailed,
}

pub enum CommandReportV2 {
    Prepared(Report),
    EffectRetry(EffectRetryReport),
    SqlCheck(SqlCheckReportV1),
    SchemaDiff(SchemaDiffReportV1),
    Introspection(IntrospectionReportV1),
    Verification(VerificationReport),
    Test(TestReportV1),
    ResourceStatus(ResourceStatusV1),
    EditorHandshake(EditorHandshakeV1),
    EditorCompletion(EditorCompletionV1),
    EditorSymbols(EditorSymbolsV1),
    EditorDiagnostics(EditorDiagnosticsV1),
}

pub struct CommandIdentity {
    pub path: Vec<String>,
    pub fingerprint: RequestSyntaxFingerprint,
    pub read_only: bool,
}

pub struct SqlCheckReportV1 {
    pub dialect: SqlDialect,
    pub catalog_digest: ObjectId,
    pub evidence: EvidenceLevel,
    pub queries: Vec<QueryCheckResult>,
}

pub struct QueryCheckResult {
    pub id: QueryId,
    pub status: CheckStatus,
    pub contract: Option<QueryContractV1>,
    pub diagnostics: Vec<Diagnostic>,
    pub duration_us: u64,
}
~~~

CommandReportV2 is externally tagged by the kind/schema/data object shown above. Its schema identifiers are respectively `jails.prepared-report.v1`, `jails.effect-retry.v1`, `jails.sql-check.v1`, `jails.schema-diff.v1`, `jails.introspection.v1`, `jails.verification.v1`, `jails.test-report.v1`, `jails.resource-status.v1`, `jails.editor-handshake.v1`, `jails.editor-completion.v1`, `jails.editor-symbols.v1`, and `jails.editor-diagnostics.v1`. Adding another report variant or field after v2 is released requires a new containing schema version.

Envelope invariants:

| Status class | report | receipt | error | project_commit |
|---|---|---|---|---|
| succeeded read-only command | required | null | null | none |
| preview | prepared/effect report required | null | null | none |
| no-op | optional | null | null | none |
| applied, conflicted, finalised, aborted | null | required | null | receipt-derived |
| effect-retried, effect-superseded | effect report required | null | null | none |
| refused, stale, recovery-blocked, effect-failed | optional completed-phase report | null | required | none |

Succeeded is new in v2 and is used only for a completed read-only command. All v1 statuses map to the same-named v2 status. The exit code remains a projection: Succeeded maps to zero and every other status follows Section 7.6.

Every declared field is present. Optional values are null and collections are empty arrays. Object keys use the order shown by the manual serializer; semantically unordered arrays are sorted. Durations are integer microseconds. Paths are project-relative UTF-8 slash paths. Secrets, absolute roots, random container names, PIDs, and environment values are redacted before the serializer receives them.

Diagnostics use:

~~~rust
pub struct Diagnostic {
    pub code: DiagnosticCode,
    pub severity: Severity,
    pub message: String,
    pub subject: Option<SemanticPath>,
    pub primary: Option<SourceLabel>,
    pub related: Vec<SourceLabel>,
    pub evidence: Vec<EvidenceRecord>,
    pub fixes: Vec<TypedFix>,
}

pub enum Severity {
    Note,
    Warning,
    Error,
}

pub struct TypedFix {
    pub title: String,
    pub request: CanonicalRequestSyntaxV1,
    pub preconditions: Vec<Precondition>,
}
~~~

A diagnostic code is lowercase kebab case and immutable once released. Message text may improve. A typed fix is data; renderers MUST NOT execute shell text.

Long-running commands emit JSON Lines with schema jails.event.v1, one complete object per line:

~~~json
{"schema":"jails.event.v1","session":"01K...","sequence":0,"epoch":92,"kind":"ready","data":{}}
{"schema":"jails.event.v1","session":"01K...","sequence":1,"epoch":93,"kind":"compiled","data":{"sources":1,"duration_us":143000}}
{"schema":"jails.event.v1","session":"01K...","sequence":2,"epoch":93,"kind":"tested","data":{"selected":4,"passed":4,"failed":0,"duration_us":51000}}
~~~

Sequence starts at zero and increases by one. Epoch increases only when the input snapshot changes. Consumers SHALL discard a result whose epoch is older than the latest announced epoch.

### 7.8 Unified test coordinator and internal testd v2

`jails test` owns selection, compile policy, engine partitioning, and one result. `testd` is the internal warm engine, not an alternative product workflow. The canonical planning values are:

~~~rust
pub struct TestExecutionPlanV1 {
    pub scope: TestScope,
    pub requested: Vec<TestSelector>,
    pub compile: TestCompilePolicy,
    pub engine: TestEnginePolicy,
    pub epoch: u64,
    pub partitions: Vec<TestPartition>,
}

pub enum TestScope { Unit, Integration, All }
pub enum TestCompilePolicy { Auto, Ide, Build, None }
pub enum TestEnginePolicy { Auto, Build, Warm }

pub struct TestPartition {
    pub engine: TestEngine,
    pub selectors: Vec<TestSelector>,
    pub reasons: Vec<SelectionReason>,
}

pub enum TestEngine { Maven, Gradle, TestdV2 }
~~~

Selectors are deduplicated before partitioning. `Auto` may delegate but cannot delete a selector. `Warm` refuses when any requested selector is ineligible. All engines return normalized cases to one `TestReportV1`, including engine, selector/source, outcome, duration, stdout/stderr summary, selection reason, and fallback reason. JSON, slowest, failed, and fail-fast operate on that report rather than depending on Surefire-only XML. The one-release `--fast` alias normalizes to `engine=auto`; the one-release `jails testd` alias normalizes to strict warm plus compile none and reports that canonical command.

#### 7.8.1 Local IPC

The internal v2 transport is a Unix-domain socket at `.jails/run/testd-v2.sock`. A Windows implementation may use a named pipe; it MUST NOT expose an unauthenticated TCP port as fallback.

Each frame is:

~~~text
4-byte unsigned big-endian payload length
payload bytes encoded with the canonical jails codec
~~~

Maximum payload is 8 MiB. Larger frames close the connection and report protocol-mismatch. The daemon writes .jails/run/testd-v2.meta atomically with protocol range, project-root digest, PID, start time, and a random 256-bit cookie. The directory and files are user-only. Every request carries the cookie and project digest.

Messages:

~~~rust
pub enum TestdRequestV2 {
    Hello {
        request_id: RequestId,
        protocol_min: u16,
        protocol_max: u16,
        project: ObjectId,
        cookie: SecretBytes,
    },
    Run {
        request_id: RequestId,
        epoch: u64,
        selectors: Vec<TestSelector>,
        classpath: ObjectId,
        outputs: OutputSnapshot,
        isolation: TestIsolation,
    },
    Status { request_id: RequestId },
    Cancel { request_id: RequestId },
    Stop { request_id: RequestId },
}

pub enum TestdResponseV2 {
    Hello { request_id: RequestId, protocol: u16 },
    Accepted { request_id: RequestId, epoch: u64 },
    Event { request_id: RequestId, event: TestEvent },
    Completed { request_id: RequestId, result: TestResult },
    Refused { request_id: RequestId, diagnostic: Diagnostic },
}
~~~

Request IDs make retries idempotent for the lifetime of the daemon. A duplicate completed Run returns the cached result only when epoch, selectors, classpath, outputs, and isolation match exactly. Otherwise it is refused.

#### 7.8.2 Epoch and selection rules

OutputSnapshot contains sorted class/resource paths with size, modification nanoseconds, and content digest. Modification time is a rescan optimization; the digest is authority. A source newer than its class output marks the epoch stale and testd MUST refuse to run it as current.

Affected selection is a safety optimization:

1. begin with changed production classes, test classes, resources, migrations, query contracts, build files, and processor configuration;
2. walk reverse source and bytecode edges to tests;
3. add explicit selectors and tags by intersection;
4. union prior failures only when failed is set;
5. widen to all tests on an unknown edge kind, parse gap, missing output, classpath change, watcher overflow, deleted owner, processor change, or stale graph;
6. print and serialize every widening reason.

An empty current affected set succeeds without starting the daemon. An empty set from incomplete facts widens; it is never reported as nothing changed.

#### 7.8.3 Test-watch state machine

~~~text
Cold
  → Observing
  → Ready(epoch)
  → Debouncing(epoch+1)
  → WaitingForClasses(epoch+1)
  → Selecting(epoch+1)
  → Testing(epoch+1)
  → Ready(epoch+1)

Any active state
  → Delegating(epoch, reason)
  → ordinary build-tool test command
  → Ready(new epoch) or Failed(diagnostic)

Any state
  → Stopping
  → Stopped
~~~

Watch events are hints. The coordinator waits the configured delay, default 75 ms, then hashes the affected roots. A continuous edit stream is forced through after 500 ms so feedback cannot starve. Overflow triggers a full rescan before further work. `testd` consumes classes produced according to the coordinator's compile policy; it never compiles inside the daemon. When output is absent or stale, it emits `classes-stale` to the coordinator, which waits, compiles through the wrapper, delegates, or refuses according to `TestCompilePolicy`.

The daemon is eligible only for isolated unit/contract tests. Spring context, container, integration, fork-sensitive, global-process-state, and unknown-category tests delegate to the ordinary Maven/Gradle route. The warm daemon recycles after 50 classloader generations, 128 MiB metaspace growth, a leaked non-daemon thread, or a test-engine/classpath change. A deterministic full-suite oracle SHALL continuously prove that affected selection never excludes a failing test; any unknown edge widens or delegates.

Every `Ready` event names the active epoch and whether test output is current. A daemon crash or protocol mismatch delegates to the ordinary build tool. `testd` never launches, restarts, attaches to, or redefines an application JVM. Application reload remains `jails run --watch` plus Spring DevTools; jdtls/DAP owns debugging and HotSwap.

### 7.9 Explicit datasource and service resolution

The RFC adds no service lease, provider, container startup, reset, or down protocol. Project services retain the existing explicit lifecycle: committed Compose is controlled through `jails start`/`jails stop`, and generated integration tests declare Spring-managed Testcontainers `@Bean` values with `@ServiceConnection`.

Read-only and SQL commands accept an already available datasource:

~~~rust
pub struct ResolvedDatasource {
    pub dialect: SqlDialect,
    pub source: DatasourceSource,
    pub redacted_endpoint: RedactedEndpoint,
    pub server_major: u16,
    pub capability_digest: ObjectId,
}

pub enum DatasourceSource {
    ExplicitEnvironment { variable: String },
    DeclaredRunningService { declaration: ProjectPath, service: String },
    SpringTestConfiguration { source: ProjectPath },
}
~~~

Resolution order is exact: an explicitly named environment reference first, then an already-running service from the committed project declaration, then a generated Spring test configuration when the ordinary test route owns startup. A CLI database consumer checks reachability from its own network namespace; it MUST NOT reuse an application-only container hostname. Failure returns `service-unavailable` with attempted sources and redacted endpoints. Help, completion, preview, static SQL checking, `doctor`, `why`, editor diagnostics, and `testd` never start a container or mutate service state.

### 7.10 Verification gates and compatibility

~~~rust
pub struct VerificationPlan {
    pub prepared_after: ObjectId,
    pub steps: Vec<VerificationStep>,
}

pub struct VerificationStep {
    pub id: VerificationStepId,
    pub requirement: EvidenceRequirement,
    pub inputs: BTreeSet<ProjectPath>,
    pub timeout: Duration,
}

pub struct VerificationReport {
    pub prepared_after: ObjectId,
    pub results: Vec<VerificationResult>,
    pub passed: bool,
}
~~~

Verification step IDs are stable lowercase kebab case. Results are ordered by plan order, not completion order. Concurrent execution MAY reduce latency, but output ordering and digesting stay deterministic.

Policy composition:

Verification strength is the total order None < Fast < Full. A caller may strengthen a prepared plan's minimum but cannot weaken it.

| Step | none | fast | full |
|---|:---:|:---:|:---:|
| protocol/spec/ownership invariants | required | required | required |
| reconcile prepared preimages | required | required | required |
| changed Java syntax and generated-source compile | — | required | required |
| SQL parse and offline catalog resolution | — | required | required |
| migration static lint | — | required | required |
| affected unit/contract tests | — | required | required |
| clean build-tool verification | — | — | required |
| migrations on clean scratch database | — | — | required |
| live SQL prepare/describe | — | — | required |
| generated integration contracts | — | — | required |
| declared/source/runtime HTTP comparison when configured | — | — | required |

None skips optional external verification, never structural validation, ownership, reconciliation, or commit preconditions. Full requires an explicit reachable datasource for live database gates; otherwise it refuses before commit. The caller may start a declared service explicitly before retrying.

HTTP compatibility defaults:

| Change | Classification |
|---|---|
| remove route or supported method | breaking |
| add required request property | breaking |
| make optional request property required | breaking |
| narrow accepted enum/range/format | breaking |
| remove response property | breaking |
| change response type or nullability incompatibly | breaking |
| add optional request property | compatible |
| add response property | compatible only when consumer policy allows unknown fields; otherwise risky |
| add route or response enum value | risky by default, policy-configurable |
| documentation/example text only | compatible |

Contract check compares normalized contracts. Formatting and declaration order do not produce changes. Every result records whether its evidence is declared, source-derived, or runtime-observed.

### 7.11 Files, state, atomicity, and secrets

New paths:

| Path | Authority | Commit policy |
|---|---|---|
| .jails/app.toml | existing user-declared application intent | committed |
| .jails/sql-contracts/*.json | checked evidence contract | committed |
| .jails/cache/sql/* | recomputable cache | ignored |
| .jails/cache/graph/* | recomputable source/bytecode facts | ignored |
| .jails/run/testd-v2.* | process-local daemon state | ignored |
| existing ledger/journal/receipt paths | durable mutation state | preserve current policy |

All state formats carry a schema identifier before payload data. Unknown major versions fail closed. Durable writers use write-temp → fsync file → atomic rename → fsync parent where the existing commit protocol requires it. Cache writers use write-temp → atomic rename; a torn or undecodable cache entry is deleted and recomputed.

No new file may contain a database password, bearer token, service cookie outside .jails/run, absolute user home, or full unredacted JDBC URL. Plan-out files are rejected when a required secret cannot be represented as an environment reference.

Portable prepared plans use:

~~~rust
pub struct PreparedPlanV1 {
    pub schema: PlanSchema,
    pub project_root_digest: ObjectId,
    pub observed_generation: Generation,
    pub request: CanonicalRequestSyntaxV1,
    pub subject: PlannedSubject,
    pub desired: DesiredChangeSet,
    pub prepared_after: ObjectId,
    pub preconditions: Vec<Precondition>,
    pub objects: BTreeMap<ObjectId, Vec<u8>>,
    pub protocol_version: u16,
    pub tool_version: String,
    pub template_digests: BTreeSet<ObjectId>,
    pub minimum_verification: VerificationPolicy,
    pub environment_refs: BTreeSet<String>,
    pub plan_digest: ObjectId,
}
~~~

The on-disk schema identifier is jails.prepared-plan.v1. Objects are base64 in JSON, keyed by their SHA-256 object ID; the reader recomputes every ID before use. Plan digest is the domain-separated canonical digest of every preceding field. Environment refs contain names such as DEV_DATABASE_URL, never values, and may supply verification or post-commit effects only. A secret that would change desired project bytes makes plan-out refuse.

Plan-in performs, in order: schema/version decode, plan digest check, object digest check, root binding check, current generation/preimage recheck, template/protocol compatibility check, requested verification at least as strong as minimum_verification, then normal guarded commit. It never reparses the original semantic CLI arguments and never fetches a missing object from the network.

### 7.12 Conformance suite

Every implementation issue SHALL name the RFC test IDs it satisfies. The minimum suite is:

| ID | Property | Required test |
|---|---|---|
| JDX-INV-001 | preview/apply parity | compare ordered operation and prepared-after digests |
| JDX-INV-002 | no preview effects | fake filesystem/process/database spies remain untouched |
| JDX-INV-003 | deterministic plan | randomized map order, locale, temp root, and repeated runs produce equal codec bytes |
| JDX-INV-004 | stale refusal | mutate one preimage between verify and commit; no file is committed |
| JDX-INV-005 | crash safety | failpoint at every durable transition; next run rolls forward or reports recovery-blocked |
| JDX-INV-006 | runtime independence | remove jails and .jails cache, then build/test generated project |
| JDX-JAVA-001 | release selection | new Maven and Gradle projects use release 26, explicit 21 is preserved, and an adopted release-21 project is not upgraded |
| JDX-JAVA-002 | toolchain guard | a host compiler unable to target 26 refuses the default gate with a typed fix; no default template enables preview features |
| JDX-OUT-001 | human/JSON agreement | both renderers project the same semantic result fixture |
| JDX-OUT-002 | JSON schema | golden key order, null fields, sorted sets, newline, and no ANSI |
| JDX-OUT-003 | truthful directory effects | nested creates report mkdir only for absent parents; existing directories are omitted, collisions refuse, and preview/receipt operations agree |
| JDX-SQL-001 | source mapping | database error after parameter rewrite maps to original SQL span |
| JDX-SQL-002 | frozen drift | query, migration order, mapping, dialect, and server-major changes each fail frozen mode |
| JDX-SQL-003 | generated types | bind/read types equal live described metadata for the PostgreSQL matrix |
| JDX-SQL-004 | closed dynamic criteria | every allowed filter/sort fragment verifies; request values never become identifiers or SQL fragments |
| JDX-SCHEMA-001 | rename safety | similarity alone never creates a Rename operation |
| JDX-SCHEMA-002 | dependency order | randomized schema declarations produce the same valid topological order |
| JDX-SCHEMA-003 | opaque preservation | an operation touching an unsupported object refuses with its identity and provenance |
| JDX-TEST-001 | affected-test safety | selected tests are a superset of failures from an all-tests oracle |
| JDX-TEST-002 | epoch safety | stale daemon completion is discarded |
| JDX-TEST-003 | one requested universe | auto partition union equals build-engine discovery; no selector is omitted or duplicated |
| JDX-TEST-004 | one report contract | build and warm engines support failed, fail-fast, slowest, human, and JSON through equal normalized fixtures |
| JDX-TEST-005 | compilation policy | auto repairs stale output through the selected owner; ide/build/none obey their refusal and invocation contracts on Maven and Gradle |
| JDX-WATCH-001 | overflow safety | overflow forces full rescan and visible widening |
| JDX-WATCH-002 | isolation and recycling | ineligible tests delegate; leak, metaspace, generation, engine, and classpath thresholds recycle the daemon |
| JDX-RUN-001 | fast launch parity | current Maven/Gradle outputs launch directly with the exact main/profile/argv/environment seen by build-tool mode |
| JDX-RUN-002 | explicit service policy | existing/start/none perform only their declared checks/actions and report service time separately |
| JDX-RUN-003 | single compiler owner | ide/build/auto/none watch fixtures never launch two compilers; output change triggers one DevTools restart |
| JDX-RUN-004 | readiness and lifecycle | process-started/started/ready remain distinct; signals and every child exit propagate without orphaning a process |
| JDX-CLI-001 | shell-safe DSL | documented fields produce identical argv in Bash, Zsh, Fish, PowerShell, and direct execution |
| JDX-DATASOURCE-001 | explicit service boundary | live checks never start a service and reject an endpoint unreachable from the command consumer |
| JDX-DIAG-001 | read-only diagnostics | doctor, why, and editor diagnostics perform no writes, builds, application boot, or service startup |
| JDX-CONTRACT-001 | compatibility | seeded changes cover every classification row in Section 7.10 |
| JDX-EDITOR-001 | completion authority | every command, alias, option, exclusion, value, and positional completion matches the Clap command graph without a Lua vocabulary table |
| JDX-EDITOR-002 | structured file actions | prepared reports and applied receipts open the exact files without parsing human stdout |
| JDX-EDITOR-003 | diagnostic fidelity | path, UTF-8 range, severity, code, evidence, and typed fixes map losslessly; an older epoch is discarded |
| JDX-EDITOR-004 | plan identity | preview and confirmation apply the same plan digest through plan-in; stale preimages and cancellation write nothing |
| JDX-EDITOR-005 | event streaming | partial/multiple JSONL chunks, cancellation, restart, crash, sequence gaps, and malformed frames never block the UI or publish stale state |
| JDX-EDITOR-006 | Java-tool coexistence | jails, jdtls, and javac namespaces remain separate; active test watch suppresses duplicate save compilation and never attaches a debugger |
| JDX-EDITOR-007 | root and capability selection | Maven, Gradle, wrapper, manifest, monorepo, unsupported-schema, and JDK-release fixtures negotiate the correct root and capabilities |
| JDX-TOOL-001 | transparent process semantics | TTY, byte streams, signals, terminal resize, working directory, and every child exit class are preserved |
| JDX-TOOL-002 | secret boundary | credentials and sensitive headers occur only in child environment/stdin and never argv, debug, reports, state, or process inspection |
| JDX-HTTP-001 | route-aware curl | route/method/parameter/base-origin fixtures produce exact argv; ambiguity and cross-origin credential forwarding refuse; no app starts |
| JDX-DB-001 | pgcli-first console | pgcli receives libpq environment, missing pgcli does not fall back, explicit psql works, and neither path changes service state |
| JDX-CONSOLE-001 | application-aware REPL | Maven and Gradle fixtures on releases 21 and 26 boot a Spring context, expose every helper, and run cleanup on every exit path |
| JDX-CONSOLE-002 | current classpath | stale outputs refuse by default; explicit compile uses the wrapper and the resulting runtime classpath fingerprint |
| JDX-RUNNER-001 | noninteractive app script | file/stdin snippets boot the same context, propagate snippet failure, reject inline/path escape, and close cleanly |
| JDX-LOG-001 | read-only log bridge | only declared services resolve; bounded/follow behavior preserves child status and never mutates a service |
| JDX-LIFE-001 | sealed migration history | scaffold sync, destroy, rename, undo, and repair cannot replace, rename, or delete a sealed migration; exact restoration is the only same-version write |
| JDX-LIFE-002 | explicit destroy storage | plain table-backed destroy refuses; preserve records a tombstone; drop appends a dependency-safe migration and reports planned versus applied truthfully |
| JDX-LIFE-003 | forward field evolution | add/rename/type/nullability/drop fixtures update all projections through new migrations and never rewrite their create migration |
| JDX-LIFE-004 | deterministic repair | edited, missing, old-applied, live-behind, object-missing, and checksum-divergent fixtures either produce the specified roll-forward plan or refuse without writes |
| JDX-LIFE-005 | preserved-storage revival | revive reuses EntityId, binding, model, queries, and migration lineage without another create; drop-planned and live-observed-applied states refuse |
| JDX-RENAME-001 | stable identity | logical, table, file, query-owner, and editor-name changes preserve one EntityId and produce deterministic plans |
| JDX-RENAME-002 | storage strategies | preserve-table records an explicit binding; single-cutover creates only a forward migration; rolling requires matching campaign and retirement attestation |
| JDX-RENAME-003 | dependency safety | unresolved hand-written Java/SQL, opaque objects, incomplete scans, collisions, and stale campaign inputs all refuse without writes |
| JDX-RENAME-004 | contract boundary | API names remain stable by default; explicit API rename is classified and seeded breaking-policy fixtures refuse |
| JDX-EXP-001 | sandbox safety | every supported handoff rolls back; every independent-transaction escape fails explicitly |

Generated-project gates SHALL include Maven and Gradle where the feature claims both, Spring and plain Java where the feature claims both, Java 21 and the repository default release 26, and PostgreSQL server majors selected by the supported-version policy. A fixture is not evidence for a combination it does not actually compile or execute.

### 7.13 Work-package dependency graph for coding agents

Agents should implement vertical work packages, each leaving the workspace green:

| Package | Deliverable | Depends on | Completion evidence |
|---|---|---|---|
| DX-001 | freeze result v1; add diagnostic registry and v2 serializer | none | JDX-OUT-001/002 |
| DX-002 | prepared diff/AST renderer, truthful directory-effect projection, and prepared-after digest | DX-001 | JDX-INV-001/003 and JDX-OUT-003 |
| DX-003 | append Query resource variant; add non-owning observed SchemaObjectId | DX-001 | codec golden, old-tag compatibility, and no schema ledger owner |
| DX-004 | align the new-project JDK 26 default across project models, CLI, templates, toolchains, docs, fixtures, and compiler gates | none | JDX-JAVA-001/002 |
| DX-005 | seal migration-history ownership and make table-backed destroy require preserve/drop without deleting old migrations | none | JDX-LIFE-001/002 plus the Task regression fixture |
| DX-006 | resource status, forward field-evolution requests, roll-forward repair, and preserved-storage revival | DX-002, DX-005 | JDX-LIFE-003/004/005 plus generated-project gates |
| DX-007 | optional live Flyway/catalog evidence and explicit post-commit migrate effect for lifecycle operations | DX-006, DX-020, DX-050 | JDX-LIFE-002/004 against the PostgreSQL matrix |
| DX-010 | extend the existing app manifest to ApplicationSpecV1; add decoder and app plan | DX-003 | CLI/manifest digest equivalence |
| DX-020 | PostgreSQL catalog IR and migration application | DX-003 | deterministic catalog fixtures |
| DX-021 | named SQL parser and offline contracts | DX-020 | JDX-SQL-001/002 |
| DX-022 | live describe and Java projection | DX-021 | JDX-SQL-003 plus generated compile |
| DX-030 | three-authority schema differ and migration lint | DX-020 | JDX-SCHEMA-001/002 |
| DX-040 | graph epoch store and affected selection v2 | DX-001 | JDX-TEST-001 |
| DX-041 | framed testd v2 IPC | DX-040 | protocol, retry, stale-epoch tests |
| DX-042 | internal testd watch engine, isolation policy, and recycle controls | DX-041 | JDX-TEST-002/JDX-WATCH-001/002 |
| DX-043 | make `jails test` own compile policy, build/warm partitioning, normalized reports, watch, and compatibility aliases | DX-041, DX-042 | JDX-TEST-003/004/005 plus Maven/Gradle CLI gates |
| DX-044 | shared runtime classpath cache and direct application launcher with explicit services/readiness/watch policy | DX-004, DX-043 | JDX-RUN-001/002/003/004 |
| DX-050 | explicit datasource resolver and consumer-reachability diagnostics | DX-001 | JDX-DATASOURCE-001 |
| DX-060 | composed verification and HTTP contracts | DX-021, DX-030 | JDX-CONTRACT-001 |
| DX-070 | history/show/undo extensions | DX-002 | crash sweep and edited-after-image cases |
| DX-081 | editor handshake, completion, symbol, and diagnostic reports over command-result v2 | DX-001, DX-040 | JDX-EDITOR-001/003/007 |
| DX-082 | migrate `jails.nvim` to asynchronous v2 plans, receipts, diagnostics, pickers, and test-watch events | DX-002, DX-042, DX-081 | JDX-EDITOR-002/004/005/006 |
| DX-083 | transparent tool executor plus route-aware curl and read-only logs | DX-050, DX-081 | JDX-TOOL-001/002, JDX-HTTP-001, JDX-LOG-001 |
| DX-084 | pgcli/psql datasource console | DX-050, DX-083 | JDX-DB-001 |
| DX-085 | Maven/Gradle runtime classpath providers and Spring-booted JShell console/runner | DX-004, DX-083 | JDX-CONSOLE-001/002, JDX-RUNNER-001 |
| DX-086 | stable EntityId, rename impact report, and preserve-table strategy | DX-006, DX-010, DX-040 | JDX-RENAME-001/003 |
| DX-087 | single-cutover and rolling storage rename campaigns | DX-030, DX-060, DX-086 | JDX-RENAME-002/004 plus clean-database and generated-project gates |
| DX-090 | generated SQL sandbox spike | DX-022, DX-050 | JDX-EXP-001 plus benchmark gate |

For each package, the implementing agent SHALL:

1. confirm current symbol locations and protocol versions before editing;
2. add or extend protocol values first, including codec and compatibility tests;
3. keep observation, pure planning, preparation, commit, and rendering in their owning crates;
4. add one end-to-end CLI scenario using the real route;
5. add failure tests before claiming the happy path complete;
6. record benchmarks only with fixture, warm/cold state, sample count, p50, p95, and cache reason;
7. leave experimental behavior behind an explicit flag;
8. update this RFC when an accepted interface changes, including rationale and migration effect.

An agent MUST NOT mark a package complete based only on unit tests of a parser or template. Completion requires its listed conformance evidence and a generated-project or real-route test at the boundary the feature claims.

### 7.14 Experimental interface contracts

Experimental means the interface and safety bar are specified, but the feature is deferred from the roadmap and is not eligible to become a default until a separately approved, time-boxed spike passes its promotion gate.

#### 7.14.1 Generated SQL sandbox

The generated test-only API is:

~~~java
@Target({ElementType.TYPE, ElementType.METHOD})
@Retention(RetentionPolicy.RUNTIME)
@ExtendWith(ProjectSqlSandboxExtension.class)
public @interface SandboxedDatabase {
    SandboxMode value() default SandboxMode.SHARED_TRANSACTION;
}

public enum SandboxMode {
    SHARED_TRANSACTION,
    ISOLATED_SCHEMA
}
~~~

ProjectSqlSandboxExtension and SandboxDataSource are generated beneath src/test/java in the project package. Production sources and dependencies do not reference them.

Shared transaction semantics:

1. beforeEach checks out one physical connection and begins one root transaction;
2. SandboxDataSource returns logical proxies over that connection to application and test threads;
3. logical close releases a handle but does not close the physical connection;
4. logical commit, connection state mutation outside the supported set, a second concurrent statement, or acquisition after timeout throws SandboxEscapeException;
5. afterEach rolls back the root transaction, restores connection state, closes it, and fails the test if any logical handle leaked;
6. a failed cleanup discards the physical connection and fails the test run.

The mode serializes statements with a fair lock and has a default five-second acquisition timeout. It does not claim independent transaction semantics. A test using `REQUIRES_NEW`, an explicit commit, `AFTER_COMMIT` callbacks, outbox visibility across transactions, advisory locks requiring multiple sessions, unsupported thread handoff, or concurrent database statements MUST use `ISOLATED_SCHEMA`. In shared-transaction mode `AFTER_COMMIT` is never reported as having run: the extension refuses the test before execution when it can prove such a listener and fails on a captured invocation otherwise. Escape is a failing diagnostic with the suggested annotation change; jails MUST NOT silently rerun the test under different semantics.

JDX-EXP-001 requires:

- HTTP request handoff, virtual-thread handoff, nested savepoint, timeout, leaked handle, pool exhaustion, parallel test, and cleanup-failure cases;
- explicit failing cases for `REQUIRES_NEW`, commit, `AFTER_COMMIT`, unsupported thread handoff, and concurrent sessions;
- the same repository contract passing under shared transaction and isolated schema;
- p50/p95 comparison with isolated schema on the reference integration suite;
- no residual row after every injected failure point.

Promotion requires zero semantic escapes in the supported corpus and at least a twofold p95 improvement for eligible tests. Otherwise the spike is rejected and isolated schema remains the default.

### 7.15 Editor and Neovim integration

The editor boundary is a versioned CLI protocol. `jails.nvim` is the reference adapter and the personal Neovim configuration is an acceptance consumer, not a product dependency. The CLI owns root discovery, command vocabulary, completion semantics, project symbols, diagnostics, plans, receipts, and daemon epochs. The plugin owns asynchronous process integration and projection into Neovim APIs. Neither the plugin nor a dotfiles module may infer domain behavior from terminal prose.

#### 7.15.1 Handshake and capability negotiation

`jails editor handshake --path <path> --output json` is read-only. `path` may name a file or directory and is only the starting point for canonical project-root discovery. Its report is `jails.editor-handshake.v1`:

~~~rust
pub struct EditorHandshakeV1 {
    pub editor_protocol: u16,
    pub cli_version: String,
    pub command_result_schema: String,
    pub event_schema: String,
    pub project: EditorProjectV1,
    pub capabilities: BTreeSet<EditorCapability>,
}

pub struct EditorProjectV1 {
    pub identity: ObjectId,
    pub root_digest: ObjectId,
    pub build_systems: BTreeSet<BuildSystem>,
    pub java_release: JavaRelease,
    pub new_project_default_java_release: JavaRelease,
    pub source_roots: Vec<EditorSourceRoot>,
}

pub struct EditorSourceRoot {
    pub path: ProjectRelativePath,
    pub kind: EditorSourceKind,
}

pub enum EditorSourceKind {
    MainJava,
    TestJava,
    MainResources,
    TestResources,
    Generated,
}

pub enum EditorCapability {
    CompletionV1,
    SymbolsV1,
    DiagnosticsV1,
    PreparedPlansV1,
    TestWatchEventsV1,
    TestdV2,
}
~~~

The first supported `editor_protocol` is 1. Both schema strings are exact (`jails.command-result.v2` and `jails.event.v1`), the new-project default is 26, and `java_release` is the observed project value. Paths are project-relative slash paths and source roots are sorted by `(kind, path)`. The handshake does not expose an absolute root, environment value, process ID, or credential. Unsupported protocol or schema versions produce `protocol-mismatch`; the plugin falls back only to documented compatibility behavior.

The plugin caches a successful handshake by `(resolved executable path, executable version, executable mtime, root_digest)`. It invalidates the cache when any member changes, when a build file changes, or when the CLI returns `stale` or `protocol-mismatch`. It MUST NOT cache a failed handshake indefinitely.

#### 7.15.2 Completion owned by the CLI

The exact request is:

~~~text
jails editor complete --arg-index <n> --byte-offset <n>
  [--path <path>] --output json -- <argv>...
~~~

`argv` excludes the executable and is already tokenized. `arg-index` and `byte-offset` follow Section 6.11. Completion performs no mutation, service startup, project lock acquisition, or daemon startup. It asks the same Clap command definitions and typed project vocabulary used by dispatch; aliases, option arity, mutually exclusive flags, and positional rules MUST NOT be copied into Lua.

The report is `jails.editor-completion.v1`:

~~~rust
pub struct EditorCompletionV1 {
    pub input: EditorCursor,
    pub replace: EditorReplacement,
    pub candidates: Vec<EditorCompletionCandidate>,
}

pub struct EditorCursor {
    pub argument_index: u32,
    pub byte_offset: u32,
}

pub struct EditorReplacement {
    pub argument_index: u32,
    pub start_byte: u32,
    pub end_byte: u32,
}

pub struct EditorCompletionCandidate {
    pub value: String,
    pub display: String,
    pub kind: EditorCompletionKind,
    pub description: Option<String>,
}

pub enum EditorCompletionKind {
    Command,
    Option,
    Value,
    Path,
    Type,
    Test,
    Symbol,
}
~~~

The replacement lies inside the selected argument and uses UTF-8 byte offsets. Candidates are unique and sort by `(kind rank, value, display)`. `value` is inserted verbatim into the token; it contains no shell quoting. An editor is responsible for its own command-line escaping only when it renders tokens into a shell-facing UI.

#### 7.15.3 Project symbols and source locations

The symbol command supports the closed v1 kinds `routes`, `beans`, `queries`, `tests`, and `types`:

~~~text
jails editor symbols <kind> [--query <text>] [--path <path>] --output json
~~~

`query` is a case-folded subsequence filter over label, detail, and semantic ID. An omitted query returns the full currently known set. The report is `jails.editor-symbols.v1`:

~~~rust
pub struct EditorSymbolsV1 {
    pub root_digest: ObjectId,
    pub epoch: u64,
    pub kind: EditorSymbolKind,
    pub symbols: Vec<EditorSymbol>,
}

pub struct EditorSymbol {
    pub id: String,
    pub label: String,
    pub detail: Option<String>,
    pub location: Option<EditorLocation>,
    pub evidence: EvidenceLevel,
}

pub struct EditorLocation {
    pub path: ProjectRelativePath,
    pub range: EditorRange,
}

pub struct EditorRange {
    pub start: EditorPosition,
    pub end: EditorPosition,
}

pub struct EditorPosition {
    pub line: u32,
    pub byte_column: u32,
}
~~~

Lines and columns are zero-based; columns count UTF-8 bytes from the start of the line; the end position is exclusive. Every boundary must land on a code-point boundary. Items without a source position have `location = null` and remain displayable but not jumpable. IDs use these canonical forms:

~~~text
route:<METHOD>:<normalized-route>:<qualified-handler>
bean:<qualified-java-type>:<declaring-member-or-type>
query:<slice>.<query>
test:<junit-unique-id>
type:<qualified-java-type>
~~~

Symbols sort by `(label case-folded, id)`. A route or bean picker consumes these fields directly; it MUST NOT depend on bespoke payload keys or synchronously run `routes --json`/`beans --json` and reinterpret their report shapes.

#### 7.15.4 Diagnostics and fixes

The exact command is:

~~~text
jails editor diagnostics --scope buffer --file <project-relative-path>
  [--path <path>] [--evidence parsed|offline|live] --output json
jails editor diagnostics --scope project
  [--path <path>] [--evidence parsed|offline|live] --output json
~~~

`--file` is required for buffer scope and invalid for project scope. `parsed` never starts a service, `offline` may use checked project evidence, and `live` requires an explicit, already reachable datasource under Section 7.9. The report is `jails.editor-diagnostics.v1`:

~~~rust
pub struct EditorDiagnosticsV1 {
    pub root_digest: ObjectId,
    pub epoch: u64,
    pub scope: EditorDiagnosticScope,
    pub diagnostics: Vec<Diagnostic>,
}

pub enum EditorDiagnosticScope {
    Buffer(ProjectRelativePath),
    Project,
}
~~~

Every SourceLabel in a diagnostic uses the project-relative, zero-based UTF-8 range rules above. The Neovim adapter owns one namespace named `jails` and maps Note/Warning/Error to `vim.diagnostic.severity.INFO/WARN/ERROR`. It sets `source = "jails"`, preserves the stable diagnostic code, and places evidence plus serialized TypedFix values in `user_data`. A code action invokes the typed canonical request through preview; it never executes a shell string.

Updates replace only diagnostics for `(root_digest, epoch, scope)`. A result older than the newest observed epoch is discarded. Buffer results update only the named buffer. Project results update loaded buffers and populate a quickfix list for unloaded paths. The adapter MUST NOT clear or overwrite `jdtls`, `javac_xlint`, compiler, or any other namespace.

Structured diagnostics are preferred. The existing `compiler jails` errorformat remains a one-release compatibility fallback when handshake negotiation shows that diagnostics v1 is unavailable; it is not extended with new semantic formats.

#### 7.15.5 Preview, apply, and created files

`:JailsPreview <command> <args...>` runs that mutating command with `--pretend --output json --plan-out <file>`. The plugin creates the plan beneath a private mode-0700 temporary directory and removes the directory after apply, cancellation, invalidation, or editor exit. It renders the `Prepared` report in a read-only, unlisted, `nofile` buffer named `jails://plan/<operation-id>`. The buffer shows ordered operations, risk, verification evidence, semantic edits, and optional unified diffs from the structured report; it does not parse the human renderer.

Apply uses the same command path with the original semantic arguments removed and `--plan-in <same-file> --output json --yes`. For example, a preview of `generate scaffold ...` applies as `generate scaffold --plan-in <file> --output json --yes`. Clap definitions for every mutating leaf SHALL make semantic arguments and `--plan-in` mutually exclusive. The CLI rechecks the PreparedPlanV1 digest and preconditions from Section 7.11 and never reconstructs intent from the earlier argv.

Confirmation uses `vim.ui.select` and displays the plan digest plus risk summary. Cancellation performs no project mutation. After an Applied receipt, the plugin derives created/changed project-relative paths from receipt operations, fills quickfix, and opens the first created Java file when `open_created` is enabled. It MUST NOT extract paths from stdout lines such as `create ...`.

#### 7.15.6 Test-watch stream and Java tooling coexistence

`watch_start` launches `jails test --watch --output json` as an asynchronous job and decodes `jails.event.v1` JSON Lines incrementally. Reads may split one JSON object across arbitrary chunks or contain multiple objects per chunk; the adapter buffers until newline and bounds an unterminated frame to 8 MiB. Stdout is protocol-only. Stderr is appended to a terminal/log buffer and is never decoded as an event.

The adapter rejects a sequence gap, session change without a new start, malformed event, or unsupported schema with a visible `protocol-mismatch`, stops trusting that stream, and exposes the ordinary CLI fallback. Events older than the newest epoch are discarded. Stopping sends the normal termination request, waits up to two seconds, then kills only the exact job handle it owns.

Per root, the adapter exposes `cold`, `starting`, `ready`, `testing`, `stale`, `failed`, or `stopped` through `require('jails').watch_status(root)`. It emits these Neovim User autocommands with `data = { root_digest, session, epoch }`:

~~~text
JailsWatchStarted
JailsWatchReady
JailsWatchStopped
~~~

`JailsWatchReady` additionally carries `data.current = true|false`; `JailsWatchStopped` carries `data.reason`. Statusline integrations read `watch_status` instead of scraping messages.

Java ownership remains explicit:

1. jdtls owns language intelligence, refactoring, debugging, and its autobuild into the configured class output.
2. `jails test --watch` consumes fresh class output and follows its explicit compile policy; the daemon itself never invokes a compiler.
3. Debug-session hot code replacement remains owned by jdtls/DAP. The plugin never attaches a second debugger or instrumentation agent.
4. A standalone save-time `javac -Xlint:all` diagnostic source may continue when test watch is inactive. While `watch_status(root)` is starting, ready, testing, or stale, the acceptance configuration suppresses that duplicate compile and resumes it on `JailsWatchStopped`.
5. Suppression prevents a new `javac` job; it never clears existing `javac_xlint` or jdtls diagnostics.

The current JDK-26-aware jdtls and `javac -Xlint:all --release <project-release>` configuration is the acceptance fixture for these coexistence rules. It remains outside the product dependency graph.

#### 7.15.7 `jails.nvim` public surface

The target setup contract is:

~~~lua
require('jails').setup({
  command = 'jails',
  root_markers = {
    '.jails/app.toml', 'pom.xml', 'mvnw',
    'build.gradle', 'build.gradle.kts', 'gradlew', '.git',
  },
  terminal = { height = 12 },
  output_schema = 'v2',
  diagnostics = { enabled = true, on_save = 'offline' },
  watch = { auto_start = false, statusline = true, compile = false },
  open_created = true,
})
~~~

`terminal_height` remains a deprecated alias for `terminal.height` for one compatibility release; specifying both is invalid. Unknown setup keys are errors reported by `:checkhealth jails`, not silently ignored.

The public Lua functions are `setup`, `health`, `run`, `preview`, `apply_plan`, `watch_start`, `watch_stop`, `watch_toggle`, `watch_status`, `test_at_cursor`, `pick`, and `complete`. All operations that can touch a project are asynchronous and accept an optional callback `(result, error)`; no picker, completion, diagnostic, or health path calls `vim.system(...):wait()` on the UI thread. Superseding per-root completion/diagnostic requests cancel the older process and ignore a late callback.

The stable Ex commands are:

~~~text
:Jails <args...>
:JailsPreview <args...>
:JailsWatch[!]
:JailsHealth
~~~

`:JailsWatch` toggles test watch for the current root; bang restarts only the owned test daemon. It never restarts an application. Plugin defaults add no global keymaps. Buffer-local Java mappings may be enabled through setup and must call the public functions, allowing a personal configuration to replace the keys without forking behavior.

The migration order is handshake/decoder first, then structured completion and pickers, then plans/receipts, diagnostics, and the test-watch event stream. Until each capability negotiates successfully, only its documented compatibility path remains active; the adapter never mixes human parsing with a partially decoded v2 result.

### 7.16 Application developer-tool gateway

#### 7.16.1 Shared process contract

These commands delegate to standard tools; they do not emulate them:

~~~rust
pub enum DeveloperTool {
    Curl,
    Pgcli,
    Psql,
    Jshell,
    Compose,
}

pub struct PreparedToolInvocation {
    pub tool: DeveloperTool,
    pub executable: ResolvedExecutable,
    pub args: Vec<OsString>,
    pub public_environment: BTreeMap<OsString, OsString>,
    pub secret_environment: SecretEnvironment,
    pub working_directory: CanonicalRoot,
    pub stdio: ToolStdio,
    pub redacted_debug: String,
}

pub enum ToolStdio {
    InheritTty,
    ProxyBytes,
}

pub trait DeveloperToolAdapter {
    type Request;

    fn probe(&self, environment: &HermeticEnvironment)
        -> Result<ToolProbe, Diagnostic>;

    fn prepare(
        &self,
        root: &CanonicalRoot,
        request: &Self::Request,
        facts: &ProjectFacts,
    ) -> Result<PreparedToolInvocation, DiagnosticSet>;
}
~~~

`prepare` is read-only and cannot launch a process. `jails-drive` is the only owner of probe and execution; protocol/spec types contain tool IDs and environment-variable names, never executable handles or secret values. Interactive commands require a TTY, inherit stdin/stdout/stderr, forward `SIGINT`, `SIGTERM`, and terminal resize, and return the exact child exit status (signal termination maps to `128 + signal` on Unix). `request` proxies response bytes on stdout and curl diagnostics on stderr. Global `--output json` is invalid for these transparent sessions; structured preflight is available from `--print --output json` without launching the child.

Lookup uses the hermetic command environment and rejects aliases containing whitespace or shell operators. The default executables are `curl`, `pgcli`, `psql`, the `jshell` beside the selected Java executable, and the existing project-selected Compose implementation. Optional overrides are executable paths, not command strings. No execution uses `sh -c`, reparses a command line, installs a tool, or silently substitutes one tool for another.

Debug and `--print` output MUST redact values for environment entries marked secret and headers named `authorization`, `proxy-authorization`, `cookie`, `set-cookie`, or `x-api-key`, case-insensitively. A redacted value renders as `<redacted:ENV_NAME>`. The child may receive a password in its environment, but credentials, bearer tokens, inline request bodies, JShell snippets, and session transcripts never enter a report, cache, journal, receipt, plan, or process argument.

#### 7.16.2 Route-aware curl

The v1 grammar is:

~~~text
jails request <METHOD> <route-id|route-name|origin-relative-path>
  [--profile <name> | --base-url <http-or-https-origin>]
  [--param <name>=<value>]... [--query <name>=<value>]...
  [--header <name>=<value>]... [--header-env <name>=<ENV_NAME>]...
  [--json @<project-relative-path>|@-]
  [--data @<project-relative-path>|@-]
  [--timeout <duration>] [--follow] [--print]
~~~

`METHOD` is uppercase and from the route model's supported methods. A target beginning `/` is an origin-relative literal path. Any other target resolves against route ID first and then an exact unique route name; ambiguity reports sorted candidates. A resolved route MUST support the requested method. Every `{parameter}` in its normalized path has exactly one `--param`, extra parameters fail, values are UTF-8 percent-encoded as path-segment data, and query pairs are encoded by curl without becoming option tokens. Literal paths cannot contain a scheme or authority. An absolute target is accepted only through explicit `--base-url` plus an origin-relative path, preventing a route profile from accidentally forwarding credentials to another host.

Resolution of the base origin is exact: explicit `--base-url`, then `[tools.http.profiles.<name>].base_url` in `.jails/app.toml`, then the existing `run` configuration's explicit local origin. If no source exists, the command refuses; it does not guess a port or start `jails run`. A profile may declare header environment references, never header secrets. A command-line header overrides a non-secret profile header; a command cannot replace the host/authority header unless `--allow-host-header` is explicit.

The prepared curl argv starts with `--silent --show-error --fail-with-body --request <METHOD> --url <URL>`. `--json` adds curl's JSON request semantics and accepts only `@file` or `@-`; `--data` is mutually exclusive and likewise file/stdin only. `--follow` adds redirect following but strips all environment-backed sensitive headers on cross-origin redirect; if the installed curl cannot prove that policy, preparation refuses. `--print` emits the exact shell-escaped public argv with environment placeholders and performs no network I/O. Curl's response body and exit status remain authoritative.

#### 7.16.3 PostgreSQL console

~~~text
jails db console [--database <name>] [--profile <name>]
  [--client pgcli|psql] [--single-connection]
~~~

The default client for PostgreSQL is `pgcli`; `psql` is an explicit compatibility choice. `--single-connection` is valid only for pgcli. The datasource resolves through Section 7.9, and the selected endpoint MUST be reachable from the CLI process. A named database selects a declared datasource; an unknown or ambiguous name fails before tool lookup.

The pgcli adapter passes host, port, username, and database through `PGHOST`, `PGPORT`, `PGUSER`, and `PGDATABASE`; a resolved password uses `PGPASSWORD` in the child-only secret environment. It supplies `--warn` and optionally `--single-connection`, but never puts a connection URI or password in argv. `psql` receives the equivalent libpq environment. Missing `pgcli` returns `tool-unavailable` with its probe path and install hints; it never falls back silently. Neither client path starts, stops, resets, migrates, or seeds a database.

#### 7.16.4 Spring-booted JShell console

~~~text
jails console [--profile <name>]... [--main <qualified-type>]
  [--web none|random|configured] [--compile]
jails runner --file <project-relative.jsh|->
  [--profile <name>]... [--main <qualified-type>]
  [--web none|random|configured] [--compile]
~~~

The default profile is `dev` and default web mode is `none`. Main-class resolution is explicit `--main`, then the existing run configuration, then exactly one source/build-model type annotated with `@SpringBootApplication`; zero or multiple candidates refuse. Profiles other than `dev` or `test`, and `web=configured`, require an interactive confirmation that names the main class, profiles, web mode, and redacted datasource sources; `--yes` may authorize this exact preflight in automation.

The runtime classpath provider is a parity boundary:

~~~rust
pub trait RuntimeClasspathProvider {
    fn resolve(
        &self,
        root: &CanonicalRoot,
        mode: ClasspathMode,
    ) -> Result<RuntimeClasspath, DiagnosticSet>;
}

pub enum ClasspathMode {
    ExistingOutputs,
    CompileThenResolve,
}

pub struct RuntimeClasspath {
    pub build_system: BuildSystem,
    pub release: JavaRelease,
    pub main_output: ProjectPath,
    pub resource_outputs: Vec<ProjectPath>,
    pub entries: Vec<CanonicalPath>,
    pub fingerprint: ObjectId,
}
~~~

Maven and Gradle implementations use the project wrapper when present and the same selected toolchain as `jails check`. Default `ExistingOutputs` performs no build and refuses `classes-stale` when sources/resources/build inputs are newer than their output snapshot. `--compile` explicitly invokes the ordinary build-tool compile/classes task before resolving. Dependency download policy is the project's existing hermetic process policy and is reported; the console path does not invent a second dependency cache. JShell comes from the selected JDK and MUST support the adopted project release; new JDK-26 projects therefore use JDK 26 JShell.

The launcher creates a mode-0600 startup script in a private mode-0700 temporary directory, passes it with JShell `--startup`, and deletes the directory after exit or failed launch. The script contains no secret. It imports the application types, starts a `ConfigurableApplicationContext` with `SpringApplicationBuilder`, selected profiles, and the requested `WebApplicationType`, and exposes this exact convenience surface:

~~~java
ConfigurableApplicationContext ctx;
<T> T bean(Class<T> type);
Object bean(String name);
Stream<String> beans();
Environment env();
<T> T tx(Supplier<T> work);
~~~

`beans()` returns sorted names. `tx` looks up exactly one `PlatformTransactionManager` on first use and evaluates the supplier through `TransactionTemplate`; absence or ambiguity is an ordinary JShell exception and no work runs. It makes no promise about work outside that transaction. `/exit`, EOF, child failure, or a forwarded termination signal closes the Spring context through `SpringApplication`'s registered shutdown handling. A conformance fixture additionally asserts `@PreDestroy` ran; failure to observe clean shutdown makes the session exit non-zero.

`runner` uses the identical startup script and classpath, then evaluates one trusted `.jsh` script and exits. The file must be project-relative and regular, or `-` for stdin. Inline Java is intentionally unsupported. Runner exit is non-zero when boot, any snippet, or context cleanup fails. Neither console nor runner launches Compose/Testcontainers, mutates project files, retains history on behalf of JShell, or claims a whole-session database sandbox.

#### 7.16.5 Service logs and diagnostics

~~~text
jails logs [<declared-service>]... [--follow] [--since <duration>] [--tail <count>]
~~~

This command resolves only services in the committed Compose declaration and delegates to its read-only logs operation. Unknown names refuse. `--follow` requires a TTY and owns only the log subprocess; stopping it does not stop a service. Without `--follow`, output is bounded by a default tail of 200 lines unless `--tail` is explicit. Debug output redacts known secret values from the prepared invocation, but the CLI cannot guarantee application log content is secret-free and therefore never persists or wraps it in JSON.

`doctor` reports the resolved path and version for curl, pgcli, psql, Compose, Java, JShell, Maven, and Gradle as applicable. It is read-only and returns inert typed fixes with official installation references; it never downloads or installs a tool.

### 7.17 Coordinated resource rename protocol

#### 7.17.1 CLI and canonical request

~~~text
jails rename resource <slice>.<current-name> <new-name>
  --strategy preserve-table|single-cutover|rolling
  [--table <target-table>]
  [--api preserve|rename] [--route <target-route>]
  [review flags]

jails rename storage <slice>.<current-name>
  --complete <campaign-id> --old-version-retired
  [review flags]
~~~

`--strategy` is required in non-interactive use; an interactive terminal recommends `preserve-table` as the lowest-risk complete state but records the chosen value in the canonical request. `--api` defaults to `preserve`. `--route` requires `--api rename`. `--table` is optional only when the current binding is conventional and the target table name is uniquely derived by the configured naming policy. An explicit current table binding requires an explicit target or `preserve-table`.

~~~rust
pub struct RenameResourceRequestV1 {
    pub entity: EntityId,
    pub expected_path: EntityPath,
    pub new_name: EntityName,
    pub strategy: RenameStrategy,
    pub target_table: Option<SqlName>,
    pub api: ExternalRenamePolicy,
    pub target_route: Option<RoutePath>,
}

pub enum RenameStrategy {
    PreserveTable,
    SingleCutover,
    Rolling,
}

pub enum ExternalRenamePolicy {
    Preserve,
    Rename,
}

pub struct CompleteStorageRenameRequestV1 {
    pub entity: EntityId,
    pub campaign: RenameCampaignId,
    pub old_version_retired: bool,
}
~~~

The selector resolves to exactly one `EntityId`; the request captures both ID and expected old path so a concurrently renamed manifest fails stale. The target logical name, Java package/type set, route policy, table policy, and migration risk participate in the request fingerprint.

#### 7.17.2 Dependency closure and ownership

Observation builds a `RenameImpactV1` from manifest edges, ledger ownership, Java symbol/bytecode facts, SQL parse/catalog facts, migration-derived and optional live catalog snapshots, contracts, fixtures, architecture rules, and editor symbol records:

~~~rust
pub struct RenameImpactV1 {
    pub entity: EntityId,
    pub owned: Vec<OwnedRenameProjection>,
    pub manual_java: Vec<SourceLabel>,
    pub manual_sql: Vec<SourceLabel>,
    pub external_names: Vec<ExternalContractName>,
    pub schema_dependencies: Vec<SchemaDependency>,
    pub opaque_blockers: Vec<SchemaObjectId>,
    pub evidence: Vec<EvidenceRecord>,
}
~~~

The owned closure includes generated domain/request/response/event types, repository ports and adapters, filenames, factories, contract tests, imports inside owned regions, manifest relations by `EntityId`, query owner IDs, generated mapper/projection names, architecture configuration, ledger paths, and editor semantic IDs. Exact generated paths and Java names derive again from the renamed typed model; they are not patched by string replacement.

Hand-written Java references, reflective type-name strings, reader-owned SQL mentioning the physical table, database routines/views/triggers/policies with unresolved bodies, and unowned deployment scripts are not mutated automatically. Each known location appears under `manual-edit-required`. A rename plan refuses until those references either target the prepared new name, are explicitly proven unrelated, or—only for `preserve-table`—remain valid because the physical name does not change. An incomplete Java/SQL scan widens to the relevant source roots; an incomplete or opaque database dependency blocks `single-cutover` and storage completion.

#### 7.17.3 Storage strategies

`preserve-table` changes the logical name and persists `TableBinding::Explicit(current_table)`. The report states `physical-table-preserved` and no migration is created. Generated SQL continues to use the bound physical table. This is a complete stable state, not a pending or inferred mismatch.

`single-cutover` prepares one ordinary forward Flyway migration after the latest known version. For PostgreSQL it uses dialect-quoted typed operations equivalent to:

~~~sql
ALTER TABLE public.tasks RENAME TO work_items;
~~~

The adapter also renames only constraint/index/owned-sequence names that are both recorded as generator-owned and equal to their expected old derived names. PostgreSQL object dependencies are tracked by catalog identity; the planner does not emit global textual SQL replacement. Applied migration files remain byte-identical. The risk set includes `DeploymentIncompatible`; verification runs the complete ordered migration history on a clean database, checks the prepared named SQL/contracts against the renamed catalog, compiles generated Java, and runs affected repository contracts. Commit writes project files only—the database changes later through the application's normal Flyway execution.

`rolling` performs a code-stage rename and persists:

~~~rust
pub struct RenameCampaignV1 {
    pub id: RenameCampaignId,
    pub entity: EntityId,
    pub from_logical: EntityPath,
    pub to_logical: EntityPath,
    pub current_table: SqlName,
    pub target_table: SqlName,
    pub code_stage_receipt: ReceiptId,
    pub state: RenameCampaignState,
}

pub enum RenameCampaignState {
    AwaitingOldVersionRetirement,
}
~~~

The entity uses `TableBinding::PendingRename` while the campaign is active, so generators keep using `current_table` and `check` reports the exact next command as a note. Only `rename storage --complete <id> --old-version-retired` may create the forward table-rename migration, switch the binding to the target, and remove the campaign. The boolean is an explicit operator attestation recorded in the request/receipt; `--yes` does not imply it. Completion refuses when the code-stage receipt, entity ID, current/target table, manifest generation, or observed catalog no longer matches. No compatibility view, dual write, trigger, or reverse migration is inferred.

#### 7.17.4 External contract policy and refusal rules

The default `api=preserve` keeps route paths, JSON property names, OpenAPI operation IDs when explicitly configured, event type/name/version, and externally visible error codes. Internal Java handler/type names and source locations change. `api=rename` changes only the external names listed in the prepared report, requires the ordinary compatibility check, and is refused by a no-breaking-change policy. Event renames require generation of a new event version; an existing event name/version is never rewritten in place.

The mutation is atomic at the project-file boundary and follows Section 7.1. Any of these conditions is `plan-refused`: target logical/table collision; ambiguous selector or naming policy; unpersisted identity collision; unresolved hand-written reference; stale generated owner; reader-owned SQL still naming a table scheduled for cutover; opaque database dependency; unsupported cross-schema move; active rename campaign on the same entity; or a prepared-after compile/SQL/contract failure. File undo may undo a `preserve-table` code-only rename when its receipt meets the ordinary safe-file rules. Every rename receipt containing a migration or campaign refuses undo and gives a forward corrective plan.

### 7.18 Fast application launch and watch contract

~~~rust
pub struct RunRequestV1 {
    pub launcher: RunLauncherPolicy,
    pub compile: RunCompilePolicy,
    pub services: RunServicePolicy,
    pub watch: bool,
    pub profiles: Vec<SpringProfile>,
    pub application_argv: Vec<OsString>,
}

pub enum RunLauncherPolicy { Auto, Classpath, BuildTool, Jar }
pub enum RunCompilePolicy { Auto, Ide, Build, None }
pub enum RunServicePolicy { Existing, Start, None }

pub struct RuntimeLaunchPlanV1 {
    pub main_class: JavaType,
    pub java: ResolvedExecutable,
    pub release: JavaRelease,
    pub classpath: RuntimeClasspath,
    pub launcher: ResolvedRunLauncher,
    pub compiler_owner: CompilerOwner,
    pub service_resolution: Vec<ServiceResolution>,
    pub readiness: Option<ReadinessProbe>,
    pub application_argv: Vec<OsString>,
}
~~~

`Auto` launcher selection is exact: use direct classpath when `RuntimeClasspath` and output snapshots are current; otherwise satisfy the compile policy, recompute the classpath, and use direct classpath; refuse if it remains stale. It does not silently switch to a packaged jar. `BuildTool` deliberately invokes `spring-boot:run` or `bootRun`; `Jar` requires a current executable artifact; `Classpath` refuses rather than compiling when its inputs are unavailable. The selected Java executable must support the observed release, and direct classpath order is main output, main resource outputs, then dependency entries in build-model order with canonical-path deduplication.

The runtime-classpath fingerprint covers build system and wrapper version, all build/settings/version-catalog/lock files, selected JDK/release, main/resources output digests, dependency coordinates and resolved artifact digests, main class, profiles that affect classpath, and resolver version. It excludes host-specific path prefixes from semantic identity while binding cached paths to the canonical root. Any mismatch invalidates the cache. A cache entry is ignored data, never authority.

`Existing` checks only committed declarations and explicitly configured endpoints and refuses with the exact `jails start` fix when a required service is absent. `Start` invokes the existing start command as a visible phase and reports its time; `None` performs no check and lets the application own the resulting startup failure. The run path never creates a dynamic shadow service, rewrites application connection configuration, or tears down a service when the foreground application exits.

Only one compile owner is active in watch mode. `Ide` requires a negotiated editor/output epoch and watches those output directories. `Build` uses OS source/resource notifications plus a content rescan and invokes the narrow wrapper task through mvnd or the Gradle daemon. `Auto` selects IDE only when output roots and epoch are proved, else build. `None` watches outputs and reports staleness without compiling. A watcher overflow rescans all tracked inputs. Spring DevTools owns restart; the CLI neither attaches an agent nor restarts a second application JVM.

The foreground run session emits ordered `jails.event.v1` phases `service-check`, `compiled`, `classpath-resolved`, `process-started`, `application-started`, `application-ready`, `restart-observed`, `stale`, and `stopped`. `process-started` means only that spawn succeeded. `application-started` requires an ordinary captured Spring started signal. `application-ready` requires the configured HTTP/TCP probe or ordinary captured readiness signal. Log-pattern heuristics may be labelled hypotheses but cannot produce ready. The CLI forwards signals to the exact child/process group it owns and returns its status; it stores no durable supervisor state.

The deprecated `--no-build` spelling normalizes for one release to `compile=none, launcher=auto` and still validates freshness. Maven and Gradle fixtures MUST receive identical tokenized application argv; no adapter joins argv into one string. AOT-cache preparation is experimental and separate: an archive binds to packaged artifact, JDK, classpath, JVM args, profiles, training inputs, and Spring version, and `run` refuses a non-exact cache. It is never used with watch mode.

### 7.19 Resource lifecycle, append-only migrations, and repair

#### 7.19.1 Durable model and migration seal

Generated projections and schema history have different deletion rules. The durable model makes that distinction explicit:

~~~rust
pub struct ResourceLifecycleV1 {
    pub entity: EntityId,
    pub expected_path: EntityPath,
    pub state: ResourceState,
    pub table: Option<TableBinding>,
    pub migrations: Vec<MigrationSealV1>,
}

pub enum ResourceState {
    Active,
    RetiredPreservingStorage { retired_by: ReceiptId },
    RetiredDropPlanned { migration: ProjectPath, retired_by: ReceiptId },
}

pub struct MigrationSealV1 {
    pub version: MigrationVersion,
    pub path: ProjectPath,
    pub content_digest: ObjectId,
    pub introduced_by: EntityId,
    pub receipt: ReceiptId,
}
~~~

A migration becomes sealed when the transaction that first publishes it commits. From then on, normal generate, evolve, rename, destroy, undo, and sync operations SHALL treat its path and bytes as append-only history. They may read it, restore its exact object-store image, or append a later migration; they may not replace, rename, or delete it. The operation graph has a distinct `SchemaHistory` disposition so retiring `ResourceOwner::Entity` does not turn an owned migration into a file absence.

On upgrade, existing ordered migration files are captured as seals before the first mutation that could affect a schema-backed entity. This adoption is a no-content-change transaction and records their current bytes as observed history; it does not claim that a database applied them. If current ledger/receipt objects already prove an earlier generated image that differs from the file, adoption refuses as `migration-edited-after-seal` and routes to repair.

The application model stores the declared active/retired state; detailed receipt IDs, migration seals, and live applied evidence stay in `ResourceLifecycleV1` in the durable ledger. Legacy command-authored entities receive the same receipt-backed ledger tombstone even before they are exported into a manifest. `app export` includes active and retired entries. A tombstone is compact identity and lineage, not generated runtime code, and cannot be garbage-collected while a table is preserved, a drop is only planned, a query/relationship refers to the entity, or a receipt may need its migration object.

#### 7.19.2 Field-evolution requests

~~~rust
pub struct EvolveFieldRequestV1 {
    pub entity: EntityId,
    pub expected_path: EntityPath,
    pub expected_table: SqlName,
    pub action: FieldEvolution,
    pub data: DataEvolution,
}

pub enum FieldEvolution {
    Add(FieldSpec),
    Rename {
        field: FieldId,
        new_name: FieldName,
        column: ColumnRenamePolicy,
    },
    ChangeType {
        field: FieldId,
        to: FieldType,
        strategy: TypeChangeStrategy,
    },
    SetNullability { field: FieldId, nullable: bool },
    Drop { field: FieldId, confirmed_column: SqlName },
}

pub enum DataEvolution {
    None,
    TypedLiteral(TypedLiteral),
    ReaderOwnedSql(ProjectPath),
}

pub enum ColumnRenamePolicy { Preserve, SingleCutover, Rolling }
pub enum TypeChangeStrategy { Safe, ExpandContract }
~~~

The request resolves entity and field identity before planning and binds the expected old table/column/type/nullability plus the latest migration seal into its fingerprint. CLI field strings are parsed once into `FieldSpec`; conversion/backfill SQL comes only from a project-relative file, is hashed as an input, and is never passed through a shell.

Every accepted evolution appends the next migration version and updates the declared model, owned Java, tests, fixtures, query contracts, and architecture/contracts in one prepared project transaction. Rules are conservative:

- adding a nullable column needs no data plan;
- adding a required column when rows may exist requires a typed literal or backfill file, and the migration orders add-nullable → backfill → set-not-null unless a proved empty-table plan is accepted;
- safe type change is limited to the dialect's reviewed lossless widening matrix; all other conversions require an expand/contract campaign or reader-owned conversion SQL and live verification;
- logical field rename preserves the physical column by default through an explicit binding; physical single-cutover/rolling follows the same dependency and deployment rules as Section 7.17;
- making a nullable field required needs a backfill or live proof that no null exists;
- drop requires the exact resolved column name, complete dependency evidence, destructive risk in the report, and a new forward `DROP COLUMN` migration without `CASCADE` or `IF EXISTS`.

`generate field` normalizes to Add and participates in these rules. Re-running `generate scaffold` for an active entity is a sync of owned projections from its existing declaration; it MUST NOT reinterpret the current domain record as permission to rewrite its create migration. A supplied field set that differs from the declaration returns a typed diff and the exact `resource field ...` commands required, with no write.

#### 7.19.3 Destroy and revive state machine

~~~rust
pub struct DestroyResourceRequestV2 {
    pub entity: EntityId,
    pub expected_path: EntityPath,
    pub storage: StorageRetirement,
    pub migration_effect: Option<DatasourceRef>,
}

pub enum StorageRetirement {
    Preserve { expected_table: SqlName },
    Drop { confirmed_table: SqlName },
}

pub struct ReviveResourceRequestV1 {
    pub entity: EntityId,
    pub expected_table: SqlName,
}
~~~

For a table-backed entity, `destroy scaffold` without a storage policy is `storage-policy-required` and writes nothing. Interactive mode may explain the choices but must still record one explicit policy. `--force` governs only otherwise permitted reader-edited file removal and never selects Drop or satisfies its exact table confirmation.

Preserve retires deletable owned projections, keeps every migration byte, persists the explicit table binding, and transitions to `RetiredPreservingStorage`. Drop does the same plus appends a dependency-ordered migration containing an exact qualified `DROP TABLE`; it refuses foreign keys, views, routines, queries, policies, or opaque dependents not handled by the plan and never emits `CASCADE` or `IF EXISTS`. The result is `drop-planned`, not `drop-applied`, until live Flyway evidence proves application.

`--migrate --datasource <name>` is allowed only with Drop. Before confirmation, the report lists the complete pending migration set because Flyway may apply migrations in addition to the new drop. Project files commit first; migration application is an explicit post-commit effect. Failure leaves a valid drop-planned project and a retryable effect receipt, never a claim that the transaction was rolled back. No service starts implicitly.

Revive is valid only from `RetiredPreservingStorage`. It reuses the same `EntityId`, table binding, last declared entity model, migration lineage, external names, and query ownership; it regenerates code/tests/contracts without `CREATE TABLE`. Offline migration evidence must prove that the table exists at the end of history. If a datasource is supplied, live table shape must also satisfy the declared model. `RetiredDropPlanned` refuses revive whether the drop is pending or live evidence says it was applied, because an append-only drop cannot be cancelled; the diagnostic offers a separately named new-resource/create migration plan and explains the data consequence.

#### 7.19.4 Status and roll-forward repair

`resource status` returns a report even when some authorities are unreadable:

~~~rust
pub struct ResourceStatusV1 {
    pub entity: Option<EntityId>,
    pub state: ResourceConsistency,
    pub declaration: AuthorityStatus,
    pub generated: AuthorityStatus,
    pub migration_history: AuthorityStatus,
    pub live: Option<AuthorityStatus>,
    pub table: Option<SqlName>,
    pub findings: Vec<Diagnostic>,
    pub next_requests: Vec<CanonicalRequestSyntaxV1>,
}

pub enum ResourceConsistency {
    Consistent,
    SourceDiverged,
    MigrationEditedAfterSeal,
    MigrationMissingAfterSeal,
    RuntimeSchemaBehind,
    RetiredStoragePresent,
    DropPending,
    DropObservedApplied,
    Ambiguous,
}
~~~

The report compares the declared entity, last sealed entity/migration images, current owned and reader-edited sources, the migration-derived catalog, and optional live Flyway/catalog facts. `live: null` means no datasource evidence; it is not an error and cannot be promoted to live verification. Secrets and the database identity remain redacted.

`repair --strategy roll-forward` has one deterministic automatic case: a sealed migration is edited or missing, its exact object-store image is available, and the current declared semantic delta from the sealed entity model is representable by supported forward operations. Preparation then restores the sealed bytes, appends one or more new migrations for that delta, reconciles owned projections, and verifies the complete ordered history on a clean database. With a datasource, it additionally validates Flyway history and the resulting live/prepared catalog relationship.

Repair refuses without writes when the intended model is ambiguous; a required sealed object is missing; the applied migration checksum corresponds to neither a proved sealed nor current image; different environments are known to have applied different bytes for one version; current Java cannot be mapped to the declaration; a conversion/backfill is required but absent; an opaque dependency may be affected; or a clean-history/generated-project verification fails. It never invokes Flyway `repair`, mutates `flyway_schema_history`, blesses a checksum, edits a database directly, or deletes data. Diagnostics identify the conflicting authorities and offer only typed next requests; manual database reconciliation remains an operator action outside the automatic plan.

The `Task` acceptance fixture is mandatory: initial V001 creates `tasks`; two fields are later desired; a scaffold sync cannot replace V001; field evolution restores/retains the sealed V001 and emits V002; plain destroy refuses; preserve keeps V001 and allows revive without another create; drop keeps V001 and adds V003; and every intermediate status remains addressable. The same fixture runs against an empty database, a database with the old V001 applied, a dirty edited V001, a missing V001 recoverable from the object store, and a deliberately divergent applied checksum.

---

## Section 8: Generated Java Code Blueprints

These examples show the intended shape, not a new runtime API. All types are ordinary Java/Spring/Testcontainers/ArchUnit code. Package names follow ports and adapters; an actual generator uses the project's configured layout.

### 8.1 Pure domain record with intrinsic validation

```java
package com.acme.billing.domain;

import java.math.BigDecimal;
import java.time.Instant;
import java.util.Objects;
import java.util.UUID;

public record Order(
        UUID id,
        UUID accountId,
        BigDecimal total,
        OrderStatus status,
        Instant createdAt) {

    public Order {
        Objects.requireNonNull(id, "id");
        Objects.requireNonNull(accountId, "accountId");
        Objects.requireNonNull(total, "total");
        Objects.requireNonNull(status, "status");
        Objects.requireNonNull(createdAt, "createdAt");

        if (total.signum() <= 0) {
            throw new IllegalArgumentException("total must be positive");
        }
        if (total.scale() > 4) {
            throw new IllegalArgumentException("total must have at most 4 decimal places");
        }
    }
}
```

```java
package com.acme.billing.domain;

public enum OrderStatus {
    PENDING,
    PAID,
    CANCELLED
}
```

Why this shape:

- domain invariants use only the JDK and execute on every construction path;
- transport concerns such as missing JSON fields are not mixed into the domain;
- a record is immutable and transparent to `javac`, debuggers, serializers, and tests;
- `BigDecimal`, never `double`, backs money-like decimal values. If currency is part of the invariant, generate a separate `Money(BigDecimal amount, Currency currency)` record.

### 8.2 Repository port and type-safe raw `JdbcClient` adapter with no reflective row mapping

```java
package com.acme.billing.application;

import com.acme.billing.domain.Order;
import java.util.List;
import java.util.Optional;
import java.util.UUID;

public interface OrderRepository {
    void insert(Order order);
    Optional<Order> findById(UUID id);
    List<Order> findAll();
    boolean deleteById(UUID id);
}
```

```java
package com.acme.billing.application;

import java.util.UUID;

public final class OrderAlreadyExistsException extends RuntimeException {
    public OrderAlreadyExistsException(UUID id, Throwable cause) {
        super("order already exists: " + id, cause);
    }
}
```

```java
package com.acme.billing.adapter.jdbc;

import com.acme.billing.application.OrderAlreadyExistsException;
import com.acme.billing.application.OrderRepository;
import com.acme.billing.domain.Order;
import com.acme.billing.domain.OrderStatus;
import java.sql.Timestamp;
import java.time.OffsetDateTime;
import java.util.List;
import java.util.Optional;
import java.util.UUID;
import org.springframework.jdbc.core.RowMapper;
import org.springframework.jdbc.core.simple.JdbcClient;
import org.springframework.dao.DuplicateKeyException;
import org.springframework.stereotype.Repository;

@Repository
public class JdbcOrderRepository implements OrderRepository {
    private static final RowMapper<Order> ORDER_MAPPER = (result, row) -> new Order(
            result.getObject("id", UUID.class),
            result.getObject("account_id", UUID.class),
            result.getBigDecimal("total"),
            OrderStatus.valueOf(result.getString("status")),
            result.getObject("created_at", OffsetDateTime.class).toInstant());

    private final JdbcClient jdbc;

    public JdbcOrderRepository(JdbcClient jdbc) {
        this.jdbc = jdbc;
    }

    @Override
    public void insert(Order order) {
        final int changed;
        try {
            changed = jdbc.sql("""
                            INSERT INTO orders (id, account_id, total, status, created_at)
                            VALUES (:id, :accountId, :total, :status, :createdAt)
                            """)
                    .param("id", order.id())
                    .param("accountId", order.accountId())
                    .param("total", order.total())
                    .param("status", order.status().name())
                    .param("createdAt", Timestamp.from(order.createdAt()))
                    .update();
        } catch (DuplicateKeyException duplicate) {
            throw new OrderAlreadyExistsException(order.id(), duplicate);
        }
        if (changed != 1) {
            throw new IllegalStateException("inserting order changed " + changed + " rows");
        }
    }

    @Override
    public Optional<Order> findById(UUID id) {
        return jdbc.sql("""
                        SELECT id, account_id, total, status, created_at
                        FROM orders
                        WHERE id = :id
                        """)
                .param("id", id)
                .query(ORDER_MAPPER)
                .optional();
    }

    @Override
    public List<Order> findAll() {
        return jdbc.sql("""
                        SELECT id, account_id, total, status, created_at
                        FROM orders
                        ORDER BY created_at, id
                        """)
                .query(ORDER_MAPPER)
                .list();
    }

    @Override
    public boolean deleteById(UUID id) {
        return jdbc.sql("DELETE FROM orders WHERE id = :id")
                .param("id", id)
                .update() == 1;
    }
}
```

Spring's `JdbcClient` provides a fluent facade over positional/named JDBC operations ([Spring JDBC reference](https://docs.spring.io/spring-framework/reference/data-access/jdbc/core.html)). The generated mapper remains an explicit lambda. `query(Order.class)` is intentionally avoided because its property/constructor mapping is reflective and hides column-to-component decisions.

### 8.3 In-memory test fake implementing the same port

```java
package com.acme.billing.adapter.memory;

import com.acme.billing.application.OrderAlreadyExistsException;
import com.acme.billing.application.OrderRepository;
import com.acme.billing.domain.Order;
import java.util.Comparator;
import java.util.List;
import java.util.Optional;
import java.util.UUID;
import java.util.concurrent.ConcurrentHashMap;
import java.util.concurrent.ConcurrentMap;

public final class InMemoryOrderRepository implements OrderRepository {
    private static final Comparator<Order> STABLE_ORDER =
            Comparator.comparing(Order::createdAt).thenComparing(Order::id);

    private final ConcurrentMap<UUID, Order> orders = new ConcurrentHashMap<>();

    @Override
    public void insert(Order order) {
        if (orders.putIfAbsent(order.id(), order) != null) {
            throw new OrderAlreadyExistsException(order.id(), null);
        }
    }

    @Override
    public Optional<Order> findById(UUID id) {
        return Optional.ofNullable(orders.get(id));
    }

    @Override
    public List<Order> findAll() {
        return orders.values().stream().sorted(STABLE_ORDER).toList();
    }

    @Override
    public boolean deleteById(UUID id) {
        return orders.remove(id) != null;
    }

    public void clear() {
        orders.clear();
    }
}
```

The fake deliberately has no Spring stereotype. Tests or generated test configuration choose it explicitly. Its repository contract must match insert-only duplicate behavior, stable ordering, and delete semantics, while documentation states that it does not emulate SQL isolation, locks, collation, or database constraint timing.

### 8.4 REST controller, JSpecify nullness default, and RFC 9457 errors

`package-info.java` makes non-null the package default:

```java
@org.jspecify.annotations.NullMarked
package com.acme.billing.web;
```

Spring now documents JSpecify annotations as its null-safety model and recommends build-time checking with tools such as NullAway ([Spring null safety](https://docs.spring.io/spring-framework/reference/core/null-safety.html)).

```java
package com.acme.billing.web;

import jakarta.validation.Valid;
import com.acme.billing.domain.Order;
import com.acme.billing.service.OrderService;
import java.net.URI;
import java.util.UUID;
import org.springframework.http.ResponseEntity;
import org.springframework.web.bind.annotation.GetMapping;
import org.springframework.web.bind.annotation.PathVariable;
import org.springframework.web.bind.annotation.PostMapping;
import org.springframework.web.bind.annotation.RequestBody;
import org.springframework.web.bind.annotation.RequestMapping;
import org.springframework.web.bind.annotation.RestController;

@RestController
@RequestMapping("/orders")
public final class OrderController {
    private final OrderService orders;

    public OrderController(OrderService orders) {
        this.orders = orders;
    }

    @PostMapping
    public ResponseEntity<OrderResponse> create(@Valid @RequestBody OrderRequest request) {
        Order created = orders.create(request.accountId(), request.total());
        return ResponseEntity
                .created(URI.create("/orders/" + created.id()))
                .body(OrderResponse.from(created));
    }

    @GetMapping("/{id}")
    public OrderResponse find(@PathVariable UUID id) {
        return orders.find(id)
                .map(OrderResponse::from)
                .orElseThrow(() -> new OrderNotFoundException(id));
    }
}
```

`OrderRequest.java` keeps transport validation out of the domain type:

```java
package com.acme.billing.web;

import jakarta.validation.constraints.DecimalMin;
import jakarta.validation.constraints.Digits;
import jakarta.validation.constraints.NotNull;
import java.math.BigDecimal;
import java.util.UUID;

public record OrderRequest(
        @NotNull UUID accountId,
        @NotNull @DecimalMin(value = "0.0001") @Digits(integer = 15, fraction = 4)
        BigDecimal total) {}
```

`OrderResponse.java` makes the wire projection explicit:

```java
package com.acme.billing.web;

import com.acme.billing.domain.Order;
import java.math.BigDecimal;
import java.util.UUID;

public record OrderResponse(
        UUID id,
        UUID accountId,
        BigDecimal total,
        String status) {

    static OrderResponse from(Order order) {
        return new OrderResponse(
                order.id(), order.accountId(), order.total(), order.status().name());
    }
}
```

```java
package com.acme.billing.web;

import java.util.UUID;

public final class OrderNotFoundException extends RuntimeException {
    private final UUID orderId;

    public OrderNotFoundException(UUID orderId) {
        super("order " + orderId + " was not found");
        this.orderId = orderId;
    }

    public UUID orderId() {
        return orderId;
    }
}
```

The project-level advice is augmented once; slices do not generate competing `@RestControllerAdvice` classes:

```java
package com.acme.web;

import com.acme.billing.web.OrderNotFoundException;
import java.net.URI;
import org.springframework.http.HttpStatus;
import org.springframework.http.ProblemDetail;
import org.springframework.http.ResponseEntity;
import org.springframework.web.bind.annotation.ExceptionHandler;
import org.springframework.web.bind.annotation.RestControllerAdvice;

@RestControllerAdvice
public final class ApiProblemAdvice {
    @ExceptionHandler(OrderNotFoundException.class)
    ResponseEntity<ProblemDetail> notFound(OrderNotFoundException failure) {
        ProblemDetail problem = ProblemDetail.forStatusAndDetail(
                HttpStatus.NOT_FOUND, "The requested order does not exist.");
        problem.setType(URI.create("https://api.acme.test/problems/order-not-found"));
        problem.setTitle("Order not found");
        problem.setProperty("orderId", failure.orderId());
        return ResponseEntity.status(HttpStatus.NOT_FOUND).body(problem);
    }
}
```

Spring's `ProblemDetail` is the standard RFC 9457 representation and is rendered with `application/problem+json` ([Spring MVC error responses](https://docs.spring.io/spring-framework/reference/web/webmvc/mvc-ann-rest-exceptions.html)). The generated detail is stable and does not expose an exception message or stack trace.

### 8.5 Flyway migration

```sql
-- V014__create_orders.sql
CREATE TABLE orders (
    id          uuid           PRIMARY KEY,
    account_id  uuid           NOT NULL,
    total       numeric(19, 4) NOT NULL,
    status      text           NOT NULL DEFAULT 'PENDING',
    created_at  timestamptz    NOT NULL,

    CONSTRAINT ck_orders_total_positive
        CHECK (total > 0),
    CONSTRAINT ck_orders_status
        CHECK (status IN ('PENDING', 'PAID', 'CANCELLED'))
);

CREATE INDEX ix_orders_status_created_at
    ON orders (status, created_at);
```

The migration spells out constraint and index names so later diagnostics can point to stable objects. The flagship uses text plus a check constraint because the ordinary Java enum does not prove that the project wants a PostgreSQL enum type. A manifest may request a vendor enum explicitly; its additive evolution and Java mapping then require catalog evidence. Generated migrations remain forward-only and contain no rollback body.

### 8.6 Slice integration test with Testcontainers

```java
package com.acme.billing.adapter.jdbc;

import static org.assertj.core.api.Assertions.assertThat;

import com.acme.billing.application.OrderRepository;
import com.acme.billing.domain.Order;
import com.acme.billing.domain.OrderStatus;
import java.math.BigDecimal;
import java.time.Instant;
import java.util.UUID;
import org.junit.jupiter.api.Test;
import org.springframework.beans.factory.annotation.Autowired;
import org.springframework.boot.test.context.TestConfiguration;
import org.springframework.boot.test.context.SpringBootTest;
import org.springframework.boot.testcontainers.service.connection.ServiceConnection;
import org.springframework.context.annotation.Bean;
import org.springframework.context.annotation.Import;
import org.springframework.transaction.annotation.Transactional;
import org.testcontainers.containers.PostgreSQLContainer;

@SpringBootTest
@Import(JdbcOrderRepositoryIT.Containers.class)
@Transactional
final class JdbcOrderRepositoryIT {
    @TestConfiguration(proxyBeanMethods = false)
    static class Containers {
        @Bean
        @ServiceConnection
        PostgreSQLContainer<?> postgres() {
            return new PostgreSQLContainer<>("postgres:17.6-alpine");
        }
    }

    @Autowired
    OrderRepository orders;

    @Test
    void inserts_reads_and_deletes_an_order() {
        Order order = new Order(
                UUID.randomUUID(),
                UUID.randomUUID(),
                new BigDecimal("19.9500"),
                OrderStatus.PENDING,
                Instant.parse("2026-08-25T10:15:30Z"));

        orders.insert(order);

        assertThat(orders.findById(order.id())).contains(order);
        assertThat(orders.findAll()).containsExactly(order);
        assertThat(orders.deleteById(order.id())).isTrue();
        assertThat(orders.findById(order.id())).isEmpty();
    }
}
```

Spring owns the container bean and derives the datasource through `@ServiceConnection`; there is no static JUnit container lifecycle competing with the application context. Flyway migrates that datasource through ordinary Spring Boot configuration. `@Transactional` rolls back each same-thread test; tests that intentionally cross threads or commit use an isolated schema/database instead of claiming transactional isolation they do not have. Cross-slice setup, when a real foreign key is declared, must call an explicit fixture port or public testkit contract rather than importing another slice's implementation package.

### 8.7 ArchUnit ports-and-adapters rules

```java
package com.acme;

import static com.tngtech.archunit.lang.syntax.ArchRuleDefinition.noClasses;
import static com.tngtech.archunit.library.dependencies.SlicesRuleDefinition.slices;

import com.tngtech.archunit.junit.AnalyzeClasses;
import com.tngtech.archunit.junit.ArchTest;
import com.tngtech.archunit.lang.ArchRule;

@AnalyzeClasses(packages = "com.acme")
final class ArchitectureTest {
    @ArchTest
    static final ArchRule DOMAIN_HAS_NO_FRAMEWORK_DEPENDENCIES = noClasses()
            .that().resideInAPackage("..domain..")
            .should().dependOnClassesThat().resideInAnyPackage(
                    "org.springframework..",
                    "jakarta.persistence..",
                    "..adapter..",
                    "..web..");

    @ArchTest
    static final ArchRule APPLICATION_DOES_NOT_DEPEND_ON_ADAPTERS = noClasses()
            .that().resideInAPackage("..application..")
            .should().dependOnClassesThat().resideInAnyPackage(
                    "..adapter..",
                    "..web..");

    @ArchTest
    static final ArchRule TOP_LEVEL_SLICES_ARE_ACYCLIC = slices()
            .matching("com.acme.(*)..")
            .should().beFreeOfCycles();
}
```

This is intentionally executable documentation. The generator derives the single project-level package pattern from the adopted layout rather than hard-coding `com.acme` or adding one rule per adapter. Configured cross-slice/shared-kernel allowances are explicit `(from, to, packages, reason)` entries; unused, blanket, or expired allowances fail the suite.

### 8.8 SQL-first named-query projection

This additional blueprint shows the distinct output enabled by a reader-owned `.sql` contract rather than an entity scaffold. The source file is ordinary SQL with a small directive header:

```sql
-- jails:name FindPayableOrders
-- jails:cardinality many
-- jails:param status text
-- jails:param minimum numeric
-- jails:param limit int4
SELECT id, account_id, total, status, created_at
FROM orders
WHERE status = :status
  AND total >= :minimum
ORDER BY created_at, id
LIMIT :limit
```

Directives describe the Java contract; they do not alter the statement sent to PostgreSQL. Offline checking resolves it against the migration-derived catalog. Live checking, when explicitly requested with a datasource, prepares/describes the same normalized statement and upgrades the evidence record.

```java
package com.acme.billing.application.query;

import com.acme.billing.domain.OrderStatus;
import java.math.BigDecimal;
import java.time.Instant;
import java.util.List;
import java.util.Objects;
import java.util.UUID;

public interface FindPayableOrders {
    List<Row> execute(Params params);

    record Params(OrderStatus status, BigDecimal minimum, int limit) {
        public Params {
            Objects.requireNonNull(status, "status");
            Objects.requireNonNull(minimum, "minimum");
            if (minimum.signum() < 0) {
                throw new IllegalArgumentException("minimum must not be negative");
            }
            if (limit < 1 || limit > 1_000) {
                throw new IllegalArgumentException("limit must be between 1 and 1000");
            }
        }
    }

    record Row(UUID id, UUID accountId, BigDecimal total, OrderStatus status, Instant createdAt) {}
}
```

```java
package com.acme.billing.adapter.jdbc;

import com.acme.billing.application.query.FindPayableOrders;
import com.acme.billing.domain.OrderStatus;
import java.time.OffsetDateTime;
import java.util.List;
import java.util.UUID;
import org.springframework.stereotype.Repository;
import org.springframework.jdbc.core.RowMapper;
import org.springframework.jdbc.core.simple.JdbcClient;

@Repository
public class JdbcFindPayableOrders implements FindPayableOrders {
    private static final String SQL = """
            SELECT id, account_id, total, status, created_at
            FROM orders
            WHERE status = :status
              AND total >= :minimum
            ORDER BY created_at, id
            LIMIT :limit
            """;

    private static final RowMapper<Row> ROW_MAPPER = (result, rowNumber) -> new Row(
            result.getObject("id", UUID.class),
            result.getObject("account_id", UUID.class),
            result.getBigDecimal("total"),
            OrderStatus.valueOf(result.getString("status")),
            result.getObject("created_at", OffsetDateTime.class).toInstant());

    private final JdbcClient jdbc;

    public JdbcFindPayableOrders(JdbcClient jdbc) {
        this.jdbc = jdbc;
    }

    @Override
    public List<Row> execute(Params params) {
        return jdbc.sql(SQL)
                .param("status", params.status().name())
                .param("minimum", params.minimum())
                .param("limit", params.limit())
                .query(ROW_MAPPER)
                .list();
    }
}
```

The `.sql` file remains reader-owned; these Java files are derived and ledger-owned. A golden contract test asserts that the normalized statement in the `.sql` file, excluding directives, is byte-for-byte equal to `SQL`; generation must not cast, reorder, qualify, or otherwise rewrite it invisibly. The generated contract records the query text digest, schema digest, cardinality, parameters, result columns, nullability, and evidence level. Removing the named query proposes retirement of its generated Java and contract without deleting unrelated SQL.

---

## Section 9: Phased Roadmap and Crate-by-Crate Implementation Plan

### 9.1 Sequencing principles

The roadmap extends the current pipeline; it does not create a parallel implementation. The order is deliberately biased toward shortening the inner loop before adding new generation surface:

1. repair known generation and resource-lifecycle defects, seal migration history, align the JDK 26 default, expose exact diffs, and record honest baseline timings;
2. make field evolution/destroy/repair usable, make `jails test` the single dependable test front door, and make `jails run` launch current outputs without paying build-tool startup on every invocation;
3. time-box the SQL dependency decision before committing the workspace to a parser or native library;
4. land one offline named-query path before live catalog observation, then add live evidence only through an explicitly supplied datasource;
5. stabilize diagnostics, contracts, the editor protocol, and the application-tool gateway on the same result envelopes;
6. implement resource rename only after stable entity identity and schema impact facts exist;
7. offer safe file-only undo only after receipts carry enough preimage evidence; migrations remain forward-only.

Each phase ships a coherent vertical increment behind stable protocol versions. A feature is complete only when Maven and Gradle have parity where promised, human and JSON output represent the same result, `--pretend` runs the real preparation path, failures say whether project files changed, and an ordinary build-tool fallback preserves correctness. Long-lived JVMs are optimization engines, never correctness authorities. `jails` MUST NOT implicitly provision application services: live SQL, database consoles, integration tests, and application launch use explicit configuration or an explicit `--services start` choice.

### 9.2 Phases and exit gates

#### Phase 0 — Repair, measure, and make plans inspectable

Deliver:

- remove the scaffold `PROBE` output and add a stray-debug-output gate;
- make plain-project scaffold output compile through a framework-free projection or refuse it before writing, then add a real compiler gate;
- seal migration files at first publication; stop scaffold sync/destroy/undo from replacing or deleting them;
- make table-backed plain destroy refuse with `storage-policy-required` and show the exact preserve/drop forms;
- add the `Task` regression fixture covering current dry-run destroy, scaffold resync, edited V001, and old-applied-schema failure modes;
- align the new-project default to JDK 26 across Maven, Gradle, CLI help, templates, fixtures, toolchain metadata, and compiler gates while preserving adopted Java 21+ releases;
- add `--diff`/`--ast` rendering from the existing prepared base/current/desired values;
- capture parent-directory state and remove phantom `mkdir` operations for paths already observed as directories;
- timing spans for discover, observe, parse, project, prepare, verify, commit, process, and container work;
- benchmark fixtures for small, medium, and multi-module Java projects;
- protocol golden tests for current human/JSON envelopes, prepared bundles, ledgers, and testd IPC;
- a checked-in feature inventory mapping every existing command to its owner crate and side-effect class;
- automated proof for the existing unheld Maven and Gradle example manifests, with an explicit cost/tier policy before adding more;
- explicit compatibility versions for CLI manifests, daemon messages, SQL contracts, and durable state;
- a black-box test matrix that records the current results of `jails test`, `jails test --fast`, `jails testd`, `jails run`, and `jails watch` for Maven and Gradle before behavior changes.

Exit gates:

- benchmark variance is documented and repeatable in CI;
- every mutating route has a dry-run parity test;
- the plain-project flagship scaffold compiles or produces a no-write refusal;
- `--diff` shows create, replace, and three-way cases without losing user changes;
- human/JSON preview and applied receipts report `mkdir` only for an actually absent parent, with no count or wording drift;
- unknown persisted/protocol versions fail closed with an upgrade instruction;
- no performance claim combines JVM/container cold start with the warm operation being measured;
- new Maven and Gradle projects compile with release 26, explicit release 21 remains supported, and adoption never changes the configured release.
- no command can replace/delete a sealed V001; the `Task` fixture is always left with a readable lifecycle status and a typed roll-forward next action.

#### Phase 1 — Safe resource evolution and one fast test/run loop

Deliver:

- `resource status`, typed field add/rename/type/nullability/drop requests, and new forward migrations for every storage delta;
- explicit preserve/drop destroy, preserved-table tombstones, preserved-table revive, and offline roll-forward repair from exact receipt objects;
- compatibility routing from `generate field` to the canonical field-add request and refusal of divergent scaffold resync;
- the Section 6.6 `jails test` grammar and `TestExecutionPlanV1`, with all-tests as the default requested universe;
- one discovery and filtering implementation shared by build and warm engines;
- Maven and Gradle parity for selectors, JSON, fail-fast, slowest tests, watch mode, and compilation policy;
- `--fast` as a compatibility alias for default auto-engine behavior for one release, never as a reduced test suite;
- `jails testd` as an internal implementation/compatibility entry point whose result contract is identical to `jails test`;
- safe warm-engine invalidation, isolation, recycle, epoch, and build fallback behavior;
- a runtime classpath cache and direct Java launcher for `jails run`, with `--launcher build-tool` as the diagnostic fallback;
- exact `--services existing|start|none`, `--compile auto|ide|build|none`, readiness, signal, and child-lifecycle semantics;
- `jails test --watch --output json` as the only test-watch process consumed by `jails.nvim`.

Exit gates:

- the Section 7.19 Task matrix passes for clean, old-applied, edited-migration, missing-migration, preserved, drop-planned, and revived states;
- required-field and destructive changes refuse without their exact data/dependency/confirmation evidence;
- revive reuses preserved storage and migration lineage without another create migration;
- `jails test`, `jails test --engine build`, and `jails test --engine warm` discover the same requested test universe in Maven and Gradle fixtures;
- every warm-engine uncertainty falls back or widens; it never silently drops a test;
- human and JSON reports have identical test identities, outcomes, durations, output ownership, and exit status across engines;
- stale outputs are repaired by the chosen compile owner or refused with one actionable command; `--compile none` never compiles implicitly;
- direct application launch receives the same main class, runtime classpath, profiles, application arguments, and environment as build-tool launch;
- readiness never reports success merely because a JVM process exists;
- Ctrl-C, daemon failure, application failure, and watcher failure leave no child process owned by the invocation;
- reference-fixture warm-loop p50/p95 and cold-start figures are published separately, including invalidation and fallback reasons.

Use the build tools' existing resident processes before inventing another one: Maven acceleration may use mvnd where installed and compatible, while Gradle invocations reuse the Gradle Daemon. Neither is required for correctness, and the application launcher does not keep a second compiler alive when the IDE or build watcher owns compilation.

#### Phase 2 — Dependency decision and one offline query

Deliver:

- an architecture decision record comparing a pure-Rust PostgreSQL parser, a libpg_query binding, and server-only live description against the same corpus;
- recorded effects on clean build time, incremental build time, release artifact size, supported target triples, transitive/native dependencies, licensing, grammar coverage, error spans, and maintenance ownership;
- `ApplicationSpecV1`, `SliceSpec`, `EntitySpec`, stable `EntityId`, `TableBinding`, `ResourceKey::Query`, `SchemaObjectId`, and versioned evidence values;
- the expanded field grammar and manifest decoding through the same typed constructors used by CLI requests;
- a PostgreSQL-first catalog IR derived from ordered migrations, with unsupported statements preserved as opaque blockers;
- the Section 7.4 named-query directive grammar, exact statement preservation, digests, cardinality, bind metadata, result metadata, and deterministic Java projection;
- one flagship `FindPayableOrders.sql` flowing through offline check, contract JSON, generated parameter/result records, `JdbcClient` adapter, fake boundary, and generated-project tests.

The dependency spike passes only if one option can parse the supported corpus with source spans, ships on every target currently supported by `jails`, adds no network/runtime service requirement to offline checking, has an acceptable redistribution license, and keeps both clean-build and release-binary growth within an explicitly approved budget. If no option passes, Phase 2 narrows offline support to directives plus migration facts that can be proven and labels all unresolved types `Unknown`; it does not implement a home-grown SQL grammar or pretend that live-only evidence is offline evidence.

Exit gates:

- equivalent CLI and manifest input serialize to the same canonical request digest;
- generators accept typed values only; downstream phases never reparse compact field strings;
- the reader-owned normalized SQL is byte-for-byte equal to the generated Java `SQL` constant and is never invisibly rewritten;
- every Java type and nullability decision points to a query/catalog evidence record;
- unknown or vendor-specific mappings fail with a targeted override instead of silently becoming `Object`;
- rerunning from the same inputs produces identical generated bytes and contract digest;
- `--frozen --offline` detects query, migration-order, dialect, catalog, and mapping drift without opening a network connection.

Do not implement a general SQL optimizer, database emulator, or ORM. Delegate grammar and live description to mature implementations where possible; keep `jails` responsible for normalization, evidence, ownership, and ordinary Java projection.

#### Phase 3 — Explicit live evidence and bounded schema reconciliation

Deliver:

- `jails sql check --live --datasource <name>` and live query prepare/description against an explicitly resolved datasource;
- PostgreSQL observation for schemas, tables, columns, keys, indexes, checks, enums, domains, views, routines, and policies;
- `jails introspect db`, `pull`, and `schema diff`, all read-only unless a separate mutation command consumes an accepted plan;
- a normalized `SchemaOp` graph with dependency ordering and destructive-risk classification;
- three authorities recorded independently: declared manifest, generated/migration baseline, and observed live catalog;
- explicit ignored-object policy and opaque-object blockers;
- `jails migrate lint` for destructive, data-dependent, constraint-loss, and deployment-incompatibility findings.

Exit gates:

- observation is read-only and credential-redacted in every output mode;
- an import followed by a no-change pull is byte-for-byte idempotent;
- live drift never silently rewrites declared intent or generated Java;
- destructive operations show row/data evidence when the connected database permits it.
- no datasource is guessed from an unrelated running container, and no service starts unless the command carries the explicit `--services start` policy;
- opaque SQL or catalog dependencies block any operation they may invalidate;
- `--frozen --live` detects server-major and observed-catalog drift while storing no credential, host secret, or nondeterministic database identifier.

#### Phase 4 — Diagnostics, editor protocol, and application tools

Deliver:

- the evidence-tagged cause graph and read-only `why`, route, bean, migration, query, architecture, and contract reports;
- generated ArchUnit rules with explicit, expiring allowances and a baseline path for adopted projects;
- OpenAPI/JSON Schema projection plus `jails contract check --against`;
- the exact editor handshake, completion, symbols, diagnostics, plan, and test-report protocols from Sections 7.7 and 7.15;
- asynchronous `jails.nvim` consumption of `jails test --watch --output json`, separate diagnostic namespaces, structured plan review, and no human-output parsing;
- `jails request` as a route-aware, transparent curl adapter;
- `jails db console` with pgcli as the default PostgreSQL client and `--client psql` as an explicit fallback;
- `jails console` as a JDK 26 JShell session that boots the application through the same runtime classpath launcher and exposes the Spring `ApplicationContext` helpers in Section 7.16;
- `jails runner` for noninteractive `.jsh`/stdin snippets and `jails logs` for bounded read-only application log access.

Exit gates:

- reports distinguish static inference, live observation, and hypothesis;
- diagnostics, completion, symbols, and logs do not acquire a write lock, start services, or mutate machine state;
- architecture rules fail on seeded violations and do not depend on generated production runtime code;
- contract checks catch seeded breaking changes and label declared/source/runtime observation scope;
- editor completion is derived from the CLI command graph, and diagnostic/test results preserve typed fixes and reject stale epochs;
- request preview and execution show the exact curl argv with secrets redacted, never invoke a shell, and preserve curl status/output semantics;
- the database console uses the selected real client in a controlling terminal and does not leak a datasource URL or password through argv, JSON, logs, or history;
- console and runner boot the selected application/profile, expose only documented helpers, propagate application/snippet failure, and always close the context;
- Neovim remains an adapter: jdtls owns Java language intelligence, nvim-dap owns debugging, and the CLI owns command semantics and processes.

#### Phase 5 — Coordinated rename

Deliver:

- stable `EntityId` adoption and explicit `TableBinding` for every managed entity;
- `jails rename resource <from> <to> --strategy preserve-table|single-cutover|rolling` using the canonical request in Section 7.17;
- a complete impact report covering generated Java, reader-owned Java references, routes/contracts, query contracts, migration/catalog objects, indexes/constraints, fixtures, and opaque dependencies;
- `preserve-table` as the first and recommended strategy, changing the Java/resource name while retaining the current table binding and wire names unless separately requested;
- `single-cutover` as a generated forward migration plus coordinated Java/query/manifest changes, refused when dependencies are unknown;
- a durable rolling-rename campaign with explicit expand, backfill/verify, switch, and retire phases and an attested retirement precondition;
- exact `--pretend`, `--diff`, plan export/import, stale-plan, crash-recovery, and conflict behavior for every strategy.

Exit gates:

- renaming `Task` to `WorkItem` never accidentally leaves an implicit `task` table association: the receipt records either the preserved binding or the exact forward storage plan;
- hand-written Java and owned named SQL are either transformed through proven spans or listed as manual blockers; plain text replacement is forbidden;
- unknown strings, dynamic SQL, database routines, views, triggers, and external consumers block storage cutover unless explicitly attested outside scope;
- generated index/constraint names are updated consistently, while reader-owned names require an explicit accepted operation;
- every accepted plan is root-, protocol-, input-, catalog-, and preimage-bound and fails stale without writing;
- clean-database and data-bearing PostgreSQL fixtures prove each migration strategy, generated Java builds, and tests pass after removing the `jails` executable.

#### Phase 6 — Safe file undo and hardening

Deliver:

- operation receipts with reason, owner, before/after digest, verification evidence, risk, and external-effect classification;
- `history`, `show`, and safe file-only forward undo, distinct from crash `recover`;
- portable `plan-out`/`plan-in` with root, protocol, input, toolchain, catalog, and preimage preconditions;
- crash-point sweeps, compatibility migrations, documentation, upgrade notes, and telemetry-free local performance summaries.

Exit gates:

- undo refuses or three-way merges user-edited after-images and refuses every receipt containing a migration or unresolved external effect;
- a prepared plan cannot be applied to another root, protocol version, toolchain, catalog, or changed preimage;
- full crash-point sweeps preserve the existing roll-forward durability guarantee;
- generated projects build and test after removing the `jails` executable and clearing optional caches.

Explicitly deferred until measurements justify them: a domain-specific TUI or web studio, application-process supervision, JVM class redefinition, implicit service discovery/provisioning, a general ORM, migration rollback generation, and a generated SQL transaction sandbox. None is a prerequisite for the roadmap above.

### 9.3 Crate-by-crate ownership plan

| Workspace member | Keep as its invariant | Add for this roadmap | Must not absorb |
|---|---|---|---|
| root `jails` CLI | Clap parsing, dispatch, consistent terminal entry point | New command trees, compatibility aliases, common review flags, shell completions | Semantic validation, generation logic, process ownership, a second result model |
| `jails-protocol` | Shared typed vocabulary and durable wire/state contracts; no direct filesystem I/O | `ApplicationSpecV1`, slice/query/schema values, `EntityId`, lifecycle/tombstone/migration-seal/field-evolution values, non-owning `SchemaObjectId`, `ResourceKey::Query`, rename campaigns, test/run/tool requests, evidence, cause nodes, daemon messages, plans and receipts | Project discovery, parsing Java, running tools, rendering templates |
| `jails-spec` | Pure parsing and structural validation | Expanded field grammar, manifest decoder, schema/query/policy IR validation, cross-entity relation checks | Live database observation, filesystem mutation, Java rendering |
| `jails-project` | Observation and surgical project-file models | Cached Maven/Gradle model, runtime classpath and main-class facts, migration/query discovery, catalog snapshots, manifest observations | Starting long-running tools, durable commits, report formatting |
| `jails-java` | Lightweight Java/class-file facts and surgical edits | Persistent source facts, descriptor/annotation bytecode edges, class ownership, reference/rename spans, source-span-preserving edits | Becoming a Java compiler or language server, owning the application JVM |
| `jails-generate` | Pure deterministic projections to desired artifacts | Manifest/slice projection, named-query Java, contracts, fakes, ArchUnit/OpenAPI templates, forward lifecycle/rename migrations, semantic ownership markers | Reading a live database, writing files, executing formatter/build tools |
| `jails-prepare` | Pure in-memory reconciliation and exact prepared bundle | Typed semantic edit reports, risk/evidence aggregation, plan serialization checks, schema-op and rename presentation | Taking locks, running tools, starting services, hidden observations, inverse migration generation |
| `jails-commit` | Lock, WAL, atomic publication, ledger, roll-forward recovery | Rich receipts, plan precondition enforcement, safe file-undo preparation hooks, state migrations | Domain planning, database rollback, shell/container effects |
| `jails-engine` | One orchestration path and global execution policy | Routes for app/SQL/schema/history/resource-lifecycle/rename/tools; phase barriers for observe → plan → verify → commit → reconcile | Feature-specific ad hoc writes or alternate pretend behavior |
| `jails-drive` | Explicit external process execution | Unified test engine/testd, build-tool adapters, runtime launcher, watcher, tool adapters, affected graph store, explicit datasource access, live SQL description | Static reporting, canonical domain rules, implicit service lifecycle, owning generated source |
| `jails-report` | Read-only human/JSON diagnosis and explanation | Test/run/tool/resource-status reports, cause graph, migration lint, evidence rendering, query/migration/bean explanations, route/OpenAPI/contract conflicts, benchmark summaries | Mutation implementation, shell execution, service startup |
| `jails-state` | Fail-closed read-only interpretation of `.jails/` | Versioned readers for SQL contracts, graph/classpath caches, test daemon metadata, lifecycle tombstones/seals, rename campaigns, transactions, history, and compatibility diagnostics | Writing/migrating state in place, application project observation |
| `jails-testkit` | Shared test-only concurrency and fixture support | Protocol goldens, crash sweeps, fake catalog facts, daemon epoch simulator, Maven/Gradle/generated-project assertions | Production utilities or public runtime APIs |
| `jails-support` | Domain-neutral OS, process, lock, codec and error primitives | Framed local IPC, bounded watcher abstraction, clock/process fakes, secure redaction helpers if broadly reusable | Java/SQL/project concepts or user-facing feature policy |
| `jails.nvim` | Thin asynchronous Neovim adapter over public CLI schemas | Handshake/result/event decoders, completion/pickers, separate diagnostics, plan confirmation, receipt-driven file opening, per-root test-watch status | Command grammar, project semantics, human-output parsing, application supervision, Java compilation, debugger ownership |
| optional future `jails-catalog` | Extract only after catalog parsing/observation is independently reusable | Dialect adapters, catalog normalization, query description contracts | Generation, mutation, CLI rendering, datasource lifecycle |

The dependency direction should stay acyclic and visible:

```text
protocol/spec
     ↓
project/java → generate
     ↓           ↓
        prepare
           ↓
state → commit
           ↑
         engine
      ↙          ↘
 report          drive
      ↖          ↗
          CLI
```

This is a responsibility diagram, not permission to introduce all arrows. `report` remains structurally unable to mutate or run tools; `generate` remains unable to observe live state; `prepare` remains unable to write. Cross-cutting cached fact schemas belong in `protocol`, while their on-disk readers belong in `state` and their producers remain in `project`, `java`, or `drive`.

### 9.4 First releasable slice

The first releasable slice targets the daily pain before introducing the SQL compiler:

1. fix the three immediate generation/report defects and hold them with real JDK 26 compiler and operation-report gates;
2. seal migration history, make plain table-backed destroy refuse, and make preserve/drop behavior pass the `Task` regression fixture;
3. add `resource status`, forward field evolution, preserved-table revive, and roll-forward repair so the observed edited-V001 state has a safe exit;
4. show prepared create/replace/three-way diffs with provenance and evidence;
5. make `jails test` the sole public test front door for Maven and Gradle, with build-engine parity and a stable `TestReportV1`;
6. make `jails test --fast` normalize to the safe auto engine and make `jails testd` an explicitly deprecated compatibility alias;
7. add the warm engine behind auto selection, with compile-owner, invalidation, epoch, isolation, recycle, and fallback gates;
8. cache the Maven/Gradle runtime classpath and make `jails run` launch current outputs directly under explicit service and readiness policies;
9. wire `jails.nvim` test watching to `jails test --watch --output json` without changing jdtls or DAP ownership;
10. publish cold and warm p50/p95 measurements for test selection, first result, full completion, process start, Spring start, and readiness.

That release makes the common loop smaller while preserving the complete-test default and ordinary build fallback. The next releasable slice is the dependency-gated offline named query from Phase 2. The application tools, coordinated rename, and live schema work then build on stable launch, result, identity, and evidence contracts instead of each inventing a process or database path.

### 9.5 Verification strategy and release scorecard

| Property | Test method | Release gate |
|---|---|---|
| Determinism | Generate twice across randomized input order, locale and temp roots | Identical desired bytes, operation ordering and semantic digest |
| Dry-run parity | Compare pretend bundle with the bundle passed to commit | Same operation/evidence digests; only commit and post-commit effects absent |
| Crash safety | Inject a stop at every durable commit transition | Next invocation reaches exactly the intended after-state or reports unreadable state |
| Incremental safety | Mutation corpus: source, resource, migration, deletion, rename, processor, classpath | Selected tests are a superset of the oracle's failures; unknowns widen |
| SQL correctness | Dialect fixtures plus live PostgreSQL version matrix | Generated bind/read types match described metadata; frozen drift fails |
| Merge safety | Property tests over base/user/generated triples and ownership regions | User-owned text is preserved or a conflict is returned; never silently dropped |
| Runtime independence | Build/test generated fixtures after removing `jails` and clearing `.jails/` | Standard Java build remains green |
| Architectural purity | Workspace dependency checks plus generated ArchUnit suite | No runtime dependency, no JPA generation, domain remains framework-free |
| Latency | Hyperfine/criterion-style warm and cold suites with cache reasons | Section 1 p50/p95 budgets, or a published miss reason and regression waiver |
| Output contract | Golden human/JSON results for success, no-op, conflict and failure | One semantic result; stable machine-readable codes and schema version |

Release reporting should publish distributions and fixture sizes, not a single “1000x” number. The trustworthy claim is concrete: which former workflow collapsed, its before/after p50 and p95, what was warm, and which safety checks still ran.

### 9.6 Principal risks and controls

| Risk | Control |
|---|---|
| A broad manifest becomes a hidden framework | Keep it optional and ejectable; generated standard Java/SQL remains authoritative to normal tools |
| Static SQL support grows into an incomplete database | PostgreSQL-first scope, evidence levels, live prepare/describe, explicit unsupported constructs |
| Affected testing misses reflective/resource coupling | Rule-based external edges, compile/source completeness checks, and fail-safe widening |
| Daemon caches return plausible stale answers | Content/build/protocol digests, epochs, atomic cache publication, visible invalidation reasons |
| Schema pull destroys hand modeling | Three independent authorities, preview by default, rename conflicts, granular ownership |
| Resource destroy or scaffold sync erases schema history | Migration seals are append-only; destroy requires storage policy, and every schema change is a later forward migration |
| Live tooling takes over explicit configuration | Explicit datasource/service policy always wins; read-only commands never provision, reset, or stop infrastructure |
| “Undo” implies database rollback or history erasure | File-only forward correction; recovery stays roll-forward; every receipt containing migration/external ambiguity refuses undo |
| Too many features erode crate boundaries | Dependency-denial tests and the ownership table above reviewed with every new cross-crate edge |

---

## Source and Evidence Notes

### Current `jails` implementation inspected

The recommendations were anchored in the current source responsibilities, particularly:

- `crates/jails-spec/src/spec/field.rs` for field parsing and derived projections;
- `crates/jails-generate/src/generate/scaffold.rs` and `crates/jails-generate/src/sql.rs` for vertical slices and the shared field-to-DDL/binder/row-mapper path;
- `crates/jails-prepare/src/pipeline.rs`, `merge.rs`, and `reconcile.rs` for pure preparation and three-way reconciliation;
- `crates/jails-prepare/src/report.rs` and `pipeline.rs::parents` for the current unconditional parent-directory/CreateDirectory report path;
- `crates/jails-commit/src/execute.rs`, `journal.rs`, and `recover.rs` for durable commit and roll-forward recovery;
- `crates/jails-engine/src/route/artifact.rs` and `route/commit.rs` for orchestration;
- `crates/jails-java/src/classfile.rs` for constant-pool dependency extraction;
- `crates/jails-drive/src/testd.rs` and `affected.rs` for the resident test runner and selection;
- `crates/jails-report/src/doctor.rs` and `why.rs` for read-only diagnostics;
- `src/cli.rs` for the existing command surface;
- `jails.nvim/lua/jails/init.lua`, `plugin/jails.lua`, `compiler/jails.vim`, and `after/ftplugin/java.lua` for current Neovim dispatch, completion, human-output parsing, compiler integration, and Java buffer setup.

The `task-service` acceptance checkout supplied the concrete lifecycle failure: its Task domain/JDBC/DTO/test projections, `V001__create_tasks.sql`, ledger/receipt objects, and current Git diff were inspected directly. Read-only CLI previews proved that destroy schedules deletion of V001 without a database effect and scaffold resync schedules replacement of V001. Those observations define the mandatory Task matrix in Section 7.19; the acceptance checkout is evidence, not a runtime or build dependency.

The acceptance configuration was also inspected in the sibling `../my-dotfiles` checkout at `home/.config/nvim/init.lua`, `home/.config/nvim/lua/jails_pickers.lua`, `home/.config/nvim/ftplugin/java.lua`, and `home/.config/nvim/lua/javac_lint.lua`. It proves the required jdtls, DAP, picker, save-time lint, source-root, and JDK 26 coexistence scenarios. It is a downstream consumer and SHALL NOT become a build, runtime, or configuration dependency of `jails` or `jails.nvim`.

The main repository graph was refreshed in full at generation `2026-08-25T16:52:03Z`, and task-directed coverage checks reported no recorded gaps for the relied-on implementation paths. The acceptance checkout's locally edited Java paths were reported metadata-changed, so their current source/diff and non-code migration/state files were read directly. No negative or exhaustive claim relies on graph absence alone.

### Representative upstream implementation paths inspected under `deps/`

- `deps/sqlc/internal/compiler/compile.go`, `internal/analyzer/analyzer.go`, and `internal/metadata/meta.go`: catalog/query separation, named cardinality, and content-addressed query analysis;
- `deps/quarkus/extensions/datasource/deployment/src/main/java/io/quarkus/datasource/deployment/devservices/DevServicesDatasourceProcessor.java`: missing configuration as the trigger for a managed development service;
- `deps/django/django/db/migrations/autodetector.py` and `deps/alembic/alembic/autogenerate/compare/`: whole-state migration detection and per-concern schema comparators;
- `deps/atlas/sql/sqlcheck/{destructive,datadepend,condrop,incompatible}/`: risk-specific migration analyzers;
- `deps/postgrest/src/library/PostgREST/SchemaCache.hs`: database catalog facts and cache invalidation as an API substrate;
- `deps/phoenix/lib/mix/tasks/phx.gen.context.ex`: bounded-context scaffolding and augment-existing behavior;
- `deps/rails/railties/lib/rails/generators/rails/scaffold/` and `deps/laravel/framework/src/Illuminate/Console/GeneratorCommand.php`: composable generator hooks, templates, and command ergonomics;
- `deps/ecto/lib/ecto/changeset.ex` and `deps/ecto_sql/lib/ecto/adapters/sql/sandbox.ex`: validation boundaries and transactional test checkout/ownership;
- `deps/loco/loco-gen/src/infer.rs`: migration intent inferred from command names;
- `deps/archunit/archunit/src/main/java/com/tngtech/archunit/library/Architectures.java`: executable layered/onion architecture rules;
- `deps/spring-framework/spring-tx/src/main/java/org/springframework/transaction/support/TransactionSynchronizationManager.java` and Spring Boot failure analyzers: thread-bound transactions and causal startup diagnostics;
- `deps/jhipster/generator-jhipster/lib/jdl/`: multi-entity declarative application modeling.

### Primary upstream references

- Java/JVM and build loops: [JDK 26 documentation](https://docs.oracle.com/en/java/javase/26/); [Java SE and JDK 26 specifications](https://docs.oracle.com/en/java/javase/26/docs/specs/index.html); [JDK 26 API](https://docs.oracle.com/en/java/javase/26/docs/api/); [JDK 26 JShell](https://docs.oracle.com/en/java/javase/26/jshell/); [pattern matching for `switch`](https://openjdk.org/jeps/441); [virtual threads](https://openjdk.org/jeps/444); [Maven Daemon](https://maven.apache.org/tools/mvnd.html); [Gradle Daemon](https://docs.gradle.org/current/userguide/gradle_daemon.html).
- Spring: [`JdbcClient`](https://docs.spring.io/spring-framework/reference/data-access/jdbc/core.html); [RFC 9457 `ProblemDetail`](https://docs.spring.io/spring-framework/reference/web/webmvc/mvc-ann-rest-exceptions.html); [JSpecify null safety](https://docs.spring.io/spring-framework/reference/core/null-safety.html); [DevTools](https://docs.spring.io/spring-boot/reference/using/devtools.html); [Testcontainers service connections](https://docs.spring.io/spring-boot/reference/testing/testcontainers.html); [failure analyzers](https://docs.spring.io/spring-boot/reference/features/spring-application.html#features.spring-application.application-startup-failure).
- Testing and architecture: [Testcontainers reusable containers](https://java.testcontainers.org/features/reuse/); [ArchUnit user guide](https://www.archunit.org/userguide/html/000_Index.html); [Spring test-managed transactions](https://docs.spring.io/spring-framework/reference/testing/testcontext-framework/tx.html).
- SQL and schema: [sqlc documentation](https://docs.sqlc.dev/en/stable/); [SQLx checked query macro and offline mode](https://docs.rs/sqlx/latest/sqlx/macro.query.html); [jOOQ code generation](https://www.jooq.org/doc/latest/manual/code-generation/); [Prisma introspection](https://www.prisma.io/docs/orm/prisma-schema/introspection); [Alembic autogenerate](https://alembic.sqlalchemy.org/en/latest/autogenerate.html); [Atlas SQL checks](https://github.com/ariga/atlas/tree/master/sql/sqlcheck); [PostgREST schema cache](https://docs.postgrest.org/en/v16/references/schema_cache.html).
- Framework generation, application tools, and testing patterns: [Rails command line/generators, console, runner, and dbconsole](https://guides.rubyonrails.org/command_line.html); [Rails migrations](https://guides.rubyonrails.org/active_record_migrations.html); [curl manual](https://curl.se/docs/manpage.html); [pgcli](https://www.pgcli.com/); [Django migrations](https://docs.djangoproject.com/en/6.0/topics/migrations/); [Phoenix context generator](https://hexdocs.pm/phoenix/Mix.Tasks.Phx.Gen.Context.html); [Ecto changesets](https://hexdocs.pm/ecto/Ecto.Changeset.html); [Ecto SQL Sandbox](https://hexdocs.pm/ecto_sql/Ecto.Adapters.SQL.Sandbox.html); [FastAPI features](https://fastapi.tiangolo.com/features/); [AdonisJS scaffolding](https://docs.adonisjs.com/guides/concepts/scaffolding); [Loco generators](https://loco.rs/docs/reference/generators/).
- Declarative and ambient-service patterns: [Quarkus Dev Services](https://quarkus.io/guides/dev-services); [Wasp application specification](https://wasp.sh/docs/general/spec); [JHipster JDL](https://www.jhipster.tech/jdl/intro/).

Where the translation matrix lists an ecosystem for breadth rather than a source used for a material behavioral claim, the linked official repository is the catalogue pointer; implementation recommendations remain constrained by the primary evidence above and by `jails`' no-runtime-dependency rule.
