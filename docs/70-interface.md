<!--
The interface research: what a reader of jails sees, types, edits, commits
and waits for, measured on the binary. `docs/60-abstraction.md` is the
shape of the code; this file is the shape of the surface, organised by
subject rather than by when each thing was found.

**A closed item is deleted from this file**, in the commit that closes it.
Item numbers `I70.n` and `I71.n` are stable identifiers and nothing more;
the series number carries no meaning. Every number carries the command that
produced it (Appendix A); re-measure before quoting one.
-->

# 70 — The interface: what to change, and why

**Read `docs/00-contracts.md` first.** Nothing here reopens a contract. The
compiler is right; what this file measures is how much of it a reader has
to *know* before `jails g scaffold Note title:string!` feels obvious, how
long they wait, and what happens to them in git afterwards.

Every section has the same shape: **Today** is what the binary does,
measured; **After** shows the result the section is aiming at, as a
reader would see it; the items say what to **change** to get there and how
to tell it is **done**. Where a change could go two ways, the item says
which way and why. An item reads:

> **I70.n — the change, as an imperative.** *Today* … *Change* … *Done when* …

All numbers: 2026-09-02, `cargo build --release` of this tree
(`target/release/jails`, 12.6 MB), Linux, 4 cores, scratch projects from
`jails new <name> --offline --no-git`, JDK 26 through mise where a JVM
runs, wall time from a shell wrapper around `date +%s%N`. The debug binary
is within 2× on every row.

## The bar

- **Clear.** One spelling per verb, one report shape, one JSON encoding.
- **Obvious.** The first screen of `--help` fits on a screen. A refusal
  names the fix. A file jails writes is one the reader asked for, or the
  report says it was written.
- **No magic.** Nothing derived is hidden, and nothing is repeated: a
  warning about a fact the source states is noise.
- **No surprises.** A read never writes. A count matches its list. One
  command is one plan. An error surfaces on the command that caused it.
  Deleting a folder called `.jails` does not delete the application.
- **Milliseconds.** Every operation that starts no JVM finishes under
  50 ms on a thirty-entity project, measured.

## The plan

**First, the structural change (§1): done.** Generated Java lives in
`src/` like any other Java, nothing generated reads `.jails` at build or
test time, and a project whose `.jails/` is gone builds, passes the same
tests, and is told by the next `jails g` what it lost. The scanners ask one
`input_roots` now, so what is left of §1 is the lock's shape, which is §2's.
S60.7 in `docs/60-abstraction.md` names the mechanism.

**Then, in this order,** each one commit or one short series, none
reopening a contract:

| # | items | what a reader gets |
|---|---|---|
| ~~1~~ | ~~I70.12~~ | ~~the most-seen line in the tool stops printing on every command~~ |
| ~~2~~ | ~~I70.1, I71.13~~ | ~~the report is the delta; a manifest prints one report~~ |
| ~~3~~ | ~~I71.35, I70.19, I70.20~~ | ~~consent is `--yes`, nothing else, and JSON has no shortcut past it~~ |
| ~~4~~ | ~~I70.13, I71.47, I71.16~~ | ~~the model file reads like the specification's, and editing it by hand is the first path~~ |
| 5 | I70.22, I71.3, I71.4, I71.5 | the lock is 1× the tree, a scaffold is a 20-line diff, a mutation at a hundred entities is under 100 ms |
| ~~6~~ | ~~I71.40, I71.41, I71.24~~ | ~~every scanner sees every source root; `test --affected` never selects nothing and passes~~ |
| ~~7~~ | ~~I70.2~~ | ~~one JSON encoding, carrying the same value as the human report~~ |
| 8 | ~~I70.8~~, I70.9 (one line over), I71.18 | a one-screen `--help`, global flags printed once, `g <kind> --help` about the kind |
| 9 | I71.29, I71.26, I71.28 | README, the specification and the binary agree |
| ~~10~~ | ~~I71.14~~ | ~~every mutation prints the JDL it wrote~~ |

**Worth a prototype before a decision:** `jails undo` (I71.15), bare `jails`
as status (I71.17), an LSP for the model (I71.19), an MCP server (I71.20),
the manifest folded into the model (I71.21). `sync --watch` was prototyped
and declined; §9 says why.

---

## 1. The project layout: a Java project first

### Today

**Where the code is.** `src/` holds the shell `new` wrote; the application
is under `.jails/generated`:

**The relocation has landed.** Generated Java, SQL, resources and `.http`
files are emitted under `src/` beside the reader's own, the lock says
which paths are jails', and no build file declares a second source root.
The numbers below were measured on 2026-09-02, before that; the shape of
`.jails/` is re-measured under §2, and the scanner table further down
predates the move and is what its own items are measured against.

`ls -a` at the root of a `new --offline --no-git` project plus one
`g scaffold Note id:uuid@pk title:string!` prints `AGENTS.md mise.toml
pom.xml src` and `.jails`; `grep -rl '\.jails' src` is empty; `rm -rf
.jails && mvn test` is `BUILD SUCCESS` with the same 13 tests it runs with
`.jails` in place, `ArchitectureTest` among them (2026-09-03).
`.jails/` holds `model.jdl`, `compiler.lock.json`, an empty `apply.lock`,
a `.gitignore` naming the two scratch entries, `run/` (a classpath cache,
an affected-index file, the daemon's socket) when a daemon has run, and
`app.toml` on manifest projects.

**The scanners now ask one question.** `inspect::roots::input_roots` is the
one answer to "where is the source", and the affected index, the watch
fingerprint, the Kafka topic scan, `jails lint` and the editor handshake all
read it; a caller resolves the roots once and passes the slice down. A change
under a root jails does not know widens `--affected` to everything and names
the path, rather than being hidden by a pathspec and reading as "nothing
changed" (2026-09-03).

### After

A jails project is a Maven or Gradle project with a folder of inputs
beside it:

```
notes/
  pom.xml                                  no build-helper block; the pom of any Boot project
  jails.toml                               layout and the architecture policy
  src/main/java/com/example/notes/
    NotesApplication.java
    domain/Note.java                       // jails: art_ent_note_record  -- every generated file, first line
    repository/NoteRepository.java
    service/NoteService.java
    web/NoteController.java  NoteRequest.java  NoteResponse.java
    adapters/jdbc/JdbcNoteRepository.java
  src/main/resources/db/migration/V001__create_notes.sql
  src/test/java/com/example/notes/…        generated tests beside hand-written ones
  src/test/http/notes.http
  src/test/resources/archunit/frozen/      the freeze store, ArchUnit's own convention
  .jails/
    .gitignore                             `apply.lock` and `run/`: scratch, never committed
    model.jdl                              the source
    lock.json                              one line per managed path: path, artifact id, digest
    base/src/main/java/…                   BASE bytes, one file per managed path (§2)
    run/                                   ignored: daemon socket, caches
```

`rm -rf .jails` leaves a project that builds and passes its tests and can
no longer regenerate; the next `jails g` says so. `rg`, `ls`, the IDE, the
test selector and the Kafka tool see one source tree because there is one.
`jails model relocate` moves an existing project across once: it moves
the files, rewrites the lock's paths, removes the build-helper block, and
refuses if any destination already exists.

### Change

**I71.44 — `.jails/` is the input, and only the input.** *Today* it holds
`model.jdl`, the lock, and two scratch entries a `.gitignore` keeps out of
every commit; deleting it leaves a project that builds and passes its
tests, and the next `jails g` refuses by name rather than seeding a second
model over the generated tree. What is left is the lock's *shape*:
`compiler.lock.json` is still every managed file's BASE bytes as a JSON
array of integers, 15× the tree it describes. *Change* the lock becomes a
manifest of path, artifact id and digest, with the merge base beside it as
a tree of files — which is I70.22 in §2, and is the whole of what remains
here. *Done when* I70.22 is done.

---

## 2. Generated files in git

### Today

**One scaffold is a 42,000-line diff.** In a committed project the second
`g scaffold` changes `compiler.lock.json` by +27,859/−14,230 lines beside
7 lines of `model.jdl` and 14 new files. The lock is 15× the tree it
describes (468 KB for 31 KB; 21.4 MB for 1.4 MB) because it stores every
file's BASE bytes as a pretty-printed JSON array of integers; re-encoded
as strings the same lock measures 1.48× compact and 1.70× pretty. `.`
sorts before `s`, so the lock is the first file in every pull request.
`--pretend --diff` for one record prints 48,397 lines, 48,360 of them the
lock. Before `git add`, the whole scaffold is one line: `?? .jails/generated/`.

**Two branches, one entity each, then `git merge`:** one conflict hunk in
`model.jdl` (both appended after the same line) and 167 in the lock.
Resolve the model by hand and jails refuses the conflicted lock (*key must
be a string at line 4*); take either side's lock and `sync` refuses
(*generated path `TagRequest.java` is already reader-owned; fix: move the
existing file or explicitly import it*), because the other branch's files
are on disk with no accepted digest; `resource repair` refuses with the
same sentence. The one way through is `rm .jails/compiler.lock.json &&
jails sync`, which rebuilds the lock; `model check --frozen` then passes.
It works, nothing documents it, and it re-baselines every hand edit on
both branches without saying so.

**A lost lock is silent.** Delete `compiler.lock.json` and mutate: the
mutation succeeds, a new lock is written, hand edits survive the next
merge, and nothing says the merge base was rebuilt from the model.

**Repair has two verbs.** Delete a managed file, then `sync`: refuses
(*deleted by you while the generator still needs it*) and names `resource
repair`, which takes no selector and writes it back. `resource repair Tag`
refuses because repair takes no selector, on a command family whose every
other verb takes one.

**Two commands at once:** a `g record` started 5 ms after a `g scaffold`
refuses with *stale exact plan: `compiler.lock.json` no longer matches its
captured precondition*. Correct, and it never says another jails was
running or that rerunning is the fix.

**What works.** A fresh clone passes `model check --frozen`. `git status`
after `rename resource Task Todo --strategy preserve-table` shows `R
Task.java -> Todo.java` for seven files, and the model diff is three
readable lines. `.gitattributes` with `.jails/compiler.lock.json -diff`
turns the scaffold's lock diff into `Bin 822840 -> 835086 bytes`. A hand
edit to a managed file is merged forward on the next change and `doctor`
says so. `remove json` over a hand-edited file refuses with the fix.
`.jails/.gitignore` covers `apply.lock` and `run/`, so neither reaches a
diff; `new`'s own root `.gitignore` is untouched and does not have to
know.

### After

```
$ git merge alice
Auto-merging .jails/model.jdl
Auto-merging .jails/lock.json
Merge made by the 'ort' strategy.
$ jails sync
accepted  7 files from alice (Task)      matching the model, headers intact
accepted  7 files from bob (Tag)
nothing to regenerate
$ git show --stat HEAD~1
 .jails/lock.json                          |  7 +
 .jails/base/src/main/java/.../Task.java   | 21 +
 src/main/java/com/example/notes/domain/Task.java | 21 +
 …
```

A scaffold is a diff a reviewer can read: one line per file in the lock,
the BASE file beside the source file, no integer arrays. Two branches
adding two entities merge without a conflict because each appended beside
the entities, not at the end of the file, and the lock is one sorted line
per path. A lost lock or a lost base is announced, never silently rebuilt.

### Change

**I70.22 — the merge base is a tree of files, not an array of integers.**
*Change* BASE is one file per managed path under `.jails/base/<project
path>` and the lock is `path -> artifact id, digest`: the ratio becomes
1×, a base diff is readable per file, git deduplicates unchanged blobs.
The merge base is kept (`docs/00-contracts.md` §6.1); only its encoding
changes. Interim, if the tree layout waits: strings instead of integer
arrays (1.48×). *Done when* the ratio reads under 1.1 with the tree, under
1.5 with the interim.

**I71.45 — a merge is a merge.** *Change* the model conflicts only where
two branches touched the same declaration (step 4 puts a new entity beside
the entities rather than at the end, which is half of it); the manifest is one sorted line per path and
merges by line; the base tree merges by file. After a merge, `jails sync`
accepts a file that carries a provenance header and matches the model's
render for its artifact, and reports which files it accepted and which it
re-baselined. *Done when* the two-branch scenario merges with `git merge`
and one `jails sync`, no deletion, and the report lists the files each
side brought.

**I71.46 — until then, the two lines `new` can write today.** The ignore
half is done, and not where this said: the executor writes
`.jails/.gitignore` naming `apply.lock` and `run/`, from inside, so an
adopted repository and a `--no-git` project get it too and a reader's own
root `.gitignore` is left alone. *Change* what is left: `.gitattributes`
with `.jails/compiler.lock.json -diff merge=binary`, and README
documenting `rm .jails/compiler.lock.json && jails sync` as the merge
resolution with the re-baseline caveat. *Done when* a fresh `new` carries
the `.gitattributes` and the lock never appears in a `git diff`.

**I71.8 — a lost merge base is said out loud.** *Change* when no lock
exists and managed files do, the mutation prints *no compiler lock:
treating the managed tree as accepted; edits since the last generation
cannot be told from generated code until the next one*, and `doctor`
carries the row. *Done when* the lost-lock run prints it.

**I71.7 — one verb makes the tree match the model.** *Change* `sync`
restores a deleted managed file (BASE is in the base tree, the render is
the model's) and says `restore <path> deleted by hand`; `resource repair`
is an alias for one release. *Done when* the
deletion above is healed by `jails sync` and the report names the file.

**I71.37 — a concurrent run is named.** *Change* *another jails run
changed this project while this one was planning; run the command again*.
*Done when* the race prints that line.

**I70.3 — `--diff` diffs managed files, never the lock.** *Change* the
lock and the base tree are derived from the plan and are skipped by
`--diff`, which prints the model hunk and the managed files. *Done when*
the lock has no hunk in a `--diff`.

**I71.15 — `jails undo` (prototype).** *Change* add `jails undo`, built
on what exists: every planned operation carries a before-image (`before: Option<FileImageRef>` in `plan.rs`; blobs in the
bundle), so the inverse plan is the same bundle with before and after
swapped and the current after-images as preconditions. Keep the last
applied bundle at `.jails/run/last-plan.json`; `undo` hands the inverse to
the one executor, which refuses if anything moved. README already
documents an `undo` that does not exist (§8). *Done when* `g scaffold X`
then `jails undo` leaves `git status --short` empty.

---

## 3. The model file and the language

### Today

**The file the tool writes is the file the specification shows** (2026-09-03,
after step 4). A generated model carries no `@id`: the writer emits one only
where it differs from the derivation, by the rule `AppModel.derived` uses for
`pinned`, and `rename resource` is what materialises one. Declarations go in
where a reader keeping the file tidy would have put them, the type column of
a run of members is lined up, and the layout is decided once in the mutation
pipeline -- so `model fmt --check` passes after every mutation in
`tests/cli/model`, over a source that was already canonical, which is how JDL
v1 §19.2's byte preservation survives. `model fmt` formats anything the
parser accepts and reports the linker's answer afterwards.

**Diagnostics.** The parser gives a position (`[JDL0114] line 36, column
31: attribute `@uniq` is not valid here`, then the closed list). The
linker gives a path (`[model-field-type] $.entities.tag.fields.name.type`),
and one typo in a type produces four diagnostics, three of them
consequences. `editor diagnostics --scope project` returns `[]` on a model
`model check` refuses. A name that cannot be a Java identifier is refused
once in the frontend, before the model: `2Fast` and `Café` get the same
sentence and the same fix.

**What the language accepts.** `@default(0)`, `@default(true)`,
`@default(now())` work on the command line; `--timestamps` writes the two
fields `docs/00-contracts.md` §6.2 says it must. `list<string>` is *an
unknown field type* on the CLI and in a hand-written model, although
README, `g --help` and the specification (§9) allow it on non-stored
records. `g record Timing when:instant` refuses *`when` derives the
PostgreSQL table `when`, which is a reserved word*: a column on a record
with no storage, refused as a table. `--package util` writes
`@package(util)` although the specification (§2, §17) calls `--package`
*the sole intentional refusal*. The specification's own §4 example refuses
`model check` (A3.15, the recorded gap), so the first document a reader
copies from does not check. A relation is `g association CommentNote
noteId=id --on Comment --yields Note`, written as `relation commentNote to
Note`: the CLI says `--yields` where the language says `to`, and a field
typed by an entity (`note:Note`) is a second way to relate that embeds
instead. Nothing in the binary teaches the grammar: `model --help` mentions
JDL once, `model check --help` and `explain <kind>` never.

### After

```
$ jails g record Money amount:long currency:Colour
  .jails/model.jdl
+ entity Money {
+   amount:   long
+   currency: Colour
+ }
create  src/main/java/com/example/notes/domain/Money.java
create  src/test/java/com/example/notes/domain/MoneyTest.java
2 created
```

A typo is one diagnostic with a position (`.jails/model.jdl:14:18:
`strin` is not a type jails knows; a type you own is capitalised`), in the
editor as well as on the command line. `jails explain jdl` prints the
grammar the parser accepts. Collections work on non-stored records,
`--package` is `@package` in the specification as well as in the binary, and
a relation's parent is `--to`.

### Change

**I71.11 — linker diagnostics carry a line, and the cascade collapses.**
*Change* the CST keeps spans (`jdl/v1/cst.rs`), so a `model-*` diagnostic
prints `.jails/model.jdl:36:9` beside its path; a field whose type is
unknown suppresses the diagnostics that depend on it. *Done when* the
`strin` typo prints one diagnostic with a line and column.

**I71.48 — the editor protocol carries the language's diagnostics.**
*Change* `editor diagnostics` runs the same parse and link as `model
check` and maps each code to the schema's diagnostic shape with the span.
*Done when* it returns what `model check` returns, JDL and model codes
alike, with line and column.

**I71.26 — collections exist.** *Change* the model accepts `list<T>` and
`map<K,V>` on non-stored records and component payloads, as the
specification's §9 already says; a stored entity refuses them with the
reason (no column type). Implement rather than un-advertise: three
documents promise it and the compact syntax already parses the angle
brackets. *Done when* `g record Bag tags:list<string>` generates and
`g scaffold Bag tags:list<string>` refuses naming the column.

**I71.27 — the reserved-word check is about columns, on stored entities.**
*Change* `model-sql-reserved` runs only for entities with storage and
names the column, not a table. *Done when* `g record Timing when:instant` succeeds and the message on a
stored entity says *column*.

**I71.28 — `--package` has one story.** *Change* keep the binary and fix
the specification: `@package(name)` becomes a §9 attribute with a
formatter rule, a stable-ID rule (identity does not move with the package)
and a conformance test, and the *sole intentional refusal* sentence goes.
The flag is advertised and works, so R7 forbids the other way round. *Done
when* the specification documents `@package` and `model fmt --check`
passes on a model carrying one.

**I71.38 — `to`, not `--yields`, for a relation's parent.** *Change* `g
association … --on Comment --to Note`, `--yields` a hidden alias for one
release; `explain association` says when a typed field is the better
relation. *Done when* `g association --help` shows `--to`.

**I71.50 — the binary explains its own language.** *Change* `jails explain
jdl` prints the declaration families, the attributes per declaration, the
`use` projections, the builtin types and the `cap` kinds, walked out of
the registries `docs/10-language.md` counts. *Done when* its attribute
count equals the parser's refusal list.


**I71.21 — the manifest is the model (prototype).** *Today*
`.jails/app.toml` is a second declarative source whose `[[generate]]` rows
are CLI calls, replayed one pipeline each (1.9 s for a no-op replay of
fourteen rows). Everything a manifest says, JDL says, and `app init`
already refuses on a modelled project. *Change* `jails new <name> --model
crawler.jdl` is a copy and one `sync`; `examples/*/.jails/app.toml` become
`model.jdl` files, which also gives the specification the corpus it lacks.
*Done when* `new --app` accepts a `.jdl`, the examples carry one, and
`app.toml` is refused by name like `model.toml`.

**I71.19 — an LSP for the model (prototype).** *Change* add `jails lsp`,
a Language Server Protocol server over stdio. `jails editor` already
emits diagnostics and symbols as versioned JSON in 5 ms. A `jails lsp`
over stdio gives every editor completion of the closed sets, hover from
`explain`, spans from I71.11, go-to-generated-file from the artifact id;
`jails.nvim` (926 lines) becomes thinner. *Done when* `jails lsp` answers
`textDocument/completion` after `@` with the attribute list.

---

## 4. What a command prints

### Today

**A repeat `g scaffold Note`** prints `Note: nothing to do`, where the
entity name reads as a label.

**Three encodings of "what happened".** `--output json` on `g record`
prints five counts and no file; `--pretend --output json` prints the whole
bundle, 924,449 lines; `--output json routes` prints the human table while
`routes --json` prints JSON (11 commands carry a per-command `--json`).

**`model explain`** prints 23 `java-package` rows for empty packages before
the five rows about `Note` (524 rows at a hundred entities). **`new`**
prints one line and names none of `AGENTS.md`, `mise.toml`, `.gitignore`,
the model or the lock it wrote. **`about`** on a single-module project
prints `Reactor`, `Module` and `Modules (0): (none)`.

**Refusals, good and bad.** The template exists: `add actuator` over a
colliding `set` answers *conflicts with model value `beans`; fix: remove
the duplicate setting or give it the capability-required value …; nothing
was written*, and `remove json` over a hand edit answers *edited by you but
removed by the generator; fix: move the custom code to reader source, keep
the model component, or repeat with `--yes`*. `g record Bad ref:Missing`
answers *`Missing` is neither in this model nor in your own sources; fix:
declare it with `jails g record Missing …`*. Against that: outside a
project the same situation is reported three ways (*this directory is not
a Java project…*, *no pom.xml (or build.gradle, …)*, *could not read
application model*), one with a fix line; twelve messages point at
`[entities]`, `[capabilities]`, `[settings]` or `[dependencies]`, tables
of a format the tree refuses by name; `kafka lag` on a
fresh group prints a raw `GroupIdNotFoundException` with exit 0; `doctor`
prints `ok psql executable … Can't exec "--version"`. Of 713 `fix:` lines,
70 name a `jails` command and 18 a file. `"sample-bodie"` appears in the
request collection and the controller test because `named_json_sample`
pluralises `body` and trims one `s`.

**The words.** A census over every `--help` text (200,885 characters) and
every message literal of twenty or more characters (303,656 characters):

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

The 192 is the seven global flags echoed on 96 rows. One thing has three
names: in messages `entity` 213 times, `resource` 129, `scaffold` 18. No
report uses colour or `NO_COLOR`; a 23-line file list and a two-line
refusal look alike.

### After

One report shape for every mutation, the same value in JSON:

```
$ jails g scaffold Note id:uuid@pk title:string! body:string?
  .jails/model.jdl
+ entity Note {
+   use scaffold
+   id:    uuid   @pk
+   title: string @notBlank
+   body:  string?
+ }
create  src/main/java/com/example/notes/domain/Note.java
create  src/main/java/com/example/notes/repository/NoteRepository.java
… (15 more)
patch   pom.xml
17 created, 1 patched, 3 unchanged
note    Note has no table: this project declares `storage none`; `jails add db` gives it one

$ jails --output json g scaffold Note …
{"status":"applied","model":{"hunk":"+ entity Note {…"},"files":[{"op":"create","path":"src/main/java/…/Note.java"},…],"notes":[…]}
```

The note prints once, at the moment it becomes true, and never again.
`jails new` names every file it wrote and ends with `next: cd notes &&
jails g scaffold …`. `model explain Note` prints the five rows about
`Note`. A refusal is always *fact, fix, nothing was written*, and every
`fix:` names a command or a file. `canonical`, `semantic`, `exact` and
`authenticated prepared transaction` appear nowhere.

### Change

**I70.2 — one JSON, carrying the human report's value.** *Landed:* every
mutation, preview, `sync` and `model apply` prints one
`jails.command-result.v2` envelope with the status, the same file list the
screen shows -- each entry a verb and a path, off the one `Delta` both
encodings render -- the declaration written into the model, the counts and
the notes. The bundle is what `--plan-out` writes, which is where a caller
that wants the reviewed transition looks. The ten per-command `--json`
flags are hidden aliases for one release. `jails request --json` is not
one of them and stays advertised: it is curl's request body, not an
encoding.


**I70.10 — `model explain` leads with what the reader pinned.** *Change*
rows whose owner is in the model first, grouped by owner; empty layer
packages under `--all`; an optional entity argument. *Done when* `jails
model explain Note` prints exactly the rows owned by `ent_note` and its
fields.

**I70.11 — `new` says what it wrote and what to do next.** *Change* one
line for the project, one per file the reader did not name, one `next:`
line; `apply.lock` removed at the end of a successful `new`. *Done when*
the creation report names every file outside `src/` and `pom.xml`.

**I70.21 — `about` speaks the project's language.** *Change* print
`Reactor` and `Modules` rows only when there is more than one module, and
say *project* otherwise. *Done when* the single-module `about` fits in
five lines.

**I70.4 — one "not a project" refusal.** *Change* one message with one fix
line, decided in `model_command::root` where the one walk is. *Done when*
thirteen commands print byte-identical refusals outside a project.

**I70.5 and I71.39 — every refusal is fact, fix, nothing written.**
*Change* rewrite the twelve `[entities]`-style messages; every `fix:`
line names a `jails` command, a file, or gives an imperative sentence;
a gate scans `fix:` lines the way
`every_command_a_message_tells_the_reader_to_run_is_one_that_exists`
scans commands. *Done when* `grep -rn 'declared under \`\[' src crates`
is empty and the gate is green.

**I71.42 — a broker's exception is a refusal.** *Change* `kafka lag`
recognises `GroupIdNotFoundException` and answers *no consumer group
`<name>` has committed yet; run the application once*, exit 1; any other
broker exception is a refusal carrying its first line. *Done when* the
`lag` line on a fresh group is one sentence and the exit code is non-zero.

**I71.32 — a probe that fails is not `ok`.** *Change* an executable row
whose `--version` probe fails reports `warn` with the probe's error and a
fix line. *Done when* the `psql executable` row reads `warn`.

**I71.9 — one noun.** *Change* `entity` for the thing and `scaffold` for
the facet, in help and messages; `jails resource …` keeps working with
`entity` as the visible name and `resource` the alias; *canonical* leaves
every message, there being one model now. *Done when* `canonical` and
`resource` read 0 in the message census.

**I71.10 — a vocabulary budget, gated.** *Change* a closed list of words
that may not appear in help or messages (*authenticated prepared
transaction* → *plan file*, *canonical*, *semantic*, *exact* → nothing,
*projection* → *generated tree*, *reconcile* → *update*), measured by the
census above as a test. *Done when* the six read 0 in both columns.

---

## 5. The command surface

### Today

| what | count |
|---|---:|
| top-level commands | 50 |
| subcommand rows, all depths | 96 |
| generator kinds / capabilities | 39 / 25 |
| distinct non-global flags / flag rows | 107 / 183 |
| rows carrying `--plan-in`, `--plan-out`, `--ast`, `--diff` | 96 of 96 |
| lines of `jails --help` / `jails g --help` / `jails resource --help` | 93 / 235 / 45 |
| of the 45, global-flag boilerplate | 33 |
| README lines / command bullets | 1,758 / 98 |

**Two spellings, still.** `app plan` beside `app apply --pretend`.

**`--pretend` where it means nothing.** `jails test --pretend NoteTest`
runs Maven for 7.3 s, Gradle for 48 s; `check --pretend` runs the build.

**Help and completion.** `jails g scaffold --help` prints the 235 generic
lines. `jails explain db` refuses: `explain` knows the 39 kinds and no
capability. Shell completion completes kinds and capabilities (`g sca` →
`scaffold`, `add k` → `kafka k8s`); `--on` completes file names and a
field completes nothing, although every one of those is a closed set
jails knows and `editor complete` answers in 4 ms. `testd` prints *a
compatibility alias* on every call. Bare `jails` prints clap's usage.

### After

```
$ jails --help
jails — Spring Boot and Maven projects from one model

Change the project
  new        Create a project                      g          Generate from a kind
  add        Add a capability                      remove     Take one out
  set        Set a property                        destroy    Remove a generated thing
  rename     Rename an entity                      entity     Inspect or evolve an entity
  sync       Make the project match the model

Run and ask
  run  test  check  build  start  stop             doctor  why  explain  routes  beans

Global options
  --pretend   Write nothing; show the plan          --yes      Answer every prompt yes
  --output    human | json                          --plan-out / --plan-in   Save or apply a plan file

`jails commands` lists everything, tooling and protocol commands included.
```

`jails g scaffold --help` explains scaffold and lists the flags scaffold
takes. `jails destroy scaffold Note` asks, `--yes` answers, and JSON
without `--yes` refuses. `jails test --pretend` refuses in 5 ms. `<TAB>`
completes kinds, capabilities, entities, fields and markers.

### Change

**I71.6 — `--pretend` refuses where it means nothing.** *Change* on `test`,
`run`, `check`, `build`, `clean`, `mvn`, `gradle`, `console`, `bench`,
`migrate`, `kafka`, `db`: *`test` runs a JVM and writes nothing;
`--pretend` does not apply*, before any JVM starts. *Done when* `jails
test --pretend` returns that line in under 10 ms.


**I70.9 — global flags appear once.** *Landed:* the seven global flags
carry one help line each, the rationale moved to `jails explain --flag
<name>`, and one line at the foot of `jails --help` says they are global
and where the reasons are. *Remains:* a subcommand's help still lists all
seven, so `jails resource --help` is 21 lines against the item's 20; 15 of
those are its own. clap hides an argument from every help or from none, so
collapsing them to a summary line on subcommands needs either per-context
hiding upstream or a hand-written list at the root -- and a hand-written
list of the flags clap already knows is the second source this repository
spends its gates preventing. Declined at one line over, deliberately.


**I71.18 — `g <kind> --help` is about the kind.** *Change* the `explain`
entry for the kind plus the flags that kind accepts, derived from where
the frontends already refuse a flag that does not apply. *Done when*
`jails g scaffold --help` is under 40 lines and names `--index`, `--path`,
`--timestamps`.

**I71.30 — `explain` knows capabilities.** *Change* one entry per
capability in the same table as the kinds, with the build-time test
extended to both. *Done when* `jails explain db` prints one.

**I71.23 — the closed sets complete on the command line.** *Change* the
shell completer calls `editor complete` for field types, markers, `--on`
targets, `--yields` events and `--via` fields. *Done when* `jails g query
X st<TAB>` completes `status:` from the entity.

**I71.17 — bare `jails` is `status` (prototype).** *Change* what `git
status` prints for a repository: the model in one line, what the lock has
not accepted, managed files edited by hand, services declared and not
running, the JDK row; every fact already computed, assembled without the
version probes. *Done when* bare `jails` inside a project prints it in
under 20 ms and outside one prints the usage.

**I71.20 — agents are readers too: `jails mcp` (prototype).** *Change* add
`jails mcp`, a Model Context Protocol server over stdio. `AGENTS.md`
is written for them, `commands --json` is a tool schema, `--output json`
the wire. A Model Context Protocol server over stdio derived from the same
clap tree runs nothing inside jails, so it is not the plugin system the
scope bar refuses. Expose I70.8's twenty words plus `commands` and
`explain`; an agent that needs the protocol commands has the CLI.

---

## 6. No surprises

### Today

- **A test run edits the model.** `jails test --fast NoteTest` declares
  `cap fast-test @id(cap_fast_test)` and edits `pom.xml`; every later
  `test` prints `fast-test: nothing to do`.
- **One command, two plans.** `g scaffold Task … --index 'done, created_at
  desc'` prints two `applied` lines and appends two migrations
  (`V002__create_tasks.sql`, `V003__add_idx_…`).
- **An error surfaces later.** `g usecase CreateNote --on Note` with no
  fields is accepted and writes `command CreateNote() {}`; `set`,
  `rename` and every `g` keep working; the first `add db` refuses with
  *canonical command `create_note` cannot construct required field
  `title`*.
- **The compiler declares what its linter forbids.** After one scaffold,
  `jails lint` reports `pom.xml:58: spring-boot-starter-web; use
  spring-boot-starter-webmvc`: `new` writes `webmvc`, the scaffold's
  dependency reconciliation adds `web`.
- **A stack trace for a long path.** Under a 115-byte project path the
  test daemon dies in `ServerSocketChannel.bind` (a unix socket path is
  limited to 108 bytes) with *fix: inspect the daemon diagnostic above*;
  the same project under `/tmp/jc` starts the daemon in 1.8 s and answers
  in about 100 ms after that.
- **Two path styles in one application.** The crawler serves `POST
  /crawl_runs` beside `POST /actions/queue-crawl`: table names become
  paths with an underscore, actions with a hyphen.
- **README's testd numbers** (0.06 to 0.10 s per method) do not say they
  exclude every generated test.
- What is right: property collisions and hand-edit removals refuse well
  (§4); `g scaffold Empty` refuses with *needs exactly one `@pk` field …
  for example `id:uuid@pk`*; a required field added to a stored entity
  refuses until it has a backfill; ejection followed by a model change
  still compiles.

### Change

**I70.16 — a read never writes.** *Change* `--fast` refuses with `fix:
jails add fast-test` when the launcher is absent (`remove fast-test`
exists; give it its `add`) and says which path it took. *Done when* `git
status --short` is empty after any `jails test` on a clean tree.

**I70.17 — one command is one plan.** *Change* `--index` at creation is
part of the `create table`. *Done when* one plan digest and one migration.

**I70.18 — an error surfaces on the command that caused it.** *Change* the
storage-independent half of the check runs at `g usecase` time, or the
linker refuses an operation that can construct none of its entity's
required fields under either storage. *Done when* the `g usecase` above
refuses with that message.

**I71.12 — the compiler and the linter agree.** *Change* the web facet
declares `spring-boot-starter-webmvc` where the captured Boot version is
4, decided once the way the `Import::Moved` rows are. *Done when* `jails
lint` on a fresh scaffold reports nothing.

**I71.34 — the daemon refuses a socket path it cannot bind.** *Change*
compute the length before starting the JVM and refuse with the limit and
the fix (a shorter checkout, or a socket under `$XDG_RUNTIME_DIR` keyed by
the project's digest). *Done when* the refusal names the byte count and no
stack trace prints.

**I71.31 — one path style.** *Change* resource paths derive from the same
plural as the table and spell it with hyphens (`/crawl-runs`), through a
second `DerivedValue` role so `model explain` shows it; an existing
project keeps its paths through `use scaffold(path: …)`, which `rename`
already writes. *Done when*
`jails routes` on the crawler prints no path with `_`.

**I71.33 — README's measurements name their subject.** *Change* the testd
and `--fast` paragraphs say which project and which tests their numbers
were taken on. *Done when*
README line 936 names the project.

---

## 7. Speed

### Today

**One entity.** Process floor 5 ms (`--version`, `model check`, `routes`,
`beans`, `stats`, every refusal outside a project). `new` 12 ms, `model
explain` 12, `model plan` 29, `g scaffold` 38 (20 files), the same again
26 (a full recompile to learn there is nothing to do), `g record` 41,
`resource field add` 47, `add db --no-start` 68, `add api` 102, twelve
capabilities in one `add` 69, `--plan-out` for one record 579 ms and a
9.6 MB file, `--plan-in` 53. No mutation starts a subprocess (`strace`:
one `execve`). `doctor` 540 ms release, 1,040 debug, all in `java`,
`jshell` and `mvn -version`.

**Scale.** Thirty scaffolds in a loop, then a hundred:

| command | 1 | 30 | 100 |
|---|---:|---:|---:|
| `g scaffold` | 38 ms | 510 ms | 1,664 ms |
| `model plan` | 29 ms | 417 ms | 2,106 ms |
| `model check --frozen` | 24 ms | 228 ms | -- |
| `model explain` | 12 ms | -- | 420 ms |
| `resource status <Entity>` | 14 ms | -- | 283 ms |
| `model check` | 5 ms | 6 ms | 9 ms |
| `model fmt --check` | -- | -- | 15 ms |
| `routes` | 5 ms | 13 ms | 23 ms |
| `editor symbols routes` | 5 ms | -- | 22 ms |
| `doctor` | 540 ms | -- | 825 ms |
| `compiler.lock.json` | 428 KB | 6.5 MB | 21.2 MB |
| managed files | 19 | 423 | 1,403 |

Linear, about 16 ms per scaffolded entity; the model side (`model
check`, `fmt --check`) stays under 20 ms at any size.

**Where the time goes.** `--debug` already prints a stopwatch, under a
help line about echoing Maven commands:

| entities | capture | compile | materialize | execute |
|---:|---:|---:|---:|---:|
| 1 (`g record`) | 5.8 ms | 0.5 ms | 11.0 ms | 22.3 ms |
| 100 (`g scaffold`) | 307 ms | 14.5 ms | 630 ms | 798 ms |

The pure compiler is one per cent. Capture reads every managed file (477
`openat` at thirty, 1,457 at a hundred); materialize copies every file's
bytes into the bundle's `blobs` (924,449 lines of JSON for one record on
one entity); execute re-hashes every precondition in
`verify_preconditions`, stats and hashes every after-tree entry in
`publish_merged_tree`, then writes the 21 MB lock. `model explain` opens
1,457 files at a hundred entities although every row is a function of the
model. A no-op `app apply` of the fourteen-row crawler manifest costs
1.9 s: fourteen full pipelines.

**The loops.** `jails test NoteTest` 6.5 s and `jails test` 6.1 s (Maven);
a hand-written test through the warm engine 3.9 s on the first run, then
106 to 132 ms, 96 ms as `testd NoteHandTest`. `jails run` from cold
answers its first request 24.6 s after the command (Maven build, then a
3 s Boot start); `run --watch` on a compiled tree is up in 4.1 s and
restarts 2.8 s after a save, reporting `jails: changed <path>`. With a
container: `start db` 10.0 s, `migrate` 476 ms, `migrate --check` 324 ms,
`test --scope integration` 43.0 s for 23 unit tests and 2 ITs, `stop`
236 ms; `start kafka` 14.0 s; `console` with a script on stdin 4.9 s;
`db -- -c '\dt'` 688 ms. `new` against start.spring.io 1,665 ms.

### Change

**I71.5 — the stopwatch is a first-class flag.** *Change* `--timing`
prints the four phases and what each read, on every mutation, apart from
`--debug`'s command echo. *Done when* `jails --timing g record X a:int`
prints the table above for that run.

**I71.3 — the executor's work is proportional to the change.** *Change*
verify preconditions by `stat` against what the last execution recorded,
hashing only what moved; publish only entries whose digest differs from
the lock's; hash the whole tree only under `check --frozen`. The crash
proof in `tests/crash.rs` is untouched. *Done when* `g scaffold Thing101`
on the hundred-entity project is under 100 ms and `execute` under 20 ms.

**I71.4 — the bundle carries what changed.** *Change* trees keyed by
digest, blobs only for entries not already in the lock, the lock named
as the bundle's base. *Done when* `--plan-out` for one record is a few
kilobytes and the 924,449-line bundle is under 200 lines.

**I70.23 — capture reads what the plan needs.** *Change* stat each locked
path and read only what moved; memoise the per-node render across a run;
serialise the lock only for changed entries; add one scale test in
`tests/cli` asserting a thirty-entity mutation costs under five times a
one-entity one. *Done when* `g scaffold Thing31` on the thirty-entity
project is under 50 ms.

**I70.25 — "nothing to do" is decided before compiling.** *Change* when
the edited source equals the source on disk and the lock's model digest
matches, answer before capture. *Done when* the repeat scaffold is 5 ms.

**I71.1 — `model explain` and `resource status` read the model, not the
tree.** *Change* both answer from `AppModel` and the lock's path list and
never call capture. *Done when* both show under 30 `openat` at a hundred entities and
finish under 20 ms.

**I71.2 — a manifest replay is one capture.** *Change* link every row into
one edited source and run the pipeline once. *Done when* the idempotent
replay costs what one `model plan` costs (211 ms on the crawler).

**I70.24 — `doctor` under 100 ms warm.** *Change* run the three version
probes concurrently and cache each under `.jails/run/` keyed by the
executable's path and mtime. *Done when* the second `doctor` in a minute
is under 100 ms.

**I71.22 — the second `jails run` skips Maven (prototype).** *Change* on a
tree whose classes are newer than its sources, `java -cp` the main class
directly (`launcher.rs` already decides staleness and knows the
classpath), saying so in one line, and fall back to Maven loudly. *Done
when* the second `jails run` on an unchanged tree answers in under 6 s.

---

## 8. The documents against the binary

| the document says | the binary does | where |
|---|---|---|
| README: `jails history`, `jails show <transaction>`, `jails undo <transaction>` | none of the three exists | README line 839 against `jails commands --json` |
| README, `jails g --help`, spec §9: collections are `list<T>` and `map<K,V>` | *unknown field type*, on the CLI and in a hand-written model | README line 1266; `docs/01-jdl-v1.md` line 924 |
| spec §2, §17: `--package` is *the sole intentional refusal* | `--package util` writes `@package(util)` | `docs/01-jdl-v1.md` lines 89, 2122 |
| README `jails.toml`: `[project] capabilities` is what `sync` applies | `sync` reads the model; `new` writes no `jails.toml` | README line 1108 |
| twelve messages: *declared under `[entities]`* and kin | the file has no such tables | `grep -rn 'declared under' src crates` |
| README's testd numbers | no generated test is eligible (§1) | README line 936 |
| README: *every unknown widens* `--affected` | a change under `.jails/generated` selects nothing (§1) | README line 954 |

`every_command_a_message_tells_the_reader_to_run_is_one_that_exists`
scans messages against `jails commands`; nothing scans README or the
specification.

**I71.29 — README is scanned like a message.** *Change* the same oracle
reads README's backticked `jails <word>`; `history`, `show` and `undo`
exist or leave the file (`undo` is I71.15). *Done when* the first row
above is gone. I71.26, I71.28 and I71.33 close the rest.

---

## 9. Not proposed, and not measured

Not proposed: a `jails dev` supervisor, a TUI or a wizard (README says
why; the save-and-reload loop is the answer); any new JDL construct (every
item is a fewer-bytes change inside `docs/00-contracts.md` §6.2); shorter
aliases for kinds and capabilities (tab completion is how a closed set is
typed); a daemon for the mutation path (the compiler is 14 ms at a hundred
entities; the time is in reading and writing the tree); dropping the merge
base (§6.1 stands); replacing JDL with the manifest or the CLI with JDL
(I71.21 removes a third source, and README now leads with the JDL block and
`jails sync`). A readable ejection path is A3.15's; `jails adopt resource` is
P8.11a's.

Prototyped and declined: **`sync --watch`** (was I71.16's second half). It is
cheap and safe -- a no-op `sync` is 22-26 ms on a scaffolded project, 69 ms
when it declares an entity, `run --watch`'s debounce is already written, and
`sync` does not rewrite `model.jdl`, so a watch cannot retrigger itself. It
is the *input* that makes it wrong: `run --watch` watches compiled classes,
which an editor writes only when they are complete, while `model.jdl` is
watched while a hand is typing in it. Every save of a half-written
declaration is a parse refusal, and a debounce cannot tell one from a
finished edit -- so the loop's normal output is a screen of diagnostics about
a document nobody has finished. `jails sync` at 25 ms, run when the edit is
done, is the better shape.

Not measured: `bench` (no k6 here), Neovim and the IDEs on either layout
(none installed), `jails new --gradle` fetching a wrapper jar, the `model
relocate` migration S60.7 names (it does not exist yet), and a merge on
the relocated layout (it needs I71.45).

---

## Appendix A — how the numbers were made

Counts of the surface come from `jails commands --json` (rows, kinds,
capabilities, options) and `wc -l` over `--help`; the jargon census from
every `<row> --help` and `grep -rhoE '"[^"]{20,}"' src crates
--include=*.rs`; the `fix:` audit from `grep -rhoE 'fix: [^"]{5,}'`. Lock
sizes are `wc -c .jails/compiler.lock.json` against `find .jails/generated
-type f | xargs cat | wc -c`; diff sizes are `git diff --numstat`; file
reads are `strace -c` and `-e trace=execve`; the phase table is `jails
--debug`. The scale projects are `for i in $(seq 1 100); do jails g
scaffold Thing$i id:uuid@pk name:string! count:long; done`.

The walks, in the order they were run, with `J=target/release/jails`:

```
# one entity, reads and errors
$J new demo --offline --no-git; $J g scaffold Note id:uuid@pk title:string! body:string?   # twice
$J model explain; $J model check; $J model check --frozen; $J model plan; $J routes; $J beans; $J stats; $J doctor; $J about
$J g scaffold note id:uuid@pk; $J g scafold Note; $J g record Foo a:strng; $J g record Foo a:string@idx
$J g scaffold Note id:uuid@pk title:string; $J g query OpenNotes title:string --on Missing
$J g usecase CreateNote --on Note; $J add db --pretend                       # accepted, then refused
$J destroy scaffold Note; $J resource status Note; $J rename resource Note Memo --strategy preserve-table --pretend
$J set server.port=8081; $J --pretend --output json g record M a:long | wc -l; $J --pretend --diff g record M3 a:long | wc -l
$J model fmt --check; $J model eject art_ent_note_repository_memory --pretend
# storage
$J add db --no-start; $J g scaffold Task id:uuid@pk name:string! done:boolean createdAt:instant --index 'done, created_at desc'
$J resource field add Task priority:int; $J add api; $J remove db --force
$J start db; $J migrate; $J migrate --check; $J test --scope integration; $J stop
# the real toolchain
mvn -q -B test-compile; $J test NoteTest; $J test --fast NoteTest; $J test
$J run   # curl POST /notes until 201; GET a missing id; POST a blank title
$J run --watch   # touch the application class; time to the second "Started"
$J testd NoteTest; $J test --engine warm --compile none <fqcn>; the same for a hand-written NoteHandTest, under a short path
$J test --affected --explain-selection   # after a change under .jails/generated, then under src/
# wide
$J new gr --gradle --offline; $J new online1; $J add db api actuator security cors json testkit docker ci k8s observability cache --no-start
$J new crawler --offline --app examples/web-crawler/.jails/app.toml --no-start; $J app apply --no-start
$J add kafka --no-start; $J g event OrderPlaced id:uuid total:long --on Order; $J start kafka; $J kafka topics; $J kafka describe; $J kafka lag
$J editor handshake|symbols routes|diagnostics --output json; $J lint; $J why run.log; $J contract emit; $J console (piped); $J db -- -c '\dt'
# the model by hand
cat >> .jails/model.jdl <<'JDL' entity Tag { use scaffold  id: uuid @pk  name: string @notBlank @unique  command Rename(id, name) { route POST "/tags/{id}/rename" } } JDL
$J model check; $J sync; $J routes; # then @uniq, a missing brace, `strin`, `#` and `//` comments, an aligned entity, the spec's §4 example
# git
git init; commit; $J g scaffold; git diff --numstat; two branches each scaffolding one entity; git merge; rm .jails/compiler.lock.json; $J sync
echo '.jails/compiler.lock.json -diff' > .gitattributes; git clone; $J model check --frozen
# .jails
rm -rf .jails target; mvn test-compile                                        # 0 in 2 s, empty application
cp -r .jails/generated/main/java/. src/main/java/ (and test, resources, requests); strip the build-helper block; rm -rf .jails; mvn test   # green in 8 s
```
