<!--
One of six. `docs/00-contracts.md` is the one every reader starts from; it
carries the contracts, the identifier map and the ownership table that keep
these six from contradicting each other.

**A closed item is deleted from the file that holds it**, in the commit that
closes it -- never marked done. `git log -p -- docs/` is the record.

**Item and section numbers are stable and never reused.** A section with no
open items disappears rather than being renumbered.

Status prose is dated where it is a measurement. Everything else is written in
the present tense as a rule: a note narrating what a module used to be gives a
reader nothing to act on and goes stale on its own.
-->

# 10 — The language — model, grammar, linker, diagnostics

**Read `docs/00-contracts.md` first.** It carries the five contracts, the
deletion map, the identifier map and the ownership table; nothing here repeats
them, and work that contradicts them is wrong however well it reads.

## What you own

`crates/jails-model/**` -- the closed source schema, stable IDs, linking,
semantic diagnostics, `AppModel` and `ModelPatch`. Above it, the JDL front
ends in the binary: `src/model_generate_jdl*`, `src/model_jdl_edit.rs`,
`src/model_upgrade.rs`, `src/model_explain.rs`.

You are the only workstream that may change what a `.jails/model.jdl` means.

## What you do not touch

`crates/jails-compiler/**` and `templates/**` are B's: an emitter reading a
new model field is B's change, not yours. Adding the field, linking it and
diagnosing it is yours. Coordinate by landing the model half first -- a field
nothing reads is inert, an emitter reading a field that does not exist does not
compile.

Three files are shared with the other three workstreams and have resolution
rules in `docs/00-contracts.md`: `tests/golden/**`, `tests/architecture/board.rs`
and `LAYERS`. Append to `tests/common/scenarios.rs`; move nothing in it.

## The specification sections this work answers to

§5 lexical, §6 grammar, §7 source and linked model, §8 stable identity,
§9 app/types/fields, §10 enums, §11 projections, §18 static semantics and
diagnostics, §19 formatting and CLI source edits, §20.3 normalization,
§20.4 language versioning, §21 conformance, §22 upgrade from the pre-v1 draft.

## The surface you own, as closed registries

Every one of these is a closed set: an unknown member is a refusal, never a
silent no-op, and that is what §6.2 of `docs/00-contracts.md` is protecting.
They are the things a change in this workstream adds to, and each one is
exhaustive somewhere the compiler checks — so the count is a fact about the
tree, not a tally somebody maintains.

Counts taken 2026-09-01, by the commands below the table and, for the two
diagnostic rows, by the ones in A3.13. A count whose method is unrecorded can
only be replaced, not re-measured — which is how the numbers this document
carried before drifted without anyone noticing.

| registry | n | where | spec |
|---|---:|---|---|
| capability kinds (`cap`) | 24 | `CAPS`, `jdl/v1/parser/declaration.rs` | §15.1 |
| component kinds (`component`) | 23 | `ComponentKind::parse`, `component.rs`; rules in `linker/component/registry.rs` | §14.2 |
| builtin scalars | 16 | `ALL`, `builtin.rs` — one `BuiltinSemantics` row each | §9.2 |
| field attributes | 13 | `parse_field`'s `reject_unknown_attributes`, `jdl/v1/parser/declaration.rs` | §9.4 |
| projection spellings (`use`) | 9 | `projection_list`, `jdl/v1/parser/projection.rs` — 8 kinds plus the `scaffold` macro | §11.1 |
| operation statements | 14 | `parse_operation_member`, `jdl/v1/parser/operation.rs` | §12 |
| `ModelPatch` variants | 34 | `patch.rs` | §16 |
| syntax diagnostics (`JDL*`) | 96 | `jdl/v1/` | §18 |
| semantic diagnostics (`model-*`) | 148 | the linker | §18.2 |

```
grep -n 'const CAPS' -A 32 crates/jails-model/src/jdl/v1/parser/declaration.rs
grep -c '=> Ok(Self::' crates/jails-model/src/component.rs
grep -c 'token: "'   crates/jails-model/src/builtin.rs
awk '/^pub enum ModelPatch/,/^}/' crates/jails-model/src/patch.rs | grep -cE '^    [A-Z]'
grep -o '"[a-z-]*"' crates/jails-model/src/jdl/v1/parser/projection.rs | sort -u
```

Three properties of that table are the ones worth defending, because each has
already been paid for once:

- **The registry is the authority, not a second list.** `ComponentKind`'s
  member rules live in one exhaustive `match` in
  `linker/component/registry.rs`, so a kind added without a row is a compile
  error rather than a silently permissive declaration. `Presence` is
  three-valued for the same reason: `Optional` has to be a deliberate answer.
- **One question, one answer.** `BuiltinSemantics` is one row per builtin and
  the board holds a ratchet on the *largest table of per-builtin knowledge
  outside its row*, because the failure mode here is a second table that
  drifts. `TypeRef::parse` had exactly that — a private copy of "is this a
  Java identifier" beside `naming.rs`'s keyword-aware one — and it emitted
  `import enum.PENDING.PAID;` into a file that could not compile.
- **The aliases in `BuiltinSemantics` are not v1's, and the CLI hides it.**
  `text`, `String`, `bool` and the other Java spellings are canonicalised on
  the way *in* — by the pre-v1 importer (`jdl.rs::normalize_type`) and by the
  compact CLI syntax (`src/model_field_parse.rs::normalize_type`) — while
  `jdl 1` itself matches `BuiltinType::from_token`, which compares the
  canonical token alone. So the two layers disagree, and only a hand-edited
  model shows it:

  ```
  $ jails g record Alias title:String!        # CLI: normalized on the way in
  entity Alias { title: string @notBlank }

  # the same thing typed into .jails/model.jdl:
  title: String @notBlank
  [model-non-blank-type] `non_blank` is valid only for builtin `string` fields
  ```

  `String` is an *external type reference* in v1, so the refusal lands on the
  attribute rather than on the type, one step from the mistake. A field
  carrying no attribute gets no refusal at all. This is the first thing to
  check when a hand-written field "links but is wrong", and §22 is where the
  front ends stop disagreeing.

## How you know you are green

```
cargo test -p jails-model
cargo test --test cli model::
mise run verify-rewrite
```

`the_specification_complete_example_links_except_its_one_recorded_gap` is your
canary: it extracts §4 out of `docs/01-jdl-v1.md` and links it, and it pins
*both* halves -- the rest of the example links, and the one recorded gap still
refuses -- so it cannot go stale in either direction.

---

## A3.13 — three diagnostic vocabularies

96 `JDL0001`–`JDL1002` codes live in `crates/jails-model/src/jdl/v1/`; 148
kebab `model-*` codes live in the linker, which is §18.2's passes 2–9. Below
that, `jails-compiler` and `jails-workspace` return `Result<_, String>` in 80
places and `CompileError` is a newtype over `String` -- a third vocabulary with
no codes at all, and no spans below the parser.

Re-measure with the greps that produced those three, so the next number is
comparable rather than merely different:

```
grep -rho '"JDL[0-9]*"' crates/jails-model/src/jdl/v1/ | sort -u | wc -l
grep -rho '"model-[a-z0-9-]*"' crates/jails-model/src/ | sort -u | wc -l
grep -rho 'Result<[^>]*, *String>' crates/jails-{compiler,workspace}/src | wc -l
```

§18.3 asks for one diagnostic contract. This is the gap.

**Exit:** one diagnostic contract, as §18.3 specifies it. The two coded
vocabularies are not the problem -- the `JDL*` and kebab `model-*` codes are
one boundary apart and both carry spans or paths. The third is the
`Result<_, String>` returns below the linker, with no code and no span, so a
refusal from the compiler and a refusal from the parser are different kinds of
object and only one of them can be pointed at a line.

**Most of the fix is not yours.** Those returns are in `jails-compiler` and
`jails-workspace`, which are B's and C's. What was yours -- the contract they
adopt -- is landed, so a conversion is now a mechanical change in their crate
rather than a design question in yours:

- **The shape** is `jails_model::Diagnostic`, and it is constructible from
  above now. `Diagnostic::new` and `Diagnostics::from_vec` were `pub(crate)`,
  so nothing outside `jails-model` could produce one -- which is the literal
  reason the third vocabulary is strings. `Diagnostic::warning` and
  `Severity` are there for §18.3's severity column.
- **The namespace is closed and gated.** `JDL####` and `model-*` are
  `jails-model`'s, `compile-*` is `jails-compiler`'s, `workspace-*` is
  `jails-workspace`'s, and
  `every_diagnostic_code_belongs_to_the_crate_that_owns_its_phase` fails the
  build when a code escapes its crate. It found two real ones on its first
  run, which is why the workspace prefix is `workspace-` and not the obvious
  `plan-`: `jails-prepare` already spells `plan-refused` in its table of
  command outcomes, and two vocabularies under one prefix is the thing the
  table exists to stop.
- **Below the parser a diagnostic points at a model path, not a source span.**
  The compiler is pure over a `WorkspaceSnapshot` and never sees the reader's
  bytes; carrying a span there would mean threading one through linking for
  every node that might later refuse. The canonical model path the linker
  already uses resolves to a declaration, and where the subject is a file
  rather than a node the path is that file's project-relative path.

**What is left is theirs:** 80 `Result<_, String>` returns, and `CompileError`
as a newtype over `String`. The exit is unchanged.

## A4.4 — three model front ends, and why the simplicity claim is blocked on you

**`docs/00-contracts.md` A4.4 carries the measurement; this section carries
the work.** Both of us re-measured it on 2026-09-01 and agreed on three
numbers and disagreed on two, which is the drift the six-document split
exists to prevent -- so the count lives in one place and the exit condition
lives here. (Its numbers are the right ones: the pre-v1 draft is the whole
pre-v1 half of `jdl/`, `upgrade.rs` included, and the frontend adapters are
the *root binary's*, which is 9,465 rather than the 10,081 a path match on
`/src/model_` sweeps up from other crates.)

Three model front ends are live and editable: `.jails/model.toml`
(`source.rs`), the pre-v1 JDL draft (`jdl.rs` and its children) and `jdl 1`
(`jdl/v1/`). Above them sit the root binary's `model_*` adapters, because
every mutating command is written three times.

`jdl/upgrade.rs` is §22, and it is the only one of these that shrinks the
others: the second front end goes away by *upgrading* projects onto the first,
never by supporting both, because `docs/00-contracts.md` forbids two editable
model sources. That rule is what fixes the order of this work.

**The route exists now.** `jails model upgrade --to 1` moves a project off
`.jails/model.toml`: it links the TOML, renders v1 with `render_jdl_v1`, and
writes `.jails/model.jdl` **while retiring the TOML in the same exact plan** --
because writing one without the other leaves two editable model sources, and a
crash between two plans would leave that state permanently. It is a
`RemoveReaderFile` beside the `ReplaceModelFile` rather than a seventh
operation: a model source that stops being the model is reader-owned source,
and widening the vocabulary for one caller costs every executor and verifier.

Two things it does that a naive version would not, and the second was a real
defect caught by running it:

- **The `db` capability is materialised out loud.** v1 reads `storage
  postgres` as a `db` capability and the TOML `dialect` is not one, so the
  upgrade genuinely gains a JDBC adapter -- which is §22's reason for
  requiring review. `render_jdl_v1` refuses to add it silently; the upgrade
  adds it and prints a note. One mapping, `storage_capability`, read by both.
- **The axes are observed, not defaulted.** `.jails/model.toml` carries
  neither `platform` nor `build`, and `ProjectIntent` defaults them to
  `spring`/`maven` -- so the first version marked a `new-cli` project
  `platform spring`. §22 says these are inspected once and "never guessed",
  so the upgrade reads them the way the JDL path already did.
- **The flat input list becomes parameters, and that is what makes the route
  reach a project with an operation in it at all.** The TOML front end lets an
  operation state its inputs as `fields = ["title"]` with no parameters, and
  `emit_java::input` reads exactly that list when the rich one is empty -- so
  it is the request's whole shape rather than a projection of it, and v1 has
  one spelling, the parameter list. The renderer refused it as
  `$.operations.<label>.fields`; before that refusal existed the round trip
  caught it as an unactionable "does not reproduce its operations", which is
  the same defect wearing a worse message. The parameter is named for the
  field's **Java member**, not its label, because the two paths render
  different component names -- `createdAt` against `created_at` -- and
  `render_parameter` writes the difference back out as `created_at as
  createdAt`, so a caller's request field does not move under it.
- **`projection_for_facet` returns a `ProjectionKind`, not a `use` spelling.**
  Handing back the label made the upgrade map six strings back to the four
  values that can reach it, which needs an arm for a case that cannot happen
  -- a refusal with no next step to name, about a mistake the reader did not
  make. The R3.4 ratchet caught it.

`a_toml_project_upgrades_onto_jdl_v1_and_the_toml_is_retired_with_it` pins
five: the JDL is written, the TOML is retired, every stable id survives, the
axes are the project's, and the command's flat inputs arrive as
`command CreateNote(title)`.
`a_flat_input_list_with_no_parameters_refuses_by_name` pins the renderer's
half, including that the same model *with* parameters renders.

**What is left is deleting the branches.** 31 `is_v1_source` sites, and the
test surface that reaches them: ~43 references to `.jails/model.toml` in
`tests/cli/model.rs`, most of them exercising a mutating command *through* the
TOML front end. Those become `jdl_project` fixtures. Nothing is blocked any
more -- every project can be carried across first.

**Exit:** `.jails/model.toml` and the pre-v1 draft are read only by
`jails model upgrade`, the one-shot that carries a project across;
`is_v1_source` has no callers.

## Open items

**P9.3 §4.2 — slices.** `SliceSpecV1` and `SliceName` exist and nothing reaches
them, while `rename resource` *requires* a `<slice>.<name>` selector. A project
with no slices must keep working unchanged, with the unqualified name meaning
what it means today.

**Most of this is not in your paths.** `SliceSpecV1` and `SliceName` live in
`jails-protocol`, which is C's under the ownership table. The *language* half
is yours, and it is decided:

**A slice is a declaration, not a CLI selector.** §4.2 settles it in one
sentence -- "package layout, ports, migrations, and route prefixes derive from
the slice" -- because a thing that derives names has to be in the model.
`AppModel.derived` records every derived name keyed by owner and role with the
`rule_id` that produced it, and it is recomputed from the model rather than
accumulated, so a slice that moved a package while living only in an argument
vector would make `derived` stop being a function of the model. That is the
same rule that keeps `pinned` from being a flag carried off the source, and it
is what makes `jails model explain` answerable at all. `Billing.Order` at the
CLI is then sugar that resolves to the declaration, which is the shape every
other familiar command already has.

**The implicit slice is derived, never written.** §4.2 also requires that "a
project with no slices must keep working unchanged", and in a language that
means the implicit slice must not appear in the source -- otherwise every
`.jails/model.jdl` in existence changes, and `model fmt --check` fails on all
of them the day it ships. So: one implicit slice, derived when nothing
declares one, and it is what `rename resource` accepts unqualified.

**It is a v2 construct, and §6.2 of `docs/00-contracts.md` lists the price.**
Nine things, not syntax: grammar, typed linked-model payload, validation
schema, stable-ID rule, ownership boundary, formatter behaviour, CLI mapping,
upgrade rule, conformance tests. Two of those are already awkward and worth
knowing before starting -- the stable-ID rule has to say what happens to
`ent_order` when `Order` moves into a slice, and the upgrade rule has to leave
every existing model on the implicit slice without rewriting a byte.

With that settled, C can reach for `SliceSpecV1` knowing what it is holding.

**This is the whole of what is left of §6.1's `generate scaffold` surface.**
`--path` (§6.1 spelled it `--route`), `--index`, `--unique`, `--package` and
`--timestamps` all ship and materialize into the model; `--index` needs a
declared storage, because it is written into the migration. The
`Slice.Entity` selector is the one part of that surface with nothing behind
it.


## The one item you share with B

`A3.15` -- the boundary registry -- lives in `docs/20-generated-java.md`
because most of it is about output names. **The linker half is yours**:
`known_targets` is the set of stable IDs already in the model, so
`eject Task.repo.fake` refuses with `model-ejection-target` and §16.4's
readable boundary path does not resolve. One registry serves both halves; agree
its shape with B before either of you writes it.

**Do not write the linker half alone, and here is the specific reason.** An
ejection has to resolve to the id the compiler actually emits, and those ids
are built by `format!` at the point of use:

```text
§16.4 calls it   Entity.repo.fake
the emitter says art_ent_note_repository_memory
```

The mapping is not mechanical, so a
registry written only in `jails-model` would be a *second* answer to "what is
this artifact called", and the first divergence would be an ejection silently
targeting an artifact no emitter produces. That is the same defect shape as
the two `valid_java_type` copies, which is worth naming because it took a
generated file that could not compile to notice.

`the_specification_complete_example_links_except_its_one_recorded_gap` is
built for this landing: it pins both halves, so when the registry resolves
`eject Task.repo.fake` the second assertion fails and tells you the first can
absorb it.
