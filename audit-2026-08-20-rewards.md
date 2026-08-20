# What a real project found wrong with jails' output

Audit of `~/code/bank/rewards` — a project bootstrapped with `jails new` +
`jails add db json testkit format` + `jails add kafka` — against jails' own
`java.md`, `spring.md`, `backend.md` and the upstream checkouts in `deps/`.
Every finding is something jails generated, and every one is checkable.

**Status: living document.** Originally written 2026-08-20 as a point-in-time
audit. Re-verified against the source and updated the same day, in two passes:
the first fixed eleven findings, the second closed the rest. **Every finding
below is now ✅ fixed.** Each carries its state:

| | meaning |
|---|---|
| ✅ **fixed** | changed in jails, with a test that fails if it regresses |
| 🔸 **open** | still true; not yet done |
| ⛔ **won't fix** | deliberate, with the reason recorded |

The headline of the original audit was: **jails' documentation is better than
jails' templates** — four findings were violations of rules written in
`spring.md` and `backend.md`, so the persona would refuse code the generator
emitted. That is now closed in both directions: the templates were fixed, and
where the *documentation* was the thing that did not survive contact with a
real project (§16), it was corrected instead.

---

## 1. ✅ `jails add db` produced a Flyway that never ran

`pom.xml` got `flyway-core` and `flyway-database-postgresql`. Boot 4 split
auto-configuration into ~130 modules, and Flyway's lives in
**`org.springframework.boot:spring-boot-flyway`**, which was absent. There is no
Flyway class in `spring-boot-autoconfigure-4.1.0.jar` — I checked the jar.

The failure mode is the expensive kind: no error, no warning, **no Flyway log
line at all**, and then `relation "reward" does not exist` from the first query.
`jails why` blamed the missing test datasource; `jails doctor` reported
`ok migrations  1 migration(s) in src/main/resources/db/migration` — it counted
the files without ever asking whether anything would run them.

**The general rule, not just the Flyway fix:** in Boot 4 the technology jar and
the auto-configuration jar are different artifacts. Every capability that
integrates with Boot needs its `spring-boot-<tech>` module asserted, not just
the library.

**Fixed:** `SPRING_BOOT_FLYWAY` is in `db_plan`'s Spring dependency list
(`src/add.rs`). `doctor`'s migrations check now asks *"will these run"* rather
than *"do these exist"*: Spring + `flyway-core` + no `spring-boot-flyway` is a
`FAIL` naming the module. The other capabilities were audited at the same
time — `kafka`, `cache`, `security`, `actuator` and `observability` all go
through a `spring-boot-starter-*`, which carries its own auto-configuration, so
Flyway was the only bare-library case.

## 2. ✅ The generated migration was not valid SQL, and nothing checked

`jails generate migration` emitted:

```sql
create table reward (
  ...
  cureated_at     timestampz  not null default now(),   -- two errors on one line
  ...
)                                                        -- no semicolon

create index reward_customer_history ...                 -- no semicolon
```

`timestampz` is not a type, `cureated_at` is a typo the index below it
contradicts, and the missing semicolons make the file one unparseable statement.
None of it mattered while finding 1 was live, because nothing ever parsed the
file. Two bugs hid each other.

**Fixed in three parts.**

The DDL is derived from the field spec through `src/sql.rs`, where
`timestamptz` is spelled once and the column list comes from — so the typo
class is gone by construction (finding 3 is the same fix). Every statement is
now terminated, which was the other half of the original bug:
`every_generated_statement_is_terminated` counts the `create`s and the `;`s.

**The field spec can now express a constraint.** `@pk` (composite by
repetition), `@unique`, `@index`, `@positive` and `@nonnegative` parse off the
type into `Field::constraints`, and `--index` carries the composite or ordered
index a per-column marker cannot spell. This reproduces `rewards`'
hand-written `V001__rewards.sql` exactly:

```
jails g scaffold Reward transactionId:uuid@pk ruleId:string@pk \
    customerId:uuid amount:long@positive currency:string createdAt:instant \
    --index 'customer_id, created_at desc, transaction_id desc'
```

Two design rules recorded in `CLAUDE.md`: an unknown marker is an **error**
listing the real ones (a typo meaning "no constraint" reintroduces exactly the
failure this removes), and there is deliberately **no** `@check(arbitrary
sql)` — a passthrough jails cannot validate fails at `flyway migrate`, which
is the slow, remote failure the field spec exists to prevent.

**Migrations are now checkable.** `jails migrate` applies them in Flyway's
order to a scratch database created and dropped around the run, and reports
the first failure with psql's file and line. Run against the original bug it
prints exactly what was missing for weeks:

```
  FAIL  V001__rewards.sql

  psql:<stdin>:13: ERROR:  type "timestampz" does not exist
  LINE 8:   created_at      timestampz not null default now(),
```

It is **not** a `doctor` check: doctor is read-only by contract so it stays
safe mid-debug, and applying migrations writes. Doctor answers whether
anything *will* run them (finding 1); this answers whether they work. A
scratch database rather than a throwaway container because the isolation that
matters is from your *data*, not your postgres — and it runs against the same
server and version the migrations will really meet.

## 3. ✅ The repository template drifts its own column list

`jails generate class JdbcRewardRepository` emitted `amount` in the insert and
`amount_minor` in the select. Both compile. The select fails at runtime.

`spring.md` §8 already names this exact bug:

> **One column list, shared by the DDL, the select, the insert and the row
> mapper.** A hand-maintained pair drifts — `amount` in the insert against
> `amount_minor` in the select compiles fine and fails at runtime.

**Fixed** (before this pass, in `src/sql.rs`): one field spec produces the DDL,
the select, the insert, the bind and the row mapper together. The Spring
template additionally hoists them into a single `private static final String
COLUMNS` interpolated into both reads, which is what the reference repo does by
hand.

## 4. ✅ The repository template uses the API `spring.md` calls legacy

Generated: `JdbcTemplate` with **positional `?` parameters**. `spring.md` §8
mandates `JdbcClient` with **named parameters only**, and gives the reason —
"positional `?` in a five-column insert is a silent-swap bug waiting for a
schema change". A seven-column insert here.

**Fixed.** `generate::jdbc_client_repository` emits `JdbcClient` with
`.param("name", …)` bindings and `:name` placeholders, and is chosen whenever
`spring-boot-starter-jdbc` is on the classpath
(`generate::repository_wiring`). `the_spring_adapter_binds_by_name_and_shares_one_column_list`
pins it.

Two consequences worth knowing, both of which the fix had to handle:

- **The adapter is now a bean.** `JdbcClient` is injected, so the adapter
  carries `@Repository` — which means the in-memory adapter must *not*, or two
  beans qualify for one injection point. `exactly_one_repository_adapter_carries_the_bean_annotation`
  is the invariant test. The in-memory adapter is now a plain fake once a
  database exists, which is also the manual cleanup `rewards` performed by hand.
- **Plain Maven cannot have it.** `JdbcClient` lives in `spring-jdbc`, so a
  project without the starter keeps the caller-owned `Connection` adapter —
  not as a second-best choice but because the type does not exist. The
  real-toolchain tier caught this: the first version of the fix emitted
  `JdbcClient` for every Spring project and `g scaffold` stopped compiling on
  a project that had not run `add db`.

## 5. ✅ `Instant` bound straight to a JDBC parameter

`save()` passed `reward.createdAt()`, an `Instant`. pgjdbc has no mapping for
`Instant`; the first real save would have thrown. It needs `OffsetDateTime`
(or the driver-level `Timestamp`).

**Fixed** in `src/sql.rs`, whose write expression bakes in the conversion. Note
the gotcha recorded in `CLAUDE.md`: the expression bakes in the *receiver* too,
because `Timestamp.from(x.at())` puts it in the middle and gluing it on the
front yields `x.Timestamp.from(at())` — which reads fine and does not compile.
Only the real-toolchain tier catches that.

## 6. ✅ Generated companion tests are worse than no tests

`jails generate class` emits:

```java
@Test
void shouldDoSomething() {
    JdbcRewardRepository jdbcRewardRepository = new JdbcRewardRepository(null);
    assertThat(jdbcRewardRepository).isNotNull();
}
```

It passes while the class is entirely broken, it inflates the count (the suite
reported 39 green tests over a repository that could not read or write), and
`null` as the constructor argument teaches the pattern. `java.md` §7: "Don't
test getters, records' `equals`, or Spring's wiring."

**Fixed.** `g repo` emits a container-backed `*IT`, and `generate class` now
emits a `@Disabled` test whose name says what to prove, keeping the
construction (so it still stops compiling the day a real constructor arrives —
the one genuinely useful property of the old body).

**A deliberate deviation from this finding's own first choice.** It suggested a
*failing* `@Test void TODO()`. `@Disabled` was chosen instead because a failing
test would make `jails new` followed by `jails check` red on a project where
nothing is wrong, and a red build that is expected is a red build nobody
reads. `@Disabled` fixes all three stated defects anyway — it is reported as
skipped rather than counted green, so it cannot masquerade as coverage — and
it is already jails' idiom for "you have to finish this" (the field-spec
sample problem emits `@Disabled` tests for the same reason).

## 7. ✅ Testcontainers wired in the shape both persona files forbid

`jails add db` generated a static `PostgreSQLContainer` inside an
`ApplicationContextInitializer`, registered through
`src/test/resources/META-INF/spring.factories`, publishing a hand-built
`MapPropertySource`.

`spring.md` §10 and `backend.md`:

> **Declare containers as Spring `@Bean`s with `@ServiceConnection`**, not as
> `@Testcontainers`/`@Container` static fields … No `@DynamicPropertySource`
> plumbing when `@ServiceConnection` covers the container.

A global registration means **every** test starts Postgres, including pure unit
slices and a `@WebMvcTest` that has no business touching a database.

**This was the one finding that was a genuine disagreement rather than a bug.**
jails' `CLAUDE.md` argued at length *for* the global initializer, and its
reason was real: once `spring-boot-starter-jdbc` is present, JDBC auto-config
demands a DataSource for *every* `@SpringBootTest`, including the
`contextLoads` test that came with the project and never queries anything. An
`@Import`-per-test-class default breaks a test the user did not write, with a
message ("Failed to determine a suitable driver class") that names neither
cause nor fix.

**Fixed, by resolving both halves rather than picking a side.** The config is
now `TestcontainersConfig`, a plain `@TestConfiguration(proxyBeanMethods = false)`
holding the `@ServiceConnection` bean — *and* `add db` splices
`@Import(TestcontainersConfig.class)` into every `@SpringBootTest` already in
the project, including ones in other packages (which need the import statement
too). A leftover `spring.factories` from the old shape is deleted on the way
past, or it would keep registering a second container and the migration would
look like it had not worked.

`doctor`'s `test datasource` check was updated with it, and now asserts the
thing that actually matters rather than a file path: a container config exists
**and** every `@SpringBootTest` can see one. Checking only the file passes on a
project where a rebase dropped the `@Import` and every context test is red.

## 8. ✅ `doctor` was all-clear on a project that could not start

`jails run` failed with `spring-boot-docker-compose` shelling out to
`podman-compose` with Docker Compose v2 syntax (`--ansi never`,
`config --format=json`), which podman-compose does not accept. Meanwhile:

```
13 checks, all clear.
```

`jails why` diagnosed it afterwards, and the diagnosis was excellent — that
knowledge just arrived one failed run too late. It is a static fact about the
machine: **`spring-boot-docker-compose` on the classpath + a compose provider
that is podman-compose = the app cannot start.** That is a doctor check.

**Fixed.** `doctor`'s `compose provider` check fires whenever
`spring-boot-docker-compose` is on the classpath, reads the provider's own
version banner, and fails on podman-compose naming the syntax it rejects.

The fix line is the one that leaves nothing broken: install real Compose v2 as
a docker CLI plugin (`~/.docker/cli-plugins/docker-compose`), which drives
podman fine over `DOCKER_HOST`. Note the fix jails' `why` rule used to
suggest, `spring.docker.compose.enabled=false`, trades one failure for
another — it also removes the datasource URL the module was contributing, so
the app then dies on "no database URL" instead.

The banner classification is split into `classify_compose_provider` so it can
be tested without a subprocess; on this machine the check now reports
`Docker Compose version v5.5.0 -- spring-boot-docker-compose can drive it`.

## 9. ✅ `jails add kafka` does not keep the promise in `jails add --help`

> A capability is a whole slice, not a dependency line: the artifact in
> `pom.xml`, **the code that uses it, a test that proves it works**, and where
> relevant a compose service and the properties that make it behave.

What `add kafka` actually produced: the starter, a (good, KRaft, dual-listener)
compose service, and six properties. No code, no test, no `testcontainers-kafka`.
`jails doctor` then said `-- kafka  not in use`.

**Fixed.** `add kafka` on Spring now writes `KafkaConfig` + `KafkaConfigTest`
and adds `testcontainers-kafka`, `spring-boot-testcontainers`,
`testcontainers-junit-jupiter` and `awaitility`. Every item on the original
list is covered:

- **`testcontainers-kafka`**, without which no test can touch a broker. ✅
- **A poison-message path.** `DefaultErrorHandler` +
  `DeadLetterPublishingRecoverer` + explicit `.DLT` destination. ✅
- **`ErrorHandlingDeserializer`** wrapping the JSON deserializer. ✅
- **Retryable vs fatal classification.** `addNotRetryableExceptions(...)`. ✅
- **`group.protocol=consumer`** (KIP-848). ✅
- **`acks=all` / `enable.idempotence=true`** stated rather than inherited. ✅
- **A `NewTopic` bean.** ✅ — but moved to `g event`, not `add kafka`.
  `add kafka` does not know what this service's topics are called, and a
  generated `NewTopic` for a guessed name is worse than none.
- **`spring.json.use.type.headers=false` + `spring.json.value.default.type`.**
  ✅ — same split, and for the same reason: `default.type` *is* a type name,
  so only `g event` can write it.

**A trap worth encoding**, and now encoded in the template's Javadoc:
`DeadLetterPublishingRecoverer`'s default destination is `<topic>-dlt`, not
`<topic>.DLT`, and it reuses the source partition number. Declare a `.DLT`
topic, ship a consumer, and the records land somewhere else with only a WARN to
say so. The generated recoverer names the destination explicitly.

**One process note.** The first version of `KafkaConfigTest` asserted through
`handler.getClassifier()`, which does not exist — written from memory rather
than from `deps/`, which is precisely the failure `CLAUDE.md` warns about for
this file. The real-toolchain tier caught it; the fix went through
`deps/spring-kafka/.../ExceptionClassifier.java`, where `removeClassification`
turns out to be the only public way to read a classification back.

## 10. ✅ `*IT` classes are generated but nothing runs them

`jails generate integration-test` emits `FooIT`, and `jails --help` says "`*IT`
names use Failsafe" — but the generated `pom.xml` binds no
`maven-failsafe-plugin` execution, so `mvn verify` compiles the class and never
runs it. The generated body is `@Disabled` and throws
`UnsupportedOperationException`, so the omission is invisible.

**Fixed** (before this pass): `generate::ensure_failsafe` is called from the
write path rather than per-kind, so a new generator cannot forget it, and
`add.rs` does the same for capability plans. Both goals are bound —
`integration-test` runs them, `verify` makes a failure fail the build.

## 11. ✅ Requested `--deps` silently absent

The gym bootstrap asked for `--deps web,validation,actuator,jdbc,flyway,postgresql,kafka,testcontainers,docker-compose`.
The project had no `spring-boot-starter-validation` and no
`spring-boot-starter-actuator`.

**Fixed** in `new::verify_requested_deps`: after the zip is extracted, each
requested id is looked for in the generated `pom.xml` and anything missing is
named on stderr. A **warning**, not an error — the mapping from an Initializr id
to the artifact it contributes is not always one-to-one, and a false positive
must not stop a project being created.

## 12. ✅ `jails add json` installs the legacy Jackson

It added `com.fasterxml.jackson.core:jackson-databind` and
`jackson-datatype-jsr310` to a Boot 4 project that already had Jackson 3
(`tools.jackson`, 3.1.4) from the web starter, and generated `Json.java` against
the 2.x `ObjectMapper`. Result: two Jackson majors on one classpath and a
utility written against the deprecated binding.

**Fixed**, verified against `deps/jackson-databind`:

- Coordinates are `tools.jackson.core:jackson-databind`.
- `Json.java` uses `tools.jackson.databind.json.JsonMapper`.
- `jackson-datatype-jsr310` is **dropped** — java.time is in core databind, so
  the migration deletes a dependency. `WRITE_DATES_AS_TIMESTAMPS` moved to
  `cfg.DateTimeFeature` and already defaults to `false`, so the explicit
  `.disable(...)` went too.
- `JacksonException extends RuntimeException` in 3.x, so `throws
  JsonProcessingException` is gone from the generated signatures.

`doctor`'s json check was rewritten around the failure that is actually hard to
see: **both majors declared at once**. They do not conflict — the packages
differ — so nothing warns, and half the code ends up on a mapper nobody
configured. That is now a `FAIL`; a working Jackson 2 pair is a `WARN`.

## 13. ✅ No JSpecify anywhere

`java.md` §2 and `spring.md` §3 both make `@NullMarked` package-level opt-in
mandatory ("this is the standard now, not a proposal"). jails generated seven
packages and not one `package-info.java`.

**Fixed** at the single write path (`generate::ensure_package_info`), so no
generator can forget it: the first time jails puts a class into a package under
`src/main/java`, that package gets a null-marked `package-info.java`. `new` and
`new-cli` add `org.jspecify:jspecify`.

It is **conditional on the dependency being present** — annotating a package
that cannot resolve `@NullMarked` would hand the reader a compile error for a
file they did not ask for, which is the opposite of what a scaffold is for.

Note `java.md` §9's standard still applies and is **not** met: see "Left
undone".

## 14. ✅ Defaults the persona files call defaults, unset

Missing from the generated `application.properties`:

- `spring.threads.virtual.enabled=true` — `spring.md` §9 and `backend.md` both
  call this the default posture for blocking web workloads on JDK 21+.
- `spring.mvc.problemdetails.enabled=true` — RFC 9457 bodies.

**Fixed** in `new::write_default_properties`, which writes both with the
comment explaining why. Neither is discoverable from a failure: virtual threads
absent just means the service is quietly less concurrent than it should be, and
problemdetails absent means error bodies are Boot's ad-hoc map — which nobody
notices until a client has parsed the wrong shape.

## 15. ✅ Smaller template defects

- ✅ **`generate record` emitted accessor-only tests**
  (`accessorsReturnWhatWasConstructed`) — testing that javac generated an
  accessor, which `java.md` §7 names directly. Dropped. What is pinned instead
  is the compact constructor's validation, which is real behaviour and can
  really regress; a record with nothing to validate gets a `@Disabled` todo
  rather than a manufactured green tick, for the same reason as finding 6.
- ✅ **File/class name mismatch** (`MccTest.java` holding `class MCCTest`).
  Re-verified: fixed as a side effect of finding 17. `g test MCC` now writes
  `MCCTest.java` containing `class MCCTest`, because the file name and the
  class name are both derived from the same normalised stem.
- ✅ **`generate class` emitted an empty final class plus a not-null test** —
  see finding 6.
- ✅ **`@Service`/`@RestController` classes generated `public`.** Both stubs are
  package-private now, with the reason in their Javadoc: Spring instantiates
  and calls them by reflection, so `public` buys nothing and only widens what
  other packages can compile against. The generated handler method is
  package-private too.
- ⛔ **Generated services re-sort in Java what the SQL already ordered** — not
  reproducible in the current templates. The generated service does no
  sorting; the `order by` in the adapter is the single definition of stable
  order. This was a property of the hand-written `rewards` code rather than of
  anything jails emits.

## 16. ✅ A doc/tool disagreement to settle deliberately

`java.md` §8 and `spring.md` §11 both mandate **package by feature**. But
jails ships **package by layer** (`domain`, `service`, `web`, `app`,
`adapters`), and the gym spec asks for a layer layout too.

**Settled, in both places.**

`jails.toml` removes the concrete pain: a project renames each layer
(`service = "application"`, `adapters = "persistence"`, `web = "api"`) once,
instead of passing `--package` to every call — which is what `rewards` did,
and it still ended up renaming directories by hand, including one commit
fixing a `persistance` typo across two packages.

The argument itself was settled by noticing that both sides were describing
different projects. **Package-by-feature is a rule about the second feature.**
A single-domain service — which is most services, and every service on day one
— has exactly one feature package, so packaging by feature collapses to flat
and the layer names are the only structure there is. A scaffolding tool cannot
start you anywhere else. What the rule is really warning about is the moment a
service grows a second feature, when layer packages become bags of unrelated
things and the cut has to move.

`java.md` §8 and `spring.md` §11 now say that, so they no longer argue against
the layout jails ships while jails ships it. Both files are tracked in **this**
repo; `~/code/bank` reaches them through symlinks, so the edit lands in one
place and both projects see it.

---

## New findings from the same project (added 2026-08-20)

These came from reading `rewards`' git history rather than its final state —
the hand-edits after each jails command are the record of what jails did not do.

## 17. ✅ Generators doubled a suffix the name already had

`jails g service RewardHistoryService` wrote
`RewardHistoryServiceService.java`. So did `g controller RewardController` →
`RewardControllerController.java`, and the tests alongside them. `rewards`
renamed four generated files by hand in the two commits after they were
created.

Typing the name the type will actually have is the obvious thing to do — it is
what the file is called and what every reference to it says — and jails
punished it.

**Fixed** in `generate::strip_redundant_suffix`, applied to `Controller`,
`Service`, `Repository`, `Cli`, `Job`, `Client`, `Test` and `IT`. Only a whole
trailing suffix counts and never the entire name (`g service Service` means a
type called `Service`), and `scaffold` is exempt because it spans three
suffixes at once. **It runs in `destroy` too** — a normalisation applied to one
and not the other strands files that the tool then claims to have deleted.

## 18. ✅ `remove <capability>` silently deletes hand-written properties

`rewards`' `application.properties` had ~20 hand-written Kafka settings
*inside* jails' own `# jails:kafka` … `# /jails:kafka` markers — the
`ErrorHandlingDeserializer` pair, `acks=all`, the KIP-848 opt-in, the
`default.type`. `remove kafka` deletes that block wholesale, and would have
taken every one of them with it without a word.

The marked block is how `remove` knows what to take back out, and it is also,
inevitably, where people tune the capability — it is the block with the
capability's name on it.

**Fixed:** `add::unowned_properties` diffs the block against what jails would
write, and `remove` names every line it did not write, at the confirmation
prompt and in `--dry-run`. jails cannot refuse to remove them — they are inside
the block it owns — but it must not delete them silently.

(Most of those specific properties are now generated, per finding 9. The
warning matters for the next capability someone tunes.)

## 19. ✅ No way to send or inspect a Kafka message

`rewards` hand-wrote `scripts/kafka.sh` — 100 lines wrapping
`kafka-console-producer`, `kafka-console-consumer` and
`kafka-consumer-groups` with the broker address and the topic filled in.
jails had `jails db` for Postgres and nothing at all for the broker.

**Fixed:** `jails kafka <topics|describe|send|poison|tail|dlt|lag|reset>`.
Everything runs inside the compose broker container, so there is nothing to
install. The topic defaults to the one the source declares — read textually
out of a `TOPIC` constant, so it answers on a project that does not compile —
and the group to `spring.kafka.consumer.group-id`.

`poison` is the one worth naming: it publishes a deliberately unparseable
record so you can watch it reach the DLT instead of stalling the partition,
which is the behaviour finding 9's `KafkaConfig` exists to produce and which is
otherwise tedious to demonstrate.

## 20. ✅ `scaffold`'s in-memory adapter is a manual cleanup step

`rewards` deleted `InMemoryRewardRepository` and its test by hand once the real
database arrived. The README told it to.

**Fixed by finding 4 for new code, and by a check for existing code.**

Newly generated adapters get this right: exactly one carries `@Repository`,
decided by whether the JDBC starter is present, and the in-memory one becomes
an unannotated fake — genuinely useful to keep for tests.

`add db` still does not *rewrite* an already-generated scaffold, and that stays
deliberate: those files may have been edited by hand since, and silently
regenerating them would cost more than the alternative. The alternative is a
`doctor` check, because the state it leaves is the dangerous kind — **quiet**.
Two `@Repository` adapters is loud: Spring refuses to start and `jails beans`
reports the ambiguity. *One*, on the in-memory adapter, with a `DataSource`
sitting right there, starts perfectly and serves every request out of a map
that empties on restart. Nothing fails; the data simply is not there, and the
first person to notice asks why last week's records are gone.

`doctor`'s `repository bean` check reports exactly that, with the regenerate
command as its fix.

---

## Suggested checks, in cost-of-failure order

| Check | Catches | Where it lives |
|---|---|---|
| Migrations actually apply, first failure with file and line | 1, 2 | `jails migrate` |
| Every Boot-integrating capability asserts its `spring-boot-<tech>` module | 1 | `doctor` (Flyway; others audited) |
| Compose provider is Compose v2 when `spring-boot-docker-compose` is present | 8 | `doctor` |
| An `*IT` exists but no failsafe binding | 10 | `ensure_failsafe`, at the write path |
| A `@KafkaListener` exists with no error handler / DLT | 9 | moot — `add kafka` ships one |
| Test datasource is a `@ServiceConnection` bean, imported where needed | 7 | `doctor` |
| Every package has a null-marked `package-info.java` | 13 | `write_new_file` |
| Two Jackson majors on one classpath | 12 | `doctor` |
| An `@Repository` in-memory adapter beside a real DataSource | 20 | `doctor` |
| A class whose only test asserts `isNotNull()` | 6 | moot — no longer generated |
| `--deps` requested vs actually present in `pom.xml` | 11 | `jails new` |

## The one thing to change first

The original recommendation was: make `jails check` **start the real thing
once** — apply the migrations, boot the context against the compose services,
and fail loudly. Findings 1, 2, 5, 8 and 9 were all "the generated project
cannot actually run", and all five were invisible to a build that compiled and
to a doctor that reported thirteen checks all clear.

All five are now fixed at the generator, and the two halves that can be
checked cheaply are checked: `jails migrate` really applies the migrations,
and `doctor` catches the compose-provider and Jackson cases statically.

**What is deliberately still not done is booting the context.** It needs
Docker, it takes tens of seconds, and it would make `jails check` — the CI
gate — conditional on infrastructure. The three fast checks cover the failures
that actually happened. If a sixth "it compiled but could not run" failure
turns up that none of them catch, that is the evidence for building it, and
this is the note to point at.

## Left undone

**In jails** — one thing, deliberately:

- **Error Prone + NullAway are not wired into the build**, so the `@NullMarked`
  packages finding 13 generates are documentation rather than enforcement
  (`java.md` §9: "Static nullness checking that isn't enforced by CI is
  decoration"). This is a real gap and not a small one: it is a build-plugin
  change with its own configuration surface and its own failure modes, and it
  belongs with `add format` as a capability rather than bolted onto `new`.
  Generating the annotation without the checker is still strictly better than
  generating neither — the annotations are what a checker would read, and they
  are correct now rather than retrofitted later.

**In the reference repo (`~/code/bank/rewards`):**

- **`amount` was not renamed to `amount_minor`.** `specs/plan.md` specifies
  `amount_minor`, and `backend.md` §5 requires the unit in the column name
  ("Name the column so the unit is unmissable"), so the column, the record
  component and the JSON field should all carry it. Left alone on instruction —
  it is a three-file change when wanted.
- `rewards` has not been re-generated against the fixed jails, so it still
  contains the hand-written `TestcontainersConfig`, `KafkaConfig`,
  `V001__rewards.sql` and `scripts/kafka.sh` that jails now produces. They
  agree in substance — the generated migration and the hand-written one are
  the same table — so nothing needs doing unless you want the file provenance
  to match.
