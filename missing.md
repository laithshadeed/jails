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
| `mc-16-11-2025` | Django + Channels + HF bot | `minicom-2025-11-16` | **RED** — M1, now closed |

A seventh check ran `jails` inside the *unmodified* `mc-01-06-2026/spring`
tree — Gradle 8.5, Spring Boot 2.7.18, Java 21 — to test the foreign-project
path. That is M2.

**M1 is closed.** The port stays in `domain` and its beans moved to `service`,
so the ArchUnit rule `g scaffold` writes and the `@Component` `g strategy`
needs stop contradicting each other; `--on`/`--yields` reach both through
`import_of`, which is what makes `--package` compile; and a `destroy` that
cannot find a `--package`-placed resource now names the package it was
recorded under instead of reporting it as never generated. The reason the
suite could not see it is closed too — the shared Spring toolbox generates a
scaffold *and* a strategy into one project and runs `mvn test` over both, and
its cache is salted with the harness text so adding a step to that list
actually runs it.

The domain modelling held up well. `g scaffold`, `g enum`, `g association`,
`g query`, `g transition`, `g usecase`, `g strategy`, `add db/api/cors/json/sse`
and `set` covered most of every app's schema and CRUD surface, and five of six
reached `BUILD SUCCESS` with no file touched by hand. What follows is the
remainder.

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

### Reproduce

```sh
cp -r minicom/mc-01-06-2026/spring /tmp/m2 && cd /tmp/m2
jails doctor          # ok project: Spring Boot (Gradle) — jails reads it correctly
jails add db          # succeeds, no warning
JAVA_HOME=<a JDK 21> ./gradlew build
```

```
> Could not resolve all files for configuration ':compileClasspath'.
   > Could not find org.springframework.boot:spring-boot-flyway:.
   > Could not find org.flywaydb:flyway-database-postgresql:.
```

Note the trailing `:` with nothing after it — the coordinate was spliced with
no version because jails classified the project as `MANAGED`.

The contrast is one command away, in the same directory:

```sh
jails add api
```

```
jails: `api` generates code that uses ProblemDetail, and this project is Spring Boot 2.
       ProblemDetail arrived with the Jakarta EE 9 line, which is Spring Boot 3 …
       fix: `jails g controller`, `jails g scaffold`, `jails g usecase`, `jails add cors`
            and every non-web kind … work on this project.
```

That is exactly the refusal `add db` should be giving, and the `fix:` line is
exactly the shape it should have. `require_jakarta_spring` names a *type*;
this one would name the *module* — "`add db` wires Flyway through
`spring-boot-flyway`, which is Spring Boot 4's split-out auto-configuration
module and does not exist on this project."

Confirming the version predicate is the right one, also one command:

```sh
cd /tmp/m2 && grep -n "spring-boot-gradle-plugin" build.gradle
#   classpath("org.springframework.boot:spring-boot-gradle-plugin:2.7.18")
```

jails already reads that number — `jails doctor` prints "Spring Boot (Gradle)"
and `mockmvc_autoconfigure_import` / `webmvc_test_import` / `validation_package`
all branch on the Boot version. `add db` is the one that does not.

This matters because the Boot 2.7 Gradle server is the *only* Spring server
four of the six checkouts ship. It is the project a reader is actually in.

---

## M3 — no identity column for integer primary keys, and the loss is silent

`id:uuid@pk` is complete: the create use case emits `UUID.randomUUID()`.
`id:long@pk` and `id:int@pk` are not.

### Reproduce — two commands

```sh
jails new m3 --package com.x --offline --no-git && cd m3
jails g scaffold Message id:long@pk sender:string! content:string!
jails g usecase PostMessage sender:string! content:string! --on Message
sed -n '/public Message execute/,/^    }/p' src/main/java/com/x/service/DefaultPostMessageUseCase.java
```

```java
public Message execute(PostMessageCommand command) {
    Objects.requireNonNull(command, "command is required");
    Message message = new Message(
            0L,                      // ← every insert, every time
            command.sender(),
            command.content());
    repository.save(message);
    return message;
}
```

```sh
head -4 src/main/resources/db/migration/V001__create_messages.sql
```

```sql
create table messages (
  id       bigint not null,          -- no identity, no sequence, no default
```

Swap `id:long@pk` for `id:uuid@pk` and the same command emits
`UUID.randomUUID()` and `id uuid not null`. So the machinery for
"server assigns the key" exists and is reachable for exactly one type.

### The failure is worse on the default adapter than in the database

`InMemoryMessageRepository.save` is `items.put(String.valueOf(message.id()), message)`
— and `message.id()` is always `0`. Run it:

```sh
jails build
cat > /tmp/probe.jsh <<'EOF'
import com.x.adapters.InMemoryMessageRepository;
import com.x.service.*;
var repo = new InMemoryMessageRepository();
var uc = new DefaultPostMessageUseCase(repo);
uc.execute(new PostMessageCommand("alice", "first"));
uc.execute(new PostMessageCommand("alice", "second"));
System.out.println("rows after two creates: " + repo.findAll().size() + " -> " + repo.findAll());
/exit
EOF
jshell --class-path target/classes -q /tmp/probe.jsh
```

```
rows after two creates: 1 -> [Message[id=0, sender=alice, content=second]]
```

Two creates, one row, the first message **silently gone** — no exception, no
log line. Against the JDBC adapter the same pair is a primary-key violation
instead, so the app fails one way in dev and another in production.

No generated test catches it because every generated test inserts exactly one
row. A `saves_two` case on the scaffold's own service test would have.

### What is missing

A constraint marker in the field spec — `@identity`, or `@pk` on an integer
type implying it — that emits `generated always as identity` and makes the
generated create read the key back rather than construct it. All six originals
use one: Django's implicit `AutoField`, Sequelize's `autoIncrement: true`.

This is adjacent to the "client must invent the id" note in `bugs.md`, but it
is a different failure: there the *request* carries a value the caller chose;
here the generated *server* hardcodes `0` and drops rows.

## M4 — no WebSocket anything

Four of the six originals are bidirectional chat over Django Channels:

- `mc-16-11-2025` — `ws/chat/` echo consumer
- `mc-21-11-2025` — `ws/chat/<role>/<email>/`, per-user rooms, admin room
  switching, `read_messages` → `messages_read` broadcast
- `minicom-05-02-2026` — `ws/chat/<email>?role=`, plus admin presence tracking
  and an `admin_status` broadcast on connect/disconnect

### Reproduce

```sh
jails commands | grep -iE 'socket|websocket' ; echo "exit=$?"
```

```
exit=1        # no kind, no capability, no subcommand
```

The nearest thing is one-directional:

```sh
jails add sse && jails routes | grep stream
```

```
GET     /events/{topic}/stream             EventStreamController#stream
```

which covers the server→client half of read receipts and presence and none of
the client→server half. Everything above was written by hand outside jails.

### Two separable pieces, and the second is the valuable one

1. **A `WebSocketHandler`-shaped kind** — the handler, its
   `WebSocketConfigurer` registration, and a test. Mechanical; the same shape
   as `g handler`.
2. **A presence primitive.** `minicom-05-02-2026/django/minicom/consumers.py`
   tracks admin presence in a module-level dict and says in a comment why that
   is allowed:

   ```python
   # Module-level presence tracker: { group_name: set(channel_names) }
   # Works because InMemoryChannelLayer = single Daphne process.
   ```

   — which is to say the author knew it was wrong and shipped it anyway. That
   is the same class of "the default is wrong in a way nothing reports" that
   `g auth` and `add sse` exist for: an in-memory presence map is silently
   correct on one node and silently wrong on two, with no error either way.

## M5 — a query cannot join, so no endpoint keyed by a natural key works

`g query --on X` filters on X's own columns by equality. Every real read in
these apps crosses a table.

### Reproduce

In a project with `User(id, email)`, `Message(id, toUserId, …)` and the
association between them — which is what `minicom-2026-02-05` is:

```sh
jails g association MessageRecipient toUserId=id --on Message --yields User   # succeeds
jails g query MessagesByEmail email:string! --on Message --pretend
```

```
jails: query MessagesByEmail filters `email`, but Message has no component with that name
```

The refusal is correct and there is no flag that changes it. There is also no
ordering or bound:

```sh
jails g query RecentMessages toUserId:long --on Message --limit 20
```

```
error: unexpected argument '--limit' found
```

### What each original endpoint needs

| original endpoint | what it needs |
|---|---|
| `POST /customer_api/ping {email}` | `users ⋈ messages` — unread messages for the user *with that email* |
| `GET /messages` (node) | each message with its author's `{id, email}` embedded |
| `GET /api/conversations/` | 20 most recent conversations, each with its **last** message |
| `GET /admin_api/issues` | issues with `user.email` — the Django says `select_related('user')` |

The first is the whole customer-facing surface of `minicom-05-02-2026`. jails
generated `UnreadMessagesForUserQuery(toUserId, read)` — correct, and reachable
only by a caller who already knows the surrogate id, which no minicom client
does.

### The information needed is already recorded

`g association` reads both records and type-checks the field mapping across the
boundary; that is exactly what a join needs, and it is used today only to emit
a foreign key. A `--via <Association>` on `g query`, letting one filter name a
column on the parent, would cover all four rows above without inventing a query
language:

```sh
jails g query UnreadForEmail email:string! read:boolean --on Message --via MessageRecipient
```

Ordering and bounds are a separate, smaller ask (`--order-by`, `--limit`).
`GET /api/conversations/` is `[:20]` ordered by `-created_at`;
`User.unread_count()` is a `count()`. Both are hand-written today.

## M6 — no get-or-create, and it is the first line of three of the six apps

```python
User.get_or_create_from_email(email)     # minicom-05-02-2026, on every ping
await User.upsert({ id: 1, email: … })   # mc-01-06-2026, on every request
conv = Conversation.objects.create() if not conv_id else …   # mc-16-11-2025
```

### Reproduce

```sh
jails commands | grep -iE 'upsert|get-or-create|find-or|ensure' ; echo "exit=$?"
```

```
exit=1
```

`g usecase` is the only create verb and it always inserts — see the M3
transcript: `repository.save(...)` with no conflict clause anywhere. On a
column with `@unique` (which `email` has, and must have) the second call is a
constraint violation, not a fetch.

### Why `g idempotency` is not it

```sh
jails explain idempotency
```

```
idempotency  At-most-once execution with a retained result: receipt store, guard, table.

  A `@unique` column on the key already gives you one row per key. What it does
  not give you is the *retained result* …
```

`explain` is right that it is a different primitive — it keys on a hash of the
request, not on a natural key of the row, and it stores a receipt beside the
data rather than returning the row. But the *statement* it already knows how to
write is the one this needs:

```sql
insert into users (email) values (?) on conflict (email) do nothing returning *
```

which `explain idempotency` itself describes verbatim: "The claim is one
`insert ... on conflict do nothing returning`. Select-then-insert leaves a
window where two callers both see nothing and both proceed."

So the shape exists; what is missing is a verb that applies it to a scaffold's
own unique key — `jails g ensure User email:string!@unique --on User`, or a
`--on-conflict <field>` on `g usecase`. This is the single most repeated
hand-written line across the six projects.

## M7 — `g client` has one fixed shape, and it is a REST collection

### Reproduce

```sh
jails g client OpenAiChatClient
cat src/main/java/com/x/clients/OpenAiChatClient.java
```

```java
public interface OpenAiChatClient {
    @GetExchange("/open-ai-chats")      List<OpenAiChatPayload> findAll();
    @GetExchange("/open-ai-chats/{id}") OpenAiChatPayload findById(@PathVariable String id);
    record OpenAiChatPayload(String id, String name) {}
}
```

The call `minicom-05-02-2026/django/minicom/ai_service.py` actually makes is
`POST /v1/chat/completions` with a JSON body of `{role, content}` messages and
a `model`/`temperature`/`max_tokens` envelope. Nothing above survives: not the
verb, not the path, not the arguments, not the return type. 100% overwritten.

What *is* worth keeping is what the same command wrote alongside it —
`HttpClientsConfig`, the `spring-boot-starter-restclient` splice, and the
`spring.http.serviceclient.*.base-url` convention. That splice is the
non-obvious part the module's own docs flag (`@ImportHttpServices` builds the
proxies without it, and the first call dies on `URI with undefined scheme`).

### The fix already exists on a sibling generator

`g controller` takes exactly the three arguments this needs:

```sh
jails g controller Verify --method post --on ChatRequest --returns ChatResponse
```

`g client` taking `--method` / `--on` / `--returns` (and a path, see M8) would
make it generate the call the project makes rather than a shape to delete.

## M8 — no way to name a route path

### Reproduce

```sh
jails g --help | grep -cE -- '--path|--route|--url|--mapping'
```

```
0
```

Paths are derived from the generated name, everywhere:

```sh
jails routes
```

```
POST    /actions/escalate-issue            EscalateIssueController#execute
PUT     /actions/mark-message-read         MarkMessageReadController#execute
POST    /actions/send-message              SendMessageController#execute
POST    /queries/unread-messages-for-user  UnreadMessagesForUserQueryController#execute
GET     /users                             UserController#list
```

The originals are not derivable from any name:

```
/customer_api/ping        /customer_api/read
/admin_api/messages       /admin_api/users        /admin_api/issues
/api/customer/message/    /api/agent/reply/       /api/conversations/
```

For a greenfield service the convention is a virtue — one shape, and `destroy`
can find what `generate` wrote. For **porting an existing service, or writing a
new server against an existing frontend, the URLs are a fixed external
contract** and jails cannot meet it. `foo-website/foo.js` and
`bar-website/bar.js` in every checkout hardcode theirs.

### The exception that proves it

`POST /foo` and `POST /bar` are the whole acceptance test in every minicom
README ("verify that an alert with `Yay! Everything works` fires"). jails gets
half of it, by luck of the singular name:

```sh
jails g record Verification success:boolean
jails g controller Bar --method post --returns Verification
sed -n '/class BarController/,$p' src/main/java/com/x/web/BarController.java
```

```java
class BarController {
    @PostMapping("/bar")                       // ← right path, for free
    Verification post() {
        throw new UnsupportedOperationException(
                "todo: build the Verification this route answers with");
    }
}
```

The refusal is the right call — jails cannot know what a `Verification` should
contain. But it means the two-line `{"success": true}` that the whole minicom
setup is graded on is not something jails can produce.

### The derivability argument does not block this

`destroy` finds files by what the ledger recorded, not by recomputing paths
(`CLAUDE.md`: "`destroy` acts on what the store recorded, and nothing else …
`KIND_FILES` is deleted"). A `--path` recorded as a value is no harder to undo
than a `--package` is meant to be. Wanted on `g controller`, `g usecase`,
`g query`.

## M9 — no way to add an index to an existing table

`--index` exists on `g scaffold` and `@index` on a field, both at creation
time. Afterwards there is nothing.

### Reproduce

```sh
jails g scaffold Message id:long@pk userId:long@index customerId:long? content:string!
jails g migration add_customer_id_index
cat src/main/resources/db/migration/V00*__add_customer_id_index.sql
```

```sql
-- Forward-only migration. Write explicit SQL below.
```

That is the whole file. `mc-01-06-2026`'s third migration is exactly this on a
live table:

```ts
await queryInterface.addIndex('messages', ['customer_id']);
```

`g field` can already add a *column* to a live table with a data plan
(`--default-literal` / `--backfill-file`), which is the harder problem — an
index has no data plan to argue about. `sql::validate_index` already parses
`'created_at desc'` into column plus ordering for the `--index` flag, so the
validation half exists too.

Wanted: `jails g index MessagesByCustomer 'customer_id' --on Message`, or
`--index` accepted on `g field` / a `resource index` verb.

## M10 — no seed or fixture data path

### Reproduce

```sh
jails add db
jails commands | grep -icE 'seed|fixture'      # 0
ls src/main/resources/db/migration/            # .gitkeep only
ls src/test/resources/fixtures/                # messages.json, users.json — test scope only
```

jails writes `src/test/resources/fixtures/*.json` for generated tests and
nothing for `dev`. There is no `db/seed` convention, no `jails db seed`, and
`add db` writes no `V00X__seed_*.sql`.

This is why `mc-01-06-2026` does database writes inside a `GET` handler:

```ts
async function listMessages(_req, res) {
  await ensureSeedUsers();      // User.upsert(...) x3, on every request
  await ensureSeedMessages();   // Message.bulkCreate(...), on every request
  …
}
```

Lower severity than the rest — it is a convention, not a mechanism — but the
convention's absence is what pushed a write into a read path.

## M11 — `transition` requires a `version` column the original schema does not have

Marking a message read is, in all three Django apps, `is_read = True; save()`.

### Reproduce

```sh
jails g scaffold Message id:long@pk content:string! isRead:boolean
jails g transition MarkRead id:long isRead:boolean --on Message --pretend
```

```
jails: transition MarkRead needs a required numeric `version` field
```

```sh
jails g field Message version:long --pretend
```

```
jails: required field `version` needs a data plan for existing rows.
       fix: pass `--default-literal <typed-value>` or `--backfill-file <project-path>`.
```

So the working sequence is two commands and a column the schema's owner did not
ask for:

```sh
jails g field Message version:long --default-literal 0
jails g transition MarkRead id:long isRead:boolean version:long --on Message
```

Both refusals are good ones and `explain transition` argues the compare-and-set
case well. The friction is that there is no unguarded alternative, so a schema
being *ported* grows a column to satisfy the tool. Worth either an
`--unguarded` that states in the generated Javadoc what was given up, or a line
in `explain transition` naming `g usecase` plus a manual update as the escape
hatch. Recording it as friction, not as a defect.

## Two smaller things, with their one-line checks

**`g strategy` generates no evaluator, and has no ordering.**

```sh
grep -rn 'List<BotRule>' src/main/java/ | grep -v '\*'      # nothing outside Javadoc
grep -rn '@Order\|Ordered' src/main/java/com/x/domain/       # nothing
```

The port's Javadoc shows the fold you are meant to write, and `--yields` makes
the return shape unambiguous, so the fold is derivable. Ordering matters here
specifically: `FallbackBotRule` must run last or it swallows every message, and
nothing in the generated code says so.

**A `usecase` defaults an enum positionally.**

```sh
jails g enum IssueStatus OPEN IN_PROGRESS RESOLVED
jails g scaffold Issue id:long@pk summary:string! status:IssueStatus
jails g usecase EscalateIssue summary:string! --on Issue
grep -n 'IssueStatus' src/main/java/com/x/service/DefaultEscalateIssueUseCase.java
```

```java
IssueStatus.values()[0],
```

It happened to be `OPEN`, which is what the Django `default='OPEN'` says — by
luck of declaration order. Reordering the `g enum` arguments silently changes
the default of every generated create, and no test would notice.

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

M1 and M2 were different: defects rather than absences, and both invisible to
the suite for the same reason — the golden scenarios exercise one kind on one
flavour, and each bug needs a second thing present. M1 is closed, along with
the blind spot behind it.

---

## Where the evidence lives

The six builds are at `/home/laith/code/minicom-jails/`, each a plain Maven
project — `cd` into one and `jails build`. The command log per project is its
`.jails/ledger.toml`; `jails history` prints it.

Every transcript above was produced by running the commands as written, on
`jails 0.1.0` built from `9aac1b0`, JDK 26.0.2, Maven via `./mvnw`. The M2
Gradle run used `JAVA_HOME=…/openjdk-21.0.2` because Gradle 8.5 cannot run on
JDK 26 (`Unsupported class file major version 70`) — that part is the
checkout's age, not a jails problem.

The M2 module claims were checked against `deps/spring-boot`, not from memory:

```sh
cd deps/spring-boot
git ls-tree -r --name-only v2.7.18 | grep -c spring-boot-flyway          # 0
git ls-tree -r --name-only v2.7.18 | grep -c spring-boot-testcontainers  # 0
```
