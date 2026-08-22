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

### Current evidence -- payments gateway (2026-08-22)

`jails app apply` + `doctor` + `migrate --check` + `jails check`: **15 doctor
checks all clear, 7 migrations applied to a scratch database, 66 unit tests
and 17 integration tests against real PostgreSQL and Kafka, 0 failures,
BUILD SUCCESS in 1 m 45 s.** 80 manifest lines produced 6,429 lines across 137
files. **No Java or SQL was hand-edited.**

Four clauses of its contract are **not** proved and are not claimed:

- a duplicate idempotency key produces one payment, but the retained result is
  not returned and a conflicting payload under the same key is not a 409. The
  unique column is `generated`; the receipt semantics are `not selected`;
- SLO histogram buckets, a `ping`-only liveness probe and the readiness
  datasource contribution are `not selected`;
- the connection-pool and replica-awareness datasource defaults are
  `not selected`;
- no load profile is generated and no p99 is recorded, so nothing here says
  anything about throughput.

The three skipped tests are the scaffold's `Jdbc<Name>RepositoryIT` stubs,
which are `@Disabled` by generation rather than by failure. They are recorded
as friction in [`DOGFOOD.md`](DOGFOOD.md), not as passing coverage.

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

## Current evidence -- ledger CLI (2026-08-22)

`jails new-cli` + `app apply` + `jails check`: **53 tests, 0 failures, 6
skipped, BUILD SUCCESS in 1.9 s**, from 31 manifest lines to 1,633 generated
lines across 38 files. **No Java or SQL was hand-edited.** The six skipped
tests are the three strategy implementations' `@Disabled` "say what makes this
qualify" bodies and their generated companions -- work the reader is meant to
do, not coverage jails dropped.

Three generic defects were found and fixed rather than worked around; they are
in the defect ledger in [`DOGFOOD.md`](DOGFOOD.md). The application is
`generated` and `configured`; nothing here claims it reconciles anything.

## Current evidence -- crawler and inbox (2026-08-21)

The clean-manifest gate now proves idempotent application, Java 25 LTS
compilation, real PostgreSQL migrations/repositories/typed equality queries,
generated create use cases through MVC, authenticated observability, typed
Kafka round trips, and PostgreSQL-leased durable work. Durable tests cover
same-payload replay, conflicting idempotency keys, expired-lease reclaim,
bounded retry, terminal error visibility, and recovery after the business
effect committed before queue acknowledgement.

Generated equality queries use stable key ordering and a hard 100-row result
ceiling. Richer list/null filters, caller-selected sorting, projections, and
keyset cursors remain explicit future query contracts rather than inferred
behavior.

Production authentication is a JWT resource server. The inbox marks arbitrary
request fields with generic `@scope` constraints, which compare against
same-named JWT claims; scaffold routes that cannot prove scope are not emitted.
The crawler now has a generic safe outbound fetch boundary with exact-host
redirect policy, HTTPS downgrade prevention, private/reserved-address SSRF
rejection, DNS pinning after validation, byte/media/time/redirect bounds,
failure classification, metrics, and adversarial real-socket tests. Both apps
generate pinned least-privilege CI and non-root multi-stage image assets.
The inbox also generates an atomic `transition` slice: tenant scope is part of
the SQL predicate, a numeric version is compared and incremented in one
statement, and real PostgreSQL tests prove stale retries and cross-scope writes
cannot mutate the row. A use case with `strategy_yields` now generates a
transactional multi-sink outbox: the business row and stable event payload
commit together, the leased relay waits for every configured sink, and
PostgreSQL tests prove bounded retry plus inspectable terminal failure. Kafka
is the default sink; the generic conditional HTTP sink sends typed JSON with a
stable `Idempotency-Key`, finite timeouts, no redirects, 2xx-only
acknowledgement, and a real loopback provider contract.

The crawler now composes its safe fetcher through the generic `http-workflow`
intent. Its PostgreSQL frontier/run/page ledger implements leases and expired
claim recovery, robots handling, exact-origin canonical discovery,
cycle/duplicate suppression, retry classification, maximum pages/depth,
persistent cancellation, metrics, and status/pages APIs. The inbox now uses
ten generic composite `association` intents. Workspace ownership now spans
contacts, members, inboxes, inbox membership, conversations, messages, and
assignments; both a conversation's contact/inbox and an assignment's
conversation/member must belong to the same workspace. Generated tests prove
exact ordered foreign-key mappings and reject impossible cross-boundary
historical data; checks are deferred until commit so valid atomic units of work
may write related records in either order. A second optimistic transition
changes assignment member/status with the same tenant and stale-version
guards as conversation status.

The latest complete CLI/integration sweep passed all 123 tests in 293.35
seconds. Its expanded fresh-manifest gate applied all 19 support-inbox
migrations and all 4 crawler migrations, ran every generated unit/integration
test against real PostgreSQL and Kafka, passed the provider socket contract,
built both images, and retained runtime user `10001:10001`. No Java or SQL was
hand-edited. The remaining external boundary is execution of the generated
hosted CI workflows in an actual repository;
their pinned, least-privilege source is generated, but a local run does not
claim that GitHub executed it. Jails also still lacks atomic whole-manifest
`ChangeSet` application, provenance/drift repair, and offline project creation,
which are tool lifecycle gaps rather than hidden application-code edits.
