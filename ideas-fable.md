# ideas-fable.md — Java/Spring/Neovim at Rails speed, verified

Written 2026-08-21 against this tree at `6a74938` (plus the uncommitted
`src/app.rs` / README work in progress), the checkouts under `deps/`, the
crawler and minicom corpora under `ideas/`, this machine's toolchain and
Neovim config, and primary sources on the web. Twelve research passes fed it;
their full reports (≈700 KB, every claim with a `file:line` or URL) are copied
to `ideas/fable-research/` and listed in §12.

This is the third ideas document. `ideas-opus.md` and `ideas-grok.md` are
good and mostly right, and this one does not restate them. It does three
things they could not:

1. **Corrects them** where the source says otherwise (§1). Several of those
   corrections are load-bearing — the JDWP command set that would fail on the
   first packet, the AOT cache that cannot apply to `jails run` at all, the
   webhook algorithm that would reject every real Intercom delivery, the JVM
   plan that cannot load a `--release 27` class file.
2. **Puts this machine's numbers on the diagnosis** (§2): Maven's median is
   2.57 s, a Postgres container is 8 s, a domain test is 5 ms, `javac` is
   1.45 s of which 1.2 s is JVM start.
3. **Adds what neither had**: the save→restart loop that already exists with
   no Maven in it; Quarkus-style affected-test selection from class-file
   constant pools; Spring context-cache accounting; a Postgres `SKIP LOCKED`
   queue in visible SQL; `g timeline` for the Intercom data model;
   `crawler-commons` instead of a hand-rolled robots parser; projectionist
   instead of a Lua `:A`; AGENTS.md as a jails-maintained file; and the fact
   that `jails app` already shipped the "templates" both docs proposed.

On "1000×": nobody gets that on a single loop. The honest shape is 5–10× on
the three loops you run hundreds of times a day (edit→see, edit→test,
question→answer), 3–5× on standing up a vertical, and a category change in
*not being stuck* — and those multiply. Rails feels 1000× because it removed
every one of those at once. This document is ordered so the cheap, certain
wins come first and the big bets are measured before they are promised.

---

## 1. Corrections to the two earlier documents

Every row below was checked against source. The "where" column is the
evidence; paths are relative to this repo unless they are URLs.

| # | Earlier claim | What the source says | Where |
|---|---|---|---|
| 1 | opus A1: JDWP `RedefineClasses` is "command set 2, command 18" | Command set **1** (`VirtualMachine`), command 18. Set 2 is `ReferenceType`. You also need `IDSizes` (1/7) and `ClassesBySignature` (1/2) first, and ID widths are dynamic — a working client is ~400 lines, not 150 | `deps/jdk/src/java.se/share/data/jdwp/jdwp.spec:27,44,164,458,603` |
| 2 | opus A2: AOT cache pays "on every devtools restart" and on `jails run/dev/test` | A devtools restart is a new classloader in the same JVM, not a process start (`Restarter.java:277-283`) — AOT never pays there. Worse: the cache **cannot be created or used with a non-empty directory on the classpath** (`target/classes`), so `spring-boot:run`, `mvn test` and `java -cp target/classes` are all out. Boot's recipe needs the **extracted** jar and `-Dspring.context.exit=onRefresh`; the cache is keyed on jar size+mtime, so every `mvn package` invalidates it | `deps/jdk/src/hotspot/share/cds/aotClassLocation.cpp:687-708`; `deps/spring-boot/documentation/.../packaging/aot-cache.adoc:20-38`; `deps/spring-boot/module/spring-boot-devtools/.../restart/Restarter.java:277-283` |
| 3 | opus A1, grok: enhanced hot swap via JetBrains Runtime | The flag is real, but JBR tops out at **JDK 25** (`jbr-release-25.0.4b508.27`, 2026-08-03). `pom::TARGET_RELEASE = 27` emits class-file major 71, which JBR 25 refuses (`UnsupportedClassVersionError` / JDWP `UNSUPPORTED_VERSION`). mise has no `jbr` entry; JBR GitHub releases carry zero binaries. Blocked until JBR ships 27+ or the dev loop compiles at 25 | github.com/JetBrains/JetBrainsRuntime (branches, releases); `src/pom.rs:41`; `deps/jdk/make/conf/version-numbers.conf` |
| 4 | grok §8.1: Intercom webhook strategy `hmac_sha256_hex`, header `X-Hub-Signature-256` | Intercom signs `X-Hub-Signature` with HMAC-**SHA-1**, `sha1=` prefix, 40 hex chars, keyed by the app's `client_secret` (a different secret from the Messenger JWT secret). A verifier built to grok's spec rejects every delivery. The closed set is three: `hmac_sha1_hex`, `hmac_sha256_hex` (GitHub `-256`), `stripe_v1` (with a **required** 300 s tolerance check) | developers.intercom.com/docs/references/webhooks/webhook-models; docs.stripe.com/webhooks/signature |
| 5 | opus B1: "`SseEmitter` default timeout is 30 s" | Spring's default is `null` ("depends on the underlying server"); the 30 s is Tomcat's `Connector.asyncTimeout = 30000`. Generate `new SseEmitter(0L)` (≤0 = no timeout, verified end to end into Tomcat's `AbstractProcessor`) and/or `spring.mvc.async.request-timeout=-1` | `deps/spring-framework/spring-webmvc/.../SseEmitter.java:53-65`; `deps/tomcat/java/org/apache/catalina/connector/Connector.java:169`; `deps/tomcat/java/org/apache/coyote/AbstractProcessor.java:720-723` |
| 6 | grok §8.3: "`spring.threads.virtual.enabled=true` if not already" | Boot 4's default is **`false`**, so it must be set. And a virtual-threads app with only `@Scheduled` work **exits 0 immediately** unless `spring.main.keep-alive=true` — a `why` rule | `deps/spring-boot/core/spring-boot/.../thread/Threading.java:41-51`; `features/spring-application.adoc:401-419` |
| 7 | opus B2 `jails new --template`; grok §8.5 "a README section, not a command" | **`jails app` already exists**: `.jails/app.toml` with `schema`, `capabilities`, `[[generate]]` intents, `plan`/`apply`, resumable `.jails/app-state-v1`, closed schema. Build *on* it: the minicom and crawler deliverables are `app.toml` files plus new kinds, not a new command | `src/app.rs`; `README.md:354-400`; `examples/DOGFOOD.md` |
| 8 | grok: `rails test --only-failures` | Not a Rails feature (it is RSpec's). Rails prints a copy-pasteable `bin/rails test path:LINE` on failure instead | `deps/rails/railties/lib/rails/test_unit/reporter.rb:84-92`; grep of `railties/lib` |
| 9 | opus: `--watch` "already pipes through `why`" | It does not: `run::watch` spawns with inherited stdio; only `run::run` → `run_watched` scans `FATAL_MARKERS`. Also the poll is *max mtime*: it cannot name the changed file, misses deletions, and misses `git checkout`/`stash pop` (older mtimes) | `src/run.rs:283-285,325-345,381` |
| 10 | opus: "`notify` would be the second dependency" | Third — `clap` and `clap_complete` are both declared. Polling is still the right call (devtools and Quarkus both poll) | `Cargo.toml` |
| 11 | grok §14.7: nvim `CAPABILITIES` missing `toxiproxy` | Also `SUBCOMMANDS` missing `app`, no `OPTIONS.app`, `STREAMING` missing `migrate`/`app`, `OPTIONS.migrate` missing `--check`. Plus two bugs: `setqflist({}, 'r', …)` **replaces** the error list jdtls just built; `vim.fn.termopen` is deprecated since 0.11 | `jails.nvim/lua/jails/init.lua:12-36,65-81,84-122,222,297` |
| 12 | grok §9: "DevTools remote shell is gone" | The CRaSH SSH shell was an *actuator* feature removed in Boot 2.0. Devtools *remote* (`RemoteSpringApplication`) still ships in 4.2 | `deps/spring-boot/module/spring-boot-devtools/.../remote/` |
| 13 | opus C1: `vim.ui.select`/Telescope pickers | This config is **fzf-lua** (`nvim-pack-lock.json`); no telescope. Pickers must be `fzf_exec` | `~/.config/nvim/init.lua:142,773-811` |
| 14 | opus C4: LuaSnip snippets generated from `templates/*.java` | Don't. The existing 642-line `snippets/java.lua` was measured against two corpora; a snippet that duplicates `jails g class` is worse than running it | `~/.config/nvim/lua/snippets/java.lua` header |
| 15 | opus B3 / grok Idea 5: robots via `re2j` / "a tiny parser"; tests via WireMock; capability `add html` | Take `crawlercommons.robots` (RFC 9309's longest-octet match, Allow-wins-tie and group-combining are where hand parsers go wrong); test with a `com.sun.net.httpserver` `FakeSite` (jails' own `add http` already made that call in writing); name it `add crawl` — `add http` exists and is a *server* | `ideas/stormcrawler/core/.../protocol/RobotRulesParser.java:22-25,74,80,197-199`; `src/add/tooling.rs:14-16` |
| 16 | grok: "crawler4j unmaintained since ~2018" | Last release 2018-03-26, last commit 2020-10-03, last push 2021-11, 188 open issues, not archived. "Dead" is fair; the date is not | `ideas/crawler4j` log; GitHub API |
| 17 | CLAUDE.md: "Commons CSV renamed `build()` to `get()` in 1.13" | *Deprecated* in 1.13.0 (`Builder` became a `Supplier`); jails pins 1.14.1; whether `build()` is removed there is unverified — add commons-csv to `deps.tsv` | `src/add/data.rs:14,19`; javadoc.io commons-csv 1.13.0 |
| 18 | CLAUDE.md: manifest at `deps/deps.tsv` | It is `deps.tsv` at the repo root now (109 lines) | `ls deps.tsv deps/deps.tsv` |
| 19 | CLAUDE.md: "Testcontainers reads `DOCKER_HOST` … and finds neither" | On this machine today `DOCKER_HOST=unix:///run/user/1000/podman/podman.sock` is exported and `podman.socket` is active. Re-verify the failure mode before designing around it | `machine.md §5` |
| 20 | both: nothing about the JDK line | **JDK 27 is non-LTS** (six months of updates); 25 is LTS, is JBR's ceiling, is Boot's AOT-cache floor, and is where compact source files, module imports, flexible constructors and `ScopedValue` all went final. `TARGET_RELEASE = 27` is a choice worth recording and maybe reversing (`--java 27` opt-in) | bell-sw JDK 27 overview; `deps/jdk` `Source.java:278-283` at `jdk-27+34` |
| 21 | grok §2 / anyone: `StableValue` for lazy singletons | Renamed to `java.lang.LazyConstant` in 26, still preview (JEP 531, 3rd). **`ScopedValue` is final since 25** and is the right crawl-context carrier across virtual threads | `deps/jdk` `java/lang/LazyConstant.java:206,215`; `java/lang/ScopedValue.java` |
| 22 | folklore: `-XX:TieredStopAtLevel=1`, `spring.jmx.enabled=false` as speed tips | `spring-boot:run` already adds `TieredStopAtLevel=1` (`optimizedLaunch=true`); JMX is already off. Obsolete advice | `deps/spring-boot/build-plugin/spring-boot-maven-plugin/.../RunMojo.java:56-57,89-90`; `JmxProperties.java:36` |
| 23 | anyone planning browser refresh on devtools LiveReload | Deprecated in Boot 4.1.0 "with no replacement", off by default | `DevToolsProperties.java:197,204,213`; `devtools.adoc:285` |
| 24 | grok §8.4: GreenMail or a fake for mail | Boot's own integration test uses **Mailpit** `axllent/mailpit:v1.19.0` (SMTP 1025, POP3 1110) and reads mail back over POP3; Boot 4 also ships `spring-boot-starter-mail-test` | `deps/spring-boot/test-support/.../TestImage.java:163`; `MailSenderAutoConfigurationIntegrationTests.java:75-119` |
| 25 | opus B1: minicom is "users → conversations → messages" | `ideas/minicom-rails/db/schema.rb` has **no conversations table** — messages hang off users with a `direction` char. The conversation entity has to be invented, which is what `g timeline` is for (§6.2) | `ideas/minicom-rails/db/schema.rb` |

Confirmed as claimed, for the record: JEP 483/514 in 24/25 (flags appear at
`jdk-24+36` / `jdk-25+36` in `cds_globals.hpp`); structured concurrency is
JEP 533, seventh preview, and primitive patterns JEP 532, fifth; `import
module` and compact source files final in 25; `@MockBean` gone,
`@MockitoBean` in `spring-test`; `MockMvcTester` at `@since 6.2` and
`RestTestClient` at `@since 7.0`; `spring.docker.compose.skip.in-tests`
defaults true; devtools restarts when `target/classes` changes and the docs
name Eclipse saving as a trigger; Lightpanda is Zig, speaks CDP, ships
`lightpanda/browser:nightly` — and is **AGPL-3.0**, so it is run as a process,
never linked.

---

## 2. The diagnosis, with this machine's numbers

Measured, not estimated. Sources: `~/.codex/sessions` logs (281 Maven
`Total time:` lines, dozens of Testcontainers `started in PT…S` lines),
`~/code/bank/rewards/target/surefire-reports/*.xml`, and timing runs in the
research passes.

| Loop | Where the time goes | Number |
|---|---|---|
| `mvn test -Dtest=X` wall time | Maven JVM + plugin realms (0.4–0.8 s), compiler plugin twice, Surefire fork (0.5–1 s) | min 0.56 s, **median 2.57 s**, p75 13.3 s, p90 25.2 s |
| the test itself | a domain record test | **5 ms** (`domain.MccTest`, 3 tests) |
| a `@WebMvcTest` slice | context start | 2.2 s |
| a Testcontainers-backed test | `postgres:17-alpine` start | **7.0–8.8 s**, plus 0.45 s of Ryuk |
| save → app restarted (`jails run --watch`) | jails 750 ms poll + `mvn compile` (seconds) + devtools 1 s poll + 400 ms quiet + context refresh | ≥2.15 s of pure waiting before any work |
| `javac` on one file | JVM start, not compilation | **1.45 s**; **0.25 s** with `-J-XX:+AutoCreateSharedArchive -J-XX:SharedArchiveFile=` (measured) |
| `java -cp out T` hello world | JVM floor | 0.11 s |
| jdt.ls | CDS already in place ("more than half" off time-to-ready); **no** java-debug / vscode-java-test / Spring Boot Tools bundles; no nvim-dap; `updateBuildConfiguration` left at `interactive` | every dependency-splicing `jails` command leaves phantom errors until prompted |

Reference points from elsewhere: Quarkus' own continuous-testing example goes
**1470 ms → 295 ms** on a one-line change, from *selection* alone; Quarkus
nags you to enable instrumentation when a reload exceeds **4 s** — that is
the bar to beat. Spring PetClinic on Zulu 26 (blog.rasc.ch, 2026-04): fat jar
6.28 s → extracted 5.30 s → AOT cache 2.70 s → CRaC 1.01 s.

So the 2.5 s around a 5 ms test is three separable costs — Maven start,
whole-module recompile (`useIncrementalCompilation=true` recompiles
*everything* on any change, MCOMPILER-209), JVM start — and each has a
different fix. Rails' save is free because of runtime reloading and runtime
schema introspection; neither transfers. What transfers is making the *next*
test, the *next* restart and the *next* question cheap.

---

## 3. Tier 0 — free or near-free, done in hours

Each item is a config change, a handful of lines, or a doctor check. Together
they are most of the felt improvement.

### 3.1 The save→restart loop already exists, with no Maven in it

jdt.ls imports Maven projects through m2e, which sets the Eclipse output
folder to `<build><outputDirectory>` = `target/classes`
(`org.eclipse.m2e.jdt/.../AbstractJavaProjectConfigurator.java:232-234,348-349`).
Devtools watches exactly classpath *directories*
(`ClassPathDirectories.java:57-59`) and Spring's docs say "In Eclipse, saving
a modified file … triggers a restart" (`devtools.adoc:99-105`). So with
`java.autobuild.enabled` (default `true`) and an app started by `jails run`:
**`:w` → jdt.ls writes the class → devtools restarts within ~1.4 s.** No
`mvn compile`, no jails poll. `jails run --watch`'s Maven rebuild is
redundant and slower whenever jdt.ls is attached.

Do:
- `jails run --hot` (or just the README): start the app, print "save in your
  editor; the language server compiles and devtools restarts", and **do not**
  run the Maven loop.
- `doctor`: devtools on the classpath; `spring.devtools.restart.enabled` not
  `false`; the app launched from `target/classes`, not a repackaged jar (no
  directory → nothing to watch); jdt.ls output directory is `target/classes`
  (devtools prints the classpath at startup, `devtools.adoc:245`).
- `why` rule, **after reproducing it once**: `java.lang.Error: Unresolved
  compilation problem` — ECJ emits a class for a file that does not compile,
  so a mid-edit save can restart into a method that throws at call time.
- Know that jdt.ls and `mvn clean` fight over `target/`
  (redhat-developer/vscode-java#3275); `jails check` may need a retry with
  the buffer saved. A doctor note is cheaper than the bug report.

### 3.2 A dev-only property layer nobody uses: `META-INF/spring-devtools.properties`

Every `defaults.<key>` in that file, on any classpath entry, is applied **only
when devtools is present**
(`DevToolsPropertyDefaultsPostProcessor.java:69`, `DevToolsSettings.java:69-70`).
Boot's own modules use it (`spring.thymeleaf.cache=false`, the
`docker-compose` readiness wait, …). Zero effect on the packaged jar, no
profile to remember. `jails new` should write one:

```properties
defaults.spring.devtools.restart.poll-interval=200ms
defaults.spring.devtools.restart.quiet-period=50ms
defaults.spring.docker.compose.enabled=false        # this machine's podman-compose problem
defaults.logging.structured.format.console=ecs      # see §8.4, only if `why` reads JSON
```

The first two lines take ~1.2 s off every restart (defaults are 1 s / 400 ms,
`DevToolsProperties.java:87,93`). Add `spring.devtools.restart.trigger-file`
(`devtools.adoc`) when batching several saves into one restart is wanted — a
`touch` instead of a supervisor.

### 3.3 Testcontainers reuse — the biggest single number here (~8 s per run)

- `TestcontainersConfig`'s `@Bean` gets `.withReuse(true)`
  (`GenericContainer.java:1424`, `@UnstableAPI` — say so in the Javadoc). Safe
  to generate unconditionally: without the machine flag it is a no-op plus a
  warning naming the file (`GenericContainer.java:406-413`). Boot honours it
  (`TestcontainersLifecycleBeanPostProcessor.java:172,188-190`).
- The flag **must** be in `~/.testcontainers.properties` or
  `TESTCONTAINERS_REUSE_ENABLE`; a classpath `testcontainers.properties` does
  nothing (`TestcontainersConfiguration.java:284-286`). `doctor` reports it
  (reading `$HOME` is a read); a one-off `jails setup` writes it, because
  doctor never writes.
- Reused containers are **never registered with Ryuk**
  (`GenericContainer.java:419-421`) — which is why reuse works under podman,
  and why they accumulate (a real report of 172 orphans). `doctor` counts
  containers labelled `org.testcontainers.hash` and prints the `docker rm -f
  $(docker ps -q --filter label=org.testcontainers.hash)` line.
- Generated Javadoc: **the database keeps its state between runs.** Flyway has
  already run; a test assuming an empty table fails on the second run.
  Truncate in `@BeforeEach` or use a random schema.
- `spring.testcontainers.beans.startup=parallel` when a project has more than
  one container (`TestcontainersStartup.java:72`).
- `jails test <Name> --keep` (reuse for that run + print the connection
  string so `jails db` can attach) is the CLI version of Testcontainers
  Desktop's "freeze".

### 3.4 `jails test`: flags and selection

- Always pass `-o -q -ntp -Dsurefire.failIfNoSpecifiedTests=false`. The last
  is mandatory the moment any selection feature exists: Surefire's default is
  `true`, and an empty selection becomes "No tests were executed!" and a red
  build (`src/run.rs:178` passes only `-Dtest=`).
- Accept `Name#method` (Surefire: `-Dtest=MoneyTest#roundsHalfUp`, `+`-joined
  lists, `[5:*]` parameterized indices).
- `jails test src/test/java/…/MoneyTest.java:42` → jails finds the enclosing
  `@Test` method with `java::blanked()` + `java::annotations()` and emits
  `Class#method`. Jupiter never resolves a `FileSelector`
  (`DiscoverySelectorResolver.java:41-45`), so `--select-file …?line=N` on the
  console launcher parses and silently runs nothing — jails must resolve it.
  Nested classes are `Outer$Nested#method`; the Neovim ftplugin currently
  uses the filename (`ftplugin/java.lua:215`), which is wrong for `@Nested`.
- On failure print the rerun line, Rails-style: `jails test path:LINE`.
- `--fail-fast` → `-Dsurefire.skipAfterFailureCount=1`.
- `--failed`: every `<testcase>` with a `<failure>`/`<error>` child in
  `target/surefire-reports/TEST-*.xml`, re-selected. ~30 lines, no XML
  library. The console launcher (§4.2) writes the same legacy schema
  (`LegacyXmlReportGeneratingListener.java:117`), so one parser serves both.
- `--retry N` → `-Dsurefire.rerunFailingTestsCount=N`, **off by default** (a
  green build over a flake is the Failsafe failure mode again).

### 3.5 jdt.ls: six lines of settings and the bundles that unlock everything

`ftplugin/java.lua` sets no `init_options` and no `settings.java.configuration`
(`:119-162`). Add:

```lua
settings = { java = {
  configuration = { updateBuildConfiguration = 'automatic' }, -- default 'interactive': every `jails add` leaves red squiggles until prompted
  completion = { maxResults = 100 },
  autobuild = { enabled = true },                             -- state it; §3.1 depends on it
  maxConcurrentBuilds = 4,
  eclipse = { downloadSources = true }, maven = { downloadSources = true },
}},
-- and --jvm-arg=-Xmx2G
```

Bundles (`init_options.bundles`): **java-debug 0.53.2** and **vscode-java-test
0.46.0** (exclude `com.microsoft.java.test.runner-jar-with-dependencies.jar`
and `jacocoagent.jar` — the nvim-jdtls README's list is load-bearing). Then
`require('jdtls.dap').setup_dap({ hotcodereplace = 'auto' })` gives:

- `require('jdtls.dap').test_nearest_method()` / `test_class()` /
  `pick_test()` — **one test method, no Maven, no Surefire**, on jdt.ls's
  already-compiled workspace.
- Hot code replace: after a jdt.ls build, java-debug raises
  `event_hotcodereplace{BUILD_COMPLETE}` and nvim-jdtls answers with the
  custom DAP request `redefineClasses`, **with frame popping**
  (`JavaHotCodeReplaceProvider.attemptPopFrames`). Method bodies swap in
  under a second with no restart and no state loss.
- Spring Boot Tools (STS4 `spring-boot-language-server`, 5.3.0) as a third
  bundle: property completion, bean/endpoint workspace symbols (so
  `<leader>s` → `lsp_live_workspace_symbols` becomes a bean picker).

Pair it with **`jails run --debug`**: append
`-agentlib:jdwp=transport=dt_socket,server=y,suspend=n,address=127.0.0.1:5005`
through `-Dspring-boot.run.jvmArguments=…`, set
`spring.devtools.restart.enabled=false` for that run (a devtools restart kills
the JDWP session; HCR owns the loop while debugging), and ship one
`dap.configurations.java` attach entry in `jails.nvim`. Stay on nvim-jdtls:
`nvim-java` would throw away the `.session`-keyed workspace, the
Lombok-vs-CDS logic and the forced `documentChanges` fix.

### 3.6 `gf` on an import, and into JDK source — six lines, no plugin

Neovim 0.12.4's own `$VIMRUNTIME/ftplugin/java.vim` already sets
`includeexpr`, `suffixesadd=.java`, `include`/`define` (`:53-60`) and honours
`g:ftplugin_java_source_path` — a directory is prepended to `'path'`, a
`.zip`/`.jar` gets a `JavaFileTypeZipFile()` `includeexpr` (`:69-92`). Only
`'path'` is missing:

```lua
vim.opt_local.path:prepend({ root .. '/src/main/java', root .. '/src/test/java' })
vim.g.ftplugin_java_source_path = (vim.env.JAVA_HOME or vim.fn.expand('~/.local/share/jdk/jdk-27')) .. '/lib/src.zip'
```

That is grok's "`gf` while jdt.ls is cold" by configuration instead of code,
plus `[i`, `:ilist`, `:dsearch` for free — and it replaces the devdocs pin to
Java 25 (`init.lua:654`) with the actual JDK 27 source.

### 3.7 `:compiler jails` — Neovim already ships the hard part

`$VIMRUNTIME/compiler/maven.vim` (2025-11-18) carries javac errors with and
without columns, non-parseable POM, SpotBugs, and the Surefire multi-line
`<<< FAILURE!` … `at Foo.bar(Foo.java:42)` pattern. `jails.nvim/compiler/jails.vim`
is ~15 lines: `makeprg=jails …`, maven's `errorformat` (copy it verbatim
rather than `runtime!` it — the `current_compiler` guard bites), plus
`%-Gcreate\ %f`. Use `vim.system` + `setqflist({}, ' ', {lines=…, efm=…})`
(push, don't replace) for `:Jails test`; `caddexpr` for streaming `--watch`
output; keep the terminal only for `run`, `console`, `db`, `kafka`, `rename`.
`why`'s explanation goes in the quickfix *title* and a `vim.notify`, not as
entry text. `jails why --json` (`{signature, explanation, fix}` — `why.rs`
stores exactly that) lets the plugin offer the `fix:` line as a runnable
choice.

### 3.8 `jails.nvim` bugs and drift

- `setqflist({}, 'r', …)` → `' '` (`init.lua:222`). One character; stops
  `:Jails g …` from destroying the compile-error list.
- `vim.fn.termopen` → `vim.fn.jobstart(cmd, { term = true })` (`:297`).
- After any command whose output mentions `pom.xml`, call
  `require('jdtls').update_projects_config()` (`java/projectConfigurationUpdate`).
  Today `:Jails a db` leaves the buffer red until jdt.ls is restarted.
- Lists: add `toxiproxy` to `CAPABILITIES`, `app` to `SUBCOMMANDS`,
  `migrate`/`app` to `STREAMING`, `--check` to `OPTIONS.migrate`. Then a Rust
  test that pins every Lua list to `Capability::label()`, `ArtifactKind`'s
  value variants and the clap `Command` tree — or replace the lists with
  `jails commands --json` (a walk of the clap tree; `bld` ships exactly this
  for its IDE plugin).
- A `terminal` injection point in `setup()` so the plugin can reuse the
  user's `require('term').send` instead of opening a second pane.
- `project_root` should read `about --json`'s `workspace`/`module` instead of
  re-deriving from `pom.xml`.

### 3.9 `jails run` passes `-Dspring.jmx.enabled=true` when `actuator` is a capability

STS4's live hover (request-mapping call counts, bean injection timing,
`@ConditionalOn*` outcomes, active profiles) reads the actuator's **JMX**
MBeans, which Boot turns off by default (`JmxProperties.java:36`). The editor
cannot supply that flag; the process launcher can. Three lines in `run.rs`.
Unverified: whether `spring-boot.nvim` exposes a manual "connect to this
process" — test before investing further.

### 3.10 New `doctor` checks, all read-only, each a silent failure seen in the corpus

- `@EnableWebMvc` in a project with the Boot webmvc starter — switches off
  Boot's WebMvc auto-configuration; static resources stop being served
  (`ideas/minicom-public/spring/.../WebConfig.java`).
- CORS `addMapping("/**")` with no `allowedOrigins` (same file).
- `@WebMvcTest`/`@JdbcTest`/`@RestClientTest` present without the matching
  `spring-boot-starter-*-test` — Boot 4.2 moved slices into per-module
  packages (`org.springframework.boot.webmvc.test.autoconfigure.WebMvcTest`)
  and `spring-boot-starter-test` does not bring them
  (`starter/spring-boot-starter-test/build.gradle:24-42`). The generator side:
  any kind emitting a slice annotation splices its `-test` starter.
- Surefire < 3.0.0 with JUnit 6; a `junit-platform-console-standalone` whose
  major differs from the managed Jupiter (JUnit 6 unified versions: `6.x`, not
  `1.x`).
- `org.jspecify:jspecify` declared but a package under `src/main/java` with
  no `package-info.java` — silently unchecked once §8.1 lands.
- Virtual threads on, only `@Scheduled` work, `spring.main.keep-alive` unset.
- A CRaC checkpoint directory inside the repo (it is a memory image; Spring's
  docs warn twice that it contains every secret the JVM saw).
- A JVM agent on the command line together with an AOT cache — mutually
  exclusive (`filemap.cpp:1720,1747`).

And `why` rules: `keep-alive` exit-0; JFR `jdk.VirtualThreadPinned` (on by
default at 20 ms, `default.jfc:75-78`; `-Djdk.tracePinnedThreads` **no longer
exists** — a rule that recommends it is wrong on JDK ≥ 24); ECJ's
`Unresolved compilation problem` (§3.1).

### 3.11 The Neovim keymaps: five collisions, not one

`<leader>j{t,c,r,b,g}` (ftplugin: mvn test, extract constant, run main, mvn
package, generate) versus `<leader>J{t,c,r,b,g}` (jails test, check, run,
beans, generate). A shift-key slip turns "extract constant" into `mvn clean
verify`. Make the split semantic: `<leader>j` = this buffer / language server,
`<leader>J` = the project / jails. Delete `jb`; move `jr` to `jM`; route
`jt`/`jf`/`jm` through `jails test` (one Maven resolver — `about --json`
already emits `maven_command`, nothing reads it) or better through
`test_nearest_method()` (§3.5).

`javac_lint` on `BufWritePost` has three problems: it recompiles the **whole**
`src/main/java` on every save (`javac_lint.lua:74-85`); it runs bare `javac`
with **no `--release`**, i.e. JDK 26 semantics against a release-27 project
(`java_release` is already in `about --json`); and it re-runs
`dependency:build-classpath` on every pom mtime change, so the first save
after any `jails add` pays a second Maven run on top of jdt.ls's re-import.
Pass `--release`, resolve `javac` from `JAVA_HOME`, and put the autocmd behind
`vim.g.jails_javac_on_save`. Keep its output **out of** `target/classes` —
that is what stops it triggering devtools.

### 3.12 Toolchain pinning and the release line

- `new`/`new-cli` write a `mise.toml` pinning `java` and `maven` to what the
  pom targets, so doctor's most common FAIL has a copy-pasteable fix and
  "which JDK is Maven actually using" stops being a session-long mystery
  (`real_path_without_mvnd()` already proves the gate and Maven can disagree).
- `jails about` gains one health line: parent POM vs current
  (`deps/spring-boot` newest tag; 4.1.1 GA on 2026-08-21), `TARGET_RELEASE`
  vs the JDK on PATH, LTS or not.
- **Decision to record**: default to 25 (LTS) with `--java 27` opt-in, or
  keep 27 and say why. Everything this document uses is available at 25.

---

## 4. Tier 1 — the inner loop

### 4.1 `jails dev`

What it is: one process that starts the services a project's capabilities
imply, starts the app with JDWP listening, watches the right files, compiles
only what changed with a warm `javac`, swaps or restarts — and **says which
and why** — applies new migrations, and prints one timed line per action.
Quarkus' bar (4 s nag threshold, 295 ms re-test) is the number to print next
to.

Built on the research, not on the earlier sketch:

1. **Watcher** (replaces `latest_mtime`): a `HashMap<PathBuf, SystemTime>`
   over `src/main/java`, `src/test/java`, `src/main/resources/**`,
   `db/migration`, `pom.xml`, `compose.yaml`, `jails.toml`; compare with `!=`
   (editors and `git checkout` backdate); report added/changed/**deleted**;
   150–250 ms poll; 400 ms quiet period; Quarkus' extra 200 ms sleep when a
   file is size 0 (caught mid-write). Cost measured: a 9,243-file tree walks
   in 100 ms in Python; a jails project is 1–2 ms in Rust. No crate, and **no
   inotify path at all** — a second code path that only runs on some machines
   only breaks on some machines. `inotifywait`/`entr`/`watchexec` are not
   installed here anyway.
2. **Compile** the changed files (plus every class whose constant pool names
   them — §4.2's index) with
   `javac -J-XX:+AutoCreateSharedArchive -J-XX:SharedArchiveFile=.jails/javac.jsa -J-Xlog:disable --release N -cp <cached cp>:target/classes -d target/classes <files>`.
   **0.25 s instead of 1.45 s**, measured; versus `mvn compile` at seconds.
   Classpath from `mvn -q -o dependency:build-classpath
   -Dmdep.outputFile=.jails/cp.txt -Dmdep.regenerateFile=true`, re-run only when
   `pom.xml`'s hash or the parent version changes, invalidated by every
   `add`/`remove`. Fall back to `mvn compile` loudly if `javac` is missing or
   the archive misbehaves.
3. **Then one of three things, classified before acting**, because stock
   HotSpot refuses everything except method bodies
   (`jvmtiRedefineClasses.cpp:780-912,954-1228`, `jvmti.xml:8070-8079`):
   - method-body change in a class that is loaded → **swap**;
   - a `record` component, a `sealed … permits`, an annotation, a new class, a
     field, a signature → **restart**, printing the JVMTI reason by name.
     **jails' whole domain layer is records and sealed types, so every edit
     there is a restart**; a `jails dev` that promises "hot reload" without
     saying this looks broken;
   - `pom.xml` → full restart and classpath re-resolution.
   Write `target/classes/.jails-reload` only after a *successful* compile and
   point `spring.devtools.restart.trigger-file` at it, so devtools never
   restarts into a half-written directory (it polls snapshots and loops while
   they differ, `FileSystemWatcher.java:272-284`).
4. **How to swap**, in order of cost: (a) leave it to the editor — §3.5's
   `redefineClasses` with frame popping, zero jails code; (b) drive `jdb`:
   `jdb -attach localhost:5005` then `redefine com.x.Foo target/classes/com/x/Foo.class`
   (`TTYResources.java:437`, `Commands.java:2097-2140`) — ships with every
   JDK, English output, no frame popping; (c) a Rust JDWP client (handshake
   `JDWP-Handshake`, `IDSizes`, `ClassesBySignature`, `VirtualMachine/18`,
   `StackFrame.PopFrames`) — a day, structured errors. Sequence c after a and
   b work.
   **Trap**: devtools' restart classloader can make one class name resolve to
   two `ReferenceType`s, and JDI refuses ("More than one class named",
   `Commands.java:2108`). A run is *either* devtools-restart *or*
   JDWP-redefine. `new-cli` projects have no devtools, so jails-owned swap is
   also what makes `jails dev` work there at all.
5. **Services**: read `Config::capabilities()`, `compose::up` what they imply,
   print which and why (Dev Services' idea on jails' existing machinery). On
   Spring projects with `db`, the better default may be §4.4.
6. **Migrations**: a new file under `db/migration` is applied to the dev
   database immediately (`migrate.rs` has the psql path).
7. **Output**: pipe through `why::FATAL_MARKERS` (today only `run`, not
   `--watch`, does this); keep the last fatal match for `jails why --last`
   (the supervisor outlives the crashed app — `quarkus-agent-mcp`'s design
   point). Print the routes table once at boot (`inspect.rs` computes it).
8. **Keys**, Quarkus' map: `r` re-run tests, `f` failing only, `b` broken-only
   toggle, `o` toggle test output, `m` re-apply migrations, `s` force restart,
   `q` quit. Raw-mode stdin is `stty raw -echo` through `process.rs`, no crate.
9. **`--timings`** on everything; the README promises no number it has not
   printed.

Out of `jails dev`, with reasons: the AOT cache (directories on the classpath
— §1 row 2; it belongs to `jails build`/`add docker`, §8.7); CRaC (needs a
vendor JDK none of which is installed, Azul's `warp` engine removes the CRIU
privilege problem but nobody has tested Liberica under rootless podman);
JBR/DCEVM (§1 row 3); HotswapAgent/JRebel (an agent plus a DCEVM JVM or a
licence — document as opt-in, `doctor` notices `-javaagent`, do not ship).

### 4.2 `jails test --fast`, `--watch`, `--affected`, and the resident JVM

The category change is not running Maven in the loop.

- **Launcher**: splice `org.junit.platform:junit-platform-console` in test
  scope with **no version** — Boot's parent imports `junit-bom`, so it tracks
  the project's Jupiter; it `api`-depends on `junit-platform-reporting` and
  shades picocli (`junit-platform-console.gradle.kts:9,17`). One idempotent
  `pom::add_dependency`, the rule CLAUDE.md already states. Never hardcode the
  standalone jar's version (6.x unified; `~/.m2` here has both
  `junit-platform-launcher/1.14.0` and `/6.1.2`).
- **Run**: `java @target/jails/cp.args org.junit.platform.console.ConsoleLauncher execute --select-method com.x.MoneyTest#roundsHalfUp --details=testfeed --fail-if-no-tests --reports-dir target/jails/reports`,
  `cwd = module root` (Surefire's `basedir`; Flyway `filesystem:` locations
  and `src/test/resources` relative paths depend on it), through `process.rs`.
  `testfeed` streams one line per test — the `--watch` and quickfix format.
  Exit 2 = nothing selected. Spring's test support is pure Jupiter extensions
  plus `spring.factories`; nothing reads a Surefire API. `argLine` is empty in
  every jails pom today; if a capability ever adds an agent it must be
  replicated here.
- **Expected**: 0.35–0.6 s for a domain test versus 2.57 s — **unverified on
  this machine; first thing `jails bench` measures**. `neotest-java` ships
  exactly this path, which is independent evidence it works.
- **Correctness price**: compiling only the changed file is
  `useIncrementalCompilation=false`'s unsoundness (a removed method leaves a
  stale caller → `NoSuchMethodError`). The §4.2 index closes the common case;
  `static final` constants javac inlines, annotation-processor output and
  generated code stay unsound — which is why `jails check` stays
  `mvn clean verify` and every fast path falls back to it loudly.
- **`--affected`** (Quarkus' actual killer feature; neither earlier doc has
  it): a reverse-dependency index from `.class` constant pools in
  `target/classes` + `target/test-classes`. ~120 lines of Rust: magic, pool
  count, skip entries by tag width (`CONSTANT_Long`/`Double` take **two**
  slots), keep `Utf8` and `Class`; scan `Utf8` too for `L<pkg>/<Class>;` so
  descriptor-only and annotation references count. Sound for plain-Java tests
  (jails' `record`/`value`/`enum`/`sealed`/`command` kinds); blunt rules for
  Spring: any change to a `@Component`/`@Service`/`@Repository`/`@Configuration`
  class, any new file under the base package, any resource or migration change
  re-runs every context-starting test. **Unknown ⇒ run** (Quarkus'
  `getTestsToRun` falls back to everything). Exclude `*IT` from the watch
  loop by default (Quarkus' default exclude pattern; Testcontainers on every
  save is unusable), `--it` opts in. Print the count skipped; `--pretend`
  names the selection before it is trusted; `--since <ref>` takes the change
  set from `git diff --name-only`.
- **`jails testd`**: one resident JVM holding `javax.tools.JavaCompiler` and
  the launcher's `ToolProvider` named `"junit"`
  (`ConsoleLauncherToolProvider.java:30`), over a unix socket. A fresh
  `URLClassLoader` per run or edits are never seen; a test that calls
  `System.exit` kills it (jails' `command` template already avoids that).
  Target band 50–150 ms — rspec parity — unverified until built.
- **`jails bench`**: prints the ladder for *this* project on *this* machine
  (Maven lifecycle / `surefire:test` / launcher / daemon / ±container reuse).
  A tool whose pitch is speed should prove its own numbers.

### 4.3 Spring context accounting — a bigger lever than containers on a Spring suite

The cache key is `MergedContextConfiguration`: classes, initializers,
profiles (sorted), property sources, **inlined properties as an array**,
customizers, parent, loader. `@MockitoBean` does **not** dirty the context;
it changes the key via `BeanOverrideContextCustomizer.equals` over the set of
overrides (`:74-88`) — so the cost is fan-out of *distinct override sets*, and
one set per test package is a real win. `@DirtiesContext` rebuilds. Default
cache size 32 (`ContextCache.java:77`); `failure.threshold` 1 (one failed
load poisons every sibling — raise it when a flaky container bites).

`jails test --why-slow`: run with
`-Dlogging.level.org.springframework.test.context.cache=DEBUG`, read
`missCount` from the statistics line (`DefaultContextCache.java:445-468`);
statically count distinct `@SpringBootTest(properties=…)`/`@MockitoBean`/
`@Import`/`@ActiveProfiles` sets with `java::annotations()`; `jails stats`
gains "tests that start a context: N (M distinct)". New since Framework
7.0.3: inactive contexts are **paused** (`spring.test.context.cache.pause`,
default `on_context_switch`) — Kafka listeners and schedulers in a cached
context stop running while another context's tests run; `=never` is the
bisect switch if a suite changes behaviour after an upgrade.

### 4.4 `jails run --tc`: Boot's own `bin/dev`, with a real database that survives restarts

`mvn spring-boot:test-run` runs a `src/test` `TestApplication` that does
`SpringApplication.from(App::main).with(TestcontainersConfig.class).run(args)`;
container `@Bean`s carry `@ServiceConnection`; **`@RestartScope`** on the
container bean keeps it alive across devtools restarts
(`dev-services.adoc:427-516`). No compose file, so it also routes around
`spring-boot-docker-compose` being unable to drive podman-compose. It still
needs `DOCKER_HOST` (set on this machine today). `add db` can generate all of
it; `jails run --tc` (or `jails dev` on Spring+db) launches it. This is the
closest Java gets to `bin/dev`.

### 4.5 `jails boot`, `jails runner`, `g script`

- `jails boot`: `-Dspring.context.exit=onRefresh` — boots past singleton
  creation and `afterPropertiesSet`, before `Lifecycle` start and the HTTP
  port, then exits (`DefaultLifecycleProcessor.java:101`). A startup smoke
  test with no port and no compose; also the AOT training-run switch, so one
  mechanism serves two features.
- `jails runner -e '<expr>' | <file> | -` and `g script <Name>` (Rails 8.1's
  `script` generator gives one-offs a committed home): `scripts/<Name>.java`
  with `main`, run on the project classpath. Spring variant boots the context
  and `getBean`s. Say where the DataSource comes from (compose up, or `--tc`);
  the jshell-hosted-context idea fails on exactly that and on
  `spring-boot-docker-compose` not being skipped outside tests (claims-audit
  O3). Most `rails runner` use is non-interactive; this is 80 % of "console
  with beans" for a tenth of the work.
- `jails console` keeps its honest banner (jshell + classpath, not a Spring
  context) and gains a generated `startup.jsh` (`import module java.base;`,
  base package, AssertJ). Note `jails c` re-runs `dependency:build-classpath`
  on every start even with `--no-build` (`console.rs:104-130`) — use the
  §4.1 classpath cache.

---

## 5. Tier 2 — Rails parity that pays weekly

### 5.1 `jails g migration AddBodyToNote body:text!` — grow a slice

Rails' value is the *name grammar*, three regexes
(`migration_generator.rb:29-46`): `^(add)_.*_to_(.*)`, `^(remove)_.*?_from_(.*)`,
`^create_(.+)`. jails has the SQL projection (`sql.rs`) and the record reader
(`fields_from_record`); today `g migration` writes an empty file. Generate the
`alter table … add column` **and** append the component to the record, the
column to select/insert/bind/mapper, the field to the DTO, the key to both
fixture rows, the assertion to the test — six edits that must agree, done
once. Refuse to overwrite a hand-edited adapter (hash or header); print the
snippet instead, the honesty `remove` already has. `remove_*` stays out of v1
(data). Forward-only stays — Rails needed a 40-method recorder plus nine
special cases and `change_column` is *still* irreversible.

Two markers the grammar earns: `@fk(users)` (validatable — the table is in
`db/migration`), and `author:User` where `User` is a record with one `@pk` →
`author_id <that type> references users (id)`, no `ON DELETE` invented. And
`parse_fields` should reject the eight names javac refuses as record
components — `clone finalize getClass hashCode notify notifyAll toString wait`
(`Attr.java:1341-1371`, `illegal.record.component.name`) — with a sentence,
instead of emitting a file that does not compile. Note the `!` collision:
Rails' attribute `!` means `null: false`; jails' means non-blank. Different,
and jails' split (Java validation in the suffix, column constraints in
`@markers`) is the more defensible one; say so in the README.

### 5.2 `jails schema [--json] --diff`, then `g migration --auto`, then `--from-table`

The Django direction, and the one that can ship first because its first
deliverable writes nothing: **replay the migrations jails wrote** — only the
DDL subset it emits (`create table`, `alter table … add column`, `create
[unique] index`, the five markers); an unrecognised statement is an error,
not a no-op — diff against the records on disk, exit non-zero on drift. That
is a `doctor` check (`migrate --check` cannot be one; it writes). The silent
failure it removes: a record with a component the table lacks compiles and
fails at the first insert on whoever runs it first. `makemigrations --check
--dry-run`'s exit-code contract makes it a CI gate. `g migration --auto`
writes the closing migration. Composes with §5.1.

The Rails mechanism itself is **one cached SQL query** against `pg_attribute`
(`postgresql_adapter.rb:1261-1276`). `jails schema --live` runs it through the
`psql` path `jails db` already has, and `g record --from-table notes` inverts
`sql.rs`'s mapping — and because `migrate.rs` can stand up a scratch database
and apply every migration, `--from-table` works on a fresh clone with no
database running. Emit the `rails query schema` JSON shape
(`{table, columns:[{name,type,null,default}], indexes:[…]}`) — agent food.

### 5.3 Seeds, status, reset

- `db/seed.sql` with `insert … on conflict do nothing`; `jails db seed`
  refuses while migrations are pending; `--replant` truncates first. Both
  target products need it on day one (seed URLs; a workspace, an admin, a
  conversation before any screen renders).
- `jails migrate --status`: Flyway `info`'s table, including Rails'
  `********** NO FILE **********` for an applied version with no file on
  disk — the rollback-adjacent thing forward-only still leaves you needing.
- `jails db reset` = drop, create, migrate, seed. Destructive: confirm,
  `--force` to skip.

### 5.4 `jails ci`

Rails 8.1's `config/ci.rb`: named, timed steps, `-f`, an aggregate, and a
step that **runs the seeds** (a broken seed is found on a new machine at the
worst time). jails' `check` is one opaque `mvn clean verify`. `jails ci`:
format check → compile → unit tests → `migrate --check` → `schema --diff` →
`doctor` → seeds → ITs, each timed and named, `--json`. The step list is
**derived from `jails.toml` capabilities**, not configured — that is what
keeps the file a closed two-table manifest.

### 5.5 Small, cheap, daily

- `jails secret` (a 256-bit base64 key); `add security`/`add auth` write the
  secret placeholder to `application.properties` and the value to a gitignored
  `application-local.properties`, appending to `.gitignore` **and saying so**
  (`encryption_key_file_generator.rb:26-45`).
- `g resource` — scaffold minus DTOs and migration.
- `routes -g <pattern> -c <Controller>`, and the Spring inversion of
  `--unused`: a public controller method with no mapping, a route no test
  touches.
- `g controller --version 1`: Framework 7 API versioning is first-class
  (`RequestMapping.version()`, `ApiVersionConfigurer` with
  `detectSupportedVersions=true` so unknown versions 4xx with no list to
  maintain, `ApiVersionDeprecationHandler` for `Sunset` headers).
- `g client --group <name>` also writes
  `spring.http.serviceclient.<group>.base-url` and timeouts — per-site
  configuration with no client code, which is the crawler's client story.
- Generate `@Transactional` on DB-backed tests (Rails' rollback fixtures);
  document the interaction with §3.3's persistent state.
- Document `g cases` in the README (it exists and is undocumented, `migration.rs:88-123`).

---

## 6. Tier 3 — the two products

Both are `app.toml` files once the kinds exist (§1 row 7). Each new kind has
to be expressible in the closed `[[generate]]` schema — `kind`, `name`,
`fields`, `indexes`, `package`, `strategy_on`, `strategy_yields` — so `g
timeline --parts …` and `g webhook --strategy …` each add a key, and unknown
keys stay fatal.

### 6.1 The crawler: `jails add crawl` + `jails g spider <Name>`

Why jails should own it: the three Monzo solutions in `ideas/` have, between
them, a `HashMap` visited set with unsynchronised reads
(`monzo-crawler/.../CacheService.java:11,26`), the trailing-slash rule
*commented out* (`LinkUtils.java:30`), a crawl that never terminates
(`CrawlerService.java:35`), **every relative link silently dropped** because
`Jsoup.parse(response)` was called with no base URI and the tests use only
absolute hrefs (`monzo-crawler2/.../WebParser.java:176,182`,
`WebParserTest.java:21`), a check-then-act race across the fetch
(`monzo-web-crawler/.../crawler.go:77,93`), "depth" that counts path
segments, a normaliser that force-rewrites `https` (untestable against a
local server), an injected HTTP client that is dead code — and **zero**
robots.txt implementations. Not hard decisions; decisions nobody makes twice
correctly.

**The cut** (CLAUDE.md's `add kafka`/`g event` precedent): the capability
knows no site and owns the engine; the generator owns the seed and scope.

| | `add crawl` (alias `spider`) | `g spider <Name> --seed <url> [--scope host\|domain\|any] [--cache]` |
|---|---|---|
| dependencies | `org.jsoup:jsoup:1.23.1` (2026-07-30; StormCrawler's pin), `com.github.crawler-commons:crawler-commons:1.6` (2024-12-04; pinned by StormCrawler and Nutch; RFC 9309 has not moved) | none |
| writes | `Url`, `Scope`, `Robots`, `Politeness`, `Fetcher`/`HttpFetcher`/`Fetched`, `Links`, `Frontier`, `Crawler`, `PageHandler`/`CrawledPage`/`CrawlReport`, and `FakeSite` in the testkit layer | `<Name>Spider`, its `CrawlConfig` defaults, one `PageHandler`, the nine tests; registers the `Crawl` command in `new-cli` projects (via `register_command`) or an `ApplicationRunner` on Spring; `g job Recrawl` for recurring |
| `deps.tsv` | two new lines: `jsoup	jhy/jsoup`, `crawler-commons	crawler-commons/crawler-commons` — templates are written against checkouts, not memory | |

Key types (full signatures in `ideas/fable-research/crawler-shapes.md §8.2`):

- **`Url`** — a record over a normalised `URI`; the dedup key *is* the value;
  `resolve(href, base, config) -> Optional<Url>` **never throws** (one
  `mailto:` must not lose a page's links).
- **`Scope`** — `HOST` (the Monzo brief: `monzo.com` but not
  `community.monzo.com`), `DOMAIN` (`EffectiveTldFinder.getAssignedDomain`,
  free in the crawler-commons jar, `ideas/nutch/.../URLUtil.java:33,142`),
  `ANY`. Katana's `fqdn`/`rdn` vocabulary, not a boolean.
- **`Robots`** — one call into `SimpleRobotRulesParser.parseContent(url,
  bytes, contentType, List.of(robotName))`; `ALLOW_ALL`/`ALLOW_NONE` sentinels;
  4xx ⇒ allow all, **5xx or unreachable ⇒ disallow all** (RFC 9309
  §2.3.1.4 — the rule everyone gets backwards; StormCrawler's
  `http.robots.5xx.allow: false`), cache 24 h, read cap 512 KiB, sitemaps from
  `getSitemaps()` for free.
- **`Politeness`** — per host: `Semaphore(1)` + a next-allowed instant.
  Acquire the permit **first**, then sleep the gap (the Go solution does it
  backwards and collapses its own concurrency). Per-host concurrency is 1 by
  construction (StormCrawler, Nutch, colly's own doc comment) — not a property.
- **`Fetcher`** (interface) + **`Fetched`** (sealed: `Page`/`Skipped`/`Failed`)
  — the loop is a `switch` with no `default`. `HttpFetcher`: `Redirect.NORMAL`
  with the javadoc's caveat that https→http is **not** followed
  (`HttpClient.java:682-684`); record `HttpResponse.uri()` (the *final* URL)
  and re-scope it; `Accept: text/html`; check `Content-Type`; body cap 4 MiB;
  `Retry-After` on 429/503; retry with full jitter on 5xx/`IOException` only,
  never 4xx; `HttpClient` is `AutoCloseable` (since 21) — close it or the CLI
  hangs on the selector thread.
- **`Frontier`** — `offer(url, depth) -> boolean` is the **only** dedup gate
  (`ConcurrentHashMap.newKeySet().add`); termination is `enqueued ==
  completed` with the enqueue inside the same critical section as the
  parent's completion (`monzo-crawler2`'s `EngineObserver`, the one thing it
  got right). Never `queue.isEmpty()`.
- **`Crawler`** — `Executors.newVirtualThreadPerTaskExecutor()` (AutoCloseable
  since 19) + a `Semaphore(concurrency)` acquired on the *submitting* thread;
  `frontier.completed` in `finally`. `StructuredTaskScope` is
  `@PreviewFeature` at `jdk-27+34` and HEAD (JEP 533); `ScopedValue` is final
  and is the right carrier for depth/request-id across the tasks.
- **`PageHandler`** — colly's `OnHTML` as one functional interface collected
  into a `List`, i.e. the `g strategy` shape: a new extractor is `jails g
  strategy PageHandler …` and nothing else.

Normalisation: always resolve against the base (incl. `<base href>`),
lowercase scheme and host (IDN → punycode), uppercase percent triplets and
decode unreserved, remove dot segments, drop default port, empty path → `/`,
drop the fragment. Policy, each a named property with a sourced default: sort
query params (**on**), drop a closed list of tracking params
(`utm_*`, `gclid`, `fbclid`, `msclkid`, `mc_cid`, `mc_eid`, `igshid` — **on**),
drop all query params (**off**, katana's `-iqp`), fold trailing slash
(**off** — `/help` and `/help/` are different resources; dedup on the
redirect target instead), **never rewrite the scheme**. Idempotence is the
property test: `normalise(normalise(u)) == normalise(u)`.

Properties, every default sourced (katana, StormCrawler `crawler-default.yaml`,
Nutch `nutch-default.xml`, RFC 9309): `user-agent` with a contact URL,
`robot-name`, `scope=host`, `max-depth=3`, `max-pages=10000`,
`max-duration=10m`, `concurrency=10`, `delay=1s`, `max-crawl-delay=30s`
(skip the host above it), `obey-robots=true` (colly defaults this **off** —
do not copy), `connect/request-timeout=10s`, `max-body-bytes=4194304`,
`max-robots-bytes=524288`, `max-retries=2`, the skip-extension list.

Tests — all tier 1, pure JDK, no container — over `FakeSite`
(`page/redirect/status/robots/slow/raw/hits`): a relative link is followed;
one page reached four ways is fetched once; a cycle terminates (under
`assertTimeoutPreemptively`); an off-domain link is not fetched; a subdomain
is out under `host` and in under `domain`; a redirect records the target; a
404 and a 500 do not stop the crawl and only the 500 is retried; `Disallow`
is honoured and robots is fetched once; a 5xx robots.txt disallows the host;
a non-HTML content type is skipped; normalising twice changes nothing.

Later, its own capability: **`add browser`** — `lightpanda/browser:nightly`
in compose on 9222 (123 MB vs 2 GB, 5 s vs 46 s per 100 pages by its own
benchmark; Beta; AGPL — a separate process over CDP keeps the project out of
scope), Playwright Java `connectOverCDP` as a second `Fetcher`. Which CDP
domains a Beta browser answers is the unverified part; try it first.

Build order: `Url` + property test (½ d), `FakeSite` (½ d), `Frontier` +
`Crawler` + `Fetched` with the cycle/dedup tests (1 d), `Robots` +
`Politeness` (1 d), templates + `deps.tsv` + README (½ d). Ship the first
three as `add crawl` before robots: a crawler that terminates and never
double-fetches already beats all three references.

```toml
# examples/crawler/app.toml
schema = 1
capabilities = ["json", "testkit", "crawl"]
[[generate]]
kind = "spider"
name = "Monzo"
seed = "https://monzo.com/"     # new key for this kind
scope = "host"
```

### 6.2 The Intercom shape: slices, then one `app.toml`

What `minicom` actually is: two tables, five form-encoded POSTs, identity
from a **client-writable cookie holding an email**
(`testapp-www/index.html:98`), no conversations, no realtime (a full-page
reload after send), no jobs, no mail, a dead route, a direction flag that is
overwritten, and — in the Spring port — allow-all CORS plus `@EnableWebMvc`.
What a real one needs, from Intercom's own docs: JWT identity (HS256 only,
`user_id` the only mandatory claim, delivered as the `intercom_user_jwt`
boot attribute, `exp` "strongly recommended" — jails requires it); SHA-1
webhooks keyed by `client_secret`; conversations as first-class rows with
**15 typed `conversation_parts`** where assignment/close/snooze are parts,
not updates; a denormalised statistics row the inbox sorts on; inbound email
as a *forwarded* channel; attachments as two-step uploads; search;
`workspace_id` on everything; rate limits enforced in 10-second sub-windows;
WebSocket for the widget (CSP docs name `nexus-websocket-*.intercom.io` over
both `wss:` and `https:`), which means SSE for the agent inbox is not
obviously worse.

Each slice follows the capability/generator cut and names the silent failure
it prevents. Full designs, SQL and signatures are in
`ideas/fable-research/intercom-shapes.md §5-6`.

- **`add sse`** — `SseHub` (`subscribe(topic)`, `emit(topic, event, data)`,
  `replayFrom(topic, lastEventId)`), `GET /stream/{topic}` as
  `text/event-stream`, `new SseEmitter(0L)`, all three callbacks
  (`onCompletion`/`onTimeout`/`onError`, `ResponseBodyEmitter.java:318,335,352`)
  removing the emitter, a 15 s `: keep-alive` comment, `.reconnectTime()`
  (the `retry:` field — control the browser's backoff from the server),
  `spring.threads.virtual.enabled=true`, `spring.mvc.async.request-timeout=-1`.
  `Last-Event-ID` replay is `select … where conversation_id = ? and id > ?
  order by id` — visible SQL. IT over a real port with `RestTestClient`
  (the Framework 7 blocking client that drives a real server; `MockMvc` has
  no connector). Prevents: the 30 s silent reconnect loop, the leaked
  emitter map, the idle-proxy kill that shows as a 200 in the access log.
- **`add queue`** — the ActiveJob/Solid Queue analogue in plain JDBC, the
  largest parity gap; `g job` is a *timer*. One `jobs` table (`jsonb`
  payload, `state in (ready,running,done,dead)`, `run_at`, `attempts`,
  `max_attempts`, `locked_at/by`, `idempotency_key`), a **partial index on
  `state='ready'`** (the claim stays O(1) as `done` rows accumulate), a
  unique partial index on the idempotency key. Enqueue **in the caller's
  transaction** (the adapter takes the caller's `Connection`, as `g scaffold`'s
  does — the property Redis queues cannot give). Claim: `select … for update
  skip locked limit ?` **inside** a CTE, `order by run_at, id`, `attempts`
  incremented **at claim** (a SIGKILLed worker has burned its attempt, so an
  OOM-killing job cannot loop forever). Failure: full-jitter backoff capped at
  an hour, `dead` at `max_attempts`. A **reaper** every minute for
  `running` rows with a stale `locked_at` — without it a `kill -9` leaves a job
  `running` forever and nothing logs it; this, not `SKIP LOCKED`, is what is
  usually missing. Worker: a semaphore acquired **before** claiming, virtual
  threads, `LISTEN jobs_ready` on a dedicated non-pooled connection via
  `PGConnection.getNotifications(1000)` (`PGConnection.java:76`) with polling
  as the durable fallback, graceful shutdown with `server.shutdown=graceful`.
  Tests: two workers × 100 jobs = exactly 100 runs; a throwing job backs off;
  `dead` is never claimed; shutdown completes in-flight work. Off the shelf:
  `db-scheduler-spring-boot-4-starter` 16.11.0 is the same design plus
  cluster coordination — name it in the Javadoc as the graduation path;
  JobRunr is a framework with a dashboard and does not fit the bar.
- **`add mail`** — `spring-boot-starter-mail` + `spring-boot-starter-mail-test`
  (Boot 4's `-test` twin convention: splice `X-test` with every `X`), a
  `Mailer` with one method per email, Mailpit `axllent/mailpit:v1.19.0` in
  compose (1025/1110/8025), the IT reading mail back over **POP3** as Boot's
  own test does, `Mailer.send` enqueuing through `add queue` when present
  (`deliver_later`). No `@ServiceConnection` factory exists for SMTP — a
  `DynamicPropertyRegistrar`, and say why in the Javadoc.
- **`add auth`** — the Rails 8 shape, not JWT-for-everything: sessions as
  **rows** via `spring-boot-starter-session-jdbc` (exists in 4.2; no Redis),
  `admins` + Spring Session's schema as migrations, `POST /login`/`DELETE
  /logout`, `DelegatingPasswordEncoder` (`{bcrypt}` default,
  `PasswordEncoderFactories.java:72-90`), rate limit **in the generated code**,
  the enumeration-safe reset message, destroy all sessions on password
  change, `last_authenticated_at` for sudo mode (Phoenix 1.8), conditional on
  what is installed (websocket auth only if the realtime capability exists).
  It must turn **CSRF back on** in the same change — `add security`'s
  stateless/CSRF-off chain is safe only together. Tests assert both
  directions and that stored hashes start with `{bcrypt}$2`.
- **`g messenger-token <Name>`** — HS256 mint/verify with **no dependency**
  (`Mac`/`HmacSHA256` + url-safe base64 + the `add json` mapper, ~40 lines,
  auditable); `exp` **required**; `POST /widget/token` behind the host
  session. Tests: round trip, expired, wrong secret, missing subject, and
  **`alg: "none"`** — the CVE class a signature-only verifier accepts.
  Prevents exactly minicom's cookie hole.
- **`g webhook <Name> --strategy hmac_sha1_hex|hmac_sha256_hex|stripe_v1`** —
  header derived from the strategy, `@RequestBody byte[]` (signature over raw
  bytes, never re-serialised JSON), `MessageDigest.isEqual`, per-webhook
  secret property, 401/202, Stripe tolerance check mandatory. Tests include a
  body with a non-ASCII character. Inbound email is this generator pointed
  at Postmark/SendGrid/Mailgun plus a threading rule on
  `In-Reply-To`/`References` against stored `Message-ID`s with a
  plus-addressed fallback — never the subject line; do not run an MX.
- **`g timeline Conversation --parts COMMENT,NOTE,ASSIGNMENT,OPEN,CLOSE,SNOOZED,…`**
  — the generator neither doc has and the one the data model needs:
  `<Name>PartType` enum, an append-only `<Name>Part` record + table with
  `(conversation_id, id)` index, a port with `append`/`list(after, limit)`
  and its derived JDBC adapter, a `Projection.fold(parts)` **switch with no
  `default`** (add a part type, the build stops until its effect on state is
  decided), and the denormalised `conversations` row (`state`, assignees,
  `waiting_since`, `snoozed_until`, `last_part_at`, `part_count`) updated **in
  the same transaction**. Tests: `comment, assignment, close,
  note_and_reopen` folds to `open` with `count_reopens == 1`; a rollback
  leaves neither write. Prevents the inbox saying "open" while the timeline
  says closed. `g scaffold` would generate `update … set assignee = ?` and
  lose the audit trail.
- **`add storage`** — `ObjectStore` port (`presignPut/Get`, `delete`),
  `S3Presigner` adapter (`software.amazon.awssdk:s3` + bom, from
  `deps/aws-sdk-java-v2`), MinIO in compose (9000/9001,
  `MINIO_ROOT_USER/PASSWORD`), `org.testcontainers:testcontainers-minio` (2.0
  naming). **`forcePathStyle(true)`** and the endpoint override, proven by a
  test that uploads through the JDK `HttpClient` to the presigned URL (what
  the browser does) — virtual-host addressing fails against MinIO with a DNS
  error that reads like a network problem.
- **`g search <Entity> --on body,subject`** — a `generated always as (…) stored`
  `tsvector` column (a trigger someone forgets to fire on UPDATE is the
  silent failure), a GIN index, `websearch_to_tsquery` (does not throw on
  user input; `textsearch.sgml:796`), `ts_rank_cd`, keyset pagination. No
  search service.
- **`add widget`** — serve the static messenger script from
  `src/main/resources/static/` with a **narrow** CORS config from an explicit
  origin property, and a test that an unlisted origin is rejected. Plus the
  two doctor checks from §3.10.
- **`--tenant-column workspace_id`** on `g scaffold`/`g timeline` (every table
  from V1; retrofitting tenancy is the most expensive migration in this
  shape) — a flag, not a capability.
- **The UI decision**: an agent inbox needs HTML. Recommendation: `g page
  <Name>` with **JTE** (`gg.jte:jte-spring-boot-starter-4`, 3.2.3 on
  2026-02-10 — confirm the newest at build time) + htmx + `add sse`. JTE
  templates are compiled, type-checked Java (a renamed record component breaks
  the build, not the request — jails' bar), and `gg.jte.developmentMode=true`
  runs **its own** watcher, so template edits are sub-second with no context
  restart and no dependence on the deprecated LiveReload. One layout, one
  fragment convention, no asset pipeline; the widget stays hand-written JS.
  API-only is the alternative; it leaves the take-home rendering nothing.

```toml
# examples/minicom/app.toml — what exists today vs what §6.2 adds
schema = 1
capabilities = ["db", "json", "api", "testkit", "format", "security",
                "auth", "queue", "mail", "sse", "storage", "widget"]   # last six: new

[[generate]]
kind = "scaffold"
name = "Contact"
fields = ["id:uuid@pk", "workspaceId:uuid@index", "email:string!@unique", "name:string?", "createdAt:instant"]

[[generate]]
kind = "scaffold"
name = "Admin"
fields = ["id:uuid@pk", "workspaceId:uuid@index", "email:string!@unique", "name:string!", "teamId:uuid?"]

[[generate]]
kind = "enum"
name = "ConversationState"
fields = ["OPEN", "CLOSED", "SNOOZED"]

[[generate]]
kind = "timeline"                       # new
name = "Conversation"
parts = ["COMMENT", "NOTE", "ASSIGNMENT", "OPEN", "CLOSE", "SNOOZED", "UNSNOOZED", "NOTE_AND_REOPEN"]

[[generate]]
kind = "search"                         # new
name = "ConversationPart"
fields = ["body"]

[[generate]]
kind = "messenger-token"                # new
name = "Messenger"

[[generate]]
kind = "webhook"                        # new
name = "InboundEmail"
strategy = "hmac_sha256_hex"

[[generate]]
kind = "page"                           # new
name = "Inbox"
```

One file, one `jails app apply`. Order of slices by what minicom cannot exist
without: `sse` → `queue` → `mail` → `auth` → `messenger-token`/`webhook` →
`timeline` → `storage`/`search`/`widget`/`page`.

---

## 7. Tier 4 — the editor and the agent

### 7.1 `about --json` v2, and line numbers

Add `layout` (through `Config::layers()`, i.e. *renamed* values — the drift
`inspect.rs` already suffered once), `base_package`, `capabilities`,
`java_root`/`test_root`; pin the keys to `config::LAYERS_IN_ORDER` with a
test. Normalise the version key: `about` uses `schema_version`, `routes`/`beans`
use `version` (`project.rs:131` vs `inspect.rs:226,485`) — fix before a
fourth JSON emitter. Add `offset` to `java::Annotated` and `line` to
`Route`/`Bean` (`blank_range` already preserves newlines for exactly this,
`java.rs:150-159`); emit `primary` in `beans --json` (tracked, never
serialised, `inspect.rs:242,475-482`). Without a line, `routes --json` is a
list; with one it is a quickfix and picker source.

### 7.2 vim-rails navigation via projectionist, not a Lua reimplementation

`tpope/vim-projectionist` (finished, 1.1k lines, dormant since 2024-12 —
stability, pin it) fires `User ProjectionistDetect`, and a plugin supplies
projections **in memory** with `projectionist#append(root, projections)` —
nothing written into the repo, which dissolves grok's anti-goal. jails.nvim:
on detect, run `jails about --json` once per root (invalidate on
`BufWritePost jails.toml,pom.xml`), build the table from `layout` +
`base_package`, append. You get `:A`/`:AS`/`:AV`/`:AT` with a **list of
alternates, first readable wins** (controller → its test, else its service;
JDBC adapter → its IT, else the in-memory one, else the port), `:Econtroller`
`:Eservice` `:Erepo` `:Emigration` `:Etest` `:Efixture` with completion,
`related`, `path` (so `gf` works), `make: jails` (which auto-loads
`compiler/jails.vim`), `dispatch: jails test`, `console: jails console`,
`start: jails run`, and `{dot}`/`{capitalize}` transforms for package↔path.
Degrade to a minimal `:JailsAlternate` when the plugin is absent. Several
hundred lines of Lua not written or tested.

### 7.3 Pickers (fzf-lua), `jails src`, and the rest

- `:JailsRoutes`/`:JailsBeans`: `require('fzf-lua').fzf_exec` over
  `routes --json`/`beans --json`, jumping to `source:line`. Sub-50 ms on a
  project that does not compile — jdt.ls cannot do that.
- `jails src <Type>`: resolve a project type, else a type under `deps/`
  (filename stem, then `^public (class|interface|record|enum) X\b`), print
  `file:line`; `why`'s `fix:` lines cite it (`jails src DeadLetterPublishingRecoverer`).
  `deps/` is a real checkout with `git log`; `gd` through jdt.ls is a source
  jar download of possibly another version. The JDK half is §3.6.
- `jails symbols --json` (types, methods, `file:line` via `java.rs`) as a
  fallback document/workspace-symbol source while jdt.ls is cold.

### 7.4 Agents: AGENTS.md first, `--json` everywhere second, MCP third

You develop with Claude Code and Cursor; both read **AGENTS.md** (Linux
Foundation's AAIF, 60k+ repos, nearest-file precedence). `jails agents`
writes one — the exact command list (from the clap tree), capabilities and
layout (from `jails.toml`, renames applied), the field-spec grammar, and the
standing instruction "run `jails g <kind>` rather than writing these files;
run `jails doctor` before claiming it works" — as a **marked block that
`add`/`remove`/`generate` keep true**, the same rule as the manifest, or it
is a file somebody has to remember. An agent that calls `jails g scaffold`
writes nine correct files in one step instead of nine subtly-wrong-for-Boot-4
ones (`@MockBean`, the `@AutoConfigureMockMvc` package, Jackson 2 vs 3 — all
things CLAUDE.md records jails already knows). `jails explain <kind>` exposes
the design rationale the Javadoc and CLAUDE.md carry, so the agent stops
"fixing" `@Repository` onto the second adapter.

Then `--json` on `doctor`, `why`, `stats`, `test` (one object per `testfeed`
event), `schema`, `notes`, `add --list`, `commands` — jails has the emitter
and the `schema_version` convention; this also makes every Neovim picker
trivial and `jq` useful today. `jails context --json` aggregates.

Then, and only then, **`jails mcp`**: stdio JSON-RPC, `tools/list` +
`tools/call` over functions that already return JSON; the 2026-07-28 MCP
revision retired `initialize` and session ids, so it is a line loop plus a
JSON **parser** jails does not have (~300 lines) — ~600–800 lines total, zero
crates, tool schemas as `templates/mcp/*.json` via `include_str!`.
Precedent: `quarkusio/quarkus-agent-mcp`. Pin the protocol version like
`TARGET_RELEASE`. No LLM inside jails, ever: deterministic generation is the
product, and the moment `g spider` asks a model for a selector it cannot be
golden-filed or destroyed.

### 7.5 Not a Rust LSP

`jails lsp` is ~1,200–1,800 lines (a correct JSON parser, LSP framing,
lifecycle, document sync with an in-memory overlay, then features), a
long-lived process on a one-shot CLI, a second server fighting jdt.ls over
`.java` buffers, and the pressure that turns `java.rs` into the parser
CLAUDE.md says it must never become. If a unified editor surface is ever
wanted, Neovim runs an **in-process Lua language server** (`:h lsp-server`;
`runtime/lua/vim/pack/_lsp.lua` is a 268-line shipped example) whose handlers
shell out to `jails … --json`. Zed/VS Code would each need a ~250-line shim —
a cost deferred to whoever wants that editor.

---

## 8. Quality, architecture, operations — capabilities that enforce what jails already declares

1. **`add nullcheck`** — `am.ik.maven:nullability-maven-plugin:0.4.3` wires
   Error Prone + NullAway (`onlyNullMarked`, `RequireExplicitNullMarking`) and
   injects the `--add-exports` itself (no `.mvn/jvm.config`). It turns the
   `@NullMarked` `package-info.java` jails already writes from documentation
   into enforcement. Turn its `generate-package-info` **off** (two writers of
   one file). Gate on Error Prone supporting the toolchain — JDK 27 is
   unproven there — the way tier-3 tests are gated.
2. **`add arch`** — ArchUnit with `FreezingArchRule`: the first run records
   violations to a committed `archunit_store/` and passes; the baseline can
   only shrink, so it ships today on an existing project. Rules from
   `Config::layers()`: `web` must not depend on `adapters` except through
   `app`; `domain` depends on nothing; no `@Repository` under `adapters` when
   a `DataSource` exists (doctor's textual check, made permanent). Prefer over
   Modulith (a dependency plus a package convention).
3. **`jails doc [--format md|mermaid]`** — Modulith's `Documenter` idea with
   no dependency: a mermaid module graph from `Config::layers()` +
   `java::type_info`, the route table and the bean table from `inspect.rs`.
   Generated from code rather than maintained. The natural body of AGENTS.md.
4. **Structured logs as `why`'s substrate** — Boot ships ECS/GELF/Logstash
   JSON behind one property (`logging.structured.format.console`,
   `LoggingSystemProperty.java:95`). Dev-only via §3.2; `why` matches
   `error.type`/`error.stack_trace` fields when the line is JSON and falls
   back to the regex table otherwise; `jails logs --level ERROR --logger
   com.x` (Laravel's `pail`); `run_watched` re-renders JSON to readable lines
   so humans never see it. Needs the small JSON parser from §7.4.
5. **`jails regen [--branch jails/regen]`** — replay `jails.toml`/`app.toml`
   with the current jails onto a branch from the merge-base, commit, print
   `git merge jails/regen` (never run it). Bootify's git-based update; the
   golden harness already produces the tree; git owns conflict resolution, so
   no ownership hashes. `rails app:update`'s per-file prompt and dated
   `new_framework_defaults` file are the UX to borrow if a merge is too
   blunt.
6. **`jails upgrade [--dry-run]`** — OpenRewrite fully qualified
   (`mvn org.openrewrite.maven:rewrite-maven-plugin:<pinned>:dryRun
   -Drewrite.recipeArtifactCoordinates=… -Drewrite.activeRecipes=…`) — no pom
   edit at all, which satisfies "surgical edits only" by making none;
   `--dry-run` default, prints `target/rewrite/rewrite.patch`. Twice a year,
   a day instead of a week. (Refaster cannot do jails' real migrations —
   annotations, imports, method names — skip it.)
7. **`jails build --extract` / `add aot` / `add docker`** — `java -Djarmode=tools
   -jar app.jar extract` is 15.7 % off start for free and the prerequisite
   for the AOT cache; `add aot` trains on the *extracted* jar with
   `-Dspring.context.exit=onRefresh`, uses `-XX:AOTMode=auto` (never
   `required`; a stale cache degrades instead of refusing to boot), and
   `doctor` flags a cache older than any jar. Deploy-time and short-lived
   workers only (crawler workers started per job are the real beneficiary).
   `add docker` writes the two-stage Dockerfile Boot documents
   (`dockerfiles.adoc:66-94`).
8. **`jails scratch <name>`** — a compact-source-file (`void main()`, JEP 512
   final at 25) with `//DEPS` lines from the resolved dependencies, run via
   `jbang` if present (not installed here) else the cached classpath; three
   sharp edges to encode: `-proc:none`, one-way classpath visibility, and a
   same-named class in `target/classes` is a hard error. **`--promote <Name>
   --as command|class|record`** turns a scratch that earned its keep into a
   generated artifact with its test — the inverse of `jbang export maven`,
   and the mechanic that makes "just try something" feed the generators.
9. **`jails add --list [--json]`, `jails new --list-deps`, `jails commands --json`**
   — the closed sets, printable and machine-readable (`spring init --list`,
   `mn create-app --list-features`, bld's IDE JSON).
10. **`src/codemod.rs`** — collect the six splice primitives jails already
    has (`pom::add_dependency/plugin`, compose marked blocks, property blocks
    + `exposure_include`, `register_command`, `install_test_container_import`,
    the `jails.toml` one-liner) under named operations. Same extraction as
    `process.rs`, for the same reason. A refactor, not a feature; pays on
    every capability.
11. **Housekeeping the research surfaced**: Toxiproxy's Java bodies are still
    `format!` strings with doubled braces (`add/testing.rs:150-438`) — finish
    the template migration; no test asserts every `ArtifactKind`/`Capability`
    has a golden `Scenario`; `jails start` should pass `-p <project>` so two
    jails projects' compose stacks cannot collide on container names; fix
    CLAUDE.md's `deps/deps.tsv` path.

---

## 9. What not to build, and why

| Temptation | Why not |
|---|---|
| A Rust `jails lsp` | §7.5 — 1.2–1.8k lines, a JSON parser, a second server on `.java`, and parser pressure on `java.rs` |
| Migrating to `nvim-java` | discards three correct fixes in `ftplugin/java.lua` for ~15 lines of bundle install |
| neotest / neotest-java, overseer.nvim | `jdtls.dap.test_nearest_method()` gives the no-Maven single test through bundles wanted anyway; `lua/term.lua` already fits `jails run` better than a task runner |
| Snippets generated from `templates/*.java` | a snippet that duplicates `jails g class` is worse than `jails g class`; the existing snippets were measured |
| The AOT cache in `jails dev`/`test` | directories on the classpath; devtools restarts are not process starts (§1 row 2) |
| CRaC now | no CRaC JDK installed; vendor-only; the privilege story under rootless podman is untested; the checkpoint is a memory image with secrets |
| JBR / DCEVM / HotswapAgent / JRebel now | JBR ≤ 25 vs release 27; agents plus a DCEVM JVM or a licence move the decision off jails |
| Maven 4 | 4.0.0-rc-6 (2026-08-04), not GA; nothing in it moves the inner loop; `mvnup` only after GA |
| `useIncrementalCompilation=false` in generated poms | trades ~200 ms for stale-dependent `NoSuchMethodError`s |
| Making `jails check` incremental; Maven build cache extension | leftover classes; a cache that restores a `target/` the `clean` just deleted |
| `TESTCONTAINERS_RYUK_DISABLED` by default | buys 0.45 s, loses all crash cleanup; reuse already bypasses Ryuk where it matters |
| Resilience4j for crawl politeness; WireMock for crawler tests; wrapping crawler4j/webmagic/Nutch/StormCrawler/spider/crawl4ai | global limiter is the wrong shape for per-host concurrency 1; WireMock drags Jetty + Jackson 2 into a Boot 4 test scope; the brief forbids frameworks and jails generates its own types |
| Refaster rules | expressions and statements only — cannot rewrite annotations, imports or method names, which are all three of jails' real migrations |
| A `jails recipe`/template DSL, `jbang app install`-style catalogs, Spring CLI's user-defined commands, AdonisJS assembler hooks | plugin systems under other names; `jails app` is the closed-schema answer |
| Booting a Spring context inside jshell | dies on the DataSource, tries to drive podman-compose, and is slower than `jails run`; `jails runner` and the attach-to-running-app path are the honest designs |
| `StableValue`/`LazyConstant`, `StructuredTaskScope`, primitive patterns in generated Java | preview at 27 (JEPs 531, 533, 532) |
| Browser refresh on devtools LiveReload | deprecated in 4.1.0, off by default; `add sse` is the durable path |
| An LLM inside jails | deterministic output is the product |

---

## 10. Order of work

Ordered by value ÷ effort, respecting dependencies. S < 1 day, M 1–3 days,
L > 3 days, in jails-shaped days (golden scenario, README, nvim lists, enum,
no plugin).

| # | Item | Effort | Unblocks |
|---|---|---|---|
| 1 | Editor config: jdt.ls settings + bundles + HCR, `'path'`/`src.zip`, `:compiler jails`, keymap split, `javac_lint --release`/opt-in (§3.5–3.7, 3.11) | S, no Rust | test-at-cursor, debugging, `gf`, quickfix — today |
| 2 | `jails.nvim` fixes + list-pinning test; `jails run --debug`; `-Dspring.jmx.enabled` (§3.8, 3.9) | S | the plugin stops lying |
| 3 | Testcontainers reuse + doctor + `jails setup`; `META-INF/spring-devtools.properties` defaults; `mise.toml` from `new` (§3.2, 3.3, 3.12) | S | −8 s per container test, −1.2 s per restart |
| 4 | `jails test`: flags, `Name#method`, `path:line`, rerun snippet, `--failed`, `--fail-fast`, `--keep` (§3.4) | S–M | every test run |
| 5 | doctor/why additions (§3.1, 3.10) | S | stops the silent ones |
| 6 | `about --json` v2 + line numbers; projectionist; fzf-lua pickers (§7.1–7.3) | M | `:A`/`:E*`, routes/beans jump |
| 7 | `jails test --fast` (console launcher + classpath cache) + `jails bench` (§4.2) | M | the 2.5 s → 0.5 s loop, measured |
| 8 | `jails dev` v1: watcher fix, `javac` + CDS, trigger file, `why` piping, migrations, keys, timings (§4.1) | L | edit → see under 1 s for method bodies; honest restarts for records |
| 9 | `--affected` + the constant-pool index; `--why-slow` context accounting; `run --tc` (§4.2–4.4) | M–L | Quarkus-grade selection; Spring suites explained |
| 10 | `jails schema --diff` (doctor) → `g migration Add…` / `--auto` → `--from-table`; seeds; `migrate --status`; `jails ci` (§5) | L | the week after scaffold |
| 11 | `add crawl` + `g spider` + `deps.tsv` lines (§6.1) | L (3–4 d) | the Monzo crawler in 30 minutes |
| 12 | Intercom slices in dependency order (§6.2) | L each | minicom in an hour |
| 13 | `jails agents` + `--json` everywhere; `jails src`; `explain` (§7.4) | M | every agent session |
| 14 | `add nullcheck`, `add arch`, `jails doc`, structured `why`, `regen`, `upgrade`, `add aot`/`docker`, `scratch --promote`, `codemod.rs` (§8) | S–M each | as they come up |

A first week that changes the feel: 1–5 are config and small Rust and they
compound immediately; 6 and 7 are the two that change the day. Do not start
`g spider` before 4 and 7 — a spider you test with `mvn clean verify` is the
old experience with more files.

---

## 11. Measure before promising

The research marked these as the unverified numbers and facts a feature
would rest on. `jails bench` exists to answer the first four.

1. Console-launcher wall time here (est. 0.35–0.6 s) and the resident-JVM band
   (est. 50–150 ms).
2. How many distinct Spring contexts a jails scaffold's suite actually builds
   (`missCount` under the DEBUG category) — 1, 2 or 3 decides how much §4.3
   is worth.
3. Whether the Surefire/test JVM can use an AOT cache at all given
   `target/test-classes` is a directory (the jdk source says no; jar the
   classes for the training run and measure).
4. `postgres:17` with reuse: confirm ~0 s on the second run under podman.
5. Does a `@SpringBootTest` with Testcontainers succeed on this machine
   today? `DOCKER_HOST` is set and the socket is active, which contradicts
   the documented gotcha. `JAILS_REQUIRE_TOOLCHAIN=1 JAVA_HOME=~/.local/share/jdk/jdk-27 cargo test`.
6. Where jdt.ls writes `.class` files in this setup (`target/classes` via m2e
   is the expectation; devtools prints the classpath at startup). The whole
   §3.1 loop pivots on it.
7. Whether `spring-boot.nvim` can manually attach STS4 live data to a
   `jails run`-launched process.
8. Whether Playwright Java `connectOverCDP` gets enough CDP out of Lightpanda
   to fetch a page.
9. Whether `CSVFormat.Builder.build()` still exists at commons-csv 1.14.1
   (add the checkout to `deps.tsv`).
10. The newest `gg.jte:jte-spring-boot-starter-4` and
    `com.microsoft.playwright:playwright` versions (both single-sourced; Maven
    Central returned 403 to the fetcher).

---

## 12. Research log

Twelve read-only passes on 2026-08-21, each a markdown report with `file:line`
and URL citations, copied to `ideas/fable-research/` (untracked, like the
rest of `ideas/`):

| report | what it covers |
|---|---|
| `repo-map.md` | every enum, layer, JSON shape, `run --watch`/`test`/`console` mechanics, golden harness, `java.rs` limits, `jails.nvim` drift, `g cases`, doctor and why tables, extension points |
| `machine.md` | every JDK (no CRaC, no JBR, no GraalVM), Maven/mvnd (the native-library wrapper), Neovim 0.12.4 plugins and keymaps, jdt.ls 1.60.0/1.61.0 with no bundles, podman 5.8.4 with `DOCKER_HOST` set |
| `rails-catalog.md` | every Rails generator and command against jails (73-row parity table), the migration grammar, Solid Queue/Cache/Cable schemas, why a Rails save is free, the ten gaps |
| `crawler-shapes.md` | the three Monzo solutions read line by line, the modern feature matrix, crawler-commons/jsoup/Lightpanda facts, normalisation and robots rules, the full crawler slice design |
| `intercom-shapes.md` | what minicom is, Intercom's identity/webhook/conversation/inbox/email/search/tenancy/rate-limit facts, the Spring 4.2 mapping, the capability set, the `SKIP LOCKED` queue in full |
| `jdk-facts.md` | preview vs final at `jdk-27+34` and HEAD, AOT flags and constraints from HotSpot source, CRaC engines and flags, JVMTI redefinition limits, the JDWP spec, the source launcher, virtual-thread diagnostics, LTS status |
| `spring-facts.md` | release train (4.1.1 / 7.0.8 GA), devtools internals and the `defaults.*` layer, startup knobs that are already on, context caching and pausing, Boot 4 testing APIs and module map, SSE/WebSocket/virtual threads, mail, templating, actuator endpoints, spring-cli status |
| `hot-reload.md` | Quarkus live reload and continuous testing internals, Micronaut, JBR/HotswapAgent/JRebel, java-debug HCR, `jdb redefine`, measured `javac` CDS numbers, the watcher design, the ranked `jails dev` stack |
| `test-loop.md` | this machine's Maven/container/test-class numbers, every Surefire flag, `useIncrementalCompilation`, the console launcher verified in JUnit source, JUnit 6, constant-pool test-impact analysis and its blind spots, Testcontainers reuse under podman, the ranked plan |
| `neovim.md` | nvim-jdtls vs nvim-java, STS4 live hover over JMX, jdt.ls preferences and the `target/classes` finding, projectionist, `compiler/maven.vim`, dap/test bundles, neotest-java, the `jails lsp` verdict, line-referenced conflicts in this config |
| `java-dx-tools.md` | mechanics from Quarkus, mill, mvnd, bld, Spring CLI/OpenRewrite, Error Prone/NullAway/Refaster/ArchUnit/Modulith, Testcontainers reuse cost, jbang, JHipster/Bootify/Micronaut, Phoenix/Django/Laravel/AdonisJS, MCP and AGENTS.md, with a 28-row adoption table |
| `claims-audit.md` | 28 specific claims from the two earlier documents checked against source, plus ten more things they missed |

Versions read: `deps/spring-boot` `e3d4b1ceb6d` 2026-08-14 (4.2.0-SNAPSHOT;
tags to `v4.1.0`), `deps/spring-framework` `526c706d1c` 2026-08-19
(7.1.0-SNAPSHOT; tags to `v7.0.8`), `deps/jdk` `9a601b46b2f` 2026-08-19
(`jdk-28+11`; `jdk-27+34` is the newest 27 tag, no `jdk-27-ga` yet),
`deps/testcontainers-java` 2.0.5, `deps/junit-framework` 6.2.0-SNAPSHOT,
`deps/rails` `a757a27584` 2026-08-20 (8.2.0.alpha), `deps/tomcat` 12.0,
`ideas/stormcrawler` v39, `ideas/browser` `accb34eaa` 2026-08-21.

Three things the research could not do and a session should: run anything
(no `cargo`, `mvn`, `podman` were executed — every latency not marked
"measured" is an estimate); read the design stage (a limit hit twice; the
synthesis above is mine from the twelve reports, not a fourth agent's); and
see the tree as it is now — `jails app`, `src/app.rs` and the root
`deps.tsv` appeared during the session, so anything here that contradicts the
working tree defers to the working tree.
