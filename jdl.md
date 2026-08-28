# jdl.md — one grammar for Java and the database

A comparison of **jails JDL** (as built 2026-08-28), **JHipster JDL** and
**Prisma Schema Language**, and a proposal that generalises jails' 39 kinds,
25 capabilities and 18 modifiers into four syntactic categories.

Goals, in the order they conflict: **general** (covers every use case),
**simple** (few rules), **concise**, **readable**.

All three grammars were read from source: `crates/jails-model/src/jdl.rs`,
`deps/jhipster/generator-jhipster/lib/jdl/core/parsing/`, and
`deps/prisma/scorecard/03-psl-schema-language.md`.

---

## 1. The three grammars, same entity

**JHipster JDL** — closest by name and target (Spring Boot + relational DB):

```
@service(serviceClass)
@paginate(pagination)
entity BankAccount {
  name String required,
  balance BigDecimal required min(0)
}

relationship OneToMany {
  BankAccount{operations} to Operation{account}
}

dto * with mapstruct
service * with serviceImpl except User
paginate Operation with infinite-scroll
```

**Prisma PSL**:

```
model Post {
  id        String   @id @default(uuid(7)) @map("_id")
  title     String
  published Boolean  @default(false)
  authorId  String
  author    User     @relation(fields: [authorId], references: [id])
  createdAt DateTime @default(now())

  @@index([createdAt(sort: Desc)])
  @@unique([authorId, title])
  @@map("posts")
}
```

**jails JDL** (today):

```
entity Task @id(ent_task) @scaffold @factory @dto {
  id: uuid @id(fld_task_id) @pk
  title: string!(1..200) @index
  done: boolean?

  command CreateTask(title) @id(op_create_task) {
    route: POST /tasks
  }
}
```

---

## 2. What each gets right, and wrong

### JHipster JDL

**The one great idea: set-scoped options with wildcards and exclusions.**

```
dto * with mapstruct
service * with serviceImpl except User, Authority
paginate Operation, Label with infinite-scroll
```

This is the only grammar of the three that can say *"this projection applies to
every entity except these"*. jails needs exactly this — `@scaffold`,
`@factory`, `@dto` are currently per-entity annotations, so "give every entity a
DTO" is N edits and drifts the moment somebody adds entity N+1. Its `filter`,
`search`, `microservice` and `angularSuffix` options use the same shape, so one
rule covers a dozen concerns.

**Relationships are declared outside entities**, because a relationship belongs
to *two* entities and nesting forces an arbitrary owner:

```
relationship ManyToOne {
  Operation{account(name)} to BankAccount{operations}
}
```

Verbose, but honest about the symmetry. jails will meet this the moment
`association` becomes declarative.

**What is wrong with it.** The application block is 25 flat config keys
(`applicationType`, `authenticationType`, `cacheProvider`, `dtoSuffix`,
`enableHibernateCache`, `jhiPrefix`, `nativeLanguage`, `skipUserManagement`…),
which is a settings file wearing a grammar's clothes. Field declarations use
juxtaposition *and* commas (`name String required,`) so the separator carries no
information. And its deepest problem is not syntax: regenerating over an evolved
application is its most-complained-about behaviour — which is precisely the
merge problem jails has already chosen to solve rather than avoid.

### Prisma PSL

**The one great idea: `@` versus `@@`.** Field-level attributes take one `@`,
model-level attributes take two. That single convention removes an entire class
of ambiguity and gives composite constraints an obvious home:

| field-level | model-level |
|---|---|
| `@id` `@unique` `@map` `@default` `@relation` `@db.*` | `@@id` `@@unique` `@@index` `@@map` `@@base` `@@discriminator` |

**`@map` / `@@map` — the logical/physical split as one concept.** The code name
and the stored name are different facts about one thing, expressed with one
annotation at two levels. This is exactly jails' `java_name` versus `column` and
`table`, and Prisma says it in a quarter of the space.

**Declarative default functions.** `@default(now())`, `@default(uuid(7))`,
`@default(autoincrement())`, `@default(dbgenerated("..."))`, plus a literal form.
jails currently *infers* defaults in Rust — `usecase_default` has a hardcoded
list covering ids, timestamps, status defaults, counters, flags and empty
optionals, and refuses when it cannot infer. Every one of those refusals is a
default the author could have written down.

**Alignment as syntax.** Prisma files column-align type and attributes, and it
is a large part of why they read well. A formatter makes that free.

**What is wrong with it for jails.** PSL is **DB-first**: the model describes
tables, and the typed client is a projection. There is no place to say anything
about the *Java* side that is not also a database fact — no package placement,
no facet selection, no HTTP surface, no build dependency. `@map` exists
precisely because the two naming worlds differ, and that is the whole extent of
its dual-world vocabulary.

### jails JDL (today)

**Right:** operations nest inside the entity they act on — the thing TOML could
not do and the reason to have a grammar at all. `@id(...)` is optional, which is
a better answer to stable identity than either alternative. `//` comments work,
and field edits are line-level splices, so hand-written comments survive
`jails g field`.

**Wrong, and cheap to fix now:**

1. **`@id` means two different things at two levels** — `@id(ent_task)` on the
   entity, `@id(fld_task_id)` on the field. Prisma's `@@` fixes this.
2. **Three assignment forms with an unwritten rule.** `java 26` (juxtaposition),
   `limit: 100` (colon), `IN_PROGRESS = "in_progress"` (equals), and
   `dependency org.example:widget @scope(test) = "1.2.3"` uses two on one line.
3. **The colon collides with Maven coordinates.** In a grammar where `:` means
   "has type", `org.example:widget` reads wrong. Both prior arts use
   juxtaposition and neither has this problem.
4. **Coverage.** The operation block accepts five keys — `route`, `orderBy`,
   `limit`, `sets`, `yields`. The CLI has eighteen modifiers. `--via`,
   `--on-conflict`, `--select`, `--if-match`, `--consumes`, `--set`, `--bind`,
   `--timestamps`, `--package`, `--default-literal`, `--backfill-file` are all
   inexpressible.
5. **No set-scoped facets, no relationships, no declarative defaults.**

---

## 3. Design from the job, not from the kinds

Someone describes the service they want:

> "I have Tasks. They have a title and an assignee. They go open → doing → done,
> and you can't start one without an assignee. Finishing one notifies billing."

Everything in that sentence is domain. Nothing in it is a layer, an adapter, a
DTO, a table, an HTTP verb, a transaction boundary or an optimistic lock — and
yet all of those are completely determined by it. **That gap is the product.**

So the design rule is one line:

> **Declare the domain. Derive the service. Override the exceptions.**

Which inverts the current shape. Today the author names the *artefacts*
(`scaffold`, `usecase`, `dto`, `repo`) and jails writes them. Instead the author
names the *domain* and jails decides which artefacts that implies.

### 3.1 Four things, and only four, are domain

| | what the author must say | what is never said |
|---|---|---|
| **shape** | things and their fields | column types, nullability plumbing, Java packages |
| **structure** | what owns what, what refers to what | foreign keys, cascade rules, aggregate boundaries, nested routes |
| **lifecycle** | the states a thing moves through, and what each move requires | status columns, check constraints, optimistic locks, If-Match, transition endpoints |
| **consequence** | what the outside world hears about | outbox tables, publishers, topics, delivery retry |

Nothing else is a decision the author is better placed to make than the compiler.

### 3.2 The idea worth having: lifecycle is first class

Business backends are state machines. Order: created → paid → shipped →
delivered. Ticket: open → assigned → resolved → closed. jails already has
`transition` as a kind — it just never noticed that transitions form a
**machine**, so each one is declared alone and nothing checks the whole.

Make the machine the declaration and one block yields: the status column, its
check constraint, one endpoint per move, optimistic locking on each, the guard
that refuses an illegal move, the events, and **a test per illegal transition** —
which is the class of bug nobody writes tests for.

Neither JHipster nor Prisma has this. Both are data-only. It is the single
largest piece of leverage available here and it is not a syntax question.

### 3.3 The second idea: owns versus refers

Real services are graphs, and the two edges behave completely differently:

- A `LineItem` is **part of** an `Order`. Delete the order, delete the items.
  You never query items on their own. They share a transaction. Their route is
  nested.
- A `Task` **refers to** a `User`. Deleting the user must not delete tasks. They
  are separate aggregates, separate transactions, separate routes.

That is the DDD aggregate boundary, and it decides cascade behaviour, FK
direction, transaction scope, route nesting and whether a repository exists.
Today it is `--on child --yields parent` plus hand-written field mappings. Two
symbols carry all of it.

---

## 4. The grammar

### 4.1 The whole of it

```
service notes {
  package com.example.notes
  store   postgres
}

type Money {
  amount   decimal @positive
  currency currency
}

entity User {
  email text @unique @notBlank
  name  text
}

entity Task {
  title    text @notBlank
  due      date?
  price    Money
  assignee -> User
  items    [Checklist]

  states open -> doing (needs assignee) -> done (emits TaskCompleted)
  states * -> cancelled

  read overdue where due < today order due desc
}

entity Checklist {
  label text
  done  bool = false
}

port billing at https://api.example.com {
  @safe @retry 5
}

sink notify on TaskCompleted {
  post https://hooks.example.com/tasks
}

run reconcile every "0 3 * * *" {
  calls billing
}
```

That is a complete service. Twenty-six lines of domain.

### 4.2 What those lines produce

From `entity Task` alone:

- `Task` record with `id`, the declared fields, `createdAt`, `updatedAt`,
  `version`, `status`
- `tasks` table, migration, FK to `users`, check constraint on `status`
- `checklists` table owned by `tasks`, cascade delete
- repository port, JDBC adapter, in-memory fake
- `POST /tasks`, `GET /tasks`, `GET /tasks/{id}`, `PATCH /tasks/{id}`,
  `DELETE /tasks/{id}`, `GET /tasks/{id}/items`
- `POST /tasks/{id}/doing`, `/done`, `/cancelled` — each optimistically locked,
  each refusing an illegal source state
- the guard refusing `doing` without an assignee, at the domain layer *and* as a
  400
- `GET /tasks/overdue`
- `TaskCompleted` event, outbox row, publisher
- record validation tests, an adapter test against a real database, controller
  tests, **and one test per illegal transition**

The author wrote none of that and can read all of it.

### 4.3 Field forms — four, all one line

```
title    text @notBlank        // scalar with constraints
due      date?                 // nullable
status   Status = open         // default
assignee -> User               // refers to another aggregate
items    [Checklist]           // owns many; part of this aggregate
price    Money                 // value type, inlined into the table
```

`?` is the only type modifier. `=` always means "the value when unset". `->` and
`[]` are the two edges. Everything else is an `@attr`.

### 4.4 Deviating from the default

Generality lives here, and it is the part that must stay cheap:

```
entity Task {
  ...
  hide delete                        // no DELETE endpoint
  at /v2/tasks                       // route base override
  read overdue where due < today     // a query beyond CRUD
  fix tenant from header X-Tenant    // the endpoint supplies it, not the caller
  @@in adapters.legacy               // package override
  @@stored task_records              // table name override
}
```

And for behaviour that is not about one aggregate:

```
run reconcile every "0 3 * * *" { calls billing }   // scheduled
run import on file.uploaded { }                     // message-triggered
```

`run` is the escape hatch for behaviour with no CRUD shape. Everything that
*does* have a CRUD shape never needs to be declared at all.

### 4.5 Policies are adjectives

```
@safe          // SSRF guard, DNS pinning, redirect revalidation
@durable       // leased, bounded retry, DB-backed frontier
@idempotent    // retained result keyed by request hash
@verified      // signature checked over raw bytes, constant time
@retry 5
```

An adjective composes; a kind does not. This is why the current tool has
`durable-job` but no durable webhook — adding one means a new kind rather than a
word.

### 4.6 Capabilities mostly disappear

Most of the 25 are consequences of what was declared:

| declared | implies |
|---|---|
| any entity | `db`, migrations, Testcontainers |
| any HTTP route | `api`, validation, JSON |
| `emits` to a topic | `kafka` |
| `@scope` on a field | `security` |
| a `[]` field with full-text search | `search` |

What remains genuinely optional is a short list of cross-cutting choices —
`observability`, `docker`, `k8s`, `ci`, `coverage`, `loadtest` — and those stay
one line:

```
use observability, docker, ci
```

Halving the list the author has to know is a bigger ergonomic win than any
syntax choice in this document.

---

## 5. Why this is more general, not less

The obvious objection to deriving everything is that derivation constrains. The
opposite is true here, and the reason is worth stating:

**Enumerating artefacts is what constrains.** A `durable-job` kind exists; a
durable webhook does not, because nobody enumerated it. A `usecase` may emit an
event; a `transition` may too, but only because someone added that arm. A
`fetcher` is safe; a `client` is not, and there is no safe client.

Under adjectives and derivation, every combination exists by construction —
`@durable` on any operation, `emits` from any state change, `@safe` on any port.
The grammar cannot have a hole that the compiler could have filled.

The cases that genuinely resist derivation are named and kept: `run` for
non-CRUD behaviour, `class`/`interface` for plain Java, `eject` for handing an
artefact over, and hand-written tests as ordinary source.

---

## 6. What this costs

Stated plainly, because a design that only lists benefits has not been thought
through.

1. **Derivation must be inspectable.** `jails why Task` has to print *why* there
   is a `TaskRepository` and *what* would remove it. Without that, magic is
   indistinguishable from a bug. This is a required deliverable, not a nicety.
2. **Defaults are opinions, and someone will disagree.** REST verbs, route
   shapes, cascade rules, `version` on every entity. Each needs an override, and
   §4.4 is where they live; the list will grow and must stay one line each.
3. **The state machine is a real language feature.** Guards, illegal-move
   refusal, terminal states, `*` sources. It earns its complexity but it is not
   free.
4. **`[]` composition implies transaction and cascade semantics** the compiler
   must get right every time. Getting it wrong deletes data.
5. **It is a different product.** Today's author says "generate me a use case".
   This author says "here is my domain". That is a better product and a real
   change, and existing muscle memory does not transfer.

---

## 7. Migration is not the hard part

The CLI survives unchanged as a way to *write this file*:

| command | writes |
|---|---|
| `jails g scaffold Task title:string!` | `entity Task { title text @notBlank }` |
| `jails g transition Complete --on Task` | a `states` arrow |
| `jails g query Recent --on Task` | a `read` line |
| `jails g usecase Publish --on Task --yields E` | a `states` arrow with `emits E` |
| `jails add db` | nothing — implied by the first entity |
| `jails add observability` | `use observability` |

Muscle memory keeps working; what changes is that the commands now converge on
one artefact you can read, diff and review.

---

## 8. Decisions

1. **Are CRUD endpoints on by default, or opt-in?** Default-on is why the
   twenty-six line example is twenty-six lines. Default-off is more explicit and
   much longer. Recommendation: on, with `hide`.
2. **Is `states` one machine per entity or several?** One column, one machine is
   simpler and covers nearly everything. Two machines on one entity needs two
   columns and a name.
3. **How much may `where` express?** `due < today` is readable and compiles to
   SQL. A full expression language is a second grammar. Recommendation: field,
   operator, literal or parameter — refuse anything else and say so.
4. **Does `[]` always mean composition?** Prisma uses `[]` for both edges and
   disambiguates with `@relation`. Using `[]` for owns and `->` for refers is
   clearer, and the cost is that a many-to-many needs a third form.
5. **Inspectability first.** Build `jails why` for derivation before the
   derivation, or the first surprising output loses the argument for the whole
   design.

---

## 9. Worked examples

Every "today" block below is a real scenario from `tests/common/scenarios.rs`,
and every file count is the real golden tree.

### 9.1 The same service, both ways

**Today** — `tests/golden/usecase-query-transition`, five commands:

```
jails g enum PayoutStatus PENDING SETTLED FAILED
jails g scaffold Payout id:uuid@pk amount:long@positive status:PayoutStatus@index \
        version:long@nonnegative createdAt:instant
jails g usecase   RequestPayout      id:uuid amount:long        --on Payout
jails g query     PayoutsByStatus    status:PayoutStatus        --on Payout
jails g transition ChangePayoutStatus status:PayoutStatus ...   --on Payout
```

→ **40 files.**

**Proposed:**

```
entity Payout {
  amount long @positive

  states pending -> settled
  states pending -> failed

  read byStatus(status)
}
```

→ the same 40 files.

Now look at what disappeared, because this is the whole argument:

| declared today | why it is not domain |
|---|---|
| `id:uuid@pk` | every entity has one |
| `version:long@nonnegative` | every entity that can be updated has one |
| `createdAt:instant` | every entity has one |
| `status:PayoutStatus@index` | implied by declaring states |
| `g enum PayoutStatus PENDING SETTLED FAILED` | the states *are* the enum |
| `--on Payout` × 3 | implied by nesting |
| `g usecase RequestPayout id:uuid amount:long` | creating a Payout is the default create |

**Of the five fields declared today, one is domain.** The other four are
ceremony that the compiler is better placed to supply than the author.

### 9.2 What one `states` line actually derives

```
states pending -> settled
```

**Java** — a transition port, a JDBC adapter, an HTTP adapter:

```java
public interface ChangePayoutStatusUseCase {
    Payout execute(Command command);
    record Command(UUID id, PayoutStatus status, long version) { … }
}
```

**SQL** — a compare-and-swap that is also the guard:

```sql
update payouts
   set status = :status, version = version + 1, updated_at = now()
 where id = :id
   and version = :version          -- optimistic lock
   and status = 'PENDING'          -- refuses an illegal source state
returning *;
```

**HTTP** — `POST /payouts/{id}/settled`, requiring `If-Match`, answering `409`
on a version mismatch and `422` on an illegal source state.

**Tests** — the ones nobody writes by hand:

```java
@Test void aSettledPayoutCannotSettleAgain()      { … expect 422 }
@Test void aFailedPayoutCannotBecomeSettled()     { … expect 422 }
@Test void aStaleVersionIsRefused()               { … expect 409 }
```

Today those three tests are the author's problem, and the illegal-transition
class is exactly the one that gets skipped. Declaring the machine rather than
one move at a time is what makes them derivable: **the compiler knows which
moves are absent.**

### 9.3 `->` versus `[]`, concretely

```
entity Order {
  total  Money
  items  [LineItem]      // owns
  buyer  -> Customer     // refers
}
```

| | `items [LineItem]` | `buyer -> Customer` |
|---|---|---|
| table | `line_items` with `order_id` | `orders.buyer_id` |
| FK | `on delete cascade` | `on delete restrict` |
| deleting the parent | deletes the children | refused while orders exist |
| repository | none — reached through `Order` | `CustomerRepository` exists |
| route | `GET /orders/{id}/items` | `GET /customers/{id}` |
| transaction | one, with the order | separate |
| loaded | with the order | by reference |

Seven decisions from one symbol. Today this is `jails g association …
--on LineItem --yields Order orderId=id`, plus the author deciding each of the
seven correctly and consistently every time.

### 9.4 When derivation is wrong

Generality lives here, so the overrides must be one line each:

```
entity Payout {
  amount long @positive

  states pending -> settled

  hide delete                       // money is never DELETEd
  at /v2/payouts                    // this resource moved
  fix tenant from header X-Tenant   // the endpoint supplies it, not the caller
  read large where amount > 100000  // a query beyond CRUD
  @@stored payout_records           // the table was named before we arrived
  @@in adapters.legacy              // this one lives elsewhere
}
```

And behaviour that is not about one aggregate keeps its own form:

```
run reconcile every "0 3 * * *" { calls billing }
run importPayouts on file.uploaded { }
```

### 9.5 Inspectability — the feature that makes derivation safe

```
$ jails why Payout

Payout is an entity, so it has:
  domain/Payout.java             a record with id, amount, status, createdAt,
                                 updatedAt, version
  app/PayoutRepository.java      a storage port          (any entity is stored)
  adapters/JdbcPayoutRepository  the PostgreSQL adapter  (store postgres)
  adapters/InMemoryPayout…       a fake                  (capability fake)
  db/migration/V1__payouts.sql   the table

It has 2 states, so it also has:
  domain/PayoutStatus.java       an enum of pending, settled, failed
  …/ChangePayoutStatusUseCase    one transition port
  web/…Controller.java           POST /payouts/{id}/settled, /failed
  a check constraint on payouts.status
  3 refusal tests

Remove `states` and the last five disappear.
Add `hide delete` and DELETE /payouts/{id} disappears.
```

Without this, magic and bug are indistinguishable. It is why §8 decision 5 says
build `jails why` before the derivation.

### 9.6 A larger service, end to end

Checkout: two aggregates, a value type, composition, a reference, two state
machines, an outbound port, an event and its sink.

```
service shop {
  package com.example.shop
  store   postgres
}

use observability, docker

type Money {
  amount   decimal @positive
  currency currency
}

entity Customer {
  email text @unique @notBlank
  name  text
}

entity Order {
  total Money
  items [LineItem]
  buyer -> Customer

  states cart -> placed (needs items) -> paid (emits OrderPaid) -> shipped -> delivered
  states cart, placed -> cancelled

  read forBuyer(buyer) order createdAt desc limit 50
  hide delete
}

entity LineItem {
  sku      text
  quantity int @positive
  price    Money
}

port payments at https://api.stripe.com {
  @safe @retry 5 @idempotent
}

sink notifyShipping on OrderPaid {
  post https://logistics.example.com/orders
  @idempotent id
}

run expireCarts every "0 * * * *" { }
```

**Thirty-four lines.** What it produces:

- two aggregates, three tables (`customers`, `orders`, `line_items`), `total`
  and `price` inlined as `total_amount`/`total_currency`
- `line_items` cascade-deleted with its order; `orders.buyer_id` restricted
- full REST for both, `GET /orders/{id}/items` nested, no `DELETE /orders/{id}`
- six transition endpoints, each optimistically locked, each refusing illegal
  sources; `placed` refuses an empty cart
- `OrderPaid` event, outbox, publisher; the sink delivering it with an
  idempotency key and bounded retry
- a safe HTTP client for payments with SSRF protection and retry
- an hourly job
- migrations, Testcontainers wiring, an ArchUnit fitness test, record validation
  tests, adapter tests against a real database, controller tests, and one
  refusal test per illegal transition

The equivalent today is roughly **fifteen CLI invocations** carrying about forty
flags between them, and the result exists only as files — there is no artefact
you can read to see what the service *is*.

That difference is the point of the whole document.
