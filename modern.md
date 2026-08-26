# modern.md — what "modern idiomatic 2026 Java" would have looked like

Notes from assessing `my-minicom/`, the minicom clone built with jails, against
`java.md` and `backend.md`. No jails code was changed. Everything below is
either quoted from the generated tree or measured against it.

To keep this honest I did not only read the output — I built the same slice by
hand and ran it. That reference is referred to throughout as **the hand-built
slice**; its essential files are reproduced in §10. It compiles against
`my-minicom`'s own `pom.xml` (Boot 4.1.1, Java 26) and its six behaviour tests
pass.

**§1–§12 are about `my-minicom`. §13 is about the six other attempts at the
same app in `~/code/minicom-jails`**, all of which were also built and run.
That second corpus is what separates *"jails rendered a bad field spec
faithfully"* from *"jails does this whatever you type"* — §13.11 is the split,
and the right place to start.

---

## 1. The verdict

The generated code is *well-explained wrong code*. It has the vocabulary of a
staff engineer — ports and adapters, JSpecify, `ProblemDetail`, an outbox,
optimistic versions, `MockMvcTester` — and underneath it the schema has no
primary key, the domain record has snake_case components, the closed set is a
`String`, and one of the two error models it ships is dead code.

The prose is the tell. 572 of 2,157 lines of production Java are comment —
**27%** — and a large fraction of it argues for a decision the code next to it
did not actually make. A comment that says "keyed on the `email` component"
sits on a class keyed on `id`. A Javadoc explaining that the Kafka key gives
"ordering per entity" sits on a publisher keyed by a value that is unique per
record. That is worse than no comment: a wrong explanation is believed, and the
next reader stops looking.

The single sentence version: **jails generates the shape of good code from
input it never questions, and then writes a justification for whatever came
out.**

---

## 2. Verified facts, before any opinion

These were run, not inferred.

| Claim | How it was checked | Result |
|---|---|---|
| `users` has no primary key | `grep "primary key" db/migration/` | Only `send_message_outbox` has one. `users` and `messages` have **none**. |
| `messages` has no primary key | same | Correct — and `V003` works around it by adding `create unique index users_id_association_key on users (id)` purely so a foreign key has something to point at. |
| `ApiException` is dead | `grep -rn "new ApiException"` | **Nothing throws it.** The sealed hierarchy, the exhaustive `switch`, and the Javadoc explaining why the switch has no `default` are all unreachable. |
| `AppMetrics` is dead | `grep -rn AppMetrics src/main` | Referenced only by its own test. |
| The event id is the message id | `OutboxSendMessageUseCase:30-31` | `result.id()` passed as **both** `id` and `messageId`. |
| snake_case reached Java | `grep -rhoE '\b[a-z]+_[a-z_]+\b'` | `user_id` ×32, `is_read` ×25, `time_stamp` ×17 in production Java. |
| Unused imports ship | `JdbcMarkAsReadTransition.java` | Imports `java.sql.Timestamp` and `java.time.Instant`; uses neither. Same in `JdbcUnreadMessagesQuery`. |

---

## 3. Naming — the loudest problem, and the cheapest to fix

### 3.1 snake_case components in a Java record

```java
public record Message(UUID user_id, String message, boolean is_read,
                      String direction, Instant time_stamp, UUID id, long version) {
```

`message.is_read()`. `message.time_stamp()`. `command.user_id()`. This is not a
style quibble — it is the first thing any Java reviewer sees, and it makes the
whole tree read as machine output rather than as code someone wrote.

The cause is mechanical: the field spec was typed as `user_id:uuid
is_read:boolean time_stamp:timestamptz`, and jails renders the spec name
verbatim as the Java component name *and* the SQL column name. jails' own
`examples/minicom/.jails/app.toml` uses `userId`, `isRead`, `timeStamp` — so
the tool is capable of the right output and simply has no opinion about which
it gets.

**What it should do:** the spec name is a *concept*, and the two renderings are
different. `userId` → Java `userId`, SQL `user_id`. `user_id` → Java `userId`,
SQL `user_id`. Both spellings converge, and a Java identifier that cannot be
produced by convention is the only case worth an error. There is no reading of
"idiomatic Java" in which `is_read()` is an accessor.

### 3.2 Names that carry no meaning

| Generated | Problem | Better |
|---|---|---|
| `Message.message()` | The field has the same name as its type. Reads as `message.message()`. | `body` — which is what both the Rails `t.text "message"` and jails' own example manifest call it |
| `time_stamp` | Names the *datatype*, not the event. Every column is a timestamp. | `sentAt` |
| `MessageService` | Does nothing but forward four calls | delete it, or name the thing it actually coordinates |
| `Message_userAssociationIT` | An underscore in a Java class name | `MessageBelongsToUserIT` |
| `MyMinicomApplication` | "My" is the directory name, not the system's | `MinicomApplication` |
| `UnreadMessagesQuery` / `UnreadMessagesQueryPort` | The params record and the interface differ by the suffix `Port`. Neither name says which is which. | `ConversationPage` / the repository method that returns it |
| `JdbcMarkAsReadTransition` | A JDBC class implementing a *use case* interface | see §6.2 |
| `DefaultSendMessageUseCase` | `Default` is what you name something when you have not decided what it is | `Conversation.send(…)` |

`backend.md` §8 bans `Helper`, `Manager`, `Util`, `Processor` because they are
"a bag of unrelated functions with no invariant." `Default…UseCase` and
`…QueryPort` are the same failure wearing hexagonal-architecture clothes.

### 3.3 Three naming conventions inside one five-component record

```java
public record MessageCreatedEvent(UUID id, UUID messageId, UUID user_id,
                                  String message, Instant occurredAt) {
```

`id`, `messageId` (camel), `user_id` (snake), `occurredAt` (camel). Two of
these came from a template and two from the field spec, and nothing reconciled
them.

---

## 4. Migrations — the part that is actually dangerous

### 4.1 No primary keys

```sql
-- V001__create_users.sql
create table users (
  id     uuid        not null,     -- not a primary key
  email  text        not null unique,
  name   text
);

-- V002__create_messages.sql   (no id column at all)
create table messages (
  user_id     uuid        not null,
  message     text        not null,
  is_read     boolean     not null,
  direction   text        not null,
  time_stamp  timestamptz not null
);

-- V004__add_id_to_messages.sql
alter table messages add column id uuid default gen_random_uuid() not null;
```

`messages.id` is a nullable-free `uuid` column with no unique constraint and no
index. Everything downstream assumes it is a key:

- `JdbcMessageRepository.findById` does `where id = :id` and calls `.optional()`
  — which throws if two rows ever match.
- `JdbcMarkAsReadTransition` runs `update … where id = :id and version = :version`
  as a compare-and-swap. Without uniqueness that is a **multi-row update
  presented as an atomic single-row CAS**.
- `V003` adds `create unique index users_id_association_key on users (id)`
  purely so the foreign key has a target. The right fix — `primary key` — was
  available and one word shorter.

`backend.md` §5: *"The schema is the last line of defence and the cheapest
one."* Here it is not a line of defence at all.

### 4.2 uuidv4 as a key, on PostgreSQL 18

`gen_random_uuid()` in the migration and `UUID.randomUUID()` in Java are both
v4. `backend.md` §5 names this exactly: *"Do not use `uuidv4` /
`gen_random_uuid()` for a primary key on a large table — random UUIDs destroy
b-tree locality."* PostgreSQL 18 ships `uuidv7()`; the project runs Postgres 17
in tests and has no reason to.

### 4.3 No index serves any query the application runs

`JdbcUnreadMessagesQuery` runs:

```sql
where user_id = :user_id and is_read = :is_read order by id limit 100
```

There is no index on `(user_id, is_read)`, none on `user_id`, none on
`time_stamp`. Every unread lookup is a sequential scan and a sort. The example
manifest asks for `indexes = ["user_id, time_stamp desc"]`; this one did not,
and jails did not mention it.

### 4.4 `order by id` on a random UUID

The list endpoint, the unread query and `findAll` all `order by id`. With v4
ids that is a **stable random order** presented to a user as their conversation.
Messages must order by `sent_at desc`. This is the defect a reader is most
likely to notice as a user and least likely to find in the code.

### 4.5 A closed set stored as free text

`direction text not null` — no `check`. `Message.direction` is a `String` whose
compact constructor only rejects blank. `'banana'` is a valid direction at
every layer. The example manifest models it as
`enum MessageDirection { TO_USER, FROM_USER }`; this project typed `direction:String!`
and jails accepted it without comment.

### 4.6 Column order records generation history

`user_id, message, is_read, direction, time_stamp, id, version` — the identity
column is sixth and the version seventh because they were appended by later
`alter table`s. Cosmetic in Postgres, but it is the order the record, the DTO,
the request, the response, the row mapper and the `COLUMNS` constant all
inherit, so every file in the tree reads back-to-front.

### 4.7 Things a schema this size should have and does not

- `created_at` / `updated_at` on either table (`timestamps = true` exists in
  jails and was not used).
- `check (length(btrim(body)) > 0)` — the Java constructor enforces non-blank,
  the database does not, so any import path bypasses it.
- A case-insensitive unique index on `email`. As written, `A@b.com` and
  `a@b.com` are two accounts.
- `on delete cascade` (or an explicit `restrict`) on the message→user FK. The
  generated one is `on delete no action … deferrable initially deferred`, an
  unusual choice with no comment saying why.

---

## 5. Type modelling — where the Java is furthest from 2026

`java.md` §1 calls data modelling "the whole game": records plus sealed
interfaces, then exhaustive `switch`. The generated tree uses records for
*carrying* data and nothing else. Every type distinction is a `String`, a
`UUID`, or a `boolean`.

### 5.1 Everything is a `UUID`

```java
public record Message(UUID user_id, …, UUID id, long version)
```

`markRead(userId, messageId)` and `markRead(messageId, userId)` both compile.
A one-line wrapper removes the entire class of mistake:

```java
public record UserId(UUID value)    { public UserId    { requireNonNull(value); } }
public record MessageId(UUID value) { public MessageId { requireNonNull(value); } }
```

### 5.2 The `String` id that is not a `String`

`MessageRepository.findById(String id)` while `Message.id` is a `UUID` — so the
adapter has to write `where id = cast(:id as uuid)`, the in-memory fake keys a
`Map<String, Message>` with `String.valueOf(message.id())`, and every test says
`repository.findById(String.valueOf(created.id()))`. `UserRepository.findById`
takes a `UUID`. Two ports over two tables in one application disagree about how
identity is typed.

### 5.3 Expected outcomes modelled as exceptions

```java
public interface MarkAsReadUseCase {
    Message execute(MarkAsReadCommand command);
    final class NotFoundException extends RuntimeException {
        public NotFoundException() { super("resource not found in the authorized scope"); }
    }
    final class StaleVersionException extends RuntimeException { … }
}
```

Three problems in nine lines:

1. **These are outcomes, not faults.** `java.md` §5: *"Domain failures that are
   expected are not exceptions. Model them in the return type."* A caller that
   forgets a `catch` finds out in production; a caller that forgets a `switch`
   arm does not compile.
2. **No values in the message.** `backend.md` §1: *"Exception messages carry the
   values."* `"resource not found in the authorized scope"` names neither the
   resource nor the id. It is the same string for every 404 the service will
   ever serve.
3. **"in the authorized scope" is not true.** The SQL is `where id = :id`. There
   is no scope, no tenant, no authorization. The local variable is called
   `existsInScope` and the class comment says *"scoped matches cannot mutate
   another tenant's row."* Nothing in the query does that.

The hand-built version is a sealed type with four outcomes (§10.3), and the
controller's `switch` over it has no `default` — so a fifth outcome is a
compile error at every site that has to decide what it means.

### 5.4 Boxed primitives on the wire

`MessageResponse(… Boolean is_read, … Long version)` — boxed, so `null` is
representable in a response describing a `boolean` and a `long` that cannot be
null. `MessageRequest` boxes them too and then `@NotNull`s them, which is the
validation annotation compensating for the wrong type.

---

## 6. Architecture — the layering the comments describe is not the layering that exists

### 6.1 Two error models, one of which is dead

- `minicom.api.ApiException` — sealed, three variants, an exhaustive `switch`
  in `ApiExceptionHandler`, and 40 lines of Javadoc explaining why the switch
  has no `default` arm. **Nothing throws it.**
- `MarkAsReadUseCase.NotFoundException` / `StaleVersionException` — thrown by
  the JDBC adapter, caught in the controller, and rethrown as
  `ResponseStatusException`, bypassing `ProblemDetail` entirely.

So the RFC 9457 machinery `add api` installed is unused, and the one operation
with real failure modes hand-rolls its own status mapping. A reader will find
`ApiException`, believe it is the error model, and be wrong.

### 6.2 The service layer calls a JDBC class directly

```java
package minicom.service;
import minicom.jobs.JdbcSendMessageOutbox;   // <- a concrete JDBC adapter

public class OutboxSendMessageUseCase implements SendMessageUseCase {
    public OutboxSendMessageUseCase(DefaultSendMessageUseCase delegate,
                                    JdbcSendMessageOutbox outbox) { … }
```

Every port/adapter comment in this codebase says the application depends on
interfaces. This one takes a concrete `Jdbc*` class *and* a concrete sibling
implementation. `minicom.jobs.JdbcSendMessageOutbox` also imports
`minicom.adapters.Json`, so `jobs` → `adapters` and `service` → `jobs`. There
is no layering; there are eight packages and unrestricted edges between them.

### 6.3 `AppMetrics`, `CorsConfig`, `MetricsConfig` in the root package

Three configuration classes sit in `minicom` alongside the `@SpringBootApplication`
while everything else is in a layer package. No rule places them; they are
where they are because nothing decided.

### 6.4 Interfaces with one implementation

- `SendMessageOutboxSink` — one implementation, and its `name()` method is never
  called.
- `UnreadMessagesQueryPort` — one implementation, one caller.
- `SendMessageUseCase` — two implementations, but one is `@Primary` and wraps the
  other, so `List<SendMessageUseCase>` injection would silently pick up both.

`java.md` §8: *"Interfaces are extracted when there is a second implementation
or a test seam you actually need — not reflexively."* Three files
(`SendMessageUseCase`, `DefaultSendMessageUseCase`, `OutboxSendMessageUseCase`)
implement one method that appends a row and stages an event. The hand-built
version is one `@Transactional` method (§10.4).

### 6.5 Two API styles in one service

```
GET    /messages
POST   /messages
DELETE /messages/{id}
PUT    /actions/mark-as-read
POST   /actions/send-message
POST   /queries/unread-messages
```

REST for the scaffold, RPC-over-POST for the generated use cases — including a
`POST` to read (`/queries/unread-messages`), which is uncacheable, unlinkable
and unloggable-by-URL. Both styles create a message, so there are two
independent write paths into `messages` with different validation.

---

## 7. The HTTP contract is wrong in a way that would be caught in review

```java
public record MessageRequest(
        @NotNull  UUID    user_id,
        @NotBlank String  message,
        @NotNull  Boolean is_read,      // server state
        @NotBlank String  direction,
        @NotNull  Instant time_stamp,   // server clock
        @NotNull  UUID    id,           // primary key
        @NotNull  Long    version) {    // concurrency counter
```

`POST /messages` **requires the client to supply the primary key, the
timestamp, the read flag and the optimistic-lock version.** A client can post a
message that is already read, backdated, at version 900, under an id it chose.
`UserRequest` has the opposite bug — it calls `UUID.randomUUID()` inside the
web layer, so identity is minted in the HTTP adapter.

The generated test knows about this class of defect and commits it anyway. Its
own Javadoc:

> *"a collection describing a request the record refuses is a request nobody can
> make, and it shipped. A timestamped scaffold asked the caller for `createdAt`
> and `updatedAt`, so its own documented POST answered 400 naming two columns
> the create path sets itself."*

— and directly beneath it, `CREATE_REQUEST` sends `id`, `version`, `is_read`
and `time_stamp`. The lesson was written down and not applied.

Two more:

- `MarkAsReadController` and `UnreadMessagesQueryController` bind
  `minicom.service.MarkAsReadCommand` and `minicom.service.UnreadMessagesQuery`
  **directly as `@RequestBody`** — while `MessageRequest`'s Javadoc argues at
  length that the wire type must not be the domain type. The rule is stated in
  one file and broken in the next.
- `UnreadMessagesQuery(UUID user_id, boolean is_read)` — a query named *unread*
  that takes `is_read` as a parameter, so `POST /queries/unread-messages` with
  `{"is_read": true}` returns read messages.
- `MAX_RESULTS = 100`, applied silently with no cursor, no total, and no
  indication to the caller that the list was truncated.

### The version belongs in `If-Match`

`MarkAsReadCommand` carries `version` in the JSON body. HTTP already has this:
serve the version as an `ETag`, require `If-Match`, answer `412` on a mismatch.
The hand-built controller does that in §10.5 and the client gets standard
semantics instead of a bespoke field.

---

## 8. Correctness bugs, ranked

1. **The Kafka partition key is unique per record.**
   `MessageCreatedPublisher` keys on `event.id()`, and its Javadoc claims *"The
   key is the event id, which is what gives ordering per entity."* An id that is
   unique per event round-robins across every partition — the exact behaviour
   the comment says it prevents. Ordering must key on `user_id`. `backend.md` §4:
   *"The partition key is the design decision."*

2. **The event id *is* the message id.**
   `OutboxSendMessageUseCase` passes `result.id()` as both `id` and `messageId`.
   The outbox stages `on conflict (id) do nothing`, so a second event about the
   same message is **silently discarded**.

3. **`InMemoryUserRepository` cannot work.**
   ```java
   public Optional<User> findById(UUID id) {
       // TODO: this type has no `id` component …
       return Optional.empty();
   }
   public void save(User user) { items.put(String.valueOf(items.size()), user); }
   public boolean deleteById(UUID id) { return items.remove(id) != null; }  // Map<String,…>
   ```
   `findById` always empty; `save` keys on a counter that collides after any
   removal; `deleteById` removes a `UUID` from a `Map<String, User>` and is
   always `false`. The class Javadoc says the type has no `id` component —
   `User`'s first component is `UUID id`. The file was generated before `id`
   existed and never regenerated, and nothing detected the contradiction.

4. **The outbox relay ceiling is one event per second.**
   `claim()` is `limit 1`; the worker runs on `fixedDelay=PT1S` and processes
   one claim per tick. There is also no jitter on the backoff (`backend.md` §3:
   *"Exponential backoff with jitter"*), and a multi-sink partial failure
   retries every sink, so a Kafka publish that succeeded is re-sent.

5. **`MessageCreatedListener` is a `TODO`.**
   It logs an id and drops the event. Shipped.

---

## 9. Tests

**They mostly test the framework, or nothing.**

- `MessageServiceTest` stubs the port to return empty and asserts the service
  returns empty. The service is a one-line forward. The test can only fail if
  Mockito breaks.
- `Message_userAssociationIT` queries `pg_constraint` and `unnest(conkey,
  confkey)` to assert that PostgreSQL recorded the foreign key that the
  migration two files away declared. It tests Flyway and Postgres.
  `backend.md` §3: *"Don't test … Spring's wiring. Test the behaviour you'd be
  embarrassed to break."*
- `MessageTest` has **one** test: that a null `user_id` throws. Nothing covers
  blank rejection, trimming, or a `direction` that is not one of the two legal
  values — because there is no legal set to test against.
- Every fixture value is `"sample"`. `direction` is `"sample"` in the domain
  test, the controller test and both integration tests. No test in the suite
  ever exercises a real direction, so `direction:String!` costs nothing to get
  wrong.
- `CorsConfigTest` used to fail the moment the origin was configured (§2,
  closed): it asserted the placeholder rather than reading the property.

What is missing entirely: a concurrency test for the mark-as-read CAS — the one
behaviour the `version` column exists for, and the one thing the example
manifest's README claims as jails' improvement over Rails and Django. The
hand-built slice covers it in three tests (§10.6).

Two things the generated tests get right and are worth keeping: `MockMvcTester`
with AssertJ (no `throws Exception`, no two families of static imports), and
containers as `@Bean`s with `@ServiceConnection` rather than `@Container`
statics.

---

## 10. The hand-built slice

Built against `my-minicom`'s own `pom.xml`. 24 files, ~850 lines for the
user + conversation slice — not a like-for-like total, since it has no Kafka
config, metrics or CORS, but it covers everything §3–§9 is about.

```
$ mvn -o test
Tests run: 6, Failures: 0, Errors: 0, Skipped: 0
BUILD SUCCESS
```

Package by feature, not by layer — `minicom.user`, `minicom.conversation`,
`minicom.http` — so the boundary is where a module would be cut.

### 10.1 Identity is a type

```java
public record UserId(UUID value) {
    public UserId { Objects.requireNonNull(value, "value"); }
}
public record MessageId(UUID value) { … }
```

`markRead(messageId, userId)` no longer compiles.

### 10.2 The closed set is closed, and the value object validates once

```java
public enum MessageDirection { TO_USER, FROM_USER }

public record MessageBody(String text) {
    public static final int MAX_LENGTH = 4_000;
    public MessageBody {
        Objects.requireNonNull(text, "text");
        text = text.strip();
        if (text.isEmpty()) throw new IllegalArgumentException("message body is blank");
        if (text.length() > MAX_LENGTH)
            throw new IllegalArgumentException(
                "message body is " + text.length() + " characters, limit is " + MAX_LENGTH);
    }
}

public record Message(MessageId id, UserId userId, MessageBody body,
                      MessageDirection direction, boolean read,
                      Instant sentAt, long version) { … }
```

Identity first, then ownership, then content — the order a reader expects.

### 10.3 Expected outcomes are a sealed return type

```java
public sealed interface MarkAsReadResult {
    record Marked(Message message)          implements MarkAsReadResult {}
    record AlreadyRead(Message message)     implements MarkAsReadResult {}
    record VersionConflict(Message current) implements MarkAsReadResult {}
    record NoSuchMessage(MessageId id)      implements MarkAsReadResult {}
}
```

`AlreadyRead` is separate from `Marked` because a redelivered request and a
genuine first read are different facts, and a client that retries needs to tell
them apart.

### 10.4 One class, one transaction, no `Default…UseCase`

```java
@Service
public class Conversation {

    @Transactional
    public Message send(UserId userId, MessageBody body, MessageDirection direction) {
        Message sent = messages.append(userId, body, direction, clock.instant());
        events.stageSent(sent);
        return sent;
    }

    // No @Transactional: the repository does this in one statement, and a
    // transaction around a single statement is the one Postgres opens anyway.
    public MarkAsReadResult markRead(MessageId id, long expectedVersion) {
        return messages.markRead(id, expectedVersion);
    }
}
```

`Clock` is a bean, so a test pins time instead of tolerating it.

### 10.5 The controller translates and nothing else

```java
@PostMapping("/messages/{id}/read")
ResponseEntity<MessageView> markRead(@PathVariable UUID id,
                                     @RequestHeader("If-Match") String ifMatch) {
    MarkAsReadResult result = conversation.markRead(new MessageId(id), parseVersion(ifMatch));
    return switch (result) {                                  // no default
        case MarkAsReadResult.Marked(Message m) ->
                ResponseEntity.ok().eTag(etag(m)).body(MessageView.of(m));
        // 200, not 409: the caller asked for a state the message is in.
        case MarkAsReadResult.AlreadyRead(Message m) ->
                ResponseEntity.ok().eTag(etag(m)).body(MessageView.of(m));
        case MarkAsReadResult.VersionConflict(Message current) ->
                ResponseEntity.status(412).eTag(etag(current)).body(MessageView.of(current));
        case MarkAsReadResult.NoSuchMessage _ -> ResponseEntity.notFound().build();
    };
}
```

Record deconstruction patterns, `_` for the binding that is not read, no
`default`, and the version travels as an `ETag`. The request record carries
only what a client decides:

```java
public record SendMessageRequest(
        @NotBlank @Size(max = MessageBody.MAX_LENGTH) String body,
        @NotNull MessageDirection direction) {}
```

### 10.6 Tests assert outcomes

```java
@Test
void a_second_reader_at_a_stale_version_is_told_the_current_one() {
    Message sent = conversation.send(ALICE, new MessageBody("hello"), TO_USER);
    conversation.markRead(sent.id(), sent.version());

    MarkAsReadResult loser = conversation.markRead(sent.id(), sent.version());

    assertThat(loser).isInstanceOfSatisfying(MarkAsReadResult.VersionConflict.class,
            conflict -> assertThat(conflict.current().version()).isEqualTo(sent.version() + 1));
}

@Test
void a_page_larger_than_the_maximum_is_refused_rather_than_silently_capped() { … }
```

Snake_case sentence names, a fake that behaves like the real adapter rather
than a mock that records calls, no Spring context, no database.

### 10.7 The schema

```sql
create table messages (
    id        uuid        primary key default uuidv7(),
    user_id   uuid        not null references users (id) on delete cascade,
    body      text        not null,
    direction text        not null,
    read      boolean     not null default false,
    sent_at   timestamptz not null default now(),
    version   bigint      not null default 0,

    constraint messages_direction_known
        check (direction in ('TO_USER', 'FROM_USER')),
    constraint messages_body_not_blank    check (length(btrim(body)) > 0),
    constraint messages_body_within_limit check (length(body) <= 4000),
    constraint messages_version_nonnegative check (version >= 0)
);

create index messages_conversation_idx on messages (user_id, sent_at desc, id desc);

create index messages_unread_idx on messages (user_id, sent_at desc) where read = false;
```

A primary key; `uuidv7()`; the enum's set held as a `check`; the body limit
held in both the type and the column; two indexes that serve the two queries
the application actually runs, the second partial so it shrinks as messages are
read. And on users:

```sql
create unique index users_email_key on users (lower(email));
```

The CAS the whole `version` column exists for, as one statement:

```sql
update messages set read = true, version = version + 1
where id = :id and version = :version and read = false
returning id, user_id, body, direction, read, sent_at, version
```

with the three-way discrimination — missing, already read, or overtaken — done
by a single follow-up read rather than by two exception types.

---

## 11. Where this comes from, and what would fix it

These are notes for jails, not changes.

1. **The field spec is rendered verbatim into three languages that have three
   conventions.** One concept should produce `userId` in Java, `user_id` in
   SQL, and whatever the wire format wants. Rendering the input string into all
   three is why `is_read()` is an accessor. This is the highest-value single
   fix in the list.

2. **`scaffold` has no non-negotiable core.** `rails g scaffold` gives you a
   primary key, timestamps and an FK index whether you ask or not, because
   those are not preferences. jails made all three opt-in (`@pk`,
   `timestamps = true`, `indexes = [...]`), and a project that did not opt in
   got two tables with no primary key and no index. **A scaffold with no
   primary key should be a refusal, not an output.**

3. **A `String` field with a small closed set should be challenged.**
   `direction:String!` produced an unconstrained column, an unconstrained
   record, and a test fixture of `"sample"`. jails already has `g enum` and its
   own example manifest uses it here. Nothing pointed at it.

4. **Evolution regenerates the schema but not the code that was derived from
   it.** `g field id` wrote `V004` and left `InMemoryUserRepository.findById`
   returning `Optional.empty()` with a TODO saying the type has no id, and left
   `MessageRepository.findById(String)` typed against an id that is now a
   `UUID`. A generated file whose stated premise has become false should be
   re-planned or reported, not left with a comment contradicting the code beside it.

5. **The generated prose is asserted, never checked.** "keyed on the `email`
   component" (it is not), "ordering per entity" (it is not), "scoped matches
   cannot mutate another tenant's row" (there is no scope), "this type has no
   `id` component" (it has one). Comments that restate a decision are the
   fastest thing in a codebase to go stale, and this codebase is 27% comment. A
   template that cannot verify its own claim should say less. The load-bearing
   ones — the `@ServiceConnection` explanation, the Failsafe note, the
   `DeadLetterPublishingRecoverer` default — are excellent and should stay.

6. **Capabilities install machinery nothing uses.** `add api` installs a sealed
   `ApiException` and an exhaustive handler that nothing throws, while the
   operation with real failure modes hand-rolls `ResponseStatusException`. A
   capability should wire the code that already exists into itself, or say it
   did not.

---

## 12. What is genuinely good, and worth not losing

Being fair about this, because a rewrite that discards it would be worse:

- **`KafkaConfig`.** The `DeadLetterPublishingRecoverer` with an explicit `.DLT`
  destination, the DLT counter, and the comment explaining why
  `NullPointerException` is deliberately *not* classified fatal — that is
  correct, hard-won, and better than most hand-written Kafka setups.
- **`TestcontainersConfig`.** Container-as-`@Bean` with `@ServiceConnection`,
  the process-scoped `stop()` override, and the honest explanation of why
  `withReuse(true)` is not on.
- **The outbox table.** Leases, `for update skip locked`, bounded attempts, a
  partial index on the runnable states. The relay's throughput is wrong (§8.4)
  but the schema is right.
- **`ApiExceptionHandler`** as a design — `ResponseEntityExceptionHandler`,
  RFC 9457, field errors as an extension member, and not echoing the constraint
  name back to an unauthenticated client. It just needs something to throw into
  it.
- **`MockMvcTester`, `@MockitoBean`, Jackson 3, JSpecify `@NullMarked`
  package-info, `JdbcClient` with named parameters, one `COLUMNS` constant.**
  All current, all correct, none of it recalled from 2019.

The bones are 2026. The naming, the schema and the type modelling are not — and
those three are what a reader judges first.

---

## 13. The other six: `~/code/minicom-jails`

Six independent attempts at the same app between 2025-11-16 and 2026-06-01.
This is the more useful dataset, because it separates *"jails rendered bad
input faithfully"* from *"jails does this no matter what you type"*.

Every one of them was built and run.

| snapshot | capabilities | main/test files | `mvn test` |
|---|---|---|---|
| 2025-11-16 | db, api, cors | 49 / 30 | **FAILS** — `ArchitectureTest` (§13.2, closed) |
| 2025-11-21 | db, api, cors, sse | 30 / 16 | green |
| 2025-12-13 | api, cors | 9 / 6 | green (5 of 9 tests `@Disabled`) |
| 2026-01-09 | db, api, cors | 31 / 17 | green |
| 2026-02-05 | db, api, json, cors, sse | 65 / 37 | green |
| 2026-06-01 | db, api, cors | 34 / 20 | green |
| *`my-minicom`* | +kafka, actuator, o11y | 53 / 30 | **FAILED** — `CorsConfigTest`, now closed |

### 13.1 First: better input really does produce much better output

These six confirm §11.1 and §11.2 outright. They were written with `id:long@pk`,
camelCase names, real enums, and in one case `timestamps = true` — and they read
enormously better than `my-minicom`:

```java
public record Conversation(long id, boolean agentJoined, Instant createdAt, long version)
public record Message(long id, long conversationId, Sender sender, String content, Instant createdAt)
public record Issue(long id, long userId, String issueSummary, String conversationSummary,
                    IssueStatus status, Instant createdAt)
public enum Sender { CUSTOMER, BOT, AGENT }
```

Identity first. camelCase throughout. Closed sets as enums. And the schemas
have what `my-minicom`'s lacked:

```sql
create table messages (
  id               bigint      not null,
  conversation_id  bigint      not null,
  ...
  constraint messages_pk primary key (id)
);
create index messages_conversation_id_idx on messages (conversation_id);
```

Primary keys everywhere, an index generated automatically beside every
association, and the `version` migration uses the safe three-step form
(add nullable → backfill → `set not null`) rather than a default-and-drop.

**So `my-minicom` is close to the worst case and these are close to the best
case, and the delta is almost entirely the field spec.** That is the strongest
possible argument for §11.2: the input that produces a table with no primary
key should be refused, because the same tool given one more character produces
this.

### 13.3 `g usecase` hard-codes the primary key to `0L` — in every project

```java
// DefaultPostMessageUseCase, 2026-06-01
Message message = new Message(
        0L,                       // <- the primary key
        command.userId(),
        command.customerId(),
        command.content(),
        false,
        Instant.now(),
        Instant.now());
repository.save(message);
```

Every generated use case over a `long@pk` target does this. All five, across
four projects:

| project | use case | id passed |
|---|---|---|
| 2025-11-16 | `DefaultPostMessageUseCase` | `0L` |
| 2026-01-09 | `DefaultCreateMessageUseCase` | `0L` |
| 2026-02-05 | `DefaultSendMessageUseCase` | `0L` |
| 2026-02-05 | `DefaultEscalateIssueUseCase` | `0L` |
| 2026-06-01 | `DefaultPostMessageUseCase` | `0L` |

And the table is:

```sql
id bigint not null,
constraint messages_pk primary key (id)
```

**No `generated always as identity`. No `default nextval(...)`. No sequence
anywhere in any migration.** So nothing assigns an id, the use case supplies
`0`, and the *second* call to any of these endpoints is a duplicate-key
violation surfaced as a 500. The primary create path of every one of these
projects works exactly once.

`my-minicom` escaped this only because its id was a `uuid`, where jails emits
`UUID.randomUUID()`. The `long@pk` form — the one jails' own
`examples/minicom/.jails/app.toml` recommends — is the broken one.

The generated test cannot see it:

```java
Message created = useCase.execute(command);
assertThat(created.id()).isNotNull();       // a primitive long. Never null.
```

`id()` returns `long`; autoboxing makes `isNotNull()` a tautology, and it holds
for `0L`. The test also inserts exactly one row, so the collision never occurs.
**The single test of the create path asserts something that cannot fail, about
the one value that is wrong.**

Fixing this is a schema question, not a Java one: `id bigint generated always as
identity primary key`, and the use case stops naming the id at all — which is
what the hand-built slice does with `insert … returning` (§10).

### 13.4 The closed set is still never enforced in the schema — 0 checks in 20 migrations

`grep -c "check (" */src/main/resources/db/migration/*.sql` → **zero**, across
all six projects and `my-minicom`. Every enum column is bare `text`:

```sql
sender       text not null,   -- Sender { CUSTOMER, BOT, AGENT }
sender_type  text not null,   -- SenderType { ADMIN, … }
status       text not null,   -- IssueStatus { OPEN, IN_PROGRESS, … }
```

This is not an input problem. The user declared `g enum`, jails generated the
Java enum, jails generated the column, and jails knows the constant list —
and still wrote a column that accepts `'banana'`. A one-line
`check (sender in ('CUSTOMER','BOT','AGENT'))` is derivable from information
jails already holds, and `backend.md` §5 makes it the highest-value line in the
file.

The follow-on question jails would then have to answer — what happens to that
`check` when a constant is added — is a real design problem, and worth solving
rather than avoiding: `g enum` adding a constant should generate the
`alter table … drop constraint … add constraint …` migration in the same step.

### 13.5 `findById(String)` in 11 of 12 generated ports

| domain component | port signature |
|---|---|
| `Conversation(long id, …)` | `findById(String)` |
| `Message(long id, …)` ×5 | `findById(String)` |
| `Ticket(UUID id, …)` | `findById(String)` |
| `Issue(long id, …)` | `findById(String)` |
| `User(long id, …)` ×2 | `findById(String)` |
| `User(UUID id, …)` — `my-minicom` | `findById(UUID)` |

Neither `long` nor `UUID` survives to the port. The one exception is in
`my-minicom`, where `UserRepository` takes a `UUID` and `MessageRepository`
takes a `String` — **two ports in one application, over two tables, disagreeing
about how identity is typed.** Everything downstream inherits it:
`repository.findById(String.valueOf(created.id()))` appears in every generated
test, and the JDBC adapter has to `cast(:id as uuid)` to undo it.

### 13.6 `g client` generates a plausible shape nobody asked for

*The unbounded-call half is closed: the generator writes a base URL and both
timeouts beside the client now, from the plan, the way `ensure_failsafe` is
written from the write path.*

The generic CRUD shape applied to a name like `OpenAiChat` — yielding
`GET /open-ai-chats` returning `{id, name}` — is separately worth flagging.
It is plausible-looking fiction, and it is the kind of output that gets
committed because it compiles. `missing.md` M7 is the same finding from the
other end and carries the fix: `--method` / `--on` / `--returns`, which
`g controller` already takes.

### 13.9 Two generators, two answers, one of them arguing against the other

`minicom-2026-06-01`, same record, same two audit columns:

```java
// MessageRequest.toDomain() — the scaffold path
// Audit columns: set here rather than received, and one
// instant for both, so a freshly created row does not look
// already edited.
Instant now = Instant.now();
return new Message(id, userId, …, now, now);
```

```java
// DefaultPostMessageUseCase.execute() — the use-case path
Message message = new Message(0L, command.userId(), …,
        Instant.now(),
        Instant.now());          // a different instant
```

One generator writes a comment explaining precisely why both timestamps must be
the same value, and the other calls the clock twice. Both are in the same
package, generated by the same command sequence, minutes apart.

### 13.10 Smaller, consistent across all six

- **`timestamp` as a column name** (2025-11-21, 2026-01-09, 2026-02-05). Legal
  in Postgres, but it is a type name; `sentAt`/`createdAt` costs nothing.
- **`deferrable initially deferred` on every generated foreign key**, in all
  six, with no comment saying why. It moves every FK violation from the
  statement to the commit, which changes where the error surfaces and what a
  retry means. Either it is a deliberate default worth one line of explanation,
  or it should not be the default.
- **`ApiException` is thrown in 0 of 7 projects.** §6.1's finding is not a
  `my-minicom` accident — `add api` has never once installed error machinery
  that anything used.
- **`users.email` is only unique where `@unique` was typed** (2026-02-05 has it,
  2026-06-01 does not), and never case-insensitively.

### 13.11 What this changes about §11

The corpus splits the root causes cleanly, and only the first two are about
input:

| | fixable by typing a better spec | jails-side, survives perfect input |
|---|---|---|
| snake_case in Java | ✅ (but §11.1 still holds — jails should converge) | |
| no primary key | ✅ | |
| no FK index | ✅ | |
| enum vs `String` in Java | ✅ | |
| **enum not enforced in SQL** | | ❌ §13.4 — 0 checks in 20 migrations |
| **`g usecase` id = `0L`** | | ❌ §13.3 — every project, create path broken |
| **`findById(String)`** | | ❌ §13.5 — 11 of 12 ports |
| **dead `ApiException`** | | ❌ §13.10 — 0 of 7 projects |

The second column is the list worth working from. None of it is a taste
argument, all of it is reproducible from a clean `jails new`, and the top three
are each a defect a reviewer would block a PR on.
