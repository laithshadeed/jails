# Making Java/Spring feel faster than Rails

Research notes and a concrete build plan for `jails`, written against this
repo's current surface (`README.md`, `src/`, `jails.nvim/`), the reference
checkouts in `deps/`, and the two kinds of thing you say you want to build:
an Intercom-shaped messaging product (`ideas/minicom-*`) and web crawlers
(`ideas/monzo-*`, `colly`, `katana`, `crawl4ai`, `webclaw`, `nutch`).

---

## 0. Diagnosis first: where the time actually goes

The instinct is "Java has more boilerplate than Ruby, so generate more code."
That is the smaller half. jails already generates a *running* REST resource
from one line — more than `rails g scaffold` does, because it also writes the
DDL, the JDBC adapter, the DTOs and the tests. On raw keystrokes-to-feature
you are close to parity already.

What is actually slow, ranked by seconds-per-day:

| # | Cost | Rails | Spring today | Fixable in jails? |
|---|---|---|---|---|
| 1 | **Change → see it** | ~0.3 s, save the file | 6–25 s: javac, context restart, container wait | **Yes, biggest win** |
| 2 | **Ask the running system a question** | `rails c`, 2 s, live objects | `jshell` with no beans; or add a log line and restart | **Yes** |
| 3 | **Test feedback** | `rspec path:12`, 1 s | `mvn verify`, 40 s cold | **Yes** |
| 4 | **Boilerplate typing** | `g scaffold` | `jails g scaffold` | Mostly done |
| 5 | **Editor latency** | none (dynamic lang, grep is enough) | jdt.ls index, 20–60 s at open, stale after `add` | **Partly** |
| 6 | **Knowing which API** | one framework, one way | Boot 4 moved things; memory is wrong | Yes (`deps/` + `why`) |
| 7 | **Infra ceremony** | sqlite, zero setup | podman/Testcontainers/compose | Mostly done (`doctor`) |

So: **the roadmap below is ordered by loop latency, not by generator count.**
Rails is not productive because of `g scaffold`. It is productive because the
gap between "I typed a thing" and "I saw the consequence" is under a second,
and because `rails c` lets you poke a live system. Everything else is
decoration. Java can now match both — JDK 25/27 and Boot 4 shipped the
pieces — but nobody has wired them together into one tool. That is the
opening for jails, and it is worth more than twenty new `generate` kinds.

A note on "1000x": you will not get 1000x on anything. You can plausibly get
**10× on the inner loop** (25 s → 2 s, hundreds of times a day), **3× on new
verticals**, and a step change in *not being stuck* — which is where the real
hours vanish. Compounded over a project that is where the multiple lives.

---

## Tier A — The inner loop (do these first; ~70% of the value)

### A1. `jails dev` — one command, live reload, DB up, no restarts

Today: `jails run --watch` uses spring-boot-devtools' restart classloader.
That is a *restart* (1.5–4 s and losing all in-memory state), not a hot swap,
and it does not cover the non-Spring/CLI projects at all.

Target: edit a method body, save, next HTTP request already runs it — no
restart, no state loss.

Three mechanisms, stacked, best-available-wins:

1. **HotSwap for method bodies.** Plain JVM HotSwap only redefines method
   bodies. With `-XX:+AllowEnhancedClassRedefinition` on a JetBrains Runtime
   (or DCEVM build) you additionally get added/removed methods and fields —
   which covers ~90% of real edits. `jails doctor` should detect an enhanced
   JVM and say so; `jails dev` picks it up if present.
2. **Devtools restart** as the fallback for structural changes (new bean, new
   `@Configuration`), which it already does well.
3. **Full restart** only when `pom.xml` changes.

Implementation sketch, staying inside jails' existing shape:

```
src/dev.rs                       # the supervisor
  - resolves the JVM: enhanced? (java -XX:+AllowEnhancedClassRedefinition -version)
  - starts `mvn spring-boot:run` (or plain java for new-cli) with a
    CommandSpec from process.rs, piped like run::run_watched already pipes
  - watches src/main/java with a debounced 120 ms notify(3) watcher
  - per change: javac ONLY the changed file against target/classes
      -> java.lang.instrument redefineClasses over the JDWP socket
      (attach with -agentlib:jdwp=transport=dt_socket,server=y,suspend=n)
  - prints one line: "↻ OrderService.java  hot-swapped in 240ms"
  - falls back to touching target/classes so devtools restarts, and says why
```

`notify` would be jails' second dependency after clap. That is a real cost
against the "clap only" rule — the alternative is a 200 ms `stat` poll over
`src/main/java`, which for a few hundred files is genuinely fine and keeps the
dependency list at one. **Recommend polling.** Measure before adding a crate.

The JDWP redefinition can be done without a Java agent by speaking the JDWP
wire protocol (`RedefineClasses`, command set 2, command 18) over the socket —
about 150 lines of Rust, no JVM-side artifact, no dependency. That is the
elegant version and it is very much in this project's taste.

**Also fold in what `jails dev` should do besides watching**, because the
point is one terminal instead of four:

- start compose services (already in `run`),
- print the routes table once at startup (`inspect.rs` already computes it),
- pipe output through `why`'s FATAL_MARKERS (already in `run_watched`),
- and **watch `src/main/resources/db/migration/`**: a new migration file is
  applied to the running dev database immediately (`flyway:migrate` or the
  psql path `migrate.rs` already builds). In Rails you never think about this;
  in Spring people restart the app to get a migration applied.

### A2. Kill startup time with the JDK's AOT cache

JDK 24 shipped AOT class loading & linking (JEP 483) and JDK 25 made the
ergonomics one flag (JEP 514). On a Boot app this is typically **30–45% off
cold start** for free — which is paid on every devtools restart, every
`mvn test` fork, every `jails run`.

```
jails add aot        # or fold into `new`
  -> records a training run:  java -XX:AOTCacheOutput=app.aot -jar target/app.jar
  -> writes .jails/app.aot (gitignored)
  -> jails run/dev/test pass -XX:AOTCache=app.aot when it exists and is newer
     than target/classes; silently skip otherwise
  -> `jails doctor` reports a stale cache rather than letting it be silently ignored
```
Pair with `-XX:+AutoCreateSharedArchive -XX:SharedArchiveFile=` on older JDKs.
This is pure win: no code change, no risk, and it compounds with everything.

### A3. `jails console` should have beans in it

`jshell` with a classpath is not `rails c`. The Rails console's whole value is
that **the objects are alive**: the DB is connected, the services are wired,
you can call `MessageService.send(...)` against real data.

Boot 4 makes this reachable. Two designs, both real:

**(a) Attach to the app `jails dev` is already running.** Start it with a
JDWP socket (you need it for A1 anyway) and expose an eval endpoint from a
dev-only auto-configuration jails adds under `spring.profiles.active=dev`:

```java
// generated by `jails add console`, dev profile only, never on the main path
@RestController @Profile("dev")
final class JailsConsoleController {
  private final ApplicationContext ctx;
  @PostMapping("/__jails/eval") String eval(@RequestBody String snippet) { ... }
}
```
The evaluator is a `jshell` `LocalExecutionControl` bound to the live context
so `ctx.getBean(MessageService.class)` returns the *running* bean. `jails c`
then becomes a readline REPL over HTTP with `$ctx` predefined, plus sugar:
`beans`, `routes`, `sql "select ..."`.

**(b) Cheaper first cut:** `jails c` boots the Spring context itself in the
jshell process (`SpringApplication.run(App.class)` as the first snippet, with
`--web-application-type=none` unless asked), predefining `ctx`, `jdbc`
(a `JdbcClient`) and every `@Service` bean as a variable named after its type
in camelCase. Two seconds with A2's AOT cache. **Ship (b) first**; it is a
~150-line change to `console.rs` and it is 80% of the value.

Either way, add:
```
jails c -e 'messageService.send(1L, "hi")'    # one-shot, scriptable — rails runner
jails sql 'select * from messages limit 5'    # already half-there in `jails db --`
```
`rails runner` equivalence is underrated: it makes every "let me just check
something" a one-liner instead of a test file.

### A4. Sub-second tests

`jails test` today shells to Maven, which costs 3–8 s before a single test
runs. Fixes, in order of payoff:

1. **`jails test --watch`**: keep one Maven daemon (mvnd) or one forked JVM
   alive, recompile the changed test, rerun only it. Print `1 passed in 0.4s`.
2. **`jails test --failed`**: rerun only what failed last time, read from
   `target/surefire-reports/*.xml`. Rails' `--only-failures` again.
3. **Testcontainers reuse**: write `~/.testcontainers.properties`
   `testcontainers.reuse.enable=true` and add `.withReuse(true)` in
   `TestcontainersConfig` — a Postgres container survives between runs and
   saves ~4 s per invocation. `jails doctor` should check the flag, because
   without it people conclude Testcontainers is "just slow".
4. **`jails test <file>:<line>`** — map a line to its `@Test` method by scanning
   with `java::blanked()` (which already exists and is exactly the right tool)
   and pass `-Dtest=Class#method`. This is the single most-used Rails testing
   affordance and jails is ~40 lines from having it.
5. Then wire it to Neovim: `<leader>Jt` on the cursor line runs that one test.

### A5. Make errors terminal, not exploratory

`why.rs` is the best idea in this repo and is under-exploited. Extend it:

- **`jails why --fix`**: rules already carry a `fix:`; where that fix is a
  jails command (`jails add db`, a property line), offer to run it. Most of
  the top-10 real failures on this machine are one command away.
- **Compile errors, not just runtime.** `javac` diagnostics are a closed set:
  `cannot find symbol` after a `generate` almost always means a missing import
  jails could add; `does not override abstract method` after `add`; the Boot 4
  moved-class family (`@MockBean`, `@AutoConfigureMockMvc`,
  `MeterRegistryCustomizer`) which jails already knows about in three places.
  A `javac` rule table would turn "grep the exception, open a browser" into a
  line of output.
- **`jails why` on stdin from the LSP**: pipe jdt.ls diagnostics through it.

---

## Tier B — Verticals for what you actually build

### B1. The Intercom shape: `jails g conversation`, and the real-time slice

`ideas/minicom-*` is users → conversations → messages, with a foo (customer)
widget and a bar (agent) inbox, and the interesting part is delivery: the
agent must see a customer message without refreshing.

jails today gets you the CRUD in one line. The missing generators are exactly
the ones that took the longest in every messaging app anybody has written:

**`jails g sse <Name>`** — server-sent events, which is the right default for
this shape (unidirectional, survives proxies, no protocol upgrade, reconnects
for free with `Last-Event-ID`). Generates:
- `SseHub` — a `Map<ConversationId, List<SseEmitter>>` with the three things
  everyone forgets: `onCompletion`/`onTimeout`/`onError` removal (leak
  otherwise), an infinite timeout set explicitly (default is 30 s and the
  browser silently reconnects forever), and a heartbeat comment every 15 s
  (proxies kill idle streams).
- A controller producing `text/event-stream`.
- A test that opens the stream on a real port, posts a message, and asserts it
  arrives — over a socket, the way `handler`'s test already works.
- Under `Last-Event-ID`, a replay hook from the store, because "I missed the
  messages sent while my laptop was asleep" is the bug this always ships with.

**`jails g websocket <Name>`** — for when you need bidirectional (typing
indicators, read receipts). Boot 4 + `spring-websocket`: a `WebSocketHandler`,
registration, a `TextMessage` codec over the JSON mapper `add json` installs,
and a test with a real `StandardWebSocketClient`. Include the session registry
and the *broadcast to everyone but the sender* helper, since that off-by-one is
the classic.

**`jails g presence`** — online/typing/last-seen over Redis (`add redis` exists)
with TTL keys, because it is three lines of Redis and a day of getting the
expiry semantics right.

**`jails add auth`** — the gap between `add security` (a filter chain) and a
product. Sessions or JWT, a `User` record + migration, signup/login/logout
handlers, BCrypt via Spring Security's `PasswordEncoder`, and a
`@AuthenticationPrincipal` sample. `rails g devise` is a big reason Rails feels
fast, and this is squarely inside jails' "the failure is silent" bar: a
hand-rolled auth chain that permits too much looks identical to one that does
not.

**`jails g page <Name>`** — the deliberate scope decision. A messaging product
needs HTML. The Rails-shaped answer for Java in 2026 is
**Thymeleaf + htmx + SSE**, not a React build. `g page Conversation` writes a
Thymeleaf template, a `@Controller` returning a view name, and an htmx
`hx-get`/`sse-swap` fragment — so the SSE hub above already has a consumer.
This is the single biggest "Rails feeling" item in the list: right now a jails
project can only be an API, and a take-home that renders nothing is half a
submission. Guard it: one layout, one fragment convention, no asset pipeline.

**`jails g inbox`** — the composed scaffold: `User`, `Conversation`, `Message`,
the SSE hub, the agent list page and the customer widget endpoint, wired. One
command → a working minicom. This is `rails new --template`, and it is how you
turn a 3-hour take-home setup into 3 minutes. Which leads to:

### B2. `jails new --template <name>` (project templates)

Rails' `rails new -m template.rb` and `rails app:template` are the actual
1000x feature nobody copies. A jails template is just **a list of jails
commands**, which makes it trivial and safe:

```toml
# templates/inbox.toml  (or ~/.config/jails/templates/)
[template]
name = "inbox"
description = "Intercom-shaped: users, conversations, live agent inbox"
capabilities = ["db", "json", "testkit", "format", "api", "auth"]
generate = [
  "record User id:uuid@pk email:string! name:string!",
  "scaffold Conversation id:uuid@pk subject:string! createdAt:instant@index",
  "scaffold Message id:uuid@pk conversationId:uuid@index body:text! authorId:uuid sentAt:instant",
  "sse Message",
  "page Conversation",
]
```
`jails new minicom --template inbox` then does the whole thing. Because
`jails.toml`'s `[project] capabilities` already exists and `sync` already
applies it, half of this is built. Extend the manifest with a
`[project] generated` list and `sync` becomes "make this project be what the
file says" — which is also a genuinely better answer to "my teammate cloned it
and nothing works" than any README.

Ship three templates: `inbox` (above), `crawler` (below), `api` (scaffold +
api + db + observability + format). Keep it a closed set of jails commands —
**no arbitrary shell, no Ruby-style DSL**. That is what keeps it from becoming
the plugin system README.md says is out of scope.

### B3. The crawler shape

`ideas/` has eleven crawlers; the Java ones (`crawler4j`, `webmagic`,
`nutch`, `stormcrawler`, `heritrix3`) are heavyweight frameworks, and the ones
you actually admire (`colly`, `katana`, the Monzo Go solution) are ~500 lines
of concurrency done right. Java 25+ makes that shape *easier* than Go —
virtual threads plus structured concurrency — and nobody has a scaffold for it.

**`jails g crawler <Name> --seed <url>`** should write the six things every
crawler gets wrong:

1. **A frontier**, not a queue: `BlockingQueue<Uri>` plus a visited
   `ConcurrentHashMap.newKeySet()` where the key is the *normalised* URL —
   lowercase host, strip default port, strip fragment, sort query params, drop
   trailing `/`. The Monzo interview fails on exactly this.
2. **Same-domain scoping** as an explicit `Predicate<URI>` (registrable-domain
   aware, so `blog.monzo.com` is a decision, not an accident).
3. **Politeness**: a per-host token bucket + `robots.txt` fetch and cache.
   `re2j` is already in `deps/` and is the right matcher for the path rules —
   linear time, no catastrophic backtracking on a hostile pattern.
4. **Concurrency**: one virtual thread per URL, bounded by a `Semaphore`, the
   whole run inside `try (var scope = StructuredTaskScope.open())`. Note
   `deps/jdk`: structured concurrency is still preview in 27 and CLAUDE.md
   forbids preview features in generated code — so **generate the executor
   version** (`Executors.newVirtualThreadPerTaskExecutor()` + `Semaphore` +
   `CompletableFuture.allOf`) and leave a Javadoc pointing at the SC version.
   This is exactly the kind of call jails should make once, correctly.
5. **A parser boundary**: link extraction behind an interface with a regex
   implementation and a Jsoup one, so the test can use neither.
6. **Termination**: an in-flight counter that goes to zero, not
   `queue.isEmpty()` — the bug that makes crawlers exit early with 3 URLs.

Plus `--out json` writing the `output_example.json` shape, and a test using
**WireMock** (`deps/wiremock` is already checked out) serving a tiny fake site
with a cycle, an off-domain link, a 301 and a 404 — so the generated test
proves the parts people actually get wrong.

**`jails add http`** already exists; extend the crawler path with
`resilience4j` (in `deps/`) for the retry/circuit-breaker, and a
`Caffeine`-backed response cache for re-runs during development, so iterating
on the parser doesn't refetch the internet every time. That last one is a real
dev-loop win in the same family as A1.

**`jails crawl <url> --depth 2`** as a *jails* subcommand for one-off use is
tempting; resist it. jails scaffolds Java projects, it isn't itself a crawler.

### B4. Scratch-first development: `jails scratch`

JDK 25 finalised compact source files and instance `main` (JEP 512), and the
multi-file source launcher (JEP 458) means `java Main.java` compiles a whole
directory in memory. Combined:

```
jails scratch parse-sitemap        # writes .jails/scratch/parse-sitemap.java
                                   # with the project's deps on the classpath
                                   # (from `mvn dependency:build-classpath`, cached)
                                   # and opens it, then `jails scratch -r` runs it
```
A 3-line file, `void main() { ... }`, no class, no package, run in 400 ms with
Jackson and your own domain types available. This is the missing "just try
something" mode that Ruby has by existing. It is small to build and you will
use it every day — and it pairs with `jails c -e` from A3.

---

## Tier C — Neovim

`jails.nvim` is a thin shell wrapper today. The complaint "neovim is slow for
Java" is usually really "jdt.ls is slow and I can't see the project". Things
jails is uniquely placed to fix, because it already reads Java without a
language server (`java.rs`, `inspect.rs`):

1. **Instant pickers that don't need jdt.ls.** `jails routes --json` and
   `jails beans --json` already emit structured data. Wire them to
   `vim.ui.select`/Telescope: `:JailsRoutes` jumps to the handler,
   `:JailsBeans` to the declaration. Sub-50 ms on a project that does not
   compile — jdt.ls cannot do that. Add `jails symbols --json` (types, methods,
   file:line via `java.rs`) as a fallback document-symbol source.
2. **A `jails dev` panel**, not a terminal: parse the piped output, put
   compile errors straight into the quickfix list with `file:line`, and run
   unrecognised failures through `why` so the *explanation* is the quickfix
   text. That is the loop closing inside the editor.
3. **`<leader>Jt` = run the test under the cursor** (needs A4.4).
4. **Snippets generated from `templates/*.java`.** They are real Java files
   with `{{name}}` placeholders — a 30-line script emits LuaSnip/`vsnip`
   snippets from the same source, so the editor and the generator never drift.
   `jails snippets --format luasnip` as a subcommand keeps it honest.
5. **`jails about --json` already exists** — use it to set `JAVA_HOME`, the
   jdt.ls workspace root and the runtime config automatically, so opening a
   file below any module attaches the right server. Half the "Java in Neovim is
   awful" experience is a misconfigured jdt.ls, and jails knows the answer.
6. **A `:Jails` command palette** driven by `jails completion`-style data
   rather than the hand-maintained `SUBCOMMANDS`/`KINDS` lists in
   `init.lua` — CLAUDE.md already flags those as a silent-drift hazard. Emit
   `jails completion json` and have the plugin read it once at startup.
7. **Consider `nvim-java` or `jdtls` + `spring-boot-tools`** for the parts
   jails should not reimplement (rename, extract, imports). jails' `rename` is
   correctly documented as the fallback; don't grow it.

---

## Tier D — Not being stuck (the hidden hours)

1. **`jails docs <symbol>`** — grep `deps/` for the real declaration and print
   it with `file:line`. CLAUDE.md already says the checkouts are the reference
   "not memory", and the `@MockBean`/`@AutoConfigureMockMvc`/Jackson-3 notes are
   evidence that this lookup happens constantly and by hand.
   `jails docs MeterRegistryCustomizer` → the file, the package, the module,
   and which Boot version moved it. ~80 lines with `java.rs`.
2. **`jails upgrade`** — diff a project against what the current jails would
   generate (the golden-file harness from commit `4a51104` already knows how to
   snapshot every byte). Report drift, offer to apply the ones that are
   unmodified. This is `rails app:update`, which is a big part of why staying
   current in Rails is cheap.
3. **`jails explain <file>`** — for generated code, print *why* it is shaped
   that way (the Javadoc already carries some of this). Turns the tool into the
   thing that teaches the ecosystem instead of hiding it.
4. **Extend `why`'s rule table by mining logs again.** CLAUDE.md documents the
   grep over `~/.codex/sessions` that took coverage 2/6 → 6/6. Run it monthly;
   it is the highest-yield hour in this whole document, and it is already a
   solved procedure.

---

## Tier E — Interview / take-home mode

You are clearly using this for timed exercises. Two small things pay off
disproportionately:

- **`jails new --template inbox|crawler`** (B2) turns the first hour into a
  minute, and the hour you save is the one where you'd otherwise be fighting
  Gradle-vs-Maven and a compose file rather than showing judgement.
- **`jails stats` + `jails check` as a pre-submit gate.** Add
  `jails ship`: format, full clean verify, `doctor`, `notes` (so no stray
  TODO ships), and a printed summary of routes/beans/test-ratio. One command
  before you hit send.

Note that `ideas/minicom-public/spring` is **Gradle**, and README.md defers
Gradle deliberately. For a take-home you can't choose the build tool, so
either accept that jails doesn't apply there, or add the narrowest possible
`jails g`-only Gradle support: generators only, no `add`/`run`/`check`. I'd
scope it to detecting `build.gradle` and refusing everything that splices
`pom.xml` with a clear message, rather than half-supporting it.

---

## Recommended order (what I'd actually build)

| Order | Item | Why here | Rough effort |
|---|---|---|---|
| 1 | A4.4 `test <file>:<line>` + `--failed` | Hours of value, ~a day of work | S |
| 2 | A2 AOT cache | Free speed under everything else | S |
| 3 | A3(b) `jails c` with a live context | Unlocks "just check something" | M |
| 4 | A1 `jails dev` (poll + devtools; JDWP later) | The big one | L |
| 5 | B4 `jails scratch` | Tiny, used daily | S |
| 6 | B2 templates (`new --template`) | Compounds every generator you have | M |
| 7 | B1 `g sse` + `g page` + `add auth` | Makes a jails project a *product* | L |
| 8 | C1–C3 nvim pickers, dev panel, test-at-cursor | Closes the loop in the editor | M |
| 9 | B3 `g crawler` | Whole vertical, one command | M |
| 10 | D1 `jails docs`, D4 mine `why` again | Anti-stuck | S |

Items 1, 2 and 5 are a weekend and change the feel of the tool immediately.
Item 4 is the one that changes the language.

## What to keep saying no to

The scope bar in `CLAUDE.md` is a feature, and most of the above respects it.
Explicitly still out: a plugin system (B2's templates are a closed list of
jails commands, not hooks); an ORM; full Gradle; a runtime bean view that boots
the context (A3 boots one *on purpose*, for the console — that is not the same
as `beans` doing it); and any generator whose failure mode isn't silent, since
that has been the bar for every kind so far and it is why this tool is good.
