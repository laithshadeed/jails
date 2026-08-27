# Simplifying `jails` — a second opinion

> **Author**: GLM, working independently of `simplify-gemini.md` (read afterwards for comparison).
> **Method**: doc-comment sweep of all 299 Rust files; full or deep reads of the load-bearing files in every
> crate (`route.rs`, `pipeline.rs`, `execute.rs`, `recipes.rs`, `envelope.rs`, `merge.rs`, `query_compiler.rs`,
> `dispatch.rs`, `request.rs`, `board.rs`, …); line-count and public-type census; inspection of a real project's
> `.jails/` directory (`my-minicom`). Where I could not read a file completely, I say so by leaning on counts
> rather than quotes. Numbers below are measured, not estimated — see the appendix for how.

---

## 1. The verdict

You are right that the architecture doesn't make sense — but not in the direction you might expect.

**The code is not badly written. It is badly *aimed*.** Almost every file is disciplined, commented, and
defended by tests. The problem is what the code collectively *is*: `jails` has spent roughly **45–50k of its
122k lines of Rust building a database engine** — a WAL, a content-addressed object store, an
optimistic-concurrency protocol, crash recovery, dual schema generations, a canonical request wire format, an
editor protocol, and portable transactions — as the substrate underneath what is actually a
**deterministic template projector**.

That determinism is the key fact, and the codebase already knows it. `pipeline.rs` step 1 *replays every
`DesiredChange` onto a fresh projection and asserts the result equals the `LedgerIntent`* — that is a purity
proof, enforced at runtime, on every run. A generator that is provably re-derivable from
`(manifest, project facts)` does not need a journal, objects, receipts, publish-inodes, generation counters,
or an eleven-step commit. It needs:

```
target_state = render(manifest, facts)     # pure
--pretend    = print(diff(disk, target_state))
apply        = stage to temp dir, rename into place
recovery     = run apply again
history      = git log
undo         = git checkout / re-render minus the row
```

Everything else is ceremony defending against a crash whose honest repair is *"re-run the command"* — which is
what `terraform`, `dotnet new`, and every lockfile-driven tool already assume. `git merge-file`, `git init`,
and `git diff` are already inside this binary; the journal's job is already done by something you already
depend on.

The three levers, in order of size:

1. **Shrink the durable contract** — manifest-as-truth, disk-as-store, git-as-journal. Deletes ~40k LOC.
2. **Buy parsers** — the codebase already adopted `sqlparser` and shells to `git merge-file`; the same move
   on TOML/YAML/XML/Java deletes another ~8–10k LOC of hand-rolled parsers.
3. **Finish the recipes-as-data journey** the codebase is visibly already on (`One table per recipe`,
   `Share the join rule between the recipes that need it`) — every generator becomes a declaration row plus
   templates, and the per-kind Rust plumbing folds into one engine.

I do *not* think the win comes from inventing a DSL grammar — §5 (P8) explains why. That is where my analysis
parted ways with the Gemini document. The resulting design is stated end to end in **§4** — the new vision:
jails as a project compiler, manifest in, project tree out, reconcile as the only verb — and §5 walks there
one deletable step at a time.

---

## 2. What the 122k lines actually are

Measured today: **122,265 LOC of Rust across 299 files**, plus 30,587 LOC of integration tests, 156 template
files (mostly Java), 61 golden trees, and 1,133 `#[test]` functions.

| crate | files | LOC | what it actually is |
|---|---|---|---|
| `jails-generate` | 56 | 25,339 | **the product**: recipes, field→SQL, Spring slices, capabilities |
| `jails-protocol` | 50 | 22,779 | transaction vocabulary: 285 pub types, most suffixed `V1`/`V2` |
| `jails-project` | 26 | 14,308 | hand-rolled parsers (pom, gradle, compose, toml, properties) + project facts |
| `jails-drive` | 26 | 10,084 | testd, run, affected, console, kafka — the toolchain |
| `jails-prepare` | 23 | 10,992 | the 14-step pipeline, 3-way merge, command-result envelopes |
| `jails-engine` | 34 | 9,502 | routing + the V2 route + recipe plumbing |
| `src/` | 32 | 9,431 | clap surface (167 commands), app manifest, editor protocol, history |
| `jails-report` | 14 | 6,503 | doctor / why / explain / routes / beans |
| `jails-commit` | 11 | 5,264 | the 11-step executor, journal, objects, receipts, GC |
| `jails-support` | 8 | 3,538 | hand-rolled binary codec (946 LOC), process, locks |
| `jails-java` | 8 | 2,541 | `blanked()` Java reader, classfile pool, identifier surgery |
| `jails-spec` | 7 | 1,691 | field-spec DSL, closed kind/capability vocabularies |
| `jails-state` | 3 | 271 | compat classifier |

**The split that matters**: knowledge (generate + spec + templates + drive + report + the good half of
project/java) ≈ 55k. Everything serving the durable store and the wire formats ≈ **45–50k** — protocol,
prepare, commit, state, `support/codec.rs`, the ledger half of engine, and much of `src/`.

The substrate in the wild: `my-minicom/.jails/` is **4.1 MB** for a demo service — a 94 KB `ledger.toml`
whose payload is a 47,269-byte binary blob hex-encoded onto one line (the file explicitly refuses to be read
by anything but `jails doctor --output json`), 122 content-addressed objects, 19 receipts, a transactions
directory, and two flock files. The manifest a human could have read is in there somewhere, encrypted by
discipline.

The CLI surface: **167 rows in `jails commands --json`, 39 `ArtifactKind`s, ~30 capabilities**. Each kind is
multiplied by a variant matrix — Maven/Gradle, Boot 2.7→4.x, JUnit 5/6 sniffing, db/no-db, Spring/plain,
`--package`, `--on/--yields/--via/--select/--if-match/--method/--consumes/--pins` — and every cell of that
matrix is resolved by conditional Rust, not by data.

---

## 3. Four complexity engines (where the code went and why)

### 3.1 The database you didn't need (~45k LOC)

What exists: `JournalV1`/`ReceiptV1` journals, an `objects/sha256/**` blob store, transaction directories,
`.publish` staging inodes, hard-link publication, an 11-step commit protocol with a designated commit
instant, failpoint-injected crash tests, generation counters, read-sets and declared read captures, guarded
preimages, `FrozenDesiredInput`/`DesiredInputGuard`, effect-retry plans, `PendingConflict` freezing, GC, and
a canonical request fingerprint (`CanonicalRequestSyntaxV1` + `domain_hash`) so a *reconciliation can tell
that the command that fixed it is the same command that started it*.

That last sentence should be a red flag to its author. Every property in that list is a distributed-systems
property. None of the threats exist for a single-user, single-machine, single-writer codegen tool — except
one, and git is already installed:

| Guarantee the engine provides | What actually provides it |
|---|---|
| A file never half-written | write-temp-then-rename (one function, already exists in spirit in `apply/`) |
| Multi-file atomicity | **git** (stage + single commit), or per-file renames + deterministic re-run |
| Crash recovery | deterministic re-derivation of the same target state (the codebase already asserts this property) |
| Concurrent writers | one `flock` (keep it — it's ~50 lines) |
| `--pretend` parity | print the diff; there is no other execution mode left |
| Reader conflicts on generated files | `git merge-file` — already called today from `merge.rs` |
| `jails show` / `history` / `undo` | `git log` / `git checkout`, plus a ~200-line append-only JSONL receipt file if you want receipts without git |
| Ledger integrity | it's derived from the manifest now; a corrupt cache is regenerated, not recovered |

The irony is documented in your own comments: `merge.rs` refuses to implement a merge algorithm because
"`git merge-file` is already on the machine of anyone running a code generator." The same argument, applied
one layer out, retires the entire journal.

**What genuinely has to survive**: the capture-once principle (read the project once per command — that is
just `capture.rs` minus the ceremony), the refusal vocabulary (it's excellent), the three-way merge, the
flock, and `--pretend` parity. That's maybe 2k LOC, not 45k.

### 3.2 The parsers you wrote by hand (~10k LOC, and the correctness risk)

One dependency principle — "only clap" — has already been broken the right way, twice: `sqlparser` for
migrations (`query_compiler.rs` is genuinely good, and *prefers a real parser over lexical resemblance*) and
`git merge-file` for merges. The precedent is established; it just hasn't been applied to the formats the
tool edits most:

| Hand-rolled | LOC | The trap it already sprung |
|---|---|---|
| `pom.rs` XML | 1,377 | version sniffing families; "a missing pom silently changes generated Java" |
| `gradle.rs` Groovy | 1,530 | newest, largest reader; the "answer exactly or refuse" bar is hardest in Groovy |
| `config.rs` TOML | 1,347 | closed key set to avoid silent typo-swallowing — a parser gives you that for free |
| `compose.rs` YAML | 792 | `Marked::indented` exists because a marker at column 0 in YAML is a parse error |
| `codec.rs` binary | 946 | a private encoding nobody can inspect |
| `java.rs` + `identifier.rs` + `annotate.rs` + `dispatch.rs` | ~1,700 | `blanked()` tricks; the `@SpringBootTest`-in-Javadoc bug; three separate walks of `src/test/java`; textual whole-word rename |
| `classfile.rs` | 300 | `CONSTANT_Long` takes two slots — "produces a plausible wrong answer rather than an error" |
| JSON, hex, SHA-256, properties, JUnit XML | ~1,500 | reimplementations of standard library work |

The pattern to adopt already has a name inside the repo: *borrow the real parser; keep the splicer*.

- **TOML**: `toml_edit` exists precisely for "edit a user-owned TOML leaving every comment byte-for-byte
  alone." It deletes `config.rs`'s hand parser *and* the risk it manages.
- **Java**: `tree-sitter-java` for reads (or `com.sun.source` via the resident JVM — P7). Real ASTs kill the
  Javadoc-vs-annotation ambiguity class *permanently*, not one bug at a time. Keep byte-splices for writes,
  but verify by re-parse.
- **XML**: `quick-xml`/`roxmltree` + a span-preserving writer; `reports.rs` (JUnit XML) should just be serde.
- **YAML**: keep the marked-block splicer (it's genuinely good design), but parse for validation instead of
  shape-guessing.

Note the interesting asymmetry: the project adopted `sqlparser` (a heavyweight) for a *read-mostly* concern
while hand-rolling TOML, YAML, XML and Java for *write-critical* paths. The risk is backwards.

### 3.3 The generations that never shipped

`jails` has never been released, yet it carries museum-grade compatibility machinery: `LedgerV2` alongside a
schema-1 store "until R1.5 step 6 retires it"; `CommandEnvelope` and `CommandEnvelopeV2`; `serialize.rs` and
`serialize/v2.rs`; `Output::JsonV1` and `Output::Json`; a `compatibility.rs` of identifiers and superseded
codecs; `strategy_on`/`strategy_yields` kept as deprecated aliases "because they shipped in a user-facing
file format"; dozens of `*V1` type names (my census found `TestReportV1`, `ResourceLifecycleV1`,
`OperationSemanticsV1`, `FlywayHistoryV1`, … — 282 public types in `jails-protocol` alone); and `ledger-v10.toml`
protocol goldens for formats with no external consumer.

Every `*V1` suffix on a type that has never had a `V2` consumer is a tax paid on behalf of a future that
hasn't arrived. Version suffixes earn their place the day a second format actually ships — and the ledger
÷ envelope split (§3.1) removes the one place a second format was imminent. The rule going forward should
be: **no versioned wire format until there are two of them.**

### 3.4 The variant matrix (the part that can't be deleted, only flattened)

39 kinds × {Maven, Gradle} × {Boot 2.7 … 4.x} × {JUnit 5, 6} × {db, no db} × {Spring, plain} × `--package`.
The codebase fights this honestly — version facts are read, never assumed; refusals name types; golden tests
pin every cell — but the matrix is *represented as control flow*: `mockmvc_autoconfigure_import()`,
`webmvc_test_import()`, `validation_package()`, `junit.rs`, `validation_dependency()`, `pom::TARGET_RELEASE`
negotiations, `require_jakarta_spring`…

Two honest answers:

- **Recipes as data (do it).** `generate/recipes.rs` is already "one match from a kind to the files it would
  write, and nothing else." Finish the journey: each kind becomes a row — `(templates[], path rules, context
  builder, requires)` — and the *facts* layer (one place, typed) resolves every version question. Kind logic
  that reads a record off disk or refuses a precondition stays code, exactly as `recipes.rs`' own header says;
  the test is whether the knowledge is *declared* or *buried in plumbing*.
- **One version floor** (a product decision, flagged in §9): one Boot floor, one JUnit, deletes the sniffing
  family almost entirely. This is a README change, not a refactor — which is why it's the cheapest lever per
  LOC deleted. The counterweight is real: the daily Gradle project. Keep Maven+Gradle; consider one Boot
  floor.

---

## 4. The new vision: jails as a project compiler

*(This is the chapter the rest of the document serves: §5 is how to get there, §7 is the resulting crate map,
§8 is the order to walk it.)*

The user's instinct — "maybe write a compiler, invent a DSL, dynamic schema" — is pointing at something real,
but the *valuable* property of a compiler isn't its phases or its IR. It's two properties:

1. **Determinism**: same inputs → same bytes.
2. **Referential transparency**: the output can be thrown away and re-derived at any time.

The codebase already believes in (1) — `pipeline.rs` replays and asserts. The new design follows both to
their conclusion and lets them define the whole system:

### 4.1 The system in one diagram

```
              ┌────────────────────────────────────────────┐
              │   jails.toml  —  the truth (human-owned)   │
              │  project · layout · capabilities · entities│
              └──────────────────┬─────────────────────────┘
                                 │  edit (toml_edit, one row)
                                 ▼
  facts = observe(project)  ┌─────────┐   resolve:
  (build tool, deps,        │ resolve │   which recipe rows,
   versions, disk) ────────▶│         │   which variant cells
  ┌───────────────────┐     └────┬────┘   apply to this project
  │ recipes           │◀─────────┘
  │ (39 kind rows +   │           ▼
  │  templates, SQL/  │     target tree ──▶ diff(disk) ──▶ staged renames
  │  DDL projection)  │     (never stored)  (the plan)      │
  └───────────────────┘                                     ▼
                                             receipts.jsonl ◀── write
```

`--pretend` prints the diff instead of writing. That is the entire runtime model. The rest of this
section walks it: what the five nouns are (§4.2), what happened to the 25-step pipeline (§4.3), what every
today's command becomes (§4.4), and a worked example in both worlds (§4.5).

### 4.2 The five nouns

Everything jails keeps is one of these five. Everything in the current system is either one of these, or
machinery serving them:

| noun | what it is | where it lives | today's counterpart |
|---|---|---|---|
| **Manifest** | what the user asked for: package, layers, capabilities, and one row per entity | `jails.toml` — human-readable, human-owned, `toml_edit`-edited so comments survive | today's `jails.toml` + `.jails/app.toml` + the ledger's ownership registry, merged into **one file** |
| **Facts** | what the project *is right now*: build tool, deps and versions, layer renames, disk state of every file the command could touch | in-memory only, observed once per command | `capture.rs`'s capture-once principle survives; the 985-line `fact.rs` ceremony collapses into one struct |
| **Recipes** | how each artifact is produced: 39 kind rows + the 156 templates + field→SQL | `jails-core`, as data (P3) | today's per-kind Rust plumbing, already half-consolidated in `recipes.rs` |
| **Target tree** | the complete set of files the manifest implies | computed, **never stored** | today's `LedgerIntent` + objects + publish inodes, i.e. a tree stored in triplicate |
| **Receipts** | append-only JSONL, one line per apply: what ran, what changed, content hashes | `.jails/receipts.jsonl`, ~200 LOC | today's `JournalV1` + `ReceiptV1` + objects + transactions + gc + recovery ≈ 5,264 LOC |

One verb — **reconcile** — and one flag — `--pretend` — cover everything the executor does today.

The manifest, concretely (existing vocabulary throughout — kinds, layout keys, field-spec syntax, `--on`/
`--yields`/`--select` all already exist; they move into the file):

```toml
# jails.toml — the only durable registry jails keeps
[project]
package     = "com.example.demo"
java        = 26
capabilities = ["db", "api", "testkit"]

[layout]                          # unchanged — `jails adopt` already writes this
domain = "domain"
web    = "adapters.web"

[[entity]]
kind    = "scaffold"
name    = "Note"
fields  = ["id:uuid", "title:string!", "done:boolean"]
mig     = "V3"                     # the one state a render can't re-derive: burned migration serials

[[entity]]
kind    = "usecase"
name    = "ResolveNote"
fields  = ["resolution:string!"]
on      = "Note"
yields  = "NoteResolved"

[[entity]]
kind    = "query"
name    = "OpenNotes"
on      = "Note"
select  = "status = 'OPEN' ORDER BY created_at DESC"
```

Read that file and you know the whole project — what `ledger.toml`'s 47 KB of hex claims and what
`jails resource status` reconstructs, in a form a human (and every editor) can already read. The `mig`
column is deliberately the only piece of state a render cannot re-derive from disk, because Flyway has no
undo; everything else in the file is what the user *asked for*, and ownership is *implied by the row* — not
recorded separately from it.

### 4.3 The pipeline: five stages replace 25

Today's runtime is a 14-step prepare (`pipeline.rs`'s replay, render, diff, preimage guards, identity
freeze), then an 11-step commit (object writes, journal states, `.publish` inodes, hard-link publication,
ledger-last), plus capture read-sets, generation counters, canonical request fingerprints, and a GC. The new
runtime is the diagram's right half, in full:

1. **observe** — read the project once, as values (the capture-once principle survives; it is `capture.rs`
   minus the ceremony). Every declared read records present-or-absent: an absence is a fact the plan can
   check, not a gap.
2. **resolve** — manifest rows ∩ facts: which recipes this command realizes, and which variant cells apply
   (Boot floor, JUnit line, adapter shape, layer placement). One typed `Facts` value; version questions are
   answered here and nowhere else — the sniffing family (`mockmvc_autoconfigure_import`, `webmvc_test_import`,
   `validation_package`…) becomes data in this one place.
3. **render** — pure: `(rows, facts) → Vec<OutputFile>`, provenance header stamped. Determinism is a tested
   property of this function (today it is an assertion buried in prepare step 1).
4. **diff** — target tree vs disk: per file, create / replace / leave-alone; reader-moved files go to
   `git merge-file` (already the codebase's answer); rows the manifest no longer names become deletions.
   This is the only "plan" that exists — and `--pretend` prints it.
5. **write** — stage to temp dir, `rename` into place, append one receipt line, bring compose up if needed.
   Interrupted? The next run of the same command re-derives the same target and finishes. `crash.rs` spends
   390 lines proving what here is an axiom.

Under this model the executor's guarantees come from the filesystem, not from protocol (§3.1's table, one
row each). `--pretend` parity is free because there is no second mode to drift from. `crash recovery` is
"run it again." And the 47 KB binary ledger stops existing: ownership is the manifest, file state is the
disk, content hashes live in the receipt line that wrote them.

### 4.4 What every existing command becomes

The CLI surface doesn't shrink in *words* — it shrinks in *mechanism*. Same UX, one engine:

| today | new world | what folds |
|---|---|---|
| `g scaffold Note id:uuid title:string!` | append one `[[entity]]` row → reconcile | sugar stays for humans |
| `g field`, `resource field add/change/drop/rename/nullability` (1,945 LOC, six verbs, twelve entry points) | edit the row's `fields` → reconcile | one verb on one row; companions re-derive in render |
| `add db`, `add kafka`, … | add the capability key → reconcile | unchanged UX; `sync` *is* reconcile |
| `destroy <kind> <name>` | delete the row → reconcile | no `--force` semantics needed: the row is the truth |
| `rename resource` | edit the name in the manifest → reconcile | a rename is a render under a different name |
| `app plan` / `app apply` | the plan step / the write step | `app apply` stops being special — every command is apply |
| `adopt layout`, `set/unset`, `add dependency` | manifest edits → reconcile | same file, same verb |
| `show` / `history` / `undo` | `git log` / `git checkout`; `receipts.jsonl` for non-git projects | ~1,600 LOC of receipt machinery → 200 |
| `doctor`, `why`, `explain`, `routes`, `beans`, `stats` | **unchanged** | they get *better*: one source of truth to read |
| `test`, `testd`, `run`, `migrate`, `kafka`, `console`, `bench` | unchanged | the toolchain crate is already right |

### 4.5 A worked example, both worlds

`jails g scaffold Note id:uuid title:string!` —

**Old world** (traced through the code): clap → `dispatch.rs::mutate` → `route/artifact.rs` assembles a
`CanonicalGenerateRequest`, fingerprinted via `CanonicalRequestSyntaxV1::fingerprint()` so a mid-merge
reconciliation can later ask "is this the same command?" → recipe becomes desired changes and ownership
claims → `capture::projected` takes a read-set capture → `pipeline.rs`'s 14 steps (replay changes onto a
fresh projection and assert equality with `LedgerIntent`, materialize deferred renders, diff against the
snapshot, guard preimages, freeze identity) → flock → `execute.rs`'s 11 steps (write content objects to
`.jails/objects/sha256/**`, journal Prepared → Active, copy into transaction-local `.publish` inodes,
verify, sync, hard-link into place, journal → Committed, write the 47 KB binary hex ledger last) → receipt
→ report. 60 golden trees pin the ledger's bytes, so any protocol change reddens the e2e suite.

**New world**: clap → `toml_edit` appends one `[[entity]]` row (comments untouched) → render emits the same
~10 files — record, port, JDBC adapter, in-memory adapter, DTOs, service, controller, companion tests,
migration, `requests/Note.http` — *byte-identical to today's goldens* (which is why §10's oracle split comes
first) → diff → 10 staged renames (or `git merge-file` where the reader edited) → one receipt line appended.
`--pretend` is stages 1–4 printed.

What vanishes from the path: fingerprinting, read-sets, guarded preimages, identity freezing, the object
store, publish inodes, hard-link publication, journal states, generation counters, the 47 KB hex ledger,
and the 60-golden coupling that turned an unrelated feature into a repo-wide red. What the user keeps:
the same commands, the same bytes on disk, the same `--pretend` parity — plus durable state they can read.

### 4.6 What this vision is *not*, and what you gain besides smaller

- **It is not a grammar, not an IR, not minijinja** (§5 P8): the render function keeps today's templates,
  today's field-spec DSL, and typed Rust context builders. The vision changes what jails *is* — a compiler
  from a file to a tree — not what it *generates*. Every opinion that makes jails jails (immutable records,
  ports/adapters, one-column-list SQL, visible SQL, honest status codes, tests that prove recipes) lives in
  render, and render is untouched.
- **You gain features the old substrate couldn't afford**: user-owned custom scaffolds (a `[[entity]]` row
  plus project-local templates — no core change), `plan --from <manifest-diff>` as the portable story
  (`route/portable.rs` simulates it with authenticated envelopes today), and a manifest that editors and
  agents can read and write without jails as intermediary — because the truth stopped being hex.
- **The remaining protocol is small enough to be honest about**: `Name`, `Package`, `EntityId`, the closed
  kind/capability vocabularies, `FieldSpec` (one parser, spoken by CLI and manifest alike — ending the
  three-spelling drift in §4's final paragraph), and an unversioned receipt schema. That is §7's ~2k-line
  `jails-protocol`.

## 5. The proposals: eight steps toward §4

Each proposal below is one deletable step toward the §4 design; the vision is the destination, these are the
stepping stones.

### P1 — Manifest is the truth; the ledger becomes a derived cache *(the hinge)*

`.jails/app.toml` already exists and already expresses capabilities and generic intents; `jails.toml`
expresses layout; the ledger separately records resource ownership. **Merge them into one project schema
file.** Every mutating command — `g scaffold`, `g field`, `add db`, `rename resource`, `destroy` — becomes:
parse argv → edit the manifest (a `toml_edit` one-liner) → reconcile.

What it deletes: `route/request.rs` (870), most of `intent/request.rs` (1,479 — the canonical request
fingerprint exists to answer "is this the same command?" across a crash, and under re-run semantics the
question vanishes), `durable/lifecycle.rs`, `observe/resource_status.rs`, `ownership.rs`'s reconciliation
scopes, `generated_files.rs` (the ledger-derived file list — it becomes "read the manifest"), and the
`Recorded` unpacking ritual in `route.rs`.

What it buys that's *new*: user-owned custom scaffolds fall out for free (a manifest row + template in the
project's `templates/` dir), and `jails plan --from <manifest-diff>` becomes the export/import story that
`route/portable.rs` is currently simulating with authenticated envelopes.

### P2 — Replace the WAL with staging + rename + re-run

Keep: one flock, temp-dir staging (you already have `ScratchDir`), per-file `rename` publication, the
completion marker, recovery-as-reconcile-on-next-run. That's a ~300-line module. Delete: `journal.rs`
(1,012), `execute.rs` down from 1,161 to ~150, `store.rs` (699), `recover.rs` (532), `gc.rs`, `fault.rs`
(failpoints), the object store, `envelope.rs` (887), and the `durable/` half of protocol. Keep the crash
tests — re-anchored on "interrupt between renames, then re-run reaches the target state" — they're the proof
the simple design is *more* correct, not less.

Budget honestly: this deletes the majority of three crates and ~15k test LOC. It is the phase that changes
behavior most, so it lands behind the existing CLI, command by command, exactly like the V2 route migration
you already did — except this time the migration is *down*.

### P3 — Finish recipes-as-data

`recipes.rs`'s header says it: option F ("each kind a descriptor") is "the right eventual shape for the kinds
that are pure data." Draw that line kind by kind. Target: `generate/` becomes a table + ~20 context-builder
functions + `write.rs` (keep — it's the good kind of chokepoint). This is also where `--package`, placement,
`package-info`, import normalization, and `ensure_failsafe/ensure_assertj/ensure_webmvc_test` already live;
one engine, no per-kind forgetting.

### P4 — Buy parsers, keep splicers (the `sqlparser` precedent)

As §3.2. Sequenced *after* P1/P2 because it's independent and additive. Biggest single correctness win:
Java via `tree-sitter-java` (or via the resident JVM — P7), because it retires the whole
`blanked()`-vs-Javadoc bug family rather than the current bug.

### P5 — Delete the unshipped generations *(do this first; it's pure subtraction)*

V1 formats, `Output::JsonV1`, `serialize/v2.rs`'s twin, compatibility identifiers, deprecated aliases,
`route/portable.rs` (authenticated export/import of prepared transactions — for an exporter that doesn't
exist), `editor_command.rs`'s protocol vocabulary, `DesiredInputGuard`/`FrozenDesiredInput`, effect-retry
plans, migration seals. ~6–10k LOC. No behavior change. The rule this installs: **a protocol consumer must
exist before a protocol does** — the editor integration can ship on the two `--json` outputs that already
exist (`doctor --json`, `commands --json`) and grow when an actual editor asks.

### P6 — Consolidate 13 crates → 5

The crate layering solved a real problem (one 12-module cycle from three symbols), but the cure overshot:
13 facade blocks, 13 Cargo.tomls, 13 test binaries, `template_here!` wrappers, and LAYERS doing the real
enforcement anyway. Target:

```
jails            (bin)     — clap, dispatch, new/new-cli, app manifest
jails-core       — spec + field DSL + project facts + java + recipes + render (the knowledge)
jails-apply      — capture, diff, merge, staged write, manifest write-back
jails-protocol   — small: names, kinds, kinds' closed vocabularies, field spec identity
jails-tooling    — testd, run, affected, doctor, why, explain, console, kafka (merge drive+report)
```

The architecture gates (`LAYERS`, the ratchets) already enforce module edges compiler-independently — keep
them, and let them police the new, larger modules. Do this *last* (or never); it's churn without behavior.

### P7 — The crazy idea worth actually testing: move Java-awareness into the JVM

`jails` already ships a resident JVM (`testd`) and already shells out to tools (`git merge-file`, `spotless`,
`mvn`). A second single-file Java helper — same pattern as `JailsTestDaemon.java` — using **`com.sun.source`**
(javac's public parser API, no new dependency, the compiler you invited) can do every Java edit jail currently
does by string surgery: splice imports, add/remove annotations, register dispatchers, rename identifiers,
report annotated types, read components. The Rust CLI stays; `java.rs`, `identifier.rs`, `annotate.rs`,
`jails-java/dispatch.rs`, and most textual inspection die; the whole "plausible wrong answer" class
(`CONSTANT_Long` slot pairs, Javadoc-walk false positives, three walks of `src/test/java`) is deleted rather
than fixed. Cost: JVM required for those commands (they effectively already are), two languages, a
subprocess hop. I'd prototype this on `splice_import` + `register_command` — the two highest-churn, lowest-
glamour splices — before committing either way.

### P8 — Where I'd not go (and why)

- **A bespoke JDL grammar (pest/winnow).** Seductive, and the Gemini doc's centerpiece. But it re-creates
  the exact tax the codebase just paid by hand: a parser, an error-reporting surface, an editor integration
  problem, a closed vocabulary — for a syntax your users must learn. TOML + serde + `toml_edit` gives you the
  declarative model with a toolchain people already have. If manifests ever outgrow TOML, grow the *manifest*,
  not a second language.
- **Minijinja everywhere.** The templates are fine — I checked: `resource_service_java.java` is 46 lines with
  11 flat `{{placeholders}}`. The complexity was never in the strings; it's in the parameter derivation, and
  that must stay typed Rust. Conditionals inside template strings would *move* logic out of the compiler's
  reach. (Your own template.rs docs already state this rule; I'm agreeing with it against the Gemini doc.)
- **A jails runtime jar.** Explicitly a non-goal in the spec, and correctly so: generated code that a reader
  must be able to read and own is the point. Generated-code duplication is fine; the *tool* is the thing that
  must not duplicate.
- **Dynamic schema à la liquid** — same answer as JDL: the field spec is already the dynamic schema. One
  parser, spoken everywhere, is the win; more expressiveness is not.

---

## 6. What to keep (the parts that are already right)

Being fair matters; this list is why the codebase is worth simplifying rather than rewriting:

- **The template file system** — `{{name}}` substitution with panic-on-missing, `include_str!`, templates as
  real Java an editor can check. Keep exactly as is.
- **The field-spec DSL** and its single parser in `jails-spec` — it *is* the dynamic schema.
- **`sql.rs`'s one-column-list discipline** — DDL, insert, select, bind, and row mapper from one list is the
  single best idea in the repo. Everything else should be this shape.
- **`doctor`/`why`/`explain`** as hand-written tables with oracle tests (`commands --json` walked from clap;
  the `fix:`-line oracle). Cheap, honest, and they get better as the tool shrinks.
- **`tests/architecture/` ratchets and the `LAYERS` table** — they enforce every proposal above; the ceilings
  are the demolition crew's contract.
- **Golden + agreement tests** — the real moat. Keep; re-anchor on the new executor.
- **`testd`, `affected`, `run`, `reports.rs`** — 10k LOC of genuinely valuable, mostly-clean toolchain code.
  Touching it only to extract `classfile.rs`-level correctness wins (§3.2).
- **`process.rs`, `scratch.rs`, `lock.rs`, `hermetic.rs`** — small, sharp, correct.
- **The doc comments.** Genuinely excellent. The audit would have been half as useful without them.

---

## 7. Target shape and the math

```
jails (bin)          ~4k    clap surface (167→~60 rows), dispatch, new/new-cli, app manifest
jails-core           ~7k    field DSL, kinds, recipes-as-data, template engine, sql/ddl, facts (pom/gradle/compose/java readers)
jails-apply          ~2.5k  capture → diff → staged atomic write → git-merge on conflict → manifest write-back
jails-protocol       ~2k    validating newtypes, closed sets, receipt schema (one, unversioned)
jails-tooling        ~12k   testd, affected, run/watch, doctor, why, explain, consoles (mostly unchanged)
templates/           as-is  156 files, plus recipe rows declaring them
tests/               ~20k   golden + agreement + ratchets + real-toolchain (rebalanced during Phases 1–2)
```

**≈ 33k production LOC, from 122k — a −73% codebase, roughly 5 crates, and the entire remaining LOC is
either Java knowledge or tooling** — the two things the tool is actually for. The mechanical subtractive
budget:

| Phase | What | Deleted (net) |
|---|---|---|
| 0 | unshipped generations (P5) | ~8k |
| 1 | manifest-as-truth (P1) | ~12k |
| 2 | WAL → staging (P2) | ~15k |
| 3 | recipes-as-data (P3) | ~6k |
| 4 | real parsers (P4) | ~6k |
| 5 | crate consolidation (P6) | ~5k of ceremony |
| — | **total** | **~50–52k, plus the target-shape rewrites ≈ −89k net** |

## 8. Sequencing, risk, and how to stay honest

0. **Phase -1: get the harness refactor-proof first — see §10.** The golden e2e suite is currently red over
   ledger bytes alone (§10.2); until the product contract and the durable-state contract are separated, the
   refactoring runs with a blind oracle. Phase -1 is ~1–2 days and changes no product code.
1. **Phase 0 (P5) is pure deletion** — no behavior change, some test re-anchoring. Do it in one weekend;
   it proves the appetite.
2. **Phase 1 (P1) is the hinge.** Once ownership lives in the manifest, the WAL's reason to exist is gone —
   so do it *before* attempting P2, and land it command-by-command behind the existing CLI (the V2-route
   playbook, in reverse).
3. **Phase 2 (P2)**: port the crash-test suite to the new executor *first* — "interrupt between renames,
   re-run, assert convergence" — then delete the journal. If any of those tests can't be expressed in the
   new model, that's a finding about the new model, not an excuse to keep the old one.
4. **P3/P4 are additive** and can interleave with the daily Gradle work at any point after Phase 0.
5. Every phase lands only when: `cargo test --workspace` green, `JAILS_REQUIRE_TOOLCHAIN=1 cargo test --workspace`
   green (no silent skips — a skipped tier-3 test reports as passing), and the §10.3 characterization report
   diffs clean. Keep a running branch note in `pending.md` (§-numbered as always) per phase, with the LOC delta
   the architecture board prints — the ratchets will catch any ceiling that should have fallen with the code.

Risks worth stating plainly:

- **Undo/history without git**: projects jails creates are git repos by default (`new` runs `git init`);
  `affected` and `merge-file` already require git. For non-git projects, keep a ~200-line JSONL receipt log.
  Nobody who runs a codegen CLI on a non-git directory expects rollback; say so in `doctor`.
- **Real parsers can round-trip badly.** That's why P4 is "adopt parser for *reads* and *verification*; keep
  span-preserving splicers for edits" — with `toml_edit` as the exception that proves you can have both.
- **The daily Gradle project must not regress** — P4 makes it *better* (real Groovy understanding is
  currently the tool's weakest read), and P1/P2 don't touch it.
- **The golden suite will shrink temporarily during Phases 1–2.** Land each phase with re-anchored goldens in
  the same change, per your existing rule that a ratchet may only move with the reason recorded beside it.

## 9. Open questions for the maintainer

1. Is `jails history/show/undo` (receipt-driven) earning its keep against `git log`? If git is acceptable,
   receipts shrink to an audit JSONL; if not, that's the one durable artifact worth keeping — and it's ~200
   lines, not 5k.
2. One Boot floor (e.g. ≥3.5) would delete the version-sniffing family (~2–3k LOC) and several template
   variants. It's a README-level product decision with the largest LOC-per-effort ratio after the ledger.
   The Gradle project keeps its configured release either way.
3. Does the editor protocol have a consumer yet? If not, P5 deletes it; if yes, it can ride on the existing
   `--json` outputs until it asks for more.
4. Are you attached to the 14-step/11-step algorithms as *the* design? My honest read after reading them:
   they're beautifully built and defending against a crash whose correct response is "run the command again."
   The algorithms are the complexity; the properties they protect are cheap once generation is pure.

---

*An audit like this is one reader's pass over a large tree — check every "delete" against your own
knowledge of what its tests pin before executing. The architecture ratchets in `tests/architecture/` are the
right tool for the demolition: lower a ceiling, delete the code, let the gate hold the line.*

---

## 10. The safety net: what today's tests catch, what they can't, and Phase -1

*(Added after the maintainer asked: "what about test coverage / e2e — I don't want to start refactoring and
break the code.")*

### 10.1 What the suite actually is (measured)

412 `#[test]` functions live in `tests/`, on top of ~1,133 unit tests inside the crates. Four layers, by
how close each sits to what a user experiences:

| layer | where | what it proves | coupled to the machinery? |
|---|---|---|---|
| **Binary e2e** | `tests/cli/*` (~310 fns: generate 108, capabilities 52, tooling 42, reports 34, new 18, app 15, sql 11, developer_tools 11, …) + `tests/golden.rs` (3 fns over 61 trees) + `tests/agreement.rs` (4) | drives the **compiled binary** (`CARGO_BIN_EXE_jails`) against real fixture projects; checks files on disk, exit codes, JSON envelopes | low — it asserts outcomes, and survives every phase if its subject survives |
| **Byte oracle** | `tests/golden.rs` | "every generated byte identical before and after", deliberately end-to-end through ~20 invocations, `UPDATE_GOLDEN=1` + read-the-diff workflow. The right design — **but it also snapshots `ledger.toml` raw** | high: re-fires on any ledger encoding change |
| **Machinery e2e** | `tests/engine.rs` (66 fns, 194 imports of engine/prepare/protocol), `tests/desired.rs`, `tests/protocol-golden/`, `tests/cli/portable_plan.rs`, `tests/cli/editor_protocol.rs`, `tests/editor.rs`, `tests/cli/history.rs`, `crates/jails-commit/tests/crash.rs` | drives the transaction protocol directly and asserts on store contents | **tests the thing P1/P2 delete — must be replaced, not preserved** |
| **Real-toolchain (tier 3)** | gated in 8 files (`generate`, `capabilities`, `tooling`, `new`, `app`, `examples`, `developer_tools`, `sql`) via `real_java_supports_target_release` / `real_maven_cmd` / `real_path_without_mvnd` + `common::skip()`; ~68 touchpoints; `JAILS_REQUIRE_TOOLCHAIN=1` turns skips into failures | **the only tests that answer the question the tool exists for** — generated Java compiles and passes under real Maven | independent of P1–P4 — this is the net that must never re-anchor |

Plus the black-box `tests/cli/behavior_matrix.rs`, which asserts a checked-in behavior contract
(`docs/black-box-behavior.tsv`) through **fake build tools** — argv and exit-code contracts, not whatever
Maven is installed. That file's own doc names the exact risk this section is about: "without a baseline, an
intentional compatibility change and an accidental routing regression look identical."

### 10.2 The live finding: the e2e oracle is currently red — and over nothing you care about

At audit time, the working tree's golden suite fails **60 of 61 golden trees, 100% over `.jails/ledger.toml`
bytes, with zero generated Java/SQL/properties changed** (`agreement` is green; the one in-flight feature's
trees aside, no product output moved). The uncommitted usecase work changed the ledger's binary encoding, and
the byte oracle — which was built to make *template* refactors safe — now screams on *machinery* churn.

Scale that to the proposal: Phases 1–2 rewrite the durable format repeatedly. Under today's suite the e2e
layer would be red for the entire refactoring, which is worse than no oracle — it trains you to regenerate
without reading. Three consequences, all fixable in Phase -1:

1. **A refactoring must start from a green corpus.** Commit or set aside the in-flight usecase work first
   (`UPDATE_GOLDEN=1 cargo test --test golden`, then actually read `git diff tests/golden`).
2. **Split the byte oracle in two.** Keep generated files byte-for-byte (product contract). Replace the raw
   `ledger.toml` hex snapshot with a *decoded semantic view* — `jails resource status --json`
   (`lifecycle_status.rs` already reads the ledger) or a small `jails debug ledger --json`. Then P1/P2
   produce zero golden churn, and a red golden means "the product changed" — the property the suite's own
   doc comment promises.
3. **Build the characterization beam.** One script/test that runs a pinned binary over the full
   `SCENARIOS` corpus on fixture projects and dumps a JSON report: files created, exit codes, stdout/stderr
   contracts at behavior-matrix level, decoded resource status, `doctor --json`. Commit the report as the
   phase baseline; diff it before/after each phase. ~1 day of work; it is the one artifact that survives the
   demolition unchanged, because it asserts only user-visible behavior.

### 10.3 What must survive untouched, and what dies with what it tests

- **Never re-anchor (the keepers):** golden.rs *after* the split, agreement.rs, the `tests/cli/*` product
  assertions, all tier-3 real-toolchain tests, `tests/architecture/` ratchets, `behavior_matrix.rs`.
  If a phase changes any of these, the phase is wrong.
- **Dies with the machinery — budget for it, don't mourn it:** `tests/engine.rs`'s store assertions,
  `crash.rs` (390), `protocol-golden/`, `desired.rs`, `portable_plan.rs`, `editor_protocol.rs`,
  `tests/editor.rs`, `cli/history.rs` (~1,600 test LOC testing features P1/P2/P5 delete). They get replaced
  by the §10.2 beam, not deleted silently: for each, note what user-visible behavior it pinned, and confirm
  the beam covers it.
- **P2's crash tests are the spec, not the casualty:** before deleting the journal, port `crash.rs`'s
  failpoint matrix to the staging model — "interrupt between renames, re-run, assert convergence." A test
  the old executor satisfies and the new one doesn't is a finding about the new model.
- **Tier-3 honesty:** skipped tier-3 tests report as passing (the project's own doc says so). Run
  `JAILS_REQUIRE_TOOLCHAIN=1 cargo test --workspace` before declaring any phase done; the ~68 gated
  touchpoints in those 8 files are the only witnesses for "the generated project actually works."

One structural fact worth keeping from all of this: **the suite's center of gravity is already right** —
310+ binary e2e tests and 61 byte-level trees over the product, with the machinery tests in the minority.
Phase -1 is not about building a net from scratch; it's about un-coupling the net's loudest wire from the
thing being demolished.

---

## Appendix — how the numbers were measured

- **LOC and file counts**: `find crates src -name '*.rs' | xargs wc -l`, summed per crate. Test functions
counted via `#[cfg(test)]` and `#[test]` greps across `crates/`, `src/` and `tests/`. Template count:
`find templates -type f`. Golden trees: directories under `tests/golden/`.
- **The 45–50k substrate split** is a judgment call, stated so you can argue with it: all of
`jails-protocol` (22,779) + `jails-prepare` (10,992) + `jails-commit` (5,264) + `jails-state` (271) +
`support/codec.rs` (946) ≈ 40k, plus roughly the ledger/request halves of `jails-engine` and `src/`
(`route/request.rs`, `route/history.rs`, `route/portable.rs`, `history_command.rs`, `plan_command.rs`) ≈ 5–10k.
The knowledge side (generate + spec + templates + drive + report) is counted the same way from below.
- **`.jails/` census**: `du -sh my-minicom/.jails`; the ledger envelope read directly
(`payload_len = 47269`, line 5 is 94,554 bytes — the hex payload); `ls objects/sha256 | wc -l` = 122;
`ls receipts | wc -l` = 19.
- **CLI surface**: `jails commands | wc -l` = 167; `ArtifactKind` counted from `spec/kind.rs` (39);
capabilities counted from the `cap-*` golden trees plus the remaining `add` targets.
- **Test census**: `#[test]` greps per file in `tests/` (412 total); tier-3 population from greps of
`real_java_supports_target_release` / `real_maven_cmd` / `real_path_without_mvnd` across the 8 gated files.
Suite state measured by actually running: `cargo test --test architecture --test architecture_allowances`
(17 passed), `cargo test --test agreement` (4 passed), and `cargo test --test golden` — which failed
60 of 61 trees over `ledger.toml` bytes alone at audit time (§10.2), reproduced twice to confirm the
failure signature (no non-ledger file in any failure list).
- **Files read in full or deeply** (beyond the doc sweep): `engine/route.rs`, `prepare/pipeline.rs`,
`commit/execute.rs`, `generate/recipes.rs`, `protocol/lib.rs`, `protocol/durable/envelope.rs`,
`protocol/intent/request.rs`, `project/query_compiler.rs`, `prepare/merge.rs`, `dispatch.rs`,
`architecture/board.rs` (partial), plus the README command surface and the simplify-gemini.md comparison.
Everything else was covered by the doc-comment sweep, structural greps, and LOC analysis — which is why
critical claims in this document quote code or count types rather than paraphrase.