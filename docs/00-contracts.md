<!--
`docs/00-contracts.md` is the one every reader starts from; it carries the
contracts, the deletion map and the ownership rules that keep the other
documents from contradicting each other.

**A closed item is deleted from the file that holds it**, in the commit that
closes it -- never marked done. `git log -p -- docs/` is the record.

**Item and section numbers are stable and never reused.** A section with no
open items disappears rather than being renumbered.

Everything here is written in the present tense as a rule. A measurement is
dated and carries its method.
-->

# 00 — The contracts, and how the documents are used

Jails is a compiler. Its source is one application model; its generated tree is
merge-managed by stable artifact identity; its irreversible output is an
explicit evolution plan; and its executor applies exactly the plan the reader
reviewed.

That sentence is the whole design. These documents are what it means, and what
has not been done.

| file | what it is | who reads it |
|---|---|---|
| `docs/00-contracts.md` | this file: the decision, the five contracts, the deletion map, the fitness rules, the non-goals | **everyone, first** |
| `docs/01-jdl-v1.md` | JDL v1, normative; its section numbers are the ones source comments cite as `JDL v1 §N` | whoever needs a section |
| `docs/10-language.md` | workstream A: the model, the grammar, the linker, diagnostics | one agent |
| `docs/20-generated-java.md` | workstream B: what the compiler emits, and how good it is | one agent |
| `docs/30-cutover.md` | workstream C: the workspace, the project crates, adoption | one agent |
| `docs/40-gates-and-ci.md` | workstream D: the gates, the suite, and the CI budget | one agent |
| `docs/50-simplify.md` | the simplification pass: brief, baseline, ownership for five agents | everyone, during the pass |
| `docs/51-kernel.md` .. `docs/55-compiler.md` | the five plans of that pass | one agent each |
| `docs/60-abstraction.md` | the target shape the five plans converge on: five nouns, four verbs | everyone, during the pass |

The split exists so several agents can work at once without reading each
other's context or editing each other's files. Everything genuinely shared is
in this file, once.

## Working the plan

**Reproduce an item from a clean `jails new` before believing it**, and state
the command that produced it. Goldens compare bytes and never run the code, so
the oracle that finds this class of defect is a real build.

`mise run verify-rewrite` is the single answer to "is this green". Run it
before every push; `.githooks/pre-push` and CI invoke it and nothing else.

Close an item by **deleting** it, in the commit that closes it.

## Who owns which paths

**While the simplification pass runs, `docs/50-simplify.md`'s ownership table
is the one in force**, and its rules R1 to R9 add to the resolution rules
below. When the pass ends this table returns to four workstreams over whatever
is left:

| workstream | file | owns |
|---|---|---|
| **A — language** | `docs/10-language.md` | `crates/jails-model/**`, the JDL front ends in `src/model_generate_jdl*` and `src/model_jdl_edit.rs`, `src/model_explain.rs` |
| **B — generated Java** | `docs/20-generated-java.md` | `crates/jails-compiler/**`, `templates/**` |
| **C — workspace and project** | `docs/30-cutover.md` | `crates/jails-workspace/**`, `crates/jails-project/**`, `crates/jails-{drive,report,java}/**`, `src/new.rs`, `src/app.rs`, `src/dispatch.rs` |
| **D — gates and CI** | `docs/40-gates-and-ci.md` | `.github/**`, `.githooks/**`, `mise.toml`, `scripts/**`, `tests/common/**`, `tests/architecture/**`, `tests/corpus/**` |

### The files everyone touches

Four things are shared by construction, and each has a resolution rule:

- **`tests/golden/**`** -- regenerated, never hand-edited. A conflict is
  resolved by taking either side and re-running `UPDATE_GOLDEN=1 cargo test
  --test golden`, then **reading the diff**: a golden that changes for a reason
  you cannot state is the finding, not the noise.
- **`tests/architecture/board.rs`** -- one row per ratchet, each carrying the
  reason its ceiling has its value. A conflict keeps both notes and
  re-measures; the recorded ceiling is whatever the merged tree reports.
- **`LAYERS` in `tests/architecture/rules.rs`** -- one row per module. Keep
  both sides and dedupe; `layers_lists_each_module_once` fails on a duplicate.
- **`tests/common/scenarios.rs`** -- append only. A new kind or capability adds
  a `Scenario`; nothing else in the table moves.

`CLAUDE.md`, `ARCHITECTURE.md` and `README.md` are edited by whoever's change
makes them wrong, in the same commit, and only in the section the work is
about.

## Item identifiers

Open items carry stable identifiers -- `P<phase>.<item>`, `A<section>.<item>`,
`S<plan>.<item>`, `G<n>`, `B<n>` -- and an identifier is never renumbered or
reused. Read `<id>` as "the entry with that id" and find it in the workstream
file that owns the subject; when the entry is gone, the item is closed and
`git log -p -- docs/` holds it.

---

# Part 1 — The decision

## 1.1 The problem the compiler solves

Four things make a generator hard to change, and none of them is volume of
code.

**Product breadth is real and is not the problem.** Thirty-nine generator
kinds and twenty-five capabilities are a legitimate surface.

**The editable-output paradox is the first real cause.** Generated code is
meant to be edited, and jails is meant to regenerate it. That is only
answerable with a merge base; without one every generator grows its own theory
of "is this file still ours".

**No canonical semantic world is the second, and the deepest.** A generator
that recovers requirements from the bytes it already emitted makes source the
database, so every question has two answers and nothing can say which is
stale.

**One intent copied through many representations is the third.** A request
that exists as a dozen shapes has a dozen agreement methods.

## 1.2 The product choice

| | shape | why not |
|---|---|---|
| A | honest one-shot generator | throws away the iterative loop, which is the product |
| **B** | **merge-managed compiler with implementation-boundary ejection** | **chosen** |
| C | disposable managed tree with ejection | a tree nobody may edit is a tree nobody trusts |
| D | JVM build-time compiler | moves the problem into a build plugin and a second language |

**D1 -- the iterative edit loop is the product.** A reader adds a method to a
generated record, edits a validation message jails wrote, and edits the exact
line jails rewrites; all three must survive regeneration, and the third must
refuse rather than guess. This overrides any simplification that trades it
away.

**D2 -- JDL is a required deliverable.** Without a durable authoring source,
the model is a transcript of CLI invocations and cannot be reviewed, diffed or
hand-edited.

**D3 -- ergonomics are a requirement.** A refusal that does not say what to do
next is a defect, and every refusal carries a `fix:` line.

## 1.3 The five contracts

- **`AppModel` is desired-state authority.** Stable IDs carry identity; Java,
  SQL, route and configuration names are projections of it.
- **`WorkspaceSnapshot` captures every external fact once.** Code below the
  compiler may observe the filesystem; the compiler may not.
- **`Compiler` is pure.** Equal snapshot, model, evolution and compiler version produce
  equal desired artifacts.
- **`PlanBundle` is the exact reviewed transition.** Preview, export,
  confirmation and apply all refer to its digest; apply never replans.
- **`jails-workspace::execute` is the only project writer.** It locks, rechecks
  preconditions, publishes exact after-images, and converges on retry.

## 1.4 The pipeline

```text
.jails/model.jdl / CLI sugar
        -> edited JDL source (+ Evolution)
        -> AppModel + WorkspaceSnapshot
        -> pure Compiler
        -> PlanDraft
        -> exact content-addressed PlanBundle
        -> preview or the one Executor
```

The passes, in the order JDL v1 §20.1 specifies them: capture; edit the
source and link it; resolve and validate; normalize facets and derive
the dependency graph; lower to typed artifact IR; derive schema and evolution;
emit a draft, then materialize one exact plan.

## 1.5 The crates

Lowest first. A crate may only depend on one below it, and Cargo enforces that;
`no_module_depends_on_a_layer_above_its_own` enforces the same rule for
module-level edges, and `LAYERS` in `tests/architecture/rules.rs` is the
authority on which crate a module belongs to.

| crate | contract |
|---|---|
| `jails-model` | closed source schema, stable IDs, linking, semantic diagnostics, `AppModel` and `Evolution` |
| `jails-contracts` | portable `WorkspaceSnapshot`, `PlanDraft`, exact `Plan`, operations, trees and blobs |
| `jails-compiler` | pure semantic lowering to a desired artifact tree; no filesystem, environment or subprocess access |
| `jails-workspace` | capture, exact materialization, verification and the single executor |

`jails-codemod` (the marked block, no dependencies) and `jails-support` (write,
run, encode, name) are the leaves beside them; `jails-project`, `jails-drive`
and `jails-report` are the tool crates above; the binary is the root package.

## 1.6 Managed output, ejection, and what stays irreproducible

Reproducible output lives below `.jails/generated` and is merge-managed. The
accepted model renders BASE, capture supplies OURS, and the next model renders
THEIRS. Clean merges are frozen into the plan; conflicts refuse without writes.
The lock advances to THEIRS so hand edits remain deltas.

Migrations, model revisions and explicit reader-file patches are
**irreproducible** operations and stay visible in the plan rather than being
smuggled into rendering.

`model eject <artifact-id>` transfers one ejectable adapter implementation into
reader source, records the transfer, and excludes that artifact from later
managed trees. Records and ports remain managed ABI. Capture includes every
prospective reader destination, collision refuses, and ejection never infers
ownership from edited bytes or silently reclaims it. Its before-image is
`Missing`: transfer is creation of a new reader-owned source, never
reconciliation with an existing one.

## 1.7 The deletion map

What the simplification pass deletes, and what replaces it. A plan is measured
against this list.

| area | destination | deletion |
|---|---|---|
| `jails-spec::Field` plus protocol `FieldSpec` | one field-syntax parser producing model fields; `BuiltinSemantics` as the one type table | the derivation tables |
| generated Java/SQL reparsing | `AppModel` and the snapshot | source-as-database paths |
| `#[derive(Codec)]` on the test-execution wire | one `serde` protocol (S60.6) | the codec and its derive crate |
| `Project`/`ProjectContext`/snapshot overlap | a snapshot-backed project view | post-capture disk reads |
| duplicate Maven XML scanners | one document backend in `jails-workspace` | `pom.rs`'s scanner |

The largest deletion does not come from shorter render functions. It comes
from making six questions disappear: which representation is authoritative;
what facts can be recovered from the Java we emitted; which recipe kinds
depend on this field; is this edited file still ours; did preview and apply
run the same computation; does recovery reproduce every side condition of a
normal commit. One model, one graph, one plan and one explicit ownership
transfer answer all six.

## 1.8 Architecture fitness rules

Properties that express the shape. **Not** file-size thresholds -- those can
be satisfied while every duplicated concept survives.

| rule | held by |
|---|---|
| after capture, no compiler module reaches `std::fs`, a project root or a process runner | `rules::canonical_compiler_is_pure_after_capture`, and structurally: the compiler crates depend on nothing that can read a disk |
| identical snapshot + evolution + compiler version yields an identical plan digest | `materialize::the_same_snapshot_patch_and_compiler_produce_the_same_plan_digest`, plus the two negatives |
| preview, export, confirmation and apply reference one digest | `preview_export_and_apply_all_name_one_plan_digest` |
| every command and alias resolves through one generated catalog | `commands.rs` walks the live `clap::Command`; `every_command_a_message_tells_the_reader_to_run_is_one_that_exists` checks messages against it |
| every builtin type has one semantics row | `BuiltinSemantics` is one exhaustive match, held by the *largest table of per-builtin knowledge outside its row* ratchet |
| every artifact requirement comes from IR, never a content or path scan | structural, with one recorded exception: `emit_component/cli.rs` reads captured `App.java` and `<mainClass>` to decide whether to retarget the jar (`docs/55-compiler.md` S55.8) |
| managed output is written only below the managed root | `execute`'s precondition check |
| reader-owned source changes only by an explicit typed patch/eject/adopt operation | `PatchReaderFile` with a captured before-image |
| every advertised failpoint fires in at least one test | `failpoints!` emits both the registry and the constants, so an unfired point is `-D dead-code` |
| every active transaction state has one tested recovery transition | `crates/jails-workspace/tests/crash.rs` |
| the planner's read set is complete by construction | structural, via `WorkspaceSnapshot` |
| optional tool crates cannot import mutation executor internals | Cargo, plus `LAYERS` |

## 1.9 Coverage

| | |
|---|---|
| generators on the compiler | 39 of 39 |
| capabilities | 25 of 25 |
| component kinds with a backend | 23 of 23 |

Each is held by an exhaustive match over a `clap::ValueEnum`, so a kind added
without a backend fails to compile.

---

# Part 6 — Deliberate non-goals

## 6.1 What will not simplify jails by itself

- **Minijinja or another template engine.** The templates are real `.java`
  files with `{{name}}` substitution and no logic.
- **A three-crate rewrite.** Fewer crates is not fewer concepts.
- **"Render in a temp directory and atomically swap it."** It cannot preserve a
  reader's edits, which is D1.
- **A fully dynamic runtime schema.** The closed vocabularies are what make an
  unknown declaration an error rather than a silent no-op.
- **A full Java compiler in Rust.** The readers stay small.
- **Deleting the merge base because generated output is reproducible.**
  Reproducible output does not make a half-applied transition safe.
- **LOC as the limiting variable.** A gate on file length can be satisfied
  while every duplicated concept survives.

## 6.2 What JDL v1 will never have

Includes, imports, macros, templates, conditional declarations; environment
overlays or secrets; arbitrary Java annotations, Java expressions, SQL
expressions or build XML; per-declaration packages, configurable
suffixes/plurals, route styles, test templates or migration names; implicit
many-to-many relations or ORM navigation collections; migration history in the
desired-state file; plugin-defined unnamespaced keywords; automatic reverse
adoption of ejected code; multi-file partial declaration merging.

**No second spelling of a fact the language already states.** `--timestamps`
writes `@default(now())` and `@updated`; `n:int=0` is `@default(0)`;
`--with-events` is `jails g event`; `status:enum.PENDING.PAID` is `jails g
enum` plus `status:Status`. Two ways to say one thing is a drift generator.

**A construct is added only with its grammar, typed linked-model payload,
validation schema, stable-ID rule, ownership boundary, formatter behavior, CLI
mapping, upgrade rule and conformance tests. Syntax alone is not a language
feature.**

## 6.3 The product scope bar

No ORM, no jails runtime jar, no Lombok, no preview features in generated Java,
and no plugin system with lifecycle hooks. `README.md`'s "Not yet" is the list.

Gradle is supported, and the bar `gradle.rs` clears is answer exactly or
refuse, never guess: a tool that half-understands a build file and reports a
dependency the build does not have is the worst outcome available.
