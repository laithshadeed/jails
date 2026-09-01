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

94 `JDL0001`–`JDL1002` codes live in `crates/jails-model/src/jdl/v1/`; 140
kebab `model-*` codes live in the linker, which is §18.2's passes 2–9. Below
that, `jails-compiler` and `jails-workspace` return `Result<_, String>` in 78
places and `CompileError` is a newtype over `String` -- a third vocabulary with
no codes at all, and no spans below the parser.

§18.3 asks for one diagnostic contract. This is the gap.


**Exit:** one diagnostic contract, as §18.3 specifies it. The two coded
vocabularies are not the problem -- 94 `JDL*` codes and 140 kebab `model-*`
codes are one boundary apart and both carry spans or paths. The third is: 78
`Result<_, String>` returns below the linker, with no code and no span, so a
refusal from the compiler and a refusal from the parser are different kinds of
object and only one of them can be pointed at a line.

## A4.4 — three model front ends, and why the simplicity claim is blocked on you

Three model front ends are live and editable: `.jails/model.toml`
(`source.rs`, 581 lines), the pre-v1 JDL draft (1,912) and `jdl 1` (`jdl/v1/`,
4,955). Above them sit ~6,551 lines of frontend adapters in the root binary
carrying 25 `is_v1_source` branch sites, because every mutating command is
written three times.

This is expected mid-cutover, and **no simplicity claim can be banked until two
of the three are gone**. §22 is the upgrade path that removes them. The rule
that constrains the order is in `docs/00-contracts.md`: two editable model
sources are never permitted, so the second front end goes away by *upgrading*
projects onto the first, never by supporting both.

**Exit:** `.jails/model.toml` and the pre-v1 draft are read by `model import`
and by nothing else; `is_v1_source` has no callers.

## A6.2 — `AppModel::apply` carries what §20.1 splits into passes

```
404  crates/jails-model/src/model_apply.rs:9  AppModel::apply
```

Conceptually present, not separately addressable. Craft debt rather than a
correctness gap, and worth recording because the ratchet that measures module
size can be satisfied by moving it rather than splitting it.

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
