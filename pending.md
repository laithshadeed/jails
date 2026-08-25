# What is pending

**This file is only what is not done.** What the code already *is* belongs in
`CLAUDE.md`; what it *does* belongs in `README.md`; why a closed decision went
the way it did belongs in git.

It replaces six files. `plan.md`, `abstract.md` and `playground.md` were deleted
earlier; `missing.md`, `refactor.md` and `test.md` were folded in here on
**2026-08-25**. Five of the six are in git history:

```sh
git log --diff-filter=D -- missing.md   # finds the commit that removed it
git show <commit>^:missing.md           # prints it
```

**`refactor.md` is the exception and the version folded in here is gone.** An
older one is recoverable — it was tracked until `8803a60 Remove superseded audit
and refactor notes` — but the copy that was on disk on 2026-08-25 had been
regenerated since -- and `.gitignore` deliberately ignores `/refactor.md` to
stop that regeneration being committed again -- so `git show` reaches a
different document. Everything in it that was still true is in §4 through §10 below, item
by item, with each number re-measured rather than transcribed; what is lost is
its prose. If a claim below looks thin, that is why, and the fix is to
re-measure rather than to hunt for the file.

Roughly 154 comments still cite `plan.md §N` and 48 cite `abstract.md §N`; six
cite `missing.md` or `refactor.md`. Those citations are the best record of
*why*, and §10.2 is about the ones that are load-bearing rather than historical.

**Every number in this file was measured on 2026-08-25** against `main`, and
each item says how. A claim with no measurement is an opinion and is labelled
one. Numbers inherited from the merged files were **re-measured, not
transcribed** — several had gone stale, and the stale half is recorded in
"Already closed" at the end so nobody re-derives it.

---

## 1. Open defects in what jails generates

Highest priority: these are wrong output shipping today.

### 1.1 A `@unique` violation answers 500, not 409

Create a resource, then create another with the same value in its unique
column. jails put that constraint in the schema and `add api` generates an
`ApiException.Conflict` documented "Becomes a 409" — nothing connects the two,
so a duplicate reads as the server breaking. 5xx is what alerting pages on and
what clients retry, so a duplicate becomes an incident and then a retry storm.

Measured: `grep -rn DuplicateKeyException crates/ templates/` returns **zero**.

It is not a one-line handler. `DuplicateKeyException` arrives with the JDBC
stack; `ApiExceptionHandler` is written by `add api`, which does not require a
database. An unconditional arm hands an `api`-without-`db` project a compile
error for a file it did not write. The fix needs a conditional arm plus a pass
that revisits `api` after `db` lands — `app apply` reconciles the whole manifest
as one transition and would catch it, `jails add api` then `jails add db` would
not. **Decide that ordering contract before writing the handler.** The generated
controller test is where the assertion goes.

### 1.2 Seven generated tests have a Boot floor

`jails new --gradle --boot 2.7.18` made pre-Boot-3.4 projects reachable for the
first time, and that surfaced an assumption nothing had ever tested: nine of
jails' companion tests are written against `MockMvcTester`
(`org.springframework.test.web.servlet.assertj`), which is Spring Framework 6.2.

Two now carry a classic `MockMvc` variant and pick it from the project's Boot
major — `g controller` and `add cors`, the two `examples/minicom-spring/` needs.
The other seven **refuse** rather than write a test that cannot compile:
`add api`, `add security`, `g scaffold`, `g usecase`, `g query`, `g transition`.

That is the right failure and the wrong feature. The fix is a classic variant
per template, and the reason it has not been done is that a template written and
not exercised is a template nobody has proved compiles. Each wants its own
real-toolchain run against a Boot 2 project, which `examples/minicom-spring/`
now makes cheap.

**The general lesson outlived the fix.** Three sibling bugs were found in the
same afternoon, all one shape — a version fact answered confidently and wrongly:

- `Project::boot_major` read `build.gradle` through the *POM* parser and
  answered 3, so every Boot-4-only artifact and property name jails picked off
  it was picked correctly by accident.
- `Project::projected` had the same drift and answered `PlainMaven` for a Gradle
  project, which is why `jails app apply` refused every Spring capability on a
  build `jails about` called Spring Boot.
- `add h2` declared `spring-boot-h2console` (a Boot 4 module) and wrote
  `spring.persistence.exceptiontranslation.enabled` (renamed at 4.0.0, so the
  older spelling is silently unbound rather than rejected).

`read_build_file`'s doc comment records the first two instances of this. Assume
there is a fourth.

### 1.3 Two lists of the same types — **closed 2026-08-25**

The JSON sample table and the field-type vocabulary were two spellings of one
set, and they were apart — which is how a `uri` component came to document a
request its own record refuses.

Re-measured before fixing, and it was worse than one gap: there were **four**
copies. `jails_spec::spec::field_type`'s match; the list of accepted spellings
typed out again inside its own error message; `scaffold::json_sample`; and
`spring::workflow::json_sample`. A fifth, `sql::sample_value`, is a different
question (two *distinct* rows for a fixture) and stays.

The real holes: `path` had no sample in `scaffold`, and `currency` and `bytes`
had none in `spring::workflow` — so a scaffold or a use-case over one of those
emitted a request body with the field silently absent.

`jails_spec::spec::BUILTIN_FIELD_TYPES` is the vocabulary now, one row per
accepted spelling, and both the resolver and its error message read it.
`builtin_java_types()` is the distinct set anything mapping a field to something
else has to cover, and `every_builtin_type_has_a_json_sample` fails when a
sample table falls behind it — checked by breaking it, which reports
`scaffold: Path`. That is the relationship the tables should always have had:
one of them is the list, the others answer to it.

### 1.4 Generated business behaviour is unwritten, by design

The ledger match rules and the Kafka listeners in every generated application
contain the application-specific reaction nobody has written, so the ledger does
not reconcile and a received event drives nothing. That is the honest boundary
of a scaffolding tool. **The open question is whether the declarative manifest
can be extended far enough to generate those decisions, or whether they are
properly the reader's code.** Opinion, not measurement — and §2.2 and §2.3 are
the two experiments most likely to settle it, because a ranking rule set and
four framework ports of one domain are both cases where the answer is
falsifiable rather than arguable.

---

## 2. The portfolio: what jails has to be able to build

The `examples/` applications are not demos. They are the acceptance criterion —
the only evidence that the generic machinery is generic, because a crawler, a
support inbox and a payments gateway are three lists of the same intents and
none of them gets a command, branch, enum or template in core. Every gap in §1
was found by building one.

Where the portfolio stands today:

| application | what it clones | manifest | proved by the suite |
|---|---|---|---|
| payments gateway | a payments gateway | `examples/payments-gateway/` | yes — `SPRING_APP_MANIFESTS` |
| support inbox | Intercom | `examples/support-inbox/` | yes |
| web crawler | Google | `examples/web-crawler/` | yes |
| ledger CLI | stacks.ai | `examples/ledger-cli/` | yes — `ledger_cli_manifest_builds_without_spring`, the one non-Spring proof |
| minicom | Intercom, ported from the Rails and Django originals | `examples/minicom/` | **no** |
| minicom-spring | the Gradle interview scaffold | `examples/minicom-spring/` | **no** — verified by hand on 2026-08-25, nothing holds it |
| Gradient Lattes | `gradient.md` | — | not started |
| Throxy persona ranker | `throxy/` | — | not started |

`gradient.md` and `throxy/` are **local-only inputs and are gitignored**, so a
clone of this public repository will not have them. Three reasons, the first
sufficient on its own: they are other companies' take-home material and not
ours to publish; `throxy/` is its own upstream repo, which is the gitlink
accident `/deps/` is in `.gitignore` to prevent; and `throxy/data/leads.csv`
carries real people's names, job titles and employer domains. **The generated
proof application is jails' own output and is committable — the brief it was
built from is not.** Anything landing in `examples/` has to stand on its own
without quoting either.

Two things fall out of that table before any new work. **Two Intercom-shaped
manifests exist and only one is proved** — `support-inbox` is in
`SPRING_APP_MANIFESTS` and `examples/minicom/` is not, so the second can drift
against a generator change with nothing failing. And `examples/minicom-spring/`
is the same shape: it is the proof that `jails new --gradle` works and it is
held by nothing.

### 2.1 Gradient Lattes — `gradient.md`

Spring Boot 4.1, Java 26. An ordering API for autonomous baristas over two bean
suppliers: a cheap roastery with limited stock and an expensive chain with
plenty. Part 1 hides the supplier choice from the caller behind a 30-second
deadline; part 2 makes two stores share a supply that runs out at lunch.

**The suppliers are part of the solution, not a hosted service.** The brief was
rewritten so nothing reaches the public internet: no external URLs, no
credentials, no `Authorization` header. jails writes the supplier service too,
and its funky behaviour is the thing being reproduced — 429 with a `Retry-After`
set from *when stock actually replenishes*, 200 with `{"success": "true"}` for
most orders, and 200 with a garbage body for ~5% of them **with the beans still
consumed**, which is what makes the rotten case expensive.

What it will exercise, and where it is likely to find gaps:

- **One client, two configurations.** `g client` writes one `@HttpExchange`
  interface; the roastery and the chain differ in stock and price, not protocol,
  so they are one port constructed twice. That interface is also the seam a test
  substitutes a fake at, so no test needs a socket — which is §9's rule about
  developer services, arriving from the application side for once.
- **Retry that reads the signal.** Honouring `Retry-After` rather than backing
  off on a constant. jails has no retry capability; `resilience4j` is in
  `deps.tsv` and nothing generates against it. This is the first real candidate.
- **A deadline, not a timeout.** "Apologise and offer instant coffee" at 30
  seconds is a budget spanning several supplier calls, which is a different
  thing from a per-call timeout and jails expresses neither.
- **Fair share between two stores.** A quota or allocator over a contended
  resource. `g idempotency` is the nearest primitive and is not it.
- **Seedable randomness and configurable stock**, so a 429, a rotten delivery
  and part 2's both-suppliers-empty case are forced on demand rather than waited
  for. `add testkit` gives deterministic clocks and ids; a seeded generator is
  the missing half.

### 2.2 Throxy persona ranker — `throxy/`

Spring Boot 4.1, Java 26. A Next.js scaffold (`src/app/api/rank/route.ts`) that
loads ~200 leads from `data/leads.csv`, ranks them against
`data/persona-spec.md`, and returns the best relevant contacts per company.
`GET /api/leads` lists, `POST /api/rank` ranks. Relevance filtering is part of
the ranking: an HR contact at a target company is a lead you should *not* email
about a sales platform.

**Two jobs, and the second is the interesting one.** Re-implement it in Spring
Boot — and do the whole homework **without any external service**, which means
without the Vercel AI SDK and without an OpenAI or Anthropic key. The original
brief expects an LLM to do the ranking; doing it locally forces the scoring to
be explicit, deterministic and testable, which is the only version jails can
generate and the only version a test can assert on.

Note the shape this shares with 2.1: **an interview brief pointing at an
external service, re-done with that service replaced by something local.** That
is not a coincidence, it is what makes both of them admissible as proof
applications at all — §9's success criteria forbid a test that needs a developer
service, and a proof app that cannot be proved is a demo.

Likely exercise: `add csv` for the lead load, `g record`/`g value` for the lead
and the persona spec, `g scaffold` or `g query` for the two routes, and a
scoring strategy — `g strategy` is the open-set primitive, one bean per rule,
which is exactly the shape "disqualification criteria plus weighted signals"
wants. If the persona spec's rules can be expressed as a manifest, that is
evidence for §1.4's open question; if they cannot, that is evidence against it,
and either answer is worth having.

### 2.3 All of minicom, with jails only

`minicom/minicom-public/` is a whole prototype Intercom: a Rails server, a
Django server, a Node server, a Spring server, and two static sites (`foo` on
`127.0.0.1:8008`, `bar` on `8009`) that talk to them. `examples/minicom/`
already ports the *domain* — users, messages, a read flag, a direction enum —
and `examples/minicom-spring/` reproduces the Gradle scaffold. Neither is the
whole thing.

The target is the rest: every server in that repository re-expressed as jails
manifests, and nothing hand-written. It is the largest of the three and the one
that most directly tests the claim in §1.4 — four framework ports of one domain
is the strongest available evidence about where the generic manifest stops.

Start by proving what already exists: put `examples/minicom/` into
`SPRING_APP_MANIFESTS` and `examples/minicom-spring/` behind a Gradle equivalent
of it, so the two manifests that exist stop drifting silently. That is a small
change and it is the prerequisite for the rest.

### 2.4 The cost, which has to be decided before the first one lands

**`SPRING_APP_MANIFESTS` currently holds three applications and they dominate
the suite.** §9 measures the tail: three concurrent Failsafe runs against the
shared PostgreSQL and Kafka, starting at ~21.5 s and alone determining when the
CLI binary ends. Adding three or four more proof applications to that list
multiplies the thing that is already the bottleneck, on a suite that is
59.60 s today and has a stated target of 30.

So decide the relationship first. The options, none of them free:

- **All of them in `SPRING_APP_MANIFESTS`.** Honest and slow. Only viable after
  §9's Failsafe tail is shortened, which makes this blocked on that work rather
  than merely expensive.
- **A tier.** Proof applications that run on every `cargo test`, and a larger
  set behind an env var that CI runs and a laptop does not. The risk is the one
  §9's success criteria name: a test that does not run by default is a test
  nobody notices breaking, and this repository already has the
  `JAILS_REQUIRE_TOOLCHAIN=1` precedent for turning a silent skip into a
  failure — the same trick would have to apply here.
- **Generate-and-typecheck by default, full Maven gate on a subset.** Cheapest,
  and it gives up exactly the property the proof applications exist for: that
  the generated project *runs*.

**Do not add the first new application before choosing.** Three of them arriving
one at a time, each adding ten seconds, is how the suite gets to two minutes
with nobody having decided that it should.

---

## 3. Gradle and Maven parity

**Maven stays the default.** `jails new` with no `--gradle` creates a Maven
project and should go on doing so.

Landed: `gradle.rs` reads and splices a Groovy `build.gradle`; `Build::Gradle`
means "jails can read this"; `add`, `generate`, `doctor`, `about`, `build`,
`clean`, `check`, `test`, `run`, `watch`, `add format`, `add coverage`, the
Failsafe/`integrationTest` claim, `jails gradle` and the report-reading test
flags all work. **`jails new --gradle` is done** — `examples/minicom-spring/` is
the manifest, verified end to end against real Gradle 8.5 and JDK 21.
`build.gradle.kts` and a root holding only `settings.gradle` stay `Foreign` on
purpose.

Still Maven-only:

| what | why it is not portable yet |
|---|---|
| `jails fmt` | The *transactional* half. `route::format` runs the formatter in a sandbox laid out from the projection, so the reformat is a reviewed diff committed in the same transaction — and it drives that with Maven. Gradle in a throwaway tree needs its wrapper, its caches and a writable `build/`, which is a different bargain. It refuses by name and points at `./gradlew spotlessApply`, which the project is already configured for |
| `testd`, `test --fast`, `test --affected`, `jails console` | All need a *resolved classpath*, which jails gets from `dependency:build-classpath`. Gradle has no equivalent without adding a task to the build — and adding one to a file the reader owns, for a convenience, is a different bargain from splicing a dependency they asked for |

**The naming debt, deliberately taken.** `Change.plugins` is still
`(artifact_id, xml_block)` — a Maven plugin with Maven's syntax baked in — and
`ResourceKey::MavenPlugin` keys the claim by a coordinate Gradle does not
resolve. `gradle::feature_of` maps the coordinate onto what the plugin *does*,
which is total for the closed set jails emits and `None` for anything else, so
the behaviour is right and only the name is wrong. Renaming the key to the
feature is a protocol change across five files; it buys no behaviour and can be
done whenever the churn is convenient.

What makes the debt safe: a plugin with no known Gradle equivalent **refuses the
whole capability**, so nothing is half-installed on the strength of a name jails
half-recognised.

---

## 4. One gate reads green over unfinished work — **closed 2026-08-25**

`tests/architecture.rs` used to read:

```
  name:    "filesystem mutation sites outside the write layer"
  rung:    "R6.4 — every mutation through the executor"
  now: 0   ceiling: 0   target: 0   status: done
```

`mutation_sites` counted raw `std::fs::*` mutating calls outside `apply/`,
`store.rs`, `journal.rs`, `execute.rs`, `scratch` and `sandbox` — and the
`apply` module had already driven that to zero. It did **not** count
`apply::put`, `apply::create`, `apply::remove` or `apply::move_file`, so the row
read `done` on the rung "every mutation through the executor" while the
migration that rung names had not happened.

Fixed by fixing the measurement, not the ceiling. The row is now **"mutations
that bypass the executor"** and counts `mutation_sites + executor_bypasses`,
where `executor_bypasses` is every `apply::` occurrence in production outside
the write layer. `no_bare_apply_verb_imports` holds the counting honest: an
`use jails_support::apply::put;` would let a call be spelled bare `put(` and
step around the gate, so importing `apply`'s verbs by name is a test failure.

**Measured 56, not the 69 this file previously recorded** — that number came
from a raw `grep` that also counted `use` lines and `#[cfg(test)]` bodies, which
the gate blanks. It fell to **54** the same afternoon when `rename.rs` was
deleted (§7.1). Where the remaining 54 live:

```
  21  src/new.rs                                  §5
  12  src/new/gradle_project.rs                   §5
   4  crates/jails-generate/src/generate/write.rs §7.7
   3  crates/jails-generate/src/spring/durable.rs §7.7
   2  crates/jails-project/src/compose.rs
   2  crates/jails-project/src/config.rs
   2  crates/jails-drive/src/testd.rs
   2  src/new/publish.rs                          §5 — keep, see there
   1  each: add/database.rs, generate/scaffold.rs, merge.rs,
        console.rs, doctor.rs, run.rs
```

`src/new.rs` and `src/new/gradle_project.rs` are 33 of the 54, which is what
makes §5 the largest single move rather than a tidy-up.

## 5. `jails new` was not a second transaction protocol — **re-measured 2026-08-25**

`src/new/publish.rs` implements publication-by-rename: write everything into a
sibling scratch directory, `rename` once, so the destination is absent or
complete. Its doc comment justifies this correctly — `jails new` has no project
to lock and no ledger to journal, because there is nothing there yet.

**The rest of this item was wrong, and re-measuring it was the work.** It said
`jails new --app <manifest>` then ran a whole `app apply` inside the scratch
tree *"through a mechanism with no journal, no recovery and no conflict
detection — the three things `jails-commit` exists to provide."* It does not:
`app::apply_in` builds `jails_engine::route::Run::committing`, which is the
ordinary V2 transition. A `jails new-cli demo --app app.toml` leaves a `.jails/`
holding `ledger.toml`, `objects/`, `receipts/` and `transactions/`. Checked by
running it.

So the proposed fix — publish the skeleton, then apply the manifest against the
now-real project — would have made things *worse* on the axis
`publish.rs` exists for: it would allow a destination to exist holding a project
whose manifest half-applied, which is the state publication-by-rename removes.

**What was real is the 33 `apply::` calls, and they were a measurement problem.**
§4's gate counted them as mutations bypassing the executor. They are not: every
byte lands in a reserved scratch that is renamed into place or discarded entire,
which is the same guarantee the executor gives, bought the way §R6.5 describes.
The gate could not see that, because `root: &Path` is a path like any other and
nothing distinguished a write into the staging tree from a write into a live
project.

`publish::Tree` is what makes it visible, and it is the fix that was actually
available:

- A `Tree` is obtained from a `Publication` and nowhere else, so **a function
  that takes one cannot reach a published project**. Thirteen `root: &Path`
  helpers in `new.rs` and two in `gradle_project.rs` take one now.
- Its absolute-path verbs (`put_at`, `put_named_at`, `ensure_directory_at`,
  `remove_at`, `put_executable_at`) **check containment rather than assuming
  it** — half of `new`'s writes arrive as absolute paths, because the source and
  test directories are computed once from the package name and joined many
  times. A write outside the staging tree is a refusal.
- `Publication::root()` is gone. Nothing hands out a bare path any more.

`publish.rs` joins the write layer in `tests/architecture.rs` on the strength of
that, not on a promise. **§4's count went 46 → 11**, and `root: &Path` 94 → 81 —
the second is this row's cure rather than a coincidence, since a `root` threaded
through a call graph so each level can re-derive facts is the disease, and a
`Tree` is the parameter object that says *which* tree.

What is left of §4's 11 is §7.7's work: `generate/write.rs`, `add/database.rs`,
`compose.rs`, `capture.rs`, `desire.rs`, `merge.rs`, `console.rs`, `run.rs`,
`testd.rs`, `doctor.rs`.

## 6. The abstractions worth introducing

Ordered by leverage. Every count here was taken today.

### 6.1 One `Codec` trait — **done 2026-08-25**

What it was:

```
  fn encode(&self, encoder: &mut Encoder) -> Result<()>     129 identical signatures
  fn decode(decoder: &mut Decoder<'_>)   -> Result<Self>    133 identical signatures
  traits in the entire workspace                              0
```

`jails_support::codec::Codec` now exists and **126 types implement it**. A
ratchet holds the migration: `codec halves outside `impl Codec`` reads
`0 / 0 / 0 done`, counting any method with either signature that is not a trait
method. Its scanner reads `trim_start()` rather than column zero, because
`digest_newtype!` and `logical_id!` expand to `impl Codec for $name` indented
inside a `macro_rules!` body — a gate that misread six good impls as violations
would have been retired within the week.

`Encoder`/`Decoder` gained `seq`, `set`, `map`, `maybe`/`perhaps` over the
bound, and **eight named monomorphisations are gone**: `encode_service_map`,
`decode_service_map`, `encode_service_set`, `decode_service_set`,
`encode_owners`, `decode_owners`, `encode_keys`, `decode_keys`, plus
`decode_vec` and the `encode_contributors`/`decode_contributors` aliases that
only forwarded to two of them.

Three things worth knowing before touching this again.

**Thirteen `encode` methods were infallible and are not any more.** They wrote
only fixed-width fields, so they returned `()`; the trait's signature is
`Result<()>`, and a type that cannot be `Codec` cannot go in a generic
collection — `ObjectId` is in a great many sets and maps. Making them fallible
cascaded through ~95 call sites that now carry `?`. Every one of those was
found by the compiler, not by reading.

**`InputPrecondition` was the eighth named copy in a disguise**: an inherent
`encode` method paired with a free `decode_precondition` *function*. Same split,
nothing naming it, and the gate above is what surfaced it.

**Two collections are deliberately not `Encoder::set`.**
`conflict::encode_paths` and the `entries` list in `snapshot::InputPrecondition`
order by a *field* of the element rather than by the element, so `T: Ord` where
`T`'s own order is the wire order does not hold. Both now say so in a comment.
That is the shape a future `ordered_by` helper would take, if a third appears.

The byte-level output is unchanged, and that is not an assumption: the golden
suite and the ledger round-trip tests compare bytes, and 1,169 tests pass.

### 6.2 One validated request, parsed at the edge — **done 2026-08-25**

A single `jails generate` existed in four shapes, and the manifest path built
three of them before anything was checked: `.jails/app.toml` →
`app::ResolvedIntent` (carrying the deprecated `strategy_on` aliases) →
`route::Intent` → `IntentSpec`.

**`ResolvedIntent` is deleted.** `GenerateIntent::finish` produces a
`route::Intent` directly, and the aliases are resolved by the parser that read
them — file-format syntax should not survive its own parser. Its three methods
became three free functions over `Intent` in the same module, `fingerprint`
still `#[cfg(test)]`.

**The route parses once.** `request::intent` (eight arguments, four of them
unused for long enough to have grown `_` prefixes) and `request::spec` (seven)
are one `request::declared(project, recipe, package) -> Declared { id, spec }`.
Both call sites — `artifact::generate` and the manifest loop — were already
building the same `Recipe` and then passing its parts to the two functions
separately.

The second parse is gone with them. Translating `--index created_at` into the
field it names needs the fields, so the arguments were parsed once for the
translation and then again inside `IntentSpec::parse`;
`IntentSpec::from_arguments` takes the parsed value, and `parse` is now a
two-line wrapper over it, so it stays the one authority on what a valid
declaration is.

**The `too_many_arguments` escape hatch is closed** — `deny`, not `allow`. Its
comment claimed 21 generator functions were waiting on a parameter-struct
refactor; re-measured, there were **nine**, and one of the five carrying a local
`#[allow]` no longer had eight arguments at all. Each took the parameter object
its arguments were already a group of:

| was | now |
|---|---|
| `pipeline::assemble`, 9 | `Produced` — what one preparation produced, plus `Produced::nothing()` for the two degenerate cases |
| `sql::finish`, 8 | the finished non-optional `Column` and one `bool`; the caller already knew all seven values |
| `repository::jdbc_repository_test_{for,with_wiring}`, 7 and 9 | `Subject` — the two adjacent `&str` triples were an ordering nothing could catch |
| `new_cli` and `new_offline`, 8 and 9 | the `Request` that already existed for `jails new` |
| `request::{intent,spec}`, 8 and 7 | `Declared`, above |
| `ledger::record_outputs` | nothing: it had three arguments and a stale `#[allow]` |

**One claim in this item was not met, and it could not have been.** "Parsed at
the edge" cannot mean at clap-parse time: a `FieldSpec` is parsed against the
project's base `Package`, which is not known until the project resolves. What
is true is that every parse now happens once, before the lock — `app_apply`
plans every row before any of them commits, so a malformed manifest is still
refused before a transition writes anything.

### 6.3 One field model — **done 2026-08-25**

`name:type[!?]@marker` had two parsers and two result types —
`jails-spec/src/spec/field.rs`'s `parse_fields` → `Field`, and
`jails-protocol/src/declaration/field.rs`'s `FieldSpec::parse` → `FieldSpec` —
and they were bridged **through text**:

```rust
// crates/jails-engine/src/route/field.rs, before
let parsed = jails_generate::generate::parse_fields(&[added.canonical()])?;
```

A typed `FieldSpec` was rendered back to a string token and re-parsed by the
other parser to obtain a `Field`. Counting the parse that already ran on the way
in, a field spec was parsed up to **three** times per request, and one of those
parses read text this program had printed a line earlier.

**Done 2026-08-25.** `parse_fields` split into the half that reads a token and
the half that *derives* the Java facts (`jails_spec::spec::derive_field`), and
`FieldSpec::projected()` calls the second directly with values it already holds.
The call site above is now `added.projected()?`. §6.3's own words for the target
were *"`java_type` and `imports` are derived facts computed by a function on
`FieldSpec`, not a second parse result"*; that function exists and has one
caller on each side.

**Done 2026-08-25: there is one parser.** The merge is smaller than either
option this item proposed, and nothing moved. `parse_fields` is *parsing*, so it
came up to the parser — it is four lines in `jails-protocol` now, mapping each
token through `FieldSpec::parse(..)?.projected()`. `derive_field` is
*derivation*, so it stayed below in `jails-spec` with the Java facts it
computes. `jails-generate` re-exports `parse_fields` from `generate.rs`, the
same job its facade block does for everything else, so every generator still
says `parse_fields`.

The base package is `Package::base()`, deliberately: a `Field` records `owned`
and a simple `java_type` and no package at all, so qualifying an owned type
against the project's base and then discarding the qualification would be an
argument every one of the 58 call sites had to supply and none could get wrong.
That assumption is what
`the_base_package_does_not_reach_the_derived_field` now asserts — the same
twenty-six tokens as before, parsed against a real package and against the base
one, deriving identical fields. It replaces
`a_projected_field_spec_equals_the_parsed_one`, which became a tautology the
moment the two parsers were one.

**Merging them found two live divergences, which is the argument for having
done it.** Neither was visible with two parsers, and the pinning test could not
see either because its token list had no case that distinguished them:

- **`amount:Currency` meant two different things.** `README.md` documents
  `jails g enum Currency GBP EUR USD` followed by `currency:Currency`, and
  `jails-spec`'s `builtin_by_java_name` implements it: capitalised means a type
  the project owns, and `Currency` is deliberately absent from the Java-name
  table because an enum of the currencies a project deals in is an ordinary
  thing to generate. `ScalarFieldType::parse` had `"currency" | "Currency"`, so
  the protocol read the same token as `java.util.Currency` — which means the
  `IntentSpec` in the ledger and the Java on disk disagreed about what the
  field was. Every other Java spelling in that match (`String`, `Instant`,
  `UUID`) is a name nobody declares themselves, which is the line the arm
  crossed.
- **`jails g field <Target> ref:SomeOwnedType` did not work at all.**
  `FieldSpec::projected` renders the type as `field_type.canonical()`, which for
  an owned type is fully qualified against the base package; `resolve_type`
  matched case on the *whole* token, so a qualified name fell through to the
  builtin table and was refused as `unknown field type
  'com.example.demo.domain.currency'` — a message about a type nobody typed.
  Case now applies to the simple name, the way `ScalarFieldType::parse` already
  did.

**One refusal changed wording**, and the surviving one is the better half: `!`
on a non-text type now says ``\`date\` is not text, so `!` (non-blank) has no
meaning for it`` with a `fix:` line, rather than the older "only applies to
text" with none. Deleting the duplicate parser took four refusals with it, which
is what moved §4's `fix:` ratchet from 443 to 439 — a duplicate parser is four
duplicate refusals.

`derive_field`'s three checks stay, and are not redundant: they run against the
*resolved Java type* rather than the declared one.
`a_declaration_that_parses_is_one_the_projection_derives` keeps the two halves
of the one parser in step, because a token accepted at the edge and refused
mid-transition is the failure that guard exists for.

### 6.4 One table per kind — **done 2026-08-25**

`ArtifactKind` has seven independently maintained tables keyed on it across five
crates: the enum and clap aliases; `metadata()`/`argument_shape()`;
`kind_suffix()`; `recorded_name()`/`strip_redundant_suffix()`;
`artifacts_for()`; `explain()`; `SCENARIOS`.

**The two holes are closed, 2026-08-25** — which is the half that was actually
costing correctness rather than typing:

- **`kind_suffix` ended in `_ => None`** (`generate.rs`), so a kind added to the
  enum got no suffix and nothing said so; `recorded_name` and
  `strip_redundant_suffix` read it and inherited the silence, and a kind whose
  name is not normalised is one whose `destroy` rebuilds different paths from
  the ones `generate` wrote. Every arm is now explicit, including the
  twenty-three kinds that genuinely add nothing, so a new variant fails to
  compile until somebody decides which half it is in.
- **Three copies of "is this kind a one-shot".** `generate::plan_recipe`
  refused `matches!(kind, Field | Cases | Migration)`; `route::artifact::recipe`
  listed the same three as match arms above a `_`, under a doc comment claiming
  *"the match is closed on `ArtifactKind`, so a kind added without deciding
  which policy it follows is a compile error"* — which the `_` made false;
  and `artifacts_for` carried three `unreachable!`s trusting both. There is one
  owner now, `jails_protocol::recipe::lifecycle`/`is_persistent`, which was
  already written and had no callers (§7.2 found it). `plan_recipe` asks it, and
  the route matches on `LifecycleClass` rather than on kinds, so both halves are
  exhaustive and a fourth one-shot cannot fall through to the persistent branch.
  `jails-generate` gained a direct `jails-protocol` dependency to ask; it is a
  downward edge and `no_module_depends_on_a_layer_above_its_own` agrees.

The `unreachable!` arms stay, and deliberately: they are the only thing trusting
the guard, and turning them into a `_` would let a new kind reach the persistent
renderer unclassified.

**The tables are one, 2026-08-25 — and the count was wrong.** Re-measured, the
seven were not seven things of one kind. Four of them are not tables that could
be merged, and merging them would have *created* second copies:

| | why it is not a row in `RecipeFacts` |
|---|---|
| the enum and clap aliases | clap's `ValueEnum` is the owner. A `label` field would be a hand-written copy of the name clap parses, which is the drift this item is about |
| `artifacts_for` | logic, not data — `recipes.rs`'s doc comment argues that correctly |
| `explain()` | prose with nowhere to derive it from, held to `why.rs`'s shape by `every_kind_has_an_explanation` |
| `SCENARIOS` | a test corpus, and `every_kind_and_capability_has_a_golden_scenario` already fails when it falls behind — the right relationship |

That leaves `metadata()` and `kind_suffix()`, and they are one now.
`RecipeMetadata` carries `suffix`, so one function in
`jails-protocol/src/vocabulary/recipe.rs` answers everything mechanical about a
recipe: lifecycle class, `--on`/`--yields` arity, argument shape, `--method`,
and the suffix. It keeps its shape of one arm list per *question* — a single
combined list would be unreadable at exactly the point a new kind is added —
but there is one place to edit.

`recorded_name` and `strip_redundant_suffix` moved with it, out of
`jails-generate`. That is a layering fix as much as a table one: they are
**identity** rules — `recorded_name` decides the name a ledger row carries —
and `jails-engine` was reaching down into the generators for one. Both are
re-exported from `generate.rs`, so every generator keeps its spelling.

**`Capability::label()` was a real second copy and is pinned rather than
deleted.** It returns `&'static str` without leaking, which the `ValueEnum`
route cannot (`recipe_label` leaks a `String` per call), so the match stays and
`every_capability_label_is_the_word_clap_parses` fails the build the moment it
separates from clap — plus the same pin for `HttpMethod`, whose label reaches a
generated annotation, and a round-trip for every `ArtifactKind`. `jails-spec`'s
`kind.rs` had no test module at all before this.

`Capability`'s other three appearances are the ones the table above excuses:
the clap enum, `add::plan_for`'s dispatch (logic), and
`capability_class`/`prerequisites` (already in `jails-protocol`, beside
`metadata`).

### 6.5 The empty-string sentinel — **closed 2026-08-25**

`src/main.rs` read:

```rust
if !err.is_empty() { eprintln!("jails: {err}"); }
```

An empty error string meant *"the command already printed everything; print
nothing more."* A control-flow decision encoded as the absence of characters in
a message. Seven commands depended on it — `doctor`, `lint`, `run`, `migrate`,
`testd`, `reports` and `invoke` — and nothing named it. The failure mode is the
quiet kind: any path that happened to build an empty message became "already
reported", and the process exited non-zero having printed nothing at all.

`jails_support::Failure` is that decision as a type. Two variants, which is
exactly the number of distinctions in use:

```rust
pub enum Failure {
    Told(String),
    /// The command has already reported. Set the exit code and say nothing.
    Reported,
}
```

`Result<T>` is `Result<T, Failure>`, `main` matches on `message()`, and the
seven sites say `Failure::Reported`.

**The message stays free text**, and the doc comment this replaced got that
trade right: the only consumer is `main`, which prints, so an enum per failure
mode would buy pattern-matching nobody does. What it got wrong was that there
were two outcomes, not one.

Three notes for whoever touches this next.

**It costs `.into()` at every `return Err(format!(..))`.** ~500 sites, all
inserted from rustc's own machine suggestions rather than by guessing — guessing
is how a tail `expr.map_err(..)` becomes `Result::into`, which is a different
error. Three ratchet rows rose a total of 17 lines because rustfmt wrapped the
closing paren, and the reasons are recorded beside them.

**`Failure` derefs to `str`.** Deliberate: 131 assertions say
`error.contains("...")`, and rewriting each to `error.to_string().contains(..)`
would have made a type migration look like a change to what the tests assert.
Code that needs to *know* whether anything was said calls `message()`, which is
the only thing that can reconstruct the distinction.

**`jails-commit`'s `runtime.rs` had its own `Failure`** — a post-commit
effect's, carrying a closed `EffectFailureCode` rather than free text. It is
`EffectFailure` now. Two types with one name at different layers is a collision
that would have been resolved by whichever `use` came last.

The second half of §6.5 landed too: **"every refusal carries a fix" is now
checkable across the workspace** rather than in `doctor`'s one test. The
`refusals with no fix: line` ratchet counts every `Err(..)` whose argument
builds a message and whose message has no `fix:` in it — located on the blanked
production text so `#[cfg(test)]` bodies and parens inside literals cannot
confuse it, then read from the raw file at the same offsets, because the message
is exactly what blanking erases.

**It reads 443.** The target is *withdrawn*, not reached, for the same reason
§8.0 withdrew `root: &Path`'s: a decoder rejecting a corrupt tag, a length over
its cap, a duplicate row in a receipt can only say what they found, and a `fix:`
on one would be an invented instruction. What the row buys is that the number
cannot rise — a new refusal has to carry a fix or lower something else — and
separating the two kinds is per-message work that brings it down.

**Not done, and deliberately:** splitting `Told(String)` into
`Told { what, fix }`. §6.5 proposed it, and the cost is the exact spelling: the
convention is `\n       fix: ` with that indentation, embedded mid-message, and
a `Display` that reassembled it as `\n\nfix: ` would change bytes the CLI tests
assert on. The ratchet above gets the checkability without the churn; the split
is worth doing when something needs the `fix` as a *value* rather than as text.

### 6.6 Where the other traits belong

Zero traits is a legitimate Rust style. It stops being legitimate where the same
shape repeats with no way to name it. 5.1 is the first. The others:

- **`Renderer`.** Every generator is a free function reached through a 36-arm
  match, each taking a different tuple. `spring::Slice` fixed exactly this for
  the Spring kinds and worked — no function in `spring.rs` takes more than five
  parameters and a ratchet holds it there. The same treatment has not reached
  `generate/recipes.rs`. 5.2's request object is what makes it possible.
- **`ToolRunner`.** Real Maven is mocked by shadowing `PATH` with a shell
  script, and `real_path_without_mvnd()` exists to rebuild `PATH` around `mvn`'s
  launcher shelling out to coreutils. That is a lot of machinery to avoid one
  trait behind `process::CommandSpec`. **Genuinely optional** — the PATH
  approach tests the real argv construction, which a fake would not. Weigh it,
  do not assume it.

---

## 7. Crates, and the concepts they should hold

Ten crates today. It is a DAG and Cargo enforces it, which is the win the split
bought.

### 7.1 Dead and unreal edges — **closed 2026-08-25**

All four done:

- **`crates/jails-tooling/src/rename.rs`** deleted — 220 lines, zero production
  callers, `main.rs` having routed `jails rename` through
  `jails_engine::route::maintenance::rename` since V2. Its module docs carried
  the one thing worth keeping (when to prefer jdt.ls `grn`, and why textual is
  honest here), which moved onto the live route's doc comment; the pointer in
  `jails-java/src/identifier.rs` re-points there.
- **`jails-tooling` → `jails-protocol`** edge deleted.
- **Root `jails` → `jails-commit`** dropped from `[dependencies]`; the
  `[dev-dependencies]` entry with `features = ["fault-injection"]` is what the
  44 `tests/` uses were always resolving through.
- **`[workspace.dependencies]` added** and all eleven manifests inherit from it.
  The per-crate comment saying *why* that crate takes clap stays beside
  `clap.workspace = true`; only the version moved.

Two ratchet rows fell as a result and are recorded: `root: &Path` 105 → 103, and
§4's bypass count 56 → 54.

(Two edges that item 3 of the old `refactor.md` also listed —
root → `jails-protocol` and root → `jails-spec` — are **no longer** movable:
they now have 9 and 4 uses in `src/` respectively.)

### 7.2 Closed crate APIs — **done 2026-08-25, and it found a great deal**

`dead_code = "deny"` was set workspace-wide and found almost nothing, because
Rust assumes a `pub` item in a library may be used by another crate. Every crate
root was `pub mod` for every module across ~829 `pub` items, so the compiler had
been told not to look.

Every crate is now narrowed: each item starts `pub(crate)` and only what
something outside really names is `pub`. Five modules went `pub(crate)` whole
(`commit::activate`, `prepare::merge`, `tooling::affected`/`launcher`/
`reports`). The method was mechanical and compiler-driven — narrow everything,
build, reopen exactly what the errors name — which is why it is trustworthy:
nothing was decided by reading.

**What it found, and what was done with each.** The rule applied throughout:
*if a finding has a live V2 counterpart doing the same job, delete it; otherwise
keep it `pub` with a note beside it saying nothing calls it and what would.*
Deleting a specified, tested feature because it is not wired yet is not cleanup.

Two real defects, both now fixed and pinned by a test:

- **`jails remove` left the dependency in `build.gradle`.**
  `projection.rs`'s `ResourceKey::MavenDependency` retirement opened
  `pom_path()` unconditionally, found no `pom.xml` on a Gradle project,
  returned "nothing to do", and reported the claim retired. The *installing*
  edit had branched on `self.build` since the day it was written, which is what
  made the asymmetry invisible. `gradle::remove_dependency` existed, was
  tested, and had no caller — and `pub` is exactly what stopped `dead_code`
  saying so. `ResourceKey::MavenMainClass` had the identical hole.
  `removing_a_capability_from_a_gradle_project_unsplices_the_dependency` in
  `tests/engine.rs` pins it, and the narrowing itself now fails the *build* if
  the branch is removed again.
- **§1.2's predicted fourth instance of "a version fact answered confidently
  and wrongly".** `generate/cli.rs` asked `pom::main_class(project.pom())`, and
  `Project::pom()` returns whichever build file the project has. Handed Groovy,
  the XML reader finds no `<mainClass>` and answers `None` — which means "this
  build declares no entry point", so `g cli` on a Gradle project silently
  declined to retarget the packaged jar. There is now one `Project::main_class`
  that dispatches on the build tool, and `gradle::main_class` — written, tested
  and never called — is its Gradle half.

Deleted, each having a live V2 counterpart: `apply`'s `put_bytes`, `move_file`,
`copy_into_scratch`, `remove_managed_tree`, `remove_managed_directory` and
`atomically`; `spec::fields_from_record`; `compose::add_service`/
`remove_service`/`has_service`/`stop`; `config::record_capability`/
`forget_capability`/`edit_capabilities`/`record_layout`; `properties::set`;
`maven::format_quietly`; `generate/write.rs`'s four dependency-ensuring
functions; `generate/scaffold.rs`'s `field_spec`, `generate_field` and
`prepared_artifact_contents`; `spring/durable.rs`'s install/uninstall pair;
`prepare`'s `preconditions_of`, `subject_of`, `contributors_of`,
`desire::POM`/`COMPOSE`; and `journal::ObservedImage::tag`.

**The test inventory went 1,171 → 1,169, and every one of the five removals is
accounted for.** Where a V1 entry point was the only thing a test called, the
test was re-pointed at the shipped splice it wrapped — `compose`'s
`add_service_ref`/`remove_service_ref`, `config`'s `edited_capabilities`,
`properties::introduce`, `codemod::Marked` for the durable-job blocks. Several
are *better* for it: they now exercise the function the shipped path calls.
Two tests were added (`no_bare_apply_verb_imports` and the Gradle removal
regression). The five that went, and why each is not a hole:

| test | why it is not a loss |
|---|---|
| `apply::atomically_leaves_no_temporary_behind` | its subject was deleted |
| `rename::a_package_qualified_name_is_rejected` | the live route has its own `validate`, and `tests/cli.rs` asserts the same "simple name" refusal end to end |
| `durable::an_edited_safety_block_is_rejected_instead_of_silently_clobbered` | superseded, and by something stronger. V1 compared the block's bytes with what it would have written. V2 records the file as a `FactKind::Properties` input, so **any** reader edit fails the commit precondition — not only one inside the markers |
| `durable::a_later_duplicate_cannot_override_the_safety_values` | the same mechanism |
| `durable::removing_the_only_job_removes_the_generated_source` | split in two: `removing_the_only_job_leaves_an_empty_source` keeps the half this module owns, and turning empty text into an absence is `projection::write_or_delete`'s |

`prefix_related_job_names_keep_independent_property_blocks` was nearly lost the
same way and was restored — a marker matched as a substring would have
`durable-job-email` retiring take `durable-job-email-sender`'s opening line
with it, which is a property of `codemod::Marked` worth keeping pinned.

Kept `pub` with the finding recorded beside it — five bodies of specified,
encoded, unit-tested work that nothing reaches:

| what | where | what would wire it |
|---|---|---|
| the conflict-resume protocol, 33 items | `protocol::pending`, `protocol::conflict`, `protocol::bootstrap` | §11, which lands as one piece or not at all |
| finalisation's two halves | `prepare::reconcile`'s `MarkerTokens::for_operation`, `still_conflicted` | the same |
| the prerequisite graph and reference resolution, 11 items | `protocol::recipe` | §6.2's one parsed request, validating at the edge |
| **garbage collection, entire** | `commit::gc` + `store::list_objects`/`is_object_name` | one call at the end of a successful commit |
| `ToolSpec` and `canonical_args` | `prepare::tool` | `route::format` building one instead of passing an identity and a `Vec<String>` separately — which is the shape `ToolSpec` exists to make impossible |

**The garbage-collection one is new and worth stating plainly: nothing collects
anything, so `.jails/objects` only grows.** Every rendered body, base and
preimage a project has ever had is still on disk. The module is complete and
tested; what is missing is the call site and the decision about where its
warnings go, and that decision is already written in its own header.

Two smaller ones in the same shape: `store::same_device` — the refusal that
stops a publication rename silently becoming a copy across a mount boundary —
is never made; and `prepare::report::summary` draws a distinction
(*a prepared Apply whose plan turned out to be empty*) that no command makes,
because `render_envelope`'s "nothing to do" is a different case.

Four ratchet rows fell and are recorded in `tests/architecture.rs`:
`root: &Path` 105 → 94, and §4's bypass count 56 → 46.

### 7.3 `jails-commit` no longer reaches up — **done 2026-08-25**

Committing a transaction is lower-level than knowing what Maven is, yet
`jails-commit` had 5 `jails_project::` references — `compat::*` and
`capture::list_directory`. That is one coherent thing (**reading and
translating jails' own machine state**) living in the crate that models Java
projects, because that is where `.jails/` reading grew up.

`jails-state` sits between `jails-protocol` and `jails-project` and holds
`compat` and `listing`. `jails-commit` depends on it instead, and
`grep -rn "jails_project::" crates/jails-commit/src` returns **0**.

Two things the extraction turned up. **`compat.rs` was already independent** —
its only imports were `jails_protocol::envelope::LedgerV2`, `jails_support` and
`std::path::Path`, so the crate boundary it needed had been available all along
and nothing had drawn it. And **the one reference from below was a doc comment**:
`jails-protocol::resource` mentions `codemod::Marked` in prose, which is what
made `codemod`'s move in §7.5 possible in the same pass.

`capture` keeps the name its callers use by re-exporting `list_directory`. Only
that function and its private half moved; the rest of `capture` knows what a
`Project` is and stays where it is.

`jails-commit` still reaches `jails-project` *transitively*, through
`jails-prepare`, because a `PreparedChange` is about a Java project. That is the
honest shape and not the thing §7.3 was about.

### 7.4 `jails-protocol` is four concepts — **grouped 2026-08-25**

**23** flat `pub mod`s. Every module had a genuinely distinct secret and said
so — this was careful work, not a mess. The problem was that a reader arriving
at `lib.rs` saw a flat list with no shape.

Four submodules now, and the grouping is a claim rather than filing: a type that
belonged in two of them would be a type doing two jobs.

```text
  vocabulary/   identity, declaration{,/field,/index}, recipe, coordinate,
                entity, resource            — what a value is allowed to be
  observe/      snapshot, fact, bootstrap, context, provenance
                                            — what a planner may know
  intent/       request, change, plan, transition, effect, edit, render,
                ownership                   — what is being asked for
  durable/      envelope, record, pending, conflict
                                            — what survives a crash
```

**Submodules, not crates**, as §7.4 said: mechanical, compiler-checked, free to
undo. Every module is re-exported at the crate root, so
`jails_protocol::identity::Name` still resolves and the grouping cost no call
site anything — renaming four hundred of them would have made a filing decision
look like an API change.

The `observe`/`intent` split is the one that carries weight. A planner reads the
first and writes the second, and a type appearing in both would be a fact that
could be asserted, which is the shape of a plan that justifies itself.

Promotion to a crate stays where §7.4 left it: exactly one group has a case, and
`durable`'s own header says so. It is the only group whose members have a *file*
behind them, and it belongs with `jails-state` from §7.3 — which now exists.

### 7.5 `jails-support` is three concepts now — **done 2026-08-25**

It was eight modules and four subjects, plus `Result`, `debug_cmd` and
`CWD_LOCK` at the root. All three moves landed:

- **`codemod` is in `jails-project`.** Its subject is a `# jails:<marker>` block
  in a *project's* `compose.yaml`, keyed to jails' own comment syntax, which
  does not clear `lib.rs`'s own bar — *"a module belongs at this layer only when
  it would still make sense in a tool that had never heard of Maven."* The one
  reference from below turned out to be a doc comment in
  `jails-protocol::resource`, not code, so the edge was never real.
- **`runner` is `hermetic`.** `process` runs a program with the reader's
  terminal, inherited environment and no timeout; this one runs it with a
  timeout, a byte cap and nothing inherited. Two near-identical names for two
  different safety contracts is how a caller reaches for the wrong one.
- **`CWD_LOCK` is `jails-testkit`**, a crate taken as a `[dev-dependency]` by
  the three crates whose tests change the process-global working directory. The
  old doc comment's reasoning was right — a `#[cfg(test)]` item is invisible to
  a *dependent* crate's tests — and that is a reason to give it a crate, not a
  reason to ship it in the lowest layer's public API. The scope is unchanged:
  every test binary is its own process, so each links one instance.

What is left is coherent: **write, run, encode.**

### 7.6 `jails-tooling` was two crates wearing one name — **split 2026-08-25**

17 modules, two unrelated jobs. They are `jails-report` and `jails-drive` now:

- **`jails-report` answers a question** and is read-only by contract: `doctor`,
  `why`, `explain`, `commands`, `source`.
- **`jails-drive` starts something**: `run`, `testd`, `launcher`, `affected`,
  `reports`, `migrate`, `kafka`, `console`, `bench`, `lint`.

**The contract is structural rather than a promise.** `doctor` used to live one
`use` away from `run::mvn`; `jails-report` cannot depend on `jails-drive`
because `jails-drive` depends on *it*, so a reporting command that started
something would not compile.

Two things worth knowing about the direction.

**It goes the way round the original item did not expect.** §7.6 imagined the
reporting crate simply not depending on the driving one; in fact the edge exists
and points *down* — `run` → `report::why`, because `mvn spring-boot:run` exits 0
over a failed startup, so `run` pipes its own output and explains the failure
inline. That is a better arrangement than two unrelated crates: the dependency
is real, it is one-way, and it puts the read-only crate underneath.

**Severing `doctor` took one deletion.** `run::find_on_path` was a one-line
alias for `process::on_path`, and it was `doctor`'s only reason to name `run` at
all. The `doctor` module-lines ratchet fell 1481 → 1479 as a result.

### 7.7 Every mutation goes through the executor — **done 2026-08-25**

The largest crate holds one job — *decide what Java to write* — and held one
leftover. §7.2 deleted most of it, §5's `publish::Tree` took the `jails new`
path out of the count, and §4's row went 56 → 46 → 11 → 6.

**It is 0, and the R6.4 rung is reached.** Each of the last six was a decision
rather than a migration, and each was decided:

| where | what it was | what it is |
|---|---|---|
| `generate/write.rs` ×2 | `apply::create` and the `package-info.java` write, on a `root: &Path` | `write_new_file` takes an `apply::Tree`. Every one of its nine callers is on the `jails new` path, so the signature says what a comment used to — and a write outside the staging tree is now *refused* rather than merely not attempted |
| `run.rs` | spliced `junit-platform-console` into `pom.xml` from inside `test --fast` | `route::install_fast_test`, which was already written and unwired. `jails remove fast-test` is the other half |
| `add/database.rs` | a delete under `target/` after a source is removed | `apply::remove_derived` |
| `console.rs` | a classpath directory under `target/` | `apply::ensure_derived_directory` |
| `testd.rs` | the daemon's cache directory under the user's home | `apply::ensure_directory_outside_project` |

Three things worth keeping from how it went.

**`Tree` moved down to `jails-support::apply`.** §5 put it beside `jails new`
because that is the only thing that publishes; the generators write into it
too, and a type is where the writes are. `Publication` — the lock, the scratch,
the rename, the "already exists" refusal — stays in `src/new/publish.rs`, which
is the half that knows what a *new project* is.

**The two derived verbs refuse rather than promise.** `remove_derived` and
`ensure_derived_directory` check for a `target` or `build` path segment and
error otherwise, so "this is build output, not the project" is a claim the
program enforces. That matters because the exemption is what lets the gate stop
counting them: an exemption on a name anybody could apply to a `src/` path
would be the gate reading green over exactly what it exists to catch.

**`apply::Tree` had to be exempted too, and for the opposite reason.** The gate
counts the literal `apply::`, and `use jails_support::apply::Tree;` is an
import of the type that makes a staging write checkable — the opposite of a
bypass. That one import was the difference between 1 and 0.

**Two follow-on measurements.** `root: &Path` fell 80 → 77, because
`write_new_file` and `ensure_package_info` stopped taking a root at all; and
`A_FRESH_READ_IS_CORRECT` lost two of its four entries, since neither
`ensure_console_launcher` nor `ensure_package_info` re-reads a pom any more.

**A limitation of §7.2's pass, recorded so it is not mistaken for coverage.**
The narrowing reopened items by name, taken from the compiler's error text —
so an item with a *common* name (`write`, `read`, `path`) could be reopened
because some unrelated error mentioned the word, and `dead_code` then stopped
looking at it. `compose::write` was exactly that: zero callers, deleted here.
`why::FATAL_MARKERS` was the other. To re-derive:

```sh
# every `pub` item no file but its own mentions, by name
grep -rn "^pub \(fn\|struct\|enum\|const\|type\|trait\) " crates/*/src src
```

and check the survivors by hand. The 41 that remain are the documented
unwired-protocol bodies from §7.2's table, not new findings.

## 8. Files and tests

### 8.1 Modules with a visible seam — **the four named ones are split**

All four cuts §8.1 named have landed. Sizes are raw lines, since that is what a
reader scrolls:

| was | is now |
|---|---|
| `route.rs` 880 | `route.rs` 136 + `route/request.rs` 586 + `route/commit.rs` 195 |
| `route/maintenance.rs` 717 | a module root of 28 + `rename` 320, `format` 182, `adopt` 132, `app_init` 84 |
| `src/main.rs` 1,071 | `main.rs` 377 + `cli.rs` 718 |
| `src/new.rs` 1,283 | `new.rs` 386 + `new/spring.rs` 575 + `new/plain.rs` 237 + `new/seed.rs` 138 |

Each cut is the seam the item named, not a size one:

- **`route`**: *assembling a request* against *driving a commit*. Everything in
  `request.rs` is testable against a `Project` and a store with no transaction
  in sight; everything in `commit.rs` needs a lock, a journal and a project on
  disk.
- **`maintenance`**: *"maintenance" is a filing category, not a secret*, so it
  is a module root and one file per command. What the four share is a rule
  about *not* creating a desired entity, and a rule nobody can violate by
  accident does not need them in one file to hold.
- **`main.rs`**: what the CLI *accepts* against what it *does*. `cli.rs` is read
  when somebody asks "what can I type"; `main`'s match when they ask "what does
  it do".
- **`new.rs`**: the half that knows what Spring is, the half that does not, and
  what both seed. `seed.rs` is its own file rather than a section of either,
  because a helper both call from a file one of them owns is a helper that will
  grow a special case for its owner.

**A gate caught the split, and the gate was wrong.** `functions in spring.rs
taking over 5 parameters` matched `ends_with("spring.rs")`, so the new
`src/new/spring.rs` joined the set the moment it appeared and two rows went red
for a file neither is about. That is `module_of`'s failure from §10.3 in another
place: a name is not an identity. Four gates now name their file by path
(`SPRING_RS`, `CODEMOD_RS`, `DOCTOR_RS`, `SCRATCH_RS`).

Still open from this item: the largest module is `projection.rs` at 662, and it
is the honest answer to the next rise there rather than another ceiling.

### 8.2 Test files — **done 2026-08-25**

```
  8,142  tests/cli.rs          175 #[test]      3,581  tests/engine.rs
  1,816  tests/architecture.rs
```

Both split into submodules of **one binary**, not into new binaries: each extra
integration-test target is a full link of the workspace and there are already
nine.

```
tests/architecture/  main 46  board 777  rules 506  measure 908
tests/cli/           main 920  generate 2,146  capabilities 1,853
                     tooling 1,227  app 793  new 650  reports 602
```

`architecture` split along the seam the item named — the ratchet board, the
rules, the measurement (with the Rust blanking parser and its own unit tests).
`cli` split by **subject** rather than by tier, because which tier a test is in
is already visible in whether it calls `common::skip`, while what a test is
*about* was visible nowhere. `reports` is a sixth subject the item did not
predict: the read-only commands (`about`, `routes`, `beans`, `doctor`, `why`,
`notes`, `stats`, `src`, `lint`, `rename`, `completion`) are a third of the file
and share a shape — a fixture on disk, one assertion on stdout, nothing started.

Three things a mechanical split had to get right, and the first two cost a
restart each:

- **A brace-matching splitter must blank strings first.** Ending a test at the
  next line that is exactly `}` cuts a Java-heavy test mid-literal, which is the
  same trap §8.3 records for `generate.rs`. The splitter blanks comments and
  string literals — including `r#"…"#` — to spaces of the same length and counts
  braces in the blanked copy, then slices the original.
- **`tests/<name>.rs` is a crate root, so `mod board;` resolves to
  `tests/board.rs`.** The fix is `git mv` to `tests/<name>/main.rs`, which cargo
  discovers as the same target. `tests/common/` stays where it is, shared with
  the other eight binaries, and each new root reaches it through
  `#[path = "../common/mod.rs"]`.
- **A reader of one file reports the world ended when that file moves.**
  `golden.rs`'s `COVERED_ELSEWHERE` check read `tests/cli.rs` to prove the
  exemption still had a test behind it; it now reads every file under
  `tests/cli/`. `include_str!` is relative to the file, so every manifest path
  gained a `../`.

### 8.3 The colocated-test convention has two exceptions — **done 2026-08-25**

- **`crates/jails-generate/src/generate.rs`** carried **1,020** lines of tests
  belonging to its submodules. `CLAUDE.md` documented why a mechanical
  extraction had failed: the tests contain Java strings full of braces, so a
  brace-matching splitter cut them mid-identifier.

  That is exactly the trap §8.2's splitter had just been taught to avoid, so the
  extraction was mechanical after all — blank comments and string literals to
  spaces of the same length, count braces in the blanked copy, slice the
  original. 901 lines moved: 16 tests to `generate/domain.rs`, 10 to
  `generate/web.rs`, 5 to `generate/repository.rs`, 4 to `generate/cli.rs`.

  **Thirteen went further than the item asked.** `field_type_*`, the column
  markers and every `parse_fields_*` test were testing `jails-spec`'s field
  spec through `generate.rs`'s re-export, so they went to
  `crates/jails-spec/src/spec/field.rs` — the crate that owns the code. The
  file was 1,474 lines and is 572; the nine tests still there are the nine
  about `generate.rs` itself (`strip_redundant_suffix`, `capitalize`,
  `find_project_root`, `base_package`, and the cross-table sample check).

- **`jails-engine` has zero `#[cfg(test)]` modules** — it had, and now has nine
  tests over the helpers `tests/engine.rs` could only reach through a whole
  command: `route::label` (every `ArtifactKind`'s printed spelling parses back,
  so a refusal naming `jails g <kind>` names one that exists),
  `route::relative_path` (both directions, including the refusal), and
  `request::as_field_names` / `request::dispatcher_source`, the two pure
  translations at the request boundary.

  `as_field_names` is the one worth having: `--index created_at` is column
  spelling for a field called `createdAt`, and the pass-through case — a token
  naming neither — is load-bearing, because rewriting it into something
  plausible would rob `IndexSpec::parse` of the refusal that lists the fields
  that *are* declared.

### 8.4 `playground/` — **untracked 2026-08-25**

A fully generated Java project, committed, regenerated by hand, drifting
silently whenever a template changed with nothing failing. It had **grown** —
663 tracked files across four applications, 1,773 on disk, against the 663 this
entry first recorded.

Re-measured before deciding, and the answer was clearer than the item's two
options suggested: **all four are generated from manifests that live in
`examples/`**, and `playground/ledger-cli/.jails/app.toml` is byte-identical to
`examples/ledger-cli/.jails/app.toml`. (`playground/intercom` is
`examples/support-inbox` under an older name.)

So the "give it a test that regenerates and diffs it" option was already taken,
and by something stronger than a diff: `SPRING_APP_MANIFESTS` and
`ledger_cli_manifest_builds_without_spring` `include_str!` those manifests,
generate from them, and run the full Maven gate. A second golden corpus would
pin bytes `tests/golden/` already pins for the same generators, at 1,773 files
of maintenance nobody was doing.

Untracked and gitignored rather than deleted, so a local scratch copy costs
nothing — the same bargain `/deps/` gets. `examples/` is the shape that was
always right: the manifest and its markdown, with the generated output not
committed.

## 9. Test-suite performance

**Not achieved: plain, unfiltered `cargo test` under 30 seconds.**

Measured 2026-08-25, after a binary change invalidated the generated-project
cache:

```sh
/usr/bin/time -v -o /tmp/jails-full.time \
  env -u JAVA_TOOL_OPTIONS -u MAVEN_OPTS cargo test > /tmp/jails-full.log 2>&1
```

59.60 s wall, 292.91 user CPU-seconds, 57.63 system, 648,816 KiB peak RSS,
173,537 involuntary context switches. The CLI binary alone was 38.74 s with
177/177 passing. The best recorded warm CLI figure is 38.54 s, so the CLI half
is at its known floor and the extra ~21 s is regeneration plus the other nine
binaries.

**The bottleneck is CPU and scheduling, not disk, network or swap.** A warm CLI
run accumulates ~255 CPU-seconds in ~34 wall seconds with ~219k involuntary
context switches, only 16 major faults, and almost no permit-queue time at a
limit of eight. The tail is three concurrent real Failsafe runs against the
shared PostgreSQL and Kafka services; once they start they alone determine the
end of the binary.

**The next optimization must shorten or safely overlap that Failsafe/Maven tail
without deleting, disabling, ignoring or filtering any acceptance test.** The
remaining candidates are another reduction in Maven/JVM startup work, or a safe
long-lived build daemon.

### Reproducing a measurement

Dependency-warm throughout: Cargo registry, Maven local repository, container
images and Rust artifacts already present. Do not delete `~/.cargo`, `~/.m2` or
the image store. `target/jails-e2e-cache` holds generated projects and their
Maven `target` directories; **its key includes the `jails` executable**, so
always warm up after switching revisions.

```sh
# a warm number is the *second* invocation
cargo test --test cli && cargo test --test cli

# per-subprocess timings: start_ms, run_start_ms, end_ms, queue_ms, run_ms
JAILS_TEST_PROFILE=1 cargo test --test cli -- --nocapture 2>&1 \
  | rg JAILS_TEST_PROFILE

# the toolchain permit limit is 6 unless overridden; record it with any result
JAILS_TEST_MAX_TOOLCHAIN_PROCESSES=8 JAILS_TEST_PROFILE=1 cargo test --test cli
```

`queue_ms` is time waiting for a toolchain permit and `run_ms` is time inside
the child — libtest's own per-test duration includes the wait and is therefore
misleading. Stable libtest rejects `--report-time`; it is nightly-only.

Run three warm trials and report min/median/max. Never compare a quiet run with
one taken while another Cargo, Maven, Java or Podman job is active. A full zram
allocation is **not** proof of swapping — `vmstat`'s `si`/`so` columns decide
that.

Verify a measured run did not silently omit tests:

```sh
rg 'test result:' /tmp/jails-full.log
git diff <base>..HEAD -- | rg '@Disabled|#\[ignore\]|DskipTests|Dtest='
```

### Experiments deliberately not retained

Recorded so nobody re-walks a path that made the suite slower:

| tried | result |
|---|---|
| three-module Maven reactor | 56.70 s, peak RSS ~1.30 GiB |
| parallel Failsafe classes | 47.79 s — the ITs compete for the same host resources |
| Surefire and Failsafe in one Maven verification | 45.56 s |
| four-process toolchain budget | 62.50 s |
| eight-process budget | 88.53 s, host load above 24 |
| `mvnd` | unreliable here — stale daemon socket |
| Testcontainers cross-run reuse | **rejected on correctness**: the reuse key does not identify the project, retained state leaks between runs, and Ryuk deliberately does not reap reusable containers |

Two retained findings worth not rediscovering: short-lived Maven JVMs use Serial
GC and `-XX:TieredStopAtLevel=1` (a representative verification went 9.89 s →
4.54 s), and Podman's default `--pull=missing` cost 66.25 s on a fully cached
build against 1.19 s with `--pull=never` — the harness pre-pulls every `FROM`
image, so its builds use `--pull=never`.

### Success criteria, unchanged

- `cargo test` still runs every Rust, generated JUnit, Surefire, Failsafe and
  container integration test it runs today.
- No test requires developer services on ports 5432, 6379 or 9092.
- No unrelated Spring context repeatedly attempts to connect to Kafka.
- The suite passes from a clean container state.
- Wall time, peak aggregate memory, involuntary context switches and container
  starts are recorded for each phase.

---

## 10. Documentation and the gates that shape it

### 10.1 `CLAUDE.md` describes a repository that no longer exists

It documents **seven** crates; there are **ten**. `jails-engine`,
`jails-commit`, `jails-prepare` and `jails-protocol` — the largest share of the
workspace — appear nowhere in its crate table. It also describes
`crates/jails-tooling/src/rename.rs` as live code (6.1: zero callers).

Two changes:

- **Add the four missing crates to the table before doing anything else in this
  file.** Every item above was harder to establish than it needed to be because
  the map was four crates out of date.
- Cut it to what is **not derivable from the code**: the crate map, the scope
  bar, and the gotchas that cost real time (podman's socket, Jackson 3,
  `mvn spring-boot:run` exiting 0, mvnd under JDK 26). Anything that restates
  what a module does belongs in that module's doc comment, where it sits next to
  the code and goes stale visibly.

### 10.2 Load-bearing citations of deleted files

154 `plan.md` references and 48 `abstract.md` in `.rs`, plus six naming
`missing.md` or `refactor.md`. `CLAUDE.md` is right that these resolve through
git and right that they are the best record of *why*. But a citation needing two
git commands to follow is one nobody follows, and the code is organised around
section numbers in documents that are not present.

Promote the ones a **rule still depends on** into short decision records and
re-point those citations:

```text
  docs/decisions/001-one-writer.md
  docs/decisions/002-transaction-protocol.md
  docs/decisions/003-machine-state-compatibility.md
  docs/decisions/004-hermetic-processes.md
  docs/decisions/005-closed-schemas.md
```

Leave the rest citing `plan.md §N`. A *historical* citation is fine; a
load-bearing one is not.

### 10.3 A test was choosing production names — **closed 2026-08-25**

`src/invoke.rs` opened by explaining it was named `invoke` rather than
`dispatch` because `jails-java` already has a `dispatch`, and
`no_two_crates_share_a_module_name` identified a file by its first path
component.

`module_of` answers `(crate, module)` now, so the collision cannot arise, the
gate that forbade it is gone, and the file is `src/dispatch.rs`.

Two things came out of it that were not in the original item.

**`LAYERS` had four rows naming modules that are not there** — `ledger` and
`migration` had become submodules (`pipeline/ledger.rs`,
`generate/migration.rs`), `rename` was deleted, and `main.rs` is excluded by
`module_of` by design. Nothing had ever checked the other direction. It does
now, with the same rule `SUBPROCESS_CLASSIFICATION` is held to: a row naming a
module that is no longer there is permission for nothing, and it hides the fact
that the module went.

**The table is crate-qualified**, which makes it say out loud what its comments
were saying in prose, and makes the same-crate case explicit: a reference within
one crate is a same-level edge by construction, so only the crates above are
checked.

Not done: reading the crate-dependency table from `cargo metadata` instead of
rebuilding it from source text. §10.3 offered it as a way to drop the Rust
parser, but the parser is not there for the dependency table — it is there for
`blank()`, which every other gate in the file needs.

## 11. Not started, and open by design

- **Hosted CI has never been set up.** No `.github/workflows` exists. This is
  the last item from the V2 cutover. The four example applications are already
  proved by the suite — `SPRING_APP_MANIFESTS` and
  `ledger_cli_manifest_builds_without_spring` `include_str!` the
  `examples/*/.jails/app.toml` files, generate from them and run the full Maven
  gate — so a manifest that stopped building fails `cargo test`. There is
  nothing to write but the workflow.

- **Conflicted merges cannot be resumed.** When a regeneration and a reader's
  edit genuinely overlap, the three-way merge produces conflict markers. The
  specification commits those with a frozen record that the next invocation
  continues or aborts. The bytes are produced and validated and
  `jails-protocol/src/conflict.rs` has the abort's both-images machinery; the
  frozen record, the refusal while it stands, and the continue/abort commands do
  not exist (`jails --help` has no `continue` or `abort`). jails refuses
  instead, naming the hunk count. **It lands as one piece or not at all** — a
  project that can enter a conflicted state and not leave it is worse than one
  that refuses the merge. Building the enter side alone was tried and backed
  out.

- **Unmeasured:** the k6 load profile `add loadtest` writes has never been run,
  so the p99 claim is unmeasured and says so. Spring context-cache misses across
  the example applications have never been counted.

- **Anti-goals**, unchanged: domain-specific generators, executable plugin
  hooks, a conditional template language, an ORM or a runtime support jar,
  silent Gradle support, an embedded model server, incremental `check`, or
  treating a skipped test as coverage.

---

## Sequencing

Each PR leaves `cargo build --workspace && cargo test --workspace` green.

0. **Decide the proof-application tier** (§2.4) before adding any of the three
   new applications. It is not code and it takes an afternoon, but three
   applications arriving one at a time is how the suite reaches two minutes with
   nobody having chosen that. Proving `examples/minicom/` and
   `examples/minicom-spring/`, which exist and are held by nothing, is the
   cheapest first move and answers the question with real numbers. **Still
   open.**
1. ~~**Honest gates and an honest map**~~ — **done 2026-08-25.** §4's row now
   measures what its rung claims (and the number was 56, not the 69 a raw grep
   reported); `CLAUDE.md`'s crate table names all ten crates.
2. ~~**Delete and close**~~ — **done 2026-08-25.** `rename.rs` and the three
   unreal edges are gone, `[workspace.dependencies]` exists, and every crate's
   API is narrowed. See §7.2 for what `dead_code` then found, which was
   considerably more than "some dead code": two shipped defects and five bodies
   of specified, tested, unwired work.
3. ~~**The `Codec` trait**~~ — **done 2026-08-25.** 126 types, eight named
   monomorphisations deleted, a ratchet at zero. See §6.1.
4. ~~**One request, one field model**~~ (§6.2, §6.3) — **done 2026-08-25.**
   `ResolvedIntent` deleted, one parse per request, one field-spec parser, and
   `too_many_arguments` is `deny`. The merge found two live divergences the
   pinning test could not see: `amount:Currency` meant the built-in to one
   parser and a project enum to the other, and `g field X ref:SomeOwnedType`
   did not work at all.
5. ~~**One table per kind**~~ (§6.4) — **done 2026-08-25.** Four of the seven
   were never mergeable tables; the two that were are one, and the third real
   copy (`Capability::label`) is pinned to clap by a test.
6. ~~**One transaction protocol**~~ (§5, §7.7) — **done 2026-08-25.** §4's gate
   reads **0** against a target of 0, honestly: 56 → 46 → 11 → 6 → 0. `jails
   new` did not become a V2 transition and should not — §5 re-measured that and
   found publication-by-rename already gives the guarantee — but it says so in
   a type now, and the last six direct writes were each decided rather than
   moved.
7. **Crate boundaries** (§7.3, §7.4, §7.5) and the file splits (§8).

The three new proof applications (§2.1, §2.2, §2.3) sequence against §9 rather
than against this list: each one lengthens the Failsafe tail, so §9's work is
what makes them affordable, and §2.4 is what decides whether they wait for it.

**If only four things get done:** §4 (the gate that reads green over unfinished
work), §7.2 (close the APIs and let the compiler find the dead code), §6.1 (the
cheapest large reduction in the repo), §6.3 (the deepest remaining seam between
the two engines). **All four are done.**

---

## Already closed — do not re-derive these

Each was listed as pending in one of the merged files and was **verified done on
2026-08-25**. Kept as a short list so nobody re-measures them; the evidence and
reasoning are in git.

From `missing.md` — **all eight entries, closed**. It was the raw evidence from
one real migration (`minicom-public/spring` → `spring-4`, 2026-08-24): what a
real project needed and did not get. Every gap it named now has a command:
`--group`/`--package` on `new`; `--method`/`--returns`/`--on` on `g controller`;
`jails add h2`; `jails add dependency <g>:<a>`; the CORS `CorsFilter`
registration and its preflight-through-the-dispatcher test; `jails set`/`unset`;
the `doctor` check for `spring.sql.init.mode` with no schema file;
`jails set --tests`.

From `refactor.md`:

| item | why it is closed |
|---|---|
| the red architecture gate | green; the board passes |
| `/minicom/` untracked and unignored | `.gitignore:22` |
| V1 dead code (~1,380 lines) | `add/shrink.rs`, `generate/remove.rs`, `add::add`/`add_in`, `generate_in_project` all deleted. Only `rename.rs` survives — see §7.1 |
| root → `jails-protocol`, root → `jails-spec` as dev-deps | no longer movable: 9 and 4 uses in `src/` |
| `generate cli` retargets `<mainClass>` with a direct write | it is `SemanticEdit::MavenMainClass` now, carrying the entry point it displaces so `destroy` restores it |

From `test.md`: Phase 1 (deterministic Kafka wiring, the shared process gate),
Phase 2 (short-lived JVM settings, shared Spring toolboxes), Phase 3 (shared
suite PostgreSQL and Kafka with per-application isolation) and Phase 4
(benchmarked toolchain limits — six is the retained value) all landed. The CLI
binary went 156.89 s → 38.54 s warm. What is left is §9.
