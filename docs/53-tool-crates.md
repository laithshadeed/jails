<!--
One of six. `docs/50-simplify.md` is the brief every agent reads first; it
carries the baseline, the ownership table and rules R1-R9. Nothing here
repeats them.

**A closed item is deleted from this file**, in the commit that closes it.
Item numbers `S53.n` are stable and never reused.
-->

# 53 — Tool crates: one answer per question

**Read `docs/50-simplify.md` first.** You are agent 3. Your subject is the
crates that outlive the cutover -- the reader-owned files, the Java reader,
the commands that start something and the commands that answer a question --
and the half of the strangler they still carry: a second Maven parser, a
second project model, and two vocabularies for one test run.

## What you own

`crates/jails-project/**`, `crates/jails-java/**`, `crates/jails-drive/**`,
`crates/jails-report/**`, `crates/jails-workspace/**`,
`crates/jails-support/**` except `codec*`, `crates/jails-codemod/**`,
`crates/jails-contracts/**`. Tests: `tests/cli/{tooling,capabilities,reports,examples}.rs`,
`tests/corpus/**`, `tests/baseline.rs`, `tests/architecture_allowances.rs`.
The `CLAUDE.md` entries for these crates.

## What you do not touch

`jails-spec` is agent 1's; when its closed vocabularies move into
`jails-model` (`docs/60-abstraction.md` S60.2) what is left is yours to
*shrink* under S53.4. `jails-model` and `jails-compiler` are not yours;
where a deletion here needs a fact from the compiler (S53.3) you ask agent 5
for the function and wait.

## Baseline

| | production | raw |
|---|---:|---:|
| `jails-drive` | 7,186 | 10,165 |
| `jails-workspace` | 4,208 | 6,405 |
| `jails-project` | 3,932 | 7,285 |
| `jails-report` | 3,403 | 5,376 |
| `jails-support` | 2,569 | 5,388 |
| `jails-java` | 752 | 1,505 |

Re-measure with the method in `docs/50-simplify.md` before quoting.

## Steps

**S53.3 -- One Maven document backend (P13.2).** `pom.rs` is a reader:
`has_dependency`, `Flavor`, `read`, `flavor`, `release_level`, `problems`,
`main_class`, `spring_boot_major_of`, and four constants. Every one of those
questions is already a captured fact in `WorkspaceSnapshot.project` on the
canonical path. Move the constants to `jails-spec`, route the questions
through the snapshot where the caller has one, and give the callers that do
not (`doctor`, `run`, `console`) one small reader in
`jails-workspace/src/capture/observe.rs`, which is the surviving parser.
Exit: the board's *production files parsing Maven XML with their own
scanner* row reads **1** and its target is reached. `gradle.rs` is the same
shape over one build system and answers `launches_on` and the wrapper
version; leave it, it is the one Gradle reader.

**S53.4 -- One field-syntax parser.** `src/model_field_parse.rs` is the one
parser of `name:type[!?]` and `BuiltinType::from_alias` the one alias table.
Keep it that way: a second parser is the repository's most reliable drift
generator.

**S53.5 -- `jails-drive`: two test stacks.** `run/test_plan.rs`,
`run/test_execution.rs`, `testing.rs`, `testing/testd.rs`, `testd.rs`,
`testd/v2.rs` and `launcher.rs` are 2,500 lines describing how `jails test`
selects, partitions and runs tests through three engines. Read them as one
and name what is said twice: the selector vocabulary, the JUnit report
reader, the engine choice. Do not touch the
resident-JVM classpath split or the `--affected` index; both are measured
and load-bearing. Agent 1's S51.4 waits on this.

**S53.7 -- `jails-support`.** `identity` keeps the newtypes `jails-java`,
`jails-report`, `jails-drive` and `jails-spec` construct and nothing else.
`unified` (the bounded diff) has one caller and stays. `lock` has one;
check `jails-workspace` does not carry a second. `capture_import` is the one
remaining capture variant: it differs from `capture` in one precondition
(the model must *not* exist yet) and has one caller, `model init`; make the
precondition an argument if a second caller appears, not a fifth function.

**S53.8 -- Fold the leaves.** `jails-spec` holds the closed CLI vocabularies
and where a project is; `jails-java` is the Java reader and the template
renderer, used by `jails-project` and the binary. When S53.3-S53.7 are done,
measure what each leaf still exports and fold `jails-java` into
`jails-project` if nothing below `jails-project` needs it. `jails-codemod`
stays separate whatever happens: it is dependency-free so both ladders can
reach it. Fewer crates is not the goal (`docs/00-contracts.md` §6.1); a crate
whose reason is gone is.

## Traps

- **A facade re-export keeps a module alive.** `pub(crate) use
  jails_project::{compose, maven, model, pom}` in `jails-drive`'s `lib.rs`
  makes `crate::pom` compile in every module of that crate; a symbol-path
  grep does not see those uses. Build after every removal.
- **`dead_code = "deny"` cannot see across the crate boundary.** A `pub`
  item nothing calls is invisible to it; the way to find one is to count its
  mentions outside its definition across `src`, `crates` and `tests` with
  comments stripped, and read what comes back at one.
- **`inspect` is not dead.** `routes` and `beans` reach it through the root
  facade. `project::about` too.
- **`gradle.rs`'s bar is answer exactly or refuse.** Nothing here may make it
  guess. A Gradle question the canonical adapter cannot answer is a refusal
  with a `fix:`, not a heuristic.
- **Scanners walk `crates/*/src` and assert a minimum file count.** A crate
  you delete or fold lowers the count; if a gate's floor was set near it, the
  gate reports the tree as lost. Lower the floor in the same change and say
  why.

## Items you close elsewhere

`docs/30-cutover.md` P13.2; `docs/00-contracts.md` §1.7 rows *duplicate
Maven XML scanners* and *`Project`/`ProjectContext`/snapshot overlap*.

## Green

```
cargo test -p jails-project -p jails-drive -p jails-report -p jails-workspace -p jails-support
cargo test --test cli capabilities:: tooling:: reports::
mise run verify-rewrite
```
