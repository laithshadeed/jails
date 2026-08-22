# plan.md — the jails plan, written for handover

Rewritten 2026-08-22 against the tree at `e523c16` (builds clean). Sources:
the six `ideas-*.md` documents (7,741 lines), the 28,305 lines of Rust in
`src/`, the 57 Java templates, the 162 golden snapshots, the two proof
applications under `examples/`, the upstream checkouts in `deps/`, and
**`/home/laith/code/projects/payments-gateway-service`** — a 22-module,
332,397-line production payments system read for this rewrite and used in §5
as the source of jails' production defaults.

## 0. The goal, and how to know it is met

### 0.1 The goal

**Build four real applications using nothing but `jails` commands, with zero
hand-written Java or SQL — and make each rebuild faster, cheaper and easier
than the last.**

The applications are not the product. **jails is the product.** The
applications exist to make the tool's gaps observable, because a generic tool
cannot be proved generic by its own test suite.

| | Application | Shape | What it falsifies if it fails |
|---|---|---|---|
| **A** | **Web crawler** | outbound I/O, bounded traversal, termination | that generators only do CRUD |
| **B** | **Intercom-shaped support inbox** | tenancy, ordering, durable delivery | that generators only do single-tenant |
| **C** | **Payments gateway** | money, idempotency, o11y, throughput | that "production ready" is a slogan (§5) |
| **D** | **CLI app** (ledger reconciler, **no Spring**) | plain Maven, no web, no database | that the machinery is Spring-shaped |

Full manifests: **all four now exist and all four pass their gates** --
`examples/{web-crawler,support-inbox,payments-gateway,ledger-cli}/.jails/app.toml`.
C and D were stood up on 2026-08-22 (§4.3, §4.4, and the ledgers in
`examples/DOGFOOD.md`); six generic defects came out of the two runs.

### 0.2 The one constraint

**jails stays a generic tool.** A crawler, a support inbox, a payments gateway
and a ledger CLI are four lists of the same generic intents.

> **The moment a proof app needs the word `crawl`, `conversation`,
> `workspace`, `payment`, `merchant`, `settlement`, `ledger` or `inbox`
> inside `src/` or `templates/`, the abstraction has failed. The fix is a new
> generic primitive, never a branch.**

§4.6 is the enforceable form: six questions every core change must pass, plus
a repository test that greps for showcase vocabulary and fails on a hit.

### 0.3 The loop

This is the whole method. It has already produced twenty-one generic defect
fixes (`examples/DOGFOOD.md`), so it is known to work.

```
1. Write / update the app's .jails/app.toml
2. Run the runbook (§18) — record EVERY command, verbatim
3. Did it need a hand-edit to Java or SQL?
       yes -> that is the finding. Do NOT fix the app.
               Write a friction-ledger row, fix jails generically, go to 2.
       no  -> continue
4. Does it pass its acceptance contract (examples/ACCEPTANCE.md)?
       no  -> same as above
       yes -> continue
5. Score it (§0.4). Did faster/cheaper/easier improve?
       no  -> pick the worst number, fix that, go to 2
       yes -> the app is done; move to the next one
```

**Rule 3 is the one that matters, and it is the easiest to violate under time
pressure.** Never hand-fix a proof application to make it pass. A manual edit
is not a fix — it is evidence, and it belongs in `examples/DOGFOOD.md`'s
friction ledger as a row naming the generic jails improvement it implies.

### 0.4 The scorecard — what "faster, cheaper, easier" means

Record these per app, per run, in `examples/DOGFOOD.md`. Every one is a
number, so "it feels better" is never the answer.

| Axis | Metric | Now | Target |
|---|---|---|---|
| **Easier** | **hand-written Java or SQL lines** | **0** for A and B | **0**, always — any other value is a defect |
| **Easier** | manual interventions during the gate | friction-ledger row count | trending to zero |
| **Easier** | commands from empty directory to passing gate | 4 (`new`, `mkdir`, `cp`, `app apply`) | **1** — see §18's closing question |
| **Cheaper** | manifest lines per app | 65 (A), 263 (B) | falls as generators absorb repetition |
| **Cheaper** | generated lines per manifest line | ~18× for one scaffold | rises |
| **Faster** | full gate wall time | **293 s** | container reuse (§10.1) is the biggest lever |
| **Faster** | edit → test result | 3,810 ms | ~110 ms (§10.2), measured not estimated |
| **Confidence** | kinds with a golden snapshot | **30 of 30** (§8.5, closed) | 30 of 30, enforced by a test |
| **Confidence** | acceptance clauses open | 4, all lifecycle (§4.2) | 0 |

Two numbers are load-bearing and easy to lose:

- **Hand-written lines must stay at zero.** It is currently zero across 328
  manifest lines and 23 migrations — the strongest single fact in this
  repository. If it ever goes positive, stop feature work.
- **Golden coverage must track the kind count.** Eight kinds were added and
  the golden count did not move. That was §8.5 — now closed:
  `every_kind_and_capability_has_a_golden_scenario` reads the kinds and
  capabilities out of the binary's own help and fails when one has no
  scenario, so the count cannot silently stop tracking again.

### 0.5 Done

**Each app is done** when it is generated from its manifest with zero hand
edits, passes its contract in `examples/ACCEPTANCE.md`, and has a
friction-ledger entry for anything that was awkward.

**jails is done with this round** when all four are done *and* the four
lifecycle clauses in §4.3 are closed — atomic apply, drift repair, offline
creation, hosted CI — because those are what stop the loop from being cheap
to run again.

**jails is never done with the loop.** The next application, in a shape none
of these four covers, is the next falsifier.

## How to use this document

**This file is written to be handed to another coding agent.** It is
structured so that an agent can pick any numbered item, find the file and
symbol it touches, the evidence it is real, the fix, and the test that pins
it, without re-deriving the analysis.

**Read §0 first.** It states the goal, the four applications that validate it,
the loop, and the numbers that say whether the loop is working. Everything
below §0 is detail in service of it.

- **§1–§3** are state of the world: what exists, what it costs, what proves it.
- **§4** is the four proof applications — the testing ground, with runnable
  manifests. **§17 is the command runbook.**
- **§5** is production defaults, extracted from a real system and stated in
  generic form.
- **§6** is the maintainability plan: seven options (A–G) for the generator
  code, ranked by value ÷ effort, with what to reject and why. The disease is
  not file size — it is that *"what files does kind X produce?"* has **five**
  definitions and nothing checks they agree.
- **§7** is a corrections ledger: designs in the source documents that rest on
  facts that are not true. Read it before implementing anything from them.
- **§8–§15** are the work, in tiers.
- **§16** is the sequence, with a *Proves* column tying each item to the proof
  app whose acceptance clause it closes.

Three standing rules for anyone working from this file:

1. **jails stays generic.** A crawler, a support inbox and a payments gateway
   are three lists of the same generic intents. None of them gets a command,
   branch, enum, template or property named after its domain. §4.6 is the
   enforceable form of this rule.
2. **Never hand-edit a proof application.** A manual edit is not a fix — it is
   evidence for the next generic improvement, and it belongs in
   `examples/DOGFOOD.md`'s friction ledger.
3. **The tree wins.** Where this file and the six source documents disagree,
   the tree is right; where this file and the tree disagree, the tree is
   right. This file will drift the way `validation/README.md` did.

---

## 1. Where the project actually is

Counted today.

| | Now | Two passes ago |
|---|---:|---:|
| Rust in `src/` | **28,305** | 22,593 |
| `src/spring.rs` | **6,459** | 1,969 |
| `src/generate.rs` | 3,361 | 2,767 |
| `ArtifactKind` | **30** | 22 |
| `Capability` | **18** | 16 |
| Layers (`LAYERS_IN_ORDER`) | **11** | 8 |
| Java templates | **57** (4 `generate` / 17 `add` / 36 `spring`) | 51 |
| Golden files / scenarios | **308 / 32** | 162 / 25 |
| `doctor` checks | 50 | 50 |
| `why` rules | 20 | 20 |
| Commands with `--json` | 3 | 3 |
| `pom::TARGET_RELEASE` | **`"25"`** | `"27"` |
| Full sweep | **123 tests, 293 s** | — |

**The golden count did not move while eight kinds were added.** That was §8.5,
the single most important item in this document, and it is now closed: the
twelve uncovered kinds and capabilities have scenarios, and a test enumerating
the CLI's own value lists refuses to let a thirteenth appear.

### 1.1 What the newest generators do

Read this before any gap list; four of the six source documents propose as
future work things that now exist, three in worse shapes than what shipped.

**`usecase --on <Resource>`** reads the target record off disk, **refuses** if
it cannot (naming the fix), requires a stable non-optional `id`, **type-checks
every declared input against the target's component** (normalised Java type
*and* nullability), and **infers every component you did not supply**,
refusing to guess the rest: *"Jails only infers ids, timestamps, status
defaults, counters, flags, and empty optional/collection values."* That is
`ideas-sol.md`'s "infer aggressively, guess conservatively", implemented.
`--yields <Event>` additionally emits a transactional outbox.

**`query --on`** — typed parameters and results, JDBC adapter, real-database
test. **`transition --on`** — the update counterpart, with `version` and
`@scope`.

**`durable-job --on <UseCase> --yields <Resource>`** — a PostgreSQL-leased
queue whose store is pinned by source assertions to contain `for update skip
locked`, `lease_until <= now()`, `attempts >= max_attempts`, `on conflict (id)
do nothing`, with an IT covering replay, conflicting idempotency keys,
expired-lease reclaim, bounded retry, terminal error visibility, and recovery
after the business effect committed but before queue acknowledgement.

**`fetcher`** — a safe outbound boundary: exact-host redirect policy,
HTTPS-downgrade prevention, reserved-address rejection, DNS pinning after
validation, byte/media/time/redirect bounds, failure classification, metrics,
adversarial real-socket tests.

**`association <Name> child=parent... --on <Child> --yields <Parent>`** —
relations, in a better shape than two source documents proposed. Requires
`add db`, reads **both** records, checks each field exists on both sides,
type-checks across the boundary, rejects a child field mapped twice, checks
PostgreSQL's 63-byte identifier limit, reads the migration directory to decide
whether the parent needs a unique index, and emits a forward migration with
`on update no action on delete no action deferrable initially deferred`.
Composite keys free; no `ON DELETE` invented.

**`http-workflow <Name> --on <Fetcher>`** — durable, exact-origin,
robots-aware graph traversal: `start`/`status`/`pages`/`cancel`/`runOnce`, a
**PostgreSQL frontier** reusing the durable-queue leasing pattern, canonical
dedup, `maxPages`/`maxDepth`, cancellation. Two choices worth recording
because three documents argued the other way: **robots.txt is just another
frontier entry** (depth `-1`, `Kind.ROBOTS`) fetched through the same safe
boundary, and **HTML is parsed with the JDK's own `HTMLEditorKit` callbacks —
no jsoup, no crawler-commons, zero new dependencies.**

**`http-sink --on <UseCase> --yields <Event>`** (alias `webhook`) — delivers a
typed outbox event over HTTP.

**`@scope`** is enforced: `require_scope_authorizer` refuses a scoped
operation when `security` has not written a `ScopeAuthorizer`.

**Eleven call sites read a record off disk.** That shared field model is why
`DOGFOOD.md`'s defect table reads as it does — most of those twenty-one bugs
were *two outputs of one model disagreeing*, and each fix collapsed them onto
one source of truth.

---

## 2. The two budgets

### 2.1 Latency (measured; `ideas-opus2.md` §1, `ideas-fable.md` §2)

| What | Time |
|---|---:|
| `mvn -o -q compile`, nothing changed | 2.27 s |
| `mvn -o -q test -Dtest=X`, warm | **3.81 s** (corpus median 2.57 s, p90 25.2 s) |
| `javac` one file with `-J-XX:+AutoCreateSharedArchive` | **0.25 s** (vs 1.45 s) |
| JUnit `Launcher`, precompiled, fresh JVM | 1.65 s |
| **same test, 2nd run in the same JVM** | **9–13 ms** |
| **one file recompiled via `JavaCompiler` API, warm** | **74–166 ms** |
| a domain record test itself | 5 ms |
| `postgres:17-alpine` via Testcontainers | 7.0–8.8 s + 0.45 s Ryuk |

**Edit one test, recompile, rerun is ~110 ms of real work; Maven charges
3,810 ms.** Quarkus' bar: 1,470 → 295 ms from *selection* alone, and it nags
above 4 s.

**The number that matters most now is the gate: 293 s.** Most of it is
containers, and `grep -rn withReuse src/ templates/` still returns nothing.

### 2.2 Authorship — where the remaining asymmetry is

One `g scaffold` writes **1,180 lines in 39 ms**. Adding **one field** to it
is 6 files, ~17 edit sites, plus a hand-written migration — and there is still
no `g field` (`grep -c 'ArtifactKind::Field'` → 0).

This matters *more* than a pass ago, not less: the support-inbox manifest is
**263 lines and 40-odd intents**. jails can create all of them and evolve none
of them.

| Change shape | Today | After §9 |
|---|---|---|
| Add a field to a resource | 6 files, ~17 sites, + a migration | 1 command |
| Fix `/categorys` → `/categories` | 5 edits across controller, DDL, migration, DTO, fixture | 0 |
| Model first (`g record`, then scaffold) | blocked; retype every field | `g scaffold <Name>` |
| `created_at`/`updated_at` | typed per table; `updated_at` never updates | `--timestamps` |
| Test data for a new test | `new` a 6-component record; +1 breaks 40 call sites | `g factory` |
| Change a field in `.jails/app.toml` | **fails on a path collision** (§9.7) | re-applied |
| Store a relation | **shipped** — `g association` | — |

The right generator metric is **authored lines and decisions remaining after
generation**, not generated line count. Today that is **0** for both proof
apps across 328 manifest lines and 23 migrations. That is the strongest single
fact in this repository, and §4.5 makes keeping it at zero a gate.

---

## 3. The honest multiple

**~35×** on the edit→test cycle and **~345×** on the rerun alone (measured);
**~8 s per run** from container reuse; **6 files → 1 command** on the most
common model change; **0 → 8 commands** in a repository jails did not create.
Do not multiply unrelated ratios into a headline.

---

## 4. The proof applications

### 4.1 Why a proof has to be an application

jails' own test suite cannot prove jails is generic. Golden files pin bytes,
tier 3 compiles what someone wrote a test for, and neither answers the only
question that matters: **can a real product be built out of these primitives
with no hand-written Java, and without a domain word appearing in core?**

> **The moment a proof app needs `crawl`, `conversation`, `workspace`,
> `payment`, `settlement` or `inbox` inside `src/`, the abstraction has
> failed. The fix is a generic primitive, never a branch.**

The harness exists — do not rebuild it: `examples/*/.jails/app.toml`,
`examples/ACCEPTANCE.md`, `examples/DOGFOOD.md`, and five tests in
`tests/cli.rs` from `app_manifest_plan_is_domain_blind_and_writes_nothing`
through `app_manifests_pass_the_full_generated_verification_gate`.

**Four apps, chosen so that no two share a floor:**

| | App | Shape | Falsifies |
|---|---|---|---|
| **A** | web crawler | outbound I/O, termination, bounded traversal | that generators only do CRUD |
| **B** | support inbox | tenancy, ordering, durable delivery | that generators only do single-tenant |
| **C** | payments gateway | money, idempotency, o11y, high throughput | that "production ready" is a slogan (§5) |
| **D** | ledger CLI | **no Spring at all** | that the machinery is Spring-shaped |

### 4.2 What A and B already prove

`examples/ACCEPTANCE.md`, latest sweep: **123 tests passing in 293.35 s.** The
fresh-manifest gate applied all 19 support-inbox migrations and all 4 crawler
migrations, ran every generated unit and integration test against real
PostgreSQL and Kafka, passed the provider socket contract, built both OCI
images, and retained runtime user `10001:10001`. **No Java or SQL was
hand-edited.**

That closed what were the two largest open clauses one pass ago: the crawler's
"composition of the fetch boundary into finite traversal" (now
`http-workflow`) and the inbox's transactional outbox delivery (now
`usecase --yields` plus `http-sink`).

Between them the manifests use eleven kinds: 10 `scaffold`, 10 `usecase`, 10
`association`, 9 `query`, 6 `enum`, 2 `transition`, 2 `event`, and one each of
`http-workflow`, `http-sink`, `fetcher`, `durable-job`.

**What `ACCEPTANCE.md` still lists as open is no longer capability work:**

| Open | Closed by |
|---|---|
| Atomic whole-manifest `ChangeSet` | §11 |
| Provenance / drift repair | §11, whose primitive is §9.1 |
| Offline project creation | §11, asset already exists as `write_spring_fixture` |
| Execution of the generated hosted CI workflows | External — keep hosted CI a required check |

### 4.3 App C — the payments gateway

**Purpose.** A and B prove jails can *build* a system. C proves jails can
build a system that would survive production — and it is the app that forces
§5's defaults into the generators, because every one of them is something the
real payments-gateway-service does and a jails-generated app currently does
not.

**It must not add a payments concept to core.** Money is `amount:long` plus a
`Currency` enum; idempotency is a unique key plus a receipt row; settlement is
a `durable-job`. If any of that needs a new noun in `src/`, that is the
finding.

**The manifest**, as run. Two corrections came out of running it: the `event`
intent has to precede the `usecase` that yields it, since intents apply in
manifest order, and a `durable-job`'s fields are its command's fields exactly,
in order. Live at `examples/payments-gateway/.jails/app.toml`.

```toml
# examples/payments-gateway/.jails/app.toml  (proposed)
schema = 1
capabilities = ["db", "api", "actuator", "observability", "security",
                "json", "testkit", "kafka", "docker", "ci"]

[[generate]]
kind = "enum"
name = "PaymentStatus"
fields = ["AUTHORISED", "CAPTURED", "REFUNDED", "REVERSED", "DECLINED", "FAILED"]

[[generate]]
kind = "enum"
name = "PaymentMethod"
fields = ["CARD", "BANK_TRANSFER", "WALLET"]

[[generate]]
kind = "scaffold"
name = "Merchant"
fields = ["id:uuid@pk@scope", "reference:string!@unique", "displayName:string!", "createdAt:instant"]

[[generate]]
kind = "scaffold"
name = "Payment"
fields = ["id:uuid@pk", "merchantId:uuid@index@scope", "idempotencyKey:string!@unique",
          "amountMinor:long@positive", "currency:string!", "method:PaymentMethod",
          "status:PaymentStatus@index", "version:long@nonnegative",
          "authorisedAt:instant?", "capturedAt:instant?", "createdAt:instant"]
indexes = ["merchant_id, created_at desc", "status, created_at"]

[[generate]]
kind = "scaffold"
name = "Refund"
fields = ["id:uuid@pk", "merchantId:uuid@index@scope", "paymentId:uuid@index",
          "amountMinor:long@positive", "reason:string?", "createdAt:instant"]
indexes = ["payment_id, created_at"]

[[generate]]
kind = "association"
name = "PaymentMerchant"
fields = ["merchantId=id"]
strategy_on = "Payment"
strategy_yields = "Merchant"

[[generate]]
kind = "association"
name = "RefundPayment"
fields = ["paymentId=id"]
strategy_on = "Refund"
strategy_yields = "Payment"

[[generate]]
kind = "usecase"
name = "AuthorisePayment"
fields = ["id:uuid", "merchantId:uuid@scope", "idempotencyKey:string!",
          "amountMinor:long", "currency:string!", "method:PaymentMethod"]
strategy_on = "Payment"
strategy_yields = "PaymentAuthorised"

[[generate]]
kind = "transition"
name = "CapturePayment"
fields = ["id:uuid", "merchantId:uuid@scope", "status:PaymentStatus", "version:long"]
strategy_on = "Payment"

[[generate]]
kind = "usecase"
name = "RefundPaymentRequest"
fields = ["id:uuid", "merchantId:uuid@scope", "paymentId:uuid", "amountMinor:long"]
strategy_on = "Refund"

[[generate]]
kind = "query"
name = "PaymentsByMerchant"
fields = ["merchantId:uuid@scope"]
strategy_on = "Payment"

[[generate]]
kind = "query"
name = "PaymentsByStatus"
fields = ["merchantId:uuid@scope", "status:PaymentStatus"]
strategy_on = "Payment"

[[generate]]
kind = "event"
name = "PaymentAuthorised"
fields = ["id:uuid", "merchantId:uuid", "paymentId:uuid", "occurredAt:instant"]

[[generate]]
kind = "fetcher"
name = "AcquirerFetcher"

[[generate]]
kind = "http-sink"
name = "AcquirerSettlement"
strategy_on = "AuthorisePayment"
strategy_yields = "PaymentAuthorised"

[[generate]]
kind = "durable-job"
name = "SettlementDispatcher"
fields = ["id:uuid", "merchantId:uuid@scope", "paymentId:uuid"]
strategy_on = "AuthorisePayment"
strategy_yields = "Payment"
```

**Its acceptance contract** — every clause maps to something the real system
does:

- a duplicate `idempotencyKey` for one merchant produces **one** payment and
  returns the retained result; a different payload under the same key is a
  409;
- a stale `version` on capture fails with 409 and mutates nothing;
- cross-merchant read or write is denied (`@scope`);
- authorisation and the settlement outbox row commit in **one** transaction;
- settlement retries reuse a stable delivery ID and terminal failure is
  inspectable;
- amounts are minor units in `long` — **no `double` anywhere in generated
  money code**, which is a `jails lint` rule (§15);
- `/management/prometheus` exposes `http.server.requests` with **explicit SLO
  buckets** and `percentiles-histogram: false` (§5.2);
- liveness is `ping` only and readiness includes the datasource (§5.2);
- a k6 load profile exists and the gate records p99 (§5.5).

**What C is expected to expose.** Predicted findings, to be confirmed by
running §17: no CORS (§14.1); no idempotency receipt primitive — the
`@unique` column gives one-per-key but not the *retained result* semantics, so
this is likely the first genuinely new generic intent C demands; `--timestamps`
absent so `createdAt` is hand-declared five times; no money type, so
`amountMinor:long` plus a currency string is the honest spelling and the plan
should **not** add a `money` field type (§9.8).

### 4.4 App D — the control

A, B and C are all Spring, and all eight new kinds call
`require_spring_project`. Any Spring-shaped assumption in the generic
machinery is invisible to all three. D is a plain-Maven CLI with no Spring: a
CSV → double-entry ledger reconciler.

```toml
# examples/ledger-cli/.jails/app.toml  (proposed)
schema = 1
capabilities = ["csv", "json", "sqlite", "testkit", "format"]

[[generate]]
kind = "value"
name = "Money"
fields = ["amountMinor:long", "currency:string!"]

[[generate]]
kind = "enum"
name = "MatchOutcome"
fields = ["MATCHED", "AMOUNT_DIFFERS", "DATE_DIFFERS", "UNMATCHED"]

[[generate]]
kind = "sealed"
name = "LedgerError"
fields = ["MalformedRow", "UnknownCurrency", "DuplicateReference"]

[[generate]]
kind = "record"
name = "Entry"
fields = ["reference:string!", "postedAt:date", "amount:Money", "memo:string?"]

[[generate]]
kind = "strategy"
name = "MatchRule"
fields = ["ExactReference", "AmountAndDate", "FuzzyMemo"]
strategy_on = "Entry"
strategy_yields = "MatchOutcome"

[[generate]]
kind = "cli"
name = "Ledger"

[[generate]]
kind = "command"
name = "Reconcile"
```

Why: it removes every shared assumption; it **walks straight into live defect
§8.1** (a `new-cli` project gets a `pom.xml` Maven refuses to parse), turning
it into a failing gate instead of folklore; it exercises `value`, `sealed`,
`strategy`, `cli`, `command`, `record` — kinds A/B/C never touch, including
`register_command`'s dispatcher splice and `g strategy`'s read-disk `destroy`;
and it is cheap, `mvn -o verify` in seconds against C's containers.

**The rule that makes D a real control: adding it must not add a line to
`src/`.** If it does, that line is the finding.

### 4.5 The authorship ledger — the number that proves the thesis

Record per app, per gate run:

| Metric | Why |
|---|---|
| Manifest lines | 65 (A), 263 (B), ~110 (C), ~35 (D) — the input |
| Generated Java + SQL lines | the output |
| **Hand-written Java or SQL** | **must be 0.** Non-zero is a friction-ledger row, not a footnote |
| Manual interventions during the gate | should trend to zero |
| Commands from empty directory to passing gate | `new` → `app apply` → `check` |
| Gate wall time | 293 s today; also a latency budget |

"Smarter generators mean you move faster" is measurable exactly as **hand-written
lines per feature trending to zero while the feature set grows**.

### 4.6 The genericity gate

Before a line lands in `src/`, all six must hold:

1. Can it be named without mentioning a showcase domain?
2. Is it useful to at least three materially different applications? — a
   question you can *answer* once C and D exist.
3. Is it a Spring/build/application concern rather than business behaviour?
4. Can a project decline it without weakening unrelated capabilities?
5. Does it lower through the same intent, capability and write path?
6. Does the generated application remain operable **without jails installed**?

Two mechanical guards:

- **A repository test that greps `src/` and `templates/` for showcase
  vocabulary** (`crawl`, `spider`, `conversation`, `workspace`, `inbox`,
  `payment`, `merchant`, `settlement`, `ledger`, `reconcile`) and fails on a
  hit outside a comment. One allow-list entry with a stated reason:
  `http_workflow_java.java` legitimately contains `robots`, because RFC 9309
  is a web standard rather than a domain noun. Make the reason the point.
- **`app plan` must stay domain-blind and write nothing** — already pinned;
  keep that test first in the file, it is the canary.

---

## 5. Production defaults, from a real system

Read out of `/home/laith/code/projects/payments-gateway-service` (22 modules,
332,397 lines of Java, Boot 4 / Java 26, Prometheus + Tempo + Grafana,
Hikari, Kafka, k6 load tests, Helm). **The point of this section is that every
row is domain-blind**: each one is a Spring/ops concern that any serious
service needs, so each one has a generic home in jails and none of them
introduces a payments concept.

The pattern to copy is not the config — it is that **each setting exists
because of a specific silent failure**, and the real system documents which.
That is exactly jails' bar.

### 5.1 What "batteries included" should mean

Rails' actual promise is not that it writes more code; it is that the defaults
are the ones an expert would have chosen, and you never have to know why. So
each row below becomes one of three things:

- a **generated default** in the capability that owns it,
- a **`doctor` check** when the failure is a misconfiguration jails cannot own,
- a **`why` rule** when the failure surfaces as a runtime symptom.

Never a fourth thing: an option the user has to discover.

### 5.2 Observability — `add observability` / `add actuator`

| Practice | Generic home | The silent failure it prevents |
|---|---|---|
| `management.server.port: 8081`, `base-path: /management` | `add actuator` | Actuator on the public port. Every k8s probe and Prometheus scrape then rides the same connector and thread pool as customer traffic |
| Probe **groups**: `liveness: include: ping` only; `readiness: include: ping, <real deps>` | `add actuator` | *This is the one everyone gets wrong.* A dependency check in the **liveness** group means a transient database blip makes Kubernetes **kill the pod**. The real system carries that reasoning as a comment; jails should generate the same comment |
| `management.endpoint.health.cache.time-to-live: 5s` | `add actuator` | A 10 s probe interval × N pods hammering a health check that queries dependencies |
| `exposure.include: health,info,prometheus,threaddump` | `add actuator` | `*` exposes `env`, `configprops`, `heapdump` — a credential leak with no error |
| **Explicit SLO buckets** on `http.server.requests` (`100ms,…,10s`) with **`percentiles-histogram: false`** | `add observability` | The default histogram is ~70 buckets **per endpoint per status** — a Prometheus cardinality bomb that nothing warns about. The real system's comment says the SLO list *doubles as* the histogram |
| Per-metric `percentiles` (`0.5,0.9,0.95,0.99`) and `minimum/maximum-expected-value` | `add observability` | Unbounded ranges produce useless buckets |
| Selective `management.metrics.enable.<name>: false` | `add observability` | Resilience4j and Kafka each emit dozens of series nobody reads; the real system disables six circuit-breaker series by name |
| `tracing.propagation.type: w3c`; `sampling.probability` explicit | `add observability` | Vendor-specific propagation silently drops the trace at the first hop you do not own |
| **`tracing.baggage.correlation.fields`** → MDC, plus `tag-fields` and **`local-fields`** | `add observability` | Two distinct failures: an id that never reaches the logs, and an internal id that **is propagated over the wire to third parties** because nobody listed it as local |
| Access log to stdout: `directory: /dev, prefix: stdout, suffix: "", file-date-format: ""` | `add observability` | A container writing access logs to a file nobody reads. The real system carries a second comment: the *management* connector auto-prefixes, producing `/dev/management_stdout`, **which a non-root user cannot write** |
| `info.app.*` from `@project.*@` Maven filtering | `add actuator` | `/management/info` that says nothing, so you cannot tell which build is running |

**A `MeterRegistryCustomizer` with `commonTags`, not `management.metrics.tags.*`** — jails already knows this
(`management.metrics.tags.*` was removed in Boot 3 and its replacement tags
*observations*, so a plain `Counter` goes untagged). Keep it, and note the
payments system's `pod.name: ${POD_NAME}` as the canonical common tag.

### 5.3 Data access — `add db`

| Practice | Generic home | The silent failure it prevents |
|---|---|---|
| **`pool-name` set per pool** | `add db` | Hikari metrics are labelled by pool name; unnamed pools all report as `HikariPool-1` and are unreadable |
| `connection-timeout: 1000`, `max-lifetime: 60000`, `initialization-fail-timeout` explicit | `add db` | A pool that blocks the request thread for 30 s (the default) instead of failing fast |
| `transaction-isolation: TRANSACTION_READ_COMMITTED` explicit | `add db` | Isolation inherited from the server and silently different between environments |
| **`connection-init-sql: SELECT 1/(1-pg_is_in_recovery()::int)`** on the write pool, and the inverse on the read pool | `add db`, when a read replica is configured | **The standout trick in the whole system.** A write pool that lands on a read replica fails at the first `INSERT`, in production, under load. This makes the pool refuse to start instead. Free, one line, and no ORM required |
| Separate pools per role, sized independently (20 primary / 5 read / 3 admin) | documented; `--module` territory | One pool for everything means a batch job starves the request path |
| `spring.lifecycle.timeout-per-shutdown-phase` + `server.shutdown: graceful` | `add db` / `new` | A rolling deploy kills in-flight transactions |
| `server.max-http-request-header-size: 16KB` | `new` | Large JWTs produce a 431 nobody can reproduce locally |

### 5.4 Build and quality gates — `new` and `add format`

The real root POM carries: `maven-enforcer-plugin` with **`requireJavaVersion`
and `requireMavenVersion`** (so a wrong local toolchain fails at `validate`,
not at a mysterious bytecode error), `jacoco-maven-plugin`,
`maven-checkstyle-plugin`, `editorconfig-maven-plugin`, `flatten-maven-plugin`,
`maven-dependency-plugin`, and both `surefire` and `failsafe`.

Generic homes: **`new` writes the enforcer rules** (jails already knows
`TARGET_RELEASE`, so this is free and it converts jails' most common `doctor`
FAIL into a build-time error with a fix line). **`add coverage`** owns Jacoco
with a stated threshold. **`add format`** already owns the formatter; add
`editorconfig` alongside it. `flatten-maven-plugin` matters only for
multi-module and belongs with `--module`.

### 5.5 Load and capacity — `add loadtest`

The real system ships a `load-tests/` directory with **k6** (`load-test.js`,
`api.js`, `payload-builder.js`, `token-cache.js`, a `Makefile`, a `README`).
Not JMeter, not Gatling — a JS file and a binary.

Generic form: **`add loadtest` writes a k6 script derived from the generated
routes** (`inspect.rs` already computes the route table) with bodies from
`sample_value` — the fourth reuse of that machinery after fixtures, factories
and `.http` files — plus a `Makefile` target and a `README` paragraph. Then
**`jails bench --load` records p50/p95/p99 into `.jails/benchmarks/`**, and
App C's contract asserts a p99 budget. A tool whose pitch is speed should
prove its own numbers.

Note this replaces `ideas-kimi.md` K11's `g load` (a Java `main` with
HdrHistogram), which had an unresolved invocation problem — `jails run` finds
"the file with `static void main`" and a second one creates ambiguity. k6 has
no such problem because it is not Java.

### 5.6 Deployment — `add docker` (shipped) and `add k8s`

`add docker` already generates a non-root multi-stage image running as
`10001:10001` — verified in the gate. What the real system adds and jails does
not: a **Helm chart** whose probes point at the **management port** by name
(`port: o11y`) with `failureThreshold: 5/3, periodSeconds: 10,
timeoutSeconds: 3`, and a `prometheus-rule.yaml` whose burn-rate alerts depend
on the SLO buckets in §5.2.

`add k8s` is a reasonable capability *after* §5.2 exists, because the probes
and the alert rules are only correct if the management port and the buckets
are. Sequence it last, and keep it to one deployment, one service, one
configmap, and probes — not a chart framework.

### 5.7 One honest counterweight

**`spring.threads.virtual.enabled: false`.** A Boot 4 payments system on Java
26 explicitly *disables* virtual threads and runs a bounded pool
(`threads: 100`). Four of the six source documents recommend virtual threads
as an unqualified default.

Do not read that as "virtual threads are wrong". Read it as: **a production
system with real throughput requirements made the opposite call, so jails must
not force it.** The generated default should be explicit and commented either
way, and `doctor` should carry the two real traps rather than an opinion: a
virtual-threads app whose only work is `@Scheduled` **exits 0 immediately**
unless `spring.main.keep-alive=true`, and pinning is observable via the JFR
`jdk.VirtualThreadPinned` event (on by default at 20 ms) — not via
`-Djdk.tracePinnedThreads`, which no longer exists on JDK ≥ 24.

### 5.8 The meta-lesson

The payments system's `AGENTS.md` is 166 lines and is the highest-signal file
in a 332,397-line repository. It encodes conventions an agent would otherwise
violate: package layout, `CREATE INDEX CONCURRENTLY`, partition-by-date,
reuse `java.util.Currency`, the exact `./mvnw test -pl <module> -am
-Dtest=<X> -Dsurefire.failIfNoSpecifiedTests=false` invocation.

That is §15.1's argument, with evidence: **`jails new` should write an
`AGENTS.md`**, and its content should be *rendered from* the same tables
`jails lint` and `jails commands --json` use, so it cannot drift into a lie.
Note the payments file already carries jails' own `failIfNoSpecifiedTests`
fix (§8.3) as documented tribal knowledge — which is exactly the kind of thing
a generator should be handing you instead.

---

## 6. Maintainability — options, ranked

`src/spring.rs` is **6,459 lines** and holds ~42 whole Java files as inline
`format!` strings, each opening `r#"package {pkg};`. Every brace in them is
doubled — the exact tax `src/template.rs` exists to remove — and none of it is
Java any editor or compiler can check. CLAUDE.md's stated reason for the
`add.rs`/`spring.rs` split (*"`add.rs` was already the biggest file here"*) is
now **inverted**: `spring.rs` is nearly twice `generate.rs` and 4.5× `add.rs`.

But file size is the symptom, not the disease.

### 6.0 First principles: what a generator is, and where everyone puts the pieces

A generator is a function `(intent, project state) → set of file writes`. Every
system in this space splits that into four separable concerns, and the quality
of the system is decided almost entirely by whether concern 4 is kept **out
of** concerns 2 and 3.

1. **Identity** — name, aliases, help text, option schema, preconditions.
2. **Content** — the bytes of each file.
3. **Placement / composition** — which files, where, in what order,
   conditionally.
4. **Decisions** — read project state and choose.

**Read against the checkouts in `ideas/`, not from memory** — the same rule
CLAUDE.md applies to templates. Citations are paths inside `ideas/`.

| System | Identity | Content | Placement | Decisions |
|---|---|---|---|---|
| cookiecutter | `cookiecutter.json` | Jinja tree | directory names + Jinja | none (hooks only) |
| copier | `copier.yml` | Jinja tree | same | none — but see §6.0.2 |
| Rails generators | Thor class | ERB | **Ruby method body**, a script of *actions* (`create_file`, `inject_into_class`, `gsub_file`) | Ruby |
| Angular / Nx schematics | **`collection.json` + `schema.json`** | templates | TS `Rule: Tree → Tree` | TS |
| OpenAPI generator | config + `-t` override | **Mustache — logic-less on purpose** | Java codegen class | Java |
| JHipster | JDL + Yeoman | EJS | sub-generators | JS + **blueprints** |
| OpenRewrite | **YAML recipe** with `preconditions` + `recipeList` | — | Java visitor | Java |
| Dart `build_runner` | `build.yaml` | `source_gen` builders | `*.g.dart` beside source | Dart |
| Spring Roo (dead) | annotations | AspectJ ITDs | round-trip weaving | Java |
| **jails today** | Rust enum + clap | mix of templates and Rust strings | Rust | Rust |
| **jails with option F** | TOML descriptor | templates | descriptor (simple) + Rust (conditional) | Rust |

#### 6.0.1 What the source actually shows

**Angular schematics is jails option F, already proven at scale.**
`ideas/angular/packages/core/schematics/collection.json` maps a name to
`{description, factory: "./bundles/x.cjs#migrate", schema: "./schema.json",
aliases: [...]}` — identity and the option schema are **data**; the logic is a
function the descriptor points at. The meta-schema
(`ideas/angular-cli/packages/angular_devkit/schematics/collection-schema.json`)
names exactly seven fields, and three of them are ones jails' descriptor
should copy and I had not considered:

| Field | Why jails wants it |
|---|---|
| `aliases` | already needed (`uc`, `djob`, `webhook`) |
| `factory` | the Rust function — jails' equivalent is the dispatch arm |
| `description` | becomes `jails g --help` |
| `schema` | the option schema; jails' is the field-spec grammar plus `--on`/`--yields` |
| **`extends`** | *"a schematic override… local or from another collection"* — the sanctioned way to specialise a generator without forking it. This is a better answer to "flexibility" than a plugin hook, and it is §6.6 Tier 2 done properly |
| **`hidden`** | listed by tooling or not — jails has kinds that exist for composition (`outbox` is reached through `usecase --yields`) and should not clutter `--help` |
| **`private`** | callable from another schematic but not from the CLI — exactly `outbox`'s status |

**OpenRewrite makes preconditions first-class data.**
`ideas/rewrite/rewrite.yml` is `type` / `name` / `displayName` / `description`
/ **`preconditions:`** / **`recipeList:`** — declarative *composition* of
units that are implemented in Java. jails' `requires = { spring = true,
capabilities = ["db"] }` is the same idea, and `recipeList` is what
`.jails/app.toml` already is one level up. Confirmation that the hybrid —
declarative composition, coded units — is the stable shape.

**OpenAPI generator's `-t/--template-dir` is real** and is at
`ideas/openapi-generator/modules/openapi-generator-cli/src/main/java/org/openapitools/codegen/cmd/Generate.java:88`,
wired at `:504`. §6.6 Tier 2 is not invented.

**Nx converged on Angular's format independently.**
`ideas/nx/packages/js/generators.json` is the same object — `factory`,
`schema`, `aliases`, `description` — plus one extra field, `x-type`, a
classification used for filtering. Two mature systems arriving at the same
descriptor shape is the strongest available signal that option F is the
stable design rather than a guess.

**cookiecutter is the concrete evidence for §6.6's refusal of hooks.**
`ideas/cookiecutter/cookiecutter/hooks.py:95` runs `pre_gen_project` /
`post_gen_project` through
`subprocess.Popen(script_command, shell=run_thru_shell)` — annotated `# nosec`
in the source. That is arbitrary shell execution from a downloaded template,
and it is exactly the line §6.6 says never to cross. The refusal is not
squeamishness; it is a citation.

**JHipster confirms the maximal plugin model, and its cost.**
`ideas/generator-jhipster/generators/base/command.ts:49` takes
`--blueprints kotlin,vuejs` and `--disable-blueprints`: npm packages that
override sub-generators wholesale. That is real power and it is also why
blueprint/core version skew is the standing cautionary tale — the override
surface is every sub-generator, so every core change can break a blueprint.

**Dart `build_runner` is the model jails deliberately rejects.**
`ideas/dart/build/example/build.yaml` declares `builders:` with `import`,
`builder_factories`, **`build_extensions: {".txt": [".txt.copy"]}`** and
`build_to: source` — a *derivation* model keyed on input extension, where the
output is a function of a source file and is regenerated every build. The
`build_to: source` versus cache switch is precisely the "is generated code
yours?" fork. jails takes the other branch: **you own and edit the generated
code.** Worth naming so nobody drifts toward it — the moment generated code is
"not yours", `g field`, `edited_files` and print-never-clobber all stop making
sense.

Three observations carried forward:

- **Rails' generator is a script of actions, not a template.** jails already
  has that vocabulary scattered across `pom::add_dependency`,
  `register_command`, `install_test_container_import` and the `@Import`
  merger — the independent argument for `src/codemod.rs` (§11).
- **OpenAPI generator picked Mustache *because* it has no conditionals**,
  independently arriving at `template.rs`'s rule.
- **jails with option F lands where Angular schematics landed.** That is not a
  novel design; it is a well-trodden point in the space, and the meta-schema
  above is a ready-made field list.

#### 6.0.2 When a DSL is actually justified

A DSL earns its place when you need to **analyze** the programs, not merely
run them — validate, diff, invert, or reconcile them.

jails genuinely needs that: `destroy` needs the inverse, `--pretend` needs the
plan, `app apply` needs drift detection. That is a real argument for data over
code, and it is why "just write more Rust" is not the whole answer.

But look at *what* it needs to analyze: **paths and ownership, never
content.** So the thing to model declaratively is the **artifact manifest** —
which files, where, owned by which intent — not the generator. A full DSL
would force you to encode the 90% that is never analyzed to reach the 10% that
is.

> **Model the output, not the process.** Declare the artifact set; keep the
> decisions in Rust.

That rule is what separates options B/D/F (worth doing) from a generator DSL
(§6.3, rejected).

**And copier proves the corollary**: for *reconciliation* you do not need an
ownership model at all — you need the stored inputs and the ability to re-run
the generator. See §11.1, which replaces this plan's earlier "output
fingerprints" design.

### 6.1 The disease, measured

**"What files does kind X produce?" is answered in five separate places, and
nothing checks that they agree.**

| Copy | Where | Size |
|---|---|---|
| 1. The generator | 14 `*_files` functions in `spring.rs`, all returning `Vec<(PathBuf, String, &'static str)>` | ~6,000 lines |
| 2. **The destroy path list** | `generate::destroy`, a `match kind` with **17 hand-written `vec![]` arms** | ~200 lines |
| 3. The golden scenario | `tests/common/scenarios.rs` `SCENARIOS` | **complete, and a test keeps it so** (§8.5) |
| 4. The editor lists | four Lua tables in `jails.nvim` | **stale by 8 kinds** |
| 5. The README table | prose | stale |

Copy 2 is the dangerous one. CLAUDE.md already warns about it — *"a kind added
to one and not the other silently strands files"* — and it is a **manual
transcription of paths that the generator right next door already computes.**
Adding `usecase` meant writing ten `format!("{name}…java")` lines twice.

Copies 1 and 2 are now *checked* against each other for every kind in the
scenario table (`tests/agreement.rs`, §6.2 A) — but checked is not the same as
single: the transcription is still there to drift, and the test only says so
after the fact.

So the maintainability question is not "how do we write less Rust". It is:
**how does "what kind X produces" stop having five definitions?**

### 6.2 The options

Ranked by value ÷ effort. A–C need no new file format at all, which is why
they come first.

---

**A. Generalise the agreement test.** ~~*Hours. No design.*~~ **DONE** —
`tests/agreement.rs`.

Every scenario in `tests/common/scenarios.rs` runs, the created files are
attributed to the command that wrote them, then `destroy --pretend` runs per
generate step and the two sets are compared **in both directions**: a path
destroy names that nothing created, and a file generate wrote that destroy
would strand. A leftover that is deliberate — a forward-only migration, a
fixture, a shared `SchedulingConfig` — is listed in `ALLOWED_LEFTOVER` with
its reason, scoped to the kind that earns it.

- **Bought**: it found a live bug on the first run — `g usecase --yields`
  writes `{name}OutboxSink.java` and `{name}KafkaOutboxSink.java`, and the
  destroy arm listed neither, so destroying the use case left a port nothing
  implements and an implementation of a deleted type: a project that stops
  compiling. Fixed in `src/generate.rs`.
- **Costs**: nothing structural; it did not reduce a line of code.
- **This is the evidence that makes B safe to attempt** — run it before and
  after and the sets must not move.

---

**B. Delete the destroy path list; derive it from the generator.** *~1 day.*

`destroy` calls the same `*_files` function `generate` does, and takes the
path out of each returned tuple. Copy 2 stops existing.

- **Buys**: kills a documented class of bug outright. No new format, no new
  concept, ~200 lines deleted.
- **Note**: this is the right mechanism for `--pretend`, where nothing has
  been written yet. For `destroy` after a jails upgrade, prefer the **recorded**
  file list of §11.2 — a recomputed path gives you today's answer for
  yesterday's file.
- **Costs**: some `*_files` read records off disk (`fields_from_record`) to
  decide what to emit, and at destroy time the record may be the thing being
  deleted. Two mitigations, both already precedented in the tree: make
  rendering lazy so paths are computed without bodies, and keep the
  `g strategy` pattern where destroy deliberately reads disk to find
  implementations added by hand.
- **This is the highest value-per-day item in the whole section**, and the
  previous draft of this plan missed it entirely.

---

**C. Finish the template migration.** *Ongoing, incremental.*

Every `r#"package {pkg};` block becomes `templates/spring/*.java`. Already the
house pattern; `add/` is done (7 `include_str!` against 2 `format!`).

- **Buys**: `spring.rs` from ~6,459 to roughly 2,500 lines *of decisions*;
  Java that an editor highlights and a human can review as Java; no doubled
  braces.
- **Costs**: none beyond the work. Each extraction is independently
  reviewable and golden-testable.
- **Do it as you touch each generator, never as a big-bang refactor.**

---

**D. A typed artifact builder.** *~2 days, Rust only.*

Replace the `Vec<(PathBuf, String, &'static str)>` convention with a builder
each generator declares into:

```rust
Artifacts::new(&root)
    .main(layout::SERVICE, "{name}Command.java",   tpl::USECASE_COMMAND)
    .main(layout::SERVICE, "Default{name}UseCase.java", tpl::USECASE_IMPL)
    .test(layout::WEB,     "{name}ControllerTest.java", tpl::USECASE_CTRL_TEST)
```

- **Buys**: path and template declared together and once; `destroy` reads
  `.paths()`; each generator's *shape* becomes readable at a glance instead of
  buried in 400 lines of string building. Makes B fall out for free.
- **Costs**: 14 call sites to convert; still Rust, so no external tool can
  read it.
- **B and D are really one move** — do D as the mechanism, B as the result.

---

**E. Data-ise the type table.** *~1 day.*

`builtin_by_java_name` (`generate/field.rs`) and `builtin_mapping` (`sql.rs`)
are a pure lookup table — java type ↔ SQL type ↔ read expression ↔ write
expression ↔ sample value — expressed as Rust `match` arms in two files that
must stay in step.

```toml
# types/instant.toml
java = "java.time.Instant"
sql  = "timestamptz"
read  = "rs.getTimestamp(\"{col}\").toInstant()"
write = "Timestamp.from({recv})"
sample = "Instant.parse(\"2026-01-01T00:00:00Z\")"
imports = ["java.time.Instant", "java.sql.Timestamp"]
```

- **Buys**: adding a field type becomes one file instead of edits in two
  modules; the round-trip property (`reverse(forward(t)) == t`) becomes a
  table test; `jails inspect db` (reverse mapping) gets its table for free.
- **Costs**: small; the `write` expression must keep baking in the receiver
  (`Timestamp.from(x.at())`, not `x.Timestamp.from(at())`) — a documented trap.
- **Genuinely declarative, genuinely low-risk.** This is data, not logic.

---

**F. One descriptor file per kind.** *~1 week.*

```toml
# kinds/usecase.toml
name     = "usecase"
aliases  = ["uc"]
summary  = "A create workflow with a transaction boundary, route and tests"
requires = { spring = true, capabilities = ["db"] }
args     = { on = "required:Resource", yields = "optional:Event", fields = "spec" }

[[artifact]]
template = "spring/usecase_command.java"
path     = "{service}/{Name}Command.java"

[golden]                                   # REQUIRED KEY
fixture = "spring"
steps   = [["g","scaffold","Note","id:uuid@pk","title:string!"],
           ["g","usecase","CreateNote","id:uuid","title:string!","--on","Note"]]
```

Copies 1–5 collapse to one file, consumed by: the `ArtifactKind` enum and
clap aliases (generated in `build.rs`), `--help` text, `destroy`'s paths, the
golden scenario table, `jails commands --json` (which then *deletes* the Lua
lists rather than pinning them), the README table, and `AGENTS.md` in
generated projects (§5.8).

- **Buys**: the structural fix. And one property nothing else on this list
  has: **`[golden]` is a required key, so it becomes impossible to add a kind
  without a snapshot test** — converting §8.5's recurring discipline failure
  into a compile error.
- **Costs**: a week; a `build.rs`; a second place to look when reading a
  generator; and a standing risk of drifting into a template language.
- **Scope rule**: descriptors hold **data** — names, aliases, preconditions,
  template→path pairs, golden steps. Never logic. The test: *could this be
  wrong in a way only a human reading the generated Java would notice?* If
  yes, it is logic and it stays in Rust. `usecase`'s compatibility check and
  inference engine are logic.
- **Do F after A–D**, because A–D change what a descriptor needs to hold. A
  format designed before the agreement test exists will forget the golden key.

---

**G. Path metadata in the template header.** *Considered, not recommended.*

```java
// jails:path {service}/{{name}}Command.java
```

- **Buys**: no new files at all; the template *is* the descriptor; impossible
  to add a template without declaring where it goes.
- **Costs**: ordering and conditionality are not expressible, and roughly a
  third of jails' artifacts are conditional (`--yields` adds the outbox;
  `repository_wiring` changes the adapter shape). You would end up with the
  header for simple kinds and Rust for the rest — six copies instead of five.
- **Rejected**, but worth recording so nobody re-proposes it.

---

### 6.3 What not to build

**A template language with conditionals and loops.** It would shrink Rust and
grow something worse: logic no test can reach directly and no compiler can
check. `template.rs`'s module docs already rule it out and the reason holds.
Substitution only; anything structural stays in Rust and arrives pre-rendered.

**A general "generator DSL" in YAML/JSON that describes how to build Java.**
That is a plugin system with a different file extension, and README's "Not
yet" defers it for good reasons.

**Codegen from an external schema language** (JSON Schema, protobuf). Two
build steps and a dependency, for a config file with fifteen keys.

### 6.4 Recommended path

1. **A** (hours) — the agreement test over all 30 kinds. Do this in the same
   sitting as §8.5's golden-coverage test; they are the same shape and the
   same argument.
2. **B + D** (~2 days) — the artifact builder, and delete the destroy list.
   This is the one that removes a real bug class.
3. **C** (ongoing) — templates out of `spring.rs` as you touch each generator,
   plus §6.5's file split.
4. **E** (~1 day) — the type table as data. Independent of everything else.
5. **F** (~1 week) — descriptors, once A–D have settled what they must hold.

Steps 1–2 are three days and remove the duplication that actually causes bugs.
Steps 3–5 are the long-term shape.

### 6.5 The file split, whichever option you take

`spring.rs` is one file for two unrelated reasons: Spring *capabilities* and
Spring *generators*. Mirror the convention `add/` and `generate/` already use:

```
src/spring.rs            -> require_spring, exposure_include, shared helpers
src/spring/capability/   -> api.rs actuator.rs cache.rs security.rs observability.rs
src/spring/workflow/     -> usecase.rs query.rs transition.rs
src/spring/durable/      -> job.rs outbox.rs sink.rs
src/spring/http/         -> client.rs fetcher.rs workflow.rs
src/spring/schema.rs     -> association.rs
```

and **fix CLAUDE.md's rationale in the same change**, so the next split is
made on a true premise.

### 6.6 Extension: four tiers, and where a plugin system is and is not safe

README defers "any kind of plugin system", and all six source documents list
it as an anti-goal. That deferral is right about *one* form of plugin and
wrong as a blanket answer, because "I want flexibility" decomposes into four
wants with wildly different costs.

**What a plugin system actually buys** is third-party extension without a core
release, plus per-team customisation. For a tool whose maintainer and user are
the same person, the first is worth approximately nothing — you can add a kind.
What it **costs** is permanent: a public API surface that must stay stable
forever, version skew between core and plugins (JHipster blueprints are the
standing cautionary tale), untested combinations, and the loss of the property
that every generated file is golden-tested.

So do not ask "should jails have plugins". Ask which tier is wanted:

| Tier | The want | Mechanism | Cost | Status |
|---|---|---|---|---|
| 1 | "put generated code somewhere else" | `jails.toml [layout]` | — | **exists**, and §12's `jails adopt` extends it |
| 2 | "change what the generated code *looks like*" | `--template-dir` / `.jails/templates/` override, resolved before `include_str!` defaults | small | **worth doing** |
| 3 | **"add a new generator"** | **data-only kind**: a descriptor plus templates dropped in `.jails/kinds/` or `~/.config/jails/kinds/` | falls out of option F | **the safe plugin system** |
| 4 | "add a generator that makes decisions" | Rust, and a release | — | unchanged |

**Tier 2 — template overrides.** OpenAPI generator's `-t` flag is the
precedent, and it is the flexibility people actually reach for: not a new
generator, just *this* class shaped differently. Resolution order becomes
`.jails/templates/<name>.java` → `~/.config/jails/templates/<name>.java` →
the `include_str!` default. Cheap, and independent of every other item here.

The honest cost: **an overridden template is not golden-tested**, so a project
that overrides one has opted out of the guarantee for that file. Mitigate by
having `doctor` report every active override by name — the same honesty rule
as `remove`'s `unowned_properties`, which names hand-written lines before
deleting a block.

**Tier 3 — data-only kinds, which is a plugin system in the only form that
does not break jails' guarantees.** A kind expressible as

- metadata (name, aliases, summary, preconditions),
- a list of `template → path` pairs,
- a `[golden]` block,

with **no conditionals**, needs no Rust. Dropping such a directory in gives a
user-defined generator with no arbitrary code execution, no API to keep
stable, and — because the descriptor carries its own golden steps — output
that is still snapshot-tested. `destroy` works on it for free, because the
path list is the same data.

**The line is precise and checkable: data is extensible, logic is not.** A
kind that needs an `if` is logic and belongs in core.

**The slope, and the guard.** The first person to want a conditional in a
data-only kind will ask for one, and granting it is how this becomes JHipster.
Two guards, both cheap:

- **Refuse conditionals outright** in the descriptor schema — an unknown key
  is an error, the same closed-set rule `jails.toml` and the field markers
  already use.
- **Make the boundary visible**: `jails commands --json` and `doctor` report
  which kinds are core and which are data-only, so nobody discovers by
  accident that half their generators are unversioned local files.

**What stays refused, at every tier**: lifecycle hooks, arbitrary shell,
downloadable packs, and anything that executes code at plan or apply time.
Those are the plugin system README defers, and nothing above requires them.

## 7. Corrections ledger — do not build these

| Claim, and where it came from | What the source says |
|---|---|
| **AOT cache pays on every devtools restart, `mvn test` fork and `jails run`** (`ideas-opus.md` A2) | The cache **refuses any classpath containing a directory**, and `target/classes` is one. All three named loops are out. A devtools restart is also a new classloader in the same JVM, so there is no process start to save. AOT is real — 6.6 s → 2.96 s on a **jar** classpath — and belongs to `jails build` / `add docker` |
| **`-XX:+AllowEnhancedClassRedefinition` covers ~90% of edits** (`ideas-opus.md` A1) | Not an OpenJDK flag; it is JetBrains Runtime / DCEVM. The documents ruled it out because JBR tops out at JDK 25 while `TARGET_RELEASE` was 27 — **it is now `"25"`, so the path is reachable.** Still not a default: stock JVMTI is method bodies only, and jails' domain layer is records and sealed types, so **every domain edit is a restart on a stock JVM** |
| **JDWP `RedefineClasses` is command set 2** (`ideas-opus.md` A1) | Command set **1**; set 2 is `ReferenceType`. A working client is ~400 lines, not 150. Use jdt.ls's HCR or `jdb redefine` first |
| **`SseEmitter`'s never-time-out value is `Long.MAX_VALUE`** | Spring's own reactive path uses **`-1L`**; Spring's default is `null` and the 30 s is Tomcat's `Connector.asyncTimeout` |
| **Intercom webhooks are `X-Hub-Signature-256` / SHA-256** (`ideas-grok.md` §8.1) | Intercom signs `X-Hub-Signature` with HMAC-**SHA-1**, `sha1=` prefix, keyed by `client_secret`. A verifier built to that spec **rejects every real delivery** |
| **minicom is "users → conversations → messages"** (`ideas-opus.md` B1, `ideas-grok.md` §8) | Its entire success condition is an alert reading `Yay! Everything works`. **No messaging code and no conversations table.** Two documents designed against a product that does not exist |
| **A crawler needs jsoup and crawler-commons** (`ideas-grok.md` §7, `ideas-fable.md` §6.1) | `http_workflow_java.java` parses with the JDK's `HTMLEditorKit` and fetches `/robots.txt` as a frontier entry. **Zero new dependencies.** The `add crawl` capability those documents specify is unnecessary — do not add it, do not clone jsoup |
| **Relations should be inferred from an `author:User` component** (`ideas-grok.md` §6) | `g association` does it **explicitly**, with both records read, types checked across the boundary, composite keys free, identifier length checked, no `ON DELETE` invented. Explicit beat inferred. What survives is narrower — §9.2 |
| **`jails run --watch` already pipes through `why`** | It does not — §8.4 |
| **A `notify` crate would be the second dependency** | Third: `clap` and `clap_complete` are both declared. Polling is still right |
| **`rails test --only-failures`** | RSpec's, not Rails'. Rails prints a copy-pasteable `bin/rails test path:LINE` — copy that instead |
| **Boot 4 sets `spring.threads.virtual.enabled=true`** (`ideas-grok.md` §8.3) | Default is **`false`** — and see §5.7, where a production system sets it to `false` deliberately |
| **`-XX:TieredStopAtLevel=1` / `spring.jmx.enabled=false` are speed tips** | `spring-boot:run` already passes the first; JMX is already off. For STS4 live hover you want JMX back **on** |
| **Mint JWTs with Nimbus directly** (`ideas-grok.md` §8.2) | A level too low. Spring Security 7 ships `NimbusJwtEncoder.withSecretKey` (`@since 7.0`). The silent failure: **a JWT with no `exp` passes the default decoder**, and the default chain checks no issuer and no audience |
| **Toxiproxy's Java bodies are still `format!` strings** (`ideas-fable.md` §8.11) | **Fixed.** `add/testing.rs` is 7 `include_str!` against 2 `format!` |
| **`g load` with HdrHistogram** (`ideas-kimi.md` K11) | Superseded by §5.5 — k6, which sidesteps the "two `main` methods" invocation problem entirely |
| **CLAUDE.md: the manifest is `deps/deps.tsv`** | It is `deps.tsv` and `deps-update.sh` at the repo root |
| **`withReuse(true)` is "safe unconditionally" and the largest lever on the 293 s gate** (§10.1, this document) | **False, and it was tried.** The reuse key is `sha1` of the serialised `CreateContainerCmd` (`GenericContainer.hash`), and **nothing in it identifies the project** — so every jails project on `postgres:17` reuses the *same* database. Both number their migrations from `V001`, so Flyway refuses to start: *"Migration checksum mismatch for migration version 001 — applied to database 544218698, resolved locally 656450728."* The verification gate went red on the support inbox inheriting the crawler's schema history. A per-project label would fix the hash, but nothing deterministic and portable is unique per project — package and coordinates are `com.example.demo` in half the world. **So the generated config does not ask for reuse**; it documents the one-line change, `jails setup` writes the machine flag, and `doctor` counts what reuse leaves running |

---

## 8. Tier 0 — the debt behind the new code

All re-verified against `e523c16`. The tree **builds**; the previous pass's
blocker (`spring::transition_files` missing) is gone.

| # | Defect | Evidence | Fix | Effort |
|---|---|---|---|---|
| 8.1 | ~~**`g scaffold` and `g dto` write a `pom.xml` Maven cannot read**~~ **CLOSED** | was `mvn -o test` on a `new-cli` project: `'dependencies.dependency.version' … is missing` | `spring::validation_dependency` picks by `pom::flavor`: the starter under a Boot parent, and pinned `jakarta.validation:jakarta.validation-api` -- the artifact the generated code actually imports -- without one, so a plain project does not get Boot dragged into it either. `spring::failsafe_plugin` pins the plugin version the same way. `ensure_assertj` is the same rule for the test dependency every generated test needs | done |
| 8.2 | ~~**The golden suite ratifies that broken pom**~~ **CLOSED** | `tests/golden/scaffold-plain/pom.xml` | Regenerated and read: the versionless Spring starter is gone, the validation API and AssertJ are pinned, Failsafe carries a version. The plain **fixture** was also invalid -- no `modelVersion`, no `version` -- which is why nothing had ever built it | done |
| 8.3 | ~~**`jails test 'Class#method'` silently runs the wrong thing and exits 0**~~ **CLOSED** | was `src/run.rs`: `format!("{f}Test")` then `if test_name.ends_with("IT")` | `expand_filter` suffixes the class half only, `split_method` decides Surefire vs Failsafe on the class, and both `failIfNoSpecifiedTests` flags are passed so an empty filter is "no tests ran" rather than a stack trace. `test_command_infers_unit_and_integration_test_names` covers `Payout#settles` and `PayoutIT#settles` | done |
| 8.4 | ~~**`run --watch` cannot report a failed startup, and the watcher only stats `.java`**~~ **CLOSED** | was `src/run.rs` | `watch` runs `spring-boot:run` through `run_watched` on its own thread and polls the filesystem on the main one, so a startup that dies is reported with `why`'s explanation instead of watched in silence. `fingerprint` is a `BTreeMap<PathBuf, SystemTime>` over main/test java **and resources**, plus `pom.xml`, `compose.yaml` and `jails.toml`; `changes_between` compares with `!=` and names each file as added/changed/**deleted**. Three unit tests, including `git checkout`'s backwards mtime | done |
| 8.5 | ~~**Twelve kinds and three capabilities have zero golden coverage**~~ **CLOSED.** Seven scenarios added (`usecase-query-transition`, `association-durable-job`, `fetcher-workflow`, `outbox-http-sink`, `cases`, `cap-ci`, `cap-docker`): **308 files / 32 scenarios**, up from 162 / 25 | was `grep -c '"http-workflow"' tests/golden.rs` → 0 | `every_kind_and_capability_has_a_golden_scenario` reads `jails generate --help` / `jails add --help` and fails on a kind with no `Scenario`. `format` is the one exemption, in `COVERED_ELSEWHERE` with the test that covers it and an assertion that the test still exists. The table itself moved to `tests/common/scenarios.rs`, shared with `tests/agreement.rs`, and *which* kinds a scenario covers is derived from its steps rather than declared beside them | done |
| 8.6 | **`spring.rs` at 6,459 lines with ~42 inline Java bodies** | `grep -c 'r#"'` → 42 | §6 | ongoing |
| 8.7 | ~~**Drift**~~ **CLOSED.** `g cases`, `usecase` and `query` now have README entries; `jails.nvim` has all 30 kinds, all 18 capabilities and `app`; `validation/README.md` says which of its seven assumed features shipped (all of them) and what it does not cover; CLAUDE.md's golden-suite, destroy-hazard and JDK entries are rewritten | was greps | `tests/editor.rs` reads the Lua tables and the binary's own help and **fails when the plugin cannot complete something the CLI accepts** -- the drift is checked now, not just fixed | done |
| 8.8 | ~~**Nothing cheap asks whether the generated project builds**~~ **PARTLY CLOSED** | The structural cause of 8.1 | `every_generated_pom_is_one_maven_can_read` runs `mvn -o validate` over `{plain, spring}` x four pom-touching kinds, ~9 s for the lot. Still open: the `{none, db, json}` capability axis, and `validate` parses the pom without compiling -- a plain-project scaffold still emits Spring MVC code it cannot resolve, which is the next cell to add | partly |
| 8.9 | ~~**`doctor` reports health over a pom Maven refuses to parse**~~ **CLOSED** | was `pom::read` + `unwrap_or_default` | `pom::problems` names every structural reason Maven would refuse the file -- no `modelVersion`, no inheritable `version`, a versionless dependency with no BOM -- and `project_check` FAILs with the first one and its fix. Structural only: `doctor` is read-only, so it cannot run `mvn validate`, and it does not need to | done |


---

## 9. Tier 1 — the authorship engine

jails can create almost anything and change almost nothing.

### 9.1 `g field` — the highest-value generator jails does not have

```
jails g field Payment settledAt:instant?
```

Reads the record with `fields_from_record`, refuses a duplicate component,
appends in declaration order, then rewrites **only the derived files that
still match what jails would have written**, printing snippets for the rest:

```
updated  domain/Payment.java
updated  web/PaymentRequest.java
created  db/migration/V021__add_settled_at_to_payments.sql
skipped  adapters/JdbcPaymentRepository.java -- you have edited this file
         add to the select list:  settled_at
         bind:                    ps.setObject(9, …)
```

**The refusal is the design.** The ownership oracle is `edited_files`
(`src/add/database.rs`) — nine lines that re-render the template and diff the
bytes. It over-reports after a jails upgrade; over-reporting prints a snippet
you paste, over-writing destroys work. **Print, never clobber.**

Migration from `sql.rs`, forward-only. A `not null` column on a populated
table needs a default, so the generated SQL carries one and says so.
**`--remove` is not in v1** — dropping a column is a data decision.

### 9.2 The narrow relation gap that survives `g association`

`g scaffold Post id:uuid@pk author:User` still emits `author text not null`
plus a Javadoc reading *"Not persisted, because jails has no mapping for the
type"*. **The app compiles, starts, returns 201, and the author is gone.**

The fix is now small because `association` proved the machinery: when a
component's type is a record in this project with exactly one `@pk`, **refuse
the scaffold** and name the two commands that do the job. A refusal that
teaches beats both a silent `text` column and a second inference path.

### 9.3 ~~The pluraliser is `+ "s"`~~ — **CLOSED**

`sql::table_name` is the single owner and now applies `…y` after a consonant
→ `ies`, `…s|x|z|ch|sh` → `es`, `…f|fe` → `ves` (but not `ff`), a short
irregular list matched on the **last word** (`SupportPerson` →
`support_people`), and a short uncountable list (`equipment`, `metadata`,
`news`). No `jails.toml` override: derivability is what lets `destroy` find
what `generate` wrote.

The half that was still live is the interesting one, and it is what "one
owner" means: `web::resource_path` had its own `push('s')`, so
`g handler Category` served **`/categorys`** over a table called
`categories`, while the Spring scaffold's controller — which did go through
`table_name` — disagreed with it about the URL of the same resource. It
delegates now, and `the_route_path_and_the_table_are_the_same_pluralisation`
pins the two together.

### 9.4 One rule for where fields come from

Eleven call sites read a record off disk and disagree about failure. `g repo`
errors; **`g dto` uses `unwrap_or_default()` — the one silent case**;
`usecase`, `query`, `transition`, `durable-job`, `association` and `outbox`
each raise their own wording. **And `g scaffold` does not read it at all.**

So model-first is blocked on the kind spanning the most files, while eight
newer kinds *require* it. State the rule once: spec if given, else the record
on disk, else an error naming the record and the fix.

### 9.5 `--timestamps`

Absent. **Half of it exists in the wrong half of the tool**: `usecase` already
infers timestamps. What is missing is the DDL and adapter side, where the lie
lives — `updated_at` is a column nothing updates. All four proof manifests
hand-declare `createdAt:instant`.

### 9.6 `g factory`, `requests/*.http`, and refusals

**`g factory Payment`** — defaults from `sample_value`; a component jails
cannot sample starts `null` and `build()` **throws naming it**, never a
guessed default. **`requests/payment.http`** as a `g scaffold` side artifact.

**Refusals are ergonomics.** `jails: …/fixtures/payments.json already exists`
is the message for the most common mistaken command in the tool. It should
name the cause and the next command. `doctor` is held to this standard by a
test asserting every `FAIL` carries `fix:`; **generators are not — add the
same test.**

### 9.7 The manifest is the ergonomic unit, and editing a field breaks it

`app.rs`: `plan` runs `add::preflight` and writes nothing; `apply` installs
capabilities, runs each pending intent, **writes state after every one** (so
an interrupt resumes), then **reconciles every capability a second time**.

The gap is the state key — `kind|name|package|fields|indexes|on|yields`.
**Change a `fields` line and you change the key**, so the old intent stays in
state and the edited one arrives *pending*; `apply` calls `generate`, which
finds the files and refuses. **It fails, with §9.6's useless message.** At 263
manifest lines that is not theoretical. `g field` is the primitive;
`DOGFOOD.md` already names the durable fix: *"store output fingerprints and
reconcile drift instead of blindly skipping."*

### 9.8 What not to build here

No ORM, no lazy loading, no `g field --remove` in v1, no inflector overrides,
no rewriting an applied migration, no provenance ledger as a *prerequisite*
(§11), no `makemigrations` autodetect. **And no `money` field type** — App C
uses `amountMinor:long` plus a currency string, which is what the real
payments system does; a `money` type would be a domain concept in core and
would fail §4.6 question 1.

---

## 10. Tier 2 — the latency engine

### 10.1 Free wins

**~~Testcontainers reuse — the largest lever on the 293 s gate.~~ Tried, and
it is not safe by default.** See §7: the reuse key is a hash of the container
configuration, nothing in it identifies the project, so two jails projects on
one image share a database and Flyway rejects the other one's migration
history — the gate went red. What shipped instead: `jails setup` writes the
machine flag (`testcontainers.reuse.enable=true` in
`~/.testcontainers.properties` — **not** the classpath, which is the trap),
`doctor` reports whether it is on and **counts the containers reuse leaves
behind** (a reused container is never registered with Ryuk, so nothing reaps
it), and `TestcontainersConfig`'s Javadoc states the one-line change and
exactly what it costs. The saving is real for a single-project machine; it is
the reader's decision, not a default.

**`META-INF/spring-devtools.properties` — done.** `new` writes it: poll 200 ms,
quiet 50 ms, against Boot's 1 s / 400 ms, so a save costs up to 1.4 s less
before the restart begins. Verified in `DevToolsSettings`: `defaults.*`
entries become the **last** property source, so anything the reader sets wins,
and they apply only when devtools is active locally — zero effect on the
packaged jar and zero in tests. `spring.docker.compose.enabled=false` is
deliberately *not* here: `add db` owns that property in its own marked block,
and two owners is how a property ends up with two values.

**`META-INF/spring-devtools.properties`** — `defaults.*` apply only when
devtools is present, zero effect on the packaged jar. Poll 200 ms, quiet 50 ms
(defaults are 1 s / 400 ms), and `spring.docker.compose.enabled=false` for
this machine's podman problem.

**`jails test` flags** — `-o -q -ntp`, `-Dsurefire.failIfNoSpecifiedTests=false`,
`Class#method`, `--fail-fast`, `--failed` (parse surefire XML, ~30 lines),
`--slowest`, and **print the rerun line on failure**. `--retry` off by
default.

**`jails test <file>:<line>`** — resolve the enclosing `@Test` with
`java::blanked()` + `java::annotations()`. Jupiter never resolves a
`FileSelector`, so jails must do it. Nested classes are `Outer$Nested#method`.

**`why` on every Maven failure**, not just watched runs. Non-zero exit → run
the tail through `why::explain`. Multiplies all 20 rules.

### 10.2 `jails test --fast`

**Step 1, console launcher.** Splice `junit-platform-console` test-scoped with
**no version** (Boot's parent imports `junit-bom`), then
`java @cp.args org.junit.platform.console.ConsoleLauncher execute
--select-method … --details=testfeed --fail-if-no-tests`. `cwd` must be the
module root. Estimated 0.35–0.6 s vs 2.57 s — **unverified; measure first.**

**Step 2, `jails testd`** — one resident JVM holding
`ToolProvider.getSystemJavaCompiler()` and the `"junit"` provider over a unix
socket. Compile in-process (74–166 ms warm), run via
`LauncherFactory.openSession()` (9–13 ms warm), fresh `URLClassLoader` per
run. **That classloader cost is the one unmeasured piece.**

**Step 3, `--affected`** — a reverse-dependency index from `.class` constant
pools: ~120 lines (skip entries by tag width — `CONSTANT_Long`/`Double` take
**two** slots — keep `Utf8` and `Class`, scan `Utf8` for `L<pkg>/<Class>;`).
Blunt rules for Spring; **unknown ⇒ run**; exclude `*IT` by default.

**The correctness price:** compiling only the changed file is unsound — a
removed method leaves a stale caller. Which is why **`jails check` stays `mvn
clean verify`** and every fast path falls back to it loudly.

### 10.3 `jails dev`

Watcher (8.4's replacement, 150–250 ms poll, 400 ms quiet, plus Quarkus' extra
200 ms when a file is size 0); compile with `javac -J-XX:+AutoCreateSharedArchive`
(**0.25 s vs 1.45 s**); **classify before acting** — method body → swap;
record component, `sealed`, annotation, new class, field or signature →
**restart, printing the JVMTI reason by name**; `pom.xml` → full restart.
**jails' domain layer is records, so every edit there is a restart** — say so
or it looks broken. Swap via jdt.ls's java-debug bundle (free, with frame
popping) before `jdb`, before a Rust JDWP client. Write
`target/classes/.jails-reload` only after a successful compile and point
`spring.devtools.restart.trigger-file` at it. Pipe through
`why::FATAL_MARKERS`. Quarkus' key map. `--timings` on everything.

**Check first:** with m2e setting the output folder to `target/classes` and
devtools watching classpath directories, `:w` → jdt.ls writes the class →
devtools restarts in ~1.4 s, with no Maven. If that holds here, `jails run
--hot` is a README paragraph and a doctor check rather than a supervisor.

### 10.4 `jails run --tc`, `runner`, `boot`

`mvn spring-boot:test-run` with a generated `TestApplication` doing
`SpringApplication.from(App::main).with(TestcontainersConfig.class).run(args)`
and `@RestartScope` on the container bean gives a dev run backed by the
Testcontainers config `add db` **already generates**, with no compose — which
also routes around podman-compose. **`jails runner -e`** boots the context and
`getBean`s for the non-interactive case. **`jails boot`** =
`-Dspring.context.exit=onRefresh`: a startup smoke test with no port, and the
AOT training-run switch. This replaces "boot a context inside jshell", which
dies on the DataSource.

---

## 11. Tier 3 — lifecycle: the clauses `ACCEPTANCE.md` still names

Sequence, rather than `ideas-sol.md`'s 3–5 week `ChangeSet` up front:

1. **`g field` first** (§9.1) — drift reconciliation for a changed manifest
   line *is* that command.
2. **Regenerate and 3-way merge** — see §11.1, which replaces the "output
   fingerprints" design this plan used to carry. Less code, no new file
   format, and it reuses `git merge-file`.
3. **Then** one atomic plan, if it still earns its keep.

Keep: paths normalised and confined; all conflicts detected before the first
write; `--pretend` and apply rendering the **same** object; expected hashes,
not string matching; a second identical apply a no-op. **A sequence of
per-file renames is not an atomic transaction** — promise deterministic
preflight plus crash recovery, and say so.

**Structural note:** `write_new_file` is *not* the single choke point it looks
like — `src/add.rs` writes an existing path directly with `fs::write`,
bypassing the collision check. A ledger hung off `write_new_file` alone has a
hole exactly where a capability updates a file it previously wrote.

**`src/codemod.rs`** — collect the splice primitives (`pom::add_dependency`,
compose blocks, property blocks, `register_command`,
`install_test_container_import`, the `@Import` merger, the `jails.toml`
one-liner) under named operations. Same extraction as `process.rs`; pays on
every capability and is a prerequisite for §6.2 option F.

**`new --offline`** closes the third clause in a day: vendor
`write_spring_fixture` via `include_str!`, explicit flag, and when
start.spring.io fails the error *suggests* it rather than silently falling
back.

---

### 11.1 Drift repair: regenerate and 3-way merge, not an ownership ledger

**This supersedes the "output fingerprints" design earlier drafts of this plan
carried.** `ideas/copier` solves the same problem and solves it better, and
jails is unusually well placed to copy it.

**Copier's `_apply_update`** (`ideas/copier/copier/_main.py:1377`):

1. Regenerate the **old** template version into a temp dir, using the
   **stored answers** (`subproject.last_answers`) and the **stored commit**
   (`subproject.template.commit`).
2. Regenerate the **new** version into a second temp dir.
3. Diff old-generated against new-generated.
4. `git apply` that diff to the user's real project, and where it conflicts,
   **`git merge-file`** — a 3-way merge leaving conflict markers
   (`_main.py:1610-1642`).

**The insight: you never need per-file ownership hashes.** You need the stored
inputs and the ability to re-run the generator. The diff between old-output
and new-output *is* exactly what jails changed; git decides how it lands on
top of the user's edits, which is a problem git is far better at than any
hash comparison.

#### Why this fits jails better than it fits copier

The hard part for copier is step 1 — it must check out an old template commit.
**jails' most important case does not need that at all.** For the §9.7 failure
— a `fields` line edited in `.jails/app.toml` — the *generator* is unchanged;
only the *intent* differs. So:

```
regenerate intent with the OLD fields   -> temp A        (jails is deterministic)
regenerate intent with the NEW fields   -> temp B
git diff --no-index A B                 -> patch
git apply --3way <patch>                -> the project
```

No version pinning, no ledger, no ownership model. `.jails/app-state-v1`
already stores the old intent — that *is* `last_answers`. And `--pretend`
becomes "print the patch", which is strictly more informative than the list of
paths it prints today.

#### What it costs, stated honestly

- **It needs a git repo.** `jails new` runs `git init` by default and
  `--no-git` exists, so the fallback is: no repo → fall back to today's
  behaviour and say so.
- **It leaves conflict markers** in a `.java` file when the user has edited
  the same lines. That is correct and is what every developer already knows
  how to resolve — and it is strictly better than `g field`'s "print the
  snippet and refuse", which is the right call only while this does not exist.
- **The jails-upgrade case is genuinely harder, and Nx has the better
  answer.** When the *template* changed rather than the intent, step 1 would
  need the old jails binary to regenerate old output. Do not go there.
  `ideas/nx/packages/js/migrations.json` shows the alternative: a
  `generators` map whose every entry carries a **`version`**, a
  `description` and a `factory`, so upgrading collects every migration newer
  than the project's recorded version and runs them in order — transforming
  the existing project forward instead of reconstructing its past.

  That is the same shape as Flyway, which jails already uses for SQL, so it
  is idiomatic here: a jails release that changes a template ships a
  migration declaring the version it applies at, and `.jails/version`
  (§11.2) is the project's current mark. **Scope v1 to intent edits** — the
  case broken today — and add versioned migrations when the first template
  change actually needs one.

#### How it changes the sequence

`g field` (§9.1) stays first — it is the primitive, and its "print, never
clobber" refusal remains correct until this lands. But **step 2 of §11 becomes
"regenerate + 3-way merge" instead of "output fingerprints"**, which is less
code, no new file format, and reuses `git merge-file` already on the machine.
The `edited_files` oracle (`src/add/database.rs`) stays for the capability
path, where there is no stored intent to re-run.


### 11.2 The path set: record what was written, do not recompute it

`ideas/openapi-generator` solves the other half, and its javadoc states the
purpose exactly (`DefaultGenerator.java:2000`):

> *"Generates a file at `.openapi-generator/FILES` to track the files created
> by the user's latest run. This is ideal for CI and regeneration of code
> without stale/unused files from older generations."*

The implementation (`:2005-2050`) is ~40 lines: take the list of files the run
produced, relativise each against the output dir, **normalise separators to
`/` so Windows and Linux agree**, sort case-sensitively, write one per line.
Alongside it goes `.openapi-generator/VERSION` — the generator version that
produced them.

**jails should write `.jails/files` and `.jails/version` the same way**, and it
is better than both designs this plan previously carried:

- **Better than recomputing paths from the generator** (§6.2 option B). Option
  B is still right for `--pretend`, where nothing has been written yet. But
  for `destroy` *after a jails upgrade*, recomputation gives you today's paths
  for yesterday's files — and silently strands anything whose path changed.
  A recorded list cannot drift, because it is not derived.
- **Better than output fingerprints.** It answers "what did this intent
  write?" directly, which is the question `destroy` and drift repair both ask.
- **It closes the stale-file case `examples/DOGFOOD.md` names** — *"does not
  yet notice a generated file deleted afterward"*. Regenerate, diff the new
  file list against the recorded one, and act on the difference: files no
  longer produced are stale, files missing from disk were deleted by hand.
- **`VERSION` is exactly the pin §11.1 says the upgrade case needs.**

The two halves compose into the whole drift story: **§11.2 gives you the path
set; §11.1 gives you the content merge.** Neither needs an ownership model.

Two details worth copying rather than rediscovering: sort and separator
normalisation are what make the file diffable and stable across machines, and
`FILES` deliberately excludes its own metadata entry so regeneration does not
churn it.

## 12. Tier 4 — reach: the codebase you did not create

In `ideas/minicom-public/spring`, **zero of ~30 commands work** — the gate is
`generate::find_project_root`, 11 lines looking for `pom.xml` and nothing
else, with ~30 call sites and three further copies of the rule.

Dropping a **one-line stub `pom.xml`** into a copy makes `routes`, `beans`,
`stats`, `notes`, `rename --dry-run`, `destroy --pretend`, `doctor` and
`g record` all work against Gradle sources. `inspect.rs` and `rename.rs`
contain **zero** occurrences of `pom`.

```rust
pub(crate) enum Build { Maven, Foreign(&'static str), Bare }
pub(crate) fn project_build(root: &Path) -> Build      // new; signature of find_project_root unchanged
```

Nearest wins. Then three guards: `pom::read` says which build tool it found;
eight Maven-inherent commands get `require_maven`; and **`doctor` reports the
real build tool** — not optional, because a confident wrong report is worse
than a refusal. **Frame it in README**: *jails never reads, writes, parses or
invokes `build.gradle`.* That is strictly less than Gradle support.

Caveats: the stub-pom trick **changes the Java jails emits**
(`repository_wiring` returns `PlainJdbc`, `jspecify_available` false), so
degraded mode must *say* which shape it chose; **`add` still will not work**
and should not be exempted; **multi-module Gradle** puts `build.gradle` in
`app/` with `settings.gradle` above.

**`jails adopt`** writes a `[layout]` table, not new machinery — verified:
`[layout] web = "controllers"` made `stats` report `Web 2` (was `Other 4`)
with no code change. Map subpackages onto `LAYERS_IN_ORDER` through a closed
synonym table; a directory matching nothing is **reported, not guessed**. It
must **never** write `[project] capabilities`.

---

## 13. Tier 5 — the capabilities still missing

`ci`, `docker`, the durable queue, the outbox, the safe fetcher, traversal and
outbound HTTP delivery have all shipped. What is left:

### 13.1 `add cors` — still the actual blocker

`grep -rni cors src/ templates/ README.md` returns **nothing**, and
`security_config_java.java` has `anyRequest().authenticated()` and never calls
`.cors(...)`. **A jails app plus `add security` cannot serve a browser
widget.** The naive fix is wrong in a way that bites later:
`applyPermitDefaultValues()` permits only GET, HEAD and POST and no
credentials — the classic "works until mark-as-read becomes a PUT". Name the
methods, put origins in a marked properties block, and **wire `.cors(...)`
into the generated chain in the same change.** Two doctor checks fall out:
`@EnableWebMvc` with the webmvc starter (switches off auto-configuration), and
`addMapping("/**")` with no `allowedOrigins`.

### 13.2 `add sse` — the four details both SSE designs get wrong

`-1L` (or `0L`), not `Long.MAX_VALUE`. **`onCompletion` alone suffices** for
removal — but it runs on a *container* thread concurrently with the
broadcaster, so the registry must be `ConcurrentHashMap<K, Set<SseEmitter>>`
with `newKeySet()`, which both documents miss.
**`spring.task.scheduling.pool.size` defaults to 1**, so a 15 s heartbeat
blocking on one dead client stalls every other scheduled job.
**`Last-Event-ID` is not implemented by Spring** — emitting `id()` without a
`@RequestHeader` replay path advertises resumability you do not have. One
Framework-7-only fact that makes "SSE + virtual threads" real: Framework 7
replaced `synchronized` with a `ReentrantLock` throughout
`ResponseBodyEmitter` to avoid pinning.

### 13.3 The rest, each waiting for an acceptance clause

| Slice | The silent failure it prevents | Proves |
|---|---|---|
| **`g idempotency`** (a receipt keyed by scope + key + canonical request hash) | A `@unique` column gives one-row-per-key but **not the retained result**; a retry gets a 409 instead of the original response. App C is the first app that needs it | C |
| **`add coverage`** (Jacoco + a stated threshold) | §5.4 | C |
| **`add loadtest`** (k6 from the route table) | §5.5 | C |
| **`add k8s`** (probes on the management port, burn-rate rules) | §5.6 — only correct after §5.2 | C |
| **`g auth`** | Boot 4 auto-configures **no `JwtEncoder`**, and **a JWT with no `exp` passes the default decoder** | B, C |
| **`g webhook`** (inbound; `http-sink` is outbound) | Signature over **raw bytes**, `MessageDigest.isEqual`, Stripe's 300 s tolerance | B |
| **`add mail`** | Boot 4's `-test` twin convention; the IT reads mail back over POP3 as Boot's own test does | B |
| **`g search`** | A `generated always as (…) stored` `tsvector` — *generated*, because a trigger someone forgets on UPDATE is the silent failure | B |
| **`add flags`, `add shedlock`, `add storage`, `add arch`, `add nullcheck`** | Build the first time a project needs one. `add shedlock`: two instances fire the 02:00 job, customers get two emails, nothing logs an error | — |

### 13.4 The UI decision

An agent inbox needs HTML; a take-home that renders nothing is half a
submission. `g page` with **JTE** + htmx + `add sse`: JTE templates are
compiled, type-checked Java, so a renamed record component breaks the build
rather than the request — jails' bar — and its dev mode runs its own watcher,
so edits are sub-second with no dependence on devtools LiveReload (deprecated
in Boot 4.1 with no replacement).

---

## 14. Tier 6 — the editor

**Projectionist, not a Lua reimplementation.** `tpope/vim-projectionist`
accepts projections **in memory** via `projectionist#append(root, …)` —
nothing written into the repo, which dissolves the objection. On detect, run
`jails about --json` once per root and build the table from `layout` +
`base_package`. **This matters more now: 11 layers means a generated slice
crosses more directories than it used to.**

**`about --json` v2** is the prerequisite: add `layout` (through
`Config::layers()`, i.e. *renamed* values), `base_package`, `capabilities`,
`java_root`/`test_root`, pinned to `LAYERS_IN_ORDER` by a test. Normalise the
version key (`about` uses `schema_version`, `routes`/`beans` use `version`).
**Add `line` to `Route`/`Bean`** — today `line` exists on `Note` and nothing
else, so `routes --json` is a list; with a line it is a quickfix and a picker.

**`gf` into JDK and project source — six lines, no plugin.** Neovim's
`ftplugin/java.vim` already sets `includeexpr`, `suffixesadd`, `include`,
`define` and honours `g:ftplugin_java_source_path`; only `'path'` is missing.

**jdt.ls settings and bundles.** Add `updateBuildConfiguration = 'automatic'`
(the default `'interactive'` is why **every `jails add` leaves red squiggles
until prompted**), `autobuild.enabled`, `downloadSources`, `-Xmx2G`. Then
**java-debug** and **vscode-java-test**, which give
`jdtls.dap.test_nearest_method()` — one test, no Maven, no Surefire — and hot
code replace with frame popping. Pair with `jails run --debug` and
`spring.devtools.restart.enabled=false` for that run.

**`:compiler jails`** — `$VIMRUNTIME/compiler/maven.vim` already carries javac
errors, non-parseable POM and the Surefire multi-line pattern. Copy the
`errorformat` verbatim (the `current_compiler` guard makes `runtime!` bite).

**Pickers** via `fzf-lua` (this config has no telescope) over `routes --json`
/ `beans --json` — sub-50 ms on a project that does not compile, which jdt.ls
cannot do. **`jails src <Type>`** resolves a project type, else a type under
`deps/`.

**Keymap collisions**: `<leader>j{t,c,r,b,g}` vs `<leader>J{t,c,r,b,g}` — a
shift slip turns "extract constant" into `mvn clean verify`. Split
semantically. **`javac_lint`** recompiles the whole tree on every save, runs
bare `javac` with **no `--release`**, and re-runs `dependency:build-classpath`
on every pom change; fix all three and keep its output out of
`target/classes`. **Two bugs**: `setqflist({}, 'r', …)` **replaces** the list
jdtls just built (should be `' '`), and `vim.fn.termopen` is deprecated.

---

## 15. Tier 7 — the agent as second user

### 15.1 `AGENTS.md`, with evidence

§5.8: a 166-line `AGENTS.md` is the highest-signal file in a 332K-line
repository. **`jails new` should write one**, and its banned-API list must be
*rendered from* the same table `jails lint` matches against, so it cannot
drift into a lie — a hand-written one is a `validation/README.md` waiting to
happen. Content: use `jails test <Name>`, not `mvn test`; `jails check` is the
gate and *why*; `jails doctor` before debugging the environment; records, no
Lombok, no ORM; the layer table; the field-spec grammar.

### 15.2 The rest

**`jails lint`** — a closed rule table over the stale-API families jails
already knows (`@MockBean`, `javax.validation`, Jackson 2 alongside 3,
`spring-boot-starter-web`, `@Entity`, Lombok, preview features), plus **`double`
in money code** from §4.3. Sub-second, exit 1, `file:line`.

**`--json` everywhere.** Three commands have it. `doctor --json`,
`why --json`, `test --json`, `stats`, `notes` are an afternoon each.
**`why --json` is highest value** — it makes the explanation available as
quickfix text, and `why.rs` already stores exactly `{signature, explanation,
fix}`. **`jails commands --json`** then *deletes* the Lua lists rather than
pinning them (§6.2).

**`jails explain <kind>`** exposes the rationale the Javadoc carries, so an
agent stops "fixing" `@Repository` onto the second adapter. **Promote
`g cases`** — it turns a markdown brief's acceptance bullets into a test
class, and 8.7 says nobody can find it.

**No MCP server** — worse than the CLI an agent already shells to. **No LLM
inside jails** — deterministic generation is the product.

---

## 16. Anti-goals

| Temptation | Why not |
|---|---|
| Plugin **lifecycle hooks**, arbitrary shell, downloadable packs, a **generator DSL with conditionals**, codegen from an external schema language (note §6.6 Tier 3 is *not* this) | §6.3. `.jails/app.toml`'s closed schema and §6.2's descriptors are data, not logic |
| Gradle *support* | Distinct from Gradle-directory *tolerance*, which is §12 |
| ORM, lazy loading, an Active Record clone | `g association` is a constraint, not a mapping layer |
| A `money` field type, a `payment` kind, a `spider` kind | §4.6 |
| A `jails-support` runtime jar | ActiveSupport lock-in; capabilities write classes *into* the project |
| Lombok; preview features; `StructuredTaskScope` | Editor tax; still preview |
| **`add crawl`, jsoup, crawler-commons** | Superseded — `http-workflow` does it with zero dependencies |
| Wrapping crawler4j / webmagic / Nutch / StormCrawler | Dead, platforms, or a second runtime |
| A Rust `jails lsp` | ~1.2–1.8k lines, a JSON parser, a second server fighting jdt.ls, parser pressure on `java.rs` |
| The AOT cache in `dev`/`test`; CRaC; JBR as a default | §7 |
| Maven 4, the build cache extension, `useIncrementalCompilation=false` | Not GA / restores a deleted `target/` / stale-dependent `NoSuchMethodError` |
| Making `jails check` incremental | The leftover-`.class` bug is real |
| Forcing virtual threads on | §5.7 — a production system chose the opposite |
| Treating a skipped test as coverage | `JAILS_REQUIRE_TOOLCHAIN=1` exists for this |

---

## 17. The sequence

**Proves**: **A** crawler, **B** inbox, **C** payments gateway, **D** ledger
CLI, **—** infrastructure.

| # | Item | § | Effort | Proves |
|---|---|---|---|---|
| ~~0~~ | ~~**The golden-coverage test and the generate/destroy agreement test**~~ **DONE.** Seven scenarios added (308 files / 32 scenarios); `tests/agreement.rs` found and fixed a live stranding bug in `usecase --yields` on its first run | §8.5, §6.2 | — | A B C D |
| 0b | **§6.2 B + D** — artifact builder; ~~delete `destroy`'s 17 hand-written path arms~~ **HALF DONE.** The arms are gone: `KIND_FILES` is one table of (tree, layer, placement, filename) and `NO_FILE_TABLE` holds the four kinds that genuinely have no path list, each with its reason. `every_kind_is_either_in_the_file_table_or_deliberately_outside_it` reads the enum, so a forgotten kind fails a test instead of printing "nothing to destroy" over files that are right there. **Still copy 2**, though: a table is a shorter transcription, not a derivation. Deriving it needs D — lazily-rendered artifacts, so a path can be computed without a body and without the `--on`/`--yields`/fields that `destroy` is never given | §6.2 | 1 day left | A B C D |
| ~~1~~ | ~~**Stand up App D and App C**~~ **DONE 2026-08-22.** Both manifests live, both gates green (C: 83 tests, 1 m 45 s; D: 53 tests, 1.9 s), zero hand-written Java or SQL. Six generic fixes came out of it; friction and defect rows are in `examples/DOGFOOD.md` | §4.3–4.4, §18 | — | C D |
| ~~2~~ | ~~Tier 0 remainder: 8.1–8.4, 8.7–8.9~~ **DONE.** All closed except 8.6 (`spring.rs` size, which is §6's ongoing work). Each closure carries the test that keeps it closed: the pom matrix, the editor pinning test, the watcher's three unit tests, `Class#method` routing | §8 | — | A B C **D** |
| 3 | Editor config: jdt.ls settings + bundles + HCR, `'path'`, `:compiler jails`, keymap split | §14 | S, no Rust | — |
| ~~4~~ | ~~Testcontainers reuse~~ **corrected — see §7: unsafe as a default, and the gate proved it.** `jails setup` + the `doctor` reuse/leak count + the devtools defaults from `new` are **done**; `mise.toml` from `new` is not | §10.1 | — | A B C |
| 5 | `jails test` flags: `Class#method`, `path:line`, `--failed`, `--fail-fast`, `--slowest` | §10.1 | S–M | — |
| 6 | `why` on every Maven failure; `why --json` | §10.1, §15 | S | A B C D |
| 7 | **§5.2 observability defaults** + the three doctor checks | §5.2 | M | C |
| 8 | **§5.3 datasource defaults** incl. the `pg_is_in_recovery` init SQL | §5.3 | M | C |
| 9 | ~~**The inflector**~~ **DONE** (§9.3); **scaffold reads the record**, **refusal messages with `fix:`** still open | §9.3, §9.4, §9.6 | S each | A B C |
| 10 | **`g field`** | §9.1 | M | A B C |
| 11 | **`.jails/files` + `.jails/version`** (§11.2), then **regenerate + 3-way merge** (§11.1) | §11.1–11.2 | M | A B C |
| 12 | `--timestamps`, `g factory`, `requests/*.http` | §9.5, §9.6 | M total | A B C D |
| 13 | `g idempotency` | §13.3 | M | C |
| 14 | Scaffold **refuses** an unmapped project-typed component | §9.2 | S | B C |
| 15 | §5.4 enforcer rules in `new`; `add coverage`; `add loadtest` | §5.4–5.5 | M | C |
| 16 | `about --json` v2 + line numbers; projectionist; pickers; `jails src` | §14 | M | — |
| 17 | **§6.2 C + §6.5** — templates out of `spring.rs`, split the file; **§6.2 E** — type table as data | §6 | M, ongoing | — |
| 18 | `jails test --fast` + `jails bench` | §10.2 | M | D first |
| 19 | `add cors` | §13.1 | S | B C |
| 19b | **§6.6 Tier 2** — template overrides (`.jails/templates/`) + `doctor` reports active overrides | §6.6 | S | — |
| 20 | `new --offline` + `app init` | §11 | S–M | A B C D |
| 21 | **§6.2 F** — one descriptor per kind; delete the Lua lists; `[golden]` becomes a required key | §6 | L | — |
| 22 | §12 marker widening + `jails adopt` | §12 | M | — |
| 23 | `jails testd` + `--affected` | §10.2 | L | — |
| 24 | `jails dev` v1 | §10.3 | L | — |
| 25 | `add sse`; `g auth`, `g webhook`, `add mail`, `g search`; `add k8s` | §13 | M each | B C |
| 26 | `AGENTS.md` + `jails lint` + `--json` everywhere | §15 | M | — |
| 27 | Atomic whole-manifest `ChangeSet`; `codemod.rs` | §11 | L | A B C |

Items 0–2 close a verification hole that is currently growing once per kind.
Items 7–8 are what make "batteries included" true rather than a slogan. Items
9–14 are the authorship debt, paid on **every model change**. Item 23 is the
biggest latency number and is correctly late.

**The stopping rule:** when a proof app's acceptance clause is closed, stop
working on that capability. `ACCEPTANCE.md` says the gate may report
`generated`, `configured`, `user-owned` or `not selected` and **must never
call an unproved property guaranteed or production ready.**

---

## 18. Runbook — the exact commands

Nothing in this section has been run in the session that wrote it. **Record
every deviation in `examples/DOGFOOD.md`'s friction ledger as you go; that
ledger is the output, not a by-product.**

```bash
cd ~/code/jails
cargo build && cargo test && cargo install --path .
export JAILS_BIN="$PWD/target/debug/jails"
```

**App C — payments gateway**

```bash
cd /tmp && "$JAILS_BIN" new payments-gateway --deps web,validation
mkdir -p payments-gateway/.jails
cp ~/code/jails/examples/payments-gateway/.jails/app.toml payments-gateway/.jails/app.toml
cd payments-gateway
"$JAILS_BIN" app plan                 # must write nothing; read the intent list
"$JAILS_BIN" app apply --no-start
"$JAILS_BIN" routes && "$JAILS_BIN" beans && "$JAILS_BIN" stats
"$JAILS_BIN" doctor
"$JAILS_BIN" migrate --check
"$JAILS_BIN" check                    # mvn clean verify
git -C . diff --stat                  # MUST be empty of hand edits
```

**App D — ledger CLI (no Spring)**

```bash
cd /tmp && "$JAILS_BIN" new-cli ledger-cli
mkdir -p ledger-cli/.jails
cp ~/code/jails/examples/ledger-cli/.jails/app.toml ledger-cli/.jails/app.toml
cd ledger-cli
"$JAILS_BIN" app plan
"$JAILS_BIN" app apply --no-start     # EXPECT §8.1 to fail here — that is the point
mvn -o validate                       # the assertion that 8.1 is real
"$JAILS_BIN" check
```

**A and B — regression, unchanged**

```bash
cargo test app_manifests_compile_without_manual_source_edits -- --nocapture
cargo test app_manifests_pass_the_full_generated_verification_gate -- --nocapture
```

**What to record for each app** (§4.5): manifest lines, generated lines,
**hand-written lines (must be 0)**, manual interventions, command count, wall
time. Then add one row per friction item to `DOGFOOD.md`, in its existing
shape: *Application | Step | Manual intervention or weak output | Generic
jails improvement.*

**Then ask the question this whole exercise exists to answer:** which two
commands in the runbook above should have been one? Today the answer is
already visible — `new` + `mkdir` + `cp` + `app apply` is four steps that
should be `jails new <name> --app <manifest>`. That is item 20.

---

## 19. Measure before promising

1. Console-launcher wall time here (est. 0.35–0.6 s) and the resident-JVM band
   (est. 50–150 ms).
2. The cost of a fresh `URLClassLoader` per `testd` run.
3. How many distinct Spring contexts the proof apps build (`missCount` under
   `org.springframework.test.context.cache=DEBUG`). At 293 s and 123 tests
   this is the highest-value measurement in the list.
4. `postgres:17` with reuse under podman, and whether `withReuse(true)`
   disturbs `@ServiceConnection`.
5. Where jdt.ls writes `.class` files here — **§10.3's "the loop already
   exists" finding pivots on it.**
6. p99 for App C under the §5.5 k6 profile, before any performance claim.
7. Whether `CSVFormat.Builder.build()` still exists at commons-csv 1.14.1.

### The JDK decision, already made

`TARGET_RELEASE` is `"25"` and the golden poms pin release 25. Four
consequences to collect: **the tier-3 skips should be gone** (run
`JAILS_REQUIRE_TOOLCHAIN=1 cargo test` and confirm); **`doctor`'s daily false
FAIL should be gone**; **`add docker` no longer needs `jlink`** because
`eclipse-temurin:25-jre` exists, which is presumably why both images now
build; and **the JetBrains Runtime path is reachable** (§7).

Note the payments gateway targets **Java 26**, not 25. That is a data point,
not a reason to move: 25 is LTS and everything in this plan is available at
25. **What still needs recording is the reason, next to the pin, in
CLAUDE.md**, which documents the 27 rationale and is now wrong (8.7).

---

## 20. Provenance

- **`ideas-opus.md`** — loop-latency framing, `jails dev`, sub-second tests.
  Its two headline mechanisms (AOT in the dev loop, enhanced redefinition) are
  dead; see §7.
- **`ideas-grok.md`** — vim-rails projections, `jails src`, `g field` + alter
  migrations, the Lua pinning test. Its webhook algorithm is corrected; its
  `add html` + `g spider` design is superseded by `http-workflow`.
- **`ideas-kimi.md`** — the synthesis discipline and K1–K21. `add queue` was
  its best addition and has shipped as `durable-job`; `g load` is superseded
  by §5.5.
- **`ideas-sol.md`** — `ChangeSet`/provenance, the CLI schema protocol, the
  production contract, the genericity gate (§4.6), the capability lifecycle
  trait that §6.2 option F is the data-driven form of. Its "infer
  aggressively, guess conservatively" is now implemented in `usecase`. Its
  sequencing is overruled once (§11).
- **`ideas-fable.md`** — twelve research passes with `file:line` citations.
  §7's table is largely its work, as are the jdt.ls/HCR path, the `--affected`
  index, `spring-devtools.properties`, the JWT `exp` finding, the SSE
  `ReentrantLock` finding and projectionist. It predicted §8.5 before it
  happened.
- **`ideas-opus2.md`** — the only document that ran the tool. Every number in
  §2 and the authorship budget come from it.
- **`examples/`** — `ACCEPTANCE.md`, `DOGFOOD.md`, the manifests. The harness,
  and §4 makes it the plan's driver.
- **`/home/laith/code/projects/payments-gateway-service`** — read for this
  rewrite: `AGENTS.md`, the root POM's plugin set,
  `payments-gateway-service-web/src/main/resources/application.yml` (the
  `management:`, `spring.datasource:` and `server:` blocks), `k8s/chart`,
  `load-tests/`. §5 is entirely from it, restated in generic form.

**Verified against `e523c16`** (still live unless noted): 8.1 (versionless
validation dep), 8.2 (golden pom), 8.3 (`run.rs` mangle-then-route), 8.4
(`run_watched`'s single non-watch caller; `.java`-only scan), 8.5 (`grep -c`
→ 0 for twelve kinds and capabilities; 162 files / 25 scenarios unchanged
across eight new kinds), 8.6 (6,459 lines, ~42 `r#"package {pkg};` blocks),
8.7 (`grep -c cases README.md` → 1; `toxiproxy`/`app` absent from the Lua;
CLAUDE.md's 8-layer description against 11), 9.1 (no `ArtifactKind::Field`),
9.2 ("Not persisted" in `generate/repository.rs`), 9.3 (`sql.rs` naive
`+ "s"`), 9.4 (`scaffold` does not read the record), 9.5 (no `--timestamps`),
13.1 (**zero** `cors` matches), 10.1 (**zero** `withReuse` matches), §12
(`find_project_root`, `pom.xml` only).

**Not load-bearing**: §2.1's latency figures (measured in earlier sessions);
the 293 s and "both images built" claims, from `ACCEPTANCE.md` rather than a
run here; **every manifest in §4.3 and §4.4, which has not been executed** —
§18 is how they become evidence; the upstream `deps/` line numbers, which
drift; and everything in §19.
