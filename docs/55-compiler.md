<!--
One of six. `docs/50-simplify.md` is the brief every agent reads first; it
carries the baseline, the ownership table and rules R1-R9. Nothing here
repeats them.

**A closed item is deleted from this file**, in the commit that closes it.
Item numbers `S55.n` are stable and never reused.
-->

# 55 — Compiler and templates: render the shell once

**Read `docs/50-simplify.md` first.** You are agent 5. Your subject is the
pure compiler -- 14,376 production lines that assemble Java and SQL in 834
`format!(` sites -- and the templates it renders.

## What you own

`crates/jails-compiler/**`, `templates/**`, `tests/golden/**`, `tests/golden.rs`,
`tests/agreement.rs`, `tests/cli/generate.rs`, `docs/20-generated-java.md`.
`tests/common/scenarios.rs` stays append-only for everyone, you included.

## What you do not touch

`jails-model` is agent 4's: an emitter reading a new model field is your
change, adding the field is theirs and lands first. `jails-workspace` is agent
3's: the compiler may not observe the filesystem, and a fact you need is a
capture change they make. `templates/new/**` is rendered by the binary (agent
2); leave it.

## Baseline

| | |
|---|---:|
| `jails-compiler` production / raw | 14,376 / 21,526 |
| `format!(` sites | 834 |
| of which `emit_unit.rs` / `emit_sql.rs` / `emit_operation/proof.rs` | 67 / 46 / 40 |
| `Compiler::compile` | 508 lines, one function |
| `templates/` | 142 files, held live by `every_template_is_named_by_a_rust_source` |
| `tests/cli/generate.rs` | 7,715 lines |

The orphan count is zero, re-measured from the repository root -- templates
are named through `template!("spring/x.java")` and the like, never by
`include_str!` directly, so the match is on the path or basename:

```
grep -rhoE 'template(_here|_at)?!\([^)]*"[^"]+"' crates/*/src src | grep -oE '"[^"]+"' | tr -d '"' | sort -u > /tmp/refs
for f in $(find templates -type f); do rel=${f#templates/}; b=$(basename $f)
  grep -q "$rel\|$b" /tmp/refs || grep -rq "$b" crates/*/src src || echo "$f"; done
```

## Steps

**S55.7 -- `tests/cli/generate.rs`.** 7,948 lines and 110 tests, most of
them "generate X, then read a file". After S55.2 the assertions about the
package line and the import block are one property, not a hundred; name the
duplicates and delete them (R6). The real-toolchain tier is the oracle here
and none of it goes.

## Traps

- **Templates are `.java` files with `{{name}}` substitution, never
  `format!`.** `format!` renders `{{` as `{`, which is how an extraction of
  the alert rules' PromQL once changed them silently. `templates/add/**` are
  substituted with `str::replace` because GitHub and Docker use `{{` and
  `${{` themselves; keep that split.
- **Goldens compare bytes and never run the code.** The oracle is a
  generated project that compiles and passes its own tests, under
  `JAILS_TOOLCHAIN=1`. Reproduce every item from a clean `jails new` and
  state the command.
- **Three of the last four generators needed no emitter**; a construct that
  only wants syntax in front of an existing backend is agent 4's, not yours.
- **`{{` appears in generated `.http` files** as the HTTP Client's own
  variable syntax; those are built with `format!` and escaped `{{{{`
  deliberately. Check `.java` alone if S55.2 touches the renderer.
- **The compiler may not read the disk.** `canonical_compiler_is_pure_after_capture`
  holds it, and the crate depends on nothing that can.

## Green

```
cargo test -p jails-compiler
UPDATE_GOLDEN=1 cargo test --test golden   # then read the diff
JAILS_TOOLCHAIN=1 cargo test --test cli generate::
mise run verify-rewrite
```
