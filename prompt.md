# Research Prompt: 1000x Developer Experience (DX) for `jails`

You are an expert systems architect and developer experience (DX) researcher. Your mission is to research, analyze, and synthesize state-of-the-art DX paradigms across modern programming languages and frameworks, and translate those insights into groundbreaking, actionable ideas for **`jails`**.

---

## 1. Project Context: What is `jails`?

**`jails`** is an ultra-fast, opinionated CLI tool written in Rust that generates clean, production-ready **Java code** (targeting Spring Boot and plain Maven/Gradle projects), inspired by the ergonomics of modern web frameworks.

### Fundamental Constraints & Principles of `jails`
- **A Code Generator, NOT a Runtime Framework or Library**:
  `jails` does not ship a `jails.jar` runtime dependency. It does not introduce magical reflection, proxy bytecode manipulation at runtime, or opaque framework abstractions. It generates pure, transparent, standard Java that compiles with vanilla `javac`/Maven/Gradle.
- **Interface: Strictly CLI & Future TUI**:
  `jails` is exclusively a terminal tool (CLI commands, flags, subcommands, and potential future interactive TUI/wizards). There are **no web GUIs or desktop apps**.
- **Modern Java Idioms**:
  Java has unique strengths and constraints. `jails` leverages modern Java (Java 21+ records, sealed interfaces, pattern matching, virtual threads, `JdbcClient`, Testcontainers, JSpecify annotations, ArchUnit). Generated architectures follow Hexagonal / Explicit Ports & Adapters (immutable records as domain models, raw SQL/JDBC repositories, in-memory test fakes, zero heavy Hibernate/JPA).
- **Transactional & Safe Codebase Mutations**:
  Every file generation, edit, or configuration patch (`pom.xml`, `application.properties`, migrations) is executed as an atomic, journaled transaction with full dry-run support (`--pretend`), AST diffs, and crash-recovery roll-forward durability.

---

## 2. Core Objective & The Three DX Pillars

Your goal is to explore how other languages and ecosystems have revolutionized DX, and devise concrete proposals to make developing Java applications with `jails` **1000x faster, clearer, and more joyful**.

All research must directly serve these **Three Core DX Pillars**:

```
+-------------------------------------------------------------------------------+
|                               3 PILLARS OF DX                                 |
+------------------------------------+------------------------------------------+
|  1. Sub-Second Feedback Cycles     |  * Low e2e latency doing ANYTHING        |
|     (Zero Friction & Waiting)      |  * Instant scaffolding & dry runs        |
|                                    |  * 20-50ms test execution (testd)        |
|                                    |  * Instant schema & bytecode reflection  |
+------------------------------------+------------------------------------------+
|  2. Correctness & Explicit Actions |  * Developer is never puzzled or misled  |
|     (High Trust & Zero Magic)      |  * Atomic mutations with visual diffs    |
|                                    |  * Rich diagnostics (why, doctor)       |
|                                    |  * Deterministic, transparent code       |
+------------------------------------+------------------------------------------+
|  3. High-Velocity Authoring        |  * Write massive production code fast    |
|     (Dense Ergonomics & Scaffolds) |  * Expressive CLI DSL & vertical slices  |
|                                    |  * Bidirectional schema/code workflows   |
|                                    |  * Rich fakes, seeds, migrations, tests  |
+------------------------------------+------------------------------------------+
```

### Pillar 1: Faster Feedback Cycles (Lower e2e Latency)
- How do we eliminate every delay when creating, running, modifying, testing, and debugging Java services?
- Techniques to explore: resident background daemons, bytecode constant-pool analysis, incremental compilation triggers, hot-reloading/live-replacement hooks, DevServices/instant ephemeral containers, fast SQL parsing.

### Pillar 2: Correctness & Explicit Actions (Zero Puzzlement)
- How do we make sure the developer always understands what happened, why it happened, and what to do next?
- Techniques to explore: visual terminal diffs before writing, bidirectional consistency checks, AST-aware merging that preserves user code, precise failure diagnosis (e.g. diagnosing why a Spring bean failed to inject or why a DB migration failed), reversible operations, compile-time verifiable SQL queries.

### Pillar 3: High-Velocity Authoring (Tons of Quality Java in Seconds)
- How can a developer type one short command and get a complete, robust, enterprise-grade vertical slice?
- Techniques to explore: concise field/relation DSLs, auto-generated unit & integration test suites with in-memory fakes, schema-first (derive Java from existing SQL/Postgres schema) and code-first bidirectional flows, seed data generation, policy/auth matrices, automated mock/contract generation.

---

## 3. Research Landscape & Project Catalogue

Explore the local repositories cloned under `@deps` (`/home/laith/code/jails/deps/`), and clone additional upstream repositories into `deps/` as needed to study their implementations, CLI command structures, AST parsers, code generation engines, and DX workflows.

### Ecosystems & Projects to Investigate

#### 1. Ruby
- **Rails** (`rails/rails`): Convention over configuration, `rails g scaffold`, timestamped reversible migrations, `db/seeds`, console REPL, Solid Queue/Cache (DB-backed infrastructure).
- **Hanami** (`hanami/hanami`): Explicit slices/bounded contexts, dependency injection, ROM repository pattern (clean architecture alternative to ActiveRecord).
- **Sinatra** (`sinatra/sinatra`) & **Roda** (`jeremyevans/roda`): Routing trees and branch-level middleware hoisting.
- **Kamal** (`basecamp/kamal`): Zero-downtime bare-metal deployment primitives.
- **Hotwire / Turbo** (`hotwired/turbo`): HTML-over-the-wire partial page updates driven by the server.

#### 2. PHP
- **Laravel** (`laravel/framework`): Artisan CLI generator ergonomics, model factories, seeders, Tinker REPL, first-party queue/mail/scheduler scaffolds.
- **Filament** (`filamentphp/filament`): Code-defined resource management, admin forms, and tables derived from schemas.
- **Livewire** (`livewire/livewire`): Server-rendered stateful components with automatic DOM diffing.

#### 3. Python
- **Django** (`django/django`): Declarative models, automatic migration diff generator (`makemigrations` diffing model state against database schema), reusable apps.
- **FastAPI** (`fastapi/fastapi`): Type hints as single source of truth for serialization, validation, and OpenAPI 3.1 docs.
- **Reflex** (`reflex-dev/reflex`): Frontend in pure Python compiled to React; state syncing.
- **Alembic** (`sqlalchemy/alembic`): AST model diffing for migration generation with merge support.

#### 4. Elixir
- **Phoenix** (`phoenixframework/phoenix`): `mix phx.gen.context` & `mix phx.gen.schema` (scaffolding explicit domain boundaries/contexts), Channels.
- **Phoenix LiveView** (`phoenixframework/phoenix_live_view`): Server-held UI state with minimal diffs pushed over WebSockets.
- **Ecto** (`elixir-ecto/ecto`): Changesets (separating casting/validation from persistence), SQL sandbox enabling concurrent transactional testing without dirty state.

#### 5. JavaScript / TypeScript
- **AdonisJS** (`adonisjs/core`): Ace CLI generators, lucid migrations, typed validators.
- **RedwoodJS / CedarJS** (`redwoodjs/graphql` / `cedarjs/cedar`): Declarative full-stack slice generation and Cells (declarative query/state boundaries).
- **Wasp** (`wasp-lang/wasp`): Declarative `.wasp` config language describing entities, routes, auth, and jobs.
- **Prisma** (`prisma/prisma`): Schema DSL as single source of truth, typed client generation, schema migration engine.
- **Next.js** (`vercel/next.js`): File-system routing, Server Components and Server Actions.
- **HTMX** (`bigskysoftware/htmx`): Hypermedia controls and declarative DOM swapping via attributes.
- **tRPC** (`trpc/trpc`): End-to-end type inference across boundaries without runtime overhead.

#### 6. Go
- **sqlc** (`sqlc-dev/sqlc`): SQL as the single source of truth; parses raw SQL queries and schemas to emit compile-safe, zero-reflection Go structs and queries. *(Critical pattern for jails raw-JDBC philosophy)*.
- **Encore** (`encoredev/encore`): Infrastructure-from-code (static analysis of code declarations auto-provisions databases, queues, crons), built-in local tracing.
- **Ent** (`ent/ent`): Schema-as-code generating graph-traversal queries and migrations.
- **Bun** (`uptrace/bun`): SQL-first query builder with struct mapping.
- **templ** (`a-h/templ`): Type-checked HTML components compiled to Go functions.
- **Huma** (`danielgtaylor/huma`) & **Fuego** (`go-fuego/fuego`): Generics and type signatures driving validation, serialization, and OpenAPI generation.
- **Goa** (`goadesign/goa`): Design-first API DSL generating servers, clients, and docs.
- **PocketBase** (`pocketbase/pocketbase`): Entire backend (SQLite, auth, realtime subscriptions, file storage, REST API) as single binary.
- **Goravel** (`goravel/goravel`): Laravel's structure, facades, and Artisan CLI in Go.
- **Air** (`air-verse/air`): Custom trigger live-reload for Go.
- **goose** (`pressly/goose`): Embeddable SQL and Go function migrations.
- **Testcontainers-Go** (`testcontainers/testcontainers-go`): Ephemeral infrastructure containers for zero-mock testing.

#### 7. Rust
- **Loco** (`loco-rs/loco`): Rails-style full-stack framework in Rust; smart migration generators that infer schema changes from command names (`add_columns_to_users`).
- **Axum** (`tokio-rs/axum`): Type-safe extractors and Tower middleware ecosystem.
- **SQLx** (`launchbadge/sqlx`): Compile-time verification of raw SQL against live/cached database schemas without ORM.
- **SeaORM** (`SeaQL/sea-orm`) & **Diesel** (`diesel-rs/diesel`): Async entity generation from live schemas, compile-time query safety.
- **utoipa** (`juhaku/utoipa`): Compile-time OpenAPI generation via AST inspection / derive macros.
- **Leptos** (`leptos-rs/leptos`) & **Dioxus** (`DioxusLabs/dioxus`): Fine-grained reactivity, hot-reloading RSX.
- **Shuttle** (`shuttle-hq/shuttle`): Infrastructure requested via function annotations.

#### 8. Zig
- **Jetzig** (`jetzig-framework/jetzig`): Build-time route/template compilation, minimal runtime overhead, integrated KV/query primitives.
- **http.zig** (`karlseguin/http.zig`): High-performance pure-Zig HTTP server with per-request arena allocators.
- **Zap** (`zigzap/zap`): Facil.io microframework in Zig.
- **Ziex** (`ziex-dev/ziex`): JSX-style HTML syntax embedded directly in Zig, compiled at build time.
- **zzz** (`tardy-org/zzz`): io_uring-based async server.

#### 9. JVM Ecosystem
- **Spring Boot** (`spring-projects/spring-boot`) & **Spring Data REST** (`spring-projects/spring-data-rest`): Auto-configuration from classpath detection, starter dependencies, repository-to-REST exposure.
- **Quarkus** (`quarkusio/quarkus`): DevServices (zero-config auto-starting of Postgres/Kafka containers based on missing config), Dev UI, build-time metadata processing.
- **JHipster** (`jhipster/generator-jhipster`): JDL (JHipster Domain Language) for entity/relationship modeling and full-stack scaffolding.
- **jOOQ** (`jOOQ/jOOQ`): Typesafe SQL DSL generated directly from database schemas.
- **Micronaut** (`micronaut-projects/micronaut-core`): Compile-time dependency injection and AOP via annotation processors.
- **Ktor** (`ktorio/ktor`): Coroutine-native server with DSL routing tree.
- **ArchUnit** (`TNG/ArchUnit`): Architectural rules (layering, package isolation) asserted as automated unit tests.

#### 10. Database & Backend-as-a-Service (BaaS)
- **Supabase** (`supabase/supabase`) & **PostgREST** (`PostgREST/postgrest`): Reverse-engineering complete REST APIs and policies directly from database catalog introspection.
- **Hasura** (`hasura/graphql-engine`) & **Directus** (`directus/directus`): Real-time schema introspection, instant API reflection.
- **Appwrite** (`appwrite/appwrite`) & **Nhost** (`nhost/nhost`): Complete self-hosted BaaS bundles.

---

## 4. Research Methodology & Execution Steps

When executing this research:

1. **Inspect Codebases in `@deps`**:
   - Check existing checkouts in `/home/laith/code/jails/deps/`.
   - Read source code, CLI parsers, code generation templates, schema diffing engines, and test harnesses.
   - If a listed repository is missing from `deps/`, clone it into `deps/`
2. **Extract & Abstract the Core Mechanisms**:
   - For each notable DX feature, identify *why* it feels so fast, intuitive, or productive.
   - Separate the core pattern from framework-specific or language-specific runtime baggage.
3. **Translate to Modern Java & `jails` Architecture**:
   - Convert the idea into modern Java (Java 21+ records, `JdbcClient`, Testcontainers, sealed types, virtual threads).
   - Ensure the proposal respects `jails`'s no-runtime-dependency and pure CLI/TUI design.
   - Identify which crate in `jails` (`jails-spec`, `jails-generate`, `jails-drive`, `jails-prepare`, `jails-report`, `jails-commit`, `jails-engine`) would implement the capability.
4. **Stress-Test Against the 3 DX Pillars**:
   - Does this reduce feedback latency?
   - Does this eliminate developer confusion and increase explicitness?
   - Does this multiply authoring velocity?

---

## 5. Required Output & Deliverable Structure

Synthesize your research into a structured, highly actionable engineering report with the following sections:

### Section 1: Executive DX Vision & Top 10 Breakthrough Concepts
A high-level blueprint of how `jails` can achieve a 1000x DX leap, followed by the top 10 highest-impact ideas adapted from the cross-ecosystem survey.

### Section 2: Deep Dive into Pillar 1 — Sub-Second Feedback Loops
- Specific mechanisms to make every `jails` command and Java iteration loop near-instantaneous.
- Daemon optimizations (`testd`), bytecode constant-pool dependency tracking, incremental AST updates.
- DevServices & ephemeral test environment orchestration (Postgres, Kafka, Redis) without manual setup.
- Instant schema diffing and compile-time SQL verification against cached/live DB catalogs.

### Section 3: Deep Dive into Pillar 2 — Correctness, Trust & Zero Puzzlement
- How `jails` ensures the developer is never confused about what code was created or why a failure happened.
- Visual interactive CLI dry-runs (`--pretend`) with colorized unified diffs and AST merge previews.
- Deep diagnostic commands (e.g. `jails why` for Spring context failures / circular dependencies / missing beans, `jails doctor` for environment sanity, `jails routes` / `jails beans` / `jails explain`).
- Architectural fitness gates generated as unit tests (ArchUnit) to prevent accidental boundary violations.

### Section 4: Deep Dive into Pillar 3 — Ultra-High Velocity Authoring
- Next-generation CLI DSL syntax for generating rich vertical slices (e.g. `jails generate scaffold Order user:ref total:money status:enum{PENDING,PAID,CANCELLED} --with-events --with-audit`).
- Reverse-engineering & introspection: Generating complete Java domain slices directly from live DB schemas (`jails pull` / `jails introspect`).
- SQL-first workflow (inspired by `sqlc`): Writing raw `.sql` files and having `jails` generate type-safe Java Records, `JdbcClient` queries, and repository fakes.
- Rich test fakes, seed data generators (`db/seeds.json` or Java seeders), model factories, and contract test harnesses.
- Interactive TUI wizard modes for complex multi-entity domain modeling in the terminal.

### Section 5: Cross-Ecosystem Pattern Translation Matrix
A comprehensive table detailing:
| Source Ecosystem / Tool | Core DX Innovation | How `jails` Adapts It for Java | Affected `jails` Crate | Expected DX Impact (Latency / Correctness / Authoring) |
| :--- | :--- | :--- | :--- | :--- |

### Section 6: Concrete CLI Command Specifications
Detailed specification of proposed new or enhanced `jails` commands, including:
- Command syntax, flags, and arguments.
- Example execution flow and sample terminal output (including dry-run visual diffs).
- Exact file layout and Java code generated (showing records, repositories, controllers, migrations, tests).

### Section 7: Generated Java Code Blueprints
Production-quality code examples illustrating the target output generated by `jails`:
1. Pure Domain Record with validation rules.
2. Type-safe Raw JDBC Repository (`JdbcClient`) with zero reflection.
3. In-Memory Test Fake implementing the repository interface.
4. Clean REST Controller with JSpecify nullability and problem-detail error handling.
5. Flyway/Liquibase migration script.
6. Slice Integration Test with Testcontainers.
7. ArchUnit architectural rule asserting ports-and-adapters isolation.

### Section 8: Implementation Roadmap & Crate-by-Crate Architecture Plan
A prioritized, step-by-step implementation plan showing how to build these capabilities into the `jails` Rust workspace (`crates/jails-*`).
