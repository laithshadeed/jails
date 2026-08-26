---
name: jails
description: "Scaffolding and developing Spring Boot and plain Java applications using the jails CLI. Triggers on: jails, jails new, jails generate, jails g, jails add, spring boot scaffolding, ports and adapters, raw jdbc java, testcontainers spring, jails doctor, jails routes, jails test, hex architecture java, flyway migrations with jails."
---

# Jails — Spring Boot & Java Scaffolding CLI

`jails` is an opinionated developer CLI and scaffolding tool for Java and Spring Boot (Java 21/26, Maven/Gradle) inspired by Rails. It enforces **Hexagonal Architecture** (pure Java records, explicit ports, raw-JDBC with `JdbcClient`, zero ORM), transactional codebase mutations with Write-Ahead Logging (`.jails/`), and sub-second feedback loops.

---

## 1. Quick Command Matrix

| Task | Command | Description |
| :--- | :--- | :--- |
| **New Spring Project** | `jails new <name> [--deps web,jdbc] [--java 26]` | Creates Spring Boot project with git, devtools, Maven wrapper |
| **New Gradle Project** | `jails new <name> --gradle [--boot 2.7.18]` | Creates Groovy Gradle Spring Boot project without initializr |
| **New Plain CLI** | `jails new-cli <name> [--release 26]` | Plain Java Maven CLI with command dispatcher |
| **Scaffold Resource** | `jails g scaffold <Name> [field:type...]` | Record, port, JDBC + in-memory adapters, controller, DTOs, tests, DDL, .http |
| **Record** | `jails g record <Name> [field:type...]` | Pure immutable record with compact constructor validation & test |
| **Repository** | `jails g repo <Name> [field:type...]` | Repository port, derived `JdbcClient` adapter, and integration test |
| **Use Case (Create)** | `jails g usecase <Name> [field:type...] --on <Resource> [--yields <Event>]` | Transactional command, port, HTTP adapter, optional outbox relay |
| **Query (Read)** | `jails g query <Name> [field:type...] --on <Resource>` | Filtered query port, derived SQL adapter, HTTP endpoint |
| **State Transition** | `jails g transition <Name> [field:type...] --on <Resource>` | Atomic PostgreSQL compare-and-swap with version bump |
| **Association / FK** | `jails g association <Name> childField=parentField... --on <Child> --yields <Parent>` | Emits foreign key constraint, validation, and schema tests |
| **Full-Text Search** | `jails g search <Name> <field>...` | Stored `tsvector` generated column & PostgreSQL search queries |
| **Inbound Webhook** | `jails g webhook <Name>` | Raw-byte HMAC verified endpoint with timestamp skew checks |
| **Outbound Sink** | `jails g http-sink <Name> --on <Usecase> --yields <Event>` | Transactional outbox HTTP relay with idempotency key |
| **Migration** | `jails g migration <desc>` | Creates next sequential `VNNN__<desc>.sql` under `db/migration/` |
| **Add Database** | `jails add db` | PostgreSQL JDBC, Flyway, Testcontainers `@ServiceConnection`, compose.yaml |
| **Add API Handling** | `jails add api` | RFC 9457 Problem Details `@RestControllerAdvice`, sealed `ApiException` |
| **Add Kafka** | `jails add kafka` | KRaft broker in compose, dead-letter recovery (`.DLT`), error handlers |
| **Add Redis** | `jails add redis` | `KeyValueStore` wrapper, compose service, mandatory TTL enforcement |
| **Add Observability** | `jails add observability` | Prometheus scrape endpoint, customizer tagging metrics with app name |
| **Inspect Routes** | `jails routes [--json]` | Derived list of HTTP endpoints directly from Java source |
| **Diagnose Project** | `jails doctor [--json]` | Diagnostic checks for classpath, compose services, properties |
| **Run Tests** | `jails test [--fast]` | Fast test execution using bytecode constant-pool affected analysis |
| **Run App** | `jails run [--watch]` | Starts compose services and runs Spring Boot application |

---

## 2. Field Specification DSL

Format: `fieldName:type[modifiers]`

### Types
- `string`: `String` / `VARCHAR(255)` (or `TEXT`)
- `int` / `integer`: `int` / `INTEGER`
- `long`: `long` / `BIGINT`
- `boolean`: `boolean` / `BOOLEAN`
- `uuid`: `UUID` / `UUID`
- `instant`: `Instant` / `TIMESTAMP WITH TIME ZONE`
- `localdate`: `LocalDate` / `DATE`
- `decimal`: `BigDecimal` / `NUMERIC(19, 4)`
- `double`: `double` / `DOUBLE PRECISION`
- `bytes`: `byte[]` / `BYTEA`

### Modifiers
- `!` : Required / non-null (e.g. `email:string!`)
- `?` : Optional / nullable (e.g. `phone:string?`)
- `@unique` : Unique constraint (e.g. `email:string!@unique`)
- `@scope` : Scoped / tenant partition key (e.g. `org_id:uuid@scope`)
- `@index` : Explicit database index

---

## 3. Standard Architecture Rules

1. **No ORM**: Never generate JPA entities (`@Entity`, `@Table`) or Hibernate annotations. Jails uses pure Java records and Spring `JdbcClient`.
2. **Explicit Ports & Adapters**:
   - Ports: Pure interfaces (e.g. `UserRepository`).
   - Adapters: Raw SQL implementations (`JdbcUserRepository`) and in-memory test fakes (`InMemoryUserRepository`).
   - Exactly one adapter is a `@Repository` bean (JDBC when `spring-boot-starter-jdbc` exists; in-memory fake otherwise).
3. **Transactional Mutations**:
   - Multi-file updates are prepared in-memory and committed atomically with Write-Ahead Logging (`.jails/`).
   - Always run with `--pretend` first if checking what files will change.
4. **Idempotency & Outbox**:
   - When emitting domain events from use cases, use `--yields <EventName>` to generate transactional outbox tables and relays.
   - Sinks deduplicate on stable event UUIDs.

---

## 4. Common Workflows

### Starting a New Microservice with PostgreSQL
```bash
jails new order-service --deps web,jdbc,validation
cd order-service
jails add db
jails add api
jails add actuator
jails add observability

# Scaffold core resource
jails g scaffold Order user_id:uuid! total_cents:long! status:string!

# Add business actions
jails g usecase PlaceOrder user_id:uuid! total_cents:long! --on Order --yields OrderPlaced
jails g transition CancelOrder id:uuid! status:string! --on Order
jails g query OrdersByUser user_id:uuid! --on Order

# Verify and run
jails doctor
jails routes
jails test
jails run
```

### Diagnosing & Fixing Issues
- Run `jails doctor` to verify environment dependencies, Docker status, and application properties.
- Run `jails routes` to inspect all mapped endpoints.
- If an app fails to start, run `jails why <log_file>` for parsed root cause analysis.
