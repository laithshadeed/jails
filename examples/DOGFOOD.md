# Jails application dogfood log

This log records every command used to build the web crawler and the
Intercom-shaped support inbox. The manifests intentionally use the same
generic `capabilities + [[generate]]` vocabulary. There is no crawler or
inbox branch in Jails core.

## Current repeatable flow

Build Jails once:

```bash
cargo build
export JAILS_BIN="$PWD/target/debug/jails"
```

Create an ordinary Spring Boot application, copy the appropriate manifest,
then apply it:

```bash
"$JAILS_BIN" new web-crawler --deps web,validation
mkdir -p web-crawler/.jails
cp examples/web-crawler/.jails/app.toml web-crawler/.jails/app.toml
cd web-crawler
"$JAILS_BIN" app plan
"$JAILS_BIN" app apply --no-start
"$JAILS_BIN" routes
"$JAILS_BIN" beans
"$JAILS_BIN" doctor
"$JAILS_BIN" check
```

From the repository root, repeat for the support inbox:

```bash
"$JAILS_BIN" new support-inbox --deps web,validation
mkdir -p support-inbox/.jails
cp examples/support-inbox/.jails/app.toml support-inbox/.jails/app.toml
cd support-inbox
"$JAILS_BIN" app plan
"$JAILS_BIN" app apply --no-start
"$JAILS_BIN" routes
"$JAILS_BIN" beans
"$JAILS_BIN" doctor
"$JAILS_BIN" check
```

`app apply` records each completed generation intent in
`.jails/app-state-v1`. Repeating it skips completed intents, while every
capability remains independently idempotent through `jails.toml`.

## Exact dogfood command log — 2026-08-21

The automated dogfood tests use the repository's offline Spring Boot fixture,
copy one of the manifests above into `.jails/app.toml`, run
`jails app apply --no-start`, and then invoke Maven. This isolates Jails from
Initializr/network availability while exercising the same generated source.

These are the commands run, in order. Repeated commands are intentional: each
rerun followed a generic fix discovered by the previous failure.

```bash
cargo test app::tests --no-fail-fast
cargo test app_manifest --no-fail-fast
cargo test app_manifest --no-fail-fast -- --nocapture
cargo test app_manifests_compile_without_manual_source_edits -- --nocapture
mvn -q -Dtest='*AssociationIT' test
cargo test app_manifests_compile_without_manual_source_edits -- --nocapture
cargo test app_manifests_compile_without_manual_source_edits -- --nocapture
docker info --format '{{json .}}'
docker info
cargo test app_manifests_pass_the_full_generated_verification_gate -- --nocapture
cargo test app_manifests_pass_the_full_generated_verification_gate -- --nocapture
cargo fmt --all && cargo test app_manifest_builds_the_support_inbox_from_the_same_generic_intents -- --nocapture
cargo test generated_http_sink_delivers_typed_json_with_a_stable_idempotency_key -- --nocapture
cargo test app_manifests_compile_without_manual_source_edits -- --nocapture
cargo test app_manifests_pass_the_full_generated_verification_gate -- --nocapture
cargo fmt -- --check
cargo test app_manifest --no-fail-fast
cargo test optional_transformed_types_map_before_unwrapping
cargo test app_manifests_pass_the_full_generated_verification_gate
rustfmt --edition 2021 --check src/app.rs
cargo test --bin jails
cargo test --test golden
rustfmt --edition 2021 --check src/app.rs
cargo test app::tests
cargo test --bin jails
cargo test spring:: --no-fail-fast
cargo test spring::event_tests --no-fail-fast
cargo test app_manifest --no-fail-fast
cargo test --test golden
cargo test add_kafka_and_generate_event_compile_against_real_spring
cargo test splice_ --no-fail-fast
cargo test app_manifests_compile_without_manual_source_edits
cargo test add_kafka_and_generate_event_compile_against_real_spring
cargo test spring::event_tests
cargo test --test golden
cargo test app_manifests_pass_the_full_generated_verification_gate
cargo test spring::event_tests
cargo test --test golden
cargo test app_manifests_pass_the_full_generated_verification_gate
cargo test --bin jails
cargo test --test golden
rustfmt --edition 2021 --check src/app.rs src/generate.rs src/generate/domain.rs src/spring.rs src/add/database.rs src/add.rs && git diff --check
rg -n '^edition|^rust-version' Cargo.toml && cargo test --test golden -q && git diff --check
cargo test spring:: --no-fail-fast
cargo test app_manifest_builds_ --no-fail-fast -- --nocapture
cargo test app_manifest_builds_ --no-fail-fast
cargo test app_manifests_compile_without_manual_source_edits -- --nocapture
cargo test spring::usecase_tests --no-fail-fast
cargo test app_manifests_pass_the_full_generated_verification_gate -- --nocapture
cargo test spring::usecase_tests --no-fail-fast
cargo test app_manifests_pass_the_full_generated_verification_gate -- --nocapture
cargo test spring::usecase_tests --no-fail-fast
cargo test app_manifest_builds_ --no-fail-fast
cargo test app_manifests_compile_without_manual_source_edits -- --nocapture
cargo test app_manifests_compile_without_manual_source_edits -- --nocapture
cargo test app_manifests_pass_the_full_generated_verification_gate -- --nocapture
cargo test optional_transformed_types_map_before_unwrapping
cargo test app_manifests_compile_without_manual_source_edits
cargo test optional_transformed_types_map_before_unwrapping -q && cargo test app_manifests_compile_without_manual_source_edits -q
cargo test app_manifests_pass_the_full_generated_verification_gate -- --nocapture
cargo test spring::query_tests -q && cargo test app_manifest_builds_ -q
cargo test spring::query_tests spring::usecase_tests --no-fail-fast
cargo test spring:: -q
cargo test app_manifest_builds_ --no-fail-fast -- --nocapture
cargo test app_manifest_builds_ --no-fail-fast -q
cargo test app_manifests_compile_without_manual_source_edits -- --nocapture
cargo test app_manifests_pass_the_full_generated_verification_gate -- --nocapture
cargo test spring::durable_job_tests -q && cargo test app_manifests_compile_without_manual_source_edits -q
cargo test app_manifests_pass_the_full_generated_verification_gate -- --nocapture
cargo test spring::durable_job_tests -q && cargo test app_manifests_compile_without_manual_source_edits -q
cargo test security_ --no-fail-fast -q && cargo test app_manifests_compile_without_manual_source_edits -q
cargo test add_security_writes_an_explicit_chain_that_denies_by_default -- --nocapture
cargo test app_manifests_compile_without_manual_source_edits -q
cargo fmt --all && cargo test spring::usecase_tests spring::query_tests spring::durable_job_tests --no-fail-fast
cargo test spring:: --no-fail-fast
cargo fmt --all && cargo test spring:: --no-fail-fast
cargo test app_manifest_builds_ --no-fail-fast -q && cargo test app_manifests_compile_without_manual_source_edits -- --nocapture
cargo fmt --all && cargo test delivery_tests -q && cargo test app_manifest_builds_ -q && cargo test app_manifests_compile_without_manual_source_edits -q
cargo fmt --all && cargo test target_release_matches_the_binary new::tests:: --no-fail-fast
cargo test target_release_matches_the_binary -q && cargo test new::tests:: -q && cargo test --test golden -- --nocapture
UPDATE_GOLDEN=1 cargo test --test golden -q
cargo fmt --all && cargo test spring:: --no-fail-fast -q
cargo test app_manifest_builds_the_web_crawler -- --nocapture && cargo test app_manifests_compile_without_manual_source_edits -- --nocapture
cargo test app_manifests_compile_without_manual_source_edits -- --nocapture
cargo test app_manifests_pass_the_full_generated_verification_gate -- --nocapture
cargo test add_observability_serves_a_prometheus_scrape -- --nocapture
cargo test app_manifests_pass_the_full_generated_verification_gate -- --nocapture
cargo fmt --all && cargo test app_manifest_builds_ --no-fail-fast -q && cargo test delivery_tests -q
cargo test app_manifest_builds_the_ --no-fail-fast -q
cargo test app_manifests_pass_the_full_generated_verification_gate -- --nocapture
cargo fmt --all && cargo check
cargo fmt --all && cargo test app_manifest_builds_the_support_inbox_from_the_same_generic_intents -- --nocapture
cargo test app_manifests_compile_without_manual_source_edits -- --nocapture
cargo fmt --all && cargo test app_manifests_pass_the_full_generated_verification_gate -- --nocapture
df -h /tmp && find /tmp -maxdepth 1 -type d -name 'jails-e2e-*' -printf '%p\\n' | wc -l && du -ch /tmp/jails-e2e-* 2>/dev/null | tail -n 1 && docker images --format '{{.Repository}}:{{.Tag}} {{.Size}}' | rg '^jails-dogfood-'
find /tmp -maxdepth 1 -type d -name 'jails-e2e-*' -exec rm -rf -- {} +
cargo fmt --all && cargo test app_manifest_builds_the_support_inbox_from_the_same_generic_intents -- --nocapture
cargo test app_manifests_compile_without_manual_source_edits -- --nocapture
cargo fmt --all && cargo test app_manifests_pass_the_full_generated_verification_gate -- --nocapture
cargo fmt --all && cargo test app_manifests_compile_without_manual_source_edits -- --nocapture
cargo test app_manifests_pass_the_full_generated_verification_gate -- --nocapture
cargo fmt --all && cargo test app_manifest_builds_the_support_inbox_from_the_same_generic_intents -- --nocapture
cargo test app_manifests_compile_without_manual_source_edits -- --nocapture
cargo test app_manifests_compile_without_manual_source_edits -- --nocapture
cargo fmt --all && cargo check
cargo fmt --all && cargo test app_manifests_compile_without_manual_source_edits -- --nocapture
cargo test app_manifests_pass_the_full_generated_verification_gate -- --nocapture
cargo test app_manifests_pass_the_full_generated_verification_gate -- --nocapture
cargo test --test golden -- --nocapture
UPDATE_GOLDEN=1 cargo test --test golden -q
cargo test --bin jails
cargo test --test cli
cargo test --test cli generate_scaffold_produces_a_project_that_compiles_and_passes_tests -- --exact --nocapture
cargo test --test cli a_scaffold_with_database_types_compiles_including_its_derived_jdbc_adapter -- --exact --nocapture
cargo test --test cli generate_dto_client_and_job_compile_and_pass_against_real_spring -- --exact --nocapture
cargo test --test cli every_generator_and_capability_together_compiles_and_passes_tests -- --exact --nocapture
cargo fmt -- --check && cargo test --test golden -q && git diff --check
cargo fmt --all && cargo test delivery_tests spring:: --no-fail-fast -q && cargo test app_manifests_compile_without_manual_source_edits -q
cargo test --bin jails -q && cargo test app_manifests_compile_without_manual_source_edits -q
cargo fmt --all && cargo test app_manifest_builds_the_crawler_skeleton_and_is_resumable -q && git diff --check
mvn -q -Dtest=SafePageFetcherTest test
cargo fmt --all && cargo test app_manifest_builds_ --no-fail-fast -- --nocapture
cargo test app_manifests_compile_without_manual_source_edits -- --nocapture
cargo fmt --all && cargo test app_manifest_builds_the_crawler_skeleton_and_is_resumable -- --nocapture
cargo test app_manifests_compile_without_manual_source_edits -- --nocapture
cargo test app_manifests_pass_the_full_generated_verification_gate -- --nocapture
cargo fmt --all && cargo test --bin jails
cargo test --test cli
cargo test --test golden
cargo fmt --all && cargo test association_and_http_sink_tests --bin jails && cargo test app_manifests_compile_without_manual_source_edits -- --nocapture
cargo fmt --all && cargo test --bin jails && cargo test --test golden && cargo check
cargo test app_manifest_builds_the_support_inbox_from_the_same_generic_intents -- --nocapture
cargo fmt --all && cargo test app_manifest_builds_the_support_inbox_from_the_same_generic_intents -- --nocapture
cargo fmt --all && cargo test app_manifest_builds_the_support_inbox_from_the_same_generic_intents -- --nocapture && cargo test app_manifests_compile_without_manual_source_edits -- --nocapture
cargo test app_manifests_pass_the_full_generated_verification_gate -- --nocapture
cargo fmt --all && cargo test spring::query_tests --bin jails && cargo test app_manifests_compile_without_manual_source_edits -- --nocapture
cargo fmt --all && cargo test association_and_http_sink_tests spring::query_tests --bin jails --no-fail-fast
cargo test --bin jails
cargo fmt --all && cargo test --bin jails
cargo test app_manifest_builds_the_support_inbox_from_the_same_generic_intents -- --nocapture && cargo test app_manifests_compile_without_manual_source_edits -- --nocapture
cargo test --test cli
cargo test --test golden
cargo fmt --all && cargo test --bin jails && cargo check
git diff --check && rg -n 'web-crawler|support-inbox|Intercom|SiteTraversal|ContactWorkspace|ProviderHttp|ConversationAssignment' src templates || true && rg -n '291 generator|285\.00|members, inboxes.*remain|inboxs|unbounded equality' README.md ideas-sol.md examples/DOGFOOD.md examples/ACCEPTANCE.md || true && git status --short && git diff --stat
rg -n 'Crawler|Inbox|Conversation|Workspace|Contact|Provider|Crawl' src templates || true
cargo fmt --all && cargo test --bin jails && cargo check && git diff --check
```

Results:

- Both manifests compile from untouched generated source with
  `mvn -q -DskipTests package`.
- Both manifests pass `mvn -q verify` against real Testcontainers/PostgreSQL
  and Kafka. The durable-work baseline gate took 196.38 seconds; the first
  gate with scoped authorization, the adversarial safe fetcher suite, and real
  OCI builds took 297.04 seconds. The final gate including optimistic
  transitions and the acknowledged transactional outbox took 225.29 seconds.
  The gate with durable traversal, the first three tenant associations,
  provider delivery, and both OCI builds took 206.38 seconds. The final
  expanded inbox gate—including members, inbox membership, assignment, ten
  tenant relationships, and reassignment—took 341.08 seconds.
- The crawler generated 4 Flyway migrations and the support inbox generated
  19; both complete generated test suites passed from fresh manifests.
- The latest kernel verification pass ran all 292 generator unit tests. The
  latest complete sweep ran all 123 CLI/integration tests in 293.35 seconds
  with no failures; it includes the expanded fresh-manifest
  PostgreSQL/Kafka/socket/image gate. Both golden tests and `cargo check`
  passed separately.
- The crawler's typed `PageDiscovered(UUID id, UUID crawlRunId, URI url,
  Instant occurredAt)` event and the inbox's typed
  `MessageReceived(UUID id, UUID workspaceId, UUID conversationId,
  Instant occurredAt)` event both made a real broker round trip. Their
  publishers use the event id as the Kafka key.
- The crawler now has generated `QueueCrawl` and `RecordCrawledPage` create
  workflows plus typed database queries by status/run. The inbox has members,
  inboxes, inbox membership, contacts, conversations, messages, and a unique
  conversation-assignment record. Eight generated create workflows, seven
  tenant-key-shaped queries, and two optimistic transitions cover creation,
  listing, status change, assignment, reassignment, and clearing. Their
  controllers, application ports, implementations, JDBC adapters, and
  mock-free focused tests come from the same field model.
- Both applications now select generic `docker` and `ci` capabilities. Their
  generated workflows pin checkout/setup-java by full commit SHA, and their
  multi-stage images derive Java from the POM and run as `10001:10001`. The
  local gate built and inspected `jails-dogfood-web-crawler:test` and
  `jails-dogfood-support-inbox:test`.
- Inbox request boundaries mark arbitrary fields with `@scope`; Jails compares
  those values with same-named JWT claims in `prod`. Broad scaffold reads and
  deletes that cannot prove scope are not emitted. The crawler's generic
  fetcher revalidates redirects, pins validated DNS results, rejects reserved
  networks, caps bytes/time/redirects/media types, and reports metrics.
- The inbox now uses the generic `transition` intent for conversation status.
  Its generated compare-and-swap includes tenant scope and version in the SQL
  predicate, increments the version atomically, and distinguishes cross-scope
  absence from a stale version.
- `ReceiveMessage` now uses ordinary `strategy_yields = "MessageReceived"`.
  Jails turns that generic linkage into a same-transaction outbox write and a
  leased relay that waits for every configured sink. Its real PostgreSQL/Kafka
  test covers the happy path, stable event identity, bounded retries, and an
  inspectable terminal failure; the provider socket test covers the optional
  HTTP sink.
- `SiteTraversal` is an ordinary `http-workflow` over `PageFetcher`: its real
  PostgreSQL test proves robots handling, exact-origin canonical traversal,
  duplicate/cycle suppression, retry leases, hard page/depth limits, status
  APIs, and persistent cancellation.
- Ten ordinary `association` intents make workspace ownership a persisted
  database invariant across contacts, members, inboxes, membership,
  conversations, messages, and assignments. Their tests prove exact ordered
  foreign-key shape and that impossible cross-boundary historical data cannot
  validate. Migration generation recognizes earlier Jails primary and unique
  key declarations and only adds a target unique index when the required
  ordered key is not already declared.
- Generated equality queries have deterministic key ordering and an explicit
  100-row ceiling. Jails does not guess nullable/list filters, arbitrary sort,
  projections, or keyset cursor semantics.
- `Provider` is an ordinary `http-sink` on the `ReceiveMessage` outbox. Its
  socket contract proves typed JSON, 2xx-only acknowledgement, 503 rejection,
  and the same stable `Idempotency-Key` on retries. Kafka and HTTP share one
  generic ordered sink chain and terminal outbox state.
- The targeted `rustfmt --edition 2021` probe was invalid for this Rust 2024
  crate and also reported the same pre-existing whole-file drift. The manifest
  confirms the actual edition; the independent `git diff --check` whitespace
  check is clean.
- The first formatted `docker info` probe was not portable to the local Podman
  compatibility API; plain `docker info` confirmed the runtime was healthy.
- The sandboxed all-CLI sweep passed 118/122 tests; four real-Spring fixtures
  could not open local sockets or let Mockito self-attach. Each of those four
  passed unchanged when rerun with local-runtime access, so this was an
  execution-sandbox limitation rather than a generated-code regression.

The full interactive flow above remains the acceptance path for project
creation. It additionally requires Initializr/network access. When any step
needs a manual source edit, record it below before making the edit. That edit
is evidence for the next generic generator improvement.

## Defects discovered and fixed

| Attempt | Observable failure | Generic fix made in Jails |
|---|---|---|
| compile | multiword resources generated variables such as `crawlrun`, but other generated code referenced `crawlRun` | all relevant generators now use one lower-camel naming rule |
| compile | `Optional<Instant>` and `Optional<URI>` were passed to transforms such as `Timestamp.from(...)` before unwrapping | optional JDBC writes map the contained value before `orElse(null)` |
| verify | DB/Testcontainers wiring only covered tests that existed when `db` was added; later generated `@SpringBootTest` classes reached an unrelated local database | `app apply` reconciles every generic capability after all generation intents |
| verify | the observability contract called a protected Prometheus endpoint anonymously when `security` was also present | the generated probe authenticates through the real filter chain with test-only credentials; the endpoint stays protected |
| compile | `generate event` ignored declared fields and always emitted a `String id, Instant occurredAt` payload | event contracts, constructors, examples, and broker tests now derive from the shared typed field model; typed events require a stable non-optional `id` |
| compile | database reconciliation added a second `@Import` to a Kafka integration test, but Spring's annotation is not repeatable | the generic Java splicer merges configuration classes into one `@Import` and removes only its own member on unsplice |
| test/JDK | the generated `KafkaConfigTest` depended on Mockito self-attachment, which is blocked under the sandboxed JDK 26 runtime | the generated Kafka configuration contract now uses JDK dynamic proxies and no mocking agent |
| verify | the broker test generated a valid UUID payload but still asserted the legacy literal `probe-1` | samples and assertions now come from the same field model, so the generated test oracle cannot drift from its event constructor |
| compile | a use-case field parsed from a literal spec compared `Integer` with `int`, and `@NonBlank` input with the target's required component, as different types | compatibility now compares semantic Java type/nullability instead of syntax produced at different parse stages |
| verify | generated transactional use-case implementations were `final`, so Spring could not create its CGLIB transaction proxy | transactional generated components are proxyable non-final classes, protected by a generator regression test |
| compile | a JDBC query mapper used `Optional` without importing it when the target contained nullable fields | adapter imports are derived from generated read expressions as well as declared column types |
| verify | nullable `Instant`, URI, and owned-enum JDBC reads dereferenced the database value before wrapping it in `Optional` | transformed nullable reads now call `Optional.ofNullable(raw).map(transform)`, verified centrally and against PostgreSQL |
| generate | durable work declared a non-text payload with the `!` non-blank suffix | the shared parser rejects the semantic mismatch and the manifest uses required scalar syntax |
| verify | PostgreSQL rejected an unqualified `id` in `UPDATE ... FROM claimed ... RETURNING` as ambiguous | durable stores qualify every returned queue column with the target table alias; a source regression pins it |
| security | Boot's default generated password still appeared and no production identity scheme existed | local-only explicit BCrypt credentials and a separate `prod` JWT resource-server chain are generated |
| tenancy | generic scaffold CRUD exposed operations that could not prove tenant ownership | `@scope` wires same-named claim checks into commands/queries/jobs and omits broad scaffold routes that cannot preserve the scope boundary |
| delivery | manifests had no reproducible CI or image output | generic `ci` and `docker` capabilities generate least-privilege pinned workflows and a non-root multi-stage image |
| crawler | the ordinary service client followed a different trust model and did not close DNS rebinding | generic `fetcher` validates and pins every hop, bounds the response, classifies failures, emits metrics, and includes adversarial socket tests |
| verify | Spring saw the fetcher's public production constructor plus its package-private test seam and looked for a nonexistent default constructor | the production constructor is explicitly selected for injection; the restricted constructor remains available only to same-package adversarial tests |
| verify | the authenticated Prometheus probe still configured Boot's removed default-user property names after security moved to explicit local credentials | every generated authenticated probe now uses `app.security.dev.*`, the single local credential contract owned by the security capability |
| verify | a scoped scaffold test expected both removed read shapes to return 405, but Spring correctly reports an unmapped item path as 404 | the contract now distinguishes “method absent on an existing collection path” (405) from “no item route/resource exists” (404) while proving neither broad read is callable |
| generate | the first optimistic-transition binding compared Java lower-camel component names with SQL snake-case column names and panicked on `workspaceId` | transition parameters now resolve through the shared SQL column model, so names, JDBC values, and predicates cannot drift |
| verify | the transactional JDBC transition was generated `final`, preventing Spring from creating its class-based transaction proxy | transactional transition adapters are proxyable classes, matching the invariant already enforced for generated use-case implementations |
| compile | the first outbox decorator returned the target domain record from the service package without importing it | outbox composition now derives and emits the target import from the same configured package model as ordinary use cases |
| verify | committed query integration fixtures reused the generator-wide sample UUID and collided with later transition tests | generated database query tests are transactional and roll back their fixtures, making the shared Testcontainers database deterministic |
| verify | an outbox test compared a freshly created `Instant` with PostgreSQL's microsecond-precision round trip | the generated assertion proves the persisted business effect by stable identity and leaves temporal precision to repository mapping tests |
| test command | Cargo accepts one name filter, so the first combined `delivery_tests spring::` probe was invalid | reran the complete 289-test generator unit suite and the fresh-manifest compilation gate |
| runtime permission | the focused generated fetcher test needed an ephemeral loopback socket and the escalation reviewer was busy, so the command was rejected before Maven started | keep the source/compile regression green now and run the adversarial socket test in the next authorized full-manifest gate |
| source audit | generated durable workers and outbox relays carried `@Scheduled`, but no generated configuration enabled scheduling | all scheduling generators now share one idempotent `SchedulingConfig`, and both fresh manifests assert that it exists |
| crawler | a bounded traversal was still hand-written application work despite the safe fetch boundary | generic `http-workflow --on <Fetcher>` now composes a PostgreSQL frontier, robots policy, canonical exact-origin link traversal, hard page/depth bounds, retry leases, cancellation, APIs, metrics, migration, and integration tests |
| verify | the accepted-status fetcher test returned `text/plain` while its deliberately narrowed test policy allowed only HTML, so media-type enforcement correctly rejected it | keep accepted statuses subject to every safety policy and make the fixture isolate status handling with an allowed media type; production defaults separately permit `text/plain` robots files |
| tenancy | matching a JWT `workspaceId` at the HTTP boundary did not prove that a supplied `contactId` or `conversationId` belonged to that workspace | generic `association --on <Child> --yields <Parent>` validates explicit field mappings, emits ordered composite PostgreSQL foreign keys and target uniqueness, and generates real constraint-shape plus invalid-data tests; checks are deferred to commit so an atomic unit of work can write related rows in either order |
| generate | the first HTTP-outbox template placed a Jails placeholder directly inside Spring's `${...}` syntax, then its test render supplied one obsolete value; the strict renderer rejected both mistakes before Java compilation | construct complete Spring property expressions in the generator, substitute them as ordinary values, and keep each template's inputs exact |
| runtime permission | the focused provider contract could not bind its loopback HTTP server inside the filesystem sandbox | reran the identical documented command with local-runtime permission; the typed JSON, rejection, and stable `Idempotency-Key` contract passed |
| test infrastructure | 15,288 retained `jails-e2e-*` fixtures consumed 5.5 GB of `/tmp`, so Maven reached `repackage` after real crawler tests and failed with `Disk quota exceeded` | audited the exact task-owned prefix, removed only those disposable fixtures, and reran from fresh manifests; fixture lifecycle cleanup remains a harness improvement |
| generate | an `association` with no field mappings could reach rendering and emit empty-column SQL | reject the intent before reading the project unless it has at least one explicit `childField=parentField` mapping |
| compile | an HTTP-sink event containing a project-owned value type Jails could not fabricate disabled its generated test but also dropped every constructor argument, so the disabled Java source still could not compile | preserve constructor arity with an explicit `null` only for unknown samples, mark the contract disabled and visible, and pin the fallback with a generator test |
| generate | the first expanded assignment relationship produced a PostgreSQL constraint name longer than 63 bytes | the existing generic length preflight stopped before writing invalid SQL and named the exact identifier; the manifest selected a shorter semantic relationship name without any core branch |
| schema | the naive append-`s` convention generated `inboxs` | the shared table/path pluralizer now handles deterministic `-x/-z/-ch/-sh/-ss -> -es` and consonant-`y -> -ies` rules while still refusing to guess irregular nouns |
| query | generated equality reads had stable ordering but no hard result bound | every generic equality query now emits `limit :max_results`, binds the generator-owned value, and caps the first deliberately narrow query contract at 100 rows |
| schema | each association emitted a target unique index even when an earlier Jails migration already declared the same ordered primary/unique key | inspect only recognized Jails Flyway statement shapes and reuse an exact prior key; keep emitting an index when evidence is absent instead of guessing from arbitrary SQL |
| compile | a migration scan used `Result::ok`, which resolved to Jails' one-parameter result alias instead of `std::io::Result` | use an explicit `entry.ok()` closure so inference remains local and the full 292-test generator suite compiles |
| source audit | generic validation paths still illustrated missing inputs with crawler/inbox nouns | replace production help examples with neutral task terminology; keep showcase vocabulary confined to manifests, documentation, and regression fixtures |

## Friction ledger

| Application | Step | Manual intervention or weak output | Generic Jails improvement |
|---|---|---|---|
| Both | project creation | `jails new` depends on Initializr/network | versioned `jails new --offline` baseline |
| Both | manifest setup | user copies `.jails/app.toml` manually | `jails app init --manifest <path>` or `new --app <path>` |
| Both | apply | application planning currently describes logical intents but does not yet produce one atomic `ChangeSet` | lower the whole manifest through the universal planner |
| Both | resume | `.jails/app-state-v1` records completed intent keys but does not yet notice a generated file deleted afterward | store output fingerprints and reconcile drift instead of blindly skipping |
| Both | domain behavior | executable creates, equality queries, optimistic transitions, durable work, transactional publication, bounded HTTP workflows, persisted associations, HTTP delivery, inbox membership, and conversation assignment now remove most walking-skeleton boilerplate; richer query/channel semantics remain | pagination/sort plus generic inbound-signature, realtime/replay, audit, and command/workflow policies, without showcase-specific artifacts |
| Both | architecture | current scaffold is layer-first unless `--package` flattens the slice | feature-first placement with verified Modulith boundaries |
| Both | security | production JWT identity and explicit `@scope` enforcement now exist; role/permission policy and audit are not generated yet | closed authorization-policy inputs plus generated allowed/denied HTTP integration tests |
| Both | tests | scheduled jobs and Kafka listeners start in broad `@SpringBootTest` contexts; differing contexts start several PostgreSQL containers and unrelated tests produce large broker logs | generated test profile, selective listener startup, and shared container/context conventions |
| Both | JDK | the Kafka configuration test is now mock-free, but generated controller tests still warn that Mockito dynamic self-attachment will stop working on a future JDK | prefer mock-free contracts where practical and generate explicit Maven test-agent configuration for the remaining mock-based tests |
| Both | delivery | image and CI are generated and the local gate builds/inspects both images, but it cannot execute hosted Actions | keep hosted CI as a required repository check |

Do not hide this table by hand-fixing the examples. The purpose of both apps
is to turn repeated friction into evidence-backed improvements to generic
Jails commands.

The precise done/not-done boundary is maintained in
[`ACCEPTANCE.md`](ACCEPTANCE.md); the local crawler-safety, tenant, durability,
delivery, and image checks now pass, while hosted CI execution remains an
external repository gate rather than a locally proven claim.
