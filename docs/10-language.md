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
| operation statements | 13 | `parse_operation_member`, `jdl/v1/parser/operation.rs` | §12 |
| `ModelPatch` variants | 34 | `patch.rs` | §16 |
| syntax diagnostics (`JDL*`) | 96 | `jdl/v1/` | §18 |
| semantic diagnostics (`model-*`) | 146 | the linker | §18.2 |

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

96 `JDL0001`–`JDL1002` codes live in `crates/jails-model/src/jdl/v1/`; 146
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
`jails-workspace`, which are B's and C's. What is yours is the contract they
would adopt: the `Diagnostic` shape, the code namespace, and whether a
span below the parser is a source span or a model path. Agree it before
anyone converts a crate.

## A4.4 — three model front ends, and why the simplicity claim is blocked on you

Three model front ends are live and editable: `.jails/model.toml`
(`source.rs`), the pre-v1 JDL draft (`jdl.rs` and its `declaration`,
`operation` and `render` children) and `jdl 1` (`jdl/v1/`). Above them sit the
root binary's `model_*` frontend adapters, carrying 31 `is_v1_source` branch
sites, because every mutating command is written three times.

Measured 2026-09-01 with the tree's own `production_lines` -- comments,
string literals and `#[cfg(test)]` modules blanked, blank lines excluded,
which is what `tests/architecture/measure.rs` counts and therefore the only
number the ratchets can be read against:

| | production lines |
|---|---:|
| `source.rs` (the TOML front end) | 537 |
| `jdl.rs` + `declaration`/`operation`/`render` (the pre-v1 draft) | 1,247 |
| `jdl/v1/` (`jdl 1`) | 3,725 |
| `jdl/upgrade.rs` (§22, the path that removes the other two) | 554 |
| root `src/model_*` frontend adapters | 9,957 |

**The earlier figures here were measured another way and are not comparable**
-- they counted raw non-blank lines including tests, which is why `jdl/v1/`
appeared to *shrink* when it grew. Stating the method is the point: a number
whose method is unrecorded cannot be re-measured, only replaced.

This is expected mid-cutover, and **no simplicity claim can be banked until two
of the three are gone**. §22 is the upgrade path that removes them. The rule
that constrains the order is in `docs/00-contracts.md`: two editable model
sources are never permitted, so the second front end goes away by *upgrading*
projects onto the first, never by supporting both.

**Exit:** `.jails/model.toml` and the pre-v1 draft are read by `model import`
and by nothing else; `is_v1_source` has no callers.

## Open items

**P9.3 §4.2 — slices.** `SliceSpecV1` and `SliceName` exist and nothing reaches
them, while `rename resource` *requires* a `<slice>.<name>` selector. A project
with no slices must keep working unchanged, with the unqualified name meaning
what it means today.

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
