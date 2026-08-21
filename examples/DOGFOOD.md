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
cargo test app_manifests_compile_without_manual_source_edits -- --nocapture
cargo test app_manifests_compile_without_manual_source_edits -- --nocapture
docker info --format '{{json .}}'
docker info
cargo test app_manifests_pass_the_full_generated_verification_gate -- --nocapture
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
```

Results:

- Both manifests compile from untouched generated source with
  `mvn -q -DskipTests package`.
- Both manifests pass `mvn -q verify` against real Testcontainers/PostgreSQL
  and Kafka. The durable-work baseline gate took 196.38 seconds; the first
  gate with scoped authorization, the adversarial safe fetcher suite, and real
  OCI builds took 297.04 seconds.
- The crawler generated 3 Flyway migrations and the support inbox generated 5;
  both complete generated test suites passed.
- The crawler's typed `PageDiscovered(UUID id, UUID crawlRunId, URI url,
  Instant occurredAt)` event and the inbox's typed
  `MessageReceived(UUID id, UUID workspaceId, UUID conversationId,
  Instant occurredAt)` event both made a real broker round trip. Their
  publishers use the event id as the Kafka key.
- The crawler now has generated `QueueCrawl` and `RecordCrawledPage` create
  workflows plus typed database queries by status/run. The inbox has four
  generated create workflows and tenant-key-shaped queries for contacts,
  conversations, and messages. Their controllers, application ports,
  implementations, JDBC adapters, and mock-free focused tests are generated
  from the same field model.
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
- The targeted `rustfmt --edition 2021` probe was invalid for this Rust 2024
  crate and also reported the same pre-existing whole-file drift. The manifest
  confirms the actual edition; the independent `git diff --check` whitespace
  check is clean.
- The first formatted `docker info` probe was not portable to the local Podman
  compatibility API; plain `docker info` confirmed the runtime was healthy.

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

## Friction ledger

| Application | Step | Manual intervention or weak output | Generic Jails improvement |
|---|---|---|---|
| Both | project creation | `jails new` depends on Initializr/network | versioned `jails new --offline` baseline |
| Both | manifest setup | user copies `.jails/app.toml` manually | `jails app init --manifest <path>` or `new --app <path>` |
| Both | apply | application planning currently describes logical intents but does not yet produce one atomic `ChangeSet` | lower the whole manifest through the universal planner |
| Both | resume | `.jails/app-state-v1` records completed intent keys but does not yet notice a generated file deleted afterward | store output fingerprints and reconcile drift instead of blindly skipping |
| Both | domain behavior | executable creates, typed equality queries, and optimistic transitions remove the first behavior boilerplate; crawler traversal, conversation assignment, durable publication, and richer query semantics remain | domain-event linkage, transactional outbox, pagination/sort, and policy-bearing workflow intents |
| Both | architecture | current scaffold is layer-first unless `--package` flattens the slice | feature-first placement with verified Modulith boundaries |
| Both | security | production JWT identity and explicit `@scope` enforcement now exist; role/permission policy and audit are not generated yet | closed authorization-policy inputs plus generated allowed/denied HTTP integration tests |
| Both | tests | scheduled jobs and Kafka listeners start in broad `@SpringBootTest` contexts; differing contexts start several PostgreSQL containers and unrelated tests produce large broker logs | generated test profile, selective listener startup, and shared container/context conventions |
| Both | JDK | the Kafka configuration test is now mock-free, but generated controller tests still warn that Mockito dynamic self-attachment will stop working on a future JDK | prefer mock-free contracts where practical and generate explicit Maven test-agent configuration for the remaining mock-based tests |
| Both | delivery | image and CI are generated and the local gate builds/inspects both images, but it cannot execute hosted Actions | keep hosted CI as a required repository check |

Do not hide this table by hand-fixing the examples. The purpose of both apps
is to turn repeated friction into evidence-backed improvements to generic
Jails commands.

The precise done/not-done boundary is maintained in
[`ACCEPTANCE.md`](ACCEPTANCE.md); passing the current gate does not waive its
remaining crawler-safety, tenant, durability, delivery, image, or CI checks.
