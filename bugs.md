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

## B47 — `doctor` reports "all clear" over a ledger no command can read

`doctor` is the command you run when something is wrong, and it is the one
command that does not notice this.

```sh
echo garbage > .jails/ledger.toml
jails doctor                        # 25 checks, all clear.
jails g record Foo a:string         # jails: ... ledger.toml cannot be read by this jails
jails resource status Book          # state: ambiguous, declaration: unknown
```

Three oracles over one store, three answers: the generator names the file and
the schema rule it violates and tells you to restore it, `resource status`
reports the resource as ambiguous without saying why, and `doctor` reports a
clean project. Two commands reading one store must not answer differently —
the reader cannot tell which to believe, and the one they will believe is the
one that says everything is fine.

`compat` already classifies the store as absent / current / unreadable, so
`doctor` has the answer available and does not ask for it.

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

## B49 — `--method` is accepted and silently ignored by `g query`

```sh
jails g query MsgsThree --on Message userId:uuid --path '/y/{userId}' --method post
grep Mapping src/main/java/.../MsgsThreeQueryController.java   # @GetMapping
```

`--method post` produces `@GetMapping`; `--method get` on a query whose filters
are not all path variables produces `@PostMapping`. The verb is derived — GET
when every filter comes from the URL, POST otherwise — and that derivation is
defensible. Accepting a flag that contradicts it and saying nothing is not.

`--path` on `g scaffold` is the same situation handled correctly:

```
jails: `--path` applies to a controller, a use case or a query.
       fix: drop it from `jails g scaffold User`.
```

## B50 — an entity named after a `java.lang` type shadows it in its own package

```sh
jails g record String value:string
```

```java
package com.example.p.domain;

public record String(String value) {          // the component's type is the record
    public String {
        Objects.requireNonNull(value, "value");
    }
}
```

A package member outranks the implicit `java.lang` import, so `value` is
typed as the record being declared rather than as text. The caller asked for a
string field and got a self-reference.

Nothing reports it. `javac` accepts the record **and** the generated
`StringTest`, so the compiler tier — the only tier that answers the question
this tool exists for — is green over it. `Name` refuses Java reserved words,
but every reserved word is lowercase and the name is capitalised before the
check, so `class`, `int` and `String` all pass. `java.lang`'s own type names
are a closed list and the same check is the place for them.

## B51 — `jails explain query` describes a restriction that no longer holds

```
query  A typed read: query record, port, JDBC adapter, controller, tests.

  Required scalar equality filters only, for the same reason `transition`
  refuses optionals: null and list semantics would have to be guessed.
```

Optional filters ship and are correct:

```sh
jails g query UnreadMessages --on Message isRead:boolean?
```

```sql
where (cast(:is_read as boolean) is null or is_read = :is_read)
```

`explain` is a hand-written table by design, which is exactly why it needs the
gate the rest of the surface has: `every_kind_has_an_explanation` checks that a
kind *has* a row, and nothing checks that the row is still true.

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
