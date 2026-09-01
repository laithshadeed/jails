<!--
One of six. `docs/00-contracts.md` is the one every reader starts from; it
carries the contracts, the identifier map and the ownership table that keep
these six from contradicting each other.

**A closed item is deleted from the file that holds it**, in the commit that
closes it -- never marked done. `git log -p -- docs/` is the record.

**Item and section numbers are stable and never reused.** A section with no
open items disappears rather than being renumbered.

Status prose is dated where it is a measurement. Everything else is written in
the present tense as a rule: a note narrating what a module used to be gives a
reader nothing to act on and goes stale on its own.
-->

# 00 — The contracts, and how the six documents are used

Jails is a compiler. Its source is one application model; its generated tree is
merge-managed by stable artifact identity; its irreversible output is an
explicit evolution plan; and its executor applies exactly the plan the reader
reviewed.

That sentence is the whole design. These six documents are what it means, how
far it is built, and what has not been done.

| file | what it is | who reads it |
|---|---|---|
| `docs/00-contracts.md` | this file: the decision, the five contracts, the deletion map, the scorecard, the non-goals | **everyone, first** |
| `docs/01-jdl-v1.md` | JDL v1, normative. Section numbers are the ones source comments cite | whoever needs a section |
| `docs/10-language.md` | workstream A: the model, the grammar, the linker, diagnostics | one agent |
| `docs/20-generated-java.md` | workstream B: what the compiler emits, and how good it is | one agent |
| `docs/30-cutover.md` | workstream C: deleting the legacy engine | one agent |
| `docs/40-gates-and-ci.md` | workstream D: the gates, the suite, and the CI budget | one agent |

**They replaced `new.md`, which replaced twelve.** The split is not a second
attempt at organising prose: it exists so four agents can work at once without
reading each other's context or editing each other's files. What made twelve
documents fail was that they described *one* system four ways over; these four
describe four disjoint pieces of work, and everything genuinely shared is in
this file, once.

## Working the plan

**Reproduce an item from a clean `jails new` before believing it**, and state
the command that produced it. Goldens compare bytes and never run the code, so
the oracle that finds this class of defect is a real build.

`mise run verify-rewrite` is the single answer to "is this green". Run it
before every push -- `.githooks/pre-push` and CI invoke it and nothing else.

Close an item by **deleting** it, in the commit that closes it. Do not mark it
done, and do not close one by deleting the entry that is its only record.

## Who owns which paths

Four agents, four disjoint areas. Work only in the paths your file lists.

| workstream | file | owns |
|---|---|---|
| **A — language** | `docs/10-language.md` | `crates/jails-model/**`, `src/model_jdl_edit.rs`, `src/model_generate_jdl*`, `src/model_upgrade.rs`, `src/model_explain.rs` |
| **B — generated Java** | `docs/20-generated-java.md` | `crates/jails-compiler/**`, `templates/**` |
| **C — cutover** | `docs/30-cutover.md` | `crates/jails-workspace/**`, `crates/jails-project/**`, the nine legacy crates, `crates/jails-{drive,report,java}/**`, `src/new.rs`, `src/app.rs`, `src/dispatch.rs` |
| **D — gates and CI** | `docs/40-gates-and-ci.md` | `.github/**`, `.githooks/**`, `mise.toml`, `scripts/**`, `tests/common/**`, `tests/architecture/**`, `tests/corpus/**` |

### The files all four touch

Four things are shared by construction. Each has a resolution rule, so a
collision is mechanical rather than a judgement call:

- **`tests/golden/**`** -- regenerated, never hand-edited. A conflict is
  resolved by taking either side and re-running `UPDATE_GOLDEN=1 cargo test
  --test golden`, then **reading the diff**: a golden that changes for a reason
  you cannot state is the finding, not the noise.
- **`tests/architecture/board.rs`** -- one row per ratchet, each carrying the
  reason its ceiling last moved. A conflict keeps *both* notes and re-measures;
  the recorded ceiling is whatever the merged tree reports.
- **`LAYERS` in `tests/architecture/rules.rs`** -- one row per module. Keep
  both sides and dedupe; `layers_lists_each_module_once` fails on a duplicate.
- **`tests/common/scenarios.rs`** -- append only. A new kind or capability adds
  a `Scenario`; nothing else in the table moves.

`CLAUDE.md` and `README.md` are edited by whoever's change makes them wrong,
in the same commit. Keep the edit to the section your work is about.

## Where an identifier resolves

649 source comments cite the deleted documents by identifier --
`plan.md P13.4`, `audit.md A3.14`, `bugs.md B57`, `research.md §4.2`,
`modern.md §6.5`, `jdl-sol.md §9.7`, `simplify-sol.md` G1. **Every one still
resolves**, because the identifier travelled rather than being renumbered.
Read `<file>.md <id>` as "the entry with that id", and find it here:

| id | file |
|---|---|
| `jdl-sol.md §N`, `jdl.md §N` | `docs/01-jdl-v1.md` -- Part 2's numbering, exact |
| `A1`, `A3.14`, `A3.15`, `A4.*`, `A6.2` | the workstream that owns the subject; `A4` itself is below |
| `G0`–`G5`, `P13.7`, `P13.9`, `P13.10`, `P13.11` | `docs/40-gates-and-ci.md` |
| `P6.6`, `P9.1`, `P9.5`, `P8.11b` | `docs/20-generated-java.md` |
| `P9.3`, `P9.4` | `docs/10-language.md` |
| `P8.11a`, `P9.2`, `P9.6`–`P9.10`, `P12.1`, `P13.2`, `P13.4` | `docs/30-cutover.md` |
| `D1`, `D2`, `D3`, the five contracts, the deletion map | this file |

The documents these ids came from are deleted and recoverable:

```
git log --diff-filter=D -- jdl-sol.md    # the commit that removed it
git show <commit>^:jdl-sol.md            # its last content
```

The count above was taken 2026-09-01 by:

```
grep -rhoE '(plan|audit|bugs|research|modern|jdl-sol|simplify-sol|pending|abstract|refactor)\.md' \
  --include='*.rs' crates/ src/ tests/ | wc -l
```

---

# Part 1 — The decision

## 1.1 The problem the compiler solves

Four things made jails hard to change, and none of them was volume of code.

**Product breadth is real and is not the problem.** Thirty-nine generator
kinds and twenty-five capabilities are a legitimate surface; a tool that
scaffolds one shape is not the tool this is.

**The editable-output paradox is the first real cause.** Generated code is
meant to be edited, and jails is meant to regenerate it. That is only
answerable with a merge base, and without one every generator grows its own
theory of "is this file still ours".

**No canonical semantic world is the second, and the deepest.** Requirements
were recovered from the bytes jails had already emitted -- a capability decided
what to install by scanning rendered Java. Source became the database, so every
question had two answers and the compiler could not report which was stale.

**One intent copied through too many representations is the third.** A single
`jails g record` request existed as `Intent`, `Recipe`, `Recorded`, `Declared`,
`Asked`, `CanonicalMutationRequest`, `DesiredChange`, `SemanticEdit`, `Change`,
`PreparedChange`, `PreparedKind`, a ledger row, a journal record, a receipt, an
effect and an `Outcome` -- sixteen shapes, each with its own agreement method.

## 1.2 The product choice

Four options were considered and one was taken.

| | shape | why not |
|---|---|---|
| A | honest one-shot generator | throws away the iterative loop, which is the product |
| **B** | **merge-managed compiler with implementation-boundary ejection** | **chosen** |
| C | disposable managed tree with ejection | a tree nobody may edit is a tree nobody trusts |
| D | JVM build-time compiler | moves the problem into a build plugin and a second language |

**D1 — the iterative edit loop is the product.** A reader adds a method to a
generated record, edits a validation message jails wrote, and edits the exact
line jails rewrites; all three must survive regeneration, and the third must
refuse rather than guess. This is confirmed and overrides any simplification
that trades it away.

**D2 — JDL is a required deliverable**, not an optional front end. It overrides
the earlier reading that a new language would not simplify anything: without a
durable authoring source, the model is a transcript of CLI invocations and
cannot be reviewed, diffed, or hand-edited.

**D3 — ergonomics are a requirement.** A refusal that does not say what to do
next is a defect, and every refusal carries a `fix:` line.

## 1.3 The five contracts

These are authoritative. Everything else in Part 1 follows from them.

- **`AppModel` is desired-state authority.** Stable IDs carry identity; Java,
  SQL, route and configuration names are projections of it.
- **`WorkspaceSnapshot` captures every external fact once.** Code below the
  compiler may observe the filesystem; the compiler may not.
- **`Compiler` is pure.** Equal snapshot, patch and compiler version must
  produce equal desired artifacts.
- **`PlanBundle` is the exact reviewed transition.** Preview, export,
  confirmation and apply all refer to its digest; apply never replans.
- **`jails-workspace::execute` is the only canonical project writer.** It
  locks, rechecks preconditions, publishes exact after-images, and converges on
  retry.

## 1.4 The pipeline

```text
.jails/model.jdl / CLI sugar
        -> ModelPatch
        -> AppModel + WorkspaceSnapshot
        -> pure Compiler
        -> PlanDraft
        -> exact content-addressed PlanBundle
        -> preview or the one Executor
```

The passes, in the order §20.1 specifies them: capture; parse front ends to
`ModelPatch`; link, resolve and validate; normalize facets and derive the
dependency graph; lower to typed artifact IR; derive schema and evolution;
emit a draft, then materialize one exact plan.

## 1.5 The canonical crates

Lowest first. A crate may only depend on one below it, and Cargo enforces that;
`no_module_depends_on_a_layer_above_its_own` enforces the same rule for
module-level edges, and `LAYERS` in `tests/architecture/rules.rs` is the
authority on which crate a module belongs to.

| crate | contract |
|---|---|
| `jails-model` | closed source schema, stable IDs, linking, semantic diagnostics, `AppModel` and `ModelPatch` |
| `jails-contracts` | portable `WorkspaceSnapshot`, `PlanDraft`, exact `Plan`, operations, trees and blobs |
| `jails-compiler` | pure semantic lowering to a desired artifact tree; no filesystem, environment or subprocess access |
| `jails-workspace` | capture, exact materialization, verification and the single canonical executor |

`jails-codec-derive` (a `#[derive(Codec)]` proc macro) and `jails-codemod` (the
marked block, and no dependencies at all) are the two leaves beside them.

## 1.6 Managed output, ejection, and what stays irreproducible

Reproducible output belongs below `.jails/generated` and is merge-managed. The
accepted model renders BASE, capture supplies OURS, and the next model renders
THEIRS. Clean merges are frozen into the plan; conflicts refuse without writes.
The lock advances to THEIRS so hand edits remain deltas.

Migrations, model revisions and explicit reader-file patches are
**irreproducible** operations and stay visible in the plan rather than being
smuggled into rendering.

`model eject <artifact-id>` transfers one ejectable adapter implementation into
reader source, records the transfer, and excludes that artifact from later
managed trees. Records and ports remain managed ABI. Capture must include every
prospective reader destination, collision must refuse, and ejection never
infers ownership from edited bytes or silently reclaims it. Its before-image
must be `Missing`: transfer is creation of a new reader-owned source, never
reconciliation with an existing one.

## 1.7 Concrete deletion map

What the cutover deletes, and what replaces it. This is the list a cutover step
is measured against.

| Current area | Destination | Eventual deletion |
|---|---|---|
| `jails-spec::Field` plus protocol `FieldSpec` | one model type registry and renderer views | old field derivation/translation tables |
| repeated `Layer`, route and name tables | small typed registries with derived projections | synchronized enum/label/package tables |
| `Recipe` + `refuse_misplaced` + the giant artifact match | typed declarations, recipe metadata and compiler passes | optional-field bag and negative flag matrix |
| `Recorded`, `Declared`, `Asked` | `ModelPatch`, canonical request identity and `Plan` | manual recipe reconstruction and syntax copies |
| generated Java/SQL reparsing | adoption importer plus `AppModel` | source-as-database normal paths |
| `Change` + `DesiredChange` + `SemanticEdit` + projected edit forests | one semantic `PatchSet` | mirrored apply/retire and translation layers |
| desired/observed/applied/pending record variants | canonical records plus `StateDelta` | phase-specific row duplication |
| hand codecs/tags/JSON serializers | generated wire/report schemas | repetitive encode/decode/match code |
| project `Project`/`ProjectContext`/snapshot overlap | snapshot-backed `ProjectView` | post-capture arbitrary disk reads |
| Gradle partial parser | managed fragment/plugin anchor | most arbitrary Groovy rewriting |
| duplicate Maven XML scanners | one document backend | second scanners and field-name lies |
| per-byte dependency inference | typed IR requirements | `with_test_support`-style scans |
| route-level rename/field evolution replay | stable IDs + semantic schema diff | whole-generator reruns and hard-coded companions |
| prepare identity/semantics/prepared representations | one canonical `Plan` | agreement methods and vector cloning |
| journal/receipt/object directory protocols | lock-last convergent exact-file executor plus compiler projection lock | bespoke storage, codec, GC, WAL, receipt and roll-forward machinery |
| `main.rs` dispatch + command-path + pre-clap parsing | generated command catalog | duplicated command oracles |
| `new` generation paths | compiler `PlanDraft` plus workspace materializer | manual preview path lists and nested engine state |
| drive/report/tool suites | optional command processes or explicit core modules | facade coupling and duplicated runners/caches |

The largest deletion does not come from shorter render functions. It comes from
making six questions disappear: which of six representations is authoritative;
what facts can be recovered from the Java we emitted; which recipe kinds happen
to depend on this field; is this edited file still ours; did preview and apply
run the same computation; does recovery reproduce every side condition of
normal commit. One model, one graph, one plan and one explicit ownership
transfer answer all six.

## 1.8 Architecture fitness rules

Properties that express the new shape. **Not** file-size thresholds -- those
improve navigation and can be satisfied while every duplicated concept
survives.

| rule | held by |
|---|---|
| after capture, no compiler module reaches `std::fs`, a project root or a process runner | `rules::canonical_compiler_is_pure_after_capture`, and structurally: the canonical crates depend on nothing that can read a disk |
| identical snapshot + patch + compiler version yields an identical plan digest | `materialize::the_same_snapshot_patch_and_compiler_produce_the_same_plan_digest`, plus the two negatives |
| preview, export, confirmation and apply reference one digest | `preview_export_and_apply_all_name_one_plan_digest` |
| every command and alias resolves through one generated catalog | `commands.rs` walks the live `clap::Command`; `every_command_a_message_tells_the_reader_to_run_is_one_that_exists` checks messages against it |
| every builtin type has one semantics row | `BuiltinSemantics` is one exhaustive match, held by the *largest table of per-builtin knowledge outside its row* ratchet |
| every artifact requirement comes from IR, never a content or path scan | structural |
| managed output is written only below the managed root | `execute`'s precondition check |
| reader-owned source changes only by an explicit typed patch/eject/adopt operation | `PatchReaderFile` with a captured before-image |
| every persisted union tag and field number is generated and golden-tested | partly: 90 formats are still hand-written, and 49 of those validate rather than describe a layout. See P13.4 |
| every advertised failpoint fires in at least one test | `failpoints!` emits both the registry and the constants, so an unfired point is `-D dead-code` |
| every active transaction state has one tested recovery transition | `crates/jails-workspace/tests/crash.rs` |
| the planner's read set is complete by construction | structural, via `WorkspaceSnapshot` |
| optional tool crates cannot import mutation executor internals | Cargo, plus `LAYERS` |


---

# Where it stands

Every number here was produced by running the binary or reading the tree, not
estimated. Where a claim is dated, the date is when it was measured.

## The answer

**The legacy path cannot be deleted yet.** The canonical architecture is real,
correctly layered, and delivers the hardest thing the design asked for. What
stops the deletion is named in Part 5 -- not doubt about the design.

Three things were done and should not be relitigated. Two of them still hold
exactly; the second has acquired three exceptions since it was written, which
is why it is stated with them rather than as a slogan:

- **Source is no longer a database.** `jails-java` is not a dependency of any
  canonical crate, and nothing on the canonical path reparses generated Java or
  SQL.
- **Requirements come from the model, not from bytes** -- with three
  exceptions as of 2026-09-01, and one of them is the pattern this line is
  about. `contains("…")` in `jails-compiler` is no longer confined to
  `#[cfg(test)]`:

  | site | what it reads | verdict |
  |---|---|---|
  | `lib.rs` | `snapshot.project.dependencies`, a captured coordinate set | a fact, not a scan |
  | `emit_http.rs` | a string the compiler itself just rendered | harmless, but it is re-reading its own output |
  | `emit_component/cli.rs` | captured `App.java` and `<mainClass>` out of `pom.xml`, to decide whether to retarget the jar | **this is the pattern** |

  The third stays inside the *purity* contract and is careful about it -- it
  reads the snapshot rather than the disk, and goes through
  `jails_codemod::text::blanked` so a comment cannot be mistaken for a
  registration -- and its own comment says why. What it does not do is derive
  the requirement from the model: "does this project already have a
  dispatcher" is answered by scanning bytes. `crates/jails-compiler/**` is
  workstream B's, so the fix is B's to make or to record as deliberate.

  Re-measure before trusting either direction; a count of zero here would be
  the claim restored, and a count that grew silently is the regression:

  ```
  python3 - <<'EOF'
  import pathlib
  for f in pathlib.Path('crates/jails-compiler/src').rglob('*.rs'):
      src = f.read_text(); cut = src.find('#[cfg(test)]')
      n = (src if cut < 0 else src[:cut]).count('contains("')
      if n: print(n, f)
  EOF
  ```
- **Preview and apply cannot plan twice.** `finish_generation` does one
  capture, one compile, one materialize, then either reports the bundle or
  executes *that* bundle.


## Coverage

| | |
|---|---|
| generators on the canonical path | 39 of 39 |
| capabilities | 25 of 25 |
| component kinds with a backend | 23 of 23 |

Each is held by an exhaustive match over a `clap::ValueEnum`, so a kind added
without a backend fails to compile rather than at the cutover.


## A4 — simplicity, measured

Production lines, `#[cfg(test)]` stripped and blank lines excluded.

| | lines | units covered |
|---|---:|---:|
| legacy transaction kernel (`prepare` + `commit` + protocol `intent`/`durable`/`observe`) | 18,789 | — |
| replaced by `jails-workspace` + `jails-contracts` | **3,763** | — |
| legacy generation (`generate` + `spec` + `java` + `project` + `engine`) | 41,328 | 64 |
| replaced by `jails-model` + `jails-compiler` + root `model_*` frontends | **25,389** | 41 |

**A4.1 — the transaction-kernel simplification is real, and it is the big
one.** Roughly 5×. Object store, custom codec, GC, journal, receipts and
roll-forward are replaced by capture → merge → exact plan → lock-last
publication. This is the largest claim in Part 1 and it is delivered.

**A4.2 — the generation simplification has not happened.** 646 production lines
per generator-or-capability on the legacy side; 619 on the canonical side.
Flat. The cause is A3.14: moving string assembly into a new crate does not make
it cheaper. The one place a real IR exists -- `Pack` -- is also the one place
legacy and canonical share templates and cannot drift.

**A4.3 — representation count is about the same; the win is authority, not
arity.** Sixteen shapes became a comparable number, but exactly one of them is
authoritative and the rest are projections of it. That is the change, and
counting shapes does not show it.

**A4.4 — the tree is currently larger than before the rewrite began.** Three
model front ends are live and editable: `.jails/model.toml` (`source.rs`, 581
lines), the pre-v1 JDL draft (1,912) and `jdl 1` (`jdl/v1/`, 4,955). Above them
sit ~6,551 lines of frontend adapters in the root binary carrying 25
`is_v1_source` branch sites, because every mutating command is written three
times. Expected mid-cutover -- **but no simplicity claim can be banked until
two of the three front ends are gone.** §22 is the upgrade path that removes
them.


---

# Part 6 — Deliberate non-goals

## 6.1 What will not simplify jails by itself

- **Minijinja or another template engine.** The templates are real `.java`
  files with `{{name}}` substitution and no logic. Anything structural stays in
  Rust and is passed in rendered.
- **A three-crate rewrite.** Fewer crates is not fewer concepts.
- **"Render in a temp directory and atomically swap it."** It cannot preserve a
  reader's edits, which is D1.
- **A fully dynamic runtime schema.** The closed vocabularies are what make an
  unknown declaration an error rather than a silent no-op.
- **A full Java compiler in Rust.** The readers stay small: `java.rs` answers
  "what is annotated with what", `classfile.rs` answers "which types does this
  class name", and neither may grow into a parser.
- **Deleting the WAL because generated output is reproducible.** Reproducible
  output does not make a half-applied transition safe.
- **LOC as the limiting variable.** It is not, and a gate on file length can be
  satisfied while every duplicated concept survives.

## 6.2 What JDL v1 will never have

Includes, imports, macros, templates, conditional declarations; environment
overlays or secrets; arbitrary Java annotations, Java expressions, SQL
expressions or build XML; per-declaration packages, configurable
suffixes/plurals, route styles, test templates or migration names; implicit
many-to-many relations or ORM navigation collections; migration history in the
desired-state file; plugin-defined unnamespaced keywords; automatic reverse
adoption of ejected code; multi-file partial declaration merging.

**No second spelling of a fact the language already states.** `@audit` and
`--with-audit` are `--timestamps`, which on a `jdl 1` source writes
`@default(now())` and `@updated`; `n:int=0` is `@default(0)`; `--with-events`
is `jails g event`; `status:enum.PENDING.PAID` is `jails g enum` plus
`status:Status`. Each was a documented proposal (`research.md` §4.1, whose own
note called two ways to say one thing "the drift generator this repository has
paid for twice") and each is refused for that reason, not for cost.

These omissions keep name resolution, ownership, diffs and safety review
deterministic. Repetition that proves common should become a typed projection
or component kind in the shared registry first. A capability that cannot be
modeled safely can still expose a managed port and an explicit ejected
implementation boundary.

**Future versions may add a construct only with its grammar, typed
linked-model payload, validation schema, stable-ID rule, ownership boundary,
formatter behavior, CLI mapping, upgrade rule and conformance tests. Syntax
alone is not a language feature.**

## 6.3 The product scope bar

No ORM, no jails runtime jar, no Lombok, no preview features in generated Java,
and no plugin system with lifecycle hooks. `README.md`'s "Not yet" is the list;
check it before adding a command that is not already there.

**"No Gradle" was on that list and was deliberately removed on 2026-08-24.**
The target that reversed it is a Gradle + Spring Boot project that has to be
worked in daily: `add`, `check`, `test`, `build` and `run` all refused there.
Degrading politely is worth less than working, when the project is the one you
are actually in. The old rule's *reason* survives as the bar `gradle.rs` has to
clear -- answer exactly or refuse, never guess -- because a tool that
half-understands a build file and reports a dependency the build does not have
is still the worst outcome available.
