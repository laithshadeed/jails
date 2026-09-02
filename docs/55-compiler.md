<!--
One of six. `docs/50-simplify.md` is the brief every agent reads first; it
carries the baseline, the ownership table and rules R1-R9. Nothing here
repeats them.

**A closed item is deleted from this file**, in the commit that closes it.
Item numbers `S55.n` are stable and never reused.
-->

# 55 — Compiler and templates: render the shell once

**Read `docs/50-simplify.md` first.** You are agent 5. Your subject is the
pure compiler -- the ~14,400 production lines that assemble Java and SQL --
and the templates it renders.

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

Re-measured after the last item closed; the method is the one in
`docs/50-simplify.md`, and the board is the authority where the two disagree.

| | at the start | now |
|---|---:|---:|
| `jails-compiler` production / raw | 14,376 / 21,526 | 14,428 / 21,772 |
| `format!(` sites | 834 | 809 |
| `Compiler::compile` | 508 lines, one function | 467 lines, plus three named helpers |
| `templates/` | 142 files | 142 files, still held live by `every_template_is_named_by_a_rust_source` |
| `tests/cli/generate.rs` | 7,715 lines | 7,654 lines, 107 tests |

**Production lines went up, not down, and that is the honest result.** The
compiler ends the pass with one MockMvc dialect where three emitters each had
one, one sampler over `BuiltinSemantics` where four did, one entity sampler
where two did, one dependency guard where eight copies stood, one source-root
walk where two did, and a `Pack` row that carries its own image tags and moved
imports instead of a shared substitution bag. Every one of those is a concept
removed; several cost lines to remove, because the reasoning the copies could
not carry between them now has one place to live. `docs/50-simplify.md` R9
says LOC is not the limiting variable, and this is what that looks like when
it is true.

The orphan count is zero, re-measured from the repository root -- templates
are named through `template!("spring/x.java")` and the like, never by
`include_str!` directly, so the match is on the path or basename:

```
grep -rhoE 'template(_here|_at)?!\([^)]*"[^"]+"' crates/*/src src | grep -oE '"[^"]+"' | tr -d '"' | sort -u > /tmp/refs
for f in $(find templates -type f); do rel=${f#templates/}; b=$(basename $f)
  grep -q "$rel\|$b" /tmp/refs || grep -rq "$b" crates/*/src src || echo "$f"; done
```

## Steps

**None left.** Every `S55.n` item is closed and deleted, per R1; `git log -p --
docs/55-compiler.md` is the record of what each one was and which commit
closed it. What remains below is the standing context for whoever touches the
compiler next, not work.

The plan itself is now retirable, along with its row in `docs/50-simplify.md`'s
five-plan table -- left for whoever owns that file, since it is not this
agent's to edit.

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
