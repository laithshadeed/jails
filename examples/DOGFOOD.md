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
```

Results:

- Both manifests compile from untouched generated source with
  `mvn -q -DskipTests package`.
- Both manifests pass `mvn -q verify` against real Testcontainers/PostgreSQL
  and Kafka. The final two-application gate took 157.90 seconds.
- The crawler generated 2 Flyway migrations and the support inbox generated 4;
  both complete generated test suites passed.
- The crawler's typed `PageDiscovered(UUID id, UUID crawlRunId, URI url,
  Instant occurredAt)` event and the inbox's typed
  `MessageReceived(UUID id, UUID workspaceId, UUID conversationId,
  Instant occurredAt)` event both made a real broker round trip. Their
  publishers use the event id as the Kafka key.
- `cargo fmt -- --check` remains red because the existing worktree contains
  broad formatting drift outside this slice. A repository-wide formatter was
  deliberately not run because it would rewrite unrelated work.
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

## Friction ledger

| Application | Step | Manual intervention or weak output | Generic Jails improvement |
|---|---|---|---|
| Both | project creation | `jails new` depends on Initializr/network | versioned `jails new --offline` baseline |
| Both | manifest setup | user copies `.jails/app.toml` manually | `jails app init --manifest <path>` or `new --app <path>` |
| Both | apply | application planning currently describes logical intents but does not yet produce one atomic `ChangeSet` | lower the whole manifest through the universal planner |
| Both | resume | `.jails/app-state-v1` records completed intent keys but does not yet notice a generated file deleted afterward | store output fingerprints and reconcile drift instead of blindly skipping |
| Both | domain behavior | scaffolds and typed transport events provide plumbing, not crawler traversal or conversation assignment behavior | generic `usecase`, `query`, domain-event, and durable `job` intents |
| Both | architecture | current scaffold is layer-first unless `--package` flattens the slice | feature-first placement with verified Modulith boundaries |
| Both | security | `add security` supplies a baseline filter chain, not product identity/tenant policy | policy inputs plus generated authorized/denied integration tests |
| Both | tests | scheduled jobs and Kafka listeners start in broad `@SpringBootTest` contexts; differing contexts start several PostgreSQL containers and unrelated tests produce large broker logs | generated test profile, selective listener startup, and shared container/context conventions |
| Both | JDK | the Kafka configuration test is now mock-free, but generated controller tests still warn that Mockito dynamic self-attachment will stop working on a future JDK | prefer mock-free contracts where practical and generate explicit Maven test-agent configuration for the remaining mock-based tests |
| Both | delivery | image and CI are not capabilities yet | generic `docker` and `ci` capabilities in production profiles |

Do not hide this table by hand-fixing the examples. The purpose of both apps
is to turn repeated friction into evidence-backed improvements to generic
Jails commands.
