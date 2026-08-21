# ideas-grok.md

Research notes on making Java + Spring + Neovim feel like Rails, using
jails as the lever. Written 2026-08-21 against the jails tree, `deps/`,
`ideas/`, this machine's Neovim config, and a handful of upstream tools.

This is a working document, not a roadmap commitment. Every idea names
the command, the files it would touch, the test that would pin it, and
what it must not become. Ideas that would violate README "Not yet"
(Gradle, runtime bean/route view, plugin system) or the no-ORM rule are
marked as such and left on the floor.

---

## 0. How to read this

The 1000x is not "more generators." Rails is fast because five loops
compound, and Java + Neovim currently lose on four of them after the
first `jails g scaffold`. jails already bought the fifth (a scaffold
that *runs*). The remaining work is to make the next hour, the next
file, and the next failure as cheap as the first generate.

A honest multiplier, against *stock* Java/Spring/Neovim with no jails:

| Loop | Stock | After jails today | After the P0 in this file |
| --- | --- | --- | --- |
| First REST resource that actually starts | 2–4 hours | ~2 minutes (`g scaffold` + `run`) | same |
| Jump Note.java → test → repo → migration → fixture | 10–30s of search | none (no vim-rails) | <1s (`:A` / `:R`) |
| "What does this Spring failure mean?" | 20 min of googling | `jails why` | `why` + jump to the Spring source line in `deps/` |
| Re-run the test you just edited | 30–90s (`mvn verify` culture) | `jails test Name` exists; `jails check` is still `clean verify` | 2–8s, watch mode |
| Open `RestClient` / `@ServiceConnection` source | jdtls download, or a browser | `deps/` is cloned and unused by the editor | `gf` / `jails src` |
| Polite same-domain crawler | a day of queue/fetch/parse | nothing | `add html` + `g spider` |
| Inbox + identity + inbound webhook | a week of ceremony | scaffold + `add security` + `add redis` | plus `g webhook`, `g auth`, `add sse` |

Compound those and you get the "1000x vs stock" claim on the tasks you
actually do. You do not get 1000x on designing the product. jails cannot
think. It can stop you fighting the language, the framework, Maven, and
the editor.

---

## 1. Diagnosis: why Rails feels fast here and Java does not

Measured against *this* setup, not a generic blog post.

### 1.1 What Rails actually sells (and jails already copied)

Rails productivity is not ActiveRecord. It is:

1. **A closed convention.** You never decide where a file goes. jails
   has this: `generate::layout` + `jails.toml [layout]`.
2. **Generators that produce a running slice, with destroy.** jails has
   this for REST (`g scaffold`), CLI (`g command` registers itself),
   Kafka (`g event`), HTTP clients (`g client`).
3. **A CLI that is the project.** `rails c`, `dbconsole`, `routes`,
   `test`, `runner`. jails has `console`, `db`, `routes`, `beans`,
   `test`, `run --watch`, `doctor`, `why`.
4. **An editor that knows the convention.** This is `vim-rails`: `:A`,
   `:R`, `:Econtroller`, `gf` on a partial. **jails.nvim does not have
   this.** It is a terminal wrapper around the binary, by design
   (README: "deliberately reimplements none of its project-generation
   logic"). That design is right for generation and wrong for
   navigation. Navigation is not generation.
5. **A REPL that reloads.** `jails console` is `jshell` plus the
   classpath. It is not a Spring-booted `reload!`. The module comment
   in `src/console.rs` already admits this. Living with it is fine;
   pretending it is `rails c` is how people bounce off.

### 1.2 Where Java + Neovim actually lose, on this machine

These are not theoretical. They are in `~/code/my-dotfiles` and in
jails itself.

**The inner loop is paying for CI.** `jails check` is `mvn clean
verify`. That is correct for the gate (`CLAUDE.md`: leftover `*Test`
classes in `target/` still run). It is catastrophic as a habit. The
Neovim Java ftplugin already has `<leader>jf` (this class) and
`<leader>jm` (this method) via a raw `mvn test -Dtest=`. Those should
be the default muscle memory; `check` is what you run before push.
Today the README leads with `check` as "format + tests" and a new user
internalises the slow path.

**You compile twice on every save.** `ftplugin/java.lua` attaches
nvim-jdtls *and* a `BufWritePost` `javac_lint` that shells `mvn`.
jdtls is ECJ; javac_lint is javac `-Xlint:all`. Both are load-bearing
(they catch different warnings) and together they make "save" feel
like "please wait." Rails save is free.

**jdtls cold start is a solved problem you already solved, then tax
again.** CDS archive, conditional Lombok agent, per-project workspace
keyed off `.session` for gym throwaways — that is excellent. Lombok is
already treated as a tax (CDS cannot dump with a javaagent on JDK 26).
jails generates records, not Lombok. Keep it that way. A "productivity"
idea that adds Lombok would make the editor slower, which is the
opposite of the goal.

**`<leader>jg` is IntelliJ's Generate menu (getters/setters).**
`<leader>Jg` is `:Jails g`. On a record project the first one is a
trap: it offers to generate the ceremony records exist to delete. The
keymap collision is a daily papercut.

**There is no projection.** From `NoteController.java` you cannot
`:A` to `NoteControllerTest`, `:R` to `NoteService`, or `:Emigration`
to `V00N__create_notes.sql`. vim-rails users do this hundreds of times
a day. jails.nvim can complete test class names for `:Jails test` and
type names for `:Jails rename`, then stops. That is the largest
editor-shaped hole.

**`deps/` is a private JDK/Spring/Jackson source tree that the editor
does not know about.** 80+ upstream checkouts, versions pinned in
`deps/deps.tsv`, cloned blobless, used to *write* jails templates.
When you are *using* a generated project, `gf` on `JdbcClient` still
goes through jdtls source-jar download (slow, sometimes wrong version)
or a browser. Rails has `bundle open`. You built the equivalent and
never wired it to a key.

**Java remains verbose after generation.** Records, compact
constructors, JSpecify, import normalisation, `package-info.java` —
jails already removed the 2015 ceremony. What remains is structural:
a field added by hand does not update the JDBC adapter, the migration,
the DTO, the fixture, or the test. Rails' most-used generator after
`scaffold` is `rails g migration AddBodyToNotes body:text`. jails can
write a *new* table from a field spec and cannot grow an existing one.

**Spring Boot restart is seconds, not milliseconds.** `jails run
--watch` recompiles on a 750ms poll of `src/main/java` mtime and lets
devtools restart the JVM. It does not watch tests. It does not watch
`src/main/resources`. It does not use jdtls's in-memory compile. A
Rails request after a model edit is ~50ms. A Boot restart after a
service edit is 2–8s on a small app, worse with Testcontainers
already up. You will not make Boot into Rails. You can make the *test*
loop that fast, and stop restarting the app to find out if a mapper
compiles.

### 1.3 What "I want to build Intercom and crawlers" actually demands

`ideas/` is a research shelf, not a product spec. Read as a closed
set of *shapes*, not as "wrap these libraries."

**Crawlers** (Monzo challenge, crawler4j, webmagic, colly, spider,
crawl4ai, StormCrawler, Nutch, Heritrix):

The Monzo brief is the right size: same-domain BFS, print URL +
outbound links, production structure, no scrapy/colly. crawler4j is
unmaintained since ~2018 — do not wrap it. Nutch is Hadoop.
StormCrawler is Storm. Heritrix is a WARC archival engine. spider and
crawl4ai are excellent and are Rust/Python; wrapping them from Java
is a plugin. The Colly API is the DX to copy:

```go
c.OnHTML("a[href]", func(e *colly.HTMLElement) { e.Request.Visit(e.Attr("href")) })
c.Visit("https://monzo.com/")
```

That is a `Collector` + callbacks + politeness. In jails terms it is
`add html` (jsoup + `HttpClient`) plus `g spider Monzo --same-domain`,
with virtual threads doing what Go's goroutines do, WireMock ITs
doing what Colly's `httpmock` does, and a `command` so `jails run`
crawls. Do not generate a framework. Generate the four types every
crawler is: Frontier, Fetcher, Parser, Store.

**Intercom** (`ideas/minicom-public`, `minicom-rails`):

The public interview stub is two static sites plus `POST /foo` and
`POST /bar`. The real product is: identity (JWT, Intercom moved off
HMAC user_hash), conversations, realtime fanout, inbound webhooks
with signature verification, an agent inbox, a third-party widget.
jails should not generate "an Intercom." Phoenix's `phx.gen.auth` is
the model: one generator that emits a *complete, tested identity
slice*, composed with existing `g scaffold Conversation` and two new
closed capabilities (`g webhook`, `add sse`). The widget JS is a
static file you will write once; it is not a jails kind.

---

## 2. Constraints that stay

Do not "improve" these away. They are why jails is small enough to
trust.

- **No plugin system.** A crawler capability is an enum variant, like
  `kafka`. A team-specific generator is a documented sequence of
  existing commands, not a hook.
- **No Gradle.** Maven only. mill/jbang can be *tools jails shells
  to* for scripts; they must not become the project build.
- **No ORM.** `@Entity` / `JpaRepository` stay forbidden. SQL is
  derived from the field spec, visible, migrated by Flyway.
- **No jails-runtime library.** Rails' secret weapon is
  ActiveSupport, and it is also the lock-in. Generated code depends
  on JDK + Spring + the one library the capability added (jsoup,
  nimbus-jose-jwt, …). If you need a helper in every project, it is
  a capability that writes a class into *the project*, like
  `KeyValueStore` already does.
- **No Lombok.** Records + compact constructors. Lombok makes jdtls
  slower on this JDK.
- **No preview features in generated Java.** Structured concurrency
  is still preview on JDK 27. Virtual threads are final — use those.
- **Templates stay `.java` files with `{{name}}` substitution**, not
  a template engine, not doubled braces in Rust strings.
- **`jails.toml` stays a closed set.** New layout keys only when a
  new layer exists. New capability names are `Capability::label()`.
- **Doctor stays read-only.** A crawl-time check that hits the
  network is not a doctor check.
- **Tests stay three-tier.** A spider IT that needs the network is
  tier 3 and gated. Golden files pin the Java.

Spring CLI (spring-attic/spring-cli) was archived 2026-05-14. JHipster
generates JPA + a frontend. OpenRewrite is a migrator, not a
generator. None of those are the shape to copy. jails already won the
"Spring CLI" niche by being small and opinionated. Copy vim-rails,
Phoenix `phx.gen.auth`, Colly's collector API, and Rails' *alter*
migration — not JHipster.

---

## 3. Idea 1 — jails.nvim becomes vim-rails (P0, highest leverage)

### Problem

Generation is solved. Navigation is not. You spend more time finding
the JDBC adapter than you spent generating it.

### Design

Keep generation in the Rust binary. Teach the plugin the *layout*,
which is already a closed set.

**Projections, derived from `jails.toml` + `jails about --json`.**

Extend `about --json` (schema_version bump to 2) with:

```json
{
  "schema_version": 2,
  "layout": {
    "domain": "domain",
    "app": "app",
    "adapters": "adapters",
    "web": "web",
    "..." : "..."
  },
  "base_package": "com.example.demo",
  "capabilities": ["db", "kafka"],
  "layers": { "java_root": "src/main/java", "test_root": "src/test/java" }
}
```

The plugin caches this per `project_root()` (invalidate on
`jails.toml` / `pom.xml` write). It does not parse Java.

**Commands (vim-rails names, jails nouns):**

| Command | From `NoteController.java` |
| --- | --- |
| `:A` | `NoteControllerTest.java` (or `NoteTest` from a record, `JdbcNoteRepositoryIT` from a JDBC adapter) |
| `:R` | next related: Controller → Service → Repository port → JDBC adapter → record → migration → fixture. Cycle. |
| `:Edomain Note` | `domain/Note.java` |
| `:Erepo Note` | `app/NoteRepository.java` |
| `:Eadapter Note` | `adapters/JdbcNoteRepository.java` (and `InMemoryNoteRepository` as `:A` from there) |
| `:Eweb Note` | `web/NoteController.java` |
| `:Emigration notes` | latest `V*notes*.sql` or picker |
| `:Etest Note` | companion test |
| `:Efixture notes` | `src/test/resources/fixtures/notes.yml` (or whatever `add testkit` writes) |

Bang (`:Edomain Foo!`) does **not** invent a template in Lua. It
calls `jails g record Foo` (or the kind that owns that layer) and
opens the created file — same path the plugin already uses for
`:Jails g`.

**`gf` / `gd` fallback.** If jdtls has not attached yet (the common
cold-start case), map `gf` on a PascalCase ident to the same
projection. Once jdtls attaches, leave `gd` to LSP. This is the
single biggest "Neovim Java is slow" fix that is not "wait for jdtls."

**Alternate for the field spec's owned types.** On `author:User` inside
a generate command line or a record component, `gf` opens `User.java`.

**Do not** implement `:A` by filename suffix only (`X.java` ↔
`XTest.java`). That misses `JdbcNoteRepository` ↔
`JdbcNoteRepositoryIT`, `Note` ↔ `V003__create_notes.sql`, and
`NoteController` ↔ `NoteService`. Use a table keyed by kind, which
`about --json` can also emit later as `"kind_of_file": ...` if you
want to stop guessing from suffixes. Until then, suffix + layer
directory is enough: a file under `adapters/` named `JdbcFoo*`
alternates with `JdbcFoo*IT` then `InMemoryFoo*` then `app/FooRepository`.

### Tests

- Lua: a fixture tree matching `tests/golden/scaffold-spring` and
  assertions on resolved paths.
- Rust: `about --json` schema_version 2 includes `layout` and
  `base_package`. An integration test pins the keys to
  `config::LAYERS_IN_ORDER` so a new layer cannot ship undocumented.

### Anti-goals

- No second generator in Lua.
- No dependence on jdtls for navigation.
- Do not auto-write `projections.json` into the *Java* project
  (that is editor config leaking into the repo). Keep it in the
  plugin, driven by `about`.

### Why this is 10–50x

vim-rails users jump files without thinking. You currently `:FzfLua
files` or wait for jdtls. That tax is paid 50–200 times a day.

---

## 4. Idea 2 — Split the inner loop from the gate (P0)

### Problem

`jails check` = `mvn clean verify` is the right *gate* and the wrong
*loop*. `jails test Note` already exists and is the right loop. The
product does not make that obvious, and nothing watches tests.

### Design

**Keep `check` exactly as it is.** Do not "optimize" it back to bare
`verify`. The leftover-class bug is real.

**Make the fast path the default in the editor and the README.**

- README: lead the everyday workflow with `jails test [Name]`, then
  `jails run --watch`, then `jails check` before push. One paragraph.
- jails.nvim: `<leader>Jt` already runs `Jails test` (whole suite,
  no clean). Change it to **current buffer's test** when the buffer
  is a `*Test`/`*IT`, otherwise the type's companion test (the
  `:A` mapping). Whole-suite stays as `<leader>JT` or `:Jails test`.
- Unify with ftplugin `<leader>jf` / `<leader>jm`. Today those call
  `mvn` directly and bypass `jails test`'s mvnd/mvnw resolution.
  They should call `jails test`, which is the thing `about` reports.
  One Maven resolver. The CLAUDE.md gotcha about `about` vs `run`
  drifting already happened once.

**`jails test --watch` (and `jails test Note --watch`).**

Poll `src/main/java` *and* `src/test/java` (run.rs currently watches
only main, and only for `run --watch`). On change, re-run the same
Surefire filter. No `clean`. No Failsafe unless the filter is an
`*IT`. No Spotless. Print the first failure with file:line so the
plugin can `cgetexpr` it into the quickfix.

Implementation: same 750ms mtime loop as `run --watch`, different
command. Do not pull in watchexec as a dependency; the existing poll
is 40 lines and matches the rest of the crate.

**Stop double-compiling on save, or make it opt-in.**

`javac_lint` on every `BufWritePost` is the right *check* and the
wrong *default while typing*. Propose: run javac_lint on
`<leader>jl` (already mapped) and on `BufWritePost` only if
`vim.g.jails_javac_on_save` is set. jdtls diagnostics cover the
inner loop; javac `-Xlint` covers the gate. You can have both
without paying both on every `w`.

**`jails test` should not start compose.** It already doesn't. Keep
it that way. ITs that need a broker are Failsafe + Testcontainers,
not compose.

### Tests

- `jails test --watch` in the mocked-mvn tier: touch a file, assert
  the Surefire invocation happened twice with the same `-Dtest=`.
- A unit test that `check` still contains `clean verify` (pin the
  gate so a future "speedup" cannot reintroduce leftover classes).

### Anti-goals

- Do not make `check` incremental.
- Do not add Gradle or mill as the project build. mill is faster;
  it is also a second build tool, which is the Gradle deferral
  under another name.
- Do not have the watch loop call `spotless:apply`. Format is a
  gate concern (`add format` already runs it once).

---

## 5. Idea 3 — `jails src`: deps/ as `bundle open` (P0)

### Problem

You already clone Spring, JDK, Jackson, Kafka, Testcontainers, jsoup
(not yet), etc. under `deps/`. Templates are written against those
checkouts. When *using* a generated project, none of that is a
keystroke away. jdtls source download is slow and often a different
version than Boot's BOM.

### Design

**`jails src <Type-or-file> [--web]`**

Resolution order:

1. A type in the current project (reuse `java.rs` / filename stem,
   same as `rename`).
2. A type in `deps/*` whose simple name or FQCN matches. Search
   `**/*.java` with a filename stem first (`RestClient.java`), then
   ripgrep `^public (class|interface|record|enum) RestClient\b` if
   needed.
3. A Spring annotation: `@ServiceConnection` → the file in
   `deps/spring-boot` that declares it.

Print the path and, without `--`, open it (`$EDITOR` or stdout for
the plugin). `--web` prints the javadoc.io URL as a fallback when
deps are not cloned.

**Neovim:** `gf` on a type that is not in the project jumps to
`jails src`. `<leader>Js` prompts. A location list of all matches
when the simple name is ambiguous (`Logger` in slf4j vs JUL).

**`jails src --sync`** is not a new command: it is `deps/update.sh`,
already tracked. Document it in README next to `jails src`. Doctor
gains a WARN (not FAIL) if `deps/spring-boot` is absent *and* the
current module is Spring — only when `JAILS_DEPS` is set, so a
machine that never cloned deps is not nagged. Default off.

**Version alignment.** Optional later: read Boot's BOM from the
project pom and warn when `deps/spring-framework` is on a different
minor. Do not auto-checkout a different SHA from doctor (doctor is
read-only, and changing deps mid-debug is hostile). A `jails src
--doctor` that only prints would be fine.

**`jails why` cites a path.** When a rule is about
`DeadLetterPublishingRecoverer` defaulting to `-dlt`, the fix line
can include `jails src DeadLetterPublishingRecoverer`. That is how
deps stops being a jails-developer trick and becomes a
jails-user trick.

### Tests

- Unit: given a fake `deps/spring-framework/.../RestClient.java`,
  `src RestClient` resolves it.
- Do not clone real deps in CI. The fake tree is enough.

### Anti-goals

- Do not index deps into the Java project's jdtls workspace (that
  would make jdtls *slower*).
- Do not vendor Spring source into generated projects.

### Why this is 10–20x on Spring questions

The failure mode of this stack is "the blog post is about Boot 2."
You already solved that for *writing* jails (`Every template was
written against deps/`). Expose it for *using* jails.

---

## 6. Idea 4 — Grow an existing type (`g field`, alter migrations) (P1)

### Problem

`g scaffold Note id:uuid title:string!` is the first afternoon.
The rest of the week is `ALTER TABLE notes ADD body text`. Today
that is: edit the record, edit the compact constructor, edit the
test, edit both adapters, edit the DTOs, edit the fixture, write a
migration by hand. That is the boilerplate you hate, and generators
that only *create* cannot touch it.

### Design

**`jails g field Note body:text! [--migration]`**

- Reads `Note.java` via `fields_from_record` (already exists).
- Refuses if `body` is already a component.
- Rewrites the record (and its test) with the new component in
  declaration order, appended.
- If a JDBC adapter exists, derived from the same column list it
  already uses, regenerate the select/insert/bind/mapper **only
  if the adapter is still marked as jails-owned** (a header
  comment or a hash, same idea as capability property blocks).
  If the user edited it, print the new snippets and do not
  overwrite — same honesty as `remove` on hand-tuned properties.
- If `db/migration` exists, write `V00N__add_body_to_notes.sql`
  with `alter table ... add column ...` from `sql.rs`. Never
  edit an old migration (forward-only, already the rule).
- If a fixture exists, add the column to both rows with
  `sample_value`, or `@Disabled` the fixture test if a sample
  is impossible (same rule as record tests).
- DTOs: if `NoteRequest`/`NoteResponse` exist and still match
  the old field list, rewrite them; otherwise say so.

**`jails g field Note --remove body`** is tempting and dangerous.
Skip it in v1. Destroying a column is a migration you write by
hand because of data. Adding is the 95% case.

**Owned types as FKs.** `author:User` currently passes through as
a Java type with no import (same package) and no SQL mapping
worth using. Define one closed rule:

- If `User.java` is a record with a single `@pk` (or a component
  named `id`), persist `author_id` with that component's SQL
  type and a `references users (id)` in the *new table*
  migration. Do not invent ON DELETE behaviour; omit it, document
  the default.
- Unknown shape → name it in the JDBC adapter Javadoc, same as
  today for unmapped types.

This is the Rails `belongs_to :author` that does not require an
ORM: it is a column + a constructor argument of type `User` that
the service loads. The adapter stores the id. Do not generate
lazy loading.

### Tests

- Golden: a scaffolded `Note`, then `g field Note body:text!`,
  snapshot the record, adapter, DTO, new migration, fixture.
- Refuse to overwrite a hand-edited adapter (hash mismatch).
- `User` with `id:uuid@pk` + `Note author:User` produces
  `author_id uuid references users (id)`.

### Anti-goals

- No `g field` that rewrites `V001__create_notes.sql`.
- No arbitrary SQL in the field spec (`@check(...)` stays
  rejected).
- No generated JPA associations.

---

## 7. Idea 5 — Crawler: `add html` + `g spider` (P1)

### Problem

`ideas/` is full of crawlers because that is a thing you want to
build, and Java has no Colly. crawler4j is dead. Nutch/StormCrawler
are platforms. jsoup is a parser, not a crawler. `java.net.http.HttpClient`
is a client, not a frontier.

### Design (two closed pieces, like `add kafka` + `g event`)

**`jails add html`** — the library slice, topic-agnostic:

- Dependency: `org.jsoup:jsoup` (pin a version, add to `deps/deps.tsv`,
  clone it; same discipline as Jackson 3).
- A small `Html` wrapper in `adapters/` (or a new `html` layer —
  prefer `adapters` to avoid a layout key): `Document parse(String)`,
  `List<URI> links(Document, URI base)`, `String text(Document)`.
  No crawling in this class.
- A `Fetcher` over `HttpClient` with: timeout, User-Agent required
  (constructor arg, no default that lies), `HEAD`/`GET`, charset
  from Content-Type.
- Tests: jsoup against a string; Fetcher against the JDK HTTP
  server or WireMock. WireMock is already in `deps/`; `add html`
  should splice `wiremock-standalone` (test scope) if not present.
  Do not add Playwright here.

**`jails g spider <Name> [--same-domain] [--delay-ms 200]`**

Writes four types plus a command (plain Maven) or a job (Spring):

- `<Name>Frontier` — visited set + queue. In-memory for the
  generated default (so it runs without `add db`). Document that
  a JDBC frontier is a follow-up, not a surprise `@Repository`.
- `<Name>Parser` — `shouldVisit(URI, URI from)`, `onPage(Page)`.
  Default `shouldVisit`: same host if `--same-domain`, skip
  `mailto:`, `#`, obvious static suffixes. Override is the point.
- `<Name>Crawler` — virtual-thread executor, politeness delay
  *per host* (not global), robots.txt fetch once per host,
  bounded queue, hard cap (`--max-pages`, default 1000 in
  properties, not a constant).
- `<Name>Page` record: `uri`, `status`, `fetchedAt`, `links`.
- A `g command Crawl` (new-cli) or `g job` (Spring) that starts
  it. Output streams are arguments, same as existing commands, so
  the test is in-process.
- `robots.txt`: fetch and honour `Disallow`. If the fetch fails,
  crawl proceeds but logs. Do not invent a robots parser; use a
  tiny one or jsoup+manual. Pin the behaviour with a WireMock
  fixture.
- IT: WireMock serves a three-page diamond (A→B, A→C, B→C).
  Assert C is visited once. This is the test the Monzo brief is
  really asking for (dedup + same-domain). `@Disabled` if WireMock
  is not on the classpath; `g spider` therefore depends on
  `add html`.

**Do not wrap crawler4j, webmagic, spider, crawl4ai.** Generate
*your* types. The Colly-shaped API is the `Parser` callbacks, not a
third-party collector.

**Optional later, not v1:** `add browser` with Playwright Java for
JS-heavy pages. That image is huge and will dominate compose.
Crawl4AI's markdown output is an `add html` method
(`Html.markdown(Document)`) you can add when you have a real
caller, not before. Do not take a Python dependency.

### Tests

- Golden for the four types.
- WireMock diamond IT in the real-toolchain tier, gated.
- `shouldVisit` unit tests: subdomain rejected when `--same-domain`,
  `community.monzo.com` vs `monzo.com` (the brief's own example).

### Why this matches jails

`add kafka` does not know a topic name; `g event` does.
`add html` does not know a seed URL; `g spider` does. Same cut.

---

## 8. Idea 6 — Intercom-shaped slices, not an Intercom generator (P1–P2)

Phoenix `mix phx.gen.auth` is the best generator in any framework
in 2026: it emits a complete identity slice, tested, without an
ORM-shaped user model leaking everywhere. Rails' `authentication`
generator caught up later. jails should steal *that* granularity,
not `rails g scaffold` for "Conversation" (you already have that).

### 8.1 `jails g webhook <Name> [--header X-Hub-Signature-256] [--secret-prop app.webhooks.secret]`

Every inbound webhook is the same object:

- A record `NamePayload` (body; fields optional, default a
  `JsonNode` payload if none given — `add json` required).
- A `@RestController` that: reads the raw body as bytes (signature
  is over the raw bytes, not the re-serialised JSON), verifies
  HMAC-SHA256, returns 401 on mismatch, 202 on accept, then
  dispatches to a port `NameHandler.handle(payload)`.
- Timing-safe compare (`MessageDigest.isEqual`).
- Secret from a property, never a literal. `add security` is not
  required (the endpoint is unauthenticated on purpose; the
  signature *is* the auth).
- IT: good signature, bad signature, missing header. Use
  `MockMvcTester`. No `@Disabled`.

Intercom still signs with `X-Hub-Signature`. Stripe uses
`Stripe-Signature` with a timestamp. Support `--header` and a
strategy enum of two: `hmac_sha256_hex` and `stripe_v1`. Unknown
strategy is an error (closed set). Do not take a "pass through the
algorithm name" string.

### 8.2 `jails g auth <Name>` (Phoenix-shaped, JWT)

Intercom's messenger identity is a server-minted JWT with `user_id`.
A lot of APIs are the same shape.

- `spring-boot-starter-security` already comes from `add security`.
  `g auth` *requires* `add security` (like `g event` requires
  `add kafka` in spirit — refuse with the fix line).
- Nimbus JOSE (Boot 4's usual JWT story) or
  `spring-boot-starter-oauth2-resource-server` if you are a resource
  server. For a *minting* service (the Intercom case: you issue the
  token the widget sends), mint with Nimbus, HS256, secret from
  property, `user_id` required claim. Expiry required. No "forever"
  token.
- `NameTokenService.issue(UserId)` / `verify(String)`.
- Tests: round-trip, expired, wrong secret, missing `user_id`.
- Does **not** generate a User table. You already have `g scaffold
  User`. Auth takes a `userId: String` (or UUID). Wiring the two is
  one constructor argument.

This is deliberately smaller than Spring's full OAuth2 login
generator. Full OIDC is `add security` plus hand-written
`application.properties`. Do not generate a Keycloak stack.

### 8.3 `jails add sse`

Realtime fanout for inbox events. SSE, not WebSocket, as the
default: one-way server→browser, HTTP/2 multiplexed, EventSource
reconnects for free, virtual threads make `SseEmitter` honest.

- `SseHub` : subscribe(id) → `SseEmitter`, emit(id, event, data),
  complete on IOException, heartbeat every 15s so proxies do not
  kill the connection.
- A tiny controller `GET /stream/{id}` producing
  `text/event-stream`.
- Test: subscribe, emit, assert the named event. Use a
  `CountDownLatch`; do not sleep.
- Property: `spring.threads.virtual.enabled=true` if not already
  (Boot 4 / JDK 24+ pinning fix is already in `backend.md`).

WebSocket/STOMP is a second capability (`add ws`) only when you
have a bidirectional chat. For Intercom's *agent inbox updates*,
SSE is the right protocol. For the *widget conversation*, you
probably want WS. Do not generate both on day one. Document the
choice in the capability Javadoc.

### 8.4 `jails g mailer <Name>`

Rails mailer. `spring-boot-starter-mail` + a class with one method
per email, body as a text block, recipient required.

- Test with GreenMail (test scope) or a `JavaMailSender` fake in
  `testkit`. Prefer a Fake: it is already the jails idiom, and
  GreenMail is another container.
- No HTML template engine. A text block in the method is visible.
  Thymeleaf is a rabbit hole.

### 8.5 The Intercom *recipe* is a README section, not a command

```
jails new inbox --deps web
jails add db redis security api
jails g auth Messenger
jails g scaffold Conversation id:uuid@pk userId:string! ...
jails g scaffold Message id:uuid@pk conversation:Conversation body:string! ...
jails add sse
jails g webhook Intercom --header X-Hub-Signature
```

That is eight commands, all idempotent, all destroyable. A
`jails new --like intercom` mega-generator is a plugin in a trench
coat. If the sequence is typed often, a markdown file under
`recipes/inbox.md` that you `jails cases` against is enough. Do
not add `jails recipe`.

The widget (`foo-website` / `bar-website` in minicom) is static
JS. jails does not own frontends. Put a 40-line `messenger.js` in
the project's `src/main/resources/static/` by hand, or not at all.

---

## 9. Idea 7 — Console that is not pretending to be `rails c` (P2)

### Problem

`jails console` is useful for `new User(...)` and Jackson
experiments. It cannot `@Autowired` a bean, cannot `reload!`, and
cannot see uncompiled edits. People will try and bounce.

### Design

**Tell the truth in the banner.** On start, print:

```
jails console: jshell + this module's classpath (not a Spring context)
reload: not supported — re-run `jails c` after compile
beans: jails beans
db:    jails db
```

**A `startup.jshell` snippet** generated once into
`src/test/resources/jshell/` (or `~/.jails/startup.jsh`): import the
base package, `import module java.base` (JDK 25+), static import
AssertJ if on the test classpath. This is the Rails `~/.irbrc`
equivalent and is ~15 lines.

**`/bean FooService` is a trap.** Booting a context inside jshell
takes as long as `jails run` and dies on missing DataSource. Do
not build it. If you want to poke a running app, that is an
actuator endpoint (`add actuator` already exists) or `jails db`.

**JBang for throwaways, not for the project.** `jbang --interactive`
is a better scratch REPL for "does this Jackson annotation work"
because `//DEPS` pulls a library without touching the pom. Document
it in README as the scratchpad; do not wrap JBang in jails. A
wrapper that is "jbang if present else jshell" is hidden
complexity for one-digit uses a week.

### Anti-goals

- No Spring Boot DevTools remote shell (it is gone, and was a
  security incident waiting).
- No compiling `.java` into jshell on save.

---

## 10. Idea 8 — Agent surface: `jails context` (P2)

You develop in Neovim *and* in Cursor. jails already has `--json`
on `about`, `routes`, `beans`. Agents still grep.

### Design

**`jails context [--json]`** — one document an agent can ingest:

- `about` (module, Java release, Spring?, maven command)
- layout + capabilities from `jails.toml`
- `stats` per layer (short)
- `doctor` FAIL count (not the full report; doctor stays
  separately runnable)
- last 5 `why` matches if a log is passed

This is not a new analysis. It is a concatenation with a
schema_version. Cursor/Claude skills can say "run `jails context
--json` before generating Java in this repo."

**`g cases` is already the agent-shaped generator.** A markdown
brief with an Acceptance section becomes a test class. Lean on it
for crawler and inbox work: write the brief first, `g cases`, then
fill methods. Document that workflow next to `g cases` in README;
it is under-sold.

**Do not put an LLM inside jails.** Deterministic generation is
the product. The moment `g spider` asks a model for a CSS
selector, you cannot golden-file it and you cannot destroy it.

---

## 11. Idea 9 — Smaller generators Rails has that jails still lacks (P2)

Each is a kind, not a capability. Closed set, destroyable.

| Kind | What it writes | Why |
| --- | --- | --- |
| `g resource` | record + repo + service + controller, **no** DTOs, **no** migration | Rails `resource` vs `scaffold`. Sometimes you do not want the full REST surface. Today you either scaffold everything or piece it by hand. |
| `g migration add_*` | already have `g mig`; see Idea 4 for field-driven alter | The empty `g mig "add body to notes"` is a filename. The useful one has SQL. |
| `g error` | one `ApiException` variant + advice branch | `add api` generates the sealed type; adding a variant today is a manual edit that the compiler *does* catch (no default). A generator that only adds the record + the switch arm is still worth it so the status code is decided at generate time, not in a hurry during the compile error. |
| `g port` | interface in `app/` + in-memory fake in testkit | Half of `g repo` without SQL. Useful for `PaymentGateway`. |
| `g filter` / `g interceptor` | skip | Easy to write, rarely what you meant. Spring's filter chain is already `add security`. |

`g resource` is the only one I would actually ship. The rest are
"it would be nice" and expand `ArtifactKind` for little daily gain.

---

## 12. Idea 10 — OpenRewrite as `jails upgrade`, not as a generator (P3)

OpenRewrite recipes exist for Java 25 and Boot 4. That is a real
job (this machine will need it every six months). It is not
day-to-day productivity.

- `jails upgrade --dry-run` shells `mvn rewrite:dryRun` with a
  pinned recipe list (Java release, Boot 4.x).
- Doctor stays read-only and can *nudge*: "Boot parent is 4.0.x,
  4.1.x is current; `jails upgrade`."
- Never run rewrite without `--dry-run` as the default.

This is P3 because it is infrequent and because rewrite plugins
are heavy. Do not make it a reason to add Maven recipes to every
generated pom.

---

## 13. Idea 11 — Modulith / ArchUnit as optional walls (P3)

`deps/spring-modulith` is already cloned. Spring Modulith 2.x
would let `jails doctor` verify that `web` does not import
`adapters` except through `app`. That is a real failure mode
(controllers that `new JdbcNoteRepository(ds)`).

- `add modulith` splices the dependency and a `ModularityTests`
  IT that calls `ApplicationModules.of(Application.class).verify()`.
- Package names come from `Config::layers()`, not Modulith
  defaults — same drift bug `stats` already had.

ArchUnit can do the same without Modulith. Prefer Modulith if the
project is Spring; skip if it is `new-cli`.

P3 because the layout is already conventional and `doctor` already
catches the operational failures. This catches *style* failures.
Ship it when a real project first grows a forbidden import.

---

## 14. Neovim specifics (this machine)

These live in `~/code/my-dotfiles`, not in jails, except where
noted. They compound with Ideas 1–3.

1. **`<leader>jg` vs `<leader>Jg`.** Rename jdtls Generate to
   `<leader>jG` or delete it on record-heavy buffers. Offer
   getters on a record is noise.
2. **Point `<leader>jf` / `<leader>jm` / `<leader>jt` at `jails
   test`.** Same resolver as `about`. Keep the treesitter
   nearest-method logic; pass `Class#method` to `jails test`.
3. **Quickfix from `jails test --watch`.** Parse `file:line` and
   `caddexpr`. You already do this for `java_errors.lua`.
4. **Do not start jdtls until you need `gd`.** Optional: a
   `g:jails_lsp_deferred` that lets `:A` / `gf` work in the first
   5 seconds while CDS loads. Aggressive; try Idea 1 first.
5. **Snippet `jfile` vs `jails g class`.** Snippets fill an empty
   buffer; generators place the file in the right package *and*
   write the test. Prefer the generator for anything with a test.
   Keep snippets for the gym (no pom / no jails).
6. **devdocs.io is pinned to Java 25 in init.lua** while projects
   target 27. Either scrape 27 when it is GA (2026-09-15) or
   `jails src` for JDK types from `deps/jdk`. The latter works
   today.
7. **jails.nvim CAPABILITIES list is already stale** relative to
   `src/add.rs` (plugin is missing `toxiproxy`; has an older
   set). Any new capability *must* update
   `jails.nvim/lua/jails/init.lua` — CLAUDE.md already says so.
   A test that greps the Lua lists against `Capability::label()`
   / `ArtifactKind` would have caught this. **Add that test as
   part of P0.** It is a one-hour fix that prevents silent
   completion rot.

---

## 15. What not to build (explicit)

| Temptation | Why not |
| --- | --- |
| JHipster / Bootify / "full stack" | JPA + a frontend. Opposite of this tool. |
| Plugin system / `jails plugin install crawler` | README Not yet. The day you have it, every idea in this file becomes a plugin and none of them get tests. |
| Gradle / mill as project build | Not yet. mill is faster; it is also a second universe. |
| Wrap crawler4j / webmagic / Nutch / StormCrawler | Dead, or a platform. Generate types instead. |
| Wrap spider / crawl4ai via JNI or HTTP | A Python/Rust sidecar is a second runtime. `add html` is Java. |
| Lombok | Editor tax on JDK 26+; records exist. |
| A `jails-support` jar | ActiveSupport lock-in. Write the class into the project. |
| Runtime `beans` / `routes` that boot the context | Not yet, and doctor is read-only. Source-reading is the feature. |
| `jails recipe intercom` | A plugin. Document the command sequence. |
| STOMP/SockJS by default | SSE first. WS when a real bidirectional UI exists. |
| Thymeleaf / HTMX generators | jails is an API/CLI tool. minicom's sites are static. |
| OpenAPI-first codegen (`openapi-generator` is in deps/) | You write the Java and the SQL; the spec follows. The other direction produces types you do not own. Use it only to *consume* someone else's spec (`g client` already covers the first-party case). |
| Spring CLI user-defined commands YAML | Archived project. jails' closed `ValueEnum` is the point. |
| String templates, structured concurrency in generated code | Preview / withdrawn. Virtual threads only. |
| Making `jails check` fast | Leftover classes. Fast path is `jails test`. |

---

## 16. Priority

Effort is "jails-shaped days": golden files, README, nvim list,
`Capability`/`ArtifactKind` enum, no plugin.

| P | Item | Effort | Leverage | Touches |
| --- | --- | --- | --- | --- |
| P0 | nvim projections `:A` `:R` `:E…` + `about --json` v2 | 2–3d | every file jump | `project.rs`, `jails.nvim`, README |
| P0 | Lua KINDS/CAPABILITIES pinned by a test | 0.5d | stops silent rot | `tests/`, `jails.nvim` |
| P0 | Inner loop: README, `<leader>Jt` = current test, `test --watch` | 1–2d | every save | `run.rs`, nvim, README |
| P0 | `jails src` + `gf` into deps | 1–2d | every Spring question | new `src/src.rs`, nvim, `deps/` |
| P1 | `g field` + alter migration | 3d | the week after scaffold | `generate/`, `sql.rs` |
| P1 | `add html` + `g spider` | 3–4d | crawler projects | `add/`, `generate/`, `deps/jsoup` |
| P1 | `g webhook` | 1d | Intercom, Stripe, GitHub | `spring.rs` or `generate/web.rs` |
| P1 | `g auth` (JWT mint/verify) | 2d | every product with a widget | `spring.rs` |
| P2 | `add sse` | 1–2d | inbox fanout | `spring.rs`, compose none |
| P2 | `g mailer` | 1d | transactional email | `spring.rs` |
| P2 | console banner + startup.jsh | 0.5d | honesty | `console.rs` |
| P2 | `jails context --json` | 0.5d | agents | `project.rs` |
| P2 | `g resource` (scaffold minus DTO/migration) | 1d | smaller slices | `generate.rs` |
| P3 | `jails upgrade` (OpenRewrite dry-run) | 2d | twice a year | `run.rs` |
| P3 | `add modulith` | 1d | first forbidden import | `add.rs` |
| P3 | `add browser` (Playwright) | 2d | JS-heavy crawl only | compose + heavy image |
| P3 | FK `references` for `author:User` | 1d | baked into `g field` if cheap | `sql.rs` |

**Do P0 before any new generator.** A spider you cannot jump
around in, whose test you run with `clean verify`, is the old
experience with more files.

---

## 17. First week (if you actually start)

Day 1–2: Idea 1 projections. Drive it against
`tests/golden/scaffold-spring`. Ship `:A` and `:Edomain` only;
`:R` cycle can follow. README Neovim section grows a table.

Day 2: the Lua-list test. Fix `toxiproxy` (and anything else
that drifted) in `jails.nvim`.

Day 3: `jails test --watch` + point `<leader>jf` at `jails test`.
Turn off javac_lint-on-save by default, keep `<leader>jl`.

Day 4–5: `jails src RestClient` opens
`deps/spring-framework/.../RestClient.java`. Wire `gf`. Add one
`why` rule citation as a proof.

Do *not* start `g spider` in the same week. The crawler will
feel good only after jumps and the test loop are not painful.

---

## 18. Research log (what was looked at)

**In-tree:** README (commands, Neovim, Not yet), CLAUDE.md,
`src/generate.rs` (`ArtifactKind`, `layout`), `src/add.rs`
(`Capability`), `src/config.rs` (`LAYERS_IN_ORDER`),
`src/console.rs`, `src/run.rs` (`watch` polls main only),
`src/project.rs` (`about --json` schema_version 1),
`src/generate/field.rs` (closed type table), `jails.nvim`
(terminal wrapper, hand-maintained lists), `deps/deps.tsv`
(80+ checkouts, including rails and spring-modulith, **not**
jsoup), `java.md` / `spring.md` / `backend.md`.

**ideas/:** minicom-public (Spring stub is two controllers;
Rails app is a full tree), crawler4j (dead), webmagic (Java
collector, log4j12, not Boot 4-era), colly (API to copy),
spider / webclaw (Rust, sidecar-not-wanted), crawl4ai (Python,
markdown-for-LLM — steal the *output shape* later), Monzo
challenge (same-domain BFS + dedup is the acceptance test).

**Editor:** `~/code/my-dotfiles/.../init.lua`, `ftplugin/java.lua`
(CDS, conditional Lombok, javac_lint on save, `<leader>j*`),
`lua/java_errors.lua`, `lua/snippets/java.lua`.

**Upstream tools:** vim-rails projections; Phoenix
`phx.gen.auth` / `phx.gen.json`; Spring CLI (archived 2026-05-14);
JHipster / Bootify (JPA, skip); OpenRewrite (upgrade, not generate);
JBang (scratch REPL, do not wrap); mill (faster Maven, do not
switch); nvim-jdtls (inner compile, completion cost); jsoup vs
crawler4j vs Nutch vs StormCrawler (jsoup+HttpClient is the
library cut); Intercom JWT identity + `X-Hub-Signature` webhooks;
Spring Boot 4 SSE (`SseEmitter`) vs WebSocket.

**Intentionally not used as models:** JHipster, Spring Initializr
(jails `new` already wraps it), OpenAPI-generator as a source of
truth, LangChain4j / Spring AI (out of scope for crawler/inbox v1).

---

## 19. One-sentence north star

jails already generates a running system; the 1000x left is making
the *next* edit, jump, test, and Spring question as cheap as
`rails generate` plus `vim-rails`, without adding an ORM, a plugin
system, or a runtime library — and then spending that budget on
two closed slices you actually want to build: a polite spider and
a JWT+webhook+SSE inbox.
