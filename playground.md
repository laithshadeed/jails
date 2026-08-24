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

## Problems and difficulties

### 1. Spring test compilation is broken

`jails check` compiles all production sources (85 payment, 168 Intercom, 68
crawler), then all three fail while compiling `SecurityConfigTest.java`:

```text
package org.springframework.boot.webmvc.test.autoconfigure does not exist
cannot find symbol: class WebMvcTest
```

The generated Spring Boot 4.1.0 projects have
`spring-boot-starter-test`, but not the split
`spring-boot-starter-webmvc-test` dependency which supplies that package.
This is a generic Jails security-capability/dependency defect and blocks all
Spring tests before container-backed tests can run. It also makes the
generated CI workflow fail (`mvn clean verify`). The generated Dockerfile uses
`-DskipTests`, which still compiles test sources, so the image build is blocked
by the same error; the successful packaging probe above had to use the stronger
`-Dmaven.test.skip=true` workaround.

### 2. `doctor` reports generated reconciliation failures

Each Spring project reports 31 checks with five failures. Three are local
infrastructure failures (Docker/Podman daemon, Compose provider, and no
PostgreSQL on port 5432). Two are generated-output/tooling failures:

- the test-datasource check says 12 payment, 30 Intercom, and 9 crawler
  `@SpringBootTest` classes lack `KafkaTestcontainersConfig`, even after two
  manifest applies;
- the CORS capability check treats the generated explanatory comment as a
  required property and reports it missing.

The suggested `jails add db` cannot match the first message: it concerns a
Kafka test configuration, and the manifest has already reconciled `db`
twice.

### 3. Container-backed proof is unavailable in this sandbox

`jails migrate --check` cannot start PostgreSQL because the Docker command is
a Podman shim whose runtime directory is read-only here:

```text
Failed to obtain podman configuration: set sticky bit on:
chmod /run/user/1000/libpod: read-only file system
```

Therefore migrations, real PostgreSQL/Kafka integration tests, application
startup, and image builds are not proven in this run. This is an environment
limit, separate from the compile defect above.

### 4. Jails prefers `mvnd` without a usable-daemon check or override

The ledger manifest initially failed while adding `format` because `mvnd`
was present but could not write `/home/laith/.m2/mvnd/.../registry.bin`.
Restricting `PATH` so Jails selected ordinary `mvn` fixed it. Jails should
offer an explicit Maven-command override or fall back when `mvnd` cannot
start.

### 5. The generated ledger is not the packaged/default CLI

`java -jar target/ledger-cli.jar` and `jails run -- ...` select the original
`App` dispatcher, whose help only lists `help`. The manifest-generated
`LedgerCli` contains `reconcile`, but it is reachable only by naming its main
class manually. `jails run --no-build -- reconcile` fails with `unknown
command: reconcile`.

Jails needs a default-dispatcher/main-class concept so `generate cli Ledger`
can become the executable selected by packaging and `jails run`.

### 6. The ledger business logic is explicitly unfinished

The successful ledger gate runs 54 tests but skips six. The three matching
strategies contain TODO implementations and their generated tests are
`@Disabled`; the generated sealed ledger errors also contain TODO payloads.
The `reconcile statement.csv` command currently echoes `statement.csv`.
This is honest scaffolding, but it is not a production ledger.

### 7. Generated message consumers still contain TODO behavior

The payment authorisation, Intercom message, and crawler page Kafka listeners
all say `TODO: hand this to the application service that owns the reaction`.
The outbox and Kafka transport are production-shaped, but the consumer-side
business reaction remains user work.

### 8. Minor workflow mistakes and warnings

- The first manifest copy attempt used `../../examples/...` from
  `playground/` and failed; the correct path is `../examples/...`.
- The ledger build emits a Jackson deprecation note, a future Java native
  access warning from SQLite JDBC, and a future `Unsafe` warning from
  Spotless. They do not fail the current build, but should be tracked for JDK
  upgrades.

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
