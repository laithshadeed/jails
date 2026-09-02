<!--
The interface research: what a reader of jails sees, typed and waits for,
measured on the binary rather than read off the README. `docs/60-abstraction.md`
is the shape of the code; this is the shape of the *surface*, and the two are
kept apart so a deletion inside the compiler is never mistaken for a change
the reader can feel.

**A closed item is deleted from this file**, in the commit that closes it.
Item numbers `I70.n` and `I71.n` are stable and never reused; the two
series mark the first and second walk and nothing else. Every number below carries
the command that produced it; re-measure before quoting one.
-->

# 70 — The interface: clear, obvious, fast

**Read `docs/00-contracts.md` first.** Nothing here reopens a contract. The
compiler is right; what this file measures is how much of it a reader has to
*know* before `jails g scaffold Note title:string!` feels obvious, and how
long they wait for it.

The bar, stated once so every item below can be checked against it:

- **Clear.** One spelling per verb, one report shape, one JSON encoding. A
  fact is stated where it lives and nowhere else.
- **Obvious.** The first screen of `--help` fits on a screen. A message
  names the command that fixes it. A file jails writes is one the reader
  asked for, or the creation line says it was written.
- **No magic.** Nothing derived is hidden (`jails model explain` already
  holds that line); nothing is *repeated* either -- a warning about a fact
  the source states explicitly is noise, not transparency.
- **No surprises.** A read never writes. A count matches its list. One
  command is one plan. An error surfaces on the command that caused it.
- **Milliseconds.** Every operation that starts no JVM finishes under 50 ms
  on a 30-entity project, and the number is measured, not assumed.

## 1. What was measured

All on 2026-09-02, `cargo build --release` of this tree (`target/release/jails`,
12.6 MB), Linux, 4 cores, a scratch project from `jails new <name> --offline
--no-git`. Wall time is one run of a shell wrapper around `date +%s%N`; the
debug binary is within 2× on every row and identical on the 5 ms ones.

### 1.1 The surface

| what | count | method |
|---|---:|---|
| top-level commands | 50 | `jails commands --json`, rows with no space in `name` |
| subcommand rows, all depths | 96 | same, all rows |
| generator kinds | 39 | same, `kinds` |
| capabilities | 25 | same, `capabilities` |
| distinct non-global flags | 107 | same, `options` minus the seven globals |
| flag rows over all commands | 183 | same |
| rows carrying `--plan-in`, `--plan-out`, `--ast`, `--diff` | 96 of 96 | same |
| rows with a per-command `--json` beside the global `--output json` | 11 | same |
| rows with `--force` / rows with `--yes` for the same question | 5 / 2 | same |
| lines of `jails --help` | 93 | `jails --help \| wc -l` |
| lines of `jails g --help` | 235 | |
| lines of `jails resource --help`, of which global-flag boilerplate | 45 / 33 | `sed -n '/^Options:/,$p'` |
| `README.md` lines / command bullets | 1,758 / 98 | `wc -l`; `grep -c '^- \`jails '` |

### 1.2 Latency, one entity

| command | ms | note |
|---|---:|---|
| `jails --version`, `model check`, `routes`, `beans`, `stats`, any refusal outside a project | 5 | process floor |
| `jails new demo --offline --no-git` | 12 | |
| `model explain` | 12 | |
| `model plan` | 29 | |
| `g scaffold Note …` (first) | 38 | 20 files |
| `g scaffold Note …` (repeat, "nothing to do") | 26 | full recompile to learn there is nothing to do |
| `g record Money …` | 41 | |
| `resource field add Note tags:string` | 47 | |
| `add db --no-start` | 68 | |
| `add api` | 102 | |
| `--pretend --output json g record …` | 70 | 924,449 lines of output |
| `doctor` | 540–1,040 | spawns `java`, `jshell` and `mvn -version` |
| `test NoteTest` / `test --fast NoteTest` / `test` | 6,452 / 3,872 / 6,149 | a JVM; `--fast` still ran Maven |

`strace -f -e trace=execve` on `g scaffold`: one `execve`, the binary
itself. No mutation starts a subprocess.

### 1.3 Latency, thirty entities

Thirty `g scaffold Thing<n> id:uuid@pk name:string! count:long` in a loop:

| n | `g scaffold` ms |
|---:|---:|
| 5 | 108 |
| 10 | 172 |
| 20 | 339 |
| 30 | 510 |

At thirty: `model plan` 417 ms, `model check --frozen` 228 ms, `resource
field add` 507 ms, `routes` 13 ms, `model check` 6 ms. **Every mutation is
linear in the size of the project, about 16 ms per scaffolded entity**, and
`model check` (parse and link only) is not. `strace -c` on `model plan` at
thirty shows 477 `openat` calls: every one of the 422 managed files is read
on every command, on top of a 6.5 MB lock.

### 1.4 The lock

| project | managed bytes | `compiler.lock.json` | ratio |
|---|---:|---:|---:|
| 1 entity | 28,326 | 428,011 | 15.1× |
| 30 entities | 427,108 | 6,509,900 | 15.2× |

Method: `wc -c .jails/compiler.lock.json` against `find .jails/generated
-type f | xargs cat | wc -c`. The lock stores every managed file's BASE
bytes as a pretty-printed JSON array of integers (`"bytes": [47, 47, 32,
…]`), so 28 KB of Java costs 137 KB inside `projection` and the rest is
the indentation. It is committed. `--pretend --diff g record Money3 …` on
the one-entity project prints 48,397 diff lines, of which 48,360 are the
lock (`awk '/^\+\+\+ /{f=$2} /^[-+]/{n[f]++}'`).

### 1.5 What `new` writes and what `.jails/` holds

`jails new demo --offline` writes `pom.xml`, `src/`, `mise.toml`,
`AGENTS.md`, `.gitignore`, `.jails/model.jdl`, `.jails/compiler.lock.json`
and an empty `.jails/apply.lock`; its one output line names none of them
(`Created ./demo offline (deps: web,devtools, Java 26)`). `.gitignore` does
not cover `.jails/run/` or `.jails/apply.lock`. The first `g scaffold`
splices a `build-helper-maven-plugin` block with three executions into the
pom so the build can see `.jails/generated/{main,test}` (S60.7 holds the
plan for that). The seed model carries six `prop` lines and one `dep` the
reader did not write. The strings the code still refuses by name --
`.jails/ledger.toml`, `.jails/model.toml`, `.jails/objects` -- are the
legacy formats (`grep -rn 'ledger.toml' src`).

## 2. What the walk found, and what each finding asks for

Each item is a finding reproduced on the binary, the change it asks for,
and how to tell it is done. Group headings are the five bars above.

### Clear: one report shape

**I70.1 -- print the delta, not the tree.** After `resource field add Note
tags:string` the report says `10 files written` and lists 22 lines, 19 of
them `write` over files git shows unchanged (`git status --short`: two new
files, the model and the lock). The `write` verb means "in the plan", not
"changed". Show `create`, `delete`, `patch` and *changed* `write` lines
only, one `unchanged <n>` line for the rest, and make the count the length
of the list. **Exit:** `git status --short | wc -l` equals the number of
lines the report prints, on every mutation in `tests/cli/model`.

**I70.2 -- one JSON, carrying the same value as the human report.**
`--output json` on `g record` prints five counts (`jails.execution.v1`) and
no file; `--pretend --output json` prints the whole bundle, 924,449 lines
with every after-image; `--output json routes` prints the human table and
`routes --json` prints JSON. Three encodings of "what happened" and two
spellings of "as JSON". One rule: `--output json` is the same report as
human output -- status, the file list with verbs, the diagnostics -- on
every command, and the bundle is what `--plan-out <file>` writes. The
per-command `--json` becomes a hidden alias for one release and goes.
**Exit:** `rows with --json` in §1.1 reads 0; `jails --output json g record
X a:long | jq '.files | length'` equals the human list length.

**I70.3 -- `--diff` diffs managed files, never the lock.** The lock is
derived from the plan; a diff of it is a diff of the diff. **Exit:** the
`awk` in §1.4 finds no `compiler.lock.json` hunk.

**I70.4 -- one "not a project" refusal.** Outside any project the same
situation is reported three ways: *this directory is not a Java project:
jails reads the base package off the shallowest source…* (`g`, `add`,
`sync`), *no pom.xml (or build.gradle, settings.gradle, build.xml,
BUILD.bazel) in this or any parent directory* (`doctor`, `routes`, `test`),
*could not read application model `.jails/model.jdl`* (`model check`), and
only the first carries a `fix:`. One message, one fix line, decided in
`model_command::root` where the one walk already is. **Exit:** the thirteen
commands in the experiment log print byte-identical refusals outside a
project.

**I70.5 -- fix lines name a command, and no message names a TOML table.**
Ten refusals point at `[entities]`, `[capabilities]`, `[settings]` or
`[dependencies]` (`grep -rn 'declared under \`\[' src crates`), sections of
the `model.toml` this tree refuses by name; *remove db* answers *retire
every table through an explicit schema policy before removing `db`* and
names no command; `destroy record Nope` says *name an entity declared under
`[entities]`*. `every_command_a_message_tells_the_reader_to_run_is_one_that_exists`
already scans backticked commands; add its complement -- every `fix:` line
contains a backticked `jails` command or a file path -- and rewrite the ten.
**Exit:** the grep above returns nothing and the new gate is green.

**I70.6 -- an entity called `Note` must not read as a label.** `g scaffold
Note …` twice prints `Note: nothing to do, the project already matches the
model`. Print `nothing to do: the project already matches the model` and
put the name in the plan line, where it already is. **Exit:** no report
line begins with an identifier followed by a colon.

**I70.7 -- `"sample-bodie"`.** `named_json_sample` pluralises the field name
and trims one `s`, so `body` becomes `bodie` in `requests/notes.http` and
in `NoteControllerTest`'s `CREATE_REQUEST`. Snake-case the name and stop.
**Exit:** the golden for a `body:string?` scaffold reads `"sample-body"`.

### Obvious: the first screen

**I70.8 -- the top-level help is one screen.** Fifty words, of which a
reader on day one needs about twenty: `new`, `g`, `add`, `remove`, `set`,
`destroy`, `rename`, `resource`, `sync`, `run`, `test`, `check`, `build`,
`start`, `stop`, `doctor`, `why`, `explain`, `routes`, `beans`. The
protocol and tooling words -- `editor`, `contract`, `request`, `runner`,
`architecture`, `adopt`, `modernize`, `setup`, `bench`, `migrate`, `about`,
`src`, `notes`, `stats`, `lint`, `logs`, `kafka`, `db`, `console`, `mvn`,
`gradle`, `fmt`, `clean`, `testd`, `commands`, `completion`, `app`,
`model` -- stay, `hide`-flagged in clap so `jails --help` shows the twenty
under two headings (*change the project*, *run and ask*) and `jails
commands` still lists everything. No behaviour changes; R7 holds. **Exit:**
`jails --help | wc -l` is under 40 and `jails commands --json` still
reports 96 rows.

**I70.9 -- global flags appear where they mean something.** `--plan-in`,
`--plan-out`, `--ast` and `--diff` ride on all 96 rows, including `about`,
`explain`, `completion` and `commands`, and the rationale paragraph under
`--pretend` is printed on every one of them: 33 of the 45 lines of `jails
resource --help` are the seven globals. Keep them global in `Invocation` --
that is the right design -- and print them under one `Global options`
heading on `jails --help` only, with a one-line summary on subcommands. The
essays move to `jails explain --flag pretend`, beside the kind
explanations. **Exit:** `jails resource --help | wc -l` is under 20.

**I70.10 -- `model explain` leads with what the reader pinned.** On a
one-entity project it prints 23 `java-package` rows for packages holding
nothing, then the five rows about `Note`. Print rows whose owner is in the
model first, group by owner, and put the empty layer packages under
`--all`. Take an optional entity name. **Exit:** `jails model explain Note`
prints exactly the rows whose owner is `ent_note` or its fields.

**I70.11 -- `new` says what it wrote and what to do next.** One line for
the project, one line per file the reader did not name (`AGENTS.md`,
`mise.toml`), one `next:` line. `jails new` also leaves `git init` with no
commit and a `.gitignore` that will commit `.jails/run/` and
`.jails/apply.lock`; ignore both, and delete `apply.lock` at the end of a
successful `new` the way `sweep_staged` removes its own debris. **Exit:**
`git status --short` in a fresh `jails new` lists no file under
`.jails/run` or `.jails/apply.lock`; the creation report names every file
outside `src/` and `pom.xml`.

### No magic, and nothing said twice

**I70.12 -- a warning about a fact the source states is not a warning.**
Every command on a project whose `app` block says `storage none` prints,
on stderr with the `jails:` error prefix, one `storage-absent` warning per
entity: two entities, two warnings, on `set`, `unset`, `rename`, `model
plan`, `model check --frozen`, `eject`, every `g`. The reader wrote
`storage none`; the model says it; `resource status` says it; `doctor` says
it. Say it once, at the moment it becomes true (the first `use scaffold`
under `storage none`), and never as a `jails:` line, which is what a
refusal looks like. The same rule applies to the sequential-scan warning
beside it. **Exit:** `jails g scaffold Note … 2>&1 >/dev/null | grep -c
'^jails:'` is 0 on the repeat run.

**I70.13 -- write `@id(…)` only when it is pinned.** JDL v1 §8 makes
`@id` optional and derived deterministically, and the specification's own
complete example (§4) carries none; the tool writes one on every
declaration, so a seven-field entity is a quarter `@id` by characters
(`grep -c '@id(' .jails/model.jdl` on the demo: 8 of 26 lines). `set
server.port=8081` writes `prop server.port = "8081"
@id(set_64d0f0de270fe184)`, a hash for a key that is its own identity.
`AppModel.derived` already decides `pinned` by comparing a value with its
convention; the formatter applies the same rule to `@id` and materializes
one only where it differs -- which is exactly what §8 says happens at
rename. **Exit:** the seed model and a scaffold carry no `@id`; `rename
resource` materializes exactly one; `model fmt --check` passes on every
fixture under `tests/`.

**I70.14 -- what jails writes, jails' formatter accepts.** `g usecase
CreateNote --on Note` appends the `command` block without the blank line
`model fmt` wants, so `model fmt --check` fails on a model no hand has
touched. Every JDL edit in `jdl/v1/edit.rs` goes through the formatter
before the plan captures the after-image. **Exit:** `model fmt --check`
passes after every mutation in `tests/cli/model`.

**I70.15 -- the seed model's six `prop` lines carry a reason.** They exist
so a capability declaring the same key collides visibly (CLAUDE.md, `new`).
A reader's first sight of `.jails/model.jdl` is six properties they did not
write and cannot tell from their own. One comment line above the block --
`# written by jails new; yours to edit or delete` -- and JDL v1 §5.2
already permits it. **Exit:** the seed template carries the line and
`model fmt` keeps it.

### No surprises

**I70.16 -- a read never writes.** `jails test --fast NoteTest` declares
`cap fast-test @id(cap_fast_test)` in the model and edits `pom.xml`
(`git status` after the run: `M .jails/model.jdl`, `M pom.xml`), and in
this walk still ran Maven, without saying why. A test run that changes
the build is the surprise a reader will not forgive twice. `--fast`
refuses with `fix: jails add fast-test` when the launcher is absent
(`remove fast-test` already exists; give it its `add`), and says in one
line which path it took and why. **Exit:** `git status --short` is empty
after any `jails test` invocation on a clean tree.

**I70.17 -- one command is one plan.** `g scaffold Task … --index 'done,
created_at desc'` prints two `applied model patch` lines and appends two
migrations (`V002__create_tasks.sql`, `V003__add_idx_…`), because the
index rides as a second mutation after the scaffold. `--index` at creation
is part of the `create table`. **Exit:** one plan digest, one migration
with the index inside it.

**I70.18 -- an error surfaces on the command that caused it.** `g usecase
CreateNote --on Note` with no fields is accepted and writes `command
CreateNote() {}`; `set`, `rename` and every `g` keep working, and the first
`add db` refuses with *canonical command `create_note` cannot construct
required field `title`* -- a defect in one command reported by an unrelated
one, days later, at the moment storage arrives.
`refuse::preflight` should run the storage-independent half of that check
at `g usecase` time, or the linker should refuse an operation that can
construct none of its entity's required fields under either storage.
**Exit:** the `g usecase` above refuses with the same message.

**I70.19 -- one spelling per verb.** Three ways to add a field (`g field`,
`resource field add`, re-running `g scaffold` with a longer list, which
refuses with a message that names the second); two ways to skip a prompt
(`--force` on `destroy`, `rename`, `remove`; `--yes` on `console`,
`runner`); two `rename`s (the legacy positional pair and `rename
resource`); `--dry-run` beside `--pretend`; `model plan --bundle` beside
`--plan-out`; `app plan` beside `app apply --pretend`. Keep the one that
states the whole answer: `resource field add`, `--yes`, `rename resource`,
`--pretend`, `--plan-out`; the others become hidden aliases for one
release, then go. `g field` is a kind that is not an artifact, which is
why it does not fit the `g` list. **Exit:** `jails commands --json` shows
one flag for each of the five questions and no `field` kind.

**I70.20 -- `destroy` and `remove` answer with the command.** `destroy
scaffold Todo` with no terminal answers *this deletion needs an answer and
nothing is connected to read one from* -- true, and the reader's next
keystroke is `--force`, which on `destroy` reads as "ignore the guard" and
cannot ignore it (the operation-edge refusal stands with or without it).
`--yes` (I70.19) says what it does. `remove db` over accepted tables names
no command; the fix is `jails destroy scaffold <Entity> --storage
preserve|drop` per table, and the message can list them. **Exit:** both
refusals name the exact command.

**I70.21 -- `about` speaks the project's language.** On a single-module
project it reports `Reactor`, `Module` and `Modules (0): (none)`. Print
the reactor rows only when there is more than one module. **Exit:** the
single-module `about` fits in five lines.

### Milliseconds, at thirty entities

**I70.22 -- the lock is a tree of files, not an array of integers.** The
15× ratio in §1.4 is the format, not the content: integer arrays instead
of strings, pretty-printed. Storing BASE as one file per managed path
under `.jails/base/<project path>` -- the tree the lock already describes
-- with the lock itself reduced to `path -> digest`, makes the ratio 1.0,
makes a `git diff` of the base readable per file, and lets git deduplicate
unchanged blobs across commits. It does not delete the merge base, which
`docs/00-contracts.md` §6.1 forbids for the half-applied reason; it
changes its encoding. The interim step, if the tree layout waits: strings
instead of integer arrays, which re-encoding the one-entity lock in place
measures at 1.48× compact and 1.70× pretty-printed (a Python re-encode of
`projection.files[*].bytes` as UTF-8 strings). **Exit:** the ratio row in
§1.4 reads under 1.1 on both projects with the tree layout, under 1.5 with
the interim one.

**I70.23 -- capture reads what the plan needs.** At thirty entities a
mutation opens 477 files. The lock already carries every managed path and
its digest; a capture that stats each path and reads only those whose size
or mtime moved -- verifying by digest before trusting either -- reads a
handful. The compiler is pure and the emitters are per node, so a render
keyed by `(entity digest, snapshot facts, compiler version)` can be
memoised across the same run's twenty files, and the lock serialised only
for entries that changed. The order of work: **instrument first**. A
`JAILS_PROFILE=1` line per phase -- read lock, capture, link, compile,
materialize, execute, write lock -- on every mutation, so the next change
is aimed at a number. README's *Latency work behind a measurement* asked
for exactly this measurement; §1.3 is it. **Exit:** `g scaffold Thing31`
on the thirty-entity project is under 50 ms; `tests/cli` holds one
scale test asserting a thirty-entity mutation costs less than five times
the one-entity mutation on the same machine, a bound loose enough never
to flake and tight enough to catch linear regressing to quadratic.

**I70.24 -- `doctor` is the slowest read-only command and the first one a
newcomer runs.** 540 ms release, 1,040 ms debug, all of it in `java
-version`, `jshell -version` and `mvn -version`. Run the three probes
concurrently and cache each answer under `.jails/run/` keyed by the
executable's path and mtime. **Exit:** `doctor` under 100 ms warm.

**I70.25 -- "nothing to do" is decided before compiling.** The repeat
`g scaffold` costs 26 ms of full recompilation to learn the model did not
change. The edit is a byte-preserving function of the source; when the
edited source equals the source on disk and the lock's model digest
matches, the answer is known before capture. **Exit:** the repeat row in
§1.2 reads 5 ms.

## 3. What this file does not propose

- A `jails dev` supervisor, an interactive TUI, or a wizard: README says
  why, and the save-and-reload loop is the answer.
- Any new JDL construct. Every item above is a *fewer-bytes* change to
  what the tool writes, inside `docs/00-contracts.md` §6.2. I70.13 removes
  bytes the specification already says are optional.
- Shorter aliases for kinds and capabilities. Thirty-nine kinds is the
  product's breadth (§1.1 of the contracts), and tab completion is how a
  closed set is typed.
- Moving managed output out of `.jails/generated` -- S60.7 owns that, and
  §1.5 here is evidence for it, not a second plan.
- A readable ejection path -- A3.15 owns it; `model eject` refusing
  `Note.repo.fake` while the `art_…` id is visible only in the generated
  file's first line is the reader's side of that item.
- `jails adopt resource` -- P8.11a.

## 4. The experiment log

Everything in §1 and §2 was produced by these, in a scratch directory,
with `J=target/release/jails` (§1.2 was also run with the debug binary):

```
$J new demo --offline --no-git && cd demo && git init -q && git add -A && git commit -qm base
$J g scaffold Note id:uuid@pk title:string! body:string?     # twice
$J model explain; $J model check; $J model check --frozen; $J model plan
$J routes; $J beans; $J stats; $J doctor; $J about
$J --output json g record Money amount:long currency:string
$J g scaffold note id:uuid@pk; $J g scafold Note; $J g record Foo a:strng
$J g record Foo a:string@idx; $J g record Foo when:date!
$J g scaffold Note id:uuid@pk title:string                    # refuses: which change?
$J g query OpenNotes title:string --on Missing
$J g usecase CreateNote --on Note                             # accepted; see I70.18
$J g query NotesByTitle title:string --on Note                # refuses: use the bare name
$J destroy scaffold Note; $J destroy record Nope
$J add db --pretend                                           # refuses through CreateNote
$J resource status Note; $J resource field add Note tags:string --pretend
$J rename Note Memo --pretend; $J rename resource Note Memo --strategy preserve-table --pretend
$J set server.port=8081 --pretend
$J --pretend --output json g record Money2 amount:long | wc -l
$J --pretend --diff g record Money3 amount:long | wc -l
$J model fmt --check; $J model fmt
$J model eject ent_note; $J model eject Note.repo.fake; $J model eject art_ent_note_repository_memory --pretend
# a copy with a hand edit in NoteService.java, then:
$J resource field add Note tags:string; $J resource field add Note rating:int
# a copy without the usecase, then:
$J add db --no-start
$J g scaffold Task id:uuid@pk name:string! done:boolean createdAt:instant --index 'done, created_at desc'
$J resource field add Task priority:int                       # refuses: needs a backfill
$J add api; $J remove db --force
# thirty entities:
for i in $(seq 1 30); do $J g scaffold Thing$i id:uuid@pk name:string! count:long; done
strace -f -c $J model plan; strace -f -e trace=execve $J g scaffold Thing31 id:uuid@pk
# the real toolchain, JDK 26 through mise:
mvn -q -B test-compile                                        # 38 s cold, BUILD SUCCESS
$J test NoteTest; $J test --fast NoteTest; $J test
# a hand-written pom.xml with one class, no model:
$J g record Sku code:string!                                  # seeds the model itself, 46 ms
```

The generated project compiles under JDK 26 and its generated tests pass;
the adoption path (`g record` on a project jails did not create) seeds a
model without a separate `model init`. Neither is a finding; both are
what the walk was checking first.

## 5. Not measured in the first walk

`jails new` against start.spring.io, Gradle projects, `add kafka` and the
broker tools, `testd`, `run --watch`, `jails.nvim`, and every command that
needs a container. The second walk below takes most of them.

---

# The second walk: deep, wide, wild

The first walk (§1 to §5) measured the surface a reader meets in the first
hour. This half goes below it (where a mutation spends its time), beside it
(Gradle, Kafka, twelve capabilities at once, the example manifests, the
editor protocol, hand-edited JDL, a running application) and past it (the
ideas that would change what the tool *is* to a reader, each checked
against the code before it was written down). Items here are `I71.n`;
the number is a stable identifier and nothing else. Same setup as §1:
2026-09-02, `target/release/jails`, Linux, 4 cores, scratch projects from
`jails new <name> --offline --no-git`, wall time from a shell wrapper
around `date +%s%N`.

## 6. Deep: where a mutation spends its time

`--debug` already prints a stopwatch (`Stopwatch` in `model_generate.rs`,
marks at `capture`, `compile`, `materialize`, `execute`). Its help line says
*print the mvnw/mvnd/mvn/java/git/curl commands jails executes*, so nobody
looking for a profile finds it; it is the number I70.23 asked for, and it
was there.

| entities | capture | compile | materialize | execute | whole command |
|---:|---:|---:|---:|---:|---:|
| 1 (`g record`) | 5.8 ms | 0.5 ms | 11.0 ms | 22.3 ms | 41 ms |
| 100 (`g scaffold`) | 307 ms | 14.5 ms | 630 ms | 798 ms | 1,664 ms |

**The pure compiler is one per cent of the time.** What scales is the
exact-plan machinery around it, and each part is proportional to the
*tree*, not the *change*:

- **capture** reads every managed file: 477 `openat` at thirty entities,
  1,457 at a hundred (`strace -c`), on every command that captures.
- **materialize** builds a before-tree and an after-tree with every file's
  bytes in `blobs` (`captured_tree` and `reconcile::tree` in
  `materialize.rs`), so the bundle for adding one record on a one-entity
  project is 924,449 lines of JSON.
- **execute** re-hashes every file named in `plan.base.files` in
  `verify_preconditions`, stats and hashes every entry of the after-tree
  in `publish_merged_tree` to skip the unchanged ones, then writes the
  lock: 21.2 MB at a hundred entities.

The rest of the scale table, one to a hundred entities:

| command | 1 | 30 | 100 |
|---|---:|---:|---:|
| `g scaffold` | 38 ms | 510 ms | 1,664 ms |
| `model plan` | 29 ms | 417 ms | 2,106 ms |
| `model explain` | 12 ms | -- | 420 ms |
| `resource status <Entity>` | 14 ms | -- | 283 ms |
| `model check` | 5 ms | 6 ms | 9 ms |
| `model fmt --check` | -- | -- | 15 ms |
| `routes` | 5 ms | 13 ms | 23 ms |
| `editor symbols routes` | 5 ms | -- | 22 ms |
| `doctor` | 540 ms | -- | 825 ms |
| `compiler.lock.json` | 428 KB | 6.5 MB | 21.2 MB |
| managed files | 19 | 423 | 1,403 |

**The model side is already fast** (`model check` and `model fmt --check`
stay under 20 ms at a hundred entities). The tree side is what a reader
waits for, and two commands that need no tree read it anyway: `model
explain` opens 1,457 files at a hundred entities although every row it
prints is a function of the model, and `resource status` pays 283 ms for
one entity.

**I71.1 -- `model explain` and `resource status` read the model, not the
tree.** Both answer from `AppModel` and the lock's path list. **Exit:**
`strace -c` on either shows under 30 `openat` at a hundred entities, and
both finish under 20 ms there.

**I71.2 -- a manifest replay is one capture.** `jails app apply` on the
web-crawler manifest that is already applied costs 1,915 ms: fourteen
rows, fourteen full pipelines, nothing to do. A replay that links every
row into one edited source and runs the pipeline once is one plan (211 ms
on that project for `model plan`) and one report. **Exit:** the idempotent
replay costs what one `model plan` costs.

**I71.3 -- the executor's work is proportional to the change.** The lock
already records every managed path and its accepted digest. Verify
preconditions by `stat` (size and mtime against what the last execution
recorded), hashing only what moved; publish only entries whose digest
differs from the lock's; hash the whole tree once in `verify_after` only
when `--paranoid` (or `check --frozen`) asks. The crash proof in
`tests/crash.rs` stays as it is: the sweep and the lock are untouched.
**Exit:** `g scaffold Thing101` on the hundred-entity project is under
100 ms, and `execute` in the stopwatch under 20 ms.

**I71.4 -- the bundle carries what changed.** Trees keyed by digest,
blobs only for entries whose digest is not already in the lock, and the
lock named by digest as the bundle's base. `--plan-out` for one record on
one entity is then a few kilobytes, and `--pretend --output json` stops
printing after-images nobody asked for. **Exit:** the 924,449-line bundle
in §1.2 is under 200 lines.

**I71.5 -- the stopwatch is a first-class flag.** `--timing` (or
`JAILS_PROFILE=1`) prints the four phases plus what each read (files,
bytes) on every mutation, separately from `--debug`'s command echo. It is
the instrument I70.23 asked for; it exists; it needs a name a reader can
find. **Exit:** `jails --timing g record X a:int` prints the table above
for that run.

## 7. Wide: what the other surfaces said

### 7.1 The whole application, and running it

`jails new crawler --offline --no-git --app examples/web-crawler/.jails/app.toml
--no-start`: **2,171 ms** from nothing to 123 files (99 managed), 18
routes, 6 migrations, a 93-line model, a 3.7 MB lock -- and 887 lines of
output, one `applied model patch … sha256:…` per manifest row. `jails run`
on the one-entity project, JDK 26 through mise: **first `201 Created`
24.6 s after the command** (Maven build then Boot start); `GET /notes/{id}`
for a missing id is 404, a blank title is a 400 problem detail. `jails
test NoteTest` is 6.5 s and `jails test` 6.1 s, all JVM.

### 7.2 Gradle

`jails new gr --gradle --offline`: 13 ms, and the wrapper jar is honestly
absent with the fix line. `g scaffold` 35 ms, `add db --no-start` 49 ms
with the marked blocks in `build.gradle`; `add format` refuses by name in
14 ms, as CLAUDE.md says it must. `jails test --pretend` ran Gradle for
48 s (§7.5).

### 7.3 Twelve capabilities in one command

`jails add db api actuator security cors json testkit docker ci k8s
observability cache --no-start`: **69 ms**, 43 files, `Dockerfile`,
`compose.yaml`, `deploy/`, `.github/workflows/{ci,image}.yml`, 46 lines in
`application.properties`, a 1.08 MB lock, and one `applied` line naming
all twelve. `add kafka --no-start` after it: 102 ms. Nothing here is slow;
the report is the only thing a reader has to scroll.

### 7.4 The JDL-first workflow

Appending by hand to `.jails/model.jdl`:

```jdl
entity Tag {
  use scaffold
  id: uuid @pk
  name: string @notBlank @unique

  command Rename(id, name) {
    route POST "/tags/{id}/rename"
  }
}
```

then `jails sync`: **53 ms**, sixteen files, four new routes. The
hand-written entity carries no `@id` and needs none; the CLI's later
`g record Y a:int` appended its own declaration byte-preservingly (the
diff is the four new lines) -- which is I70.13's proof: only the CLI
frontends write `@id`, and the language never asked for it.

Errors: the parser reports a position (`[JDL0114] line 36, column 31, byte
771: attribute `@uniq` is not valid here`, then the closed list); the
linker reports a path (`[model-field-type]
$.entities.tag.fields.name.type: `strin` is an unknown field type`) and a
typo in one type produces **four** diagnostics, three of them consequences
of the first. The specification's own complete example (`docs/01-jdl-v1.md`
§4) refuses `model check` with `model-ejection-target` -- the one
recorded gap, A3.15 -- so the first document a reader copies from does
not check.

### 7.5 What `--pretend` does on a command that only runs

`jails test --pretend NoteTest` runs Maven for 7.3 s on the Maven project
and Gradle for 48 s on the Gradle one, and `jails check --pretend` runs the
build. The flag is global, accepted everywhere, and means nothing on a
command that writes nothing.

### 7.6 Repair, conflict, loss

- Delete a managed file, then `jails sync`: refuses (*deleted by you while
  the generator still needs it*) and names `jails resource repair`, which
  takes no selector and writes it back. So `sync` does not make the tree
  match the model when a file is missing; a second verb does.
- Hand-edit a managed file, then change the model: the edit is merged
  forward; `doctor` reports *changed since generation; jails merges the
  edit forward on every sync*. Correct and visible.
- Delete `compiler.lock.json`, then mutate: the mutation succeeds, a new
  lock is written, the hand edit survives the next merge, and nothing says
  the merge base was lost and rebuilt from the model.

### 7.7 Two small self-contradictions

`jails lint` on a project one `g scaffold` old reports `pom.xml:58:
spring-boot-starter-web; use spring-boot-starter-webmvc` -- `new` writes
`webmvc`, the scaffold's dependency reconciliation adds `web` with
`<scope>compile</scope>`, and the linter forbids it: the compiler declares
the starter its own linter rejects. `jails resource repair
Tag` refuses because repair takes no selector, on a command family whose
every other verb takes one.

### 7.8 The editor protocol and the plugin

`editor handshake` 6 ms, `symbols routes` 5 ms (22 ms at a hundred
entities), `diagnostics --scope project` 5 ms, `complete` 4 ms:
keystroke-fast. `jails.nvim/lua/jails/init.lua` is 926 lines carrying
preview and apply of prepared plans, a watch loop, diagnostics into the
quickfix list, pickers over routes, beans, tests and types, and a JDL
buffer configuration; Neovim itself is not installed here, so none of it
was driven.

### 7.9 The words a reader has to learn

A census over every `--help` text at every depth (200,885 characters,
4,955 lines, from the rows of `jails commands --json`) and over every
string literal of twenty or more characters in `src/` and `crates/`
(303,656 characters):

| word | help | messages |
|---|---:|---:|
| authenticated prepared transaction | 192 | -- |
| exact | 112 | 52 |
| semantic | 102 | 62 |
| projection | 100 | 58 |
| reconcile | 100 | -- |
| frozen | 97 | -- |
| canonical | 11 | 139 |
| artifact | 9 | 120 |
| lock | 3 | 101 |
| reader | 6 | 84 |

The 192 is `--plan-out`/`--plan-in` repeated on 96 rows; `exact`,
`semantic`, `projection` and `reconcile` are the same seven global flags
echoed. And three words name one thing: in messages `entity` 213 times,
`resource` 129, `scaffold` 18; in help 22, 46 and 24. The JDL says
`entity`, the CLI says `resource`, the refusals say *canonical entity*.

### 7.10 Items from the wide walk

**I71.6 -- `--pretend` refuses where it means nothing.** On `test`,
`run`, `check`, `build`, `clean`, `mvn`, `gradle`, `console`, `bench`,
`migrate`, `kafka`, `db`: *`test` runs a JVM and writes nothing;
`--pretend` does not apply*, in 5 ms, before any JVM starts. **Exit:**
`jails test --pretend` returns in under 10 ms with that line.

**I71.7 -- one verb makes the tree match the model.** `sync` repairs a
deleted managed file the way `resource repair` does, saying so in its
report (`restore  <path>  deleted by hand`); `resource repair` becomes an
alias for one release. **Exit:** the deletion in §7.6 is healed by `jails
sync` and the report names the file.

**I71.8 -- a lost merge base is said out loud.** When no lock exists and
managed files do, the mutation prints one line -- *no compiler lock:
treating the managed tree as accepted; edits since the last generation
cannot be told from generated code until the next one* -- and `doctor`
carries the same row. **Exit:** the §7.6 run prints it.

**I71.9 -- one noun.** `entity` is the language's word and the model's;
help and messages use it for the thing and `scaffold` for the facet.
`jails resource …` stays as the advertised command (R7), gains `entity`
as its visible name with `resource` the alias, and *canonical* leaves
every message: there is one model now, and the word marks a split that no
longer exists. **Exit:** `grep -c canonical` over the message corpus is 0;
the census row for `resource` in messages is 0.

**I71.10 -- a vocabulary budget, gated.** The census is a test: a closed
list of words that may not appear in a `--help` text or a message
(*authenticated prepared transaction*, *canonical*, *semantic*, *exact*,
*projection*, *reconcile* -- each replaced by the plain word: *plan
file*, nothing, nothing, nothing, *generated tree*, *update*), measured
over `jails commands --json` and the literal scan above, with a ratchet
row beside the board's. **Exit:** the six words read 0 in both columns.

**I71.11 -- linker diagnostics carry a line, and the cascade collapses.**
The CST keeps spans (`jdl/v1/cst.rs`, `token.rs`); the linker's `$.path`
resolves to the declaration that produced the node, so a `model-*`
diagnostic can print `.jails/model.jdl:36:9` beside the path. A field
whose type is unknown suppresses the diagnostics that depend on that
field (`non_blank`, references from operations), so one typo is one
message. **Exit:** the `strin` typo in §7.4 prints one diagnostic with a
line and column.

**I71.12 -- the compiler and the linter agree.** The scaffold's web
facet declares `spring-boot-starter-webmvc` where the captured Boot version
is 4, or `lint` stops forbidding `web` on a project whose Boot version
accepts it; one `BuildDependency` row, decided by the version the way the
`Import::Moved` rows are. **Exit:** `jails lint` on a fresh scaffold
reports nothing.

**I71.13 -- the report for a manifest or a multi-capability `add` is one
report.** Fourteen `applied model patch … sha256:…` lines become one
summary with the file list grouped by row, and the digest moves to
`--output json`; §7.1's 887 lines become under 150. Depends on I71.2 for
the single plan and on I70.1 for the delta. **Exit:** `jails new --app`
prints one `applied` line.

## 8. Wild: what would change what jails *is* to a reader

Each was checked for feasibility in the code before it was written down;
each names the code that makes it cheap or the contract that makes it
expensive.

**I71.14 -- the report is the model diff.** Every mutation prints, above
its file list, the hunk it wrote into `.jails/model.jdl`: four to six lines
for a scaffold, one for `set`. The bundle already carries the model's
before- and after-image (`ReplaceModelFile { before, after }`), so the
executor has both. The reader learns the language from the tool --
`title:string!` on the command line becomes `title: string @notBlank` on
the screen -- and the CLI is visibly sugar over one source. **Exit:** `g
record Money amount:long` prints the `entity Money { … }` hunk first.

**I71.15 -- `jails undo`.** Every planned operation carries a before-image
(`before: Option<FileImageRef>` in `plan.rs`; blobs in the bundle), so
the inverse plan is the same bundle with before and after swapped and the
current after-images as its preconditions. Keep the last applied bundle at
`.jails/run/last-plan.json` (ignored by git), and `undo` hands the inverse
to the one executor, which refuses if anything moved since. No new writer,
no new contract. **Exit:** `g scaffold X …` then `jails undo` leaves `git
status --short` empty.

**I71.16 -- JDL-first is the documented first path.** §7.4 measured it:
edit the model, `jails sync`, 53 ms. README opens with the CLI and
mentions the model as what the CLI edits; reverse the order, and add
`jails sync --watch`, which re-syncs on every save of `model.jdl` -- the
whole pipeline is 40 ms on a small project, so the loop is an editor
loop. `test --watch` and `run --watch` already own the file-watch
mechanics. **Exit:** README's first example is a JDL block and `jails
sync`; `sync --watch` exists.

**I71.17 -- bare `jails` is `status`.** Today it prints clap's usage.
Print what `git status` prints for a repository: the model in one line
(entities, operations, capabilities), what the lock has not accepted,
managed files edited by hand, services declared and not running, the JDK
row -- every fact already computed by `model check`, `doctor` and
`resource status`, assembled without the three version probes so it is
under 20 ms. `jails status` is the same command with a name. **Exit:**
bare `jails` inside a project prints the summary in under 20 ms; outside
one, the usage.

**I71.18 -- `g <kind> --help` is about the kind.** `jails g scaffold
--help` prints the 235 generic lines. Print the `explain` entry for the
kind (they exist for all 39, three to eleven lines each) and the flags
that kind accepts, derived from the same place the frontends refuse a
flag that does not apply (`--on` without a target, `--index` off a
scaffold), never a hand-written table. **Exit:** `jails g scaffold --help`
is under 40 lines and names `--index`, `--path`, `--timestamps`.

**I71.19 -- an LSP for the model.** `jails editor` already emits
diagnostics and symbols as versioned JSON in 5 ms; a `jails lsp` speaking
the Language Server Protocol over stdio gives every editor completion of
the closed sets (attributes, types, `use` projections, `cap` kinds --
the registries in `docs/10-language.md`), hover from `explain`, spans
from I71.11 and go-to-generated-file from the artifact id. `jails.nvim`
becomes thinner, not thicker. **Exit:** `jails lsp` answers
`textDocument/completion` after `@` with the attribute list.

**I71.20 -- agents are readers too: `jails mcp`.** `AGENTS.md` is written
for them, `commands --json` is already a tool schema and `--output json`
the wire. A Model Context Protocol server over stdio, derived from the
same clap tree, offers every subcommand as a tool with its flags as
parameters and returns the JSON report. Nothing runs inside jails, so it
is not the plugin system the scope bar refuses. Open question, not a
plan: whether the surface a tool exposes to an agent should be the whole
tree or the twenty words of I70.8.

**I71.21 -- the manifest is the model.** `.jails/app.toml` is a second
declarative source: `capabilities` and `[[generate]]` rows that are CLI
calls in TOML, replayed one pipeline each (§6, I71.2). Everything a
manifest says, JDL says -- caps, entities, fields, indexes, operations,
components -- and `app init` already refuses on a modelled project, which
is the code agreeing. `jails new <name> --model crawler.jdl` is a copy and
one `sync`: 200 ms for the crawler instead of 2 s, one report, one
source. `examples/*/.jails/app.toml` become `model.jdl` files, which also
makes them the JDL corpus the specification lacks. **Exit:** `new --app`
accepts a `.jdl`; the examples carry one; `app.toml` is read for one
release and then refused by name like `model.toml`.

**I71.22 -- the second `jails run` skips Maven.** 24.6 s to the first
request is a Maven build in front of a 3 s Boot start. `launcher.rs`
already decides staleness for `test --fast` and knows the classpath;
`run` on a tree whose classes are newer than its sources can `java -cp`
the main class directly and say so in one line, falling back to Maven
loudly when anything is stale. **Exit:** the second `jails run` on an
unchanged tree answers its first request in under 6 s.

**I71.23 -- the closed sets complete on the command line.** Field types,
markers (`@pk`, `@unique`, …), `--on` targets (the entities in the
model), `--yields` events, `--via` fields: every one is a closed set jails
knows at completion time, and `jails completion bash` completes none of
them today because the arguments are free-form `String`s. `jails editor
complete` already answers in 4 ms; the shell completer calls it. **Exit:**
`jails g query X st<TAB>` completes `status:` from the entity.

## 9. What the second walk does not propose

- A daemon or a resident process for the mutation path. §6 shows the
  pure compiler at 14 ms on a hundred entities; the milliseconds are in
  reading and writing the tree, and I71.3 and I71.4 get them back without
  a process to manage.
- Dropping the merge base. I71.3 and I71.4 change what is read and
  carried, not what is kept; `docs/00-contracts.md` §6.1 stands.
- Replacing JDL with the manifest, or the CLI with JDL. I71.21 removes a
  *third* source; I71.16 orders the two that remain.

## 10. The experiment log, second walk

```
# deep
$J --debug g record Dbg1 a:int                      # the stopwatch, 1 entity
for i in $(seq 31 100); do $J g scaffold Thing$i id:uuid@pk name:string! count:long; done
$J --debug g scaffold Thing101 id:uuid@pk name:string!
$J model plan; $J model explain; $J resource status Thing50; $J model fmt --check; $J routes; $J doctor
strace -c $J model explain                          # 1,457 openat
# wide
$J new gr --gradle --offline --no-git; $J g scaffold Note id:uuid@pk title:string!; $J add db --no-start; $J add format; $J test --pretend; $J doctor
$J new many --offline --no-git; $J add db api actuator security cors json testkit docker ci k8s observability cache --no-start; $J add kafka --no-start
$J new crawler --offline --no-git --app examples/web-crawler/.jails/app.toml --no-start; $J app apply --no-start; $J model plan
$J run   # then curl -X POST localhost:8080/notes until 201; GET a missing id; POST a blank title
$J --pretend test NoteTest
cat >> .jails/model.jdl <<'JDL' … entity Tag … JDL; $J model check; $J sync; $J routes
sed -i 's/@unique/@uniq/' .jails/model.jdl; $J model check       # and a missing brace, and `strin`
# the spec's §4 example as the model:  $J model check
$J editor handshake --output json; $J editor symbols routes --output json; $J editor diagnostics --scope project --output json
rm <managed file>; $J sync; $J resource repair Tag; $J resource repair
echo '// my edit' >> <managed file>; rm .jails/compiler.lock.json; $J g record Q b:int; $J resource field add Note extra:string; $J doctor
$J lint; $J why run.log; $J notes; $J contract emit; $J src Note
# the census: every `<row> --help` from `jails commands --json`, and
#   grep -rhoE '"[^"]{20,}"' src crates --include=*.rs
```

## 11. Not measured

Neovim (not installed here), the Kafka tools against a broker, `testd`,
`run --watch`, `test --affected`, `jails db`, `jails console`, `bench`,
`migrate --check`, and `jails new` against start.spring.io.

---

# The third walk: the loops, the documents, the corners

Same setup. This half measures the loops the README leads with (the resident
JVM, save-and-reload, the database path with a container), reads the three
documents against the binary, and pokes the corners of the field syntax.
Items continue the `I71.n` series.

## 12. The loops

### 12.1 The resident JVM cannot see a generated test

README §*testd* promises 0.06 to 0.10 s per test method against a resident
JVM. On the scaffolded project:

| command | result | ms |
|---|---|---:|
| `jails testd NoteTest` | *could not resolve `NoteTest` to a fully qualified name* | 11 |
| `jails test --engine warm --compile none com.example.democlean.domain.NoteTest` | *strict warm execution is ineligible: … has no attributable test source* | 24 |
| `jails test NoteTest` (auto, Maven) | 1 test, green | 3,830 |
| `jails test --engine warm --compile none com.example.democlean.domain.NoteHandTest` (the same test copied by hand into `src/test/java`, project under a short path) | 1 test, green; daemon start on the first run | 3,905 then 132, 106, 117 |
| `jails testd NoteHandTest` (bare name, hand-written test) | 1 test, green | 96 |

The reason is one line: `run/isolation.rs` builds the warm engine's test
universe from `src/test/java` alone, and every test a scaffold writes lives
under `.jails/generated/test/java`. On a project made of scaffolds the
resident JVM can run none of its tests, while a hand-written test under
`src/test/java` runs in about 100 ms by its bare name, exactly as README
promises. The refusal is what misleads: *pass its fully qualified test
class* is not the fix, the source root is. `testd` prints *a compatibility
alias* on every call, and each `test` run also printed `fast-test: nothing
to do`, the mutation of I70.16 reporting itself.

### 12.2 Staleness does not watch the managed tree

`run/fingerprint.rs` stamps `src/main/java`, `src/main/resources`,
`src/test/java`, `src/test/resources`, `.mvn`, `gradle` and the build
files; `.jails/generated` is not in the list. It turns out not to matter
yet: `jails test --fast NoteTest` prints a selection block --
`Maven: NoteTest, reason: Widened("NoteTest has no attributable test
source")` -- and hands the class to Maven on every run, fresh or stale,
for the §12.1 reason. The fast path is never taken for a generated test, so
the blind spot is hidden behind the ineligibility; the day I71.24 makes
generated tests eligible, the fingerprint has to learn the managed tree in
the same change or `--fast` will run stale classes with a green result.

### 12.3 Save and reload, measured

`jails run --watch` on a compiled tree: the application answers **4.1 s**
after the command; a saved change to `DemoCleanApplication.java` is
reported (`jails: changed src/main/java/…`), recompiled through Maven and
restarted by devtools, with the second `Started DemoCleanApplication`
line **2.8 s** after the save. From cold (§7.1) the first request is 24.6 s
away. The structured `jails: process-started`, `application-ready` and
`changed` lines are the best output in the tool: an event a script can read
and a person can too.

### 12.4 The database path, with a container

On the project with `db`, two scaffolds and `api`: `jails start db`
**9.96 s** (compose pull and health), `jails migrate` 476 ms and `migrate
--check` 324 ms (three migrations, clean), `doctor` with the database up
1.57 s, `jails test --scope integration` **43.0 s** for 23 unit tests and
the two Testcontainers ITs, all green, `jails stop` 236 ms. The path works
end to end with nothing to write. One row lies: `ok  psql executable
/usr/share/postgresql-common/pg_wrapper -- Can't exec "--version": No such
file or directory` reports a failed probe as `ok`.

### 12.5 `new` with the network

`jails new online1` against start.spring.io: **1,665 ms**, and beside the
offline fixture it adds `mvnw`, `mvnw.cmd`, `.mvn/wrapper` and
`.gitattributes`. Both write `spring-boot-starter-webmvc`; the seed model is
identical apart from the name.

### 12.6 The plan file round trip

`--plan-out p.json g record PlanA a:int`: 579 ms and a **9.6 MB** file for
one record on a one-entity project (I71.4); `--plan-in p.json` applies it in
53 ms; a second `--plan-in` writes nothing and says so. `model plan
--bundle` and `model apply --bundle` do the same under the second spelling
I70.19 names.

## 13. The documents against the binary

| the document says | the binary does | where |
|---|---|---|
| README: `jails history`, `jails show <transaction>`, `jails undo <transaction>` | none of the three exists | README line 839 against `jails commands --json` |
| README, `jails g --help`, spec §9: collections are `list<T>` and `map<K,V>` | `list<string>` is *an unknown field type*, on the CLI and in a hand-written model alike | README line 1266; `docs/01-jdl-v1.md` line 924 |
| spec §2, §17: `--package` is *the sole intentional refusal in v1 managed mode* | `g record Deep a:int --package util` writes `@package(util)` and places the file | `docs/01-jdl-v1.md` lines 89, 2122 |
| README `jails.toml`: `[project] capabilities` is what `sync` applies | `sync` reads the model; `doctor` says so; `new` writes no `jails.toml` | README line 1108; `jails sync --help` |
| twelve messages: *declared under `[entities]`*, `[capabilities]`, `[settings]`, `[dependencies]` | the file has none of those tables | `grep -rn 'declared under' src crates` |
| README's testd numbers, 0.06–0.10 s per method | no generated test is eligible (§12.1) | README line 936 |

`every_command_a_message_tells_the_reader_to_run_is_one_that_exists`
already scans the messages against `jails commands`; nothing scans README
or the specification.

## 14. The corners of the field syntax

- `n:int@default(0)`, `flag:boolean@default(true)`,
  `at:instant@default(now())` work on the command line and land as
  `@default(…)`; README's marker table does not list `@default`.
- `--timestamps` writes `createdAt: instant @default(now())` and
  `updatedAt: instant @default(now()) @updated`, exactly as
  `docs/00-contracts.md` §6.2 says it must.
- `g record Timing when:instant` refuses: *`when` derives the PostgreSQL
  table `when`, which is a reserved word* -- a column on a record that
  declares no storage, refused as a table.
- `g record Bad ref:Missing` prints the best message in the tool: *`Missing`
  is neither in this model nor in your own sources; fix: declare it with
  `jails g record Missing …` or `jails g enum Missing …`, write it yourself,
  or use one of jails' lowercase types*.
- In the crawler, `POST /crawl_runs` and `POST /crawled_pages` sit beside
  `POST /actions/queue-crawl` and `POST /workflows/site-traversal`: table
  names become paths with an underscore, actions with a hyphen.
- `jails explain db` refuses: `explain` knows the 39 kinds and none of the
  25 capabilities.
- Shell completion completes kinds and capabilities (`g sca` → `scaffold`,
  `add k` → `kafka k8s`); `--on` completes file names and a field completes
  nothing, which is I71.23 measured.
- Of 713 `fix:` lines in the tree, 70 name a `jails` command and 18 a file;
  the rest say what to do in words (*use `asc` or `desc`*), which is often
  right and sometimes (*remove a capability declared under
  `[capabilities]`*) is not. Fourteen of the 887 lines `new --app` prints
  carry a `sha256:` digest.

## 15. Items from the third walk

**I71.24 -- every test engine sees the managed tree.** The warm engine's
universe and the staleness fingerprint take their source roots from the
build (the `build-helper` block and the Gradle source set already name
them), so `.jails/generated/{main,test}/java` count until S60.7 makes the
question go away. **Exit:** `jails testd NoteTest` on a fresh scaffold
runs warm in under 200 ms; `test --fast` after a `resource field add`
reports the stale class and falls back.

**I71.25 -- the warm refusal names the real reason.** *No attributable
test source* on a generated test says where the class lives and that the
warm engine does not look there yet, instead of asking for a qualified
name that changes nothing. **Exit:** the first two rows of §12.1 name
`.jails/generated/test/java` in the refusal until I71.24 removes it.

**I71.26 -- collections exist or are not advertised.** Either the model
accepts `list<T>` and `map<K,V>` on non-stored records, as the
specification and README say, or both stop saying it and the compact syntax
refuses with the reason. **Exit:** `g record Bag tags:list<string>`
matches the documents, whichever way.

**I71.27 -- the reserved-word check is about columns, on stored entities.**
`model-sql-reserved` fires only when the entity has storage, and says
*column*. **Exit:** `g record Timing when:instant` succeeds.

**I71.28 -- `--package` has one story.** The specification calls it the
one refusal; the binary writes `@package(...)`. Either the attribute is a
language feature with §6.2's full price (grammar, formatter, stable-ID
rule, conformance test) and the specification says so, or the flag refuses
on a managed projection as the specification says. **Exit:** the spec and
`jails g record X a:int --package p` agree.

**I71.29 -- README is scanned like a message.** The oracle that keeps
messages honest reads README's backticked `jails <word>` too, so `history`,
`show` and `undo` either exist or leave the file (`undo` is I71.15; the
README already promised it). **Exit:** the first row of §13 is gone.

**I71.30 -- `explain` knows capabilities.** One entry per capability
beside the kinds, checked by the same build-time test. **Exit:** `jails
explain db` prints one.

**I71.31 -- one path style.** Resource paths derive from the same plural
as the table and spell it with hyphens (`/crawl-runs`), so a project's
routes have one convention; the table keeps its underscore. **Exit:**
`jails routes` on the crawler prints no path with `_`.

**I71.32 -- a probe that fails is not `ok`.** `doctor`'s executable rows
report a version they could not read as `warn` with the error. **Exit:**
the `psql executable` row in §12.4 reads `warn`.

**I71.34 -- the daemon refuses a socket path it cannot bind.** With the
project under a 115-byte path the warm engine dies with a Java stack trace
ending in `ServerSocketChannel.bind` and *fix: inspect the daemon
diagnostic above*; a unix socket path is limited to 108 bytes, and the same
project copied to `/tmp/jc` starts the daemon in 1.8 s and answers in about
100 ms after that. Compute the path length before starting the JVM and
refuse with the limit and the fix (a shorter checkout, or a socket under
`$XDG_RUNTIME_DIR` keyed by the project's digest). **Exit:** the refusal
names the byte count and no stack trace is printed.

**I71.33 -- README's measurements name their subject.** The testd and
`--fast` numbers say which project and which tests they were taken on, and
carry the §12.1 caveat until I71.24 closes. **Exit:** README line 936
names the project.

## 16. Not measured in the third walk

The Kafka tools against a broker, `jails db` and `jails console` (both
interactive), `bench`, `test --affected` on a tree with real changes, and
Neovim.
