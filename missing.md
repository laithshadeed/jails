# missing.md — what eight real minicom projects needed and jails could not give

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
| `minicom-05-02-2026`, second deeper pass | Django + Channels + OpenAI | `mc2-full` | **green**, 70 tests |
| `minicom-15-01-2026` (4 backends) | Django / Rails / Node / Spring | `mc-15-01` | **green**, 70 tests, **0 of 10 endpoints match** |

A seventh check ran `jails` inside the *unmodified* `mc-01-06-2026/spring`
tree — Gradle 8.5, Spring Boot 2.7.18, Java 21 — to test the foreign-project
path. That was M2, **now closed**: `add db` reads the project's Boot version
and picks the module set that version has. Boot 4 gets `spring-boot-flyway`;
Boot 3.1 upwards gets Flyway's auto-configuration where it always was, with
both Flyway artifacts pinned to one version; below 3.1 it refuses by name,
saying which module is missing and which capabilities do work. Each of the
three boundaries was checked in `deps/spring-boot`, not recalled.

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

## Case study A — Minicom 2.0, feature by feature

`minicom-05-02-2026/django/minicom/`: 13 Python files, 671 lines, doing
customer↔admin chat over WebSockets with admin presence, an OpenAI assistant
that takes over when no admin is online, and escalation into an issue queue.
The jails rebuild is `minicom-jails/mc2-full` — 106 Java files, 4,646 lines,
`BUILD SUCCESS`, 70 tests, nothing hand-written.

Taken feature by feature rather than stopping at the schema:

| what the Django app does | jails | how |
|---|---|---|
| `User`, `Message`, `Issue` + their enums | yes | `g scaffold`, `g enum` |
| FKs `message.to_user`, `issue.user` | yes | `g association` |
| `Meta.ordering = ['timestamp']` / `['-created_at']` | **no** | **M17** — every list is `order by id` |
| `User.get_or_create_from_email` | **no** | **M6** — used by 4 of the 5 endpoints |
| `unread_messages()` / `unread_count()` | partial | list yes; `count()` no — **M5** |
| `Message.mark_read()` | partial | `g transition`, after adding `version` — **M11** |
| `POST /customer_api/ping {email}` | **no** | get-or-create **and** a join on email — M5, M6, M8 |
| `POST /customer_api/read {email, message_id}` | partial | keyed by id, not email |
| `POST /admin_api/messages {email, content}` | partial | keyed by `toUserId`, not email |
| `GET /admin_api/users` | yes | the scaffold's own `GET /users` |
| `GET /admin_api/issues` with `user_email` joined | **no** | `select_related('user')` — **M5** |
| 400/404 error bodies | yes | `add api` |
| Django admin at `/admin/` | **no** | **M18** — no back-office surface |
| CORS | yes | `add cors` |
| `ws/chat/<email>/?role=`, groups, `history`, broadcast | **no** | **M4** |
| admin presence + `admin_status` broadcast | **no** | **M4** |
| bot greeting / "no admin → let the AI answer" | **no** | domain logic |
| `POST /v1/chat/completions` to OpenAI | **no** | **M7** |
| `SYSTEM_PROMPT`, history mapping, `parse_escalation` | **no** | domain logic (fair) |
| create the `Issue` on escalation | yes | `g usecase … --on Issue` |
| `OPENAI_API_KEY` from the environment | partial | `jails set`; no secret-shaped setting |

**10 covered, 6 partial, 14 not.** By weight the split is cleaner than the
count: **jails built the entire persistence and CRUD half and none of the
transport or conversation half.** The WebSocket rows are one absence (M4) and
the two email-keyed endpoints are another (M5/M6), and between them they are
what makes this a chat product rather than a table with a REST face.

Prompt text and response parsing are honest non-goals. An outbound `POST` with
a typed body is not — see M7.

---

## Case study B — `minicom-15-01-2026`, and what "100% feature complete" costs

This checkout is the pristine old-generation skeleton with **four backends
implementing one contract** — Django, Rails, Node and Spring — plus two static
websites on `:8010` and `:8011` that call it on `:3001`. jails only emits Java,
so the question it can be asked is: *can one jails-built Spring service satisfy
the contract all four implement?*

The contract, as the two shipped websites call it:

```
POST   /customer_api/ping                          form {email}
POST   /customer_api/read                          form {message_id}
POST   /customer_api/messages                      form {email, content, category?, priority?}
GET    /admin_api/users?status=&category=&priority=
POST   /admin_api/messages                         form {email, content}
GET    /admin_api/messages/:user_id
GET    /admin_api/conversations?status=&category=
PATCH  /admin_api/conversations/:user_id/status    json {status}
PATCH  /admin_api/conversations/:user_id/category  json {category}
PATCH  /admin_api/conversations/:user_id/priority  json {priority}
```

`mc-15-01` models the whole domain — `User`, `Message`, `Conversation`, four
enums, two associations, four transitions, four queries, two use cases —
and builds green with 70 tests. Then:

```sh
jails routes
```

```
PUT   /actions/mark-read              POST  /queries/conversations-by-status
POST  /actions/post-message           POST  /queries/messages-for-user
PUT   /actions/set-status             GET   /users
PUT   /actions/set-category           GET   /messages
PUT   /actions/set-priority           GET   /conversations
…22 routes
```

**Zero of the ten match.** Not one path, and the ones that are close are the
wrong verb. The two websites cannot talk to it at all, and they are not files
you are allowed to rewrite — they are the fixed side of the contract.

Four separate causes, three of them new:

- **the paths are derived** — M8
- **the requests are form-encoded**, and jails only binds `@RequestBody` JSON — **M15**
- **three of the four enums have wire values jails cannot spell**: `open`,
  `in_progress` (lowercase), `Product`, `Billing` (TitleCase), and `-`, `!`,
  `!!` (not identifiers at all) — **M14**
- **the admin filters are optional** — any subset of status/category/priority —
  and `g query` takes required scalars only — **M16**

This is the clearest answer available to "can jails do 100%": for a service
with an existing client, **the domain half is free and the contract half is
unreachable**, and that is a property of four missing knobs rather than of any
deep design commitment.

---

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

## M7 — `g client` ignores `--method`, `--on` and `--returns` without saying so

Two problems. The shape is fixed to a REST collection, and — the worse half —
the flags that would change it are **accepted and silently discarded**:

```sh
jails g record Rq a:string!
jails g client Gamma --method post --on Rq        # exit 0, reports success
grep -c PostExchange src/main/java/com/x/clients/GammaClient.java
```

```
0
```

No refusal, no warning, no `PostExchange`, and `Rq` is referenced nowhere. That
is the failure class jails is otherwise scrupulous about — "an unknown marker
is an error, not a no-op" — and `g controller` honours the very same three
flags. Refusing them here would at least be honest; today the command reports
success for work it did not do.

### Reproduce the fixed shape

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

`g client` already *takes* those three flags. Honouring them — plus a path, see
M8 — would make it generate the call the project makes rather than a shape to
delete. See also **M13**: a second `g client` breaks the first.

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

## M12 — a fully applied transaction exits 1 because an external effect failed

`add db` writes every file, records the ledger, and then returns a failure
status because it could not start the compose service.

```sh
jails new p --package com.x --offline --no-git && cd p
jails add db ; echo "EXIT=$?"
```

```
  create  compose.yaml
  create  jails.toml
  replace pom.xml
  …
  ledger  create
  effect  compose reconcile (1 up, 0 stopped) (failed)
EXIT=1
```

The project is correct — `compose.yaml`, `jails.toml` and the migration
directory are all there, and a second run says `nothing to do` and exits 0.
Only the side effect failed.

Two things follow. In a script — `for c in db api cors json sse; do jails add
$c || fail; done`, which is how I installed capabilities on six projects —
this reads as a failed install of the one capability that actually succeeded.
And the natural response is to re-run it, which is a no-op that reports
success, so the operator learns to ignore the status.

**`--no-start` exists and fixes it** (`jails add db --no-start` → `EXIT=0`),
but the failure line names neither the cause nor the flag. Everywhere else
jails puts a `fix:` on a refusal; this line has none.

The narrow fix is a distinct exit status for "applied, effect failed" — or, at
minimum, `fix: jails add db --no-start` on that line.

---

## M13 — `g client` is not additive: a second client silently breaks the first

`@ImportHttpServices` carries **one group name**, jails regenerates the single
shared `HttpClientsConfig` with the newest client's name, and `basePackages`
scans every client into that one group. So every previously generated client
loses its configuration, with no error at generate time.

```sh
jails g client Alpha
grep -o 'group = "[a-z-]*"' src/main/java/com/x/clients/HttpClientsConfig.java   # "alpha"
jails g client Beta
grep -o 'group = "[a-z-]*"' src/main/java/com/x/clients/HttpClientsConfig.java   # "beta"
jails build
```

```
AlphaClientTest.findAllReadsTheCollection <<< ERROR!
org.springframework.web.client.ResourceAccessException:
  I/O error on GET request for "https://example.invalid/alphas"
BetaClientTest — Tests run: 2, Failures: 0, Errors: 0
```

Alpha's own test sets `spring.http.serviceclient.alpha.base-url` through
`@DynamicPropertySource`, but Alpha is now registered under group `beta`, so
the dynamic override never reaches it and the `https://example.invalid`
placeholder from `application.properties` wins. Beta passes. **The newest
client always works and every older one is broken.**

Three clients is three broken tests and one passing one. `destroy client Beta`
does not restore `alpha` either — the only jails-only repair is to destroy and
regenerate the client you want last.

The fix is one group per client (a config class per client, or
`@ImportHttpServices` listed by type rather than by package scan).

---

## M14 — enum constants are silently uppercased, and there is no wire value

`explain enum` says "a closed set of named constants, **stored by name**".
There is no way to give a constant a different serialized form, and jails
rewrites what you pass without saying so.

```sh
jails g enum Status open in_progress resolved closed
grep -A4 'enum Status' src/main/java/com/x/domain/Status.java
```

```java
public enum Status {
    OPEN,
    IN_PROGRESS,
    RESOLVED,
    CLOSED
```

I asked for `open` and got `OPEN`, with no warning. The generated API then
emits `"OPEN"` where the shipped admin website sends and expects `"open"`.
Same for `Product`/`Billing` → `PRODUCT`/`BILLING`.

And the third enum cannot be expressed at all:

```sh
jails g enum Priority - '!' '!!'
```

```
jails: name `-` starts with `-`; a Java identifier starts with a letter, `_` or `$`
```

That refusal is correct — those are not Java identifiers — but there is no
`@value("-")` to attach the wire form to a legal constant name either.

All three of `minicom-15-01-2026`'s enums are unrepresentable on the wire, and
enum-valued columns are exactly where a ported service meets an existing
client. The silent uppercasing is the worse half: an unknown field marker is an
error in jails, but an unspellable enum constant is quietly rewritten.

---

## M15 — every generated endpoint binds JSON, and the clients post forms

Each of the eight websites across these checkouts calls the backend with
jQuery's `$.post(url, {email})`, which sends
`application/x-www-form-urlencoded`. Every jails controller binds
`@RequestBody`:

```java
@PostMapping
public ResponseEntity<MessageResponse> execute(@Valid @RequestBody PostMessageCommand command)
```

```java
@PostMapping
public ResponseEntity<UserResponse> create(@Valid @RequestBody UserRequest request)
```

There is no flag on `g controller`, `g usecase` or `g query` to bind
`@ModelAttribute` / `@RequestParam` instead, so a jails endpoint answers a form
post with 415. The Spring backend that ships in `minicom-15-01-2026` binds
`@RequestParam Map<String, String>`, which is what the frontends need.

JSON is the right default. What is missing is the ability to say otherwise for
a service whose callers already exist.

---

## M16 — query filters are all-or-nothing, so no filtered list view works

```sh
jails g query UsersFiltered 'status:Status?' 'category:Category?' --on Conversation
```

```
jails: query UsersFiltered filter `status` is optional or a collection. This first
       query contract only accepts required scalar equality filters so null/list
       semantics are never guessed.
```

The refusal names itself "this first query contract", so the limit is known.
But an inbox filter bar is the ordinary case: `minicom-15-01-2026`'s admin
website sends any subset of `?status=&category=&priority=`, which is eight
combinations. jails can express exactly one of them per generated query, and
the unfiltered list is a different endpoint again.

`sql::Column` already knows each filter's column and nullability, which is what
"absent means no predicate" needs. The semantics that must not be guessed are
`IS NULL` versus "no filter" — naming that explicitly (`--optional-filter`, or
`?` meaning "omit the predicate when absent") is the whole feature.

---

## M17 — list ordering is fixed at `order by id`

Every generated read orders by the primary key, in both the repository and the
query adapter:

```java
// Ordered explicitly: SQL does not otherwise promise row order.
select … from messages order by id
```

The comment is right that an order is needed. The problem is that it is the
only one available, and `jails g --help` has no `--order-by`.

`minicom-05-02-2026` needs `ordering = ['timestamp']` on messages and
`['-created_at']` on issues. The second is not merely unsupported, it comes out
**backwards** — the escalated-issues panel is meant to show newest first and
`order by id` ascending shows oldest first, which looks like working software.

---

## M18 — no back-office surface

Every Django checkout registers its models with the Django admin — list
columns, filters, search fields, read-only keys — in about twenty lines:

```python
@admin.register(Issue)
class IssueAdmin(admin.ModelAdmin):
    list_display = ('id', 'user', 'issue_summary', 'status', 'created_at')
    list_filter = ('status',)
    search_fields = ('user__email', 'issue_summary')
```

```sh
jails commands | grep -icE 'admin|crud|backoffice'    # 0
```

jails generates the REST surface and no operator surface. This is a defensible
scope line — an admin UI is a product, not scaffolding — but it is worth
recording that it is the one thing every Django port gets for free and every
jails port does not, and that `jails.toml` plus the ledger already know the
field model such a view would need.

---

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

The scoreboard is 7 of 8 green with **zero** hand-written Java, on eight real
apps written by as many people in four languages. The layering, the
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

The second cluster is smaller and cheaper: **M8, M14, M15 and M16 are four
missing knobs** — a route path, an enum's wire value, a form binding, an
optional filter. None of them asks jails to understand anything new. Together
they are the whole reason `mc-15-01` matches zero of ten endpoints while
modelling the domain perfectly, and they are what separates "scaffolds a new
service" from "can be pointed at an existing client".

M1 and M2 were different again: defects rather than absences, and both
invisible to the suite for the same reason — the golden scenarios exercise one
kind on one flavour, and each bug needs a second thing present. Both are
closed, along with the blind spot behind them. **M7, M12 and M13 are the same
species and still open**, and all three share one symptom worth naming: a
command that reports success for something it did not do.

---

## Where the evidence lives

The eight builds are at `/home/laith/code/minicom-jails/`, each a plain Maven
project — `cd` into one and `jails build`. The command log per project is its
`.jails/ledger.toml`; `jails history` prints it.

Every transcript above was produced by running the commands as written, JDK
26.0.2, Maven via `./mvnw`. **M3–M11 were recorded against `9aac1b0`; M12–M18
were recorded and every earlier entry re-checked against `d1e2185`** — the tree
moved under this document while it was being written, which is why the versions
are stated. One thing found on the way is deliberately not filed: between
`3b58d17` and `d1e2185`, `g query` emitted a port whose declared type did not
match its filename, so every query broke the build. `d1e2185` closed it. The M2
Gradle run used `JAVA_HOME=…/openjdk-21.0.2` because Gradle 8.5 cannot run on
JDK 26 (`Unsupported class file major version 70`) — that part is the
checkout's age, not a jails problem.

The M2 module claims were checked against `deps/spring-boot`, not from memory:

```sh
cd deps/spring-boot
git ls-tree -r --name-only v2.7.18 | grep -c spring-boot-flyway          # 0
git ls-tree -r --name-only v2.7.18 | grep -c spring-boot-testcontainers  # 0
```
