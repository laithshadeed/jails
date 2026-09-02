<!--
One of six. `docs/50-simplify.md` is the brief every agent reads first; it
carries the baseline, the ownership table and rules R1-R9. Nothing here
repeats them.

**A closed item is deleted from this file**, in the commit that closes it.
Item numbers `S54.n` are stable and never reused.
-->

# 54 — Language: one front end, one way to change a model

**Read `docs/50-simplify.md` first.** You are agent 4. Your subject is
`jails-model`: the `jdl 1` front end and the linker. There is one way to
change a model now -- edit the source -- and `Evolution` carries what the
source cannot say.

## What you own

`crates/jails-model/**` and `docs/10-language.md`. `docs/10-language.md`'s open items stay yours and are
not this pass unless listed below.

## What you do not touch

`src/model_generate_jdl*` and the other frontends are agent 2's this time,
because their restructuring is about the binary's pipeline rather than the
language; you supply the `jails-model` functions it needs. The
compiler is agent 5's. Where S53.4 lands the field-syntax parser in your
crate, agent 3 writes the move and you review it.

## Baseline

| | raw lines |
|---|---:|
| `jdl/v1/**` (the `jdl 1` front end) | 5,312 |
| `linker/**` | 3,458 |

## Steps

**S54.5 -- The field-syntax parser.** `src/model_field_parse.rs` produces a
model field and `BuiltinType::from_alias` is the one alias table; if the
parser moves into this crate (`docs/60-abstraction.md` S60.2), it moves as
is.

## Traps

- **`derived` is recomputed, never accumulated**, and `pinned` is decided by
  comparison with the convention. Anything that carries a flag off the
  source breaks `jails model explain`.
- **`the_specification_complete_example_links_except_its_one_recorded_gap`
  pins both halves of §4.** It must keep passing through every step, and the
  one recorded gap (`eject Task.repo.fake`) stays recorded -- A3.15 is not
  this pass.
- **A count with no method is a count that cannot be re-measured.** The
  registry table in `docs/10-language.md` carries its greps; keep them
  beside any number you change.

## Green

```
cargo test -p jails-model
cargo test --test cli model::
mise run verify-rewrite
```
