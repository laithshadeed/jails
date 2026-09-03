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
| ~~5~~ | ~~I70.22~~, ~~I71.3~~, ~~I71.4~~, ~~I71.5~~ | ~~the lock is 1× the tree, a scaffold is a 20-line diff, a mutation at a hundred entities is under 100 ms~~ |
| ~~6~~ | ~~I71.40, I71.41, I71.24~~ | ~~every scanner sees every source root; `test --affected` never selects nothing and passes~~ |
| ~~7~~ | ~~I70.2~~ | ~~one JSON encoding, carrying the same value as the human report~~ |
| 8 | ~~I70.8~~, I70.9 (one line over), ~~I71.18~~ | a one-screen `--help`, global flags printed once, `g <kind> --help` about the kind |
| ~~9~~ | ~~I71.29~~, ~~I71.26~~, ~~I71.28~~ | ~~README, the specification and the binary agree~~ |
| ~~10~~ | ~~I71.14~~ | ~~every mutation prints the JDL it wrote~~ |

**Worth a prototype before a decision:** an LSP for the model (I71.19), an
MCP server (I71.20), the manifest folded into the model (I71.21). Two came
off this list by being built rather than prototyped, for the same reason:
every part of each already existed. Bare `jails` as status (I71.17) is
facts other commands compute, and `jails undo` (I71.15) is the last plan
read backwards. `sync --watch` was prototyped and declined; §9 says why.

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

**I71.44 — `.jails/` is the input, and only the input.** *Landed with
I70.22.* It holds `model.jdl` and `app.toml` -- the input -- the lock and
the merge base it names -- what the last plan accepted -- and two scratch
entries a `.gitignore` keeps out of every commit. Deleting it leaves a
project that builds and passes its tests, and the next `jails g` refuses by
name rather than seeding a second model over the generated tree. The lock's
*shape* was the last of it: it is a manifest of path, kind, mode,
provenance and digest now, with the bytes as files under `.jails/base`.

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
*The interim landed*, and it is most of the win: the projection's bytes go
into the lock as the text they are rather than as four characters per byte.
Measured on a thirty-entity project, debug binary: the lock was 445.9 kB
against a 25.0 kB tree (**17.8×**) and is now 123.2 kB (**4.9×**), of which
the projection is 39.4 kB (**1.58×** the tree) and the accepted model 44.9 kB
-- the model is now the larger half. Capture reads 152 kB where it read
475 kB, so a mutation at thirty entities went from about 30 ms to about
16 ms as a side effect.

The digest rule did not change with the encoding: `projection_digest` is
still a digest of the form `serde` derives, and the reader recomputes exactly
that from what it decoded, which is what lets a lock written by the previous
release verify and be rewritten in the new shape by the next mutation. The
schema is `jails.compiler-lock.v4` so an older client refuses rather than
finding no `bytes` and inferring an empty merge base.

*And the tree layout landed too*, which is the whole of the item. `jails.
compiler-lock.v5` carries a `base` manifest -- one row per managed path with
its kind, mode, provenance and digest -- and the bytes are files under
`.jails/base`, published by an ordinary `PublishMergedTree` through the one
executor. Capture reads them back, rebuilds the projection and recomputes
`projection_digest`, so the rule that has held since v2 is unchanged: the
digest is of the form `serde` derives from the whole tree, whatever shape
the file kept it in.

Measured on the crawler -- 99 managed files, a 249 kB source tree:

| | before | after |
|---|---|---|
| lock | 344 kB (1.38x the tree) | 152 kB (**0.61x**) |
| base on disk | inline | 241 kB, byte-identical to the tree |
| **git objects** | one opaque blob per commit | **zero new blobs** |

The last row is the one that matters and it is measured, not argued: a
fresh `git commit` of the whole project reports 222 files and **122 blobs**,
because every base file is byte-identical to the managed file it is the base
for and git stores one object for both. The whole repository packs to 113
KiB. `.gitattributes` marks `base/**` as `-diff` -- a diff of it is the diff
of those files twice -- while leaving the merge alone, because a per-file
conflict in the merge base is exactly the conflict worth seeing (I71.45).

Three decisions worth the words. The base is published only when it is
stale *or absent*, and absence is read from the snapshot rather than from
the trees: a v4 lock decodes to exactly this projection, so equality alone
would upgrade the schema and leave no base beside it. The report says
nothing about it -- a line per base file doubles every report, and a
summary line was tried and withdrawn because it has no path, and every
other entry in that list is one in the JSON as much as on the screen. And
four scanners had to learn that `.jails/` is jails' own record rather than
anybody's source: the external-type index (where a stale copy let `destroy
enum Status` succeed while a field still named it), `jails src`, and two
test walkers.


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
with `.jails/compiler.lock.json -diff merge=binary`, and README documenting the
merge resolution.

*Landed, with a different resolution.* The executor writes
`.jails/.gitattributes` beside the ignore file, from inside, so an adopted
repository gets it too and a root `.gitattributes` -- which is the
reader's -- is left alone. The documented resolution is *not* `rm
.jails/compiler.lock.json && jails sync`: with no lock there is no merge
base, so every managed path reads as reader-owned and the next command
refuses (I71.8). Keeping either side and running `jails sync` is the
resolution, and it loses nothing but the other side's record of what it had
generated, because either projection is a real ancestor of both trees.

**I71.8 — a lost merge base is said out loud.** *Landed, and the premise
was wrong.* jails does not treat the managed tree as accepted when the
lock is gone: it refuses, on the first managed path it renders, because
without BASE that path reads as reader-owned -- and it told the reader to
move their own generated code. The capture cannot tell a deleted lock from
a file the reader wrote (a project that has never generated and one whose
lock is gone are the same capture), so the refusal now names both repairs
rather than guessing, and `doctor` carries a `merge base` row for the
condition before a mutation runs into it.

**I71.15 — `jails undo`.** *Landed.* Every applied operation carries the
image it found beside the image it wrote -- that is what makes a plan
reviewable -- so the inverse is that plan read backwards.
`jails_workspace::invert` swaps the two images of every operation, turns a
created file into a removal and an appended migration into one, publishes
the tree the applied one replaced, and takes the applied plan's
after-images as the inverse's preconditions. Nothing is recompiled and
there is no reverse renderer: `g scaffold X` then `jails undo` leaves
`git status --short` empty, which is the item's own measurement.

The executor is what makes it safe. A managed file edited since, a second
command in between, a `git checkout` -- each is a stale precondition and
the whole thing refuses with nothing written. The refusal carries its own
fix rather than the executor's: "run the command again" is right for a
mutation, which would replan, and wrong for `undo`, which has one plan and
cannot make another.

One command deep, deliberately. The bundle is kept at
`.jails/run/last-plan.json`, which the state `.gitignore` keeps out of
every commit, and it is forgotten once reversed -- so a second `undo` says
there is nothing to undo rather than refusing on a stale precondition and
leaving the reader to work out which of the two it meant. `git` is what
reverses more than one.

Two things it does not do. It does not re-run follow-up effects: `add db`
started a container, and removing the files does not stop it. And it is
hidden from the first screen, like every command outside I70.8's twenty
words, while `jails commands` lists it.


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
*Landed.* `jdl::v1::spans` turns the CST's declaration and member spans
into one table from the path the linker writes to the line it was written
on, and `Linker::problem` resolves it for all 145 diagnostic sites at
once -- longest prefix first, so `$.entities.loan.fields.status.type`
lands on the field's own line without a table of which suffixes a linker
may append. `Diagnostic` carries `line` and `column`; the human renderer
adds `at .jails/model.jdl:17:3` as its own line, so every message gate,
golden and reader that knows the first line is unmoved. Syntax
diagnostics, which computed the two numbers already and put them in
prose, now carry them as fields too.

The cascade is the other half. A field whose type is misspelled does not
link, so an index on it reported that the column does not name a field --
false, and it sends the reader to delete a correct line. `Linker` records
the fields that did not link and the five sites that would report a
consequence stay quiet: index columns, constraint columns, an
operation's field references (through the entity id), the type-dependent
half of `@notBlank`, `@positive`, `@scope`, `@version` and the length
bounds. The fixture typo went from four diagnostics to one, and fixing it
leaves the model valid -- nothing was hiding behind the noise.

**I71.48 — the editor protocol carries the language's diagnostics.**
*Landed*, on I71.11's spans. `editor diagnostics` runs the same
`parse_jdl` that `model check` runs and reports each diagnostic in the
schema's shape: the code an adapter branches on, the model path as
`subject`, the fix as `fixes`, and a zero-based `primary` range from the
diagnostic's own line and column. An adapter no longer has to shell out
to `model check` and scrape prose that a reword would break. A buffer
that is not `.jails/model.jdl` reports none of them and does not pay for
a link to find that out; a diagnostic with no location -- a collision
between two declarations, which is about neither line alone -- carries a
null `primary` rather than pointing at the top of the file.

**I71.26 — collections exist.** *Landed.* `TypeRef` has `List` and `Map`
variants, parsed by `TypeRef::parse` -- the element is parsed rather than
pattern-matched, so a nested collection, an optional element and a
non-string, non-enum map key are each refused with their own sentence
instead of the "unknown field type" a reader used to get. `g record Bag
tags:list<string> counts:map<Colour,int>` renders `List<String>` and
`Map<Colour, Integer>` with the boxed type arguments a generic needs, the
`java.util` imports, and the `List.copyOf` / `Map.copyOf` defensive copy
§9.2 promises; an optional collection is `Optional<List<T>>` with the
usual `requireNonNullElse`. A stored entity is refused by
`model-stored-collection`, which names the *column* that cannot exist
rather than the type that cannot be parsed, and `emit_sql` carries the
same refusal as a backstop.

Two things fell out of it. The durable job refuses to enqueue a
collection payload rather than sampling one, because a queue row that is
not the payload the caller sent loses the difference silently. And `g
factory` did not import a declared component type at all -- `private
Colour colour = null;` in `testkit` with no import, a generated file that
does not compile -- which collections made visible and which is fixed
here for every element type, bare or collected. The spring toolbox
compiles the record and the factory under real javac, which is the only
tier that could have caught either.


**I71.27 — the reserved-word check is about columns, on stored entities.**
*Landed.* `SqlName` carries the noun and the guard together, so a call site
cannot pass one and forget the other, and storage is read off the
*declarations* rather than the facets -- facets are filled in from the
projections after the entity loop, so a JDL entity saying `use repo` had an
empty facet set at the point the check runs.

**I71.28 — `--package` has one story.** *Landed.* The specification called
`--package` "the sole intentional refusal in v1 managed mode" while the
binary had shipped `@package(name)` as an entity header attribute all
along. §9.8 is now the story: what it moves (every projection the entity
owns, into one slice package, tests included), the three rules that make
it a placement rather than a second naming system, and the identity rule
that a move is not a remove and an add. §8.4's closed attribute matrix,
§3.1's convention boundary, §17.2's argument table, §18.2, §19.2 and §21's
conformance family were each carrying the refusal and now carry the
attribute.
`a_package_attribute_moves_an_entitys_projections_and_not_its_identity` is
the conformance test: the slice, the formatter round-trip, and the stable
ids unchanged across a hand edit from one package to another.

One binary fix fell out of writing it down. `--package ''` is the
documented way to say *straight into the base package*, and it rendered
`@package()`, which is a syntax error -- the one spelling the README told a
reader to use wrote a model the next command refused to read. The base
package is the quoted empty string, and the specification says so.

**I71.38 — `to`, not `--yields`, for a relation's parent.** *Landed, with
`--yields` still visible.* `--to` is one spelling of the same argument, and
it is the one `association`'s help, the kind list, the README and
`explain association` all use. Hiding `--yields` was declined: it is the
right word for `strategy` (*what a matching implementation produces*) and
`durable-job`, and clap hides an argument from every context or from none,
so hiding it there would take the correct word away from the two kinds that
own it. `explain association` now says when a typed field is the better
relation.

**I71.50 — the binary explains its own language.** *Landed as `jails model
jdl`*, beside the other commands that answer from the model rather than
under `explain`, whose argument is a generator kind or a capability. The
attribute count equals the parser's refusal list by construction: the
twelve inline lists at the `reject_unknown_attributes` call sites moved
into `jdl::v1::grammar`, which both halves read, and the `use` projections
did too. The field types come from `builtin::ALL` and the `cap` kinds from
`CapabilityKind::declarable_in_source`.


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
declare it with `jails g record Missing …`*. Against that: twelve messages point at
`[entities]`, `[capabilities]`, `[settings]` or `[dependencies]`, tables
of a format the tree refuses by name; Of 713 `fix:` lines,
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

The 192 was `--plan-out`'s help line echoed on 96 rows. One thing had three
names: in messages `entity` 213 times, `resource` 129, `scaffold` 18 --
`jails entity` is the command family now, with `resource` as a visible
alias. No report uses colour or `NO_COLOR`; a 23-line file list and a
two-line refusal look alike.

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


**I70.10 — `model explain` leads with what the reader declared.** *Landed,
without `--all`.* An entity name resolves through the model to the entity
and its fields, so `jails model explain Note` prints exactly those rows,
and every row a declaration owns sorts before the project's twenty-three
layer packages. The `--all` half is declined: "empty layer package" needs a
facet-to-layer map that lives in the compiler, and hiding a package row a
reader arrived with -- they saw it in a stack trace -- is worse than
printing it last.

**I70.11 — `new` says what it wrote and what to do next.** *Landed:* the
creation report counts the source files and then names every other file the
reader did not ask for -- `AGENTS.md`, `mise.toml`, `.jails/model.jdl`, the
lock, the ignore file -- read off the staged tree rather than a hand-written
list, plus one `next:` line. *Declined:* removing `.jails/apply.lock` at the
end. It is the executor's advisory lock, `.jails/.gitignore` names it, and a
lock file removed while a holder has it open is a lock the next opener
cannot see -- the trap `claim_fixture` documents in the harness. The report
leaves it out instead.


**I70.5 and I71.39 — every refusal is fact, fix, nothing written.**
*Landed.* The twelve `[entities]`-style messages name
`.jails/model.jdl` instead of a TOML table the model has not had since
JDL, and `every_fix_line_leads_with_something_the_reader_can_do` holds the
rest: a `fix:` leads with a backticked command or an imperative verb from a
closed list, extended deliberately the way `UNJOURNEYED` is. An open
pattern would have passed *the lock must be a real file*, which states a
fact and leaves the reader where they were; eleven such lines were
rewritten to lead with the action.

**I71.10 — a vocabulary budget, gated.** *Landed, with one exemption.* The
gate is `the_six_retired_words_appear_in_no_help_page_and_no_message`: it
reads every help page the binary can print and every production string
literal of twenty characters or more that contains a space, and fails on
any of the six. `projection` keeps the JDL sense in four named files,
because JDL v1 §11 calls a `use` declaration a projection and renaming it
in the diagnostic but not in the language would be the drift the gate
exists to stop.

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


**I70.9 — global flags appear once.** *Landed:* the eight global flags
carry one help line each, the rationale moved to `jails explain --flag
<name>`, and one line at the foot of `jails --help` says they are global
and where the reasons are. The screen is 40 lines: twenty commands, eight
flags, `--help`, `--version` and the footer. `--timing` (I71.5) was the
eighth flag and cost one line, which is why the number is 40 rather than
the 39 measured before it. *Remains:* a subcommand's help still lists all
seven, so `jails resource --help` is 21 lines against the item's 20; 15 of
those are its own. clap hides an argument from every help or from none, so
collapsing them to a summary line on subcommands needs either per-context
hiding upstream or a hand-written list at the root -- and a hand-written
list of the flags clap already knows is the second source this repository
spends its gates preventing. Declined at one line over, deliberately.


**I71.18 — `g <kind> --help` is about the kind.** *Landed.* `jails g <kind>
--help` is intercepted before clap and prints the kind's `explain` entry
followed by the flags that kind accepts. `jails g scaffold --help` is 20
lines and names `--timestamps`, `--index`, `--unique`, `--path` and
`--package`, against 233 lines for the shared page. The flags are read off
the tables the frontends already refuse by: the component registry's
`accepts` — extracted out of `reject_v1_options`, so the page and the
refusal are one table read twice — the entity profile, and the operation
lowering's own per-kind branch. `generator_help_is_about_the_kind_and_names_only_flags_that_exist`
walks every advertised kind, holds each page under 60 lines, and fails on a
flag no `jails g --help` declares.

*Deviation:* the operation kinds' rows are a list beside `OperationProfile`
rather than a predicate lifted out of the lowering. There is no single
refusal to lift: `operation.rs` gates `--via`, `--order-by` and `--limit`
inline on `args.kind` at the point each is read, and hoisting them into one
predicate would move behaviour to shorten a help page. The spelling gate is
what holds that list honest.

**I71.23 — the closed sets complete on the command line.** *Landed.*
`editor complete` answered from the clap tree alone, so a completer could
offer `--on` and then nothing after it. `editor_complete` adds the half
that is not the same in every project: the entities `--on`, `--via` and
`--yields` can name, the components a field list can filter on, the type
table after a colon and the `@markers` after that. `jails completion bash`
now emits a hook that asks the binary and falls through to the static
script when the answer is empty, and
`the_generated_bash_completion_asks_the_binary_what_this_project_declares`
sources it in a real bash and reads `COMPREPLY`: `jails g query Recent
st<TAB>` completes `status:` from the entity.

Two decisions worth the words. Only `g`/`generate` reaches the binary,
because that is where the model has something to say and a completer that
spawns a process on every TAB is one people turn off. And every path is
best-effort and silent -- no model, a model mid-edit that does not parse,
a `--on` naming an entity that does not exist yet -- because the reader is
in the middle of typing the thing that would fix it.

*Deviation:* the hook is bash's alone. zsh and fish keep the static script
they had; a hook written for a shell nobody has run it in would be worse
than none.

**I71.17 — bare `jails` is `status`.** *Landed, minus the one line that
would have cost eight times the budget.* Bare `jails` inside a project
prints five lines: the platform, build and Java release; the model's
counts and its path; whether the lock has accepted that model; the
managed file count with edited and missing beside it; and which
capabilities declare a compose service. Measured on the crawler -- 99
managed files, 11 capabilities -- it is **10 ms**, against the item's 20.
Outside a project it is the usage clap printed before, on stderr, exit 2,
so a script that tested for either still sees it. `--output json` carries
the same facts as `jails.status.v1`.

Every fact is one another command already computes, from one model parse
and one capture; the classifier the managed counts come from is
`model_ownership`'s own, so the summary and `jails model status`'s list
cannot disagree about the same tree.

*Deviation:* the item asked for services declared *and not running*, and
the running half is not there. `docker info` is 165 ms on this machine
and must never be cached -- a remembered engine reports as up ten minutes
after it stopped -- so one line would have been eight times the whole
budget. The line says what is declared and which command runs it;
`jails doctor` is where the machine gets asked.


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
- **The compiler declares what its linter forbids.** After one scaffold,
  `jails lint` reports `pom.xml:58: spring-boot-starter-web; use
  spring-boot-starter-webmvc`: `new` writes `webmvc`, the scaffold's
  dependency reconciliation adds `web`.
- **A stack trace for a long path.** Under a 115-byte project path the
  test daemon dies in `ServerSocketChannel.bind` (a unix socket path is
  limited to 108 bytes) with *fix: inspect the daemon diagnostic above*;
  the same project under `/tmp/jc` starts the daemon in 1.8 s and answers
  in about 100 ms after that.
- **README's testd numbers** (0.06 to 0.10 s per method) do not say they
  exclude every generated test.
- What is right: property collisions and hand-edit removals refuse well
  (§4); `g scaffold Empty` refuses with *needs exactly one `@pk` field …
  for example `id:uuid@pk`*; a required field added to a stored entity
  refuses until it has a backfill; ejection followed by a model change
  still compiles.

### Change

**I70.16 — a read never writes.** *Landed.* `jails test` installs nothing.
`fast-test` is an ordinary capability with an ordinary `jails add`, and the
refusal is at the point the warm engine is about to run rather than at the
flag -- which is what lets `--fast` keep falling back to the build tool
when nothing is compiled, since that run reaches no launcher.

**I70.17 — one command is one plan.** *Landed.* `--index` at creation is
rendered into the entity declaration, so one compile writes the `create
table` and its `create index` into one migration. `entity index add` is
unchanged and still the command for a table that already exists. A
side-effect worth having: `--index` on a project with no database now
refuses before anything is written, where it used to write the entity and
refuse afterwards.

**I70.18 — an error surfaces on the command that caused it.** *Landed, as
the narrow half.* The linker refuses a command that carries no input, no
parameters and no assignments while its entity has a required field with no
default -- `model-command-constructs-nothing`, at the declaration rather
than at the first `add db`. Deliberately narrow: a command that carries
*some* fields stays the insert emitter's to judge field by field, with the
resolution in hand, so the linker cannot refuse something the compiler
would have accepted.

**I71.12 — the compiler and the linter agree.** *Landed.* The servlet
starter's name is decided from the captured Boot major in one place beside
the `spring_starter` call, so a Boot 4 project gets
`spring-boot-starter-webmvc` and everything below it keeps
`spring-boot-starter-web`. Twenty goldens lost the dependency entirely
rather than changing it: the fixture already declares `webmvc` outside the
marked block, so the reconciler now sees the requirement met and adds
nothing -- which is the duplicate the linter was reporting.

**I71.34 — the daemon refuses a socket path it cannot bind.** *Landed,
without the `$XDG_RUNTIME_DIR` half.* `refuse_an_unbindable_socket` runs in
`Client::for_project`, before any JVM, and names both numbers and the path.
The runtime-directory fallback is declined for now: the socket lives beside
`testd.meta` and `testd.java` under `.jails/run`, and moving one of the
three somewhere keyed by a digest makes "where is my daemon" two answers
instead of one. The fix line names the two commands that need no socket.

**I71.31 — one path style.** *Landed.* `naming::route_segment` is the one
place the hyphen is applied, to the same plural the table uses, and
`DerivedRole::HttpPath` puts the answer in `jails model explain` -- so a
project that pinned a path with `use scaffold(path: …)` reads as pinned
rather than as a convention that moved. No golden changed: every golden
entity is one word, which is why the underscore survived this long.


**I71.3 — the executor's work is proportional to the change.** *Declined as
specified, and the cost measured.* `execute` is 34 ms of a 138 ms
mutation at a hundred entities, and it is spent re-reading and re-hashing
every locked path -- which is the race guard, not waste: the preconditions
are rechecked against the tree *as it is now* precisely because another run
or an editor may have changed it since the capture. Verifying by `stat`
would answer that question wrongly, because two files of equal length are
not equal, and a wrong answer here is a wrong merge rather than a slow one.
The pipeline's other phases came down instead: see I70.23 and I70.25.

**I71.4 — the bundle carries what changed.** *Part landed; the rest would
cost portability.* Adding one record to a hundred-entity project wrote a
64 MB, six-million-line plan file, because every blob went out as a
pretty-printed array of integers -- one line a byte. Blobs are text now
(`jails_contracts::bytes_field::map`): 7.6 MB and 24,735 lines, and both
halves of the encoding could move because nothing digests the bundle's own
JSON.

*Declined:* "blobs only for entries not already in the lock, the lock named
as the bundle's base". A plan is portable by content -- `--plan-in` applies
one exported somewhere else, and `imported_plan_refuses_another_root`
proves it refuses when the target does not match. A bundle that referred to
a lock would be portable only to a project that already has that lock,
which is most of the way to not being portable at all.

**I70.23 — capture reads what the plan needs.** *Part landed, and the
premise measured.* Reading and hashing all 1,421 files of a hundred-entity
project is 22 ms; the capture was 122 ms, and 95 ms of it was the lock's
text being rewritten into an array of `serde_json::Number` so the type
could decode it. The fields read either shape now
(`jails_contracts::bytes_field`), and `sha256` hashes the input where it is
instead of copying it first -- capture is 64 ms.

*Declined:* "stat each locked path and read only what moved". Two files of
equal length are not equal, so a stat cannot answer whether a managed file
changed, and a capture that guesses wrong produces a wrong merge rather
than a slow one. What remains after this is `materialize` at 116 ms, which
is the projection serialised as fourteen megabytes of JSON to compute the
digest the lock rule fixes -- that belongs with I71.4.

**I70.25 — "nothing to do" is decided before writing.** *Answered
differently, and the difference matters.* Deciding before *capture* would
make the answer a claim about the model rather than about the project: a
managed file deleted or a `pom.xml` edited by hand changes what the plan
does, and a run that skipped the capture would report "nothing to do" over
a tree that no longer matches. The decision is made where it is still true
-- the lock encoder is a pure function of the accepted model, projection,
compiler and migrations, so when all four are what the file was written
from it is not re-encoded. A repeat mutation at a hundred entities went
from 619 ms of measured phases to about 132 ms, and materialize from 116 ms
to 30 ms.

**I71.1 — `model explain` reads the model, not the tree.** *Half landed.*
`model explain` needed exactly one external fact -- `jails.toml`'s layer
renames -- and captured the whole workspace to get it: 1,421 files read to
learn one table's worth of package names. It asks `capture::facts` for the
layout instead, which is the same reader the capture boundary uses, and
costs 9 ms at a hundred entities against 149 ms.

*Remains:* `entity status` is 122 ms and genuinely needs live bytes -- it
answers *edited* against *missing* per managed file, and the migration
history it reads is a capture-boundary rule rather than a file list. Doing
it without a capture means re-implementing both, which is a second reader
of two things that have one; it belongs with I70.23 rather than beside it.

**I71.2 — a manifest replay is one capture.** *Landed, by the other
route.* The item proposed linking every row into one edited source; what
landed keeps a row's own pipeline for a row that changes something and
skips it for a row that does not. A row whose declaration is already in
the model produces the source it was handed, and discovering that cost a
capture, a compile and a materialize each -- on the crawler's twelve
rows, 255 ms of finding twelve times over that there was nothing to do.

`Invocation::defer_unchanged` is the flag and only a replay sets it. What
makes the skip sound is the closing pass `app apply` now runs: an
ordinary mutation over the finished source, with nothing to change about
the model, that captures once and writes whatever the tree is missing. So
a file a skipped row would have repaired is repaired there, and the
convergence property the manifest is for is unchanged.

**Measured on the crawler, release binary: 255 ms to 30 ms**, against the
20 ms one `model plan` costs -- the difference is twelve model parses,
which is what a row that skips still pays. A fresh apply is unchanged
except for the closing pass, which finds nothing to do because the rows
did it. `a_converged_replay_captures_once` asserts the shape rather than
the milliseconds: `--timing` prints one `capture` line per pipeline, and a
converged replay has exactly one.

**I70.24 — `doctor` warm.** *Half landed, and the other half declined.*
The probes run concurrently -- the version probes among themselves, and
the three process-starting groups against each other -- which takes a
warm `doctor` on this machine from 225 ms to about 163 ms. The 100 ms
target is not reachable: `docker info` alone is 165 ms, so it is the
floor, and caching it is the one thing that must not happen. A remembered
`docker info` reports an engine that stopped ten minutes ago, which is the
fact the row exists to check.

**I71.22 — the second `jails run` skips Maven (prototype).** *Change* on a
tree whose classes are newer than its sources, `java -cp` the main class
directly (`launcher.rs` already decides staleness and knows the
classpath), saying so in one line, and fall back to Maven loudly. *Done
when* the second `jails run` on an unchanged tree answers in under 6 s.

---

## 8. The documents against the binary

| the document says | the binary does | where |
|---|---|---|
| ~~README: `jails history`, `jails show <transaction>`, `jails undo <transaction>`~~ | ~~none of the three exists~~ | *Closed.* `history` and `show` left the README with the receipts system; `undo` exists (I71.15) |
| ~~README, `jails g --help`, spec §9: collections are `list<T>` and `map<K,V>`~~ | ~~*unknown field type*, on the CLI and in a hand-written model~~ | *Closed.* Collections exist (I71.26) |
| ~~spec §2, §17: `--package` is *the sole intentional refusal*~~ | ~~`--package util` writes `@package(util)`~~ | *Closed.* §9.8 documents the attribute (I71.28) |
| README `jails.toml`: `[project] capabilities` is what `sync` applies | `sync` reads the model; `new` writes no `jails.toml` | README line 1108 |
| twelve messages: *declared under `[entities]`* and kin | the file has no such tables | `grep -rn 'declared under' src crates` |
| README's testd numbers | no generated test is eligible (§1) | README line 936 |
| README: *every unknown widens* `--affected` | a change under `.jails/generated` selects nothing (§1) | README line 954 |

`every_command_a_message_tells_the_reader_to_run_is_one_that_exists`
scans messages against `jails commands`; nothing scans README or the
specification.

**I71.29 — README is scanned like a message.** *Landed.*
`every_command_a_message_tells_the_reader_to_run_is_one_that_exists` reads
the README's backticked commands too, expanding a `generate|g` heading into
both spellings so the alias is part of the promise. `history`, `show` and
`undo` left the file: they documented the receipts system deleted with the
codec. Two entries are exempt by name -- "there is no `jails dev`" and the
`g action` row under *Not yet* -- because they are the README saying a
command does not exist, and a gate that could not tell them apart would
push towards deleting the sentence rather than the gap.

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
