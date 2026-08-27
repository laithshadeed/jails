# bugs.md — open defects found by dogfooding jails

Binary: `jails 0.1.0`, built and installed from this checkout. Every report
below was reproduced from an empty directory with the commands as written, in a
disposable project under a scratch directory. **No jails source, test, build or
doc file is modified while reproducing.**

**A closed report is *deleted* from this file, not marked done.**
`git log -p -- bugs.md` is where a closed one and the run that closed it live.
Numbers are stable and never reused, so a `bugs.md B33` citation in the source
still resolves to a subject.

---

## B46 — a resource that has been dropped and re-created can never be dropped again

The second `destroy --storage drop` on the same resource refuses with jails'
own internal-bug message and writes nothing. The resource is then stuck: the
only command that retires its table is the one that no longer runs.

```sh
jails new --offline u --package com.example.u
cd u && jails add db --no-start
jails g scaffold Book id:uuid@pk title:string
jails destroy scaffold Book --storage drop --confirm-table books --force   # ok, V002__drop_books.sql
jails g scaffold Book id:uuid@pk title:string                              # ok, V003__create_books.sql
jails destroy scaffold Book --storage drop --confirm-table books --force
```

```
jails: this command planned against `src/main/resources/db/migration/V001__create_books.sql`
       without observing it first, which is a bug in jails rather than in your project
       -- nothing was written.
```

`V001` is the **superseded** create; the live one is `V003`. The drop planner
reaches the whole sealed lineage while the read set declares only the current
head, so the guard fires on the first command that has more than one create to
walk. One create (with or without intervening `resource field` migrations)
works — it is the second create that is fatal.

The refusal itself is correct behaviour for an undeclared read: the bug is the
missing declaration.

## B48 — a path-variable query generates a controller test that cannot pass

```sh
jails g query MessagesForUser --on Message userId:uuid \
      --path '/admin_api/messages/{userId}'
```

The controller is right — `@GetMapping` with `@PathVariable UUID userId` on
`/admin_api/messages/{userId}`. The generated test is wrong three ways at once:

```java
assertThat(mvc.post()                                   // 1. POST to a GET-only route
        .uri(MessagesForUserQueryController.PATH)       // 2. "{userId}" never expanded
        .contentType(MediaType.APPLICATION_JSON)
        .content("""
{
  "userId": "00000000-0000-0000-0000-000000000001"
}
"""))                                                   // 3. a body for a @PathVariable
        .hasStatusOk();
```

It fails at the URI, before the verb or the body matter:
`java.lang.IllegalArgumentException: Not enough variable values available to
expand 'userId'`. Observed on `minicom-15-01-2026`, reproduced from a clean
`jails new --offline`.

The test renderer branches on the criteria record; it does not know the path
carries variables, which the controller renderer already worked out.

## B55 — `jails add websocket` is rejected as an invalid capability

While `jails g socket <Name>` exists to scaffold WebSocket handlers, `jails add websocket` (and `jails add socket`) is rejected:
```
error: invalid value 'websocket' for '<CAPABILITIES>...'
  [possible values: db, kafka, csv, sqlite, h2, json, testkit, fake, http, format, coverage, loadtest, ci, docker, k8s, api, actuator, cache, security, cors, sse, mail, redis, observability, toxiproxy]
```

Adding `websocket` as a recognized capability in `jails add` should install `spring-boot-starter-websocket` into `pom.xml` / `build.gradle` and configure `[project] capabilities = ["websocket"]`.

---

## Never covered

Recorded so the gaps in this file are visible rather than implied.
`testd` and `--affected`; `test --engine warm`; `jails run` cold start;
`sql check --live`, `introspect`, `pull`, `contract check`, `editor`,
`request`, `runner`, `logs`, `console`; a generated application run end to end
against a live database. No `gradle` binary is on PATH, so a Gradle claim can
only be exercised through a checkout that ships its own wrapper -- which
`minicom/minicom-15-01-2026/spring` and `minicom/old/mc-01-06-2026/spring` both
do, under `JAVA_HOME` pointing at JDK 21.
