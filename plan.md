# plan.md — the work that remains

> **RFC status:** implementation-ready target; all work is still queued unless
> a row explicitly says otherwise.
> **Audience:** an engineer unfamiliar with the repository, or a small coding
> model working one bounded slice at a time.
> **Normative authority:** this file fixes delivery order, concrete types,
> algorithms, wire rules and acceptance gates; `abstract.md` fixes the shorter
> architectural invariants. If they disagree, stop and reconcile both before
> writing code.

Audited 2026-08-23, re-baselined against **`119ed20`** (`main`). §3 is
shipped; R1 is the next queued work and nothing below it has been started.

Two structural changes landed between the audit and this baseline, neither of
which is a roadmap phase and neither of which may be counted as one. The
generator/template/test work the audit found in the worktree is committed. And
`src/` is now a seven-crate workspace — `jails-support`, `jails-java`,
`jails-spec`, `jails-project`, `jails-generate`, `jails-tooling` and the `jails`
binary — because the tree was one twelve-module strongly connected component
and no boundary could be drawn anywhere in it. **Every path in this RFC that
names `src/<module>` should be read against `CLAUDE.md`'s Workspace table**;
line numbers here are evidence locators, not current addresses.

The workspace has one consequence for every gate below: `cargo test` at the
root tests the root package only. Every command in this document means
`cargo test --workspace`.

`deps/`, `ideas/` and `patterns/` are untracked research trees, not in-flight
product work.
The `home-laith-code-jails` knowledge-graph transport was closed during the
audit, so structural claims below were verified from the named source and test
symbols directly. Do not treat old graph line numbers as current evidence.

This is a living roadmap, not a changelog. Shipped behaviour belongs in
`README.md`, current implementation facts and traps in `CLAUDE.md`, architectural
rationale in `abstract.md`, proof contracts in `examples/ACCEPTANCE.md`, and
measurements and friction in `examples/DOGFOOD.md`. Git history is the archive.

## 1. The contract for this document

The goal is unchanged:

> Build real applications using only `jails` commands, with zero hand-written
> Java or SQL, and make each rebuild faster, cheaper and easier than the last.

The proof applications are falsifiers, not the product. A crawler, inbox,
payments gateway or ledger CLI may expose a missing generic primitive; it may
never put its domain vocabulary into `src/` or `templates/`.

Every roadmap item has one status:

- **SHIPPED** — committed at the baseline and its named gate passed. A shipped
  section's *status line* is what changes; its normative text stays. That is an
  amendment to the earlier rule, made under §1.1's step 7 because implementation
  proved it inconsistent: 73 source and test comments cite these sections by
  number (`§R1.4`, `§R3.1`, `§R5.3`), and deleting a section on the day it ships
  would turn every one of those citations into a dangling reference to git
  history. Prose that merely *describes* the pre-implementation baseline is
  replaced when it goes stale; the closed tables, wire definitions and numbered
  algorithms are the reference the code points at and are kept.
- **IN FLIGHT** — present only in tracked worktree changes. It is not a trusted
  dependency and must not be described as shipped.
- **QUEUED** — no active implementation claim.

R1 through R5 are shipped and R6 is in flight: its routes exist and are tested,
and **no production dispatch uses them**. That is the state §R6.1 step 1
describes, not a half-finished cutover -- the flip is one commit, step 9, and
until it lands every command still runs the V1 path. The sole order is:
immediate integrity work, then R1 through R6. Later sections do not define a
second queue.

### 1.1 Implementation protocol for one bounded slice

Treat each numbered algorithm step or one row of a touchpoint matrix as the
largest acceptable slice for a junior engineer or small coding model:

1. Read the named current symbols and the nearest existing tests. Confirm the
   current signature/behaviour; line numbers in this RFC are evidence locators,
   not an instruction to preserve a private name.
2. Add the smallest failing unit/golden/integration fixture that proves the
   named invariant and at least one rejection. For a wire/state slice, add its
   canonical bytes and one independently corrupted field before adding a
   writer.
3. Implement through the target type/owner named in this RFC. Prefer evolving
   the existing type; if a temporary adapter is required, name the phase that
   deletes it and keep production dispatch on the old path until the stated
   cutover.
4. Run the focused test, `cargo fmt --all -- --check`,
   `cargo test --all-targets --no-run`, and every phase gate made reachable by
   the slice. A skipped/environment-constrained external tier is recorded, not
   called passed.
5. Search production mutation/process sites when the slice adds or moves I/O.
   Classify the site in R4/R6 in the same change; never grow a temporary
   allowlist count.
6. Stop at the phase boundary. Do not activate a writer, migrate state, delete
   a compatibility source, or broaden a public CLI merely because a later
   section already specifies it.
7. Update this RFC only if implementation proves the contract incomplete or
   inconsistent. Update a status only under §6's evidence rule; passing a
   focused test does not make a phase shipped.

For every slice, “done” means the new invariant is enforced at constructors
and decoders—not only at one caller—its failure leaves authoritative state
unchanged, and no second representation or interpreter was introduced. If a
required case is absent from the closed tables below, stop and amend both
documents before choosing behaviour.

Normative phrases such as “must”, “never”, “only”, “fixed”, “reject” and
“refuse” are requirements. Code blocks defining wire/state types, numbered
algorithms, compatibility decisions and phase gates are normative. “Current
evidence” paragraphs are diagnoses of the audited baseline, not APIs to
preserve. Private helper names may change only when the same ownership boundary
and tests remain obvious; public CLI spellings, wire fields/tags, state
transitions and failure meanings require an RFC edit first. When a case is not
specified, do not guess a permissive behaviour: add a failing fixture, update
this RFC, then implement it.

The transaction work is deliberately scoped. It covers mutations inside one
project root, including machine state under `.jails/`. It does not attempt to
roll back a container start, a hosted-CI dispatch, a process outside the
project, or a machine-level setup file. Read-only inspection commands keep their
own model; forcing `routes`, `beans` or environment probes through the mutation
pipeline would erase a useful boundary.

The finished pipeline has five invariants:

1. One desired entity has one typed identity, regardless of which command or
   manifest declared it.
2. Planning is deterministic and performs no live-tree write.
3. Preparation exhausts every renderer, splice, merge and refusal before
   commit.
4. Commit is serialised, journalled and recoverable; the ledger is last.
5. Reconciliation uses the exact recorded base, never an imitation rendered by
   a newer binary.

## 2. Baseline truth

Verified at this baseline:

- `cargo fmt --all --check` and `cargo clippy --workspace --all-targets` pass.
  `dead_code` is denied workspace-wide: a function nobody calls inside a
  `mod tests` is almost always a test that stopped being one, and that happened
  once while moving code between modules.
- `cargo test --workspace` passes: 1,068 tests across 30 binaries. So does
  `JAILS_REQUIRE_TOOLCHAIN=1 cargo test --workspace`, **with zero skips**,
  which is the only run that distinguishes an executed generated-Maven tier
  from a skipped one.
- `tests/architecture.rs` passes 13/13; agreement, genericity, golden, editor,
  desired, engine and CLI pass. The count is evidence, not an acceptance
  criterion.
- The architecture board reports one `Change`/`Artifact`, no ad-hoc file tuple
  or alias, no `KIND_FILES`, **zero** filesystem mutation sites outside the
  write layer (not only `fs::write`: deletes, copies, renames, hard links,
  directory creation and permission changes are all counted now), no inline
  Java in `spring.rs`, and a 666-line largest production module.
- Every module that starts a subprocess is classified against §R6.6's fixed
  rows by `every_module_that_starts_a_process_is_classified`, which fails both
  on an unclassified module and on a stale row.
- Ten crates, layered, with the edges Cargo cannot see enforced at module
  granularity by `no_module_depends_on_a_layer_above_its_own`.

Not verified as green at this baseline:

- The last recorded 293-second proof-app sweep predates this baseline. It is
  historical evidence until §R6.8 repeats it on a capable host.
- Generated hosted CI has never executed in an actual repository. §R6.8 owns
  that gate and it remains explicitly unclaimed.

## 3. Immediate integrity work — SHIPPED

Do this before R1 so a red run and an absent ledger each have exactly one
meaning. These are correctness repairs, not prerequisites to disguise inside
the architecture refactor.

### 3.1 Fail closed on machine state

`src/ledger.rs::load` currently uses `let Ok(source) = ... else`, so permission,
encoding and other read failures become an empty ledger. That can make a
destructive command believe jails owns nothing or make apply overwrite the
only record of what it owns.

Replace it with an explicit match:

```rust
match fs::read_to_string(path) {
    Ok(source) => parse(&source),
    Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(Ledger::empty()),
    Err(error) => Err(contextual_read_error(path, error)),
}
```

Parsing remains closed-schema. Empty, malformed, unreadable, non-UTF-8 and
unsupported-newer ledgers are errors; only `NotFound` constructs new empty
state. `app plan`, `--pretend`, inspection and other read-only paths parse an
older supported form in memory but never save it, delete its source, create
`.jails/`, or clean legacy files. A migration is included in the same prepared
transaction as the mutating command that first needs it, so the old state
survives until the new ledger is durable.

`Applied::has_spec()` currently guesses presence from non-empty fields,
indexes, references or timestamps. That collapses a valid zero-argument app
intent into a path-only direct-generation row. Make presence data:

```rust
enum SpecPresence { Present, Absent, UnknownLegacy }
```

New app rows persist `has_spec = true`, including a spec whose fields are all
empty/default. New direct-generation path rows persist `has_spec = false`.
Older rows without the key parse as `UnknownLegacy`; no caller may infer the
answer from content. Migration preserves the row and reports the ambiguity,
even when a current human manifest happens to match it. A matching manifest
may let `jails adopt` *propose* the explicit claim described in R1.4, but neither
loading nor migration resolves the unknown automatically. `UnknownLegacy`
survives only inside `LegacyEntry`, never as an alternate `AppliedEntity`
schema or inferred boolean. The sole route from unknowable legacy origin to a
named owner is the guarded, user-requested adoption transaction.

Focused tests must cover `NotFound`, permission/read error, invalid UTF-8,
malformed and unknown-key input, unsupported-newer schema, zero-argument
present spec, path-only absent spec and unknown legacy. For each case run
`app plan`, direct `--pretend` and one read-only inspection with filesystem
before/after snapshots proving no migration, deletion or directory creation.

### 3.2 Make every scratch tree exclusive and owned

The observed collision was in `src/generated_files.rs::tests::scratch`, but
fixing only that helper would preserve the same failure mode elsewhere. Audit
every `std::env::temp_dir()` scratch creator in `src/` and `tests/`. Test-only
callers use a common guard (`src/test_support/temp.rs` for unit tests and
`tests/common/temp.rs` for integration tests); production scratch work in
`src/app/reconcile.rs`, `src/new.rs` and later R3 staging uses
`src/scratch.rs::ScratchDir`.

Use the `tempfile` crate's `Builder::tempdir_in` for
`ScratchDir::reserve(parent, prefix)`. It provides atomic, random,
cross-platform exclusive creation and owned cleanup; reimplementing OS
randomness and Windows deletion semantics is not product work. Add it as a
normal pinned dependency, not only a dev dependency, because reconciliation
and R3 preparation need the guard in production. `ScratchDir` is a narrow
wrapper that exposes only its path, an explicit fallible `close()`, and
`persist()` for recovery storage. No caller may use `create_dir_all` to claim
an already-existing scratch root. `Drop` removes only the path returned by
`tempfile`; normal success paths call `close()` so cleanup failure is reported.

Regressions pre-create the first candidate with a sentinel and prove it is
neither reused nor removed; exercise two child processes under one `TMPDIR`;
panic inside the guarded scope and prove cleanup; simulate cleanup failure and
prove the error is surfaced on the explicit close path. Then run the fast Rust
suite twice with the normal temp directory and once with a fresh `TMPDIR`; all
three runs must agree.

### 3.3 Re-establish the full evidence baseline

Run every hermetic Rust gate after the ledger/temp fixes. Run the current
generated-Maven tier when the available host permits loopback sockets, JVM
agent attachment and containers; otherwise record the exact environment
constraint without weakening or calling that tier passed. Record commands,
host prerequisites, result and commit in `examples/DOGFOOD.md`; reconcile only
stale status sentences in `abstract.md`/`examples/ACCEPTANCE.md` with those
results. Creating a disposable hosted repository and proving the generated
least-privilege workflow belongs exclusively to R6 product acceptance; it is
not a credential-dependent prerequisite for starting R1.

Immediate gate — met, `36ddee2` (§3.1) and `119ed20` (§3.2):

- every ledger read failure has a distinct fail-closed test and only
  `NotFound` means empty — **met**; empty, malformed, non-UTF-8, permission and
  unsupported-newer input each have one;
- present/absent/unknown spec state is explicit and zero-argument specs survive
  — **met**; `SpecPresence` is persisted as `has_spec`, and the previous binary
  is on record planning `pending generate` where this one plans `update`;
- plan, pretend and inspection are byte-for-byte non-mutating on legacy state —
  **met**, at unit and CLI level with before/after tree snapshots. The previous
  binary printed "nothing to destroy" *and deleted the records that said
  otherwise*;
- every scratch root is exclusively created, guarded and cleaned without
  touching a pre-existing directory — **met**; `ScratchDir` over
  `tempfile::Builder`, with an architecture gate against a new
  `env::temp_dir()` in production;
- ordinary suites repeat under shared and fresh temp parents — **met**; two
  normal-`TMPDIR` runs and one fresh, all 19/19 binaries and 612 tests;
- the current Maven tier is recorded as passed, failed or
  environment-constrained without changing the meaning of its gate — **passed**
  on this host: `JAILS_REQUIRE_TOOLCHAIN=1 cargo test --workspace` is green
  with zero skips, which is the only run that distinguishes an executed tier
  from a skipped one. Hosted-CI proof remains explicitly unclaimed until R6.

Commands, host prerequisites and results are in `examples/DOGFOOD.md`.

## 4. Authoritative roadmap

| Phase | Deliverable | Production constraint |
|---|---|---|
| Immediate | Fail-closed legacy reads, owned scratch, trustworthy evidence | Correct current path only; no V2 state. |
| R1 | Complete closed schema-2 protocol leaves, identity/spec/ownership ledger model and pure legacy translation | Read/shadow only. |
| R2 | Complete immutable snapshot, projected planning and closed transition routing | Plan/shadow only; no project mutation. |
| R3 | Exact prepared operations, identities, tools and reports | Scratch allowed; no managed-root or ledger write. |
| R4 | Lock, journal, receipt, executor, recovery and effect state machine | Dark/test-only V2 dispatch. |
| R5 | Stored-base reconciliation, renderer provenance, conflict lifecycle and object GC | Still dark until every mutator is ready. |
| R6 | All command adapters, one atomic dispatch cutover, deletion and product proof | First and only production V2 activation. |

Each phase consumes only the completed contracts above it. A later target type
may be shown earlier so the final model is readable; introduce that type in the
earliest owning phase, not as a second placeholder DTO. No phase may weaken an
earlier gate to make its own implementation easier.

#### 4.1 Protocol and phase ownership rule

The schema dependency direction is fixed before work begins. R1 owns one pure
`src/protocol/` module tree containing the **complete** schema-2-ledger wire
model and codec, including dormant provenance, renderer, pending-conflict and
post-commit-effect leaves whose behaviour is activated only in R3–R5. It also
owns `CanonicalMutationRequest` and its nested canonical request types because
they are identity-reachable protocol values. Later sections give the normative
field/tag definitions for readability; the R1 implementer must transcribe
those definitions into the protocol module, and later phases must reuse them
without adding a schema-2 field, tag, placeholder DTO or opaque byte escape.

Behaviour remains phased. R1 constructs/validates desired identity, ownership,
requests and pure ledger values. R2 adds the complete snapshot and desired-
input hashes, constructs `InvocationFingerprint`, `ResolvedMutation` and the
closed `PlannedTransition`, and performs routing. R3 consumes those frozen
values for preparation and adds journal/receipt identity codecs that are not
ledger-reachable. R4 activates execution semantics; R5 activates the already-
encoded renderer/conflict/object-GC semantics. A later phase may add functions
and validators around an R1 wire type, never a peer representation.

Consequently, any final-model code block printed under R1 that mentions
`InvocationFingerprint`, `ResolvedMutation` or `ResolvedAction` is an R2
target listing, not a requirement to fabricate those values before the R2
snapshot exists. Conversely, a ledger-reachable type printed under R3 or R5 is
an R1 protocol declaration even though its semantic constructor belongs to
the later phase. The build must preserve this dependency direction:

```text
protocol values/codecs (R1)
    <- desired constructors (R1)
    <- snapshot + resolution + planning (R2)
    <- preparation + report projections (R3)
    <- executor/recovery (R4)
    <- reconciliation/conflict/GC semantics (R5)
```

### R1 — Desired-state model — SHIPPED

Gate: the closed protocol leaves round-trip and refuse a corrupted field in
`crates/jails-protocol`, and `no_two_crates_share_a_module_name` plus the layer
table hold the dependency direction §4.1 fixes.

Replace the command-shaped app loop with typed declarations and an explicit
comparison between human desire and machine observation.

Current evidence:

- `src/app.rs::GenerateIntent` and `ResolvedIntent` carry
  `strategy_on`/`strategy_yields` as `Option<String>` and compute a string key
  from identity and content together;
- `src/app.rs::apply` executes manifest rows sequentially and records each row
  after its mutation;
- `src/ledger.rs::Applied` records content and file paths but no owner, while
  `Ledger.models` repeats a subset under a second identity; and
- `src/generated_files.rs` and destroy logic use ledger rows as applied facts,
  but current code cannot express “this path is shared by two declarations” or
  “this manifest no longer wants this entity.”

The critical boundary is normative: **the ledger is never parsed into desired
state**. `DesiredState` comes only from human-owned `jails.toml`, the selected
human-owned app manifest, and the current direct command request. The ledger is
observed/applied machine state inside `ProjectSnapshot`. A command compares the
owner scope it controls against that observation; rows outside the scope are
carried forward as observed facts, not converted into declarations.

#### R1.1 Canonical types

Add `src/desired.rs` and move the syntax-only manifest DTO to
`src/app/manifest.rs`. The following names and distinctions are fixed:

```rust
#[derive(Clone, Eq, Ord, PartialEq, PartialOrd)]
struct IntentId {
    recipe: Recipe,
    name: Name,
    package: Package,              // convention resolved; never optional here
}

#[derive(Clone, Eq, PartialEq)]
struct IntentSpec {
    arguments: IntentArguments,
    indexes: Vec<IndexSpec>,
    timestamps: bool,
    on: Option<ResolvedRef>,
    yields: Option<ResolvedRef>,
}

// The positional list means three different things, and which one is decided
// by the recipe alone -- never by looking at the tokens. See the argument
// shape table below.
enum IntentArguments {
    Fields(Vec<FieldSpec>),
    Names(Vec<Name>),
    Mappings(Vec<FieldMapping>),
}

struct FieldMapping { child: Name, parent: Name }

struct FieldSpec {
    name: Name,
    field_type: FieldType,
    optionality: Optionality,
    constraints: FieldConstraints,
}

enum FieldType {
    Scalar(ScalarFieldType),
    List(ScalarFieldType),              // nested collections are unsupported
    Map { key: ScalarFieldType, value: ScalarFieldType },
}

enum ScalarFieldType {
    Text, Integer, Long, Boolean, LocalDate, LocalDateTime, Instant,
    Uuid, Currency, Decimal, Bytes, Duration, ZoneId, Uri, Path, Double,
    Project(JavaType),
}

enum Optionality { Required, NonBlank, Nullable }

struct FieldConstraints {
    primary_key: bool,
    unique: bool,
    indexed: bool,
    scoped: bool,
    numeric: Option<NumericConstraint>,
}

enum NumericConstraint { Positive, NonNegative }

struct IndexSpec { columns: Vec<IndexColumn> }

struct IndexColumn {
    field: Name,
    direction: IndexDirection,
}

enum IndexDirection { Ascending, Descending }

#[derive(Clone, Eq, PartialEq)]
struct ResolvedRef {
    target: RefTarget,
    expect: Referent,
}

enum RefTarget {
    Managed(IntentId),
    Existing(ExistingTypeRef),
}

struct ExistingTypeRef {
    qualified_name: JavaType,
    source: ProjectPath,
    source_sha256: ObjectId,
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum Referent { Resource, UseCase, Event, Dispatcher, Type }

#[derive(Clone, Eq, Ord, PartialEq, PartialOrd)]
enum EntityId {
    Capability(CapabilityId),
    Intent(IntentId),
    ToolFeature(ToolFeature),
}

enum ToolFeature { FastTest }

struct CapabilityId {
    kind: Capability,                 // the existing exhaustive value enum
    instance: CapabilityInstance,
}

enum CapabilityInstance {
    Singleton,
    Named { name: Name, package: Package },
}

struct CapabilitySpec {
    placement: Option<Package>,       // only singleton-placed capabilities
}

enum TypeTargetId {
    Managed(IntentId),
    Existing(JavaType),               // stable FQCN; bytes live in the spec/read set
}

enum SourceInputId {
    Project(ProjectPath),
    External { path_id: ExternalPathId },
}

struct ExternalPathId(ObjectId);

#[derive(Clone, Eq, Ord, PartialEq, PartialOrd)]
enum OneShotId {
    Field { target: TypeTargetId, field: Name },
    Migration { path: ProjectPath },
    Cases { source: SourceInputId },
}

struct CasesReceiptId(ObjectId);

enum OneShotSpec {
    Field { target: RefTarget, field: FieldSpec },
    Migration { description: String, allocated_version: u64,
                path: ProjectPath, body: ObjectRef },
    Cases { source: SourceInputId, source_sha256: ObjectId,
            output: ProjectPath },
}

#[derive(Clone, Eq, Ord, PartialEq, PartialOrd)]
enum ResourceOwner {
    Entity(EntityId),
    OneShot(OneShotId),
}

#[derive(Clone, Eq, PartialEq)]
enum EntitySpec {
    Capability(CapabilitySpec),
    Intent(IntentSpec),
    ToolFeature(ToolFeatureSpec),
}

struct ToolFeatureSpec { console_version: MavenVersion }
// MavenVersion, not ManagedVersion: the console launcher's version must equal
// the project's own JUnit version, and a pom that manages that version (a
// Spring Boot parent, an imported junit-bom) must be given none at all -- a
// redundant one pins the launcher while the BOM moves the engine, which is
// the misalignment that dies at run time with NoSuchMethodError. A spec that
// could only say "pinned X" could not describe the commonest project there
// is, and recording an invented number would be a claim about bytes jails did
// not write.

#[derive(Clone, Eq, Ord, PartialEq, PartialOrd)]
enum OwnerId {
    AppManifest,                    // the one selected app declaration source
    DirectConfig,                   // capability declared in `jails.toml`
    DirectCli,                      // direct generate/destroy ownership
}

struct DesiredEntity {
    id: EntityId,
    spec: EntitySpec,
    owners: BTreeSet<OwnerId>,
}

enum ReconcileScope {
    AppManifest,                    // complete presence/absence for this owner
    DirectConfig,                   // projected `jails.toml` after add/remove
    DirectEntity(EntityId),         // one direct generate/destroy request
}

struct DesiredState {
    scope: ReconcileScope,
    entities: BTreeMap<EntityId, DesiredEntity>,
}

struct ResolvedMutation {
    invocation: InvocationFingerprint,
    action: ResolvedAction,
}

enum ResolvedAction {
    Reconcile(DesiredState),
    ApplyOneShot { id: OneShotId, spec: OneShotSpec },
    DestroyCases { id: OneShotId, force: bool },
    AppInit { target: ProjectPath },
    Rename { from: JavaType, to: JavaType, force: bool },
    AdoptLayout,
    AdoptLegacy { legacy_key: LegacyKey, intent: IntentId,
                  replace: bool, force: bool },
    Format { scopes: BTreeSet<ProjectPath> },
    ContinueConflict,
    AbortConflict,
}
```

`ResolvedMutation` is the only output of command resolution. It prevents the
ordinary desired graph from becoming a bag that silently drops one-shots or
control operations. The routing table is closed:

| Canonical request | `ResolvedAction` | Lifecycle rule |
|---|---|---|
| `add`, `remove`, `sync`, `app apply` | `Reconcile(DesiredState)` | Human declarations plus the direct request are authoritative only for their `ReconcileScope`. |
| persistent `generate` or `destroy` | `Reconcile(DesiredState)` | Add or remove exactly the `DirectCli` owner; a retained typed dependant makes last-owner removal refuse. |
| `test --fast` | `Reconcile(DesiredState)` | Add or retain the `DirectCli` owner of persistent `ToolFeature::FastTest`; it is not a maintenance side channel. |
| `remove fast-test [--force]` | `Reconcile(DesiredState)` | Remove exactly the `DirectCli` owner through `ReconcileScope::DirectEntity`; refuse a retained dependant, and use the ordinary stored-base/force rules for a drifted shared POM. |
| `generate field|migration|cases` | `ApplyOneShot` | Record one stable one-shot receipt; field and migration have no destroy route. |
| `destroy cases` | `DestroyCases` | Remove only the matching cases contribution/output under stored-base and force rules. Any other one-shot destroy rejects during resolution. |
| `app init`, `rename`, `adopt layout`, `adopt legacy`, `format` | matching maintenance variant | Plan one typed maintenance subject; never invent a desired entity to carry it. |
| rerun matching pending command | `ContinueConflict` or `AbortConflict` | R5 constructs a finalisation or abort plan from frozen state; it never replans the command's new desired graph. |

After resolution and the pending-conflict gate, `plan_all` may select a
matching interrupted/failed effect attempt before ordinary planning. That result contains a receipt/effect compare-and-swap plan
and cannot also contain project changes. Unknown request/subject combinations
reject exhaustively; there is no `Other`, `Custom`, or `Vec<Change>` escape
hatch. The CLI parser treats the reserved subject `fast-test` separately from
capability names: `jails remove fast-test [--force]` becomes
`CanonicalMutationRequest::RemoveToolFeature`, never the capability-oriented
`Remove` variant. Capability `remove` accepts `--no-start` just like `add`,
`sync` and `app apply`; without it, a compose-service removal emits the runtime
`ComposeReconcile` effect. This flag never suppresses the logical/file
removal transaction.

`destroy cases` accepts exactly one of an existing `<source>` argument or
`--receipt <64-lowercase-hex>`. The latter is `CasesReceiptId`, printed by the
successful import and `doctor`; resolution requires exactly one captured
`OneShotReceipt { id: OneShotId::Cases { source }, spec:
OneShotSpec::Cases { source: spec_source, .. }, .. }` whose two source IDs are
equal and whose derived ID matches, then canonicalises to the same
`ChangeSubject::OneShot(OneShotId::Cases { source })` as the source form. The
derivation is exact:

```text
CasesReceiptId = ObjectId(SHA256(
    "JAILS-CASES-RECEIPT-1" || encode(OneShotId::Cases { source })
))
```

`encode` is the shared canonical protocol codec, including the `OneShotId`
variant tag and the complete `SourceInputId`; the ASCII domain separator is not
length-prefixed. The ID deliberately excludes source content hash, output path,
receipt operation and mutable state, so it remains stable across a same-source
cases refresh. The wire newtype is exactly the wrapped 32 bytes. Its CLI form is
exactly 64 lowercase hexadecimal characters, and parsing followed by rendering
must be byte-identical. It is derived, never independently stored in the
ledger; ledger validation always rechecks the ID/spec source equality before a
row is eligible. A missing,
ambiguous, wrong-kind or malformed ID refuses. The CLI never attempts to
canonicalise a missing external leaf and guess its former absolute identity.

`Recipe`, `Name`, `Package`, `FieldSpec`, `IndexSpec`, `CapabilityId` and
`ProjectPath` are types, not string aliases. `CapabilitySpec.placement` is
`Some` only for the singleton-placed class; named identity carries its package
in `CapabilityInstance`, and conventional singletons use `None`. Reject every
other shape at decoding and resolution boundaries. `ProjectPath` is UTF-8,
project-relative and `/`-normalised; it rejects empty, `.`, `..`, absolute and
platform-prefix components and every path beneath `.git` or `target`.
Everything beneath `.jails` is reserved by default. The constructor's complete
exception allowlist is exactly `.jails/app.toml` and `.jails/templates` plus
its validated descendants; those exist for the human app manifest and
project-template input layer. `.jails/ledger.toml` is represented only by the
dedicated ledger image, legacy machine paths only by `LegacySourcePath`, and
coordination/object/transaction/receipt paths by executor-owned types—never an
ordinary `ProjectPath` or `FileOp` target. Operation validation further permits
the `.jails/app.toml` exception only as the absent create target of `AppInit`
and forbids writes to template overrides; recipes may
read, but never mutate, the override layer. Managed non-UTF-8 paths refuse
instead of entering lossy ledger text. `Name` and `Package`
validate the Java/package rules once. Those rules were unspecified here and are
now fixed, from JLS §3.8/§3.9 rather than invented: an identifier starts with a
letter, `_` or `$` and continues with those plus digits, and is never a
reserved word or one of the `true`/`false`/`null` literals. The *contextual*
keywords (`record`, `sealed`, `permits`, `yield`, `var`) stay legal, because
`javac` accepts them as identifiers and refusing them would reject a good field
name for a rule the compiler does not have. The first character is restricted
to ASCII, which is stricter than the JLS: jails derives file names from these,
filesystems disagree about Unicode normalisation, and a type whose name is one
sequence in the ledger and another on disk is not a class of bug worth
accepting for a feature nobody has asked for. `Package` is a dot-separated
sequence of the same and **is allowed to be empty** — `--package ''` puts a
generated tree flat in the base package, which `CLAUDE.md` pins as a shape that
must keep compiling. That is also why `IntentId.package` is not an `Option`:
"the user did not say" and "the user said flat" must not share a slot. `Recipe` is the existing `ArtifactKind`
renamed at the internal boundary; Clap spelling stays at the CLI boundary.

Other scalar names in this RFC are validating newtypes, not aliases:
`JavaType` is a fully qualified Java type; `TemplateId`, `TemplateKey`,
`ToolId`, `ToolCachePath`, `ServiceName`, `MavenId`, `MarkerId`, `VolumeName`
and `PropertyKey` are non-empty canonical logical identifiers with no absolute
path; `ManagedVersion` is a
non-empty pinned version string; `ObjectId`, `OperationId` and `TransactionId`
are exactly 32 bytes internally. Their constructors are the only place that
accepts strings, and every wire decoder calls the same constructors.

`ExternalPathId` is constructed once for an existing explicitly selected
external manifest or cases file. On the Unix v1 platform, resolve the supplied
path to its canonical absolute leaf, require the resulting path to be valid
UTF-8 with `/` separators and no lossy conversion, encode that exact string
with the shared length-prefixed codec, and compute
`SHA256("JAILS-EXTERNAL-PATH-1" || encode(canonical_utf8_path))`. The digest is
the wrapped value; the absolute string remains only in runtime bindings.
`SourceInputId`, `ExternalInputId::CasesBrief` and
`ManifestSourceId::External` must use this one constructor/value—never rehash a
display string independently. An original symlink spelling resolves to the same
canonical target identity; a moved target has a new identity even with equal
bytes, and a non-UTF-8 canonical path refuses. Goldens cover direct-vs-symlink
selection equality, move inequality, non-UTF-8 refusal and lowercase-hex CLI
presentation where exposed.

Field syntax is normalised once at the declaration edge. `string` and `text`
both become `ScalarFieldType::Text`; Java and short spellings of every built-in
become the same scalar; a capitalised project type resolves to a fully
qualified `JavaType`. `!` is `NonBlank` and is valid only for `Text`; `?` is
`Nullable` and is invalid for collections and primary keys. Constraint marker
order is irrelevant and duplicate/conflicting numeric markers reject.
`IndexSpec` replaces the current pass-through SQL tail: each comma-separated
column must name a declared field and may have only optional `asc` or `desc`.
Column order is semantic and never sorted. Arbitrary SQL after an index column
is deliberately rejected instead of persisted as trusted generated SQL.

**The positional argument list is not always fields, and the recipe is the
only thing that says what it is.** This is an amendment made under §1.1 step 7:
the original `IntentSpec` had `fields: Vec<FieldSpec>` alone, and four shipped
kinds do not take fields at all. `jails g enum Status ACTIVE CLOSED` names enum
constants; `g sealed`/`g strategy` name types; `g search Article title body`
names components of a record that already exists; and `g association` takes
`childField=parentField` mappings. Parsing any of those as `name:type` refuses
a command that works today, and a spec that stored them as fields would be
claiming a record component that does not exist.

The shape is a total function of the recipe, and it is a closed table:

| Shape | Kinds |
|---|---|
| `Fields` | every persistent kind not named below, including those that take no positional argument at all |
| `Names` | `enum`, `sealed`, `strategy`, `search` |
| `Mappings` | `association` |

Two rules follow, and both are load-bearing. The shape is chosen from the
recipe **before** a token is read, so a mis-typed constant is refused as a bad
constant rather than reinterpreted as a field. And a spec whose shape
disagrees with its identity's recipe is refused at the constructor and at the
decoder, so a ledger row cannot claim `enum Status` holds record components.
Order inside each list is semantic and is never sorted, exactly as §R1.4 says
for sealed variants, strategy implementations and association mappings —
which is the same statement, now with a type that can hold them.

There is one persistent app-manifest namespace, not one owner per path.
`--manifest` selects the human input for this invocation; its canonical source
identity and content hash live in `InvocationFingerprint`/the read set. A
project-internal source uses `ProjectPath`; an external source uses
`ExternalInputId` plus absolute path only for the recheck. Neither becomes an
`OwnerId`, and switching manifest paths cannot leave a hidden second app owner.

Syntax DTOs retain `package: Option<String>` and reference spellings because
omission is a user input fact. `desired::resolve` expands package conventions,
parses field/index syntax, and returns only canonical values. No recipe planner
may accept the syntax DTO.

Recipe classification is data in `src/desired/recipe.rs`, covered by an
exhaustive `ArtifactKind::value_variants()` test. It is not another switch:

| Class | Kinds | Identity/spec and lifecycle |
|---|---|---|
| Persistent generated intent | `scaffold`, `controller`, `service`, `class`, `interface`, `record`, `factory`, `value`, `enum`, `sealed`, `strategy`, `repo`, `handler`, `command`, `cli`, `client`, `fetcher`, `job`, `http-workflow`, `association`, `http-sink`, `idempotency`, `auth`, `webhook`, `search`, `durable-job`, `dto`, `usecase`, `query`, `transition`, `event`, `test`, `integration-test` | `IntentId(recipe,name,resolved package)` plus complete typed `IntentSpec`; may be manifest/direct owned, updated by stored-base reconciliation and removed when its last owner is explicitly absent. `association` remains forward-only at the database boundary: removing its Java/resource ownership never emits a reverse SQL migration. |
| One-shot field evolution | `field` | `OneShotSpec::Field { target: RefTarget, field }`; `OneShotId` uses the stable managed identity or existing FQCN, while the spec/read set carries the current source path/hash. It is not a desired entity and cannot be referenced. It prepares a guarded record change, derivative refresh and forward migration, then records an active receipt/resource contribution against the target. Every later managed-target render reapplies active field overlays in canonical `OneShotId` order before derivative refresh. Repeating the identical active operation is a no-op; a different field under the same name is a conflict. Removing a managed target retires its overlays as specified below; there is no independent field-destroy route. |
| One-shot migration | `migration` | `OneShotSpec::Migration { description, allocated_version, body }`; append-only receipt, never desired ownership, update, reconciliation or destroy. Version allocation happens from the snapshot and is rechecked under the lock. |
| One-shot case import | `cases` | `OneShotSpec::Cases { source: SourceInputId, source_hash, output }`; project-local markdown uses `ProjectPath`, while an external file uses the SHA-256 of its canonical path as stable identity and keeps the absolute binding only in runtime commit context. The markdown is an input, not an owned entity. Re-run replaces the exact recorded test output through stored-base reconciliation. Explicit `destroy cases <source>` resolves an existing source normally; `destroy cases --receipt <CasesReceiptId>` selects the same ledger row when the source was deleted/moved. Only that receipt's matching output is removed; manifest absence never does. |

Capabilities are persistent declarations too. Normalize defaults before forming
identity, and never silently ignore a CLI parameter:

| Capability class | Kinds | Canonical identity and accepted parameters |
|---|---|---|
| Multi-instance named | `csv`, `sqlite`, `json`, `http` | `CapabilityId { kind, name: resolved default-or-Name, package: resolved Package }`; `--name` and `--package` are accepted and both participate in identity. Shared dependencies compose by `ResourceKey`. |
| Singleton placed | `api`, `actuator`, `cache`, `security`, `cors`, `sse`, `mail`, `redis`, `observability` | identity is `kind`; `CapabilitySpec.placement` is mutable and accepts `--package`, but `--name` is rejected. A placement change reconciles the same entity. |
| Singleton conventional | `db`, `kafka`, `testkit`, `fake`, `format`, `coverage`, `loadtest`, `ci`, `docker`, `k8s`, `toxiproxy` | identity is `kind`; both `--name` and `--package` are rejected because their outputs are project-global/conventional. |

`jails.toml` keeps the existing string array for conventional singleton/default
instances. Parameterised or non-default placement uses a closed repeated table:

```toml
[[capability]]
kind = "csv"
name = "Dataset"
package = "io.example.imports"
```

The surgical config editor preserves unrelated bytes and orders newly inserted
tables by canonical `CapabilityId`; it never rewrites a user's existing table
solely to normalise formatting. A duplicate string/table identity is an error.
`app.toml` uses the same capability DTO, so the two human sources cannot
disagree about parameter meaning.

#### R1.2 Reference and graph validation

Recipe metadata is the single table of allowed inputs/outputs. It defines each
recipe's lifecycle class, own referent kind, allowed `on` and `yields` targets,
required capabilities and default package. The manifest parser does not repeat
it. The capability prerequisite graph has exactly three edges at this
baseline—`k8s → docker`, `k8s → actuator`, and `k8s → observability`—and the
metadata test asserts that no hidden POM/file probe adds another. Spring/plain
flavour and Java release are project preconditions, not fake capabilities.

A prerequisite edge is a validation requirement, never an implicit declaration
or synthetic owner. For every desired capability, each transitive prerequisite
must already appear in the effective desired union with at least one real
`OwnerId`: the same request/manifest may declare it, or another retained scope
may continue to own it. Otherwise resolution refuses and lists the missing
capabilities in canonical order. A dependency, plugin, generated file or
compose service found in the live project does not satisfy this requirement.
Last-owner removal of a prerequisite likewise refuses while any retained
desired capability depends on it. The planner never auto-injects `DirectCli`,
`DirectConfig` or `AppManifest` ownership to make the graph pass.

Resolve an unqualified reference first among compatible managed intents in the
referring entity's package, then the expected conventional package, then among
compatible existing source types captured by the snapshot. A managed and
existing candidate with the same fully-qualified output is one
`RefTarget::Managed`; any other multiple candidates are ambiguous and list
sorted qualified choices. A fully-qualified spelling restricts the search to
that name but still validates kind. Zero matches names the missing identity and
expected generator. Existing targets carry their source path/hash into R2's
read set. Only `Managed` edges participate in desired topological order;
`Existing` is an already-satisfied leaf. Reject managed self-reference and
cycles with the full stable cycle path.

The parser accepts current `strategy_on` and `strategy_yields` keys as
deprecated aliases for `on` and `yields` through R6. Supplying an alias and its
canonical key together is an error even when values match. Rendered manifests
use only canonical keys; the internal model contains no `strategy_*` fields.

Validation order is fixed so errors are deterministic: closed-schema syntax,
newtype validation, duplicate `EntityId`, reference resolution, cycle
detection, capability prerequisites, resource/output identity collision, then
path confinement. Sort all diagnostic entities and candidates.

#### R1.3 Desired/observed comparison and ownership

Build the operation's inputs as follows:

1. Load strict human config and selected manifest syntax without mutation.
2. Apply the current direct request to that syntax in memory: `add/remove` projects the
   resulting `jails.toml`; `generate` declares one `DirectCli` entity;
   `destroy` declares that same direct owner absent.
3. Bootstrap R2's project/source facts and load `LedgerV2` separately as
   `ProjectSnapshot.observed`; the ledger contributes no declaration.
4. Resolve only the human/request syntax into `DesiredState` against captured
   facts. Do not insert or translate ledger rows.
5. Treat the active scope's claim as a replacement, not an immutable assertion:
   discard that scope's prior observed owner claim in memory, then add its
   current desired claim when present. Retain every owner outside the scope.
   For a retained outside owner, use a current captured declaration's spec only
   when that authoritative source participates and explicitly declares the
   same identity; otherwise retain the observed shared spec. Never interpret
   outside-scope omission as removal. All resulting owner claims must agree on
   one canonical `EntitySpec`; if they do, store that spec once and union the
   owners. If they differ, refuse and show every owner/source plus a field-level
   diff. Thus a sole manifest/direct owner may update normally, and two owners
   may update together when their captured declarations agree; an old outside
   claim cannot be silently overwritten.
6. Removing one owner removes only that claim. Plan semantic absence only when
   the last owner disappears. Refuse a last-owner removal while a retained
   desired entity has a typed reference to it.

App-manifest scope is authoritative: absence means `AppManifest` relinquished
the entity. Direct config scope is the projected complete capability list in
`jails.toml`. Direct entity scope touches exactly the requested identity and
cannot remove another direct row merely because it was not on the command
line. This scoping is the mechanism that preserves unrelated applied state
without lying that machine state is human desire.

`app apply` stops copying manifest capabilities into `jails.toml`.
`jails.toml` remains the portable declaration for direct `add/remove` only.
`AppManifest` capability ownership is recorded in the ledger after commit. The
effective project capability set is the compatible union of `DirectConfig`,
manifest and retained observed owners; two incompatible parameterised
capability claims refuse.

`ResourceKey::HumanConfigCapability(id)` has one exceptional but closed
contribution rule because it represents the human declaration itself. It exists
iff the matching applied entity has `OwnerId::DirectConfig`, and its sole
resource owner is `ResourceOwner::Entity(id)` with
`ResourceValue::HumanConfigCapability(the exact capability spec)`. An
`AppManifest` or `DirectCli` owner alone never emits that resource and never
copies a capability into `jails.toml`. If `DirectConfig` is removed while
another owner keeps the entity alive, reconciliation surgically removes this
human-config resource but retains the entity and every contribution still
required by its other owners. This conditional is implemented once in desired
resource derivation, not repeated in command adapters.

The first schema-2 transition has one explicit bootstrap for an already valid
`jails.toml`: each captured config row is itself the authoritative
`DirectConfig` declaration, so an equal parsed declaration/resource is not an
unmanaged same-key collision. When the translated observed state has no V2
human-config resource/output yet and the pure human-config editor would leave
the complete file byte-for-byte unchanged, preparation may perform a
**ledger-only authoritative bootstrap**. It records the exact current whole
file as both base and current with a truthful `FormatOwner::HumanConfig` stamp
and emits no `FileOp`. Any unequal spec, duplicate/invalid syntax, competing
managed record, or edit required outside the requested surgical change follows
the ordinary collision/stored-base rules; this exception is not a general
adoption path and is unavailable after the V2 resource has existed.

That same first-transition authority closes the resource side of upgrading a
real V1 direct project. For the complete resource/output closure of an entity
declared by captured `DirectConfig`—and for `ToolFeature::FastTest` only when
the current request is explicitly `test --fast`—preparation may establish V2
ownership without a file write when all of these hold:

1. no V2 record/output for any candidate key/path has ever existed;
2. every ID/spec and prerequisite comes from the current real owner; retained
   legacy rows, an app manifest alone or an inferred live dependency cannot
   nominate a candidate;
3. the current closed format parser finds every candidate semantic key exactly
   once with the exact desired value, no competing managed record exists, and
   the complete fresh format-owner edit/render over captured live bytes is
   byte-for-byte and mode-for-mode unchanged; and
4. the entire candidate closure passes together—partial adoption refuses.

On success, emit no `FileOp`; create the real entity/resource/output rows and
store the exact live shared-file image as base/current with the fresh truthful
renderer/context stamp. Preserve unrelated syntax and every unmatched
`LegacyEntry`; this rule does not infer that an ambiguous legacy row supplied
the owner. An unequal value, duplicate key, renderer edit, partial closure,
manifest-only/legacy-only claim or post-V2 attempt follows ordinary collision
rules and gives the actionable fallback: remove the unmanaged matching key and
rerun, or use a separately specified future resource-adoption command. Format
1 deliberately adds no generic resource-adoption CLI.

#### R1.4 Observed ledger schema and migration

Replace `Ledger`/`Applied`/`Model` with one independently versioned closed
schema. These types describe what was applied, never what is wanted:

```rust
struct LedgerV2 {
    schema: u32,                        // exactly 2
    written_by: String,
    generation: u64,                    // monotonic; increment once per commit
    last_operation: Option<OperationId>,
    applied: Vec<AppliedEntity>,
    one_shots: Vec<OneShotReceipt>,
    resources: Vec<ResourceRecord>,     // one canonical row per ResourceKey
    outputs: Vec<OutputRecord>,         // one canonical row per ProjectPath
    legacy: Vec<LegacyEntry>,
    pending_conflict: Option<PendingConflict>,
}

struct LedgerV2Draft {
    applied: Vec<AppliedEntity>,
    one_shots: Vec<OneShotReceipt>,
    resources: Vec<ResourceRecord>,
    outputs: Vec<OutputRecord>,
    legacy: Vec<LegacyEntry>,
}

struct AppliedEntity {
    id: EntityId,
    owners: BTreeSet<OwnerId>,
    version: AppliedVersion,
}

struct AppliedVersion {
    spec: EntitySpec,
    operation: OperationId,
}

struct LegacyEntry {
    legacy_key: LegacyKey,
    source_kind: LegacySourceKind,
    source_path: LegacySourcePath,
    source_object: ObjectRef,            // exact original source bytes
    owner_hint: LegacyOwnerHint,
    spec_presence: SpecPresence,
    raw_spec: Option<LegacySpec>,
    paths: Vec<ProjectPath>,
}

struct LegacyKey {
    source_kind: LegacySourceKind,
    digest: ObjectId,
}

enum LegacyOwnerHint { DirectCli, Unknown }

enum LegacySourceKind {
    Schema1Ledger,
    Schema1Applied,
    Schema1Model,
    AppStateHeader,
    AppState,
    IntentFiles,
    ModelFiles,
    GlobalFiles,
    VersionFile,
}

enum LegacySourcePath {
    Schema1Ledger,                       // `.jails/ledger.toml`
    AppState,                           // `.jails/app-state-v1`
    IntentFiles { name: LegacyFileName }, // `.jails/intents/<name>`
    ModelFiles { name: LegacyFileName },  // `.jails/models/<name>`
    GlobalFiles,                        // `.jails/files`
    VersionFile,                        // `.jails/version`
}

struct LegacyFileName(String);           // one UTF-8 component ending `.files`

enum LegacyDirectoryKind {
    Intents,                             // `.jails/intents`
    Models,                              // `.jails/models`
}

struct LegacySpec {
    kind: String,
    name: String,
    package: Option<String>,
    fields: Vec<String>,
    indexes: Vec<String>,
    timestamps: bool,
    on: Option<String>,
    yields: Option<String>,
}

struct OutputRecord {
    path: ProjectPath,
    contributors: BTreeSet<ResourceOwner>,
    current: LiveFileImage,
    base: StoredFileImage,
    renderer: RendererStamp,
}

struct LiveFileImage {
    sha256: ObjectId,
    len: u64,
    mode: FileMode,
}

struct StoredFileImage {
    object: ObjectRef,
    mode: FileMode,
}

struct ResourceRecord {
    key: ResourceKey,
    owners: BTreeSet<ResourceOwner>,
    value: ResourceValue,
}

enum ResourceValue {
    WholeFile,
    MavenDependency(DependencySpec),
    MavenPlugin(PluginSpec),
    ComposeService(ComposeServiceSpec),
    Property(String),
    MarkedBlock(String),
    CommandRegistration { command: JavaType },
    HumanConfigCapability(CapabilitySpec),
}

struct OneShotReceipt {
    id: OneShotId,
    spec: OneShotSpec,
    state: OneShotState,
    lifecycle: OneShotLifecycle,
    operation: OperationId,
}

enum OneShotState {
    Active,
    RetiredTargetRemoved,
}

enum OneShotLifecycle {
    Field {
        target_coupled: BTreeSet<ResourceKey>,
        append_only: BTreeSet<ResourceKey>,
    },
    Migration,
    Cases,
}

struct MavenCoordinate {
    group_id: MavenId,
    artifact_id: MavenId,
}

enum MavenVersion { Managed, Pinned(ManagedVersion) }

enum MavenScope { Compile, Runtime, Test }

struct DependencySpec {
    coordinate: MavenCoordinate,
    version: MavenVersion,
    scope: MavenScope,
    optional: bool,
}

struct PluginSpec {
    coordinate: MavenCoordinate,
    block: CanonicalPluginXml,
}

struct ComposeServiceSpec {
    name: ServiceName,
    marker: MarkerId,
    mapping: CanonicalYamlMapping,
    volumes: BTreeSet<VolumeName>,
}
```

These specs replace—not wrap—the transitional source shapes
`pom::Dependency`, `Change.plugins` and `compose::Service`. A managed dependency
is identified by `(group_id, artifact_id)`; `Managed` means the project/BOM
supplies its version, and an absent scope normalises to `Compile`.
`CanonicalPluginXml` is validated UTF-8 with LF endings containing exactly one
complete `<plugin>` element and no surrounding POM bytes. Its embedded
group/artifact coordinate must equal `PluginSpec.coordinate`; children remain
opaque because Maven plugin configuration is intentionally open-ended. This is
safer and simpler than a partial plugin AST. `CanonicalYamlMapping` is exactly
the mapping beneath one service, without `services:`, the service-name key,
markers or another top-level section. The format owner supplies indentation and
markers. It validates that the mapping and declared volume names are confined
to that service block.

`DependencySpec`, `PluginSpec` and `ComposeServiceSpec` contain no root path or
rendered whole-file bytes. A `ResourceKey` coordinate/name and the corresponding
spec coordinate/name must agree or decoding/planning rejects. An unmarked live
dependency, plugin or compose service with the same key is unmanaged input: an
equal desired claim still requires explicit adoption, while a different value
is a collision. Never silently claim it because a loose text search found the
coordinate/name. `ResourceRecord` therefore contains enough value—not only a
hash—to render a shared file after one owner leaves.
Resources and outputs are global because a dependency, marked source file or
compose file can be shared; nesting the same aggregate record under several
entities would create competing authorities. `AppliedEntity.owners` answers
which declaration claims the entity. `ResourceRecord.owners` and
`OutputRecord.contributors` answer which entity/one-shot contributions keep a
semantic resource or complete path alive. Cross-links are derived by scanning
those canonical owner sets, not persisted twice.

##### R1.4.1 Exact schema-2 ledger wire format

`.jails/ledger.toml` remains the one ledger path for compatibility, but schema
2 intentionally uses a tiny TOML envelope around the same closed binary codec
as the transaction protocol. A second bespoke recursive TOML serializer would
double the wire surface and make canonical byte identity much harder to audit.
The payload is opaque machine state; `jails doctor --output json` is the
supported decoder. The complete file is exactly these five LF-terminated lines,
in this order, with no BOM, CR, comment, blank line, extra whitespace or key:

```toml
schema = 2
codec = "jails-ledger-payload-1"
payload_len = 0
payload_sha256 = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
payload_hex = ""
```

The example demonstrates the envelope only; an empty byte payload is not a
valid `LedgerPayloadV1`. `payload_len` is unsigned canonical decimal with no
leading zero except `0`. `payload_sha256` is lowercase SHA-256 of the decoded
payload bytes. `payload_hex` is exactly two lowercase hex characters per byte,
on one line. The codec constants are
`MAX_LEDGER_PAYLOAD = 32 * 1024 * 1024` bytes and
`MAX_LEDGER_SOURCE = 2 * MAX_LEDGER_PAYLOAD + 512` bytes. The fixed keys,
quotes, digest, decimal length and LF separators must fit within the 512-byte
envelope allowance; the renderer asserts this. Parsing caps the source before
allocation, then caps the declared/decoded payload at `MAX_LEDGER_PAYLOAD`,
checks multiplication and `usize` conversion, and only then verifies
length and digest before binary decoding. Mixed/uppercase hex, odd length,
escape sequences, trailing text and a non-canonical re-render reject. The
renderer always writes one final LF. These rules form a strict valid TOML
subset while avoiding a general TOML dependency or permissive parse tree.

“Caps the source” means: open a confined regular file, reject metadata already
over the limit, then read through a `take(MAX_LEDGER_SOURCE + 1)`-style bounded
reader and reject if the sentinel byte exists. Do not call unbounded
`read_to_string`. Scan the five ASCII envelope lines in that bounded buffer;
allocate decoded payload only after validating canonical decimal length,
`2 * payload_len`, exact hex-line length and all conversions. Metadata is an
early rejection only—the bounded read remains required because file size may
change concurrently.

The payload is the shared R3 big-endian codec over fields in declaration order:

```rust
struct LedgerPayloadV1 {
    written_by: String,
    generation: u64,
    last_operation: Option<OperationId>,
    applied: Vec<AppliedEntity>,
    one_shots: Vec<OneShotReceipt>,
    resources: Vec<ResourceRecord>,
    outputs: Vec<OutputRecord>,
    legacy: Vec<LegacyEntry>,
    pending_conflict: Option<PendingConflict>,
}
```

`LedgerV2.schema` is the envelope `schema`; all other fields are exactly the
payload above. `written_by` is the canonical Cargo package version without a
leading `v`. The decoder requires complete consumption and validates all
newtypes, relationship constraints, unique keys, sort order and the single
pending-conflict rule before constructing `LedgerV2`. Vectors used as semantic
sets/maps are sorted and duplicate-free by their key (`EntityId`, `OneShotId`,
`ResourceKey`, `ProjectPath`, `legacy_key`); declaration/order-bearing vectors
retain input order. Rendering an accepted value and parsing it must produce
byte-identical payload and envelope.

Numeric tags for every ledger-reachable sum type are frozen below. Struct field
order is its declaration order; tuple-variant fields follow source order.
`Recipe`, `Capability` and `ToolFeature` encode their canonical lowercase CLI
spelling as a string rather than a Rust discriminant, and unknown spellings
reject.

| Type | Tags in numeric order |
|---|---|
| `EntityId` | capability `0`, intent `1`, tool feature `2` |
| `EntitySpec` | capability `0`, intent `1`, tool feature `2` |
| `OwnerId` | app manifest `0`, direct config `1`, direct CLI `2` |
| `ReconcileScope` | app manifest `0`, direct config `1`, direct entity `2` |
| `CapabilityInstance` | singleton `0`, named `1` |
| `RefTarget` | managed `0`, existing `1` |
| `Referent` | resource `0`, use case `1`, event `2`, dispatcher `3`, type `4` |
| `FieldType` | scalar `0`, list `1`, map `2` |
| `ScalarFieldType` | text `0`, integer `1`, long `2`, boolean `3`, local date `4`, local date-time `5`, instant `6`, UUID `7`, currency `8`, decimal `9`, bytes `10`, duration `11`, zone ID `12`, URI `13`, path `14`, double `15`, project type `16` |
| `Optionality` | required `0`, nonblank `1`, nullable `2` |
| `NumericConstraint` | positive `0`, nonnegative `1` |
| `IndexDirection` | ascending `0`, descending `1` |
| `IntentArguments` | fields `0`, names `1`, mappings `2` |
| `TypeTargetId` | managed `0`, existing `1` |
| `SourceInputId` | project `0`, external `1` |
| `ExternalInputId` | app manifest `0`, user template `1`, cases brief `2` |
| `OneShotId` / `OneShotSpec` | field `0`, migration `1`, cases `2` |
| `OneShotState` | active `0`, retired-target-removed `1` |
| `OneShotLifecycle` | field `0`, migration `1`, cases `2` |
| `ResourceOwner` | entity `0`, one-shot `1` |
| `MavenVersion` | managed `0`, pinned `1` |
| `MavenScope` | compile `0`, runtime `1`, test `2` |
| `ResourceKey` | whole file `0`, Maven dependency `1`, Maven plugin `2`, compose service `3`, property `4`, marked block `5`, command registration `6`, human-config capability `7`, Spring test import `8` |
| `ResourceValue` | whole file `0`, Maven dependency `1`, Maven plugin `2`, compose service `3`, property `4`, marked block `5`, command registration `6`, human-config capability `7`, Spring test import `8` |
| `LegacyOwnerHint` | direct CLI `0`, unknown `1` |
| `LegacySourceKind` | schema-1 ledger `0`, schema-1 applied `1`, schema-1 model `2`, app-state header `3`, app-state row `4`, intent files `5`, model files `6`, global files `7`, version file `8` |
| `LegacySourcePath` | schema-1 ledger `0`, app state `1`, intent files `2`, model files `3`, global files `4`, version file `5` |
| `SpecPresence` | present `0`, absent `1`, unknown legacy `2` |

R1 reserves and implements the renderer, pending-conflict, deferred-intent and
effect leaves whose tags are specified next to their R3/R5 semantic sections.
R5 activates their constructors/semantic validators; it adds no schema-2 byte.
Any future variant, field semantic change, tag or sort-rule change requires a
new payload codec name and a pure migration. Never use Serde defaults, Rust
enum ordinals or `Debug` text as wire format. R1 golden fixtures include the
minimal valid ledger, one fully populated value exercising `RendererStamp`,
`Some(PendingConflict)`, every dormant effect/intent leaf and every tag,
invalid noncanonical envelopes, and the exact schema-1-to-2 result.

Models are derived from applied record/scaffold specs; delete `Ledger.models`
rather than maintain a compatibility registry. `PendingConflict` is one
project-wide optional record, defined fully in R5; it never replaces the last
successful `AppliedEntity`. `generation` starts at zero and increments for
every journaled ledger transition, including migration, conflict, finalisation
and abort. Conflict abort is a new forward generation that clears pending state
while preserving the successful logical tables; it never decrements generation
or “rolls back” a ledger. Sort
entities, owner sets, set-valued indexes/resources and outputs in the wire
format. **Never sort field/component order, ordered index columns, sealed
variants, strategy implementations or association mappings:** those orders
change generated Java/DDL behaviour and remain part of the spec.

R1 implements and tests the schema-2 codec and pure schema-1-to-2 translation,
but does not activate its writer. Loading schema 1 never writes. R4 first makes
schema-2 commit recoverable; R6 installs the compatibility facade described in
R6.1 before the first command switches. The mapping is one total pure function:

```text
translate_legacy(LegacySnapshot) -> Result<LedgerV2Draft, LegacyError>
```

`LedgerV2Draft` is exactly the five successful logical tables, with the same
canonical sorting and validation as `LedgerV2`. It deliberately has no schema
header, writer, generation, last operation or pending conflict. The first V2
commit combines this translated generation-zero logical prior with the current
typed request, then writes `LedgerV2 { schema: 2, generation: 1,
last_operation: Some(operation_id), pending_conflict: None, .. }`. No decoder
accepts the draft as an on-disk ledger.

`LegacySnapshot` contains exact bytes plus sorted directory listings for the
schema-1 `.jails/ledger.toml`, `.jails/app-state-v1`, `.jails/files`,
`.jails/version`, and every regular `*.files` child under `.jails/intents` and
`.jails/models`. Missing known sources are ordinary absence. A symlink,
non-regular known source, unknown child, unreadable byte, malformed supported
schema, unsafe/non-UTF-8 recorded project path or unsupported schema is a
`LegacyError`; translation returns nothing and cleanup is empty.

For each present source, intern its complete original bytes as `source_object`.
Then emit candidates in this closed mapping:

| Source logical record | `LegacyEntry` mapping |
|---|---|
| each present schema-1 ledger header | one `Schema1Ledger` entry with no spec/paths; exact version/header text remains in the shared source object; `UnknownLegacy`/`Unknown` |
| each schema-1 `Applied` row | `Schema1Applied`; exact ledger source/path; `raw_spec` copies recipe/name/package/ordered fields/indexes/timestamps/on/yields; exact sorted recorded files become `paths`; explicit `has_spec=true/false/unknown` becomes `Present/Absent/UnknownLegacy`; owner hint is `DirectCli` only for explicit absent, otherwise `Unknown` |
| each schema-1 `Model` row | `Schema1Model`; model name/package/ordered fields in `raw_spec` with `kind="legacy-model"`; no paths; `UnknownLegacy`/`Unknown` |
| each present app-state header | one `AppStateHeader` entry with no spec/paths; exact schema header remains in the shared source object; `UnknownLegacy`/`Unknown` |
| each app-state schema-1/schema-2 `intent=` or `legacy=` logical row | `AppState`; preserve its decoded supported values in `raw_spec` without inferring a manifest source; no paths; `Present` when its schema explicitly carries a full spec, otherwise `UnknownLegacy`; owner hint `Unknown` |
| each `.jails/intents/*.files` file | `IntentFiles`; recover only the filename's validated recipe/name hint, retain unknown package, copy its validated sorted path lines, set `Absent` and `DirectCli` |
| each `.jails/models/*.files` file | `ModelFiles`; recover only the validated name hint, store each line as an ordered raw field under `kind="legacy-model"`, no paths, `UnknownLegacy`/`Unknown` |
| `.jails/files` | one `GlobalFiles` entry with all validated sorted path lines, no raw spec, `UnknownLegacy`/`Unknown` |
| `.jails/version` | one `VersionFile` entry with no spec/paths; its exact version text remains in `source_object`, `UnknownLegacy`/`Unknown` |

Do not join records across sources merely because kind/name/package happens to
match. In particular, an app-state spec and an intent-files path list remain two
legacy entries until explicit adoption proves they belong together. This is
intentional conservatism: old state did not preserve that relation.

`LegacySourcePath` is the only type allowed to name a compatibility file under
the reserved `.jails` namespace; normal managed `ProjectPath` remains unable to
name the ledger, locks, transactions, receipts or object store. Its variant must
match `LegacySourceKind`: schema-1 header/applied/model rows use
`Schema1Ledger`, app-state rows/header use `AppState`, and each remaining kind
uses its identically named path variant. `LegacyFileName` is the complete
captured basename, must be canonical UTF-8, one non-dot component with no path
separator/platform prefix and a `.files` suffix, and must re-render to the exact
captured directory entry. The closed variant supplies its fixed parent; a
decoder never accepts an arbitrary `.jails/...` string.

For each candidate form `row_bytes` with the shared codec over its
`source_kind`, closed `source_path`, presence/owner hint, raw spec and
paths, excluding `legacy_key` and the shared source body. Its stable key is
`LegacyKey { source_kind, digest: SHA256("JAILS-LEGACY-1" || row_bytes) }`;
its CLI spelling is
`<source-kind-token>:<64-lowercase-hex-digest>`. Constructors reject a token
that disagrees with the embedded source kind. Sort by `(source_kind tag,
digest)`. Byte-identical duplicate logical records collapse; a digest
collision with unequal `row_bytes` is fatal. Never use directory enumeration
order or an ordinal in the key. The exact source object preserves duplicate
physical lines for audit even when their semantic candidate collapses.

The draft is always `schema=2`, `written_by=<current package version>`,
`generation=0`, `last_operation=None`, empty applied/one-shot/resource/output
tables, the sorted legacy entries above and no pending conflict. Human
`jails.toml` is **not** translated into observed rows: its capabilities become
`DirectConfig` desired claims during ordinary resolution, and the file is
never deleted or rewritten by migration. A schema-1 row never becomes an
`AppliedEntity` from coincidence with today's manifest/config.

On the first V2 mutation, R3 combines this draft with that operation and writes
generation 1. The schema-1 ledger is the guarded ledger before-image. Every
other known legacy source is deleted by an exact guarded `FileOp` in the same
transaction only when all its bytes parsed into the candidate/source-object
mapping above. Legacy directories are allowed to remain as empty monotonic
shells. There is no partial cleanup: any unknown/invalid source prevents all
legacy deletion and the V2 commit. Because each deleted source's exact object
is in the prepared manifest/receipt, cleanup is lossless even when the semantic
row remains deliberately ambiguous.

There is no dual write. After a successful lossless migration, schema 2 is the
only machine registry. The optional project-wide pending-conflict record is
reserved now so R5 can commit conflict markers without an incompatible ledger
redesign; R1-R4 leave it absent.

Extend the existing `jails adopt` command rather than inventing implicit
migration. `jails doctor` lists every ambiguous row's stable `LegacyKey` and a
copyable command skeleton. `jails adopt --legacy-key <key> --manifest <path>
--intent <kind:name[:package]>` selects exactly that decoded row—never “the
first matching” row—and claims it as `AppManifest`, using the selected human
source as explicit evidence, only when every expected current output exists and is
byte/mode-identical to the newly rendered candidate under the captured current
renderer/template context. Plain adoption records that exact candidate as
base/current with its truthful `RendererStamp`; it never claims an arbitrary
legacy byte was renderer-produced. A mismatch refuses and points to
`--replace --force`, which prepares newly rendered desired bytes and treats the
current bytes as guarded preimages.
`--pretend` shows the exact claim or replacement and writes nothing. A missing
key, key/source mismatch, missing path or ambiguous intent mapping refuses.
Skipping leaves the entry untouched. When old state kept a spec-bearing row
and path-only row separate, adopt the spec-bearing key first. A later command
may retire the explicitly selected path-only key only when the named already-
applied entity has an equal spec/owner claim and its exact current
`OutputRecord` path set equals that row; it adds no duplicate owner or output.
Otherwise it refuses and names the prerequisite key. This is the only route
from unknowable legacy manifest origin to a named owner, and every row is
handled explicitly rather than heuristically joined.

#### R1.5 Touchpoints and migration sequence

Primary touchpoints are new `src/desired.rs`, split
`src/app/manifest.rs`, `src/app.rs`, `src/ledger.rs`,
`src/generated_files.rs`, `src/model/mod.rs`, `src/config.rs`, `src/add.rs`,
recipe metadata under `src/generate/`, and CLI conversion in `src/main.rs`.

Implement in this order:

1. Create `src/protocol/{codec,identity,request,ledger,provenance,conflict,
   effect}.rs` with every complete ledger-reachable value/tag specified
   throughout this RFC, including dormant R3/R5 leaves. Add validating newtypes
   and the full-value schema-2 golden before a semantic constructor depends on
   the module.
2. Add canonical request constructors/codecs and convert at CLI/config/
   manifest boundaries; invalid request cross-products below reject here.
3. Split identity from spec and replace string keys in app/ledger lookups.
4. Add typed references and graph validation while adapters still feed current
   recipes.
5. Add owner/resource records and desired/observed comparison behind existing
   imperative application; compare its decisions in tests without switching
   writes yet.
6. Add strict schema-2 parse/render and in-memory schema-1 migration; remove
   the model registry only after derived-view parity tests pass.
7. Switch only `app plan` and test-only shadow reports to the typed comparison.
   R1 through R3 are plan/shadow-only in production: they may not delete an
   artifact, migrate a ledger or replace the imperative writer. R4 may exercise
   schema-2 commit only through test-only dark dispatch. R6 implements command
   adapters incrementally behind that dark path, then activates schema 2 for
   every production mutator in one top-level dispatch flip; no command-by-
   command production activation is permitted.

R1 gate:

- reordering manifest declarations, an owner set or a set-valued index emits byte-identical
  desired comparison and ledger bytes;
- duplicate identity, incompatible owners, invalid names/packages, wrong or
  ambiguous typed references, cycles, resource collisions and path escapes
  refuse before any write;
- `ProjectPath` constructor vectors reject every machine/legacy `.jails/**`
  path and descendant except the two read allowlists; operation vectors allow
  only `AppInit` to create `.jails/app.toml`, reject template-override writes,
  and require `LegacyMachine` for every compatibility deletion;
- removing one manifest capability/intent removes only that claim, while
  direct and shared claims survive; last-owner removal becomes explicit
  semantic absence;
- `HumanConfigCapability` exists exactly for a real `DirectConfig` claim:
  manifest-only and direct-CLI-only entities never copy a row into
  `jails.toml`, while removing `DirectConfig` deletes only that row when another
  owner retains the entity;
- first-V2 direct db, docker and explicit fast-test fixtures establish their
  exact already-rendered shared resources ledger-only, while unequal/partial,
  app-only and ambiguous legacy claims refuse and no generic adoption route is
  inferred;
- an active owner can replace its own spec, agreeing captured owners update
  together, an incompatible retained outside owner refuses, and prerequisite
  capabilities require real declared owners rather than inferred files;
- a direct destroy touches only its `DirectEntity` scope;
- plan remains domain-blind and byte-for-byte non-mutating on schema 1 and 2;
- a valid zero-argument manifest intent is not confused with a path-only row;
- schema-1 migration is deterministic, a schema-2 parse/render round trip is
  byte-stable, and ambiguous legacy state is preserved rather than guessed;
- no code outside the desired/ledger boundary compares composite string keys,
  infers spec presence, or reads `Ledger.models`; and
- tests cover canonical and alias keys, both-at-once refusal, package fallback
  and ambiguity, missing/wrong referents, cycles, owner union/conflict,
  scope-limited removal, model derivation and every migration rule above.

### R2 — Deterministic `ProjectSnapshot` and projected planning — SHIPPED

Gate: `jails_project::capture` reads a declared set into one snapshot and
`ProjectSnapshot::read` refuses anything undeclared; `ProjectedProject::advance`
implements §R2.4's six steps over it.

Planning must stop learning by mutating or rereading the live tree.

Current evidence: `src/model/mod.rs::Project` caches the pom, layout and some
derived values, but `record_in` and multiple planners still reach through a
root. `src/app.rs::project_at` deliberately reloads after each applied
capability because the previous step already rewrote pom/config. That is
correct for the imperative implementation and proof it cannot plan one atomic
manifest.

R2 is still shadow/plan-only. It may compare its `DesiredChangeSet` with the
old planner and power `app plan`; it does not switch a writer or enact a
desired removal before R4/R6.

#### R2.1 Snapshot values and the full read set

Create `src/planning/{mod,snapshot,projected,inputs}.rs`:

```rust
struct SnapshotFile {
    bytes: Arc<[u8]>,
    sha256: ObjectId,
    len: u64,
    mode: FileMode,
}

enum InputPrecondition {
    Absent { path: ProjectPath },
    File { path: ProjectPath, sha256: ObjectId, len: u64,
           mode: FileMode },
    Directory { path: ProjectPath, entries: Vec<ProjectPath>,
                entries_sha256: ObjectId },
    ExternalAbsent { id: ExternalInputId },
    ExternalFile { id: ExternalInputId, sha256: ObjectId, len: u64 },
    LegacyAbsent { path: LegacySourcePath },
    LegacyFile { path: LegacySourcePath, sha256: ObjectId, len: u64,
                 mode: FileMode },
    LegacyDirectory { kind: LegacyDirectoryKind,
                      state: LegacyDirectoryState },
    MachineObject { source: MachineObjectSource, object: ObjectRef },
    MachineReceipt { transaction: TransactionId, generation: u64,
                     record_checksum: ObjectId },
    MachineReceiptDirectory { state: MachineReceiptDirectoryState },
    MachineRoot { presence: MachineRootPresence },
}

enum MachineRootPresence { Absent, Present }

enum MachineReceiptDirectoryState {
    Absent,
    Present { transactions: Vec<TransactionId>, entries_sha256: ObjectId },
}

enum LegacyDirectoryState {
    Absent,
    Present { entries: Vec<LegacyFileName>, entries_sha256: ObjectId },
}

enum MachineObjectSource {
    Global,
    Receipt(TransactionId),
}

enum ExternalInputId {
    AppManifest,
    UserTemplate(TemplateId),
    CasesBrief { path_id: ExternalPathId },
}

struct ReadSet { inputs: Vec<InputPrecondition> } // canonical order below

enum Build {
    Maven,
    Foreign(ForeignBuild),
    Bare,
}

enum ForeignBuild { Gradle, Ant, Bazel }

enum Flavor { SpringBoot, PlainMaven }

enum Layer {
    Domain,
    App,
    Service,
    Web,
    Api,
    Messaging,
    Cli,
    Clients,
    Jobs,
    Adapters,
    Testkit,
}

struct Layers {
    packages: BTreeMap<Layer, Package>,
}

struct ProjectSnapshot {
    root: CanonicalRoot,
    files: BTreeMap<ProjectPath, SnapshotFile>,
    absences: BTreeSet<ProjectPath>,
    directories: BTreeMap<ProjectPath, Vec<ProjectPath>>,
    read_set: ReadSet,
    build: Build,
    base_package: Package,
    java_release: u32,
    flavor: Option<Flavor>,             // `Some` iff build is Maven
    layers: Layers,
    config: HumanConfig,
    observed: LedgerV2,
    ledger_image: InputImage,          // `Absent` is first-class
    facts: ProjectFacts,
    templates: BTreeMap<TemplateId, ResolvedTemplate>,
    objects: BTreeMap<ObjectId, Arc<[u8]>>, // verified ledger-referenced inputs
    receipts: BTreeMap<TransactionId, ReceiptV1>, // validated retained records
    external_files: BTreeMap<ExternalInputId, SnapshotFile>,
    external_absences: BTreeSet<ExternalInputId>,
}

struct LoadedSnapshot {
    snapshot: ProjectSnapshot,
    commit_context: CommitContext,       // opaque runtime bindings, no bytes
}

enum LoadedProject {
    Ready {
        loaded: LoadedSnapshot,
        declarations: DeclarationSyntax,
    },
    Pending {
        loaded: LoadedPendingSnapshot,
    },
}

struct LoadedPendingSnapshot {
    snapshot: PendingSnapshot,
    commit_context: CommitContext,
}

struct PendingSnapshot {
    root: CanonicalRoot,
    files: BTreeMap<ProjectPath, SnapshotFile>,
    absences: BTreeSet<ProjectPath>,
    external_files: BTreeMap<ExternalInputId, SnapshotFile>,
    external_absences: BTreeSet<ExternalInputId>,
    read_set: ReadSet,
    observed: LedgerV2,                  // contains exactly one PendingConflict
    ledger_image: InputImage,
    objects: BTreeMap<ObjectId, Arc<[u8]>>,
    receipts: BTreeMap<TransactionId, ReceiptV1>,
    origin_transaction: TransactionId,
}

struct DeclarationSyntax {
    human_config: HumanConfigSyntax,
    app_manifest: Option<AppManifestSyntax>,
}

struct HumanSourceSelection {
    manifest: Option<SelectedInputPath>,
    user_template_root: Option<PathBuf>,  // resolved once at CLI boundary
}

enum SelectedInputPath {
    Project(ProjectPath),
    External(PathBuf),                   // loader boundary only; never encoded
}

enum InputImage { Absent, File(SnapshotFile) }

enum InputRequest {
    RequiredFile(ProjectPath),
    OptionalFile(ProjectPath),
    Directory(DirectoryRequest),
    ExternalTemplate(TemplateId),
    ExternalCasesBrief { path_id: ExternalPathId },
}

struct DirectoryRequest {
    root: ProjectPath,
    extensions: BTreeSet<String>,
    recursive: bool,
    excludes: BTreeSet<ProjectPath>,
}

struct InputSet { requests: BTreeSet<InputRequest> }

struct SnapshotFingerprintV1 {
    read_set: ReadSet,
    templates: Vec<SnapshotTemplateRef>,
}

struct SnapshotTemplateRef {
    id: TemplateId,
    origin: TemplateOrigin,
    source_object: ObjectRef,
    required_keys: BTreeSet<TemplateKey>,
}

enum FactKind {
    Pom,
    HumanConfig,
    Compose,
    Properties(ProjectPath),
    JavaSource(ProjectPath),
}

enum FactSourceState {
    Absent,
    Present { sha256: ObjectId, len: u64 },
}

struct ProjectFacts {
    sources: BTreeMap<FactKind, FactSourceState>,
    values: BTreeMap<ProjectFactKey, ProjectFact>,
}

enum ProjectFactKey {
    MavenDependency(MavenCoordinate),
    MavenPlugin(MavenCoordinate),
    ComposeService(ServiceName),
    Property { path: ProjectPath, key: PropertyKey },
    MarkedBlock { path: ProjectPath, marker: MarkerId },
    CommandRegistration { dispatcher: JavaType, command: JavaType },
    HumanConfigCapability(CapabilityId),
    JavaType(JavaType),
}

enum ProjectFact {
    MavenDependency(DependencySpec),
    MavenPlugin(PluginSpec),
    ComposeService(ComposeServiceSpec),
    Property(String),
    MarkedBlock { body_sha256: ObjectId },
    CommandRegistration,
    HumanConfigCapability(CapabilitySpec),
    JavaType(JavaTypeFact),
}

enum JavaTypeKind { Class, Record, Interface, Enum }

struct JavaTypeFact {
    source: ProjectPath,
    kind: JavaTypeKind,
    supertypes: Vec<JavaType>,
    constructor: Vec<JavaParameterFact>,
    enum_constants: Vec<Name>,
}

struct JavaParameterFact {
    name: Name,
    type_expression: JavaTypeExpression,
}

enum JavaTypeExpression {
    Primitive(JavaPrimitive),
    Declared { qualified_name: JavaType, arguments: Vec<JavaTypeArgument> },
    TypeVariable(Name),
    Array(Box<JavaTypeExpression>),
}

enum JavaTypeArgument {
    Exact(JavaTypeExpression),
    Extends(JavaTypeExpression),
    Super(JavaTypeExpression),
    Unbounded,
}

enum JavaPrimitive { Boolean, Byte, Short, Int, Long, Char, Float, Double }

struct ResolvedTemplate {
    id: TemplateId,
    origin: TemplateOrigin,
    source: Arc<str>,
    source_object: ObjectRef,
    required_keys: BTreeSet<TemplateKey>,
}
```

The block shows the target snapshot. During delivery, R2 initially omits the
`receipts` field because `ReceiptV1` depends on R3's prepared identity. R4 adds
the field, the receipt-first capture below and receipt-local object resolution
before effect retry or R5 conflict finalise/abort is reachable. Before that
addition, machine-object resolution is global-only and no production schema-2
writer is active. Do not introduce an opaque placeholder receipt or duplicate
header DTO merely to make the phase compile.

`JavaTypeExpression` is the closed Java type grammar the source parser accepts.
It preserves generic argument order; every declared name is resolved to a
qualified `JavaType`, and type variables remain distinct. It is not an
arbitrary source fragment. Expand its variants and parser tests together when a
real project needs another type form.

Every `ProjectFactKey` may pair only with the same-named `ProjectFact` variant;
constructors and decoders enforce this, and duplicate keys reject. `sources`
is the explicit absence/presence authority for each parser input, so deleting a
POM/properties/Java source cannot leave stale values. Snapshot scalar facts
(`build`, base package, release, flavour, layers and parsed human config) remain
the single named fields on `ProjectSnapshot`; do not duplicate them in the
keyed map.

`Layers.packages` contains every `Layer` exactly once. `Build::Foreign` is
closed to the three detected classes above; a new foreign build is a model and
codec change, not an arbitrary persisted string. `flavor` is `Some` exactly
when `build == Build::Maven`; foreign/bare projects use `None`. A planner that
needs Maven flavour first requires `Build::Maven` and otherwise refuses—it
must not call a foreign or bare project “plain Maven” to fill a field.

`ResolvedTemplate.source` is the exact selected UTF-8 body and must hash/size
to `source_object`; required placeholder keys are set-valued. Resolution
precedence is project override, then user override, then built-in. Project
origins persist a `ProjectPath`; user origins persist only the logical template
ID and keep their absolute binding in `CommitContext`. Recipe metadata declares
every `TemplateId` up front. Replace the current process-global `OnceLock`
catalog with this invocation-local snapshot value; a template selected for one
project/invocation can never leak into another.

Canonical `ReadSet` order is project absent/file/directory by `ProjectPath`,
then external absent/file rows by `ExternalInputId`, legacy absent/file rows by
`LegacySourcePath`, the two legacy-directory rows by `LegacyDirectoryKind`,
machine objects by `(MachineObjectSource, ObjectId)` with global before receipt
and receipt sources ordered by `TransactionId`, and machine receipts by
`TransactionId`, followed by the sole machine-receipt-directory row and sole
machine-root row; enum discriminant precedes the key. An external
ID occurs exactly once across present and absent rows. Each applicable legacy
source occurs exactly once across absent/file rows; the two directory kinds
occur exactly once. A present legacy-directory entry has one matching
`LegacyFile` and no extra legacy file is legal. Its digest is
`SHA256("JAILS-LEGACY-DIRECTORY-1" || encode(kind, entries))`; absent state has
no invented listing hash. Machine-receipt transactions sort uniquely and their
directory present-state `entries_sha256` is
`SHA256("JAILS-RECEIPT-DIRECTORY-1" || encode(transactions))`; absent state has
no invented empty-directory hash.

`snapshot_fingerprint` is exactly
`SHA256("JAILS-SNAPSHOT-1" || encode(SnapshotFingerprintV1))`. The template
rows sort uniquely by `TemplateId`; built-ins therefore affect the fingerprint
even though they have no filesystem precondition. Every other parsed/scalar
fact is a deterministic function of `ReadSet` bytes and is deliberately not
encoded again. Runtime absolute bindings and root device/inode are excluded;
the journal validates the latter separately. Golden vectors prove that a file
hash, recorded absence, directory entry, template origin/body or placeholder
contract changes the fingerprint while map insertion order does not.
Directory entry lists are direct-child relative paths sorted by raw UTF-8 byte
order. Recursive scope is represented by one precondition for every visited
directory, including empty directories, so a new source appearing after load
is stale input rather than invisible. File order, manifest order and map seed
do not affect it. `entries_sha256` is exactly
`SHA256("JAILS-DIRECTORY-1" || encode(entries))`; the empty directory hashes a
canonical zero-count vector.

`CanonicalRoot` stores the canonical path and platform file identity of the
root. The loader rejects symlinks for managed files and managed directory
components; it never follows a link outside or inside the project and then
pretends the target is managed. It reads metadata before and after each file;
file-kind/identity change during the read retries the whole snapshot once and
then returns `project changed while loading`. Hashes, not mtimes, are commit
preconditions. Modes contain only portable executable permission intent; owner
IDs, timestamps and inode numbers do not enter deterministic fingerprints.

`ExternalFile`/`ExternalAbsent` are allowed only for the selected external app
manifest, metadata-declared user template candidates and a `cases` brief
explicitly supplied by the command. A present input is read once, hashed and
rechecked at commit because it affects after-bytes; an optional missing
candidate records `ExternalAbsent` and is rechecked with `symlink_metadata` so
creation between load and commit makes the plan stale. Neither form is ever a
mutation target. An external manifest uses
`ExternalInputId::AppManifest`; a project-internal manifest uses the ordinary
`File` precondition for its `ProjectPath`. Absolute bindings stay in R3's
runtime-only `CommitContext`, not the read-set value. Do not snapshot process
CWD, clock, random state or environment. Tool executable/version fingerprints
belong to R3's `PreparationContext`.

`load(ProjectHandle, HumanSourceSelection, DirectRequest)` returns one
`LoadedProject`: ordinary input becomes `Ready`, while an already committed
pending conflict becomes the deliberately smaller, parse-free `Pending` mode
defined in R2.2. `HumanConfigSyntax` and
`AppManifestSyntax` are closed parser DTOs that retain user omissions/aliases;
they never reach recipe planners. `HumanSourceSelection`, raw `DirectRequest`
and `SelectedInputPath::External` are invocation-boundary values only. The CLI
boundary selects at most one user-template root. If selected, that root must
exist as a real directory: the loader canonicalises it once, records its
device/inode in runtime-only `ExternalBinding::ConfinedCandidate`, maps each
validated metadata `TemplateId` to a confined relative child, and walks every
currently existing parent/leaf with `symlink_metadata` without following a
link. The binding is `(canonical root identity, validated relative child)` even
when the leaf or an intermediate component is absent; an absent leaf cannot be
“canonicalised.” An explicitly configured missing/non-directory/symlink root
refuses; an unconfigured user layer is `None` and has no candidate.

Commit rechecks the root identity, repeats the confined no-symlink component
walk, and then checks the exact `ExternalFile` bytes or `ExternalAbsent` leaf.
A new leaf makes absence stale; a changed root or symlink is refusal. Existing
external manifest/cases inputs use `ExternalBinding::ExactFile` and retain
their canonical leaf. Neither root nor absolute child path enters a codec.

Ordinary resolution and planning receive only the `Ready` snapshot plus its
captured syntax; pending resolution receives only `PendingSnapshot` and the
parsed request-syntax fingerprint. Neither can see an absolute source path.
Preparation consumes the matching `LoadedProject` variant and moves its
`commit_context` unchanged into `PreparedBundle`. `CommitPlan::Apply` requires
`Ready`; `Finalise` and `Abort` require `Pending`; every other pairing is an
internal invariant error. The runtime map is excluded from equality,
fingerprints, reports and every codec.

`CommitContext.project_root` is the loader's exact `CanonicalRoot` device/inode
identity with no absolute path. Before lock bootstrap or any managed-project
write, commit resolves the supplied `ProjectHandle` again without following a
symlink and requires the same identity; a mismatch is `StaleInput`. The first
validated journal copies that identity into `JournalV1.root_identity`, and every
recovery call compares its own resolved handle with the journal before trusting
paths or objects. Thus deterministic fingerprints remain machine-independent
without leaving root recheck authority behind in an earlier stack frame.

#### R2.2 Deterministic bootstrap and input closure

The loader cannot ask a planner which inputs it needs before it knows the
project. It also cannot parse ordinary bootstrap files before checking the
ledger: a valid committed conflict may intentionally leave a POM, human config,
manifest or source file containing markers. Use this fixed, ledger-first
algorithm:

1. Resolve `ProjectHandle` to one canonical project root without changing CWD.
2. Capture `.jails` itself as the sole `MachineRoot` presence row. When
   present it must be a real directory and its device/inode goes only into
   `CommitContext.machine_root`; when absent no ordinary project-directory
   absence or `DirectoryOp` is created for it. Capture
   `.jails/ledger.toml` immediately as a confined exact file or absence, and
   strictly decode only that ledger. Do not parse the build, human config,
   manifest, compose, properties or source tree yet.
3. Capture `.jails/receipts` as the one `MachineReceiptDirectory` row before
   resolving ledger objects. Absence is distinct from a present empty
   directory; the latter contains the sorted transaction-directory listing and
   its digest. Decode every retained `receipt.bin` and sibling Complete
   `journal.bin` under the R4
   limits, validate their record checksums/link and byte-identical transaction,
   generation and prepared identity, then resolve every prepared-manifest
   member from that receipt's own confined `objects` directory or the global
   content-addressed store. Never use a different, not-yet-validated receipt to
   make this pair valid. Verify kind/length/SHA-256 and run the complete durable
   validator before inserting `ReceiptV1`. Add one `MachineReceipt`
   precondition using its transaction, generation and receipt record checksum;
   that checksum binds the Complete journal through the link. Unknown top-level
   names, symlinks and non-directories refuse. Receipt temp files inside a
   valid receipt directory are never inputs. A corrupt retained receipt or
   listing change is an error/stale input, not a reason to skip history. The
   bounded retention rule keeps this inventory finite.
4. Resolve the complete transitive closure of object references in the decoded
   ledger, including every pending-conflict and historical applied object. For
   each object, prefer a verified confined global object. If none exists during
   the pre-R5 dark R4 period, select the lowest-`TransactionId` fully validated
   retained receipt whose own manifest and local object set contain it.
   Content identity is still length plus SHA-256; two verified locations cannot
   disagree. Store the bytes in the selected snapshot mode's object map and add
   `MachineObject { source, object }`, pinning the exact selected location.
   Commit reopens and rehashes that global or receipt-local location. Missing or
   corrupt bytes at the selected source are stale/corrupt input even if a new
   duplicate appears elsewhere; the command reloads instead of switching
   sources after planning. Missing required bytes in all eligible sources fail
   loading, and no planner performs a later object-store read. R5 eliminates
   this transitional fallback for new transactions by promoting every
   ledger-reachable object globally before the ledger commit point.
5. If the ledger contains `PendingConflict`, take the **pending fast path** and
   return `LoadedProject::Pending`; never fall through to ordinary bootstrap
   parsing. Require exactly one receipt whose immutable prepared conflict
   structure equals the complete pending record under R5.4, pin its
   transaction/checksum, and load its complete object-manifest closure. Capture
   raw bytes/presence for the sorted union of every pending conflict path, every
   frozen non-conflict postimage, every affected path in the origin receipt,
   and every current human input named by `desired_inputs`. Capture allowed
   external inputs through the same exact bindings as normal mode. Recompute
   `RequestSyntaxFingerprint` from the current CLI without project-derived
   parsing and require it and `ManifestSourceId` to match the pending
   invocation. Only after that equality may the pending resolver reuse the
   stored `CanonicalMutationRequest`; it never re-resolves package/default facts
   from marker-bearing files. Exact read-only desired-input rows are rehashed
   from current raw bytes, and every `Absent` row is recaptured and required to
   remain absent. A `ProjectedTransactionOutput` row reuses its stored projected digest
   only after the named path is proven to be one of this transaction's frozen
   clean or conflict paths; its current content is guarded by that path's own
   clean/marker/resolution rule. Any row that is neither independently captured
   nor transaction-guarded is corrupt state. Freeze that smaller read set. Do
   **not** parse `Build`, `HumanConfig`, app manifest, general Java facts,
   templates or any file that may contain markers.
6. Pending abort uses only the pinned receipt, exact raw images and request/input
   guards. Pending continue first checks every frozen path. When all conflict
   paths are marker-free, it parses only each resolved shared-format path with
   its frozen `FormatOwner` and validates the exact candidate-owned semantic
   slots from R5.4; a whole-generated file needs no domain parse. This narrow
   parse cannot expand the read set. Thus a marker-bearing bootstrap file can
   always reach guarded continue/abort, while malformed resolved shared syntax
   still refuses finalisation without mutation.
7. With no pending conflict, close the legacy machine namespace before ordinary
   project bootstrap. Without a schema-2 ledger, capture the complete
   `LegacySnapshot`: add `LegacyAbsent` or `LegacyFile` for each static legacy
   path, exactly one `LegacyDirectory` row for each of `Intents` and `Models`,
   and a `LegacyFile` for every validated child named by a present directory
   row. Directory entries are safe `LegacyFileName`s sorted by raw UTF-8 bytes;
   unknown children, links, non-regular files and a listing/file mismatch
   refuse. Intern every present source body and run `translate_legacy` purely.
   The resulting `LedgerV2Draft` supplies the generation-zero observed logical
   tables in memory. A supported schema-1 ledger's exact image is both the
   ordinary `ledger_before` and the `LegacySourcePath::Schema1Ledger` source;
   absence must agree in both places. Loading neither deletes a source nor
   writes the translated value.

   With a schema-2 ledger, `Schema1Ledger` is inapplicable because that same
   path is the current ledger. Require every other static legacy file absent
   and each legacy directory absent or present-and-empty, recording those
   `LegacyAbsent`/`LegacyDirectory` rows for commit recheck. Any reintroduced
   legacy child or file—such as output from an older binary—is conflicting
   machine state and fails closed; an empty retained `intents`/`models`
   directory is allowed by the monotonic-directory policy.
8. Capture the remaining required/optional bootstrap
   paths: detected build descriptor, `jails.toml`, selected app manifest,
   `compose.yaml`, `src/main/resources/application.properties`, layer package
   directories and project template-override directory. Record absence for
   every optional path. Parse `Build`, `HumanConfig` and manifest syntax only
   from these captured bytes. A parse error stops here; no migration.
9. From syntax recipe/capability kinds and the parsed direct-request kind,
   union their metadata `InputSet`s with fixed format-module inputs. Capture an
   explicitly selected external cases brief here. For each required template,
   capture the project-override candidate first. When it is absent and a user
   template root was selected, capture that exact external candidate as
   `ExternalFile` or `ExternalAbsent` before falling back to the built-in.
   Thus every higher-precedence absence that selected a lower-precedence
   template is part of the read set. Enumerate each directory scope once in
   sorted order, rejecting `.git`, `target`, `.jails/transactions`, object
   storage and any symlink.
10. Build the Java/source `ProjectFacts` index from captured source bytes. Any
   recipe that may resolve an existing type declares the containing main/test
   source roots in syntax metadata, so discovery never causes a later read.
11. Freeze the sorted `ReadSet`, derive all parsed facts and return
    `LoadedProject::Ready` with the immutable snapshot and syntax.

After ordinary loading, `desired::resolve` resolves typed references and the
canonical direct request using only the ready snapshot. Pending loading has its
own closed resolver and can return only `ContinueConflict` or `AbortConflict`;
it never constructs an ordinary desired graph. Before ordinary planning, ask each resolved
recipe for its final `InputSet` and require it to be a subset of the captured
syntax-metadata/fixed-format set. A missing exact file or directory scope is
the internal error `recipe metadata omitted input`; do not silently expand or
rerun loading. `RefTarget::Existing` points to an already captured source and
hash. This single bounded superset plus subset assertion makes completeness
testable without an unbounded fixed point or resolving the same graph twice.

The bootstrap never copies `.git`, `target`, dependency caches or an entire
project by default. A recipe that scans all Java sources declares the main/test
source roots explicitly. An optional file request records `Absent`; a required
one errors. A directory request records the listing even when no matching file
exists.

All parsers take `&[u8]`/`SnapshotView`, never a root path. `SnapshotView`
exposes `read`, `exists` and `list`; a request absent from `ReadSet` is an
internal error. Template discovery becomes
`snapshot.templates.resolve(TemplateId)` and freezes built-in/project/user
origin once. A test-only spy makes any filesystem/environment/process call from
a planner fail.

#### R2.3 Desired change and resource ownership

Evolve the existing `model::Change`; do not build a permanent peer beside it.
During migration `type Change = DesiredChange` may keep call sites compiling,
but that alias is deleted in R6.

```rust
struct DesiredChange {
    attribution: ChangeAttribution,
    resources: Vec<DesiredResource>,
    files: Vec<DesiredFile>,
    edits: Vec<SemanticEdit>,
    absences: Vec<ManagedPath>,
    preconditions: Vec<SemanticPrecondition>,
    fact_delta: FactDelta,
}

enum ChangeAttribution {
    Resource(ResourceOwner),
    Maintenance(MaintenanceAttribution),
}

enum MaintenanceAttribution {
    AppInit,
    Rename,
    AdoptLayout,
    AdoptLegacy,
    Format,
}

struct DesiredChangeSet {
    ordered: Vec<DesiredChange>,
    subject: PlannedSubject,
    ledger_intent: LedgerIntent,
}

enum PlannedSubject {
    Reconcile(DesiredState),
    ApplyOneShot { id: OneShotId, spec: OneShotSpec },
    DestroyCases { id: OneShotId, force: bool },
    AppInit { target: ProjectPath },
    Rename { from: JavaType, to: JavaType, force: bool },
    AdoptLayout,
    AdoptLegacy { legacy_key: LegacyKey, intent: IntentId,
                  replace: bool, force: bool },
    Format { scopes: BTreeSet<ProjectPath> },
}

enum PlannedTransition {
    Commit(CommitPlan),
    RetryEffect(EffectRetryPlan),
}

enum CommitPlan {
    Apply(DesiredChangeSet),
    Finalise(FinalisationPlan),
    Abort(AbortPlan),
}

struct ConflictOrigin {
    operation: OperationId,
    transaction: TransactionId,
    generation: u64,
    receipt: ReceiptGuard,
    pending: PendingIdentity,
}

struct ReceiptGuard {
    transaction: TransactionId,
    generation: u64,
    record_checksum: ObjectId,
}

struct FinalisationPlan {
    origin: ConflictOrigin,
    resolutions: Vec<ResolutionIdentity>,
    effect_intents: Vec<DeferredEffectIntent>,
}

struct AbortPlan {
    origin: ConflictOrigin,
    restores: Vec<RestoreIdentity>,
}

struct EffectRetryPlan {
    invocation: InvocationFingerprint,
    receipt: ReceiptGuard,
    operation: OperationId,
    effect_index: u32,
    effect_id: EffectId,
    effect: PostCommitEffect,
    expected_state: EffectState,
    reason: EffectResumeReason,
}

enum EffectResumeReason { Interrupted, ExplicitRetry }

struct DesiredFile {
    path: ProjectPath,
    body: DesiredBody,
    mode: Option<FileMode>,              // desired policy; resolved by preparation
    resource: Option<ResourceKey>,
}

enum DesiredBody {
    Bytes(Arc<[u8]>),
    Render { template: TemplateId, bindings: TemplateBindings },
}

struct TemplateBindings(BTreeMap<TemplateKey, TemplateValue>);

enum TemplateValue {
    Text(String),
    Name(Name),
    Package(Package),
    JavaType(JavaType),
    Boolean(bool),
    Ordered(Vec<TemplateValue>),
}

struct ManagedPath {
    path: ProjectPath,
    resource: ResourceKey,
    force: bool,
}

enum SemanticEdit {
    MavenDependency { key: ResourceKey, value: DependencySpec },
    MavenPlugin { key: ResourceKey, value: PluginSpec },
    ComposeService { key: ResourceKey, value: ComposeServiceSpec },
    Property { key: ResourceKey, value: String },
    MarkedBlock { key: ResourceKey, body: String },
    CommandRegistration { key: ResourceKey, command: JavaType },
    HumanConfigCapability { key: ResourceKey, spec: CapabilitySpec },
    HumanConfigLayout { layer: Layer, directory: String }, // one validated relative component
    // One `@TestConfiguration` imported into one `@SpringBootTest`. `statement`
    // is the `import` line the annotation needs when the config lives in
    // another package, already rendered, and empty when it does not.
    SpringTestImport { key: ResourceKey, class: JavaType, statement: String },
}

enum SemanticPrecondition {
    RequiresCapability(CapabilityId),
    RequiresFact(ProjectFactKey),
    ResourceOwned(ResourceKey),
    ResourceUnclaimed(ResourceKey),
}

struct FactDelta {
    add: BTreeMap<ProjectFactKey, ProjectFact>,
    remove: BTreeSet<ProjectFactKey>,
}

struct LedgerIntent {
    generation_before: u64,
    entities_after: Vec<DesiredAppliedEntity>,
    one_shots_after: Vec<DesiredOneShotReceipt>,
    resources_after: Vec<DesiredResource>,
    legacy_after: Vec<LegacyEntry>,
}

struct DesiredAppliedEntity {
    id: EntityId,
    owners: BTreeSet<OwnerId>,
    spec: EntitySpec,
}

struct DesiredOneShotReceipt {
    id: OneShotId,
    spec: OneShotSpec,
    state: OneShotState,
    lifecycle: OneShotLifecycle,
}

enum ResourceKey {
    WholeFile(ProjectPath),
    MavenDependency(MavenCoordinate),
    MavenPlugin(MavenCoordinate),
    ComposeService(ServiceName),
    Property { path: ProjectPath, key: PropertyKey },
    MarkedBlock { path: ProjectPath, marker: MarkerId },
    CommandRegistration { dispatcher: JavaType, command: JavaType },
    HumanConfigCapability(CapabilityId),
    // Keyed by the file *and* the class: the same capability imports the same
    // config into every `@SpringBootTest` there is, and each of those is an
    // independent claim.
    SpringTestImport { path: ProjectPath, class: JavaType },
}

struct DesiredResource {
    key: ResourceKey,
    owners: BTreeSet<ResourceOwner>,
    value: ResourceValue,
}

struct ProjectedResource {
    value: ResourceValue,
    owners: BTreeSet<ResourceOwner>,
}
```

`DesiredFile.mode` is the only optional mode in the mutation model. `Some`
means the recipe explicitly requires those permission bits. Preparation
resolves `None` deterministically: a replace preserves the concrete captured
live mode, while a create uses `0o644`; a recipe that creates an executable
must request its concrete mode (normally `0o755`). Every `SnapshotFile`, read
precondition, stored/live/guarded image and prepared file after-image therefore
carries one concrete `FileMode`. The supported R4 platform is Unix; capture is
`metadata.mode() & 0o777`, and invalid extra bits reject. No prepared byte or
mode depends on process umask.

`ResourceKey` is the deletion unit. Compatible identical values union owner
sets; different values for one key are a planning conflict naming every
contributor. Removing one owner retains the resource until the last owner is
gone. A shared file is not itself deleted while any contained resource or
unmanaged byte remains; R3 asks its format owner to render the remaining
resource map. Whole-file ownership is reserved for fully generated files and
cannot coexist with semantic contribution ownership at the same path.

Field one-shots are durable overlays, not edits that disappear the next time
their managed target is rendered. For every active `OneShotId::Field`, planning
renders the target's current `IntentSpec`, applies all active field overlays in
sorted `OneShotId` order, and only then refreshes derivatives. The field
receipt's `OneShotLifecycle::Field` partitions every resource it owns into two
sorted, disjoint sets. `target_coupled` contains source/derivative
contributions that have no meaning without the target; `append_only` contains
forward migrations or other historical files that must survive target removal.
Every resource owned by that field receipt appears in exactly one set, and
every listed key is either owned by that receipt or, for a retired row, is a
remembered target-coupled key removed by the retirement transition. These are
durable facts; removal never reclassifies a path using current renderer
metadata.

Removing a managed target first applies the ordinary retained-dependant rule,
then atomically changes each active field receipt for that exact target to
`RetiredTargetRemoved`, removes only its `target_coupled` owner contributions,
and leaves its `append_only` resources and files owned by the retired receipt.
The report lists every retired field. Recreating a target does not silently
reactivate history. A later identical `generate field` may explicitly
reactivate the retired row, rebuilding only target-coupled resources and never
allocating a second forward migration; a changed spec for the same ID still
refuses. `Migration` and `Cases` lifecycle variants must always be `Active`,
have no hidden field partition, and match their ID/spec discriminants. Durable
ledger validation enforces this matrix and rejects an orphan field owner,
overlapping sets, or a retired field that still owns a target-coupled resource.

`LedgerIntent` is semantic, not serialised ledger bytes. R2 decides desired
entity/owner/resource/one-shot state and carries untouched legacy rows; it does
not invent output hashes, provenance, pending-conflict state, operation IDs or
a final wire image. R3 alone combines this intent with exact postimages and
renders `LedgerV2` bytes.

Every resource contribution is owner-local. `AppAggregate` is not an owner and
does not appear in the type system: app planning produces an ordered set of
entity/one-shot-attributed changes, and composition derives aggregate owner
sets. A resource-attributed change may emit only resources whose owner set
contains that owner; its generated files have `resource: Some(key)`. A
maintenance-attributed change never enters a `ResourceRecord.owners` set. It
may propose `Some(key)` rows owned only by real entities/one-shots when its
subject-specific validator and `LedgerIntent` describe the exact identity
transition (for example rename or legacy adoption), or use `resource: None`
for a deliberately unowned human/shared file such as a newly initialised app
manifest or standalone formatted source. It can never use its maintenance tag
as a contributor. An unowned file is
guarded/reported/receipted but creates no `OutputRecord`; it cannot later be
removed by owner reconciliation. `PlannedSubject` supplies the exact
maintenance identity and arguments, so attribution is not an authority for
semantics.
`TemplateBindings` is a closed typed binding tree—never an untyped template
map; map keys sort, while each `Ordered` value preserves semantic order. R3
combines it with resolved project/entity/reference facts to construct the full
durable `RendererContextV1`; the planner cannot supply that provenance object
or its hash.

`plan_all(snapshot, resolved)` returns exactly one `PlannedTransition`. The
rules are:

1. If pending state exists, only its matching `ContinueConflict` or
   `AbortConflict` action may produce `Commit(Finalise)` or `Commit(Abort)`
   after the R5 receipt/path checks. Every other action—including retrying an
   older failed effect—refuses. Pending commands never pass through ordinary
   reconciliation.
2. With no pending conflict, an invocation that exactly matches one eligible
   `Deferred`, `Pending`, first-attempt `Running`, or `Failed` effect returns
   `RetryEffect` and no `DesiredChange`; multiple matches refuse.
3. Every remaining action produces `Commit(Apply)` with the matching closed
   `PlannedSubject`.
4. `DesiredChangeSet.subject` must be the canonical projection of the input
   `ResolvedAction`; planning may normalise values but may not change action
   class. `LedgerIntent` describes exactly the result of its ordered changes.
5. `FinalisationPlan`, `AbortPlan`, and `EffectRetryPlan` carry the pinned
   receipt checksum. They do not depend on a later receipt-directory scan.

The ordinary-subject postconditions are also closed:

| `PlannedSubject` | Required `DesiredChangeSet` result |
|---|---|
| `Reconcile` | Entity/owner/resource/one-shot tables equal scoped desired-vs-observed reconciliation; no unrelated owner is removed. Removing a managed field target retires exactly its active field overlays and removes only their recorded target-coupled contributions; append-only contributions remain. |
| `ApplyOneShot` | Field: add or idempotently retain the active named receipt and its exact lifecycle partition; the identical command may reactivate `RetiredTargetRemoved` without allocating another append-only migration, while same ID/different spec refuses. Migration: add or idempotently retain exactly the named active receipt. Cases: the same `SourceInputId` may replace `source_hash` and regenerated bytes, updating the existing active receipt/output through stored-base reconciliation; the output path is immutable for that ID, and a changed derived path refuses with an explicit destroy-then-generate instruction. |
| `DestroyCases` | Only a cases one-shot is legal; remove exactly its receipt/contributions and reconcile its guarded output. Field/migration IDs reject before planning. |
| `AppInit` | Exactly one unowned create for the validated absent manifest plus required parent directories; entity, one-shot, resource, output and legacy tables stay unchanged. No replace/force form exists. |
| `Rename` | Rewrite the selected Java identity, every typed managed reference and affected real owner/resource/output identity as one transition. Re-render every managed entity/dependant under the new typed IDs/context and reconcile from its stored base, producing truthful new base/current/`RendererStamp`; a moved managed source lowers to guarded target create plus old delete in the same result. Unmanaged source/reference edits remain unowned maintenance receipts. Unrelated text matches are warnings/refusals, never blind replacements. |
| `AdoptLayout` | One surgical human-config layout edit and its projected fact changes; it creates no fake resource owner. If `jails.toml` already has a managed `OutputRecord`, retain that record's contributors, generated base and renderer, and advance only `current` to the exact committed postimage because the layout declaration is a human delta. If no managed config output exists, the edit remains unowned and creates no `OutputRecord`. |
| `AdoptLegacy` | Consume exactly the named legacy row only after the selected intent and current/replace base policy are proven; create only real entity/resource/output rows and retain all other legacy rows byte-for-value. |
| `Format` | Only declared scopes may change. For an existing managed output update `current` to the formatted live image while retaining its generated `base`, contributors and renderer; an unmanaged source stays unowned and creates no output row. |

Preparation asserts these subject postconditions against the final ledger
draft before hashing. Adding a subject requires a new enum variant, one row
here, codec tags if identity-reachable, planner tests and prepared-kind
negative tests; a generic maintenance callback is forbidden.

For `PlannedSubject::Reconcile`, ordering is explicit:
`DesiredChangeSet.ordered` contains capability additions
and updates in topological prerequisite order with `CapabilityId` tie-break;
persistent intent additions/updates in topological `RefTarget::Managed` order
with `IntentId` tie-break; one-shots after their targets; removals in reverse
topological order of observed managed references with reverse `EntityId`
tie-break; and the format semantic effect last. Other subjects define their
order in their own planner and may not be mixed into this graph ordering.
Field/component, sealed
variant, ordered-index and mapping order inside a spec is preserved and never
used as a planning tie-break.

#### R2.4 Projected project and cache invalidation

```rust
struct ProjectedProject {
    base: Arc<ProjectSnapshot>,
    overlay: BTreeMap<ProjectPath, ProjectedEntry>,
    resources: BTreeMap<ResourceKey, ProjectedResource>,
    facts: ProjectFacts,
    build: Build,
    base_package: Package,
    java_release: u32,
    flavor: Option<Flavor>,
    layers: Layers,
    config: HumanConfig,
    fact_dependencies: BTreeMap<FactKind, BTreeSet<ProjectPath>>,
}

enum ProjectedEntry {
    File(SnapshotFile),
    Deferred { body: DesiredBody, facts: FactDelta },
    Deleted,
}
```

Advance the projection after each `DesiredChange`:

1. Compose its resources and reject an incompatible `ResourceKey`.
2. Apply pure in-process format owners (pom, config, compose, properties and
   marked codemods) once to the current projected bytes. A materialised
   `DesiredBody::Bytes` enters the overlay as exact bytes. A
   `DesiredBody::Render` remains `ProjectedEntry::Deferred`; R3 renders it
   exactly once. Never eagerly render a template here and render it again in
   preparation.
3. Mark an absence as `Deleted`; never remove a parent directory implicitly.
4. Invalidate every `FactKind` whose dependency set contains a changed path or
   directory listing.
5. Reparse invalidated pom/config/compose/properties/Java facts from projected
   bytes in `FactKind` enum order. A deleted input yields that parser's explicit
   `FactSourceState::Absent`, not stale cache. Recompute the corresponding
   projected scalar (`build`, package/release/flavour, config or layers) when
   its owning input changes, so a later change in the same aggregate observes a
   prior `HumanConfigLayout` or POM edit rather than the base snapshot value.
6. Apply the recipe's `FactDelta` and assert it equals the reparsed facts for
   known bytes. A disagreement is an internal planner error. Formatter-deferred
   files may defer byte layout only; their declared Java type/resource facts
   must already be complete and formatting is forbidden to change semantics.

Later planners read only the projection and thus see an earlier generated
record, newly added dependency, updated package layout or deleted entity.
Projection never invokes a formatter or mutating subprocess. R3 materialises
those exact deferred operations in scratch.

Retain current `Project` temporarily as a read-only facade over `SnapshotView`.
It may expose cached facts but no root/CWD/filesystem access. Migrate a recipe
only after golden bytes and declared-input tests match. New recipes take
`&ProjectedProject`, never `root: &Path`; delete the facade in R6.

Primary touchpoints: new `src/planning/`, `src/model/mod.rs`, `src/template.rs`,
`src/pom.rs`, `src/config.rs`, `src/compose.rs`, `src/properties.rs`,
`src/java.rs`, recipe planners under `src/generate/`, capability planners under
`src/add/`, and `src/app.rs`.

R2 gate:

- reverse-ordered multi-intent manifests plan when later entities consume
  earlier generated types, while cycles/missing prerequisites name stable
  participants;
- each recipe/capability has a metadata-input fixture; an undeclared read fails
  immediately and production planner modules contain no direct filesystem,
  CWD, environment, clock or process access;
- missing optional files and every enumerated directory are read-set
  preconditions; absent ledger is distinct from an empty ledger file;
- schema-1/legacy loading captures every closed machine source, absence and
  intents/models listing before pure translation; once schema 2 exists, all
  old static files must remain absent and those directories absent or empty,
  so state reintroduced by an older binary refuses instead of being ignored;
- a pending ledger with markers in POM, `jails.toml`, app manifest and a scanned
  Java source reaches `LoadedProject::Pending` without invoking their ordinary
  parsers; the same request-syntax fingerprint reaches guarded continue/abort,
  while a different request/source selection refuses deterministically;
- mutating a file or adding/removing a directory entry after snapshot leaves
  the in-memory plan unchanged and causes R4's later stale check to refuse;
- projected pom/config/compose/properties/Java facts are invalidated and
  reparsed after a change, and no stale cache survives deletion;
- two `app plan` runs emit byte-identical semantic plans with no write under
  reordered manifests, different CWD, directory creation order and hash-map
  seeds;
- shared contribution tests prove removing one owner retains the dependency,
  property, service, registration and marked block; and
- the shadow planner matches current golden outputs for every persistent kind,
  capability class and one-shot without switching production mutation.

### R3 — Exhaustive preparation — SHIPPED

Gate: `jails_prepare::pipeline::prepare` turns a validated desired change set
into exact operations with no live-tree write, and the sandboxed runner is the
one place a tool is executed.

Separate semantic desire from an exact executable transition. Current
`model::Change` still contains deps/plugins/properties/compose/files and a
Spring import; `add_in`, `generate::write`, app reconcile and `--pretend`
interpret those buckets independently. R3 finishes every renderer, splice,
merge, formatter and report decision so R4 contains no domain logic.

R3 remains plan/shadow-only. It may use scratch outside the project and produce
a `PreparedChange`; it does not create `.jails/`, migrate state or commit an
operation until R4 exists.

#### R3.1 Exact prepared model

Create `src/planning/{prepare,report}.rs` and use the abstract contract's
names:

```rust
#[derive(Clone, Eq, PartialEq)]
enum FileImage {
    Absent,
    Present { object: ObjectRef, mode: FileMode },
}

#[derive(Clone, Eq, PartialEq)]
struct GuardedImage {
    object: ObjectRef,
    mode: FileMode,
}

enum OperationTarget {
    Project(ProjectPath),
    LegacyMachine(LegacySourcePath),
}

enum FileOp {
    Create {
        path: OperationTarget,
        after: ObjectRef,
        mode: FileMode,
        contributors: BTreeSet<ResourceOwner>,
    },
    Replace {
        path: OperationTarget,
        before: GuardedImage,
        after: ObjectRef,
        mode: FileMode,
        contributors: BTreeSet<ResourceOwner>,
    },
    Delete {
        path: OperationTarget,
        before: GuardedImage,
        contributors: BTreeSet<ResourceOwner>,
    },
}

enum DirectoryOp {
    Create { path: ProjectPath },
}

enum PreparedKind {
    Apply,
    Conflict { paths: Vec<ProjectPath> },
    Finalise { origin: OperationId },
    Abort { origin: OperationId },
}

struct OperationIdentityV1 {
    snapshot: SnapshotFingerprintV1,
    operation_context: OperationContextFingerprint,
    invocation: Option<InvocationFingerprint>,
    proposed_generation: u64,
    semantics: OperationSemanticsV1,
}

enum OperationSemanticsV1 {
    Apply { subject: PlannedSubject, ledger_intent: LedgerIntent,
            migration: Option<LegacyMigrationIdentity> },
    Finalise { origin: OperationId, origin_transaction: TransactionId,
               pending: PendingIdentity,
               resolutions: Vec<ResolutionIdentity> },
    Abort { origin: OperationId, origin_transaction: TransactionId,
            restores: Vec<RestoreIdentity> },
}

struct LegacyMigrationIdentity {
    snapshot: LegacySnapshotIdentity,
    translated_before: LedgerV2Draft,
}

struct LegacySnapshotIdentity {
    sources: Vec<LegacySourceImage>,
    directories: Vec<LegacyDirectoryIdentity>,
}

enum LegacySourceImage {
    Absent { path: LegacySourcePath },
    Present { path: LegacySourcePath, object: ObjectRef, mode: FileMode },
}

struct LegacyDirectoryIdentity {
    kind: LegacyDirectoryKind,
    state: LegacyDirectoryState,
}

struct PendingIdentity(ObjectId);

struct ResolutionIdentity { path: ProjectPath, resolved: FileImage }

struct RestoreIdentity {
    path: ProjectPath,
    guarded_from: FileImage,
    restore_to: FileImage,
}

struct OperationContextFingerprint {
    schema: u32,                         // exactly 1
    tools: Vec<OperationToolFingerprint>, // sorted unique by ToolInvocationKey
}

struct OperationToolFingerprint {
    identity: ToolIdentityFingerprint,
    args: Vec<ToolArgTemplate>,
}

enum ToolArgTemplate {
    Literal(String),
    OperationLabel { prefix: String, hex_chars: u8 },
}

struct ObjectRef { id: ObjectId, len: u64 }

struct OperationId(ObjectId);           // before ID-bearing argv/bytes
struct TransactionId(ObjectId);         // exact immutable prepared identity

struct FileMode(u32);                    // Unix permission bits, masked `0o777`

enum PostCommitEffect {
    ComposeReconcile {
        compose_output: ProjectPath,
        before_document: Option<StoredFileImage>,
        after_document: Option<StoredFileImage>,
        prior_managed_services: BTreeMap<ServiceName, ObjectId>,
        desired_services: BTreeMap<ServiceName, ObjectId>,
        stop_services: BTreeSet<ServiceName>,
    },
}

enum DeferredEffectIntent {
    ComposeReconcile {
        before_document: Option<StoredFileImage>,
        compose_output: ProjectPath,
        prior_managed_services: BTreeMap<ServiceName, ObjectId>,
        desired_services: BTreeMap<ServiceName, ObjectId>,
    },
}
```

`LegacyMigrationIdentity` is `Some` only for the first schema-2 commit when
the captured machine state contains a supported schema-1 ledger or another
legacy source. Its `sources` contains every closed static path as present or
absent plus every present child named by the two directory identities; the two
directory rows occur exactly once. Sources sort by `LegacySourcePath` and
directories by `LegacyDirectoryKind`. Every present object, mode and directory
listing must equal the corresponding `LegacyFile`/`LegacyDirectory` read-set
row. Preparation reruns `translate_legacy` from those immutable objects and
requires byte-for-value equality with `translated_before` before applying the
current `LedgerIntent`.

Only this validated migration may contain
`OperationTarget::LegacyMachine`, and only as `FileOp::Delete` with an exact
preimage and an empty contributor set. Its targets are exactly every present
legacy source except `Schema1Ledger`; the guarded ledger create/replace consumes
that path separately as `ledger_before → ledger_after`. A legacy target is
never a renderer output, conflict path, semantic resource, create/replace
target or directory operation. `migration = None` requires every operation
target to be `Project`. Thus cleanup is atomic with the first V2 ledger commit
without weakening `ProjectPath`'s reserved-namespace rule.

Emit at most one aggregate `ComposeReconcile`. It exists iff the canonical
invocation has `no_start == false` and either the complete canonical managed
compose-service map or the committed compose output changes. The prior/desired
maps are the entire managed maps before/after the transition, sorted by
`ServiceName`; each value is
`SHA256("JAILS-COMPOSE-SERVICE-SPEC-1" || encode(ComposeServiceSpec))` for the
complete canonical desired spec. For an executable descriptor,
`before_document` is the exact regular-file live preimage object at the
committing transition when one existed; `after_document` is the exact
marker-free committed live postimage `M` after reconciliation whenever that
path remains present. It is deliberately not merely the renderer's desired
base `N`. Both stored images include mode. `compose_output` is the one canonical
project path; `after_document = Some(image)` is the exact committed live path
postimage, including retained unowned content, while `None` means exact
committed absence.

For a clean executable descriptor, parse the committed marker-free
`after_document` when present and define `stop_services =
prior_managed_services.keys - all_service_names(after_document)`. Thus a
removed managed name deliberately retained in the file as an unmanaged service
is not stopped. Require `before_document` when `stop_services` is nonempty and
`after_document` when `desired_services` is nonempty; document presence is
otherwise independent of whether any managed service remains. Parse a present
`before_document` with the same closed compose parser and require
`stop_services ⊆ all_service_names(before_document)` during clean preparation.

Conflict preparation cannot derive `stop_services`, because the resolved
postimage does not exist yet. It freezes and validates the exact
`before_document` plus the separate intent form below, and emits no executable
effect. Finalisation parses the marker-free resolution, derives the stop set,
and applies the same subset invariant before committing. If a user has already
deleted a formerly managed service block from live `L`, jails has no truthful
frozen compose document with which to address that service: clean preparation
refuses before commit, while pending finalisation refuses and remains pending.
The user must abort that conflict and rerun the original command with
`--no-start`; a changed flag cannot match or mutate the frozen invocation. The
executor never leaves this as a predictable Docker failure or substitutes a
regenerated/stored-base document. Durable descriptor validation repeats the
subset invariant. These fields are the effect's relevance and execution
guards, not merely reporting metadata.

Owner-only changes, unrelated-file repair and an already-satisfied second
apply emit no effect. Managed service removal or an executable compose
postimage change does emit one unless `no_start` suppresses runtime
reconciliation. A true semantic/file/ledger no-op can consequently satisfy
R3's empty-effect rule. A conflict freezes only the semantic intent and waits
for its resolved postimage as described below. A request variant with no
`no_start` field is ineligible and behaves as `no_start == true`; maintenance
and one-shot actions never change runtime services by accident.

Immediately before an execution attempt, the complete compose-service map in
the current ledger must equal `desired_services`, every referenced document
must revalidate by kind, length, mode and SHA-256, and the actual confined
`compose_output` must equal the presence/image encoded by `after_document`.
When a managed `OutputRecord` still exists, its `current` image must equal that
same postimage. A later committed
operation that preserves service specs but changes the compose output/renderer
therefore supersedes the old effect. A live mismatch while the ledger still
claims this operation's image is unrecorded drift and returns
`Err(EffectRunError::StaleInput)` without
running or rewriting the receipt; it is not mislabeled supersession.

Execution is a closed idempotent sequence under one effect attempt. When
`stop_services` is nonempty, use the immutable `before_document` for
`docker compose --project-directory <runtime-root> --file
<verified-before-object> stop -- <sorted stop services>`, followed by the same
prefix and `rm -f -- <sorted stop services>`. When `desired_services` is
nonempty, use the immutable `after_document`—not a fresh read of live
`compose.yaml`—for `up -d -- <all sorted desired managed service names>`.
Repeated stop/rm/up is the required idempotency behaviour. Never use
`down` or `--remove-orphans`: those commands can destroy unmanaged services,
networks or volumes in a shared compose project. Removal therefore has an
explicit inverse bounded to formerly managed names.

Runtime paths are opaque executor bindings and never identity fields. The
closed Compose process specification must disable implicit override discovery,
use a scrubbed explicit environment, and declare any future additional project
input before that input is allowed; otherwise preparation refuses that service
shape. `EffectId` is the retry/idempotency key.

`DeferredEffectIntent` is semantic frozen conflict state, never executable
receipt state. A clean apply materialises `ComposeReconcile` immediately:
`before_document` is the exact regular-file preimage object of the last live
compose document when one existed, while `after_document` is the exact
marker-free committed postimage `M` after reconciliation (not renderer base
`N`). This means a clean three-way merge executes the same user-preserving
compose bytes that the transaction actually committed. Removing the final
service requires a usable `before_document`; if the live document is missing
or unreadable, preparation refuses runtime reconciliation unless the caller
explicitly selected `--no-start`.

A conflicted apply instead stores one `DeferredEffectIntent` in
`PendingConflict.effect_intents` and keeps
`PreparedChange.post_commit` empty. It records the exact pre-conflict document,
the canonical compose output path and complete desired service map, but cannot
pretend to know the user's future resolved postimage. Finalisation validates
the resolved marker-free live path, interns those exact bytes as the
`after_document`, and deterministically materialises the new finalisation
receipt's executable `ComposeReconcile`. Abort discards the intent by clearing
pending state. Thus conflict and finalisation share semantic intent without
copying an unknowable or false executable descriptor.

```rust

struct RequestSyntaxFingerprint(ObjectId);

struct CanonicalRequestSyntaxV1 {
    command_path: Vec<String>,
    positionals: Vec<String>,
    options: BTreeMap<String, Vec<String>>,
    flags: BTreeSet<String>,
}

struct InvocationFingerprint {
    request_syntax: RequestSyntaxFingerprint,
    request: CanonicalMutationRequest,
    manifest_source: Option<ManifestSourceId>,
    desired_input_sha256: ObjectId,
}

enum CanonicalMutationRequest {
    Add { capabilities: Vec<CanonicalCapability>, no_start: bool },
    Remove { capabilities: Vec<CanonicalCapability>, force: bool,
             no_start: bool },
    Sync { no_start: bool },
    Generate(CanonicalGenerateRequest),
    Destroy { subject: ChangeSubject, force: bool },
    AppInit { target: ProjectPath },
    AppApply { no_start: bool },
    Rename { from: JavaType, to: JavaType, force: bool },
    AdoptLayout,
    AdoptLegacy { legacy_key: LegacyKey, intent: IntentId,
                  replace: bool, force: bool },
    FastTest,
    Format { scopes: BTreeSet<ProjectPath> },
    RemoveToolFeature { feature: ToolFeature, force: bool },
}

struct CanonicalCapability { id: CapabilityId, spec: CapabilitySpec }

enum CanonicalGenerateRequest {
    Entity { id: EntityId, spec: EntitySpec },
    OneShot { id: OneShotId, spec: OneShotSpec },
}

enum ChangeSubject { Entity(EntityId), OneShot(OneShotId) }

enum ManifestSourceId {
    Project(ProjectPath),
    External { path_id: ExternalPathId },
}

// Shared-codec tags, in declaration order:
// CanonicalMutationRequest = 0..12; CanonicalGenerateRequest = 0..1;
// ChangeSubject = 0..1; ManifestSourceId = 0..1.

enum EffectState {
    Deferred,
    Pending { next_attempt: u32 },
    Running { attempt: u32 },
    Succeeded,
    Failed { attempt: u32, code: EffectFailureCode, summary: String },
    Superseded { by: Option<OperationId> },
}

enum EffectFailureCode {
    Spawn,
    Timeout,
    ExitNonzero,
    InterruptedTwice,
    Protocol,
}

struct EffectReceipt {
    id: EffectId,
    effect: PostCommitEffect,
    state: EffectState,
}

struct EffectId(ObjectId);

type JournalEffect = EffectReceipt;     // one wire representation, not a peer
```

`Failed.summary` is diagnostic, not raw subprocess output. Construct it
deterministically by decoding with UTF-8 replacement, normalising CRLF/CR to
LF, replacing control characters other than LF/TAB, redacting every known
canonical project/external/runtime absolute path, and truncating at a UTF-8
boundary to 4,096 bytes with a fixed `…[truncated]` suffix. Never include
environment values or command secrets. `EffectFailureCode` tags
spawn/timeout/exit-nonzero/interrupted-twice/protocol are `0/1/2/3/4`.

```rust

enum ApplyOutcome {
    Applied,
    Conflicted,
    Finalised,
    Aborted,
}

struct FileReceipt {
    path: OperationTarget,
    before: FileImage,
    after: FileImage,
    contributors: BTreeSet<ResourceOwner>,
}

struct DirectoryReceipt {
    path: ProjectPath,
}

struct PreparedChange {
    format: u32,                         // exactly 1
    operation_identity: OperationIdentityV1,
    operation_id: OperationId,
    transaction_id: TransactionId,
    preparation: PreparationContextFingerprint,
    input_preconditions: Vec<InputPrecondition>,
    operations: Vec<FileOp>,
    directories: Vec<DirectoryOp>,
    ledger_before: FileImage,            // absent ledger is representable
    ledger_after: FileImage,
    objects: BTreeMap<ObjectId, Arc<[u8]>>,
    post_commit: Vec<PostCommitEffect>,
    kind: PreparedKind,
}

struct PreparedIdentityV1 {
    format: u32,
    operation_identity: OperationIdentityV1,
    operation_id: OperationId,
    preparation: PreparationContextFingerprint,
    input_preconditions: Vec<InputPrecondition>,
    operations: Vec<FileOp>,
    directories: Vec<DirectoryOp>,
    ledger_before: FileImage,
    ledger_after: FileImage,
    object_manifest: Vec<ObjectRef>,
    post_commit: Vec<PostCommitEffect>,
    kind: PreparedKind,
}

struct PreparedBundle {
    change: PreparedChange,
    commit_context: CommitContext,       // runtime-only; never persisted/reported
}

struct CommitContext {
    project_root: RootIdentity,
    external_inputs: BTreeMap<ExternalInputId, ExternalBinding>,
    machine_root: MachineRootBinding,
}

struct RootIdentity { device: u64, inode: u64 }

struct MachineRootBinding {
    expected: MachineRootPresence,
    device_inode: Option<(u64, u64)>,    // Some iff expected Present
}

enum ExternalBinding {
    ExactFile { canonical_path: PathBuf },
    ConfinedCandidate {
        canonical_root: PathBuf,
        root_device: u64,
        root_inode: u64,
        relative: PathBuf,              // validated components; runtime-only
    },
}

struct PreparationContextFingerprint {
    schema: u32,                         // exactly 1
    tools: Vec<ToolFingerprint>,         // sorted unique by ToolInvocationKey
}

struct AppliedReceipt {
    operation_id: OperationId,
    transaction_id: TransactionId,
    files: Vec<FileReceipt>,
    directories: Vec<DirectoryReceipt>,
    ledger_before: FileImage,
    ledger_after: FileImage,
    outcome: ApplyOutcome,
    post_commit: Vec<EffectReceipt>,
}

enum CommitResult {
    NoOp,
    Committed(CommittedResult),
    CommittedRecoveryRequired(CommittedRecoveryRequired),
    RecoveredPriorTransaction(RecoveryOutcome),
}

struct CommittedResult {
    receipt: AppliedReceipt,             // last checksum-validated projection
    effect: CommitEffectOutcome,
}

enum CommitEffectOutcome {
    NotApplicable,
    Succeeded { effect: EffectId },
    Failed { effect: EffectId },
    Superseded { effect: EffectId },
    DeferredError { effect: EffectId, error: CommittedEffectError },
}

enum CommittedEffectError {
    StaleInput,
    CorruptMachineState,
    ReceiptIo,
}

struct CommittedRecoveryRequired {
    operation: OperationId,
    transaction: TransactionId,
    outcome: ApplyOutcome,
    receipt: Option<AppliedReceipt>,
    stage: PostCommitStage,
    error: PostCommitRecoveryError,
}

enum PostCommitStage {
    JournalCompletion,
    ReceiptPublication,
    ReceiptReconciliation,
}

enum PostCommitRecoveryError {
    Io,
    RecoveryBlocked,
    CorruptMachineState,
}

enum EffectRunResult {
    Succeeded(AppliedReceipt),
    Failed(AppliedReceipt),
    Superseded(AppliedReceipt),
    RecoveredPriorTransaction(RecoveryOutcome),
}

struct RecoveryOutcome {
    changes: Vec<RecoveryChange>,
    pending_effects: Vec<RecoverableEffect>,
}

enum RecoveryChange {
    Transaction {
        operation: OperationId,
        transaction: TransactionId,
        generation: u64,
        action: RecoveryTransactionAction,
    },
    EffectStateChanged {
        operation: OperationId,
        transaction: TransactionId,
        generation: u64,
        effect: EffectId,
        before: EffectState,
        after: EffectState,
    },
}

enum RecoveryTransactionAction {
    AbandonedPrepared,
    RolledForwardAndPublished,
    PublishedCommittedReceipt,
}

struct RecoverableEffect {
    operation: OperationId,
    transaction: TransactionId,
    generation: u64,
    effect: EffectId,
    state: EffectState,
}

enum CommitError {
    StaleInput,
    MutationBusy,
    EffectBusy,
    RecoveryBlocked,
    CorruptMachineState,
    InvalidPrepared,
    PreActivationIo,
}

enum EffectRunError {
    StaleInput,
    MutationBusy,
    EffectBusy,
    RecoveryBlocked,
    CorruptMachineState,
    InvalidPlan,
    ReceiptIo,
}

enum RecoveryError {
    MutationBusy,
    RecoveryBlocked,
    CorruptMachineState,
    Io,
}
```

`RecoveryOutcome` is an in-memory/report value, not a durable protocol record.
`changes` is sorted by `(generation, transaction, change-kind, effect)` and has
no duplicate locator. It records every authoritative change made by that
recovery call: removal of a fully validated `Prepared`/all-before transaction,
forward completion/publication of one active or already committed transaction,
and each allowed structural effect-state transition. Those transitions are
closed: any nonterminal executable effect whose logical ledger guard is obsolete
may become `Superseded`, and an interrupted `Running { attempt >= 2 }` becomes
`Failed { code: InterruptedTwice, .. }` without a subprocess. Cleanup of a directory in
which no valid journal ever existed is executor-private debris cleanup and is
not a `RecoveryChange`. `pending_effects` is the sorted, duplicate-free snapshot
of executable `Deferred`, `Pending`, every `Running`, and `Failed` effect that
recovery reported but did not execute; terminal states are absent.
An empty `changes` vector means recovery was observationally clean even when
`pending_effects` is nonempty.

Both public `recover` and private `recover_locked` return this exact value. At
the start of `commit` or `resume_effect`, a nonempty `changes` vector means the
caller planned against stale project or receipt state: release every lock and
return `RecoveredPriorTransaction(outcome)` without beginning the requested
transition. The outer command reloads/replans at most once and may report all
locators from the value; it never discards all but one “primary” transaction.
An empty `changes` vector permits the current operation to continue. A blocked
or corrupt classification is `Err(RecoveryError)` and never a fabricated clean
outcome.

`CommittedResult` closes the otherwise ambiguous post-commit error channel.
Once the ledger commit point is crossed, `commit` never returns `CommitError`.
It returns the last checksum-validated `AppliedReceipt` plus exactly one effect
outcome. `NotApplicable` covers no executable effect (including conflict and
abort); the three terminal variants agree with the named receipt state.
`DeferredError` means no trustworthy terminal state was recorded: a pre-call
live guard changed, an object/receipt became corrupt, or receipt-state I/O
failed. Its receipt projection is explicitly the last checksum-validated state,
which may still be `Deferred`, `Pending` or `Running`; recovery/retry owns what
happens next. V1 permits at most one executable aggregate effect, so this is one
value rather than a vector. A future multi-effect protocol must version this
result contract and define stop/continue ordering rather than silently widening
it.

`CommittedRecoveryRequired` covers structural work that failed after the ledger
commit point: completing the journal, publishing/fsyncing the receipt pair, or
reconciling older receipt states before external effects may start. It is also
a success-side result, never `CommitError`. The operation/transaction/outcome
are known from the validated prepared value and committed ledger.
`receipt = Some` is allowed only when that exact checksum-valid Complete-
journal/receipt pair was reread successfully; otherwise it is `None`, and no
`AppliedReceipt` is fabricated from the prepared value alone. Keep the journal
and every object for later structural recovery. `stage` identifies the exact
unfinished boundary. `Io`, a newly discovered blocked classification and
corrupt machine state remain distinct so the adapter can give a stable repair
message. This result has `project_commit: New`, exit 1 and prevents the outer
driver from re-planning or starting an effect in the same invocation. Once
recovery completes the structure, subsequent reporting derives the real
receipt.

The canonical request tag table is explicit; declaration order is explanatory,
not the implementation mechanism:

| Tag | `CanonicalMutationRequest` |
|---:|---|
| 0 | `Add` |
| 1 | capability `Remove` |
| 2 | `Sync` |
| 3 | `Generate` |
| 4 | `Destroy` |
| 5 | `AppInit` |
| 6 | `AppApply` |
| 7 | `Rename` |
| 8 | `AdoptLayout` |
| 9 | `AdoptLegacy` |
| 10 | `FastTest` |
| 11 | `Format` |
| 12 | `RemoveToolFeature` |

Constructors and decoders enforce this closed admissibility matrix before
resolution; a matching outer/inner Rust shape is not sufficient:

| Request/value pair | Only accepted shape |
|---|---|
| `Add` / capability `Remove` | Nonempty, sorted, duplicate-free `CanonicalCapability` rows; every ID/spec pair is capability/capability. |
| `Generate::Entity` | `EntityId::Intent(id)` paired with exactly `EntitySpec::Intent(spec)`. Capability/tool-feature entities reject. |
| `Generate::OneShot` | ID/spec discriminants match (`Field↔Field`, `Migration↔Migration`, `Cases↔Cases`) and their repeated target/path/source identity fields agree. |
| `Destroy::Entity` | Persistent `EntityId::Intent` only. Capability and tool-feature removal use their dedicated requests. |
| `Destroy::OneShot` | `OneShotId::Cases` only; field/migration destroy rejects. |
| `FastTest` | Resolver injects exactly `ToolFeature::FastTest` and the one pinned `ToolFeatureSpec` constant into `DirectEntity`; no caller-supplied version. |
| `RemoveToolFeature` | Exactly `ToolFeature::FastTest`; removing an absent owner is no-op and any future feature needs a protocol/CLI addition. |
| `AdoptLegacy` | Valid `LegacyKey`, manifest source, canonical intent and the R1.4 evidence rules; `replace` implies `force`. |

The same discriminant equality is required wherever ID/spec values pair:
`DesiredEntity`, `AppliedEntity`, renderer context and pending candidates. One
negative codec and resolution test covers every forbidden cross-product, plus
empty/duplicate/unsorted collections and mismatched repeated identity fields.

`ObjectRef` is an `ObjectId` plus its length; `FileImage` never repeats those
facts. In R3/R4 every referenced byte
sequence is present in `PreparedChange.objects` and then the transaction-local
blob directory; R5 may promote selected blobs to durable object storage. This
gives crash recovery exact bytes before the durable provenance store exists.
There is no lazy callback/body in a prepared value.

`PreparedIdentityV1.object_manifest` is computed by one shared pure function,
`prepared_object_closure(prepared_without_manifest)`. Its initial roots are
every `ObjectRef` that occurs in: (a) each operation before/after image and both
ledger images; (b) `operation_identity`, including snapshot template rows,
machine-object inputs, frozen conflict resolution/restore semantics and any
object-bearing planned subject; (c) `input_preconditions`; and (d) immutable
post-commit descriptors. The function then decodes only object kinds whose
closed codec is declared to contain references—ledger, pending state,
renderer context and other protocol aggregate objects—and follows every such
reference to a fixed point. Raw file/template/tool-output bodies are leaves;
a bare file hash or ordinary present/absent precondition does not invent an
object reference.

The manifest is that closure sorted by `(ObjectId, len)`, with one length per
ID; duplicate IDs with different lengths reject. `PreparedChange.objects`
must contain exactly those IDs with the declared lengths—no missing object and
no unreferenced extra. Preparation rejects a hash/length disagreement before
returning. Journal and receipt validation call the same closure function and
repeat the check against durable object storage. This manifest, not directory
enumeration, is the authority for transaction object membership and
garbage-collection reachability.

`operation_identity` is stored—not reconstructed from ledger output—so durable
validation can recompute `operation_id` exactly. Its `snapshot.read_set.inputs`
must equal `input_preconditions`; its full `SnapshotFingerprintV1` yields the
derived `snapshot_fingerprint` used by reports. Its invocation is the sole
prepared invocation field.
`operation_context.tools` and `preparation.tools` must have identical sorted
tool identities/counts; each final tool fingerprint must preserve the matching
identity, and its argv hash must equal expansion of the stored template with
the stored operation ID. These cross-field checks reject duplicate identity
authorities rather than trusting whichever copy a caller happens to read.

Every user-originated mutation carries
`operation_identity.invocation = Some(InvocationFingerprint)`; internal
recovery-only maintenance may carry `None` and can never own a post-commit
effect. The fingerprint stores parsed typed values after defaults/aliases and
package resolution. Capability sets sort; field/index/variant order does not.
It excludes debug/output flags and `--abort-conflict`. Raw argv, secrets and
display text never enter it. R1 implements the complete fields and tags; R2
constructs the request and desired-input portions, and R5 only activates their
pending-resume validation.

`RequestSyntaxFingerprint` lets a pending project recognise the same command
without parsing marker-bearing project files or re-running project-derived
defaults. The CLI adapter first constructs `CanonicalRequestSyntaxV1` using
canonical command/option names after alias resolution. `command_path` contains
the command and subcommand components without leading dashes. `positionals`
uses validated UTF-8 lexical values, sorting only command positions whose
semantics are sets and preserving every ordered position. `options` contains
only explicitly supplied semantic options, keyed without leading dashes;
repeated values preserve order unless that option is a semantic set. `flags`
contains explicitly supplied semantic flags. Presentation/debug flags and
`--abort-conflict` are omitted; omission of a project-derived default remains
distinguishable from an explicit value. Then compute exactly
`SHA256("JAILS-REQUEST-SYNTAX-1" || encode(CanonicalRequestSyntaxV1))`. Every
mutation route has a golden syntax fixture, and aliases that the CLI promises
as equivalent must produce the same projection. A future secret-bearing option
cannot enter this projection without an explicit redacted typed representation
and protocol-version decision.

`ObjectId`, `OperationId`, `TransactionId` and `snapshot_fingerprint` use
lowercase SHA-256.
R1 adds the pinned RustCrypto `sha2` crate and one `protocol::hash` utility
before implementing object IDs, the ledger payload or golden vectors; R3
reuses it and does not introduce a second digest API. SHA-256 is stable, portable content
identity; RustCrypto avoids an OpenSSL/git subprocess and `DefaultHasher` is
explicitly unstable. Format hex in a local codec rather than add a hex crate.
Test published empty-string and `abc` vectors. `Cargo.lock` pins the transitive
implementation.

`OperationId` breaks the otherwise self-referential identity cycle. Render
semantic candidate bytes that cannot contain an operation ID, determine every
tool invocation that will be needed, and form its typed literal/operation-label
argv template. Then compute
`SHA256("JAILS-OPERATION-1" || encode(OperationIdentityV1))`. The operation
context contains tool identities and templates, never hashes of already
expanded ID-bearing argv. After this point the selected tool set and templates
are frozen; substitute the operation label, execute remaining tools and put
the hashes of actual argv only in `PreparationContextFingerprint`. A validator
rejects a full-argv hash or output byte fed back into operation identity. A
golden Git-merge fixture proves this ordering has no fixed-point dependency.

The invocation contains the canonical typed request. For ordinary
apply/conflict the semantic payload is the closed `PlannedSubject` plus
`LedgerIntent`; a conflict is not a different pre-merge identity. For
finalisation it is the R5 frozen pending identity plus path-sorted resolution
images; for abort it is the exact origin operation/transaction plus path-sorted
guarded restore transitions. The latter two always produce new operation IDs
and never reuse the conflict ID. Ledger attribution and conflict marker labels
embed only this operation ID. No exact after-byte embeds `TransactionId`.
`OperationSemanticsV1` tags apply/finalise/abort are `0/1/2`;
resolution/restore vectors reject duplicate paths. `ToolArgTemplate` tags
literal/operation-label are `0/1`; `OperationLabel` format-1 validation is the
exact prefix and 12-character rule in R3.3.
For finalisation and abort, the canonical semantic payload is respectively the
frozen pending candidate plus resolution hashes, or the origin receipt plus
guarded inverse targets; both include `PreparedKind`, origin operation and next
generation. They never reuse the origin operation ID.

After every renderer, merge, formatter and ledger byte is final, construct the
immutable `PreparedIdentityV1` projection: format, complete operation identity,
matching operation ID, preparation fingerprint, ordered
preconditions/file/directory operations, both ledger
images, `PreparedKind`, canonical post-commit effect descriptors and sorted
object references. `transaction_id` is SHA-256 of
`JAILS-PREPARED-1 || encode(PreparedIdentityV1)`. It excludes only itself,
runtime `CommitContext` and derived presentation. Equivalent exact prepared
work at one ledger generation has one transaction ID; changing any byte, mode,
precondition, owner, kind or effect changes it. This projection is the sole
identity definition embedded in and reused by both journal and receipt
validation.

Absolute external paths live only in `PreparedBundle.commit_context`. The
durable `InputPrecondition::ExternalFile` contains opaque `ExternalInputId`,
hash and length; commit uses the runtime binding for its final pre-activation
recheck. Recovery never needs the external source because all exact after
objects were durable before activation. Reports and journals therefore persist
no home/template/brief path.

For format 1, every non-no-op project mutation has
`ledger_after = FileImage::Present`: the first mutation creates an otherwise
empty schema-2 ledger and advances generation. `ledger_before` may be absent.
Abort preserves the prior successful logical tables and clears pending state
in a new-generation present ledger rather than deleting machine state.
`ledger_after = Absent` is rejected by the
format-1 validator, so every active transaction has one unambiguous ledger
commit point.

Operations sort by `(OperationTarget tag, inner canonical path, operation
tag)`: `Project` before `LegacyMachine`, each by its own closed path order, then
`Delete < Replace < Create`. Duplicate targets are forbidden, so the operation
tag is a defensive canonical rule. Directory creates sort shallowest-first and
are exactly the absent parents required by a project `Create`; reports wrap
their path as `OperationTarget::Project`. A legacy target never has a directory
operation. There is no directory-delete operation in this roadmap. Deleting a
file never auto-removes its parent, even when empty. Created directories are
monotonic structural shells: recovery ensures they exist, receipts report
them, but abort/removal does not delete them. This deliberately avoids racing
with user-created contents and means a guarded abort may leave an empty parent
directory; reports and tests state that exception instead of promising an
unsafe exact directory rollback.

All closed binary formats share `src/codec.rs`; do not let each module invent
framing. Version 1 rules are normative:

| Value | Encoding |
|---|---|
| digest/ID | 32 raw bytes; lowercase 64-hex is presentation only |
| `u32` / `u64` | unsigned big-endian, fixed width |
| boolean | one byte: `0` false, `1` true; other values reject |
| `Option<T>` | one-byte `0`/`1`, followed by `T` only for `1` |
| UTF-8 string/path | `u32` byte length then bytes; validate before allocation |
| raw object body | `u64` length then bytes when inline; object references never inline implicitly |
| list/set/map | `u32` element count then canonical elements; decoders reject duplicate or out-of-order set/map keys |

Limits before allocation are: 4,096 bytes per project path, 1 MiB per ordinary
string/diagnostic, 1,000,000 collection entries, and the configured 256 MiB
default per object. A command may lower the object limit; raising it requires
an explicit CLI/config value and still checks `usize` conversion. Every inline closed-codec record (snapshot identity, operation identity,
prepared identity, journal, receipt, invocation or render context) is also
bounded to `MAX_PROTOCOL_RECORD = 64 * 1024 * 1024` bytes. File readers use the
same limit-plus-one pattern as the ledger envelope, and in-memory encoders stop
before exceeding it; per-field/count limits do not replace this aggregate cap.
Recursive values (`TemplateValue`, Java type arguments and future explicitly
recursive codec nodes) have `MAX_CODEC_DEPTH = 64`; encoders and decoders carry
an explicit checked depth counter rather than recurse without a limit.
Enum tags are fixed: `InputPrecondition` absent/file/directory/external-absent/
external-file/legacy-absent/legacy-file/legacy-directory/machine-object/
machine-receipt/machine-receipt-directory/machine-root =
`0/1/2/3/4/5/6/7/8/9/10/11`;
`LegacyDirectoryKind` intents/models = `0/1`; `LegacyDirectoryState`
absent/present = `0/1`; `MachineObjectSource` global/receipt = `0/1`;
`MachineRootPresence` absent/present = `0/1`;
`MachineReceiptDirectoryState` absent/present = `0/1`;
`OperationTarget` project/legacy-machine = `0/1`; `LegacySourceImage`
absent/present = `0/1`;
`FileImage` absent/present = `0/1`; `FileOp` create/replace/delete = `0/1/2`;
`DirectoryOp::Create = 0`; `PreparedKind` apply/conflict/finalise/abort =
`0/1/2/3`; `PostCommitEffect::ComposeReconcile = 0`;
`DeferredEffectIntent::ComposeReconcile = 0`; and `EffectState`
deferred/pending/running/succeeded/failed/superseded = `0/1/2/3/4/5`.
`Superseded` is followed by an optional superseding operation ID;
`Pending`, `Running` and `Failed` carry their fields as declared and require a
nonzero attempt. Unknown tags,
invalid UTF-8, excessive lengths, integer overflow and trailing bytes reject.
`OperationSemanticsV1` apply/finalise/abort = `0/1/2`;
`PlannedSubject` reconcile/apply-one-shot/destroy-cases/app-init/rename/adopt-
layout/adopt-legacy/format = `0/1/2/3/4/5/6/7`; and `ToolArgTemplate`
literal/operation-label = `0/1`. These identity-reachable tags are format-1
protocol values even though they are not ledger variants.

Golden identity vectors cover every legacy input-precondition variant, both
machine-object sources, both operation targets, an absent and fully populated
legacy snapshot identity, and an `Apply` with `migration = Some`; flipping a
source byte, mode, directory entry, translated row or delete target changes the
operation/transaction identity or fails validation as specified.

Every other wire enum declares explicit numeric tags next to its codec and a
test preventing reuse; Rust discriminants are never serialised. Snapshot,
directory-listing, desired-input and relevant-input hashes use the same codec
with distinct ASCII domain prefixes (`JAILS-SNAPSHOT-1`,
`JAILS-DIRECTORY-1`, `JAILS-DESIRED-INPUT-1`,
`JAILS-RELEVANT-INPUT-1`). Commit byte-level golden vectors for an empty and a
fully populated prepared identity, journal, receipt, render context and
invocation fingerprint. A second implementation must reproduce their exact hex.

#### R3.2 Preparation algorithm

`prepare(LoadedProject, CommitPlan, PreparationContext) -> PreparedBundle`
performs this closed algorithm:

1. Match the `CommitPlan`. For `Apply`, reapply every `DesiredChange` to a
   fresh projection, require its result to equal `LedgerIntent`, and compose
   all `ResourceKey`s. Equal values deduplicate; incompatible values name
   sorted owners and refuse. For `Finalise`/`Abort`, do not run a recipe or
   interpret current declarations: use only the frozen plan and snapshot.
2. Validate the expected generation. For `Apply`, require
   `LedgerIntent.generation_before == observed.generation` and no pending
   conflict. For `Finalise`/`Abort`, validate the exact origin receipt and
   pending record under R5.4, including its `MachineReceipt` precondition.
3. Materialise every ID-independent deferred template once from the snapshot's
   frozen bytes and canonical bindings. Lower POM, plugin, property, compose,
   config, registration and codemod edits through their one format owner.
   Preserve materialised projected bytes; no semantic edit or
   `DesiredBody::Render` survives this step.
4. Run any selected literal-only formatter whose output is needed to determine
   final candidate bytes. Fold its complete scratch diff into the projection.
   Determine the R5 reconciliation case for every output and therefore whether
   Git is required, but do not invoke Git yet. Finalisation and abort select no
   renderer, formatter or merge tool.
5. Freeze the exact sorted `OperationContextFingerprint`: every tool identity
   and typed argv template selected in steps 3–4, including Git only for an
   actual divergent text merge. Compute `OperationId` from the snapshot,
   invocation, proposed generation, closed semantics and this context. From
   this point changing the subject, selected tool, identity or argv template is
   an internal error.
6. Expand the sole permitted operation-label placeholder and run remaining
   ID-bearing Git merge invocations. Materialise exact clean or marker bytes,
   modes, renderer contexts and full actual-argv `ToolFingerprint`s. Require
   the executed tool set to equal the frozen operation-context tool set.
   Before R5 is active, only an unchanged, exact owned image may be replaced or
   deleted; drift/missing base refuses. R3 may test synthetic conflict values,
   but production conflict acceptance waits for R5.
7. Derive the canonical post-commit descriptor vector from the final projected
   resource transition *before* rendering the ledger or deciding no-op. Apply
   may emit the one guarded aggregate `ComposeReconcile` by the rule above. A
   clean apply materialises it from the exact committed postimage; a conflict
   stores only `DeferredEffectIntent` in pending state and has an empty
   executable vector; finalisation materialises that frozen intent from the
   exact resolution image; abort emits none. No later step may add, remove or
   rewrite an executable effect.
8. Diff the final projection against snapshot images. Equal bytes and mode emit
   no op; create then delete collapses; every other path becomes exactly one
   `FileOp::{Create,Replace,Delete}` with all contributors. Refuse a symlink,
   directory/device target, case-fold collision, reserved path, non-UTF-8 path
   or whole-file/semantic-ownership overlap. Contributors are empty exactly for
   a maintenance-attributed unowned file; every managed path carries the exact
   nonempty owner set derived from its global resource/output rows.
9. Derive absent parent directories for creates only. Refuse a required parent
   that is a file/symlink. Never plan parent cleanup for deletes. Finalisation
   has no file or directory operation; abort has only the exact guarded restore
   operations from `AbortPlan` and creates no directory. Stop parent derivation
   at `.jails`: it is executor-owned machine structure, never a `DirectoryOp`
   or ordinary `Absent` precondition. A create of `.jails/app.toml` guards the
   leaf while using the executor's post-bootstrap machine root as its parent.
10. Render the exact schema-2 ledger image from the prior observation, semantic
   plan, exact output/provenance records, operation ID and prepared kind. A
   conflict preserves all five successful top-level tables exactly and stores
   the complete candidate separately. Finalisation promotes that entire
   candidate and fills only resolution-derived current images; abort preserves
   the successful tables and clears pending state. Ledger is excluded from
   `operations`; R4 commits its `FileImage` last.
11. A complete `Apply` with no file/directory operation, logical ledger change,
    or post-commit effect preserves exact `ledger_before` bytes, generation and
    `last_operation`; this true no-op is never journaled. Every other result
    increments generation exactly once and has a present ledger after-image.
    `Finalise` and `Abort` cannot be no-ops.
12. Validate the exhaustive prepared-kind matrix in R4.2, including exact
    correspondence among the already-frozen effect vector, pending-conflict
    candidate and prepared kind.
13. Include the entire R2 `ReadSet` in `input_preconditions`, including the
    pinned receipt guard where applicable. Preparation/tool fingerprints enter
    immutable prepared encoding but are not live tool preconditions: commit
    consumes their already-materialised bytes and never reruns a tool.
    Canonically sort/deduplicate effects and objects, construct
    `PreparedIdentityV1`, and derive `TransactionId`.
14. Return runtime absolute external bindings beside the durable change as
    `CommitContext`. Derive human and JSON reports only from final
    `PreparedChange`; reporting never replans or reads disk.

Refusals include invalid/unsupported renderer, parse/splice failure, object/hash
failure, output collision, missing declared input, unsafe legacy state and tool
failure. They return no `PreparedChange` and leave the managed root/ledger
unchanged. `Conflict`, `Finalise`, and `Abort` are explicit prepared kinds and
follow R4's same commit protocol. Post-commit effects are limited to idempotent external actions
such as `ComposeReconcile { frozen documents, desired services }`; registration, formatting and state
recording are file operations, not effects.

#### R3.3 Formatter and subprocess sandbox

`PreparationContext` contains a `ScratchExecutor`, runtime executable/cache
bindings and closed tool specifications:

```rust
struct ToolSpec {
    fingerprint: ToolFingerprint,
    args: Vec<String>,
}

struct ToolFingerprint {
    identity: ToolIdentityFingerprint,
    canonical_args_sha256: ObjectId,
}

struct ToolIdentityFingerprint {
    key: ToolInvocationKey,
    executable_sha256: ObjectId,
    version_stdout_sha256: ObjectId,
    runner_schema: u32,
    timeout_ms: u64,
    mutable_scopes: BTreeSet<ProjectPath>,
    offline_inputs: Vec<ToolInput>,
}

struct ToolInvocationKey {
    tool: ToolId,
    subject: Option<ProjectPath>,
}

struct ToolInput {
    logical_path: ToolCachePath,
    sha256: ObjectId,
    len: u64,
}
```

`ToolSpec.fingerprint.canonical_args_sha256` must equal
`SHA256("JAILS-TOOL-ARGS-1" || encode(args))`; a mismatch is an internal
error. `ToolIdentityFingerprint` is the complete execution policy used by the
runner—there is no duplicate timeout/scope/input authority on `ToolSpec`.
Resolve executable/version and every `ToolInput` once. Absolute executable and
cache source paths stay in runtime context; persisted provenance contains only
logical IDs and hashes. An unreadable executable is unsupported—do not weaken
identity to a path string.

For operation identity, every actually selected tool enters
`OperationContextFingerprint` as its identity plus typed argv template. A
literal-only invocation uses only `Literal`. Format 1 permits one nonliteral
form: a complete argv item
`OperationLabel { prefix: "jails-desired-", hex_chars: 12 }` for Git's desired
label. No placeholder may occur inside a free-form string, and no other prefix,
length or environment substitution decodes. Once `OperationId` exists,
preparation expands that item from lowercase operation hex and hashes the full
actual argv into `ToolFingerprint`. Those full fingerprints enter
`PreparationContextFingerprint`; only tools actually executed appear. The
operation template set and final executed tool set must have identical sorted
`ToolInvocationKey`s and identities. A project-wide formatter uses
`subject: None`; each per-output Git merge uses that output path as `Some`, so
several divergent outputs never collapse into one ambiguous fingerprint.

Use `tempfile::TempDir` under the configured system temp parent, with a child
named `project`. Copy only declared read-set files and synthesize projected
files, preserving relative paths/modes. Do not copy `.git`, `.jails` machine
state, `target`, secrets outside declared files or symlinks. Invoke the tool
with the child as CWD, a minimal explicit environment (`PATH`, locale forced to
`C`, tool-specific home/cache under scratch) and no stdin. Default timeout is
120 seconds. On Unix, create every tool in a new process group with
`std::os::unix::process::CommandExt::process_group(0)` and use the shared
`BoundedProcessRunner` backed by lockfile-pinned `nix` with default features
disabled and only `process,signal`. At timeout, send `SIGTERM` to the group,
drain pipes and poll the direct child/group for exactly two seconds, then send
`SIGKILL` to the group (treat `ESRCH` as already gone), wait the direct child,
and require the group to have no remaining observable member before returning.
Failure to kill/wait is a tool-cleanup refusal, never a detached success. Two
reader threads continuously drain stdout/stderr to prevent pipe deadlock,
retain at most 64 KiB per stream, and set a truncation bit while discarding
excess bytes. Raw environment values are never reported.

Every persisted argv uses deterministic relative scratch paths. Runtime
absolute executable/cache/temp bindings are applied by `ScratchExecutor` and
never copied into `ToolSpec.args`; an implementation that can invoke a tool
only with an absolute random path is unsupported until its runner schema
defines an opaque path-binding argument type. `runner_schema` fixes the exact
minimal environment keys, relative working-directory layout, stdin policy,
output caps and process-group behaviour; changing any of those rules requires
a new schema value.

The current Maven/Spotless formatter runs with `--offline` and a scratch-local
Maven repository. Before invocation copy only the wrapper/JAR/POM/checksum
files enumerated in `ToolSpec.fingerprint.identity.offline_inputs`, verify every hash, and point
`maven.repo.local` at that directory. Missing plugin or transitive artifact is
a preparation refusal that names the coordinate and tells the operator to warm
the cache explicitly; preparation never downloads it. A non-Maven tool must
provide an equivalent offline flag or an OS sandbox proven by integration
test. Host caches are inputs, never writable scratch aliases.

Before and after invocation recursively enumerate the entire scratch child.
Every change must fall under `ToolSpec.fingerprint.identity.mutable_scopes`; reject a
created symlink/device, path escape, deletion of an undeclared file or any
surprise output (including logs/cache under the project child). Re-read all
changed regular files, validate source encodings where required, and include
their bytes as file operations. Formatter-created parent directories become
explicit `DirectoryOp::Create`; no scratch directory itself enters the plan.
Always close/remove scratch explicitly and surface cleanup failure.

Formatting is deterministic evidence: run the tool a second time on its first
after-state in tests and require no diff. A formatter that wants network access
or produces different bytes is unsupported for transactional preparation; do
not quietly run it against the real project after commit.

#### R3.4 Reports and command outcomes

`Report` has one presentation-neutral schema:

```rust
struct Report {
    schema: &'static str,               // `jails.prepared-change.v1`
    operation: OperationId,
    transaction: TransactionId,
    kind: PreparedKind,
    operations: Vec<ReportedOp>,
    ledger: ReportedLedger,
    post_commit: Vec<ReportedEffect>,
    warnings: Vec<Warning>,
}

enum ReportedOpKind { Create, Replace, Delete, CreateDirectory }

struct ReportedOp {
    kind: ReportedOpKind,
    path: OperationTarget,
    before: Option<ObjectId>,
    after: Option<ObjectId>,
    bytes: Option<u64>,
    mode: Option<FileMode>,
    contributors: BTreeSet<ResourceOwner>,
}

enum ReportedLedgerKind { Unchanged, Create, Replace }

struct ReportedLedger {
    kind: ReportedLedgerKind,
    before: FileImage,
    after: FileImage,
}

struct ReportedEffect {
    effect: PostCommitEffect,
    state: EffectState,
}

enum WarningCode {
    LegacyUntrusted,
    UnmanagedRetained,
    PostCommitDeferred,
    EnvironmentConstrained,
}

struct Warning {
    code: WarningCode,
    paths: Vec<OperationTarget>,
    message: String,
}

enum CommandReport {
    Prepared(Report),
    EffectRetry(EffectRetryReport),
}

struct EffectRetryReport {
    operation: OperationId,
    transaction: TransactionId,
    effect_index: u32,
    effect_id: EffectId,
    effect: PostCommitEffect,
    reason: EffectResumeReason,
    before: EffectState,
    after: Option<EffectState>,
}

struct CommandEnvelope {
    schema: &'static str,               // `jails.command-result.v1`
    status: CommandStatus,
    exit_code: u8,
    project_commit: ProjectCommitDisposition,
    recovery: Vec<RecoveryOutcome>,
    report: Option<CommandReport>,
    receipt: Option<AppliedReceipt>,
    error: Option<ErrorReport>,
}

enum ProjectCommitDisposition { None, Existing, New }

enum CommandStatus {
    Preview, NoOp, Applied, Conflicted, Finalised, Aborted,
    EffectRetried, EffectSuperseded, Refused, Stale, RecoveryBlocked,
    EffectFailed,
}

struct ErrorReport {
    code: ErrorCode,
    message: String,
    paths: Vec<OperationTarget>,
}

enum ErrorCode {
    InvalidRequest,
    InputUnreadable,
    InputInvalid,
    UnsupportedProject,
    LegacyAmbiguous,
    PlanRefused,
    PrepareRefused,
    ToolFailed,
    StaleInput,
    MutationBusy,
    EffectBusy,
    RecoveryBlocked,
    CorruptMachineState,
    EffectFailed,
    InternalInvariant,
}
```

`Report` is a pure projection function over `PreparedChange`; it is not stored
inside that value and cannot drift from execution. Human output starts
`plan <transaction> apply|conflict|finalise|abort`, prints one ordered
`create|replace|delete|mkdir` relative path per operation, then `ledger
create|replace`, executable effects (or conflict-deferred semantic intents) and
actionable warnings. It never prints file
contents, secrets or absolute user-template paths. JSON uses the schema string,
lowercase enum tags, relative `/` paths, SHA-256 IDs, byte lengths, mode and
sorted contributors. Human and JSON golden tests must describe the same ops.
Report operations inherit `PreparedChange` order. Executable effects are
already sorted/deduplicated by enum tag and service name before hashing, so
report and executor consume the same vector. Warnings sort by `WarningCode`, first path and message, with
their path lists sorted. Ledger images preserve `Absent`, so a true no-ledger
no-op is not rendered as an invented hash.
For `ReportedOp`, `mode` is `Some(exact_after_mode)` for create/replace,
`Some(exact_before_mode)` for delete, and `None` only for create-directory.
Project paths render as relative `/` strings; legacy-machine targets render as
their fixed `.jails/...` spelling and are labelled `legacy-machine` in both
human and JSON output, never disguised as an ordinary project output.

`EffectRetryReport` has two exact pure constructors:
`describe_effect(plan)` sets `before = plan.expected_state` and `after = None`;
`describe_effect_result(plan, run_result)` uses the validated `AppliedReceipt`
carried by the success-side result and sets `after = Some(terminal_state)` only for a checksum-validated
`Succeeded`, `Failed`, or `Superseded` result.
`RecoveredPriorTransaction` and every `EffectRunError` use the preview
projection with `after = None` because the plan did not own a terminal
transition. It describes no file/ledger operation
and reuses the already committed operation/transaction IDs. Human preview says
`retry effect <effect-id> for <transaction>`; JSON uses
`CommandReport::EffectRetry`. This lets pretend/app-plan describe the exact
action without preparing a fake project transaction.

`ReceiptV1` is an internal durable record and is never serialised through
`CommandEnvelope`. The envelope's `AppliedReceipt` is the derived, stable API
projection. `CommitResult::NoOp` has no receipt.
`RecoveredPriorTransaction(outcome)` is executor control flow: the outer driver
appends that exact outcome to `CommandEnvelope.recovery`, reloads and replans
once, and emits the fresh attempt's result in the other envelope fields.
Outcomes remain in invocation order; each outcome's two vectors retain the
canonical sorting and duplicate rules defined above. Do not merge adjacent
outcomes or reinterpret an earlier `pending_effects` snapshot as final command
state. Omit observationally clean implicit recovery calls, so the ordinary
value is `recovery: []`. Thus JSON mode still emits exactly one envelope, while
human mode prints each recorded recovery change and recoverable effect before
the requested command result. The public Rust `recover()` API returns its
`RecoveryOutcome` directly; format 1 adds no `jails recover` CLI request. If
recovery changes authoritative state twice during one invocation, return
`RecoveryBlocked` rather than loop; the envelope still carries the first
successful outcome followed by the blocking error.

Add `--output human|json` with `human` default to mutation commands and
`app plan`. Those commands emit exactly one `CommandEnvelope` in JSON mode.
Existing read-only `--json` commands retain their established payload schemas
and byte-level compatibility; they do **not** wrap domain data in this mutation
envelope or normalise through a fake `CommandReport` variant. A future unified
read-only envelope is a separately versioned API decision.

The `jails.command-result.v1` JSON encoding is normative and uses a dedicated
encoder, not Serde defaults:

- emit one compact UTF-8 object followed by `\n`, with struct fields in their
  declaration order and snake-case field names; escape only JSON-required
  characters/control bytes and otherwise preserve UTF-8;
- encode enum variant names in kebab case. A unit enum is a string; a newtype or
  tuple variant is `{ "variant": value-or-array }`; a struct variant is
  `{ "variant": { ...fields... } }`. No internal `tag`/`content` keys exist;
- emit every declared field. `Option::None` is `null`, an empty vector/set/map
  is `[]`, and `recovery` is therefore always present. Sets are sorted arrays;
  maps are sorted arrays of `{ "key": ..., "value": ... }` so non-string
  typed keys never depend on a JSON implementation's object-key coercion;
- encode digest/ID newtypes as 64 lowercase hex, `ProjectPath` as its validated
  relative `/` string, `LegacySourcePath` as its exact relative `.jails/...`
  spelling, and `OperationTarget` by the externally tagged enum rule. Encode
  `u8`/`u32` and `FileMode` as JSON numbers, but every `u64` as an unsigned
  decimal string so consumers do not lose precision. `ObjectRef` remains an
  object with `id` and decimal-string `len`; and
- encode `FileImage::Absent` as `"absent"` and `Present` by the struct-variant
  rule. `receipt`, `report` and `error` are explicit `null` when absent; no
  status-dependent field omission is permitted.

A fully populated golden envelope exercises every nested enum shape, both
operation-target variants, recovery changes/effects, a receipt, report,
warning and error-compatible detail. Separate byte goldens cover minimal
preview, no-op, conflicted, and refusal envelopes. Decoder/JSON-schema fixtures
reject unknown fields/variants, missing fields, uppercase IDs, numeric `u64`,
unsorted set/map arrays, duplicate keys and trailing JSON values.

`project_commit` is `New` only when this
invocation crossed a new ledger commit point, `Existing` only for an effect
retry against an already committed receipt, and `None` for preview, no-op,
pre-commit refusal/stale input and recovery that blocked before this invocation
committed. A
`CommittedResult::DeferredError` still uses `New` because the logical commit is
durable. `error` contains a stable code,
message and sorted path details; it never contains a second prose-only result
beside a receipt. An effect retry returns the updated derived `AppliedReceipt`,
status `EffectRetried`/`EffectFailed`, and `project_commit: Existing`. A guard
mismatch caused by a later logical ledger transition returns the updated
receipt with `EffectSuperseded`, `project_commit: Existing`, no `error`, and no
subprocess attempt. Live-only drift and missing/corrupt descriptor objects use
their typed errors and do not rewrite or return an updated receipt.

Exit codes are fixed: `0` for clean preview, no-op, committed success or a
successful/superseded effect retry; `1` for a refusal, stale input, blocked
recovery or effect failure; `2` for a
prepared or committed conflict. A committed post-commit failure still returns
`1` but prints/serialises its receipt and `project_commit: New`.

`ErrorCode` JSON spellings are the declaration names converted to lowercase
kebab case (for example `stale-input` and `corrupt-machine-state`). This enum
is the exhaustive v1 registry in `src/report/error.rs`; command adapters may
add detail to `message` but may not invent string codes. Golden tests cover
every spelling and map each refusal/stale/lock/recovery/effect branch to one
code. Adding a code is an explicit command-result schema change.

Execution errors have one closed adapter mapping; command modules do not match
them independently:

| Execution result/error | `CommandStatus` / `ErrorCode` | Receipt |
|---|---|---|
| `CommitResult::NoOp` | `NoOp` / no error; exit `0` | none; `project_commit: None` |
| `CommittedResult { receipt.outcome: Applied, effect: NotApplicable\|Succeeded, .. }` | `Applied` / no error; exit `0` | committed derived receipt; `project_commit: New` |
| `CommittedResult { receipt.outcome: Conflicted, effect: NotApplicable, .. }` | `Conflicted` / no error; exit `2` | committed derived receipt; `project_commit: New` |
| `CommittedResult { receipt.outcome: Finalised, effect: NotApplicable\|Succeeded, .. }` | `Finalised` / no error; exit `0` | committed derived receipt; `project_commit: New` |
| `CommittedResult { receipt.outcome: Aborted, effect: NotApplicable, .. }` | `Aborted` / no error; exit `0` | committed derived receipt; `project_commit: New` |
| `EffectRunResult::Succeeded` | `EffectRetried` / no error; exit `0` | updated derived receipt; `project_commit: Existing` |
| `CommitError::StaleInput` or `EffectRunError::StaleInput` | `Stale` / `StaleInput` | none |
| `*::MutationBusy` | `Refused` / `MutationBusy` | none |
| `CommitError::EffectBusy` or `EffectRunError::EffectBusy` | `Refused` / `EffectBusy` | none |
| `*::RecoveryBlocked` | `RecoveryBlocked` / `RecoveryBlocked` | none |
| `*::CorruptMachineState` | `RecoveryBlocked` / `CorruptMachineState` | none |
| `CommitError::InvalidPrepared` or `EffectRunError::InvalidPlan` | `Refused` / `InternalInvariant` | none; debug builds additionally assert |
| `CommitError::PreActivationIo`, `EffectRunError::ReceiptIo`, `RecoveryError::Io` | `Refused` / `InputUnreadable` | none |
| `CommittedResult.effect == Failed` with `EffectFailureCode::Protocol` | `EffectFailed` / `CorruptMachineState` | committed derived receipt, `project_commit: New` |
| `CommittedResult.effect == Failed` with any other code | `EffectFailed` / `EffectFailed` | committed derived receipt, `project_commit: New` |
| `CommittedResult.effect == Superseded` | `EffectSuperseded` / no error | committed derived receipt, `project_commit: New` |
| `CommittedResult.effect == DeferredError { StaleInput }` | `EffectFailed` / `StaleInput` | last checksum-validated committed receipt, `project_commit: New` |
| `CommittedResult.effect == DeferredError { CorruptMachineState }` | `EffectFailed` / `CorruptMachineState` | last checksum-validated committed receipt, `project_commit: New` |
| `CommittedResult.effect == DeferredError { ReceiptIo }` | `EffectFailed` / `InputUnreadable` | last checksum-validated committed receipt, `project_commit: New` |
| `CommittedRecoveryRequired { error: Io, .. }` | `RecoveryBlocked` / `InputUnreadable` | optional only when checksum-validated; operation/transaction IDs, `project_commit: New` |
| `CommittedRecoveryRequired { error: RecoveryBlocked, .. }` | `RecoveryBlocked` / `RecoveryBlocked` | optional only when checksum-validated; operation/transaction IDs, `project_commit: New` |
| `CommittedRecoveryRequired { error: CorruptMachineState, .. }` | `RecoveryBlocked` / `CorruptMachineState` | optional only when checksum-validated; operation/transaction IDs, `project_commit: New` |
| `EffectRunResult::Failed` with `EffectFailureCode::Protocol` | `EffectFailed` / `CorruptMachineState` | updated derived receipt, `project_commit: Existing` |
| `EffectRunResult::Failed` with any other code | `EffectFailed` / `EffectFailed` | updated derived receipt, `project_commit: Existing` |
| `EffectRunResult::Superseded` | `EffectSuperseded` / no error | updated derived receipt, `project_commit: Existing` |

Any `ApplyOutcome`/`CommitEffectOutcome` pairing not listed is an internal
invariant failure. In particular, conflict and abort receipts cannot carry an
executable effect; failure/deferred/superseded effect outcomes take the typed
effect-status rows below instead of hiding the committed logical outcome.

`RecoveryError::MutationBusy/RecoveryBlocked/CorruptMachineState` use the same
three corresponding rows. A failure after the ledger commit point is never
returned as `CommitError`. If structural recovery can complete the linked
journal/receipt protocol, the API returns `CommittedResult` with its typed
effect outcome and last validated receipt; if it cannot yet do so, the API
returns `CommittedRecoveryRequired` with the exact unfinished stage and only a
checksum-validated optional receipt.
`--pretend`/`app plan` may prepare a `CommitPlan` in scratch or describe an
`EffectRetryPlan` directly, but acquire no mutation lock, write no
project/machine state, perform no migration/deletion and run no post-commit
effect. Their report warns that commit/resume rechecks staleness.

Primary touchpoints: new `src/planning/prepare.rs` and `report.rs`, evolved
`src/model/mod.rs::Change`, `src/generate/write.rs`, `src/add.rs`, `src/pom.rs`,
`src/compose.rs`, `src/config.rs`, `src/codemod.rs`, `src/template.rs`,
`src/run.rs` formatter hooks and `src/main.rs` output/exit handling.

R3 gate:

- every touched path has one exact guarded op, absent ledger is represented,
  and the full read set/tool context participates in the fingerprint;
- for a commit transition, `--pretend`, JSON, human output and eventual commit
  consume the same `PreparedBundle.change`; for an effect retry they consume
  the same `EffectRetryPlan`. There is no second dry-run renderer and absolute
  runtime bindings never enter durable/report bytes;
- preparing the after-state is empty and tool formatting is idempotent;
- focused tests cover every semantic edit, create/replace/delete, relevant mode
  change, absence, symlink/device/case-fold/path refusal, duplicate contributor,
  create-delete collapse and canonical encodings;
- first-migration fixtures freeze the complete legacy snapshot/translation,
  preview every non-ledger source as a labelled `LegacyMachine` delete with no
  contributor, reject any extra/missing target, and prove schema-1 ledger bytes
  are never decoded as schema 2;
- formatter tests cover timeout, nonzero exit, killed child, surprise file,
  undeclared deletion, symlink, output limit, cleanup failure and a valid
  multi-file diff;
- synthetic conflict fixtures produce exact marker operations/pending ledger
  bytes and exercise `Finalise`/`Abort` representation; actual Git merge and
  conflict lifecycle acceptance remains an R5 gate, while every non-conflict
  preparation error returns no prepared value;
- spy filesystem/process backends prove preparation never mutates the managed
  root and R4 receives no semantic bucket, renderer or closure; and
- shadow prepared operations match the current command golden outputs but no
  production writer has yet switched.

### R4 — Recoverable commit — SHIPPED

Gate: `crates/jails-commit/tests/crash.rs` converges at all 21 named
failpoints, and `tests/engine.rs` now sweeps the same set through a *whole*
capability install -- either completely applied or completely absent, with
recovery idempotent on the second pass.

Make a `PreparedChange` durable without pretending several filesystem names
change instantaneously. Default crash recovery rolls a fully persisted,
validated journal **forward**. Preimages exist for guarded explicit abort and
audit, not as the default crash policy or a universal rollback API.

#### R4.1 Storage, dependencies and cooperative lock

Add lockfile-pinned `fs2` and use
`fs2::FileExt::try_lock_exclusive` on `.jails/lock`. It provides advisory locks
on supported Unix filesystems without project-local unsafe/libc code. A PID lockfile is
not acceptable: process IDs are reused and stale-file cleanup creates a race.
Across Immediate, R1, R3 and R4, the only architecture dependencies added are
`sha2`, `fs2`, `tempfile`, and `nix` with default features disabled and only
`process,signal`; ledger/journal codecs remain hand-written and closed rather
than adding serde/TOML. R3 preparation and R4 post-commit effects share the one
`BoundedProcessRunner`; no second kill/timeout implementation or async runtime
is permitted.

Bootstrap `.jails` before opening the lock with `symlink_metadata`: `NotFound`
uses one `create_dir` (a racing `AlreadyExists` is rechecked); an existing real
directory is accepted; a symlink or non-directory refuses. Open/create
`.jails/lock` mode `0600`, acquire it, then compare the open handle's Unix
device/inode with the path's post-lock metadata and refuse a symlink or changed
entry. This lock is cooperative concurrency control, not a defence against a
malicious user swapping paths continuously. Create `transactions`, `receipts`
and `objects` only after the lock is held. Hold the file handle through
recovery or commit. Never
delete the lock file: deleting/recreating changes the locked inode and permits
two holders. After acquisition, replace its informational content with current
PID, command and start time and sync it; this content is diagnostic only. On
contention read it best-effort and return exit 1: `another jails mutation holds
.jails/lock`; never wait invisibly. Every in-project mutator routes through
this lock by R6. Read-only/pretend commands do not acquire it and report an
incomplete transaction without recovering it.

The bootstrap returns `CreatedByThisInvocation` or the existing directory's
device/inode. Under the acquired lock, compare it with the runtime
`MachineRootBinding` and durable presence precondition. Snapshot `Absent`
requires `CreatedByThisInvocation`; a racing pre-existing directory is stale.
Snapshot `Present` requires the same recorded device/inode. The expected
post-bootstrap presence is then accepted as the parent of dedicated machine
leaves such as `.jails/app.toml` and the ledger. `.jails` itself never appears
in `DirectoryOp`, so executor bootstrap cannot make its own prepared change
stale.

Apply the analogous rule to `MachineReceiptDirectoryState` after creating the
fixed subdirectories under the lock. Snapshot `Absent` requires this invocation
to have created a still-empty real `receipts` directory; snapshot `Present`
requires the same exact sorted transaction listing/digest captured by the
loader. A symlink, non-directory, unknown entry or a receipt appearing or
disappearing before activation is stale/corrupt as classified by its decoded
contents. Executor bootstrap therefore does not invalidate a recorded absence,
but it also cannot hide a concurrent receipt inventory change.

The pre-activation failure promise is deliberately precise. Lock bootstrap,
even for a stale or refused commit, may create the executor-owned `.jails`
coordination shell, persistent lock files and fixed machine subdirectories, and
may rewrite/sync diagnostic lock contents. It may not create or alter a managed
project leaf, human declaration, ledger, transaction, receipt, migration or
content object before activation. The same boundary applies to mutation-lock
contention, `EffectBusy`, stale preconditions and pre-activation I/O refusal.
`plan`, `prepare` and `--pretend` remain stricter: they create none of this
machine state. Tests compare these two promises separately instead of asserting
that a commit attempt wrote literally no path.

Storage is fixed:

```text
.jails/lock
.jails/effects.lock
.jails/transactions/<64-hex-transaction-id>/
    journal.bin
    journal.bin.tmp
    objects/sha256/<first-two>/<remaining-62>
    live-temp/<operation-index>.publish|.deleted
.jails/receipts/<64-hex-transaction-id>/
    journal.bin                             # Complete; immutable recovery witness
    receipt.bin
    receipt.bin.tmp
    objects/sha256/<first-two>/<remaining-62>   # before R5 promotion
```

Execution authority is intentionally partitioned. The current ledger owns
logical ownership/provenance/pending state. One validated active journal owns
the allowed continuation of an incomplete transaction. A receipt owns
immutable prepared history and its mutable effect state. An object file owns
only the bytes of a hash referenced by one of those records. The shared live
tree owns the user's current file contents. Recovery may rely on journal phase
only after validating its prepared identity and classifying ledger/live images;
no receipt, object listing or journal may be mined to synthesize current desire.

Create/open `effects.lock` mode `0600` only while holding `lock`, and perform
the same open-handle/path inode and symlink checks. `effects.lock` follows the
same persistent-inode rule as `lock`; it serialises external effect execution
and fences every project commit from crossing its activation point while an
effect is running. A commit or effect runner always tries it while holding the
project lock, never waits, and releases the project lock immediately on
contention. The runner may then retain `effects.lock` without the project lock
during the subprocess. Transaction/receipt directories are mode `0700`
where supported. All staging
is beneath `.jails`. Before activation, compare the transaction directory's
device with the already-created `.jails/receipts` parent and with every
existing target parent (including parents reached through a nested mount), and
refuse a cross-device receipt publication or file operation; do not infer this
from path ancestry. The absent receipt destination is checked before
activation. Object writes use `create_new`, write all bytes,
`sync_all`, verify length/hash by reread, then fsync containing directories.
Finding an existing object is acceptable only after exact hash/length
verification.

Receipt retention is a deterministic mark-and-sweep, not an age check applied
one directory at a time. The initial roots are: the latest 32 valid committed
receipts in each of the four `PreparedKind` **discriminant** buckets
(`Apply`/`Conflict`/`Finalise`/`Abort`); the unique origin receipt selected by
the current `PendingConflict`'s complete immutable-structure match; and every
receipt with an executable `Deferred`, `Pending`, `Running` or `Failed` effect.
“Latest” sorts by `(generation, transaction_id)`; mtime is never authority.
`LedgerV2.last_operation` is not a transaction locator and creates no receipt
root. Format 1 has no administrative-pin store or pin/unpin API; adding one is
a future protocol and confined-machine-state design, not an implied root.
Conflict effect intents are not `EffectReceipt`s. `Succeeded` and `Superseded`
are terminal and add no root by themselves.

Next traverse receipt dependencies to a fixed point. Every retained
`Finalise { origin }` or `Abort { origin }` receipt pins the exact origin
transaction stored in its immutable semantics, recursively. Only after this
graph is marked may cleanup delete an unmarked receipt. Thus a retained
finalisation/abort can never outlive the conflict receipt that recovery needs
to validate it, even when that origin's own effects are terminal. A missing
dependency during marking is corruption, not permission to prune the
dependent.

A successful transaction directory is atomically published at
`.jails/receipts/<id>` as specified below; it is not journal debris. Retention
cleanup occurs only after a later successful commit, never removes an
incomplete transaction, and recomputes the complete root/dependency graph
before each sweep. R5 promotes shared base objects and computes object GC roots
from all retained ledger/journal/receipt closures before pruning any
receipt-local copy or global object.

#### R4.2 `JournalV1`

`journal.bin` is a closed canonical binary format, not ad-hoc TOML:

```rust
struct JournalV1 {
    magic: [u8; 16],                    // `JAILS-JOURNAL-1\0`
    transaction: TransactionId,
    generation: u64,
    root_identity: RootIdentity,
    state: JournalState,
    prepared: PreparedIdentityV1,
    record_checksum: ObjectId,
}

enum JournalState {
    Prepared,                           // validated, no live mutation allowed
    Active,                             // recovery must roll forward
    LedgerCommitted,
    Complete,
    Blocked { resume: ResumeState, path: Option<ProjectPath>,
              reason: BlockReason },
}

enum ResumeState { Prepared, Active, LedgerCommitted, Complete }

enum ObservedImage {
    Before,
    After,
    Unknown { actual: ActualImage },
    Unreadable { error_kind: String },
}

enum ActualImage {
    Absent,
    File { sha256: ObjectId, len: u64, mode: FileMode },
    Directory,
    Symlink,
    Other,
}

enum BlockReason {
    UnknownLiveImage { actual: ActualImage },
    Unreadable { error_kind: String },
    RootChanged,
    CorruptJournal,
    CorruptObject(ObjectId),
    MultipleTransactions,
}

struct ReceiptV1 {
    magic: [u8; 16],                    // `JAILS-RECEIPT-1\0`
    transaction: TransactionId,
    generation: u64,
    prepared: PreparedIdentityV1,
    complete_journal_checksum: ObjectId,
    post_commit: Vec<EffectReceipt>,
    record_checksum: ObjectId,
}
```

The remaining R4 wire tags are fixed: `JournalState`
prepared/active/ledger-committed/complete/blocked = `0/1/2/3/4`;
`ResumeState` prepared/active/ledger-committed/complete = `0/1/2/3`;
`ObservedImage` before/after/unknown/unreadable = `0/1/2/3`;
`ActualImage` absent/file/directory/symlink/other = `0/1/2/3/4`; and
`BlockReason` unknown-live/unreadable/root-changed/corrupt-journal/corrupt-
object/multiple-transactions = `0/1/2/3/4/5`. `FileMode` is one big-endian
`u32` whose only permitted bits are `0o777`; platforms that cannot apply and
verify the prepared mode refuse before activation.

`PreparedIdentityV1.operations` and `.directories` are the only execution
authority. All replace/delete preimages and the prior ledger bytes are
transaction objects, not merely hashes. Publication/preimage temporary names
are derived as a pure function of `(transaction, canonical operation index)`;
they are never persisted as a second path authority. Encoding uses fixed enum
tags and big-endian length-prefixed bytes; paths use validated UTF-8 `/` form.
Parsing rejects unknown version/tag, duplicates, unsorted collections,
object/path escape, transaction/directory-name mismatch and trailing bytes.

Validate either durable record in this order: decode under limits; require the
stored record checksum to equal SHA-256 of the domain-separated canonical
encoding of every preceding field (`JAILS-JOURNAL-STATE-1` or
`JAILS-RECEIPT-STATE-1`, excluding only the checksum itself); require the
directory name and stored `transaction` to equal
`SHA256("JAILS-PREPARED-1" || encode(prepared))`; require stored `generation`
to be nonzero; require `prepared.operation_id` to equal
`SHA256("JAILS-OPERATION-1" || encode(prepared.operation_identity))`; require
`prepared.input_preconditions` to equal
`prepared.operation_identity.snapshot.read_set.inputs`; validate the closed
tool-template/full-fingerprint correspondence from R3; resolve and
hash/length-check every member of
`prepared.object_manifest`; require a present ledger after-object, decode its
strict schema-2 payload, and require its generation to equal the stored
generation; recompute
`prepared_object_closure(prepared_without_manifest)` across the complete
immutable identity and its declared typed-object edges, require byte-for-byte
equality with the manifest, and reject extras; then apply journal- or
receipt-specific checks. An object
resolver may find identical bytes in the record-local `objects` directory or,
after R5 promotion, the global content-addressed store. Pruning a local copy is
allowed only after the global copy is synced and verified. Missing bytes in
both locations are corruption.

Receipt-specific validation also decodes the sibling `journal.bin`, requires
`JournalState::Complete`, verifies its own checksum, and requires that checksum
to equal `ReceiptV1.complete_journal_checksum`. Transaction, generation and
`PreparedIdentityV1` must be byte-identical across the two records. Because the
receipt record checksum covers `complete_journal_checksum`, the existing
`MachineReceipt.record_checksum` precondition cryptographically binds both
published records; a journal edit cannot evade the receipt guard.

After structural validation, decode the required schema-2 `ledger_after` and
derive one canonical logical before-state. With `migration = None`, decode a
present `ledger_before` strictly as schema 2; `Absent` is the empty
generation-zero state. With `migration = Some(m)`, require
`PreparedKind::Apply`, require the exact raw `ledger_before` to equal
`m.snapshot`'s `Schema1Ledger` image or absence, rerun `translate_legacy` from
every frozen source/listing, and require exact equality with
`m.translated_before`; do not feed schema-1 bytes to the schema-2 decoder. That
translated draft is the generation-zero logical before-state, and
`ledger_after.generation` must be 1. Then run this exhaustive semantic
validator. Define `successful_tables(L)` as the value tuple
`(applied, one_shots, resources, outputs, legacy)`; generation,
`last_operation`, and `pending_conflict` are deliberately excluded from that
helper. Every non-no-op kind requires
`after.generation == before.generation + 1` with checked arithmetic and
`after.last_operation == Some(prepared.operation_id)`.

| `PreparedKind` | Required semantic and ledger shape | Operations and effects |
|---|---|---|
| `Apply` | `OperationSemanticsV1::Apply`; logical `before.pending_conflict == None`; `after.pending_conflict == None`; the after successful tables equal the fully prepared projection of the embedded `PlannedSubject` and `LedgerIntent` over the schema-2 or exactly translated logical before-state. | With no migration, every file target is `Project`. With migration, `LegacyMachine` operations are exactly the present non-ledger legacy sources as guarded deletes with empty contributors; `Schema1Ledger` is consumed only by the ledger transition. Ordinary project operations/directories and the aggregate-effect rule remain unchanged. |
| `Conflict { paths }` | Apply semantics; no prior pending conflict; `successful_tables(after) == successful_tables(before)` exactly; `after.pending_conflict` exists once, names this operation/after generation/invocation, and its complete candidate equals the fully prepared apply projection. Its sorted unique path list equals `paths`. | Marker and frozen-clean postimages cover every file op exactly; no unrelated operation exists. `prepared.post_commit` and the conflict receipt's effect vector are empty. `pending.effect_intents` equals the semantic effect-intent derivation from the candidate transition. |
| `Finalise { origin }` | Finalise semantics names the same origin, selected origin transaction, current pending identity and exact path-sorted resolutions; `ledger_before` is byte-identical to the origin conflict receipt's ledger after-image; `ledger_after.pending_conflict == None`; successful after tables equal promotion of the complete candidate with only `ResolveFromLive` current images filled. | `operations` and `directories` are empty. `post_commit` equals deterministic materialisation of every pending effect intent from the exact resolution images; each receipt state starts `Deferred`. The origin receipt has no effect state to transfer or rewrite. |
| `Abort { origin }` | Abort semantics names the same origin and selected origin transaction; `ledger_before` is byte-identical to the origin conflict receipt's ledger after-image; successful after tables equal successful before tables; only pending state is cleared. | Operations equal the path-complete guarded inverse of the origin receipt's file results, with no extra/missing path; directories and `post_commit` are empty. Pending effect intents are discarded by the new ledger state; the origin receipt remains immutable. |

The origin receipt for finalise/abort is selected before preparation from the
captured receipt set. A match requires all of: conflict kind and identical
sorted paths; operation and generation equal the pending record; its exact
`ledger_after` equals the current ledger image; every conflict/clean operation
postimage equals the pending marker/frozen record; its prepared executable
effect vector is empty; and its ledger pending intents equal the current
`pending.effect_intents`. Require exactly one match. The
plan copies that receipt's transaction, generation and `record_checksum` into
`ConflictOrigin`/`MachineReceipt`. Preparation and commit revalidate the guard;
zero, multiple, missing, a nonempty origin effect vector, or any
intent/checksum mismatch refuses before a new journal. This exact
structural selection pins one receipt without putting the self-referential
transaction hash inside `ledger_after`.

Run the same structural matrix while preparing, decoding a journal/receipt,
recovering, and before activation. A structurally valid record with a nonempty
conflict-receipt effect vector or an impossible kind is corruption,
not a variant to execute leniently. Golden negatives mutate each
kind, semantics, pending table, path, origin, operation, descriptor, effect
state and generation independently and require rejection.

`root_identity`, `JournalV1.state`, `ResumeState`, derived temporary names and
mutable `EffectState` are execution progress and are excluded from prepared
identity; the operation ID and original ordered `PostCommitEffect` descriptors
remain included. Golden tests prove a journal-state/effect-state rewrite
preserves the transaction ID, changes the record checksum, and any immutable
prepared byte changes both. A state rewrite always recomputes the checksum
before the temp-file sync/rename protocol; a checksum mismatch is corruption,
not an incomplete state transition to guess through.

`ReceiptV1.prepared` is the self-verifying immutable audit record. Its only
mutable section is `post_commit`, replaced atomically and synced after each
effect transition. It must contain exactly one `EffectReceipt`, in prepared
descriptor order, for every `prepared.post_commit` item; descriptor and
`EffectId` mismatches are corruption. `FileReceipt`, `DirectoryReceipt`,
`AppliedReceipt` and `ApplyOutcome` are public/report projections derived from
the prepared operations, kind and effect states, never parallel durable
authorities. In particular, `NoOp` has no receipt;
the committed outcome follows `PreparedKind`, and effect success/failure is
reported only by the separate `EffectReceipt` states and command status. This
keeps logical project outcome orthogonal to an external effect attempt.

`EffectReceipt` carries an `EffectId =
SHA256("JAILS-EFFECT-1" || operation || index || descriptor)`; this is the
idempotency key handed to an effect implementation and logged in errors. The
prepared invocation is the canonical secret-free identity from R5, not raw
argv. A conflict receipt is pinned while the ledger names its operation; an
abort/finalisation creates a new receipt and never rewrites the old receipt's
prepared kind or historical file result. A conflict receipt has no executable
effect vector; its immutable ledger after-image carries semantic effect intents
until finalisation materialises them or abort clears them.

Receipt publication is one closed same-filesystem rename protocol. While the
directory is still at `transactions/<id>`: persist and sync
`JournalState::Complete`; construct `ReceiptV1` with that exact Complete-record
checksum; write/sync/reread-validate `receipt.bin` and the pair; remove only derived
publication/deletion temps and stale `*.tmp` files; fsync the transaction
directory; and require the absent `receipts/<id>` destination again. Then
atomically rename the **intact directory**, including its Complete
`journal.bin`, to `receipts/<id>`, fsync both the `transactions` and `receipts`
parent directories, and finally fsync `.jails`. The Complete journal remains
as an immutable recovery witness; `receipt.bin` is the receipt/effect-state
authority, and validation requires both immutable identities to match exactly.
There is no copy-then-delete publication and no window in which the journal is
deleted before the receipt directory is durable.

For a known transaction ID, recovery accepts exactly one placement. A
transaction-only directory is validated and completed/moved forward; a
receipt-only directory must contain a valid Complete journal and matching
receipt and is already published. Both placements, a pre-existing destination,
an in-receipts non-Complete journal, or mismatched prepared identities are
corruption. Atomic same-device rename plus parent fsync makes a neither
placement unreachable for a directory recovery already observed; recovery
never fabricates one from ledger state alone.

Persist a journal state change by writing `journal.bin.tmp` with `create_new`,
syncing, renaming over `journal.bin`, then fsyncing the transaction directory.
Persist a receipt effect-state change through the identical
`receipt.bin.tmp → receipt.bin` protocol in its receipt directory. A stale
receipt temp never wins over a valid checksum-bearing receipt. A missing valid
receipt in a transaction directory after its ledger commit is reconstructed
from the immutable journal before that directory is published; a published
receipt directory missing either durable record is corruption.
The
executor may not touch a live target until `Active` is durable. A directory
with no valid journal is unvalidated staging and can be removed under the lock
only if no `journal.bin` ever existed; corrupt/unknown journals are preserved
and block every mutation.

`journal.bin.tmp` is never authoritative. On recovery, a valid `journal.bin`
wins even when a synced temp contains a later phase, because live before/after
classification reconstructs progress; remove the stale temp under the lock
before the next journal rewrite. A temp without `journal.bin` can only precede
activation and is unvalidated staging, so validate its transaction/path
confinement and remove the whole unactivated directory. An invalid temp beside
a valid journal is removed; an invalid `journal.bin` itself is preserved and
blocks. Never promote a temp by guessing that its rename was intended.

#### R4.3 Commit protocol and durability

Expose `recover(ProjectHandle) -> Result<RecoveryOutcome, RecoveryError>`, which
acquires/releases the lock, and an executor-private
`recover_locked(&LockedProject) -> Result<RecoveryOutcome, RecoveryError>`.
A mutating command's outer
driver is `recover → load → resolve → plan`, then either
`prepare → commit` for `PlannedTransition::Commit` or `resume_effect` for
`RetryEffect`. Read-only and pretend paths replace the first step with a
non-mutating recovery-status read and never resume an effect.
`commit(ProjectHandle, PreparedBundle) -> Result<CommitResult, CommitError>` is
the only clean project/ledger
mutation entrypoint; it has no parser, command request or planner and therefore
never re-plans. `resume_effect(ProjectHandle, EffectRetryPlan) ->
Result<EffectRunResult, EffectRunError>` is the only
entrypoint that changes receipt effect state outside commit/recovery; it
performs the same lock/checksum compare-and-swap and external-call handoff in
step 12 and cannot receive file operations or ledger bytes:

1. Resolve `ProjectHandle` without following a symlink and compare
   `CommitContext.project_root`, then bootstrap/acquire the lock even for a
   prepared no-op and recheck the root identity under that lock. Run
   `recover_locked` first. If
   `outcome.changes` is nonempty, release the lock and return
   `CommitResult::RecoveredPriorTransaction(outcome)`; the outer command driver may run
   the entire pipeline once more. A second such result in one invocation is an
   actionable concurrency error. The executor itself does not reload or plan.
2. Recheck **every** project/external
   `InputPrecondition`, every operation's before/absence, and the exact
   `ledger_before` under the lock. Resolve an external precondition only through
   the matching runtime `CommitContext` binding. `ExternalFile` requires the
   exact confined/canonical regular-file type, length and hash;
   `ExternalAbsent` repeats the root-identity/confined no-symlink walk and
   requires the leaf absent. A missing, duplicate or extra binding refuses.
   Any mismatch returns stale/refusal before an
   active journal. A command may present a newly prepared report for explicit
   reconfirmation; commit never substitutes changed operations silently.
3. If file/directory/ledger images are unchanged and there is no post-commit
   effect, release the lock and return `CommitResult::NoOp`. This
   ordering makes no-op truthful for the rechecked snapshot rather than a race
   that bypasses incomplete-transaction recovery.
4. Still holding the project lock, non-blockingly acquire the validated
   persistent `effects.lock`. If it is held, release the project lock and
   return `EffectBusy` before creating a transaction directory or writing any
   project state. Hold both locks through activation, the ledger commit point,
   and receipt publication. A command
   with its own executable post-commit effect retains the effect lock for step
   12; every other command releases it after step 11. This deliberately fences
   *all* commits, rather than trying to predict which future file/resource
   transition is relevant to an in-flight external process.
5. Clean unactivated staging. Validate that `PreparedChange.objects` exactly
   matches `PreparedIdentityV1.object_manifest`, recompute `TransactionId` from
   that immutable identity, then create its absent transaction directory. An
   existing receipt with that ID means the snapshot/generation is stale; an
   existing transaction is recovery work. Persist and verify every manifest
   object before activation. Preimages are read only after their hashes were
   rechecked. Write, sync and reread-validate the `Prepared` journal.
6. Rewrite/sync journal as `Active`. From this instant any failure leaves
   recovery work; do not return an ordinary “nothing written” error.
7. Create parent directories shallowest-first with `create_dir`, fsyncing each
   parent. On initial execution, an already-existing path violates the captured
   absence and blocks. During active recovery, an ordinary directory is the
   permitted after-state; a file, symlink or other kind blocks. Directories are
   recorded but deliberately never removed by abort/removal.
8. Execute file ops in canonical path order. Immediately reclassify the live
   path before each op; `Before` applies, `After` is already complete,
   `Unknown/Unreadable` records `Blocked` and stops.
9. `Create` copies the immutable content object into a distinct
   `live-temp/<operation-index>.publish` inode, verifies it, applies the prepared mode,
   syncs it, then hard-links that publication inode to the absent destination
   and fsyncs the parent. It never links a mutable live path to an immutable
   content/preimage object. If atomic no-replace hard links are unsupported,
   refuse that filesystem before activation; never stream a partial live
   create. `Replace` likewise writes/verifies the transaction-local
   `<index>.publish`, syncs it, atomically renames it over the guarded old path
   and fsyncs both directories. `Delete` atomically renames the guarded live
   file to transaction-local `live-temp/<index>.deleted` and fsyncs both
   directories. No publication temp is created beside a user file. Every operation is thus
   observably either its complete before or complete after image.
10. Recheck that all operations/directories classify `After`. Apply the exact
   `ledger_before → ledger_after` transition with the same guarded
   create/replace primitive **last**, then fsync `.jails`. The required present
   ledger after-image is the commit point.
11. Persist `LedgerCommitted`, then execute the exact Complete-journal/
    checksum-linked-receipt/intact-directory publication protocol above,
    including both parent-directory syncs. If any journal/receipt/publication
    step now fails, classify it once using the same structural matrix, retain
    every authoritative record/object, release the locks and return
    `CommittedRecoveryRequired` with the matching stage and `Io`,
    `RecoveryBlocked` or `CorruptMachineState`; include a receipt only if the
    pair was checksum-validated, and never fabricate one, return `CommitError`,
    or rerun planning after the known ledger commit. If
    `PreparedKind::Conflict`, leave only its semantic intents in pending state,
    return exit 2 and return `CommitResult::Committed(CommittedResult {
    receipt: derived_conflicted_receipt, effect: NotApplicable })`; it has no
    executable effect record. Finalise and abort do
    not rewrite the origin receipt: finalise owns newly materialised effects in
    its own receipt, while abort owns none. Still holding both locks, scan older
    executable receipts and atomically mark only ledger-guard mismatches caused
    by this committed operation `Superseded { by: Some(operation_id) }`. A
    crash or I/O error in this metadata pass returns
    `CommittedRecoveryRequired { receipt: Some(current_receipt), stage:
    ReceiptReconciliation, .. }`; the next structural recovery repairs it and
    no external effect starts first.
    live-only drift never enters this transition.
12. For `Apply`/`Finalise`, run canonical idempotent post-commit effects in
    recorded order. The common external-call runner is entered only by commit or
    `resume_effect`: commit retains `effects.lock` from step 4, while
    `resume_effect` acquires it non-blockingly while holding the project lock.
    Contention performs no state transition and returns/skips with `EffectBusy`.
    Structural recovery may acquire the same lock for receipt metadata repair,
    but never enters the external-call branch.

    Before `Pending → Running`—and before reviving a first-attempt `Running`
    state—validate the complete descriptor guard: immutable document images,
    complete ledger service map, ledger output-current image when owned, and
    actual live `compose_output` equal to `after_document` presence/image. For an executable
    `Apply`/`Finalise` receipt whose ledger guard was superseded by a later
    operation, atomically persist
    `Superseded { by: current_ledger.last_operation }`, run no subprocess and
    treat that state as terminal. A live-only image mismatch makes no receipt
    transition. `resume_effect` returns `Err(EffectRunError::StaleInput)`;
    commit, which has already crossed its ledger point, returns
    `CommittedResult.effect = DeferredError { error: StaleInput, .. }` with the
    last validated receipt. Missing/corrupt object bytes are machine-state
    corruption, not supersession, and map similarly to the typed resume error or
    committed `DeferredError`. Conflict receipts never enter this runner;
    their pending semantic intents are not `EffectReceipt`s.

    For a matching guard, atomically persist `Deferred → Pending {
    next_attempt: 1 } → Running { attempt: 1 }` around the first call. A crash
    in `Pending { next_attempt: n }` resumes exactly `Running { attempt: n }`;
    the next attempt is never inferred. A receipt left `Running { attempt: 1
    }` after process death may advance once to `Running { attempt: 2 }`; finding
    an orphaned `Running { attempt >= 2 }` is the structural-recovery transition
    above, never an external-runner input. An explicit retry
    of `Failed { attempt: n, ... }` passes through `Pending { next_attempt: n +
    1 }` and uses that checked `Running` attempt. A returned subprocess error records `Failed`
    with the current attempt. Zero and overflow attempts are invalid durable
    state. Release the project lock but retain `effects.lock` during the
    external call. Because every commit must acquire that same effect lock
    before activation, no project commit can cross its commit point during the
    call. Read-only work may continue.

    After the call, reacquire the project lock **blocking** while retaining the
    effect lock. This is the sole blocking lock acquisition in the protocol and
    is deadlock-free: every competing mutator holds the project lock only long
    enough to fail its nonblocking `effects.lock` attempt and release. Do not
    release `effects.lock` or expose an unfenced result while waiting.

    Reread the same receipt first. A decode/checksum/generation/descriptor or
    expected-`Running` CAS mismatch is unknown receipt state: make no rewrite,
    release both locks, and return `EffectRunError::CorruptMachineState` from
    resume or committed `DeferredError::CorruptMachineState` with the pre-call
    last-validated projection. If the receipt is exact but project-root identity
    changed, use the same no-rewrite result because even the receipt path is no
    longer trusted. Only with the exact expected receipt and root may the runner
    revalidate ledger/live guards. An out-of-protocol ledger/live mismatch then
    CASes that exact `Running` state to `Failed { code: Protocol, ... }`, reports
    corruption, and never silently blesses the call as success. Otherwise
    persist `Succeeded` or the structured subprocess `Failed`, then release both
    locks. No second process can run an effect or rewrite its receipt
    concurrently. If a receipt transition cannot be persisted and reread, keep
    the last checksum-validated projection: `resume_effect` returns
    `Err(EffectRunError::ReceiptIo)`, while commit returns
    `CommittedResult.effect = DeferredError { error: ReceiptIo, .. }`. Neither
    path guesses whether an unvalidated temp/rename completed; structural
    recovery resolves it from the durable checksum protocol.

    `plan_all` scans only captured validated receipts and selects exactly one
    eligible `Apply`/`Finalise` receipt whose prepared invocation equals the
    current invocation and whose descriptor/index/`EffectId` are internally
    valid. `Deferred`, valid `Pending`, or first-attempt `Running` produces
    `reason: Interrupted`; a retryable `Failed` produces `reason:
    ExplicitRetry`. `Failed { code: Protocol }` produces that plan only after
    the captured root/descriptor/ledger/live guard is exact again; otherwise it
    is a `CorruptMachineState` refusal. The plan
    pins the complete `expected_state` and receipt checksum. Zero matches
    continues ordinary planning; multiple matches refuse with sorted operation
    and effect IDs. One match becomes `EffectRetryPlan`—never a `CommitPlan`.

    `resume_effect` acquires the project lock, runs `recover_locked`, rereads
    that one transaction, then acquires `effects.lock` and compare-and-swaps
    the exact checksum, descriptor, ID and `expected_state`. The reason/state
    pairing above is validated exhaustively. A receipt mismatch is
    stale and runs no effect. Classify a descriptor guard failure exactly as in
    the common runner: a logical ledger mismatch CASes the exact receipt to
    `Superseded` and returns `EffectRunResult::Superseded` without first moving
    to `Pending`; a live-image mismatch with the logical ledger still matching
    returns `EffectRunError::StaleInput` with no receipt rewrite; and a
    missing/corrupt referenced object returns
    `EffectRunError::CorruptMachineState` with no receipt rewrite. If
    `recover_locked` returned a nonempty `outcome.changes`, return
    `EffectRunResult::RecoveredPriorTransaction(outcome)` and let the outer driver rerun
    the whole pipeline once. Otherwise execute only the lock-handoff retry
    protocol and return `Ok(Succeeded|Failed|Superseded)` or a typed error.
    This invocation cannot also create a project transaction. Other invocations
    and structural recovery never execute an effect. An effect failure never
    rolls back committed files and never changes the historical file outcome.

    After the applicable effect work, return
    `CommitResult::Committed(CommittedResult { receipt:
    last_checksum_validated_applied_receipt, effect: exact_outcome })`.
    Succeeded/Failed/Superseded must match the terminal receipt state;
    pre-terminal stale/corrupt/receipt-I/O failures use the matching
    `DeferredError`. A failed/deferred-error effect remains a committed result
    whose logical `ApplyOutcome` still follows `PreparedKind` and whose command
    envelope uses exit 1; it is not converted into a pre-commit error or a
    durable `ReceiptV1` API value.

Call `File::sync_all` for file contents and `sync_all` on each changed directory
handle. The initial support contract is Linux and other Unix filesystems on
which the repository builds and these durability primitives/hard links behave
as tested. Windows is explicitly unsupported until the existing unconditional
`std::os::unix` code (including `testd`) is ported and a Windows journal backend
passes the same crash suite; do not leave substitute primitives to an
implementer in this phase. A directory-sync `Unsupported` result is a startup
refusal before activation; other sync errors keep the journal for recovery.
Network/filesystems that do not honour the primitives fail closed.

#### R4.4 Recovery and blocked UX

Every mutating command calls `recover(ProjectHandle)` once before snapshot and
`commit` calls it again under its lock. `doctor` and read-only plan report the
journal path/state but do not change it.

Recovery validates root, journal and every object first. It classifies the
ledger before file paths, then follows the durable phase—never a guessed phase:

- `Blocked { resume, .. }` dispatches through the matrix as its named
  `ResumeState`; the stored path/reason is diagnostic, not a permanent veto or
  evidence that the old observation still holds. Reclassify ledger and every
  path from scratch. If the named phase is now valid, atomically rewrite that
  phase and continue; if not, rewrite `Blocked` with the new first failing path
  and reason. An invalid resume value is corrupt journal state.

- `Prepared` promises no live mutation. If ledger, files and directories are
  all their before states, remove the validated unactivated directory. If any
  differs, preserve it as `Blocked { resume: Prepared }` and report foreign
  change; recovery never promotes `Prepared` to `Active`.
- `Active` with ledger equal to `ledger_before` classifies every file as exact
  `Before` or `After` and every planned directory as absent or a directory. If
  all classify, roll every remaining operation **forward**, then commit the
  ledger. This is safe regardless of the last recorded operation. An abort is
  itself an `Active` forward transaction; it has no special reverse recovery.
- `Active`, `LedgerCommitted` or `Complete` with ledger equal to
  `ledger_after` has crossed the commit point. Do not overwrite later user
  edits; persist/finish the Complete-journal/receipt pair, publish/clean the
  directory, validate effect guards and report eligible effects. Do not start
  an external process; execution requires a later `EffectRetryPlan` and
  `resume_effect`.
- Any effective phase with a ledger matching neither image or an unreadable
  ledger blocks. `Active + ledger_before` also blocks when any live file is
  `Unknown`/`Unreadable` or a directory has neither allowed state;
  `LedgerCommitted`/`Complete + ledger_before` always blocks. Preserve every object/preimage,
  persist `Blocked` with its prior `ResumeState` when possible, and return exit
  1 with operation/transaction IDs, path, expected states, actual hash or I/O
  kind, and a concrete repair instruction.

Recovery is idempotent. A retry revalidates even a `Blocked` journal, so a user
who restores the named path can continue without editing journal files. It
never guesses from mtime/length or overwrites an unknown path. I/O failure
during recovery stops immediately with journal intact. More than one active
transaction directory is corruption and blocks all mutation; do not order or
merge them.

After transaction recovery, scan retained receipts in generation/transaction
order and validate every finalise/abort origin dependency. The origin receipt
must be the exact immutable conflict receipt selected by the prepared
semantics, have an empty effect vector, and match the retained pending-history
identity. A missing/mismatched origin is corruption. There is no effect-state
transfer/cancellation repair because conflict receipts never own executable
effects.

`recover` is deliberately **structural**: it completes journals, publishes
receipts, validates dependencies and may atomically mark an executable
`Apply`/`Finalise` effect `Superseded` when its complete logical guard no longer
matches the current ledger. While holding the project lock it non-blockingly
acquires `effects.lock` before any receipt-state CAS. With that lock, every
validated orphaned `Running { attempt >= 2 }` becomes `Failed { attempt, code:
InterruptedTwice, summary: canonical_interrupted_summary }`; no third automatic
subprocess starts. Each transition is a
`RecoveryChange::EffectStateChanged`. If `effects.lock` is busy, recovery leaves
receipt bytes untouched and reports the current state. It never starts an
external process. It reports remaining `Deferred`, `Pending`, every `Running`,
and `Failed` state.
This ordering prevents a later command that intends to remove/change a service
from first resurrecting an older `ComposeReconcile` before that new intent can
be planned.

After the pending-conflict gate, `plan_all` scans captured receipts for the
same `InvocationFingerprint` as the current request. Exactly one matching
eligible `Apply`/`Finalise` effect in `Deferred`, `Pending`, first-attempt
`Running`, or `Failed` becomes `EffectRetryPlan`; zero matches proceeds to
ordinary planning and multiple matches refuse with sorted operation/effect
IDs. A still-`Running { attempt >= 2 }` means structural recovery could not
obtain `effects.lock`; it blocks ordinary planning with `EffectBusy` and never
becomes a subprocess plan. `Deferred`/`Pending`/first-attempt `Running` means
interrupted automatic resumption; `Failed` means an explicit same-invocation
retry. `Failed { code: Protocol }` is eligible only after the complete
descriptor/root/ledger/live guard is exact again; otherwise it reports
`CorruptMachineState` and remains pinned. A different invocation never
runs old external work. It may commit a new logical transition; after that
ledger commit, structural receipt reconciliation marks any now-mismatched old
guard `Superseded`. This is why removal can safely supersede an unstarted add
effect, while its own `ComposeReconcile` stops/removes only the exact frozen
managed service-name difference and then starts the exact desired managed set;
it never invokes `down` or `--remove-orphans`.
Every actual attempt still uses `EffectId`, `effects.lock`, the immutable
document and the compare-and-swap protocol above.

Explicit conflict abort is described in R5. It prepares a new guarded
`PreparedKind::Abort` transition from current marker/clean postimages to the
original preimages and a new-generation logical ledger state. The completed
conflict receipt remains immutable history. Ordinary recovery handles the
abort transaction by the same forward rules.

#### R4.5 Failure-injection proof

Put failpoints behind `cfg(any(test, feature = "fault-injection"))`; production
builds contain no environment-triggered abort. Integration tests spawn the CLI
as a child with one named failpoint and use `std::process::abort()` to model
loss of stack cleanup:

```text
after-lock
after-recheck
after-object-<n>-sync
after-journal-prepared
after-journal-active
before-directory-<n>
after-directory-<n>-sync
after-live-temp-<n>-sync
before-file-<n>
after-file-<n>-rename
after-file-<n>-dirsync
before-ledger
after-ledger-rename
after-ledger-dirsync
after-journal-ledger-committed
after-journal-complete
after-receipt-sync
before-receipt-move
after-receipt-move
after-transactions-parent-sync
after-receipts-parent-sync
after-effect-lock
effect-<n>-running
after-receipt-effect-state-sync
```

For each failpoint, rerun recovery twice and assert exact expected files,
ledger, journal/receipt state and no unowned temp. `FaultFs` separately injects
ordinary errors before/after every open, read, create, link, rename, chmod,
sync, journal replacement and cleanup, including unreadable/unknown live paths.
Child tests edit an untouched future op after a crash and prove recovery
blocks, then restore the before or after image and prove continuation.

Primary touchpoints: expand `src/apply/` into executor/journal/lock/recovery/
receipt modules; narrow old `create/replace/put/atomically` to adapters; wire
command entry in `src/main.rs`; add failure tests under `tests/transaction.rs`
and extend the architecture ratchet from literal writes to all mutation APIs.

R4 gate:

- lock contention and stale full-read-set input refuse before activation;
- a validated `Prepared` crash with all before-images is discarded without a
  live write and is never silently activated;
- every validated active journal with only before/after live states rolls
  forward to exact after files and matching ledger on repeated recovery;
- an unknown/unreadable path, corrupt object/journal or multiple active journal
  blocks without another project write and retains all preimages;
- ledger is never written before all file postimages are durable; a crash after
  ledger commit never overwrites subsequent user drift;
- all named crash failpoints and ordinary I/O failpoints converge or block with
  the documented classification on supported Unix filesystems;
- clean, conflicted, post-commit-failed and aborted receipts are durable and
  report truthful outcomes; an abort is a new forward transaction and receipts
  with nonterminal effects remain pinned/recoverable;
- every dark R4 commit can be loaded by a fresh process before R5 exists:
  receipt-first capture validates its local object set, ledger closure selects
  and guards that exact source, and crash recovery succeeds without assuming a
  global object copy;
- the first schema-2 commit is fault-injected for absent/schema-1 ledger plus
  every combination of app-state, global/version and intents/models sources;
  each guarded legacy delete and ledger create/replace converges, a fresh load
  sees only schema 2, and any source/listing change before activation is stale;
- create/replace/delete mode-only changes and restrictive process umasks still
  publish and recover the exact concrete prepared mode; recovery classification
  compares mode as well as kind, length and hash;
- concurrent recovery/commands never execute one effect twice concurrently;
  a crash before/after finalisation receipt publication or any receipt-state checksum
  rewrite converges without making conflict-deferred work executable; and
- crashing attempt 1 permits exactly one automatic attempt 2; crashing attempt
  2 is structurally CASed under `effects.lock` to `InterruptedTwice` with no
  third call. A busy effect lock leaves/reports `Running` and blocks ordinary
  planning. A post-call receipt checksum/CAS mismatch is never overwritten;
  only an exact expected receipt plus a changed ledger/live guard may become
  `Failed::Protocol`, which maps to `CorruptMachineState` and is retryable only
  after its complete guard is restored;
- retry fixtures distinguish every negative descriptor branch: a logical
  ledger mismatch records `Superseded`, a live-image-only mismatch returns
  `StaleInput` without rewriting the receipt, and a missing/corrupt referenced
  object returns `CorruptMachineState` without rewriting it;
- a failed direct-add compose effect is selected by the identical rerun after
  the committed surgical `jails.toml` edit, while an unrelated later human-input
  edit prevents selection; recovery alone never starts either effect;
- clean compose removal whose live preimage already lacks a service that must
  be stopped refuses before commit unless `--no-start`; conflicted preparation
  freezes only the before document and semantic intent, and finalisation derives
  and validates the stop set from the marker-free resolution. A failed pending
  subset check stays pending and requires abort followed by a new
  `--no-start` invocation; every executable stop name is present in its frozen
  before document, and no Docker call is made for an impossible descriptor;
- successful runs leave no active journal/publication temp, while retained receipts
  and their objects are bounded roots rather than debris. Empty directories
  created by a committed/aborted transition may remain by the explicit
  monotonic-directory contract and are reported.

### R5 — Durable reconciliation — SHIPPED

Gate: `jails_prepare::reconcile` decides every row of §R5.3's matrix, and the
preparation consults it for owned outputs -- a file jails did not write is
refused with the `jails adopt` fix rather than replaced.

Stop reconstructing yesterday's merge base with today's binary, templates and
context. Current `src/app/reconcile.rs` regenerates old/new intents into copied
projects and runs `git merge-file`; after a renderer/template/context change,
that “old” side is not necessarily bytes jails ever wrote.

#### R5.1 Immutable object store

Promote generated bases and render contexts into:

```text
.jails/objects/sha256/<first-two-hex>/<remaining-62-hex>
```

`ObjectId` is SHA-256 of the raw object bytes; object files contain raw bytes
with no header. The ledger supplies type/length. Reject uppercase/non-hex,
wrong shard, symlink, non-regular file, wrong length or hash. Never “repair” a
corrupt object from the current live file.

Create a shard with confined `create_dir`; write a unique same-shard temporary
with `create_new`, sync, reread/hash, then hard-link it to the final absent name
and unlink the temporary. If the final name already exists, verify exact bytes
and discard the temporary. Fsync shard and parent object directory. This is
atomic no-replace and shares R4's supported-filesystem rule. Object files are
immutable; chmod/read-only is defence in depth, not correctness.

Transaction-local objects remain the source during an active transaction. Once
R5 is installed, the R4 executor copies/hard-links every object reachable from
the prospective ledger—including output bases, template bodies and renderer
contexts—into the durable store and fsyncs them **before** the ledger that
references them. A new R5 ledger may never point only into an active/receipt
directory. Receipts created by the earlier dark R4 gate remain readable through
R2's explicitly guarded receipt-local fallback until their objects are promoted
by R5's retention/GC prepass; this is a delivery bridge, not a second long-term
object authority. Every R5 GC cycle first promotes and fsync-verifies the full
object closure of **all retained receipts**, including receipt-only preimages
and audit objects. It may prune a retained receipt's local copy only after the
matching global object is verified. If any promotion fails, GC reports its
post-commit warning and performs no local or global deletion; a later cycle
retries the whole prepass.

#### R5.2 Provenance schema

Every newly written managed output has a non-optional base and renderer:

```rust
struct RendererStamp {
    renderer: RendererId,
    renderer_schema: u32,
    jails_version: String,
    template: Option<TemplateStamp>,
    context_schema: u32,
    context_object: ObjectRef,
    relevant_inputs: ObjectId,
    tools: Vec<ToolFingerprint>,
}

enum RendererId {
    Recipe(Recipe),
    Capability(Capability),
    Format(FormatOwner),
    OneShot(OneShotKind),
    ToolFeature(ToolFeature),
}

enum FormatOwner {
    Pom, Compose, Properties, HumanConfig, MarkedSource,
    CommandRegistration, WholeFile,
}

enum OneShotKind { Field, Migration, Cases }

enum TemplateOrigin {
    BuiltIn { name: TemplateId },
    ProjectOverride { path: ProjectPath },
    UserOverride { logical_name: TemplateId },
}

struct TemplateStamp {
    origin: TemplateOrigin,
    source_object: ObjectRef,
}

struct RendererContextV1 {
    schema: u32,                         // exactly 1
    renderer: RendererId,
    subject: Option<RenderedSubjectContext>,
    references: Vec<ResolvedReferenceContext>,
    base_package: Package,
    layers: Vec<LayerContext>,
    build: Build,
    flavor: Option<Flavor>,
    java_release: u32,
    capabilities: Vec<CanonicalCapability>,
    bindings: TemplateBindings,
}

enum RenderedSubjectContext {
    Entity { id: EntityId, spec: EntitySpec },
    OneShot { id: OneShotId, spec: OneShotSpec },
}

struct ResolvedReferenceContext {
    role: ReferenceRole,
    target: JavaType,
    managed: Option<EntityId>,
}

enum ReferenceRole { On, Yields }

struct LayerContext {
    layer: Layer,
    package: Package,
}
```

This completes the single R1 `OutputRecord` definition. Every managed output
has an exact `base` and `renderer`; ambiguous compatibility paths remain
inside `LegacyEntry` and never become weaker output rows. `current` records the
committed live/resolved SHA-256, length and mode. `base` names the exact desired
generated bytes and their mode. Length and mode are not optional validation
details: commit, reconciliation and conflict finalisation compare the complete
image.

`template` is `None` for a pure format owner and `Some` only when template bytes
actually contributed. Never store an absolute home/template path.
`source_object` proves bytes even when a user override later disappears.
`RendererStamp.tools` contains only full fingerprints of tools that produced
the desired base `N`; Git merge is transaction preparation, not a renderer, and
appears only in the preparation fingerprint. `relevant_inputs` is the canonical
hash of only snapshot inputs declared by that renderer; it explains a changed
render without making all project files part of provenance.

Specifically, collect the exact sorted `InputPrecondition` rows consumed by the
renderer (including machine objects and directory listings, excluding unrelated
bootstrap rows), require each to exist in the snapshot `ReadSet`, and compute
`relevant_inputs = SHA256("JAILS-RELEVANT-INPUT-1" || encode(rows))`. An empty
set hashes the canonical zero-count vector, not an empty byte string. The
renderer records which input IDs it consumed through `SnapshotView`; a caller
cannot hand it an unverified hash.

`context_object` contains exactly
`encode(RendererContextV1)`—there is no JSON/TOML peer and no renderer-supplied
opaque map. Its `ObjectRef` length/hash must match the raw codec bytes;
`RendererStamp.context_schema` and `RendererContextV1.schema` must both equal
1, and the two renderer IDs must match. `subject` is closed: `Recipe` requires
an `Entity::Intent` whose recipe and ID/spec discriminants match; `Capability`
requires the matching `Entity::Capability`; `ToolFeature` requires the matching
`Entity::ToolFeature`; `OneShot(kind)` requires `OneShot` with the same
field/migration/cases discriminant and repeated target/path/source identities;
and an aggregate `Format` context requires `None`. No renderer may omit a
required subject or smuggle one-shot identity/spec through template bindings.
References sort by `(role, target, managed)` and contain only the
resolved qualified target plus optional managed identity, never source paths.
Capabilities sort uniquely by `CapabilityId`. `layers` contains exactly the
eleven roles in this order with no duplicate or omission: domain, app, service,
web, api, messaging, cli, clients, jobs, adapters, testkit.

Template metadata defines the exact binding-key set. When `template` is
`Some`, `bindings` must have exactly `required_keys`; missing and extra keys
both reject. When `template` is `None`, bindings must be empty. The context
codec preserves semantic order inside fields, index columns, variants and
ordered template values while sorting set/map values. It rejects a subject/spec
mismatch, a renderer/subject mismatch, a reference inconsistent with that spec, a capability ID/spec
mismatch, an invalid Java release, unknown build/flavour/layer/tag, duplicate
row, excessive value, and trailing bytes. `TemplateValue` tags
text/name/package/java-type/boolean/ordered are `0/1/2/3/4/5`;
`ReferenceRole` on/yields is `0/1`; `Layer` uses the eleven order values above;
`RenderedSubjectContext` entity/one-shot is `0/1`;
`Build` maven/foreign/bare is `0/1/2`; `ForeignBuild` Gradle/Ant/Bazel is
`0/1/2`; and `Flavor` spring-boot/plain-maven is `0/1` beneath its ordinary
`Option` tag.

Golden vectors cover a pure format context, every entity renderer class, each
field/migration/cases one-shot subject, every `TemplateValue`, reordered-map
canonical equality, and each rejection.
Unknown `context_schema` prevents re-render-based explanation but never
prevents reading the exact stored base. Renderer stamps explain the new
candidate; reconciliation never regenerates the old base.

#### R5.3 Three-way cases and base advancement

For every persistent output, preparation computes `old base B` from
`base.object`, live bytes/absence `L`, and newly rendered desired base/absence
`N`:

| Case | Prepared result |
|---|---|
| no prior output, `L` absent, `N` present | create `N`; base/current become the exact `N` images |
| no prior output, whole-file output, `L` present, `N` present | collision refusal unless the exact-match adoption or explicit R1 `adopt --replace` path was chosen |
| no prior V2 human-config resource/output, valid captured `jails.toml` already contains the exact `DirectConfig` declaration/resource, and the pure config editor would make no byte change | perform the R1.3 ledger-only authoritative bootstrap: emit no `FileOp`; record the exact `L` image as base/current with `FormatOwner::HumanConfig`; this exception is unavailable after any V2 human-config resource has existed |
| first V2 transition, complete resource/output closure belongs to a captured `DirectConfig` entity or the explicit current `test --fast` request, every semantic value is exact in `L`, and the fresh complete format-owner edit is byte/mode unchanged | atomically perform the R1.3 authoritative resource bootstrap for the whole closure: emit no `FileOp`; create only the real owner/resource rows and exact `L` base/current outputs with fresh renderer stamps; any partial, app/legacy-only, unequal, duplicate or post-V2 case refuses |
| no prior output, recognised shared-format file, `L` present, `N` present, and every newly desired `ResourceKey` is absent from parsed `L` | format owner performs one guarded surgical `L → N` replace, preserving all unmanaged syntax/contributions; base/current become exact `N` with the real renderer stamp |
| no prior output, recognised shared-format file, `L` contains a newly desired semantic key | collision refusal even when its value happens to match; ownership requires explicit adoption rather than silently claiming user configuration |
| no prior output, `L` absent, `N` absent | no output and no ledger row |
| no prior output, `L` present, `N` absent | preserve the unmanaged file and create no output row |
| prior output, `L == B == N` | no file op and no base/current churn |
| prior output, `L == B`, `N` present and `N != B` | guarded replace with `N`; base/current become the exact `N` images |
| prior output, `L == N`, `N != B` | file already has the desired bytes; no file op, but a committing ledger transition advances base/current to `N` |
| prior output, `L != B`, `N == B` | preserve user bytes with no file op; base remains `B`, current image advances to `L` only on a committing owner/state change |
| prior output, all present and `L != B != N` | three-way merge `B + L + N`; clean bytes become replace, conflict markers become conflicted replace |
| prior output missing, `N` present | refuse deleted-owned-output; only explicit replace may recreate |
| prior output, `L` absent, `N` absent | remove the output/resource claim without a file op |
| prior output, `N` absent and `L == B` | guarded delete, whether absence comes from last-owner removal or a renderer version dropping one output; retain forward-only SQL resources |
| prior output, `N` absent and `L != B` | refuse unless explicit force removal records discarded hash/preimage; force deletes only this managed output |
| semantic resource owner removed but shared file remains | render remaining resource map, then reconcile the complete shared file by the same table |

`AdoptLayout` is deliberately orthogonal to this ownership table. When its
surgical edit touches a `jails.toml` that already has a managed output row, it
preserves that row's contributors, generated base and renderer and advances
only `current` to the exact committed postimage. When no managed config output
row exists, it leaves the file unowned and creates none. Layout adoption never
claims an existing capability or manufactures a `HumanConfigCapability`
resource.

In this table, image equality includes bytes, length and mode. When all three
images are present and their bytes require a three-way merge, reconcile mode
independently with this closed rule: if `L.mode == B.mode`, choose `N.mode`; if
`N.mode == B.mode`, choose `L.mode`; if `L.mode == N.mode`, choose that mode;
otherwise both user and generator changed mode differently and preparation
refuses. A content conflict may commit marker bytes only when mode has one
unambiguous result. There is deliberately no marker syntax or later
finalisation guess for a permission conflict. Every chosen result enters the
`FileOp`, `LiveFileImage`, report and receipt.

“Prior output” means a canonical global `OutputRecord`, and `N absent` is
handled per output—not only when the whole entity disappears. When a shared
format file has no remaining managed contributions, its format owner preserves
unmanaged bytes and returns either complete remaining bytes or semantic
absence; it does not equate “zero managed resources” with deleting a human
file.

The shared-format bootstrap row is required for ordinary existing projects:
the first managed Maven dependency/plugin, compose service or properties key
must not require whole-file adoption. It applies only to a successfully parsed
file owned by the corresponding closed `FormatOwner`; the semantic editor must
prove that every desired key is absent, preserve every other token/comment/
entry according to that format's contract, and produce one exact postimage.
Parse ambiguity, duplicate keys, same-key user configuration or an unsupported
format refuses before mutation. Golden fixtures cover a normal existing POM,
compose file and properties file, preservation of unrelated user content, and
same-key collision refusal.

Format 1 has no delete/modify conflict-marker protocol. Therefore `N absent`
with edited `L != B` follows only the refusal/explicit-force row above and can
never produce `PreparedKind::Conflict`. Every conflict path has present `B`,
`L`, `N`, a nonoptional prior/desired base and a desired-present candidate.
This deliberately removes an otherwise unimplementable desired-absent wire
branch; adding interactive delete/modify merging would require a new protocol
version and explicit marker grammar.

For divergent text, invoke captured Git through R3's bounded scratch executor
with exactly:

```text
git merge-file -p --no-diff3 --marker-size=7 --diff-algorithm=histogram \
  -L current -L base -L jails-desired-<first-12-operation-hex> \
  ../merge-inputs/<path-key>/current \
  ../merge-inputs/<path-key>/base \
  ../merge-inputs/<path-key>/desired
```

`path-key` is lowercase hex of
`SHA256("JAILS-MERGE-PATH-1" || encode(ProjectPath))`. The three deterministic
relative arguments are scratch files containing exact `L`, `B`, and `N` under
the temp directory but outside the projected `project` child; no random or
absolute scratch path enters argv, a fingerprint, or a label. The desired label
is represented by the typed `OperationLabel` until expansion. Exit 0 is clean.
An ordinary exit status in `1..=127` is conflict output: Git reports the
conflict count and truncates it to 127. A status at least 128, termination by
signal, spawn/timeout/protocol failure or missing status is a preparation
refusal. Before accepting conflict output, parse one or more non-nested hunks
with exact line tokens `<<<<<<< current`, `=======`, and
`>>>>>>> jails-desired-<prefix>` in that order. Reject zero hunks, nesting,
duplicate/missing separators, unbalanced order or a label mismatch. Persist the
three exact token lines and exact `hunk_count`; require
`exit_status == min(hunk_count, 127)` before accepting the bytes. Finalisation rejects any exact
stored token line, any generic seven-character Git marker line left in that
formerly conflicted path, or malformed/unbalanced remnants; it does not merely
search for one substring. Missing/unfingerprintable Git refuses only a merge
case; unchanged/new output does not acquire a gratuitous dependency.

The merge path accepts UTF-8 text without NUL only. Binary/non-UTF-8 data still
uses all equality/create/delete rows above, but divergent `B/L/N` refuses with
`binary three-way merge unsupported`; only an explicit guarded replace/force
policy may discard it. Git executable/version/arguments are part of
`ToolFingerprint`.

After a clean merge, `current` is the actual merged live image, while `base`
advances to exact `N`, **not** merged bytes. User edits therefore
remain the delta from the newest generator output on the next update. After a
no-op `N == B`, base does not churn. New template/context/renderer stamps commit
with the new base only.

A schema-1/legacy row has no guessed base. The baseline schema-1 formats do not
store exact historical bytes or a usable historical digest, so they never
auto-prove a base. Only a separately recognised older format fixture that
actually contains exact bytes/hash may be translated under an explicit codec
rule. Otherwise the row remains non-reconcilable until the exact R1
`jails adopt ...` (only an exact current-render match gains truthful
provenance) or `--replace --force` (a guarded new render receipts/discards the
old bytes) operation. Plan/pretend never makes
that choice or deletes legacy state.

#### R5.4 Committed conflict protocol

A merge conflict is a valid `PreparedChange` with marker-file postimages. The
same R4 journal commits all marker and clean postimages atomically relative to
the ledger commit point. The ledger retains all five successful top-level
tables unchanged and adds at most one project-wide frozen candidate:

```rust
struct PendingConflict {
    operation: OperationId,
    generation: u64,
    invocation: InvocationFingerprint,
    resume_display: String,
    desired_inputs: Vec<FrozenDesiredInput>,
    candidate: PendingLedgerState,
    paths: Vec<PendingConflictPath>,
    frozen_nonconflict_postimages: Vec<FrozenPath>,
    effect_intents: Vec<DeferredEffectIntent>,
}

struct PendingLedgerState {
    applied: Vec<AppliedEntity>,
    one_shots: Vec<OneShotReceipt>,
    resources: Vec<ResourceRecord>,
    outputs: Vec<PendingOutput>,
    legacy: Vec<LegacyEntry>,
}

struct PendingOutput {
    path: ProjectPath,
    contributors: BTreeSet<ResourceOwner>,
    current: PendingCurrent,
    base: StoredFileImage,
    renderer: RendererStamp,
}

enum PendingCurrent {
    Exact(LiveFileImage),               // frozen clean/unaffected postimage
    ResolveFromLive,                    // learned only at finalisation
}

struct PendingConflictPath {
    path: ProjectPath,
    prior_base: StoredFileImage,
    desired_base: StoredFileImage,
    marker_image: StoredFileImage,
    markers: MarkerTokens,
    hunk_count: u32,
}

struct MarkerTokens { open: String, separator: String, close: String }

struct FrozenPath { path: ProjectPath, postimage: FileImage }

struct FrozenDesiredInput {
    id: DesiredInputId,
    guard: DesiredInputGuard,
}

enum DesiredInputGuard {
    Exact { sha256: ObjectId, len: u64 },
    ProjectedTransactionOutput {
        path: ProjectPath,
        sha256: ObjectId,
        len: u64,
    },
    Absent,
}

enum DesiredInputId {
    HumanConfig,
    AppManifest(ManifestSourceId),
    DirectRequest,
    CasesBrief(SourceInputId),
}
```

For finalisation identity, construct this exact projection of the stored
pending record:

```rust
struct PendingIdentityV1 {
    origin_operation: OperationId,
    origin_generation: u64,
    invocation: InvocationFingerprint,
    desired_inputs: Vec<FrozenDesiredInput>,
    candidate: PendingLedgerState,
    paths: Vec<PendingConflictPath>,
    frozen_nonconflict_postimages: Vec<FrozenPath>,
    effect_intents: Vec<DeferredEffectIntent>,
}
```

`PendingIdentity` equals
`SHA256("JAILS-PENDING-1" || encode(PendingIdentityV1))` after all canonical
sort/uniqueness checks. `resume_display` is deliberately excluded because it is
presentation, but every semantic/object/effect field is included. Finalisation
recomputes this hash from the current ledger and requires it to equal the value
used in `OperationIdentityV1`; it never trusts a separately stored hash.

Schema-2 payload tags implemented by R1 and semantically activated by R5 are fixed: `RendererId`
recipe/capability/format/one-shot/tool-feature = `0/1/2/3/4`;
`FormatOwner` pom/compose/properties/human-config/marked-source/command-
registration/whole-file = `0/1/2/3/4/5/6`; `OneShotKind`
field/migration/cases = `0/1/2`; `TemplateOrigin` built-in/project/user =
`0/1/2`; `PendingCurrent` exact/resolve-from-live = `0/1`;
and `DesiredInputId` human-config/app-
manifest/direct-request/cases-brief = `0/1/2/3`. `ManifestSourceId` and
`SourceInputId` both use project/external = `0/1`;
`DesiredInputGuard` exact/projected-transaction-output/absent = `0/1/2`. Marker strings encode as
ordinary bounded strings but must equal the grammar derived from the stored
operation ID; arbitrary marker text is invalid even when its hash is sound.

`PendingLedgerState` is the complete frozen logical schema-2 state that a
successful resolution will promote, including entity and one-shot changes and
one canonical global resource/output table. It avoids duplicating shared
resource records under candidates. `PendingCurrent::ResolveFromLive` appears
only for a desired-present conflicted output; marker bytes are never represented
as successful generated current/base bytes. Clean candidate outputs carry
their exact postimage. No candidate is promoted early. Post-commit effects are
represented only by semantic intents; no executable descriptor or effect
receipt exists yet. Commit returns a receipt with `outcome: Conflicted`, prints
resolution paths and exits 2—never an ordinary error after unrecorded mutation.

`CanonicalMutationRequest` stores parsed typed arguments after defaults,
aliases and package resolution: set-valued capabilities sort, while field,
index-column and variant order is preserved. It excludes presentation flags
(`--debug`, output format) and the resume-only `--abort-conflict`.

Build one `FrozenDesiredInput` for each participating human source or value,
including relevant absence. A present source that this transition does not
mutate uses `Exact` over its captured bytes; a relevant absent source uses
`Absent`. A
project human input that this same request surgically creates or edits uses
`ProjectedTransactionOutput`: run the same pure/idempotent semantic editor used
by planning, hash its exact projected postimage, and name the target
`ProjectPath`. The eventual planned output for that input must have exactly that
projected image before preparation may return; there is no second projection
implementation. `DirectRequest` is mandatory and uses `Exact` over the shared-
codec bytes of `CanonicalMutationRequest`. Rows sort uniquely by
`DesiredInputId`, and
`desired_input_sha256 = SHA256("JAILS-DESIRED-INPUT-1" || encode(rows))`.
Only irrelevant sources have no row. The ordinary read set independently
contains the corresponding present/absent precondition. Ledger observation, presentation and
absolute paths never enter a row. A project manifest stores its `ProjectPath`;
an external manifest stores only the shared domain-separated `ExternalPathId`,
not the canonical absolute path itself.

This projected-image rule is required for progress. On the first direct
`add/remove` invocation the read set still guards the old `jails.toml` preimage,
but the invocation fingerprint names the exact surgical postimage. Rerunning
the identical idempotent edit against that committed postimage produces the
same fingerprint, so conflict continue/abort and a failed-effect retry remain
reachable. An unrelated later user edit changes the projection and therefore
does not match. In pending mode a marker/resolution cannot be fed back through
the ordinary editor: the ledger-first loader uses the stored projected row only
after the path is proven to be guarded by that pending transaction, as specified
in R2.2. Clean transaction outputs are independently checked against their
frozen postimages; conflict outputs use marker/resolution and frozen semantic-
slot validation. Read-only inputs always rehash current bytes or re-prove their
recorded absence, so a file created during a pending conflict makes the frozen
candidate stale.

On an ordinary rerun, the current invocation reconstructs all four fields. On a
pending rerun it reconstructs only request syntax, source identity and
admissible current input guards, then reuses the frozen canonical request after
those checks; it never resolves project-derived defaults from marker-bearing
files. Structural equality of all four `InvocationFingerprint` fields defines
“same command.” `resume_display` is
deliberately outside the fingerprint; it is a shell-neutral, secret-free
diagnostic rendering and can change presentation without changing identity.

While pending state exists, all ordinary mutation paths stop before planning.
The invocation matching `InvocationFingerprint` may do exactly one of:

1. **Continue by rerunning the same command with no abort flag.** It first
   requires the original manifest/direct-request input hash; changed input
   refuses and tells the user to finish/abort the frozen conflict first. It
   does not re-render or re-merge. Every frozen nonconflict path must equal its
   recorded `FileImage`. Every conflict path must exist, differ from the marker
   image and pass the complete stored marker grammar check. For a shared-format
   output (POM, compose, properties, human config, registration or marked
   source), parse the resolved bytes with its frozen `FormatOwner` and require
   every candidate-owned `ResourceKey` assigned to that path to occur exactly
   once without semantic-key collision. The user may change a managed value as
   a live delta from desired base `N`, but may not delete/duplicate the owned
   semantic slot while the candidate ledger still claims it. Whole generated
   files need only the marker/presence checks and may contain arbitrary user
   resolution bytes. Report **all** unresolved/mismatched paths and write
   nothing.
2. When all paths pass, prepare a `PreparedKind::Finalise { origin }`
   ledger-only transaction. Capture each `ResolveFromLive` output as an exact
   `LiveFileImage`, promote the frozen `PendingLedgerState`, clear
   `PendingConflict`, increment generation and materialise each frozen
   `DeferredEffectIntent` into a new executable descriptor. For compose, intern
   the exact marker-free resolved compose postimage as `after_document`; do not
   copy renderer base `N` or an origin descriptor. The
   origin conflict receipt has an empty effect vector and remains immutable.
   The operation identity includes
   the selected origin transaction; the full read set includes its
   `MachineReceipt` checksum guard. Recheck both at commit.
   Do not also apply a newly changed manifest; a later invocation starts a new
   snapshot.
3. **Abort with `--abort-conflict` on that same command.** This is a flag, not a
   new subcommand. Require every file changed by the conflicted transaction to
   equal its exact marker/clean postimage; any partial resolution refuses so
   work is not discarded. Select exactly one structurally matching origin
   receipt by the R4.2 matrix—not merely by operation/generation—and require
   its effect vector to be empty and its pending intents to equal the current
   ledger. Pin its transaction/checksum, then
   prepare a new
   `PreparedKind::Abort { origin }` whose guarded forward operations restore
   its file preimages. Its ledger after-state is the current successful logical
   rows with pending state cleared and generation incremented—not an in-place
   rewrite of the old receipt or a generation rollback. It clears the pending
   intents, runs no effect and exits 0. For a newly created conflict file, restore means
   absence; for a deleted file, restore its preimage. Transaction-created empty
   directories may remain under R3's explicit directory policy.

Other command/invocation fingerprints refuse with operation ID and the exact
same-command continue/abort syntax. An interrupted conflict commit follows
ordinary journal roll-forward into marker bytes plus pending state; recovery
never silently aborts or finalises it.

#### R5.5 Object garbage collection

Run mark-and-sweep under the mutation lock only after a successful clean,
finalised or aborted ledger/receipt commit. First run the all-retained-receipt
promotion prepass from R5.1; only a complete successful prepass may enter the
deletion phase. Roots are:

- every applied output base, renderer template source and context object;
- every pending ledger output/base/context, conflict prior/desired base and
  marker image;
- every valid active/blocked journal and its before/after/preimage objects;
- all objects reachable from receipts retained by the exact R4 root/dependency
  algorithm; and
- legacy objects explicitly referenced by a lossless migration/adoption.

Traverse object references in renderer/context/receipt records, mark by
`ObjectId`, then enumerate valid shard files in byte order. Verify each object
before deleting any. A corrupt object, symlink, unknown filename or unreadable
shard aborts GC without deleting another object. Delete only unmarked verified
objects and fsync affected shards; remove an empty shard only when explicitly
modelled by GC (never as file-op parent cleanup). GC failure is a post-commit
maintenance warning with receipt, not a claim that the project commit failed.

Primary touchpoints: new `src/objects.rs`; `src/ledger.rs` object/provenance and
pending codecs; replace `src/app/reconcile.rs` regeneration with
`src/planning/reconcile.rs`; `src/template.rs`; R3 prepare/report; R4 journal,
receipt/recovery; CLI pending-conflict dispatch in `src/main.rs`.

R5 gate:

- user edits survive jails binary, built-in template, project override and user
  override changes because old base bytes are exact;
- first-V2 fixtures cover an existing valid `jails.toml` whose exact
  `DirectConfig` rows bootstrap ledger ownership without a `FileOp`, plus
  unequal/duplicate/already-managed cases that refuse or use ordinary
  reconciliation; app-only apply never copies those rows, and removing direct
  ownership while an app owner remains removes only the config contribution;
- V1 direct db/docker and explicit fast-test projects whose complete current
  shared-format closure exactly matches fresh output gain truthful V2
  resource/output provenance with no write; changed values, partial matches,
  and external/app-manifest-only declarations stay collisions;
- layout-adoption fixtures cover both provenance branches: an existing managed
  config output preserves contributors/base/renderer and advances only
  `current`, while an unmanaged config remains unowned and gains no output row;
- all tabled create/update/delete/drift cases have byte-level tests, including
  base advancement after a clean merge and no advancement to merged user bytes;
- missing/corrupt object, unknown schema/renderer context, legacy ambiguity,
  absent owned output and drifted removal fail closed with no mutation;
- conflict apply commits exact markers plus pending candidate state, retains
  all five successful top-level tables unchanged, defers effects and returns
  exit 2;
- unchanged/partially resolved/changed-input reruns leave every managed project
  leaf, human declaration, ledger, transaction and receipt unchanged (apart
  from the executor-owned coordination shell/lock diagnostics) and name all
  blockers; complete resolution finalises frozen candidates without
  rerender; guarded abort is a new recoverable transaction, restores exact file
  preimages and preserves the successful logical tables at a new generation,
  leaves only
  explicitly permitted empty directories, and refuses after any affected file
  edit;
- direct `add` and `remove` conflicts whose transaction also edits
  `jails.toml` can both continue and abort by identical rerun; projected input
  hashes remain stable across the command's own clean edit, marker-bearing
  loader-critical paths bypass ordinary parsing, unrelated human edits refuse,
  and marker-free shared resolutions pass the frozen semantic-slot validator;
- crash failpoints through conflict commit/finalise/abort recover idempotently;
- object atomic-create/hash/confinement/round-trip tests pass on supported
  platforms, and GC retains every ledger/pending/journal/receipt root while
  removing only verified unreachable objects; a retained dark-R4 receipt whose
  only unique object is a non-ledger preimage is promoted globally before its
  local copy may be pruned.

### R6 — Migrate every mutation path and prove the product — IN FLIGHT

Switch every in-project mutation to R1→R5, remove the old inverse/sequential
paths, and prove the new route on real applications. A command moves only when
all mutations it can trigger are represented; no command may apply half through
the executor and half through a legacy helper.

#### R6.1 Migration order

Step status, against the ordered list below. A step is done only when its work
is committed and its gate ran; "the route exists" is not the same claim as "the
command uses it", and dispatch is still V1 for every command.

| Step | State | Evidence |
|---|---|---|
| 1. Land the executor dark | done | `jails-engine` is a library crate precisely so nothing in `main.rs` calls it; the workspace `dead_code` denial makes dark code in the binary impossible rather than merely discouraged. |
| 2. Capability `add`/`remove`/`sync` on V2 | done | `route::{install,remove,sync}`; `tests/desired.rs` compares 21 capabilities against V1 on dependencies, effective property values and file bytes; `tests/engine.rs` sweeps 21 failpoints through a real install. `sync` is one transition, not a loop. |
| 3. Persistent `generate`, then the one-shots | done | `generate::plan_recipe` separates planning from writing and `route::generate` commits it; 22 scenarios match V1 byte-for-byte. `route::destroy` retires an entity from the recorded exact state, and `every_persistent_kind_destroys_back_to_where_it_started` round-trips 22 of the 25 scenarios to a byte-identical project. `route::migration` allocates from a declared directory listing §R4.3 step 2 now genuinely rechecks; `route::cases` records a source-hash receipt and reconciles a same-source re-run; `route::field` re-desires the target at its new spec and lets §R5.3 decide each derivative, with the migration owned by the field so removing the target cannot delete it. A derivative both sides changed is three-way merged; only a genuine overlap refuses, and it names §R5.4's pending protocol as the half that is missing. |
| 4. `app init/plan/apply/reconcile` as one aggregate | done | All four routed. `route::app_apply` declares the whole manifest under `ReconcileScope::AppManifest` and commits once, so a row the manifest stops naming is relinquished rather than merely unmentioned. Each step plans against `Project::projected` -- a projection of everything before it -- which is what replaces the per-step reload: `g search` sees `add db`'s POM and `g scaffold`'s record without either having been written; the falsifier is `examples/web-crawler`'s eleven capabilities and eleven intents as one transition. `--pretend` is that same computation stopped one step before the lock, and there is no second function for it at all: every route takes a `Run`, which says whether it may write. So `--pretend` names exactly the files the apply then writes, there is no second walk to disagree with, and the shadow comparison retires. `route::app_init` plans `PlannedSubject::AppInit` with an empty `LedgerIntent` -- nothing enters the store, because seeding hands the file to the reader -- and its target is a declared read, so "already exists" is a precondition rechecked under the lock instead of a `Path::exists` with a write after it. **`reconcile` needed no route**: §R5.3's decision table already three-way merges any rewrite with a recorded base, so an edited file and a changed manifest row merge for free, and a genuine overlap refuses without writing -- both pinned by tests. |
| 5. Maintenance mutations | mostly | Four of five routed as `PlannedSubject` maintenance subjects with empty `LedgerIntent`s. `rename` is the one with a real defect behind it: V1 rewrites contents then moves files and its own comment admits the half-applied state, while here a move is `Create`+`Delete` in one operation list and every source, destination and directory is a declared read. It also carries the **identity transition** §R6.4's row asks for -- an entity is named by its `IntentId`, so a renamed type's rows arrive at the new paths under the renamed id and the old id is removed in the same intent, which is what lets `destroy record Bonus` find what `rename Reward Bonus` moved. `adopt_layout` emits one `SemanticEdit::HumanConfigLayout` per adopted layer and lands one operation instead of one per layer -- keyed rather than a whole-file body because `jails.toml` has a second contributor (`[project] capabilities`, which `add` owns), so the two compose instead of one deciding every byte. `format` runs Spotless in a scratch tree synthesised from the projection and commits only what it changed inside its declared `mutable_scopes` -- so a formatter reaching outside `src/` is a refusal rather than a fait accompli, `target/` is skipped as derived output, and a run that changes nothing has no operations. `test --fast` is `ToolFeature::FastTest` under `DirectCli`, with `remove fast-test` as the same scope declaring nothing. **`adopt --legacy-key` is not routed**: §R2.5 makes it the only path from an unknowable legacy manifest origin to a named owner, and it needs `doctor` to enumerate ambiguous `LegacyKey`s first, which nothing does. |
| 6. New-project bootstrap through publish | done | §R6.5; `new`/`new-cli` build in a scratch sibling under `<parent>/.jails-new.lock` and become real in one rename, `--app` included. |
| 7. Read-only `StateCompatibility` facade | done | `compat::read` classifies absent/current/legacy/unreadable without mutating, `ledger::parse` refuses a newer schema with a no-downgrade message, and `compat::translate` now turns a schema-1 ledger into a schema-2 draft in memory — so `Store::observe` reads a project V1 built instead of refusing it. §R2.5's conservatism is the design: every schema-1 row becomes a `LegacyEntry` and **none** becomes an `AppliedEntity`, because the old format did not record who asked for a row and one whose fields match today's manifest is still of unknown origin. The draft is generation 0 with empty applied/one-shot/resource/output tables, so the first V2 mutation writes generation 1 with the schema-1 bytes as its guarded before-image. `jails.toml` is not translated: its capabilities become `DirectConfig` claims during ordinary resolution and migration never touches the file. What is still missing is the explicit deletion of the *other* legacy sources (`app-state-v1`, `files`, `version`) through a `LegacyMigrationIdentity`; V1's own fold already removes them, so only a project that skipped that path still carries them. |
| 8. Classify every remaining mutating path | done | Filesystem: the write-layer ratchet is at zero and counts deletes, copies, renames, links, directory creation and permissions. Subprocesses: §R6.6's table is enforced by a test that fails on stale rows too. |
| 9. Flip the single dispatch point | ready | The translation that blocked it exists (step 7). What settled *how* it must happen still holds: **all at once**, because one V2 command in a V1 project shares a ledger path with thirteen V1 ones and each writes a schema the other cannot read. Formerly: **blocked on the V1→V2 ledger translation, which did not exist.** `.jails/ledger.toml` is one path with two incompatible schemas: `ledger::load` parses V1 and `LedgerV2::parse_file` parses V2, and neither reads the other. So a V2 route refuses any project that has ever run a V1 command — which is every real project — with `ledger has N line(s); schema 2 is exactly five`. This was found by flipping `adopt` and `rename` and running them against a project built by V1; both were reverted. It also settles *how* the flip must happen: **all at once**, since one V2 command in a V1 project is the same collision. Everything else is ready — every existing V1 mutation path is routed and `--pretend` is a `Run` mode honoured in one place. What remains at the boundary is mechanical: `--name`/`--package` for capability instances, `no_start`/`debug` post-commit behaviour, and reporting what was written. |
| 10. Delete V1, then prove the product | not started | §R6.8 owns the proof-app sweep and hosted CI; both remain unclaimed. |

One gap is open, and it is named where it bites rather than left to be
discovered:

- **§R5.4's *committed conflict* half is not wired to these routes.** The merge
  itself is: §R5.3's fifth answer runs `git merge-file` through R3's bounded
  executor with §R5.4's exact arguments, and a clean merge commits the merged
  bytes while the recorded base advances to the generator's output rather than
  to the merge -- which is what keeps a reader's edit a delta from the newest
  render. That is the case that matters in practice, and without it `g field`
  refused on any project where anybody had ever touched a derivative.

  What is missing is the other outcome. Genuinely overlapping edits produce
  marker bytes, and §R5.4 commits those with a frozen `PendingConflict` that
  the next invocation of the same command continues or aborts. The bytes are
  produced and validated against the stored grammar; what does not exist is the
  pending state, the ledger-first refusal while it stands, and the
  continue/abort routes. It refuses instead, naming the hunk count and the
  section.

The two schema gaps this section used to name are closed:

  The prerequisite that was missing is **now built**. A `PendingConflict`'s
  identity includes an `InvocationFingerprint` carrying the
  `CanonicalMutationRequest` that stalled, which is what a resume proves
  sameness by; `OperationIdentityV1.invocation` used to be `None` on every
  path. Every route now supplies an `Asked` — the canonical request *and* the
  canonical syntax, built by the route rather than parsed back out of `argv`,
  because a route knows what it was asked far more exactly than a re-parse
  does and there is no second implementation to disagree. The fingerprint's
  `desired_input_sha256` covers the mandatory `DirectRequest` row plus the
  `HumanConfig` row (present, or `Absent` as a fact rather than a gap),
  computed against the same capture the plan was so it describes the bytes
  the plan actually read.

  What is left is the candidate itself, and it **lands as one piece or not at
  all**: a project that can enter a pending conflict and not leave it is worse
  than one that refuses the merge. Building the enter side alone was tried and
  backed out for exactly that reason.

  The *enter* side is close. `diff` already collects every conflicting path
  with its marker bytes, both bases, the tokens and the contributors; the
  candidate is `PendingLedgerState` built from the store the apply would have
  written, with a conflicted row's `current` as `ResolveFromLive` (not a
  placeholder image — what that file ends up holding is not knowable until
  somebody resolves it) and its `base` as the *desired* image, so a reader's
  fix stays a delta from the newest render. `ledger_after` is then the
  observed store unchanged plus a `PendingMarker`, which is what stops markers
  ever being recorded as an entity's output.

  Two things the attempt found that the schema does not yet have:

  - **A `PendingMarker` cannot address its own record.** It carries the
    operation, generation, request syntax and display string — enough to
    bootstrap and to check a rerun is the same command, and nothing that says
    where the complete `PendingConflict` bytes are. `PendingIdentity` is a
    domain-separated hash of the identity bytes, not the object's sha256, so
    it cannot be used as an object key either. The marker needs the record's
    `ObjectId`.
  - **`abort` cannot reach the file results it must invert.** §R6's Abort row
    says the operations are "the path-complete guarded inverse of the origin
    receipt's file results", but `ReceiptV1` carries a `PreparedIdentityV1`
    and a `complete_journal_checksum` — it *pins* the journal rather than
    holding the results. So abort needs journal read-back for a completed
    transaction, which `recover` has for crash paths and nothing exposes for
    this one. The conflicted paths cannot be recovered from the pending record
    instead: `PendingConflictPath` holds both bases and the marker image, but
    not the reader's pre-conflict bytes, which is what a restore returns to.

  Two things did land in the meantime. `Merged::Conflicted` keeps the marker
  bytes and the tokens git was told to write, rather than discarding them —
  a conflict that threw away its own resolution would leave the reader nothing
  to resolve, and this is the value §R5.4 freezes. And the refusal now names
  **every** conflicting path with its hunk count in one message, instead of
  erroring on the first: a merge conflict is one path's problem, and refusing
  the whole transition on the first one made a reader fix a file, run again,
  and be told about the next.

- ~~`LedgerV2.outputs` is written empty.~~ Closed. Every managed output now
  records the exact bytes jails wrote, so §R5.3's three-way rule has the base
  it needs: only-the-generator-moved replaces, only-the-reader-moved keeps
  their bytes and holds the base still, and both-moved-to-the-same-bytes
  advances the base with no write. Both sides moving *differently* refuses,
  naming §R5.4 — the committed conflict protocol is not wired to these routes.

  `RendererStamp` is honest about two fields rather than plausible. `template`
  is `None`: §R5.2 allows `Some` only when template bytes contributed and
  carries exactly one `TemplateStamp`, while a recipe here renders one output
  from several built-in templates, whose bytes are pinned by `jails_version`
  because they are `include_str!`d. `relevant_inputs` is the canonical empty
  set: §R5.2 says a renderer records what it consumed *through `SnapshotView`*
  and these recipes read the project directly, so hashing the request's whole
  read set would make every unrelated edit appear to explain a change — the
  exact failure the field exists to prevent. Both become answerable when
  §R6.3's `template::{install,resolve}` row lands.

  An output row whose images this transition did not move keeps every field,
  the stamp included. Restamping a file it did not write would make the store
  differ from itself on a repeat run, and "already set up" would stop being
  reachable.

- ~~`add db` and anything else contributing a Spring test import cannot yet be
  stated as desired state.~~ Closed. §R6.3's `add::test_wiring` row landed as
  `ResourceKey`/`ResourceValue`/`SemanticEdit::SpringTestImport`: one claim per
  `@SpringBootTest` the capability edits, keyed by that file, so a test written
  later is not silently covered by a claim about a file it is not in. The
  target list is read while planning and every path is declared, which turns
  the read into a precondition the executor rechecks under the lock. `add db`'s
  `spring.datasource.*` block became ordinary plan properties in the same
  change, so the V2 install produces a project that starts rather than one that
  merely compiles.


Schema 2 and command-by-command production switching cannot coexist: once one
command writes schema 2, an unswitched schema-1 writer cannot safely read or
update it, and dual write would create two authorities. Therefore migration is
incremental in code and tests but **atomic at the production dispatch point**.

1. Land executor/journal/object/ledger code dark. Keep R1-R3 shadow comparisons
   and run R4 fault tests without routing default production commands. A
   test-only engine selector may exercise V2; production builds contain no
   environment flag that silently changes state format.
2. Implement capability `add/remove/sync` on V2, including format/runtime
   effects, while default dispatch remains V1.
3. Implement persistent `generate/destroy`, then `field`, `migration`, `cases`
   one-shot policies on V2. Remove derived destroy fallback in the V2 path only
   after legacy fixtures prove exact safe refusal/adoption.
4. Implement `app init/plan/apply/reconcile` as one aggregate V2 desired graph.
   The V2 path has no per-intent ledger save or scratch regeneration.
5. Implement maintenance mutations (`rename`, both adopt modes, fast-test
   dependency and standalone `fmt`) and pure state translation on V2.
6. Route new-project bootstrap through prepare-in-scratch/publish, then route
   `new --app` inside that unpublished project.
7. Install a read-only `StateCompatibility` facade used by every command to
   parse schema 1, schema 2 and legacy inputs without mutation. Its schema-1
   translation produces `LegacyEntry` where information is incomplete. There
   is still no schema-2 write from default dispatch.
8. Classify every remaining mutating subprocess/path as transaction input,
   post-commit external effect, derived cache or explicit out-of-project action.
9. After the entire command matrix passes, flip the single top-level mutation
   dispatch to V2. The first mutating command on schema 1 includes lossless
   translation and source cleanup in its R4 journal. From that commit onward no
   V1 writer is reachable; a startup architecture assertion/test refuses a
   mixed dispatch table.
10. Remove V1 adapters/types only after cutover parity, crash and compatibility
    suites pass; then run the four proof apps and hosted CI.

#### R6.2 Managed lifecycle matrix

| Surface/current symbols | Target route and required deletion | Switch gate |
|---|---|---|
| `add::preflight[_in]`, `build_plan`, `add[_in]` | recipe metadata → desired capability/resource change → one prepared commit; guarded `ComposeReconcile` deferred. Delete the second imperative interpretation and direct config/pom/file writes. | every capability default and parameter class: preview/apply/no-op/remove, shared resources and fault sweep |
| `add::shrink::remove`, `sync` | owner-scope desired absence/presence; no mirrored hand-written undo. Confirmation occurs after describe but before commit; commit rechecks staleness. | removal preserves another owner/user bytes, sync order invariant, force receipt for drift |
| `generate::artifacts_for`, `generate::write`, command registration | persistent `IntentSpec` → `DesiredChange`; same direct-owner semantics as an equivalent manifest row. Delete `write_new_file` side effects and per-file ledger recording. | golden parity for every persistent `ArtifactKind`, reverse refs, no-op, update and conflict |
| `generate::remove::destroy` | remove `DirectCli` claim and forward-plan remaining resources from recorded exact state. Delete recomputed/mirrored path tables and direct `remove_file`; legacy ambiguity refuses/adopts. | every kind, changed layout/version, shared owner, edited/missing file, `--force`, crash sweep |
| `generate_field` | active one-shot field overlay with a durable target-coupled/append-only resource partition, derivative refresh and forward migration in one prepared commit. Later target renders reapply active overlays; target removal retires them without deleting append-only history; an identical explicit field command may reactivate without a second migration. | repeat no-op, same-name/different-spec conflict, target update preserves field, target remove/recreate remains retired, explicit reactivation, shared target resource, edited derivative, crash at each op |
| `generate_migration` | snapshot allocates next number; lock rechecks directory listing; append-only file/receipt, no destroy. | two concurrent allocations produce distinct serial commits or one stale retry; never overwrite |
| `generate_cases` / `destroy cases` | one-shot source-hash receipt; same-source updates reconcile the immutable output path, and destroy selects by existing source or stable `--receipt` ID when the source is gone. Delete only its exact recorded output. | source update, output-path-change refusal, edited output conflict, missing-source receipt destroy, malformed/wrong-kind receipt ID |
| `run::ensure_console_launcher`, `test --fast`, `remove fast-test` | persistent `ToolFeature::FastTest` with a `DirectCli` owner; add/remove its console dependency through the shared POM resource planner and delete the imperative ensure helper. The test subprocess starts only after a successful/no-op feature transition. | first add, repeat no-op, explicit clean/drifted removal, retained-dependant refusal, `--force`, pretend and crash sweep |
| `app::init` | explicit create-only write of a human manifest through a prepared operation; no ledger desire synthesis. Init targets must resolve to `ProjectPath` inside the project. Existing files always refuse—there is no implicit replace/force mode. An external `--manifest` remains valid for plan/apply input but is refused for init with instructions to copy it explicitly. | existing-path refusal, absent/internal custom manifest, external-target refusal, pretend purity |
| `app::plan` | recovery-status read → load/resolve/`PlannedTransition`; prepare/describe a `CommitPlan`, or describe an `EffectRetryPlan` directly. No lock, mutating recovery, effect execution or migration. An incomplete transaction is reported and blocks a misleading fresh plan. | byte-for-byte root and legacy state unchanged; active/blocked journal reporting; JSON/human parity |
| `app::apply[_in]`, `project_at` loop | one aggregate projected plan and one commit. Delete project reload after each capability/intent, per-intent `state.record`, double capability pass and partial success. | reverse manifest dependency order, last-owner removal, one ledger commit, all failpoints |
| `app::reconcile_intent` | exact object-base reconciliation in R5; delete project copies and old-base re-render. | clean/user-edit/template-upgrade/conflict/finalise/abort matrix |

Direct and manifest forms share canonical identity/spec/resource planners; only
`OwnerId`/`ReconcileScope` differ. `--pretend` always follows and describes the
same planned transition, without executing its commit/effect branch. A direct
multi-capability command or app manifest has one journal, one ledger commit and
one receipt when it produces a new project transition.

#### R6.3 Format/state mutation matrix

| Current area/symbol | Required destination |
|---|---|
| `generated_files::{record,forget,record_model}` | projections over `LedgerV2`; recipes never save state. Remove model registry and path-only writer adapters. |
| `generated_files::migrate_legacy` | pure compatibility reader returning `LegacyEntry`/migration changes. It currently removes sources while still building memory state; new source deletion is a `FileOp` committed only after schema-2 `ledger_after` is durable. |
| `app::migrate_app_state` | same rule for `.jails/app-state-v1`; it currently removes the source before the later ledger save. Preserve unknown/external-manifest ownership as `LegacyEntry` until explicit adoption. |
| `ledger::{load,save,entry_mut}` | strict snapshot parse, canonical ledger draft and R4 ledger-last executor. No caller gets a mutable ledger or saves it independently. |
| `config::{record_capability,forget_capability,record_layout}` | `SemanticEdit::{HumanConfigCapability,HumanConfigLayout}` with before bytes and surgical complete after bytes; human comments/order outside the edit survive. `HumanConfigLayout.directory` is a validated single relative component, not a Java `Name`. |
| `pom::*`, `compose::{write,add_service,remove_service}`, property installers | pure format owners from resource maps; all create/replace/delete flows lower to one `FileOp`. `compose::up/stop` stay external effects. |
| `codemod::Marked`, command/Spring registration, `add::test_wiring` | keyed semantic contributions with explicit owners; install/uninstall render complete shared source/config bytes before commit. |
| `template::{install,resolve}` | frozen snapshot `ResolvedTemplate` plus R5 stamp/context; remove global rediscovery during a plan. |
| formatter hooks `run::fmt[_quietly]` | R3 scratch formatter, exact resulting operations. Standalone `jails fmt` becomes a transaction over its declared source scopes. |

Any schema-1 compatibility deletion and schema-2 ledger creation/replacement
are in the same journal. A read-only command never calls a migration helper.

#### R6.4 Other project mutations

The architecture gate currently bans only literal `fs::write`; it must expand
to `write`, `OpenOptions` write modes, `remove_file/remove_dir[_all]`,
`copy`, `rename`, hard links, directory creation, permissions and mutating
subprocesses. Production occurrences in `app`/`app::reconcile`, `new`,
`rename`, `compose`, `generated_files`, `generate::remove`, `add::shrink`,
`add::database`, `add::test_wiring`, `testd` and `console` must land in one of
these explicit rows:

| Surface | Decision |
|---|---|
| `rename::plan`/`rename::rename` | snapshot all Java inputs. For managed entities/dependants, re-render under the renamed typed identity and reconcile `B/L/N`; update `OutputRecord.base/current/RendererStamp` truthfully. A path move becomes guarded target `Create` plus old `Delete` in one journal (clean or conflict result preserves user edits through the same three-way rules). Unmanaged codemod edits are unowned maintenance `Replace`s. Collision/literal warning and confirmation occur before commit. Delete live `fs::rename`. |
| `adopt::report` and R1 legacy adoption | surgical config/ledger desired change through R4; extend existing `adopt` options as specified, with pretend using the same prepared value. |
| `run::ensure_console_launcher` (`test --fast`) | model `org.junit.platform:junit-platform-console` as resource owned by internal `ToolFeature::FastTest`; commit its guarded POM edit first, release lock, then run tests. Repeated fast test is no-op. `remove fast-test [--force]` removes that owner through the same stored-base route before deleting this imperative helper. |
| `run::fmt` | scratch-format and commit exact changed sources. Do not let Maven/Spotless mutate the live source tree directly. |
| `compose::write` including empty-file deletion | pure complete compose bytes/absence into file ops; never direct delete. Runtime start/stop is external below. |
| database/test-wiring legacy cleanup | every factories/properties/config/source deletion or directory creation is a resource-derived file/directory op; no cleanup helper mutates during planning. |

`jails adopt` keeps its current no-argument layout behaviour. Legacy ownership
adoption is the same command with a closed option group:

```text
jails adopt
jails adopt --legacy-key <kind:sha256> --manifest <path> --intent <kind:name[:package]>
jails adopt --legacy-key <kind:sha256> --manifest <path> --intent <kind:name[:package]> --replace --force
```

`--legacy-key`, `--manifest` and `--intent` form one all-or-none legacy option
group. `--replace` is valid only in that mode and requires the local `--force`;
there is no invented global `--yes`. Bare `--force`, `--replace` without all
three selectors, a partial selector group, or layout flags mixed with legacy
selectors refuse in Clap validation. `jails doctor` prints the stable key and
copyable skeleton; users never guess it. The global `--pretend`
works for both modes and describes the same prepared transition. Plain legacy
adoption succeeds only when every current byte and mode equals the freshly
rendered candidate, then records that candidate and its real renderer stamp as
base/current. `--replace --force` guards and receipts mismatching old bytes
before installing the freshly rendered candidate. There is no
`LegacyAdopted` renderer fiction or provenance-less `OutputRecord` variant.

Activate the already-defined `EntityId::ToolFeature(ToolFeature)` with the sole initial value `FastTest`
so the persistent console dependency has explicit ownership. `test --fast`
constructs request-owned desired state in
`ReconcileScope::DirectEntity(FastTest)` with `OwnerId::DirectCli`; only a
successful ledger commit makes it observed state. Unrelated scopes carry that
observation forward without relabelling it human desire. A later ordinary test
does not remove it. The closed removal route is
`jails remove fast-test [--force]`, encoded as
`CanonicalMutationRequest::RemoveToolFeature`; it removes the `DirectCli`
owner and reconciles the shared POM only when no typed dependant remains.
`--force` has exactly the local drift semantics of other managed-output
removal and is never an ownership cascade.

#### R6.5 New-project publication

`new::new`, `new_offline`, `finish_spring_project`, `new_cli` and
`seed_manifest` create a project that has no lock/root yet. Under an advisory
`<parent>/.jails-new.lock`, recheck the requested destination is absent, reserve
a `tempfile::TempDir` sibling on the same filesystem, and perform downloads,
unpack, pom/source/config generation, manifest seeding and optional app apply
inside it. The internal project uses the normal pipeline and must be green
before publication. Remove downloaded archive debris, fsync all created files
and directories, then atomically rename the completed scratch root to the
destination and fsync the parent. Destination is therefore absent or complete.

Network/Initializr failure, invalid archive path/symlink, generator refusal or
build failure closes scratch and leaves destination absent. An existing
destination is never merged. Online `new --pretend` may download/unpack into an
owned system temporary directory so it can report exact bytes, but it acquires
no parent publication lock, creates nothing at/under the requested destination,
runs no `git init` or generated build, and explicitly closes the temp tree.
Offline pretend uses vendored inputs. A request-summary-only preview is not
allowed to masquerade as exact parity. `git init` runs inside
scratch after generated files are complete and before publication; its `.git`
content is bootstrap-owned but excluded from later project transactions.
`new --app` does not publish until app apply and its generated build pass.

Keep the parent lock file, as with project locks, to avoid inode replacement.
Its scope is only publication in that parent; it is not a second project ledger.

#### R6.6 Explicitly outside the project transaction

These actions do not become ledger file ops. Their ordering/classification is
fixed so “one writer” is not overclaimed:

| Action | Classification and ordering |
|---|---|
| Maven/mvnd/wrapper build, raw `jails mvn`, console classpath resolution | derived build process may write `target/` and dependency caches. Run without project lock only after any required prepared project commit. `target/` is excluded from snapshot/ledger and may be cleaned by Maven. |
| `testd` cache, daemon source and Unix socket | tool cache/runtime coordination outside managed source and ledger; never hold project mutation lock while daemon/tests run. Pom/source hashes decide staleness. |
| compose/container start/stop, PostgreSQL/SQLite/Kafka clients | external runtime effects. A capability/app commit records desired project files first; automatic runtime reconciliation is an idempotent receipt effect. Every explicit compose/container-mutating `start` or `stop` command briefly acquires the project lock, performs structural recovery/recheck, non-blockingly acquires the validated `effects.lock`, then releases the project lock while retaining the effect lock for the subprocess; contention maps to `EffectBusy`. It uses the same frozen-document/no-implicit-discovery process contract and releases the effect lock afterward. Read-only `db`/`console` clients remain outside both locks and none of these commands claims filesystem rollback. |
| machine `setup` files | explicit out-of-project writer with its own exact-path confirmation/atomic replace; never reached from project apply and never journaled under a project. |
| k6 run and `bench --export` | runtime process; user-selected export is an explicit command output, guarded against overwrite according to its CLI contract, not a managed generated file. Run after reading a stable project snapshot and without mutation lock. |
| watcher, raw Java/jshell, installed tool caches | processes/derived caches; no ledger ownership. They may observe a receipt but cannot join its commit. |

The audit must leave no unclassified production mutation. Test-fixture scratch
uses Immediate's guard and is excluded by an architecture scanner that blanks
`#[cfg(test)]` code rather than weakening production rules.

#### R6.7 Compatibility and downgrade contract

- Preserve command names, aliases, manifest default, generated paths and
  golden bytes unless a named ownership/correctness change in this plan
  requires otherwise. `strategy_on/yields` continue to parse through R6 but
  canonical output uses `on/yields`.
- Deliberate changes are: manifest absence relinquishes that owner; direct
  parameter misuse refuses rather than being ignored; conflict apply commits
  markers/pending state and exits 2; `--pretend` can run preparation tools in
  scratch but never writes; `app init --manifest` now requires an in-project
  target even though plan/apply may read an external manifest; and schema-2
  legacy ambiguity requires adoption.
- `--no-start`, `--force`, `--package`, `--name`, custom `--manifest` and global
  `--pretend` retain meaning. Mutation commands and `app plan` gain
  `--output human|json`; existing read-only command-local `--json` keeps its
  current domain payload and is not rewrapped in `CommandEnvelope`.
  Destructive confirmation uses the exact prepared
  report and occurs before lock; a changed tree makes commit stale and requires
  confirmation of the new report.
- Schema 2 begins with `schema = 2`; new code refuses a newer schema. Migration
  is one-way and atomic, with old bytes/preimages in the retained receipt. No
  dual write or automatic downgrade exists.
- Running a pre-schema-2 binary after migration is unsupported. The baseline
  binary's closed ledger parser must refuse the unknown top-level schema before
  every ledger-aware mutation in compatibility tests; documentation warns that
  still older binaries which did not consult state before mutation are unsafe.
  There is no generic `rollback <transaction>` command in this roadmap and a
  migration receipt is not a downgrade format. To return to an old binary,
  restore the whole project **and** `.jails` from one VCS/backup snapshot made
  before migration. Never emit a lossy schema-1 projection or claim that file
  preimages alone recreate an older state schema.

Commit immutable fixtures for: no `.jails`; each legacy `.jails/files`,
`intents`, `models`, `version` combination; app-state schema 1/2; schema-1
ledger with present/absent/unknown zero-argument spec; custom external manifest
whose origin is unknowable; parameterised capabilities; custom package/layout;
edited/missing outputs; stored conflicts; corrupt/unreadable/newer state; and
Unix/Windows-normalised paths. For every fixture assert plan purity, migration
after commit only, second-run byte stability, exact preservation/refusal and
recovery at the ledger commit window.

#### R6.8 Product proof and final deletion

From empty directories rebuild `examples/web-crawler`, `support-inbox`,
`payments-gateway` and `ledger-cli` using only recorded jails commands and zero
hand-written Java/SQL. For each capture:

1. initial plan/pretend purity and exact apply receipt;
2. capable-host generated build/tests with required sockets, JVM attachment and
   containers;
3. second apply/no-op and manifest-order invariance;
4. one shared-owner removal and one last-owner removal;
5. clean drift merge, committed conflict, partial-resolution refusal, complete
   rerun finalisation and guarded abort on a fresh conflict;
6. crash recovery at a mid-file and post-ledger failpoint;
7. direct destroy/one-shot policies and full final build.

Generate the least-privilege CI workflow into a disposable real hosted
repository, push the generated application, observe the workflow itself pass
with immutable actions and intended permissions, and link repository, commit
and run in `examples/DOGFOOD.md`. Local YAML inspection is not this gate.

Only then delete: app-state/per-intent legacy writers; mirrored remove/destroy;
old reconcile scratch regeneration; sequential add/app apply; independent
pretend branches; mutable `Ledger` access; root-taking planner facade; direct
project mutation helpers outside the executor; and temporary `Change` alias.
Update `README.md`, `CLAUDE.md`, `abstract.md`, `examples/ACCEPTANCE.md` and
`DOGFOOD.md` from the same evidence.

R6 gate:

- `cargo fmt --check`, all-target compile, ordinary unit/integration,
  architecture, agreement, genericity, golden, editor, CLI, migration,
  transaction and object suites pass twice under normal temp; the historical
  “9/9” architecture count is not an acceptance criterion;
- `JAILS_REQUIRE_TOOLCHAIN=1 cargo test` passes with zero skips on a capable
  host, including generated Maven/PostgreSQL/Kafka/socket/Mockito tests;
- `tests/architecture.rs` carries the mutation inventory, the executor-ownership
  ratchet, the subprocess classification and `v2_dispatch_is_all_or_nothing`.
  The first three exist under the names the board prints them by -- the
  `filesystem mutation sites outside the write layer` ratchet (at zero) and
  `every_module_that_starts_a_process_is_classified`, which fails on a stale
  row as well as an unclassified module. The fourth arrives with step 9, and
  until it does there is nothing for it to assert: no production dispatch
  reaches V2. The inventory recognises `fs`/`File` writes,
  `OpenOptions` create/write/append/truncate, remove/copy/rename/link/symlink,
  directory and permission mutation, plus process spawn/status/output sites.
  It scans production syntax with resolved aliases where the lightweight
  scanner supports them, requires each process site to name a §R6.6 class, and
  has **zero unclassified production sites**; an allowlist entry names exact
  module, symbol, API and classification, never a wildcard or count ceiling;
- all command-matrix parity, compatibility fixtures and named crash failpoints
  pass on the supported Unix filesystems (Linux CI first); Windows remains a
  documented unsupported port until R4's backend contract is implemented;
- all four proof contracts pass, generated/manual Java and SQL remain
  100%/0%, hosted CI is observed, and all lifecycle clauses are closed; and
- a conceptual grep/architecture test finds one
  `ResolvedMutation`→`PlannedTransition` route, one
  `CommitPlan`→`PreparedChange`→journal route, one bounded
  `EffectRetryPlan`→`resume_effect` route, one strict ledger and no second
  authoritative sequence.

## 5. Deferred, rejected and triggered work

- **Deferred measurement:** count Spring context-cache misses across proof apps,
  then optimise only if the count explains the gate. Run App C's k6 profile and
  record p95/p99 before making a performance claim.
- **Deferred maintenance:** consolidate the Java/SQL built-in type maps into
  one data table after R6. It is a one-edit improvement, not a lifecycle blocker.
- **Rejected for now:** one descriptor file per kind. Golden coverage, derived
  destroy paths and `commands --json` already bought its main value; a `build.rs`
  and file format need a new consumer before they earn their cost.
- **Deliberately not built:** `jails dev`; measurement showed jdt.ls and
  devtools already own that loop. Editor/dotfiles work belongs in the dotfiles
  repository.
- **Triggered, not queued:** flags, shedlock, storage, architecture tests,
  nullability checks or another capability begin only when a proof application
  contributes a concrete acceptance clause.
- **Anti-goals:** domain-specific generators, executable plugin hooks, a
  conditional template DSL, an ORM/runtime support jar, silent Gradle support,
  an embedded LLM/MCP server, incremental `check`, or treating a skipped test
  as coverage.

## 6. Update protocol

- Update the baseline and status in the same commit that changes them.
- `QUEUED → IN FLIGHT` requires tracked implementation. `IN FLIGHT → SHIPPED`
  requires a commit and every named gate. Never promote from a dirty tree.
- Replace snapshots; do not append push reviews, crossed-out rows or relative
  language such as “this morning”. Remove shipped work instead of preserving
  it here.
- Keep volatile counts in tests, timings in `DOGFOOD.md`, shipped command
  documentation in `README.md`, and architectural reasoning in `abstract.md`.
- A failed or environment-constrained gate is recorded as such. “Not run” and
  “skipped” never mean passed.

## 7. Legacy section locator

Other files still cite the former numbered document. Until those references
are made self-contained, interpret them as follows:

| Former reference | Current authority |
|---|---|
| §0, §4, §18 | §1 goal and R6 proof gates |
| §2, §10, §19 | §5 deferred measurements |
| §5, §9, §12–§15 | `README.md`; only triggered residue remains in §5 here |
| §6, §21 | `abstract.md` plus R1–R5 |
| §7 | §5 rejected/deferred decisions |
| §11 | R1 ownership, R3 preparation, R4 commit, R5 reconciliation, R6 migration |
| §16 | §5 anti-goals |
| §17 | R1–R6, the sole current sequence |
| §1, §3, §8, §20 or finer historical claims | Git history; do not infer pending work |
