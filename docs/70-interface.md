<!--
The interface research: what a reader of jails sees, typed and waits for,
measured on the binary rather than read off the README. `docs/60-abstraction.md`
is the shape of the code; this is the shape of the *surface*, and the two are
kept apart so a deletion inside the compiler is never mistaken for a change
the reader can feel.

**A closed item is deleted from this file**, in the commit that closes it.
Item numbers `I70.n` are stable and never reused. Every number below carries
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

## 5. Not measured

`jails new` against start.spring.io (no network here), Gradle projects,
`add kafka` and the broker tools, `testd`, `run --watch`, `jails.nvim`,
and every command that needs a container. Each of those deserves the same
walk before an item about it is written here.
