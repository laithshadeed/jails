# Jails production playground

Re-run on 2026-08-24 with Jails 0.1.0, OpenJDK 26.0.2 (generated release 25),
Maven 3.9.16, and a working Podman container runtime. The generated
applications are preserved under [`playground/`](playground/).

This is a production-readiness exercise, not a demo. The manifests ask Jails
for persistence, validation, tenant scoping, security, metrics, migrations,
Kafka, transactional delivery, bounded background work, containers, and CI.
"Intercom" below means an Intercom-style support inbox: workspaces, members,
inboxes, contacts, conversations, assignments, messages, and provider
delivery.

**The first run could not operate a container runtime and could not compile a
single generated Spring test.** Everything below is the second run, on the
same machine with a working runtime and with the five defects the first run
named already fixed. It found six more. Each one needed a part of the
exercise the first run never reached — applying migrations, inspecting the
ledger it wrote, and sending the requests Jails generates — and five are
fixed here, each with a test that fails without it.

## Outcome

| Application | Generated proof | Routes | Main/test code | Gate |
|---|---|---:|---:|---|
| Payments gateway | [`playground/payments-gateway/`](playground/payments-gateway/) | 10 | 2,692 / 2,037 lines | `jails check` green: 70 unit + 17 integration tests, 0 failures |
| Intercom-style inbox | [`playground/intercom/`](playground/intercom/) | 25 | 4,409 / 3,890 lines | `jails check` green: 123 unit + 42 integration tests, 0 failures |
| Web crawler | [`playground/web-crawler/`](playground/web-crawler/) | 18 | 2,424 / 1,582 lines | `jails check` green: 63 unit + 11 integration tests, 0 failures |
| Ledger CLI | [`playground/ledger-cli/`](playground/ledger-cli/) | 0 | 370 / 641 lines | `jails check` green: 54 tests, 0 failures, 6 deliberately skipped |

`jails check` is `mvn clean verify`, so the integration tier really ran:
PostgreSQL and Kafka came up under Testcontainers, Flyway applied every
migration, and the repository, query, association, outbox, durable-job and
messaging tests exercised them. Nothing here is a `-Dmaven.test.skip=true`
jar.

All four manifests applied twice without changing any non-`target` file. The
before and after aggregate SHA-256 values were identical:

| Application | Aggregate SHA-256 |
|---|---|
| Payments gateway | `98bca0c733cb85542e15c74daae36f03898fffe509bb059057f94b80373f41f0` |
| Intercom-style inbox | `02cd226012be3276828ef2e80c70e9a0e4ed2c3808b415492761d57420d3d5ad` |
| Web crawler | `151cfc47c85f3869fbaf2f250ff23350fe0882cdf41e252e73a76ec7ca3268dd` |
| Ledger CLI | `645b32f261e0258fed81a4abfe6a3bbb7a99ac7ec8cb5decc053d0215df2194f` |

They are also unchanged by everything that follows: building, testing,
applying migrations and building images leave the generated tree
byte-identical.

## What worked and what did not

- **Generation worked.** Jails created all four directory trees, applied every
  manifest intent, generated routes/migrations/tests/CI/container files, and
  repeated the operation without changing file contents.
- **The production gate worked, for all four.** `mvn clean verify` is green in
  every project, unit and integration tiers both.
- **The generated tests are the evidence.** Application startup
  (`PaymentsGatewayApplicationTests`), health (`ActuatorEndpointsTest`), the
  metrics scrape (`PrometheusScrapeTest`), the rejection of an unauthenticated
  request (`SecurityConfigTest`) and the acceptance of the documented create
  request (`MerchantControllerTest.theDocumentedCreateRequestIsAccepted`) are
  all generated tests that ran in that gate. Thirteen of those create tests
  exist across the three Spring projects and none is `@Disabled`.
- **Real infrastructure ran.** 7, 19 and 4 Flyway migrations applied cleanly to
  a scratch PostgreSQL database from a cold container start; the three
  generated Dockerfiles build (372–374 MB) and run as uid 10001.
- **The ledger entry point worked.** `java -jar target/ledger-cli.jar` and
  `jails run` both reach the generated `LedgerCli` dispatcher and agree.
- **End-to-end business behavior is still not complete.** The ledger
  strategies and the generated Kafka listeners still contain the
  application-specific reaction nobody has written. That is by design; see P7.

### Problem summary

| ID | Severity | Classification | Affected | Short version | Status |
|---|---|---|---|---|---|
| P1 | Blocker | Confirmed Jails defect | All Spring apps | Generated `SecurityConfigTest` needs a Boot 4 test dependency that the generated POM omits | **fixed; the gate is green** |
| P2 | High | Confirmed `doctor` failure | All Spring apps | Broad Spring tests reported without Kafka Testcontainers wiring, and the fix named the wrong capability | **fixed; `doctor` is silent on it** |
| P3 | Medium | Confirmed Jails diagnostic defect | All Spring apps | `doctor` mistakes a generated CORS comment for a required property | **fixed; `doctor` is silent on it** |
| P4 | Blocker for the first run | Environment limitation | All Spring apps | The sandbox could not operate the container runtime, so integration evidence was unavailable | **closed — every layer it blocked has now run** |
| P5 | Medium | Jails portability/UX defect | Ledger apply | Jails selects unusable `mvnd` and offers no Maven override | **fixed; `mvnd` is on `PATH` and every apply succeeded** |
| P6 | Blocker | Confirmed Jails packaging defect | Ledger CLI | The jar and `jails run` start `App`, not the generated `LedgerCli` | **fixed and confirmed** |
| P7 | Blocker for production semantics | Intentional scaffolding gap | All four | Generated strategies/listeners do not implement the application-specific reaction | open — by design |
| P8 | Low | Warnings/workflow | Ledger and this exercise | Future-JDK warnings and one incorrect relative copy path | open — upstream |
| P9 | High | Confirmed Jails defect, new | Spring apps with a database | `migrate --check` starts PostgreSQL and connects before it is listening | **fixed** |
| P10 | Medium | Confirmed Jails defect, new | Manifests naming a suffixed entity | One entity gets two ledger rows, and `doctor` offers to adopt the empty one | **fixed** |
| P11 | High | Confirmed Jails defect, new | Any timestamped scaffold | The documented create request demands `createdAt`/`updatedAt` and answers 400 | **fixed** |
| P12 | Medium | Confirmed Jails defect, new | Scoped scaffolds | The request collection offers a `GET` a create-only controller answers 405 | **fixed** |
| P13 | High | Confirmed Jails defect, new | Scaffolds using `uri`, `currency`, `zone-id`, `duration` or `bytes` | The documented request sends `null` for a required component, and answers 400 | **fixed** |
| P14 | Low | Transitional, not a defect | All four | `doctor` reports every entity of a freshly generated project as adoptable | open — closes with plan.md §R6 step 9 |
| P15 | High | Confirmed Jails defect, new | Every scaffold with a `@unique` column | A duplicate key is a 500, though the generated API vocabulary has a 409 | open — the fix is a capability-ordering question |

### What was fixed, and how each is held

Every fix carries a test that fails without it. The numbers in the tables
above were measured **after** all of them, against the binary at this commit.

- **P1** — `spring-boot-starter-webmvc-test` is spliced from the write path,
  keyed off the emitted bytes, for the same reason AssertJ and Failsafe are.
  `@WebMvcTest`'s import is version-sniffed, so a Boot 3 project gets the
  package it has and no dependency it does not need. The reason no test caught
  this was worse than the defect: the Spring test fixture declared the module
  and `jails new` does not, so every real-toolchain test compiled against a POM
  the tool never produces. The fixture matches `new` now.
- **P2** — the `test datasource` check discriminates on the container's *type*.
  The invariant it exists for is specific to JDBC: once
  `spring-boot-starter-jdbc` is present, auto-configuration demands a
  `DataSource` for every `@SpringBootTest`, including ones that never touch a
  database. A broker has no equivalent demand. Three copies of "walk
  `src/test/java` for annotated classes" existed, two of them matching raw
  bytes — which is also why the scan read the `@SpringBootTest` in
  `TestcontainersConfig`'s own Javadoc as a declaration. There is one reader.
- **P3** — a capability's property block is prose *and* settings, and only the
  settings are keys.
- **P5** — `JAILS_MAVEN` names the Maven command and overrides every rule.
  Jails also declines to pick `mvnd` when its registry directory is not
  writable, because that failure happens *before* Maven runs and is
  indistinguishable from a failing build at the call site — a blind retry there
  would re-run a genuinely broken build. In this run `mvnd` is on `PATH` and
  every apply completed with no override.
- **P6** — `jails run` resolves the POM's `<mainClass>`, so it and `java -jar`
  agree by construction. `generate cli` moves that entry point onto the new
  dispatcher, but only off a stub jails wrote that has no command registered in
  it: once `App` dispatches something, it is the project's real CLI.
- **P9** — `migrate --check` waits for the database it started. `compose up`
  returns when the container is *running*, several seconds before PostgreSQL
  accepts a connection. The retry loop takes its probe as a parameter, so what
  is worth pinning — stops at the first success, reports the *last* failure,
  does not sleep after the final attempt — is testable without a database.
- **P10** — a manifest intent is keyed by the name `generate` records it under.
  The duplicate-identity gate uses the same name, so `fetcher Acquirer` and
  `fetcher AcquirerFetcher` in one manifest are refused as the one entity they
  generate into rather than accepted as two and applied over each other.
- **P11, P12 and P13 are one gate.** The scaffold now generates a controller
  test that POSTs the exact body `requests/<name>.http` documents, from the
  same builder, and asserts 201. All three defects are a collection describing
  a request the record refuses, and none of them could survive that test. What
  each needed on its own:
  - **P11** — the request record drops the audit pair and `toDomain` supplies
    one `Instant now` for both. Recognised as a **pair**, never a lone
    component: `--timestamps` refuses to expand over a hand-declared
    `createdAt`, so one on its own was written by hand and means data.
  - **P12** — the `### List` block is emitted only for unscoped scaffolds,
    whose controller does serve `findAll`. The generated controller test
    already asserted that a scoped resource answers 405 there; the test knew
    and the collection did not.
  - **P13** — `URI`, `Currency`, `ZoneId`, `Duration` and `byte[]` have wire
    samples, checked against Jackson's defaults. Anything still unsampleable
    disables the generated test naming the type, rather than shipping one that
    fails on every build — jails' existing rule for a test it cannot fully
    write.

## Commands run

Build the Jails binary used by the exercise:

```bash
cargo build --workspace
```

Create the four clean project baselines from `playground/`:

```bash
mkdir -p playground
cd playground
../target/debug/jails new payments-gateway --offline --no-git
../target/debug/jails new intercom --offline --no-git
../target/debug/jails new web-crawler --offline --no-git
../target/debug/jails new-cli ledger-cli --no-git
```

Copy the production manifests into the projects:

```bash
install -D ../examples/payments-gateway/.jails/app.toml payments-gateway/.jails/app.toml
install -D ../examples/support-inbox/.jails/app.toml intercom/.jails/app.toml
install -D ../examples/web-crawler/.jails/app.toml web-crawler/.jails/app.toml
install -D ../examples/ledger-cli/.jails/app.toml ledger-cli/.jails/app.toml
```

For each project, inspect and apply the manifest without starting local
services. Every apply below ran on the ordinary `PATH`, with `mvnd` on it:

```bash
for app in payments-gateway intercom web-crawler ledger-cli; do
  (cd "$app" && ../../target/debug/jails app plan && ../../target/debug/jails app apply --no-start) || exit 1
done
```

Hash the generated content, reapply every manifest, and hash it again:

```bash
for app in payments-gateway intercom web-crawler ledger-cli; do
  find "$app" -type f -not -path '*/target/*' -print0 | sort -z | xargs -0 sha256sum | sha256sum
done

for app in payments-gateway intercom web-crawler ledger-cli; do
  (cd "$app" && ../../target/debug/jails app apply --no-start) || exit 1
done
```

Run Jails' structural/readiness reports in each project:

```bash
../../target/debug/jails doctor
../../target/debug/jails routes --json
../../target/debug/jails stats --json
```

Run the real clean build/test gate in each project. `mvnd` is unreliable on
this machine under JDK 26 for reasons unrelated to Jails, so the gate is
pinned to plain Maven the same way the repository's own real-toolchain tests
are:

```bash
env PATH=/home/laith/.local/share/mise/installs/maven/3.9.16/apache-maven-3.9.16/bin:/home/laith/.local/share/mise/installs/java/openjdk-26.0.2/bin:/usr/bin:/bin ../../target/debug/jails check
```

Apply every Spring migration set to a scratch database, from a **cold** start
each time — the container is removed first, so the service is created, not
merely restarted:

```bash
for app in payments-gateway intercom web-crawler; do
  (cd "$app" && docker compose -f compose.yaml down -v && ../../target/debug/jails migrate --check)
done
```

Build the generated container images and check what they run as:

```bash
for app in payments-gateway intercom web-crawler; do
  (cd "$app" && docker build -t "jails-playground-$app:test" .) || exit 1
  docker run --rm --entrypoint id "jails-playground-$app:test"
done
```

There is deliberately no `curl` step. Application startup, health, the metrics
scrape, the rejection of an unauthenticated request and the acceptance of the
documented create request are all generated tests, and they ran in the gate
above. An exercise whose evidence is a shell session proves the tool worked
once, on this machine, for whoever ran it; a generated test proves it on every
build of every project. The create test exists *because* this defect was first
found by hand — see P11.

Exercise the packaged ledger CLI and the dispatcher Jails generated:

```bash
java -jar target/ledger-cli.jar
../../target/debug/jails run --no-build -- reconcile
java -cp target/ledger-cli.jar com.example.ledgercli.cli.LedgerCli reconcile statement.csv
```

Audit obvious unfinished generated behavior:

```bash
rg -n --glob '!target/**' --glob '!*.http' 'TODO|@Disabled|UnsupportedOperationException|localhost:|change.?me|example\.com|return null' payments-gateway intercom web-crawler ledger-cli
```

## Detailed problem reports

### P1 — generated Spring tests do not compile

**Classification:** confirmed Jails defect and release blocker. **Fixed, and
confirmed by the re-run:** all three `jails check` runs are green, with 70,
123 and 63 unit tests and 17, 42 and 11 integration tests executing. The
dependency is spliced from the write path and the Spring test fixture no
longer supplies it for jails; see the fix summary above.

Everything below describes the first run and is kept as the record of what
the defect looked like.

**Affected:** payments gateway, Intercom-style inbox, and web crawler.

**Reproduction:** run `jails check` in any of the three Spring projects.

**Expected:** generated production and test sources compile, then unit and
integration tests run.

**Actual:** Maven successfully compiles every production source and then stops
while compiling the generated `SecurityConfigTest.java`:

```text
package org.springframework.boot.webmvc.test.autoconfigure does not exist
cannot find symbol: class WebMvcTest
```

The generated test imports
`org.springframework.boot.webmvc.test.autoconfigure.WebMvcTest`; for example,
see
[`SecurityConfigTest.java`](playground/payments-gateway/src/test/java/com/example/paymentsgateway/SecurityConfigTest.java).
The generated Spring Boot 4.1.0 POM contains
`spring-boot-starter-test`, but does not contain the split
`spring-boot-starter-webmvc-test` artifact that provides that package; see
[`pom.xml`](playground/payments-gateway/pom.xml).

**Production impact:**

- no generated Spring test executes, including security, repository, Kafka,
  outbox, migration, workflow, and HTTP tests;
- the generated CI job runs `mvn clean verify`, so CI fails;
- the generated Dockerfile runs `mvn -DskipTests package`. `-DskipTests`
  suppresses test execution but still compiles test sources, so the image
  build fails too;
- only `-Dmaven.test.skip=true package`, which skips both compilation and
  execution of tests, produced jars. Those jars are compile evidence, not
  release evidence.

**Likely Jails fix:** when the security capability generates a Boot 4
`@WebMvcTest`, add `org.springframework.boot:spring-boot-starter-webmvc-test`
with test scope, or ensure the offline project baseline supplies the complete
test slice. Add a regression test which creates a fresh offline Spring app,
adds security, and runs `mvn clean test`.

### P2 — Testcontainers wiring reported incomplete after reconciliation

**Classification:** confirmed `doctor` failure. **Fixed, and confirmed by the
re-run:** across all four projects `doctor` now emits no warning of this kind
— or of any kind other than P14's transitional one — and the broad Spring
tests it named all pass against real containers. The check discriminates on
the container type, so a broker config is not read as the project's datasource
config.

**Affected:** all three Spring projects.

**Expected:** after `app apply --no-start` and its reconciliation pass,
`jails doctor` should either report complete generated wiring or give an
accurate command that adds the missing wiring.

**Actual:** after applying every manifest twice with identical output hashes,
`doctor` reports that these `@SpringBootTest` classes do not import
`KafkaTestcontainersConfig`:

- payments gateway: 12 classes;
- Intercom-style inbox: 30 classes;
- web crawler: 9 classes.

The check is labelled **test datasource**, but the missing type it names is
the Kafka configuration. Its proposed fix is `jails add db`, even though the
database capability is already installed and reconciled. That message mixes
database and Kafka responsibilities, so a user cannot confidently tell
whether the generator or the diagnostic is wrong.

**Production impact:** `doctor` cannot become green on Jails' own untouched
output. If those broad Spring contexts really start Kafka listeners, their
tests may also try to reach `localhost:9092` instead of a Testcontainers
broker. That runtime consequence remains unproved in this run.

**Likely Jails fix:** split the database and Kafka checks; inspect which
auto-configurations each test actually activates; generate only the necessary
`@Import` members; and make the remediation name the capability responsible
for the missing type. Pin a second-apply test so reconciliation itself must
make `doctor` green.

### P3 — CORS capability is present but `doctor` says it is missing

**Classification:** confirmed Jails diagnostic false-positive, not an observed
runtime CORS failure. **Fixed, and confirmed by the re-run:** `capability cors`
reports `everything it installs is present`, and the generated `CorsConfigTest`
passes. Comment lines in a capability's property block are prose, not required
keys.

**Affected:** all three Spring projects.

The generated properties contain both lines:

```properties
# Exact browser origins; never use `*` together with credentials.
app.cors.allowed-origins=http://localhost:3000
```

Nevertheless `doctor` reports:

```text
FAIL capability cors  1 missing: property # Exact browser origins; never use `*` together with credentials.
```

The wording shows that the verifier is treating Jails' explanatory comment as
if it were a required property key.

**Production impact:** the generated runtime property exists, but readiness
automation and deployment gates that trust `doctor` remain red. Re-running
`app apply` does not fix it.

**Likely Jails fix:** ignore blank/comment lines when deriving required
properties from a capability recipe, and test the generated CORS block against
the same parser used by `doctor`.

### P4 — container-backed acceptance could not run here

**Classification:** environment limitation, not evidence of a Jails code
defect. **Closed by the re-run.** Every layer it blocked has now run on this
machine:

| What it prevented | Now |
|---|---|
| applying every Flyway migration to PostgreSQL | 7, 19 and 4 migrations applied cleanly from a cold container start |
| repository/query/association integration tests | executing in `jails check` — 17, 42 and 11 integration tests, 0 failures |
| Kafka publish/consume and transactional-outbox tests | executing; `PaymentAuthorisedMessagingIT` and `AuthorisePaymentOutboxIT` pass |
| durable-job and crawler-workflow recovery tests | executing; `SettlementDispatcherJobIT` passes |
| real application startup and API smoke tests | generated tests: `PaymentsGatewayApplicationTests`, `ActuatorEndpointsTest`, `PrometheusScrapeTest`, `SecurityConfigTest`, and the create test written for P11 |
| OCI image build and non-root runtime inspection | three images build (372–374 MB) and run as uid 10001 |

The container runtime is still Podman behind a `docker` shim, and `doctor`
reports its Compose provider as drivable. What changed is the machine, not
jails: nothing in the tool was altered for P4.

The first run's report follows.

**Affected:** all Spring integration evidence.

`doctor` reports that the Docker daemon is unavailable, its Compose provider
does not support the required Compose v2 invocation, and PostgreSQL is not
listening on port 5432. `jails migrate --check` reaches the installed Docker
command, which is a Podman shim, and fails with:

```text
Failed to obtain podman configuration: set sticky bit on:
chmod /run/user/1000/libpod: read-only file system
```

**What this prevented:**

- applying every Flyway migration to PostgreSQL;
- repository/query/association integration tests;
- Kafka publish/consume and transactional-outbox tests;
- durable-job and crawler-workflow recovery tests;
- real application startup and API smoke tests;
- OCI image build and non-root runtime inspection.

**Required follow-up:** after P1 is fixed, rerun on a host with a responding
Docker-compatible daemon, a Compose v2 provider, and writable container runtime
directories. The exact commands are `jails migrate --check`, `jails check`,
the generated Docker build, and application/API smoke tests. Until that occurs,
the database and broker behavior is **unverified**, not passing or failing.

### P5 — Jails chooses an unusable `mvnd` and does not fall back

**Classification:** Jails portability/UX defect exposed by the sandbox's
read-only home directory. **Fixed.** `JAILS_MAVEN` overrides the choice, and
mvnd is not selected when its registry directory is unwritable.

**Affected:** ledger manifest application while reconciling the `format`
capability. The same selection could affect any Maven-backed Jails command.

**Expected:** use a project wrapper when present; otherwise allow the user to
choose Maven, or fall back from an unusable daemon to ordinary `mvn`.

**Actual:** because `mvnd` existed on `PATH`, Jails selected it. `mvnd` then
failed before Maven ran because it could not write:

```text
/home/laith/.m2/mvnd/registry/1.0.6/registry.bin: Read-only file system
```

The manifest stopped after generating files but before completing format
reconciliation. There is no documented Jails flag in this workflow to select
`mvn`. Removing `mvnd` from the command's `PATH` allowed the same apply to
resume and finish successfully.

**Production impact:** generation depends on whichever Maven executable happens
to appear first on a machine, and a failed daemon can leave an interrupted
apply which must be resumed.

**Likely Jails fix:** support a stable Maven-command setting/flag and, when
auto-selecting `mvnd`, probe it before use or retry ordinary `mvn` when daemon
initialisation fails.

### P6 — the intended ledger command is not the executable entry point

**Classification:** confirmed Jails packaging/run-selection defect. **Fixed.**
`jails run` reads the POM's `<mainClass>`, and `generate cli` moves it onto
the new dispatcher when the old one is an unused stub.

**Expected:** the manifest's generated `LedgerCli` dispatcher, containing the
generated `reconcile` command, should be what `java -jar` and `jails run`
execute.

**Actual:** [`pom.xml`](playground/ledger-cli/pom.xml) fixes the jar main class
to `com.example.ledgercli.App`. Consequently:

```text
$ java -jar target/ledger-cli.jar
usage: ledger-cli <command> [args]
commands:
  help

$ jails run --no-build -- reconcile
unknown command: reconcile
```

The generated dispatcher itself works only when invoked by its class name:

```text
$ java -cp target/ledger-cli.jar com.example.ledgercli.cli.LedgerCli
usage: ledger <command> [args]
commands:
  help
  reconcile
```

**Production impact:** the produced jar does not expose the application the
manifest describes. A deployment or shell user naturally invoking the jar
cannot reach `reconcile`.

**Likely Jails fix:** record a default dispatcher in project metadata, let the
manifest designate it explicitly, update the Maven main class during
generation, and make `jails run` resolve the same entry point as the packaged
jar.

### P7 — generated application-specific behavior remains unfinished

**Classification:** intentional scaffolding gap, but a blocker for the user's
production-grade goal.

**Ledger evidence:**

- `ExactReferenceMatchRule`, `AmountAndDateMatchRule`, and
  `FuzzyMemoMatchRule` contain TODO decision bodies;
- each strategy has two `@Disabled` tests, producing the six skipped tests in
  the otherwise successful gate;
- `LedgerError` variants contain TODO payload descriptions;
- `reconcile statement.csv` currently prints `statement.csv`; it does not read,
  match, persist, report, or reconcile ledger entries. Still true in the
  re-run.

**Spring evidence:**

- `PaymentAuthorisedListener` contains `TODO: hand this to the application
  service that owns the reaction`;
- `MessageReceivedListener` contains the same TODO;
- `PageDiscoveredListener` contains the same TODO.

Jails did generate useful production mechanisms around these points: typed
events, Kafka configuration, transactional outboxes, persistent jobs, bounded
fetching, repositories, migrations, HTTP endpoints, security, metrics, CI, and
containers. What is missing is the application-specific decision or reaction.

**Production impact:** passing compilation would still not mean the requested
business systems work end to end. The ledger does not reconcile; received
events do not drive downstream application behavior.

**Required follow-up:** either implement these decisions as user-owned domain
code, or extend Jails' manifests/generators with enough declarative semantics
to generate and test them. Remove disabled tests only by replacing them with
executable behavior and assertions.

### P8 — non-blocking workflow mistakes and upgrade warnings

**Classification:** low severity; does not explain any main gate failure.

- The first manifest-copy command used `../../examples/...` from
  `playground/`. It was an operator path mistake; `../examples/...` succeeded.
- Jackson-generated test code emits a deprecation note.
- SQLite JDBC warns that future Java releases will require explicit native
  access.
- Spotless warns about an internal `Unsafe` call that a future JDK will remove.

These warnings should be tracked during dependency/JDK upgrades. None of them
fails a gate in the re-run.

### P9 — `migrate --check` connects before the database it started is listening

**Classification:** confirmed Jails defect, found by this run. **Fixed.**

**Affected:** any Spring project with a database, on a cold container start.

**Reproduction:** remove the PostgreSQL container (`docker compose down -v`)
and run `jails migrate --check`.

**Actual:** 2 of 2 cold starts failed immediately:

```text
 Container intercom-postgres-1 Started
jails: could not create the scratch database: psql: error: connection to server
at "localhost" (::1), port 5432 failed: server closed the connection unexpectedly
```

Inserting a `sleep 8` between the start and the check made 3 of 3 succeed,
which is what identified the race rather than a broken database.

**Why it matters more than the delay it costs:** the message names the server
closing a connection, so it reads like a database or a migration problem. It
sends the reader to the migrations, which are fine. `compose up` returns when
the container is *running*; PostgreSQL accepts connections some seconds later.

**Fix:** a bounded readiness poll — `select 1`, 250 ms apart, for thirty
seconds — before the scratch database is created, and only when jails started
the service itself. Under `--no-start` the caller has asserted the database is
up, and half a minute spent polling a port with nothing behind it is a worse
answer than the connection error. Cold-start runs now take 11 s and pass: 7,
19 and 4 migrations applied cleanly.

### P10 — one entity, two ledger rows, when a name carries its kind's suffix

**Classification:** confirmed Jails defect, found by this run. **Fixed.**

**Affected:** any manifest whose intent name already ends in its kind's
suffix. Both playground manifests that name a fetcher were affected.

**Actual:** `.jails/ledger.toml` held two `[[applied]]` rows for one intent:

```toml
[[applied]]
recipe = "fetcher"
name = "Acquirer"          # generate wrote the files here
has_spec = false
files = [ ... four files ... ]

[[applied]]
recipe = "fetcher"
name = "AcquirerFetcher"   # app apply wrote the spec here
has_spec = true
```

`generate` normalises a name before it writes — stripping a suffix the kind
already implies — and records its files under the result. `app apply` recorded
the manifest's spec under the manifest's spelling. Identity is
`(recipe, name, package)`, so the two writers keyed one entity two ways.

**Production impact:** `doctor` reported the empty half as an entity with
"0 file(s) ... and no recorded owner" and offered an `adopt` command that would
adopt nothing. The half holding the files carried no spec, so `app apply` could
not three-way merge an edit to that intent.

**Fix:** one `recorded_name` function, used by `generate` and by `app apply`'s
identity, exempting `cases` and `migration` the way `generate` already does.
The duplicate-identity gate uses it too. The V2 route was never wrong here; it
normalises at the boundary already.

### P11 — the documented create path demands the columns it says it supplies

**Classification:** confirmed Jails defect. **Fixed.**

**Affected:** every scaffold generated with `timestamps = true`.

**Actual:** sending the `### Create Merchant` request from
`requests/merchant.http`:

```text
HTTP 400
{"detail":"the request has invalid fields","status":400,
 "fields":{"createdAt":"must not be null","updatedAt":"must not be null"}}
```

`jails g scaffold --help` says `--timestamps` adds conventional `createdAt`
and `updatedAt` components and that **"the generated create path supplies
both"**. It did not. The flag expands into two ordinary components before any
recipe sees it — right for the record, the DDL and the response, wrong for the
one artifact that describes what a *caller* may send — so they arrived as
`@NotNull` wire components.

**Production impact:** the documented create request fails on every timestamped
resource, and a caller who does satisfy the validation can backdate a row.

**Why no test saw it:** no golden scenario passes `--timestamps`, and no
generated test sent a create request at all.

**Fix:** the request record drops the pair and `toDomain` supplies one
`Instant now` for both, so a freshly created row does not look already edited.
Recognised as a **pair** and never as a lone component, because `--timestamps`
refuses to expand over a hand-declared `createdAt` — one on its own was
written by hand and is data the caller sends.

### P12 — a scoped scaffold documents a route its controller never serves

**Classification:** confirmed Jails defect. **Fixed.**

**Affected:** scoped scaffolds — every resource in the payments manifest.

**Actual:** `requests/<name>.http` ended with

```http
### List Merchant
GET {{baseUrl}}/merchants
Accept: application/json
```

A scoped resource's controller is create-only: every read has to carry the
tenant, so it is a `jails g query`. That request answers `405 Method Not
Allowed`, and **the generated controller test already asserted exactly that**
— `broadUnscopedReadsAreNotExposed` has pinned the 405 since the scoped
controller was written. The test knew and the collection did not.

**Fix:** the block is emitted only for unscoped scaffolds, whose controller
does serve `findAll`. The first attempt removed it from both and was caught by
the golden suite plus a new scoped-scaffold integration test — there was no
scoped scaffold among the golden scenarios, which is the coverage gap that let
this and P11 through.

### P13 — the documented request sends `null` for a required component

**Classification:** confirmed Jails defect. **Fixed.**

**Affected:** any scaffold with a required component of type `uri`,
`currency`, `zone-id`, `duration` or `bytes` — the web crawler declares
`seedUrl:uri`, so this was live in a shipped example manifest.

**Actual:** the sample-body builder had cases for `String`, the numerics,
`Boolean`, `UUID`, `LocalDate`, `LocalDateTime`, `Instant` and project enums,
and wrote `null` for everything else. The request record declares
`@NotNull URI seedUrl`, so the documented body is a 400.

**How it was found:** by the test written for P11. It was added, the suite was
run, and two of the crawler's three controller tests failed with
`expected: 201 but was: 400` — a defect that had been in every generated
crawler and was invisible to a `curl` of the payments gateway, which happens
to declare no such type.

**Fix:** those five types have wire samples, checked against Jackson's
defaults. Anything still unsampleable writes `null` in the collection, where a
reader replaces it, and disables the generated test naming the type — jails'
existing rule for a test it cannot fully write, rather than shipping one that
fails on every build.

### P14 — every entity of a brand-new project is reported as adoptable

**Classification:** transitional state, not a defect. **Open, and closes with
plan.md §R6 step 9.**

**Affected:** all four projects. Every one of the 77 `doctor` warnings across
them is this one message, and there is no other warning of any kind:

```text
warn  scaffold Merchant   18 file(s) from a schema-1 ledger, with no recorded
                          owner -- so `destroy` and `sync` cannot act on them
      fix: jails adopt --legacy-key schema1-applied:09ecc87e... --intent scaffold:Merchant
```

The advice is accurate: those rows genuinely have no V2 owner. It is noise only
because `main.rs` still dispatches the V1 write path, so *every* project the
current binary creates is a schema-1 project — including one created five
seconds earlier. The moment dispatch flips, a freshly created project is
schema-2 and this fires only on projects that really predate the migration,
which is what it is for.

Nothing was changed for it. Suppressing a correct report to make a transitional
state look tidy is how a check stops being believed.

### P15 — a `@unique` violation is a 500, though the generated vocabulary has a 409

**Classification:** confirmed Jails defect. **Open**, with the reason it is not
a one-line fix recorded below.

**Affected:** every scaffold with a `@unique` column, which is every resource
the three manifests declare.

**Reproduction:** create a resource, then create another with the same value
in its `@unique` column.

**Actual:** `HTTP 500`, with
`org.springframework.dao.DuplicateKeyException: ... duplicate key value
violates unique constraint "merchants_reference_key"`.

**Expected:** `409 Conflict`. The generated code already has the word for it —
`add api` writes a sealed `ApiException` whose `Conflict` variant is documented
"Becomes a 409" — and jails is the thing that put the constraint in the schema,
from `reference:string!@unique` in the manifest. It knows the constraint exists
and it knows the status that describes violating it; nothing connects the two.

**Production impact:** a caller retrying a create, or racing another caller, is
told the server broke rather than that the request conflicts. 5xx is what
alerting pages on and what clients retry, so a duplicate becomes an incident
and then a retry storm.

**Why it is not a one-line handler:** `DuplicateKeyException` lives in
`org.springframework.dao`, which arrives with the JDBC stack, and
`ApiExceptionHandler` is written by `add api`, which does not require a
database. Adding the arm unconditionally hands a project with `api` and no `db`
a compile error for a file it did not write — the exact failure
`generate::report_degraded_shape` and the versionless-dependency rule exist to
prevent. The shape a fix needs is a conditional arm plus a reconciliation pass
that revisits `api` after `db` lands, which `app apply` already performs twice
for this reason but `jails add api` followed by `jails add db` does not.

That is a capability-ordering design question, not a patch, so it belongs in
`plan.md`. The generated controller test written for P11 is where its
assertion goes once it is answered.

**Note on the response body:** during `jails run` that 500 carries a full stack
trace, including the SQL and the offending key. That is `spring-boot-devtools`
setting `server.error.include-stacktrace=always`; devtools is excluded from the
repackaged jar, so it is not what the built image returns. It is worth knowing
that the dev-time response is that verbose.

## Production-readiness verdict

The manifests prove that Jails can generate a large, coherent production shape
with very few commands, and — unlike the first run of this exercise — that the
shape it generates passes its own gate. `mvn clean verify` is green in all four
projects, with PostgreSQL and Kafka really started, every migration really
applied, and the integration tier really executed. The generated Dockerfiles
build and run unprivileged.

What that still does **not** establish is working business software. The
generated strategies and Kafka listeners contain the application-specific
reaction nobody has written (P7), so the ledger does not reconcile and a
received event drives nothing downstream. That gap is deliberate and is the
honest boundary of a scaffolding tool; it is not a defect, and it is also not
a product. Neither is a duplicate key answering 500 (P15).

Ten defects have now been confirmed and fixed across the two runs — P1, P2,
P3, P5, P6, P9, P10, P11, P12 and P13 — each with a test that fails without it.
Five of those are new here, and every one needed a part of the exercise the
first run could not reach:

- P9 needed a database that starts;
- P10 needed a manifest applied and then inspected rather than trusted;
- P11 needed the generated request sent at a running application;
- P12 needed someone to notice what the *second* request in that collection
  answers;
- P13 needed the test written for P11 to be run against a different manifest.

The last one is the argument for this document's method, and for the one
change of method it forced. P11 was found by hand, with `curl`. That is a
weak gate: it proves the tool worked once, on one machine, for one resource
shape. Turning it into a generated test — the scaffold now sends its own
documented request on every build — immediately found P13 in a manifest nobody
had thought to curl, and would have caught P12 as well. **The exercise's real
output is not the four applications; it is the tests the four applications
made it obvious to generate.**

The next thing this exercise cannot answer is P7 — whether the declarative
manifest can be extended far enough to generate the decisions themselves, or
whether that is properly the reader's code. `plan.md` is where that belongs,
along with P15's capability-ordering question.
