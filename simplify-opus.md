# simplify-opus.md — where the complexity actually comes from, and what to do

Written 2026-08-27 after reading the tree end to end: 326 `.rs` files, 13 crates,
156 templates, 61 golden trees. Every number below is measured from this
checkout, not estimated. Where I disagree with a decision the source already
argues for, I quote the source's argument first and answer it.

The short version: **the architecture is not wrong, it is unfinished in one
specific way — jails is a compiler that has no intermediate representation.**
Everything expensive in this tree is a consequence of that, plus one policy
(no dependencies) that was abandoned without collecting the refund.

---

## 1. The measurements

| | |
|---|---:|
| Rust in `crates/` + `src/` | 122,253 lines |
| — production (excluding colocated `#[cfg(test)]`) | 98,084 |
| — colocated tests | 24,169 |
| Rust in `tests/` | 30,587 |
| `.rs` files | 326 |
| Templates | 156 files / 10,330 lines |
| Golden trees | 61 trees / 912 files / 39,594 lines |
| Tests | 1,520 `#[test]`, 166 s, green |
| `pub` items across the workspace | 1,501 |
| CLI surface | 98 subcommands + 39 kinds + 25 capabilities = 162 |
| `target/` | 54 GB |

### Growth

```
2026-08-13     2,481 lines     6 files
2026-08-17     8,425 lines     8 files      ~15 kinds, ~13 commands
2026-08-21    32,833 lines    36 files
2026-08-25   107,889 lines   235 files
2026-08-27   152,163 lines   326 files
```

**The whole tree is fourteen days old.** On 2026-08-17 jails did most of this
job in 8,425 lines across eight files. Since then the user-visible surface grew
about 7× and the code grew about 18×. That gap — code growing 2.5× faster than
surface — is the entire subject of this document. It is not a judgement about
the features; it is the definition of accumulating incidental complexity, and it
is measurable rather than felt.

The corollary matters more than the number: **nothing here has ever been
through a compression pass.** Every mechanism in the tree is a first draft that
has only ever been added to. That is a much better position to be in than a
codebase that is complex because it is old.

---

## 2. Five mechanisms

Not five problems — five *machines* that turn each new feature into more lines
than it needs. Fixing a symptom without stopping the machine buys a week.

### 2.1 The dependency ban was lifted; the refund was never collected

`jails-support/src/json.rs` says, today, in its module docs:

> jails has two dependencies and intends to keep it that way.

The workspace manifest has **nine**: clap, clap_complete, fs2, nix, tempfile,
sqlparser, serde, serde_json, toml. The ban is gone. What did not happen is the
deletion of everything that existed *because* of it:

- `jails-support/src/codec.rs` + **208 hand-written `impl Codec` blocks =
  5,598 lines**, plus 47 `fn tag()` and 12 `fn from_tag()`. serde is already in
  the tree.
- `jails-prepare/src/serialize.rs` + `serialize/v2.rs` = **1,058 lines of
  hand-written JSON emission**. serde_json is already in the tree.
- `jails-support/src/json.rs`, a hand-rolled string escaper, 92 lines.
- `jails-project/src/config.rs`, **1,347 lines**, still hand-parses
  `jails.toml` line by line (`split_once('=')`, `starts_with('[')`). The `toml`
  crate is a declared dependency of that exact crate and is used in **one
  file** (`application_manifest.rs`).

This is the worst of both positions: the audit surface and build weight of nine
dependencies, and the line count of zero.

`serialize.rs` argues against serde specifically, and it is the one argument
worth answering in detail because it is the best-reasoned of them:

> A `u64` emitted as a JSON number loses precision in every JavaScript consumer
> above 2^53 … Maps are therefore sorted arrays … Every declared field is
> emitted; `None` is `null` … Variants are externally tagged.

Three of those four are serde's **defaults**. Externally tagged is the default.
Emitting `None` as `null` is the default (only `skip_serializing_if` omits).
Sorted map-as-array is what you get for free by declaring the field
`Vec<Entry>` instead of `BTreeMap`. The remaining one — `u64` as a decimal
string — is a ten-line `serde(with = …)` module applied at four or five sites.
The conclusion ("do not let serde's defaults decide a normative encoding") is
right. The implementation ("therefore write 1,058 lines by hand") does not
follow from it.

### 2.2 There is no IR: the front end talks directly to text

This is the load-bearing one.

The pipeline is `argv → Recipe → match on 39 kinds → Vec<Artifact{path,
String}>`. There is no stage in between where the *meaning* of a request is a
value. Consequences, each visible in the tree:

**Validation is re-implemented per kind.** `usecase_files` in
`spring/workflow.rs` spends its first 110 lines checking that each supplied
field exists on the target, has a compatible type, matching optionality, is not
the database-assigned key, and that a `--via` lookup names a parent component
rather than a target one. `query_files` and `transition_files` do their own
versions of the same thing. `Target::read` exists precisely because three
generators each raised their own wording for one refusal — the comment says so.
That is a **type checker, written three times, inline, in the middle of a code
generator.**

**Cross-entity references are resolved by re-parsing generated Java.**
`Target::read` → `slice.record(Layer::Domain, target)` → parse the record
components out of `Task.java`. But the ledger *already stores* the declaration:
`IntentSpec { arguments, indexes, timestamps, on, yields, via, order_by, limit,
on_conflict, path, select, … }`. So the project's schema exists twice — once as
a durable declaration jails wrote, once as Java jails also wrote — and the
generators trust the second. Every "does `Task` have `status`" question is a
filesystem read and a hand-rolled Java scan.

**"A field" is modelled three times.** `jails_spec::spec::Field` (608 lines),
`jails_protocol::declaration::FieldSpec` (953 lines, with
`fn projected() -> jails_spec::spec::Field`), and `sql::Column` (996 + 612
lines of projection). 1,561 lines before you reach SQL.

**"A file to write" is modelled eight times.** `Artifact` → `Change` →
`DesiredChange`/`DesiredFile`/`DesiredBody` → `ProjectedEntry` →
`PreparedChange`/`FileOp`/`GuardedImage` → `FileImage` → `ReportedOp`. Each has
a module doc, a codec, and tests. Most of the conversions are total and
information-preserving; they exist because each crate wanted its own vocabulary,
not because a fact changes.

**"The project" is modelled twelve times.** `Project`, `ProjectedProject`,
`Captured`, `ProjectSnapshot`, `Bootstrap`, `OrdinaryBootstrap`,
`PendingBootstrap`, `LoadedProject`, `ProjectHandle`, `LockedProject`,
`ProjectFacts`, `Slice`. Some of these are genuine phase types (a `LockedProject`
really is a different capability from a `Project`). Most are the same facts
under a different crate's name.

**Adding a kind touches five parallel tables.** `ArtifactKind` is matched in
`vocabulary/recipe.rs` (50 sites), `explain.rs` (44), `generate/recipes.rs`
(40), `generate.rs` (25), `generate/recipes/flags.rs` (24). Plus a `SCENARIOS`
row, plus a golden tree. `recipes.rs` argues for keeping the match:

> It is a `match` on purpose, not a table. … the ones that read a record off
> disk, refuse a precondition or vary structurally are logic, and logic in a
> descriptor is a conditional no test can reach directly.

Correct, and it argues against the wrong alternative. The answer is not a data
table with escape hatches; it is **one trait with one impl per kind, in one file
per kind**, where the varying parts are methods. That is still logic, still
directly testable, still exhaustive by the compiler — and it is one edit instead
of five.

### 2.3 The template language is too weak, so Rust does its job

`jails-java/src/template.rs`, stated as policy:

> Substitution only: no conditionals, no loops, no expressions. Anything that
> varies structurally … stays in Rust and is passed in as a rendered value.

The bill for that policy, measured:

- **1,048 `format!` and 454 `.join(`** in `jails-generate` alone.
- **1,484 placeholder holes across 156 templates, 214 distinct names.**
- **208 of those 1,484 holes (14%) exist only to inject `import` statements**,
  and there are 393 `_import` identifiers in the generator Rust plus
  `import_of` at 122 sites.
- **Combinatorial template forks.** One controller test exists four times:
  `resource_controller_test`, `…_classic`, `…_scoped`, `…_scoped_classic` —
  two orthogonal booleans (MockMvcTester vs classic MockMvc; scoped vs not)
  crossed into files. `usecase_controller_test` and `cors_config_test` each
  have a `_classic` twin. `query_controller` has `_path`, `_test`,
  `_path_test`, `_form_test`.
- **One concept shredded into six holes.** `usecase_controller_java.java` has
  `{{scope_import}}`, `{{scope_field}}`, `{{scope_constructor}}`,
  `{{scope_assignment}}`, `{{scope_parameter}}`, `{{scope_checks}}` — six holes
  filled by one tuple-returning function, all meaning "this endpoint is
  scope-guarded."
- The ratchet that holds inline Java at zero names **`spring.rs` by path**, so
  the pattern relocated: **16 `r#"package {pkg};` bodies remain in production
  code** in `generate/web.rs` (8), `repository.rs` (3), `cli.rs` (3),
  `closed.rs` (2).

The stated reason for the policy — "a conditional in a template is logic that
no test can reach directly and no compiler can check" — is a real objection to
Handlebars. It is not an objection to the thing that actually fits here, which
is an emitter (§3.5).

### 2.4 The durability layer is sized for a distributed system

`jails-protocol` (16,917 production lines) + `jails-prepare` (8,588) +
`jails-commit` (3,743) = **29,248 lines, 30% of production**, to write files
into a project directory.

What is in there: a canonical binary codec with domain-separated SHA-256
identities; a content-addressed object store sharded by hex prefix; a
write-ahead journal with a state machine and a checksum-over-everything-but-
itself; receipts cross-bound to journals by checksum; guarded preimages; a
three-way merge; hard-link publishing with directory `fsync`; roll-forward crash
recovery; a mark-and-sweep collector.

Now the scale it operates at. The 61 golden trees hold a **median of 8
files each** (max 58) — and that count includes `pom.xml` and `.jails/` itself;
the biggest single recipe, `g scaffold`, writes fifteen. The files are
40-line Java classes. The project is, in every realistic case, a git working
tree.

Three specific findings:

1. **The collector is dead.** `gc.rs` says so in its own module docs: *"with
   `dead_code = "deny"`, `sweep` and everything it needs are reached from
   nothing. **No commit collects anything, so `.jails/objects` only grows.**"*
   Every rendered body, base and preimage the project has ever had is still on
   disk. 262 lines of complete, unit-tested, unreachable machinery.

2. **The ledger is a TOML file containing hex.** For the 12-file
   `scaffold-path` golden, `.jails/ledger.toml` is **15,267 bytes**, of which
   ~15,100 are a hex-encoded 7,551-byte binary payload under
   `payload_hex = "0000000530..."`. The TOML wrapper buys nothing (four header
   keys), the hex doubles the size, and the canonical binary encoding exists so
   the SHA-256 is reproducible — a property **only jails itself consumes**,
   on one machine, in an unreleased tool. A readable ledger would be diffable,
   greppable, editable in an emergency, and readable by `jails why`, `history`
   and `doctor` without the protocol crate.

3. **Roll-forward recovery is solving a problem that determinism already
   solves.** `recover.rs`: *"Rolling forward needs the prepared bytes, which the
   journal already carries, and it converges: the same work applied twice lands
   in the same place."* That last clause is the whole point — **generation is a
   pure function of (project state, declaration)**, which is exactly why 61
   golden trees can compare bytes. If the plan can be recomputed, it does not
   need to be *carried* through a crash. It needs to be *detectable* as
   incomplete.

   The minimal design with the same guarantee: write `.jails/inflight.json`
   listing `(path, target_sha256)` **before** touching anything; write each file
   to `path.jails-tmp` and `rename`; delete inflight; rewrite the ledger. On the
   next run, an inflight file means "these paths are mine and possibly
   half-written" — every path whose hash matches is done, every other one is
   rewritten, and the ownership question that the current design's
   ledger-written-last ordering creates never arises. That is roughly 300 lines,
   and it keeps every guarantee anybody actually depends on: never clobber a
   user edit, `--pretend` parity, `destroy` knows what it wrote, and a partial
   apply is detectable and repairable.

   What it gives up: nothing that is currently used. Content-addressed dedup
   across transactions (never collected anyway), cross-machine reproducible
   transaction identity (no second machine), and rollback-by-preimage (already
   explicitly not the crash policy).

### 2.5 Complexity is governed by splitting, and splitting is not removal

`tests/architecture/board.rs` is **986 lines** of ratchets. Two of them are
`production lines in the largest module: ceiling 669` and `spring.rs lines:
ceiling 558`. They worked exactly as specified: no module is large. There are
326 of them.

The board measures *distribution* and never *total*. Splitting a 2,000-line
module into four 500-line modules moves every gate green while adding four
module headers, four `pub(crate)` surfaces, four entries in a facade block, and
four names a reader has to learn. The tree went from 8 files to 326 in ten days
with every gate green.

The prose has the same shape. **19,825 comment lines (16% of all Rust)**, of
which 15,934 are doc comments. A large share is archaeology rather than
explanation:

- **408 citations to design documents** in source comments. **127 of them
  point at files that are not on disk** (`pending.md` 90, `abstract.md` 36,
  `refactor.md` 1) and resolve only through
  `git log --diff-filter=D` + `git show <commit>^:file`.
- **220 mentions of `V1`**, an architecture deleted days ago.
- **78 "used to"**.
- One ceiling in `board.rs` carries ~30 sequential justification comments —
  "142 → 143 for `testd::socket_path` … 143 → 145 for `affected::select` …" —
  a hand-maintained changelog of a single integer, in a file `git log -p`
  already covers.

And the top-of-file doc on the single most important module in the tree,
`jails-engine/src/route.rs`, currently reads:

> The V2 route, assembled end to end and **not yet reachable from dispatch**.
> Nothing in `main.rs` calls it; the tests do.

`src/dispatch.rs::mutate` has called it since the flip. The most-read paragraph
in the codebase describes an architecture that no longer exists — which is the
strongest possible argument that history belongs in git rather than in headers.

---

## 3. The refactors, ranked

Each carries an estimated line delta and the safety net that makes it
tractable. **The 61 golden trees are that net**: they compare bytes, so any
refactor that is supposed to be behaviour-preserving is verified by
`cargo test --test golden`. That harness is the single best thing in this
repository and it is what makes the expensive items on this list rational
rather than reckless.

### R1 — Derive the codec  · −5,000 · risk: none

208 `impl Codec` blocks, 5,598 lines, all mechanical. Two options:

- A `derive(Codec)` proc macro (needs `syn`/`quote`; you already took nine deps).
- A `macro_rules! codec!` covering the struct and externally-tagged-enum shapes,
  with hand-written impls kept only where encoding is genuinely non-obvious.
  Zero new dependencies, probably covers 85% of the 208.

Keep the wire format byte-identical; the crash suite and the golden ledgers
verify it. Fold `fn tag()`/`from_tag()` into the same derive.

### R2 — One `Resource`, not three parallel enums · −1,000 · risk: none

`ResourceKey` (11 variants), `ResourceValue` (11 variants, same tags),
`SemanticEdit` (11 variants, each `{key: ResourceKey, value: …}`), plus
`ResourceValue::agrees_with(&ResourceKey)` — a *runtime* check that two enums
you wrote by hand still line up. Collapse to:

```rust
enum Resource {
    WholeFile { path: ProjectPath },
    MavenDependency { coordinate: MavenCoordinate, spec: DependencySpec },
    BuildFeature { feature: BuildFeature, maven: PluginSpec },
    // …
}
impl Resource { fn key(&self) -> ResourceKey { … } }   // derived, not asserted
```

`agrees_with` becomes unrepresentable rather than checked. `Change`'s eleven
parallel `Vec` fields become one `Vec<Resource>`, which removes the "add a new
ownable thing → edit eight files" fan-out.

### R3 — serde for both wire formats · −900 · risk: low

`serialize.rs` + `serialize/v2.rs` → derives plus a `u64_as_string` module.
Keep the *encoding contract* verbatim (it is well-reasoned); drop the hand
implementation. While there: there is no reason for a `jails.command-result.v1`
**and** a v2 in an unreleased binary — pick one, delete `from_v1`, delete
`Output::JsonV1`.

Then delete `jails-support/src/json.rs`, and replace `config.rs`'s hand parser
with `toml` (keeping the closed-key-set validation, which is the part that
matters and is 30 lines on top of a real parser).

### R4 — One `Field` · −700 · risk: low

`FieldSpec` exists separately from `spec::Field` because it needed validated
newtypes and a hand-written codec. With R1 both properties are available on one
type. Merge them; delete `projected()`.

### R5 — A Java emitter · −6,000 to −9,000 Rust, −1,500 templates · risk: medium

This is the highest-value item in `jails-generate`.

Introduce an output IR and a printer:

```rust
struct Unit { package: Package, types: Vec<TypeDecl> }
enum Type { Ref(Fqn), Generic(Fqn, Vec<Type>), Primitive(&'static str) }
struct Method { annotations: Vec<Anno>, ret: Type, params: Vec<Param>, body: Body }
```

`Type` is a value carrying a fully-qualified name. **Imports become a derived
property of the unit** — walk the tree, collect FQNs, drop `java.lang` and
same-package, sort, emit. That single change deletes:

- all 208 import-shaped template holes and the 393 `_import` identifiers;
- `import_of` (122 sites), `extra` threading (65), `push_type_import`;
- `normalize_imports` and `tidy_blank_lines` at write time (the printer owns
  layout, so there is nothing to normalise after the fact);
- `refuse_java_lang_shadow` as a *check* — the emitter simply qualifies a
  shadowed name;
- `package-info.java` planning-by-string.

And it dissolves the combinatorial forks. `classic` vs `MockMvcTester` becomes a
`MockMvcDialect` consumed by one `http_test_unit()` builder; `scoped` becomes an
`if` in Rust. Four controller-test templates become one builder. The same trick
retires the `jakarta`/`javax` and Boot 3/Boot 4 version sniffs into one
`Dialect` value threaded through the printer instead of through every template.

Templates do not go away — they stay for *bodies*, which is where literal Java
is the right representation. They stop being whole-file forms with 17 holes.

**This is exactly what the goldens are for.** Rewrite one kind, run
`cargo test --test golden`, require zero diff, move to the next.

### R6 — One resolver: name resolution and type checking, once · −3,000 to −5,000 · risk: medium

Insert the missing pass:

```
Recipe ──resolve(&Schema)──▶ Resolved { target: &Entity, inputs: Vec<ResolvedField>,
                                        defaults: Vec<Expr>, endpoint: Endpoint, … }
```

`resolve` is the *only* place that:

- looks up `--on` / `--via` / `--yields` and refuses with the "generate it
  first" message;
- checks a supplied field against the target's declared component (type,
  optionality, database-assigned key);
- resolves `--set` pins, `@scope` requirements, `--select`, `--order-by`;
- computes conventional defaults for unsupplied components.

Every `*_files` function then takes a `Resolved` and contains **no error path at
all** — it is a total function from a checked value to a `Unit`. That is what
makes the 22 generators shrink: today roughly a third of `usecase_files`,
`query_files` and `transition_files` is validation, restated.

`Target::read`, `record_in`-for-jails-written-facts, and the "is the field on
the record" scans all disappear into `resolve`.

### R7 — One descriptor per kind · −1,500 · risk: low

```rust
trait Kind {
    fn spec() -> KindSpec;                       // clap name/aliases, suffix, refs, field policy
    fn explain() -> Explanation;                 // replaces explain.rs's 44 arms
    fn example() -> Invocation;                  // replaces the SCENARIOS row
    fn lower(r: &Resolved) -> Result<Vec<Unit>>; // replaces the recipes.rs arm
}
```

One file per kind under `crates/jails-generate/src/kinds/`. The five parallel
`ArtifactKind` tables (183 match sites) collapse into one impl each. Adding a
kind becomes **one new file** — which is the DX jails offers its users and does
not yet offer its own authors.

This answers `recipes.rs`'s objection directly: nothing becomes data, nothing
gets an escape hatch, everything stays compiler-checked and unit-testable.

### R8 — Right-size the commit engine · −20,000 to −24,000 · risk: high, value highest

Keep, precisely:

1. never overwrite a byte the user wrote (three-way merge against a recorded base);
2. `--pretend` shows exactly what will happen;
3. `destroy` acts on what was recorded, not on a recomputed path list;
4. a partial apply is detectable and repairable;
5. one lock per project.

Delete: the content-addressed object store, the mark-and-sweep collector (dead),
the journal state machine, receipts-bound-to-journals-by-checksum, roll-forward
recovery, canonical-binary-identity, and most of `jails-protocol`'s codec
surface.

Replace with:

```
.jails/ledger.toml     readable: path → { owners, base_sha256, mode }, entity declarations
.jails/inflight.json   written before any file touch: [ {path, target_sha256} ]
.jails/history/NNN.json.gz  optional, retained N: the before/after hashes and small diffs
```

Apply = write `path.jails-tmp`, `rename`, repeat; delete inflight; rewrite
ledger. Recover = inflight present → hash each listed path, finish the ones
that do not match, re-run the pure planner if the plan is needed. Undo = replay
a history entry, or `git checkout` — say so out loud rather than reimplementing
version control beside one.

**Do this last**, after R6, because a resolver plus a readable ledger is what
makes the smaller engine obviously sufficient.

---

## 4. The bet: make the CLI edit a source file, and make the tool a compiler over it

Everything above is repair. This is the one structural change that would make
the repairs unnecessary in the next feature, and it is mostly *already built*.

Observe what `jails g transition` already is:

```
jails g transition Approve id:uuid tenantId:uuid@scope status:TaskStatus version:long \
  --on Task --select id --if-match required --set senderType=USER --method PUT --consumes json --path /x
```

That is a sentence in a domain-specific language with a bad grammar: ten
modifiers, no comments, no way to see two declarations side by side, no diff, no
review. And `.jails/app.toml` plus `jails app apply` is the *same language*
already written down — `kind`/`name`/`fields`/`timestamps`/`indexes`/`package`/
`on`/`yields` — with reconciliation over the whole manifest in one transition.

**Promote it.** The manifest becomes the source of truth; the CLI becomes a
front end that edits it and recompiles — exactly as `cargo add` edits
`Cargo.toml`.

```
app.toml (source)  ──parse──▶  Declaration
Declaration + adopted facts ──resolve──▶  Schema        ← IR-1, persisted, readable
Schema ──lower──▶  Vec<Unit> + SQL + build claims       ← IR-2, pure
Units ──emit──▶  bytes
bytes + ledger ──reconcile──▶  writes
```

What falls out for free, rather than being implemented:

| today | under a compiler |
|---|---|
| `--pretend` machinery | compile, diff, stop |
| idempotence checks per recipe | recompile is a pure function |
| `destroy` + `ALLOWED_LEFTOVER` + `tests/agreement.rs` | delete a declaration, recompile, diff |
| `sync` | recompile |
| `doctor` capability drift | compile and diff |
| portable plans (`--plan-out`/`--plan-in`) | the source file **is** the portable plan |
| `history` / `undo` | `git log` / `git checkout` of the source file |
| roll-forward recovery | re-run |

`tests/agreement.rs` is worth dwelling on: it exists to check that `generate`
and `destroy` agree about what a kind writes. Under a compiler that question
cannot be asked — there is one function, and destroy is "compile without this
declaration."

**Where the "dynamic schema" idea belongs.** IR-1 is it. Persist the resolved
schema in the ledger as readable TOML: entities, fields, types, constraints,
relations, operations, capabilities. Then:

- `jails resource field add` = edit the schema, recompile.
- Cross-entity resolution reads the schema, never `Task.java`.
- A hand-edited record is reconciled **once**, explicitly, at adopt/capture
  time — parsed into the schema and recorded — after which divergence is a
  `doctor` finding rather than a silent re-read that three generators each
  perform differently.

That last point retires the deepest coupling in the tree: today the *output*
is the schema, so jails must reverse-engineer its own output on every
subsequent command, and `jails-java`'s Java reader has to keep growing to do it.

**Risk, stated honestly.** This changes the product's shape: `jails g X` stops
being a write and becomes an edit-plus-recompile. Two things make it much
smaller than it sounds: `app apply` already reconciles a whole manifest in one
transition, and the goldens pin the output bytes. The migration is per-kind:
route a kind through the manifest, require zero golden diff, move on.

**If you do only one thing from this document, do R5 (the emitter) and this.**
R5 pays for itself inside `jails-generate` immediately; the compiler framing is
what stops kind number 40 costing five files.

---

## 5. What not to touch

Deleting good work in a simplification pass is the standard failure mode. These
are load-bearing and correct:

- **The golden suite.** 61 trees, 912 files. It is what makes every refactor
  above a mechanical exercise instead of a rewrite. Grow it, do not trim it.
- **`capture` + `projection`.** "One immutable reading of a project, taken
  once", with an overlay so a later intent sees an earlier one's output, and no
  filesystem access during planning. That is the right design, it is well
  argued, and only 4 direct `fs::read` calls remain in `jails-generate`. Keep it
  exactly; a compiler needs precisely this.
- **The ownership model.** A resource claimed by a set of owners, `remove`
  retiring one claim while another stands, per-key property ownership instead of
  marked blocks. This is the thing that makes `add`/`remove` inverses, and it is
  hard-won.
- **"Refuse rather than guess."** `gradle.rs`'s three-valued readers,
  `compat`'s three states, the closed-key-set validation, the refusal to infer a
  default it cannot prove. This is the best judgement call in the project.
- **`why.rs`'s evidence rule** — rules only from failures that actually
  happened, mined from real logs.
- **`testd` and `classfile.rs`.** Genuinely novel, measured (0.06–0.10 s vs
  0.62 s), verified against 2,957 real class files.
- **The three-way merge semantics** in `reconcile.rs`. The *table* is right even
  though the machinery underneath it (§3.8) is oversized.

---

## 6. Governance: stop the machine, not just the output

The board works — it just measures the wrong axis. Concretely:

1. **Ratchet the total, not the distribution.** Add one row: *total production
   Rust lines*, ceiling = today's 98,084, target 60,000. Keep the largest-module
   row as a growth guard, but a split that leaves the total unchanged should no
   longer read as progress.

2. **Add `lines per kind` and `files touched to add a kind`.** They are the
   numbers that predict next month's size. Today: ~540 and 5.

3. **Ratchet by pattern, not by path.** `SPRING_RS`, `CODEMOD_RS`, `DOCTOR_RS`
   name files, so the smell relocates — 16 inline Java bodies now live in four
   files the gate does not watch. Every gate should scan all of `crates/*/src`.

4. **Move archaeology to git.** The 127 citations to files that are not on disk
   and the 220 `V1` references are load on every reader for a benefit only the
   author of that line ever collects. Rule: **a comment explains the code as it
   is; git explains how it got there.** Concretely — delete the sequential
   ceiling-change log in `board.rs` (one line, one reason, `git log -p` for the
   rest), and fix `route.rs`'s header today.

5. **One doc-comment budget.** 16% of the tree is comments and it is climbing.
   A module header that is longer than the module is a sign the module should be
   in a design doc or in a test name.

6. **Before adding a mechanism, ask what it costs to *not* have it.** The object
   store, the journal state machine and the canonical binary identity each
   answer a real question. None of them was measured against the cheaper answer
   to the same question, and each brought a codec, a module doc, a test suite
   and a vocabulary with it.

---

## 7. Sequence

Ordered so each step makes the next cheaper, and every step ends green.

| # | step | delta | risk |
|---|---|---:|---|
| 1 | R1 derive the codec | −5,000 | none |
| 2 | R2 one `Resource` | −1,000 | none |
| 3 | R3 serde wire formats; delete `json.rs`; `toml` for `config.rs`; drop JSON v1 | −900 | low |
| 4 | R4 one `Field` | −700 | low |
| 5 | Governance: total-lines ratchet, path→pattern gates, delete archaeology, fix `route.rs` | ±0 | none |
| 6 | R7 one descriptor per kind | −1,500 | low |
| 7 | R5 the Java emitter, one kind at a time against the goldens | −7,500 | medium |
| 8 | R6 the resolver | −4,000 | medium |
| 9 | Readable ledger (drop the hex payload) | −1,000 | medium |
| 10 | R8 right-size the commit engine | −22,000 | high |
| 11 | Promote the manifest to source; CLI edits it | −5,000 | high |

Steps 1–6 are about three days of mechanical work and take the tree from 98,084
to roughly **89,000** production lines with no behaviour change. Steps 7–11 take
it to somewhere around **50,000–55,000** with the same 162-item surface, and —
the part that actually matters — take the marginal cost of a new kind from five
files to one.

**The first thing to do is none of these.** It is to run
`cargo test --test golden` and confirm it is the net this document assumes it
is. Everything above is safe because that suite compares bytes. If it does not
cover a kind, that gap is the first commit.
