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
second SQL projection, a second field spec, and modules whose only caller is
the kernel agent 1 deletes.

## What you own

`crates/jails-project/**`, `crates/jails-generate/**`, `crates/jails-java/**`,
`crates/jails-drive/**`, `crates/jails-report/**` except the three ledger
readers agent 1 owns (`lifecycle_status.rs`, `managed_drift.rs`, the ledger
half of `schema_lineage.rs`), `crates/jails-workspace/**`,
`crates/jails-support/**` except `codec*`, `crates/jails-codemod/**`,
`crates/jails-contracts/**`. Tests: `tests/cli/{tooling,capabilities,reports,examples}.rs`,
`tests/corpus/**`, `tests/baseline.rs`, `tests/architecture_allowances.rs`.
The `CLAUDE.md` entries for these crates.

## What you do not touch

`jails-spec` is agent 1's until S51.2 lands and then yours to *shrink* under
S53.4; agree the hand-over in the pull request that lands S51.2.
`jails-model` and `jails-compiler` are not yours; where a deletion here needs
a fact from the compiler (S53.3) you ask agent 5 for the function and wait.

## Baseline

| | production | raw |
|---|---:|---:|
| `jails-project` | 8,443 | 14,909 |
| `jails-drive` | 8,265 | 11,463 |
| `jails-report` | 4,352 | 6,627 |
| `jails-workspace` | 4,226 | 6,467 |
| `jails-support` | 3,302 | 6,839 |
| `jails-generate` | 641 | 1,324 |
| `jails-java` | 774 | 1,546 |

`jails-project` modules by production lines and external user (2026-09-02):

| module | lines | used outside the crate by |
|---|---:|---|
| `query_compiler` | 1,078 | drive |
| `inspect` | 778 | root, via `facade.rs` (`routes`, `beans`) |
| `pom` | 697 | root 2, prepare 3, plus facades in drive/report/generate |
| `projection` | 689 | **prepare only** |
| `model` | 648 | root, report, drive, prepare |
| `config` | 600 | root |
| `named_query` | 563 | root |
| `gradle` | 537 | root, report |
| `compose` | 531 | root, drive, prepare |
| `query_workspace` | 453 | root, drive |
| `project` | 378 | root, via facade |
| `modernize` | 341 | root |
| `schema` | 280 | root |
| `application_manifest` | 278 | **nobody** |
| `capture` | 197 | **nobody** |
| `synonyms` | 111 | root |
| `capability` | 98 | **nobody** |
| `properties` | 86 | **nobody** |
| `maven` | 50 | facades |
| `generated_files` | 25 | **nobody** |

"Nobody" was measured by symbol path and misses a facade re-export; confirm
each with `cargo build` after removing the `pub mod`.

## Steps

**S53.1 -- Dead now.** `application_manifest`, `capture`, `capability`,
`properties`, `generated_files`: remove the `pub mod`, build, delete what the
compiler agrees is unreached. Then the same for every `pub fn` in the crate
that only a `#[cfg(test)]` module calls -- `dead_code = "deny"` cannot see
across the crate boundary, so a public function nothing calls is invisible
to it. The tool is `cargo +nightly udeps`-shaped but for functions: make
each `pub` item `pub(crate)` one module at a time and read what fails.

**S53.2 -- Dead after S51.4.** `projection` and `projection/edit` (689),
the intent half of `model` (the `Intent`/`Recorded` values that only
`jails-prepare` consumed), the splice half of `pom` (`add_dependency`,
`add_plugin`, `unsplice`) and of `compose`, and `jails-state`'s `listing`
if anything of it survives. Wait for agent 1's S51.4, then repeat S53.1.

**S53.3 -- One Maven document backend (P13.2).** After S53.2, `pom.rs` is a
reader: `has_dependency` (13 external uses), `Flavor` (10), `read` (7),
`flavor`, `release_level`, `problems`, `main_class`,
`spring_boot_major_of`, and four constants. Every one of those questions is
already a captured fact in `WorkspaceSnapshot.project` on the canonical path.
Move the constants to `jails-spec`, route the questions through the snapshot
where the caller has one, and give the callers that do not (`doctor`, `run`,
`console`) one small reader in `jails-workspace/src/capture/observe.rs`,
which is the surviving parser. Exit: the board's *production files parsing
Maven XML with their own scanner* row reads **1** and its target is reached.
`gradle.rs` is the same shape one build system over and answers
`launches_on` and the wrapper version; leave it, it is the one Gradle reader.

**S53.4 -- One SQL projection, one field spec.** `jails-generate` is 641
lines: `sql` (484) maps the legacy field spec to column types and JDBC
expressions, and `write_new_file` (143) is the legacy write path. The
compiler has its own SQL lowering in `emit_sql.rs` and its own type
semantics in `jails-model`'s `BuiltinSemantics`; two projections of one
type table is the drift `docs/00-contracts.md` §1.7 names first. The
surviving callers are `schema_lineage::columns_from` (expected columns for
`doctor`) and one in the binary. Ask agent 5 for the compiler's answer --
the columns the model's storage lowering would emit for an entity -- and
delete the crate. `write_new_file`'s last caller is agent 2's (S52.3).

Then `jails-spec::spec::field` (809 lines with `kind`): it parses the
compact `name:type[!?]` syntax *and* derives Java and SQL types from it. The
syntax survives -- it is the CLI's -- and the derivation is the compiler's.
Agree with agent 4 whether the parser moves into `jails-model` beside
`BuiltinType::from_alias` (the crate that owns the alias table; recommended,
since the parser's only output is a model field) or stays in `jails-spec`
depending on `jails-model`. Either way the derivation tables and
`Field::java_type`/`sql` go. Exit: one place knows that `text` is `string`.

**S53.5 -- `jails-drive`: two test stacks.** `run/test_plan.rs`,
`run/test_execution.rs`, `testing.rs`, `testing/testd.rs`, `testd.rs`,
`testd/v2.rs` and `launcher.rs` are 2,500 lines describing how `jails test`
selects, partitions and runs tests through three engines. Read them as one
and name what is said twice: the selector vocabulary, the JUnit report
reader, the engine choice. `live_sql.rs` (747), `datasource.rs` and
`console.rs` share one "find the running PostgreSQL" answer -- measure. The
migration-effect replay goes with agent 1 (S51.3b). Do not touch the
resident-JVM classpath split or the `--affected` index; both are measured
and load-bearing.

**S53.6 -- `jails-report`.** `doctor.rs`'s `capability_drift_checks` is one
`Skip` and the plumbing that reaches it; delete both. After agent 1's S51.3,
`doctor` reads no ledger and `why_subject` and `explain` are the two
hand-written tables that stay. `commands.rs` is the oracle and stays.

**S53.7 -- `jails-support`.** After S51.4, `identity` (1,022 lines of
validating newtypes for the legacy protocol) has users in `jails-java`,
`jails-report` and `jails-drive` only. Keep the newtypes those three
construct; delete the rest and the `identity/{sql,route,literal,component}`
tables behind them. `unified` (the bounded diff) has one caller and stays.
`lock` has one after `jails-commit` goes; check `jails-workspace` does not
carry a second.

**S53.8 -- Fold the leaves.** `jails-spec` exists "to keep the ladder
acyclic" and the ladder it kept acyclic is gone; `jails-java` is the Java
reader and the template renderer, used by `jails-project` and the binary;
`jails-state` is deleted. When S53.1-S53.7 are done, measure what each leaf
still exports and fold `jails-java` into `jails-project` if nothing below
`jails-project` needs it. `jails-codemod` stays separate whatever happens: it
is dependency-free so both ladders can reach it. Fewer crates is not the goal
(`docs/00-contracts.md` §6.1); a crate whose reason is gone is.

## Traps

- **A facade re-export keeps a module alive.** `pub(crate) use
  jails_project::{compose, maven, model, pom}` in `jails-drive`'s `lib.rs`
  makes `crate::pom` compile in every module of that crate; the symbol-path
  grep above does not see those uses. Build after every removal.
- **`inspect` is not dead.** `routes` and `beans` reach it through the root
  facade. `project::about` too.
- **The pluraliser has two implementations and a test that holds them equal**
  (`both_pluralizers_answer_the_same_for_every_specified_rule`). When the
  legacy one goes, the test goes with it, not the rule.
- **`gradle.rs`'s bar is answer exactly or refuse.** Nothing here may make it
  guess. A Gradle question the canonical adapter cannot answer is a refusal
  with a `fix:`, not a heuristic.
- **Scanners walk `crates/*/src` and assert a minimum file count.** A crate
  you delete or fold lowers the count; if a gate's floor was set near it, the
  gate reports the tree as lost. Lower the floor in the same change and say
  why.

## Items you close elsewhere

`docs/30-cutover.md` P13.2; `docs/00-contracts.md` §1.7 rows *`jails-spec::Field`
plus protocol `FieldSpec`*, *duplicate Maven XML scanners*, *project
`Project`/`ProjectContext`/snapshot overlap*, *drive/report/tool suites*.

## Green

```
cargo test -p jails-project -p jails-drive -p jails-report -p jails-workspace -p jails-support
cargo test --test cli capabilities:: tooling:: reports::
mise run verify-rewrite
```
