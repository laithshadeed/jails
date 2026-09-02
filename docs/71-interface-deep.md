<!--
The second interface walk. `docs/70-interface.md` measured the surface a
reader meets in the first hour; this file goes below it (where a mutation
spends its time), beside it (Gradle, Kafka, twelve capabilities at once,
the example manifests, the editor protocol, hand-edited JDL, a running
application) and past it (the ideas that would change what the tool *is*
to a reader, each checked against the code before it was written down).

**A closed item is deleted from this file**, in the commit that closes it.
Item numbers `I71.n` are stable and never reused. Every number carries the
command that produced it; re-measure before quoting one.
-->

# 71 — The interface, second walk: deep, wide, wild

**Read `docs/70-interface.md` first**; its five bars (clear, obvious, no
magic, no surprises, milliseconds) are the ones every item here answers
to, and nothing in §1 of that file is repeated. Same setup: 2026-09-02,
`target/release/jails`, Linux, 4 cores, scratch projects from `jails new
<name> --offline --no-git`, wall time from a shell wrapper around `date
+%s%N`.

## 1. Deep: where a mutation spends its time

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
in `docs/70-interface.md` §1.2 is under 200 lines.

**I71.5 -- the stopwatch is a first-class flag.** `--timing` (or
`JAILS_PROFILE=1`) prints the four phases plus what each read (files,
bytes) on every mutation, separately from `--debug`'s command echo. It is
the instrument I70.23 asked for; it exists; it needs a name a reader can
find. **Exit:** `jails --timing g record X a:int` prints the table above
for that run.

## 2. Wide: what the other surfaces said

### 2.1 The whole application, and running it

`jails new crawler --offline --no-git --app examples/web-crawler/.jails/app.toml
--no-start`: **2,171 ms** from nothing to 123 files (99 managed), 18
routes, 6 migrations, a 93-line model, a 3.7 MB lock -- and 887 lines of
output, one `applied model patch … sha256:…` per manifest row. `jails run`
on the one-entity project, JDK 26 through mise: **first `201 Created`
24.6 s after the command** (Maven build then Boot start); `GET /notes/{id}`
for a missing id is 404, a blank title is a 400 problem detail. `jails
test NoteTest` is 6.5 s and `jails test` 6.1 s, all JVM.

### 2.2 Gradle

`jails new gr --gradle --offline`: 13 ms, and the wrapper jar is honestly
absent with the fix line. `g scaffold` 35 ms, `add db --no-start` 49 ms
with the marked blocks in `build.gradle`; `add format` refuses by name in
14 ms, as CLAUDE.md says it must. `jails test --pretend` ran Gradle for
48 s (§2.5).

### 2.3 Twelve capabilities in one command

`jails add db api actuator security cors json testkit docker ci k8s
observability cache --no-start`: **69 ms**, 43 files, `Dockerfile`,
`compose.yaml`, `deploy/`, `.github/workflows/{ci,image}.yml`, 46 lines in
`application.properties`, a 1.08 MB lock, and one `applied` line naming
all twelve. `add kafka --no-start` after it: 102 ms. Nothing here is slow;
the report is the only thing a reader has to scroll.

### 2.4 The JDL-first workflow

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

### 2.5 What `--pretend` does on a command that only runs

`jails test --pretend NoteTest` runs Maven for 7.3 s on the Maven project
and Gradle for 48 s on the Gradle one, and `jails check --pretend` runs the
build. The flag is global, accepted everywhere, and means nothing on a
command that writes nothing.

### 2.6 Repair, conflict, loss

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

### 2.7 Two small self-contradictions

`jails lint` on a project `jails new --offline` just wrote reports
`pom.xml:58: spring-boot-starter-web; use spring-boot-starter-webmvc` --
the fixture writes the starter the linter forbids. `jails resource repair
Tag` refuses because repair takes no selector, on a command family whose
every other verb takes one.

### 2.8 The editor protocol and the plugin

`editor handshake` 6 ms, `symbols routes` 5 ms (22 ms at a hundred
entities), `diagnostics --scope project` 5 ms, `complete` 4 ms:
keystroke-fast. `jails.nvim/lua/jails/init.lua` is 926 lines carrying
preview and apply of prepared plans, a watch loop, diagnostics into the
quickfix list, pickers over routes, beans, tests and types, and a JDL
buffer configuration; Neovim itself is not installed here, so none of it
was driven.

### 2.9 The words a reader has to learn

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

### 2.10 Items from the wide walk

**I71.6 -- `--pretend` refuses where it means nothing.** On `test`,
`run`, `check`, `build`, `clean`, `mvn`, `gradle`, `console`, `bench`,
`migrate`, `kafka`, `db`: *`test` runs a JVM and writes nothing;
`--pretend` does not apply*, in 5 ms, before any JVM starts. **Exit:**
`jails test --pretend` returns in under 10 ms with that line.

**I71.7 -- one verb makes the tree match the model.** `sync` repairs a
deleted managed file the way `resource repair` does, saying so in its
report (`restore  <path>  deleted by hand`); `resource repair` becomes an
alias for one release. **Exit:** the deletion in §2.6 is healed by `jails
sync` and the report names the file.

**I71.8 -- a lost merge base is said out loud.** When no lock exists and
managed files do, the mutation prints one line -- *no compiler lock:
treating the managed tree as accepted; edits since the last generation
cannot be told from generated code until the next one* -- and `doctor`
carries the same row. **Exit:** the §2.6 run prints it.

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
message. **Exit:** the `strin` typo in §2.4 prints one diagnostic with a
line and column.

**I71.12 -- the fixture and the linter agree.** `jails new --offline`
writes `spring-boot-starter-webmvc`, or `lint` stops forbidding the other
on a project whose Boot version accepts it. **Exit:** `jails lint` on a
fresh offline project reports nothing.

**I71.13 -- the report for a manifest or a multi-capability `add` is one
report.** Fourteen `applied model patch … sha256:…` lines become one
summary with the file list grouped by row, and the digest moves to
`--output json`; §2.1's 887 lines become under 150. Depends on I71.2 for
the single plan and on I70.1 for the delta. **Exit:** `jails new --app`
prints one `applied` line.

## 3. Wild: what would change what jails *is* to a reader

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

**I71.16 -- JDL-first is the documented first path.** §2.4 measured it:
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
calls in TOML, replayed one pipeline each (§1, I71.2). Everything a
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

## 4. What this file does not propose

- A daemon or a resident process for the mutation path. §1 shows the
  pure compiler at 14 ms on a hundred entities; the milliseconds are in
  reading and writing the tree, and I71.3 and I71.4 get them back without
  a process to manage.
- Dropping the merge base. I71.3 and I71.4 change what is read and
  carried, not what is kept; `docs/00-contracts.md` §6.1 stands.
- Replacing JDL with the manifest, or the CLI with JDL. I71.21 removes a
  *third* source; I71.16 orders the two that remain.

## 5. The experiment log

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

## 6. Not measured

Neovim (not installed here), the Kafka tools against a broker, `testd`,
`run --watch`, `test --affected`, `jails db`, `jails console`, `bench`,
`migrate --check`, and `jails new` against start.spring.io.
