# Dogfood acceptance contracts

The examples are executable specifications for Jails' generic machinery.
They are not special templates, and no crawler-, inbox-, conversation-,
workspace-, payment-, or ledger-named branch belongs in Jails core.

## Shared generated-application contract

A claim is complete only when a fresh directory produced from a manifest,
with no hand-edited Java or SQL, proves it. Every Spring application must:

- apply the same manifest twice without changing generated output;
- compile every generated source file;
- run Flyway migrations against PostgreSQL and execute repository/query tests;
- execute generated HTTP use cases through Spring MVC and persist the result;
- publish and consume typed messages through a real Kafka broker;
- expose authenticated health/metrics checks through the real security chain;
- start without development-only security credentials in a production profile;
- build a non-root, reproducible OCI image and pass a generated CI gate;
- expose bounded timeouts, retries, metrics, correlation IDs, and useful
  terminal-failure diagnostics for every generated external or background path.

The gate may report `generated`, `configured`, `user-owned`, or `not selected`.
It must never call an unproved property guaranteed or production ready.

The **ledger CLI is deliberately outside that list**: it is the control, and
every clause above naming Spring, HTTP, a broker or a container is a clause it
must *not* need. Its own contract is below.

## Web crawler contract

From `examples/web-crawler/.jails/app.toml`, Jails must produce a runnable
application that can accept a seed URL, durably resume a crawl after restart,
fetch an exact-host finite page graph, store each canonical URL once, and
report status/pages through generated APIs. Its executable adversarial tests
must cover robots policy, redirects leaving scope, private/reserved-address
SSRF attempts (including DNS rebinding), response-size/type/time limits,
cycles, duplicate links, retryable versus terminal failures, cancellation,
and a hard maximum-page/depth bound.

## Support inbox contract

From `examples/support-inbox/.jails/app.toml`, Jails must produce a runnable
application that creates workspaces, contacts, conversations, and messages;
creates members and inboxes, relates members to inboxes, assigns and reassigns
conversations; lists those resources through tenant-scoped queries; and
durably stages outbound delivery. Its executable tests must prove
cross-workspace reads and writes are denied, duplicate idempotency keys do not
duplicate effects, stale optimistic versions fail without mutation, message
creation and delivery staging are atomic, retries keep a stable delivery ID,
and terminal delivery failure is inspectable.

## Payments gateway contract

From `examples/payments-gateway/.jails/app.toml`, Jails must produce a runnable
application that authorises a payment under a merchant scope, stages the
authorisation event and the business row in one transaction, captures it under
an optimistic version, records refunds against an existing payment, and lists
payments by merchant and status through tenant-scoped queries. Ownership must
be enforced in PostgreSQL -- a payment's merchant and a refund's payment --
rather than trusted from a JWT claim. Money is minor units in a `long`; no
`double` may appear in generated money code.

## Ledger CLI contract -- the control

From `examples/ledger-cli/.jails/app.toml`, Jails must produce a runnable
plain-Maven application with **no Spring, no web server and no PostgreSQL**:
a value object with its own validation, an enum, a sealed result set, a
record, an open strategy with one bean-free implementation per variant, a
second CLI dispatcher, and a subcommand registered into the dispatcher the
manifest names. `mvn clean verify` must pass offline against the local
repository, including the formatter the manifest asks for.

The clause that makes it a control: **standing it up must not add a line to
`src/`**. A generator that only works because the project is a Spring Boot
application is a generator this app cannot use, and that is the finding.
