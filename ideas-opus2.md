# ideas-opus2.md — the measured one

Written 2026-08-21 against the jails tree at `25b7394`, the upstream checkouts
in `deps/`, the reference projects in `ideas/`, this machine's Neovim config,
and — the part that makes this document different — **the tool itself, run**.

There are now four other documents in this repo proposing what to build:
`ideas-opus.md` (24 KB, ordered by loop latency), `ideas-grok.md` (43 KB,
vim-rails + `deps/` + closed slices), `ideas-kimi.md` (48 KB, a synthesis plus
K1–K21), and `ideas-sol.md` (143 KB, nine "bets" for an application compiler,
whose Bet 6 has already partly shipped as `jails app` / `.jails/app.toml`).

All four say some version of *measure first*. `ideas-kimi.md` K1 says it
outright: "every idea in every doc claims a time win… today nothing measures
them." `ideas-sol.md` principle 9 says "measure before adding exotic
acceleration."

**Nobody measured.** This document is the measurement, and the bug report that
came out of it. Its rule: *nothing goes in here that I did not run.* Where I
could not run something, it is marked UNVERIFIED and it is not load-bearing.

It also measures the half the other four barely touch. They are all, including
`ideas-opus.md`'s "ordered by loop latency," about **how long you wait**. The
other budget is **how much you type** — the generator surface, where a missing
command does not cost you seconds of waiting but an afternoon of mechanical
edits. §5 counts that one, and per occurrence it is the larger number.

Read the other four for design breadth. Read this one for what is true.

---

## 0. The short version

There are **two** loops, and they are not the same problem. Latency is what you
pay hundreds of times a day and it is what the other four documents optimise.
Ergonomics is what you pay every time the *shape* of the code changes — and
when it is missing you do not wait, you type. Both are measured here.

1. **Maven is 97% of your inner loop, and none of it is Java's fault.** One
   unit test costs 3.8 s through Maven, 1.65 s through the JUnit launcher
   directly, and **11 ms** when the JVM stays alive. `java Hello.java` runs in
   0.6 s. The language is not slow; the build tool is. §1.
2. **The generator budget is wildly asymmetric.** One `g scaffold` writes
   **1,180 lines in 39 ms** and you write none of it. Adding **one field** to
   that scaffold is **six files, ~17 edit sites, plus a hand-written
   migration**, and there is no command for it. That asymmetry is why
   `g scaffold` is the first afternoon and the rest of the week is not. §5.
3. **A relation is silently not stored.** `g scaffold Post … author:User` emits
   `author text not null` and an adapter that does not persist it — the app
   starts, `POST /posts` returns 201, the author is gone. `User.java` is on
   disk with `id:uuid@pk` and jails already reads records off disk elsewhere.
   §5.3.
4. **Six live defects, found by using the tool for an afternoon.** Two of them
   make jails write a project that does not build. One makes it write Java that
   does not compile. §2.
5. **The reason the test suite missed all six**: three tiers plus a golden
   tier, and **not one of them asks whether the generated project builds**. The
   golden files currently *ratify* a broken `pom.xml`. §3.
6. **The workload you named cannot run jails at all.** Intercom's minicom stub
   is Gradle + Boot 2.7.18 + H2 + JDK 21; every jails command exits with
   `no pom.xml found`. The fix is 11 lines in one function — not Gradle
   support. §7.
7. **And minicom is not the exercise.** It is a connectivity smoke test whose
   entire success condition is an alert saying `Yay! Everything works`. Both
   earlier documents built Intercom generators against a product spec that does
   not exist. §8.
8. There is no 1000x on anything. There is a measured ~345x on one loop, a
   measured 0→8 on another, and six defects whose cost is unbounded because
   they are silent. §10 does the arithmetic honestly.

---

## 1. The inner loop, timed

Method: `tests/golden/scaffold-spring` copied to a scratch dir (Boot 4.1.0,
Java 26 target, 13 files, 3 unit tests in `NoteServiceTest`), `mvn -o` so
nothing touches the network, `~/.m2` warm at 602 MB. JDK 26.0.2 on PATH,
JDK 27-ea available at `~/.local/share/jdk/jdk-27`. Load average under 5 for
the primary run.

| What | Time | How it was measured |
|---|---:|---|
| JVM boot (`java -version`) | **0.03 s** | `/usr/bin/time`, 3 runs, identical |
| `mvn -v` (launcher only) | 0.22 s | 2 runs |
| `java Hi.java` (source launcher: compile in memory + run) | **0.61–0.94 s** | 3 runs |
| `mvn -o -q compile`, nothing changed | **2.27 s** | no-op incremental |
| `mvn -o -q test -Dtest=NoteServiceTest`, cold | 5.88 s | first run |
| `mvn -o -q test -Dtest=NoteServiceTest`, warm | **3.81 s / 3.90 s** | 2 runs |
| `mvn -o -q test` (whole module) | 4.43 s | — |
| `javac` main + test, all 13 files, cold | 3.19 s | `--release 26` |
| JUnit Platform `Launcher`, precompiled, fresh JVM | **1.65–1.87 s** | 3 runs |
| **Same test, 2nd run inside the same JVM** | **9–13 ms** | 5 iterations |
| **One test file recompiled via `JavaCompiler` API, warm** | **74–166 ms** | 5 iterations |

The last two rows are the whole story. Here is the actual output:

```
run 0: 3 ok, 0 failed, 1278 ms      <- class loading + engine discovery
run 1: 3 ok, 0 failed, 13 ms
run 2: 3 ok, 0 failed, 13 ms
run 3: 3 ok, 0 failed, 11 ms
run 4: 3 ok, 0 failed, 9 ms

compile 0: ok=true, 837 ms          <- javac warming up
compile 1: ok=true, 166 ms
compile 2: ok=true, 118 ms
compile 3: ok=true, 107 ms
compile 4: ok=true, 74 ms
```

**Edit one test, recompile it, rerun it: ~110 ms of real work.** Maven charges
3,810 ms for the same thing. The overhead is not the JVM (30 ms), not javac
(~100 ms warm), and not JUnit (11 ms warm). It is Maven: project model
resolution, plugin descriptor loading, and a forked JVM per invocation.

I re-ran the headline pair later under load average 23 (eight research agents
running): Maven 3.68–6.55 s, warm-JVM repeat 18–24 ms. Absolute numbers move;
**the ratio does not**.

### 1.1 What this kills

`ideas-opus.md` A2 says the JDK AOT cache is "pure win: no code change, no
risk," paid back on "every devtools restart, every `mvn test` fork, every
`jails run`." I tested it rather than grepping for it.

The flags are real. `~/.local/share/jdk/jdk-27/bin/java -XX:+PrintFlagsFinal`
lists `AOTCache`, `AOTCacheOutput`, `AOTConfiguration` and `AOTMode` as
`{product}` on both JDK 26 and 27-ea, and one-step training with
`-XX:AOTCacheOutput=` works — no separate `AOTMode=record` pass needed. But:

```
$ java -XX:AOTCacheOutput=test.aot -cp "runner/out:target/classes:...:$(cat cp.txt)" Run ...
[12.885s][error][aot] Error: non-empty directory 'runner/out'
[12.886s][error][aot] Error: non-empty directory 'target/classes'
Error occurred during CDS dumping
```

**The AOT cache refuses any classpath containing a directory.** `target/classes`
is a directory. So is `target/test-classes`. Every `mvn test` fork, every
`spring-boot:run`, and every `jails run` uses exactly that layout. A2's
mechanism does not apply to any of the three loops it was proposed for.

Where it *does* apply — a jar classpath, i.e. `java -jar` after `jails build` —
it works: same test, jar classpath, 6.6–7.9 s without the cache and 2.96 s
with (measured under load; the ratio is ~2.4x). The cache was **36 MB** for a
13-file project.

So AOT is real, and it belongs in the `jails build` / deployment story with a
`.gitignore` line and a staleness check — not in the dev loop. `ideas-opus.md`
A2 is not wrong about the flags; it is wrong about where the win lives.

Two smaller corrections from the same session, both from strand-level source
reads I then spot-checked:

- `-XX:+AllowEnhancedClassRedefinition`, on which `ideas-opus.md` A1 builds its
  headline "~90% of real edits" claim, **does not exist in OpenJDK**:
  `grep -rl AllowEnhancedClassRedefinition deps/jdk/` returns nothing. It is a
  JetBrains Runtime / DCEVM flag. This machine runs an EA OpenJDK 27
  (`mise.toml`). Stock JVMTI redefinition is method bodies only —
  `deps/jdk/src/hotspot/share/prims/jvmti.xml:8136-8140`, "must not add, remove
  or rename fields or methods" — with
  `JVMTI_ERROR_UNSUPPORTED_REDEFINITION_METHOD_ADDED` as the failure. A new
  `@GetMapping`, a new field and a new bean all fall back to a restart. The
  honest pitch is "method-body edits are instant, structural ones restart."
- A devtools restart does not relaunch the JVM at all — `RestartClassLoader`
  reloads application classes in the same process
  (`deps/spring-boot/module/spring-boot-devtools/.../restart/classloader/RestartClassLoader.java`),
  so there is no JVM start cost for an AOT cache to save there either.

---

## 2. Six defects, each found by running the tool

Not code review. I typed the commands a user types.

### 2.1 `g scaffold` and `g dto` write a `pom.xml` Maven cannot read

```
$ jails new-cli bench --no-git
$ cd bench && jails g scaffold Note id:uuid@pk title:'string!' body:text
...
     dep org.springframework.boot:spring-boot-starter-validation
$ mvn -o test
[ERROR] 'dependencies.dependency.version' for
        org.springframework.boot:spring-boot-starter-validation:jar is missing.
[ERROR] The build could not read 1 project
```

Root cause, `src/generate.rs:1054-1057`:

```rust
match kind {
    ArtifactKind::Dto | ArtifactKind::Scaffold => {
        ensure_dependency(&root, &crate::spring::VALIDATION_STARTER)?
    }
```

`VALIDATION_STARTER` is `version: None` (`src/spring.rs:29-35`), which is
correct under a `spring-boot-starter-parent` and fatal without one. There is no
flavor check: `require_spring_project` guards only `client`, `job` and `event`
(`src/generate.rs:668, 680, 692`). `dto` reproduces it identically — verified
on a second fresh project.

`maven-failsafe-plugin` is also spliced without a `<version>` into the same
pom; Maven downgrades that one to a warning, so it is cosmetic today and a trap
tomorrow.

**Fix.** Either gate both kinds behind `require_spring_project`, or — better,
since `scaffold` genuinely is useful on plain Maven — have `ensure_dependency`
consult `pom::flavor` and splice a pinned version when there is no parent BOM.
The second is what `add`'s own capabilities already do for every non-Spring
dependency.

### 2.2 The golden test snapshots the broken pom

`tests/golden/scaffold-plain/pom.xml:14-17`:

```xml
<dependency>
    <groupId>org.springframework.boot</groupId>
    <artifactId>spring-boot-starter-validation</artifactId>
</dependency>
```

The golden harness pins every byte jails generates. It pinned these. A test
suite that captures the output of a broken code path and asserts it does not
change is not a regression test — it is a ratchet holding the bug in place.
See §3.

### 2.3 `g controller` / `g scaffold` write Java that cannot compile on Boot 3.0–3.3

`templates/generate/controller_stub_test.java` imports `MockMvcTester`
unconditionally. That type is `@since 6.2`
(`deps/spring-framework/spring-test/src/main/java/org/springframework/test/web/servlet/assertj/MockMvcTester.java:129`),
i.e. Spring Boot ≥ 3.4 — confirmed from the BOM itself:

```
$ git -C deps/spring-boot show v3.3.0:gradle.properties | grep springFramework
springFrameworkVersion=6.1.8
$ git -C deps/spring-boot show v3.4.0:gradle.properties | grep springFramework
springFrameworkVersion=6.2.0
```

Meanwhile the only version sniff jails has, `spring_boot_major`
(`src/generate.rs:187-206`), resolves the **major** only and gates just the two
`@AutoConfigureMockMvc` / `@WebMvcTest` package moves. So on a Boot 3.2 project
jails emits the legacy `@AutoConfigureMockMvc` package **and** the 6.2-only
`MockMvcTester` — two mutually exclusive choices in one file. Reproduced by
generating into the Boot 2.7 stub, where both were emitted together.

CLAUDE.md already flags the general shape of this ("don't hardcode the import
again"). The lesson the incident actually teaches is stronger: **the sniff
pattern does not generalise.** Nine distinct API families would have to fork to
support Boot 2.7 output (`MockMvcTester`, `@MockitoBean`, `JdbcClient`,
`@ServiceConnection`, `@ImportHttpServices`, Jackson 3 vs 2, Testcontainers 2
package renames, `jakarta` vs `javax`, the Boot-4-only `spring-boot-flyway`
artifact), against a golden suite pinned to exactly one Boot version
(`tests/common/mod.rs:191`). That is the maintenance trap, quantified.

**Fix, and it is the cheap one:** resolve a framework version rather than a Boot
major, and when `framework < 6.2`, **refuse with a fix line** instead of growing
a second template family. Add `--no-test` as the escape hatch; it is worth
having anyway.

### 2.4 `jails test 'Class#method'` silently runs the wrong thing

`src/run.rs:166-179` appends `Test` to any filter that does not end in
`Test`/`Tests`/`IT` and contains no `*`. So:

| you type | jails runs |
|---|---|
| `jails test 'MoneyTest#roundsDown'` | `mvn test -Dtest=MoneyTest#roundsDownTest` |
| `jails test 'MoneyIT#x'` | `mvn test -Dtest=MoneyIT#xTest` — Surefire, not Failsafe |

Both match nothing and both exit successfully. This matters more than it looks:
`ideas-opus.md` ranks `jails test <file>:<line>` #1 in its build order and
`ideas-grok.md` §14.2 proposes pointing `<leader>jm` at `jails test`. Neither
noticed that the destination is broken, and grok's version would **regress a
working keymap** — `ftplugin/java.lua:217-221` already resolves the enclosing
method with treesitter and passes `Class#method` to raw `mvn`, which works.

**Fix, and it is two changes rather than one.** Split on `#` first and suffix
only the class part — but also move the Failsafe decision, because
`src/run.rs:174` tests `ends_with("IT")` against the **already-mangled** string.
So `CheckoutIT#happyPath` becomes `CheckoutIT#happyPathTest`, no longer ends in
`IT`, and routes to `mvn test -Dtest=` instead of `verify -Dit.test=`. Fixing
the suffix without moving the routing check leaves the `IT` case still broken.

### 2.5 `jails run --watch` cannot report a failed startup

CLAUDE.md documents the gotcha at length: `mvn spring-boot:run` exits 0 over a
dead application because devtools runs `main` on `restartedMain` and catches
there. `run::run_watched` (`src/run.rs:83-155`) is the fix — it pipes the
output and scans for `why::FATAL_MARKERS`.

It has exactly one caller: `src/run.rs:372`, the **non-watch** Spring branch.
`watch()` (`src/run.rs:264-322`) spawns `mvn spring-boot:run` with a bare
`.spawn()` and inherited stdio. The documented fix does not apply to the
documented loop. A watched app that dies on restart says nothing.

While there: `latest_mtime` (`src/run.rs:325-346`) only stats `.java`, so
editing `application.properties` or dropping a migration triggers nothing.

### 2.6 `doctor` reads `java` on PATH, not `JAVA_HOME`

```
$ JAVA_HOME=~/.local/share/jdk/jdk-27 jails doctor
FAIL  jdk   project targets Java 27, but `java` on PATH is 26
      fix: use a JDK 27+ (`mise exec java@27 -- jails ...`, or set JAVA_HOME)
9 checks: 1 failing, 0 warning(s)     # exit 1
```

`JAVA_HOME` *is* set, to a JDK 27. Maven would use it. CLAUDE.md itself records
that "this shell does not run mise's activation hook, so `java` on a bare PATH
is still 26" — which means **every `jails doctor` on this machine emits a false
FAIL by default**, and the fix line names the very thing you already did. A
health check that cries wolf on the default configuration is a health check
people learn to ignore, and `doctor` is the command CLAUDE.md wants you to
reach for first.

Worse, on the broken project from §2.1 the same run printed:

```
ok    maven           mvnd on PATH (no wrapper)
ok    beans           3 bean(s), every project-typed dependency resolvable
```

on a pom Maven refuses to parse. Nothing in `doctor` asks the build tool
whether the build tool can read the project.

### 2.7 The drift, which is the same root cause wearing different clothes

- **`jails g cases` is implemented and completely absent from README.** It is
  dispatched at `src/generate.rs:1344`, is `ArtifactKind::Cases`
  (`src/generate.rs:64`), and lives in `src/generate/migration.rs:88-245`.
  `grep -c cases README.md` → 1 hit, at line 566, about sample values.
  CLAUDE.md calls README "the spec… update it in the same change as the code."
  `ideas-grok.md` §10 calls this generator "under-sold." It is worse than that:
  it is undiscoverable, and it is the most agent-shaped generator in the repo.
- **`jails.nvim` completion is stale in four places.** `CAPABILITIES`
  (`jails.nvim/lua/jails/init.lua:65-81`) has 15 entries against
  `Capability`'s 16 — **`toxiproxy` is missing** — plus every alias
  (`postgres`, `errors`, `metrics`, `faults`). `KINDS` (:39-63) is missing
  `repository`, `mig`, `it`. `RUNTIMES` (:82) is missing `postgres`. `OPTIONS`
  (:124-154) never lists the global `--debug` on any subcommand, lists
  `--pretend` on only four of the ~20 commands that accept it, and omits
  `migrate --check`. `SUBCOMMANDS` is the one list that is actually current.
- **`CLAUDE.md` itself names two files that do not exist.** Line 542: "`deps/deps.tsv`
  is the manifest… and `deps/update.sh` clones what's missing." The files are
  `deps.tsv` and `deps-update.sh`, both at the repo root; `deps/` holds only the
  80-odd checkouts. This one matters more than it looks, because CLAUDE.md is
  the first thing every agent on this project reads — the wrong path propagated
  into roughly six citations across the research behind this document before
  anyone checked it.
- **`validation/README.md`'s status table is seven features out of date.** It
  is dated 2026-08-14 and blocks nine of ten workouts on `list<T>`,
  `map<K,V>`, `instant`, `g repo`, `g sealed`, `g handler` — all of which have
  since shipped. That table is the first thing a reader uses to decide what
  jails can do.

---

## 3. The hole all five documents share

jails has an unusually good test story on paper: three tiers, a documented rule
that tier 2 must not masquerade as tier 3, `JAILS_REQUIRE_TOOLCHAIN=1` so a
skipped tier-3 test fails loudly, and 159 golden files pinning every byte.

**None of it asks whether the generated project builds.**

- Tier 1 tests pure functions.
- Tier 2 tests which argv jails hands a fake `mvn`.
- Tier 3 compiles — but only the combinations someone wrote a test for, and
  `new-cli` + `g scaffold` is not one of them.
- The golden tier compares bytes to bytes. It cannot tell a correct pom from
  an unparseable one; §2.2 is the proof.

That is why §2.1 and §2.3 shipped, and it is a structural blind spot, not an
oversight. CLAUDE.md already contains the exactly-right sentence one level
down — *"a skipped tier-3 test is reported as passing"* — and the same
reasoning applies one level up: **a snapshotted artifact is reported as
correct.**

### The fix: a build matrix, and it is small

A tier-3 test that walks the cross product of `{new-cli, new-spring}` ×
`{scaffold, dto, repo, handler, command, event, client, job}` × `{no
capability, add db, add json}` and, for each cell, runs **`mvn -o validate`**
(not compile — validate is enough to catch every versionless dependency and
costs ~2 s). Gate it the way the other tier-3 tests are gated, and add it to
`JAILS_REQUIRE_TOOLCHAIN`.

Two cells would have failed today. The whole matrix at `mvn validate` costs
under a minute, which is affordable as a pre-push gate even if it is too slow
for `cargo test`.

**This is the highest-leverage item in this document that is not a feature.**
Every one of the five documents proposes new generators. Generators are exactly
what this gap silently breaks.

---

## 4. Tier 0 — the day that pays for itself

Everything in §2 and §3, in order, plus the drift. This is roughly one focused
day and it is strictly ahead of every feature in every document, because
features land on top of it.

| # | Item | Where | Effort |
|---|---|---|---|
| 1 | Version the validation dep (or gate `scaffold`/`dto` on Spring) | `src/generate.rs:1054`, `src/spring.rs:29` | 30 min |
| 2 | Regenerate `tests/golden/scaffold-plain` and **read the diff** | `tests/golden/` | 10 min |
| 3 | The `mvn validate` build matrix | `tests/cli.rs` | 2–3 h |
| 4 | Split `Class#method` before suffixing | `src/run.rs:166` | 20 min |
| 5 | Route `--watch` through `run_watched`; widen `latest_mtime` to resources | `src/run.rs:264-346` | 1 h |
| 6 | `doctor` resolves `JAVA_HOME` first, then PATH; report which it used | `src/doctor.rs` | 30 min |
| 7 | `doctor` runs `mvn -o -q validate` — or at minimum `pom::read` must FAIL loudly rather than `unwrap_or_default` | `src/doctor.rs:117` | 1 h |
| 8 | Framework-version resolver; refuse below 6.2 with a `fix:` line; add `--no-test` | `src/generate.rs:187-217` | 2 h |
| 9 | README section for `g cases` | `README.md` | 15 min |
| 10 | A Rust test pinning `jails.nvim`'s four Lua tables to the ValueEnums, then fix the four gaps | `tests/cli.rs`, `jails.nvim` | 1 h |
| 11 | Refresh `validation/README.md`'s status table | `validation/` | 15 min |

Item 7 deserves a note. `doctor` is read-only by contract and `mvn validate`
writes nothing, so it stays inside the contract — but it does shell out, and
that is a judgement call about whether `doctor` should be instant. The minimum
version (make an unparseable pom a FAIL rather than silently `default()`) is
free and catches §2.1 without a subprocess.

Item 10 is the one that stops the class of bug rather than an instance of it.
Every idea in all five documents adds an enum variant; each one would otherwise
silently fail to complete.

---

## 5. Tier 1 — the generator surface, where the keystrokes actually are

Everything above this line is latency, and so is most of the other four
documents. Latency is the loop you pay hundreds of times a day. **The generator
surface is the loop you pay every time the shape of the code changes** — and
unlike latency, when it is missing you do not wait, you *type*. It is the
difference between a change taking a command and a change taking an afternoon
of mechanical edits across six files.

So here is that budget, measured with the same discipline as §1.

Note up front that every idea in this section is **domain-blind**, so all of it
sits inside the line `src/app.rs:1-6` draws (see §8.6). Growing a record,
resolving a foreign key, pluralising a noun and reading a field spec are not
crawler features or inbox features. They are the generic machinery both
manifests in `examples/` are written on top of.

### 5.1 The budget, measured

Method: `jails new-cli inbox`, `add sqlite`, `add json`, then one scaffold —
the minicom `messages` table, spelled as jails wants it:

```
jails g scaffold Message id:uuid@pk userId:uuid@index content:'string!' \
    read:boolean sentAt:instant
```

| | |
|---|---:|
| Commands typed | **3 + 1** |
| Java files in the project afterwards | 25 |
| Lines of Java | **1,180** |
| Lines *I* wrote | 0 |
| Time for the scaffold command itself | **0.039 s** |

That is jails at its best, and it is why the tool exists. `jails` itself costs
39 ms — the generator is never the bottleneck.

Now the second row of the budget, which nobody has measured. **Add one field.**

`grep -rl content src/` on that project:

| File | Occurrences | What has to change |
|---|---:|---|
| `domain/Message.java` | 5 | component, `requireNonNull`, trim, blank check, Javadoc |
| `adapters/JdbcMessageRepository.java` | 5 | select list, insert columns, `values` placeholders, bind, row mapper |
| `web/MessageRequest.java` | 2 | component + mapping |
| `web/MessageResponse.java` | 2 | component + mapping |
| `db/migration/V002__create_messages.sql` | 1 | and you cannot edit it — migrations are forward-only, so you hand-write a new one |
| `test/resources/fixtures/messages.json` | 2 | one per fixture row |
| **6 files** | **~17 sites** | **plus a migration written from scratch** |

And there is no command for it. `jails g --help` has no `field`, no `column`, no
`alter`. Re-running the scaffold with the extra field does not work either:

```
$ jails g scaffold Message id:uuid@pk userId:uuid@index content:'string!' \
      read:boolean sentAt:instant priority:int
jails: .../src/test/resources/fixtures/messages.json already exists
```

**One command creates 1,180 lines; changing one field is six files by hand.**
That asymmetry is the whole ergonomic story, and it is why `g scaffold Note` is
the first afternoon and the rest of the week is not.

`ideas-grok.md` §6 identifies this and is right. What it gets wrong is the
mechanism — it assumes a jails-owned marker or hash already exists "same idea
as capability property blocks". It does not: property blocks are delimited by
`# jails:<label>` comments, but generated **Java** carries nothing. The only
ownership oracle is `edited_files` (`src/add/database.rs:371-379`), nine lines
that re-render the current template and diff the bytes. That is enough to ship
`g field` today, as long as you are honest about what it can and cannot tell —
see below.

### 5.2 E1 — `jails g field <Type> <field:type>`

The single highest-value generator jails does not have.

```
jails g field Message priority:int
jails g field Message archivedAt:instant?
```

It reads the record with `fields_from_record` (exists), refuses if the
component is already there, appends it in declaration order, and then rewrites
**only the derived files that still match what jails would have written**,
using `edited_files` as the oracle. For each file that has drifted it prints
the exact snippet instead of overwriting:

```
updated  domain/Message.java
updated  web/MessageRequest.java
updated  web/MessageResponse.java
created  db/migration/V007__add_priority_to_messages.sql
skipped  adapters/JdbcMessageRepository.java -- you have edited this file
         add to the select list:  priority
         add to the insert:       priority
         bind:                    ps.setInt(6, m.priority())
         map:                     rs.getInt("priority")
```

That refusal is the design, not a limitation. The oracle cannot distinguish
"you edited this" from "jails changed its template", so on any jails upgrade it
will over-report — and over-reporting prints a snippet you paste, while
over-writing silently destroys work. Print, never clobber.

The migration is emitted from `sql.rs`, which already owns the column
projection, as `alter table messages add column priority integer not null`.
Forward-only, never an edit to `V002`. A `not null` column added to a table
with rows needs a default, so the generated SQL carries one for non-optional
fields and says so in a comment.

**`--remove` is deliberately not in v1.** Dropping a column is a migration you
write by hand because of the data. Adding is the 95% case.

Test: a golden scenario that scaffolds, runs `g field`, and snapshots the
record, both DTOs, the adapter, the new migration and the fixture; plus a unit
test that a hand-edited adapter is skipped rather than rewritten.

### 5.3 E2 — relations, which are the biggest silent gap

Measured. `User.java` is on disk with `id:uuid@pk`. Then:

```
jails g scaffold Post id:uuid@pk author:User title:'string!'
```

The migration:

```sql
-- jails could not derive a column type for: author.
-- Those are guesses; correct them before this runs anywhere real.
create table posts (
  id      uuid not null,
  author  text not null,
  title   text not null,
  ...
```

and the JDBC adapter:

```java
* Not persisted, because jails has no mapping for the type: author.
```

**The author is silently not stored.** The scaffold compiles, the app starts,
`POST /posts` returns 201, and the author is gone. jails is honest about it in
two comments — which is consistent with its philosophy and is *not* enough,
because a comment in a generated file is exactly what nobody reads.

And the information needed is right there. `User` is a record with a single
`@pk` component of type `uuid`, sitting in the domain package, and jails
already reads records off disk for `g repo` and `g dto`.

**The rule, closed and small:** if the referenced type is a record in this
project with exactly one `@pk` component — or, failing that, a component named
`id` — persist `<name>_id` with that component's SQL type, and emit
`references <pluralised type> (<pk column>)` in the create table. The Java
component stays `User`, unchanged; the *adapter* stores the id and the service
loads it. No lazy loading, no proxies, no ORM, no `@ManyToOne`. This is
`belongs_to` without ActiveRecord, and it is the single change that takes
relations from "silently dropped" to "a column and a foreign key".

Anything jails cannot resolve this way — a type with no pk, a collection, a
type it has never seen — keeps today's behaviour exactly: named in the Javadoc,
named in the migration comment, not guessed at. **Do not invent `on delete`
behaviour**; omit it and document that the database default applies.

Why this is the biggest one: relations are roughly half of real modelling, and
this is the only defect in the whole document whose symptom is *lost data* in a
project that looks like it works.

### 5.4 E3 — the pluraliser is `+ "s"`, and it is visible in the URL

Measured, by scaffolding six nouns into one project:

| Type | route | table | fixture | should be |
|---|---|---|---|---|
| `Category` | `/categorys` | `categorys` | `categorys.json` | categories |
| `Company` | `/companys` | `companys` | `companys.json` | companies |
| `Box` | `/boxs` | `boxs` | `boxs.json` | boxes |
| `Analysis` | `/analysis` | `analysis` | `analysis.json` | analyses |
| `Person` | `/persons` | `persons` | `persons.json` | people (or persons — defensible) |
| `Status` | `/status` | `status` | `status.json` | statuses |

The rule is "append `s` unless it already ends in `s`". `/categorys` is the kind
of thing you notice in a demo, and fixing it by hand means editing the
controller path, the table name, the migration, the DTO mapping and renaming
the fixture file — the same six-file spread as §5.2, for a typo you did not
make.

**The good news is that it is one owner.** Route path, table name and fixture
filename all already derive from the same place, which is exactly the "one
column list feeds the DDL, the select, the insert, the bind and the row mapper"
principle the codebase is built on. So one closed inflector fixes all three at
once: `…y` after a consonant → `ies`; `…s|x|z|ch|sh` → `es`; `…f|fe` → `ves`;
plus a short irregular table (`person/people`, `analysis/analyses`,
`status/statuses`, `child/children`) and an uncountable list (`data`, `info`).
About 60 lines and a table test.

Keep it closed and keep it out of `jails.toml`. A per-project override would
mean the table name is no longer derivable from the type name, and derivability
is the whole reason `destroy` can find what `generate` wrote.

### 5.5 E4 — three kinds read the record off disk; the one you want does not

`g repo Draft` with no field spec reads `Draft.java` and derives everything.
So does `g dto`. `g scaffold Draft` does not:

```
$ jails g record Draft id:uuid@pk title:'string!' body:text
$ jails g scaffold Draft
jails: .../domain/Draft.java already exists
```

So the natural workflow — model the type first, then generate the machinery
around it — is blocked on the one kind that spans the most files. It should
read the record and generate everything *except* the record, exactly as
`g repo` does.

More generally there should be **one rule**, stated once: a kind's fields come
from the spec if given, else from the record on disk if there is one, else it
is an error. Today that rule is implemented three times and differs each time.

### 5.6 E5 — the quoting tax on every field spec

`validation/README.md` already records this and nothing has been done:

> `<` and `>` need shell quoting. Writing `matched:list<Match>` unquoted is a
> shell redirect — every script here had to quote those arguments, and every
> user will hit it.

It is worse than that line suggests, because `!` is history expansion in
interactive bash, and `!` is the *required-and-non-blank* suffix — the most
common one in the field table. So a realistic scaffold needs quotes on most of
its arguments:

```
jails g scaffold Message id:uuid@pk userId:uuid@index content:'string!' \
    read:boolean sentAt:instant
```

Every quote is two characters and a decision, on a command you type dozens of
times a week. The same README already lists the alternatives that need no
quoting — `matched:list:Match`, `matched:Match...`, `matched:Match+` — and the
suffix has the same options (`content:string.req`, `content:req(string)`, or a
`--required content` flag).

I do not think the syntax should change lightly; `string!` reads well and it is
in every example and golden file. But the decision should be **made**, not left
as a known tax with a note in a file nobody reads. The cheapest resolution that
keeps the syntax: accept both spellings, document the quote-free one as the
thing to type at a prompt, and keep `list<T>`/`!` as what you write in a
manifest, where there is no shell.

That last point matters more now than when the note was written, because
`.jails/app.toml` exists — and inside a manifest **there is no quoting problem
at all.** Which is an argument for making the manifest, not the prompt, the
place you write field specs.

### 5.7 E6 — the columns every table has, that you type every time

minicom's schema, in all four language stubs, is
`users(id, email, created_at, updated_at)` and
`messages(id, user_id, content, message_read, created_at, updated_at)`. Both
tables carry both timestamps. Every table anyone writes carries both timestamps.

jails has no `--timestamps` (verified: `jails g --help` lists `--package`,
`--index`, `--on`, `--yields`, `--debug`, `--pretend`, and nothing else), so you
type `createdAt:instant updatedAt:instant` on every scaffold, and then the
`updatedAt` half is a lie because nothing updates it.

`--timestamps` should add both columns, put `created_at` in the insert with the
clock the project already has (`add testkit` generates one), and — the part
that makes it worth a flag rather than two more keystrokes — **write the
`updated_at` assignment into the adapter's update path**, so the column means
what its name says. That is a small amount of code jails can get right once and
a thing hand-written code gets wrong forever.

### 5.8 E7 — the manifest is now the ergonomic unit, and it cannot be edited

`jails app` shipped mid-session (README's "Declarative applications" section,
`src/app.rs`). That changes where ergonomics lives: the unit is no longer a
command line, it is `.jails/app.toml`, and the question is no longer "how many
characters do I type" but **"can I change a line and re-apply?"**

Today, no. `.jails/app-state-v1` records completed intent keys and skips them,
and README says so plainly: "Changing an already-applied intent does not rewrite
user code: it is a new intent." So editing a `fields` line in the manifest and
re-running `app apply` does nothing at all.

That is the same gap as §5.2, one level up — and it is the one `examples/DOGFOOD.md`
already flags in its own friction ledger ("`.jails/app-state-v1` records
completed intent keys but does not yet notice a generated file deleted
afterward → store output fingerprints and reconcile drift instead of blindly
skipping").

So `g field` is not merely a nice generator. **It is the primitive the manifest
needs to become editable**, because "reconcile drift" for a changed `fields`
line *is* "add the field that is in the manifest and not in the record". Build
the command first, then let `app apply` call it. Doing it the other way round
means writing the reconciliation logic twice.

### 5.9 One more, cheap: refusals are ergonomics

```
$ jails g scaffold Message ... priority:int
jails: .../src/test/resources/fixtures/messages.json already exists
```

That is technically true and practically useless. It names a file the user has
probably forgotten exists, gives no cause and no next step, and it is the
message you get for the single most common mistaken command in the tool. It
should say what happened and what to do:

```
jails: Message is already scaffolded (6 files).
       jails cannot grow a scaffold in place.
  fix: jails g field Message priority:int
```

`doctor` already holds jails to this standard — an integration test asserts
every `FAIL` carries a `fix:` line. Generators are held to no such standard, and
they are the commands people actually run.

### 5.10 What not to build here

- **No ORM, no relation traversal, no lazy loading.** E2 is a column and a
  foreign key. `post.author()` returns a `User` the service loaded; it does not
  hit the database behind your back.
- **No `g field --remove`** in v1 (§5.2).
- **No inflector overrides in `jails.toml`** (§5.4).
- **No `g field` that rewrites an old migration.** Forward-only is the rule and
  it stays.
- **No provenance ledger as a prerequisite.** It would be better, and
  `write_new_file` is not even the single choke point it looks like
  (`src/add.rs:325-333` writes directly with `fs::write`), so building it right
  is a real project. `edited_files` plus "print the snippet, never clobber" is
  correct today and does not block on it.
- **No domain-specific field types.** `email:string` is a string; if it needs a
  check it needs a constraint, and constraints are already a closed set.

---

## 6. Tier 2 — `jails testd`: the only two-orders-of-magnitude win here

**Measured: 3,810 ms → ~110 ms.** Roughly 35x on the whole edit-test cycle, and
345x on the rerun alone. This is the number the "1000x" question was really
asking about, and it is available with no new dependency, no agent, no
alternate JVM and no preview feature.

Both `ideas-opus.md` A4.1 and `ideas-grok.md` §4 propose a watch mode. Both
re-invoke Maven on every change, so both cap out at ~3.8 s per iteration —
they remove the keystroke, not the cost. `ideas-sol.md` Bet 1 wants a resident
loop and is right; it does not say what makes it fast. The answer is that the
process must not die.

### The design

A long-lived JVM, `~/.jails/testd/<project-hash>.sock` (or a port in
`.jails/`), started lazily by `jails test` and reused:

1. **Classpath**, resolved once via `mvn dependency:build-classpath` and cached
   against the `pom.xml` mtime. Measured: ~2 s, once per pom change, and it
   already exists as a step in `console.rs`.
2. **Compile** with `ToolProvider.getSystemJavaCompiler()` in-process, only the
   changed file and its dependents. Measured **74–166 ms** warm. This is the
   part people assume needs Gradle/Bazel machinery; it does not, because javac
   warm in a live JVM is already fast.
3. **Run** through `LauncherFactory.openSession()` +
   `DiscoverySelectors.selectClass` / `selectMethod`. Measured **9–13 ms**
   warm.
4. **Isolate** each run behind a fresh `URLClassLoader` over
   `target/test-classes` so redefinition limits never apply — you throw the
   classloader away rather than trying to hot-swap into it. (This is the piece
   I did *not* measure; the 11 ms figure is with a shared loader. Expect tens
   of ms, not seconds, but treat it as UNVERIFIED until benchmarked.)
5. **Die** on `pom.xml` change, on `jails testd stop`, and after an idle
   timeout.

`jails test` keeps its exact current surface and gains `--fast` (use the
daemon), `--watch`, `--failed` (read `target/surefire-reports/*.xml`), and
`Class#method` support from Tier 0 item 4. `jails check` stays `mvn clean
verify` — the daemon is the loop, not the gate, and CLAUDE.md's reason for
`clean` is still correct.

### What it must not become

- Not a second build tool. The daemon never resolves dependencies itself; it
  asks Maven and caches the answer.
- Not the gate. A green daemon run is not a green `jails check`, and the
  README must say so in the same paragraph.
- Not silent about staleness. If `pom.xml` changed and the daemon restarted,
  say so on the line it prints.
- Not a new crate dependency. A Unix socket, a JSON line protocol and a ~200
  line Java file that jails writes into `~/.jails/` and compiles once.

### The honest caveat

The daemon helps unit tests, which is where most of the volume is. It does
nothing for `@SpringBootTest` (context startup dominates) and nothing for
Testcontainers ITs. For those, the verified cheap win is different and is
already sitting there unused: `GenericContainer.withReuse(true)`
(`deps/testcontainers-java/core/.../GenericContainer.java:1424`) plus
`testcontainers.reuse.enable`
(`.../TestcontainersConfiguration.java:184`). Spring's own lifecycle processor
already refuses to destroy a reused container
(`deps/spring-boot/core/spring-boot-testcontainers/.../TestcontainersLifecycleBeanPostProcessor.java:188-191`).
`grep -rn withReuse templates/ src/` → nothing. Two lines in one template plus
a `doctor` WARN when the flag is absent, because without it people conclude
Testcontainers is simply slow.

### And the mechanism nobody proposed for the *app* loop

`spring-boot:test-run` is a real Maven goal since Boot 3.1.0
(`deps/spring-boot/build-plugin/spring-boot-maven-plugin/src/main/java/org/springframework/boot/maven/TestRunMojo.java:36-74`).
It runs the app on the **test** runtime classpath. Paired with a generated

```java
// src/test/java/<base>/TestApplication.java
SpringApplication.from(App::main).with(TestcontainersConfig.class).run(args);
```

(`SpringApplication.from` at
`deps/spring-boot/core/spring-boot/src/main/java/org/springframework/boot/SpringApplication.java:1432`,
`.with` at `:1510`), it gives a dev run backed by the Testcontainers config
`add db` **already generates** — and it needs no compose at all.

That matters here specifically. CLAUDE.md documents that
`spring-boot-docker-compose` cannot drive podman-compose, so every `add db`
project on this machine needs `spring.docker.compose.enabled=false`. This path
removes compose from the dev loop entirely rather than working around it.
`ideas-opus.md` A1's three-tier hot-swap design should be replaced by this: one
tier of it does not exist (§1.1), and this one is supported upstream.

---

## 7. Tier 3 — the codebase you did not create

Ranked here because it is measured, cheap, and it is the difference between
having the tool on interview day and not.

### The facts

```
$ cd ideas/minicom-public/spring && jails about
jails: no pom.xml found in this or any parent directory
$ jails routes
jails: no pom.xml found in this or any parent directory
$ jails g record Foo x:string --pretend
jails: no pom.xml found in this or any parent directory
```

Same in `ideas/monzo-crawler2/app`. **Zero of ~30 commands work.**

The gate is one function — `generate::find_project_root`, 11 lines at
`src/generate.rs:86-96`, which walks up looking for `pom.xml` and nothing else.
It has ~30 call sites — `grep -rn 'find_project_root()' src/ | wc -l` returned
28, then 29, then 30 over one afternoon, because someone else is editing this
tree; prefer the grep to the number. There are three further copies of the rule:
`project::nearest_pom` (`src/project.rs:160-167`),
`project::workspace_root` (`:169-175`), and — in Lua —
`jails.nvim/lua/jails/init.lua:9` (`root_markers = { 'pom.xml' }`).

And the decisive experiment: dropping a **one-line stub `pom.xml`** into a copy
of the Intercom stub makes `routes`, `beans`, `stats`, `notes`, `rename
--dry-run`, `destroy --pretend`, `doctor` and `g record`/`g controller` all
work correctly against the Gradle sources. `jails routes` printed
`POST /bar BarController#verify` and `POST /foo FooController#verify`;
`jails beans` printed all three beans.

`src/inspect.rs` (routes, beans, stats, notes) and `src/rename.rs` contain
**zero** occurrences of the string `pom`. Their entire Maven dependency is the
root-finding call.

### The change

Widen the marker list in that one function and return *why* it matched:

```rust
pub(crate) enum Build { Maven, Foreign(&'static str), Bare }
pub(crate) fn find_project_root() -> Result<PathBuf>   // signature unchanged, 29 call sites untouched
pub(crate) fn project_build(root: &Path) -> Build      // new
```

Checked per directory while walking up, so the nearest wins: `pom.xml` →
`Maven`; `build.gradle{,.kts}` / `settings.gradle{,.kts}` → `Foreign`;
`jails.toml` → `Bare`. A `pom.xml` beside a `build.gradle` still wins, so
polyglot repos behave as today.

Then three guards so the degraded mode is honest rather than lying:

- `pom::read` is the single funnel for every command that needs pom *content*.
  On ENOENT it consults `project_build` and says `this project is built by
  Gradle (build.gradle); jails only edits pom.xml`.
- The eight Maven-inherent commands (`test build clean fmt check mvn run
  console`) get a one-line `require_maven` guard.
- `doctor` reports the real build tool instead of `plain Maven`, and
  `maven_check` becomes `-- maven  not a Maven project`. **This part is not
  optional.** With a stub pom, `doctor` today prints `9 checks, all clear` over
  a Gradle Boot 2.7 project — three lies in one report — and a confident wrong
  report is worse than a refusal.

**Frame it correctly in README**: *jails never reads, writes, parses or invokes
`build.gradle`. It stops treating `pom.xml` as the only thing that marks a
project root.* That is strictly less than Gradle support. `ideas-grok.md` §2
applies the "no Gradle" constraint to a case it does not cover, and pays for it
by leaving 100% of the tool unreachable in the one codebase the user is
actually handed.

### Two caveats the experiment surfaced

1. **The stub-pom trick changes the Java jails emits.** Without a readable pom,
   `repository_wiring` silently returns `PlainJdbc`
   (`src/generate/repository.rs:91-97`) and `jspecify_available` returns false
   (`src/generate.rs:473-478`), so no `package-info.java` is written and the
   adapter shape changes. A degraded mode must *say* which shape it chose, not
   just choose one.
2. **`add` still will not work**, marker widening or not:
   `require_java_release` (`src/add.rs:776-789`) hard-errors on any project
   without `<maven.compiler.release>`/`<java.version>`. So "use `add sqlite` to
   rehearse the H2 shape" does not compose with the degraded mode unless `add`
   is explicitly exempted — and it should not be, because `add`'s whole job is
   a pom edit.
3. **Multi-module Gradle breaks the Maven-shaped assumption.** In
   `ideas/monzo-crawler2`, `build.gradle` lives in `app/` while
   `settings.gradle` (the real workspace root) is a directory above. Rooting at
   the nearest marker is right for `generate` and wrong for `workspace_root`.
   Pick per command, and write the test.

### `jails adopt`

Once the root resolves, `g record` in the Intercom stub lands in
`com.intercom.spring.domain` — a package that project does not have. Its real
packages are `models` and `controllers`.

**The placement engine for this already exists and I verified it end to end.**
Writing

```toml
[layout]
web    = "controllers"
domain = "models"
```

into the stub made `jails stats` report `Web 2` (previously `Other 4`), put
`g record Message` in `com/intercom/spring/models/` and `g controller
Conversation` in `com/intercom/spring/controllers/`. No code change.

So `jails adopt` writes no new placement machinery — it writes that file. It
resolves the base package (`base_package` already falls back to the shallowest
`.java` file), enumerates the immediate subpackage directories, and maps them
onto `config::LAYERS_IN_ORDER` through a small **closed** synonym table
(`model|models|entity|entities|domain → domain`,
`controller|controllers|web|rest → web`, and so on). A directory matching
nothing is **reported, not guessed**; a layer with two candidates is reported
and left unmapped, because choosing between `model/` and `domain/` is exactly
the silent-wrong-placement failure the command exists to prevent.

It must **never** write `[project] capabilities`. That table means "what `add`
installed and `sync` should restore," and in a foreign project jails installed
nothing — writing it would make the next `sync` try to splice a pom that is not
there.

---

## 8. Tier 4 — the two verticals, corrected by their own source

### 8.1 minicom is not a product spec

`ideas/minicom-public/README.md:6-9`: run foo at :8008, run bar at :8009, "verify
that an alert with `Yay! Everything works` fires." Both frontends do one thing —
`$.post(endpoint)` and check `response.success === true`
(`foo-website/foo.js:10-19`, `bar-website/bar.js:10-19`). **There is no
messaging code in the repository at all.** The stub is a connectivity smoke
test; the actual exercise is handed out at interview time.

Both earlier documents treat it as a spec — `ideas-opus.md` B1 as "users →
conversations → messages", `ideas-grok.md` §8 as "conversations, realtime
fanout, inbound webhooks". They are designing against a product that is not in
the repo.

What *is* in the repo is the schema, identical across all four language stubs
and flat: `users(id, email, created_at, updated_at)` and
`messages(id, user_id, content, message_read, created_at, updated_at)`
(`spring/src/main/resources/schema.sql:1-14`). No conversation entity, no
direction column. `jails g scaffold Message id:uuid@pk userId:uuid@index
content:string! read:boolean` is already a one-line match for the whole thing.

**The generator gap is not the CRUD. It is everything after it.**

### 8.2 The honest counterweight, from the user's own Rails solution

`ideas/minicom-rails` (2018) is the thing "Rails is super productive" points
at. It shipped three bugs a compiler would have caught, in about 90 lines:

- `config/routes.rb:10` routes to `endpoints#n_last_messages`; the controller
  defines `n_last` (`endpoints_controller.rb:46`). Routing error at request
  time.
- `endpoints_controller.rb:26,29` call `user.find_by` / `message.find_by` on
  undefined local variables instead of the `User` / `Message` constants.
- `_send_message(direction)` overwrites its own parameter with `'O'` at
  line 69, so `send_admin` (line 43, passing `'I'`) stores an outbound message.

It also did **not** do realtime: the widget polls `/api/ping`
(`app/assets/javascripts/minicom.js:3-12`) and the agent inbox refreshes with
`window.location = window.location`. `config/cable.yml` exists and is unused.

Two conclusions worth writing down, because they should drive what gets built:

1. Rails bought speed-to-first-endpoint and paid in latent runtime failures.
   The jails pitch is not "as fast as Rails" — it is **"as fast to the first
   endpoint, and those three bugs cannot compile."** That is a claim jails can
   actually make and Rails cannot.
2. Realtime is a **differentiator, not table stakes**. It should be ranked
   below the thing that currently makes a jails project unusable from a
   browser widget at all:

### 8.3 `jails add cors` — the actual blocker

The exercise is inherently cross-origin: foo serves from `127.0.0.1:8008`, bar
from `127.0.0.1:8009`, both POST to `localhost:3000` — three distinct browser
origins (`localhost` and `127.0.0.1` are different origins). The stub handles it
with a blanket `registry.addMapping("/**")` (`WebConfig.java:14-16`).

`grep -rni cors src/ templates/ README.md` returns **nothing**. And
`templates/spring/security_config_java.java` has `anyRequest().authenticated()`
and never calls `.cors(...)`, so `add security` leaves no `CorsFilter`
registered and the preflight is handled by the security chain. **A
jails-generated Spring app plus `add security` cannot serve a browser widget.**
Neither earlier document mentions CORS.

And the naive fix is wrong in a way that bites later:
`CorsConfiguration.applyPermitDefaultValues()` — what a bare `addMapping("/**")`
gets you — permits only GET, HEAD and POST and does not allow credentials
(`deps/spring-framework/spring-web/src/main/java/org/springframework/web/cors/CorsConfiguration.java:522-538`,
`:68-69`). That is the classic "works until mark-as-read becomes a PUT"
failure. `add cors` must name the methods explicitly, put the origins in a
marked properties block, and wire `.cors(...)` into the generated security
chain in the same change.

### 8.4 `add sse`, with the four things both documents get wrong

`SseEmitter` is alive and undeprecated in Framework 7
(`deps/spring-framework/spring-webmvc/src/main/java/org/springframework/web/servlet/mvc/method/annotation/SseEmitter.java:45`).
Both documents pick it and both are right. The details are where they slip:

- **The never-time-out value is `-1L`, not `Long.MAX_VALUE`.** Spring's own
  reactive path uses exactly `-1` (`ReactiveTypeHandler.java:80,166`), and the
  chain is verifiable end to end: emitter timeout → `new
  DeferredResult<>(emitter.getTimeout())` → `asyncContext.setTimeout(-1)` →
  Tomcat's `if (asyncTimeout > 0)` guard, which never fires. Both documents say
  "infinite timeout" without saying what to write; `Long.MAX_VALUE` is the folk
  answer and is not what Spring does. The 30 s default that makes this matter
  is Tomcat's, not Spring's
  (`deps/tomcat/java/org/apache/catalina/connector/Connector.java:169`).
- **`onCompletion` alone suffices for removal.** Its javadoc: called "when an
  async request completed for **any** reason including timeout and network
  error" (`ResponseBodyEmitter.java:346-352`). `ideas-opus.md` prescribes three
  callbacks; `ideas-grok.md` prescribes "complete on IOException", which the
  `send()` javadoc explicitly calls unnecessary (`:176-186`). The requirement
  both **miss** is the real one: `onCompletion` runs on a *container* thread,
  concurrently with whatever thread is broadcasting, so the registry must be
  `ConcurrentHashMap<K, Set<SseEmitter>>` with `newKeySet()` values — not a
  synchronized list.
- **`spring.task.scheduling.pool.size` defaults to 1**
  (`deps/spring-boot/core/spring-boot-autoconfigure/.../task/TaskSchedulingProperties.java:65-71`).
  A `@Scheduled(fixedRate = 15000)` heartbeat that blocks on one dead client
  stalls **every other scheduled job in the application**, and nothing logs it.
  `add sse` must either raise the pool size or set
  `spring.threads.virtual.enabled=true`, and say which in the Javadoc.
- **`Last-Event-ID` is not implemented by Spring.** Zero matches across
  spring-web and spring-webmvc main sources. Spring gives you
  `SseEventBuilder.id()` on the way out and reads nothing on the way back in.
  A hub that emits `id()` without a matching
  `@RequestHeader("Last-Event-ID")` replay path is advertising resumability it
  does not have. `ideas-opus.md` proposes the replay hook and is right; it is
  now provably not free.

One genuinely Framework-7-only fact that makes "SSE + virtual threads" a real
recommendation rather than a slogan: Framework 7 replaced `synchronized` with
an explicit `ReentrantLock` throughout `ResponseBodyEmitter` specifically to
avoid virtual-thread pinning (`ResponseBodyEmitter.java:91-94`). On Framework
6.2 and earlier the same hub pins the carrier thread on every `send()`.

### 8.5 `g auth` — against Spring Security 7, not raw Nimbus

`ideas-grok.md` §8.2 proposes minting with Nimbus directly. That is a level too
low. Spring Security 7.2 ships both sides —
`NimbusJwtEncoder.withSecretKey(SecretKey)` (**`@since 7.0`**, so every
tutorial and every model's memory predates it) and
`NimbusJwtDecoder.withSecretKey(...).macAlgorithm(HS256)`
(`deps/spring-security/oauth2/oauth2-jose/src/main/java/org/springframework/security/oauth2/jwt/NimbusJwtEncoder.java:448-457`,
`NimbusJwtDecoder.java:266`) — and `spring-security-oauth2-jose` declares
`api 'com.nimbusds:nimbus-jose-jwt'`, so Nimbus arrives transitively and never
needs declaring. Coding against `SignedJWT` directly gives up `JwtDecoder`'s
validator chain, `@AuthenticationPrincipal Jwt`, and
`oauth2ResourceServer().jwt()`.

This is a textbook jails-bar generator, for a reason neither document states:
**Boot 4 auto-configures no `JwtEncoder` at all** (grep over
`deps/spring-boot` main sources → zero hits) and configures a `JwtDecoder` only
from `jwk-set-uri`/`issuer-uri`/`public-key-location` — there is no property
for a shared HMAC secret. So 100% of symmetric mint/verify wiring is
hand-written boilerplate Boot will never do for you.

And the silent failure that earns it a place: **a JWT with no `exp` claim
passes the default decoder.** `JwtTimestampValidator.allowEmptyExpiryClaim`
defaults to `true` — `private boolean allowEmptyExpiryClaim = true;` at
`deps/spring-security/oauth2/oauth2-jose/.../JwtTimestampValidator.java:58`,
opt-out at `:82` —
and the default chain has no issuer and no audience check
(`JwtValidators.java:77-81`). A forever-token validates and nothing says so.
`ideas-grok.md` requires expiry when *minting* and misses that the *verifying*
side is the one that lets it through. The generator must call
`setAllowEmptyExpiryClaim(false)` and add the issuer/audience validators
explicitly.

Also: use `spring-boot-starter-security-oauth2-resource-server`.
`spring-boot-starter-oauth2-resource-server` is deprecated in Boot 4
(`deps/spring-boot/starter/spring-boot-starter-oauth2-resource-server/build.gradle:21`) —
the same rename family as `spring-boot-starter-web` → `-webmvc`, which jails
already got right in its golden poms.

### 8.6 The crawler — and a doctrine that lands on it

**Read this before the design below.** While this document was being written,
`src/app.rs` and `examples/` appeared in the working tree, and `src/app.rs:1-6`
states a doctrine that the design in this section violates:

> This is deliberately domain-blind. A crawler and a support inbox are two
> different lists of the same generic intents; neither gets a command, branch,
> enum, or template in Jails core.

`examples/web-crawler/.jails/app.toml` and `examples/support-inbox/.jails/app.toml`
already exist, and `examples/DOGFOOD.md`'s friction ledger already records the
exact gap a spider would fill — "scaffolds provide CRUD plumbing, not crawler
traversal or conversation assignment behavior" — with a chosen generic fix:
**"generic `usecase`, `query`, `event`, and durable `job` intents."**

So `g spider` as an `ArtifactKind` is now explicitly off the table, and
`ideas-opus.md` B3 / `ideas-grok.md` §7 / everything below are all proposing a
kind the repo has since decided not to have. That decision is defensible and I
am not arguing with it.

What survives, and why this section is still worth reading:

- **`add html` is a capability, not a domain branch.** jsoup + `Urls` +
  `Robots` + `Fetcher` are as domain-blind as `add csv` or `add json` — they
  know no seed URL, exactly as `add kafka` knows no topic. That is the same cut
  the doctrine draws, so a capability is inside the line even though a `spider`
  kind is outside it.
- **The five bugs below are the acceptance criteria for whatever shape wins.**
  Whether traversal arrives as a generic `usecase` intent or as a capability,
  it has to not have these bugs, and the WireMock assertions at the end of this
  section pin them regardless of which command emits the code.
- **`add cors` (§8.3) is likewise a capability** and is unaffected by the
  doctrine.

Read the rest as a specification for the crawler-shaped `usecase` intent the
friction ledger already commits to, not as a proposal for a new kind.

The reference implementations in `ideas/` say what to actually generate, and —
more usefully — what five bugs to generate *around*. Every one was read out of
the source:

| Bug | Where | What it costs |
|---|---|---|
| check-then-act dedup | `monzo-web-crawler/crawler.go:76` vs `:95` | two goroutines fetch the same URL |
| raw URL as the visited key | `monzo-crawler2/CrawlerEngineImpl.java:81` | `/a` and `/a#x` crawled twice |
| fused fetch + parse | `monzo-crawler/HtmlParser.java:27` | the parser cannot be tested without a network |
| latch counted down on one path only | `monzo-crawler2/EngineObserver.java:38-46` | any skipped path hangs forever |
| no robots.txt at all | all three Monzo solutions | — |

The cut mirrors `add kafka` / `g event` exactly: `add html` knows no seed URL,
`g spider` does.

**`add html`** writes four classes into `adapters/` (no new layout key):
`Urls` (canonicalisation), `Robots` (RFC 9309, ~90 lines, no dependency —
robots patterns are not regexes, which is why no regex engine appears),
`Fetcher` (one `HttpClient`, `Redirect.NEVER` with redirects followed manually
so every hop can be re-scoped, User-Agent a required constructor argument with
no lying default, and `BodySubscribers.limiting` — `@since 25` — for the size
cap), and `Html` (jsoup parse + link extraction, never `Jsoup.connect`).

Two prerequisites the documents skip. **jsoup is not in the deps manifest**
(`grep -ic jsoup deps.tsv` → 0), and CLAUDE.md's rule is that every
template is written against the checkout, not from memory —
`ideas-grok.md` §7 notes the gap in its research log and then writes the `Html`
API surface from memory anyway. Clone it first. And `add http` is an HTTP
**server** on `com.sun.net.httpserver` in the `api` layer
(`templates/add/http_server_java.java:3-4`); `ideas-opus.md` B3's "extend `add
http`" would extend a server capability into a fetcher.

**`g spider`** writes the frontier, the politeness gate and the termination.
The three lines that carry the design:

```java
boolean claim(URI u) { return claimed.size() < maxPages && claimed.add(u); }
// add() IS the dedup. No separate contains() — check-then-act is the monzo bug.

inFlight.incrementAndGet();          // at claim time, BEFORE submission
try { visit(url, depth); } finally { inFlight.decrementAndGet(); }
// termination is inFlight == 0, never queue.isEmpty()
```

with `Executors.newVirtualThreadPerTaskExecutor()` (final on JDK 27;
`StructuredTaskScope` is still `@PreviewFeature` in the JDK 28 mainline
checkout, so it stays out of generated code) and per-host — not global —
politeness with the pace sleep held *inside* the permit, which is what makes
`--per-host 1 --delay-ms 250` a real floor.

Two corrections worth carrying:

- **Do not match `output_example.json`.** `ideas-opus.md` B3 says to. That file
  is `{"urls": {url: true}}` — a visited set with no per-page links, which
  fails requirement 3 of the very brief it answers ("print each URL visited,
  **and** a list of links found on that page"). Emit per-page records with the
  internal/external split and the anchor text.
- **Do not generate `shouldVisit`/`onPage` overrides.** That is crawler4j's
  subclassing API (last commit 2020-10-03). `ideas-grok.md` §7 proposes it;
  constructor-injected `Predicate<URI>` and `Consumer<Page>` express the same
  thing and let the test pass a lambda.

The IT is WireMock (already in `deps/`) serving a cycle, an off-domain link, a
301 chain, a 404 and a robots.txt. Three assertions carry the whole design:
`verify(1, getRequestedFor(urlEqualTo("/a")))` (exactly one *request* per
canonical URL — stronger than "visited once"),
`assertTimeoutPreemptively(...)` (pins the in-flight counter and fails the
moment anyone moves `leave()` out of the `finally`), and
`verify(0, getRequestedFor(urlEqualTo("/private/x")))` (pins robots).

---

## 9. Tier 5 — the agent is the second user

You run Claude Code all day, and you have already invested in this: `java.md`,
`spring.md` and `backend.md` are 70 KB of hand-written personas whose whole
purpose is stopping a model from writing Boot 2 / JPA / Lombok / `@MockBean`
code. `CLAUDE.md` is 40 KB of the same, for this repo.

**A generated project inherits none of it.** That is the gap.

The three pieces, in order of value:

1. **`jails new` writes `AGENTS.md` into the project** — and the banned-API
   list in it is *rendered from* the same table a new `jails lint` matches
   against, so it cannot drift into a lie. That is the whole trick: a
   hand-written AGENTS.md is a `validation/README.md` waiting to happen (§2.7).
2. **`jails lint`** — a closed rule table over the stale-API families jails
   already knows about (`@MockBean`, `javax.validation`, Jackson 2 alongside
   3, `spring-boot-starter-web`, `@Entity`, Lombok annotations, preview
   features). Sub-second, exit 1, `file:line`. It turns a six-minute
   compile-read-fix loop into a check an agent can run before handing back.
   Note that `doctor` already does the Jackson-majors version of this, so the
   rule shape is proven.
3. **Machine-readable everything.** Only `about`, `routes` and `beans` have
   `--json` today (`src/main.rs:86, 274, 282`). `doctor --json`, `why --json`
   and `test --json` are each an afternoon and each removes a parsing step from
   both the editor and the agent. `why --json` is the highest-value one,
   because it makes the *explanation* available as quickfix text.

Two things to say no to. An MCP server for jails is worse than the CLI an agent
already shells to — it adds a process, a schema to keep in sync, and a failure
mode in headless runs, for no capability the CLI lacks. And do not put an LLM
inside jails: deterministic generation is the product, and the moment `g spider`
asks a model for a selector you cannot golden-file it and you cannot destroy it.

One thing already there and worth promoting: **`g cases`** turns a markdown
brief's acceptance bullets into a test class. It is the spec-first workflow both
you and an agent want, it is implemented, and §2.7 says nobody can find it.

---

## 10. The honest arithmetic

You asked for 1000x. Here is what is actually available, with the source of
each number.

**The latency loop** — how long you wait:

| Loop | Today | After | Multiple | Basis |
|---|---:|---:|---:|---|
| Rerun the test you just edited | 3,810 ms | ~110 ms | **35x** | measured, §1 |
| …the rerun alone | 3,810 ms | 11 ms | 345x | measured, §1 |
| JVM start for a packaged app | 6.6 s | 2.96 s | 2.4x | measured, §1.1 |
| Testcontainers per test run | ~4 s | ~0 s | — | upstream, unmeasured here |

**The ergonomic loop** — how much you type. This is the half the other four
documents skip, and per-occurrence it is the larger number:

| Change | Today | After | Basis |
|---|---:|---:|---|
| First running REST resource | 1 command, 1,180 lines | unchanged | measured, §5.1 — already won |
| **Add one field to it** | **6 files, ~17 sites, + a migration** | **1 command** | measured, §5.1–5.2 |
| Store a relation (`author:User`) | not stored at all; column + bind + mapper + FK by hand | 1 field spec | measured, §5.3 |
| Correct `/categorys` → `/categories` | 5 edits across controller, DDL, migration, DTO, fixture | 0 | measured, §5.4 |
| Model-first (`g record` then scaffold) | blocked; retype every field | `g scaffold <Name>` | measured, §5.5 |
| `created_at`/`updated_at` | typed per table, and `updated_at` never updates | `--timestamps` | measured, §5.7 |
| Change a field in `.jails/app.toml` | no effect — the intent is skipped | re-applied | README + `examples/DOGFOOD.md`, §5.8 |
| Commands available in a foreign repo | 0 | 8 demonstrated | run, §7 |
| Finding out *why* it broke | 20 min | seconds | `why` already exists |

Multiply the first table's first row by the number of times a day you run one
test, and the second table's second row by the number of times a week you
change a model. Neither is 1000x, and no honest row will be — but the second
table is the one that decides whether a two-day feature is two days or four,
and it is the one nobody has been counting.

The unquantifiable part is bigger than all of it: **six defects that fail
silently.** A pom Maven cannot read, a test filter that matches nothing and
exits 0, a watched app that dies without a word, a health check that cries wolf
by default, a generated test that cannot compile on a common Boot version, and
a golden suite that ratifies the first of these. None of those cost seconds.
They cost the afternoon where you assume the tool works and debug your own code
instead.

That is where the multiple actually lives, and it is why §4 comes before
everything — and why, given the choice between §5 and §6, §5 wins. A test
that reruns in 11 ms is worth little on a model you cannot change without
six files of hand-editing.

---

## 11. Monday morning

**Monday.** §4, items 1–7. You will have fixed two ways jails writes a broken
project, one silently-wrong test filter, one silent startup failure, one false
FAIL you see every day, and you will have a build matrix that stops the class.
Regenerate the goldens and *read the diff* — that diff is the bug report.

**Tuesday.** §4, items 8–11 in the morning (framework-version resolver, README
for `g cases`, the nvim list test, the `validation/` table). Afternoon:
`withReuse(true)` plus the `doctor` WARN — two lines for ~4 s off every db test
run.

**Wednesday–Thursday.** §5.4 and §5.5 — the inflector and "scaffold reads the
record off disk". Both are half-day changes, both are visible in the first
minute of a demo, and the inflector has one owner so it fixes routes, tables
and fixture filenames in one go. Then §5.9, the refusal messages, which is an
hour and makes every wrong command teach you the right one.

**The following week — the one that changes how the tool feels.** §5.2
`g field`, then §5.3 relations. These are the two that move jails from "makes
a project" to "grows a project", and they are the only items in the document
whose absence you pay for on every single model change. `g field` first,
because §5.8 means `app apply` needs it before the manifest can become
editable; relations second, because they are the only defect here whose
symptom is lost data.

**After that.** §7, the marker widening plus `jails adopt`. Then run
`jails routes` in `ideas/minicom-public/spring` and watch it work. This is the
one that changes what the tool *is* — from "a thing that makes new projects"
to "a thing you can bring to a codebase."

**Then.** §6, `jails testd`. It is the biggest single number in the document
and it is deliberately not first: it is the only item with an unmeasured piece
(the fresh-classloader cost), and an 11 ms test loop is worth little on a model
you still cannot change without editing six files.

**Then, and only then, a vertical.** `add cors` first — it is small and it is
what currently makes a jails project unusable from a browser. Then `add sse`.
Then `add html` as a capability — and route traversal through the generic
`usecase` intent `examples/DOGFOOD.md` already commits to, not through a
`spider` kind, which `src/app.rs:1-6` rules out. Clone jsoup before writing a
line of the template; note that `deps-update.sh:49` reads the manifest with
`IFS=$'\t' read`, so appending a row with a bare `echo 'jsoup\tjhy/jsoup'` lands
one literal token and silently does nothing — use `printf`.

What to keep saying no to: a plugin system (note that `jails app`'s
`.jails/app.toml` is already close to the line README's "Not yet" draws — a
closed `[[generate]]` schema of existing kinds is defensible, an ordered list
of arbitrary user-authored intents that a compiler expands is the thing being
deferred); an ORM; Gradle *support* as distinct from Gradle-directory
*tolerance*; a second template family for pre-6.2 Spring; and any generator
whose failure mode is loud, since a loud failure is one the compiler already
reports.

---

## 12. Research log — what was actually run

Commands executed this session, in the scratchpad, against real files:

- `jails new-cli` + `g scaffold` + `mvn -o test` → §2.1 (reproduced twice, and
  again with `g dto`).
- `tests/golden/scaffold-plain/pom.xml` read directly → §2.2.
- `mvn -o -q compile` / `test -Dtest=…` × 7, `javac` × 2, JUnit `Launcher` in a
  precompiled runner × 3, an in-JVM loop × 5, an in-process `JavaCompiler` loop
  × 5 → §1.
- `java -XX:AOTCacheOutput=…` with a directory classpath (failed) and with a
  jar classpath (succeeded), plus `-XX:+PrintFlagsFinal` on JDK 26 and 27-ea →
  §1.1.
- `jails doctor` with and without `JAVA_HOME`, on a broken project and a
  healthy one → §2.6.
- `jails about` / `routes` / `g record --pretend` in
  `ideas/minicom-public/spring` and `ideas/monzo-crawler2/app`; the stub-pom
  experiment; the `[layout]` adoption experiment → §7.
- `git -C deps/spring-boot show v{2.7.18,3.3.0,3.4.0}:gradle.properties` →
  §2.3.
- The whole of §5, on three throwaway projects: `new-cli` + `add sqlite` +
  `add json` + one `g scaffold`, then `grep -rlc` for a single field across the
  output (6 files, ~17 sites); a re-run of the same scaffold with one extra
  field (refused, naming the fixture); `g record User` + `g scaffold Post
  author:User` and read the emitted migration and adapter (column typed `text`,
  field not persisted); six scaffolds of `Person Category Company Status Box
  Analysis` read back through `jails routes`, the migrations and
  `ls fixtures/`; `g record Draft` + `g scaffold Draft` (refused) against
  `g repo Draft --pretend` (reads the record); and `jails g --help` for the
  absent `--timestamps`.
- `grep -c cases README.md`; the four `jails.nvim` tables against the two
  ValueEnums; `validation/README.md` against today's README; CLAUDE.md's
  `deps/deps.tsv` / `deps/update.sh` against `ls` → §2.7.
- The three §8 claims that carry the most weight, re-checked by hand rather
  than taken from a research pass: `private int size = 1`
  (`TaskSchedulingProperties.java:71`), `private boolean allowEmptyExpiryClaim
  = true` (`JwtTimestampValidator.java:58`), and `STREAMING_TIMEOUT_VALUE = -1`
  used as `new SseEmitter(...)` (`ReactiveTypeHandler.java:80,166`). All three
  hold.

Source read for the API claims, all in `deps/`: `jdk` (jvmti.xml, cds_globals,
Preview.java, StructuredTaskScope, HttpClient/HttpResponse), `spring-boot`
(TestRunMojo, SpringApplication, devtools/RestartClassLoader, WebMvcProperties,
TaskSchedulingProperties, the starter deprecations), `spring-framework`
(SseEmitter, ResponseBodyEmitter, ReactiveTypeHandler, CorsConfiguration,
MockMvcTester, WebSocketSession), `spring-security` (NimbusJwtEncoder/Decoder,
JwtTimestampValidator, JwtValidators), `testcontainers-java` (GenericContainer,
TestcontainersConfiguration), `tomcat` (Connector, AsyncContextImpl).

Reference projects read for design, in `ideas/`: `minicom-public` (all four
stubs, both websites, `script/`), `minicom-rails` (routes, controllers,
migrations, assets), `monzo-crawler`, `monzo-crawler2`, `monzo-web-crawler`,
`monzo-code-challenge`, `colly`, `katana`, `nutch`, `heritrix3`, `crawl4ai`,
`crawler4j`.

**Marked UNVERIFIED and not relied on**: which JDK release finalised each AOT
JEP (the `deps/jdk` checkout is mainline at feature version 28, so it can prove
a feature is still preview but not which release shipped it); the released
Maven coordinates for jsoup and the split WireMock artifacts; the cost of a
fresh `URLClassLoader` per daemon run; Quarkus's dev-mode internals (not
cloned).

**Note on line numbers.** An adversarial verification pass caught several
off-by-one citations in the underlying research; those are corrected here.
Prefer the stable anchors — function names, enum variants, the grep itself — to
the numbers. `grep -rn 'find_project_root()' src/ | wc -l` returned 28, then 29,
then 30 over one afternoon, because the tree was being edited while this was
written. This file will drift the same way `validation/README.md` did.

**What changed underneath this document while it was being written.**
`ideas-kimi.md` and `ideas-sol.md` appeared (so this is the fifth document, not
the third), README.md gained a `jails app` section, and `src/app.rs`,
`examples/web-crawler/`, `examples/support-inbox/` and `examples/DOGFOOD.md`
all landed. §8.6 is rewritten around that; §8.3's `add cors` and §8.4's
`add sse` are capabilities and are unaffected. Everything in §1–§7 was measured
or reproduced after those files existed.

**One claim I was told to include and did not.** A verification pass reported
that `g cases --pretend` is "inverted" because `src/generate/migration.rs:101`
checks `path.exists()` before the `pretend` branch. It is not a defect: the same
ordering is deliberate in `generate.rs:1016-1019`, whose comment says
`--pretend` runs every check "so a run that would have collided reports the
collision rather than a clean-looking plan." `g cases` matches the invariant.
Recorded because it is the clearest illustration of this document's rule —
running the thing is what separates a defect from a house style.

**Left unverified when the session ended**, and therefore not load-bearing
anywhere above: the cost of a fresh `URLClassLoader` per `jails testd` run
(§6); Quarkus dev-mode internals; the released Maven coordinates for jsoup and
the split WireMock artifacts; which JDK release finalised each AOT JEP (the
`deps/jdk` checkout is mainline at feature version 28, so it proves what is
*still* preview and not what shipped when); and whether `add db`'s generated
`TestcontainersConfig` accepts `withReuse(true)` without disturbing the
`@ServiceConnection` wiring (§6) — that one is two lines to try and should be
tried before it is written into a template.

One structural note for whoever builds provenance (`jails diff`, `jails
upgrade`, or safe field evolution): **`write_new_file` is not the single choke
point it looks like.** `src/add.rs:325-333` writes an existing path directly
with `fs::write` after calling `normalize_imports`, bypassing the collision
check and `package-info` planning. A ledger hung off `write_new_file` alone
would have a hole exactly where a capability updates a file it previously
wrote.
