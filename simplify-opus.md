# simplify-opus.md — a target design for jails, and how to get there without breaking it

Written 2026-08-27 after reading the tree end to end: 326 `.rs` files, 13 crates,
156 templates, 61 golden trees. Every number is measured from this checkout.

**The thesis in one line: jails is already a compiler, but it is built as a
script — no intermediate representation, no resolver, no emitter. Give it those
three things and two thirds of the code stops having a reason to exist.**

§1 is the target system. §2 is the evidence. §3 is why the current shape keeps
producing lines. §4 is what the tests actually protect. §5–§8 are the route.

---

## 1. The vision: jails as a compiler

### 1.1 One sentence

> **A jails project is a declaration. jails is a deterministic compiler from
> that declaration to a Java project. Every command is either an edit to the
> declaration or a query about the compilation.**

Nothing else in this document is an idea; it is all consequences of that
sentence.

### 1.2 The source language

Today the source language exists but is *typed at a prompt and thrown away*:

```
jails g transition Approve id:uuid tenantId:uuid@scope status:TaskStatus version:long \
  --on Task --select id --if-match required --set senderType=USER --method PUT --path /x
```

That is a sentence in a DSL with ten modifiers, no comments, no diff, no way to
see two declarations side by side, and no record except a hex blob in
`.jails/ledger.toml`. Meanwhile `.jails/app.toml` is *the same language*, already
written down, already reconciled as one transition by `jails app apply`.

Promote it. The project's declaration is a file:

```toml
[project]
package      = "com.example.demo"
capabilities = ["db", "api", "security"]

[entity.Task]
fields  = { id = "uuid @pk", title = "string!", status = "TaskStatus",
            createdAt = "instant", version = "long" }
indexes = ["status, createdAt desc"]

[enum.TaskStatus]
values = ["OPEN", "DONE"]

[op.CreateTask]
kind    = "usecase"
on      = "Task"
accepts = ["title"]
route   = "POST /tasks"

[op.ApproveTask]
kind     = "transition"
on       = "Task"
select   = "id"
sets     = ["status"]
if_match = "required"
```

`jails g transition Approve … --on Task` no longer *writes Java*. It **appends
that block and recompiles** — exactly the relationship `cargo add` has with
`Cargo.toml`. The CLI keeps every argument it has today; it stops being the
place where meaning lives.

### 1.3 The compiler: five passes, five types

```
   jails.toml ───parse───▶  Ast        pure, no filesystem, round-trips
Ast + adopted ──resolve──▶  Schema     ← THE IR. one type checker, one error site
       Schema ───lower───▶  Ir         Vec<Unit> + Vec<Sql> + Vec<Claim>; total
           Ir ───emit────▶  Tree       BTreeMap<ProjectPath, Vec<u8>>
Tree + Ledger + Disk ──apply──▶  writes
```

Five functions. Each has a nameable type on both sides. That is the whole
difference from today, where `Recipe` goes straight to `Vec<Artifact{path,
String}>` through a 39-arm match and there is no value in between that means
"what this project *is*".

**`parse`** — TOML → `Ast`. serde derive plus the field-spec DSL. No project
needed, so its whole test suite is string-in/value-out. **~600 lines.**

**`resolve`** — the only place in jails that raises an error about *meaning*.
"`Task` has no component `stauts`." "`CreateTask` cannot infer `createdAt`."
"`@scope` needs `ScopeAuthorizer`; run `jails add security`." It produces
`Schema`, in which every reference is a resolved pointer, every type a resolved
`JavaType`, every unsupplied component an `Expr`. **~1,500 lines**, and it
replaces validation that is currently written out separately inside
`usecase_files`, `query_files`, `transition_files`, `durable_job_files`,
`association_files` and a dozen more.

**`lower`** — one function per kind: `fn lower(&Schema, &Op) -> Vec<Unit>`.
**Total: it cannot fail**, because `resolve` already checked everything. One
file per kind under `kinds/`. This is where the 39 kinds live and it is where
most of today's `jails-generate` goes — but at roughly a third the size, because
there is no validation, no error path, and no import plumbing left in it.

**`emit`** — `Unit` (a Java AST) → bytes. The critical property: **a type
reference is a value carrying its fully-qualified name, so the import block is
*derived* by walking the tree.** Formatting is the printer's job. The
version/dialect facts — Boot 3 vs 4, `jakarta` vs `javax`, `MockMvcTester` vs
classic `MockMvc` — become **one `Dialect` value threaded into the printer**
instead of a fork per template. **~2,500 lines**, plus templates for method
bodies only, which is where literal Java is genuinely the right representation.

**`apply`** — `Tree` vs `Ledger` vs disk. Keep today's three-way reconciliation
table verbatim; it is correct and hard-won. Write via an inflight marker and
`rename`. **~800 lines.**

**And one new command: `jails adopt schema`.** This is the piece that does not
exist today and the vision does not work without it. Right now the generators
read `Task.java` *continuously*, because there is no import step — that is why
`Target::read` exists at all. The compiler replaces "read continuously" with
"import once, then diff": `adopt schema` parses an existing project's records
into the declaration (reusing `java.rs`'s reader), and from then on the
declaration is the source. Anything it cannot parse stays **unmanaged** — jails
simply does not own it, which is exactly how ownership already works.

The payoff beyond deletion: schema drift becomes *visible*. Today, if someone
hand-adds a component to `Task.java`, jails silently picks it up in one
generator and not another. After, `doctor` reports it as a difference between
the declaration and the tree, and `adopt schema` is the fix. **~400 lines.**

### 1.4 What a kind looks like

Today, adding a kind means edits in `vocabulary/recipe.rs`, `explain.rs`,
`generate/recipes.rs`, `generate.rs`, `generate/recipes/flags.rs`, a `SCENARIOS`
row and a golden tree — 183 `ArtifactKind::` match sites across five files.

After:

```rust
// kinds/transition.rs — the whole kind, one file.
impl Kind for Transition {
    fn spec() -> KindSpec { /* clap name, aliases, suffix, required refs, field policy */ }
    fn explain() -> Explanation { /* replaces explain.rs's arm */ }
    fn example() -> Invocation { /* replaces the SCENARIOS row; goldens derive from it */ }
    fn resolve(ast: &OpAst, s: &Schema) -> Result<Op>;   // this kind's extra checks
    fn lower(s: &Schema, op: &Op) -> Vec<Unit>;          // total
}
```

Adding a kind becomes **one new file** — which is the developer experience jails
sells to its users and does not currently offer its own authors.

This answers the objection recorded in `recipes.rs` ("logic in a descriptor is a
conditional no test can reach directly"). Nothing here becomes data. It is all
ordinary Rust, exhaustive by the compiler, directly unit-testable — just
*collected by subject* instead of scattered across five tables by phase.

### 1.5 What the CLI becomes

Three verbs, which is already the crate layout — evidence that this is the right
cut rather than a new invention:

- **edit** — `g`, `add`, `destroy`, `remove`, `sync`, `resource field …`,
  `rename`, `app apply`. Every one is: edit the declaration, run passes 1–5,
  print the diff.
- **query** — `doctor`, `why`, `explain`, `routes`, `beans`, `stats`, `show`,
  `history`, `commands`, `src`. Every one runs passes 1–4 and inspects the
  result. None touches disk. (`jails-report`'s read-only contract becomes
  structural rather than enforced by layering.)
- **drive** — `test`, `build`, `run`, `db`, `kafka`, `console`, `migrate`,
  `bench`. Genuinely a different program; leave it alone.

### 1.6 The payoff table

This is where the line count goes. Each row is a mechanism that stops being
*implemented* and starts being a *consequence*:

| today | in the compiler |
|---|---|
| `--pretend` machinery through prepare | run passes 1–4, print the diff — "pretend" is *stop before pass 5* |
| per-recipe idempotence checks | nothing: compiling twice yields the same `Tree` |
| `destroy` as a second code path, `ALLOWED_LEFTOVER`, `tests/agreement.rs` | delete the block, recompile, diff |
| `sync` | recompile |
| `doctor` capability drift | compile, diff against disk |
| `--plan-out` / `--plan-in` portable plans | the `jails.toml` diff **is** the portable plan |
| roll-forward recovery, journal state machine, receipts | re-run (compilation is pure) + an inflight marker |
| content-addressed object store, mark-and-sweep gc | ledger holds `path → sha256` |
| `history` / `undo` | git history of `jails.toml` — say so instead of reimplementing version control beside one |
| `Target::read` re-parsing generated Java | a `Schema` lookup; jails stops reverse-engineering its own output |
| 3 models of "a field" (1,561 lines) | one `Field` in `Schema` |
| 8 models of "a file to write" | `Tree = BTreeMap<ProjectPath, Vec<u8>>` |
| 12 models of "the project" | `Schema` + `Ledger` + `Disk` |
| 208 hand-written `Codec` impls (5,598 lines) | serde on a readable ledger |
| 4 templates for one controller test | 1 builder + a `Dialect` |

`tests/agreement.rs` is the clearest case. It exists to check that `generate`
and `destroy` agree about what a kind writes. Under a compiler that question
**cannot be asked**: there is one function, and destroy is "compile without this
declaration."

### 1.7 Sizing — shape, not promise

| crate | today (prod) | target | what it is |
|---|---:|---:|---|
| `jails-syntax` | — | 600 | `Ast`, serde, the field DSL |
| `jails-schema` | — | 1,500 | resolve; `Schema` is the IR |
| `jails-lower` | 21,219 | 7,000 | 39 kinds, one file each, total functions |
| `jails-emit` | (inside generate) | 2,500 | Java/SQL printers; imports derived |
| `jails-project` | 9,705 | 6,000 | readers for pom/gradle/compose/properties |
| `jails-apply` | 29,248 | 2,500 | replaces protocol + prepare + commit |
| `jails-drive` | 8,590 | 8,000 | unchanged |
| `jails-report` | 5,415 | 4,000 | queries over `Schema` + `Tree` |
| `jails` (CLI) | 8,504 | 4,000 | clap + three verbs |
| **total** | **98,084** | **~36,000** | |

Treat 36,000 as the floor the design implies and **50,000 as the number to
plan against** — foreign-file readers and `jails-drive` are irreducible, and
real migrations always keep more than the sketch does. Either way the number
that matters is not the total. It is this one:

> **marginal cost of a new kind: 5 files → 1 file.**

### 1.8 The risk, named

The vision has one load-bearing assumption: **that a project's meaning can live
in a declaration rather than in its Java.** For projects jails created, that is
already true — the ledger holds it. For adopted projects it is true only after
`adopt schema` runs, and only for what that importer can parse.

So the honest boundary is: *jails owns what it can declare; everything else it
leaves alone and says so.* That is the same rule `gradle.rs` already lives by —
answer exactly or refuse — applied to the domain model instead of the build
file. If you are not willing to draw that line, stop at §5's R1–R7; they are
worth doing on their own and they do not depend on this.

---

## 2. The measurements

| | |
|---|---:|
| Rust in `crates/` + `src/` | 122,253 lines (98,084 production) |
| Rust in `tests/` | 30,587 |
| `.rs` files | 326 |
| Templates | 156 files / 10,330 lines |
| Golden trees | 61 trees / 912 files / 39,594 lines |
| Tests | 1,520 `#[test]`, 166 s, green |
| `pub` items | 1,501 |
| CLI surface | 98 subcommands + 39 kinds + 25 capabilities = 162 |
| `target/` | 54 GB |

### Growth

```
2026-08-13     2,481 lines     6 files
2026-08-17     8,425 lines     8 files     ~15 kinds, ~13 commands
2026-08-21    32,833 lines    36 files
2026-08-25   107,889 lines   235 files
2026-08-27   152,163 lines   326 files
```

**The tree is fourteen days old.** On 2026-08-17 jails did most of this job in
8,425 lines across eight files. Since then the surface grew about 7× and the
code grew about 18×. That gap — code outrunning surface by 2.5× — is the whole
subject.

The corollary matters more than the number: **nothing here has been through a
compression pass.** Every mechanism is a first draft that has only ever been
added to. That is a far better position than a codebase that is complex because
it is old.

---

## 3. Why the current shape keeps producing lines

Five machines. Fixing a symptom without stopping the machine buys a week.

### 3.1 No IR — the front end talks straight to text

- **Validation is a type checker written three times.** `usecase_files` spends
  its first 110 lines checking supplied fields against the target: exists,
  compatible type, matching optionality, not the database-assigned key, `--via`
  lookup names a parent component. `query_files` and `transition_files` have
  their own versions. `Target::read` exists *because* three generators raised
  three different wordings for one refusal — its doc comment says so.
- **Cross-entity references are resolved by re-parsing generated Java.**
  `Target::read` → `slice.record(Layer::Domain, "Task")` → scan `Task.java`.
  But the ledger already stores `IntentSpec { arguments, indexes, on, yields,
  via, order_by, limit, on_conflict, path, select, … }`. The schema exists
  twice and the generators trust the *output*.
- **"A field" ×3** (1,561 lines): `spec::Field` (608), `declaration::FieldSpec`
  (953, with `fn projected() -> spec::Field`), `sql::Column`.
- **"A file to write" ×8**: `Artifact` → `Change` → `DesiredChange`/
  `DesiredFile`/`DesiredBody` → `ProjectedEntry` → `PreparedChange`/`FileOp`/
  `GuardedImage` → `FileImage` → `ReportedOp`.
- **"The project" ×12**: `Project`, `ProjectedProject`, `Captured`,
  `ProjectSnapshot`, `Bootstrap`, `OrdinaryBootstrap`, `PendingBootstrap`,
  `LoadedProject`, `ProjectHandle`, `LockedProject`, `ProjectFacts`, `Slice`.
- **A kind is five tables**: `ArtifactKind` is matched in `recipe.rs` (50 sites),
  `explain.rs` (44), `recipes.rs` (40), `generate.rs` (25), `flags.rs` (24).

### 3.2 No emitter — the template language is too weak, so Rust does its job

`template.rs`, as policy: *"Substitution only: no conditionals, no loops … stays
in Rust and is passed in as a rendered value."* The bill:

- **1,048 `format!` and 454 `.join(`** in `jails-generate` alone.
- **1,484 placeholder holes across 156 templates**, 214 distinct names.
- **208 of those holes (14%) exist only to inject `import` statements**, plus
  393 `_import` identifiers in Rust and `import_of` at 122 sites.
- **Combinatorial forks**: one controller test exists four times —
  `resource_controller_test`, `_classic`, `_scoped`, `_scoped_classic` — two
  orthogonal booleans crossed into files. `query_controller` has `_path`,
  `_test`, `_path_test`, `_form_test`.
- **One concept, six holes**: `usecase_controller_java.java` has
  `{{scope_import}}`, `{{scope_field}}`, `{{scope_constructor}}`,
  `{{scope_assignment}}`, `{{scope_parameter}}`, `{{scope_checks}}` — all
  meaning "this endpoint is scope-guarded", all filled by one function.
- The ratchet holding inline Java at zero names **`spring.rs` by path**, so the
  pattern relocated: **16 `r#"package {pkg};` bodies remain in production
  code** in `generate/web.rs` (8), `repository.rs` (3), `cli.rs` (3),
  `closed.rs` (2).

The stated objection — "a conditional in a template is logic no test can reach
and no compiler can check" — is a good argument against Handlebars. It is not an
argument against an emitter, where the conditional is ordinary Rust.

### 3.3 The durability layer is sized for a distributed system

`jails-protocol` (16,917) + `jails-prepare` (8,588) + `jails-commit` (3,743) =
**29,248 lines, 30% of production**, to write files into a directory that is,
in every realistic case, a git working tree. The 61 golden trees hold a median
of 8 files each; the biggest single recipe writes fifteen 40-line Java classes.

Three findings:

1. **The collector is dead.** `gc.rs`, in its own module docs: *"with
   `dead_code = "deny"`, `sweep` and everything it needs are reached from
   nothing. **No commit collects anything, so `.jails/objects` only grows.**"*
   262 lines of complete, unit-tested, unreachable machinery.
2. **The ledger is TOML wrapped around hex.** For the 12-file `scaffold-path`
   golden it is **15,267 bytes**, ~15,100 of them a hex-encoded 7,551-byte
   binary payload. The canonical encoding exists so a SHA-256 is reproducible —
   a property **only jails consumes**, on one machine, unreleased.
3. **Roll-forward recovery duplicates what determinism already gives.**
   `recover.rs`: *"it converges: the same work applied twice lands in the same
   place."* Exactly — generation is a pure function, which is why 61 golden
   trees can compare bytes. A recomputable plan does not need carrying through a
   crash; it needs to be *detectable* as incomplete. An inflight marker listing
   `(path, target_sha256)`, written before the first byte, does that in ~300
   lines and keeps every guarantee anyone depends on.

### 3.4 The dependency ban was lifted; the refund was never collected

`jails-support/src/json.rs` still says *"jails has two dependencies and intends
to keep it that way."* The workspace has **nine** (clap, clap_complete, fs2,
nix, tempfile, sqlparser, serde, serde_json, toml). What did not happen is the
deletion of what existed *because* of the ban:

- **208 `impl Codec` blocks = 5,598 lines**, plus 47 `tag()` and 12
  `from_tag()`. serde is in the tree.
- **1,058 lines of hand-written JSON** (`serialize.rs` + `v2.rs`). serde_json is
  in the tree.
- **`config.rs`, 1,347 lines, still hand-parses TOML** (`split_once('=')`,
  `starts_with('[')`). The `toml` crate is a declared dependency of that exact
  crate, used in **one** file.

`serialize.rs` argues against serde and it is the best-reasoned of these, so it
deserves an answer: of its four objections, **externally-tagged enums, `None` as
`null`, and every-field-emitted are serde's defaults**; sorted-map-as-array
comes free by declaring the field `Vec<Entry>`; and `u64`-as-decimal-string is a
ten-line `serde(with = …)`. The conclusion (don't let serde's defaults define a
normative encoding) is right. "Therefore write 1,058 lines by hand" does not
follow.

### 3.5 Complexity is governed by splitting, and splitting is not removal

`tests/architecture/board.rs` is **986 lines** of ratchets, two of which are
`largest module ≤ 669 lines` and `spring.rs ≤ 558`. They worked exactly as
specified: no module is large, and there are 326 of them. The board measures
*distribution* and never *total*, so splitting a 2,000-line module into four
500-line modules turns every gate green while adding four headers, four
`pub(crate)` surfaces and four names to learn.

The prose has the same shape — **19,825 comment lines (16% of all Rust)** — and
much of it is archaeology:

- **408 citations to design documents; 127 point at files not on disk**
  (`pending.md` 90, `abstract.md` 36, `refactor.md` 1), reachable only through
  `git log --diff-filter=D` + `git show <commit>^:file`.
- **220 mentions of `V1`**, an architecture deleted days ago; **78 "used to"**.
- One integer in `board.rs` carries ~30 sequential justification comments
  ("142 → 143 for `testd::socket_path` … 143 → 145 for `affected::select` …") —
  a hand-maintained changelog of a number `git log -p` already covers.
- And the header of the most important module in the tree,
  `jails-engine/src/route.rs`, still reads *"not yet reachable from dispatch.
  Nothing in `main.rs` calls it"*. `dispatch::mutate` has called it since the
  flip.

---

## 4. What the tests actually protect

The right question before any of this, and it is answerable rather than felt.

### 4.1 The shape of the suite

| layer | count | what it is |
|---|---:|---|
| colocated unit tests | ~1,101 | pure functions, per crate |
| integration tests in `tests/` | 419 | **spawn the real compiled binary** in a scratch dir |
| golden trees | 61 | byte-for-byte snapshots: 912 files, 39,594 lines |
| failpoint sweeps | 17 points × 2 | crash convergence, unit *and* through a real route |
| **total** | **1,520** | 166 s, green |

`tests/common/mod.rs::bin()` is `env!("CARGO_BIN_EXE_jails")` — so every one of
those 419 is a genuine end-to-end test through argv, not a library call.

### 4.2 Command coverage

**48 of 55 top-level subcommands are invoked by some test.** Only two have zero
references anywhere in the suite: **`fmt` and `setup`**. The most-exercised:
`add` (74), `generate` (45), `resource` (30), `test` (28), `app` (26),
`destroy` (22), `doctor` (18), `run` (16), `rename` (15).

### 4.3 The crash suite is better than I expected

There are **17 named failpoints** (`before-file`, `after-journal-active`,
`after-ledger-rename`, `after-receipt-move`, …). They are swept twice:
`crash.rs::every_named_failpoint_converges` at the unit level, and
`tests/engine.rs::a_capability_install_converges_from_every_failpoint` through a
real route, with an assertion that the sweep actually *saw* interruptions rather
than passing vacuously.

That is a **property test** — "from any interruption point, running again
converges" — which is exactly the kind of net a replacement engine can be held
to. It is what makes §5's R8 an engineering task rather than a gamble.

The suite states its own limit honestly, and it matters: *"An injected error
models a process that stopped at that point and unwound. It does not model
losing stack cleanup — that needs a child process and `abort()`."* **So `kill
-9` is not covered today.** The current durability design is not verified against
the failure it is most elaborate about.

### 4.4 Silent skips — measured, and the news is good

`common::skip()` appears at **86 sites**, and a skipped test *passes*. The suite
has the answer built in: `JAILS_REQUIRE_TOOLCHAIN=1` turns every skip into a
failure naming what was missing.

**I ran it.** Result: in the `cli` target — 311 end-to-end tests through the real
binary — **exactly one cannot run**:

```
examples::unheld_gradle_example_manifest_builds_on_its_pinned_toolchain
  Gradle 8.5 running on JDK 21 is required by the example proof policy
```

`mvn`, `java 26` (matching `TARGET_RELEASE`), `docker` and `psql` are all on
this machine, so every other tier-3 test genuinely compiles and runs generated
code. The one gap is Gradle, which is not installed — so `gradle.rs` (1,530
lines, three-valued readers) is exercised only through checkouts that ship a
wrapper.

That is a much better position than the "skipped tests reported as passing"
warning in `CLAUDE.md` implies today. **The net is real.**

### 4.5 So: which steps are protected, and which are naked

| step | net | verdict |
|---|---|---|
| R1 derive codec | 276 protocol unit tests, many round-trip; golden ledgers pin the bytes | **protected** |
| R2 one `Resource` | same, plus `tests/desired.rs`, `engine.rs` | **protected** |
| R3 serde wire formats | `tests/cli/portable_plan.rs`, `history.rs`, JSON assertions in `reports.rs` | **protected** |
| R4 one `Field` | `tests/cli/generate.rs` (108 tests) + goldens | **protected** |
| R5 emitter | 61 golden trees, byte-exact — `golden.rs` says it exists for exactly this | **strongest net in the repo** |
| R6 resolver | 108 generate tests + refusal-message assertions | **protected**, but error *wording* will churn; expect to update assertions deliberately |
| R7 one kind descriptor | `every_kind_has_an_explanation`, `every_kind_and_capability_has_a_golden_scenario`, `commands --json` | **protected** |
| R8 commit engine | 17-point failpoint sweep × 2, `crash.rs`, `engine.rs` | **protected for unwind-crashes; NOT for `kill -9`** |
| Promote manifest to source | `tests/cli/app.rs` (15), `examples/` | **thinnest net** — build it up first |

**Two things to do before touching anything:**

1. `JAILS_REQUIRE_TOOLCHAIN=1 cargo test --workspace` — read every failure. Each
   one is a test you currently believe covers something and does not.
2. Add the missing net where it is thinnest: a golden tree per kind driven from
   `Kind::example()` (so it cannot drift), and a `kill -9` variant of the
   failpoint sweep using a child process, since that is the failure the
   durability design is *for*.

Everything else on the list is a mechanical exercise held to bytes.

---

## 5. The refactors, ranked

Each is a step toward §1, not an independent cleanup.

**R1 — Derive the codec** · −5,000 · risk none.
208 impls, 5,598 lines, all mechanical. Either a `derive(Codec)` proc macro or a
`macro_rules! codec!` covering the struct and externally-tagged-enum shapes with
zero new dependencies. Keep the wire format byte-identical.

**R2 — One `Resource`, not three parallel enums** · −1,000 · risk none.
`ResourceKey` (11 variants), `ResourceValue` (11, same tags), `SemanticEdit`
(11, each `{key, value}`), plus a *runtime* `agrees_with` check that two
hand-written enums still line up. Collapse to one enum with a derived `key()`;
`agrees_with` becomes unrepresentable. `Change`'s eleven parallel `Vec` fields
become one `Vec<Resource>`.

**R3 — serde for both wire formats** · −900 · risk low.
Plus: delete `json.rs`, use `toml` in `config.rs` (keeping the closed-key-set
validation, which is 30 lines on a real parser), and delete
`jails.command-result.v1` — there is no reason for two JSON schema versions in
an unreleased binary.

**R4 — One `Field`** · −700 · risk low. With R1 done, `FieldSpec`'s two reasons
to exist separately (newtypes, hand codec) are both available on `spec::Field`.

**R5 — The Java emitter** · −7,500 · risk medium. §1.3's `emit` pass. Deletes
all 208 import holes, the 393 `_import` identifiers, `import_of`,
`normalize_imports`, `tidy_blank_lines`, `package-info` planning-by-string, and
every combinatorial template fork. **Do it one kind at a time against
`cargo test --test golden`, requiring zero diff.**

**R6 — The resolver** · −4,000 · risk medium. §1.3's `resolve` pass. Every
`lower` function becomes total.

**R7 — One descriptor per kind** · −1,500 · risk low. §1.4. Collapses 183 match
sites across five files into one impl each.

**R8 — Right-size the commit engine** · −22,000 · risk high, value highest.
Keep: never clobber a user edit; `--pretend` parity; `destroy` acts on the
record; a partial apply is detectable; one lock. Delete: object store, dead
collector, journal state machine, receipts-bound-by-checksum, roll-forward,
canonical-binary identity. Replace with a readable ledger, an inflight marker,
and `write-tmp + rename`. **Held to the existing 17-point failpoint sweep** —
and extend it to `kill -9` first (§4.5).

---

## 6. What not to touch

- **The golden suite.** It is what makes every step above mechanical. Grow it.
- **`capture` + `projection`.** One immutable reading, an overlay so a later
  intent sees an earlier one's output, no filesystem during planning. A compiler
  needs precisely this; keep it exactly.
- **The ownership model.** A resource owned by a *set*; `remove` retires one
  claim while another stands; per-key property ownership. This is what makes
  `add`/`remove` inverses.
- **"Refuse rather than guess."** `gradle.rs`'s three-valued readers, `compat`'s
  three states, closed key sets. The best judgement call in the project.
- **`why.rs`'s evidence rule** — rules only from failures that actually happened.
- **`testd` and `classfile.rs`.** Genuinely novel, measured, verified against
  2,957 real class files.
- **The three-way merge table in `reconcile.rs`.** The table is right; only the
  machinery under it is oversized.

---

## 7. Governance: stop the machine, not the output

1. **Ratchet the total, not the distribution.** One new row: total production
   Rust, ceiling 98,084, target 50,000. A split that leaves the total unchanged
   should stop reading as progress.
2. **Add `lines per kind` and `files touched to add a kind`** (today ~540 and 5).
   They predict next month's size.
3. **Ratchet by pattern, not by path.** `SPRING_RS`, `CODEMOD_RS`, `DOCTOR_RS`
   name files, so smells relocate — 16 inline Java bodies now sit in four files
   no gate watches.
4. **Move archaeology to git.** A comment explains the code as it *is*; git
   explains how it got there. Start by deleting the ceiling-change log in
   `board.rs` and fixing `route.rs`'s header today.
5. **Before adding a mechanism, price the cheaper answer to the same question.**
   The object store, the journal state machine and the canonical identity each
   answer something real. None was measured against its cheap alternative, and
   each arrived with a codec, a module doc, a test suite and a vocabulary.

---

## 8. Sequence

| # | step | delta | risk |
|---|---|---:|---|
| 0 | ~~`JAILS_REQUIRE_TOOLCHAIN=1 cargo test`~~ **done** — 1 skip (Gradle); install Gradle 8.5 or accept the gap | — | — |
| 1 | R1 derive the codec | −5,000 | none |
| 2 | R2 one `Resource` | −1,000 | none |
| 3 | R3 serde; delete `json.rs`; `toml` in `config.rs`; drop JSON v1 | −900 | low |
| 4 | R4 one `Field` | −700 | low |
| 5 | Governance: total-lines ratchet, path→pattern gates, fix `route.rs` | ±0 | none |
| 6 | R7 one descriptor per kind | −1,500 | low |
| 7 | **R5 the emitter**, one kind per commit against the goldens | −7,500 | medium |
| 8 | **R6 the resolver**; `lower` becomes total | −4,000 | medium |
| 9 | Readable ledger (drop the hex payload) | −1,000 | medium |
| 10 | `kill -9` failpoint sweep | +200 | low |
| 11 | R8 right-size the commit engine | −22,000 | high |
| 12 | Promote `jails.toml` to source; CLI edits it | −5,000 | high |

Steps 1–6 are roughly three days of mechanical work, take the tree to ~89,000
production lines, and change no behaviour.

Steps 7–12 are the vision. **They are done as a strangler, not a rewrite** —
which is the shape this repo has already executed once, successfully. Its own
words for the V1→V2 migration: *"land the executor dark, and build each
command's route while default dispatch stays on V1."* Do the same: build
`resolve`/`lower`/`emit` beside the current path behind a flag, port one kind
per commit, require zero golden diff each time, flip the default when all 39
pass, then delete the old path.

At no point is the tree red, and at no point is there a commit you cannot
revert.
