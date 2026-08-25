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
   2  crates/jails-tooling/src/testd.rs
   2  src/new/publish.rs                          §5 — keep, see there
   1  each: add/database.rs, generate/scaffold.rs, merge.rs,
        console.rs, doctor.rs, run.rs
```

`src/new.rs` and `src/new/gradle_project.rs` are 33 of the 54, which is what
makes §5 the largest single move rather than a tidy-up.

## 5. `jails new` is a second transaction protocol

`src/new/publish.rs` implements publication-by-rename: write everything into a
sibling scratch directory, `rename` once, so the destination is absent or
complete. Its doc comment justifies this correctly — `jails new` has no project
to lock and no ledger to journal, because there is nothing there yet.

That reasoning is sound for the *first* write. It stops being sound for
`jails new --app <manifest>`, which then runs a whole `app apply` inside the
scratch tree through a mechanism with no journal, no recovery and no conflict
detection — the three things `jails-commit` exists to provide.

The honest shape: `new` reserves and publishes the *skeleton* by rename (keep
`publish.rs` exactly as it is), then `--app` runs an ordinary V2 transition
against the now-real project. One transaction protocol, and `publish.rs` shrinks
to what only it can do.

`src/new.rs` has 21 direct `apply::*` calls and `src/new/gradle_project.rs`
another 12 — the most of any pair of files, and the Gradle half is new debt
added on 2026-08-25 with eyes open. Those 33 are 61% of §4's whole remaining
count, so this item *is* that gate's descent.

---

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

### 6.2 One validated request, parsed at the edge

A single `jails generate` still exists in four shapes:

```
  clap `Command::Generate { kind, name, fields: Vec<String>, … }`   strings
  src/app.rs:104  ResolvedIntent                                    strings
  jails-engine/src/route/app.rs:43  Intent                          strings
  jails-protocol/src/declaration.rs:268  IntentSpec                 validated
```

Validation happens in `IntentSpec::parse`, called from inside the route — after
the run has started. The manifest path is worse: `.jails/app.toml` →
`ResolvedIntent` (with the deprecated `strategy_on` aliases) → `route::Intent` →
`IntentSpec`. Three copies before anything is checked.

Parse once, at each edge, into one value holding the already-parsed `Recipe`,
`Name`, `Package`, `Vec<FieldSpec>`, `Vec<IndexSpec>` and resolved
`on`/`yields`. `ResolvedIntent` and `route::Intent` both collapse into it. The
manifest's deprecated-alias handling stays in `src/app/manifest.rs` where it
belongs: it is file-format syntax and should not survive past the parser.

Note `jails-protocol/src/request.rs:46` already declares
`CanonicalRequestSyntaxV1` — the name is taken and the concept is half-present,
which is worth checking before inventing a third.

Payoff: a malformed manifest is refused **before** the transition starts, the
double parse in the route disappears, and the workspace-wide
`too_many_arguments = "allow"` (`Cargo.toml:23`, whose comment says it is
waiting on exactly this) can come out.

### 6.3 One field model — **the round trip is gone; one parser is still two**

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

**What is not done: there are still two parsers.** Merging them is blocked on
layering, and the shape of the block is worth writing down because it decides
how the merge has to go. `Field`, `Optionality` and `Constraints` live in
`jails-spec`; `FieldSpec`, its own `Optionality` and `FieldConstraints` live in
`jails-protocol`, which is **above** `jails-spec` — so the lower parser cannot
delegate to the upper one, and `jails-generate` does not depend on
`jails-protocol` at all, which is why the bridge was a string in the first
place. The merge is therefore a *move*, not a rewrite: either `FieldSpec` comes
down to `jails-spec` (dragging `identity::{JavaType, Name, Package}` with it) or
`Field` goes up. The second is smaller and is the one to do.

Until then the two are pinned to each other by
`a_projected_field_spec_equals_the_parsed_one`, which runs 26 tokens through
both and compares all seven fields of the result, and by
`a_spec_the_projection_refuses_is_one_the_parser_refuses`, which asserts they
agree about what to reject. **A failure there means one parser learned something
the other did not; the fix is to teach the projection, never to relax the
assertion.** The guard was checked by breaking it — dropping `indexed` from the
projection fails on `owner:string@index`.

### 6.4 One table per kind — **the holes are closed; the tables are still seven**

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

**Still open: the tables themselves.** One `RecipeFacts` value returned by a
single exhaustive match, carrying label, aliases, suffix, layer, lifecycle,
argument shape and rationale. `artifacts_for` stays a `match` — its arms are
*logic*, not data, and `recipes.rs`'s doc comment argues that correctly.
`Capability` has the same shape across four crates.

Keep `SCENARIOS` separate. It is a test corpus, and
`every_kind_and_capability_has_a_golden_scenario` already fails when it falls
behind, which is the right relationship.

### 6.5 `Result<T, String>` and the empty-string sentinel

`src/main.rs:1029`:

```rust
if !err.is_empty() { eprintln!("jails: {err}"); }
```

An empty error string means *"the command already printed everything; print
nothing more."* A control-flow decision encoded as the absence of characters in
a message. `doctor` depends on it and nothing names it.

The `"fix:"` convention is real and load-bearing — every `FAIL` is supposed to
carry one — but it is a substring convention over free text, so it can only be
checked where someone greps for it.

```rust
pub enum Failure {
    Told { what: String, fix: Option<String> },
    /// The command has already reported. Set the exit code and say nothing.
    Reported,
}
```

`Display` renders `what` and, when present, `\n\nfix: {fix}`, so every existing
message keeps its exact shape and the hand-written `fix:` lines migrate
mechanically. **Do not** replace `String` with an error enum per failure mode —
the existing doc comment on `Result` gets that trade right, since the only
consumer is `main`, which prints. This adds the two distinctions actually used
and nothing else, and makes "every refusal carries a fix" checkable across the
workspace rather than in `doctor`'s one test.

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

### 7.3 `jails-commit` reaches up into `jails-project`

Committing a transaction is lower-level than knowing what Maven is, yet
`jails-commit` has 5 `jails_project::` references — `compat::*` and
`capture::list_directory`. That is one coherent thing (**reading and translating
jails' own machine state**) living in the crate that models Java projects,
because that is where `.jails/` reading grew up.

Extract a `jails-state` crate between `jails-protocol` and `jails-project`
holding `.jails/` discovery, ledger read/write, the envelope's file half and
`capture::list_directory`. `jails-project` keeps what makes it *a Java project*:
`pom`, `compose`, `config`, `model`, `projection`, `inspect`, `maven`, `junit`,
`synonyms`.

`jails-commit`'s doc comment says its whole point is that the executor is small
because there is one direction to finish in. It cannot be small while it also
has to know how `.jails/` is laid out.

### 7.4 `jails-protocol` is four concepts in one crate

**23** flat `pub mod`s. Every module has a genuinely distinct secret and says so
— this is careful work, not a mess. The problem is that a reader arriving at
`lib.rs` sees a flat list with no shape. Four groups fall out:

```text
  vocabulary/   identity, declaration{,/field,/index}, recipe, coordinate,
                entity, resource            — validating newtypes, closed sets
  observe/      snapshot, fact, bootstrap, context, provenance
                                            — what a planner may know
  intent/       request, change, plan, transition, effect, edit, render,
                ownership                   — what is being asked for
  durable/      envelope, record, pending, conflict
                                            — what survives a crash
```

Start as **submodules**, not crates — mechanical, compiler-checked, and free to
undo. Promote a group only where the split enforces an edge that matters. On the
evidence exactly one would: `durable/` belongs with `jails-state` from 6.3,
because an envelope is a file format and the rest of the crate is values that
never touch a disk.

### 7.5 `jails-support` is four concepts and a lost-property office

Eight modules, four subjects: changing the filesystem (`apply`, `scratch`,
`lock`), running programs (`process`, `runner`), encoding (`codec`, `json`),
text surgery (`codemod`). Plus `Result`, `debug_cmd` and `CWD_LOCK` at the root.

- **`codemod` does not belong here.** Its subject is `# jails:<marker>` blocks
  in a *project's* `compose.yaml`. `lib.rs` says the boundary is "a module
  belongs at this layer only when it would still make sense in a tool that had
  never heard of Maven" — a marked-block splice keyed to jails' own comment
  syntax does not clear that bar. It belongs beside the files it edits, in
  `jails-project`.
- **`runner` should be named for what it is.** `process` runs a program with the
  user's terminal; `runner` runs one hermetically with a timeout and a byte cap.
  Different safety rules, near-identical names. Rename to `hermetic`.
- **`CWD_LOCK` is test infrastructure in production.** Deliberately not
  `#[cfg(test)]`, and the doc comment explains why correctly — a `#[cfg(test)]`
  item is invisible to dependent crates' tests. That reasoning is sound, so this
  is the weakest of the three; a tiny `jails-testkit` taken as a
  `[dev-dependency]` would say what it is instead of hiding it at the bottom of
  production.

What is left after those moves is coherent: **write, run, encode**.

### 7.6 `jails-tooling` is two crates wearing one name

17 modules, two unrelated jobs:

- **Drives a toolchain** (starts processes, may write): `run`, `testd`,
  `launcher`, `affected`, `reports`, `migrate`, `kafka`, `console`, `bench`,
  `lint`
- **Answers a question** (read-only by contract): `doctor`, `why`, `explain`,
  `commands`, `source`

`doctor` is read-only *by contract* and currently lives one `use` away from
`run::mvn`. Splitting into `jails-drive` and `jails-report` makes that contract
structural: the reporting crate simply could not depend on the crate that starts
things. **Lowest-priority crate split of the three** — do it after 6.3 and 6.5.

### 7.7 `jails-generate` still writes

The largest crate. It holds one job — *decide what Java to write* — and one
leftover: `generate/write.rs`, `add/database.rs`, `generate/cli.rs`,
`spring/durable.rs` and `generate/scaffold.rs` call `apply::*` directly, outside
any transaction. The planning half (`plan_for`, `plan_named`, `artifacts_for`)
is what the engine calls, is pure, and is the crate's real contribution. Getting
the write calls out is item 4's work, and once they are gone the crate is
honestly named for the first time.

---

## 8. Files and tests

### 8.1 Modules with a visible seam

The architecture board's own listing, today:

```
  644  jails-project/src/pom.rs             flavour detection | splice | unsplice
  642  jails-project/src/projection.rs
  633  jails-protocol/src/fact.rs
  631  src/new.rs
  624  jails-project/src/config.rs           jails.toml parse | LAYERS_IN_ORDER | writeback
  622  jails-project/src/inspect.rs
  619  src/main.rs
  614  jails-tooling/src/doctor/wiring.rs
```

(Production lines, comments and `#[cfg(test)]` blanked. Print it with
`cargo test --test architecture -- --nocapture --test-threads=1`.)

Two with named seams rather than just size:

- **`crates/jails-engine/src/route.rs`** (879 raw lines) is a module root plus
  helpers that are **two** subjects: *assembling a request* (`intent`, `spec`,
  `as_field_names`, `declaration`, `declared_capabilities`, `Asked`,
  `impl Request`) and *driving a commit* (`observed`, `commit`, `commit_set`,
  `prepare_set`, `reconciled`, `describe`, `relative_path`). Split into
  `route/request.rs` and `route/commit.rs`. Its ratchet is no longer red, so
  this is now a readability item rather than a blocked build.
- **`crates/jails-engine/src/route/maintenance.rs`** (~30 KB) is four unrelated
  commands sharing a file because none is big enough to justify one.
  "Maintenance" is a filing category, not a secret; one file per command.

For the binary:

```text
  src/main.rs      → cli/mod.rs (the clap definition) + cli/dispatch.rs + main.rs
  src/new.rs       → new/spring.rs (start.spring.io + offline)
                     new/plain.rs  (hand-written pom/App/AppTest)
                     new/seed.rs   (mise.toml, AGENTS.md, .gitkeep, git init)
                     new/gradle_project.rs (already split — keep)
                     new/publish.rs        (already split — keep)
```

### 8.2 Test files

```
  8,142  tests/cli.rs          175 #[test]
  3,581  tests/engine.rs
  1,816  tests/architecture.rs
```

Split into submodules of **one binary** — `tests/cli/{new,generate,capabilities,app,tooling}.rs`
— not into new binaries. Each extra integration-test binary is a full link of
the workspace and there are already nine.

`tests/architecture.rs` should split the same way for a stronger reason: it
mixes the ratchet board, the architecture rules, a small Rust blanking parser,
the crate-layer table, and that parser's own unit tests. Four files under
`tests/architecture/`, one binary.

### 8.3 The colocated-test convention has two exceptions

- **`crates/jails-generate/src/generate.rs`** carries **1,020** lines of tests
  belonging to its submodules (`domain`, `web`, `repository`, `cli`,
  `migration`, `scaffold`, `write`). `CLAUDE.md` documents why a mechanical
  extraction failed: the tests contain Java strings full of braces, so a
  brace-matching splitter cut them mid-identifier. Still true. Move them by hand
  a few at a time — `scratch()` is already hoisted so a submodule test mod can
  use it — or stop calling it a convention.
- **`jails-engine` has zero `#[cfg(test)]` modules.** It is the crate that
  assembles whole commands, and every assertion about it lives in
  `tests/engine.rs` and `tests/cli.rs`. Defensible (whole commands are
  integration-shaped) but it means `route.rs`'s shared helpers have no direct
  test at all. 7.1's split is the moment to add them.

### 8.4 `playground/` is 1,773 generated files in git

A fully generated Java project, committed, regenerated by hand, drifting
silently whenever a template changes with nothing failing. It has **grown** —
663 files when this was first written.

`tests/golden/` is 480 generated files and earns its place: it is a
byte-for-byte contract and a test reads it. `examples/` is the other correct
shape — five `.jails/app.toml` manifests and their markdown, with the generated
output *not* committed.

Either give `playground/` a test that regenerates and diffs it (making it a
second golden corpus, which is real value), or move it out of the repository.
Right now it is maintenance nobody is doing.

---

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

### 10.3 A test is choosing production names

`src/invoke.rs` opens by explaining it is named `invoke` rather than `dispatch`
because `jails-java` already has a `dispatch`, and
`no_two_crates_share_a_module_name` (`tests/architecture.rs:730`) identifies a
file by its first path component.

Identify a module by `(crate, path)` rather than by basename and the constraint
goes away. `cargo metadata` can supply the crate-dependency table the test
rebuilds from source text, which also removes the reason it needs a Rust parser.
Then rename `invoke` back to `dispatch`, which is what it is.

---

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
4. **One request, one field model** (§6.2, §6.3) — removes the parse-print-reparse
   round trip, and drops `too_many_arguments = "allow"`. **Next.**
5. **One table per kind** (§6.4).
6. **One transaction protocol** (§5, §7.7) — `new --app` becomes an ordinary V2
   transition and the remaining `apply::` calls move behind the executor. §4's
   gate reaches its target, honestly this time. It is at **46**, down from the
   56 it started at, entirely as a side effect of step 2 deleting V1 writers.
7. **Crate boundaries** (§7.3, §7.4, §7.5) and the file splits (§8).

The three new proof applications (§2.1, §2.2, §2.3) sequence against §9 rather
than against this list: each one lengthens the Failsafe tail, so §9's work is
what makes them affordable, and §2.4 is what decides whether they wait for it.

**If only four things get done:** §4 (the gate that reads green over unfinished
work), §7.2 (close the APIs and let the compiler find the dead code), §6.1 (the
cheapest large reduction in the repo), §6.3 (the deepest remaining seam between
the two engines). **Three of the four are done**; §6.3 is what is left of that
list.

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
