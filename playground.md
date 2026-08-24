# Jails production playground

Tested on 2026-08-24 with Jails 0.1.0, OpenJDK 26.0.2 (generated release
25), and Maven 3.9.16. The generated applications are preserved under
[`playground/`](playground/).

This is a production-readiness exercise, not a demo. The manifests ask Jails
for persistence, validation, tenant scoping, security, metrics, migrations,
Kafka, transactional delivery, bounded background work, containers, and CI.
"Intercom" below means an Intercom-style support inbox: workspaces, members,
inboxes, contacts, conversations, assignments, messages, and provider
delivery.

## Outcome

| Application | Generated proof | Routes | Main/test code | Gate |
|---|---|---:|---:|---|
| Payments gateway | [`playground/payments-gateway/`](playground/payments-gateway/) | 10 | 2,695 / 1,968 lines | production sources compile; generated tests do not compile |
| Intercom-style inbox | [`playground/intercom/`](playground/intercom/) | 25 | 4,417 / 3,711 lines | production sources compile; generated tests do not compile |
| Web crawler | [`playground/web-crawler/`](playground/web-crawler/) | 18 | 2,426 / 1,542 lines | production sources compile; generated tests do not compile |
| Ledger CLI | [`playground/ledger-cli/`](playground/ledger-cli/) | 0 | 370 / 641 lines | build succeeds: 54 tests, 0 failures, 6 skipped |

All four manifests applied twice without changing any non-`target` file. The
before and after aggregate SHA-256 values were identical:

| Application | Aggregate SHA-256 |
|---|---|
| Payments gateway | `af2ab3f7234123fcc16dcb9400e255bdcd04cde440fb0bd45071cc6bf80569d7` |
| Intercom-style inbox | `0397c576b9dbc383caca84b92092b20a3e8fdfdf5bc1a0129d7e54c962339ab9` |
| Web crawler | `3394fca759030618adb567394ffdfeffbfe0f51a6c78420d2cdf90b4662e7e24` |
| Ledger CLI | `9ae21d049c0e25191f1a24f9083d725d2ca60e486ba57b7936012aea98f8e913` |

The three Spring projects produce executable jars with tests skipped. That is
useful compile evidence, but it is deliberately **not** called a passing
production gate. `jails check` is red for all three.

## What worked and what did not

The following distinction matters:

- **Generation worked.** Jails created all four directory trees, applied every
  manifest intent, generated routes/migrations/tests/CI/container files, and
  repeated the operation without changing file contents.
- **Main-source compilation worked.** The 85 payment, 168 Intercom, and 68
  crawler production Java files compile. They package only when Maven is told
  not to compile tests at all.
- **The ledger build gate worked.** Its jar was produced and Maven reported 54
  tests, 0 failures, and 6 deliberately skipped tests.
- **The Spring production gate did not work.** All three `jails check` runs
  stop at generated test compilation. No Spring test executes.
- **Real infrastructure was not proved.** PostgreSQL, Kafka, Flyway, application
  startup, and container images could not run in this sandbox.
- **End-to-end business behavior was not complete.** The ledger strategies and
  the generated Kafka consumer reactions still contain TODO behavior.

### Problem summary

| ID | Severity | Classification | Affected | Short version |
|---|---|---|---|---|
| P1 | Blocker | Confirmed Jails defect | All Spring apps | Generated `SecurityConfigTest` needs a Boot 4 test dependency that the generated POM omits |
| P2 | High | Confirmed `doctor` failure; runtime effect unverified | All Spring apps | Many broad Spring tests are reported without Kafka Testcontainers wiring, and the suggested fix names the wrong capability |
| P3 | Medium | Confirmed Jails diagnostic defect | All Spring apps | `doctor` mistakes a generated CORS comment for a required property |
| P4 | Blocker for this run | Environment limitation | All Spring apps | The sandbox cannot operate the Podman/Docker runtime, so integration evidence is unavailable |
| P5 | Medium | Jails portability/UX defect exposed by environment | Ledger apply | Jails selects unusable `mvnd` and provides no explicit Maven override or fallback |
| P6 | Blocker | Confirmed Jails packaging defect | Ledger CLI | The jar and `jails run` start `App`, not the generated `LedgerCli` dispatcher |
| P7 | Blocker for production semantics | Intentional scaffolding gap | All four | Generated strategies/listeners do not implement the application-specific reaction |
| P8 | Low | Warnings/workflow | Ledger and this exercise | Future-JDK warnings and one incorrect relative copy path |

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

The first copy attempt used the wrong relative path and failed before copying
anything:

```bash
install -D ../../examples/payments-gateway/.jails/app.toml payments-gateway/.jails/app.toml
```

For each project, inspect and apply the manifest without starting local
services:

```bash
cd playground/payments-gateway
../../target/debug/jails app plan
../../target/debug/jails app apply --no-start

cd ../intercom
../../target/debug/jails app plan
../../target/debug/jails app apply --no-start

cd ../web-crawler
../../target/debug/jails app plan
../../target/debug/jails app apply --no-start

cd ../ledger-cli
../../target/debug/jails app plan
../../target/debug/jails app apply --no-start
```

The ledger apply failed when Jails automatically selected `mvnd`. The retry
below removes `mvnd` from `PATH` while retaining Maven and Java:

```bash
env PATH=/home/laith/.local/share/mise/installs/maven/3.9.16/apache-maven-3.9.16/bin:/home/laith/.local/share/mise/installs/java/openjdk-26.0.2/bin:/usr/bin:/bin ../../target/debug/jails app apply --no-start
```

Hash the generated content, reapply every manifest, and hash it again:

```bash
find payments-gateway -type f -not -path '*/target/*' -print0 | sort -z | xargs -0 sha256sum | sha256sum
find intercom -type f -not -path '*/target/*' -print0 | sort -z | xargs -0 sha256sum | sha256sum
find web-crawler -type f -not -path '*/target/*' -print0 | sort -z | xargs -0 sha256sum | sha256sum
find ledger-cli -type f -not -path '*/target/*' -print0 | sort -z | xargs -0 sha256sum | sha256sum

for app in payments-gateway intercom web-crawler ledger-cli; do
  (cd "$app" && env PATH=/home/laith/.local/share/mise/installs/maven/3.9.16/apache-maven-3.9.16/bin:/home/laith/.local/share/mise/installs/java/openjdk-26.0.2/bin:/usr/bin:/bin ../../target/debug/jails app apply --no-start) || exit 1
done
```

Run Jails' structural/readiness reports in each project:

```bash
../../target/debug/jails doctor
../../target/debug/jails routes --json
../../target/debug/jails stats --json
```

Run the real clean build/test gate in each project:

```bash
env PATH=/home/laith/.local/share/mise/installs/maven/3.9.16/apache-maven-3.9.16/bin:/home/laith/.local/share/mise/installs/java/openjdk-26.0.2/bin:/usr/bin:/bin ../../target/debug/jails check
```

Prove that the three Spring production source sets package independently of
the broken generated test source:

```bash
for app in payments-gateway intercom web-crawler; do
  (cd "$app" && env PATH=/home/laith/.local/share/mise/installs/maven/3.9.16/apache-maven-3.9.16/bin:/home/laith/.local/share/mise/installs/java/openjdk-26.0.2/bin:/usr/bin:/bin mvn -q -Dmaven.test.skip=true package) || exit 1
done
```

Confirm that the generated Dockerfile's weaker `-DskipTests` Maven mode is
still blocked by generated test compilation:

```bash
env PATH=/home/laith/.local/share/mise/installs/maven/3.9.16/apache-maven-3.9.16/bin:/home/laith/.local/share/mise/installs/java/openjdk-26.0.2/bin:/usr/bin:/bin mvn -q -DskipTests package
```

Attempt every Spring migration set against a scratch database:

```bash
for app in payments-gateway intercom web-crawler; do
  (cd "$app" && ../../target/debug/jails migrate --check)
done
```

Exercise the packaged ledger CLI and the dispatcher Jails generated:

```bash
java -jar target/ledger-cli.jar
env PATH=/home/laith/.local/share/mise/installs/maven/3.9.16/apache-maven-3.9.16/bin:/home/laith/.local/share/mise/installs/java/openjdk-26.0.2/bin:/usr/bin:/bin ../../target/debug/jails run --no-build -- reconcile
java -cp target/ledger-cli.jar com.example.ledgercli.cli.LedgerCli
java -cp target/ledger-cli.jar com.example.ledgercli.cli.LedgerCli reconcile statement.csv
```

Audit obvious unfinished generated behavior:

```bash
rg -n --glob '!target/**' --glob '!*.http' 'TODO|@Disabled|UnsupportedOperationException|localhost:|change.?me|example\.com|return null' payments-gateway intercom web-crawler ledger-cli
```

## Detailed problem reports

### P1 — generated Spring tests do not compile

**Classification:** confirmed Jails defect and release blocker.

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

**Classification:** confirmed `doctor` failure; the eventual runtime failure
is not verified because P1 stops test compilation and P4 prevents containers.

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
runtime CORS failure.

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
defect.

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
read-only home directory.

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

**Classification:** confirmed Jails packaging/run-selection defect.

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
  match, persist, report, or reconcile ledger entries.

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

These warnings should be tracked during dependency/JDK upgrades, but none
caused the current ledger build or Spring compile failure.

## Production-readiness verdict

The manifests prove that Jails can generate a large, coherent production
shape with very few commands. They do **not** prove production-grade working
software today. The ledger passes its gate but deliberately contains
unfinished domain behavior and a broken default entry point. The three Spring
systems package their production sources, but their generated test suites do
not compile, and this environment cannot run the container-backed acceptance
layer.

The next highest-value Jails fixes are: add the Boot 4 WebMVC test dependency,
correct test-datasource/CORS doctor reconciliation, make Maven selection
overrideable/fallback-safe, and let a generated CLI become the packaged
default. After those, rerun `doctor`, `migrate --check`, `jails check`, image
builds, and real startup/API smoke tests before calling any Spring application
production-ready.
