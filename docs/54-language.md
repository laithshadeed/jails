<!--
One of six. `docs/50-simplify.md` is the brief every agent reads first; it
carries the baseline, the ownership table and rules R1-R9. Nothing here
repeats them.

**A closed item is deleted from this file**, in the commit that closes it.
Item numbers `S54.n` are stable and never reused.
-->

# 54 — Language: one front end, one way to change a model

**Read `docs/50-simplify.md` first.** You are agent 4. Your subject is
`jails-model`, the largest crate in the tree at 15,996 production lines, of
which about 3,500 raw lines exist to read two inputs this binary no longer
writes and to carry them across once.

## What you own

`crates/jails-model/**`, `src/model_upgrade.rs` (the frontend that exists only
to call what you delete), `docs/10-language.md`, and §22 of
`docs/01-jdl-v1.md`. `docs/10-language.md`'s open items stay yours and are
not this pass unless listed below.

## What you do not touch

`src/model_generate_jdl*` and the other frontends are agent 2's this time,
because their restructuring (S52.1) is about the binary's pipeline rather
than the language; you supply the `jails-model` functions it needs. The
compiler is agent 5's. Where S53.4 lands the field-syntax parser in your
crate, agent 3 writes the move and you review it.

## Baseline

| | raw lines |
|---|---:|
| `jdl/v1/**` (the `jdl 1` front end) | 5,312 |
| `linker/**` | 3,458 |
| `source.rs` (`.jails/model.toml`) | 622 |
| `jdl.rs` (the pre-v1 draft parser) | 972 |
| `jdl/upgrade.rs` | 979 |
| `jdl/emit/**` (`render_jdl_v1`) | 970 |
| `model_apply.rs` | 736 |
| `patch.rs` (`ModelPatch`, 34 variants) | -- |
| `src/model_upgrade.rs` | 432 |

```
grep -rn 'render_jdl_v1\|projection_for_facet\|storage_capability' src crates/*/src --include=*.rs | grep -v '^crates/jails-model'
grep -rn 'is_jdl_v1\|TOML_PATH' src --include=*.rs
grep -c 'upgrade' tests/cli/model.rs
```

On 2026-09-02 the first prints only `src/model_upgrade.rs`: the renderer has
one caller. The third is 39.

## Steps

**S54.1 -- Delete the compatibility inputs and the upgrade.** The two
parsers were kept "until every project that has one has been carried
across". Jails is not released, the checked-in examples are `app.toml`
manifests, the fixtures were ported in `9afa8ec` and `f1339c7`, and the
tree holds one file that still mentions `.jails/model.toml`. So:

1. Tag the current `main` as the last binary that can carry a project across:
   `git tag last-model-upgrade`, pushed. The ledger refusal agent 2 keeps
   (S52.2) and the `.jails/model.toml` refusal name that tag in their `fix:`.
2. Delete `source.rs`, `jdl.rs`, `jdl/upgrade.rs`, `jdl/emit/**`,
   `src/model_upgrade.rs`, the `Upgrade` arm in `src/cli/model.rs` (R2), the
   `is_jdl_v1` sniff and the `TOML_PATH` refusal in
   `model_command::read_source_at` (R2 -- they shrink to one `fix:` naming
   the tag), and the 39 `upgrade` tests and the `model.toml` tests in
   `tests/cli/model.rs` (R2; agent 2 reviews).
3. `docs/01-jdl-v1.md` §22 (upgrade from the pre-v1 draft) becomes one
   paragraph naming the tag. The `jdl-sol.md §22` citations in source
   comments resolve to that paragraph, which is why it is not deleted whole.
4. `docs/00-contracts.md` A4.4 closes.

**Check before deleting `jdl/emit`** that nothing but the upgrade renders a
model back to JDL: `model init` builds its source by hand (`app_node` in
`new/seed.rs`) and `model fmt` is `jdl/v1/format.rs` over the CST. If a
canonical command ever needs a renderer, it is this one and it comes back
from the tag; today it is 970 lines with one caller.

**S54.2 -- Is there a second way to change a model?** The frontends edit JDL
*text* and re-parse. `ModelPatch` has 34 variants and `model_apply.rs` (736
lines) applies one to a linked model. Measure who constructs a `ModelPatch`
outside `jails-model` after S54.1:

```
grep -rn 'ModelPatch::' src crates/*/src --include=*.rs | grep -v '^crates/jails-model' | grep -v '^\s*//'
```

If the answer is the compiler alone (the `CanonicalModelPatch` a plan
carries), then `model_apply` is the executor's replay path and stays. If a
frontend both edits text *and* builds a patch for the same change, there are
two encodings of one mutation and the text edit is the surviving one --
`docs/00-contracts.md` §1.1 names sixteen shapes of one request as the
original defect. Delete the variants nothing constructs.

**S54.3 -- `jdl/v1`: what the parser says three times.** `parser/declaration.rs`
(540), `parser/operation.rs` (669) and `parser/projection.rs` each carry
attribute parsing, `@id` handling and an unknown-attribute refusal. The
board's largest-module row sits on `parser.rs` at 688 production lines. One
attribute reader shared by the three, taking the closed set it accepts, is the
shape; measure the duplication first by counting the `reject_unknown`-style
sites and the `@id(` handling. `edit.rs` (463) is the syntax editor agent 2's
pipeline will call more uniformly; keep its byte-preserving contract.

**S54.4 -- The linker's two vocabularies.** 148 `model-*` codes in the linker
and 96 `JDL*` codes in the parser, each a literal at its site. Not a line
reduction, and not this pass, unless S54.3 shows the same refusal spelled at
several sites; then one constructor per code and the table stays where
`every_diagnostic_code_belongs_to_the_crate_that_owns_its_phase` can see it.

**S54.5 -- The field-syntax parser lands here (with agent 3, S53.4).** When
it does, `BuiltinType::from_alias` is the one alias table and the parser's
output is a model field, not a `Field` with derived Java and SQL. Delete the
`builtin_by_java_name` re-export path the moment it is.

**S54.6 -- `docs/10-language.md`.** The registry table's rows for the
compatibility spellings, the *A3.13* paragraph's claim that the third
vocabulary is theirs (it is agent 3's and 5's `Result<_, String>` returns;
leave the exit, delete the narrative), and the §22 reference in *The
specification sections this work answers to*.

## Traps

- **Both dialects state field order and `FieldPlacement` has to agree with
  re-parsing.** A record's positional constructor is ABI. When the TOML
  source goes, the sort-by-label branch in `patch.rs` goes with it -- read
  the comment there before deleting, because the heuristic it records is the
  one that produced a silently wrong argument list once.
- **`derived` is recomputed, never accumulated**, and `pinned` is decided by
  comparison with the convention. Anything in S54.2 that carries a flag off
  the source breaks `jails model explain`.
- **`the_specification_complete_example_links_except_its_one_recorded_gap`
  pins both halves of §4.** It must keep passing through every step, and the
  one recorded gap (`eject Task.repo.fake`) stays recorded -- A3.15 is not
  this pass.
- **A count with no method is a count that cannot be re-measured.** The
  registry table in `docs/10-language.md` carries its greps; keep them
  beside any number you change.

## Items you close elsewhere

`docs/00-contracts.md` A4.4; `docs/01-jdl-v1.md` §22 (reduced to the tag
paragraph); `docs/30-cutover.md` *Two editable model sources are never
permitted* (delete; with one parser there is nothing to permit).

## Green

```
cargo test -p jails-model
cargo test --test cli model::
mise run verify-rewrite
```
