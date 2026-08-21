# ideas-kimi.md — synthesize the three plans, then fill the gaps

Research date: 2026-08-22. Written against this repo's source, `deps/` (84
checkouts, verified by reading them — not memory), `ideas/`
(minicom, eleven crawlers), and the three sibling documents.

---

## 0. What this file is

Three plans already exist, and they agree more than they differ:

| File | Thesis | Best ideas |
| --- | --- | --- |
| `ideas.md` | jails becomes a **product compiler**: one `ChangeSet` engine, provenance lock, CLI schema/event protocol, toolchain correctness, feature-first layout, blueprints | `ChangeSet` + `.jails/state`, `jails schema --json`, `jails bench dx`, JDK-27-default-is-wrong |
| `ideas-grok.md` | Rails is five loops; jails bought one (generation). Fix the other four: **navigation** (vim-rails projections), inner loop, `deps/` as `bundle open`, grow existing types, then the two verticals | `:A`/`:R`/`:E…`, `jails src`, `g field` + alter migrations, `add html` + `g spider`, `g webhook` / `g auth` / `add sse` / `g mailer`, the Lua-list pinning test |
| `ideas-opus.md` | Order by **loop latency**, not generator count: hot-swap dev, AOT cache, console with live beans, sub-second tests, then verticals and project templates | JDWP `redefineClasses` from Rust, `-XX:AOTCache`, `jails c` booting the context, `test file:line` + `--failed`, `new --template`, `g crawler` six-mistakes list, CRaC-adjacent startup work |

This file does three things the others do not:

1. **Merges them into one dependency-ordered spine** (§2), because three
   P0 lists is a coordination bug, not a roadmap.
2. **Adds the gaps all three missed** (§3, K1–K21): self-measurement,
   `.env`, reversing a database into a field spec, test-data factories,
   the Rails timestamps convention, a DB-backed job queue, Postgres
   full-text search, feature flags, ShedLock, the *extraction* half of
   crawling, a load harness, CI/Dockerfile generation, `.http` request
   files, offline `new`, seeds and migration status, a CRaC experiment,
   and `AGENTS.md` in generated projects.
3. **Re-plans the two verticals** — crawler and Intercom-shaped inbox —
   as compositions of everything above (§4), so "which pieces exist, in
   which document" stops being something you hold in your head.

Everything here obeys the constraints in `CLAUDE.md` and README's "Not
yet": no plugin system, no Gradle, no ORM, no runtime support jar, no
Lombok, no preview features, templates stay real `.java` files, doctor
stays read-only. Ideas that would break those are in §7 with reasons.

---

## 1. The honest arithmetic of "1000×"

You will not get 1000× on a Tuesday. The multiple is a product of four
factors, and only two of them are generators:

```
productivity = (time saved per inner loop × loops/day)
             + (stuck-hours removed × stuck events/week)
             + (setup time saved per new vertical × verticals/year)
             + (test-authoring time saved × tests written/day)
```

The first three docs cover factors 1–3 well and factor 4 barely at all.
Realistic numbers for this machine, stock Spring+nvim as baseline:

| Loop | Stock | After all four docs | Events |
| --- | --- | --- | --- |
| Save → app healthy with change | 15–40 s | 1–3 s (opus A1+A2) | 100–300/day |
| Save → test result | 40–90 s (`clean verify` culture) | 2–8 s (opus A4, grok §4) | 50–150/day |
| File → related file (test, adapter, migration) | 10–30 s | <1 s (grok §3) | 50–200/day |
| "What does this Boot 4 error mean" | 10–20 min | 10 s (`why`, K19) | 2–5/day |
| "How does this library actually behave" | browser, wrong version | `jails src` (grok §5) | 3–10/day |
| Test data setup per new test | 3–8 min of constructor noise | 10 s (K4 factories) | 5–20/day |
| New vertical (crawler, inbox) to walking skeleton | 1–3 days | <1 hour (§4) | 4–10/year |
| Stuck on env/secrets/CI/Docker ceremony | 1–2 h per occurrence | ~0 (K2, K12, K13) | weekly |

Compounded over the first month of a real project, against *stock*
tooling with no jails at all, that is honestly 100–1000× on the tasks
jails touches — and about 3–10× against jails as it stands today, which
is the comparison that should actually drive sequencing.

**The measurement gap.** All four documents (this one included) make
claims like the table above, and today nothing measures them. That is
why K1 (`jails recap`) is first in the gap list: not because it is the
biggest win, but because it tells you which of the other twenty ideas
is actually costing you time this week.

---

## 2. One spine, not three plans

Merged across all four documents, ordered by dependency. An idea lands
*after* the things it needs, not the things it resembles.

**Phase 0 — correctness floor** (days)
- Toolchain resolution; default `new` never targets an unreleased JDK
  (`ideas.md` bet 4). Note the deliberate tension: `CLAUDE.md` pins
  `TARGET_RELEASE = 27` with mise-symlinked EA builds on purpose. Resolve
  by making the default the installed GA and `--java 27-ea` the explicit
  spell, or document the mise requirement in `doctor` — pick one, don't
  leave it implied.
- The Lua-list pinning test (grok P0): grep `jails.nvim` KINDS/
  CAPABILITIES/OPTIONS against `ArtifactKind`/`Capability::label()` in CI.
  Half a day, and it prevents the silent completion rot that already
  happened once (`toxiproxy`).

**Phase 1 — the inner loop** (weeks 1–3; independent of everything else)
- `jails test <file>:<line>` and `--failed` (opus A4.4/A4.2 — the
  highest value-per-day ratio in any document).
- `jails test --watch` over main+test trees (grok §4).
- AOT class-data cache on every fork (opus A2): pure win, ~1 day.
- K19: every Maven failure gets a `why` pass automatically (§3.19).
- `jails dev`: poll + devtools first, JDWP hotswap when measured
  (opus A1, `ideas.md` bet 1). **Do not build CRaC (K17) before
  `bench dx` proves restart is still the bottleneck after AOT.**
- Testcontainers `.withReuse(true)` + doctor check (opus A4.3).

**Phase 2 — the editor** (weeks 2–4; parallel with Phase 1)
- `about --json` v2 + projections `:A`/`:R`/`:E…` (grok §3).
- `jails src` + `gf` into `deps/` (grok §5).
- Pickers from `routes --json`/`beans --json`, palette from
  `jails schema --json` (opus C1–C2, `ideas.md` bet 3); the hand-copied
  Lua lists die here, guarded by the Phase 0 test.
- K14: generated `requests/*.http` (§3.14).

**Phase 3 — safe mutation** (weeks 3–6; everything structural depends on it)
- `ChangeSet` + provenance + atomic apply (`ideas.md` bet 2).
- Then, riding on provenance: `g field` + alter migrations (grok §6),
  `jails upgrade`/drift (opus D2), K4 factories, K5 timestamps.

**Phase 4 — poke the system** (weeks 4–6)
- `jails c` booting a context, `jails c -e` runner (opus A3).
- `jails scratch` (opus B4). `jails sql` sugar (K21).
- K2: `.env` loading + `.env.example` (§3.2) — before `g webhook` and
  `g auth`, which are the first capabilities with real secrets.

**Phase 5 — vertical backbone** (weeks 5–9)
- **K6 `add queue` first.** It is the shared dependency of the inbox
  (mailer delivery, webhook retry) and the crawler (durable frontier),
  and neither grok nor opus named it. Details in §3.6.
- Then the grok slices: `g webhook`, `g auth`, `add sse`, `g mailer`.
- Crawler: `add html` + `g spider` (grok §7), then K10 `g extractor`.
- K7 `g search`, K8 `add flags`, K9 `add shedlock` as needed by real
  features, not before.

**Phase 6 — ship it** (weeks 8+)
- K12 `add ci`, K13 `add docker`, project templates (opus B2),
  `jails upgrade` (opus D2 / `ideas.md` packs).

Parallelism: Phases 1, 2, 4 are mutually independent and can interleave
by energy level. Phase 5's `remove` correctness genuinely needs Phase
3's provenance — a queue capability that can't cleanly uninstall is the
kind of thing `remove` exists to be honest about.

---

## 3. The gaps (K1–K21)

Each entry: command surface, what it writes/touches, tests, effort, and
why it earns its place. All new capabilities/kinds imply the three
mechanical edits (`Capability`/`ArtifactKind` variant, README entry,
`jails.nvim` list — the last guarded by the Phase 0 test).

### K1. `jails recap` — measure your own DX first

```bash
jails recap            # this week
jails recap --today
jails recap --json
```

Reads shell history **read-only** (`~/.bash_history` with
`HISTTIMEFORMAT` epoch lines; `~/.zsh_history` extended
`: epoch:duration;cmd`), buckets `jails`/`mvn`/`mvnd` invocations
(clean-verify, test, run, generate, add), and multiplies each bucket by
measured medians from `.jails/benchmarks/` when present (`ideas.md`'s
`bench dx`), else built-in priors stated as priors.

Output:

```text
this week, waiting time (estimated):
  41 × clean verify        ~41 min   18 avoidable → jails test <Name>
  63 × mvn test            ~26 min
   9 × jails run (cold)    ~ 4 min
top suggestion: <leader>Jt on the current test would have saved ~20 min
```

- Why first: every other idea in every doc claims a time win. This is
  the only one that *ranks the rest with your data*. It also makes the
  "1000×" claim honest over time — you can watch the baseline move.
- Constraints: no telemetry, no network, history never leaves the
  machine. Non-jails commands are counted and discarded in memory.
- ~200 lines, one new file (`src/recap.rs`), unit tests over fixture
  history files. Effort: 1 day.

### K2. `.env` — secrets stop living in `application.properties`

Today a webhook secret or API key has two homes: a committed properties
file, or your shell rc. Both are wrong, and `g webhook`/`g auth` (grok
§8) make it urgent — they are the first capabilities whose entire point
is a secret.

- `process.rs` loads `<root>/.env` (`KEY=value`, `#` comments, optional
  double quotes) into the child environment before spawning
  `run`/`dev`/`console`/`db`/`test`; the real environment wins. Spring
  reads env vars natively (`APP_WEBHOOK_SECRET` ↔
  `app.webhook.secret` relaxed binding), so generated code changes: it
  already says "secret from a property", and the property now has
  somewhere to come from.
- **Secret-rendering integration:** values loaded from `.env` are added
  to the secret set for `--debug` rendering *by origin*, not by name —
  the `ALWAYS_SECRET` name-list backstop stays, but `FOO` from `.env` is
  masked even when FOO doesn't look secret. This plugs directly into the
  existing `secret_env` machinery.
- `new`/`new-cli` gitignore `.env` at creation time (jails owns the
  `.gitignore` then). Capabilities with secrets append to
  **`.env.example`** (never `.env`) as marked blocks, same splice
  discipline as `pom.rs`/`compose.rs`; `remove` takes the block out.
- Tests: parse rules, precedence, debug-rendering masks an
  env-loaded value. Effort: 1 day.

### K3. `jails inspect db` — Django's `inspectdb`, jails-shaped

Django's best onboarding trick is pointing `manage.py inspectdb` at an
existing database and getting models back. Crawler and inbox prototypes
constantly start from an existing dump or a hand-rolled table; today you
hand-transcribe it into a field spec.

```bash
jails inspect db notes
# paste-ready:
jails g scaffold Note id:uuid@pk title:string! body:text? created_at:instant

jails inspect db notes --write Note   # writes domain/Note.java directly
```

- Implementation: `psql` against the compose dev DB via the `console.rs`
  path (host/creds already resolved there), querying
  `information_schema.columns` + `table_constraints`/`key_column_usage`.
  Read-only; never the scratch DB.
- The reverse type map is **new code in `sql.rs`**, and it must be an
  explicit table, not an inversion-by-convention: `timestamptz → instant`,
  `text → string`, `numeric → decimal` (lossy: precision is not
  recoverable into the jails closed set — document, and emit `decimal`
  with a Javadoc note). Unit-test both directions over the full builtin
  table: for every jails type, `reverse(forward(t))` is `t` or a named
  lossy case. PK columns get `@pk`; a column named `id` is *not*
  auto-promoted beyond what the constraint says — same honesty rule as
  `fields_from_record`.
- `--from-csv <file>` variant sniffs a header row and emits
  `string`-typed specs (compose with `add csv`): crawlers dump CSVs.
- Effort: 1–2 days, mostly the reverse map + its round-trip tests.

### K4. `jails g factory <Name>` — the missing half of test speed

After generation, the biggest remaining Java tax is **test data setup**:
every hand-written test `new`s a six-component record, and the day a
component is added, forty call sites break. FactoryBot is half of why
Rails testing feels fast; the type-safe Java equivalent is mechanical
and jails already owns the hard part (`sample_value`).

```bash
jails g factory Note
```

writes `<Name>Factory` in the test tree (`testkit` layer):

```java
/** Test data for {@link Note}. Defaults are valid; override what the test is about. */
public final class NoteFactory {
    public static Builder valid() { ... }  // every component seeded from sample_value
    public static final class Builder {
        public Builder title(String v) { ... }
        public Builder author(User v) { ... }
        public Note build() { ... }
    }
}
```

- Rules inherit the field-spec semantics exactly: enums get
  `Currency.values()[0]`, `?` components get `Optional.empty()`,
  collections default empty, `!` fields get non-blank samples.
- A component jails cannot sample (a project type it doesn't know): the
  builder field starts `null` and `build()` throws naming the component
  — the factory analogue of the `@Disabled`-with-a-name rule. Never a
  guessed value: a silently-wrong default in a factory poisons every
  test that uses it, which is worse than forty broken constructors.
- Reads the record off disk (`fields_from_record` — exists), so it works
  on hand-written records too.
- Effort: 1–2 days. Golden-file the factory; add a tier-3 compile test
  with one project-type component proving the throw path compiles.

### K5. `--timestamps` — the Rails convention, on by default where it matters

Rails migrations carry `created_at`/`updated_at` unless you decline.
jails makes you type `createdAt:instant` by hand today (every README
example does), and then `updatedAt` is a second, never-quite-maintained
column.

- `g scaffold` (and `g repo` when it writes a migration) **append
  `createdAt:instant updatedAt:instant` to the field spec by default**,
  visible in the record, DDL, DTOs and fixtures; `--no-timestamps`
  declines. Off for bare `record`/`value` — a domain type is not a row.
- The adapter sets both on insert and `updated_at` on update; both ride
  the single column list, so they cannot drift — that machinery already
  exists and is the reason this is cheap.
- Golden files change wholesale; that is the correct price.
- Effort: 1 day, after Phase 3 (it touches the same generator surfaces
  `g field` does).

### K6. `add queue` — the backbone both verticals are missing

Rails 8 moved the job queue *into the database* (Solid Queue, the
default) because requiring Redis to send an email later was the wrong
shape for small apps. jails has `add redis` and no queue at all. Every
product-shaped thing in `ideas/` needs "do this later, retry if it
fails": mailer delivery, webhook re-delivery, crawl frontier
persistence, digest jobs. Hand-rolled, each one re-derives the same five
mistakes.

```bash
jails add queue                 # requires add db
jails g worker WelcomeEmail --queue emails
jails queue list                # counts by state, per queue
jails queue failed              # dead letters with last_error
jails queue retry               # failed -> ready
```

- **Migration** (`jails_jobs`): `id uuid pk default gen_random_uuid()`,
  `queue text`, `payload jsonb`, `run_at timestamptz`,
  `attempts int default 0`, `max_attempts int default 5`,
  `state text default 'ready'`, `locked_at`, `locked_by`,
  `last_error text`, timestamps. Partial index
  `(queue, run_at) where state = 'ready'`.
- **Claim query** — the one thing everyone gets wrong, generated once,
  correct: an `UPDATE … WHERE id = (SELECT id … FOR UPDATE SKIP LOCKED)
  RETURNING *`, so two workers never take the same job. That is the
  capability's reason to exist: the failure it prevents (double-send) is
  silent.
- **Backoff**: `run_at = now() + attempts² × interval '1 minute'`;
  `attempts >= max_attempts` → `state = 'failed'` with `last_error`.
  Never dropped, never swallowed — the same rule `g event` encodes for
  Kafka, for the same reason.
- **Handlers**: `g worker <Name> --queue <q>` writes a `JobHandler`
  (`String queue(); void handle(JsonNode)`), a bean per queue.
  `JobWorker` runs one virtual thread per distinct handler queue, poll
  interval from a property. **A generated `JobWiringTest` fails the
  context when two handlers claim one queue or a handler's queue has no
  enqueuer in the project** — the "missing `@Component` means the list
  is silently short" failure `g strategy` already teaches.
- **No new infrastructure**: it runs on the postgres `add db` already
  put there. Redis stays a cache/presence capability, not a queue
  dependency. That is the Solid Queue lesson applied.
- `jails queue …` shells through the `console.rs` psql path, mirroring
  `jails kafka`. Laravel Horizon is a dashboard; the CLI covers the
  honest half of it.
- Tests (tier 3, Testcontainers + Awaitility — splice Awaitility if
  absent): enqueue→handled; poison handler → retried → `failed` with
  `last_error`; two competing workers claim disjoint jobs.
- Effort: 3–4 days. **Sequence it before `g mailer` and before any
  persistent crawler frontier** — both are one `enqueue` call once this
  exists.

### K7. `jails g search <Name> --fields title,body` — FTS without a platform

Inbox wants conversation search; the crawler wants page search.
Elasticsearch is a platform decision; Postgres full-text search is 30
lines of visible SQL — exactly jails' cut (raw SQL, no ORM, no new
service).

- Per-entity, like `g event` vs `add kafka`: `add db` is the
  precondition, the entity is the argument.
- Migration: a `generated always as (to_tsvector('english',
  coalesce(title,'') || ' ' || coalesce(body,''))) stored` column plus a
  GIN index — a *generated* column, so there is no trigger to rot.
  `--lang` overrides `'english'`.
- `<Name>Search` adapter: `where search_tsv @@ plainto_tsquery(:lang, ?)`
  with `ts_rank` ordering and a `ts_headline` highlight method. Port in
  `app/` per the layout.
- `--fields` entries are validated against the record's components
  before anything is written — same rule as `--index` validation, for
  the same reason (a typo surfacing at `flyway migrate` on someone
  else's machine is the failure this feature exists to remove).
- IT over Testcontainers seeded from the testkit fixtures the scaffold
  already writes: two rows, one matches, one doesn't, rank order pinned.
- Effort: 1–2 days.

### K8. `add flags` — OpenFeature with a dev provider you can read

`openfeature-java-sdk` is **already cloned in `deps/`** — the research
happened; the capability didn't. Verified against the checkout:
`FeatureProvider` (`dev.openfeature.sdk`) is a small interface
(`getMetadata()` + typed `get*Evaluation(key, default, ctx)`).

- Splice `dev.openfeature:sdk`, generate `Flags` with names declared
  once (`public static final String INBOX_V2 = "inbox.v2";` — the
  `AppMetrics` convention from `add observability`, because flag names
  as string literals at call sites drift identically to meter names).
- Generate `PropertyFlagProvider implements FeatureProvider` reading
  `features.*` properties with env override (`FEATURES_INBOX_V2=true`).
  The provider is a class written into the project — the jails idiom —
  not a sidecar. When a real vendor arrives (flagd, LaunchDarkly), the
  OpenFeature seam means a config change, not a rewrite; that is the
  whole reason to use the SDK API rather than a `boolean isOn(String)`.
- Test: two `@SpringBootTest` slices (or one with a swapped provider)
  asserting both states of one flag — a flag that can't be flipped in a
  test is a flag nobody trusts.
- `doctor` WARN (not FAIL) when source references `Flags.` but no
  provider is configured — the silent default-false trap.
- Effort: 1–2 days. P2 — build it the first time a feature ships
  half-finished, not before.

### K9. `add shedlock` — the scheduled job that runs twice

The failure: two instances (or dev + a teammate's dev) both fire the
02:00 `@Scheduled` job; customers get two digest emails; nothing logs an
error. That is precisely the bar every existing capability meets ("the
failure is silent"), and `g job` already exists to compound it.

- Pin `net.javacrumbs.shedlock:shedlock-spring` +
  `shedlock-provider-jdbc-template` (both currently **absent** from
  `deps/` — add to `deps.tsv`, clone, write the template against the
  checkout per the house rule).
- `shedlock` table migration (`name` pk, `lock_until`, `locked_at`,
  `locked_by`), `@EnableSchedulerLock`, and `g job` emits
  `@SchedulerLock(name = "...")` when the capability is present.
  Lock-at-most-for from a property, because the default (run-until-done)
  turns a hung job into a lock that never releases — the same shape of
  trap as a TTL-less Redis write.
- Requires `add db`. IT: two schedulers, one lock row, assert exactly
  one execution (Awaitility).
- Effort: 1 day.

### K10. `jails g extractor <Name>` — the other half of crawling

grok's `g spider` covers traversal (frontier, politeness, dedup). Real
crawls then need **structured extraction**: crawl4ai's whole pitch and
webmagic's `@ExtractBy` pattern are selector→struct mapping. With
records, it is mechanical:

```bash
jails g extractor Product \
  --field 'name:css:h1.product-title' \
  --field 'price:css:.price' \
  --field 'link:url:a.product-link'
```

- Grammar is a **closed set of three prefixes**: `css:` (element text),
  `attr:` (`selector@attribute`), `url:` (`absUrl("href")` — the
  resolved absolute URL, because a relative `href` is the default source
  of junk links). Unknown prefix → error listing the three, same rule as
  field-type case and `@markers` everywhere else.
- Writes the `<Name>` record, `<Name>Extractor` (`List<Name>
  extract(Document)` — jsoup, so it depends on grok's `add html`), and a
  test parsing a checked-in fixture HTML under
  `src/test/resources/fixtures/` (the directory every project already
  gets).
- v1 stores strings; a `:decimal`-style coercion suffix is the obvious
  v2, deferred until a real crawl needs it — a guessed coercion table is
  how `sql.rs` complexity leaks into a second place.
- Effort: 1–2 days after `add html` exists.

### K11. `jails g load <path>` — the load test nobody writes

JMeter/Gatling setup is why take-homes and side projects never get
load-tested. The honest 80% is ~60 lines of Java: virtual threads +
`HdrHistogram` (**already in `deps/`**).

```bash
jails g load /notes --rate 200 --duration 30s
```

- Writes `<Name>Load` with a `main`:
  `Executors.newVirtualThreadPerTaskExecutor()` (final since 21, no
  preview — structured concurrency stays forbidden per CLAUDE.md), a
  `Recorder`, and a closing report: count, p50/p90/p99/max, throughput.
  Rate and duration are args/properties, never constants (the `g job`
  rule).
- **The one open design question is invocation.** `jails run` finds
  "the file with `static void main`" — a second main creates ambiguity.
  v1 spell: document `jails mvn -- compile exec:java
  -Dexec.mainClass=...Load`; if that proves annoying in practice, give
  `run` an explicit `--main <Class>` flag rather than making it guess.
  Do not teach `run` to pick a main by heuristics.
- Deliberately **not a JUnit test** — it must never run in `verify`.
  That distinction (and why) goes in the generated Javadoc.
- Effort: 1 day. P2/P3.

### K12. `add ci` — the workflow every repo hand-writes

Every jails project ends up with the same GitHub Actions file: JDK
matching the pom's release, Maven cache, `mvn -B clean verify` (the
`jails check` contract), Docker available for Testcontainers. Getting
`setup-java`'s distribution/version/cache right is 30 minutes, every
time, plus the day you learn a runner's default JDK rejects
`--release 27`.

- Writes `.github/workflows/ci.yml` (release read from the pom via
  `pom.rs`, temurin, `cache: maven`, one `verify` job, a commented-out
  `jails doctor` step with the cargo-install line) and
  `.github/dependabot.yml` (`maven` + `github-actions`, weekly).
- Marked files, idempotent, `remove ci` takes them out. No Java
  dependencies, no network at generation time.
- EA-JDK honesty: when the pom targets an EA release, the workflow uses
  `oracle-actions/setup-java` EA builds or a `JAVA_HOME` download step —
  and says which in a comment, because the temurin line silently won't
  have it. This is the CI twin of the Phase 0 toolchain fix.
- Effort: 0.5–1 day.

### K13. `add docker` + `jails image` — past "runs on my machine"

Neither doc covers shipping. Verified against `deps/spring-boot`:
`spring-boot-jarmode-tools` exists with `ExtractLayersCommand`, so the
Boot 4 layered-jar dance is `java -Djarmode=tools -jar app.jar extract
--layers --destination application` (the `layertools` spelling is gone;
this is why templates are written against checkouts).

- `Dockerfile`: stage 1 `maven:<mvn>-eclipse-temurin-<release>` with a
  BuildKit cache mount on `.m2`, `mvn -B package -DskipTests`, layer
  extraction. Stage 2 runtime — **and here the EA toolchain bites**:
  there is no GA `eclipse-temurin:27-jre` image while 27 is EA. The
  honest default for EA targets is `jlink` in stage 1 producing a
  minimal runtime (`jdeps`-derived module list, `zipfs`/`jdk.crypto.ec`
  gotchas handled once, in the template), which also covers `new-cli`
  projects, which buildpacks never will. GA targets get the plain
  `-jre` base. Non-root user, layered `COPY` in
  dependencies/loader/snapshot/application order, `EXPOSE 8080`.
  `HEALTHCHECK` omitted by default (no curl in slim images) with the
  comment saying so — a healthcheck that always fails is worse than
  none.
- `.dockerignore`: `target/`, `.git/`, `.jails/` (once provenance lands).
- `jails image` builds and tags from the pom (`artifact:version`);
  `jails image --run` joins the compose network so the app reaches
  `postgres`/`kafka` by service name.
- Alternative honestly documented: `mvn spring-boot:build-image`
  (buildpacks) is fine for Spring-only, no-cache-control cases; the
  Dockerfile exists for CI caching, non-root control, and CLI projects.
- Effort: 2 days including the jlink path.

### K14. `requests/*.http` — run the endpoint you just made

The third everyday loop, after test and dev, is "fire the request".
Today it's a hand-typed curl in terminal scrollback.

- `g scaffold` gains a side artifact (or `jails g requests <Name>` for
  later): `requests/note.http` — `@host = http://localhost:8080`, one
  block per operation, sample JSON bodies built from `sample_value` (the
  machinery that already fills fixture rows and factory defaults —
  third reuse).
- The `.http` format is tool-agnostic: IntelliJ HTTP Client, VS Code
  REST Client, and `kulala.nvim` all read it. The user's nvim config
  currently has no REST-client plugin (checked `init.lua`) — kulala is
  the one plugin worth adding, and the files are useful without it.
- Once `g field` (grok §6) exists, it updates the request body the same
  way it updates the fixture — same provenance rule.
- Effort: 0.5 day.

### K15. `jails new --offline` — the Initializr is a network away

`new` wraps start.spring.io; `new-cli` is hand-written and works
anywhere. The middle case — a *Spring* skeleton with no network — is
missing, and the fixture for it already exists: `write_spring_fixture`
in `tests/common/mod.rs` is exactly a hand-written, version-pinned
Spring pom + Application.

- Vendored in the binary via `include_str!` (the template.rs idiom):
  minimal pom (parent pinned to the Boot version the golden suite
  tests), `Application.java`, `ApplicationTests.java`, `.gitignore`,
  jspecify dependency, fixtures `.gitkeep` — byte-identical to what the
  integration tests already compile.
- Explicit flag; when start.spring.io fails, the error suggests
  `--offline` rather than silently falling back (explicit-network
  principle). `about`/`doctor` note when the vendored Boot version is
  behind, same staleness honesty as the `deps/` alignment check.
- Effort: 1 day — the asset is built; this is packaging it.

### K16. `migrate --status` + `db --seed` — the two `db:` tasks Rails users reach for

- `jails migrate --status`: `select version, description, success,
  installed_on from flyway_schema_history order by installed_rank`
  against the **dev** database (the `console.rs` psql path; not the
  scratch database — that one is for `--check`), plus the on-disk files
  not yet in history listed as pending. `rails db:migrate:status`.
  Read-only, doctor-safe. Effort: 0.5 day.
- `jails db --seed` applies a conventional `db/seed/dev.sql` to the dev
  database only — refuses when there's no compose DB, never touches the
  scratch database, says which database it wrote to. Plain visible SQL,
  no Java seed DSL. `g seed` isn't a kind; the file is the thing, so
  `add db` can lay down an empty one next to `db/migration`. Effort:
  0.5 day.
- Why: an inbox demo wants 3 users and 20 messages; a crawler wants a
  dozen seed URLs. Today that's a psql heredoc lost in scrollback.

### K17. CRaC dev restore — the endgame, explicitly last

opus's A1 (JDWP hotswap) + A2 (AOT cache) take restart from ~10 s to
~2–3 s. The remaining floor is context construction itself. Project
CRaC's answer: checkpoint a warmed JVM, restore in ~100–300 ms. Spring
Framework has first-class lifecycle support — verified in `deps/`:
`spring-context`'s `DefaultLifecycleProcessor` references `org.crac`,
so beans get checkpoint/restore callbacks and connection pools close
cleanly at checkpoint time.

- `jails dev --crac` (flag, never default): boot to healthy →
  `jcmd <pid> JDK.checkpoint` → on source change: restore the checkpoint
  **and then apply opus's JDWP `redefineClasses` for the changed
  bytecode** — the composition is the trick: restore gives you the warm
  context back, redefine gives it your edit.
- Honest constraints, all checkable by `doctor`: needs a CRaC-enabled
  JDK (Azul Zulu CRaC / OpenJDK `crac` builds — stock JDK 27 does not
  ship it); file locks and held ports across checkpoint are real;
  Testcontainers interactions need a spike.
- **Do not build this until `jails bench dx` shows restart is still the
  bottleneck after A1+A2.** It is listed so the sequence is visible, not
  to inflate the plan. Spike: 2–3 days, outcome uncertain.

### K18. `AGENTS.md` in every generated project

You develop with coding agents some of the time. Today an agent dropped
into a jails project rediscovers Maven from scratch and reaches for
`mvn verify` as the loop — the exact habit Phase 1 exists to kill.

- `new`/`new-cli` write a 30-line `AGENTS.md` beside the README: the
  project is jails-managed; use `jails test <Name>` not `mvn test`;
  `jails g record` for types; `jails check` (`mvn clean verify`) is the
  gate and *why* (leftover classes); `jails doctor` before debugging
  environment issues; conventions (records, no Lombok, no ORM, the
  layer table, field-spec syntax pointer to README).
- Static complement to grok's `jails context --json` (dynamic half).
  Claude Code, Cursor, codex all pick it up with zero wiring.
- Effort: 0.5 day; one template, two golden files.

### K19. `why` on every Maven failure, not just `run --watch`

`run.rs` pipes output through `why::FATAL_MARKERS` only in watched
runs. `test`, `build`, `check`, `fmt` failures print raw Maven and
stop. Extend the same treatment to all of them: non-zero exit → the
tail is run through `why::explain` and the top rule's explanation +
`fix:` line prints after the raw output (`--plain` opts out).

- Piping costs the child its TTY — the colour flags `run_watched`
  already passes (`-Dstyle.color=always` etc.) are the known fix, reuse
  them.
- This multiplies the value of the existing rule table and of every
  future mined rule (the `~/.codex/sessions` grep procedure in
  CLAUDE.md), and it composes with grok/opus's `why --fix` and the
  javac-rule family rather than duplicating them.
- Effort: 1 day.

### K20. `--module` — the day the inbox and the crawler share a repo

`about` already walks the Maven reactor and reports the active module;
`generate`/`add`/`test` ignore that and operate from the nearest
`pom.xml`. The first real product (an inbox app plus a crawler worker,
say) wants both in one repo as modules — `boot-multi-runners` in this
machine's own project list is evidence the shape already occurs.

- `--module <name>` on `generate`/`destroy`/`add`/`test` resolves the
  project root to the named submodule (validated against `about`'s
  module list; unknown name → error listing real ones — the closed-set
  rule applied to paths).
- No multi-module *generation* (`new` stays single-module; a
  `g module <name>` is a separate, later idea, deliberately not
  specified here).
- Effort: 1 day. P3 — build it when the second module exists, not
  before; this section is the spec for that day.

### K21. Small wins (each ≤ 1 day, batch them)

| Idea | Note |
| --- | --- |
| `jails sql 'select …'` | Sugar over `db -- -c` (exists); one alias, real keystrokes |
| `--notify` global flag | `notify-send`/`osascript` when `test`/`check`/`build` finish — you alt-tab during 40 s gates |
| `jails test --slowest` | Parse surefire XML, print the 10 slowest tests — the cargo-nextest habit; pairs with opus A4 |
| `jails outdated` | `versions:display-dependency-updates` rendered as a table + `why`-style risk notes for majors; twice-a-month value |
| `jails config get/set layout.<layer>` | CLI edit of the closed-set manifest instead of hand-TOML; the splice machinery already exists |
| `--pretend` renders unified diffs | Folds into `ideas.md` bet 2's ChangeSet renderer; listed so it isn't lost |
| `jails mutate <Class>` (P3) | PIT via `add coverage`; JaCoCo is in `deps/`, PIT needs pinning + a clone; mutation testing finds the assertions factories (K4) make cheap to write |

---

## 4. The two verticals, composed

The point of the shelf in `ideas/` is these two products. Here is each
as one command sequence, with every piece attributed to where it's
specified. **[exists]** = shipped today.

### 4.1 Polite same-domain crawler (the Monzo shape, then beyond)

```bash
jails new crawl --deps web                      # [exists]  (or new-cli for pure CLI)
jails add db testkit format                     # [exists]
jails add html                                  # [grok §7]   jsoup + Fetcher + WireMock
jails g spider Monzo --same-domain --delay-ms 200   # [grok §7]   Frontier/Fetcher/Parser/Store
jails g extractor Product \                     # [K10]       selector → record
    --field 'name:css:h1' --field 'price:css:.price'
jails add queue                                 # [K6]        durable frontier, retry
jails g search Page --fields title,body         # [K7]        what you crawled, searchable
jails db --seed                                 # [K16]       seed URLs as data
jails g load /                                  # [K11]       politeness under concurrency, proven
jails add ci docker                             # [K12, K13]
```

Acceptance gates, in order: the WireMock diamond IT (A→B, A→C, B→C, C
visited once — the Monzo brief's real question), `shouldVisit` unit
tests (`community.monzo.com` rejected under `--same-domain`), then a
live run capped at `--max-pages`. The eleven crawlers in `ideas/` were
the research; colly's callback API is the DX model and
crawler4j/webmagic/Nutch/StormCrawler/Heritrix are the warnings (dead,
pre-Boot-4, or platforms). Nothing gets wrapped; the four types get
generated (grok §7) and extraction gets generated (K10).

### 4.2 Intercom-shaped support inbox (minicom, grown up)

```bash
jails new inbox --deps web                      # [exists]
jails add db redis security api testkit format  # [exists]
jails g auth Messenger                          # [grok §8.2] JWT mint/verify, needs security
jails g scaffold User id:uuid@pk email:string! name:string!           # [exists]
jails g scaffold Conversation id:uuid@pk userId:uuid@index subject:string!   # [exists]
jails g scaffold Message id:uuid@pk conversationId:uuid@index \
    authorId:uuid body:text! direction:string!                        # [exists]
jails g factory User ; jails g factory Message  # [K4]        test data stops hurting
jails add sse                                   # [grok §8.3 / opus B1] agent inbox fanout
jails g webhook Intercom --header X-Hub-Signature   # [grok §8.1] raw-body HMAC
jails g mailer ConversationDigest               # [grok §8.4]
jails add queue                                 # [K6]        mail delivery + webhook retry
jails g search Conversation --fields subject    # [K7]
jails add flags                                 # [K8]        inbox.v2 rollout
jails add shedlock                              # [K9]        the digest job runs once
jails g page Conversation                       # [opus B1]   Thymeleaf+htmx, guarded
jails add ci docker                             # [K12, K13]
```

The widget stays static JS in `src/main/resources/static/` — jails does
not own frontends (grok §8.5, agreed). The Rails reference
(`ideas/minicom-rails`) is 4 models and one controller because Rails
hides the rest; the sequence above is the honest Java equivalent where
the hidden parts (auth, delivery, retry, search) are generated but
*visible*, which is the entire jails bet.

Sequencing note: **K6 (`add queue`) before `g mailer` and before any
durable frontier.** A mailer that sends synchronously in the request is
the tutorial version; a queue that retries is the product version; the
difference is one capability.

---

## 5. What to steal, by ecosystem (research summary)

| Source | The feature | jails analogue | Status |
| --- | --- | --- | --- |
| Rails | `console` / `runner` / `dbconsole` | `jails c` with live context, `c -e`, `jails db` | opus A3; db **[exists]** |
| Rails | `g migration AddXToY` alter | `g field` + alter migration | grok §6 |
| Rails | `db:migrate:status`, `db:seed` | `migrate --status`, `db --seed` | **K16** |
| Rails 8 | Solid Queue: DB-backed jobs, no Redis required | `add queue` (SKIP LOCKED) | **K6** |
| Rails | `app:update` | `jails upgrade` / drift diff | opus D2, ideas.md |
| Rails | FactoryBot | `g factory` | **K4** |
| Rails | migration timestamps by default | `--timestamps` on scaffold | **K5** |
| Rails | Zeitwerk reload | dev loop + hotswap | opus A1 |
| Django | `inspectdb` (DB → models) | `jails inspect db` (DB → field spec) | **K3** |
| Django | `makemigrations` autodetect | **deliberately not** — needs model-owned schema, i.e. ORM thinking; `g field` is the honest half | §7 |
| Django | `shell_plus` auto-imports | console startup snippet | grok §9 |
| Laravel | `artisan make:*` | `jails g` | **[exists]** |
| Laravel | Horizon (queue dashboard) | `jails queue` CLI (the honest half) | **K6** |
| Laravel | Pennant (feature flags) | `add flags` via OpenFeature | **K8** |
| Laravel | Sail (docker dev env) | compose **[exists]** + `add docker` | **K13** |
| Laravel | Telescope (debug UI) | `why` + structured events cover the terminal half; a UI is a non-goal | §7 |
| Phoenix | `phx.gen.auth` | `g auth` (JWT mint/verify) | grok §8.2 |
| Phoenix | LiveView | SSE + `g page` as the honest slice | opus B1 |
| Phoenix | `mix test --stale/--failed` | `jails test --failed` | opus A4.2 |
| Symfony | `debug:router` / `debug:container` | `jails routes` / `jails beans` | **[exists]** |
| .NET | `dotnet watch` hot reload | `jails dev` + JDWP redefine | opus A1 |
| Quarkus | continuous testing | `jails test --watch` | grok §4 |
| Quarkus | dev services | compose auto-start | **[exists]** |
| Cargo | nextest (runner quality) | `test --failed`, `--slowest`, JUnit XML parsing | opus A4, **K21** |
| Go/colly | collector callbacks | `Parser` callbacks in `g spider` | grok §7 |
| crawl4ai/webmagic | selector → struct extraction | `g extractor` | **K10** |
| Stripe/Intercom | signed webhooks | `g webhook` | grok §8.1 |
| Spring Roo | round-trip codegen via AspectJ ITDs nobody could read | **warning**: real-Java templates only, no hidden weaving — jails already complies; never regress | §7 |
| JHipster | JPA + frontend monolith-gen | **warning**: opposite of this tool | §7 |

---

## 6. Merged priority table (all four documents, one sequence)

| Order | Item | Doc | Effort | Depends on |
| --- | --- | --- | --- | --- |
| 1 | Lua-list pinning test | grok P0 | 0.5d | — |
| 2 | Toolchain resolution (GA default, `-ea` explicit) | ideas.md | 2–3d | — |
| 3 | `jails test file:line` + `--failed` + `--slowest` | opus A4, K21 | 1d | — |
| 4 | AOT cache on forks | opus A2 | 1d | — |
| 5 | K19 `why` on every mvn failure | here | 1d | — |
| 6 | `test --watch` (main+test trees) | grok §4 | 1–2d | 3 |
| 7 | K1 `jails recap` | here | 1d | bench dx useful but not required |
| 8 | projections `:A`/`:R`/`:E…` + `about --json` v2 | grok §3 | 2–3d | — |
| 9 | `jails src` + `gf` into deps | grok §5 | 1–2d | — |
| 10 | `jails dev` (poll+devtools; JDWP when measured) | opus A1, ideas.md | 2–3w | 4 |
| 11 | `jails c` live context + `c -e` | opus A3 | 2–3d | — |
| 12 | K2 `.env` + `.env.example` | here | 1d | — |
| 13 | CLI schema/event protocol; Lua lists deleted | ideas.md | 1–2w | 1 |
| 14 | ChangeSet + provenance + atomic apply | ideas.md | 3–5w | — |
| 15 | `g field` + alter migrations | grok §6 | 3d | 14 |
| 16 | K4 `g factory`, K5 `--timestamps` | here | 2–3d | 14 |
| 17 | K14 `requests/*.http` | here | 0.5d | — |
| 18 | K15 `new --offline` | here | 1d | — |
| 19 | K6 `add queue` + `jails queue` CLI | here | 3–4d | 14 (for `remove`) |
| 20 | `g webhook`, `g auth` | grok §8.1–8.2 | 3d | 12 |
| 21 | `add html` + `g spider` | grok §7 | 3–4d | — |
| 22 | K10 `g extractor` | here | 1–2d | 21 |
| 23 | `add sse`, `g mailer` (mailer via queue) | grok §8.3–8.4 | 2–3d | 19 |
| 24 | K7 `g search` | here | 1–2d | — |
| 25 | K16 `migrate --status`, `db --seed` | here | 1d | — |
| 26 | K18 `AGENTS.md` in new projects | here | 0.5d | — |
| 27 | `new --template` (closed command lists) | opus B2 | 2d | 19–23 worth templating |
| 28 | K12 `add ci`, K13 `add docker` | here | 2–3d | — |
| 29 | K8 `add flags`, K9 `add shedlock` | here | 2–3d | first real need |
| 30 | `jails upgrade` / drift | opus D2 | 2d+ | 14 |
| 31 | K11 `g load`, K20 `--module`, K21 remainder | here | ~3d | real need |
| 32 | K17 CRaC spike | here | 2–3d | bench dx proves need |

Items 1–9 are roughly two working weeks and change the feel of every
subsequent hour; the crawler and inbox become *evenings* at item 27.

---

## 7. Anti-goals (union of all four documents, plus new ones)

| Temptation | Why not |
| --- | --- |
| Plugin system / `jails recipe intercom` | README "Not yet"; templates (opus B2) are closed command lists, data-only |
| Gradle / mill as the project build | "Not yet"; `--module` (K20) covers the actual growth pain |
| ORM, JPA, `JpaRepository` | SQL stays derived and visible; K3 reverses *into* a spec, not into entities |
| Django-style `makemigrations` autodetect | Requires models to own the schema — ORM thinking through the back door; `g field` is the explicit, reviewable half |
| A `jails-support` runtime jar | ActiveSupport lock-in; capabilities write classes *into* the project (`KeyValueStore`, `PropertyFlagProvider` precedent) |
| Lombok | Editor tax on modern JDKs; records exist |
| Preview features / string templates | CLAUDE.md; virtual threads are the concurrency story |
| Wrapping crawler4j/webmagic/Nutch/StormCrawler | Dead or platforms; generate four types + an extractor |
| Wrapping spider/crawl4ai as a sidecar | Second runtime; `add html` is Java |
| UI dashboards (Telescope/Horizon/DevUI analogues) | `why`, `jails queue`, structured events are the terminal-honest half; a web UI is a product, not a capability |
| Redis required for the queue | K6's whole point (Solid Queue lesson); Redis stays cache/presence |
| Elasticsearch for v1 search | K7 covers 90% in 30 lines of visible SQL |
| An LLM inside jails core | Deterministic, golden-tested, destroyable output is the product (all three docs agree) |
| Auto-fallback to vendored `new` on network failure | Explicit `--offline` flag; silent mode switches are how "works on my machine" happens |
| Making `jails check` incremental | Leftover-class bug is real; the fast loop is `jails test`, loudly documented |
| `jails crawl <url>` as a jails subcommand | opus B3 said it: jails scaffolds crawlers, it isn't one |

---

## 8. Research log (what was actually looked at)

**This repo:** `README.md`, `CLAUDE.md`, `src/main.rs` (command enum —
no CI/docker/env/queue surface exists today), `src/add/tooling.rs`
(`add http` is a JDK-httpserver slice, not a client), `src/console.rs`
(psql/sqlite3/jshell paths — K3/K6/K16 reuse), `src/run.rs` (`why`
markers only in `run_watched`; 750 ms poll), `src/config.rs`
(closed-set TOML, `[layout]`+`[project]` only), `src/template.rs`
(`{{name}}` + `include_str!` idiom — K15 reuses it),
`src/generate/migration.rs` (`g cases` exists — markdown → JUnit),
`src/new.rs` (writes `.gitignore` at creation — K2's hook point),
`tests/common/mod.rs` (`write_spring_fixture` — K15's asset),
`jails.nvim/lua/jails/init.lua` (hand-copied `KINDS`/`CAPABILITIES`/
`SUBCOMMANDS`/`OPTIONS` confirmed; the Phase 0 pinning test is
justified by inspection), `spring.md` §11 (package-by-feature),
`backend.md` (virtual threads position), `ideas.md`, `ideas-grok.md`,
`ideas-opus.md` (read in full; this file deliberately avoids
re-specifying their ideas).

**`deps/` verified by reading, not memory:**
`spring-boot/loader/spring-boot-jarmode-tools/…/ExtractLayersCommand.java`
(K13's layer extraction command is real); `spring-framework`
`DefaultLifecycleProcessor` references `org.crac` (K17's Spring support
is real); `openfeature-java-sdk` `FeatureProvider.java` (K8's interface
shape: `getMetadata()` + typed evaluations — a generated property-file
provider is ~100 lines). Absence checks: **jsoup, nimbus-jose-jwt,
shedlock, crawler-commons are NOT in `deps/`** — grok §8.2's auth and
K9 each need a pin + clone via `deps/update.sh` before writing
templates. Present and reusable: `HdrHistogram` (K11), `wiremock`
(K10/grok §7), `caffeine`, `resilience4j`, `spring-modulith`,
`springdoc-openapi`, `awaitility` (K6 tests).

**`ideas/`:** `minicom-public` (foo/bar static sites + `POST /foo` /
`POST /bar`; README's framework menu is rails/spring/node/django — note
its `spring` option is **Gradle**, outside jails' scope per README),
`minicom-rails` (the whole product is `User has_many Message`, one
endpoints controller, 5 routes — §4.2 is the honest superset),
`monzo-code-challenge/CHALLENGE.md` (same-domain, one subdomain,
"no scrapy/colly… we care about structure, concurrency, tests" — the
acceptance criteria `g spider` is designed against), `monzo-crawler`
(Boot service: QueueService/CacheService/MetricService — what a crawler
looks like when you grow it without a frontier abstraction),
`monzo-crawler2` (Gradle, engine/observer split).

**User environment:** `my-dotfiles` nvim `init.lua` — jdtls via
nvim-jdtls, fzf-lua; **no REST-client plugin** (K14 recommends kulala as
the single addition; files work without it). Project list on this
machine includes `boot-multi-runners` — multi-module (K20) is a real
local shape, not speculation.

**External (from knowledge, flagged as such):** Rails 8 Solid Queue
(rails/solid_queue, separate gem, not in `deps/rails`); Django
`inspectdb`/`shell_plus`; Laravel Horizon/Pennant/Sail/Telescope;
Phoenix `phx.gen.auth`; Symfony `debug:*`; .NET `dotnet watch`; Quarkus
continuous testing/dev services; cargo-nextest; Project CRaC JDK
availability (Azul/OpenJDK crac builds). Each is cited where used;
none of these claims change if the details drift — the jails-side
designs stand on local checkouts.

---

## 9. One paragraph, if you read nothing else

The three existing docs already say the true thing: the 1000× is not
more generators, it is killing the wait between edits, the hunt between
files, and the stuck-hours on Boot-4-moved-it-again — then spending the
saved budget on the two products you actually want. What they missed is
smaller and concrete: measure yourself first (`jails recap`), give
secrets a home (`.env`), reverse existing databases into field specs
(`inspect db`), generate the test-data builders that make Java tests
cheap (`g factory`), put timestamps where Rails puts them, and build
the one capability both verticals silently assume — a DB-backed job
queue (`add queue`) — before the mailer, the webhook retry, and the
crawler frontier each reinvent it badly. Everything else is sequencing,
and §6 is the sequence.
