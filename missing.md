# missing.md — what six real minicom projects needed and jails could not give

Written 2026-08-26. `CLAUDE.md` says an earlier `missing.md` was folded into
`pending.md`; `pending.md` is itself gone now (deleted in `2f8003b`), so the
live open-item files are `bugs.md` and `research.md`. This file is the same
kind of document as the original: **what one set of real migrations needed and
did not get.** Nothing here is a proposal for its own sake — every entry is a
line I could not write with the CLI.

## What was built

Every project in `minicom/` dated 2025 or 2026 was rebuilt from scratch as a
Spring Boot 4.1 / Java 26 service using **only** `jails` commands — no editor,
no hand-written Java, no hand-edited pom. Sources are at
`/home/laith/code/minicom-jails/`.

| project | original stack | jails project | result |
|---|---|---|---|
| `minicom-05-02-2026` ("Minicom 2.0") | Django 5 + Channels + OpenAI | `minicom-2026-02-05` | **green**, 68 tests |
| `mc-01-06-2026` | Node 24 + TypeScript + Sequelize | `minicom-2026-06-01` | **green**, 34 tests |
| `mc-public-09-01-2026` | Django 5 | `minicom-2026-01-09` | **green**, 34 tests |
| `mc-13-12-2025` | Django 5 / Node / Rails / Spring skeleton | `minicom-2025-12-13` | **green**, 9 tests |
| `mc-21-11-2025` | Django + Channels, read receipts | `minicom-2025-11-21` | **green**, 32 tests |
| `mc-16-11-2025` | Django + Channels + HF bot | `minicom-2025-11-16` | **RED** — see M1 |

A seventh check ran `jails` inside the *unmodified* `mc-01-06-2026/spring`
tree — Gradle 8.5, Spring Boot 2.7.18, Java 21 — to test the foreign-project
path. That is M2.

The domain modelling held up well. `g scaffold`, `g enum`, `g association`,
`g query`, `g transition`, `g usecase`, `g strategy`, `add db/api/cors/json/sse`
and `set` covered most of every app's schema and CRUD surface, and five of six
reached `BUILD SUCCESS` with no file touched by hand. What follows is the
remainder.

---

## M1 — `g strategy` and `g scaffold` contradict each other, and there is no third option

**This is the one thing that made a build red.** Reproduction, from an empty
directory:

```
jails new x --package com.x && cd x
jails add db
jails g enum Sender CUSTOMER BOT AGENT
jails g scaffold Message id:long@pk conversationId:long@index sender:Sender content:string! createdAt:instant
jails g record BotReply text:string!
jails g strategy BotRule Greeting Order Refund Fallback --on Message --yields BotReply
jails build
```

```
ArchitectureTest.DOMAIN_HAS_NO_FRAMEWORK_DEPENDENCIES … violated (7 times):
Class <com.x.domain.GreetingBotRule> is annotated with <org.springframework.stereotype.Component>
```

`g scaffold` writes `ArchitectureTest`, which forbids `org.springframework..`
inside `domain..`. `g strategy` writes `@Component` implementations *into*
`domain..` — and the `@Component` is load-bearing, as its own Javadoc says:
without it the bean is silently absent from the injected `List<BotRule>`.

Both workarounds fail:

- `--package service` → **does not compile.** The generated `BotRule.java` and
  every implementation reference `Message` and `BotReply` with no import:
  `cannot find symbol: class Message`. `g strategy` never imports its `--on` /
  `--yields` types when it is placed anywhere but the default package.
- `--package ''` → identical compile failure in the base package.

And after generating with `--package service`, `jails destroy strategy BotRule`
refuses:

```
fix: `jails g strategy BotRule` is what records one. A destroy that guessed at
     paths would delete files jails never wrote.
```

leaving 8 orphan files under `service/` that only `rm` removes. So the
`--package` path is a one-way door.

Three distinct defects, one blast radius: **a strategy cannot be generated into
any project that also has a scaffold.** This is the exact bot-dispatch shape
that `mc-16-11-2025/django/chat/bot.py` uses (seven keyword rules → one reply),
and the shape `minicom-05-02-2026`'s AI escalation wants too.

Worth noting `tests/agreement.rs` and the golden suite cannot see this: the
scenario table exercises each kind in isolation, and the collision needs *two*
kinds in one project.

---

## M2 — `add db` has no Spring Boot floor, and produces an unresolvable build on Boot 2

`add api` refuses Boot 2 by name, precisely and helpfully:

```
jails: `api` generates code that uses ProblemDetail, and this project is Spring Boot 2.
       fix: `jails g controller`, `jails g scaffold`, … work on this project.
```

`add db` does not check at all. Run in `mc-01-06-2026/spring` (Boot 2.7.18,
Gradle) it succeeds, and then:

```
> Could not find org.springframework.boot:spring-boot-flyway:.
> Could not find org.flywaydb:flyway-database-postgresql:.
```

Four of the spliced coordinates are wrong for Boot 2.7:

| coordinate | problem on Boot 2.7 |
|---|---|
| `org.springframework.boot:spring-boot-flyway` | module does not exist — it is the Boot 4 split-out of the auto-configuration |
| `org.flywaydb:flyway-database-postgresql` | not in Boot 2.7's BOM (Flyway 10 / Boot 3.2 onward), and jails supplied no version |
| `org.springframework.boot:spring-boot-testcontainers` | Boot 3.1+ |
| `org.springframework.boot:spring-boot-docker-compose` | Boot 3.1+ |

Verified against the upstream checkout rather than from memory, per the rule in
`CLAUDE.md`: at `deps/spring-boot` tag `v2.7.18`, `git ls-tree -r` finds zero
paths matching `spring-boot-flyway` and zero matching `spring-boot-testcontainers`,
and `flyway-database-postgresql` appears nowhere in that tag's dependency
management. All three modules are Boot 3.1+/4 additions.

`database.rs` already carries the `MANAGED` / `PINNED` pair, chosen on whether
the project has Boot's dependency management. That is the wrong question:
Boot 2.7 *does* manage `flyway-core` and *does not* manage
`flyway-database-postgresql`, and has no `spring-boot-flyway` at all. The
predicate has to be the Boot **version**, the same one
`webmvc_test_import_for` and `validation_package` already read.

`what_jails_generates_for_boot_2_compiles_and_what_cannot_refuses_by_name`
covers `add cors`, `g enum`, `g scaffold`, `g usecase` — `add db` is in
neither list, so this is untested rather than known-broken.

This matters because the Boot 2.7 Gradle server is the *only* Spring server
four of the six checkouts ship. It is the project a reader is actually in.

---

## M3 — no identity column for integer primary keys

`id:uuid@pk` is complete: the create use case emits `UUID.randomUUID()`.
`id:long@pk` and `id:int@pk` are not:

```java
Message message = new Message(
        0L,                       // ← every insert, every time
        command.toUserId(), …);
repository.save(message);
```

and the DDL is `id bigint not null` with no `generated always as identity` and
no sequence. The second `POST /actions/send-message` violates the primary key.

Every one of the six originals uses an auto-incrementing integer key — Django's
implicit `AutoField`, Sequelize's `autoIncrement: true`. There is no marker
that asks for one: `@pk` says "this is the key", not "the database assigns it".

This is adjacent to the "client must invent the id" note in `bugs.md`, but it
is a different failure: there the request carries a value the caller chose;
here the *generated server code* hardcodes `0`, and no test catches it because
each generated test inserts exactly one row.

What is missing is a constraint marker — `@identity`, or `@pk` on an integer
implying it — that emits `generated always as identity` and makes the use case
read the key back rather than construct it.

---

## M4 — no WebSocket anything

Four of the six originals are bidirectional chat over Django Channels:

- `mc-16-11-2025` — `ws/chat/` echo consumer
- `mc-21-11-2025` — `ws/chat/<role>/<email>/`, per-user rooms, admin room
  switching, `read_messages` → `messages_read` broadcast
- `minicom-05-02-2026` — `ws/chat/<email>?role=`, plus admin presence tracking
  and an `admin_status` broadcast on connect/disconnect

`jails commands` lists no websocket kind or capability. `add sse` is the
nearest thing and is one-directional (`GET /events/{topic}/stream`), which
covers the server→client half of read receipts and presence and none of the
client→server half. Everything above was written by hand outside jails.

Two separable pieces, and the second is the harder and more valuable one:

1. **A `@ServerEndpoint`-shaped kind** — a `WebSocketHandler`, its registration,
   and a test. Mechanical.
2. **A presence primitive.** `minicom-05-02-2026` tracks admin presence in a
   module-level dict and says in a comment why that is allowed
   (`InMemoryChannelLayer` = single Daphne process) — which is to say the author
   knew it was wrong and shipped it anyway. This is the same class of "the
   default is wrong in a way nothing reports" that `g auth` and `add sse` exist
   for: an in-memory presence map is silently correct on one node and silently
   wrong on two, with no error either way.

---

## M5 — a query cannot join, so no endpoint keyed by a natural key works

`g query --on X` filters on X's own columns by equality. Every real read in
these apps crosses a table:

| original endpoint | what it needs |
|---|---|
| `POST /customer_api/ping {email}` | `users ⋈ messages` — unread messages for the user *with that email* |
| `GET /messages` (node) | each message with its author's `{id, email}` embedded |
| `GET /api/conversations/` | 20 most recent conversations, each with its **last** message |
| `GET /admin_api/issues` | issues with `user.email` — the Django code says `select_related('user')` |

The first is the whole customer-facing surface of `minicom-05-02-2026`. jails
generated `UnreadMessagesForUserQuery(toUserId, read)` — correct, and reachable
only by a caller who already knows the surrogate id, which no minicom client
does.

`g association` already reads both records and type-checks the field mapping
across the boundary. That is exactly the information a join needs; it is
recorded and then used only to emit a foreign key. A `--via <Association>` on
`g query`, letting a filter name a column on the parent, would cover all four
rows above without inventing a query language.

Related and smaller: **no aggregate or ordering.** No `order by`, no `limit`,
no `count`, no `max`. `GET /api/conversations/` is `[:20]` ordered by
`-created_at`; `User.unread_count()` is a `count()`. Both are hand-written.

---

## M6 — no get-or-create, and it is the first line of three of the six apps

```python
User.get_or_create_from_email(email)     # minicom-05-02-2026, on every ping
await User.upsert({ id: 1, email: … })   # mc-01-06-2026, on every request
conv = Conversation.objects.create() if not conv_id else …   # mc-16-11-2025
```

There is no jails verb for it. `g usecase` always inserts. `g idempotency` is a
different primitive (retained result keyed by request hash) and `explain` is
right that it is different — but the shape a chat app needs on every inbound
message is "one row per natural key, return it either way", which is one
`insert … on conflict (email) do nothing returning`, the same statement
`g idempotency` already knows how to write.

This is the single most repeated hand-written line across the six projects.

---

## M7 — `g client` has one fixed shape, and it is a REST collection

`jails g client OpenAiChatClient` produces:

```java
@GetExchange("/open-ai-chats")     List<OpenAiChatPayload> findAll();
@GetExchange("/open-ai-chats/{id}") OpenAiChatPayload findById(@PathVariable String id);
```

The call `minicom-05-02-2026` actually makes is
`POST /v1/chat/completions` with a JSON body of messages. Nothing about the
generated interface survives: not the verb, not the path, not the arguments,
not the return type. The `HttpClientsConfig` and the `spring-boot-starter-restclient`
splice — the parts that are genuinely hard to remember, per the module's own
docs — are worth having; the interface is 100% overwritten.

`g controller` already takes `--method`, `--on` (request body) and `--returns`.
`g client` taking the same three would make it generate the call the project
makes rather than a shape to delete.

---

## M8 — no way to name a route path

Route paths are derived from the generated name, with no override anywhere in
`jails g --help`. The originals are not derivable:

```
/customer_api/ping        /customer_api/read
/admin_api/messages       /admin_api/users        /admin_api/issues
/api/customer/message/    /api/agent/reply/       /api/conversations/
```

against jails' `/actions/send-message`, `/queries/unread-messages-for-user`,
`/users`. For a greenfield service that is a virtue — one convention, and
`destroy` can find what `generate` wrote. For **porting an existing service, or
writing a new server against an existing frontend, the URLs are a fixed
external contract** and jails cannot meet it. Both `foo-website/foo.js` and
`bar-website/bar.js` in every checkout hardcode their paths.

`POST /foo` and `POST /bar` are the exception that proves it: `jails g
controller Foo --method post` does land on `/foo`, by luck of the singular
name. The body still has to be written by hand — `g controller --returns
Verification` emits `throw new UnsupportedOperationException("todo: …")`, so
the two-line `{"success": true}` that the minicom README makes the whole
acceptance test is not something jails can produce.

A `--path` on `g controller` / `g usecase` / `g query`, recorded in the ledger
like any other value, would close this. The derivability argument does not
apply once the path is a recorded value rather than a recomputed one.

---

## M9 — no way to add an index to an existing table

`--index` exists on `g scaffold` and `@index` on a field, both at creation
time. Afterwards there is nothing: `mc-01-06-2026`'s third migration is
`addIndex('messages', ['customer_id'])` on a live table, and
`jails g migration add_customer_id_index` produces

```sql
-- Forward-only migration. Write explicit SQL below.
```

`g field` can already add a column to a live table with a data plan
(`--default-literal` / `--backfill-file`), which is the harder problem. An
index has no data plan to argue about.

---

## M10 — no seed or fixture data path

`mc-01-06-2026` seeds three users and four messages on every request because it
has nowhere else to put them; `minicom-05-02-2026` relies on
`get_or_create_from_email`. jails writes `src/test/resources/fixtures/*.json`
for tests, and nothing for `dev`. There is no `db/seed` convention, no
`jails db seed`, and `add db` writes no `V00X__seed_*.sql`.

Lower severity than the rest — it is a convention, not a mechanism — but it is
why the Node app does database writes inside a GET handler.

---

## M11 — `transition` requires a `version` column the original schema does not have

Marking a message read is, in all three Django apps, `is_read = True; save()`.
In jails it is:

```
jails g field Message version:long --default-literal 0
jails g transition MarkMessageRead id:long read:boolean version:long --on Message
```

The compare-and-set argument is right and `explain transition` makes it well.
But there is no unguarded alternative, so a schema being *ported* grows a column
its owner did not ask for. Worth either an `--unguarded` that says in the
generated Javadoc what was given up, or a line in `explain transition` naming
`g usecase` + a manual update as the escape hatch. Recording it as friction,
not as a defect.

---

## Two smaller things, noted without a section

- **`g strategy` generates no evaluator.** The port's Javadoc shows the
  `List<BotRule>` fold you are meant to write, and `--yields` makes the return
  shape unambiguous, so the fold is derivable. There is also no ordering
  concept, which matters here: `FallbackBotRule` must run last, and nothing in
  the generated code says so.
- **A `usecase` defaults an enum positionally**, `IssueStatus.values()[0]`.
  It happened to be `OPEN`, which is what the Django `default='OPEN'` says —
  by luck of declaration order. Reordering the `g enum` arguments would silently
  change the default of every generated create.

---

## What this exercise says about the tool

The scoreboard is 5 of 6 green with **zero** hand-written Java, on six real
apps written by six different people in four languages. The layering, the
field spec, `association`, `query`, `transition` and the write-path rules
(import normalisation, `ensure_failsafe`, `ensure_assertj`,
`ensure_webmvc_test`) all did their job without being thought about, which is
the point of them.

The gaps cluster, and the cluster has a shape. **jails models a resource
extremely well and a conversation not at all.** Every one of M4, M5, M6 and M10
is the same missing idea from a different angle: a participant identified by a
natural key, a stream of messages between participants, and presence. That is
not a request for a chat feature in core — `app.rs` is domain-blind on purpose
and should stay that way. It is the observation that *get-or-create by natural
key*, *read across an association*, and *bidirectional push* are three generic
primitives, and that all six of these projects needed all three.

M1 and M2 are different: those are defects, not absences, and both are
invisible to the current test suite for the same reason — the golden scenarios
exercise one kind on one flavour, and both bugs need a second thing present.

---

## Reproducing

The six builds are at `/home/laith/code/minicom-jails/`, each a plain Maven
project — `cd` into one and `jails build`. The full command log per project is
its `.jails/ledger.toml`; `jails history` prints it.

The two defects reproduce from an empty directory in under a minute:

```sh
# M1 — strategy vs. scaffold
jails new m1 --package com.x && cd m1
jails add db
jails g enum Sender CUSTOMER BOT AGENT
jails g scaffold Message id:long@pk sender:Sender content:string!
jails g record BotReply text:string!
jails g strategy BotRule Greeting Fallback --on Message --yields BotReply
jails build          # ArchitectureTest fails
jails g strategy … --package service   # then: cannot find symbol: class Message
```

```sh
# M2 — add db on Spring Boot 2
cp -r minicom/mc-01-06-2026/spring /tmp/m2 && cd /tmp/m2
jails add db         # succeeds
JAVA_HOME=…/openjdk-21.0.2 ./gradlew build
                     # Could not find org.springframework.boot:spring-boot-flyway:
```

M3 needs only `jails g scaffold X id:long@pk name:string!` followed by
`jails g usecase MakeX name:string! --on X` — read the generated
`DefaultMakeXUseCase.execute` and the `V001` DDL.
