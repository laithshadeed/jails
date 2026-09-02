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
and the half of the strangler they still carry: a second project model and
two vocabularies for one test run.

## What you own

`crates/jails-project/**`, `crates/jails-java/**`, `crates/jails-drive/**`,
`crates/jails-report/**`, `crates/jails-workspace/**`,
`crates/jails-support/**` except `codec*`, `crates/jails-codemod/**`,
`crates/jails-contracts/**`. Tests: `tests/cli/{tooling,capabilities,reports,examples}.rs`,
`tests/corpus/**`, `tests/baseline.rs`, `tests/architecture_allowances.rs`.
The `CLAUDE.md` entries for these crates.

## What you do not touch

`jails-spec` is agent 1's; its closed vocabularies have moved into
`jails-model` (`docs/60-abstraction.md` S60.2) and what is left is yours to
*shrink* under S53.8. `jails-model` and `jails-compiler` are not yours;
where a deletion here needs a fact from the compiler you ask agent 5 for the
function and wait.

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

**S53.8 -- Fold the leaves.** `jails-spec` holds where a project is and what
builds it, and nothing else; `jails-java` is the Java reader and the template
renderer, used by `jails-project` and the binary. When S53.5-S53.7 are done,
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

`docs/00-contracts.md` §1.7 row *`Project`/`ProjectContext`/snapshot
overlap*.

## Green

```
cargo test -p jails-project -p jails-drive -p jails-report -p jails-workspace -p jails-support
cargo test --test cli capabilities:: tooling:: reports::
mise run verify-rewrite
```
