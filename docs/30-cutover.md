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

# 30 — The cutover — deleting the legacy engine

**Read `docs/00-contracts.md` first.** It carries the five contracts, the
deletion map, the identifier map and the ownership table; nothing here repeats
them, and work that contradicts them is wrong however well it reads.

## What you own

`crates/jails-workspace/**` (capture, exact materialization, verification
and the single canonical executor), `crates/jails-project/**`, the nine legacy
crates the strangler deletes, `crates/jails-{drive,report,java}/**`, and the
pre-canonical binary surfaces `src/new.rs`, `src/app.rs`, `src/dispatch.rs`.

**You execute the deletion map** in `docs/00-contracts.md` §1.7. Every item
below is one row of it, or one thing standing between you and a row.

## What you do not touch

`crates/jails-model/**` is A's and `crates/jails-compiler/**` is B's. You
capture facts and execute plans; you do not decide what a model means or what
Java it renders.

`jails-drive` and `jails-report` are **not** legacy -- they are the commands
that outlive the cutover -- so vocabulary they need may not live in a crate
that dies. That is why `identity` and `identifier` are in `jails-support` and
`testing` is in `jails-drive`, rather than beside the legacy engine that
happens to use them.

Three things are shared with the other three workstreams and have resolution
rules in `docs/00-contracts.md`: `tests/golden/**`, `tests/architecture/board.rs`
and `LAYERS`. Append to `tests/common/scenarios.rs`; move nothing in it.

## The specification sections this work answers to

§1.6 managed output and ejection (in `docs/00-contracts.md`), §15 caps and
project declarations, §16 lifecycle, change evidence and ownership,
§22 upgrade from the pre-v1 draft, §23 deliberate non-goals.

## How you know you are green

```
cargo test -p jails-workspace -p jails-project
cargo test -p jails-prepare -p jails-commit -p jails-protocol   # before/after P13.4 batches
cargo test --test product_loop --test agreement --test crash
mise run verify-rewrite
```

**Deleting a legacy path is not green until the canonical one is measured**,
not assumed. `tests/product_loop.rs` is the harness; read *What
"both implementations" currently means* in `docs/40-gates-and-ci.md` before
quoting it as a differential.

---

# Open items

**The legacy generator stack is deleted.** `jails-generate` was 23,809 lines
across `generate`'s per-kind recipes, `spring`'s Boot-only kinds and
capabilities, and `add`'s capability planner; it is 1,324, and what survives is
not a generator -- the write path's byte-keyed rules and the field spec's SQL
projection, which the binary and `jails-report`'s schema lineage still call.

**The anchor was `doctor`, not `jails g`.** Every advertised kind has compiled
through `jails-compiler` for some time and `Command::Generate` has no legacy
branch, but rustc cannot see across a crate boundary: a `pub` item in a library
is never dead code, so the twenty-nine ratchets that police this tree were
blind to all of it. What kept it *genuinely* reachable was one call --
`doctor::capability_drift_checks` re-planning each recorded capability through
`add::plan_for`. That branch returns early for a modelled project, and `jails
add`, `add dependency` and `sync` all seed a model before they act, so it could
only fire on a project this jails can no longer produce. Removing it made the
whole stack unreachable, and then the in-crate dead-code pass named the rest.


**P13.2 Five production files parse Maven XML; the design asks for one.**
`jails-project/src/pom.rs` is the path being replaced;
`jails-workspace/src/{capture,documents}.rs` and `documents/build_feature.rs`
are replacing it; `jails-protocol/src/vocabulary/coordinate.rs` reads a plugin
block as a protocol value. Four of the five are the strangler migration, so the
duplication is deliberate until the cutover -- and the ratchet exists so a
*sixth* answer cannot appear while it is going on, which is the failure a
migration invites. `jails-workspace/src/capture/observe.rs`'s `junit_version`
is deliberately below the bar: it matches one element to read one artifact's
version, which is a lookup rather than an opinion about structure.

**Exit:** delete `pom.rs` once the document backend is trusted. This is the
cutover decision, not a refactor.


**P13.4 90 wire formats are still hand-written, and the seam has now ended
for a reason.** 133 -> 90 by the method this item asked for -- reading them one
at a time -- and the class the first sweep had no name for is worth keeping:
**an enum with a hand-written `tag()`/`from_tag()` pair is a second encoding of
that type**, and its containers called the pair rather than the trait.
`ReferenceRole` is the one to read: it already derived `Codec` *and* still
carried the pair, so one enum had two encodings of one byte and only one of
them was the wire's. Seven such pairs are gone; four more enums had the pair
and no codec at all (`MavenScope`, `Optionality`, `JavaTypeKind`,
`ToolFeature`), and deriving each is what made its containers derivable in
turn. `encoder.count(..)` was never the blocker the first sweep took it for:
`Encoder::seq` *is* a count followed by a loop of `encode`, ordering guarantee
included.

**What is left is not a backlog.** 49 of the 90 validate: `decode` calls a
validating constructor, or `encode` enforces an invariant before writing a byte
-- `ByteSpan` refusing a span that starts after it ends, `PropertySetting`
refusing a comment carrying its own `#`, `CanonicalRequestSyntaxV1` rejecting
dashes, `AppliedEntity` refusing an empty owner set. That is R1.1 working: the
constructor is the only place a value is validated, so a derive that skipped it
would let a value rejected at the CLI arrive through a recovered journal
instead. The other 41 are as deliberate and smaller in number -- six encode a
label string rather than a discriminant (§R1.4 records the *name*, so
reordering an enum cannot change a recorded value), four are the primitive
impls the derive is built on, three frame a length-capped blob through
`encoder.object`, two write a raw digest array, one carries a depth counter.

**So the next move is a decision, not more reading:** either a
`#[codec(validate)]` that calls the constructor after decoding, which would
reach most of the 49 and is a change to `jails-codec-derive` rather than to 49
types -- or accept the number, which is what the board row's withdrawn target
already says.

**The golden trees are not sufficient on their own.** `PreparedIdentityV1`
passed all 62 of them and still changed the wire: its `encode` opens with a
bare `encoder.u32(1)` belonging to no field, so a derive dropped four bytes and
only `prepared_bundle_matches_the_protocol_golden` caught it. A struct whose
`encode` carries anything that is not a field is not a candidate, however well
its fields line up. **Convert in small batches, running `cargo test -p
jails-prepare -p jails-commit -p jails-protocol` alongside `--test golden`
after each.** One trap the 133 -> 90 pass hit and the next one will: reverting
a file to undo a bad edit also reverts the good conversions already made in it,
and nothing fails -- the count is the only thing that notices.


## P12 — the defect found while re-confirming the closed ones

**P12.1 (B57)** Re-running an already-installed capability in a project that
declares a compose service leaves an unfinished transaction, and every mutating
command afterwards dies on an object that was never stored. It is terminal
rather than transient, the refusal names a path inside `.jails/` and carries no
`fix:`, and `doctor` prescribes running the same command again -- which is the
reproduction. `jails sync`, whose whole job is re-applying recorded
capabilities, is among the commands that cannot run.

`add sqlite` is the control, so the trigger is the compose service rather than
any one capability. Not caught because every scenario and every proof
application exercises the *first* install.

**Two things to fix, and the second matters more:** the no-op re-apply must not
leave a transaction expecting an object it never wrote, and `doctor`'s `fix:`
must not name a command that reproduces the fault.

## P8.11a — the adoption half of P8.11

Split from P8.11 when these documents did; the generator half is `P8.11b` in
`docs/20-generated-java.md`.

- **`jails adopt resource <Name>`** registers an existing hand-written type
  into the store so `resource field`, `destroy` and `rename resource` work on
  it. Today they refuse: *"no `Message` is recorded in this project"*.
- **`modernize` does not re-plan jails' own output.** It moves the Boot
  version, and the Boot version decides what jails' generated files should say
  -- `javax.validation` against `jakarta.validation`, the `@AutoConfigureMockMvc`
  package, the MockMvc form. It should re-plan what the ledger records.

## Product direction that is yours

**P9.2 §3.3 — frozen conflicts, `continue` and `abort`.** Three-way
reconciliation, conflict detection and per-path reporting all work; the marker
bytes are produced and dropped. `PendingIdentity`, `ResolutionIdentity` and
`RestoreIdentity` exist and nothing reaches them. Note the trap this item is
named after: the frozen-conflict message told readers to run `jails continue`,
which has never existed.


**P9.6 §5.1 — Gradle behavioural parity** for the warm test engine, `jails fmt`
and `jails console`. No longer blocked. **Exit gate:** `jails test`,
`--engine build` and `--engine warm` discover the same tests and report the
same counts on both build systems.


**P9.7 §2.3, §2.4a, §2.4b, §2.4c — the latency work, each behind a dated
measurement.** The incremental source/AST index, additive test-dependency
hints, service identity labels and semantic readiness. §2.3's own note is the
rule: the report claimed a latency win here and never measured one, so no item
in this group starts without a number.


**P9.8 §2.7 — the Ecto-style SQL sandbox stays deliberately deferred.** Not a
roadmap dependency and not a default. If the experiment is run, record the
negative result rather than deleting the section.


**P9.10 `jails schema diff` requires `.jails/app.toml`**, so it does not run on
the shape `jails new` produces.


---

## What the cutover cannot start on

Recorded rather than chosen, because each has two possible shapes and no third.

**Ordinary `jails new` is still legacy.** Seeding a model in `spring.rs` is one
line, and the flip reaches a canonical project that applies and compiles. What
stops it is a measurement: how much of the legacy engine's generated suite the
compiler reproduces for the same manifest. The legacy side is pinned at
`reports: 21, tests: 57` in `tests/cli/examples.rs`; the canonical side is
pinned nowhere. `new` must also seed its six default properties as `prop`
declarations rather than reader-owned text, or a capability declaring the same
key collides with the project's own scaffolding.

**`adopt` and `modernize` have two possible shapes.** Both write reader-owned
files on projects with no model; `apply::` is banned outside the write layer;
and `execute` takes a `PlanBundle` whose `Plan` carries a
`CanonicalModelPatch`. So either the layout and release become model nodes --
which contradicts both commands being pre-canonical -- or `jails-workspace`
gains an explicit reader-file operation, widening "the only canonical project
writer" from *runs exact compiled plans* to *runs exact operations, some of
which no compiler produced*. Forging a `Plan` outside the compiler is the one
option that is not on the list.

**Two editable model sources are never permitted.** `.jails/model.jdl` is the
intended authoring boundary; `.jails/model.toml` remains a temporary
compatibility input for existing canonical projects. `model import` is one-way
and fail-closed. **It no longer accepts an edit**, which is A4.4's exit:
`model_command::read_source_at` refuses a mutation against one by name and
names `jails model upgrade --to 1`, so the compatibility input is read-only
and §22 is the route off it. The parser stays until every project that has one
has been carried across.

