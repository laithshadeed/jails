# What jails could not do

Every entry here came out of one real job: migrating
`minicom/minicom-public/spring` (Spring Boot 2.7, Gradle, Java 21) to
`minicom/minicom-public/spring-4` (Spring Boot 4.1.1, Maven, Java 26) on
2026-08-24. The target is small — two POST endpoints, a CORS policy, an H2
database and a two-table schema — which is what makes it useful: nothing here
is an exotic requirement jails declined on purpose. Each entry says what was
asked, what jails did instead, and what had to be written by hand.

`pending.md` is where an idea goes once it has been designed. This file is the
raw evidence: what a real project needed and did not get.

## All eight are closed

Kept as written, because the evidence is the point and a list of solved
problems edited into silence teaches nothing. What shipped, per entry:

| # | what was missing | what closed it |
|---|---|---|
| 1 | `new` cannot choose the group or package | `--group` / `--package` on `new` and `new-cli` |
| 2 | `g controller` emits one shape | `--method`, `--returns` (`--yields`), `--on` |
| 3a | no H2 capability | `jails add h2` |
| 3b | no way to add an arbitrary dependency | `jails add dependency <g>:<a> [--version] [--scope]` |
| 4 | `add cors` generates CORS that does not work | the `CorsFilter` registration, and a preflight test through the dispatcher |
| 5 | nothing writes a property outside a capability | `jails set` / `jails unset` |
| 6 | no non-Flyway schema, and its silent failure | a `doctor` check for `spring.sql.init.mode` with no schema file; H2 dialect in `sql.rs` |
| 7 | no test-only datasource | `jails set --tests`, writing the additive `config/` overlay |
| 8 | smaller friction | `HELP.md` and `info.app.description` gone; 8a did not reproduce and is recorded as such |

Entry 4 was the one to act on first and was: the defect is fixed, and the
replacement test was checked by removing the fix again -- two failures where
the old test passed.

---

## 1. `jails new` cannot choose the group or the package

**Wanted:** a project in `com.intercom.spring`, matching the one being
migrated.

**Got:** `com.example.spring_4`. `new` passes `artifactId`, `name` and
`baseDir` to start.spring.io and nothing else, so the package is always
Initializr's default `com.example.<sanitised name>`, and `--offline` derives
the same thing locally through `sanitize_package(name)`.

**Fix by hand:** move two files, rewrite two `package` lines, rename
`Spring4Application` to `Application`, and patch `<groupId>` in the pom. Then
patch `AGENTS.md`, which jails had already written with the old package name
in it.

This is the first thing anyone migrating an existing service hits, because an
existing service already has a package and it is never `com.example`. `--group`
and `--package` are two more `-d` arguments on the curl call (`groupId=` and
`packageName=`) plus the same two values threaded into the offline path.

The underscore is a second, smaller bug: a project called `spring-4` gets
`com.example.spring_4`, which is a legal but unidiomatic package name.
Initializr would have produced it too, so this is not jails' invention — but
`new` is the layer that could refuse or normalise it.

## 2. `g controller` emits one shape, and it is `GET` returning a `String`

**Wanted:** `POST /foo` returning `{"success": true}`.

**Got:**

```java
@GetMapping("/foo")
String get() {
    return "Foo";
}
```

There is no way to say the method, the path, or the response type. Both
generated controllers were rewritten by hand, and so were both generated
tests — `mvc.get()...bodyText()` had to become `mvc.post()...bodyJson()`.

The surrounding work jails did here was genuinely valuable and worth keeping:
placement in the `web` layer, the package-private class with the Javadoc
explaining why, `package-info.java` with `@NullMarked`, the `MockMvcTester`
test with the right Boot 4 import for `@AutoConfigureMockMvc`, and
`spring-boot-starter-webmvc-test` spliced into the pom off the emitted bytes.
Every one of those is a thing a person gets wrong. Only the four lines in the
middle were wrong, and they are the four lines a flag could fix:

    jails g controller Foo --method post --returns Verification

`scaffold` is not the answer for this case. The README is right that
`controller` is "a stub, for a route that exists for a reason jails cannot
infer" — but the *method* is not part of that reason, and defaulting it to GET
silently is what turns a stub into a rewrite.

## 3. There is no H2 capability, and no way to add a dependency at all

**Wanted:** H2, file-backed, with the browser console — what the project being
migrated used, and what its README documents.

**Got:** nothing applicable. `add db` is PostgreSQL + Flyway + Testcontainers +
a compose service; `add sqlite` is a different database. Neither is a
substitute for "the interview scaffold's in-process database that the README
tells the reader to open in a browser".

**Fix by hand:** two `<dependency>` blocks in `pom.xml`
(`com.h2database:h2` and `org.springframework.boot:spring-boot-h2console`) and
four properties.

Two separate gaps sit behind this:

- **No `add h2`.** It is a small capability and a common one: an in-process
  database, the console module, the datasource properties, and a test-scoped
  in-memory override. If it existed, this migration would have been one
  command.
- **No way to add an arbitrary dependency.** This is the bigger one. jails
  splices dependencies constantly — `ensure_dependency`, `ensure_assertj`,
  `ensure_failsafe`, `pom::add_dependency` — and every one of them is reachable
  only from inside a generator. A project that needs one artifact jails has
  never heard of has to hand-edit the pom, which is exactly the file
  `pom.rs` exists to edit surgically. Something like

      jails add dependency com.h2database:h2 --scope runtime

  would cost almost nothing and would mean "jails does not know this library"
  stops meaning "open the pom yourself".

**Boot 4 detail worth recording:** `H2ConsoleAutoConfiguration` moved out of
the core autoconfiguration into its own `spring-boot-h2console` module
(verified in `deps/spring-boot/module/spring-boot-h2console` and in the BOM at
`platform/spring-boot-dependencies/build.gradle`). Without that artifact
`spring.h2.console.enabled=true` is a property with nothing listening to it —
no warning, no console. Any future `add h2` has to know this, and `lint` could
carry it as a rule.

## 4. `add cors` generates CORS that does not work

**This one is a defect, not a gap.** `add cors` writes a
`CorsConfigurationSource` bean and a unit test that asserts the bean is
correctly shaped. The bean is never consulted.

A `CorsConfigurationSource` bean is read by Spring Security's filter chain. In
a plain Spring MVC application with no Spring Security — which is what
`jails new --deps web` produces, and what `add cors` does not check for —
nothing reads it. A preflight is answered by the dispatcher's default `OPTIONS`
handler: **200, an `Allow` header, and no `Access-Control-Allow-Origin` at
all.** The browser blocks the real request. `doctor` reports
`capability cors  everything it installs is present`.

The failure is invisible from the server side, which is why the generated test
passes: it constructs `new CorsConfig().corsConfigurationSource(...)` directly
and asserts on the returned object, so it would pass even if the class were
never registered as a bean.

**Fix applied in the migrated project:**

```java
@Bean
FilterRegistrationBean<CorsFilter> corsFilter(
        @Qualifier("corsConfigurationSource") CorsConfigurationSource source) {
    var registration = new FilterRegistrationBean<>(new CorsFilter(source));
    registration.setOrder(Ordered.HIGHEST_PRECEDENCE);
    return registration;
}
```

The qualifier is load-bearing and cost a debugging round: Spring MVC's own
`mvcHandlerMappingIntrospector` is *also* a `CorsConfigurationSource`, so the
unqualified injection point finds two candidates and the context does not
start.

Two things follow for jails:

- `add cors` should register this filter (harmless when Security is present —
  Security's chain and the filter agree, since both read the same bean), or
  refuse with "run `jails add security` first".
- **The generated test has to go through the dispatcher.** A unit test over the
  config object cannot observe the only failure mode this capability has. The
  test that found it is now `CorsPreflightTest` in the migrated project: a
  `@SpringBootTest` + `MockMvcTester` preflight asserting
  `Access-Control-Allow-Origin` for an allowed origin and a 403 for an unlisted
  one. That is the shape `add cors` should emit, and it is the same lesson
  `ensure_failsafe` records — a test that never runs and a test that cannot
  observe the failure are the same bug.

## 5. Nothing writes or checks `application.properties` outside a capability

**Wanted:** `server.port=3000`, the datasource URL, `spring.sql.init.mode`, and
the CORS origins the project actually uses.

**Got:** `add cors` wrote `app.cors.allowed-origins=http://localhost:3000` — a
placeholder that is not just wrong here but *actively misleading*, since 3000
is this application's own port and never a browser origin. `add actuator` wrote
its keys correctly. Everything else was hand-edited.

Under the transaction protocol a setting is a `ResourceKey::Property` owned by
whoever wrote it, so the machinery for this exists and only the entry point is
missing:

    jails set server.port=3000
    jails set app.cors.allowed-origins=http://127.0.0.1:8008,http://127.0.0.1:8009

The value of routing it through jails rather than a text editor is that jails
knows which keys it owns, so `remove` and `sync` keep working, and a key with a
known closed vocabulary could be validated.

A related smaller thing: `add cors`'s placeholder origin should be
`https://example.invalid` or absent-and-refused, not a plausible-looking
`localhost` URL that will pass review.

## 6. `g migration` cannot express a non-Flyway, non-PostgreSQL schema

**Wanted:** `src/main/resources/schema.sql`, applied by
`spring.sql.init.mode=always` — the mechanism the project being migrated used
and the one its README documents.

**Got:** `g migration` writes `db/migration/VNNN__description.sql`, forward-only,
against Flyway and PostgreSQL. Correct for the projects jails builds; not
applicable here.

Written by hand. This one is arguably not a gap at all — Flyway-and-PostgreSQL
is a deliberate scope choice and `README.md` says so. It is recorded because
the *consequence* is a gap: since the schema did not come from jails, nothing
told the project that `spring.sql.init.mode=always` **fails silently**. A typo
in the file, or a statement H2 2.x no longer accepts, does not stop the context
from starting. The tables are simply absent, and the first query to need one
fails in front of a user. `SchemaTest` in the migrated project is the test that
closes it, and a `doctor` check —"`spring.sql.init.mode` is set and the schema
file parses" — would generalise.

**H2 2.x detail:** `AUTO_INCREMENT` and `datetime` were dropped outside MySQL
compatibility mode. `GENERATED BY DEFAULT AS IDENTITY` and `TIMESTAMP` are the
replacements. The original schema would not have applied.

## 7. `jails test` cannot get a project a test-only datasource

Tests inherited `spring.datasource.url=jdbc:h2:file:~/minicom-spring-4`, so the
suite would have written to the developer's home directory and would have
failed on H2's file lock the moment it ran while the server was up.

The fix is a one-key override in `src/test/resources/config/application.properties`,
which works because `classpath:/config/` outranks `classpath:/` and is
additive — unlike `src/test/resources/application.properties`, which shadows
the main file wholesale. jails already knows this trick (`durable-job` writes
into exactly that file) but there is no way to reach it, and the trap on the
other side of it is the kind of thing `CLAUDE.md` documents because people keep
falling into it.

## 8. Smaller friction, no design needed

- **`jails g test <Name>` ignores its layer.** `--package ''` was needed to get
  a test in the base package; without it the skeleton lands in `web`.
- **`jails new` writes `AGENTS.md` with the base package baked in**, so
  anything that later changes the package leaves that file lying. It should be
  derived, or `about` should own the fact.
- **`HELP.md` arrives from start.spring.io and is gitignored**, so every new
  project ships a tracked-looking file that is not tracked. `new` deletes
  `.gitignore` entries it does not want; it could delete this too.
- **`info.app.description=@project.description@`** is written by `add actuator`
  against Initializr's empty `<description/>`, which resolves to an empty
  string. Harmless, but it is a generated line that says nothing.

---

## What jails got right, for contrast

Recorded because a list of gaps read alone is misleading about the ratio. On a
migration of this size the tool did most of the work and all of the parts that
are easy to get wrong:

- `jails new` produced Boot 4.1.1 on Java 26 with the enforcer plugin pinning
  both, `mise.toml`, the fixtures directory and a working wrapper — one
  command.
- `add actuator` isolated the management connector on 8081, wrote explicit
  probe groups, and generated a test asserting that `/env` and `/heapdump`
  stay unexposed. Both of jails' opening `doctor` warnings were real and both
  were fixed by the command `doctor` named.
- `g record` produced the response type; `g controller` produced everything
  around the four lines that were wrong, including the Boot 4 test imports.
- `write_new_file` normalised imports and wrote `package-info.java` with
  `@NullMarked` without being asked.
- `doctor` went from 2 warnings to 17 checks all clear, and its
  `capability drift` half re-planned both capabilities and confirmed them.
- `routes`, `beans`, `lint` and `stats` all answered correctly on a project
  whose package layout jails had not chosen.

The one thing `doctor` reported clear that was not clear is entry 4, and that
is the entry to act on first.
