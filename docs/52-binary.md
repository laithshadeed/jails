<!--
One of six. `docs/50-simplify.md` is the brief every agent reads first; it
carries the baseline, the ownership table and rules R1-R9. Nothing here
repeats them.

**A closed item is deleted from this file**, in the commit that closes it.
Item numbers `S52.n` are stable and never reused.
-->

# 52 — Binary: one pipeline, no `owns()`

**Read `docs/50-simplify.md` first.** You are agent 2. Your subject is the
root package: 14,852 production lines that turn a parsed command into a
canonical mutation, and do it by repeating one pipeline in sixteen files and
guarding a legacy branch in seven.

## What you own

`src/**` except `src/dispatch.rs` (agent 1) and `src/model_upgrade.rs` (agent
4). `tests/cli/**` except `generate.rs` (agent 5) and `capabilities.rs`,
`tooling.rs`, `reports.rs`, `examples.rs` (agent 3). `docs/feature-inventory.tsv`,
the `Commands` section of `README.md`, and the *Layout* entries of `CLAUDE.md`
that describe `src/`.

## What you do not touch

The crates. If a frontend needs a helper that lives below, the crate owner
adds it; if a frontend calls a symbol another agent deletes, R2 lets them
remove the call. `src/model_command.rs` is yours and it is the file two other
agents will need to touch under R2 (S51.3f, S54.1); keep your edits to it
small and early.

## Baseline

| | |
|---|---:|
| `src/**` production / raw | 14,852 / 20,317 |
| `src/model_*.rs` frontends | 16 files, 8,300 raw |
| `crate::model_generate_jdl::parse(` call sites | 30, in 11 files |
| `owns()` / `owns_at()` / `!owns` switches | 10, in 7 files |
| `refuse_legacy_mutation` calls | 9 |
| `src/new/**` | 2,543 raw |
| `src/cli.rs` + `src/cli/*.rs` | 2,350 raw |
| `schema_command`, `sql_command`, `editor_command`, `contract_command`, `tool_command` | 2,350 raw |
| `tests/cli/model.rs` | 14,392 lines, 159 tests |

```
grep -rn 'model_generate_jdl::parse(' src --include=*.rs | wc -l
grep -rn 'model_command::owns\|owns_at(\|!owns' src --include=*.rs | grep -v 'fn owns' | wc -l
```

## Steps

**S52.1 -- One mutation pipeline.** Every mutating frontend does the same
five things: `read_source_at`, parse and link the current model, edit the
JDL text, parse and link the next model, then `finish_generation` with a
report. Write that once -- a function in `model_command` taking the root, an
edit closure over the source and a report builder -- and make each of
`model_capability`, `model_resource`, `model_field_evolution`, `model_destroy`,
`model_rename`, `model_index`, `model_migration`, `model_setting`,
`model_eject` and `model_generate_jdl` an *edit* plus a *report*. The exit is
the first grep above at three sites or fewer, and no frontend reading a file.
Two things the pipeline must keep, because each was a defect: capture is
taken over the *intended* model (`capture_planned`), and the frontends'
own exact-field-shape checks run before the edit, not after.

**S52.2 -- Delete every `owns()` branch.** Ten sites, seven files. For each,
the other side is a project this binary cannot create:

- `schema_command`: the `!owns()` half of `resource status` reaches
  `lifecycle_status`, which agent 1 deletes (S51.3d). Delete the branch.
- `app.rs`: the legacy backend and the "one transition" prose (about 200
  lines), `refuse_legacy_mutation` and its nine callers. `app apply` is the
  canonical replay; `app init` writes the manifest and keeps its refusal
  *reworded* -- the reason is "one editable source", not "legacy".
- `rename_source`, `new/seed.rs`, `new/plain.rs`, `model_doctor`,
  `model_command`: each branch that asks whether the project is canonical.
  `model_command::owns` itself survives as the one place that answers
  "is there a model here", and only `project_root` calls it.
- The ledger refusal agent 1 hands you (S51.3f): one `is_file` on
  `.jails/ledger.toml` in `project_root`, with a `fix:` naming the commit
  tagged by agent 4 (S54.1) as the last binary that could carry it across.

**S52.3 -- `new` without the legacy write path.** `new/spring.rs` writes three
files through `jails_generate::write_new_file`, which is the last edge from
the binary into `jails-generate`. Those files are template renders into a
reserved `Tree`; import normalisation and `package-info.java` are the
compiler's on every later write, and the three templates already carry sorted
imports. Write them with `tree.put` and drop the dependency. Then the online
and offline bodies in `spring.rs` are the same forty lines but for
`download_starter`; fold them. `gradle_project.rs` (759 raw) is the third
copy of "seed a project and a model": measure what it shares with `plain.rs`
before deciding how much of it is Gradle.

**S52.4 -- `cli.rs`: what the parser accepts that nothing honours.** Read every
arm of `main.rs` for a flag that is parsed and ignored -- `resource repair
--strategy` is one, refused rather than honoured -- and remove it from clap
with its help text. `Command::Add` has two arms for one command; `Declare`
exists to distinguish them. Fold. `feature-inventory.tsv`'s *owner crate* and
*entry point* columns describe crates agent 1 deletes; regenerate the file
from what `main.rs` actually dispatches to, and keep
`every_inventoried_command_path_is_invoked_by_a_test` green while you do.

**S52.5 -- The surfaces with no page.** `Command::Sql`, `Introspect`, `Pull`,
`Schema`, `Editor`, `Contract`, `Request`, `Runner`, `Logs` and
`Architecture` are 2,350 raw lines of frontend over about 4,000 in
`jails-drive` and `jails-project`. Measure each against `README.md`'s
`Commands` section and against `every_advertised_command_path_has_a_journey`.
A command with a section and a journey stays (R7). A command with neither is
**listed for the user in your first pull request** with its line count, and
not touched until they answer. Do not delete a command on your own reading of
whether it is used.

**S52.6 -- `tests/cli/model.rs`.** 14,392 lines is not a test file anyone
reads. Three moves, in order. First, the tests whose subject is
`.jails/model.toml`, the pre-v1 draft or `jails model upgrade` go with agent
4's parsers (S54.1; R2 -- they delete, you review). Second, name the
duplicates: tests that prove one property through one path twice -- a
"refuses X" test per frontend where the refusal now comes from the one
pipeline is the likely shape after S52.1. Third, split what remains by
subject into `tests/cli/model/*.rs` so a reader can find one; a split
changes no line count and is done last so it does not hide the first two.

**S52.7 -- The prose.** `README.md` says `jails model import` exists (it does
not), shows the pre-v1 spelling as "the operational default" and says ordinary
`new` is legacy. The `Commands` section is yours; `Canonical application
compiler` is shared with agent 4 by section. `CLAUDE.md`'s *Layout* entries
for `src/` describe files by their old names (`invoke`, `generate.rs`
dispatch). Rewrite to what is there after S52.1.

## Traps

- **`Invocation` carries the root; the `_at` family is a containment
  boundary.** `jails new` runs in the *parent* of the project it creates, so
  `model_command::root` resolves the wrong directory there. S52.1's pipeline
  takes the root explicitly and does not walk. Do not extend the `_at`
  family downward; make the one pipeline take a root.
- **Capture reads the pre-patch model unless told otherwise.** A test that
  runs two commands and then reads the tree does not catch a frontend that
  forgot `capture_planned`; assert after each command.
- **`--manifest` is resolved absolute; every other model path stays
  project-relative** because the same value becomes a `ProjectPath` in the
  exact plan. The pipeline must preserve that split or every report grows an
  absolute path.
- **Two frontends must not each decide how a request binds.** A query is
  `@ModelAttribute`, a command is `@RequestBody`, decided once in the
  compiler; nothing in `src/` re-derives it (`bugs.md` B48).

## Items you close elsewhere

`docs/00-contracts.md` §1.7's *sixteen copies of one frontend pipeline* row,
once S52.1 and S52.2 land.

## Green

```
cargo test -p jails
cargo test --test cli
mise run verify-rewrite
```
