# Research Report: 1000x Developer Experience for `jails`

Date: 2026-08-25. **Pruned to what is still open: 2026-08-26 at HEAD `e9ca5ca`.**
Status: engineering proposal grounded in the current `jails` worktree and upstream implementations.

## What this file is now, and how to read the parts that are gone

The original report was a 4,442-line RFC covering the whole product direction.
Most of it shipped. **A delivered section is *deleted* from this file rather
than marked done**, the same convention `pending.md` and `bugs.md` use, so this
document is a list of what is *not* built. `git log -p -- research.md` is where
the delivered text lives, and `git show 2f8003b:research.md` prints the last
full version — worth reaching for when you need the normative contract for
something that already exists, because that prose is now documenting code and
the code is the authority.

**Section numbers are stable and never reused.** A `research.md §7.4` citation
in the source still resolves to a subject through git even though §7.4 is no
longer here.

### Delivered since the report was written, and therefore removed

Verified against HEAD by running the binary, not by reading the roadmap:

- **§2.1, §6.6, §7.8 — the unified test front door.** `jails test` has the full
  Section 6.6 grammar (`--scope`, `--engine auto|build|warm`,
  `--compile auto|ide|build|none`, `--watch`, `--affected`, `--failed`, `--tag`,
  `--fail-fast`, `--slowest`, `--until-fail`, `--repeat`, `--timeout`,
  `--explain-selection`), backed by `TestExecutionPlanV1`, `TestEngine` and
  `TestEnginePolicy` in `jails-protocol`.
- **§2.2, §6.5, §7.18 — explicit run ownership.** `--launcher
  auto|classpath|build-tool|jar`, `--compile`, `--services existing|start|none`.
- **§2.5, §6.2, §7.4 — the SQL contract compiler.** `jails sql check|generate`
  with `--offline`, `--live`, `--datasource`, `--frozen`, `--no-cache`, and the
  `EvidenceLevel { Parsed, VerifiedOffline, VerifiedLive, Executed }` vocabulary
  in `jails-protocol::vocabulary::database`.
- **§3.2 — the dry-run and diff contract.** Global `--pretend`, `--diff`, `--ast`.
- **§3.4, §4.3, §6.3, §7.5 — bounded schema observation.** `jails introspect`,
  `pull`, `schema diff`, and `SchemaObjectId`.
- **§3.5 — migration linting by risk.** `jails migrate lint`.
- **§3.6, §6.7 — evidence-tagged diagnostics.** `CauseNode` in
  `jails-report::why`; reports carry `evidence:` and `limitation:` lines.
- **§3.7 — architectural fitness gates.** `jails-generate::architecture` emits
  an ArchUnit suite and `.jails/architecture.toml`; `jails lint`.
- **§3.8, §6.8, §7.11 — reversibility and portable plans.** `jails history`,
  `show`, `undo`, and global `--plan-out` / `--plan-in`.
- **§3.9, §6.9 — HTTP contracts.** `jails contract emit|check`.
- **§4.8 — JDK 26 default with a 21 floor.** `pom::TARGET_RELEASE`.
- **§4.9, §6.11, §7.15 — the editor protocol.** `jails editor`.
- **§4.10, §6.12, §7.16 — the application tool gateway.** `jails request`,
  `runner`, `logs`, `console`, `db console`.
- **§4.11, §6.13, §7.17 — coordinated rename.** `jails rename resource|storage`
  with `--strategy preserve-table|single-cutover|rolling`. *(Reachable only with
  a slice selector; see §4.2 below and `bugs.md` B2.)*
- **§4.12, §6.14, §7.19 — the resource lifecycle.** `jails resource
  status|repair|revive|field add|rename|type|nullability|drop`, append-only
  migration sealing, preserved-table tombstones.
- **§7.1, §7.7 — one execution protocol and machine-readable results.** The v2
  command envelope, now including a failure envelope with `timings`.
- **§8 — the Java blueprints.** They are `templates/**.java` now; the files are
  the authority and an editor can check them.
- **§0.2 — the three Phase-0 defects.** No `PROBE` output remains; a plain-Maven
  scaffold refuses before writing rather than emitting Spring imports.
- **§0.4 — the acceptance portfolio.** `examples/proof-policy.tsv` is enforced by
  `tests/cli/examples.rs`; `minicom` and `minicom-spring` are both held, with an
  explicit tier and cadence per manifest.
- **Timing spans** are in the JSON envelope under `timings`.

Everything below is what remains.

---

## Section 0: Where the work actually is now

### 0.1 One missing check accounts for most of the remaining harm

The dogfooding ledger (`bugs.md`, rechecked at this same HEAD) closed thirteen
reports and left nine. Three of the survivors — B18, B2, B22 — are different
roads to one destination: a project where `doctor` is green, `mvn verify` is
green, and every insert fails at runtime because the Java names a column no
migration creates.

The enabling gap is a question nothing asks:

> **does a recorded entity's field list match the columns its migrations
> created?**

`doctor` answers *"are these the bytes jails wrote"*. `capability_drift_checks`
already does the harder shape of work for capabilities — it re-plans, purely,
and reports what the plan wants and the project lacks. Entities have no
equivalent, and every projection needed to build one already exists in
`jails-generate`'s planning half.

This is the highest-leverage remaining item in the whole report, and it is
smaller than anything else on this list.

### 0.2 The second theme is oracles that disagree

`bugs.md` B41, B37 and B43 are one shape: two or three commands read the same
store and answer differently, and a `fix:` line names a command that then
refuses. The cheapest control is a conformance test that extracts every `fix:`
command the scenario suite produces and asserts it does not immediately refuse.

### 0.3 The declarative path is a tier behind the imperative one

`app apply` has no route to the field-evolution or storage-policy machinery the
CLI grew (`bugs.md` B20 and B22). Since `new --app` and the whole proof-app
portfolio run on the manifest, this is not a side path. The fix is routing, not
new mechanism: an added or changed field in a `[[generate]]` block should build
the same canonical request `jails resource field add` builds, and a removed block
should require the same storage policy `destroy` requires.

### 0.4 Constraints that still hold

- Generated migrations are forward-only; file recovery is roll-forward. This RFC
  does not add generated down migrations or database undo.
- `jails` never reads, writes, parses or invokes a foreign build file.
  Recognising a filename is not understanding a build.
- No shipped `jails` runtime or framework jar, no web or desktop UI, no ORM/JPA
  substitution, no opaque runtime magic, no unsafe or unexplained mutation path.
- A fast path must prove its eligibility. If a selector cannot prove an affected
  set is safe, it widens. If SQL analysis is parse-only, output says
  `parsed`, never `verified`.
- Preview and apply are the same computation.

---

## Section 1: What remains of the top twelve

| Rank | Breakthrough | State | What is left |
|---:|---|---|---|
| 1 | SQL Contract Compiler | **shipped** | — |
| 2 | Evidence-Carrying Prepared Diffs | **shipped** | — |
| 3 | Safe Resource Lifecycle and Coordinated Rename | **mostly shipped** | the coherence check (§0.1); the logical→physical column binding (§3.10); rename reachable without slices (§4.2) |
| 4 | Unified Fast Test and Run Loop | **Maven only** | warm-engine parity for Gradle (§5.1) |
| 5 | Maven/Gradle Behavioral Parity | **partial** | §5.1 |
| 6 | Existing App-Manifest Extension | **open** | §4.1, §4.2, §4.7, and the routing in §0.3 |
| 7 | Bounded Schema Observation | **shipped** | — |
| 8 | Evidence-Bounded Diagnostics | **shipped** | — |
| 9 | Generated Test Economy | **partial** | §4.6 — `g factory` exists; states, seeds and the repository contract test do not |
| 10 | Application Tool Gateway | **shipped** | — |
| 11 | Versioned Editor Bridge | **shipped** | — |
| 12 | Adoptable Architecture Fitness | **shipped** | — |

---

## Section 2: Remaining latency and index work

### 2.3 Incremental source and AST indexes

**Not built.** There is no persistent source index, no index epoch, and no
`JavaEditReport`. `jails routes`, `beans`, field evolution and merge previews
each re-read what they need.

Incremental AST updates should be one shared facility, not separate caches inside
commands, keyed by `(canonical path, content digest, parser version)`. Store only
facts derivable from source:

- package and top-level/nested types;
- record components and annotations;
- imports and constructor parameters;
- Spring stereotype, bean, and mapping annotations;
- generated ownership anchors and syntax spans.

On edit, parse the changed file and update its edges in one transaction.
Consumers see an immutable index epoch. A parse failure preserves the last good
facts only as `stale`; it may not silently present them as current. Terminal
output must say:

```text
routes: 18 current, 2 unavailable
  src/main/java/.../LegacyController.java:87 parse failed near `}`
```

A semantic edit vocabulary is useful as a **report view**, not as a new mutation
engine:

```rust
enum JavaEditReport {
    AddImport { qualified: JavaType },
    AddAnnotation { target: TypeId, annotation: Annotation },
    AddRecordComponent { target: TypeId, component: FieldSpec },
    AddMember { target: TypeId, anchor: MemberAnchor, source: String },
}
```

These values describe what an existing owned-region/text splice changed after
preparation. Applying changes remains the current ownership-aware byte
preparation and three-way merge. An unowned or ambiguous Java shape conflicts;
`jails-java` MUST NOT grow classpath symbol resolution, whole-file AST rewriting,
comment reprinting, or import disambiguation. A future external OpenRewrite
adapter, if justified, runs as an explicit external verifier/recipe and still
returns bytes through the same prepare path.

**Prerequisite for measurement:** the report claimed a latency win here and never
measured one. Record a dated baseline for `routes`/`beans` on the largest proof
app before building this, or it is an optimisation with no number behind it.

### 2.4a Additive test-dependency hints

**Not built.** Constant-pool reachability is conservative but not complete:
interface injection, Spring configuration, AOP pointcuts, profiles/conditions,
reflection, resources, and context-cache keys create edges absent from direct
bytecode references. The widening rules for those cases are implemented. What is
missing is the escape hatch for irreducible project-specific edges:

```toml
# .jails/app.toml
[[test_dependency]]
input = "src/main/resources/db/migration/**"
tests = ["**/*RepositoryIT", "**/*MigrationIT"]

[[test_dependency]]
input = "src/main/resources/application*.properties"
tests = ["**/*ContextIT"]
```

Hints can only **add** tests to the computed set; they can never remove a test,
suppress widening, or make an incomplete graph current. Rename/delete validation
must report stale patterns.

### 2.4b Service identity labels

**Not built.** Discovery and the refusal rules shipped —
`jails-drive::datasource` resolves only already-available endpoints, `jails
start`/`stop` remain the only CLI-owned Compose lifecycle, and nothing
absence-provisions. What is missing is stable identity for diagnostics and
live-SQL cache keys: hash the committed image/tag, ports, non-secret environment
names, init scripts, and relevant migrations, and label managed resources:

```text
dev.jails.project=<root-id>
dev.jails.service=postgres
dev.jails.spec=<sha256>
dev.jails.managed=true
```

### 2.4c Semantic readiness

**Not built, and it has a live defect behind it** (`bugs.md` B10).
`jails-project::compose` builds `["up", "-d"]` with no `--wait` and there is no
healthcheck or probe on the start path, so `jails run` can boot Spring before
PostgreSQL accepts TCP connections.

Readiness must be semantic — `SELECT 1`, broker metadata — not "the container is
running". A merely live PID is `started`, not `ready`. Docker, Podman, WSL and
Colima are capability-tested **by consumer**: a container visible to a CLI is not
assumed visible to Testcontainers.

### 2.7 Experimental Ecto-style SQL sandbox

**Deliberately deferred. Not a roadmap dependency, and not a default.** Kept here
because the negative result is worth recording if the experiment is ever run.

Ecto's SQL Sandbox makes a real database cheap by checking out a connection per
test, opening a transaction, and rolling it back; shared mode lets collaborating
processes use the test owner's connection. Spring's ordinary test transaction is
bound to the current thread, so a request handled on another thread, an outbox
poller, or a `REQUIRES_NEW` operation can escape it.

Four explicit isolation modes to prototype:

| Mode | Best for | Guarantee | Known limitation |
|---|---|---|---|
| in-memory fake | domain/use-case unit tests | deterministic port behavior | no database semantics |
| ordinary `@Transactional` | same-thread repository tests | rollback on the test thread | work on other threads escapes |
| generated shared `SandboxDataSource` | HTTP tests whose collaborating threads can be bound to one checkout | real SQL and rollback without truncation | incompatible with real commit/independent transactions unless explicitly handled |
| isolated schema/database | outbox, jobs, concurrent transactions, commit behavior | strongest realistic isolation | slower setup/cleanup |

The candidate consists only of generated test code: a JUnit extension, connection
lease, `DataSource` decorator, and opt-in annotation. It is never a `jails`
runtime dependency. A proof must include HTTP thread handoff, connection-pool
exhaustion, nested and `REQUIRES_NEW` transactions, virtual threads, outbox
polling, timeout cleanup, and parallel failure cases. Measure against the
recorded Failsafe tail using the same application and warm-run procedure. Promote
only if it materially lowers wall time without weakening a test's semantics;
otherwise retain per-schema/Testcontainers isolation **and record the negative
result here**.

---

## Section 3: Remaining correctness work

### 3.3 Frozen conflicts, `continue`, and `abort`

**Half built, and the honest half is the one that is missing.** Three-way
reconciliation, conflict detection and per-path reporting all work: every
conflicting path is collected and reported together rather than erroring on the
first. But `jails-prepare::pipeline::diff` says it outright — the marker bytes
are **produced and dropped**, because writing them without a pending candidate
would record markers as the entity's output and the *next* generate would merge
against them. `PendingIdentity`, `ResolutionIdentity` and `RestoreIdentity` exist
in `jails-protocol::durable::conflict` with no route and no CLI verb.

The consequence is visible in `bugs.md` B18: after a conflict the only offered
escape is "move your version aside, or destroy and regenerate".

What is needed is a complete durable state machine, not another flag:

1. a conflicted prepare commits **marker bytes plus a pending candidate**, so the
   base a later run diffs against is the pre-conflict base, not the markers;
2. `jails resource resolve continue` re-reads the resolved file, validates that
   no marker tokens survive, and commits it as the entity's new base;
3. `jails resource resolve abort` restores the pre-conflict image from the
   pending candidate and retires it;
4. any ordinary mutation on an entity with an open pending candidate refuses and
   names both verbs;
5. crash recovery over a pending candidate rolls forward to the same two choices.

Each conflict should show the selector and candidates:

```text
CONFLICT src/main/java/.../Order.java
  wanted: add component `status: OrderStatus` to record Order
  found:  two top-level records named Order after parse recovery
  kept:   current file unchanged
  fix:    make the target unique, then rerun; or use --package/--type
```

There must be no generic `--force` that discards unowned Java. Existing narrowly
scoped destructive authorization — removing a hand-written strategy
implementation whose generated interface is being destroyed — remains valid only
for that named operation and after its exact diff is confirmed. New conflicts use
a narrowly named `--accept-generated <operation-id>` only after the resulting
diff is shown and becomes part of a new prepared bundle.

### 3.10 The logical-to-physical column binding

**Not built, and it is one refusal away from being reachable.**
`jails-engine::route::field` refuses today with:

```text
jails: `--column preserve` needs a recorded logical-to-physical column binding.
       fix: use `--column single-cutover`, or wait until the binding model is available.
```

So a field can be renamed in Java only by also renaming its column. That is the
wrong default for a column with a live consumer — a view, a routine, a
hand-written query, an external reader — and it is exactly the case
`preserve-table` covers at the entity level.

`TableBinding` exists for entities. The field-level equivalent is a recorded
`(EntityId, field name) → column name` pair per managed entity, written at
create time and consulted by every SQL projection instead of re-deriving the
column from the field name. Once it exists, `--column preserve` is a ledger edit
with no migration, and `single-cutover` is that edit plus a forward
`alter table … rename column`.

The same record is what makes §0.1's coherence check cheap: comparing a
declaration to a schema needs a binding to compare *through*.

---

## Section 4: Remaining authoring work

### 4.1 A backward-compatible slice DSL

**Not built.** All four extensions refuse today: `status:enum.PENDING.PAID`
(package segment `enum` is a Java reserved word), `n:int=0` (unknown field type),
`at:instant@audit` (unknown constraint), and there is no `--with-events` or
`--with-audit`. `RelationSpec` and `EventSpec` do not exist. Composite
relationships and join tables are unexpressible in either surface.

Keep the existing compact field tokens and extend them through a typed grammar
rather than ad hoc suffix checks. The CLI form is deliberately shell-safe:
unquoted tokens contain no braces, angle brackets, brackets, parentheses, pipes,
spaces, glob characters, or arbitrary SQL.

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

`shell-safe-literal` is limited to letters, digits, `_`, `.`, `:`, `+`, and `-`.
Values outside that alphabet, collections, composite keys/relations, join tables,
database expressions, policies, and custom types use structured manifest fields.
The conformance suite passes every documented CLI example through Bash, Zsh,
Fish, PowerShell, and direct argv construction and asserts identical tokens.

Parsing produces stable values such as `FieldSpec`, `RelationSpec`, `IndexSpec`,
and `EventSpec`. Decimal amount plus currency is modeled explicitly or as a
separately generated `Money` value; `money` is not a magic field type and never
silently chooses scale, currency, cents, or floating point.

The command must print normalization in debug/JSON output:

```text
accountId:uuid → column account_id uuid not null
total:decimal@positive → BigDecimal + configured numeric scale + check(total > 0)
```

**Note the ordering dependency:** `@audit` and `--with-audit` overlap the
existing `--timestamps` and `AuditPolicy::CreatedAndUpdated`. Decide whether the
annotation is a spelling of the existing policy or a distinct one *before*
shipping it; two ways to say the same thing is the drift generator this
repository has paid for twice.

### 4.2 Contexts and slices, not CRUD bags

**Not built at the CLI, and it is already load-bearing elsewhere.**
`SliceSpecV1` and `SliceName` exist in `jails-protocol::vocabulary::application`
and nothing reaches them: `jails g scaffold Billing.Order` is rejected as an
invalid Java identifier. Meanwhile `jails rename resource` *requires* a
`<slice>.<name>` selector, which is why it cannot be used on any project `jails
new` produces (`bugs.md` B2). One of the two has to move.

Phoenix's context generator makes the domain boundary a first argument and
augments an existing context with conflict prompts. `jails` should allow:

```text
jails g scaffold Billing.Order ...
jails g scaffold Support.Order ...
```

The same noun may exist in different slices. Package layout, ports, migrations,
and route prefixes derive from the slice. Cross-slice references require an
explicit port or event; a generated ArchUnit rule enforces it — and the ArchUnit
generator already exists, so this is a rule to add rather than machinery to
build. The existing app manifest records the mapping so singularization or
package guessing is never the durable identity.

**A project with no slices must keep working unchanged.** The unqualified
spelling stays the default and resolves to a single implicit slice; that implicit
slice is also what `rename resource` should accept, which closes B2 without
waiting for the rest of this section.

### 4.6 Fakes, factories, seeds, and contracts

**Partially built.** `g factory` generates a test data builder whose defaults
come from the same `sample_value` the generated tests use, and a component jails
cannot sample starts `null` with `build()` throwing by name. The in-memory fake
is generated and carries `@Repository` under the documented rules. What is
missing is the part that catches drift.

- **The repository contract test — the highest-value item here.** Generate one
  contract interface executed once against the fake and once against
  `JdbcOrderRepository`, so semantic drift between them becomes a failing test.
  Today the two adapters can disagree indefinitely and nothing notices.
- Factory **named states** and sequences: `.paid()`, `.cancelled()`,
  `.withUserId(...)`, for combinatorial tests. JSON fixtures stay the readable
  stable examples.
- **Seeds:** `db/seeds/*.json` plus a plain Java `SeedRunner` that goes through
  repository *ports*, never JDBC directly. Production execution requires an
  explicit profile or flag.
- Use `@Transactional` rollback for in-process integration tests and unique
  schema/database names for tests that spawn threads or commit independently.

The fake must document what it does not emulate: locking, isolation, vendor
collation, constraint timing, SQL planner behavior. Those remain live-database
contract tests.

### 4.7 Policy and contract generation

**Not built.** `@scope` and `spring::require_scope_authorizer` cover the tenancy
half — a request-boundary field proved against a same-named JWT claim — and
nothing covers role or ownership authorization.

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

This closed form means "permit when the principal has an allowed role or when
`owner_field` equals `principal_claim`". V1 has **no** expression string, SpEL
passthrough, function call, negation, or user-defined evaluator — the same rule
that keeps `@check(...)` out of the field spec. Generate a sealed policy decision
type, explicit authorizer port, table-driven unit tests, Spring adapter
configuration, event record, JSON Schema/OpenAPI component, and
producer/consumer contract tests. Unsupported policy logic remains ordinary
hand-written code behind the authorizer port.

A policy matrix is high-risk: `--pretend` must summarize added and removed
permissions **separately** from ordinary file edits.

---

## Section 5: Remaining parity work

### 5.1 Gradle behavioral parity

Gradle is a first-class target for project creation, detection, dependency
splicing, generation, destroy and `jails test`. Three paths are still Maven-only,
each refusing by name rather than half-working:

| Path | Where | Why it refuses |
|---|---|---|
| warm test engine | `jails-drive::run::test_plan` gates on `build_engine == TestEngine::Maven` | `testd`, `--engine warm` and `--affected` need a resolved classpath and a hermetic wrapper arrangement |
| `jails fmt` | `jails-engine::route::maintenance::format` | the route runs the formatter in a sandbox laid out from the projection and drives it with Maven; Gradle in a throwaway tree needs its wrapper, its caches and a writable `build/` |
| `jails console` | `jails-drive::console` | resolved runtime classpath |

The refusals are correct behavior and should stay until the parity is real —
`add format` *does* configure Gradle's `spotless {}` block, so formatting is
enforced there; only jails' reviewed-diff guarantee is absent.

**Exit gate:** `jails test`, `--engine build` and `--engine warm` discover the
same requested test universe in Maven and Gradle fixtures, and human/JSON reports
have identical test identities, outcomes, durations, output ownership and exit
status across engines. **Blocker:** no `gradle` binary is installed on the
development machine, so every Gradle claim in this repository is currently
inferred from file contents rather than observed. Installing one is the
prerequisite for the whole row.

---

## Section 6: Remaining CLI surface

### 6.1 Enhanced `generate scaffold`

The CLI surface for §4.1 and §4.2. Retained because it is the shape the two
sections have to agree on.

```text
jails generate scaffold <Slice.Entity|Entity> <field>...
  [--package <java.package>]
  [--route <path>]
  [--index <field[,field...]>]...
  [--unique <field[,field...]>]...
  [--with-events <Event[,Event...]>]
  [--with-audit]
```

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

The operation list depends on installed capabilities. If Flyway is absent, output
says `SKIP migration: no database migration capability`; it does not create dead
SQL. A reference such as `accountId:ref.Account.id` must resolve to one stored key
or planning fails with candidates and a fix. Policy matrices, composite
relations, custom database types, and other complex shapes are manifest-only.

Note the generated layout puts `adapter/jdbc/` and `adapter/memory/` under the
slice and adds `adapter/OrderRepositoryContract.java` — the §4.6 contract test.
That differs from the current flat `adapters` layer, so the slice work and the
contract test should land together or the layout changes twice.

---

## Section 9: Remaining roadmap

Six phases shipped. What is left is small enough to sequence in one list, ordered
by leverage rather than by phase number.

1. **The entity coherence check** (§0.1). Re-plan a recorded entity and compare
   its field list to the columns its migrations create. Closes the enabling gap
   behind `bugs.md` B18, B2 and B22. *Exit gate:* the B18 reproduction — a torn
   transaction adopted by `resource repair` — reports a failure rather than
   `25 checks, all clear`.
2. **Transactional integrity of the write phase** (`bugs.md` B18, B45). A publish
   that cannot complete rolls back or forward, never stops half-applied; a failed
   post-commit effect never unmakes the commit.
3. **Oracle agreement** (§0.2). A conformance test over every `fix:` command the
   scenario suite emits.
4. **Manifest routing** (§0.3). `app apply` reaches field evolution and storage
   policy.
5. **The column binding** (§3.10). Unblocks `--column preserve` and makes item 1
   cheap.
6. **The repository contract test** (§4.6). One interface, two adapters, one
   failing test when they drift.
7. **Frozen conflict `continue`/`abort`** (§3.3). The durable state machine.
8. **Slices** (§4.2), then the extended field grammar (§4.1), then policy
   matrices (§4.7). In that order: the grammar and the policy form both name
   slices.
9. **Gradle warm-engine parity** (§5.1), gated on installing a `gradle` binary.
10. **Semantic readiness** (§2.4c), **service identity labels** (§2.4b),
    **test-dependency hints** (§2.4a), **the shared source index** (§2.3) — each
    behind a dated measurement, not an assumption.

Explicitly deferred until measurements justify them: a domain-specific TUI or web
studio, application-process supervision, JVM class redefinition, implicit service
discovery/provisioning, a general ORM, migration rollback generation, and the
generated SQL transaction sandbox (§2.7). None is a prerequisite for the list
above.

### 9.1 Sequencing principles, unchanged

A feature is complete only when Maven and Gradle have parity **where promised**,
human and JSON output represent the same result, `--pretend` runs the real
preparation path, failures say whether project files changed, and an ordinary
build-tool fallback preserves correctness. Long-lived JVMs are optimization
engines, never correctness authorities. `jails` MUST NOT implicitly provision
application services.

### 9.2 Crate ownership for the remaining work

| Workspace member | Add for this list | Must not absorb |
|---|---|---|
| root `jails` CLI | slice-qualified selectors, `--with-events`/`--with-audit`, `resource resolve continue|abort` | semantic validation, generation logic, a second result model |
| `jails-protocol` | `RelationSpec`, `EventSpec`, field-level `ColumnBinding`, policy values, pending-conflict routing values | project discovery, parsing Java, running tools |
| `jails-spec` | the extended field grammar, slice resolution, policy IR validation | live database observation, filesystem mutation |
| `jails-project` | the shared source index (§2.3), compose readiness probes | starting long-running tools, durable commits |
| `jails-generate` | the repository contract test, factory states, seeds, policy projections | anything that writes |
| `jails-prepare` | the pending-conflict half of §3.3 | a second mutation engine |
| `jails-report` | the entity coherence check | starting anything — it is below `jails-drive` by design |

---

## Source and Evidence Notes

*Trimmed to the sources still bearing on open work. The full inventory of
inspected paths — including every `deps/` checkout used for the delivered
sections — is in `git show 2f8003b:research.md`.*

### Upstream implementations behind the open sections

- `deps/phoenix/lib/mix/tasks/phx.gen.context.ex` — bounded-context scaffolding
  and augment-existing behavior (§4.2);
- `deps/ecto/lib/ecto/changeset.ex` and
  `deps/ecto_sql/lib/ecto/adapters/sql/sandbox.ex` — validation boundaries and
  transactional test checkout/ownership (§2.7, §4.6);
- `deps/laravel/framework/src/Illuminate/Console/GeneratorCommand.php` — factory
  states and sequences (§4.6);
- `deps/jhipster/generator-jhipster/lib/jdl/` — multi-entity declarative
  application modeling (§4.1, §4.7);
- `deps/archunit/archunit/src/main/java/com/tngtech/archunit/library/Architectures.java`
  — the cross-slice rule §4.2 needs;
- `deps/spring-framework/spring-tx/.../TransactionSynchronizationManager.java` —
  thread-bound transactions, which is why §2.7's shared mode is the risky one.

### Primary references

- [Phoenix context generator](https://hexdocs.pm/phoenix/Mix.Tasks.Phx.Gen.Context.html);
  [JHipster JDL](https://www.jhipster.tech/jdl/intro/);
  [Wasp application specification](https://wasp.sh/docs/general/spec) — §4.1, §4.2, §4.7.
- [Laravel factories](https://laravel.com/framework/docs/13.x/eloquent-factories);
  [Laravel database testing](https://laravel.com/framework/docs/13.x/database-testing);
  [Ecto SQL Sandbox](https://hexdocs.pm/ecto_sql/Ecto.Adapters.SQL.Sandbox.html);
  [Spring test-managed transactions](https://docs.spring.io/spring-framework/reference/testing/testcontext-framework/tx.html) — §4.6, §2.7.
- [AdonisJS scaffolding and codemods](https://docs.adonisjs.com/guides/concepts/scaffolding) — §3.3.
- [ArchUnit user guide](https://www.archunit.org/userguide/html/000_Index.html) — §4.2.
- [Gradle Daemon](https://docs.gradle.org/current/userguide/gradle_daemon.html);
  [Maven Daemon](https://maven.apache.org/tools/mvnd.html) — §5.1.
- [Testcontainers reusable containers](https://java.testcontainers.org/features/reuse/) — §2.4b, §2.4c.
