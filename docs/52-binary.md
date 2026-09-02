<!--
One of six. `docs/50-simplify.md` is the brief every agent reads first; it
carries the baseline, the ownership table and rules R1-R9. Nothing here
repeats them.

**A closed item is deleted from this file**, in the commit that closes it.
Item numbers `S52.n` are stable and never reused.
-->

# 52 — Binary: one decision per mutation

**Read `docs/50-simplify.md` first.** You are agent 2. Your subject is the
root package: 13,071 production lines that turn a parsed command into a
canonical mutation.

The pipeline is one place now, and one decision. `model_command::Current::load`
reads and links the invocation's model; a frontend edits the JDL text and
names an `Evolution`; `model_generate::finish_generation` links the edited
source, captures over that model, compiles it beside the evolution,
materializes, and previews or executes *that* bundle. The plan's input bytes
are the evolution serialised; the edit is in the plan as the model file's
after-image.

## What you own

`src/**` except `src/dispatch.rs` (agent 1). `tests/cli/**` except
`generate.rs` (agent 5) and `capabilities.rs`, `tooling.rs`, `reports.rs`,
`examples.rs` (agent 3). `docs/feature-inventory.tsv`, the `Commands`
section of `README.md`, and the *Layout* entries of `CLAUDE.md` that
describe `src/`.

## What you do not touch

The crates. If a frontend needs a helper that lives below, the crate owner
adds it; if a frontend calls a symbol another agent deletes, R2 lets them
remove the call. `src/model_command.rs` is yours and it is the file other
agents will need to touch under R2; keep your edits to it small and early.

## Baseline

| | |
|---|---:|
| `src/**` production / raw | 13,071 / 17,988 |
| `src/model_*.rs` frontends | 16 files |
| re-parses of an edited source, each a check rather than a second decision | 9 |
| `src/new/**` raw | 2,543 |
| `src/cli.rs` + `src/cli/*.rs` raw | 2,350 |
| `editor_command`, `contract_command`, `tool_command` raw | 1,400 |
| `tests/cli/model.rs` | 13,765 lines |

```
grep -rn 'parse(&next\|parse(&requested' src --include=*.rs | wc -l
```

## Steps

**S52.3 -- `new`'s three seeds.** `src/new/write.rs` writes `App.java`, its
test and a `package-info.java` by hand before the model takes over. The
online and offline bodies in `spring.rs` are the same forty lines but for
`download_starter`; fold them. `gradle_project.rs` (759 raw) is the third
copy of "seed a project and a model": measure what it shares with `plain.rs`
before deciding how much of it is Gradle.

**S52.5 -- The surfaces with no page.** `contract`, `editor`, `request`,
`runner` and `setup` have inventory rows and journeys and no `jails <name>`
in `README.md`'s `Commands` section (`editor` is what `jails.nvim` drives).
A command with a section and a journey stays (R7). These five are **listed
for the user**; do not delete a command on your own reading of whether it is
used. When the answer comes back, either write the section or remove the
command with its inventory row and journey.

**S52.6 -- `tests/cli/model.rs`.** 13,800 lines is not a test file anyone
reads. Two moves, in order. First, name the duplicates: tests that prove one
property through one path twice -- a "refuses X" test per frontend where the
refusal now comes from the one pipeline is the likely shape. Second, split
what remains by subject into `tests/cli/model/*.rs` so a reader can find
one; a split changes no line count and is done last so it does not hide the
first.

**S52.7 -- The prose.** `README.md`'s `Commands` section is yours; rewrite
it to what is there after S52.5.

## Traps

- **`Invocation` carries the root, and every model function takes it or the
  root it resolved.** `jails new` runs in the *parent* of the project it
  creates, so a walk from the process directory resolves the wrong directory
  there; `Invocation::root` is the one place the walk happens, and
  `Invocation::for_new` pins the root instead. A function that walks on its
  own is the defect, not a convenience.
- **Capture reads the model on disk unless handed the intended one.** A test
  that runs two commands and then reads the tree does not catch a frontend
  that forgot to pass `intended`; assert after each command.
- **`--manifest` is resolved absolute; every other model path stays
  project-relative** because the same value becomes a `ProjectPath` in the
  exact plan. The pipeline must preserve that split or every report grows an
  absolute path.
- **Two frontends must not each decide how a request binds.** A query is
  `@ModelAttribute`, a command is `@RequestBody`, decided once in the
  compiler; nothing in `src/` re-derives it.

## Green

```
cargo test -p jails
cargo test --test cli
mise run verify-rewrite
```
